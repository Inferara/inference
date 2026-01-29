/// Expression parsing

use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;

/// Parse an expression
pub fn parse_expression(p: &mut Parser) {
    parse_assignment(p);
}

/// Parse assignment or lower precedence
fn parse_assignment(p: &mut Parser) {
    parse_logical_or(p);
    
    if p.at(SyntaxKind::ASSIGN) {
        p.bump();
        parse_assignment(p);
    }
}

/// Parse logical OR
fn parse_logical_or(p: &mut Parser) {
    parse_logical_and(p);
    
    while p.at(SyntaxKind::OR) {
        p.bump();
        parse_logical_and(p);
    }
}

/// Parse logical AND
fn parse_logical_and(p: &mut Parser) {
    parse_comparison(p);
    
    while p.at(SyntaxKind::AND) {
        p.bump();
        parse_comparison(p);
    }
}

/// Parse comparison operators
fn parse_comparison(p: &mut Parser) {
    parse_additive(p);
    
    while matches!(
        p.current(),
        SyntaxKind::EQ_EQ
            | SyntaxKind::NOT_EQ
            | SyntaxKind::LESS
            | SyntaxKind::LESS_EQ
            | SyntaxKind::GREATER
            | SyntaxKind::GREATER_EQ
    ) {
        p.bump();
        parse_additive(p);
    }
}

/// Parse additive operators
fn parse_additive(p: &mut Parser) {
    parse_multiplicative(p);
    
    while matches!(p.current(), SyntaxKind::PLUS | SyntaxKind::MINUS) {
        p.bump();
        parse_multiplicative(p);
    }
}

/// Parse multiplicative operators
fn parse_multiplicative(p: &mut Parser) {
    parse_unary(p);
    
    while matches!(
        p.current(),
        SyntaxKind::STAR | SyntaxKind::SLASH | SyntaxKind::PERCENT
    ) {
        p.bump();
        parse_unary(p);
    }
}

/// Parse unary operators
fn parse_unary(p: &mut Parser) {
    if matches!(
        p.current(),
        SyntaxKind::NOT | SyntaxKind::MINUS | SyntaxKind::AMPERSAND | SyntaxKind::STAR
    ) {
        p.bump();
        parse_unary(p);
    } else {
        parse_postfix(p);
    }
}

/// Parse postfix operators (field access, indexing, calls)
fn parse_postfix(p: &mut Parser) {
    parse_primary(p);
    
    loop {
        if p.at(SyntaxKind::DOT) {
            p.bump();
            p.bump(); // field name
            
            if p.at(SyntaxKind::L_PAREN) {
                parse_call_args(p);
            }
        } else if p.at(SyntaxKind::L_BRACKET) {
            p.bump();
            parse_expression(p);
            if p.at(SyntaxKind::R_BRACKET) {
                p.bump();
            }
        } else if p.at(SyntaxKind::L_PAREN) && is_likely_call() {
            parse_call_args(p);
        } else {
            break;
        }
    }
}

/// Parse primary expression
pub fn parse_primary(p: &mut Parser) {
    match p.current() {
        SyntaxKind::TRUE | SyntaxKind::FALSE => p.bump(),
        SyntaxKind::INT_NUMBER | SyntaxKind::FLOAT_NUMBER | SyntaxKind::STRING | SyntaxKind::CHAR => {
            p.bump()
        }
        SyntaxKind::IDENT => {
            p.bump();
        }
        SyntaxKind::L_PAREN => {
            p.bump();
            parse_expression(p);
            if p.at(SyntaxKind::R_PAREN) {
                p.bump();
            }
        }
        SyntaxKind::L_BRACKET => {
            p.bump();
            while !p.at(SyntaxKind::R_BRACKET) && !p.at_eof() {
                parse_expression(p);
                if p.at(SyntaxKind::COMMA) {
                    p.bump();
                }
            }
            if p.at(SyntaxKind::R_BRACKET) {
                p.bump();
            }
        }
        SyntaxKind::IF => parse_if_expr(p),
        SyntaxKind::WHILE => parse_while_expr(p),
        SyntaxKind::FOR => parse_for_expr(p),
        SyntaxKind::LOOP => parse_loop_expr(p),
        SyntaxKind::MATCH => parse_match_expr(p),
        _ => {
            if !p.at_eof() {
                p.bump();
            }
        }
    }
}

/// Parse if expression
pub fn parse_if_expr(p: &mut Parser) {
    p.expect(SyntaxKind::IF);
    parse_expression(p);
    super::items::parse_block(p);
    
    while p.at(SyntaxKind::ELSE) {
        p.bump();
        if p.at(SyntaxKind::IF) {
            parse_if_expr(p);
        } else if p.at(SyntaxKind::L_BRACE) {
            super::items::parse_block(p);
        }
    }
}

/// Parse while expression
pub fn parse_while_expr(p: &mut Parser) {
    p.expect(SyntaxKind::WHILE);
    parse_expression(p);
    super::items::parse_block(p);
}

/// Parse for expression
pub fn parse_for_expr(p: &mut Parser) {
    p.expect(SyntaxKind::FOR);
    p.bump(); // loop variable
    
    if p.at(SyntaxKind::IN) {
        p.bump();
    }
    
    parse_expression(p);
    super::items::parse_block(p);
}

/// Parse loop expression
pub fn parse_loop_expr(p: &mut Parser) {
    p.expect(SyntaxKind::LOOP);
    super::items::parse_block(p);
}

/// Parse match expression
pub fn parse_match_expr(p: &mut Parser) {
    p.expect(SyntaxKind::MATCH);
    parse_expression(p);
    
    if p.at(SyntaxKind::L_BRACE) {
        p.bump();
        
        while !p.at(SyntaxKind::R_BRACE) && !p.at_eof() {
            // Pattern
            parse_expression(p);
            
            if p.at(SyntaxKind::FAT_ARROW) {
                p.bump();
            }
            
            // Expression
            parse_expression(p);
            
            if p.at(SyntaxKind::COMMA) {
                p.bump();
            }
        }
        
        if p.at(SyntaxKind::R_BRACE) {
            p.bump();
        }
    }
}

/// Parse return expression
pub fn parse_return_expr(p: &mut Parser) {
    p.expect(SyntaxKind::RETURN);
    
    if !p.at(SyntaxKind::SEMICOLON) && !p.at(SyntaxKind::R_BRACE) {
        parse_expression(p);
    }
}

/// Parse function call arguments
fn parse_call_args(p: &mut Parser) {
    if !p.at(SyntaxKind::L_PAREN) {
        return;
    }
    p.bump();
    
    while !p.at(SyntaxKind::R_PAREN) && !p.at_eof() {
        parse_expression(p);
        if p.at(SyntaxKind::COMMA) {
            p.bump();
        }
    }
    
    if p.at(SyntaxKind::R_PAREN) {
        p.bump();
    }
}

/// Quick heuristic to determine if this is a function call
fn is_likely_call() -> bool {
    // In a real parser, we'd look back to check if we're on an identifier
    // For now, just return true since parse_call_args checks for L_PAREN anyway
    true
}
