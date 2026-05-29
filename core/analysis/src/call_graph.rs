//! Whole-program call graph shared by the recursion (A035) and stack-depth
//! (A036) analyses.
//!
//! Both rules need the same directed graph of function definitions keyed by the
//! canonical name codegen lowers to (the [`FnKey`] Display scheme: free `f`,
//! spec-free `S.f`, method `T.m`, spec-method `S.T.m`). This module builds that
//! graph once and exposes the spec-first edge resolution that turns raw callee
//! keys into node indices.
//!
//! # Call resolution
//!
//! A call site carries only the callee *expression*, never the resolved callee
//! `DefId`. Rather than re-running full name resolution, the graph keys mirror
//! the strings codegen lowers to, which guarantees the same call targets the
//! compiler would actually emit. Resolution is deliberately conservative: an
//! edge is created only when it can be resolved to an existing graph node, so
//! callers never produce a false positive.
//!
//! Resolved forms (see [`resolve_callee_raw`]):
//! - `Expr::Identifier(name)` — a free or spec-inner free function.
//! - `Expr::TypeMemberAccess { Type::assoc }` — an associated function `T.assoc`.
//! - `Expr::MemberAccess { recv.m }` — a method, when the receiver's type is a
//!   known struct; otherwise the edge is dropped.
//! - any other callee form (e.g. a function-valued expression) is dropped.
//!
//! [`FnKey`]: (codegen-internal; mirrored here, not imported)

use std::collections::{HashMap, HashSet};

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, ExprId, NodeId, StmtId};
use inference_ast::nodes::{Def, Expr, Location, Stmt, TypeNode};
#[cfg(test)]
use inference_ast::ids::idx_from_u32;
use inference_type_checker::type_info::TypeInfoKind;

use crate::rule::TypedContext;
use crate::walker::{for_each_stmt_expr, walk_expr};

/// One outgoing call edge: the resolved *raw* (un-spec-prefixed) callee key, the
/// enclosing spec of the caller (for spec-first resolution), and the call-site
/// location used for the diagnostic.
pub(crate) struct CallEdge {
    pub(crate) callee_raw: String,
    pub(crate) spec: Option<String>,
    pub(crate) location: Location,
}

/// A function node in the call graph.
///
/// `key` is the canonical name matching the codegen `FnKey` Display scheme
/// (free `f`, spec-free `S.f`, method `T.m`, spec-method `S.T.m`). `display` is
/// the human-facing label used to render a chain; today it equals `key`.
///
/// The remaining fields carry just enough of the definition for downstream
/// rules that weight or inspect each node (A036's frame-size estimator): the
/// `DefId` of the function, the body block, and the enclosing struct name (so a
/// mutable-`self` frame slot can be sized). A035 ignores them.
pub(crate) struct FnNode {
    pub(crate) key: String,
    pub(crate) display: String,
    pub(crate) edges: Vec<CallEdge>,
    pub(crate) def_id: DefId,
    pub(crate) body: BlockId,
    pub(crate) location: Location,
    /// Name of the struct that owns this function when it is a method, used to
    /// size a mutable-`self` frame slot. `None` for free and associated
    /// functions.
    pub(crate) struct_name: Option<String>,
}

/// Builds the whole-program call graph across every source file.
pub(crate) fn build_call_graph(ctx: &TypedContext) -> Vec<FnNode> {
    let arena = ctx.arena();
    let mut nodes: Vec<FnNode> = Vec::new();
    for source_file in ctx.source_files() {
        collect_defs(ctx, arena, &source_file.defs, None, None, &mut nodes);
    }
    nodes
}

/// Recurses through definitions, tracking the enclosing spec name and
/// struct/type name so the graph keys match the codegen `FnKey` Display scheme.
/// `ExternFunction` has no body and is never a node.
fn collect_defs(
    ctx: &TypedContext,
    arena: &AstArena,
    def_ids: &[DefId],
    spec: Option<&str>,
    type_name: Option<&str>,
    nodes: &mut Vec<FnNode>,
) {
    for &def_id in def_ids {
        match &arena[def_id].kind {
            Def::Function { name, body, .. } => {
                let fname = arena[*name].name.clone();
                let key = fn_key(spec, type_name, &fname);
                let mut edges = Vec::new();
                collect_calls_in_block(ctx, arena, *body, spec, &mut edges);
                nodes.push(FnNode {
                    display: key.clone(),
                    key,
                    edges,
                    def_id,
                    body: *body,
                    location: arena[def_id].location,
                    struct_name: type_name.map(str::to_string),
                });
            }
            Def::Struct { name, methods, .. } => {
                let tn = arena[*name].name.clone();
                collect_defs(ctx, arena, methods, spec, Some(&tn), nodes);
            }
            Def::Spec { name, defs, .. } => {
                let sn = arena[*name].name.clone();
                collect_defs(ctx, arena, defs, Some(&sn), type_name, nodes);
            }
            Def::Module { defs: Some(d), .. } => {
                collect_defs(ctx, arena, d, spec, type_name, nodes);
            }
            Def::Enum { .. }
            | Def::Constant { .. }
            | Def::ExternFunction { .. }
            | Def::TypeAlias { .. }
            | Def::Module { defs: None, .. } => {}
        }
    }
}

/// Visits every statement of `body` (recursing into nested `If`/`Loop`/`Block`
/// sub-blocks) and every sub-expression, collecting one edge per
/// `Expr::FunctionCall` whose callee resolves.
fn collect_calls_in_block(
    ctx: &TypedContext,
    arena: &AstArena,
    body: BlockId,
    spec: Option<&str>,
    edges: &mut Vec<CallEdge>,
) {
    for &stmt_id in &arena[body].stmts {
        collect_calls_in_stmt(ctx, arena, stmt_id, spec, edges);
    }
}

fn collect_calls_in_stmt(
    ctx: &TypedContext,
    arena: &AstArena,
    stmt_id: StmtId,
    spec: Option<&str>,
    edges: &mut Vec<CallEdge>,
) {
    for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
        walk_expr(arena, expr_id, &mut |sub| {
            if let Expr::FunctionCall { function, .. } = &arena[sub].kind
                && let Some(raw) = resolve_callee_raw(ctx, *function)
            {
                edges.push(CallEdge {
                    callee_raw: raw,
                    spec: spec.map(str::to_string),
                    location: arena[sub].location,
                });
            }
        });
    });
    // `for_each_stmt_expr` yields only a statement's own expressions; it does
    // not descend into nested control-flow blocks, so recurse explicitly.
    match &arena[stmt_id].kind {
        Stmt::If {
            then_block,
            else_block,
            ..
        } => {
            collect_calls_in_block(ctx, arena, *then_block, spec, edges);
            if let Some(else_id) = else_block {
                collect_calls_in_block(ctx, arena, *else_id, spec, edges);
            }
        }
        Stmt::Loop { body, .. } => collect_calls_in_block(ctx, arena, *body, spec, edges),
        Stmt::Block(b) => collect_calls_in_block(ctx, arena, *b, spec, edges),
        _ => {}
    }
}

/// Builds a canonical node key matching the codegen `FnKey` Display scheme.
pub(crate) fn fn_key(spec: Option<&str>, type_name: Option<&str>, fname: &str) -> String {
    match (spec, type_name) {
        (Some(s), Some(t)) => format!("{s}.{t}.{fname}"),
        (Some(s), None) => format!("{s}.{fname}"),
        (None, Some(t)) => format!("{t}.{fname}"),
        (None, None) => fname.to_string(),
    }
}

/// Resolves a callee expression to a *raw* (un-spec-prefixed) key, or `None`
/// when the callee form is unsupported or its receiver type is unknown.
///
/// Known limitation: a `recv.method()` whose receiver type only resolves via
/// [`TypedContext::lookup_struct`] is dropped when that lookup misses. The
/// symbol table does not recurse into `Def::Module.defs`, so a method of a
/// module-nested struct is not visible there and its edge would go unrecorded.
/// This is unreachable today (the grammar has no module-body syntax — see the
/// `unimplemented!` in `core/ast/src/builder.rs`) but must be revisited when
/// modules land, alongside the matching `lookup_struct` gap.
fn resolve_callee_raw(ctx: &TypedContext, function: ExprId) -> Option<String> {
    let arena = ctx.arena();
    match &arena[function].kind {
        Expr::Identifier(id) => Some(arena[*id].name.clone()),
        Expr::TypeMemberAccess { expr, name } => {
            let tn = type_name_of(arena, *expr)?;
            Some(format!("{tn}.{}", arena[*name].name))
        }
        Expr::MemberAccess { expr, name } => {
            let recv = ctx.get_node_typeinfo(NodeId::Expr(*expr))?;
            let tn = match &recv.kind {
                TypeInfoKind::Struct(t) => t.clone(),
                TypeInfoKind::Custom(t) if ctx.lookup_struct(t).is_some() => t.clone(),
                _ => return None,
            };
            Some(format!("{tn}.{}", arena[*name].name))
        }
        _ => None,
    }
}

/// Extracts a type name from the left-hand side of a `TypeMemberAccess`
/// (`Type::assoc()`), accepting either a bare identifier or a `Custom` type.
fn type_name_of(arena: &AstArena, expr: ExprId) -> Option<String> {
    match &arena[expr].kind {
        Expr::Identifier(id) => Some(arena[*id].name.clone()),
        Expr::Type(ty) => match &arena[*ty].kind {
            TypeNode::Custom(id) => Some(arena[*id].name.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Resolves each node's raw edges into a directed adjacency list of node
/// indices, paired with the call-site location of the edge.
///
/// Each raw edge is resolved with spec-first resolution (matching codegen's
/// spec-active call probe): a bare name inside spec `S` is first tried as
/// `S.name`, then as the top-level `name`; if neither names an existing node
/// the edge is dropped. Because edges only ever target existing nodes, callers
/// can never manufacture a false edge.
pub(crate) fn resolve_adjacency(nodes: &[FnNode]) -> Vec<Vec<(usize, Location)>> {
    let index: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.key.as_str(), i))
        .collect();
    let known: HashSet<&str> = index.keys().copied().collect();

    nodes
        .iter()
        .map(|n| {
            n.edges
                .iter()
                .filter_map(|e| {
                    let spec_key = e.spec.as_ref().map(|s| format!("{s}.{}", e.callee_raw));
                    let resolved = spec_key
                        .as_deref()
                        .filter(|k| known.contains(k))
                        .or_else(|| known.get(e.callee_raw.as_str()).copied());
                    resolved.and_then(|k| index.get(k).map(|&j| (j, e.location)))
                })
                .collect()
        })
        .collect()
}

/// DFS coloring used by the cycle-aware traversals in both rules.
pub(crate) const WHITE: u8 = 0;
pub(crate) const GRAY: u8 = 1;
pub(crate) const BLACK: u8 = 2;

/// Builds a graph node whose outgoing edges are top-level (spec-free) raw keys,
/// for the unit tests of the rules that share this module. The body/def
/// metadata is irrelevant to graph-shape tests, so it is filled with
/// arena-index placeholders.
#[cfg(test)]
pub(crate) fn test_node(key: &str, callees: &[&str]) -> FnNode {
    FnNode {
        key: key.to_string(),
        display: key.to_string(),
        edges: callees
            .iter()
            .map(|c| CallEdge {
                callee_raw: (*c).to_string(),
                spec: None,
                location: Location::default(),
            })
            .collect(),
        def_id: idx_from_u32(0),
        body: idx_from_u32(0),
        location: Location::default(),
        struct_name: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc() -> Location {
        Location::default()
    }

    #[test]
    fn adjacency_resolves_known_edges_and_drops_unknown() {
        let nodes = vec![test_node("a", &["b", "ext"]), test_node("b", &[])];
        let adj = resolve_adjacency(&nodes);
        assert_eq!(adj[0].len(), 1, "edge to unknown `ext` must be dropped");
        assert_eq!(adj[0][0].0, 1, "edge `a -> b` must resolve to node index 1");
        assert!(adj[1].is_empty());
    }

    #[test]
    fn adjacency_spec_first_prefers_spec_inner_node() {
        let nodes = vec![
            FnNode {
                key: "S.f".to_string(),
                display: "S.f".to_string(),
                edges: vec![CallEdge {
                    callee_raw: "f".to_string(),
                    spec: Some("S".to_string()),
                    location: loc(),
                }],
                def_id: idx_from_u32(0),
                body: idx_from_u32(0),
                location: loc(),
                struct_name: None,
            },
            test_node("f", &[]),
        ];
        let adj = resolve_adjacency(&nodes);
        assert_eq!(adj[0].len(), 1);
        assert_eq!(adj[0][0].0, 0, "bare `f` inside spec `S` must resolve to `S.f`");
    }

    #[test]
    fn fn_key_matches_codegen_display_scheme() {
        assert_eq!(fn_key(None, None, "f"), "f");
        assert_eq!(fn_key(Some("S"), None, "f"), "S.f");
        assert_eq!(fn_key(None, Some("T"), "m"), "T.m");
        assert_eq!(fn_key(Some("S"), Some("T"), "m"), "S.T.m");
    }
}
