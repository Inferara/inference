//! A compact bitset of [`SyntaxKind`] token kinds.
//!
//! [`TokenSet`] backs the parser's recovery and expectation sets: "stop
//! recovering when you reach one of these tokens", "this construct may start with
//! one of these tokens". Membership is a single bit test.
//!
//! The set is a `u128` indexed by discriminant, so it only holds kinds whose
//! discriminant is `< 128`. By construction every token kind satisfies this (see
//! [`crate::syntax_kind`] discriminant layout); node kinds must never be inserted.

use crate::syntax_kind::SyntaxKind;

/// An O(1)-membership set of token [`SyntaxKind`]s, stored as a `u128` bitset.
#[derive(Clone, Copy)]
pub struct TokenSet(u128);

impl TokenSet {
    /// An empty set.
    pub const EMPTY: TokenSet = TokenSet(0);

    /// Builds a set from the given token kinds.
    ///
    /// Every kind's discriminant must be `< 128`, which holds for all token
    /// kinds; passing a node kind is a programming error and panics in `const`
    /// evaluation via the shift overflow.
    #[must_use]
    pub const fn new(kinds: &[SyntaxKind]) -> TokenSet {
        let mut bits = 0u128;
        let mut i = 0;
        while i < kinds.len() {
            bits |= mask(kinds[i]);
            i += 1;
        }
        TokenSet(bits)
    }

    /// The union of two sets.
    #[must_use]
    pub const fn union(self, other: TokenSet) -> TokenSet {
        TokenSet(self.0 | other.0)
    }

    /// Whether `kind` is a member.
    #[must_use]
    pub const fn contains(self, kind: SyntaxKind) -> bool {
        self.0 & mask(kind) != 0
    }
}

/// The single-bit mask for a token kind's discriminant.
///
/// Only token kinds belong in a [`TokenSet`]: a node kind would shift past the
/// `u128`. Callers guarantee tokens; [`SyntaxKind::is_token`] is the predicate.
const fn mask(kind: SyntaxKind) -> u128 {
    1u128 << (kind as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_contains_nothing() {
        assert!(!TokenSet::EMPTY.contains(SyntaxKind::Plus));
        assert!(!TokenSet::EMPTY.contains(SyntaxKind::Ident));
    }

    #[test]
    fn new_contains_members_only() {
        let set = TokenSet::new(&[SyntaxKind::Plus, SyntaxKind::Minus, SyntaxKind::Semi]);
        assert!(set.contains(SyntaxKind::Plus));
        assert!(set.contains(SyntaxKind::Minus));
        assert!(set.contains(SyntaxKind::Semi));
        assert!(!set.contains(SyntaxKind::Star));
        assert!(!set.contains(SyntaxKind::Ident));
    }

    #[test]
    fn union_merges_members() {
        let a = TokenSet::new(&[SyntaxKind::Plus, SyntaxKind::Minus]);
        let b = TokenSet::new(&[SyntaxKind::Star, SyntaxKind::Slash]);
        let both = a.union(b);
        assert!(both.contains(SyntaxKind::Plus));
        assert!(both.contains(SyntaxKind::Minus));
        assert!(both.contains(SyntaxKind::Star));
        assert!(both.contains(SyntaxKind::Slash));
        assert!(!both.contains(SyntaxKind::Percent));
    }

    #[test]
    fn union_is_usable_in_const() {
        const RECOVERY: TokenSet = TokenSet::new(&[SyntaxKind::Semi])
            .union(TokenSet::new(&[SyntaxKind::RBrace, SyntaxKind::Eof]));
        assert!(RECOVERY.contains(SyntaxKind::Semi));
        assert!(RECOVERY.contains(SyntaxKind::RBrace));
        assert!(RECOVERY.contains(SyntaxKind::Eof));
        assert!(!RECOVERY.contains(SyntaxKind::Comma));
    }

    #[test]
    fn all_token_discriminants_fit_u128() {
        // Every kind below the first node kind is a token and must be a valid
        // bit index, so it can be stored in a `TokenSet`.
        let last_token = SyntaxKind::Eof;
        assert!((last_token as u16) < 128);
        let set = TokenSet::new(&[last_token]);
        assert!(set.contains(SyntaxKind::Eof));
    }
}
