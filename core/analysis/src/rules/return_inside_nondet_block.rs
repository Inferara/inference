//! A005: Return statement must not appear inside a non-deterministic block.

use inference_ast::nodes::Stmt;

use crate::{errors::AnalysisError, walker};

crate::rule! {
    /// Return inside a non-deterministic block is prohibited.
    #[id = "A005"]
    #[name = "Return inside nondet block"]
    #[severity = error]
    pub struct ReturnInsideNonDetBlock;
    fn check(ctx: &TypedContext) -> Vec<AnalysisError> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            if matches!(arena[stmt_id].kind, Stmt::Return { .. })
                && walk_ctx.nondet_depth > 0
            {
                errors.push(AnalysisError::ReturnInsideNonDetBlock {
                    location: arena[stmt_id].location,
                });
            }
        });
        errors
    }
}
