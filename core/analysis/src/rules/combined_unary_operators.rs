//! A033: Combined unary operators are prohibited.
//!
//! Rejects expressions in which a prefix unary operator (`!`, `-`, `~`)
//! is applied to another prefix unary operator, including parenthesized
//! variants and the special case where the grammar lexes a leading `-`
//! into a number literal token:
//!
//! ```text
//! --x      // prohibited
//! -~x      // prohibited
//! !!x      // prohibited
//! -(~x)    // prohibited
//! ~(-(x))  // prohibited
//! --42     // prohibited (parser lexes `-42` as one NumberLiteral token,
//!          //             so this lands as PrefixUnary(Neg, NumberLiteral("-42")))
//! -42      // allowed (single negation of a literal; no outer unary)
//! ```
//!
//! Every `PrefixUnary` node is examined; the rule fires when its operand
//! (with any `Parenthesized` wrappers peeled off) is either itself a
//! `PrefixUnary`, or a `NumberLiteral` whose textual value starts with
//! `-` — the second case represents the grammar's eager negative-literal
//! lexing of `--N` / `-~N` constructs.

use inference_ast::arena::AstArena;
use inference_ast::ids::ExprId;
use inference_ast::nodes::{Expr, UnaryOperatorKind};

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// Combined unary operators are prohibited.
    #[id = "A033"]
    #[name = "Combined unary operators"]
    #[severity = error]
    pub struct CombinedUnaryOperators;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            let module_path = walk_ctx.module_path.clone();
            walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
                walker::walk_expr(arena, expr_id, &mut |sub_id| {
                    check_prefix_unary(arena, &module_path, sub_id, &mut errors);
                });
            });
        });
        errors
    }
}

fn check_prefix_unary(
    arena: &AstArena,
    module_path: &[String],
    expr_id: ExprId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    let Expr::PrefixUnary { expr, op } = &arena[expr_id].kind else {
        return;
    };
    if let Some(inner_op) = inner_unary_op(arena, *expr) {
        errors.push(LabeledDiagnostic::new(
            module_path.to_vec(),
            AnalysisDiagnostic::CombinedUnaryOperators {
                op_outer: op_glyph(op),
                op_inner: op_glyph(&inner_op),
                location: arena[expr_id].location,
            },
        ));
    }
}

fn inner_unary_op(arena: &AstArena, expr_id: ExprId) -> Option<UnaryOperatorKind> {
    match &arena[expr_id].kind {
        Expr::PrefixUnary { op, .. } => Some(op.clone()),
        Expr::Parenthesized { expr } => inner_unary_op(arena, *expr),
        Expr::NumberLiteral { value } if value.starts_with('-') => Some(UnaryOperatorKind::Neg),
        _ => None,
    }
}

fn op_glyph(op: &UnaryOperatorKind) -> &'static str {
    match op {
        UnaryOperatorKind::Not => "!",
        UnaryOperatorKind::Neg => "-",
        UnaryOperatorKind::BitNot => "~",
    }
}
