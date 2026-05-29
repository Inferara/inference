//! A035: Direct and mutual/indirect recursion is forbidden (Power of 10, Rule 1).
//!
//! Inference forbids all recursion so that the maximum stack depth of a program
//! is statically bounded. This rule builds a directed call graph keyed by the
//! canonical function name (mirroring the codegen [`FnKey`] Display scheme) and
//! reports each call cycle exactly once via a white/gray/black DFS, pointing the
//! diagnostic at the call site that closes the cycle.
//!
//! # Call resolution
//!
//! A call site carries only the callee *expression*, never the resolved callee
//! `DefId`. Rather than re-running full name resolution, the graph keys mirror
//! the strings codegen lowers to, which guarantees the same call targets the
//! compiler would actually emit. Resolution is deliberately conservative: an
//! edge is created only when it can be resolved to an existing graph node, so
//! the rule never produces a false positive.
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
use inference_type_checker::type_info::TypeInfoKind;

use crate::errors::AnalysisDiagnostic;
use crate::rule::TypedContext;
use crate::walker::{for_each_stmt_expr, walk_expr};

/// One outgoing call edge: the resolved *raw* (un-spec-prefixed) callee key, the
/// enclosing spec of the caller (for spec-first resolution), and the call-site
/// location used for the diagnostic.
struct CallEdge {
    callee_raw: String,
    spec: Option<String>,
    location: Location,
}

/// A function node in the call graph.
///
/// `key` is the canonical name matching the codegen `FnKey` Display scheme
/// (free `f`, spec-free `S.f`, method `T.m`, spec-method `S.T.m`). `display` is
/// the human-facing label used to render a cycle chain; today it equals `key`.
struct FnNode {
    key: String,
    display: String,
    edges: Vec<CallEdge>,
}

crate::rule! {
    /// Direct and mutual recursion is forbidden (Power of 10, Rule 1).
    #[id = "A035"]
    #[name = "Recursion detected"]
    #[severity = error]
    pub struct RecursionDetected;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let nodes = build_call_graph(ctx);
        detect_cycles(&nodes)
    }
}

/// Builds the whole-program call graph across every source file.
fn build_call_graph(ctx: &TypedContext) -> Vec<FnNode> {
    let arena = ctx.arena();
    let mut nodes: Vec<FnNode> = Vec::new();
    for source_file in ctx.source_files() {
        collect_defs(ctx, arena, &source_file.defs, None, None, &mut nodes);
    }
    nodes
}

/// Recurses through definitions (same shape as `missing_return::check_defs`),
/// tracking the enclosing spec name and struct/type name so the graph keys
/// match the codegen `FnKey` Display scheme. `ExternFunction` has no body and is
/// never a node.
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
fn fn_key(spec: Option<&str>, type_name: Option<&str>, fname: &str) -> String {
    match (spec, type_name) {
        (Some(s), Some(t)) => format!("{s}.{t}.{fname}"),
        (Some(s), None) => format!("{s}.{fname}"),
        (None, Some(t)) => format!("{t}.{fname}"),
        (None, None) => fname.to_string(),
    }
}

/// Resolves a callee expression to a *raw* (un-spec-prefixed) key, or `None`
/// when the callee form is unsupported or its receiver type is unknown.
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

const WHITE: u8 = 0;
const GRAY: u8 = 1;
const BLACK: u8 = 2;

/// Detects every call cycle in the graph and emits one diagnostic per cycle.
///
/// Each raw edge is resolved to a node index using spec-first resolution
/// (matching codegen's spec-active call probe): a bare name inside spec `S` is
/// first tried as `S.name`, then as the top-level `name`; if neither names an
/// existing node the edge is dropped. Because edges only ever target existing
/// nodes, the graph cannot manufacture a false cycle.
fn detect_cycles(nodes: &[FnNode]) -> Vec<AnalysisDiagnostic> {
    let index: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.key.as_str(), i))
        .collect();
    let known: HashSet<&str> = index.keys().copied().collect();

    let adj: Vec<Vec<(usize, Location)>> = nodes
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
        .collect();

    let mut color = vec![WHITE; nodes.len()];
    let mut stack: Vec<usize> = Vec::new();
    let mut reported: HashSet<Vec<usize>> = HashSet::new();
    let mut diags = Vec::new();
    for start in 0..nodes.len() {
        if color[start] == WHITE {
            dfs(
                start,
                &adj,
                nodes,
                &mut color,
                &mut stack,
                &mut reported,
                &mut diags,
            );
        }
    }
    diags
}

fn dfs(
    u: usize,
    adj: &[Vec<(usize, Location)>],
    nodes: &[FnNode],
    color: &mut [u8],
    stack: &mut Vec<usize>,
    reported: &mut HashSet<Vec<usize>>,
    diags: &mut Vec<AnalysisDiagnostic>,
) {
    color[u] = GRAY;
    stack.push(u);
    for &(v, call_loc) in &adj[u] {
        match color[v] {
            GRAY => {
                if let Some(canon) = cycle_from_back_edge(stack, v)
                    && reported.insert(canon.clone())
                {
                    diags.push(AnalysisDiagnostic::RecursionDetected {
                        cycle: render_cycle(nodes, &canon),
                        location: call_loc,
                    });
                }
            }
            WHITE => dfs(v, adj, nodes, color, stack, reported, diags),
            _ => {}
        }
    }
    color[u] = BLACK;
    stack.pop();
}

/// Reconstructs the cycle node-index list from a back edge to GRAY ancestor `v`.
///
/// The cycle is the slice of the DFS `stack` from the first occurrence of `v` to
/// the top. It is canonicalised by rotating so the minimum node index appears
/// first, which makes every rotation of the same cycle hash identically for
/// deduplication. A self-loop canonicalises to `[u]`.
fn cycle_from_back_edge(stack: &[usize], v: usize) -> Option<Vec<usize>> {
    let start = stack.iter().position(|&n| n == v)?;
    let slice = &stack[start..];
    let min_pos = slice
        .iter()
        .enumerate()
        .min_by_key(|&(_, &n)| n)
        .map(|(i, _)| i)?;
    let mut canon = Vec::with_capacity(slice.len());
    canon.extend_from_slice(&slice[min_pos..]);
    canon.extend_from_slice(&slice[..min_pos]);
    Some(canon)
}

/// Renders a cycle as `a -> b -> ... -> a` using node display labels.
fn render_cycle(nodes: &[FnNode], cycle: &[usize]) -> String {
    let mut chain = String::new();
    for &i in cycle {
        chain.push_str(&nodes[i].display);
        chain.push_str(" -> ");
    }
    chain.push_str(&nodes[cycle[0]].display);
    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc() -> Location {
        Location::default()
    }

    /// Builds a node whose outgoing edges are top-level (spec-free) raw keys.
    fn node(key: &str, callees: &[&str]) -> FnNode {
        FnNode {
            key: key.to_string(),
            display: key.to_string(),
            edges: callees
                .iter()
                .map(|c| CallEdge {
                    callee_raw: (*c).to_string(),
                    spec: None,
                    location: loc(),
                })
                .collect(),
        }
    }

    fn cycles(diags: &[AnalysisDiagnostic]) -> Vec<String> {
        diags
            .iter()
            .map(|d| match d {
                AnalysisDiagnostic::RecursionDetected { cycle, .. } => cycle.clone(),
                other => panic!("unexpected diagnostic: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn direct_self_recursion_reports_one_cycle() {
        let nodes = vec![node("f", &["f"])];
        let diags = detect_cycles(&nodes);
        assert_eq!(diags.len(), 1);
        assert_eq!(cycles(&diags), vec!["f -> f"]);
    }

    #[test]
    fn two_cycle_reported_once() {
        let nodes = vec![node("a", &["b"]), node("b", &["a"])];
        let diags = detect_cycles(&nodes);
        assert_eq!(diags.len(), 1);
        assert_eq!(cycles(&diags), vec!["a -> b -> a"]);
    }

    #[test]
    fn three_cycle_reported_once() {
        let nodes = vec![node("a", &["b"]), node("b", &["c"]), node("c", &["a"])];
        let diags = detect_cycles(&nodes);
        assert_eq!(diags.len(), 1);
        assert_eq!(cycles(&diags), vec!["a -> b -> c -> a"]);
    }

    #[test]
    fn non_recursive_chain_has_no_cycle() {
        let nodes = vec![node("a", &["b"]), node("b", &["c"]), node("c", &[])];
        let diags = detect_cycles(&nodes);
        assert!(diags.is_empty(), "expected no cycle, got: {:?}", cycles(&diags));
    }

    #[test]
    fn edge_to_unknown_callee_is_dropped() {
        // `a` calls an extern/unknown `ext` which is not a node: no cycle.
        let nodes = vec![node("a", &["ext"])];
        let diags = detect_cycles(&nodes);
        assert!(diags.is_empty());
    }

    #[test]
    fn two_independent_cycles_reported_separately() {
        let nodes = vec![
            node("a", &["b"]),
            node("b", &["a"]),
            node("c", &["d"]),
            node("d", &["c"]),
        ];
        let diags = detect_cycles(&nodes);
        assert_eq!(diags.len(), 2);
        let mut got = cycles(&diags);
        got.sort();
        assert_eq!(got, vec!["a -> b -> a", "c -> d -> c"]);
    }

    #[test]
    fn shared_cycle_reached_from_multiple_roots_deduped() {
        // Both `a` and `x` lead into the same `b <-> c` cycle.
        let nodes = vec![
            node("a", &["b"]),
            node("b", &["c"]),
            node("c", &["b"]),
            node("x", &["c"]),
        ];
        let diags = detect_cycles(&nodes);
        assert_eq!(diags.len(), 1);
        assert_eq!(cycles(&diags), vec!["b -> c -> b"]);
    }

    #[test]
    fn spec_first_resolution_prefers_spec_inner_callee() {
        // Inside spec `S`, bare `f` resolves to `S.f` when both exist.
        let nodes = vec![
            FnNode {
                key: "S.f".to_string(),
                display: "S.f".to_string(),
                edges: vec![CallEdge {
                    callee_raw: "f".to_string(),
                    spec: Some("S".to_string()),
                    location: loc(),
                }],
            },
            node("f", &[]),
        ];
        let diags = detect_cycles(&nodes);
        assert_eq!(diags.len(), 1);
        assert_eq!(cycles(&diags), vec!["S.f -> S.f"]);
    }

    #[test]
    fn method_self_cycle_reported() {
        // A method `T.m` resolved by call-site resolution to raw `T.m`.
        let nodes = vec![FnNode {
            key: "T.m".to_string(),
            display: "T.m".to_string(),
            edges: vec![CallEdge {
                callee_raw: "T.m".to_string(),
                spec: None,
                location: loc(),
            }],
        }];
        let diags = detect_cycles(&nodes);
        assert_eq!(diags.len(), 1);
        assert_eq!(cycles(&diags), vec!["T.m -> T.m"]);
    }

    #[test]
    fn fn_key_matches_codegen_display_scheme() {
        assert_eq!(fn_key(None, None, "f"), "f");
        assert_eq!(fn_key(Some("S"), None, "f"), "S.f");
        assert_eq!(fn_key(None, Some("T"), "m"), "T.m");
        assert_eq!(fn_key(Some("S"), Some("T"), "m"), "S.T.m");
    }
}
