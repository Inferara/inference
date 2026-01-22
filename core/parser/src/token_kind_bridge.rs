/// Bridging module to convert between TokenKind and SyntaxKind

use crate::lexer::TokenKind;
use crate::syntax_kind::SyntaxKind;

impl SyntaxKind {
    /// Convert a TokenKind to its corresponding SyntaxKind
    pub fn from_token_kind(tk: &TokenKind) -> Self {
        match tk {
            TokenKind::Eof => SyntaxKind::EOF,
            TokenKind::Unknown(_) => SyntaxKind::ERROR,
            
            // Literals
            TokenKind::Number(_) => SyntaxKind::INT_NUMBER,
            TokenKind::String(_) => SyntaxKind::STRING,
            TokenKind::Identifier(_) => SyntaxKind::IDENT,
            
            // Keywords
            TokenKind::Fn => SyntaxKind::FN,
            TokenKind::Let => SyntaxKind::LET,
            TokenKind::Const => SyntaxKind::CONST,
            TokenKind::Type => SyntaxKind::TYPE,
            TokenKind::Struct => SyntaxKind::STRUCT,
            TokenKind::Enum => SyntaxKind::ENUM,
            TokenKind::Impl => SyntaxKind::IMPL,
            TokenKind::Trait => SyntaxKind::TRAIT,
            TokenKind::If => SyntaxKind::IF,
            TokenKind::Else => SyntaxKind::ELSE,
            TokenKind::While => SyntaxKind::WHILE,
            TokenKind::For => SyntaxKind::FOR,
            TokenKind::In => SyntaxKind::IN,
            TokenKind::Return => SyntaxKind::RETURN,
            TokenKind::Match => SyntaxKind::MATCH,
            TokenKind::Import => SyntaxKind::IMPORT,
            TokenKind::As => SyntaxKind::AS,
            TokenKind::Pub => SyntaxKind::PUB,
            TokenKind::Mut => SyntaxKind::MUT,
            TokenKind::Ref => SyntaxKind::REF,
            TokenKind::Where => SyntaxKind::WHERE,
            TokenKind::Async => SyntaxKind::ASYNC,
            TokenKind::Await => SyntaxKind::AWAIT,
            TokenKind::Mod => SyntaxKind::MOD,
            TokenKind::Self_ => SyntaxKind::SELF_KW,
            TokenKind::Super => SyntaxKind::SUPER,
            TokenKind::Crate => SyntaxKind::CRATE,
            
            // Operators
            TokenKind::Plus => SyntaxKind::PLUS,
            TokenKind::Minus => SyntaxKind::MINUS,
            TokenKind::Star => SyntaxKind::STAR,
            TokenKind::Slash => SyntaxKind::SLASH,
            TokenKind::Percent => SyntaxKind::PERCENT,
            TokenKind::Assign => SyntaxKind::ASSIGN,
            TokenKind::PlusAssign => SyntaxKind::PLUS_ASSIGN,
            TokenKind::MinusAssign => SyntaxKind::MINUS_ASSIGN,
            TokenKind::StarAssign => SyntaxKind::STAR_ASSIGN,
            TokenKind::SlashAssign => SyntaxKind::SLASH_ASSIGN,
            TokenKind::EqEq => SyntaxKind::EQ_EQ,
            TokenKind::NotEq => SyntaxKind::NOT_EQ,
            TokenKind::Less => SyntaxKind::LESS,
            TokenKind::LessEq => SyntaxKind::LESS_EQ,
            TokenKind::Greater => SyntaxKind::GREATER,
            TokenKind::GreaterEq => SyntaxKind::GREATER_EQ,
            TokenKind::And => SyntaxKind::AND,
            TokenKind::Or => SyntaxKind::OR,
            TokenKind::Not => SyntaxKind::NOT,
            TokenKind::Ampersand => SyntaxKind::AMPERSAND,
            TokenKind::Pipe => SyntaxKind::PIPE,
            TokenKind::Caret => SyntaxKind::CARET,
            TokenKind::Tilde => SyntaxKind::TILDE,
            TokenKind::LeftShift => SyntaxKind::LSHIFT,
            TokenKind::RightShift => SyntaxKind::RSHIFT,
            TokenKind::Arrow => SyntaxKind::ARROW,
            TokenKind::DoubleArrow => SyntaxKind::FAT_ARROW,
            TokenKind::Dot => SyntaxKind::DOT,
            TokenKind::DotDot => SyntaxKind::DOTDOT,
            TokenKind::DotDotEq => SyntaxKind::DOTDOT_EQ,
            TokenKind::Colon => SyntaxKind::COLON,
            TokenKind::DoubleColon => SyntaxKind::COLON_COLON,
            TokenKind::Comma => SyntaxKind::COMMA,
            TokenKind::Semicolon => SyntaxKind::SEMICOLON,
            TokenKind::Question => SyntaxKind::QUESTION,
            
            // Delimiters
            TokenKind::LeftParen => SyntaxKind::L_PAREN,
            TokenKind::RightParen => SyntaxKind::R_PAREN,
            TokenKind::LeftBrace => SyntaxKind::L_BRACE,
            TokenKind::RightBrace => SyntaxKind::R_BRACE,
            TokenKind::LeftBracket => SyntaxKind::L_BRACKET,
            TokenKind::RightBracket => SyntaxKind::R_BRACKET,
            TokenKind::LeftAngle => SyntaxKind::L_ANGLE,
            TokenKind::RightAngle => SyntaxKind::R_ANGLE,
            
            TokenKind::Newline => SyntaxKind::NEWLINE,
        }
    }
}

/// Standalone function to convert TokenKind to SyntaxKind
pub fn from_token_kind(tk: &TokenKind) -> SyntaxKind {
    SyntaxKind::from_token_kind(tk)
}
