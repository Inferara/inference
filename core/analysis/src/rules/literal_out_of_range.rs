//! A022: Numeric literal value exceeds the valid range for the target type.
//!
//! For example, `let x: u8 = 256` or `let y: i8 = 200`.
//! Uses type information from the typed context to determine the target type
//! and validate the literal value fits within its range.
//!
//! A literal takes its type from the position it appears in, which is usually
//! not where the literal is written — a `u8` parameter three lines up is enough
//! to put a literal out of range. The diagnostic therefore also carries the
//! position that supplied the type, so the report says why this literal is being
//! measured against this type.

use inference_ast::ids::{ExprId, NodeId};
use inference_ast::nodes::Expr;
use inference_type_checker::errors::TypeMismatchContext;
use inference_type_checker::type_info::{NumberType, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// Numeric literal value must be within the range of its target type.
    #[id = "A022"]
    #[name = "Literal out of range"]
    #[severity = error]
    pub struct LiteralOutOfRange;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            let module_path = walk_ctx.module_path.clone();
            walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
                walker::walk_expr(arena, expr_id, &mut |sub_id| {
                    check_number_literal(ctx, &module_path, sub_id, &mut errors);
                });
            });
        });
        errors
    }
}

fn check_number_literal(
    ctx: &TypedContext,
    module_path: &[String],
    expr_id: ExprId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    let arena = ctx.arena();
    if let Expr::NumberLiteral { value } = &arena[expr_id].kind
        && let Some(ti) = ctx.get_node_typeinfo(NodeId::Expr(expr_id))
    {
        validate_literal_range(
            value,
            module_path,
            &ti.kind,
            ctx.literal_type_source(expr_id),
            arena[expr_id].location,
            errors,
        );
    }
}

fn validate_literal_range(
    value: &str,
    module_path: &[String],
    target_kind: &TypeInfoKind,
    type_source: Option<&TypeMismatchContext>,
    location: inference_ast::nodes::Location,
    errors: &mut Vec<LabeledDiagnostic>,
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
        errors.push(LabeledDiagnostic::new(
            module_path.to_vec(),
            AnalysisDiagnostic::LiteralOutOfRange {
                value: value.to_string(),
                type_name: type_name.to_string(),
                min,
                max,
                type_source: type_source.cloned(),
                location,
            },
        ));
    }
}
