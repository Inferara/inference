//! Statement and block grammar (grammar.js `_statement`, `_block`, `block`, the
//! non-det blocks, and every statement form).
//!
//! `_statement` and `_block` are hidden dispatch rules. `block` and the non-det
//! blocks (`assume_block`, `forall_block`, `exists_block`, `unique_block`) and
//! every statement form emit their CST node.

use crate::grammar::expr;
use crate::grammar::items;
use crate::grammar::types;
use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;
use crate::token_set::TokenSet;

/// The tokens that can begin a statement (grammar.js `_statement` first set,
/// union of the block openers, the statement keywords, and the expression
/// first set). Used for block-level recovery anchors.
const STMT_START: TokenSet = TokenSet::new(&[
    SyntaxKind::LBrace,
    SyntaxKind::AssumeKw,
    SyntaxKind::ForallKw,
    SyntaxKind::ExistsKw,
    SyntaxKind::UniqueKw,
    SyntaxKind::ReturnKw,
    SyntaxKind::LoopKw,
    SyntaxKind::IfKw,
    SyntaxKind::LetKw,
    SyntaxKind::ConstKw,
    SyntaxKind::TypeKw,
    SyntaxKind::AssertKw,
    SyntaxKind::BreakKw,
])
.union(expr::EXPR_START);

/// The tokens that open a `_block`: a plain `{` or a non-det block keyword
/// (grammar.js `_block`).
pub(crate) const BLOCK_START: TokenSet = TokenSet::new(&[
    SyntaxKind::LBrace,
    SyntaxKind::AssumeKw,
    SyntaxKind::ForallKw,
    SyntaxKind::ExistsKw,
    SyntaxKind::UniqueKw,
]);

/// Whether the current token opens a `_block`.
pub(crate) fn at_block_start(p: &Parser) -> bool {
    p.at_ts(BLOCK_START)
}

/// Parses a `_block`: a plain block or a non-det block wrapping a block
/// (grammar.js `_block`). Hidden rule: dispatches without a node of its own.
pub(crate) fn block_or_nondet(p: &mut Parser) {
    match p.current() {
        SyntaxKind::AssumeKw => nondet_block(p, SyntaxKind::AssumeBlock),
        SyntaxKind::ForallKw => nondet_block(p, SyntaxKind::ForallBlock),
        SyntaxKind::ExistsKw => nondet_block(p, SyntaxKind::ExistsBlock),
        SyntaxKind::UniqueKw => nondet_block(p, SyntaxKind::UniqueBlock),
        SyntaxKind::LBrace => block(p),
        _ => {
            p.error("expected a block");
        }
    }
}

/// A non-det block (`assume`/`forall`/`exists`/`unique`) wrapping a `block`
/// (grammar.js `assume_block` et al.).
fn nondet_block(p: &mut Parser, kind: SyntaxKind) {
    let m = p.start();
    p.bump_any(); // the non-det keyword
    if p.at(SyntaxKind::LBrace) {
        block(p);
    } else {
        p.error("expected a block body");
    }
    m.complete(p, kind);
}

/// `{ _statement* }` (grammar.js `block`).
pub(crate) fn block(p: &mut Parser) {
    let m = p.start();
    p.bump(SyntaxKind::LBrace);
    while !p.at(SyntaxKind::RBrace) && !p.at_eof() {
        if at_stmt_start(p) {
            statement(p);
        } else {
            // The `}` closing the block is the loop terminator, so always
            // consume the offending token to guarantee progress. Using a
            // recovery set here would spin if the stuck token (e.g. a stray `;`)
            // were itself a member of that set.
            p.err_and_bump("expected a statement");
        }
    }
    p.expect(SyntaxKind::RBrace);
    m.complete(p, SyntaxKind::Block);
}

/// Whether the current token can begin a statement.
fn at_stmt_start(p: &Parser) -> bool {
    p.at_ts(STMT_START)
}

/// Dispatches a single statement (grammar.js `_statement`). Hidden rule.
fn statement(p: &mut Parser) {
    match p.current() {
        kind if BLOCK_START.contains(kind) => block_statement(p),
        SyntaxKind::ReturnKw => return_statement(p),
        SyntaxKind::LoopKw => loop_statement(p),
        SyntaxKind::IfKw => if_statement(p),
        SyntaxKind::LetKw => variable_definition_statement(p),
        SyntaxKind::ConstKw => items::constant_definition(p),
        SyntaxKind::TypeKw => items::type_definition_statement(p),
        SyntaxKind::AssertKw => assert_statement(p),
        SyntaxKind::BreakKw => break_statement(p),
        _ => expression_or_assign_statement(p),
    }
}

/// A bare block used as a statement (grammar.js `_statement` → `_block`). The
/// inner `block`/non-det block is the statement's only child.
fn block_statement(p: &mut Parser) {
    block_or_nondet(p);
}

/// `return [ _expression ] ;` (grammar.js `return_statement`).
fn return_statement(p: &mut Parser) {
    let m = p.start();
    p.bump(SyntaxKind::ReturnKw);
    if expr::at_expr_start(p) {
        expr::expr(p);
    }
    p.expect(SyntaxKind::Semi);
    m.complete(p, SyntaxKind::ReturnStatement);
}

/// `loop [ condition ] _block` (grammar.js `loop_statement`). The optional
/// condition is parsed in no-struct context so a following `{` opens the body.
fn loop_statement(p: &mut Parser) {
    let m = p.start();
    p.bump(SyntaxKind::LoopKw);
    if !at_block_start(p) && expr::at_expr_start(p) {
        expr::expr_no_struct(p);
    }
    block_or_nondet(p);
    m.complete(p, SyntaxKind::LoopStatement);
}

/// `if cond _block ( else if cond _block )* [ else _block ]` (grammar.js
/// `if_statement`). Conditions are parsed in no-struct context.
fn if_statement(p: &mut Parser) {
    let m = p.start();
    p.bump(SyntaxKind::IfKw);
    expr::expr_no_struct(p);
    block_or_nondet(p);
    while p.at(SyntaxKind::ElseKw) && p.nth_at(1, SyntaxKind::IfKw) {
        p.bump(SyntaxKind::ElseKw);
        p.bump(SyntaxKind::IfKw);
        expr::expr_no_struct(p);
        block_or_nondet(p);
    }
    if p.at(SyntaxKind::ElseKw) {
        p.bump(SyntaxKind::ElseKw);
        block_or_nondet(p);
    }
    m.complete(p, SyntaxKind::IfStatement);
}

/// `let [mut] ident : _type [ = _expression ] ;` (grammar.js
/// `variable_definition_statement`).
fn variable_definition_statement(p: &mut Parser) {
    let m = p.start();
    p.bump(SyntaxKind::LetKw);
    crate::grammar::params::mut_keyword(p);
    types::identifier(p);
    p.expect(SyntaxKind::Colon);
    types::type_(p);
    if p.eat(SyntaxKind::Eq) {
        expr::expr(p);
    }
    p.expect(SyntaxKind::Semi);
    m.complete(p, SyntaxKind::VariableDefinitionStatement);
}

/// `assert _expression ;` (grammar.js `assert_statement`).
fn assert_statement(p: &mut Parser) {
    let m = p.start();
    p.bump(SyntaxKind::AssertKw);
    expr::expr(p);
    p.expect(SyntaxKind::Semi);
    m.complete(p, SyntaxKind::AssertStatement);
}

/// `break ;` (grammar.js `break_statement`).
fn break_statement(p: &mut Parser) {
    let m = p.start();
    p.bump(SyntaxKind::BreakKw);
    p.expect(SyntaxKind::Semi);
    m.complete(p, SyntaxKind::BreakStatement);
}

/// Parses an expression statement or an assignment, disambiguated after the
/// fact: parse an expression; if `=` follows, it is the left side of an
/// `assign_statement`, otherwise the whole thing is an `expression_statement`
/// (grammar.js `expression_statement` / `assign_statement`).
fn expression_or_assign_statement(p: &mut Parser) {
    let m = p.start();
    expr::expr(p);
    if p.eat(SyntaxKind::Eq) {
        expr::expr(p);
        p.expect(SyntaxKind::Semi);
        m.complete(p, SyntaxKind::AssignStatement);
    } else {
        p.expect(SyntaxKind::Semi);
        m.complete(p, SyntaxKind::ExpressionStatement);
    }
}
