//! Struct mutability and variable shadowing tests

use crate::utils::build_ast;
use inference_type_checker::TypeCheckerBuilder;

fn try_type_check(
    source: &str,
) -> anyhow::Result<inference_type_checker::typed_context::TypedContext> {
    let arena = build_ast(source.to_string());
    Ok(TypeCheckerBuilder::build_typed_context(arena)?.typed_context())
}

mod mutability {
    use super::*;

    #[test]
    fn test_struct_field_assign_immutable_variable() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn test() {
                let p: Point = Point { x: 1, y: 2 };
                p.x = 42;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Assignment to field of immutable struct variable should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("cannot assign to immutable variable"),
                "Error should mention immutable variable: {}",
                error_msg
            );
            assert!(
                error_msg.contains("p"),
                "Error should mention the variable name: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_struct_field_assign_mutable_variable() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn test() {
                let mut p: Point = Point { x: 1, y: 2 };
                p.x = 42;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Assignment to field of mutable struct variable should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_struct_field_assign_immutable_parameter() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn test(p: Point) {
                p.x = 42;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Assignment to field of immutable parameter should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("cannot assign to immutable variable"),
                "Error should mention immutable variable: {}",
                error_msg
            );
            assert!(
                error_msg.contains("p"),
                "Error should mention the parameter name: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_struct_field_assign_mutable_parameter() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn test(mut p: Point) {
                p.x = 42;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Assignment to field of mutable parameter should succeed, got: {:?}",
            result.err()
        );
    }
}

mod shadowing {
    use super::*;

    #[test]
    fn test_variable_shadowing_in_inner_block() {
        let source = r#"
            fn test() {
                let x: i32 = 1;
                if true {
                    let x: i32 = 2;
                }
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Variable shadowing in inner block should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("shadows a binding"),
                "Error should mention shadowing: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_variable_shadowing_in_if_block() {
        let source = r#"
            fn test() {
                let x: i32 = 1;
                if true {
                    let x: i32 = 2;
                } else {
                    let x: i32 = 3;
                }
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Variable shadowing in if/else blocks should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("shadows a binding"),
                "Error should mention shadowing: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_variable_shadowing_in_loop() {
        let source = r#"
            fn test() {
                let x: i32 = 1;
                loop 5 {
                    let x: i32 = 2;
                    break;
                }
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Variable shadowing in loop should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("shadows a binding"),
                "Error should mention shadowing: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_no_shadowing_same_name_different_functions() {
        let source = r#"
            fn foo() {
                let x: i32 = 1;
            }
            fn bar() {
                let x: i32 = 2;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Same variable name in different functions should not be shadowing, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_no_shadowing_sequential_blocks() {
        let source = r#"
            fn test() {
                if true {
                    let x: i32 = 1;
                }
                if true {
                    let x: i32 = 2;
                }
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Same variable name in sequential blocks should not be shadowing, got: {:?}",
            result.err()
        );
    }
}
