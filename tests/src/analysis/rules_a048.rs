/// Integration tests for analysis rule A048.
///
/// - A048: `StringNotSupported` — `string` (and its `String` spelling) is a
///   builtin type name the type checker accepts, but nothing after it can lower
///   a string: there is no layout in linear memory, no WebAssembly value type,
///   and no proof term. The rule rejects every position at which a string value
///   could be introduced — a string literal, the declared type of a
///   `let`/`const`, a parameter, a return type, and a struct field — looking
///   through array nesting at any depth.
///
/// These tests exercise the rule through a real parse -> type-check -> analyze
/// pipeline, complementing the in-crate message/`rule_id` unit tests in
/// `core/analysis`. Every rejected shape reached code generation before this
/// rule existed: most aborted the compiler outright, one failed with a clean
/// unsupported-type error, and a couple compiled into a signature nobody could
/// call. The tests that pin "this never reaches code generation" say which of
/// those each shape did.
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

    /// Returns true if any analysis error is a `StringNotSupported` (A048).
    /// Filters by variant rather than asserting a total error count, since the
    /// surface may also trip unrelated rules (or warnings).
    fn has_a048(source: &str) -> bool {
        match analyze(source) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::StringNotSupported { .. })),
        }
    }

    /// Counts how many `StringNotSupported` (A048) diagnostics the analysis
    /// emits, filtering by variant so unrelated rules do not perturb the count.
    fn count_a048(source: &str) -> usize {
        match analyze(source) {
            Ok(_) => 0,
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::StringNotSupported { .. }))
                .count(),
        }
    }

    /// Collects the `position` string of every A048 diagnostic, in report order,
    /// so a test can pin exactly which positions fired.
    fn a048_positions(source: &str) -> Vec<&'static str> {
        match analyze(source) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .errors()
                .iter()
                .filter_map(|e| match e {
                    AnalysisDiagnostic::StringNotSupported { position, .. } => Some(*position),
                    _ => None,
                })
                .collect(),
        }
    }

    /// Whether any analysis *error* with the given rule id was produced, used to
    /// assert that a neighbouring rule still fires beside A048 — the crate has
    /// no cross-rule suppression and must not acquire one silently.
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
    // Fires — one test per position
    // ---------------------------------------------------------------------

    /// An annotated binding offends twice: the annotation names a type no value
    /// can be produced at, and the initializer produces one anyway. Before this
    /// rule the literal aborted the `Expr::StringLiteral` arm of
    /// `lower_expression` in `core/wasm-codegen/src/compiler.rs`.
    ///
    /// There is no un-annotated companion to this case: the grammar requires
    /// `: type` on a `let`, so `let s = "hi";` is a parse error and never
    /// reaches analysis at all.
    #[test]
    fn a048_annotated_binding_reports_the_type_and_the_literal() {
        let source = r#"
            pub fn f() -> i32 { let s: string = "hi"; return 0; }
        "#;
        assert_eq!(
            a048_positions(source),
            vec![
                "the declared type of a variable",
                "the type of a string literal",
            ],
            "the annotation and the literal are two separate things to remove"
        );
        assert!(
            !compiles(source),
            "a string literal must never reach code generation"
        );
    }

    /// A `string` return type and the literal that satisfies it. The signature
    /// half is reported by the declaration walk, which runs before the body
    /// walk, so the return type is named first.
    ///
    /// Code generation fails this program on the signature alone — the return
    /// type resolves to `a `string` value has no WebAssembly lowering; A048
    /// rejects it before code generation` — and never reaches the literal. That
    /// failure is a clean error rather than an abort, which is why the closure
    /// this rule provides is about the *literal* and the layout positions rather
    /// than about signatures.
    #[test]
    fn a048_string_return_type_and_returned_literal() {
        let source = r#"
            pub fn f() -> string { return "hi"; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a048_positions(source),
            vec!["the return type of a function", "the type of a string literal"],
            "the declaration walk precedes the body walk"
        );
        assert!(!compiles(source));
    }

    #[test]
    fn a048_named_string_parameter() {
        let source = r#"
            pub fn f(s: string) -> i32 { return 0; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(a048_positions(source), vec!["the type of a parameter"]);
    }

    /// `_: string` is a parameter like any other as far as this rule is
    /// concerned: the declaration still claims a value no caller can supply.
    #[test]
    fn a048_ignored_string_parameter() {
        let source = r#"
            pub fn f(_: string) -> i32 { return 0; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(a048_positions(source), vec!["the type of a parameter"]);
    }

    /// Both builtin spellings resolve to the same type kind, so the capitalized
    /// alias is not a way around the rule.
    #[test]
    fn a048_capitalized_string_alias_parameter() {
        let source = r#"
            pub fn f(s: String) -> i32 { return 0; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(a048_positions(source), vec!["the type of a parameter"]);
    }

    #[test]
    fn a048_struct_field() {
        let source = r#"
            struct S { s: string; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(a048_positions(source), vec!["the type of a struct field"]);
    }

    /// An array of strings is reported once, on the annotation that carries it —
    /// not once per element and not once per layer.
    ///
    /// This declaration compiles today, which is precisely why the position is
    /// worth covering: an array parameter is passed by reference, so its element
    /// size is never computed and the shape slips through to emit a signature
    /// nobody can call. The binding form below is the one that aborts.
    #[test]
    fn a048_array_of_string_parameter_reports_once() {
        let source = r#"
            pub fn f(a: [string; 2]) -> i32 { return 0; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(a048_positions(source), vec!["the type of a parameter"]);
    }

    /// The array shape that reaches frame layout. Before this rule it aborted
    /// `element_size` in `core/wasm-codegen/src/memory.rs` with "Unsupported
    /// type for byte-size computation: String", one phase earlier than a bare
    /// literal does.
    #[test]
    fn a048_array_of_string_binding() {
        let source = r#"
            pub fn f() -> i32 { let a: [string; 2] = ["a", "b"]; return 0; }
        "#;
        assert_eq!(
            a048_positions(source),
            vec![
                "the declared type of a variable",
                "the type of a string literal",
                "the type of a string literal",
            ]
        );
        assert!(
            !compiles(source),
            "an array of strings must never reach frame layout"
        );
    }

    /// The struct shape that reaches layout: a field type only costs bytes once
    /// a value of the struct exists. Before this rule it aborted the same
    /// `element_size` in `core/wasm-codegen/src/memory.rs`.
    #[test]
    fn a048_string_field_of_an_instantiated_struct() {
        let source = r#"
            struct S { s: string; }
            pub fn f() -> i32 { let v: S = S { s: "x" }; return 0; }
        "#;
        assert_eq!(
            a048_positions(source),
            vec!["the type of a struct field", "the type of a string literal"]
        );
        assert!(
            !compiles(source),
            "a string field must never reach struct layout"
        );
    }

    /// The peel is recursive, so nesting does not multiply the report.
    #[test]
    fn a048_nested_array_of_string_reports_once() {
        let source = r#"
            pub fn f(a: [[string; 2]; 3]) -> i32 { return 0; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a048(source), 1, "one annotation, one finding");
        assert_eq!(a048_positions(source), vec!["the type of a parameter"]);
    }

    #[test]
    fn a048_function_local_const() {
        let source = r#"
            pub fn f() -> i32 { const S: string = "x"; return 0; }
        "#;
        assert_eq!(
            a048_positions(source),
            vec!["the declared type of a variable", "the type of a string literal"]
        );
    }

    /// A module-scope `const` is checked in its own right. A032 rejects every
    /// top-level `const` as an unimplemented feature, and both fire: resting
    /// this closure on A032 would make it silently incomplete the day that
    /// feature lands.
    #[test]
    fn a048_module_scope_const_reports_beside_a032() {
        let source = r#"
            const S: string = "x";
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a048_positions(source),
            vec!["the declared type of a variable", "the type of a string literal"]
        );
        assert!(
            has_error(source, "A032"),
            "A032 must still reject the top-level `const`; there is no cross-rule suppression"
        );
    }

    #[test]
    fn a048_method_parameter() {
        let source = r#"
            struct P { x: i32; fn m(self, s: string) -> i32 { return 0; } }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(a048_positions(source), vec!["the type of a parameter"]);
        assert!(
            has_warning(source, "A010"),
            "the unrelated A010 warning on this method must be unaffected"
        );
    }

    /// An `external fn` declares an ABI surface, and a string has no ABI
    /// representation to declare — so both halves of its signature are in scope.
    #[test]
    fn a048_external_function_parameter() {
        let source = r#"
            external fn e(string);
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(a048_positions(source), vec!["the type of a parameter"]);
    }

    #[test]
    fn a048_external_function_return_type() {
        let source = r#"
            external fn e() -> string;
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a048_positions(source),
            vec!["the return type of a function"]
        );
    }

    /// A spec function is lowered to a real WebAssembly function in proof mode,
    /// so a string inside a spec body reaches the same expression lowering a
    /// top-level one does and aborted there. Compile mode emits no spec
    /// functions at all, so the rule is stated on source shape and reports in
    /// both modes alike.
    #[test]
    fn a048_spec_body_is_covered() {
        let source = r#"
            pub fn main() -> i32 { return 0; }
            spec S { fn c() -> i32 { let s: string = "x"; return 0; } }
        "#;
        assert_eq!(
            a048_positions(source),
            vec!["the declared type of a variable", "the type of a string literal"],
            "a spec body is walked exactly as a top-level body is"
        );
    }

    /// The literal half descends into sub-expressions, so a literal that is
    /// never bound to anything is still reported — once per literal.
    #[test]
    fn a048_literals_in_a_sub_expression() {
        let source = r#"
            pub fn f() -> i32 { let b: bool = "a" == "b"; return 0; }
        "#;
        assert_eq!(
            a048_positions(source),
            vec!["the type of a string literal", "the type of a string literal"],
            "both operands are reported; the `bool` annotation is not"
        );
        assert!(
            !compiles(source),
            "a string literal must never reach code generation"
        );
    }

    // ---------------------------------------------------------------------
    // Controls — must not fire
    // ---------------------------------------------------------------------

    #[test]
    fn a048_plain_program_is_untouched() {
        let source = r#"
            pub fn main() -> i32 { let x: i32 = 1; return x; }
        "#;
        assert_eq!(count_a048(source), 0);
        assert!(compiles(source), "a program with no string must compile");
    }

    /// Documented non-scope: aliases are nominal in Inference, so `type S =
    /// string;` names a type at which no value can be produced. Every position
    /// that could produce one is covered elsewhere.
    #[test]
    fn a048_item_level_type_alias_is_not_flagged() {
        let source = r#"
            type S = string;
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a048(source), 0);
        assert!(
            compiles(source),
            "an item-level alias is erased, so it compiles unchanged"
        );
    }

    /// The statement form of the same non-scope.
    #[test]
    fn a048_local_type_alias_is_not_flagged() {
        let source = r#"
            pub fn f() -> i32 { type S = string; return 0; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a048(source), 0);
        assert!(
            compiles(source),
            "a body-level alias is erased just as an item-level one is"
        );
    }

    /// A `self` receiver spells no type of its own, so the receiver arm is never
    /// a route into this rule.
    #[test]
    fn a048_self_receiver_is_not_flagged() {
        let source = r#"
            struct P { x: i32; fn m(self) -> i32 { return self.x; } }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a048(source), 0);
        assert!(compiles(source));
    }

    /// Two value-representation rules on one signature, one per parameter. They
    /// are independent facts with independent fixes, and neither suppresses the
    /// other.
    #[test]
    fn a048_and_a045_both_report_on_one_signature() {
        let source = r#"
            struct E { }
            pub fn f(s: string, e: E) -> i32 { return 0; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(has_a048(source), "the `string` parameter must be reported");
        assert!(
            has_error(source, "A045"),
            "the field-less struct parameter must still be reported"
        );
    }
}
