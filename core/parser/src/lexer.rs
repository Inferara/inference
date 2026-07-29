//! Total lexer for the Inference language (issue #62 design §4).
//!
//! [`tokenize`] turns a source string into a flat `Vec<Token>` in a single pass,
//! tracking byte offset plus 1-based line and 1-based byte column so each token
//! carries a full [`Location`]. The stream is **lossless**: trivia tokens
//! (whitespace and comments) are kept, so concatenating every token's source
//! slice reconstructs the input byte-for-byte. The parser skips trivia later.
//!
//! The lexer is **total**: it never panics, always terminates, and covers every
//! input byte exactly once (the sum of token byte spans equals `src.len()`).
//! Malformed input — an unterminated string, an unknown byte — becomes an
//! [`SyntaxKind::Error`] token rather than a failure.

use inference_ast::nodes::Location;

use crate::syntax_kind::SyntaxKind;

/// A single lexical token: its kind, source [`Location`], and whether it abuts
/// the next token with no trivia between them.
///
/// The `joint` bit drives the grammar's `token.immediate` rules — gluing `::`,
/// the type-argument `'`, and the unit `()` — and is true iff this token's
/// `offset_end` equals the next token's `offset_start`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The lexical class of this token.
    pub kind: SyntaxKind,
    /// The token's source span and line/column position.
    pub loc: Location,
    /// Whether the next token begins exactly where this one ends (no trivia).
    pub joint: bool,
}

impl Token {
    /// The source text this token spans, sliced from `src` by its byte offsets.
    ///
    /// The token counterpart of [`crate::SyntaxNode::text`], for the places that
    /// hold a token rather than a node: the grammar quoting an offending
    /// spelling in a diagnostic, and lowering reading one token out of a node
    /// that spans several.
    #[must_use]
    pub fn text<'s>(&self, src: &'s str) -> &'s str {
        let start = self.loc.offset_start as usize;
        let end = self.loc.offset_end as usize;
        src.get(start..end).unwrap_or("")
    }
}

/// Lexes `src` into a lossless token stream terminated by an [`SyntaxKind::Eof`].
///
/// Every input byte is covered by exactly one token (trivia included), so the
/// concatenated token slices reproduce `src`. The lexer never panics.
#[must_use]
pub fn tokenize(src: &str) -> Vec<Token> {
    Lexer::new(src).run()
}

/// Single-pass scanner over the source bytes, tracking position for [`Location`].
struct Lexer<'a> {
    src: &'a [u8],
    /// Current byte offset into `src`.
    offset: u32,
    /// Current 1-based line number.
    line: u32,
    /// Current 1-based byte column within the line.
    column: u32,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Lexer {
            src: src.as_bytes(),
            offset: 0,
            line: 1,
            column: 1,
        }
    }

    /// Scans the whole input, then patches `joint` bits and appends `Eof`.
    fn run(mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        while (self.offset as usize) < self.src.len() {
            tokens.push(self.next_token());
        }
        let eof_loc = self.zero_width_location();
        tokens.push(Token {
            kind: SyntaxKind::Eof,
            loc: eof_loc,
            joint: false,
        });
        Self::patch_joints(&mut tokens);
        tokens
    }

    /// Sets `joint` on each token whose end abuts the following token's start.
    fn patch_joints(tokens: &mut [Token]) {
        for i in 0..tokens.len().saturating_sub(1) {
            tokens[i].joint = tokens[i].loc.offset_end == tokens[i + 1].loc.offset_start;
        }
    }

    /// Lexes the single token starting at the current position.
    fn next_token(&mut self) -> Token {
        let byte = self.peek();
        if byte.is_ascii_whitespace() {
            return self.whitespace();
        }
        if byte == b'/' && self.peek_at(1) == b'/' {
            return self.line_comment();
        }
        if byte == b'"' {
            return self.string();
        }
        if byte.is_ascii_digit() || (byte == b'-' && self.peek_at(1).is_ascii_digit()) {
            return self.number();
        }
        if is_ident_start(byte) {
            return self.identifier();
        }
        self.operator_or_error()
    }

    /// A maximal run of ASCII whitespace, as a single trivia token.
    fn whitespace(&mut self) -> Token {
        let start = self.mark();
        while (self.offset as usize) < self.src.len() && self.peek().is_ascii_whitespace() {
            self.bump();
        }
        self.finish(start, SyntaxKind::Whitespace)
    }

    /// A `//` line comment or `///` doc comment, up to (not including) end of
    /// line. The leading `///` is checked before `//`.
    fn line_comment(&mut self) -> Token {
        let start = self.mark();
        let kind = if self.peek_at(2) == b'/' {
            SyntaxKind::DocComment
        } else {
            SyntaxKind::Comment
        };
        while (self.offset as usize) < self.src.len() && self.peek() != b'\n' {
            self.bump();
        }
        self.finish(start, kind)
    }

    /// A `"..."` string literal including the quotes. An unterminated run (a
    /// newline or EOF before the closing quote) becomes an `Error` token.
    fn string(&mut self) -> Token {
        let start = self.mark();
        self.bump(); // opening quote
        loop {
            if (self.offset as usize) >= self.src.len() {
                return self.finish(start, SyntaxKind::Error);
            }
            match self.peek() {
                b'"' => {
                    self.bump(); // closing quote
                    return self.finish(start, SyntaxKind::String);
                }
                b'\n' | b'\\' => return self.finish(start, SyntaxKind::Error),
                _ => self.bump(),
            }
        }
    }

    /// A `-?\d+` integer literal. A leading `-` is consumed only because the
    /// caller verified a digit follows it.
    fn number(&mut self) -> Token {
        let start = self.mark();
        if self.peek() == b'-' {
            self.bump();
        }
        while (self.offset as usize) < self.src.len() && self.peek().is_ascii_digit() {
            self.bump();
        }
        self.finish(start, SyntaxKind::Number)
    }

    /// An identifier `[A-Za-z_]\w*`, resolved to a keyword kind, `Underscore`
    /// (the lone `_`), or `Ident`.
    fn identifier(&mut self) -> Token {
        let start = self.mark();
        let start_off = self.offset as usize;
        self.bump();
        while (self.offset as usize) < self.src.len() && is_ident_continue(self.peek()) {
            self.bump();
        }
        let text = &self.src[start_off..self.offset as usize];
        let kind = if text == b"_" {
            SyntaxKind::Underscore
        } else {
            // `text` is ASCII identifier bytes, so this is always valid UTF-8.
            let text = std::str::from_utf8(text).unwrap_or_default();
            SyntaxKind::from_keyword(text).unwrap_or(SyntaxKind::Ident)
        };
        self.finish(start, kind)
    }

    /// A multi-char operator (longest match), then a single-char punctuation or
    /// operator, then a one-byte `Error` for any unrecognized byte.
    fn operator_or_error(&mut self) -> Token {
        let start = self.mark();
        let first = self.peek();
        let second = self.peek_at(1);

        if let Some(kind) = two_char_op(first, second) {
            self.bump();
            self.bump();
            return self.finish(start, kind);
        }

        let kind = single_char(first).unwrap_or(SyntaxKind::Error);
        self.bump();
        self.finish(start, kind)
    }

    // -- position tracking --

    /// The byte at the cursor, or `0` past the end (callers guard the end).
    fn peek(&self) -> u8 {
        self.peek_at(0)
    }

    /// The byte `n` positions ahead of the cursor, or `0` past the end.
    fn peek_at(&self, n: u32) -> u8 {
        self.src
            .get((self.offset + n) as usize)
            .copied()
            .unwrap_or(0)
    }

    /// Consumes one byte, advancing offset/line/column.
    fn bump(&mut self) {
        if self.peek() == b'\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        self.offset += 1;
    }

    /// Snapshots the current position as the start of a token.
    fn mark(&self) -> Mark {
        Mark {
            offset: self.offset,
            line: self.line,
            column: self.column,
        }
    }

    /// Builds a token spanning `start` to the current position.
    fn finish(&self, start: Mark, kind: SyntaxKind) -> Token {
        Token {
            kind,
            loc: Location::new(
                start.offset,
                self.offset,
                start.line,
                start.column,
                self.line,
                self.column,
            ),
            joint: false,
        }
    }

    /// A zero-width location at the current (end-of-input) position.
    fn zero_width_location(&self) -> Location {
        Location::new(
            self.offset,
            self.offset,
            self.line,
            self.column,
            self.line,
            self.column,
        )
    }
}

/// A snapshot of the scanner position at a token's start.
struct Mark {
    offset: u32,
    line: u32,
    column: u32,
}

/// Whether `byte` can start an identifier.
///
/// The expression grammar reads this too: a `Number` glued to a byte that starts
/// an identifier is one malformed literal the scanners split in two, and sharing
/// the predicate keeps that check tied to the definition rather than a copy.
pub(crate) fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

/// Whether `byte` can continue an identifier (`\w` = alnum or `_`).
fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The two-char operator/punctuation kind for `first`+`second`, if any.
fn two_char_op(first: u8, second: u8) -> Option<SyntaxKind> {
    let kind = match (first, second) {
        (b':', b':') => SyntaxKind::ColonColon,
        (b'-', b'>') => SyntaxKind::Arrow,
        (b'*', b'*') => SyntaxKind::StarStar,
        (b'&', b'&') => SyntaxKind::AmpAmp,
        (b'|', b'|') => SyntaxKind::PipePipe,
        (b'<', b'<') => SyntaxKind::Shl,
        (b'>', b'>') => SyntaxKind::Shr,
        (b'<', b'=') => SyntaxKind::Le,
        (b'>', b'=') => SyntaxKind::Ge,
        (b'=', b'=') => SyntaxKind::EqEq,
        (b'!', b'=') => SyntaxKind::Ne,
        _ => return None,
    };
    Some(kind)
}

/// The single-char punctuation/operator kind for `byte`, if any.
fn single_char(byte: u8) -> Option<SyntaxKind> {
    let kind = match byte {
        b'(' => SyntaxKind::LParen,
        b')' => SyntaxKind::RParen,
        b'{' => SyntaxKind::LBrace,
        b'}' => SyntaxKind::RBrace,
        b'[' => SyntaxKind::LBracket,
        b']' => SyntaxKind::RBracket,
        b';' => SyntaxKind::Semi,
        b':' => SyntaxKind::Colon,
        b',' => SyntaxKind::Comma,
        b'.' => SyntaxKind::Dot,
        b'@' => SyntaxKind::At,
        b'\'' => SyntaxKind::Tick,
        b'+' => SyntaxKind::Plus,
        b'-' => SyntaxKind::Minus,
        b'*' => SyntaxKind::Star,
        b'/' => SyntaxKind::Slash,
        b'%' => SyntaxKind::Percent,
        b'&' => SyntaxKind::Amp,
        b'|' => SyntaxKind::Pipe,
        b'^' => SyntaxKind::Caret,
        b'<' => SyntaxKind::Lt,
        b'>' => SyntaxKind::Gt,
        b'=' => SyntaxKind::Eq,
        b'!' => SyntaxKind::Bang,
        b'~' => SyntaxKind::Tilde,
        _ => return None,
    };
    Some(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The kinds of every non-trivia token, for compact assertions.
    fn kinds(src: &str) -> Vec<SyntaxKind> {
        tokenize(src).into_iter().map(|t| t.kind).collect()
    }

    /// The source slice each token covers, joined back together.
    fn reassemble(src: &str) -> String {
        tokenize(src)
            .iter()
            .map(|t| &src[t.loc.offset_start as usize..t.loc.offset_end as usize])
            .collect()
    }

    #[test]
    fn round_trip_reconstructs_source() {
        let sources = [
            "fn main() { return; }",
            "fn add(a: i32, b: i32) -> i32 { return a + b; }",
            "pub fn f<T>(self) { let mut i32 x = 1; assert x == 1; }",
            "// a comment\n/// a doc\nstruct S { i32 field; }",
            "use foo::bar from \"m\";\nspec s() { forall { assume true; } }",
            "let bool b = a && b || !c;\nx = arr[0] + obj.field;",
        ];
        for src in sources {
            let toks = tokenize(src);
            // Every byte is covered exactly once.
            let covered: usize = toks
                .iter()
                .map(|t| (t.loc.offset_end - t.loc.offset_start) as usize)
                .sum();
            assert_eq!(covered, src.len(), "byte coverage mismatch for {src:?}");
            assert_eq!(reassemble(src), src, "round-trip mismatch for {src:?}");
        }
    }

    #[test]
    fn round_trip_corpus_snippet() {
        // A representative snippet from the tree-sitter corpus.
        let src = "fn main() {\n  return;\n}\n";
        assert_eq!(reassemble(src), src);
        let covered: usize = tokenize(src)
            .iter()
            .map(|t| (t.loc.offset_end - t.loc.offset_start) as usize)
            .sum();
        assert_eq!(covered, src.len());
    }

    #[test]
    fn keywords_lex_as_keyword_kinds() {
        assert_eq!(
            kinds("fn let mut spec struct enum const type external"),
            [
                SyntaxKind::FnKw,
                SyntaxKind::Whitespace,
                SyntaxKind::LetKw,
                SyntaxKind::Whitespace,
                SyntaxKind::MutKw,
                SyntaxKind::Whitespace,
                SyntaxKind::SpecKw,
                SyntaxKind::Whitespace,
                SyntaxKind::StructKw,
                SyntaxKind::Whitespace,
                SyntaxKind::EnumKw,
                SyntaxKind::Whitespace,
                SyntaxKind::ConstKw,
                SyntaxKind::Whitespace,
                SyntaxKind::TypeKw,
                SyntaxKind::Whitespace,
                SyntaxKind::ExternalKw,
                SyntaxKind::Eof,
            ]
        );
    }

    #[test]
    fn control_and_nondet_keywords() {
        assert_eq!(
            kinds("return loop if else assert break use from self pub assume forall exists unique"),
            [
                SyntaxKind::ReturnKw,
                SyntaxKind::Whitespace,
                SyntaxKind::LoopKw,
                SyntaxKind::Whitespace,
                SyntaxKind::IfKw,
                SyntaxKind::Whitespace,
                SyntaxKind::ElseKw,
                SyntaxKind::Whitespace,
                SyntaxKind::AssertKw,
                SyntaxKind::Whitespace,
                SyntaxKind::BreakKw,
                SyntaxKind::Whitespace,
                SyntaxKind::UseKw,
                SyntaxKind::Whitespace,
                SyntaxKind::FromKw,
                SyntaxKind::Whitespace,
                SyntaxKind::SelfKw,
                SyntaxKind::Whitespace,
                SyntaxKind::PubKw,
                SyntaxKind::Whitespace,
                SyntaxKind::AssumeKw,
                SyntaxKind::Whitespace,
                SyntaxKind::ForallKw,
                SyntaxKind::Whitespace,
                SyntaxKind::ExistsKw,
                SyntaxKind::Whitespace,
                SyntaxKind::UniqueKw,
                SyntaxKind::Eof,
            ]
        );
    }

    #[test]
    fn type_keywords() {
        assert_eq!(
            kinds("i8 i16 i32 i64 u8 u16 u32 u64 bool"),
            [
                SyntaxKind::I8Kw,
                SyntaxKind::Whitespace,
                SyntaxKind::I16Kw,
                SyntaxKind::Whitespace,
                SyntaxKind::I32Kw,
                SyntaxKind::Whitespace,
                SyntaxKind::I64Kw,
                SyntaxKind::Whitespace,
                SyntaxKind::U8Kw,
                SyntaxKind::Whitespace,
                SyntaxKind::U16Kw,
                SyntaxKind::Whitespace,
                SyntaxKind::U32Kw,
                SyntaxKind::Whitespace,
                SyntaxKind::U64Kw,
                SyntaxKind::Whitespace,
                SyntaxKind::BoolKw,
                SyntaxKind::Eof,
            ]
        );
    }

    #[test]
    fn true_false_self_are_keywords() {
        assert_eq!(
            kinds("true false self"),
            [
                SyntaxKind::TrueKw,
                SyntaxKind::Whitespace,
                SyntaxKind::FalseKw,
                SyntaxKind::Whitespace,
                SyntaxKind::SelfKw,
                SyntaxKind::Eof,
            ]
        );
    }

    #[test]
    fn multi_char_operators() {
        assert_eq!(
            kinds(":: -> ** && || << >> <= >= == !="),
            [
                SyntaxKind::ColonColon,
                SyntaxKind::Whitespace,
                SyntaxKind::Arrow,
                SyntaxKind::Whitespace,
                SyntaxKind::StarStar,
                SyntaxKind::Whitespace,
                SyntaxKind::AmpAmp,
                SyntaxKind::Whitespace,
                SyntaxKind::PipePipe,
                SyntaxKind::Whitespace,
                SyntaxKind::Shl,
                SyntaxKind::Whitespace,
                SyntaxKind::Shr,
                SyntaxKind::Whitespace,
                SyntaxKind::Le,
                SyntaxKind::Whitespace,
                SyntaxKind::Ge,
                SyntaxKind::Whitespace,
                SyntaxKind::EqEq,
                SyntaxKind::Whitespace,
                SyntaxKind::Ne,
                SyntaxKind::Eof,
            ]
        );
    }

    #[test]
    fn single_char_punct_and_operators() {
        assert_eq!(
            kinds("( ) { } [ ] ; : , . @ ' + * / % & | ^ < > = ! ~"),
            [
                SyntaxKind::LParen,
                SyntaxKind::Whitespace,
                SyntaxKind::RParen,
                SyntaxKind::Whitespace,
                SyntaxKind::LBrace,
                SyntaxKind::Whitespace,
                SyntaxKind::RBrace,
                SyntaxKind::Whitespace,
                SyntaxKind::LBracket,
                SyntaxKind::Whitespace,
                SyntaxKind::RBracket,
                SyntaxKind::Whitespace,
                SyntaxKind::Semi,
                SyntaxKind::Whitespace,
                SyntaxKind::Colon,
                SyntaxKind::Whitespace,
                SyntaxKind::Comma,
                SyntaxKind::Whitespace,
                SyntaxKind::Dot,
                SyntaxKind::Whitespace,
                SyntaxKind::At,
                SyntaxKind::Whitespace,
                SyntaxKind::Tick,
                SyntaxKind::Whitespace,
                SyntaxKind::Plus,
                SyntaxKind::Whitespace,
                SyntaxKind::Star,
                SyntaxKind::Whitespace,
                SyntaxKind::Slash,
                SyntaxKind::Whitespace,
                SyntaxKind::Percent,
                SyntaxKind::Whitespace,
                SyntaxKind::Amp,
                SyntaxKind::Whitespace,
                SyntaxKind::Pipe,
                SyntaxKind::Whitespace,
                SyntaxKind::Caret,
                SyntaxKind::Whitespace,
                SyntaxKind::Lt,
                SyntaxKind::Whitespace,
                SyntaxKind::Gt,
                SyntaxKind::Whitespace,
                SyntaxKind::Eq,
                SyntaxKind::Whitespace,
                SyntaxKind::Bang,
                SyntaxKind::Whitespace,
                SyntaxKind::Tilde,
                SyntaxKind::Eof,
            ]
        );
    }

    #[test]
    fn negative_number_is_single_token() {
        assert_eq!(
            kinds("-42"),
            [SyntaxKind::Number, SyntaxKind::Eof],
            "-42 must lex as one Number"
        );
    }

    #[test]
    fn spaced_minus_then_number() {
        assert_eq!(
            kinds("- 42"),
            [
                SyntaxKind::Minus,
                SyntaxKind::Whitespace,
                SyntaxKind::Number,
                SyntaxKind::Eof,
            ]
        );
    }

    #[test]
    fn identifier_minus_identifier() {
        assert_eq!(
            kinds("a - b"),
            [
                SyntaxKind::Ident,
                SyntaxKind::Whitespace,
                SyntaxKind::Minus,
                SyntaxKind::Whitespace,
                SyntaxKind::Ident,
                SyntaxKind::Eof,
            ]
        );
    }

    #[test]
    fn underscore_alone_vs_underscored_ident() {
        assert_eq!(
            kinds("_ _x"),
            [
                SyntaxKind::Underscore,
                SyntaxKind::Whitespace,
                SyntaxKind::Ident,
                SyntaxKind::Eof,
            ]
        );
    }

    #[test]
    fn reserved_idents_lex_as_ident() {
        assert_eq!(
            kinds("constructor proof uzumaki"),
            [
                SyntaxKind::Ident,
                SyntaxKind::Whitespace,
                SyntaxKind::Ident,
                SyntaxKind::Whitespace,
                SyntaxKind::Ident,
                SyntaxKind::Eof,
            ]
        );
    }

    #[test]
    fn string_literal_includes_quotes() {
        let toks = tokenize("\"hello\"");
        assert_eq!(toks[0].kind, SyntaxKind::String);
        assert_eq!("\"hello\"", reassemble("\"hello\""));
    }

    #[test]
    fn unterminated_string_eof_is_error_no_panic() {
        let toks = tokenize("\"abc");
        assert!(toks.iter().any(|t| t.kind == SyntaxKind::Error));
        assert_eq!(toks.last().map(|t| t.kind), Some(SyntaxKind::Eof));
    }

    #[test]
    fn unterminated_string_newline_is_error() {
        let toks = tokenize("\"abc\ndef\"");
        assert_eq!(toks[0].kind, SyntaxKind::Error);
    }

    #[test]
    fn comment_vs_doc_comment_classification() {
        assert_eq!(
            kinds("// plain\n/// doc"),
            [
                SyntaxKind::Comment,
                SyntaxKind::Whitespace,
                SyntaxKind::DocComment,
                SyntaxKind::Eof,
            ]
        );
    }

    #[test]
    fn whitespace_is_trivia_token() {
        let toks = tokenize("  \t\n ");
        assert_eq!(toks[0].kind, SyntaxKind::Whitespace);
        assert!(toks[0].kind.is_trivia());
    }

    #[test]
    fn unknown_char_is_one_byte_error() {
        let toks = tokenize("#");
        assert_eq!(toks[0].kind, SyntaxKind::Error);
        assert_eq!(toks[0].loc.offset_start, 0);
        assert_eq!(toks[0].loc.offset_end, 1);
    }

    #[test]
    fn type_argument_tick_is_joint_to_type() {
        // `Vec i32'` — the `'` immediately follows `i32` (token.immediate).
        let toks = tokenize("Vec i32'");
        let kinds: Vec<_> = toks.iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            [
                SyntaxKind::Ident,
                SyntaxKind::Whitespace,
                SyntaxKind::I32Kw,
                SyntaxKind::Tick,
                SyntaxKind::Eof,
            ]
        );
        // `i32` is joint to `'` (no space): they abut byte-for-byte, which is
        // what `token.immediate('\'')` needs. The joint bit is purely byte
        // adjacency: in "Vec i32'" every consecutive token abuts (the only
        // whitespace is itself a token), so each leading token is joint. The
        // parser resolves true immediacy on the trivia-free stream later.
        let [vec, _ws, i32_kw, tick, _eof] = toks.as_slice() else {
            panic!("expected exactly five tokens");
        };
        assert_eq!(i32_kw.kind, SyntaxKind::I32Kw);
        assert!(i32_kw.joint, "i32 must be joint to the tick");
        assert!(tick.joint, "the tick abuts the zero-width Eof");
        assert!(
            vec.joint,
            "Vec abuts the whitespace, so it is joint by bytes"
        );
    }

    #[test]
    fn colon_colon_joint_bits() {
        // `a::b` — all glued, every leading token joint.
        let toks = tokenize("a::b");
        assert_eq!(
            toks.iter().map(|t| t.kind).collect::<Vec<_>>(),
            [
                SyntaxKind::Ident,
                SyntaxKind::ColonColon,
                SyntaxKind::Ident,
                SyntaxKind::Eof,
            ]
        );
        assert!(toks[0].joint);
        assert!(toks[1].joint);
    }

    #[test]
    fn location_is_one_based_line_and_byte_column() {
        // First token on line 1, the `x` on line 2.
        let toks = tokenize("fn\nx");
        let fn_tok = &toks[0];
        assert_eq!(fn_tok.loc.start_line, 1);
        assert_eq!(fn_tok.loc.start_column, 1);
        let x_tok = toks.iter().find(|t| t.kind == SyntaxKind::Ident).unwrap();
        assert_eq!(x_tok.loc.start_line, 2);
        assert_eq!(x_tok.loc.start_column, 1);
    }

    #[test]
    fn byte_column_counts_bytes() {
        // The leading token spans bytes 0..2 (`ab`), the `+` starts at column 3.
        let toks = tokenize("ab+");
        let plus = toks.iter().find(|t| t.kind == SyntaxKind::Plus).unwrap();
        assert_eq!(plus.loc.start_column, 3);
        assert_eq!(plus.loc.offset_start, 2);
    }

    #[test]
    fn empty_input_is_just_eof() {
        let toks = tokenize("");
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].kind, SyntaxKind::Eof);
        assert_eq!(toks[0].loc.offset_start, 0);
        assert_eq!(toks[0].loc.offset_end, 0);
    }

    #[test]
    fn arrow_then_type() {
        assert_eq!(
            kinds("-> i32"),
            [
                SyntaxKind::Arrow,
                SyntaxKind::Whitespace,
                SyntaxKind::I32Kw,
                SyntaxKind::Eof,
            ]
        );
    }

    #[test]
    fn uzumaki_at_token() {
        assert_eq!(kinds("@"), [SyntaxKind::At, SyntaxKind::Eof]);
    }
}
