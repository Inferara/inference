//! Driver-side orchestration of external `.wasm` resolution and validation.
//!
//! Between type checking and linking, the build driver must turn each bound
//! `external fn` into actual `.wasm` bytes the static-merge linker can consume.
//! This module performs that, end to end, for every extern in a program:
//!
//! 1. enumerate the program's bound externs ([`TypedContext::extern_origins`]),
//! 2. [resolve](super::resolve) each logical module to a concrete `.wasm` path,
//! 3. [validate](super::validate) that the resolved module exports the named
//!    function with the declared signature, and
//! 4. read the deduplicated module bytes for the linker.
//!
//! Signature validation needs the `external fn`'s declared parameter and return
//! types, which live in the AST. The arena is reachable from the
//! [`TypedContext`], so this stays a pure post-type-check step with no extra
//! plumbing through the front end.

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;

use inference_ast::arena::AstArena;
use inference_ast::ids::{DefId, NodeId};
use inference_ast::nodes::Def;
use inference_type_checker::ExternOrigin;
use inference_type_checker::typed_context::TypedContext;
use inference_wasm_linker::ImportWriteSet;

use super::resolve::{resolve_wasm_module, ManifestDeps, ModulePath, SearchPath};
use super::validate::{lower_extern_signature, validate_extern, LoweredExtern};

/// Maximum size, in bytes, of a resolved external `.wasm` module.
///
/// External modules are read fully into memory before validation, so an
/// unbounded read of an attacker-influenced (sparse, multi-GB) file in a search
/// location would drive the compiler toward OOM. A real `.wasm` library is well
/// under this bound; the cap exists solely to defeat that resource cliff.
pub const MAX_EXTERNAL_MODULE_BYTES: u64 = 64 * 1024 * 1024;

/// A resolved external module: its logical name, the file it resolved to, and
/// the bytes read from disk.
#[derive(Debug, Clone)]
pub struct ResolvedExternalModule {
    /// Logical `::`-joined module reference, for diagnostics.
    pub logical_module: String,
    /// The `.wasm` file the logical module resolved to.
    pub path: PathBuf,
    /// The module's bytes, ready for the linker.
    pub bytes: Vec<u8>,
}

/// Everything a program's `external fn` declarations resolve to: the modules to
/// merge, and the write-set contracts the merge is checked against.
///
/// The two travel together because they are two halves of one answer. The bytes
/// alone let a caller link without a check; the contracts alone describe imports
/// nothing satisfies. Handing them over as one value keeps a caller from
/// reaching the linker with the first and not the second.
#[derive(Debug, Clone, Default)]
pub struct ResolvedExternals {
    /// One entry per distinct logical module the program binds, sorted by name.
    pub modules: Vec<ResolvedExternalModule>,
    /// One entry per distinct `(module, field)` the program binds, sorted by
    /// that pair. Every satisfied import of the codegen output has an entry, so
    /// the checked link mode holds all of them to a declaration.
    pub contracts: Vec<ImportWriteSet>,
}

impl ResolvedExternals {
    /// The `(logical_module, bytes)` pairs the linker takes, borrowed from
    /// [`ResolvedExternals::modules`].
    #[must_use]
    pub fn module_bytes(&self) -> Vec<(&str, &[u8])> {
        self.modules
            .iter()
            .map(|m| (m.logical_module.as_str(), m.bytes.as_slice()))
            .collect()
    }
}

/// Why the driver could not assemble the external-module set.
#[derive(Debug)]
pub enum ExternalResolutionError {
    /// A logical module could not be resolved to a `.wasm` file.
    Resolve(super::resolve::ResolveError),
    /// A resolved module failed export/signature validation. The error is boxed
    /// because [`super::validate::ValidateError`] carries a signature mismatch
    /// payload large enough to dominate the enum's size.
    Validate {
        logical_module: String,
        error: Box<super::validate::ValidateError>,
    },
    /// A resolved module failed full WASM validation (its bytes do not decode to
    /// a structurally and semantically valid module). A malformed-but-decodable
    /// external must be rejected here, before it can reach the linker.
    Invalid {
        logical_module: String,
        path: PathBuf,
        reason: String,
    },
    /// A resolved module is well-formed WebAssembly but uses a feature outside the
    /// linker's supported WASM 1.0 subset (see
    /// [`inference_wasm_linker::SUPPORTED_WASM_FEATURES`]). Rejecting it here — the
    /// same gate the linker applies — surfaces the feature-named diagnostic at the
    /// earliest point in the build, keeping the supported-version contract a single
    /// source of truth.
    UnsupportedFeature {
        logical_module: String,
        path: PathBuf,
        reason: String,
    },
    /// A resolved `.wasm` file exceeded [`MAX_EXTERNAL_MODULE_BYTES`].
    TooLarge {
        path: PathBuf,
        size: u64,
        limit: u64,
    },
    /// An `external fn`'s declared signature could not be lowered to WASM value
    /// types (e.g. a `unit` parameter or an unsupported type form).
    Signature {
        export_field: String,
        error: super::validate::LowerSignatureError,
    },
    /// A logical name was not a valid module path (empty or separator-bearing
    /// segment).
    ModulePath(super::resolve::ModulePathError),
    /// The resolved `.wasm` file could not be read.
    Read {
        path: PathBuf,
        error: std::io::Error,
    },
    /// A bound extern named a function the AST has no `external fn` declaration
    /// for — an internal inconsistency between provenance and the parsed tree.
    MissingDeclaration { export_field: String },
    /// Two bound declarations of one `(module, field)` declare different write
    /// sets: one marks a parameter `mut` and the other does not.
    ///
    /// Both declarations are real and both are linked against the same merged
    /// body, which is checked once. Accepting either reading would compile one
    /// of the two files against a contract it never made, so the program is
    /// rejected until the declarations agree.
    ConflictingWriteSet {
        logical_module: String,
        export_field: String,
        /// The two files, already rendered for the message.
        first_file: String,
        second_file: String,
    },
}

impl std::fmt::Display for ExternalResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExternalResolutionError::Resolve(e) => write!(f, "{e}"),
            ExternalResolutionError::Validate {
                logical_module,
                error,
            } => write!(f, "module `{logical_module}`: {error}"),
            ExternalResolutionError::Invalid {
                logical_module,
                path,
                reason,
            } => write!(
                f,
                "module `{logical_module}` at `{}` is not a valid WASM module: {reason}",
                path.display()
            ),
            ExternalResolutionError::UnsupportedFeature {
                logical_module,
                path,
                reason,
            } => write!(
                f,
                "module `{logical_module}` at `{}` uses an unsupported WebAssembly feature: {reason}",
                path.display()
            ),
            ExternalResolutionError::TooLarge { path, size, limit } => write!(
                f,
                "external `.wasm` `{}` is {size} bytes, exceeding the {limit}-byte limit",
                path.display()
            ),
            ExternalResolutionError::Signature {
                export_field,
                error,
            } => write!(f, "external fn `{export_field}`: {error}"),
            ExternalResolutionError::ModulePath(e) => write!(f, "{e}"),
            ExternalResolutionError::Read { path, error } => {
                write!(f, "failed to read `{}`: {error}", path.display())
            }
            ExternalResolutionError::MissingDeclaration { export_field } => write!(
                f,
                "internal error: extern `{export_field}` is bound but has no declaration"
            ),
            ExternalResolutionError::ConflictingWriteSet {
                logical_module,
                export_field,
                first_file,
                second_file,
            } => write!(
                f,
                "conflicting write sets for external function `{export_field}` of module \
                 `{logical_module}`: {first_file} and {second_file} declare it with different \
                 `mut` parameters. Both declarations bind the same imported function, which the \
                 linker checks once against the merged body, so the two must agree on which \
                 parameters that body may write through; mark the same parameters `mut` in both \
                 declarations"
            ),
        }
    }
}

impl std::error::Error for ExternalResolutionError {}

/// Resolves, validates, and reads every external `.wasm` module a program binds.
///
/// Returns the resolved modules together with the write-set contracts their
/// declarations state, as one [`ResolvedExternals`].
///
/// One resolved module per distinct **logical module** the program
/// binds. Two externs from the same logical module yield a single entry, and a
/// physical `.wasm` file is read and validated once even if two logical modules
/// resolve to it — but each logical module still gets its own entry, because the
/// linker matches an import's recorded `(module, field)` on the logical module.
/// The order is deterministic (sorted by logical module name).
///
/// A program with no externs yields an empty vector, and the build proceeds
/// without invoking the linker.
///
/// Every bound *declaration* is signature-validated, including two declarations
/// in different files that name the same module and field. The linker satisfies
/// an import on `(module, field)` alone and compares no signatures, so a
/// declaration whose signature never reached validation here would be linked
/// against a library it does not match.
///
/// Every bound declaration also contributes its **write set** — the parameters
/// it marks `mut` — keyed on the `(module, field)` pair the linker satisfies an
/// import on. Two declarations of one pair that disagree on that set are
/// rejected rather than reconciled; see [`record_write_set`].
///
/// # Errors
///
/// Returns an [`ExternalResolutionError`] if any extern fails to resolve,
/// validate, lower its signature, or read its bytes, or if two declarations of
/// one `(module, field)` declare different write sets.
pub fn resolve_external_modules(
    typed_context: &TypedContext,
    search_path: &SearchPath,
    manifest_deps: Option<&ManifestDeps>,
) -> Result<ResolvedExternals, ExternalResolutionError> {
    let origins = typed_context.extern_origins();
    if origins.is_empty() {
        return Ok(ResolvedExternals::default());
    }

    let arena = typed_context.arena();

    // Cache reads/validations by resolved path so a physical file is read once,
    // even when two logical modules resolve to it.
    let mut read_cache: BTreeMap<PathBuf, Vec<u8>> = BTreeMap::new();
    // Output keyed by logical module: the linker matches each import's recorded
    // `(module, field)` on the logical module, so every bound logical module
    // needs its own entry even if it shares bytes with another. `BTreeMap` keeps
    // the output deterministic.
    let mut by_module: BTreeMap<String, ResolvedExternalModule> = BTreeMap::new();
    // The write-set contracts, keyed on the `(module, field)` pair an import is
    // satisfied on — not on the logical module the modules above are keyed on,
    // because one library may back several imports with different write sets.
    let mut contracts: BTreeMap<(String, String), DeclaredWriteSet> = BTreeMap::new();

    for origin in &origins {
        let module_path = parse_module_path(&origin.logical_module)?;
        let resolved = resolve_wasm_module(&module_path, search_path, manifest_deps)
            .map_err(ExternalResolutionError::Resolve)?;

        // For a path seen before, the bytes are already read AND validated; reuse
        // them. A fresh path is size-checked, read with a bounded streaming read,
        // and validated as a real WASM module before any byte reaches the linker.
        let bytes = if let Some(existing) = read_cache.get(&resolved) {
            existing.clone()
        } else {
            let bytes = read_external_module(&resolved)?;
            validate_module_bytes(&bytes, &origin.logical_module, &resolved)?;
            read_cache.insert(resolved.clone(), bytes.clone());
            bytes
        };

        // Recover the declared signature from the *exact* declaration this
        // binding attaches to, by `DefId`. Two same-named externs (e.g. a
        // top-level and a spec-inner `sort`) must not collide into one slot:
        // validating the resolved library against a same-named sibling's
        // signature would either reject a matching library or accept a
        // mismatching one. Only the bound declaration is the source of truth.
        let (args, returns) = extern_declaration(arena, origin.decl).ok_or_else(|| {
            ExternalResolutionError::MissingDeclaration {
                export_field: origin.export_field.clone(),
            }
        })?;
        let lowered = lower_extern_signature(arena, &args, returns).map_err(|error| {
            ExternalResolutionError::Signature {
                export_field: origin.export_field.clone(),
                error,
            }
        })?;

        validate_extern(&bytes, &origin.export_field, &lowered.signature).map_err(|error| {
            ExternalResolutionError::Validate {
                logical_module: origin.logical_module.clone(),
                error: Box::new(error),
            }
        })?;

        record_write_set(arena, origin, &lowered, &mut contracts)?;

        by_module
            .entry(origin.logical_module.clone())
            .or_insert(ResolvedExternalModule {
                logical_module: origin.logical_module.clone(),
                path: resolved,
                bytes,
            });
    }

    Ok(ResolvedExternals {
        modules: by_module.into_values().collect(),
        contracts: contracts.into_values().map(|d| d.write_set).collect(),
    })
}

/// Folds one declaration's write set into the per-`(module, field)` contract
/// map, rejecting a disagreement between two declarations of the same import.
///
/// Two files may each declare and bind the same `(module, field)`: both
/// declarations survive resolution, while codegen folds them onto a **single**
/// WASM import. The linker then performs one write-set check, and codegen
/// consults each declaration separately — so if one file says `mut p` and the
/// other does not, the non-`mut` file's calls are compiled against a contract
/// only the `mut` file satisfied.
///
/// Neither reconciliation is available. A union licenses the non-`mut` file's
/// calls to a body that writes; an intersection refuses the `mut` file's
/// legitimate link; taking the first match is the miscompile itself. So the
/// disagreement is a hard error naming both files, and the fix is for the two
/// declarations to agree.
///
/// `mut` has no counterpart in a found WASM signature, so this cannot ride along
/// on the existing signature comparison: two declarations differing only in
/// `mut` lower to the identical [`DeclaredSignature`] and both validate.
fn record_write_set(
    arena: &AstArena,
    origin: &ExternOrigin,
    lowered: &LoweredExtern,
    contracts: &mut BTreeMap<(String, String), DeclaredWriteSet>,
) -> Result<(), ExternalResolutionError> {
    let key = (origin.logical_module.clone(), origin.export_field.clone());
    match contracts.entry(key) {
        Entry::Vacant(slot) => {
            slot.insert(DeclaredWriteSet {
                decl: origin.decl,
                write_set: ImportWriteSet {
                    module: origin.logical_module.clone(),
                    field: origin.export_field.clone(),
                    mut_params: lowered.mut_params.clone(),
                    param_names: lowered.param_names.clone(),
                },
            });
            Ok(())
        }
        Entry::Occupied(slot) if slot.get().write_set.mut_params == lowered.mut_params => Ok(()),
        Entry::Occupied(slot) => Err(ExternalResolutionError::ConflictingWriteSet {
            logical_module: origin.logical_module.clone(),
            export_field: origin.export_field.clone(),
            first_file: declaring_file(arena, slot.get().decl),
            second_file: declaring_file(arena, origin.decl),
        }),
    }
}

/// One `(module, field)`'s declared write set, tagged with the declaration it
/// came from so a later disagreement can name both files.
struct DeclaredWriteSet {
    decl: DefId,
    write_set: ImportWriteSet,
}

/// How a diagnostic names the file a declaration lives in.
///
/// The entry file has no module path, so it is named in words rather than
/// rendered as an empty label — a message quoting nothing at all would leave the
/// reader with one of the two files unidentified, which is the whole point of
/// the diagnostic. A declaration whose file cannot be recovered falls back to
/// the same wording, which is honest: nothing better is known about it.
fn declaring_file(arena: &AstArena, decl: DefId) -> String {
    match arena
        .node_module_path(NodeId::Def(decl))
        .and_then(inference_ast::nodes::file_label)
    {
        Some(label) => format!("`{label}`"),
        None => "the entry file".to_string(),
    }
}

/// Reads a resolved external `.wasm` module's bytes, enforcing
/// [`MAX_EXTERNAL_MODULE_BYTES`].
///
/// The size is checked twice to defeat a TOCTOU race: once against the file
/// metadata before opening, and once against the actual bytes read. The read is
/// bounded to `limit + 1` bytes via [`Read::take`], so a file that grows past
/// the cap between the `stat` and the read still cannot force an unbounded
/// allocation — the streaming read stops one byte past the limit and the module
/// is rejected.
fn read_external_module(path: &std::path::Path) -> Result<Vec<u8>, ExternalResolutionError> {
    let limit = MAX_EXTERNAL_MODULE_BYTES;

    let metadata = std::fs::metadata(path).map_err(|error| ExternalResolutionError::Read {
        path: path.to_path_buf(),
        error,
    })?;
    if metadata.len() > limit {
        return Err(ExternalResolutionError::TooLarge {
            path: path.to_path_buf(),
            size: metadata.len(),
            limit,
        });
    }

    let file = std::fs::File::open(path).map_err(|error| ExternalResolutionError::Read {
        path: path.to_path_buf(),
        error,
    })?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| ExternalResolutionError::Read {
            path: path.to_path_buf(),
            error,
        })?;

    if bytes.len() as u64 > limit {
        return Err(ExternalResolutionError::TooLarge {
            path: path.to_path_buf(),
            size: bytes.len() as u64,
            limit,
        });
    }

    Ok(bytes)
}

/// Runs the linker's supported-version gate over a resolved external module,
/// rejecting any module that is not structurally valid WASM or that uses a
/// feature outside the supported WASM 1.0 subset.
///
/// `validate_extern` only inspects the exported function's signature; it never
/// decodes bodies, locals, or non-root sections. This gate closes that gap so a
/// malformed-but-decodable external cannot reach the linker, where it would
/// otherwise drive a recoverable error into a panic.
///
/// The check delegates to [`inference_wasm_linker::validate_external`], the same
/// two-pass gate the linker applies at `link()`, so the CLI rejects a non-1.0
/// external at the earliest point with the *same* feature-named diagnostic — a
/// single source of truth for the supported-version contract rather than two
/// divergent validations. A structural failure surfaces as
/// [`ExternalResolutionError::Invalid`]; a well-formed-but-unsupported module
/// surfaces as [`ExternalResolutionError::UnsupportedFeature`].
fn validate_module_bytes(
    bytes: &[u8],
    logical_module: &str,
    path: &std::path::Path,
) -> Result<(), ExternalResolutionError> {
    inference_wasm_linker::validate_external(logical_module, bytes).map_err(|error| match error {
        inference_wasm_linker::LinkError::UnsupportedWasmFeature { details, .. } => {
            ExternalResolutionError::UnsupportedFeature {
                logical_module: logical_module.to_string(),
                path: path.to_path_buf(),
                reason: details,
            }
        }
        other => ExternalResolutionError::Invalid {
            logical_module: logical_module.to_string(),
            path: path.to_path_buf(),
            reason: other.to_string(),
        },
    })
}

/// Splits a `::`-joined logical module string into a validated [`ModulePath`].
fn parse_module_path(logical_module: &str) -> Result<ModulePath, ExternalResolutionError> {
    ModulePath::from_segments(logical_module.split("::"))
        .map_err(ExternalResolutionError::ModulePath)
}

/// The declared argument list and return type of the `external fn` at `decl`.
///
/// Resolving by [`DefId`] (rather than by bare name) is what lets the driver
/// validate a bound extern against its *own* declaration when two same-named
/// externs exist — the top-level and a spec-inner `sort` no longer collide into
/// one signature slot.
fn extern_declaration(
    arena: &inference_ast::arena::AstArena,
    decl: inference_ast::ids::DefId,
) -> Option<(
    Vec<inference_ast::nodes::ArgData>,
    Option<inference_ast::ids::TypeId>,
)> {
    match &arena[decl].kind {
        Def::ExternFunction { args, returns, .. } => Some((args.clone(), *returns)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the driver error diagnostics. Each `ExternalResolutionError`
    //! variant renders a distinct, actionable message; these assert the rendered
    //! text so a future refactor cannot silently drop the context a build needs.

    use super::*;
    use crate::wasm_link::resolve::{ModulePathError, ResolveError};
    use crate::wasm_link::validate::{
        DeclaredSignature, LowerSignatureError, SignatureMismatch, ValidateError, WasmValType,
    };

    #[test]
    fn resolve_error_display_forwards_inner_message() {
        let inner = ResolveError::NotFound {
            logical_name: "sorting".into(),
            searched: vec![PathBuf::from("lib").join("sorting.wasm")],
        };
        let rendered = ExternalResolutionError::Resolve(inner).to_string();
        assert!(rendered.contains("sorting"), "{rendered}");
    }

    #[test]
    fn validate_error_display_names_the_module() {
        let rendered = ExternalResolutionError::Validate {
            logical_module: "crypto::sha256".into(),
            error: Box::new(ValidateError::SignatureMismatch {
                export_field: "hash".into(),
                mismatch: SignatureMismatch {
                    declared: DeclaredSignature {
                        params: vec![WasmValType::I32],
                        results: vec![WasmValType::I32],
                    },
                    found_params: vec![WasmValType::I64],
                    found_results: vec![WasmValType::I32],
                },
            }),
        }
        .to_string();
        assert!(rendered.contains("crypto::sha256"), "names the module: {rendered}");
        assert!(rendered.contains("hash"), "names the export: {rendered}");
    }

    #[test]
    fn signature_error_display_names_the_export() {
        let rendered = ExternalResolutionError::Signature {
            export_field: "f".into(),
            error: LowerSignatureError::UnitParameter,
        }
        .to_string();
        assert!(rendered.contains("external fn `f`"), "{rendered}");
        assert!(rendered.contains("unit"), "{rendered}");
    }

    #[test]
    fn module_path_error_display_forwards_inner_message() {
        let rendered =
            ExternalResolutionError::ModulePath(ModulePathError::Empty).to_string();
        assert!(!rendered.is_empty(), "empty module path renders a message");
    }

    #[test]
    fn read_error_display_shows_path() {
        let rendered = ExternalResolutionError::Read {
            path: PathBuf::from("vendor").join("arith.wasm"),
            error: std::io::Error::new(std::io::ErrorKind::NotFound, "missing"),
        }
        .to_string();
        assert!(rendered.contains("arith.wasm"), "names the path: {rendered}");
        assert!(rendered.contains("missing"), "carries the io error: {rendered}");
    }

    #[test]
    fn missing_declaration_display_is_an_internal_error() {
        let rendered = ExternalResolutionError::MissingDeclaration {
            export_field: "ghost".into(),
        }
        .to_string();
        assert!(rendered.contains("ghost"), "{rendered}");
        assert!(rendered.contains("internal error"), "{rendered}");
    }
}
