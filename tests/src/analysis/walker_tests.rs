/// Integration tests for analysis walker traversal into struct methods,
/// spec definitions, and module definitions.
///
/// These tests verify that `inference_analysis::analyze()` correctly recurses
/// into nested definition scopes via `for_each_function_body`. Before the
/// walker fix, struct methods, spec functions, and module functions were
/// silently skipped.
#[cfg(test)]
mod walker_traversal_tests {
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

    // --- Positive tests: errors expected ---

    #[test]
    fn a001_break_outside_loop_in_struct_method() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct Foo {
                fn bar() {
                    break;
                }
            }
        "#;
        let errors = expect_errors(source);
        assert_eq!(errors.len(), 1);
        assert!(
            matches!(&errors[0], AnalysisDiagnostic::BreakOutsideLoop { .. }),
            "expected BreakOutsideLoop, got: {:?}",
            errors[0]
        );
    }

    #[test]
    fn a003_return_inside_loop_in_struct_method() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct Foo {
                fn bar() -> i32 {
                    loop {
                        return 42;
                    }
                }
            }
        "#;
        let errors = expect_errors(source);

        let has_return_inside_loop = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ReturnInsideLoop { .. }));
        assert!(
            has_return_inside_loop,
            "expected ReturnInsideLoop among errors: {errors:?}"
        );
    }

    #[test]
    fn a004_infinite_loop_without_break_in_struct_method() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct Foo {
                fn bar() {
                    loop {
                    }
                }
            }
        "#;
        let errors = expect_errors(source);

        let has_infinite_loop = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::InfiniteLoopWithoutBreak { .. }));
        assert!(
            has_infinite_loop,
            "expected InfiniteLoopWithoutBreak among errors: {errors:?}"
        );
    }

    #[test]
    fn a001_break_outside_loop_in_spec_function() {
        let source = r#"
            fn main() -> i32 { return 0; }
            spec MySpec {
                fn check() {
                    break;
                }
            }
        "#;
        let errors = expect_errors(source);
        assert_eq!(errors.len(), 1);
        assert!(
            matches!(&errors[0], AnalysisDiagnostic::BreakOutsideLoop { .. }),
            "expected BreakOutsideLoop, got: {:?}",
            errors[0]
        );
    }

    #[test]
    fn a003_return_inside_loop_in_spec_function() {
        let source = r#"
            fn main() -> i32 { return 0; }
            spec MySpec {
                fn check() -> i32 {
                    loop {
                        return 42;
                    }
                }
            }
        "#;
        let errors = expect_errors(source);

        let has_return_inside_loop = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ReturnInsideLoop { .. }));
        assert!(
            has_return_inside_loop,
            "expected ReturnInsideLoop among errors: {errors:?}"
        );
    }

    #[test]
    fn a004_infinite_loop_without_break_in_spec_function() {
        let source = r#"
            fn main() -> i32 { return 0; }
            spec MySpec {
                fn check() {
                    loop {
                    }
                }
            }
        "#;
        let errors = expect_errors(source);

        let has_infinite_loop = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::InfiniteLoopWithoutBreak { .. }));
        assert!(
            has_infinite_loop,
            "expected InfiniteLoopWithoutBreak among errors: {errors:?}"
        );
    }

    #[test]
    fn a001_break_outside_loop_in_spec_nested_struct() {
        let source = r#"
            fn main() -> i32 { return 0; }
            spec MySpec {
                struct Inner {
                    fn method() {
                        break;
                    }
                }
            }
        "#;
        let errors = expect_errors(source);
        assert_eq!(errors.len(), 1);
        assert!(
            matches!(&errors[0], AnalysisDiagnostic::BreakOutsideLoop { .. }),
            "expected BreakOutsideLoop in spec-nested struct, got: {:?}",
            errors[0]
        );
    }

    #[test]
    fn a001_break_outside_loop_in_module_function() {
        // FIXME: module definitions are not yet supported by the grammar.
        // Once supported, this test should verify that analysis detects BreakOutsideLoop
        // inside module functions. Currently the parser rejects `mod` blocks.
        let source = r#"
            fn main() -> i32 { return 0; }
            mod utils {
                fn helper() {
                    break;
                }
            }
        "#;
        let result = crate::utils::try_build_ast(source.to_string());
        assert!(
            result.is_err(),
            "expected parse error for module definition, but parsing succeeded"
        );
    }

    #[test]
    fn multiple_violations_across_struct_methods() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct Foo {
                fn method_a() {
                    break;
                }
                fn method_b() {
                    loop {
                    }
                }
            }
        "#;
        let errors = expect_errors(source);

        let has_break_outside = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::BreakOutsideLoop { .. }));
        let has_infinite_loop = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::InfiniteLoopWithoutBreak { .. }));
        assert!(
            has_break_outside,
            "expected BreakOutsideLoop among errors: {errors:?}"
        );
        assert!(
            has_infinite_loop,
            "expected InfiniteLoopWithoutBreak among errors: {errors:?}"
        );
    }

    #[test]
    fn violations_in_both_free_function_and_struct_method() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn free_func() {
                break;
            }
            struct Foo {
                fn method() {
                    break;
                }
            }
        "#;
        let errors = expect_errors(source);

        let break_count = errors
            .iter()
            .filter(|e| matches!(e, AnalysisDiagnostic::BreakOutsideLoop { .. }))
            .count();
        assert_eq!(
            break_count, 2,
            "expected 2 BreakOutsideLoop errors (free fn + struct method), got {break_count}"
        );
    }

    #[test]
    fn violations_in_both_free_function_and_spec_function() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn free_func() {
                break;
            }
            spec MySpec {
                fn check() {
                    break;
                }
            }
        "#;
        let errors = expect_errors(source);

        let break_count = errors
            .iter()
            .filter(|e| matches!(e, AnalysisDiagnostic::BreakOutsideLoop { .. }))
            .count();
        assert_eq!(
            break_count, 2,
            "expected 2 BreakOutsideLoop errors (free fn + spec fn), got {break_count}"
        );
    }

    // --- Negative tests: no errors expected ---

    #[test]
    fn valid_struct_method_with_loop_and_break() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct Foo {
                fn bar() {
                    loop {
                        break;
                    }
                }
            }
        "#;
        let result = analyze(source);
        assert!(
            result.is_ok(),
            "expected no analysis errors, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn valid_spec_function_with_loop_and_break() {
        let source = r#"
            fn main() -> i32 { return 0; }
            spec MySpec {
                fn check() {
                    loop {
                        break;
                    }
                }
            }
        "#;
        let result = analyze(source);
        assert!(
            result.is_ok(),
            "expected no analysis errors, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn valid_free_function_no_errors() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() -> i32 {
                let x: i32 = 1;
                return x;
            }
        "#;
        let result = analyze(source);
        assert!(
            result.is_ok(),
            "expected no analysis errors for valid free function, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn empty_struct_no_errors() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct Empty {
                x: i32;
            }
        "#;
        let result = analyze(source);
        assert!(
            result.is_ok(),
            "expected no analysis errors for struct with no methods, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn empty_spec_no_errors() {
        let source = r#"
            fn main() -> i32 { return 0; }
            spec EmptySpec {}
        "#;
        let result = analyze(source);
        assert!(
            result.is_ok(),
            "expected no analysis errors for empty spec, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn spec_with_external_fn_no_errors() {
        let source = r#"
            fn main() -> i32 { return 0; }
            spec MySpec {
                external fn sorting_function(a: i32, b: i32) -> i32;
            }
        "#;
        let result = analyze(source);
        assert!(
            result.is_ok(),
            "expected no analysis errors for spec with external fn, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn a002_break_inside_nondet_block_in_struct_method() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct Foo {
                fn bar() {
                    loop {
                        forall {
                            break;
                        }
                    }
                }
            }
        "#;
        let errors = expect_errors(source);

        let has_nondet_break = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::BreakInsideNonDetBlock { .. }));
        assert!(
            has_nondet_break,
            "expected BreakInsideNonDetBlock among errors: {errors:?}"
        );
    }

    #[test]
    fn a002_break_inside_nondet_block_in_spec_function() {
        let source = r#"
            fn main() -> i32 { return 0; }
            spec MySpec {
                fn check() {
                    loop {
                        forall {
                            break;
                        }
                    }
                }
            }
        "#;
        let errors = expect_errors(source);

        let has_nondet_break = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::BreakInsideNonDetBlock { .. }));
        assert!(
            has_nondet_break,
            "expected BreakInsideNonDetBlock among errors: {errors:?}"
        );
    }

    #[test]
    fn a002_break_inside_assume_block() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() {
                loop {
                    assume {
                        break;
                    }
                    break;
                }
            }
        "#;
        let errors = expect_errors(source);
        let has_nondet_break = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::BreakInsideNonDetBlock { .. }));
        assert!(
            has_nondet_break,
            "expected BreakInsideNonDetBlock in assume block: {errors:?}"
        );
    }

    #[test]
    fn a002_break_inside_unique_block() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() {
                loop {
                    unique {
                        break;
                    }
                    break;
                }
            }
        "#;
        let errors = expect_errors(source);
        let has_nondet_break = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::BreakInsideNonDetBlock { .. }));
        assert!(
            has_nondet_break,
            "expected BreakInsideNonDetBlock in unique block: {errors:?}"
        );
    }

    #[test]
    fn a002_break_inside_exists_block() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() {
                loop {
                    exists {
                        break;
                    }
                    break;
                }
            }
        "#;
        let errors = expect_errors(source);
        let has_nondet_break = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::BreakInsideNonDetBlock { .. }));
        assert!(
            has_nondet_break,
            "expected BreakInsideNonDetBlock in exists block: {errors:?}"
        );
    }

    #[test]
    fn a005_return_inside_assume_block() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() -> i32 {
                assume {
                    return 0;
                }
                return 1;
            }
        "#;
        let errors = expect_errors(source);
        let has_return_nondet = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ReturnInsideNonDetBlock { .. }));
        assert!(
            has_return_nondet,
            "expected ReturnInsideNonDetBlock in assume block: {errors:?}"
        );
    }

    #[test]
    fn a005_return_inside_unique_block() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() -> i32 {
                unique {
                    return 0;
                }
                return 1;
            }
        "#;
        let errors = expect_errors(source);
        let has_return_nondet = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ReturnInsideNonDetBlock { .. }));
        assert!(
            has_return_nondet,
            "expected ReturnInsideNonDetBlock in unique block: {errors:?}"
        );
    }

    #[test]
    fn a004_infinite_loop_break_inside_assume() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() {
                loop {
                    assume {
                        break;
                    }
                }
            }
        "#;
        let errors = expect_errors(source);
        let has_infinite_loop = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::InfiniteLoopWithoutBreak { .. }));
        assert!(
            has_infinite_loop,
            "expected InfiniteLoopWithoutBreak (break in assume doesn't count): {errors:?}"
        );
    }

    #[test]
    fn a004_infinite_loop_break_inside_exists() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() {
                loop {
                    exists {
                        break;
                    }
                }
            }
        "#;
        let errors = expect_errors(source);
        let has_infinite_loop = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::InfiniteLoopWithoutBreak { .. }));
        assert!(
            has_infinite_loop,
            "expected InfiniteLoopWithoutBreak (break in exists doesn't count): {errors:?}"
        );
    }

    #[test]
    fn a004_infinite_loop_break_inside_unique() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() {
                loop {
                    unique {
                        break;
                    }
                }
            }
        "#;
        let errors = expect_errors(source);
        let has_infinite_loop = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::InfiniteLoopWithoutBreak { .. }));
        assert!(
            has_infinite_loop,
            "expected InfiniteLoopWithoutBreak (break in unique doesn't count): {errors:?}"
        );
    }

    // --- Edge case tests ---

    #[test]
    fn a004_nested_loops_outer_has_no_break() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() {
                loop {
                    loop {
                        break;
                    }
                }
            }
        "#;
        let errors = expect_errors(source);

        let has_infinite_loop = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::InfiniteLoopWithoutBreak { .. }));
        assert!(
            has_infinite_loop,
            "expected InfiniteLoopWithoutBreak for outer loop: {errors:?}"
        );
    }

    #[test]
    fn a002_deeply_nested_nondet() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() {
                forall {
                    let mut i: i32 = 0;
                    loop i < 10 {
                        exists {
                            break;
                        }
                        i = i + 1;
                    }
                }
            }
        "#;
        let errors = expect_errors(source);

        let has_nondet_break = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::BreakInsideNonDetBlock { .. }));
        assert!(
            has_nondet_break,
            "expected BreakInsideNonDetBlock with deeply nested nondet: {errors:?}"
        );
    }

    #[test]
    fn a002_break_in_if_inside_nondet_in_loop() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo(x: bool) {
                let mut i: i32 = 0;
                loop i < 10 {
                    forall {
                        if x {
                            break;
                        }
                    }
                    i = i + 1;
                }
            }
        "#;
        let errors = expect_errors(source);

        let has_nondet_break = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::BreakInsideNonDetBlock { .. }));
        assert!(
            has_nondet_break,
            "expected BreakInsideNonDetBlock for break in if inside nondet: {errors:?}"
        );
    }

    #[test]
    fn a001_multiple_breaks_outside_loop() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() {
                break;
                break;
            }
        "#;
        let errors = expect_errors(source);

        let break_count = errors
            .iter()
            .filter(|e| matches!(e, AnalysisDiagnostic::BreakOutsideLoop { .. }))
            .count();
        assert_eq!(
            break_count, 2,
            "expected exactly 2 BreakOutsideLoop errors, got {break_count}"
        );
    }

    #[test]
    fn a003_return_inside_conditional_loop() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo(x: i32) -> i32 {
                let mut i: i32 = 0;
                loop i < x {
                    return 0;
                }
                return 1;
            }
        "#;
        let errors = expect_errors(source);

        let has_return_inside_loop = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ReturnInsideLoop { .. }));
        assert!(
            has_return_inside_loop,
            "expected ReturnInsideLoop for conditional loop: {errors:?}"
        );
    }

    #[test]
    fn a004_infinite_loop_break_only_in_nondet() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() {
                loop {
                    forall {
                        break;
                    }
                }
            }
        "#;
        let errors = expect_errors(source);

        let has_infinite_loop = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::InfiniteLoopWithoutBreak { .. }));
        assert!(
            has_infinite_loop,
            "expected InfiniteLoopWithoutBreak (break inside nondet doesn't count): {errors:?}"
        );

        let has_nondet_break = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::BreakInsideNonDetBlock { .. }));
        assert!(
            has_nondet_break,
            "expected BreakInsideNonDetBlock as well: {errors:?}"
        );
    }

    #[test]
    fn valid_break_in_conditional_loop() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() {
                let mut i: i32 = 0;
                loop i < 10 {
                    break;
                }
            }
        "#;
        let result = analyze(source);
        assert!(
            result.is_ok(),
            "expected no analysis errors for break in conditional loop, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn valid_break_in_if_inside_infinite_loop() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo(x: bool) {
                loop {
                    if x {
                        break;
                    }
                }
            }
        "#;
        let result = analyze(source);
        assert!(
            result.is_ok(),
            "expected no analysis errors for break in if inside infinite loop, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn a005_return_inside_nondet_block() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() -> i32 {
                forall {
                    return 0;
                }
                return 1;
            }
        "#;
        let errors = expect_errors(source);

        let has_return_nondet = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ReturnInsideNonDetBlock { .. }));
        assert!(
            has_return_nondet,
            "expected ReturnInsideNonDetBlock among errors: {errors:?}"
        );
    }

    #[test]
    fn a005_return_inside_exists_block() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() -> i32 {
                exists {
                    return 0;
                }
                return 1;
            }
        "#;
        let errors = expect_errors(source);

        let has_return_nondet = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ReturnInsideNonDetBlock { .. }));
        assert!(
            has_return_nondet,
            "expected ReturnInsideNonDetBlock in exists block: {errors:?}"
        );
    }

    #[test]
    fn valid_return_outside_nondet_block() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() -> i32 {
                return 42;
            }
        "#;
        let result = analyze(source);
        assert!(
            result.is_ok(),
            "expected no analysis errors for return outside nondet block, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn a001_break_outside_loop_inside_spec_nested_function() {
        let source = r#"
            fn main() -> i32 { return 0; }
            spec Utils {
                fn helper() {
                    break;
                }
            }
        "#;
        let errors = expect_errors(source);
        assert_eq!(errors.len(), 1);
        assert!(
            matches!(&errors[0], AnalysisDiagnostic::BreakOutsideLoop { .. }),
            "expected BreakOutsideLoop inside spec, got: {:?}",
            errors[0]
        );
    }

    #[test]
    fn valid_assert_inside_loop() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo(x: i32) {
                loop {
                    assert x > 0;
                    break;
                }
            }
        "#;
        let result = analyze(source);
        assert!(
            result.is_ok(),
            "expected no analysis errors for assert inside loop with break, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn break_in_nondet_outside_loop_fires_both() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() {
                forall {
                    break;
                }
            }
        "#;
        let errors = expect_errors(source);

        let break_outside_count = errors
            .iter()
            .filter(|e| matches!(e, AnalysisDiagnostic::BreakOutsideLoop { .. }))
            .count();
        assert_eq!(
            break_outside_count, 1,
            "expected exactly 1 BreakOutsideLoop, got {break_outside_count}"
        );

        let nondet_break_count = errors
            .iter()
            .filter(|e| matches!(e, AnalysisDiagnostic::BreakInsideNonDetBlock { .. }))
            .count();
        assert_eq!(
            nondet_break_count, 1,
            "expected 1 BreakInsideNonDetBlock, got {nondet_break_count}"
        );
    }

    #[test]
    fn a003_and_a004_return_inside_infinite_loop() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() -> i32 {
                loop {
                    return 0;
                }
            }
        "#;
        let errors = expect_errors(source);

        let has_return_inside_loop = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ReturnInsideLoop { .. }));
        assert!(
            has_return_inside_loop,
            "expected ReturnInsideLoop: {errors:?}"
        );

        let has_infinite_loop = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::InfiniteLoopWithoutBreak { .. }));
        assert!(
            has_infinite_loop,
            "expected InfiniteLoopWithoutBreak: {errors:?}"
        );
    }

    #[test]
    fn a003_and_a005_return_inside_loop_and_nondet() {
        let source = r#"
            fn main() -> i32 { return 0; }
            fn foo() -> i32 {
                loop {
                    forall {
                        return 0;
                    }
                    break;
                }
                return 1;
            }
        "#;
        let errors = expect_errors(source);

        let has_return_inside_loop = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ReturnInsideLoop { .. }));
        assert!(
            has_return_inside_loop,
            "expected ReturnInsideLoop: {errors:?}"
        );

        let has_return_nondet = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::ReturnInsideNonDetBlock { .. }));
        assert!(
            has_return_nondet,
            "expected ReturnInsideNonDetBlock: {errors:?}"
        );
    }
}
