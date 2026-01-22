/// Type expression parsing

use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;

/// Parse a type expression
pub fn parse_type(p: &mut Parser) {
    // Handle reference/pointer prefixes
    if p.at(SyntaxKind::AMPERSAND) {
        p.bump();
        if p.at(SyntaxKind::MUT) {
            p.bump();
        }
    } else if p.at(SyntaxKind::STAR) {
        p.bump();
        if p.at(SyntaxKind::MUT) || p.at(SyntaxKind::REF) {
            p.bump();
        }
    }
    
    // Parse base type name
    if p.at(SyntaxKind::L_PAREN) {
        // Function type or tuple
        parse_tuple_type(p);
    } else {
        p.bump(); // type name
    }
    
    // Parse generic parameters
    if p.at(SyntaxKind::L_ANGLE) {
        parse_generic_args(p);
    }
    
    // Parse array type
    if p.at(SyntaxKind::L_BRACKET) {
        p.bump();
        if !p.at(SyntaxKind::R_BRACKET) {
            p.bump(); // array size
        }
        if p.at(SyntaxKind::R_BRACKET) {
            p.bump();
        }
    }
}

/// Parse tuple type (T, U, V)
fn parse_tuple_type(p: &mut Parser) {
    p.bump(); // '('
    
    while !p.at(SyntaxKind::R_PAREN) && !p.at_eof() {
        parse_type(p);
        if p.at(SyntaxKind::COMMA) {
            p.bump();
        }
    }
    
    if p.at(SyntaxKind::R_PAREN) {
        p.bump();
    }
}

/// Parse generic arguments <T, U>
fn parse_generic_args(p: &mut Parser) {
    p.bump(); // '<'
    
    while !p.at(SyntaxKind::R_ANGLE) && !p.at_eof() {
        parse_type(p);
        if p.at(SyntaxKind::COMMA) {
            p.bump();
        }
    }
    
    if p.at(SyntaxKind::R_ANGLE) {
        p.bump();
    }
}
