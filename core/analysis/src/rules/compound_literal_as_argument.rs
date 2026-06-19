//! A012: Compound literals (array/struct) cannot be passed directly as function arguments.
//!
//! The codegen requires a named variable for frame slot allocation, so compound
//! literals must be assigned to a variable before passing to functions.

use inference_ast::nodes::Expr;

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// Compound literals cannot be passed directly as function arguments.
    #[id = "A012"]
    #[name = "Compound literal as argument"]
    #[severity = error]
    pub struct CompoundLiteralAsArgument;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            let module_path = walk_ctx.module_path.clone();
            walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
                walker::walk_expr(arena, expr_id, &mut |sub_id| {
                    if let Expr::FunctionCall { args, .. } = &arena[sub_id].kind {
                        for (_, arg_expr) in args {
                            let kind = match &arena[*arg_expr].kind {
                                Expr::ArrayLiteral { .. } => Some("Array"),
                                Expr::StructLiteral { .. } => Some("Struct"),
                                _ => None,
                            };
                            if let Some(kind) = kind {
                                errors.push(LabeledDiagnostic::new(
                                    module_path.clone(),
                                    AnalysisDiagnostic::CompoundLiteralAsArgument {
                                        kind,
                                        location: arena[*arg_expr].location,
                                    },
                                ));
                            }
                        }
                    }
                });
            });
        });
        errors
    }
}
