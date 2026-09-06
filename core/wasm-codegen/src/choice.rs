//! Choice lowering: a specification function's `@`s arrive as hidden trailing
//! parameters, so its body compiles to vanilla WebAssembly.
//!
//! A `@` outside a specification is drawn with the custom `0xfc 0x31`/`0x32`
//! opcode, which no standard WebAssembly tool can load. Inside a specification
//! the draw is unnecessary: nothing downstream reads a spec body's bytes as a
//! non-deterministic program. The universal (`ValidSpec`) judgment reads the
//! obligation, which the obligation pass builds from the typed AST; the
//! reachability judgment reduces the compiled body under the *plain*
//! WebAssembly semantics, where a `0xfc` byte has no reduction rule at all. So
//! every `@` a specification body draws is replaced by a read of a parameter
//! the caller (or, for a proof, the quantifier) supplies, and every
//! non-deterministic block wrapper — body modifier or nested block, any of the
//! four kinds — is dropped. The statements themselves lower unchanged, in
//! source order.
//!
//! This module is the single place that decides which `@` becomes which
//! parameter. [`plan_choice_lowering`] walks every specification function code
//! generation will compile — free functions *and* methods — once, in source
//! order, and records one [`ChoicePlan`] per function. Three consumers read the
//! same `ExprId`-keyed map: the compiler's signature suffix, the compiler's
//! body lowering, and (for `exists`/`unique` bodies only, through
//! [`crate::hassert::reach`]) the obligation pass's payload slot indices.
//!
//! ## Scope comes from the emittable-function buckets
//!
//! The planner iterates [`EmittableFunctions::spec_funcs`] and
//! [`EmittableFunctions::spec_methods`] — the exact lists
//! `traverse_t_ast_with_compiler` compiles — rather than re-walking the AST.
//! A second walk can drift from the one code generation performs, and drift
//! here is invisible: an unplanned `@` silently keeps its custom opcode.
//!
//! ## Ordinals, not absolute local indices
//!
//! A [`ChoiceRun`] records an *ordinal into the parameter suffix*, counted from
//! the first choice parameter. The absolute WebAssembly local is
//! `suffix_base + ordinal`, where `suffix_base` is the local index the compiler
//! *observes* at the site it appends the suffix. That index already counts an
//! sret pointer and a method receiver, both of which precede the declared
//! parameters. Re-deriving either here would fork the classifier that decides
//! them, and the two forks can disagree without any signal — an sret pointer,
//! a receiver and an `i32` choice are all `i32`, so the module still validates
//! while the body reads the wrong local.
//!
//! ## Scalars and aggregates are told apart by kind, never by count
//!
//! A scalar `@` binds one parameter and is read directly. An aggregate `@`
//! (`let a: [i32; 1] = @;`, `let p: Point = @;`) binds one parameter per scalar
//! *leaf*, and its value is a pointer to a frame slot the emitter fills leaf by
//! leaf. A one-leaf aggregate reserves exactly one parameter, so a length test
//! cannot tell the two apart: treating `[i32; 1]` as a scalar would skip the
//! frame store and bind a raw `i32` where every later access expects a pointer,
//! and the module would still validate. [`ChoiceRun`] is therefore an enum
//! discriminated on the planned kind.
//!
//! ## The frame contract
//!
//! [`FrameContract::Bound`] marks the one shape whose *absolute* slot
//! arithmetic is load-bearing: an `exists`/`unique`-bodied specification free
//! function, whose obligation payload denotes against the real activation
//! frame. There, and only there, `suffix_base` must equal the declared arity —
//! which the no-return rule in [`crate::hassert::reach`] is what guarantees.
//! Every other specification function is [`FrameContract::Free`]: it may carry
//! an sret pointer, a receiver, a declared return type and a `return`, because
//! its obligation is built from the AST and never denotes a frame index.
//!
//! ## Purity boundary
//!
//! Everything here is a pure function of [`TypedContext`]/[`AstArena`] and the
//! collected buckets — no compiler state is read or written. The plans are
//! facts about the typed program, computed once ahead of code generation,
//! never scraped out of one backend by another.

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, ExprId, NodeId};
use inference_ast::nodes::{BlockKind, Def, Expr, Stmt};
use inference_type_checker::type_info::{NumberType, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;
use rustc_hash::FxHashMap;

use crate::EmittableFunctions;
use crate::compiler::{
    Compiler, MAX_UZUMAKI_UNROLL_ELEMENTS, UZUMAKI_I32_OPCODE, UZUMAKI_I64_OPCODE,
    leaf_scalar_type, total_leaf_count,
};
use crate::memory::{CompoundFieldLayout, compute_struct_field_layout};

/// WASM value class of one choice parameter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ChoiceClass {
    /// `bool`, sub-32-bit and 32-bit integers, and enum tags — one `i32`
    /// parameter, domain-normalized at its use (or binding) site.
    I32,
    /// `i64`/`u64` — one `i64` parameter; every bit pattern is in-domain.
    I64,
}

impl ChoiceClass {
    /// The class an uzumaki draw opcode would have produced. The emitters pick
    /// the opcode from the slot's own type, so this is the *emitter's*
    /// classification of a leaf, which [`ChoiceCursor::take`] checks against
    /// the planner's.
    pub(crate) fn of_draw_opcode(opcode: u8) -> Self {
        match opcode {
            UZUMAKI_I64_OPCODE => Self::I64,
            UZUMAKI_I32_OPCODE => Self::I32,
            other => unreachable!("{other:#04x} is not an uzumaki draw opcode"),
        }
    }

    /// The class of a scalar type, or `None` for a type that is not a scalar
    /// this lowering can supply as one parameter.
    fn of_scalar(kind: &TypeInfoKind) -> Option<Self> {
        match kind {
            TypeInfoKind::Bool
            | TypeInfoKind::Number(
                NumberType::I8
                | NumberType::U8
                | NumberType::I16
                | NumberType::U16
                | NumberType::I32
                | NumberType::U32,
            )
            | TypeInfoKind::Enum(_, _) => Some(Self::I32),
            TypeInfoKind::Number(NumberType::I64 | NumberType::U64) => Some(Self::I64),
            _ => None,
        }
    }

    /// The class the emitters derive for a leaf, mirroring their own
    /// `is_i64_type` test rather than re-deciding it.
    fn of_leaf(kind: &TypeInfoKind) -> Self {
        if Compiler::is_i64_type(kind) {
            Self::I64
        } else {
            Self::I32
        }
    }
}

/// One planned choice parameter.
///
/// The `@` it stands for is recorded the other way round, in
/// [`ChoicePlan::by_expr`]: an aggregate `@` owns a whole contiguous run, so
/// the expression is a property of the run, not of the individual parameter.
#[derive(Clone, Debug)]
pub(crate) struct ChoiceParam {
    /// The WASM value class of the parameter.
    pub(crate) class: ChoiceClass,
    /// Whether the parameter *is* the whole value of a `let x: T = @;`.
    /// A named choice is bound directly *to* its parameter slot (no fresh
    /// local), so the source name, the name-section entry, and the obligation's
    /// payload slot index all denote the same frame value. False for every
    /// aggregate leaf: an aggregate's `let` binds a frame pointer, not a
    /// parameter.
    pub(crate) named: bool,
}

/// What one `@` expression was planned as.
///
/// Discriminated on the planned *kind*, never on a count: a one-leaf aggregate
/// reserves exactly one parameter and would be indistinguishable from a scalar
/// otherwise (see the module documentation).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ChoiceRun {
    /// A scalar `@`: the parameter at this suffix ordinal *is* the value.
    Scalar(u32),
    /// An aggregate `@`: `len` contiguous parameters starting at ordinal
    /// `first`, one per scalar leaf in the order the emitters fill them.
    Leaves { first: u32, len: u32 },
}

/// Whether a function's *absolute* choice-parameter indices are load-bearing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FrameContract {
    /// The obligation never denotes a frame index; the suffix may start
    /// anywhere the signature puts it (after an sret pointer, a receiver, and
    /// the declared parameters).
    Free,
    /// An `exists`/`unique`-bodied specification free function: the obligation
    /// payload denotes against the real activation frame, so the suffix must
    /// begin exactly at the declared arity.
    Bound,
}

/// The choice lowering plan for one specification function.
#[derive(Clone, Debug)]
pub(crate) struct ChoicePlan {
    pub(crate) contract: FrameContract,
    /// Number of declared source parameters (receiver included, sret pointer
    /// excluded). Meaningful as a slot base only under
    /// [`FrameContract::Bound`]; the compiler uses the *observed* suffix base
    /// everywhere else.
    pub(crate) entry_arity: u32,
    /// Whether the body contains a `return` statement. Recorded by the same
    /// walk that plans the choices so the body is read exactly once.
    pub(crate) has_return: bool,
    /// Planned parameters in suffix order; index k sits at WASM local
    /// `suffix_base + k`.
    pub(crate) params: Vec<ChoiceParam>,
    /// `@` expression → the run of parameters it was planned as.
    pub(crate) by_expr: FxHashMap<ExprId, ChoiceRun>,
    /// Whether every `@` in the body was planned. False when a `@` has a shape
    /// this lowering deliberately leaves alone — a compound one under
    /// [`FrameContract::Bound`], which the obligation pass rejects, or a shape
    /// the emitters themselves refuse. Such a body never reaches an artifact,
    /// so the compiler skips its end-of-body vanilla check rather than turning
    /// a diagnostic into a panic.
    pub(crate) covers_every_uzumaki: bool,
}

impl ChoicePlan {
    fn new(contract: FrameContract, entry_arity: u32) -> Self {
        Self {
            contract,
            entry_arity,
            has_return: false,
            params: Vec::new(),
            by_expr: FxHashMap::default(),
            covers_every_uzumaki: true,
        }
    }

    /// The run planned for `expr`, or `None` when that `@` still draws.
    pub(crate) fn run(&self, expr: ExprId) -> Option<ChoiceRun> {
        self.by_expr.get(&expr).copied()
    }
}

/// A one-shot reader over the parameters of one aggregate [`ChoiceRun`],
/// handed to the leaf emitters in place of their draw.
///
/// Every leaf is taken with the class the emitter itself derived, and
/// [`Self::finish`] demands the run be exhausted. Together they make a
/// planner/emitter classification drift a hard failure at the site it happens,
/// rather than a wrong artifact: the planner and the emitters each walk the
/// aggregate's layout independently, and nothing else compares their walks.
#[derive(Debug)]
pub(crate) struct ChoiceCursor {
    classes: Vec<ChoiceClass>,
    base_local: u32,
    next: usize,
}

impl ChoiceCursor {
    /// Opens a cursor over `run`'s parameters, whose first one sits at
    /// `suffix_base + first`.
    pub(crate) fn open(plan: &ChoicePlan, first: u32, len: u32, suffix_base: u32) -> Self {
        let start = first as usize;
        let end = start + len as usize;
        Self {
            classes: plan.params[start..end].iter().map(|p| p.class).collect(),
            base_local: suffix_base + first,
            next: 0,
        }
    }

    /// The WASM local of the next leaf, checked against the class the emitter
    /// derived for it.
    pub(crate) fn take(&mut self, expected: ChoiceClass) -> u32 {
        assert!(
            self.next < self.classes.len(),
            "an aggregate `@` emitted more scalar leaves than the plan reserved parameters for; \
             the planner and the emitter disagree about the aggregate's layout",
        );
        assert_eq!(
            self.classes[self.next], expected,
            "leaf {} of an aggregate `@` was planned as {:?} but emitted as {expected:?}; the \
             planner and the emitter disagree about the leaf's value class",
            self.next, self.classes[self.next],
        );
        let local = self.base_local
            + u32::try_from(self.next).expect("a run holds fewer than u32::MAX leaves");
        self.next += 1;
        local
    }

    /// Consumes the cursor, demanding every reserved parameter was read.
    pub(crate) fn finish(self) {
        assert_eq!(
            self.next,
            self.classes.len(),
            "an aggregate `@` emitted fewer scalar leaves than the plan reserved parameters for; \
             the planner and the emitter disagree about the aggregate's layout",
        );
    }
}

/// The per-function [`ChoicePlan`]s for a whole program, keyed by the
/// specification function's [`DefId`]. Empty in compile mode, where
/// specifications are not collected at all.
#[derive(Debug, Default)]
pub(crate) struct ChoicePlans {
    by_def: FxHashMap<DefId, ChoicePlan>,
}

impl ChoicePlans {
    /// The plan for `def_id`, or `None` when the function is not a
    /// specification function.
    pub(crate) fn get(&self, def_id: DefId) -> Option<&ChoicePlan> {
        self.by_def.get(&def_id)
    }
}

/// Builds the [`ChoicePlan`] of every specification function code generation
/// will compile, free functions first, then methods, in bucket order.
///
/// The parameter-count ceiling is *not* checked here. It is checked where the
/// compiler appends the suffix, against the local index it observes there —
/// the only place the sret pointer and the receiver are already counted, and
/// therefore the only place a check needs no second classifier for either.
pub(crate) fn plan_choice_lowering(
    ctx: &TypedContext,
    buckets: &EmittableFunctions,
) -> ChoicePlans {
    let arena = ctx.arena();
    let mut plans = ChoicePlans::default();
    for entry in &buckets.spec_funcs {
        let plan = plan_function(ctx, arena, entry.def_id, &entry.module_path, false);
        plans.by_def.insert(entry.def_id, plan);
    }
    for entry in &buckets.spec_methods {
        let plan = plan_function(ctx, arena, entry.def_id, &entry.module_path, true);
        plans.by_def.insert(entry.def_id, plan);
    }
    plans
}

/// Plans one specification function's body.
fn plan_function(
    ctx: &TypedContext,
    arena: &AstArena,
    def_id: DefId,
    module_path: &[String],
    is_method: bool,
) -> ChoicePlan {
    let Def::Function { args, body, .. } = &arena[def_id].kind else {
        // The buckets only ever hold functions; a non-function entry has no
        // body to plan and no signature to extend.
        return ChoicePlan::new(FrameContract::Free, 0);
    };
    let body_kind = arena[*body].block_kind;
    let contract = if !is_method && matches!(body_kind, BlockKind::Exists | BlockKind::Unique) {
        FrameContract::Bound
    } else {
        FrameContract::Free
    };
    // Every declared parameter costs one frame slot, named or not: the WebAssembly
    // signature declares one per written argument, and the choice suffix is
    // appended after all of them. A count that skipped the unnamed ones would put
    // the k-th choice on a declared parameter's slot, so the obligation payload
    // would read an argument where it expects a drawn value.
    let entry_arity = u32::try_from(args.len()).expect("more than u32::MAX parameters");

    let mut builder = PlanBuilder {
        ctx,
        arena,
        module_path,
        plan: ChoicePlan::new(contract, entry_arity),
    };
    builder.walk_block(*body);
    builder.plan
}

/// One walk over one function body, accumulating the plan and the
/// `return`-presence fact together so the body is read exactly once.
struct PlanBuilder<'a> {
    ctx: &'a TypedContext,
    arena: &'a AstArena,
    module_path: &'a [String],
    plan: ChoicePlan,
}

impl PlanBuilder<'_> {
    /// Visits a block's statements in source order. Statement kinds that the
    /// obligation pass later rejects (`loop`, reassignment, an unencodable
    /// nested block) are still walked: the plan must cover every `@` the
    /// compiler will lower, and the rejection is the obligation pass's to
    /// raise — duplicating it here would fork the diagnostic surface.
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
                    self.plan.has_return = true;
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

    /// Visits an expression tree in syntactic order, planning every `@` leaf as
    /// an anonymous choice. The variant list is exhaustive on purpose: a future
    /// expression kind must be classified here rather than silently becoming a
    /// position whose `@`s the plan misses.
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

    /// Records one `@` as a scalar parameter or as a contiguous run of leaf
    /// parameters, or declines to plan it.
    ///
    /// Declining is not a silent fallback: every declined shape either raises a
    /// diagnostic in the obligation pass or is refused by the emitters
    /// themselves, so no artifact is written. It does clear
    /// [`ChoicePlan::covers_every_uzumaki`], which is what keeps the
    /// compiler's end-of-body vanilla check from turning that diagnostic into
    /// a panic.
    fn plan_choice(&mut self, expr_id: ExprId, named: bool) {
        let Some(type_info) = self.ctx.get_node_typeinfo(NodeId::Expr(expr_id)) else {
            self.plan.covers_every_uzumaki = false;
            return;
        };
        let kind = type_info.kind.clone();
        if let Some(class) = ChoiceClass::of_scalar(&kind) {
            let ordinal = self.next_ordinal();
            self.record(expr_id, ChoiceRun::Scalar(ordinal));
            self.plan.params.push(ChoiceParam { class, named });
            return;
        }
        // An aggregate `@` in an `exists`/`unique` body stays unplanned: its
        // obligation payload would have to denote one frame slot per leaf, and
        // an aggregate lives in linear memory, which the assertion IR has no
        // term for. The obligation pass rejects it before any artifact exists.
        let Some(classes) = (match self.plan.contract {
            FrameContract::Bound => None,
            FrameContract::Free => self.leaf_classes(&kind),
        }) else {
            self.plan.covers_every_uzumaki = false;
            return;
        };
        let first = self.next_ordinal();
        let len =
            u32::try_from(classes.len()).expect("an aggregate holds fewer than u32::MAX leaves");
        self.record(expr_id, ChoiceRun::Leaves { first, len });
        self.plan
            .params
            .extend(classes.into_iter().map(|class| ChoiceParam {
                class,
                // An aggregate's `let` binds a frame pointer, never a
                // parameter, so no leaf is the source-visible face of the
                // binding.
                named: false,
            }));
    }

    fn next_ordinal(&self) -> u32 {
        u32::try_from(self.plan.params.len()).expect("more than u32::MAX choices")
    }

    fn record(&mut self, expr_id: ExprId, run: ChoiceRun) {
        let previous = self.plan.by_expr.insert(expr_id, run);
        debug_assert!(
            previous.is_none(),
            "a `@` expression was planned twice; the walk must visit every node exactly once"
        );
    }

    /// The class of every scalar leaf of an aggregate `@`, in the order the
    /// emitters fill them, or `None` for a shape the emitters refuse.
    ///
    /// Mirrors the emitters by calling their own helpers rather than
    /// re-deriving: an array takes one class for the whole array from its leaf
    /// scalar type, and a struct takes one per field slot from
    /// [`compute_struct_field_layout`], the same layout the frame slot caches.
    fn leaf_classes(&self, kind: &TypeInfoKind) -> Option<Vec<ChoiceClass>> {
        match kind {
            TypeInfoKind::Array(elem, length) => {
                let leaf = leaf_scalar_type(&elem.kind);
                // Reject what `ChoiceClass::of_scalar` would not accept as a
                // parameter, so a struct-element array (which analysis rule
                // A028 rejects) is not planned as a run of `i32`s.
                ChoiceClass::of_scalar(leaf)?;
                let total = total_leaf_count(&elem.kind, *length);
                if total > MAX_UZUMAKI_UNROLL_ELEMENTS {
                    return None;
                }
                Some(vec![ChoiceClass::of_leaf(leaf); total as usize])
            }
            TypeInfoKind::Struct(name, _) | TypeInfoKind::Custom(name) => {
                // Prefer the defining-file canonical key, exactly as the frame
                // layout does; a `::`-qualified type's leaf name is not bound
                // by name in the accessing file.
                let struct_info = match kind {
                    TypeInfoKind::Struct(_, key) => self
                        .ctx
                        .lookup_struct(key)
                        .or_else(|| self.ctx.lookup_struct_in(name, self.module_path)),
                    _ => self.ctx.lookup_struct_in(name, self.module_path),
                }?;
                if struct_info.fields.is_empty() {
                    return Some(Vec::new());
                }
                let (_, fields) =
                    compute_struct_field_layout(&struct_info, self.ctx, self.module_path).ok()?;
                let mut classes = Vec::new();
                for field in &fields {
                    match &field.layout {
                        CompoundFieldLayout::Scalar => {
                            ChoiceClass::of_scalar(&field.type_kind)?;
                            classes.push(ChoiceClass::of_leaf(&field.type_kind));
                        }
                        CompoundFieldLayout::NestedArray {
                            elem_kind, length, ..
                        } => {
                            if *length > MAX_UZUMAKI_UNROLL_ELEMENTS {
                                return None;
                            }
                            ChoiceClass::of_scalar(elem_kind)?;
                            let class = ChoiceClass::of_leaf(elem_kind);
                            classes.extend(std::iter::repeat_n(class, *length as usize));
                        }
                        // Analysis rule A027 rejects a `@` over a struct with a
                        // nested struct field, and the emitter refuses it too.
                        CompoundFieldLayout::NestedStruct { .. } => return None,
                    }
                }
                Some(classes)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use inference_type_checker::TypeCheckerBuilder;
    use inference_type_checker::typed_context::TypedContext;

    use super::{ChoiceClass, ChoicePlan, ChoicePlans, ChoiceRun, FrameContract};
    use crate::CompilationMode;

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

    fn plans_of(source: &str) -> ChoicePlans {
        let ctx = type_check(source);
        let mut buckets = crate::EmittableFunctions::default();
        for source_file in ctx.source_files() {
            crate::collect_emittable_functions(
                ctx.arena(),
                &source_file.defs,
                &source_file.module_path,
                CompilationMode::Proof,
                &mut buckets,
            )
            .expect("collecting emittable functions should succeed");
        }
        super::plan_choice_lowering(&ctx, &buckets)
    }

    /// The plan of the program's single specification function.
    fn sole_plan(source: &str) -> ChoicePlan {
        let plans = plans_of(source);
        assert_eq!(
            plans.by_def.len(),
            1,
            "expected exactly one specification function"
        );
        plans.by_def.into_values().next().expect("checked above")
    }

    fn classes(plan: &ChoicePlan) -> Vec<ChoiceClass> {
        plan.params.iter().map(|p| p.class).collect()
    }

    /// Every planned run, ordered by the suffix ordinal it starts at — which is
    /// source order, since ordinals are handed out as the walk encounters `@`s.
    fn runs(plan: &ChoicePlan) -> Vec<ChoiceRun> {
        let mut runs: Vec<ChoiceRun> = plan.by_expr.values().copied().collect();
        runs.sort_by_key(|run| match run {
            ChoiceRun::Scalar(ordinal) => *ordinal,
            ChoiceRun::Leaves { first, .. } => *first,
        });
        runs
    }

    #[test]
    fn every_quantifier_is_planned_including_plain_and_assume_bodies() {
        let plans = plans_of(
            "spec S {
              fn a() forall { let n: i32 = @; assert(n >= n); }
              fn b(x: i32) { assert(x >= x); }
              fn c() assume { let n: i32 = @; assert(n >= n); }
              fn d() exists { let n: i32 = @; assert(n == 1); }
              fn e() unique { let n: i32 = @; assert(n == 1); }
            }",
        );
        assert_eq!(
            plans.by_def.len(),
            5,
            "every specification function is choice-lowered, whatever its body kind"
        );
    }

    #[test]
    fn a_spec_method_is_planned() {
        let plans = plans_of(
            "spec S {
              struct T {
                x: i32;
                fn m(self) forall {
                  let y: i32 = @;
                  assert(y > 0);
                }
              }
            }",
        );
        assert_eq!(plans.by_def.len(), 1, "a specification method is planned");
        let plan = plans.by_def.into_values().next().expect("checked above");
        assert_eq!(plan.contract, FrameContract::Free);
        assert_eq!(plan.params.len(), 1);
        assert_eq!(plan.entry_arity, 1, "`self` is a declared parameter");
    }

    #[test]
    fn an_exists_free_function_carries_the_bound_frame_contract() {
        let plan = sole_plan(
            "spec S {
              fn f(x: i32) exists {
                let a: i64 = @;
                assert(a > 0);
              }
            }",
        );
        assert_eq!(plan.contract, FrameContract::Bound);
        assert_eq!(plan.entry_arity, 1);
        assert_eq!(classes(&plan), vec![ChoiceClass::I64]);
        assert!(plan.params[0].named);
    }

    #[test]
    fn choices_are_ordered_across_if_condition_then_and_else() {
        let plan = sole_plan(
            "fn g(v: i32) -> i32 { return v; }
            spec S {
              fn f(x: i32) forall {
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
        let named: Vec<_> = plan.params.iter().map(|p| p.named).collect();
        assert_eq!(
            named,
            vec![false, true, true],
            "condition `@` first, then the then-arm binding, then the else-arm binding"
        );
    }

    #[test]
    fn a_nested_block_contributes_its_choices() {
        let plan = sole_plan(
            "spec S {
              fn f(x: i32) forall {
                assume { assert(x > 0); }
                exists {
                  let n: i32 = @;
                  assert(n > x);
                }
              }
            }",
        );
        assert_eq!(plan.params.len(), 1, "the nested block's `@` hoists");
        assert_eq!(runs(&plan), vec![ChoiceRun::Scalar(0)]);
    }

    /// A one-leaf array must be planned as an aggregate, never as a scalar: it
    /// reserves exactly one parameter, so only the run's *kind* tells the two
    /// apart, and treating it as a scalar would skip the frame-slot store.
    #[test]
    fn a_single_element_array_is_planned_as_an_aggregate() {
        let plan = sole_plan(
            "spec S {
              fn f() forall {
                let a: [i32; 1] = @;
                assert(a[0] >= a[0]);
              }
            }",
        );
        assert_eq!(plan.params.len(), 1);
        assert_eq!(
            runs(&plan),
            vec![ChoiceRun::Leaves { first: 0, len: 1 }],
            "a one-leaf aggregate is an aggregate run, not a scalar one"
        );
        assert!(!plan.params[0].named, "an aggregate leaf is never named");
    }

    /// A struct with exactly one scalar field is the other single-leaf shape.
    #[test]
    fn a_single_field_struct_is_planned_as_an_aggregate() {
        let plan = sole_plan(
            "struct One { x: i32; }
            spec S {
              fn f() forall {
                let s: One = @;
                assert(s.x >= s.x);
              }
            }",
        );
        assert_eq!(runs(&plan), vec![ChoiceRun::Leaves { first: 0, len: 1 }]);
    }

    #[test]
    fn a_mixed_aggregate_body_reserves_one_parameter_per_leaf() {
        let plan = sole_plan(
            "struct Pt { x: i32; y: i64; }
            spec S {
              fn f() forall {
                let a: [i32; 3] = @;
                let p: Pt = @;
                let b: bool = @;
                assert(b || a[0] == 0 || p.x == 0);
              }
            }",
        );
        assert_eq!(
            classes(&plan),
            vec![
                ChoiceClass::I32,
                ChoiceClass::I32,
                ChoiceClass::I32,
                ChoiceClass::I32,
                ChoiceClass::I64,
                ChoiceClass::I32,
            ],
            "three array leaves, then the struct's i32/i64 fields, then the bool"
        );
        assert_eq!(
            runs(&plan),
            vec![
                ChoiceRun::Leaves { first: 0, len: 3 },
                ChoiceRun::Leaves { first: 3, len: 2 },
                ChoiceRun::Scalar(5),
            ]
        );
        assert!(plan.covers_every_uzumaki);
    }

    #[test]
    fn a_multidimensional_array_expands_to_every_leaf() {
        let plan = sole_plan(
            "spec S {
              fn f() forall {
                let a: [[i64; 2]; 3] = @;
                assert(a[0][0] >= a[0][0]);
              }
            }",
        );
        assert_eq!(classes(&plan), vec![ChoiceClass::I64; 6]);
    }

    /// A compound `@` in a reachability body stays unplanned — its obligation
    /// would have to denote linear memory — and that clears the coverage flag.
    #[test]
    fn a_compound_uzumaki_in_a_reach_body_is_not_planned() {
        let plan = sole_plan(
            "spec S {
              fn f() exists {
                let n: i32 = @;
                let arr: [i32; 2] = @;
                assert(n >= n);
              }
            }",
        );
        assert_eq!(plan.params.len(), 1);
        assert!(!plan.covers_every_uzumaki);
    }

    #[test]
    fn a_return_statement_is_recorded_without_rejection() {
        let plan = sole_plan(
            "spec S {
              fn f() -> i32 forall {
                let n: i32 = @;
                assert(n >= n);
                return 0;
              }
            }",
        );
        assert!(plan.has_return, "a universal body may legally return");
        assert_eq!(plan.contract, FrameContract::Free);
    }
}
