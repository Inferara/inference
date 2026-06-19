//! A040: Uzumaki (@) on a struct- or array-typed array-literal element.
//!
//! In an array literal, `@` initializes one element directly. A scalar element
//! (bool/number/enum) lowers to a single uzumaki opcode and needs no enclosing
//! variable, but a struct- or array-typed element requires a named frame slot the
//! element position cannot supply -- so `@` there reaches codegen with no variable
//! name and panics. Reject it here. Initialize a compound element by binding the
//! value to a variable first, then using the variable as the element.
//!
//! This is the array-literal-element analogue of A038 (uzumaki on a struct-literal
//! field) and A014 (array uzumaki as a function argument): same root cause,
//! different no-variable position. It is distinct from A028 (uzumaki on an array of
//! structs), which flags the *whole-array* form `let a: [Point; 2] = @;` (a
//! statement-position `@`); A040 flags an `@` *element* of an array literal. A040
//! also rejects a nested-array element -- the outer `@` in `[@, [1, 2]]` typed
//! `[[i32; 2]; 2]`, whose element type is itself an array -- which A028 does not
//! cover.

use inference_ast::ids::NodeId;
use inference_ast::nodes::Expr;
use inference_type_checker::type_info::TypeInfoKind;
use inference_type_checker::typed_context::TypedContext;

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// Uzumaki (@) cannot initialize a struct- or array-typed array-literal element.
    #[id = "A040"]
    #[name = "Uzumaki on compound array element"]
    #[severity = error]
    pub struct UzumakiOnCompoundArrayElement;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            if walk_ctx.nondet_depth == 0 {
                return;
            }
            let module_path = walk_ctx.module_path.clone();
            walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
                walker::walk_expr(arena, expr_id, &mut |sub_id| {
                    if let Expr::ArrayLiteral { elements } = &arena[sub_id].kind {
                        for &element in elements {
                            if matches!(arena[element].kind, Expr::Uzumaki)
                                && let Some(ti) = ctx.get_node_typeinfo(NodeId::Expr(element))
                                && element_needs_named_slot(ctx, &ti.kind)
                            {
                                errors.push(LabeledDiagnostic::new(
                                    module_path.clone(),
                                    AnalysisDiagnostic::UzumakiOnCompoundArrayElement {
                                        ty: ti.kind.to_string(),
                                        location: arena[element].location,
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

/// Whether an array-literal element of this type, initialized with `@`, needs a
/// named frame slot the element position cannot supply. Scalars (bool/number) and
/// enums lower `@` to a single opcode and are fine; structs and arrays (including
/// scalar arrays like `[i32; 3]`, the element type of a multidimensional array)
/// are not. A `Custom` name that resolves to an enum is scalar-like and allowed.
fn element_needs_named_slot(ctx: &TypedContext, kind: &TypeInfoKind) -> bool {
    match kind {
        TypeInfoKind::Struct(_, _) | TypeInfoKind::Array(_, _) => true,
        TypeInfoKind::Custom(name) => walker::uzumaki_custom_is_struct_like(ctx, name),
        _ => false,
    }
}
