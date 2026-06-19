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

/// The diagnostic for a glob use (`use a::b::*;`). Glob imports are rejected so a
/// file's public surface stays explicit; the message points at the two supported
/// alternatives.
const GLOB_IMPORT_MESSAGE: &str =
    "glob imports are not supported; import the file (use a::b;) or list items \
     explicitly (use a::b::{x, y};)";

/// `[pub] use ( path [ :: { types } ] | { types } from module_ref ) ;`
/// (`use_directive`). An optional leading `pub` re-exports the import. The two
/// forms are distinguished by whether the body starts with `{`. In the `from`
/// form, `module_ref` is a logical identifier path (`name` or `a::b`) — not a
/// filesystem string — so source stays portable.
pub(crate) fn use_directive(p: &mut Parser) {
    let m = p.start();
    visibility(p);
    p.expect(SyntaxKind::UseKw);
    if p.at(SyntaxKind::LBrace) {
        imported_type_list(p);
        p.expect(SyntaxKind::FromKw);
        if p.at(SyntaxKind::Ident) {
            module_ref(p);
        } else {
            p.error("expected a module name");
        }
    } else if p.at(SyntaxKind::Star) {
        // A leading `*` (`use *;`) is a glob with no path: reject and recover.
        p.error(GLOB_IMPORT_MESSAGE);
        recover_to_semicolon(p);
        m.complete(p, SyntaxKind::UseDirective);
        return;
    } else {
        types::identifier(p);
        while p.at(SyntaxKind::ColonColon) && !p.nth_at(1, SyntaxKind::LBrace) {
            p.bump(SyntaxKind::ColonColon);
            if p.at(SyntaxKind::Star) {
                // `use a::b::*;` — reject the glob and skip to the terminating
                // `;` so the following item still parses.
                p.error(GLOB_IMPORT_MESSAGE);
                recover_to_semicolon(p);
                m.complete(p, SyntaxKind::UseDirective);
                return;
            }
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

/// Consumes tokens up to and including the next `;`, wrapping the skipped run in
/// an `Error` node, so a malformed directive resynchronises on the next item.
/// Stops early at EOF or an item-recovery anchor so it never eats a following
/// definition.
fn recover_to_semicolon(p: &mut Parser) {
    if p.at(SyntaxKind::Semi) || p.at_eof() || p.at_ts(crate::grammar::ITEM_RECOVERY) {
        p.eat(SyntaxKind::Semi);
        return;
    }
    let err = p.start();
    while !p.at(SyntaxKind::Semi) && !p.at_eof() && !p.at_ts(crate::grammar::ITEM_RECOVERY) {
        p.bump_any();
    }
    err.complete(p, SyntaxKind::Error);
    p.eat(SyntaxKind::Semi);
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

/// `spec ident { _definition* }` (`spec_definition`). Specs take no visibility
/// modifier, so a stray leading `pub` is reported and then consumed as a
/// `Visibility` node for resilience; the spec body still parses.
pub(crate) fn spec_definition(p: &mut Parser) {
    let m = p.start();
    if p.at(SyntaxKind::PubKw) {
        p.error("specs take no visibility modifier; they are stripped before codegen");
        visibility(p);
    }
    p.expect(SyntaxKind::SpecKw);
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
            SyntaxKind::FnKw => function_definition(p),
            // `pub fn …` is a method; `pub field : T;` is a field with a stray
            // visibility modifier — fields have no individual visibility, so the
            // field handler reports it and recovers.
            SyntaxKind::PubKw if p.nth_at(1, SyntaxKind::FnKw) => function_definition(p),
            SyntaxKind::PubKw => {
                struct_field(p);
                p.expect(SyntaxKind::Semi);
            }
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

/// `ident : _type` (`struct_field`). A field has no individual visibility: it is
/// accessible iff its struct is. A stray leading `pub` is reported and consumed
/// as a `Visibility` node, then the field parses normally so it still lands in
/// the tree.
fn struct_field(p: &mut Parser) {
    let m = p.start();
    if p.at(SyntaxKind::PubKw) {
        p.error("fields inherit visibility from their struct");
        visibility(p);
    }
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
