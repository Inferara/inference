//! Rocq Code Generation from Parsed WASM Data
//!
//! This module provides the translation phase (Phase 2) of WASM to Rocq conversion.
//! It converts structured WASM data (from [`crate::wasm_parser`]) into Rocq (Coq)
//! formal verification code.
//!
//! ## Overview
//!
//! The translator takes [`WasmParseData`] populated during parsing and generates a
//! complete Rocq module that represents the WASM module in a form suitable for
//! formal verification using the Rocq proof assistant.
//!
//! ## Translation Process
//!
//! The translation happens in [`WasmParseData::translate`] through these steps:
//!
//! 1. **Generate Header**: Rocq imports from standard libraries (`List`, `String`, `BinNat`, `ZArith`, `Wasm`)
//! 2. **Generate Helpers**: Convenience constructors (`Vi32`, `Vi64`, `Mt`, `Mm`, `Mg`, `Mi`, `Me`, `Ma`)
//! 3. **Translate Imports**: External dependencies → `Mi` records (module, name, descriptor)
//! 4. **Translate Exports**: Public interface → `Me` records (name, descriptor)
//! 5. **Translate Tables**: Indirect call tables → `Mt` definitions (limits, element type)
//! 6. **Translate Memory**: Linear memory → `Mm` definitions (size limits)
//! 7. **Translate Globals**: Global variables → `Mg` definitions (type, mutability, initialization)
//! 8. **Translate Data**: Memory initialization → data segment records
//! 9. **Translate Elements**: Table initialization → element segment records
//! 10. **Translate Types**: Function signatures → Rocq function type definitions
//! 11. **Translate Functions**: Function bodies → `module_func` definitions with locals and instructions
//! 12. **Generate Module**: Assemble all components into final `module` record
//!
//! ## Code Generation Strategy
//!
//! The translator generates Rocq code as strings using helper functions for each
//! WASM construct. This string-based approach prioritizes:
//!
//! - **Correctness**: Mapping from WASM semantics to Rocq types
//! - **Readability**: Well-formatted output with consistent indentation
//! - **Debuggability**: Preserve names from WASM custom sections
//! - **Simplicity**: Easy to understand and maintain translation logic
//!
//! ## Index Immediates
//!
//! The proof contract types every index immediate as `N` — branch depths, function,
//! type, table, memory, global, local and data-segment indexes, `BI_br_table`'s label
//! list and default, and the function indexes an element segment initializes a table
//! with. Each is emitted with an explicit `%N` scope.
//!
//! Rocq's numeral notation is type-directed, so a bare numeral elaborates correctly
//! wherever the expected type is already known, and a bare spelling is accepted today.
//! That inference is the only thing making it correct, though, so a bare operand is
//! silently fine until a contract or notation change loses the inference — at which
//! point it breaks at the prover rather than in-repo. Writing the scope at every site
//! makes the emitted term independent of the surrounding notation scope, and keeps
//! structurally identical operands from being spelled two different ways.
//!
//! ## Expression Translation
//!
//! WASM's stack-based instruction model is converted to structured Rocq expression lists.
//! The translator reconstructs control flow from linear instruction sequences:
//!
//! ```text
//! WASM (stack-based)          Rocq (structured)
//! ──────────────────          ─────────────────
//! local.get 0                 BI_get_local 0%N ::
//! local.get 1                 BI_get_local 1%N ::
//! i32.add                     BI_binop (Binop_i BOI_add) ::
//!                             nil
//! ```
//!
//! ### Expression Reconstruction
//!
//! Control flow is reconstructed using helper structures:
//!
//! - [`Expression`] - Represents a sequence of WASM instructions as Rocq expressions
//! - [`BlockExpr`] - Represents a structured block with type and body
//! - [`ConditionExpr`] - Represents an if-then-else conditional
//! - [`ExpressionPart`] - Discriminated union for different expression types
//!
//! These structures enable proper nesting and scoping of Rocq expressions.
//!
//! ## Helper Definitions
//!
//! The translator generates these Rocq helper definitions at the top of every file
//! to simplify generated code:
//!
//! - `Vi32 i`: Construct i32 value from integer literal
//! - `Vi64 i`: Construct i64 value from integer literal
//! - `Mt l et`: Construct table type with limits and element type
//! - `Mm l`: Construct memory type with limits
//! - `Mg mut t init`: Construct global with mutability, type, and initializer
//! - `Mi m n d`: Construct import with module name, import name, and descriptor
//! - `Me n d`: Construct export with name and descriptor
//! - `Ma of al`: Construct memory argument with offset and alignment
//!
//! ## Translation Functions
//!
//! The module provides numerous translation functions, organized by WASM construct:
//!
//! ### Type Translation
//! - `translate_ref_type` - Reference types (funcref, externref)
//! - `translate_value_type` - Value types (i32, i64; f32, f64, and v128 are rejected)
//! - `translate_block_type` - Block result types
//! - `translate_function_type` - Function signatures from RecGroup
//!
//! ### Section Translation
//! - `translate_module_import` - Import section entries
//! - `translate_export_module` - Export section entries
//! - `translate_table_type` - Table definitions
//! - `translate_memory_type` - Memory definitions
//! - `translate_global` - Global variable definitions
//! - `translate_data` - Data segment definitions
//! - `translate_element` - Element segment definitions
//!
//! ### Instruction Translation
//! - `translate_expression` - Main expression translation entry point
//! - `translate_expr` - Recursive expression builder
//! - `translate_basic_operator` - Individual WASM operators
//! - `translate_memarg` - Memory operation arguments
//!
//! ## Error Recovery
//!
//! Unlike the parser (which fails fast), the translator uses **error recovery**:
//!
//! 1. Collect translation errors from all sections into a `Vec<anyhow::Error>`
//! 2. Continue translating remaining sections even after errors
//! 3. Return the first error only if translation failed
//!
//! This approach provides better diagnostics by showing multiple related errors
//! instead of requiring users to fix one error at a time.
//!
//! ## Name Generation
//!
//! Generated Rocq identifiers follow these rules:
//!
//! - **Named functions**: Use names from custom name section if available
//! - **Anonymous functions**: Deterministically named `func_<index>` from the
//!   output function index, so the `.v` is reproducible for identical input
//! - **Module name**: Use name from custom section, or parameter to `translate_bytes`
//!
//! ## Output Format
//!
//! The generated Rocq file has this structure:
//!
//! ```coq
//! (* Standard library imports *)
//! Require Import List.
//! Require Import String.
//! Require Import BinNat.
//! Require Import ZArith.
//! From Wasm Require Import bytes numerics datatypes host.
//! From WasmVerifier Require Import Assertions Verifier.
//! (* The proof-contract import line gains ` Exists` when the module carries a
//!    reachability (`exists`/`unique`) obligation, and `Open Scope byte_scope.`
//!    follows when the module carries a data segment. *)
//!
//! (* Helper definitions *)
//! Definition Vi32 i := ...
//! Definition Vi64 i := ...
//! (* ... more helpers ... *)
//!
//! (* Function definitions *)
//! Definition func_0 : module_func := ...
//! Definition func_1 : module_func := ...
//!
//! (* Module record *)
//! Definition module_name : module := {|
//!   mod_types := ...;
//!   mod_funcs := ...;
//!   mod_tables := ...;
//!   mod_mems := ...;
//!   mod_globals := ...;
//!   mod_elems := ...;
//!   mod_datas := ...;
//!   mod_start := ...;
//!   mod_imports := ...;
//!   mod_exports := ...;
//! |}.
//! ```

use std::collections::HashMap;

use inf_wasmparser::{
    BlockType, CompositeInnerType, Data, DataKind, Element, ElementItems, ElementKind, Export,
    FunctionBody, Global, Import, MemoryType, Operator, OperatorsIterator, OperatorsReader,
    RecGroup, RefType, Table, TableType, TypeRef, ValType as wpValType,
};
use inference_hassert::{HSpecEntry, HSpecMap, ReachMeta, SpecKind};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::errors::WasmToVError;
use crate::gallina::z_literal;
use crate::hassert_print;

const LCB: &str = "{|\n";
const RCB_DOT: &str = "|}.\n";

const LIST_EXT: &str = " ::\n";
const LIST_SEAL: &str = "nil";

/// The function-index renumbering the spec-function omission forces on the
/// emitted module, together with the retention set the reachability kinds add.
///
/// A `forall`/plain `spec` function is a downstream contract obligation, not
/// part of the executable module, so it is dropped from the `.v` module
/// record. Dropping a function at absolute index `s` shifts every later
/// function down by one, so every surviving reference — `BI_call` operands,
/// export/element/start descriptors, and `T_app` targets — must be renumbered.
/// Imports are never spec functions (codegen records only local functions), so
/// imported indices are stable.
///
/// An `exists`/`unique` spec function is different: its obligation is a
/// reachability judgment that looks the function up **in the emitted module**
/// and reduces its body under vanilla semantics, so its body must be
/// *retained* in the module record. Retained functions shift nothing (they
/// keep their place in `mod_funcs`), but the protection omission used to
/// provide accidentally — no executable construct can reach a spec function —
/// must now be provided deliberately: the reference sites reject a retained
/// target through [`Self::referenced_instantiated`], while the obligation
/// emitter computes the retained function's own `reach_func` index through
/// [`Self::mod_funcs_index`] directly, bypassing that reference guard.
struct FuncRemap {
    /// Absolute WASM indices of the omitted (forall/plain) spec functions,
    /// sorted ascending and de-duplicated.
    spec_abs: Vec<u32>,
    /// Absolute WASM indices of the retained (`exists`/`unique`) spec
    /// functions, sorted ascending and de-duplicated. Disjoint from
    /// `spec_abs`.
    retained_abs: Vec<u32>,
    /// Number of imported functions, occupying the lowest function indices.
    func_import_count: u32,
}

impl FuncRemap {
    /// Builds the remap from the classified omit and retain sets and the
    /// function-import count. The caller has already split the spec-function
    /// union by obligation kind; both sets are normalized here.
    fn new(mut omitted: Vec<u32>, mut retained: Vec<u32>, func_import_count: u32) -> Self {
        omitted.sort_unstable();
        omitted.dedup();
        retained.sort_unstable();
        retained.dedup();
        Self {
            spec_abs: omitted,
            retained_abs: retained,
            func_import_count,
        }
    }

    /// Whether the function at absolute index `abs` is an omitted spec function.
    fn is_omitted(&self, abs: u32) -> bool {
        self.spec_abs.binary_search(&abs).is_ok()
    }

    /// Whether the function at absolute index `abs` is a retained
    /// (`exists`/`unique`) spec function.
    fn is_retained(&self, abs: u32) -> bool {
        self.retained_abs.binary_search(&abs).is_ok()
    }

    /// The number of omitted spec functions strictly below `abs`.
    fn below(&self, abs: u32) -> u32 {
        // `partition_point` returns the count of elements for which the
        // predicate holds; on the sorted `spec_abs` that is exactly the number
        // of omitted indices strictly below `abs`.
        u32::try_from(self.spec_abs.partition_point(|&s| s < abs)).unwrap_or(u32::MAX)
    }

    /// Renumbers a function index into the emitted module's instantiated
    /// function space (imports first, then surviving defined functions).
    /// Fail-closed: a reference to an omitted spec function is an error.
    ///
    /// This is the raw index arithmetic; it accepts a retained spec function,
    /// because the reachability obligation's own `reach_func` lookup needs
    /// exactly that. Reference sites go through
    /// [`Self::referenced_instantiated`] instead, which rejects retained
    /// targets first.
    fn instantiated(&self, abs: u32) -> anyhow::Result<u32> {
        if self.is_omitted(abs) {
            return Err(anyhow::anyhow!(WasmToVError::HspecInconsistent(format!(
                "a construct retained in the emitted module references function {abs}, \
                 which is an omitted specification function"
            ))));
        }
        Ok(abs - self.below(abs))
    }

    /// The operand form for `BI_call`, `BI_ref_func`, exports, element items,
    /// and `mod_start`: [`Self::instantiated`] plus the retained-spec-function
    /// rejection. A retained `exists`/`unique` spec function stays in the
    /// module record only as the subject of its reachability obligation — its
    /// signature carries hidden choice parameters and its body traps on
    /// filtered paths, so it is not a callable and no executable construct may
    /// reference it.
    fn referenced_instantiated(&self, abs: u32) -> anyhow::Result<u32> {
        if self.is_retained(abs) {
            return Err(anyhow::anyhow!(WasmToVError::HspecInconsistent(format!(
                "a surviving construct references function {abs}, which is a retained \
                 `exists`/`unique` specification function; its body stays in the emitted \
                 module only as the subject of its reachability obligation, not as a \
                 callable"
            ))));
        }
        self.instantiated(abs)
    }

    /// The index of a defined function into `mod_funcs` (imports excluded), the
    /// form `T_app` and `reach_func` use. Fail-closed on an omitted or
    /// imported function; a retained function is accepted (the `reach_func`
    /// computation is the reason this method must not carry the reference
    /// guard).
    fn mod_funcs_index(&self, abs: u32) -> anyhow::Result<u32> {
        let instantiated = self.instantiated(abs)?;
        instantiated
            .checked_sub(self.func_import_count)
            .ok_or_else(|| {
                anyhow::anyhow!(WasmToVError::HspecInconsistent(format!(
                    "a `T_app` obligation references imported function {abs}, but only \
                     module-defined functions can be applied"
                )))
            })
    }
}

/// One spec's obligations split by quantifier kind. Built by
/// [`partition_entries`] — the single classification helper both the
/// definition emitter and the theorem emitter consume, so the two can never
/// disagree about which partition a spec's entries fall into.
struct SpecPartition<'e> {
    /// Universal obligations, consumed by the `_specs : list hassert` /
    /// `ValidSpec` grammar.
    forall: Vec<&'e HSpecEntry>,
    /// `exists`-kind obligations, consumed by the `_ex_specs : list
    /// reachability_spec` / `ValidExistsSpec` grammar.
    exists: Vec<(&'e HSpecEntry, &'e ReachMeta)>,
    /// `unique`-kind obligations, consumed by the `_uq_specs : list
    /// reachability_spec` / `ValidUniqueSpec` grammar.
    unique: Vec<(&'e HSpecEntry, &'e ReachMeta)>,
}

/// Splits one spec's entries by quantifier kind, preserving source order
/// within each partition.
fn partition_entries(entries: &[HSpecEntry]) -> SpecPartition<'_> {
    let mut partition = SpecPartition {
        forall: Vec::new(),
        exists: Vec::new(),
        unique: Vec::new(),
    };
    for entry in entries {
        match &entry.kind {
            SpecKind::Forall => partition.forall.push(entry),
            SpecKind::Exists(meta) => partition.exists.push((entry, meta)),
            SpecKind::Unique(meta) => partition.unique.push((entry, meta)),
        }
    }
    partition
}

/// Renders a `reach_visible_locs` value: `nil` when empty, `(a%N :: b%N ::
/// nil)` otherwise, matching the emitted `seq`-literal style elsewhere.
fn visible_locs_list(locs: &[u32]) -> String {
    if locs.is_empty() {
        return "nil".to_string();
    }
    let mut out = String::from("(");
    for loc in locs {
        out.push_str(&format!("{loc}%N :: "));
    }
    out.push_str("nil)");
    out
}

/// Structured representation of a parsed WASM module.
///
/// This structure holds all information extracted from WASM bytecode sections,
/// ready for translation to Rocq code. It is populated by [`crate::wasm_parser::parse`]
/// and consumed by [`WasmParseData::translate`].
///
/// # Lifetime
///
/// The lifetime `'a` represents borrowed data from the original WASM bytecode.
/// Most WASM section data (imports, exports, function bodies) reference slices
/// of the original bytecode to avoid allocations.
///
/// # Fields
///
/// ## Module Metadata
/// - `mod_name`: Rocq module identifier (from parameter or custom name section)
/// - `func_names_map`: Maps absolute function index → name (from custom name section)
/// - `func_locals_name_map`: Maps absolute function index → (local index → name) (from custom name section)
/// - `start_function`: Optional module entry point function index
///
/// ## WASM Sections
/// - `imports`: External dependencies (functions, tables, memories, globals)
/// - `exports`: Public interface (exported functions, tables, memories, globals)
/// - `tables`: Indirect call table definitions
/// - `memory_types`: Linear memory specifications
/// - `globals`: Global variable definitions with initialization
/// - `data`: Memory initialization segments
/// - `elements`: Table initialization segments
/// - `function_types`: Function type signatures (as recursion groups)
/// - `function_type_indexes`: Maps defined-function position (code-section
///   order, imports excluded) → type index
/// - `function_bodies`: Function code with locals and instructions
///
/// ## Translation State (private)
/// - `translated_function_names`: Accumulates Rocq function names during translation
/// - `translated_functions_string`: Accumulates Rocq function definitions during translation
pub(crate) struct WasmParseData<'a> {
    pub(crate) mod_name: String,
    pub(crate) func_names_map: Option<HashMap<u32, String>>,
    /// The RAW, unsanitized name-section function names keyed by absolute WASM
    /// function index. Whereas `func_names_map` holds Rocq-sanitized names for
    /// `Definition` emission, `inference.hspecs` obligations reference callees
    /// by their exact `FnKey::Display` symbol, so `T_app` resolution keys on
    /// these untouched strings.
    pub(crate) raw_func_names_map: Option<HashMap<u32, String>>,
    pub(crate) func_locals_name_map: Option<HashMap<u32, HashMap<u32, String>>>,

    pub(crate) start_function: Option<u32>,

    pub(crate) imports: Vec<Import<'a>>,
    pub(crate) exports: Vec<Export<'a>>,
    pub(crate) tables: Vec<Table<'a>>,
    pub(crate) memory_types: Vec<MemoryType>,
    pub(crate) globals: Vec<Global<'a>>,
    pub(crate) data: Vec<Data<'a>>,
    pub(crate) elements: Vec<Element<'a>>,
    pub(crate) function_types: Vec<RecGroup>,
    pub(crate) function_type_indexes: Vec<u32>,
    pub(crate) function_bodies: Vec<FunctionBody<'a>>,
    /// WASM function indices that originated from `spec` blocks, keyed by spec
    /// name. A spec function whose obligation is universal (or that carries no
    /// obligation — a method) is OMITTED from the emitted module record: it is
    /// a downstream contract obligation, not part of the executable module. A
    /// spec function with an `exists`/`unique` obligation is RETAINED instead —
    /// its reachability judgment reduces the emitted body — and the split
    /// drives the [`FuncRemap`] that renumbers every surviving function
    /// reference. Each spec also materializes a
    /// `<mod>__<SpecName>_specs : list hassert` definition and a `ValidSpec`
    /// theorem, plus `_ex_specs`/`_uq_specs : list reachability_spec`
    /// definitions and `ValidExistsSpec`/`ValidUniqueSpec` theorems for its
    /// non-empty reachability partitions.
    pub(crate) spec_funcs_by_spec: FxHashMap<String, Vec<u32>>,
    /// Per-spec `hassert` verification obligations decoded from the
    /// `inference.hspecs` custom section (or supplied explicitly). A subset of
    /// `spec_funcs_by_spec` by spec name: a spec with only methods contributes
    /// indices but no obligations.
    pub(crate) hspecs_by_spec: HSpecMap,

    translated_function_names: Vec<String>,
    translated_functions_string: String,
}

impl WasmParseData<'_> {
    /// Creates a new empty [`WasmParseData`] with the given module name and spec indices.
    ///
    /// All section vectors are initialized as empty. This is called by the parser
    /// before streaming through WASM sections.
    ///
    /// # Parameters
    ///
    /// - `mod_name`: Default Rocq module name (may be overridden by custom name section)
    /// - `spec_funcs_by_spec`: WASM function indices grouped by spec name
    pub(crate) fn new<'a>(
        mod_name: String,
        spec_funcs_by_spec: FxHashMap<String, Vec<u32>>,
        hspecs_by_spec: HSpecMap,
    ) -> WasmParseData<'a> {
        WasmParseData {
            mod_name,
            func_names_map: None,
            raw_func_names_map: None,
            func_locals_name_map: None,
            start_function: None,
            imports: Vec::new(),
            exports: Vec::new(),
            tables: Vec::new(),
            memory_types: Vec::new(),
            globals: Vec::new(),
            data: Vec::new(),
            elements: Vec::new(),
            function_types: Vec::new(),
            function_type_indexes: Vec::new(),
            function_bodies: Vec::new(),
            spec_funcs_by_spec,
            hspecs_by_spec,

            translated_function_names: Vec::new(),
            translated_functions_string: String::new(),
        }
    }

    /// Translates the parsed WASM data into complete Rocq code.
    ///
    /// This is the main translation entry point. It generates a complete Rocq file
    /// including imports, helper definitions, and a module record containing all
    /// translated WASM sections.
    ///
    /// # Translation Steps
    ///
    /// 1. Generate Rocq imports and helper definitions
    /// 2. Translate each WASM section to Rocq definitions:
    ///    - Imports → `Mi` records
    ///    - Exports → `Me` records
    ///    - Tables → `Mt` definitions
    ///    - Memory → `Mm` definitions
    ///    - Globals → `Mg` definitions
    ///    - Data segments → data initialization
    ///    - Element segments → table initialization
    ///    - Function types → type signatures
    ///    - Functions → `module_func` definitions
    /// 3. Assemble module record with all translated components
    ///
    /// # Error Recovery
    ///
    /// This method collects translation errors from every section so a single
    /// failure does not mask later ones, but it is fail-closed: if any section
    /// failed, the assembled module is discarded and the first error is
    /// returned. The emitted `.v` is a mission-critical proof artifact, so a
    /// partial translation (e.g. a module missing a function body) must never
    /// be returned as success.
    ///
    /// # Returns
    ///
    /// Returns a `String` containing complete Rocq code ready to write to a `.v` file.
    ///
    /// # Errors
    ///
    /// Returns an error if translation of any section fails due to:
    /// - Unsupported WASM features (tags, unknown reference types)
    /// - Invalid WASM data (malformed expressions, out-of-bounds indices)
    /// - Unimplemented instruction opcodes
    #[allow(clippy::too_many_lines)]
    pub(crate) fn translate(&mut self) -> anyhow::Result<String /* WasmModuleParseError*/> {
        // Spec names reaching this point have already been validated individually
        // either at the public-API boundary (`wasm_parser::translate_bytes`
        // validates the caller-supplied map) or at the decode boundary
        // (`wasm_parser::decode_spec_funcs_section` validates embedded names). What
        // those per-component checks cannot see is the `__` separator the join
        // *fabricates* when this module name and a spec name are concatenated into
        // the `<module>__<spec>_specs` / `valid_<module>__<spec>` grammar below.
        // Validate that boundary here, where both names are final (the module name
        // may have been overridden by the custom name section). A trailing-`_`
        // component is rejected before any output is built, so a contaminated proof
        // never reaches disk. This is the entry file too: `qualified_spec_name`
        // leaves the entry's empty module path off the spec name, so codegen's
        // own `_`-join check never sees a `__`, but the entry stem still joins here.
        for spec_name in self.spec_funcs_by_spec.keys() {
            crate::rocq_names::validate_spec_join_boundary(&self.mod_name, spec_name)?;
        }

        // One shared symbol->index inversion of the raw name section, feeding
        // both the reachability-target classification below and `T_app` symbol
        // resolution later, so the two can never resolve one symbol
        // differently.
        let by_name = self.invert_raw_func_names();

        // Classify the reachability (`exists`/`unique`) obligations up front:
        // each must resolve to the defined spec function it judges, and that
        // function is RETAINED in the module record while every other spec
        // function stays omitted. Classification is fail-closed — an
        // unresolvable or inconsistent reachability target rejects the module
        // before any output is built.
        let reach_targets = self.classify_reachability_targets(&by_name)?;
        let mut retained: Vec<u32> = reach_targets.values().copied().collect();
        retained.sort_unstable();
        retained.dedup();
        let mut omitted: Vec<u32> = self
            .spec_funcs_by_spec
            .values()
            .flatten()
            .copied()
            .collect();
        omitted.sort_unstable();
        omitted.dedup();
        omitted.retain(|abs| retained.binary_search(abs).is_err());

        // The renumbering forced by omitting spec functions from the module
        // record. Built once here and threaded through every function-index
        // site (function bodies, exports, elements, `mod_start`, and `T_app`
        // resolution).
        let func_import_count =
            u32::try_from(self.func_import_count()).expect("import count exceeds u32");
        let remap = FuncRemap::new(omitted, retained, func_import_count);

        let mut res = String::new();
        res.push_str("Require Import List.\n");
        res.push_str("Require Import String.\n");
        res.push_str("Require Import BinNat.\n");
        res.push_str("Require Import ZArith.\n");
        res.push_str("From Wasm Require Import bytes numerics datatypes host.\n");
        // The reachability grammar (`reachability_spec` records and the
        // `ValidExistsSpec`/`ValidUniqueSpec` predicates) lives in the
        // wasm-verifier `Exists` module. The import joins the line only when a
        // reachability obligation exists, so a forall-only module's preamble is
        // byte-identical to what it was before reachability emission existed
        // (the same keying discipline as `Open Scope byte_scope.` below).
        if self.has_reachability_entries() {
            res.push_str("From WasmVerifier Require Import Assertions Verifier Exists.\n");
        } else {
            res.push_str("From WasmVerifier Require Import Assertions Verifier.\n");
        }
        // A data segment's bytes are mostly written in the hex notations the
        // `Wasm.bytes` module declares in `byte_scope`, and those parse only
        // while that scope is open. Whether an `Import` chain leaves it open is
        // a detail of the library, not of this contract, so a module that can
        // spell a byte notation states its own requirement. The line is keyed
        // on the presence of a data segment rather than on the bytes in it:
        // opening a scope nothing happens to use is inert, while deciding per
        // byte would make the preamble depend on segment contents. A module
        // with no data segment names no byte at all, and emits no such line.
        if !self.data.is_empty() {
            res.push_str("Open Scope byte_scope.\n");
        }
        res.push('\n');
        res.push_str("Definition Vi32 i := VAL_int32 (Wasm_int.int_of_Z i32m i).\n");
        res.push_str("Definition Vi64 i := VAL_int64 (Wasm_int.int_of_Z i64m i).\n");
        res.push_str(
            "Definition Mt l et := {|modtab_type := {|tt_limits := l; tt_elem_type := et|}|}.\n",
        );
        res.push_str("Definition Mm l := {|modmem_type := l|}.\n");
        res.push_str("Definition Mg mut t init := {|modglob_type := {|tg_mut := mut; tg_t := t|}; modglob_init := init|}.\n");
        res.push('\n');
        res.push_str("Definition Mi m n d := {|\n");
        res.push_str("  imp_module := list_byte_of_string m;\n");
        res.push_str("  imp_name := list_byte_of_string n;\n");
        res.push_str("  imp_desc := d;\n");
        res.push_str("|}.\n");
        res.push('\n');
        res.push_str("Definition Me n d := {|\n");
        res.push_str("  modexp_name := list_byte_of_string n;\n");
        res.push_str("  modexp_desc := d;\n");
        res.push_str("|}.\n");
        res.push('\n');
        res.push_str("Definition Ma of al := {|memarg_offset := of; memarg_align := al|}.\n");
        res.push('\n');

        let mut errors = Vec::new();

        let mut translated_imports = String::new();
        for import in &self.imports {
            match translate_module_import(import) {
                Ok(translated_import) => {
                    translated_imports.push_str("    ");
                    translated_imports.push_str(translated_import.as_str());
                    translated_imports.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        translated_imports.push_str("    ");
        translated_imports.push_str(LIST_SEAL);

        let mut created_exports = String::new();
        for export in &self.exports {
            match translate_export_module(export, &remap) {
                Ok(translated_export) => {
                    created_exports.push_str("    ");
                    created_exports.push_str(translated_export.as_str());
                    created_exports.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        created_exports.push_str("    ");
        created_exports.push_str(LIST_SEAL);

        let mut created_tables = String::new();
        for table in &self.tables {
            match translate_table_type(table) {
                Ok(translated_table_type) => {
                    created_tables.push_str("    ");
                    created_tables.push_str(translated_table_type.as_str());
                    created_tables.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        created_tables.push_str("    ");
        created_tables.push_str(LIST_SEAL);

        let mut created_memory_types = String::new();
        for memory_type in &self.memory_types {
            match translate_memory_type(memory_type) {
                Ok(translated_memory) => {
                    created_memory_types.push_str("    ");
                    created_memory_types.push_str(translated_memory.as_str());
                    created_memory_types.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        created_memory_types.push_str("    ");
        created_memory_types.push_str(LIST_SEAL);

        let mut created_globals = String::new();
        for global in &self.globals {
            match translate_global(global, &remap) {
                Ok(translated_global) => {
                    created_globals.push_str("    ");
                    created_globals.push_str(translated_global.as_str());
                    created_globals.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        created_globals.push_str("    ");
        created_globals.push_str(LIST_SEAL);

        let mut created_data_segments = String::new();
        for data in &self.data {
            match translate_data(data, &remap) {
                Ok(translated_data) => {
                    created_data_segments.push_str("    ");
                    created_data_segments.push_str(translated_data.as_str());
                    created_data_segments.push_str(LIST_EXT);
                }
                Err(e) => errors.push(e),
            }
        }
        created_data_segments.push_str("    ");
        created_data_segments.push_str(LIST_SEAL);

        let mut created_elements = String::new();
        for element in &self.elements {
            match translate_element(element, &remap) {
                Ok(translated_element) => {
                    created_elements.push_str("    ");
                    created_elements.push_str(translated_element.as_str());
                    created_elements.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        created_elements.push_str("    ");
        created_elements.push_str(LIST_SEAL);

        let mut created_function_types = String::new();
        for rec_group in &self.function_types {
            // created_function_types.push(LRB);
            match translate_function_type(rec_group) {
                Ok(translated_function_type) => {
                    created_function_types.push_str("    ");
                    created_function_types.push_str(translated_function_type.as_str());
                    created_function_types.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        created_function_types.push_str("    ");
        created_function_types.push_str(LIST_SEAL);

        let mut created_functions = String::new();
        match self.translate_functions(&remap) {
            Ok(_) => {
                res.push_str(self.translated_functions_string.as_str());
                for function_name in &self.translated_function_names {
                    created_functions.push_str("    ");
                    created_functions.push_str(function_name.as_str());
                    created_functions.push_str(LIST_EXT);
                }
            }
            Err(e) => {
                errors.push(e);
            }
        }
        created_functions.push_str("    ");
        created_functions.push_str(LIST_SEAL);

        //Record module
        let module_name = &self.mod_name;
        res.push_str(format!("Definition {module_name} : module := ").as_str());
        res.push_str(LCB);
        res.push_str(format!("  mod_types :=\n{created_function_types};\n").as_str());
        res.push_str(format!("  mod_funcs :=\n{created_functions};\n").as_str());
        res.push_str(format!("  mod_tables :=\n{created_tables};\n").as_str());
        res.push_str(format!("  mod_mems :=\n{created_memory_types};\n").as_str());
        res.push_str(format!("  mod_globals :=\n{created_globals};\n").as_str());
        res.push_str(format!("  mod_elems :=\n{created_elements};\n").as_str());
        res.push_str(format!("  mod_datas :=\n{created_data_segments};\n").as_str());
        if let Some(start_function) = self.start_function {
            let start = remap.referenced_instantiated(start_function)?;
            res.push_str(
                format!("  mod_start := Some {{|modstart_func := {start}%N|}};\n").as_str(),
            );
        } else {
            res.push_str("  mod_start := None;\n");
        }
        res.push_str(format!("  mod_imports :=\n{translated_imports};\n").as_str());
        res.push_str(format!("  mod_exports :=\n{created_exports};\n").as_str());
        res.push_str(RCB_DOT);

        // Fail-closed: any section error means the assembled module is
        // incomplete (e.g. a function body that hit an unsupported operator).
        // Surface it before emitting the obligation definitions, which resolve
        // symbols against the (now-known) function layout.
        if let Some(first) = errors.into_iter().next() {
            return Err(first);
        }

        self.emit_spec_definitions(&mut res, &remap, &by_name, &reach_targets)?;
        self.emit_theorems(&mut res);

        Ok(res)
    }

    /// Whether any obligation in the module is a reachability
    /// (`exists`/`unique`) obligation. Keys the conditional ` Exists`
    /// preamble import and nothing else — emission itself is driven per spec by
    /// [`partition_entries`].
    fn has_reachability_entries(&self) -> bool {
        self.hspecs_by_spec
            .values()
            .flatten()
            .any(|entry| !matches!(entry.kind, SpecKind::Forall))
    }

    /// Inverts the RAW name-section map: symbol → absolute indices of the
    /// functions carrying it. The single inversion both
    /// [`Self::classify_reachability_targets`] and
    /// [`Self::resolve_app_symbols`] consume, so a symbol can never resolve to
    /// different indices on the two paths.
    fn invert_raw_func_names(&self) -> HashMap<String, Vec<u32>> {
        let mut by_name: HashMap<String, Vec<u32>> = HashMap::new();
        if let Some(raw) = &self.raw_func_names_map {
            for (idx, name) in raw {
                by_name.entry(name.clone()).or_default().push(*idx);
            }
        }
        by_name
    }

    /// Parameter count of the defined function at absolute index `abs`, read
    /// from its type-section entry. `None` when the function or its type
    /// cannot be located (a malformed module; callers fail closed).
    fn defined_func_param_count(&self, abs: u32) -> Option<u32> {
        let defined = (abs as usize).checked_sub(self.func_import_count())?;
        let type_idx = *self.function_type_indexes.get(defined)? as usize;
        // The type index space flattens recursion groups in section order.
        let ty = self
            .function_types
            .iter()
            .flat_map(inf_wasmparser::RecGroup::types)
            .nth(type_idx)?;
        match &ty.composite_type.inner {
            CompositeInnerType::Func(ft) => u32::try_from(ft.params().len()).ok(),
            _ => None,
        }
    }

    /// Declared-local count (locals section, parameters excluded) of the
    /// defined function at absolute index `abs`. `None` when the body cannot
    /// be located or its locals cannot be read.
    fn defined_func_local_count(&self, abs: u32) -> Option<u32> {
        let defined = (abs as usize).checked_sub(self.func_import_count())?;
        let body = self.function_bodies.get(defined)?;
        let mut count: u32 = 0;
        for local in body.get_locals_reader().ok()? {
            let (reps, _) = local.ok()?;
            count = count.checked_add(reps)?;
        }
        Some(count)
    }

    /// Resolves every reachability (`exists`/`unique`) obligation to the
    /// absolute index of the defined spec function it judges, fail-closed.
    ///
    /// The obligation's own symbol is the spec-folded `FnKey` display,
    /// `<folded_spec>.<name>` — but the name section stores the bare `<name>`
    /// (spec membership travels separately in `inference.spec_funcs`), so
    /// resolution strips the spec qualifier when present and then
    /// disambiguates through the spec's own index list: the retained target
    /// is the one function that both carries the bare name and is listed
    /// under the obligation's spec.
    ///
    /// The reachability judgment looks its function up in the emitted module
    /// (`reach_func`) and evaluates its payload against the frame an actual
    /// execution reaches, so an obligation whose target cannot be located —
    /// or whose frame metadata does not fit the located function — would be
    /// silently unprovable downstream. Every such inconsistency is rejected
    /// here instead, where it can name the offender:
    ///
    /// * the module carries no name section (symbols are name-section
    ///   strings, so reachability translation hard-depends on it);
    /// * no defined function carries the symbol's name;
    /// * none of the carriers is listed under the obligation's spec in
    ///   `inference.spec_funcs`, or several are (ambiguous);
    /// * `entry_arity` exceeds the function's parameter count (the choice
    ///   suffix can only extend the source parameters, never shrink them);
    /// * a `visible_locs` slot falls outside the function's frame
    ///   (parameters + declared locals).
    ///
    /// Spec names are visited in sorted order so the reported offender is
    /// deterministic.
    fn classify_reachability_targets(
        &self,
        by_name: &HashMap<String, Vec<u32>>,
    ) -> anyhow::Result<FxHashMap<String, u32>> {
        let mut targets = FxHashMap::default();
        let mut spec_names: Vec<&String> = self.hspecs_by_spec.keys().collect();
        spec_names.sort_unstable();
        for spec_name in spec_names {
            for entry in &self.hspecs_by_spec[spec_name] {
                let (kind, meta) = match &entry.kind {
                    SpecKind::Forall => continue,
                    SpecKind::Exists(meta) => ("exists", meta),
                    SpecKind::Unique(meta) => ("unique", meta),
                };
                let sym = entry.fn_symbol.0.as_str();
                if self.raw_func_names_map.is_none() {
                    return Err(anyhow::anyhow!(WasmToVError::HspecInconsistent(format!(
                        "the `{kind}`-quantified obligation for `{sym}` in spec \
                         `{spec_name}` must resolve to its retained function through the \
                         WASM `name` section, but the module carries no function names"
                    ))));
                }
                let bare = sym.strip_prefix(&format!("{spec_name}.")).unwrap_or(sym);
                let name_matches = by_name.get(bare).map_or(&[][..], Vec::as_slice);
                if name_matches.is_empty() {
                    return Err(anyhow::anyhow!(WasmToVError::HspecInconsistent(format!(
                        "the `{kind}`-quantified obligation for `{sym}` in spec \
                         `{spec_name}` names a function symbol that no defined \
                         function in the module carries"
                    ))));
                }
                let spec_indices = self
                    .spec_funcs_by_spec
                    .get(spec_name)
                    .map_or(&[][..], Vec::as_slice);
                let candidates: Vec<u32> = name_matches
                    .iter()
                    .copied()
                    .filter(|abs| spec_indices.contains(abs))
                    .collect();
                let abs = match candidates[..] {
                    [one] => one,
                    [] => {
                        return Err(anyhow::anyhow!(WasmToVError::HspecInconsistent(format!(
                            "the `{kind}`-quantified obligation for `{sym}` in spec \
                             `{spec_name}` resolves only to functions \
                             `inference.spec_funcs` does not list under that spec"
                        ))));
                    }
                    _ => {
                        return Err(anyhow::anyhow!(WasmToVError::HspecInconsistent(format!(
                            "the `{kind}`-quantified obligation for `{sym}` in spec \
                             `{spec_name}` names a function symbol that {} defined \
                             functions of its spec share; the retained target is \
                             ambiguous",
                            candidates.len()
                        ))));
                    }
                };
                let Some(params) = self.defined_func_param_count(abs) else {
                    return Err(anyhow::anyhow!(WasmToVError::HspecInconsistent(format!(
                        "the `{kind}`-quantified obligation for `{sym}` in spec \
                         `{spec_name}` resolves to function {abs}, whose type cannot be \
                         read from the module"
                    ))));
                };
                if meta.entry_arity > params {
                    return Err(anyhow::anyhow!(WasmToVError::HspecInconsistent(format!(
                        "the `{kind}`-quantified obligation for `{sym}` in spec \
                         `{spec_name}` declares entry arity {}, but the retained \
                         function's parameter count is {params}",
                        meta.entry_arity
                    ))));
                }
                let Some(locals) = self.defined_func_local_count(abs) else {
                    return Err(anyhow::anyhow!(WasmToVError::HspecInconsistent(format!(
                        "the `{kind}`-quantified obligation for `{sym}` in spec \
                         `{spec_name}` resolves to function {abs}, whose locals cannot \
                         be read from the module"
                    ))));
                };
                let frame = u64::from(params) + u64::from(locals);
                if let Some(loc) = meta
                    .visible_locs
                    .iter()
                    .find(|&&loc| u64::from(loc) >= frame)
                {
                    return Err(anyhow::anyhow!(WasmToVError::HspecInconsistent(format!(
                        "the `{kind}`-quantified obligation for `{sym}` in spec \
                         `{spec_name}` declares source-visible slot {loc}, but the \
                         retained function's frame size (parameters + locals) \
                         is {frame}"
                    ))));
                }
                targets.insert(sym.to_string(), abs);
            }
        }
        Ok(targets)
    }

    /// Spec names, sorted, so the `list hassert` definitions and the theorems
    /// iterate in the same deterministic order. The authoritative spec set is
    /// `inference.spec_funcs`; `inference.hspecs` is a subset (a method-only
    /// spec has indices but no obligations), so a spec present here with no
    /// obligation entry emits an empty `list hassert`.
    fn sorted_spec_names(&self) -> Vec<&String> {
        let mut names: Vec<&String> = self.spec_funcs_by_spec.keys().collect();
        names.sort();
        names
    }

    /// Resolves every function symbol any obligation applies to its `mod_funcs`
    /// index, up front, so a missing / ambiguous / imported / spec-function
    /// target fails before a line of output is built.
    ///
    /// `by_name` is the shared raw name-section inversion built once in
    /// [`Self::translate`]. A `T_app` names exactly one defined function, so
    /// zero or several matches is a hard error.
    fn resolve_app_symbols(
        &self,
        by_name: &HashMap<String, Vec<u32>>,
        remap: &FuncRemap,
    ) -> anyhow::Result<FxHashMap<String, u32>> {
        let mut symbols: Vec<&str> = Vec::new();
        for entries in self.hspecs_by_spec.values() {
            for entry in entries {
                hassert_print::collect_symbols(&entry.hassert, &mut symbols);
            }
        }
        symbols.sort_unstable();
        symbols.dedup();
        if symbols.is_empty() {
            return Ok(FxHashMap::default());
        }

        let mut resolved = FxHashMap::default();
        for sym in symbols {
            let abs = match by_name.get(sym).map(Vec::as_slice) {
                Some([one]) => *one,
                Some(many) if many.len() > 1 => {
                    return Err(anyhow::anyhow!(WasmToVError::HspecInconsistent(format!(
                        "obligation applies function symbol `{sym}`, which {} defined \
                         functions share; the target is ambiguous",
                        many.len()
                    ))));
                }
                _ => {
                    return Err(anyhow::anyhow!(WasmToVError::HspecInconsistent(format!(
                        "obligation applies function symbol `{sym}`, which no defined function \
                         in the module carries"
                    ))));
                }
            };
            // A retained `exists`/`unique` spec function is the subject of its
            // own reachability obligation, not an interpretable symbol: no
            // payload may apply it. Rejected explicitly, because the retained
            // function passes the omitted/imported arithmetic below.
            if remap.is_retained(abs) {
                return Err(anyhow::anyhow!(WasmToVError::HspecInconsistent(format!(
                    "obligation applies function symbol `{sym}`, which is a retained \
                     `exists`/`unique` specification function; a specification function \
                     is the subject of its own obligation, not an interpretable symbol"
                ))));
            }
            // Rejects an imported or omitted (spec) target: a `T_app` may only
            // name a module-defined, non-spec function.
            let idx = remap.mod_funcs_index(abs)?;
            resolved.insert(sym.to_string(), idx);
        }
        Ok(resolved)
    }

    /// Appends the per-spec obligation definitions to `out`, partitioned by
    /// quantifier kind through [`partition_entries`].
    ///
    /// The universal partition keeps its grammar unconditionally: one
    /// `<mod>__<Spec>_hspec{k} : hassert` per obligation (source order,
    /// 1-based), then a `<mod>__<Spec>_specs : list hassert` gathering them —
    /// or the explicitly-typed `(@nil hassert)` when the partition is empty (a
    /// spec with only methods, an empty `spec { }`, or only reachability
    /// obligations). The `exists` and `unique` partitions each add
    /// `reachability_spec` record definitions and an
    /// `_ex_specs`/`_uq_specs : list reachability_spec` list, but only when
    /// non-empty, so a forall-only module's output is byte-identical to what
    /// it was before reachability emission existed.
    ///
    /// Spec names were validated against the Rocq identifier rules at the top
    /// of `translate()` so that every joined definition name is a
    /// syntactically legal Rocq identifier.
    fn emit_spec_definitions(
        &self,
        out: &mut String,
        remap: &FuncRemap,
        by_name: &HashMap<String, Vec<u32>>,
        reach_targets: &FxHashMap<String, u32>,
    ) -> anyhow::Result<()> {
        let resolved = self.resolve_app_symbols(by_name, remap)?;
        let module_name = &self.mod_name;
        for spec_name in self.sorted_spec_names() {
            out.push('\n');
            let entries = self
                .hspecs_by_spec
                .get(spec_name)
                .map_or(&[][..], Vec::as_slice);
            let partition = partition_entries(entries);
            if partition.forall.is_empty() {
                // No universal free-function obligations: an explicitly-typed
                // empty list, scope- and Require-order-independent.
                out.push_str(
                    format!(
                        "Definition {module_name}__{spec_name}_specs : list hassert := (@nil hassert).\n"
                    )
                    .as_str(),
                );
            } else {
                let mut hspec_names = Vec::with_capacity(partition.forall.len());
                for (k, entry) in partition.forall.iter().enumerate() {
                    let def_name = format!("{module_name}__{spec_name}_hspec{}", k + 1);
                    let body = hassert_print::print_assert(&entry.hassert, &resolved);
                    out.push_str(
                        format!("Definition {def_name} : hassert :=\n  {body}.\n").as_str(),
                    );
                    hspec_names.push(def_name);
                }
                let joined = hspec_names.join(" :: ");
                out.push_str(
                    format!(
                        "Definition {module_name}__{spec_name}_specs : list hassert := ({joined} :: nil).\n"
                    )
                    .as_str(),
                );
            }
            self.emit_reachability_partition(
                out,
                remap,
                &resolved,
                reach_targets,
                spec_name,
                &partition.exists,
                "exspec",
                "ex_specs",
            )?;
            self.emit_reachability_partition(
                out,
                remap,
                &resolved,
                reach_targets,
                spec_name,
                &partition.unique,
                "uqspec",
                "uq_specs",
            )?;
        }
        Ok(())
    }

    /// Appends one reachability partition's definitions: one
    /// `<mod>__<Spec>_{def_suffix}{k} : reachability_spec` record per
    /// obligation (source order, 1-based), then the gathering
    /// `<mod>__<Spec>_{list_suffix} : list reachability_spec`. An empty
    /// partition emits nothing.
    ///
    /// `reach_func` is computed through the direct index arithmetic
    /// ([`FuncRemap::mod_funcs_index`]) rather than the reference-guarded
    /// path: the retained function is the obligation's own subject, and the
    /// downstream predicate looks it up in `mod_funcs` (imports excluded).
    #[allow(clippy::too_many_arguments)]
    fn emit_reachability_partition(
        &self,
        out: &mut String,
        remap: &FuncRemap,
        resolved: &FxHashMap<String, u32>,
        reach_targets: &FxHashMap<String, u32>,
        spec_name: &str,
        entries: &[(&HSpecEntry, &ReachMeta)],
        def_suffix: &str,
        list_suffix: &str,
    ) -> anyhow::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let module_name = &self.mod_name;
        let mut def_names = Vec::with_capacity(entries.len());
        for (k, (entry, meta)) in entries.iter().enumerate() {
            let def_name = format!("{module_name}__{spec_name}_{def_suffix}{}", k + 1);
            let abs = reach_targets
                .get(&entry.fn_symbol.0)
                .copied()
                .expect("every reachability obligation is classified before emission");
            let reach_func = remap.mod_funcs_index(abs)?;
            let payload = hassert_print::print_assert(&entry.hassert, resolved);
            let locs = visible_locs_list(&meta.visible_locs);
            out.push_str(
                format!(
                    "Definition {def_name} : reachability_spec :=\n  \
                     {{| reach_func := {reach_func}%N; reach_entry_arity := {}%nat;\n     \
                     reach_visible_locs := {locs}; reach_payload := {payload} |}}.\n",
                    meta.entry_arity
                )
                .as_str(),
            );
            def_names.push(def_name);
        }
        let joined = def_names.join(" :: ");
        out.push_str(
            format!(
                "Definition {module_name}__{spec_name}_{list_suffix} : list reachability_spec := ({joined} :: nil).\n"
            )
            .as_str(),
        );
        Ok(())
    }

    /// Appends the `Section Host` block: the always-emitted 1-ary
    /// `ValidModule` theorem, then per spec the `ValidSpec` theorem over its
    /// universal obligations, plus a `ValidExistsSpec`/`ValidUniqueSpec`
    /// theorem for each non-empty reachability partition — consuming the
    /// definitions emitted by [`Self::emit_spec_definitions`]. Both emitters
    /// partition through [`partition_entries`], so a theorem can never name a
    /// list the definitions did not emit.
    fn emit_theorems(&self, out: &mut String) {
        let module_name = &self.mod_name;
        out.push('\n');
        out.push_str("Section Host.\n");
        out.push_str("Context `{ho: host}.\n");
        out.push('\n');
        out.push_str(
            format!("Theorem valid_{module_name} : ValidModule {module_name}.\n").as_str(),
        );
        out.push_str("Proof.\n");
        out.push_str("  (* TODO: fill the proof *)\n");
        out.push_str("Qed.\n");
        for spec_name in self.sorted_spec_names() {
            out.push('\n');
            out.push_str(
                format!(
                    "Theorem valid_{module_name}__{spec_name} : ValidSpec {module_name} {module_name}__{spec_name}_specs.\n"
                )
                .as_str(),
            );
            out.push_str("Proof.\n");
            out.push_str("  (* TODO: fill the proof *)\n");
            out.push_str("Qed.\n");
            let entries = self
                .hspecs_by_spec
                .get(spec_name)
                .map_or(&[][..], Vec::as_slice);
            let partition = partition_entries(entries);
            if !partition.exists.is_empty() {
                out.push('\n');
                out.push_str(
                    format!(
                        "Theorem valid_exists_{module_name}__{spec_name} : ValidExistsSpec {module_name} {module_name}__{spec_name}_ex_specs.\n"
                    )
                    .as_str(),
                );
                out.push_str("Proof.\n");
                out.push_str("  (* TODO: fill the proof *)\n");
                out.push_str("Qed.\n");
            }
            if !partition.unique.is_empty() {
                out.push('\n');
                out.push_str(
                    format!(
                        "Theorem valid_unique_{module_name}__{spec_name} : ValidUniqueSpec {module_name} {module_name}__{spec_name}_uq_specs.\n"
                    )
                    .as_str(),
                );
                out.push_str("Proof.\n");
                out.push_str("  (* TODO: fill the proof *)\n");
                out.push_str("Qed.\n");
            }
        }
        out.push('\n');
        out.push_str("End Host.\n");
    }

    /// Number of imported functions, which occupy the lowest function indices
    /// in WASM's index space before any locally-defined (code-section) function.
    ///
    /// The static-merge linker removes every import before `-v`, so this is `0`
    /// for every artifact the pipeline produces (the always-link invariant). It
    /// is non-zero only when a pre-link or third-party module is translated
    /// directly; the offset below keeps that case correctly indexed rather than
    /// relying on the invariant for soundness.
    fn func_import_count(&self) -> usize {
        self.imports
            .iter()
            .filter(|import| matches!(import.ty, TypeRef::Func(_)))
            .count()
    }

    //Record module_func
    fn translate_functions(&mut self, remap: &FuncRemap) -> anyhow::Result<()> {
        // Rocq `Definition`s are not overloadable, so every emitted function
        // name must be globally unique. A static merge can fold an external
        // library's private function (carrying its own debug name) next to a
        // main-module function of the same name. We disambiguate by appending
        // the WASM function index on collision, deriving the `Definition` and
        // the matching `mod_funcs` entry from the same per-index name.
        //
        // `function_bodies` is indexed 0-based over the *code section*, but the
        // name section, start/export descriptors, and the
        // `inference.spec_funcs` map key on the *absolute* WASM function index,
        // which numbers imported functions first. Offset the body position by
        // the function-import count to recover the absolute index for those
        // lookups. `mod_funcs` order itself stays body-relative (it excludes
        // imports, which appear via `mod_imports`). With no imports — every
        // post-link artifact — the offset is zero and output is unchanged.
        let func_import_base = self.func_import_count();
        let mut used_names: FxHashSet<String> = FxHashSet::default();
        for (index, function_body) in self.function_bodies.iter().enumerate() {
            let modfunc_type = *self.function_type_indexes.get(index).unwrap_or(&0);
            let abs_index = (func_import_base + index) as u32;
            // A forall/plain spec function is a downstream contract
            // obligation, not part of the executable module: omit its body and
            // its `mod_funcs` entry. `mod_types` stays complete (its
            // now-unused type is still legal), so `modfunc_type` above needs
            // no adjustment; the `remap` renumbers every surviving reference
            // to the functions that remain. A retained `exists`/`unique` spec
            // function is not skipped: its reachability obligation reduces the
            // emitted body, so it flows through the ordinary emission below
            // (its lowering is vanilla WASM by construction).
            if remap.is_omitted(abs_index) {
                continue;
            }
            // A function with no name-section entry is named deterministically
            // from its absolute index (`func_<abs_index>`) rather than a
            // per-process random UUID, so the `.v` is byte-identical across runs
            // for byte-identical input (reproducible builds, content-addressed
            // proof caches, CI diffs). The linker fills every merged inner
            // callee's name, so this fallback fires only for an unnamed function
            // reaching the translator directly.
            let base_name = match &self.func_names_map {
                Some(func_names_map) => func_names_map
                    .get(&abs_index)
                    .cloned()
                    .unwrap_or_else(|| format!("func_{abs_index}")),
                None => format!("func_{abs_index}"),
            };
            let func_name = unique_function_name(base_name, abs_index, &mut used_names);
            self.translated_function_names.push(func_name.clone());

            let mut modfunc_locals = String::new();
            if let Ok(locals_reader) = function_body.get_locals_reader() {
                for local in locals_reader {
                    let (reps, val_type) = local.unwrap();
                    let val_type = translate_value_type(&val_type, "a function local")?;
                    for _ in 0..reps {
                        modfunc_locals.push_str(format!("{val_type} :: ").as_str());
                    }
                }
            }
            modfunc_locals.push_str("nil");

            let local_name_map = self
                .func_locals_name_map
                .as_ref()
                .and_then(|func_locals_name_map| func_locals_name_map.get(&abs_index).cloned());
            let ctx = OperatorContext { local_name_map };
            let modfunc_body =
                translate_expr(&mut function_body.get_operators_reader()?, ctx, remap)?;

            self.translated_functions_string
                .push_str(format!("Definition {func_name} : module_func := ").as_str());
            self.translated_functions_string.push_str(LCB);
            self.translated_functions_string
                .push_str(format!("  modfunc_type := {modfunc_type}%N;\n").as_str());
            self.translated_functions_string
                .push_str(format!("  modfunc_locals := {modfunc_locals};\n").as_str());
            self.translated_functions_string
                .push_str(format!("  modfunc_body :=\n{modfunc_body};\n").as_str());
            self.translated_functions_string.push_str(RCB_DOT);
            self.translated_functions_string.push('\n');
        }
        Ok(())
    }
}

//Inductive reference_type
fn translate_ref_type(ref_type: &RefType) -> anyhow::Result<String> {
    if *ref_type == RefType::FUNCREF {
        Ok(String::from("T_funcref"))
    } else if *ref_type == RefType::EXTERNREF {
        Ok(String::from("T_externref"))
    } else {
        Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
            description: format!("reference type {ref_type:?}"),
        }))
    }
}

//Inductive value_type
/// Translates one `value_type`, the single chokepoint for every position a type
/// can occupy: function parameters and results, locals, globals, and block
/// result types.
///
/// `role` names that position ("a function parameter", "a local", …) and is
/// spelled into the rejection message. A `.wasm` carries no source locations and
/// translation stops at the first offending construct, so the role is what
/// narrows the search in a foreign binary.
fn translate_value_type(val_type: &wpValType, role: &'static str) -> anyhow::Result<String> {
    let res = match val_type {
        wpValType::I32 => "T_num T_i32",
        wpValType::I64 => "T_num T_i64",
        // The proof model's `number_type` is `T_i32 | T_i64` and it declares no
        // `T_vec` constructor at all, so a float or vector is unrepresentable in
        // a *signature* even when no float or vector instruction appears in any
        // body. Rejecting here rather than at the five call sites keeps that
        // leak closed in one place.
        wpValType::F32 => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: format!(
                    "floating-point value type `f32` in {role} (the wasm-verifier proof contract covers no floating-point types)"
                ),
            }));
        }
        wpValType::F64 => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: format!(
                    "floating-point value type `f64` in {role} (the wasm-verifier proof contract covers no floating-point types)"
                ),
            }));
        }
        wpValType::V128 => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: format!(
                    "vector value type `v128` in {role} (SIMD proposal — the wasm-verifier proof contract covers no vector types)"
                ),
            }));
        }
        wpValType::Ref(ref_type) => {
            let ref_type_translated = translate_ref_type(ref_type)?;
            return Ok(format!("T_ref {ref_type_translated}"));
        }
    };
    Ok(res.to_string())
}

//Record module_import
fn translate_module_import(import: &Import) -> anyhow::Result<String> {
    let imp_name = String::from(import.name);
    let imp_module = String::from(import.module);
    // let definition_name =
    //     imp_module.clone() + &imp_name.clone().remove(0).to_uppercase().to_string();
    let imp_desc = translate_module_import_desc(import)?;
    Ok(format!("Mi \"{imp_module}\" \"{imp_name}\" ({imp_desc})"))
}

//Inductive module_import_desc
fn translate_module_import_desc(import: &Import) -> anyhow::Result<String> {
    let res = match import.ty {
        TypeRef::Func(index) => format!("MID_func {index}%N"),
        TypeRef::Global(global_type) => {
            let tg_mut = translate_mutability(global_type.mutable);
            let tg_t = translate_value_type(&global_type.content_type, "an imported global")?;
            format!("MID_global {{|tg_mut := {tg_mut}; tg_t := {tg_t}|}}")
        }
        TypeRef::Memory(memory_type) => {
            let limits = translate_memory_type_limits(&memory_type)?;
            format!("MID_mem {limits}")
        }
        // `MID_table` takes a whole `table_type`, not the `limits` its sibling
        // `MID_mem` takes: the element type is a second field of that record and
        // has to be spelled alongside the limits.
        TypeRef::Table(table_type) => {
            let tt_limits = translate_table_type_limits(&table_type)?;
            let tt_elem_type = translate_ref_type(&table_type.element_type)?;
            format!("MID_table {{|tt_limits := {tt_limits}; tt_elem_type := {tt_elem_type}|}}")
        }
        TypeRef::Tag(_) => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: "tag import (exception-handling proposal)".into(),
            }));
        }
    };
    Ok(res)
}

//Inductive mutability
fn translate_mutability(mutable: bool) -> String {
    if mutable {
        "MUT_var".to_string()
    } else {
        "MUT_const".to_string()
    }
}

//Record limits
fn translate_table_type_limits(table_type: &TableType) -> anyhow::Result<String> {
    let lim_min = format!("{}%N", table_type.initial);
    let lim_max = match table_type.maximum {
        Some(max) => format!("Some({max}%N)"),
        None => "None".to_string(),
    };
    Ok(format!("{{|lim_min := {lim_min}; lim_max := {lim_max}|}}"))
}

//Record limits
fn translate_memory_type_limits(memory_type: &MemoryType) -> anyhow::Result<String> {
    // The target model (`Mm {|lim_min; lim_max|}`) has no field for `memory64`,
    // `shared`, or a custom page size, so any such memory would be silently
    // re-encoded as a 32-bit, non-shared, default-page-size machine — a `.v`
    // describing a machine the `.wasm` is not. Reject rather than miscompile the
    // proof artifact (defense in depth behind the linker's shape guard; audit
    // C-4/L-1).
    if memory_type.memory64 {
        return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
            description: "memory64 (i64-addressed) linear memory".into(),
        }));
    }
    if memory_type.shared {
        return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
            description: "shared linear memory (threads proposal)".into(),
        }));
    }
    if memory_type.page_size_log2.is_some() {
        return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
            description: "linear memory with a custom page size".into(),
        }));
    }
    let lim_min = format!("{}%N", memory_type.initial);
    let lim_max = match memory_type.maximum {
        Some(max) => format!("Some({max}%N)"),
        None => "None".to_string(),
    };
    Ok(format!("{{|lim_min := {lim_min}; lim_max := {lim_max}|}}"))
}

//Inductive translate_export_module
fn translate_export_module(export: &Export, remap: &FuncRemap) -> anyhow::Result<String> {
    let modexp_name = export.name;
    let modexp_desc = translate_module_export_desc(export, remap)?;
    Ok(format!("Me \"{modexp_name}\" ({modexp_desc})"))
}

//Inductive module_export_desc
fn translate_module_export_desc(export: &Export, remap: &FuncRemap) -> anyhow::Result<String> {
    let res = match export.kind {
        // A function export's index shifts down past every omitted spec
        // function; no export is ever dropped (spec functions are not
        // exportable, so an exported spec function — omitted or retained — is
        // a fail-closed error). Table/memory/global indices are not function
        // indices, so they pass through unchanged.
        inf_wasmparser::ExternalKind::Func => {
            format!(
                "MED_func {}%N",
                remap.referenced_instantiated(export.index)?
            )
        }
        inf_wasmparser::ExternalKind::Table => format!("MED_table {}%N", export.index),
        inf_wasmparser::ExternalKind::Memory => format!("MED_mem {}%N", export.index),
        inf_wasmparser::ExternalKind::Global => format!("MED_global {}%N", export.index),
        inf_wasmparser::ExternalKind::Tag => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: "tag export (exception-handling proposal)".into(),
            }));
        }
    };
    Ok(res)
}

//Record table_type
fn translate_table_type(table: &Table) -> anyhow::Result<String> {
    let tt_limits = translate_table_type_limits(&table.ty)?;
    let tt_elem_type = translate_ref_type(&table.ty.element_type)?;
    Ok(format!("Mt {tt_limits} {tt_elem_type}"))
}

//Definition memory_type
fn translate_memory_type(memory_type: &MemoryType) -> anyhow::Result<String> {
    let limits = translate_memory_type_limits(memory_type)?;
    Ok(format!("Mm {limits}"))
}

//Record global_type
fn translate_global(global: &Global, remap: &FuncRemap) -> anyhow::Result<String> {
    let tg_mut = translate_mutability(global.ty.mutable);
    let tg_t = translate_value_type(&global.ty.content_type, "a global")?;
    let mg_init = translate_expr(
        &mut global.init_expr.get_operators_reader(),
        OperatorContext::default(),
        remap,
    )?;
    Ok(format!("Mg {tg_mut} ({tg_t}) ({mg_init})"))
}

//Inductive module_datamode
fn translate_module_datamode(data: &Data, remap: &FuncRemap) -> anyhow::Result<String> {
    let res = match &data.kind {
        DataKind::Active {
            memory_index,
            offset_expr,
        } => {
            let expression = translate_expr(
                &mut offset_expr.get_operators_reader(),
                OperatorContext::default(),
                remap,
            )?;
            format!("MD_active {memory_index}%N ({expression})")
        }
        DataKind::Passive => "MD_passive".to_string(),
    };
    Ok(res)
}

enum ExpressionPart<'a> {
    Operator(Operator<'a>),
    Block(BlockExpr<'a>),
    Condition(ConditionExpr<'a>),
}

struct BlockExpr<'a> {
    label: Operator<'a>,
    parts: Expression<'a>,
}

struct ConditionExpr<'a> {
    label: Operator<'a>,
    then_arm: Expression<'a>,
    else_arm: Expression<'a>,
}

/// Per-function data the operator translators consult while rendering a
/// function body. Bundling it in one struct keeps the operator-translation
/// signatures stable as body-level state accrues, instead of widening each
/// signature independently.
#[derive(Default)]
struct OperatorContext {
    /// Local index → source name, used to annotate `BI_local_*` with the
    /// original variable name as a Rocq comment.
    local_name_map: Option<HashMap<u32, String>>,
}

#[derive(Default)]
struct Expression<'a> {
    parts: Vec<ExpressionPart<'a>>,
    ctx: OperatorContext,
}

impl Expression<'_> {
    fn last_part(&'_ self) -> Option<&'_ ExpressionPart<'_>> {
        self.parts.last()
    }

    /// Renders this expression tree to its Rocq list form, indenting nested
    /// blocks by `tabs_count` levels.
    ///
    /// `depth` bounds the self-recursion independently of the indentation: a
    /// body nested deeper than [`MAX_EXPRESSION_DEPTH`] is rejected with a
    /// recoverable [`WasmToVError::UnsupportedFeature`] rather than recursing to
    /// stack exhaustion (an unrecoverable `abort()`). The bound mirrors the one
    /// in `translate_expression`, so a body that built its tree without
    /// overflowing also renders without overflowing.
    fn print_with_offset(
        &self,
        tabs_count: usize,
        depth: usize,
        remap: &FuncRemap,
    ) -> anyhow::Result<String> {
        if depth >= MAX_EXPRESSION_DEPTH {
            return Err(too_deeply_nested_err());
        }
        let mut res = String::new();
        let offset = "  ".repeat(tabs_count);
        for part in &self.parts {
            match part {
                ExpressionPart::Operator(op) => match op {
                    Operator::Else | Operator::End => {}
                    _ => {
                        res.push_str(offset.as_str());
                        res.push_str(translate_basic_operator(op, &self.ctx, remap)?.as_str());
                        res.push_str(LIST_EXT);
                    }
                },
                ExpressionPart::Block(block) => {
                    res.push_str(offset.as_str());
                    res.push_str(
                        translate_basic_operator(&block.label, &self.ctx, remap)?.as_str(),
                    );
                    res.push_str(" (\n");
                    res.push_str(
                        block
                            .parts
                            .print_with_offset(tabs_count + 1, depth + 1, remap)?
                            .as_str(),
                    );
                    res.push_str(") ");
                    res.push_str("::\n");
                }
                ExpressionPart::Condition(cond) => {
                    res.push_str(offset.as_str());
                    res.push_str(translate_basic_operator(&cond.label, &self.ctx, remap)?.as_str());
                    res.push_str(" (\n");
                    res.push_str(
                        cond.then_arm
                            .print_with_offset(tabs_count + 1, depth + 1, remap)?
                            .as_str(),
                    );
                    res.push_str(") (\n");
                    res.push_str(
                        cond.else_arm
                            .print_with_offset(tabs_count + 1, depth + 1, remap)?
                            .as_str(),
                    );
                    res.push_str(") ");
                    res.push_str("::\n");
                }
            }
        }
        res.push_str(format!("{offset}nil").as_str());
        Ok(res)
    }
}

/// Maximum structured-control-flow nesting depth the translator recurses
/// through before rejecting a body as too deeply nested.
///
/// `translate_expression` (tree build) and [`Expression::print_with_offset`]
/// (render) are mutually-bounded self-recursive: a body of N nested blocks
/// recurses N deep. A Rust stack overflow is an `abort()` that bypasses every
/// `?`/`Err` path, so an adversarial external `.wasm` with thousands of nested
/// blocks would crash the proof path (SIGABRT) instead of failing cleanly.
/// Capping the depth turns that DoS into a recoverable
/// [`WasmToVError::UnsupportedFeature`]. The bound is far above any nesting a
/// real Inference function produces and comfortably below the depth at which
/// either pass would exhaust even a small (2 MiB) thread stack.
const MAX_EXPRESSION_DEPTH: usize = 256;

fn too_deeply_nested_err() -> anyhow::Error {
    anyhow::anyhow!(WasmToVError::UnsupportedFeature {
        description: format!(
            "function body nests structured control flow deeper than {MAX_EXPRESSION_DEPTH} levels"
        ),
    })
}

fn translate_expression<'a>(
    operators_reader: &mut OperatorsIterator<'a>,
    depth: usize,
) -> anyhow::Result<Expression<'a>> {
    if depth >= MAX_EXPRESSION_DEPTH {
        return Err(too_deeply_nested_err());
    }
    let mut result = Expression::default();
    while let Some(next_operator) = operators_reader.next() {
        let next_operator = next_operator.as_ref().unwrap();
        match next_operator {
            inf_wasmparser::Operator::Block { .. }
            | inf_wasmparser::Operator::Loop { .. }
            | inf_wasmparser::Operator::Forall { .. }
            | inf_wasmparser::Operator::Exists { .. }
            | inf_wasmparser::Operator::Assume { .. }
            | inf_wasmparser::Operator::Unique { .. } => {
                let block_operations = translate_expression(operators_reader, depth + 1)?;
                let block = BlockExpr {
                    label: next_operator.to_owned(),
                    parts: block_operations,
                };
                result.parts.push(ExpressionPart::Block(block));
            }
            inf_wasmparser::Operator::If { .. } => {
                let then_arm = translate_expression(operators_reader, depth + 1)?;
                let else_arm = if matches!(
                    then_arm.last_part().unwrap(),
                    ExpressionPart::Operator(Operator::End)
                ) {
                    Expression::default()
                } else {
                    translate_expression(operators_reader, depth + 1)?
                };

                let condition = ConditionExpr {
                    label: next_operator.to_owned(),
                    then_arm,
                    else_arm,
                };
                result.parts.push(ExpressionPart::Condition(condition));
            }
            inf_wasmparser::Operator::Else | inf_wasmparser::Operator::End => {
                result
                    .parts
                    .push(ExpressionPart::Operator(next_operator.to_owned()));
                break;
            }
            _ => result
                .parts
                .push(ExpressionPart::Operator(next_operator.to_owned())),
        }
    }
    Ok(result)
}

fn translate_expr(
    operators_reader: &mut OperatorsReader,
    ctx: OperatorContext,
    remap: &FuncRemap,
) -> anyhow::Result<String> {
    let mut peekable_operators_reader = operators_reader.clone().into_iter();
    let mut expression = translate_expression(&mut peekable_operators_reader, 0)?;
    expression.ctx = ctx;
    // Render through the fallible `print_with_offset` directly rather than the
    // `Display` impl, so that an unsupported operator surfaces as a returned
    // `WasmToVError` instead of being swallowed into placeholder text.
    expression.print_with_offset(2, 0, remap)
}

fn translate_block_type(block_type: &BlockType) -> anyhow::Result<String> {
    let res = match block_type {
        BlockType::Empty => "BT_valtype None".to_string(),
        BlockType::FuncType(index) => format!("BT_id {index}%N"),
        BlockType::Type(valtype) => {
            let valtype = translate_value_type(valtype, "a block result type")?;
            format!("BT_valtype (Some ({valtype}))")
        }
    };
    Ok(res)
}

//Record memarg
fn translate_memarg(memarg: &inf_wasmparser::MemArg) -> anyhow::Result<String> {
    let memarg_offset = memarg.offset.to_string();
    let memarg_align = memarg.align.to_string();
    Ok(format!("Ma {memarg_offset}%N {memarg_align}%N"))
}

/// Renders one element segment as a `module_element` record.
///
/// The contract's `modelem_init` is a list of *initializer expressions*, and
/// the segment's mode is a separate `modelem_mode` field. WASM's binary format
/// also admits a shorthand in which a segment carries bare function indexes
/// instead of expressions; that shorthand is desugared here exactly as the WASM
/// specification defines it — index `i` becomes the one-instruction expression
/// `ref.func i` — so both item forms produce the same record shape, and both
/// carry their function index through the same remap.
fn translate_element(element: &Element, remap: &FuncRemap) -> anyhow::Result<String> {
    let mut res = String::new();
    // let id = get_id();
    let modelem_mode = match &element.kind {
        ElementKind::Active {
            table_index,
            offset_expr,
        } => {
            // The active-element table index is a table index, not a function
            // index, so it is not renumbered.
            let tableidx = table_index.unwrap_or_default();
            let expr = translate_expr(
                &mut offset_expr.get_operators_reader(),
                OperatorContext::default(),
                remap,
            )?;
            format!("ME_active {tableidx}%N ({expr})")
        }
        ElementKind::Passive => "ME_passive".to_string(),
        ElementKind::Declared => "ME_declarative".to_string(),
    };
    let modelem_type: String;
    let modelem_init = match &element.items {
        ElementItems::Expressions(reftype, elements) => {
            modelem_type = translate_ref_type(reftype)?;
            let mut expr_list = String::new();
            for result in elements.clone().into_iter_with_offsets() {
                let (_, expr_reader) = result?;
                let expr = translate_expr(
                    &mut expr_reader.get_operators_reader(),
                    OperatorContext::default(),
                    remap,
                )?;
                expr_list.push_str(format!("({expr})").as_str());
                expr_list.push_str(" ::\n");
            }
            format!("{expr_list}nil")
        }
        ElementItems::Functions(elements) => {
            // Each item is a function index into the instantiated space;
            // renumber it past every omitted spec function before wrapping it
            // in its `ref.func` initializer expression.
            modelem_type = "T_funcref".to_string();
            let mut expr_list = String::new();
            for result in elements.clone().into_iter_with_offsets() {
                let (_, index) = result?;
                let target = remap.referenced_instantiated(index)?;
                expr_list.push_str(format!("(BI_ref_func {target}%N :: nil)").as_str());
                expr_list.push_str(" ::\n");
            }
            format!("{expr_list}nil")
        }
    };
    res.push_str("{|\n");
    res.push_str(format!("modelem_type := {modelem_type};\n").as_str());
    res.push_str(format!("modelem_init :=\n{modelem_init};\n").as_str());
    res.push_str(format!("modelem_mode := {modelem_mode};\n").as_str());
    res.push_str("|}");
    Ok(res)
}

// struct ValType {}

// struct FunctionType {
//     id: String,
//     ft_params: Vec<String>,
//     ft_results: Vec<String>,
// }

// impl FunctionType {
//     fn name(&self) -> String {
//         format!("ft_{}", self.id)
//     }
// }

// impl Display for FunctionType {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         let mut res = String::new();
//         let name = self.name();
//         res.push_str(format!("Definition {name} : function_type :=\n").as_str());
//         res.push_str(RLB);
//         let ft_params = String::from("(") + &self.ft_params.join(" :: ") + &String::from("nil)");
//         res.push_str(format!("ft_params := {ft_params}").as_str());
//         let ft_results = String::from("(") + &self.ft_results.join(" :: ") + &String::from("nil)");
//         res.push_str(format!("ft_results := {ft_results}").as_str());
//         res.push_str(RRB);
//         res.push_str(".\n");
//         write!(f, "{}", res)
//     }
// }

//Inductive function_type
fn translate_function_type(rec_group: &RecGroup) -> anyhow::Result<String> {
    let mut res = String::new();
    // let id = get_id();
    for ty in rec_group.types() {
        match &ty.composite_type.inner {
            CompositeInnerType::Func(ft) => {
                let mut params_str = String::new();
                for param in ft.params() {
                    let val_type = translate_value_type(param, "a function parameter")?;
                    params_str.push_str(format!("{val_type} :: ").as_str());
                }
                params_str.push_str("nil");

                let mut results_str = String::new();
                for result in ft.results() {
                    let val_type = translate_value_type(result, "a function result")?;
                    results_str.push_str(format!("{val_type} :: ").as_str());
                }
                results_str.push_str("nil");

                res.push_str(format!("Tf ({params_str}) ({results_str})").as_str());
            }
            CompositeInnerType::Array(_)
            | CompositeInnerType::Struct(_)
            | CompositeInnerType::Cont(_) => {
                //TODO
            }
        }
    }
    Ok(res)
}

/// The rejection every unmodeled proposal family shares: valid WASM the
/// wasm-verifier proof contract does not cover, so there is nothing to lower it
/// to. Inference codegen emits none of these constructs, and each of them used to
/// hit an unimplemented-macro panic — a process abort on the linking path,
/// strictly worse than a diagnostic.
///
/// `label` names the family instead of printing `{operator:?}`, whose debug form
/// for these structured-payload variants is often large. The labels read the same
/// as the linker's `operator_family` today; nothing enforces that, and nothing
/// needs to — for a family both know, the linker rejects first on the CLI path,
/// so the two labels are never visible in one run.
fn unsupported_family(label: &str) -> anyhow::Error {
    anyhow::anyhow!(WasmToVError::UnsupportedFeature {
        description: format!("{label} (no lowering under the wasm-verifier proof contract)"),
    })
}

//Inductive basic_instruction
/// Translates one WASM operator into its Rocq `basic_instruction` term.
///
/// Index immediates carry an explicit `%N` scope; see the module-level
/// "Index Immediates" section for why.
fn translate_basic_operator(
    operator: &Operator,
    ctx: &OperatorContext,
    remap: &FuncRemap,
) -> anyhow::Result<String> {
    let operator = match operator {
        inf_wasmparser::Operator::Nop => "BI_nop".to_string(),
        inf_wasmparser::Operator::Unreachable => "BI_unreachable".to_string(),
        inf_wasmparser::Operator::Block { blockty } => {
            let blockty = translate_block_type(blockty)?;
            format!("BI_block ({blockty})")
        }
        Operator::Loop { blockty } => {
            let blockty = translate_block_type(blockty)?;
            format!("BI_loop ({blockty})")
        }
        Operator::If { blockty } => {
            let blockty = translate_block_type(blockty)?;
            format!("BI_if ({blockty})")
        }
        // Non-deterministic instructions have no counterpart in the vanilla
        // WasmCert proof model the wasm-verifier library targets. The bodies
        // the emitted module record keeps are executable (non-spec) functions —
        // where the language rule (analysis A042) bars non-det — and retained
        // `exists`/`unique` spec functions, whose reachability lowering is
        // vanilla WASM by construction (each `@` arrives as a hidden trailing
        // choice parameter, filters trap). Neither can carry one of these
        // opcodes from Inference-compiled code, so this rejection is
        // defense-in-depth for foreign or hand-crafted `.wasm`.
        Operator::Forall { .. }
        | Operator::Exists { .. }
        | Operator::Assume { .. }
        | Operator::Unique { .. }
        | Operator::I32Uzumaki { .. }
        | Operator::I64Uzumaki { .. } => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: "non-deterministic instruction in a function body the emitted \
                             module retains cannot be represented in the vanilla WasmCert \
                             proof model"
                    .into(),
            }));
        }
        Operator::Else => String::new(),
        Operator::End => String::new(),
        Operator::Br { relative_depth } => format!("BI_br {relative_depth}%N"),
        Operator::BrIf { relative_depth } => format!("BI_br_if {relative_depth}%N"),
        Operator::BrTable { targets } => {
            // `targets()` yields the explicit label vector only; the default
            // label is a separate immediate and a separate `BI_br_table`
            // operand. A table with no explicit targets (`br_table 0`) is
            // valid WASM, and still carries its default. Labels are relative
            // branch depths, not function indexes, so none is renumbered.
            let default = targets.default();
            if targets.is_empty() {
                format!("BI_br_table nil {default}%N")
            } else {
                let mut labelidx = String::new();
                for target in targets.targets() {
                    let id = target?;
                    labelidx.push_str(format!("{id}%N").as_str());
                    labelidx.push_str(" :: ");
                }
                labelidx.push_str("nil");
                format!("BI_br_table ({labelidx}) {default}%N")
            }
        }
        Operator::Return => "BI_return".to_string(),
        Operator::Call { function_index } => {
            // A `BI_call` operand indexes the emitted module's instantiated
            // function space; renumber it past every omitted spec function. No
            // body kept in the module — executable or retained — may call a
            // spec function (omitted ones are not executable; retained ones
            // are obligation subjects), so either target is a fail-closed
            // error.
            let target = remap.referenced_instantiated(*function_index)?;
            format!("BI_call {target}%N")
        }
        Operator::CallIndirect {
            type_index,
            table_index,
        } => format!("BI_call_indirect {type_index}%N {table_index}%N"),
        Operator::Drop => "BI_drop".to_string(),
        Operator::Select => "BI_select None".to_string(),
        Operator::LocalGet { local_index } => {
            if let Some(local_name_map) = &ctx.local_name_map {
                if local_name_map.contains_key(local_index) {
                    format!(
                        "BI_local_get {local_index}%N (*{}*)",
                        local_name_map.get(local_index).unwrap()
                    )
                } else {
                    format!("BI_local_get {local_index}%N")
                }
            } else {
                format!("BI_local_get {local_index}%N")
            }
        }
        Operator::LocalSet { local_index } => {
            if let Some(local_name_map) = &ctx.local_name_map {
                if local_name_map.contains_key(local_index) {
                    format!(
                        "BI_local_set {local_index}%N (*{}*)",
                        local_name_map.get(local_index).unwrap()
                    )
                } else {
                    format!("BI_local_set {local_index}%N")
                }
            } else {
                format!("BI_local_set {local_index}%N")
            }
        }
        Operator::LocalTee { local_index } => {
            if let Some(local_name_map) = &ctx.local_name_map {
                if local_name_map.contains_key(local_index) {
                    format!(
                        "BI_local_tee {local_index}%N (*{}*)",
                        local_name_map.get(local_index).unwrap()
                    )
                } else {
                    format!("BI_local_tee {local_index}%N")
                }
            } else {
                format!("BI_local_tee {local_index}%N")
            }
        }
        Operator::GlobalGet { global_index } => format!("BI_global_get {global_index}%N"),
        Operator::GlobalSet { global_index } => format!("BI_global_set {global_index}%N"),
        Operator::I32Load { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i32 None ({memarg})")
        }
        Operator::I64Load { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 None ({memarg})")
        }
        Operator::I32Load8S { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i32 (Some (Tp_i8, SX_S)) ({memarg})")
        }
        Operator::I32Load8U { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i32 (Some (Tp_i8, SX_U)) ({memarg})")
        }
        Operator::I32Load16S { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i32 (Some (Tp_i16, SX_S)) ({memarg})")
        }
        Operator::I32Load16U { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i32 (Some (Tp_i16, SX_U)) ({memarg})")
        }
        Operator::I64Load8S { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 (Some (Tp_i8, SX_S)) ({memarg})")
        }
        Operator::I64Load8U { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 (Some (Tp_i8, SX_U)) ({memarg})")
        }
        Operator::I64Load16S { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 (Some (Tp_i16, SX_S)) ({memarg})")
        }
        Operator::I64Load16U { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 (Some (Tp_i16, SX_U)) ({memarg})")
        }
        Operator::I64Load32S { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 (Some (Tp_i32, SX_S)) ({memarg})")
        }
        Operator::I64Load32U { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 (Some (Tp_i32, SX_U)) ({memarg})")
        }
        Operator::I32Store { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i32 None ({memarg})")
        }
        Operator::I64Store { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i64 None ({memarg})")
        }
        Operator::I32Store8 { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i32 (Some Tp_i8) ({memarg})")
        }
        Operator::I32Store16 { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i32 (Some Tp_i16) ({memarg})")
        }
        Operator::I64Store8 { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i64 (Some Tp_i8) ({memarg})")
        }
        Operator::I64Store16 { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i64 (Some Tp_i16) ({memarg})")
        }
        Operator::I64Store32 { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i64 (Some Tp_i32) ({memarg})")
        }
        Operator::MemorySize { mem } => {
            if *mem > 0 {
                return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                    description: "multi-memory (memory index > 0 on memory.size)".into(),
                }));
            }
            "BI_memory_size".to_string()
        }
        Operator::MemoryGrow { mem } => {
            if *mem > 0 {
                return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                    description: "multi-memory (memory index > 0 on memory.grow)".into(),
                }));
            }
            "BI_memory_grow".to_string()
        }
        Operator::I32Const { value } => {
            format!("BI_const_num (Vi32 {})", z_literal(i64::from(*value)))
        }
        Operator::I64Const { value } => format!("BI_const_num (Vi64 {})", z_literal(*value)),
        Operator::I32Eqz => "BI_testop T_i32 TO_eqz".to_string(),
        Operator::I32Eq => "BI_relop T_i32 (Relop_i ROI_eq)".to_string(),
        Operator::I32Ne => "BI_relop T_i32 (Relop_i ROI_ne)".to_string(),
        Operator::I32LtS => "BI_relop T_i32 (Relop_i (ROI_lt SX_S))".to_string(),
        Operator::I32LtU => "BI_relop T_i32 (Relop_i (ROI_lt SX_U))".to_string(),
        Operator::I32GtS => "BI_relop T_i32 (Relop_i (ROI_gt SX_S))".to_string(),
        Operator::I32GtU => "BI_relop T_i32 (Relop_i (ROI_gt SX_U))".to_string(),
        Operator::I32LeS => "BI_relop T_i32 (Relop_i (ROI_le SX_S))".to_string(),
        Operator::I32LeU => "BI_relop T_i32 (Relop_i (ROI_le SX_U))".to_string(),
        Operator::I32GeS => "BI_relop T_i32 (Relop_i (ROI_ge SX_S))".to_string(),
        Operator::I32GeU => "BI_relop T_i32 (Relop_i (ROI_ge SX_U))".to_string(),
        Operator::I64Eqz => "BI_testop T_i64 TO_eqz".to_string(),
        Operator::I64Eq => "BI_relop T_i64 (Relop_i ROI_eq)".to_string(),
        Operator::I64Ne => "BI_relop T_i64 (Relop_i ROI_ne)".to_string(),
        Operator::I64LtS => "BI_relop T_i64 (Relop_i (ROI_lt SX_S))".to_string(),
        Operator::I64LtU => "BI_relop T_i64 (Relop_i (ROI_lt SX_U))".to_string(),
        Operator::I64GtS => "BI_relop T_i64 (Relop_i (ROI_gt SX_S))".to_string(),
        Operator::I64GtU => "BI_relop T_i64 (Relop_i (ROI_gt SX_U))".to_string(),
        Operator::I64LeS => "BI_relop T_i64 (Relop_i (ROI_le SX_S))".to_string(),
        Operator::I64LeU => "BI_relop T_i64 (Relop_i (ROI_le SX_U))".to_string(),
        Operator::I64GeS => "BI_relop T_i64 (Relop_i (ROI_ge SX_S))".to_string(),
        Operator::I64GeU => "BI_relop T_i64 (Relop_i (ROI_ge SX_U))".to_string(),
        Operator::I32Clz => "BI_unop T_i32 (Unop_i UOI_clz)".to_string(),
        Operator::I32Ctz => "BI_unop T_i32 (Unop_i UOI_ctz)".to_string(),
        Operator::I32Popcnt => "BI_unop T_i32 (Unop_i UOI_popcnt)".to_string(),
        Operator::I32Add => "BI_binop T_i32 (Binop_i BOI_add)".to_string(),
        Operator::I32Sub => "BI_binop T_i32 (Binop_i BOI_sub)".to_string(),
        Operator::I32Mul => "BI_binop T_i32 (Binop_i BOI_mul)".to_string(),
        Operator::I32DivS => "BI_binop T_i32 (Binop_i (BOI_div SX_S))".to_string(),
        Operator::I32DivU => "BI_binop T_i32 (Binop_i (BOI_div SX_U))".to_string(),
        Operator::I32RemS => "BI_binop T_i32 (Binop_i (BOI_rem SX_S))".to_string(),
        Operator::I32RemU => "BI_binop T_i32 (Binop_i (BOI_rem SX_U))".to_string(),
        Operator::I32And => "BI_binop T_i32 (Binop_i BOI_and)".to_string(),
        Operator::I32Or => "BI_binop T_i32 (Binop_i BOI_or)".to_string(),
        Operator::I32Xor => "BI_binop T_i32 (Binop_i BOI_xor)".to_string(),
        Operator::I32Shl => "BI_binop T_i32 (Binop_i BOI_shl)".to_string(),
        Operator::I32ShrS => "BI_binop T_i32 (Binop_i (BOI_shr SX_S))".to_string(),
        Operator::I32ShrU => "BI_binop T_i32 (Binop_i (BOI_shr SX_U))".to_string(),
        Operator::I32Rotl => "BI_binop T_i32 (Binop_i BOI_rotl)".to_string(),
        Operator::I32Rotr => "BI_binop T_i32 (Binop_i BOI_rotr)".to_string(),
        Operator::I64Clz => "BI_unop T_i64 (Unop_i UOI_clz)".to_string(),
        Operator::I64Ctz => "BI_unop T_i64 (Unop_i UOI_ctz)".to_string(),
        Operator::I64Popcnt => "BI_unop T_i64 (Unop_i UOI_popcnt)".to_string(),
        Operator::I64Add => "BI_binop T_i64 (Binop_i BOI_add)".to_string(),
        Operator::I64Sub => "BI_binop T_i64 (Binop_i BOI_sub)".to_string(),
        Operator::I64Mul => "BI_binop T_i64 (Binop_i BOI_mul)".to_string(),
        Operator::I64DivS => "BI_binop T_i64 (Binop_i (BOI_div SX_S))".to_string(),
        Operator::I64DivU => "BI_binop T_i64 (Binop_i (BOI_div SX_U))".to_string(),
        Operator::I64RemS => "BI_binop T_i64 (Binop_i (BOI_rem SX_S))".to_string(),
        Operator::I64RemU => "BI_binop T_i64 (Binop_i (BOI_rem SX_U))".to_string(),
        Operator::I64And => "BI_binop T_i64 (Binop_i BOI_and)".to_string(),
        Operator::I64Or => "BI_binop T_i64 (Binop_i BOI_or)".to_string(),
        Operator::I64Xor => "BI_binop T_i64 (Binop_i BOI_xor)".to_string(),
        Operator::I64Shl => "BI_binop T_i64 (Binop_i BOI_shl)".to_string(),
        Operator::I64ShrS => "BI_binop T_i64 (Binop_i (BOI_shr SX_S))".to_string(),
        Operator::I64ShrU => "BI_binop T_i64 (Binop_i (BOI_shr SX_U))".to_string(),
        Operator::I64Rotl => "BI_binop T_i64 (Binop_i BOI_rotl)".to_string(),
        Operator::I64Rotr => "BI_binop T_i64 (Binop_i BOI_rotr)".to_string(),
        // Vanilla WasmCert declares a full floating-point surface, but the
        // wasm-verifier proof contract covers none of it — no `T_f32`/`T_f64`,
        // no `relop_f`/`binop_f`/`unop_f`, no float value constructors — so a
        // float term has no verifiable lowering. Inference has no
        // floating-point types, so no Inference-compiled program reaches
        // this arm; it is reachable only from foreign or hand-crafted `.wasm`.
        // The float relop arms folded in here were additionally ill-typed,
        // wrapping integer `ROI_*` constructors inside the float `Relop_f`.
        Operator::F32Load { .. }
        | Operator::F64Load { .. }
        | Operator::F32Store { .. }
        | Operator::F64Store { .. }
        | Operator::F32Const { .. }
        | Operator::F64Const { .. }
        | Operator::F32Eq
        | Operator::F32Ne
        | Operator::F32Lt
        | Operator::F32Gt
        | Operator::F32Le
        | Operator::F32Ge
        | Operator::F64Eq
        | Operator::F64Ne
        | Operator::F64Lt
        | Operator::F64Gt
        | Operator::F64Le
        | Operator::F64Ge
        | Operator::F32Abs
        | Operator::F32Neg
        | Operator::F32Ceil
        | Operator::F32Floor
        | Operator::F32Trunc
        | Operator::F32Nearest
        | Operator::F32Sqrt
        | Operator::F32Add
        | Operator::F32Sub
        | Operator::F32Mul
        | Operator::F32Div
        | Operator::F32Min
        | Operator::F32Max
        | Operator::F32Copysign
        | Operator::F64Abs
        | Operator::F64Neg
        | Operator::F64Ceil
        | Operator::F64Floor
        | Operator::F64Trunc
        | Operator::F64Nearest
        | Operator::F64Sqrt
        | Operator::F64Add
        | Operator::F64Sub
        | Operator::F64Mul
        | Operator::F64Div
        | Operator::F64Min
        | Operator::F64Max
        | Operator::F64Copysign => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: format!(
                    "floating-point instruction {operator:?} (the wasm-verifier proof contract covers no floating-point surface)"
                ),
            }));
        }
        // The wasm-verifier proof contract covers no conversion surface
        // (`cvtop`/`BI_cvtop`), so every conversion is untranslatable —
        // including the three integer width conversions, which involve no float
        // type at all. Inference codegen emits no conversion of any kind, so
        // this arm is reachable only from foreign or hand-crafted `.wasm`.
        Operator::I32WrapI64
        | Operator::I32TruncF32S
        | Operator::I32TruncF32U
        | Operator::I32TruncF64S
        | Operator::I32TruncF64U
        | Operator::I64ExtendI32S
        | Operator::I64ExtendI32U
        | Operator::I64TruncF32S
        | Operator::I64TruncF32U
        | Operator::I64TruncF64S
        | Operator::I64TruncF64U
        | Operator::F32ConvertI32S
        | Operator::F32ConvertI32U
        | Operator::F32ConvertI64S
        | Operator::F32ConvertI64U
        | Operator::F32DemoteF64
        | Operator::F64ConvertI32S
        | Operator::F64ConvertI32U
        | Operator::F64ConvertI64S
        | Operator::F64ConvertI64U
        | Operator::F64PromoteF32
        | Operator::I32ReinterpretF32
        | Operator::I64ReinterpretF64
        | Operator::F32ReinterpretI32
        | Operator::F64ReinterpretI64
        | Operator::I32Extend8S
        | Operator::I32Extend16S
        | Operator::I64Extend8S
        | Operator::I64Extend16S
        | Operator::I64Extend32S
        | Operator::I32TruncSatF32S
        | Operator::I32TruncSatF32U
        | Operator::I32TruncSatF64S
        | Operator::I32TruncSatF64U
        | Operator::I64TruncSatF32S
        | Operator::I64TruncSatF32U
        | Operator::I64TruncSatF64S
        | Operator::I64TruncSatF64U => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: format!(
                    "conversion instruction {operator:?} (the wasm-verifier proof contract covers no conversion instructions, integer width conversions included)"
                ),
            }));
        }
        Operator::RefEq
        | Operator::StructNew { .. }
        | Operator::StructNewDefault { .. }
        | Operator::StructGet { .. }
        | Operator::StructGetS { .. }
        | Operator::StructGetU { .. }
        | Operator::StructSet { .. }
        | Operator::ArrayNew { .. }
        | Operator::ArrayNewDefault { .. }
        | Operator::ArrayNewFixed { .. }
        | Operator::ArrayNewData { .. }
        | Operator::ArrayNewElem { .. }
        | Operator::ArrayGet { .. }
        | Operator::ArrayGetS { .. }
        | Operator::ArrayGetU { .. }
        | Operator::ArraySet { .. }
        | Operator::ArrayLen
        | Operator::ArrayFill { .. }
        | Operator::ArrayCopy { .. }
        | Operator::ArrayInitData { .. }
        | Operator::ArrayInitElem { .. }
        | Operator::RefTestNonNull { .. }
        | Operator::RefTestNullable { .. }
        | Operator::RefCastNonNull { .. }
        | Operator::RefCastNullable { .. }
        | Operator::BrOnCast { .. }
        | Operator::BrOnCastFail { .. }
        | Operator::AnyConvertExtern
        | Operator::ExternConvertAny => {
            return Err(unsupported_family(
                "garbage collection (struct.new / array.new / ref.cast)",
            ));
        }
        Operator::RefI31 | Operator::I31GetS | Operator::I31GetU | Operator::RefI31Shared => {
            return Err(unsupported_family("i31 references (ref.i31 / i31.get_s)"));
        }
        Operator::MemoryInit { data_index, mem: _ } => format!("BI_memory_init {data_index}%N"),
        Operator::DataDrop { data_index } => format!("BI_data_drop {data_index}%N"),
        Operator::MemoryCopy {
            dst_mem: _,
            src_mem: _,
        } => "BI_memory_copy".to_string(),
        Operator::MemoryFill { mem: _ } => "BI_memory_fill".to_string(),
        Operator::TableInit { .. } | Operator::ElemDrop { .. } | Operator::TableCopy { .. } => {
            return Err(unsupported_family(
                "segment-indexed table initialization (table.init / elem.drop / table.copy)",
            ));
        }
        Operator::TypedSelect { .. } => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: "typed select (WasmCert supports it; no translator lowering is wired)"
                    .into(),
            }));
        }
        Operator::RefNull { .. } => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: "ref.null (typed reference instruction)".into(),
            }));
        }
        Operator::RefIsNull => "BI_ref_is_null".to_string(),
        Operator::RefFunc { function_index } => {
            // A `ref.func` operand indexes the instantiated function space, the
            // same space `BI_call` and the desugared element items index, so it
            // is renumbered past every omitted spec function and is fail-closed
            // on a reference to a spec function, omitted or retained.
            let target = remap.referenced_instantiated(*function_index)?;
            format!("BI_ref_func {target}%N")
        }
        Operator::TableFill { table } => format!("BI_table_fill {table}%N"),
        Operator::TableGet { table } => format!("BI_table_get {table}%N"),
        Operator::TableSet { table } => format!("BI_table_set {table}%N"),
        Operator::TableGrow { table } => format!("BI_table_grow {table}%N"),
        Operator::TableSize { table } => format!("BI_table_size {table}%N"),
        Operator::ReturnCall { .. } | Operator::ReturnCallIndirect { .. } => {
            return Err(unsupported_family(
                "tail calls (return_call / return_call_indirect)",
            ));
        }
        Operator::MemoryDiscard { .. } => {
            return Err(unsupported_family("memory.discard"));
        }
        Operator::MemoryAtomicNotify { memarg: _ }
        | Operator::MemoryAtomicWait32 { memarg: _ }
        | Operator::MemoryAtomicWait64 { memarg: _ }
        | Operator::AtomicFence
        | Operator::I32AtomicLoad { memarg: _ }
        | Operator::I64AtomicLoad { memarg: _ }
        | Operator::I32AtomicLoad8U { memarg: _ }
        | Operator::I32AtomicLoad16U { memarg: _ }
        | Operator::I64AtomicLoad8U { memarg: _ }
        | Operator::I64AtomicLoad16U { memarg: _ }
        | Operator::I64AtomicLoad32U { memarg: _ }
        | Operator::I32AtomicStore { memarg: _ }
        | Operator::I64AtomicStore { memarg: _ }
        | Operator::I32AtomicStore8 { memarg: _ }
        | Operator::I32AtomicStore16 { memarg: _ }
        | Operator::I64AtomicStore8 { memarg: _ }
        | Operator::I64AtomicStore16 { memarg: _ }
        | Operator::I64AtomicStore32 { memarg: _ }
        | Operator::I32AtomicRmwAdd { memarg: _ }
        | Operator::I64AtomicRmwAdd { memarg: _ }
        | Operator::I32AtomicRmw8AddU { memarg: _ }
        | Operator::I32AtomicRmw16AddU { memarg: _ }
        | Operator::I64AtomicRmw8AddU { memarg: _ }
        | Operator::I64AtomicRmw16AddU { memarg: _ }
        | Operator::I64AtomicRmw32AddU { memarg: _ }
        | Operator::I32AtomicRmwSub { memarg: _ }
        | Operator::I64AtomicRmwSub { memarg: _ }
        | Operator::I32AtomicRmw8SubU { memarg: _ }
        | Operator::I32AtomicRmw16SubU { memarg: _ }
        | Operator::I64AtomicRmw8SubU { memarg: _ }
        | Operator::I64AtomicRmw16SubU { memarg: _ }
        | Operator::I64AtomicRmw32SubU { memarg: _ }
        | Operator::I32AtomicRmwAnd { memarg: _ }
        | Operator::I64AtomicRmwAnd { memarg: _ }
        | Operator::I32AtomicRmw8AndU { memarg: _ }
        | Operator::I32AtomicRmw16AndU { memarg: _ }
        | Operator::I64AtomicRmw8AndU { memarg: _ }
        | Operator::I64AtomicRmw16AndU { memarg: _ }
        | Operator::I64AtomicRmw32AndU { memarg: _ }
        | Operator::I32AtomicRmwOr { memarg: _ }
        | Operator::I64AtomicRmwOr { memarg: _ }
        | Operator::I32AtomicRmw8OrU { memarg: _ }
        | Operator::I32AtomicRmw16OrU { memarg: _ }
        | Operator::I64AtomicRmw8OrU { memarg: _ }
        | Operator::I64AtomicRmw16OrU { memarg: _ }
        | Operator::I64AtomicRmw32OrU { memarg: _ }
        | Operator::I32AtomicRmwXor { memarg: _ }
        | Operator::I64AtomicRmwXor { memarg: _ }
        | Operator::I32AtomicRmw8XorU { memarg: _ }
        | Operator::I32AtomicRmw16XorU { memarg: _ }
        | Operator::I64AtomicRmw8XorU { memarg: _ }
        | Operator::I64AtomicRmw16XorU { memarg: _ }
        | Operator::I64AtomicRmw32XorU { memarg: _ }
        | Operator::I32AtomicRmwXchg { memarg: _ }
        | Operator::I64AtomicRmwXchg { memarg: _ }
        | Operator::I32AtomicRmw8XchgU { memarg: _ }
        | Operator::I32AtomicRmw16XchgU { memarg: _ }
        | Operator::I64AtomicRmw8XchgU { memarg: _ }
        | Operator::I64AtomicRmw16XchgU { memarg: _ }
        | Operator::I64AtomicRmw32XchgU { memarg: _ }
        | Operator::I32AtomicRmwCmpxchg { memarg: _ }
        | Operator::I64AtomicRmwCmpxchg { memarg: _ }
        | Operator::I32AtomicRmw8CmpxchgU { memarg: _ }
        | Operator::I32AtomicRmw16CmpxchgU { memarg: _ }
        | Operator::I64AtomicRmw8CmpxchgU { memarg: _ }
        | Operator::I64AtomicRmw16CmpxchgU { memarg: _ }
        | Operator::I64AtomicRmw32CmpxchgU { memarg: _ } => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: format!("atomic instruction {operator:?} (threads proposal)"),
            }));
        }
        // The SIMD proposal's `v128` type is outside the wasm-verifier proof
        // contract, which covers neither the vector value type nor any
        // vector instruction. Inference has no vector types, so this arm is
        // reachable only from foreign or hand-crafted `.wasm`. The arms folded in
        // here were also miswired beyond being undeclared — the four
        // `V128Load*Lane` variants emitted the *store* constructor, and the six
        // splats applied a three-argument constructor to two arguments.
        Operator::V128Load { .. }
        | Operator::V128Load8x8S { .. }
        | Operator::V128Load8x8U { .. }
        | Operator::V128Load16x4S { .. }
        | Operator::V128Load16x4U { .. }
        | Operator::V128Load32x2S { .. }
        | Operator::V128Load32x2U { .. }
        | Operator::V128Load8Splat { .. }
        | Operator::V128Load16Splat { .. }
        | Operator::V128Load32Splat { .. }
        | Operator::V128Load64Splat { .. }
        | Operator::V128Load32Zero { .. }
        | Operator::V128Load64Zero { .. }
        | Operator::V128Store { .. }
        | Operator::V128Load8Lane { .. }
        | Operator::V128Load16Lane { .. }
        | Operator::V128Load32Lane { .. }
        | Operator::V128Load64Lane { .. }
        | Operator::V128Store8Lane { .. }
        | Operator::V128Store16Lane { .. }
        | Operator::V128Store32Lane { .. }
        | Operator::V128Store64Lane { .. }
        | Operator::V128Const { .. }
        | Operator::I8x16Shuffle { .. }
        | Operator::I8x16ExtractLaneS { .. }
        | Operator::I8x16ExtractLaneU { .. }
        | Operator::I8x16ReplaceLane { .. }
        | Operator::I16x8ExtractLaneS { .. }
        | Operator::I16x8ExtractLaneU { .. }
        | Operator::I16x8ReplaceLane { .. }
        | Operator::I32x4ExtractLane { .. }
        | Operator::I32x4ReplaceLane { .. }
        | Operator::I64x2ExtractLane { .. }
        | Operator::I64x2ReplaceLane { .. }
        | Operator::F32x4ExtractLane { .. }
        | Operator::F32x4ReplaceLane { .. }
        | Operator::F64x2ExtractLane { .. }
        | Operator::F64x2ReplaceLane { .. }
        | Operator::I8x16Swizzle
        | Operator::I8x16Splat
        | Operator::I16x8Splat
        | Operator::I32x4Splat
        | Operator::I64x2Splat
        | Operator::F32x4Splat
        | Operator::F64x2Splat
        | Operator::I8x16Eq
        | Operator::I8x16Ne
        | Operator::I8x16LtS
        | Operator::I8x16LtU
        | Operator::I8x16GtS
        | Operator::I8x16GtU
        | Operator::I8x16LeS
        | Operator::I8x16LeU
        | Operator::I8x16GeS
        | Operator::I8x16GeU
        | Operator::I16x8Eq
        | Operator::I16x8Ne
        | Operator::I16x8LtS
        | Operator::I16x8LtU
        | Operator::I16x8GtS
        | Operator::I16x8GtU
        | Operator::I16x8LeS
        | Operator::I16x8LeU
        | Operator::I16x8GeS
        | Operator::I16x8GeU
        | Operator::I32x4Eq
        | Operator::I32x4Ne
        | Operator::I32x4LtS
        | Operator::I32x4LtU
        | Operator::I32x4GtS
        | Operator::I32x4GtU
        | Operator::I32x4LeS
        | Operator::I32x4LeU
        | Operator::I32x4GeS
        | Operator::I32x4GeU
        | Operator::I64x2Eq
        | Operator::I64x2Ne
        | Operator::I64x2LtS
        | Operator::I64x2GtS
        | Operator::I64x2LeS
        | Operator::I64x2GeS
        | Operator::F32x4Eq
        | Operator::F32x4Ne
        | Operator::F32x4Lt
        | Operator::F32x4Gt
        | Operator::F32x4Le
        | Operator::F32x4Ge
        | Operator::F64x2Eq
        | Operator::F64x2Ne
        | Operator::F64x2Lt
        | Operator::F64x2Gt
        | Operator::F64x2Le
        | Operator::F64x2Ge
        | Operator::V128Not
        | Operator::V128And
        | Operator::V128AndNot
        | Operator::V128Or
        | Operator::V128Xor
        | Operator::V128Bitselect
        | Operator::V128AnyTrue
        | Operator::I8x16Abs
        | Operator::I8x16Neg
        | Operator::I8x16Popcnt
        | Operator::I8x16AllTrue
        | Operator::I8x16Bitmask
        | Operator::I8x16NarrowI16x8S
        | Operator::I8x16NarrowI16x8U
        | Operator::I8x16Shl
        | Operator::I8x16ShrS
        | Operator::I8x16ShrU
        | Operator::I8x16Add
        | Operator::I8x16AddSatS
        | Operator::I8x16AddSatU
        | Operator::I8x16Sub
        | Operator::I8x16SubSatS
        | Operator::I8x16SubSatU
        | Operator::I8x16MinS
        | Operator::I8x16MinU
        | Operator::I8x16MaxS
        | Operator::I8x16MaxU
        | Operator::I8x16AvgrU
        | Operator::I16x8ExtAddPairwiseI8x16S
        | Operator::I16x8ExtAddPairwiseI8x16U
        | Operator::I16x8Abs
        | Operator::I16x8Neg
        | Operator::I16x8Q15MulrSatS
        | Operator::I16x8AllTrue
        | Operator::I16x8Bitmask
        | Operator::I16x8NarrowI32x4S
        | Operator::I16x8NarrowI32x4U
        | Operator::I16x8ExtendLowI8x16S
        | Operator::I16x8ExtendHighI8x16S
        | Operator::I16x8ExtendLowI8x16U
        | Operator::I16x8ExtendHighI8x16U
        | Operator::I16x8Shl
        | Operator::I16x8ShrS
        | Operator::I16x8ShrU
        | Operator::I16x8Add
        | Operator::I16x8AddSatS
        | Operator::I16x8AddSatU
        | Operator::I16x8Sub
        | Operator::I16x8SubSatS
        | Operator::I16x8SubSatU
        | Operator::I16x8Mul
        | Operator::I16x8MinS
        | Operator::I16x8MinU
        | Operator::I16x8MaxS
        | Operator::I16x8MaxU
        | Operator::I16x8AvgrU
        | Operator::I16x8ExtMulLowI8x16S
        | Operator::I16x8ExtMulHighI8x16S
        | Operator::I16x8ExtMulLowI8x16U
        | Operator::I16x8ExtMulHighI8x16U
        | Operator::I32x4ExtAddPairwiseI16x8S
        | Operator::I32x4ExtAddPairwiseI16x8U
        | Operator::I32x4Abs
        | Operator::I32x4Neg
        | Operator::I32x4AllTrue
        | Operator::I32x4Bitmask
        | Operator::I32x4ExtendLowI16x8S
        | Operator::I32x4ExtendHighI16x8S
        | Operator::I32x4ExtendLowI16x8U
        | Operator::I32x4ExtendHighI16x8U
        | Operator::I32x4Shl
        | Operator::I32x4ShrS
        | Operator::I32x4ShrU
        | Operator::I32x4Add
        | Operator::I32x4Sub
        | Operator::I32x4Mul
        | Operator::I32x4MinS
        | Operator::I32x4MinU
        | Operator::I32x4MaxS
        | Operator::I32x4MaxU
        | Operator::I32x4DotI16x8S
        | Operator::I32x4ExtMulLowI16x8S
        | Operator::I32x4ExtMulHighI16x8S
        | Operator::I32x4ExtMulLowI16x8U
        | Operator::I32x4ExtMulHighI16x8U
        | Operator::I64x2Abs
        | Operator::I64x2Neg
        | Operator::I64x2AllTrue
        | Operator::I64x2Bitmask
        | Operator::I64x2ExtendLowI32x4S
        | Operator::I64x2ExtendHighI32x4S
        | Operator::I64x2ExtendLowI32x4U
        | Operator::I64x2ExtendHighI32x4U
        | Operator::I64x2Shl
        | Operator::I64x2ShrS
        | Operator::I64x2ShrU
        | Operator::I64x2Add
        | Operator::I64x2Sub
        | Operator::I64x2Mul
        | Operator::I64x2ExtMulLowI32x4S
        | Operator::I64x2ExtMulHighI32x4S
        | Operator::I64x2ExtMulLowI32x4U
        | Operator::I64x2ExtMulHighI32x4U
        | Operator::F32x4Ceil
        | Operator::F32x4Floor
        | Operator::F32x4Trunc
        | Operator::F32x4Nearest
        | Operator::F32x4Abs
        | Operator::F32x4Neg
        | Operator::F32x4Sqrt
        | Operator::F32x4Add
        | Operator::F32x4Sub
        | Operator::F32x4Mul
        | Operator::F32x4Div
        | Operator::F32x4Min
        | Operator::F32x4Max
        | Operator::F32x4PMin
        | Operator::F32x4PMax
        | Operator::F64x2Ceil
        | Operator::F64x2Floor
        | Operator::F64x2Trunc
        | Operator::F64x2Nearest
        | Operator::F64x2Abs
        | Operator::F64x2Neg
        | Operator::F64x2Sqrt
        | Operator::F64x2Add
        | Operator::F64x2Sub
        | Operator::F64x2Mul
        | Operator::F64x2Div
        | Operator::F64x2Min
        | Operator::F64x2Max
        | Operator::F64x2PMin
        | Operator::F64x2PMax
        | Operator::I32x4TruncSatF32x4S
        | Operator::I32x4TruncSatF32x4U
        | Operator::F32x4ConvertI32x4S
        | Operator::F32x4ConvertI32x4U
        | Operator::I32x4TruncSatF64x2SZero
        | Operator::I32x4TruncSatF64x2UZero
        | Operator::F64x2ConvertLowI32x4S
        | Operator::F64x2ConvertLowI32x4U
        | Operator::F32x4DemoteF64x2Zero
        | Operator::F64x2PromoteLowF32x4
        | Operator::I8x16RelaxedSwizzle
        | Operator::I32x4RelaxedTruncF32x4S
        | Operator::I32x4RelaxedTruncF32x4U
        | Operator::I32x4RelaxedTruncF64x2SZero
        | Operator::I32x4RelaxedTruncF64x2UZero
        | Operator::F32x4RelaxedMadd
        | Operator::F32x4RelaxedNmadd
        | Operator::F64x2RelaxedMadd
        | Operator::F64x2RelaxedNmadd
        | Operator::I8x16RelaxedLaneselect
        | Operator::I16x8RelaxedLaneselect
        | Operator::I32x4RelaxedLaneselect
        | Operator::I64x2RelaxedLaneselect
        | Operator::F32x4RelaxedMin
        | Operator::F32x4RelaxedMax
        | Operator::F64x2RelaxedMin
        | Operator::F64x2RelaxedMax
        | Operator::I16x8RelaxedQ15mulrS
        | Operator::I16x8RelaxedDotI8x16I7x16S
        | Operator::I32x4RelaxedDotI8x16I7x16AddS => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: format!(
                    "vector instruction {operator:?} (SIMD proposal — the wasm-verifier proof contract covers no vector types)"
                ),
            }));
        }
        Operator::TryTable { .. } => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: "try_table (exception-handling instruction)".into(),
            }));
        }
        Operator::Throw { .. } => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: "throw (exception-handling instruction)".into(),
            }));
        }
        Operator::ThrowRef => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: "throw_ref (exception-handling instruction)".into(),
            }));
        }
        Operator::Try { .. }
        | Operator::Catch { .. }
        | Operator::Rethrow { .. }
        | Operator::Delegate { .. }
        | Operator::CatchAll => {
            return Err(unsupported_family(
                "legacy exception handling (try / catch / rethrow)",
            ));
        }
        Operator::GlobalAtomicGet {
            ordering: _,
            global_index: _,
        }
        | Operator::GlobalAtomicSet {
            ordering: _,
            global_index: _,
        }
        | Operator::GlobalAtomicRmwAdd {
            ordering: _,
            global_index: _,
        }
        | Operator::GlobalAtomicRmwSub {
            ordering: _,
            global_index: _,
        }
        | Operator::GlobalAtomicRmwAnd {
            ordering: _,
            global_index: _,
        }
        | Operator::GlobalAtomicRmwOr {
            ordering: _,
            global_index: _,
        }
        | Operator::GlobalAtomicRmwXor {
            ordering: _,
            global_index: _,
        }
        | Operator::GlobalAtomicRmwXchg {
            ordering: _,
            global_index: _,
        }
        | Operator::GlobalAtomicRmwCmpxchg {
            ordering: _,
            global_index: _,
        }
        | Operator::TableAtomicGet {
            ordering: _,
            table_index: _,
        }
        | Operator::TableAtomicSet {
            ordering: _,
            table_index: _,
        }
        | Operator::TableAtomicRmwXchg {
            ordering: _,
            table_index: _,
        }
        | Operator::TableAtomicRmwCmpxchg {
            ordering: _,
            table_index: _,
        }
        | Operator::StructAtomicGet {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicGetS {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicGetU {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicSet {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicRmwAdd {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicRmwSub {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicRmwAnd {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicRmwOr {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicRmwXor {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicRmwXchg {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicRmwCmpxchg {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::ArrayAtomicGet {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicGetS {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicGetU {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicSet {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicRmwAdd {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicRmwSub {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicRmwAnd {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicRmwOr {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicRmwXor {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicRmwXchg {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicRmwCmpxchg {
            ordering: _,
            array_type_index: _,
        } => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: format!("atomic GC instruction {operator:?} (GC + threads proposals)"),
            }));
        }
        Operator::CallRef { .. } => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: "call_ref (typed function reference instruction)".into(),
            }));
        }
        Operator::ReturnCallRef { .. } => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: "return_call_ref (typed function reference instruction)".into(),
            }));
        }
        Operator::RefAsNonNull | Operator::BrOnNull { .. } | Operator::BrOnNonNull { .. } => {
            return Err(unsupported_family(
                "typed function references (ref.as_non_null / br_on_null)",
            ));
        }
        Operator::ContNew { .. }
        | Operator::ContBind { .. }
        | Operator::Suspend { .. }
        | Operator::Resume { .. }
        | Operator::ResumeThrow { .. }
        | Operator::Switch { .. } => {
            return Err(unsupported_family(
                "stack switching (cont.new / resume / suspend)",
            ));
        }
        Operator::I64Add128 { .. }
        | Operator::I64Sub128 { .. }
        | Operator::I64MulWideS
        | Operator::I64MulWideU => {
            return Err(unsupported_family(
                "128-bit wide arithmetic (i64.add128 / i64.mul_wide_s)",
            ));
        }
        // Every variant the parser can currently produce is matched above. This
        // residual exists because `Operator` is `#[non_exhaustive]`: a variant
        // added upstream lands here and is refused rather than silently mistaken
        // for something else. The wording claims only that no translation exists,
        // which stays true whether or not the proof model could represent it.
        _ => {
            return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                description: format!(
                    "instruction {operator:?} (not translated to the WasmCert proof model)"
                ),
            }));
        }
    };
    Ok(operator.to_string())
}

/// Spells one byte as a term of the proof backend's `byte` type.
///
/// In the backend's `coq-wasm` dependency a `byte` is CompCert's
/// `Integers.byte`, built from a `Z` by the exported `encode`, and abbreviated
/// by two-digit uppercase hex notations in `byte_scope`. That notation block is
/// hand-written and covers 244 of the 256 values: `#12` .. `#19` and `#1C` ..
/// `#1F` are absent, and spelling one of those would emit syntax the backend
/// cannot parse even though the notation looks uniform. Those twelve values are
/// therefore written as the `encode` application the notation would have
/// abbreviated, which needs no scope and elaborates for every value.
///
/// The notation is preferred where it exists because it keeps a data segment
/// legible as the hex dump it came from.
fn byte_literal(byte: u8) -> String {
    if matches!(byte, 0x12..=0x19 | 0x1C..=0x1F) {
        format!("(encode {byte}%Z)")
    } else {
        format!("#{byte:02X}")
    }
}

/// Renders one data segment as a `module_data` record.
///
/// `moddata_init` is a `list byte`, whose elements are spelled by
/// [`byte_literal`].
fn translate_data(data: &Data, remap: &FuncRemap) -> anyhow::Result<String> {
    let mut res = String::new();
    let moddata_mode = translate_module_datamode(data, remap)?;
    let mut moddata_init = String::new();
    for &byte in data.data {
        moddata_init.push_str(byte_literal(byte).as_str());
        moddata_init.push_str(" :: ");
    }
    moddata_init.push_str("nil");
    res.push_str("{|\n");
    res.push_str(format!("    moddata_init := {moddata_init};\n").as_str());
    res.push_str(format!("    moddata_mode := {moddata_mode};\n").as_str());
    res.push_str("|}");
    Ok(res)
}

/// Returns a Rocq `Definition` name guaranteed not to collide with any name
/// already in `used_names`, recording the chosen name. On collision the WASM
/// function `index` is appended (`<base>_<index>`); should that already be
/// taken, a monotonically increasing suffix is added until the name is free.
fn unique_function_name(
    base_name: String,
    index: u32,
    used_names: &mut FxHashSet<String>,
) -> String {
    if used_names.insert(base_name.clone()) {
        return base_name;
    }
    let mut candidate = format!("{base_name}_{index}");
    let mut disambiguator = 0u32;
    while !used_names.insert(candidate.clone()) {
        candidate = format!("{base_name}_{index}_{disambiguator}");
        disambiguator += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem(memory64: bool, shared: bool, page_size_log2: Option<u32>) -> MemoryType {
        MemoryType {
            memory64,
            shared,
            initial: 1,
            maximum: Some(1),
            page_size_log2,
        }
    }

    fn assert_unsupported(result: anyhow::Result<String>, needle: &str) {
        let err = result.expect_err("a non-32-bit memory must be rejected");
        let Some(WasmToVError::UnsupportedFeature { description }) =
            err.downcast_ref::<WasmToVError>()
        else {
            panic!("expected UnsupportedFeature, got {err:?}");
        };
        assert!(
            description.contains(needle),
            "description names the feature: {description}"
        );
    }

    #[test]
    fn a_32_bit_memory_translates() {
        // The default 32-bit, non-shared, default-page-size memory is the only
        // shape the model encodes; it must still translate cleanly.
        let limits = translate_memory_type_limits(&mem(false, false, None))
            .expect("a standard 32-bit memory translates");
        assert_eq!(limits, "{|lim_min := 1%N; lim_max := Some(1%N)|}");
    }

    #[test]
    fn a_memory64_memory_is_rejected() {
        // C-4: the translator must never silently encode a 64-bit machine as the
        // 32-bit `Mm` record, which has no memory64 field.
        assert_unsupported(
            translate_memory_type_limits(&mem(true, false, None)),
            "memory64",
        );
    }

    #[test]
    fn a_shared_memory_is_rejected() {
        // L-1: a shared memory has no representable flag in the target model.
        assert_unsupported(
            translate_memory_type_limits(&mem(false, true, None)),
            "shared",
        );
    }

    #[test]
    fn a_custom_page_size_memory_is_rejected() {
        assert_unsupported(
            translate_memory_type_limits(&mem(false, false, Some(0))),
            "custom page size",
        );
    }
}
