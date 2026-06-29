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
/// dependency merged into it. Controls whether the `inference.spec_funcs`
/// custom section is decoded: the main module's drives proof-mode translation
/// and is re-emitted, while an external's is verification-only scaffolding that
/// the merge strips, so it is skipped (and never fails the link if malformed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModuleRole {
    Main,
    External,
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
    /// [(func_idx, kind_byte)]` in the *pre-link* index space. The merge
    /// rewrites each index into the output space (carrying the obligation-kind
    /// byte through verbatim) and re-emits the section, so a bare linked `.wasm`
    /// still names the correct spec functions with their kinds (the input to
    /// formal verification). Only the main module carries one; externals never do.
    pub spec_funcs: Option<Vec<crate::spec_funcs::SpecEntry>>,
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
    /// as the logical name the module was bound under (empty for the main
    /// module). The merge uses it to disambiguate two externals that export the
    /// same field but were bound from different logical modules.
    ///
    /// An external module's `inference.spec_funcs` custom section — and any spec
    /// functions it names — are *not* merged into the executable output: only
    /// the executable closure of the satisfied export crosses the merge. So the
    /// section is skipped here rather than decoded, and a malformed one in an
    /// external never fails the link (the section is irrelevant to the merge).
    pub(crate) fn parse_external(bytes: &[u8], logical_module: &str) -> Result<Self, LinkError> {
        let mut module = Self::parse_with_role(bytes, ModuleRole::External)?;
        module.logical_module = logical_module.to_string();
        Ok(module)
    }

    /// Parses the main module's `bytes`, decoding its `inference.spec_funcs`
    /// section (a verification deliverable the merge re-emits, re-indexed).
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, LinkError> {
        Self::parse_with_role(bytes, ModuleRole::Main)
    }

    /// Parses `bytes` into the owned representation under the given `role`, which
    /// decides whether the `inference.spec_funcs` custom section is decoded (main
    /// module) or skipped (external module).
    fn parse_with_role(bytes: &[u8], role: ModuleRole) -> Result<Self, LinkError> {
        let mut module = ParsedModule::default();

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
/// `name` section's module/function/local subsections, and (for the main module
/// only) the `inference.spec_funcs` section that drives proof-mode translation.
///
/// The `name` subsections are best-effort (an unparseable one is skipped). The
/// main module's `inference.spec_funcs` payload, by contrast, is a verification
/// deliverable: a malformed one is a hard [`LinkError`], never silently dropped.
/// An external module's spec section is verification-only scaffolding the merge
/// strips, so it is skipped here without decoding — its presence never fails the
/// link, and a malformed one in an irrelevant external cannot block the merge.
fn collect_custom_section(
    custom: &CustomSectionReader,
    module: &mut ParsedModule,
    role: ModuleRole,
) -> Result<(), LinkError> {
    if custom.name() == crate::spec_funcs::SECTION_NAME {
        if role == ModuleRole::External {
            return Ok(());
        }
        // A second spec_funcs section would silently discard the first under a
        // last-wins assignment, dropping its proof obligations. Since the section
        // is a verification deliverable, reject the duplicate with a clean error
        // rather than vanish the earlier obligations.
        if module.spec_funcs.is_some() {
            return Err(LinkError::Parse(
                "main module declares more than one inference.spec_funcs section; \
                 its proof obligations would be silently dropped"
                    .into(),
            ));
        }
        let decoded = crate::spec_funcs::decode(custom.data())?;
        module.spec_funcs = Some(decoded);
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
        // An external's is verification-only scaffolding the merge strips: skip
        // it so the external never even materialises a `spec_funcs` field.
        // version=1, count=1, name_len=1 'S', idx_count=1, idx=0.
        let payload = [1u8, 1, 1, b'S', 1, 0];
        let bytes = module_with_spec_section(&payload);

        let main = ParsedModule::parse(&bytes).expect("main parse");
        assert_eq!(
            main.spec_funcs,
            // A v1 payload decodes with a default kind byte of 0 per index.
            Some(vec![("S".to_string(), vec![(0u32, 0u8)])]),
            "the main module must decode its spec section"
        );

        let external = ParsedModule::parse_external(&bytes, "lib").expect("external parse");
        assert_eq!(
            external.spec_funcs, None,
            "an external's spec section must be skipped, not decoded"
        );
    }

    #[test]
    fn a_malformed_spec_section_fails_main_but_not_external() {
        // A malformed spec section (version byte 0xff) is a hard error for the
        // main module (a verification deliverable), but for an external — which
        // strips it — it must not fail the parse at all.
        let bytes = module_with_spec_section(&[0xffu8, 0xff, 0xff]);

        assert!(
            matches!(ParsedModule::parse(&bytes), Err(LinkError::Parse(_))),
            "a malformed main spec section must be a hard parse error"
        );
        assert!(
            ParsedModule::parse_external(&bytes, "lib").is_ok(),
            "a malformed external spec section must not fail the parse"
        );
    }
}
