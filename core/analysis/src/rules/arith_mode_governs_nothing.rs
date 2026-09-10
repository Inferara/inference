//! A053: an arithmetic-mode annotation must have arithmetic to govern.
//!
//! `checked(e)` and `wrapping(e)` say how the `+`, `-`, `*` and unary `-`
//! written *between their own parentheses* treat a result that leaves the
//! operand type. An annotation with none of those inside it therefore does
//! nothing at all, and the two spellings fail differently enough to be worth
//! separate wordings:
//!
//! - `wrapping(...)` is a no-op the author believes is an opt-out. Nothing
//!   changes, and nothing warns them.
//! - `checked(...)` is worse, because it reads as a guarantee. A later reader
//!   takes the expression to be checked when no guard was emitted anywhere in
//!   it.
//!
//! Two shapes account for nearly all of these, and the message names both
//! because each is invisible from the site:
//!
//! - **A call.** `wrapping(mix(s))` annotates the *call*, and the annotation
//!   does not reach into `mix`'s body. Under a default that traps this is the
//!   mistake authors of hashes, mixers and pseudo-random generators will make
//!   most often, and the rule is only useful if it says why.
//! - **A glued negative literal.** `-2147483648` is a single token carrying its
//!   own sign, not a negation applied to a value, so `wrapping(-2147483648)`
//!   governs nothing. The detached spelling `- 2147483648` is a negation, and
//!   A046 rejects that spelling for its own reasons.
//!
//! An error rather than a warning: this is a typo, not a redundancy. The
//! annotation is somewhere other than where the author meant to put it, and no
//! change to the language's default makes it right.
//!
//! ## Where the line with A054 falls
//!
//! Containment, not government. An annotation whose subtree holds an operator
//! that a *nested* annotation governs — `wrapping(checked(a + b))` — is not
//! reported here: the author did write arithmetic inside it. That annotation is
//! [`super::arith_mode_changes_nothing`]'s business, which is the rule about an
//! annotation that reaches operators and leaves them as they were.

use inference_ast::nodes::Expr;

use crate::errors::{AnalysisDiagnostic, LabeledDiagnostic};
use crate::walker::{contains_governed_operator, for_each_stmt_expr, walk_expr};

crate::rule! {
    /// An arithmetic-mode annotation must contain an operator it can govern.
    #[id = "A053"]
    #[name = "Arithmetic-mode annotation with nothing to govern"]
    #[severity = error]
    pub struct ArithModeGovernsNothing;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let arena = ctx.arena();
        let mut errors = Vec::new();
        crate::walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
                walk_expr(arena, expr_id, &mut |node| {
                    let Expr::ArithMode { mode, .. } = &arena[node].kind else {
                        return;
                    };
                    if contains_governed_operator(ctx, node) {
                        return;
                    }
                    errors.push(LabeledDiagnostic::new(
                        walk_ctx.module_path.clone(),
                        AnalysisDiagnostic::ArithModeGovernsNothing {
                            mode: *mode,
                            location: arena[node].location,
                        },
                    ));
                });
            });
        });
        errors
    }
}
