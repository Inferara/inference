/// Integration tests for analysis rule A051.
///
/// - A051: `GenericNotSupported` — generic code has no lowering. A `fn`
///   declaring type parameters is refused because nothing carries the type
///   checker's per-call-site substitution any further, and a type application
///   (`Q i32'`) is refused because no type declaration in the language accepts
///   type arguments at all.
///
/// The two sites are one rule with one id, so the tests assert the *site* a
/// finding carries as well as the count: a change that collapsed them would keep
/// every count row green.
///
/// Several sources here compile to a valid module today, which is what the
/// `compiles` assertions pin. A generic `spec` function, an unused struct field
/// typed `Q i32'`, a `[Q i32'; 2]` parameter and a type parameter shadowing a
/// declared struct all reach code generation without a diagnostic before this
/// rule — the last of them emitting a module WebAssembly validation rejects.
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::{build_ast, try_codegen_no_analysis};
    use inference_analysis::errors::{
        AnalysisDiagnostic, AnalysisErrors, AnalysisResult, GenericSite,
    };
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

    /// Returns true if any analysis error is a `GenericNotSupported` (A051).
    fn has_a051(source: &str) -> bool {
        match analyze(source) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::GenericNotSupported { .. })),
        }
    }

    /// Counts how many `GenericNotSupported` (A051) diagnostics the analysis
    /// emits, filtering by variant so unrelated rules do not perturb the count.
    fn count_a051(source: &str) -> usize {
        match analyze(source) {
            Ok(_) => 0,
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::GenericNotSupported { .. }))
                .count(),
        }
    }

    /// Collects the site of every A051 diagnostic, in report order, so a test
    /// can pin which surface fired and how the construct was rendered.
    fn a051_findings(source: &str) -> Vec<GenericSite> {
        match analyze(source) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .errors()
                .iter()
                .filter_map(|e| match e {
                    AnalysisDiagnostic::GenericNotSupported { site, .. } => Some(site.clone()),
                    _ => None,
                })
                .collect(),
        }
    }

    /// Collects `(line, column)` of every A051 diagnostic, in report order.
    fn a051_carets(source: &str) -> Vec<(u32, u32)> {
        match analyze(source) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .errors()
                .iter()
                .filter_map(|e| match e {
                    AnalysisDiagnostic::GenericNotSupported { location, .. } => {
                        Some((location.start_line, location.start_column))
                    }
                    _ => None,
                })
                .collect(),
        }
    }

    /// The rendered text of every A051 diagnostic, in report order.
    fn a051_messages(source: &str) -> Vec<String> {
        match analyze(source) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::GenericNotSupported { .. }))
                .map(std::string::ToString::to_string)
                .collect(),
        }
    }

    fn declaration(function: &str, params: &str) -> GenericSite {
        GenericSite::Declaration {
            function: function.to_string(),
            params: params.to_string(),
        }
    }

    fn application(rendered: &str, position: &'static str) -> GenericSite {
        GenericSite::TypeApplication {
            rendered: rendered.to_string(),
            position,
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
    // Fires: a declaration that binds type parameters
    // ---------------------------------------------------------------------

    /// The shape the rule exists for. Before it, this reached signature lowering
    /// and failed with an unsupported-type error naming `T` and carrying no
    /// location at all, because that lowering sees a type node and no enclosing
    /// type parameters and cannot tell a binder from a misspelled type name.
    ///
    /// Dropping the type-parameter check turns this red.
    #[test]
    fn a051_generic_function_with_a_caller() {
        let source = r#"
            fn id T'(x: T) -> T { return x; }
            pub fn f(n: i32) -> i32 { return id(n); }
        "#;
        assert_eq!(a051_findings(source), vec![declaration("id", "T'")]);
    }

    /// The defect is a property of the declaration, not of a use: an uncalled
    /// generic reaches exactly the same dead end, because code generation lowers
    /// every definition regardless of reachability.
    ///
    /// Filtering the check by the call graph turns this red.
    #[test]
    fn a051_uncalled_generic_function_is_still_refused() {
        let source = r#"
            fn id T'(x: T) -> T { return x; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(a051_findings(source), vec![declaration("id", "T'")]);
    }

    /// The binders are one list removed by one edit, so two of them are one
    /// finding naming both, not two findings.
    ///
    /// Reporting per binder turns this red with two findings.
    #[test]
    fn a051_two_binders_are_one_finding_naming_both() {
        let source = r#"
            fn pick T' U'(a: T, b: U) -> T { return a; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(a051_findings(source), vec![declaration("pick", "T' U'")]);
    }

    /// A method is named the way a call spells it, so the message points at a
    /// declaration the reader can find even when two structs declare the same
    /// method name.
    ///
    /// Dropping the descent into `Def::Struct { methods }` turns this red.
    #[test]
    fn a051_generic_method_is_named_by_its_struct() {
        let source = r#"
            struct P { x: i32; fn m T'(self, v: T) -> i32 { return 1; } }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(a051_findings(source), vec![declaration("P::m", "T'")]);
        assert!(
            has_warning(source, "A010"),
            "the unrelated A010 warning on this method must be unaffected"
        );
    }

    /// Compile mode does not emit spec functions at all, so a generic one
    /// *appears* to compile today: the function is dropped from the artifact,
    /// not supported in it. Proof mode lowers it for real and fails. The rule is
    /// stated on source shape so both modes get the same message.
    ///
    /// Dropping the `Def::Spec` recursion turns this red — the program builds
    /// green, which is exactly the state this rule removes.
    #[test]
    fn a051_spec_declared_generic_is_covered() {
        let source = r#"
            pub fn main() -> i32 { return 0; }
            spec S { fn id T'(x: T) -> T { return x; } }
        "#;
        assert_eq!(a051_findings(source), vec![declaration("id", "T'")]);
        assert!(
            !compiles(source),
            "a generic spec function compiled green before this rule and must not now"
        );
    }

    /// The declaration walk recurses through `spec` into the methods of a struct
    /// declared inside it, which is a level neither the emitted-function walk nor
    /// a one-level descent would reach.
    #[test]
    fn a051_generic_method_of_a_spec_nested_struct_is_covered() {
        let source = r#"
            pub fn main() -> i32 { return 0; }
            spec S { struct P { x: i32; fn m T'(self, v: T) -> i32 { return self.x; } } }
        "#;
        assert_eq!(a051_findings(source), vec![declaration("P::m", "T'")]);
    }

    /// The miscompile this rule closes. The type checker binds `T` to the type
    /// parameter and the backend binds it to the struct, so the program exits 0
    /// and emits a module WebAssembly validation rejects. The predicate is the
    /// declared binders and nothing else, which is what catches it.
    ///
    /// Keying the check on whether the binder resolves to no declared type turns
    /// this red, and the invalid module ships again.
    #[test]
    fn a051_type_parameter_shadowing_a_struct_is_refused() {
        let source = r#"
            struct T { x: i32; }
            fn g T'(a: T) -> T { return a; }
            pub fn f(n: i32) -> i32 { return g(n); }
        "#;
        assert_eq!(a051_findings(source), vec![declaration("g", "T'")]);
        assert!(
            !compiles(source),
            "the shadowing program emitted an invalid module before this rule"
        );
    }

    /// A binder that appears in no lowered type is uninferable, so the type
    /// checker refuses every call to the function — while code generation still
    /// emits the definition, with the binder silently discarded. The declaration
    /// has to be written without a call site or it fails at type checking and
    /// never reaches analysis at all.
    ///
    /// Narrowing the predicate to a binder used in a lowered type turns this red.
    #[test]
    fn a051_binder_used_in_no_lowered_type_is_refused() {
        let source = r#"
            fn h T'(x: i32) -> i32 { return x; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(a051_findings(source), vec![declaration("h", "T'")]);
        assert!(
            !compiles(source),
            "a discarded binder compiled green before this rule and must not now"
        );
    }

    /// The entry point is not carved out. `pub fn main T'()` built a complete
    /// module, and in proof mode a complete obligation, with the binder thrown
    /// away — a declaration promising polymorphism the compiler discards.
    ///
    /// Any entry-point exemption turns this red.
    #[test]
    fn a051_generic_entry_point_is_refused() {
        let source = "pub fn main T'() -> i32 { return 0; }";
        assert_eq!(a051_findings(source), vec![declaration("main", "T'")]);
        assert!(
            !compiles(source),
            "a generic entry point compiled green before this rule and must not now"
        );
    }

    /// Two declarations are two findings. Both are written callless, since a
    /// call to either would fail at type checking first.
    ///
    /// De-duplicating by anything other than the declaration turns this red.
    #[test]
    fn a051_two_generic_declarations_report_separately() {
        let source = r#"
            fn h T'(x: i32) -> i32 { return x; }
            fn k U'(x: i32) -> i32 { return x; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a051(source), 2);
        assert_eq!(
            a051_findings(source),
            vec![declaration("h", "T'"), declaration("k", "U'")]
        );
    }

    /// The caret sits on the first binder, which is the construct the message is
    /// about — not on the function name and not on the definition. The columns
    /// are read off this exact source layout; carets move with it, so a rewrite
    /// of the fixture has to re-derive them.
    ///
    /// Moving the caret to the name or to the definition turns this red.
    #[test]
    fn a051_caret_sits_on_the_first_type_parameter() {
        let source = "fn pick T' U'(a: T, b: U) -> T { return a; }\npub fn main() -> i32 { return 0; }";
        assert_eq!(a051_carets(source), vec![(1, 9)]);
    }

    // ---------------------------------------------------------------------
    // Fires: a type application in a declared type
    // ---------------------------------------------------------------------

    /// The parameter position, which failed before this rule with an
    /// unsupported-type error carrying no location.
    #[test]
    fn a051_type_application_as_a_parameter() {
        let source = r#"
            struct Q { x: i32; }
            pub fn g(p: Q i32') -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a051_findings(source),
            vec![application("Q i32'", "the type of a parameter")]
        );
    }

    /// The return position, asserted beside the parameter one because the only
    /// value a function returning `Q i32'` can return is a parameter of the same
    /// type — nothing else in the language produces one.
    #[test]
    fn a051_type_application_as_a_return_type() {
        let source = r#"
            struct Q { x: i32; }
            pub fn g(p: Q i32') -> Q i32' { return p; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a051_findings(source),
            vec![
                application("Q i32'", "the type of a parameter"),
                application("Q i32'", "the return type of a function"),
            ]
        );
    }

    /// A struct nobody uses still declares a field the layout has no size for.
    /// The program compiled green before this rule, because nothing lowers an
    /// unused struct.
    ///
    /// Dropping the struct-field walk turns this red.
    #[test]
    fn a051_type_application_as_a_struct_field() {
        let source = r#"
            struct Q { x: i32; }
            struct P { y: Q i32'; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a051_findings(source),
            vec![application("Q i32'", "the type of a struct field")]
        );
        assert!(
            !compiles(source),
            "an unused struct with a generic field compiled green before this rule"
        );
    }

    /// An array of a type that has no layout has none either, so the annotation
    /// is looked through to its element. The parameter compiled to a module
    /// WebAssembly validation *accepts*, whose export took a bare `i32` — the
    /// generic element vanished from the ABI.
    ///
    /// Stopping the array descent turns this red.
    #[test]
    fn a051_type_application_as_an_array_element() {
        let source = r#"
            struct Q { x: i32; }
            pub fn g(p: [Q i32'; 2]) -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a051_findings(source),
            vec![application("Q i32'", "the type of a parameter")]
        );
        assert!(
            !compiles(source),
            "a generic array element emitted a module with a silently bare ABI before this rule"
        );
    }

    /// The descent reaches the element at any depth, and still reports once, on
    /// the annotation that carries it. The nested case is asserted beside the
    /// flat one because a descent that peels a single layer keeps the flat case
    /// green.
    #[test]
    fn a051_type_application_nested_in_arrays_reports_once() {
        let source = r#"
            struct Q { x: i32; }
            pub fn g(p: [[Q i32'; 2]; 3]) -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a051_findings(source),
            vec![application("Q i32'", "the type of a parameter")]
        );
        assert!(!compiles(source));
    }

    /// An `external fn` declares an ABI signature, and a type application states
    /// no ABI at all: an unbound extern with one produced no import section
    /// whatsoever before this rule.
    ///
    /// Excluding `Def::ExternFunction` turns this red.
    #[test]
    fn a051_type_application_on_an_extern_parameter() {
        let source = r#"
            struct Q { x: i32; }
            external fn e(p: Q i32') -> i32;
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a051_findings(source),
            vec![application("Q i32'", "the type of a parameter")]
        );
        assert!(
            !compiles(source),
            "an unbound extern with a generic parameter compiled green before this rule"
        );
    }

    /// The binding is declared without an initializer, which isolates the
    /// annotation: the only expression of this type is the application itself,
    /// and writing one would add a second finding. A025 rejects the missing
    /// initializer in its own right, so the count is filtered by variant.
    #[test]
    fn a051_type_application_as_a_let_annotation() {
        let source = r#"
            struct Q { x: i32; }
            pub fn main() -> i32 { let y: Q i32'; return 0; }
        "#;
        assert_eq!(
            a051_findings(source),
            vec![application("Q i32'", "the declared type of a variable")]
        );
    }

    /// A `const` must be initialized, and the only expression of this type is
    /// the application itself, so the annotation and the value are asserted
    /// together — which is also what pins that the two are read from different
    /// places on the same statement.
    #[test]
    fn a051_type_application_as_a_function_local_const_annotation() {
        let source = r#"
            struct Q { x: i32; }
            pub fn main() -> i32 { const C: Q i32' = Q i32'; return 0; }
        "#;
        assert_eq!(
            a051_findings(source),
            vec![
                application("Q i32'", "the declared type of a variable"),
                application("Q i32'", "a value"),
            ]
        );
    }

    /// A module-scope `const` is checked in its own right rather than left to
    /// A032's blanket rejection of top-level `const`, which is a gate on an
    /// unimplemented feature. Both rules fire, so the count is filtered by
    /// variant.
    ///
    /// Its annotation and its value are both asserted here, and the value half is
    /// the one that needs the declaration walk: the body walk visits function
    /// bodies only, so no expression of a module-scope declaration is reachable
    /// from it. Dropping that descent turns this red with one finding.
    #[test]
    fn a051_type_application_in_a_module_scope_const() {
        let source = r#"
            struct Q { x: i32; }
            const C: Q i32' = Q i32';
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a051(source), 2);
        assert_eq!(
            a051_findings(source),
            vec![
                application("Q i32'", "the declared type of a variable"),
                application("Q i32'", "a value"),
            ]
        );
    }

    /// Every type argument is rendered, each with the tick that marks it.
    #[test]
    fn a051_two_type_arguments_are_both_rendered() {
        let source = r#"
            struct Q { x: i32; }
            pub fn g(p: Q i32' u8') -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a051_findings(source),
            vec![application("Q i32' u8'", "the type of a parameter")]
        );
    }

    /// The spelling comes from the arena, from the identifiers the source wrote.
    /// The type checker's own rendering of the same node puts a tick on the base
    /// as well (`Q' i32'`), which is not a spelling the grammar accepts — and a
    /// message whose fix asks the author to edit what they wrote must quote what
    /// they wrote.
    ///
    /// Rendering from the recorded type instead turns this red.
    #[test]
    fn a051_renders_the_application_the_way_the_source_spells_it() {
        let source = r#"
            struct Q { x: i32; }
            pub fn g(p: Q i32') -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        let messages = a051_messages(source);
        assert_eq!(messages.len(), 1);
        assert!(
            messages[0].contains("`Q i32'`"),
            "A051 must quote the application as written, got: {}",
            messages[0]
        );
        assert!(
            !messages[0].contains("`Q' i32'`"),
            "A051 must not quote the type checker's spelling, which the grammar rejects, got: {}",
            messages[0]
        );
    }

    // ---------------------------------------------------------------------
    // Fires: a type application in expression position
    // ---------------------------------------------------------------------

    /// A type standing where a value is required. This one is a statement root.
    #[test]
    fn a051_type_application_as_a_statement() {
        let source = r#"
            struct Q { x: i32; }
            pub fn main() -> i32 { Q i32'; return 0; }
        "#;
        assert_eq!(a051_findings(source), vec![application("Q i32'", "a value")]);
    }

    /// The nested case, and the reason the expression half is a recursive
    /// descent rather than a pass over statement roots: here the type sits inside
    /// a parenthesization, so the statement's root expression is the wrapper and a
    /// root-only walk never sees the application.
    ///
    /// Replacing the descent with a statement-root pass turns this row red on its
    /// own, while the statement row above stays green.
    #[test]
    fn a051_type_application_nested_in_an_expression() {
        let source = r#"
            struct Q { x: i32; }
            pub fn main() -> i32 { (Q i32'); return 0; }
        "#;
        assert_eq!(a051_findings(source), vec![application("Q i32'", "a value")]);
    }

    /// An annotation and an expression are two separate things to remove, and a
    /// reader repairing one still has to see the other.
    #[test]
    fn a051_annotation_and_expression_report_separately() {
        let source = r#"
            struct Q { x: i32; }
            pub fn main() -> i32 { let y: Q i32' = Q i32'; return 0; }
        "#;
        assert_eq!(
            a051_findings(source),
            vec![
                application("Q i32'", "the declared type of a variable"),
                application("Q i32'", "a value"),
            ]
        );
    }

    // ---------------------------------------------------------------------
    // Co-firing
    // ---------------------------------------------------------------------

    /// A declaration that binds a type parameter and applies one is two
    /// findings, in that order: the binder and the application are refused for
    /// different reasons and are two different edits.
    ///
    /// Collapsing the sites, or de-duplicating them, turns this red with one
    /// finding; reporting either twice turns it red with three.
    #[test]
    fn a051_a_declaration_that_also_applies_a_type_argument_reports_both() {
        let source = r#"
            struct Q { x: i32; }
            fn g T'(p: Q T') -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(
            a051_findings(source),
            vec![
                declaration("g", "T'"),
                application("Q T'", "the type of a parameter"),
            ]
        );
    }

    // ---------------------------------------------------------------------
    // Controls
    // ---------------------------------------------------------------------

    /// The false positive the shadowing family makes tempting. A struct named
    /// `T` is an ordinary declaration, and a function taking one is ordinary
    /// code: the predicate is the declared binders, so nothing here is generic.
    #[test]
    fn a051_a_struct_named_like_a_type_parameter_is_untouched() {
        let source = r#"
            struct T { x: i32; }
            pub fn g(a: T) -> T { return a; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a051(source), 0);
        assert!(compiles(source));
    }

    /// A bare base name is a type, not an application of one.
    #[test]
    fn a051_a_bare_base_name_is_not_an_application() {
        let source = r#"
            struct Q { x: i32; }
            pub fn g(p: Q) -> i32 { return p.x; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a051(source), 0);
        assert!(compiles(source));
    }

    /// The array descent must reach the element and stop there, not fire on the
    /// array itself — at one layer or at several.
    #[test]
    fn a051_arrays_of_ordinary_types_are_untouched() {
        let flat = r#"
            struct Q { x: i32; }
            pub fn g(p: [Q; 2]) -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a051(flat), 0);
        assert!(compiles(flat));

        let nested = r#"
            pub fn g(p: [[i32; 2]; 3]) -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a051(nested), 0);
        assert!(compiles(nested));
    }

    /// The `spec` recursion must find generic declarations without claiming
    /// ordinary ones.
    #[test]
    fn a051_a_non_generic_spec_function_is_untouched() {
        let source = r#"
            pub fn main() -> i32 { return 0; }
            spec S { fn c(x: i32) -> i32 { return x; } }
        "#;
        assert_eq!(count_a051(source), 0);
        assert!(compiles(source));
    }

    /// An `external fn` is in scope for the application site, so the control has
    /// to show that a concrete one is untouched by it.
    #[test]
    fn a051_a_concrete_extern_is_untouched() {
        let source = r#"
            external fn e(a: i32, b: i32) -> i32;
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a051(source), 0);
    }

    /// A `::`-qualified type is a path to a declaration, not an application of
    /// type arguments to one, and the two are distinct nodes.
    #[test]
    fn a051_a_qualified_type_is_untouched() {
        let source = r#"
            spec S { struct P { x: i32; } }
            pub fn g(p: S::P) -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert_eq!(count_a051(source), 0);
        assert!(compiles(source));
    }

    /// A `self` receiver spells no type of its own — its type is the enclosing
    /// struct — so the signature walk never sees a type node for it.
    #[test]
    fn a051_a_self_receiver_is_untouched() {
        let source = r#"
            struct P { x: i32; fn m(self) -> i32 { return self.x; } }
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(!has_a051(source));
        assert!(compiles(source));
    }

    /// A function type has no value representation with or without a type
    /// parameter in it, and is refused by code generation for a reason that has
    /// nothing to do with generics. The assertion is on A051 rather than on the
    /// pipeline, because this source still fails to compile.
    #[test]
    fn a051_a_function_type_is_not_claimed() {
        let source = r#"
            pub fn g(cb: fn(i32) -> i32) -> i32 { return 1; }
            pub fn main() -> i32 { return 0; }
        "#;
        assert!(
            !has_a051(source),
            "function types are refused for their own reason and are not this rule's subject"
        );
    }
}
