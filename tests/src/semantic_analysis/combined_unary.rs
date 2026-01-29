//! Tests for the semantic analysis pass that prohibits combined unary operators.
//!
//! These checks run after type checking and detect chained/combined prefix
//! unary operators such as `--x`, `!!x`, `-~x`, and parenthesized variants
//! like `-(~x)`.

#[cfg(test)]
mod combined_unary_tests {
    use crate::utils::build_ast;
    use inference_semantic_analysis::diagnostics::Severity;
    use inference_type_checker::TypeCheckerBuilder;

    /// Runs parse → type check → semantic analysis on the source, returning the
    /// semantic result.
    fn run_semantic_analysis(
        source: &str,
    ) -> inference_semantic_analysis::diagnostics::SemanticResult {
        let arena = build_ast(source.to_string());
        let typed_context = TypeCheckerBuilder::build_typed_context(arena)
            .expect("type checking should succeed before semantic analysis")
            .typed_context();
        inference_semantic_analysis::analyze(&typed_context)
    }

    // ── single unary operators should pass ──────────────────────────────

    #[test]
    fn single_negate_succeeds() {
        let result = run_semantic_analysis(r#"fn test(x: i32) -> i32 { return -(x); }"#);
        assert!(
            !result.has_errors(),
            "Single negation should not produce errors"
        );
    }

    #[test]
    fn single_bitnot_succeeds() {
        let result = run_semantic_analysis(r#"fn test(x: i32) -> i32 { return ~x; }"#);
        assert!(
            !result.has_errors(),
            "Single bitwise NOT should not produce errors"
        );
    }

    #[test]
    fn single_logical_not_succeeds() {
        let result = run_semantic_analysis(r#"fn test(x: bool) -> bool { return !x; }"#);
        assert!(
            !result.has_errors(),
            "Single logical NOT should not produce errors"
        );
    }

    #[test]
    fn negate_parenthesized_arithmetic_succeeds() {
        let result =
            run_semantic_analysis(r#"fn test(a: i32, b: i32) -> i32 { return -(a + b); }"#);
        assert!(
            !result.has_errors(),
            "Negation of parenthesized arithmetic should not produce errors"
        );
    }

    // ── combined / chained unary operators should error ──────────────────

    #[test]
    fn double_negate_is_prohibited() {
        let result = run_semantic_analysis(r#"fn test(x: i32) -> i32 { return --(x); }"#);
        assert!(result.has_errors(), "Double negation should be prohibited");
        let errors = result.errors();
        assert!(
            errors
                .iter()
                .any(|d| d.message.contains("combined unary operators")),
            "Error should mention combined unary operators"
        );
        assert!(
            errors.iter().all(|d| d.severity == Severity::Error),
            "Combined unary diagnostics should be errors"
        );
    }

    #[test]
    fn double_negate_literal_is_prohibited() {
        let result = run_semantic_analysis(r#"fn test() -> i32 { return --42; }"#);
        assert!(
            result.has_errors(),
            "Double negation of literal should be prohibited"
        );
    }

    #[test]
    fn bitnot_combined_with_negate_is_prohibited() {
        let result = run_semantic_analysis(r#"fn test(x: i32) -> i32 { return ~-(x); }"#);
        assert!(
            result.has_errors(),
            "Combining BitNot and Neg should be prohibited"
        );
    }

    #[test]
    fn negate_combined_with_bitnot_is_prohibited() {
        let result = run_semantic_analysis(r#"fn test(x: i32) -> i32 { return -(~x); }"#);
        assert!(
            result.has_errors(),
            "Combining Neg and BitNot should be prohibited"
        );
    }

    #[test]
    fn bitnot_then_neg_literal_is_prohibited() {
        let result = run_semantic_analysis(r#"fn test() -> i32 { return -~42; }"#);
        assert!(
            result.has_errors(),
            "Combined unary operators on literal should be prohibited"
        );
    }

    #[test]
    fn double_logical_not_is_prohibited() {
        let result = run_semantic_analysis(r#"fn test(x: bool) -> bool { return !!x; }"#);
        assert!(
            result.has_errors(),
            "Double logical NOT should be prohibited"
        );
    }
}
