//! A017: Cannot assign from a compound-returning function call.
//!
//! Functions returning arrays or structs use the sret calling convention.
//! Assignment targets already have an address but the codegen cannot wire
//! it as an sret destination. Use a fresh `let` binding instead.

use inference_ast::ids::{ExprId, NodeId};
use inference_ast::nodes::{Expr, Stmt};
use inference_type_checker::type_info::TypeInfoKind;
use inference_type_checker::typed_context::TypedContext;

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// Cannot assign from a compound-returning function call.
    #[id = "A017"]
    #[name = "Compound return call in assignment"]
    #[severity = error]
    pub struct CompoundReturnCallAssignment;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            let module_path = walk_ctx.module_path.clone();
            if let Stmt::Assign { right, .. } = &ctx.arena()[stmt_id].kind {
                check_assign_rhs(ctx, &module_path, *right, &mut errors);
            }
        });
        errors
    }
}

fn check_assign_rhs(
    ctx: &TypedContext,
    module_path: &[String],
    expr_id: ExprId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    if matches!(ctx.arena()[expr_id].kind, Expr::FunctionCall { .. })
        && let Some(ti) = ctx.get_node_typeinfo(NodeId::Expr(expr_id))
        && matches!(
            ti.kind,
            TypeInfoKind::Array(_, _) | TypeInfoKind::Struct(_, _) | TypeInfoKind::Custom(_)
        )
    {
        errors.push(LabeledDiagnostic::new(
            module_path.to_vec(),
            AnalysisDiagnostic::CompoundReturnCallInAssignment {
                location: ctx.arena()[expr_id].location,
            },
        ));
    }
}
