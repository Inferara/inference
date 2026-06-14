//! A025: Variable declarations must have an initializer.
//!
//! Every variable must be initialized at declaration. Uninitialized variables
//! would require tracking definite assignment, which complicates formal
//! verification. Use explicit initialization instead.

use inference_ast::nodes::Stmt;

use crate::{errors::{AnalysisDiagnostic, LabeledDiagnostic}, walker};

crate::rule! {
    /// Variable declarations must have an initializer.
    #[id = "A025"]
    #[name = "Uninitialized variable"]
    #[severity = error]
    pub struct UninitializedVariable;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            let module_path = walk_ctx.module_path.clone();
            if let Stmt::VarDef { name, value: None, .. } = &arena[stmt_id].kind {
                errors.push(LabeledDiagnostic::new(module_path.clone(), AnalysisDiagnostic::UninitializedVariable {
                    name: arena[*name].name.clone(),
                    location: arena[stmt_id].location,
                }));
            }
        });
        errors
    }
}
