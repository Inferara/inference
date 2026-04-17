//! A027: Uzumaki on nested struct type.
//!
//! Uzumaki (@) cannot be assigned to a struct variable if that struct has any
//! nested struct field or other non-scalar compound field (for example, an array
//! of structs or a multidimensional array). Uzumaki is only supported for structs
//! whose fields are all scalars or 1D arrays of scalars.

use inference_ast::ids::{ExprId, NodeId, StmtId};
use inference_ast::nodes::{Def, Expr, Stmt};
use inference_type_checker::type_info::TypeInfoKind;
use inference_type_checker::typed_context::TypedContext;
use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// Uzumaki (@) cannot be assigned to a struct with compound fields.
    #[id = "A027"]
    #[name = "Uzumaki on nested struct"]
    #[severity = error]
    pub struct UzumakiOnNestedStruct;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            if walk_ctx.nondet_depth == 0 {
                return;
            }
            let init_expr = match &arena[stmt_id].kind {
                Stmt::VarDef { value: Some(expr_id), .. } => Some(*expr_id),
                Stmt::ConstDef(def_id) => match &arena[*def_id].kind {
                    Def::Constant { value, .. } => Some(*value),
                    _ => None,
                },
                _ => None,
            };
            if let Some(expr_id) = init_expr {
                check_uzumaki_init(ctx, stmt_id, expr_id, &mut errors);
            }
        });
        errors
    }
}

fn check_uzumaki_init(
    ctx: &TypedContext,
    stmt_id: StmtId,
    expr_id: ExprId,
    errors: &mut Vec<AnalysisDiagnostic>,
) {
    let arena = ctx.arena();
    if matches!(arena[expr_id].kind, Expr::Uzumaki)
        && let Some(type_info) = ctx.get_node_typeinfo(NodeId::Stmt(stmt_id))
        && let TypeInfoKind::Struct(name) | TypeInfoKind::Custom(name) = &type_info.kind
        && walker::has_compound_fields(ctx, &type_info.kind)
    {
        errors.push(AnalysisDiagnostic::UzumakiOnNestedStruct {
            name: name.clone(),
            location: arena[expr_id].location,
        });
    }
}
