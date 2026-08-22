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

/// The diagnostic for a `mut` before a bare type or an ignored parameter —
/// neither names a binding whose mutability the keyword could describe.
const MUT_WITHOUT_BINDING_MESSAGE: &str =
    "`mut` applies to a named parameter or to `self`; write `mut name: type`, or drop the `mut`";

/// The diagnostic for a `mut` before an identifier that no `:` follows. Naming
/// the missing type is the point: the parameter *is* named, so the general
/// message would deny what the source plainly says. The identifier cannot be
/// told apart from a bare custom type here, so the message serves both readings.
const MUT_PARAMETER_MISSING_TYPE_MESSAGE: &str = "a `mut` parameter needs a type; write `mut name: type`, or drop the `mut` if this is a \
     bare type";

/// Whether a `mut` at the cursor qualifies a binding, i.e. whether it precedes
/// one of the two argument forms that carry a mutability flag.
fn mut_qualifies_a_binding(p: &Parser) -> bool {
    p.nth_at(1, SyntaxKind::SelfKw)
        || (p.nth_at(1, SyntaxKind::Ident) && p.nth_at(2, SyntaxKind::Colon))
}

/// Which diagnostic a non-qualifying `mut` at the cursor earns. Reached only
/// when [`mut_qualifies_a_binding`] is false, so an identifier here is never
/// followed by a `:`.
fn stray_mut_message(p: &Parser) -> &'static str {
    if p.nth_at(1, SyntaxKind::Ident) {
        MUT_PARAMETER_MISSING_TYPE_MESSAGE
    } else {
        MUT_WITHOUT_BINDING_MESSAGE
    }
}

/// A single argument (`argument_list` choice arm): a declaration,
/// a self reference, an ignore argument, or a bare type.
///
/// Disambiguation by lookahead:
/// - `self` / `mut self` → `self_reference`
/// - `mut ident :` → `argument_declaration`
/// - `_ :` → `ignore_argument`
/// - `ident :` → `argument_declaration`
/// - otherwise → a bare `_type` (the `TypeOnly` arm)
///
/// A `mut` qualifying none of the binding forms is reported and dropped before
/// the dispatch, so the rest of the argument is parsed as the form the source
/// otherwise wrote. Dropping the keyword as a bare token, rather than wrapping
/// it in an `Error` node, keeps it out of the argument list's node children —
/// which lowering reads as exactly one argument each — and keeps the lowered
/// argument the one the source wrote instead of a named argument carrying an
/// error name and an `is_mut` no source spelled.
fn argument(p: &mut Parser) {
    // A run of stray keywords is drained by a loop rather than by recursing
    // through `argument`, so a pathological input keeps the stack flat.
    while p.at(SyntaxKind::MutKw) && !mut_qualifies_a_binding(p) {
        p.error(stray_mut_message(p));
        p.bump(SyntaxKind::MutKw);
    }
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
