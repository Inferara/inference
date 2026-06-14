//! Whole-program call graph shared by the recursion (A035) and stack-depth
//! (A036) analyses.
//!
//! Both rules need the same directed graph of function definitions keyed by the
//! canonical name codegen lowers to (the [`FnKey`] Display scheme: free `f`,
//! spec-free `S.f`, method `T.m`, spec-method `S.T.m`, each prefixed by the
//! defining file's `.`-joined module path when it is non-entry). This module
//! builds that graph once and exposes the spec-first edge resolution that turns
//! raw callee keys into node indices.
//!
//! # Module-path identity
//!
//! In a multi-file program two files may each define a free `fn helper`, and a
//! `::`-qualified call (`lib::b::pong()`) targets a function whose defining file
//! differs from the call site's. Both node keys and edge targets are therefore
//! qualified by the **defining file's** module path (empty for the entry file,
//! so a single-file program's keys stay byte-identical to an unqualified
//! program). The qualified key mirrors codegen's `FnKey` Display exactly, which
//! is what makes the A036 frame-size parity check (estimate keyed the same as
//! codegen's emitted frame) hold.
//!
//! # Call resolution
//!
//! Type checking already resolves every non-extern call to its defining file via
//! [`TypedContext::call_target`], which records the callee's `module_path`, bare
//! `name`, and (for a namespaced associated function) its `receiver_struct`.
//! Edge collection consumes that recorded target first, so a qualified or
//! re-exported call resolves to the same file-qualified node codegen emits.
//! When no target was recorded (e.g. a spec-inner call, or a higher-order
//! callee), it falls back to the structural [`resolve_callee_raw`] against the
//! **caller's** file. An `external fn` import has no node, so its edge is always
//! dropped — calls into externs cannot recurse back into this module.
//!
//! Resolution is deliberately conservative: an edge is created only when it
//! resolves to an existing graph node, so callers never produce a false
//! positive.
//!
//! [`FnKey`]: (codegen-internal; mirrored here, not imported)

use std::collections::HashMap;

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, ExprId, NodeId, StmtId};
use inference_ast::nodes::{Def, Expr, Location, Stmt, TypeNode};
#[cfg(test)]
use inference_ast::ids::idx_from_u32;
use inference_type_checker::type_info::TypeInfoKind;

use crate::rule::TypedContext;
use crate::walker::{for_each_stmt_expr, walk_expr};

/// One outgoing call edge.
///
/// `callee_raw` is the resolved *raw* (un-spec-prefixed, un-module-prefixed)
/// callee key (free `f`, method/assoc `T.m`); `module_path` is the callee's
/// *defining* file (empty for an entry-file callee), used to qualify the key
/// when resolving it to a node; `spec` is the enclosing spec of the *caller*,
/// used for spec-first resolution within the same file; `location` is the
/// call-site location used for the diagnostic.
pub(crate) struct CallEdge {
    pub(crate) callee_raw: String,
    pub(crate) module_path: Vec<String>,
    pub(crate) spec: Option<String>,
    pub(crate) location: Location,
}

/// A function node in the call graph.
///
/// `key` is the canonical name matching the codegen `FnKey` Display scheme,
/// including the defining file's `.`-joined module path when non-entry
/// (`lib.b.pong`, `lib.b.T.m`). `display` is the human-facing label used to
/// render a chain; it equals `key`, so a cross-file cycle/chain diagnostic names
/// the offending file.
///
/// The remaining fields carry just enough of the definition for downstream
/// rules that weight or inspect each node (A036's frame-size estimator): the
/// `DefId` of the function, the body block, the defining file's `module_path`
/// (so a cross-file struct field type resolves to its own file's layout), and
/// the enclosing struct name (so a mutable-`self` frame slot can be sized).
/// A035 ignores all but `key`/`display`/`edges`.
pub(crate) struct FnNode {
    pub(crate) key: String,
    pub(crate) display: String,
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
/// match the codegen `FnKey` Display scheme. `ExternFunction` has no body and is
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
                let key = fn_key(module_path, spec, type_name, &fname);
                let mut edges = Vec::new();
                collect_calls_in_block(ctx, arena, *body, module_path, spec, &mut edges);
                nodes.push(FnNode {
                    display: key.clone(),
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
                && let Some((callee_raw, callee_module_path)) =
                    resolve_callee(ctx, *function, module_path)
            {
                edges.push(CallEdge {
                    callee_raw,
                    module_path: callee_module_path,
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

/// Builds a canonical node key matching the codegen `FnKey` Display scheme.
///
/// The `module_path` of the defining file (empty for the entry file) is joined
/// with `.` and prefixed to the spec/type/name part, exactly as codegen's
/// `FnKey::Display` does, so a single-file program keeps unqualified keys and a
/// cross-file key (`lib.b.pong`) matches the name codegen emits for that file.
pub(crate) fn fn_key(
    module_path: &[String],
    spec: Option<&str>,
    type_name: Option<&str>,
    fname: &str,
) -> String {
    let rest = match (spec, type_name) {
        (Some(s), Some(t)) => format!("{s}.{t}.{fname}"),
        (Some(s), None) => format!("{s}.{fname}"),
        (None, Some(t)) => format!("{t}.{fname}"),
        (None, None) => fname.to_string(),
    };
    if module_path.is_empty() {
        rest
    } else {
        format!("{}.{rest}", module_path.join("."))
    }
}

/// Resolves a callee expression to its `(raw_key, defining_module_path)`.
///
/// Type checking's recorded [`TypedContext::call_target`] is consulted first: it
/// resolves every non-extern call — including `::`-qualified module calls,
/// `root::` calls back into the entry file, and re-exported paths — to the
/// callee's actual defining file. A structural walk of the call expression
/// cannot do this (a nested `TypeMemberAccess` for `lib::b::pong` does not name
/// a known type, and `root::ping` has no struct receiver), which is why dropping
/// back to [`resolve_callee_raw`] alone silently lost every cross-file qualified
/// edge.
///
/// When no target was recorded, the structural fallback runs against the
/// caller's `caller_module_path` (covering spec-inner free calls and any
/// untracked form). `None` means the edge cannot be resolved and is dropped.
fn resolve_callee(
    ctx: &TypedContext,
    function: ExprId,
    caller_module_path: &[String],
) -> Option<(String, Vec<String>)> {
    if let Some(target) = ctx.call_target(function) {
        let raw = match &target.receiver_struct {
            Some(struct_name) => format!("{struct_name}.{}", target.name),
            None => target.name.clone(),
        };
        return Some((raw, target.module_path.clone()));
    }
    resolve_callee_raw(ctx, function).map(|raw| (raw, caller_module_path.to_vec()))
}

/// Resolves a callee expression to a *raw* (un-spec-prefixed, un-module-prefixed)
/// key, or `None` when the callee form is unsupported or its receiver type is
/// unknown. Used only as the fallback for calls type checking did not record a
/// [`TypedContext::call_target`] for.
///
/// Known limitation: a `recv.method()` whose receiver type only resolves via
/// [`TypedContext::lookup_struct`] is dropped when that lookup misses, so its
/// edge goes unrecorded.
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
                TypeInfoKind::Struct(t, _) => t.clone(),
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
/// Each raw edge is qualified by its callee's defining `module_path` and
/// resolved spec-first (matching codegen's spec-active call probe): inside spec
/// `S` a bare name is first tried as `S.name`, then as the top-level `name`,
/// both qualified by the callee's module path. If neither names an existing node
/// the edge is dropped. Because edges only ever target existing nodes, callers
/// can never manufacture a false edge.
pub(crate) fn resolve_adjacency(nodes: &[FnNode]) -> Vec<Vec<(usize, Location)>> {
    let index: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.key.as_str(), i))
        .collect();

    nodes
        .iter()
        .map(|n| {
            n.edges
                .iter()
                .filter_map(|e| {
                    let spec_key = e
                        .spec
                        .as_ref()
                        .map(|s| qualify(&e.module_path, &format!("{s}.{}", e.callee_raw)));
                    let bare_key = qualify(&e.module_path, &e.callee_raw);
                    let resolved = spec_key
                        .as_deref()
                        .and_then(|k| index.get(k))
                        .or_else(|| index.get(bare_key.as_str()))
                        .copied();
                    resolved.map(|j| (j, e.location))
                })
                .collect()
        })
        .collect()
}

/// Prefixes a raw key with the `.`-joined module path (empty for the entry
/// file), matching [`fn_key`] and codegen's `FnKey` Display.
fn qualify(module_path: &[String], rest: &str) -> String {
    if module_path.is_empty() {
        rest.to_string()
    } else {
        format!("{}.{rest}", module_path.join("."))
    }
}

/// DFS coloring used by the cycle-aware traversals in both rules.
pub(crate) const WHITE: u8 = 0;
pub(crate) const GRAY: u8 = 1;
pub(crate) const BLACK: u8 = 2;

/// Builds a graph node whose outgoing edges are top-level (spec-free,
/// entry-file) raw keys, for the unit tests of the rules that share this module.
/// The body/def metadata is irrelevant to graph-shape tests, so it is filled
/// with arena-index placeholders.
#[cfg(test)]
pub(crate) fn test_node(key: &str, callees: &[&str]) -> FnNode {
    FnNode {
        key: key.to_string(),
        display: key.to_string(),
        edges: callees
            .iter()
            .map(|c| CallEdge {
                callee_raw: (*c).to_string(),
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
                    module_path: Vec::new(),
                    spec: Some("S".to_string()),
                    location: loc(),
                }],
                def_id: idx_from_u32(0),
                body: idx_from_u32(0),
                location: loc(),
                module_path: Vec::new(),
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
        assert_eq!(fn_key(&[], None, None, "f"), "f");
        assert_eq!(fn_key(&[], Some("S"), None, "f"), "S.f");
        assert_eq!(fn_key(&[], None, Some("T"), "m"), "T.m");
        assert_eq!(fn_key(&[], Some("S"), Some("T"), "m"), "S.T.m");
    }

    #[test]
    fn fn_key_qualifies_non_entry_file_like_codegen() {
        // Mirrors codegen's `FnKey` Display for a non-entry file: the `.`-joined
        // module path is prefixed to every form. A single-file (entry) program
        // passes an empty path and stays unqualified (covered above).
        let path = ["lib".to_string(), "b".to_string()];
        assert_eq!(fn_key(&path, None, None, "pong"), "lib.b.pong");
        assert_eq!(fn_key(&path, None, Some("T"), "m"), "lib.b.T.m");
        assert_eq!(fn_key(&path, Some("S"), None, "f"), "lib.b.S.f");
        assert_eq!(fn_key(&path, Some("S"), Some("T"), "m"), "lib.b.S.T.m");
    }

    #[test]
    fn adjacency_resolves_cross_file_edge_by_callee_module_path() {
        // Entry `ping` calls `lib.b.pong`; the edge carries the callee's defining
        // module path so it resolves to the qualified node, not a dropped one.
        let lib_b = ["lib".to_string(), "b".to_string()];
        let nodes = vec![
            FnNode {
                key: "ping".to_string(),
                display: "ping".to_string(),
                edges: vec![CallEdge {
                    callee_raw: "pong".to_string(),
                    module_path: lib_b.to_vec(),
                    spec: None,
                    location: loc(),
                }],
                def_id: idx_from_u32(0),
                body: idx_from_u32(0),
                location: loc(),
                module_path: Vec::new(),
                struct_name: None,
            },
            FnNode {
                key: "lib.b.pong".to_string(),
                display: "lib.b.pong".to_string(),
                edges: vec![],
                def_id: idx_from_u32(0),
                body: idx_from_u32(0),
                location: loc(),
                module_path: lib_b.to_vec(),
                struct_name: None,
            },
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
        let lib_x = ["lib".to_string(), "x".to_string()];
        let nodes = vec![
            test_node("helper", &[]),
            FnNode {
                key: "lib.x.helper".to_string(),
                display: "lib.x.helper".to_string(),
                edges: vec![],
                def_id: idx_from_u32(0),
                body: idx_from_u32(0),
                location: loc(),
                module_path: lib_x.to_vec(),
                struct_name: None,
            },
            FnNode {
                key: "caller".to_string(),
                display: "caller".to_string(),
                edges: vec![CallEdge {
                    callee_raw: "helper".to_string(),
                    module_path: lib_x.to_vec(),
                    spec: None,
                    location: loc(),
                }],
                def_id: idx_from_u32(0),
                body: idx_from_u32(0),
                location: loc(),
                module_path: Vec::new(),
                struct_name: None,
            },
        ];
        let adj = resolve_adjacency(&nodes);
        assert_eq!(adj[2].len(), 1);
        assert_eq!(
            adj[2][0].0, 1,
            "edge into `lib::x::helper` must target the qualified node, not the entry `helper`"
        );
    }
}
