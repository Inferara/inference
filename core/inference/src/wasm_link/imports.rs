//! What a compiled module's import section declares, and whether it declares
//! exactly the host imports the program's `external fn` bindings did, and the
//! memory import the build's layout asked for when it asked for one.
//!
//! Every other module here runs *before* any bytes exist. This one runs after:
//! it reads the artifact codegen produced and holds it to the declarations the
//! [driver](super::driver) collected. That pairing is the whole reason a host
//! import can ship without a link step — the linker's guarantee is that no
//! import survives it, and what replaces that guarantee is the check below,
//! which says every import that survived is one this build asked for and
//! nothing it asked for was lost on the way.
//!
//! The two sets of function imports are produced by entirely separate walks
//! over separate data — the driver's over the type checker's extern
//! provenance, codegen's over the same declarations through its own lowering —
//! so their agreement is a real check and not a restatement.
//!
//! The memory import is held to something the driver never sees: the
//! [`MemoryImport`] the caller passes in, which is the build's memory
//! configuration and the value codegen wrote the import from. A build that
//! asked to import its memory may leave exactly that one memory import for an
//! embedder, under that name and at those limits, and a build that did not may
//! leave none.

use inf_wasmparser::{
    CompositeInnerType, FuncType, MemoryType, Parser, Payload, RecGroup, TypeRef,
};
use inference_wasm_codegen::MemoryImport;

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
    /// A memory, with the type the entry declares for it, which the check
    /// compares against the memory import the build's layout asked for.
    Memory(MemoryType),
    /// A table, global or tag, named for a diagnostic.
    NonFunction(&'static str),
}

/// Why an artifact's imports could not be reconciled with a program's host
/// declarations and the memory import its build's layout asked for, or why the
/// parts a caller assembled — the two halves of a resolution, or resolved
/// modules and a layout that imports its memory — could not be linked.
#[derive(Debug)]
pub enum HostImportError {
    /// The module's bytes do not decode.
    Parse(String),
    /// The module imports something that is not a function and not the memory
    /// its build's layout asked to import: a table, a global, a tag, or a
    /// memory when the layout asked for none. No `external fn` declaration can
    /// describe one, so nothing in the program accounts for it.
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
    /// The module imports one two-level name twice: as two functions, or, in a
    /// build whose layout imports its memory, as two memories or as the memory
    /// and a function. One declaration cannot account for two import entries,
    /// nor one layout for two memories, and an embedder cannot satisfy one name
    /// twice.
    Duplicated { module: String, field: String },
    /// The module, in a build whose layout imports its memory, imports a
    /// memory other than the one the layout asked for — under another name,
    /// at other limits, or shared, 64-bit or at a custom page size — whether in
    /// place of that import or beside it. `module` and `field` are the emitted
    /// import's name, and `emitted` is its type in the text format's spelling:
    /// `(memory 1 2)`, with no maximum when it has none (`(memory 1)`), `i64`
    /// ahead of a 64-bit memory's limits (`(memory i64 1 2)`), `shared` after
    /// the limits (`(memory 1 2 shared)`), and a custom page size last, in
    /// bytes (`(memory 1 2 (pagesize 1))`), or as the power of two it declares
    /// when that is too large to count in a `u64` (`(pagesize 2^64)`).
    ///
    /// The emitted type is carried as that text and not as the decoder's own
    /// value so that the decoder stays out of this crate's public API: the
    /// text is everything the report needs, and the report is all a reader
    /// gets.
    MemoryImportMismatch {
        module: String,
        field: String,
        expected: MemoryImport,
        emitted: String,
    },
    /// The build's layout asked to import its memory and the module imports
    /// no memory at all. `module` and `field` are the name the layout asked
    /// for, [`MemoryImport::MODULE`] and [`MemoryImport::FIELD`].
    MissingMemoryImport { module: String, field: String },
    /// Resolution produced both merged modules and host imports. The driver
    /// refuses that program before it resolves anything, so reaching here means
    /// the two were assembled by something other than one resolution.
    MixedResolution {
        modules: usize,
        host_imports: usize,
    },
    /// Resolution produced merged modules for a build whose layout imports its
    /// memory. Code generation refuses every linked binding in such a build, so
    /// reaching here means the two were assembled by something other than one
    /// build. The static merge models only a memory its modules define, so
    /// linking would leave the memory the layout asked for unread: an artifact
    /// that imports none would ship a module the host's memory never reaches.
    ModulesWithImportedMemory { modules: usize },
}

/// What every arm below but [`HostImportError::MixedResolution`] and
/// [`HostImportError::ModulesWithImportedMemory`] adds after the detail it
/// carries.
///
/// Every arm here reaches a terminal through `infc`'s `Link step failed: {e}`,
/// and none of the eight can be caused by anything an author wrote.
/// [`HostImportError::Parse`] is code generation writing bytes that do not
/// decode. Each of the others is two derivations of one set parting company,
/// which cannot happen unless the compiler is wrong. For the function imports
/// the two are the driver's walk over the type checker's extern provenance and
/// code generation's walk over the same declarations. For the memory import
/// they are the build's memory layout and the import code generation emitted
/// from it. Without the marker,
/// "the compiled module imports `env`.`entropy`, which this program declares no
/// host binding for" sends a reader hunting their source for an `entropy` that
/// is not in it.
///
/// The detail stays where it was, ahead of this: it is the only description of
/// the disagreement that exists, so it is what a report has to carry.
///
/// [`HostImportError::MixedResolution`] and
/// [`HostImportError::ModulesWithImportedMemory`] are left without it because
/// they are the two arms about the *value* a caller assembled rather than about
/// the bytes, and each body already says so — that the parts were not produced
/// by one resolution, or by one build, is both the diagnosis and the place to
/// look.
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
                 fn` declaration can describe; only function imports, and the memory of a build \
                 whose layout asks to import it, may be left for an embedder to satisfy. \
                 {COMPILER_BUG}",
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
                "internal error: the compiled module imports {} twice; one declaration, or the \
                 one memory a layout imports, cannot account for two entries, and no embedder \
                 can satisfy one name twice. {COMPILER_BUG}",
                host_import_label(module, field)
            ),
            HostImportError::MemoryImportMismatch {
                module,
                field,
                expected,
                emitted,
            } => write!(
                f,
                "internal error: this build's memory layout asks to import its memory as {} with \
                 type `{}`, but the compiled module imports a memory as {} with type `{emitted}`. \
                 An embedder provides the memory this build's configuration describes, and the \
                 stack and data were laid out in it, so an import that disagrees either asks for \
                 a memory the embedder was not told to provide or admits one the program was not \
                 laid out for. {COMPILER_BUG}",
                host_import_label(MemoryImport::MODULE, MemoryImport::FIELD),
                memory_type_text(&expected_memory_type(*expected)),
                host_import_label(module, field)
            ),
            HostImportError::MissingMemoryImport { module, field } => write!(
                f,
                "internal error: this build's memory layout asks to import its memory as {}, \
                 which the compiled module does not import. The layout and the artifact have to \
                 agree, or an embedder provides a memory the program never uses. {COMPILER_BUG}",
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
            HostImportError::ModulesWithImportedMemory { modules } => write!(
                f,
                "internal error: {modules} resolved external module(s) arrived with a memory \
                 layout that imports its memory as {}; code generation refuses a linked module \
                 in a build that imports its memory, so these were not produced by one build, \
                 and the static merge cannot carry a memory a host supplies",
                host_import_label(MemoryImport::MODULE, MemoryImport::FIELD)
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
                        TypeRef::Memory(ty) => ImportShape::Memory(ty),
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
/// `(module, field, params, results)`, and to exactly the memory import
/// `memory` describes, if it describes one.
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
/// `memory` is the caller's and not the driver's: the memory import the
/// build's layout asked for, which is the value codegen emitted the import
/// from, and which no `external fn` declaration could describe. With `None`,
/// a memory import is refused like a table, global or tag import. With
/// `Some`, the artifact must import exactly one memory, under the name
/// [`MemoryImport::MODULE`] and [`MemoryImport::FIELD`] spell, 32-bit,
/// unshared, at the default page size, and with `memory`'s minimum and
/// maximum, the type codegen emits it at.
///
/// With `Some`, a function import under the memory's name, in an artifact that
/// also imports a memory under it, is refused as a duplicate of that memory
/// whether it comes before or after it, and whether or not `declared` names
/// it. Like every refusal here it is reported where it stands in artifact
/// order, so a function after a memory under that name at the wrong type is
/// never reached: the mismatch comes first. With no memory under that name the
/// function is checked like any other, and the absent memory is reported once
/// the section has been read, so that neither refusal says the module carries
/// an entry it does not.
///
/// # Errors
///
/// [`HostImportError`] naming the first disagreement found, in artifact order.
/// Absences are reported after every entry has been examined: first a memory
/// `memory` describes and the artifact does not import, since codegen emits
/// the memory ahead of every function import, then a declaration no import
/// covers, in declaration order.
pub(crate) fn check_declared_host_imports(
    main_wasm: &[u8],
    declared: &[HostImport],
    memory: Option<MemoryImport>,
) -> Result<(), HostImportError> {
    let imports = module_imports(main_wasm)?;
    let memory_name_imports_a_memory = memory.is_some()
        && imports.iter().any(|import| {
            matches!(import.shape, ImportShape::Memory(_))
                && is_memory_import_name(&import.module, &import.field)
        });
    let mut matched = vec![false; declared.len()];
    let mut memory_seen = false;
    for import in imports {
        let emitted = match import.shape {
            ImportShape::Function(_)
                if memory_name_imports_a_memory
                    && is_memory_import_name(&import.module, &import.field) =>
            {
                return Err(HostImportError::Duplicated {
                    module: import.module,
                    field: import.field,
                })
            }
            ImportShape::Function(signature) => signature,
            ImportShape::Memory(ty) => {
                let Some(expected) = memory else {
                    return Err(HostImportError::NonFunctionImport {
                        module: import.module,
                        field: import.field,
                        kind: "memory",
                    });
                };
                check_memory_import(import.module, import.field, ty, expected, memory_seen)?;
                memory_seen = true;
                continue;
            }
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

    if memory.is_some() && !memory_seen {
        return Err(HostImportError::MissingMemoryImport {
            module: MemoryImport::MODULE.to_string(),
            field: MemoryImport::FIELD.to_string(),
        });
    }
    if let Some(position) = matched.iter().position(|seen| !seen) {
        return Err(HostImportError::Missing {
            module: declared[position].module.clone(),
            field: declared[position].field.clone(),
        });
    }
    Ok(())
}

/// Holds one memory import entry to the memory `expected` describes, given
/// whether a memory under that name has already been accepted.
///
/// The name is checked for a repeat before the type is compared, so a second
/// memory under the memory's name is [`HostImportError::Duplicated`] whatever
/// its type, while a memory under any other name is
/// [`HostImportError::MemoryImportMismatch`] however many there are.
fn check_memory_import(
    module: String,
    field: String,
    emitted: MemoryType,
    expected: MemoryImport,
    already_accepted: bool,
) -> Result<(), HostImportError> {
    let named_as_expected = is_memory_import_name(&module, &field);
    if named_as_expected && already_accepted {
        return Err(HostImportError::Duplicated { module, field });
    }
    if !named_as_expected || emitted != expected_memory_type(expected) {
        return Err(HostImportError::MemoryImportMismatch {
            module,
            field,
            expected,
            emitted: memory_type_text(&emitted),
        });
    }
    Ok(())
}

/// Whether `(module, field)` is the two-level name a build imports its memory
/// under.
fn is_memory_import_name(module: &str, field: &str) -> bool {
    module == MemoryImport::MODULE && field == MemoryImport::FIELD
}

/// The type an import of `expected` declares: 32-bit, unshared, at the default
/// page size, with both limits present — the one shape codegen emits it at.
fn expected_memory_type(expected: MemoryImport) -> MemoryType {
    MemoryType {
        memory64: false,
        shared: false,
        initial: u64::from(expected.minimum),
        maximum: Some(u64::from(expected.maximum)),
        page_size_log2: None,
    }
}

/// Renders a memory type in the text format's spelling, `(memory 1 2)`: `i64`
/// ahead of a 64-bit memory's limits, the maximum only when there is one,
/// `shared` after the limits, and a custom page size last as
/// `(pagesize N)`, `N` in bytes.
///
/// That is `wasmprinter`'s order, with the page size in decimal rather than
/// hexadecimal. A page size too large to count in a `u64` is written as the
/// power of two the bytes declare, `(pagesize 2^K)`, which no valid module
/// declares but a decoded one can.
fn memory_type_text(ty: &MemoryType) -> String {
    let index = if ty.memory64 { "i64 " } else { "" };
    let maximum = ty
        .maximum
        .map_or_else(String::new, |maximum| format!(" {maximum}"));
    let shared = if ty.shared { " shared" } else { "" };
    let page_size = match ty.page_size_log2 {
        None => String::new(),
        Some(log2) => match 1_u64.checked_shl(log2) {
            Some(bytes) => format!(" (pagesize {bytes})"),
            None => format!(" (pagesize 2^{log2})"),
        },
    };
    format!("(memory {index}{}{maximum}{shared}{page_size})", ty.initial)
}
