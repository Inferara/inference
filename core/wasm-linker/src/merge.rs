//! The static-merge pass: fold satisfied imports' closures into the main
//! module and rebuild a single self-contained module.
//!
//! ## Index spaces
//!
//! The merge defines one new function index space for the output. Every import
//! must be satisfied (an unsatisfiable one is a hard [`LinkError::UnsatisfiedImport`]
//! — the merge is fail-closed and never carries a surviving import), so the
//! output has no import section and:
//!
//! 1. The main module's local functions occupy the lowest indices, starting at
//!    0 (every satisfied import is removed, so there is no import block above
//!    them).
//! 2. Each merged external function is appended after the main locals.
//!
//! Every `call`, `ref.func`, and `call_indirect` type index inside a copied
//! body is rewritten through [`crate::rewrite`] to land in this space. The main
//! module's own bodies are re-encoded too, because removing imports shifts
//! their local-function indices and redirects their calls to satisfied imports
//! onto the merged bodies.

use std::cell::RefCell;
use std::collections::BTreeMap;

use inf_wasmparser::ExternalKind;
use wasm_encoder::{
    CodeSection, ConstExpr, ExportKind, ExportSection, Function, FunctionSection, GlobalSection,
    GlobalType as EncGlobalType, MemorySection, MemoryType as EncMemoryType, Module, NameMap,
    NameSection, TypeSection, ValType as EncValType,
};

use crate::closure;
use crate::parse::{FuncSig, GlobalDef, GlobalInit, ParsedModule, TypeEntry};
use crate::rewrite::{reencode_body, BodyOrigin, IndexMap};
use crate::tier::{self, Tier};
use crate::LinkError;

/// Resolves and merges every satisfiable import of `main` from the supplied
/// external modules, returning the unified module bytes.
///
/// Each external arrives as `(logical_module, bytes)` so the merge can match an
/// import's recorded `(module, field)` against the external's logical module.
pub(crate) fn link(
    main_bytes: &[u8],
    externals: &[(&str, &[u8])],
) -> Result<Vec<u8>, LinkError> {
    // Structural validation of the main module on entry. The main module is the
    // linker's own codegen output on the live CLI pipeline, but the public
    // library API (`inference_wasm_linker::link`, `inference::link`) accepts
    // arbitrary `main_bytes`, so this entry point must never panic on a hostile
    // main. Without this gate a main whose FunctionSection names an out-of-range
    // type index, or any other structural corruption, would reach a raw
    // main-derived slice index in `emit`/the re-encoder and abort *before* the
    // post-merge gate ever runs. Validating structurally here (under the parser's
    // default features, the same validation the post-merge gate applies to the
    // merged module — which embeds these same main bodies — so no legitimate
    // proof-mode main is regressed) turns that panic into a clean `Parse` error.
    inf_wasmparser::validate(main_bytes)
        .map_err(|e| LinkError::Parse(format!("main module is invalid WASM: {e}")))?;

    // Two-pass entry gate over every external, before any closure or provenance
    // work touches its bytes. The CLI driver validates each resolved external
    // (`wasm_link/driver.rs`) before it reaches this crate, but the public
    // library API is an entry point in its own right whose contract previously
    // only *assumed* pre-validated input. Validating here makes that backstop
    // universal: a structurally-invalid or adversarially-crafted external (e.g.
    // an over-declared locals count) is rejected as a clean `Parse`, and a
    // well-formed but post-1.0 external is rejected up front with a feature-named
    // `UnsupportedWasmFeature` rather than late, when a specific unmodeled opcode
    // happens to reach the merge.
    for (logical_module, bytes) in externals {
        validate_external(logical_module, bytes)?;
    }

    let main = ParsedModule::parse(main_bytes)?;
    let externals = externals
        .iter()
        .map(|(logical_module, bytes)| ParsedModule::parse_external(bytes, logical_module))
        .collect::<Result<Vec<_>, _>>()?;

    let plan = Plan::build(&main, &externals)?;
    let merged = plan.emit(&main, &externals)?;

    // Post-merge validation gate. The effect scanner is an allow-list and can
    // never be proven complete against an adversarial external `.wasm`; this
    // final check ensures the merge never persists a structurally-invalid
    // artifact (the input to formal verification), converting every effect-
    // scanner gap into a clean diagnostic instead of a silent miscompile.
    inf_wasmparser::validate(&merged)
        .map_err(|e| LinkError::InvalidMergedModule(e.to_string()))?;

    Ok(merged)
}

/// Validates one external against the linker's supported-version contract in two
/// passes, so the diagnostic distinguishes a malformed module from a well-formed
/// but unsupported one.
///
/// 1. **Structural** pass under the parser's default features: a failure here is
///    genuinely malformed or adversarial bytes, surfaced as
///    [`LinkError::Parse`]. This keeps the prior universal pre-validation
///    behavior (a structurally-invalid external is rejected before the permissive
///    `parse_external` reader or the provenance interpreter sees it).
/// 2. **Feature** pass under [`crate::SUPPORTED_WASM_FEATURES`]: a failure here
///    means the module is valid WebAssembly but uses a proposal beyond the
///    supported WASM 1.0 subset, surfaced as
///    [`LinkError::UnsupportedWasmFeature`] with the validator's feature-named
///    message.
///
/// Running structural-first is deliberate: a malformed module reported by the
/// restricted-feature pass alone could mask the real defect behind a feature
/// name, so the broad pass classifies malformedness first and the narrow pass
/// classifies version.
pub(crate) fn validate_external(logical_module: &str, bytes: &[u8]) -> Result<(), LinkError> {
    inf_wasmparser::validate(bytes).map_err(|e| {
        LinkError::Parse(format!("external module `{logical_module}` is invalid WASM: {e}"))
    })?;

    inf_wasmparser::Validator::new_with_features(crate::SUPPORTED_WASM_FEATURES)
        .validate_all(bytes)
        .map_err(|e| LinkError::UnsupportedWasmFeature {
            module: logical_module.to_string(),
            details: e.to_string(),
        })?;

    Ok(())
}

/// One merged external function, ready to be appended to the output.
struct MergedFunc {
    /// Index of the source external module within the `externals` slice.
    external_idx: usize,
    /// The function's index within that external module.
    source_func_idx: u32,
    /// The function's type index within the *output* type section.
    out_type_idx: u32,
    /// The name to record for this function in the output `name` section, so
    /// the Rocq translator emits a `Definition <name>` rather than an opaque
    /// `func_<uuid>`. A closure root takes the satisfied import field; an inner
    /// callee keeps its own debug name when the source module carried one.
    name: Option<String>,
}

/// The fully-resolved merge plan: which imports are satisfied, the output type
/// table, and the output index of every function.
struct Plan {
    /// Output type section: the main module's types followed by the deduped
    /// external function types pulled in by closures.
    out_types: Vec<FuncSig>,
    /// For each main-module type index, its index in `out_types`.
    main_type_remap: Vec<u32>,
    /// `satisfied main import index -> output function index of its body`.
    import_target: BTreeMap<u32, u32>,
    /// Output function index of the first main local function.
    main_local_base: u32,
    /// The merged external functions, in output order (appended after main
    /// locals).
    merged: Vec<MergedFunc>,
    /// `(external_idx, source_func_idx) -> output function index`.
    merged_index: BTreeMap<(usize, u32), u32>,
    /// Per external module: `source_type_idx -> output type idx` for the types
    /// its merged closure references.
    external_type_remap: Vec<BTreeMap<u32, u32>>,
    /// The single shared linear memory the output declares, reconciled across
    /// the main module and every memory-using merged external. `None` when no
    /// module needs a memory (a fully pure merge).
    reconciled_memory: Option<EncMemoryType>,
}

impl Plan {
    fn build(main: &ParsedModule, externals: &[ParsedModule]) -> Result<Self, LinkError> {
        // 0. Reject a main module that carries its own data or element segments.
        //    `emit` rebuilds the main module section-by-section and emits no
        //    `DataSection`/`ElementSection`, so a main-side data segment would be
        //    silently dropped (its memory initializer lost — a valid-but-wrong
        //    `.wasm`/`.v`) and a main-side element segment would survive as an
        //    orphaned table reference. Until full preservation-and-reindexing of
        //    these sections exists, reject up front with a clean diagnostic,
        //    mirroring the external-side Tier-C reasons. Today Inference codegen
        //    emits neither section, so this guards the public library API rather
        //    than the live CLI pipeline.
        if main.data_count > 0 {
            return Err(LinkError::UnsupportedConstruct(format!(
                "main module declares {} data segment(s); the static merge does not yet \
                 preserve and re-index main-side data segments",
                main.data_count
            )));
        }
        if main.element_count > 0 {
            return Err(LinkError::UnsupportedConstruct(format!(
                "main module declares {} element segment(s); the static merge does not yet \
                 preserve and re-index main-side element segments",
                main.element_count
            )));
        }
        // A main-side start function runs side-effecting initialization that
        // `emit` rebuilds no `StartSection` for — so it would be silently dropped,
        // losing its initializer effects in a valid-but-wrong `.wasm`/`.v`. Reject
        // it up front, mirroring the external-side start guard. Inference codegen
        // emits no start section, so this guards the public library API.
        if main.start.is_some() {
            return Err(LinkError::UnsupportedConstruct(
                "main module declares a start function; the static merge does not \
                 preserve the start section"
                    .into(),
            ));
        }
        // `emit` writes no import section: every function import is satisfied and
        // removed, and the merge models *function* imports only. A main-side
        // non-function import (global/memory/table) would be silently dropped, and
        // a body's `global.get`/etc. would then rebind to the first *defined*
        // entity — a wrong value in a valid-but-wrong output, with no diagnostic.
        // Reject it up front.
        if main.non_func_imports > 0 {
            return Err(LinkError::UnsupportedConstruct(format!(
                "main module imports {} non-function (global/memory/table) entit{} from its \
                 environment; the static merge models function imports only",
                main.non_func_imports,
                if main.non_func_imports == 1 { "y" } else { "ies" }
            )));
        }
        // `emit` writes no `TableSection`, so a main-side table is silently
        // dropped; a surviving `call_indirect`/`table.*` then fails *after* the
        // merge as `InvalidMergedModule("unknown table 0")`, blaming the linker's
        // own output rather than naming the unsupported construct. Reject the
        // table section up front so the diagnostic names the real cause.
        if !main.tables.is_empty() {
            return Err(LinkError::UnsupportedConstruct(format!(
                "main module declares {} table(s); the static merge does not preserve tables",
                main.tables.len()
            )));
        }
        // The output declares a single shared linear memory. The parser keeps only
        // the first declared memory, so a second main-side memory would be silently
        // dropped and a body's memarg over it would rebind to memory 0 — a
        // valid-but-wrong output. Reject up front, mirroring the external-side
        // multi-memory guard below.
        if main.memory_count > 1 {
            return Err(LinkError::UnsupportedConstruct(format!(
                "main module declares {} memories; the static merge models a single shared memory",
                main.memory_count
            )));
        }

        // 1. Seed the output type table with the main module's function types,
        //    recording where each main type index lands.
        let mut out_types: Vec<FuncSig> = Vec::new();
        let mut sig_to_out: BTreeMap<Vec<u8>, u32> = BTreeMap::new();
        let mut main_type_remap = vec![0u32; main.types.len()];
        for (i, entry) in main.types.iter().enumerate() {
            if let TypeEntry::Func(sig) = entry {
                let out_idx = intern_sig(&mut out_types, &mut sig_to_out, sig)?;
                main_type_remap[i] = out_idx;
            }
        }

        // 2. Resolve each satisfied import to an external export and close over
        //    it. An import is satisfiable when some external module exports a
        //    function of the import's field name; the module name is the
        //    logical module the front-end bound, but the merge keys on the
        //    field, matching the codegen import contract.
        let main_import_count = main.imported_funcs.len() as u32;
        let mut import_target = BTreeMap::new();
        let mut merged: Vec<MergedFunc> = Vec::new();
        let mut merged_index: BTreeMap<(usize, u32), u32> = BTreeMap::new();
        let mut external_type_remap: Vec<BTreeMap<u32, u32>> =
            externals.iter().map(|_| BTreeMap::new()).collect();

        // Every import of the main module must be satisfiable: the driver
        // resolves all extern bindings before linking, so an unsatisfied import
        // is a real error rather than a survivor to keep. Resolving them all up
        // front also lets every main local function start at index 0.
        let mut satisfied: Vec<(usize, u32)> = Vec::with_capacity(main_import_count as usize);
        for import in &main.imported_funcs {
            let Some((ext_idx, root)) =
                find_export(externals, &import.module, &import.field)?
            else {
                return Err(LinkError::UnsatisfiedImport {
                    field: import.field.clone(),
                });
            };
            satisfied.push((ext_idx, root));
        }

        // With every import removed, main locals occupy indices `0..`, and
        // merged functions follow them.
        let main_local_base = 0u32;
        let mut next_output_idx = main.local_funcs.len() as u32;

        // The output declares one shared linear memory, reconciled across the
        // main module and every memory-using external. Seed it with the main
        // module's memory (if any); each satisfied external folds its memory and
        // memory-effect requirements in below.
        let mut memory = MemoryReconciler::new(main.memory.as_ref())?;

        // 3. For every satisfied import, compute its closure, classify the tier,
        //    and allocate output indices + output types for the whole closure.
        for (import_idx, &(ext_idx, root)) in satisfied.iter().enumerate() {
            let external = &externals[ext_idx];

            if external.non_func_imports > 0 {
                return Err(LinkError::UnsupportedConstruct(format!(
                    "external module providing `{}` imports its environment",
                    main.imported_funcs[import_idx].field
                )));
            }

            // A start function runs side-effecting initialization (e.g.
            // `__wasm_call_ctors`) whose closure the merge never folds in. Were
            // it silently dropped, those effects would vanish and a host import
            // reachable only via the start function would bypass the
            // `TransitiveHostImport` gate. Reject rather than miscompile.
            if external.start.is_some() {
                return Err(LinkError::UnsupportedConstruct(format!(
                    "external module providing `{}` declares a start function, which the static merge cannot run",
                    main.imported_funcs[import_idx].field
                )));
            }

            // The output has a single shared linear memory. An external with
            // more than one memory would carry memargs naming memories the
            // output lacks; keeping only the first (the prior behavior) silently
            // miscompiled. Reject the whole module.
            if external.memory_count > 1 {
                return Err(LinkError::UnsupportedConstruct(format!(
                    "external module providing `{}` declares {} memories; the static merge supports a single shared memory",
                    main.imported_funcs[import_idx].field, external.memory_count
                )));
            }

            let cl = closure::compute(external, root)?;
            // Tier C is rejected here, before any output index is committed. The
            // classifier runs the address-provenance analysis for memory-using
            // closures, so an absolute-address access is rejected as Tier C.
            let _tier: Tier =
                tier::classify(external, &cl, root, &main.imported_funcs[import_idx].field)?;

            // Reconcile this external's memory into the shared output memory:
            // fold in its declared limits (widening minimum/maximum) and check
            // its memory effects against the reconciled result. This folds an
            // external memory onto a memoryless main (H24), keeps the merged
            // minimum large enough for every module's static range (H15), and
            // rejects growth the reconciled maximum cannot satisfy. Incompatible
            // fundamental shapes (`memory64`/`shared`/page size) are rejected.
            memory.fold(
                external.memory.as_ref(),
                cl.effects.uses_memory,
                cl.effects.uses_memory_grow,
                &main.imported_funcs[import_idx].field,
            )?;

            for &src_func in &cl.local_func_indices {
                let key = (ext_idx, src_func);
                if merged_index.contains_key(&key) {
                    continue;
                }
                // Allocate the output type for this function (deduped).
                let sig = external
                    .func_sig(src_func)
                    .ok_or_else(|| LinkError::Parse(format!(
                        "external function {src_func} has no function type"
                    )))?
                    .clone();
                let out_type_idx = intern_sig(&mut out_types, &mut sig_to_out, &sig)?;
                let local = external
                    .local_funcs
                    .get((src_func - external.local_func_base()) as usize)
                    .ok_or_else(|| {
                        LinkError::Parse(format!(
                            "external function index {src_func} is out of range"
                        ))
                    })?;
                let src_type_idx = local.type_idx;
                external_type_remap[ext_idx].insert(src_type_idx, out_type_idx);

                // Make the type remap total: a body's function-typed blocks and
                // indirect calls reference type indices other than the
                // function's own, which the re-encoder must remap. Intern each
                // referenced signature now so re-encoding never hits an unmapped
                // index (the prior `.expect()` panic, H2) — and an out-of-range
                // source type index surfaces as a clean parse error.
                for type_idx in scan_body_type_indices(&local.body)? {
                    if external_type_remap[ext_idx].contains_key(&type_idx) {
                        continue;
                    }
                    let referenced = match external.types.get(type_idx as usize) {
                        Some(TypeEntry::Func(s)) => s.clone(),
                        _ => {
                            return Err(LinkError::Parse(format!(
                                "merged body references type index {type_idx}, which is not a function type"
                            )));
                        }
                    };
                    let out_idx = intern_sig(&mut out_types, &mut sig_to_out, &referenced)?;
                    external_type_remap[ext_idx].insert(type_idx, out_idx);
                }

                let out_func_idx = next_output_idx;
                next_output_idx += 1;
                merged_index.insert(key, out_func_idx);
                // Prefix the merged inner callee's debug name with its logical
                // module (`mathlib.helper`). Two externals bound under different
                // logical modules may export — and internally call — functions of
                // the same name; without the prefix those names would collide in
                // the output name section and force wasm-to-v's index-suffix
                // disambiguation (`helper` vs `helper_2`), which is index-
                // dependent and shifts across merges. The prefix keeps each merged
                // function traceable to its source module and makes the *wasm-level*
                // names distinct. It is not a hard collision guarantee at the Rocq
                // level: wasm-to-v sanitizes `.` (and other non-identifier bytes)
                // to `_`, so two distinct sources can still sanitize to the same
                // Rocq identifier (e.g. via `__` runs); wasm-to-v's index suffix
                // remains the final disambiguator. The `.` separator matches
                // Inference's `Type.method` convention.
                merged.push(MergedFunc {
                    external_idx: ext_idx,
                    source_func_idx: src_func,
                    out_type_idx,
                    name: external
                        .func_name(src_func)
                        .map(|name| format!("{}.{name}", external.logical_module)),
                });
            }

            let root_output = merged_index[&(ext_idx, root)];
            import_target.insert(import_idx as u32, root_output);

            // The closure root satisfies this import: name it after the import
            // field, prefixed with the external's logical module
            // (`mathlib.sum`), so the merged function reads as an ordinary, named
            // definition that is traceable to its source module. The field alone
            // is not unique: two externals bound under different logical modules
            // may satisfy imports of the same field, and their roots would then
            // collide in the output name section, forcing wasm-to-v's index-
            // suffix disambiguation (`sum` vs `sum_2`), which is index-dependent
            // across merges. The module prefix makes the wasm-level names distinct;
            // it is not a hard Rocq-level collision guarantee, since wasm-to-v
            // sanitizes `.` to `_` and two distinct sources can still sanitize to
            // the same Rocq identifier (`__` runs), with wasm-to-v's index suffix
            // as the final disambiguator. The `.` separator matches Inference's
            // `Type.method` convention. An explicit debug name on the source module
            // would otherwise win, but a codegen-produced external typically
            // exports the field under that same name, so this is stable.
            let external = &externals[ext_idx];
            let field = &main.imported_funcs[import_idx].field;
            if let Some(root_merged) = merged.iter_mut().find(|m| {
                merged_index.get(&(m.external_idx, m.source_func_idx)) == Some(&root_output)
            }) {
                root_merged.name = Some(format!("{}.{field}", external.logical_module));
            }
        }

        // Give every still-nameless merged inner callee a name derived from its
        // output function index, prefixed with its logical module
        // (`lib.func_5`). An external stripped of its `name` section (third-party
        // / `wasm-tools`-stripped) leaves inner callees with `name: None`;
        // without a name `build_func_names` emits no name-section entry, and
        // `wasm-to-v` then falls back to a per-process random UUID `Definition`
        // name, making the `.v` non-reproducible for byte-identical input. Naming
        // each from its deterministic output index keeps the name section
        // complete and the proof artifact reproducible. The module prefix keeps
        // the synthesized name in the same `module.field` namespace as the named
        // roots and callees above, so two stripped externals can never produce
        // the same fallback name for distinct functions. The `.` separator
        // matches Inference's `Type.method` convention and sanitizes to `_` in
        // the Rocq name.
        let merged_base = main_local_base + main.local_funcs.len() as u32;
        for (i, m) in merged.iter_mut().enumerate() {
            if m.name.is_none() {
                let logical_module = &externals[m.external_idx].logical_module;
                m.name = Some(format!("{}.func_{}", logical_module, merged_base + i as u32));
            }
        }

        Ok(Plan {
            out_types,
            main_type_remap,
            import_target,
            main_local_base,
            merged,
            merged_index,
            external_type_remap,
            reconciled_memory: memory.finish(),
        })
    }

    /// Maps a main-module function index into the output index space.
    ///
    /// Every import is satisfied and removed, so an import index maps to its
    /// merged body's output index, and a main local shifts down by the
    /// (now fully removed) import count onto `main_local_base`.
    ///
    /// The local index is bounds-checked against the main module's local
    /// function count. Most callers feed indices the parser already validated
    /// (a body's `call` targets, an export), but `remap_spec_funcs` feeds indices
    /// straight from the `inference.spec_funcs` custom section — which the
    /// post-merge `inf_wasmparser::validate` treats as opaque, so a garbage or
    /// out-of-range spec index would otherwise be silently remapped onto the
    /// wrong or a nonexistent function and emitted into the Rocq proof obligation.
    /// Rejecting an out-of-range local here keeps that verification deliverable
    /// honest.
    fn map_main_func(&self, main: &ParsedModule, idx: u32) -> Result<u32, LinkError> {
        let import_count = main.imported_funcs.len() as u32;
        if idx < import_count {
            return self.import_target.get(&idx).copied().ok_or_else(|| {
                LinkError::Parse(format!(
                    "main function index {idx} references an unsatisfied import"
                ))
            });
        }
        let local_idx = idx - import_count;
        if local_idx as usize >= main.local_funcs.len() {
            return Err(LinkError::Parse(format!(
                "function index {idx} out of range"
            )));
        }
        Ok(self.main_local_base + local_idx)
    }

    /// Emits the unified module bytes.
    fn emit(
        &self,
        main: &ParsedModule,
        externals: &[ParsedModule],
    ) -> Result<Vec<u8>, LinkError> {
        let mut module = Module::new();

        // Type section.
        let mut types = TypeSection::new();
        for sig in &self.out_types {
            let params = sig
                .params
                .iter()
                .map(map_val_type)
                .collect::<Result<Vec<_>, _>>()?;
            let results = sig
                .results
                .iter()
                .map(map_val_type)
                .collect::<Result<Vec<_>, _>>()?;
            types.ty().function(params, results);
        }
        module.section(&types);

        // No import section: every import is satisfied and removed. The merge is
        // fail-closed (an unsatisfiable import is rejected in `Plan::build` as
        // `UnsatisfiedImport`), so no import can survive to be re-emitted here.

        // Function section: main locals (remapped types) then merged functions.
        let mut functions = FunctionSection::new();
        for local in &main.local_funcs {
            // Checked lookup, mirroring the `reencode_main_body` `ty` closure: a
            // main FunctionSection naming an out-of-range type index must surface
            // as a clean error rather than panic on a raw slice index (S3). The
            // entry-side structural validation already rejects such a main, but
            // this keeps the index access self-defending in its own right.
            let out_type = self
                .main_type_remap
                .get(local.type_idx as usize)
                .copied()
                .ok_or_else(|| {
                    LinkError::Parse(format!(
                        "main function references type index {} out of range",
                        local.type_idx
                    ))
                })?;
            functions.function(out_type);
        }
        for m in &self.merged {
            functions.function(m.out_type_idx);
        }
        module.section(&functions);

        // Memory section: the single shared linear memory reconciled across the
        // main module and every memory-using merged external.
        if let Some(mem) = &self.reconciled_memory {
            let mut memory = MemorySection::new();
            memory.memory(*mem);
            module.section(&memory);
        }

        // Global section (main globals only; external globals are Tier C).
        if !main.globals.is_empty() {
            let mut globals = GlobalSection::new();
            for g in &main.globals {
                globals.global(map_global_type(g)?, &map_global_init(g.init));
            }
            module.section(&globals);
        }

        // Export section: rewrite function-export indices into the output space.
        if !main.exports.is_empty() {
            let mut exports = ExportSection::new();
            for export in &main.exports {
                let (kind, index) = match export.kind {
                    ExternalKind::Func => {
                        (ExportKind::Func, self.map_main_func(main, export.index)?)
                    }
                    ExternalKind::Memory => (ExportKind::Memory, export.index),
                    ExternalKind::Global => (ExportKind::Global, export.index),
                    ExternalKind::Table => (ExportKind::Table, export.index),
                    ExternalKind::Tag => (ExportKind::Tag, export.index),
                };
                exports.export(&export.name, kind, index);
            }
            module.section(&exports);
        }

        // Code section: re-encode every main body, then every merged body, each
        // under its own index map.
        let mut code = CodeSection::new();
        for local in &main.local_funcs {
            let body = self.reencode_main_body(main, &local.body)?;
            code.function(&body);
        }
        for m in &self.merged {
            let external = &externals[m.external_idx];
            let local = external
                .local_funcs
                .get((m.source_func_idx - external.local_func_base()) as usize)
                .ok_or_else(|| {
                    LinkError::Parse(format!(
                        "merged external function index {} is out of range",
                        m.source_func_idx
                    ))
                })?;
            let body = self.reencode_external_body(m.external_idx, &local.body)?;
            code.function(&body);
        }
        module.section(&code);

        // Name section: preserve sane debug names so the Rocq translator emits
        // named `Definition`s. Subsections must appear in ascending id order:
        // module (0), then functions (1), then locals (2). Without this section
        // every function — main locals included — would translate to an opaque
        // `func_<uuid>`, and the module/local debug names would be lost.
        let func_names = self.build_func_names(main);
        let local_names = self.build_local_names(main);
        if main.module_name.is_some() || func_names.is_some() || local_names.is_some() {
            let mut name_section = NameSection::new();
            if let Some(module_name) = &main.module_name {
                name_section.module(module_name);
            }
            if let Some(names) = &func_names {
                name_section.functions(names);
            }
            if let Some(locals) = &local_names {
                name_section.locals(locals);
            }
            module.section(&name_section);
        }

        // `inference.spec_funcs` section: rewrite each recorded spec function
        // index into the post-link output space and re-emit it. Codegen records
        // these indices in the pre-link space (which includes the now-removed
        // imports); without this rewrite a bare linked `.wasm` would name the
        // wrong functions in its proof obligations (C1), or — were the section
        // simply dropped (H25) — carry no obligations at all.
        if let Some(spec_funcs) = &main.spec_funcs {
            let remapped = self.remap_spec_funcs(main, spec_funcs)?;
            let payload = crate::spec_funcs::encode(&remapped);
            module.section(&wasm_encoder::CustomSection {
                name: crate::spec_funcs::SECTION_NAME.into(),
                data: (&payload[..]).into(),
            });
        }

        // `inference.hspecs` section: the obligation payload references functions
        // by symbolic name, not index, so — unlike `spec_funcs` — the merge
        // carries it through with no remap. The main module's function names
        // survive the rebuilt name section verbatim (only merged external names
        // are synthesized), so every symbol stays resolvable post-link. It was
        // validated at parse time; re-encoding the decoded map reproduces the
        // canonical bytes.
        if let Some(hspecs) = &main.hspecs {
            let payload = inference_hassert::encode(hspecs);
            module.section(&wasm_encoder::CustomSection {
                name: inference_hassert::HSPECS_SECTION_NAME.into(),
                data: (&payload[..]).into(),
            });
        }

        Ok(module.finish())
    }

    /// Rewrites every recorded spec-function index from the pre-link space into
    /// the post-link output space via [`Self::map_main_func`].
    ///
    /// Each index names a main-module function (a spec function is emitted by
    /// codegen as an ordinary local function), so the same import-removal shift
    /// that re-indexes calls applies here.
    fn remap_spec_funcs(
        &self,
        main: &ParsedModule,
        spec_funcs: &[(String, Vec<u32>)],
    ) -> Result<Vec<(String, Vec<u32>)>, LinkError> {
        spec_funcs
            .iter()
            .map(|(name, indices)| {
                let mapped = indices
                    .iter()
                    .map(|&idx| self.map_main_func(main, idx))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((name.clone(), mapped))
            })
            .collect()
    }

    /// Builds the output `name`-section local map: each main local function's
    /// local-variable names, re-indexed onto the import-free output space. The
    /// local indices within a function are unchanged by the merge; only the
    /// enclosing function index shifts. Returns `None` when no local carries a
    /// name.
    fn build_local_names(&self, main: &ParsedModule) -> Option<wasm_encoder::IndirectNameMap> {
        let import_count = main.imported_funcs.len() as u32;
        let mut entries: Vec<(u32, &Vec<(u32, String)>)> = Vec::new();
        for (local_idx, _) in main.local_funcs.iter().enumerate() {
            let source_idx = import_count + local_idx as u32;
            if let Some(locals) = main.local_names.get(&source_idx) {
                entries.push((self.main_local_base + local_idx as u32, locals));
            }
        }
        if entries.is_empty() {
            return None;
        }
        entries.sort_unstable_by_key(|(idx, _)| *idx);
        let mut indirect = wasm_encoder::IndirectNameMap::new();
        for (func_idx, locals) in entries {
            let mut map = NameMap::new();
            for (local_idx, name) in locals {
                map.append(*local_idx, name);
            }
            indirect.append(func_idx, &map);
        }
        Some(indirect)
    }

    /// Builds the output `name`-section function map: main locals keep their
    /// source debug names (re-indexed onto the import-free output space), and
    /// each merged function takes the name resolved at plan-build time. Returns
    /// `None` when no function carries a name, leaving the section out entirely.
    fn build_func_names(&self, main: &ParsedModule) -> Option<NameMap> {
        let import_count = main.imported_funcs.len() as u32;
        let mut entries: Vec<(u32, &str)> = Vec::new();

        for (local_idx, _) in main.local_funcs.iter().enumerate() {
            let source_idx = import_count + local_idx as u32;
            if let Some(name) = main.func_name(source_idx) {
                entries.push((self.main_local_base + local_idx as u32, name));
            }
        }
        for (i, m) in self.merged.iter().enumerate() {
            if let Some(name) = &m.name {
                entries.push((self.main_local_base + main.local_funcs.len() as u32 + i as u32, name));
            }
        }

        if entries.is_empty() {
            return None;
        }
        entries.sort_unstable_by_key(|(idx, _)| *idx);
        let mut names = NameMap::new();
        for (idx, name) in entries {
            names.append(idx, name);
        }
        Some(names)
    }

    fn reencode_main_body(
        &self,
        main: &ParsedModule,
        body: &[u8],
    ) -> Result<Function, LinkError> {
        // A re-encode failure inside the `func` closure cannot be returned
        // through `IndexMap`'s `Fn` signature, so it is captured in a `RefCell`
        // (keeping the closure `Fn`) and surfaced after `reencode_body` returns.
        let func_err: RefCell<Option<LinkError>> = RefCell::new(None);
        let func = |idx: u32| match self.map_main_func(main, idx) {
            Ok(mapped) => mapped,
            Err(e) => {
                func_err.borrow_mut().get_or_insert(e);
                0
            }
        };
        let ty = |idx: u32| {
            self.main_type_remap
                .get(idx as usize)
                .copied()
                .ok_or_else(|| {
                    LinkError::Parse(format!("main body references type index {idx} out of range"))
                })
        };
        let map = IndexMap {
            func: &func,
            ty: &ty,
        };
        let function = reencode_body(body, &map, BodyOrigin::Main)?;
        if let Some(e) = func_err.into_inner() {
            return Err(e);
        }
        Ok(function)
    }

    fn reencode_external_body(
        &self,
        external_idx: usize,
        body: &[u8],
    ) -> Result<Function, LinkError> {
        // As in `reencode_main_body`, a missing function-index mapping is
        // captured and surfaced after re-encoding rather than panicking.
        let func_err: RefCell<Option<LinkError>> = RefCell::new(None);
        let func = |idx: u32| match self.merged_index.get(&(external_idx, idx)) {
            Some(&mapped) => mapped,
            None => {
                func_err.borrow_mut().get_or_insert(LinkError::Parse(format!(
                    "merged body references function index {idx} not in its closure"
                )));
                0
            }
        };
        let remap = &self.external_type_remap[external_idx];
        let ty = |idx: u32| {
            remap.get(&idx).copied().ok_or_else(|| {
                LinkError::UnsupportedConstruct(format!(
                    "merged body references an unmapped type index {idx}"
                ))
            })
        };
        let map = IndexMap {
            func: &func,
            ty: &ty,
        };
        let function = reencode_body(body, &map, BodyOrigin::External)?;
        if let Some(e) = func_err.into_inner() {
            return Err(e);
        }
        Ok(function)
    }
}

/// Interns a signature into `out_types`, returning its index. Two functions
/// with identical signatures share one type entry (type dedup).
///
/// # Errors
///
/// Returns [`LinkError::UnsupportedConstruct`] if the signature contains a
/// reference-typed parameter or result. The static merge models no reference
/// types: collapsing `Ref(_)` to `i32` (the prior behavior) silently produced a
/// module whose bodies still operated on the reference, which no runtime
/// accepts. Rejecting here, at the single interning chokepoint, keeps every
/// merged signature representable.
fn intern_sig(
    out_types: &mut Vec<FuncSig>,
    cache: &mut BTreeMap<Vec<u8>, u32>,
    sig: &FuncSig,
) -> Result<u32, LinkError> {
    let key = sig_key(sig)?;
    if let Some(&idx) = cache.get(&key) {
        return Ok(idx);
    }
    let idx = out_types.len() as u32;
    out_types.push(sig.clone());
    cache.insert(key, idx);
    Ok(idx)
}

/// A stable byte key for a signature, used for dedup. Value types are encoded
/// as their discriminant; a `0xFF` separator distinguishes params from results.
///
/// Fails if any value type is a reference type, so a ref-typed signature can
/// never be interned and silently emitted.
fn sig_key(sig: &FuncSig) -> Result<Vec<u8>, LinkError> {
    let mut key = Vec::with_capacity(sig.params.len() + sig.results.len() + 1);
    for ty in &sig.params {
        key.push(val_type_tag(*ty)?);
    }
    key.push(0xFF);
    for ty in &sig.results {
        key.push(val_type_tag(*ty)?);
    }
    Ok(key)
}

/// A dedup discriminant for a supported value type. Floating-point, SIMD, and
/// reference types have no tag: each is an unsupported construct, surfaced as a
/// clean error (a float because the Inference language has no `f32`/`f64` types;
/// a `v128` because it has no SIMD types and every SIMD operator is rejected; a
/// reference rather than the prior `Ref(_) => I32` collapse). This is the
/// signature-axis chokepoint, paired with the operator-stream gate in
/// [`crate::safety`].
fn val_type_tag(ty: inf_wasmparser::ValType) -> Result<u8, LinkError> {
    use inf_wasmparser::ValType::*;
    Ok(match ty {
        I32 => 0,
        I64 => 1,
        F32 | F64 => {
            return Err(LinkError::UnsupportedConstruct(
                "floating-point value type (f32/f64) in merged function signature: \
                 the Inference language has no f32/f64 types"
                    .into(),
            ));
        }
        V128 => {
            return Err(LinkError::UnsupportedConstruct(
                "v128 value type in merged function signature: \
                 the Inference language has no SIMD types"
                    .into(),
            ));
        }
        Ref(_) => {
            return Err(LinkError::UnsupportedConstruct(
                "reference-typed value in merged function signature".into(),
            ));
        }
    })
}

/// Finds the external module bound under `module` that exports a function named
/// `field`, returning `(external_idx, func_idx)`.
///
/// Matches on the full `(module, field)` pair codegen records for every import,
/// not the field alone: an external is a candidate only when its logical module
/// equals `module`. This disambiguates two libraries that export the same field
/// but were bound under different logical modules — the earlier behavior, which
/// matched on field alone, let the path-sort order decide which body was merged.
///
/// Returns `Ok(None)` when no external bound under `module` exports `field`, and
/// [`LinkError::AmbiguousImport`] when more than one external is bound under the
/// same `(module, field)` pair, in which case the merge cannot soundly choose a
/// body and fails rather than silently linking the first.
fn find_export(
    externals: &[ParsedModule],
    module: &str,
    field: &str,
) -> Result<Option<(usize, u32)>, LinkError> {
    let mut found: Option<(usize, u32)> = None;
    for (i, ext) in externals.iter().enumerate() {
        if ext.logical_module != module {
            continue;
        }
        if let Some(idx) = ext.exported_func_index(field) {
            if found.is_some() {
                return Err(LinkError::AmbiguousImport {
                    module: module.to_string(),
                    field: field.to_string(),
                });
            }
            found = Some((i, idx));
        }
    }
    Ok(found)
}

/// Collects the type indices a body references through function-typed
/// `block`/`loop`/`if` and `call_indirect`/`return_call_indirect`, so the merge
/// can intern each signature and keep the type remap total.
///
/// Every operator is also gated through the fail-closed allow-list, matching
/// the closure scanner: a body reaching here has been closure-scanned already,
/// but re-checking keeps this walk self-contained. The verification-only non-det
/// blocks share the `blockty` payload, but they are rejected by the allow-list
/// (they have no executable semantics, so an external body that carries one is
/// not mergeable), so this walk never interns a type index on their behalf.
fn scan_body_type_indices(body: &[u8]) -> Result<Vec<u32>, LinkError> {
    use inf_wasmparser::{BinaryReader, BlockType, FunctionBody, Operator};

    let func_body = FunctionBody::new(BinaryReader::new(body, 0));
    let ops = func_body
        .get_operators_reader()
        .map_err(|e| LinkError::Parse(e.to_string()))?;

    let mut indices = Vec::new();
    for op in ops {
        let op = op.map_err(|e| LinkError::Parse(e.to_string()))?;
        crate::safety::check_operator(&op)?;
        match op {
            Operator::Block {
                blockty: BlockType::FuncType(idx),
            }
            | Operator::Loop {
                blockty: BlockType::FuncType(idx),
            }
            | Operator::If {
                blockty: BlockType::FuncType(idx),
            }
            | Operator::CallIndirect {
                type_index: idx, ..
            }
            | Operator::ReturnCallIndirect {
                type_index: idx, ..
            } => indices.push(idx),
            _ => {}
        }
    }
    Ok(indices)
}

/// Maps a value type into the encoder equivalent, rejecting floating-point and
/// reference types.
///
/// A float value type cannot appear in a merged signature: the Inference language
/// has no `f32`/`f64` types. A `v128` likewise cannot: the language has no SIMD
/// types and every SIMD operator is rejected, so the type axis must stay
/// consistent rather than carry the SIMD type into the output. A reference-typed
/// value cannot be soundly emitted either: the static merge models no reference
/// types, and collapsing `Ref(_)` to `i32` (the prior behavior) silently produced
/// a module whose bodies still operate on the reference, which no runtime
/// accepts. Surface each as a clean error. This duplicates the rejection in
/// [`val_type_tag`] as defense in depth: the two functions are reached on
/// independent paths (dedup keying vs. type emission), so each must guard the
/// unsupported value-type axes itself.
fn map_val_type(ty: &inf_wasmparser::ValType) -> Result<EncValType, LinkError> {
    use inf_wasmparser::ValType::*;
    Ok(match ty {
        I32 => EncValType::I32,
        I64 => EncValType::I64,
        F32 | F64 => {
            return Err(LinkError::UnsupportedConstruct(
                "floating-point value type (f32/f64) in merged function signature: \
                 the Inference language has no f32/f64 types"
                    .into(),
            ));
        }
        V128 => {
            return Err(LinkError::UnsupportedConstruct(
                "v128 value type in merged function signature: \
                 the Inference language has no SIMD types"
                    .into(),
            ));
        }
        Ref(_) => {
            return Err(LinkError::UnsupportedConstruct(
                "reference-typed value in merged function signature".into(),
            ));
        }
    })
}

/// Reconciles the linear memories of the main module and every memory-using
/// merged external into one shared output memory.
///
/// The merge folds every body onto a *single* memory, so the output's memory
/// must satisfy all of them at once. This accumulator folds each module's
/// memory in turn:
///
/// - **Fundamental shape** (`memory64`, `shared`, page size) must match across
///   every memory: a memory64 body addresses with i64, a shared body needs an
///   atomic memory, and a custom page size changes the address-to-page mapping —
///   none can be folded onto a differently-shaped memory. A mismatch is a clean
///   [`LinkError::IncompatibleMemory`].
/// - **Minimum** is widened to the maximum of every module's minimum, so the
///   output reserves enough pages for every module's static range (closing the
///   out-of-bounds miscompile, H15).
/// - **Maximum** is widened (a larger or unbounded maximum is the
///   least-restrictive choice), and a module that grows memory forces the
///   maximum to admit growth or the merge rejects it (H15).
/// - A **memoryless main** with a memory-using external synthesizes an output
///   memory from the external's declaration (H24); a memory-using external with
///   *no* memory declaration of its own and a memoryless main is irreconcilable
///   (there is nothing to address), so it is rejected (the guard, part C).
struct MemoryReconciler {
    /// The reconciled memory so far, or `None` if no module has contributed one.
    current: Option<EncMemoryType>,
    /// Whether the reconciled memory is required (some closure uses memory),
    /// even if no module declared one — which is then an error.
    required: bool,
}

impl MemoryReconciler {
    /// Seeds the reconciler with the main module's memory, if it has one.
    ///
    /// The main memory's shape is rejected here for the same reasons an
    /// external's is rejected in [`MemoryReconciler::fold`]: the output models a
    /// single 32-bit, non-shared, default-page-size memory, and wasm-to-v encodes
    /// only that model. A `memory64`, `shared`, or custom-page-size main memory
    /// would be merged into an output the translator silently re-encodes as
    /// 32-bit, so it is rejected absolutely rather than on the reconcile path
    /// alone (audit C-4/L-1).
    fn new(main_mem: Option<&inf_wasmparser::MemoryType>) -> Result<Self, LinkError> {
        if let Some(main_mem) = main_mem {
            reject_unsupported_memory_shape(main_mem, "<main module>")?;
        }
        Ok(MemoryReconciler {
            current: main_mem.map(to_enc_memory),
            required: false,
        })
    }

    /// Folds one external's memory and memory effects into the reconciliation.
    ///
    /// `uses_memory`/`uses_memory_grow` are the external closure's effects, used
    /// to decide whether a memory is required at all and whether growth must be
    /// admitted.
    fn fold(
        &mut self,
        ext_mem: Option<&inf_wasmparser::MemoryType>,
        uses_memory: bool,
        uses_memory_grow: bool,
        field: &str,
    ) -> Result<(), LinkError> {
        if uses_memory {
            self.required = true;
        }

        if let Some(ext_mem) = ext_mem {
            // Reject an unsupported memory shape for *every* contributed external
            // memory, including the `None => ext` adopt path onto a memoryless
            // main — otherwise a memory64/shared/custom-page external would be
            // adopted verbatim and wasm-to-v would silently re-encode it as a
            // 32-bit memory (audit C-4/L-1).
            reject_unsupported_memory_shape(ext_mem, field)?;
            let ext = to_enc_memory(ext_mem);
            self.current = Some(match self.current {
                None => ext,
                Some(cur) => reconcile_pair(cur, ext, field)?,
            });
        }

        if uses_memory_grow {
            self.admit_growth(field)?;
        }

        // A closure that uses memory but no module supplies one to address has
        // no valid shared memory to fold onto — reject rather than emit a body
        // that references a memory the output lacks (the guard, part C).
        if self.required && self.current.is_none() {
            return Err(LinkError::IncompatibleMemory {
                field: field.to_string(),
                reason: "the external accesses linear memory, but neither it nor the main module \
                         declares a memory to share"
                    .to_string(),
            });
        }

        Ok(())
    }

    /// Verifies the reconciled memory can actually grow: its maximum must
    /// exceed its minimum (or be unbounded). When the reconciled memory is
    /// pinned (`max == min`), a `memory.grow` always fails at runtime (returning
    /// -1), so the merge rejects it with a clear diagnostic rather than emit a
    /// module that silently mis-grows. Widening main's fixed maximum is avoided
    /// deliberately: an external must not silently relax the host program's own
    /// memory bound. A memoryless reconciliation that needs to grow is rejected
    /// by the caller's required-memory guard before reaching here.
    fn admit_growth(&self, field: &str) -> Result<(), LinkError> {
        let Some(mem) = self.current.as_ref() else {
            return Ok(());
        };
        if let Some(max) = mem.maximum
            && max <= mem.minimum
        {
            return Err(LinkError::IncompatibleMemory {
                field: field.to_string(),
                reason: format!(
                    "the external grows linear memory, but the reconciled memory's maximum \
                     ({max} pages) does not exceed its minimum ({} pages)",
                    mem.minimum
                ),
            });
        }
        Ok(())
    }

    /// Returns the reconciled memory to emit, or `None` when no module needs one.
    fn finish(self) -> Option<EncMemoryType> {
        self.current
    }
}

/// Reconciles the **anchor** memory `a` (the main module's declared memory, or
/// the accumulator already reconciled with it) with a contributing external
/// memory `b`, or returns a clean [`LinkError::IncompatibleMemory`].
///
/// `a`'s declared maximum is *authoritative* and is **never relaxed upward** by
/// `b`: the output keeps `a`'s maximum unchanged. Widening the output maximum to
/// the larger bound or to unbounded (the prior behavior) silently relaxed a main
/// that declared `(memory 1 1)` to admit an external's looser cap —
/// contradicting [`MemoryReconciler::admit_growth`]'s own refusal to relax the
/// host's memory bound, and removing the runtime backstop that would otherwise
/// trap an over-long fill early. An external declaring a larger or unbounded
/// maximum is *clamped* to the anchor's bound, not rejected: the external's
/// declared maximum only caps growth, and folding it under main's stricter cap is
/// a more-restrictive (sound) runtime. (A closure that actually grows memory is
/// gated separately by [`MemoryReconciler::admit_growth`] against the kept
/// maximum.)
///
/// The minimum is widened to `max(a.min, b.min)` to reserve enough pages for
/// every module's static range. The one reconciliation that *cannot* be honored
/// is a reserved minimum that exceeds the anchor's maximum — the external's
/// static footprint does not fit under the host's declared cap — which is
/// rejected rather than emitting an invalid `min > max` memory.
fn reconcile_pair(
    a: EncMemoryType,
    b: EncMemoryType,
    field: &str,
) -> Result<EncMemoryType, LinkError> {
    if a.memory64 != b.memory64 {
        return Err(LinkError::IncompatibleMemory {
            field: field.to_string(),
            reason: format!(
                "memory64 mismatch (one memory is memory64={}, the other memory64={})",
                a.memory64, b.memory64
            ),
        });
    }
    if a.shared != b.shared {
        return Err(LinkError::IncompatibleMemory {
            field: field.to_string(),
            reason: format!(
                "shared mismatch (one memory is shared={}, the other shared={})",
                a.shared, b.shared
            ),
        });
    }
    if a.page_size_log2 != b.page_size_log2 {
        return Err(LinkError::IncompatibleMemory {
            field: field.to_string(),
            reason: "custom page sizes differ between the two memories".to_string(),
        });
    }

    // Widen the minimum to satisfy both modules' static ranges, but keep the
    // anchor's maximum: the main module's declared cap is never relaxed upward.
    let minimum = a.minimum.max(b.minimum);
    let maximum = a.maximum;

    // The external's static footprint must fit under the host's declared cap; a
    // reserved minimum above it cannot be honored without relaxing the cap.
    if let Some(anchor_max) = maximum
        && minimum > anchor_max
    {
        return Err(LinkError::IncompatibleMemory {
            field: field.to_string(),
            reason: format!(
                "the reconciled minimum ({minimum} pages) exceeds the declared maximum \
                 ({anchor_max} pages) of the memory it is merged into; the kept memory bound \
                 is not relaxed"
            ),
        });
    }

    Ok(EncMemoryType {
        minimum,
        maximum,
        memory64: a.memory64,
        shared: a.shared,
        page_size_log2: a.page_size_log2,
    })
}

/// Rejects a memory whose fundamental shape the static merge and the Rocq
/// translator cannot model: a `memory64` (i64-addressed) memory, a `shared`
/// memory, or a memory with a non-default page size.
///
/// The output module declares a single 32-bit, non-shared, default-page-size
/// memory, and wasm-to-v encodes exactly that model (`Mm {|lim_min; lim_max|}`,
/// with no `memory64`/`shared`/page-size field). Adopting any other shape would
/// produce a `.wasm` whose machine the paired `.v` silently misdescribes — the
/// worst failure class for a verification-first toolchain. Every contributed
/// memory (the main module's and each external's) is checked, so the rejection
/// is absolute rather than reachable only on the two-memory reconcile path
/// (audit C-4/L-1).
fn reject_unsupported_memory_shape(
    mem: &inf_wasmparser::MemoryType,
    field: &str,
) -> Result<(), LinkError> {
    let reason = if mem.memory64 {
        "the memory is `memory64` (i64-addressed); the static merge models only a 32-bit memory \
         and would require a relocatable build"
    } else if mem.shared {
        "the memory is `shared`; the static merge models only a non-shared memory"
    } else if mem.page_size_log2.is_some() {
        "the memory declares a custom page size; the static merge models only the default page size"
    } else {
        return Ok(());
    };
    Err(LinkError::IncompatibleMemory {
        field: field.to_string(),
        reason: reason.to_string(),
    })
}

fn to_enc_memory(mem: &inf_wasmparser::MemoryType) -> EncMemoryType {
    EncMemoryType {
        minimum: mem.initial,
        maximum: mem.maximum,
        memory64: mem.memory64,
        shared: mem.shared,
        page_size_log2: mem.page_size_log2,
    }
}

fn map_global_type(g: &GlobalDef) -> Result<EncGlobalType, LinkError> {
    Ok(EncGlobalType {
        val_type: map_val_type(&g.ty.content_type)?,
        mutable: g.ty.mutable,
        shared: g.ty.shared,
    })
}

fn map_global_init(init: GlobalInit) -> ConstExpr {
    match init {
        GlobalInit::I32(v) => ConstExpr::i32_const(v),
        GlobalInit::I64(v) => ConstExpr::i64_const(v),
    }
}

#[cfg(test)]
mod tests {
    //! Direct unit tests for memory reconciliation paths the public `link` API
    //! cannot reach through valid WAT — notably the guard for a memory-using
    //! closure with no memory to address, which would require a structurally
    //! invalid external the `wat` assembler refuses to build.

    use super::*;
    use inf_wasmparser::MemoryType;

    fn mem(initial: u64, maximum: Option<u64>) -> MemoryType {
        MemoryType {
            memory64: false,
            shared: false,
            initial,
            maximum,
            page_size_log2: None,
        }
    }

    #[test]
    fn memory_using_closure_without_any_memory_is_rejected() {
        // The guard (part C): a closure that touches memory while no module —
        // neither main nor the external — declares one has nothing to address.
        let mut r = MemoryReconciler::new(None).expect("a memoryless main is supported");
        let err = r
            .fold(None, true, false, "f")
            .expect_err("a memory-using closure with no memory must be rejected");
        assert!(matches!(err, LinkError::IncompatibleMemory { .. }), "got {err:?}");
    }

    #[test]
    fn pure_closure_without_memory_is_fine() {
        // No memory effect, no memory declared: a pure merge needs no memory.
        let mut r = MemoryReconciler::new(None).expect("a memoryless main is supported");
        r.fold(None, false, false, "f").expect("pure closure needs no memory");
        assert!(r.finish().is_none(), "no memory is emitted for a pure merge");
    }

    #[test]
    fn minimum_is_widened_to_the_larger_of_two_memories() {
        let mut r =
            MemoryReconciler::new(Some(&mem(1, Some(20)))).expect("a 32-bit main is supported");
        r.fold(Some(&mem(10, Some(20))), true, false, "f")
            .expect("compatible memories reconcile");
        let out = r.finish().expect("a memory is emitted");
        assert_eq!(out.minimum, 10, "reconciled minimum is the larger of 1 and 10");
        assert_eq!(out.maximum, Some(20));
    }

    #[test]
    fn an_unbounded_external_maximum_does_not_relax_a_bounded_main() {
        // S4: a main that declared `(memory 1 5)` must NOT be relaxed to unbounded
        // by an external with no maximum. The external's static footprint (min 2)
        // fits under the cap, so the merge succeeds — but the output maximum stays
        // the main's declared 5, never silently unbounded.
        let mut r =
            MemoryReconciler::new(Some(&mem(1, Some(5)))).expect("a 32-bit main is supported");
        r.fold(Some(&mem(2, None)), true, false, "f")
            .expect("an unbounded external fits under the main's cap and clamps to it");
        let out = r.finish().expect("a memory is emitted");
        assert_eq!(
            out.maximum,
            Some(5),
            "the output maximum stays the main's declared cap, not unbounded"
        );
        assert_eq!(out.minimum, 2, "the minimum widens to the external's larger footprint");
    }

    #[test]
    fn a_larger_external_maximum_is_clamped_to_the_main_cap() {
        // S4: an external declaring a larger maximum (9) than the main's cap (5)
        // is clamped down to the main's bound, not widened up to the external's.
        // The external's static minimum (1) fits, so the merge succeeds with the
        // main's maximum preserved.
        let mut r =
            MemoryReconciler::new(Some(&mem(1, Some(5)))).expect("a 32-bit main is supported");
        r.fold(Some(&mem(1, Some(9))), true, false, "f")
            .expect("a larger external maximum is clamped to the main's cap");
        let out = r.finish().expect("a memory is emitted");
        assert_eq!(out.maximum, Some(5), "the output maximum stays the main's declared cap");
    }

    #[test]
    fn an_external_minimum_above_the_main_cap_is_rejected() {
        // S4: when the external's static footprint (min 9) exceeds the main's cap
        // (5), the reservation cannot be honored without relaxing the host's
        // declared maximum — reject rather than emit an invalid `min > max`.
        let mut r =
            MemoryReconciler::new(Some(&mem(1, Some(5)))).expect("a 32-bit main is supported");
        let err = r
            .fold(Some(&mem(9, None)), true, false, "f")
            .expect_err("an external footprint above the main's cap must be rejected");
        assert!(matches!(err, LinkError::IncompatibleMemory { .. }), "got {err:?}");
    }

    #[test]
    fn grow_against_a_fixed_reconciled_memory_is_rejected() {
        let mut r =
            MemoryReconciler::new(Some(&mem(1, Some(1)))).expect("a 32-bit main is supported");
        let err = r
            .fold(Some(&mem(1, Some(1))), true, true, "f")
            .expect_err("growth against a pinned memory must reject");
        assert!(matches!(err, LinkError::IncompatibleMemory { .. }), "got {err:?}");
    }

    #[test]
    fn a_smaller_bounded_external_keeps_the_main_maximum() {
        // Reconciling a `(memory 10 10)` main with a `(memory 1 3)` external: the
        // minimum widens to 10 (the larger), and the maximum stays the main's
        // declared 10 (the external's smaller 3 fits under it), so the result is a
        // valid `10..10` memory, not an invalid `min > max` and not a relaxed cap.
        let mut r =
            MemoryReconciler::new(Some(&mem(10, Some(10)))).expect("a 32-bit main is supported");
        r.fold(Some(&mem(1, Some(3))), true, false, "f")
            .expect("a smaller external maximum keeps the memory valid");
        let out = r.finish().expect("a memory is emitted");
        assert_eq!(out.minimum, 10);
        assert_eq!(out.maximum, Some(10), "the main's declared maximum is preserved");
    }

    #[test]
    fn a_no_max_external_keeps_a_pinned_main_cap_not_unbounded() {
        // S4 (the audit's named case): main `(memory 1 1)` + external `(memory 1)`
        // (no maximum). The external's static footprint (min 1) fits under the
        // pinned cap, so the merge succeeds — and the output maximum stays the
        // main's pinned 1, never silently unbounded.
        let mut r =
            MemoryReconciler::new(Some(&mem(1, Some(1)))).expect("a 32-bit main is supported");
        r.fold(Some(&mem(1, None)), true, false, "f")
            .expect("a no-max external fits under the pinned cap");
        let out = r.finish().expect("a memory is emitted");
        assert_eq!(
            out.maximum,
            Some(1),
            "the output maximum stays the main's pinned cap, NOT silently unbounded"
        );
    }

    fn memory64(initial: u64, maximum: Option<u64>) -> MemoryType {
        MemoryType { memory64: true, ..mem(initial, maximum) }
    }

    fn shared(initial: u64, maximum: Option<u64>) -> MemoryType {
        MemoryType { shared: true, ..mem(initial, maximum) }
    }

    fn custom_page(initial: u64, maximum: Option<u64>) -> MemoryType {
        MemoryType { page_size_log2: Some(0), ..mem(initial, maximum) }
    }

    /// Asserts the reconciler's `new` rejected the main memory by shape. `new`
    /// returns `Result<MemoryReconciler, _>`, whose `Ok` arm is not `Debug`, so
    /// we match the `Err` directly rather than calling `expect_err`.
    fn assert_new_rejects(main_mem: &MemoryType) {
        let result = MemoryReconciler::new(Some(main_mem));
        assert!(
            matches!(result, Err(LinkError::IncompatibleMemory { .. })),
            "expected IncompatibleMemory, got {:?}",
            result.err()
        );
    }

    #[test]
    fn a_memory64_main_is_rejected_absolutely() {
        // C-4: the main memory's shape is checked in `new`, so a 64-bit main can
        // never reach the output the translator re-encodes as 32-bit.
        assert_new_rejects(&memory64(1, Some(1)));
    }

    #[test]
    fn a_shared_main_is_rejected_absolutely() {
        // L-1: a bare `shared` main memory (no atomic op) is rejected by shape.
        assert_new_rejects(&shared(1, Some(1)));
    }

    #[test]
    fn a_custom_page_main_is_rejected_absolutely() {
        assert_new_rejects(&custom_page(1, Some(1)));
    }

    #[test]
    fn a_memory64_external_on_a_memoryless_main_is_rejected_on_the_adopt_path() {
        // C-4: the `None => ext` adopt path must reject too, so a 64-bit external
        // forwarded by a memoryless main is never silently adopted as 32-bit.
        let mut r = MemoryReconciler::new(None).expect("a memoryless main is supported");
        let err = r
            .fold(Some(&memory64(1, Some(1))), true, false, "f")
            .expect_err("a memory64 external must be rejected on adoption");
        assert!(matches!(err, LinkError::IncompatibleMemory { .. }), "got {err:?}");
    }

    #[test]
    fn a_shared_external_on_a_memoryless_main_is_rejected_on_the_adopt_path() {
        // L-1: a bare `shared` external (non-atomic body) onto a memoryless main
        // is rejected by shape on the adopt path, not just on reconcile.
        let mut r = MemoryReconciler::new(None).expect("a memoryless main is supported");
        let err = r
            .fold(Some(&shared(1, Some(1))), true, false, "f")
            .expect_err("a shared external must be rejected on adoption");
        assert!(matches!(err, LinkError::IncompatibleMemory { .. }), "got {err:?}");
    }

    #[test]
    fn a_custom_page_external_on_a_memoryless_main_is_rejected_on_the_adopt_path() {
        let mut r = MemoryReconciler::new(None).expect("a memoryless main is supported");
        let err = r
            .fold(Some(&custom_page(1, Some(1))), true, false, "f")
            .expect_err("a custom-page external must be rejected on adoption");
        assert!(matches!(err, LinkError::IncompatibleMemory { .. }), "got {err:?}");
    }

    #[test]
    fn a_memory64_external_against_a_32_bit_main_is_rejected_before_reconcile() {
        // The fold-path shape guard runs before `reconcile_pair`, so the
        // rejection reason names the unsupported shape, not a `memory64` mismatch.
        let mut r =
            MemoryReconciler::new(Some(&mem(1, Some(1)))).expect("a 32-bit main is supported");
        let err = r
            .fold(Some(&memory64(1, Some(1))), true, false, "f")
            .expect_err("a memory64 external must be rejected");
        let LinkError::IncompatibleMemory { reason, .. } = &err else {
            panic!("got {err:?}");
        };
        assert!(reason.contains("memory64"), "reason names the unsupported shape: {reason}");
    }

    #[test]
    fn ref_typed_signature_is_rejected_at_intern_time() {
        // Defense-in-depth behind the WASM 1.0 feature gate: the gate rejects a
        // ref-typed external up front, but `intern_sig` is the chokepoint every
        // merged signature passes through, so it must independently reject a
        // reference type rather than collapse it to `i32` (the prior silent
        // miscompile). This is the unit-level coverage for the layer the
        // integration test
        // `reference_typed_parameter_signature_is_rejected_at_the_feature_gate`
        // can no longer reach (the gate fronts it).
        use inf_wasmparser::{RefType, ValType};

        let ref_param = FuncSig {
            params: vec![ValType::Ref(RefType::FUNCREF)],
            results: vec![],
        };
        let err = sig_key(&ref_param).expect_err("a ref-typed param must not be interned");
        assert!(
            matches!(&err, LinkError::UnsupportedConstruct(msg) if msg.contains("reference-typed")),
            "expected an UnsupportedConstruct naming reference types, got {err:?}"
        );

        let ref_result = FuncSig {
            params: vec![],
            results: vec![ValType::Ref(RefType::FUNCREF)],
        };
        let mut out_types = Vec::new();
        let mut cache = std::collections::BTreeMap::new();
        let err = intern_sig(&mut out_types, &mut cache, &ref_result)
            .expect_err("a ref-typed result must not be interned");
        assert!(
            matches!(err, LinkError::UnsupportedConstruct(_)),
            "expected an UnsupportedConstruct, got {err:?}"
        );
        assert!(out_types.is_empty(), "no signature is committed on rejection");
    }

    #[test]
    fn v128_signature_is_rejected_at_intern_time() {
        // The Inference language has no SIMD types, and every SIMD operator is
        // rejected, so a `v128` in a function signature must be rejected on the
        // signature axis too rather than carried through into the merged type
        // table. `sig_key` is the dedup chokepoint every signature passes through;
        // `intern_sig` reaches it. This parallels the float/reference rejections.
        use inf_wasmparser::ValType;

        let v128_param = FuncSig {
            params: vec![ValType::V128],
            results: vec![],
        };
        let err = sig_key(&v128_param).expect_err("a v128 param must not be interned");
        assert!(
            matches!(&err, LinkError::UnsupportedConstruct(msg) if msg.contains("v128")),
            "expected an UnsupportedConstruct naming v128, got {err:?}"
        );

        let v128_result = FuncSig {
            params: vec![],
            results: vec![ValType::V128],
        };
        let mut out_types = Vec::new();
        let mut cache = std::collections::BTreeMap::new();
        let err = intern_sig(&mut out_types, &mut cache, &v128_result)
            .expect_err("a v128 result must not be interned");
        assert!(
            matches!(&err, LinkError::UnsupportedConstruct(msg) if msg.contains("v128")),
            "expected an UnsupportedConstruct naming v128, got {err:?}"
        );
        assert!(out_types.is_empty(), "no signature is committed on rejection");
    }
}
