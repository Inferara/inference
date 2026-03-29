/// Integration tests for analysis rules A029, A030.
///
/// - A029: CompoundLiteralInMemberAssign -- compound literals cannot be assigned directly to struct fields
/// - A030: UzumakiOnDeepArray -- uzumaki on arrays with more than 2 dimensions is rejected
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

    // --- A029: Compound literal in member access assignment ---

    #[test]
    fn a029_variable_rhs_accepted() {
        let source = r#"
            struct HasArray { arr: [i32; 3]; val: i32; }
            fn main() -> i32 {
                let temp: [i32; 3] = [4, 5, 6];
                let mut s: HasArray = HasArray { arr: [1, 2, 3], val: 0 };
                s.arr = temp;
                return s.arr[0];
            }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a029 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInMemberAssign { .. }));
            assert!(!has_a029, "variable RHS should be accepted, got: {e}");
        }
    }

    /// The type checker currently rejects struct literal assignment to a struct
    /// field due to Struct/Custom type comparison (known limitation). This test
    /// documents that the type checker catches it first. Once the type checker
    /// is fixed, this test should be updated to verify A029 catches it.
    #[test]
    fn a029_struct_literal_rhs_blocked_by_type_checker() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Outer { inner: Inner; val: i32; }
            fn main() -> i32 {
                let mut o: Outer = Outer { inner: Inner { x: 0, y: 0 }, val: 0 };
                o.inner = Inner { x: 1, y: 2 };
                return o.inner.x;
            }
        "#;
        let arena = build_ast(source.to_string());
        let result = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena);
        assert!(
            result.is_err(),
            "type checker currently rejects struct literal assignment to struct field (known limitation)"
        );
    }

    #[test]
    fn a029_array_literal_rhs_rejected() {
        let source = r#"
            struct HasArray { arr: [i32; 3]; val: i32; }
            fn main() -> i32 {
                let mut s: HasArray = HasArray { arr: [1, 2, 3], val: 0 };
                s.arr = [4, 5, 6];
                return s.arr[0];
            }
        "#;
        let errors = expect_errors(source);
        let has_a029 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInMemberAssign { .. }));
        assert!(has_a029, "array literal RHS should be rejected, got: {errors:?}");
    }

    #[test]
    fn a029_rule_id_is_a029() {
        let source = r#"
            struct HasArray { arr: [i32; 3]; val: i32; }
            fn main() -> i32 {
                let mut s: HasArray = HasArray { arr: [1, 2, 3], val: 0 };
                s.arr = [4, 5, 6];
                return s.arr[0];
            }
        "#;
        let errors = expect_errors(source);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInMemberAssign { .. }))
            .expect("expected CompoundLiteralInMemberAssign");
        assert_eq!(diag.rule_id(), "A029");
    }

    #[test]
    fn a029_scalar_field_assign_accepted() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() -> i32 {
                let mut p: Point = Point { x: 0, y: 0 };
                p.x = 42;
                return p.x;
            }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a029 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInMemberAssign { .. }));
            assert!(!has_a029, "scalar field assign should be accepted, got: {e}");
        }
    }

    #[test]
    fn a029_error_message() {
        let source = r#"
            struct HasArray { arr: [i32; 3]; val: i32; }
            fn main() -> i32 {
                let mut s: HasArray = HasArray { arr: [1, 2, 3], val: 0 };
                s.arr = [4, 5, 6];
                return s.arr[0];
            }
        "#;
        let errors = expect_errors(source);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInMemberAssign { .. }))
            .expect("expected CompoundLiteralInMemberAssign");
        let msg = diag.to_string();
        assert!(
            msg.contains("compound literal"),
            "error message should mention compound literal, got: {msg}"
        );
        assert!(
            msg.contains("temporary variable"),
            "error message should suggest workaround, got: {msg}"
        );
    }

    // --- A030: Uzumaki on deep array ---

    #[test]
    fn a030_1d_array_uzumaki_accepted() {
        let source = r#"
            fn main() -> i32 {
                forall {
                    let a: [i32; 3] = @;
                }
                return 0;
            }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a030 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnDeepArray { .. }));
            assert!(!has_a030, "1D array uzumaki should be accepted, got: {e}");
        }
    }

    #[test]
    fn a030_2d_array_uzumaki_accepted() {
        let source = r#"
            fn main() -> i32 {
                forall {
                    let g: [[i32; 3]; 2] = @;
                }
                return 0;
            }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a030 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnDeepArray { .. }));
            assert!(!has_a030, "2D array uzumaki should be accepted, got: {e}");
        }
    }

    #[test]
    fn a030_3d_array_uzumaki_rejected() {
        let source = r#"
            fn main() -> i32 {
                forall {
                    let c: [[[i32; 2]; 3]; 4] = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a030 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnDeepArray { .. }));
        assert!(has_a030, "3D array uzumaki should be rejected, got: {errors:?}");
    }

    #[test]
    fn a030_rule_id_is_a030() {
        let source = r#"
            fn main() -> i32 {
                forall {
                    let c: [[[i32; 2]; 3]; 4] = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::UzumakiOnDeepArray { .. }))
            .expect("expected UzumakiOnDeepArray");
        assert_eq!(diag.rule_id(), "A030");
    }

    #[test]
    fn a030_outside_nondet_not_checked() {
        let source = r#"
            fn main() -> i32 {
                let c: [[[i32; 2]; 3]; 4] = @;
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a030 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnDeepArray { .. }));
        assert!(!has_a030, "A030 should not fire outside nondet block (A006 handles it), got: {errors:?}");
        let has_a006 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOutsideNonDetBlock { .. }));
        assert!(has_a006, "A006 should fire for uzumaki outside nondet block, got: {errors:?}");
    }

    #[test]
    fn a030_error_message() {
        let source = r#"
            fn main() -> i32 {
                forall {
                    let c: [[[i32; 2]; 3]; 4] = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let diag = errors
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::UzumakiOnDeepArray { .. }))
            .expect("expected UzumakiOnDeepArray");
        let msg = diag.to_string();
        assert!(
            msg.contains("2 dimensions"),
            "error message should mention 2 dimensions, got: {msg}"
        );
    }

    #[test]
    fn a030_4d_array_uzumaki_rejected() {
        let source = r#"
            fn main() -> i32 {
                forall {
                    let d: [[[[i32; 2]; 3]; 4]; 5] = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a030 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnDeepArray { .. }));
        assert!(has_a030, "4D array uzumaki should be rejected, got: {errors:?}");
    }

    #[test]
    fn a030_uzumaki_in_exists_block_rejected() {
        let source = r#"
            fn main() -> i32 {
                exists {
                    let c: [[[i32; 2]; 3]; 4] = @;
                }
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a030 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnDeepArray { .. }));
        assert!(has_a030, "3D array uzumaki in exists block should be rejected, got: {errors:?}");
    }
}
