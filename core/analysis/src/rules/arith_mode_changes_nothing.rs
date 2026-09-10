//! A054: an arithmetic-mode annotation that leaves its operators as they were.
//!
//! An annotation is redundant when the mode it names is already the mode in
//! force where it is written — from the language's own default at the top level,
//! or from an enclosing annotation of the same mode. Every operator it reaches
//! then behaves identically with it and without it, so the annotation is text
//! that says nothing.
//!
//! A **warning**, not an error, and the reason is the default. Which spelling is
//! redundant is decided by the mode unannotated arithmetic has, so an annotation
//! that is redundant under one default is meaningful under the other. A rule
//! that turned source the previous release taught people to write into a hard
//! error, with no deprecation window, would be a worse answer than the
//! redundancy it reports. This is the position rustc takes on
//! `unused_attributes`, and languages whose opt-out is an operator (`+%`, `&+`)
//! have no such rule at all, because such a spelling cannot be redundant.
//!
//! The initializer of a body-level `const` is walked like any other expression,
//! so an annotation written there is held to exactly this rule; top-level
//! `const` declarations do not exist in the language yet (A032 rejects them),
//! and when they do their initializers have to join this walk.
//!
//! ## What counts as changing something
//!
//! The comparison is against the *enclosing* effective mode, never against the
//! default alone. `checked(a * wrapping(b + c))` is redundant nowhere under a
//! wrapping default: the outer annotation changes the `*`, and the inner one
//! restores wrapping inside a region the outer had made checked.
//!
//! ## One finding per stack
//!
//! A stack of same-mode annotations reports its outermost member and stops, at
//! any depth. `checked(checked(checked(a + b)))` under a checked default is one
//! finding, on the outermost: removing that one is what the author has to do
//! first, and the next compile reports the next one down if it is still
//! redundant. Suppression is therefore transitive — a layer that is itself
//! suppressed suppresses what it encloses, or a three-deep stack would report
//! its first and third members and read as two unrelated findings. This is the
//! convention A048 and A049 already follow for nested array annotations —
//! report the carrier once, not once per layer.
//!
//! ## Where the line with A053 falls
//!
//! An annotation with no governed operator anywhere inside it is
//! [`super::arith_mode_governs_nothing`]'s finding, and this rule steps aside
//! for it: both would fire on `wrapping(f(x))`, and "it governs nothing" is the
//! fact the author needs rather than "it changes nothing". Both rules read one
//! containment predicate, so the handoff cannot drift.

use inference_ast::ids::ExprId;
use inference_ast::nodes::Expr;
use rustc_hash::FxHashSet;

use crate::errors::{AnalysisDiagnostic, LabeledDiagnostic, RedundantArithMode};
use crate::walker::{
    arith_mode_under, contains_governed_operator, for_each_stmt_expr, walk_expr_with_arith_mode,
};

crate::rule! {
    /// An arithmetic-mode annotation that names the mode already in force.
    #[id = "A054"]
    #[name = "Arithmetic-mode annotation that changes nothing"]
    #[severity = warning]
    pub struct ArithModeChangesNothing;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let arena = ctx.arena();
        let mut findings = Vec::new();
        // Every redundant annotation this pass has already accounted for,
        // whether by reporting it or by suppressing it under one it had
        // reported. A layer enters the set either way, so an inner layer is
        // suppressed by the stack it sits in rather than only by a layer that
        // produced a finding.
        let mut accounted: FxHashSet<ExprId> = FxHashSet::default();
        crate::walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
                walk_expr_with_arith_mode(arena, expr_id, None, &mut |node, enclosing| {
                    let Expr::ArithMode { mode, .. } = &arena[node].kind else {
                        return;
                    };
                    if *mode != arith_mode_under(arena, enclosing) {
                        return;
                    }
                    if !contains_governed_operator(ctx, node) {
                        return;
                    }
                    let suppressed = enclosing.is_some_and(|outer| accounted.contains(&outer));
                    accounted.insert(node);
                    if suppressed {
                        return;
                    }
                    let enclosure = if enclosing.is_some() {
                        RedundantArithMode::InsideTheSameAnnotation
                    } else {
                        RedundantArithMode::AgainstTheDefault
                    };
                    findings.push(LabeledDiagnostic::new(
                        walk_ctx.module_path.clone(),
                        AnalysisDiagnostic::ArithModeChangesNothing {
                            mode: *mode,
                            enclosure,
                            location: arena[node].location,
                        },
                    ));
                });
            });
        });
        findings
    }
}
