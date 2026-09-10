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
//!
//! ## Rule ownership: the separated sign belongs to A046
//!
//! A literal is measured exactly as written, un-negated, so a minus separated
//! from its digits used to make this rule speak about a value nobody wrote: at
//! `i8`, `- 128` reported "literal 128 is out of range" — true of `128` and
//! false of the `-128` the author meant. A046 rejects that spelling outright, so
//! this rule steps aside for it, skipping every literal
//! `walker::separated_negated_literal` identifies. Both rules read the shared
//! predicate rather than restating it, so neither can start flagging a shape the
//! other has stopped covering.
//!
//! The handoff accepts nothing: a magnitude that fits no type in either sign
//! (`- 300` at `i8`) is still an error, reported by A046. Once the spelling is
//! fixed to `-300` the literal carries its sign, arrives here as a single token,
//! and is measured as the negative number it is. Parenthesized negation
//! (`-(128)`) is not part of the handoff — A046 does not claim it and this rule
//! still measures `128`.
//!
//! ## Rule ownership: the arithmetic around the literal belongs to A052
//!
//! A052 rejects an operation whose operands fold to constants and whose result
//! leaves the type it is performed at. It folds nothing containing a literal
//! this rule reports, so `let x: u8 = 300 + 1;` is one finding about one
//! literal rather than two about the same mistake. The condition it hands over
//! for is [`walker::literal_leaves_range`], which this rule reads as its own
//! verdict, so the rule that steps aside cannot start stepping aside for a shape
//! this one has stopped covering.

use inference_ast::ids::{ExprId, NodeId};
use inference_ast::nodes::Expr;
use inference_type_checker::errors::TypeMismatchContext;
use inference_type_checker::type_info::TypeInfoKind;
use inference_type_checker::typed_context::TypedContext;
use rustc_hash::FxHashSet;

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
                // Collected in a pass of its own rather than while checking, so
                // that skipping does not depend on a parent being visited before
                // its operand.
                let mut owned_by_a046 = FxHashSet::default();
                walker::walk_expr(arena, expr_id, &mut |sub_id| {
                    if let Some(literal_id) = walker::separated_negated_literal(arena, sub_id) {
                        owned_by_a046.insert(literal_id);
                    }
                });
                walker::walk_expr(arena, expr_id, &mut |sub_id| {
                    if !owned_by_a046.contains(&sub_id) {
                        check_number_literal(ctx, &module_path, sub_id, &mut errors);
                    }
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
    let range = number_type.range();
    if walker::literal_leaves_range(value, *number_type) {
        errors.push(LabeledDiagnostic::new(
            module_path.to_vec(),
            AnalysisDiagnostic::LiteralOutOfRange {
                value: value.to_string(),
                type_name: number_type.as_str().to_string(),
                min: *range.start(),
                max: *range.end(),
                type_source: type_source.cloned(),
                location,
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use inference_ast::nodes::Location;
    use inference_type_checker::type_info::{NumberType, TypeInfoKind};

    use super::validate_literal_range;

    /// This rule measures a literal against [`NumberType::range`] and nothing of
    /// its own.
    ///
    /// It once carried its own copy of the eight widths' bounds, and the
    /// specification translator carried a second. The bounds decide whether a
    /// program compiles, so a width read one way here and another way there
    /// would let the same literal be an error in one pass and a value in the
    /// next. The boundary is walked at every width — the last value inside the
    /// range and the first outside it at each end — because a copy that differed
    /// by one is exactly what a spot check misses, and the reported bounds are
    /// compared too, since they are what the message tells the author.
    #[test]
    fn the_accepted_literals_are_exactly_the_range() {
        for number in NumberType::ALL {
            let range = number.range();
            let kind = TypeInfoKind::Number(*number);
            let verdict = |value: i128| {
                let mut errors = Vec::new();
                validate_literal_range(
                    &value.to_string(),
                    &[],
                    &kind,
                    None,
                    Location::default(),
                    &mut errors,
                );
                errors
            };
            let name = number.as_str();
            assert!(verdict(*range.start()).is_empty(), "`{name}` minimum");
            assert!(verdict(*range.end()).is_empty(), "`{name}` maximum");
            assert!(
                !verdict(*range.start() - 1).is_empty(),
                "`{name}` below its minimum"
            );
            let above = verdict(*range.end() + 1);
            assert_eq!(above.len(), 1, "`{name}` above its maximum");
            let rendered = above[0].diagnostic.to_string();
            assert!(
                rendered.contains(&format!(
                    "out of range for type `{name}` (valid range: {}..={})",
                    range.start(),
                    range.end()
                )),
                "the reported bounds are the range's own: {rendered}"
            );
        }
    }
}
