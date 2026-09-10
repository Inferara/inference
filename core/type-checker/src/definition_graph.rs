//! Definition-value cycle detection over `const` initializers.
//!
//! File-to-file import cycles are allowed (#63): scope resolution walks a
//! pre-built tree, so a cyclic import graph costs nothing. What *cannot* be
//! resolved is a cycle among definition **values** — a `const` whose initializer
//! reads another `const` that (transitively) reads the first. Such a cycle has no
//! evaluation order, so it is a hard error
//! ([`TypeCheckError::CircularDefinition`]).
//!
//! The graph is built across all files. A node is one top-level `const`,
//! identified by `(defining-scope-id, name)` so same-named definitions in
//! different files stay distinct. An edge `A -> B` means `A`'s initializer
//! expression references `B` by name or qualified path. The graph is checked for
//! cycles; when acyclic, a topological order is produced for a later phase to
//! emit definitions in a computable order.

use inference_ast::arena::AstArena;
use inference_ast::ids::{DefId, ExprId};
use inference_ast::nodes::{Def, Expr, Location};
use rustc_hash::FxHashMap;

use crate::symbol_table::{ResolvedImportTarget, SymbolTable};

/// A node in the definition-value graph: a top-level `const`.
#[derive(Debug, Clone)]
pub(crate) struct DefNode {
    /// The `const` declaration this node stands for.
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
    /// A value cycle. Carries the members in cycle order, the location of the
    /// entry member, and the scope that member is defined in — so the diagnostic
    /// is stamped with the file the cycle lives in rather than rendering bare when
    /// the cycle is entirely within a non-entry file.
    Cyclic {
        cycle: Vec<String>,
        location: Location,
        scope_id: u32,
    },
}

/// Cross-file edge discovery via item/namespace imports.
///
/// A bare reference (`const A: i32 = B;`) or a namespace-qualified one (`const A:
/// i32 = m::C;`) can name a definition in *another* file only through an import.
/// The referrer's own scope chain (`by_key`) and `::`-canonical paths (`by_path`)
/// never see those bindings, so without this translation every cross-file
/// definition cycle is invisible to the cycle check (#63).
///
/// This maps a referring file scope's import bindings to the canonical `::`-path
/// of what they bind, so a reference can be rewritten to a path that `by_path`
/// resolves:
/// - an item import `use lib::t::{B};` binds local `B` to canonical `lib::t::B`;
/// - a namespace import `use lib::m;` binds alias `m` to file path `lib::m`, so a
///   reference `m::C` rewrites to canonical `lib::m::C`.
struct ImportBindings {
    /// `(referring scope id, local item name)` -> canonical `::`-path of the item.
    items: FxHashMap<(u32, String), String>,
    /// `(referring scope id, namespace alias)` -> `::`-joined file path the alias
    /// names, so a qualified reference through the alias rewrites to canonical.
    namespaces: FxHashMap<(u32, String), String>,
}

impl ImportBindings {
    /// Collects the import bindings of every file scope that owns a tracked
    /// definition. Only those scopes can originate an edge, so unrelated scopes
    /// are skipped.
    fn collect(table: &SymbolTable, nodes: &[DefNode]) -> Self {
        let mut items = FxHashMap::default();
        let mut namespaces = FxHashMap::default();
        let mut seen_scopes: Vec<u32> = nodes.iter().map(|n| n.scope_id).collect();
        seen_scopes.sort_unstable();
        seen_scopes.dedup();
        for scope_id in seen_scopes {
            let Some(scope) = table.get_scope(scope_id) else {
                continue;
            };
            for (local_name, resolved) in &scope.resolved_imports {
                match &resolved.target {
                    ResolvedImportTarget::Item {
                        definition_scope_id,
                        ..
                    } => {
                        // Import aliasing is unsupported, so the local name equals
                        // the item's own name; the item's canonical path is thus its
                        // defining file path joined with the local name.
                        let file_path = table.module_path_of_scope(*definition_scope_id);
                        let canonical = if file_path.is_empty() {
                            local_name.clone()
                        } else {
                            format!("{file_path}::{local_name}")
                        };
                        items.insert((scope_id, local_name.clone()), canonical);
                    }
                    ResolvedImportTarget::Namespace {
                        scope_id: ns_scope_id,
                    } => {
                        let file_path = table.module_path_of_scope(*ns_scope_id);
                        namespaces.insert((scope_id, local_name.clone()), file_path);
                    }
                }
            }
        }
        ImportBindings { items, namespaces }
    }

    /// Rewrites a bare or namespace-qualified reference made from scope
    /// `from_scope` to the canonical `::`-path it names through an import, or
    /// `None` if no import binding applies.
    fn canonicalize(&self, from_scope: u32, reference: &str) -> Option<String> {
        if let Some((alias, rest)) = reference.split_once("::") {
            // `alias::Name` (and deeper) through a namespace import: replace the
            // alias with the file path it binds, leaving the remaining segments.
            let file_path = self.namespaces.get(&(from_scope, alias.to_string()))?;
            if file_path.is_empty() {
                return Some(rest.to_string());
            }
            return Some(format!("{file_path}::{rest}"));
        }
        self.items.get(&(from_scope, reference.to_string())).cloned()
    }
}

/// Builds and analyzes the definition-value graph for `nodes`.
///
/// `nodes` must list every top-level `const` across all files.
/// References resolve against the scope chains carried by each node and against
/// `table`'s resolved import bindings, so a cross-file reference expressed only
/// through an item or namespace import is discovered as an edge (#63).
pub(crate) fn analyze(arena: &AstArena, table: &SymbolTable, nodes: &[DefNode]) -> GraphOutcome {
    let imports = ImportBindings::collect(table, nodes);
    let graph = DefGraph::build(arena, nodes, &imports);
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
    fn build(arena: &AstArena, nodes: &'a [DefNode], imports: &ImportBindings) -> Self {
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
                _ => continue,
            }
            for name in referenced {
                // A self-edge (`const A = A;`) is a degenerate value cycle: the
                // node's value depends on its own, with no evaluation order. It is
                // recorded like any other edge so the cycle check rejects it.
                if let Some(target) = resolve_ref(&by_key, &by_path, imports, node, &name)
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
        let scope_id = self.nodes[to].scope_id;
        GraphOutcome::Cyclic {
            cycle,
            location,
            scope_id,
        }
    }
}

/// Resolves a referenced name (bare or `::`-qualified) to a node index.
///
/// Resolution proceeds in the order name lookup would:
/// 1. an absolute `::`-qualified reference (`lib::vals::V`) names a definition's
///    canonical path directly and resolves by exact match;
/// 2. a bare reference resolves along the referencing node's scope chain (own
///    file first, then the program root);
/// 3. failing both, the reference is rewritten through the referring file's
///    import bindings — an item import (`use lib::t::{B};`) or a namespace import
///    (`use lib::m;` then `m::C`) — and the resulting canonical path is matched.
///    This is how a cross-file reference reaches a definition the referring file
///    cannot otherwise name, so it is essential for cross-file cycle detection
///    (#63).
///
/// Returns `None` when the reference names something that is not a tracked
/// const (a function, a builtin, a local, an unimported name) — such references
/// never participate in a definition-value cycle.
fn resolve_ref(
    by_key: &FxHashMap<(u32, &str), usize>,
    by_path: &FxHashMap<String, usize>,
    imports: &ImportBindings,
    from: &DefNode,
    reference: &str,
) -> Option<usize> {
    if reference.contains("::") {
        if let Some(&idx) = by_path.get(reference) {
            return Some(idx);
        }
    } else {
        for &scope_id in &from.scope_chain {
            if let Some(&idx) = by_key.get(&(scope_id, reference)) {
                return Some(idx);
            }
        }
    }
    let canonical = imports.canonicalize(from.scope_id, reference)?;
    by_path.get(&canonical).copied()
}

/// Collects names referenced by a const initializer expression: bare
/// identifiers and the leaf of any `::`-qualified path. Recurses through the
/// arithmetic and access expression forms a const initializer can contain.
fn collect_expr_refs(arena: &AstArena, expr_id: ExprId, out: &mut Vec<String>) {
    // Parentheses and the arithmetic-mode annotations contribute no name of
    // their own and are peeled together, so `const B: i32 = checked(A + 1);`
    // records the same dependency on `A` that `(A + 1)` does. Losing it would
    // order the two constants by nothing.
    if let Some(inner) = arena.transparent_inner(expr_id) {
        collect_expr_refs(arena, inner, out);
        return;
    }
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
        Expr::PrefixUnary { expr, .. } => {
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

#[cfg(test)]
mod tests {
    use super::collect_expr_refs;
    use inference_ast::arena::AstArena;
    use inference_ast::ids::ExprId;
    use inference_ast::nodes::{ArithMode, Expr, ExprData, Ident, Location};

    fn push(arena: &mut AstArena, kind: Expr) -> ExprId {
        arena.exprs.alloc(ExprData {
            location: Location::default(),
            kind,
        })
    }

    fn name(arena: &mut AstArena, text: &str) -> ExprId {
        let ident = arena.idents.alloc(Ident {
            location: Location::default(),
            name: text.to_string(),
        });
        push(arena, Expr::Identifier(ident))
    }

    fn refs(arena: &AstArena, expr_id: ExprId) -> Vec<String> {
        let mut out = Vec::new();
        collect_expr_refs(arena, expr_id, &mut out);
        out
    }

    /// Every grouping spelling contributes the same dependency edges.
    ///
    /// A constant initializer decides the order two constants are laid out in.
    /// If an annotation hid the name inside it, `const B: i32 = checked(A + 1);`
    /// would look independent of `A`, which is how a constant gets emitted
    /// before the value it is built from.
    #[test]
    fn each_grouping_form_collects_the_same_references() {
        let mut arena = AstArena::default();
        let a = name(&mut arena, "A");
        let one = push(
            &mut arena,
            Expr::NumberLiteral {
                value: "1".to_string(),
            },
        );
        let sum = push(
            &mut arena,
            Expr::Binary {
                left: a,
                right: one,
                op: inference_ast::nodes::OperatorKind::Add,
            },
        );

        let bare = refs(&arena, sum);
        assert_eq!(bare, vec!["A".to_string()]);

        let wrappers: Vec<ExprId> = vec![
            push(&mut arena, Expr::Parenthesized { expr: sum }),
            push(
                &mut arena,
                Expr::ArithMode {
                    mode: ArithMode::Checked,
                    expr: sum,
                },
            ),
            push(
                &mut arena,
                Expr::ArithMode {
                    mode: ArithMode::Wrapping,
                    expr: sum,
                },
            ),
        ];
        for wrapper in wrappers {
            assert_eq!(
                refs(&arena, wrapper),
                bare,
                "a grouping form changed the collected references"
            );
        }
    }
}
