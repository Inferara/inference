/// Integration tests for analysis rule A050.
///
/// - A050: `UnnamedParameter` — a parameter of a function with a body must be
///   written with a name or with `_`. A bare positional type binds nothing, can
///   be labelled by nothing, and says strictly less than `_: T` while occupying
///   the same slot. `external fn` keeps the form, because an extern declares an
///   ABI signature with no body to read a parameter in.
///
/// Unlike the two rules beside it, this one is not a gate on an unimplemented
/// feature: `_: T` is supported, and the bare form is rejected because one
/// spelling for the concept is better than two. The controls are what pin that
/// distinction.
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::{build_ast, try_codegen_no_analysis};
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

    /// Returns true if any analysis error is an `UnnamedParameter` (A050).
    fn has_a050(source: &str) -> bool {
        match analyze(source) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::UnnamedParameter { .. })),
        }
    }

    /// Counts how many `UnnamedParameter` (A050) diagnostics the analysis
    /// emits, filtering by variant so unrelated rules do not perturb the count.
    fn count_a050(source: &str) -> usize {
        match analyze(source) {
            Ok(_) => 0,
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::UnnamedParameter { .. }))
                .count(),
        }
    }

    /// Collects `(function, index, ty)` from every A050 diagnostic, in report
    /// order, so a test can pin exactly which parameters fired and how each was
    /// described.
    fn a050_findings(source: &str) -> Vec<(String, usize, String)> {
        match analyze(source) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .errors()
                .iter()
                .filter_map(|e| match e {
                    AnalysisDiagnostic::UnnamedParameter {
                        function, index, ty, ..
                    } => Some((function.clone(), *index, ty.clone())),
                    _ => None,
                })
                .collect(),
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

    /// Before this rule the form reached the `ArgKind::TypeOnly` arm of the
    /// parameter loop in `visit_function_definition_body`
    /// (`core/wasm-codegen/src/compiler.rs`) and aborted it.
    #[test]
    fn a050_single_bare_parameter() {
        let source = r#"
            pub fn f(i32) -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a050_findings(source),
            vec![("f".to_string(), 0, "i32".to_string())]
        );
        assert!(
            !compiles(source),
            "a bare positional parameter must never reach code generation"
        );
    }

    /// The index counts written arguments from zero, so a bare parameter after a
    /// named one is parameter 1.
    #[test]
    fn a050_bare_parameter_after_a_named_one() {
        let source = r#"
            pub fn f(a: i32, i32) -> i32 { return a; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a050_findings(source),
            vec![("f".to_string(), 1, "i32".to_string())]
        );
    }

    /// Two offending parameters are two findings, in declaration order, each
    /// naming its own type.
    #[test]
    fn a050_two_bare_parameters_report_separately() {
        let source = r#"
            pub fn f(i32, i64) -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a050_findings(source),
            vec![
                ("f".to_string(), 0, "i32".to_string()),
                ("f".to_string(), 1, "i64".to_string()),
            ]
        );
    }

    /// An array type is rendered as the source spells it, brackets and length
    /// included, because the message's fix quotes it back verbatim.
    ///
    /// The `bool` case is the one that matters: the element's builtin name has
    /// to survive the rebuild, or the message would recommend `_: [Bool; 2]`,
    /// which is not a type anyone can write.
    #[test]
    fn a050_array_type_is_rendered_as_written() {
        let numeric = r#"
            pub fn f([i32; 2]) -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a050_findings(numeric),
            vec![("f".to_string(), 0, "[i32; 2]".to_string())]
        );

        let boolean = r#"
            pub fn f([bool; 2]) -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a050_findings(boolean),
            vec![("f".to_string(), 0, "[bool; 2]".to_string())]
        );

        let nested = r#"
            pub fn f([[bool; 2]; 3]) -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a050_findings(nested),
            vec![("f".to_string(), 0, "[[bool; 2]; 3]".to_string())],
            "the element name must survive at every depth"
        );
    }

    /// A named type is rendered by its canonical key, which is the bare name in
    /// a single-file program.
    #[test]
    fn a050_struct_type_is_rendered_by_name() {
        let source = r#"
            struct Q { x: i32; }
            pub fn f(Q) -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a050_findings(source),
            vec![("f".to_string(), 0, "Q".to_string())]
        );
    }

    /// A `self` receiver is not counted, so a method's first written parameter
    /// is index 0. That is the number the type checker already uses when it
    /// talks about an argument of this method, because the parameter lists its
    /// messages index into are built with the receiver filtered out, in the
    /// signature the symbol table records for the method; two user-facing
    /// messages about one slot must not disagree about which slot it is. The
    /// function is named the way a call spells it.
    #[test]
    fn a050_method_excludes_the_receiver_and_is_named_by_its_struct() {
        let source = r#"
            struct P { x: i32; fn m(self, i32) -> i32 { return 1; } }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a050_findings(source),
            vec![("P::m".to_string(), 0, "i32".to_string())]
        );
        assert!(
            has_warning(source, "A010"),
            "the unrelated A010 warning on this method must be unaffected"
        );
    }

    /// A spec function is a defined function like any other. It is named by its
    /// own name: a `spec` is a namespace of definitions, not a receiver, so
    /// there is nothing to qualify it with the way a method is qualified.
    #[test]
    fn a050_spec_function_is_covered() {
        let source = r#"
            pub fn main() -> i32 { return 0; }
            spec S { fn c(i32) -> i32 { return 1; } }
        "#;
        assert_eq!(
            a050_findings(source),
            vec![("c".to_string(), 0, "i32".to_string())]
        );
    }

    // ---------------------------------------------------------------------
    // Controls
    // ---------------------------------------------------------------------

    /// Documented non-scope. An extern declares an ABI signature and has no body
    /// to read a parameter in, so a positional type is a complete statement of
    /// it — and it is the spelling the corpus uses.
    #[test]
    fn a050_external_function_keeps_the_bare_form() {
        let source = r#"
            external fn e(i32, i32) -> i32;
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a050(source), 0);
        assert!(
            compiles(source),
            "an extern with bare positional parameters must still compile"
        );
    }

    /// `_: T` is the spelling this rule leaves standing: a deliberate
    /// declaration that the parameter exists, its type is part of the signature,
    /// and the body does not read it.
    #[test]
    fn a050_ignored_parameter_is_legal() {
        let source = r#"
            pub fn f(_: i32) -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a050(source), 0);
        assert!(
            compiles(source),
            "`_: T` is the spelling this rule leaves standing, so it must compile"
        );
    }

    /// Repeats of `_` stay legal: the type checker deliberately does not treat
    /// them as a duplicate name, because `_` binds nothing to collide.
    #[test]
    fn a050_repeated_ignored_parameters_are_legal() {
        let source = r#"
            pub fn f(_: i32, _: i32) -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a050(source), 0);
        assert!(
            compiles(source),
            "`_: T` is the spelling this rule leaves standing, so it must compile"
        );
    }

    #[test]
    fn a050_named_and_ignored_parameters_mix() {
        let source = r#"
            pub fn f(a: i32, _: i32) -> i32 { return a; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a050(source), 0);
        assert!(
            compiles(source),
            "`_: T` is the spelling this rule leaves standing, so it must compile"
        );
    }

    /// A `self` receiver is not an unnamed parameter; it is a parameter that
    /// spells its type implicitly.
    #[test]
    fn a050_self_receiver_is_not_an_unnamed_parameter() {
        let source = r#"
            struct P { x: i32; fn m(self) -> i32 { return self.x; } }
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(!has_a050(source));
        assert!(compiles(source));
    }

    /// The boundary this rule does not own. `fn f(x)` is not a parameter named
    /// `x` — the grammar's fallback arm reads it as a parameter whose *type* is
    /// `x` — and the type checker rejects the unknown type before analysis runs,
    /// so the shape never reaches A050. Constructed from the arena directly,
    /// because the shared helper expects type checking to succeed.
    #[test]
    fn a050_forgotten_type_annotation_is_a_type_error_first() {
        let arena = build_ast("pub fn f(x) -> i32 { return 1; }".to_string());
        let result = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena);
        assert!(
            result.is_err(),
            "a bare positional parameter naming an unknown type must fail type checking"
        );
    }
}
