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
            item(p);
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
fn item(p: &mut Parser) {
    match p.current() {
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

    // ---- items ----

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

    // ---- use directives ----

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
    fn use_from_literal() {
        let src = "use { sort, hash } from \"./sort.rs\";";
        assert_clean(src);
        let u = first(src, SyntaxKind::UseDirective);
        assert!(u.child(SyntaxKind::StringLiteral).is_some());
        assert_eq!(count_kind(&u, SyntaxKind::Identifier), 2);
    }

    // ---- types ----

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

    // ---- statements ----

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

    // ---- expressions: literals & atoms ----

    #[test]
    fn number_literal_atom() {
        let src = "fn f() { x = 42; }";
        assert_clean(src);
        assert!(find(&parse_to_cst(src).0, SyntaxKind::NumberLiteral).is_some());
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

    // ---- expressions: precedence & associativity ----

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

    // ---- disambiguations ----

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

    // ---- resilience: never panic, always reach EOF, produce errors ----

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

    // ---- corpus smoke ----

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
