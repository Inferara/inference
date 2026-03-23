//! A022: Numeric literal value exceeds the valid range for the target type.
//!
//! For example, `let x: u8 = 256` or `let y: i8 = 200`.
//! Uses type information from the typed context to determine the target type
//! and validate the literal value fits within its range.

use inference_ast::ids::{ExprId, NodeId};
use inference_ast::nodes::{Expr, Stmt};
use inference_type_checker::type_info::{NumberType, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;

use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// Numeric literal value must be within the range of its target type.
    #[id = "A022"]
    #[name = "Literal out of range"]
    #[severity = error]
    pub struct LiteralOutOfRange;
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
        Stmt::ConstDef(def_id) => {
            if let inference_ast::nodes::Def::Constant { value, .. } = &ctx.arena()[*def_id].kind {
                check_expr(ctx, *value, errors);
            }
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
    expr_id: ExprId,
    errors: &mut Vec<AnalysisDiagnostic>,
) {
    let arena = ctx.arena();
    match &arena[expr_id].kind {
        Expr::NumberLiteral { value } => {
            if let Some(ti) = ctx.get_node_typeinfo(NodeId::Expr(expr_id)) {
                validate_literal_range(value, &ti.kind, arena[expr_id].location, errors);
            }
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
        | Expr::BoolLiteral { .. }
        | Expr::StringLiteral { .. }
        | Expr::UnitLiteral
        | Expr::Uzumaki
        | Expr::Type(_) => {}
    }
}

fn validate_literal_range(
    value: &str,
    target_kind: &TypeInfoKind,
    location: inference_ast::nodes::Location,
    errors: &mut Vec<AnalysisDiagnostic>,
) {
    let TypeInfoKind::Number(number_type) = target_kind else {
        return;
    };
    let (type_name, min, max): (&str, i128, i128) = match number_type {
        NumberType::I8 => ("i8", i128::from(i8::MIN), i128::from(i8::MAX)),
        NumberType::I16 => ("i16", i128::from(i16::MIN), i128::from(i16::MAX)),
        NumberType::I32 => ("i32", i128::from(i32::MIN), i128::from(i32::MAX)),
        NumberType::I64 => ("i64", i128::from(i64::MIN), i128::from(i64::MAX)),
        NumberType::U8 => ("u8", i128::from(u8::MIN), i128::from(u8::MAX)),
        NumberType::U16 => ("u16", i128::from(u16::MIN), i128::from(u16::MAX)),
        NumberType::U32 => ("u32", i128::from(u32::MIN), i128::from(u32::MAX)),
        NumberType::U64 => ("u64", i128::from(u64::MIN), i128::from(u64::MAX)),
    };
    let out_of_range = match value.parse::<i128>() {
        Ok(parsed) => parsed < min || parsed > max,
        Err(_) => true,
    };
    if out_of_range {
        errors.push(AnalysisDiagnostic::LiteralOutOfRange {
            value: value.to_string(),
            type_name: type_name.to_string(),
            min,
            max,
            location,
        });
    }
}
