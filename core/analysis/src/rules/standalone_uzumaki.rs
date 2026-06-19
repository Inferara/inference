//! A008: Standalone uzumaki (@) has no effect.

use inference_ast::nodes::{Expr, Stmt};

use crate::{errors::{AnalysisDiagnostic, LabeledDiagnostic}, walker};

crate::rule! {
    /// Standalone uzumaki (@) used as an expression statement has no effect.
    #[id = "A008"]
    #[name = "Standalone uzumaki"]
    #[severity = error]
    pub struct StandaloneUzumaki;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            let module_path = walk_ctx.module_path.clone();
            if let Stmt::Expr(expr_id) = &arena[stmt_id].kind
                && matches!(arena[*expr_id].kind, Expr::Uzumaki)
            {
                errors.push(LabeledDiagnostic::new(module_path.clone(), AnalysisDiagnostic::StandaloneUzumaki {
                    location: arena[*expr_id].location,
                }));
            }
        });
        errors
    }
}
