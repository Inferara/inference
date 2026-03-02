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
    use inference_analysis::errors::{AnalysisError, AnalysisErrors};
    use inference_type_checker::typed_context::TypedContext;

    fn type_check(source: &str) -> TypedContext {
        let arena = build_ast(source.to_string());
        inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .expect("type checking should succeed for analysis test input")
            .typed_context()
    }

    fn analyze(source: &str) -> Result<(), AnalysisErrors> {
        let ctx = type_check(source);
        inference_analysis::analyze(&ctx)
    }

    fn expect_errors(source: &str) -> Vec<AnalysisError> {
        analyze(source)
            .expect_err("expected analysis errors but got Ok(())")
            .errors
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
            matches!(&errors[0], AnalysisError::BreakOutsideLoop { .. }),
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
            .any(|e| matches!(e, AnalysisError::ReturnInsideLoop { .. }));
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
            .any(|e| matches!(e, AnalysisError::InfiniteLoopWithoutBreak { .. }));
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
            matches!(&errors[0], AnalysisError::BreakOutsideLoop { .. }),
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
            .any(|e| matches!(e, AnalysisError::ReturnInsideLoop { .. }));
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
            .any(|e| matches!(e, AnalysisError::InfiniteLoopWithoutBreak { .. }));
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
            matches!(&errors[0], AnalysisError::BreakOutsideLoop { .. }),
            "expected BreakOutsideLoop in spec-nested struct, got: {:?}",
            errors[0]
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
            .any(|e| matches!(e, AnalysisError::BreakOutsideLoop { .. }));
        let has_infinite_loop = errors
            .iter()
            .any(|e| matches!(e, AnalysisError::InfiniteLoopWithoutBreak { .. }));
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
            .filter(|e| matches!(e, AnalysisError::BreakOutsideLoop { .. }))
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
            .filter(|e| matches!(e, AnalysisError::BreakOutsideLoop { .. }))
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
            .any(|e| matches!(e, AnalysisError::BreakInsideNonDetBlock { .. }));
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
            .any(|e| matches!(e, AnalysisError::BreakInsideNonDetBlock { .. }));
        assert!(
            has_nondet_break,
            "expected BreakInsideNonDetBlock among errors: {errors:?}"
        );
    }
}
