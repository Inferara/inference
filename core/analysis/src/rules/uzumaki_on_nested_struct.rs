//! A027: Uzumaki on nested struct type.
//!
//! Uzumaki (@) cannot be assigned to a struct variable if that struct has any
//! nested struct field or other non-scalar compound field (for example, an array
//! of structs or a multidimensional array). Uzumaki is only supported for structs
//! whose fields are all scalars or 1D arrays of scalars.

use inference_ast::ids::NodeId;
use inference_ast::nodes::{Expr, Stmt};
use inference_type_checker::type_info::TypeInfoKind;
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
            if let Stmt::VarDef {
                value: Some(expr_id),
                ..
            } = &arena[stmt_id].kind
                && matches!(arena[*expr_id].kind, Expr::Uzumaki)
                && walk_ctx.nondet_depth > 0
                && let Some(type_info) = ctx.get_node_typeinfo(NodeId::Stmt(stmt_id))
                && let TypeInfoKind::Struct(name) | TypeInfoKind::Custom(name) = &type_info.kind
                && walker::has_compound_fields(ctx, &type_info.kind)
            {
                errors.push(AnalysisDiagnostic::UzumakiOnNestedStruct {
                    name: name.clone(),
                    location: arena[*expr_id].location,
                });
            }
        });
        errors
    }
}
