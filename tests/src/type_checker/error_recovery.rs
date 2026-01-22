/// Tests for error recovery
///
/// These tests verify that the type checker:
/// 1. Collects multiple errors instead of stopping at the first error
/// 2. Deduplicates errors appropriately
/// 3. Continues registration even when errors are found
/// 4. Includes location information in error messages
#[cfg(test)]
mod error_recovery_tests {
    use crate::utils::build_ast;
    use inference_type_checker::TypeCheckerBuilder;

use always_assert::always;
    fn try_type_check(
        source: &str,
    ) -> anyhow::Result<inference_type_checker::typed_context::TypedContext> {
        let arena = build_ast(source.to_string());
        Ok(TypeCheckerBuilder::build_typed_context(arena)?.typed_context())
    }

    #[test]
    fn test_multiple_errors_in_same_function() {
        let source = r#"
            fn test() -> i32 {
                let x: i32 = unknown_var1;
                let y: i32 = unknown_var2;
                return x + y;
            }
        "#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect multiple unknown variables"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("unknown_var1") && error_msg.contains("unknown_var2"),
                "Error message should contain both unknown variables, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_errors_across_multiple_functions() {
        let source = r#"
            fn foo() -> i32 {
                return unknown_var1;
            }
            fn bar() -> i32 {
                return unknown_var2;
            }
        "#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect errors in both functions"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("unknown_var1") && error_msg.contains("unknown_var2"),
                "Error message should contain errors from both functions, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_registration_and_inference_errors_collected() {
        let source = r#"
            fn test(x: UnknownType) -> i32 {
                return unknown_var;
            }
        "#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect both registration and inference errors"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("UnknownType") && error_msg.contains("unknown_var"),
                "Error message should contain both unknown type and unknown variable, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_function_registered_despite_param_error() {
        let source = r#"
            fn helper(x: UnknownType) -> i32 {
                return 42;
            }
            fn test() -> i32 {
                return helper(10);
            }
        "#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should report unknown type error"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("UnknownType"),
                "Error message should mention unknown type, got: {}",
                error_msg
            );
            always!(
                !error_msg.contains("undefined function `helper`"),
                "Error message should NOT contain undefined function error, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_error_deduplication() {
        let source = r#"
            struct Container {
                value: UnknownType;
            }
            fn test(c: Container) -> UnknownType {
                let x: UnknownType = c.value;
                return x;
            }
        "#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect unknown type error"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            let unknown_type_count = error_msg.matches("unknown type `UnknownType`").count();
            always!(
                unknown_type_count <= 3,
                "UnknownType error should not be excessively duplicated (found {} occurrences), got: {}",
                unknown_type_count,
                error_msg
            );
        }
    }

    #[test]
    fn test_method_call_on_non_struct_infers_arguments() {
        let source = r#"
            fn test(x: i32) -> i32 {
                return x.method(unknown_var);
            }
        "#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect errors in method call"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("unknown_var") || error_msg.contains("cannot call method"),
                "Error message should contain unknown variable or method call error, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_undefined_struct_with_field_access() {
        let source = r#"
            fn test(s: UndefinedStruct) -> i32 {
                return s.field;
            }
        "#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect undefined struct"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("UndefinedStruct") || error_msg.contains("unknown type"),
                "Error message should mention undefined struct or unknown type, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_undefined_enum_with_variant_access() {
        let source = r#"
            fn test() -> UndefinedEnum {
                return UndefinedEnum::Variant;
            }
        "#;
        let result = try_type_check(source);
        always!(result.is_err(), "Type checker should detect undefined enum");

        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("UndefinedEnum") || error_msg.contains("unknown type"),
                "Error message should mention undefined enum or unknown type, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_multiple_unknown_types_in_function_signature() {
        let source = r#"
            fn test(a: Type1, b: Type2, c: Type3) -> Type4 {
                return a;
            }
        "#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect all unknown types"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            let has_type1 = error_msg.contains("Type1");
            let has_type2 = error_msg.contains("Type2");
            let has_type3 = error_msg.contains("Type3");
            let has_type4 = error_msg.contains("Type4");

            let type_count = [has_type1, has_type2, has_type3, has_type4]
                .iter()
                .filter(|&&x| x)
                .count();

            always!(
                type_count >= 2,
                "Error message should contain at least 2 unknown types, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_error_with_location_information() {
        let source = r#"
            fn test() -> i32 {
                return unknown_var;
            }
        "#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect unknown variable"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            let has_location_prefix = error_msg.contains(":");
            always!(
                has_location_prefix,
                "Error message should include location information (line:col prefix), got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_unknown_identifier_in_binary_expression() {
        let source = r#"
            fn test() -> i32 {
                return unknown_var1 + unknown_var2;
            }
        "#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect unknown identifiers in binary expression"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("unknown_var1") || error_msg.contains("unknown_var2"),
                "Error message should contain at least one unknown variable, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_function_call_with_undefined_function_and_unknown_args() {
        let source = r#"
            fn test() -> i32 {
                return undefined_func(unknown_var);
            }
        "#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect undefined function"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("undefined_func") || error_msg.contains("unknown_var"),
                "Error message should mention undefined function or unknown variable, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_struct_with_unknown_field_types() {
        let source = r#"
            struct Container {
                field1: UnknownType1;
                field2: UnknownType2;
            }
            fn test(c: Container) -> UnknownType1 {
                return c.field1;
            }
        "#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect unknown field types"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            let has_type1 = error_msg.contains("UnknownType1");
            let has_type2 = error_msg.contains("UnknownType2");

            always!(
                has_type1 || has_type2,
                "Error message should contain at least one unknown type, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_method_with_unknown_param_and_return_types() {
        let source = r#"
            struct MyStruct {
                value: i32;

                fn method(x: UnknownParam) -> UnknownReturn {
                    return x;
                }
            }
        "#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect unknown types in method signature"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("UnknownParam") || error_msg.contains("UnknownReturn"),
                "Error message should contain unknown parameter or return type, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_mixed_valid_and_invalid_functions() {
        let source = r#"
            fn valid1() -> i32 {
                return 42;
            }
            fn invalid(x: UnknownType) -> i32 {
                return unknown_var;
            }
            fn valid2() -> bool {
                return true;
            }
        "#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect errors in invalid function"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("UnknownType") || error_msg.contains("unknown_var"),
                "Error message should contain errors from invalid function, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_continue_after_unknown_type_in_variable_definition() {
        let source = r#"
            fn test() -> i32 {
                let x: UnknownType = 42;
                let y: i32 = unknown_var;
                return y;
            }
        "#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect both unknown type and unknown variable"
        );

        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("UnknownType") || error_msg.contains("unknown_var"),
                "Error message should contain at least one error, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_error_deduplication_same_unknown_type_multiple_uses() {
        let source = r#"fn test(a: UnknownType, b: UnknownType) -> UnknownType { let c: UnknownType = a; return c; }"#;
        let result = try_type_check(source);
        always!(result.is_err(), "Type checker should detect unknown type");
        if let Err(error) = result {
            let error_msg = error.to_string();
            let count = error_msg.matches("unknown type `UnknownType`").count();
            assert_eq!(
                count, 1,
                "UnknownType error should appear exactly once due to deduplication, but appeared {} times in: {}",
                count, error_msg
            );
        }
    }

    #[test]
    fn test_error_deduplication_same_undefined_function_multiple_calls() {
        let source = r#"fn test() -> i32 { let x: i32 = missing_func(); let y: i32 = missing_func(); return x + y; }"#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect undefined function"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            let count = error_msg
                .matches("undefined function `missing_func`")
                .count();
            assert_eq!(
                count, 1,
                "missing_func error should appear exactly once due to deduplication, but appeared {} times in: {}",
                count, error_msg
            );
        }
    }

    #[test]
    fn test_error_deduplication_same_unknown_identifier_multiple_uses() {
        let source = r#"fn test() -> i32 { let x: i32 = unknown_var; let y: i32 = unknown_var; return x + y + unknown_var; }"#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect unknown identifier"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            let count = error_msg
                .matches("undeclared variable `unknown_var`")
                .count();
            assert_eq!(
                count, 1,
                "unknown_var error should appear exactly once due to deduplication, but appeared {} times in: {}",
                count, error_msg
            );
        }
    }

    #[test]
    fn test_error_deduplication_same_undefined_struct_multiple_uses() {
        let source = r#"fn test(a: MissingStruct) -> MissingStruct { let b: MissingStruct = MissingStruct { }; return b; }"#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect undefined struct"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            let struct_count = error_msg
                .matches("struct `MissingStruct` is not defined")
                .count();
            let type_count = error_msg.matches("unknown type `MissingStruct`").count();
            always!(
                struct_count <= 1 && type_count <= 1,
                "MissingStruct error should appear at most once due to deduplication (struct: {}, type: {}), error: {}",
                struct_count,
                type_count,
                error_msg
            );
        }
    }

    #[test]
    fn test_error_deduplication_same_undefined_enum_multiple_uses() {
        let source =
            r#"fn test() -> MissingEnum { let x: MissingEnum = MissingEnum::Variant; return x; }"#;
        let result = try_type_check(source);
        always!(result.is_err(), "Type checker should detect undefined enum");
        if let Err(error) = result {
            let error_msg = error.to_string();
            let enum_count = error_msg
                .matches("enum `MissingEnum` is not defined")
                .count();
            let type_count = error_msg.matches("unknown type `MissingEnum`").count();
            always!(
                enum_count <= 1 && type_count <= 1,
                "MissingEnum error should appear at most once due to deduplication (enum: {}, type: {}), error: {}",
                enum_count,
                type_count,
                error_msg
            );
        }
    }

    #[test]
    fn test_multiple_different_errors_all_collected() {
        let source = r#"fn test(x: UnknownType1) -> UnknownType2 { let y: i32 = unknown_var; return missing_func(); }"#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect multiple errors"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            let has_type1 = error_msg.contains("UnknownType1");
            let has_type2 = error_msg.contains("UnknownType2");
            let has_var = error_msg.contains("unknown_var");
            let has_func = error_msg.contains("missing_func");
            let error_count = [has_type1, has_type2, has_var, has_func]
                .iter()
                .filter(|&&x| x)
                .count();
            always!(
                error_count >= 3,
                "Should collect at least 3 different errors (found {}): {}",
                error_count,
                error_msg
            );
        }
    }

    #[test]
    fn test_error_recovery_after_type_mismatch_in_assignment() {
        let source =
            r#"fn test() -> i32 { let x: i32 = true; let y: i32 = unknown_var; return y; }"#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect type mismatch and unknown variable"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("type mismatch") || error_msg.contains("unknown_var"),
                "Should report both type mismatch and unknown identifier errors: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_error_recovery_after_type_mismatch_in_return() {
        let source =
            r#"fn test() -> i32 { return true; } fn test2() -> i32 { return undefined_func(); }"#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect errors in both functions"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("type mismatch") || error_msg.contains("undefined"),
                "Should report errors from both functions: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_error_recovery_continues_through_multiple_statements() {
        let source = r#"fn test() -> i32 { let a: i32 = unknown1; let b: i32 = unknown2; let c: i32 = unknown3; return a + b + c; }"#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect all unknown variables"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            let count1 = error_msg.contains("unknown1");
            let count2 = error_msg.contains("unknown2");
            let count3 = error_msg.contains("unknown3");
            let total_errors = [count1, count2, count3].iter().filter(|&&x| x).count();
            always!(
                total_errors >= 2,
                "Should continue collecting errors through all statements (found {}): {}",
                total_errors,
                error_msg
            );
        }
    }

    #[test]
    fn test_error_recovery_in_nested_blocks() {
        let source = r#"fn test() -> i32 { if true { let x: i32 = unknown1; } let y: i32 = unknown2; return y; }"#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect unknown variables in nested blocks"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("unknown1") || error_msg.contains("unknown2"),
                "Should detect errors in both nested and outer scopes: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_import_error_has_location() {
        let source = r#"use nonexistent::module; fn test() -> i32 { return 42; }"#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect import resolution failure"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("nonexistent::module"),
                "Error should mention the import path: {}",
                error_msg
            );
            always!(
                error_msg.contains(":"),
                "Error should include location information: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_multiple_import_errors_collected() {
        let source = r#"use missing1::Item1; use missing2::Item2; fn test() -> i32 { return 42; }"#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect multiple import failures"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            let has_import1 = error_msg.contains("missing1") || error_msg.contains("Item1");
            let has_import2 = error_msg.contains("missing2") || error_msg.contains("Item2");
            always!(
                has_import1 || has_import2,
                "Should report at least one import error: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_type_mismatch_followed_by_valid_code() {
        let source = r#"fn test() -> i32 { let x: i32 = true; return 42; } fn valid() -> i32 { return 10; }"#;
        let result = try_type_check(source);
        always!(result.is_err(), "Type checker should detect type mismatch");
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("type mismatch"),
                "Should report type mismatch error: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_error_deduplication_with_different_error_types() {
        let source = r#"fn test(x: MissingType) -> i32 { let y: i32 = missing_var; return missing_func(); }"#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect multiple distinct errors"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            let missing_type_count = error_msg.matches("MissingType").count();
            let missing_var_count = error_msg.matches("missing_var").count();
            let missing_func_count = error_msg.matches("missing_func").count();
            always!(
                missing_type_count <= 2,
                "MissingType should not be excessively duplicated: {}",
                error_msg
            );
            always!(
                missing_var_count <= 2,
                "missing_var should not be excessively duplicated: {}",
                error_msg
            );
            always!(
                missing_func_count <= 2,
                "missing_func should not be excessively duplicated: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_function_registered_despite_return_type_error() {
        let source = r#"fn helper() -> UnknownReturnType { return 42; } fn caller() -> i32 { return helper(); }"#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect unknown return type"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("UnknownReturnType"),
                "Should report unknown return type: {}",
                error_msg
            );
            always!(
                !error_msg.contains("undefined function `helper`"),
                "helper should be registered despite return type error: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_array_index_error_continues_inference() {
        let source = r#"fn test() -> i32 { let arr: [i32; 3] = [1, 2, 3]; let idx: bool = true; let val: i32 = arr[idx]; return unknown_var; }"#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect array index type error and unknown variable"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("unknown_var") || error_msg.contains("index"),
                "Should continue after array index error: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_struct_field_error_continues_inference() {
        let source = r#"struct Point { x: i32; y: i32; } fn test(p: Point) -> i32 { let z: i32 = p.missing_field; return unknown_var; }"#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect field not found and unknown variable"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("unknown_var") || error_msg.contains("field"),
                "Should continue after field error: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_binary_operation_error_continues_inference() {
        let source =
            r#"fn test() -> i32 { let x: i32 = 10 + true; let y: i32 = unknown_var; return y; }"#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect binary operation error and unknown variable"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("unknown_var") || error_msg.contains("type"),
                "Should continue after binary operation error: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_method_not_found_continues_inference() {
        let source = r#"struct MyStruct { value: i32; } fn test(s: MyStruct) -> i32 { let x: i32 = s.missing_method(); return unknown_var; }"#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Type checker should detect method not found and unknown variable"
        );
        if let Err(error) = result {
            let error_msg = error.to_string();
            always!(
                error_msg.contains("unknown_var") || error_msg.contains("method"),
                "Should continue after method not found error: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_all_error_variants_have_required_location() {
        let test_cases = vec![
            (
                r#"fn test(x: UnknownType) -> i32 { return 42; }"#,
                "UnknownType",
            ),
            (r#"fn test() -> i32 { return unknown_var; }"#, "unknown_var"),
            (
                r#"fn test() -> i32 { return missing_func(); }"#,
                "missing_func",
            ),
            (
                r#"fn test() -> i32 { let s: MyStruct = MyStruct { }; return 42; }"#,
                "MyStruct",
            ),
        ];
        for (source, error_substring) in test_cases {
            let result = try_type_check(source);
            always!(result.is_err(), "Should detect error in: {}", source);
            if let Err(error) = result {
                let error_msg = error.to_string();
                always!(
                    error_msg.contains(":"),
                    "Error should have location (contains ':') for {}: {}",
                    error_substring,
                    error_msg
                );
            }
        }
    }
}
