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
    fn test_const_shadowing_in_inner_block() {
        let source = r#"
            fn test() {
                let x: i32 = 1;
                if true {
                    const x: i32 = 2;
                }
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Const shadowing variable in inner block should fail"
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
    fn test_struct_variable_shadowing() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn test() {
                let p: Point = Point { x: 1, y: 2 };
                if true {
                    let p: Point = Point { x: 3, y: 4 };
                }
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Struct variable shadowing in inner block should fail"
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

    #[test]
    fn test_parameter_shadowed_in_inner_block() {
        let source = r#"
            fn test(x: i32) {
                if true {
                    let x: i32 = 2;
                }
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Variable shadowing function parameter in inner block should fail"
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
}

mod field_validation {
    use super::*;

    #[test]
    fn test_missing_struct_field() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn test() {
                let p: Point = Point { x: 1 };
            }
        "#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Missing struct field should fail");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("missing field"),
                "Error should mention missing field: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_unknown_struct_field() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn test() {
                let p: Point = Point { x: 1, y: 2, z: 3 };
            }
        "#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Unknown struct field should fail");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("unknown field"),
                "Error should mention unknown field: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_duplicate_struct_field() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn test() {
                let p: Point = Point { x: 1, x: 2, y: 3 };
            }
        "#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Duplicate struct field should fail");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("duplicate field"),
                "Error should mention duplicate field: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_all_fields_present_ok() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn test() {
                let p: Point = Point { x: 1, y: 2 };
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "All fields present should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_struct_field_type_mismatch_bool_for_i32() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn test() { let p: Point = Point { x: true, y: 2 }; }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Bool for i32 struct field should fail"
        );
    }

    #[test]
    fn test_struct_field_type_mismatch_number_for_bool() {
        let source = r#"
            struct Flags { active: bool; count: i32; }
            fn test() { let f: Flags = Flags { active: 42, count: 1 }; }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Number literal for bool struct field should fail"
        );
    }

    #[test]
    fn test_struct_field_correct_types_ok() {
        let source = r#"
            struct Flags { active: bool; count: i32; }
            fn test() { let f: Flags = Flags { active: true, count: 1 }; }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Correct struct field types should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_nested_struct_field_not_supported() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            struct Rect { origin: Point; size: Point; }
            fn test() -> i32 {
                let r: Rect = Rect { origin: Point { x: 0, y: 0 }, size: Point { x: 10, y: 20 } };
                return 0;
            }
        "#;
        let result = try_type_check(source);
        // Nested structs are not yet supported. The type checker rejects struct-typed fields
        // because struct literal field type matching does not handle struct types correctly
        // (reports "expected Point, found Point"). Codegen would also panic in element_size.
        // This test documents the current limitation at both levels.
        assert!(
            result.is_err(),
            "Nested structs are not yet supported; type checker should reject struct-typed fields"
        );
    }

    #[test]
    fn test_struct_literal_as_argument_passes_type_checker() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn takes_point(p: Point) -> i32 { return p.x; }
            fn test() -> i32 { return takes_point(Point { x: 1, y: 2 }); }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "struct literal as argument check migrated to analysis (A012), got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_struct_literal_as_standalone_expression_passes_type_checker() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn test() { Point { x: 1, y: 2 }; }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "compound literal position check migrated to analysis (A015), got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_struct_literal_in_binary_op() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn test() -> i32 {
                let x: i32 = 1;
                return x + Point { x: 1, y: 2 };
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Struct literal in binary expression should fail"
        );
    }

    #[test]
    fn test_struct_literal_in_let_ok() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn test() -> i32 { let p: Point = Point { x: 1, y: 2 }; return p.x; }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Struct literal in let binding should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_struct_literal_in_assign_ok() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn test() -> i32 {
                let mut p: Point = Point { x: 1, y: 2 };
                p = Point { x: 3, y: 4 };
                return p.x;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Struct literal in assignment should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_struct_literal_in_return_ok() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn make() -> Point { return Point { x: 1, y: 2 }; }
            pub fn test() -> i32 { return 0; }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Struct literal in return should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_struct_return_call_in_expression_position_passes_type_checker() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn make() -> Point { return Point { x: 1, y: 2 }; }
            fn takes_point(p: Point) -> i32 { return p.x; }
            fn test() -> i32 { return takes_point(make()); }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "compound return call position check migrated to analysis (A016), got: {:?}",
            result.err()
        );
    }
}

mod method_call_chain {
    //! Method call chain on compound-returning functions has been migrated to
    //! analysis rule A018. These tests verify the type checker accepts them.
    use super::*;

    #[test]
    fn method_chain_on_compound_return_passes_type_checker() {
        let source = r#"
            struct Point { x: i32; y: i32;
                fn translate(self, dx: i32, dy: i32) -> Point {
                    return Point { x: self.x + dx, y: self.y + dy };
                }
                fn get_x(self) -> i32 { return self.x; }
            }
            fn test() -> i32 {
                let p: Point = Point { x: 10, y: 20 };
                return p.translate(5, 3).get_x();
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "method chain on compound return check migrated to analysis (A018), got: {:?}",
            result.err()
        );
    }

    #[test]
    fn method_chain_on_compound_return_with_unknown_var_still_fails() {
        let source = r#"
            struct Point { x: i32; y: i32;
                fn translate(self, dx: i32, dy: i32) -> Point {
                    return Point { x: self.x + dx, y: self.y + dy };
                }
                fn get_x(self) -> i32 { return self.x; }
            }
            fn test() -> i32 {
                let p: Point = Point { x: 10, y: 20 };
                return p.translate(5, 3).get_x(unknown_var);
            }
        "#;
        let Err(error) = try_type_check(source) else {
            panic!("undefined variable should still be caught by type checker");
        };
        let error_msg = error.to_string();
        assert!(
            error_msg.contains("unknown_var"),
            "Should report the undefined variable: {error_msg}"
        );
    }

    #[test]
    fn method_chain_on_associated_function_return_passes_type_checker() {
        let source = r#"
            struct Point { x: i32; y: i32;
                fn new(x: i32, y: i32) -> Point {
                    return Point { x: x, y: y };
                }
                fn get_x(self) -> i32 { return self.x; }
            }
            fn test() -> i32 {
                return Point::new(1, 2).get_x();
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "method chain on compound return check migrated to analysis (A018), got: {:?}",
            result.err()
        );
    }
}

mod compound_return_call_in_assignment {
    //! Compound return call in assignment has been migrated to analysis rule A017.
    //! These tests verify the type checker accepts them.
    use super::*;

    #[test]
    fn plain_function_returning_struct_passes_type_checker() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn make_point(x: i32, y: i32) -> Point {
                return Point { x: x, y: y };
            }
            fn test() {
                let mut p: Point = Point { x: 0, y: 0 };
                p = make_point(1, 2);
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "compound return call assignment check migrated to analysis (A017), got: {:?}",
            result.err()
        );
    }

    #[test]
    fn instance_method_returning_struct_passes_type_checker() {
        let source = r#"
            struct Point { x: i32; y: i32;
                fn translate(self, dx: i32, dy: i32) -> Point {
                    return Point { x: self.x + dx, y: self.y + dy };
                }
            }
            fn test() {
                let mut p: Point = Point { x: 1, y: 2 };
                p = p.translate(5, 3);
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "compound return call assignment check migrated to analysis (A017), got: {:?}",
            result.err()
        );
    }

    #[test]
    fn associated_function_returning_struct_passes_type_checker() {
        let source = r#"
            struct Point { x: i32; y: i32;
                fn new(x: i32, y: i32) -> Point {
                    return Point { x: x, y: y };
                }
            }
            fn test() {
                let mut p: Point = Point { x: 0, y: 0 };
                p = Point::new(1, 2);
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "compound return call assignment check migrated to analysis (A017), got: {:?}",
            result.err()
        );
    }
}

mod compound_return_call_in_expression_position {
    use super::*;

    #[test]
    fn instance_method_returning_struct_as_standalone_passes_type_checker() {
        let source = r#"
            struct Point {
                x: i32; y: i32;
                fn translate(self, dx: i32, dy: i32) -> Point {
                    let p: Point = Point { x: self.x + dx, y: self.y + dy };
                    return p;
                }
            }
            fn test(p: Point) {
                p.translate(5, 3);
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "compound return call position check migrated to analysis (A016), got: {:?}",
            result.err()
        );
    }

    #[test]
    fn associated_function_returning_struct_as_standalone_passes_type_checker() {
        let source = r#"
            struct Point {
                x: i32; y: i32;
                fn new(x: i32, y: i32) -> Point {
                    let p: Point = Point { x: x, y: y };
                    return p;
                }
            }
            fn test() {
                Point::new(1, 2);
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "compound return call position check migrated to analysis (A016), got: {:?}",
            result.err()
        );
    }

    #[test]
    fn instance_method_returning_struct_as_argument_passes_type_checker() {
        let source = r#"
            struct Point {
                x: i32; y: i32;
                fn translate(self, dx: i32, dy: i32) -> Point {
                    let p: Point = Point { x: self.x + dx, y: self.y + dy };
                    return p;
                }
            }
            fn consume(p: Point) -> i32 { return p.x; }
            fn test(p: Point) -> i32 {
                return consume(p.translate(5, 3));
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "compound return call position check migrated to analysis (A016), got: {:?}",
            result.err()
        );
    }

    #[test]
    fn associated_function_returning_struct_as_argument_passes_type_checker() {
        let source = r#"
            struct Point {
                x: i32; y: i32;
                fn new(x: i32, y: i32) -> Point {
                    let p: Point = Point { x: x, y: y };
                    return p;
                }
            }
            fn consume(p: Point) -> i32 { return p.x; }
            fn test() -> i32 {
                return consume(Point::new(1, 2));
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "compound return call position check migrated to analysis (A016), got: {:?}",
            result.err()
        );
    }

    #[test]
    fn instance_method_returning_primitive_as_standalone_is_ok() {
        let source = r#"
            struct Point {
                x: i32; y: i32;
                fn get_x(self) -> i32 { return self.x; }
            }
            fn test(p: Point) {
                p.get_x();
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Instance method returning primitive as standalone should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn associated_function_returning_primitive_as_standalone_is_ok() {
        let source = r#"
            struct Math {
                fn add(a: i32, b: i32) -> i32 { return a + b; }
            }
            fn test() {
                Math::add(1, 2);
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Associated function returning primitive as standalone should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn instance_method_returning_struct_in_let_binding_is_ok() {
        let source = r#"
            struct Point {
                x: i32; y: i32;
                fn translate(self, dx: i32, dy: i32) -> Point {
                    let p: Point = Point { x: self.x + dx, y: self.y + dy };
                    return p;
                }
            }
            fn test(p: Point) {
                let q: Point = p.translate(5, 3);
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Instance method returning struct in let binding should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn associated_function_returning_struct_in_let_binding_is_ok() {
        let source = r#"
            struct Point {
                x: i32; y: i32;
                fn new(x: i32, y: i32) -> Point {
                    let p: Point = Point { x: x, y: y };
                    return p;
                }
            }
            fn test() {
                let p: Point = Point::new(1, 2);
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Associated function returning struct in let binding should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn instance_method_returning_struct_in_return_is_ok() {
        let source = r#"
            struct Point {
                x: i32; y: i32;
                fn translate(self, dx: i32, dy: i32) -> Point {
                    let p: Point = Point { x: self.x + dx, y: self.y + dy };
                    return p;
                }
            }
            fn test(p: Point) -> Point {
                return p.translate(5, 3);
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Instance method returning struct in return should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn associated_function_returning_struct_in_return_is_ok() {
        let source = r#"
            struct Point {
                x: i32; y: i32;
                fn new(x: i32, y: i32) -> Point {
                    let p: Point = Point { x: x, y: y };
                    return p;
                }
            }
            fn test() -> Point {
                return Point::new(1, 2);
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Associated function returning struct in return should succeed, got: {:?}",
            result.err()
        );
    }
}
