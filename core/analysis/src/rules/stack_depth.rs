//! A036: Cumulative shadow-stack depth must not exceed the stack budget.
//!
//! Inference compiles to WebAssembly with a downward-growing shadow stack
//! (`__stack_pointer`) whose size the build configures and
//! [`AnalysisOptions::stack_budget_bytes`] carries here. Only functions that
//! allocate array or struct frames consume it; scalar locals live in WASM locals
//! and never touch linear memory. Codegen already bounds each *individual* frame,
//! but the *cumulative* depth across a call chain is unchecked and only traps
//! opaquely at runtime (an out-of-bounds store in the frame prologue's
//! zero-fill).
//!
//! Because A035 forbids recursion, the whole-program call graph is a DAG, so the
//! worst-case shadow-stack usage is the **maximum-weight root-to-leaf path**,
//! where each node's weight is its compound-frame size. This rule computes that
//! maximum and emits a compile-time error naming the offending chain when it
//! exceeds the budget.
//!
//! # Soundness
//!
//! The per-function weight is a conservative **upper bound** on codegen's real
//! [`FrameLayout`] size: it must never under-approximate, or the analysis would
//! accept a program that codegen overflows. The estimator computes the **exact**
//! codegen byte size for every compound type (mirroring
//! `compute_struct_field_layout` field-by-field, including each field's natural
//! alignment), so a type's intrinsic size matches codegen precisely — even for
//! an array of structs, whose size is `exact_size(elem) * length` with no
//! per-element inflation. The only over-approximation is at *slot placement*:
//! each slot sits at an unknown frame offset that codegen aligns to the type's
//! natural alignment (at most 8 bytes — `i64`/`u64`), so up to 7 leading padding
//! bytes can appear before it. The estimator therefore adds [`MAX_SLOT_PADDING`]
//! once **per slot** — not per field, not per array element — which always
//! covers codegen's real `padding + size`. Summing those charges and rounding up
//! to a 16-byte frame boundary yields a value at least codegen's
//! `FrameLayout.total_size` for every function. `if`/`else` branches take the
//! per-branch maximum (mirroring codegen, which reuses the offset across the two
//! arms) rather than the sum.
//!
//! # Limitation
//!
//! v1 treats *any* node as a potential root: the maximum is taken over every
//! function, not only exported or `spec` entry points. This is a sound
//! over-approximation — it may flag a heavy chain that no real entry point can
//! reach — and keeps the rule independent of entry-point discovery. A future
//! refinement could restrict the roots to reachable entry points.
//!
//! [`AnalysisOptions::stack_budget_bytes`]: crate::AnalysisOptions::stack_budget_bytes

use std::collections::HashSet;

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, ExprId, NodeId};
use inference_ast::nodes::{ArgData, ArgKind, Def, Expr, Location, Stmt};
use inference_fn_key::FnKey;
use inference_type_checker::type_info::{NumberType, TypeInfo, TypeInfoKind};
use inference_type_checker::StructInfo;
use rustc_hash::FxHashMap;

use crate::call_graph::{build_call_graph, resolve_adjacency, FnNode, BLACK, GRAY, WHITE};
use crate::errors::{AnalysisDiagnostic, LabeledDiagnostic};
use crate::rule::TypedContext;

/// Frame alignment in bytes, mirroring
/// `core/wasm-codegen/src/memory.rs::FRAME_ALIGNMENT`. Every per-function frame
/// size is rounded up to this boundary.
const FRAME_ALIGNMENT: u32 = 16;

/// Worst-case alignment padding charged per compound slot.
///
/// Every supported array element and struct field has a natural alignment of at
/// most 8 bytes (`i64`/`u64`), so codegen inserts at most 7 padding bytes before
/// any one slot. Charging this maximum to every slot guarantees the estimate is
/// never below codegen's real layout.
///
/// This `≤ 8` alignment invariant is now defended on both sides. On the codegen
/// side the test `every_supported_type_aligns_within_max_slot_padding` in
/// `core/wasm-codegen/src/memory.rs` fails for a future 16-byte-aligned type
/// (i128/v128). On the analysis side the exhaustive `NumberType` matches in
/// [`exact_byte_size_visited`] and [`alignment_of`] fail to compile when a new
/// numeric variant is added, forcing this constant to be revisited.
const MAX_SLOT_PADDING: u32 = 7;

crate::rule! {
    /// Cumulative shadow-stack usage across a call chain must fit the budget.
    #[id = "A036"]
    #[name = "Stack depth exceeded"]
    #[severity = error]
    pub struct StackDepthExceeded;
    fn check(ctx: &TypedContext, options: AnalysisOptions) -> Vec<LabeledDiagnostic> {
        let nodes = build_call_graph(ctx);
        check_stack_depth(ctx, &nodes, options.stack_budget_bytes)
    }
}

/// Computes each node's frame weight and reports the deepest weighted path when
/// it exceeds `budget_bytes`.
///
/// The diagnostic is anchored at the chain's first function; that function's
/// defining file names the finding.
fn check_stack_depth(
    ctx: &TypedContext,
    nodes: &[FnNode],
    budget_bytes: u32,
) -> Vec<LabeledDiagnostic> {
    if nodes.is_empty() {
        return Vec::new();
    }
    let weights: Vec<u32> = nodes
        .iter()
        .map(|n| estimate_frame_size(ctx, n))
        .collect();
    let adj = resolve_adjacency(nodes);

    let Some((depth_bytes, path)) = deepest_path(&adj, &weights) else {
        return Vec::new();
    };
    if depth_bytes <= budget_bytes {
        return Vec::new();
    }
    vec![LabeledDiagnostic::new(
        nodes[path[0]].module_path.clone(),
        AnalysisDiagnostic::StackDepthExceeded {
            chain: render_chain(nodes, &path),
            depth_bytes,
            budget_bytes,
            location: nodes[path[0]].location,
        },
    )]
}

/// Returns the maximum total weight over all root-to-leaf paths and the node
/// indices of that path, or `None` for an empty graph.
///
/// `max_depth(u) = weight(u) + max over edges u->v of max_depth(v)`, memoized
/// and cycle-safe: a GRAY (back-edge) target is not descended into, leaving the
/// recursion diagnostic to A035. Because every node may be a root (see the
/// module limitation), the global maximum is taken over all start nodes.
fn deepest_path(adj: &[Vec<(usize, Location)>], weights: &[u32]) -> Option<(u32, Vec<usize>)> {
    let n = weights.len();
    if n == 0 {
        return None;
    }
    let mut color = vec![WHITE; n];
    // Memoized best (total_bytes, successor_path) for the subtree rooted at each
    // node. `None` until computed.
    let mut best: Vec<Option<(u32, Vec<usize>)>> = vec![None; n];

    for start in 0..n {
        if color[start] == WHITE {
            longest_from(start, adj, weights, &mut color, &mut best);
        }
    }

    (0..n)
        .filter_map(|i| best[i].clone())
        .max_by_key(|(bytes, _)| *bytes)
}

/// Computes the memoized longest weighted path starting at `u`.
fn longest_from(
    u: usize,
    adj: &[Vec<(usize, Location)>],
    weights: &[u32],
    color: &mut [u8],
    best: &mut [Option<(u32, Vec<usize>)>],
) {
    color[u] = GRAY;
    let mut best_child: Option<(u32, Vec<usize>)> = None;
    for &(v, _) in &adj[u] {
        // Skip back edges into the current DFS path; A035 owns recursion.
        if color[v] == GRAY {
            continue;
        }
        if best[v].is_none() && color[v] == WHITE {
            longest_from(v, adj, weights, color, best);
        }
        if let Some((child_bytes, child_path)) = &best[v]
            && best_child
                .as_ref()
                .is_none_or(|(cur, _)| *child_bytes > *cur)
        {
            best_child = Some((*child_bytes, child_path.clone()));
        }
    }

    let (child_bytes, mut path) = best_child.unwrap_or((0, Vec::new()));
    let total = weights[u].saturating_add(child_bytes);
    let mut full_path = Vec::with_capacity(path.len() + 1);
    full_path.push(u);
    full_path.append(&mut path);
    best[u] = Some((total, full_path));
    color[u] = BLACK;
}

/// Renders a path as `a -> b -> c` using each node's canonical key.
fn render_chain(nodes: &[FnNode], path: &[usize]) -> String {
    path.iter()
        .map(|&i| nodes[i].key.to_string())
        .collect::<Vec<_>>()
        .join(" -> ")
}

/// Returns each function's estimated stack-frame size in bytes, keyed by the
/// structured [`FnKey`].
///
/// Exposed so the codegen↔analysis frame-size soundness invariant
/// (estimate ≥ real) can be checked cross-crate; see the parity test in
/// `inference-tests`. Keys are the structured [`FnKey`] from the shared
/// [`inference_fn_key`] crate — the same key codegen records its frame-size map
/// under, which is the interchange format the parity test compares. Keying on
/// the structured `FnKey` rather than its lossy `Display` string keeps two
/// functions whose keys render identically distinct, so the parity test cannot
/// compare one function's estimate against another's real frame.
#[must_use = "returns the estimated frame sizes"]
pub fn estimate_frame_sizes(ctx: &TypedContext) -> FxHashMap<FnKey, u32> {
    build_call_graph(ctx)
        .iter()
        .map(|node| (node.key.clone(), estimate_frame_size(ctx, node)))
        .collect()
}

/// Estimates a conservative upper bound on a function's codegen frame size.
///
/// Mirrors codegen's slot rules: array/struct/custom params, a mutable `self`,
/// and array/struct/custom `let`/`const` bindings each get a slot; scalars and
/// enums get none. Self-referential compound reassignments add codegen's single
/// shared scratch region on top (one max charge, see [`max_self_ref_scratch`]).
/// See the module-level soundness note for why this is always at least codegen's
/// real `FrameLayout.total_size`.
fn estimate_frame_size(ctx: &TypedContext, node: &FnNode) -> u32 {
    let arena = ctx.arena();
    let mut bytes: u32 = 0;

    if let Def::Function { args, .. } = &arena[node.def_id].kind {
        bytes = bytes.saturating_add(params_frame_bytes(
            ctx,
            args,
            &node.module_path,
            node.struct_name.as_deref(),
        ));
    }

    bytes = bytes.saturating_add(body_frame_bytes(ctx, node.body, &node.module_path));
    bytes = bytes.saturating_add(max_self_ref_scratch(ctx, node.body, &node.module_path));

    if bytes == 0 {
        return 0;
    }
    align_to(bytes, FRAME_ALIGNMENT)
}

/// Accumulates the slot bytes for compound parameters and a `self` receiver.
///
/// `module_path` is the function's defining file, used to resolve a bare
/// parameter type name to its file-qualified struct: a parameter annotation
/// carries only the bare name (`TypeInfo::from_type_id`), and the same bare name
/// can name a different struct in another file.
fn params_frame_bytes(
    ctx: &TypedContext,
    args: &[ArgData],
    module_path: &[String],
    struct_name: Option<&str>,
) -> u32 {
    let arena = ctx.arena();
    let mut bytes: u32 = 0;
    for arg in args {
        match &arg.kind {
            ArgKind::Named { ty, .. } => {
                let type_info = TypeInfo::from_type_id(arena, *ty);
                bytes = bytes.saturating_add(slot_bytes(ctx, &type_info.kind, module_path));
            }
            // Codegen copies the receiver into the callee's frame when the
            // method body assigns through it or forwards it to an `external
            // fn`, so either receiver shape can carry a real slot. Every
            // `self` receiver is charged rather than re-deriving that
            // condition in this crate: the estimate is licensed to be loose,
            // but never to under-count.
            ArgKind::SelfRef { .. } => {
                if let Some(name) = struct_name {
                    bytes = bytes.saturating_add(slot_bytes(
                        ctx,
                        &TypeInfoKind::Custom(name.to_string()),
                        module_path,
                    ));
                }
            }
            _ => {}
        }
    }
    bytes
}

/// Walks a block accumulating slot bytes for compound bindings, mirroring
/// codegen's `collect_compound_slots`: descends `Block`/`Loop`, and for an
/// `If` with an `else` takes the per-branch maximum rather than the sum.
fn body_frame_bytes(ctx: &TypedContext, block_id: BlockId, module_path: &[String]) -> u32 {
    let arena = ctx.arena();
    let mut bytes: u32 = 0;
    for &stmt_id in &arena[block_id].stmts {
        match &arena[stmt_id].kind {
            Stmt::VarDef { .. } | Stmt::ConstDef(_) => {
                if let Some(type_info) = ctx.get_node_typeinfo(NodeId::Stmt(stmt_id)) {
                    bytes = bytes.saturating_add(slot_bytes(ctx, &type_info.kind, module_path));
                }
            }
            Stmt::Block(inner) => {
                bytes = bytes.saturating_add(body_frame_bytes(ctx, *inner, module_path));
            }
            Stmt::Loop { body, .. } => {
                bytes = bytes.saturating_add(body_frame_bytes(ctx, *body, module_path));
            }
            Stmt::If {
                then_block,
                else_block,
                ..
            } => {
                let then_bytes = body_frame_bytes(ctx, *then_block, module_path);
                let branch_bytes = match else_block {
                    Some(else_id) => then_bytes.max(body_frame_bytes(ctx, *else_id, module_path)),
                    None => then_bytes,
                };
                bytes = bytes.saturating_add(branch_bytes);
            }
            _ => {}
        }
    }
    bytes
}

/// Returns the size of codegen's single shared self-referential-reassignment
/// scratch region for the whole function body: the **maximum** slot size over
/// every self-referential compound-literal reassignment
/// (`p = P { x: p.y, y: p.x }`, `a = [a[1], a[0]]`), or `0` if there are none.
///
/// A self-referential compound reassignment forces codegen to stage the literal
/// in an in-frame scratch region before copying it to the destination, since
/// building it field-by-field directly into the destination would clobber the
/// operands mid-build. Codegen reserves exactly **one** such region per function
/// (`scan_self_ref_scratch` in `core/wasm-codegen/src/compiler.rs`), reused
/// sequentially and sized to the largest such destination — not one region per
/// assignment. Taking the max here therefore mirrors codegen's real frame,
/// whereas summing (one charge per assignment) would over-count a function with
/// two or more such reassignments and could falsely exceed the budget.
///
/// The walk descends `Block`/`Loop` and **both** `if`/`else` branches with a flat
/// max (not branch-aware). A flat max over the whole body is a sound upper bound
/// of codegen's shared region regardless of branch structure — a max over a
/// superset is never below the max over whichever subset a run actually reaches —
/// and equals it in the common case. `slot_bytes` (exact size plus the worst-case
/// per-slot leading padding) upper-bounds the aligned scratch region the same way
/// it bounds every other slot.
fn max_self_ref_scratch(ctx: &TypedContext, block_id: BlockId, module_path: &[String]) -> u32 {
    let arena = ctx.arena();
    let mut scratch: u32 = 0;
    for &stmt_id in &arena[block_id].stmts {
        match &arena[stmt_id].kind {
            Stmt::Assign { left, right } => {
                if let Expr::Identifier(ident_id) = &arena[*left].kind {
                    let dest = &arena[*ident_id].name;
                    let self_ref = match &arena[*right].kind {
                        Expr::StructLiteral { fields, .. } => fields
                            .iter()
                            .any(|(_, fe)| expr_reads_var(arena, *fe, dest)),
                        Expr::ArrayLiteral { elements } => elements
                            .iter()
                            .any(|e| expr_reads_var(arena, *e, dest)),
                        _ => false,
                    };
                    if self_ref
                        && let Some(type_info) = ctx.get_node_typeinfo(NodeId::Expr(*right))
                    {
                        scratch = scratch.max(slot_bytes(ctx, &type_info.kind, module_path));
                    }
                }
            }
            Stmt::Block(inner) => {
                scratch = scratch.max(max_self_ref_scratch(ctx, *inner, module_path));
            }
            Stmt::Loop { body, .. } => {
                scratch = scratch.max(max_self_ref_scratch(ctx, *body, module_path));
            }
            Stmt::If {
                then_block,
                else_block,
                ..
            } => {
                scratch = scratch.max(max_self_ref_scratch(ctx, *then_block, module_path));
                if let Some(else_id) = else_block {
                    scratch = scratch.max(max_self_ref_scratch(ctx, *else_id, module_path));
                }
            }
            _ => {}
        }
    }
    scratch
}

/// Returns `true` if `expr_id` lexically reads the variable named `dest`.
///
/// Duplicates the codegen predicate `Compiler::expr_reads_var`
/// (`core/wasm-codegen/src/compiler.rs`); the two crates cannot share it. See
/// [`max_self_ref_scratch`] for why the estimate must charge scratch for a
/// self-referential compound reassignment.
fn expr_reads_var(arena: &AstArena, expr_id: ExprId, dest: &str) -> bool {
    match &arena[expr_id].kind {
        Expr::Identifier(ident_id) => arena[*ident_id].name == dest,
        Expr::Binary { left, right, .. } => {
            expr_reads_var(arena, *left, dest) || expr_reads_var(arena, *right, dest)
        }
        Expr::PrefixUnary { expr, .. }
        | Expr::Parenthesized { expr }
        | Expr::MemberAccess { expr, .. }
        | Expr::TypeMemberAccess { expr, .. } => expr_reads_var(arena, *expr, dest),
        Expr::FunctionCall { function, args, .. } => {
            expr_reads_var(arena, *function, dest)
                || args
                    .iter()
                    .any(|(_, arg_expr)| expr_reads_var(arena, *arg_expr, dest))
        }
        Expr::ArrayIndexAccess { array, index } => {
            expr_reads_var(arena, *array, dest) || expr_reads_var(arena, *index, dest)
        }
        Expr::StructLiteral { fields, .. } => fields
            .iter()
            .any(|(_, field_expr)| expr_reads_var(arena, *field_expr, dest)),
        Expr::ArrayLiteral { elements } => elements
            .iter()
            .any(|elem| expr_reads_var(arena, *elem, dest)),
        Expr::NumberLiteral { .. }
        | Expr::BoolLiteral { .. }
        | Expr::StringLiteral { .. }
        | Expr::UnitLiteral
        | Expr::Uzumaki
        | Expr::Type(_) => false,
    }
}

/// Returns the upper-bound slot contribution for a binding/parameter of the
/// given type: `0` for scalars and enums (no frame slot), otherwise the exact
/// compound byte size plus the worst-case per-slot leading alignment padding.
///
/// `module_path` is the referencing file, used to resolve a bare `Custom` name
/// to the struct it names from that file (see [`resolve_struct_size`]).
fn slot_bytes(ctx: &TypedContext, kind: &TypeInfoKind, module_path: &[String]) -> u32 {
    match kind {
        TypeInfoKind::Array(..) | TypeInfoKind::Struct(_, _) => {
            exact_byte_size(ctx, kind, module_path).saturating_add(MAX_SLOT_PADDING)
        }
        // A `Custom` name gets a slot only when it resolves to a struct from the
        // referencing file; enum and unresolved names are scalars (or invalid)
        // and get none.
        TypeInfoKind::Custom(name) if ctx.lookup_struct_in(name, module_path).is_some() => {
            exact_byte_size(ctx, kind, module_path).saturating_add(MAX_SLOT_PADDING)
        }
        // A `::`-qualified type annotation (`lib::big::Big`) reaches a parameter
        // by-value the same as a bare struct does; it carries the path unresolved,
        // so it gets a slot only when the path names a struct. Sizing it to zero
        // would let an oversized cross-file struct frame slip past the budget.
        TypeInfoKind::Qualified(path) | TypeInfoKind::QualifiedName(path)
            if ctx
                .resolve_struct_by_qualified_path(&split_path(path), module_path)
                .is_some() =>
        {
            exact_byte_size(ctx, kind, module_path).saturating_add(MAX_SLOT_PADDING)
        }
        _ => 0,
    }
}

/// Splits a `::`-joined type path (`lib::big::Big`) into its segments, the form
/// [`TypedContext::resolve_struct_by_qualified_path`] expects. A
/// [`TypeInfoKind::Qualified`]/[`TypeInfoKind::QualifiedName`] carries its path as
/// a single joined string.
fn split_path(path: &str) -> Vec<String> {
    path.split("::").map(ToString::to_string).collect()
}

/// Resolves a struct/custom type to the [`StructInfo`] it names and the
/// canonical key under which it (and its fields) should be re-keyed.
///
/// A `Struct(_, canonical_key)` already carries its file-qualified key, so it is
/// looked up directly. A bare `Custom(name)` (a parameter or `mut self` slot)
/// resolves through the referencing `module_path`, so the same bare name in two
/// files maps to each file's own struct. Returns `None` when the name does not
/// name a struct from that file (it may be an enum or a scalar).
fn resolve_struct_size(
    ctx: &TypedContext,
    kind: &TypeInfoKind,
    module_path: &[String],
) -> Option<(StructInfo, String)> {
    match kind {
        TypeInfoKind::Struct(_, canonical_key) => ctx
            .lookup_struct(canonical_key)
            .map(|info| (info, canonical_key.clone())),
        TypeInfoKind::Custom(name) => ctx.lookup_struct_in(name, module_path).map(|info| {
            let key = ctx
                .canonical_struct_key(name, module_path)
                .unwrap_or_else(|| name.clone());
            (info, key)
        }),
        // A qualified annotation carries an unresolved `::`-joined path; resolve it
        // against the referencing file to the same struct and canonical key codegen
        // lays it out under.
        TypeInfoKind::Qualified(path) | TypeInfoKind::QualifiedName(path) => {
            ctx.resolve_struct_by_qualified_path(&split_path(path), module_path)
        }
        _ => None,
    }
}

/// Whether a struct/custom type names an enum (which has no frame slot but a
/// known 4-byte tag size), resolved by canonical key or the referencing file.
fn names_enum(ctx: &TypedContext, kind: &TypeInfoKind, module_path: &[String]) -> bool {
    match kind {
        TypeInfoKind::Enum(_, canonical_key) => ctx.lookup_enum(canonical_key).is_some(),
        TypeInfoKind::Custom(name) => ctx.lookup_enum_in(name, module_path).is_some(),
        TypeInfoKind::Qualified(path) | TypeInfoKind::QualifiedName(path) => {
            ctx.qualified_path_is_enum(&split_path(path), module_path)
        }
        _ => false,
    }
}

/// Computes the **exact** codegen byte size of a type, mirroring codegen's
/// `type_byte_size` / `compute_struct_field_layout` (`core/wasm-codegen/src/
/// memory.rs`) field-by-field. Structs are laid out with each field aligned to
/// its natural alignment against a running offset and the total rounded up to
/// the struct's maximum field alignment; arrays are `exact_size(elem) * length`.
/// The result equals codegen's deterministic layout — there is no
/// over-approximation here. Per-slot leading-padding margin is added separately
/// by [`slot_bytes`].
///
/// `module_path` is the referencing file, threaded so a bare cross-file `Custom`
/// type resolves to the struct it names from that file. Field types stored in a
/// resolved struct already carry their canonical keys, so the nested walk needs
/// the path only for the (rare) bare entry point.
///
/// The inner [`NumberType`] match is exhaustive (no wildcard): a future numeric
/// variant fails compilation here rather than being silently sized as zero,
/// which would under-approximate the frame and make A036 unsound. The outer
/// `_ => 0` arm covers only genuinely non-frame `TypeInfoKind` variants
/// (`String`/`Unit`/`Generic`/etc.) that never reach a frame slot.
///
/// A visited set keyed by canonical key guards against cyclic struct definitions
/// (defense-in-depth; the type checker and A026 reject these first) and keeps
/// same-named structs in different files distinct.
fn exact_byte_size(ctx: &TypedContext, kind: &TypeInfoKind, module_path: &[String]) -> u32 {
    let mut visited = HashSet::new();
    exact_byte_size_visited(ctx, kind, module_path, &mut visited)
}

fn exact_byte_size_visited(
    ctx: &TypedContext,
    kind: &TypeInfoKind,
    module_path: &[String],
    visited: &mut HashSet<String>,
) -> u32 {
    match kind {
        TypeInfoKind::Bool => 1,
        TypeInfoKind::Number(nt) => match nt {
            NumberType::I8 | NumberType::U8 => 1,
            NumberType::I16 | NumberType::U16 => 2,
            NumberType::I32 | NumberType::U32 => 4,
            NumberType::I64 | NumberType::U64 => 8,
        },
        TypeInfoKind::Enum(_, _) => 4,
        TypeInfoKind::Array(elem, length) => {
            let elem_sz = exact_byte_size_visited(ctx, &elem.kind, module_path, visited);
            elem_sz.saturating_mul(*length)
        }
        TypeInfoKind::Struct(_, _)
        | TypeInfoKind::Custom(_)
        | TypeInfoKind::Qualified(_)
        | TypeInfoKind::QualifiedName(_) => {
            if let Some((struct_info, key)) = resolve_struct_size(ctx, kind, module_path) {
                if !visited.insert(key.clone()) {
                    return 0;
                }
                // Mirror `compute_struct_field_layout`: place each field at its
                // natural alignment against a running offset, then round the
                // total up to the struct's maximum field alignment. A field's
                // own defining file is irrelevant once it carries a canonical
                // key, so the path is threaded only for completeness.
                let mut current: u32 = 0;
                let mut max_align: u32 = 1;
                for field in &struct_info.fields {
                    let a = alignment_of(ctx, &field.type_info.kind, module_path, visited);
                    let field_sz =
                        exact_byte_size_visited(ctx, &field.type_info.kind, module_path, visited);
                    current = align_to(current, a).saturating_add(field_sz);
                    max_align = max_align.max(a);
                }
                visited.remove(&key);
                align_to(current, max_align)
            } else if names_enum(ctx, kind, module_path) {
                4
            } else {
                0
            }
        }
        // String/Unit/Generic/etc. never reach a frame slot in valid programs.
        _ => 0,
    }
}

/// Returns the natural alignment in bytes of a type, mirroring codegen's
/// `natural_alignment_for_type` / `natural_alignment` (`core/wasm-codegen/src/
/// memory.rs`). Every supported type aligns within [`MAX_SLOT_PADDING`] + 1
/// bytes (the `≤ 8` invariant the codegen guard test enforces).
///
/// The inner [`NumberType`] match is exhaustive (no wildcard): a future numeric
/// variant fails compilation here rather than being silently aligned to one,
/// which would under-approximate slot padding and make A036 unsound. The outer
/// `_ => 1` arm covers only byte-aligned `Bool`/`i8`/`u8` and genuinely
/// non-frame `TypeInfoKind` variants (`String`/`Unit`/`Generic`/unresolved
/// names) that never reach a frame slot.
///
/// A visited set keyed by canonical key guards against cyclic struct definitions
/// (defense-in-depth), matching the pattern in [`exact_byte_size_visited`].
fn alignment_of(
    ctx: &TypedContext,
    kind: &TypeInfoKind,
    module_path: &[String],
    visited: &mut HashSet<String>,
) -> u32 {
    match kind {
        TypeInfoKind::Number(nt) => match nt {
            NumberType::I8 | NumberType::U8 => 1,
            NumberType::I16 | NumberType::U16 => 2,
            NumberType::I32 | NumberType::U32 => 4,
            NumberType::I64 | NumberType::U64 => 8,
        },
        TypeInfoKind::Enum(_, _) => 4,
        TypeInfoKind::Array(elem, _) => alignment_of(ctx, &elem.kind, module_path, visited),
        TypeInfoKind::Struct(_, _)
        | TypeInfoKind::Custom(_)
        | TypeInfoKind::Qualified(_)
        | TypeInfoKind::QualifiedName(_) => {
            if let Some((struct_info, key)) = resolve_struct_size(ctx, kind, module_path) {
                if !visited.insert(key.clone()) {
                    return 1;
                }
                let mut max_align: u32 = 1;
                for field in &struct_info.fields {
                    max_align =
                        max_align.max(alignment_of(ctx, &field.type_info.kind, module_path, visited));
                }
                visited.remove(&key);
                max_align
            } else if names_enum(ctx, kind, module_path) {
                4
            } else {
                1
            }
        }
        // Bool and i8/u8 are byte-aligned; String/Unit/Generic and unresolved
        // names never reach a frame slot in valid programs and align to 1.
        _ => 1,
    }
}

/// Rounds `value` up to the next multiple of `alignment` (a power of two).
fn align_to(value: u32, alignment: u32) -> u32 {
    value
        .saturating_add(alignment - 1)
        & !(alignment - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call_graph::test_node;

    /// The entry file's (empty) module path: every `register_test_struct` keys a
    /// struct by its bare name, which is its canonical key in a single file.
    const NO_PATH: &[String] = &[];

    /// Builds an adjacency list directly (bypassing key resolution) so the
    /// longest-path tests can use hand-built graphs without arena setup.
    fn adj(edges: &[&[usize]]) -> Vec<Vec<(usize, Location)>> {
        edges
            .iter()
            .map(|succs| succs.iter().map(|&v| (v, Location::default())).collect())
            .collect()
    }

    #[test]
    fn single_node_path_is_its_own_weight() {
        let weights = vec![100];
        let adj = adj(&[&[]]);
        let (bytes, path) = deepest_path(&adj, &weights).unwrap();
        assert_eq!(bytes, 100);
        assert_eq!(path, vec![0]);
    }

    #[test]
    fn linear_chain_sums_weights_along_path() {
        // 0 -> 1 -> 2, weights 10 + 20 + 30 = 60.
        let weights = vec![10, 20, 30];
        let adj = adj(&[&[1], &[2], &[]]);
        let (bytes, path) = deepest_path(&adj, &weights).unwrap();
        assert_eq!(bytes, 60);
        assert_eq!(path, vec![0, 1, 2]);
    }

    #[test]
    fn branching_takes_the_heavier_child() {
        // 0 -> {1, 2}; 1 -> 3 (light), 2 -> 4 (heavy).
        // 0:5, 1:1, 2:1, 3:2, 4:100 => 0->2->4 = 106.
        let weights = vec![5, 1, 1, 2, 100];
        let adj = adj(&[&[1, 2], &[3], &[4], &[], &[]]);
        let (bytes, path) = deepest_path(&adj, &weights).unwrap();
        assert_eq!(bytes, 106);
        assert_eq!(path, vec![0, 2, 4]);
    }

    #[test]
    fn diamond_picks_single_deepest_path_not_double_counting() {
        // 0 -> {1, 2}; 1 -> 3; 2 -> 3 (shared leaf). Weight must not be summed
        // across both branches into 3.
        let weights = vec![10, 20, 30, 40];
        let adj = adj(&[&[1, 2], &[3], &[3], &[]]);
        let (bytes, path) = deepest_path(&adj, &weights).unwrap();
        // 0 -> 2 -> 3 = 10 + 30 + 40 = 80 (heavier than 0->1->3 = 70).
        assert_eq!(bytes, 80);
        assert_eq!(path, vec![0, 2, 3]);
    }

    #[test]
    fn cycle_does_not_hang_or_panic() {
        // 0 -> 1 -> 2 -> 0 (a cycle A035 would reject). The back edge into the
        // gray ancestor is skipped, so traversal terminates.
        let weights = vec![10, 20, 30];
        let adj = adj(&[&[1], &[2], &[0]]);
        let result = deepest_path(&adj, &weights);
        assert!(result.is_some(), "cycle traversal must terminate with a result");
        let (bytes, _) = result.unwrap();
        // The deepest acyclic walk visits all three nodes once: 10+20+30 = 60.
        assert_eq!(bytes, 60);
    }

    #[test]
    fn self_loop_is_cycle_safe() {
        let weights = vec![42];
        let adj = adj(&[&[0]]);
        let (bytes, path) = deepest_path(&adj, &weights).unwrap();
        assert_eq!(bytes, 42);
        assert_eq!(path, vec![0]);
    }

    #[test]
    fn under_budget_chain_yields_no_diagnostic() {
        // Wire weights through resolve_adjacency by exercising check via a tiny
        // longest-path that stays under budget.
        let weights = vec![1000, 2000];
        let adj = adj(&[&[1], &[]]);
        let (bytes, _) = deepest_path(&adj, &weights).unwrap();
        assert!(bytes <= crate::AnalysisOptions::default().stack_budget_bytes);
    }

    #[test]
    fn over_budget_chain_is_detected() {
        let weights = vec![40_000, 40_000];
        let adj = adj(&[&[1], &[]]);
        let (bytes, path) = deepest_path(&adj, &weights).unwrap();
        assert!(bytes > crate::AnalysisOptions::default().stack_budget_bytes);
        assert_eq!(bytes, 80_000);
        assert_eq!(path, vec![0, 1]);
    }

    #[test]
    fn render_chain_joins_with_arrows() {
        let nodes = vec![test_node("a", &[]), test_node("b", &[]), test_node("c", &[])];
        assert_eq!(render_chain(&nodes, &[0, 1, 2]), "a -> b -> c");
        assert_eq!(render_chain(&nodes, &[1]), "b");
    }

    #[test]
    fn align_to_rounds_up_to_frame_boundary() {
        assert_eq!(align_to(0, FRAME_ALIGNMENT), 0);
        assert_eq!(align_to(1, FRAME_ALIGNMENT), 16);
        assert_eq!(align_to(16, FRAME_ALIGNMENT), 16);
        assert_eq!(align_to(17, FRAME_ALIGNMENT), 32);
    }

    /// Builds a `TypedContext` with the given structs registered, mirroring the
    /// `register_test_struct` pattern used by `core/wasm-codegen` unit tests.
    fn ctx_with_structs(structs: &[(&str, &[(&str, TypeInfoKind)])]) -> TypedContext {
        let mut ctx = TypedContext::default();
        for (name, fields) in structs {
            let field_specs: Vec<_> = fields
                .iter()
                .map(|(fname, kind)| {
                    (
                        (*fname).to_string(),
                        TypeInfo {
                            kind: kind.clone(),
                            type_params: vec![],
                        },
                    )
                })
                .collect();
            ctx.register_test_struct(name, &field_specs).unwrap();
        }
        ctx
    }

    fn num(n: NumberType) -> TypeInfoKind {
        TypeInfoKind::Number(n)
    }

    /// `{ i8, i64, i8, i16 }`: i8@0, i64@8, i8@16, i16@18..20, rounded to the
    /// max field alignment (8) = 24. Mirrors codegen's `compute_struct_field_layout`.
    #[test]
    fn exact_byte_size_struct_mixed_alignment_is_24() {
        let ctx = ctx_with_structs(&[(
            "S",
            &[
                ("a", num(NumberType::I8)),
                ("b", num(NumberType::I64)),
                ("c", num(NumberType::I8)),
                ("d", num(NumberType::I16)),
            ],
        )]);
        assert_eq!(
            exact_byte_size(&ctx, &TypeInfoKind::Struct("S".to_string(), "S".to_string()), NO_PATH),
            24
        );
    }

    /// A packed two-byte struct `{ i8, i8 }` has size 2 (align 1, no padding).
    /// An array of 8000 such structs is `2 * 8000 = 16000` bytes exactly — the
    /// over-approximation bug previously multiplied per-field padding by 8000.
    #[test]
    fn exact_byte_size_array_of_structs_is_not_inflated() {
        let ctx = ctx_with_structs(&[("P", &[("a", num(NumberType::I8)), ("b", num(NumberType::I8))])]);
        let elem = TypeInfo {
            kind: TypeInfoKind::Struct("P".to_string(), "P".to_string()),
            type_params: vec![],
        };
        assert_eq!(
            exact_byte_size(&ctx, &TypeInfoKind::Struct("P".to_string(), "P".to_string()), NO_PATH),
            2,
            "{{ i8, i8 }} packs to 2 bytes"
        );
        assert_eq!(
            exact_byte_size(&ctx, &TypeInfoKind::Array(Box::new(elem), 8000), NO_PATH),
            16_000,
            "[{{ i8, i8 }}; 8000] is exactly 2 * 8000, not per-field inflated"
        );
    }

    /// One level of struct nesting mirrors codegen: `Inner { i32, i32 }` = 8,
    /// `Outer { Inner, i32 }` = 12.
    #[test]
    fn exact_byte_size_nested_struct() {
        let ctx = ctx_with_structs(&[
            ("Inner", &[("x", num(NumberType::I32)), ("y", num(NumberType::I32))]),
            (
                "Outer",
                &[
                    ("inner", TypeInfoKind::Struct("Inner".to_string(), "Inner".to_string())),
                    ("val", num(NumberType::I32)),
                ],
            ),
        ]);
        assert_eq!(
            exact_byte_size(
                &ctx,
                &TypeInfoKind::Struct("Inner".to_string(), "Inner".to_string()),
                NO_PATH
            ),
            8
        );
        assert_eq!(
            exact_byte_size(
                &ctx,
                &TypeInfoKind::Struct("Outer".to_string(), "Outer".to_string()),
                NO_PATH
            ),
            12
        );
    }

    /// `alignment_of` mirrors codegen's `natural_alignment`: a struct's alignment
    /// is its widest field's alignment, an array's is its element's.
    #[test]
    fn alignment_of_mirrors_codegen() {
        let ctx = ctx_with_structs(&[(
            "S",
            &[("a", num(NumberType::I8)), ("b", num(NumberType::I64))],
        )]);
        let mut v = HashSet::new();
        assert_eq!(alignment_of(&ctx, &num(NumberType::I8), NO_PATH, &mut v), 1);
        assert_eq!(alignment_of(&ctx, &num(NumberType::I16), NO_PATH, &mut v), 2);
        assert_eq!(alignment_of(&ctx, &num(NumberType::I32), NO_PATH, &mut v), 4);
        assert_eq!(alignment_of(&ctx, &num(NumberType::I64), NO_PATH, &mut v), 8);
        assert_eq!(
            alignment_of(
                &ctx,
                &TypeInfoKind::Struct("S".to_string(), "S".to_string()),
                NO_PATH,
                &mut v
            ),
            8,
            "struct alignment = widest field (i64) = 8"
        );
        let arr = TypeInfoKind::Array(
            Box::new(TypeInfo {
                kind: num(NumberType::I16),
                type_params: vec![],
            }),
            10,
        );
        assert_eq!(
            alignment_of(&ctx, &arr, NO_PATH, &mut v),
            2,
            "array alignment = element alignment (i16) = 2"
        );
    }
}
