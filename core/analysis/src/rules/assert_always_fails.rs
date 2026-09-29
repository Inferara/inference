//! A057: an `assert` must not fail on every run that reaches it.
//!
//! An `assert` whose condition cannot hold is not a check: it is an
//! unconditional halt written as one. Placed where a guard's failing branch
//! should recover — `if i < 8 { a[i] } else { assert(false); }` — it satisfies
//! A056's letter while giving the out-of-range index exactly the outcome that
//! rule exists to rule out. This rule closes that path: a failure branch must
//! do something, not stop.
//!
//! The condition is judged by the range analysis in `src/range.rs`: an
//! `assert` is reported when no state the analysis allows at that point
//! satisfies it — `assert(false)`, or `assert(i < 8)` inside the branch where
//! `i >= 8`. The analysis over-approximates the states a run can be in, so a
//! reported `assert` fails on every run that reaches it. An `assert` the
//! analysis cannot decide is not reported, and neither is one no run reaches.
//!
//! Only executable code is examined. In a specification an `assert` is part of
//! what the specification states, and an `assume` block's `assert` filters the
//! choices it admits by design.

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    range, walker,
};

crate::rule! {
    /// An `assert` must not fail on every run that reaches it.
    #[id = "A057"]
    #[name = "Assert always fails"]
    #[severity = error]
    pub struct AssertAlwaysFails;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            let module_path = &source_file.module_path;
            walker::for_each_executable_function_body(arena, &source_file.defs, &mut |body_id| {
                for stmt_id in range::analyze_body(ctx, body_id).failing_asserts {
                    errors.push(LabeledDiagnostic::new(
                        module_path.clone(),
                        AnalysisDiagnostic::AssertAlwaysFails {
                            location: arena[stmt_id].location,
                        },
                    ));
                }
            });
        }
        errors
    }
}
