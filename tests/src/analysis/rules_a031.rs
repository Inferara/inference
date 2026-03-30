/// Integration tests for analysis rule A031.
///
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

    #[allow(dead_code)]
    fn expect_errors(source: &str) -> Vec<AnalysisDiagnostic> {
        analyze(source)
            .expect_err("expected analysis errors but got Ok")
            .errors()
            .to_vec()
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

    #[test]
    fn a031_uzumaki_return_in_compound_function_rejected() {
        let source = r#"
            fn make() -> [i32; 3] {
                forall {
                    return @;
                }
                return [0, 0, 0];
            }
            fn main() -> i32 { return 0; }
        "#;
        let errors = expect_errors(source);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UnsupportedCompoundReturnExpression { .. })),
            "uzumaki as return expr in compound-returning function should be rejected by A031, got: {errors:?}"
        );
    }
}
