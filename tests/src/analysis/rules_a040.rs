/// Integration tests for analysis rule A040.
///
/// - A040: UzumakiOnCompoundArrayElement — in an array literal, `@` may be a
///   scalar element (bool/number/enum) but not a struct- or array-typed element.
///   A compound element's `@` reaches codegen with no enclosing variable name and
///   panics; the rule rejects it at the analysis phase instead. This is the
///   array-element analogue of A038 (uzumaki on a compound struct-literal field)
///   and A014 (array uzumaki as a function argument).
///
/// A040 is distinct from A028 (uzumaki on an array of structs): A028 flags the
/// *whole-array* form `let a: [Point; 2] = @;` (a `NodeId::Stmt` position), while
/// A040 flags an `@` *element* of an array literal `[p, @]`. A040 also rejects a
/// nested-array element (the outer `@` in `[@, [1, 2]]` typed `[[i32; 2]; 2]`),
/// which A028 does not cover.
///
/// These tests are the cross-crate guard that the rule fires through a real
/// parse -> type-check -> analyze pipeline on Inference source, complementing the
/// in-crate message/`rule_id` unit test in `core/analysis`. The rejected inputs
/// are type-valid (only analysis rejects them), so `type_check` must succeed: the
/// type checker threads each array-element `@` its declared element type.
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

    /// Returns true if any analysis error is an `UzumakiOnCompoundArrayElement`
    /// (A040). Filters by variant rather than asserting a total error count, since
    /// the surface may also trip unrelated rules.
    fn has_a040(source: &str) -> bool {
        match analyze(source) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnCompoundArrayElement { .. })),
        }
    }

    fn a040_diag(source: &str) -> AnalysisDiagnostic {
        analyze(source)
            .expect_err("expected analysis errors but got Ok")
            .errors()
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::UzumakiOnCompoundArrayElement { .. }))
            .expect("expected an UzumakiOnCompoundArrayElement diagnostic")
            .clone()
    }

    /// Counts how many `UzumakiOnCompoundArrayElement` (A040) diagnostics the
    /// analysis emits for `source`. Filters by variant so unrelated rules tripped
    /// by the same surface do not perturb the count.
    fn count_a040(source: &str) -> usize {
        match analyze(source) {
            Ok(_) => 0,
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::UzumakiOnCompoundArrayElement { .. }))
                .count(),
        }
    }

    // ---------------------------------------------------------------------
    // Rejected: struct, array, and nested-array elements
    // ---------------------------------------------------------------------

    /// A struct-typed array element initialized with `@` (`[Point { .. }, @]`)
    /// is rejected: the element position has no variable name, so a struct `@`
    /// there panics codegen ("Struct uzumaki ... has no enclosing variable name").
    #[test]
    fn a040_struct_element_in_vardef_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() {
                forall {
                    let a: [Point; 2] = [Point { x: 0, y: 0 }, @];
                }
            }
        "#;
        assert!(
            has_a040(source),
            "expected A040 for a struct-typed array element `@` in `[Point {{..}}, @]`"
        );
    }

    /// The same struct-element case inside a spec obligation (the proof-mode path
    /// that originally panicked): the walker visits spec bodies just as it does
    /// free-function bodies.
    #[test]
    fn a040_struct_element_in_spec_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            spec Check {
                fn obligation() {
                    forall {
                        let a: [Point; 2] = [Point { x: 0, y: 0 }, @];
                    }
                }
            }
            pub fn main() {}
        "#;
        assert!(
            has_a040(source),
            "expected A040 for a struct-typed array element `@` inside a spec obligation"
        );
    }

    /// A nested-array element initialized with `@` — the OUTER `@` of a 2D array
    /// literal typed `[[i32; 2]; 2]` — is rejected. The element type is itself an
    /// array (`[i32; 2]`), which has no enclosing variable name at the element
    /// position. A028 (whole-array `@`) does NOT cover this; A040 does.
    #[test]
    fn a040_nested_array_outer_element_rejected() {
        let source = r#"
            fn main() {
                forall {
                    let a: [[i32; 2]; 2] = [@, [1, 2]];
                }
            }
        "#;
        assert!(
            has_a040(source),
            "expected A040 for the outer array-typed element `@` in `[@, [1, 2]]` typed [[i32; 2]; 2]"
        );
    }

    /// An array-of-struct element `@` inside an array-typed struct field's literal
    /// is rejected. The struct field initializer is itself an array literal whose
    /// element is a struct-typed `@`.
    #[test]
    fn a040_struct_element_in_struct_field_literal_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            struct Holder { ps: [Point; 2]; }
            fn main() {
                forall {
                    let h: Holder = Holder { ps: [Point { x: 0, y: 0 }, @] };
                }
            }
        "#;
        assert!(
            has_a040(source),
            "expected A040 for a struct-typed array element `@` in an array-typed struct field literal"
        );
    }

    /// Two compound `@` elements in one array literal produce exactly two A040
    /// diagnostics, one per offending element.
    #[test]
    fn a040_two_compound_elements_counted() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() {
                forall {
                    let a: [Point; 2] = [@, @];
                }
            }
        "#;
        assert_eq!(
            count_a040(source),
            2,
            "expected exactly two A040 diagnostics for two struct-typed `@` elements"
        );
    }

    /// A struct-typed array element `@` inside an `exists` block is rejected:
    /// every non-det block kind is walked, and the `nondet_depth` gate fires for
    /// all of them.
    #[test]
    fn a040_struct_element_in_exists_block_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() {
                exists {
                    let a: [Point; 2] = [Point { x: 0, y: 0 }, @];
                }
            }
        "#;
        assert!(
            has_a040(source),
            "expected A040 for a struct-typed array element `@` inside an exists block"
        );
    }

    /// A struct-typed array element `@` inside a `unique` block is rejected; the
    /// rule gates on `nondet_depth`, not a specific block kind.
    #[test]
    fn a040_struct_element_in_unique_block_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() {
                unique {
                    let a: [Point; 2] = [Point { x: 0, y: 0 }, @];
                }
            }
        "#;
        assert!(
            has_a040(source),
            "expected A040 for a struct-typed array element `@` inside a unique block"
        );
    }

    /// A compound array element `@` in the RHS of an assignment is rejected. The
    /// assignment position (`a = [.., @];`) is a distinct type-checker branch from
    /// `let`; this guards that the assignment-position threading does not bypass
    /// array-element analysis.
    #[test]
    fn a040_struct_element_in_assignment_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() {
                let mut a: [Point; 2] = [Point { x: 0, y: 0 }, Point { x: 1, y: 1 }];
                forall {
                    a = [Point { x: 2, y: 2 }, @];
                }
            }
        "#;
        assert!(
            has_a040(source),
            "expected A040 for a struct-typed array element `@` on the RHS of an assignment"
        );
    }

    /// A struct-typed array element `@` inside an `assume` block is rejected;
    /// `assume` increments `nondet_depth` like the other non-det block kinds, so
    /// all four kinds (forall/exists/unique/assume) are covered.
    #[test]
    fn a040_struct_element_in_assume_block_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() {
                forall {
                    assume {
                        let a: [Point; 2] = [Point { x: 0, y: 0 }, @];
                    }
                }
            }
        "#;
        assert!(
            has_a040(source),
            "expected A040 for a struct-typed array element `@` inside an assume block"
        );
    }

    /// A compound array element `@` in a `const` initializer is rejected. The
    /// walker reaches `const` initializers via `Stmt::ConstDef` -- a distinct
    /// branch from `let` -- so this guards that the const-init type-checker path
    /// threads the element type onto the `@` (without which the element would
    /// arrive untyped and codegen would panic rather than A040 firing).
    #[test]
    fn a040_compound_element_in_const_initializer_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() {
                forall {
                    const A: [Point; 2] = [Point { x: 0, y: 0 }, @];
                }
            }
        "#;
        assert!(
            has_a040(source),
            "expected A040 for a compound array element `@` in a const initializer"
        );
    }

    // ---------------------------------------------------------------------
    // Diagnostic quality
    // ---------------------------------------------------------------------

    /// The diagnostic names the offending element type, explains that only scalar
    /// elements may use `@`, and reports rule id A040.
    #[test]
    fn a040_diagnostic_names_type() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() {
                forall {
                    let a: [Point; 2] = [Point { x: 0, y: 0 }, @];
                }
            }
        "#;
        let diag = a040_diag(source);
        let msg = diag.to_string();
        assert!(
            msg.contains("Point"),
            "A040 message must include the element type (single-file canonical key is the bare name), got: {msg}"
        );
        assert!(
            msg.contains("scalar"),
            "A040 message must explain only scalar array elements may use @, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A040");
    }

    // ---------------------------------------------------------------------
    // Accepted: scalar and enum elements
    // ---------------------------------------------------------------------

    /// A scalar array element `@` (`[0, @]` typed `[i32; 2]`) is accepted: a
    /// scalar `@` lowers to a single uzumaki opcode and needs no enclosing
    /// variable.
    #[test]
    fn a040_scalar_element_accepted() {
        let source = r#"
            fn main() {
                forall {
                    let a: [i32; 2] = [0, @];
                }
            }
        "#;
        assert!(
            !has_a040(source),
            "a scalar array element `@` in `[0, @]` must not trip A040"
        );
    }

    /// The INNER `@` of a 2D array literal (`[[0, @], [1, 2]]` typed
    /// `[[i32; 2]; 2]`) is a scalar element of the inner array and is accepted.
    /// This guards that the recursion types inner elements with the scalar leaf
    /// type, not the array type, so a genuinely scalar `@` is not over-rejected.
    #[test]
    fn a040_nested_array_inner_scalar_element_accepted() {
        let source = r#"
            fn main() {
                forall {
                    let a: [[i32; 2]; 2] = [[0, @], [1, 2]];
                }
            }
        "#;
        assert!(
            !has_a040(source),
            "the inner scalar element `@` of `[[0, @], [1, 2]]` must not trip A040"
        );
    }

    /// A `bool` array element `@` is accepted: `bool` is scalar.
    #[test]
    fn a040_bool_element_accepted() {
        let source = r#"
            fn main() {
                forall {
                    let a: [bool; 2] = [true, @];
                }
            }
        "#;
        assert!(
            !has_a040(source),
            "a bool array element `@` must not trip A040"
        );
    }

    /// An `i64` array element `@` is accepted: a 64-bit scalar lowers to the
    /// i64 uzumaki opcode and needs no enclosing variable.
    #[test]
    fn a040_i64_element_accepted() {
        let source = r#"
            fn main() {
                forall {
                    let a: [i64; 2] = [0, @];
                }
            }
        "#;
        assert!(
            !has_a040(source),
            "an i64 array element `@` must not trip A040"
        );
    }

    /// An enum-typed array element `@` is accepted: an enum lowers to a single
    /// scalar uzumaki opcode like any other scalar.
    #[test]
    fn a040_enum_element_accepted() {
        let source = r#"
            enum Color { Red, Green, Blue }
            fn main() {
                forall {
                    let a: [Color; 2] = [Color::Red, @];
                }
            }
        "#;
        assert!(
            !has_a040(source),
            "an enum-typed array element `@` must not trip A040 (enums are scalar-like)"
        );
    }

    // ---------------------------------------------------------------------
    // Accepted: positions A040 does not own
    // ---------------------------------------------------------------------

    /// Whole-array `let a: [i32; 2] = @;` is not an element position. A040 must
    /// stay silent here; the `@` is the array value, not an array-literal element.
    #[test]
    fn a040_whole_scalar_array_uzumaki_not_flagged() {
        let source = r#"
            fn main() {
                forall {
                    let a: [i32; 2] = @;
                }
            }
        "#;
        assert!(
            !has_a040(source),
            "whole-array `let a: [i32; 2] = @;` is not an array-element position; A040 must not fire"
        );
    }

    /// Whole-array `let a: [Point; 2] = @;` is A028's position (uzumaki on an
    /// array of structs), not A040's. A040 must stay silent; the `@` is not an
    /// array-literal element.
    #[test]
    fn a040_whole_struct_array_uzumaki_not_flagged() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() {
                forall {
                    let a: [Point; 2] = @;
                }
            }
        "#;
        assert!(
            !has_a040(source),
            "whole-array `let a: [Point; 2] = @;` is A028's job, not A040's"
        );
    }

    /// A struct-literal field `@` (`Point { x: @ }`) is A038's position, not
    /// A040's. A040 must stay silent; the `@` is a struct-literal field, not an
    /// array-literal element.
    #[test]
    fn a040_struct_literal_field_uzumaki_not_flagged() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() {
                forall {
                    let p: Point = Point { x: @, y: 0 };
                }
            }
        "#;
        assert!(
            !has_a040(source),
            "a struct-literal field `@` is A038's job, not A040's"
        );
    }

    /// A compound array-element `@` written OUTSIDE any non-det block must not
    /// trip A040: the rule is `nondet_depth`-guarded, and A006 (uzumaki outside
    /// non-det block) owns that position. This proves the gate.
    #[test]
    fn a040_compound_element_outside_nondet_block_not_flagged() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() {
                let a: [Point; 2] = [Point { x: 0, y: 0 }, @];
            }
        "#;
        assert!(
            !has_a040(source),
            "a compound array element `@` outside a non-det block is A006's job, not A040's"
        );
        let has_a006 = analyze(source)
            .expect_err("expected analysis errors but got Ok")
            .errors()
            .iter()
            .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOutsideNonDetBlock { .. }));
        assert!(
            has_a006,
            "A006 should fire for the element `@` outside a non-det block"
        );
    }

    /// An array literal with no `@` element at all (all concrete values) must not
    /// trip A040, even inside a non-det block.
    #[test]
    fn a040_array_literal_without_uzumaki_accepted() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn main() {
                forall {
                    let a: [Point; 2] = [Point { x: 0, y: 0 }, Point { x: 1, y: 1 }];
                }
            }
        "#;
        assert!(
            !has_a040(source),
            "an array literal with no element `@` must not trip A040"
        );
    }

    /// Boundary guard: an array literal with a `@` element passed *as a function
    /// argument* (`take([0, @])`) is rejected by A012 (a compound literal cannot be
    /// a function argument, regardless of its elements), so it never reaches codegen
    /// and the `@` is never threaded a type. A040 therefore does not — and need not —
    /// fire here. This pins the boundary so the (unreachable) argument path is not
    /// later "fixed" by threading element types into it, which would be dead code
    /// and risk masking removal of the A012 guard.
    #[test]
    fn a040_array_literal_uzumaki_element_as_argument_is_a012_not_a040() {
        let source = r#"
            fn take(a: [i32; 2]) -> i32 { return a[0]; }
            spec C {
                fn o() {
                    forall {
                        let r: i32 = take([0, @]);
                    }
                }
            }
            pub fn main() {}
        "#;
        let errors = analyze(source).expect_err("expected analysis errors but got Ok");
        assert!(
            errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralAsArgument { .. })),
            "an array-literal argument must be rejected by A012, got: {:?}",
            errors.errors()
        );
        assert!(
            !errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UzumakiOnCompoundArrayElement { .. })),
            "A040 must not fire for an array-literal argument (A012 owns that position), got: {:?}",
            errors.errors()
        );
    }
}
