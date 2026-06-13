//! Definition-value cycle detection over `const` initializers and type aliases.
//!
//! File-to-file import cycles are allowed (#63): scope resolution walks a
//! pre-built tree, so a cyclic import graph costs nothing. What *cannot* be
//! resolved is a cycle among definition **values** — a `const` whose initializer
//! reads another `const` that (transitively) reads the first, or mutually
//! recursive type aliases. Such a cycle has no evaluation order, so it is a hard
//! error ([`TypeCheckError::CircularDefinition`]).
//!
//! The graph is built across all files. A node is one top-level `const` or
//! `type` alias, identified by `(defining-scope-id, name)` so same-named
//! definitions in different files stay distinct. An edge `A -> B` means `A`'s
//! value (a const initializer expression or a type-alias target type) references
//! `B` by name or qualified path. The graph is checked for cycles; when acyclic,
//! a topological order is produced for a later phase to emit definitions in a
//! computable order.

use inference_ast::arena::AstArena;
use inference_ast::ids::{DefId, ExprId, TypeId};
use inference_ast::nodes::{Def, Expr, Location, TypeNode};
use rustc_hash::FxHashMap;

/// A node in the definition-value graph: a top-level `const` or `type` alias.
#[derive(Debug, Clone)]
pub(crate) struct DefNode {
    /// The `const` / `type alias` declaration this node stands for.
    pub(crate) def_id: DefId,
    /// The scope the definition was registered in (its file scope for a
    /// top-level definition). With its `name`, uniquely identifies the node.
    pub(crate) scope_id: u32,
    /// The defined name.
    pub(crate) name: String,
    /// The `::`-joined module path of the defining file (empty for the entry
    /// file). Joined with `name` it gives the canonical path a qualified
    /// reference (`lib::vals::V`) resolves against.
    pub(crate) file_path: String,
    /// Source location of the declaration, for the cycle diagnostic.
    pub(crate) location: Location,
    /// Scope-id ancestry from the node's own scope up to the root, used to
    /// resolve a referenced bare name the way name lookup would (own file first,
    /// then the program root).
    pub(crate) scope_chain: Vec<u32>,
}

impl DefNode {
    /// Canonical `::`-path of this definition: `<file_path>::<name>`, or the bare
    /// name for the entry file. A qualified reference resolves by matching it.
    fn canonical_path(&self) -> String {
        if self.file_path.is_empty() {
            self.name.clone()
        } else {
            format!("{}::{}", self.file_path, self.name)
        }
    }
}

/// The outcome of analyzing the definition-value graph.
pub(crate) enum GraphOutcome {
    /// No value cycle. Carries a topological order of the definitions (`DefId`s),
    /// dependencies first, for a later phase to emit in a computable order.
    Acyclic { topo_order: Vec<DefId> },
    /// A value cycle. Carries the members in cycle order and the location of the
    /// entry member, for the diagnostic.
    Cyclic { cycle: Vec<String>, location: Location },
}

/// Builds and analyzes the definition-value graph for `nodes`.
///
/// `nodes` must list every top-level `const` and `type` alias across all files.
/// References are resolved against the scope chains carried by each node, so the
/// builder needs no live symbol table.
pub(crate) fn analyze(arena: &AstArena, nodes: &[DefNode]) -> GraphOutcome {
    let graph = DefGraph::build(arena, nodes);
    graph.analyze()
}

/// Internal adjacency representation: nodes indexed `0..n`, edges as target
/// indices.
struct DefGraph<'a> {
    nodes: &'a [DefNode],
    /// `edges[i]` = indices of nodes that node `i` references (its dependencies).
    edges: Vec<Vec<usize>>,
}

impl<'a> DefGraph<'a> {
    fn build(arena: &AstArena, nodes: &'a [DefNode]) -> Self {
        // Two indexes: bare names by (scope_id, name) for in-file/absolute name
        // lookup, and full canonical paths for qualified (`lib::vals::V`)
        // references.
        let mut by_key: FxHashMap<(u32, &str), usize> = FxHashMap::default();
        let mut by_path: FxHashMap<String, usize> = FxHashMap::default();
        for (i, n) in nodes.iter().enumerate() {
            by_key.insert((n.scope_id, n.name.as_str()), i);
            by_path.insert(n.canonical_path(), i);
        }

        let mut edges = vec![Vec::new(); nodes.len()];
        for (i, node) in nodes.iter().enumerate() {
            let mut referenced: Vec<String> = Vec::new();
            match &arena[node.def_id].kind {
                Def::Constant { value, .. } => {
                    collect_expr_refs(arena, *value, &mut referenced);
                }
                Def::TypeAlias { ty, .. } => {
                    collect_type_refs(arena, *ty, &mut referenced);
                }
                _ => continue,
            }
            for name in referenced {
                // A self-edge (`const A = A;`) is a degenerate value cycle: the
                // node's value depends on its own, with no evaluation order. It is
                // recorded like any other edge so the cycle check rejects it.
                if let Some(target) = resolve_ref(&by_key, &by_path, node, &name)
                    && !edges[i].contains(&target)
                {
                    edges[i].push(target);
                }
            }
        }

        DefGraph { nodes, edges }
    }

    fn analyze(&self) -> GraphOutcome {
        // Iterative DFS with three colors. `White` = unvisited, `Gray` = on the
        // current DFS stack (a back-edge to a gray node is a cycle), `Black` =
        // fully explored. Postorder accumulation yields the topological order
        // (dependencies finish before dependents, so postorder is dependency
        // order).
        #[derive(Clone, Copy, PartialEq)]
        enum Color {
            White,
            Gray,
            Black,
        }
        let n = self.nodes.len();
        let mut color = vec![Color::White; n];
        let mut postorder: Vec<usize> = Vec::with_capacity(n);
        // Parent pointers reconstruct a cycle path when a back-edge is found.
        let mut parent = vec![usize::MAX; n];

        for start in 0..n {
            if color[start] != Color::White {
                continue;
            }
            // Stack frames: (node, next child index to examine).
            let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
            color[start] = Color::Gray;
            while let Some(&(node, child_idx)) = stack.last() {
                if child_idx < self.edges[node].len() {
                    stack.last_mut().expect("non-empty").1 += 1;
                    let next = self.edges[node][child_idx];
                    match color[next] {
                        Color::White => {
                            parent[next] = node;
                            color[next] = Color::Gray;
                            stack.push((next, 0));
                        }
                        Color::Gray => {
                            return self.cycle_from_back_edge(node, next, &parent);
                        }
                        Color::Black => {}
                    }
                } else {
                    color[node] = Color::Black;
                    postorder.push(node);
                    stack.pop();
                }
            }
        }

        let topo_order = postorder.iter().map(|&i| self.nodes[i].def_id).collect();
        GraphOutcome::Acyclic { topo_order }
    }

    /// Reconstructs the cycle that a back-edge `from -> to` (with `to` gray)
    /// closes, walking parent pointers from `from` back up to `to`.
    fn cycle_from_back_edge(
        &self,
        from: usize,
        to: usize,
        parent: &[usize],
    ) -> GraphOutcome {
        let mut path = vec![from];
        let mut cur = from;
        while cur != to {
            cur = parent[cur];
            if cur == usize::MAX {
                break;
            }
            path.push(cur);
        }
        path.reverse();
        // `path` is `to .. from`; the back-edge closes it by returning to `to`,
        // so append `to`'s name to render `A -> B -> A`.
        let mut cycle: Vec<String> = path.iter().map(|&i| self.nodes[i].name.clone()).collect();
        cycle.push(self.nodes[to].name.clone());
        let location = self.nodes[to].location;
        GraphOutcome::Cyclic { cycle, location }
    }
}

/// Resolves a referenced name (bare or `::`-qualified) to a node index.
///
/// A `::`-qualified reference (`lib::vals::V`) names a definition's canonical
/// path directly and resolves by exact match. A bare reference resolves the way
/// name lookup would: along the referencing node's scope chain (own file first,
/// then the program root). Returns `None` when the reference names something that
/// is not a tracked const/type alias (a function, a builtin, a local) — such
/// references never participate in a definition-value cycle.
fn resolve_ref(
    by_key: &FxHashMap<(u32, &str), usize>,
    by_path: &FxHashMap<String, usize>,
    from: &DefNode,
    reference: &str,
) -> Option<usize> {
    if reference.contains("::") {
        return by_path.get(reference).copied();
    }
    for &scope_id in &from.scope_chain {
        if let Some(&idx) = by_key.get(&(scope_id, reference)) {
            return Some(idx);
        }
    }
    None
}

/// Collects names referenced by a const initializer expression: bare
/// identifiers and the leaf of any `::`-qualified path. Recurses through the
/// arithmetic and access expression forms a const initializer can contain.
fn collect_expr_refs(arena: &AstArena, expr_id: ExprId, out: &mut Vec<String>) {
    match &arena[expr_id].kind {
        Expr::Identifier(ident_id) => out.push(arena[*ident_id].name.clone()),
        Expr::TypeMemberAccess { expr, name } => {
            if let Some(path) = flatten_path(arena, expr_id) {
                out.push(path);
            } else {
                collect_expr_refs(arena, *expr, out);
                out.push(arena[*name].name.clone());
            }
        }
        Expr::MemberAccess { expr, .. } => collect_expr_refs(arena, *expr, out),
        Expr::Binary { left, right, .. } => {
            collect_expr_refs(arena, *left, out);
            collect_expr_refs(arena, *right, out);
        }
        Expr::PrefixUnary { expr, .. } | Expr::Parenthesized { expr } => {
            collect_expr_refs(arena, *expr, out);
        }
        Expr::ArrayIndexAccess { array, index } => {
            collect_expr_refs(arena, *array, out);
            collect_expr_refs(arena, *index, out);
        }
        Expr::FunctionCall { args, .. } => {
            for (_, arg) in args {
                collect_expr_refs(arena, *arg, out);
            }
        }
        Expr::ArrayLiteral { elements } => {
            for elem in elements {
                collect_expr_refs(arena, *elem, out);
            }
        }
        _ => {}
    }
}

/// Flattens a `TypeMemberAccess` chain whose deepest base is an identifier into
/// its `::`-joined path; `None` if the base is a value expression.
fn flatten_path(arena: &AstArena, expr_id: ExprId) -> Option<String> {
    fn walk(arena: &AstArena, expr_id: ExprId, segments: &mut Vec<String>) -> bool {
        match &arena[expr_id].kind {
            Expr::Identifier(ident_id) => {
                segments.push(arena[*ident_id].name.clone());
                true
            }
            Expr::TypeMemberAccess { expr, name } => {
                if !walk(arena, *expr, segments) {
                    return false;
                }
                segments.push(arena[*name].name.clone());
                true
            }
            _ => false,
        }
    }
    let mut segments = Vec::new();
    if walk(arena, expr_id, &mut segments) {
        Some(segments.join("::"))
    } else {
        None
    }
}

/// Collects type-alias references from a type node: `Custom`, `QualifiedName`,
/// and `Qualified` names, recursing into arrays.
fn collect_type_refs(arena: &AstArena, ty_id: TypeId, out: &mut Vec<String>) {
    match &arena[ty_id].kind {
        TypeNode::Custom(ident_id) => out.push(arena[*ident_id].name.clone()),
        TypeNode::QualifiedName { qualifier, name } => {
            out.push(format!("{}::{}", arena[*qualifier].name, arena[*name].name));
        }
        TypeNode::Qualified { name, .. } => out.push(arena[*name].name.clone()),
        TypeNode::Array { element, .. } => collect_type_refs(arena, *element, out),
        _ => {}
    }
}
