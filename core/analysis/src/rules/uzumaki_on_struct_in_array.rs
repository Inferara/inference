//! A028: Uzumaki on array of structs.
//!
//! Uzumaki (@) cannot be assigned to an array whose element type is a struct.
//! Multidimensional arrays of scalars (e.g., `[[i32; 3]; 2]`) CAN use uzumaki
//! -- only struct elements are prohibited.

use inference_ast::ids::NodeId;
use inference_ast::nodes::{Expr, Stmt};
use inference_type_checker::type_info::TypeInfoKind;

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
            if let Stmt::VarDef {
                value: Some(expr_id),
                ..
            } = &arena[stmt_id].kind
                && matches!(arena[*expr_id].kind, Expr::Uzumaki)
                && walk_ctx.nondet_depth > 0
                && let Some(type_info) = ctx.get_node_typeinfo(NodeId::Stmt(stmt_id))
                && walker::array_nesting_depth(&type_info.kind) <= 2
                && array_contains_struct(&type_info.kind)
            {
                errors.push(AnalysisDiagnostic::UzumakiOnStructInArray {
                    location: arena[*expr_id].location,
                });
            }
        });
        errors
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
