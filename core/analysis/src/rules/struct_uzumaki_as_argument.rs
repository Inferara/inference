//! A039: Struct uzumaki (@) cannot be used as a function argument.
//!
//! When a function parameter is a struct (or a custom non-enum type), passing
//! `@` directly is not supported: codegen lowers a struct-typed `@` by filling a
//! named frame slot, and a call argument has no such name -- so it reaches codegen
//! with no enclosing variable and panics. Assign `@` to a variable first, then
//! pass the variable. This is the struct sibling of A014 (the array case in the
//! same position); arrays stay with A014, scalars and enums need no slot and are
//! allowed.

use inference_ast::ids::NodeId;
use inference_ast::nodes::Expr;
use inference_type_checker::type_info::TypeInfoKind;
use inference_type_checker::typed_context::TypedContext;

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// Struct uzumaki (@) cannot be used as a function argument.
    #[id = "A039"]
    #[name = "Struct uzumaki as argument"]
    #[severity = error]
    pub struct StructUzumakiAsArgument;
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
                                && arg_is_struct_like(ctx, &ti.kind)
                            {
                                errors.push(LabeledDiagnostic::new(
                                    module_path.clone(),
                                    AnalysisDiagnostic::StructUzumakiAsArgument {
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

/// A struct-typed `@` argument needs a named frame slot the call site cannot
/// supply. Arrays are A014's; scalars and enums need no slot. A `Custom` name
/// that resolves to an enum is scalar-like and allowed.
fn arg_is_struct_like(ctx: &TypedContext, kind: &TypeInfoKind) -> bool {
    match kind {
        TypeInfoKind::Struct(_, _) => true,
        TypeInfoKind::Custom(name) => ctx.lookup_enum(name).is_none(),
        _ => false,
    }
}
