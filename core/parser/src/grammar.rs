//! The recursive-descent grammar for the Inference language (issue #62, Phases
//! 3 & 4).
//!
//! Each function here implements one grammar production. The emission contract
//! uses a hidden-rule convention: a production whose name starts with an
//! underscore is **inlined** (it opens no CST node and only dispatches),
//! while a named rule produces a CST [`SyntaxNode`](crate::SyntaxNode) of the
//! matching kind. So `_statement`, `_type`, `_expression`, `_name` etc. here are
//! plain dispatch functions, whereas `function_definition`, `binary_expression`,
//! `identifier`, `number_literal` open a marker and complete it.
//!
//! The grammar is **resilient**: every loop is guaranteed to make progress (it
//! bumps the offending token into an `Error` node when stuck), so the parser
//! never panics and always reaches end of input, even on malformed sources.

mod expr;
mod items;
mod params;
mod stmt;
mod types;

use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;
use crate::token_set::TokenSet;

/// The tokens that may begin a top-level item or a definition inside a `spec`.
///
/// Used as the recovery anchor for top-level parsing: when an unexpected token
/// is hit, the parser bumps tokens into an `Error` node until it reaches one of
/// these so it can resynchronise on the next item.
pub(crate) const ITEM_RECOVERY: TokenSet = TokenSet::new(&[
    SyntaxKind::UseKw,
    SyntaxKind::SpecKw,
    SyntaxKind::FnKw,
    SyntaxKind::ExternalKw,
    SyntaxKind::ConstKw,
    SyntaxKind::TypeKw,
    SyntaxKind::EnumKw,
    SyntaxKind::StructKw,
    SyntaxKind::PubKw,
]);

/// Parses a whole source file: a sequence of use directives, spec definitions
/// and definitions, wrapped in a `SourceFile` node (`source_file`).
pub fn source_file(p: &mut Parser) {
    let m = p.start();
    while !p.at_eof() {
        if at_item_start(p) {
            // Defense-in-depth: if a future `item` handler completes without
            // consuming any token, the cursor is unchanged and this loop would
            // spin forever (the fuel guard does not catch it, since completing a
            // marker refills the fuel). Bump the offending token into an Error
            // node so any non-advancing handler degrades to a recoverable error.
            let before = p.pos();
            item(p);
            if p.pos() == before {
                p.err_and_bump("expected an item");
            }
        } else {
            // An unexpected token at item position: consume it into an Error
            // node so the loop always advances, then retry from the next token.
            p.err_recover("expected an item", ITEM_RECOVERY);
        }
    }
    m.complete(p, SyntaxKind::SourceFile);
}

/// Whether the current token can begin a top-level item.
fn at_item_start(p: &Parser) -> bool {
    p.at_ts(ITEM_RECOVERY)
}

/// Dispatches a single top-level item: a use directive, a spec definition, or a
/// plain definition (`source_file` choice arms).
///
/// Because `use`, `spec`, and `external` carry their own visibility rules, the
/// dispatch peeks past an optional leading `pub` to the item keyword so that
/// `pub use`/`pub spec` reach their dedicated handlers (which accept or report
/// the modifier) rather than falling into `definition`, where a `pub` followed
/// by a non-definition keyword would error generically.
fn item(p: &mut Parser) {
    let keyword = if p.at(SyntaxKind::PubKw) {
        p.nth(1)
    } else {
        p.current()
    };
    match keyword {
        SyntaxKind::UseKw => items::use_directive(p),
        SyntaxKind::SpecKw => items::spec_definition(p),
        _ => items::definition(p),
    }
}

#[cfg(test)]
mod tests {
    use crate::parse_to_cst;
    use crate::syntax_kind::SyntaxKind;
    use crate::syntax_tree::{SyntaxElement, SyntaxNode};

    /// Parses `src` and returns the rendered CST plus the parse-error count.
    fn parse(src: &str) -> (SyntaxNode, usize) {
        let (tree, errors) = parse_to_cst(src);
        (tree, errors.len())
    }

    /// Parses `src` and returns the rendered CST plus the parse-error messages,
    /// in source order. Used by the diagnostic-matrix tests that assert the exact
    /// message text and the exact error count (no cascade).
    fn parse_messages(src: &str) -> (SyntaxNode, Vec<String>) {
        let (tree, errors) = parse_to_cst(src);
        (tree, errors.into_iter().map(|e| e.message).collect())
    }

    /// The verbatim glob-rejection diagnostic, kept in sync with
    /// `items::GLOB_IMPORT_MESSAGE`. Duplicated here (rather than re-exported)
    /// so the test pins the user-facing wording: a change to the production
    /// message must be a deliberate edit here too.
    const GLOB_MESSAGE: &str =
        "glob imports are not supported; import the file (use a::b;) or list items \
         explicitly (use a::b::{x, y};)";

    /// The verbatim non-decimal-tail diagnostic for `tail`, kept in sync with
    /// `expr::non_decimal_message`. Duplicated for the same reason as
    /// [`GLOB_MESSAGE`]: the user-facing wording is pinned here.
    fn non_decimal_message(tail: &str) -> String {
        format!(
            "`{tail}` cannot follow the digits of a number literal; Inference numbers are decimal \
             digits only — no `_` separators and no `0x`/`0b`/`0o` prefixes"
        )
    }

    /// The verbatim type-suffix diagnostic for `suffix`, kept in sync with
    /// `expr::suffix_message`.
    fn suffix_message(suffix: &str) -> String {
        format!(
            "integer literals do not take a type suffix — remove `{suffix}`; an integer literal \
             takes its type from where it is used. If there is no value of that type nearby, name \
             the type at the binding: `let n: i64 = 16;`"
        )
    }

    /// The indented S-expression of `src`'s CST, for shape assertions.
    fn tree(src: &str) -> String {
        let (node, _) = parse_to_cst(src);
        node.debug_tree(src)
    }

    /// Whether the CST contains any `Error` node.
    fn has_error_node(node: &SyntaxNode) -> bool {
        if node.kind == SyntaxKind::Error {
            return true;
        }
        node.children.iter().any(|c| match c {
            SyntaxElement::Node(n) => has_error_node(n),
            SyntaxElement::Token(_) => false,
        })
    }

    /// Asserts `src` parses cleanly: no `Error` node and no parse errors.
    fn assert_clean(src: &str) {
        let (node, errors) = parse_to_cst(src);
        assert!(
            !has_error_node(&node),
            "unexpected Error node parsing {src:?}:\n{}",
            node.debug_tree(src)
        );
        assert!(
            errors.is_empty(),
            "unexpected parse errors parsing {src:?}: {errors:?}"
        );
    }

    /// Counts the descendant nodes of a given kind anywhere in the tree.
    fn count_kind(node: &SyntaxNode, kind: SyntaxKind) -> usize {
        let here = usize::from(node.kind == kind);
        here + node
            .children
            .iter()
            .map(|c| match c {
                SyntaxElement::Node(n) => count_kind(n, kind),
                SyntaxElement::Token(_) => 0,
            })
            .sum::<usize>()
    }

    /// The first descendant node of `kind`, depth-first.
    fn find(node: &SyntaxNode, kind: SyntaxKind) -> Option<&SyntaxNode> {
        if node.kind == kind {
            return Some(node);
        }
        for c in &node.children {
            if let SyntaxElement::Node(n) = c
                && let Some(found) = find(n, kind)
            {
                return Some(found);
            }
        }
        None
    }

    /// The root's first definition/statement-level node of `kind`.
    fn first(src: &str, kind: SyntaxKind) -> SyntaxNode {
        let (root, _) = parse_to_cst(src);
        find(&root, kind)
            .cloned()
            .unwrap_or_else(|| panic!("no {kind:?} node in:\n{}", root.debug_tree(src)))
    }

    // items

    #[test]
    fn function_definition_shape() {
        let src = "fn add(a: i32, b: i32) -> i32 { return a + b; }";
        assert_clean(src);
        let f = first(src, SyntaxKind::FunctionDefinition);
        assert_eq!(
            f.child(SyntaxKind::Identifier).map(|n| n.text(src)),
            Some("add")
        );
        let args = f.child(SyntaxKind::ArgumentList).unwrap();
        assert_eq!(count_kind(args, SyntaxKind::ArgumentDeclaration), 2);
        assert!(f.child(SyntaxKind::TypeI32).is_some(), "returns type i32");
        assert!(f.child(SyntaxKind::Block).is_some());
    }

    #[test]
    fn function_with_visibility_and_no_return() {
        assert_clean("pub fn f() { }");
        let f = first("pub fn f() { }", SyntaxKind::FunctionDefinition);
        assert!(f.child(SyntaxKind::Visibility).is_some());
    }

    #[test]
    fn function_with_type_parameters() {
        let src = "fn foo T'(a: T) { }";
        assert_clean(src);
        let f = first(src, SyntaxKind::FunctionDefinition);
        let tps = f.child(SyntaxKind::TypeArgumentListDefinition).unwrap();
        assert_eq!(count_kind(tps, SyntaxKind::Identifier), 1);
    }

    #[test]
    fn function_body_nondet_forall() {
        let src = "fn f() -> () forall { return (); }";
        assert_clean(src);
        let f = first(src, SyntaxKind::FunctionDefinition);
        assert!(f.child(SyntaxKind::ForallBlock).is_some());
    }

    #[test]
    fn external_function_definition_shape() {
        let src = "external fn ideal_hash(b: [u8;100]) -> [u8;32];";
        assert_clean(src);
        let e = first(src, SyntaxKind::ExternalFunctionDefinition);
        assert_eq!(
            e.child(SyntaxKind::Identifier).map(|n| n.text(src)),
            Some("ideal_hash")
        );
        assert!(e.child(SyntaxKind::ArgumentList).is_some());
    }

    #[test]
    fn external_function_with_bare_type_args() {
        let src = "external fn sub(i32, i32) -> i32;";
        assert_clean(src);
        let e = first(src, SyntaxKind::ExternalFunctionDefinition);
        let args = e.child(SyntaxKind::ArgumentList).unwrap();
        // Two bare-type arguments: each is a TypeI32 directly in the list.
        assert_eq!(count_kind(args, SyntaxKind::TypeI32), 2);
    }

    #[test]
    fn struct_definition_with_field_and_method() {
        let src = "struct identity { field : T; fn getField() -> T { return field; } }";
        assert_clean(src);
        let s = first(src, SyntaxKind::StructDefinition);
        assert_eq!(count_kind(&s, SyntaxKind::StructField), 1);
        assert_eq!(count_kind(&s, SyntaxKind::FunctionDefinition), 1);
    }

    #[test]
    fn struct_field_shape() {
        let src = "struct s { numSlices : i32; }";
        assert_clean(src);
        let field = first(src, SyntaxKind::StructField);
        assert_eq!(
            field.child(SyntaxKind::Identifier).map(|n| n.text(src)),
            Some("numSlices")
        );
        assert!(field.child(SyntaxKind::TypeI32).is_some());
    }

    #[test]
    fn enum_definition_shape() {
        let src = "enum Arch { Wasm, Evm }";
        assert_clean(src);
        let e = first(src, SyntaxKind::EnumDefinition);
        assert_eq!(count_kind(&e, SyntaxKind::Identifier), 3); // name + 2 variants
    }

    #[test]
    fn constant_definition_shape() {
        let src = "const MAX : i64 = 1000;";
        assert_clean(src);
        let c = first(src, SyntaxKind::ConstantDefinition);
        assert_eq!(
            c.child(SyntaxKind::Identifier).map(|n| n.text(src)),
            Some("MAX")
        );
        assert!(c.child(SyntaxKind::TypeI64).is_some());
        assert!(c.child(SyntaxKind::NumberLiteral).is_some());
    }

    #[test]
    fn type_definition_statement_shape() {
        let src = "type Address = u32;";
        assert_clean(src);
        let t = first(src, SyntaxKind::TypeDefinitionStatement);
        assert_eq!(
            t.child(SyntaxKind::Identifier).map(|n| n.text(src)),
            Some("Address")
        );
        assert!(t.child(SyntaxKind::TypeU32).is_some());
    }

    #[test]
    fn spec_definition_shape() {
        let src = "spec S { const a: i32 = 10; type T = u32; }";
        assert_clean(src);
        let s = first(src, SyntaxKind::SpecDefinition);
        assert_eq!(count_kind(&s, SyntaxKind::ConstantDefinition), 1);
        assert_eq!(count_kind(&s, SyntaxKind::TypeDefinitionStatement), 1);
    }

    #[test]
    fn empty_spec() {
        assert_clean("spec some_spec {}");
    }

    #[test]
    fn pub_spec_is_rejected_and_recovers() {
        // Specs take no visibility modifier. The stray `pub` is reported and
        // consumed, but the spec body still parses.
        let src = "pub spec S { const a: i32 = 10; }";
        let (root, errors) = parse(src);
        assert!(errors > 0, "expected a diagnostic for the stray `pub`");
        let s = find(&root, SyntaxKind::SpecDefinition)
            .expect("the spec should still be recognised");
        assert!(
            s.child(SyntaxKind::Visibility).is_some(),
            "the stray `pub` is consumed as a Visibility node"
        );
        assert_eq!(count_kind(s, SyntaxKind::ConstantDefinition), 1);
    }

    #[test]
    fn pub_struct_field_is_rejected_and_recovers() {
        // A field has no individual visibility. The stray `pub` is reported and
        // consumed, and the field itself still lands in the tree.
        let src = "struct S { pub x : i32; }";
        let (root, errors) = parse(src);
        assert!(errors > 0, "expected a diagnostic for the stray `pub`");
        let field = find(&root, SyntaxKind::StructField)
            .expect("the field should still be recognised");
        assert!(
            field.child(SyntaxKind::Visibility).is_some(),
            "the stray `pub` is consumed as a Visibility node"
        );
        assert_eq!(
            field.child(SyntaxKind::Identifier).map(|n| n.text(src)),
            Some("x")
        );
        assert!(field.child(SyntaxKind::TypeI32).is_some());
    }

    #[test]
    fn pub_method_is_still_a_method_not_a_field() {
        // `pub fn …` inside a struct is a method and must parse cleanly — the
        // field-vs-method disambiguation keys off the token after `pub`.
        let src = "struct S { pub fn g() { } }";
        assert_clean(src);
        let s = first(src, SyntaxKind::StructDefinition);
        assert_eq!(count_kind(&s, SyntaxKind::FunctionDefinition), 1);
        assert_eq!(count_kind(&s, SyntaxKind::StructField), 0);
    }

    // use directives

    #[test]
    fn use_path() {
        assert_clean("use inference::std::algorithms::sort;");
        let u = first(
            "use inference::std::algorithms::sort;",
            SyntaxKind::UseDirective,
        );
        assert_eq!(count_kind(&u, SyntaxKind::Identifier), 4);
    }

    #[test]
    fn use_path_with_braced_list() {
        let src = "use inference::std::algorithms::{sort, hash};";
        assert_clean(src);
        let u = first(src, SyntaxKind::UseDirective);
        assert_eq!(count_kind(&u, SyntaxKind::Identifier), 5);
    }

    #[test]
    fn use_from_simple_name() {
        let src = "use { sort, hash } from sorting;";
        assert_clean(src);
        let u = first(src, SyntaxKind::UseDirective);
        assert!(u.child(SyntaxKind::StringLiteral).is_none());
        // two imported types plus one module-ref segment
        assert_eq!(count_kind(&u, SyntaxKind::Identifier), 3);
    }

    #[test]
    fn use_from_path() {
        let src = "use { hash } from crypto::sha256;";
        assert_clean(src);
        let u = first(src, SyntaxKind::UseDirective);
        assert!(u.child(SyntaxKind::StringLiteral).is_none());
        // one imported type plus two module-ref segments
        assert_eq!(count_kind(&u, SyntaxKind::Identifier), 3);
    }

    #[test]
    fn pub_use_path_parses_clean() {
        let src = "pub use lib::arith;";
        assert_clean(src);
        let u = first(src, SyntaxKind::UseDirective);
        assert!(
            u.child(SyntaxKind::Visibility).is_some(),
            "the leading `pub` is a Visibility node"
        );
        assert_eq!(count_kind(&u, SyntaxKind::Identifier), 2);
    }

    #[test]
    fn pub_use_braced_parses_clean() {
        let src = "pub use a::b::{ x, y };";
        assert_clean(src);
        let u = first(src, SyntaxKind::UseDirective);
        assert!(u.child(SyntaxKind::Visibility).is_some());
        // a + b segments, plus x + y imported items.
        assert_eq!(count_kind(&u, SyntaxKind::Identifier), 4);
    }

    #[test]
    fn pub_use_from_parses_clean() {
        let src = "pub use { x } from M;";
        assert_clean(src);
        let u = first(src, SyntaxKind::UseDirective);
        assert!(u.child(SyntaxKind::Visibility).is_some());
    }

    #[test]
    fn glob_use_is_rejected_and_recovers() {
        // `use a::b::*;` has no grammar support: the parser reports the glob and
        // skips to the `;`, then the following item still parses cleanly.
        let src = "use a::b::*; fn f() { }";
        let (root, errors) = parse(src);
        assert!(errors > 0, "expected a diagnostic for the glob import");
        assert_eq!(root.kind, SyntaxKind::SourceFile);
        assert!(
            find(&root, SyntaxKind::FunctionDefinition).is_some(),
            "the following item must still parse after glob recovery:\n{}",
            tree(src)
        );
    }

    #[test]
    fn pub_glob_use_is_rejected_and_recovers() {
        let src = "pub use math::*; fn f() { }";
        let (root, errors) = parse(src);
        assert!(errors > 0);
        assert!(find(&root, SyntaxKind::FunctionDefinition).is_some());
    }

    // use directives: #63 matrix (CST)
    //
    // The smoke tests above cover one happy case per `pub`/glob form. The
    // matrix below broadens coverage across every path depth, every glob
    // position, exact diagnostic wording, exact error counts (no cascade), and
    // recovery quality (subsequent top-level items still parse).
    // Lowering-level `vis`/segment assertions live in `lower.rs`.

    #[test]
    fn single_segment_use_parses_clean() {
        // A brace-free single-segment `use math;` names a file. It must parse
        // with exactly one path identifier.
        let src = "use math;";
        assert_clean(src);
        let u = first(src, SyntaxKind::UseDirective);
        assert!(u.child(SyntaxKind::Visibility).is_none());
        assert_eq!(count_kind(&u, SyntaxKind::Identifier), 1);
    }

    #[test]
    fn pub_single_segment_use_parses_clean() {
        let src = "pub use math;";
        assert_clean(src);
        let u = first(src, SyntaxKind::UseDirective);
        assert!(u.child(SyntaxKind::Visibility).is_some());
        assert_eq!(count_kind(&u, SyntaxKind::Identifier), 1);
    }

    #[test]
    fn deep_path_use_parses_clean() {
        // A 5-segment path stresses the `::`-segment loop well past the 2/3
        // segments the smoke tests use.
        let src = "use a::b::c::d::e;";
        assert_clean(src);
        let u = first(src, SyntaxKind::UseDirective);
        assert_eq!(count_kind(&u, SyntaxKind::Identifier), 5);
    }

    #[test]
    fn pub_deep_path_braced_items_parse_clean() {
        // `pub` + a multi-segment path + a multi-item brace list together: the
        // leading `pub` must not perturb the segment/imported-item split.
        let src = "pub use a::b::c::{ x, y, z };";
        assert_clean(src);
        let u = first(src, SyntaxKind::UseDirective);
        assert!(u.child(SyntaxKind::Visibility).is_some());
        // 3 path segments + 3 imported items.
        assert_eq!(count_kind(&u, SyntaxKind::Identifier), 6);
    }

    #[test]
    fn use_single_braced_item_parses_clean() {
        // The single-item import form `use a::{b};`: braces always name
        // items, even when there is exactly one.
        let src = "use a::{ b };";
        assert_clean(src);
        let u = first(src, SyntaxKind::UseDirective);
        assert_eq!(count_kind(&u, SyntaxKind::Identifier), 2);
    }

    #[test]
    fn use_trailing_comma_in_braces_parses_clean() {
        // A trailing comma in the import list is tolerated by `imported_type_list`
        // (the loop breaks on `}` after a comma). Pin this as accepted behavior.
        let src = "use a::{ x, y, };";
        assert_clean(src);
        let u = first(src, SyntaxKind::UseDirective);
        // 1 segment + 2 imported items (the trailing comma adds no identifier).
        assert_eq!(count_kind(&u, SyntaxKind::Identifier), 3);
    }

    #[test]
    fn use_empty_braces_parses_clean_with_no_items() {
        // `use a::b::{};` — empty braces. Current behavior: parses cleanly with
        // zero imported items (no diagnostic). Asserted as-is per the
        // CONTRIBUTING rule on pinning current behavior; whether an empty
        // import list is rejected is a type-checker concern, not the parser's.
        let src = "use a::b::{};";
        assert_clean(src);
        let u = first(src, SyntaxKind::UseDirective);
        // 2 path segments, no imported-item identifiers.
        assert_eq!(count_kind(&u, SyntaxKind::Identifier), 2);
    }

    // -- glob rejection at every depth; exact message; exact count; recovery --

    #[test]
    fn glob_use_bare_star_exact_message_single_error() {
        // `use *;` — a glob with no path at all. Exactly one diagnostic with the
        // educational wording; no cascade.
        let (_root, msgs) = parse_messages("use *;");
        assert_eq!(msgs, vec![GLOB_MESSAGE.to_string()]);
    }

    #[test]
    fn glob_use_one_segment_exact_message_single_error() {
        let (_root, msgs) = parse_messages("use a::*;");
        assert_eq!(msgs, vec![GLOB_MESSAGE.to_string()]);
    }

    #[test]
    fn glob_use_deep_path_exact_message_single_error() {
        // Glob at depth 3 (`use a::b::c::*;`): the `::`-segment loop rejects the
        // `*` after the final `::`, still a single error.
        let (_root, msgs) = parse_messages("use a::b::c::*;");
        assert_eq!(msgs, vec![GLOB_MESSAGE.to_string()]);
    }

    #[test]
    fn pub_glob_use_consumes_pub_with_single_glob_error() {
        // `pub use a::b::*;` — the leading `pub` is consumed cleanly as a
        // Visibility node and only the glob is reported: exactly one error, not
        // a `pub`-plus-glob cascade.
        let (root, msgs) = parse_messages("pub use a::b::*;");
        assert_eq!(msgs, vec![GLOB_MESSAGE.to_string()]);
        let u = find(&root, SyntaxKind::UseDirective).expect("use directive node");
        assert!(
            u.child(SyntaxKind::Visibility).is_some(),
            "the leading `pub` is still consumed as a Visibility node:\n{}",
            tree("pub use a::b::*;")
        );
    }

    #[test]
    fn glob_use_without_semicolon_recovers_at_item_anchor() {
        // `use a::b::*` with no trailing `;`: recovery must stop at the next
        // ITEM_RECOVERY anchor (`fn`) rather than swallowing it, so the function
        // still parses. Exactly one diagnostic (the glob); the missing `;` does
        // not add a cascade because recovery short-circuits at the anchor.
        let src = "use a::b::* fn f() { }";
        let (root, msgs) = parse_messages(src);
        assert_eq!(msgs, vec![GLOB_MESSAGE.to_string()]);
        assert!(
            find(&root, SyntaxKind::FunctionDefinition).is_some(),
            "the following function must survive glob recovery:\n{}",
            tree(src)
        );
    }

    #[test]
    fn glob_use_at_eof_terminates_with_single_error() {
        // `use a::b::*` truncated at EOF: recovery hits `at_eof()` immediately,
        // still exactly one diagnostic, and the parser terminates (reaching this
        // assertion proves it did) with a SourceFile root.
        let (root, msgs) = parse_messages("use a::b::*");
        assert_eq!(msgs, vec![GLOB_MESSAGE.to_string()]);
        assert_eq!(root.kind, SyntaxKind::SourceFile);
    }

    #[test]
    fn glob_use_followed_by_struct_recovers_and_parses_struct() {
        // Recovery quality with a richer following item than a bare `fn`: a
        // struct definition after the glob must parse intact.
        let src = "use a::*; struct P { x: i32; }";
        let (root, msgs) = parse_messages(src);
        assert_eq!(msgs, vec![GLOB_MESSAGE.to_string()]);
        let s = find(&root, SyntaxKind::StructDefinition).expect("struct survives recovery");
        assert_eq!(count_kind(s, SyntaxKind::StructField), 1);
    }

    #[test]
    fn glob_use_between_two_good_use_directives() {
        // A glob wedged between two valid `use`s: the good directives on either
        // side parse, and only the middle glob errors (one error total).
        let src = "use first; use mid::*; use last;";
        let (root, msgs) = parse_messages(src);
        assert_eq!(msgs, vec![GLOB_MESSAGE.to_string()]);
        // Three UseDirective nodes survive (the two clean ones plus the rejected
        // one, which still completes its node).
        assert_eq!(count_kind(&root, SyntaxKind::UseDirective), 3);
    }

    #[test]
    fn use_trailing_colon_colon_without_item_recovers() {
        // `use a::b::` with nothing after the final `::`: not a glob, but a
        // missing path/item. Current behavior is two diagnostics (a missing
        // identifier and the missing `;`); pin that count and that the parser
        // still terminates with a SourceFile root.
        let (root, msgs) = parse_messages("use a::b::");
        assert_eq!(
            msgs,
            vec!["expected an identifier".to_string(), "expected Semi".to_string()],
            "trailing `::` without an item:\n{}",
            tree("use a::b::")
        );
        assert_eq!(root.kind, SyntaxKind::SourceFile);
    }

    // -- pub spec rejection: exact message, body integrity, following items

    #[test]
    fn pub_spec_exact_message_single_error() {
        // The `pub spec` diagnostic is reported exactly once at the CST level; the
        // `pub` is then consumed and the spec body parses, so no parse cascade
        // follows. This checks parsing only; the parse+lower variant below guards
        // against a lowering cascade re-reporting the same invalid input.
        let (_root, msgs) = parse_messages("pub spec S { const a: i32 = 10; type T = u32; }");
        assert_eq!(
            msgs,
            vec!["specs take no visibility modifier; they are stripped before codegen".to_string()]
        );
    }

    #[test]
    fn pub_spec_single_error_through_lowering() {
        // The full parse+lower pipeline must surface exactly one diagnostic for a
        // `pub spec`. The stray `pub` is a `Visibility` node child of the spec; a
        // naive "skip the first node child" loop over the spec body would re-lower
        // the name `Identifier` as if it were a definition and emit a spurious
        // second diagnostic. Drive the public `parse` (which runs lowering) so a
        // CST-only check cannot mask that cascade.
        let parsed = crate::parse("pub spec S { const a: i32 = 10; type T = u32; }");
        assert_eq!(
            parsed.errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>(),
            vec!["specs take no visibility modifier; they are stripped before codegen".to_string()]
        );
    }

    #[test]
    fn pub_spec_body_items_survive_recovery() {
        // After the stray `pub`, every item inside the spec body must still land
        // in the tree: a const, a type alias and a function.
        let src = "pub spec S { const a: i32 = 1; type T = u32; fn h() { } }";
        let (root, _errors) = parse(src);
        let s = find(&root, SyntaxKind::SpecDefinition).expect("spec survives the stray pub");
        assert_eq!(count_kind(s, SyntaxKind::ConstantDefinition), 1);
        assert_eq!(count_kind(s, SyntaxKind::TypeDefinitionStatement), 1);
        assert_eq!(count_kind(s, SyntaxKind::FunctionDefinition), 1);
    }

    #[test]
    fn pub_spec_followed_by_top_level_item_still_parses() {
        // A top-level definition after a `pub spec` must parse: the spec error
        // does not leak into the following item.
        let src = "pub spec S { } fn after() { }";
        let (root, msgs) = parse_messages(src);
        assert_eq!(
            msgs,
            vec!["specs take no visibility modifier; they are stripped before codegen".to_string()]
        );
        assert!(
            find(&root, SyntaxKind::FunctionDefinition).is_some(),
            "the item after the pub spec must parse:\n{}",
            tree(src)
        );
    }

    // -- pub field rejection: exact message, AST integrity, mixed members

    #[test]
    fn pub_field_exact_message_single_error() {
        let (_root, msgs) = parse_messages("struct S { pub x : i32; }");
        assert_eq!(msgs, vec!["fields inherit visibility from their struct".to_string()]);
    }

    #[test]
    fn multiple_pub_fields_report_one_error_each() {
        // Two `pub` fields produce exactly two diagnostics — one per field, no
        // cascade — and both fields still land in the struct.
        let src = "struct S { pub x : i32; pub y : i32; }";
        let (root, msgs) = parse_messages(src);
        assert_eq!(
            msgs,
            vec![
                "fields inherit visibility from their struct".to_string(),
                "fields inherit visibility from their struct".to_string(),
            ]
        );
        let s = find(&root, SyntaxKind::StructDefinition).expect("struct node");
        assert_eq!(count_kind(s, SyntaxKind::StructField), 2);
    }

    #[test]
    fn struct_mixes_pub_field_normal_field_method_and_pub_method() {
        // A single struct exercising every member-disambiguation branch: a `pub`
        // field (rejected, kept), a normal field, a method, and a `pub` method
        // (which stays a method). Exactly one diagnostic, for the `pub` field.
        let src = "struct S { pub a : i32; b : i32; fn m(self) { } pub fn p(self) { } }";
        let (root, msgs) = parse_messages(src);
        assert_eq!(msgs, vec!["fields inherit visibility from their struct".to_string()]);
        let s = find(&root, SyntaxKind::StructDefinition).expect("struct node");
        // Two fields (the pub one and the normal one).
        assert_eq!(count_kind(s, SyntaxKind::StructField), 2);
        // Two methods (the plain one and the pub one).
        assert_eq!(count_kind(s, SyntaxKind::FunctionDefinition), 2);
    }

    #[test]
    fn pub_field_struct_followed_by_top_level_item_parses() {
        let src = "struct S { pub x : i32; } fn after() { }";
        let (root, msgs) = parse_messages(src);
        assert_eq!(msgs, vec!["fields inherit visibility from their struct".to_string()]);
        let fns = count_kind(&root, SyntaxKind::FunctionDefinition);
        assert_eq!(fns, 1, "the trailing fn must parse:\n{}", tree(src));
    }

    // types

    #[test]
    fn primitive_types() {
        for (kw, kind) in [
            ("i8", SyntaxKind::TypeI8),
            ("i16", SyntaxKind::TypeI16),
            ("i32", SyntaxKind::TypeI32),
            ("i64", SyntaxKind::TypeI64),
            ("u8", SyntaxKind::TypeU8),
            ("u16", SyntaxKind::TypeU16),
            ("u32", SyntaxKind::TypeU32),
            ("u64", SyntaxKind::TypeU64),
            ("bool", SyntaxKind::TypeBool),
        ] {
            let src = format!("fn f() {{ let x : {kw} = 0; }}");
            assert_clean(&src);
            let (root, _) = parse_to_cst(&src);
            assert!(find(&root, kind).is_some(), "missing {kind:?} for {kw}");
        }
    }

    #[test]
    fn unit_type() {
        let src = "fn f() { let a : () = (); }";
        assert_clean(src);
        let (root, _) = parse_to_cst(src);
        assert!(find(&root, SyntaxKind::TypeUnit).is_some());
    }

    #[test]
    fn array_type_with_length() {
        let src = "fn f() { let a : [i32; 3] = [1, 2, 3]; }";
        assert_clean(src);
        let arr = first(src, SyntaxKind::TypeArray);
        assert!(arr.child(SyntaxKind::TypeI32).is_some());
        assert!(arr.child(SyntaxKind::NumberLiteral).is_some());
    }

    #[test]
    fn array_type_without_length() {
        let src = "fn f() { let mem : [i32]; }";
        assert_clean(src);
        assert!(find(&parse_to_cst(src).0, SyntaxKind::TypeArray).is_some());
    }

    #[test]
    fn nested_array_type() {
        let src = "fn f() { let a : [[[u32]]]; }";
        assert_clean(src);
        assert_eq!(count_kind(&parse_to_cst(src).0, SyntaxKind::TypeArray), 3);
    }

    #[test]
    fn fn_type() {
        let src = "fn f() { let plus: fn(i32, i32) -> i32 = add; }";
        assert_clean(src);
        let t = first(src, SyntaxKind::TypeFn);
        assert!(t.child(SyntaxKind::ArgumentList).is_some());
        assert!(t.child(SyntaxKind::TypeI32).is_some());
    }

    #[test]
    fn generic_name_type() {
        let src = "fn f() { let x : Vec i32'; }";
        assert_clean(src);
        let g = first(src, SyntaxKind::GenericName);
        assert_eq!(
            g.child(SyntaxKind::Identifier).map(|n| n.text(src)),
            Some("Vec")
        );
        assert!(g.child(SyntaxKind::TypeArgumentList).is_some());
    }

    #[test]
    fn type_qualified_name() {
        let src = "fn f() { let x : someNamespace::String; }";
        assert_clean(src);
        let q = first(src, SyntaxKind::TypeQualifiedName);
        assert_eq!(count_kind(&q, SyntaxKind::Identifier), 2);
    }

    #[test]
    fn bare_name_type_is_identifier() {
        let src = "fn f(a: Address) -> Address { return a; }";
        assert_clean(src);
        // The argument type `Address` is a plain Identifier node.
        let args = first(src, SyntaxKind::ArgumentList);
        assert!(find(&args, SyntaxKind::Identifier).is_some());
    }

    // statements

    #[test]
    fn variable_definition_with_value() {
        let src = "fn f() { let mut i : i32 = -10; }";
        assert_clean(src);
        let v = first(src, SyntaxKind::VariableDefinitionStatement);
        assert!(v.child(SyntaxKind::MutKeyword).is_some());
        assert!(v.child(SyntaxKind::TypeI32).is_some());
        assert!(v.child(SyntaxKind::NumberLiteral).is_some());
    }

    #[test]
    fn variable_definition_without_value() {
        let src = "fn f() { let mem : [i32]; }";
        assert_clean(src);
        let v = first(src, SyntaxKind::VariableDefinitionStatement);
        assert!(v.child(SyntaxKind::MutKeyword).is_none());
    }

    #[test]
    fn return_with_and_without_expression() {
        assert_clean("fn f() -> i32 { return 0; }");
        assert_clean("fn f() { return; }");
        assert_clean("fn f() -> () { return (); }");
    }

    #[test]
    fn assign_statement() {
        let src = "fn f() { a = b + 1; }";
        assert_clean(src);
        assert!(find(&parse_to_cst(src).0, SyntaxKind::AssignStatement).is_some());
    }

    #[test]
    fn expression_statement() {
        let src = "fn f() { foo(1, 2); }";
        assert_clean(src);
        assert!(find(&parse_to_cst(src).0, SyntaxKind::ExpressionStatement).is_some());
    }

    #[test]
    fn member_access_assign() {
        let src = "fn f() { self.type = ABC; }";
        assert_clean(src);
        let a = first(src, SyntaxKind::AssignStatement);
        assert!(find(&a, SyntaxKind::MemberAccessExpression).is_some());
    }

    #[test]
    fn loop_with_condition() {
        let src = "fn f() { loop 10 { break; } }";
        assert_clean(src);
        let l = first(src, SyntaxKind::LoopStatement);
        assert!(l.child(SyntaxKind::NumberLiteral).is_some());
        assert!(l.child(SyntaxKind::Block).is_some());
    }

    #[test]
    fn loop_without_condition() {
        let src = "fn f() { loop { break; } }";
        assert_clean(src);
        let l = first(src, SyntaxKind::LoopStatement);
        assert!(l.child(SyntaxKind::NumberLiteral).is_none());
        assert!(l.child(SyntaxKind::Block).is_some());
    }

    #[test]
    fn if_else_chain() {
        let src = "fn f() { if a { } else if b { } else { } }";
        assert_clean(src);
        let i = first(src, SyntaxKind::IfStatement);
        assert_eq!(count_kind(&i, SyntaxKind::Block), 3);
    }

    #[test]
    fn assert_statement() {
        let src = "fn f() { assert a < 0; }";
        assert_clean(src);
        assert!(find(&parse_to_cst(src).0, SyntaxKind::AssertStatement).is_some());
    }

    #[test]
    fn break_statement() {
        let src = "fn f() { loop { break; } }";
        assert_clean(src);
        assert!(find(&parse_to_cst(src).0, SyntaxKind::BreakStatement).is_some());
    }

    #[test]
    fn const_and_type_statements_in_block() {
        let src = "fn f() { const d: i32 = 10; type T = u32; }";
        assert_clean(src);
        let (root, _) = parse_to_cst(src);
        assert!(find(&root, SyntaxKind::ConstantDefinition).is_some());
        assert!(find(&root, SyntaxKind::TypeDefinitionStatement).is_some());
    }

    #[test]
    fn nondet_blocks() {
        for (kw, kind) in [
            ("assume", SyntaxKind::AssumeBlock),
            ("forall", SyntaxKind::ForallBlock),
            ("exists", SyntaxKind::ExistsBlock),
            ("unique", SyntaxKind::UniqueBlock),
        ] {
            let src = format!("fn f() {{ {kw} {{ break; }} }}");
            assert_clean(&src);
            let (root, _) = parse_to_cst(&src);
            let block = find(&root, kind).unwrap();
            assert!(
                block.child(SyntaxKind::Block).is_some(),
                "{kw} wraps a Block"
            );
        }
    }

    // expressions: literals & atoms

    #[test]
    fn number_literal_atom() {
        let src = "fn f() { x = 42; }";
        assert_clean(src);
        assert!(find(&parse_to_cst(src).0, SyntaxKind::NumberLiteral).is_some());
    }

    // -- malformed numeric literals: one teaching diagnostic, no cascade --
    //
    // The lexer's number scanner stops at the first non-digit, so `16i64`,
    // `1_000` and `0x1F` each arrive as a `Number` glued to an identifier run.
    // Every case below must produce EXACTLY one diagnostic: the pre-#219
    // behavior was "expected Semi" + "expected an expression" plus a stray
    // `Error` node, and `1_000` silently parsed as the literal `1`.

    #[test]
    fn number_type_suffix_reports_one_teaching_error() {
        let (root, msgs) = parse_messages("fn f() { let x: i64 = 16i64; }");
        assert_eq!(msgs, vec![suffix_message("i64")]);
        assert!(
            !has_error_node(&root),
            "the suffix is consumed into the literal, not left as an Error node:\n{}",
            tree("fn f() { let x: i64 = 16i64; }")
        );
    }

    #[test]
    fn number_underscore_type_suffix_reports_one_teaching_error() {
        // `16_i64` splits as Number(16) + Ident(_i64); removing `_i64` is what
        // leaves a well-formed literal, so the message names the whole tail.
        let (_root, msgs) = parse_messages("fn f() { let x: i64 = 16_i64; }");
        assert_eq!(msgs, vec![suffix_message("_i64")]);
    }

    #[test]
    fn every_suffix_shaped_tail_gets_the_same_message() {
        // None of these name an Inference type, but all are suffix-shaped: each
        // must get the suffix message, never the decimal-digits one, so the
        // diagnostic never implies `i64` is recognized while `i128` is garbage —
        // and so `16usize`, the likeliest habit from Rust, is not told about
        // separators and radix prefixes it does not contain.
        for (src, tail) in [
            ("fn f() { let x: i64 = 5i128; }", "i128"),
            ("fn f() { let x: i64 = 5usize; }", "usize"),
            ("fn f() { let x: i64 = 5isize; }", "isize"),
            ("fn f() { let x: i64 = 5u; }", "u"),
            ("fn f() { let x: i64 = 5i; }", "i"),
            ("fn f() { let x: i64 = 5_u32; }", "_u32"),
            ("fn f() { let x: i64 = 16i64i64; }", "i64i64"),
        ] {
            let (_root, msgs) = parse_messages(src);
            assert_eq!(msgs, vec![suffix_message(tail)], "for {src:?}");
        }
    }

    #[test]
    fn suffix_diagnostic_points_at_the_suffix() {
        // The span must cover exactly the text the message says to remove.
        let src = "fn f() { let x: i64 = 16i64; }";
        let (_root, errors) = parse_to_cst(src);
        assert_eq!(errors.len(), 1, "expected one error, got {errors:?}");
        let span = errors[0].span;
        assert_eq!(
            &src[span.offset_start as usize..span.offset_end as usize],
            "i64"
        );
    }

    #[test]
    fn non_decimal_diagnostic_points_at_the_tail() {
        let src = "fn f() { let x: i32 = 1_000; }";
        let (_root, errors) = parse_to_cst(src);
        assert_eq!(errors.len(), 1, "expected one error, got {errors:?}");
        let span = errors[0].span;
        assert_eq!(
            &src[span.offset_start as usize..span.offset_end as usize],
            "_000"
        );
    }

    #[test]
    fn digit_separator_reports_one_decimal_digits_error() {
        // The silent-token-split trap: without a diagnostic this parses as `1`.
        let (_root, msgs) = parse_messages("fn f() { let x: i32 = 1_000; }");
        assert_eq!(msgs, vec![non_decimal_message("_000")]);
    }

    #[test]
    fn non_suffix_tails_report_one_decimal_digits_error() {
        // Everything whose tail starts with neither `i` nor `u`: radix prefixes,
        // an exponent, a lone trailing `_` (which lexes as `Underscore`, not
        // `Ident`), and plain adjacent garbage.
        for (src, tail) in [
            ("fn f() { let x: i32 = 0x1F; }", "x1F"),
            ("fn f() { let x: i32 = 0b01; }", "b01"),
            ("fn f() { let x: i32 = 0o17; }", "o17"),
            ("fn f() { let x: i32 = 1e10; }", "e10"),
            ("fn f() { let x: i32 = 16f32; }", "f32"),
            ("fn f() { let x: i32 = 1_; }", "_"),
            ("fn f() { let x: i32 = 16true; }", "true"),
        ] {
            let (_root, msgs) = parse_messages(src);
            assert_eq!(msgs, vec![non_decimal_message(tail)], "for {src:?}");
        }
    }

    #[test]
    fn glued_negative_literal_with_suffix_reports_one_error() {
        // `-9223372036854775808` lexes as a single Number (the `-` is glued), so
        // the suffix detection must still see the tail after it.
        let src = "fn f() { let x: i64 = -9223372036854775808i64; }";
        let (_root, msgs) = parse_messages(src);
        assert_eq!(msgs, vec![suffix_message("i64")]);
    }

    #[test]
    fn malformed_literal_still_yields_a_number_literal_node() {
        // The resilient path: IDE consumers must still find a usable literal.
        let src = "fn f() { let x: i64 = 16i64; }";
        let (root, _msgs) = parse_messages(src);
        let lit = find(&root, SyntaxKind::NumberLiteral).expect("literal node survives");
        assert_eq!(
            lit.text(src),
            "16i64",
            "the node spans what the author wrote, tail included"
        );
        let digits = lit
            .child_token(SyntaxKind::Number)
            .expect("the digits stay a `Number` token inside the node");
        assert_eq!(
            digits.text(src),
            "16",
            "lowering reads this token, so it must hold the digits alone"
        );
    }

    #[test]
    fn statements_after_a_malformed_literal_stay_siblings() {
        // The anti-cascade claim stated structurally rather than by error count:
        // the following statements must still parse as siblings of the broken
        // one, not get swallowed into its recovery.
        let src = "fn f() { let x: i64 = 16i64; let y: i64 = 7; let z: i64 = 1; }";
        let (root, msgs) = parse_messages(src);
        assert_eq!(msgs, vec![suffix_message("i64")]);
        assert_eq!(
            count_kind(&root, SyntaxKind::VariableDefinitionStatement),
            3,
            "all three lets survive:\n{}",
            tree(src)
        );
    }

    #[test]
    fn malformed_literal_is_rejected_in_every_expression_position() {
        // `number_literal` is the atom rule, so one repair covers every position
        // a literal can appear in. Pin the matrix so a future rewrite that moves
        // the check to a caller cannot quietly lose most of them.
        for src in [
            "fn f() { let x: i64 = 16i64; }",
            "const C: i64 = 16i64;",
            "fn f() { x = 16i64; }",
            "fn f() { return 16i64; }",
            "fn f() { g(16i64); }",
            "fn f() { let s: S = S { a: 16i64 }; }",
            "fn f() { let a: [i64; 1] = [16i64]; }",
            "fn f() { x = a + 16i64; }",
            "fn f() { x = (16i64); }",
            "fn f() { x = a[16i64]; }",
            // `-16` is a single Number token, so the tail follows the digits of
            // a negative literal just as it does a positive one.
            "fn f() { x = -16i64; }",
        ] {
            let (_root, msgs) = parse_messages(src);
            assert_eq!(msgs, vec![suffix_message("i64")], "for {src:?}");
        }
    }

    #[test]
    fn several_malformed_literals_report_one_error_each() {
        // Recovery quality: one diagnostic per literal across a spec body, a
        // `let` initializer and a binary operand — no cascade, no
        // cross-contamination, and the clean function after them still parses.
        let src = "spec S { fn p(a: i64) -> bool { return a > 1_000; } } \
                   pub fn f(a: i64) -> i64 { let b: i64 = 5i128; return a + 0x1F; } \
                   pub fn g() -> i32 { return 7; }";
        let (root, msgs) = parse_messages(src);
        assert_eq!(
            msgs,
            vec![
                non_decimal_message("_000"),
                suffix_message("i128"),
                non_decimal_message("x1F"),
            ]
        );
        assert_eq!(
            count_kind(&root, SyntaxKind::FunctionDefinition),
            3,
            "every function survives:\n{}",
            tree(src)
        );
    }

    #[test]
    fn malformed_literal_in_array_size_reports_one_error() {
        // Array sizes reuse `number_literal`, so the repair covers them too.
        let (_root, msgs) = parse_messages("fn f() { let x: [i32; 1_0] = a; }");
        assert_eq!(msgs, vec![non_decimal_message("_0")]);
    }

    #[test]
    fn spaced_identifier_after_number_is_not_a_malformed_literal() {
        // Not joint: `16 i64` is two tokens the author separated. Detection is
        // adjacency-based, so this keeps its pre-existing parse — and the two
        // messages it produces are exactly the cascade the glued case above
        // replaces with one teaching diagnostic.
        let (_root, msgs) = parse_messages("fn f() { let x: i64 = 16 i64; }");
        assert_eq!(
            msgs,
            vec![
                "expected Semi".to_string(),
                "expected an expression".to_string(),
            ],
            "spaced tokens keep today's behavior"
        );
    }

    #[test]
    fn well_formed_number_literals_stay_clean() {
        // Negative control: adjacency to a non-word token is untouched. The
        // glued-operator cases are what `is_identifier_run` alone rules out.
        assert_clean("fn f() { let x: i32 = 16; }");
        assert_clean("fn f() { let x: [i32; 4] = a; }");
        assert_clean("fn f() { g(16); }");
        assert_clean("fn f() { let x: i32 = 16+1; }");
        assert_clean("fn f() { let x: i32 = 16*2; }");
        assert_clean("fn f() { let x: i32 = 16<<2; }");
        assert_clean("fn f() { let x: i32 = a[0]; }");
    }

    #[test]
    fn bool_literals() {
        assert_clean("fn f() { let a: bool = true; let b: bool = false; }");
        let (root, _) = parse_to_cst("fn f() { let a: bool = true; }");
        assert!(find(&root, SyntaxKind::BoolLiteral).is_some());
    }

    #[test]
    fn string_literal_atom() {
        let src = "fn f() { print(\"hello\"); }";
        assert_clean(src);
        assert!(find(&parse_to_cst(src).0, SyntaxKind::StringLiteral).is_some());
    }

    #[test]
    fn array_literal_atom() {
        let src = "fn f() { x = [1, 2, 3]; }";
        assert_clean(src);
        let a = first(src, SyntaxKind::ArrayLiteral);
        assert_eq!(count_kind(&a, SyntaxKind::NumberLiteral), 3);
    }

    #[test]
    fn unit_literal_atom() {
        let src = "fn f() { x = (); }";
        assert_clean(src);
        assert!(find(&parse_to_cst(src).0, SyntaxKind::UnitLiteral).is_some());
    }

    #[test]
    fn uzumaki_atom() {
        let src = "fn f() { let a: i32 = @; }";
        assert_clean(src);
        assert!(find(&parse_to_cst(src).0, SyntaxKind::UzumakiKeyword).is_some());
    }

    #[test]
    fn parenthesized_expression() {
        let src = "fn f() { x = (1 + 2) * 3; }";
        assert_clean(src);
        assert!(find(&parse_to_cst(src).0, SyntaxKind::ParenthesizedExpression).is_some());
    }

    #[test]
    fn struct_expression() {
        let src = "fn f() { let x : S = S { a: 1, b: 2 }; }";
        assert_clean(src);
        let s = first(src, SyntaxKind::StructExpression);
        // name + two field names = three Identifier nodes at least.
        assert!(count_kind(&s, SyntaxKind::Identifier) >= 3);
    }

    #[test]
    fn single_segment_qualified_struct_literal() {
        // `geo::Point { .. }` is a struct literal whose name is a
        // `TypeQualifiedName` head.
        let src = "fn f() { let p : Point = geo::Point { x: 1, y: 2 }; }";
        assert_clean(src);
        let s = first(src, SyntaxKind::StructExpression);
        let head = s.node_children().next().expect("struct head node");
        assert_eq!(head.kind, SyntaxKind::TypeQualifiedName);
        assert_eq!(head.text(src), "geo::Point");
    }

    #[test]
    fn multi_segment_qualified_struct_literal() {
        // Previously-failing case (#63): `lib::geo::Point { .. }` now parses into
        // a struct literal whose head is the whole `::` chain.
        let src = "fn f() { let p : Point = lib::geo::Point { x: 1, y: 2 }; }";
        assert_clean(src);
        let s = first(src, SyntaxKind::StructExpression);
        let head = s.node_children().next().expect("struct head node");
        assert_eq!(head.kind, SyntaxKind::TypeMemberAccessExpression);
        assert_eq!(head.text(src), "lib::geo::Point");
    }

    #[test]
    fn deep_qualified_struct_literal_with_empty_body() {
        let src = "fn f() { let p : Point = a::b::c::Point { }; }";
        assert_clean(src);
        let s = first(src, SyntaxKind::StructExpression);
        let head = s.node_children().next().expect("struct head node");
        assert_eq!(head.kind, SyntaxKind::TypeMemberAccessExpression);
        assert_eq!(head.text(src), "a::b::c::Point");
    }

    #[test]
    fn qualified_call_is_not_a_struct_literal() {
        // `a::b::c(...)` is a call, not a struct literal: no `StructExpression`.
        let src = "fn f() { x = a::b::c(); }";
        assert_clean(src);
        let (root, _) = parse_to_cst(src);
        assert!(find(&root, SyntaxKind::StructExpression).is_none());
        assert!(find(&root, SyntaxKind::FunctionCallExpression).is_some());
    }

    #[test]
    fn qualified_variant_access_is_not_a_struct_literal() {
        // `a::b::C` (e.g. an enum variant) stays a type-member access chain.
        let src = "fn f() { x = a::b::C; }";
        assert_clean(src);
        let (root, _) = parse_to_cst(src);
        assert!(find(&root, SyntaxKind::StructExpression).is_none());
        assert_eq!(
            count_kind(&root, SyntaxKind::TypeMemberAccessExpression),
            1
        );
    }

    #[test]
    fn qualified_struct_literal_suppressed_in_if_head() {
        // In an `if` head the `{` opens the body, even after a `::` chain: no
        // struct literal is parsed (mirrors the bare/single-segment behaviour).
        let src = "fn f() { if a::b::Point { } }";
        assert_clean(src);
        let (root, _) = parse_to_cst(src);
        assert!(
            find(&root, SyntaxKind::StructExpression).is_none(),
            "if head must not greedily parse a qualified struct literal:\n{}",
            tree(src)
        );
        assert!(find(&root, SyntaxKind::IfStatement).is_some());
    }

    #[test]
    fn qualified_struct_literal_suppressed_in_loop_head() {
        let src = "fn f() { loop a::b::Cond { break; } }";
        assert_clean(src);
        assert!(find(&parse_to_cst(src).0, SyntaxKind::StructExpression).is_none());
    }

    // expressions: precedence & associativity

    #[test]
    fn add_binds_looser_than_mul() {
        // a + b * c  =>  Binary(+, a, Binary(*, b, c))
        let src = "fn f() { x = a + b * c; }";
        assert_clean(src);
        let outer = first(src, SyntaxKind::BinaryExpression);
        // The outer operator is `+`; its right child is another BinaryExpression.
        let inner = find(
            outer.node_children().nth(1).unwrap(),
            SyntaxKind::BinaryExpression,
        );
        assert!(inner.is_some(), "a + (b * c) nesting:\n{}", tree(src));
    }

    #[test]
    fn pow_is_right_associative() {
        // a ** b ** c  =>  Binary(**, a, Binary(**, b, c))
        let src = "fn f() { x = a ** b ** c; }";
        assert_clean(src);
        let outer = first(src, SyntaxKind::BinaryExpression);
        let right = outer.node_children().nth(1).unwrap();
        assert_eq!(
            right.kind,
            SyntaxKind::BinaryExpression,
            "right-assoc nesting:\n{}",
            tree(src)
        );
    }

    #[test]
    fn comparison_vs_equality() {
        // a < b == c  =>  Binary(==, Binary(<, a, b), c)
        let src = "fn f() { x = a < b == c; }";
        assert_clean(src);
        let outer = first(src, SyntaxKind::BinaryExpression);
        let left = outer.node_children().next().unwrap();
        assert_eq!(left.kind, SyntaxKind::BinaryExpression);
    }

    #[test]
    fn logical_or_vs_and() {
        // a || b && c  =>  Binary(||, a, Binary(&&, b, c))
        let src = "fn f() { x = a || b && c; }";
        assert_clean(src);
        let outer = first(src, SyntaxKind::BinaryExpression);
        let right = outer.node_children().nth(1).unwrap();
        assert_eq!(right.kind, SyntaxKind::BinaryExpression);
    }

    #[test]
    fn postfix_chain_binds_tighter_than_unary() {
        // -a.b()[0]  => PrefixUnary(-, Index(Call(Member(a, b)), 0))
        let src = "fn f() { x = -a.b()[0]; }";
        assert_clean(src);
        let pre = first(src, SyntaxKind::PrefixUnaryExpression);
        assert!(find(&pre, SyntaxKind::ArrayIndexAccessExpression).is_some());
        assert!(find(&pre, SyntaxKind::FunctionCallExpression).is_some());
        assert!(find(&pre, SyntaxKind::MemberAccessExpression).is_some());
    }

    #[test]
    fn type_member_access_chain() {
        // a::B::c
        let src = "fn f() { x = a::B::c; }";
        assert_clean(src);
        assert_eq!(
            count_kind(&parse_to_cst(src).0, SyntaxKind::TypeMemberAccessExpression),
            1
        );
        // a::B is a TypeQualifiedName, then ::c is a type-member access.
        assert!(find(&parse_to_cst(src).0, SyntaxKind::TypeQualifiedName).is_some());
    }

    #[test]
    fn call_with_named_and_positional_args() {
        let src = "fn f() { g = f(x: 1, 2); }";
        assert_clean(src);
        let call = first(src, SyntaxKind::FunctionCallExpression);
        assert_eq!(count_kind(&call, SyntaxKind::NumberLiteral), 2);
    }

    #[test]
    fn generic_name_in_expression() {
        let src = "fn f() { x = (Array u32')::new(); }";
        assert_clean(src);
        assert!(find(&parse_to_cst(src).0, SyntaxKind::GenericName).is_some());
        assert!(find(&parse_to_cst(src).0, SyntaxKind::TypeMemberAccessExpression).is_some());
    }

    #[test]
    fn parenthesized_changes_precedence() {
        let src = "fn f() { x = (a + b) * c; }";
        assert_clean(src);
        let outer = first(src, SyntaxKind::BinaryExpression);
        // Outer op is `*`; its left is a ParenthesizedExpression.
        let left = outer.node_children().next().unwrap();
        assert_eq!(left.kind, SyntaxKind::ParenthesizedExpression);
    }

    // disambiguations

    #[test]
    fn negative_number_is_single_literal() {
        // `-42` lexes as one Number → number_literal, no prefix unary.
        let src = "fn f() { let x: i32 = -42; }";
        assert_clean(src);
        let (root, _) = parse_to_cst(src);
        assert!(find(&root, SyntaxKind::NumberLiteral).is_some());
        assert!(
            find(&root, SyntaxKind::PrefixUnaryExpression).is_none(),
            "-42 must not be a prefix unary:\n{}",
            tree(src)
        );
        assert_eq!(
            find(&root, SyntaxKind::NumberLiteral).unwrap().text(src),
            "-42"
        );
    }

    #[test]
    fn spaced_minus_is_prefix_unary() {
        // `- 42` lexes as Minus then Number → prefix unary.
        let src = "fn f() { let x: i32 = - 42; }";
        assert_clean(src);
        let (root, _) = parse_to_cst(src);
        let pre = find(&root, SyntaxKind::PrefixUnaryExpression).unwrap();
        assert!(pre.child(SyntaxKind::UnaryMinus).is_some());
        assert!(find(pre, SyntaxKind::NumberLiteral).is_some());
    }

    #[test]
    fn double_negation_spaced() {
        // `-1 - -2` : Binary(-, -1, -2) with both operands negative literals.
        let src = "fn f() -> i32 { return -1 - -2; }";
        assert_clean(src);
        let b = first(src, SyntaxKind::BinaryExpression);
        assert_eq!(count_kind(&b, SyntaxKind::NumberLiteral), 2);
    }

    #[test]
    fn if_condition_is_not_struct_literal() {
        // `if cond { }` : the `{` opens the if body, not a struct literal on cond.
        let src = "fn f() { if number < 5 { } }";
        assert_clean(src);
        let (root, _) = parse_to_cst(src);
        assert!(
            find(&root, SyntaxKind::StructExpression).is_none(),
            "if condition must not parse a struct literal:\n{}",
            tree(src)
        );
        assert!(find(&root, SyntaxKind::IfStatement).is_some());
    }

    #[test]
    fn loop_condition_is_not_struct_literal() {
        let src = "fn f() { loop cond { break; } }";
        assert_clean(src);
        assert!(find(&parse_to_cst(src).0, SyntaxKind::StructExpression).is_none());
    }

    #[test]
    fn struct_literal_in_normal_context() {
        // In a `let` initializer (not a condition), `S { .. }` is a struct literal.
        let src = "fn f() { let env : Binding = Binding { a: 1, b: 2 }; }";
        assert_clean(src);
        assert!(find(&parse_to_cst(src).0, SyntaxKind::StructExpression).is_some());
    }

    #[test]
    fn argument_kinds() {
        let src = "fn f(self, mut x: i32, _: i32, y: i32) { }";
        assert_clean(src);
        let args = first(src, SyntaxKind::ArgumentList);
        assert_eq!(count_kind(&args, SyntaxKind::SelfReference), 1);
        assert_eq!(count_kind(&args, SyntaxKind::IgnoreArgument), 1);
        assert_eq!(count_kind(&args, SyntaxKind::ArgumentDeclaration), 2);
    }

    #[test]
    fn mut_self_argument() {
        let src = "struct S { fn g(mut self) { } }";
        assert_clean(src);
        let sr = first(src, SyntaxKind::SelfReference);
        assert!(sr.child(SyntaxKind::MutKeyword).is_some());
    }

    #[test]
    fn bare_type_argument() {
        let src = "external fn h(i32, bool) -> i32;";
        assert_clean(src);
        let args = first(src, SyntaxKind::ArgumentList);
        assert!(find(&args, SyntaxKind::TypeI32).is_some());
        assert!(find(&args, SyntaxKind::TypeBool).is_some());
    }

    #[test]
    fn contextual_keyword_as_member_name() {
        // `self.type = ABC;` uses `self` as a name base and `type` as a member
        // name; both are contextual keywords in identifier position.
        let src = "fn f() -> () { self.type = ABC; }";
        assert_clean(src);
        let a = first(src, SyntaxKind::AssignStatement);
        assert!(find(&a, SyntaxKind::MemberAccessExpression).is_some());
    }

    // resilience: never panic, always reach EOF, produce errors

    #[test]
    fn missing_semicolon_recovers() {
        let (root, errors) = parse("fn f() { let x: i32 = 1 let y: i32 = 2; }");
        assert!(errors > 0, "expected a parse error for the missing ;");
        assert!(count_kind(&root, SyntaxKind::VariableDefinitionStatement) >= 1);
    }

    #[test]
    fn missing_closing_paren_recovers() {
        let (_root, errors) = parse("fn f(a: i32 { }");
        assert!(errors > 0);
    }

    #[test]
    fn truncated_function_does_not_panic() {
        let (root, _errors) = parse("fn foo(");
        assert_eq!(root.kind, SyntaxKind::SourceFile);
    }

    #[test]
    fn stray_tokens_at_top_level_recover() {
        let (root, errors) = parse("# fn f() { } );");
        assert!(errors > 0);
        assert!(find(&root, SyntaxKind::FunctionDefinition).is_some());
    }

    #[test]
    fn pub_external_fn_terminates_with_diagnostic() {
        // Regression for C3: `pub external fn …` used to spin the source_file
        // loop forever because the external handler never consumed the leading
        // `pub`. The parser must now terminate (reaching this assertion proves
        // it did) and emit a diagnostic, producing the external node.
        let (root, errors) = parse("pub external fn f();");
        assert!(errors > 0, "expected a diagnostic for the stray `pub`");
        assert_eq!(root.kind, SyntaxKind::SourceFile);
        assert!(
            find(&root, SyntaxKind::ExternalFunctionDefinition).is_some(),
            "the external declaration should still be recognised:\n{}",
            root.debug_tree("pub external fn f();")
        );
        let e = first("pub external fn f();", SyntaxKind::ExternalFunctionDefinition);
        assert!(
            e.child(SyntaxKind::Visibility).is_some(),
            "the stray `pub` is consumed as a Visibility node"
        );
    }

    #[test]
    fn pub_external_fn_with_return_terminates() {
        // The `-> i32` form must also terminate cleanly (it shared the same
        // non-advancing path before C3 was fixed).
        let (root, errors) = parse("pub external fn f() -> i32;");
        assert!(errors > 0);
        assert_eq!(root.kind, SyntaxKind::SourceFile);
        assert!(find(&root, SyntaxKind::ExternalFunctionDefinition).is_some());
    }

    #[test]
    fn spec_pub_external_fn_terminates_with_diagnostic() {
        // Regression for C3 inside a spec body: the spec loop dispatches through
        // `definition`, so a `pub external fn` there also has to terminate.
        let src = "spec S { pub external fn f(); }";
        let (root, errors) = parse(src);
        assert!(errors > 0, "expected a diagnostic for the stray `pub`");
        assert_eq!(root.kind, SyntaxKind::SourceFile);
        assert!(
            find(&root, SyntaxKind::SpecDefinition).is_some(),
            "the spec should still be recognised:\n{}",
            root.debug_tree(src)
        );
        assert!(
            find(&root, SyntaxKind::ExternalFunctionDefinition).is_some(),
            "the spec-inner external declaration should still be recognised"
        );
    }

    #[test]
    fn fuzz_lite_never_panics() {
        // A handful of garbage strings: the parser must never panic and must
        // always return a SourceFile root reaching EOF.
        let garbage = [
            "",
            "}{)(][;;;",
            "fn fn fn",
            "@@@@",
            "let let let",
            "struct { } enum",
            "fn f() { if if if { } }",
            "::::::",
            "\"unterminated",
            "1 2 3 + + +",
            "fn f(((((((",
            "spec spec spec {{{{",
            "pub pub pub const",
            "------",
            "[[[[[[[[",
            "loop loop loop",
            "fn f() -> -> -> { }",
            "use use use ;;;",
            "i32 i32 i32 '''",
            "a::::b",
        ];
        for src in garbage {
            let (root, _errors) = parse(src);
            assert_eq!(
                root.kind,
                SyntaxKind::SourceFile,
                "garbage {src:?} must still yield a SourceFile root"
            );
        }
    }

    // corpus smoke

    // Real Inference programs, vendored under `core/parser/test_data/` and
    // embedded at compile time via `include_str!` so these tests run everywhere
    // (CI and any checkout), not just on a machine with a sibling
    // `tree-sitter-inference` clone. Both must parse with no `Error` node and no
    // collected parse errors.

    #[test]
    fn corpus_example_inf_parses_clean() {
        let src = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/test_data/example.inf"
        ));
        let (root, errors) = parse_to_cst(src);
        assert!(
            !has_error_node(&root),
            "example.inf produced Error node(s):\n{}",
            root.debug_tree(src)
        );
        assert!(
            errors.is_empty(),
            "example.inf produced parse errors: {errors:?}"
        );
    }

    #[test]
    fn corpus_debug_inf_parses_clean() {
        let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/test_data/debug.inf"));
        let (root, errors) = parse_to_cst(src);
        assert!(
            !has_error_node(&root),
            "debug.inf produced Error node(s):\n{}",
            root.debug_tree(src)
        );
        assert!(
            errors.is_empty(),
            "debug.inf produced parse errors: {errors:?}"
        );
    }
}
