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

#[cfg(test)]
mod tests {
    use super::z_literal;

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
}
