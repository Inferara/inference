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

/// Neutralizes Coq comment delimiters in `text` so it can be emitted inside a
/// `(* … *)` comment without breaking out of it.
///
/// A space is inserted between the two characters of each delimiter — `(*`
/// becomes `( *`, `*)` becomes `* )` — which is the smallest edit that stops
/// them pairing. Coq comments nest, so an unbalanced `(*` swallows the rest of
/// the file, and a `*)` closes the comment early and lets whatever follows be
/// read as Gallina: a local named `*) :: BI_unreachable :: (*` injected a
/// `BI_unreachable` the `.wasm` does not contain.
///
/// The two passes are order-safe. Rewriting `(*` can leave a `*)` behind (from
/// `(*)`), which the second pass then catches; the second pass only ever
/// inserts a space before `)`, so it can never create a new `(*`.
///
/// Deliberately not [`crate::rocq_names::sanitize_rocq_identifier`]: a local
/// name is comment prose, not an identifier. That function collapses `__` runs
/// and forces an alphabetic start, so `__frame_ptr` — which codegen emits for
/// every array-using program — would render as `f_frame_ptr` and move every
/// byte-compared `.v` golden. Only the two delimiters are touched.
#[must_use]
pub(crate) fn neutralize_comment_delimiters(text: &str) -> String {
    text.replace("(*", "( *").replace("*)", "* )")
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

    /// The name codegen emits for every array-using program must render
    /// unchanged: routing local names through identifier sanitization would
    /// rewrite it and move every byte-compared `.v` golden.
    #[test]
    fn frame_pointer_local_is_untouched() {
        assert_eq!(neutralize_comment_delimiters("__frame_ptr"), "__frame_ptr");
    }
}
