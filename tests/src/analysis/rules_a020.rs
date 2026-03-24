/// Integration tests for analysis rule A020: Dead code detection.
#[cfg(test)]
mod dead_code_tests {
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

    fn expect_warnings(source: &str) -> Vec<AnalysisDiagnostic> {
        let result = analyze(source);
        match result {
            Ok(r) => r.warnings().to_vec(),
            Err(e) => e.warnings().to_vec(),
        }
    }

    #[test]
    fn a020_dead_code_after_return() {
        let source = r#"
            fn foo() -> i32 {
                return 42;
                let x: i32 = 0;
            }
        "#;
        let warnings = expect_warnings(source);
        let has_a020 = warnings
            .iter()
            .any(|w| matches!(w, AnalysisDiagnostic::DeadCode { .. }));
        assert!(has_a020, "expected DeadCode warning, got: {warnings:?}");
    }

    #[test]
    fn a020_dead_code_after_break() {
        let source = r#"
            fn foo() -> i32 {
                let mut x: i32 = 0;
                loop {
                    break;
                    x = 1;
                }
                return x;
            }
        "#;
        let warnings = expect_warnings(source);
        let has_a020 = warnings
            .iter()
            .any(|w| matches!(w, AnalysisDiagnostic::DeadCode { .. }));
        assert!(
            has_a020,
            "expected DeadCode warning for code after break, got: {warnings:?}"
        );
    }

    #[test]
    fn a020_no_warning_for_live_code() {
        let source = r#"
            fn foo() -> i32 {
                let x: i32 = 42;
                return x;
            }
        "#;
        let warnings = expect_warnings(source);
        let has_a020 = warnings
            .iter()
            .any(|w| matches!(w, AnalysisDiagnostic::DeadCode { .. }));
        assert!(
            !has_a020,
            "should NOT trigger DeadCode for live code, got: {warnings:?}"
        );
    }

    #[test]
    fn a020_dead_code_after_if_else_both_return() {
        let source = r#"
            fn foo(x: i32) -> i32 {
                if x > 0 {
                    return 1;
                } else {
                    return 2;
                }
                let y: i32 = 0;
            }
        "#;
        let warnings = expect_warnings(source);
        let has_a020 = warnings
            .iter()
            .any(|w| matches!(w, AnalysisDiagnostic::DeadCode { .. }));
        assert!(
            has_a020,
            "expected DeadCode warning after if-else that both return, got: {warnings:?}"
        );
    }

    #[test]
    fn a020_no_warning_when_only_then_returns() {
        let source = r#"
            fn foo(x: i32) -> i32 {
                if x > 0 {
                    return 1;
                }
                return 0;
            }
        "#;
        let warnings = expect_warnings(source);
        let has_a020 = warnings
            .iter()
            .any(|w| matches!(w, AnalysisDiagnostic::DeadCode { .. }));
        assert!(
            !has_a020,
            "should NOT trigger DeadCode when only then-branch returns, got: {warnings:?}"
        );
    }

    #[test]
    fn a020_missing_return_fixed_with_early_return() {
        let source = r#"
            fn foo() -> i32 {
                return 42;
                let x: i32 = 0;
            }
        "#;
        let result = analyze(source);
        match &result {
            Ok(r) => {
                let has_a020 = r
                    .warnings()
                    .iter()
                    .any(|w| matches!(w, AnalysisDiagnostic::DeadCode { .. }));
                assert!(has_a020, "expected DeadCode warning");
            }
            Err(e) => {
                let has_a007 = e
                    .errors()
                    .iter()
                    .any(|err| matches!(err, AnalysisDiagnostic::MissingReturn { .. }));
                assert!(
                    !has_a007,
                    "should NOT trigger MissingReturn when early return exists, got: {e}"
                );
            }
        }
    }

    #[test]
    fn a020_multiple_dead_statements_after_return() {
        let source = r#"
            fn foo() -> i32 {
                return 1;
                let a: i32 = 0;
                let b: i32 = 0;
                let c: i32 = 0;
            }
        "#;
        let warnings = expect_warnings(source);
        let dead_code_count = warnings
            .iter()
            .filter(|w| matches!(w, AnalysisDiagnostic::DeadCode { .. }))
            .count();
        assert_eq!(
            dead_code_count, 3,
            "expected 3 DeadCode warnings (one per dead statement), got: {dead_code_count}"
        );
    }

    #[test]
    fn a020_dead_code_inside_nested_if() {
        let source = r#"
            fn foo(x: i32) -> i32 {
                if x > 0 {
                    return 1;
                    let y: i32 = 0;
                }
                return 0;
            }
        "#;
        let warnings = expect_warnings(source);
        let has_a020 = warnings
            .iter()
            .any(|w| matches!(w, AnalysisDiagnostic::DeadCode { .. }));
        assert!(
            has_a020,
            "expected DeadCode warning inside if branch, got: {warnings:?}"
        );
    }

    #[test]
    fn a020_dead_code_inside_loop() {
        let source = r#"
            fn foo() -> i32 {
                let x: i32 = 0;
                loop {
                    break;
                    let y: i32 = 1;
                    let z: i32 = 2;
                }
                return x;
            }
        "#;
        let warnings = expect_warnings(source);
        let dead_code_count = warnings
            .iter()
            .filter(|w| matches!(w, AnalysisDiagnostic::DeadCode { .. }))
            .count();
        assert_eq!(
            dead_code_count, 2,
            "expected 2 DeadCode warnings inside loop after break, got: {dead_code_count}"
        );
    }

    #[test]
    fn a020_dead_code_terminator_message_return() {
        let source = r#"
            fn foo() -> i32 {
                return 42;
                let x: i32 = 0;
            }
        "#;
        let warnings = expect_warnings(source);
        let dead = warnings
            .iter()
            .find(|w| matches!(w, AnalysisDiagnostic::DeadCode { .. }));
        assert!(dead.is_some(), "expected DeadCode warning");
        if let Some(AnalysisDiagnostic::DeadCode { terminator, .. }) = dead {
            assert_eq!(*terminator, "return");
        }
    }

    #[test]
    fn a020_dead_code_terminator_message_break() {
        let source = r#"
            fn foo() -> i32 {
                let mut x: i32 = 0;
                loop {
                    break;
                    x = 1;
                }
                return x;
            }
        "#;
        let warnings = expect_warnings(source);
        let dead = warnings
            .iter()
            .find(|w| matches!(w, AnalysisDiagnostic::DeadCode { .. }));
        assert!(dead.is_some(), "expected DeadCode warning");
        if let Some(AnalysisDiagnostic::DeadCode { terminator, .. }) = dead {
            assert_eq!(*terminator, "break");
        }
    }

    #[test]
    fn a020_dead_code_after_infinite_loop_without_break() {
        let source = r#"
            fn foo() {
                loop {
                    let x: i32 = 1;
                }
                let y: i32 = 0;
            }
        "#;
        let warnings = expect_warnings(source);
        let has_a020 = warnings
            .iter()
            .any(|w| matches!(w, AnalysisDiagnostic::DeadCode { .. }));
        assert!(
            has_a020,
            "expected DeadCode warning after infinite loop without break, got: {warnings:?}"
        );
    }

    #[test]
    fn a020_no_dead_code_after_infinite_loop_with_break() {
        let source = r#"
            fn foo() -> i32 {
                loop {
                    break;
                }
                return 0;
            }
        "#;
        let warnings = expect_warnings(source);
        let has_a020 = warnings
            .iter()
            .any(|w| matches!(w, AnalysisDiagnostic::DeadCode { .. }));
        assert!(
            !has_a020,
            "should NOT flag code after loop-with-break as dead, got: {warnings:?}"
        );
    }

    #[test]
    fn a020_no_dead_code_after_conditional_loop() {
        let source = r#"
            fn foo(x: i32) -> i32 {
                loop x > 0 {
                    break;
                }
                return 0;
            }
        "#;
        let warnings = expect_warnings(source);
        let has_a020 = warnings
            .iter()
            .any(|w| matches!(w, AnalysisDiagnostic::DeadCode { .. }));
        assert!(
            !has_a020,
            "should NOT flag code after conditional loop as dead, got: {warnings:?}"
        );
    }

    #[test]
    fn a020_dead_code_terminator_message_loop() {
        let source = r#"
            fn foo() {
                loop {
                    let x: i32 = 1;
                }
                let y: i32 = 0;
            }
        "#;
        let warnings = expect_warnings(source);
        let dead = warnings
            .iter()
            .find(|w| matches!(w, AnalysisDiagnostic::DeadCode { .. }));
        assert!(dead.is_some(), "expected DeadCode warning");
        if let Some(AnalysisDiagnostic::DeadCode { terminator, .. }) = dead {
            assert_eq!(*terminator, "loop");
        }
    }

    #[test]
    fn a020_dead_code_if_else_mixed_terminators_reports_conditional() {
        let source = r#"
            fn foo(x: i32) -> i32 {
                let mut result: i32 = 0;
                loop {
                    if x > 0 {
                        return 1;
                    } else {
                        break;
                    }
                    result = 42;
                }
                return result;
            }
        "#;
        let warnings = expect_warnings(source);
        let dead = warnings
            .iter()
            .find(|w| matches!(w, AnalysisDiagnostic::DeadCode { .. }));
        assert!(
            dead.is_some(),
            "expected DeadCode warning after if-else with mixed terminators"
        );
        if let Some(AnalysisDiagnostic::DeadCode { terminator, .. }) = dead {
            assert_eq!(
                *terminator, "conditional",
                "mixed return+break should report 'conditional', not '{terminator}'"
            );
        }
    }

    #[test]
    fn a020_dead_code_if_else_same_terminator_reports_kind() {
        let source = r#"
            fn foo(x: i32) -> i32 {
                if x > 0 {
                    return 1;
                } else {
                    return 2;
                }
                let y: i32 = 0;
            }
        "#;
        let warnings = expect_warnings(source);
        let dead = warnings
            .iter()
            .find(|w| matches!(w, AnalysisDiagnostic::DeadCode { .. }));
        assert!(dead.is_some(), "expected DeadCode warning");
        if let Some(AnalysisDiagnostic::DeadCode { terminator, .. }) = dead {
            assert_eq!(*terminator, "return");
        }
    }
}
