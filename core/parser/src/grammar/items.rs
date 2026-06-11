//! Item (top-level and spec-body) grammar.
//!
//! Covers `use_directive`, `spec_definition`, `function_definition`,
//! `external_function_definition`, `struct_definition`, `struct_field`,
//! `enum_definition`, `constant_definition`, `type_definition_statement`, and the
//! hidden `_definition` dispatch.

use crate::grammar::expr;
use crate::grammar::params;
use crate::grammar::stmt;
use crate::grammar::types;
use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;

/// Dispatches a `_definition` (`_definition`). Hidden rule: it peeks
/// past an optional `pub` to the definition keyword and routes accordingly.
///
/// `external` definitions have no visibility, so a `pub` before `external` is a
/// grammar error; we still consume it as a `Visibility` node for resilience and
/// let the keyword routing carry on.
pub(crate) fn definition(p: &mut Parser) {
    let keyword = if p.at(SyntaxKind::PubKw) {
        p.nth(1)
    } else {
        p.current()
    };
    match keyword {
        SyntaxKind::FnKw => function_definition(p),
        SyntaxKind::ExternalKw => external_function_definition(p),
        SyntaxKind::ConstKw => constant_definition(p),
        SyntaxKind::TypeKw => type_definition_statement(p),
        SyntaxKind::EnumKw => enum_definition(p),
        SyntaxKind::StructKw => struct_definition(p),
        _ => {
            p.err_and_bump("expected a definition");
        }
    }
}

/// Consumes an optional `pub` as a `Visibility` node (`visibility`).
fn visibility(p: &mut Parser) {
    if p.at(SyntaxKind::PubKw) {
        let m = p.start();
        p.bump(SyntaxKind::PubKw);
        m.complete(p, SyntaxKind::Visibility);
    }
}

/// `use ( path [ :: { types } ] | { types } from module_ref ) ;`
/// (`use_directive`). The two forms are distinguished by whether the body starts
/// with `{`. In the `from` form, `module_ref` is a logical identifier path
/// (`name` or `a::b`) — not a filesystem string — so source stays portable.
pub(crate) fn use_directive(p: &mut Parser) {
    let m = p.start();
    p.bump(SyntaxKind::UseKw);
    if p.at(SyntaxKind::LBrace) {
        imported_type_list(p);
        p.expect(SyntaxKind::FromKw);
        if p.at(SyntaxKind::Ident) {
            module_ref(p);
        } else {
            p.error("expected a module name");
        }
    } else {
        types::identifier(p);
        while p.at(SyntaxKind::ColonColon) && !p.nth_at(1, SyntaxKind::LBrace) {
            p.bump(SyntaxKind::ColonColon);
            types::identifier(p);
        }
        if p.at(SyntaxKind::ColonColon) {
            p.bump(SyntaxKind::ColonColon);
            imported_type_list(p);
        }
    }
    p.expect(SyntaxKind::Semi);
    m.complete(p, SyntaxKind::UseDirective);
}

/// `ident ( :: ident )*` — the logical module reference of a `from` clause.
/// Emits one `Identifier` per path segment; segments are separated by `::`.
fn module_ref(p: &mut Parser) {
    types::identifier(p);
    while p.at(SyntaxKind::ColonColon) {
        p.bump(SyntaxKind::ColonColon);
        if p.at(SyntaxKind::Ident) {
            types::identifier(p);
        } else {
            p.error("expected a module path segment");
            break;
        }
    }
}

/// `{ sep1(ident, ,) }` — the imported-type list shared by both use forms.
fn imported_type_list(p: &mut Parser) {
    p.expect(SyntaxKind::LBrace);
    if !p.at(SyntaxKind::RBrace) {
        types::identifier(p);
        while p.eat(SyntaxKind::Comma) {
            if p.at(SyntaxKind::RBrace) {
                break;
            }
            types::identifier(p);
        }
    }
    p.expect(SyntaxKind::RBrace);
}

/// `spec ident { _definition* }` (`spec_definition`).
pub(crate) fn spec_definition(p: &mut Parser) {
    let m = p.start();
    p.bump(SyntaxKind::SpecKw);
    types::identifier(p);
    p.expect(SyntaxKind::LBrace);
    while !p.at(SyntaxKind::RBrace) && !p.at_eof() {
        if at_definition_start(p) {
            // Defense-in-depth: a `definition` handler that consumes nothing
            // (e.g. a future non-advancing routing) would spin this loop, since
            // completing a marker refills the fuel guard. Detect the unchanged
            // cursor and bump the offending token into an Error node.
            let before = p.pos();
            definition(p);
            if p.pos() == before {
                p.err_and_bump("expected a definition");
            }
        } else {
            // The `}` terminating this body is the recovery anchor, so consume
            // the offending token into an Error node to guarantee progress —
            // never leaving it via a recovery set, which would spin the loop.
            p.err_and_bump("expected a definition");
        }
    }
    p.expect(SyntaxKind::RBrace);
    m.complete(p, SyntaxKind::SpecDefinition);
}

/// Whether the current token can begin a `_definition`.
fn at_definition_start(p: &Parser) -> bool {
    matches!(
        p.current(),
        SyntaxKind::PubKw
            | SyntaxKind::FnKw
            | SyntaxKind::ExternalKw
            | SyntaxKind::ConstKw
            | SyntaxKind::TypeKw
            | SyntaxKind::EnumKw
            | SyntaxKind::StructKw
    )
}

/// `[pub] fn ident [type_params] argument_list [ -> _type ] _block`
/// (`function_definition`).
pub(crate) fn function_definition(p: &mut Parser) {
    let m = p.start();
    visibility(p);
    p.expect(SyntaxKind::FnKw);
    types::identifier(p);
    if params::at_type_argument_list_definition(p) {
        params::type_argument_list_definition(p);
    }
    params::argument_list(p);
    if p.eat(SyntaxKind::Arrow) {
        types::type_(p);
    }
    if stmt::at_block_start(p) {
        stmt::block_or_nondet(p);
    } else {
        p.error("expected a function body");
    }
    m.complete(p, SyntaxKind::FunctionDefinition);
}

/// `external fn ident argument_list [ -> _type ] ;`
/// (`external_function_definition`). No visibility is allowed: a stray leading
/// `pub` is a grammar error, so we report it and then consume it as a
/// `Visibility` node for resilience — mirroring every other definition handler —
/// so the cursor always advances past it (otherwise the `source_file` item loop
/// would spin on `pub external …`).
pub(crate) fn external_function_definition(p: &mut Parser) {
    let m = p.start();
    if p.at(SyntaxKind::PubKw) {
        p.error("`external` functions cannot be `pub`");
        visibility(p);
    }
    p.expect(SyntaxKind::ExternalKw);
    p.expect(SyntaxKind::FnKw);
    types::identifier(p);
    params::argument_list(p);
    if p.eat(SyntaxKind::Arrow) {
        types::type_(p);
    }
    p.expect(SyntaxKind::Semi);
    m.complete(p, SyntaxKind::ExternalFunctionDefinition);
}

/// `[pub] struct ident { ( struct_field ; | function_definition )* }`
/// (`struct_definition`).
pub(crate) fn struct_definition(p: &mut Parser) {
    let m = p.start();
    visibility(p);
    p.expect(SyntaxKind::StructKw);
    types::identifier(p);
    p.expect(SyntaxKind::LBrace);
    while !p.at(SyntaxKind::RBrace) && !p.at_eof() {
        // Defense-in-depth: capture the cursor so a member handler that consumes
        // nothing degrades to a recoverable error instead of spinning the loop
        // (completing a marker refills the fuel guard, so it cannot catch this).
        let before = p.pos();
        match p.current() {
            SyntaxKind::Ident => {
                struct_field(p);
                p.expect(SyntaxKind::Semi);
            }
            SyntaxKind::FnKw | SyntaxKind::PubKw => function_definition(p),
            // The `}` terminating the struct body is the recovery anchor, so
            // consume the offending token to guarantee progress rather than
            // leaving it via a recovery set (which could spin the loop).
            _ => p.err_and_bump("expected a struct field or method"),
        }
        if p.pos() == before {
            p.err_and_bump("expected a struct field or method");
        }
    }
    p.expect(SyntaxKind::RBrace);
    m.complete(p, SyntaxKind::StructDefinition);
}

/// `ident : _type` (`struct_field`).
fn struct_field(p: &mut Parser) {
    let m = p.start();
    types::identifier(p);
    p.expect(SyntaxKind::Colon);
    types::type_(p);
    m.complete(p, SyntaxKind::StructField);
}

/// `[pub] enum ident { sep1(ident, ,) }` (`enum_definition`).
pub(crate) fn enum_definition(p: &mut Parser) {
    let m = p.start();
    visibility(p);
    p.expect(SyntaxKind::EnumKw);
    types::identifier(p);
    p.expect(SyntaxKind::LBrace);
    if p.at(SyntaxKind::Ident) {
        types::identifier(p);
        while p.eat(SyntaxKind::Comma) {
            if p.at(SyntaxKind::RBrace) {
                break;
            }
            types::identifier(p);
        }
    }
    p.expect(SyntaxKind::RBrace);
    m.complete(p, SyntaxKind::EnumDefinition);
}

/// `[pub] const ident : _type = _expression ;`
/// (`constant_definition`). Also reachable as a statement inside a block.
pub(crate) fn constant_definition(p: &mut Parser) {
    let m = p.start();
    visibility(p);
    p.expect(SyntaxKind::ConstKw);
    types::identifier(p);
    p.expect(SyntaxKind::Colon);
    types::type_(p);
    p.expect(SyntaxKind::Eq);
    expr::expr(p);
    p.expect(SyntaxKind::Semi);
    m.complete(p, SyntaxKind::ConstantDefinition);
}

/// `[pub] type ident = _type ;` (`type_definition_statement`). Also
/// reachable as a statement inside a block.
pub(crate) fn type_definition_statement(p: &mut Parser) {
    let m = p.start();
    visibility(p);
    p.expect(SyntaxKind::TypeKw);
    types::identifier(p);
    p.expect(SyntaxKind::Eq);
    types::type_(p);
    p.expect(SyntaxKind::Semi);
    m.complete(p, SyntaxKind::TypeDefinitionStatement);
}
