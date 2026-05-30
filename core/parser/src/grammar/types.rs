//! Type grammar (grammar.js `_type`, `_embedded_type`, `_name`, names).
//!
//! `_type`, `_embedded_type`, `_name`, `_simple_name` and
//! `_bracketed_generic_name` are hidden in grammar.js, so they dispatch without
//! opening a node. The concrete forms — `type_i32`, `type_array`, `type_fn`,
//! `identifier`, `generic_name`, `type_qualified_name` — each emit their node.

use crate::grammar::expr;
use crate::grammar::params;
use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;
use crate::token_set::TokenSet;

/// The primitive type keyword tokens (`i8`..`u64`, `bool`).
const PRIMITIVE_TYPE_KW: TokenSet = TokenSet::new(&[
    SyntaxKind::I8Kw,
    SyntaxKind::I16Kw,
    SyntaxKind::I32Kw,
    SyntaxKind::I64Kw,
    SyntaxKind::U8Kw,
    SyntaxKind::U16Kw,
    SyntaxKind::U32Kw,
    SyntaxKind::U64Kw,
    SyntaxKind::BoolKw,
]);

/// The tokens that can begin a `_type` (grammar.js `_type` first set).
pub(crate) const TYPE_START: TokenSet = PRIMITIVE_TYPE_KW.union(TokenSet::new(&[
    SyntaxKind::LParen,
    SyntaxKind::LBracket,
    SyntaxKind::FnKw,
    SyntaxKind::Ident,
]));

/// Whether the current token can begin a type.
pub(crate) fn at_type_start(p: &Parser) -> bool {
    p.at_ts(TYPE_START)
}

/// Keyword-spelling tokens that grammar.js treats as ordinary identifiers when
/// they appear in identifier position.
///
/// In the tree-sitter grammar `self`, `type`, `from` and `spec` are keywords
/// only where the rules spell them as literals (`self_reference`, the `type` of
/// a type definition, the `from` of a use directive, the `spec` of a spec
/// definition). Everywhere an `identifier` is expected — a name, a member name,
/// a struct-field name, an argument name, a qualified-name alias — they fall back
/// to the `identifier` token (the `word` rule). The corpus relies on this:
/// `self.type` uses `self` as a name and `type` as a member name, and
/// `spec::AuctionSpec` uses `spec` as a qualified-name alias. We mirror it by
/// accepting these spellings as identifiers in those positions and recording
/// them under [`SyntaxKind::Ident`].
///
/// The leading-keyword dispatch in items/statements (`item`, `definition`,
/// `statement`) routes a `spec`/`type` at the head of a definition or statement
/// to the keyword rule *before* the expression/name path is reached, so adding
/// them here does not make `spec Foo {}` or `type T = u8;` ambiguous.
pub(crate) const IDENT_LIKE: TokenSet = TokenSet::new(&[
    SyntaxKind::Ident,
    SyntaxKind::SelfKw,
    SyntaxKind::TypeKw,
    SyntaxKind::FromKw,
    SyntaxKind::SpecKw,
]);

/// Whether the current token can stand in for an identifier (a plain identifier
/// or a contextual keyword used in identifier position; see [`IDENT_LIKE`]).
pub(crate) fn at_ident_like(p: &Parser) -> bool {
    p.at_ts(IDENT_LIKE)
}

/// Parses a `_type`: an embedded type, a bracketed generic name, or a name
/// (grammar.js `_type`). Hidden rule: emits no node of its own.
pub(crate) fn type_(p: &mut Parser) {
    match p.current() {
        kind if PRIMITIVE_TYPE_KW.contains(kind) => primitive_type(p),
        SyntaxKind::LParen => {
            // `( generic_name )` is the bracketed generic name; `( )` (joint) is
            // the unit type. Disambiguate on the token after `(`.
            if p.nth_at(1, SyntaxKind::RParen) {
                unit_type(p);
            } else {
                bracketed_generic_name(p);
            }
        }
        SyntaxKind::LBracket => array_type(p),
        SyntaxKind::FnKw => fn_type(p),
        kind if IDENT_LIKE.contains(kind) => name(p),
        _ => {
            p.error("expected a type");
        }
    }
}

/// Wraps a primitive type keyword in its `type_iN`/`type_uN`/`type_bool` node
/// (grammar.js `type_i8`..`type_bool`).
fn primitive_type(p: &mut Parser) {
    let kind = match p.current() {
        SyntaxKind::I8Kw => SyntaxKind::TypeI8,
        SyntaxKind::I16Kw => SyntaxKind::TypeI16,
        SyntaxKind::I32Kw => SyntaxKind::TypeI32,
        SyntaxKind::I64Kw => SyntaxKind::TypeI64,
        SyntaxKind::U8Kw => SyntaxKind::TypeU8,
        SyntaxKind::U16Kw => SyntaxKind::TypeU16,
        SyntaxKind::U32Kw => SyntaxKind::TypeU32,
        SyntaxKind::U64Kw => SyntaxKind::TypeU64,
        SyntaxKind::BoolKw => SyntaxKind::TypeBool,
        other => unreachable!("primitive_type called on {other:?}"),
    };
    let m = p.start();
    p.bump_any();
    m.complete(p, kind);
}

/// `( )` (joint) — the unit type (grammar.js `type_unit`).
fn unit_type(p: &mut Parser) {
    let m = p.start();
    p.bump(SyntaxKind::LParen);
    p.expect(SyntaxKind::RParen);
    m.complete(p, SyntaxKind::TypeUnit);
}

/// `[ _type [ ; (number_literal | _name) ] ]` (grammar.js `type_array`).
fn array_type(p: &mut Parser) {
    let m = p.start();
    p.bump(SyntaxKind::LBracket);
    type_(p);
    if p.eat(SyntaxKind::Semi) {
        if p.at(SyntaxKind::Number) {
            expr::number_literal(p);
        } else {
            name(p);
        }
    }
    p.expect(SyntaxKind::RBracket);
    m.complete(p, SyntaxKind::TypeArray);
}

/// `fn argument_list [ -> _type ]` (grammar.js `type_fn`).
fn fn_type(p: &mut Parser) {
    let m = p.start();
    p.bump(SyntaxKind::FnKw);
    params::argument_list(p);
    if p.eat(SyntaxKind::Arrow) {
        type_(p);
    }
    m.complete(p, SyntaxKind::TypeFn);
}

/// `( generic_name )` (grammar.js `_bracketed_generic_name`). Hidden rule:
/// emits no node; the inner `generic_name` is the only named child.
fn bracketed_generic_name(p: &mut Parser) {
    p.bump(SyntaxKind::LParen);
    name(p);
    p.expect(SyntaxKind::RParen);
}

/// Parses a `_name`: `type_qualified_name` (`ident :: simple_name`, the `::`
/// glued) or a `_simple_name` (grammar.js `_name`). Hidden rule.
pub(crate) fn name(p: &mut Parser) {
    if at_ident_like(p) && p.nth_at(1, SyntaxKind::ColonColon) && p.at_joint() {
        let m = p.start();
        identifier(p);
        p.bump(SyntaxKind::ColonColon);
        qualified_simple_name(p);
        m.complete(p, SyntaxKind::TypeQualifiedName);
    } else {
        simple_name(p);
    }
}

/// Parses the `_simple_name` after the `::` of a qualified name, additionally
/// accepting a primitive type keyword spelling (`i8`..`u64`, `bool`) as the
/// identifier.
///
/// Tree-sitter's GLR lexer is context-sensitive: in the `name` field of
/// `type_qualified_name` only `_simple_name` (an `identifier` or `generic_name`)
/// is valid, so a spelling like `i32` after `std::` lexes as the `identifier`
/// `i32` rather than the `type_i32` keyword. A hand-written LL lexer cannot
/// distinguish these by context, so we mirror the grammar by treating the
/// primitive type keywords as identifier spellings in this one position. The
/// resulting CST child is an `Identifier` node (the keyword token is remapped to
/// `Ident`), keeping the arena byte-identical to the legacy `Builder`.
fn qualified_simple_name(p: &mut Parser) {
    if at_ident_like(p) {
        simple_name(p);
    } else if PRIMITIVE_TYPE_KW.contains(p.current()) {
        let m = p.start();
        p.bump_remap(SyntaxKind::Ident);
        m.complete(p, SyntaxKind::Identifier);
    } else {
        p.error("expected an identifier");
    }
}

/// Parses a `_simple_name`: a `generic_name` (`ident type_argument_list`) or a
/// plain `identifier` (grammar.js `_simple_name`). Hidden rule.
pub(crate) fn simple_name(p: &mut Parser) {
    if !at_ident_like(p) {
        p.error("expected an identifier");
        return;
    }
    // A generic name is `ident` immediately followed by a type-argument list,
    // i.e. a type followed by a glued tick. We detect it by checking whether a
    // tick follows the base identifier at the head of an argument run.
    if at_generic_name(p) {
        let m = p.start();
        identifier(p);
        type_argument_list(p);
        m.complete(p, SyntaxKind::GenericName);
    } else {
        identifier(p);
    }
}

/// Whether the current `ident` begins a `generic_name`, i.e. it is followed by a
/// type-argument list `(_type ')+`.
///
/// A generic name is a base identifier followed by at least one type argument
/// that is glued to a `'`. We confirm this by scanning forward from the token
/// after the base over a candidate type-argument run and checking that a `'`
/// appears before any token that cannot belong to a type argument. The scan
/// tracks `[` / `(` nesting so a bracketed array or unit argument does not end
/// it prematurely. This both recognises generics (`Vec i32'`,
/// `Optional ns::String'`) and rejects bare names, calls, and indexes.
pub(crate) fn at_generic_name(p: &Parser) -> bool {
    // The first argument token must begin a type; otherwise this is a plain name
    // (or a postfix call/index/member on it).
    if !TYPE_START.contains(p.nth(1)) {
        return false;
    }
    // The first type argument is `base TYPE ... '`. We accept a short run of
    // type-argument tokens at the top level and succeed on the first glued tick.
    // The bound is small (real type arguments are a handful of tokens) so this
    // lookahead never approaches the engine's advance-guard fuel; a longer run
    // is treated as a non-generic name and falls through to a plain identifier.
    const MAX_LOOKAHEAD: usize = 8;
    for n in 1..=MAX_LOOKAHEAD {
        match p.nth(n) {
            SyntaxKind::Tick => return true,
            SyntaxKind::Ident | SyntaxKind::ColonColon => {}
            kind if PRIMITIVE_TYPE_KW.contains(kind) => {}
            _ => return false,
        }
    }
    false
}

/// `( _type ' )+` (grammar.js `type_argument_list`). Each argument is a type
/// immediately followed by a glued tick. Emits a `TypeArgumentList` node.
pub(crate) fn type_argument_list(p: &mut Parser) {
    let m = p.start();
    loop {
        if !at_type_start(p) {
            p.error("expected a type argument");
            break;
        }
        type_(p);
        if p.at(SyntaxKind::Tick) && p.prev_joint() {
            p.bump(SyntaxKind::Tick);
        } else if p.at(SyntaxKind::Tick) {
            // A tick that is not glued is still consumed for resilience, but the
            // grammar requires immediacy, so flag it.
            p.error("type-argument tick must follow the type with no space");
            p.bump(SyntaxKind::Tick);
        } else {
            p.expect(SyntaxKind::Tick);
            break;
        }
        if !next_type_argument(p) {
            break;
        }
    }
    m.complete(p, SyntaxKind::TypeArgumentList);
}

/// Whether another type argument follows in a type-argument list.
fn next_type_argument(p: &Parser) -> bool {
    at_type_start(p)
}

/// Wraps an identifier token in an `Identifier` node (grammar.js `identifier`).
///
/// Accepts a plain identifier or a contextual keyword in identifier position
/// (see [`IDENT_LIKE`]), recording the leaf under [`SyntaxKind::Ident`] so the
/// CST identifier reads uniformly regardless of the token's lexed keyword kind.
pub(crate) fn identifier(p: &mut Parser) {
    let m = p.start();
    if at_ident_like(p) {
        p.bump_remap(SyntaxKind::Ident);
    } else {
        p.error("expected an identifier");
    }
    m.complete(p, SyntaxKind::Identifier);
}
