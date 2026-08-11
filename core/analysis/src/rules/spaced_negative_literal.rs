//! A046: A unary minus applied to a numeric literal must be written glued to
//! the digits.
//!
//! `-128` is one token. The lexer folds a `-` into the digits that follow it, so
//! the glued spelling produces a `NumberLiteral` whose text carries the sign and
//! whose value is checked, ranged, and lowered as the negative number the author
//! wrote. Put a space in and nothing of the sort happens: `- 128` is a `Neg`
//! expression over the *bare* literal `128`, and every rule downstream measures
//! `128`, not `-128`.
//!
//! That is why the same value used to compile or fail depending on the
//! whitespace. At `i8`, `- 100` was accepted (`100` fits) while `- 128` was
//! rejected with "literal 128 is out of range for type i8" — a diagnostic
//! pointing at a value the author never wrote, about a limit `-128` does not
//! exceed. Every signed minimum was unreachable in that spelling and only in
//! that spelling. A rule whose outcome turns on a space is not one a reader can
//! hold, so rather than teach the range check to look through a negation, this
//! rule removes the second spelling: there is one way to write a negative
//! literal, and the sign belongs to it.
//!
//! This follows A033, which prohibits combined unary operators for the same
//! reason — a spelling that is legal but harder to read than its alternative
//! earns nothing, and Inference is a language whose programs are meant to be
//! proved.
//!
//! ## What stays legal
//!
//! - `-128`, the canonical spelling, in every position.
//! - `- x`, and a spaced minus over any operand that is not a literal. Negation
//!   of a *value* is an operator applied to an expression; there is no token to
//!   glue the sign to and no second spelling to choose between.
//! - `a - 1` and `a-1`, which are binary subtraction. A binary minus has a left
//!   operand, so it is never a `PrefixUnary` and is never seen by this rule.
//!
//! ## Rule ownership
//!
//! The predicate lives in `walker::separated_negated_literal` and is shared
//! with A022 (`literal_out_of_range`), which skips exactly the literals this
//! rule claims. Without that handoff A022 would keep reporting `128` as out of
//! range for `i8` on a program whose author meant `-128` — advice that is
//! actively wrong for the construct. Nothing is silently accepted by the
//! handoff: every literal A022 stops measuring is one this rule rejects, so a
//! magnitude that fits no type at all (`- 300` at `i8`) is still an error, just
//! a different one. The author fixes the spelling first; the glued literal is
//! then range-checked as the negative number it is.
//!
//! ## Documented non-scope
//!
//! - `~ 5` and `! x`. A complement or a logical negation is not part of any
//!   literal's spelling — there is no glued form of `~5` that lexes differently
//!   from the spaced one — so no whitespace-dependent second spelling exists to
//!   remove. Only `-` is folded by the lexer, and only `-` is checked here.
//! - `-(128)`, whose operand is a parenthesized expression rather than a
//!   literal. Behaviour there is unchanged: the parentheses are a deliberate
//!   grouping the author wrote, `-(128)` cannot be closed up into a token, and
//!   A022 still measures `128` against the target type. Peeling them would make
//!   this rule demand a rewrite that the syntax does not offer.
//! - `--42` and `- -42`, which A033 owns. See
//!   `walker::separated_negated_literal` for why a literal carrying its own
//!   sign is excluded here rather than reported twice.
//! - A module-scope `const` initializer, which no function-body walk reaches.
//!   The coverage is deliberately identical to A022's, since the two rules
//!   partition one construct between them: a literal A022 cannot measure is not
//!   one this rule needs to claim. Nothing escapes today either way — A032
//!   rejects every module-scope `const` as not yet implemented — but the day
//!   that feature lands, both rules must gain the position together.

use inference_ast::arena::AstArena;
use inference_ast::ids::ExprId;
use inference_ast::nodes::Expr;

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// A unary minus applied to a numeric literal must be written glued to the
    /// digits.
    #[id = "A046"]
    #[name = "Spaced negative literal"]
    #[severity = error]
    pub struct SpacedNegativeLiteral;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            let module_path = walk_ctx.module_path.clone();
            walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
                walker::walk_expr(arena, expr_id, &mut |sub_id| {
                    check_negation(arena, &module_path, sub_id, &mut errors);
                });
            });
        });
        errors
    }
}

/// Reports `expr_id` when it is a minus written apart from the literal it
/// negates. The reported location is the `PrefixUnary` node, which starts at the
/// minus, so the report opens on the character the fix deletes the gap after.
fn check_negation(
    arena: &AstArena,
    module_path: &[String],
    expr_id: ExprId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    if let Some(literal_id) = walker::separated_negated_literal(arena, expr_id)
        && let Expr::NumberLiteral { value } = &arena[literal_id].kind
    {
        errors.push(LabeledDiagnostic::new(
            module_path.to_vec(),
            AnalysisDiagnostic::SpacedNegativeLiteral {
                value: value.clone(),
                location: arena[expr_id].location,
            },
        ));
    }
}
