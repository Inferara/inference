/// Integration tests for analysis rule A029.
///
/// - A029: CompoundLiteralInCompoundAssign -- compound literals cannot be assigned directly to compound elements
/// - A030: removed (multidimensional scalar array uzumaki is now supported at any depth)
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
                .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. }));
            assert!(!has_a029, "variable RHS should be accepted, got: {e}");
        }
    }

    /// Struct literal assigned to a struct member access is rejected by A029.
    /// Previously the type checker rejected this due to Struct/Custom mismatch,
    /// but the resolve_custom_type fix now lets it pass type checking, so A029
    /// handles it.
    #[test]
    fn a029_struct_literal_to_member_access_rejected() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Outer { inner: Inner; val: i32; }
            fn main() -> i32 {
                let mut o: Outer = Outer { inner: Inner { x: 0, y: 0 }, val: 0 };
                o.inner = Inner { x: 1, y: 2 };
                return o.inner.x;
            }
        "#;
        let errors = expect_errors(source);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. })),
            "expected CompoundLiteralInCompoundAssign, got: {errors:?}"
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
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. }));
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
            .find(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. }))
            .expect("expected CompoundLiteralInCompoundAssign");
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
                .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. }));
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
            .find(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. }))
            .expect("expected CompoundLiteralInCompoundAssign");
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

    // --- A030 removed: multidimensional scalar array uzumaki now accepted at any depth ---

    #[test]
    fn a030_removed_3d_array_uzumaki_accepted() {
        let source = r#"
            fn main() -> i32 { return 0; }
            spec S {
                fn check() {
                    forall {
                        let c: [[[i32; 2]; 3]; 4] = @;
                    }
                }
            }
        "#;
        let result = analyze(source);
        assert!(result.is_ok(), "3D scalar array uzumaki should be accepted (A030 removed), got: {result:?}");
    }

    #[test]
    fn a030_removed_4d_array_uzumaki_accepted() {
        let source = r#"
            fn main() -> i32 { return 0; }
            spec S {
                fn check() {
                    forall {
                        let d: [[[[i32; 2]; 3]; 4]; 5] = @;
                    }
                }
            }
        "#;
        let result = analyze(source);
        assert!(result.is_ok(), "4D scalar array uzumaki should be accepted (A030 removed), got: {result:?}");
    }

    #[test]
    fn a030_removed_uzumaki_in_exists_block_accepted() {
        let source = r#"
            fn main() -> i32 { return 0; }
            spec S {
                fn check() {
                    exists {
                        let c: [[[i32; 2]; 3]; 4] = @;
                    }
                }
            }
        "#;
        let result = analyze(source);
        assert!(result.is_ok(), "3D scalar array uzumaki in exists block should be accepted (A030 removed), got: {result:?}");
    }

    #[test]
    fn a030_removed_outside_nondet_still_caught_by_a006() {
        let source = r#"
            fn main() -> i32 {
                let c: [[[i32; 2]; 3]; 4] = @;
                return 0;
            }
        "#;
        let errors = expect_errors(source);
        let has_a006 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOutsideNonDetBlock { .. }));
        assert!(has_a006, "A006 should fire for uzumaki outside nondet block, got: {errors:?}");
    }

    // --- A029: Compound literal in array index assignment ---

    #[test]
    fn a029_array_index_struct_literal_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() -> i32 {
                let mut pts: [Point; 2] = [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }];
                pts[0] = Point { x: 10, y: 20 };
                return pts[0].x;
            }
        "#;
        let errors = expect_errors(source);
        let has_a029 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. }));
        assert!(has_a029, "struct literal in array index assignment should be rejected, got: {errors:?}");
    }

    #[test]
    fn a029_array_index_array_literal_rejected() {
        let source = r#"
            fn main() -> i32 {
                let mut arr: [[i32; 2]; 2] = [[1, 2], [3, 4]];
                arr[0] = [10, 20];
                return arr[0][0];
            }
        "#;
        let errors = expect_errors(source);
        let has_a029 = errors
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. }));
        assert!(has_a029, "array literal in array index assignment should be rejected, got: {errors:?}");
    }

    #[test]
    fn a029_array_index_scalar_accepted() {
        let source = r#"
            fn main() -> i32 {
                let mut arr: [i32; 3] = [1, 2, 3];
                arr[0] = 42;
                return arr[0];
            }
        "#;
        let result = analyze(source);
        if let Err(ref e) = result {
            let has_a029 = e
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralInCompoundAssign { .. }));
            assert!(!has_a029, "scalar array index assignment should pass analysis, got: {e}");
        }
    }

}
