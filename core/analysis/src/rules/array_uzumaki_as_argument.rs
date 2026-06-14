//! A014: Array uzumaki (@) cannot be used as a function argument.
//!
//! When the parameter type of a function is an array, passing `@` directly
//! is not supported. The codegen requires a named variable for frame slot
//! allocation.

use inference_ast::ids::NodeId;
use inference_ast::nodes::Expr;
use inference_type_checker::type_info::TypeInfoKind;

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// Array uzumaki (@) cannot be used as a function argument.
    #[id = "A014"]
    #[name = "Array uzumaki as argument"]
    #[severity = error]
    pub struct ArrayUzumakiAsArgument;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            let module_path = walk_ctx.module_path.clone();
            walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
                walker::walk_expr(arena, expr_id, &mut |sub_id| {
                    if let Expr::FunctionCall { args, .. } = &arena[sub_id].kind {
                        for (_, arg_expr) in args {
                            if matches!(arena[*arg_expr].kind, Expr::Uzumaki)
                                && let Some(ti) = ctx.get_node_typeinfo(NodeId::Expr(*arg_expr))
                                && matches!(ti.kind, TypeInfoKind::Array(_, _))
                            {
                                errors.push(LabeledDiagnostic::new(
                                    module_path.clone(),
                                    AnalysisDiagnostic::ArrayUzumakiAsArgument {
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
