//! A005: Return statement must not appear inside a non-deterministic block.

use inference_ast::nodes::Stmt;

use crate::{errors::{AnalysisDiagnostic, LabeledDiagnostic}, walker};

crate::rule! {
    /// Return inside a non-deterministic block is prohibited.
    #[id = "A005"]
    #[name = "Return inside nondet block"]
    #[severity = error]
    pub struct ReturnInsideNonDetBlock;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            let module_path = walk_ctx.module_path.clone();
            if matches!(arena[stmt_id].kind, Stmt::Return { .. })
                && walk_ctx.nondet_depth > 0
            {
                errors.push(LabeledDiagnostic::new(module_path.clone(), AnalysisDiagnostic::ReturnInsideNonDetBlock {
                    location: arena[stmt_id].location,
                    block_kind: walk_ctx.nondet_block_kind.expect("nondet_depth > 0 implies nondet_block_kind is Some"),
                }));
            }
        });
        errors
    }
}
