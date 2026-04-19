//! A028: Uzumaki on array of structs.
//!
//! Uzumaki (@) cannot be assigned to an array whose element type is a struct.
//! Multidimensional arrays of scalars (e.g., `[[i32; 3]; 2]`) CAN use uzumaki
//! -- only struct elements are prohibited.

use inference_ast::ids::{ExprId, NodeId, StmtId};
use inference_ast::nodes::{Def, Expr, Stmt};
use inference_type_checker::type_info::TypeInfoKind;
use inference_type_checker::typed_context::TypedContext;

use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// Uzumaki (@) cannot be assigned to an array of structs.
    #[id = "A028"]
    #[name = "Uzumaki on struct in array"]
    #[severity = error]
    pub struct UzumakiOnStructInArray;
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
        && array_contains_struct(&type_info.kind)
    {
        errors.push(AnalysisDiagnostic::UzumakiOnStructInArray {
            location: arena[expr_id].location,
        });
    }
}

/// Recursively checks if an array type ultimately contains struct elements.
/// Returns true for `[Point; 3]` and `[[Point; 3]; 2]`, but false for
/// `[i32; 3]` and `[[i32; 3]; 2]`.
fn array_contains_struct(kind: &TypeInfoKind) -> bool {
    match kind {
        TypeInfoKind::Array(elem_type, _) => match &elem_type.kind {
            TypeInfoKind::Struct(_) | TypeInfoKind::Custom(_) => true,
            TypeInfoKind::Array(_, _) => array_contains_struct(&elem_type.kind),
            _ => false,
        },
        _ => false,
    }
}
