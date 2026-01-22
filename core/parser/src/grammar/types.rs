/// Parsing of type expressions

use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;

/// Parse a type expression
pub fn parse_type(p: &mut Parser) {
    parse_type_inner(p);
}

fn parse_type_inner(p: &mut Parser) {
    match p.current() {
        SyntaxKind::IDENT => {
            p.bump();
            
            // Handle generics like Vec<T>
            if p.at(SyntaxKind::L_ANGLE) {
                parse_generic_args(p);
            }
        }
        SyntaxKind::L_PAREN => {
            p.bump();
            // Tuple type
            while !p.at(SyntaxKind::R_PAREN) && !p.at_eof() {
                parse_type_inner(p);
                if !p.at(SyntaxKind::R_PAREN) {
                    p.expect(SyntaxKind::COMMA);
                }
            }
            p.expect(SyntaxKind::R_PAREN);
        }
        SyntaxKind::L_BRACKET => {
            p.bump();
            // Array or slice type
            parse_type_inner(p);
            if p.at(SyntaxKind::SEMICOLON) {
                p.bump();
                // Array with explicit length
                p.bump();
            }
            p.expect(SyntaxKind::R_BRACKET);
        }
        SyntaxKind::AND => {
            p.bump();
            // Reference type
            if p.at(SyntaxKind::MUT) {
                p.bump();
            }
            parse_type_inner(p);
        }
        SyntaxKind::MUL => {
            p.bump();
            // Pointer type
            parse_type_inner(p);
        }
        SyntaxKind::FN => {
            p.bump();
            // Function pointer type
            parse_fn_type_params(p);
            if p.at(SyntaxKind::ARROW) {
                p.bump();
                parse_type_inner(p);
            }
        }
        _ => {
            p.error("expected type");
        }
    }
}

/// Parse generic type arguments
fn parse_generic_args(p: &mut Parser) {
    p.expect(SyntaxKind::L_ANGLE);
    
    while !p.at(SyntaxKind::R_ANGLE) && !p.at_eof() {
        parse_type_inner(p);
        if !p.at(SyntaxKind::R_ANGLE) {
            p.expect(SyntaxKind::COMMA);
        }
    }
    
    p.expect(SyntaxKind::R_ANGLE);
}

/// Parse function type parameters
fn parse_fn_type_params(p: &mut Parser) {
    p.expect(SyntaxKind::L_PAREN);
    
    while !p.at(SyntaxKind::R_PAREN) && !p.at_eof() {
        parse_type_inner(p);
        if !p.at(SyntaxKind::R_PAREN) {
            p.expect(SyntaxKind::COMMA);
        }
    }
    
    p.expect(SyntaxKind::R_PAREN);
}
