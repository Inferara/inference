//! Tests for TypeInfoKind::Error (Error Type Poisoning)
//!
//! These tests verify that the type checker:
//! 1. Uses TypeInfoKind::Error to represent failed type lookups
//! 2. Suppresses cascading errors when operating on Error types
//! 3. Propagates Error types through expressions without spurious errors
//! 4. Continues type checking gracefully after encountering Error types
//!
//! The error poisoning feature is inspired by rustc's TyKind::Error model.

#[cfg(test)]
mod error_type_poisoning_tests {
    use crate::utils::build_ast;
    use inference_type_checker::type_info::{TypeInfo, TypeInfoKind};
    use inference_type_checker::TypeCheckerBuilder;

    fn try_type_check(
        source: &str,
    ) -> anyhow::Result<inference_type_checker::typed_context::TypedContext> {
        let arena = build_ast(source.to_string());
        Ok(TypeCheckerBuilder::build_typed_context(arena)?.typed_context())
    }

    mod type_info_error_variant {
        use super::*;

        #[test]
        fn test_error_type_is_error() {
            let error_type = TypeInfo::error("test error");
            assert!(error_type.is_error());
        }

        #[test]
        fn test_non_error_types_are_not_error() {
            let types = vec![
                TypeInfo::boolean(),
                TypeInfo::string(),
                TypeInfo::default(),
                TypeInfo {
                    kind: TypeInfoKind::Struct("Point".to_string()),
                    type_params: vec![],
                },
            ];
            for ty in types {
                assert!(
                    !ty.is_error(),
                    "Expected {:?} to not be an error type",
                    ty.kind
                );
            }
        }

        #[test]
        fn test_error_type_display() {
            let error_type = TypeInfo::error("test message");
            // Error types now display without message (unit variant)
            assert_eq!(error_type.to_string(), "{error}");
        }

        #[test]
        fn test_error_type_has_no_unresolved_params() {
            let error_type = TypeInfo::error("test error");
            assert!(!error_type.has_unresolved_params());
        }

        #[test]
        fn test_error_type_substitute_unchanged() {
            use rustc_hash::FxHashMap;

            let error_type = TypeInfo::error("test error");
            let mut subs = FxHashMap::default();
            subs.insert("T".to_string(), TypeInfo::boolean());

            let result = error_type.substitute(&subs);
            assert!(result.is_error());
        }
    }

    mod cascading_error_suppression {
        use super::*;

        #[test]
        fn test_undeclared_variable_no_cascading_type_mismatch() {
            // When a variable is undeclared, we should NOT also get a type mismatch error
            let source = r#"fn test() -> i32 { let x: i32 = unknown_var; return x; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect undeclared variable");

            if let Err(error) = result {
                let error_msg = error.to_string();
                let errors: Vec<&str> = error_msg.split("; ").collect();
                assert_eq!(
                    errors.len(),
                    1,
                    "Should produce exactly 1 error (undeclared variable), got: {:?}",
                    errors
                );
                assert!(
                    error_msg.contains("unknown_var"),
                    "Should report undeclared variable: {}",
                    error_msg
                );
            }
        }

        #[test]
        fn test_undefined_struct_no_cascading_field_access_errors() {
            // When a struct type is unknown, field access should not produce additional errors
            let source = r#"fn test(s: UndefinedStruct) -> i32 { return s.field; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect undefined struct");

            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("UndefinedStruct") || error_msg.contains("unknown type"),
                    "Should report undefined struct: {}",
                    error_msg
                );
                // The field access should not produce a separate "expected struct type" error
                // because the Error type prevents this cascade
            }
        }

        #[test]
        fn test_undefined_function_no_cascading_return_type_errors() {
            // When a function is undefined, we should not get cascading return type mismatch
            let source = r#"fn test() -> i32 { return undefined_func(); }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect undefined function");

            if let Err(error) = result {
                let error_msg = error.to_string();
                let errors: Vec<&str> = error_msg.split("; ").collect();
                assert_eq!(
                    errors.len(),
                    1,
                    "Should produce exactly 1 error (undefined function), got: {:?}",
                    errors
                );
                assert!(
                    error_msg.contains("undefined_func"),
                    "Should report undefined function: {}",
                    error_msg
                );
            }
        }

        #[test]
        fn test_binary_operation_with_error_type_no_cascading() {
            // Binary operation with one undeclared operand should not cause cascading errors
            let source = r#"fn test() -> i32 { return unknown_var + 10; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect undeclared variable");

            if let Err(error) = result {
                let error_msg = error.to_string();
                let errors: Vec<&str> = error_msg.split("; ").collect();
                assert_eq!(
                    errors.len(),
                    1,
                    "Should produce exactly 1 error (undeclared variable), got: {:?}",
                    errors
                );
                assert!(
                    error_msg.contains("unknown_var"),
                    "Should report undeclared variable: {}",
                    error_msg
                );
            }
        }

        #[test]
        fn test_array_index_with_error_array_no_cascading() {
            // Array indexing with undeclared array should not cause cascading errors
            let source = r#"fn test() -> i32 { return unknown_arr[0]; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect undeclared variable");

            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("unknown_arr"),
                    "Should report undeclared variable: {}",
                    error_msg
                );
                // Should NOT report "expected array type" for the indexing
                // because the array expression is Error type
            }
        }

        #[test]
        fn test_unary_operation_with_error_type_no_cascading() {
            // Unary operation on undeclared variable should not cascade
            let source = r#"fn test() -> i32 { return -unknown_var; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect undeclared variable");

            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("unknown_var"),
                    "Should report undeclared variable: {}",
                    error_msg
                );
            }
        }

        #[test]
        fn test_if_condition_with_error_type_no_cascading() {
            // If condition using undeclared variable should not cascade
            let source = r#"fn test() -> i32 { if unknown_cond { return 1; } return 0; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect undeclared variable");

            if let Err(error) = result {
                let error_msg = error.to_string();
                let errors: Vec<&str> = error_msg.split("; ").collect();
                assert_eq!(
                    errors.len(),
                    1,
                    "Should produce exactly 1 error (undeclared variable), got: {:?}",
                    errors
                );
                assert!(
                    error_msg.contains("unknown_cond"),
                    "Should report undeclared variable: {}",
                    error_msg
                );
            }
        }

        #[test]
        fn test_loop_condition_with_error_type_no_cascading() {
            // Loop condition using undeclared variable should not cascade
            let source = r#"fn test() -> i32 { loop unknown_cond { break; } return 0; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect undeclared variable");

            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("unknown_cond"),
                    "Should report undeclared variable: {}",
                    error_msg
                );
            }
        }
    }

    mod error_propagation {
        use super::*;

        #[test]
        fn test_error_propagates_through_assignment() {
            // Error type on RHS should not produce type mismatch with LHS
            let source =
                r#"fn test() -> i32 { let x: i32 = unknown_var; let y: bool = x; return 0; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect errors");

            if let Err(error) = result {
                let error_msg = error.to_string();
                // The first error is undeclared variable
                assert!(
                    error_msg.contains("unknown_var"),
                    "Should report undeclared variable: {}",
                    error_msg
                );
            }
        }

        #[test]
        fn test_error_in_struct_initialization() {
            // Using undefined struct should yield Error type and not cascade
            let source = r#"fn test() -> i32 { let p: UndefinedStruct = UndefinedStruct { x: 10 }; return 0; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect undefined struct");

            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("UndefinedStruct"),
                    "Should report undefined struct: {}",
                    error_msg
                );
            }
        }

        #[test]
        fn test_chained_member_access_with_error() {
            // Chained member access starting with undefined should not cascade
            let source = r#"fn test() -> i32 { return unknown_var.field1.field2; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect undeclared variable");

            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("unknown_var"),
                    "Should report undeclared variable: {}",
                    error_msg
                );
                // Should not have multiple cascading field access errors
            }
        }

        #[test]
        fn test_method_call_on_error_type_no_cascade() {
            // Method call on undeclared receiver should not produce cascading errors
            let source = r#"fn test() -> i32 { return unknown_obj.method(); }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect undeclared variable");

            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("unknown_obj"),
                    "Should report undeclared variable: {}",
                    error_msg
                );
            }
        }

        #[test]
        fn test_error_in_function_argument() {
            // Passing undefined variable as function argument
            let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; } fn test() -> i32 { return add(unknown_var, 10); }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect undeclared variable");

            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("unknown_var"),
                    "Should report undeclared variable: {}",
                    error_msg
                );
            }
        }

        #[test]
        fn test_multiple_error_sources_independent() {
            // Multiple independent errors should all be reported
            let source = r#"fn test() -> i32 { let x: i32 = unknown1; let y: i32 = unknown2; return x + y; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect undeclared variables");

            if let Err(error) = result {
                let error_msg = error.to_string();
                // Both undeclared variables should be reported
                assert!(
                    error_msg.contains("unknown1") && error_msg.contains("unknown2"),
                    "Should report both undeclared variables: {}",
                    error_msg
                );
            }
        }
    }

    mod error_type_in_complex_expressions {
        use super::*;

        #[test]
        fn test_error_in_nested_binary_expressions() {
            // Nested binary expression with one error should not cascade
            let source = r#"fn test() -> i32 { return (unknown_var + 1) * 2; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect undeclared variable");

            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("unknown_var"),
                    "Should report undeclared variable: {}",
                    error_msg
                );
            }
        }

        #[test]
        fn test_error_in_array_literal_element() {
            // Array literal with undefined element
            let source =
                r#"fn test() -> i32 { let arr: [i32; 3] = [1, unknown_var, 3]; return arr[0]; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect undeclared variable");

            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("unknown_var"),
                    "Should report undeclared variable: {}",
                    error_msg
                );
            }
        }

        #[test]
        fn test_error_in_conditional_expression() {
            // Using undefined variable in if arms
            let source = r#"fn test() -> i32 { if true { return unknown_var; } return 0; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect undeclared variable");

            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("unknown_var"),
                    "Should report undeclared variable: {}",
                    error_msg
                );
            }
        }

        #[test]
        fn test_error_type_in_assert_condition() {
            // Assert with undefined variable should not cascade
            let source = r#"fn test() -> i32 { assert unknown_cond; return 0; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Should detect undeclared variable");

            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("unknown_cond"),
                    "Should report undeclared variable: {}",
                    error_msg
                );
            }
        }
    }

    mod error_type_equality {
        use super::*;

        #[test]
        fn test_error_types_are_equal() {
            // All error types are equal (unit variant without payload)
            let error1 = TypeInfo::error("message one");
            let error2 = TypeInfo::error("message two");
            let error3 = TypeInfo::error("different message");
            // All Error types should be equal regardless of the message passed in
            assert_eq!(error1, error2);
            assert_eq!(error1, error3);
            assert_eq!(error2, error3);
        }

        #[test]
        fn test_error_not_equal_to_other_types() {
            let error = TypeInfo::error("test error");
            assert_ne!(error, TypeInfo::boolean());
            assert_ne!(error, TypeInfo::string());
            assert_ne!(error, TypeInfo::default());
        }
    }
}
