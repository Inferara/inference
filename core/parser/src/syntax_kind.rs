/// Syntax kinds for all token and node types in Inference language
/// Organized to match rust-analyzer's approach for maintainability

use std::fmt;

/// All syntax kinds in the Inference language
/// Token kinds are used for lexical elements (keywords, operators, etc.)
/// Node kinds are used for structural elements (expressions, items, etc.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u16)]
pub enum SyntaxKind {
    // Tokens
    #[doc(hidden)]
    EOF,
    #[doc(hidden)]
    ERROR,

    // Literals
    INT_NUMBER,
    FLOAT_NUMBER,
    STRING,
    CHAR,

    // Keywords
    FN,
    LET,
    CONST,
    TYPE,
    STRUCT,
    ENUM,
    IMPL,
    TRAIT,
    IF,
    ELSE,
    WHILE,
    FOR,
    IN,
    RETURN,
    MATCH,
    IMPORT,
    AS,
    PUB,
    MUT,
    REF,
    WHERE,
    ASYNC,
    AWAIT,
    MOD,
    SELF_KW,
    SUPER,
    CRATE,
    TRUE,
    FALSE,
    BREAK,
    CONTINUE,
    LOOP,

    // Operators
    PLUS,
    MINUS,
    STAR,
    SLASH,
    PERCENT,
    ASSIGN,
    PLUS_ASSIGN,
    MINUS_ASSIGN,
    STAR_ASSIGN,
    SLASH_ASSIGN,
    EQ_EQ,
    NOT_EQ,
    LESS,
    LESS_EQ,
    GREATER,
    GREATER_EQ,
    AND,
    OR,
    NOT,
    AMPERSAND,
    PIPE,
    CARET,
    TILDE,
    LSHIFT,
    RSHIFT,
    ARROW,
    FAT_ARROW,
    DOT,
    DOTDOT,
    DOTDOT_EQ,
    COLON,
    COLON_COLON,
    QUESTION,

    // Delimiters
    L_PAREN,
    R_PAREN,
    L_BRACE,
    R_BRACE,
    L_BRACKET,
    R_BRACKET,
    L_ANGLE,
    R_ANGLE,

    // Punctuation
    COMMA,
    SEMICOLON,
    AT,

    // Identifiers
    IDENT,

    // Whitespace & Comments
    WHITESPACE,
    LINE_COMMENT,
    BLOCK_COMMENT,
    NEWLINE,
}

impl fmt::Display for SyntaxKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
