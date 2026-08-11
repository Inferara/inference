/// Integration tests for analysis rule A046.
///
/// - A046: `SpacedNegativeLiteral` — a unary minus applied to a numeric literal
///   must be written glued to the digits. `-128` is one token whose text carries
///   the sign; `- 128` is a `Neg` over the bare literal `128`, which every later
///   rule measures on its own. That made the same value compile or fail on a
///   space — `- 100` was accepted at `i8` while `- 128` was rejected as "literal
///   128 is out of range" — so the separated spelling is removed rather than
///   patched, leaving one canonical way to write a negative literal.
///
/// These tests exercise the rule through a real parse -> type-check -> analyze
/// pipeline, complementing the in-crate message/`rule_id` unit tests in
/// `core/analysis`. Two halves carry most of the weight. The first is the
/// handoff from A022: every literal A022 stopped measuring must still be
/// rejected here, so a magnitude that fits no type in either sign (`- 300` at
/// `i8`) is asserted to remain an error, and each signed minimum is asserted to
/// report A046 rather than the A022 message about the un-negated value. The
/// second is that the fix the message recommends is real: the glued spelling of
/// every rejected form is compiled end to end.
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::{build_ast, wasm_codegen};
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

    /// Collects the diagnostics of one rule, by variant rather than by count, so
    /// an unrelated rule tripped by the same surface cannot perturb the result.
    fn diagnostics_of(
        source: &str,
        want: fn(&AnalysisDiagnostic) -> bool,
    ) -> Vec<AnalysisDiagnostic> {
        match analyze(source) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| want(e))
                .cloned()
                .collect(),
        }
    }

    fn is_a046(diag: &AnalysisDiagnostic) -> bool {
        matches!(diag, AnalysisDiagnostic::SpacedNegativeLiteral { .. })
    }

    fn is_a022(diag: &AnalysisDiagnostic) -> bool {
        matches!(diag, AnalysisDiagnostic::LiteralOutOfRange { .. })
    }

    fn is_a033(diag: &AnalysisDiagnostic) -> bool {
        matches!(diag, AnalysisDiagnostic::CombinedUnaryOperators { .. })
    }

    fn a046_diags(source: &str) -> Vec<AnalysisDiagnostic> {
        diagnostics_of(source, is_a046)
    }

    fn assert_a046(source: &str) {
        assert!(
            !a046_diags(source).is_empty(),
            "expected A046 for {source:?}, got: {:?}",
            analyze(source).err()
        );
    }

    fn assert_no_a046(source: &str) {
        assert!(
            a046_diags(source).is_empty(),
            "did not expect A046 for {source:?}"
        );
    }

    // --- Every separation is the same offence ---

    #[test]
    fn a046_rejects_one_space() {
        assert_a046("pub fn f() -> i8 { return - 42; }");
    }

    #[test]
    fn a046_rejects_several_spaces() {
        assert_a046("pub fn f() -> i8 { return -   42; }");
    }

    #[test]
    fn a046_rejects_a_newline() {
        assert_a046("pub fn f() -> i8 { return -\n42; }");
    }

    #[test]
    fn a046_rejects_a_comment_in_the_gap() {
        // Inference has line comments only, so this is the comment form of the
        // gap. Trivia of any kind is a separation.
        assert_a046("pub fn f() -> i8 { return - // sign\n42; }");
    }

    // --- Every position a literal can be written in ---

    #[test]
    fn a046_rejects_in_a_let_initializer() {
        assert_a046("pub fn f() -> i8 { let x: i8 = - 42; return x; }");
    }

    #[test]
    fn a046_rejects_in_a_return() {
        assert_a046("pub fn f() -> i8 { return - 42; }");
    }

    #[test]
    fn a046_rejects_in_a_call_argument() {
        assert_a046("fn g(v: i8) -> i8 { return v; }\npub fn f() -> i8 { return g(- 42); }");
    }

    #[test]
    fn a046_rejects_in_an_array_index() {
        assert_a046("pub fn f(a: [i32; 4]) -> i32 { return a[- 1]; }");
    }

    #[test]
    fn a046_rejects_in_a_binary_operand() {
        // The left operand of a subtraction is itself a separated negation; the
        // binary minus beside it is untouched.
        assert_a046("pub fn f(a: i8) -> i8 { return - 42 - a; }");
    }

    #[test]
    fn a046_reports_each_offending_negation() {
        let source = "pub fn f() -> i8 { let x: i8 = - 1; let y: i8 = - 2; return x + y; }";
        assert_eq!(
            a046_diags(source).len(),
            2,
            "each separated negation is its own offence"
        );
    }

    // --- The handoff from A022: the signed minima ---

    #[test]
    fn a046_owns_every_signed_minimum() {
        // These are the values the old rule made unreachable in this spelling:
        // the un-negated magnitude is one past the type's maximum every time, so
        // A022 used to report a limit the negated value does not exceed. A046
        // must own them, and A022 must be silent.
        for (source, magnitude) in [
            ("pub fn f() -> i8 { return - 128; }", "128"),
            ("pub fn f() -> i16 { return - 32768; }", "32768"),
            ("pub fn f() -> i32 { return - 2147483648; }", "2147483648"),
            (
                "pub fn f() -> i64 { return - 9223372036854775808; }",
                "9223372036854775808",
            ),
        ] {
            let diags = a046_diags(source);
            assert_eq!(diags.len(), 1, "expected exactly one A046 for {source:?}");
            let text = diags[0].to_string();
            assert!(
                text.contains(&format!("write `-{magnitude}`")),
                "A046 must recommend the glued form for {source:?}, got: {text}"
            );
            assert!(
                diagnostics_of(source, is_a022).is_empty(),
                "A022 must stay silent on the construct A046 owns, for {source:?}"
            );
        }
    }

    #[test]
    fn a046_leaves_no_silent_acceptance_hole() {
        // `300` fits no `i8` in either sign, so nothing about the handoff may
        // let it through: A022 steps aside and A046 rejects the program.
        let source = "pub fn f() -> i8 { return - 300; }";
        let diags = a046_diags(source);
        assert_eq!(diags.len(), 1, "expected A046 for {source:?}");
        assert!(
            diags[0].to_string().contains("write `-300`"),
            "got: {}",
            diags[0]
        );
        assert!(
            diagnostics_of(source, is_a022).is_empty(),
            "A022 must not also speak about the un-negated 300"
        );
    }

    #[test]
    fn a046_hands_a_range_error_back_once_the_spelling_is_fixed() {
        // The glued form carries its sign into A022, which measures it as the
        // negative number it is — so the out-of-range value is still caught, now
        // with the value the author actually wrote.
        let source = "pub fn f() -> i8 { return -300; }";
        assert_no_a046(source);
        let diags = diagnostics_of(source, is_a022);
        assert_eq!(diags.len(), 1, "expected A022 for {source:?}");
        assert!(
            diags[0].to_string().contains("literal `-300`"),
            "A022 must measure the signed literal, got: {}",
            diags[0]
        );
    }

    // --- The recommended fix compiles ---

    #[test]
    fn a046_recommended_glued_spelling_compiles() {
        // The message tells the author to close the gap; each closed form is
        // taken all the way through codegen so the advice cannot be empty.
        for source in [
            "pub fn f() -> i8 { return -42; }",
            "pub fn f() -> i8 { return -128; }",
            "pub fn f() -> i16 { return -32768; }",
            "pub fn f() -> i32 { return -2147483648; }",
            "pub fn f() -> i64 { return -9223372036854775808; }",
            "pub fn f() -> i8 { let x: i8 = -42; return x; }",
            "fn g(v: i8) -> i8 { return v; }\npub fn f() -> i8 { return g(-42); }",
        ] {
            assert_no_a046(source);
            assert!(
                !wasm_codegen(source).is_empty(),
                "the glued spelling must compile: {source:?}"
            );
        }
    }

    // --- What stays legal ---

    #[test]
    fn a046_accepts_negating_a_variable() {
        // There is no token to glue the sign to, so there is no second spelling
        // to choose between.
        assert_no_a046("pub fn f(x: i8) -> i8 { return - x; }");
        assert_no_a046("pub fn f(x: i8) -> i8 { return -x; }");
    }

    #[test]
    fn a046_accepts_negating_a_call_result() {
        assert_no_a046("fn g() -> i8 { return 1; }\npub fn f() -> i8 { return - g(); }");
    }

    #[test]
    fn a046_accepts_binary_subtraction() {
        // Both spacings of a binary minus, including the glued one the lexer now
        // reads as an operator rather than a sign.
        assert_no_a046("pub fn f(a: i8) -> i8 { return a - 1; }");
        assert_no_a046("pub fn f(a: i8) -> i8 { return a-1; }");
        assert_no_a046("pub fn f(a: i8, b: i8) -> i8 { return a - b; }");
    }

    #[test]
    fn a046_ignores_other_unary_operators() {
        // Only `-` is folded into a literal by the lexer, so only `-` has a
        // whitespace-dependent second spelling to remove.
        assert_no_a046("pub fn f() -> i8 { return ~ 5; }");
        assert_no_a046("pub fn f(x: bool) -> bool { return ! x; }");
    }

    // --- Neighbouring rules keep their subjects ---

    #[test]
    fn a046_leaves_parenthesized_negation_to_a022() {
        // `-(128)` cannot be closed up into a token, so A046 does not claim it
        // and A022's reading of the parenthesized `128` is unchanged.
        let source = "pub fn f() -> i8 { return -(128); }";
        assert_no_a046(source);
        let diags = diagnostics_of(source, is_a022);
        assert_eq!(
            diags.len(),
            1,
            "A022 must still measure the parenthesized 128"
        );
        assert!(
            diags[0].to_string().contains("literal `128`"),
            "got: {}",
            diags[0]
        );
    }

    #[test]
    fn a046_leaves_a_doubled_sign_to_a033() {
        // `--42` reaches lowering as `Neg` over the literal `-42`, glued; the
        // spaced `- -42` reaches it separated. Both are A033's subject, and
        // A046's advice on either would spell out a form A033 rejects.
        for source in [
            "pub fn f() -> i8 { return --42; }",
            "pub fn f() -> i8 { return - -42; }",
        ] {
            assert_no_a046(source);
            assert!(
                !diagnostics_of(source, is_a033).is_empty(),
                "expected A033 for {source:?}"
            );
        }
    }

    // --- Diagnostic surface ---

    #[test]
    fn a046_rule_id_is_a046() {
        let diags = a046_diags("pub fn f() -> i8 { return - 42; }");
        assert_eq!(diags[0].rule_id(), "A046");
    }

    #[test]
    fn a046_location_points_at_the_minus() {
        let source = "pub fn f() -> i8 { return - 42; }";
        let minus = u32::try_from(source.find("- 42").expect("the source spells `- 42`"))
            .expect("offset fits u32");
        let location = *a046_diags(source)[0].location();
        assert_eq!(
            location.offset_start, minus,
            "the report must open on the minus, not on the digits"
        );
        assert_eq!((location.start_line, location.start_column), (1, minus + 1));
    }
}
