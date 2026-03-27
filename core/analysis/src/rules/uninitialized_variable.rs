//! A025: Variable declarations must have an initializer.
//!
//! Every variable must be initialized at declaration. Uninitialized variables
//! would require tracking definite assignment, which complicates formal
//! verification. Use explicit initialization instead.

use inference_ast::nodes::Stmt;

use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// Variable declarations must have an initializer.
    #[id = "A025"]
    #[name = "Uninitialized variable"]
    #[severity = error]
    pub struct UninitializedVariable;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, _walk_ctx| {
            if let Stmt::VarDef { name, value: None, .. } = &arena[stmt_id].kind {
                errors.push(AnalysisDiagnostic::UninitializedVariable {
                    name: arena[*name].name.clone(),
                    location: arena[stmt_id].location,
                });
            }
        });
        errors
    }
}
