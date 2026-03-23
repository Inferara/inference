//! A006: Uzumaki (@) must appear inside a non-deterministic block.

use inference_ast::arena::AstArena;
use inference_ast::ids::ExprId;
use inference_ast::nodes::{Expr, Stmt};

use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// Uzumaki (@) must appear inside a non-deterministic block.
    #[id = "A006"]
    #[name = "Uzumaki outside nondet block"]
    #[severity = error]
    pub struct UzumakiOutsideNonDetBlock;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            if walk_ctx.nondet_depth > 0 {
                return;
            }
            match &arena[stmt_id].kind {
                Stmt::VarDef { value: Some(expr_id), .. }
                | Stmt::Expr(expr_id) => {
                    find_uzumaki(arena, *expr_id, &mut errors);
                }
                Stmt::Assign { right, .. } => {
                    find_uzumaki(arena, *right, &mut errors);
                }
                Stmt::Return { expr }
                | Stmt::Assert { expr } => {
                    find_uzumaki(arena, *expr, &mut errors);
                }
                Stmt::If { condition, .. } => {
                    find_uzumaki(arena, *condition, &mut errors);
                }
                Stmt::Loop { condition: Some(cond), .. } => {
                    find_uzumaki(arena, *cond, &mut errors);
                }
                _ => {}
            }
        });
        errors
    }
}

fn find_uzumaki(arena: &AstArena, expr_id: ExprId, errors: &mut Vec<AnalysisDiagnostic>) {
    match &arena[expr_id].kind {
        Expr::Uzumaki => {
            errors.push(AnalysisDiagnostic::UzumakiOutsideNonDetBlock {
                location: arena[expr_id].location,
            });
        }
        Expr::Binary { left, right, .. } => {
            find_uzumaki(arena, *left, errors);
            find_uzumaki(arena, *right, errors);
        }
        Expr::PrefixUnary { expr, .. }
        | Expr::Parenthesized { expr }
        | Expr::MemberAccess { expr, .. }
        | Expr::TypeMemberAccess { expr, .. } => {
            find_uzumaki(arena, *expr, errors);
        }
        Expr::FunctionCall { function, args, .. } => {
            find_uzumaki(arena, *function, errors);
            for (_, arg_expr) in args {
                find_uzumaki(arena, *arg_expr, errors);
            }
        }
        Expr::ArrayIndexAccess { array, index } => {
            find_uzumaki(arena, *array, errors);
            find_uzumaki(arena, *index, errors);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, field_expr) in fields {
                find_uzumaki(arena, *field_expr, errors);
            }
        }
        Expr::ArrayLiteral { elements } => {
            for elem in elements {
                find_uzumaki(arena, *elem, errors);
            }
        }
        Expr::Identifier(_)
        | Expr::NumberLiteral { .. }
        | Expr::BoolLiteral { .. }
        | Expr::StringLiteral { .. }
        | Expr::UnitLiteral
        | Expr::Type(_) => {}
    }
}
