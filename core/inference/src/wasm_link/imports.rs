//! What a compiled module's import section declares, and whether it declares
//! exactly the host imports the program's `external fn` bindings did.
//!
//! Every other module here runs *before* any bytes exist. This one runs after:
//! it reads the artifact codegen produced and holds it to the declarations the
//! [driver](super::driver) collected. That pairing is the whole reason a host
//! import can ship without a link step — the linker's guarantee is that no
//! import survives it, and what replaces that guarantee is the check below,
//! which says every import that survived is one this program declared and no
//! declaration was lost on the way.
//!
//! The two sets are produced by entirely separate walks over separate data —
//! the driver's over the type checker's extern provenance, codegen's over the
//! same declarations through its own lowering — so their agreement is a real
//! check and not a restatement.

use inf_wasmparser::{CompositeInnerType, FuncType, Parser, Payload, RecGroup, TypeRef};

use super::driver::{host_import_label, HostImport};
use super::validate::{DeclaredSignature, WasmValType};

/// One entry of a module's import section.
pub(crate) struct ModuleImport {
    /// The import's module string, the first half of its two-level name.
    pub(crate) module: String,
    /// The import's field string, the second half.
    pub(crate) field: String,
    /// What is being imported.
    pub(crate) shape: ImportShape,
}

/// What an import section entry asks for.
pub(crate) enum ImportShape {
    /// A function, with the signature the module's type section gives it —
    /// `None` when that signature uses a value type no `external fn`
    /// declaration can name, which is a shape no declaration can match either.
    Function(Option<DeclaredSignature>),
    /// A memory, table, global or tag, named for a diagnostic.
    NonFunction(&'static str),
}

/// Why an artifact's imports and a program's host declarations could not be
/// reconciled, or why the two halves of a resolution could not be linked.
#[derive(Debug)]
pub enum HostImportError {
    /// The module's bytes do not decode.
    Parse(String),
    /// The module imports something that is not a function. No `external fn`
    /// declaration can describe one, so nothing in the program accounts for it.
    NonFunctionImport {
        module: String,
        field: String,
        /// What is being imported: `memory`, `table`, `global` or `tag`.
        kind: &'static str,
    },
    /// The module imports a function the program declared no host binding for.
    Undeclared { module: String, field: String },
    /// The program declared a host import the module does not carry.
    Missing { module: String, field: String },
    /// The module imports a declared pair at a different signature. `emitted` is
    /// `None` when the artifact's signature uses a value type no declaration can
    /// name.
    ///
    /// Both signatures are boxed so this variant does not set the size of every
    /// `Err` the reader below returns — the decode failure and the two
    /// name-only refusals are the common cases and carry nothing like it.
    SignatureMismatch {
        module: String,
        field: String,
        declared: Box<DeclaredSignature>,
        emitted: Option<Box<DeclaredSignature>>,
    },
    /// The module imports one two-level name twice. One declaration cannot
    /// account for two import entries, and an embedder cannot satisfy one name
    /// twice.
    Duplicated { module: String, field: String },
    /// Resolution produced both merged modules and host imports. The driver
    /// refuses that program before it resolves anything, so reaching here means
    /// the two were assembled by something other than one resolution.
    MixedResolution {
        modules: usize,
        host_imports: usize,
    },
}

/// What every arm below but [`HostImportError::MixedResolution`] adds after the
/// detail it carries.
///
/// Every arm here reaches a terminal through `infc`'s `Link step failed: {e}`,
/// and none of the six can be caused by anything an author wrote. Each is the
/// driver's walk over the type checker's extern provenance disagreeing with code
/// generation's walk over the same declarations — two derivations of one set,
/// which cannot part company unless the compiler is wrong. Without the marker,
/// "the compiled module imports `env`.`entropy`, which this program declares no
/// host binding for" sends a reader hunting their source for an `entropy` that
/// is not in it.
///
/// The detail stays where it was, ahead of this: it is the only description of
/// the disagreement that exists, so it is what a report has to carry.
///
/// [`HostImportError::MixedResolution`] is left without it because it is the one
/// arm about the *value* a caller assembled rather than about the bytes, and its
/// own body already says so — that the two were not produced by one resolution
/// is both the diagnosis and the place to look.
const COMPILER_BUG: &str = "This is a compiler bug; please report it, quoting the detail above";

impl std::fmt::Display for HostImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostImportError::Parse(reason) => write!(
                f,
                "internal error: the compiled module does not decode: {reason}. {COMPILER_BUG}"
            ),
            HostImportError::NonFunctionImport {
                module,
                field,
                kind,
            } => write!(
                f,
                "internal error: the compiled module imports a {kind}, {}, which no `external \
                 fn` declaration can describe; only function imports may be left for an embedder \
                 to satisfy. {COMPILER_BUG}",
                host_import_label(module, field)
            ),
            HostImportError::Undeclared { module, field } => write!(
                f,
                "internal error: the compiled module imports {}, which this program declares no \
                 host binding for. Every surviving import must be one an embedder was asked to \
                 supply, or the artifact will not instantiate for a reason nothing in the source \
                 explains. {COMPILER_BUG}",
                host_import_label(module, field)
            ),
            HostImportError::Missing { module, field } => write!(
                f,
                "internal error: this program declares the host import {}, which the compiled \
                 module does not carry. The declaration and the artifact have to agree, or an \
                 embedder registers a function nothing will call. {COMPILER_BUG}",
                host_import_label(module, field)
            ),
            HostImportError::SignatureMismatch {
                module,
                field,
                declared,
                emitted,
            } => {
                let emitted = emitted.as_ref().map_or_else(
                    || "a signature using a value type Inference does not model".to_string(),
                    |sig| format!("`{sig}`"),
                );
                write!(
                    f,
                    "internal error: the host import {} is declared `{declared}` but emitted \
                     with {emitted}. An embedder registers one function against the emitted \
                     signature, so a declaration that disagrees with it describes a call the \
                     program cannot make. {COMPILER_BUG}",
                    host_import_label(module, field)
                )
            }
            HostImportError::Duplicated { module, field } => write!(
                f,
                "internal error: the compiled module imports {} twice; one declaration cannot \
                 account for two entries, and no embedder can satisfy one name twice. \
                 {COMPILER_BUG}",
                host_import_label(module, field)
            ),
            HostImportError::MixedResolution {
                modules,
                host_imports,
            } => write!(
                f,
                "internal error: {modules} resolved external module(s) arrived together with \
                 {host_imports} host import(s); resolution refuses that combination, so these \
                 were not produced by one resolution"
            ),
        }
    }
}

impl std::error::Error for HostImportError {}

/// Every entry of `wasm`'s import section, in section order.
///
/// A decode failure is an [`Err`] and never an empty list. The distinction is
/// the whole safety property of this reader: an empty list compares equal to an
/// empty declared set, so bytes that do not parse would be waved through as a
/// module that imports nothing.
///
/// # Errors
///
/// [`HostImportError::Parse`] if the bytes do not decode.
pub(crate) fn module_imports(wasm: &[u8]) -> Result<Vec<ModuleImport>, HostImportError> {
    let mut types: Vec<Option<FuncType>> = Vec::new();
    let mut imports = Vec::new();
    for payload in Parser::new(0).parse_all(wasm) {
        let payload = payload.map_err(|e| HostImportError::Parse(e.to_string()))?;
        match payload {
            Payload::TypeSection(reader) => {
                for group in reader {
                    let group = group.map_err(|e| HostImportError::Parse(e.to_string()))?;
                    collect_types(&group, &mut types);
                }
            }
            Payload::ImportSection(reader) => {
                for import in reader {
                    let import = import.map_err(|e| HostImportError::Parse(e.to_string()))?;
                    let shape = match import.ty {
                        TypeRef::Func(type_idx) => {
                            ImportShape::Function(signature_at(&types, type_idx))
                        }
                        TypeRef::Table(_) => ImportShape::NonFunction("table"),
                        TypeRef::Memory(_) => ImportShape::NonFunction("memory"),
                        TypeRef::Global(_) => ImportShape::NonFunction("global"),
                        TypeRef::Tag(_) => ImportShape::NonFunction("tag"),
                    };
                    imports.push(ModuleImport {
                        module: import.module.to_string(),
                        field: import.name.to_string(),
                        shape,
                    });
                }
            }
            _ => {}
        }
    }
    Ok(imports)
}

/// The declared-signature form of the function type at `type_idx`, or `None`
/// when there is no function type there or it uses a value type no `external fn`
/// declaration can name.
///
/// Both misses collapse into `None` because both mean the same thing to the
/// caller: no declaration in any Inference program matches this import, so the
/// only correct outcome is a refusal.
fn signature_at(types: &[Option<FuncType>], type_idx: u32) -> Option<DeclaredSignature> {
    let func_type = types.get(type_idx as usize)?.as_ref()?;
    let params = func_type
        .params()
        .iter()
        .map(|&v| WasmValType::from_parser(v))
        .collect::<Option<Vec<_>>>()?;
    let results = func_type
        .results()
        .iter()
        .map(|&v| WasmValType::from_parser(v))
        .collect::<Option<Vec<_>>>()?;
    Some(DeclaredSignature { params, results })
}

/// Appends each type in a `RecGroup` to `out` in type-section order, keeping a
/// `None` slot for non-function composite types so that type indices stay
/// aligned with the section they reference.
fn collect_types(group: &RecGroup, out: &mut Vec<Option<FuncType>>) {
    for sub_type in group.types() {
        match &sub_type.composite_type.inner {
            CompositeInnerType::Func(func_type) => out.push(Some(func_type.clone())),
            _ => out.push(None),
        }
    }
}

/// Holds `main_wasm`'s import section to exactly `declared`, comparing
/// `(module, field, params, results)`.
///
/// Set **equality**, in both directions, and on the signature rather than the
/// name alone. Each direction catches a different thing and neither is a
/// formality:
///
/// - an emitted import no declaration covers is an artifact that will not
///   instantiate, for a reason the source does not explain;
/// - a declaration no import covers means the program compiled calls against a
///   function the artifact never asks anyone for;
/// - a signature difference means the embedder's registration and the program's
///   call sites disagree about the stack, which traps at the first call.
///
/// `declared` must be sorted by `(module, field)` and free of duplicate pairs —
/// which is what [`super::driver::resolve_external_modules`] returns — because
/// the lookup is a binary search. An unsorted list cannot turn a disagreement
/// into an acceptance: a binary search over one either finds an equal entry or
/// finds nothing, so the worst it can do is refuse an artifact that agreed.
///
/// # Errors
///
/// [`HostImportError`] naming the first disagreement found, in artifact order
/// and then in declaration order.
pub(crate) fn check_declared_host_imports(
    main_wasm: &[u8],
    declared: &[HostImport],
) -> Result<(), HostImportError> {
    let mut matched = vec![false; declared.len()];
    for import in module_imports(main_wasm)? {
        let emitted = match import.shape {
            ImportShape::Function(signature) => signature,
            ImportShape::NonFunction(kind) => {
                return Err(HostImportError::NonFunctionImport {
                    module: import.module,
                    field: import.field,
                    kind,
                })
            }
        };
        let Ok(position) = declared.binary_search_by(|candidate| {
            (candidate.module.as_str(), candidate.field.as_str())
                .cmp(&(import.module.as_str(), import.field.as_str()))
        }) else {
            return Err(HostImportError::Undeclared {
                module: import.module,
                field: import.field,
            });
        };
        if std::mem::replace(&mut matched[position], true) {
            return Err(HostImportError::Duplicated {
                module: import.module,
                field: import.field,
            });
        }
        if emitted.as_ref() != Some(&declared[position].signature) {
            return Err(HostImportError::SignatureMismatch {
                module: import.module,
                field: import.field,
                declared: Box::new(declared[position].signature.clone()),
                emitted: emitted.map(Box::new),
            });
        }
    }

    if let Some(position) = matched.iter().position(|seen| !seen) {
        return Err(HostImportError::Missing {
            module: declared[position].module.clone(),
            field: declared[position].field.clone(),
        });
    }
    Ok(())
}
