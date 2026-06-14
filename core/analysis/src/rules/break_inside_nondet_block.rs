//! A002: Break statement must not appear inside a non-deterministic block.

use inference_ast::nodes::Stmt;

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// Break inside a non-deterministic block is prohibited.
    #[id = "A002"]
    #[name = "Break inside nondet block"]
    #[severity = error]
    pub struct BreakInsideNonDetBlock;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            if matches!(arena[stmt_id].kind, Stmt::Break)
                && walk_ctx.nondet_depth > 0
            {
                errors.push(LabeledDiagnostic::new(
                    walk_ctx.module_path.clone(),
                    AnalysisDiagnostic::BreakInsideNonDetBlock {
                        location: arena[stmt_id].location,
                        block_kind: walk_ctx.nondet_block_kind.expect("nondet_depth > 0 implies nondet_block_kind is Some"),
                    },
                ));
            }
        });
        errors
    }
}
