//! A003: Return statement must not appear inside a loop body.

use inference_ast::nodes::Stmt;

use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// Return inside a loop body is prohibited.
    #[id = "A003"]
    #[name = "Return inside loop"]
    #[severity = error]
    pub struct ReturnInsideLoop;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            if matches!(arena[stmt_id].kind, Stmt::Return { .. })
                && walk_ctx.loop_depth > 0
            {
                errors.push(AnalysisDiagnostic::ReturnInsideLoop {
                    location: arena[stmt_id].location,
                });
            }
        });
        errors
    }
}
