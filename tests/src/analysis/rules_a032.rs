/// Integration tests for analysis rule A032.
///
/// - A032: ConstInSpecNotSupported -- a `const` declared inside a `spec` block
///   is rejected with a diagnostic at the declaration. Module-scope constants,
///   which the rule once rejected too, are computed by the type checker and
///   emitted by code generation, and pass analysis.
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

    // --- Module-scope constants are accepted ---

    /// Scalar, array and struct module constants, read from a function, pass
    /// every rule: none of them is a construct analysis has to refuse any more.
    #[test]
    fn a032_module_scope_consts_pass_analysis() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            const X: i32 = 42;
            const ARR: [i32; 3] = [1, 2, 3];
            const P: Point = Point { x: 1, y: 2 };
            pub fn read() -> i32 { return X + ARR[2] + P.y; }
        "#;
        if let Err(errors) = analyze(source) {
            panic!("module-scope constants must pass analysis, got: {errors}");
        }
    }

    #[test]
    fn a032_function_scoped_const_not_rejected() {
        let source = r#"
            fn test() -> i32 {
                const X: i32 = 42;
                const ARR: [i32; 3] = [1, 2, 3];
                return X + ARR[0];
            }
        "#;
        if let Err(errors) = analyze(source) {
            panic!("function-scoped consts must pass analysis, got: {errors}");
        }
    }

    // --- A032: a const inside a spec is rejected ---

    #[test]
    fn a032_const_in_spec_rejected() {
        let source = r#"
            spec S {
                const X: i32 = 42;
            }
        "#;
        let errors = expect_errors(source);
        let a032 = errors
            .iter()
            .find(|e| {
                matches!(e, AnalysisDiagnostic::ConstInSpecNotSupported { name, spec_name, .. }
                    if name == "X" && spec_name == "S")
            })
            .unwrap_or_else(|| panic!("expected A032 for a const in a spec, got: {errors:?}"));
        assert_eq!(a032.rule_id(), "A032");
        let text = a032.to_string();
        assert!(
            text.contains("declare `X` at module scope, outside `spec S`"),
            "A032 should name the fix, got: {text}"
        );
    }

    /// Every spec-inner const is reported, and a module-scope one beside them
    /// is not.
    #[test]
    fn a032_reports_each_spec_inner_const_and_no_module_const() {
        let source = r#"
            const OUTER: i32 = 1;
            spec S {
                const A: i32 = 1;
                const B: [i32; 2] = [3, 4];
            }
        "#;
        let errors = expect_errors(source);
        let names: Vec<&str> = errors
            .iter()
            .filter_map(|e| match e {
                AnalysisDiagnostic::ConstInSpecNotSupported { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["A", "B"], "got: {errors:?}");
    }
}
