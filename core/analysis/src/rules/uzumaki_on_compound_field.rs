//! A038: Uzumaki (@) on a struct- or array-typed struct-literal field.
//!
//! In a struct literal, `@` initializes a field directly. A scalar field
//! (bool/number/enum) lowers to a single uzumaki opcode and needs no enclosing
//! variable, but a struct- or array-typed field requires a named frame slot the
//! field position cannot supply -- so `@` there reaches codegen with no variable
//! name and panics. Reject it here. Initialize a compound field with a literal
//! whose scalar leaves use `@` instead (e.g. `Inner { v: @ }`).
//!
//! This is the struct-literal-field analogue of A014 (array uzumaki as a function
//! argument): same root cause, different no-variable position. Unlike A027, which
//! permits `let s: S = @;` when `S`'s fields are all scalars or 1D scalar arrays
//! (codegen has a named slot to fill there), the field position has no slot at
//! all, so even a scalar-array field must be rejected -- do not narrow this rule
//! to A027's `has_compound_fields` predicate.

use inference_ast::ids::NodeId;
use inference_ast::nodes::Expr;
use inference_type_checker::type_info::TypeInfoKind;
use inference_type_checker::typed_context::TypedContext;

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// Uzumaki (@) cannot initialize a struct- or array-typed struct-literal field.
    #[id = "A038"]
    #[name = "Uzumaki on compound field"]
    #[severity = error]
    pub struct UzumakiOnCompoundField;
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
                    if let Expr::StructLiteral { fields, .. } = &arena[sub_id].kind {
                        for &(field_name_id, field_expr) in fields {
                            if matches!(arena[field_expr].kind, Expr::Uzumaki)
                                && let Some(ti) = ctx.get_node_typeinfo(NodeId::Expr(field_expr))
                                && field_needs_named_slot(ctx, &ti.kind)
                            {
                                errors.push(LabeledDiagnostic::new(
                                    module_path.clone(),
                                    AnalysisDiagnostic::UzumakiOnCompoundField {
                                        field: arena[field_name_id].name.clone(),
                                        ty: ti.kind.to_string(),
                                        location: arena[field_expr].location,
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

/// Whether a struct-literal field of this type, initialized with `@`, needs a
/// named frame slot the field position cannot supply. Scalars (bool/number) and
/// enums lower `@` to a single opcode and are fine; structs and arrays (including
/// scalar arrays like `[i32; 3]`, which panic identically in codegen) are not. A
/// `Custom` name that resolves to an enum is scalar-like and allowed.
fn field_needs_named_slot(ctx: &TypedContext, kind: &TypeInfoKind) -> bool {
    match kind {
        TypeInfoKind::Struct(_, _) | TypeInfoKind::Array(_, _) => true,
        TypeInfoKind::Custom(name) => walker::uzumaki_custom_is_struct_like(ctx, name),
        _ => false,
    }
}
