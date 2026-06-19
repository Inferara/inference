//! A023: Uzumaki (@) in reassignment is not allowed.
//!
//! Uzumaki expressions may only appear in variable declarations (`let x = @;`)
//! or as function arguments. Using uzumaki in an assignment (`x = @;`) is
//! rejected because the codegen cannot resolve the target slot for compound
//! types and the semantic intent is ambiguous for scalar types.

use inference_ast::nodes::{Expr, Stmt};

use crate::{errors::{AnalysisDiagnostic, LabeledDiagnostic}, walker};

crate::rule! {
    /// Uzumaki (@) in reassignment is not allowed.
    #[id = "A023"]
    #[name = "Uzumaki in reassignment"]
    #[severity = error]
    pub struct UzumakiInReassignment;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            let module_path = walk_ctx.module_path.clone();
            if let Stmt::Assign { right, .. } = &arena[stmt_id].kind
                && matches!(arena[*right].kind, Expr::Uzumaki)
            {
                errors.push(LabeledDiagnostic::new(module_path.clone(), AnalysisDiagnostic::UzumakiInReassignment {
                    location: arena[*right].location,
                }));
            }
        });
        errors
    }
}
