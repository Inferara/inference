/// Core parser implementation with resilient error recovery and advance tracking
///
/// Uses a marker-based approach similar to rust-analyzer for building syntax trees.
/// Supports error recovery and ensures forward progress during parsing.

use crate::error::{ParseError, ParseErrorCollector};
use crate::lexer::{Lexer, Token, TokenKind};
use crate::syntax_kind::SyntaxKind;

/// Marker for tracking node boundaries in parsing
#[derive(Debug, Clone, Copy)]
pub struct Marker {
    pos: usize,
}

/// Completed marker after calling complete()
#[derive(Debug, Clone, Copy)]
pub struct CompletedMarker {
    _pos: usize,
}

impl Marker {
    pub fn complete(self, _p: &mut Parser, _kind: SyntaxKind) -> CompletedMarker {
        CompletedMarker { _pos: self.pos }
    }

    pub fn precede(self, _p: &mut Parser) -> Marker {
        Marker { pos: self.pos }
    }
}

impl CompletedMarker {
    pub fn precede(self, _p: &mut Parser) -> Marker {
        Marker { pos: self._pos }
    }
}

/// Parser with advance tracking mechanism for preventing infinite loops
#[derive(Debug)]
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    advance_stack: Vec<usize>,
    errors: ParseErrorCollector,
}

impl Parser {
    pub fn new(source: &str) -> Self {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();
        loop {
            let token = lexer.next_token();
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        // Ensure we always have at least one EOF token
        if tokens.is_empty() {
            tokens.push(Token::new(TokenKind::Eof, 0, 0, 1, 1));
        }
        Self {
            tokens,
            pos: 0,
            advance_stack: Vec::new(),
            errors: ParseErrorCollector::new(),
        }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len() || self.current_token().kind == TokenKind::Eof
    }

    fn synchronize(&mut self) {
        while !self.is_eof() {
            match &self.current_token().kind {
                TokenKind::Fn | TokenKind::Let | TokenKind::Type | TokenKind::Struct
                | TokenKind::Enum | TokenKind::Impl | TokenKind::Semicolon => break,
                _ => {
                    if !self.is_eof() {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
            }
        }
    }


    pub fn errors(&self) -> Vec<ParseError> {
        self.errors.clone().take_errors()
    }

    // === Public API for grammar modules ===

    pub fn start(&mut self) -> Marker {
        Marker { pos: self.pos }
    }

    pub fn at(&self, kind: SyntaxKind) -> bool {
        if self.is_eof() {
            return false;
        }
        crate::token_kind_bridge::from_token_kind(&self.current_token().kind) == kind
    }

    pub fn at_contextual_kw(&self, _kw: &str) -> bool {
        if self.is_eof() {
            return false;
        }
        matches!(self.current_token().kind, TokenKind::Identifier(_))
    }

    pub fn at_eof(&self) -> bool {
        self.is_eof()
    }

    pub fn current(&self) -> SyntaxKind {
        if self.is_eof() {
            SyntaxKind::EOF
        } else {
            crate::token_kind_bridge::from_token_kind(&self.current_token().kind)
        }
    }

    fn current_token(&self) -> &Token {
        &self.tokens[self.pos]
    }

    pub fn bump(&mut self) {
        if !self.is_eof() {
            self.pos += 1;
        }
    }

    pub fn expect(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            self.error(format!("expected {:?}", kind));
            false
        }
    }

    pub fn eat_whitespace_and_comments(&mut self) {
        while matches!(self.current_token().kind, TokenKind::Newline) {
            self.bump();
        }
    }

    pub fn error(&mut self, message: impl Into<String>) {
        let err = ParseError::InvalidSyntax {
            pos: self.current_token().pos,
            reason: message.into(),
        };
        self.errors.add_error(err);
    }

    /// Parse a complete module
    pub fn parse_module(&mut self) -> Result<(), Vec<ParseError>> {
        while !self.at_eof() {
            crate::grammar::parse_item(self);
        }
        
        let errors = self.errors();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}
