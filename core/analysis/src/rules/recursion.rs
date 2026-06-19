//! A035: Direct and mutual/indirect recursion is forbidden (Power of 10, Rule 1).
//!
//! Inference forbids all recursion so that the maximum stack depth of a program
//! is statically bounded. This rule builds a directed call graph (see
//! [`crate::call_graph`]) keyed by the canonical function name (mirroring the
//! codegen `FnKey` Display scheme) and reports each call cycle exactly once via
//! a white/gray/black DFS, pointing the diagnostic at the call site that closes
//! the cycle.
//!
//! The call-graph construction and spec-first edge resolution are shared with
//! the stack-depth analysis (A036); see [`crate::call_graph`] for the
//! resolution rules and their known limitations.

use std::collections::HashSet;

use inference_ast::nodes::Location;

use crate::call_graph::{build_call_graph, resolve_adjacency, FnNode, BLACK, GRAY, WHITE};
use crate::errors::{AnalysisDiagnostic, LabeledDiagnostic};

crate::rule! {
    /// Direct and mutual recursion is forbidden (Power of 10, Rule 1).
    #[id = "A035"]
    #[name = "Recursion detected"]
    #[severity = error]
    pub struct RecursionDetected;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let nodes = build_call_graph(ctx);
        detect_cycles(&nodes)
    }
}

/// Detects every call cycle in the graph and emits one diagnostic per cycle.
///
/// The diagnostic points at the call site that closes the cycle, which lives in
/// the body of node `u`; that node's defining file names the finding.
fn detect_cycles(nodes: &[FnNode]) -> Vec<LabeledDiagnostic> {
    let adj = resolve_adjacency(nodes);

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
    diags: &mut Vec<LabeledDiagnostic>,
) {
    color[u] = GRAY;
    stack.push(u);
    for &(v, call_loc) in &adj[u] {
        match color[v] {
            GRAY => {
                if let Some(canon) = cycle_from_back_edge(stack, v)
                    && reported.insert(canon.clone())
                {
                    diags.push(LabeledDiagnostic::new(
                        nodes[u].module_path.clone(),
                        AnalysisDiagnostic::RecursionDetected {
                            cycle: render_cycle(nodes, &canon),
                            location: call_loc,
                        },
                    ));
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

/// Renders a cycle as `a -> b -> ... -> a` using each node's canonical key.
fn render_cycle(nodes: &[FnNode], cycle: &[usize]) -> String {
    let mut chain = String::new();
    for &i in cycle {
        chain.push_str(&nodes[i].key.to_string());
        chain.push_str(" -> ");
    }
    chain.push_str(&nodes[cycle[0]].key.to_string());
    chain
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call_graph::{test_node, CallEdge, FnNode};
    use inference_ast::ids::idx_from_u32;
    use inference_ast::nodes::Location;
    use inference_fn_key::FnKey;

    fn loc() -> Location {
        Location::default()
    }

    /// Builds a node with an explicit structured key and edges; placeholder
    /// def/body metadata since these tests exercise graph shape only.
    fn node(key: FnKey, edges: Vec<CallEdge>) -> FnNode {
        FnNode {
            key,
            edges,
            def_id: idx_from_u32(0),
            body: idx_from_u32(0),
            location: loc(),
            module_path: Vec::new(),
            struct_name: None,
        }
    }

    fn cycles(diags: &[LabeledDiagnostic]) -> Vec<String> {
        diags
            .iter()
            .map(|d| match &d.diagnostic {
                AnalysisDiagnostic::RecursionDetected { cycle, .. } => cycle.clone(),
                other => panic!("unexpected diagnostic: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn direct_self_recursion_reports_one_cycle() {
        let nodes = vec![test_node("f", &["f"])];
        let diags = detect_cycles(&nodes);
        assert_eq!(diags.len(), 1);
        assert_eq!(cycles(&diags), vec!["f -> f"]);
    }

    #[test]
    fn two_cycle_reported_once() {
        let nodes = vec![test_node("a", &["b"]), test_node("b", &["a"])];
        let diags = detect_cycles(&nodes);
        assert_eq!(diags.len(), 1);
        assert_eq!(cycles(&diags), vec!["a -> b -> a"]);
    }

    #[test]
    fn three_cycle_reported_once() {
        let nodes = vec![test_node("a", &["b"]), test_node("b", &["c"]), test_node("c", &["a"])];
        let diags = detect_cycles(&nodes);
        assert_eq!(diags.len(), 1);
        assert_eq!(cycles(&diags), vec!["a -> b -> c -> a"]);
    }

    #[test]
    fn non_recursive_chain_has_no_cycle() {
        let nodes = vec![test_node("a", &["b"]), test_node("b", &["c"]), test_node("c", &[])];
        let diags = detect_cycles(&nodes);
        assert!(diags.is_empty(), "expected no cycle, got: {:?}", cycles(&diags));
    }

    #[test]
    fn edge_to_unknown_callee_is_dropped() {
        // `a` calls an extern/unknown `ext` which is not a node: no cycle.
        let nodes = vec![test_node("a", &["ext"])];
        let diags = detect_cycles(&nodes);
        assert!(diags.is_empty());
    }

    #[test]
    fn two_independent_cycles_reported_separately() {
        let nodes = vec![
            test_node("a", &["b"]),
            test_node("b", &["a"]),
            test_node("c", &["d"]),
            test_node("d", &["c"]),
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
            test_node("a", &["b"]),
            test_node("b", &["c"]),
            test_node("c", &["b"]),
            test_node("x", &["c"]),
        ];
        let diags = detect_cycles(&nodes);
        assert_eq!(diags.len(), 1);
        assert_eq!(cycles(&diags), vec!["b -> c -> b"]);
    }

    #[test]
    fn spec_first_resolution_prefers_spec_inner_callee() {
        // Inside spec `S`, bare `f` resolves to `S.f` when both exist.
        let nodes = vec![
            node(
                FnKey::spec_free_folded(&[], "S", "f"),
                vec![CallEdge {
                    name: "f".to_string(),
                    receiver_struct: None,
                    module_path: Vec::new(),
                    spec: Some("S".to_string()),
                    location: loc(),
                }],
            ),
            test_node("f", &[]),
        ];
        let diags = detect_cycles(&nodes);
        assert_eq!(diags.len(), 1);
        assert_eq!(cycles(&diags), vec!["S.f -> S.f"]);
    }

    #[test]
    fn method_self_cycle_reported() {
        // A method `T.m` whose body calls itself (`recv.m()` -> receiver struct
        // `T`, name `m`).
        let nodes = vec![node(
            FnKey::method_in(Vec::new(), "T", "m"),
            vec![CallEdge {
                name: "m".to_string(),
                receiver_struct: Some("T".to_string()),
                module_path: Vec::new(),
                spec: None,
                location: loc(),
            }],
        )];
        let diags = detect_cycles(&nodes);
        assert_eq!(diags.len(), 1);
        assert_eq!(cycles(&diags), vec!["T.m -> T.m"]);
    }
}
