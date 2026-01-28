//! Tests for the semantic analysis pass that warns about combined unary operators.
//!
//! These checks run after type checking and detect chained/combined prefix
//! unary operators such as `--x`, `!!x`, `-~x`, and parenthesized variants
//! like `-(~x)`. These produce warnings (not errors) as they are style/readability
//! concerns rather than semantic violations.

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

    // ── combined / chained unary operators should produce warnings ────────

    #[test]
    fn double_negate_produces_warning() {
        let result = run_semantic_analysis(r#"fn test(x: i32) -> i32 { return --(x); }"#);
        assert!(
            result.has_warnings(),
            "Double negation should produce a warning"
        );
        let warnings = result.warnings();
        assert!(
            warnings
                .iter()
                .any(|d| d.message.contains("combined unary operators")),
            "Warning should mention combined unary operators"
        );
        assert!(
            warnings.iter().all(|d| d.severity == Severity::Warning),
            "Combined unary diagnostics should be warnings"
        );
        assert!(
            !result.has_errors(),
            "Combined unary operators should not produce errors"
        );
    }

    #[test]
    fn double_negate_literal_produces_warning() {
        let result = run_semantic_analysis(r#"fn test() -> i32 { return --42; }"#);
        assert!(
            result.has_warnings(),
            "Double negation of literal should produce a warning"
        );
        assert!(
            !result.has_errors(),
            "Double negation of literal should not produce errors"
        );
    }

    #[test]
    fn bitnot_combined_with_negate_produces_warning() {
        let result = run_semantic_analysis(r#"fn test(x: i32) -> i32 { return ~-(x); }"#);
        assert!(
            result.has_warnings(),
            "Combining BitNot and Neg should produce a warning"
        );
        assert!(
            !result.has_errors(),
            "Combining BitNot and Neg should not produce errors"
        );
    }

    #[test]
    fn negate_combined_with_bitnot_produces_warning() {
        let result = run_semantic_analysis(r#"fn test(x: i32) -> i32 { return -(~x); }"#);
        assert!(
            result.has_warnings(),
            "Combining Neg and BitNot should produce a warning"
        );
        assert!(
            !result.has_errors(),
            "Combining Neg and BitNot should not produce errors"
        );
    }

    #[test]
    fn bitnot_then_neg_literal_produces_warning() {
        let result = run_semantic_analysis(r#"fn test() -> i32 { return -~42; }"#);
        assert!(
            result.has_warnings(),
            "Combined unary operators on literal should produce a warning"
        );
        assert!(
            !result.has_errors(),
            "Combined unary operators on literal should not produce errors"
        );
    }

    #[test]
    fn double_logical_not_produces_warning() {
        let result = run_semantic_analysis(r#"fn test(x: bool) -> bool { return !!x; }"#);
        assert!(
            result.has_warnings(),
            "Double logical NOT should produce a warning"
        );
        assert!(
            !result.has_errors(),
            "Double logical NOT should not produce errors"
        );
    }

    #[test]
    fn double_bitnot_produces_warning() {
        let result = run_semantic_analysis(r#"fn test(x: i32) -> i32 { return ~~x; }"#);
        assert!(
            result.has_warnings(),
            "Double bitwise NOT should produce a warning"
        );
        assert!(
            !result.has_errors(),
            "Double bitwise NOT should not produce errors"
        );
    }

    #[test]
    fn triple_negate_produces_multiple_warnings() {
        let result = run_semantic_analysis(r#"fn test(x: i32) -> i32 { return ---(x); }"#);
        assert!(
            result.has_warnings(),
            "Triple negation should produce warnings"
        );
        let warnings = result.warnings();
        assert_eq!(
            warnings.len(),
            2,
            "Triple negation should produce 2 warnings (one per combined pair)"
        );
        assert!(
            !result.has_errors(),
            "Triple negation should not produce errors"
        );
    }

    #[test]
    fn deeply_nested_parentheses_combined_unary_produces_warning() {
        let result = run_semantic_analysis(r#"fn test(x: i32) -> i32 { return -(((~x))); }"#);
        assert!(
            result.has_warnings(),
            "Deeply nested combined unary should produce warning"
        );
        assert!(
            !result.has_errors(),
            "Deeply nested combined unary should not produce errors"
        );
    }

    #[test]
    fn warning_message_format_is_correct() {
        let result = run_semantic_analysis(r#"fn test(x: i32) -> i32 { return --x; }"#);
        let warnings = result.warnings();
        assert_eq!(warnings.len(), 1, "Should produce exactly one warning");

        let warning = &warnings[0];
        assert_eq!(warning.severity, Severity::Warning);
        assert!(
            warning.message.contains("combined unary operators"),
            "Message should mention combined unary operators"
        );
        assert!(
            warning.message.contains("hard to read"),
            "Message should mention readability"
        );
        assert!(
            warning.message.contains("simplifying"),
            "Message should suggest simplification"
        );
    }

    #[test]
    fn warning_location_is_valid() {
        let result = run_semantic_analysis(r#"fn test(x: i32) -> i32 { return --x; }"#);
        let warnings = result.warnings();
        assert_eq!(warnings.len(), 1, "Should produce exactly one warning");

        let warning = &warnings[0];
        assert!(
            warning.location.start_line > 0,
            "Location should have valid line number"
        );
        assert!(
            warning.location.start_column > 0,
            "Location should have valid column"
        );
    }

    #[test]
    fn combined_unary_in_binary_expression_produces_warning() {
        let result = run_semantic_analysis(r#"fn test(x: i32, y: i32) -> i32 { return --x + y; }"#);
        assert!(
            result.has_warnings(),
            "Combined unary in binary expression should produce warning"
        );
        assert!(
            !result.has_errors(),
            "Combined unary in binary expression should not produce errors"
        );
    }

    #[test]
    fn combined_unary_in_variable_initializer_produces_warning() {
        let result =
            run_semantic_analysis(r#"fn test(x: i32) -> i32 { let y: i32 = --x; return y; }"#);
        assert!(
            result.has_warnings(),
            "Combined unary in variable initializer should produce warning"
        );
        assert!(
            !result.has_errors(),
            "Combined unary in variable initializer should not produce errors"
        );
    }

    #[test]
    fn multiple_independent_combined_unary_produces_multiple_warnings() {
        let result =
            run_semantic_analysis(r#"fn test(x: i32, y: i32) -> i32 { return --x + ~~y; }"#);
        let warnings = result.warnings();
        assert_eq!(
            warnings.len(),
            2,
            "Two independent combined unary operators should produce 2 warnings"
        );
        assert!(
            !result.has_errors(),
            "Multiple combined unary operators should not produce errors"
        );
    }

    #[test]
    fn negate_negative_literal_in_parens_produces_warning() {
        let result = run_semantic_analysis(r#"fn test() -> i32 { return -(-42); }"#);
        assert!(
            result.has_warnings(),
            "Negating parenthesized negative literal should produce warning"
        );
        assert!(
            !result.has_errors(),
            "Negating parenthesized negative literal should not produce errors"
        );
    }

    #[test]
    fn empty_function_produces_no_warnings() {
        let result = run_semantic_analysis(r#"fn test() { }"#);
        assert!(
            !result.has_warnings(),
            "Empty function should not produce warnings"
        );
        assert!(
            !result.has_errors(),
            "Empty function should not produce errors"
        );
    }

    #[test]
    fn no_unary_operators_produces_no_warnings() {
        let result = run_semantic_analysis(r#"fn test(x: i32, y: i32) -> i32 { return x + y; }"#);
        assert!(
            !result.has_warnings(),
            "Binary operations should not trigger unary warnings"
        );
        assert!(
            !result.has_errors(),
            "Binary operations should not produce errors"
        );
    }
}
