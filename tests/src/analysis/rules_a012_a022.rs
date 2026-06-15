/// Integration tests for analysis rules A012-A022.
///
/// These rules were migrated from the type checker to the analysis phase:
/// - A012: CompoundLiteralAsArgument (array and struct)
/// - A014: ArrayUzumakiAsArgument
/// - A015: CompoundLiteralInUnsupportedPosition
/// - A016: CompoundReturnCallInExpressionPosition
/// - A017: CompoundReturnCallInAssignment
/// - A018: MethodCallChainOnCompoundReturn
/// - A019: ArrayIndex64Bit
/// - A022: LiteralOutOfRange
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::build_ast;
    use inference_analysis::errors::{AnalysisDiagnostic, AnalysisErrors, AnalysisResult};
    use inference_type_checker::typed_context::TypedContext;

    fn type_check(source: &str) -> TypedContext {
        let arena = build_ast(source.to_string());
        inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .expect("type checking should succeed for analysis test input")
            .typed_context()
    }

    fn analyze(source: &str) -> Result<AnalysisResult, AnalysisErrors> {
        let ctx = type_check(source);
        inference_analysis::analyze(&ctx)
    }

    fn expect_errors(source: &str) -> Vec<AnalysisDiagnostic> {
        analyze(source)
            .expect_err("expected analysis errors but got Ok")
            .errors()
            .to_vec()
    }

    // A012: CompoundLiteralAsArgument ---

    #[test]
    fn a012_array_literal_as_argument_rejected() {
        let source = r#"
            fn sum(arr: [i32; 3]) -> i32 { return arr[0]; }
            fn test() -> i32 { return sum([1, 2, 3]); }
        "#;
        let errors = expect_errors(source);
        let has_a012 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralAsArgument { kind: "Array", .. }));
        assert!(
            has_a012,
            "expected CompoundLiteralAsArgument(Array), got: {errors:?}"
        );
    }

    #[test]
    fn a012_array_variable_as_argument_accepted() {
        let source = r#"
            fn sum(arr: [i32; 3]) -> i32 { return arr[0]; }
            fn test() -> i32 {
                let a: [i32; 3] = [1, 2, 3];
                return sum(a);
            }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_a012 = errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralAsArgument { kind: "Array", .. }));
            assert!(
                !has_a012,
                "array variable as argument should NOT trigger A012, got: {errors}"
            );
        }
    }

    #[test]
    fn a013_struct_literal_as_argument_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn takes_point(p: Point) -> i32 { return p.x; }
            fn test() -> i32 { return takes_point(Point { x: 1, y: 2 }); }
        "#;
        let errors = expect_errors(source);
        let has_a013 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralAsArgument { kind: "Struct", .. }));
        assert!(
            has_a013,
            "expected CompoundLiteralAsArgument(Struct), got: {errors:?}"
        );
    }

    #[test]
    fn a013_struct_variable_as_argument_accepted() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn takes_point(p: Point) -> i32 { return p.x; }
            fn test() -> i32 {
                let p: Point = Point { x: 1, y: 2 };
                return takes_point(p);
            }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_a013 = errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralAsArgument { kind: "Struct", .. }));
            assert!(
                !has_a013,
                "struct variable as argument should NOT trigger A012, got: {errors}"
            );
        }
    }

    // A014: ArrayUzumakiAsArgument ---

    #[test]
    fn a014_array_uzumaki_as_argument_rejected() {
        let source = r#"
            fn process(arr: [i32; 5]) -> i32 { return arr[0]; }
            pub fn spec() -> i32 { return process(@); }
        "#;
        let errors = expect_errors(source);
        let has_a014 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ArrayUzumakiAsArgument { .. }));
        assert!(
            has_a014,
            "expected ArrayUzumakiAsArgument, got: {errors:?}"
        );
    }

    #[test]
    fn a014_scalar_uzumaki_as_argument_accepted() {
        let source = r#"
            fn identity(x: i32) -> i32 { return x; }
            pub fn spec() -> i32 { return identity(@); }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_a014 = errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::ArrayUzumakiAsArgument { .. }));
            assert!(
                !has_a014,
                "scalar uzumaki as argument should NOT trigger A014, got: {errors}"
            );
        }
    }

    // A015: CompoundLiteralInUnsupportedPosition ---

    #[test]
    fn a015_struct_literal_as_standalone_expression_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn test() { Point { x: 1, y: 2 }; }
        "#;
        let errors = expect_errors(source);
        let has_a015 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition { .. }));
        assert!(
            has_a015,
            "expected CompoundLiteralInUnsupportedPosition, got: {errors:?}"
        );
    }

    #[test]
    fn a015_struct_literal_as_method_receiver_rejected() {
        // A struct literal used directly as a method receiver is an unsupported
        // compound-literal position, so it defers to A015 rather than producing a
        // spurious "method not found" — the method `sum` does exist. The supported
        // form binds the literal to a local first (see
        // `reexport_qualified_struct_literal_method_call_executes` in the codegen
        // multi-file suite).
        let source = r#"
            struct Point {
                x: i32;
                y: i32;
                pub fn sum(self) -> i32 { return self.x + self.y; }
            }
            fn test() -> i32 { return Point { x: 30, y: 12 }.sum(); }
        "#;
        let errors = expect_errors(source);
        let has_a015 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition { .. }));
        assert!(
            has_a015,
            "a struct-literal method receiver must defer to A015, got: {errors:?}"
        );
    }

    #[test]
    fn a015_struct_literal_in_let_binding_accepted() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn test() { let p: Point = Point { x: 1, y: 2 }; }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_a015 = errors
                .errors()
                .iter()
                .any(|e| {
                    matches!(e, AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition { .. })
                });
            assert!(
                !has_a015,
                "struct literal in let binding should NOT trigger A015, got: {errors}"
            );
        }
    }

    #[test]
    fn a015_array_literal_in_const_initializer_accepted() {
        let source = r#"
            fn test() -> i32 {
                const ARR: [i32; 3] = [1, 2, 3];
                return ARR[0];
            }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_a015 = errors
                .errors()
                .iter()
                .any(|e| {
                    matches!(e, AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition { .. })
                });
            assert!(
                !has_a015,
                "array literal in const initializer should NOT trigger A015, got: {errors}"
            );
        }
    }

    #[test]
    fn a015_compound_literal_in_forbidden_subposition_of_const_initializer_rejected() {
        // The outer array literal is the const's initializer (allowed position),
        // but the inner `[1, 2, 3]` literals sit as operands of a binary `==`,
        // which the rule treats as a forbidden sub-position. Confirms the
        // ConstDef arm does not accidentally mark *every* nested compound
        // literal as allowed. Mirrors `a015_compound_literal_in_if_condition_rejected`.
        let source = r#"
            fn test() -> bool {
                const R: [bool; 1] = [[1, 2, 3] == [1, 2, 3]];
                return R[0];
            }
        "#;
        let errors = expect_errors(source);
        let has_a015 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition { .. }));
        assert!(
            has_a015,
            "expected CompoundLiteralInUnsupportedPosition for array literal in \
             binary `==` sub-position of a const initializer, got: {errors:?}"
        );
    }

    #[test]
    fn a015_struct_literal_in_const_initializer_accepted() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn test() -> i32 {
                const P: Point = Point { x: 1, y: 2 };
                return P.x;
            }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_a015 = errors
                .errors()
                .iter()
                .any(|e| {
                    matches!(e, AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition { .. })
                });
            assert!(
                !has_a015,
                "struct literal in const initializer should NOT trigger A015, got: {errors}"
            );
        }
    }

    // A016: CompoundReturnCallInExpressionPosition ---

    #[test]
    fn a016_array_returning_call_as_standalone_rejected() {
        let source = r#"
            fn make() -> [i32; 3] { return [1, 2, 3]; }
            fn test() -> i32 { make(); return 0; }
        "#;
        let errors = expect_errors(source);
        let has_a016 = errors
            .iter()
            .any(|e| {
                matches!(e, AnalysisDiagnostic::CompoundReturnCallInExpressionPosition { .. })
            });
        assert!(
            has_a016,
            "expected CompoundReturnCallInExpressionPosition, got: {errors:?}"
        );
    }

    #[test]
    fn a016_array_returning_call_as_argument_rejected() {
        let source = r#"
            fn make() -> [i32; 3] { return [1, 2, 3]; }
            fn sum(a: [i32; 3]) -> i32 { return a[0]; }
            fn test() -> i32 { return sum(make()); }
        "#;
        let errors = expect_errors(source);
        let has_a016 = errors
            .iter()
            .any(|e| {
                matches!(e, AnalysisDiagnostic::CompoundReturnCallInExpressionPosition { .. })
            });
        assert!(
            has_a016,
            "expected CompoundReturnCallInExpressionPosition for nested call, got: {errors:?}"
        );
    }

    #[test]
    fn a016_struct_returning_call_as_argument_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn make() -> Point { return Point { x: 1, y: 2 }; }
            fn takes_point(p: Point) -> i32 { return p.x; }
            fn test() -> i32 { return takes_point(make()); }
        "#;
        let errors = expect_errors(source);
        let has_a016 = errors
            .iter()
            .any(|e| {
                matches!(e, AnalysisDiagnostic::CompoundReturnCallInExpressionPosition { .. })
            });
        assert!(
            has_a016,
            "expected CompoundReturnCallInExpressionPosition for struct, got: {errors:?}"
        );
    }

    #[test]
    fn a016_struct_returning_call_as_standalone_rejected() {
        let source = r#"
            struct Point {
                x: i32; y: i32;
                fn new(x: i32, y: i32) -> Point {
                    let p: Point = Point { x: x, y: y };
                    return p;
                }
            }
            fn test() { Point::new(1, 2); }
        "#;
        let errors = expect_errors(source);
        let has_a016 = errors
            .iter()
            .any(|e| {
                matches!(e, AnalysisDiagnostic::CompoundReturnCallInExpressionPosition { .. })
            });
        assert!(
            has_a016,
            "expected CompoundReturnCallInExpressionPosition for standalone, got: {errors:?}"
        );
    }

    #[test]
    fn a016_compound_returning_call_in_let_binding_accepted() {
        let source = r#"
            fn make() -> [i32; 3] { return [1, 2, 3]; }
            fn test() -> i32 { let a: [i32; 3] = make(); return a[0]; }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_a016 = errors
                .errors()
                .iter()
                .any(|e| {
                    matches!(e, AnalysisDiagnostic::CompoundReturnCallInExpressionPosition { .. })
                });
            assert!(
                !has_a016,
                "compound call in let binding should NOT trigger A016, got: {errors}"
            );
        }
    }

    #[test]
    fn a016_compound_returning_call_in_return_accepted() {
        let source = r#"
            fn inner() -> [i32; 3] { return [1, 2, 3]; }
            fn outer() -> [i32; 3] { return inner(); }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_a016 = errors
                .errors()
                .iter()
                .any(|e| {
                    matches!(e, AnalysisDiagnostic::CompoundReturnCallInExpressionPosition { .. })
                });
            assert!(
                !has_a016,
                "compound call in return should NOT trigger A016, got: {errors}"
            );
        }
    }

    #[test]
    fn a016_non_compound_returning_call_standalone_accepted() {
        let source = r#"
            fn make() -> i32 { return 42; }
            fn test() -> i32 { make(); return 0; }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_a016 = errors
                .errors()
                .iter()
                .any(|e| {
                    matches!(e, AnalysisDiagnostic::CompoundReturnCallInExpressionPosition { .. })
                });
            assert!(
                !has_a016,
                "non-compound returning standalone call should NOT trigger A016, got: {errors}"
            );
        }
    }

    // A017: CompoundReturnCallInAssignment ---

    #[test]
    fn a017_compound_returning_call_in_assignment_rejected() {
        let source = r#"
            fn make() -> [i32; 3] { return [1, 2, 3]; }
            fn test() -> i32 {
                let mut a: [i32; 3] = [0, 0, 0];
                a = make();
                return a[0];
            }
        "#;
        let errors = expect_errors(source);
        let has_a017 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundReturnCallInAssignment { .. }));
        assert!(
            has_a017,
            "expected CompoundReturnCallInAssignment, got: {errors:?}"
        );
    }

    #[test]
    fn a017_struct_returning_call_in_assignment_rejected() {
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
        let errors = expect_errors(source);
        let has_a017 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundReturnCallInAssignment { .. }));
        assert!(
            has_a017,
            "expected CompoundReturnCallInAssignment for struct, got: {errors:?}"
        );
    }

    #[test]
    fn a017_instance_method_returning_struct_in_assignment_rejected() {
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
        let errors = expect_errors(source);
        let has_a017 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundReturnCallInAssignment { .. }));
        assert!(
            has_a017,
            "expected CompoundReturnCallInAssignment for method, got: {errors:?}"
        );
    }

    #[test]
    fn a017_associated_function_returning_struct_in_assignment_rejected() {
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
        let errors = expect_errors(source);
        let has_a017 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundReturnCallInAssignment { .. }));
        assert!(
            has_a017,
            "expected CompoundReturnCallInAssignment for assoc fn, got: {errors:?}"
        );
    }

    // A018: MethodCallChainOnCompoundReturn ---

    #[test]
    fn a018_method_chain_on_compound_return_rejected() {
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
        let errors = expect_errors(source);
        let has_a018 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::MethodCallChainOnCompoundReturn { .. }));
        assert!(
            has_a018,
            "expected MethodCallChainOnCompoundReturn, got: {errors:?}"
        );
    }

    #[test]
    fn a018_method_chain_on_associated_function_return_rejected() {
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
        let errors = expect_errors(source);
        let has_a018 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::MethodCallChainOnCompoundReturn { .. }));
        assert!(
            has_a018,
            "expected MethodCallChainOnCompoundReturn for assoc fn, got: {errors:?}"
        );
    }

    // A019: ArrayIndex64Bit ---

    #[test]
    fn a019_i64_array_index_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                let idx: i64 = 0;
                return arr[idx];
            }
        "#;
        let errors = expect_errors(source);
        let has_a019 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ArrayIndex64Bit { .. }));
        assert!(
            has_a019,
            "expected ArrayIndex64Bit for i64, got: {errors:?}"
        );
    }

    #[test]
    fn a019_u64_array_index_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                let idx: u64 = 0;
                return arr[idx];
            }
        "#;
        let errors = expect_errors(source);
        let has_a019 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ArrayIndex64Bit { .. }));
        assert!(
            has_a019,
            "expected ArrayIndex64Bit for u64, got: {errors:?}"
        );
    }

    #[test]
    fn a019_i32_array_index_accepted() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                let idx: i32 = 0;
                return arr[idx];
            }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_a019 = errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::ArrayIndex64Bit { .. }));
            assert!(
                !has_a019,
                "i32 index should NOT trigger A019, got: {errors}"
            );
        }
    }

    // A022: LiteralOutOfRange ---

    #[test]
    fn a022_i8_out_of_range() {
        let source = r#"fn test() -> i32 { let x: i8 = 200; return 0; }"#;
        let errors = expect_errors(source);
        let has_a022 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. }));
        assert!(has_a022, "expected LiteralOutOfRange for i8, got: {errors:?}");
    }

    #[test]
    fn a022_u8_out_of_range() {
        let source = r#"fn test() -> i32 { let x: u8 = 256; return 0; }"#;
        let errors = expect_errors(source);
        let has_a022 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. }));
        assert!(has_a022, "expected LiteralOutOfRange for u8, got: {errors:?}");
    }

    #[test]
    fn a022_i16_out_of_range() {
        let source = r#"fn test() -> i32 { let x: i16 = 40000; return 0; }"#;
        let errors = expect_errors(source);
        let has_a022 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. }));
        assert!(
            has_a022,
            "expected LiteralOutOfRange for i16, got: {errors:?}"
        );
    }

    #[test]
    fn a022_u16_out_of_range() {
        let source = r#"fn test() -> i32 { let x: u16 = 70000; return 0; }"#;
        let errors = expect_errors(source);
        let has_a022 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. }));
        assert!(
            has_a022,
            "expected LiteralOutOfRange for u16, got: {errors:?}"
        );
    }

    #[test]
    fn a022_i32_overflow() {
        let source = r#"fn test() -> i32 { let x: i32 = 2147483648; return 0; }"#;
        let errors = expect_errors(source);
        let has_a022 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. }));
        assert!(
            has_a022,
            "expected LiteralOutOfRange for i32 overflow, got: {errors:?}"
        );
    }

    #[test]
    fn a022_array_element_out_of_range() {
        let source = r#"fn test() -> i32 { let arr: [u8; 3] = [255, 256, 0]; return 0; }"#;
        let errors = expect_errors(source);
        let has_a022 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. }));
        assert!(
            has_a022,
            "expected LiteralOutOfRange for array element, got: {errors:?}"
        );
    }

    #[test]
    fn a022_assign_literal_out_of_range() {
        let source = r#"fn test() -> i32 { let mut x: u8 = 0; x = 256; return 0; }"#;
        let errors = expect_errors(source);
        let has_a022 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. }));
        assert!(
            has_a022,
            "expected LiteralOutOfRange for assignment, got: {errors:?}"
        );
    }

    #[test]
    fn a022_constant_out_of_range() {
        let source = r#"fn test() -> i32 { const x: u8 = 300; return 0; }"#;
        let errors = expect_errors(source);
        let has_a022 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. }));
        assert!(
            has_a022,
            "expected LiteralOutOfRange for constant, got: {errors:?}"
        );
    }

    #[test]
    fn a022_i128_overflow_i32() {
        let source =
            r#"fn test() -> i32 { let x: i32 = 99999999999999999999999999999999; return 0; }"#;
        let errors = expect_errors(source);
        let has_a022 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. }));
        assert!(
            has_a022,
            "expected LiteralOutOfRange for i128 overflow, got: {errors:?}"
        );
    }

    #[test]
    fn a022_i128_overflow_u64() {
        let source = r#"fn test() -> i32 { let x: u64 = 999999999999999999999999999999999999999999; return 0; }"#;
        let errors = expect_errors(source);
        let has_a022 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. }));
        assert!(
            has_a022,
            "expected LiteralOutOfRange for u64 i128 overflow, got: {errors:?}"
        );
    }

    #[test]
    fn a022_i128_overflow_i8() {
        let source =
            r#"fn test() -> i32 { let x: i8 = 99999999999999999999999999999999; return 0; }"#;
        let errors = expect_errors(source);
        let has_a022 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. }));
        assert!(
            has_a022,
            "expected LiteralOutOfRange for i8 i128 overflow, got: {errors:?}"
        );
    }

    #[test]
    fn a022_i128_overflow_array_element() {
        let source =
            r#"fn test() -> i32 { let arr: [u8; 2] = [1, 99999999999999999999999999999999]; return 0; }"#;
        let errors = expect_errors(source);
        let has_a022 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. }));
        assert!(
            has_a022,
            "expected LiteralOutOfRange for array i128 overflow, got: {errors:?}"
        );
    }

    #[test]
    fn a022_i128_overflow_constant() {
        let source =
            r#"fn test() -> i32 { const X: i32 = 99999999999999999999999999999999; return 0; }"#;
        let errors = expect_errors(source);
        let has_a022 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. }));
        assert!(
            has_a022,
            "expected LiteralOutOfRange for constant i128 overflow, got: {errors:?}"
        );
    }

    #[test]
    fn a022_i128_overflow_assignment() {
        let source =
            r#"fn test() -> i32 { let mut x: i32 = 0; x = 99999999999999999999999999999999; return 0; }"#;
        let errors = expect_errors(source);
        let has_a022 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. }));
        assert!(
            has_a022,
            "expected LiteralOutOfRange for assignment i128 overflow, got: {errors:?}"
        );
    }

    #[test]
    fn a022_boundary_values_accepted() {
        let source = r#"
            fn test() -> i32 {
                let a: i8 = 127;
                let b: u8 = 255;
                let c: u8 = 0;
                let d: i32 = 2147483647;
                return 0;
            }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_a022 = errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. }));
            assert!(
                !has_a022,
                "boundary values should NOT trigger A022, got: {errors}"
            );
        }
    }

    // Condition expression coverage tests ---
    // These tests verify that analysis rules scan expressions inside
    // loop conditions and if conditions (not just statement-level expressions).

    #[test]
    fn a012_array_literal_in_if_condition_rejected() {
        let source = r#"
            fn check(arr: [i32; 3]) -> bool { return true; }
            fn test() -> i32 {
                if check([1, 2, 3]) { return 1; }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a012 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralAsArgument { kind: "Array", .. }));
        assert!(
            has_a012,
            "expected CompoundLiteralAsArgument(Array) in if condition, got: {errors:?}"
        );
    }

    #[test]
    fn a012_array_literal_in_loop_condition_rejected() {
        let source = r#"
            fn check(arr: [i32; 3]) -> bool { return true; }
            fn test() -> i32 {
                loop check([1, 2, 3]) { return 1; }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a012 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralAsArgument { kind: "Array", .. }));
        assert!(
            has_a012,
            "expected CompoundLiteralAsArgument(Array) in loop condition, got: {errors:?}"
        );
    }

    #[test]
    fn a013_struct_literal_in_if_condition_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn check(p: Point) -> bool { return true; }
            fn test() -> i32 {
                if check(Point { x: 1, y: 2 }) { return 1; }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a013 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralAsArgument { kind: "Struct", .. }));
        assert!(
            has_a013,
            "expected CompoundLiteralAsArgument(Struct) in if condition, got: {errors:?}"
        );
    }

    #[test]
    fn a013_struct_literal_in_loop_condition_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn check(p: Point) -> bool { return true; }
            fn test() -> i32 {
                loop check(Point { x: 1, y: 2 }) { return 1; }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a013 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralAsArgument { kind: "Struct", .. }));
        assert!(
            has_a013,
            "expected CompoundLiteralAsArgument(Struct) in loop condition, got: {errors:?}"
        );
    }

    #[test]
    fn a014_array_uzumaki_in_if_condition_rejected() {
        let source = r#"
            fn check(arr: [i32; 5]) -> bool { return true; }
            pub fn spec() -> i32 {
                if check(@) { return 1; }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a014 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ArrayUzumakiAsArgument { .. }));
        assert!(
            has_a014,
            "expected ArrayUzumakiAsArgument in if condition, got: {errors:?}"
        );
    }

    #[test]
    fn a014_array_uzumaki_in_loop_condition_rejected() {
        let source = r#"
            fn check(arr: [i32; 5]) -> bool { return true; }
            pub fn spec() -> i32 {
                loop check(@) { return 1; }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a014 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ArrayUzumakiAsArgument { .. }));
        assert!(
            has_a014,
            "expected ArrayUzumakiAsArgument in loop condition, got: {errors:?}"
        );
    }

    #[test]
    fn a015_compound_literal_in_if_condition_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                if arr == [1, 2, 3] { return 1; }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a015 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition { .. }));
        assert!(
            has_a015,
            "expected CompoundLiteralInUnsupportedPosition in if condition, got: {errors:?}"
        );
    }

    #[test]
    fn a015_compound_literal_in_loop_condition_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                loop arr == [1, 2, 3] { return 1; }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a015 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition { .. }));
        assert!(
            has_a015,
            "expected CompoundLiteralInUnsupportedPosition in loop condition, got: {errors:?}"
        );
    }

    #[test]
    fn a016_compound_return_call_in_if_condition_rejected() {
        let source = r#"
            fn make() -> [i32; 3] { return [1, 2, 3]; }
            fn check(arr: [i32; 3]) -> bool { return true; }
            fn test() -> i32 {
                if check(make()) { return 1; }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a016 = errors
            .iter()
            .any(|e| {
                matches!(e, AnalysisDiagnostic::CompoundReturnCallInExpressionPosition { .. })
            });
        assert!(
            has_a016,
            "expected CompoundReturnCallInExpressionPosition in if condition, got: {errors:?}"
        );
    }

    #[test]
    fn a016_compound_return_call_in_loop_condition_rejected() {
        let source = r#"
            fn make() -> [i32; 3] { return [1, 2, 3]; }
            fn check(arr: [i32; 3]) -> bool { return true; }
            fn test() -> i32 {
                loop check(make()) { return 1; }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a016 = errors
            .iter()
            .any(|e| {
                matches!(e, AnalysisDiagnostic::CompoundReturnCallInExpressionPosition { .. })
            });
        assert!(
            has_a016,
            "expected CompoundReturnCallInExpressionPosition in loop condition, got: {errors:?}"
        );
    }

    #[test]
    fn a018_method_chain_in_if_condition_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32;
                fn translate(self, dx: i32, dy: i32) -> Point {
                    return Point { x: self.x + dx, y: self.y + dy };
                }
                fn get_x(self) -> i32 { return self.x; }
            }
            fn test() -> i32 {
                let p: Point = Point { x: 10, y: 20 };
                if p.translate(5, 3).get_x() == 15 { return 1; }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a018 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::MethodCallChainOnCompoundReturn { .. }));
        assert!(
            has_a018,
            "expected MethodCallChainOnCompoundReturn in if condition, got: {errors:?}"
        );
    }

    #[test]
    fn a018_method_chain_in_loop_condition_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32;
                fn translate(self, dx: i32, dy: i32) -> Point {
                    return Point { x: self.x + dx, y: self.y + dy };
                }
                fn get_x(self) -> i32 { return self.x; }
            }
            fn test() -> i32 {
                let p: Point = Point { x: 10, y: 20 };
                loop p.translate(5, 3).get_x() == 15 { return 1; }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a018 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::MethodCallChainOnCompoundReturn { .. }));
        assert!(
            has_a018,
            "expected MethodCallChainOnCompoundReturn in loop condition, got: {errors:?}"
        );
    }

    #[test]
    fn a019_64bit_index_in_if_condition_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                let idx: i64 = 0;
                if arr[idx] == 1 { return 1; }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a019 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ArrayIndex64Bit { .. }));
        assert!(
            has_a019,
            "expected ArrayIndex64Bit in if condition, got: {errors:?}"
        );
    }

    #[test]
    fn a019_64bit_index_in_loop_condition_rejected() {
        let source = r#"
            fn test() -> i32 {
                let arr: [i32; 3] = [1, 2, 3];
                let idx: i64 = 0;
                loop arr[idx] == 1 { return 1; }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a019 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ArrayIndex64Bit { .. }));
        assert!(
            has_a019,
            "expected ArrayIndex64Bit in loop condition, got: {errors:?}"
        );
    }

    #[test]
    fn a022_literal_out_of_range_in_if_condition_rejected() {
        let source = r#"
            fn id(x: i32) -> bool { return x == 0; }
            fn test() -> i32 {
                if id(2147483648) { return 1; }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a022 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. }));
        assert!(
            has_a022,
            "expected LiteralOutOfRange in if condition, got: {errors:?}"
        );
    }

    #[test]
    fn a022_literal_out_of_range_in_loop_condition_rejected() {
        let source = r#"
            fn id(x: i32) -> bool { return x == 0; }
            fn test() -> i32 {
                loop id(2147483648) { return 1; }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a022 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. }));
        assert!(
            has_a022,
            "expected LiteralOutOfRange in loop condition, got: {errors:?}"
        );
    }

    // A016: CompoundReturnCallInExpressionPosition in const initializers ---

    #[test]
    fn a016_compound_return_call_indexed_in_const_initializer_rejected() {
        let source = r#"
            fn make() -> [i32; 3] { return [1, 2, 3]; }
            fn test() -> i32 {
                const X: i32 = make()[0];
                return X;
            }
        "#;
        let errors = expect_errors(source);
        let has_a016 = errors
            .iter()
            .any(|e| {
                matches!(e, AnalysisDiagnostic::CompoundReturnCallInExpressionPosition { .. })
            });
        assert!(
            has_a016,
            "expected CompoundReturnCallInExpressionPosition for indexed compound call in const init, got: {errors:?}"
        );
    }

    #[test]
    fn a016_compound_return_call_as_arg_in_const_initializer_rejected() {
        let source = r#"
            fn make() -> [i32; 3] { return [1, 2, 3]; }
            fn consume(a: [i32; 3]) -> i32 { return a[0]; }
            fn test() -> i32 {
                const X: i32 = consume(make());
                return X;
            }
        "#;
        let errors = expect_errors(source);
        let has_a016 = errors
            .iter()
            .any(|e| {
                matches!(e, AnalysisDiagnostic::CompoundReturnCallInExpressionPosition { .. })
            });
        assert!(
            has_a016,
            "expected CompoundReturnCallInExpressionPosition for nested compound call in const init, got: {errors:?}"
        );
    }

    #[test]
    fn a016_compound_return_call_directly_in_const_initializer_accepted() {
        let source = r#"
            fn make() -> [i32; 3] { return [1, 2, 3]; }
            fn test() -> i32 {
                const ARR: [i32; 3] = make();
                return ARR[0];
            }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_a016 = errors
                .errors()
                .iter()
                .any(|e| {
                    matches!(e, AnalysisDiagnostic::CompoundReturnCallInExpressionPosition { .. })
                });
            assert!(
                !has_a016,
                "direct compound-returning call in const initializer should NOT trigger A016 (sret destination), got: {errors}"
            );
        }
    }

    // A018: MethodCallChainOnCompoundReturn in const initializers ---

    #[test]
    fn a018_method_chain_on_compound_return_in_const_initializer_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32;
                fn translate(self, dx: i32, dy: i32) -> Point {
                    return Point { x: self.x + dx, y: self.y + dy };
                }
                fn get_x(self) -> i32 { return self.x; }
            }
            fn test() -> i32 {
                let p: Point = Point { x: 10, y: 20 };
                const X: i32 = p.translate(5, 3).get_x();
                return X;
            }
        "#;
        let errors = expect_errors(source);
        let has_a018 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::MethodCallChainOnCompoundReturn { .. }));
        assert!(
            has_a018,
            "expected MethodCallChainOnCompoundReturn for method chain in const init, got: {errors:?}"
        );
    }

    /// Positive const-analogue mirroring the VarDef `p.translate(5, 3).get_x()` chain,
    /// but with the chain rooted at an associated-function call that returns a struct.
    /// Covers the `Call` -> `MethodCall` chain shape (as distinct from the
    /// `MethodCall` -> `MethodCall` shape above) inside a `ConstDef` initializer.
    #[test]
    fn a018_method_chain_on_assoc_fn_compound_return_in_const_initializer_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32;
                fn new(x: i32, y: i32) -> Point {
                    return Point { x: x, y: y };
                }
                fn get_x(self) -> i32 { return self.x; }
            }
            fn test() -> i32 {
                const R: i32 = Point::new(1, 2).get_x();
                return R;
            }
        "#;
        let errors = expect_errors(source);
        let has_a018 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::MethodCallChainOnCompoundReturn { .. }));
        assert!(
            has_a018,
            "expected MethodCallChainOnCompoundReturn for assoc-fn -> method chain in const init, got: {errors:?}"
        );
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::MethodCallChainOnCompoundReturn { .. }))
            .expect("expected MethodCallChainOnCompoundReturn");
        let msg = diag.to_string();
        assert!(
            msg.to_lowercase().contains("method")
                || msg.to_lowercase().contains("chain")
                || msg.to_lowercase().contains("compound"),
            "diagnostic message should reference the method/chain/compound concept, got: {msg}"
        );
    }

    /// Negative sanity for A018 in const initializers: a const with a single
    /// non-chained call (no method chained on top of a compound-returning call)
    /// must NOT fire A018. Prevents false positives on innocuous const inits
    /// after the rule was extended from VarDef to ConstDef.
    #[test]
    fn a018_single_compound_return_call_in_const_initializer_accepted() {
        let source = r#"
            struct Point { x: i32; y: i32;
                fn new(x: i32, y: i32) -> Point {
                    return Point { x: x, y: y };
                }
            }
            fn test() -> i32 {
                const P: Point = Point::new(1, 2);
                return P.x;
            }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_a018 = errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::MethodCallChainOnCompoundReturn { .. }));
            assert!(
                !has_a018,
                "single unchained compound-returning call in const init should NOT trigger A018, got: {errors}"
            );
        }
    }

    /// Positive const-analogue for A016: a compound-returning call used in a
    /// sub-expression position (operand of `+`) inside a const initializer.
    /// Confirms that extending A016 to `ConstDef` does not stop at the top-level
    /// RHS — it must recurse into operands. Mirrors the `make()[0]` and
    /// `consume(make())` cases above but for a binary-op shape.
    #[test]
    fn a016_compound_return_call_in_binary_op_in_const_initializer_rejected() {
        let source = r#"
            fn make_arr() -> [i32; 3] { return [1, 2, 3]; }
            fn test() -> i32 {
                const X: i32 = make_arr()[0] + make_arr()[1];
                return X;
            }
        "#;
        let errors = expect_errors(source);
        let has_a016 = errors
            .iter()
            .any(|e| {
                matches!(e, AnalysisDiagnostic::CompoundReturnCallInExpressionPosition { .. })
            });
        assert!(
            has_a016,
            "expected CompoundReturnCallInExpressionPosition for compound call in binary-op sub-position of const init, got: {errors:?}"
        );
    }

    // A022: LiteralOutOfRange in const initializer inside function ---

    #[test]
    fn a022_literal_out_of_range_in_const_inside_function() {
        let source = r#"
            fn main() {
                const X: u8 = 256;
            }
        "#;
        let errors = expect_errors(source);
        let has_a022 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::LiteralOutOfRange { .. }));
        assert!(
            has_a022,
            "literal out of range in const inside function should trigger A022, got: {errors:?}"
        );
    }
}
