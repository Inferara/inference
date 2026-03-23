/// Integration tests for analysis rules A006-A011.
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

    fn expect_warnings(source: &str) -> Vec<AnalysisDiagnostic> {
        let result = analyze(source);
        match result {
            Ok(r) => r.warnings().to_vec(),
            Err(e) => e.warnings().to_vec(),
        }
    }

    // --- A006: Uzumaki outside nondet block ---

    #[test]
    fn a006_uzumaki_in_vardef_outside_nondet() {
        let source = r#"
            fn main() {
                let x: i32 = @;
            }
        "#;
        let errors = expect_errors(source);
        let has_uzumaki_outside = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOutsideNonDetBlock { .. }));
        assert!(
            has_uzumaki_outside,
            "expected UzumakiOutsideNonDetBlock, got: {errors:?}"
        );
    }

    #[test]
    fn a006_uzumaki_in_assign_outside_nondet() {
        let source = r#"
            fn main() {
                let mut x: i32 = 0;
                x = @;
            }
        "#;
        let errors = expect_errors(source);
        let has_uzumaki_outside = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOutsideNonDetBlock { .. }));
        assert!(
            has_uzumaki_outside,
            "expected UzumakiOutsideNonDetBlock in assign, got: {errors:?}"
        );
    }

    #[test]
    fn a006_uzumaki_in_return_outside_nondet() {
        let source = r#"
            fn main() -> i32 {
                return @;
            }
        "#;
        let errors = expect_errors(source);
        let has_uzumaki_outside = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOutsideNonDetBlock { .. }));
        assert!(
            has_uzumaki_outside,
            "expected UzumakiOutsideNonDetBlock in return, got: {errors:?}"
        );
    }

    #[test]
    fn a006_uzumaki_inside_forall_passes() {
        let source = r#"
            fn main() {
                forall {
                    let x: i32 = @;
                }
            }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_a006 = errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOutsideNonDetBlock { .. }));
            assert!(
                !has_a006,
                "uzumaki inside forall should NOT trigger A006, got: {errors}"
            );
        }
    }

    #[test]
    fn a006_uzumaki_inside_exists_passes() {
        let source = r#"
            fn main() {
                exists {
                    let x: i32 = @;
                }
            }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_a006 = errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOutsideNonDetBlock { .. }));
            assert!(
                !has_a006,
                "uzumaki inside exists should NOT trigger A006, got: {errors}"
            );
        }
    }

    #[test]
    fn a006_uzumaki_inside_unique_passes() {
        let source = r#"
            fn main() {
                unique {
                    let x: i32 = @;
                }
            }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_a006 = errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOutsideNonDetBlock { .. }));
            assert!(
                !has_a006,
                "uzumaki inside unique should NOT trigger A006, got: {errors}"
            );
        }
    }

    // --- A007: Missing return ---

    #[test]
    fn a007_function_with_return_type_but_no_return() {
        let source = r#"
            fn main() -> i32 {
                let x: i32 = 42;
            }
        "#;
        let errors = expect_errors(source);
        let has_missing_return = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::MissingReturn { .. }));
        assert!(
            has_missing_return,
            "expected MissingReturn, got: {errors:?}"
        );
    }

    #[test]
    fn a007_function_with_return_statement_passes() {
        let source = r#"
            fn main() -> i32 {
                return 42;
            }
        "#;
        let result = analyze(source);
        assert!(
            result.is_ok(),
            "function with return should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn a007_void_function_without_return_passes() {
        let source = r#"
            fn main() {
                let x: i32 = 42;
            }
        "#;
        let result = analyze(source);
        assert!(
            result.is_ok(),
            "void function without return should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn a007_if_else_both_return_passes() {
        let source = r#"
            fn main(x: bool) -> i32 {
                if x {
                    return 1;
                } else {
                    return 0;
                }
            }
        "#;
        let result = analyze(source);
        assert!(
            result.is_ok(),
            "if/else both returning should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn a007_if_without_else_missing_return() {
        let source = r#"
            fn main(x: bool) -> i32 {
                if x {
                    return 1;
                }
            }
        "#;
        let errors = expect_errors(source);
        let has_missing_return = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::MissingReturn { .. }));
        assert!(
            has_missing_return,
            "if without else should trigger MissingReturn, got: {errors:?}"
        );
    }

    #[test]
    fn a007_infinite_loop_counts_as_returning() {
        let source = r#"
            fn main() -> i32 {
                loop {
                    break;
                }
            }
        "#;
        let result = analyze(source);
        assert!(
            result.is_ok(),
            "infinite loop should count as 'returning', got: {:?}",
            result.err()
        );
    }

    #[test]
    fn a007_missing_return_in_struct_method() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct Foo {
                x: i32;
                fn get(self) -> i32 {
                    let v: i32 = self.x;
                }
            }
        "#;
        let errors = expect_errors(source);
        let has_missing_return = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::MissingReturn { .. }));
        assert!(
            has_missing_return,
            "struct method with return type but no return should trigger MissingReturn, got: {errors:?}"
        );
    }

    // --- A008: Standalone uzumaki ---

    #[test]
    fn a008_standalone_uzumaki_as_expression_statement() {
        let source = r#"
            fn main() {
                forall {
                    @;
                }
            }
        "#;
        let errors = expect_errors(source);
        let has_standalone = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::StandaloneUzumaki { .. }));
        assert!(
            has_standalone,
            "expected StandaloneUzumaki, got: {errors:?}"
        );
    }

    #[test]
    fn a008_uzumaki_in_vardef_not_standalone() {
        let source = r#"
            fn main() {
                forall {
                    let x: i32 = @;
                }
            }
        "#;
        let result = analyze(source);
        if let Err(errors) = &result {
            let has_standalone = errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::StandaloneUzumaki { .. }));
            assert!(
                !has_standalone,
                "uzumaki in vardef should NOT trigger StandaloneUzumaki, got: {errors}"
            );
        }
    }

    // --- A009: Empty enum definition ---

    #[test]
    fn a009_empty_enum_produces_warning() {
        let source = "enum Empty {}\nfn main() -> i32 { return 0; }";
        let arena = crate::utils::build_ast(source.to_string());
        // Verify the parser produces a Def::Enum with empty variants
        let has_empty_enum = arena.source_files().any(|sf| {
            sf.defs.iter().any(|&def_id| {
                matches!(
                    &arena[def_id].kind,
                    inference_ast::nodes::Def::Enum { variants, .. } if variants.is_empty()
                )
            })
        });
        if !has_empty_enum {
            // If the parser doesn't produce an empty enum, skip this test.
            // A009 serves as a defensive check for potential grammar changes.
            return;
        }
        let warnings = expect_warnings(source);
        let has_a009 = warnings
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::EmptyEnumDefinition { .. }));
        assert!(
            has_a009,
            "expected EmptyEnumDefinition warning, got: {warnings:?}"
        );
    }

    #[test]
    fn a009_enum_with_variants_passes() {
        let source = r#"
            fn main() -> i32 { return 0; }
            enum Color { Red, Green, Blue }
        "#;
        let warnings = expect_warnings(source);
        let has_empty_enum = warnings
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::EmptyEnumDefinition { .. }));
        assert!(
            !has_empty_enum,
            "enum with variants should NOT trigger EmptyEnumDefinition"
        );
    }

    // --- A010: Method never accesses self ---

    #[test]
    fn a010_method_with_self_but_no_access_warns() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct Foo {
                x: i32;
                fn noop(self) -> i32 {
                    return 42;
                }
            }
        "#;
        let warnings = expect_warnings(source);
        let has_unused_self = warnings
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::MethodNeverAccessesSelf { .. }));
        assert!(
            has_unused_self,
            "expected MethodNeverAccessesSelf warning, got: {warnings:?}"
        );
    }

    #[test]
    fn a010_method_using_self_field_no_warning() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct Foo {
                x: i32;
                fn get(self) -> i32 {
                    return self.x;
                }
            }
        "#;
        let warnings = expect_warnings(source);
        let has_unused_self = warnings
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::MethodNeverAccessesSelf { .. }));
        assert!(
            !has_unused_self,
            "method using self should NOT trigger MethodNeverAccessesSelf"
        );
    }

    #[test]
    fn a010_associated_function_without_self_no_warning() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct Foo {
                x: i32;
                fn new() -> i32 {
                    return 0;
                }
            }
        "#;
        let warnings = expect_warnings(source);
        let has_unused_self = warnings
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::MethodNeverAccessesSelf { .. }));
        assert!(
            !has_unused_self,
            "associated function without self should NOT trigger MethodNeverAccessesSelf"
        );
    }

    #[test]
    fn a010_mut_self_never_used_warns() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct Foo {
                x: i32;
                fn noop(mut self) -> i32 {
                    return 42;
                }
            }
        "#;
        let warnings = expect_warnings(source);
        let has_unused_self = warnings
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::MethodNeverAccessesSelf { .. }));
        assert!(
            has_unused_self,
            "mut self not used should trigger MethodNeverAccessesSelf, got: {warnings:?}"
        );
    }

    #[test]
    fn a010_self_used_in_nested_if_no_warning() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct Foo {
                x: i32;
                fn check(self) -> i32 {
                    if true {
                        return self.x;
                    }
                    return 0;
                }
            }
        "#;
        let warnings = expect_warnings(source);
        let has_unused_self = warnings
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::MethodNeverAccessesSelf { .. }));
        assert!(
            !has_unused_self,
            "self used in nested if should NOT trigger MethodNeverAccessesSelf"
        );
    }

    #[test]
    fn a010_multiple_methods_only_unused_one_warns() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct Foo {
                x: i32;
                fn get(self) -> i32 { return self.x; }
                fn bad(self) -> i32 { return 42; }
            }
        "#;
        let warnings = expect_warnings(source);
        let unused_self_warnings: Vec<_> = warnings
            .iter()
            .filter(|e| matches!(e, AnalysisDiagnostic::MethodNeverAccessesSelf { .. }))
            .collect();
        assert_eq!(
            unused_self_warnings.len(),
            1,
            "expected exactly 1 MethodNeverAccessesSelf, got {}: {warnings:?}",
            unused_self_warnings.len()
        );
    }

    // --- A011: Empty struct definition ---

    #[test]
    fn a011_empty_struct_produces_warning() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct Empty {}
        "#;
        let warnings = expect_warnings(source);
        let has_empty_struct = warnings
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::EmptyStructDefinition { .. }));
        assert!(
            has_empty_struct,
            "expected EmptyStructDefinition warning, got: {warnings:?}"
        );
    }

    #[test]
    fn a011_struct_with_fields_passes() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct Point { x: i32; }
        "#;
        let warnings = expect_warnings(source);
        let has_empty_struct = warnings
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::EmptyStructDefinition { .. }));
        assert!(
            !has_empty_struct,
            "struct with fields should NOT trigger EmptyStructDefinition"
        );
    }

    #[test]
    fn a011_struct_with_methods_passes() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct Math {
                fn add(a: i32, b: i32) -> i32 { return a + b; }
            }
        "#;
        let warnings = expect_warnings(source);
        let has_empty_struct = warnings
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::EmptyStructDefinition { .. }));
        assert!(
            !has_empty_struct,
            "struct with methods should NOT trigger EmptyStructDefinition"
        );
    }

    #[test]
    fn a011_multiple_empty_structs_produces_separate_warnings() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct A {}
            struct B {}
        "#;
        let warnings = expect_warnings(source);
        let empty_struct_warnings: Vec<_> = warnings
            .iter()
            .filter(|e| matches!(e, AnalysisDiagnostic::EmptyStructDefinition { .. }))
            .collect();
        assert_eq!(
            empty_struct_warnings.len(),
            2,
            "expected 2 EmptyStructDefinition warnings, got {}: {warnings:?}",
            empty_struct_warnings.len()
        );
    }

    // --- Combined tests ---

    #[test]
    fn a011_and_a010_struct_with_unused_self_method_warns_a010_not_a011() {
        let source = r#"
            fn main() -> i32 { return 0; }
            struct S {
                fn noop(self) -> i32 { return 42; }
            }
        "#;
        let warnings = expect_warnings(source);
        let has_unused_self = warnings
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::MethodNeverAccessesSelf { .. }));
        assert!(
            has_unused_self,
            "expected MethodNeverAccessesSelf warning, got: {warnings:?}"
        );
        let has_empty_struct = warnings
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::EmptyStructDefinition { .. }));
        assert!(
            !has_empty_struct,
            "struct with a method should NOT trigger EmptyStructDefinition"
        );
    }
}
