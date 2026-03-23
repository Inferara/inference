//! A006: Uzumaki (@) must appear inside a non-deterministic block.

use inference_ast::nodes::{Expr, Stmt};

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
            let uzumaki_location = match &arena[stmt_id].kind {
                Stmt::VarDef { value: Some(expr_id), .. } if matches!(arena[*expr_id].kind, Expr::Uzumaki) => {
                    Some(arena[*expr_id].location)
                }
                Stmt::Assign { right, .. } if matches!(arena[*right].kind, Expr::Uzumaki) => {
                    Some(arena[*right].location)
                }
                Stmt::Return { expr } if matches!(arena[*expr].kind, Expr::Uzumaki) => {
                    Some(arena[*expr].location)
                }
                _ => None,
            };
            if let Some(location) = uzumaki_location {
                errors.push(AnalysisDiagnostic::UzumakiOutsideNonDetBlock { location });
            }
        });
        errors
    }
}
