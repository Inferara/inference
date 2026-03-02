//! A001: Break statement must appear inside a loop body.

use inference_ast::nodes::Stmt;

use crate::{errors::AnalysisError, walker};

crate::rule! {
    /// Break statement must appear inside a loop body.
    #[id = "A001"]
    #[name = "Break outside loop"]
    pub struct BreakOutsideLoop;
    fn check(ctx: &TypedContext) -> Vec<AnalysisError> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            if matches!(arena[stmt_id].kind, Stmt::Break)
                && walk_ctx.loop_depth == 0
            {
                errors.push(AnalysisError::BreakOutsideLoop {
                    location: arena[stmt_id].location,
                });
            }
        });
        errors
    }
}
