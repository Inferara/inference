//! A029: Compound literal in compound assignment.
//!
//! Compound literals (struct or array) cannot be used as the RHS of an
//! assignment where the LHS is a member access or array index expression.
//! The codegen does not yet support writing struct/array literals directly
//! to a compound element. Use a temporary variable instead:
//! ```text
//! let temp = Inner { x: 1, y: 2 };
//! outer.inner = temp;
//! ```

use inference_ast::nodes::{Expr, Stmt};

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// Compound literals cannot be used as RHS in compound element assignments.
    #[id = "A029"]
    #[name = "Compound literal in compound assignment"]
    #[severity = error]
    pub struct CompoundLiteralMemberAssign;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            let module_path = walk_ctx.module_path.clone();
            if let Stmt::Assign { left, right } = &arena[stmt_id].kind
                && matches!(arena[*left].kind, Expr::MemberAccess { .. } | Expr::ArrayIndexAccess { .. })
                && matches!(
                    arena[*right].kind,
                    Expr::StructLiteral { .. } | Expr::ArrayLiteral { .. }
                )
            {
                errors.push(LabeledDiagnostic::new(
                    module_path.clone(),
                    AnalysisDiagnostic::CompoundLiteralInCompoundAssign {
                        location: arena[*right].location,
                    },
                ));
            }
        });
        errors
    }
}
