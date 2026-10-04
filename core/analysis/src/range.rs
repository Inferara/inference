//! Value-range analysis over a single function body.
//!
//! The analysis computes, for every integer local at every point of a body, a
//! closed interval its value is guaranteed to lie in, and reports two facts
//! the rules read off it:
//!
//! - for every array access whose index is not a bare literal, the interval
//!   the index can take on any run that reaches the access (A056), and
//! - every `assert` whose condition is false on every run that reaches it
//!   (A057).
//!
//! ## Why no control-flow graph
//!
//! Inference's control flow is structured all the way down: `if`/`else`,
//! `loop [cond]`, `break` out of the innermost loop, and `return`, which A003
//! keeps out of loop bodies. There is no `continue` and no `goto`, so the
//! abstract interpreter follows the statement tree directly — an `if` joins
//! its two arms, a loop iterates to a fixpoint at its head, a `break` carries
//! its state to the loop's exit, and a `return` ends its path.
//!
//! ## What a state is
//!
//! A map from local name to the interval that local's value lies in, or no map
//! at all when the point is unreachable. A name with no entry can hold any
//! value of its type, which is what a parameter is at entry and what `@` is.
//! Names identify bindings soundly within one body: the type checker rejects a
//! local that shadows another binding, and a declaration in a sibling block
//! simply rebinds the entry.
//!
//! Only a direct assignment `x = …` can change a scalar local. The language
//! has value semantics — no references, pointers, or globals — so a call never
//! writes a caller's scalar, and an assignment through a field or an element
//! (`s.f = …`, `a[j] = …`) changes no scalar local either. That is what makes
//! "no reassignment between the guard and the use" a consequence of the
//! transfer functions rather than a separate check.
//!
//! ## Where facts come from
//!
//! - Initializers and assignments, evaluated with interval arithmetic that
//!   follows the operator semantics code generation emits: a checked `+`, `-`,
//!   `*` or unary `-` traps on a result its type cannot hold, so every run
//!   that continues has a result inside the type; a `wrapping(...)` one that
//!   can leave the type can be any value of it.
//! - The conditions of `if`, of `loop`, and the left operand of a
//!   short-circuiting `&&`/`||`, which narrow the operands they compare on each
//!   branch.
//!
//! An `assert` is deliberately not a source of facts. It establishes its
//! condition only by halting the program when the condition is false, and
//! the rules built on this analysis exist to make an out-of-range index a path
//! the program handles, not a halt.
//!
//! Everything the analysis cannot reason about — a call's result, a struct
//! field, an array element, a shift of a possibly negative value — is taken to
//! be any value of its type. The analysis over-approximates by construction, so
//! an interval it reports contains every value a run can produce there.

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, ExprId, NodeId, StmtId};
use inference_ast::nodes::{ArithMode, Def, Expr, OperatorKind, Stmt, UnaryOperatorKind};
use inference_type_checker::module_consts::ConstValue;
use inference_type_checker::type_info::{NumberType, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::walker;

/// How many times a loop head is widened before the analysis gives up on the
/// loop's precision and forgets every local the body writes. Widening sends a
/// bound straight to its type's limit, so a loop needs at most two widenings
/// per local it writes; the cap only guards against a state that keeps
/// changing for a reason this bound does not anticipate.
const MAX_LOOP_ITERATIONS: u32 = 64;

/// How many statements one body may execute, across every pass of every loop,
/// before loops stop being iterated. Re-analyzing a nested loop for each pass
/// of the one enclosing it multiplies with the nesting depth, and past this
/// budget each remaining loop is analyzed in a single pass instead.
const STATEMENT_BUDGET: u32 = 100_000;

/// A closed interval of integers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Interval {
    pub(crate) lo: i128,
    pub(crate) hi: i128,
}

impl Interval {
    fn exact(value: i128) -> Self {
        Self {
            lo: value,
            hi: value,
        }
    }

    /// Every value `number` can hold.
    pub(crate) fn of_type(number: NumberType) -> Self {
        let range = number.range();
        Self {
            lo: *range.start(),
            hi: *range.end(),
        }
    }

    fn hull(self, other: Self) -> Self {
        Self {
            lo: self.lo.min(other.lo),
            hi: self.hi.max(other.hi),
        }
    }

    /// The values in both, or `None` when they share none.
    fn meet(self, other: Self) -> Option<Self> {
        let lo = self.lo.max(other.lo);
        let hi = self.hi.min(other.hi);
        (lo <= hi).then_some(Self { lo, hi })
    }

    fn within(self, other: Self) -> bool {
        other.lo <= self.lo && self.hi <= other.hi
    }

    fn singleton(self) -> Option<i128> {
        (self.lo == self.hi).then_some(self.lo)
    }

    /// `self` shifted by `offset` in both bounds.
    fn offset(self, offset: i128) -> Self {
        Self {
            lo: self.lo + offset,
            hi: self.hi + offset,
        }
    }

    /// The interval of `value - x` for every `x` in `self`.
    fn subtracted_from(self, value: i128) -> Self {
        Self {
            lo: value - self.hi,
            hi: value - self.lo,
        }
    }
}

/// What the analysis established about one array access's index.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum IndexReach {
    /// No run reaches the access.
    Unreachable,
    /// Every run that reaches the access indexes with a value in this interval.
    Within(Interval),
}

impl IndexReach {
    fn join(self, other: Self) -> Self {
        match (self, other) {
            (IndexReach::Unreachable, reach) | (reach, IndexReach::Unreachable) => reach,
            (IndexReach::Within(a), IndexReach::Within(b)) => IndexReach::Within(a.hull(b)),
        }
    }
}

/// The facts [`analyze_body`] establishes about one body.
pub(crate) struct BodyRanges {
    /// Every array access whose index is not a bare number literal, keyed by
    /// the access expression. An access the analysis never met is absent,
    /// which a caller treats as unproven.
    pub(crate) indices: FxHashMap<ExprId, IndexReach>,
    /// Every `assert` statement whose condition is false on every run that
    /// reaches it, in the order the body writes them.
    pub(crate) failing_asserts: Vec<StmtId>,
}

/// Runs the analysis over the body `body` and returns what it established.
pub(crate) fn analyze_body(ctx: &TypedContext, body: BlockId) -> BodyRanges {
    let mut interpreter = Interpreter {
        ctx,
        arena: ctx.arena(),
        recording: true,
        budget: STATEMENT_BUDGET,
        breaks: Vec::new(),
        indices: FxHashMap::default(),
        failing_asserts: Vec::new(),
    };
    interpreter.exec_block(Some(Env::default()), body);
    BodyRanges {
        indices: interpreter.indices,
        failing_asserts: interpreter.failing_asserts,
    }
}

/// An integer local's type and the interval its value lies in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Tracked {
    number: NumberType,
    range: Interval,
}

/// The locals whose value the analysis knows something about. A local with no
/// entry can hold any value of its type.
type Env = FxHashMap<String, Tracked>;

/// A program point's state: `None` when no run reaches it.
type State = Option<Env>;

/// The least state both `a` and `b` are contained in.
fn join(a: State, b: State) -> State {
    match (a, b) {
        (None, state) | (state, None) => state,
        (Some(a), Some(b)) => Some(join_env(&a, &b)),
    }
}

/// A name only one side knows about can hold any value on the other, so it is
/// dropped rather than kept at the side's interval.
fn join_env(a: &Env, b: &Env) -> Env {
    a.iter()
        .filter_map(|(name, left)| {
            let right = b.get(name)?;
            Some((
                name.clone(),
                Tracked {
                    number: left.number,
                    range: left.range.hull(right.range),
                },
            ))
        })
        .collect()
}

/// Widens a loop head: a bound that moved since the previous head goes
/// straight to its type's limit, so the iteration terminates.
fn widen(old: &Env, new: &Env) -> Env {
    new.iter()
        .filter_map(|(name, next)| {
            let previous = old.get(name)?;
            let full = Interval::of_type(next.number);
            let lo = if next.range.lo < previous.range.lo {
                full.lo
            } else {
                previous.range.lo
            };
            let hi = if next.range.hi > previous.range.hi {
                full.hi
            } else {
                previous.range.hi
            };
            Some((
                name.clone(),
                Tracked {
                    number: next.number,
                    range: Interval { lo, hi },
                },
            ))
        })
        .collect()
}

/// A comparison, normalized so that it is the relation that holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Relation {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl Relation {
    /// The relation `op` states when it evaluates to `truth`, or `None` when
    /// `op` is not a comparison.
    fn of(op: &OperatorKind, truth: bool) -> Option<Self> {
        let relation = match op {
            OperatorKind::Lt => Relation::Lt,
            OperatorKind::Le => Relation::Le,
            OperatorKind::Gt => Relation::Gt,
            OperatorKind::Ge => Relation::Ge,
            OperatorKind::Eq => Relation::Eq,
            OperatorKind::Ne => Relation::Ne,
            _ => return None,
        };
        Some(if truth { relation } else { relation.negated() })
    }

    fn negated(self) -> Self {
        match self {
            Relation::Lt => Relation::Ge,
            Relation::Le => Relation::Gt,
            Relation::Gt => Relation::Le,
            Relation::Ge => Relation::Lt,
            Relation::Eq => Relation::Ne,
            Relation::Ne => Relation::Eq,
        }
    }

    /// The intervals the two operands can lie in on a run where the relation
    /// holds between them, or `None` when no pair of values satisfies it.
    fn narrow(self, left: Interval, right: Interval) -> Option<(Interval, Interval)> {
        let (left_bound, right_bound) = match self {
            Relation::Lt => (
                Interval {
                    lo: i128::MIN,
                    hi: right.hi - 1,
                },
                Interval {
                    lo: left.lo + 1,
                    hi: i128::MAX,
                },
            ),
            Relation::Le => (
                Interval {
                    lo: i128::MIN,
                    hi: right.hi,
                },
                Interval {
                    lo: left.lo,
                    hi: i128::MAX,
                },
            ),
            Relation::Gt => (
                Interval {
                    lo: right.lo + 1,
                    hi: i128::MAX,
                },
                Interval {
                    lo: i128::MIN,
                    hi: left.hi - 1,
                },
            ),
            Relation::Ge => (
                Interval {
                    lo: right.lo,
                    hi: i128::MAX,
                },
                Interval {
                    lo: i128::MIN,
                    hi: left.hi,
                },
            ),
            Relation::Eq => (right, left),
            Relation::Ne => return Some((excluding(left, right)?, excluding(right, left)?)),
        };
        Some((left.meet(left_bound)?, right.meet(right_bound)?))
    }
}

/// `values` without the one value `excluded` holds, when it holds exactly one
/// and that value is an endpoint of `values`; otherwise `values` unchanged.
/// `None` when `values` is that single excluded value.
fn excluding(values: Interval, excluded: Interval) -> Option<Interval> {
    let Some(value) = excluded.singleton() else {
        return Some(values);
    };
    if values.lo == value && values.hi == value {
        None
    } else if values.lo == value {
        Some(Interval {
            lo: value + 1,
            hi: values.hi,
        })
    } else if values.hi == value {
        Some(Interval {
            lo: values.lo,
            hi: value - 1,
        })
    } else {
        Some(values)
    }
}

/// The result of a checked or wrapping `+`, `-`, `*` or unary `-` whose exact
/// result lies in `exact`, at a type holding `full`.
///
/// A checked operator traps on a result outside the type, so every run that
/// continues has a result inside it; `None` when every result traps. A
/// wrapping one that can leave the type can wrap to any value of it.
fn arithmetic_result(exact: Interval, full: Interval, mode: ArithMode) -> Option<Interval> {
    match mode {
        ArithMode::Checked => exact.meet(full),
        ArithMode::Wrapping => Some(if exact.within(full) { exact } else { full }),
    }
}

/// The smallest interval holding every value of `f` at the four corners of
/// `left` × `right`, or `None` when one of them does not fit `i128`.
fn corners(
    left: Interval,
    right: Interval,
    f: impl Fn(i128, i128) -> Option<i128>,
) -> Option<Interval> {
    let values = [
        f(left.lo, right.lo)?,
        f(left.lo, right.hi)?,
        f(left.hi, right.lo)?,
        f(left.hi, right.hi)?,
    ];
    Some(Interval {
        lo: *values.iter().min()?,
        hi: *values.iter().max()?,
    })
}

fn bit_width(number: NumberType) -> i128 {
    match number {
        NumberType::I8 | NumberType::U8 => 8,
        NumberType::I16 | NumberType::U16 => 16,
        NumberType::I32 | NumberType::U32 => 32,
        NumberType::I64 | NumberType::U64 => 64,
    }
}

struct Interpreter<'a> {
    ctx: &'a TypedContext,
    arena: &'a AstArena,
    /// Whether facts are being recorded. Off while a loop head is iterated to
    /// its fixpoint, whose intermediate states are not yet sound for the body;
    /// on for the one pass over the body from the stable head.
    recording: bool,
    /// Statements left before loops are analyzed in a single pass.
    budget: u32,
    /// For each enclosing loop, the join of the states its `break`s leave it
    /// in during the current pass.
    breaks: Vec<State>,
    indices: FxHashMap<ExprId, IndexReach>,
    failing_asserts: Vec<StmtId>,
}

impl Interpreter<'_> {
    fn exec_block(&mut self, state: State, block: BlockId) -> State {
        let arena = self.arena;
        arena[block]
            .stmts
            .iter()
            .fold(state, |state, &stmt| self.exec_stmt(state, stmt))
    }

    fn exec_stmt(&mut self, state: State, stmt_id: StmtId) -> State {
        self.budget = self.budget.saturating_sub(1);
        let arena = self.arena;
        match &arena[stmt_id].kind {
            Stmt::Block(block) => self.exec_block(state, *block),
            Stmt::Expr(expr) => {
                self.visit(*expr, &state, ArithMode::DEFAULT);
                state
            }
            Stmt::VarDef { name, value, .. } => {
                let range = value.and_then(|value| self.evaluate(value, &state));
                self.bind(state, &arena[*name].name, NodeId::Stmt(stmt_id), range)
            }
            Stmt::ConstDef(def_id) => {
                let Def::Constant { name, value, .. } = &arena[*def_id].kind else {
                    return state;
                };
                let range = self.evaluate(*value, &state);
                self.bind(state, &arena[*name].name, NodeId::Stmt(stmt_id), range)
            }
            Stmt::Assign { left, right } => {
                let target = if let Expr::Identifier(ident) = &arena[*left].kind {
                    Some(&arena[*ident].name)
                } else {
                    self.visit(*left, &state, ArithMode::DEFAULT);
                    None
                };
                let range = self.evaluate(*right, &state);
                match target {
                    Some(name) => self.bind(state, name, NodeId::Expr(*left), range),
                    None => state,
                }
            }
            Stmt::Return { expr } => {
                self.visit(*expr, &state, ArithMode::DEFAULT);
                None
            }
            Stmt::Break => {
                if let Some(exit) = self.breaks.last_mut() {
                    *exit = join(exit.take(), state);
                }
                None
            }
            Stmt::Assert { expr } => {
                self.visit(*expr, &state, ArithMode::DEFAULT);
                if self.recording
                    && state.is_some()
                    && self
                        .refine(state.clone(), *expr, true, ArithMode::DEFAULT)
                        .is_none()
                {
                    self.failing_asserts.push(stmt_id);
                }
                state
            }
            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                self.visit(*condition, &state, ArithMode::DEFAULT);
                let taken = self.refine(state.clone(), *condition, true, ArithMode::DEFAULT);
                let skipped = self.refine(state, *condition, false, ArithMode::DEFAULT);
                let after_then = self.exec_block(taken, *then_block);
                let after_else = match else_block {
                    Some(block) => self.exec_block(skipped, *block),
                    None => skipped,
                };
                join(after_then, after_else)
            }
            Stmt::Loop { condition, body } => self.exec_loop(state, *condition, *body),
        }
    }

    /// Visits `expr` and returns the interval its value lies in.
    fn evaluate(&mut self, expr: ExprId, state: &State) -> Option<Interval> {
        self.visit(expr, state, ArithMode::DEFAULT);
        self.value_of(expr, state.as_ref()?, ArithMode::DEFAULT)
    }

    /// `state` with the local `name`, typed at `typed_node`, holding a value in
    /// `range` — or forgotten, when it is not an integer.
    fn bind(&self, state: State, name: &str, typed_node: NodeId, range: Option<Interval>) -> State {
        let mut env = state?;
        match self.integer_type_of(typed_node) {
            Some(number) => {
                let full = Interval::of_type(number);
                let range = range.and_then(|range| range.meet(full)).unwrap_or(full);
                env.insert(name.to_string(), Tracked { number, range });
            }
            None => {
                env.remove(name);
            }
        }
        Some(env)
    }

    fn exec_loop(&mut self, state: State, condition: Option<ExprId>, body: BlockId) -> State {
        let Some(entry) = state else {
            // Nothing reaches the loop; one pass records every access in it as
            // unreachable.
            self.loop_pass(None, condition, body);
            return None;
        };
        let recording = std::mem::replace(&mut self.recording, false);
        let mut head = entry;
        let mut iterations = 0;
        loop {
            if self.budget == 0 || iterations == MAX_LOOP_ITERATIONS {
                head = self.havoc(head, body);
                break;
            }
            let (back_edge, _) = self.loop_pass(Some(head.clone()), condition, body);
            let next = match back_edge {
                Some(back_edge) => widen(&head, &join_env(&head, &back_edge)),
                None => head.clone(),
            };
            if next == head {
                break;
            }
            head = next;
            iterations += 1;
        }
        self.recording = recording;
        let (_, breaks) = self.loop_pass(Some(head.clone()), condition, body);
        let exhausted = match condition {
            Some(condition) => self.refine(Some(head), condition, false, ArithMode::DEFAULT),
            None => None,
        };
        join(exhausted, breaks)
    }

    /// One pass over a loop from `head`: the condition is evaluated there and
    /// narrows the state the body starts in. Returns the state the body falls
    /// through to the next iteration in, and the join of its `break`s.
    fn loop_pass(
        &mut self,
        head: State,
        condition: Option<ExprId>,
        body: BlockId,
    ) -> (State, State) {
        let entry = match condition {
            Some(condition) => {
                self.visit(condition, &head, ArithMode::DEFAULT);
                self.refine(head, condition, true, ArithMode::DEFAULT)
            }
            None => head,
        };
        self.breaks.push(None);
        let back_edge = self.exec_block(entry, body);
        let breaks = self.breaks.pop().flatten();
        (back_edge, breaks)
    }

    /// `head` without every local `body` declares or assigns.
    ///
    /// A head that knows nothing about the locals a body writes is a fixpoint
    /// by construction: every other local only narrows on the way through the
    /// body, so the back edge cannot widen the head. This is the single-pass
    /// fallback for a loop the analysis stops iterating.
    fn havoc(&self, mut head: Env, body: BlockId) -> Env {
        let arena = self.arena;
        let mut written = FxHashSet::default();
        walker::walk_block_stmts(arena, body, &mut |stmt_id| match &arena[stmt_id].kind {
            Stmt::VarDef { name, .. } => {
                written.insert(arena[*name].name.clone());
            }
            Stmt::ConstDef(def_id) => {
                if let Def::Constant { name, .. } = &arena[*def_id].kind {
                    written.insert(arena[*name].name.clone());
                }
            }
            Stmt::Assign { left, .. } => {
                if let Expr::Identifier(ident) = &arena[*left].kind {
                    written.insert(arena[*ident].name.clone());
                }
            }
            _ => {}
        });
        head.retain(|name, _| !written.contains(name));
        head
    }

    /// Walks `expr` in evaluation order, recording each array access's index
    /// interval under the state that reaches it. The right operand of `&&`
    /// runs only where the left one held, and that of `||` only where it did
    /// not, so each is visited under the state its left operand leaves.
    fn visit(&mut self, expr: ExprId, state: &State, mode: ArithMode) {
        let arena = self.arena;
        match &arena[expr].kind {
            Expr::ArithMode { mode, expr } => self.visit(*expr, state, *mode),
            Expr::Binary { left, right, op }
                if matches!(op, OperatorKind::And | OperatorKind::Or) =>
            {
                self.visit(*left, state, mode);
                let reached =
                    self.refine(state.clone(), *left, matches!(op, OperatorKind::And), mode);
                self.visit(*right, &reached, mode);
            }
            Expr::ArrayIndexAccess { array, index } => {
                self.visit(*array, state, mode);
                self.visit(*index, state, mode);
                if self.recording && !matches!(arena[*index].kind, Expr::NumberLiteral { .. }) {
                    let reach = match state {
                        None => Some(IndexReach::Unreachable),
                        Some(env) => self.value_of(*index, env, mode).map(IndexReach::Within),
                    };
                    if let Some(reach) = reach {
                        self.indices
                            .entry(expr)
                            .and_modify(|known| *known = known.join(reach))
                            .or_insert(reach);
                    }
                }
            }
            _ => walker::expr_children(arena, expr, &mut |child| self.visit(child, state, mode)),
        }
    }

    /// The interval `expr`'s value lies in on every run reaching it in `env`,
    /// or `None` when `expr` is not an integer.
    fn value_of(&self, expr: ExprId, env: &Env, mode: ArithMode) -> Option<Interval> {
        let number = self.integer_type_of(NodeId::Expr(expr))?;
        let full = Interval::of_type(number);
        let arena = self.arena;
        let value = match &arena[expr].kind {
            Expr::NumberLiteral { value } => value.parse::<i128>().ok().map(Interval::exact),
            // A module constant is one value on every run, which the type
            // checker has already computed; a local of the same name shadows
            // it, which name resolution has already decided by recording no
            // constant for the identifier.
            Expr::Identifier(_) | Expr::TypeMemberAccess { .. }
                if self.ctx.module_const_ref(expr).is_some() =>
            {
                self.module_const_interval(expr)
            }
            Expr::Identifier(ident) => env.get(&arena[*ident].name).map(|tracked| tracked.range),
            Expr::Parenthesized { expr } => self.value_of(*expr, env, mode),
            Expr::ArithMode { mode, expr } => self.value_of(*expr, env, *mode),
            Expr::PrefixUnary {
                op: UnaryOperatorKind::Neg,
                expr,
            } => {
                let operand = self.value_of(*expr, env, mode)?;
                arithmetic_result(operand.subtracted_from(0), full, mode)
            }
            Expr::Binary { left, right, op } => {
                let left = self.value_of(*left, env, mode)?;
                let right = self.value_of(*right, env, mode)?;
                binary_value(op, left, right, number, mode)
            }
            _ => None,
        };
        Some(value.and_then(|value| value.meet(full)).unwrap_or(full))
    }

    /// `state` narrowed to the runs on which `condition` evaluates to `truth`;
    /// `None` when there are none.
    fn refine(&self, state: State, condition: ExprId, truth: bool, mode: ArithMode) -> State {
        let env = state?;
        let arena = self.arena;
        match &arena[condition].kind {
            Expr::Parenthesized { expr } => self.refine(Some(env), *expr, truth, mode),
            Expr::ArithMode { mode, expr } => self.refine(Some(env), *expr, truth, *mode),
            Expr::BoolLiteral { value } => (*value == truth).then_some(env),
            Expr::PrefixUnary {
                op: UnaryOperatorKind::Not,
                expr,
            } => self.refine(Some(env), *expr, !truth, mode),
            Expr::Binary { left, right, op } => match (op, truth) {
                (OperatorKind::And, true) | (OperatorKind::Or, false) => {
                    let after_left = self.refine(Some(env), *left, truth, mode);
                    self.refine(after_left, *right, truth, mode)
                }
                (OperatorKind::And, false) | (OperatorKind::Or, true) => {
                    let decided = self.refine(Some(env.clone()), *left, truth, mode);
                    let undecided = self.refine(Some(env), *left, !truth, mode);
                    join(decided, self.refine(undecided, *right, truth, mode))
                }
                _ => match Relation::of(op, truth) {
                    Some(relation) => self.refine_comparison(env, *left, *right, relation, mode),
                    None => Some(env),
                },
            },
            _ => Some(env),
        }
    }

    fn refine_comparison(
        &self,
        env: Env,
        left: ExprId,
        right: ExprId,
        relation: Relation,
        mode: ArithMode,
    ) -> State {
        let (Some(left_range), Some(right_range)) = (
            self.value_of(left, &env, mode),
            self.value_of(right, &env, mode),
        ) else {
            return Some(env);
        };
        let (left_range, right_range) = relation.narrow(left_range, right_range)?;
        let env = self.narrow(env, left, left_range, mode)?;
        self.narrow(env, right, right_range, mode)
    }

    /// `env` narrowed so that `expr` evaluates into `target`: a local is
    /// narrowed directly, and a checked `x + c`, `c + x`, `x - c` or `c - x`
    /// with a constant `c` narrows `x` — exact because a checked operator that
    /// did not trap computed the mathematical result. `None` when no value of
    /// the local lands in `target`.
    fn narrow(&self, mut env: Env, expr: ExprId, target: Interval, mode: ArithMode) -> State {
        let arena = self.arena;
        match &arena[expr].kind {
            Expr::Parenthesized { expr } => self.narrow(env, *expr, target, mode),
            Expr::ArithMode { mode, expr } => self.narrow(env, *expr, target, *mode),
            Expr::Identifier(ident) => {
                let Some(number) = self.integer_type_of(NodeId::Expr(expr)) else {
                    return Some(env);
                };
                // A module constant has one value on every run, which no
                // comparison narrows; and it is no local the environment tracks.
                if self.ctx.binding_mutability(expr).is_none()
                    || self.ctx.module_const_ref(expr).is_some()
                {
                    return Some(env);
                }
                let name = &arena[*ident].name;
                let current = env
                    .get(name)
                    .map_or_else(|| Interval::of_type(number), |tracked| tracked.range);
                let range = current.meet(target)?;
                env.insert(name.clone(), Tracked { number, range });
                Some(env)
            }
            Expr::Binary { left, right, op }
                if mode == ArithMode::Checked
                    && matches!(op, OperatorKind::Add | OperatorKind::Sub) =>
            {
                let constant = |operand| {
                    self.value_of(operand, &env, mode)
                        .and_then(Interval::singleton)
                };
                match (op, constant(*left), constant(*right)) {
                    (OperatorKind::Add, _, Some(c)) => {
                        self.narrow(env, *left, target.offset(-c), mode)
                    }
                    (OperatorKind::Add, Some(c), None) => {
                        self.narrow(env, *right, target.offset(-c), mode)
                    }
                    (OperatorKind::Sub, _, Some(c)) => {
                        self.narrow(env, *left, target.offset(c), mode)
                    }
                    (OperatorKind::Sub, Some(c), None) => {
                        self.narrow(env, *right, target.subtracted_from(c), mode)
                    }
                    _ => Some(env),
                }
            }
            _ => Some(env),
        }
    }

    /// The integer type the type checker recorded at `node`, if it is one.
    /// The exact interval of the integer module constant `expr` names, or
    /// `None` when it names a compound constant or one with no value.
    fn module_const_interval(&self, expr: ExprId) -> Option<Interval> {
        let def_id = self.ctx.module_const_ref(expr)?;
        match self.ctx.module_const_value(def_id)? {
            ConstValue::Int { value, .. } => Some(Interval::exact(*value)),
            _ => None,
        }
    }

    fn integer_type_of(&self, node: NodeId) -> Option<NumberType> {
        match self.ctx.get_node_typeinfo(node)?.kind {
            TypeInfoKind::Number(number) => Some(number),
            _ => None,
        }
    }
}

/// The interval of `left op right` at `number` under `mode`, or `None` when
/// the analysis does not model `op` for these operands (the result is then any
/// value of the type).
fn binary_value(
    op: &OperatorKind,
    left: Interval,
    right: Interval,
    number: NumberType,
    mode: ArithMode,
) -> Option<Interval> {
    let full = Interval::of_type(number);
    match op {
        OperatorKind::Add => arithmetic_result(
            Interval {
                lo: left.lo + right.lo,
                hi: left.hi + right.hi,
            },
            full,
            mode,
        ),
        OperatorKind::Sub => arithmetic_result(
            Interval {
                lo: left.lo - right.hi,
                hi: left.hi - right.lo,
            },
            full,
            mode,
        ),
        OperatorKind::Mul => {
            arithmetic_result(corners(left, right, i128::checked_mul)?, full, mode)
        }
        // Division and remainder truncate toward zero, as `div`/`rem` do, and
        // neither overflows by a positive divisor. Any other divisor may be zero
        // or, for a signed type, the `-1` that overflows the minimum.
        OperatorKind::Div if right.lo > 0 => corners(left, right, i128::checked_div),
        OperatorKind::Mod if right.lo > 0 => {
            let largest = right.hi - 1;
            Some(Interval {
                lo: if left.lo >= 0 {
                    0
                } else {
                    left.lo.max(-largest)
                },
                hi: if left.hi <= 0 {
                    0
                } else {
                    left.hi.min(largest)
                },
            })
        }
        // A non-negative operand clears every bit it does not have, so the
        // result lies between zero and that operand.
        OperatorKind::BitAnd => match (left.lo >= 0, right.lo >= 0) {
            (true, true) => Some(Interval {
                lo: 0,
                hi: left.hi.min(right.hi),
            }),
            (true, false) => Some(Interval { lo: 0, hi: left.hi }),
            (false, true) => Some(Interval {
                lo: 0,
                hi: right.hi,
            }),
            (false, false) => None,
        },
        OperatorKind::Shr if left.lo >= 0 => {
            let count = right
                .singleton()
                .filter(|count| (0..bit_width(number)).contains(count))?;
            let count = u32::try_from(count).ok()?;
            Some(Interval {
                lo: left.lo >> count,
                hi: left.hi >> count,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iv(lo: i128, hi: i128) -> Interval {
        Interval { lo, hi }
    }

    #[test]
    fn checked_arithmetic_keeps_the_runs_that_do_not_trap() {
        let full = Interval::of_type(NumberType::I8);
        assert_eq!(
            arithmetic_result(iv(100, 200), full, ArithMode::Checked),
            Some(iv(100, 127))
        );
        assert_eq!(
            arithmetic_result(iv(128, 200), full, ArithMode::Checked),
            None
        );
    }

    #[test]
    fn wrapping_arithmetic_that_can_leave_the_type_is_any_value() {
        let full = Interval::of_type(NumberType::I8);
        assert_eq!(
            arithmetic_result(iv(100, 128), full, ArithMode::Wrapping),
            Some(full)
        );
        assert_eq!(
            arithmetic_result(iv(100, 127), full, ArithMode::Wrapping),
            Some(iv(100, 127))
        );
    }

    #[test]
    fn division_by_a_positive_divisor_truncates_toward_zero() {
        let v = binary_value(
            &OperatorKind::Div,
            iv(-7, 9),
            iv(2, 3),
            NumberType::I32,
            ArithMode::Checked,
        );
        assert_eq!(v, Some(iv(-3, 4)));
        let unknown = binary_value(
            &OperatorKind::Div,
            iv(1, 9),
            iv(0, 3),
            NumberType::I32,
            ArithMode::Checked,
        );
        assert_eq!(unknown, None);
    }

    #[test]
    fn remainder_keeps_the_sign_of_the_dividend() {
        let m = |left| {
            binary_value(
                &OperatorKind::Mod,
                left,
                iv(8, 8),
                NumberType::I32,
                ArithMode::Checked,
            )
        };
        assert_eq!(m(iv(0, 100)), Some(iv(0, 7)));
        assert_eq!(m(iv(-100, 3)), Some(iv(-7, 3)));
        assert_eq!(m(iv(-100, -1)), Some(iv(-7, 0)));
    }

    #[test]
    fn masking_with_a_non_negative_operand_is_bounded_by_it() {
        let v = binary_value(
            &OperatorKind::BitAnd,
            iv(-50, 50),
            iv(7, 7),
            NumberType::I32,
            ArithMode::Checked,
        );
        assert_eq!(v, Some(iv(0, 7)));
        let unknown = binary_value(
            &OperatorKind::BitAnd,
            iv(-50, 50),
            iv(-8, 7),
            NumberType::I32,
            ArithMode::Checked,
        );
        assert_eq!(unknown, None);
    }

    #[test]
    fn right_shift_of_a_non_negative_value_by_a_constant() {
        let v = binary_value(
            &OperatorKind::Shr,
            iv(0, 255),
            iv(4, 4),
            NumberType::U8,
            ArithMode::Checked,
        );
        assert_eq!(v, Some(iv(0, 15)));
        let out_of_width = binary_value(
            &OperatorKind::Shr,
            iv(0, 255),
            iv(8, 8),
            NumberType::U8,
            ArithMode::Checked,
        );
        assert_eq!(out_of_width, None);
    }

    #[test]
    fn relations_narrow_both_operands() {
        assert_eq!(
            Relation::Lt.narrow(iv(0, 100), iv(5, 8)),
            Some((iv(0, 7), iv(5, 8)))
        );
        assert_eq!(
            Relation::Ge.narrow(iv(-10, 10), iv(0, 0)),
            Some((iv(0, 10), iv(0, 0)))
        );
        assert_eq!(Relation::Lt.narrow(iv(8, 9), iv(0, 8)), None);
        assert_eq!(Relation::Ne.narrow(iv(3, 3), iv(3, 3)), None);
        assert_eq!(
            Relation::Ne.narrow(iv(3, 9), iv(3, 3)),
            Some((iv(4, 9), iv(3, 3)))
        );
    }

    #[test]
    fn negation_flips_every_relation() {
        for relation in [
            Relation::Lt,
            Relation::Le,
            Relation::Gt,
            Relation::Ge,
            Relation::Eq,
            Relation::Ne,
        ] {
            assert_eq!(relation.negated().negated(), relation);
            assert_ne!(relation.negated(), relation);
        }
    }

    #[test]
    fn widening_sends_a_moving_bound_to_its_type_limit() {
        let tracked = |lo, hi| Tracked {
            number: NumberType::I32,
            range: iv(lo, hi),
        };
        let old: Env = [("i".to_string(), tracked(0, 0))].into_iter().collect();
        let new: Env = [("i".to_string(), tracked(0, 1))].into_iter().collect();
        let widened = widen(&old, &new);
        assert_eq!(widened["i"].range, iv(0, i128::from(i32::MAX)));
    }

    #[test]
    fn a_name_one_side_does_not_know_is_dropped_by_join() {
        let tracked = Tracked {
            number: NumberType::I32,
            range: iv(0, 3),
        };
        let a: Env = [("i".to_string(), tracked), ("j".to_string(), tracked)]
            .into_iter()
            .collect();
        let b: Env = [("i".to_string(), tracked)].into_iter().collect();
        let joined = join_env(&a, &b);
        assert!(joined.contains_key("i"));
        assert!(!joined.contains_key("j"));
    }
}
