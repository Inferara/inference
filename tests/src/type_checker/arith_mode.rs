//! Typing of the `checked(...)` / `wrapping(...)` annotations.
//!
//! The rule is short — the annotation takes its operand's type, and that type
//! must be scalar — but it has to hold at every width and in every position a
//! type can be expected from, because an annotation that dropped the expected
//! type would leave the integer literals inside it typed differently from the
//! same literals written without it.
//!
//! `bool` is deliberately inside the rule: the arithmetic an author means to
//! govern is usually inside a comparison (`checked(a + b == c)`), and what the
//! annotation encloses is then the comparison's own `bool` result. Only a type
//! with no arithmetic anywhere inside it — a struct, an array, a unit value —
//! is refused. Whether the annotation reached an operator at all is a separate
//! question that a later analysis rule owns, so nothing here asserts on it.

use crate::utils::build_ast;
use inference_type_checker::TypeCheckerBuilder;
use inference_type_checker::typed_context::TypedContext;

const WIDTHS: [&str; 8] = ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64"];

fn type_check(source: &str) -> anyhow::Result<TypedContext> {
    let arena = build_ast(source.to_string());
    Ok(TypeCheckerBuilder::build_typed_context(arena)?.typed_context())
}

/// Type-checks `source`, asserts it is rejected, and returns the joined error
/// string.
fn rejection(source: &str) -> String {
    match type_check(source) {
        Ok(_) => panic!("expected a type error for: {source}"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn an_annotated_expression_has_its_operand_type_at_every_width() {
    for width in WIDTHS {
        let source =
            format!("pub fn f(a: {width}, b: {width}) -> {width} {{ return wrapping(a + b); }}");
        type_check(&source).unwrap_or_else(|e| panic!("{width}: {e}"));
        let checked =
            format!("pub fn f(a: {width}, b: {width}) -> {width} {{ return checked(a * b); }}");
        type_check(&checked).unwrap_or_else(|e| panic!("{width}: {e}"));
    }
}

#[test]
fn an_annotation_passes_the_expected_type_through_to_the_literals_inside_it() {
    // Nothing inside the annotation carries a type of its own, so the binding's
    // annotation has to reach both literals. At i64 the sum is only in range if
    // it does.
    type_check("pub fn f() -> i64 { let n: i64 = wrapping(3000000000 + 1); return n; }")
        .expect("the expected type must reach the literals inside the annotation");
}

#[test]
fn an_annotation_over_a_comparison_of_sums_is_well_typed() {
    // The shape the annotation exists for at a boundary: the arithmetic is
    // inside the comparison, so what the annotation encloses is a `bool`.
    for spelling in ["checked", "wrapping"] {
        for op in ["==", "!=", "<", "<=", ">", ">="] {
            let source = format!(
                "pub fn f(a: i32, b: i32, c: i32) -> bool {{ return {spelling}(a + b {op} c); }}"
            );
            type_check(&source).unwrap_or_else(|e| panic!("{spelling} over `{op}`: {e}"));
        }
    }
}

#[test]
fn an_annotation_over_a_plain_boolean_is_well_typed() {
    // Nothing to govern here, but that is the analysis pass's finding rather
    // than a type error: the type rule asks only that the value be scalar.
    type_check("pub fn f(a: bool) -> bool { return wrapping(a); }")
        .expect("a `bool` operand is scalar");
    type_check("pub fn f() -> bool { return checked(true); }")
        .expect("a `bool` literal is scalar");
}

#[test]
fn a_struct_operand_is_refused() {
    let err = rejection(
        "struct P { x: i32; y: i32; } \
         pub fn f(p: P) -> i32 { let q: P = checked(p); return q.x; }",
    );
    assert!(
        err.contains("`checked(...)` cannot be applied to `P`")
            && err.contains(
                "the annotation fixes how `+`, `-`, `*` and unary `-` behave when their result \
                 leaves the operand type, which is a property of the scalar number types, and \
                 `P` is an aggregate, which those operators never combine\nnote: annotate the \
                 arithmetic on the scalar leaves instead, as in `P { x: checked(a + b) }` or \
                 `arr[checked(i + 1)]`"
            )
            && !err.contains("no value in expression position"),
        "unexpected error: {err}"
    );
}

#[test]
fn an_array_operand_is_refused() {
    let err = rejection(
        "pub fn f() -> i32 { let a: [i32; 3] = [1, 2, 3]; let b: [i32; 3] = wrapping(a); \
         return b[0]; }",
    );
    assert!(
        err.contains("`wrapping(...)` cannot be applied to `[i32; 3]`")
            && err.contains("`[i32; 3]` is an aggregate, which those operators never combine"),
        "unexpected error: {err}"
    );
}

#[test]
fn a_unit_operand_is_refused() {
    let err = rejection("fn g() { } pub fn f() -> i32 { let n: i32 = wrapping(g()); return n; }");
    assert!(
        err.contains("`wrapping(...)` cannot be applied to `Unit`")
            && err.contains(
                "`Unit` is the unit value, which those operators never combine and which has no \
                 value in expression position here"
            ),
        "the unit arm must name the unit value rather than call it an aggregate: {err}"
    );
}

#[test]
fn only_the_unit_arm_denies_the_operand_a_value() {
    // The tail of the message is not one sentence for every kind. A struct and
    // an array *are* values in expression position — `let q: P = p;` is a
    // program — so the clause that says otherwise belongs to the unit value
    // alone, where it is the whole reason the annotation has nothing to enclose.
    for (source, rendered) in [
        (
            "struct P { x: i32; } pub fn f(p: P) -> i32 { let q: P = checked(p); return q.x; }",
            "`P` is an aggregate, which those operators never combine",
        ),
        (
            "pub fn f() -> i32 { let a: [i32; 2] = [1, 2]; let b: [i32; 2] = wrapping(a); \
             return b[0]; }",
            "`[i32; 2]` is an aggregate, which those operators never combine",
        ),
    ] {
        let err = rejection(source);
        assert!(err.contains(rendered), "unexpected error: {err}");
        assert!(
            !err.contains("no value in expression position"),
            "an aggregate has a value in expression position, so the message must not say it \
             does not: {err}"
        );
    }
}

#[test]
fn an_enumeration_operand_is_refused() {
    let err = rejection(
        "enum Colour { Red, Green } \
         pub fn f(c: Colour) -> i32 { let d: Colour = checked(c); return 0; }",
    );
    assert!(
        err.contains("`checked(...)` cannot be applied to `Colour`")
            && err.contains(
                "`Colour` is an enumeration value, which is not a number those operators apply to"
            ),
        "an enum tag is neither an aggregate nor the unit value: {err}"
    );
}

#[test]
fn a_string_operand_is_refused() {
    // Reachable here because this is the type checker: the analysis rule that
    // rejects `string` outright runs after it, so the annotation's own rule is
    // what answers first and has to say something true about a string.
    let err = rejection("pub fn f() -> i32 { let s: string = wrapping(\"hi\"); return 0; }");
    assert!(
        err.contains("`wrapping(...)` cannot be applied to")
            && err.contains("is a string value, which is not a number those operators apply to"),
        "unexpected error: {err}"
    );
}

#[test]
fn an_enumeration_variant_path_is_refused_as_an_enumeration_value() {
    // The other spelling of an enum operand. `C::A` is a path rather than a
    // binding, and it reaches the same arm — which is worth pinning because a
    // path is also how one would try to name a function, and that spelling does
    // not produce a function *value* anywhere in this language.
    let err = rejection("enum C { A } pub fn f() -> i32 { let h: i32 = checked(C::A); return h; }");
    assert!(
        err.contains("`C` is an enumeration value, which is not a number those operators apply to"),
        "unexpected error: {err}"
    );
}

#[test]
fn nested_annotations_type_at_the_operand_width() {
    type_check("pub fn f(a: i64, b: i64, c: i64) -> i64 { return checked(a * wrapping(b + c)); }")
        .expect("a nested annotation types as its own operand");
}

#[test]
fn an_annotated_operand_combines_with_an_unannotated_one() {
    // The annotation is transparent to typing, so both sides of the outer `*`
    // still have to agree on a width.
    type_check("pub fn f(a: u32, b: u32) -> u32 { return wrapping(a + b) * b; }")
        .expect("an annotated operand keeps its own type");
    let err = rejection("pub fn f(a: u32, b: i32) -> u32 { return wrapping(a + a) * b; }");
    assert!(
        err.contains("operands of different types"),
        "unexpected error: {err}"
    );
}

#[test]
fn an_annotation_over_a_unary_minus_types_as_the_signed_operand() {
    type_check("pub fn f(a: i32) -> i32 { return checked(-a); }")
        .expect("unary minus keeps its operand's type under an annotation");
    let err = rejection("pub fn f(a: u32) -> u32 { return checked(-a); }");
    assert!(
        err.contains("can only be applied to signed integers"),
        "unexpected error: {err}"
    );
}

#[test]
fn an_annotation_is_legal_in_a_const_initializer() {
    // A constant initializer is an expression like any other, so both spellings
    // mean there exactly what they mean elsewhere.
    type_check("pub fn f() -> i32 { const K: i32 = wrapping(2147483647 + 1); return K; }")
        .expect("`wrapping(...)` is legal in a const initializer");
    type_check("pub fn f() -> i32 { const A: i32 = 1; const B: i32 = checked(A + 1); return B; }")
        .expect("a const initializer may name an earlier constant through an annotation");
}

#[test]
fn a_const_initializer_may_name_a_later_constant_through_an_annotation() {
    // Declaration order does not decide resolution order, and an annotation
    // around the initializer does not change that. The dependency edge itself
    // is measured where it is built, by the unit test in the type checker's
    // definition graph; type checking is the only layer at which the edge is
    // observable end to end, which is why the assertion lives here. It is not
    // an acceptance a user sees: A032 rejects every module-scope `const` as
    // not yet implemented, so this program is refused one pass later.
    for spelling in ["checked", "wrapping"] {
        let source = format!(
            "const B: i32 = {spelling}(A + 1); const A: i32 = 41; \
             pub fn f() -> i32 {{ return B; }}"
        );
        type_check(&source)
            .unwrap_or_else(|e| panic!("the {spelling} annotation must not hide `A`: {e}"));
    }
}

#[test]
fn whitespace_between_the_keyword_and_the_parenthesis_is_accepted() {
    // The form is not one of the language's glue-sensitive spellings: the
    // keyword and its `(` are separate tokens, so layout does not change what
    // the program means.
    for source in [
        "pub fn f(a: i32, b: i32) -> i32 { return wrapping(a + b); }",
        "pub fn f(a: i32, b: i32) -> i32 { return wrapping (a + b); }",
        "pub fn f(a: i32, b: i32) -> i32 { return wrapping\n    (a + b); }",
    ] {
        type_check(source).unwrap_or_else(|e| panic!("{source:?}: {e}"));
    }
}
