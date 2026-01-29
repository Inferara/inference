/// Grammar module - Parsing rules for Inference language constructs
/// 
/// This module provides the grammar parsing functions called by parse_module().
/// Each function parses a specific construct and advances the parser position.

use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;

pub mod items;
pub mod expressions;
pub mod types;

pub use items::*;
pub use expressions::*;
pub use types::*;

/// Parse a top-level item (function, struct, enum, etc.)
pub fn parse_item(p: &mut Parser) {
    // Check for pub visibility modifier
    if p.at(SyntaxKind::PUB) {
        p.bump();
    }

    match p.current() {
        SyntaxKind::FN => items::parse_function(p),
        SyntaxKind::STRUCT => items::parse_struct(p),
        SyntaxKind::ENUM => items::parse_enum(p),
        SyntaxKind::TRAIT => items::parse_trait(p),
        SyntaxKind::IMPL => items::parse_impl(p),
        SyntaxKind::TYPE => items::parse_type_alias(p),
        SyntaxKind::CONST => items::parse_const(p),
        SyntaxKind::IMPORT => items::parse_import(p),
        SyntaxKind::MOD => items::parse_module(p),
        SyntaxKind::LET => items::parse_let_binding(p),
        _ => {
            // Unknown item - skip it
            if !p.at_eof() {
                p.bump();
            }
        }
    }
}

/// Parse a statement inside a block
pub fn parse_statement(p: &mut Parser) {
    match p.current() {
        SyntaxKind::LET => items::parse_let_binding(p),
        SyntaxKind::IF => expressions::parse_if_expr(p),
        SyntaxKind::WHILE => expressions::parse_while_expr(p),
        SyntaxKind::FOR => expressions::parse_for_expr(p),
        SyntaxKind::LOOP => expressions::parse_loop_expr(p),
        SyntaxKind::RETURN => expressions::parse_return_expr(p),
        SyntaxKind::BREAK => {
            p.bump();
        }
        SyntaxKind::CONTINUE => {
            p.bump();
        }
        _ => {
            // Try to parse as expression
            expressions::parse_expression(p);
            if p.at(SyntaxKind::SEMICOLON) {
                p.bump();
            }
        }
    }
}
