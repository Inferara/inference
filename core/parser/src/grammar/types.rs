//! Type grammar (`_type`, `_embedded_type`, `_name`, names).
//!
//! `_type`, `_embedded_type`, `_name`, `_simple_name` and
//! `_bracketed_generic_name` are hidden rules, so they dispatch without
//! opening a node. The concrete forms — `type_i32`, `type_array`, `type_fn`,
//! `identifier`, `generic_name`, `type_qualified_name` — each emit their node.

use crate::grammar::expr;
use crate::grammar::params;
use crate::parser::{CompletedMarker, Parser};
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

/// The tokens that can begin a `_type` (`_type` first set).
///
/// The reserved word `unit` is a member although no type is spelled with it:
/// [`type_`] is where it is refused, so every position that asks whether a type
/// starts here has to answer yes for it, or the refusal is never reached and
/// the position reports its own "expected" diagnostic instead.
pub(crate) const TYPE_START: TokenSet = PRIMITIVE_TYPE_KW.union(TokenSet::new(&[
    SyntaxKind::LParen,
    SyntaxKind::LBracket,
    SyntaxKind::FnKw,
    SyntaxKind::Ident,
    SyntaxKind::UnitKw,
]));

/// Whether the current token can begin a type.
pub(crate) fn at_type_start(p: &Parser) -> bool {
    p.at_ts(TYPE_START)
}

/// Keyword-spelling tokens that the grammar treats as ordinary identifiers when
/// they appear in identifier position.
///
/// `self`, `type`, `from` and `spec` are keywords only where a rule spells them
/// as literals: `self_reference`, the `type` heading the refused alias
/// declaration, the `from` of a use directive, the `spec` of a spec definition.
/// Wherever a rule reaches [`identifier`] instead they fall back to being names,
/// recorded under [`SyntaxKind::Ident`] so the CST reads uniformly. The corpus
/// relies on this: `self.type` uses `self` as a name and `type` as a member
/// name, and `spec::AuctionSpec` uses `spec` as a qualified-name qualifier. A
/// declaration's own name is such a position too, so `fn type() { }` and
/// `let type: i32 = 5;` both bind the keyword spelling.
///
/// This set widens the identifier *rule*, not every identifier *position*. Two
/// positions are decided by a dispatch on the bare `Ident` token before
/// [`identifier`] is ever reached, and so reject all four spellings: a parameter
/// name (`argument` routes on `Ident` followed by `:`) and a struct-field name
/// (the struct body loop routes on `Ident`). `fn f(type: i32)` and
/// `struct S { type: i32; }` are parse errors, and always have been.
///
/// The leading-keyword dispatch in items/statements (`item`, `definition`,
/// `statement`) routes a `spec`/`type` at the head of a definition or statement
/// to the keyword rule *before* the expression/name path is reached, so adding
/// them here does not make `spec Foo {}` or `type T = u8;` ambiguous. `type` is
/// listed here even though the language has no type-alias declaration and its
/// keyword rule exists only to refuse `type T = u8;`: the identifier positions
/// above are the reason the token stays contextual. That dispatch wins at the
/// head, so a statement *beginning* with the spelling is read as a declaration
/// and never as an expression — `type();` does not parse as a call of a function
/// named `type`, even though `fn type() { }` declares one and a call written
/// anywhere else in an expression reaches it. That is why the alias diagnostic
/// is gated on the declaration's `type <name>` head instead of being raised for
/// everything the rule receives.
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

/// The tokens that can begin a `_name` (`_name` first set): the [`IDENT_LIKE`]
/// tokens and the reserved word `unit`.
///
/// No name is spelled `unit`, but the word is a member for the reason it is one
/// of [`TYPE_START`]: [`identifier`] is where it is refused, so a position that
/// asks whether a name starts here has to answer yes for it, or the word is left
/// in place and read as whatever the position expects next.
pub(crate) const NAME_START: TokenSet = IDENT_LIKE.union(TokenSet::new(&[SyntaxKind::UnitKw]));

/// Whether the current token can begin a name (see [`NAME_START`]).
pub(crate) fn at_name_start(p: &Parser) -> bool {
    p.at_ts(NAME_START)
}

/// The tokens that can begin a name in a position a dispatch decides on the
/// bare [`SyntaxKind::Ident`] token before any name rule is reached: an
/// identifier, and the reserved word `unit`.
///
/// Such a position does not take the contextual keywords of [`IDENT_LIKE`] (see
/// there), but it does take `unit`, for the reason [`NAME_START`] does: the name
/// rules are where the word is refused, so a dispatch that did not route it to
/// one would leave it to be read as whatever the position expects next.
pub(crate) const PLAIN_NAME_START: TokenSet =
    TokenSet::new(&[SyntaxKind::Ident, SyntaxKind::UnitKw]);

/// Parses a `_type`: an embedded type, a bracketed generic name, or a name
/// (`_type`). Hidden rule: emits no node of its own.
pub(crate) fn type_(p: &mut Parser) {
    match p.current() {
        kind if PRIMITIVE_TYPE_KW.contains(kind) => primitive_type(p),
        // `unit::T` names a module and `unit T'` a generic type, not the unit
        // type, so each parses as the name it is, the word takes the refusal a
        // name gets, and the path or type arguments after it still parse.
        SyntaxKind::UnitKw
            if (p.nth_at(1, SyntaxKind::ColonColon) && p.at_joint()) || at_generic_name(p) =>
        {
            name(p);
        }
        SyntaxKind::UnitKw => reserved_unit_type(p),
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
/// (`type_i8`..`type_bool`).
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

/// The diagnostic for the reserved word `unit` written where a type belongs.
const UNIT_IS_NOT_A_TYPE_MESSAGE: &str =
    "`unit` is a reserved word, not a type: the unit type is spelled `()`";

/// Refuses `unit` in a type position and completes the node `()` would have
/// produced, so lowering sees the unit type and nothing after the parser has
/// anything further to say about the position. The word is consumed, which is what keeps
/// the declaration around it parsing as written: left in place, it would be read
/// as whatever the enclosing rule expects next and cascade from there.
fn reserved_unit_type(p: &mut Parser) {
    let m = p.start();
    p.error(UNIT_IS_NOT_A_TYPE_MESSAGE);
    p.bump(SyntaxKind::UnitKw);
    m.complete(p, SyntaxKind::TypeUnit);
}

/// `( )` (joint) — the unit type (`type_unit`).
fn unit_type(p: &mut Parser) {
    let m = p.start();
    p.bump(SyntaxKind::LParen);
    p.expect(SyntaxKind::RParen);
    m.complete(p, SyntaxKind::TypeUnit);
}

/// `[ _type [ ; (number_literal | _name) ] ]` (`type_array`).
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

/// `fn argument_list [ -> _type ]` (`type_fn`).
fn fn_type(p: &mut Parser) {
    let m = p.start();
    p.bump(SyntaxKind::FnKw);
    params::argument_list(p);
    if p.eat(SyntaxKind::Arrow) {
        type_(p);
    }
    m.complete(p, SyntaxKind::TypeFn);
}

/// `( generic_name )` (`_bracketed_generic_name`). Hidden rule:
/// emits no node; the inner `generic_name` is the only named child.
///
/// `(unit)` is the reserved word in a type position that happens to be
/// bracketed, so it goes to [`type_`] and takes the refusal every other type
/// position gives it, lowering as the unit type, rather than being read as a
/// name.
fn bracketed_generic_name(p: &mut Parser) {
    p.bump(SyntaxKind::LParen);
    if p.at(SyntaxKind::UnitKw) {
        type_(p);
    } else {
        name(p);
    }
    p.expect(SyntaxKind::RParen);
}

/// Parses a `_name`: `type_qualified_name` (`ident (:: simple_name)+`, each `::`
/// glued) or a `_simple_name` (`_name`). Hidden rule.
///
/// A qualified type may name a type reached through a chain of file namespaces
/// (`lib::geom::Point`), so the `::` hops form a loop rather than a single step,
/// mirroring the expression postfix chain (`grammar/expr.rs`). Every segment but
/// the last is a namespace qualifier; the last is the leaf type name. The single
/// `TypeQualifiedName` node therefore carries one `Identifier` child per segment,
/// which lowering splits into a qualifier list plus the leaf.
pub(crate) fn name(p: &mut Parser) {
    if at_name_start(p) && p.nth_at(1, SyntaxKind::ColonColon) && p.at_joint() {
        let m = p.start();
        declared_name(p, NameRole::Module);
        p.bump(SyntaxKind::ColonColon);
        qualified_simple_name(p);
        while p.at(SyntaxKind::ColonColon) && p.prev_joint() {
            p.bump(SyntaxKind::ColonColon);
            qualified_simple_name(p);
        }
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
/// `i32` rather than the `type_i32` keyword. An LL lexer cannot
/// distinguish these by context, so we mirror the grammar by treating the
/// primitive type keywords as identifier spellings in this one position. The
/// resulting CST child is an `Identifier` node (the keyword token is remapped to
/// `Ident`), keeping the arena byte-identical to the legacy `Builder`.
///
/// The reserved word `unit` gets no such pass: it is refused as the module a
/// further `::` makes it, or as the type the path's last segment names. Followed
/// by type arguments it is the base of a generic name instead, which parses as
/// one so that the arguments do too, and the base takes the refusal the base of
/// an unqualified generic name gets.
fn qualified_simple_name(p: &mut Parser) {
    if p.at(SyntaxKind::UnitKw) && !at_generic_name(p) {
        let role = if p.nth_at(1, SyntaxKind::ColonColon) {
            NameRole::Module
        } else {
            NameRole::Type
        };
        declared_name(p, role);
    } else if at_name_start(p) {
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
/// plain `identifier` (`_simple_name`). Hidden rule.
pub(crate) fn simple_name(p: &mut Parser) {
    if !at_name_start(p) {
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
            // `unit` stays in the run so a type argument spelled with it reaches
            // the type rule that refuses it, rather than ending the generic name
            // one token early and leaving the tick to cascade.
            SyntaxKind::Ident | SyntaxKind::ColonColon | SyntaxKind::UnitKw => {}
            kind if PRIMITIVE_TYPE_KW.contains(kind) => {}
            _ => return false,
        }
    }
    false
}

/// `( _type ' )+` (`type_argument_list`). Each argument is a type
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

/// What a name in a declaring position names, for the diagnostic that refuses
/// the reserved word `unit` there.
#[derive(Clone, Copy)]
pub(crate) enum NameRole {
    Binding,
    Constant,
    Enum,
    Field,
    Function,
    ImportedItem,
    Module,
    Parameter,
    Spec,
    Struct,
    Type,
    TypeAlias,
    TypeParameter,
    Variant,
}

impl NameRole {
    /// The thing named, with its article, as the diagnostic reads it.
    fn noun(self) -> &'static str {
        match self {
            NameRole::Binding => "a binding",
            NameRole::Constant => "a constant",
            NameRole::Enum => "an enum",
            NameRole::Field => "a field",
            NameRole::Function => "a function",
            NameRole::ImportedItem => "an imported item",
            NameRole::Module => "a module",
            NameRole::Parameter => "a parameter",
            NameRole::Spec => "a spec",
            NameRole::Struct => "a struct",
            NameRole::Type => "a type",
            NameRole::TypeAlias => "a type alias",
            NameRole::TypeParameter => "a type parameter",
            NameRole::Variant => "an enum variant",
        }
    }
}

/// The diagnostic for the reserved word `unit` written as the name of `role`.
fn reserved_unit_name_message(role: NameRole) -> String {
    format!("`unit` is a reserved word and cannot name {}", role.noun())
}

/// The diagnostic for the reserved word `unit` written where the syntax does
/// not say what the name names.
const UNIT_IS_NOT_A_NAME_MESSAGE: &str = "`unit` is a reserved word and cannot be used as a name";

/// Parses a name whose position says what it names — a declaration's own name,
/// a parameter, a field, a variant, a segment of a `use` path or of a type —
/// which is an [`identifier`] unless it is the reserved word `unit`, refused
/// here with the thing it tried to name.
pub(crate) fn declared_name(p: &mut Parser, role: NameRole) {
    if p.at(SyntaxKind::UnitKw) {
        refuse_unit_name(p, reserved_unit_name_message(role));
    } else {
        identifier(p);
    }
}

/// Reports `message` on the reserved word `unit` and completes the
/// `Identifier` node with the word as its leaf, as the missing-identifier path
/// completes the node without one.
///
/// Consuming the word is the difference that matters: left in place, it is read
/// by the rest of the enclosing rule as whatever that rule expects next, which
/// is how a keyword in a name position turns one fault into a screen of
/// `expected …` lines. Kept as the name, it is also what a later reference to
/// the same name resolves to.
fn refuse_unit_name(p: &mut Parser, message: String) -> CompletedMarker {
    let m = p.start();
    p.error(message);
    p.bump_remap(SyntaxKind::Ident);
    m.complete(p, SyntaxKind::Identifier)
}

/// Wraps an identifier token in an `Identifier` node (`identifier`).
///
/// Accepts a plain identifier or a contextual keyword in identifier position
/// (see [`IDENT_LIKE`]), recording the leaf under [`SyntaxKind::Ident`] so the
/// CST identifier reads uniformly regardless of the token's lexed keyword kind.
///
/// The reserved word `unit` is refused here, as a name that cannot name
/// anything rather than as the thing it names. That is the refusal every name
/// a position does not classify gets — mostly references in expressions, where
/// the syntax often cannot tell: `unit::k()` may reach into a module or call an
/// associated function of a type, and `unit()` may call a function or a
/// parameter. A position that can tell goes through [`declared_name`] instead.
pub(crate) fn identifier(p: &mut Parser) -> CompletedMarker {
    if p.at(SyntaxKind::UnitKw) {
        return refuse_unit_name(p, UNIT_IS_NOT_A_NAME_MESSAGE.to_string());
    }
    let m = p.start();
    if at_ident_like(p) {
        p.bump_remap(SyntaxKind::Ident);
    } else {
        p.error("expected an identifier");
    }
    m.complete(p, SyntaxKind::Identifier)
}
