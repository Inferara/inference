//! Position → node hit-testing: the smallest AST node covering a byte offset.
//!
//! # Per-file-local offsets
//!
//! In the merged multi-file arena every file's byte offsets start at zero, so an
//! offset alone does not name a file. Hit-testing is therefore always scoped to
//! one [`SourceFileId`]: the walk starts from that file's own top-level
//! definitions and descends only through the ids they own, never crossing into
//! another file. A naive arena-wide scan by offset would return false hits from
//! same-numbered positions in unrelated files.

use inference_ast::arena::AstArena;
use inference_ast::ids::{DefId, ExprId, NodeId, SourceFileId, StmtId, TypeId};
use inference_ast::nodes::{ArgData, ArgKind, Def, Expr, Location, Stmt, TypeNode};

/// The result of a position → node hit-test.
///
/// [`node`](Self::node) is the smallest AST node whose source range covers the
/// queried offset; [`ancestors`](Self::ancestors) is the chain of enclosing
/// nodes from the covering top-level definition inward to that node's immediate
/// parent, outermost first. The ancestor chain is what lets a feature widen its
/// view — from an identifier out to the call, statement, or definition that
/// encloses it — without re-walking the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeHit {
    /// The smallest node covering the offset.
    pub node: NodeId,
    /// Enclosing nodes, outermost first, ending at `node`'s immediate parent.
    /// Empty when `node` is itself a top-level definition.
    pub ancestors: Vec<NodeId>,
}

/// Returns the smallest node in `file` whose source range covers `offset`, with
/// its ancestor chain, or `None` when no definition in `file` covers `offset`
/// (whitespace between definitions, or a position past the last one).
///
/// `offset` is a byte offset local to `file`. `offset_end` is exclusive, so the
/// last byte of a token is covered but the byte immediately after it is not.
#[must_use = "the covering node is the reason to call this"]
pub fn hit_test(arena: &AstArena, file: SourceFileId, offset: u32) -> Option<NodeHit> {
    // HARD INVARIANT: only this file's own definitions. See the module docs.
    let mut current = arena[file]
        .defs
        .iter()
        .map(|&def| NodeId::Def(def))
        .find(|&node| covers(arena.node_location(node), offset))?;

    // Descend into the smallest covering child each step, recording every node
    // passed through as an ancestor. `current` ends at the smallest covering
    // node; `ancestors` holds the enclosing chain, outermost first.
    let mut ancestors = Vec::new();
    while let Some(child) = smallest_covering_child(arena, current, offset) {
        ancestors.push(current);
        current = child;
    }

    Some(NodeHit {
        node: current,
        ancestors,
    })
}

/// Among the direct children of `node`, the one with the smallest source range
/// that still covers `offset`, or `None` when no child does (so `node` is the
/// smallest covering node).
fn smallest_covering_child(arena: &AstArena, node: NodeId, offset: u32) -> Option<NodeId> {
    children_of(arena, node)
        .into_iter()
        .filter(|&child| covers(arena.node_location(child), offset))
        .min_by_key(|&child| span_len(arena.node_location(child)))
}

/// Whether `location` covers `offset` (`start <= offset < end`).
///
/// A [`Location::default`] (all-zero) marks a node the parser left unlocated; it
/// must never match, so an unlocated node cannot spuriously claim offset 0. Its
/// zero-width `0..0` range already excludes every offset, but the guard states
/// the intent.
fn covers(location: Location, offset: u32) -> bool {
    if location == Location::default() {
        return false;
    }
    location.offset_start <= offset && offset < location.offset_end
}

/// The byte width of a location's source range.
fn span_len(location: Location) -> u32 {
    location.offset_end.saturating_sub(location.offset_start)
}

/// The direct child nodes of `node`, each a candidate for the next descent step.
/// Leaves (identifiers, literals, simple types) have none.
fn children_of(arena: &AstArena, node: NodeId) -> Vec<NodeId> {
    match node {
        NodeId::Def(id) => def_children(arena, id),
        NodeId::Stmt(id) => stmt_children(arena, id),
        NodeId::Expr(id) => expr_children(arena, id),
        NodeId::Type(id) => type_children(arena, id),
        NodeId::Block(id) => arena[id].stmts.iter().map(|&s| NodeId::Stmt(s)).collect(),
        NodeId::Ident(_) | NodeId::SourceFile(_) => Vec::new(),
    }
}

fn def_children(arena: &AstArena, id: DefId) -> Vec<NodeId> {
    let mut out = Vec::new();
    match &arena[id].kind {
        Def::Function {
            name,
            args,
            returns,
            body,
            ..
        } => {
            out.push(NodeId::Ident(*name));
            arg_children(args, &mut out);
            if let Some(ret) = returns {
                out.push(NodeId::Type(*ret));
            }
            out.push(NodeId::Block(*body));
        }
        Def::ExternFunction {
            name,
            args,
            returns,
            ..
        } => {
            out.push(NodeId::Ident(*name));
            arg_children(args, &mut out);
            if let Some(ret) = returns {
                out.push(NodeId::Type(*ret));
            }
        }
        Def::Struct {
            name,
            fields,
            methods,
            ..
        } => {
            out.push(NodeId::Ident(*name));
            for field in fields {
                out.push(NodeId::Ident(field.name));
                out.push(NodeId::Type(field.ty));
            }
            for &method in methods {
                out.push(NodeId::Def(method));
            }
        }
        Def::Enum { name, variants, .. } => {
            out.push(NodeId::Ident(*name));
            for &variant in variants {
                out.push(NodeId::Ident(variant));
            }
        }
        Def::Spec { name, defs, .. } => {
            out.push(NodeId::Ident(*name));
            for &nested in defs {
                out.push(NodeId::Def(nested));
            }
        }
        Def::Constant {
            name, ty, value, ..
        } => {
            out.push(NodeId::Ident(*name));
            out.push(NodeId::Type(*ty));
            out.push(NodeId::Expr(*value));
        }
        Def::TypeAlias { name, ty, .. } => {
            out.push(NodeId::Ident(*name));
            out.push(NodeId::Type(*ty));
        }
    }
    out
}

/// Adds the arena-backed children of each argument: a named argument contributes
/// its name identifier and type; `self` contributes no node (the keyword has no
/// arena entry).
fn arg_children(args: &[ArgData], out: &mut Vec<NodeId>) {
    for arg in args {
        match &arg.kind {
            ArgKind::Named { name, ty, .. } => {
                out.push(NodeId::Ident(*name));
                out.push(NodeId::Type(*ty));
            }
            ArgKind::Ignored { ty } | ArgKind::TypeOnly(ty) => out.push(NodeId::Type(*ty)),
            ArgKind::SelfRef { .. } => {}
        }
    }
}

fn stmt_children(arena: &AstArena, id: StmtId) -> Vec<NodeId> {
    let mut out = Vec::new();
    match &arena[id].kind {
        Stmt::Block(block) => out.push(NodeId::Block(*block)),
        Stmt::Expr(expr) | Stmt::Return { expr } | Stmt::Assert { expr } => {
            out.push(NodeId::Expr(*expr));
        }
        Stmt::Assign { left, right } => {
            out.push(NodeId::Expr(*left));
            out.push(NodeId::Expr(*right));
        }
        Stmt::Loop { condition, body } => {
            if let Some(condition) = condition {
                out.push(NodeId::Expr(*condition));
            }
            out.push(NodeId::Block(*body));
        }
        Stmt::If {
            condition,
            then_block,
            else_block,
        } => {
            out.push(NodeId::Expr(*condition));
            out.push(NodeId::Block(*then_block));
            if let Some(else_block) = else_block {
                out.push(NodeId::Block(*else_block));
            }
        }
        Stmt::VarDef {
            name, ty, value, ..
        } => {
            out.push(NodeId::Ident(*name));
            out.push(NodeId::Type(*ty));
            if let Some(value) = value {
                out.push(NodeId::Expr(*value));
            }
        }
        Stmt::TypeDef { name, ty } => {
            out.push(NodeId::Ident(*name));
            out.push(NodeId::Type(*ty));
        }
        Stmt::ConstDef(def) => out.push(NodeId::Def(*def)),
        Stmt::Break => {}
    }
    out
}

fn expr_children(arena: &AstArena, id: ExprId) -> Vec<NodeId> {
    let mut out = Vec::new();
    match &arena[id].kind {
        Expr::Binary { left, right, .. } => {
            out.push(NodeId::Expr(*left));
            out.push(NodeId::Expr(*right));
        }
        Expr::PrefixUnary { expr, .. } | Expr::Parenthesized { expr } => {
            out.push(NodeId::Expr(*expr));
        }
        Expr::FunctionCall { function, args, .. } => {
            out.push(NodeId::Expr(*function));
            for (name, arg) in args {
                if let Some(name) = name {
                    out.push(NodeId::Ident(*name));
                }
                out.push(NodeId::Expr(*arg));
            }
        }
        Expr::ArrayIndexAccess { array, index } => {
            out.push(NodeId::Expr(*array));
            out.push(NodeId::Expr(*index));
        }
        Expr::MemberAccess { expr, name } | Expr::TypeMemberAccess { expr, name } => {
            out.push(NodeId::Expr(*expr));
            out.push(NodeId::Ident(*name));
        }
        Expr::StructLiteral { name, fields } => {
            out.push(NodeId::Ident(*name));
            for (field, value) in fields {
                out.push(NodeId::Ident(*field));
                out.push(NodeId::Expr(*value));
            }
        }
        Expr::Identifier(ident) => out.push(NodeId::Ident(*ident)),
        Expr::ArrayLiteral { elements } => {
            for &element in elements {
                out.push(NodeId::Expr(element));
            }
        }
        Expr::Type(ty) => out.push(NodeId::Type(*ty)),
        Expr::NumberLiteral { .. }
        | Expr::BoolLiteral { .. }
        | Expr::StringLiteral { .. }
        | Expr::UnitLiteral
        | Expr::Uzumaki => {}
    }
    out
}

fn type_children(arena: &AstArena, id: TypeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    match &arena[id].kind {
        TypeNode::Simple(_) => {}
        TypeNode::Array { element, size } => {
            out.push(NodeId::Type(*element));
            out.push(NodeId::Expr(*size));
        }
        TypeNode::Generic { base, params } => {
            out.push(NodeId::Ident(*base));
            for &param in params {
                out.push(NodeId::Ident(param));
            }
        }
        TypeNode::Function { params, ret } => {
            for &param in params {
                out.push(NodeId::Type(param));
            }
            if let Some(ret) = ret {
                out.push(NodeId::Type(*ret));
            }
        }
        TypeNode::QualifiedName { qualifier, name } => {
            out.push(NodeId::Ident(*qualifier));
            out.push(NodeId::Ident(*name));
        }
        TypeNode::Qualified { qualifier, name } => {
            for &segment in qualifier {
                out.push(NodeId::Ident(segment));
            }
            out.push(NodeId::Ident(*name));
        }
        TypeNode::Custom(ident) => out.push(NodeId::Ident(*ident)),
    }
    out
}

#[cfg(test)]
mod tests {
    // Test offsets are found in short inline sources, so the `usize -> u32` casts
    // used to build them cannot truncate.
    #![allow(clippy::cast_possible_truncation)]

    use super::*;
    use inference_parser::parse;

    /// Parses a single-file program and returns its arena plus the sole file id.
    fn single_file(source: &str) -> (AstArena, SourceFileId) {
        let arena = parse(source).arena;
        let file = arena.source_file_ids().next().expect("one source file");
        (arena, file)
    }

    /// The source text a hit's node spans, for readable assertions.
    fn hit_text<'a>(arena: &'a AstArena, source: &'a str, hit: &NodeHit) -> &'a str {
        let location = arena.node_location(hit.node);
        &source[location.offset_start as usize..location.offset_end as usize]
    }

    #[test]
    fn hits_identifier_at_its_first_byte() {
        let source = "fn f() -> i32 { return abc; }";
        let (arena, file) = single_file(source);
        let offset = source.find("abc").unwrap() as u32;
        let hit = hit_test(&arena, file, offset).expect("a node covers the identifier");
        assert!(matches!(hit.node, NodeId::Ident(_)));
        assert_eq!(hit_text(&arena, source, &hit), "abc");
    }

    #[test]
    fn hits_identifier_at_its_last_byte() {
        let source = "fn f() -> i32 { return abc; }";
        let (arena, file) = single_file(source);
        // The last byte of `abc` is covered (offset_end is exclusive).
        let offset = (source.find("abc").unwrap() + 2) as u32;
        let hit = hit_test(&arena, file, offset).expect("last byte of the identifier");
        assert_eq!(hit_text(&arena, source, &hit), "abc");
    }

    #[test]
    fn one_past_identifier_end_is_not_covered_by_it() {
        let source = "fn f() -> i32 { return abc; }";
        let (arena, file) = single_file(source);
        // The `;` right after `abc` is not part of the identifier.
        let offset = (source.find("abc").unwrap() + 3) as u32;
        let hit = hit_test(&arena, file, offset).expect("something still covers");
        assert_ne!(hit_text(&arena, source, &hit), "abc");
    }

    #[test]
    fn offset_between_tokens_falls_back_to_the_enclosing_block() {
        // A space between two statements is inside the function body block but
        // inside no statement, so the block is the smallest covering node.
        let source = "fn f() { let x: i32 = 1;   let y: i32 = 2; }";
        let (arena, file) = single_file(source);
        // Pick an offset in the run of spaces between the two `let`s.
        let gap = source.find(";   let").unwrap() + 2;
        let hit = hit_test(&arena, file, gap as u32).expect("the block covers the gap");
        assert!(
            matches!(hit.node, NodeId::Block(_)),
            "expected the enclosing block, got {:?}",
            hit.node
        );
    }

    #[test]
    fn offset_zero_hits_the_first_definition() {
        let source = "fn first() {} fn second() {}";
        let (arena, file) = single_file(source);
        let hit = hit_test(&arena, file, 0).expect("the first def starts at 0");
        // Offset 0 is the `f` of `fn`, inside the first def but covered by no
        // child (the `fn` keyword precedes the name identifier).
        match hit.node {
            NodeId::Def(def) => assert_eq!(arena.def_name(def), "first"),
            other => panic!("expected the first definition, got {other:?}"),
        }
    }

    #[test]
    fn offset_at_eof_hits_nothing() {
        let source = "fn f() {}";
        let (arena, file) = single_file(source);
        let hit = hit_test(&arena, file, source.len() as u32);
        assert!(
            hit.is_none(),
            "EOF is past every definition's exclusive end"
        );
    }

    #[test]
    fn ancestor_chain_is_deterministic_and_outermost_first() {
        let source = "fn f() -> i32 { return a + b; }";
        let (arena, file) = single_file(source);
        let offset = source.rfind('b').unwrap() as u32;
        let hit = hit_test(&arena, file, offset).expect("covers `b`");
        // Innermost node is the identifier `b`.
        assert_eq!(hit_text(&arena, source, &hit), "b");
        // Ancestors run outermost-first: Def, Block, Stmt(return), Expr(a + b),
        // Expr(b's Identifier expr). Every ancestor must cover the offset and the
        // chain must start at the top-level definition.
        assert!(matches!(hit.ancestors.first(), Some(NodeId::Def(_))));
        for &ancestor in &hit.ancestors {
            assert!(covers(arena.node_location(ancestor), offset));
        }
        // Determinism: a second identical query yields the same chain.
        let again = hit_test(&arena, file, offset).unwrap();
        assert_eq!(hit, again);
    }

    #[test]
    fn hits_a_type_annotation() {
        let source = "fn f(p: i32) {}";
        let (arena, file) = single_file(source);
        let offset = source.find("i32").unwrap() as u32;
        let hit = hit_test(&arena, file, offset).expect("covers the type");
        assert!(matches!(hit.node, NodeId::Type(_)));
    }

    #[test]
    fn hits_the_member_name_of_a_member_access() {
        // `p.field`: the `.field` name and the `p` receiver are siblings under the
        // member access, so a hit must resolve to the one covering the offset.
        let source = "fn f() -> i32 { return p.field; }";
        let (arena, file) = single_file(source);

        let name_hit = hit_test(&arena, file, source.find("field").unwrap() as u32)
            .expect("covers the member name");
        assert_eq!(hit_text(&arena, source, &name_hit), "field");

        // The receiver descends to its own identifier, not the member name.
        let recv_hit = hit_test(&arena, file, source.find("p.field").unwrap() as u32)
            .expect("covers the receiver");
        assert_eq!(hit_text(&arena, source, &recv_hit), "p");
    }

    #[test]
    fn hits_the_callee_and_arguments_of_a_function_call() {
        // `g(x, y)`: goto-def dispatches on the callee identifier, hover on an
        // argument — both are descent targets under the call expression.
        let source = "fn f() -> i32 { return g(x, y); }";
        let (arena, file) = single_file(source);

        let callee =
            hit_test(&arena, file, source.find("g(").unwrap() as u32).expect("covers the callee");
        assert_eq!(hit_text(&arena, source, &callee), "g");

        let arg =
            hit_test(&arena, file, source.find("y)").unwrap() as u32).expect("covers the argument");
        assert_eq!(hit_text(&arena, source, &arg), "y");
    }

    #[test]
    fn hits_the_name_and_field_of_a_struct_literal() {
        // `P { a: 1 }`: goto-def on the struct name, hover on a field name.
        let source = "fn f() -> i32 { return P { a: 1 }; }";
        let (arena, file) = single_file(source);

        let name = hit_test(&arena, file, source.find("P {").unwrap() as u32)
            .expect("covers the struct name");
        assert_eq!(hit_text(&arena, source, &name), "P");

        let field = hit_test(&arena, file, source.find("a:").unwrap() as u32)
            .expect("covers the field name");
        assert_eq!(hit_text(&arena, source, &field), "a");
    }

    #[test]
    fn scoped_to_one_file_in_a_two_file_arena() {
        // Two files whose identifiers occupy the same byte range: a query against
        // one file must return that file's node, never the other's, even though
        // the offset is valid in both.
        use inference_parser::parse_into;

        let entry_src = "fn e() -> i32 { return aaa; }";
        let lib_src = "fn l() -> i32 { return bbb; }";
        let parsed = parse_into(AstArena::default(), entry_src, vec![]);
        let parsed = parse_into(parsed.arena, lib_src, vec!["lib".to_string()]);
        let arena = parsed.arena;

        let entry = arena
            .source_file_ids()
            .find(|&f| arena[f].module_path.is_empty())
            .unwrap();
        let lib = arena
            .source_file_ids()
            .find(|&f| arena[f].module_path == vec!["lib".to_string()])
            .unwrap();

        // `aaa` and `bbb` sit at the same per-file-local offset.
        let offset = entry_src.find("aaa").unwrap() as u32;
        assert_eq!(offset, lib_src.find("bbb").unwrap() as u32);

        let entry_hit = hit_test(&arena, entry, offset).expect("entry hit");
        let lib_hit = hit_test(&arena, lib, offset).expect("lib hit");
        if let (NodeId::Ident(a), NodeId::Ident(b)) = (entry_hit.node, lib_hit.node) {
            assert_eq!(arena.ident_name(a), "aaa");
            assert_eq!(arena.ident_name(b), "bbb");
        } else {
            panic!("expected identifier hits in both files");
        }
    }

    #[test]
    fn unlocated_nodes_are_never_hit() {
        // `covers` rejects a default (all-zero) location, so a node the parser
        // left unlocated cannot be returned even at offset 0.
        assert!(!covers(Location::default(), 0));
        assert!(!covers(Location::default(), 5));
    }

    #[test]
    fn hits_the_name_args_and_return_type_of_an_extern_function() {
        // An extern function declares a signature but no body, so its descendable
        // children are the name, the argument, and the return type.
        let source = "external fn hash(seed: i32) -> i64;";
        let (arena, file) = single_file(source);

        let name = hit_test(&arena, file, source.find("hash").unwrap() as u32)
            .expect("covers the extern function name");
        assert_eq!(hit_text(&arena, source, &name), "hash");

        let arg = hit_test(&arena, file, source.find("seed").unwrap() as u32)
            .expect("covers the argument name");
        assert_eq!(hit_text(&arena, source, &arg), "seed");

        let arg_ty = hit_test(&arena, file, source.find("i32").unwrap() as u32)
            .expect("covers the argument type");
        assert!(matches!(arg_ty.node, NodeId::Type(_)));

        let ret = hit_test(&arena, file, source.find("i64").unwrap() as u32)
            .expect("covers the return type");
        assert!(matches!(ret.node, NodeId::Type(_)));
        assert_eq!(hit_text(&arena, source, &ret), "i64");
    }

    #[test]
    fn hits_the_name_and_variants_of_an_enum() {
        let source = "enum Color { Red, Green, Blue }";
        let (arena, file) = single_file(source);

        let name = hit_test(&arena, file, source.find("Color").unwrap() as u32)
            .expect("covers the enum name");
        assert_eq!(hit_text(&arena, source, &name), "Color");

        let variant =
            hit_test(&arena, file, source.find("Green").unwrap() as u32).expect("covers a variant");
        assert_eq!(hit_text(&arena, source, &variant), "Green");
        assert!(matches!(variant.node, NodeId::Ident(_)));
        // A variant sits directly under the enum definition.
        assert!(matches!(variant.ancestors.first(), Some(NodeId::Def(_))));
    }

    #[test]
    fn hits_the_name_and_nested_definition_of_a_spec() {
        let source = "spec Bank { fn balance() -> i64 { return 0; } }";
        let (arena, file) = single_file(source);

        let name = hit_test(&arena, file, source.find("Bank").unwrap() as u32)
            .expect("covers the spec name");
        assert_eq!(hit_text(&arena, source, &name), "Bank");

        // Descends through the spec into its nested function's name.
        let nested = hit_test(&arena, file, source.find("balance").unwrap() as u32)
            .expect("covers the nested function name");
        assert_eq!(hit_text(&arena, source, &nested), "balance");
        // The spec is the outermost ancestor and the nested function's own
        // definition also appears on the chain, so there are two `Def` ancestors.
        assert!(matches!(nested.ancestors.first(), Some(NodeId::Def(_))));
        assert_eq!(
            nested
                .ancestors
                .iter()
                .filter(|a| matches!(a, NodeId::Def(_)))
                .count(),
            2
        );
    }

    #[test]
    fn hits_the_name_type_and_value_of_a_constant() {
        let source = "const MAX: i32 = 100;";
        let (arena, file) = single_file(source);

        let name = hit_test(&arena, file, source.find("MAX").unwrap() as u32)
            .expect("covers the constant name");
        assert_eq!(hit_text(&arena, source, &name), "MAX");

        let ty = hit_test(&arena, file, source.find("i32").unwrap() as u32)
            .expect("covers the constant type");
        assert!(matches!(ty.node, NodeId::Type(_)));

        let value = hit_test(&arena, file, source.find("100").unwrap() as u32)
            .expect("covers the constant value");
        assert_eq!(hit_text(&arena, source, &value), "100");
    }

    #[test]
    fn hits_the_name_and_aliased_type_of_a_type_alias() {
        let source = "type Word = u64;";
        let (arena, file) = single_file(source);

        let name = hit_test(&arena, file, source.find("Word").unwrap() as u32)
            .expect("covers the alias name");
        assert_eq!(hit_text(&arena, source, &name), "Word");

        let ty = hit_test(&arena, file, source.find("u64").unwrap() as u32)
            .expect("covers the aliased type");
        assert!(matches!(ty.node, NodeId::Type(_)));
        assert_eq!(hit_text(&arena, source, &ty), "u64");
    }

    #[test]
    fn hits_the_type_of_an_ignored_argument() {
        // `_: i32` names no binding, so the type is the argument's only child.
        let source = "fn f(_: i32) {}";
        let (arena, file) = single_file(source);
        let hit = hit_test(&arena, file, source.find("i32").unwrap() as u32)
            .expect("covers the ignored argument type");
        assert!(matches!(hit.node, NodeId::Type(_)));
        assert_eq!(hit_text(&arena, source, &hit), "i32");
    }

    #[test]
    fn hits_the_type_of_a_positional_type_only_argument() {
        // A bare type in argument position (`i32`, no name) is a type-only
        // argument; its type is the descent target.
        let source = "fn f(i32) {}";
        let (arena, file) = single_file(source);
        let hit = hit_test(&arena, file, source.find("i32").unwrap() as u32)
            .expect("covers the positional type argument");
        assert!(matches!(hit.node, NodeId::Type(_)));
        assert_eq!(hit_text(&arena, source, &hit), "i32");
    }

    #[test]
    fn hits_both_sides_of_an_assignment() {
        let source = "fn f() { let mut x: i32 = 0; x = 7; }";
        let (arena, file) = single_file(source);

        // The assignment's target is the last `x`, distinct from its declaration.
        let left = hit_test(&arena, file, source.rfind('x').unwrap() as u32)
            .expect("covers the assignment target");
        assert_eq!(hit_text(&arena, source, &left), "x");

        let right = hit_test(&arena, file, source.find('7').unwrap() as u32)
            .expect("covers the assigned value");
        assert_eq!(hit_text(&arena, source, &right), "7");
        assert!(right.ancestors.iter().any(|a| matches!(a, NodeId::Stmt(_))));
    }

    #[test]
    fn hits_the_condition_of_a_conditional_loop() {
        let source = "fn f() { let n: i32 = 0; loop n < 3 { break; } }";
        let (arena, file) = single_file(source);
        // The condition descends to its own operand identifier.
        let cond = hit_test(&arena, file, source.find("n < 3").unwrap() as u32)
            .expect("covers the loop condition");
        assert_eq!(hit_text(&arena, source, &cond), "n");
        assert!(cond.ancestors.iter().any(|a| matches!(a, NodeId::Expr(_))));
    }

    #[test]
    fn hits_the_body_of_an_infinite_loop() {
        // An infinite loop carries no condition, so descent goes straight through
        // the body block; the `break` inside it is a childless leaf statement.
        let source = "fn f() { loop { break; } }";
        let (arena, file) = single_file(source);
        let hit = hit_test(&arena, file, source.find("break").unwrap() as u32)
            .expect("covers the break inside the loop body");
        assert!(matches!(hit.node, NodeId::Stmt(_)));
        assert!(hit.ancestors.iter().any(|a| matches!(a, NodeId::Block(_))));
    }

    #[test]
    fn hits_the_condition_and_then_block_of_an_if_without_else() {
        let source = "fn f() { let ok: bool = true; if ok { let y: i32 = 1; } }";
        let (arena, file) = single_file(source);

        // The condition is the second `ok`, in the `if` head.
        let cond = hit_test(&arena, file, source.rfind("ok").unwrap() as u32)
            .expect("covers the if condition");
        assert_eq!(hit_text(&arena, source, &cond), "ok");

        // The then-block's inner statement is reachable through the then arm.
        let then_hit = hit_test(&arena, file, source.find("y:").unwrap() as u32)
            .expect("covers a statement in the then-block");
        assert_eq!(hit_text(&arena, source, &then_hit), "y");
    }

    #[test]
    fn hits_the_else_block_of_an_if_else() {
        // `z` lives only in the else-block, so reaching it exercises the else arm.
        let source = "fn f() { if true { let a: i32 = 1; } else { let z: i32 = 2; } }";
        let (arena, file) = single_file(source);
        let hit = hit_test(&arena, file, source.find('z').unwrap() as u32)
            .expect("covers a statement in the else-block");
        assert_eq!(hit_text(&arena, source, &hit), "z");
        assert!(hit.ancestors.iter().any(|a| matches!(a, NodeId::Block(_))));
    }

    #[test]
    fn hits_a_local_type_definition_statement() {
        // A local `type X = ..;` is a statement, distinct from a top-level alias.
        let source = "fn f() { type Small = u8; }";
        let (arena, file) = single_file(source);

        let name = hit_test(&arena, file, source.find("Small").unwrap() as u32)
            .expect("covers the local type name");
        assert_eq!(hit_text(&arena, source, &name), "Small");

        let ty = hit_test(&arena, file, source.find("u8").unwrap() as u32)
            .expect("covers the local aliased type");
        assert!(matches!(ty.node, NodeId::Type(_)));
        assert_eq!(hit_text(&arena, source, &ty), "u8");
    }

    #[test]
    fn hits_a_local_constant_definition_statement() {
        // A local const lowers to a statement wrapping a constant definition, so
        // its name is reached through statement → definition → name.
        let source = "fn f() { const LIMIT: i32 = 8; }";
        let (arena, file) = single_file(source);

        let name = hit_test(&arena, file, source.find("LIMIT").unwrap() as u32)
            .expect("covers the local constant name");
        assert_eq!(hit_text(&arena, source, &name), "LIMIT");
        assert!(name.ancestors.iter().any(|a| matches!(a, NodeId::Def(_))));

        let value = hit_test(&arena, file, source.find('8').unwrap() as u32)
            .expect("covers the local constant value");
        assert_eq!(hit_text(&arena, source, &value), "8");
    }

    #[test]
    fn hits_the_operand_of_a_prefix_unary_expression() {
        let source = "fn f() -> i32 { return -k; }";
        let (arena, file) = single_file(source);
        let hit = hit_test(&arena, file, source.find('k').unwrap() as u32)
            .expect("covers the negated operand");
        assert_eq!(hit_text(&arena, source, &hit), "k");
        assert!(hit.ancestors.iter().any(|a| matches!(a, NodeId::Expr(_))));
    }

    #[test]
    fn hits_the_inner_expression_of_a_parenthesized_expression() {
        let source = "fn f() -> i32 { return (k); }";
        let (arena, file) = single_file(source);
        let hit = hit_test(&arena, file, source.find('k').unwrap() as u32)
            .expect("covers the parenthesized inner expression");
        assert_eq!(hit_text(&arena, source, &hit), "k");
    }

    #[test]
    fn hits_the_name_of_a_named_call_argument() {
        // `g(limit: 5)`: the argument name is a descent target alongside its value.
        let source = "fn f() -> i32 { return g(limit: 5); }";
        let (arena, file) = single_file(source);

        let name = hit_test(&arena, file, source.find("limit").unwrap() as u32)
            .expect("covers the argument name");
        assert_eq!(hit_text(&arena, source, &name), "limit");
        assert!(matches!(name.node, NodeId::Ident(_)));
        assert!(name.ancestors.iter().any(|a| matches!(a, NodeId::Expr(_))));

        let value = hit_test(&arena, file, source.find('5').unwrap() as u32)
            .expect("covers the argument value");
        assert_eq!(hit_text(&arena, source, &value), "5");
    }

    #[test]
    fn hits_the_array_and_index_of_an_index_access() {
        let source = "fn f(a: [i32; 4]) -> i32 { return a[2]; }";
        let (arena, file) = single_file(source);

        let arr = hit_test(&arena, file, source.find("a[2]").unwrap() as u32)
            .expect("covers the indexed array");
        assert_eq!(hit_text(&arena, source, &arr), "a");

        let index = hit_test(&arena, file, source.find("2]").unwrap() as u32)
            .expect("covers the index expression");
        assert_eq!(hit_text(&arena, source, &index), "2");
    }

    #[test]
    fn hits_an_element_of_an_array_literal() {
        let source = "fn f() { let xs: [i32; 3] = [10, 20, 30]; }";
        let (arena, file) = single_file(source);
        let elem = hit_test(&arena, file, source.find("20").unwrap() as u32)
            .expect("covers an array-literal element");
        assert_eq!(hit_text(&arena, source, &elem), "20");
        assert!(elem.ancestors.iter().any(|a| matches!(a, NodeId::Expr(_))));
    }

    #[test]
    fn hits_the_base_and_parameter_of_a_generic_type() {
        // `Vec i32'` is a generic type: its base and each type argument are stored
        // as identifiers under the type node.
        let source = "fn f(v: Vec i32') {}";
        let (arena, file) = single_file(source);

        let base = hit_test(&arena, file, source.find("Vec").unwrap() as u32)
            .expect("covers the generic base");
        assert_eq!(hit_text(&arena, source, &base), "Vec");
        assert!(matches!(base.node, NodeId::Ident(_)));

        let param = hit_test(&arena, file, source.find("i32").unwrap() as u32)
            .expect("covers the generic type argument");
        assert_eq!(hit_text(&arena, source, &param), "i32");
        assert!(matches!(param.node, NodeId::Ident(_)));
    }

    #[test]
    fn hits_a_generic_name_used_as_an_expression() {
        // A generic name in value position lowers to an `Expr::Type` wrapping the
        // generic type, so descent passes through both an expression and a type
        // node before reaching the argument identifier.
        let source = "fn f() -> i32 { return Buf u8'; }";
        let (arena, file) = single_file(source);
        let param = hit_test(&arena, file, source.find("u8").unwrap() as u32)
            .expect("covers the generic argument in expression position");
        assert_eq!(hit_text(&arena, source, &param), "u8");
        assert!(param.ancestors.iter().any(|a| matches!(a, NodeId::Expr(_))));
        assert!(param.ancestors.iter().any(|a| matches!(a, NodeId::Type(_))));
    }

    #[test]
    fn hits_the_element_and_size_of_an_array_type() {
        let source = "fn f(a: [i32; 4]) {}";
        let (arena, file) = single_file(source);

        let element = hit_test(&arena, file, source.find("i32").unwrap() as u32)
            .expect("covers the array element type");
        assert!(matches!(element.node, NodeId::Type(_)));
        assert_eq!(hit_text(&arena, source, &element), "i32");

        let size = hit_test(&arena, file, source.find('4').unwrap() as u32)
            .expect("covers the array size expression");
        assert_eq!(hit_text(&arena, source, &size), "4");
        assert!(size.ancestors.iter().any(|a| matches!(a, NodeId::Type(_))));
    }

    #[test]
    fn hits_the_return_type_of_a_function_type_annotation() {
        // `fn(i32) -> i64` as a type: the arrow's return type is descendable.
        let source = "fn f(cb: fn(i32) -> i64) {}";
        let (arena, file) = single_file(source);
        let ret = hit_test(&arena, file, source.find("i64").unwrap() as u32)
            .expect("covers the function type's return type");
        assert!(matches!(ret.node, NodeId::Type(_)));
        assert_eq!(hit_text(&arena, source, &ret), "i64");
        assert!(ret.ancestors.iter().any(|a| matches!(a, NodeId::Type(_))));
    }

    #[test]
    fn function_type_without_return_type_is_the_smallest_covering_node() {
        // A `fn(...)` type with no `->` has a `None` return arm. The parser does
        // not lower `fn`-type parameters (a pinned quirk), so the annotation has
        // no descendable children and is itself the smallest covering node.
        let source = "fn f(cb: fn(i32)) {}";
        let (arena, file) = single_file(source);
        let hit = hit_test(&arena, file, source.find("fn(i32)").unwrap() as u32)
            .expect("covers the function type annotation");
        assert!(matches!(hit.node, NodeId::Type(_)));
        assert_eq!(hit_text(&arena, source, &hit), "fn(i32)");
    }

    #[test]
    fn hits_the_segments_of_a_qualified_type() {
        // `lib::geom::Point`: every `::`-segment qualifier and the leaf name are
        // descent targets under the qualified type node.
        let source = "fn f(p: lib::geom::Point) {}";
        let (arena, file) = single_file(source);

        let qualifier = hit_test(&arena, file, source.find("geom").unwrap() as u32)
            .expect("covers a qualifier segment");
        assert_eq!(hit_text(&arena, source, &qualifier), "geom");
        assert!(matches!(qualifier.node, NodeId::Ident(_)));

        let leaf = hit_test(&arena, file, source.find("Point").unwrap() as u32)
            .expect("covers the leaf type name");
        assert_eq!(hit_text(&arena, source, &leaf), "Point");
        assert!(matches!(leaf.node, NodeId::Ident(_)));
    }
}
