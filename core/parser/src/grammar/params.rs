//! Parameter grammar: argument lists and type-parameter lists.
//!
//! Covers `argument_list`, `argument_declaration`, `self_reference`,
//! `ignore_argument`, the bare-type argument arm, and
//! `type_argument_list_definition`.

use crate::grammar::types;
use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;

/// `( [ sep1(argument, ,) ] )` (`argument_list`). Emits an
/// `ArgumentList` node holding the comma-separated argument nodes.
pub(crate) fn argument_list(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::LParen);
    if !p.at(SyntaxKind::RParen) {
        argument(p);
        while p.eat(SyntaxKind::Comma) {
            if p.at(SyntaxKind::RParen) {
                break;
            }
            argument(p);
        }
    }
    p.expect(SyntaxKind::RParen);
    m.complete(p, SyntaxKind::ArgumentList);
}

/// A single argument (`argument_list` choice arm): a declaration,
/// a self reference, an ignore argument, or a bare type.
///
/// Disambiguation by lookahead:
/// - `self` / `mut self` → `self_reference`
/// - `mut ident` → `argument_declaration`
/// - `_ :` → `ignore_argument`
/// - `ident :` → `argument_declaration`
/// - otherwise → a bare `_type` (the `TypeOnly` arm)
fn argument(p: &mut Parser) {
    match p.current() {
        SyntaxKind::SelfKw => self_reference(p),
        SyntaxKind::MutKw => {
            if p.nth_at(1, SyntaxKind::SelfKw) {
                self_reference(p);
            } else {
                argument_declaration(p);
            }
        }
        SyntaxKind::Underscore if p.nth_at(1, SyntaxKind::Colon) => ignore_argument(p),
        SyntaxKind::Ident if p.nth_at(1, SyntaxKind::Colon) => argument_declaration(p),
        _ => {
            if types::at_type_start(p) {
                types::type_(p);
            } else {
                p.error("expected an argument");
            }
        }
    }
}

/// `[mut] ident : _type` (`argument_declaration`).
fn argument_declaration(p: &mut Parser) {
    let m = p.start();
    mut_keyword(p);
    types::identifier(p);
    p.expect(SyntaxKind::Colon);
    types::type_(p);
    m.complete(p, SyntaxKind::ArgumentDeclaration);
}

/// `[mut] self` (`self_reference`).
fn self_reference(p: &mut Parser) {
    let m = p.start();
    mut_keyword(p);
    p.expect(SyntaxKind::SelfKw);
    m.complete(p, SyntaxKind::SelfReference);
}

/// `_ : _type` (`ignore_argument`).
fn ignore_argument(p: &mut Parser) {
    let m = p.start();
    p.bump(SyntaxKind::Underscore);
    p.expect(SyntaxKind::Colon);
    types::type_(p);
    m.complete(p, SyntaxKind::IgnoreArgument);
}

/// Consumes an optional `mut` keyword as a `MutKeyword` node
/// (`mut_keyword`, an optional field on declarations and self references).
pub(crate) fn mut_keyword(p: &mut Parser) {
    if p.at(SyntaxKind::MutKw) {
        let m = p.start();
        p.bump(SyntaxKind::MutKw);
        m.complete(p, SyntaxKind::MutKeyword);
    }
}

/// `( ident ' )+` (`type_argument_list_definition`). Each parameter
/// is an identifier immediately followed by a glued tick. Emits a
/// `TypeArgumentListDefinition` node.
pub(crate) fn type_argument_list_definition(p: &mut Parser) {
    let m = p.start();
    loop {
        types::identifier(p);
        if p.at(SyntaxKind::Tick) && p.prev_joint() {
            p.bump(SyntaxKind::Tick);
        } else if p.at(SyntaxKind::Tick) {
            p.error("type-parameter tick must follow the identifier with no space");
            p.bump(SyntaxKind::Tick);
        } else {
            p.expect(SyntaxKind::Tick);
            break;
        }
        if !p.at(SyntaxKind::Ident) {
            break;
        }
    }
    m.complete(p, SyntaxKind::TypeArgumentListDefinition);
}

/// Whether the current position begins a `type_argument_list_definition`, i.e.
/// `ident '` with the tick glued to the identifier.
pub(crate) fn at_type_argument_list_definition(p: &Parser) -> bool {
    p.at(SyntaxKind::Ident) && p.nth_at(1, SyntaxKind::Tick)
}
