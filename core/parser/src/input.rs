//! A trivia-free view of the lexer's token stream for the parser engine.
//!
//! The lexer emits a lossless stream that keeps whitespace and comments as
//! trivia tokens (issue #62 design §4). The recursive-descent parser never wants
//! to see trivia, so [`Input`] precomputes the indices of the non-trivia tokens
//! and exposes lookahead over *that* sub-sequence while keeping a map back to the
//! original positions so the tree builder can re-attach trivia later.
//!
//! # Joint semantics
//!
//! The grammar's `token.immediate` rules (`a::b`, the type-argument `'`, the unit
//! `()`) need to know whether two adjacent *meaningful* tokens abut with nothing
//! between them. [`Input::is_joint`] answers that for the trivia-free view: the
//! token at a position is joint to the next non-trivia token iff every original
//! token from this one up to (but not including) that next non-trivia token is
//! byte-adjacent to its successor. A single space, comment, or newline anywhere
//! in the gap breaks the join.

use crate::lexer::Token;
use crate::syntax_kind::SyntaxKind;

/// A trivia-free cursor source over a lexed token stream.
///
/// Stores the indices of the meaningful (non-trivia) tokens in the original
/// stream, so the parser can look ahead by meaningful position while the tree
/// builder still has the original positions for trivia re-attachment. The source
/// travels with the tokens so a rule can also look ahead by *spelling*, which the
/// diagnostics that quote what the author wrote need.
pub struct Input<'t> {
    /// The source the tokens were lexed from, for [`Input::text`].
    src: &'t str,
    /// The full, lossless token stream (trivia included).
    tokens: &'t [Token],
    /// Original-stream indices of the non-trivia tokens, in order.
    meaningful: Vec<usize>,
    /// `joint[i]` is true iff meaningful token `i` abuts meaningful token `i+1`
    /// with no trivia between them (see the module docs).
    joint: Vec<bool>,
}

impl<'t> Input<'t> {
    /// Builds a trivia-free view over `tokens`, which were lexed from `src`.
    ///
    /// `tokens` is expected to be a full lexer stream terminated by an
    /// [`SyntaxKind::Eof`] sentinel; the `Eof` is meaningful and acts as the
    /// end-of-stream marker for lookahead.
    #[must_use]
    pub fn new(src: &'t str, tokens: &'t [Token]) -> Input<'t> {
        // The two arguments must describe the same source, or every span and
        // spelling read through this view is wrong. `Lexer::run` places the
        // zero-width `Eof` sentinel at `src.len()`, which pins the pairing.
        debug_assert_eq!(
            tokens.last().map(|t| t.loc.offset_start),
            Some(src.len() as u32),
            "tokens must be the output of tokenize(src)"
        );

        let mut meaningful = Vec::new();
        for (i, token) in tokens.iter().enumerate() {
            if !token.kind.is_trivia() {
                meaningful.push(i);
            }
        }

        let mut joint = Vec::with_capacity(meaningful.len());
        for window in meaningful.windows(2) {
            let (cur, next) = (window[0], window[1]);
            // Joint iff `next` is the immediately following original token (no
            // trivia between them) AND `cur` abuts it byte-for-byte. The lexer's
            // `joint` bit marks pure byte adjacency, but it is also set on a
            // whitespace token that abuts its neighbour, so adjacency alone is
            // not enough: we additionally require no intervening token at all.
            let glued = next == cur + 1 && tokens[cur].joint;
            joint.push(glued);
        }
        if !meaningful.is_empty() {
            // The last meaningful token (Eof) has no successor to be joint to.
            joint.push(false);
        }

        Input {
            src,
            tokens,
            meaningful,
            joint,
        }
    }

    /// The number of meaningful (non-trivia) tokens, including the `Eof`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.meaningful.len()
    }

    /// Whether the view has no meaningful tokens at all (not even `Eof`).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.meaningful.is_empty()
    }

    /// The kind of the meaningful token at `pos`, or [`SyntaxKind::Eof`] past the
    /// end of the stream.
    #[must_use]
    pub fn kind(&self, pos: usize) -> SyntaxKind {
        match self.meaningful.get(pos) {
            Some(&orig) => self.tokens[orig].kind,
            None => SyntaxKind::Eof,
        }
    }

    /// The kind `n` meaningful positions ahead of `pos`.
    #[must_use]
    pub fn nth(&self, pos: usize, n: usize) -> SyntaxKind {
        self.kind(pos + n)
    }

    /// Whether the meaningful token at `pos` abuts the next meaningful token with
    /// no trivia (whitespace, comment) between them.
    ///
    /// This is the parser-facing `token.immediate` predicate: it is true for the
    /// `::` in `a::b` but false for the `::` in `a :: b`, and true for the `'` in
    /// `i32'`.
    #[must_use]
    pub fn is_joint(&self, pos: usize) -> bool {
        self.joint.get(pos).copied().unwrap_or(false)
    }

    /// The original [`Token`] backing the meaningful token at `pos`, for span and
    /// location access. Returns `None` past the end of the stream.
    #[must_use]
    pub fn token(&self, pos: usize) -> Option<&Token> {
        self.meaningful.get(pos).map(|&orig| &self.tokens[orig])
    }

    /// The source spelling of the meaningful token at `pos`, or `""` past the end
    /// of the stream (where the zero-width `Eof` sentinel also spells `""`).
    #[must_use]
    pub fn text(&self, pos: usize) -> &'t str {
        match self.token(pos) {
            Some(token) => token.text(self.src),
            None => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    #[test]
    fn skips_trivia_in_lookahead() {
        let toks = tokenize("a  +\t b");
        let input = Input::new("a  +\t b", &toks);
        assert_eq!(input.kind(0), SyntaxKind::Ident);
        assert_eq!(input.kind(1), SyntaxKind::Plus);
        assert_eq!(input.kind(2), SyntaxKind::Ident);
        assert_eq!(input.kind(3), SyntaxKind::Eof);
        // Past the end stays Eof.
        assert_eq!(input.kind(99), SyntaxKind::Eof);
    }

    #[test]
    fn nth_offsets_meaningful_positions() {
        let toks = tokenize("a + b");
        let input = Input::new("a + b", &toks);
        assert_eq!(input.nth(0, 0), SyntaxKind::Ident);
        assert_eq!(input.nth(0, 1), SyntaxKind::Plus);
        assert_eq!(input.nth(0, 2), SyntaxKind::Ident);
        assert_eq!(input.nth(1, 1), SyntaxKind::Ident);
    }

    #[test]
    fn colon_colon_glued_is_joint() {
        let toks = tokenize("a::b");
        let input = Input::new("a::b", &toks);
        // Tokens: Ident(0) ColonColon(1) Ident(2) Eof(3).
        assert_eq!(input.kind(1), SyntaxKind::ColonColon);
        assert!(input.is_joint(0), "a abuts ::");
        assert!(input.is_joint(1), ":: abuts b");
    }

    #[test]
    fn spaced_colon_colon_is_not_joint() {
        let toks = tokenize("a :: b");
        let input = Input::new("a :: b", &toks);
        assert_eq!(input.kind(1), SyntaxKind::ColonColon);
        assert!(!input.is_joint(0), "a does not abut :: across a space");
        assert!(!input.is_joint(1), ":: does not abut b across a space");
    }

    #[test]
    fn type_argument_tick_is_joint() {
        let toks = tokenize("Vec i32'");
        let input = Input::new("Vec i32'", &toks);
        // Meaningful: Ident(Vec) I32Kw Tick Eof.
        assert_eq!(input.kind(0), SyntaxKind::Ident);
        assert_eq!(input.kind(1), SyntaxKind::I32Kw);
        assert_eq!(input.kind(2), SyntaxKind::Tick);
        assert!(!input.is_joint(0), "Vec is separated from i32 by a space");
        assert!(input.is_joint(1), "i32 abuts the tick");
    }

    #[test]
    fn comment_between_tokens_breaks_join() {
        let toks = tokenize("a// c\nb");
        let input = Input::new("a// c\nb", &toks);
        assert_eq!(input.kind(0), SyntaxKind::Ident);
        assert_eq!(input.kind(1), SyntaxKind::Ident);
        assert!(
            !input.is_joint(0),
            "a is followed by a comment, not glued to b"
        );
    }

    #[test]
    fn empty_source_is_just_eof() {
        let toks = tokenize("");
        let input = Input::new("", &toks);
        assert_eq!(input.len(), 1);
        assert_eq!(input.kind(0), SyntaxKind::Eof);
        assert!(!input.is_joint(0));
    }
}
