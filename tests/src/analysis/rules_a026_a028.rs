/// Integration tests for analysis rules A026, A027, A028.
///
/// - A026: NestedCompoundDepthExceeded — struct definitions must not have compound nesting deeper than 1 level
/// - A027: UzumakiOnNestedStruct — uzumaki on struct with compound fields is rejected
/// - A028: UzumakiOnStructInArray — uzumaki on array of structs is rejected
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

    // --- A026: Nested compound depth ---

    #[test]
    fn a026_depth_1_struct_in_struct_accepted() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Outer { inner: Inner; val: i32; }
            fn main() -> i32 { return 0; }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a026 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::NestedCompoundDepthExceeded { .. }));
            assert!(!has_a026, "depth-1 struct nesting should be accepted, got: {e}");
        }
    }

    #[test]
    fn a026_depth_1_array_in_struct_accepted() {
        let source = r#"
            struct HasArray { arr: [i32; 3]; val: i32; }
            fn main() -> i32 { return 0; }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a026 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::NestedCompoundDepthExceeded { .. }));
            assert!(!has_a026, "array-in-struct (depth 1) should be accepted, got: {e}");
        }
    }

    #[test]
    fn a026_depth_1_array_of_flat_struct_in_struct_accepted() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Outer { items: [Inner; 3]; val: i32; }
            fn main() -> i32 { return 0; }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a026 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::NestedCompoundDepthExceeded { .. }));
            assert!(!has_a026, "array-of-flat-struct in struct (depth 1) should be accepted, got: {e}");
        }
    }

    #[test]
    fn a026_array_of_arrays_in_struct_accepted() {
        let source = r#"
            struct Foo { grid: [[i32; 3]; 2]; val: i32; }
            fn main() -> i32 { return 0; }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a026 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::NestedCompoundDepthExceeded { .. }));
            assert!(!has_a026, "array-of-arrays (no struct nesting) should be accepted, got: {e}");
        }
    }

    #[test]
    fn a026_depth_2_struct_in_struct_in_struct_rejected() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Middle { inner: Inner; val: i32; }
            struct Deep { middle: Middle; z: i32; }
            fn main() -> i32 { return 0; }
        "#;
        let errors = expect_errors(source);
        let a026_errors: Vec<_> = errors
            .iter()
            .filter(|e| matches!(e, AnalysisDiagnostic::NestedCompoundDepthExceeded { .. }))
            .collect();
        assert!(
            !a026_errors.is_empty(),
            "depth-2 nesting should be rejected, got: {errors:?}"
        );
    }

    #[test]
    fn a026_depth_2_struct_with_array_of_structs_field_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            struct HasPoints { points: [Point; 3]; val: i32; }
            struct Wrapper { hp: HasPoints; z: i32; }
            fn main() -> i32 { return 0; }
        "#;
        let errors = expect_errors(source);
        let a026_errors: Vec<_> = errors
            .iter()
            .filter(|e| matches!(e, AnalysisDiagnostic::NestedCompoundDepthExceeded { .. }))
            .collect();
        assert!(
            !a026_errors.is_empty(),
            "depth-2 nesting via array-of-structs should be rejected, got: {errors:?}"
        );
    }

    #[test]
    fn a026_depth_2_array_of_nested_struct_in_struct_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            struct Inner { p: Point; }
            struct Outer { items: [Inner; 3]; }
            fn main() -> i32 { return 0; }
        "#;
        let errors = expect_errors(source);
        let a026_errors: Vec<_> = errors
            .iter()
            .filter(|e| matches!(e, AnalysisDiagnostic::NestedCompoundDepthExceeded { .. }))
            .collect();
        assert!(
            !a026_errors.is_empty(),
            "array of nested struct in struct should be rejected (depth-2 via [Inner; 3] where Inner has compound field), got: {errors:?}"
        );
    }

    #[test]
    fn a026_error_message_includes_struct_and_field_names() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Middle { inner: Inner; val: i32; }
            struct Deep { middle: Middle; z: i32; }
            fn main() -> i32 { return 0; }
        "#;
        let errors = expect_errors(source);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::NestedCompoundDepthExceeded { .. }))
            .expect("expected NestedCompoundDepthExceeded");
        let msg = diag.to_string();
        assert!(
            msg.contains("Deep"),
            "error message should include outer struct name, got: {msg}"
        );
        assert!(
            msg.contains("middle"),
            "error message should include field name, got: {msg}"
        );
    }

    #[test]
    fn a026_rule_id_is_a026() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Middle { inner: Inner; val: i32; }
            struct Deep { middle: Middle; z: i32; }
            fn main() -> i32 { return 0; }
        "#;
        let errors = expect_errors(source);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::NestedCompoundDepthExceeded { .. }))
            .expect("expected NestedCompoundDepthExceeded");
        assert_eq!(diag.rule_id(), "A026");
    }

    /// Known limitation: type aliases are not resolved by A026, so depth-2
    /// nesting via a type alias is not detected. This test documents the
    /// current behavior — it should start failing once type alias resolution
    /// is implemented, at which point the assertion should be flipped.
    #[test]
    fn a026_type_alias_bypasses_check() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Middle { inner: Inner; val: i32; }
            type MiddleAlias = Middle;
            struct Deep { m: MiddleAlias; z: i32; }
            fn main() -> i32 { return 0; }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a026 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::NestedCompoundDepthExceeded { .. }));
            assert!(!has_a026, "type alias nesting is not yet detected (known limitation), got: {e}");
        }
    }

    #[test]
    fn a026_flat_struct_accepted() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() -> i32 { return 0; }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a026 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::NestedCompoundDepthExceeded { .. }));
            assert!(!has_a026, "flat struct should be accepted, got: {e}");
        }
    }

    /// Known limitation: A026 does recurse into spec definitions (via
    /// `Def::Spec` in `check_defs`), but `lookup_struct` does not register
    /// spec-scoped structs, so the lookup returns `None` and no error is
    /// emitted. Should be flipped once spec-scoped struct registration is
    /// implemented.
    #[test]
    fn a026_struct_in_spec_not_detected() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Middle { inner: Inner; val: i32; }
            spec MySpec {
                struct Deep { middle: Middle; z: i32; }
                fn check() -> i32 { return 0; }
            }
            fn main() -> i32 { return 0; }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a026 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::NestedCompoundDepthExceeded { .. }));
            assert!(
                !has_a026,
                "spec-scoped structs are not yet detected (known limitation), got: {e}"
            );
        }
    }

    /// AD-1 / AD-5 symmetry: A026 fires on struct *definitions*, not on
    /// bindings, so a depth-2 nested struct used as a `const` initializer
    /// is rejected via the same code path as `let`. Pinned here so that any
    /// future short-circuit in the const-init path that bypasses struct-def
    /// analysis (e.g., a future CTFE pre-pass) re-trips A026.
    #[test]
    fn a026_depth_2_via_const_initializer_rejected() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Middle { inner: Inner; val: i32; }
            struct Deep { middle: Middle; z: i32; }
            fn main() -> i32 {
                const D: Deep = Deep { middle: Middle { inner: Inner { x: 1, y: 2 }, val: 3 }, z: 4 };
                return D.z;
            }
        "#;
        let errors = expect_errors(source);
        let a026_errors: Vec<_> = errors
            .iter()
            .filter(|e| matches!(e, AnalysisDiagnostic::NestedCompoundDepthExceeded { .. }))
            .collect();
        assert!(
            !a026_errors.is_empty(),
            "depth-2 nested struct used in const initializer should be rejected by A026, got: {errors:?}"
        );
    }

    /// Module definitions are not yet supported by the parser. This test
    /// confirms the parser rejects them — once supported, A026 should
    /// detect depth-2 nesting inside modules.
    #[test]
    fn a026_struct_in_module_not_parsed() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Middle { inner: Inner; val: i32; }
            mod utils {
                struct Deep { middle: Middle; z: i32; }
            }
            fn main() -> i32 { return 0; }
        "#;
        let result = crate::utils::try_build_ast(source.to_string());
        assert!(
            result.is_err(),
            "expected parse error for module definition, but parsing succeeded"
        );
    }

    // --- A027: Uzumaki on nested struct ---

    #[test]
    fn a027_uzumaki_on_flat_struct_accepted() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() -> i32 {
                forall {
                    let p: Point = @;
                }
                return 0;
            }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a027 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnNestedStruct { .. }));
            assert!(!has_a027, "uzumaki on flat struct should be accepted, got: {e}");
        }
    }

    #[test]
    fn a027_uzumaki_on_nested_struct_rejected() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Outer { inner: Inner; val: i32; }
            fn main() -> i32 {
                forall {
                    let o: Outer = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a027 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnNestedStruct { .. }));
        assert!(has_a027, "uzumaki on nested struct should be rejected, got: {errors:?}");
    }

    #[test]
    fn a027_uzumaki_on_struct_with_scalar_array_field_accepted() {
        let source = r#"
            struct HasArray { arr: [i32; 3]; val: i32; }
            fn main() -> i32 {
                forall {
                    let h: HasArray = @;
                }
                return 0;
            }
        "#;
        let result = analyze(source);
        assert!(result.is_ok(), "struct with scalar array field should pass analysis, got: {result:?}");
    }

    #[test]
    fn a027_uzumaki_on_struct_with_struct_array_field_rejected() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct HasStructArray { items: [Inner; 3]; val: i32; }
            fn main() -> i32 {
                forall {
                    let h: HasStructArray = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a027 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnNestedStruct { .. }));
        assert!(has_a027, "uzumaki on struct with struct-array field should be rejected, got: {errors:?}");
    }

    #[test]
    fn a027_uzumaki_on_struct_with_2d_array_field_rejected() {
        let source = r#"
            struct Has2D { grid: [[i32; 3]; 2]; val: i32; }
            fn main() -> i32 {
                forall {
                    let h: Has2D = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a027 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnNestedStruct { .. }));
        assert!(has_a027, "uzumaki on struct with 2D array field should be rejected, got: {errors:?}");
    }

    #[test]
    fn a027_error_message_includes_struct_name() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Outer { inner: Inner; val: i32; }
            fn main() -> i32 {
                forall {
                    let o: Outer = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::UzumakiOnNestedStruct { .. }))
            .expect("expected UzumakiOnNestedStruct");
        let msg = diag.to_string();
        assert!(
            msg.contains("Outer"),
            "error message should include struct name, got: {msg}"
        );
    }

    #[test]
    fn a027_rule_id_is_a027() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Outer { inner: Inner; val: i32; }
            fn main() -> i32 {
                forall {
                    let o: Outer = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::UzumakiOnNestedStruct { .. }))
            .expect("expected UzumakiOnNestedStruct");
        assert_eq!(diag.rule_id(), "A027");
    }

    #[test]
    fn a027_uzumaki_on_scalar_accepted() {
        let source = r#"
            fn main() -> i32 {
                forall {
                    let x: i32 = @;
                }
                return 0;
            }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a027 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnNestedStruct { .. }));
            assert!(!has_a027, "uzumaki on scalar should be accepted, got: {e}");
        }
    }

    /// Uzumaki outside a nondet block is caught by A006, not A027. Verify
    /// that A027 does not fire, avoiding redundant diagnostics.
    #[test]
    fn a027_no_fire_outside_nondet_block() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Outer { inner: Inner; val: i32; }
            fn main() -> i32 {
                let o: Outer = @;
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a027 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnNestedStruct { .. }));
        assert!(!has_a027, "A027 should not fire outside nondet block (A006 handles it), got: {errors:?}");
        let has_a006 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOutsideNonDetBlock { .. }));
        assert!(has_a006, "A006 should fire for uzumaki outside nondet block, got: {errors:?}");
    }

    #[test]
    fn a027_uzumaki_in_exists_block_rejected() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Outer { inner: Inner; val: i32; }
            fn main() -> i32 {
                exists {
                    let o: Outer = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a027 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnNestedStruct { .. }));
        assert!(has_a027, "uzumaki on nested struct in exists block should be rejected, got: {errors:?}");
    }

    #[test]
    fn a027_uzumaki_in_unique_block_rejected() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Outer { inner: Inner; val: i32; }
            fn main() -> i32 {
                unique {
                    let o: Outer = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a027 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnNestedStruct { .. }));
        assert!(has_a027, "uzumaki on nested struct in unique block should be rejected, got: {errors:?}");
    }

    #[test]
    fn a027_uzumaki_in_method_body_rejected() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Outer { inner: Inner; val: i32; }
            struct Checker {
                val: i32;
                fn check(self) -> i32 {
                    forall {
                        let o: Outer = @;
                    }
                    return self.val;
                }
            }
            fn main() -> i32 { return 0; }
        "#;
        let errors = expect_errors(source);
        let has_a027 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnNestedStruct { .. }));
        assert!(has_a027, "uzumaki on nested struct in method body should be rejected, got: {errors:?}");
    }

    // --- A028: Uzumaki on struct-in-array ---

    #[test]
    fn a028_uzumaki_on_scalar_array_accepted() {
        let source = r#"
            fn main() -> i32 {
                forall {
                    let arr: [i32; 3] = @;
                }
                return 0;
            }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a028 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnStructInArray { .. }));
            assert!(!has_a028, "uzumaki on [i32; 3] should be accepted, got: {e}");
        }
    }

    #[test]
    fn a028_uzumaki_on_multidim_scalar_array_accepted() {
        let source = r#"
            fn main() -> i32 {
                forall {
                    let grid: [[i32; 3]; 2] = @;
                }
                return 0;
            }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a028 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnStructInArray { .. }));
            assert!(!has_a028, "uzumaki on [[i32; 3]; 2] should be accepted, got: {e}");
        }
    }

    #[test]
    fn a028_uzumaki_on_struct_array_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() -> i32 {
                forall {
                    let points: [Point; 3] = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a028 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnStructInArray { .. }));
        assert!(has_a028, "uzumaki on [Point; 3] should be rejected, got: {errors:?}");
    }

    #[test]
    fn a028_error_message() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() -> i32 {
                forall {
                    let points: [Point; 3] = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::UzumakiOnStructInArray { .. }))
            .expect("expected UzumakiOnStructInArray");
        let msg = diag.to_string();
        assert!(
            msg.contains("array of structs"),
            "error message should mention array of structs, got: {msg}"
        );
    }

    #[test]
    fn a028_rule_id_is_a028() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() -> i32 {
                forall {
                    let points: [Point; 3] = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::UzumakiOnStructInArray { .. }))
            .expect("expected UzumakiOnStructInArray");
        assert_eq!(diag.rule_id(), "A028");
    }

    /// Uzumaki outside a nondet block is caught by A006, not A028. Verify
    /// that A028 does not fire, avoiding redundant diagnostics.
    #[test]
    fn a028_no_fire_outside_nondet_block() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() -> i32 {
                let points: [Point; 3] = @;
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a028 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnStructInArray { .. }));
        assert!(!has_a028, "A028 should not fire outside nondet block (A006 handles it), got: {errors:?}");
        let has_a006 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOutsideNonDetBlock { .. }));
        assert!(has_a006, "A006 should fire for uzumaki outside nondet block, got: {errors:?}");
    }

    #[test]
    fn a028_uzumaki_in_exists_block_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() -> i32 {
                exists {
                    let points: [Point; 3] = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a028 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnStructInArray { .. }));
        assert!(has_a028, "uzumaki on [Point; 3] in exists block should be rejected, got: {errors:?}");
    }

    #[test]
    fn a028_uzumaki_in_unique_block_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() -> i32 {
                unique {
                    let points: [Point; 3] = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a028 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnStructInArray { .. }));
        assert!(has_a028, "uzumaki on [Point; 3] in unique block should be rejected, got: {errors:?}");
    }

    #[test]
    fn a028_uzumaki_on_nested_struct_array_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() -> i32 {
                forall {
                    let points: [[Point; 2]; 3] = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a028 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnStructInArray { .. }));
        assert!(has_a028, "uzumaki on [[Point; 2]; 3] should be rejected, got: {errors:?}");
    }

    #[test]
    fn a028_uzumaki_on_3d_struct_array_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() -> i32 {
                forall {
                    let cube: [[[Point; 2]; 3]; 4] = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a028 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnStructInArray { .. }));
        assert!(has_a028, "uzumaki on [[[Point; 2]; 3]; 4] should be rejected, got: {errors:?}");
    }

    // --- A027/A028: Uzumaki in const initializers ---

    #[test]
    fn a027_uzumaki_on_nested_struct_via_const_rejected() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Outer { inner: Inner; val: i32; }
            fn main() -> i32 {
                forall {
                    const O: Outer = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a027 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnNestedStruct { .. }));
        assert!(has_a027, "uzumaki on nested struct via const should be rejected, got: {errors:?}");
    }

    #[test]
    fn a028_uzumaki_on_struct_in_array_via_const_rejected() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            fn main() -> i32 {
                forall {
                    const ARR: [Inner; 2] = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a028 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnStructInArray { .. }));
        assert!(has_a028, "uzumaki on struct-in-array via const should be rejected, got: {errors:?}");
    }
}
