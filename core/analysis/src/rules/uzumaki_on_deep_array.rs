//! A030: Uzumaki on deep array (3D+).
//!
//! Uzumaki (@) cannot be assigned to arrays with more than 2 dimensions.
//! 1D arrays (`[i32; 3]`) and 2D arrays (`[[i32; 3]; 2]`) are supported,
//! but 3D and deeper arrays are not.

use inference_ast::ids::NodeId;
use inference_ast::nodes::{Expr, Stmt};

use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// Uzumaki (@) cannot be assigned to arrays with more than 2 dimensions.
    #[id = "A030"]
    #[name = "Uzumaki on deep array"]
    #[severity = error]
    pub struct UzumakiOnDeepArray;
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
                && walker::array_nesting_depth(&type_info.kind) > 2
            {
                errors.push(AnalysisDiagnostic::UzumakiOnDeepArray {
                    location: arena[*expr_id].location,
                });
            }
        });
        errors
    }
}
