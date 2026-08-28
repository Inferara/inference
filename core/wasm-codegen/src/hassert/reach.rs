//! The reachability view over the shared choice-lowering plans.
//!
//! Every specification function is choice-lowered — its `@`s arrive as hidden
//! trailing parameters and its body compiles to vanilla WebAssembly (see
//! [`crate::choice`]). For most of them that is purely a code-generation fact:
//! the obligation is built from the typed AST and never mentions a frame index.
//!
//! An `exists`- or `unique`-quantified specification **free function** is the
//! exception. Its obligation is *operational*: the verifier runs the compiled
//! body under the vanilla WebAssembly semantics and asks whether some (or
//! exactly one observation's worth of) choice of the `@` values reaches the end
//! without trapping. Its payload therefore denotes against the *real*
//! activation frame, and the payload's slot indices must equal the compiled
//! frame's. This module is the narrow view that carries that extra promise:
//! [`reachability_plans`] selects exactly those functions, enforces the
//! no-return rule that keeps the arithmetic valid, and hands the obligation
//! pass a [`ReachPlan`] that can answer "which frame slot is this `@`".
//!
//! ## Slot numbering
//!
//! The declared parameters occupy WASM locals `0..entry_arity`; the k-th
//! planned `@` (source order) occupies local `entry_arity + k`. This holds only
//! because a reachability body can never have an sret pointer parameter: an
//! sret would occupy local 0 and shift every index by one. Under the universal
//! (`ValidSpec`) judgment such a skew is inert — the valuation is
//! unconstrained, so a misnumbered slot merely names a different unconstrained
//! value — but a reachability payload denotes against the real frame, where the
//! same skew silently binds the wrong value. The no-return rule below is what
//! keeps sret (and every other body exit) out of the picture, so the arithmetic
//! above is an invariant rather than a convention. Code generation asserts the
//! same equality from its own side, against the local index it observes.
//!
//! The rule is scoped to this judgment and must not follow the choice lowering
//! outward. A specification *method* may legitimately return a compound value,
//! and a universal free function may legitimately declare a return type and
//! `return`; neither obligation denotes a frame index, so neither needs the
//! rule.
//!
//! ## The no-return rule
//!
//! One rule, two clauses, both fatal: a planned function must declare **no
//! return type** and contain **no `return` statement**.
//!
//! - A declared compound result would introduce the sret parameter that breaks
//!   the slot arithmetic above.
//! - Any `return` — scalar included — emits a WASM `return` instruction, and
//!   the verifier reduces the retained body *without* an enclosing activation
//!   frame, so a `return` can never take a reduction step: the obligation
//!   would be unprovable with no signal to the author.
//!
//! Analysis already closes both clauses as a pincer (A005 bans `return` inside
//! any quantified block; A007 rejects a declared return type whose paths do
//! not all return), but the corpus gate and unit-test pipelines run
//! parse → typecheck → codegen *without* analysis, so this pass carries its own
//! hard error rather than trusting a pass that may not have run.
//!
//! ## Purity boundary
//!
//! Everything here is a pure function of [`TypedContext`]/[`AstArena`] and the
//! shared plans — no compiler state is read or written. This preserves the
//! obligation pass's read-only promise (see the module documentation of
//! [`super`]): the plans are facts about the typed program, computed once ahead
//! of code generation, never scraped out of one backend by another.

use inference_ast::ids::{DefId, ExprId};
use inference_ast::nodes::{BlockKind, Def};
use rustc_hash::FxHashMap;

use crate::EmittableFunctions;
use crate::choice::{ChoicePlan, ChoicePlans, ChoiceRun};
use crate::errors::CodegenError;
use inference_type_checker::typed_context::TypedContext;

/// The reachability view of one function's shared choice plan.
///
/// A borrow rather than a copy: both the compiler and this view read the one
/// plan the pre-scan built, so neither can drift from the other on which `@`
/// became which parameter.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ReachPlan<'a> {
    plan: &'a ChoicePlan,
}

impl ReachPlan<'_> {
    /// Number of declared source parameters, ahead of the choice suffix.
    pub(crate) fn entry_arity(self) -> u32 {
        self.plan.entry_arity
    }

    /// The source-visible frame slots the `unique` judgment compares exit
    /// states through: every entry parameter, plus the choice parameter of
    /// every *named* `let x = @` binding, ascending by construction (entry
    /// parameters precede the suffix, and the suffix is recorded in index
    /// order). Anonymous call-argument choices, pure-`let` locals, and
    /// compiler temporaries are excluded — a named choice is the
    /// source-visible face of its binding, while a slot nothing names is not
    /// part of the program's observable exit state. Inert for `exists`
    /// (a locals projection cannot change whether the observation set is
    /// non-empty), so the list is carried identically for both kinds.
    pub(crate) fn visible_locs(self) -> Vec<u32> {
        let entry = 0..self.plan.entry_arity;
        let named = self
            .plan
            .params
            .iter()
            .enumerate()
            .filter(|(_, choice)| choice.named)
            .map(|(k, _)| {
                self.plan.entry_arity + u32::try_from(k).expect("more than u32::MAX choices")
            });
        entry.chain(named).collect()
    }

    /// The activation-frame slot of a `@` the plan covers as a scalar choice
    /// parameter, or `None` for one it does not.
    ///
    /// A `None` is meaningful only where the caller has not already established
    /// scalarity: an anonymous `@` argument performs no pre-check of its own,
    /// so the lookup *is* the check and a miss raises a diagnostic. Where the
    /// caller has established it, use [`Self::choice_slot`].
    pub(crate) fn try_choice_slot(self, expr: ExprId) -> Option<u32> {
        match self.plan.run(expr) {
            Some(ChoiceRun::Scalar(ordinal)) => Some(self.plan.entry_arity + ordinal),
            Some(ChoiceRun::Leaves { .. }) | None => None,
        }
    }

    /// The activation-frame slot of a `@` the caller has already established is
    /// scalar.
    ///
    /// A miss is a compiler bug, never a program error, and there is no honest
    /// slot to fall back on — inventing one would emit a payload slot that is
    /// not the choice parameter, a silently wrong obligation.
    pub(crate) fn choice_slot(self, expr: ExprId) -> u32 {
        self.try_choice_slot(expr).unwrap_or_else(|| {
            panic!(
                "scalar `@` reached reachability translation without a planned scalar choice \
                 parameter — the planner and the translator disagree on scalar classification, \
                 and emitting any other slot would misalign the payload with the compiled frame"
            )
        })
    }
}

/// The per-function [`ReachPlan`]s for a whole program, keyed by the spec
/// function's [`DefId`]. Empty in compile mode and for programs without
/// `exists`/`unique`-bodied specification free functions.
#[derive(Debug, Default)]
pub(crate) struct ReachPlans<'a> {
    by_def: FxHashMap<DefId, ReachPlan<'a>>,
}

impl<'a> ReachPlans<'a> {
    /// The plan for `def_id`, or `None` when the function is not
    /// reachability-quantified.
    pub(crate) fn get(&self, def_id: DefId) -> Option<ReachPlan<'a>> {
        self.by_def.get(&def_id).copied()
    }
}

/// Selects the `exists`/`unique`-bodied specification free functions out of the
/// shared choice plans, enforcing the no-return rule along the way.
///
/// Iterates [`EmittableFunctions::spec_funcs`] — the exact list code generation
/// compiles — so the two walks cannot disagree about which functions exist.
/// Specification *methods* are excluded: a quantified method carries no
/// obligation channel and is rejected as P009 by the obligation pass.
/// `forall`/plain bodies are universal, and `assume` is not a quantifier at all
/// (rejected as P001), so neither denotes a frame slot.
///
/// # Errors
///
/// [`CodegenError::ReachabilitySpecReturns`] when a selected function declares
/// a return type or contains a `return` statement (see the module
/// documentation for why both clauses are fatal).
pub(crate) fn reachability_plans<'a>(
    ctx: &TypedContext,
    buckets: &EmittableFunctions,
    choice_plans: &'a ChoicePlans,
) -> Result<ReachPlans<'a>, CodegenError> {
    let arena = ctx.arena();
    let mut plans = ReachPlans::default();
    for entry in &buckets.spec_funcs {
        let Def::Function {
            name,
            returns,
            body,
            ..
        } = &arena[entry.def_id].kind
        else {
            continue;
        };
        let body_kind = arena[*body].block_kind;
        if !matches!(body_kind, BlockKind::Exists | BlockKind::Unique) {
            continue;
        }
        let Some(plan) = choice_plans.get(entry.def_id) else {
            continue;
        };
        let function = arena[*name].name.clone();
        let kind = super::quantifier_word(body_kind);
        if returns.is_some() {
            cov_mark::hit!(wasm_codegen_reach_declared_return_rejected);
            return Err(CodegenError::ReachabilitySpecReturns {
                spec: entry.spec_name.clone(),
                function,
                kind,
                offense: "declares a return type",
            });
        }
        if plan.has_return {
            cov_mark::hit!(wasm_codegen_reach_return_stmt_rejected);
            return Err(CodegenError::ReachabilitySpecReturns {
                spec: entry.spec_name.clone(),
                function,
                kind,
                offense: "contains a `return` statement",
            });
        }
        plans.by_def.insert(entry.def_id, ReachPlan { plan });
    }
    Ok(plans)
}

#[cfg(test)]
mod tests {
    use inference_type_checker::TypeCheckerBuilder;
    use inference_type_checker::typed_context::TypedContext;

    use super::{ReachPlan, reachability_plans};
    use crate::CompilationMode;
    use crate::choice::{ChoicePlans, plan_choice_lowering};
    use crate::errors::CodegenError;
    use crate::{EmittableFunctions, collect_emittable_functions};

    fn type_check(source: &str) -> TypedContext {
        let parsed = inference_parser::parse(source);
        assert!(
            parsed.errors.is_empty(),
            "parse errors: {:?}",
            parsed.errors
        );
        TypeCheckerBuilder::build_typed_context(parsed.arena)
            .expect("type checking should succeed")
            .typed_context()
    }

    fn buckets_of(ctx: &TypedContext) -> EmittableFunctions {
        let mut buckets = EmittableFunctions::default();
        for source_file in ctx.source_files() {
            collect_emittable_functions(
                ctx.arena(),
                &source_file.defs,
                &source_file.module_path,
                CompilationMode::Proof,
                &mut buckets,
            )
            .expect("collecting emittable functions should succeed");
        }
        buckets
    }

    /// Runs the two passes in production order and hands the result to `check`,
    /// which keeps the borrowed plans alive for exactly as long as they are read.
    fn with_plans<R>(
        source: &str,
        check: impl FnOnce(Result<Vec<ReachPlan<'_>>, CodegenError>) -> R,
    ) -> R {
        let ctx = type_check(source);
        let buckets = buckets_of(&ctx);
        let choice_plans: ChoicePlans = plan_choice_lowering(&ctx, &buckets);
        let result = reachability_plans(&ctx, &buckets, &choice_plans).map(|plans| {
            buckets
                .spec_funcs
                .iter()
                .filter_map(|entry| plans.get(entry.def_id))
                .collect::<Vec<_>>()
        });
        check(result)
    }

    fn planned(source: &str) -> Vec<(u32, Vec<u32>)> {
        with_plans(source, |result| {
            result
                .expect("the reachability view should accept this source")
                .iter()
                .map(|plan| (plan.entry_arity(), plan.visible_locs()))
                .collect()
        })
    }

    fn rejection(source: &str) -> String {
        with_plans(source, |result| {
            result
                .expect_err("the reachability view should reject this source")
                .to_string()
        })
    }

    #[test]
    fn only_exists_and_unique_free_bodies_enter_the_reachability_view() {
        let views = planned(
            "spec S {
              fn a() forall { let n: i32 = @; assert(n >= n); }
              fn b(x: i32) { assert(x >= x); }
              fn c() exists { let n: i32 = @; assert(n == 1); }
              fn d() unique { let n: i32 = @; assert(n == 2); }
            }",
        );
        assert_eq!(
            views.len(),
            2,
            "a universal or plain body is choice-lowered but denotes no frame slot"
        );
    }

    #[test]
    fn a_spec_method_never_enters_the_reachability_view() {
        let views = planned(
            "spec S {
              struct T {
                x: i32;
                fn m(self) exists {
                  let y: i32 = @;
                  assert(y > 0);
                }
              }
            }",
        );
        assert!(
            views.is_empty(),
            "spec methods carry no obligation channel (P009); only free functions are viewed"
        );
    }

    #[test]
    fn visible_locs_are_the_entry_parameters_plus_the_named_choices() {
        let views = planned(
            "fn g(v: i32) -> i32 { return v; }
            spec S {
              fn f(x: i32) exists {
                let a: i64 = @;
                assert(a > 0);
                assert(g(@) == x);
              }
            }",
        );
        assert_eq!(
            views,
            vec![(1, vec![0, 1])],
            "the declared parameter and the named `let` choice; the call-argument choice at \
             slot 2 is anonymous and excluded"
        );
    }

    #[test]
    fn a_declared_return_type_is_rejected() {
        cov_mark::check!(wasm_codegen_reach_declared_return_rejected);
        let msg = rejection(
            "spec S {
              fn f() -> i32 exists {
                let n: i32 = @;
                assert(n > 0);
              }
            }",
        );
        assert!(
            msg.contains("'f'")
                && msg.contains("'S'")
                && msg.contains("'exists'-quantified")
                && msg.contains("declares a return type"),
            "the declared-type clause must name the function, spec, kind, and offense: {msg}"
        );
    }

    #[test]
    fn a_return_statement_is_rejected() {
        cov_mark::check!(wasm_codegen_reach_return_stmt_rejected);
        let msg = rejection(
            "spec S {
              fn f() unique {
                let n: i32 = @;
                assert(n == 1);
                return;
              }
            }",
        );
        assert!(
            msg.contains("'f'")
                && msg.contains("'unique'-quantified")
                && msg.contains("contains a `return` statement"),
            "the return-statement clause must name the function, kind, and offense: {msg}"
        );
    }

    #[test]
    fn the_declared_type_clause_wins_when_both_offend() {
        let msg = rejection(
            "spec S {
              fn f() -> i32 exists {
                return 1;
              }
            }",
        );
        assert!(
            msg.contains("declares a return type"),
            "the declared-type clause is checked first: {msg}"
        );
    }

    /// A universal free function may legally declare a return type and
    /// `return`; the no-return rule is scoped to the reachability judgment and
    /// must not follow the choice lowering outward.
    #[test]
    fn a_universal_body_may_declare_a_return_type() {
        let views = planned(
            "spec S {
              fn f() -> i32 forall {
                let n: i32 = @;
                assert(n >= n);
                return 0;
              }
            }",
        );
        assert!(views.is_empty());
    }
}
