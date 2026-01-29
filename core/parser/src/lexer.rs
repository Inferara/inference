/// Lexer for the Inference language
/// Converts source code into a stream of tokens

use std::str::Chars;
use std::iter::Peekable;

/// Token types for the Inference language
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // Literals
    Identifier(String),
    Number(String),
    String(String),

    // Keywords
    Fn,
    Let,
    Const,
    Type,
    Struct,
    Enum,
    Impl,
    If,
    Else,
    While,
    For,
    In,
    Return,
    Match,
    Import,
    As,
    Pub,
    Mut,
    Ref,
    Where,
    Trait,
    Async,
    Await,
    Mod,
    Self_,
    Super,
    Crate,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Assign,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    EqEq,
    NotEq,
    Less,
    LessEq,
    Greater,
    GreaterEq,
    And,
    Or,
    Not,
    Ampersand,
    Pipe,
    Caret,
    Tilde,
    LeftShift,
    RightShift,
    Arrow,
    DoubleArrow,
    Dot,
    DotDot,
    DotDotEq,
    Colon,
    DoubleColon,
    Comma,
    Semicolon,
    Question,

    // Delimiters
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    LeftAngle,
    RightAngle,

    // Special
    Newline,
    Eof,
    Unknown(char),
}

/// Token with position information
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub pos: usize,
    pub len: usize,
    pub line: usize,
    pub column: usize,
}

impl Token {
    pub fn new(kind: TokenKind, pos: usize, len: usize, line: usize, column: usize) -> Self {
        Self {
            kind,
            pos,
            len,
            line,
            column,
        }
    }
}

/// Lexer tokenizes source code
pub struct Lexer<'a> {
    chars: Peekable<Chars<'a>>,
    pos: usize,
    line: usize,
    column: usize,
    current_char: Option<char>,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        let mut chars = input.chars().peekable();
        let current_char = chars.next();
        Self {
            chars,
            pos: 0,
            line: 1,
            column: 1,
            current_char,
        }
    }

    fn advance(&mut self) {
        if let Some(ch) = self.current_char {
            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            self.pos += ch.len_utf8();
        }
        self.current_char = self.chars.next();
    }

    #[inline]
    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    pub fn next_token(&mut self) -> Token {
        while let Some(ch) = self.current_char {
            if ch.is_whitespace() && ch != '\n' {
                self.advance();
            } else {
                break;
            }
        }

        let start_pos = self.pos;
        let start_line = self.line;
        let start_column = self.column;

        match self.current_char {
            None => Token::new(TokenKind::Eof, start_pos, 0, start_line, start_column),
            Some('\n') => {
                self.advance();
                Token::new(TokenKind::Newline, start_pos, 1, start_line, start_column)
            }
            Some(ch) if ch.is_alphabetic() || ch == '_' => {
                let ident = self.read_ident();
                let len = ident.len();
                let kind = match ident.as_str() {
                    "fn" => TokenKind::Fn,
                    "let" => TokenKind::Let,
                    "const" => TokenKind::Const,
                    "type" => TokenKind::Type,
                    "struct" => TokenKind::Struct,
                    "enum" => TokenKind::Enum,
                    "impl" => TokenKind::Impl,
                    "if" => TokenKind::If,
                    "else" => TokenKind::Else,
                    "while" => TokenKind::While,
                    "for" => TokenKind::For,
                    "in" => TokenKind::In,
                    "return" => TokenKind::Return,
                    "match" => TokenKind::Match,
                    "import" => TokenKind::Import,
                    "as" => TokenKind::As,
                    "pub" => TokenKind::Pub,
                    "mut" => TokenKind::Mut,
                    "ref" => TokenKind::Ref,
                    "where" => TokenKind::Where,
                    "trait" => TokenKind::Trait,
                    "async" => TokenKind::Async,
                    "await" => TokenKind::Await,
                    "mod" => TokenKind::Mod,
                    "self" => TokenKind::Self_,
                    "super" => TokenKind::Super,
                    "crate" => TokenKind::Crate,
                    _ => TokenKind::Identifier(ident),
                };
                Token::new(kind, start_pos, len, start_line, start_column)
            }
            Some(ch) if ch.is_numeric() => {
                let num = self.read_num();
                let len = num.len();
                Token::new(TokenKind::Number(num), start_pos, len, start_line, start_column)
            }
            Some('"') => {
                let string = self.read_str();
                let len = string.len() + 2;
                Token::new(TokenKind::String(string), start_pos, len, start_line, start_column)
            }
            Some('+') => {
                self.advance();
                if self.current_char == Some('=') {
                    self.advance();
                    Token::new(TokenKind::PlusAssign, start_pos, 2, start_line, start_column)
                } else {
                    Token::new(TokenKind::Plus, start_pos, 1, start_line, start_column)
                }
            }
            Some('-') => {
                self.advance();
                match self.current_char {
                    Some('>') => {
                        self.advance();
                        Token::new(TokenKind::Arrow, start_pos, 2, start_line, start_column)
                    }
                    Some('=') => {
                        self.advance();
                        Token::new(TokenKind::MinusAssign, start_pos, 2, start_line, start_column)
                    }
                    _ => Token::new(TokenKind::Minus, start_pos, 1, start_line, start_column),
                }
            }
            Some('*') => {
                self.advance();
                if self.current_char == Some('=') {
                    self.advance();
                    Token::new(TokenKind::StarAssign, start_pos, 2, start_line, start_column)
                } else {
                    Token::new(TokenKind::Star, start_pos, 1, start_line, start_column)
                }
            }
            Some('/') => {
                self.advance();
                match self.current_char {
                    Some('/') => {
                        while self.current_char.is_some() && self.current_char != Some('\n') {
                            self.advance();
                        }
                        self.next_token()
                    }
                    Some('*') => {
                        self.advance();
                        while self.current_char.is_some() {
                            if self.current_char == Some('*') && self.peek() == Some('/') {
                                self.advance();
                                self.advance();
                                break;
                            }
                            self.advance();
                        }
                        self.next_token()
                    }
                    Some('=') => {
                        self.advance();
                        Token::new(TokenKind::SlashAssign, start_pos, 2, start_line, start_column)
                    }
                    _ => Token::new(TokenKind::Slash, start_pos, 1, start_line, start_column),
                }
            }
            Some('=') => {
                self.advance();
                match self.current_char {
                    Some('=') => {
                        self.advance();
                        Token::new(TokenKind::EqEq, start_pos, 2, start_line, start_column)
                    }
                    Some('>') => {
                        self.advance();
                        Token::new(TokenKind::DoubleArrow, start_pos, 2, start_line, start_column)
                    }
                    _ => Token::new(TokenKind::Assign, start_pos, 1, start_line, start_column),
                }
            }
            Some('!') => {
                self.advance();
                if self.current_char == Some('=') {
                    self.advance();
                    Token::new(TokenKind::NotEq, start_pos, 2, start_line, start_column)
                } else {
                    Token::new(TokenKind::Not, start_pos, 1, start_line, start_column)
                }
            }
            Some('<') => {
                self.advance();
                match self.current_char {
                    Some('=') => {
                        self.advance();
                        Token::new(TokenKind::LessEq, start_pos, 2, start_line, start_column)
                    }
                    Some('<') => {
                        self.advance();
                        Token::new(TokenKind::LeftShift, start_pos, 2, start_line, start_column)
                    }
                    _ => Token::new(TokenKind::LeftAngle, start_pos, 1, start_line, start_column),
                }
            }
            Some('>') => {
                self.advance();
                match self.current_char {
                    Some('=') => {
                        self.advance();
                        Token::new(TokenKind::GreaterEq, start_pos, 2, start_line, start_column)
                    }
                    Some('>') => {
                        self.advance();
                        Token::new(TokenKind::RightShift, start_pos, 2, start_line, start_column)
                    }
                    _ => Token::new(TokenKind::RightAngle, start_pos, 1, start_line, start_column),
                }
            }
            Some('&') => {
                self.advance();
                if self.current_char == Some('&') {
                    self.advance();
                    Token::new(TokenKind::And, start_pos, 2, start_line, start_column)
                } else {
                    Token::new(TokenKind::Ampersand, start_pos, 1, start_line, start_column)
                }
            }
            Some('|') => {
                self.advance();
                if self.current_char == Some('|') {
                    self.advance();
                    Token::new(TokenKind::Or, start_pos, 2, start_line, start_column)
                } else {
                    Token::new(TokenKind::Pipe, start_pos, 1, start_line, start_column)
                }
            }
            Some('.') => {
                self.advance();
                if self.current_char == Some('.') {
                    self.advance();
                    if self.current_char == Some('=') {
                        self.advance();
                        Token::new(TokenKind::DotDotEq, start_pos, 3, start_line, start_column)
                    } else {
                        Token::new(TokenKind::DotDot, start_pos, 2, start_line, start_column)
                    }
                } else {
                    Token::new(TokenKind::Dot, start_pos, 1, start_line, start_column)
                }
            }
            Some(':') => {
                self.advance();
                if self.current_char == Some(':') {
                    self.advance();
                    Token::new(TokenKind::DoubleColon, start_pos, 2, start_line, start_column)
                } else {
                    Token::new(TokenKind::Colon, start_pos, 1, start_line, start_column)
                }
            }
            Some('(') => {
                self.advance();
                Token::new(TokenKind::LeftParen, start_pos, 1, start_line, start_column)
            }
            Some(')') => {
                self.advance();
                Token::new(TokenKind::RightParen, start_pos, 1, start_line, start_column)
            }
            Some('{') => {
                self.advance();
                Token::new(TokenKind::LeftBrace, start_pos, 1, start_line, start_column)
            }
            Some('}') => {
                self.advance();
                Token::new(TokenKind::RightBrace, start_pos, 1, start_line, start_column)
            }
            Some('[') => {
                self.advance();
                Token::new(TokenKind::LeftBracket, start_pos, 1, start_line, start_column)
            }
            Some(']') => {
                self.advance();
                Token::new(TokenKind::RightBracket, start_pos, 1, start_line, start_column)
            }
            Some(',') => {
                self.advance();
                Token::new(TokenKind::Comma, start_pos, 1, start_line, start_column)
            }
            Some(';') => {
                self.advance();
                Token::new(TokenKind::Semicolon, start_pos, 1, start_line, start_column)
            }
            Some('?') => {
                self.advance();
                Token::new(TokenKind::Question, start_pos, 1, start_line, start_column)
            }
            Some('^') => {
                self.advance();
                Token::new(TokenKind::Caret, start_pos, 1, start_line, start_column)
            }
            Some('~') => {
                self.advance();
                Token::new(TokenKind::Tilde, start_pos, 1, start_line, start_column)
            }
            Some('%') => {
                self.advance();
                Token::new(TokenKind::Percent, start_pos, 1, start_line, start_column)
            }
            Some(ch) => {
                self.advance();
                Token::new(TokenKind::Unknown(ch), start_pos, 1, start_line, start_column)
            }
        }
    }

    fn read_ident(&mut self) -> String {
        let mut ident = String::new();
        while let Some(ch) = self.current_char {
            if ch.is_alphanumeric() || ch == '_' {
                ident.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        ident
    }

    fn read_num(&mut self) -> String {
        let mut num = String::new();
        while let Some(ch) = self.current_char {
            if ch.is_numeric() || ch == '.' || ch == '_' {
                if ch != '_' {
                    num.push(ch);
                }
                self.advance();
            } else {
                break;
            }
        }
        num
    }

    fn read_str(&mut self) -> String {
        let mut string = String::new();
        self.advance();
        while let Some(ch) = self.current_char {
            if ch == '"' {
                self.advance();
                break;
            } else if ch == '\\' {
                self.advance();
                if let Some(escaped) = self.current_char {
                    match escaped {
                        'n' => string.push('\n'),
                        't' => string.push('\t'),
                        'r' => string.push('\r'),
                        '"' => string.push('"'),
                        '\\' => string.push('\\'),
                        _ => {
                            string.push('\\');
                            string.push(escaped);
                        }
                    }
                    self.advance();
                }
            } else {
                string.push(ch);
                self.advance();
            }
        }
        string
    }
}
