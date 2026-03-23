//! A014: Array uzumaki (@) cannot be used as a function argument.
//!
//! When the parameter type of a function is an array, passing `@` directly
//! is not supported. The codegen requires a named variable for frame slot
//! allocation.

use inference_ast::ids::NodeId;
use inference_ast::nodes::{Expr, Stmt};
use inference_type_checker::type_info::TypeInfoKind;
use inference_type_checker::typed_context::TypedContext;

use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// Array uzumaki (@) cannot be used as a function argument.
    #[id = "A014"]
    #[name = "Array uzumaki as argument"]
    #[severity = error]
    pub struct ArrayUzumakiAsArgument;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut errors = Vec::new();
        walker::walk_function_bodies(ctx, &mut |stmt_id, _walk_ctx| {
            visit_stmt(ctx, &ctx.arena()[stmt_id].kind, &mut errors);
        });
        errors
    }
}

fn visit_stmt(
    ctx: &TypedContext,
    stmt: &Stmt,
    errors: &mut Vec<AnalysisDiagnostic>,
) {
    match stmt {
        Stmt::VarDef { value: Some(expr_id), .. } | Stmt::Expr(expr_id) => {
            check_expr(ctx, *expr_id, errors);
        }
        Stmt::Assign { left, right } => {
            check_expr(ctx, *left, errors);
            check_expr(ctx, *right, errors);
        }
        Stmt::Return { expr } | Stmt::Assert { expr } => {
            check_expr(ctx, *expr, errors);
        }
        Stmt::If { condition, .. } => {
            check_expr(ctx, *condition, errors);
        }
        Stmt::Loop { condition: Some(cond_expr), .. } => {
            check_expr(ctx, *cond_expr, errors);
        }
        _ => {}
    }
}

fn check_expr(
    ctx: &TypedContext,
    expr_id: inference_ast::ids::ExprId,
    errors: &mut Vec<AnalysisDiagnostic>,
) {
    let arena = ctx.arena();
    match &arena[expr_id].kind {
        Expr::FunctionCall { function, args, .. } => {
            check_expr(ctx, *function, errors);
            for (_, arg_expr) in args {
                if matches!(arena[*arg_expr].kind, Expr::Uzumaki)
                    && let Some(ti) = ctx.get_node_typeinfo(NodeId::Expr(*arg_expr))
                    && matches!(ti.kind, TypeInfoKind::Array(_, _))
                {
                    errors.push(AnalysisDiagnostic::ArrayUzumakiAsArgument {
                        location: arena[*arg_expr].location,
                    });
                }
                check_expr(ctx, *arg_expr, errors);
            }
        }
        Expr::Binary { left, right, .. } => {
            check_expr(ctx, *left, errors);
            check_expr(ctx, *right, errors);
        }
        Expr::PrefixUnary { expr, .. }
        | Expr::Parenthesized { expr }
        | Expr::MemberAccess { expr, .. }
        | Expr::TypeMemberAccess { expr, .. } => {
            check_expr(ctx, *expr, errors);
        }
        Expr::ArrayIndexAccess { array, index } => {
            check_expr(ctx, *array, errors);
            check_expr(ctx, *index, errors);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, field_expr) in fields {
                check_expr(ctx, *field_expr, errors);
            }
        }
        Expr::ArrayLiteral { elements } => {
            for elem in elements {
                check_expr(ctx, *elem, errors);
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
