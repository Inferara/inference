//! A019: Array index must be a 32-bit integer type.
//!
//! WASM array indexing uses `i32.mul` for address computation, so the index
//! must be a 32-bit (or sub-32-bit) integer type. 64-bit indices are rejected.

use inference_ast::ids::{ExprId, NodeId};
use inference_ast::nodes::{Expr, Stmt};
use inference_type_checker::type_info::{NumberType, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;

use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// Array index must be a 32-bit integer type.
    #[id = "A019"]
    #[name = "Array index 64-bit"]
    #[severity = error]
    pub struct ArrayIndex64Bit;
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
        Stmt::Return { expr } | Stmt::Assert { expr } => check_expr(ctx, *expr, errors),
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
    expr_id: ExprId,
    errors: &mut Vec<AnalysisDiagnostic>,
) {
    let arena = ctx.arena();
    match &arena[expr_id].kind {
        Expr::ArrayIndexAccess { array, index } => {
            check_expr(ctx, *array, errors);
            if let Some(index_ti) = ctx.get_node_typeinfo(NodeId::Expr(*index))
                && matches!(
                    index_ti.kind,
                    TypeInfoKind::Number(NumberType::I64 | NumberType::U64)
                )
            {
                errors.push(AnalysisDiagnostic::ArrayIndex64Bit {
                    found: index_ti.to_string(),
                    location: arena[expr_id].location,
                });
            }
            check_expr(ctx, *index, errors);
        }
        Expr::FunctionCall { function, args, .. } => {
            check_expr(ctx, *function, errors);
            for (_, arg_expr) in args {
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
