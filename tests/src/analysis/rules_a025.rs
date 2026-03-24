/// Integration tests for analysis rule A025.
///
/// - A025: UninitializedVariable — variable declarations must have an initializer
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

    // --- A025: Uninitialized variable ---

    #[test]
    fn a025_uninitialized_i32_rejected() {
        let source = "fn main() { let x: i32; }";
        let errors = expect_errors(source);
        let has_a025 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. }));
        assert!(has_a025, "expected UninitializedVariable, got: {errors:?}");
    }

    #[test]
    fn a025_initialized_i32_accepted() {
        let source = "fn main() { let x: i32 = 0; }";
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a025 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. }));
            assert!(!has_a025, "UninitializedVariable should not fire for initialized variable");
        }
    }

    #[test]
    fn a025_uninitialized_struct_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() { let p: Point; }
        "#;
        let errors = expect_errors(source);
        let has_a025 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. }));
        assert!(has_a025, "expected UninitializedVariable for struct, got: {errors:?}");
    }

    #[test]
    fn a025_uninitialized_array_rejected() {
        let source = "fn main() { let arr: [i32; 3]; }";
        let errors = expect_errors(source);
        let has_a025 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. }));
        assert!(has_a025, "expected UninitializedVariable for array, got: {errors:?}");
    }

    #[test]
    fn a025_uninitialized_mutable_bool_rejected() {
        let source = "fn main() { let mut x: bool; }";
        let errors = expect_errors(source);
        let has_a025 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. }));
        assert!(has_a025, "expected UninitializedVariable for mutable bool, got: {errors:?}");
    }

    #[test]
    fn a025_error_message_includes_variable_name() {
        let source = "fn main() { let my_var: i32; }";
        let errors = expect_errors(source);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. }))
            .expect("expected UninitializedVariable");
        let msg = diag.to_string();
        assert!(
            msg.contains("my_var"),
            "error message should include variable name, got: {msg}"
        );
    }

    #[test]
    fn a025_error_message_suggests_initialization() {
        let source = "fn main() { let x: i32; }";
        let errors = expect_errors(source);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. }))
            .expect("expected UninitializedVariable");
        let msg = diag.to_string();
        assert!(
            msg.contains("must be initialized at declaration"),
            "error message should suggest initialization, got: {msg}"
        );
    }

    #[test]
    fn a025_multiple_uninitialized_variables_all_reported() {
        let source = r#"
            fn main() {
                let a: i32;
                let b: bool;
            }
        "#;
        let errors = expect_errors(source);
        let a025_count = errors
            .iter()
            .filter(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. }))
            .count();
        assert_eq!(
            a025_count, 2,
            "expected 2 UninitializedVariable errors, got: {errors:?}"
        );
    }

    #[test]
    fn a025_rule_id_is_a025() {
        let source = "fn main() { let x: i32; }";
        let errors = expect_errors(source);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. }))
            .expect("expected UninitializedVariable");
        assert_eq!(diag.rule_id(), "A025");
    }

    #[test]
    fn a025_no_duplicate_diagnostics_for_uninitialized_var() {
        let source = r#"
            fn main() {
                let x: i32;
            }
        "#;
        let ctx = type_check(source);
        let result = inference_analysis::analyze(&ctx);
        let errors = result
            .expect_err("expected analysis error")
            .errors()
            .to_vec();
        let uninit_count = errors
            .iter()
            .filter(|e| matches!(e, AnalysisDiagnostic::UninitializedVariable { .. }))
            .count();
        assert_eq!(
            uninit_count, 1,
            "expected exactly 1 UninitializedVariable error, got {uninit_count}: {errors:?}"
        );
    }

    #[test]
    fn a025_initialized_variable_produces_no_error() {
        let source = r#"
            fn main() {
                let x: i32 = 0;
            }
        "#;
        let result = analyze(source);
        assert!(
            result.is_ok(),
            "initialized variable should not produce any analysis errors"
        );
    }
}
