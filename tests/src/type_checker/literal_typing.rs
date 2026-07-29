//! Contextual typing of integer literals.
//!
//! An integer literal has no intrinsic type: it denotes whatever integer type
//! the position it appears in asks for, and only falls back to `i32` when
//! nothing is asked of it. An expected type arises at an annotated `let`/`const`
//! initializer, an assignment right-hand side, a struct-literal field value, an
//! array-literal element, a call argument and a `return` operand; it descends
//! unchanged through parentheses, `-` and `~`, and into both operands of an
//! arithmetic, bitwise or shift operator whose operands are themselves built
//! only from literals. Where an operator has one literal-built operand and one
//! typed one, the literal-built side is typed from its peer.
//!
//! These tests pin all three halves: the positions that fix a literal's type,
//! the positions that deliberately do not, and the programs that must keep
//! being rejected with the diagnostics they produce today.

use crate::utils::build_ast;
use inference_ast::ids::{ExprId, NodeId};
use inference_ast::nodes::OperatorKind;
use inference_ast::nodes::{Def, Expr};
use inference_type_checker::errors::TypeMismatchContext;
use inference_type_checker::type_info::{TypeInfo, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;
use inference_type_checker::{TypeCheckerBuilder, check_with_diagnostics};

fn try_type_check(source: &str) -> anyhow::Result<TypedContext> {
    let arena = build_ast(source.to_string());
    Ok(TypeCheckerBuilder::build_typed_context(arena)?.typed_context())
}

/// Type-checks `source` through the lossless entry point and renders every
/// diagnostic, so a test can assert on both the count and the text.
fn diagnostics(source: &str) -> Vec<String> {
    let arena = build_ast(source.to_string());
    check_with_diagnostics(arena)
        .errors
        .into_iter()
        .map(|d| d.error.to_string())
        .collect()
}

/// Type-checks `source` through the lossless entry point, keeping the recorded
/// types even when the program is rejected.
fn typed_context(source: &str) -> TypedContext {
    let arena = build_ast(source.to_string());
    check_with_diagnostics(arena).typed_context
}

/// The one integer literal spelled `value`. The spelling must be unique in
/// `source` — an ambiguous needle fails the test rather than silently picking
/// one.
fn literal_expr(ctx: &TypedContext, source: &str, value: &str) -> ExprId {
    let matches: Vec<ExprId> = ctx
        .arena()
        .exprs
        .iter()
        .filter_map(|(id, data)| match &data.kind {
            Expr::NumberLiteral { value: spelled } if spelled == value => Some(id),
            _ => None,
        })
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "`{value}` should appear exactly once in `{source}`"
    );
    matches[0]
}

/// The type recorded for the one integer literal spelled `value`.
///
/// Acceptance alone cannot distinguish "the literal took the annotated type"
/// from "the position never looked"; the recorded type is what codegen and the
/// range rules read, so it is what these tests assert on.
fn literal_type(source: &str, value: &str) -> TypeInfo {
    let ctx = typed_context(source);
    let expr_id = literal_expr(&ctx, source, value);
    ctx.get_node_typeinfo(NodeId::Expr(expr_id))
        .unwrap_or_else(|| panic!("`{value}` should carry a recorded type in `{source}`"))
}

/// The position recorded as having given the one literal spelled `value` its
/// type, or `None` when the literal kept the `i32` default.
fn literal_source(source: &str, value: &str) -> Option<TypeMismatchContext> {
    let ctx = typed_context(source);
    literal_source_in(&ctx, source, value)
}

/// [`literal_source`] against a context the caller already built — the form
/// multi-file tests need, since their arena comes from several sources.
fn literal_source_in(ctx: &TypedContext, source: &str, value: &str) -> Option<TypeMismatchContext> {
    let expr_id = literal_expr(ctx, source, value);
    ctx.literal_type_source(expr_id).cloned()
}

/// Asserts the one literal spelled `value` kept the `i32` default and was
/// recorded as coming from nowhere.
///
/// Both halves matter: a source with no type would be an orphan entry, and a
/// non-`i32` type with no source would mean a position typed the literal
/// without saying so. Asserting only the `None` would pass if the literal
/// stopped being reached at all.
fn assert_literal_unattributed(source: &str, value: &str) {
    assert_eq!(
        literal_source(source, value),
        None,
        "`{value}` in `{source}` should have no recorded position"
    );
    assert_eq!(
        literal_type(source, value).to_string(),
        "i32",
        "`{value}` in `{source}` should have kept the default type"
    );
}

/// Asserts the one literal spelled `value` was typed `expected` (`"i64"`,
/// `"u8"`, …).
fn assert_literal_typed(source: &str, value: &str, expected: &str) {
    let recorded = literal_type(source, value);
    assert_eq!(
        recorded.to_string(),
        expected,
        "`{value}` in `{source}` should be typed `{expected}`"
    );
}

/// Asserts `source` is accepted.
fn assert_accepted(source: &str) {
    let result = try_type_check(source);
    assert!(
        result.is_ok(),
        "expected `{source}` to type-check, got: {:?}",
        result.err()
    );
}

/// Asserts `source` is rejected and returns the joined diagnostic text.
fn rejection(source: &str) -> String {
    match try_type_check(source) {
        Ok(_) => panic!("expected `{source}` to be rejected"),
        Err(error) => error.to_string(),
    }
}

/// A literal whose target type is not numeric produces exactly one diagnostic.
/// The `let`, assignment and array-target positions used to report the same
/// mismatch twice — once eagerly before inference and once from the ordinary
/// post-inference check.
mod one_diagnostic_per_mismatch {
    use super::*;

    #[test]
    fn let_with_bool_target() {
        let errors = diagnostics("pub fn f() { let x: bool = 5; }");
        assert_eq!(errors.len(), 1, "expected one diagnostic, got: {errors:?}");
        assert!(
            errors[0]
                .contains("type mismatch in variable definition: expected `Bool`, found `i32`"),
            "unexpected diagnostic: {errors:?}"
        );
    }

    #[test]
    fn let_with_array_target() {
        let errors = diagnostics("pub fn f() { let a: [i64; 2] = 5; }");
        assert_eq!(errors.len(), 1, "expected one diagnostic, got: {errors:?}");
        assert!(
            errors[0]
                .contains("type mismatch in variable definition: expected `[i64; 2]`, found `i32`"),
            "unexpected diagnostic: {errors:?}"
        );
    }

    #[test]
    fn assignment_with_bool_target() {
        let errors = diagnostics("pub fn f() { let mut b: bool = true; b = 5; }");
        assert_eq!(errors.len(), 1, "expected one diagnostic, got: {errors:?}");
        assert!(
            errors[0].contains("type mismatch in assignment: expected `Bool`, found `i32`"),
            "unexpected diagnostic: {errors:?}"
        );
    }

    #[test]
    fn top_level_const_with_bool_target() {
        let errors = diagnostics("const X: bool = 42; pub fn f() -> i32 { return 0; }");
        assert_eq!(errors.len(), 1, "expected one diagnostic, got: {errors:?}");
        assert!(
            errors[0]
                .contains("type mismatch in variable definition: expected `Bool`, found `i32`"),
            "unexpected diagnostic: {errors:?}"
        );
    }

    #[test]
    fn local_const_with_bool_target() {
        let errors = diagnostics("pub fn f() -> i32 { const C: bool = 42; return 0; }");
        assert_eq!(errors.len(), 1, "expected one diagnostic, got: {errors:?}");
        assert!(
            errors[0]
                .contains("type mismatch in variable definition: expected `Bool`, found `i32`"),
            "unexpected diagnostic: {errors:?}"
        );
    }

    #[test]
    fn struct_field_with_bool_target() {
        let errors = diagnostics(
            "struct P { b: bool; } pub fn f() -> bool { let p: P = P { b: 5 }; return p.b; }",
        );
        assert_eq!(errors.len(), 1, "expected one diagnostic, got: {errors:?}");
        assert!(
            errors[0]
                .contains("type mismatch in field `b` of struct `P`: expected `Bool`, found `i32`"),
            "unexpected diagnostic: {errors:?}"
        );
    }

    /// `return` is a position that supplies an expected type, and a
    /// non-numeric one refuses to type the literal exactly as every other
    /// position does — one diagnostic, from the ordinary return check.
    #[test]
    fn return_with_bool_target() {
        let errors = diagnostics("pub fn f() -> bool { return 5; }");
        assert_eq!(errors.len(), 1, "expected one diagnostic, got: {errors:?}");
        assert!(
            errors[0].contains("type mismatch in return statement: expected `Bool`, found `i32`"),
            "unexpected diagnostic: {errors:?}"
        );
    }

    /// An array annotation whose element type is not numeric refuses to type
    /// the elements, which then default to `i32` and are reported by the
    /// ordinary array mismatch. This used to stamp `bool` onto the literals
    /// unconditionally and crash codegen with "Unsupported number literal
    /// type: Bool".
    #[test]
    fn let_with_non_numeric_array_element_target() {
        let errors = diagnostics("pub fn f() { let a: [bool; 2] = [1, 2]; }");
        assert_eq!(errors.len(), 1, "expected one diagnostic, got: {errors:?}");
        assert!(
            errors[0].contains(
                "type mismatch in variable definition: expected `[Bool; 2]`, found `[i32; 2]`"
            ),
            "unexpected diagnostic: {errors:?}"
        );
        assert_literal_typed("pub fn f() { let a: [bool; 2] = [1, 3]; }", "3", "i32");
    }

    /// A struct element type was the worse half of the same bug. `bool` at least
    /// crashed code generation; a struct type was stamped onto the literals and
    /// the array then *matched* its own annotation, so the program was accepted
    /// and compiled as if two integers were two `Point`s. Both are now the same
    /// ordinary mismatch, reported once.
    #[test]
    fn let_with_struct_array_element_target() {
        let errors = diagnostics(
            "struct Point { x: i32; y: i32; } pub fn f() { let a: [Point; 2] = [1, 2]; }",
        );
        assert_eq!(errors.len(), 1, "expected one diagnostic, got: {errors:?}");
        assert!(
            errors[0].contains(
                "type mismatch in variable definition: expected `[Point; 2]`, found `[i32; 2]`"
            ),
            "unexpected diagnostic: {errors:?}"
        );
    }
}

/// The six shapes reported in the issue: every one of them needed an
/// `let n: i64 = …;` preamble before contextual typing.
mod issue_repros {
    use super::*;

    #[test]
    fn shift_by_a_literal_count() {
        let source = "pub fn f(a: i64) -> i64 { return a << 16; }";
        assert_accepted(source);
        assert_literal_typed(source, "16", "i64");
    }

    #[test]
    fn add_a_literal() {
        let source = "pub fn g(a: i64) -> i64 { return a + 65536; }";
        assert_accepted(source);
        assert_literal_typed(source, "65536", "i64");
    }

    #[test]
    fn compare_against_a_literal() {
        let source = "pub fn h(a: i64) -> i64 { if a < 65536 { return 1; } return 0; }";
        assert_accepted(source);
        assert_literal_typed(source, "65536", "i64");
    }

    #[test]
    fn pass_a_literal_argument() {
        let source = "fn id(x: i64, y: i64) -> i64 { return x + y; } \
                      pub fn k(a: i64) -> i64 { return id(a, 65536); }";
        assert_accepted(source);
        assert_literal_typed(source, "65536", "i64");
    }

    #[test]
    fn return_a_literal() {
        let source = "pub fn m() -> i64 { return 65536; }";
        assert_accepted(source);
        assert_literal_typed(source, "65536", "i64");
    }

    /// A glued `-42` lexes as one negative literal; only a spaced `- 42` is a
    /// prefix negation over a literal. Both must reach `i64`.
    #[test]
    fn return_a_glued_negative_literal() {
        let source = "pub fn n() -> i64 { return -42; }";
        assert_accepted(source);
        assert_literal_typed(source, "-42", "i64");
    }

    #[test]
    fn return_a_spaced_negation_of_a_literal() {
        let source = "pub fn n() -> i64 { return - 42; }";
        assert_accepted(source);
        assert_literal_typed(source, "42", "i64");
    }
}

/// The positions that supplied an expected type before this change keep
/// accepting a literal that does not fit `i32`'s default.
mod coercion_sites {
    use super::*;

    #[test]
    fn annotated_let() {
        assert_accepted("pub fn f() -> i64 { let x: i64 = 65536; return x; }");
    }

    #[test]
    fn annotated_let_unsigned() {
        assert_accepted("pub fn f() -> u64 { let x: u64 = 65536; return x; }");
    }

    #[test]
    fn annotated_let_narrow() {
        assert_accepted("pub fn f() -> u8 { let x: u8 = 200; return x; }");
    }

    #[test]
    fn assignment_to_declared_variable() {
        assert_accepted("pub fn f() -> i64 { let mut x: i64 = 0; x = 65536; return x; }");
    }

    #[test]
    fn assignment_to_struct_field() {
        let source = "struct P { x: i64; } \
                      pub fn f() -> i64 { let mut p: P = P { x: 0 }; p.x = 65536; return p.x; }";
        assert_accepted(source);
        assert_literal_typed(source, "65536", "i64");
    }

    #[test]
    fn assignment_to_array_element() {
        let source =
            "pub fn f() -> i64 { let mut a: [i64; 2] = [0, 0]; a[0] = 65536; return a[0]; }";
        assert_accepted(source);
        assert_literal_typed(source, "65536", "i64");
    }

    #[test]
    fn struct_literal_field() {
        assert_accepted(
            "struct P { x: i64; } pub fn f() -> i64 { let p: P = P { x: 65536 }; return p.x; }",
        );
    }

    #[test]
    fn array_literal_element_in_let() {
        assert_accepted("pub fn f() -> i64 { let a: [i64; 2] = [1, 2]; return a[0]; }");
    }

    #[test]
    fn top_level_const_scalar() {
        assert_accepted("const C: i64 = 65536; pub fn f() -> i64 { return C; }");
    }

    #[test]
    fn local_const_scalar() {
        assert_accepted("pub fn f() -> i64 { const C: i64 = 65536; return C; }");
    }
}

/// Parentheses, `-` and `~` group without changing what is expected, so a type
/// expected of them is expected of their operand — recursively.
mod transparent_descent {
    use super::*;

    #[test]
    fn parentheses_forward_the_expected_type() {
        let source = "pub fn f() -> i64 { let x: i64 = (65536); return x; }";
        assert_accepted(source);
        assert_literal_typed(source, "65536", "i64");
    }

    #[test]
    fn nested_parentheses_forward_the_expected_type() {
        let source = "pub fn f() -> i64 { let x: i64 = (((65536))); return x; }";
        assert_accepted(source);
        assert_literal_typed(source, "65536", "i64");
    }

    #[test]
    fn negation_forwards_the_expected_type() {
        let source = "pub fn f() -> i64 { let x: i64 = - 65536; return x; }";
        assert_accepted(source);
        assert_literal_typed(source, "65536", "i64");
    }

    #[test]
    fn complement_forwards_the_expected_type() {
        let source = "pub fn f() -> i64 { let x: i64 = ~ 65536; return x; }";
        assert_accepted(source);
        assert_literal_typed(source, "65536", "i64");
    }

    #[test]
    fn complement_forwards_an_unsigned_expected_type() {
        let source = "pub fn f() -> u64 { let x: u64 = ~ 0; return x; }";
        assert_accepted(source);
        assert_literal_typed(source, "0", "u64");
    }

    #[test]
    fn descent_composes_through_parentheses_negation_and_operators() {
        let source = "pub fn f() -> i64 { let x: i64 = -(65536 + (1 << 40)); return x; }";
        assert_accepted(source);
        assert_literal_typed(source, "65536", "i64");
        assert_literal_typed(source, "40", "i64");
    }

    /// The signedness check runs on what the operand came back as, so `-` under
    /// an unsigned expected type is rejected rather than silently accepted.
    #[test]
    fn negation_under_an_unsigned_expected_type_is_rejected() {
        let err = rejection("pub fn f() -> u64 { let x: u64 = - 5; return x; }");
        assert!(
            err.contains("operator `Neg`") && err.contains("signed integers"),
            "unexpected error: {err}"
        );
    }

    /// `!` takes a boolean operand, so it never carries an integer expectation
    /// down.
    #[test]
    fn logical_not_does_not_forward_the_expected_type() {
        let err = rejection("pub fn f() -> i64 { let x: i64 = !5; return x; }");
        assert!(
            err.contains("operator `Not`") && err.contains("booleans"),
            "unexpected error: {err}"
        );
        assert_literal_typed(
            "pub fn f() -> i64 { let x: i64 = !5; return x; }",
            "5",
            "i32",
        );
    }
}

/// An operator with one literal-built operand types that operand from its peer,
/// whatever the operator is.
mod peer_typing {
    use super::*;

    #[test]
    fn peer_on_the_right() {
        let source = "pub fn f(a: i64) -> i64 { return a * 3; }";
        assert_accepted(source);
        assert_literal_typed(source, "3", "i64");
    }

    #[test]
    fn peer_on_the_left() {
        let source = "pub fn f(a: i64) -> i64 { return 3 * a; }";
        assert_accepted(source);
        assert_literal_typed(source, "3", "i64");
    }

    #[test]
    fn peer_applies_to_a_narrow_width() {
        let source = "pub fn f(a: u8) -> u8 { return a + 1; }";
        assert_accepted(source);
        assert_literal_typed(source, "1", "u8");
    }

    #[test]
    fn peer_types_a_shift_count() {
        let source = "pub fn f(a: u64) -> u64 { return a >> 3; }";
        assert_accepted(source);
        assert_literal_typed(source, "3", "u64");
    }

    #[test]
    fn peer_types_a_comparison_operand() {
        let source = "pub fn f(a: i64) -> bool { return a >= 100; }";
        assert_accepted(source);
        assert_literal_typed(source, "100", "i64");
    }

    #[test]
    fn peer_types_an_equality_operand() {
        let source = "pub fn f(a: u32) -> bool { return 7 == a; }";
        assert_accepted(source);
        assert_literal_typed(source, "7", "u32");
    }

    /// The peer is a whole literal-built subexpression, not just a bare
    /// literal — `a + (1 << 3)` has no cliff where `a + 1` works.
    #[test]
    fn the_peer_side_may_be_a_compound_literal_expression() {
        let source = "pub fn f(a: i64) -> i64 { return a + (1 << 3); }";
        assert_accepted(source);
        assert_literal_typed(source, "1", "i64");
        assert_literal_typed(source, "3", "i64");
    }

    #[test]
    fn a_non_numeric_peer_leaves_the_literal_at_its_default() {
        let source = "pub fn f(b: bool) -> bool { return b == 1; }";
        let errors = diagnostics(source);
        assert!(
            errors.iter().any(|e| e.contains(
                "cannot apply operator `Eq` to operands of different types: `Bool` and `i32`"
            )),
            "unexpected diagnostics: {errors:?}"
        );
        assert_literal_typed(source, "1", "i32");
    }
}

/// When both operands are literal-built neither can inform the other, so the
/// type expected of the whole expression is what fixes them — but only for an
/// operator that yields its operands' own type.
mod both_operands_literal_built {
    use super::*;

    #[test]
    fn a_shift_under_an_expected_type_descends_into_both_operands() {
        let source = "pub fn f() -> i64 { return 1 << 40; }";
        assert_accepted(source);
        assert_literal_typed(source, "1", "i64");
        assert_literal_typed(source, "40", "i64");
    }

    #[test]
    fn arithmetic_under_an_expected_type_descends_into_both_operands() {
        let source = "pub fn f() -> u64 { let x: u64 = 4294967296 + 1; return x; }";
        assert_accepted(source);
        assert_literal_typed(source, "4294967296", "u64");
        assert_literal_typed(source, "1", "u64");
    }

    #[test]
    fn descent_reaches_every_leaf_of_a_nested_literal_expression() {
        let source = "pub fn f() -> i64 { let x: i64 = (1 << 32) | (3 & 7); return x; }";
        assert_accepted(source);
        for leaf in ["1", "32", "3", "7"] {
            assert_literal_typed(source, leaf, "i64");
        }
    }

    /// A comparison yields `bool` whatever its operands are, so a type expected
    /// of it says nothing about them: both literals keep the `i32` default.
    #[test]
    fn a_comparison_of_two_literals_defaults_to_i32() {
        let source = "pub fn f() -> bool { return 1 == 2; }";
        assert_accepted(source);
        assert_literal_typed(source, "1", "i32");
        assert_literal_typed(source, "2", "i32");
    }

    #[test]
    fn logical_operands_never_receive_an_expected_type() {
        let source = "pub fn f() -> bool { let b: bool = (1 < 2) && (3 < 4); return b; }";
        assert_accepted(source);
        for leaf in ["1", "2", "3", "4"] {
            assert_literal_typed(source, leaf, "i32");
        }
    }

    /// With nothing expected of it, a literal-built expression is still `i32`.
    #[test]
    fn without_an_expected_type_a_literal_expression_defaults_to_i32() {
        let source = "pub fn f() -> i32 { let x: i32 = 1 + 2; return x; }";
        assert_accepted(source);
        assert_literal_typed(source, "1", "i32");
    }
}

/// `[T; N]` expected of an array literal is `T` expected of each element,
/// recursively and in every position that supplies an expected type.
mod array_literals {
    use super::*;

    #[test]
    fn elements_in_an_assignment() {
        let source =
            "pub fn f() -> i64 { let mut a: [i64; 2] = [0, 0]; a = [11, 22]; return a[0]; }";
        assert_accepted(source);
        assert_literal_typed(source, "11", "i64");
        assert_literal_typed(source, "22", "i64");
    }

    #[test]
    fn elements_in_a_top_level_const() {
        let source = "const A: [i64; 2] = [11, 22]; pub fn f() -> i64 { return A[0]; }";
        assert_accepted(source);
        assert_literal_typed(source, "11", "i64");
    }

    #[test]
    fn elements_in_a_local_const() {
        let source = "pub fn f() -> i64 { const A: [i64; 2] = [11, 22]; return A[1]; }";
        assert_accepted(source);
        assert_literal_typed(source, "22", "i64");
    }

    #[test]
    fn nested_array_elements() {
        let source =
            "pub fn f() -> i64 { let a: [[i64; 2]; 2] = [[11, 22], [33, 44]]; return a[1][0]; }";
        assert_accepted(source);
        for leaf in ["11", "22", "33", "44"] {
            assert_literal_typed(source, leaf, "i64");
        }
    }

    #[test]
    fn nested_array_elements_in_a_const() {
        let source = "const A: [[u64; 2]; 2] = [[11, 22], [33, 44]]; \
                      pub fn f() -> u64 { return A[0][1]; }";
        assert_accepted(source);
        assert_literal_typed(source, "22", "u64");
    }

    #[test]
    fn an_element_may_be_a_literal_expression() {
        let source = "pub fn f() -> i64 { let a: [i64; 2] = [1 << 40, 3]; return a[0]; }";
        assert_accepted(source);
        assert_literal_typed(source, "40", "i64");
    }

    #[test]
    fn an_element_may_be_peer_typed_against_a_variable() {
        let source = "pub fn f(v: i64) -> i64 { let a: [i64; 2] = [v + 1, 2]; return a[0]; }";
        assert_accepted(source);
        assert_literal_typed(source, "1", "i64");
    }

    #[test]
    fn a_struct_field_holding_an_array_types_its_elements() {
        let source = "struct H { a: [i64; 2]; } \
                      pub fn f() -> i64 { let h: H = H { a: [1, 2] }; return h.a[0]; }";
        assert_accepted(source);
        assert_literal_typed(source, "1", "i64");
    }

    #[test]
    fn an_array_literal_returned_directly_types_its_elements() {
        let source = "pub fn f() -> [i64; 2] { return [1, 2]; }";
        assert_accepted(source);
        assert_literal_typed(source, "1", "i64");
    }

    /// The elements still take the declared element type when the literal has
    /// the wrong number of them, so the two diagnostics stay about the size and
    /// the array shape — not about the elements.
    #[test]
    fn a_size_mismatch_leaves_the_elements_typed() {
        let source = "pub fn f() { let a: [i64; 3] = [11, 22]; }";
        let errors = diagnostics(source);
        assert_eq!(errors.len(), 2, "expected two diagnostics, got: {errors:?}");
        assert!(
            errors[0].contains("array literal has 2 elements but the declared type expects 3"),
            "unexpected diagnostic: {errors:?}"
        );
        assert!(
            errors[1].contains(
                "type mismatch in variable definition: expected `[i64; 3]`, found `[i64; 2]`"
            ),
            "unexpected diagnostic: {errors:?}"
        );
        assert_literal_typed(source, "11", "i64");
    }
}

/// Each argument is checked against its parameter's declared type, so a bare
/// literal argument denotes that type.
mod call_arguments {
    use super::*;

    #[test]
    fn free_function_argument() {
        let source = "fn g(v: i64) -> i64 { return v; } pub fn f() -> i64 { return g(65536); }";
        assert_accepted(source);
        assert_literal_typed(source, "65536", "i64");
    }

    #[test]
    fn free_function_argument_that_is_a_literal_expression() {
        let source =
            "fn g(v: i64) -> i64 { return v; } pub fn f() -> i64 { return g((1 << 40) + 1); }";
        assert_accepted(source);
        assert_literal_typed(source, "40", "i64");
    }

    #[test]
    fn method_argument() {
        let source = "struct P { x: i64; fn add(self, v: i64) -> i64 { return self.x + v; } } \
                      pub fn f() -> i64 { let p: P = P { x: 1 }; return p.add(65536); }";
        assert_accepted(source);
        assert_literal_typed(source, "65536", "i64");
    }

    #[test]
    fn associated_function_argument() {
        let source = "struct P { x: i64; fn make(v: i64) -> P { return P { x: v }; } } \
                      pub fn f() -> i64 { let p: P = P::make(65536); return p.x; }";
        assert_accepted(source);
        assert_literal_typed(source, "65536", "i64");
    }

    /// The largest `u64` value has no `i32` reading at all, so accepting it in
    /// argument position is only possible once the parameter type reaches it.
    #[test]
    fn the_largest_u64_value_as_an_argument() {
        let source = "fn g(v: u64) -> u64 { return v; } \
                      pub fn f() -> u64 { return g(18446744073709551615); }";
        assert_accepted(source);
        assert_literal_typed(source, "18446744073709551615", "u64");
    }

    #[test]
    fn an_argument_of_the_wrong_kind_is_still_rejected() {
        let err =
            rejection("fn g(v: bool) -> bool { return v; } pub fn f() -> bool { return g(5); }");
        assert!(
            err.contains("expected `Bool`, found `i32`"),
            "unexpected error: {err}"
        );
    }
}

/// Positions the normative rule deliberately leaves alone.
mod stop_list {
    use super::*;

    /// An array index is not an expected-type position: the index literal keeps
    /// the `i32` default even when the element type is `i64`.
    #[test]
    fn an_array_index_keeps_the_i32_default() {
        let source = "pub fn f() -> i64 { let a: [i64; 2] = [0, 0]; return a[1]; }";
        assert_accepted(source);
        assert_literal_typed(source, "1", "i32");
    }

    #[test]
    fn an_index_variable_of_another_width_is_unaffected() {
        assert_accepted(
            "pub fn f() -> i64 { let a: [i64; 2] = [0, 0]; let i: i32 = 1; return a[i]; }",
        );
    }

    /// A comparison's operands are peer-typed, but its `bool` result never
    /// becomes an integer.
    #[test]
    fn a_comparison_result_is_bool_whatever_is_expected() {
        let err = rejection("pub fn f(a: i64) -> i64 { let x: i64 = a < 65536; return x; }");
        assert!(
            err.contains("type mismatch in variable definition: expected `i64`, found `Bool`"),
            "unexpected error: {err}"
        );
    }

    /// Two expressions that already have types never combine, and the message
    /// is the one it has always been.
    #[test]
    fn two_typed_variables_of_different_widths_still_mismatch() {
        let err = rejection(
            "pub fn f() -> i64 { let a: i32 = 1; let b: i64 = 2; let c: i64 = a + b; return c; }",
        );
        assert!(
            err.contains(
                "cannot apply operator `Add` to operands of different types: `i32` and `i64`"
            ),
            "unexpected error: {err}"
        );
    }

    /// A call's result carries the declared return type; nothing about the
    /// surrounding position changes it.
    #[test]
    fn a_call_result_is_not_retyped_by_its_position() {
        let err = rejection(
            "fn g() -> i32 { return 1; } pub fn f() -> i64 { let x: i64 = g(); return x; }",
        );
        assert!(
            err.contains("type mismatch in variable definition: expected `i64`, found `i32`"),
            "unexpected error: {err}"
        );
    }

    /// A field access carries the field's declared type.
    #[test]
    fn a_field_access_is_not_retyped_by_its_position() {
        let err = rejection(
            "struct P { x: i32; } \
             pub fn f() -> i64 { let p: P = P { x: 1 }; let y: i64 = p.x; return y; }",
        );
        assert!(
            err.contains("type mismatch in variable definition: expected `i64`, found `i32`"),
            "unexpected error: {err}"
        );
    }
}

/// Peer typing infers the typed operand first, which is what keeps the
/// diagnostic for the ordinary mistake at the binding rather than at the
/// operator.
mod diagnostic_precision {
    use super::*;

    #[test]
    fn a_narrower_variable_still_reports_at_the_binding() {
        let errors =
            diagnostics("pub fn f() -> i64 { let a: i32 = 3; let x: i64 = a + 1; return x; }");
        assert!(
            errors
                .iter()
                .any(|e| e
                    .contains("type mismatch in variable definition: expected `i64`, found `i32`")),
            "unexpected diagnostics: {errors:?}"
        );
        assert!(
            !errors.iter().any(|e| e.contains("cannot apply operator")),
            "the operator should not be blamed: {errors:?}"
        );
    }

    #[test]
    fn a_narrower_variable_on_the_left_of_a_shift_reports_at_the_binding() {
        let errors =
            diagnostics("pub fn f() -> i64 { let a: i32 = 3; let x: i64 = a << 1; return x; }");
        assert!(
            errors
                .iter()
                .any(|e| e
                    .contains("type mismatch in variable definition: expected `i64`, found `i32`")),
            "unexpected diagnostics: {errors:?}"
        );
        assert!(
            !errors.iter().any(|e| e.contains("cannot apply operator")),
            "the operator should not be blamed: {errors:?}"
        );
    }
}

/// A literal argument at a *generic* parameter is unchanged: the type
/// parameters are bound from the arguments before any expected type exists, so
/// such a literal is still observed as `i32` there. These tests pin today's
/// behavior so a future change to the pre-pass is a deliberate one. A literal
/// at a *concrete* parameter of the same function does take that parameter's
/// type — see `generic_calls_with_concrete_parameters`.
mod generic_calls_unchanged {
    use super::*;

    #[test]
    fn a_literal_conflicting_with_a_typed_argument_is_still_rejected() {
        let err = rejection(
            "fn id T'(x: T, y: T) -> T { return x; } \
             pub fn f(a: i64) -> i64 { return id(a, 65536); }",
        );
        assert!(
            err.to_lowercase().contains("conflicting"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn a_leading_literal_still_binds_the_type_parameter_to_i32() {
        let err = rejection(
            "fn id T'(x: T, y: T) -> T { return x; } \
             pub fn f(a: i64) -> i64 { return id(65536, a); }",
        );
        assert!(
            err.contains("i32") && err.contains("i64"),
            "unexpected error: {err}"
        );
        assert_literal_typed(
            "fn id T'(x: T, y: T) -> T { return x; } \
             pub fn f(a: i64) -> i64 { return id(65536, a); }",
            "65536",
            "i32",
        );
    }
}

/// A generic function may declare concrete parameters alongside its type
/// parameters, and those behave like any other declared type: the argument loop
/// compares against the substituted parameter type, and substitution leaves a
/// concrete type alone, so a bare literal there denotes it.
mod generic_calls_with_concrete_parameters {
    use super::*;

    /// This call is newly accepted by the type checker — it used to be rejected
    /// with "expected `i64`, found `i32`". It still cannot be compiled: codegen
    /// has no monomorphization, so it fails there with "unsupported type in
    /// WASM codegen: T". The failure moved from the checker to codegen rather
    /// than going away, which is why this is a type-checker test with no
    /// codegen fixture behind it.
    #[test]
    fn a_literal_at_a_concrete_parameter_takes_that_parameters_type() {
        let source = "fn take T'(a: T, n: i64) -> T { return a; } \
                      pub fn f(x: i64) -> i64 { return take(x, 65536); }";
        assert_accepted(source);
        assert_literal_typed(source, "65536", "i64");
    }

    #[test]
    fn a_wide_literal_at_a_concrete_unsigned_parameter_is_accepted() {
        let source = "fn take T'(a: T, n: u64) -> T { return a; } \
                      pub fn f(x: i64) -> i64 { return take(x, 18446744073709551615); }";
        assert_accepted(source);
        assert_literal_typed(source, "18446744073709551615", "u64");
    }

    /// The concrete parameter still rejects what it cannot denote.
    #[test]
    fn a_literal_at_a_concrete_non_numeric_parameter_is_rejected() {
        let err = rejection(
            "fn take T'(a: T, flag: bool) -> T { return a; } \
             pub fn f(x: i64) -> i64 { return take(x, 5); }",
        );
        assert!(
            err.contains("expected `Bool`, found `i32`"),
            "unexpected error: {err}"
        );
    }
}

/// The largest `u64` value has no `i32` or `i64` reading at all, so a position
/// that accepts it is a position whose declared type reached the literal. One
/// cell per expected-type position.
mod unsigned_maximum_values {
    use super::*;

    const MAX: &str = "18446744073709551615";

    #[test]
    fn return_operand() {
        let source = "pub fn f() -> u64 { return 18446744073709551615; }";
        assert_accepted(source);
        assert_literal_typed(source, MAX, "u64");
    }

    #[test]
    fn annotated_let() {
        let source = "pub fn f() -> u64 { let x: u64 = 18446744073709551615; return x; }";
        assert_accepted(source);
        assert_literal_typed(source, MAX, "u64");
    }

    #[test]
    fn member_assignment() {
        let source = "struct P { x: u64; } pub fn f() -> u64 { let mut p: P = P { x: 0 }; \
                      p.x = 18446744073709551615; return p.x; }";
        assert_accepted(source);
        assert_literal_typed(source, MAX, "u64");
    }

    #[test]
    fn index_assignment() {
        let source = "pub fn f() -> u64 { let mut a: [u64; 2] = [0, 0]; \
                      a[0] = 18446744073709551615; return a[0]; }";
        assert_accepted(source);
        assert_literal_typed(source, MAX, "u64");
    }

    #[test]
    fn struct_literal_field() {
        let source = "struct P { x: u64; } \
                      pub fn f() -> u64 { let p: P = P { x: 18446744073709551615 }; return p.x; }";
        assert_accepted(source);
        assert_literal_typed(source, MAX, "u64");
    }

    #[test]
    fn array_literal_element() {
        let source = "pub fn f() -> u64 { let a: [u64; 2] = [18446744073709551615, 0]; \
                      return a[0]; }";
        assert_accepted(source);
        assert_literal_typed(source, MAX, "u64");
    }

    #[test]
    fn method_argument() {
        let source = "struct P { x: u64; fn keep(self, v: u64) -> u64 { return v; } } \
                      pub fn f() -> u64 { let p: P = P { x: 0 }; \
                      return p.keep(18446744073709551615); }";
        assert_accepted(source);
        assert_literal_typed(source, MAX, "u64");
    }

    #[test]
    fn associated_function_argument() {
        let source = "struct P { x: u64; fn make(v: u64) -> P { return P { x: v }; } } \
                      pub fn f() -> u64 { let p: P = P::make(18446744073709551615); return p.x; }";
        assert_accepted(source);
        assert_literal_typed(source, MAX, "u64");
    }

    #[test]
    fn peer_typed_operand() {
        let source = "pub fn f(a: u64) -> bool { return a == 18446744073709551615; }";
        assert_accepted(source);
        assert_literal_typed(source, MAX, "u64");
    }
}

/// Literal-built arithmetic is evaluated at the type the position asks for, not
/// at arbitrary precision, so a sum that fits neither operand's width wraps at
/// run time like every other Inference add.
mod target_width_arithmetic {
    use super::*;

    // FIXME: `200 + 100` is accepted because each operand is in range for `u8`;
    // the sum is not, and it wraps to 44 at run time. Rejecting a literal-built
    // expression whose exact value overflows its target width needs a
    // const-fold range rule alongside A022, which checks operands only. This
    // test asserts the current, wrapping behavior.
    #[test]
    fn a_narrow_sum_that_overflows_is_accepted_and_wraps() {
        let source = "pub fn f() -> u8 { let x: u8 = 200 + 100; return x; }";
        assert_accepted(source);
        assert_literal_typed(source, "200", "u8");
        assert_literal_typed(source, "100", "u8");
    }

    /// An operand that is itself out of range is a different matter: A022 sees
    /// the recorded type and rejects it.
    #[test]
    fn a_narrow_operand_that_is_out_of_range_is_still_typed_narrow() {
        let source = "pub fn f() -> u8 { let x: u8 = 200 + 300; return x; }";
        assert_accepted(source);
        assert_literal_typed(source, "300", "u8");
    }
}

/// A generic call binds its type parameters in a pre-pass that infers the
/// argument expressions before any expected type exists, recording an
/// `i32`-based type for their interior nodes. That record then answers for the
/// node, so the argument is never re-derived — which keeps every diagnostic
/// inside it reported once. It costs nothing in acceptance: where the recorded
/// type disagrees with the parameter type, the pre-pass has already rejected
/// the call.
mod memoized_generic_arguments {
    use super::*;

    /// `T` is bound to `i64` by the first argument, so the second is both
    /// reported as the conflict that bound it and rejected against the
    /// parameter type it does not match. Two distinct true statements about one
    /// rejected program.
    #[test]
    fn a_binary_argument_reports_the_conflict_and_the_mismatch() {
        let source = "fn pick T'(a: T, b: T) -> T { return a; } \
                      pub fn f(x: i64) -> i64 { return pick(x, 1 + 2); }";
        let errors = diagnostics(source);
        assert_eq!(errors.len(), 2, "expected two diagnostics, got: {errors:?}");
        assert!(
            errors[0].to_lowercase().contains("conflicting"),
            "unexpected diagnostic: {errors:?}"
        );
        assert!(
            errors[1].contains("expected `i64`, found `i32`"),
            "unexpected diagnostic: {errors:?}"
        );
        assert_literal_typed(source, "1", "i32");
        assert_literal_typed(source, "2", "i32");
    }

    #[test]
    fn an_array_literal_argument_reports_the_conflict_and_the_mismatch() {
        let source = "fn pick T'(a: T, b: T) -> T { return a; } \
                      pub fn f() -> i64 { let arr: [i64; 2] = [0, 0]; \
                      let r: [i64; 2] = pick(arr, [11, 22]); return r[0]; }";
        let errors = diagnostics(source);
        assert_eq!(errors.len(), 2, "expected two diagnostics, got: {errors:?}");
        assert!(
            errors[0].to_lowercase().contains("conflicting"),
            "unexpected diagnostic: {errors:?}"
        );
        assert!(
            errors[1].contains("expected `[i64; 2]`, found `[i32; 2]`"),
            "unexpected diagnostic: {errors:?}"
        );
        assert_literal_typed(source, "11", "i32");
        assert_literal_typed(source, "22", "i32");
    }

    /// The reason the record must answer unconditionally: re-deriving the
    /// argument would run its checks a second time and report everything inside
    /// it twice. A different width on the binding argument is what makes the
    /// recorded type disagree with the parameter type, so `i64` here and `i32`
    /// in the control are the two sides of the same shape.
    #[test]
    fn a_division_by_zero_inside_a_generic_argument_is_reported_once() {
        let source = "fn two T'(x: T, y: T) -> T { return x; } \
                      pub fn f(a: i64) -> i64 { return two(a, 1 / 0); }";
        let divisions = diagnostics(source)
            .iter()
            .filter(|e| e.contains("division by zero"))
            .count();
        assert_eq!(divisions, 1, "expected one division-by-zero diagnostic");
    }

    #[test]
    fn a_division_by_zero_inside_a_matching_generic_argument_is_reported_once() {
        let source = "fn two T'(x: T, y: T) -> T { return x; } \
                      pub fn f(a: i32) -> i32 { return two(a, 1 / 0); }";
        let divisions = diagnostics(source)
            .iter()
            .filter(|e| e.contains("division by zero"))
            .count();
        assert_eq!(divisions, 1, "expected one division-by-zero diagnostic");
    }

    #[test]
    fn a_parenthesized_division_by_zero_is_reported_once() {
        let source = "fn two T'(x: T, y: T) -> T { return x; } \
                      pub fn f(a: i64) -> i64 { return two(a, (1 / 0)); }";
        let divisions = diagnostics(source)
            .iter()
            .filter(|e| e.contains("division by zero"))
            .count();
        assert_eq!(divisions, 1, "expected one division-by-zero diagnostic");
    }

    #[test]
    fn a_division_by_zero_inside_an_array_argument_is_reported_once() {
        let source = "fn two T'(x: T, y: T) -> T { return x; } \
                      pub fn f() -> i64 { let arr: [i64; 2] = [0, 0]; \
                      let r: [i64; 2] = two(arr, [1 / 0, 2]); return r[0]; }";
        let divisions = diagnostics(source)
            .iter()
            .filter(|e| e.contains("division by zero"))
            .count();
        assert_eq!(divisions, 1, "expected one division-by-zero diagnostic");
    }

    /// `**` is rejected wherever it appears, and it too must be rejected once.
    #[test]
    fn an_unsupported_operator_inside_a_generic_argument_is_reported_once() {
        let source = "fn two T'(x: T, y: T) -> T { return x; } \
                      pub fn f(a: i64) -> i64 { return two(a, 2 ** 3); }";
        let powers = diagnostics(source)
            .iter()
            .filter(|e| e.contains("**"))
            .count();
        assert_eq!(powers, 1, "expected one `**` diagnostic");
    }
}

/// The diagnostics that survive contextual typing say why they survive.
///
/// Two operands that already have types is the one shape propagation cannot
/// help with, and a struct-literal field is a position of its own — both used
/// to be reported in words that pointed somewhere else.
mod diagnostic_text {
    use super::*;

    /// The one operand mismatch in `errors`.
    fn operand_mismatch(errors: &[String]) -> String {
        errors
            .iter()
            .find(|e| e.contains("cannot apply operator"))
            .unwrap_or_else(|| panic!("expected an operand mismatch, got: {errors:?}"))
            .clone()
    }

    /// The note has to exist because the obvious next thought — "then widen it"
    /// or "then cast it" — is not available in this language.
    ///
    /// It also must not send the reader looking for a literal. Where both
    /// operands are integers this error is only reachable from two that already
    /// have types, since a literal would have taken its peer's — so there is no
    /// literal on the line to annotate.
    #[test]
    fn the_operand_mismatch_says_why_two_widths_never_combine() {
        let errors =
            diagnostics("pub fn f() -> i64 { let a: i64 = 1; let b: i32 = 2; return a + b; }");
        let mismatch = operand_mismatch(&errors);
        assert!(
            mismatch.contains(
                "cannot apply operator `Add` to operands of different types: `i64` and `i32`"
            ),
            "the base message is unchanged: {mismatch}"
        );
        assert!(
            mismatch.contains(
                "note: Inference has no implicit widening and no cast operator, so `i64` and \
                 `i32` never combine; change one of the two declarations so both operands have \
                 the same type"
            ),
            "unexpected note: {mismatch}"
        );
        assert!(
            !mismatch.contains("literal"),
            "the note must not send the reader looking for a literal: {mismatch}"
        );
    }

    /// A non-numeric peer refuses to type its literal-built neighbour, so this
    /// error *is* reachable with a literal operand — and the note is still
    /// true of it: no cast makes a `Bool` and an `i32` comparable.
    ///
    /// FIXME: the note's help ("change one of the two declarations") is
    /// unactionable here — the `1` has no declaration to change, and the fix is
    /// to write `true`. Refining the help for the non-numeric-peer case is
    /// deferred; it needs the wording to branch on operand kind.
    #[test]
    fn the_note_also_reaches_a_non_numeric_operand_mismatch() {
        let errors = diagnostics("pub fn f(b: bool) -> bool { return b == 1; }");
        let mismatch = operand_mismatch(&errors);
        assert!(
            mismatch.contains("so `Bool` and `i32` never combine"),
            "unexpected note: {mismatch}"
        );
    }

    /// The field is the position that fixed the value's type, so it is the
    /// position the diagnostic names — it used to claim a variable definition.
    #[test]
    fn a_struct_field_mismatch_names_the_field_and_its_struct() {
        let errors = diagnostics("struct P { x: i32; } pub fn f() { let p: P = P { x: true }; }");
        assert_eq!(errors.len(), 1, "expected one diagnostic, got: {errors:?}");
        assert!(
            errors[0]
                .contains("type mismatch in field `x` of struct `P`: expected `i32`, found `Bool`"),
            "unexpected diagnostic: {errors:?}"
        );
    }

    /// A field mismatch inside a nested struct literal names the inner field,
    /// not the outer binding.
    #[test]
    fn a_nested_struct_field_mismatch_names_the_inner_field() {
        let errors = diagnostics(
            "struct Inner { v: bool; } struct Outer { i: Inner; } \
             pub fn f() { let o: Outer = Outer { i: Inner { v: 5 } }; }",
        );
        assert!(
            errors.iter().any(|e| e.contains(
                "type mismatch in field `v` of struct `Inner`: expected `Bool`, found `i32`"
            )),
            "unexpected diagnostics: {errors:?}"
        );
    }

    /// Elements keep their own dedicated diagnostic: an element that disagrees
    /// with its neighbours is not a mismatch against a position's expected
    /// type, and relabelling it would say the array itself was wrong.
    #[test]
    fn an_array_element_mismatch_keeps_its_own_diagnostic() {
        let errors = diagnostics("pub fn f() { let a: [i64; 2] = [1, true]; }");
        assert_eq!(errors.len(), 1, "expected one diagnostic, got: {errors:?}");
        assert!(
            errors[0]
                .contains("array elements must be of the same type: expected `i64`, found `Bool`"),
            "unexpected diagnostic: {errors:?}"
        );
    }
}

/// Alongside the type it records for a literal, the checker records the
/// position that supplied it.
///
/// A literal's type is written where the literal is not, so a diagnostic about
/// the literal has nothing to point at without this. The table is diagnostics
/// only — nothing about how a program compiles reads it — so these tests assert
/// the recorded position directly rather than through any compiled output.
mod literal_type_provenance {
    use super::*;

    #[test]
    fn an_annotated_let_records_the_variable_definition() {
        assert_eq!(
            literal_source("pub fn f() { let x: i64 = 7; }", "7"),
            Some(TypeMismatchContext::VariableDefinition)
        );
    }

    #[test]
    fn a_const_initializer_records_the_variable_definition() {
        assert_eq!(
            literal_source("pub fn f() { const C: i64 = 7; }", "7"),
            Some(TypeMismatchContext::VariableDefinition)
        );
    }

    #[test]
    fn an_assignment_records_the_assignment() {
        assert_eq!(
            literal_source("pub fn f() { let mut x: i64 = 0; x = 7; }", "7"),
            Some(TypeMismatchContext::Assignment)
        );
    }

    #[test]
    fn a_return_operand_records_the_return() {
        assert_eq!(
            literal_source("pub fn f() -> i64 { return 7; }", "7"),
            Some(TypeMismatchContext::Return)
        );
    }

    #[test]
    fn a_struct_field_records_the_field_and_its_struct() {
        assert_eq!(
            literal_source(
                "struct P { x: i64; } pub fn f() { let p: P = P { x: 7 }; }",
                "7"
            ),
            Some(TypeMismatchContext::StructField {
                struct_name: "P".to_string(),
                field_name: "x".to_string(),
            })
        );
    }

    #[test]
    fn an_array_element_records_the_element_position() {
        assert_eq!(
            literal_source("pub fn f() { let a: [i64; 2] = [7, 0]; }", "7"),
            Some(TypeMismatchContext::ArrayElement)
        );
    }

    /// Every level of a nested initializer is an element position, so the
    /// innermost leaf reports the element rather than the outer binding.
    #[test]
    fn a_nested_array_element_records_the_element_position() {
        assert_eq!(
            literal_source(
                "pub fn f() { let g: [[i64; 2]; 2] = [[7, 0], [0, 0]]; }",
                "7"
            ),
            Some(TypeMismatchContext::ArrayElement)
        );
    }

    #[test]
    fn a_free_function_argument_records_the_parameter() {
        assert_eq!(
            literal_source(
                "fn take(v: i64) -> i64 { return v; } pub fn f() -> i64 { return take(7); }",
                "7"
            ),
            Some(TypeMismatchContext::FunctionArgument {
                function_name: "take".to_string(),
                arg_name: "arg0".to_string(),
                arg_index: 0,
            })
        );
    }

    #[test]
    fn a_method_argument_records_the_method_parameter() {
        assert_eq!(
            literal_source(
                "struct P { x: i64; fn keep(self, v: i64) -> i64 { return self.x + v; } } \
                 pub fn f() -> i64 { let p: P = P { x: 1 }; return p.keep(7); }",
                "7"
            ),
            Some(TypeMismatchContext::MethodArgument {
                type_name: "P".to_string(),
                method_name: "keep".to_string(),
                arg_name: "arg0".to_string(),
                arg_index: 0,
            })
        );
    }

    #[test]
    fn an_associated_function_argument_records_the_parameter() {
        assert_eq!(
            literal_source(
                "struct P { x: i64; fn make(v: i64) -> P { return P { x: v }; } } \
                 pub fn f() { let p: P = P::make(7); }",
                "7"
            ),
            Some(TypeMismatchContext::MethodArgument {
                type_name: "P".to_string(),
                method_name: "make".to_string(),
                arg_name: "arg0".to_string(),
                arg_index: 0,
            })
        );
    }

    /// A peer-typed operand took the type its neighbour already had, which is
    /// not a position that required anything of it — so it records the operator
    /// rather than one of the positions above.
    #[test]
    fn a_peer_typed_operand_records_the_operator() {
        assert_eq!(
            literal_source("pub fn f(a: i64) -> i64 { return a + 7; }", "7"),
            Some(TypeMismatchContext::BinaryPeerOperand(OperatorKind::Add))
        );
    }

    #[test]
    fn a_peer_typed_shift_count_records_the_shift() {
        assert_eq!(
            literal_source("pub fn f(a: i64) -> i64 { return a << 7; }", "7"),
            Some(TypeMismatchContext::BinaryPeerOperand(OperatorKind::Shl))
        );
    }

    /// Peer typing applies to comparisons too, and the operand it types is
    /// still attributed to the operator.
    #[test]
    fn a_peer_typed_comparison_operand_records_the_comparison() {
        assert_eq!(
            literal_source("pub fn f(a: i64) -> bool { return a < 7; }", "7"),
            Some(TypeMismatchContext::BinaryPeerOperand(OperatorKind::Lt))
        );
    }

    /// The transparent forms carry the position along with the type, so a leaf
    /// under parentheses and a negation still names the binding.
    #[test]
    fn transparent_descent_keeps_the_outer_position() {
        assert_eq!(
            literal_source("pub fn f() { let x: i64 = -(7); }", "7"),
            Some(TypeMismatchContext::VariableDefinition)
        );
    }

    /// Neither operand of a literal-only operation can inform the other, so
    /// both name whatever position supplied the type to the whole expression.
    #[test]
    fn both_closed_descent_keeps_the_outer_position() {
        assert_eq!(
            literal_source("pub fn f() -> i64 { return 7 + 1; }", "7"),
            Some(TypeMismatchContext::Return)
        );
        assert_eq!(
            literal_source("pub fn f() -> i64 { return 7 + 1; }", "1"),
            Some(TypeMismatchContext::Return)
        );
    }

    /// A spec body runs through the same inference pass, so a literal in a
    /// proof obligation is attributed the same way as one in a function.
    #[test]
    fn a_literal_in_a_spec_body_records_its_position() {
        assert_eq!(
            literal_source(
                "spec S { fn obligation() -> i64 { return 7; } } pub fn f() { }",
                "7"
            ),
            Some(TypeMismatchContext::Return)
        );
    }

    /// The position is recorded in the same place the type is, so the two can
    /// never disagree — including where a literal is visited twice, which the
    /// generic-argument pre-pass does before any expected type exists.
    #[test]
    fn a_revisited_literal_keeps_its_position_and_type_in_step() {
        let source = "fn take T'(a: T, n: i64) -> T { return a; } \
                      pub fn f(x: i64) -> i64 { return take(x, 65536); }";
        assert_literal_typed(source, "65536", "i64");
        assert_eq!(
            literal_source(source, "65536"),
            Some(TypeMismatchContext::FunctionArgument {
                function_name: "take".to_string(),
                arg_name: "arg1".to_string(),
                arg_index: 1,
            })
        );
    }

    /// The pre-pass records a type for an interior node before any position
    /// has asked for one, and the memoized arm then answers without
    /// descending — so these leaves keep the default and stay unattributed
    /// rather than picking up a position that never reached them.
    #[test]
    fn a_literal_the_pre_pass_typed_stays_unattributed() {
        let source = "fn two T'(a: T, b: T) -> T { return a; } \
                      pub fn f(x: i64) -> i64 { return two(x, 1 + 2); }";
        assert_literal_unattributed(source, "1");
        assert_literal_unattributed(source, "2");
    }

    /// A parameter whose type the call *inferred* did not give the literal its
    /// type — the literal's own default did, before any expected type existed.
    /// Naming the parameter would state a cause that is not there, so nothing
    /// is recorded and the range error stays silent about why.
    #[test]
    fn a_literal_at_an_inferred_parameter_stays_unattributed() {
        assert_literal_unattributed(
            "fn id T'(x: T) -> T { return x; } pub fn f() -> i32 { return id(3000000000); }",
            "3000000000",
        );
    }

    /// A *declared* parameter of a generic function is still a real position,
    /// so withholding attribution above is about substitution, not about
    /// generics.
    #[test]
    fn a_literal_at_a_declared_parameter_of_a_generic_function_is_attributed() {
        assert_eq!(
            literal_source(
                "fn take T'(a: T, n: u8) -> T { return a; } \
                 pub fn f(x: i64) -> i64 { return take(x, 300); }",
                "300"
            ),
            Some(TypeMismatchContext::FunctionArgument {
                function_name: "take".to_string(),
                arg_name: "arg1".to_string(),
                arg_index: 1,
            })
        );
    }

    /// Withholding the expected type where substitution changed the parameter
    /// changes no outcome: the pre-pass binds `T` from the first argument and
    /// has already rejected the call before any expected type could apply.
    #[test]
    fn withholding_attribution_does_not_change_what_is_rejected() {
        let errors = diagnostics(
            "fn pair T'(a: T, b: T) -> T { return a; } \
             pub fn f(x: u8) -> u8 { return pair(x, 300); }",
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("conflicting types for type parameter `T`")),
            "unexpected diagnostics: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("expected `u8`, found `i32`")),
            "unexpected diagnostics: {errors:?}"
        );
    }

    #[test]
    fn a_literal_left_at_the_default_has_no_recorded_position() {
        assert_literal_unattributed("pub fn f() { 7; }", "7");
    }

    /// An array index is on the stop list: nothing types it, so nothing is
    /// recorded for it either.
    #[test]
    fn an_array_index_has_no_recorded_position() {
        assert_literal_unattributed(
            "pub fn f() -> i64 { let a: [i64; 2] = [0, 0]; return a[1]; }",
            "1",
        );
    }

    /// A comparison's operands are peer-typed but never receive the type
    /// expected of the comparison, so a literal beside a non-numeric operand
    /// is left where it started.
    #[test]
    fn a_comparison_operand_beside_a_non_numeric_peer_is_unattributed() {
        assert_literal_unattributed(
            "pub fn f() { let b: bool = 3000000000 == 1; }",
            "3000000000",
        );
    }

    /// A shift types both sides, so the base position is attributed as well as
    /// the count.
    #[test]
    fn the_base_of_a_shift_is_attributed_like_the_count() {
        assert_eq!(
            literal_source("pub fn f(n: i64) -> i64 { return 7 << n; }", "7"),
            Some(TypeMismatchContext::BinaryPeerOperand(OperatorKind::Shl))
        );
    }

    /// A call in an imported file goes through a different argument loop than
    /// a local one, and it must record the position just the same.
    ///
    /// The two qualified loops disagree on how they name the callee: a
    /// qualified free function renders the full path it was written with,
    /// while a qualified associated function renders `Type::method` with the
    /// namespace dropped. That inconsistency predates provenance and is left
    /// alone here; these tests pin what each one does today.
    #[test]
    fn a_qualified_free_function_argument_records_the_written_path() {
        let entry = "use lib; pub fn f() -> u8 { return lib::take(300); }";
        let ctx = crate::utils::try_type_check_multi_file(&[
            (vec![], entry),
            (vec!["lib"], "pub fn take(v: u8) -> u8 { return v; }"),
        ])
        .expect("the call type-checks");
        assert_eq!(
            literal_source_in(&ctx, entry, "300"),
            Some(TypeMismatchContext::FunctionArgument {
                function_name: "lib::take".to_string(),
                arg_name: "arg0".to_string(),
                arg_index: 0,
            })
        );
    }

    #[test]
    fn a_qualified_associated_function_argument_records_the_bare_type_path() {
        let entry = "use lib; pub fn f() { let p: lib::P = lib::P::make(300); }";
        let ctx = crate::utils::try_type_check_multi_file(&[
            (vec![], entry),
            (
                vec!["lib"],
                "pub struct P { x: u8; pub fn make(v: u8) -> P { return P { x: v }; } }",
            ),
        ])
        .expect("the call type-checks");
        assert_eq!(
            literal_source_in(&ctx, entry, "300"),
            Some(TypeMismatchContext::MethodArgument {
                type_name: "P".to_string(),
                method_name: "make".to_string(),
                arg_name: "arg0".to_string(),
                arg_index: 0,
            })
        );
    }
}

/// The table's stated invariant, swept over a corpus rather than asserted one
/// position at a time: a literal whose recorded type is not the `i32` default
/// was typed by *something*, and that something has to be named. A position
/// added later that forgets to pass its source shows up here.
#[test]
fn a_non_default_literal_always_has_a_recorded_position() {
    let sources = [
        "pub fn f() { let x: i64 = 7; }",
        "pub fn f() { const C: u8 = 7; }",
        "pub fn f() { let mut x: u16 = 0; x = 7; }",
        "pub fn f() -> u32 { return 7; }",
        "struct P { x: i8; } pub fn f() { let p: P = P { x: 7 }; }",
        "pub fn f() { let a: [i64; 2] = [7, 0]; }",
        "pub fn f() { let g: [[u8; 2]; 2] = [[7, 0], [0, 0]]; }",
        "fn take(v: i64) -> i64 { return v; } pub fn f() -> i64 { return take(7); }",
        "struct P { x: i64; fn keep(self, v: i64) -> i64 { return self.x + v; } } \
         pub fn f() -> i64 { let p: P = P { x: 1 }; return p.keep(7); }",
        "pub fn f(a: i64) -> i64 { return a + 7; }",
        "pub fn f(a: u64) -> bool { return a < 7; }",
        "pub fn f() { let x: i64 = -(7); }",
        "pub fn f() -> i64 { return 7 + 1; }",
        "pub fn f() { let x: u8 = ~7; }",
        "spec S { fn obligation() -> i64 { return 7; } } pub fn f() { }",
    ];
    for source in sources {
        let recorded = literal_type(source, "7").to_string();
        if recorded == "i32" {
            continue;
        }
        assert!(
            literal_source(source, "7").is_some(),
            "`7` is typed `{recorded}` in `{source}` but nothing recorded why"
        );
    }
}

/// A `const` whose initializer matches its declared type is re-stamped with the
/// *resolved* declared type, so the recorded kind is the canonical `Struct`
/// rather than the unresolved `Custom` the initializer may have carried. The
/// stamp is the only thing that normalizes it, and dropping it would surface as
/// a same-named type from another file comparing equal.
#[test]
fn const_struct_initializer_records_the_canonical_struct_type() {
    let source = "struct Point { x: i32; } const P: Point = Point { x: 1 }; pub fn f() { }";
    let ctx = try_type_check(source).expect("the const initializer type-checks");
    let arena = ctx.arena();
    let initializer = arena
        .source_files()
        .flat_map(|file| file.defs.iter())
        .find_map(|&def_id| match &arena[def_id].kind {
            Def::Constant { value, .. } => Some(*value),
            _ => None,
        })
        .expect("the program declares one top-level constant");
    let recorded = ctx
        .get_node_typeinfo(NodeId::Expr(initializer))
        .expect("the initializer carries a recorded type");
    assert!(
        matches!(&recorded.kind, TypeInfoKind::Struct(name, _) if name == "Point"),
        "expected the canonical struct kind, got: {recorded:?}"
    );
}

/// An array annotation whose size was already rejected suppresses the follow-on
/// initializer mismatch, so a named-constant size reports only the size error.
#[test]
fn rejected_array_size_suppresses_the_initializer_mismatch() {
    let errors = diagnostics("const N: i32 = 3; pub fn f() { let a: [i32; N] = 5; }");
    assert_eq!(errors.len(), 1, "expected one diagnostic, got: {errors:?}");
    assert!(
        errors[0].contains("array size must be an integer literal"),
        "unexpected diagnostic: {errors:?}"
    );
}
