//! A029: Compound literal in member access assignment.
//!
//! Compound literals (struct or array) cannot be used as the RHS of an
//! assignment where the LHS is a member access expression. The codegen does
//! not yet support writing struct/array literals directly to a compound field.
//! Use a temporary variable instead:
//! ```text
//! let temp = Inner { x: 1, y: 2 };
//! outer.inner = temp;
//! ```

use inference_ast::nodes::{Expr, Stmt};

use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// Compound literals cannot be used as RHS in member access assignments.
    #[id = "A029"]
    #[name = "Compound literal in member access assignment"]
    #[severity = error]
    pub struct CompoundLiteralMemberAssign;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, _walk_ctx| {
            if let Stmt::Assign { left, right } = &arena[stmt_id].kind
                && matches!(arena[*left].kind, Expr::MemberAccess { .. })
                && matches!(
                    arena[*right].kind,
                    Expr::StructLiteral { .. } | Expr::ArrayLiteral { .. }
                )
            {
                errors.push(AnalysisDiagnostic::CompoundLiteralInMemberAssign {
                    location: arena[*right].location,
                });
            }
        });
        errors
    }
}
