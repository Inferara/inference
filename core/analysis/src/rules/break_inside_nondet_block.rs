//! A002: Break statement must not appear inside a non-deterministic block.

use inference_ast::nodes::Stmt;

use crate::{errors::AnalysisError, walker};

crate::rule! {
    /// Break inside a non-deterministic block is prohibited.
    #[id = "A002"]
    #[name = "Break inside nondet block"]
    pub struct BreakInsideNondetBlock;
    fn check(ctx: &TypedContext) -> Vec<AnalysisError> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            if matches!(arena[stmt_id].kind, Stmt::Break)
                && walk_ctx.loop_depth > 0
                && walk_ctx.nondet_depth > 0
            {
                errors.push(AnalysisError::BreakInsideNonDetBlock {
                    location: arena[stmt_id].location,
                });
            }
        });
        errors
    }
}
