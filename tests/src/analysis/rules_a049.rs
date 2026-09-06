/// Integration tests for analysis rule A049.
///
/// - A049: `UnitAsValue` — the unit type is the language's way of saying *there
///   is nothing here*, and it is legitimate in exactly that role. What has no
///   implementation is unit in a carrier: a parameter declared `()` is given no
///   argument slot, a binding of it has nothing to store, an array of it has no
///   element size, and a struct field of it has no meaningful offset. The rule
///   rejects those positions and the unit literal that fills them, while leaving
///   every void function and every way of returning from one alone.
///
/// The controls are the load-bearing half of this file. `return;` is spelled
/// with a parser-synthesized unit literal, so a rule that rejected the literal
/// unconditionally would reject every void function in the language — and the
/// exempt statement forms are what stop that. Each of them is asserted here.
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::{build_ast, try_build_ast, try_codegen_no_analysis};
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

    /// Returns true if any analysis error is a `UnitAsValue` (A049). Filters by
    /// variant rather than asserting a total error count, since the surface may
    /// also trip unrelated rules (or warnings).
    fn has_a049(source: &str) -> bool {
        match analyze(source) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UnitAsValue { .. })),
        }
    }

    /// Counts how many `UnitAsValue` (A049) diagnostics the analysis emits,
    /// filtering by variant so unrelated rules do not perturb the count.
    fn count_a049(source: &str) -> usize {
        match analyze(source) {
            Ok(_) => 0,
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::UnitAsValue { .. }))
                .count(),
        }
    }

    /// Collects the `position` string of every A049 diagnostic, in report order,
    /// so a test can pin exactly which positions fired.
    fn a049_positions(source: &str) -> Vec<&'static str> {
        match analyze(source) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .errors()
                .iter()
                .filter_map(|e| match e {
                    AnalysisDiagnostic::UnitAsValue { position, .. } => Some(*position),
                    _ => None,
                })
                .collect(),
        }
    }

    /// Whether any analysis *error* with the given rule id was produced, used to
    /// assert that a neighbouring rule still fires beside A049.
    fn has_error(source: &str, rule_id: &str) -> bool {
        match analyze(source) {
            Ok(_) => false,
            Err(errors) => errors.errors().iter().any(|e| e.rule_id() == rule_id),
        }
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
    /// generation. The two phases stay split so a test can assert "this never
    /// reaches code generation" as a fact about code generation alone — a single
    /// combined helper would report the analysis verdict for a rejected program
    /// and say nothing about what the backend would have done with it.
    fn compiles(source: &str) -> bool {
        analyze(source).is_ok() && try_codegen_no_analysis(source).is_ok()
    }

    // ---------------------------------------------------------------------
    // Fires
    // ---------------------------------------------------------------------

    /// A unit binding offends twice: the annotation declares storage for a value
    /// that occupies no bytes, and the initializer produces one. Before this
    /// rule the literal aborted the `Expr::UnitLiteral` arm of
    /// `lower_expression` in `core/wasm-codegen/src/compiler.rs`, before the
    /// binding could reach a slot that was never allocated for it.
    #[test]
    fn a049_unit_binding_reports_the_type_and_the_literal() {
        let source = r#"
            pub fn f() -> i32 { let u: () = (); return 0; }
        "#;
        assert_eq!(
            a049_positions(source),
            vec!["the declared type of a variable", "a value"]
        );
        assert!(
            !compiles(source),
            "a unit binding must never reach code generation"
        );
    }

    /// `unit` is a builtin name for the same type, so the alias spelling is not
    /// a way around the rule. The message names `()` because that is the
    /// canonical spelling; this test is what pins the other one.
    #[test]
    fn a049_unit_keyword_spelling_is_the_same_type() {
        let source = r#"
            pub fn f() -> i32 { let u: unit = (); return 0; }
        "#;
        assert_eq!(
            a049_positions(source),
            vec!["the declared type of a variable", "a value"]
        );
    }

    /// Before this rule a unit parameter aborted the parameter loop of
    /// `visit_function_definition_body` in
    /// `core/wasm-codegen/src/compiler.rs` with `"Function parameter type must
    /// not be unit"`.
    #[test]
    fn a049_named_unit_parameter() {
        let source = r#"
            pub fn f(u: ()) -> i32 { return 0; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(a049_positions(source), vec!["the type of a parameter"]);
        assert!(
            !compiles(source),
            "a unit parameter must never reach code generation"
        );
    }

    #[test]
    fn a049_ignored_unit_parameter() {
        let source = r#"
            pub fn f(_: ()) -> i32 { return 0; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(a049_positions(source), vec!["the type of a parameter"]);
    }

    /// The declaration alone compiles today, because a struct nobody makes a
    /// value of is never laid out — which is exactly why the field position is
    /// worth covering at its declaration rather than at each use.
    #[test]
    fn a049_struct_field() {
        let source = r#"
            struct S { u: (); }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(a049_positions(source), vec!["the type of a struct field"]);
    }

    /// The struct shape that reaches layout. Before this rule it aborted the
    /// `element_size` in `core/wasm-codegen/src/memory.rs` with "Unsupported
    /// type for byte-size computation: Unit".
    #[test]
    fn a049_unit_field_of_an_instantiated_struct() {
        let source = r#"
            struct S { u: (); }
            pub fn f() -> i32 { let v: S = S { u: () }; return 0; }
        "#;
        assert_eq!(
            a049_positions(source),
            vec!["the type of a struct field", "a value"]
        );
        assert!(
            !compiles(source),
            "a unit field must never reach struct layout"
        );
    }

    /// An array of unit is reported once, on the annotation that carries it.
    ///
    /// This declaration compiles today: an array parameter is passed by
    /// reference, so its element size is never computed and the shape slips
    /// through into a signature nobody can call. The binding form below is the
    /// one that aborts.
    #[test]
    fn a049_array_of_unit_parameter_reports_once() {
        let source = r#"
            pub fn f(a: [(); 2]) -> i32 { return 0; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a049(source), 1, "one annotation, one finding");
        assert_eq!(a049_positions(source), vec!["the type of a parameter"]);
    }

    /// The array shape that reaches frame layout. Before this rule it aborted
    /// `element_size` in `core/wasm-codegen/src/memory.rs`.
    #[test]
    fn a049_array_of_unit_binding() {
        let source = r#"
            pub fn f() -> i32 { let a: [(); 2] = [(), ()]; return 0; }
        "#;
        assert_eq!(
            a049_positions(source),
            vec!["the declared type of a variable", "a value", "a value"]
        );
        assert!(
            !compiles(source),
            "an array of unit must never reach frame layout"
        );
    }

    #[test]
    fn a049_function_local_const() {
        let source = r#"
            pub fn f() -> i32 { const U: () = (); return 0; }
        "#;
        assert_eq!(
            a049_positions(source),
            vec!["the declared type of a variable", "a value"]
        );
    }

    /// A module-scope `const` is checked in its own right, beside A032's
    /// blanket rejection of every top-level `const`. There is no cross-rule
    /// suppression.
    #[test]
    fn a049_module_scope_const_reports_beside_a032() {
        let source = r#"
            const U: () = ();
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a049_positions(source),
            vec!["the declared type of a variable", "a value"]
        );
        assert!(
            has_error(source, "A032"),
            "A032 must still reject the top-level `const`"
        );
    }

    /// The linker already rejects this exact position when it lowers an extern
    /// signature. A049 reaches it one phase earlier and for every function, not
    /// just an extern; the link-time check stays in place as defence in depth.
    #[test]
    fn a049_external_function_parameter() {
        let source = r#"
            external fn e(());
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(a049_positions(source), vec!["the type of a parameter"]);
    }

    #[test]
    fn a049_method_parameter() {
        let source = r#"
            struct P { x: i32; fn m(self, u: ()) -> i32 { return 0; } }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(a049_positions(source), vec!["the type of a parameter"]);
        assert!(
            has_warning(source, "A010"),
            "the unrelated A010 warning on this method must be unaffected"
        );
    }

    /// A spec function is lowered to a real WebAssembly function in proof mode,
    /// so a unit carrier inside a spec body reaches the same lowering a
    /// top-level one does and aborted there. Compile mode emits no spec
    /// functions at all, so the rule is stated on source shape and reports in
    /// both modes alike.
    #[test]
    fn a049_spec_body_is_covered() {
        let source = r#"
            pub fn main() -> i32 { return 0; }
            spec S { fn c() -> i32 { let u: () = (); return 0; } }
        "#;
        assert_eq!(
            a049_positions(source),
            vec!["the declared type of a variable", "a value"]
        );
    }

    /// The exemption is stated on the *root* of an expression statement. Here
    /// the root is a call, so the walk descends and reports the unit argument
    /// that has to arrive in a slot the callee was never given.
    #[test]
    fn a049_unit_argument_in_an_expression_statement() {
        let source = r#"
            fn g(u: ()) -> i32 { return 0; }
            pub fn f() -> i32 { g(()); return 0; }
        "#;
        assert_eq!(
            a049_positions(source),
            vec!["the type of a parameter", "a value"],
            "the callee's parameter and the argument are both reported"
        );
    }

    /// The same descent through the root of a `return` statement.
    #[test]
    fn a049_unit_argument_in_a_return_statement() {
        let source = r#"
            fn g(u: ()) -> i32 { return 0; }
            pub fn f() -> i32 { return g(()); }
        "#;
        assert_eq!(
            a049_positions(source),
            vec!["the type of a parameter", "a value"]
        );
    }

    // ---------------------------------------------------------------------
    // Controls — the load-bearing half of this rule
    // ---------------------------------------------------------------------

    /// The parser synthesizes a unit *type* node for a binding with no type
    /// child, which is why the rule reads the type the checker recorded rather
    /// than the raw annotation. That placeholder is unreachable from a clean
    /// parse: `variable_definition_statement` in
    /// `core/parser/src/grammar/stmt.rs` requires `: type` on a `let`, so the
    /// un-annotated form is a parse error and never reaches analysis at all. This test pins that
    /// boundary — if the grammar ever relaxes it, the rule's binding half must
    /// be re-checked against an inferred type before this test is updated.
    #[test]
    fn a049_an_unannotated_binding_does_not_parse() {
        let err = try_build_ast("pub fn f() -> i32 { let x = 5; return x; }".to_string())
            .expect_err("an un-annotated `let` must be a parse error");
        let text = err.to_string();
        assert!(
            text.contains("expected Colon"),
            "the parse error must be the missing annotation, got: {text}"
        );
    }

    /// `return;` carries a synthesized unit literal, so this is the shape that
    /// would make an over-broad rule reject every void function ever written.
    #[test]
    fn a049_bare_return_is_exempt() {
        let source = r#"
            pub fn f() { return; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a049(source), 0);
        assert!(
            compiles(source),
            "an exempt unit form must reach a generated module"
        );
    }

    /// An explicit `()` return type is the one place unit means something, and
    /// the rule does not cover the return position at all.
    #[test]
    fn a049_explicit_unit_return_type_is_exempt() {
        let source = r#"
            pub fn f() -> () { return; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a049(source), 0);
        assert!(
            compiles(source),
            "an exempt unit form must reach a generated module"
        );
    }

    #[test]
    fn a049_unit_keyword_return_type_is_exempt() {
        let source = r#"
            pub fn f() -> unit { return; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a049(source), 0);
        assert!(
            compiles(source),
            "an exempt unit form must reach a generated module"
        );
    }

    #[test]
    fn a049_explicit_unit_return_expression_is_exempt() {
        let source = r#"
            pub fn f() { return (); }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a049(source), 0);
        assert!(
            compiles(source),
            "an exempt unit form must reach a generated module"
        );
    }

    #[test]
    fn a049_bare_unit_expression_statement_is_exempt() {
        let source = r#"
            pub fn f() { (); }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a049(source), 0);
        assert!(
            compiles(source),
            "an exempt unit form must reach a generated module"
        );
    }

    /// The exemption peels parentheses, so a redundant pair does not turn a
    /// legal statement into an error. Peeling here is deliberately more
    /// permissive than A046's decision not to peel: A046 removes a redundant
    /// spelling, while this rule must not manufacture one.
    #[test]
    fn a049_parenthesized_unit_expression_statement_is_exempt() {
        let source = r#"
            pub fn f() { (()); }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a049(source), 0);
        assert!(
            compiles(source),
            "an exempt unit form must reach a generated module"
        );
    }

    #[test]
    fn a049_parenthesized_unit_return_expression_is_exempt() {
        let source = r#"
            pub fn f() { return (()); }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a049(source), 0);
        assert!(
            compiles(source),
            "an exempt unit form must reach a generated module"
        );
    }

    /// A call to a void function as a statement produces no unit literal at
    /// all — the value it does not return is never written down.
    #[test]
    fn a049_void_call_as_a_statement_is_exempt() {
        let source = r#"
            fn v() { return; }
            pub fn f() -> i32 { v(); return 0; }
        "#;
        assert_eq!(count_a049(source), 0);
        assert!(
            compiles(source),
            "an exempt unit form must reach a generated module"
        );
    }

    #[test]
    fn a049_plain_program_is_untouched() {
        let source = r#"
            pub fn main() -> i32 { let x: i32 = 1; return x; }
        "#;
        assert!(!has_a049(source));
        assert!(compiles(source), "a program with no unit value must compile");
    }
}
