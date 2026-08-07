/// Integration tests for analysis rule A045.
///
/// - A045: `FieldLessStructValue` — a struct with no fields occupies zero bytes,
///   so it has no value representation. Such a type is rejected as a struct
///   literal, as the declared type of a `let`/`const`, as a parameter, as a
///   return type, as a struct field, and as a `self` receiver, with arrays of it
///   looked through at any nesting depth. Declaring a field-less struct stays
///   legal — a field-less struct with associated functions is the supported
///   method-namespace idiom (`E::helper()`).
///
/// These tests exercise the rule through a real parse -> type-check -> analyze
/// pipeline, complementing the in-crate message/`rule_id` unit tests in
/// `core/analysis`. Several of the rejected shapes compiled silently before this
/// rule existed and one aborted the compiler on an internal assert, so the
/// acceptance half of the matrix (structs with fields, one-byte structs, enums,
/// scalars, and the namespace idiom) is asserted just as thoroughly as the
/// rejection half.
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::{
        build_ast, try_codegen, try_codegen_no_analysis, try_type_check_multi_file,
    };
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

    /// Returns true if any analysis error is a `FieldLessStructValue` (A045).
    /// Filters by variant rather than asserting a total error count, since the
    /// surface may also trip unrelated rules (or warnings).
    fn has_a045(source: &str) -> bool {
        match analyze(source) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::FieldLessStructValue { .. })),
        }
    }

    /// Counts how many `FieldLessStructValue` (A045) diagnostics the analysis
    /// emits, filtering by variant so unrelated rules do not perturb the count.
    fn count_a045(source: &str) -> usize {
        match analyze(source) {
            Ok(_) => 0,
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::FieldLessStructValue { .. }))
                .count(),
        }
    }

    /// Collects the `position` string of every A045 diagnostic, in report order,
    /// so a test can pin exactly which positions fired.
    fn a045_positions(source: &str) -> Vec<&'static str> {
        match analyze(source) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .errors()
                .iter()
                .filter_map(|e| match e {
                    AnalysisDiagnostic::FieldLessStructValue { position, .. } => Some(*position),
                    _ => None,
                })
                .collect(),
        }
    }

    fn a045_diag(source: &str) -> AnalysisDiagnostic {
        analyze(source)
            .expect_err("expected analysis errors but got Ok")
            .errors()
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::FieldLessStructValue { .. }))
            .expect("expected a FieldLessStructValue diagnostic")
            .clone()
    }

    /// Whether any analysis *warning* with the given rule id was produced, on
    /// either the success or the error path.
    fn has_warning(source: &str, rule_id: &str) -> bool {
        let warnings = match analyze(source) {
            Ok(result) => result.warnings().to_vec(),
            Err(errors) => errors.warnings().to_vec(),
        };
        warnings.iter().any(|w| w.rule_id() == rule_id)
    }

    /// Whether the whole pipeline accepts `source`: analysis first, then code
    /// generation. `try_codegen` unwraps the analysis result, so it panics rather
    /// than returning `Err` on a rejected program; splitting the two phases lets a
    /// test assert "this never reaches codegen" without relying on that panic.
    fn compiles(source: &str) -> bool {
        analyze(source).is_ok() && try_codegen_no_analysis(source).is_ok()
    }

    // ---------------------------------------------------------------------
    // The issue repros
    // ---------------------------------------------------------------------

    /// #332 repro B: before A045 this aborted the compiler with an internal
    /// `unreachable!` from struct-literal lowering. Three positions offend — the
    /// `self` receiver, the parameter, and the literal — and each is reported
    /// once. A010 still fires alongside; the two rules are complementary.
    #[test]
    fn a045_issue_repro_b_rejected_instead_of_panicking() {
        let source = r#"
            struct E { fn tag(self) -> i32 { return 7; } }
            pub fn f(mut p: E) -> i32 { p = E { }; return p.tag(); }
        "#;
        assert_eq!(
            a045_positions(source),
            vec![
                "the type of a `self` receiver",
                "the type of a parameter",
                "a struct literal",
            ],
            "repro B must report the receiver, the parameter, and the literal"
        );
        assert!(
            has_warning(source, "A010"),
            "A045 must not suppress A010's never-accesses-self warning"
        );
        assert!(
            !compiles(source),
            "repro B must be rejected before code generation"
        );
    }

    /// #332 repro A: this compiled by accident, lowering `e = e` as a scalar copy
    /// of a pointer into a zero-byte region. Exactly ONE A045 fires — on the
    /// parameter. The assignment and the reads of `e` produce none of their own,
    /// because rejecting the declaration is what makes them unreachable.
    #[test]
    fn a045_issue_repro_a_rejected_and_never_reaches_codegen() {
        let source = r#"
            struct E {
            }
            pub fn take(mut e: E) -> i32 {
                e = e;
                return 0;
            }
        "#;
        assert_eq!(
            count_a045(source),
            1,
            "the parameter is the only offending position; `e = e` must not add its own"
        );
        assert!(
            has_warning(source, "A011"),
            "A045 must not suppress A011's empty-struct warning"
        );
        assert!(
            !compiles(source),
            "repro A must no longer compile by accident"
        );
    }

    /// The issue names the local form of repro B as aborting at the same site.
    #[test]
    fn a045_local_form_of_repro_b_rejected() {
        let source = r#"
            struct E { fn tag(self) -> i32 { return 7; } }
            pub fn f() -> i32 { let mut p: E = E { }; p = E { }; return p.tag(); }
        "#;
        assert!(has_a045(source), "the local form must be rejected");
        assert!(
            !compiles(source),
            "the local form must be rejected before code generation"
        );
    }

    // ---------------------------------------------------------------------
    // Fires: struct literal, in every expression position
    // ---------------------------------------------------------------------

    #[test]
    fn a045_struct_literal_in_let_initializer_rejected() {
        let source = "struct E { } pub fn f() -> i32 { let e: E = E { }; return 0; }";
        assert!(
            a045_positions(source).contains(&"a struct literal"),
            "`let e: E = E {{ }};` must fire A045 on the literal"
        );
    }

    #[test]
    fn a045_struct_literal_in_const_initializer_rejected() {
        let source = "struct E { } pub fn f() -> i32 { const C: E = E { }; return 0; }";
        assert!(
            a045_positions(source).contains(&"a struct literal"),
            "a function-local `const` initializer literal must fire A045"
        );
    }

    #[test]
    fn a045_struct_literal_in_assignment_rhs_rejected() {
        let source = r#"
            struct E { fn tag(self) -> i32 { return 7; } }
            pub fn f(mut p: E) -> i32 { p = E { }; return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"a struct literal"),
            "an assignment RHS literal must fire A045"
        );
    }

    /// The return-type annotation and the literal are two independent offending
    /// positions, so both are reported.
    #[test]
    fn a045_struct_literal_in_return_rejected() {
        let source = r#"
            struct E { }
            fn make() -> E { return E { }; }
            pub fn main() -> i32 { return 0; }
        "#;
        let positions = a045_positions(source);
        assert!(
            positions.contains(&"a struct literal"),
            "the returned literal must fire A045, got: {positions:?}"
        );
        assert!(
            positions.contains(&"the return type of a function"),
            "the return-type annotation must fire A045, got: {positions:?}"
        );
    }

    /// A012 also rejects a compound literal argument; both fire, since the crate
    /// has no cross-rule suppression. A045 is the actionable one here.
    #[test]
    fn a045_struct_literal_as_call_argument_rejected() {
        let source = r#"
            struct E { fn tag(self) -> i32 { return 7; } }
            fn g(e: E) -> i32 { return 0; }
            pub fn f() -> i32 { return g(E { }); }
        "#;
        assert!(
            a045_positions(source).contains(&"a struct literal"),
            "a literal argument must fire A045 in addition to A012"
        );
    }

    /// This shape compiled silently before A045: the array slot had a zero
    /// element size, so no frame region was ever written.
    #[test]
    fn a045_struct_literal_as_array_literal_element_rejected() {
        let source = r#"
            struct E { }
            pub fn f() -> i32 { let a: [E; 3] = [E { }, E { }, E { }]; return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"a struct literal"),
            "each array-literal element literal must fire A045"
        );
        assert!(
            !compiles(source),
            "an array of field-less structs must no longer compile"
        );
    }

    #[test]
    fn a045_struct_literal_as_struct_literal_field_value_rejected() {
        let source = r#"
            struct E { }
            struct W { e: E; }
            pub fn f() -> i32 { let w: W = W { e: E { } }; return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"a struct literal"),
            "a nested field-value literal must fire A045"
        );
    }

    /// A015 also rejects a compound literal in statement position; both fire.
    #[test]
    fn a045_struct_literal_in_statement_position_rejected() {
        let source = "struct E { } pub fn f() -> i32 { E { }; return 0; }";
        assert!(
            a045_positions(source).contains(&"a struct literal"),
            "a bare literal statement must fire A045"
        );
    }

    /// A011 keys on no-fields-AND-no-methods, so it is silent for a methods-only
    /// struct. A045 is not: the struct is still field-less.
    #[test]
    fn a045_struct_literal_of_methods_only_struct_rejected() {
        let source = r#"
            struct E { fn tag(self) -> i32 { return 7; } }
            pub fn f() -> i32 { let e: E = E { }; return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"a struct literal"),
            "a methods-only field-less struct literal must fire A045"
        );
        assert!(
            !has_warning(source, "A011"),
            "A011 must stay silent for a struct that declares methods"
        );
    }

    // ---------------------------------------------------------------------
    // Fires: `let` and function-local `const` bindings
    // ---------------------------------------------------------------------

    #[test]
    fn a045_let_binding_typed_fieldless_rejected() {
        let source = r#"
            struct E { fn tag(self) -> i32 { return 7; } }
            pub fn f(other: E) -> i32 { let e: E = other; return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"the declared type of a variable"),
            "a field-less-typed `let` must fire A045 in its own right"
        );
    }

    #[test]
    fn a045_mut_let_binding_typed_fieldless_rejected() {
        let source = r#"
            struct E { fn tag(self) -> i32 { return 7; } }
            pub fn f(other: E) -> i32 { let mut e: E = other; return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"the declared type of a variable"),
            "a `let mut` binding is checked the same as an immutable one"
        );
    }

    #[test]
    fn a045_local_const_typed_fieldless_rejected() {
        let source = "struct E { } pub fn f() -> i32 { const C: E = E { }; return 0; }";
        assert!(
            a045_positions(source).contains(&"the declared type of a variable"),
            "a function-local `const` is the twin of a `let`"
        );
    }

    #[test]
    fn a045_let_binding_typed_array_of_fieldless_rejected() {
        let source = r#"
            struct E { }
            pub fn f() -> i32 { let a: [E; 3] = [E { }, E { }, E { }]; return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"the declared type of a variable"),
            "`[E; 3]` is zero-sized because its element type is"
        );
    }

    /// Proves the predicate recurses past one array layer.
    #[test]
    fn a045_let_binding_typed_nested_array_of_fieldless_rejected() {
        let source = r#"
            struct E { }
            pub fn f(a: [[E; 2]; 3]) -> i32 { let b: [[E; 2]; 3] = a; return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"the declared type of a variable"),
            "`[[E; 2]; 3]` must be looked through at depth 2"
        );
    }

    /// A struct-typed `@` is only reachable as a `let` initializer (A008, A023,
    /// A038, A039 and A040 reject the other positions), so the binding check is
    /// what covers uzumaki. This shape compiled silently before A045.
    #[test]
    fn a045_uzumaki_draw_typed_fieldless_in_spec_rejected() {
        let source = r#"
            struct E { }
            pub fn main() -> i32 { return 0; }
            spec S {
                fn c() -> i32 {
                    let r: i32 = 0;
                    forall {
                        let e: E = @;
                    }
                    return r;
                }
            }
        "#;
        assert!(
            a045_positions(source).contains(&"the declared type of a variable"),
            "a `@` drawn at a field-less struct type must be rejected at its binding"
        );
        assert!(
            !compiles(source),
            "a field-less struct draw must no longer compile"
        );
    }

    /// No separate `@` check exists, so the binding check reports exactly once.
    #[test]
    fn a045_uzumaki_draw_yields_exactly_one_diagnostic() {
        let source = r#"
            struct E { }
            pub fn main() -> i32 { return 0; }
            spec S {
                fn c() -> i32 {
                    let r: i32 = 0;
                    forall {
                        let e: E = @;
                    }
                    return r;
                }
            }
        "#;
        assert_eq!(
            count_a045(source),
            1,
            "the binding is the only offending position for a `@` draw"
        );
    }

    // ---------------------------------------------------------------------
    // Fires: module-scope `const`
    // ---------------------------------------------------------------------

    /// A module-scope `const` offends at both of the positions its function-local
    /// twin does: the annotation and the initializer literal.
    ///
    /// A032 rejects *every* top-level `const` today as not yet implemented, which
    /// is the only reason this shape cannot reach codegen. That is a gate on an
    /// unimplemented feature, not part of A045's closure — were A045 to lean on
    /// it, lifting A032 would reopen a direct route to the struct-literal lowering
    /// abort this rule exists to close.
    #[test]
    fn a045_top_level_const_typed_fieldless_rejected() {
        let source = r#"
            struct E { }
            const X: E = E { };
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a045_positions(source),
            vec!["the declared type of a variable", "a struct literal"],
            "a module-scope `const` must be reported at its annotation and its literal"
        );
    }

    /// Pins that A045 reports the shape in its own right rather than resting on
    /// A032's temporary rejection of top-level `const`.
    #[test]
    fn a045_and_a032_both_fire_for_a_fieldless_top_level_const() {
        let source = r#"
            struct E { }
            const X: E = E { };
            pub fn main() -> i32 { return 0; }
        "#;
        let errors = analyze(source).expect_err("expected analysis errors but got Ok");
        assert!(
            errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::TopLevelConstNotSupported { .. })),
            "A032 must still reject the top-level `const`"
        );
        assert!(
            errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::FieldLessStructValue { .. })),
            "A045 must fire alongside A032, not instead of it"
        );
    }

    #[test]
    fn a045_top_level_const_typed_array_of_fieldless_rejected() {
        let source = r#"
            struct E { }
            const A: [E; 2] = [E { }, E { }];
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"the declared type of a variable"),
            "`[E; 2]` is zero-sized because its element type is"
        );
    }

    /// The `const` arm must key on the struct being field-less, not on the
    /// declaration being top-level: A032 rejects this one, A045 must not.
    #[test]
    fn a045_top_level_const_of_struct_with_fields_accepted() {
        let source = r#"
            struct P { x: i32; }
            const P0: P = P { x: 1 };
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            count_a045(source),
            0,
            "a struct with a field is never zero-sized"
        );
        let errors = analyze(source).expect_err("A032 still rejects the top-level `const`");
        assert!(
            errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::TopLevelConstNotSupported { .. })),
            "the top-level `const` must still be rejected, by A032 alone"
        );
    }

    /// The definition pass recurses through `Def::Spec`, so a `const` declared
    /// inside one is reported at its annotation. Only at its annotation: the type
    /// checker types top-level `const` initializers only, so a spec-scope one
    /// carries no recorded type for the literal check to read. The annotation is
    /// the load-bearing half — every `const` of the type is caught there — so the
    /// closure holds regardless.
    #[test]
    fn a045_spec_scope_const_typed_fieldless_rejected_at_its_annotation() {
        let source = r#"
            struct E { }
            pub fn main() -> i32 { return 0; }
            spec S { const C: E = E { }; fn c() -> i32 { return 0; } }
        "#;
        assert_eq!(
            a045_positions(source),
            vec!["the declared type of a variable"],
            "a spec-scope `const` must be reported at its annotation"
        );
    }

    // ---------------------------------------------------------------------
    // Fires: parameters
    // ---------------------------------------------------------------------

    #[test]
    fn a045_named_parameter_typed_fieldless_rejected() {
        let source = r#"
            struct E { }
            fn g(e: E) -> i32 { return 0; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"the type of a parameter"),
            "a field-less-typed parameter must fire A045"
        );
    }

    #[test]
    fn a045_mut_parameter_typed_fieldless_rejected() {
        let source = "struct E { } pub fn take(mut e: E) -> i32 { return 0; }";
        assert!(
            a045_positions(source).contains(&"the type of a parameter"),
            "a `mut` parameter is checked the same as an immutable one"
        );
    }

    /// The `_:` parameter form is independently unimplemented in codegen; A045
    /// fires first and explains the actual problem.
    #[test]
    fn a045_ignored_parameter_typed_fieldless_rejected() {
        let source = "struct E { } pub fn f(_: E) -> i32 { return 0; }";
        assert!(
            a045_positions(source).contains(&"the type of a parameter"),
            "an ignored parameter still declares a type"
        );
    }

    /// A bare-type parameter (`fn g(E)`) is the third form a parameter can take
    /// alongside `e: E` and `_: E`: it names no binding but still declares a
    /// type, so it is checked the same way.
    #[test]
    fn a045_bare_type_parameter_typed_fieldless_rejected() {
        let source = r#"
            struct E { }
            fn g(E) -> i32 { return 0; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"the type of a parameter"),
            "a bare-type parameter must fire A045"
        );
    }

    #[test]
    fn a045_array_of_fieldless_parameter_rejected() {
        let source = "struct E { } pub fn f(a: [E; 3]) -> i32 { return 0; }";
        assert!(
            a045_positions(source).contains(&"the type of a parameter"),
            "an array-of-field-less parameter must fire A045"
        );
    }

    /// Method signatures are walked, not only free-function ones.
    #[test]
    fn a045_method_parameter_typed_fieldless_rejected() {
        let source = r#"
            struct E { }
            struct P { x: i32; fn use_it(self, e: E) -> i32 { return self.x; } }
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"the type of a parameter"),
            "a method parameter must fire A045"
        );
    }

    /// The definition pass recurses through `Def::Spec`.
    #[test]
    fn a045_spec_function_parameter_typed_fieldless_rejected() {
        let source = r#"
            struct E { }
            pub fn main() -> i32 { return 0; }
            spec S { fn c(e: E) -> i32 { return 0; } }
        "#;
        assert!(
            a045_positions(source).contains(&"the type of a parameter"),
            "a spec-inner function parameter must fire A045"
        );
    }

    // ---------------------------------------------------------------------
    // Fires: return types
    // ---------------------------------------------------------------------

    #[test]
    fn a045_function_return_type_fieldless_rejected() {
        let source = r#"
            struct E { }
            fn make(e: E) -> E { return e; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"the return type of a function"),
            "a field-less return type must fire A045"
        );
    }

    #[test]
    fn a045_method_return_type_fieldless_rejected() {
        let source = r#"
            struct E { }
            struct P { x: i32; fn make(self, e: E) -> E { return e; } }
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"the return type of a function"),
            "a method return type must fire A045"
        );
    }

    #[test]
    fn a045_return_type_array_of_fieldless_rejected() {
        let source = r#"
            struct E { }
            fn rows(a: [E; 3]) -> [E; 3] { return a; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"the return type of a function"),
            "an array-of-field-less return type must fire A045"
        );
    }

    #[test]
    fn a045_spec_function_return_type_fieldless_rejected() {
        let source = r#"
            struct E { }
            pub fn main() -> i32 { return 0; }
            spec S { fn c(e: E) -> E { return e; } }
        "#;
        assert!(
            a045_positions(source).contains(&"the return type of a function"),
            "a spec-inner return type must fire A045"
        );
    }

    // ---------------------------------------------------------------------
    // Fires: struct fields — the transitive-closure breaker
    // ---------------------------------------------------------------------

    #[test]
    fn a045_struct_field_typed_fieldless_rejected() {
        let source = r#"
            struct E { }
            struct W { e: E; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"the type of a struct field"),
            "a field-less-typed struct field must fire A045"
        );
    }

    #[test]
    fn a045_struct_field_typed_array_of_fieldless_rejected() {
        let source = r#"
            struct E { }
            struct W { a: [E; 3]; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"the type of a struct field"),
            "an array-of-field-less field must fire A045"
        );
    }

    /// A struct all of whose fields are zero-sized would itself be zero-sized, so
    /// forbidding a zero-sized *field* collapses the transitive case into the base
    /// case. `M` is caught at its own field; `V` is not caught by A045 at all,
    /// because `M` has a field and so is not field-less — yet the program is still
    /// rejected, which is the closure argument as an executable test.
    #[test]
    fn a045_transitive_chain_is_rejected_at_the_offending_field() {
        let source = r#"
            struct E { }
            struct M { e: E; }
            struct V { m: M; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            count_a045(source),
            1,
            "only `M`'s field is field-less-typed; `V`'s field `m` names a struct with a field"
        );
        assert!(
            a045_positions(source).contains(&"the type of a struct field"),
            "the one diagnostic must be the struct-field one"
        );
        assert!(!compiles(source), "the whole chain must be rejected");
    }

    // ---------------------------------------------------------------------
    // Fires: `self` receivers on a field-less struct
    // ---------------------------------------------------------------------

    #[test]
    fn a045_self_receiver_on_fieldless_struct_rejected() {
        let source = r#"
            struct E { fn tag(self) -> i32 { return 7; } }
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"the type of a `self` receiver"),
            "a `self` receiver on a field-less struct is uncallable by construction"
        );
    }

    #[test]
    fn a045_mut_self_receiver_on_fieldless_struct_rejected() {
        let source = r#"
            struct E { fn bump(mut self) -> i32 { return 1; } }
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"the type of a `self` receiver"),
            "a `mut self` receiver is checked the same as `self`"
        );
    }

    // ---------------------------------------------------------------------
    // Does not fire: the shapes the rule exists to protect
    // ---------------------------------------------------------------------

    /// THE protected shape: a field-less struct used purely as a method
    /// namespace. No A045, no A011, and it compiles end to end.
    #[test]
    fn a045_associated_function_namespace_use_accepted() {
        let source = r#"
            struct E { fn helper() -> i32 { return 3; } }
            pub fn f() -> i32 { return E::helper(); }
        "#;
        assert!(!has_a045(source), "the namespace idiom must stay legal");
        assert!(
            !has_warning(source, "A011"),
            "A011 must stay silent for a struct that declares methods"
        );
        assert!(
            try_codegen(source).is_ok(),
            "the namespace idiom must compile end to end"
        );
    }

    /// A field-less struct that declares only no-self functions is untouched even
    /// when nothing calls them.
    #[test]
    fn a045_associated_function_on_fieldless_struct_accepted() {
        let source = r#"
            struct E { fn helper() -> i32 { return 3; } }
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(
            !has_a045(source),
            "an associated function declares no value of the struct"
        );
    }

    #[test]
    fn a045_bare_fieldless_struct_declaration_accepted() {
        let source = "struct E { } pub fn main() -> i32 { return 0; }";
        assert!(
            !has_a045(source),
            "declaring a field-less struct stays legal"
        );
        assert!(
            has_warning(source, "A011"),
            "A011 still warns about a struct that declares nothing at all"
        );
        assert!(
            try_codegen(source).is_ok(),
            "a bare declaration must still compile"
        );
    }

    #[test]
    fn a045_self_receiver_on_struct_with_fields_accepted() {
        let source = r#"
            struct P { x: i32; fn get(self) -> i32 { return self.x; } }
            pub fn main() -> i32 { let p: P = P { x: 1 }; return p.get(); }
        "#;
        assert!(
            !has_a045(source),
            "a `self` receiver on a struct with fields is untouched"
        );
        assert!(try_codegen(source).is_ok(), "the shape must still compile");
    }

    /// A struct with at least one field is unaffected in every position the rule
    /// inspects: literal, `let`, `mut` parameter, return type, array element,
    /// struct field, and `self` receiver.
    #[test]
    fn a045_struct_with_fields_unaffected_in_every_position() {
        let source = r#"
            struct P { x: i32; fn get(self) -> i32 { return self.x; } }
            struct W { p: P; }
            fn make(mut q: P) -> P { let r: P = P { x: q.x }; return r; }
            pub fn main() -> i32 {
                let a: [P; 2] = [P { x: 1 }, P { x: 2 }];
                let w: W = W { p: P { x: 3 } };
                let z: P = make(w.p);
                return z.get() + a[0].x;
            }
        "#;
        assert_eq!(
            count_a045(source),
            0,
            "a struct with a field is never zero-sized"
        );
        assert!(try_codegen(source).is_ok(), "the program must compile");
    }

    /// Guards against an implementation that keys on "small" rather than
    /// "field-less": a one-byte struct is fully usable.
    #[test]
    fn a045_one_byte_struct_accepted() {
        let source = r#"
            struct B { b: bool; }
            struct H { inner: B; }
            fn pick(mut v: B) -> B { let r: B = B { b: v.b }; return r; }
            pub fn main() -> i32 {
                let h: H = H { inner: B { b: true } };
                let c: B = pick(h.inner);
                if c.b { return 1; }
                return 0;
            }
        "#;
        assert_eq!(count_a045(source), 0, "a one-byte struct is not zero-sized");
        assert!(try_codegen(source).is_ok(), "the program must compile");
    }

    /// An enum lowers to a 4-byte tag regardless of variant count, so even a
    /// variantless enum is never zero-sized. A009 warns about it; A045 must not.
    #[test]
    fn a045_enum_positions_accepted() {
        let source = r#"
            enum X { }
            enum Color { Red, Green }
            struct H { c: Color; }
            fn pick(x: X, c: Color) -> Color { return c; }
            pub fn main() -> i32 { let d: Color = Color::Red; return 0; }
        "#;
        assert_eq!(count_a045(source), 0, "an enum is never zero-sized");
        assert!(
            has_warning(source, "A009"),
            "A009 still warns about the variantless enum"
        );
        assert!(try_codegen(source).is_ok(), "the program must compile");
    }

    #[test]
    fn a045_scalar_and_scalar_array_positions_accepted() {
        let source = r#"
            struct S { v: i32; }
            fn scale(mut a: [i32; 4], k: i32) -> i32 { return a[0] * k; }
            pub fn main() -> i32 {
                let a: [i32; 4] = [1, 2, 3, 4];
                let b: bool = true;
                let s: S = S { v: 7 };
                if b { return scale(a, s.v); }
                return 0;
            }
        "#;
        assert_eq!(count_a045(source), 0, "scalars are never zero-sized");
        assert!(try_codegen(source).is_ok(), "the program must compile");
    }

    /// Documented non-route: type aliases are non-transparent in Inference, so an
    /// alias naming a field-less struct is a dead end rather than a way to reach a
    /// value of it. (Local `type` statements are independently unimplemented in
    /// codegen, so only the analysis verdict is asserted here.)
    #[test]
    fn a045_local_type_alias_to_fieldless_not_flagged() {
        let source = "struct E { } pub fn f() -> i32 { type X = E; return 0; }";
        assert!(
            !has_a045(source),
            "an alias declaration introduces no value of the aliased struct"
        );
    }

    #[test]
    fn a045_fieldless_struct_declared_inside_spec_declaration_only_accepted() {
        let source = r#"
            pub fn main() -> i32 { return 0; }
            spec S { struct E { } fn c() -> i32 { return 0; } }
        "#;
        assert!(
            !has_a045(source),
            "a declaration-only field-less struct inside a spec is legal"
        );
    }

    // ---------------------------------------------------------------------
    // `external fn` signatures
    // ---------------------------------------------------------------------

    /// An `external fn` is never callable (A024 rejects every call), so this is
    /// not load-bearing for the closure — it is rejected because the emitted
    /// import would take a pointer to a zero-byte region, an ABI surface nobody
    /// chose. The declaration compiled silently before A045.
    #[test]
    fn a045_external_fn_parameter_typed_fieldless_rejected() {
        let source = r#"
            struct E { }
            external fn ext(e: E) -> i32;
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"the type of a parameter"),
            "an `external fn` parameter must fire A045"
        );
        assert!(!compiles(source), "the declaration must no longer compile");
    }

    /// The bare-type parameter form is how an `external fn` usually spells its
    /// signature, since an imported function's parameters need no local names.
    #[test]
    fn a045_external_fn_bare_type_parameter_rejected() {
        let source = r#"
            struct E { }
            external fn ext(E) -> i32;
            pub fn f() -> i32 { return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"the type of a parameter"),
            "an `external fn` bare-type parameter must fire A045"
        );
        assert!(!compiles(source), "the declaration must no longer compile");
    }

    #[test]
    fn a045_external_fn_return_type_fieldless_rejected() {
        let source = r#"
            struct E { }
            external fn ext(x: i32) -> E;
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(
            a045_positions(source).contains(&"the return type of a function"),
            "an `external fn` return type must fire A045"
        );
        assert!(!compiles(source), "the declaration must no longer compile");
    }

    // ---------------------------------------------------------------------
    // Rule interactions and per-position reporting
    // ---------------------------------------------------------------------

    /// Two independent offending positions yield exactly two diagnostics.
    #[test]
    fn a045_two_bad_positions_yield_two_diagnostics() {
        let source = r#"
            struct E { }
            fn g(e: E) -> i32 { return 0; }
            fn h(e: E) -> i32 { return 0; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            count_a045(source),
            2,
            "each offending position is reported once"
        );
    }

    /// A011 and A045 have disjoint subjects and neither suppresses the other.
    #[test]
    fn a045_and_a011_both_fire_for_a_bare_empty_struct_given_a_value() {
        let source = r#"
            struct E { }
            fn f(e: E) -> i32 { return 0; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(has_a045(source), "A045 must fire on the parameter");
        assert!(
            has_warning(source, "A011"),
            "A011 must still report the empty declaration"
        );
    }

    /// Pins that A011 was not widened to cover the methods-only case.
    #[test]
    fn a045_does_not_change_a011_for_a_methods_only_struct() {
        let source = r#"
            struct E { fn helper() -> i32 { return 1; } }
            pub fn f() -> i32 { return E::helper(); }
        "#;
        assert!(!has_warning(source, "A011"), "A011 must stay silent");
        assert!(!has_a045(source), "A045 must stay silent");
    }

    /// Deliberate: no cross-rule suppression machinery exists, so both A012 and
    /// A045 report the same literal argument.
    #[test]
    fn a045_and_a012_both_fire_for_a_literal_argument() {
        let source = r#"
            struct E { fn tag(self) -> i32 { return 7; } }
            fn g(e: E) -> i32 { return 0; }
            pub fn f() -> i32 { return g(E { }); }
        "#;
        let errors = analyze(source).expect_err("expected analysis errors but got Ok");
        assert!(
            errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralAsArgument { .. })),
            "A012 must still fire"
        );
        assert!(
            errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::FieldLessStructValue { .. })),
            "A045 must fire alongside A012"
        );
    }

    /// Diagnostic quality through the real pipeline: the finding names the struct
    /// and the position, explains the mechanism and the verification consequence,
    /// reassures that declarations stay legal, gives both fixes, and reports A045.
    #[test]
    fn a045_diagnostic_quality() {
        let diag = a045_diag("struct E { } pub fn take(mut e: E) -> i32 { return 0; }");
        assert!(
            matches!(
                &diag,
                AnalysisDiagnostic::FieldLessStructValue { name, position, .. }
                    if name == "E" && *position == "the type of a parameter"
            ),
            "expected A045 naming `E` at the parameter position, got: {diag}"
        );
        let msg = diag.to_string();
        for fragment in [
            "`E` is a struct with no fields",
            "no value representation",
            "cannot be used as the type of a parameter",
            "zero bytes",
            "a proof has nothing to describe",
            "declaring a field-less struct stays legal",
            "at least one field",
            "without `self`",
            "`E::function_name()`",
        ] {
            assert!(
                msg.contains(fragment),
                "A045 message must contain `{fragment}`, got: {msg}"
            );
        }
        assert_eq!(diag.rule_id(), "A045");
    }

    // ---------------------------------------------------------------------
    // Multi-file: canonical-key identity across module boundaries
    // ---------------------------------------------------------------------

    /// Type-checks a multi-file program (entry first, empty module path) and runs
    /// the analysis pass, returning its result.
    fn analyze_multi(files: &[(Vec<&str>, &str)]) -> Result<AnalysisResult, AnalysisErrors> {
        let ctx = try_type_check_multi_file(files)
            .expect("multi-file type checking should succeed for analysis test input");
        inference_analysis::analyze(&ctx)
    }

    fn a045_multi_diags(files: &[(Vec<&str>, &str)]) -> Vec<AnalysisDiagnostic> {
        match analyze_multi(files) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::FieldLessStructValue { .. }))
                .cloned()
                .collect(),
        }
    }

    fn has_a045_multi(files: &[(Vec<&str>, &str)]) -> bool {
        !a045_multi_diags(files).is_empty()
    }

    /// The multi-file twin of [`a045_positions`].
    fn a045_multi_positions(files: &[(Vec<&str>, &str)]) -> Vec<&'static str> {
        a045_multi_diags(files)
            .iter()
            .filter_map(|d| match d {
                AnalysisDiagnostic::FieldLessStructValue { position, .. } => Some(*position),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a045_fieldless_struct_defined_in_another_file_literal_rejected() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::{E};
                    pub fn main() -> i32 { let e: E = E { }; return 0; }
                "#,
            ),
            (vec!["lib"], "pub struct E { }"),
        ];
        assert!(
            has_a045_multi(files),
            "a literal of an imported field-less struct must be rejected"
        );
    }

    #[test]
    fn a045_fieldless_struct_defined_in_another_file_parameter_rejected() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::{E};
                    pub fn f(mut e: E) -> i32 { return 0; }
                    pub fn main() -> i32 { return 0; }
                "#,
            ),
            (vec!["lib"], "pub struct E { }"),
        ];
        assert!(
            has_a045_multi(files),
            "a parameter typed by an imported field-less struct must be rejected"
        );
    }

    /// Exercises the `::`-qualified annotation carrier, which reaches the rule
    /// unresolved and must be resolved against the referencing file.
    #[test]
    fn a045_qualified_path_to_cross_file_fieldless_rejected() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib;
                    pub fn f(e: lib::E) -> i32 { return 0; }
                    pub fn main() -> i32 { return 0; }
                "#,
            ),
            (vec!["lib"], "pub struct E { }"),
        ];
        assert!(
            has_a045_multi(files),
            "a `::`-qualified field-less parameter type must be rejected"
        );
    }

    /// A `::`-qualified annotation reached through a binding rather than a
    /// signature. The binding position is reported in its own right, alongside
    /// the initializing literal. Unlike a signature annotation, which the rule
    /// reads raw, a binding's type arrives already resolved by the type checker,
    /// so this pins the binding path across a module boundary rather than the
    /// qualified carrier itself (which
    /// `a045_qualified_path_to_cross_file_fieldless_rejected` covers).
    #[test]
    fn a045_let_binding_from_qualified_annotation_rejected() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib;
                    pub fn main() -> i32 { let e: lib::E = lib::E { }; return 0; }
                "#,
            ),
            (vec!["lib"], "pub struct E { }"),
        ];
        let positions = a045_multi_positions(files);
        assert!(
            positions.contains(&"the declared type of a variable"),
            "a `::`-qualified binding annotation must be rejected in its own right, got: {positions:?}"
        );
    }

    /// A module-scope `const` whose annotation is a `::`-qualified path into
    /// another file. The qualified carrier reaches the rule unresolved and is
    /// resolved against the referencing file, while the initializer literal
    /// arrives already resolved to the imported struct's canonical key.
    #[test]
    fn a045_top_level_const_typed_cross_file_fieldless_rejected() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib;
                    const X: lib::E = lib::E { };
                    pub fn main() -> i32 { return 0; }
                "#,
            ),
            (vec!["lib"], "pub struct E { }"),
        ];
        let positions = a045_multi_positions(files);
        assert!(
            positions.contains(&"the declared type of a variable"),
            "a cross-file `const` annotation must be rejected, got: {positions:?}"
        );
        assert!(
            positions.contains(&"a struct literal"),
            "the cross-file `const` initializer literal must be rejected, got: {positions:?}"
        );
    }

    #[test]
    fn a045_struct_field_typed_cross_file_fieldless_rejected() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib;
                    struct W { e: lib::E; }
                    pub fn main() -> i32 { return 0; }
                "#,
            ),
            (vec!["lib"], "pub struct E { }"),
        ];
        assert!(
            has_a045_multi(files),
            "a field typed by a cross-file field-less struct must be rejected"
        );
    }

    /// The canonical-key discipline test. Two files each declare `E`; only the
    /// field-less one may be flagged. A bare-name lookup would flag the wrong
    /// parameter. This whole program compiled silently before A045.
    #[test]
    fn a045_same_named_struct_in_two_files_only_the_fieldless_one_flagged() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib;
                    struct E { x: i32; }
                    pub fn f(a: E, b: lib::E) -> i32 { return a.x; }
                    pub fn main() -> i32 { return 0; }
                "#,
            ),
            (vec!["lib"], "pub struct E { }"),
        ];
        let diags = a045_multi_diags(files);
        assert_eq!(
            diags.len(),
            1,
            "only the `lib::E` parameter is field-less, got: {diags:?}"
        );
        assert!(
            matches!(
                &diags[0],
                AnalysisDiagnostic::FieldLessStructValue { position, .. }
                    if *position == "the type of a parameter"
            ),
            "the single finding must be the parameter one, got: {:?}",
            diags[0]
        );
    }

    /// A finding in an imported file must carry that file's module path so the
    /// rendered report names it, rather than reading as an entry-file location.
    #[test]
    fn a045_finding_in_an_imported_file_is_labeled_with_that_file() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib;
                    pub fn main() -> i32 { return 0; }
                "#,
            ),
            (
                vec!["lib"],
                r#"
                    pub struct E { }
                    pub struct W { e: E; }
                "#,
            ),
        ];
        let rendered = analyze_multi(files)
            .expect_err("expected analysis errors but got Ok")
            .to_string();
        assert!(
            rendered.contains("lib:"),
            "the finding must name the file it belongs to, got: {rendered}"
        );
        assert!(
            rendered.contains("error[A045]"),
            "the rendered report must contain the A045 finding, got: {rendered}"
        );
    }

    /// A `pub` field-less struct used purely as a namespace across a module
    /// boundary stays legal.
    #[test]
    fn a045_cross_file_namespace_use_accepted() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib;
                    pub fn main() -> i32 { return lib::E::helper(); }
                "#,
            ),
            (
                vec!["lib"],
                "pub struct E { pub fn helper() -> i32 { return 3; } }",
            ),
        ];
        assert!(
            !has_a045_multi(files),
            "the cross-file namespace idiom must stay legal"
        );
    }
}
