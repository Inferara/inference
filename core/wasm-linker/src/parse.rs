//! Owned, section-by-section representation of a parsed WASM module.
//!
//! The linker needs to inspect and re-emit both the main module and each
//! external module. `inf-wasmparser` yields borrowed views into the original
//! bytes; this module copies the parts the linker manipulates into owned
//! structures so the borrow does not outlive a single parse pass.
//!
//! The main module is rebuilt section-by-section after merging, so its
//! exports, memory, globals, and name/custom sections are all retained. An
//! external module is only mined for the closure of a satisfied export, so for
//! those only the type table, import/local function split, exports, and bodies
//! matter — but the same structure is reused for both.

use std::collections::BTreeMap;

use inf_wasmparser::{
    CompositeInnerType, CustomSectionReader, Export, ExternalKind, FuncType, GlobalType, Import,
    KnownCustom, MemoryType, Name, Operator, Parser, Payload, RecGroup, TableType, TypeRef, ValType,
};

use crate::LinkError;

/// A WASM function signature, owned so it survives the parse borrow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FuncSig {
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
}

impl FuncSig {
    fn from_func_type(ty: &FuncType) -> Self {
        FuncSig {
            params: ty.params().to_vec(),
            results: ty.results().to_vec(),
        }
    }
}

/// A type-section entry. Non-function composite types are retained as `Other`
/// so that type indices stay aligned with the section they came from; the
/// merge pass only ever copies function types into the output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TypeEntry {
    Func(FuncSig),
    Other,
}

/// An imported function: its `(module, field)` pair and the type index it
/// references.
#[derive(Debug, Clone)]
pub(crate) struct ImportedFunc {
    pub module: String,
    pub field: String,
    pub type_idx: u32,
}

/// An exported entity and the index it names, retaining its kind so the main
/// module's `memory` / `__stack_pointer` exports survive the rebuild.
#[derive(Debug, Clone)]
pub(crate) struct ExportEntry {
    pub name: String,
    pub kind: ExternalKind,
    pub index: u32,
}

/// A locally-defined function: its type index plus the verbatim body bytes.
#[derive(Debug, Clone)]
pub(crate) struct LocalFunc {
    pub type_idx: u32,
    /// Raw body bytes: the locals vector and operator stream, *without* the
    /// leading body byte-length prefix — what `wasm-encoder::Function::raw`
    /// consumes and what the rewrite pass walks.
    pub body: Vec<u8>,
}

/// A global definition, captured with the operators of its (constant)
/// initializer so it can be re-emitted faithfully.
#[derive(Debug, Clone)]
pub(crate) struct GlobalDef {
    pub ty: GlobalType,
    /// The constant initializer as `i32.const` / `i64.const` style operators.
    /// The main module only ever emits a single `i32.const` here.
    pub init: GlobalInit,
}

/// The constant initializer of a global, restricted to the forms the codegen
/// output produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlobalInit {
    I32(i32),
    I64(i64),
}

/// Whether a parsed module is the main module being linked or an external
/// dependency merged into it.
///
/// This is what decides whether the two verification custom sections are
/// decoded. The main module's drive proof-mode translation and are re-emitted,
/// so they are decoded and a malformed one is a hard error. An external's
/// describe a module the output is not — only the executable closure of a
/// satisfied export crosses the merge — so they are decoded only when the
/// caller asked to adopt the library's obligations, and are otherwise merely
/// noted, which is what keeps a malformed section in a library nothing needed
/// from failing a link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModuleRole {
    Main,
    /// An external, carrying whether the caller asked to adopt its verification
    /// sections — which is what decides whether they are decoded or merely
    /// noted.
    External { decode_specs: bool },
}

/// The subset of a WASM module the static-merge linker manipulates.
#[derive(Debug, Clone, Default)]
pub(crate) struct ParsedModule {
    pub types: Vec<TypeEntry>,
    pub imported_funcs: Vec<ImportedFunc>,
    /// Count of non-function imports — a self-contained module has none.
    pub non_func_imports: usize,
    pub exports: Vec<ExportEntry>,
    /// Locally-defined functions in function-index order (imports occupy the
    /// indices below `imported_funcs.len()`).
    pub local_funcs: Vec<LocalFunc>,
    pub globals: Vec<GlobalDef>,
    pub tables: Vec<TableType>,
    pub element_count: usize,
    pub data_count: usize,
    pub memory: Option<MemoryType>,
    /// The number of memories the module declares. A module with more than one
    /// memory cannot be merged: the body's memargs would name memories the
    /// single shared output memory does not have. Counted (rather than inferred
    /// from `memory.is_some()`) so the merge can reject multi-memory modules
    /// explicitly instead of silently dropping every memory past the first.
    pub memory_count: usize,
    /// The start-function index, if the module declares a start section. A
    /// merged external must not declare one: its initialization closure is never
    /// folded in, so the merge rejects it rather than silently dropping the
    /// side-effects.
    pub start: Option<u32>,
    /// Debug names from the `name` custom section, keyed by global function
    /// index. Retained so the merged output keeps sane function names (which the
    /// Rocq translator reads to name its `Definition`s); merged external bodies
    /// that carry no name fall back to their satisfied export field.
    pub func_names: BTreeMap<u32, String>,
    /// The module name from the `name` custom section's module subsection, if
    /// present. Re-emitted so the Rocq translator's `Definition <module>` survives
    /// the merge.
    pub module_name: Option<String>,
    /// Per-function local names from the `name` section's local subsection,
    /// keyed by global function index, each carrying `(local_idx, name)` pairs.
    /// The function index shifts with the merge; the local indices within a
    /// function do not. Retained so the proof artifact keeps local debug names.
    pub local_names: BTreeMap<u32, Vec<(u32, String)>>,
    /// The decoded `inference.spec_funcs` custom section: `spec_name ->
    /// [func_idx]` in the *pre-link* index space. The merge rewrites each index
    /// into the output space and re-emits the section, so a bare linked `.wasm`
    /// still names the correct spec functions (the input to formal verification).
    ///
    /// An external's is decoded only when the caller asked to adopt the
    /// library's obligations, and then only so the library's own
    /// obligations-are-a-subset-of-specifications invariant can be checked
    /// before anything is adopted from it. Its indices name the library's own
    /// specification functions, which no export closure reaches, so they are
    /// never remapped and never re-emitted.
    pub spec_funcs: Option<Vec<(String, Vec<u32>)>>,
    /// The decoded `inference.hspecs` custom section: per-spec `hassert`
    /// obligations keyed by folded spec name. Unlike `spec_funcs`, the payload
    /// references functions by symbolic name, not index, so no index remap
    /// applies. Decoded (validated) here so a corrupt main section fails the
    /// link; the merge edits the map — pointing a symbol that names a root alias
    /// the output's name section could not record at the name it did record, and
    /// folding in whatever it adopts from a library — then re-validates and
    /// re-encodes canonically.
    ///
    /// An external's is decoded only when the caller asked to adopt the
    /// library's obligations. Under every other policy the section is noted
    /// through [`Self::carries_hspecs`] and never read, which is what keeps a
    /// malformed one in a library nothing needed from failing a link.
    pub hspecs: Option<inference_hassert::HSpecMap>,
    /// Whether the module carries an `inference.hspecs` section, recorded even
    /// when the section is not decoded.
    ///
    /// This is the obligation carrier. A `spec_funcs` section without one
    /// describes specifications that state nothing, so a report about dropped
    /// obligations keys on this flag alone. Under a non-adopting policy an
    /// external's section is not decoded, so presence is all the merge can
    /// honestly report.
    pub carries_hspecs: bool,
    /// The logical, `::`-joined module reference this module was bound under
    /// (e.g. `"crypto::sha256"`), for an external; empty for the main module.
    /// The merge matches each main-module import's recorded `(module, field)`
    /// against this, so two externals exporting the same field but bound under
    /// different logical modules can be disambiguated.
    pub logical_module: String,
}

impl ParsedModule {
    /// The function index of the first locally-defined function.
    pub(crate) fn local_func_base(&self) -> u32 {
        self.imported_funcs.len() as u32
    }

    /// Returns the type signature for a function by its global function index.
    pub(crate) fn func_sig(&self, func_idx: u32) -> Option<&FuncSig> {
        let type_idx = if (func_idx as usize) < self.imported_funcs.len() {
            self.imported_funcs[func_idx as usize].type_idx
        } else {
            self.local_funcs
                .get(func_idx as usize - self.imported_funcs.len())?
                .type_idx
        };
        match self.types.get(type_idx as usize)? {
            TypeEntry::Func(sig) => Some(sig),
            TypeEntry::Other => None,
        }
    }

    /// The debug name recorded for a function by its global function index,
    /// if the source module carried a `name` custom section entry for it.
    pub(crate) fn func_name(&self, func_idx: u32) -> Option<&str> {
        self.func_names.get(&func_idx).map(String::as_str)
    }

    /// The function index an export of this name resolves to, if it is a
    /// function export.
    pub(crate) fn exported_func_index(&self, name: &str) -> Option<u32> {
        self.exports
            .iter()
            .find(|e| e.name == name && e.kind == ExternalKind::Func)
            .map(|e| e.index)
    }

    /// Parses `bytes` into the owned representation, recording `logical_module`
    /// as the logical name the module was bound under. The merge uses it to
    /// disambiguate two externals that export the same field but were bound from
    /// different logical modules.
    ///
    /// An external module's specification functions are *not* merged into the
    /// executable output: only the executable closure of the satisfied export
    /// crosses the merge. So its two verification sections describe a module the
    /// output is not, and `decode_specs` says what to do about that. With it
    /// clear the sections are noted and never read, so a malformed one in an
    /// external cannot fail the link. With it set — the caller asked to adopt
    /// the library's universal obligations — both are decoded and validated, and
    /// a malformed or duplicated one is a hard [`LinkError`] naming the logical
    /// module.
    pub(crate) fn parse_external(
        bytes: &[u8],
        logical_module: &str,
        decode_specs: bool,
    ) -> Result<Self, LinkError> {
        Self::parse_with_role(bytes, ModuleRole::External { decode_specs }, logical_module)
    }

    /// Parses the main module's `bytes`, decoding its `inference.spec_funcs` and
    /// `inference.hspecs` sections (verification deliverables the merge
    /// re-emits, the first re-indexed).
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, LinkError> {
        Self::parse_with_role(bytes, ModuleRole::Main, "")
    }

    /// Parses `bytes` into the owned representation under the given `role`, which
    /// decides whether the `inference.spec_funcs` and `inference.hspecs` custom
    /// sections are decoded.
    ///
    /// `logical_module` is recorded before the walk rather than after it, so a
    /// rejection raised while decoding an external's verification section can
    /// name the module the caller bound it under. It is empty for the main
    /// module, which needs no such qualifier.
    fn parse_with_role(
        bytes: &[u8],
        role: ModuleRole,
        logical_module: &str,
    ) -> Result<Self, LinkError> {
        let mut module = ParsedModule {
            logical_module: logical_module.to_string(),
            ..ParsedModule::default()
        };

        // Running cursor into `local_funcs` for code-section assignment. Code
        // bodies arrive in function-declaration order, so the i-th body fills
        // slot i; tracking the next slot makes parsing O(N) instead of the
        // O(N^2) linear scan a wide external module would otherwise incur.
        let mut next_body_idx = 0usize;

        for payload in Parser::new(0).parse_all(bytes) {
            let payload = payload.map_err(|e| LinkError::Parse(e.to_string()))?;
            match payload {
                Payload::TypeSection(reader) => {
                    for group in reader {
                        let group = group.map_err(|e| LinkError::Parse(e.to_string()))?;
                        collect_types(&group, &mut module.types);
                    }
                }
                Payload::ImportSection(reader) => {
                    for import in reader {
                        let import = import.map_err(|e| LinkError::Parse(e.to_string()))?;
                        collect_import(&import, &mut module);
                    }
                }
                Payload::FunctionSection(reader) => {
                    for type_idx in reader {
                        let type_idx = type_idx.map_err(|e| LinkError::Parse(e.to_string()))?;
                        module.local_funcs.push(LocalFunc {
                            type_idx,
                            body: Vec::new(),
                        });
                    }
                }
                Payload::ExportSection(reader) => {
                    for export in reader {
                        let Export { name, kind, index } =
                            export.map_err(|e| LinkError::Parse(e.to_string()))?;
                        module.exports.push(ExportEntry {
                            name: name.to_string(),
                            kind,
                            index,
                        });
                    }
                }
                Payload::GlobalSection(reader) => {
                    for global in reader {
                        let global = global.map_err(|e| LinkError::Parse(e.to_string()))?;
                        module.globals.push(collect_global(&global)?);
                    }
                }
                Payload::TableSection(reader) => {
                    for table in reader {
                        let table = table.map_err(|e| LinkError::Parse(e.to_string()))?;
                        module.tables.push(table.ty);
                    }
                }
                Payload::ElementSection(reader) => {
                    module.element_count += reader.count() as usize;
                }
                Payload::DataSection(reader) => {
                    module.data_count += reader.count() as usize;
                }
                Payload::MemorySection(reader) => {
                    for memory in reader {
                        let memory = memory.map_err(|e| LinkError::Parse(e.to_string()))?;
                        module.memory_count += 1;
                        if module.memory.is_none() {
                            module.memory = Some(memory);
                        }
                    }
                }
                Payload::StartSection { func, .. } => {
                    module.start = Some(func);
                }
                Payload::CodeSectionEntry(body) => {
                    assign_body(&mut module, &mut next_body_idx, &body)?;
                }
                Payload::CustomSection(reader) => {
                    collect_custom_section(&reader, &mut module, role)?;
                }
                _ => {}
            }
        }

        Ok(module)
    }
}

fn collect_types(group: &RecGroup, out: &mut Vec<TypeEntry>) {
    for sub_type in group.types() {
        match &sub_type.composite_type.inner {
            CompositeInnerType::Func(func_type) => {
                out.push(TypeEntry::Func(FuncSig::from_func_type(func_type)));
            }
            _ => out.push(TypeEntry::Other),
        }
    }
}

/// Mines a custom section for everything the merge must carry through: the
/// `name` section's module/function/local subsections, and the
/// `inference.spec_funcs` and `inference.hspecs` sections that drive proof-mode
/// translation.
///
/// The `name` subsections are best-effort (an unparseable one is skipped). The
/// verification payloads, by contrast, are deliverables: where they are decoded
/// at all, a malformed or duplicated one is a hard [`LinkError`], never silently
/// dropped.
///
/// Where they are decoded depends on the `role`. The main module's always are.
/// An external's are only when the caller asked to adopt the library's
/// obligations, because otherwise nothing reads them — the merge mines an
/// external for the executable closure of a satisfied export and nothing else —
/// and decoding them would let a corrupt section in a library nothing needed
/// fail the link. Presence of `inference.hspecs` is recorded under every role,
/// since that is what a report about dropped obligations keys on and it costs
/// no decoding.
fn collect_custom_section(
    custom: &CustomSectionReader,
    module: &mut ParsedModule,
    role: ModuleRole,
) -> Result<(), LinkError> {
    if custom.name() == crate::spec_funcs::SECTION_NAME {
        if !decodes_verification_sections(role) {
            return Ok(());
        }
        // A second spec_funcs section would silently discard the first under a
        // last-wins assignment, dropping its proof obligations. Since the section
        // is a verification deliverable, reject the duplicate with a clean error
        // rather than vanish the earlier specifications.
        if module.spec_funcs.is_some() {
            return Err(duplicate_section_error(
                role,
                &module.logical_module,
                "inference.spec_funcs",
                "specifications",
            ));
        }
        let decoded = crate::spec_funcs::decode(custom.data())
            .map_err(|e| qualify_external_error(role, &module.logical_module, e))?;
        module.spec_funcs = Some(decoded);
        return Ok(());
    }

    if custom.name() == inference_hassert::HSPECS_SECTION_NAME {
        module.carries_hspecs = true;
        if !decodes_verification_sections(role) {
            return Ok(());
        }
        // As with `spec_funcs`, a duplicate would silently drop the first
        // section's obligations. Reject it rather than overwrite.
        if module.hspecs.is_some() {
            return Err(duplicate_section_error(
                role,
                &module.logical_module,
                "inference.hspecs",
                "obligations",
            ));
        }
        // Decode to validate: a corrupt payload is a hard error here rather than
        // a corrupt artifact the Rocq translator chokes on later. The merge
        // re-encodes the decoded map canonically.
        let decoded = inference_hassert::decode(custom.data())
            .map_err(|e| LinkError::Parse(format!("inference.hspecs section: {e}")))
            .map_err(|e| qualify_external_error(role, &module.logical_module, e))?;
        module.hspecs = Some(decoded);
        return Ok(());
    }

    let KnownCustom::Name(names) = custom.as_known() else {
        return Ok(());
    };
    for subsection in names {
        let Ok(subsection) = subsection else {
            continue;
        };
        match subsection {
            Name::Module { name, .. } => module.module_name = Some(name.to_string()),
            Name::Function(func_names) => {
                for naming in func_names {
                    let Ok(naming) = naming else {
                        continue;
                    };
                    module.func_names.insert(naming.index, naming.name.to_string());
                }
            }
            Name::Local(indirect) => {
                for per_func in indirect {
                    let Ok(per_func) = per_func else {
                        continue;
                    };
                    let mut locals = Vec::new();
                    for naming in per_func.names {
                        let Ok(naming) = naming else {
                            continue;
                        };
                        locals.push((naming.index, naming.name.to_string()));
                    }
                    if !locals.is_empty() {
                        module.local_names.insert(per_func.index, locals);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Whether this role reads the two verification custom sections at all.
///
/// The main module's are deliverables it re-emits. An external's describe a
/// module the output is not, so they are read only when the caller asked to
/// adopt the library's universal obligations — which is what keeps a corrupt
/// section in a library nothing needed from failing a link.
fn decodes_verification_sections(role: ModuleRole) -> bool {
    match role {
        ModuleRole::Main => true,
        ModuleRole::External { decode_specs } => decode_specs,
    }
}

/// The rejection for a module declaring one verification section twice, whose
/// repair differs by role: the main module's producer emitted a program it must
/// fix, while a library's is only ever read to adopt from, so the message says
/// what adopting would have lost.
fn duplicate_section_error(
    role: ModuleRole,
    logical_module: &str,
    section: &str,
    contents: &str,
) -> LinkError {
    match role {
        ModuleRole::Main => LinkError::Parse(format!(
            "main module declares more than one {section} section; \
             its proof obligations would be silently dropped"
        )),
        ModuleRole::External { .. } => LinkError::Parse(format!(
            "linked module `{logical_module}` declares more than one {section} section; \
             adopting from it would silently drop the first section's {contents}"
        )),
    }
}

/// Prefixes a verification-section rejection with the library it was raised
/// for, leaving the main module's own diagnostics as they were.
///
/// There is one main module, so naming it adds nothing. An external's section is
/// read only under adoption, and which library was being adopted from is the
/// first thing the reader needs — the section name alone would leave them
/// searching every dependency for a payload the merge never even reports on by
/// default.
fn qualify_external_error(role: ModuleRole, logical_module: &str, err: LinkError) -> LinkError {
    match (role, err) {
        (ModuleRole::External { .. }, LinkError::Parse(detail)) => {
            LinkError::Parse(format!("linked module `{logical_module}`: {detail}"))
        }
        (_, err) => err,
    }
}

fn collect_import(import: &Import, module: &mut ParsedModule) {
    match import.ty {
        TypeRef::Func(type_idx) => module.imported_funcs.push(ImportedFunc {
            module: import.module.to_string(),
            field: import.name.to_string(),
            type_idx,
        }),
        TypeRef::Global(_) | TypeRef::Table(_) | TypeRef::Memory(_) | TypeRef::Tag(_) => {
            module.non_func_imports += 1;
        }
    }
}

fn collect_global(global: &inf_wasmparser::Global) -> Result<GlobalDef, LinkError> {
    let mut ops = global.init_expr.get_operators_reader();
    let first = ops
        .read()
        .map_err(|e| LinkError::Parse(e.to_string()))?;
    let init = match first {
        Operator::I32Const { value } => GlobalInit::I32(value),
        Operator::I64Const { value } => GlobalInit::I64(value),
        // Only the two integer constant initializers are modeled. A float
        // initializer (`f32.const`/`f64.const`) is the most likely "other" here —
        // the Inference language has no `f32`/`f64` types — so the catch-all names
        // what is supported rather than mislabeling a constant `f32.const` as
        // "non-constant". A float global is also rejected up front by the feature
        // gate; this is the chokepoint for the main-module path that bypasses it.
        other => {
            return Err(LinkError::UnsupportedConstruct(format!(
                "unsupported global initializer for the static merge: {other:?} \
                 (only i32.const/i64.const are modeled)"
            )));
        }
    };
    Ok(GlobalDef {
        ty: global.ty,
        init,
    })
}

/// Stores a code-section body against the local function at `next_body_idx`,
/// then advances the cursor. Bodies arrive in function-declaration order, so
/// this assigns body `i` to local function `i` in a single linear pass.
fn assign_body(
    module: &mut ParsedModule,
    next_body_idx: &mut usize,
    body: &inf_wasmparser::FunctionBody,
) -> Result<(), LinkError> {
    let Some(slot) = module.local_funcs.get_mut(*next_body_idx) else {
        return Err(LinkError::Parse(
            "code section has more bodies than declared functions".into(),
        ));
    };
    slot.body = body.as_bytes().to_vec();
    *next_body_idx += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Parser unit tests for sections and constructs the `link` API only sees
    //! indirectly: non-function imports, element/data/table counting, and the
    //! function-signature lookup that bridges the import/local index split.

    use super::*;

    fn parse(wat: &str) -> ParsedModule {
        let bytes = wat::parse_str(wat).expect("valid WAT");
        ParsedModule::parse(&bytes).expect("parse")
    }

    #[test]
    fn counts_non_function_imports_separately() {
        // A memory import and a global import are non-function imports; only the
        // function import enters `imported_funcs`, the rest bump the count.
        let module = parse(
            r#"
            (module
              (type (;0;) (func))
              (import "env" "memory" (memory (;0;) 1))
              (import "env" "g" (global (;0;) i32))
              (import "env" "log" (func (;0;) (type 0)))
              (func (;1;) (type 0))
              (export "f" (func 1)))
            "#,
        );
        assert_eq!(module.imported_funcs.len(), 1, "one function import");
        assert_eq!(module.imported_funcs[0].field, "log");
        assert_eq!(
            module.non_func_imports, 2,
            "the memory and global imports are counted as non-function imports"
        );
    }

    #[test]
    fn counts_table_element_and_data_sections() {
        let module = parse(
            r#"
            (module
              (type (;0;) (func))
              (table (;0;) 1 1 funcref)
              (memory (;0;) 1)
              (func (;0;) (type 0))
              (elem (;0;) (i32.const 0) func 0)
              (data (;0;) (i32.const 0) "ab")
              (export "f" (func 0)))
            "#,
        );
        assert_eq!(module.tables.len(), 1, "one table");
        assert_eq!(module.element_count, 1, "one element segment");
        assert_eq!(module.data_count, 1, "one data segment");
        assert!(module.memory.is_some(), "memory captured");
    }

    #[test]
    fn func_sig_reads_imported_and_local_function_types() {
        // `func_sig` must follow the index space: function 0 is the import, 1 is
        // the local — and both share the same type.
        let module = parse(
            r#"
            (module
              (type (;0;) (func (param i32) (result i32)))
              (import "env" "ext" (func (;0;) (type 0)))
              (func (;1;) (type 0) (param i32) (result i32)
                local.get 0)
              (export "f" (func 1)))
            "#,
        );
        let import_sig = module.func_sig(0).expect("imported function has a type");
        let local_sig = module.func_sig(1).expect("local function has a type");
        assert_eq!(import_sig.params, vec![ValType::I32]);
        assert_eq!(import_sig.results, vec![ValType::I32]);
        assert_eq!(import_sig, local_sig, "import and local share one type here");
        assert!(
            module.func_sig(99).is_none(),
            "an out-of-range function index has no signature"
        );
    }

    #[test]
    fn captures_function_names_from_the_name_section() {
        // The `name` custom section's function subsection must be mined so merged
        // and main functions keep sane names for the Rocq translator.
        let module = parse(
            r#"
            (module
              (type (;0;) (func))
              (func $entry (;0;) (type 0))
              (export "f" (func 0)))
            "#,
        );
        assert_eq!(
            module.func_name(0),
            Some("entry"),
            "the $entry debug name must be captured"
        );
        assert_eq!(module.func_name(1), None, "no name for an unnamed index");
    }

    #[test]
    fn exported_func_index_ignores_non_function_exports() {
        // A memory export named `shared` must not be mistaken for a function of
        // that name.
        let module = parse(
            r#"
            (module
              (type (;0;) (func))
              (memory (;0;) 1)
              (func (;0;) (type 0))
              (export "shared" (memory 0))
              (export "run" (func 0)))
            "#,
        );
        assert_eq!(
            module.exported_func_index("shared"),
            None,
            "a memory export is not a function export"
        );
        assert_eq!(module.exported_func_index("run"), Some(0));
    }

    #[test]
    fn rejects_invalid_bytes() {
        let err = ParsedModule::parse(b"definitely not wasm").unwrap_err();
        assert!(matches!(err, LinkError::Parse(_)), "got {err:?}");
    }

    #[test]
    fn captures_the_start_function_index() {
        // The start section must be captured so the merge can reject modules that
        // declare one (their initialization closure is never folded in).
        let module = parse(
            r#"
            (module
              (type (;0;) (func))
              (func (;0;) (type 0))
              (func (;1;) (type 0))
              (start 1)
              (export "f" (func 0)))
            "#,
        );
        assert_eq!(module.start, Some(1), "the start function index is captured");
    }

    #[test]
    fn no_start_section_leaves_start_none() {
        let module = parse(
            r#"
            (module
              (type (;0;) (func))
              (func (;0;) (type 0))
              (export "f" (func 0)))
            "#,
        );
        assert_eq!(module.start, None, "no start section means no start index");
    }

    #[test]
    fn code_bodies_are_assigned_in_function_order() {
        // The running-cursor assignment must place each body on its own function
        // in declaration order, so a function's `type_idx` and `body` agree.
        let module = parse(
            r#"
            (module
              (type (;0;) (func (result i32)))
              (type (;1;) (func (result i64)))
              (func (;0;) (type 0) (result i32) i32.const 1)
              (func (;1;) (type 1) (result i64) i64.const 2)
              (export "a" (func 0))
              (export "b" (func 1)))
            "#,
        );
        assert_eq!(module.local_funcs.len(), 2);
        assert_eq!(module.local_funcs[0].type_idx, 0, "first body -> function 0");
        assert_eq!(module.local_funcs[1].type_idx, 1, "second body -> function 1");
        assert!(
            module.local_funcs.iter().all(|f| !f.body.is_empty()),
            "every function received a body"
        );
    }

    /// A minimal one-function module carrying an `inference.spec_funcs` custom
    /// section with the given payload.
    fn module_with_spec_section(payload: &[u8]) -> Vec<u8> {
        use wasm_encoder::{
            CodeSection, CustomSection, ExportKind, ExportSection, Function, FunctionSection, Module,
            TypeSection,
        };
        let mut module = Module::new();
        let mut types = TypeSection::new();
        types.ty().function([], []);
        module.section(&types);
        let mut funcs = FunctionSection::new();
        funcs.function(0);
        module.section(&funcs);
        let mut exports = ExportSection::new();
        exports.export("f", ExportKind::Func, 0);
        module.section(&exports);
        let mut code = CodeSection::new();
        let mut body = Function::new([]);
        body.instruction(&wasm_encoder::Instruction::End);
        code.function(&body);
        module.section(&code);
        module.section(&CustomSection {
            name: crate::spec_funcs::SECTION_NAME.into(),
            data: payload.into(),
        });
        module.finish()
    }

    #[test]
    fn main_decodes_the_spec_section_external_skips_it() {
        // The main module's spec section is a verification deliverable: decode it.
        // A non-adopting external's describes a module the output is not: skip it
        // so the external never even materialises a `spec_funcs` field.
        // version=1, count=1, name_len=1 'S', idx_count=1, idx=0.
        let payload = [1u8, 1, 1, b'S', 1, 0];
        let bytes = module_with_spec_section(&payload);

        let main = ParsedModule::parse(&bytes).expect("main parse");
        assert_eq!(
            main.spec_funcs,
            Some(vec![("S".to_string(), vec![0])]),
            "the main module must decode its spec section"
        );

        let external = ParsedModule::parse_external(&bytes, "lib", false).expect("external parse");
        assert_eq!(
            external.spec_funcs, None,
            "a non-adopting external's spec section must be skipped, not decoded"
        );
    }

    #[test]
    fn a_malformed_spec_section_fails_main_but_not_external() {
        // A malformed spec section (version byte 0xff) is a hard error for the
        // main module (a verification deliverable), but for an external nothing
        // reads it must not fail the parse at all.
        let bytes = module_with_spec_section(&[0xffu8, 0xff, 0xff]);

        assert!(
            matches!(ParsedModule::parse(&bytes), Err(LinkError::Parse(_))),
            "a malformed main spec section must be a hard parse error"
        );
        assert!(
            ParsedModule::parse_external(&bytes, "lib", false).is_ok(),
            "a malformed external spec section must not fail the parse"
        );
    }

    /// A minimal one-function module carrying both verification custom sections
    /// with the given payloads, in the order the encoder writes them.
    fn module_with_verification_sections(spec_funcs: &[u8], hspecs: &[u8]) -> Vec<u8> {
        use wasm_encoder::{CustomSection, Section as _};
        let mut bytes = module_with_spec_section(spec_funcs);
        CustomSection {
            name: inference_hassert::HSPECS_SECTION_NAME.into(),
            data: hspecs.into(),
        }
        .append_to(&mut bytes);
        bytes
    }

    /// One universal obligation under spec `S`, owned by and applying `f`.
    fn one_obligation() -> inference_hassert::HSpecMap {
        use inference_hassert::{HAssert, HFnRef, HSpecEntry, HSpecMap, HTerm, SpecKind};
        let mut map = HSpecMap::default();
        map.insert(
            "S".to_string(),
            vec![HSpecEntry::new(
                HFnRef("f".to_string()),
                HAssert::nz(HTerm::App(HFnRef("f".to_string()), vec![])),
                SpecKind::Forall,
            )],
        );
        map
    }

    /// A report about a library's dropped obligations keys on the presence of
    /// its `inference.hspecs` section, and the whole point of the non-adopting
    /// path is that presence is learned without reading the payload.
    #[test]
    fn an_external_records_its_hspecs_presence_without_decoding() {
        let spec_funcs = [1u8, 1, 1, b'S', 0];
        let payload = inference_hassert::encode(&one_obligation());
        let bytes = module_with_verification_sections(&spec_funcs, &payload);

        let external = ParsedModule::parse_external(&bytes, "lib", false).expect("external parse");
        assert!(
            external.carries_hspecs,
            "the presence of an hspecs section must be recorded under every policy"
        );
        assert_eq!(
            external.hspecs, None,
            "a non-adopting parse must not decode the section it merely noted"
        );
    }

    /// The floor the default path stands on: a corrupt verification section in a
    /// library nothing needed cannot fail a link, because nothing reads it.
    #[test]
    fn a_malformed_external_hspecs_section_still_parses_when_not_adopting() {
        // Version byte 3 is past the codec's supported version, so a decoder
        // would reject these bytes outright.
        let bytes = module_with_verification_sections(&[1u8, 0], &[3u8, 0, 0]);

        let external = ParsedModule::parse_external(&bytes, "lib", false)
            .expect("a malformed external hspecs section must not fail a non-adopting parse");
        assert!(external.carries_hspecs);
        assert_eq!(external.hspecs, None);
    }

    /// Adoption needs both sections: the obligations to carry, and the
    /// specification list to check the library's own subset invariant against
    /// before carrying anything.
    #[test]
    fn an_adopting_parse_decodes_both_external_verification_sections() {
        let spec_funcs = [1u8, 1, 1, b'S', 0];
        let map = one_obligation();
        let payload = inference_hassert::encode(&map);
        let bytes = module_with_verification_sections(&spec_funcs, &payload);

        let external = ParsedModule::parse_external(&bytes, "lib", true).expect("external parse");
        assert_eq!(
            external.spec_funcs,
            Some(vec![("S".to_string(), vec![])]),
            "an adopting parse must decode the external's spec_funcs section"
        );
        assert_eq!(
            external.hspecs,
            Some(map),
            "an adopting parse must decode the external's hspecs section"
        );
        assert!(external.carries_hspecs);
    }

    /// A rejection raised while adopting has to name the library it came from:
    /// the section name alone would leave the reader searching every dependency
    /// for a payload the merge does not otherwise report on.
    #[test]
    fn an_adopting_parse_names_the_library_in_a_section_rejection() {
        let bytes = module_with_verification_sections(&[1u8, 0], &[3u8, 0, 0]);

        let err = ParsedModule::parse_external(&bytes, "crypto::digest", true)
            .expect_err("an adopting parse must reject a malformed hspecs section");
        let LinkError::Parse(message) = &err else {
            panic!("expected a Parse rejection, got {err:?}");
        };
        assert!(
            message.contains("crypto::digest") && message.contains("inference.hspecs"),
            "the rejection must name the library and the section, got {message}"
        );
    }
}
