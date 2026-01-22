/// Parsing of patterns

use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;

/// Parse a pattern (used in match, let, function params, etc.)
pub fn parse_pattern(p: &mut Parser) {
    match p.current() {
        SyntaxKind::IDENT => {
            p.bump();
            // Could be a path pattern or binding
            if p.at(SyntaxKind::COLON_COLON) {
                while p.at(SyntaxKind::COLON_COLON) && !p.at_eof() {
                    p.bump();
                    if p.at(SyntaxKind::IDENT) {
                        p.bump();
                    }
                }
            }
        }
        SyntaxKind::L_PAREN => {
            // Tuple pattern
            p.bump();
            while !p.at(SyntaxKind::R_PAREN) && !p.at_eof() {
                parse_pattern(p);
                if !p.at(SyntaxKind::R_PAREN) {
                    p.expect(SyntaxKind::COMMA);
                }
            }
            p.expect(SyntaxKind::R_PAREN);
        }
        SyntaxKind::L_BRACKET => {
            // Array pattern
            p.bump();
            while !p.at(SyntaxKind::R_BRACKET) && !p.at_eof() {
                parse_pattern(p);
                if !p.at(SyntaxKind::R_BRACKET) {
                    p.expect(SyntaxKind::COMMA);
                }
            }
            p.expect(SyntaxKind::R_BRACKET);
        }
        SyntaxKind::L_BRACE => {
            // Struct pattern
            p.bump();
            while !p.at(SyntaxKind::R_BRACE) && !p.at_eof() {
                if p.at(SyntaxKind::IDENT) {
                    p.bump();
                    if p.at(SyntaxKind::COLON) {
                        p.bump();
                        parse_pattern(p);
                    }
                }
                if !p.at(SyntaxKind::R_BRACE) {
                    p.expect(SyntaxKind::COMMA);
                }
            }
            p.expect(SyntaxKind::R_BRACE);
        }
        SyntaxKind::INTEGER_LITERAL | 
        SyntaxKind::FLOAT_LITERAL |
        SyntaxKind::STRING_LITERAL |
        SyntaxKind::CHAR_LITERAL |
        SyntaxKind::TRUE |
        SyntaxKind::FALSE => {
            // Literal pattern
            p.bump();
        }
        SyntaxKind::UNDERSCORE => {
            // Wildcard pattern
            p.bump();
        }
        SyntaxKind::AND => {
            // Reference pattern
            p.bump();
            if p.at(SyntaxKind::MUT) {
                p.bump();
            }
            parse_pattern(p);
        }
        _ => {
            p.error("expected pattern");
        }
    }
}
