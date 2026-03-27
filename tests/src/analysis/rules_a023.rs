/// Integration tests for analysis rule A023.
///
/// - A023: UzumakiInReassignment
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

    // --- A023: Uzumaki in reassignment ---

    #[test]
    fn a023_scalar_uzumaki_reassignment_rejected() {
        let source = r#"
            fn main() {
                forall {
                    let mut x: i32 = 0;
                    x = @;
                }
            }
        "#;
        let errors = expect_errors(source);
        let has_a023 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiInReassignment { .. }));
        assert!(has_a023, "expected UzumakiInReassignment, got: {errors:?}");
    }

    #[test]
    fn a023_struct_uzumaki_reassignment_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() {
                forall {
                    let mut p: Point = Point { x: 1, y: 2 };
                    p = @;
                }
            }
        "#;
        let errors = expect_errors(source);
        let has_a023 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiInReassignment { .. }));
        assert!(has_a023, "expected UzumakiInReassignment for struct, got: {errors:?}");
    }

    #[test]
    fn a023_array_uzumaki_reassignment_rejected() {
        let source = r#"
            fn main() {
                forall {
                    let mut a: [i32; 3] = [1, 2, 3];
                    a = @;
                }
            }
        "#;
        let errors = expect_errors(source);
        let has_a023 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiInReassignment { .. }));
        assert!(has_a023, "expected UzumakiInReassignment for array, got: {errors:?}");
    }

    #[test]
    fn a023_bool_uzumaki_reassignment_rejected() {
        let source = r#"
            fn main() {
                forall {
                    let mut b: bool = true;
                    b = @;
                }
            }
        "#;
        let errors = expect_errors(source);
        let has_a023 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiInReassignment { .. }));
        assert!(has_a023, "expected UzumakiInReassignment for bool, got: {errors:?}");
    }

    #[test]
    fn a023_uzumaki_in_vardef_not_rejected() {
        let source = r#"
            fn main() {
                forall {
                    let x: i32 = @;
                }
            }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a023 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UzumakiInReassignment { .. }));
            assert!(!has_a023, "UzumakiInReassignment should not fire on let binding");
        }
    }

    #[test]
    fn a023_uzumaki_outside_nondet_fires_both_a006_and_a023() {
        let source = r#"
            fn main() {
                let mut x: i32 = 0;
                x = @;
            }
        "#;
        let errors = expect_errors(source);
        let has_a006 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOutsideNonDetBlock { .. }));
        let has_a023 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiInReassignment { .. }));
        assert!(has_a006, "expected A006, got: {errors:?}");
        assert!(has_a023, "expected A023, got: {errors:?}");
    }
}
