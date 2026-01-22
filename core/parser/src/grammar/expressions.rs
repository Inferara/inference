/// Parsing of expressions with operator precedence

use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;

/// Parse a general expression
pub fn parse_expr(p: &mut Parser) {
    parse_assignment_expr(p);
}

/// Parse assignment expression
fn parse_assignment_expr(p: &mut Parser) {
    parse_logical_or_expr(p);
    
    if p.at(SyntaxKind::ASSIGN) || p.at(SyntaxKind::PLUS_ASSIGN) || 
       p.at(SyntaxKind::MINUS_ASSIGN) || p.at(SyntaxKind::MUL_ASSIGN) || 
       p.at(SyntaxKind::DIV_ASSIGN) {
        p.bump();
        parse_assignment_expr(p);
    }
}

/// Parse logical OR expression
fn parse_logical_or_expr(p: &mut Parser) {
    parse_logical_and_expr(p);
    
    while p.at(SyntaxKind::OR) {
        p.bump();
        parse_logical_and_expr(p);
    }
}

/// Parse logical AND expression
fn parse_logical_and_expr(p: &mut Parser) {
    parse_equality_expr(p);
    
    while p.at(SyntaxKind::AND) {
        p.bump();
        parse_equality_expr(p);
    }
}

/// Parse equality expression
fn parse_equality_expr(p: &mut Parser) {
    parse_comparison_expr(p);
    
    while p.at(SyntaxKind::EQ) || p.at(SyntaxKind::NOT_EQ) {
        p.bump();
        parse_comparison_expr(p);
    }
}

/// Parse comparison expression
fn parse_comparison_expr(p: &mut Parser) {
    parse_additive_expr(p);
    
    while p.at(SyntaxKind::LT) || p.at(SyntaxKind::LE) || 
          p.at(SyntaxKind::GT) || p.at(SyntaxKind::GE) {
        p.bump();
        parse_additive_expr(p);
    }
}

/// Parse additive expression
fn parse_additive_expr(p: &mut Parser) {
    parse_multiplicative_expr(p);
    
    while p.at(SyntaxKind::PLUS) || p.at(SyntaxKind::MINUS) {
        p.bump();
        parse_multiplicative_expr(p);
    }
}

/// Parse multiplicative expression
fn parse_multiplicative_expr(p: &mut Parser) {
    parse_postfix_expr(p);
    
    while p.at(SyntaxKind::MUL) || p.at(SyntaxKind::DIV) || 
          p.at(SyntaxKind::MOD) {
        p.bump();
        parse_postfix_expr(p);
    }
}

/// Parse postfix expression (calls, field access, indexing)
fn parse_postfix_expr(p: &mut Parser) {
    parse_prefix_expr(p);
    
    loop {
        match p.current() {
            SyntaxKind::L_PAREN => parse_call_expr(p),
            SyntaxKind::DOT => parse_field_access(p),
            SyntaxKind::L_BRACKET => parse_index_expr(p),
            _ => break,
        }
    }
}

/// Parse function call
fn parse_call_expr(p: &mut Parser) {
    p.expect(SyntaxKind::L_PAREN);
    
    while !p.at(SyntaxKind::R_PAREN) && !p.at_eof() {
        parse_expr(p);
        if !p.at(SyntaxKind::R_PAREN) {
            p.expect(SyntaxKind::COMMA);
        }
    }
    
    p.expect(SyntaxKind::R_PAREN);
}

/// Parse field access
fn parse_field_access(p: &mut Parser) {
    p.expect(SyntaxKind::DOT);
    if !p.at_eof() {
        p.bump(); // field name
    }
}

/// Parse index expression
fn parse_index_expr(p: &mut Parser) {
    p.expect(SyntaxKind::L_BRACKET);
    parse_expr(p);
    p.expect(SyntaxKind::R_BRACKET);
}

/// Parse prefix expression (unary operators)
fn parse_prefix_expr(p: &mut Parser) {
    match p.current() {
        SyntaxKind::NOT | SyntaxKind::MINUS | SyntaxKind::MUL | SyntaxKind::AND => {
            p.bump();
            parse_prefix_expr(p);
        }
        _ => parse_primary_expr(p),
    }
}

/// Parse primary expression
fn parse_primary_expr(p: &mut Parser) {
    match p.current() {
        SyntaxKind::INTEGER_LITERAL | 
        SyntaxKind::FLOAT_LITERAL |
        SyntaxKind::STRING_LITERAL |
        SyntaxKind::CHAR_LITERAL |
        SyntaxKind::TRUE |
        SyntaxKind::FALSE => {
            p.bump();
        }
        SyntaxKind::IDENT => {
            p.bump();
        }
        SyntaxKind::L_PAREN => {
            p.bump();
            parse_expr(p);
            p.expect(SyntaxKind::R_PAREN);
        }
        SyntaxKind::L_BRACE => parse_block_expr(p),
        SyntaxKind::IF => parse_if_expr(p),
        SyntaxKind::MATCH => parse_match_expr(p),
        SyntaxKind::WHILE => parse_while_expr(p),
        SyntaxKind::FOR => parse_for_expr(p),
        SyntaxKind::LOOP => parse_loop_expr(p),
        SyntaxKind::RETURN => parse_return_expr(p),
        SyntaxKind::BREAK => parse_break_expr(p),
        SyntaxKind::CONTINUE => {
            p.bump();
        }
        _ => {
            p.error("expected expression");
        }
    }
}

/// Parse block expression
pub fn parse_block_expr(p: &mut Parser) {
    p.expect(SyntaxKind::L_BRACE);
    
    while !p.at(SyntaxKind::R_BRACE) && !p.at_eof() {
        match p.current() {
            SyntaxKind::LET => {
                super::items::parse_let_statement(p);
            }
            SyntaxKind::RETURN => parse_return_expr(p),
            SyntaxKind::BREAK => parse_break_expr(p),
            SyntaxKind::CONTINUE => {
                p.bump();
                p.expect(SyntaxKind::SEMICOLON);
            }
            _ => {
                parse_expr(p);
                if p.at(SyntaxKind::SEMICOLON) {
                    p.bump();
                }
            }
        }
    }
    
    p.expect(SyntaxKind::R_BRACE);
}

/// Parse if expression
pub fn parse_if_expr(p: &mut Parser) {
    p.expect(SyntaxKind::IF);
    parse_expr(p);
    parse_block_expr(p);
    
    if p.at(SyntaxKind::ELSE) {
        p.bump();
        if p.at(SyntaxKind::IF) {
            parse_if_expr(p);
        } else {
            parse_block_expr(p);
        }
    }
}

/// Parse match expression
fn parse_match_expr(p: &mut Parser) {
    p.expect(SyntaxKind::MATCH);
    parse_expr(p);
    p.expect(SyntaxKind::L_BRACE);
    
    while !p.at(SyntaxKind::R_BRACE) && !p.at_eof() {
        super::patterns::parse_pattern(p);
        p.expect(SyntaxKind::ARROW);
        parse_expr(p);
        
        if !p.at(SyntaxKind::R_BRACE) {
            p.expect(SyntaxKind::COMMA);
        }
    }
    
    p.expect(SyntaxKind::R_BRACE);
}

/// Parse while loop
pub fn parse_while_expr(p: &mut Parser) {
    p.expect(SyntaxKind::WHILE);
    parse_expr(p);
    parse_block_expr(p);
}

/// Parse for loop
pub fn parse_for_expr(p: &mut Parser) {
    p.expect(SyntaxKind::FOR);
    super::patterns::parse_pattern(p);
    p.expect(SyntaxKind::IN);
    parse_expr(p);
    parse_block_expr(p);
}

/// Parse loop expression
pub fn parse_loop_expr(p: &mut Parser) {
    p.expect(SyntaxKind::LOOP);
    parse_block_expr(p);
}

/// Parse return expression
pub fn parse_return_expr(p: &mut Parser) {
    p.expect(SyntaxKind::RETURN);
    if !p.at(SyntaxKind::SEMICOLON) && !p.at(SyntaxKind::R_BRACE) && !p.at_eof() {
        parse_expr(p);
    }
    if p.at(SyntaxKind::SEMICOLON) {
        p.bump();
    }
}

/// Parse break expression
pub fn parse_break_expr(p: &mut Parser) {
    p.expect(SyntaxKind::BREAK);
    if !p.at(SyntaxKind::SEMICOLON) && !p.at(SyntaxKind::R_BRACE) && !p.at_eof() {
        parse_expr(p);
    }
    if p.at(SyntaxKind::SEMICOLON) {
        p.bump();
    }
}
