/// Integration tests for analysis rule A054.
///
/// - A054: `ArithModeChangesNothing` — an arithmetic-mode annotation naming the
///   mode already in force where it is written changes no arithmetic.
///
/// A **warning**, so `analyze` returns `Ok` and the program still compiles.
/// Which spelling is redundant is decided by the language's default, so every
/// case here is written against that default rather than against a hard-coded
/// spelling: the same source is redundant under one default and meaningful under
/// the other, and a suite that named the spelling would have to be rewritten
/// rather than re-measured whenever the default moves.
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::{AnalysisMode, CodegenAttempt, build_ast, codegen_attempt};
    use inference_analysis::errors::AnalysisDiagnostic;
    use inference_ast::nodes::ArithMode;
    use inference_type_checker::typed_context::TypedContext;

    /// The spelling that names the mode unannotated arithmetic already has, and
    /// therefore the one that can be redundant.
    fn redundant_spelling() -> &'static str {
        ArithMode::DEFAULT.spelling()
    }

    /// The other spelling, which changes something wherever it is written
    /// outside an annotation of its own.
    fn meaningful_spelling() -> &'static str {
        if ArithMode::DEFAULT == ArithMode::Checked {
            "wrapping"
        } else {
            "checked"
        }
    }

    fn type_check(source: &str) -> TypedContext {
        let arena = build_ast(source.to_string());
        inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .expect("type checking should succeed for analysis test input")
            .typed_context()
    }

    /// The A054 messages the analysis emits, in report order. The rule is a
    /// warning, so a clean `Ok` still carries them.
    fn a054_messages(source: &str) -> Vec<String> {
        let result = inference_analysis::analyze(&type_check(source))
            .unwrap_or_else(|e| panic!("A054 is a warning, so nothing here is rejected: {e}"));
        result
            .warnings()
            .iter()
            .filter(|w| matches!(w, AnalysisDiagnostic::ArithModeChangesNothing { .. }))
            .map(ToString::to_string)
            .collect()
    }

    fn count_a054(source: &str) -> usize {
        a054_messages(source).len()
    }

    /// The rule ids of every A054-shaped warning, so a test can pin the code the
    /// message itself does not carry.
    fn a054_rule_ids(source: &str) -> Vec<&'static str> {
        let result = inference_analysis::analyze(&type_check(source))
            .unwrap_or_else(|e| panic!("A054 is a warning, so nothing here is rejected: {e}"));
        result
            .warnings()
            .iter()
            .filter(|w| matches!(w, AnalysisDiagnostic::ArithModeChangesNothing { .. }))
            .map(AnalysisDiagnostic::rule_id)
            .collect()
    }

    #[test]
    fn a_top_level_annotation_naming_the_default_is_redundant() {
        let source = format!(
            "pub fn f(a: i32, b: i32) -> i32 {{ return {}(a + b); }}",
            redundant_spelling()
        );
        assert_eq!(a054_rule_ids(&source), vec!["A054"]);
        let messages = a054_messages(&source);
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert!(
            messages[0].contains("restates the default rather than restoring it"),
            "a top-level redundancy is stated against the default: {:?}",
            messages[0]
        );
    }

    #[test]
    fn a_top_level_annotation_naming_the_other_mode_changes_something() {
        let source = format!(
            "pub fn f(a: i32, b: i32) -> i32 {{ return {}(a + b); }}",
            meaningful_spelling()
        );
        assert_eq!(count_a054(&source), 0);
    }

    #[test]
    fn an_inner_annotation_that_restores_the_enclosing_mode_changes_something() {
        // `wrapping(a * checked(b + c))` under the checked default, and its
        // mirror under the other one: the outer changes the `*`, and the inner
        // restores the enclosing mode inside a region the outer had left.
        let source = format!(
            "pub fn f(a: i32, b: i32, c: i32) -> i32 {{ return {}(a * {}(b + c)); }}",
            meaningful_spelling(),
            redundant_spelling()
        );
        assert_eq!(count_a054(&source), 0, "{source}");
    }

    #[test]
    fn a_stack_of_same_mode_annotations_reports_its_outermost_member_once() {
        // One finding per stack, at every depth: removing the outermost is the
        // edit the author has to make first, and the next compile reports what
        // is still redundant below it. The three-deep case is the one that
        // separates transitive suppression from a rule that only looks at
        // whether its immediate parent produced a finding — under the latter
        // the middle layer is silently skipped and the innermost is reported as
        // a second, unrelated-looking finding.
        let spelling = redundant_spelling();
        for depth in 2..=3 {
            let opened: String = (0..depth).map(|_| format!("{spelling}(")).collect();
            let closed: String = (0..depth).map(|_| ")").collect();
            let source =
                format!("pub fn f(a: i32, b: i32) -> i32 {{ return {opened}a + b{closed}; }}");
            let messages = a054_messages(&source);
            assert_eq!(messages.len(), 1, "depth {depth}: {messages:?}");
            assert!(
                messages[0].contains("restates the default"),
                "depth {depth}: the reported one is the outermost: {:?}",
                messages[0]
            );
        }
    }

    #[test]
    fn an_annotation_redundant_against_an_enclosing_one_states_that_annotation() {
        // The inner is redundant against the outer, which is itself meaningful,
        // so nothing suppresses the inner finding — and its message must say
        // nothing about the default, which is not what it is redundant against.
        let spelling = meaningful_spelling();
        let source = format!(
            "pub fn f(a: i32, b: i32) -> i32 {{ return {spelling}(a * {spelling}(a + b)); }}"
        );
        let messages = a054_messages(&source);
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert!(
            messages[0].contains("it is written inside already puts every operator here in that \
                                  mode"),
            "{:?}",
            messages[0]
        );
        assert!(
            !messages[0].contains("restates the default"),
            "{:?}",
            messages[0]
        );
    }

    #[test]
    fn an_annotation_with_nothing_to_govern_is_left_to_a053() {
        // Both rules would fire on this shape. "It governs nothing" is the fact
        // the author needs, so A054 steps aside and the program is rejected by
        // A053 rather than warned about twice.
        let source = format!(
            "pub fn g(x: i32) -> i32 {{ return x; }}
             pub fn f(x: i32) -> i32 {{ return {}(g(x)); }}",
            redundant_spelling()
        );
        match codegen_attempt(&source, AnalysisMode::Run) {
            CodegenAttempt::Rejected(errors) => {
                assert!(errors.contains("error[A053]"), "{errors}");
                assert!(
                    !errors.contains("A054"),
                    "A054 must not stack on an A053 finding: {errors}"
                );
            }
            CodegenAttempt::Ok(_) => panic!("A053 must reject this program"),
            CodegenAttempt::Panicked(payload) => panic!("unexpected panic: {payload}"),
        }
    }

    #[test]
    fn a_redundant_annotation_still_compiles() {
        // The whole reason this is a warning: source written against the
        // previous default keeps building.
        let source = format!(
            "pub fn f(a: i32, b: i32) -> i32 {{ return {}(a + b); }}",
            redundant_spelling()
        );
        assert!(matches!(
            codegen_attempt(&source, AnalysisMode::Run),
            CodegenAttempt::Ok(_)
        ));
    }

    #[test]
    fn a_body_level_const_initializer_is_held_to_the_rule() {
        // A `const` inside a function body is a statement whose initializer is
        // an ordinary expression, and the scan reaches it like any other: the
        // annotation governs a real `+` and names the mode already in force, so
        // it is redundant exactly where the same text would be redundant in a
        // `return`. The file-scope form is out of reach — the language has no
        // such declaration yet and A032 rejects it — which is what
        // `rules_a053` pins.
        let source = format!(
            "pub fn f(a: i32, b: i32) -> i32 {{
               const K: i32 = {}(1 + 2);
               return a + b + K;
             }}",
            redundant_spelling()
        );
        assert_eq!(a054_rule_ids(&source), vec!["A054"]);
    }

    #[test]
    fn every_width_answers_the_same_way() {
        for width in ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64"] {
            let redundant = format!(
                "pub fn f(a: {width}, b: {width}) -> {width} {{ return {}(a + b); }}",
                redundant_spelling()
            );
            assert_eq!(count_a054(&redundant), 1, "{width}");
            let meaningful = format!(
                "pub fn f(a: {width}, b: {width}) -> {width} {{ return {}(a + b); }}",
                meaningful_spelling()
            );
            assert_eq!(count_a054(&meaningful), 0, "{width}");
        }
    }
}
