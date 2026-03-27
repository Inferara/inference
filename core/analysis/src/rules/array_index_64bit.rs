//! A019: Array index must be a 32-bit integer type.
//!
//! WASM array indexing uses `i32.mul` for address computation, so the index
//! must be a 32-bit (or sub-32-bit) integer type. 64-bit indices are rejected.

use inference_ast::ids::NodeId;
use inference_ast::nodes::Expr;
use inference_type_checker::type_info::{NumberType, TypeInfoKind};

use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// Array index must be a 32-bit integer type.
    #[id = "A019"]
    #[name = "Array index 64-bit"]
    #[severity = error]
    pub struct ArrayIndex64Bit;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, _walk_ctx| {
            walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
                walker::walk_expr(arena, expr_id, &mut |sub_id| {
                    if let Expr::ArrayIndexAccess { index, .. } = &arena[sub_id].kind
                        && let Some(index_ti) = ctx.get_node_typeinfo(NodeId::Expr(*index))
                        && matches!(
                            index_ti.kind,
                            TypeInfoKind::Number(NumberType::I64 | NumberType::U64)
                        )
                    {
                        errors.push(AnalysisDiagnostic::ArrayIndex64Bit {
                            found: index_ti.to_string(),
                            location: arena[sub_id].location,
                        });
                    }
                });
            });
        });
        errors
    }
}
