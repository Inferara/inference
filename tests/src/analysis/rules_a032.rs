/// Integration tests for analysis rule A032.
///
/// - A032: TopLevelConstNotSupported -- module-scope `const` declarations are
///   rejected with a clear diagnostic instead of silently reaching codegen
///   (which panics with "Variable not found" at any use site).
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

    // --- A032: Top-level const declarations are rejected ---

    #[test]
    fn a032_top_level_scalar_const_rejected() {
        let source = r#"const X: i32 = 42;"#;
        let errors = expect_errors(source);
        let has_a032 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::TopLevelConstNotSupported { name, .. } if name == "X"));
        assert!(
            has_a032,
            "expected TopLevelConstNotSupported for scalar top-level const, got: {errors:?}"
        );
    }

    #[test]
    fn a032_top_level_array_const_rejected() {
        let source = r#"const ARR: [i32; 3] = [1, 2, 3];"#;
        let errors = expect_errors(source);
        let has_a032 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::TopLevelConstNotSupported { name, .. } if name == "ARR"));
        assert!(
            has_a032,
            "expected TopLevelConstNotSupported for top-level array const, got: {errors:?}"
        );
        // A015 walks function bodies only, so a module-scope compound const must
        // not produce a CompoundLiteralInUnsupportedPosition diagnostic. Pinning
        // this guards against future regressions that would double-report.
        let has_a015 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition { .. }));
        assert!(
            !has_a015,
            "top-level compound const should fire A032 only, not A015, got: {errors:?}"
        );
    }

    #[test]
    fn a032_multiple_top_level_consts_emit_multiple_diagnostics() {
        let source = r#"
            const A: i32 = 1;
            const B: i32 = 2;
            const C: [i32; 2] = [3, 4];
        "#;
        let errors = expect_errors(source);
        let a032_count = errors
            .iter()
            .filter(|e| matches!(e, AnalysisDiagnostic::TopLevelConstNotSupported { .. }))
            .count();
        assert_eq!(
            a032_count, 3,
            "expected one A032 per top-level const, got {a032_count} in: {errors:?}"
        );
    }

    #[test]
    fn a032_top_level_struct_const_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            const P: Point = Point { x: 1, y: 2 };
        "#;
        let errors = expect_errors(source);
        let has_a032 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::TopLevelConstNotSupported { name, .. } if name == "P"));
        assert!(
            has_a032,
            "expected TopLevelConstNotSupported for top-level struct const, got: {errors:?}"
        );
        // A015 walks function bodies only, so a module-scope compound const must
        // not produce a CompoundLiteralInUnsupportedPosition diagnostic.
        let has_a015 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInUnsupportedPosition { .. }));
        assert!(
            !has_a015,
            "top-level struct const should fire A032 only, not A015, got: {errors:?}"
        );
    }

    #[test]
    fn a032_function_scoped_const_not_rejected() {
        let source = r#"
            fn test() -> i32 {
                const X: i32 = 42;
                return X;
            }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_a032 = errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::TopLevelConstNotSupported { .. }));
            assert!(
                !has_a032,
                "function-scoped const should NOT trigger A032, got: {errors}"
            );
        }
    }

    #[test]
    fn a032_function_scoped_compound_const_not_rejected() {
        let source = r#"
            fn test() -> i32 {
                const ARR: [i32; 3] = [1, 2, 3];
                return ARR[0];
            }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_a032 = errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::TopLevelConstNotSupported { .. }));
            assert!(
                !has_a032,
                "function-scoped compound const should NOT trigger A032, got: {errors}"
            );
        }
    }

    #[test]
    fn a032_diagnostic_message_mentions_function_body_and_issue_link() {
        let source = r#"const X: i32 = 42;"#;
        let errors = expect_errors(source);
        let a032 = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::TopLevelConstNotSupported { .. }))
            .expect("expected A032 diagnostic");
        let text = a032.to_string();
        assert!(
            text.contains("inside a function body"),
            "A032 message should suggest declaring inside a function body, got: {text}"
        );
        assert!(
            text.contains("171"),
            "A032 message should reference the tracking issue, got: {text}"
        );
    }

    #[test]
    fn a032_const_in_spec_also_rejected() {
        let source = r#"
            spec S {
                const X: i32 = 42;
            }
        "#;
        let errors = expect_errors(source);
        let has_a032 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::TopLevelConstNotSupported { name, .. } if name == "X"));
        assert!(
            has_a032,
            "expected TopLevelConstNotSupported for const in spec, got: {errors:?}"
        );
    }
}
