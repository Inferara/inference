//! Reachability pre-scan for `exists`/`unique`-bodied specification free
//! functions (proof mode).
//!
//! An `exists`- or `unique`-quantified specification function is not compiled
//! into a `0xfc`-wrapped non-deterministic body the way a `forall` one is. Its
//! obligation is *operational*: the verifier runs the compiled body under the
//! vanilla WebAssembly semantics and asks whether some (or exactly one
//! observation's worth of) choice of the `@` values reaches the end without
//! trapping. That contract forces a different lowering — the body must be
//! plain WASM, and every scalar `@` must become a value the reduction can
//! quantify over: a hidden trailing *choice parameter* appended after the
//! declared parameters.
//!
//! This module is the single place that decides which `@` becomes which
//! parameter. [`plan_reachability_specs`] walks every `exists`/`unique`-bodied
//! specification free function once, in source order, and records one
//! [`ChoicePlan`] per function. Both consumers — the compiler (signature
//! suffix and body lowering) and the obligation pass (payload slot indices) —
//! read the same `ExprId`-keyed map, so neither can drift from the other on
//! traversal order.
//!
//! ## Slot numbering
//!
//! The declared parameters occupy WASM locals `0..entry_arity`; the k-th
//! planned `@` (source order) occupies local `entry_arity + k`. This holds
//! only because a reachability body can never have an sret pointer parameter:
//! an sret would occupy local 0 and shift every index by one. Under the
//! universal (`ValidSpec`) judgment such a skew is inert — the valuation is
//! unconstrained, so a misnumbered slot merely names a different unconstrained
//! value — but a reachability payload denotes against the *real* activation
//! frame, where the same skew silently binds the wrong value. The no-return
//! rule below is what keeps sret (and every other body exit) out of the
//! picture, so the arithmetic above is an invariant rather than a convention.
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
//! parse → typecheck → codegen *without* analysis, so the pre-scan carries its
//! own hard error rather than trusting a pass that may not have run.
//!
//! ## Purity boundary
//!
//! Everything here is a pure function of [`TypedContext`]/[`AstArena`] — no
//! compiler state is read or written. This preserves the obligation pass's
//! read-only promise (see the module documentation of [`super`]): the plans
//! are facts about the typed program, computed once ahead of code generation,
//! never scraped out of one backend by another.

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, ExprId, NodeId};
use inference_ast::nodes::{ArgKind, BlockKind, Def, Expr, Stmt};
use inference_type_checker::type_info::{NumberType, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;
use rustc_hash::FxHashMap;

use crate::errors::CodegenError;

/// WASM value class of one reachability choice parameter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ChoiceClass {
    /// `bool`, sub-32-bit and 32-bit integers, and enum tags — one `i32`
    /// parameter, domain-normalized at its use (or binding) site.
    I32,
    /// `i64`/`u64` — one `i64` parameter; every bit pattern is in-domain.
    I64,
}

/// One planned scalar `@`: the k-th encountered in source order becomes the
/// function's `entry_arity + k`-th parameter.
#[derive(Clone, Debug)]
pub(crate) struct ChoiceParam {
    /// The `@` expression this choice parameter stands for.
    pub(crate) expr: ExprId,
    /// The WASM value class of the parameter.
    pub(crate) class: ChoiceClass,
    /// Whether the `@` is the whole right-hand side of a `let x: T = @;`.
    /// A named choice is bound *to* its parameter slot (no fresh local), so
    /// the source name, the name-section entry, and the payload slot index
    /// all denote the same frame value.
    pub(crate) named: bool,
}

/// The reachability lowering plan for one `exists`/`unique`-bodied
/// specification free function.
#[derive(Clone, Debug)]
pub(crate) struct ChoicePlan {
    /// Number of declared source parameters, ahead of the choice suffix.
    pub(crate) entry_arity: u32,
    /// Planned choices in source order; index k sits at WASM local
    /// `entry_arity + k`.
    pub(crate) choices: Vec<ChoiceParam>,
    /// Scalar `@` `ExprId` → absolute WASM parameter index
    /// (`entry_arity + k`). Compound `@`s are deliberately absent — they
    /// cannot become a scalar parameter and are rejected by the obligation
    /// pass (P008 family).
    pub(crate) by_expr: FxHashMap<ExprId, u32>,
}

impl ChoicePlan {
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
    pub(crate) fn visible_locs(&self) -> Vec<u32> {
        let entry = 0..self.entry_arity;
        let named = self
            .choices
            .iter()
            .enumerate()
            .filter(|(_, choice)| choice.named)
            .map(|(k, _)| self.entry_arity + u32::try_from(k).expect("more than u32::MAX choices"));
        entry.chain(named).collect()
    }
}

/// The per-function [`ChoicePlan`]s for a whole program, keyed by the spec
/// function's [`DefId`]. Empty in compile mode and for programs without
/// `exists`/`unique`-bodied specification free functions.
#[derive(Debug, Default)]
pub(crate) struct ReachPlans {
    by_def: FxHashMap<DefId, ChoicePlan>,
}

impl ReachPlans {
    /// The plan for `def_id`, or `None` when the function is not
    /// reachability-lowered.
    pub(crate) fn get(&self, def_id: DefId) -> Option<&ChoicePlan> {
        self.by_def.get(&def_id)
    }
}

/// Builds the [`ChoicePlan`] for every `exists`/`unique`-bodied specification
/// free function in the program, enforcing the no-return rule along the way.
///
/// Walks the same one-level spec structure code generation collects: only
/// top-level `spec` blocks, only their free functions. Specification
/// *methods* are skipped — a quantified method carries no obligation channel
/// and is rejected as P009 by the obligation pass. `assume`-bodied functions
/// are skipped too: `assume` is not a quantifier, so there is nothing to plan
/// (the obligation pass rejects the body as P001).
///
/// # Errors
///
/// [`CodegenError::ReachabilitySpecReturns`] when a planned function declares
/// a return type or contains a `return` statement (see the module
/// documentation for why both clauses are fatal).
pub(crate) fn plan_reachability_specs(ctx: &TypedContext) -> Result<ReachPlans, CodegenError> {
    let arena = ctx.arena();
    let mut plans = ReachPlans::default();
    for source_file in ctx.source_files() {
        for &def_id in &source_file.defs {
            let Def::Spec {
                name: spec_name,
                defs: inner,
                ..
            } = &arena[def_id].kind
            else {
                continue;
            };
            let spec_name = arena[*spec_name].name.clone();
            for &inner_id in inner {
                let Def::Function {
                    name,
                    args,
                    returns,
                    body,
                    ..
                } = &arena[inner_id].kind
                else {
                    continue;
                };
                let body_kind = arena[*body].block_kind;
                if !matches!(body_kind, BlockKind::Exists | BlockKind::Unique) {
                    continue;
                }
                let function = arena[*name].name.clone();
                let kind = super::quantifier_word(body_kind);
                if returns.is_some() {
                    cov_mark::hit!(wasm_codegen_reach_declared_return_rejected);
                    return Err(CodegenError::ReachabilitySpecReturns {
                        spec: spec_name,
                        function,
                        kind,
                        offense: "declares a return type",
                    });
                }

                let entry_arity = args
                    .iter()
                    .filter(|arg| {
                        matches!(arg.kind, ArgKind::Named { .. } | ArgKind::SelfRef { .. })
                    })
                    .count();
                let entry_arity =
                    u32::try_from(entry_arity).expect("more than u32::MAX parameters");
                let mut builder = PlanBuilder {
                    ctx,
                    arena,
                    plan: ChoicePlan {
                        entry_arity,
                        choices: Vec::new(),
                        by_expr: FxHashMap::default(),
                    },
                    has_return: false,
                };
                builder.walk_block(*body);
                if builder.has_return {
                    cov_mark::hit!(wasm_codegen_reach_return_stmt_rejected);
                    return Err(CodegenError::ReachabilitySpecReturns {
                        spec: spec_name,
                        function,
                        kind,
                        offense: "contains a `return` statement",
                    });
                }
                plans.by_def.insert(inner_id, builder.plan);
            }
        }
    }
    Ok(plans)
}

/// One walk over one function body, accumulating the plan and the
/// `return`-presence fact together so the body is read exactly once.
struct PlanBuilder<'a> {
    ctx: &'a TypedContext,
    arena: &'a AstArena,
    plan: ChoicePlan,
    has_return: bool,
}

impl PlanBuilder<'_> {
    /// Visits a block's statements in source order. Statement kinds that the
    /// obligation pass later rejects (`loop`, reassignment, nested `forall`/
    /// `unique` blocks) are still walked: the plan must cover every `@` the
    /// compiler will lower, and the rejection is the translator's to raise —
    /// duplicating it here would fork the diagnostic surface.
    fn walk_block(&mut self, block_id: BlockId) {
        let arena = self.arena;
        for &stmt_id in &arena[block_id].stmts {
            match &arena[stmt_id].kind {
                Stmt::VarDef { value, .. } => {
                    if let Some(value) = *value {
                        if matches!(self.arena[value].kind, Expr::Uzumaki) {
                            self.plan_choice(value, true);
                        } else {
                            self.walk_expr(value);
                        }
                    }
                }
                Stmt::Assert { expr } | Stmt::Expr(expr) => self.walk_expr(*expr),
                Stmt::Return { expr } => {
                    self.has_return = true;
                    self.walk_expr(*expr);
                }
                Stmt::Assign { left, right } => {
                    self.walk_expr(*left);
                    self.walk_expr(*right);
                }
                Stmt::If {
                    condition,
                    then_block,
                    else_block,
                } => {
                    self.walk_expr(*condition);
                    self.walk_block(*then_block);
                    if let Some(else_block) = else_block {
                        self.walk_block(*else_block);
                    }
                }
                Stmt::Block(inner) => self.walk_block(*inner),
                Stmt::Loop { condition, body } => {
                    if let Some(condition) = condition {
                        self.walk_expr(*condition);
                    }
                    self.walk_block(*body);
                }
                Stmt::ConstDef(def_id) => {
                    if let Def::Constant { value, .. } = &arena[*def_id].kind {
                        self.walk_expr(*value);
                    }
                }
                Stmt::Break | Stmt::TypeDef { .. } => {}
            }
        }
    }

    /// Visits an expression tree in syntactic order, planning every scalar
    /// `@` leaf as an anonymous choice. The variant list is exhaustive on
    /// purpose: a future expression kind must be classified here rather than
    /// silently becoming a position whose `@`s the plan misses.
    fn walk_expr(&mut self, expr_id: ExprId) {
        let arena = self.arena;
        match &arena[expr_id].kind {
            Expr::Binary { left, right, .. } => {
                self.walk_expr(*left);
                self.walk_expr(*right);
            }
            Expr::ArrayIndexAccess { array, index } => {
                self.walk_expr(*array);
                self.walk_expr(*index);
            }
            Expr::PrefixUnary { expr, .. }
            | Expr::Parenthesized { expr }
            | Expr::MemberAccess { expr, .. }
            | Expr::TypeMemberAccess { expr, .. } => self.walk_expr(*expr),
            Expr::FunctionCall { function, args, .. } => {
                self.walk_expr(*function);
                for &(_, arg) in args {
                    self.walk_expr(arg);
                }
            }
            Expr::StructLiteral { fields, .. } => {
                for &(_, value) in fields {
                    self.walk_expr(value);
                }
            }
            Expr::ArrayLiteral { elements } => {
                for &element in elements {
                    self.walk_expr(element);
                }
            }
            Expr::Uzumaki => self.plan_choice(expr_id, false),
            Expr::Identifier(_)
            | Expr::NumberLiteral { .. }
            | Expr::BoolLiteral { .. }
            | Expr::StringLiteral { .. }
            | Expr::UnitLiteral
            | Expr::Type(_) => {}
        }
    }

    /// Records one scalar `@` as the next choice parameter. A compound
    /// (array/struct) `@` is left out of the plan: it cannot arrive as one
    /// scalar parameter, and the obligation pass rejects it (P008 family)
    /// before any artifact is emitted.
    fn plan_choice(&mut self, expr_id: ExprId, named: bool) {
        let Some(type_info) = self.ctx.get_node_typeinfo(NodeId::Expr(expr_id)) else {
            return;
        };
        let class = match type_info.kind {
            TypeInfoKind::Bool
            | TypeInfoKind::Number(
                NumberType::I8
                | NumberType::U8
                | NumberType::I16
                | NumberType::U16
                | NumberType::I32
                | NumberType::U32,
            )
            | TypeInfoKind::Enum(_, _) => ChoiceClass::I32,
            TypeInfoKind::Number(NumberType::I64 | NumberType::U64) => ChoiceClass::I64,
            _ => return,
        };
        let k = u32::try_from(self.plan.choices.len()).expect("more than u32::MAX choices");
        let index = self.plan.entry_arity + k;
        let previous = self.plan.by_expr.insert(expr_id, index);
        debug_assert!(
            previous.is_none(),
            "a `@` expression was planned twice; the walk must visit every node exactly once"
        );
        self.plan.choices.push(ChoiceParam {
            expr: expr_id,
            class,
            named,
        });
    }
}

#[cfg(test)]
mod tests {
    use inference_type_checker::TypeCheckerBuilder;
    use inference_type_checker::typed_context::TypedContext;

    use super::{ChoiceClass, ChoicePlan, ReachPlans, plan_reachability_specs};
    use crate::errors::CodegenError;

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

    fn plans_of(source: &str) -> ReachPlans {
        plan_reachability_specs(&type_check(source)).expect("pre-scan should accept this source")
    }

    /// The plan of the program's single planned function.
    fn sole_plan(source: &str) -> ChoicePlan {
        let plans = plans_of(source);
        assert_eq!(
            plans.by_def.len(),
            1,
            "expected exactly one planned function"
        );
        plans.by_def.into_values().next().expect("checked above")
    }

    fn rejection(source: &str) -> CodegenError {
        plan_reachability_specs(&type_check(source))
            .expect_err("pre-scan should reject this source")
    }

    /// Pins the positional-view ↔ by-expression-view agreement: the k-th
    /// choice's `ExprId` maps to parameter index `entry_arity + k`.
    fn assert_views_agree(plan: &ChoicePlan) {
        assert_eq!(plan.by_expr.len(), plan.choices.len());
        for (k, choice) in plan.choices.iter().enumerate() {
            let k = u32::try_from(k).expect("test plans are small");
            assert_eq!(
                plan.by_expr.get(&choice.expr),
                Some(&(plan.entry_arity + k)),
                "choice {k} must sit at parameter index entry_arity + {k}"
            );
        }
    }

    #[test]
    fn forall_plain_and_assume_bodies_are_not_planned() {
        let plans = plans_of(
            "spec S {
              fn a() forall { let n: i32 = @; assert(n >= n); }
              fn b(x: i32) { assert(x >= x); }
              fn c() assume { let n: i32 = @; assert(n >= n); }
            }",
        );
        assert!(
            plans.by_def.is_empty(),
            "only exists/unique bodies are reachability-lowered"
        );
    }

    #[test]
    fn an_exists_body_is_planned_with_choices_in_source_order() {
        let plan = sole_plan(
            "fn g(v: i32) -> i32 { return v; }
            spec S {
              fn f(x: i32) exists {
                let a: i64 = @;
                let b: bool = @;
                assert(a > 0);
                assert(g(@) == x);
              }
            }",
        );
        assert_eq!(plan.entry_arity, 1, "one declared parameter");
        let classes: Vec<_> = plan.choices.iter().map(|c| c.class).collect();
        assert_eq!(
            classes,
            vec![ChoiceClass::I64, ChoiceClass::I32, ChoiceClass::I32],
            "declared scalar class decides the parameter class, in source order"
        );
        let named: Vec<_> = plan.choices.iter().map(|c| c.named).collect();
        assert_eq!(
            named,
            vec![true, true, false],
            "a bare `let x = @` is named; a call/operand `@` is anonymous"
        );
        assert_views_agree(&plan);
    }

    #[test]
    fn a_unique_body_is_planned() {
        let plan = sole_plan(
            "spec S {
              fn f() unique {
                let n: i32 = @;
                assert(n == 7);
              }
            }",
        );
        assert_eq!(plan.entry_arity, 0);
        assert_eq!(plan.choices.len(), 1);
        assert!(plan.choices[0].named);
        assert_views_agree(&plan);
    }

    #[test]
    fn choices_are_ordered_across_if_condition_then_and_else() {
        let plan = sole_plan(
            "fn g(v: i32) -> i32 { return v; }
            spec S {
              fn f(x: i32) exists {
                if g(@) == x {
                  let t: i32 = @;
                  assert(t >= t);
                } else {
                  let e: i32 = @;
                  assert(e >= e);
                }
              }
            }",
        );
        assert_eq!(plan.choices.len(), 3);
        let named: Vec<_> = plan.choices.iter().map(|c| c.named).collect();
        assert_eq!(
            named,
            vec![false, true, true],
            "condition `@` first, then the then-arm binding, then the else-arm binding"
        );
        assert_views_agree(&plan);
    }

    #[test]
    fn a_nested_exists_or_assume_block_contributes_its_choices() {
        let plan = sole_plan(
            "spec S {
              fn f(x: i32) exists {
                assume { assert(x > 0); }
                exists {
                  let n: i32 = @;
                  assert(n > x);
                }
              }
            }",
        );
        assert_eq!(plan.choices.len(), 1, "the nested block's `@` hoists");
        assert!(plan.choices[0].named);
        assert_views_agree(&plan);
    }

    #[test]
    fn a_compound_uzumaki_is_not_planned() {
        let plan = sole_plan(
            "spec S {
              fn f() exists {
                let n: i32 = @;
                let arr: [i32; 2] = @;
                assert(n >= n);
              }
            }",
        );
        assert_eq!(
            plan.choices.len(),
            1,
            "a compound `@` cannot become a scalar parameter and is left to the \
             obligation pass's rejection"
        );
        assert!(plan.choices[0].named);
        assert_views_agree(&plan);
    }

    #[test]
    fn entry_arity_counts_the_declared_parameters() {
        let plan = sole_plan(
            "spec S {
              fn f(x: i32, y: i64) exists {
                let n: i32 = @;
                assert(n >= n);
              }
            }",
        );
        assert_eq!(plan.entry_arity, 2);
        assert_eq!(
            plan.by_expr.get(&plan.choices[0].expr),
            Some(&2),
            "the first choice sits immediately after the declared parameters"
        );
    }

    #[test]
    fn a_spec_method_is_not_planned() {
        let plans = plans_of(
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
            plans.by_def.is_empty(),
            "spec methods carry no obligation channel (P009); only free functions are planned"
        );
    }

    #[test]
    fn a_declared_return_type_is_rejected() {
        cov_mark::check!(wasm_codegen_reach_declared_return_rejected);
        let err = rejection(
            "spec S {
              fn f() -> i32 exists {
                let n: i32 = @;
                assert(n > 0);
              }
            }",
        );
        let msg = err.to_string();
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
        let err = rejection(
            "spec S {
              fn f() unique {
                let n: i32 = @;
                assert(n == 1);
                return;
              }
            }",
        );
        let msg = err.to_string();
        assert!(
            msg.contains("'f'")
                && msg.contains("'unique'-quantified")
                && msg.contains("contains a `return` statement"),
            "the return-statement clause must name the function, kind, and offense: {msg}"
        );
    }

    #[test]
    fn the_declared_type_clause_wins_when_both_offend() {
        let err = rejection(
            "spec S {
              fn f() -> i32 exists {
                return 1;
              }
            }",
        );
        assert!(
            err.to_string().contains("declares a return type"),
            "the declared-type clause is checked first: {err}"
        );
    }
}
