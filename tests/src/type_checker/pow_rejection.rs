//! Every expression position that accepts a binary expression must reject `**`
//! at type check; the former codegen panic (compiler.rs Pow todo!) must be
//! unreachable.

use crate::utils::{build_ast, try_codegen};
use inference_type_checker::TypeCheckerBuilder;

const POW_MSG: &str = "the power operator `**` is not yet supported";

fn try_type_check(
    source: &str,
) -> anyhow::Result<inference_type_checker::typed_context::TypedContext> {
    let arena = build_ast(source.to_string());
    Ok(TypeCheckerBuilder::build_typed_context(arena)?.typed_context())
}

/// Type-checks `source`, asserts it is rejected, and returns the joined error
/// string so individual tests can assert on the diagnostic shape.
fn pow_error(source: &str) -> String {
    match try_type_check(source) {
        Ok(_) => panic!("`**` must be rejected at type check: {source}"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn pow_in_fn_body_return() {
    let err = pow_error("pub fn f(a: i64, b: i64) -> i64 { return a ** b; }");
    assert!(err.contains(POW_MSG), "unexpected error: {err}");
}

#[test]
fn pow_in_method_body() {
    let err = pow_error(
        "struct P { x: i32; fn get(self) -> i32 { return 2 ** 3; } } \
         fn main(p: P) -> i32 { return p.get(); }",
    );
    assert!(err.contains(POW_MSG), "unexpected error: {err}");
}

#[test]
fn pow_in_spec_fn_body() {
    let err = pow_error("spec T { pub fn f() -> i32 { return 2 ** 3; } }");
    assert!(err.contains(POW_MSG), "unexpected error: {err}");
}

#[test]
fn pow_in_local_const_init() {
    let err = pow_error("pub fn f() -> i32 { const C: i32 = 2 ** 3; return C; }");
    assert!(err.contains(POW_MSG), "unexpected error: {err}");
}

#[test]
fn pow_in_top_level_const_init() {
    let err = pow_error("const C: i32 = 2 ** 3; pub fn f() -> i32 { return C; }");
    assert!(err.contains(POW_MSG), "unexpected error: {err}");
}

#[test]
fn pow_in_let_init() {
    let err = pow_error("pub fn f() -> i32 { let x: i32 = 2 ** 3; return x; }");
    assert!(err.contains(POW_MSG), "unexpected error: {err}");
}

#[test]
fn pow_in_call_arg() {
    let err = pow_error(
        "fn g(v: i32) -> i32 { return v; } pub fn f() -> i32 { return g(2 ** 3); }",
    );
    assert!(err.contains(POW_MSG), "unexpected error: {err}");
}

#[test]
fn pow_in_array_index() {
    let err = pow_error(
        "pub fn f() -> i32 { let a: [i32; 4] = [0, 0, 0, 0]; return a[1 ** 2]; }",
    );
    assert!(err.contains(POW_MSG), "unexpected error: {err}");
}

#[test]
fn pow_in_if_condition() {
    let err = pow_error("pub fn f() -> i32 { if (2 ** 3) == 8 { return 1; } return 0; }");
    assert!(err.contains(POW_MSG), "unexpected error: {err}");
}

#[test]
fn pow_in_loop_condition() {
    let err = pow_error("pub fn f() { loop (2 ** 3) { break; } }");
    assert!(err.contains(POW_MSG), "unexpected error: {err}");
}

#[test]
fn pow_in_struct_literal_field() {
    let err = pow_error(
        "struct P { x: i32; } pub fn f() -> i32 { let p: P = P { x: 2 ** 3 }; return p.x; }",
    );
    assert!(err.contains(POW_MSG), "unexpected error: {err}");
}

#[test]
fn pow_in_array_literal_element() {
    let err = pow_error("pub fn f() -> i32 { let a: [i32; 2] = [2 ** 3, 1]; return a[0]; }");
    assert!(err.contains(POW_MSG), "unexpected error: {err}");
}

#[test]
fn pow_in_expression_statement() {
    let err = pow_error("pub fn f() { 2 ** 3; }");
    assert!(err.contains(POW_MSG), "unexpected error: {err}");
}

#[test]
fn pow_in_assert_condition() {
    let err = pow_error("pub fn f() { assert((2 ** 3) == 8); }");
    assert!(err.contains(POW_MSG), "unexpected error: {err}");
}

#[test]
fn pow_nested_fires_per_node() {
    let err = pow_error("pub fn f(a: i32, b: i32, c: i32) -> i32 { return (a ** b) ** c; }");
    assert_eq!(
        err.matches(POW_MSG).count(),
        2,
        "each `**` node must fire its own diagnostic: {err}"
    );
}

#[test]
fn pow_bool_operands_single_diagnostic() {
    let err = pow_error("pub fn f() -> bool { return true ** false; }");
    assert!(err.contains(POW_MSG), "unexpected error: {err}");
    assert!(
        !err.contains("cannot be applied"),
        "operand-shape noise must be suppressed for `**`: {err}"
    );
    assert!(
        !err.contains("different types"),
        "operand-shape noise must be suppressed for `**`: {err}"
    );
}

#[test]
fn pow_mixed_operands_single_diagnostic() {
    let err = pow_error("pub fn f(a: i32) -> i32 { return a ** true; }");
    assert!(err.contains(POW_MSG), "unexpected error: {err}");
    assert!(
        !err.contains("different types"),
        "operand-shape noise must be suppressed for `**`: {err}"
    );
    assert!(
        !err.contains("cannot be applied"),
        "operand-shape noise must be suppressed for `**`: {err}"
    );
}

#[test]
fn pow_with_unresolved_operand_still_fires() {
    let err = pow_error("pub fn f() -> i32 { return undeclared ** 3; }");
    assert!(err.contains(POW_MSG), "unexpected error: {err}");
    assert!(
        err.contains("undeclared variable"),
        "the unresolved operand's own error must still surface: {err}"
    );
}

#[test]
fn pow_with_uzumaki_operand_in_spec() {
    let err = pow_error(
        "spec S { pub fn g() -> i32 { let r: i32 = 0; forall { let y: i32 = @ ** 2; } return r; } }",
    );
    assert!(err.contains(POW_MSG), "unexpected error: {err}");
}

#[test]
fn star_positive_control_full_pipeline() {
    let result = try_codegen("pub fn test(a: i32, b: i32) -> i32 { return a * b; }");
    assert!(
        result.is_ok(),
        "the `*` control must compile end-to-end: {:?}",
        result.err()
    );
}

/// Successor of the deleted `negative.rs` `unimplemented_operators` module: the
/// three former codegen-panic sources now fail at type check, proving codegen —
/// and the `unreachable!` in its `OperatorKind::Pow` arm — can never see `**`.
#[test]
fn pow_never_reaches_codegen() {
    for source in [
        "pub fn test() -> i32 { return 2 ** 3; }",
        "pub fn test(a: i32, b: i32) -> i32 { return a ** b; }",
        "pub fn test(a: i64, b: i64) -> i64 { return a ** b; }",
    ] {
        let err = pow_error(source);
        assert!(
            err.contains(POW_MSG),
            "`**` must be rejected before codegen for `{source}`: {err}"
        );
    }
}
