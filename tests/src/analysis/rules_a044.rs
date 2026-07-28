/// Integration tests for analysis rule A044.
///
/// - A044: ShiftCountOutOfRange — a shift (`<<`/`>>`) whose count is a
///   statically-known literal that is negative or `>=` the operand type's bit
///   width is rejected (`x << 32`, `x >> -1` on `i32`). The rule complements the
///   runtime rule that a shift count is taken modulo the operand type's bit
///   width: a literal count outside `0..width` is a program error, not a value
///   to fold silently.
///
/// These tests exercise the rule through a real parse -> type-check -> analyze
/// pipeline, complementing the in-crate message/`rule_id` unit test in
/// `core/analysis`. The width is read from the operand type, and a literal
/// count now takes the type of the operand it shifts, so the rule covers every
/// width — the `i64` and `u8` cases at the end of this module are only
/// reachable because of that.
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

    /// Returns true if any analysis error is a `ShiftCountOutOfRange` (A044).
    /// Filters by variant rather than asserting a total error count, since the
    /// surface may also trip unrelated rules (or warnings).
    fn has_a044(source: &str) -> bool {
        match analyze(source) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::ShiftCountOutOfRange { .. })),
        }
    }

    fn a044_diag(source: &str) -> AnalysisDiagnostic {
        analyze(source)
            .expect_err("expected analysis errors but got Ok")
            .errors()
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::ShiftCountOutOfRange { .. }))
            .expect("expected a ShiftCountOutOfRange diagnostic")
            .clone()
    }

    /// Counts how many `ShiftCountOutOfRange` (A044) diagnostics the analysis
    /// emits for `source`, filtering by variant so unrelated rules do not perturb
    /// the count.
    fn count_a044(source: &str) -> usize {
        match analyze(source) {
            Ok(_) => 0,
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::ShiftCountOutOfRange { .. }))
                .count(),
        }
    }

    /// Wraps a shift expression in a minimal `i32 -> i32` function body.
    fn wrap(expr: &str) -> String {
        format!("fn f(x: i32) -> i32 {{ return {expr}; }}")
    }

    // ---------------------------------------------------------------------
    // Fires: a literal count out of range for the operand type
    // ---------------------------------------------------------------------

    #[test]
    fn a044_shl_count_equal_to_width_rejected() {
        assert!(has_a044(&wrap("x << 32")), "`x << 32` on i32 must be rejected");
    }

    #[test]
    fn a044_shl_count_above_width_rejected() {
        assert!(has_a044(&wrap("x << 33")), "`x << 33` on i32 must be rejected");
    }

    #[test]
    fn a044_shr_count_equal_to_width_rejected() {
        assert!(has_a044(&wrap("x >> 32")), "`x >> 32` on i32 must be rejected");
    }

    #[test]
    fn a044_shl_negative_count_rejected() {
        assert!(has_a044(&wrap("x << -1")), "`x << -1` must be rejected");
    }

    #[test]
    fn a044_shr_negative_count_rejected() {
        assert!(has_a044(&wrap("x >> -5")), "`x >> -5` must be rejected");
    }

    #[test]
    fn a044_parenthesized_count_rejected() {
        assert!(
            has_a044(&wrap("x << (33)")),
            "a parenthesized out-of-range count must still be rejected"
        );
    }

    #[test]
    fn a044_parenthesized_negative_count_rejected() {
        assert!(
            has_a044(&wrap("x << (-1)")),
            "a parenthesized negative count must still be rejected"
        );
    }

    // ---------------------------------------------------------------------
    // Does not fire
    // ---------------------------------------------------------------------

    #[test]
    fn a044_zero_count_accepted() {
        assert!(!has_a044(&wrap("x << 0")), "`x << 0` is in range");
    }

    #[test]
    fn a044_max_valid_shl_count_accepted() {
        assert!(!has_a044(&wrap("x << 31")), "`x << 31` is the largest valid i32 count");
    }

    #[test]
    fn a044_max_valid_shr_count_accepted() {
        assert!(!has_a044(&wrap("x >> 31")), "`x >> 31` is the largest valid i32 count");
    }

    #[test]
    fn a044_dynamic_count_accepted() {
        let source = "fn f(x: i32, k: i32) -> i32 { return x << k; }";
        assert!(
            !has_a044(source),
            "a dynamic (non-literal) count is out of scope for a static-literal rule"
        );
    }

    /// Documented limitation: a const-declared count is a statically-known value
    /// but reaches the rule as an opaque identifier, so it is not detected — the
    /// same literal-only scope as A022 and the division-by-zero check.
    #[test]
    fn a044_const_declared_count_not_detected() {
        let source = "fn f(x: i32) -> i32 { const K: i32 = 33; return x << K; }";
        assert!(
            !has_a044(source),
            "a const-declared count is out of scope (documented limitation)"
        );
    }

    #[test]
    fn a044_non_shift_binary_accepted() {
        assert!(!has_a044(&wrap("x + 100")), "a non-shift binary expression is out of scope");
    }

    // ---------------------------------------------------------------------
    // Uniform applicability and quality
    // ---------------------------------------------------------------------

    /// The rule walks every function body, including a `spec`-inner function, so
    /// an out-of-range literal shift in a spec is rejected just like one in an
    /// ordinary function.
    #[test]
    fn a044_fires_in_spec_function_body() {
        let source = r#"
            fn main() -> i32 { return 0; }
            spec S {
                fn bad() -> i32 {
                    let x: i32 = 5;
                    return x << 33;
                }
            }
        "#;
        assert!(
            has_a044(source),
            "an out-of-range literal shift inside a spec function must fire A044"
        );
    }

    /// Two out-of-range shifts in one body yield exactly two diagnostics.
    #[test]
    fn a044_two_bad_shifts_two_diagnostics() {
        let source = r#"
            fn f(x: i32) -> i32 {
                let a: i32 = x << 32;
                let b: i32 = x >> 40;
                return a + b;
            }
        "#;
        assert_eq!(
            count_a044(source),
            2,
            "two out-of-range shifts must yield exactly two A044 diagnostics"
        );
    }

    /// A shift count is peer-typed against the value being shifted, so an
    /// `i64` shift is checked against a 64-bit width rather than `i32`'s 32.
    #[test]
    fn a044_i64_shift_count_out_of_range() {
        let source = r#"
            fn test(x: i64) -> i64 {
                return x << 64;
            }
        "#;
        assert!(
            has_a044(source),
            "a count of 64 is out of range for a 64-bit operand"
        );
    }

    #[test]
    fn a044_i64_shift_count_at_the_width_boundary_accepted() {
        let source = r#"
            fn test(x: i64) -> i64 {
                return x << 63;
            }
        "#;
        assert!(
            !has_a044(source),
            "a count of 63 is the largest valid count for a 64-bit operand"
        );
    }

    /// When both operands are literals neither can type the other, and the
    /// width comes from what the position expects of the whole shift — a
    /// different path to the operand type than the peer cases above.
    #[test]
    fn a044_both_operands_literal_takes_the_width_from_the_expected_type() {
        let source = r#"
            fn test() -> i64 {
                let x: i64 = 1 << 64;
                return x;
            }
        "#;
        assert!(
            has_a044(source),
            "a count of 64 is out of range once the annotation types the shift `i64`"
        );
    }

    #[test]
    fn a044_both_operands_literal_within_the_expected_width_accepted() {
        let source = r#"
            fn test() -> i64 {
                let x: i64 = 1 << 40;
                return x;
            }
        "#;
        assert!(
            !has_a044(source),
            "a count of 40 is valid for a 64-bit operand"
        );
    }

    /// A count that `i32` would have accepted is out of range for a narrow
    /// operand, which is equally unreachable while every literal is `i32`.
    #[test]
    fn a044_narrow_shift_count_out_of_range() {
        let source = r#"
            fn test(x: u8) -> u8 {
                return x << 8;
            }
        "#;
        assert!(
            has_a044(source),
            "a count of 8 is out of range for an 8-bit operand"
        );
    }

    #[test]
    fn a044_i64_diagnostic_names_the_wider_range() {
        let source = r#"
            fn test(x: i64) -> i64 {
                return x >> 100;
            }
        "#;
        let diag = a044_diag(source);
        assert!(
            matches!(
                &diag,
                AnalysisDiagnostic::ShiftCountOutOfRange { value, type_name, max, .. }
                    if value == "100" && type_name == "i64" && *max == 63
            ),
            "expected A044 with value 100, type i64, max 63, got: {diag}"
        );
        assert!(
            diag.to_string().contains("0..=63"),
            "the i64 message must state the 64-bit count range, got: {diag}"
        );
    }

    /// Diagnostic quality through the real pipeline: the finding names the count,
    /// the operand type, the valid range, and reports rule id A044.
    #[test]
    fn a044_diagnostic_quality() {
        let diag = a044_diag(&wrap("x << 32"));
        assert!(
            matches!(
                &diag,
                AnalysisDiagnostic::ShiftCountOutOfRange { value, type_name, max, .. }
                    if value == "32" && type_name == "i32" && *max == 31
            ),
            "expected A044 with value 32, type i32, max 31, got: {diag}"
        );
        let msg = diag.to_string();
        assert!(
            msg.contains("shift count `32`"),
            "A044 message must name the offending count, got: {msg}"
        );
        assert!(
            msg.contains("type `i32`"),
            "A044 message must name the operand type, got: {msg}"
        );
        assert!(
            msg.contains("0..=31"),
            "A044 message must state the valid count range, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A044");
    }
}
