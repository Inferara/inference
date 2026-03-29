/// Integration tests for analysis rules A029, A030, A031.
///
/// - A029: CompoundLiteralInCompoundAssign -- compound literals cannot be assigned directly to compound elements
/// - A030: UzumakiOnDeepArray -- uzumaki on arrays with more than 2 dimensions is rejected
/// - A031: UnsupportedCompoundReturnExpression -- compound-returning functions must use simple return forms
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

    // --- A029: Compound literal in member access assignment ---

    #[test]
    fn a029_variable_rhs_accepted() {
        let source = r#"
            struct HasArray { arr: [i32; 3]; val: i32; }
            fn main() -> i32 {
                let temp: [i32; 3] = [4, 5, 6];
                let mut s: HasArray = HasArray { arr: [1, 2, 3], val: 0 };
                s.arr = temp;
                return s.arr[0];
            }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a029 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. }));
            assert!(!has_a029, "variable RHS should be accepted, got: {e}");
        }
    }

    /// The type checker currently rejects struct literal assignment to a struct
    /// field due to Struct/Custom type comparison (known limitation). This test
    /// documents that the type checker catches it first. Once the type checker
    /// is fixed, this test should be updated to verify A029 catches it.
    #[test]
    fn a029_struct_literal_rhs_blocked_by_type_checker() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Outer { inner: Inner; val: i32; }
            fn main() -> i32 {
                let mut o: Outer = Outer { inner: Inner { x: 0, y: 0 }, val: 0 };
                o.inner = Inner { x: 1, y: 2 };
                return o.inner.x;
            }
        "#;
        let arena = build_ast(source.to_string());
        let result = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena);
        assert!(
            result.is_err(),
            "type checker currently rejects struct literal assignment to struct field (known limitation)"
        );
    }

    #[test]
    fn a029_array_literal_rhs_rejected() {
        let source = r#"
            struct HasArray { arr: [i32; 3]; val: i32; }
            fn main() -> i32 {
                let mut s: HasArray = HasArray { arr: [1, 2, 3], val: 0 };
                s.arr = [4, 5, 6];
                return s.arr[0];
            }
        "#;
        let errors = expect_errors(source);
        let has_a029 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. }));
        assert!(has_a029, "array literal RHS should be rejected, got: {errors:?}");
    }

    #[test]
    fn a029_rule_id_is_a029() {
        let source = r#"
            struct HasArray { arr: [i32; 3]; val: i32; }
            fn main() -> i32 {
                let mut s: HasArray = HasArray { arr: [1, 2, 3], val: 0 };
                s.arr = [4, 5, 6];
                return s.arr[0];
            }
        "#;
        let errors = expect_errors(source);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. }))
            .expect("expected CompoundLiteralInCompoundAssign");
        assert_eq!(diag.rule_id(), "A029");
    }

    #[test]
    fn a029_scalar_field_assign_accepted() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() -> i32 {
                let mut p: Point = Point { x: 0, y: 0 };
                p.x = 42;
                return p.x;
            }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a029 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. }));
            assert!(!has_a029, "scalar field assign should be accepted, got: {e}");
        }
    }

    #[test]
    fn a029_error_message() {
        let source = r#"
            struct HasArray { arr: [i32; 3]; val: i32; }
            fn main() -> i32 {
                let mut s: HasArray = HasArray { arr: [1, 2, 3], val: 0 };
                s.arr = [4, 5, 6];
                return s.arr[0];
            }
        "#;
        let errors = expect_errors(source);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. }))
            .expect("expected CompoundLiteralInCompoundAssign");
        let msg = diag.to_string();
        assert!(
            msg.contains("compound literal"),
            "error message should mention compound literal, got: {msg}"
        );
        assert!(
            msg.contains("temporary variable"),
            "error message should suggest workaround, got: {msg}"
        );
    }

    // --- A030: Uzumaki on deep array ---

    #[test]
    fn a030_1d_array_uzumaki_accepted() {
        let source = r#"
            fn main() -> i32 {
                forall {
                    let a: [i32; 3] = @;
                }
                return 0;
            }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a030 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnDeepArray { .. }));
            assert!(!has_a030, "1D array uzumaki should be accepted, got: {e}");
        }
    }

    #[test]
    fn a030_2d_array_uzumaki_accepted() {
        let source = r#"
            fn main() -> i32 {
                forall {
                    let g: [[i32; 3]; 2] = @;
                }
                return 0;
            }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a030 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnDeepArray { .. }));
            assert!(!has_a030, "2D array uzumaki should be accepted, got: {e}");
        }
    }

    #[test]
    fn a030_3d_array_uzumaki_rejected() {
        let source = r#"
            fn main() -> i32 {
                forall {
                    let c: [[[i32; 2]; 3]; 4] = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a030 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnDeepArray { .. }));
        assert!(has_a030, "3D array uzumaki should be rejected, got: {errors:?}");
    }

    #[test]
    fn a030_rule_id_is_a030() {
        let source = r#"
            fn main() -> i32 {
                forall {
                    let c: [[[i32; 2]; 3]; 4] = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::UzumakiOnDeepArray { .. }))
            .expect("expected UzumakiOnDeepArray");
        assert_eq!(diag.rule_id(), "A030");
    }

    #[test]
    fn a030_outside_nondet_not_checked() {
        let source = r#"
            fn main() -> i32 {
                let c: [[[i32; 2]; 3]; 4] = @;
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a030 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnDeepArray { .. }));
        assert!(!has_a030, "A030 should not fire outside nondet block (A006 handles it), got: {errors:?}");
        let has_a006 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOutsideNonDetBlock { .. }));
        assert!(has_a006, "A006 should fire for uzumaki outside nondet block, got: {errors:?}");
    }

    #[test]
    fn a030_error_message() {
        let source = r#"
            fn main() -> i32 {
                forall {
                    let c: [[[i32; 2]; 3]; 4] = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::UzumakiOnDeepArray { .. }))
            .expect("expected UzumakiOnDeepArray");
        let msg = diag.to_string();
        assert!(
            msg.contains("2 dimensions"),
            "error message should mention 2 dimensions, got: {msg}"
        );
    }

    #[test]
    fn a030_4d_array_uzumaki_rejected() {
        let source = r#"
            fn main() -> i32 {
                forall {
                    let d: [[[[i32; 2]; 3]; 4]; 5] = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a030 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnDeepArray { .. }));
        assert!(has_a030, "4D array uzumaki should be rejected, got: {errors:?}");
    }

    #[test]
    fn a030_uzumaki_in_exists_block_rejected() {
        let source = r#"
            fn main() -> i32 {
                exists {
                    let c: [[[i32; 2]; 3]; 4] = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a030 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnDeepArray { .. }));
        assert!(has_a030, "3D array uzumaki in exists block should be rejected, got: {errors:?}");
    }

    // --- A029: Compound literal in array index assignment ---

    #[test]
    fn a029_array_index_struct_literal_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() -> i32 {
                let mut pts: [Point; 2] = [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }];
                pts[0] = Point { x: 10, y: 20 };
                return pts[0].x;
            }
        "#;
        let errors = expect_errors(source);
        let has_a029 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. }));
        assert!(has_a029, "struct literal in array index assignment should be rejected, got: {errors:?}");
    }

    #[test]
    fn a029_array_index_array_literal_rejected() {
        let source = r#"
            fn main() -> i32 {
                let mut arr: [[i32; 2]; 2] = [[1, 2], [3, 4]];
                arr[0] = [10, 20];
                return arr[0][0];
            }
        "#;
        let errors = expect_errors(source);
        let has_a029 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. }));
        assert!(has_a029, "array literal in array index assignment should be rejected, got: {errors:?}");
    }

    #[test]
    fn a029_array_index_scalar_accepted() {
        let source = r#"
            fn main() -> i32 {
                let mut arr: [i32; 3] = [1, 2, 3];
                arr[0] = 42;
                return arr[0];
            }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a029 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. }));
            assert!(!has_a029, "scalar array index assignment should pass analysis, got: {e}");
        }
    }

    // --- A031: Unsupported compound return expression ---

    #[test]
    fn a031_identifier_return_accepted() {
        let source = r#"
            fn make_arr() -> [i32; 3] {
                let arr: [i32; 3] = [1, 2, 3];
                return arr;
            }
            fn main() -> i32 { return 0; }
        "#;
        let result = analyze(source);
        assert!(result.is_ok(), "identifier return should be accepted: {result:?}");
    }

    #[test]
    fn a031_array_literal_return_accepted() {
        let source = r#"
            fn make_arr() -> [i32; 3] {
                return [1, 2, 3];
            }
            fn main() -> i32 { return 0; }
        "#;
        let result = analyze(source);
        assert!(result.is_ok(), "literal return should be accepted: {result:?}");
    }

    #[test]
    fn a031_struct_literal_return_accepted() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn make() -> Point {
                return Point { x: 1, y: 2 };
            }
            fn main() -> i32 { return 0; }
        "#;
        let result = analyze(source);
        assert!(result.is_ok(), "struct literal return should be accepted: {result:?}");
    }

    #[test]
    fn a031_function_call_return_accepted() {
        let source = r#"
            fn inner() -> [i32; 3] {
                return [1, 2, 3];
            }
            fn outer() -> [i32; 3] {
                return inner();
            }
            fn main() -> i32 { return 0; }
        "#;
        let result = analyze(source);
        assert!(result.is_ok(), "function call return should be accepted: {result:?}");
    }

    #[test]
    fn a031_member_access_return_accepted() {
        let source = r#"
            struct HasArr { arr: [i32; 3]; val: i32; }
            fn get_arr(s: HasArr) -> [i32; 3] {
                return s.arr;
            }
            fn main() -> i32 { return 0; }
        "#;
        let result = analyze(source);
        assert!(result.is_ok(), "member access return should be accepted: {result:?}");
    }

    #[test]
    fn a031_array_index_return_accepted() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn get_first(pts: [Point; 3]) -> Point {
                return pts[0];
            }
            fn main() -> i32 { return 0; }
        "#;
        let result = analyze(source);
        assert!(result.is_ok(), "array index return should be accepted: {result:?}");
    }

    #[test]
    fn a031_struct_identifier_return_accepted() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn make() -> Point {
                let a: Point = Point { x: 1, y: 2 };
                return a;
            }
            fn main() -> i32 { return 0; }
        "#;
        let result = analyze(source);
        assert!(result.is_ok(), "identifier struct return should be accepted: {result:?}");
    }

    #[test]
    fn a031_scalar_return_not_checked() {
        let source = r#"
            fn add(a: i32, b: i32) -> i32 {
                return a + b;
            }
            fn main() -> i32 { return add(1, 2); }
        "#;
        let result = analyze(source);
        assert!(result.is_ok(), "scalar return should not be checked by A031: {result:?}");
    }

    #[test]
    fn a031_rule_id_is_a031() {
        let diag = AnalysisDiagnostic::UnsupportedCompoundReturnExpression {
            location: inference_ast::nodes::Location {
                offset_start: 0,
                offset_end: 0,
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 1,
            },
        };
        assert_eq!(diag.rule_id(), "A031");
    }

    #[test]
    fn a031_error_message() {
        let diag = AnalysisDiagnostic::UnsupportedCompoundReturnExpression {
            location: inference_ast::nodes::Location {
                offset_start: 0,
                offset_end: 0,
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 1,
            },
        };
        let msg = diag.to_string();
        assert!(msg.contains("compound-returning function"), "message should mention compound-returning: {msg}");
        assert!(msg.contains("temporary variable"), "message should suggest temporary: {msg}");
    }

    #[test]
    fn a031_return_in_if_else_checked() {
        let source = r#"
            fn pick(flag: bool) -> [i32; 3] {
                let a: [i32; 3] = [1, 2, 3];
                let b: [i32; 3] = [4, 5, 6];
                if flag {
                    return a;
                } else {
                    return b;
                }
            }
            fn main() -> i32 { return 0; }
        "#;
        let result = analyze(source);
        assert!(result.is_ok(), "identifier returns in if/else should be accepted: {result:?}");
    }

    #[test]
    fn a031_void_function_not_checked() {
        let source = r#"
            fn noop() {
                return;
            }
            fn main() -> i32 { return 0; }
        "#;
        let result = analyze(source);
        assert!(result.is_ok(), "void function should not be checked by A031: {result:?}");
    }

    #[test]
    fn a031_method_compound_return_checked() {
        let source = r#"
            struct Point {
                x: i32;
                y: i32;
                fn origin() -> Point {
                    return Point { x: 0, y: 0 };
                }
            }
            fn main() -> i32 { return 0; }
        "#;
        let result = analyze(source);
        assert!(result.is_ok(), "method with compound return literal should be accepted: {result:?}");
    }
}
