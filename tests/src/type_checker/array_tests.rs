//! Array type annotation tests
//!
//! Tests verifying that array type annotations correctly preserve size information.

use crate::utils::build_ast;
use inference_type_checker::TypeCheckerBuilder;

fn try_type_check(
    source: &str,
) -> anyhow::Result<inference_type_checker::typed_context::TypedContext> {
    let arena = build_ast(source.to_string());
    Ok(TypeCheckerBuilder::build_typed_context(arena)?.typed_context())
}

mod edge_cases {
    use super::*;

    #[test]
    fn test_empty_array_annotation_rejected() {
        let source = r#"fn test() -> i32 { let arr: [i32; 0] = []; return 42; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Array with size 0 should be rejected");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("invalid array size"),
                "Error should mention invalid array size: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_large_array_annotation() {
        let source = r#"fn test() -> i32 { let arr: [i32; 1000]; return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Large array (size 1000) annotation should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_very_large_array_annotation() {
        let source = r#"fn test() -> i32 { let arr: [i32; 65535]; return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Very large array (size 65535) annotation should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_nested_array_annotation() {
        let source = r#"fn test() -> i32 { let arr: [[i32; 2]; 3]; return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Nested array [[i32; 2]; 3] annotation should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_deeply_nested_array_annotation() {
        let source = r#"fn test() -> i32 { let arr: [[[i32; 2]; 3]; 4]; return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Deeply nested array [[[i32; 2]; 3]; 4] annotation should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_array_of_bool_annotation() {
        let source = r#"fn test() -> bool { let arr: [bool; 5] = [true, false, true, false, true]; return arr[0]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Array of bool with size should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_array_of_different_number_types() {
        let source = r#"
            fn test() -> i32 {
                let arr_i8: [i8; 2];
                let arr_i16: [i16; 2];
                let arr_i32: [i32; 2];
                let arr_i64: [i64; 2];
                let arr_u8: [u8; 2];
                let arr_u16: [u16; 2];
                let arr_u32: [u32; 2];
                let arr_u64: [u64; 2];
                return 42;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Arrays of all number types with sizes should work, got: {:?}",
            result.err()
        );
    }
}

mod element_type_propagation {
    use super::*;

    #[test]
    fn test_i64_array_literal() {
        let source = r#"fn test() -> i64 { let arr: [i64; 2] = [100, 200]; return arr[1]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "i64 array with number literal elements should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_u64_array_literal() {
        let source = r#"fn test() -> u64 { let arr: [u64; 3] = [10, 20, 30]; return arr[0]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "u64 array with number literal elements should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_i8_array_literal() {
        let source = r#"fn test() -> i8 { let arr: [i8; 2] = [1, 2]; return arr[0]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "i8 array with number literal elements should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_u32_array_literal() {
        let source = r#"fn test() -> u32 { let arr: [u32; 2] = [42, 84]; return arr[0]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "u32 array with number literal elements should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_i32_array_literal_still_works() {
        let source = r#"fn test() -> i32 { let arr: [i32; 3] = [1, 2, 3]; return arr[0]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "i32 array should still work after fix, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_i64_array_literal_declaration_only() {
        let source = r#"fn test() -> i32 { let arr: [i64; 2] = [100, 200]; return 0; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "i64 array literal declaration should pass type checking, got: {:?}",
            result.err()
        );
    }
}

mod function_parameters {
    use super::*;

    #[test]
    fn test_function_param_sized_array() {
        let source = r#"fn process(arr: [i32; 5]) -> i32 { return arr[0]; } fn test() -> i32 { let arr: [i32; 5] = [1, 2, 3, 4, 5]; return process(arr); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Function with sized array parameter should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_function_return_sized_array_accepted() {
        let source = r#"fn create_array() -> [i32; 3] { return [1, 2, 3]; } fn test() -> i32 { let arr: [i32; 3] = create_array(); return arr[0]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Function returning array should be accepted, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_function_nested_array_param() {
        let source = r#"fn process(matrix: [[i32; 2]; 3]) -> i32 { return matrix[0][0]; } fn test() -> i32 { let matrix: [[i32; 2]; 3]; return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Function with nested array parameter should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_multiple_array_params_different_sizes() {
        let source = r#"fn process(a: [i32; 2], b: [i32; 3], c: [i32; 5]) -> i32 { return a[0] + b[0] + c[0]; } fn test() -> i32 { return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Function with multiple differently-sized array parameters should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_function_param_and_return_sized_arrays_accepted() {
        let source = r#"fn transform(input: [i32; 3]) -> [i32; 3] { return input; } fn test() -> i32 { return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Function with array param and return should be accepted, got: {:?}",
            result.err()
        );
    }
}

mod type_mismatches {
    use super::*;

    #[test]
    fn test_array_size_mismatch_too_few_elements() {
        let source = r#"fn test() -> i32 { let arr: [i32; 3] = [1, 2]; return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Array with fewer elements than size annotation should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("type mismatch") || error_msg.contains("array"),
                "Error should mention type mismatch or array: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_array_size_mismatch_too_many_elements() {
        let source = r#"fn test() -> i32 { let arr: [i32; 2] = [1, 2, 3]; return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Array with more elements than size annotation should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("type mismatch") || error_msg.contains("array"),
                "Error should mention type mismatch or array: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_array_element_type_mismatch() {
        let source = r#"fn test() -> i32 { let arr: [i32; 3] = [1, 2, true]; return 42; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Array with wrong element type should fail");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("type mismatch") || error_msg.contains("array"),
                "Error should mention type mismatch or array: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_nested_array_size_mismatch() {
        let source =
            r#"fn test() -> i32 { let arr: [[i32; 2]; 3] = [[1, 2], [3, 4]]; return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Nested array with size mismatch should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("type mismatch") || error_msg.contains("array"),
                "Error should mention type mismatch or array: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_nested_array_inner_size_mismatch() {
        let source =
            r#"fn test() -> i32 { let arr: [[i32; 2]; 2] = [[1, 2], [3, 4, 5]]; return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Nested array with inner array size mismatch should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("type mismatch") || error_msg.contains("array"),
                "Error should mention type mismatch or array: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_array_wrong_element_type_all_same() {
        let source = r#"fn test() -> i32 { let arr: [i32; 3] = [true, false, true]; return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Array with all wrong element types should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("type mismatch")
                    || error_msg.contains("bool")
                    || error_msg.contains("i32"),
                "Error should mention type mismatch with types: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_function_param_array_size_mismatch() {
        let source = r#"fn process(arr: [i32; 5]) -> i32 { return arr[0]; } fn test() -> i32 { let arr: [i32; 3] = [1, 2, 3]; return process(arr); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Array size mismatch in function args should be detected"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("type mismatch"),
                "Error should mention type mismatch: {}",
                error_msg
            );
            assert!(
                error_msg.contains("[i32; 5]") && error_msg.contains("[i32; 3]"),
                "Error should mention both array types: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_function_param_array_size_mismatch_larger() {
        let source = r#"fn process(arr: [i32; 3]) -> i32 { return arr[0]; } fn test() -> i32 { let arr: [i32; 5] = [1, 2, 3, 4, 5]; return process(arr); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Passing larger array to smaller parameter should be detected"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("[i32; 3]") && error_msg.contains("[i32; 5]"),
                "Error should mention both array sizes: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_function_param_array_element_type_mismatch() {
        let source = r#"fn process(arr: [i32; 3]) -> i32 { return arr[0]; } fn test() -> i32 { let arr: [i64; 3] = [1, 2, 3]; return process(arr); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Passing array with wrong element type should be detected"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("type mismatch"),
                "Error should mention type mismatch: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_function_param_array_correct_size() {
        let source = r#"fn process(arr: [i32; 3]) -> i32 { return arr[0]; } fn test() -> i32 { let arr: [i32; 3] = [1, 2, 3]; return process(arr); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Passing array with matching size should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_function_multiple_array_params_size_mismatch() {
        let source = r#"fn process(a: [i32; 2], b: [i32; 3]) -> i32 { return a[0] + b[0]; } fn test() -> i32 { let x: [i32; 2] = [1, 2]; let y: [i32; 2] = [3, 4]; return process(x, y); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Second array arg with wrong size should be detected"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("[i32; 3]") && error_msg.contains("[i32; 2]"),
                "Error should mention mismatched array sizes: {}",
                error_msg
            );
            assert!(
                error_msg.contains("argument 1"),
                "Error should indicate it is the second argument: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_function_param_scalar_instead_of_array() {
        let source = r#"fn process(arr: [i32; 3]) -> i32 { return arr[0]; } fn test() -> i32 { let x: i32 = 42; return process(x); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Passing scalar where array expected should be detected"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("type mismatch"),
                "Error should mention type mismatch: {}",
                error_msg
            );
        }
    }
}

mod array_indexing {
    use super::*;

    #[test]
    fn test_array_index_returns_element_type() {
        let source = r#"fn test() -> i32 { let arr: [i32; 5] = [1, 2, 3, 4, 5]; let elem: i32 = arr[0]; return elem; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Array indexing should return correct element type, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_nested_array_index_returns_inner_array_type() {
        let source = r#"fn test() -> i32 { let arr: [[i32; 2]; 3]; let inner: [i32; 2] = arr[0]; return inner[0]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Nested array indexing should return correct inner array type, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_nested_array_double_index() {
        let source = r#"fn test() -> i32 { let arr: [[i32; 2]; 3] = [[1, 2], [3, 4], [5, 6]]; let elem: i32 = arr[0][0]; return elem; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Double indexing nested array should return element type, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_array_index_with_different_numeric_indices() {
        let source = r#"fn test() -> i32 { let arr: [i32; 10]; let idx: i32 = 0; let elem: i32 = arr[idx]; return elem; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Array indexing with numeric index should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_array_index_wrong_type_assignment() {
        let source = r#"fn test() -> i32 { let arr: [i32; 5] = [1, 2, 3, 4, 5]; let elem: bool = arr[0]; return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Assigning array element to wrong type should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("type mismatch")
                    || error_msg.contains("bool")
                    || error_msg.contains("i32"),
                "Error should mention type mismatch: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_nested_array_index_wrong_inner_type() {
        let source = r#"fn test() -> i32 { let arr: [[i32; 2]; 3]; let inner: [bool; 2] = arr[0]; return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Assigning nested array inner to wrong type should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("type mismatch")
                    || error_msg.contains("bool")
                    || error_msg.contains("i32"),
                "Error should mention type mismatch: {}",
                error_msg
            );
        }
    }
}

mod comprehensive_scenarios {
    use super::*;

    #[test]
    fn test_multiple_arrays_different_sizes_same_type() {
        let source = r#"
            fn test() -> i32 {
                let arr1: [i32; 2] = [1, 2];
                let arr2: [i32; 3] = [3, 4, 5];
                let arr3: [i32; 5] = [6, 7, 8, 9, 10];
                return arr1[0] + arr2[0] + arr3[0];
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Multiple arrays with different sizes should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_array_in_struct_field() {
        let source = r#"
            struct Point {
                coords: [i32; 3];
            }
            fn test() -> i32 {
                let p: Point;
                return 42;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Struct with sized array field should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_array_assignment_preserves_size() {
        let source = r#"
            fn test() -> i32 {
                let arr1: [i32; 5] = [1, 2, 3, 4, 5];
                let mut arr2: [i32; 5];
                arr2 = arr1;
                return arr2[0];
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Array assignment should preserve size, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_array_assignment_size_mismatch() {
        let source = r#"
            fn test() -> i32 {
                let arr1: [i32; 5] = [1, 2, 3, 4, 5];
                let arr2: [i32; 3];
                arr2 = arr1;
                return 42;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Array assignment with size mismatch should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("type mismatch") || error_msg.contains("array"),
                "Error should mention type mismatch or array: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_empty_array_with_bool_type_rejected() {
        let source = r#"fn test() -> i32 { let arr: [bool; 0] = []; return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Array with size 0 should be rejected even for bool element type"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("invalid array size"),
                "Error should mention invalid array size: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_array_size_one_vs_element() {
        let source = r#"
            fn test() -> i32 {
                let single: [i32; 1] = [42];
                let scalar: i32 = 42;
                return scalar;
            }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Array of size 1 and scalar should be distinct types, got: {:?}",
            result.err()
        );
    }
}

mod mutability {
    use super::*;

    #[test]
    fn test_array_index_assign_immutable_variable() {
        let source = r#"fn test() -> i32 { let arr: [i32; 3] = [1, 2, 3]; arr[0] = 42; return arr[0]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Assignment to immutable array element should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("cannot assign to immutable variable"),
                "Error should mention immutable variable: {}",
                error_msg
            );
            assert!(
                error_msg.contains("arr"),
                "Error should mention the array variable name: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_array_index_assign_mutable_variable() {
        let source = r#"fn test() -> i32 { let mut arr: [i32; 3] = [1, 2, 3]; arr[0] = 42; return arr[0]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Assignment to mutable array element should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_array_index_assign_immutable_parameter() {
        let source = r#"fn test(arr: [i32; 3]) -> i32 { arr[0] = 42; return arr[0]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Assignment to immutable parameter array element should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("cannot assign to immutable variable"),
                "Error should mention immutable variable: {}",
                error_msg
            );
            assert!(
                error_msg.contains("arr"),
                "Error should mention the array parameter name: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_array_index_assign_mutable_parameter() {
        let source = r#"fn test(mut arr: [i32; 3]) -> i32 { arr[0] = 42; return arr[0]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Assignment to mutable parameter array element should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_array_index_assign_with_variable_index() {
        let source = r#"fn test() -> i32 { let arr: [i32; 3] = [1, 2, 3]; let i: i32 = 0; arr[i] = 42; return arr[0]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Assignment via variable index to immutable array should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("cannot assign to immutable variable"),
                "Error should mention immutable variable: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_array_index_assign_mutable_with_variable_index() {
        let source = r#"fn test() -> i32 { let mut arr: [i32; 3] = [1, 2, 3]; let i: i32 = 0; arr[i] = 42; return arr[0]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Assignment via variable index to mutable array should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_nested_array_index_assign_immutable() {
        let source = r#"fn test() -> i32 { let arr: [[i32; 2]; 2] = [[1, 2], [3, 4]]; arr[0][1] = 42; return arr[0][1]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Assignment to nested immutable array element should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("cannot assign to immutable variable"),
                "Error should mention immutable variable: {}",
                error_msg
            );
            assert!(
                error_msg.contains("arr"),
                "Error should mention the root array variable name: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_nested_array_index_assign_mutable() {
        let source = r#"fn test() -> i32 { let mut arr: [[i32; 2]; 2] = [[1, 2], [3, 4]]; arr[0][1] = 42; return arr[0][1]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Assignment to nested mutable array element should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_multiple_array_index_assignments_mutable() {
        let source = r#"fn test() -> i32 { let mut arr: [i32; 3] = [1, 2, 3]; arr[0] = 10; arr[1] = 20; arr[2] = 30; return arr[0]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Multiple assignments to mutable array elements should succeed, got: {:?}",
            result.err()
        );
    }
}

mod literal_range_validation {
    use super::*;

    #[test]
    fn test_out_of_range_i8_200() {
        let source = r#"fn test() -> i32 { let x: i8 = 200; return 0; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "i8 = 200 should be out of range");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("out of range"),
                "Error should mention out of range: {}",
                error_msg
            );
            assert!(
                error_msg.contains("i8"),
                "Error should mention i8: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_out_of_range_u8_256() {
        let source = r#"fn test() -> i32 { let x: u8 = 256; return 0; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "u8 = 256 should be out of range");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("out of range"),
                "Error should mention out of range: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_boundary_i8_127_accepted() {
        let source = r#"fn test() -> i32 { let x: i8 = 127; return 0; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "i8 = 127 should be in range, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_boundary_u8_255_accepted() {
        let source = r#"fn test() -> i32 { let x: u8 = 255; return 0; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "u8 = 255 should be in range, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_boundary_u8_0_accepted() {
        let source = r#"fn test() -> i32 { let x: u8 = 0; return 0; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "u8 = 0 should be in range, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_out_of_range_i16() {
        let source = r#"fn test() -> i32 { let x: i16 = 40000; return 0; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "i16 = 40000 should be out of range");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("out of range"),
                "Error should mention out of range: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_out_of_range_u16() {
        let source = r#"fn test() -> i32 { let x: u16 = 70000; return 0; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "u16 = 70000 should be out of range");
    }

    #[test]
    fn test_i32_max_accepted() {
        let source = r#"fn test() -> i32 { let x: i32 = 2147483647; return x; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "i32 max should be in range, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_i32_overflow() {
        let source = r#"fn test() -> i32 { let x: i32 = 2147483648; return 0; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "i32 = 2147483648 should be out of range");
    }

    #[test]
    fn test_array_element_out_of_range() {
        let source = r#"fn test() -> i32 { let arr: [u8; 3] = [255, 256, 0]; return 0; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Array element 256 for u8 should be out of range"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("out of range"),
                "Error should mention out of range: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_array_elements_in_range() {
        let source = r#"fn test() -> i32 { let arr: [u8; 3] = [0, 127, 255]; return 0; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "All elements in range should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_assign_literal_out_of_range() {
        let source = r#"fn test() -> i32 { let mut x: u8 = 0; x = 256; return 0; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Assigning 256 to u8 should be out of range"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("out of range"),
                "Error should mention out of range: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_assign_literal_in_range() {
        let source = r#"fn test() -> i32 { let mut x: u8 = 0; x = 255; return 0; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Assigning 255 to u8 should be in range, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_constant_out_of_range() {
        let source = r#"fn test() -> i32 { const x: u8 = 300; return 0; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Constant u8 = 300 should be out of range"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("out of range"),
                "Error should mention out of range: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_constant_in_range() {
        let source = r#"fn test() -> i32 { const x: u8 = 200; return 0; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Constant u8 = 200 should be in range, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_i128_overflow_i32() {
        let source =
            r#"fn test() -> i32 { let x: i32 = 99999999999999999999999999999999; return 0; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Literal exceeding i128 range should be rejected for i32"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("out of range"),
                "Error should mention out of range: {error_msg}"
            );
            assert!(
                error_msg.contains("i32"),
                "Error should mention target type i32: {error_msg}"
            );
        }
    }

    #[test]
    fn test_i128_overflow_u64() {
        let source = r#"fn test() -> i32 { let x: u64 = 999999999999999999999999999999999999999999; return 0; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Literal exceeding i128 range should be rejected for u64"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("out of range"),
                "Error should mention out of range: {error_msg}"
            );
            assert!(
                error_msg.contains("u64"),
                "Error should mention target type u64: {error_msg}"
            );
        }
    }

    #[test]
    fn test_i128_overflow_i8() {
        let source =
            r#"fn test() -> i32 { let x: i8 = 99999999999999999999999999999999; return 0; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Literal exceeding i128 range should be rejected for i8"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("out of range"),
                "Error should mention out of range: {error_msg}"
            );
            assert!(
                error_msg.contains("i8"),
                "Error should mention target type i8: {error_msg}"
            );
        }
    }

    #[test]
    fn test_i128_overflow_array_element() {
        let source = r#"fn test() -> i32 { let arr: [u8; 2] = [1, 99999999999999999999999999999999]; return 0; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Array element exceeding i128 range should be rejected"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("out of range"),
                "Error should mention out of range: {error_msg}"
            );
            assert!(
                error_msg.contains("u8"),
                "Error should mention target type u8: {error_msg}"
            );
        }
    }

    #[test]
    fn test_i128_overflow_constant() {
        let source =
            r#"fn test() -> i32 { const X: i32 = 99999999999999999999999999999999; return 0; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Constant exceeding i128 range should be rejected"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("out of range"),
                "Error should mention out of range: {error_msg}"
            );
            assert!(
                error_msg.contains("i32"),
                "Error should mention target type i32: {error_msg}"
            );
        }
    }

    #[test]
    fn test_i128_overflow_assignment() {
        let source = r#"fn test() -> i32 { let mut x: i32 = 0; x = 99999999999999999999999999999999; return 0; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Assignment exceeding i128 range should be rejected"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("out of range"),
                "Error should mention out of range: {error_msg}"
            );
        }
    }
}

mod array_return_type_validation {
    use super::*;

    #[test]
    fn test_array_return_accepted() {
        let source = r#"fn make() -> [i32; 3] { return [1, 2, 3]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Function returning array should be accepted, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_scalar_return_accepted() {
        let source = r#"fn id(x: i32) -> i32 { return x; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Function returning scalar should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_method_array_return_accepted() {
        let source =
            r#"struct Foo { x: i32; fn get_array(self) -> [i32; 2] { return [1, 2]; } }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Method returning array should be accepted, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_void_function_accepted() {
        let source = r#"fn noop() { }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Void function should work, got: {:?}",
            result.err()
        );
    }
}

mod array_literal_as_argument {
    use super::*;

    #[test]
    fn test_array_literal_as_arg_rejected() {
        let source = r#"fn sum(arr: [i32; 3]) -> i32 { return arr[0]; } fn test() -> i32 { return sum([1, 2, 3]); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Array literal as argument should be rejected"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("array literals cannot be passed directly"),
                "Error should mention array literals: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_array_variable_as_arg_accepted() {
        let source = r#"fn sum(arr: [i32; 3]) -> i32 { return arr[0]; } fn test() -> i32 { let a: [i32; 3] = [1, 2, 3]; return sum(a); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Array variable as argument should work, got: {:?}",
            result.err()
        );
    }
}

mod array_index_64bit {
    use super::*;

    #[test]
    fn test_i64_index_rejected() {
        let source = r#"fn test() -> i32 { let arr: [i32; 3] = [1, 2, 3]; let idx: i64 = 0; return arr[idx]; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "i64 array index should be rejected");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("32-bit integer type"),
                "Error should mention 32-bit requirement: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_u64_index_rejected() {
        let source = r#"fn test() -> i32 { let arr: [i32; 3] = [1, 2, 3]; let idx: u64 = 0; return arr[idx]; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "u64 array index should be rejected");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("32-bit integer type"),
                "Error should mention 32-bit requirement: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_i32_index_accepted() {
        let source = r#"fn test() -> i32 { let arr: [i32; 3] = [1, 2, 3]; let idx: i32 = 0; return arr[idx]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "i32 array index should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_u8_index_accepted() {
        let source = r#"fn test() -> i32 { let arr: [i32; 3] = [1, 2, 3]; let idx: u8 = 0; return arr[idx]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "u8 array index should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_u16_index_accepted() {
        let source = r#"fn test() -> i32 { let arr: [i32; 3] = [1, 2, 3]; let idx: u16 = 0; return arr[idx]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "u16 array index should work, got: {:?}",
            result.err()
        );
    }
}

mod invalid_array_size {
    use super::*;

    #[test]
    fn test_overflow_u32_in_array_size() {
        let source =
            r#"fn test() -> i32 { let arr: [i32; 999999999999999999] = [1]; return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Array size exceeding u32 should be rejected"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("invalid array size"),
                "Error should mention invalid array size: {}",
                error_msg
            );
            assert!(
                error_msg.contains("999999999999999999"),
                "Error should include the original size literal: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_u32_max_plus_one_in_array_size() {
        let source = r#"fn test() -> i32 { let arr: [i32; 4294967296] = [1]; return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Array size 4294967296 (u32::MAX + 1) should be rejected"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("invalid array size"),
                "Error should mention invalid array size: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_u32_max_in_array_size_accepted() {
        let source = r#"fn test() -> i32 { let arr: [i32; 4294967295]; return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Array size 4294967295 (u32::MAX) should be accepted, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_zero_array_size_rejected() {
        let source = r#"fn test() -> i32 { let arr: [i32; 0] = []; return 42; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Array size 0 should be rejected");
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("invalid array size"),
                "Error should mention invalid array size: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_very_large_overflow_in_array_size() {
        let source = r#"fn test() -> i32 { let arr: [i32; 99999999999999999999999999999999] = [1]; return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Extremely large array size should be rejected"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("invalid array size"),
                "Error should mention invalid array size: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_overflow_in_function_param_array_size() {
        let source = r#"fn process(arr: [i32; 999999999999999999]) -> i32 { return arr[0]; } fn test() -> i32 { return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Function param with overflowing array size should be rejected"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("invalid array size"),
                "Error should mention invalid array size: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_valid_array_sizes_still_work() {
        let source = r#"fn test() -> i32 { let a: [i32; 1] = [1]; let b: [i32; 3] = [1, 2, 3]; let c: [i32; 100]; return 42; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Valid array sizes should still work, got: {:?}",
            result.err()
        );
    }
}

mod array_return_call_position {
    use super::*;

    #[test]
    fn let_binding_accepted() {
        let source = r#"fn make() -> [i32; 3] { return [1, 2, 3]; } fn test() -> i32 { let a: [i32; 3] = make(); return a[0]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "let binding should be accepted, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn return_forwarding_accepted() {
        let source = r#"fn inner() -> [i32; 3] { return [1, 2, 3]; } fn outer() -> [i32; 3] { return inner(); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "return forwarding should be accepted, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn standalone_call_rejected() {
        let source = r#"fn make() -> [i32; 3] { return [1, 2, 3]; } fn test() -> i32 { make(); return 0; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "standalone sret call should be rejected");
        if let Err(error) = result {
            let msg = error.to_string();
            assert!(
                msg.contains("let") && msg.contains("return"),
                "Error should mention let/return: {msg}"
            );
        }
    }

    #[test]
    fn as_argument_rejected() {
        let source = r#"fn make() -> [i32; 3] { return [1, 2, 3]; } fn sum(a: [i32; 3]) -> i32 { return a[0]; } fn test() -> i32 { return sum(make()); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "array-returning call as argument should be rejected"
        );
        if let Err(error) = result {
            let msg = error.to_string();
            assert!(
                msg.contains("let") && msg.contains("return"),
                "Error should mention let/return: {msg}"
            );
        }
    }

    #[test]
    fn index_access_rejected() {
        let source = r#"fn make() -> [i32; 3] { return [1, 2, 3]; } fn test() -> i32 { return make()[0]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "indexing sret call result should be rejected"
        );
        if let Err(error) = result {
            let msg = error.to_string();
            assert!(
                msg.contains("let") && msg.contains("return"),
                "Error should mention let/return: {msg}"
            );
        }
    }

    #[test]
    fn assignment_rejected() {
        let source = r#"fn make() -> [i32; 3] { return [1, 2, 3]; } fn test() -> i32 { let mut a: [i32; 3] = [0, 0, 0]; a = make(); return a[0]; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "array-returning call in assignment should be rejected"
        );
        if let Err(error) = result {
            let msg = error.to_string();
            assert!(
                msg.contains("let") && msg.contains("return"),
                "Error should mention let/return: {msg}"
            );
        }
    }

    #[test]
    fn non_array_return_standalone_accepted() {
        let source =
            r#"fn make() -> i32 { return 42; } fn test() -> i32 { make(); return 0; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "standalone call to non-array-returning function should be accepted, got: {:?}",
            result.err()
        );
    }
}
