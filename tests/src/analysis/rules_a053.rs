/// Integration tests for analysis rule A053.
///
/// - A053: `ArithModeGovernsNothing` — `checked(e)` and `wrapping(e)` must
///   contain an operator to govern. The annotation reaches the `+`, `-`, `*`
///   and unary `-` written between its own parentheses and nothing else.
///
/// The controls matter as much as the findings here. The rule's whole content
/// is *where* the annotation reaches, so a suite that only showed it firing
/// would not distinguish it from one that fired on every annotation.
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::{AnalysisMode, CodegenAttempt, build_ast, codegen_attempt};
    use inference_analysis::errors::{AnalysisDiagnostic, AnalysisErrors, AnalysisResult};
    use inference_type_checker::typed_context::TypedContext;

    const WIDTHS: [&str; 8] = ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64"];

    fn type_check(source: &str) -> TypedContext {
        let arena = build_ast(source.to_string());
        inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .expect("type checking should succeed for analysis test input")
            .typed_context()
    }

    fn analyze(source: &str) -> Result<AnalysisResult, AnalysisErrors> {
        inference_analysis::analyze(&type_check(source))
    }

    /// The A053 messages the analysis emits, in report order.
    fn a053_messages(source: &str) -> Vec<String> {
        match analyze(source) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::ArithModeGovernsNothing { .. }))
                .map(ToString::to_string)
                .collect(),
        }
    }

    fn count_a053(source: &str) -> usize {
        a053_messages(source).len()
    }

    #[test]
    fn an_annotated_call_governs_nothing_at_every_width() {
        // The mistake the rule exists for: the annotation is on the call, and
        // the callee's own arithmetic is where the wrapping would have to be.
        for width in WIDTHS {
            let source = format!(
                "pub fn mix(s: {width}) -> {width} {{ return s; }}
                 pub fn f(s: {width}) -> {width} {{ return wrapping(mix(s)); }}"
            );
            assert_eq!(count_a053(&source), 1, "{width}");
        }
    }

    #[test]
    fn a_glued_negative_literal_is_not_a_negation() {
        // One token carrying its own sign, so there is no unary `-` inside the
        // annotation for it to govern.
        for (width, literal) in [
            ("i8", "-128"),
            ("i16", "-32768"),
            ("i32", "-2147483648"),
            ("i64", "-9223372036854775808"),
        ] {
            let source =
                format!("pub fn f() -> {width} {{ return wrapping({literal}); }}");
            assert_eq!(count_a053(&source), 1, "{width}");
            let checked = format!("pub fn f() -> {width} {{ return checked({literal}); }}");
            assert_eq!(count_a053(&checked), 1, "{width}");
        }
    }

    #[test]
    fn an_annotated_name_or_projection_governs_nothing() {
        // Every shape that carries a value without computing one: a binding, a
        // literal, an element, a field, and a comparison of two of them.
        for inner in ["x", "1", "xs[0]", "p.v"] {
            let source = format!(
                "struct P {{ v: i32; }}
                 pub fn f(x: i32, xs: [i32; 2], p: P) -> i32 {{ return checked({inner}); }}"
            );
            assert_eq!(count_a053(&source), 1, "{inner}");
        }
        // A comparison of two of them is scalar and still governs nothing.
        assert_eq!(
            count_a053("pub fn f(n: i32) -> bool { return checked(n > 1); }"),
            1
        );
    }

    #[test]
    fn the_two_spellings_state_different_stakes() {
        let wrapping = a053_messages("pub fn g(x: i32) -> i32 { return x; }
             pub fn f(x: i32) -> i32 { return wrapping(g(x)); }");
        assert_eq!(wrapping.len(), 1);
        assert!(
            wrapping[0].contains("has no arithmetic to change"),
            "{:?}",
            wrapping[0]
        );

        let checked = a053_messages("pub fn g(x: i32) -> i32 { return x; }
             pub fn f(x: i32) -> i32 { return checked(g(x)); }");
        assert_eq!(checked.len(), 1);
        assert!(
            checked[0].contains("has no arithmetic to check")
                && checked[0].contains("a reader who takes it as a guarantee"),
            "{:?}",
            checked[0]
        );
    }

    #[test]
    fn every_governed_operator_at_every_width_is_accepted() {
        for width in WIDTHS {
            for op in ["+", "-", "*"] {
                let source = format!(
                    "pub fn f(a: {width}, b: {width}) -> {width} \
                     {{ return checked(a {op} b); }}"
                );
                assert_eq!(count_a053(&source), 0, "{width} {op}");
            }
        }
        // Unary `-` is legal only on the signed widths.
        for width in ["i8", "i16", "i32", "i64"] {
            let source = format!("pub fn f(a: {width}) -> {width} {{ return wrapping(-a); }}");
            assert_eq!(count_a053(&source), 0, "{width} unary -");
        }
    }

    #[test]
    fn an_operator_anywhere_inside_the_annotation_counts() {
        // Governed, not merely adjacent: an operator nested under a call
        // argument, an index, an aggregate element or a comparison is still
        // written between the annotation's own parentheses.
        let sources = [
            "pub fn g(x: i32) -> i32 { return x; }
             pub fn f(a: i32, b: i32) -> i32 { return checked(g(a + b)); }",
            "pub fn f(a: [i32; 4], i: i32) -> i32 { return wrapping(a[i + 1]); }",
            "pub fn f(a: i32, b: i32, c: i32) -> bool { return wrapping(a + b > c); }",
            "pub fn f(a: i32, b: i32, c: i32) -> i32 { return checked(f2(a) * 2); }
             pub fn f2(a: i32) -> i32 { return a; }",
        ];
        for source in sources {
            assert_eq!(count_a053(source), 0, "{source}");
        }
    }

    #[test]
    fn a_nested_annotation_does_not_empty_the_one_around_it() {
        // The predicate is containment, not government: the `+` under the inner
        // annotation is still written inside the outer one, so A053 is not the
        // finding here even though the outer governs no operator of its own.
        assert_eq!(
            count_a053("pub fn f(a: i32, b: i32) -> i32 { return wrapping(checked(a + b)); }"),
            0
        );
        assert_eq!(
            count_a053(
                "pub fn f(a: i32, b: i32, c: i32) -> i32 \
                 { return checked(a * wrapping(b + c)); }"
            ),
            0
        );
    }

    #[test]
    fn the_ungoverned_operators_do_not_satisfy_the_rule() {
        // `/`, `%`, the shifts and the bitwise operators cannot leave their
        // type, so an annotation containing only one of them governs nothing.
        for op in ["/", "%", "&", "|", "^", "<<", ">>"] {
            let source = format!(
                "pub fn f(a: i32, b: i32) -> i32 {{ return wrapping(a {op} b); }}"
            );
            assert_eq!(count_a053(&source), 1, "{op}");
        }
    }

    #[test]
    fn one_finding_per_offending_annotation() {
        let source = "pub fn g(x: i32) -> i32 { return x; }
             pub fn f(x: i32) -> i32 {
               let a: i32 = wrapping(g(x));
               let b: i32 = checked(g(x));
               return a + b;
             }";
        assert_eq!(count_a053(source), 2);
    }

    /// A rejected program never reaches code generation, and the way to observe
    /// that is the analysis-running codegen path — not `try_codegen`, which
    /// unwraps the analysis result outside the panic boundary.
    #[test]
    fn a_rejected_program_does_not_reach_code_generation() {
        let source = "pub fn g(x: i32) -> i32 { return x; }
             pub fn f(x: i32) -> i32 { return wrapping(g(x)); }";
        match codegen_attempt(source, AnalysisMode::Run) {
            CodegenAttempt::Rejected(errors) => assert!(
                errors.contains("error[A053]"),
                "the rejection must be this rule's, got: {errors}"
            ),
            CodegenAttempt::Ok(_) => panic!("A053 must reject this program"),
            CodegenAttempt::Panicked(payload) => panic!("unexpected panic: {payload}"),
        }
        // The same source with the annotation moved onto the arithmetic
        // compiles, which is what makes the rejection about the position rather
        // than about the annotation.
        assert!(matches!(
            codegen_attempt(
                "pub fn g(x: i32) -> i32 { return wrapping(x + 1); }
                 pub fn f(x: i32) -> i32 { return g(x); }",
                AnalysisMode::Run
            ),
            CodegenAttempt::Ok(_)
        ));
    }
}
