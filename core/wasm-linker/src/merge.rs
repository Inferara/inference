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
//!
//! The output **global** space is built the same way and rewritten through the
//! same pass: the main module's globals keep indices `0..`, and each external
//! whose merged closure reads or writes a global has its whole global section
//! appended after them. Main's indices being fixed is what leaves its bodies and
//! its global exports untouched.
//!
//! ## Adopted external specifications
//!
//! A library compiled in proof mode ships obligations about its own code. They
//! are not part of the output — a specification function is outside every export
//! closure — so they are carried into the merged module's own
//! `inference.spec_funcs` / `inference.hspecs` only when the caller asks for it
//! ([`crate::ExternalSpecPolicy::Adopt`]), and then only the **universal**
//! (`forall`) ones.
//!
//! An adopted specification is keyed under the logical module the library was
//! bound from, folded onto the library's own spec name, and every function
//! symbol its obligations apply is resolved in the *library's* own `name`
//! section, required to be one of the bodies this merge folded in, and rewritten
//! to the exact string the output's `name` section carries for that body. The
//! adopted key's `inference.spec_funcs` entry lists no index, because the
//! library's specification function did not cross the merge — which is the
//! correct shape for a universal obligation, whose judgment never reduces a
//! specification body.
//!
//! Adoption carries obligations, never proofs: each arrives downstream as a
//! `ValidSpec` theorem with an unfilled proof, to be discharged against the
//! merged module. That is what makes it sound across everything the merge
//! changes about the library's environment — one shared linear memory, remapped
//! globals, renumbered calls. Those change *what must be proved*; they cannot
//! turn a false claim into a discharged one. The one property the merge must
//! preserve is symbol identity, which is why every check below is about which
//! body a symbol denotes.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use inf_wasmparser::ExternalKind;
use inference_hassert::{HAssert, HFnRef, HSpecMap, HTerm};
use wasm_encoder::{
    CodeSection, ConstExpr, ExportKind, ExportSection, Function, FunctionSection, GlobalSection,
    GlobalType as EncGlobalType, MemorySection, MemoryType as EncMemoryType, Module, NameMap,
    NameSection, TypeSection, ValType as EncValType,
};

use crate::closure;
use crate::parse::{FuncSig, GlobalDef, GlobalInit, ParsedModule, TypeEntry};
use crate::rewrite::{reencode_body, BodyOrigin, IndexMap};
use crate::tier::{self, Tier, WriteContract};
use crate::{
    ExternalSpecPolicy, ImportWriteSet, LinkError, LinkOptions, LinkOutput, LinkWarning,
};

/// Resolves and merges every satisfiable import of `main` from the supplied
/// external modules, returning the unified module bytes and everything the
/// completed merge owes the user beyond them.
///
/// Each external arrives as `(logical_module, bytes)` so the merge can match an
/// import's recorded `(module, field)` against the external's logical module.
///
/// `contracts` carries the two write-set modes documented on [`crate::link`]:
/// `None` runs merge mechanics only, `Some(list)` holds every satisfied import
/// to a declared write set — and one `list` does not mention to the claim that
/// it writes nothing, which is what declaring nothing about it says.
///
/// `options` carries what the merge does with the verification sections a linked
/// external ships, which is also what decides whether they are decoded at all.
pub(crate) fn link(
    main_bytes: &[u8],
    externals: &[(&str, &[u8])],
    contracts: Option<&[ImportWriteSet]>,
    options: &LinkOptions,
) -> Result<LinkOutput, LinkError> {
    // The contract list is checked before anything reads a byte: it is a pure
    // property of the caller's argument, says nothing about either module, and
    // decides how every import below is judged, so a defect in it must not be
    // reported behind a diagnostic about the bytes.
    validate_contracts(contracts)?;

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
    let decode_specs = options.external_specs == ExternalSpecPolicy::Adopt;
    let externals = externals
        .iter()
        .map(|(logical_module, bytes)| {
            ParsedModule::parse_external(bytes, logical_module, decode_specs)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let plan = Plan::build(&main, &externals, contracts, options)?;

    // The obligation symbols are cleared against the module the plan describes,
    // before a byte of it exists. An obligation that names nothing, or names two
    // things, is a defect in the deliverable this whole pass exists to produce,
    // and the plan is the last place that still knows what each name was
    // supposed to refer to.
    plan.check_obligation_symbols(&main, &externals)?;

    let merged = plan.emit(&main, &externals)?;

    // Post-merge validation gate. The effect scanner is an allow-list and can
    // never be proven complete against an adversarial external `.wasm`; this
    // final check ensures the merge never persists a structurally-invalid
    // artifact (the input to formal verification), converting every effect-
    // scanner gap into a clean diagnostic instead of a silent miscompile.
    inf_wasmparser::validate(&merged)
        .map_err(|e| LinkError::InvalidMergedModule(e.to_string()))?;

    Ok(LinkOutput {
        wasm: merged,
        warnings: plan.warnings,
    })
}

/// Resolves the write-set contract governing one satisfied import.
///
/// The unchecked mode passes straight through. In the checked mode an import the
/// list does not mention resolves to [`WriteContract::Unmentioned`], which is
/// checked as strictly as an empty declared write set — "nothing declared" is
/// exactly the claim that the closure writes nothing — and is never an
/// exemption. It is kept distinct from an empty [`WriteContract::Declared`] so
/// the rejection can say which of the two happened rather than describe a
/// declaration nobody wrote.
///
/// The `find` is unambiguous because [`validate_contracts`] has already refused
/// a list holding two entries for one `(module, field)`.
fn resolve_contract<'a>(
    contracts: Option<&'a [ImportWriteSet]>,
    logical_module: &str,
    field: &str,
) -> WriteContract<'a> {
    let Some(list) = contracts else {
        return WriteContract::Unchecked;
    };
    match list
        .iter()
        .find(|c| c.module == logical_module && c.field == field)
    {
        Some(contract) => WriteContract::Declared {
            mut_params: &contract.mut_params,
            param_names: &contract.param_names,
        },
        None => WriteContract::Unmentioned,
    }
}

/// Refuses a contract list that holds more than one entry for one
/// `(module, field)` pair.
///
/// The list is a map written as a slice, and the lookup that reads it takes the
/// first match — so two entries for one key would decide the link by their order
/// in the slice, silently and in either direction: a permissive entry ahead of a
/// restrictive one admits bytes the reverse order refuses. Neither answer is
/// derivable from the pair, so the list is rejected instead of resolved.
///
/// Rejecting is what the front end already guarantees. It folds two agreeing
/// declarations of one import into a single entry and reports a hard error
/// naming both files when they disagree, so a list it produced never carries a
/// duplicate key; the check makes the public API hold the same invariant its own
/// caller does, rather than assume it.
fn validate_contracts(contracts: Option<&[ImportWriteSet]>) -> Result<(), LinkError> {
    let Some(list) = contracts else {
        return Ok(());
    };
    let mut seen: BTreeSet<(&str, &str)> = BTreeSet::new();
    for contract in list {
        if !seen.insert((contract.module.as_str(), contract.field.as_str())) {
            return Err(LinkError::DuplicateWriteContract {
                module: contract.module.clone(),
                field: contract.field.clone(),
            });
        }
    }
    Ok(())
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
    /// `func_<uuid>`. A closure root takes a satisfied import field; an inner
    /// callee keeps its own debug name, marked, when the source module carried
    /// one.
    ///
    /// One name, because a WASM name map holds one per function index — a body
    /// satisfying several imports records the least of its root names, and
    /// [`Plan::root_symbols`] is what keeps the rest resolvable.
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
    /// Every merged root name the satisfied imports asked for, against the
    /// output function index of the body that answers it.
    ///
    /// This is the `(logical_module, export_field) -> merged function index`
    /// table the name section cannot be: one foreign body may satisfy several
    /// imports, and a WASM name map holds one name per index. It is what makes
    /// an alias the section could not record still resolvable, and it is what
    /// lets a failure to resolve an obligation symbol say which imports the
    /// merge actually satisfied.
    root_symbols: BTreeMap<String, u32>,
    /// The main module's obligation payload as the output will carry it: every
    /// applied symbol naming an unrecorded root alias rewritten onto the name
    /// the output's name section records for that body — bar a contested one,
    /// which is left as written and rejected by
    /// [`Self::check_obligation_symbols`] instead.
    ///
    /// Held on the plan rather than re-derived in [`Self::emit`] so the symbols
    /// the post-merge check clears are the symbols the emitted bytes carry.
    hspecs: Option<inference_hassert::HSpecMap>,
    /// The distinct function symbols [`Self::hspecs`] applies, after that
    /// rewrite — the set [`Self::check_obligation_symbols`] must resolve.
    ///
    /// Adopted symbols join it, so a library's obligation is held to naming
    /// exactly one body of the merged module, exactly as the program's own are.
    obligation_symbols: BTreeSet<String>,
    /// The `inference.spec_funcs` keys minted for adopted external
    /// specifications, ascending. Empty under every policy but adoption.
    ///
    /// Only the keys: an adopted specification has no specification function in
    /// the output — the library's never crossed the merge — so its entry lists
    /// no index. The obligations themselves fold into [`Self::hspecs`], because
    /// the emitted section makes no distinction between an adopted key and one
    /// the program declared, and neither does the proof translation.
    adopted_spec_keys: BTreeSet<String>,
    /// Applied symbols that name a merged root the output's `name` section could
    /// not record, against the output index of the body that root binds.
    ///
    /// A name map holds one name per function index, so a body satisfying
    /// several imports leaves its other root names off the section. Such a name
    /// is normally repaired by the alias rewrite in [`Self::build`]; it lands
    /// here instead when some other function of the output is already named that
    /// string, which leaves the symbol naming two bodies at once. This is the
    /// carrier [`Self::check_obligation_symbols`] cannot read off the emitted
    /// section, and adding it is what turns the collision into a rejection
    /// rather than a silent choice between the two.
    contested_root_aliases: BTreeMap<String, u32>,
    /// Per external module: `source_type_idx -> output type idx` for the types
    /// its merged closure references.
    external_type_remap: Vec<BTreeMap<u32, u32>>,
    /// Output global section: the main module's globals at their original
    /// indices, followed by the globals of each external that contributes one.
    ///
    /// Main keeps indices `0..main.globals.len()` so its own bodies and its
    /// global exports need no rewriting at all.
    out_globals: Vec<GlobalDef>,
    /// Per external module: `source_global_idx -> output global idx`, total over
    /// the external's declared globals when it contributes them and **empty**
    /// when it does not. The empty case is load-bearing rather than incidental:
    /// it is what turns a body that names a global against the closure scanner's
    /// verdict into a clean [`LinkError`] instead of a rebind onto main's state.
    external_global_remap: Vec<BTreeMap<u32, u32>>,
    /// What the completed merge owes the user beyond the bytes themselves.
    warnings: Vec<LinkWarning>,
    /// The single shared linear memory the output declares, reconciled across
    /// the main module and every memory-using merged external. `None` when no
    /// module needs a memory (a fully pure merge).
    reconciled_memory: Option<EncMemoryType>,
}

impl Plan {
    fn build(
        main: &ParsedModule,
        externals: &[ParsedModule],
        contracts: Option<&[ImportWriteSet]>,
        options: &LinkOptions,
    ) -> Result<Self, LinkError> {
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
        //    it. An import is satisfiable when the external bound under its
        //    logical module exports a function of its field name: the merge
        //    keys on the full `(module, field)` pair codegen records for every
        //    import, so two libraries exporting the same field under different
        //    logical modules are never conflated.
        let main_import_count = main.imported_funcs.len() as u32;
        let mut import_target = BTreeMap::new();
        // Every merged root name the imports ask for, against the body that
        // satisfies it. Keyed by the joined name rather than the pair it is
        // built from because that is the form an obligation writes and the name
        // section carries; the pair is recoverable from neither on its own, and
        // nothing here needs it split.
        let mut root_symbols: BTreeMap<String, u32> = BTreeMap::new();
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

        // The import fields satisfied by a Tier-B external, in import order. A
        // duplicate import entry (valid WASM: two entries may name the same
        // module and field) must not name the same function twice in the
        // warning, so each field is recorded once.
        let mut tier_b_fields: Vec<String> = Vec::new();

        // Whether any merged closure of each external reads or writes a global,
        // which decides whether that external's global declarations are carried
        // into the output. One external may satisfy several imports, so this
        // accumulates across its closures: the whole module's globals are
        // contributed once, as a unit, if any of them are touched.
        let mut external_contributes_globals = vec![false; externals.len()];

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
            let logical_module = &main.imported_funcs[import_idx].module;
            let field = &main.imported_funcs[import_idx].field;
            // Tier C is rejected here, before any output index is committed. The
            // classifier runs the address-provenance analysis for memory-using
            // closures, so an absolute-address access is rejected as Tier C, and
            // holds a Tier-B closure to the import's declared write set.
            let contract = resolve_contract(contracts, logical_module, field);
            let verdict = tier::classify(external, &cl, root, logical_module, field, &contract)?;
            if verdict == Tier::B && !tier_b_fields.contains(field) {
                tier_b_fields.push(field.clone());
            }
            external_contributes_globals[ext_idx] |= cl.effects.uses_globals;

            // Reconcile this external's memory into the shared output memory:
            // fold in its declared limits (widening minimum/maximum) and check
            // its memory effects against the reconciled result. This folds an
            // external memory onto a memoryless main (H24), keeps the merged
            // minimum large enough for every module's static range (H15), and
            // rejects growth the reconciled maximum cannot satisfy. Incompatible
            // fundamental shapes (`memory64`/`shared`/page size) are rejected.
            // Only a closure that actually addresses memory contributes limits —
            // the tier verdict above is the same distinction, Tier B versus A.
            memory.fold(
                external.memory.as_ref(),
                cl.effects.uses_memory,
                cl.effects.uses_memory_grow,
                field,
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
                // Name the merged inner callee under its logical module and
                // the internal mark (`mathlib::#helper`). Two externals bound
                // under different logical modules may export — and internally
                // call — functions of the same name; without the prefix those
                // names would collide in the output name section and force
                // wasm-to-v's index-suffix disambiguation (`helper` vs
                // `helper_2`), which is index-dependent and shifts across
                // merges. The mark additionally holds a private callee apart
                // from a root of its own module: an inner debug name comes from
                // the foreign module and is unconstrained, so it may be exactly
                // an export field one of that module's roots is named after.
                //
                // Both separations are *wasm-level*. Neither is a collision
                // guarantee at the Rocq level: wasm-to-v maps every
                // non-identifier byte to `_` and collapses the runs, so two
                // distinct sources can still sanitize to one Rocq identifier,
                // and its index suffix remains the final disambiguator there.
                merged.push(MergedFunc {
                    external_idx: ext_idx,
                    source_func_idx: src_func,
                    out_type_idx,
                    name: external.func_name(src_func).map(|name| {
                        inference_fn_key::merged_name::callee(&external.logical_module, name)
                    }),
                });
            }

            let root_output = merged_index[&(ext_idx, root)];
            import_target.insert(import_idx as u32, root_output);
            root_symbols.insert(
                inference_fn_key::merged_name::root(&external.logical_module, field),
                root_output,
            );
        }

        // Name every closure root after an import field it satisfies, under its
        // external's logical module (`mathlib::sum`), so the merged function
        // reads as an ordinary named definition traceable to its source module.
        // A proof-mode obligation applying that external writes the same string
        // through the same producer, so the name comes from there rather than a
        // second `format!` that could drift from it. An explicit debug name on
        // the source module is overwritten, because the field the import names
        // is what an obligation can refer to.
        //
        // One foreign body can satisfy several imports — an external exporting
        // one function under two fields, bound by two `external fn`
        // declarations, roots both at the same output index — and only one of
        // those names can be recorded. A WASM name map is a map from a function
        // index to *one* name, and the Rocq translator's own model of the
        // section is likewise index-keyed, so a second entry for one index is
        // dropped downstream even where the encoder accepts the bytes. What
        // keeps the unrecorded alias usable is `root_symbols`, which holds every
        // `(logical_module, export_field)` pair the merge was asked for against
        // the body that satisfies it: the obligation payload is rewritten
        // through it below, so an obligation over either alias applies the name
        // the section does carry.
        //
        // The recorded name is the least of a body's aliases rather than the
        // last one written, so the output depends on the set of imports and not
        // on the order the main module happens to list them in — the rule the
        // output global space already follows.
        let mut canonical_root: BTreeMap<u32, &str> = BTreeMap::new();
        for (root_name, out_idx) in &root_symbols {
            canonical_root.entry(*out_idx).or_insert(root_name.as_str());
        }
        for m in &mut merged {
            if let Some(out_idx) = merged_index.get(&(m.external_idx, m.source_func_idx))
                && let Some(name) = canonical_root.get(out_idx)
            {
                m.name = Some((*name).to_string());
            }
        }

        // Give every still-nameless merged inner callee a name derived from its
        // output function index, under its logical module and the internal mark
        // (`lib::#func_5`). An external stripped of its `name` section
        // (third-party / `wasm-tools`-stripped) leaves inner callees with
        // `name: None`; without a name `build_func_names` emits no name-section
        // entry, and `wasm-to-v` then falls back to a per-process random UUID
        // `Definition` name, making the `.v` non-reproducible for byte-identical
        // input. Naming each from its deterministic output index keeps the name
        // section complete and the proof artifact reproducible. The prefix and
        // the mark keep the synthesized name in the same merged namespace as the
        // roots and callees above, so two stripped externals can never produce
        // the same fallback name for distinct functions and no root can be
        // shadowed by one.
        let merged_base = main_local_base + main.local_funcs.len() as u32;
        for (i, m) in merged.iter_mut().enumerate() {
            if m.name.is_none() {
                let logical_module = &externals[m.external_idx].logical_module;
                m.name = Some(inference_fn_key::merged_name::anonymous(
                    logical_module,
                    merged_base + i as u32,
                ));
            }
        }

        // Point every obligation that applies an unrecorded root alias at the
        // name the section carries for that body, and collect the symbol set
        // the merged module must answer for.
        //
        // The rewrite is a repair for a name the merge could not record, and it
        // applies only where the alias names nothing else. An alias some other
        // function of the output is already named has two readings that are both
        // genuine — the body an import bound under that field, and the function
        // the section records under that string — and neither may be picked
        // silently: rewriting moves the obligation onto the merged body, dropping
        // the alias leaves it resolving against the other function, and whichever
        // is wrong yields a *true* obligation about a body nobody wrote it about,
        // at exit 0. So a contested alias is neither rewritten nor dropped. It is
        // recorded against the root it binds, becoming a second carrier of the
        // symbol, and the check that rejects any other two-carrier symbol rejects
        // it as ambiguous — which is the one outcome that says what happened
        // instead of guessing past it.
        //
        // Only a contested alias an obligation *applies* is recorded. One nothing
        // names misresolves nothing, and rejecting it would fail a link whose
        // proof artifact is correct. The applied set is what the rewrite returns,
        // and a contested alias is left unrewritten, so a contested name an
        // obligation applies reaches that set verbatim and no second traversal is
        // needed to find it.
        //
        // `link` takes arbitrary main bytes, so a contested alias is reachable
        // even though code generation cannot give one of the program's own
        // functions a `::`-joined name.
        let carried: BTreeSet<&str> = name_section_entries(main, main_local_base, &merged)
            .into_iter()
            .map(|(_, name)| name)
            .collect();
        let mut hspecs = main.hspecs.clone();
        let mut contested_root_aliases: BTreeMap<String, u32> = BTreeMap::new();
        let mut obligation_symbols: BTreeSet<String> = BTreeSet::new();
        if let Some(map) = &mut hspecs {
            let mut aliases: BTreeMap<&str, &str> = BTreeMap::new();
            let mut contested: BTreeMap<&str, u32> = BTreeMap::new();
            for (root_name, out_idx) in &root_symbols {
                let Some(&canonical) = canonical_root.get(out_idx) else {
                    continue;
                };
                if canonical == root_name.as_str() {
                    continue;
                }
                if carried.contains(root_name.as_str()) {
                    contested.insert(root_name.as_str(), *out_idx);
                } else {
                    aliases.insert(root_name.as_str(), canonical);
                }
            }
            obligation_symbols = canonicalize_applied_symbols(map, &aliases);
            for (root_name, out_idx) in contested {
                if obligation_symbols.contains(root_name) {
                    contested_root_aliases.insert(root_name.to_string(), out_idx);
                }
            }
        }

        // Report on, or carry in, the verification sections the linked libraries
        // ship. Both run only for a library that contributed at least one merged
        // body: one nothing imports from is not part of the artifact, so nothing
        // about it was dropped and nothing about it is adoptable.
        let mut adopted_spec_keys: BTreeSet<String> = BTreeSet::new();
        let mut policy_warnings: Vec<LinkWarning> = Vec::new();
        match options.external_specs {
            ExternalSpecPolicy::Ignore => {}
            ExternalSpecPolicy::Warn => {
                policy_warnings.extend(external_spec_warning(externals, &merged_index));
            }
            ExternalSpecPolicy::Adopt => {
                let adopted = adopt_external_specs(
                    main,
                    externals,
                    &merged,
                    &merged_index,
                    merged_base,
                    &carried,
                )?;
                if !adopted.specs.is_empty() {
                    let map = hspecs.get_or_insert_with(inference_hassert::HSpecMap::default);
                    for (key, entries) in adopted.specs {
                        map.insert(key.clone(), entries);
                        adopted_spec_keys.insert(key);
                    }
                }
                obligation_symbols.extend(adopted.symbols);
                policy_warnings.extend(adopted.warnings);
            }
        }

        // Re-validating is not ceremony: the payload arrived validated, but every
        // edit above — the alias rewrite and the adoption — is this crate's own,
        // and `encode` *panics* on a map its decoder would reject. Checking the
        // edited map turns a producer defect into a diagnosable link failure. The
        // reachable one is a name pushed past the codec's cap by the adopted
        // symbol's prefix, which is why the message names adoption rather than
        // only the rewrite it used to be raised for.
        if let Some(map) = &hspecs {
            inference_hassert::validate(map).map_err(|e| {
                LinkError::Parse(format!(
                    "inference.hspecs section, after rewriting merged-body aliases and \
                     adopting external obligations: {e}"
                ))
            })?;
        }

        // Build the output global space: main's globals keep indices `0..`, and
        // each contributing external's are appended after them.
        //
        // Identical globals across two externals are **not** deduplicated, and
        // this is the one place where the merge's treatment of globals and its
        // treatment of signatures must diverge. `intern_sig` collapses two
        // structurally identical function types because a type is a *description*
        // — two modules naming the same shape mean the same thing by it. A global
        // is not a description but a variable: two externals that each declare
        // `(global (mut i32) (i32.const 0))` have a counter apiece, and collapsing
        // them would splice two modules' independent state into one cell, where
        // each module's writes silently become the other's reads. Nothing
        // downstream would object — the module validates, and the `.v` describes
        // the collapsed machine faithfully.
        //
        // Externals are walked in slice order rather than in the order their
        // imports were satisfied, so the output global space depends only on the
        // input, not on the order the main module happens to list its imports in.
        let mut out_globals = main.globals.clone();
        let mut external_global_remap: Vec<BTreeMap<u32, u32>> =
            externals.iter().map(|_| BTreeMap::new()).collect();
        for (ext_idx, external) in externals.iter().enumerate() {
            if !external_contributes_globals[ext_idx] {
                continue;
            }
            for (source_idx, global) in external.globals.iter().enumerate() {
                external_global_remap[ext_idx]
                    .insert(source_idx as u32, out_globals.len() as u32);
                out_globals.push(global.clone());
            }
        }

        let reconciled_memory = memory.finish();

        Ok(Plan {
            out_types,
            main_type_remap,
            import_target,
            main_local_base,
            merged,
            merged_index,
            root_symbols,
            hspecs,
            obligation_symbols,
            adopted_spec_keys,
            contested_root_aliases,
            external_type_remap,
            out_globals,
            external_global_remap,
            warnings: unbounded_reach_warning(&tier_b_fields, reconciled_memory.as_ref())
                .into_iter()
                .chain(policy_warnings)
                .collect(),
            reconciled_memory,
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

        // Global section: main's globals, then each contributing external's.
        // Emitted from the plan rather than from `main` directly, so a main
        // module that declares no globals of its own still gets a section when an
        // external brings one.
        if !self.out_globals.is_empty() {
            let mut globals = GlobalSection::new();
            for g in &self.out_globals {
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
        //
        // Adopted keys follow main's own entries, ascending, each listing no
        // index: the library's specification function did not cross the merge,
        // and a synthetic index naming the merged body the obligation is *about*
        // would be a lie — that body is not a specification function, and listing
        // it would drop it from the module record the obligation applies.
        //
        // The condition keys on the sections' presence rather than on the
        // concatenation being non-empty: a main module carrying an *empty*
        // `inference.spec_funcs` section still re-emits an empty one, because the
        // section's presence is itself a producer's statement.
        if main.spec_funcs.is_some() || !self.adopted_spec_keys.is_empty() {
            let mut entries = match &main.spec_funcs {
                Some(spec_funcs) => self.remap_spec_funcs(main, spec_funcs)?,
                None => Vec::new(),
            };
            entries.extend(
                self.adopted_spec_keys
                    .iter()
                    .map(|key| (key.clone(), Vec::new())),
            );
            let payload = crate::spec_funcs::encode(&entries);
            module.section(&wasm_encoder::CustomSection {
                name: crate::spec_funcs::SECTION_NAME.into(),
                data: (&payload[..]).into(),
            });
        }

        // `inference.hspecs` section: the obligation payload references functions
        // by symbolic name, not index, so — unlike `spec_funcs` — no index remap
        // applies. The main module's function names survive the rebuilt name
        // section verbatim (only merged external names are synthesized), and the
        // plan has already pointed any applied symbol naming an unrecorded root
        // alias at the name this section does record, so every symbol resolves
        // against the module emitted here. `Plan::build` re-validated the map
        // after that rewrite, which is what keeps `encode` — it panics on a map
        // its own decoder would reject — off a payload this crate edited.
        if let Some(hspecs) = &self.hspecs {
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

    /// The output `name` section's function entries, ascending by output index:
    /// main locals under their source debug names (re-indexed onto the
    /// import-free output space), then each merged function under the name
    /// resolved at plan-build time.
    ///
    /// The single source of what the emitted section says. The encoder,
    /// [`Self::check_obligation_symbols`] and the alias rewrite in
    /// [`Self::build`] all read it, so each clears the names the artifact
    /// actually carries rather than a second reconstruction of them.
    fn func_name_entries<'a>(&'a self, main: &'a ParsedModule) -> Vec<(u32, &'a str)> {
        name_section_entries(main, self.main_local_base, &self.merged)
    }

    /// Output function index of the first merged external body.
    fn merged_base(&self, main: &ParsedModule) -> u32 {
        self.main_local_base + main.local_funcs.len() as u32
    }

    /// Builds the output `name`-section function map from
    /// [`Self::func_name_entries`]. Returns `None` when no function carries a
    /// name, leaving the section out entirely.
    fn build_func_names(&self, main: &ParsedModule) -> Option<NameMap> {
        let entries = self.func_name_entries(main);
        if entries.is_empty() {
            return None;
        }
        let mut names = NameMap::new();
        for (idx, name) in entries {
            names.append(idx, name);
        }
        Some(names)
    }

    /// Main's specification functions, at their post-merge output indices.
    ///
    /// Reads the same `inference.spec_funcs` payload [`Self::remap_spec_funcs`]
    /// re-encodes into the output, through the same index mapping, so the set
    /// this returns is exactly the set the emitted section names. Empty when
    /// main carries no such section, which is every compile-mode build.
    fn merged_spec_func_indices(&self, main: &ParsedModule) -> Result<BTreeSet<u32>, LinkError> {
        let Some(spec_funcs) = &main.spec_funcs else {
            return Ok(BTreeSet::new());
        };
        let mut indices = BTreeSet::new();
        for (_, listed) in spec_funcs {
            for &idx in listed {
                indices.insert(self.map_main_func(main, idx)?);
            }
        }
        Ok(indices)
    }

    /// Rejects a merge whose obligation payload applies a function symbol the
    /// merged module does not answer for with exactly one function.
    ///
    /// The proof translator resolves an obligation's `T_app` / `HA_app_ok`
    /// symbol by looking it up in the emitted `name` section, so a symbol no
    /// function carries has nothing to say and a symbol two functions share
    /// silently picks one. Either way the obligation stops describing the
    /// program it was written about — and a *true* obligation about the wrong
    /// body is the worst outcome, because it discharges.
    ///
    /// Carriers are read off that section, plus one the section cannot show: a
    /// contested root alias binds a merged body under a name the section could
    /// not record, so the symbol reaches that body as well as the function the
    /// section names it against. [`Self::contested_root_aliases`] holds those,
    /// and they are weighed here so the collision is reported rather than
    /// resolved by whichever of the two meanings the lookup happens to reach.
    ///
    /// The check belongs here rather than in the translator because this is the
    /// last phase that knows what the symbol was supposed to name: which imports
    /// were satisfied, from which logical module, under which export field, and
    /// which external supplied each merged body. `emit` writes no import
    /// section, so downstream all of that is gone and the only honest report
    /// left is the symbol itself.
    ///
    /// Only *applied* symbols are checked. An obligation's own `fn_symbol`
    /// names a specification function, which is resolved against the
    /// `inference.spec_funcs` index list under a spec-name-stripping rule the
    /// translator owns; re-deciding it here would be a second implementation of
    /// that rule, free to disagree with the one that governs.
    ///
    /// A carrier the merged `inference.spec_funcs` lists is not a candidate:
    /// the narrowing is [`applicable_carriers`], the same rule the translator
    /// applies downstream, so counting one here would fail a link the
    /// translator resolves correctly.
    ///
    /// When *every* carrier is a specification function the full set stands, so
    /// the count is over specification functions after all: two or more are
    /// still ambiguous here, and exactly one is deliberately let through. That
    /// one is not resolvable either, but the translator is the phase that can
    /// say *why* — naming the target as an omitted or a retained specification
    /// function — where a rejection here could report only the symbol.
    fn check_obligation_symbols(
        &self,
        main: &ParsedModule,
        externals: &[ParsedModule],
    ) -> Result<(), LinkError> {
        if self.obligation_symbols.is_empty() {
            return Ok(());
        }
        let entries = self.func_name_entries(main);
        let spec_funcs = self.merged_spec_func_indices(main)?;
        for symbol in &self.obligation_symbols {
            let mut all: Vec<u32> = entries
                .iter()
                .filter(|(_, name)| *name == symbol)
                .map(|(idx, _)| *idx)
                .collect();
            // A root name the section could not record still binds its body: an
            // import asked the merge for it, so the symbol reaches that body as
            // much as it reaches the function the section names. Only a contested
            // one is held here, so this can only ever add a second carrier.
            if let Some(&idx) = self.contested_root_aliases.get(symbol.as_str()) {
                all.push(idx);
                all.sort_unstable();
            }
            let carriers = applicable_carriers(&all, &spec_funcs);
            match carriers[..] {
                [_one] => {}
                [] => {
                    return Err(LinkError::UnresolvedObligationSymbol {
                        symbol: symbol.clone(),
                        merged_roots: self.root_symbols.keys().cloned().collect(),
                    });
                }
                _ => {
                    return Err(LinkError::AmbiguousObligationSymbol {
                        symbol: symbol.clone(),
                        carriers: carriers
                            .iter()
                            .map(|&idx| self.describe_function(main, externals, symbol, idx))
                            .collect(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Where the output function at `idx` came from, in the terms the user
    /// wrote: their own program, an import they declared, or a private function
    /// of a module they linked against.
    ///
    /// A merged body is reported under the import field `symbol` names whenever
    /// that is one of its roots. A body satisfying several imports has several,
    /// and describing it under the arbitrary least of them would answer a
    /// question about one field by naming another.
    fn describe_function(
        &self,
        main: &ParsedModule,
        externals: &[ParsedModule],
        symbol: &str,
        idx: u32,
    ) -> String {
        let merged_base = self.merged_base(main);
        if idx < merged_base {
            return format!("the program's own function at index {idx}");
        }
        let bound_root = self
            .root_symbols
            .iter()
            .find(|(root, out)| **out == idx && root.as_str() == symbol)
            .or_else(|| self.root_symbols.iter().find(|(_, out)| **out == idx));
        if let Some((root, _)) = bound_root {
            return format!("the body merged to satisfy `{root}`, at index {idx}");
        }
        let module = self
            .merged
            .get((idx - merged_base) as usize)
            .and_then(|m| externals.get(m.external_idx))
            .map(|external| external.logical_module.as_str());
        match module {
            Some(module) => {
                format!("a private function of linked module `{module}`, at index {idx}")
            }
            None => format!("the function at index {idx}"),
        }
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
        // Identity: main's globals are emitted first and keep their source
        // indices, so no main-module body needs rewriting. The bounds check is
        // not redundant with that — it keeps the identity honest, so a main body
        // naming a global the module never declared errors here instead of
        // selecting whichever external's global landed at that index.
        let global = |idx: u32| {
            if (idx as usize) < main.globals.len() {
                Ok(idx)
            } else {
                Err(LinkError::Parse(format!(
                    "main body references global index {idx} out of range"
                )))
            }
        };
        let map = IndexMap {
            func: &func,
            ty: &ty,
            global: &global,
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
        let global_remap = &self.external_global_remap[external_idx];
        let global = |idx: u32| {
            global_remap.get(&idx).copied().ok_or_else(|| {
                LinkError::UnsupportedConstruct(format!(
                    "merged body references an unmapped global index {idx}"
                ))
            })
        };
        let map = IndexMap {
            func: &func,
            ty: &ty,
            global: &global,
        };
        let function = reencode_body(body, &map, BodyOrigin::External)?;
        if let Some(e) = func_err.into_inner() {
            return Err(e);
        }
        Ok(function)
    }
}

/// The warning owed to a merge that admits a Tier-B external into a linear
/// memory of more than one page, or `None` when it does not.
///
/// Tier B proves *derivation* — every address the external computes flows from a
/// parameter of the call — and not *containment*: it carries no sizes, so it
/// cannot show the address stays inside the buffer that parameter points into
/// (`p + q` and `2p` reach arbitrarily far with no constant at all, and both are
/// admitted). What has kept such a reach harmless is that memory beyond the
/// caller's buffer was usually beyond the memory itself, and trapped. That is an
/// accidental backstop, and every page above the first erodes it.
///
/// The condition is therefore the **reconciled** page count, not "the user asked
/// for more pages". A main module configured to two pages and a memoryless main
/// that adopts a seventeen-page external memory are equally exposed: the
/// out-of-buffer address lands in valid memory either way, and which module
/// enlarged it changes nothing about the reach. Rewriting this as a check on the
/// manifest would silence the second case, which is the harder one to see.
///
/// The count read is the reconciled *minimum* — the pages the instantiated
/// module addresses.
fn unbounded_reach_warning(
    tier_b_fields: &[String],
    reconciled: Option<&EncMemoryType>,
) -> Option<LinkWarning> {
    let pages = reconciled?.minimum;
    if tier_b_fields.is_empty() || pages <= 1 {
        return None;
    }
    Some(LinkWarning::TierBInMultiPageMemory {
        fields: tier_b_fields.to_vec(),
        pages,
    })
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

/// Points every function symbol the obligations *apply* at its entry in
/// `aliases`, and returns the distinct symbols left applied.
///
/// An applied symbol is a `T_app`'s or an `HA_app_ok`'s head — the two positions
/// in which an obligation names a function whose body the module must contain,
/// and so the two the merge has to answer for. An entry's own `fn_symbol` is
/// deliberately left alone here: for one of the main module's own entries it
/// names a specification function of the main module, which is never a merged
/// body, so no root alias can reach it; an adopted entry's is rewritten by
/// [`adopt_external_specs`] instead, which is the only producer that has the
/// logical module in hand.
///
/// An empty `aliases` makes this a pure collector, which is how the adoption
/// step learns what an obligation applies before it can resolve any of it.
///
/// The matches below are exhaustive on purpose. Both languages are a wire
/// format shared with the proof translator, and a variant added without a case
/// here would quietly stop being rewritten and stop being checked; a compile
/// error is the only notice that carries.
fn canonicalize_applied_symbols(
    map: &mut HSpecMap,
    aliases: &BTreeMap<&str, &str>,
) -> BTreeSet<String> {
    let mut applied = BTreeSet::new();
    for entries in map.values_mut() {
        for entry in entries {
            canonicalize_in_assert(&mut entry.hassert, aliases, &mut applied);
        }
    }
    applied
}

/// The assertion half of [`canonicalize_applied_symbols`]. Recursion is bounded
/// by the decoder's tree-depth cap, which every payload reaching here has
/// already passed.
fn canonicalize_in_assert(
    assert: &mut HAssert,
    aliases: &BTreeMap<&str, &str>,
    applied: &mut BTreeSet<String>,
) {
    match assert {
        HAssert::True | HAssert::False => {}
        HAssert::Not(inner) | HAssert::Ex(inner) | HAssert::All(inner) => {
            canonicalize_in_assert(inner, aliases, applied);
        }
        HAssert::And(left, right) | HAssert::Imp(left, right) | HAssert::Or(left, right) => {
            canonicalize_in_assert(left, aliases, applied);
            canonicalize_in_assert(right, aliases, applied);
        }
        HAssert::TermEq(left, right) => {
            canonicalize_in_term(left, aliases, applied);
            canonicalize_in_term(right, aliases, applied);
        }
        HAssert::HasType(term, _) | HAssert::Defined(term) => {
            canonicalize_in_term(term, aliases, applied);
        }
        HAssert::AppOk(symbol, args) => {
            canonicalize_symbol(symbol, aliases, applied);
            for arg in args {
                canonicalize_in_term(arg, aliases, applied);
            }
        }
    }
}

/// The term half of [`canonicalize_applied_symbols`].
fn canonicalize_in_term(
    term: &mut HTerm,
    aliases: &BTreeMap<&str, &str>,
    applied: &mut BTreeSet<String>,
) {
    match term {
        HTerm::Const(_) | HTerm::LVar(_) | HTerm::Local(_) => {}
        HTerm::App(symbol, args) => {
            canonicalize_symbol(symbol, aliases, applied);
            for arg in args {
                canonicalize_in_term(arg, aliases, applied);
            }
        }
        HTerm::Binop(_, _, left, right) | HTerm::Relop(_, _, left, right) => {
            canonicalize_in_term(left, aliases, applied);
            canonicalize_in_term(right, aliases, applied);
        }
    }
}

/// The output `name` section's function entries, ascending by output index:
/// main locals under their source debug names (re-indexed onto the import-free
/// output space), then each merged function under the name resolved at
/// plan-build time.
///
/// Taken as loose parts rather than a `&Plan` so the alias rewrite, which runs
/// while the plan is still being assembled, reads the same listing the encoder
/// will emit instead of rebuilding it.
fn name_section_entries<'a>(
    main: &'a ParsedModule,
    main_local_base: u32,
    merged: &'a [MergedFunc],
) -> Vec<(u32, &'a str)> {
    let import_count = main.imported_funcs.len() as u32;
    let mut entries: Vec<(u32, &str)> = Vec::new();

    for (local_idx, _) in main.local_funcs.iter().enumerate() {
        let source_idx = import_count + local_idx as u32;
        if let Some(name) = main.func_name(source_idx) {
            entries.push((main_local_base + local_idx as u32, name));
        }
    }
    let merged_base = main_local_base + main.local_funcs.len() as u32;
    for (i, m) in merged.iter().enumerate() {
        if let Some(name) = &m.name {
            entries.push((merged_base + i as u32, name));
        }
    }

    entries.sort_unstable_by_key(|(idx, _)| *idx);
    entries
}

/// Rewrites one applied symbol through `aliases` and records the result.
fn canonicalize_symbol(
    symbol: &mut HFnRef,
    aliases: &BTreeMap<&str, &str>,
    applied: &mut BTreeSet<String>,
) {
    if let Some(canonical) = aliases.get(symbol.0.as_str()) {
        symbol.0 = (*canonical).to_string();
    }
    applied.insert(symbol.0.clone());
}

/// The `name`-section symbol the output will carry for the merged body of
/// `(external_idx, source_func_idx)`, or `None` when the merge folded no body in
/// for that key.
///
/// The reader-side counterpart of [`MergedFunc::name`], which is the single
/// producer of every merged symbol: [`name_section_entries`] enumerates that
/// field for the encoder and for [`Plan::check_obligation_symbols`], and this
/// looks the same field up by source key for the adoption rewrite. Neither
/// formats a name of its own, so a symbol an adopted obligation is pointed at
/// cannot drift from the symbol the section records.
///
/// Every `MergedFunc::name` is `Some` by the time this is reachable — the
/// anonymous fallback fills the last of them — so a `None` here means the key
/// names no merged body, never a merged body with no name. The call site maps it
/// to exactly that fault.
///
/// Taken as loose parts rather than a `&Plan` for the same reason
/// [`name_section_entries`] is: the adoption rewrite runs while the plan is
/// still being assembled.
fn merged_output_symbol<'a>(
    merged: &'a [MergedFunc],
    merged_index: &BTreeMap<(usize, u32), u32>,
    merged_base: u32,
    key: (usize, u32),
) -> Option<&'a str> {
    let out_idx = *merged_index.get(&key)?;
    // Every value in `merged_index` is at or above `merged_base` by
    // construction, and `link` is reachable from arbitrary caller-supplied
    // bytes, so the invariant is enforced here rather than trusted.
    let slot = usize::try_from(out_idx.checked_sub(merged_base)?).ok()?;
    merged.get(slot).and_then(|m| m.name.as_deref())
}

/// The `inference.spec_funcs` / `inference.hspecs` key an external's
/// specification is adopted under: the logical module the external was bound
/// from, folded onto the specification's own (already file-folded) name.
///
/// `mathlib` + `DoubleSpec` → `mathlib_DoubleSpec`;
/// `a::b` + `DoubleSpec` → `a_b_DoubleSpec`.
///
/// The same fold code generation uses for a spec's defining file
/// (`inference_fn_key::fold_spec_name`), for the same reason: the result has to
/// be a plain Rocq identifier, so `_` is the only joiner available. That fold is
/// documented lossy, which is why every collision it admits is refused by the
/// caller rather than resolved.
///
/// An empty `logical_module` is not a case this handles — the fold would emit a
/// leading `_` and the key would not be namespaced at all — and the caller
/// refuses it before calling.
fn adopted_spec_key(logical_module: &str, spec: &str) -> String {
    let segments: Vec<String> = logical_module
        .split(inference_fn_key::MERGED_SEPARATOR)
        .map(str::to_string)
        .collect();
    inference_fn_key::fold_spec_name(&segments, spec)
}

/// Why `name` cannot be a specification name in the emitted proof, or `None`
/// when it can.
///
/// The proof translator turns a spec name into Rocq identifiers
/// (`<module>__<spec>_specs`, `valid_<module>__<spec>`, …), so a name it cannot
/// spell has to be refused before the merge mints it. That rule lives in
/// `wasm-to-v`, which sits *above* this crate, so its structural half is
/// restated here and pinned to the original by a test.
///
/// The Rocq stdlib / keyword denylist is deliberately **not** restated. It is a
/// list, not a rule: it changes on the translator's schedule, and a second copy
/// here would drift into admitting a name the translator rejects (a link that
/// fails one phase later, with a worse message) or into rejecting one it admits
/// (a link that fails for no reason at all). A key colliding with a reserved
/// name is still fail-closed — the translator validates every key of the
/// embedded `inference.spec_funcs` section, in the same `infc -v` invocation,
/// before any artifact is written — and both directions of that split are pinned
/// by test so it cannot become a hole silently.
///
/// The clauses are checked in the translator's own order, so a name breaking
/// several rules is reported here under the same clause the translator would
/// report. The trailing-`_` clause is last, after the length cap, for exactly
/// that reason; it is owned here alone because the downstream rule that enforces
/// it is crate-private to `wasm-to-v`, so nothing outside that crate can observe
/// it.
///
/// Each clause is a sentence fragment, lowercase and unpunctuated, so a caller
/// can set it inside its own message.
fn spec_name_problem(name: &str) -> Option<String> {
    if name.is_empty() {
        return Some("it is empty".to_string());
    }
    let mut chars = name.chars();
    let first = chars.next().expect("non-empty checked above");
    if !first.is_ascii_alphabetic() {
        return Some(format!(
            "it starts with `{first}`, and a generated identifier must start with an ASCII letter"
        ));
    }
    for c in chars {
        if !(c.is_ascii_alphanumeric() || c == '_') {
            return Some(format!(
                "it contains `{c}`, and a generated identifier admits only ASCII letters, \
                 digits and `_`"
            ));
        }
    }
    if name.contains("__") {
        return Some(
            "it contains a `__` run, which the generated `<module>__<spec>` grammar reserves"
                .to_string(),
        );
    }
    if name.len() > MAX_SPEC_NAME_BYTES {
        return Some(format!(
            "it is {} bytes long, past the {MAX_SPEC_NAME_BYTES}-byte limit on a generated \
             identifier",
            name.len()
        ));
    }
    if name.ends_with('_') {
        return Some(
            "it ends with `_`, which the generated `<module>__<spec>` grammar reserves"
                .to_string(),
        );
    }
    None
}

/// The byte length a generated Rocq identifier may not exceed, restated from the
/// proof translator's own cap and pinned to it by test.
const MAX_SPEC_NAME_BYTES: usize = 255;

/// What adoption contributes to a plan: the specifications to fold into the
/// merged module's own sections, the symbols they apply, and what was left
/// behind.
struct AdoptedSpecs {
    /// `(adopted key, universal obligations)` in mint order. Every key is
    /// distinct; the caller inserts them into the merged obligation map and
    /// records each key for the `inference.spec_funcs` section.
    specs: Vec<(String, Vec<inference_hassert::HSpecEntry>)>,
    /// Every function symbol the adopted obligations apply, after the rewrite
    /// onto merged bodies — the set the post-merge obligation check must resolve
    /// alongside the program's own.
    symbols: BTreeSet<String>,
    /// One entry per contributing library that shipped reachability obligations,
    /// ascending by logical module.
    warnings: Vec<LinkWarning>,
}

/// Reports every contributing library whose own proof obligations this merge did
/// not carry into the output.
///
/// Keyed on the presence of an `inference.hspecs` section alone: a library
/// carrying only `inference.spec_funcs` records spec membership — indices of its
/// own specification functions, which no export closure reaches — and loses
/// nothing worth reporting. Presence is all this can report, because under this
/// policy the section is never decoded, and that is the point of the policy.
///
/// Scoped to libraries that supplied at least one merged body. One nothing
/// imports from is not part of the artifact, so nothing about it was dropped in
/// a sense the reader can act on, and reporting it would fire on every unrelated
/// dependency of a program that links one.
fn external_spec_warning(
    externals: &[ParsedModule],
    merged_index: &BTreeMap<(usize, u32), u32>,
) -> Option<LinkWarning> {
    let modules: BTreeSet<&str> = externals
        .iter()
        .enumerate()
        .filter(|(ext_idx, external)| {
            external.carries_hspecs && contributes_a_body(*ext_idx, merged_index)
        })
        .map(|(_, external)| external.logical_module.as_str())
        .collect();
    if modules.is_empty() {
        return None;
    }
    Some(LinkWarning::ExternalSpecsDropped {
        modules: modules.into_iter().map(str::to_string).collect(),
    })
}

/// Whether the merge folded at least one body in from the external at
/// `ext_idx`.
fn contributes_a_body(ext_idx: usize, merged_index: &BTreeMap<(usize, u32), u32>) -> bool {
    merged_index.keys().any(|(idx, _)| *idx == ext_idx)
}

/// The carriers of an applied obligation symbol that could legitimately be its
/// target, narrowed from every function of that name in a module's `name`
/// section by dropping the ones `spec_funcs` records as specification
/// functions.
///
/// A specification function's name-section symbol is deliberately left
/// *unqualified* by its defining file — spec membership travels as indices in
/// `inference.spec_funcs` — so a spec-inner `fn helper` and the module's own
/// `fn helper` really do share one string. That coincidence is not an
/// ambiguity: no obligation may apply a specification function at all, so the
/// spec carriers are not candidates and dropping them leaves the one function
/// the symbol can mean. When *every* carrier is a specification function the
/// full set stands, so the rejection downstream reports the real count rather
/// than "nothing carries the name".
///
/// This is the same rule the proof translator applies to the merged module's
/// own obligations (`applicable_carriers` in `core/wasm-to-v/src/translator.rs`,
/// over its `FuncRemap`), reached here over a library's own decoded section
/// because a library's obligations are resolved before the translator ever
/// sees them. The two cannot be allowed to drift: this pass hands the
/// translator a symbol already rewritten onto a merged body, so a narrowing
/// only one of them performed would either reject a library the translator
/// resolves correctly, or resolve one it would refuse.
fn applicable_carriers(named: &[u32], spec_funcs: &BTreeSet<u32>) -> Vec<u32> {
    let applicable: Vec<u32> = named
        .iter()
        .copied()
        .filter(|idx| !spec_funcs.contains(idx))
        .collect();
    if applicable.is_empty() {
        return named.to_vec();
    }
    applicable
}

/// Carries each contributing library's **universal** obligations into the merged
/// module's own verification sections, resolving every symbol they apply onto
/// the merged body it names.
///
/// An applied symbol is resolved against the library's whole `name` section
/// narrowed by [`applicable_carriers`], which is the translator's rule: a
/// library's specification functions carry unqualified names, so a library that
/// declares `fn scale` and states a specification whose inner function is also
/// named `scale` names two functions `scale` and only one of them is a body an
/// obligation can be about. The type checker permits that pair whenever the two
/// live in different files of the library, so it is reachable from ordinary
/// source rather than only from hand-built bytes.
///
/// Two ordering facts this relies on, both established by its caller:
///
/// * it runs **after** the anonymous-name fallback, because
///   [`merged_output_symbol`] reads final `MergedFunc::name` values;
/// * it runs **before** the post-merge obligation check, because the symbols it
///   returns must be in that check's input.
///
/// An adopted symbol is not a candidate for the contested-alias treatment. A
/// contested alias is a string whose *intended* referent (the body an import
/// bound under that field) differs from the string's *resolvable* referent (the
/// function the name section records under it); the ambiguity is a property of a
/// human-written obligation naming an export field. An adopted symbol has no
/// such gap: it is not written by anyone, it is produced by
/// [`merged_output_symbol`] from the exact `MergedFunc::name` the section will
/// carry, so its intended referent *is* its resolvable referent by construction.
/// Recording a contested alias for it would add a second carrier to a symbol
/// that names exactly one body, rejecting a link the translator resolves
/// correctly. What genuinely is dangerous — two merged bodies carrying one
/// name-section string — is caught anyway, because the post-merge check counts
/// every name-section entry matching the symbol.
///
/// Entries are partitioned by kind **first**, and the key is computed only if a
/// universal entry survives. Doing it the other way round would let a
/// specification this pass declines to adopt hard-fail a link on an invalid or
/// colliding key that would never be minted.
///
/// # Errors
///
/// Every way an adoption can be refused, each naming the library and the
/// specification: the library's own two sections disagreeing, a key the proof
/// translation cannot spell or that is already claimed, an applied symbol the
/// library's own `name` section carries on no function or on several, an
/// obligation over a body this merge did not fold in, and an adopted
/// specification symbol the merged output already carries.
fn adopt_external_specs(
    main: &ParsedModule,
    externals: &[ParsedModule],
    merged: &[MergedFunc],
    merged_index: &BTreeMap<(usize, u32), u32>,
    merged_base: u32,
    carried: &BTreeSet<&str>,
) -> Result<AdoptedSpecs, LinkError> {
    let mut claimed: BTreeSet<&str> = BTreeSet::new();
    if let Some(spec_funcs) = &main.spec_funcs {
        claimed.extend(spec_funcs.iter().map(|(name, _)| name.as_str()));
    }
    if let Some(hspecs) = &main.hspecs {
        claimed.extend(hspecs.keys().map(String::as_str));
    }

    let mut adopted = AdoptedSpecs {
        specs: Vec::new(),
        symbols: BTreeSet::new(),
        warnings: Vec::new(),
    };
    // `key -> the logical module that minted it`, so a second library folding to
    // the same key can name the first in its rejection.
    let mut minted_by: BTreeMap<String, String> = BTreeMap::new();
    // `(module, specs adopted from it, obligations left behind)`. The count is
    // what tells a partial adoption from one that carried nothing at all, which
    // the report has to say out loud: a library whose every obligation is a
    // reachability obligation mints no key, and a reader told only what was left
    // behind would read the rest as having been adopted.
    let mut left_behind: Vec<(&str, usize, Vec<String>)> = Vec::new();

    for (ext_idx, external) in externals.iter().enumerate() {
        if !contributes_a_body(ext_idx, merged_index) {
            continue;
        }
        let Some(hspecs) = &external.hspecs else {
            continue;
        };
        let module = external.logical_module.as_str();
        let listed: BTreeSet<&str> = external
            .spec_funcs
            .iter()
            .flatten()
            .map(|(name, _)| name.as_str())
            .collect();
        let library_spec_funcs: BTreeSet<u32> = external
            .spec_funcs
            .iter()
            .flatten()
            .flat_map(|(_, indices)| indices.iter().copied())
            .collect();
        let mut dropped: Vec<String> = Vec::new();
        let already_adopted = adopted.specs.len();

        // The obligation map is an `FxHashMap`, so the walk is sorted
        // explicitly: every diagnostic and every warning must be reproducible.
        let mut spec_names: Vec<&String> = hspecs.keys().collect();
        spec_names.sort_unstable();
        for spec in spec_names {
            if !listed.contains(spec.as_str()) {
                return Err(LinkError::AdoptedSpecUnlisted {
                    module: module.to_string(),
                    spec: spec.clone(),
                });
            }
            let mut universal: Vec<inference_hassert::HSpecEntry> = Vec::new();
            for entry in &hspecs[spec] {
                let kind = match entry.kind {
                    inference_hassert::SpecKind::Forall => {
                        universal.push(entry.clone());
                        continue;
                    }
                    inference_hassert::SpecKind::Exists(_) => "exists",
                    inference_hassert::SpecKind::Unique(_) => "unique",
                };
                dropped.push(format!("`{spec}` / `{}` ({kind})", entry.fn_symbol.0));
            }
            if universal.is_empty() {
                // A specification whose obligations are all reachability
                // obligations mints no key at all. The alternative is a
                // `ValidSpec` over an empty list, a theorem trivially true of
                // every module and stating nothing about the program; a vacuous
                // obligation that discharges is worse than an absent one.
                continue;
            }

            if module.is_empty() {
                return Err(LinkError::AdoptedSpecNameInvalid {
                    module: module.to_string(),
                    spec: spec.clone(),
                    key: spec.clone(),
                    reason: "the external was bound under an empty logical module, so its \
                             specifications cannot be namespaced apart from the program's own"
                        .to_string(),
                });
            }
            let key = adopted_spec_key(module, spec);
            if let Some(reason) = spec_name_problem(&key) {
                return Err(LinkError::AdoptedSpecNameInvalid {
                    module: module.to_string(),
                    spec: spec.clone(),
                    key,
                    reason,
                });
            }
            if claimed.contains(key.as_str()) {
                return Err(LinkError::AdoptedSpecNameCollision {
                    spec: key,
                    module: module.to_string(),
                    contender: None,
                });
            }
            if let Some(first) = minted_by.get(&key) {
                return Err(LinkError::AdoptedSpecNameCollision {
                    spec: key.clone(),
                    module: first.clone(),
                    contender: Some(module.to_string()),
                });
            }

            for entry in &mut universal {
                let symbol = inference_fn_key::merged_name::adopted_spec(module, &entry.fn_symbol.0);
                if carried.contains(symbol.as_str()) {
                    return Err(LinkError::AdoptedSpecSymbolCollision {
                        module: module.to_string(),
                        spec: spec.clone(),
                        symbol,
                    });
                }
                entry.fn_symbol = HFnRef(symbol);
            }

            let mut pending = HSpecMap::default();
            pending.insert(key.clone(), universal);
            // Two passes over the same walker rather than a second traversal of
            // the same two languages: an empty alias map collects what the
            // obligations apply, resolution happens between the passes in
            // ordinary control flow, and the second pass rewrites.
            let applied = canonicalize_applied_symbols(&mut pending, &BTreeMap::new());
            let mut aliases: BTreeMap<&str, &str> = BTreeMap::new();
            for symbol in &applied {
                let named: Vec<u32> = external
                    .func_names
                    .iter()
                    .filter(|(_, name)| name.as_str() == symbol.as_str())
                    .map(|(idx, _)| *idx)
                    .collect();
                let carriers = applicable_carriers(&named, &library_spec_funcs);
                let [src_idx] = carriers[..] else {
                    if carriers.is_empty() {
                        return Err(LinkError::AdoptedObligationSymbolUnresolved {
                            module: module.to_string(),
                            spec: spec.clone(),
                            symbol: symbol.clone(),
                        });
                    }
                    return Err(LinkError::AdoptedObligationSymbolAmbiguous {
                        module: module.to_string(),
                        spec: spec.clone(),
                        symbol: symbol.clone(),
                        carriers,
                    });
                };
                let Some(output) =
                    merged_output_symbol(merged, merged_index, merged_base, (ext_idx, src_idx))
                else {
                    return Err(LinkError::AdoptedObligationUnmergedSymbol {
                        module: module.to_string(),
                        spec: spec.clone(),
                        symbol: symbol.clone(),
                        imported: src_idx < external.local_func_base(),
                    });
                };
                aliases.insert(symbol.as_str(), output);
            }
            adopted
                .symbols
                .extend(canonicalize_applied_symbols(&mut pending, &aliases));

            let entries = pending
                .remove(&key)
                .expect("the pending map holds exactly the key just inserted");
            minted_by.insert(key.clone(), module.to_string());
            adopted.specs.push((key, entries));
        }

        if !dropped.is_empty() {
            left_behind.push((module, adopted.specs.len() - already_adopted, dropped));
        }
    }

    left_behind.sort_by_key(|(module, _, _)| *module);
    adopted.warnings = left_behind
        .into_iter()
        .map(
            |(module, count, obligations)| LinkWarning::ReachabilityObligationsNotAdopted {
                module: module.to_string(),
                adopted: count,
                obligations,
            },
        )
        .collect();
    Ok(adopted)
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
/// - An external whose closure **never touches memory** contributes nothing,
///   whatever its module declares. Its memory section describes the machine it
///   was compiled for, not one the merged output has to provide; see
///   [`MemoryReconciler::fold`] for why dropping it is unobservable.
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
    /// `uses_memory`/`uses_memory_grow` are the external closure's effects.
    /// `uses_memory` decides both whether a memory is required at all and
    /// whether this external's own declaration contributes to the reconciled
    /// result; `uses_memory_grow` decides whether growth must be admitted. The
    /// two are not independent: every operator that grows memory also uses it,
    /// so the growth check never runs on an external whose limits were dropped.
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
            // Reject an unsupported memory shape for *every* declared external
            // memory, adopted or not, including the `None => ext` adopt path onto
            // a memoryless main — otherwise a memory64/shared/custom-page external
            // would be adopted verbatim and wasm-to-v would silently re-encode it
            // as a 32-bit memory (audit C-4/L-1). The check stays outside the
            // adoption gate below: it costs nothing on the dropped path and keeps
            // the rejection absolute rather than conditional on an effect flag.
            reject_unsupported_memory_shape(ext_mem, field)?;

            // Adopt the external's declared limits only when its closure actually
            // addresses memory. A module's memory section describes the machine
            // *that module* was built for; it becomes a fact about the merged
            // output only if some merged body reads it. Folding it in
            // unconditionally let a pure function impose its module's page count
            // on the output — visible in the emitted `.wasm` and restated in the
            // paired `.v` as the machine the proof is about — and, against a main
            // that pins its own bound, rejected the link outright over a memory
            // nothing would have touched.
            //
            // Dropping it is unobservable, on three facts that must hold together:
            //
            // - `uses_memory` is *closure-scoped*. `closure::compute` accumulates
            //   it in `scan_body` over exactly the functions it returns in
            //   `local_func_indices` — the same set the merge copies below — so
            //   `false` means no body that reaches the output contains a memory
            //   operator. This is the argument `tier` already relies on to drop an
            //   external's globals and tables.
            // - Every operator family that reaches linear memory sets it:
            //   integer load/store, `memory.copy`, `memory.fill`, `memory.init`,
            //   and `memory.size`/`memory.grow`. The last two are the ones worth
            //   spelling out — they yield or extend a *page count* rather than
            //   addressing a byte, so they read as unrelated to a memory's limits
            //   when they are in fact the operators that observe them most
            //   directly: `memory.size` returns the reconciled minimum, and
            //   `memory.grow` is answered by the reconciled maximum. Both count as
            //   use, and both are excluded here when `uses_memory` is false.
            //   Anything outside the allow-list is rejected before it could reach
            //   a body, so no unlisted operator can address memory silently.
            // - No dropped memory is written behind the closure's back at
            //   instantiation: `tier::classify` runs before this fold and rejects
            //   any external declaring a data segment, so an external reaching
            //   here has none to initialize the memory being dropped.
            if uses_memory {
                let ext = to_enc_memory(ext_mem);
                self.current = Some(match self.current {
                    None => ext,
                    Some(cur) => reconcile_pair(cur, ext, field)?,
                });
            }
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
/// rejected rather than emitting an invalid `min > max` memory. That rejection
/// names the page-count knob: the fix lies on the main module's side, which the
/// author is unlikely to guess from a diagnostic that only reports the two
/// numbers.
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
                 is not relaxed. Give the main module a memory large enough to hold the \
                 external: `pages` in the `[memory]` table of `Inference.toml`, or \
                 `infc --memory-pages <N>`"
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
    /// The table the three identifier-rule pins share: names the linker admits,
    /// one name per structural clause it refuses, and the reserved names whose
    /// treatment is the documented carve-out.
    fn identifier_pin_table() -> Vec<&'static str> {
        vec![
            // Admitted by both rules.
            "mathlib_DoubleSpec",
            "a_b_S",
            "S",
            "Spec2",
            // One per structural clause.
            "",
            "_leading",
            "9leading",
            "has.dot",
            "a__b",
            "trailing_",
            // The carve-out: syntactically legal, refused by the translator's
            // stdlib/keyword denylist alone.
            "eq_refl",
            "well_founded",
            "nat",
            "Qed",
            // Legal, and *not* denylisted — the neighbourhood of `Spec_` without
            // its trailing underscore.
            "Spec",
        ]
    }

    /// The pin that matters: the linker must never mint a key the proof
    /// translation refuses for a **structural** reason. The linker's rule is a
    /// restatement of the translator's, so an implication in this direction is
    /// what keeps the restatement honest; the reverse direction is the
    /// deliberate carve-out below.
    #[test]
    fn every_key_the_linker_admits_wasm_to_v_admits() {
        use inference_wasm_to_v_translator::errors::{InvalidIdentifierReason, WasmToVError};
        use inference_wasm_to_v_translator::rocq_names::validate_rocq_identifier;

        for name in identifier_pin_table() {
            if spec_name_problem(name).is_some() {
                continue;
            }
            match validate_rocq_identifier(name) {
                Ok(()) => {}
                Err(WasmToVError::RocqStdlibShadow { .. })
                | Err(WasmToVError::InvalidRocqIdentifier {
                    reason: InvalidIdentifierReason::ReservedKeyword,
                    ..
                }) => {}
                Err(other) => panic!(
                    "the linker admits `{name}` for a structural reason the translator refuses: \
                     {other}"
                ),
            }
        }
    }

    /// The carve-out, pinned in both directions so it cannot widen silently: the
    /// linker deliberately does not restate the translator's stdlib/keyword
    /// denylist, and the only errors that split the two rules are those two.
    #[test]
    fn reserved_names_pass_the_linker_rule_and_fail_the_translator_rule() {
        use inference_wasm_to_v_translator::errors::{InvalidIdentifierReason, WasmToVError};
        use inference_wasm_to_v_translator::rocq_names::validate_rocq_identifier;

        for name in ["eq_refl", "well_founded", "nat", "Qed"] {
            assert_eq!(
                spec_name_problem(name),
                None,
                "the linker's rule is structural only and must admit `{name}`"
            );
            let err = validate_rocq_identifier(name)
                .expect_err("the translator's denylist must refuse `{name}`");
            assert!(
                matches!(
                    err,
                    WasmToVError::RocqStdlibShadow { .. }
                        | WasmToVError::InvalidRocqIdentifier {
                            reason: InvalidIdentifierReason::ReservedKeyword,
                            ..
                        }
                ),
                "the deferred half of the rule is the denylist alone, got {err} for `{name}`"
            );
        }
    }

    /// A trailing `_` is the one clause the linker owns outright: the
    /// translator's identifier rule admits it, and the rule that does refuse it
    /// downstream is crate-private there, so no test outside that crate can
    /// observe it. Refusing at the link is also what lets the message name the
    /// library and the specification, which the translator's cannot.
    #[test]
    fn a_trailing_underscore_key_is_rejected_here_though_the_identifier_rule_admits_it() {
        use inference_wasm_to_v_translator::rocq_names::validate_rocq_identifier;

        assert!(
            validate_rocq_identifier("mathlib_Double_").is_ok(),
            "the translator's identifier rule has no trailing-underscore clause"
        );
        let reason = spec_name_problem("mathlib_Double_")
            .expect("the linker must refuse a key ending in `_`");
        assert!(
            reason.contains("ends with `_`"),
            "the clause must name the defect, got {reason}"
        );
    }

    /// Each clause's own text, not merely that something was refused: the clause
    /// is set verbatim inside a user-facing rejection, and the order is the
    /// translator's, so a name breaking several rules is reported under the
    /// clause the translator would report.
    #[test]
    fn spec_name_problem_names_each_structural_defect() {
        assert_eq!(spec_name_problem(""), Some("it is empty".to_string()));
        assert_eq!(
            spec_name_problem("_leading"),
            Some(
                "it starts with `_`, and a generated identifier must start with an ASCII letter"
                    .to_string()
            )
        );
        assert_eq!(
            spec_name_problem("has.dot"),
            Some(
                "it contains `.`, and a generated identifier admits only ASCII letters, digits \
                 and `_`"
                    .to_string()
            )
        );
        assert_eq!(
            spec_name_problem("a__b"),
            Some(
                "it contains a `__` run, which the generated `<module>__<spec>` grammar reserves"
                    .to_string()
            )
        );
        let over_long = "a".repeat(MAX_SPEC_NAME_BYTES + 1);
        assert_eq!(
            spec_name_problem(&over_long),
            Some(format!(
                "it is {} bytes long, past the 255-byte limit on a generated identifier",
                MAX_SPEC_NAME_BYTES + 1
            ))
        );
        assert_eq!(
            spec_name_problem("trailing_"),
            Some(
                "it ends with `_`, which the generated `<module>__<spec>` grammar reserves"
                    .to_string()
            )
        );
        // The cap is checked before the trailing-`_` clause, so an over-long name
        // ending in `_` reports the clause the translator would.
        let over_long_underscore = format!("{}_", "a".repeat(MAX_SPEC_NAME_BYTES));
        assert!(
            spec_name_problem(&over_long_underscore)
                .expect("refused")
                .contains("bytes long"),
            "the length cap must be reported ahead of the trailing-underscore clause"
        );
        assert_eq!(spec_name_problem("mathlib_DoubleSpec"), None);
    }

    /// The `name` section is one field, `MergedFunc::name`, written by the plan
    /// and read by the encoder. The adoption rewrite must read that same field,
    /// so a symbol an adopted obligation is pointed at cannot drift from the
    /// symbol the section records. All three producers are covered: a canonical
    /// root, a marked inner callee, and the index-derived fallback.
    #[test]
    fn merged_output_symbol_reads_the_name_the_section_will_carry() {
        let main = ParsedModule::default();
        let merged = vec![
            MergedFunc {
                external_idx: 0,
                source_func_idx: 4,
                out_type_idx: 0,
                name: Some(inference_fn_key::merged_name::root("mathlib", "double")),
            },
            MergedFunc {
                external_idx: 0,
                source_func_idx: 5,
                out_type_idx: 0,
                name: Some(inference_fn_key::merged_name::callee("mathlib", "helper")),
            },
            MergedFunc {
                external_idx: 1,
                source_func_idx: 2,
                out_type_idx: 0,
                name: Some(inference_fn_key::merged_name::anonymous("crypto", 2)),
            },
        ];
        let merged_base = 0;
        let merged_index: BTreeMap<(usize, u32), u32> =
            [((0, 4), 0), ((0, 5), 1), ((1, 2), 2)].into_iter().collect();

        let section = name_section_entries(&main, merged_base, &merged);
        for (key, out_idx) in &merged_index {
            let recorded = section
                .iter()
                .find(|(idx, _)| idx == out_idx)
                .map(|(_, name)| *name)
                .expect("every merged body is named by the time this runs");
            assert_eq!(
                merged_output_symbol(&merged, &merged_index, merged_base, *key),
                Some(recorded),
                "the lookup must agree with the section for {key:?}"
            );
        }
    }

    /// `None` means the key names no merged body, never a merged body with no
    /// name — which is the single fault the adoption call site reports. The
    /// below-base key additionally exercises the guard that keeps an index the
    /// caller supplied from underflowing into a wrong slot.
    #[test]
    fn merged_output_symbol_is_none_for_a_key_outside_the_merge() {
        let merged = vec![MergedFunc {
            external_idx: 0,
            source_func_idx: 4,
            out_type_idx: 0,
            name: Some(inference_fn_key::merged_name::root("mathlib", "double")),
        }];
        let merged_base = 7;
        let merged_index: BTreeMap<(usize, u32), u32> =
            [((0, 4), 7), ((0, 9), 3)].into_iter().collect();

        assert_eq!(
            merged_output_symbol(&merged, &merged_index, merged_base, (0, 3)),
            None,
            "a key the merge folded nothing in for names no body"
        );
        assert_eq!(
            merged_output_symbol(&merged, &merged_index, merged_base, (0, 9)),
            None,
            "an output index below the merged base cannot address a merged slot"
        );
        assert_eq!(
            merged_output_symbol(&merged, &merged_index, merged_base, (0, 4)),
            Some("mathlib::double")
        );
    }

    /// The adopted key namespaces a library's specification under the logical
    /// module the program bound it as, and a `::`-joined module contributes each
    /// of its segments — the same fold code generation applies to a spec's
    /// defining file, so both reach a plain Rocq identifier the same way.
    #[test]
    fn adopted_spec_key_folds_the_logical_module_segments() {
        assert_eq!(
            adopted_spec_key("mathlib", "DoubleSpec"),
            "mathlib_DoubleSpec"
        );
        assert_eq!(adopted_spec_key("a::b", "S"), "a_b_S");
        assert_eq!(adopted_spec_key("a::b::c", "S"), "a_b_c_S");
    }
}
