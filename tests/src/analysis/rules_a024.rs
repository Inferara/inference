/// Integration tests for analysis rule A024.
///
/// - A024: ExternFunctionCall
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

    // --- A024: External function call ---

    #[test]
    fn a024_call_to_extern_function_rejected() {
        let source = r#"
            external fn print(val: i32) -> ();
            fn main() { print(42); }
        "#;
        let errors = expect_errors(source);
        let has_a024 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ExternFunctionCall { .. }));
        assert!(has_a024, "expected ExternFunctionCall, got: {errors:?}");
    }

    #[test]
    fn a024_extern_function_declared_but_not_called_accepted() {
        let source = r#"
            external fn print(val: i32) -> ();
            fn main() -> i32 { return 42; }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a024 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::ExternFunctionCall { .. }));
            assert!(!has_a024, "ExternFunctionCall should not fire when extern fn is only declared");
        }
    }

    #[test]
    fn a024_multiple_calls_to_extern_function_rejected() {
        let source = r#"
            external fn print(val: i32) -> ();
            fn main() {
                print(1);
                print(2);
            }
        "#;
        let errors = expect_errors(source);
        let a024_count = errors
            .iter()
            .filter(|e| matches!(e, AnalysisDiagnostic::ExternFunctionCall { .. }))
            .count();
        assert_eq!(a024_count, 2, "expected 2 ExternFunctionCall errors, got: {errors:?}");
    }

    #[test]
    fn a024_call_to_extern_in_return_value_rejected() {
        let source = r#"
            external fn compute(x: i32) -> i32;
            fn main() -> i32 { return compute(10); }
        "#;
        let errors = expect_errors(source);
        let has_a024 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ExternFunctionCall { .. }));
        assert!(has_a024, "expected ExternFunctionCall in return position, got: {errors:?}");
    }

    #[test]
    fn a024_call_to_extern_in_var_init_rejected() {
        let source = r#"
            external fn compute(x: i32) -> i32;
            fn main() -> i32 {
                let v: i32 = compute(5);
                return v;
            }
        "#;
        let errors = expect_errors(source);
        let has_a024 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ExternFunctionCall { .. }));
        assert!(has_a024, "expected ExternFunctionCall in variable init, got: {errors:?}");
    }

    #[test]
    fn a024_call_to_regular_function_not_rejected() {
        let source = r#"
            fn helper(x: i32) -> i32 { return x; }
            fn main() -> i32 { return helper(42); }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a024 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::ExternFunctionCall { .. }));
            assert!(!has_a024, "ExternFunctionCall should not fire for regular functions");
        }
    }

    #[test]
    fn a024_error_message_includes_function_name() {
        let source = r#"
            external fn my_print(val: i32) -> ();
            fn main() { my_print(42); }
        "#;
        let errors = expect_errors(source);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::ExternFunctionCall { .. }))
            .expect("expected ExternFunctionCall");
        let msg = diag.to_string();
        assert!(
            msg.contains("my_print"),
            "error message should include function name, got: {msg}"
        );
    }

    #[test]
    fn a024_extern_with_no_args_rejected() {
        let source = r#"
            external fn do_something() -> ();
            fn main() { do_something(); }
        "#;
        let errors = expect_errors(source);
        let has_a024 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ExternFunctionCall { .. }));
        assert!(has_a024, "expected ExternFunctionCall for no-arg extern fn, got: {errors:?}");
    }

    #[test]
    fn a024_extern_in_nested_expression_rejected() {
        let source = r#"
            external fn compute(x: i32) -> i32;
            fn main() -> i32 {
                let v: i32 = compute(1) + compute(2);
                return v;
            }
        "#;
        let errors = expect_errors(source);
        let a024_count = errors
            .iter()
            .filter(|e| matches!(e, AnalysisDiagnostic::ExternFunctionCall { .. }))
            .count();
        assert_eq!(a024_count, 2, "expected 2 ExternFunctionCall errors for nested calls, got: {errors:?}");
    }
}
