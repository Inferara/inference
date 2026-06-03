//! A037: Constant array index must be within bounds.
//!
//! When an array is indexed by a constant integer literal, the index is known
//! at compile time and the array length is known from the array sub-expression's
//! type info (`Array(_, length)`). A literal index that is negative or `>= length`
//! is rejected statically, in every profile and mode, at zero runtime cost.
//!
//! A negative literal such as `arr[-42]` lowers to a single `NumberLiteral`
//! whose raw text keeps the leading `-`, so parsing the value as `i128` catches
//! negative constant indices directly. Spaced forms like `arr[- 42]` lower to a
//! `PrefixUnary` and are intentionally out of scope here; they fall to the future
//! runtime guard. A literal too large to fit in `i128` is treated as out of
//! bounds, mirroring `literal_out_of_range`'s parse-failure handling.

use inference_ast::ids::NodeId;
use inference_ast::nodes::Expr;
use inference_type_checker::type_info::TypeInfoKind;

use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// Constant array index must be within the array's bounds.
    #[id = "A037"]
    #[name = "Array index const out of bounds"]
    #[severity = error]
    pub struct ArrayIndexConstOob;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, _walk_ctx| {
            walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
                walker::walk_expr(arena, expr_id, &mut |sub_id| {
                    if let Expr::ArrayIndexAccess { array, index } = &arena[sub_id].kind
                        && let Expr::NumberLiteral { value } = &arena[*index].kind
                        && let Some(array_ti) = ctx.get_node_typeinfo(NodeId::Expr(*array))
                        && let TypeInfoKind::Array(_, length) = array_ti.kind
                    {
                        let out_of_bounds = match value.parse::<i128>() {
                            Ok(parsed) => parsed < 0 || parsed >= i128::from(length),
                            Err(_) => true,
                        };
                        if out_of_bounds {
                            errors.push(AnalysisDiagnostic::ArrayIndexConstOutOfBounds {
                                index: value.clone(),
                                length,
                                location: arena[sub_id].location,
                            });
                        }
                    }
                });
            });
        });
        errors
    }
}
