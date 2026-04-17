//! A015: Compound literals only in supported positions.
//!
//! Struct and array literals can only appear as variable initializers,
//! const initializers, assignment RHS, return values, or struct field
//! values. They cannot be used in arbitrary expression positions due to
//! codegen limitations.

use inference_ast::arena::AstArena;
use inference_ast::ids::ExprId;
use inference_ast::nodes::{Def, Expr, Stmt};

use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// Compound literals (struct/array) must appear in supported positions.
    #[id = "A015"]
    #[name = "Compound literal position"]
    #[severity = error]
    pub struct CompoundLiteralPosition;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, _walk_ctx| {
            check_stmt(arena, &arena[stmt_id].kind, &mut errors);
        });
        errors
    }
}

fn check_stmt(
    arena: &AstArena,
    stmt: &Stmt,
    errors: &mut Vec<AnalysisDiagnostic>,
) {
    match stmt {
        Stmt::VarDef { value: Some(expr_id), .. } => {
            // Compound literals are allowed as the init expression
            check_expr(arena, *expr_id, true, errors);
        }
        Stmt::ConstDef(def_id) => {
            // Const initializers are treated symmetrically to let initializers:
            // the initializer ExprId is an allowed compound-literal position.
            if let Def::Constant { value, .. } = &arena[*def_id].kind {
                check_expr(arena, *value, true, errors);
            }
        }
        Stmt::Assign { left, right } => {
            check_expr(arena, *left, false, errors);
            // Compound literals are allowed as the RHS
            check_expr(arena, *right, true, errors);
        }
        Stmt::Return { expr } => {
            // Compound literals are allowed in return statements
            check_expr(arena, *expr, true, errors);
        }
        Stmt::Expr(expr_id) => {
            check_expr(arena, *expr_id, false, errors);
        }
        Stmt::Assert { expr } => {
            check_expr(arena, *expr, false, errors);
        }
        Stmt::If { condition, .. } => {
            check_expr(arena, *condition, false, errors);
        }
        Stmt::Loop { condition: Some(cond_expr), .. } => {
            check_expr(arena, *cond_expr, false, errors);
        }
        _ => {}
    }
}

fn check_expr(
    arena: &AstArena,
    expr_id: ExprId,
    allowed: bool,
    errors: &mut Vec<AnalysisDiagnostic>,
) {
    match &arena[expr_id].kind {
        Expr::ArrayLiteral { elements } => {
            if !allowed {
                errors.push(AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition {
                    kind: "array",
                    location: arena[expr_id].location,
                });
                return;
            }
            // Elements of an array literal are allowed to be compound literals themselves
            for elem in elements {
                check_expr(arena, *elem, true, errors);
            }
        }
        Expr::StructLiteral { fields, .. } => {
            if !allowed {
                errors.push(AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition {
                    kind: "struct",
                    location: arena[expr_id].location,
                });
                return;
            }
            // Field values in a struct literal are allowed to be compound literals
            for (_, field_expr) in fields {
                check_expr(arena, *field_expr, true, errors);
            }
        }
        Expr::FunctionCall { function, args, .. } => {
            check_expr(arena, *function, false, errors);
            // Function arguments are an allowed position for compound literals in A015;
            // dedicated rules A012/A013 handle literal-as-argument restrictions.
            for (_, arg_expr) in args {
                check_expr(arena, *arg_expr, true, errors);
            }
        }
        Expr::Binary { left, right, .. } => {
            check_expr(arena, *left, false, errors);
            check_expr(arena, *right, false, errors);
        }
        Expr::PrefixUnary { expr, .. }
        | Expr::Parenthesized { expr }
        | Expr::MemberAccess { expr, .. }
        | Expr::TypeMemberAccess { expr, .. } => {
            check_expr(arena, *expr, false, errors);
        }
        Expr::ArrayIndexAccess { array, index } => {
            check_expr(arena, *array, false, errors);
            check_expr(arena, *index, false, errors);
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
