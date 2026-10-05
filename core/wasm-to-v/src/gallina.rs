//! Gallina lexical helpers shared by the two emitters that render Rocq terms:
//! the WASM instruction translator ([`crate::translator`]) and the `hassert`
//! obligation printer ([`crate::hassert_print`]).
//!
//! Both spell integer constants through [`z_literal`], so the parenthesization
//! rule below has exactly one implementation. It previously had two, and only
//! one of them was right (#314).

/// Renders a signed integer as a Gallina `Z` literal in *term* position.
///
/// Gallina's `-` is an infix operator, so an unparenthesized negative literal
/// in argument position reads as a subtraction: `Vi32 -1` parses as the
/// application-free expression `Vi32 - 1`, which fails to type-check. Negative
/// values therefore carry their own parentheses; non-negative values render
/// bare, where no ambiguity exists.
pub(crate) fn z_literal(value: i64) -> String {
    if value < 0 {
        format!("({value})")
    } else {
        value.to_string()
    }
}

/// Escapes `text` for use inside a Gallina string literal.
///
/// Doubling `"` is Coq's own — and only — string escape. The names this covers
/// are WASM import and export names, which are *data*: the emitted
/// `list_byte_of_string` must round-trip them byte for byte, so nothing else
/// may be rewritten. Doubling is exactly reversible and leaves every other byte
/// untouched, which is why this is not
/// [`crate::rocq_names::sanitize_rocq_identifier`] — that one rewrites for
/// legality as an *identifier* and would change the bytes the name denotes.
///
/// Without the escape, a name carrying `"` closes the literal early and its
/// remainder is read as Gallina. An export named
/// `a" (MED_func 99%N) :: Me "b` emitted two `MED_func` entries from one
/// export: an export in the proof artifact that the module does not have.
#[must_use]
pub(crate) fn escape_string_literal(text: &str) -> String {
    text.replace('"', "\"\"")
}

/// Makes `text` safe to emit as the body of a `(*{text}*)` comment, the frame
/// the translator writes local names into.
///
/// Three hazards can break out of that comment, and each one has a rewrite:
///
/// - **`*)`** closes the comment early and lets whatever follows be read as
///   Gallina: a local named `*) :: BI_unreachable :: (*` injected a
///   `BI_unreachable` the `.wasm` does not contain. It becomes `* )`.
/// - **`(*`** opens a nested comment. Coq comments nest, so an unbalanced
///   opener swallows the rest of the file. It becomes `( *`.
/// - **`"`** opens a string literal, because Coq lexes strings inside
///   comments too, and a `*)` inside that string does not end the comment.
///   Two locals named `a"` and `b"` turned everything between them into one
///   comment: the file still compiled, and the body silently lost the
///   instructions in between. Each quote is doubled, Coq's own escape, so every
///   run of quotes has even length and lexes as complete, balanced strings.
///
/// The passes are order-safe. Doubling quotes never creates or breaks a
/// delimiter. Rewriting `(*` can leave a `*)` behind (from `(*)`), which the
/// next pass catches, and that pass only ever inserts a space before `)`, so it
/// can never create a new `(*`.
///
/// The frame's own edges need one more rule. After the rewrites, no `(*` or
/// `*)` lies wholly inside the text, so a delimiter could only form across an
/// edge. At the front, the opener's `*` and the text's first character can
/// only form `*)`, which Coq does not read as a closer straight after `(*`. At
/// the back, a trailing `(` and the closer's `*` form `(*`, a nested opener
/// that leaves the comment unclosed, so a trailing `(` gets a space after it.
/// Checked exhaustively against `coqc` for every name over `( * ) " x` up to
/// length 6.
///
/// Deliberately not [`crate::rocq_names::sanitize_rocq_identifier`]: a local
/// name is comment prose, not an identifier. That function collapses `__` runs
/// and forces an alphabetic start, so `__frame_ptr` — which codegen emits for
/// every array-using program — would render as `f_frame_ptr` and move every
/// byte-compared `.v` golden. Codegen's local names are plain identifiers,
/// with no `(`, `*`, `)` or `"`, so they pass through unchanged.
#[must_use]
pub(crate) fn neutralize_comment_delimiters(text: &str) -> String {
    let mut out = text
        .replace('"', "\"\"")
        .replace("(*", "( *")
        .replace("*)", "* )");
    if out.ends_with('(') {
        out.push(' ');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{escape_string_literal, neutralize_comment_delimiters, z_literal};

    #[test]
    fn non_negative_literals_render_bare() {
        assert_eq!(z_literal(0), "0");
        assert_eq!(z_literal(1), "1");
        assert_eq!(z_literal(i64::from(i32::MAX)), "2147483647");
        assert_eq!(z_literal(i64::MAX), "9223372036854775807");
    }

    #[test]
    fn negative_literals_are_parenthesized() {
        assert_eq!(z_literal(-1), "(-1)");
        assert_eq!(z_literal(i64::from(i32::MIN)), "(-2147483648)");
        assert_eq!(z_literal(i64::MIN), "(-9223372036854775808)");
    }

    /// The escape is exactly Coq's: a quote doubles, and nothing else moves.
    #[test]
    fn quotes_double_and_nothing_else_changes() {
        assert_eq!(escape_string_literal("plain"), "plain");
        assert_eq!(escape_string_literal(r#"a"b"#), r#"a""b"#);
        assert_eq!(escape_string_literal(r#""""#), r#""""""#);
        // Every other byte a WASM name may carry is data and must survive.
        assert_eq!(
            escape_string_literal("env.mem_(*x*)\\n"),
            "env.mem_(*x*)\\n"
        );
    }

    /// A backslash is not an escape in a Gallina string literal: `"say\"` is
    /// the four bytes `say\`, closed by its final quote. Escaping it would
    /// change the name the literal denotes, so it passes through, including
    /// when it sits right before a quote that does get doubled.
    #[test]
    fn a_backslash_is_data_not_an_escape() {
        assert_eq!(escape_string_literal(r"a\"), r"a\");
        assert_eq!(escape_string_literal(r#"\""#), r#"\"""#);
        assert_eq!(escape_string_literal(r"\\"), r"\\");
    }

    /// Both delimiters are broken, in any arrangement, including the ones where
    /// neutralizing one could otherwise produce the other.
    #[test]
    fn comment_delimiters_are_neutralized() {
        assert_eq!(neutralize_comment_delimiters("plain"), "plain");
        assert_eq!(neutralize_comment_delimiters("(*"), "( *");
        assert_eq!(neutralize_comment_delimiters("*)"), "* )");
        assert_eq!(neutralize_comment_delimiters("(*)"), "( * )");
        assert_eq!(neutralize_comment_delimiters("(**)"), "( ** )");
        assert_eq!(neutralize_comment_delimiters("*)(*"), "* )( *");
        let out = neutralize_comment_delimiters("*) :: BI_unreachable :: (*");
        assert!(!out.contains("(*") && !out.contains("*)"), "{out}");
    }

    /// A quote opens a string even inside a comment, so each one doubles into
    /// an even run, alongside the delimiter rewrites.
    #[test]
    fn quotes_in_comment_text_double() {
        assert_eq!(neutralize_comment_delimiters(r#"a""#), r#"a"""#);
        assert_eq!(neutralize_comment_delimiters(r#"""#), r#""""#);
        assert_eq!(neutralize_comment_delimiters(r#""*)"#), r#"""* )"#);
        assert_eq!(neutralize_comment_delimiters(r#"(*"(*"#), r#"( *""( *"#);
    }

    /// The emitter writes `(*{name}*)`, so a trailing `(` would pair with the
    /// closer's `*` into a nested opener. Nothing else at either edge can form
    /// a delimiter, so nothing else is padded.
    #[test]
    fn a_trailing_open_paren_is_padded_and_nothing_else() {
        assert_eq!(neutralize_comment_delimiters("x("), "x( ");
        assert_eq!(neutralize_comment_delimiters("("), "( ");
        assert_eq!(neutralize_comment_delimiters("(*("), "( *( ");
        assert_eq!(neutralize_comment_delimiters("(x"), "(x");
        for unpadded in [")", "*", "x*", "*x", ")x", ""] {
            assert_eq!(neutralize_comment_delimiters(unpadded), unpadded);
        }
    }

    /// The name codegen emits for every array-using program must render
    /// unchanged: routing local names through identifier sanitization would
    /// rewrite it and move every byte-compared `.v` golden.
    #[test]
    fn frame_pointer_local_is_untouched() {
        assert_eq!(neutralize_comment_delimiters("__frame_ptr"), "__frame_ptr");
    }
}
