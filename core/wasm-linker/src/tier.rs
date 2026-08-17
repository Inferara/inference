//! Memory-merge feasibility tiers.
//!
//! Whether an external function can be merged into the single shared linear
//! memory depends on what its transitive closure touches:
//!
//! - **Tier A** — pure: no memory, no global or table access, no data or
//!   element segments. Merge is a copy + re-index.
//! - **Tier B** — memory via caller-passed pointers only: the closure
//!   loads/stores through addresses the caller supplies, but defines no static
//!   data of its own, reads or writes no global, and names no table or element
//!   entry. The one shared memory is enough; no address relocation is
//!   needed. Admission to
//!   Tier B requires *proof* (via [`crate::provenance`]) that every memory
//!   address derives from a function parameter — a closure that fabricates an
//!   address from a constant or its own state would alias the host program's
//!   memory and is rejected as Tier C instead.
//!   Tier B admission proves *derivation*, not *containment*: the addresses are
//!   shown to flow from caller parameters, not to stay inside the region the
//!   caller granted. `p + 1048576`, `p + q` and `2p` are all admitted, and a
//!   loop may walk a caller pointer off the end of any buffer. See the "What
//!   this proves, and what it does not" section of [`crate::provenance`] before
//!   relying on Tier B for a bounds property; issue #420 tracks closing it.
//! - **Tier C** — own static data, global access, or table/element use: merging
//!   would require relocating data and rewriting absolute addresses, which
//!   needs relocation metadata the static merge does not consume. Rejected with
//!   a clear error.
//!
//! Tiers A and B turn on what the closure *uses*, not on what its module
//! *declares*. A global no body reads or writes, and a table with no element
//! segment that no instruction names, are inert and do not force Tier C — real
//! toolchain output is full of such boilerplate (lld emits a `__stack_pointer`
//! global into every `wasm32-unknown-unknown` artifact, and an empty
//! `(table 1 1 funcref)` into every `std` one). Data segments are the deliberate
//! exception and stay declaration-gated; [`tier_c_reasons`] explains why.

use crate::closure::{Closure, ClosureEffects};
use crate::parse::ParsedModule;
use crate::provenance;
use crate::LinkError;

/// The feasibility tier of a merge candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tier {
    /// Pure function: touches no memory, no global, no table, and its module
    /// declares no data or element segment.
    A,
    /// Memory through caller-passed pointers only.
    B,
}

/// Classifies a closure against its source module, returning the tier or a
/// [`LinkError::RequiresRelocatableBuild`] for Tier-C inputs.
///
/// A module is Tier C when it carries any of the relocation-sensitive
/// constructs: its own data or element segments, a closure that reads or writes
/// a global (a baked-in constant or per-module mutable state), or a closure that
/// names the table space (an indirect call or a `table.*`/`ref.func` operator).
/// These imply absolute addresses or per-module state that a position-naive
/// static merge cannot reconcile across two modules sharing one memory. A
/// declared-but-untouched global or table is not among them — see
/// [`tier_c_reasons`].
///
/// A memory-touching closure is admitted to Tier B **only** when the
/// address-provenance analysis ([`provenance::verify_param_addressing`]) proves
/// every memory access — in the closure `root` and in every function it
/// transitively calls — addresses memory through a value derived from the
/// **root export's** parameters, on every reachable control-flow path. The
/// analysis is interprocedural: the root's parameters are the trusted caller
/// pointers, and an inner function's parameter is trusted only when *every*
/// reachable call site passes it a param-derived argument (a sound greatest
/// fixpoint over the call graph that handles self- and mutual recursion). A
/// closure that fabricates a memory address from a constant, a module-internal
/// source, a parameter-cancelling computation (`param - param`, `param & 0`, …),
/// a value laundered across a `call` boundary that the call site does not
/// justify, or an indirect/table-dispatched call result would silently alias the
/// host program's own linear memory, so it is rejected as Tier C rather than
/// merged.
pub(crate) fn classify(
    module: &ParsedModule,
    closure: &Closure,
    root: u32,
    field: &str,
) -> Result<Tier, LinkError> {
    let reasons = tier_c_reasons(module, &closure.effects);
    if !reasons.is_empty() {
        return Err(LinkError::RequiresRelocatableBuild {
            field: field.to_string(),
            reasons,
        });
    }

    if closure.effects.uses_memory {
        provenance::verify_param_addressing(module, &closure.local_func_indices, root, field)?;
        Ok(Tier::B)
    } else {
        Ok(Tier::A)
    }
}

/// Collects every reason the module fails Tier-A/B feasibility. Empty means the
/// module is mergeable.
///
/// Globals and table *use* are gated on use; data and element segments on
/// declaration. Each of the three declaration-gated or use-gated choices rests
/// on a different argument, and they are not interchangeable:
///
/// - An **active data segment** writes linear memory at instantiation whether
///   or not any instruction names it, so dropping an unreferenced one changes
///   what the merged program observes. (A *passive* segment is inert until a
///   `memory.init` names it, so it could in principle be dropped — but
///   [`crate::parse`] keeps only `data_count` and discards each segment's kind,
///   so the two cannot be told apart here.) This is a *correctness* argument.
/// - An **element segment** is rejected on declaration as **conservatism**, not
///   on the data-segment argument. Dropping one is in fact unobservable: the
///   merged output declares no table for it to initialize, so nothing could read
///   what it wrote. It stays rejected because an element segment is a strong
///   signal of a module built around indirect dispatch, and admitting one would
///   silently discard a construct the author wrote. Relaxing it is a deliberate
///   decision, not an oversight to clean up.
/// - A **global** nothing reads or writes, and a **table** with no element
///   segment that no instruction names, are inert: nothing observes them. Every
///   `wasm32-unknown-unknown` artifact carries lld's `__stack_pointer` global,
///   and a `std` build carries an empty `(table 1 1 funcref)`; rejecting on the
///   declaration would exclude every real toolchain artifact over boilerplate a
///   leaf integer function never touches.
///
/// Dropping the external's globals and tables from the merged output is safe
/// precisely because [`ClosureEffects`] is *closure-scoped*: it is accumulated
/// over the bodies reachable from the root export, so a closure admitted with
/// `uses_globals == false` and `uses_tables == false` contains no operator
/// naming either index space.
///
/// That property is load-bearing for **globals** specifically. [`crate::merge`]
/// re-emits the main module's global section and [`crate::rewrite::IndexMap`]
/// remaps function and type indices only, so a leaked `global.get 0` in a merged
/// body would rebind to *main's* first global and — when the types agree, as two
/// `i32` globals do — still pass post-merge validation. Wrong value, no
/// diagnostic. For **tables** the failure mode is benign by comparison: the merge
/// emits no table section at all, so a leaked table operator names a table the
/// output does not have and post-merge validation rejects it as an unknown table.
/// That is fail-safe, and it is why the global half of this argument is the one
/// that has to hold.
fn tier_c_reasons(module: &ParsedModule, effects: &ClosureEffects) -> Vec<String> {
    let mut reasons = Vec::new();

    if module.data_count > 0 || effects.uses_data_segments {
        reasons.push("defines or initializes its own static data segments".to_string());
    }
    if effects.uses_globals {
        reasons.push("reads or writes module globals".to_string());
    }
    // Split from the element-segment signal below: neither implies the other. A
    // closure may `call_indirect` through a table no segment initializes, and a
    // module may carry an element segment no body ever reaches. One shared
    // string would have named a construct the module does not have.
    if effects.uses_tables {
        reasons.push("performs an indirect call or otherwise names the table space".to_string());
    }
    if module.element_count > 0 {
        reasons.push("declares an element segment".to_string());
    }

    reasons
}

#[cfg(test)]
mod tests {
    //! Tier classification over hand-built modules, exercising the boundary
    //! between a *declared* global/table and a *used* one.
    //!
    //! Every `cargo build --target wasm32-unknown-unknown` artifact carries an
    //! lld-synthesized `__stack_pointer` mutable global, and a `std` build also
    //! carries an empty `(table 1 1 funcref)`. A leaf integer function never
    //! reads either. These tests pin that such a declaration alone does not
    //! force Tier C, while any operator that touches the global or table space
    //! still does — and that a *data* segment stays declaration-gated.

    use super::*;
    use crate::closure;
    use crate::parse::ParsedModule;
    use inf_wasmparser::{BinaryReader, FunctionBody, Operator};

    fn parse(wat: &str) -> ParsedModule {
        let bytes = wat::parse_str(wat).expect("valid WAT");
        ParsedModule::parse(&bytes).expect("parse")
    }

    /// Classifies the closure of `root` in the module assembled from `wat`.
    fn classify_root(wat: &str, root_export: &str) -> Result<Tier, LinkError> {
        let module = parse(wat);
        let root = module
            .exported_func_index(root_export)
            .expect("root export present");
        let cl = closure::compute(&module, root).expect("closure computes");
        classify(&module, &cl, root, root_export)
    }

    /// The Tier-C reasons for `root`'s closure, or an empty vector when the
    /// module is mergeable.
    fn reasons_for(wat: &str, root_export: &str) -> Vec<String> {
        match classify_root(wat, root_export) {
            Ok(_) => Vec::new(),
            Err(LinkError::RequiresRelocatableBuild { reasons, .. }) => reasons,
            Err(other) => panic!("expected a tier verdict, got {other:?}"),
        }
    }

    /// A leaf integer function alongside an lld-shaped `__stack_pointer` global
    /// that nothing reads — the shape every `wasm32-unknown-unknown` artifact
    /// has.
    const UNUSED_STACK_POINTER: &str = r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (global $__stack_pointer (mut i32) (i32.const 1048576))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#;

    /// The same leaf function alongside the empty funcref table an lld `std`
    /// build emits. No element segment initializes it, and no body names it.
    const UNUSED_TABLE: &str = r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (table (;0;) 1 1 funcref)
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#;

    /// A module whose *body set* contains a global-naming operator while the
    /// *closure* of its exported root does not: `sum` is pure, and the sibling
    /// `read_state` that reads the global is unreachable from it.
    ///
    /// This is the fixture that gives
    /// [`an_admitted_closure_names_no_global_or_table`] something to find. The
    /// other three contain no global or table operator anywhere, so scanning
    /// their admitted bodies could never fail however wrong the closure walk
    /// was; here the operator exists and correctness depends on it being left
    /// out of `local_func_indices`.
    const GLOBAL_READER_OUTSIDE_THE_CLOSURE: &str = r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (type (;1;) (func (result i32)))
          (global (;0;) (mut i32) (i32.const 7))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (func (;1;) (type 1) (result i32)
            global.get 0)
          (export "sum" (func 0))
          (export "read_state" (func 1)))
        "#;

    /// Both pieces of lld boilerplate over a function that stores through its
    /// caller-supplied pointer — the realistic Tier-B shape.
    const UNUSED_BOILERPLATE_OVER_CALLER_MEMORY: &str = r#"
        (module
          (type (;0;) (func (param i32 i32)))
          (memory (;0;) 1)
          (global $__stack_pointer (mut i32) (i32.const 1048576))
          (table (;0;) 1 1 funcref)
          (func (;0;) (type 0) (param i32 i32)
            local.get 0
            local.get 1
            i32.store)
          (export "store_at" (func 0)))
        "#;

    #[test]
    fn an_unused_global_declaration_is_tier_a() {
        assert_eq!(
            classify_root(UNUSED_STACK_POINTER, "sum").expect("declared-but-unused global merges"),
            Tier::A,
            "a global no body reads is inert: no memory either, so Tier A"
        );
    }

    #[test]
    fn an_unused_table_declaration_is_tier_a() {
        assert_eq!(
            classify_root(UNUSED_TABLE, "sum").expect("declared-but-unused table merges"),
            Tier::A,
            "an empty table no body names is inert"
        );
    }

    #[test]
    fn unused_boilerplate_over_caller_memory_is_tier_b() {
        assert_eq!(
            classify_root(UNUSED_BOILERPLATE_OVER_CALLER_MEMORY, "store_at")
                .expect("lld boilerplate must not block a Tier-B merge"),
            Tier::B,
            "the store through a caller pointer decides the tier, not the unread boilerplate"
        );
    }

    #[test]
    fn reading_a_global_is_tier_c() {
        let reasons = reasons_for(
            r#"
            (module
              (type (;0;) (func (result i32)))
              (global (;0;) (mut i32) (i32.const 7))
              (func (;0;) (type 0) (result i32)
                global.get 0)
              (export "counter" (func 0)))
            "#,
            "counter",
        );
        // Pinned verbatim: the reason names only the *access*, since defining a
        // global is no longer a rejection signal. A message that still said
        // "defines" would tell an author their declaration is fatal when it is
        // not.
        assert_eq!(
            reasons,
            vec!["reads or writes module globals".to_string()],
            "global.get must still be Tier C, and the reason must name only the access"
        );
    }

    #[test]
    fn writing_a_global_is_tier_c() {
        let reasons = reasons_for(
            r#"
            (module
              (type (;0;) (func (param i32)))
              (global (;0;) (mut i32) (i32.const 0))
              (func (;0;) (type 0) (param i32)
                local.get 0
                global.set 0)
              (export "set_counter" (func 0)))
            "#,
            "set_counter",
        );
        assert!(
            reasons.iter().any(|r| r.contains("global")),
            "global.set must still be Tier C: {reasons:?}"
        );
    }

    #[test]
    fn an_element_segment_is_tier_c_even_when_unused() {
        // A bare table links; an element segment does not. The rejection is
        // conservatism rather than a correctness requirement — the merged output
        // declares no table, so a dropped element segment initializes nothing
        // observable — but an element segment marks a module built around
        // indirect dispatch, and admitting one would silently discard a
        // construct the author wrote.
        //
        // The reason names the element segment specifically. It must NOT claim
        // the closure touched the table space: this body never names the table.
        let reasons = reasons_for(
            r#"
            (module
              (type (;0;) (func (param i32 i32) (result i32)))
              (table (;0;) 1 1 funcref)
              (elem (;0;) (i32.const 0) func 0)
              (func (;0;) (type 0) (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.add)
              (export "sum" (func 0)))
            "#,
            "sum",
        );
        assert_eq!(
            reasons,
            vec!["declares an element segment".to_string()],
            "an element segment must be Tier C under its own reason, with no \
             table-use reason claimed"
        );
    }

    #[test]
    fn call_indirect_is_tier_c() {
        let reasons = reasons_for(
            r#"
            (module
              (type (;0;) (func (result i32)))
              (table (;0;) 1 funcref)
              (func (;0;) (type 0) (result i32)
                i32.const 0
                call_indirect (type 0))
              (export "run" (func 0)))
            "#,
            "run",
        );
        // The converse of the element-segment case: the closure names the table
        // space but the module declares no element segment, so exactly the
        // table-use reason fires. The two signals are independent and each names
        // only what is actually there.
        assert_eq!(
            reasons,
            vec!["performs an indirect call or otherwise names the table space".to_string()],
            "call_indirect must be Tier C under the table-use reason alone"
        );
    }

    #[test]
    fn an_unused_data_segment_is_still_tier_c() {
        // The deliberate asymmetry with globals and tables: an *active* data
        // segment writes memory at instantiation whether or not any instruction
        // names it, so dropping it would change what the merged program
        // observes. Pinned here so a later symmetry cleanup cannot quietly
        // relax it.
        let reasons = reasons_for(
            r#"
            (module
              (type (;0;) (func (param i32 i32) (result i32)))
              (memory (;0;) 1)
              (data (;0;) (i32.const 0) "\2a\00\00\00")
              (func (;0;) (type 0) (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.add)
              (export "sum" (func 0)))
            "#,
            "sum",
        );
        assert!(
            reasons.iter().any(|r| r.contains("data")),
            "a declared data segment must stay Tier C: {reasons:?}"
        );
    }

    #[test]
    fn data_drop_is_tier_c() {
        let reasons = reasons_for(
            r#"
            (module
              (type (;0;) (func))
              (memory (;0;) 1)
              (data (;0;) "\2a")
              (func (;0;) (type 0)
                data.drop 0)
              (export "run" (func 0)))
            "#,
            "run",
        );
        assert!(
            reasons.iter().any(|r| r.contains("data")),
            "data.drop must still be Tier C: {reasons:?}"
        );
    }

    #[test]
    fn an_unused_global_alongside_a_data_segment_yields_only_the_data_reason() {
        // The sharpest single statement of this change. This module used to
        // produce *two* Tier-C reasons — one for the data segment, one for the
        // declared global — and now produces one, because the global is never
        // read. The link still fails, on the data segment alone; what changed is
        // that the diagnostic no longer accuses the author of a second problem
        // they do not have.
        let reasons = reasons_for(
            r#"
            (module
              (type (;0;) (func (param i32 i32) (result i32)))
              (memory (;0;) 1)
              (data (;0;) (i32.const 0) "\2a\00\00\00")
              (global $__stack_pointer (mut i32) (i32.const 1048576))
              (table (;0;) 1 1 funcref)
              (func (;0;) (type 0) (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.add)
              (export "sum" (func 0)))
            "#,
            "sum",
        );
        assert_eq!(
            reasons,
            vec!["defines or initializes its own static data segments".to_string()],
            "the unread global and unnamed table must contribute no reason"
        );
    }

    #[test]
    fn every_signal_at_once_accumulates_all_four_reasons_in_order() {
        // `reasons` is a `Vec`, and the error renders it as a `; `-joined list,
        // so both the accumulation and the order are user-visible. Nothing else
        // pins either: every other test drives one signal at a time.
        let reasons = reasons_for(
            r#"
            (module
              (type (;0;) (func (result i32)))
              (memory (;0;) 1)
              (data (;0;) (i32.const 0) "\2a")
              (table (;0;) 1 1 funcref)
              (elem (;0;) (i32.const 0) func 0)
              (global (;0;) (mut i32) (i32.const 0))
              (func (;0;) (type 0) (result i32)
                data.drop 0
                global.get 0
                call_indirect (type 0))
              (export "everything" (func 0)))
            "#,
            "everything",
        );
        assert_eq!(
            reasons,
            vec![
                "defines or initializes its own static data segments".to_string(),
                "reads or writes module globals".to_string(),
                "performs an indirect call or otherwise names the table space".to_string(),
                "declares an element segment".to_string(),
            ],
            "all four signals must accumulate, in declaration order"
        );
    }

    #[test]
    fn an_indirect_call_through_an_initialized_table_reports_both_table_reasons() {
        // The case the four-way split exists for: a `call_indirect` dispatching
        // through a table an element segment also initializes. Before the split
        // these two independent facts shared one string and collapsed into a
        // single reason; now each is named, so the author sees both the use and
        // the declaration they would have to remove.
        let reasons = reasons_for(
            r#"
            (module
              (type (;0;) (func (result i32)))
              (table (;0;) 1 1 funcref)
              (elem (;0;) (i32.const 0) func 0)
              (func (;0;) (type 0) (result i32)
                i32.const 0
                call_indirect (type 0))
              (export "dispatch" (func 0)))
            "#,
            "dispatch",
        );
        assert_eq!(
            reasons,
            vec![
                "performs an indirect call or otherwise names the table space".to_string(),
                "declares an element segment".to_string(),
            ],
            "table use and the element declaration are independent and both must be named"
        );
    }

    /// Every operator that names the global or table index space.
    ///
    /// Enumerated here rather than derived from [`crate::safety::check_operator`]
    /// so the scan below is independent of the very effect flags that admitted
    /// the closure — a scan driven by those flags could not detect a closure walk
    /// that disagreed with them.
    ///
    /// Only three of these are reachable through the public `link` API today:
    /// `global.get`, `global.set` and `call_indirect`. The five `table.*`
    /// accessors and `ref.func` are reference-types instructions that
    /// [`crate::SUPPORTED_WASM_FEATURES`] excludes, so an external carrying one
    /// is refused as `UnsupportedWasmFeature` before classification (pinned by
    /// `reference_typed_table_operators_are_refused_by_the_feature_gate` in
    /// `tests/link.rs`). They are listed anyway: this scan is defense in depth
    /// against a future feature-gate widening, and the segment-indexed forms
    /// (`table.init`/`table.copy`/`elem.drop`) plus `return_call_indirect` are
    /// absent only because the allow-list rejects them outright.
    fn names_a_global_or_table(op: &Operator) -> bool {
        matches!(
            op,
            Operator::GlobalGet { .. }
                | Operator::GlobalSet { .. }
                | Operator::CallIndirect { .. }
                | Operator::TableGet { .. }
                | Operator::TableSet { .. }
                | Operator::TableGrow { .. }
                | Operator::TableSize { .. }
                | Operator::TableFill { .. }
                | Operator::RefFunc { .. }
        )
    }

    /// Whether any body anywhere in `module` — inside the closure or not — names
    /// a global or table.
    fn module_has_a_naming_operator(module: &ParsedModule) -> bool {
        module.local_funcs.iter().any(|f| {
            let reader = FunctionBody::new(BinaryReader::new(&f.body, 0));
            reader
                .get_operators_reader()
                .expect("operators")
                .into_iter()
                .any(|op| names_a_global_or_table(&op.expect("operator")))
        })
    }

    #[test]
    fn an_admitted_closure_names_no_global_or_table() {
        // The agreement between the closure walk and the effect flags, which is
        // what makes dropping an admitted external's globals and tables safe.
        //
        // `classify` admits on `effects`, and the merge then copies exactly the
        // bodies in `local_func_indices`. If those two ever disagreed — a body
        // reaching the merged output without having contributed its effects — a
        // `global.get` could be copied into the output, where it would rebind to
        // main's global and still pass validation. This test reads the admitted
        // bodies back and confirms the disagreement does not happen.
        //
        // `GLOBAL_READER_OUTSIDE_THE_CLOSURE` is what gives it teeth: its module
        // *does* contain a `global.get`, just not in `sum`'s closure. Scanning
        // the other fixtures could never fail, since they contain no naming
        // operator for any closure walk to wrongly include. The guard below
        // enforces that at least one fixture is of the first kind, so the set
        // cannot quietly regress to a tautology.
        let mut saw_a_fixture_with_a_naming_operator = false;

        for (wat, root_export) in [
            (UNUSED_STACK_POINTER, "sum"),
            (UNUSED_TABLE, "sum"),
            (UNUSED_BOILERPLATE_OVER_CALLER_MEMORY, "store_at"),
            (GLOBAL_READER_OUTSIDE_THE_CLOSURE, "sum"),
        ] {
            let module = parse(wat);
            assert!(
                !module.globals.is_empty() || !module.tables.is_empty(),
                "fixture must declare the construct whose omission is being justified"
            );
            saw_a_fixture_with_a_naming_operator |= module_has_a_naming_operator(&module);

            let root = module.exported_func_index(root_export).unwrap();
            let cl = closure::compute(&module, root).expect("closure computes");
            classify(&module, &cl, root, root_export).expect("fixture must be admitted");

            let base = module.local_func_base();
            for &idx in &cl.local_func_indices {
                let body = &module.local_funcs[(idx - base) as usize].body;
                let reader = FunctionBody::new(BinaryReader::new(body, 0));
                for op in reader.get_operators_reader().expect("operators") {
                    let op = op.expect("operator");
                    assert!(
                        !names_a_global_or_table(&op),
                        "an admitted closure must contain no global/table operator, \
                         found {op:?} in function {idx} of `{root_export}`"
                    );
                }
            }
        }

        assert!(
            saw_a_fixture_with_a_naming_operator,
            "at least one fixture must contain a global/table operator OUTSIDE the \
             admitted closure — without one this test is a tautology, since a scan \
             over bodies that contain no such operator cannot fail"
        );
    }
}
