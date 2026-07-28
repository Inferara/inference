//! Expression grammar: a Pratt parser over the operator precedence table.
//!
//! Expression precedence is encoded as numeric binding powers, defined in the
//! `bp` module below.
//! Binary operators are left-associative except `**`, which is right. Prefix
//! `! - ~` bind at `UNARY`; postfix call `(`, member `.`, type-member `::` and
//! index `[` bind tighter still.
//!
//! Named expression nodes (`binary_expression`, `prefix_unary_expression`,
//! `function_call_expression`, `member_access_expression`,
//! `type_member_access_expression`, `array_index_access_expression`,
//! `parenthesized_expression`, `struct_expression`, the literals,
//! `uzumaki_keyword`) each emit a CST node. The hidden `_expression`,
//! `_literal`, `_name` arms only dispatch.

use crate::grammar::types;
use crate::lexer::is_ident_start;
use crate::parser::{CompletedMarker, Parser};
use crate::syntax_kind::SyntaxKind;
use crate::token_set::TokenSet;

/// Binding powers for binary operators. Higher binds tighter.
mod bp {
    pub(super) const LOGICAL_OR: u8 = 48;
    pub(super) const LOGICAL_AND: u8 = 49;
    pub(super) const OR: u8 = 57;
    pub(super) const XOR: u8 = 58;
    pub(super) const AND: u8 = 59;
    pub(super) const EQUALS: u8 = 60;
    pub(super) const COMPARE: u8 = 70;
    pub(super) const SHIFT: u8 = 80;
    pub(super) const ADD: u8 = 97;
    pub(super) const MUL: u8 = 98;
    pub(super) const POW: u8 = 99;
}

/// The left binding power of a binary operator token, or `None` if the token is
/// not a binary operator. `right` is whether it associates to the right.
fn binary_bp(kind: SyntaxKind) -> Option<(u8, bool)> {
    let bp = match kind {
        SyntaxKind::PipePipe => (bp::LOGICAL_OR, false),
        SyntaxKind::AmpAmp => (bp::LOGICAL_AND, false),
        SyntaxKind::Pipe => (bp::OR, false),
        SyntaxKind::Caret => (bp::XOR, false),
        SyntaxKind::Amp => (bp::AND, false),
        SyntaxKind::EqEq | SyntaxKind::Ne => (bp::EQUALS, false),
        SyntaxKind::Lt | SyntaxKind::Le | SyntaxKind::Gt | SyntaxKind::Ge => (bp::COMPARE, false),
        SyntaxKind::Shl | SyntaxKind::Shr => (bp::SHIFT, false),
        SyntaxKind::Plus | SyntaxKind::Minus => (bp::ADD, false),
        SyntaxKind::Star | SyntaxKind::Slash | SyntaxKind::Percent => (bp::MUL, false),
        SyntaxKind::StarStar => (bp::POW, true),
        _ => return None,
    };
    Some(bp)
}

/// The tokens that can begin an expression (`_expression` first set).
pub(crate) const EXPR_START: TokenSet = TokenSet::new(&[
    SyntaxKind::Number,
    SyntaxKind::String,
    SyntaxKind::TrueKw,
    SyntaxKind::FalseKw,
    SyntaxKind::LBracket,
    SyntaxKind::LParen,
    SyntaxKind::At,
    SyntaxKind::Ident,
    SyntaxKind::Bang,
    SyntaxKind::Minus,
    SyntaxKind::Tilde,
])
.union(types::TYPE_START)
.union(types::IDENT_LIKE);

/// Whether the current token can begin an expression.
pub(crate) fn at_expr_start(p: &Parser) -> bool {
    p.at_ts(EXPR_START)
}

/// Parses a full expression in a normal context where a `{` after a name opens a
/// struct literal (`_expression`). Hidden rule: emits no node.
pub(crate) fn expr(p: &mut Parser) {
    expr_bp(p, 0, true);
}

/// Parses an expression in a condition context, where a trailing `{` opens the
/// following block rather than a struct literal (the `if`/`loop` head). This is
/// the rust-analyzer technique for the struct-literal/block ambiguity.
pub(crate) fn expr_no_struct(p: &mut Parser) {
    expr_bp(p, 0, false);
}

/// The Pratt loop: parse a unary/atom operand, then fold in binary operators
/// whose binding power exceeds `min_bp`. `allow_struct` controls whether a name
/// followed by `{` is read as a struct literal.
fn expr_bp(p: &mut Parser, min_bp: u8, allow_struct: bool) -> Option<CompletedMarker> {
    let mut lhs = unary_expr(p, allow_struct)?;

    while let Some((op_bp, right_assoc)) = binary_bp(p.current()) {
        // Stop when the operator binds no tighter than the caller's floor. For a
        // right-associative operator at exactly the floor we still recurse, so
        // `a ** b ** c` nests to the right.
        if op_bp <= min_bp && !(right_assoc && op_bp == min_bp) {
            break;
        }
        let m = lhs.precede(p);
        p.bump_any(); // the operator token
        let next_min = if right_assoc { op_bp - 1 } else { op_bp };
        expr_bp(p, next_min, allow_struct);
        lhs = m.complete(p, SyntaxKind::BinaryExpression);
    }

    Some(lhs)
}

/// Parses a prefix-unary expression or, if there is no prefix operator, an atom
/// with its postfix chain (`prefix_unary_expression` plus the postfix
/// rules).
fn unary_expr(p: &mut Parser, allow_struct: bool) -> Option<CompletedMarker> {
    let op = match p.current() {
        SyntaxKind::Bang => SyntaxKind::UnaryNot,
        SyntaxKind::Minus => SyntaxKind::UnaryMinus,
        SyntaxKind::Tilde => SyntaxKind::UnaryBitnot,
        _ => return postfix_expr(p, allow_struct),
    };
    let m = p.start();
    let op_marker = p.start();
    p.bump_any();
    op_marker.complete(p, op);
    unary_expr(p, allow_struct);
    Some(m.complete(p, SyntaxKind::PrefixUnaryExpression))
}

/// Parses an atom and then repeatedly folds in postfix operators: function call
/// `(`, member access `.`, type-member access `::` (glued), and index `[`.
///
/// A `{` after a multi-segment `::` chain opens a namespace-qualified struct
/// literal (`a::b::Type { .. }`, #63), the postfix analogue of the bare and
/// single-segment forms `name_atom` recognizes. It fires only when struct
/// literals are allowed (suppressed in `if`/`loop` heads) and only directly
/// after a `::` chain, so value-position `{` after `.`, a call, or an index is
/// left to open the following block.
fn postfix_expr(p: &mut Parser, allow_struct: bool) -> Option<CompletedMarker> {
    let mut lhs = atom(p, allow_struct)?;
    loop {
        lhs = match p.current() {
            SyntaxKind::LParen => function_call(p, lhs),
            SyntaxKind::Dot => member_access(p, lhs),
            SyntaxKind::ColonColon if p.prev_joint() => type_member_access(p, lhs),
            SyntaxKind::LBracket => array_index(p, lhs),
            SyntaxKind::LBrace
                if allow_struct && lhs.kind() == SyntaxKind::TypeMemberAccessExpression =>
            {
                qualified_struct_literal(p, lhs)
            }
            _ => break,
        };
    }
    Some(lhs)
}

/// Wraps a completed `::` chain (`a::b::Type`) in a `struct_expression`, parsing
/// the `{ field: value, .. }` body. The chain becomes the struct name, matching
/// the node shape the bare and single-segment struct literals produce.
fn qualified_struct_literal(p: &mut Parser, lhs: CompletedMarker) -> CompletedMarker {
    let m = lhs.precede(p);
    struct_body(p);
    m.complete(p, SyntaxKind::StructExpression)
}

/// `lhs ( [ args ] )` (`function_call_expression`). Each argument is
/// an optional `argument_name :` followed by an expression.
fn function_call(p: &mut Parser, lhs: CompletedMarker) -> CompletedMarker {
    let m = lhs.precede(p);
    p.bump(SyntaxKind::LParen);
    if !p.at(SyntaxKind::RParen) {
        call_argument(p);
        while p.eat(SyntaxKind::Comma) {
            if p.at(SyntaxKind::RParen) {
                break;
            }
            call_argument(p);
        }
    }
    p.expect(SyntaxKind::RParen);
    m.complete(p, SyntaxKind::FunctionCallExpression)
}

/// A single call argument: `[ name : ] expression`
/// (`function_call_expression` argument). The argument name is a `_name`; when
/// present, the lower step pairs it with the following expression.
fn call_argument(p: &mut Parser) {
    if types::at_ident_like(p) && p.nth_at(1, SyntaxKind::Colon) {
        types::name(p);
        p.bump(SyntaxKind::Colon);
    }
    expr(p);
}

/// `lhs . simple_name` (`member_access_expression`).
fn member_access(p: &mut Parser, lhs: CompletedMarker) -> CompletedMarker {
    let m = lhs.precede(p);
    p.bump(SyntaxKind::Dot);
    types::simple_name(p);
    m.complete(p, SyntaxKind::MemberAccessExpression)
}

/// `lhs :: simple_name` (`type_member_access_expression`), the `::`
/// glued (token.immediate).
fn type_member_access(p: &mut Parser, lhs: CompletedMarker) -> CompletedMarker {
    let m = lhs.precede(p);
    p.bump(SyntaxKind::ColonColon);
    types::simple_name(p);
    m.complete(p, SyntaxKind::TypeMemberAccessExpression)
}

/// `lhs [ index ]` (`array_index_access_expression`).
fn array_index(p: &mut Parser, lhs: CompletedMarker) -> CompletedMarker {
    let m = lhs.precede(p);
    p.bump(SyntaxKind::LBracket);
    expr(p);
    p.expect(SyntaxKind::RBracket);
    m.complete(p, SyntaxKind::ArrayIndexAccessExpression)
}

/// Parses an atomic expression: a literal, a parenthesised expression, a unit
/// literal, the uzumaki keyword, or a name (possibly the head of a struct
/// literal). Returns `None` only when no expression could be started.
fn atom(p: &mut Parser, allow_struct: bool) -> Option<CompletedMarker> {
    let cm = match p.current() {
        SyntaxKind::Number => number_literal(p),
        SyntaxKind::TrueKw | SyntaxKind::FalseKw => bool_literal(p),
        SyntaxKind::String => string_literal(p),
        SyntaxKind::LBracket => array_literal(p),
        SyntaxKind::At => uzumaki(p),
        SyntaxKind::LParen => paren_or_unit(p),
        // A name atom: a plain identifier or a contextual keyword used in
        // identifier position (`self`, `type`).
        kind if types::IDENT_LIKE.contains(kind) => name_atom(p, allow_struct),
        _ => {
            p.err_and_bump("expected an expression");
            return None;
        }
    };
    Some(cm)
}

/// The diagnostic for a type suffix glued to an integer literal (`16i64`).
///
/// Inference has no literal suffixes: an integer literal takes its type from the
/// context it appears in, so a suffix is never the fix. The message names the
/// offending spelling and points at the one place a type can be pinned when no
/// typed value is nearby — the binding — with a worked example in a real type,
/// which stays correct whether or not the suffix names one.
fn suffix_message(suffix: &str) -> String {
    format!(
        "integer literals do not take a type suffix — remove `{suffix}`; an integer literal \
         takes its type from where it is used. If there is no value of that type nearby, name \
         the type at the binding: `let n: i64 = 16;`"
    )
}

/// The diagnostic for any other tail glued to an integer literal: a digit
/// separator (`1_000`), a radix prefix (`0x1F`, `0b01`, `0o17`), an exponent
/// (`1e10`), or plain adjacent garbage (`16true`).
///
/// These lex as a `Number` plus an identifier, so without this they would parse
/// as a *different, valid-looking* number — `1_000` as `1` — which is the trap
/// worth a dedicated message. The tail is named first so the message stays true
/// for the spellings the trailing enumeration does not describe.
fn non_decimal_message(tail: &str) -> String {
    format!(
        "`{tail}` cannot follow the digits of a number literal; Inference numbers are decimal \
         digits only — no `_` separators and no `0x`/`0b`/`0o` prefixes"
    )
}

/// Whether `text` starts an identifier run — that is, whether the lexer's
/// identifier scanner produced it.
///
/// The number scanner stops at the first non-digit and the identifier scanner
/// takes over, so a literal written as one word (`16i64`, `1_000`, `0x1F`)
/// arrives as a `Number` glued to exactly such a token. Only the first byte is
/// examined because [`is_ident_start`] is what decided the split; the rest of the
/// token is word characters by construction.
fn is_identifier_run(text: &str) -> bool {
    text.as_bytes().first().is_some_and(|&b| is_ident_start(b))
}

/// Whether `text` is shaped like an integer type suffix: an optional `_`, then
/// `i` or `u`, then anything.
///
/// Deliberately wider than the eight integer type names, so `5i128`, `5usize`
/// and `5u` all get the same message as `5i64`. Narrowing this to the real names
/// would route `i128` to [`non_decimal_message`], implying `i64` is a recognized
/// suffix while `i128` is garbage; narrowing it to `_?[iu][0-9]+` would do the
/// same to `usize`, the likeliest spelling for someone arriving from Rust — and
/// would tell that author about separators and radix prefixes they did not
/// write. Every spelling here is rejected identically, so one message serves
/// them all. The remaining tails start with neither `i` nor `u` (`_000`, `x1F`,
/// `b01`, `o17`, `e10`), so nothing is misrouted the other way.
fn is_integer_suffix(text: &str) -> bool {
    let text = text.strip_prefix('_').unwrap_or(text);
    text.starts_with(['i', 'u'])
}

/// Wraps a `Number` token in a `number_literal` node
/// (`number_literal`). A leading `-` glued to the digits is part of the token, so
/// `-42` is a single literal while `- 42` is a prefix-unary expression.
///
/// A `Number` glued to an identifier run is one malformed literal the lexer split
/// in two (`16i64`, `1_000`, `0x1F`). The tail is consumed into the literal node
/// with a single teaching diagnostic: consuming it is what keeps a stray token
/// out of expression position, where it would cascade into "expected Semi" plus
/// "expected an expression". The `Number` token still carries the digits alone,
/// which is what lowering stores as the literal's value.
pub(crate) fn number_literal(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    p.bump(SyntaxKind::Number);
    let tail = p.current_text();
    if p.prev_joint() && is_identifier_run(tail) {
        let message = if is_integer_suffix(tail) {
            suffix_message(tail)
        } else {
            non_decimal_message(tail)
        };
        p.error(message);
        p.bump_any();
    }
    m.complete(p, SyntaxKind::NumberLiteral)
}

/// Wraps `true`/`false` in a `bool_literal` node (`bool_literal`).
fn bool_literal(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    p.bump_any();
    m.complete(p, SyntaxKind::BoolLiteral)
}

/// Wraps a `String` token in a `string_literal` node
/// (`string_literal`).
pub(crate) fn string_literal(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    p.expect(SyntaxKind::String);
    m.complete(p, SyntaxKind::StringLiteral)
}

/// `[ [ sep1(expr, ,) ] ]` (`array_literal`).
fn array_literal(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    p.bump(SyntaxKind::LBracket);
    if !p.at(SyntaxKind::RBracket) {
        expr(p);
        while p.eat(SyntaxKind::Comma) {
            if p.at(SyntaxKind::RBracket) {
                break;
            }
            expr(p);
        }
    }
    p.expect(SyntaxKind::RBracket);
    m.complete(p, SyntaxKind::ArrayLiteral)
}

/// The uzumaki keyword `@` (`uzumaki_keyword`).
fn uzumaki(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    p.bump(SyntaxKind::At);
    m.complete(p, SyntaxKind::UzumakiKeyword)
}

/// Disambiguates the three `(`-led atoms: `( )` (glued) is the unit literal;
/// `( generic_name )` is a bracketed generic name used as a type-member base;
/// otherwise `( expression )` is a parenthesised expression.
fn paren_or_unit(p: &mut Parser) -> CompletedMarker {
    if p.nth_at(1, SyntaxKind::RParen) {
        let m = p.start();
        p.bump(SyntaxKind::LParen);
        p.expect(SyntaxKind::RParen);
        return m.complete(p, SyntaxKind::UnitLiteral);
    }
    let m = p.start();
    p.bump(SyntaxKind::LParen);
    expr(p);
    p.expect(SyntaxKind::RParen);
    m.complete(p, SyntaxKind::ParenthesizedExpression)
}

/// Parses a name-headed atom: a struct literal `Name { .. }` (when allowed and
/// followed by `{`), or a plain name (`identifier`, `generic_name`, or
/// `type_qualified_name`) that the postfix chain may extend.
fn name_atom(p: &mut Parser, allow_struct: bool) -> CompletedMarker {
    let name_cm = name_expr(p);
    if allow_struct && p.at(SyntaxKind::LBrace) {
        let m = name_cm.precede(p);
        struct_body(p);
        m.complete(p, SyntaxKind::StructExpression)
    } else {
        name_cm
    }
}

/// `{ [ sep1(field_name : field_value, ,) ] }` (`struct_expression`
/// body). The leading name has already been parsed by the caller.
fn struct_body(p: &mut Parser) {
    p.bump(SyntaxKind::LBrace);
    if !p.at(SyntaxKind::RBrace) {
        struct_field_init(p);
        while p.eat(SyntaxKind::Comma) {
            if p.at(SyntaxKind::RBrace) {
                break;
            }
            struct_field_init(p);
        }
    }
    p.expect(SyntaxKind::RBrace);
}

/// A single `field_name : field_value` pair in a struct literal.
fn struct_field_init(p: &mut Parser) {
    types::name(p);
    p.expect(SyntaxKind::Colon);
    expr(p);
}

/// Parses a bare name as an expression: `identifier`, `generic_name`, or
/// `type_qualified_name` (`_name`, used as a `_simple_name` lval or a
/// type-member base). Returns the completed name node so postfix can extend it.
fn name_expr(p: &mut Parser) -> CompletedMarker {
    if types::at_ident_like(p) && p.nth_at(1, SyntaxKind::ColonColon) && p.at_joint() {
        let m = p.start();
        types::identifier(p);
        p.bump(SyntaxKind::ColonColon);
        types::simple_name(p);
        m.complete(p, SyntaxKind::TypeQualifiedName)
    } else {
        simple_name_expr(p)
    }
}

/// Parses a `_simple_name` as an expression: a `generic_name` or an
/// `identifier`, returning the completed node.
fn simple_name_expr(p: &mut Parser) -> CompletedMarker {
    if types::at_generic_name(p) {
        let m = p.start();
        types::identifier(p);
        types::type_argument_list(p);
        m.complete(p, SyntaxKind::GenericName)
    } else {
        let m = p.start();
        if types::at_ident_like(p) {
            p.bump_remap(SyntaxKind::Ident);
        } else {
            p.error("expected an identifier");
        }
        m.complete(p, SyntaxKind::Identifier)
    }
}
