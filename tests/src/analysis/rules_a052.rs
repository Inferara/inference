/// Integration tests for analysis rule A052.
///
/// - A052: `ConstantArithmeticOverflow` — an operation whose operands fold to
///   constants and whose result leaves the type it is performed at is rejected,
///   because such an operation traps on every run that reaches it.
///
/// The controls carry most of the content. The rule's whole boundary is *what
/// folds* and *which operators are effectively checked*, so a suite that only
/// showed it firing would not distinguish it from one that rejected every
/// arithmetic expression it could see.
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::{AnalysisMode, CodegenAttempt, build_ast, codegen_attempt};
    use inference_analysis::errors::{AnalysisDiagnostic, AnalysisErrors, AnalysisResult};
    use inference_type_checker::typed_context::TypedContext;

    /// Every width with the two values at its ends, which are both the vectors
    /// that overflow by one and the vectors that must not.
    const BOUNDS: [(&str, &str, &str); 8] = [
        ("i8", "-128", "127"),
        ("i16", "-32768", "32767"),
        ("i32", "-2147483648", "2147483647"),
        ("i64", "-9223372036854775808", "9223372036854775807"),
        ("u8", "0", "255"),
        ("u16", "0", "65535"),
        ("u32", "0", "4294967295"),
        ("u64", "0", "18446744073709551615"),
    ];

    const SIGNED: [(&str, &str); 4] = [
        ("i8", "-128"),
        ("i16", "-32768"),
        ("i32", "-2147483648"),
        ("i64", "-9223372036854775808"),
    ];

    fn type_check(source: &str) -> TypedContext {
        let arena = build_ast(source.to_string());
        inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .expect("type checking should succeed for analysis test input")
            .typed_context()
    }

    fn analyze(source: &str) -> Result<AnalysisResult, AnalysisErrors> {
        inference_analysis::analyze(&type_check(source))
    }

    /// The A052 messages the analysis emits, in report order.
    fn a052_messages(source: &str) -> Vec<String> {
        match analyze(source) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::ConstantArithmeticOverflow { .. }))
                .map(ToString::to_string)
                .collect(),
        }
    }

    fn count_a052(source: &str) -> usize {
        a052_messages(source).len()
    }

    /// The rule ids of every diagnostic the analysis raises, so a test can pin
    /// which rule owns a program rather than only that some rule does.
    fn rule_ids(source: &str) -> Vec<&'static str> {
        match analyze(source) {
            Ok(_) => Vec::new(),
            Err(errors) => errors.errors().iter().map(AnalysisDiagnostic::rule_id).collect(),
        }
    }

    /// A body computing `a {op} b` from two `const` bindings of type `width`.
    fn binary(width: &str, a: &str, op: &str, b: &str) -> String {
        format!(
            "pub fn f() -> {width} {{ const a: {width} = {a}; const b: {width} = {b}; \
             return a {op} b; }}"
        )
    }

    #[test]
    fn every_operator_at_every_width_overflows_by_one() {
        for (width, min, max) in BOUNDS {
            assert_eq!(count_a052(&binary(width, max, "+", "1")), 1, "{width} +");
            assert_eq!(count_a052(&binary(width, min, "-", "1")), 1, "{width} -");
            assert_eq!(count_a052(&binary(width, max, "*", "2")), 1, "{width} *");
        }
        // Unary `-` exists only on the signed widths, and overflows at exactly
        // one value: the minimum, whose negation is one past the maximum.
        for (width, min) in SIGNED {
            let source =
                format!("pub fn f() -> {width} {{ const a: {width} = {min}; return -a; }}");
            assert_eq!(count_a052(&source), 1, "{width} unary -");
        }
    }

    #[test]
    fn a_result_landing_on_a_bound_is_not_an_overflow() {
        // The rule measures the result against the range, so the last value
        // inside it is a value like any other. An off-by-one in the comparison
        // would reject arithmetic that is exactly representable.
        for (width, min, max) in BOUNDS {
            assert_eq!(count_a052(&binary(width, max, "-", "0")), 0, "{width} max");
            assert_eq!(count_a052(&binary(width, min, "+", "0")), 0, "{width} min");
            assert_eq!(count_a052(&binary(width, max, "*", "1")), 0, "{width} max*1");
        }
        for (width, min) in SIGNED {
            // Negating the maximum is representable at every signed width; it is
            // the minimum that is not.
            let max = min.trim_start_matches('-').parse::<i128>().expect("bound") - 1;
            let source = format!(
                "pub fn f() -> {width} {{ const a: {width} = {max}; return -a; }}"
            );
            assert_eq!(count_a052(&source), 0, "{width} -max");
        }
    }

    #[test]
    fn a_runtime_operand_does_not_fold() {
        // The issue's own reproducer: a product that leaves `i64` at some
        // arguments and not at others. Nothing here is known before the program
        // runs, so the overflow is the emitted guard's to catch and this rule
        // has nothing to say about it.
        assert_eq!(
            count_a052("pub fn square(h: i64) -> i64 { return h * h; }"),
            0
        );
        assert_eq!(
            count_a052(
                "pub fn fixmul(a: i64, b: i64) -> i64 \
                 { const ONE: i64 = 1048576; return a * b / ONE; }"
            ),
            0
        );
        // One constant operand is not enough either.
        assert_eq!(
            count_a052(
                "pub fn f(a: i32) -> i32 { const big: i32 = 2147483647; return a + big; }"
            ),
            0
        );
    }

    #[test]
    fn a_let_binding_is_not_a_constant() {
        // `let` names a variable. Folding one would let this rule reject a
        // program on the strength of a value the language does not promise the
        // binding still has where it is read.
        let source = "pub fn f() -> i32 {
               let a: i32 = 2147483647;
               let b: i32 = 1;
               return a + b;
             }";
        assert_eq!(count_a052(source), 0);
    }

    #[test]
    fn a_wrapping_region_folds_modularly_and_is_exempt() {
        // Under a default that traps, this is the only way left to write a
        // constant that wraps, so the rule must let it through — and must let
        // the wrapped value through to whatever encloses it.
        for (width, _, max) in BOUNDS {
            let source = format!(
                "pub fn f() -> {width} {{ const a: {width} = {max}; const b: {width} = 1; \
                 return wrapping(a + b); }}"
            );
            assert_eq!(count_a052(&source), 0, "{width}");
        }
        // The wrapped value is a real value the enclosing checked operator
        // computes with: `i32::MAX + 1` wraps to `i32::MIN`, and adding one to
        // that is in range.
        assert_eq!(
            count_a052(
                "pub fn f() -> i32 { const a: i32 = 2147483647; const b: i32 = 1; \
                 return wrapping(a + b) + b; }"
            ),
            0
        );
        // And it is still a value that can overflow: `i32::MIN - 1` does.
        assert_eq!(
            count_a052(
                "pub fn f() -> i32 { const a: i32 = 2147483647; const b: i32 = 1; \
                 return wrapping(a + b) - b; }"
            ),
            1
        );
    }

    #[test]
    fn an_explicit_checked_annotation_is_no_escape() {
        // The rule reads the effective mode, so writing the mode the default
        // already gives changes nothing about the finding.
        assert_eq!(
            count_a052(
                "pub fn f() -> i32 { const a: i32 = 2147483647; return checked(a + 1); }"
            ),
            1
        );
        // Innermost wins in both directions: a checked region inside a wrapping
        // one is checked again.
        assert_eq!(
            count_a052(
                "pub fn f() -> i32 { const a: i32 = 2147483647; \
                 return wrapping(checked(a + 1)); }"
            ),
            1
        );
    }

    #[test]
    fn constants_fold_through_parentheses_annotations_and_nesting() {
        // Each shape the folder is documented to see through, one at a time,
        // over a `const` rather than a literal so the fold has to resolve a
        // name to reach the value.
        let sources = [
            "pub fn f() -> i32 { const a: i32 = 2147483647; return (a) + 1; }",
            "pub fn f() -> i32 { const a: i32 = 2147483647; return a + (1); }",
            "pub fn f() -> i32 { const a: i32 = 2147483647; return wrapping(a) + 1; }",
            "pub fn f() -> i32 { const a: i32 = 1073741824; return a * 2 + 1; }",
            "pub fn f() -> i32 { const a: i32 = 2147483647; const b: i32 = a; return b + 1; }",
        ];
        for source in sources {
            assert_eq!(count_a052(source), 1, "{source}");
        }
    }

    #[test]
    fn a_const_initializer_is_measured_where_it_is_written() {
        // The declaration is a statement of its own, so the finding is on it —
        // and the use site is silent, because a `const` that has no value is not
        // a constant the reader of `b` can be told about.
        let messages = a052_messages(
            "pub fn f() -> i32 {
               const a: i32 = 2147483647;
               const b: i32 = a + 1;
               return b;
             }",
        );
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert!(messages[0].contains("`a + 1` overflows `i32`"), "{messages:?}");
    }

    #[test]
    fn a_chain_reports_its_innermost_overflow_once() {
        // The outer operator has no constant to compute with, which is exactly
        // true: the program never reaches it.
        let messages = a052_messages(
            "pub fn f() -> i32 { const a: i32 = 2147483647; return a + 1 + 1; }",
        );
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert!(messages[0].contains("`a + 1` overflows `i32`"), "{messages:?}");
    }

    #[test]
    fn a_literal_out_of_range_stays_a022s() {
        // The handoff: `300` is not a `u8`, and that is one finding about one
        // literal. A rule that also reported the addition would be describing
        // the same mistake twice, in a sentence claiming both operands fit.
        let source = "pub fn f() -> u8 { let x: u8 = 300 + 1; return x; }";
        assert_eq!(count_a052(source), 0);
        assert_eq!(rule_ids(source), vec!["A022"]);
    }

    #[test]
    fn the_message_names_the_expression_the_operands_and_both_results() {
        let messages = a052_messages(
            "pub fn f() -> i32 { const max: i32 = 2147483647; return max + 1; }",
        );
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert_eq!(
            messages[0],
            "`max + 1` overflows `i32` before the program runs; its operands fold to the \
             constants `2147483647` and `1`, each of which is itself within \
             `-2147483648..=2147483647`, and it is their `+` that is not — the true result \
             `2147483648` is outside that range — so this is not a value the program computes \
             but a trap it takes on every run that reaches it, and no `assume`, envelope or \
             specification can recover a result the type cannot hold: if the wrap is what you \
             meant, write `wrapping(max + 1)`, which computes `-2147483648` and does not trap, \
             and otherwise the operands or the declared type have to change"
        );
    }

    #[test]
    fn a_negation_is_told_about_its_one_operand() {
        let messages =
            a052_messages("pub fn f() -> i8 { const min: i8 = -128; return -min; }");
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert_eq!(
            messages[0],
            "`-min` overflows `i8` before the program runs; its operand folds to the constant \
             `-128`, which is itself within `-128..=127`, and it is the `-` applied to it that \
             is not — the true result `128` is outside that range — so this is not a value the \
             program computes but a trap it takes on every run that reaches it, and no \
             `assume`, envelope or specification can recover a result the type cannot hold: if \
             the wrap is what you meant, write `wrapping(-min)`, which computes `-128` and does \
             not trap, and otherwise the operand or the declared type has to change"
        );
    }

    #[test]
    fn a_body_that_becomes_a_term_has_no_run_to_trap() {
        // A `forall`-quantified specification body and an unquantified one are
        // turned into obligation terms and never lowered — code generation
        // emits no instruction for either, in either mode — so the finding this
        // rule states, a trap taken on every run that reaches the operation, is
        // false of them and they are skipped. The unquantified case is the one
        // a reader thinks of as a helper `fn` written inside the `spec` block:
        // it is a specification function like any other, it produces an
        // obligation of its own, and no code is emitted for it.
        for quantifier in ["forall", ""] {
            let source = format!(
                "pub fn main() -> i32 {{ return 0; }}
                 spec S {{
                   fn claim(x: i32) {quantifier} {{
                     const a: i32 = 2147483647;
                     assert(a + 1 == x);
                   }}
                 }}"
            );
            assert_eq!(count_a052(&source), 0, "{quantifier:?}");
        }
    }

    #[test]
    fn a_retained_body_is_compiled_and_is_examined() {
        // An `exists`/`unique` body is the specification body that *is*
        // lowered and reduced, so an operation whose result leaves its type
        // does trap on the path the judgment reduces. Analysis is asked here
        // directly, without code generation, so the proof-mode rules that also
        // refuse this body cannot stand in for the finding.
        for quantifier in ["exists", "unique"] {
            let unmarked = format!(
                "pub fn main() -> i32 {{ return 0; }}
                 spec S {{
                   fn claim(lo: i32) {quantifier} {{
                     let n: i32 = @;
                     const a: i32 = 2147483647;
                     assume {{ assert(n >= lo); }}
                     assert(a + 1 == n);
                   }}
                 }}"
            );
            assert_eq!(count_a052(&unmarked), 1, "{quantifier} unmarked");
            // `wrapping(...)` is what the reachability rules require of such a
            // body in any case, and it folds modularly here for the same reason
            // it folds modularly everywhere.
            let marked = unmarked.replace("a + 1", "wrapping(a + 1)");
            assert_eq!(count_a052(&marked), 0, "{quantifier} wrapping");
        }
    }

    #[test]
    fn a_function_a_specification_calls_is_still_executable() {
        // The carve-out is about where the body is declared and what its own
        // quantifier is, not about what calls it: a function outside the `spec`
        // block is compiled and exported whether or not a specification names
        // it, so its constant overflow is a trap like any other.
        let source = "pub fn helper() -> i32 { const a: i32 = 2147483647; return a + 1; }
             pub fn main() -> i32 { return helper(); }
             spec S {
               fn claim() forall { assert(helper() == helper()); }
             }";
        assert_eq!(count_a052(source), 1);
    }

    #[test]
    fn the_ungoverned_operators_are_not_this_rules_business() {
        // `/`, `%`, the shifts and the bitwise operators cannot leave their
        // type, so no constant written with one is an overflow.
        for op in ["/", "%", "&", "|", "^", "<<", ">>"] {
            let source = format!(
                "pub fn f() -> i32 {{ const a: i32 = 2147483647; const b: i32 = 2; \
                 return a {op} b; }}"
            );
            assert_eq!(count_a052(&source), 0, "{op}");
        }
    }

    /// A rejected program never reaches code generation, and the way to observe
    /// that is the analysis-running codegen path — not `try_codegen`, which
    /// unwraps the analysis result outside the panic boundary.
    #[test]
    fn a_rejected_program_does_not_reach_code_generation() {
        let source = "pub fn f() -> i32 { const max: i32 = 2147483647; return max + 1; }";
        match codegen_attempt(source, AnalysisMode::Run) {
            CodegenAttempt::Rejected(errors) => assert!(
                errors.contains("error[A052]"),
                "the rejection must be this rule's, got: {errors}"
            ),
            CodegenAttempt::Ok(_) => panic!("A052 must reject this program"),
            CodegenAttempt::Panicked(payload) => panic!("unexpected panic: {payload}"),
        }
        // The two shapes the boundary fixture uses instead, both of which have
        // to compile: the same arithmetic written modularly, and the same
        // operation over parameters, where nothing folds.
        for accepted in [
            "pub fn f() -> i32 { const max: i32 = 2147483647; return wrapping(max + 1); }",
            "pub fn f(a: i32, b: i32) -> i32 { return a + b; }",
        ] {
            assert!(
                matches!(
                    codegen_attempt(accepted, AnalysisMode::Run),
                    CodegenAttempt::Ok(_)
                ),
                "{accepted}"
            );
        }
    }
}
