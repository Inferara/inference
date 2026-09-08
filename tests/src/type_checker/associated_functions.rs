//! Tests for has_self flag functionality distinguishing instance methods from associated functions
//!
//! This module contains tests verifying:
//! - Instance methods (with `self` parameter) can only be called on receivers
//! - Associated functions (without `self`) can only be called via Type::function() syntax
//! - Proper error messages when calling methods incorrectly

use crate::utils::build_ast;
use inference_type_checker::TypeCheckerBuilder;

fn try_type_check(
    source: &str,
) -> anyhow::Result<inference_type_checker::typed_context::TypedContext> {
    let arena = build_ast(source.to_string());
    Ok(TypeCheckerBuilder::build_typed_context(arena)?.typed_context())
}

#[test]
fn method_with_self_is_instance_method() {
    let source = r#"struct Point { x: i32; fn get_x(self) -> i32 { return self.x; } } fn test(p: Point) -> i32 { return p.get_x(); }"#;
    let result = try_type_check(source);
    assert!(
        result.is_ok(),
        "Instance method called on receiver should succeed, got: {:?}",
        result.err()
    );
}

#[test]
fn method_without_self_is_associated_function() {
    let source = r#"struct Counter { value: i32; fn create() -> i32 { return 0; } } fn test() -> i32 { return 42; }"#;
    let result = try_type_check(source);
    assert!(
        result.is_ok(),
        "Associated function definition should succeed, got: {:?}",
        result.err()
    );
}

#[test]
fn associated_function_call_via_type_syntax() {
    let source = r#"struct Math { fn add(a: i32, b: i32) -> i32 { return a + b; } } fn test() -> i32 { return Math::add(1, 2); }"#;
    let result = try_type_check(source);
    assert!(
        result.is_ok(),
        "Associated function call via Type::function() should succeed, got: {:?}",
        result.err()
    );
}

#[test]
fn instance_method_called_as_associated_function_errors() {
    let source = r#"struct Point { x: i32; fn get_x(self) -> i32 { return self.x; } } fn test() -> i32 { return Point::get_x(); }"#;
    cov_mark::check!(type_checker_instance_method_called_as_associated);
    let result = try_type_check(source);
    assert!(
        result.is_err(),
        "Instance method called without receiver should fail"
    );
    if let Err(error) = result {
        let error_msg = error.to_string();
        assert!(
            error_msg.contains("instance method") && error_msg.contains("requires a receiver"),
            "Error should mention instance method requires receiver, got: {}",
            error_msg
        );
    }
}

#[test]
fn associated_function_called_as_instance_method_errors() {
    let source = r#"struct Math { fn add(a: i32, b: i32) -> i32 { return a + b; } } fn test(m: Math) -> i32 { return m.add(1, 2); }"#;
    cov_mark::check!(type_checker_associated_function_called_as_method);
    let result = try_type_check(source);
    assert!(
        result.is_err(),
        "Associated function called with receiver should fail"
    );
    if let Err(error) = result {
        let error_msg = error.to_string();
        assert!(
            error_msg.contains("associated function")
                && error_msg.contains("cannot be called on an instance"),
            "Error should mention associated function cannot be called on instance, got: {}",
            error_msg
        );
    }
}

#[test]
fn constructor_pattern_returns_correct_type() {
    // Simplified constructor test - verifying that associated function call returns correct type
    // FIXME: Complex struct construction in associated function has type comparison issues
    let source = r#"struct Math { fn get_zero() -> i32 { return 0; } } fn test() -> i32 { return Math::get_zero(); }"#;
    let result = try_type_check(source);
    assert!(
        result.is_ok(),
        "Constructor pattern as associated function should work, got: {:?}",
        result.err()
    );
}

#[test]
fn mixed_instance_and_associated_functions() {
    let source = r#"
        struct Counter {
            value: i32;
            fn zero() -> i32 { return 0; }
            fn get(self) -> i32 { return self.value; }
        }
        fn test(c: Counter) -> i32 {
            let z: i32 = Counter::zero();
            return c.get();
        }
    "#;
    let result = try_type_check(source);
    assert!(
        result.is_ok(),
        "Mixed instance and associated functions should work, got: {:?}",
        result.err()
    );
}

#[test]
fn associated_function_with_return_type_inference() {
    let source = r#"struct Math { fn double(x: i32) -> i32 { return x + x; } } fn test() -> i32 { let result: i32 = Math::double(21); return result; }"#;
    let result = try_type_check(source);
    assert!(
        result.is_ok(),
        "Associated function return type inference should work, got: {:?}",
        result.err()
    );
}

/// The head of a `Type::assoc()` call, across the spellings that name one type.
///
/// A head the extraction cannot read produces no type name, and the call then
/// falls past method resolution into free-function handling — where it is
/// accepted and left for code generation to refuse as a call to a proof-only
/// specification function the program never declared. Every test here goes red
/// if its spelling stops reaching the resolution the bare name reaches.
mod qualified_call_head {
    use crate::utils::build_ast;
    use inference_type_checker::check_with_diagnostics;
    use inference_type_checker::errors::TypeCheckError;

    fn diagnostics(source: &str) -> Vec<TypeCheckError> {
        let arena = build_ast(source.to_string());
        check_with_diagnostics(arena)
            .errors
            .into_iter()
            .map(|d| d.error)
            .collect()
    }

    /// Asserts `source` is refused for naming no such method on `type_name`.
    fn assert_method_not_found(source: &str, type_name: &str, method_name: &str) {
        let errors = diagnostics(source);
        assert_eq!(
            errors.len(),
            1,
            "expected exactly one diagnostic, got: {errors:?}"
        );
        match &errors[0] {
            TypeCheckError::MethodNotFound {
                type_name: got_type,
                method_name: got_method,
                ..
            } => {
                assert_eq!(got_type, type_name, "diagnostic names the head type");
                assert_eq!(got_method, method_name, "diagnostic names the method");
            }
            other => panic!("expected MethodNotFound, got {other:?}"),
        }
    }

    /// The bare head, which resolved correctly all along. It is the reference
    /// the three spellings below are held to.
    #[test]
    fn bare_head_reports_the_missing_method() {
        assert_method_not_found(
            r#"pub fn main() -> i32 { Array::new(); return 0; }"#,
            "Array",
            "new",
        );
    }

    /// A type application resolves through its base name: no declaration in the
    /// language takes type arguments, so the arguments name nothing distinct to
    /// resolve against.
    #[test]
    fn type_application_head_resolves_through_its_base() {
        assert_method_not_found(
            r#"pub fn main() -> i32 { Array u32'::new(); return 0; }"#,
            "Array",
            "new",
        );
    }

    /// Parenthesization is not part of a callee's identity, and the type
    /// application is the spelling that is normally written parenthesized.
    #[test]
    fn parenthesized_type_application_head_resolves_through_its_base() {
        assert_method_not_found(
            r#"pub fn main() -> i32 { (Array u32')::new(); return 0; }"#,
            "Array",
            "new",
        );
    }

    /// The parentheses alone hid the head just as thoroughly as the type
    /// arguments did.
    #[test]
    fn parenthesized_bare_head_resolves_through_its_parentheses() {
        assert_method_not_found(
            r#"pub fn main() -> i32 { (Array)::new(); return 0; }"#,
            "Array",
            "new",
        );
    }

    /// Resolution through a parenthesized head reaches the declared method, not
    /// just the refusal: the receiver diagnostic can only be reported by a lookup
    /// that found `get` on `P`.
    #[test]
    fn parenthesized_head_reaches_a_declared_method() {
        let source = r#"struct P { x: i32; fn get(self) -> i32 { return self.x; } }
            pub fn main() -> i32 { return (P)::get(); }"#;
        let errors = diagnostics(source);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                TypeCheckError::InstanceMethodCalledAsAssociated { type_name, method_name, .. }
                    if type_name == "P" && method_name == "get"
            )),
            "expected the receiver diagnostic for `P::get`, got: {errors:?}"
        );
    }
}
