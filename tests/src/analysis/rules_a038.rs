/// Integration tests for analysis rule A038.
///
/// - A038: UzumakiOnCompoundField — in a struct literal, `@` may initialize a
///   scalar field (bool/number/enum) but not a struct- or array-typed field. A
///   compound field's `@` reaches codegen with no enclosing variable name and
///   panics; the rule rejects it at the analysis phase instead. This is the
///   struct-literal-field analogue of A014 (array uzumaki as a function argument).
///
/// These tests are the cross-crate guard that the rule fires through a real
/// parse -> type-check -> analyze pipeline on Inference source, complementing the
/// in-crate message/`rule_id` unit test in `core/analysis`. The rejected inputs
/// are type-valid (only analysis rejects them), so `type_check` must succeed: the
/// type checker threads each field-position `@` its declared field type.
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

    /// Returns true if any analysis error is an `UzumakiOnCompoundField` (A038).
    /// Filters by variant rather than asserting a total error count, since the
    /// surface may also trip unrelated rules.
    fn has_a038(source: &str) -> bool {
        match analyze(source) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnCompoundField { .. })),
        }
    }

    fn a038_diag(source: &str) -> AnalysisDiagnostic {
        analyze(source)
            .expect_err("expected analysis errors but got Ok")
            .errors()
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::UzumakiOnCompoundField { .. }))
            .expect("expected an UzumakiOnCompoundField diagnostic")
            .clone()
    }

    /// Counts how many `UzumakiOnCompoundField` (A038) diagnostics the analysis
    /// emits for `source`. Filters by variant so unrelated rules tripped by the
    /// same surface do not perturb the count.
    fn count_a038(source: &str) -> usize {
        match analyze(source) {
            Ok(_) => 0,
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::UzumakiOnCompoundField { .. }))
                .count(),
        }
    }

    // ---------------------------------------------------------------------
    // Rejected: the issue #225 regression guard and its variants
    // ---------------------------------------------------------------------

    /// THE regression guard for issue #225: a struct-typed field initialized with
    /// `@` inside a spec obligation. This exact program panicked proof-mode
    /// codegen ("Struct uzumaki ... has no enclosing variable name") before A038.
    #[test]
    fn a038_issue_225_repro_struct_field_in_spec_rejected() {
        let source = r#"
            struct Inner { v: i32; }
            struct Outer { i: Inner; }
            spec Check {
                fn obligation() {
                    forall {
                        let o: Outer = Outer { i: @ };
                    }
                }
            }
            pub fn main() {}
        "#;
        assert!(
            has_a038(source),
            "expected A038 for the struct-typed field `i: @` in the Outer literal (issue #225 repro)"
        );
    }

    /// The same struct-typed field case in a plain free function (no spec):
    /// the walker visits free-function bodies just as it does spec bodies.
    #[test]
    fn a038_struct_field_in_free_function_rejected() {
        let source = r#"
            struct Inner { v: i32; }
            struct Outer { i: Inner; }
            fn f() {
                forall {
                    let o: Outer = Outer { i: @ };
                }
            }
            fn main() {}
        "#;
        assert!(
            has_a038(source),
            "expected A038 for the struct-typed field `i: @` in a free function"
        );
    }

    /// A struct-typed field whose inner struct itself has multiple scalar fields
    /// (still one nesting level) is rejected: the field position has no variable
    /// name regardless of the inner struct's shape.
    #[test]
    fn a038_struct_field_with_multiscalar_inner_rejected() {
        let source = r#"
            struct Inner { x: i32; y: i32; z: i32; }
            struct Outer { inner: Inner; tag: i32; }
            fn main() {
                forall {
                    let o: Outer = Outer { inner: @, tag: 1 };
                }
            }
        "#;
        assert!(
            has_a038(source),
            "expected A038 for a struct-typed field even when the inner struct is all scalars"
        );
    }

    /// A scalar-array field (`[i32; 3]`) initialized with `@` is rejected. Even
    /// though A028 permits `let a: [i32; 3] = @;` at statement level (a slot
    /// exists there), the field position supplies no variable name, so codegen
    /// panics identically to a struct field.
    #[test]
    fn a038_scalar_array_field_rejected() {
        let source = r#"
            struct HasArr { a: [i32; 3]; }
            fn main() {
                forall {
                    let h: HasArr = HasArr { a: @ };
                }
            }
        "#;
        assert!(
            has_a038(source),
            "expected A038 for a scalar-array field `a: @` of type [i32; 3]"
        );
    }

    /// An array-of-structs field (`[Inner; 3]`) initialized with `@` is rejected.
    #[test]
    fn a038_array_of_struct_field_rejected() {
        let source = r#"
            struct Inner { v: i32; }
            struct HasArr { a: [Inner; 3]; }
            fn main() {
                forall {
                    let h: HasArr = HasArr { a: @ };
                }
            }
        "#;
        assert!(
            has_a038(source),
            "expected A038 for an array-of-structs field `a: @` of type [Inner; 3]"
        );
    }

    /// A multidimensional-array field (`[[i32; 2]; 3]`) initialized with `@` is
    /// rejected.
    #[test]
    fn a038_multidim_array_field_rejected() {
        let source = r#"
            struct HasGrid { a: [[i32; 2]; 3]; }
            fn main() {
                forall {
                    let h: HasGrid = HasGrid { a: @ };
                }
            }
        "#;
        assert!(
            has_a038(source),
            "expected A038 for a multidimensional-array field `a: @` of type [[i32; 2]; 3]"
        );
    }

    /// Two compound fields each initialized with `@` in one struct literal
    /// produce exactly two A038 diagnostics, one per offending field.
    #[test]
    fn a038_two_bad_fields_counted() {
        let source = r#"
            struct Inner { v: i32; }
            struct Outer { a: Inner; b: [i32; 3]; }
            fn main() {
                forall {
                    let o: Outer = Outer { a: @, b: @ };
                }
            }
        "#;
        assert_eq!(
            count_a038(source),
            2,
            "expected exactly two A038 diagnostics for two compound fields each set to @"
        );
    }

    /// A compound field `@` inside an `exists` block is rejected: every non-det
    /// block kind is walked, and the `nondet_depth` gate fires for all of them.
    #[test]
    fn a038_struct_field_in_exists_block_rejected() {
        let source = r#"
            struct Inner { v: i32; }
            struct Outer { i: Inner; }
            fn main() {
                exists {
                    let o: Outer = Outer { i: @ };
                }
            }
        "#;
        assert!(
            has_a038(source),
            "expected A038 for a struct-typed field `i: @` inside an exists block"
        );
    }

    /// A compound field `@` inside a `unique` block is rejected: the rule gates on
    /// `nondet_depth`, not a specific block kind, so all four non-det block kinds
    /// are covered.
    #[test]
    fn a038_struct_field_in_unique_block_rejected() {
        let source = r#"
            struct Inner { v: i32; }
            struct Outer { i: Inner; }
            fn main() {
                unique {
                    let o: Outer = Outer { i: @ };
                }
            }
        "#;
        assert!(
            has_a038(source),
            "expected A038 for a struct-typed field `i: @` inside a unique block"
        );
    }

    /// A compound field `@` inside an `assume` block is rejected; `assume`
    /// increments `nondet_depth` like the other non-det block kinds.
    #[test]
    fn a038_struct_field_in_assume_block_rejected() {
        let source = r#"
            struct Inner { v: i32; }
            struct Outer { i: Inner; }
            fn main() {
                forall {
                    assume {
                        let o: Outer = Outer { i: @ };
                    }
                }
            }
        "#;
        assert!(
            has_a038(source),
            "expected A038 for a struct-typed field `i: @` inside an assume block"
        );
    }

    /// A compound field `@` in a `const` initializer is rejected. The walker
    /// reaches `const` initializers via `Stmt::ConstDef` -- a distinct branch
    /// from `let` -- so this guards that the const-init lowering path does not
    /// bypass struct-literal-field analysis (mirrors the A027/A028 const tests).
    #[test]
    fn a038_compound_field_in_const_initializer_rejected() {
        let source = r#"
            struct Inner { v: i32; }
            struct Outer { i: Inner; }
            fn main() {
                forall {
                    const O: Outer = Outer { i: @ };
                }
            }
        "#;
        assert!(
            has_a038(source),
            "expected A038 for a compound field `i: @` in a const initializer"
        );
    }

    /// An array-of-enum field (`[Color; 3]`) initialized with `@` is rejected.
    /// This guards the predicate's match ordering: `Array(_, _) => true` precedes
    /// the `Custom`/enum exemption, so the enum exemption (which accepts a bare
    /// `Color` field) must not leak through an enclosing array.
    #[test]
    fn a038_array_of_enum_field_rejected() {
        let source = r#"
            enum Color { Red, Green, Blue }
            struct HasArr { a: [Color; 3]; }
            fn main() {
                forall {
                    let h: HasArr = HasArr { a: @ };
                }
            }
        "#;
        assert!(
            has_a038(source),
            "expected A038 for an array-of-enum field `a: @` of type [Color; 3]; the enum exemption must not extend through an array"
        );
    }

    /// A struct literal with a compound `@` field passed as a function argument is
    /// rejected by A038 (the field still needs a frame slot the position cannot
    /// supply). A012 (compound literal as argument) co-fires on the same literal;
    /// the variant-filtered helper isolates A038, documenting that A038 owns the
    /// field-`@` defect independently of the argument-position defect.
    #[test]
    fn a038_compound_field_in_struct_literal_argument_rejected() {
        let source = r#"
            struct Inner { v: i32; }
            struct Outer { i: Inner; }
            fn take(o: Outer) -> i32 {
                return 0;
            }
            fn main() {
                forall {
                    let r: i32 = take(Outer { i: @ });
                }
            }
        "#;
        assert!(
            has_a038(source),
            "expected A038 for a compound field `i: @` in a struct literal passed as an argument"
        );
    }

    /// A compound field `@` inside a struct *method* body is rejected: the walker
    /// descends into struct methods.
    #[test]
    fn a038_struct_field_in_method_body_rejected() {
        let source = r#"
            struct Inner { v: i32; }
            struct Outer { i: Inner; }
            struct Maker {
                n: i32;
                fn make(self) -> i32 {
                    forall {
                        let o: Outer = Outer { i: @ };
                    }
                    return self.n;
                }
            }
            fn main() {}
        "#;
        assert!(
            has_a038(source),
            "expected A038 for a struct-typed field `i: @` inside a struct method body"
        );
    }

    // ---------------------------------------------------------------------
    // Diagnostic quality
    // ---------------------------------------------------------------------

    /// The diagnostic names the offending field and its type, explains that
    /// uzumaki is only for scalar fields, and reports rule id A038.
    #[test]
    fn a038_diagnostic_names_field_and_type() {
        let source = r#"
            struct Inner { v: i32; }
            struct Outer { i: Inner; }
            fn main() {
                forall {
                    let o: Outer = Outer { i: @ };
                }
            }
        "#;
        let diag = a038_diag(source);
        let msg = diag.to_string();
        assert!(
            msg.contains("`i`"),
            "A038 message must name the offending field, got: {msg}"
        );
        assert!(
            msg.contains("Inner"),
            "A038 message must include the field type (single-file canonical key is the bare name), got: {msg}"
        );
        assert!(
            msg.contains("scalar"),
            "A038 message must explain uzumaki is only for scalar fields, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A038");
    }

    // ---------------------------------------------------------------------
    // Accepted: scalar and enum fields
    // ---------------------------------------------------------------------

    /// A scalar field (`x: i32`) initialized with `@` is accepted: a scalar `@`
    /// lowers to a single uzumaki opcode and needs no enclosing variable.
    #[test]
    fn a038_scalar_field_accepted() {
        let source = r#"
            struct Point { x: i32; }
            fn main() {
                forall {
                    let p: Point = Point { x: @ };
                }
            }
        "#;
        assert!(
            !has_a038(source),
            "a scalar field `x: @` must not trip A038"
        );
    }

    /// A `bool` field initialized with `@` is accepted: `bool` is scalar.
    #[test]
    fn a038_bool_field_accepted() {
        let source = r#"
            struct Flag { b: bool; }
            fn main() {
                forall {
                    let f: Flag = Flag { b: @ };
                }
            }
        "#;
        assert!(
            !has_a038(source),
            "a bool field `b: @` must not trip A038"
        );
    }

    /// An enum-typed field initialized with `@` is accepted: an enum lowers to a
    /// single scalar uzumaki opcode like any other scalar.
    #[test]
    fn a038_enum_field_accepted() {
        let source = r#"
            enum Color { Red, Green, Blue }
            struct Painted { c: Color; }
            fn main() {
                forall {
                    let p: Painted = Painted { c: @ };
                }
            }
        "#;
        assert!(
            !has_a038(source),
            "an enum-typed field `c: @` must not trip A038 (enums are scalar-like)"
        );
    }

    // ---------------------------------------------------------------------
    // Accepted: the correct way to write it, and positions A038 does not own
    // ---------------------------------------------------------------------

    /// The correct way to initialize a compound field: nest a literal whose
    /// scalar leaves use `@` (`Outer { i: Inner { v: @ } }`). The outer field `i`
    /// is set to a struct *literal*, not `@`, so A038 does not fire; the inner
    /// scalar field `v` uses `@`, which is allowed.
    #[test]
    fn a038_nested_literal_with_scalar_leaf_accepted() {
        let source = r#"
            struct Inner { v: i32; }
            struct Outer { i: Inner; }
            fn main() {
                forall {
                    let o: Outer = Outer { i: Inner { v: @ } };
                }
            }
        "#;
        assert!(
            !has_a038(source),
            "a nested literal whose inner scalar leaf uses @ is the correct form and must not trip A038"
        );
    }

    /// Whole-struct `let o: Outer = @;` is A027's position, not A038's. A038 must
    /// stay silent here; the `@` is not in a struct-literal field.
    #[test]
    fn a038_whole_struct_uzumaki_not_flagged() {
        let source = r#"
            struct Inner { v: i32; }
            struct Outer { i: Inner; }
            fn main() {
                forall {
                    let o: Outer = @;
                }
            }
        "#;
        assert!(
            !has_a038(source),
            "whole-struct `let o: Outer = @;` is A027's job, not A038's"
        );
    }

    /// A struct-literal field `@` written OUTSIDE any non-det block must not trip
    /// A038: the rule is `nondet_depth`-guarded, and A006 (uzumaki outside
    /// non-det block) owns that position. This proves the gate.
    #[test]
    fn a038_compound_field_outside_nondet_block_not_flagged() {
        let source = r#"
            struct Inner { v: i32; }
            struct Outer { i: Inner; }
            fn main() {
                let o: Outer = Outer { i: @ };
            }
        "#;
        assert!(
            !has_a038(source),
            "a struct-literal field `@` outside a non-det block is A006's job, not A038's"
        );
        let has_a006 = analyze(source)
            .expect_err("expected analysis errors but got Ok")
            .errors()
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOutsideNonDetBlock { .. }));
        assert!(
            has_a006,
            "A006 should fire for the field `@` outside a non-det block"
        );
    }

    /// A struct literal with no `@` field at all (all fields are concrete values)
    /// must not trip A038, even inside a non-det block.
    #[test]
    fn a038_struct_literal_without_uzumaki_accepted() {
        let source = r#"
            struct Inner { v: i32; }
            struct Outer { i: Inner; tag: i32; }
            fn main() {
                forall {
                    let o: Outer = Outer { i: Inner { v: 1 }, tag: 2 };
                }
            }
        "#;
        assert!(
            !has_a038(source),
            "a struct literal with no field `@` must not trip A038"
        );
    }
}
