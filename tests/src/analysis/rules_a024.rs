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

    /// The message states the real condition — no `use … from` directive binds
    /// the declaration — and hands back the binding to write.
    ///
    /// The wording it replaced said external functions "cannot be compiled to
    /// WebAssembly yet" and named codegen, both false since bound externals are
    /// compiled to imports and merged, and a call inside a `spec` reaches this
    /// rule without codegen being involved at all.
    #[test]
    fn a024_message_names_the_missing_binding_and_the_remedy() {
        let source = r#"
            external fn print(val: i32) -> ();
            fn main() { print(42); }
        "#;
        let errors = expect_errors(source);
        let rendered = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::ExternFunctionCall { .. }))
            .expect("A024 must fire on a call to an unbound extern")
            .to_string();
        assert!(
            rendered.contains("no `use ... from` directive binds"),
            "the message must state the missing binding; got: {rendered}"
        );
        assert!(
            rendered.contains("use { print } from <module>;"),
            "the message must show the binding to add, naming the function; got: {rendered}"
        );
        assert!(
            rendered.contains("declared inside a `spec` must be moved out"),
            "the message must cover the spec-inner case, which no `use` can reach; \
             got: {rendered}"
        );
        assert!(
            !rendered.contains("codegen") && !rendered.contains("WebAssembly"),
            "the message must not blame code generation; got: {rendered}"
        );
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

    #[test]
    fn a024_call_to_bound_extern_accepted() {
        // An extern bound to a source module via `use … from` lowers to a
        // linker-satisfied import (issue #9, Phase 4), so calling it must NOT
        // trigger A024 — only unbound bare externs remain uncompilable.
        let source = r#"
            external fn sum(a: i32, b: i32) -> i32;
            use { sum } from arith;
            fn main() -> i32 { return sum(1, 2); }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a024 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::ExternFunctionCall { .. }));
            assert!(
                !has_a024,
                "a bound extern call must not trigger A024, got: {:?}",
                e.errors()
            );
        }
    }

    #[test]
    fn a024_unbound_extern_rejected_when_other_extern_is_bound() {
        // With one bound extern and one unbound bare extern, only the call to
        // the unbound one is rejected.
        let source = r#"
            external fn sum(a: i32, b: i32) -> i32;
            use { sum } from arith;
            external fn raw(x: i32) -> i32;
            fn main() -> i32 { return sum(1, 2) + raw(3); }
        "#;
        let errors = expect_errors(source);
        let offending: Vec<&str> = errors
            .iter()
            .filter_map(|e| match e {
                AnalysisDiagnostic::ExternFunctionCall { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            offending,
            vec!["raw"],
            "only the unbound extern `raw` should be rejected, got: {errors:?}"
        );
    }

    #[test]
    fn a024_top_level_use_does_not_bind_spec_inner_extern() {
        // H8: a top-level `use { sort } from sorting;` is file-wide but binds
        // only top-level externs. With no top-level `sort` declared, the `use`
        // names an undeclared top-level extern, so the type checker reports
        // ExternImportNotDeclared rather than silently binding the spec-inner
        // `sort` (which previously suppressed A024 and crashed proof-mode
        // codegen).
        let source = r#"
            use { sort } from sorting;
            spec Ms {
                external fn sort(a: i32) -> i32;
                fn run(x: i32) -> i32 { return sort(x); }
            }
        "#;
        let arena = build_ast(source.to_string());
        let rendered = match inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
        {
            Ok(_) => panic!("a top-level use of an undeclared top-level extern must be rejected"),
            Err(err) => format!("{err:#}"),
        };
        assert!(
            rendered.contains("sort") && rendered.contains("no `external fn"),
            "expected ExternImportNotDeclared for `sort`, got: {rendered}"
        );
    }

    #[test]
    fn a024_spec_inner_extern_unbound_despite_same_named_bound_top_level() {
        // H9/H10: a bound top-level `external fn sort` and a same-named, distinct
        // spec-inner `external fn sort`. The `use` binds only the top-level
        // declaration (resolution is by DefId, not name), so the spec-inner
        // `sort` stays unbound and its call is A024-rejected — preventing the
        // proof-mode miscompile where the spec body would call the merged
        // top-level `sort` with a mismatched signature.
        let source = r#"
            external fn sort(a: i32) -> i32;
            use { sort } from sorting;
            spec Ms {
                external fn sort(a: i64, b: i64) -> i64;
                fn run(x: i64, y: i64) -> i64 { return sort(x, y); }
            }
        "#;
        let errors = expect_errors(source);
        let has_sort_rejection = errors.iter().any(|e| {
            matches!(e, AnalysisDiagnostic::ExternFunctionCall { name, .. } if name == "sort")
        });
        assert!(
            has_sort_rejection,
            "the unbound spec-inner `sort` must be A024-rejected even though a same-named top-level extern is bound, got: {errors:?}"
        );
    }

    #[test]
    fn a024_bound_top_level_extern_call_not_flagged_when_unbound_spec_inner_shadows_it() {
        // H1 (round-2 regression): a bound top-level `external fn sort` (via
        // `use … from`) called from a top-level function MUST NOT be flagged
        // just because a same-named, distinct, unbound spec-inner
        // `external fn sort` exists. Resolution is scope-aware: the top-level
        // call binds to the bound top-level declaration, the spec-inner call
        // binds to the unbound spec-inner one. Only the latter is A024-rejected.
        // A name-keyed check let the unbound spec-inner declaration poison the
        // valid top-level call site (the round-2 H-1 false positive).
        let source = r#"
            external fn sort(a: i32) -> i32;
            use { sort } from sorting;
            fn main() -> i32 { return sort(7); }
            spec Ms {
                external fn sort(a: i64, b: i64) -> i64;
                fn run(x: i64, y: i64) -> i64 { return sort(x, y); }
            }
        "#;
        let errors = expect_errors(source);
        let sort_rejections = errors
            .iter()
            .filter(
                |e| matches!(e, AnalysisDiagnostic::ExternFunctionCall { name, .. } if name == "sort"),
            )
            .count();
        assert_eq!(
            sort_rejections, 1,
            "exactly the unbound spec-inner `sort` call must be A024-rejected; the bound \
             top-level `sort(7)` call must NOT be flagged, got: {errors:?}"
        );
    }

    #[test]
    fn a024_bound_top_level_extern_call_accepted_despite_unbound_spec_inner_same_name() {
        // H1 (round-2 regression), positive form: with ONLY the bound top-level
        // `sort` called (the spec-inner `sort` is declared but never called),
        // analysis must succeed — the uncalled unbound spec-inner declaration
        // must not poison the valid top-level call.
        let source = r#"
            external fn sort(a: i32) -> i32;
            use { sort } from sorting;
            fn main() -> i32 { return sort(7); }
            spec Ms {
                external fn sort(a: i64, b: i64) -> i64;
                fn pure_run(x: i64) -> i64 { return x; }
            }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a024 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::ExternFunctionCall { .. }));
            assert!(
                !has_a024,
                "a bound top-level extern call must compile even when a same-named unbound \
                 spec-inner extern is declared but uncalled, got: {:?}",
                e.errors()
            );
        }
    }

    #[test]
    fn a024_extern_function_call_in_const_array_inside_function() {
        let source = r#"
            external fn ext_func() -> i32;
            fn main() {
                const X: [i32; 2] = [1, ext_func()];
            }
        "#;
        let errors = expect_errors(source);
        let has_a024 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ExternFunctionCall { .. }));
        assert!(
            has_a024,
            "extern function call in const array initializer should trigger A024, got: {errors:?}"
        );
    }
}
