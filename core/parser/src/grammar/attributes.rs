/// Parsing of attributes

use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;

/// Parse an attribute
pub fn parse_attribute(p: &mut Parser) {
    p.expect(SyntaxKind::HASH);
    
    if p.at(SyntaxKind::L_BRACKET) {
        p.bump();
        
        // Parse attribute path
        parse_attribute_path(p);
        
        // Parse attribute arguments if any
        if p.at(SyntaxKind::L_PAREN) {
            p.bump();
            while !p.at(SyntaxKind::R_PAREN) && !p.at_eof() {
                p.bump();
            }
            p.expect(SyntaxKind::R_PAREN);
        }
        
        p.expect(SyntaxKind::R_BRACKET);
    } else if p.at(SyntaxKind::NOT) {
        p.bump();
        p.expect(SyntaxKind::L_BRACKET);
        
        // Parse inner attributes
        parse_attribute_path(p);
        
        if p.at(SyntaxKind::L_PAREN) {
            p.bump();
            while !p.at(SyntaxKind::R_PAREN) && !p.at_eof() {
                p.bump();
            }
            p.expect(SyntaxKind::R_PAREN);
        }
        
        p.expect(SyntaxKind::R_BRACKET);
    }
}

/// Parse attribute path (e.g., derive, deprecated, etc.)
fn parse_attribute_path(p: &mut Parser) {
    if p.at(SyntaxKind::IDENT) {
        p.bump();
        
        while p.at(SyntaxKind::COLON_COLON) {
            p.bump();
            if p.at(SyntaxKind::IDENT) {
                p.bump();
            }
        }
    }
}
