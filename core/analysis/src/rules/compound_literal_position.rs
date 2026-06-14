//! A015: Compound literals only in supported positions.
//!
//! Struct and array literals can only appear as variable initializers,
//! const initializers, assignment RHS, return values, or struct field
//! values. They cannot be used in arbitrary expression positions due to
//! codegen limitations.

use inference_ast::arena::AstArena;
use inference_ast::ids::ExprId;
use inference_ast::nodes::{Def, Expr, Stmt};

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// Compound literals (struct/array) must appear in supported positions.
    #[id = "A015"]
    #[name = "Compound literal position"]
    #[severity = error]
    pub struct CompoundLiteralPosition;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            let module_path = walk_ctx.module_path.clone();
            check_stmt(arena, &module_path, &arena[stmt_id].kind, &mut errors);
        });
        errors
    }
}

fn check_stmt(
    arena: &AstArena,
    module_path: &[String],
    stmt: &Stmt,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    match stmt {
        Stmt::VarDef { value: Some(expr_id), .. } => {
            // Compound literals are allowed as the init expression
            check_expr(arena, module_path, *expr_id, true, errors);
        }
        Stmt::ConstDef(def_id) => {
            // Const initializers are treated symmetrically to let initializers:
            // the initializer ExprId is an allowed compound-literal position.
            if let Def::Constant { value, .. } = &arena[*def_id].kind {
                check_expr(arena, module_path, *value, true, errors);
            }
        }
        Stmt::Assign { left, right } => {
            check_expr(arena, module_path, *left, false, errors);
            // Compound literals are allowed as the RHS
            check_expr(arena, module_path, *right, true, errors);
        }
        Stmt::Return { expr } => {
            // Compound literals are allowed in return statements
            check_expr(arena, module_path, *expr, true, errors);
        }
        Stmt::Expr(expr_id) => {
            check_expr(arena, module_path, *expr_id, false, errors);
        }
        Stmt::Assert { expr } => {
            check_expr(arena, module_path, *expr, false, errors);
        }
        Stmt::If { condition, .. } => {
            check_expr(arena, module_path, *condition, false, errors);
        }
        Stmt::Loop { condition: Some(cond_expr), .. } => {
            check_expr(arena, module_path, *cond_expr, false, errors);
        }
        _ => {}
    }
}

fn check_expr(
    arena: &AstArena,
    module_path: &[String],
    expr_id: ExprId,
    allowed: bool,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    match &arena[expr_id].kind {
        Expr::ArrayLiteral { elements } => {
            if !allowed {
                errors.push(LabeledDiagnostic::new(
                    module_path.to_vec(),
                    AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition {
                        kind: "array",
                        location: arena[expr_id].location,
                    },
                ));
                return;
            }
            // Elements of an array literal are allowed to be compound literals themselves
            for elem in elements {
                check_expr(arena, module_path, *elem, true, errors);
            }
        }
        Expr::StructLiteral { fields, .. } => {
            if !allowed {
                errors.push(LabeledDiagnostic::new(
                    module_path.to_vec(),
                    AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition {
                        kind: "struct",
                        location: arena[expr_id].location,
                    },
                ));
                return;
            }
            // Field values in a struct literal are allowed to be compound literals
            for (_, field_expr) in fields {
                check_expr(arena, module_path, *field_expr, true, errors);
            }
        }
        Expr::FunctionCall { function, args, .. } => {
            check_expr(arena, module_path, *function, false, errors);
            // Function arguments are an allowed position for compound literals in A015;
            // dedicated rules A012/A013 handle literal-as-argument restrictions.
            for (_, arg_expr) in args {
                check_expr(arena, module_path, *arg_expr, true, errors);
            }
        }
        Expr::Binary { left, right, .. } => {
            check_expr(arena, module_path, *left, false, errors);
            check_expr(arena, module_path, *right, false, errors);
        }
        Expr::PrefixUnary { expr, .. }
        | Expr::Parenthesized { expr }
        | Expr::MemberAccess { expr, .. }
        | Expr::TypeMemberAccess { expr, .. } => {
            check_expr(arena, module_path, *expr, false, errors);
        }
        Expr::ArrayIndexAccess { array, index } => {
            check_expr(arena, module_path, *array, false, errors);
            check_expr(arena, module_path, *index, false, errors);
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
