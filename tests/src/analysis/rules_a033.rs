/// Integration tests for analysis rule A033.
///
/// - A033: CombinedUnaryOperators -- chained/adjacent prefix unary operators
///   such as `--x`, `~~x`, `-~x`, `!!x`, and parenthesized variants like
///   `-(~x)` are rejected.
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

    fn has_a033(errors: &[AnalysisDiagnostic]) -> bool {
        errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CombinedUnaryOperators { .. }))
    }

    fn assert_no_a033(source: &str) {
        let result = analyze(source);
        if let Err(errors) = &result {
            assert!(
                !has_a033(errors.errors()),
                "did not expect A033, got: {errors}"
            );
        }
    }

    // --- A033: single unary operators are accepted ---

    #[test]
    fn a033_single_negation_accepted() {
        assert_no_a033(r#"fn test(x: i32) -> i32 { return -(x); }"#);
    }

    #[test]
    fn a033_single_bitnot_accepted() {
        assert_no_a033(r#"fn test(x: i32) -> i32 { return ~x; }"#);
    }

    #[test]
    fn a033_single_logical_not_accepted() {
        assert_no_a033(r#"fn test(x: bool) -> bool { return !x; }"#);
    }

    #[test]
    fn a033_negation_of_parenthesized_arithmetic_accepted() {
        assert_no_a033(r#"fn test(a: i32, b: i32) -> i32 { return -(a + b); }"#);
    }

    #[test]
    fn a033_single_negation_of_literal_accepted() {
        // Regression: prior heuristic flagged `-42` because it confused
        // "literal that starts with -" with "combined unary". The parser
        // stores number literals without a leading sign, so this must pass.
        assert_no_a033(r#"fn test() -> i32 { return -42; }"#);
    }

    #[test]
    fn a033_negative_initializer_accepted() {
        assert_no_a033(
            r#"fn test() -> i32 {
                let y: i32 = -42;
                return y;
            }"#,
        );
    }

    // --- A033: combined/chained unary operators are rejected ---

    #[test]
    fn a033_double_negation_rejected() {
        let errors = expect_errors(r#"fn test(x: i32) -> i32 { return --(x); }"#);
        assert!(has_a033(&errors), "expected A033, got: {errors:?}");
    }

    #[test]
    fn a033_double_negation_bare_rejected() {
        // Issue #81: the bare `--x` form (no parentheses around the operand)
        // is the literal case the issue names and must be prohibited.
        let errors = expect_errors(r#"fn test(x: i32) -> i32 { return --x; }"#);
        assert!(has_a033(&errors), "expected A033, got: {errors:?}");
    }

    #[test]
    fn a033_double_bitnot_rejected() {
        // Issue #81: the `~~x` form (double bitwise NOT) must be prohibited.
        let errors = expect_errors(r#"fn test(x: i32) -> i32 { return ~~x; }"#);
        assert!(has_a033(&errors), "expected A033, got: {errors:?}");
    }

    #[test]
    fn a033_double_negation_literal_rejected() {
        let errors = expect_errors(r#"fn test() -> i32 { return --42; }"#);
        assert!(has_a033(&errors), "expected A033, got: {errors:?}");
    }

    #[test]
    fn a033_bitnot_then_neg_rejected() {
        let errors = expect_errors(r#"fn test(x: i32) -> i32 { return ~-(x); }"#);
        assert!(has_a033(&errors), "expected A033, got: {errors:?}");
    }

    #[test]
    fn a033_neg_then_bitnot_parenthesized_rejected() {
        let errors = expect_errors(r#"fn test(x: i32) -> i32 { return -(~x); }"#);
        assert!(has_a033(&errors), "expected A033, got: {errors:?}");
    }

    #[test]
    fn a033_neg_then_bitnot_literal_rejected() {
        let errors = expect_errors(r#"fn test() -> i32 { return -~42; }"#);
        assert!(has_a033(&errors), "expected A033, got: {errors:?}");
    }

    #[test]
    fn a033_double_logical_not_rejected() {
        let errors = expect_errors(r#"fn test(x: bool) -> bool { return !!x; }"#);
        assert!(has_a033(&errors), "expected A033, got: {errors:?}");
    }

    // --- A033: diagnostic surface ---

    #[test]
    fn a033_rule_id_is_a033() {
        let errors = expect_errors(r#"fn test(x: i32) -> i32 { return -~x; }"#);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::CombinedUnaryOperators { .. }))
            .expect("expected A033 diagnostic");
        assert_eq!(diag.rule_id(), "A033");
    }

    #[test]
    fn a033_message_includes_operator_glyphs() {
        let errors = expect_errors(r#"fn test(x: i32) -> i32 { return -~x; }"#);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::CombinedUnaryOperators { .. }))
            .expect("expected A033 diagnostic");
        let text = diag.to_string();
        assert!(
            text.contains("-~"),
            "A033 message should include the combined glyphs, got: {text}"
        );
    }
}
