//! A012: Array literals cannot be passed directly as function arguments.
//!
//! The codegen requires a named variable for frame slot allocation, so array
//! literals must be assigned to a variable before passing to functions.

use inference_ast::nodes::{Expr, Stmt};

use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// Array literals cannot be passed directly as function arguments.
    #[id = "A012"]
    #[name = "Array literal as argument"]
    #[severity = error]
    pub struct ArrayLiteralAsArgument;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, _walk_ctx| {
            visit_stmt_for_function_calls(arena, &arena[stmt_id].kind, &mut errors);
        });
        errors
    }
}

fn visit_stmt_for_function_calls(
    arena: &inference_ast::arena::AstArena,
    stmt: &Stmt,
    errors: &mut Vec<AnalysisDiagnostic>,
) {
    match stmt {
        Stmt::VarDef { value: Some(expr_id), .. } | Stmt::Expr(expr_id) => {
            check_expr_args(arena, *expr_id, errors);
        }
        Stmt::Assign { left, right } => {
            check_expr_args(arena, *left, errors);
            check_expr_args(arena, *right, errors);
        }
        Stmt::Return { expr } | Stmt::Assert { expr } => {
            check_expr_args(arena, *expr, errors);
        }
        Stmt::If { condition, .. } => {
            check_expr_args(arena, *condition, errors);
        }
        Stmt::Loop { condition: Some(cond_expr), .. } => {
            check_expr_args(arena, *cond_expr, errors);
        }
        _ => {}
    }
}

fn check_expr_args(
    arena: &inference_ast::arena::AstArena,
    expr_id: inference_ast::ids::ExprId,
    errors: &mut Vec<AnalysisDiagnostic>,
) {
    match &arena[expr_id].kind {
        Expr::FunctionCall { function, args, .. } => {
            check_expr_args(arena, *function, errors);
            for (_, arg_expr) in args {
                if matches!(arena[*arg_expr].kind, Expr::ArrayLiteral { .. }) {
                    errors.push(AnalysisDiagnostic::ArrayLiteralAsArgument {
                        location: arena[*arg_expr].location,
                    });
                }
                check_expr_args(arena, *arg_expr, errors);
            }
        }
        Expr::Binary { left, right, .. } => {
            check_expr_args(arena, *left, errors);
            check_expr_args(arena, *right, errors);
        }
        Expr::PrefixUnary { expr, .. }
        | Expr::Parenthesized { expr }
        | Expr::MemberAccess { expr, .. }
        | Expr::TypeMemberAccess { expr, .. } => {
            check_expr_args(arena, *expr, errors);
        }
        Expr::ArrayIndexAccess { array, index } => {
            check_expr_args(arena, *array, errors);
            check_expr_args(arena, *index, errors);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, field_expr) in fields {
                check_expr_args(arena, *field_expr, errors);
            }
        }
        Expr::ArrayLiteral { elements } => {
            for elem in elements {
                check_expr_args(arena, *elem, errors);
            }
        }
        Expr::Identifier(_)
        | Expr::NumberLiteral { .. }
        | Expr::BoolLiteral { .. }
        | Expr::StringLiteral { .. }
        | Expr::UnitLiteral
        | Expr::Uzumaki
        | Expr::Type(_) => {}
    }
}
