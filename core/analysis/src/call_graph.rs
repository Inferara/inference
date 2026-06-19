//! Whole-program call graph shared by the recursion (A035) and stack-depth
//! (A036) analyses.
//!
//! Both rules need the same directed graph of function definitions keyed by the
//! [`FnKey`] codegen lowers to — the structured identity that distinguishes a
//! free `f`, spec-free `S.f`, method `T.m`, and spec-method `S.T.m`, each
//! qualified by the defining file. This module builds that graph once and
//! exposes the spec-first edge resolution that turns recorded call targets into
//! node indices.
//!
//! # Why the structured key matters
//!
//! A flat `.`-joined string conflates the module-path join with the
//! struct-method join: a struct `mid`'s associated `make` in file `a`
//! (`a.mid.make`) and a free `make` in the sibling file `a/mid`
//! (`a.mid.make`) render identically, so one node would hijack the other's
//! adjacency slot and mask a recursion cycle. Keying on [`FnKey`] — the same
//! type codegen uses to assign WASM function indices — makes
//! `Method { module_path: ["a"], struct_name: "mid", name: "make" }` and
//! `Free { module_path: ["a", "mid"], name: "make" }` distinct by construction,
//! so the two phases agree on identity without re-deriving it.
//!
//! # Module-path identity
//!
//! Node keys and edge targets are qualified by the **defining file's** module
//! path (empty for the entry file, so a single-file program's keys stay
//! unqualified). For a method the qualifier is the **struct's** defining file,
//! not the call site's. Spec functions instead fold their defining file into the
//! spec name (`lib_checks_S`) with an empty module path, exactly as codegen does
//! (see [`inference_fn_key::fold_spec_name`]).
//!
//! # Call resolution
//!
//! Type checking already resolves every non-extern call to its defining file via
//! [`TypedContext::call_target`], which records the callee's `module_path`, bare
//! `name`, and (for a namespaced associated function or an instance method) its
//! `receiver_struct`. Edge collection consumes that recorded target first, so a
//! qualified or re-exported call resolves to the same file-qualified node
//! codegen emits. When no target was recorded (e.g. a spec-inner call, or a
//! higher-order callee), it falls back to the structural [`resolve_callee_raw`]
//! against the **caller's** file. An `external fn` import has no node, so its
//! edge is always dropped — calls into externs cannot recurse back into this
//! module.
//!
//! Resolution is deliberately conservative: an edge is created only when it
//! resolves to an existing graph node, so callers never produce a false
//! positive.

use std::collections::HashMap;

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, ExprId, NodeId, StmtId};
use inference_ast::nodes::{Def, Expr, Location, Stmt, TypeNode};
#[cfg(test)]
use inference_ast::ids::idx_from_u32;
use inference_fn_key::FnKey;
use inference_type_checker::type_info::TypeInfoKind;

use crate::rule::TypedContext;
use crate::walker::{for_each_stmt_expr, walk_expr};

/// One outgoing call edge, carrying the resolved callee's identity in the same
/// shape [`FnKey`] expects so [`resolve_adjacency`] can build the candidate key
/// without re-flattening.
///
/// The three identity fields have three different provenances and must not be
/// collapsed: `module_path` is the callee's *defining* file (from
/// [`TypedContext::call_target`]) or, for the structural fallback, the *caller's*
/// file; `receiver_struct` and `name` come from the recorded call target; `spec`
/// is the *caller's* enclosing spec (call targets carry no spec), used for
/// spec-first resolution within the same file. `location` is the call-site
/// location used for the diagnostic.
pub(crate) struct CallEdge {
    /// The callee's bare name (free `f`, method/assoc `m`).
    pub(crate) name: String,
    /// `Some(struct)` when the callee is a method or associated function;
    /// `None` for a free function.
    pub(crate) receiver_struct: Option<String>,
    /// The callee's defining file (or the caller's, for the structural
    /// fallback). Empty for the entry file.
    pub(crate) module_path: Vec<String>,
    /// The caller's enclosing spec, used to prefer a spec-inner callee.
    pub(crate) spec: Option<String>,
    pub(crate) location: Location,
}

/// A function node in the call graph.
///
/// `key` is the canonical [`FnKey`] codegen lowers this function to, including
/// the defining file's qualifier; it is the node's identity. A cross-file
/// cycle/chain diagnostic renders `key.to_string()` so it names the offending
/// file.
///
/// The remaining fields carry just enough of the definition for downstream rules
/// that weight or inspect each node (A036's frame-size estimator): the `DefId`
/// of the function, the body block, the defining file's `module_path` (so a
/// cross-file struct field type resolves to its own file's layout), and the
/// enclosing struct name (so a mutable-`self` frame slot can be sized). A035
/// uses only `key`/`edges`.
pub(crate) struct FnNode {
    pub(crate) key: FnKey,
    pub(crate) edges: Vec<CallEdge>,
    pub(crate) def_id: DefId,
    pub(crate) body: BlockId,
    pub(crate) location: Location,
    /// Source-root-relative path of the file that defines this function; empty
    /// for the entry file.
    pub(crate) module_path: Vec<String>,
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
        collect_defs(
            ctx,
            arena,
            &source_file.defs,
            &source_file.module_path,
            None,
            None,
            &mut nodes,
        );
    }
    nodes
}

/// Recurses through definitions, tracking the defining file's module path, the
/// enclosing spec name, and the enclosing struct/type name so the graph keys
/// match the [`FnKey`] codegen assigns. `ExternFunction` has no body and is
/// never a node.
#[allow(clippy::too_many_arguments)]
fn collect_defs(
    ctx: &TypedContext,
    arena: &AstArena,
    def_ids: &[DefId],
    module_path: &[String],
    spec: Option<&str>,
    type_name: Option<&str>,
    nodes: &mut Vec<FnNode>,
) {
    for &def_id in def_ids {
        match &arena[def_id].kind {
            Def::Function { name, body, .. } => {
                let fname = arena[*name].name.clone();
                let key = node_key(module_path, spec, type_name, &fname);
                let mut edges = Vec::new();
                collect_calls_in_block(ctx, arena, *body, module_path, spec, &mut edges);
                nodes.push(FnNode {
                    key,
                    edges,
                    def_id,
                    body: *body,
                    location: arena[def_id].location,
                    module_path: module_path.to_vec(),
                    struct_name: type_name.map(str::to_string),
                });
            }
            Def::Struct { name, methods, .. } => {
                let tn = arena[*name].name.clone();
                collect_defs(ctx, arena, methods, module_path, spec, Some(&tn), nodes);
            }
            Def::Spec { name, defs, .. } => {
                let sn = arena[*name].name.clone();
                collect_defs(ctx, arena, defs, module_path, Some(&sn), type_name, nodes);
            }
            Def::Enum { .. }
            | Def::Constant { .. }
            | Def::ExternFunction { .. }
            | Def::TypeAlias { .. } => {}
        }
    }
}

/// Builds the canonical [`FnKey`] for a definition from the four facts
/// `collect_defs` tracks. The spec variants fold the defining file into the spec
/// name (matching codegen); the free/method variants carry the module path.
fn node_key(
    module_path: &[String],
    spec: Option<&str>,
    type_name: Option<&str>,
    fname: &str,
) -> FnKey {
    match (spec, type_name) {
        (Some(s), Some(t)) => FnKey::spec_method_folded(module_path, s, t, fname),
        (Some(s), None) => FnKey::spec_free_folded(module_path, s, fname),
        (None, Some(t)) => FnKey::method_in(module_path.to_vec(), t, fname),
        (None, None) => FnKey::free_in(module_path.to_vec(), fname),
    }
}

/// Visits every statement of `body` (recursing into nested `If`/`Loop`/`Block`
/// sub-blocks) and every sub-expression, collecting one edge per
/// `Expr::FunctionCall` whose callee resolves. `module_path` is the *caller's*
/// defining file, used for the structural-resolution fallback.
fn collect_calls_in_block(
    ctx: &TypedContext,
    arena: &AstArena,
    body: BlockId,
    module_path: &[String],
    spec: Option<&str>,
    edges: &mut Vec<CallEdge>,
) {
    for &stmt_id in &arena[body].stmts {
        collect_calls_in_stmt(ctx, arena, stmt_id, module_path, spec, edges);
    }
}

fn collect_calls_in_stmt(
    ctx: &TypedContext,
    arena: &AstArena,
    stmt_id: StmtId,
    module_path: &[String],
    spec: Option<&str>,
    edges: &mut Vec<CallEdge>,
) {
    for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
        walk_expr(arena, expr_id, &mut |sub| {
            if let Expr::FunctionCall { function, .. } = &arena[sub].kind
                && let Some(callee) = resolve_callee(ctx, *function, module_path)
            {
                edges.push(CallEdge {
                    name: callee.name,
                    receiver_struct: callee.receiver_struct,
                    module_path: callee.module_path,
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
            collect_calls_in_block(ctx, arena, *then_block, module_path, spec, edges);
            if let Some(else_id) = else_block {
                collect_calls_in_block(ctx, arena, *else_id, module_path, spec, edges);
            }
        }
        Stmt::Loop { body, .. } => collect_calls_in_block(ctx, arena, *body, module_path, spec, edges),
        Stmt::Block(b) => collect_calls_in_block(ctx, arena, *b, module_path, spec, edges),
        _ => {}
    }
}

/// The identity of a resolved callee, kept structured (never pre-flattened) so
/// [`resolve_adjacency`] builds the [`FnKey`] candidate directly.
struct ResolvedCallee {
    name: String,
    receiver_struct: Option<String>,
    module_path: Vec<String>,
}

/// Resolves a callee expression to its structured identity.
///
/// Type checking's recorded [`TypedContext::call_target`] is consulted first: it
/// resolves every non-extern call — including `::`-qualified module calls,
/// `root::` calls back into the entry file, re-exported paths, instance methods,
/// and associated functions — to the callee's actual defining file, bare name,
/// and (for a method/assoc) receiver struct. A structural walk of the call
/// expression cannot do this (a nested `TypeMemberAccess` for `lib::b::pong`
/// does not name a known type, and `root::ping` has no struct receiver), which
/// is why dropping back to [`resolve_callee_raw`] alone silently lost every
/// cross-file qualified edge.
///
/// When no target was recorded, the structural fallback runs against the
/// caller's `caller_module_path` (covering spec-inner free calls and any
/// untracked form). `None` means the edge cannot be resolved and is dropped.
fn resolve_callee(
    ctx: &TypedContext,
    function: ExprId,
    caller_module_path: &[String],
) -> Option<ResolvedCallee> {
    if let Some(target) = ctx.call_target(function) {
        return Some(ResolvedCallee {
            name: target.name.clone(),
            receiver_struct: target.receiver_struct.clone(),
            module_path: target.module_path.clone(),
        });
    }
    resolve_callee_raw(ctx, function).map(|(name, receiver_struct)| ResolvedCallee {
        name,
        receiver_struct,
        module_path: caller_module_path.to_vec(),
    })
}

/// Resolves a callee expression to its `(name, receiver_struct)`, or `None` when
/// the callee form is unsupported or its receiver type is unknown. Used only as
/// the fallback for calls type checking did not record a
/// [`TypedContext::call_target`] for.
///
/// Known limitation: a `recv.method()` whose receiver type only resolves via
/// [`TypedContext::lookup_struct`] is dropped when that lookup misses, so its
/// edge goes unrecorded.
fn resolve_callee_raw(ctx: &TypedContext, function: ExprId) -> Option<(String, Option<String>)> {
    let arena = ctx.arena();
    match &arena[function].kind {
        Expr::Identifier(id) => Some((arena[*id].name.clone(), None)),
        Expr::TypeMemberAccess { expr, name } => {
            let tn = type_name_of(arena, *expr)?;
            Some((arena[*name].name.clone(), Some(tn)))
        }
        Expr::MemberAccess { expr, name } => {
            let recv = ctx.get_node_typeinfo(NodeId::Expr(*expr))?;
            let tn = match &recv.kind {
                TypeInfoKind::Struct(t, _) => t.clone(),
                TypeInfoKind::Custom(t) if ctx.lookup_struct(t).is_some() => t.clone(),
                _ => return None,
            };
            Some((arena[*name].name.clone(), Some(tn)))
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

/// Resolves each node's edges into a directed adjacency list of node indices,
/// paired with the call-site location of the edge.
///
/// Each edge is turned into the [`FnKey`] candidate(s) its callee names and
/// resolved spec-first (matching codegen's spec-active call probe): inside spec
/// `S` a callee is first tried as the spec-inner key, then as the top-level key.
/// The candidates are built directly from the edge's structured identity, so a
/// method (`Method`/`SpecMethod`) and a same-named sibling-file free function
/// (`Free`) target distinct nodes. If no candidate names an existing node the
/// edge is dropped, so callers can never manufacture a false edge.
pub(crate) fn resolve_adjacency(nodes: &[FnNode]) -> Vec<Vec<(usize, Location)>> {
    // Build the key → index map with an explicit insert so a duplicate key is a
    // loud failure rather than a silent last-wins overwrite. Two nodes sharing a
    // key would mean a recursive function's self-edge could resolve to a
    // different node, masking the cycle from the recursion check (A035).
    // `FnKey` is injective by construction and the type checker rejects genuine
    // duplicate definitions first, so a collision here is an upstream invariant
    // break; surface it in debug builds and keep the first node in release so the
    // graph stays usable.
    let mut index: HashMap<&FnKey, usize> = HashMap::with_capacity(nodes.len());
    for (i, n) in nodes.iter().enumerate() {
        if let Some(&existing) = index.get(&n.key) {
            debug_assert!(
                false,
                "duplicate call-graph key `{}` at node {i} (already node {existing}); \
                 FnKey must be injective or A035/A036 may miss a self-edge",
                n.key
            );
            continue;
        }
        index.insert(&n.key, i);
    }

    nodes
        .iter()
        .map(|n| {
            n.edges
                .iter()
                .filter_map(|e| {
                    candidate_keys(e)
                        .iter()
                        .find_map(|k| index.get(k).copied())
                        .map(|j| (j, e.location))
                })
                .collect()
        })
        .collect()
}

/// Builds the spec-first list of [`FnKey`] candidates a [`CallEdge`] may target,
/// most-specific first.
///
/// Inside spec `S` a bare callee `f` is first tried as the spec-inner key
/// (`SpecFree`/`SpecMethod`, with the callee's file folded into the spec name)
/// and then as the top-level key (`Free`/`Method`), mirroring codegen's
/// spec-active call probe. Outside any spec only the top-level key applies. A
/// receiver struct selects the method variants; its absence selects the free
/// variants.
fn candidate_keys(edge: &CallEdge) -> Vec<FnKey> {
    let mut candidates = Vec::new();
    match (&edge.spec, &edge.receiver_struct) {
        (Some(spec), Some(struct_name)) => {
            candidates.push(FnKey::spec_method_folded(
                &edge.module_path,
                spec,
                struct_name,
                &edge.name,
            ));
            candidates.push(FnKey::method_in(
                edge.module_path.clone(),
                struct_name,
                &edge.name,
            ));
        }
        (Some(spec), None) => {
            candidates.push(FnKey::spec_free_folded(&edge.module_path, spec, &edge.name));
            candidates.push(FnKey::free_in(edge.module_path.clone(), &edge.name));
        }
        (None, Some(struct_name)) => {
            candidates.push(FnKey::method_in(
                edge.module_path.clone(),
                struct_name,
                &edge.name,
            ));
        }
        (None, None) => {
            candidates.push(FnKey::free_in(edge.module_path.clone(), &edge.name));
        }
    }
    candidates
}

/// DFS coloring used by the cycle-aware traversals in both rules.
pub(crate) const WHITE: u8 = 0;
pub(crate) const GRAY: u8 = 1;
pub(crate) const BLACK: u8 = 2;

/// Builds a graph node for an entry-file free function whose outgoing edges are
/// entry-file free callees, for the unit tests of the rules that share this
/// module. The body/def metadata is irrelevant to graph-shape tests, so it is
/// filled with arena-index placeholders.
#[cfg(test)]
pub(crate) fn test_node(name: &str, callees: &[&str]) -> FnNode {
    FnNode {
        key: FnKey::free_in(Vec::new(), name),
        edges: callees
            .iter()
            .map(|c| CallEdge {
                name: (*c).to_string(),
                receiver_struct: None,
                module_path: Vec::new(),
                spec: None,
                location: Location::default(),
            })
            .collect(),
        def_id: idx_from_u32(0),
        body: idx_from_u32(0),
        location: Location::default(),
        module_path: Vec::new(),
        struct_name: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc() -> Location {
        Location::default()
    }

    fn path(segs: &[&str]) -> Vec<String> {
        segs.iter().map(|s| (*s).to_string()).collect()
    }

    /// A free-function node with explicit key and edges, for tests that need a
    /// non-entry-file or structured key.
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

    fn free_edge(name: &str, module_path: Vec<String>) -> CallEdge {
        CallEdge {
            name: name.to_string(),
            receiver_struct: None,
            module_path,
            spec: None,
            location: loc(),
        }
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
        let adj = resolve_adjacency(&nodes);
        assert_eq!(adj[0].len(), 1);
        assert_eq!(adj[0][0].0, 0, "bare `f` inside spec `S` must resolve to `S.f`");
    }

    #[test]
    fn adjacency_resolves_cross_file_edge_by_callee_module_path() {
        // Entry `ping` calls `lib.b.pong`; the edge carries the callee's defining
        // module path so it resolves to the qualified node, not a dropped one.
        let lib_b = path(&["lib", "b"]);
        let nodes = vec![
            node(
                FnKey::free_in(Vec::new(), "ping"),
                vec![free_edge("pong", lib_b.clone())],
            ),
            node(FnKey::free_in(lib_b, "pong"), vec![]),
        ];
        let adj = resolve_adjacency(&nodes);
        assert_eq!(adj[0].len(), 1, "cross-file edge must resolve");
        assert_eq!(adj[0][0].0, 1, "`ping` must point at `lib.b.pong`");
    }

    #[test]
    fn adjacency_keeps_same_named_cross_file_nodes_distinct() {
        // Two files each define `helper`; an edge into one must not collapse onto
        // the other. The entry `helper` and `lib.x.helper` are distinct nodes,
        // and an edge qualified by `lib::x` resolves only to the latter.
        let lib_x = path(&["lib", "x"]);
        let nodes = vec![
            test_node("helper", &[]),
            node(FnKey::free_in(lib_x.clone(), "helper"), vec![]),
            node(
                FnKey::free_in(Vec::new(), "caller"),
                vec![free_edge("helper", lib_x)],
            ),
        ];
        let adj = resolve_adjacency(&nodes);
        assert_eq!(adj[2].len(), 1);
        assert_eq!(
            adj[2][0].0, 1,
            "edge into `lib::x::helper` must target the qualified node, not the entry `helper`"
        );
    }

    /// The FAMILY 2 regression witness at the graph level: a struct associated
    /// function (`mid::make` in file `a`, a `Method` key) and a same-named free
    /// function in the sibling file (`make` in `a/mid`, a `Free` key) are
    /// distinct nodes, so a method that calls itself resolves its self-edge to
    /// its own node — it cannot be hijacked by the free function, which under the
    /// old flat-string scheme shared its `a.mid.make` key.
    #[test]
    fn method_self_edge_not_hijacked_by_sibling_file_free_fn() {
        // Node 0: free `make` in `a/mid` (innocent).
        // Node 1: struct method `mid.make` in `a`, calling `mid::make` (itself).
        let nodes = vec![
            node(FnKey::free_in(path(&["a", "mid"]), "make"), vec![]),
            node(
                FnKey::method_in(path(&["a"]), "mid", "make"),
                vec![CallEdge {
                    name: "make".to_string(),
                    receiver_struct: Some("mid".to_string()),
                    module_path: path(&["a"]),
                    spec: None,
                    location: loc(),
                }],
            ),
        ];
        let adj = resolve_adjacency(&nodes);
        assert_eq!(
            adj[1],
            vec![(1, loc())],
            "the method's self-call must resolve to its own node (1), not the free fn (0)"
        );
        assert!(
            adj[0].is_empty(),
            "the innocent sibling-file free fn has no edges"
        );
    }
}
