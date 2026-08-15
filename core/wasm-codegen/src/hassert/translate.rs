//! The specification-body-to-`hassert` translation itself.
//!
//! One [`SpecFnTranslator`] per specification function walks its typed AST and
//! produces a single [`HAssert`] obligation. The scheme is a right-folded
//! statement translator with three modes ([`Mode::Univ`]/[`Mode::Exist`]/
//! [`Mode::Reach`]) and a small term translator that mirrors the WASM operators
//! code generation emits for the same expressions, so the obligation speaks the
//! same numeric language as the compiled body it constrains.
//!
//! ## Reachability bodies read their choices from the frame
//!
//! An `exists`/`unique`-quantified body translates in [`Mode::Reach`], whose
//! statement semantics are those of [`Mode::Exist`] (an `assume` block is a
//! conjunct, an `if` a strict disjunction of guarded conjunctions) but whose
//! `@` handling is entirely different: the downstream judgment runs the
//! compiled body and quantifies the hidden trailing choice *parameters* code
//! generation appended for each scalar `@`, so the payload binds each `@` to
//! the [`HTerm::Local`] slot of its own choice parameter — no `HA_ex` binder
//! (the predicate already quantifies the choices operationally; a binder would
//! double-quantify and detach the payload from the frame) and no
//! `HA_has_type` slot guard (the payload denotes against the *real* reached
//! frame, where every slot carries its runtime type; the guard discipline
//! below targets `ValidSpec`'s unconstrained valuations). Both the compiled
//! body and this translator read the same pre-scan
//! [`ChoicePlan`], keyed by `ExprId`, so a payload slot index equals the
//! frame index of the same choice by construction rather than by parallel
//! counting. `HA_ex` still appears in reachability payloads, but only for
//! short-circuit witnesses, whose machinery is mode-independent.
//!
//! ## Logical variables carry levels, not indices, until the end
//!
//! While a tree is under construction every [`HTerm::LVar`] stores an *absolute
//! binder level* (counted from the outside), not a de Bruijn index. A single
//! [`lower_assert`] pass then rewrites each level to the index it
//! has at its own depth. Levels are position-independent, so a pure `let` that
//! captures an `HA_ex`-bound variable (a prover-chosen `@` or a pinned witness)
//! and is used further inside more binders needs no re-indexing at its use site
//! — the final pass alone resolves it. This is what keeps
//! `exists { let a = @; let t = a + 1; let b = @; assert(b > t); }` correct
//! without shifting already-built subterms.
//!
//! ## Universal slots state their own typing
//!
//! wasm-verifier's `ValidSpec` evaluates an obligation through a strong-Kleene
//! strictification (`Assertions.ktrue`) over valuations it constrains in no way,
//! so a slot readout may simply fail to denote. A payload that reads a universal
//! slot is dischargeable only when it says so itself, which is why every slot
//! introduction — a scalar parameter, `let x: T = @`, a `@` in call-argument
//! position — records a pending `HA_has_type (T_local i) T_i32`/`T_i64` guard
//! that the next *structural* statement in the same block discharges as the
//! antecedent of its own claim. `T_local` is prover-uncontrolled and bears no
//! `T_app`, so such an antecedent is honestly refutable rather than a vacuous
//! escape: where the slot is undefined or mis-typed the guard is refuted, and
//! everywhere else it hands the proof the value together with its typing. A body
//! that introduces no slot is untouched — its antecedent is `⊤`, which
//! [`HAssert::imp`] absorbs (issue #353).
//!
//! ## Short-circuit `&&`/`||` become pinned witnesses
//!
//! The term language is strict: every constructor demands each operand denote,
//! and it has no conditional. Code generation lowers `a && b` to
//! `if a != 0 then b else 0` and `a || b` to `if a != 0 then 1 else b`, so the
//! right operand is evaluated on one arm only. An eager `T_binop` term would
//! therefore demand a value the program never computes, and
//! `x == 0 || 10 / x == 10 / x` — true for every `i32` — would become refutable
//! at `x = 0`.
//!
//! Assertion position never had that problem: `&&`/`||` split into `HA_and`/
//! `Hor`, and neither demands its right conjunct on the arm the source skips.
//! Term position borrows the same escape hatch one layer up. A term-position
//! `&&`/`||` allocates a fresh `HA_ex`-bound *witness* and pins it with a
//! two-armed constraint mirroring the compiled control flow — for `a || b`,
//! `Hor (nz a ∧ v = 1) (eqz a ∧ … ∧ v = b)`. The witness is the operator's
//! term; its constraint is planted where the enclosing statement's atom is
//! wrapped in the binder.
//!
//! Placement follows evaluation. A constraint planted unconditionally is
//! demanded unconditionally, which re-creates the very bug one level up, so the
//! constraints a *right operand of `&&`/`||`* introduces are captured and moved
//! into the arm that evaluates it — in term position and in both assertion
//! polarities alike. Every other operand position is evaluated unconditionally
//! and keeps its constraints pending for the atom. The binder itself always
//! hoists to the atom even when its constraint moved deeper; the one left
//! behind is simply unconstrained, which is sound because the skipped arm never
//! reads it.
//!
//! Witnesses are the reason [`HAssert::Ex`] now appears under [`Mode::Univ`]:
//! before this, a binder could only come from an existential `@`.

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, ExprId, IdentId, StmtId, TypeId};
use inference_ast::nodes::{
    ArgKind, BlockKind, Def, Expr, Location, OperatorKind, Stmt, UnaryOperatorKind,
};
use inference_fn_key::FnKey;
use inference_hassert::{HAssert, HBinop, HConst, HFnRef, HNumType, HRelop, HTerm};
use inference_type_checker::type_info::{NumberType, TypeInfo, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;
use rustc_hash::FxHashMap;

use super::CalleeIndex;
use super::diag::{HassertDiagnostic, PCode};
use super::reach::ChoicePlan;

/// Polarity of the surrounding quantification.
///
/// The mode decides how `@`, `assume`, `if`, and `==` are encoded. It does
/// *not* decide whether an `HA_ex` binder can appear: a short-circuit witness
/// is bound in either mode.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Universal context: `assume` filters (antecedent), `if` is a
    /// conjunction of guarded implications, `@` takes a `T_local` slot.
    Univ,
    /// Existential context: `assume` constrains the witness (conjunct), `if` is
    /// a strict disjunction of guarded conjunctions, `@` binds an `HA_ex`
    /// logical variable.
    Exist,
    /// Reachability context (`exists`/`unique`-quantified body): statement
    /// semantics of [`Mode::Exist`], but `@` reads the [`HTerm::Local`] slot
    /// of the choice parameter the pre-scan planned for it — the downstream
    /// judgment quantifies the choices operationally, so no binder and no
    /// typing guard is introduced.
    Reach,
}

/// The reachability context of the function being translated, present exactly
/// while its `exists`/`unique` body translates in [`Mode::Reach`].
struct ReachCtx<'a> {
    /// The pre-scan's choice plan for this function — the same map code
    /// generation consumed for the signature suffix and the `@` lowering.
    plan: &'a ChoicePlan,
    /// Whether the body is `unique`-quantified. An anonymous call-argument
    /// `@` is rejected there ([`PCode::P012`]): it is excluded from the
    /// source-visible observation, which would silently weaken uniqueness.
    unique: bool,
}

/// What an identifier resolves to inside a specification body.
#[derive(Clone)]
enum Binding {
    /// A universally-quantified slot (`T_local n`).
    Slot(u32),
    /// An existential logical variable, stored as an absolute binder *level*.
    Level(u32),
    /// A pure `let`, inlined as its translated term. Any [`HTerm::LVar`] inside
    /// keeps its absolute level, so the term re-indexes correctly wherever it is
    /// later embedded.
    Term(HTerm),
}

/// Why a callee cannot serve a specification claim, deciding which diagnostic
/// the call site raises.
enum CalleeError {
    /// Not a module-defined deterministic function, for this reason —
    /// [`PCode::P005`].
    NotApplicable(&'static str),
    /// An `exists`/`unique`-quantified spec function ([`PCode::P011`]),
    /// carrying its quantifier word. A reachability spec function is the
    /// subject of a judgment about running its own body with its own choices,
    /// not a callable — and its compiled form has hidden trailing choice
    /// parameters no call site supplies.
    ReachabilitySpec { kind: &'static str },
}

/// The classification of a call's result, deciding whether it can be a term.
enum ResultClass {
    /// A single scalar (bool, integer, or enum) — a valid `T_app` term.
    Scalar,
    /// No result (`unit`) — only realizable as an `HA_app_ok` statement.
    Void,
    /// A compound result (array or struct) — memory-backed, not a term.
    Compound,
}

/// Pending binders taken off the stack as one group, ready to be wrapped.
///
/// The level of the group's first binder travels with the definitions because
/// wrapping needs both: the definitions become the `HA_ex` bodies, and the
/// levels name the variables whose occurrence decides which definitions survive.
/// Deriving the level at the split — rather than reading `depth` at the wrap —
/// is what keeps a group correct when binders allocated before it are left
/// behind for an enclosing wrap.
struct PendingGroup {
    /// One definition per binder, in allocation order; the first is outermost.
    defs: Vec<HAssert>,
    /// The absolute level of the first binder in `defs`.
    base_level: u32,
}

pub(super) struct SpecFnTranslator<'a> {
    arena: &'a AstArena,
    ctx: &'a TypedContext,
    module_path: &'a [String],
    spec_name: &'a str,
    callee: &'a CalleeIndex,
    /// Next universal slot number. Parameters take `0..P-1`; each universal `@`
    /// takes the next in encounter order. Never rewound — slots are global to
    /// the function.
    slots: u32,
    /// Number of committed existential binders enclosing the current point.
    depth: u32,
    /// Existential binders introduced within the statement currently being
    /// translated and not yet wrapped around its atom, in allocation order:
    /// entry `i` is the *defining constraint* of the binder at level
    /// `depth + i`.
    ///
    /// A binder nothing pins carries [`HAssert::True`] — a call-argument `@`,
    /// which the prover chooses freely, or a witness whose constraint moved
    /// into a conditional arm. Every allocation site drains its own binders
    /// around its own statement, so this is empty at every statement boundary.
    pending: Vec<HAssert>,
    /// Typing guards for the universal slots introduced since the last
    /// structural statement, in introduction order, awaiting their drain.
    univ_guards: Vec<HAssert>,
    /// The reachability context, `Some` exactly while an `exists`/`unique`
    /// body translates in [`Mode::Reach`].
    reach: Option<ReachCtx<'a>>,
    env: FxHashMap<String, Binding>,
    diags: Vec<HassertDiagnostic>,
}

impl<'a> SpecFnTranslator<'a> {
    pub(super) fn new(
        ctx: &'a TypedContext,
        module_path: &'a [String],
        spec_name: &'a str,
        callee: &'a CalleeIndex,
    ) -> Self {
        Self {
            arena: ctx.arena(),
            ctx,
            module_path,
            spec_name,
            callee,
            slots: 0,
            depth: 0,
            pending: Vec::new(),
            univ_guards: Vec::new(),
            reach: None,
            env: FxHashMap::default(),
            diags: Vec::new(),
        }
    }

    /// Removes and returns the diagnostics gathered so far.
    pub(super) fn take_diagnostics(&mut self) -> Vec<HassertDiagnostic> {
        std::mem::take(&mut self.diags)
    }

    /// Translates one specification free function into its obligation.
    ///
    /// A `forall`-quantified or plain (`Regular`) body is translated in
    /// universal mode; an `exists`/`unique`-quantified one in reachability
    /// mode, reading its `@` slots from `plan` (the same pre-scan plan code
    /// generation consumed, so payload slots and compiled frame indices agree
    /// by construction). An `assume`-quantified body states no property —
    /// `assume` is not a quantifier — and yields [`PCode::P001`] plus a
    /// trivial `⊤` obligation (discarded, since any diagnostic aborts code
    /// generation).
    pub(super) fn translate_fn(&mut self, def_id: DefId, plan: Option<&'a ChoicePlan>) -> HAssert {
        let (args, body) = match &self.arena[def_id].kind {
            Def::Function { args, body, .. } => (args.clone(), *body),
            _ => return HAssert::True,
        };

        let body_kind = self.arena[body].block_kind;
        let mode = match body_kind {
            BlockKind::Forall | BlockKind::Regular => Mode::Univ,
            BlockKind::Exists | BlockKind::Unique => {
                let plan = plan.expect(
                    "an exists/unique-bodied spec free function reached translation without a \
                     reachability plan — the pre-scan walks the same spec structure this pass \
                     iterates, so a missing plan means the two walks disagree",
                );
                self.reach = Some(ReachCtx {
                    plan,
                    unique: body_kind == BlockKind::Unique,
                });
                Mode::Reach
            }
            BlockKind::Assume => {
                let name = self.arena.def_name(def_id).to_string();
                self.error(
                    PCode::P001,
                    self.arena[def_id].location,
                    format!(
                        "spec function `{name}` has an `assume` body, which states no property: \
                         `assume` is not a quantifier — it only reinterprets a failing path as a \
                         filtered-out one for an enclosing `forall`, so with nothing enclosing it \
                         there is no claim to prove; give the function a `forall` body and nest \
                         the `assume` inside it, ahead of the assertion the surviving paths must \
                         satisfy"
                    ),
                );
                return HAssert::True;
            }
        };

        self.bind_parameters(&args, mode);

        let stmts = self.arena[body].stmts.clone();
        let raw = self.t_stmts(&stmts, mode);
        // Rewrite every logical-variable level to the de Bruijn index it has at
        // its own binder depth, now that the whole tree (and thus every binder)
        // is known.
        lower_assert(&raw, 0)
    }

    /// Binds each parameter to a slot in declaration order.
    fn bind_parameters(&mut self, args: &[inference_ast::nodes::ArgData], mode: Mode) {
        for arg in args {
            match &arg.kind {
                ArgKind::Named { name, ty, .. } => {
                    let slot = self.parameter_slot(arg.location, *ty, mode);
                    self.env
                        .insert(self.arena[*name].name.clone(), Binding::Slot(slot));
                }
                ArgKind::Ignored { ty } => {
                    let _ = self.parameter_slot(arg.location, *ty, mode);
                }
                ArgKind::SelfRef { .. } | ArgKind::TypeOnly(_) => {}
            }
        }
    }

    /// Consumes the slot a parameter occupies and, under universal
    /// quantification, records the typing guard its readers depend on. An
    /// ignored parameter is guarded like a named one: the guard is inert for a
    /// slot the payload never reads, and uniformity beats a use analysis. A
    /// non-scalar parameter type is [`PCode::P004`]; its slot is still consumed
    /// so later slot numbers stay aligned with the source, and it contributes
    /// no guard — it has no numeric typing to state.
    ///
    /// A reachability payload pushes no guard for any slot: it denotes against
    /// the frame an actual execution reaches, where every slot already carries
    /// its runtime type, so a stated typing would be dead weight the downstream
    /// exemplars do not carry.
    fn parameter_slot(&mut self, location: Location, ty: TypeId, mode: Mode) -> u32 {
        let scalar = self.type_is_scalar(ty);
        if !scalar {
            self.error(PCode::P004, location, self.non_scalar_message(ty));
        }
        let slot = self.next_slot();
        if scalar && mode == Mode::Univ {
            let width = self.declared_class(ty);
            self.push_univ_guard(slot, width);
        }
        slot
    }

    // ----- statement-list translation -----------------------------------

    /// The right-folded statement translator. `⊤` for the empty list; each
    /// statement contributes a conjunct (or an implication antecedent, for a
    /// universal `assume`) over the translation of the rest.
    fn t_stmts(&mut self, stmts: &[StmtId], mode: Mode) -> HAssert {
        debug_assert!(
            self.pending.is_empty(),
            "a binder leaked across a statement boundary: every allocation site drains its own \
             binders around its own statement's atom, so a binder still pending here would be \
             wrapped around a later statement at a level that no longer names it"
        );
        let Some((first, rest)) = stmts.split_first() else {
            // A slot introduced with nothing left to read it guards nothing.
            self.univ_guards.clear();
            return HAssert::True;
        };
        let stmt_id = *first;
        match &self.arena[stmt_id].kind {
            Stmt::VarDef {
                name, ty, value, ..
            } => {
                let (name, ty, value) = (*name, *ty, *value);
                self.t_var_def(name, ty, value, rest, mode)
            }
            Stmt::Assert { expr } => {
                let expr = *expr;
                let atom = self.eval_atom(|s| s.p_expr(expr, mode));
                self.t_structural(atom, rest, mode)
            }
            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                let (condition, then_block, else_block) = (*condition, *then_block, *else_block);
                let guarded = self.t_if(condition, then_block, else_block, mode);
                self.t_structural(guarded, rest, mode)
            }
            Stmt::Block(block_id) => {
                let block_id = *block_id;
                self.t_block(block_id, rest, mode)
            }
            Stmt::Return { expr } => {
                let expr = *expr;
                // A returned expression is validated (it may surface a
                // diagnostic) but contributes nothing; `return` is reachable
                // only in a `Regular`-kind body (analysis bans it under any
                // non-deterministic block).
                self.discard_term(expr, mode);
                self.t_stmts(rest, mode)
            }
            Stmt::Expr(expr) => {
                let expr = *expr;
                self.t_expr_stmt(expr, rest, mode)
            }
            Stmt::ConstDef(def_id) => {
                let def_id = *def_id;
                self.bind_const(def_id, rest, mode)
            }
            Stmt::TypeDef { .. } => self.t_stmts(rest, mode),
            Stmt::Assign { .. } => {
                self.error(
                    PCode::P003,
                    self.arena[stmt_id].location,
                    "reassignment is not supported in specification bodies; bind a new `let` \
                     instead"
                        .to_string(),
                );
                self.t_stmts(rest, mode)
            }
            Stmt::Loop { .. } => {
                self.error_no_encoding(self.arena[stmt_id].location, "`loop`");
                self.t_stmts(rest, mode)
            }
            Stmt::Break => {
                self.error_no_encoding(self.arena[stmt_id].location, "`break`");
                self.t_stmts(rest, mode)
            }
        }
    }

    /// A structural statement's contribution: the universal-slot guards pending
    /// at this point become the antecedent of the statement's own claim
    /// conjoined with the rest of the block.
    ///
    /// `contribution` must already be translated *and* already wrapped in the
    /// binders it introduced, so a `@` in call-argument position inside this
    /// statement joins the same drain and an `HA_ex` built inside it lands in
    /// the consequent rather than over the antecedent. Draining *before* the
    /// rest is what scopes a slot over every later reader of it rather than
    /// letting a deeper structural statement capture it into a narrower
    /// antecedent.
    fn t_structural(&mut self, contribution: HAssert, rest: &[StmtId], mode: Mode) -> HAssert {
        let antecedent = self.drain_guards_over(HAssert::True);
        debug_assert!(
            mode == Mode::Univ || antecedent == HAssert::True,
            "typing guards pend only under universal quantification: existential translation \
             introduces no universal slot, and reachability translation deliberately pushes no \
             guard for any slot (its payload denotes against the real reached frame), so none \
             can be pending in either mode"
        );
        let tail = self.t_stmts(rest, mode);
        HAssert::imp(antecedent, HAssert::and(contribution, tail))
    }

    /// Removes every pending universal-slot guard and right-folds it over
    /// `seed`: `g₁ ∧ (g₂ ∧ (… ∧ seed))`.
    fn drain_guards_over(&mut self, seed: HAssert) -> HAssert {
        std::mem::take(&mut self.univ_guards)
            .into_iter()
            .rev()
            .fold(seed, |acc, guard| HAssert::and(guard, acc))
    }

    /// Records the typing a newly-introduced universal slot depends on.
    fn push_univ_guard(&mut self, slot: u32, width: HNumType) {
        self.univ_guards
            .push(HAssert::HasType(HTerm::Local(slot), width));
    }

    /// `let` translation. A bare `@` right-hand side binds a slot (universal) or
    /// an existential binder; any other right-hand side is a pure `let`, inlined
    /// as a term.
    fn t_var_def(
        &mut self,
        name_id: IdentId,
        ty: TypeId,
        value: Option<ExprId>,
        rest: &[StmtId],
        mode: Mode,
    ) -> HAssert {
        let name = self.arena[name_id].name.clone();
        let Some(value_expr) = value else {
            // `let x: T;` without an initializer binds nothing to translate.
            return self.t_stmts(rest, mode);
        };

        if matches!(self.arena[value_expr].kind, Expr::Uzumaki) {
            let scalar = self.type_is_scalar(ty);
            if !scalar {
                self.emit_non_scalar_uzumaki(ty, self.arena[value_expr].location, mode);
            }
            return match mode {
                Mode::Univ => {
                    let slot = self.next_slot();
                    if scalar {
                        let width = self.declared_class(ty);
                        self.push_univ_guard(slot, width);
                    }
                    self.env.insert(name, Binding::Slot(slot));
                    self.t_stmts(rest, Mode::Univ)
                }
                Mode::Exist => {
                    let level = self.depth;
                    self.env.insert(name, Binding::Level(level));
                    self.depth += 1;
                    let body = self.t_stmts(rest, Mode::Exist);
                    self.depth -= 1;
                    HAssert::ex(body)
                }
                Mode::Reach => {
                    // The choice already lives in its appended parameter: code
                    // generation normalized the drawn value into the parameter
                    // itself, so the payload reads the same frame slot the
                    // compiled body binds — no binder, no guard. A non-scalar
                    // `@` was never planned and already carries its diagnostic
                    // above, so only the scalar case binds.
                    if scalar {
                        let slot = self.choice_slot(value_expr);
                        self.env.insert(name, Binding::Slot(slot));
                    }
                    self.t_stmts(rest, Mode::Reach)
                }
            };
        }

        // Pure `let`: translate the right-hand side once, then inline it. A
        // call-argument `@` or a short-circuit witness in the right-hand side
        // introduces binders that must scope over the rest of the block, since
        // that is where the inlined term is read.
        let base = self.pending.len();
        let term = self.term(value_expr, mode);
        let group = self.split_pending(base);
        self.env.insert(name, Binding::Term(term));
        self.scoped_over_rest(group, rest, mode)
    }

    /// Scopes the binders `defs` introduced by a pure `let` or a `const` over
    /// the rest of the block, dominated by the universal-slot guards pending at
    /// the binding.
    ///
    /// A pure `let` is not a structural statement, so a guard introduced earlier
    /// in the block is still undrained here. A witness constraint reads the very
    /// slots those guards type, so leaving the guards pending would put them
    /// *inside* the `HA_ex` — demanding the constraint at a slot nothing has
    /// typed yet, which is exactly the escape the slot guards exist to close.
    /// Draining first yields `Himpl guard (HA_ex (constraint ∧ …))`.
    ///
    /// A binding that introduced no binder drains nothing, so a `let` without a
    /// witness keeps the guard pending for the next structural statement.
    fn scoped_over_rest(&mut self, group: PendingGroup, rest: &[StmtId], mode: Mode) -> HAssert {
        if group.defs.is_empty() {
            return self.t_stmts(rest, mode);
        }
        let antecedent = self.drain_guards_over(HAssert::True);
        let body = self.scoped_under(group, |s| s.t_stmts(rest, mode));
        HAssert::imp(antecedent, body)
    }

    /// Commits a group as enclosing binders for the duration of `f`, then wraps
    /// what `f` built in them. The binders keep the levels they were allocated
    /// at, so `f` may read them and any binder it allocates itself takes a level
    /// beyond them.
    fn scoped_under<F>(&mut self, group: PendingGroup, f: F) -> HAssert
    where
        F: FnOnce(&mut Self) -> HAssert,
    {
        let outer_depth = self.depth;
        self.depth = group.base_level + level_count(group.defs.len());
        let body = f(self);
        self.depth = outer_depth;
        wrap_existentials(body, group)
    }

    /// A bare `if`, translated to its own contribution alone — the caller folds
    /// in the rest of the block. Universal mode is a conjunction of guarded
    /// implications (`nz`/`eqz` guards); existential mode is a strict disjunction
    /// of guarded conjunctions, so a non-denoting condition cannot fabricate a
    /// witness.
    ///
    /// The condition is translated once and read on both arms, so the binders it
    /// introduces are committed around the whole contribution rather than per
    /// arm. They stay inside what the caller's drain makes the consequent, which
    /// is why the guards need no draining here.
    fn t_if(
        &mut self,
        condition: ExprId,
        then_block: BlockId,
        else_block: Option<BlockId>,
        mode: Mode,
    ) -> HAssert {
        let base = self.pending.len();
        let cond = self.term(condition, mode);
        let group = self.split_pending(base);
        self.scoped_under(group, |s| match mode {
            Mode::Univ => {
                let then_h = s.scoped_block(then_block, s.branch_mode(then_block, Mode::Univ));
                if let Some(else_id) = else_block {
                    let else_h = s.scoped_block(else_id, s.branch_mode(else_id, Mode::Univ));
                    HAssert::and(
                        HAssert::imp(HAssert::nz(cond.clone()), then_h),
                        HAssert::imp(HAssert::eqz(cond), else_h),
                    )
                } else {
                    HAssert::imp(HAssert::nz(cond), then_h)
                }
            }
            // Reachability shares the existential shape: a branch's claim holds
            // on the arm the run takes, and its statements translate in the
            // enclosing mode so a reachability branch keeps reading its choice
            // parameters.
            Mode::Exist | Mode::Reach => {
                s.check_branch_forall(then_block);
                let then_h = s.scoped_block(then_block, mode);
                if let Some(else_id) = else_block {
                    s.check_branch_forall(else_id);
                    let else_h = s.scoped_block(else_id, mode);
                    HAssert::or(
                        HAssert::and(HAssert::nz(cond.clone()), then_h),
                        HAssert::and(HAssert::eqz(cond), else_h),
                    )
                } else {
                    // No `else`: the else path is trivially satisfiable, so a
                    // determinate false condition alone discharges it.
                    HAssert::or(
                        HAssert::and(HAssert::nz(cond.clone()), then_h),
                        HAssert::eqz(cond),
                    )
                }
            }
        })
    }

    /// A block statement, dispatched on its kind. Under universal
    /// quantification an `assume` body translates existentially (its `@`s read
    /// as "some choice satisfies the filter"); `assume` flips between
    /// implication (universal) and conjunction (existential and reachability).
    ///
    /// A universal `assume` fuses the pending slot guards into the antecedent it
    /// already builds, so `let n: i32 = @; assume { assert(n > 1); }` states the
    /// slot's typing and the source filter as one hypothesis.
    fn t_block(&mut self, block_id: BlockId, rest: &[StmtId], mode: Mode) -> HAssert {
        let kind = self.arena[block_id].block_kind;
        // Inside a reachability body, a nested `assume`/`exists` block keeps
        // translating in reachability mode: its `@`s are choice parameters of
        // the enclosing function (the pre-scan hoisted them), so an
        // existential binder here would name a value the frame already holds.
        let nested_exist_mode = if mode == Mode::Reach {
            Mode::Reach
        } else {
            Mode::Exist
        };
        match kind {
            BlockKind::Assume => {
                let body = self.scoped_block(block_id, nested_exist_mode);
                match mode {
                    Mode::Univ => {
                        let antecedent = self.drain_guards_over(body);
                        HAssert::imp(antecedent, self.t_stmts(rest, Mode::Univ))
                    }
                    Mode::Exist | Mode::Reach => HAssert::and(body, self.t_stmts(rest, mode)),
                }
            }
            BlockKind::Regular => {
                let body = self.scoped_block(block_id, mode);
                self.t_structural(body, rest, mode)
            }
            BlockKind::Forall => {
                if matches!(mode, Mode::Exist | Mode::Reach) {
                    self.error(
                        PCode::P007,
                        self.arena[block_id].location,
                        "a `forall` block inside an `exists` block is not yet supported in \
                         assertion emission"
                            .to_string(),
                    );
                }
                let body = self.scoped_block(block_id, mode);
                self.t_structural(body, rest, mode)
            }
            BlockKind::Exists => {
                let body = self.scoped_block(block_id, nested_exist_mode);
                self.t_structural(body, rest, mode)
            }
            BlockKind::Unique => {
                self.error_no_encoding(self.arena[block_id].location, "`unique` block");
                self.t_stmts(rest, mode)
            }
        }
    }

    /// A bare expression statement. A call becomes an `HA_app_ok` obligation at
    /// any result arity; any other expression is validated and dropped.
    fn t_expr_stmt(&mut self, expr: ExprId, rest: &[StmtId], mode: Mode) -> HAssert {
        if matches!(self.arena[expr].kind, Expr::FunctionCall { .. }) {
            let atom = self.eval_atom(|s| s.app_ok(expr, mode));
            self.t_structural(atom, rest, mode)
        } else {
            self.discard_term(expr, mode);
            self.t_stmts(rest, mode)
        }
    }

    /// Translates an expression for its diagnostics alone and drops both the
    /// term and every binder it introduced.
    ///
    /// The expression contributes no claim, so nothing wraps those binders.
    /// Leaving them pending would hand them to a later statement's atom, which
    /// wraps them at a depth where their levels name something else.
    fn discard_term(&mut self, expr: ExprId, mode: Mode) {
        let base = self.pending.len();
        let _ = self.term(expr, mode);
        self.pending.truncate(base);
    }

    /// Binds a block-local `const` as a pure term, exactly like a pure `let` —
    /// binders in its initializer scope over the rest of the block, where the
    /// inlined term is read.
    fn bind_const(&mut self, def_id: DefId, rest: &[StmtId], mode: Mode) -> HAssert {
        let Def::Constant { name, value, .. } = &self.arena[def_id].kind else {
            return self.t_stmts(rest, mode);
        };
        let (name, value) = (*name, *value);
        let base = self.pending.len();
        let term = self.term(value, mode);
        let group = self.split_pending(base);
        self.env
            .insert(self.arena[name].name.clone(), Binding::Term(term));
        self.scoped_over_rest(group, rest, mode)
    }

    // ----- assertion-position translators -------------------------------

    /// Truthiness of an assertion expression.
    ///
    /// `&&`/`||` split into `HA_and`/`Hor`, which is already faithful to
    /// short-circuit evaluation — neither demands its right side on the arm the
    /// source skips. The right side's own witness constraints must join it
    /// there, or they would be demanded on both arms.
    fn p_expr(&mut self, expr: ExprId, mode: Mode) -> HAssert {
        match &self.arena[expr].kind {
            Expr::Parenthesized { expr } => {
                let expr = *expr;
                return self.p_expr(expr, mode);
            }
            Expr::PrefixUnary {
                expr,
                op: UnaryOperatorKind::Not,
            } => {
                let expr = *expr;
                return self.n_expr(expr, mode);
            }
            Expr::Binary { left, right, op } => {
                let (left, right, op) = (*left, *right, op.clone());
                match op {
                    OperatorKind::And => {
                        let left_h = self.p_expr(left, mode);
                        let (right_h, right_defs) =
                            self.capture_definitions(|s| s.p_expr(right, mode));
                        return HAssert::and(left_h, HAssert::and(right_defs, right_h));
                    }
                    OperatorKind::Or => {
                        let left_h = self.p_expr(left, mode);
                        let (right_h, right_defs) =
                            self.capture_definitions(|s| s.p_expr(right, mode));
                        return HAssert::or(left_h, HAssert::and(right_defs, right_h));
                    }
                    OperatorKind::Eq
                    | OperatorKind::Ne
                    | OperatorKind::Lt
                    | OperatorKind::Le
                    | OperatorKind::Gt
                    | OperatorKind::Ge => {
                        return self.p_comparison(left, right, &op, mode);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        // Atom: a bool variable, a call, a literal, …
        HAssert::nz(self.term(expr, mode))
    }

    /// Falsiness of an assertion expression (the De Morgan dual of
    /// [`Self::p_expr`]). The right side's witness constraints ride with it into
    /// the dual arm, for the same reason.
    fn n_expr(&mut self, expr: ExprId, mode: Mode) -> HAssert {
        match &self.arena[expr].kind {
            Expr::Parenthesized { expr } => {
                let expr = *expr;
                return self.n_expr(expr, mode);
            }
            Expr::PrefixUnary {
                expr,
                op: UnaryOperatorKind::Not,
            } => {
                let expr = *expr;
                return self.p_expr(expr, mode);
            }
            Expr::Binary {
                left,
                right,
                op: OperatorKind::And,
            } => {
                let (left, right) = (*left, *right);
                let left_h = self.n_expr(left, mode);
                let (right_h, right_defs) = self.capture_definitions(|s| s.n_expr(right, mode));
                return HAssert::or(left_h, HAssert::and(right_defs, right_h));
            }
            Expr::Binary {
                left,
                right,
                op: OperatorKind::Or,
            } => {
                let (left, right) = (*left, *right);
                let left_h = self.n_expr(left, mode);
                let (right_h, right_defs) = self.capture_definitions(|s| s.n_expr(right, mode));
                return HAssert::and(left_h, HAssert::and(right_defs, right_h));
            }
            _ => {}
        }
        // Atom: the strict positive zero-equality.
        HAssert::eqz(self.term(expr, mode))
    }

    /// A comparison in assertion position. `==` is the one operator whose
    /// encoding depends on the mode: strict `term_eq` on the existential and
    /// reachability paths, `nz(relop)` under universal quantification — which
    /// the verifier's strictified reading makes just as strict (a non-denoting relop refutes
    /// the obligation rather than discharging it, which is what the slot
    /// typing guards are for).
    ///
    /// Every other comparison, `!=` included, is the bare `nz(relop)`. A
    /// per-side `HA_defined` conjunct would add nothing: wasm-verifier's
    /// `ValidSpec` evaluates the payload through a strong-Kleene strictification
    /// (`Assertions.ktrue`) under which a negated equality already demands that
    /// both sides denote, so the definedness a disequality needs comes from the
    /// relop itself rather than from an emitted conjunct.
    fn p_comparison(
        &mut self,
        left: ExprId,
        right: ExprId,
        op: &OperatorKind,
        mode: Mode,
    ) -> HAssert {
        let (num_ty, unsigned) = self.operand_class(left);
        let ta = self.term(left, mode);
        let tb = self.term(right, mode);
        match op {
            OperatorKind::Eq => match mode {
                Mode::Univ => HAssert::nz(relop(num_ty, HRelop::Eq, ta, tb)),
                Mode::Exist | Mode::Reach => HAssert::TermEq(ta, tb),
            },
            OperatorKind::Ne => HAssert::nz(relop(num_ty, HRelop::Ne, ta, tb)),
            OperatorKind::Lt => HAssert::nz(relop(num_ty, signed_relop(unsigned, Lt), ta, tb)),
            OperatorKind::Le => HAssert::nz(relop(num_ty, signed_relop(unsigned, Le), ta, tb)),
            OperatorKind::Gt => HAssert::nz(relop(num_ty, signed_relop(unsigned, Gt), ta, tb)),
            OperatorKind::Ge => HAssert::nz(relop(num_ty, signed_relop(unsigned, Ge), ta, tb)),
            _ => unreachable!("p_comparison only handles comparison operators"),
        }
    }

    // ----- term translation ---------------------------------------------

    /// Translates an expression to a term. On an untranslatable expression a
    /// diagnostic is recorded and a zero sentinel returned, so a single pass
    /// still collects every problem before the build is discarded.
    fn term(&mut self, expr: ExprId, mode: Mode) -> HTerm {
        match &self.arena[expr].kind {
            Expr::Parenthesized { expr } => {
                let expr = *expr;
                self.term(expr, mode)
            }
            Expr::NumberLiteral { value } => {
                let value = value.clone();
                self.number_literal(expr, &value)
            }
            Expr::BoolLiteral { value } => HTerm::Const(HConst::I32(i32::from(*value))),
            Expr::TypeMemberAccess {
                expr: type_expr,
                name,
            } => {
                let (type_expr, name) = (*type_expr, *name);
                self.enum_variant(expr, type_expr, name)
            }
            Expr::Identifier(ident_id) => {
                let ident_id = *ident_id;
                self.identifier(expr, ident_id)
            }
            Expr::Binary { left, right, op } => {
                let (left, right, op) = (*left, *right, op.clone());
                self.binary(expr, left, right, &op, mode)
            }
            Expr::PrefixUnary { expr: inner, op } => {
                let (inner, op) = (*inner, op.clone());
                self.unary(expr, inner, &op, mode)
            }
            Expr::FunctionCall { .. } => self.call_term(expr, mode),
            Expr::Uzumaki => {
                self.error(
                    PCode::P006,
                    self.arena[expr].location,
                    "uzumaki (@) can only be bound by a `let` or passed as a call argument \
                     inside a translated spec body"
                        .to_string(),
                );
                zero_sentinel()
            }
            Expr::ArrayIndexAccess { .. } => {
                self.error_no_encoding(self.arena[expr].location, "array indexing");
                zero_sentinel()
            }
            Expr::MemberAccess { .. } => {
                self.error_no_encoding(self.arena[expr].location, "struct field access");
                zero_sentinel()
            }
            Expr::StructLiteral { .. } => {
                self.error_no_encoding(self.arena[expr].location, "a struct literal");
                zero_sentinel()
            }
            Expr::ArrayLiteral { .. } => {
                self.error_no_encoding(self.arena[expr].location, "an array literal");
                zero_sentinel()
            }
            Expr::StringLiteral { .. } => {
                self.error_no_encoding(self.arena[expr].location, "a string literal");
                zero_sentinel()
            }
            Expr::UnitLiteral | Expr::Type(_) => {
                self.error(
                    PCode::P004,
                    self.arena[expr].location,
                    "type `unit` cannot appear in a specification term; only bool, integer, \
                     and enum values can"
                        .to_string(),
                );
                zero_sentinel()
            }
        }
    }

    /// A number literal, parsed and widened exactly as code generation lowers it.
    ///
    /// The recorded type is an invariant of the phases that ran before this one,
    /// not something to fall back from: a literal that arrives untyped means an
    /// earlier phase failed to type it, and denoting it `i32` anyway would put a
    /// constant into a proof obligation that the compiled program never computes
    /// — the obligation would then be about a different program than the one that
    /// runs.
    fn number_literal(&mut self, expr: ExprId, value: &str) -> HTerm {
        let kind = self
            .ctx
            .get_node_typeinfo(node_expr(expr))
            .map(|t| t.kind)
            .expect(
                "literal reached hassert translation without a recorded type — the type checker \
                 records a type for every literal",
            );
        match kind {
            TypeInfoKind::Number(width) => HTerm::Const(number_const(width, value)),
            other => {
                self.error(
                    PCode::P004,
                    self.arena[expr].location,
                    format!(
                        "type `{other}` cannot appear in a specification term; only bool, \
                         integer, and enum values can"
                    ),
                );
                zero_sentinel()
            }
        }
    }

    /// An enum variant reference, lowered to its zero-based tag constant.
    fn enum_variant(&mut self, expr: ExprId, type_expr: ExprId, name: IdentId) -> HTerm {
        let variant = self.arena[name].name.clone();
        let enum_info = match self.ctx.get_node_typeinfo(node_expr(expr)).map(|t| t.kind) {
            Some(TypeInfoKind::Enum(_, key)) => self.ctx.lookup_enum(&key),
            _ => None,
        }
        .or_else(|| {
            let type_name = self.type_expr_name(type_expr)?;
            self.ctx.lookup_enum_in(&type_name, self.module_path)
        });

        if let Some(info) = enum_info {
            let tag = i32::try_from(info.variant_index(&variant).unwrap_or(0)).unwrap_or(0);
            HTerm::Const(HConst::I32(tag))
        } else {
            self.error(
                PCode::P004,
                self.arena[expr].location,
                "type of this value cannot appear in a specification term; only bool, integer, \
                 and enum values can"
                    .to_string(),
            );
            zero_sentinel()
        }
    }

    /// An identifier, resolved through the environment. A universal slot becomes
    /// `T_local`; an existential variable becomes `T_lvar` at its level (finalized
    /// later); a pure `let` inlines its stored term.
    fn identifier(&mut self, expr: ExprId, ident_id: IdentId) -> HTerm {
        let name = &self.arena[ident_id].name;
        match self.env.get(name) {
            Some(Binding::Slot(n)) => HTerm::Local(*n),
            Some(Binding::Level(level)) => HTerm::LVar(*level),
            Some(Binding::Term(term)) => term.clone(),
            None => {
                let rendered = self
                    .ctx
                    .get_node_typeinfo(node_expr(expr))
                    .map_or_else(|| "this value".to_string(), |t| t.to_string());
                self.error(
                    PCode::P004,
                    self.arena[expr].location,
                    format!(
                        "type `{rendered}` cannot appear in a specification term; only bool, \
                         integer, and enum values can"
                    ),
                );
                zero_sentinel()
            }
        }
    }

    /// A binary expression as a term, mirroring `lower_binary_expression`: the
    /// number class and signedness come from the left operand, sub-word results
    /// are narrowed after the arithmetic and bitwise operators, and `**` has no
    /// encoding.
    ///
    /// `&&`/`||` are the two operators `lower_binary_expression` does not
    /// itself lower — it hands them to `lower_short_circuit_binary` and leaves
    /// `unreachable!` in their place. They leave here the same way, through
    /// [`Self::short_circuit`], before the narrowing tail.
    fn binary(
        &mut self,
        expr: ExprId,
        left: ExprId,
        right: ExprId,
        op: &OperatorKind,
        mode: Mode,
    ) -> HTerm {
        if matches!(op, OperatorKind::Pow) {
            self.error_no_encoding(self.arena[expr].location, "`**` (the power operator)");
            return zero_sentinel();
        }
        if matches!(op, OperatorKind::And | OperatorKind::Or) {
            return self.short_circuit(left, right, op, mode);
        }
        let (num_ty, unsigned) = self.operand_class(left);
        let l = self.term(left, mode);
        let r = self.term(right, mode);
        let term = match op {
            OperatorKind::Add => binop(num_ty, HBinop::Add, l, r),
            OperatorKind::Sub => binop(num_ty, HBinop::Sub, l, r),
            OperatorKind::Mul => binop(num_ty, HBinop::Mul, l, r),
            OperatorKind::Div => binop(
                num_ty,
                if unsigned { HBinop::DivU } else { HBinop::DivS },
                l,
                r,
            ),
            OperatorKind::Mod => binop(
                num_ty,
                if unsigned { HBinop::RemU } else { HBinop::RemS },
                l,
                r,
            ),
            OperatorKind::BitAnd => binop(num_ty, HBinop::And, l, r),
            OperatorKind::BitOr => binop(num_ty, HBinop::Or, l, r),
            OperatorKind::BitXor => binop(num_ty, HBinop::Xor, l, r),
            OperatorKind::Shl => binop(num_ty, HBinop::Shl, l, r),
            OperatorKind::Shr => binop(
                num_ty,
                if unsigned { HBinop::ShrU } else { HBinop::ShrS },
                l,
                r,
            ),
            OperatorKind::And | OperatorKind::Or => {
                unreachable!("`&&`/`||` handled above")
            }
            OperatorKind::Eq => relop(num_ty, HRelop::Eq, l, r),
            OperatorKind::Ne => relop(num_ty, HRelop::Ne, l, r),
            OperatorKind::Lt => relop(num_ty, signed_relop(unsigned, Lt), l, r),
            OperatorKind::Le => relop(num_ty, signed_relop(unsigned, Le), l, r),
            OperatorKind::Gt => relop(num_ty, signed_relop(unsigned, Gt), l, r),
            OperatorKind::Ge => relop(num_ty, signed_relop(unsigned, Ge), l, r),
            OperatorKind::Pow => unreachable!("`**` handled above"),
        };
        if narrows(op) {
            let left_kind = self.ctx.get_node_typeinfo(node_expr(left)).map(|t| t.kind);
            narrow(term, left_kind.as_ref())
        } else {
            term
        }
    }

    /// A term-position `&&`/`||`, mirroring `lower_short_circuit_binary`:
    /// `a && b` computes `if a != 0 then b else 0` and `a || b` computes
    /// `if a != 0 then 1 else b`, over canonical 0/1 truth values.
    ///
    /// The term language has no conditional and is strict in every operand, so
    /// the result is a fresh logical variable pinned by a two-armed constraint
    /// naming the same two cases. The constraints the right operand introduced
    /// ride in the arm that evaluates it: on the other arm the source never
    /// computes it, so demanding it there would refute an obligation the program
    /// satisfies.
    fn short_circuit(
        &mut self,
        left: ExprId,
        right: ExprId,
        op: &OperatorKind,
        mode: Mode,
    ) -> HTerm {
        let l = self.term(left, mode);
        let (r, right_defs) = self.capture_definitions(|s| s.term(right, mode));
        let taken = HAssert::nz(l.clone());
        let skipped = HAssert::eqz(l);
        match op {
            OperatorKind::And => self.bind_witness(|v| {
                HAssert::or(
                    HAssert::and(
                        taken,
                        HAssert::and(right_defs, HAssert::TermEq(v.clone(), r)),
                    ),
                    HAssert::and(
                        skipped,
                        HAssert::TermEq(v.clone(), HTerm::Const(HConst::I32(0))),
                    ),
                )
            }),
            OperatorKind::Or => self.bind_witness(|v| {
                HAssert::or(
                    HAssert::and(
                        taken,
                        HAssert::TermEq(v.clone(), HTerm::Const(HConst::I32(1))),
                    ),
                    HAssert::and(
                        skipped,
                        HAssert::and(right_defs, HAssert::TermEq(v.clone(), r)),
                    ),
                )
            }),
            _ => unreachable!("short_circuit only handles `&&` and `||`"),
        }
    }

    /// A prefix-unary expression as a term, mirroring code generation: negation
    /// is `0 - x` (narrowed), logical `!` is the term-level `i32.eqz`
    /// (`relop Eq 0`), bitwise `~` is `xor -1` (narrowed).
    fn unary(&mut self, expr: ExprId, inner: ExprId, op: &UnaryOperatorKind, mode: Mode) -> HTerm {
        let kind = self.ctx.get_node_typeinfo(node_expr(expr)).map(|t| t.kind);
        let is_i64 = matches!(
            kind,
            Some(TypeInfoKind::Number(NumberType::I64 | NumberType::U64))
        );
        let num_ty = if is_i64 { HNumType::I64 } else { HNumType::I32 };
        let inner_term = self.term(inner, mode);
        match op {
            UnaryOperatorKind::Neg => {
                let zero = if is_i64 {
                    HTerm::Const(HConst::I64(0))
                } else {
                    HTerm::Const(HConst::I32(0))
                };
                let sub = binop(num_ty, HBinop::Sub, zero, inner_term);
                if is_i64 {
                    sub
                } else {
                    narrow(sub, kind.as_ref())
                }
            }
            UnaryOperatorKind::Not => relop(
                HNumType::I32,
                HRelop::Eq,
                inner_term,
                HTerm::Const(HConst::I32(0)),
            ),
            UnaryOperatorKind::BitNot => {
                let minus_one = if is_i64 {
                    HTerm::Const(HConst::I64(-1))
                } else {
                    HTerm::Const(HConst::I32(-1))
                };
                let xor = binop(num_ty, HBinop::Xor, inner_term, minus_one);
                if is_i64 {
                    xor
                } else {
                    narrow(xor, kind.as_ref())
                }
            }
        }
    }

    /// A call in term position: a single scalar result becomes a `T_app` term,
    /// anything else is [`PCode::P005`].
    fn call_term(&mut self, call_expr: ExprId, mode: Mode) -> HTerm {
        let (function, args) = self.call_parts(call_expr);
        match self.resolve_callee(function) {
            Ok((key, def_id)) => match self.result_class(call_expr, def_id) {
                ResultClass::Scalar => {
                    let arg_terms = self.arg_terms(&args, mode);
                    HTerm::App(HFnRef(key.to_string()), arg_terms)
                }
                ResultClass::Void | ResultClass::Compound => {
                    self.error_call(function, "its result is not a single scalar");
                    zero_sentinel()
                }
            },
            Err(error) => {
                self.emit_callee_error(function, &error);
                zero_sentinel()
            }
        }
    }

    /// A bare statement call, realized as `HA_app_ok` at any result arity.
    fn app_ok(&mut self, call_expr: ExprId, mode: Mode) -> HAssert {
        let (function, args) = self.call_parts(call_expr);
        match self.resolve_callee(function) {
            Ok((key, _)) => {
                let arg_terms = self.arg_terms(&args, mode);
                HAssert::AppOk(HFnRef(key.to_string()), arg_terms)
            }
            Err(error) => {
                self.emit_callee_error(function, &error);
                HAssert::True
            }
        }
    }

    /// Translates a call's arguments, with `@` in argument position taking a
    /// fresh slot (universal) or an existential binder (existential).
    fn arg_terms(&mut self, args: &[(Option<IdentId>, ExprId)], mode: Mode) -> Vec<HTerm> {
        args.iter()
            .map(|(_, arg)| {
                if matches!(self.arena[*arg].kind, Expr::Uzumaki) {
                    self.uzumaki_argument(*arg, mode)
                } else {
                    self.term(*arg, mode)
                }
            })
            .collect()
    }

    /// A `@` in call-argument position: an anonymous universal slot, a pending
    /// existential binder to be wrapped around the enclosing statement, or —
    /// in a reachability body — the choice parameter the pre-scan planned for
    /// it. An anonymous slot has no declared type, so its guard width comes
    /// from the type recorded for the argument.
    ///
    /// Unlike a short-circuit witness, this binder carries no defining
    /// constraint: `@` *is* the prover's free choice, so pinning it to a value
    /// would be the opposite of what it means.
    ///
    /// A reachability body reads the raw choice parameter: code generation
    /// normalizes an anonymous narrow choice at its use site, not in the
    /// parameter itself, but the judgment quantifies whole choice vectors and
    /// every in-domain value is a fixed point of the normalization, so the raw
    /// and normalized readings coincide on exactly the vectors a proof would
    /// pick. In a `unique` body an anonymous choice is rejected outright
    /// ([`PCode::P012`]): it is excluded from the source-visible observation
    /// the judgment compares, so distinct choices nothing names would collapse
    /// into one observation — a silent weakening of uniqueness.
    fn uzumaki_argument(&mut self, arg: ExprId, mode: Mode) -> HTerm {
        match mode {
            Mode::Univ => {
                let slot = self.next_slot();
                let width = self.expr_class(arg);
                self.push_univ_guard(slot, width);
                HTerm::Local(slot)
            }
            Mode::Exist => self.bind_witness(|_| HAssert::True),
            Mode::Reach => {
                let (planned, unique) = {
                    let reach = self
                        .reach
                        .as_ref()
                        .expect("Mode::Reach requires a reachability context");
                    (reach.plan.by_expr.get(&arg).copied(), reach.unique)
                };
                // Unlike the `let` seam, this position performs no scalarity
                // pre-check of its own, so the plan lookup is the check: the
                // pre-scan plans every scalar `@`, and an unplanned one here
                // is at a non-scalar type.
                let Some(slot) = planned else {
                    self.emit_unplanned_reach_argument(arg);
                    return zero_sentinel();
                };
                if unique {
                    self.error(
                        PCode::P012,
                        self.arena[arg].location,
                        "anonymous `@` argument in a `unique` spec function has no \
                         source-visible face: `unique` compares source-visible exit states, and \
                         a choice nothing names cannot distinguish them — bind it first \
                         (`let c: i32 = @;`) so the choice participates in uniqueness"
                            .to_string(),
                    );
                    return zero_sentinel();
                }
                HTerm::Local(slot)
            }
        }
    }

    /// Resolves a call's callee to a `(FnKey, DefId)` for a module-defined,
    /// deterministic function, or the [`CalleeError`] the call site raises.
    ///
    /// Mirrors code generation's resolution: a bare same-file call (including a
    /// spec-sibling helper) is resolved spec-first then by the current file's
    /// free key; a cross-file item import, a `::`-qualified free function, and an
    /// associated function use the type-checker-recorded target; an instance
    /// method has no term encoding.
    fn resolve_callee(&self, function: ExprId) -> Result<(FnKey, DefId), CalleeError> {
        match &self.arena[function].kind {
            Expr::Identifier(ident_id) => {
                let name = self.arena[*ident_id].name.clone();
                if self.ctx.is_extern_function(&name) {
                    return Err(CalleeError::NotApplicable(
                        "external functions carry no verified body",
                    ));
                }
                // A cross-file item import (`use lib::arith::{add}; add()`)
                // resolves to its defining file via the recorded target.
                if let Some(target) = self.ctx.call_target(function)
                    && target.receiver_struct.is_none()
                    && target.module_path != self.module_path
                {
                    return self.validate_defined(FnKey::free_in(
                        target.module_path.clone(),
                        target.name.clone(),
                    ));
                }
                // Same-file: the spec-mangled sibling key first, then the file's
                // free key — exactly `resolve_free_callee_idx`.
                let spec_key =
                    FnKey::spec_free_folded(self.module_path, self.spec_name, name.clone());
                if let Some(def_id) = self.callee.get(&spec_key) {
                    // An `exists`/`unique`-bodied sibling is carved out *before*
                    // the general non-deterministic-body arm below: it would
                    // otherwise be swallowed by that arm's `P005`, whose
                    // "specification term" wording is wrong for a void callee
                    // and whose remedy does not fit — this callee is the
                    // subject of its own reachability judgment, not a body
                    // that merely lacks executable meaning.
                    if let Some(kind) = self.reachability_kind(def_id) {
                        return Err(CalleeError::ReachabilitySpec { kind });
                    }
                    return self.validate_body(spec_key, def_id);
                }
                let free_key = FnKey::free_in(self.module_path.to_vec(), name);
                if let Some(def_id) = self.callee.get(&free_key) {
                    return self.validate_body(free_key, def_id);
                }
                Err(CalleeError::NotApplicable(
                    "external functions carry no verified body",
                ))
            }
            // `Point::new()` / `math::arith::add()`: the recorded target names the
            // struct's or free function's defining file.
            Expr::TypeMemberAccess { .. } => {
                let Some(target) = self.ctx.call_target(function) else {
                    return Err(CalleeError::NotApplicable(
                        "it does not resolve to a module-defined function",
                    ));
                };
                let key = match &target.receiver_struct {
                    Some(struct_name) => FnKey::method_in(
                        target.module_path.clone(),
                        struct_name.clone(),
                        target.name.clone(),
                    ),
                    None => FnKey::free_in(target.module_path.clone(), target.name.clone()),
                };
                self.validate_defined(key)
            }
            Expr::MemberAccess { .. } => Err(CalleeError::NotApplicable(
                "instance methods operate on memory",
            )),
            _ => Err(CalleeError::NotApplicable(
                "it does not resolve to a module-defined function",
            )),
        }
    }

    /// Confirms a `FnKey` names a module-defined function (not an import) and
    /// validates its body.
    fn validate_defined(&self, key: FnKey) -> Result<(FnKey, DefId), CalleeError> {
        match self.callee.get(&key) {
            Some(def_id) => self.validate_body(key, def_id),
            None => Err(CalleeError::NotApplicable(
                "external functions carry no verified body",
            )),
        }
    }

    /// Rejects a callee whose body contains non-deterministic constructs — it can
    /// carry no realized claim.
    fn validate_body(&self, key: FnKey, def_id: DefId) -> Result<(FnKey, DefId), CalleeError> {
        if self.arena.def_is_non_det(def_id) {
            return Err(CalleeError::NotApplicable(
                "its body is non-deterministic and has no executable meaning",
            ));
        }
        Ok((key, def_id))
    }

    /// The quantifier word of an `exists`/`unique`-quantified function body,
    /// or `None` for any other body kind.
    fn reachability_kind(&self, def_id: DefId) -> Option<&'static str> {
        match &self.arena[def_id].kind {
            Def::Function { body, .. } => match self.arena[*body].block_kind {
                BlockKind::Exists => Some("exists"),
                BlockKind::Unique => Some("unique"),
                _ => None,
            },
            _ => None,
        }
    }

    /// Classifies a call's result. The recorded result type is preferred; a
    /// missing one falls back to the callee's declared return type.
    fn result_class(&self, call_expr: ExprId, def_id: DefId) -> ResultClass {
        if let Some(kind) = self
            .ctx
            .get_node_typeinfo(node_expr(call_expr))
            .map(|t| t.kind)
        {
            return self.classify_type_kind(&kind);
        }
        match &self.arena[def_id].kind {
            Def::Function { returns, .. } => match returns {
                None => ResultClass::Void,
                Some(ty) => self.classify_type_kind(&TypeInfo::from_type_id(self.arena, *ty).kind),
            },
            _ => ResultClass::Compound,
        }
    }

    fn classify_type_kind(&self, kind: &TypeInfoKind) -> ResultClass {
        match kind {
            TypeInfoKind::Bool | TypeInfoKind::Number(_) | TypeInfoKind::Enum(_, _) => {
                ResultClass::Scalar
            }
            TypeInfoKind::Unit => ResultClass::Void,
            TypeInfoKind::Custom(name) => {
                if self.ctx.lookup_enum_in(name, self.module_path).is_some() {
                    ResultClass::Scalar
                } else {
                    ResultClass::Compound
                }
            }
            _ => ResultClass::Compound,
        }
    }

    // ----- helpers -------------------------------------------------------

    /// Splits a `FunctionCall` into its callee expression and owned argument list.
    fn call_parts(&self, call_expr: ExprId) -> (ExprId, Vec<(Option<IdentId>, ExprId)>) {
        match &self.arena[call_expr].kind {
            Expr::FunctionCall { function, args, .. } => (*function, args.clone()),
            _ => unreachable!("call_parts called on a non-call expression"),
        }
    }

    /// Runs `f` and wraps its atom in one `HA_ex` per binder `f` introduced —
    /// a call-argument `@` on a witness path, or a short-circuit witness in
    /// either mode. The atom is built while the binders are still pending, so
    /// it reads them at the levels they were allocated at and needs no depth
    /// adjustment.
    fn eval_atom<F>(&mut self, f: F) -> HAssert
    where
        F: FnOnce(&mut Self) -> HAssert,
    {
        let base = self.pending.len();
        let atom = f(self);
        let group = self.split_pending(base);
        wrap_existentials(atom, group)
    }

    /// Translates a block's statements as a fresh environment scope, so a
    /// branch-local `let` does not leak to the rest of the enclosing block. The
    /// pending guards travel with the environment: a branch-local slot's guard
    /// stays inside the branch, and a guard pending outside cannot be drained
    /// into the narrower scope.
    fn scoped_block(&mut self, block_id: BlockId, mode: Mode) -> HAssert {
        let stmts = self.arena[block_id].stmts.clone();
        let saved_env = self.env.clone();
        let saved_guards = std::mem::take(&mut self.univ_guards);
        let result = self.t_stmts(&stmts, mode);
        self.env = saved_env;
        self.univ_guards = saved_guards;
        result
    }

    /// The translation mode of an `if` branch: existential when the branch block
    /// is an `exists` block, otherwise the enclosing mode.
    fn branch_mode(&self, block_id: BlockId, outer: Mode) -> Mode {
        match self.arena[block_id].block_kind {
            BlockKind::Exists => Mode::Exist,
            _ => outer,
        }
    }

    /// Records [`PCode::P007`] when an existential `if` branch is a `forall`
    /// block, which needs a `Hall` over logical variables (deferred).
    fn check_branch_forall(&mut self, block_id: BlockId) {
        if self.arena[block_id].block_kind == BlockKind::Forall {
            self.error(
                PCode::P007,
                self.arena[block_id].location,
                "a `forall` block inside an `exists` block is not yet supported in assertion \
                 emission"
                    .to_string(),
            );
        }
    }

    /// The number class and signedness of an operand, read from its type exactly
    /// as `lower_binary_expression` reads the left operand's.
    fn operand_class(&self, expr: ExprId) -> (HNumType, bool) {
        let kind = self.ctx.get_node_typeinfo(node_expr(expr)).map(|t| t.kind);
        let unsigned = matches!(
            kind,
            Some(TypeInfoKind::Number(
                NumberType::U8 | NumberType::U16 | NumberType::U32 | NumberType::U64
            ))
        );
        (num_class(kind.as_ref()), unsigned)
    }

    /// The number class of an expression, defaulting to i32 with the same
    /// latitude [`Self::operand_class`] takes when no type was recorded.
    fn expr_class(&self, expr: ExprId) -> HNumType {
        num_class(
            self.ctx
                .get_node_typeinfo(node_expr(expr))
                .map(|t| t.kind)
                .as_ref(),
        )
    }

    /// The number class of a declared type.
    fn declared_class(&self, ty: TypeId) -> HNumType {
        num_class(Some(&TypeInfo::from_type_id(self.arena, ty).kind))
    }

    /// Whether a declared type is a scalar the term language can represent (a
    /// bool, an integer, or an enum).
    fn type_is_scalar(&self, ty: TypeId) -> bool {
        match TypeInfo::from_type_id(self.arena, ty).kind {
            TypeInfoKind::Bool | TypeInfoKind::Number(_) => true,
            TypeInfoKind::Custom(name) => {
                self.ctx.lookup_enum_in(&name, self.module_path).is_some()
            }
            TypeInfoKind::Qualified(path) => {
                let segments: Vec<String> = path.split("::").map(str::to_string).collect();
                self.ctx.qualified_path_is_enum(&segments, self.module_path)
            }
            _ => false,
        }
    }

    /// Renders the diagnostic message for a non-scalar type in term/parameter
    /// position.
    fn non_scalar_message(&self, ty: TypeId) -> String {
        format!(
            "type `{}` cannot appear in a specification term; only bool, integer, and enum \
             values can",
            TypeInfo::from_type_id(self.arena, ty)
        )
    }

    /// Emits the right diagnostic for a `@` at a non-scalar type: [`PCode::P008`]
    /// for a compound (array/struct) type, [`PCode::P004`] otherwise. The
    /// `P008` wording is mode-aware — in a reachability body the reason a
    /// compound `@` is impossible is different (a choice arrives as one scalar
    /// parameter), and the universal wording stays byte-identical.
    fn emit_non_scalar_uzumaki(&mut self, ty: TypeId, location: Location, mode: Mode) {
        let type_info = TypeInfo::from_type_id(self.arena, ty);
        let compound = match &type_info.kind {
            TypeInfoKind::Array(_, _) => true,
            TypeInfoKind::Custom(name) => {
                self.ctx.lookup_struct_in(name, self.module_path).is_some()
            }
            _ => false,
        };
        if compound {
            let message = if mode == Mode::Reach {
                format!(
                    "uzumaki (@) over compound type `{type_info}` cannot be a reachability \
                     choice: a choice arrives as one scalar WASM parameter, and a value of type \
                     `{type_info}` lives in linear memory; quantify its scalar components \
                     individually"
                )
            } else {
                format!(
                    "uzumaki (@) over compound type `{type_info}` has no assertion encoding; \
                     quantify scalar components individually"
                )
            };
            self.error(PCode::P008, location, message);
        } else {
            self.error(
                PCode::P004,
                location,
                format!(
                    "type `{type_info}` cannot appear in a specification term; only bool, \
                     integer, and enum values can"
                ),
            );
        }
    }

    /// The `@` the pre-scan did not plan, reached in call-argument position of
    /// a reachability body. The pre-scan plans every scalar `@`, so an
    /// unplanned one is at a non-scalar type: a compound gets the reachability
    /// [`PCode::P008`] wording, anything else the standard non-scalar
    /// [`PCode::P004`] text.
    fn emit_unplanned_reach_argument(&mut self, arg: ExprId) {
        let location = self.arena[arg].location;
        let kind = self.ctx.get_node_typeinfo(node_expr(arg)).map(|t| t.kind);
        match kind {
            Some(
                kind @ (TypeInfoKind::Array(_, _)
                | TypeInfoKind::Struct(_, _)
                | TypeInfoKind::Custom(_)),
            ) => {
                self.error(
                    PCode::P008,
                    location,
                    format!(
                        "uzumaki (@) over compound type `{kind}` cannot be a reachability \
                         choice: a choice arrives as one scalar WASM parameter, and a value of \
                         type `{kind}` lives in linear memory; quantify its scalar components \
                         individually"
                    ),
                );
            }
            Some(kind) => {
                self.error(
                    PCode::P004,
                    location,
                    format!(
                        "type `{kind}` cannot appear in a specification term; only bool, \
                         integer, and enum values can"
                    ),
                );
            }
            None => {
                self.error(
                    PCode::P004,
                    location,
                    "type of this value cannot appear in a specification term; only bool, \
                     integer, and enum values can"
                        .to_string(),
                );
            }
        }
    }

    /// The appended choice-parameter index of a scalar `@` in a reachability
    /// body, from the shared pre-scan plan.
    ///
    /// A miss is a compiler bug, never a program error, and there is no honest
    /// slot to fall back on — inventing one would emit a payload slot that is
    /// not the choice parameter, a silently wrong obligation. Three sites
    /// classify "is this `@` scalar" today (`reach.rs`'s `plan_choice` and the
    /// compiler's `Expr::Uzumaki` arm from the recorded type; this
    /// translator's `type_is_scalar` from the declared type, with extra arms
    /// for enum-resolving `Custom`/`Qualified` names), so a scalar this pass
    /// sees that the plan lacks means those classifiers diverged.
    fn choice_slot(&self, expr: ExprId) -> u32 {
        let reach = self
            .reach
            .as_ref()
            .expect("Mode::Reach requires a reachability context");
        reach.plan.by_expr.get(&expr).copied().unwrap_or_else(|| {
            panic!(
                "scalar `@` reached reachability translation without a planned choice \
                 parameter — the pre-scan and the translator disagree on scalar \
                 classification, and emitting any other slot would misalign the payload \
                 with the compiled frame"
            )
        })
    }

    /// The bare type name of a type expression, for enum resolution fallback.
    fn type_expr_name(&self, type_expr: ExprId) -> Option<String> {
        match &self.arena[type_expr].kind {
            Expr::Identifier(id) => Some(self.arena[*id].name.clone()),
            Expr::Type(ty_id) => match &self.arena[*ty_id].kind {
                inference_ast::nodes::TypeNode::Custom(id) => Some(self.arena[*id].name.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    fn next_slot(&mut self) -> u32 {
        let slot = self.slots;
        self.slots += 1;
        slot
    }

    /// The number of binders pending at this point, as a level offset.
    fn pending_len(&self) -> u32 {
        level_count(self.pending.len())
    }

    /// Allocates the next pending binder and pins it with the constraint
    /// `define` builds over its own term.
    ///
    /// The binder takes the level just past every binder already in scope or
    /// pending, so it may be defined in terms of any of them but none of them in
    /// terms of it. A binder nothing pins passes `HAssert::True`.
    fn bind_witness<F>(&mut self, define: F) -> HTerm
    where
        F: FnOnce(&HTerm) -> HAssert,
    {
        let witness = HTerm::LVar(self.depth + self.pending_len());
        let definition = define(&witness);
        self.pending.push(definition);
        witness
    }

    /// Removes the binders allocated since `base` as one group.
    fn split_pending(&mut self, base: usize) -> PendingGroup {
        PendingGroup {
            base_level: self.depth + level_count(base),
            defs: self.pending.split_off(base),
        }
    }

    /// Runs `f` and takes away the *definitions* of every binder it introduced,
    /// returning them conjoined in allocation order.
    ///
    /// This is how a constraint reaches the arm that evaluates it. Only the
    /// definitions move: each binder stays pending with a `⊤` definition, so
    /// the levels allocated inside `f` remain valid and the `HA_ex`s still hoist
    /// to the enclosing atom. A binder left unconstrained that way is exactly
    /// right — on the arm the source skips, nothing reads it.
    fn capture_definitions<T, F>(&mut self, f: F) -> (T, HAssert)
    where
        F: FnOnce(&mut Self) -> T,
    {
        let base = self.pending.len();
        let value = f(self);
        let taken: Vec<HAssert> = self.pending[base..]
            .iter_mut()
            .map(|definition| std::mem::replace(definition, HAssert::True))
            .collect();
        (value, conjoin(taken))
    }

    fn error(&mut self, code: PCode, location: Location, message: String) {
        self.diags.push(HassertDiagnostic::new(
            code,
            location,
            self.module_path.to_vec(),
            message,
        ));
    }

    fn error_no_encoding(&mut self, location: Location, construct: &str) {
        self.error(
            PCode::P002,
            location,
            format!(
                "{construct} has no encoding in the verification assertion language; remove it \
                 from the spec body or move the logic into an executable helper function"
            ),
        );
    }

    fn error_call(&mut self, function: ExprId, reason: &str) {
        let name = self.call_display_name(function);
        self.error(
            PCode::P005,
            self.arena[function].location,
            format!("call to `{name}` cannot be used in a specification term ({reason})"),
        );
    }

    /// Raises the diagnostic a [`CalleeError`] stands for: [`PCode::P005`]
    /// with its reason, or [`PCode::P011`] for an `exists`/`unique`-quantified
    /// spec callee.
    fn emit_callee_error(&mut self, function: ExprId, error: &CalleeError) {
        match error {
            CalleeError::NotApplicable(reason) => self.error_call(function, reason),
            CalleeError::ReachabilitySpec { kind } => {
                let name = self.call_display_name(function);
                self.error(
                    PCode::P011,
                    self.arena[function].location,
                    format!(
                        "call to `{name}` is not allowed: `{name}` is an `{kind}`-quantified \
                         spec function, and its obligation is a claim about running its own \
                         body with its own choices — there is no predicate to apply here; state \
                         the property you want directly in this body, or move the shared part \
                         into an ordinary function both spec functions can call"
                    ),
                );
            }
        }
    }

    /// A readable callee name for a diagnostic (bare name, or `Type::method`).
    fn call_display_name(&self, function: ExprId) -> String {
        match &self.arena[function].kind {
            Expr::Identifier(id) => self.arena[*id].name.clone(),
            Expr::MemberAccess { name, .. } | Expr::TypeMemberAccess { name, .. } => {
                self.arena[*name].name.clone()
            }
            _ => "<anonymous>".to_string(),
        }
    }
}

// ----- free helpers -----------------------------------------------------

/// A distinct signed-relop selector, so [`signed_relop`] reads clearly.
#[derive(Clone, Copy)]
enum Ordered {
    Lt,
    Le,
    Gt,
    Ge,
}
use Ordered::{Ge, Gt, Le, Lt};

fn signed_relop(unsigned: bool, op: Ordered) -> HRelop {
    match (op, unsigned) {
        (Lt, false) => HRelop::LtS,
        (Lt, true) => HRelop::LtU,
        (Le, false) => HRelop::LeS,
        (Le, true) => HRelop::LeU,
        (Gt, false) => HRelop::GtS,
        (Gt, true) => HRelop::GtU,
        (Ge, false) => HRelop::GeS,
        (Ge, true) => HRelop::GeU,
    }
}

fn binop(ty: HNumType, op: HBinop, l: HTerm, r: HTerm) -> HTerm {
    HTerm::Binop(ty, op, Box::new(l), Box::new(r))
}

fn relop(ty: HNumType, op: HRelop, l: HTerm, r: HTerm) -> HTerm {
    HTerm::Relop(ty, op, Box::new(l), Box::new(r))
}

fn zero_sentinel() -> HTerm {
    HTerm::Const(HConst::I32(0))
}

/// The constant a number literal denotes at the width recorded for it, chosen
/// exactly as code generation lowers the same literal: every width below 64 bits
/// rides in an `i32` constant, and an unsigned value is reinterpreted as the
/// signed constant with the same bit pattern.
fn number_const(width: NumberType, value: &str) -> HConst {
    match width {
        // `i8` and `i16` are read at `i32` width, the same latitude code
        // generation takes: whether the value fits the narrower type is the
        // literal-range analysis rule's call, and it has already made it.
        NumberType::I8 | NumberType::I16 | NumberType::I32 => HConst::I32(parse_at(value, width)),
        NumberType::U8 => HConst::I32(i32::from(parse_at::<u8>(value, width))),
        NumberType::U16 => HConst::I32(i32::from(parse_at::<u16>(value, width))),
        NumberType::U32 => HConst::I32(parse_at::<u32>(value, width).cast_signed()),
        NumberType::I64 => HConst::I64(parse_at(value, width)),
        NumberType::U64 => HConst::I64(parse_at::<u64>(value, width).cast_signed()),
    }
}

/// A literal's text read at the width recorded for it. A value that does not fit
/// that width has already been rejected by the literal-range analysis rule, so a
/// parse failure here is a compiler bug — silently reading `0` instead would make
/// the obligation constrain a constant the program never produces.
fn parse_at<T: std::str::FromStr>(value: &str, width: NumberType) -> T {
    value.parse().unwrap_or_else(|_| {
        panic!(
            "literal `{value}` does not parse at `{}`, the type recorded for it",
            width.as_str()
        )
    })
}

/// The `emit_sub_i32_narrowing` mirror: truncates an i32 result to a sub-word
/// width. Signed widths sign-extend by `shl`/`shr_s`; unsigned widths mask.
fn narrow(term: HTerm, kind: Option<&TypeInfoKind>) -> HTerm {
    match kind {
        Some(TypeInfoKind::Number(NumberType::I8)) => sign_extend(term, 24),
        Some(TypeInfoKind::Number(NumberType::I16)) => sign_extend(term, 16),
        Some(TypeInfoKind::Number(NumberType::U8)) => mask(term, 0xFF),
        Some(TypeInfoKind::Number(NumberType::U16)) => mask(term, 0xFFFF),
        _ => term,
    }
}

fn sign_extend(term: HTerm, shift: i32) -> HTerm {
    binop(
        HNumType::I32,
        HBinop::ShrS,
        binop(
            HNumType::I32,
            HBinop::Shl,
            term,
            HTerm::Const(HConst::I32(shift)),
        ),
        HTerm::Const(HConst::I32(shift)),
    )
}

fn mask(term: HTerm, bits: i32) -> HTerm {
    binop(
        HNumType::I32,
        HBinop::And,
        term,
        HTerm::Const(HConst::I32(bits)),
    )
}

/// Whether the operator narrows a sub-word result, matching the exclusion list
/// in `lower_binary_expression` (relations, `%`, `&&`/`||`, `>>` do not).
/// `&&`/`||` are excluded because they never reach the narrowing tail — they
/// leave [`SpecFnTranslator::binary`] as a witness before it.
fn narrows(op: &OperatorKind) -> bool {
    matches!(
        op,
        OperatorKind::Add
            | OperatorKind::Sub
            | OperatorKind::Mul
            | OperatorKind::Div
            | OperatorKind::BitAnd
            | OperatorKind::BitOr
            | OperatorKind::BitXor
            | OperatorKind::Shl
    )
}

/// The number class a scalar's values ride in: `i64` and `u64` at 64 bits,
/// every other scalar — bool, enums, and the sub-word integer widths — at 32.
fn num_class(kind: Option<&TypeInfoKind>) -> HNumType {
    if matches!(
        kind,
        Some(TypeInfoKind::Number(NumberType::I64 | NumberType::U64))
    ) {
        HNumType::I64
    } else {
        HNumType::I32
    }
}

/// The level offset a binder count contributes. A specification body cannot
/// approach `u32::MAX` binders — the arena would have run out of expressions
/// first — so the conversion is an invariant, not a case to handle.
fn level_count(count: usize) -> u32 {
    u32::try_from(count).expect("a specification body cannot introduce 2^32 binders")
}

/// Right-folds assertions into one conjunction, `⊤` for none:
/// `a₀ ∧ (a₁ ∧ (… ∧ aₙ))`.
fn conjoin(assertions: Vec<HAssert>) -> HAssert {
    assertions
        .into_iter()
        .rev()
        .fold(HAssert::True, |acc, assertion| HAssert::and(assertion, acc))
}

/// Wraps `body` in one `HA_ex` per entry of `defs`, whose binders occupy levels
/// `base_level ..` in allocation order. Folding innermost-first puts the
/// first-allocated binder outermost, so a later definition may name an earlier
/// binder: `∃v₀. (def₀ ∧ ∃v₁. (def₁ ∧ … ∧ body))`.
///
/// A binder whose variable does not occur in the accumulated body is emitted
/// *without* its definition. A definition pins a value, so keeping one for a
/// variable nothing reads would turn a specification that claims nothing into a
/// refutable claim — `let unused: bool = 10 / x == 0 || true;` alone must stay
/// `HA_true`. Only the definition is dropped, never the binder: dropping the
/// binder would shift the level of every binder allocated inside it. The
/// innermost-first order lets one dropped definition cascade outward, and
/// [`HAssert::ex`] collapses the resulting `∃x. ⊤` away.
fn wrap_existentials(body: HAssert, group: PendingGroup) -> HAssert {
    let PendingGroup { defs, base_level } = group;
    let mut level = base_level + level_count(defs.len());
    let mut acc = body;
    for definition in defs.into_iter().rev() {
        level -= 1;
        acc = if assert_mentions_level(&acc, level) {
            HAssert::ex(HAssert::and(definition, acc))
        } else {
            HAssert::ex(acc)
        };
    }
    acc
}

/// Whether `assertion` reads the logical variable bound at absolute `level`.
/// Levels are position-independent, so no shifting is needed under `HA_ex`.
fn assert_mentions_level(assertion: &HAssert, level: u32) -> bool {
    match assertion {
        HAssert::True | HAssert::False => false,
        HAssert::Not(inner) | HAssert::Ex(inner) => assert_mentions_level(inner, level),
        HAssert::And(l, r) | HAssert::Imp(l, r) | HAssert::Or(l, r) => {
            assert_mentions_level(l, level) || assert_mentions_level(r, level)
        }
        HAssert::TermEq(a, b) => term_mentions_level(a, level) || term_mentions_level(b, level),
        HAssert::HasType(t, _) | HAssert::Defined(t) => term_mentions_level(t, level),
        HAssert::AppOk(_, args) => args.iter().any(|t| term_mentions_level(t, level)),
    }
}

fn term_mentions_level(term: &HTerm, level: u32) -> bool {
    match term {
        HTerm::LVar(l) => *l == level,
        HTerm::Const(_) | HTerm::Local(_) => false,
        HTerm::App(_, args) => args.iter().any(|t| term_mentions_level(t, level)),
        HTerm::Binop(_, _, l, r) | HTerm::Relop(_, _, l, r) => {
            term_mentions_level(l, level) || term_mentions_level(r, level)
        }
    }
}

fn node_expr(expr: ExprId) -> inference_ast::ids::NodeId {
    inference_ast::ids::NodeId::Expr(expr)
}

/// Rewrites logical-variable levels to de Bruijn indices, descending under each
/// `HA_ex` binder.
fn lower_assert(assertion: &HAssert, depth: u32) -> HAssert {
    match assertion {
        HAssert::True => HAssert::True,
        HAssert::False => HAssert::False,
        HAssert::Not(inner) => HAssert::Not(Box::new(lower_assert(inner, depth))),
        HAssert::And(l, r) => HAssert::And(
            Box::new(lower_assert(l, depth)),
            Box::new(lower_assert(r, depth)),
        ),
        HAssert::Imp(l, r) => HAssert::Imp(
            Box::new(lower_assert(l, depth)),
            Box::new(lower_assert(r, depth)),
        ),
        HAssert::Or(l, r) => HAssert::Or(
            Box::new(lower_assert(l, depth)),
            Box::new(lower_assert(r, depth)),
        ),
        HAssert::Ex(body) => HAssert::Ex(Box::new(lower_assert(body, depth + 1))),
        HAssert::TermEq(a, b) => HAssert::TermEq(lower_term(a, depth), lower_term(b, depth)),
        HAssert::HasType(t, ty) => HAssert::HasType(lower_term(t, depth), *ty),
        HAssert::Defined(t) => HAssert::Defined(lower_term(t, depth)),
        HAssert::AppOk(f, args) => HAssert::AppOk(
            f.clone(),
            args.iter().map(|t| lower_term(t, depth)).collect(),
        ),
    }
}

fn lower_term(term: &HTerm, depth: u32) -> HTerm {
    match term {
        // An out-of-scope level is a bookkeeping bug in this pass, not a
        // program error, and there is no honest index to fall back on: the
        // subtraction would wrap to a `T_lvar` naming a variable that does not
        // exist, which the codec and the printer would both pass through.
        HTerm::LVar(level) => {
            assert!(
                *level < depth,
                "logical variable level {level} is not bound at depth {depth}"
            );
            HTerm::LVar(depth - 1 - level)
        }
        HTerm::Const(_) | HTerm::Local(_) => term.clone(),
        HTerm::App(f, args) => HTerm::App(
            f.clone(),
            args.iter().map(|t| lower_term(t, depth)).collect(),
        ),
        HTerm::Binop(ty, op, l, r) => binop(*ty, *op, lower_term(l, depth), lower_term(r, depth)),
        HTerm::Relop(ty, op, l, r) => relop(*ty, *op, lower_term(l, depth), lower_term(r, depth)),
    }
}

/// The literal contract at its edges, none of which a translated program can
/// reach: a well-typed program has no untyped literal, no literal whose value
/// overflows its own type, and no literal at a non-numeric type. Each state is
/// provoked directly here. The two that are compiler-bug invariants are caught
/// with `catch_unwind` rather than declared with `#[should_panic]`, and the panic
/// hook is left alone so no process-global state is touched.
#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use inference_ast::nodes::ExprData;
    use inference_type_checker::TypeCheckerBuilder;

    use super::*;
    use crate::EmittableFunctions;

    /// The text of a caught panic. Both `expect` and a formatted `panic!` deliver
    /// a `String`; the other arms keep an unexpected payload from reading as an
    /// empty message and failing an assertion for the wrong reason.
    fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
        if let Some(text) = payload.downcast_ref::<String>() {
            return text.clone();
        }
        if let Some(text) = payload.downcast_ref::<&str>() {
            return (*text).to_string();
        }
        "<non-string panic payload>".to_string()
    }

    /// A one-spec program plus a number-literal expression belonging to no
    /// definition. The type checker walks definitions, never the arena, so the
    /// orphan is a literal the surrounding context has no type for — the state a
    /// type-checked program cannot produce.
    fn context_with_orphan_literal() -> (TypedContext, ExprId) {
        let parsed = inference_parser::parse("spec S { fn f() forall { assert(1 > 0); } }");
        assert!(
            parsed.errors.is_empty(),
            "parse errors: {:?}",
            parsed.errors
        );
        let mut arena = parsed.arena;
        let orphan = arena.exprs.alloc(ExprData {
            location: Location::default(),
            kind: Expr::NumberLiteral {
                value: "7".to_string(),
            },
        });
        let ctx = TypeCheckerBuilder::build_typed_context(arena)
            .expect("type checking should succeed")
            .typed_context();
        (ctx, orphan)
    }

    /// A literal with no recorded type is a typing gap in an earlier phase, not a
    /// program error: denoting it `i32` would bake a constant into a proof
    /// obligation that the compiled program never computes.
    #[test]
    fn literal_without_a_recorded_type_aborts_translation() {
        let (ctx, orphan) = context_with_orphan_literal();
        assert!(
            ctx.get_node_typeinfo(node_expr(orphan)).is_none(),
            "an expression reachable from no definition must stay untyped, or this test \
             exercises nothing"
        );

        let buckets = EmittableFunctions::default();
        let callee = CalleeIndex::build(ctx.arena(), &buckets);
        let mut translator = SpecFnTranslator::new(&ctx, &[], "S", &callee);
        let unwound = catch_unwind(AssertUnwindSafe(|| translator.number_literal(orphan, "7")));
        let payload = unwound.expect_err("an untyped literal must abort translation");
        let text = panic_text(payload.as_ref());
        assert!(
            text.contains("without a recorded type"),
            "the abort must name the missing type, got: {text}"
        );
    }

    /// The other side of that contract: a literal whose recorded type is not a
    /// number is a *program* error, so it keeps its `P004` diagnostic and a zero
    /// sentinel instead of aborting. The type checker rejects such a program long
    /// before translation, so the type is stamped onto the orphan directly.
    #[test]
    fn literal_at_a_non_numeric_type_stays_a_diagnostic() {
        let (mut ctx, orphan) = context_with_orphan_literal();
        ctx.register_test_node_type(node_expr(orphan), TypeInfo::boolean());

        let buckets = EmittableFunctions::default();
        let callee = CalleeIndex::build(ctx.arena(), &buckets);
        let mut translator = SpecFnTranslator::new(&ctx, &[], "S", &callee);
        assert_eq!(
            translator.number_literal(orphan, "7"),
            zero_sentinel(),
            "a non-numeric literal contributes the zero sentinel, not a constant"
        );
        let diagnostics = translator.take_diagnostics();
        assert_eq!(
            diagnostics.len(),
            1,
            "exactly one diagnostic, got: {diagnostics:?}"
        );
        let rendered = diagnostics[0].to_string();
        assert!(
            rendered.contains("error[P004]")
                && rendered.contains("cannot appear in a specification term"),
            "the non-numeric literal must keep its P004 diagnostic, got: {rendered}"
        );
    }

    /// `i8` and `i16` are read at `i32` width, so a value that overflows the
    /// narrower type yields its constant here rather than aborting — rejecting it
    /// is the literal-range rule's job, and code generation's own literal table
    /// takes the same latitude. Pinned so the two tables cannot drift apart
    /// unnoticed.
    #[test]
    fn sub_word_signed_widths_are_read_at_i32_width() {
        assert_eq!(number_const(NumberType::I8, "200"), HConst::I32(200));
        assert_eq!(number_const(NumberType::I16, "40000"), HConst::I32(40000));
    }

    /// Each width's parse is an invariant too: the literal-range rule accepted
    /// the value at this width, so a failure to parse it is a compiler bug and
    /// must not silently denote zero. One case per distinct arm of the table.
    #[test]
    fn value_that_overflows_its_recorded_width_aborts_translation() {
        for (width, value) in [
            (NumberType::I32, "2147483648"),
            (NumberType::U8, "300"),
            (NumberType::U16, "70000"),
            (NumberType::U32, "5000000000"),
            (NumberType::I64, "9223372036854775808"),
            (NumberType::U64, "-1"),
        ] {
            match catch_unwind(|| number_const(width, value)) {
                Err(payload) => {
                    let text = panic_text(payload.as_ref());
                    assert!(
                        text.contains(value) && text.contains(width.as_str()),
                        "the abort must name the literal and the width recorded for it, got: \
                         {text}"
                    );
                }
                Ok(constant) => panic!(
                    "`{value}` at `{}` must abort, got {constant:?}",
                    width.as_str()
                ),
            }
        }
    }
}
