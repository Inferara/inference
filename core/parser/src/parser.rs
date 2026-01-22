/// Core parser implementation with resilient error recovery and advance tracking
///
/// Uses advance tracking to prevent infinite loops and ensure forward progress.
/// Each parse attempt must consume tokens or report an error.

use crate::error::{ParseError, ParseErrorCollector};
use crate::lexer::{Lexer, Token, TokenKind};

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
        Self {
            tokens,
            pos: 0,
            advance_stack: Vec::new(),
            errors: ParseErrorCollector::new(),
        }
    }

    #[inline]
    fn current(&self) -> &Token {
        &self.tokens[self.pos]
    }

    #[inline]
    fn at(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(kind)
    }

    #[inline]
    fn is_eof(&self) -> bool {
        matches!(self.current().kind, TokenKind::Eof)
    }

    fn bump(&mut self) -> Token {
        let token = self.tokens[self.pos].clone();
        if !self.is_eof() {
            self.pos += 1;
        }
        token
    }

    #[inline]
    fn advance_push(&mut self) {
        self.advance_stack.push(self.pos);
    }

    fn advance_pop(&mut self) {
        self.advance_stack.pop();
    }

    fn expect(&mut self, expected: TokenKind) -> Result<Token, ParseError> {
        if std::mem::discriminant(&self.current().kind) == std::mem::discriminant(&expected) {
            Ok(self.bump())
        } else {
            let err = ParseError::UnexpectedToken {
                pos: self.current().pos,
                expected: format!("{:?}", expected),
                found: format!("{:?}", self.current().kind),
            };
            self.error(err)
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match &self.current().kind {
            TokenKind::Identifier(name) => {
                let name = name.clone();
                self.bump();
                Ok(name)
            }
            _ => self.error(ParseError::UnexpectedToken {
                pos: self.current().pos,
                expected: "identifier".to_string(),
                found: format!("{:?}", self.current().kind),
            }),
        }
    }

    fn error<T>(&mut self, err: ParseError) -> Result<T, ParseError> {
        self.errors.add_error(err.clone());
        Err(err)
    }

    fn synchronize(&mut self) {
        while !self.is_eof() {
            match &self.current().kind {
                TokenKind::Fn | TokenKind::Let | TokenKind::Type | TokenKind::Struct
                | TokenKind::Enum | TokenKind::Impl | TokenKind::Semicolon => break,
                _ => {
                    self.bump();
                }
            }
        }
    }

    pub fn parse_module(&mut self) -> Result<(), Vec<ParseError>> {
        self.advance_push();
        while !self.is_eof() {
            while self.at(&TokenKind::Newline) {
                self.bump();
            }
            if !self.is_eof() {
                if self.parse_item().is_err() {
                    self.synchronize();
                }
            }
        }
        self.advance_pop();
        if self.errors.has_errors() {
            Err(self.errors.clone().take_errors())
        } else {
            Ok(())
        }
    }

    fn parse_item(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        if self.at(&TokenKind::Pub) {
            self.bump();
        }
        match &self.current().kind {
            TokenKind::Fn => self.parse_function_def(),
            TokenKind::Let => self.parse_variable_decl(),
            TokenKind::Const => self.parse_const_decl(),
            TokenKind::Type => self.parse_type_alias(),
            TokenKind::Struct => self.parse_struct_def(),
            TokenKind::Enum => self.parse_enum_def(),
            TokenKind::Impl => self.parse_impl_block(),
            TokenKind::Import => self.parse_import(),
            TokenKind::Trait => self.parse_trait_def(),
            _ => self.error(ParseError::InvalidSyntax {
                pos: self.current().pos,
                reason: format!("expected item, found {:?}", self.current().kind),
            }),
        }?;
        self.advance_pop();
        Ok(())
    }

    fn parse_function_def(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        self.expect(TokenKind::Fn)?;
        self.expect_ident()?;
        self.expect(TokenKind::LeftParen)?;
        while !self.at(&TokenKind::RightParen) && !self.is_eof() {
            self.parse_parameter()?;
            if !self.at(&TokenKind::RightParen) {
                self.expect(TokenKind::Comma)?;
            }
        }
        self.expect(TokenKind::RightParen)?;
        if self.at(&TokenKind::Arrow) {
            self.bump();
            self.parse_type()?;
        }
        self.expect(TokenKind::LeftBrace)?;
        while !self.at(&TokenKind::RightBrace) && !self.is_eof() {
            self.parse_statement()?;
        }
        self.expect(TokenKind::RightBrace)?;
        self.advance_pop();
        Ok(())
    }

    fn parse_parameter(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        if self.at(&TokenKind::Mut) {
            self.bump();
        }
        self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        self.parse_type()?;
        self.advance_pop();
        Ok(())
    }

    fn parse_variable_decl(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        self.expect(TokenKind::Let)?;
        if self.at(&TokenKind::Mut) {
            self.bump();
        }
        self.expect_ident()?;
        if self.at(&TokenKind::Colon) {
            self.bump();
            self.parse_type()?;
        }
        if self.at(&TokenKind::Assign) {
            self.bump();
            self.parse_expr()?;
        }
        self.expect(TokenKind::Semicolon)?;
        self.advance_pop();
        Ok(())
    }

    fn parse_const_decl(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        self.expect(TokenKind::Const)?;
        self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        self.parse_type()?;
        self.expect(TokenKind::Assign)?;
        self.parse_expr()?;
        self.expect(TokenKind::Semicolon)?;
        self.advance_pop();
        Ok(())
    }

    fn parse_type_alias(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        self.expect(TokenKind::Type)?;
        self.expect_ident()?;
        self.expect(TokenKind::Assign)?;
        self.parse_type()?;
        self.expect(TokenKind::Semicolon)?;
        self.advance_pop();
        Ok(())
    }

    fn parse_struct_def(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        self.expect(TokenKind::Struct)?;
        self.expect_ident()?;
        if self.at(&TokenKind::LeftAngle) {
            self.parse_generics()?;
        }
        self.expect(TokenKind::LeftBrace)?;
        self.parse_field_list()?;
        self.expect(TokenKind::RightBrace)?;
        self.advance_pop();
        Ok(())
    }

    fn parse_enum_def(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        self.expect(TokenKind::Enum)?;
        self.expect_ident()?;
        if self.at(&TokenKind::LeftAngle) {
            self.parse_generics()?;
        }
        self.expect(TokenKind::LeftBrace)?;
        while !self.at(&TokenKind::RightBrace) && !self.is_eof() {
            self.expect_ident()?;
            if self.at(&TokenKind::LeftParen) {
                self.bump();
                while !self.at(&TokenKind::RightParen) && !self.is_eof() {
                    self.parse_type()?;
                    if !self.at(&TokenKind::RightParen) {
                        self.expect(TokenKind::Comma)?;
                    }
                }
                self.expect(TokenKind::RightParen)?;
            }
            if !self.at(&TokenKind::RightBrace) {
                self.expect(TokenKind::Comma)?;
            }
        }
        self.expect(TokenKind::RightBrace)?;
        self.advance_pop();
        Ok(())
    }

    fn parse_impl_block(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        self.expect(TokenKind::Impl)?;
        self.parse_type()?;
        self.expect(TokenKind::LeftBrace)?;
        while !self.at(&TokenKind::RightBrace) && !self.is_eof() {
            self.parse_function_def()?;
        }
        self.expect(TokenKind::RightBrace)?;
        self.advance_pop();
        Ok(())
    }

    fn parse_trait_def(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        self.expect(TokenKind::Trait)?;
        self.expect_ident()?;
        self.expect(TokenKind::LeftBrace)?;
        while !self.at(&TokenKind::RightBrace) && !self.is_eof() {
            self.parse_function_def()?;
        }
        self.expect(TokenKind::RightBrace)?;
        self.advance_pop();
        Ok(())
    }

    fn parse_import(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        self.expect(TokenKind::Import)?;
        self.parse_path()?;
        if self.at(&TokenKind::As) {
            self.bump();
            self.expect_ident()?;
        }
        self.expect(TokenKind::Semicolon)?;
        self.advance_pop();
        Ok(())
    }

    fn parse_statement(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        match &self.current().kind {
            TokenKind::Let => self.parse_variable_decl()?,
            TokenKind::Return => {
                self.bump();
                if !self.at(&TokenKind::Semicolon) {
                    self.parse_expr()?;
                }
                self.expect(TokenKind::Semicolon)?;
            }
            TokenKind::If => self.parse_if()?,
            TokenKind::While => self.parse_while()?,
            TokenKind::For => self.parse_for()?,
            TokenKind::LeftBrace => {
                self.bump();
                while !self.at(&TokenKind::RightBrace) && !self.is_eof() {
                    self.parse_statement()?;
                }
                self.expect(TokenKind::RightBrace)?;
            }
            _ => {
                self.parse_expr()?;
                if self.at(&TokenKind::Semicolon) {
                    self.bump();
                }
            }
        }
        self.advance_pop();
        Ok(())
    }

    fn parse_if(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        self.expect(TokenKind::If)?;
        self.parse_expr()?;
        self.parse_block()?;
        if self.at(&TokenKind::Else) {
            self.bump();
            if self.at(&TokenKind::If) {
                self.parse_if()?;
            } else {
                self.parse_block()?;
            }
        }
        self.advance_pop();
        Ok(())
    }

    fn parse_while(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        self.expect(TokenKind::While)?;
        self.parse_expr()?;
        self.parse_block()?;
        self.advance_pop();
        Ok(())
    }

    fn parse_for(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        self.expect(TokenKind::For)?;
        self.expect_ident()?;
        self.expect(TokenKind::In)?;
        self.parse_expr()?;
        self.parse_block()?;
        self.advance_pop();
        Ok(())
    }

    fn parse_block(&mut self) -> Result<(), ParseError> {
        self.expect(TokenKind::LeftBrace)?;
        while !self.at(&TokenKind::RightBrace) && !self.is_eof() {
            self.parse_statement()?;
        }
        self.expect(TokenKind::RightBrace)
            .map(|_| ())
    }

    fn parse_expr(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        self.parse_primary()?;
        loop {
            match &self.current().kind {
                TokenKind::Plus | TokenKind::Minus | TokenKind::Star | TokenKind::Slash
                | TokenKind::Percent | TokenKind::EqEq | TokenKind::NotEq | TokenKind::Less
                | TokenKind::LessEq | TokenKind::Greater | TokenKind::GreaterEq
                | TokenKind::And | TokenKind::Or => {
                    self.bump();
                    self.parse_primary()?;
                }
                TokenKind::Dot => {
                    self.bump();
                    self.expect_ident()?;
                    if self.at(&TokenKind::LeftParen) {
                        self.bump();
                        self.parse_call_args()?;
                        self.expect(TokenKind::RightParen)?;
                    }
                }
                TokenKind::LeftBracket => {
                    self.bump();
                    self.parse_expr()?;
                    self.expect(TokenKind::RightBracket)?;
                }
                _ => break,
            }
        }
        self.advance_pop();
        Ok(())
    }

    fn parse_primary(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        match &self.current().kind {
            TokenKind::Identifier(_) => {
                self.bump();
                if self.at(&TokenKind::LeftParen) {
                    self.bump();
                    self.parse_call_args()?;
                    self.expect(TokenKind::RightParen)?;
                }
            }
            TokenKind::Number(_) | TokenKind::String(_) => {
                self.bump();
            }
            TokenKind::LeftParen => {
                self.bump();
                self.parse_expr()?;
                self.expect(TokenKind::RightParen)?;
            }
            TokenKind::Not | TokenKind::Minus | TokenKind::Ampersand => {
                self.bump();
                self.parse_primary()?;
            }
            TokenKind::LeftBracket => {
                self.bump();
                if !self.at(&TokenKind::RightBracket) {
                    self.parse_expr()?;
                }
                self.expect(TokenKind::RightBracket)?;
            }
            _ => {
                // Skip invalid token to prevent infinite loops
                if !self.is_eof() {
                    self.bump();
                }
                let err = ParseError::InvalidSyntax {
                    pos: self.current().pos,
                    reason: format!("expected expression"),
                };
                self.error(err)?;
            }
        }
        self.advance_pop();
        Ok(())
    }

    fn parse_call_args(&mut self) -> Result<(), ParseError> {
        while !self.at(&TokenKind::RightParen) && !self.is_eof() {
            self.parse_expr()?;
            if !self.at(&TokenKind::RightParen) {
                self.expect(TokenKind::Comma)?;
            }
        }
        Ok(())
    }

    fn parse_type(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        if self.at(&TokenKind::Ampersand) {
            self.bump();
            if self.at(&TokenKind::Mut) {
                self.bump();
            }
        }
        self.expect_ident()?;
        if self.at(&TokenKind::LeftAngle) {
            self.parse_type_args()?;
        }
        if self.at(&TokenKind::LeftBracket) {
            self.bump();
            match &self.current().kind {
                TokenKind::Number(_) => {
                    self.bump();
                }
                _ => {}
            }
            self.expect(TokenKind::RightBracket)?;
        }
        self.advance_pop();
        Ok(())
    }

    fn parse_generics(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        self.expect(TokenKind::LeftAngle)?;
        while !self.at(&TokenKind::RightAngle) && !self.is_eof() {
            self.expect_ident()?;
            if self.at(&TokenKind::Colon) {
                self.bump();
                self.expect_ident()?;
            }
            if !self.at(&TokenKind::RightAngle) {
                self.expect(TokenKind::Comma)?;
            }
        }
        self.expect(TokenKind::RightAngle)?;
        self.advance_pop();
        Ok(())
    }

    fn parse_type_args(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        self.expect(TokenKind::LeftAngle)?;
        while !self.at(&TokenKind::RightAngle) && !self.is_eof() {
            self.parse_type()?;
            if !self.at(&TokenKind::RightAngle) {
                self.expect(TokenKind::Comma)?;
            }
        }
        self.expect(TokenKind::RightAngle)?;
        self.advance_pop();
        Ok(())
    }

    fn parse_field_list(&mut self) -> Result<(), ParseError> {
        while !self.at(&TokenKind::RightBrace) && !self.is_eof() {
            self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            self.parse_type()?;
            if !self.at(&TokenKind::RightBrace) {
                self.expect(TokenKind::Comma)?;
            }
        }
        Ok(())
    }

    fn parse_path(&mut self) -> Result<(), ParseError> {
        self.advance_push();
        self.expect_ident()?;
        while self.at(&TokenKind::DoubleColon) {
            self.bump();
            self.expect_ident()?;
        }
        self.advance_pop();
        Ok(())
    }

    pub fn errors(&self) -> Vec<ParseError> {
        self.errors.clone().take_errors()
    }
}
