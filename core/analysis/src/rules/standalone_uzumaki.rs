//! A008: Standalone uzumaki (@) has no effect.

use inference_ast::nodes::{Expr, Stmt};

use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// Standalone uzumaki (@) used as an expression statement has no effect.
    #[id = "A008"]
    #[name = "Standalone uzumaki"]
    #[severity = error]
    pub struct StandaloneUzumaki;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, _walk_ctx| {
            if let Stmt::Expr(expr_id) = &arena[stmt_id].kind
                && matches!(arena[*expr_id].kind, Expr::Uzumaki)
            {
                errors.push(AnalysisDiagnostic::StandaloneUzumaki {
                    location: arena[*expr_id].location,
                });
            }
        });
        errors
    }
}
