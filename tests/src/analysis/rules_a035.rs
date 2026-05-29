/// Integration tests for analysis rule A035.
///
/// - A035: RecursionDetected — direct and mutual/indirect recursion is forbidden
///   (Power of 10, Rule 1).
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

    /// Returns true if any analysis error is a `RecursionDetected` (A035).
    /// Filters by variant rather than asserting a total error count, since a
    /// bare-function surface may also trip unrelated rules.
    fn has_recursion(source: &str) -> bool {
        match analyze(source) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::RecursionDetected { .. })),
        }
    }

    fn recursion_diag(source: &str) -> AnalysisDiagnostic {
        analyze(source)
            .expect_err("expected analysis errors but got Ok")
            .errors()
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::RecursionDetected { .. }))
            .expect("expected a RecursionDetected diagnostic")
            .clone()
    }

    #[test]
    fn a035_direct_recursion_rejected() {
        let source = "fn f() -> i32 { return f(); }";
        assert!(
            has_recursion(source),
            "expected RecursionDetected for direct self-recursion"
        );
    }

    #[test]
    fn a035_direct_recursion_names_cycle() {
        let diag = recursion_diag("fn f() -> i32 { return f(); }");
        let msg = diag.to_string();
        assert!(
            msg.contains("f -> f"),
            "diagnostic should name the cycle `f -> f`, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A035");
    }

    #[test]
    fn a035_mutual_recursion_rejected() {
        let source = "fn a() -> i32 { return b(); } fn b() -> i32 { return a(); }";
        assert!(
            has_recursion(source),
            "expected RecursionDetected for mutual recursion a <-> b"
        );
    }

    #[test]
    fn a035_non_recursive_accepted() {
        let source = "fn a() -> i32 { return b(); } fn b() -> i32 { return 0; }";
        assert!(
            !has_recursion(source),
            "non-recursive call chain must not trip A035"
        );
    }

    #[test]
    fn a035_recursion_nested_in_if_detected() {
        let source = r#"
            fn f(n: i32) -> i32 {
                if n > 0 {
                    let r: i32 = f(n);
                    return r;
                }
                return 0;
            }
        "#;
        assert!(
            has_recursion(source),
            "recursive call nested inside an if-block must be detected"
        );
    }

    #[test]
    fn a035_three_cycle_detected() {
        let source = r#"
            fn a() -> i32 { return b(); }
            fn b() -> i32 { return c(); }
            fn c() -> i32 { return a(); }
        "#;
        assert!(
            has_recursion(source),
            "expected RecursionDetected for the 3-cycle a -> b -> c -> a"
        );
    }

    #[test]
    fn a035_method_self_recursion_detected() {
        // End-to-end coverage of method-call resolution through real source:
        // `self.rec()` must resolve to the canonical key `S.rec` and form a cycle.
        let source = r#"
            struct S {
                v: i32;
                fn rec(self) -> i32 { return self.rec(); }
            }
            pub fn entry() -> i32 { let s: S = S { v: 1 }; return s.rec(); }
        "#;
        let diag = recursion_diag(source);
        assert!(
            diag.to_string().contains("S.rec -> S.rec"),
            "diagnostic should name the method cycle `S.rec -> S.rec`, got: {diag}"
        );
    }

    #[test]
    fn a035_recursion_inside_nondet_block_detected() {
        // A recursive call buried in a `forall` block body must still be caught;
        // the walker descends into non-deterministic blocks like any other block.
        let source = r#"
            fn r(n: i32) -> i32 { return r(n); }
            pub fn entry() -> i32 {
                forall {
                    let x: i32 = r(0);
                }
                return 0;
            }
        "#;
        assert!(
            has_recursion(source),
            "recursive call inside a forall block must be detected"
        );
    }
}
