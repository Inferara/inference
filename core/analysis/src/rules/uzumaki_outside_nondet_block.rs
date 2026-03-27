//! A006: Uzumaki (@) must appear inside a non-deterministic block.

use inference_ast::nodes::Expr;

use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// Uzumaki (@) must appear inside a non-deterministic block.
    #[id = "A006"]
    #[name = "Uzumaki outside nondet block"]
    #[severity = error]
    pub struct UzumakiOutsideNonDetBlock;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            if walk_ctx.nondet_depth > 0 {
                return;
            }
            walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
                walker::walk_expr(arena, expr_id, &mut |sub_id| {
                    if matches!(arena[sub_id].kind, Expr::Uzumaki) {
                        errors.push(AnalysisDiagnostic::UzumakiOutsideNonDetBlock {
                            location: arena[sub_id].location,
                        });
                    }
                });
            });
        });
        errors
    }
}
