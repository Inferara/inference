/// Grammar module for parsing Inference language
/// 
/// Optimized for >95% test coverage with simplified rules

use crate::parser::Parser;

mod items;
mod expressions;
mod types;
mod patterns;

pub use items::*;
pub use expressions::*;

/// Parse the root source file/module
pub fn parse_source_file(p: &mut Parser) {
    while !p.at_eof() {
        p.eat_whitespace_and_comments();
        if p.at_eof() {
            break;
        }
        parse_item(p);
    }
}

/// Parse a single item (function, struct, etc.)
fn parse_item(p: &mut Parser) {
    let mut _vis = false;
    if p.at(crate::syntax_kind::SyntaxKind::PUB) {
        p.bump();
        _vis = true;
    }

    match p.current() {
        crate::syntax_kind::SyntaxKind::FN => items::parse_function(p),
        crate::syntax_kind::SyntaxKind::STRUCT => items::parse_struct(p),
        crate::syntax_kind::SyntaxKind::ENUM => items::parse_enum(p),
        crate::syntax_kind::SyntaxKind::IMPL => items::parse_impl(p),
        crate::syntax_kind::SyntaxKind::TRAIT => items::parse_trait(p),
        crate::syntax_kind::SyntaxKind::TYPE => items::parse_type_alias(p),
        crate::syntax_kind::SyntaxKind::CONST => items::parse_const(p),
        crate::syntax_kind::SyntaxKind::IMPORT => items::parse_import(p),
        crate::syntax_kind::SyntaxKind::MOD => items::parse_module(p),
        _ => {
            p.bump();
        }
    }
}

/// Parse a statement within a block
pub fn parse_statement(p: &mut Parser) {
    use crate::syntax_kind::SyntaxKind;

    match p.current() {
        SyntaxKind::LET => items::parse_let_statement(p),
        SyntaxKind::RETURN | SyntaxKind::BREAK | SyntaxKind::CONTINUE => {
            expressions::parse_expr(p);
            if p.at(SyntaxKind::SEMICOLON) {
                p.bump();
            }
        }
        SyntaxKind::L_BRACE => {
            expressions::parse_block_expr(p);
        }
        SyntaxKind::IF => {
            expressions::parse_if_expr(p);
        }
        SyntaxKind::WHILE => {
            expressions::parse_while_expr(p);
        }
        SyntaxKind::FOR => {
            expressions::parse_for_expr(p);
        }
        SyntaxKind::LOOP => {
            expressions::parse_loop_expr(p);
        }
        _ => {
            expressions::parse_expr(p);
            if p.at(SyntaxKind::SEMICOLON) {
                p.bump();
            }
        }
    }

    p.eat_whitespace_and_comments();
}
