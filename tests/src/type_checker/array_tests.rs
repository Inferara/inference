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
    fn test_empty_array_annotation() {
        let source = r#"fn test() -> i32 { let arr: [i32; 0] = []; return 42; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Empty array with size 0 should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_large_array_annotation() {
        let source = r#"fn test() -> i32 { let arr: [i32; 1000]; return 42; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Large array (size 1000) annotation should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_very_large_array_annotation() {
        let source = r#"fn test() -> i32 { let arr: [i32; 65535]; return 42; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Very large array (size 65535) annotation should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_nested_array_annotation() {
        let source = r#"fn test() -> i32 { let arr: [[i32; 2]; 3]; return 42; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Nested array [[i32; 2]; 3] annotation should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_deeply_nested_array_annotation() {
        let source = r#"fn test() -> i32 { let arr: [[[i32; 2]; 3]; 4]; return 42; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Deeply nested array [[[i32; 2]; 3]; 4] annotation should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_array_of_bool_annotation() {
        let source = r#"fn test() -> bool { let arr: [bool; 5] = [true, false, true, false, true]; return arr[0]; }"#;
        let result = try_type_check(source);
        always!(
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
        always!(
            result.is_ok(),
            "Arrays of all number types with sizes should work, got: {:?}",
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
        always!(
            result.is_ok(),
            "Function with sized array parameter should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_function_return_sized_array() {
        let source = r#"fn create_array() -> [i32; 3] { return [1, 2, 3]; } fn test() -> i32 { let arr: [i32; 3] = create_array(); return arr[0]; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Function returning sized array should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_function_nested_array_param() {
        let source = r#"fn process(matrix: [[i32; 2]; 3]) -> i32 { return matrix[0][0]; } fn test() -> i32 { let matrix: [[i32; 2]; 3]; return 42; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Function with nested array parameter should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_multiple_array_params_different_sizes() {
        let source = r#"fn process(a: [i32; 2], b: [i32; 3], c: [i32; 5]) -> i32 { return a[0] + b[0] + c[0]; } fn test() -> i32 { return 42; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Function with multiple differently-sized array parameters should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_function_param_and_return_sized_arrays() {
        let source = r#"fn transform(input: [i32; 3]) -> [i32; 3] { return input; } fn test() -> i32 { return 42; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Function with sized array parameter and return should work, got: {:?}",
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
        always!(
            result.is_err(),
            "Array with fewer elements than size annotation should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
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
        always!(
            result.is_err(),
            "Array with more elements than size annotation should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
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
        always!(result.is_err(), "Array with wrong element type should fail");
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
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
        always!(
            result.is_err(),
            "Nested array with size mismatch should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
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
        always!(
            result.is_err(),
            "Nested array with inner array size mismatch should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
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
        always!(
            result.is_err(),
            "Array with all wrong element types should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
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
        // FIXME: Array size mismatches in function arguments are not yet detected by the type checker
        // Once this is implemented, this test should verify the error contains "type mismatch" or "array"
        always!(
            result.is_ok(),
            "Array size mismatch in function args currently not detected: {:?}",
            result.err()
        );
    }
}

mod array_indexing {
    use super::*;

    #[test]
    fn test_array_index_returns_element_type() {
        let source = r#"fn test() -> i32 { let arr: [i32; 5] = [1, 2, 3, 4, 5]; let elem: i32 = arr[0]; return elem; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Array indexing should return correct element type, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_nested_array_index_returns_inner_array_type() {
        let source = r#"fn test() -> i32 { let arr: [[i32; 2]; 3]; let inner: [i32; 2] = arr[0]; return inner[0]; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Nested array indexing should return correct inner array type, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_nested_array_double_index() {
        let source = r#"fn test() -> i32 { let arr: [[i32; 2]; 3] = [[1, 2], [3, 4], [5, 6]]; let elem: i32 = arr[0][0]; return elem; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Double indexing nested array should return element type, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_array_index_with_different_numeric_indices() {
        let source = r#"fn test() -> i32 { let arr: [i32; 10]; let idx: i32 = 0; let elem: i32 = arr[idx]; return elem; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Array indexing with numeric index should work, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_array_index_wrong_type_assignment() {
        let source = r#"fn test() -> i32 { let arr: [i32; 5] = [1, 2, 3, 4, 5]; let elem: bool = arr[0]; return 42; }"#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Assigning array element to wrong type should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
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
        always!(
            result.is_err(),
            "Assigning nested array inner to wrong type should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
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

use always_assert::always;
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
        always!(
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
        always!(
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
                let arr2: [i32; 5];
                arr2 = arr1;
                return arr2[0];
            }
        "#;
        let result = try_type_check(source);
        always!(
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
        always!(
            result.is_err(),
            "Array assignment with size mismatch should fail"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("type mismatch") || error_msg.contains("array"),
                "Error should mention type mismatch or array: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_empty_array_with_bool_type() {
        let source = r#"fn test() -> i32 { let arr: [bool; 0] = []; return 42; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Empty array of bool type should work, got: {:?}",
            result.err()
        );
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
        always!(
            result.is_ok(),
            "Array of size 1 and scalar should be distinct types, got: {:?}",
            result.err()
        );
    }
}
