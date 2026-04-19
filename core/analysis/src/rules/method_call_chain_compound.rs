//! A018: Cannot chain method calls on compound-returning function calls.
//!
//! Chaining method calls on struct/array-returning functions creates implicit
//! temporaries that cannot be named in formal proofs. Assign the intermediate
//! result to a variable first.

use inference_ast::ids::ExprId;
use inference_ast::nodes::{Def, Expr, Stmt};
use inference_type_checker::typed_context::TypedContext;

use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// Cannot chain method calls on compound-returning function calls.
    #[id = "A018"]
    #[name = "Method call chain on compound return"]
    #[severity = error]
    pub struct MethodCallChainCompound;
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
        Stmt::ConstDef(def_id) => {
            // Const initializers need the same chain check as let initializers.
            if let Def::Constant { value, .. } = &ctx.arena()[*def_id].kind {
                check_expr(ctx, *value, errors);
            }
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
        // Instance method call: receiver.method(args)
        // The function expression is a MemberAccess { expr: receiver, name: method_name }
        Expr::FunctionCall { function, args, .. } => {
            if let Expr::MemberAccess { expr: receiver_expr, .. } = &arena[*function].kind {
                if walker::is_compound_returning_call(ctx, *receiver_expr) {
                    errors.push(AnalysisDiagnostic::MethodCallChainOnCompoundReturn {
                        location: arena[expr_id].location,
                    });
                    // Still recurse into args to find nested violations
                    for (_, arg_expr) in args {
                        check_expr(ctx, *arg_expr, errors);
                    }
                    return;
                }
                check_expr(ctx, *receiver_expr, errors);
            } else {
                check_expr(ctx, *function, errors);
            }
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
