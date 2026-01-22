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

    // Nodes (Structural)
    SOURCE_FILE,
    MODULE,
    FUNCTION_DEF,
    FUNCTION_PARAM,
    FUNCTION_PARAM_LIST,
    RETURN_TYPE,
    FUNCTION_BODY,

    STRUCT_DEF,
    STRUCT_FIELD,
    STRUCT_FIELD_LIST,

    ENUM_DEF,
    ENUM_VARIANT,
    ENUM_VARIANT_LIST,

    TRAIT_DEF,
    TRAIT_ITEM,
    TRAIT_ITEM_LIST,

    IMPL_BLOCK,
    IMPL_ITEM_LIST,

    TYPE_ALIAS,
    CONST_ITEM,
    STATIC_ITEM,

    IMPORT_STMT,
    IMPORT_PATH,
    IMPORT_ALIAS,

    GENERIC_PARAM,
    GENERIC_PARAM_LIST,
    GENERIC_ARG,
    GENERIC_ARG_LIST,

    WHERE_CLAUSE,
    WHERE_PREDICATE,

    TYPE_REF,
    ARRAY_TYPE,
    SLICE_TYPE,
    POINTER_TYPE,
    REF_TYPE,
    FUNCTION_TYPE,

    BLOCK_EXPR,
    IF_EXPR,
    WHILE_EXPR,
    FOR_EXPR,
    LOOP_EXPR,
    MATCH_EXPR,
    MATCH_ARM,
    MATCH_ARM_LIST,

    BINARY_EXPR,
    UNARY_EXPR,
    CALL_EXPR,
    INDEX_EXPR,
    FIELD_EXPR,
    METHOD_CALL_EXPR,

    PAREN_EXPR,
    ARRAY_EXPR,
    ARRAY_EXPR_SPREAD,
    TUPLE_EXPR,
    RECORD_EXPR,
    RECORD_EXPR_FIELD,
    RECORD_EXPR_FIELD_LIST,

    PATH_EXPR,
    PATH_SEGMENT,

    LITERAL_EXPR,
    IDENT_EXPR,
    BREAK_EXPR,
    CONTINUE_EXPR,
    RETURN_EXPR,

    VAR_DECL,
    VAR_DECL_PATTERN,
    EXPR_STMT,
    ITEM_LIST,

    PATTERN,
    TUPLE_PATTERN,
    STRUCT_PATTERN,
    ARRAY_PATTERN,

    ERROR_NODE,
}

impl fmt::Display for SyntaxKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl SyntaxKind {
    /// Check if this is a keyword token
    pub fn is_keyword(self) -> bool {
        matches!(
            self,
            SyntaxKind::FN
                | SyntaxKind::LET
                | SyntaxKind::CONST
                | SyntaxKind::TYPE
                | SyntaxKind::STRUCT
                | SyntaxKind::ENUM
                | SyntaxKind::IMPL
                | SyntaxKind::TRAIT
                | SyntaxKind::IF
                | SyntaxKind::ELSE
                | SyntaxKind::WHILE
                | SyntaxKind::FOR
                | SyntaxKind::IN
                | SyntaxKind::RETURN
                | SyntaxKind::MATCH
                | SyntaxKind::IMPORT
                | SyntaxKind::AS
                | SyntaxKind::PUB
                | SyntaxKind::MUT
                | SyntaxKind::REF
                | SyntaxKind::WHERE
                | SyntaxKind::ASYNC
                | SyntaxKind::AWAIT
                | SyntaxKind::MOD
                | SyntaxKind::SELF_KW
                | SyntaxKind::SUPER
                | SyntaxKind::CRATE
                | SyntaxKind::TRUE
                | SyntaxKind::FALSE
                | SyntaxKind::BREAK
                | SyntaxKind::CONTINUE
                | SyntaxKind::LOOP
        )
    }

    /// Check if this is a literal token
    pub fn is_literal(self) -> bool {
        matches!(
            self,
            SyntaxKind::INT_NUMBER
                | SyntaxKind::FLOAT_NUMBER
                | SyntaxKind::STRING
                | SyntaxKind::CHAR
                | SyntaxKind::TRUE
                | SyntaxKind::FALSE
        )
    }

    /// Check if this is a binary operator
    pub fn is_binary_op(self) -> bool {
        matches!(
            self,
            SyntaxKind::PLUS
                | SyntaxKind::MINUS
                | SyntaxKind::STAR
                | SyntaxKind::SLASH
                | SyntaxKind::PERCENT
                | SyntaxKind::EQ_EQ
                | SyntaxKind::NOT_EQ
                | SyntaxKind::LESS
                | SyntaxKind::LESS_EQ
                | SyntaxKind::GREATER
                | SyntaxKind::GREATER_EQ
                | SyntaxKind::AND
                | SyntaxKind::OR
                | SyntaxKind::AMPERSAND
                | SyntaxKind::PIPE
                | SyntaxKind::CARET
                | SyntaxKind::LSHIFT
                | SyntaxKind::RSHIFT
        )
    }

    /// Check if this is a unary operator
    pub fn is_unary_op(self) -> bool {
        matches!(
            self,
            SyntaxKind::NOT | SyntaxKind::MINUS | SyntaxKind::AMPERSAND | SyntaxKind::STAR
        )
    }

    /// Check if this is an assignment operator
    pub fn is_assign_op(self) -> bool {
        matches!(
            self,
            SyntaxKind::ASSIGN
                | SyntaxKind::PLUS_ASSIGN
                | SyntaxKind::MINUS_ASSIGN
                | SyntaxKind::STAR_ASSIGN
                | SyntaxKind::SLASH_ASSIGN
        )
    }

    /// Get the precedence of a binary operator
    /// Higher values = higher precedence
    pub fn binary_op_precedence(self) -> u8 {
        match self {
            SyntaxKind::OR => 1,
            SyntaxKind::AND => 2,
            SyntaxKind::PIPE => 3,
            SyntaxKind::CARET => 4,
            SyntaxKind::AMPERSAND => 5,
            SyntaxKind::EQ_EQ | SyntaxKind::NOT_EQ => 6,
            SyntaxKind::LESS
            | SyntaxKind::LESS_EQ
            | SyntaxKind::GREATER
            | SyntaxKind::GREATER_EQ => 7,
            SyntaxKind::LSHIFT | SyntaxKind::RSHIFT => 8,
            SyntaxKind::PLUS | SyntaxKind::MINUS => 9,
            SyntaxKind::STAR | SyntaxKind::SLASH | SyntaxKind::PERCENT => 10,
            _ => 0,
        }
    }
}
