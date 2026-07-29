//! Cross-layer tests for the parser's malformed-numeric-literal diagnostics.
//!
//! The grammar rejects `16i64`, `1_000` and `0x1F` with one teaching message
//! each and consumes the offending tail into the literal node. The tail is part
//! of the node's span but *not* part of the literal's value, and this module
//! pins that separation end to end: a recovered parse must flow through the type
//! checker and analysis without any second diagnostic blaming the literal.
//!
//! The regression this guards is specific. Had lowering kept reading the node
//! span, `Expr::NumberLiteral.value` would be `"16i64"`, which fails A022's
//! `i128` parse and is reported as "out of range" — a wrong, confusing second
//! error next to a correct first one.

#[cfg(test)]
mod tests {
    use inference_analysis::errors::AnalysisDiagnostic;
    use inference_ast::arena::AstArena;
    use inference_ast::nodes::Expr;

    /// The parse errors for `source`, as messages in source order.
    fn parse_messages(source: &str) -> Vec<String> {
        inference_parser::parse(source)
            .errors
            .into_iter()
            .map(|e| e.message)
            .collect()
    }

    /// Type-checks and analyzes an arena recovered from a parse that reported
    /// errors, returning the analysis diagnostics (empty when analysis passes).
    fn analyze_recovered(arena: AstArena) -> Vec<AnalysisDiagnostic> {
        let ctx = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .expect("the recovered arena should still type-check")
            .typed_context();
        match inference_analysis::analyze(&ctx) {
            Ok(_) => Vec::new(),
            Err(errors) => errors.errors().to_vec(),
        }
    }

    /// Every `NumberLiteral` value in `arena`, in allocation order.
    fn literal_values(arena: &AstArena) -> Vec<&str> {
        arena
            .exprs
            .iter()
            .filter_map(|(_, expr)| match &expr.kind {
                Expr::NumberLiteral { value } => Some(value.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn suffixed_literal_produces_only_the_parser_diagnostic() {
        // The whole point of the repair: one message, from one layer. A022 must
        // not pile a spurious "out of range" on top, and the type checker must
        // not reject the literal — `16` is a fine `i64` return value.
        let source = "pub fn f() -> i64 { return 16i64; }";
        let messages = parse_messages(source);
        assert_eq!(
            messages.len(),
            1,
            "expected one parse error, got {messages:?}"
        );
        assert!(
            messages[0].contains("do not take a type suffix"),
            "expected the suffix teaching message, got {:?}",
            messages[0]
        );

        let diagnostics = analyze_recovered(inference_parser::parse(source).arena);
        assert!(
            diagnostics.is_empty(),
            "a suffixed literal must not draw any analysis diagnostic, got {diagnostics:?}"
        );
    }

    #[test]
    fn suffixed_literal_keeps_its_value_out_of_range_checkable() {
        // A literal that IS out of range must still be caught: the digits-only
        // value keeps A022 working rather than disabling it near a parse error.
        let source = "pub fn f() -> u8 { return 300u8; }";
        let messages = parse_messages(source);
        assert_eq!(
            messages.len(),
            1,
            "expected one parse error, got {messages:?}"
        );

        let diagnostics = analyze_recovered(inference_parser::parse(source).arena);
        assert!(
            diagnostics
                .iter()
                .any(|d| matches!(d, AnalysisDiagnostic::LiteralOutOfRange { .. })),
            "300 does not fit u8; A022 must still fire, got {diagnostics:?}"
        );
    }

    #[test]
    fn separator_literal_keeps_only_the_digits_before_the_underscore() {
        // `1_000` is the silent-split trap: the value really is `1`, and saying
        // so is why the parser must reject the spelling instead of compiling it.
        let source = "pub fn f() -> i32 { return 1_000; }";
        let messages = parse_messages(source);
        assert_eq!(
            messages.len(),
            1,
            "expected one parse error, got {messages:?}"
        );
        assert!(
            messages[0].contains("decimal digits only"),
            "expected the decimal-digits message, got {:?}",
            messages[0]
        );

        let arena = inference_parser::parse(source).arena;
        assert_eq!(literal_values(&arena), vec!["1"]);
        assert!(analyze_recovered(arena).is_empty());
    }

    #[test]
    fn hex_shaped_literal_keeps_only_the_leading_zero() {
        let source = "pub fn f() -> i32 { return 0x1F; }";
        let messages = parse_messages(source);
        assert_eq!(
            messages.len(),
            1,
            "expected one parse error, got {messages:?}"
        );
        assert!(
            messages[0].contains("decimal digits only"),
            "expected the decimal-digits message, got {:?}",
            messages[0]
        );

        let arena = inference_parser::parse(source).arena;
        assert_eq!(literal_values(&arena), vec!["0"]);
        assert!(analyze_recovered(arena).is_empty());
    }

    /// A spec body with a non-deterministic `forall` and an `assume`, the shape
    /// the literal-in-a-proof-obligation risk actually takes. `{literal}` is
    /// substituted into the `assume`'s assertion.
    fn spec_with_literal(literal: &str) -> String {
        format!(
            "pub fn f(a: i64) -> i64 {{ return a; }} \
             spec S {{ fn p() forall {{ let n: i64 = @; \
             assume {{ assert(n > {literal}); }} assert(n > 0); }} }}"
        )
    }

    #[test]
    fn malformed_literal_in_a_spec_body_draws_no_analysis_diagnostic() {
        // Spec bodies are where a poisoned value is worst: the translator turns
        // literals into constants inside proof obligations, so a wrong constant
        // makes the proof about a different program than the one that runs.
        // Analysis reaches spec bodies (the sibling test below proves it), so a
        // `"16i64"` value here would surface as a spurious range error next to
        // the parser's correct one.
        let source = spec_with_literal("16i64");
        let messages = parse_messages(&source);
        assert_eq!(
            messages.len(),
            1,
            "expected one parse error, got {messages:?}"
        );
        assert!(messages[0].contains("do not take a type suffix"));

        let arena = inference_parser::parse(&source).arena;
        assert_eq!(literal_values(&arena), vec!["16", "0"]);
        assert!(
            analyze_recovered(arena).is_empty(),
            "a malformed literal in a spec body must not draw analysis diagnostics"
        );
    }

    #[test]
    fn analysis_reaches_literals_inside_a_spec_forall_assume() {
        // Reachability control for the test above: without this, "no diagnostic
        // in a spec body" could pass simply because nothing looks there. A
        // well-formed but out-of-range literal in the same position must fire.
        let source = spec_with_literal("300").replace("i64", "u8");
        assert!(
            parse_messages(&source).is_empty(),
            "the control source must parse cleanly"
        );

        let diagnostics = analyze_recovered(inference_parser::parse(&source).arena);
        assert!(
            diagnostics
                .iter()
                .any(|d| matches!(d, AnalysisDiagnostic::LiteralOutOfRange { .. })),
            "A022 must reach a spec forall/assume body, got {diagnostics:?}"
        );
    }

    #[test]
    fn separator_literal_in_a_spec_body_keeps_only_the_leading_digits() {
        let source = "spec S { fn p(a: i64) -> bool { return a > 1_000; } }";
        let messages = parse_messages(source);
        assert_eq!(
            messages.len(),
            1,
            "expected one parse error, got {messages:?}"
        );

        let arena = inference_parser::parse(source).arena;
        assert_eq!(literal_values(&arena), vec!["1"]);
        assert!(analyze_recovered(arena).is_empty());
    }

    #[test]
    fn suffixed_i64_min_survives_end_to_end() {
        // `i64::MIN` is the one boundary where a downstream `parse` can overflow
        // if the value is anything but exact, and the glued `-` makes it a
        // single token whose tail still has to be split off correctly.
        let source = "pub fn f() -> i64 { return -9223372036854775808i64; }";
        let messages = parse_messages(source);
        assert_eq!(
            messages.len(),
            1,
            "expected one parse error, got {messages:?}"
        );
        assert!(messages[0].contains("do not take a type suffix"));

        let arena = inference_parser::parse(source).arena;
        assert_eq!(literal_values(&arena), vec!["-9223372036854775808"]);
        assert!(
            analyze_recovered(arena).is_empty(),
            "i64::MIN is in range for i64; no range diagnostic is owed"
        );
    }

    #[test]
    fn well_formed_literal_reaches_analysis_with_no_diagnostics() {
        // Negative control for the whole path: the same shapes without the
        // malformed tail parse, type-check and analyze cleanly.
        let source = "pub fn f() -> i64 { return 16; }";
        assert!(parse_messages(source).is_empty());

        let arena = inference_parser::parse(source).arena;
        assert_eq!(literal_values(&arena), vec!["16"]);
        assert!(analyze_recovered(arena).is_empty());
    }
}
