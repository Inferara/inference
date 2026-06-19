//! A016: Compound-returning function calls only in `let` or `return`.
//!
//! Functions returning arrays or structs use the sret calling convention,
//! which requires the caller to provide a destination pointer. They can only
//! appear in variable definitions or return statements.

use inference_ast::ids::ExprId;
use inference_ast::nodes::{Def, Expr, Stmt};
use inference_type_checker::typed_context::TypedContext;

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// Compound-returning function calls only in `let` bindings or `return` statements.
    #[id = "A016"]
    #[name = "Compound return call position"]
    #[severity = error]
    pub struct CompoundReturnCallPosition;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            let module_path = walk_ctx.module_path.clone();
            check_stmt(ctx, &module_path, &ctx.arena()[stmt_id].kind, &mut errors);
        });
        errors
    }
}

fn check_stmt(
    ctx: &TypedContext,
    module_path: &[String],
    stmt: &Stmt,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    match stmt {
        Stmt::VarDef { value: Some(expr_id), .. } => {
            // Compound-returning calls are allowed directly as init value,
            // but we still need to recurse into subexpressions
            check_expr_children_only(ctx, module_path, *expr_id, errors);
        }
        Stmt::ConstDef(def_id) => {
            // Const initializers are symmetric to let initializers: a direct
            // compound-returning call is the whole point of sret, but any
            // nested compound-returning call in a subexpression is a violation.
            if let Def::Constant { value, .. } = &ctx.arena()[*def_id].kind {
                check_expr_children_only(ctx, module_path, *value, errors);
            }
        }
        Stmt::Return { expr } => {
            // Compound-returning calls are allowed directly in return,
            // but recurse into subexpressions
            check_expr_children_only(ctx, module_path, *expr, errors);
        }
        Stmt::Assign { left, right } => {
            // Compound-returning calls in assign RHS are handled by A017.
            // Here we check for them in the LHS and nested positions.
            check_expr(ctx, module_path, *left, errors);
            // Still check subexpressions of the RHS
            check_expr_children_only(ctx, module_path, *right, errors);
        }
        Stmt::Expr(expr_id) => {
            // Standalone expression: compound-returning calls are NOT allowed
            check_expr(ctx, module_path, *expr_id, errors);
        }
        Stmt::Assert { expr } => {
            check_expr(ctx, module_path, *expr, errors);
        }
        Stmt::If { condition, .. } => {
            check_expr(ctx, module_path, *condition, errors);
        }
        Stmt::Loop { condition: Some(cond_expr), .. } => {
            check_expr(ctx, module_path, *cond_expr, errors);
        }
        _ => {}
    }
}

/// Checks an expression and reports if it is a compound-returning call in
/// a disallowed position. Also recurses into child expressions.
fn check_expr(
    ctx: &TypedContext,
    module_path: &[String],
    expr_id: ExprId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    if walker::is_compound_returning_call(ctx, expr_id) {
        errors.push(LabeledDiagnostic::new(
            module_path.to_vec(),
            AnalysisDiagnostic::CompoundReturnCallInExpressionPosition {
                location: ctx.arena()[expr_id].location,
            },
        ));
        // Still recurse into the arguments to find nested violations
    }
    check_expr_children_only(ctx, module_path, expr_id, errors);
}

/// Recurses into child expressions without checking the current expression itself.
fn check_expr_children_only(
    ctx: &TypedContext,
    module_path: &[String],
    expr_id: ExprId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    let arena = ctx.arena();
    match &arena[expr_id].kind {
        Expr::FunctionCall { function, args, .. } => {
            check_expr(ctx, module_path, *function, errors);
            for (_, arg_expr) in args {
                check_expr(ctx, module_path, *arg_expr, errors);
            }
        }
        Expr::Binary { left, right, .. } => {
            check_expr(ctx, module_path, *left, errors);
            check_expr(ctx, module_path, *right, errors);
        }
        Expr::PrefixUnary { expr, .. }
        | Expr::Parenthesized { expr }
        | Expr::MemberAccess { expr, .. }
        | Expr::TypeMemberAccess { expr, .. } => {
            check_expr(ctx, module_path, *expr, errors);
        }
        Expr::ArrayIndexAccess { array, index } => {
            check_expr(ctx, module_path, *array, errors);
            check_expr(ctx, module_path, *index, errors);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, field_expr) in fields {
                check_expr(ctx, module_path, *field_expr, errors);
            }
        }
        Expr::ArrayLiteral { elements } => {
            for elem in elements {
                check_expr(ctx, module_path, *elem, errors);
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
