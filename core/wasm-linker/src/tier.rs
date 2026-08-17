//! Memory-merge feasibility tiers.
//!
//! Whether an external function can be merged into the single shared linear
//! memory depends on what its transitive closure touches:
//!
//! - **Tier A** — no linear-memory access: the closure computes, and may read or
//!   write its own module's globals, but never loads or stores. Merge is a copy
//!   + re-index of functions, types and globals.
//! - **Tier B** — memory via caller-passed pointers only: the closure
//!   loads/stores through addresses the caller supplies, but defines no static
//!   data of its own and names no table or element entry. The one shared memory
//!   is enough; no address relocation is needed. Admission to
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
//! - **Tier C** — own static data or table/element use: merging would require
//!   relocating data and rewriting absolute addresses, which needs relocation
//!   metadata the static merge does not consume. Rejected with a clear error.
//!
//! Globals cut across the tiers rather than deciding one: a closure may read or
//! write them at either tier, and [`crate::merge`] carries the whole global
//! section of every external that touches one into the output, re-indexed above
//! the main module's. What that is and is not sound for is the subject of the
//! next section.
//!
//! Tiers A and B turn on what the closure *uses*, not on what its module
//! *declares*. A global no body reads or writes, and a table with no element
//! segment that no instruction names, are inert and are dropped rather than
//! merged — real toolchain output is full of such boilerplate (lld emits a
//! `__stack_pointer` global into every `wasm32-unknown-unknown` artifact, and an
//! empty `(table 1 1 funcref)` into every `std` one). Data segments are the
//! deliberate exception and stay declaration-gated; [`tier_c_reasons`] explains
//! why.
//!
//! # What merging a global is sound for
//!
//! **A globals lift is sound only for globals used as pure scalar state.** The
//! merge re-indexes a global correctly and relocates its *value* not at all, and
//! in real toolchain output that value is frequently an address. `__stack_pointer`
//! and `__heap_base` hold positions in the memory layout the external was
//! compiled for; merged onto a shared memory laid out for the host program, the
//! index is right and the address means something else entirely.
//!
//! Two things keep the admitted set narrower than that sounds:
//!
//! - When the closure touches memory it is Tier B, so [`crate::provenance`] runs
//!   and treats every value read from a global as `NotParam`. An address computed
//!   through a global therefore fails the derivation proof and the closure is
//!   rejected — which is exactly the shadow-stack idiom (`global.get $sp`,
//!   subtract a frame, store through it) that a real lld artifact uses. Those
//!   externals remain Tier C.
//! - The main module cannot name an external's globals at all. Main's own
//!   indices are unchanged by the merge, externals' are appended above them, and
//!   a main module that *imports* a global is rejected outright, so no merged
//!   global is reachable from a body other than its own module's.
//!
//! What is left admitted is a closure that reads and writes globals as scalars
//! while addressing memory only through its parameters — a counter, a mode flag,
//! a seed. That much is genuinely per-module state and merges correctly.
//!
//! The protection above is conditional, and worth stating as such: provenance
//! runs only when `effects.uses_memory`. A closure that touches no memory is
//! Tier A and is never analyzed, so nothing constrains what the values it
//! computes from its globals *mean*. An external that returns `global.get
//! $__stack_pointer` to a caller which then dereferences it is admitted, and the
//! address it hands over is wrong.
//!
//! That escape is not opened by merging globals — it is the general fact that
//! nothing analyzes a scalar an external returns. The same external written as
//! `i32.const 1048576` is Tier A and admitted today, with no global anywhere.
//! What merging globals changes is how often the shape occurs: a stack-pointer
//! global is standard in the artifacts this feature exists to accept, whereas a
//! bare address constant is not. Closing it needs the callee-side counterpart of
//! the pointee-size channel issue #420 tracks — a claim about what a returned
//! integer *means* — and neither exists today.
//!
//! # Merging globals excludes placing external data segments
//!
//! This capability and a future one are mutually exclusive as things stand, and
//! the conflict is not visible from either side alone.
//!
//! Admitting an external's data segments would mean placing them at their
//! original addresses and proving safety by showing each module's claimed region
//! is disjoint from every other's. Such a proof can only range over regions the
//! linker can see: declared data segments and memory limits. An external's
//! shadow-stack region is described by no section this crate parses — it is
//! implied by the initializer of a mutable global, and the sign convention that
//! a stack grows down from it. Merging that global carries the claim into the
//! output where the disjointness argument cannot see it, so the argument would
//! be unsound in the presence of a merged global while appearing complete.
//!
//! The claim is not dormant: an external can hand a global-derived address to
//! its caller (above), and any later relaxation admitting global-derived
//! addressing would make it live inside the external too.
//!
//! Adding data-segment placement therefore means revisiting this, not building
//! on top of it — either by teaching the linker to read a region claim out of a
//! global's initializer, or by keeping the two mutually exclusive on purpose.

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
/// constructs: its own data or element segments, or a closure that names the
/// table space (an indirect call or a `table.*`/`ref.func` operator). These imply
/// absolute addresses or dispatch tables that a position-naive static merge
/// cannot reconcile across two modules sharing one memory. Neither a
/// declared-but-untouched table nor a global — touched or not — is among them;
/// see [`tier_c_reasons`] for the table, and the module documentation for what
/// merging a global is and is not sound for.
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
/// Table *use* is gated on use; data and element segments on declaration. Each
/// of these choices rests on a different argument, and they are not
/// interchangeable:
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
/// Dropping the external's inert globals and its tables from the merged output
/// is safe precisely because [`ClosureEffects`] is *closure-scoped*: it is
/// accumulated over the bodies reachable from the root export, so a closure
/// admitted with `uses_globals == false` and `uses_tables == false` contains no
/// operator naming either index space.
///
/// Neither half of that now fails silently if it were ever violated, and the two
/// are fail-safe for different reasons. A closure whose globals were dropped has
/// an **empty** global remap in [`crate::merge`], so a leaked `global.get` finds
/// no mapping and surfaces a clean [`LinkError`] — where before the global space
/// was rebuilt, the same operator would have rebound onto *main's* first global
/// and, two `i32` globals agreeing in type, still passed post-merge validation
/// with a wrong value and no diagnostic. For **tables** the merge emits no table
/// section at all, so a leaked table operator names a table the output does not
/// have and post-merge validation rejects it as unknown.
fn tier_c_reasons(module: &ParsedModule, effects: &ClosureEffects) -> Vec<String> {
    let mut reasons = Vec::new();

    if module.data_count > 0 || effects.uses_data_segments {
        reasons.push("defines or initializes its own static data segments".to_string());
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
    //! between a *declared* table and a *used* one, and the removal of globals
    //! from the gate entirely.
    //!
    //! Every `cargo build --target wasm32-unknown-unknown` artifact carries an
    //! lld-synthesized `__stack_pointer` mutable global, and a `std` build also
    //! carries an empty `(table 1 1 funcref)`. A leaf integer function never
    //! reads either. These tests pin that such a declaration alone does not
    //! force Tier C, that an operator naming the *table* space still does while
    //! one naming a global no longer does, and that a *data* segment stays
    //! declaration-gated.

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
    fn reading_a_global_is_tier_a() {
        // A closure that reads a global touches no memory, so it is Tier A: the
        // merge copies its bodies and its module's globals and re-indexes both.
        // No reason may be produced at all — a stale globals reason would reject
        // the very artifact shape this admission exists for.
        assert_eq!(
            classify_root(
                r#"
                (module
                  (type (;0;) (func (result i32)))
                  (global (;0;) (mut i32) (i32.const 7))
                  (func (;0;) (type 0) (result i32)
                    global.get 0)
                  (export "counter" (func 0)))
                "#,
                "counter",
            )
            .expect("a global read must not reject the link"),
            Tier::A,
            "reading a global is orthogonal to the memory tier"
        );
    }

    #[test]
    fn writing_a_global_is_tier_a() {
        // The write side. `global.set` is the half that carries per-module state,
        // and it is admitted on the strength of the merge giving this module's
        // globals cells of their own rather than aliasing main's.
        assert_eq!(
            classify_root(
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
            )
            .expect("a global write must not reject the link"),
            Tier::A,
        );
    }

    #[test]
    fn a_global_used_to_address_memory_is_still_tier_c() {
        // The soundness boundary of the globals admission, at the classifier
        // rather than through the public API. This closure stores through
        // `global.get $__stack_pointer` — the shadow-stack idiom, and the case
        // where a merged global's *value* is a claim about a memory layout the
        // merged output does not have. Provenance tags a global read `NotParam`,
        // so the derivation proof fails and the whole closure is rejected.
        //
        // The rejection must come from provenance, not from a surviving globals
        // reason: a `RequiresRelocatableBuild` naming globals here would mean the
        // gate was never actually relaxed, and every admission test above would
        // be measuring the wrong thing.
        let err = classify_root(
            r#"
            (module
              (type (;0;) (func (param i32)))
              (memory (;0;) 17)
              (global $__stack_pointer (mut i32) (i32.const 1048576))
              (func (;0;) (type 0) (param i32)
                global.get 0
                local.get 0
                i32.store)
              (export "store_at_stack" (func 0)))
            "#,
            "store_at_stack",
        )
        .expect_err("a global-derived address must be rejected");
        let LinkError::RequiresRelocatableBuild { reasons, .. } = &err else {
            panic!("expected RequiresRelocatableBuild, got {err:?}");
        };
        assert!(
            !reasons.iter().any(|r| r.contains("global")),
            "the rejection must come from address provenance, not a globals reason: {reasons:?}"
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
    fn every_signal_at_once_accumulates_all_three_reasons_in_order() {
        // `reasons` is a `Vec`, and the error renders it as a `; `-joined list,
        // so both the accumulation and the order are user-visible. Nothing else
        // pins either: every other test drives one signal at a time.
        //
        // The body reads a global as well, and that must contribute *no* reason.
        // Asserting the exact vector is what makes this the sharpest statement
        // that globals left the gate: a re-added globals reason lands in the
        // middle of this list and fails here even if every admission test were
        // deleted.
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
                "performs an indirect call or otherwise names the table space".to_string(),
                "declares an element segment".to_string(),
            ],
            "all three remaining signals must accumulate in declaration order, and the \
             global read must contribute none"
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

    /// Whether `op` names the global index space.
    ///
    /// Enumerated here rather than derived from [`crate::safety::check_operator`]
    /// so the scan below is independent of the very effect flags that admitted
    /// the closure — a scan driven by those flags could not detect a closure walk
    /// that disagreed with them.
    fn names_a_global(op: &Operator) -> bool {
        matches!(op, Operator::GlobalGet { .. } | Operator::GlobalSet { .. })
    }

    /// Whether `op` names the table index space.
    ///
    /// Only `call_indirect` is reachable through the public `link` API today.
    /// The five `table.*` accessors and `ref.func` are reference-types
    /// instructions that [`crate::SUPPORTED_WASM_FEATURES`] excludes, so an
    /// external carrying one is refused as `UnsupportedWasmFeature` before
    /// classification (pinned by
    /// `reference_typed_table_operators_are_refused_by_the_feature_gate` in
    /// `tests/link.rs`). They are listed anyway: this scan is defense in depth
    /// against a future feature-gate widening, and the segment-indexed forms
    /// (`table.init`/`table.copy`/`elem.drop`) plus `return_call_indirect` are
    /// absent only because the allow-list rejects them outright.
    fn names_a_table(op: &Operator) -> bool {
        matches!(
            op,
            Operator::CallIndirect { .. }
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
                .any(|op| {
                    let op = op.expect("operator");
                    names_a_global(&op) || names_a_table(&op)
                })
        })
    }

    /// A module whose exported root *reads* a global, alongside the memory-using
    /// caller-pointer store that makes it Tier B — so the scan below has a fixture
    /// whose `uses_globals` flag is actually set.
    const GLOBAL_COUNTER_INSIDE_THE_CLOSURE: &str = r#"
        (module
          (type (;0;) (func (param i32 i32)))
          (memory (;0;) 1)
          (global (;0;) (mut i32) (i32.const 0))
          (func (;0;) (type 0) (param i32 i32)
            local.get 0
            local.get 1
            i32.store
            global.get 0
            i32.const 1
            i32.add
            global.set 0)
          (export "store_at" (func 0)))
        "#;

    #[test]
    fn an_admitted_closures_operators_agree_with_its_effect_flags() {
        // The agreement between the closure walk and the effect flags. Both
        // halves of the merge's handling of an admitted external rest on it:
        // whether its global section is carried into the output, and whether its
        // tables are dropped.
        //
        // `classify` admits on `effects`, and the merge then copies exactly the
        // bodies in `local_func_indices`. If those two ever disagreed — a body
        // reaching the merged output without having contributed its effects — a
        // closure whose globals were dropped as untouched could still carry a
        // `global.get`, and a closure could carry a table operator into an output
        // with no table section. This test reads the admitted bodies back and
        // confirms the disagreement does not happen.
        //
        // The assertions are conditional on the flags rather than absolute,
        // because a global operator in an admitted closure is now legitimate:
        // what must hold is that the flag *predicts* it.
        //
        // `GLOBAL_READER_OUTSIDE_THE_CLOSURE` is what gives the negative
        // direction teeth: its module *does* contain a `global.get`, just not in
        // `sum`'s closure, so a closure walk that over-approximated would be
        // caught. The other fixtures contain no naming operator for any walk to
        // wrongly include. The guard below enforces that at least one fixture is
        // of the first kind, so the set cannot quietly regress to a tautology.
        let mut saw_a_fixture_with_a_naming_operator = false;
        let mut saw_a_globals_using_closure = false;

        for (wat, root_export) in [
            (UNUSED_STACK_POINTER, "sum"),
            (UNUSED_TABLE, "sum"),
            (UNUSED_BOILERPLATE_OVER_CALLER_MEMORY, "store_at"),
            (GLOBAL_READER_OUTSIDE_THE_CLOSURE, "sum"),
            (GLOBAL_COUNTER_INSIDE_THE_CLOSURE, "store_at"),
        ] {
            let module = parse(wat);
            assert!(
                !module.globals.is_empty() || !module.tables.is_empty(),
                "fixture must declare the construct whose handling is being justified"
            );
            saw_a_fixture_with_a_naming_operator |= module_has_a_naming_operator(&module);

            let root = module.exported_func_index(root_export).unwrap();
            let cl = closure::compute(&module, root).expect("closure computes");
            classify(&module, &cl, root, root_export).expect("fixture must be admitted");
            saw_a_globals_using_closure |= cl.effects.uses_globals;

            let base = module.local_func_base();
            for &idx in &cl.local_func_indices {
                let body = &module.local_funcs[(idx - base) as usize].body;
                let reader = FunctionBody::new(BinaryReader::new(body, 0));
                for op in reader.get_operators_reader().expect("operators") {
                    let op = op.expect("operator");
                    assert!(
                        cl.effects.uses_globals || !names_a_global(&op),
                        "a closure admitted with uses_globals clear must contain no \
                         global operator, found {op:?} in function {idx} of `{root_export}`"
                    );
                    assert!(
                        cl.effects.uses_tables || !names_a_table(&op),
                        "a closure admitted with uses_tables clear must contain no \
                         table operator, found {op:?} in function {idx} of `{root_export}`"
                    );
                }
            }
        }

        assert!(
            saw_a_fixture_with_a_naming_operator,
            "at least one fixture must contain a global/table operator OUTSIDE the \
             admitted closure — without one the negative direction is a tautology, \
             since a scan over bodies that contain no such operator cannot fail"
        );
        assert!(
            saw_a_globals_using_closure,
            "at least one fixture must be admitted with uses_globals SET — without one \
             the conditional assertions never take their permissive branch, and the \
             test would silently be the old absolute scan"
        );
    }
}
