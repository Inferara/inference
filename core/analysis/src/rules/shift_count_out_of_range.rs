//! A044: Shift count is a statically-known literal outside the valid range.
//!
//! Rejects a shift (`<<` or `>>`) whose count operand is a literal — negative or
//! greater than or equal to the operand type's bit width — so the shift never
//! means what it says: `x << 32` or `x >> -1` on an `i32`. This complements the
//! runtime rule that a shift count is taken modulo the operand type's bit width;
//! a literal that lands outside `0..width` is a program error, not a value to
//! silently fold.
//!
//! Like the division-by-zero check and A022, the scope is a statically-known
//! literal only: const-declared counts (`const K: i32 = 33; x << K`) reach here
//! as opaque identifiers and are not detected. A count wrapped in parentheses or
//! written as a negated literal (`x << (33)`, `x >> -1`) is still resolved.
//!
//! Reachability: a literal count currently type-checks only for `i32`-typed
//! shifts — bare literals are `i32` and binary operands do not coerce — but the
//! check reads the operand width from the type, so it extends automatically if a
//! narrow-typed literal count ever becomes expressible. Unparseable or
//! out-of-`i128`-range literals are left to A022, which owns that diagnostic;
//! this rule skips them to avoid double-reporting.

use inference_ast::arena::AstArena;
use inference_ast::ids::{ExprId, NodeId};
use inference_ast::nodes::{Expr, OperatorKind, UnaryOperatorKind};
use inference_type_checker::type_info::{NumberType, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// A shift count that is a statically-known literal must be within
    /// `0..width` for the operand type.
    #[id = "A044"]
    #[name = "Shift count out of range"]
    #[severity = error]
    pub struct ShiftCountOutOfRange;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            let module_path = walk_ctx.module_path.clone();
            walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
                walker::walk_expr(arena, expr_id, &mut |sub_id| {
                    check_shift_count(ctx, &module_path, sub_id, &mut errors);
                });
            });
        });
        errors
    }
}

fn check_shift_count(
    ctx: &TypedContext,
    module_path: &[String],
    expr_id: ExprId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    let arena = ctx.arena();
    let Expr::Binary { left, right, op } = &arena[expr_id].kind else {
        return;
    };
    if !matches!(op, OperatorKind::Shl | OperatorKind::Shr) {
        return;
    }
    let Some(count) = literal_shift_count(arena, *right) else {
        return;
    };
    let Some(ti) = ctx.get_node_typeinfo(NodeId::Expr(*left)) else {
        return;
    };
    let TypeInfoKind::Number(number_type) = &ti.kind else {
        return;
    };
    let (type_name, width) = name_and_bit_width(*number_type);
    if count < 0 || count >= i128::from(width) {
        errors.push(LabeledDiagnostic::new(
            module_path.to_vec(),
            AnalysisDiagnostic::ShiftCountOutOfRange {
                value: count.to_string(),
                type_name: type_name.to_string(),
                max: width - 1,
                location: arena[*right].location,
            },
        ));
    }
}

/// Resolves the shift count operand to its literal value, if it is one.
///
/// Strips any number of parenthesization layers, then accepts a bare number
/// literal (a non-negative count) or a `Neg`-prefixed number literal (a negative
/// count). Returns `None` for dynamic counts and for literals that do not parse
/// as `i128` (A022 owns those).
fn literal_shift_count(arena: &AstArena, right: ExprId) -> Option<i128> {
    match &arena[strip_parens(arena, right)].kind {
        Expr::NumberLiteral { value } => value.parse::<i128>().ok(),
        Expr::PrefixUnary {
            expr,
            op: UnaryOperatorKind::Neg,
        } => match &arena[strip_parens(arena, *expr)].kind {
            Expr::NumberLiteral { value } => value.parse::<i128>().ok().map(|v| -v),
            _ => None,
        },
        _ => None,
    }
}

fn strip_parens(arena: &AstArena, mut expr: ExprId) -> ExprId {
    while let Expr::Parenthesized { expr: inner } = &arena[expr].kind {
        expr = *inner;
    }
    expr
}

fn name_and_bit_width(number_type: NumberType) -> (&'static str, u32) {
    match number_type {
        NumberType::I8 => ("i8", 8),
        NumberType::U8 => ("u8", 8),
        NumberType::I16 => ("i16", 16),
        NumberType::U16 => ("u16", 16),
        NumberType::I32 => ("i32", 32),
        NumberType::U32 => ("u32", 32),
        NumberType::I64 => ("i64", 64),
        NumberType::U64 => ("u64", 64),
    }
}
