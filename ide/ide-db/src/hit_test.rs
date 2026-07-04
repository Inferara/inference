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
}
