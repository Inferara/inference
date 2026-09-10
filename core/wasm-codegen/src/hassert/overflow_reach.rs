//! Which functions reach arithmetic that traps on overflow, through calls.
//!
//! An `exists`/`unique`-quantified specification body is the one specification
//! body the downstream judgment reduces, and it reduces the whole activation:
//! the frames of the functions the body calls are stepped just as the body's own
//! is. A trap anywhere in that activation empties the observation set at the
//! entry that reaches it, which makes the claim false rather than narrower. The
//! lexical rule that rejects such a trap written *in* the body therefore has to
//! be paired with this one, which asks what the body reaches.
//!
//! The question is answered over code generation's own call resolution
//! ([`Compiler::resolve_function_callee_in`]), rescoped at every hop to the
//! callee's own file and enclosing specification. Resolving later hops in the
//! first hop's scope would pick the wrong function whenever two files define the
//! same name, which is exactly the case a specification about one of them is
//! written for.
//!
//! Three deliberate answers:
//!
//! - A callee that cannot be resolved counts as trapping. Failing open there
//!   would let a false obligation ship through a gate that admits open proofs,
//!   which is the outcome the rule exists to prevent.
//! - An `external fn` callee is skipped entirely. Code generation never receives
//!   a dependency's bytes — they arrive at link time, after this pass has run —
//!   so nothing here can say whether a foreign body traps. The linker is the
//!   sole judge of a merged body.
//! - Reachability, not dataflow. Only an entry-derived operand can really trip a
//!   guard, but whether a value is entry-derived is not visible at the call, and
//!   a rule whose reach a reader cannot determine from the site it fires on is
//!   worse than a stricter one they can.

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, ExprId};
use inference_ast::nodes::{ArithMode, Def, Expr, Location};
use inference_fn_key::FnKey;
use inference_type_checker::ExternIndex;
use inference_type_checker::typed_context::TypedContext;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::compiler::{CalleeScope, Compiler, ResolvedCallee};

use super::{CalleeEntry, CalleeIndex};

/// Why a call reaches arithmetic that traps on overflow.
///
/// Both variants name something the author can act on: the function whose
/// arithmetic traps, or the call this pass could not follow.
#[derive(Clone)]
pub(super) enum GuardReason {
    /// A reachable function carries an overflow guard, at `location` in the file
    /// `module_path` names.
    Guard {
        name: String,
        module_path: Vec<String>,
        location: Location,
    },
    /// A reachable call names a callee code generation cannot resolve.
    Unresolvable { spelling: String },
}

/// One outgoing call of a function body, resolved in that body's own scope.
enum Edge {
    /// A call this pass followed to a compiled function.
    To(FnKey),
    /// A call this pass could not follow. `spelling` is the callee as the source
    /// writes it, so the diagnostic can name it.
    Unresolvable { spelling: String },
}

/// What one function contributes on its own: the overflow guard its body
/// carries, if any, and the calls it makes.
struct DirectFacts {
    guard: Option<GuardReason>,
    edges: Vec<Edge>,
}

/// Which compiled functions reach an overflow guard, computed once per module.
///
/// Built over every function [`CalleeIndex`] holds rather than lazily from each
/// specification, so a module whose specifications share a call graph pays for
/// it once and every query answers from the same fixpoint.
#[derive(Default)]
pub(super) struct GuardReach {
    direct: FxHashMap<FnKey, DirectFacts>,
    reaches: FxHashSet<FnKey>,
}

impl GuardReach {
    /// Computes the reachability closure for `callee`'s whole function table.
    ///
    /// `default_mode` is the arithmetic mode an unannotated operator is compiled
    /// at, so the answer describes the module that will actually be emitted
    /// rather than the one the language ships by default.
    ///
    /// The closure is asked for only from inside a retained `exists`/`unique`
    /// body, so a module with no reachability plan gets the empty one: the walk
    /// below is a pass over every compiled body plus a fixpoint over the whole
    /// call graph, and `reach_plans` being empty is the same fact as there being
    /// no body that could ask. The gate and the bodies that consult the answer
    /// read one predicate, so the empty closure is never the answer to a
    /// question anyone asked.
    pub(super) fn build(
        ctx: &TypedContext,
        callee: &CalleeIndex,
        externs: &ExternIndex,
        default_mode: ArithMode,
        reach_plans: &super::reach::ReachPlans<'_>,
    ) -> Self {
        if reach_plans.is_empty() {
            return Self::default();
        }
        let mut direct = FxHashMap::default();
        for (key, entry) in callee.iter() {
            direct.insert(
                key.clone(),
                direct_facts(ctx, callee, externs, entry, default_mode),
            );
        }
        let reaches = close_over_calls(&direct);
        Self { direct, reaches }
    }

    /// Why the call `function_expr_id` names reaches trapping arithmetic, or
    /// `None` when nothing it reaches does.
    ///
    /// `scope` is the scope of the body the call is written in — for a
    /// specification function, its own file and its own `spec` block.
    pub(super) fn reason_for_call(
        &self,
        ctx: &TypedContext,
        callee: &CalleeIndex,
        externs: &ExternIndex,
        function_expr_id: ExprId,
        scope: CalleeScope<'_>,
    ) -> Option<GuardReason> {
        match resolve_edge(ctx, callee, externs, function_expr_id, scope)? {
            Edge::Unresolvable { spelling } => Some(GuardReason::Unresolvable { spelling }),
            Edge::To(key) => self.reason_for_key(&key),
        }
    }

    /// The reason `key` reaches trapping arithmetic, found by following the
    /// first edge that leads to one.
    ///
    /// Deterministic: the closure decides *whether* a function reaches a guard
    /// without regard to order, and this walk then picks the path a reader would
    /// trace by hand — the function's own arithmetic first, then its calls in
    /// source order.
    fn reason_for_key(&self, key: &FnKey) -> Option<GuardReason> {
        if !self.reaches.contains(key) {
            return None;
        }
        let mut visited = FxHashSet::default();
        self.first_reason(key, &mut visited)
    }

    fn first_reason(&self, key: &FnKey, visited: &mut FxHashSet<FnKey>) -> Option<GuardReason> {
        if !visited.insert(key.clone()) {
            return None;
        }
        let facts = self.direct.get(key)?;
        if let Some(reason) = &facts.guard {
            return Some(reason.clone());
        }
        for edge in &facts.edges {
            match edge {
                Edge::Unresolvable { spelling } => {
                    return Some(GuardReason::Unresolvable {
                        spelling: spelling.clone(),
                    });
                }
                Edge::To(next) => {
                    if self.reaches.contains(next)
                        && let Some(reason) = self.first_reason(next, visited)
                    {
                        return Some(reason);
                    }
                }
            }
        }
        None
    }
}

/// The guard one function's own body carries and the calls it makes, resolved
/// in that function's own scope.
fn direct_facts(
    ctx: &TypedContext,
    callee: &CalleeIndex,
    externs: &ExternIndex,
    entry: &CalleeEntry,
    default_mode: ArithMode,
) -> DirectFacts {
    let arena = ctx.arena();
    let Some(body) = function_body(arena, entry.def_id) else {
        return DirectFacts {
            guard: None,
            edges: Vec::new(),
        };
    };
    let scope = CalleeScope {
        module_path: &entry.module_path,
        spec_name: entry.spec_name.as_deref(),
    };
    let mut guard = None;
    Compiler::visit_body_guarded_operators(arena, ctx, body, default_mode, &mut |expr_id, _, _| {
        guard.get_or_insert_with(|| GuardReason::Guard {
            name: arena.def_name(entry.def_id).to_string(),
            module_path: entry.module_path.clone(),
            location: arena[expr_id].location,
        });
    });
    let mut edges = Vec::new();
    for function_expr_id in call_targets(arena, body) {
        if let Some(edge) = resolve_edge(ctx, callee, externs, function_expr_id, scope) {
            edges.push(edge);
        }
    }
    DirectFacts { guard, edges }
}

/// The `function` expression of every call written in `body`, in source order.
fn call_targets(arena: &AstArena, body: BlockId) -> Vec<ExprId> {
    let mut calls = Vec::new();
    Compiler::visit_body_expressions(arena, body, &mut |arena, expr_id| {
        if let Expr::FunctionCall { function, .. } = &arena[expr_id].kind {
            calls.push(*function);
        }
    });
    calls
}

/// Resolves one call in `scope`, or `None` when it names an `external fn`.
///
/// The resolution is code generation's own, which is the point: the function
/// this pass inspects has to be the function the call lowers to, and the two
/// answers cannot be kept in step by two implementations of the same rule.
fn resolve_edge(
    ctx: &TypedContext,
    callee: &CalleeIndex,
    externs: &ExternIndex,
    function_expr_id: ExprId,
    scope: CalleeScope<'_>,
) -> Option<Edge> {
    let arena = ctx.arena();
    // Ahead of every defined-function arm, exactly as lowering probes imports
    // before free callees: a bare name that names an `external fn` visible here
    // is a call this pass says nothing about.
    if let Expr::Identifier(ident_id) = &arena[function_expr_id].kind
        && externs
            .lookup(scope.module_path, scope.spec_name, &arena[*ident_id].name)
            .is_some()
    {
        return None;
    }
    let registered = |key: &FnKey| callee.registered(key);
    let Some(resolved) =
        Compiler::resolve_function_callee_in(arena, ctx, function_expr_id, scope, &registered)
    else {
        return Some(Edge::Unresolvable {
            spelling: render_callee(arena, function_expr_id),
        });
    };
    let key = match resolved {
        ResolvedCallee::Function(name) => {
            Compiler::free_callee_key_in(scope, &name, &registered)
        }
        ResolvedCallee::QualifiedFunction(key)
        | ResolvedCallee::AssociatedFunction { key, .. }
        | ResolvedCallee::InstanceMethod { key, .. } => key,
    };
    if callee.registered(&key) {
        Some(Edge::To(key))
    } else {
        Some(Edge::Unresolvable {
            spelling: render_callee(arena, function_expr_id),
        })
    }
}

/// The least set of functions that carry a guard or reach one through calls.
///
/// A plain worklist to a fixpoint: the predicate is monotone, so the answer does
/// not depend on the order functions are visited in, and a call cycle simply
/// stops adding anything.
fn close_over_calls(direct: &FxHashMap<FnKey, DirectFacts>) -> FxHashSet<FnKey> {
    let mut reaches: FxHashSet<FnKey> = direct
        .iter()
        .filter(|(_, facts)| {
            facts.guard.is_some()
                || facts
                    .edges
                    .iter()
                    .any(|edge| matches!(edge, Edge::Unresolvable { .. }))
        })
        .map(|(key, _)| key.clone())
        .collect();
    let mut changed = true;
    while changed {
        changed = false;
        for (key, facts) in direct {
            if reaches.contains(key) {
                continue;
            }
            let hits = facts.edges.iter().any(|edge| match edge {
                Edge::To(next) => reaches.contains(next),
                Edge::Unresolvable { .. } => true,
            });
            if hits {
                reaches.insert(key.clone());
                changed = true;
            }
        }
    }
    reaches
}

fn function_body(arena: &AstArena, def_id: DefId) -> Option<BlockId> {
    match &arena[def_id].kind {
        Def::Function { body, .. } => Some(*body),
        _ => None,
    }
}

/// The callee of a call as the source writes it, quoted, for a diagnostic that
/// has to name a function this pass could not resolve.
///
/// A callee position holding something with no name of its own gets a
/// description instead, and it is deliberately *not* quoted: a phrase in
/// backticks reads as the text the author typed, so quoting one here would send
/// a reader looking for a function called "the expression in callee position".
fn render_callee(arena: &AstArena, function_expr_id: ExprId) -> String {
    named_callee(arena, function_expr_id).map_or_else(
        || "the expression in callee position".to_string(),
        |name| format!("`{name}`"),
    )
}

/// The source spelling of a callee that has one.
fn named_callee(arena: &AstArena, function_expr_id: ExprId) -> Option<String> {
    Some(match &arena[function_expr_id].kind {
        Expr::Identifier(ident_id) => arena[*ident_id].name.clone(),
        Expr::MemberAccess { expr, name } => {
            format!("{}.{}", named_callee(arena, *expr)?, arena[*name].name)
        }
        Expr::TypeMemberAccess { expr, name } => {
            format!("{}::{}", named_callee(arena, *expr)?, arena[*name].name)
        }
        _ => return None,
    })
}
