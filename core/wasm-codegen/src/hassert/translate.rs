//! The specification-body-to-`hassert` translation itself.
//!
//! One [`SpecFnTranslator`] per specification function walks its typed AST and
//! produces a single [`HAssert`] obligation. The scheme is a right-folded
//! statement translator with four modes ([`Mode::Univ`]/[`Mode::UnivLvl`]/
//! [`Mode::Exist`]/[`Mode::Reach`]) and a small term translator that mirrors the
//! WASM operators code generation emits for the same expressions, so the
//! obligation speaks the same numeric language as the compiled body it
//! constrains.
//!
//! ## Alternating quantifiers need a real universal binder
//!
//! A `forall` block nested inside an existential context — an `exists` block, an
//! `assume` block, or a `forall`-kinded `if` branch of one — states a universal
//! claim *under* an existential one. Its statements read universally, but its
//! `@`s cannot take quantifier slots: the downstream judgment quantifies every
//! slot a payload reads, from outside the whole obligation, so a slot here would
//! be bound outside the enclosing `HA_ex` and `∃x. ∀y. P` would silently become
//! `∀y. ∃x. P` — a different and weaker property, and the reason this nesting
//! was rejected before there was a binder to express it.
//!
//! [`Mode::UnivLvl`] is that combination: universal statement semantics on the
//! level channel, wrapped in [`HAssert::All`]. Its typing guards name
//! [`HTerm::LVar`]s, which is why they must be discharged inside their own
//! binder — a `T_lvar` written outside its quantifier names nothing, and the
//! downstream strictification would collapse the guard rather than reject it,
//! quietly hardening the obligation. Both channels that carry a guard keep it
//! inside: the named form pushes it through the guard channel, whose `all` wrap
//! encloses the translation that drains it, while the anonymous call-argument
//! form carries it on the binder itself.
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
//! ## An aggregate is its ordered scalar leaves
//!
//! There is no aggregate *term*. An aggregate introduction — a compound `@`, a
//! compound parameter, an array or struct literal, a copy of one — is
//! translated to a shape-preserving tree ([`AggValue`]) whose leaves are
//! ordinary scalar terms, and every field or constant-index read selects a
//! child of that tree at translation time. Nothing about the aggregate survives
//! into the obligation except the leaves the claim actually mentions, and the
//! guards over all of them.
//!
//! **Enumeration order** is arrays row-major and struct fields in
//! `compute_struct_field_layout` order, recursing — the order the runtime
//! unrolling of a compound `@` uses, over the shapes both sides support
//! (arrays of scalars at any rank; structs whose fields are scalars or
//! one-dimensional scalar arrays). **Allocation order** is parameters first in
//! declaration order, then each `@` in binding order, one slot per scalar leaf.
//! Those two rules together fix every `T_local` index in the emitted payload,
//! and the indices are user-visible the moment a proof fails, so they are part
//! of the contract rather than an implementation detail.
//!
//! In a universal payload a guard is pushed for *every* leaf, including leaves
//! the claim never reads. The guards are antecedents, so extra ones only weaken
//! the obligation, and uniformity beats a use analysis that would make the slot
//! numbering depend on what the body happens to mention. The cost is real and
//! worth knowing: an N-leaf aggregate puts N guards in front of every
//! obligation of its function. An existential leaf carries no guard — the
//! prover chooses the value, so there is no unconstrained valuation to
//! constrain — and a literal's leaves are constants that neither bind nor
//! guard.
//!
//! A rejected introduction still advances the slot counter — one slot for a
//! refused non-scalar parameter, the full leaf count for an aggregate over the
//! budget — so a diagnostic never shifts the slot numbers of the declarations
//! after it.
//!
//! For `fn f() forall { let a: [i32; 3] = @; assert(a[0] <= a[2]); }` that is
//!
//! ```text
//! Himpl (HA_has_type (T_local 0) T_i32 ∧
//!        HA_has_type (T_local 1) T_i32 ∧
//!        HA_has_type (T_local 2) T_i32)
//!       (T_relop ROI_le (T_local 0) (T_local 2) ≠ 0)
//! ```
//!
//! — three slots for the three leaves, guarded together because the guards drain
//! at the first structural statement, and the claim reading slots 0 and 2 while
//! slot 1 is quantified and unread.
//!
//! (Emitted output is one line; the layout above is this comment's.)
//!
//! The size of that expansion is what [`SPEC_FN_MAX_QUANTIFIED_LEAVES`] bounds,
//! and it is a per-function running total rather than a per-introduction cap,
//! because the levels of every introduction — guards here, `All` and `Ex`
//! binders elsewhere — nest into one chain.
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
//!
//! ## A non-constant index defines its element by cases
//!
//! An aggregate is translated to its ordered scalar leaves, so a *constant*
//! index resolves at translation time — the access is simply that leaf's term.
//! A non-constant index names an element the translation cannot pick, so it
//! borrows the witness machinery one more time: `a[i]` over an `N`-element
//! array is a fresh binder `v` pinned by
//!
//! ```text
//! (i <u N) ∧ ⋀_{c<N} Himpl (i = c) (v = leaf c)
//! ```
//!
//! The range bound is the **first** conjunct — a reader of a failing goal
//! should meet it before the case split — and it is a single *unsigned*
//! comparison, under which a negative index is a huge value, so no lower bound
//! is missing rather than merely omitted.
//!
//! Out of range the definition is unsatisfiable and the enclosing atom is
//! refuted. That is a definedness rule, not a mirror of any runtime check
//! (proof-mode modules emit no bounds check at all): `a[i]` denotes *the
//! element at index `i`, which exists*, and the constraint saying which element
//! that is exists only where `i` is in range. The alternative — a guarded
//! implication that leaves an out-of-range read vacuously satisfied — would
//! hand back a proof saying nothing about most values of `i`, which is the very
//! thing [`PCode::P010`] rejects elsewhere.
//!
//! Constant steps of an access chain descend eagerly, so `m[1][j]` is a
//! symbolic read of one already-selected row. A chain with two non-constant
//! steps is rejected: the split would be their product, and one obligation
//! carries one such split per chain.

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, ExprId, IdentId, StmtId, TypeId};
use inference_ast::nodes::{
    ArgKind, BlockKind, Def, Expr, Location, OperatorKind, Stmt, UnaryOperatorKind,
};
use inference_fn_key::{FnKey, merged_name};
use inference_hassert::{HAssert, HBinop, HConst, HFnRef, HNumType, HRelop, HTerm};
use inference_type_checker::type_info::{NumberType, TypeInfo, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;
use rustc_hash::FxHashMap;

use super::diag::{HassertDiagnostic, PCode};
use super::reach::ChoicePlan;
use super::{CalleeIndex, ExternIndex};

/// Polarity of the surrounding quantification.
///
/// A mode answers two independent questions, which is why the predicates below
/// exist rather than bare equality tests: how *statements* read (`assume`, `if`
/// and `==` are universal or existential), and which *channel* a `@` binds
/// through (a `T_local` slot, a `T_lvar` level, or the frame slot of a planned
/// choice parameter). Neither answer decides whether an `HA_ex` binder can
/// appear: a short-circuit witness is bound in every mode.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Universal context: `assume` filters (antecedent), `if` is a
    /// conjunction of guarded implications, `@` takes a `T_local` slot.
    Univ,
    /// Universal context nested inside an existential one — a `forall` block
    /// under an `exists`/`assume` block. Statements read exactly as under
    /// [`Mode::Univ`], but a `@` binds a *logical variable* wrapped in
    /// [`HAssert::All`] rather than a slot: the enclosing existential already
    /// binds levels, and a free slot here would be quantified by the outer
    /// judgment instead, silently turning `∃x. ∀y` into `∀y. ∃x`.
    UnivLvl,
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

impl Mode {
    /// Whether statements read universally: an `assume` block becomes the
    /// antecedent of what follows, an `if` a conjunction of guarded
    /// implications, an `==` the refutable `nz (T_relop … eq …)` rather than a
    /// witness equation. This is also the axis the typing-guard channel follows
    /// — only a universal payload states the typing of the variables it reads,
    /// because only it is evaluated over valuations nothing constrains.
    fn is_universal(self) -> bool {
        matches!(self, Mode::Univ | Mode::UnivLvl)
    }

    /// Whether a `@` reads the frame slot of the choice parameter the pre-scan
    /// planned for it instead of introducing a quantified variable of its own.
    fn binds_choice_slots(self) -> bool {
        matches!(self, Mode::Reach)
    }

    /// Whether a quantified introduction consumes fresh `T_local` slots from
    /// this function's own counter. True only for [`Mode::Univ`]: the nested
    /// universal mode binds levels, and a reachability `@` reads a slot the
    /// pre-scan already assigned rather than allocating one.
    fn allocates_slots(self) -> bool {
        matches!(self, Mode::Univ)
    }
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
    /// An aggregate value — a compound `@`, a compound parameter, an
    /// array/struct literal, or a copy of one — held as its leaf tree.
    Aggregate(AggValue),
}

/// Cumulative cap on the quantified scalar leaves one specification function
/// may introduce, shared by every aggregate introduction (compound `@`s,
/// compound parameters, and array/struct literals) — [`PCode::P013`] past it.
///
/// The resource the cap protects is the assertion-tree depth budget
/// (`inference_hassert::MAX_TREE_DEPTH`, measured at 255 usable levels): slot
/// guards nest right-associated one level per leaf and accumulate across *all*
/// introductions until the first structural statement, so a per-introduction
/// cap would not bound the resource it names — four introductions of 64 leaves
/// each would still overrun the encoder. 64 as a per-function total is safe
/// with roughly half the measured ceiling to spare for the obligation itself,
/// in universal and existential positions alike. The check must run against
/// the *type* before any leaf is materialized: the translator keeps going
/// after a diagnostic, and a materialized many-thousand-node conjunction
/// overflows the stack in its derived `Drop` — the exact hazard the codec's
/// `MAX_TREE_DEPTH` documentation names.
const SPEC_FN_MAX_QUANTIFIED_LEAVES: u32 = 64;

/// A translated aggregate value: the shape-preserving tree whose leaves are
/// ordinary scalar terms. Arrays hold children in index order (row-major
/// overall); structs hold `(field_name, child)` in field-layout order — the
/// declaration order `compute_struct_field_layout` also lays fields out in,
/// and the same order the runtime `@`-unrolling enumerates for the shapes
/// both sides support.
#[derive(Clone, Debug)]
enum AggValue {
    /// A scalar leaf's term. The leaf's numeric class lives on the
    /// [`AggShape`] the introduction materialized from — no reader of a built
    /// value consumes it, so the value tree does not carry it.
    Scalar(HTerm),
    /// An array, children in index order.
    Array(Vec<AggValue>),
    /// A struct, `(field_name, child)` in field-layout order.
    Struct(Vec<(String, AggValue)>),
    /// Bound in place of a rejected aggregate (out-of-surface shape, leaf
    /// budget, or a rejected introduction mode): any read at any path resolves
    /// to a silent zero sentinel without a second diagnostic — one mistake,
    /// one message.
    Sentinel,
}

impl AggValue {
    /// Appends this value's scalar leaf terms in enumeration order. Returns
    /// `false` — leaving `out` unreliable — if a [`AggValue::Sentinel`] is
    /// anywhere in the tree, so the caller can fall silent instead of building
    /// a claim over an already-diagnosed value.
    fn collect_leaves(&self, out: &mut Vec<HTerm>) -> bool {
        match self {
            AggValue::Scalar(term) => {
                out.push(term.clone());
                true
            }
            AggValue::Array(children) => children.iter().all(|child| child.collect_leaves(out)),
            AggValue::Struct(fields) => fields.iter().all(|(_, child)| child.collect_leaves(out)),
            AggValue::Sentinel => false,
        }
    }
}

/// One step of an access chain, carrying the access expression it was written
/// as so a rejection points at the step the author can change.
#[derive(Clone, Copy)]
enum AccessStep {
    /// `.name`, read against a struct value.
    Field { at: ExprId, name: IdentId },
    /// `[index]`, read against an array value. Whether the index is constant
    /// is decided when the step is walked, not when the chain is split.
    Index { at: ExprId, index: ExprId },
}

/// What an access chain has resolved to partway through.
///
/// A chain carries one value until a non-constant index is met; from there it
/// carries one candidate per element of the indexed array, and every remaining
/// step applies to all of them. The candidates are siblings of a single
/// element type, so each later step resolves identically for each — which is
/// what lets the step decision be taken once, and a bad step report once.
enum ChainValue {
    /// No non-constant index yet.
    One(AggValue),
    /// One non-constant index taken: its translated term, the numeric class
    /// that term rides in, and the candidates in index order.
    Split {
        index: HTerm,
        class: HNumType,
        candidates: Vec<AggValue>,
    },
}

impl ChainValue {
    /// The candidate every step decision is taken against.
    fn sample(&self) -> &AggValue {
        match self {
            ChainValue::One(value) => value,
            ChainValue::Split { candidates, .. } => candidates
                .first()
                .expect("a split carries the elements of a positive-length array"),
        }
    }

    /// Applies a decided step to every candidate, keeping the split.
    fn map(self, select: impl Fn(&AggValue) -> AggValue) -> ChainValue {
        match self {
            ChainValue::One(value) => ChainValue::One(select(&value)),
            ChainValue::Split {
                index,
                class,
                candidates,
            } => ChainValue::Split {
                index,
                class,
                candidates: candidates.iter().map(select).collect(),
            },
        }
    }
}

/// The leaf skeleton of a *supported* aggregate type. The supported surface
/// equals the executable `@` surface (spec aggregate support must not exceed
/// it while proof mode lowers spec bodies): scalar arrays of any rank, and
/// flat structs whose fields are scalars or one-dimensional scalar arrays —
/// exactly the boundary analysis rules A027/A028 keep for the executable
/// unrolling. Out-of-surface shapes (arrays of structs, structs with struct
/// or multidimensional-array fields) never get a shape and keep their
/// pre-existing rejections.
///
/// A shape is the unit the leaf budget is checked against: its leaf count is a
/// product over the type's structure, computable without materializing a
/// single leaf.
enum AggShape {
    /// A scalar leaf at its numeric class.
    Scalar(HNumType),
    /// An array of `u32` children of one shape.
    Array(Box<AggShape>, u32),
    /// A struct, `(field_name, field_shape)` in field-layout order.
    Struct(Vec<(String, AggShape)>),
}

impl AggShape {
    /// The number of scalar leaves, saturating — the budget check only needs
    /// "over the cap", and the cap is far below the saturation point.
    fn leaf_count(&self) -> u32 {
        match self {
            AggShape::Scalar(_) => 1,
            AggShape::Array(elem, len) => elem.leaf_count().saturating_mul(*len),
            AggShape::Struct(fields) => fields
                .iter()
                .fold(0u32, |acc, (_, f)| acc.saturating_add(f.leaf_count())),
        }
    }
}

/// A callee a specification claim can name, resolved to the symbol its
/// application carries and the declaration whose signature classifies its
/// result.
///
/// The two cases differ in where the body comes from — this module's own
/// compilation, or a static merge that has not happened yet — and therefore in
/// who names it, which is why the symbol travels rather than being re-derived
/// at the application site.
enum Callee {
    /// A function compiled from this program's source, named by the identity
    /// code generation registered it under.
    Defined { key: FnKey, def_id: DefId },
    /// A bound `external fn`, named by the symbol the static-merge linker gives
    /// its merged body.
    External { symbol: String, decl: DefId },
}

impl Callee {
    /// The name-section symbol the application references.
    fn symbol(self) -> String {
        match self {
            Callee::Defined { key, .. } => key.to_string(),
            Callee::External { symbol, .. } => symbol,
        }
    }

    /// The declaration whose declared return type classifies the result.
    fn def_id(&self) -> DefId {
        match self {
            Callee::Defined { def_id, .. } => *def_id,
            Callee::External { decl, .. } => *decl,
        }
    }
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

/// Which quantifier wraps a pending binder, and therefore how its definition
/// attaches to the body it is wrapped around.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Binder {
    /// `∃v. (definition ∧ body)` — a short-circuit witness, an element pinned
    /// by a non-constant index, or a call-argument `@` in an existential
    /// context. The definition *pins* the variable, so it is a conjunct.
    Ex,
    /// `∀v. (definition → body)` — a call-argument `@` under
    /// [`Mode::UnivLvl`]. Nothing pins a universally quantified value; its
    /// definition is the typing guard its readers depend on, which is an
    /// assumption rather than a claim, so it enters as an antecedent. This is
    /// also what keeps such a guard *inside* its own binder: a `T_lvar` guard
    /// that escaped its quantifier would name a variable nothing binds.
    All,
}

/// One binder allocated but not yet wrapped around the assertion that reads it.
struct PendingBinder {
    quant: Binder,
    definition: HAssert,
}

/// Pending binders taken off the stack as one group, ready to be wrapped.
///
/// The level of the group's first binder travels with the binders because
/// wrapping needs both: the definitions become the quantifier bodies, and the
/// levels name the variables whose occurrence decides which definitions survive.
/// Deriving the level at the split — rather than reading `depth` at the wrap —
/// is what keeps a group correct when binders allocated before it are left
/// behind for an enclosing wrap.
struct PendingGroup {
    /// One entry per binder, in allocation order; the first is outermost.
    binders: Vec<PendingBinder>,
    /// The absolute level of the first binder in `binders`.
    base_level: u32,
}

pub(super) struct SpecFnTranslator<'a> {
    arena: &'a AstArena,
    ctx: &'a TypedContext,
    module_path: &'a [String],
    spec_name: &'a str,
    callee: &'a CalleeIndex,
    externs: &'a ExternIndex,
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
    /// A binder nothing pins carries [`HAssert::True`] — an existential
    /// call-argument `@`, which the prover chooses freely, or a witness whose
    /// constraint moved into a conditional arm. Every allocation site drains
    /// its own binders around its own statement, so this is empty at every
    /// statement boundary.
    pending: Vec<PendingBinder>,
    /// Typing guards for the universal slots introduced since the last
    /// structural statement, in introduction order, awaiting their drain.
    univ_guards: Vec<HAssert>,
    /// Running total of quantified scalar leaves this function's aggregate
    /// introductions have materialized, checked against
    /// [`SPEC_FN_MAX_QUANTIFIED_LEAVES`] *before* each introduction
    /// materializes anything. Scalar introductions are deliberately uncounted:
    /// the cap targets the aggregate amplification, and many-scalar bodies
    /// keep their pre-existing encoder backstop.
    leaves_introduced: u32,
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
        externs: &'a ExternIndex,
    ) -> Self {
        Self {
            arena: ctx.arena(),
            ctx,
            module_path,
            spec_name,
            callee,
            externs,
            slots: 0,
            depth: 0,
            pending: Vec::new(),
            univ_guards: Vec::new(),
            leaves_introduced: 0,
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

    /// Binds each parameter in declaration order: a scalar to its slot, a
    /// supported-shape aggregate to its leaf slots.
    fn bind_parameters(&mut self, args: &[inference_ast::nodes::ArgData], mode: Mode) {
        for arg in args {
            match &arg.kind {
                ArgKind::Named { name, ty, .. } => {
                    let param_name = self.arena[*name].name.clone();
                    let binding = self.parameter_binding(arg.location, *ty, mode, &param_name);
                    self.env.insert(param_name, binding);
                }
                ArgKind::Ignored { ty } => {
                    let _ = self.parameter_binding(arg.location, *ty, mode, "_");
                }
                ArgKind::SelfRef { .. } | ArgKind::TypeOnly(_) => {}
            }
        }
    }

    /// Consumes the slots a parameter occupies and, under universal
    /// quantification, records the typing guards its readers depend on. An
    /// ignored parameter is guarded like a named one: the guard is inert for a
    /// slot the payload never reads, and uniformity beats a use analysis.
    ///
    /// A scalar takes one slot. A supported-shape aggregate (scalar arrays of
    /// any rank; flat structs with scalar or 1-D scalar-array fields) takes
    /// one slot and one guard per scalar leaf in a universal payload —
    /// `ValidSpec` evaluates the payload over unconstrained valuations, so
    /// leaf slots are free names rather than frame locals, and the leaf
    /// expansion deliberately abandons the one-pointer-local coincidence the
    /// compiled function keeps. An out-of-surface aggregate, and any
    /// non-scalar in [`Mode::Reach`], stays [`PCode::P004`]; its single slot
    /// is still consumed so later slot numbers stay aligned with the source
    /// (in reachability mode this alignment is load-bearing, not merely tidy:
    /// the downstream judgment reads choices at `entry_arity + k`, so every
    /// declared parameter must keep costing exactly one payload slot), and an
    /// aggregate binds a sentinel so one mistake yields one message.
    ///
    /// A reachability payload pushes no guard for any slot: it denotes against
    /// the frame an actual execution reaches, where every slot already carries
    /// its runtime type, so a stated typing would be dead weight the downstream
    /// exemplars do not carry.
    fn parameter_binding(
        &mut self,
        location: Location,
        ty: TypeId,
        mode: Mode,
        name: &str,
    ) -> Binding {
        if self.type_is_scalar(ty) {
            let slot = self.next_slot();
            if mode.is_universal() {
                let width = self.declared_class(ty);
                self.push_univ_guard(slot, width);
            }
            return Binding::Slot(slot);
        }
        if !mode.binds_choice_slots()
            && let Some(shape) = self.agg_shape_of_type(ty)
        {
            let count = shape.leaf_count();
            if self.leaf_budget_exceeded(count) {
                self.error(
                    PCode::P013,
                    location,
                    format!(
                        "parameter `{name}` of type `{}` contributes {count} scalar leaves, and \
                         this specification already quantifies {} of the \
                         {SPEC_FN_MAX_QUANTIFIED_LEAVES} one function may hold: every leaf \
                         becomes its own quantified variable with its own typing guard, and the \
                         assertion encoding caps how deeply one obligation may nest; take the \
                         components the property reads as scalar parameters instead",
                        TypeInfo::from_type_id(self.arena, ty),
                        self.leaves_introduced,
                    ),
                );
                // Counter arithmetic only — nothing is materialized, so later
                // slot numbers stay source-aligned without a guard chain whose
                // very depth is the problem being reported.
                self.slots += count;
                return Binding::Aggregate(AggValue::Sentinel);
            }
            self.leaves_introduced += count;
            let value = match mode {
                Mode::Univ => self.univ_agg_value(&shape),
                Mode::UnivLvl | Mode::Exist | Mode::Reach => unreachable!(
                    "parameters bind at function entry, whose only modes are universal and \
                     reachability — the nested universal and existential modes are entered by a \
                     block, which has no parameters"
                ),
            };
            return Binding::Aggregate(value);
        }
        let aggregate = self.type_is_aggregate(ty);
        let message = if aggregate && mode.binds_choice_slots() {
            self.reach_parameter_message(name, ty)
        } else {
            self.non_scalar_message(ty)
        };
        self.error(PCode::P004, location, message);
        let slot = self.next_slot();
        if aggregate {
            Binding::Aggregate(AggValue::Sentinel)
        } else {
            Binding::Slot(slot)
        }
    }

    /// The reachability-body [`PCode::P004`] message for a compound parameter.
    /// A universal payload leaf-expands the same declaration, so the wording
    /// names the quantifier rather than the type: what rules it out here is
    /// that the obligation denotes against a frame an actual run reaches, in
    /// which the parameter is one pointer local.
    fn reach_parameter_message(&self, name: &str, ty: TypeId) -> String {
        let kind = self.reach_kind();
        let article = quantifier_article(kind);
        let rendered = TypeInfo::from_type_id(self.arena, ty).to_string();
        format!(
            "parameter `{name}` of type `{rendered}` cannot appear in {article} \
             `{kind}`-quantified spec function: its obligation denotes against the frame an \
             actual run reaches, where a `{rendered}` value is one pointer local; take the \
             scalar components as separate parameters, or state the property in a \
             `forall`-bodied spec function, where a compound parameter quantifies one variable \
             per scalar leaf"
        )
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
                    "reassignment has no place in a specification body: a specification names \
                     values, not storage — every name stands for one value throughout the \
                     claim, which is what lets the translation read a name as the same term \
                     wherever it appears; bind a new `let` for the new value"
                        .to_string(),
                );
                self.t_stmts(rest, mode)
            }
            Stmt::Loop { .. } => {
                self.error(
                    PCode::P002,
                    self.arena[stmt_id].location,
                    "`loop` has no encoding in the verification assertion language: a loop \
                     states a property only through an invariant this translation cannot \
                     infer, while a specification states the same property directly by \
                     quantifying; to say something about every element of an aggregate, bind \
                     it with `@`, bind an index with `@`, constrain the index in an `assume` \
                     block, and assert the property at that index; to compute a value, move \
                     the computation into an executable function the spec calls"
                        .to_string(),
                );
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
            mode.is_universal() || antecedent == HAssert::True,
            "typing guards pend only under universal quantification: existential translation \
             pins its variables with defining constraints instead, and reachability translation \
             deliberately pushes no guard for any slot (its payload denotes against the real \
             reached frame), so none can be pending in either mode"
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

    /// Records the typing a newly-introduced universal *logical variable*
    /// depends on ([`Mode::UnivLvl`]).
    ///
    /// The guard travels the same channel as a slot's, but unlike a `T_local` —
    /// which names something wherever it is written — a `T_lvar` is meaningful
    /// only inside the binder that introduced it. Every caller must therefore
    /// keep the drain within its own [`HAssert::all`] wrap, which the
    /// statement-list translation does by construction: the wrap encloses the
    /// translation of the rest of the block, and that is where the drain
    /// happens.
    fn push_lvar_guard(&mut self, level: u32, width: HNumType) {
        self.univ_guards
            .push(HAssert::HasType(HTerm::LVar(level), width));
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
            return self.t_uzumaki_let(name, ty, value_expr, rest, mode);
        }

        // Pure aggregate `let`: the right-hand side is a value tree (a
        // literal, a copy of another aggregate, or a field/element read of
        // one), bound whole. Value-copy semantics make the pure inlining
        // exact, so a copy is a clone of the bound tree.
        if self.type_is_aggregate(ty) {
            let base = self.pending.len();
            let value = if self.agg_shape_of_type(ty).is_none() {
                // An out-of-surface declared type keeps its pre-existing
                // right-hand-side diagnostics; the sentinel keeps its reads
                // from stacking further messages onto the same mistake.
                let _ = self.term(value_expr, mode);
                AggValue::Sentinel
            } else {
                self.agg_value(value_expr, mode)
            };
            let group = self.split_pending(base);
            self.env.insert(name, Binding::Aggregate(value));
            return self.scoped_over_rest(group, rest, mode);
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

    /// `let x: T = @;` — a scalar binds one slot (universal), one existential
    /// binder, or its choice parameter (reachability); a compound type
    /// dispatches to the leaf expansion.
    fn t_uzumaki_let(
        &mut self,
        name: String,
        ty: TypeId,
        value_expr: ExprId,
        rest: &[StmtId],
        mode: Mode,
    ) -> HAssert {
        if self.type_is_scalar(ty) {
            return match mode {
                Mode::Univ => {
                    let slot = self.next_slot();
                    let width = self.declared_class(ty);
                    self.push_univ_guard(slot, width);
                    self.env.insert(name, Binding::Slot(slot));
                    self.t_stmts(rest, Mode::Univ)
                }
                Mode::UnivLvl => {
                    let level = self.depth;
                    let width = self.declared_class(ty);
                    self.push_lvar_guard(level, width);
                    self.env.insert(name, Binding::Level(level));
                    self.depth += 1;
                    let body = self.t_stmts(rest, Mode::UnivLvl);
                    self.depth -= 1;
                    HAssert::all(body)
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
                    // compiled body binds — no binder, no guard.
                    let slot = self.choice_slot(value_expr);
                    self.env.insert(name, Binding::Slot(slot));
                    self.t_stmts(rest, Mode::Reach)
                }
            };
        }
        self.t_compound_uzumaki_let(name, ty, value_expr, rest, mode)
    }

    /// A compound-type `let … = @;`. A supported shape quantifies one variable
    /// per scalar leaf (slots under universal quantification, nested `Hall`
    /// binders under nested-universal, nested `HA_ex` binders under
    /// existential), subject to the cumulative leaf budget.
    /// [`Mode::Reach`] keeps its rejection: there a `@` is an operationally
    /// existential choice parameter shared with the compiled body, arriving as
    /// one scalar parameter per choice, and an aggregate cannot be represented
    /// without a choice-plan and lowering redesign.
    fn t_compound_uzumaki_let(
        &mut self,
        name: String,
        ty: TypeId,
        value_expr: ExprId,
        rest: &[StmtId],
        mode: Mode,
    ) -> HAssert {
        if !mode.binds_choice_slots()
            && let Some(shape) = self.agg_shape_of_type(ty)
        {
            let count = shape.leaf_count();
            if self.leaf_budget_exceeded(count) {
                self.error(
                    PCode::P013,
                    self.arena[value_expr].location,
                    format!(
                        "uzumaki (@) over compound type `{}` quantifies {count} scalar leaves, \
                         and this specification already quantifies {} of the \
                         {SPEC_FN_MAX_QUANTIFIED_LEAVES} one function may hold: every leaf \
                         becomes its own quantified variable with its own typing guard, and the \
                         assertion encoding caps how deeply one obligation may nest; quantify a \
                         smaller aggregate, or only the components the property actually reads",
                        TypeInfo::from_type_id(self.arena, ty),
                        self.leaves_introduced,
                    ),
                );
                if mode.allocates_slots() {
                    // Counter-only slot advance: source alignment without
                    // materializing the guard chain being reported.
                    self.slots += count;
                }
                self.env
                    .insert(name, Binding::Aggregate(AggValue::Sentinel));
                return self.t_stmts(rest, mode);
            }
            self.leaves_introduced += count;
            return match mode {
                Mode::Univ => {
                    let value = self.univ_agg_value(&shape);
                    self.env.insert(name, Binding::Aggregate(value));
                    self.t_stmts(rest, Mode::Univ)
                }
                Mode::UnivLvl => {
                    let mut next_level = self.depth;
                    let value = self.univ_lvl_agg_value(&shape, &mut next_level);
                    self.env.insert(name, Binding::Aggregate(value));
                    self.depth += count;
                    let body = self.t_stmts(rest, Mode::UnivLvl);
                    self.depth -= count;
                    (0..count).fold(body, |acc, _| HAssert::all(acc))
                }
                Mode::Exist => {
                    let mut next_level = self.depth;
                    let value = exist_agg_value(&shape, &mut next_level);
                    self.env.insert(name, Binding::Aggregate(value));
                    self.depth += count;
                    let body = self.t_stmts(rest, Mode::Exist);
                    self.depth -= count;
                    (0..count).fold(body, |acc, _| HAssert::ex(acc))
                }
                Mode::Reach => unreachable!("excluded above"),
            };
        }
        self.t_rejected_compound_uzumaki_let(name, ty, value_expr, rest, mode)
    }

    /// The rejected half of a compound `let … = @;`: an out-of-surface compound,
    /// another non-scalar type, or any of them in reachability mode.
    ///
    /// The rejection is raised once and the binding still made, so the rest of
    /// the block translates against something: an aggregate gets a sentinel
    /// (its reads resolve silently, keeping one mistake to one message) and
    /// anything else gets the binding its mode would have given it, so later
    /// slot numbers and binder levels stay where the source puts them.
    fn t_rejected_compound_uzumaki_let(
        &mut self,
        name: String,
        ty: TypeId,
        value_expr: ExprId,
        rest: &[StmtId],
        mode: Mode,
    ) -> HAssert {
        self.emit_non_scalar_uzumaki(ty, self.arena[value_expr].location, mode);
        let aggregate = self.type_is_aggregate(ty);
        match mode {
            Mode::Univ => {
                let slot = self.next_slot();
                let binding = if aggregate {
                    Binding::Aggregate(AggValue::Sentinel)
                } else {
                    Binding::Slot(slot)
                };
                self.env.insert(name, binding);
                self.t_stmts(rest, Mode::Univ)
            }
            Mode::UnivLvl => {
                let level = self.depth;
                let binding = if aggregate {
                    Binding::Aggregate(AggValue::Sentinel)
                } else {
                    Binding::Level(level)
                };
                self.env.insert(name, binding);
                self.depth += 1;
                let body = self.t_stmts(rest, Mode::UnivLvl);
                self.depth -= 1;
                HAssert::all(body)
            }
            Mode::Exist => {
                let level = self.depth;
                let binding = if aggregate {
                    Binding::Aggregate(AggValue::Sentinel)
                } else {
                    Binding::Level(level)
                };
                self.env.insert(name, binding);
                self.depth += 1;
                let body = self.t_stmts(rest, Mode::Exist);
                self.depth -= 1;
                HAssert::ex(body)
            }
            Mode::Reach => {
                // A non-scalar `@` was never planned and already carries its
                // diagnostic above; an aggregate binds the sentinel so its
                // reads do not cascade.
                if aggregate {
                    self.env
                        .insert(name, Binding::Aggregate(AggValue::Sentinel));
                }
                self.t_stmts(rest, Mode::Reach)
            }
        }
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
        if group.binders.is_empty() {
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
        self.depth = group.base_level + level_count(group.binders.len());
        let body = f(self);
        self.depth = outer_depth;
        wrap_binders(body, group)
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
            Mode::Univ | Mode::UnivLvl => {
                let then_h = s.scoped_block(then_block, s.branch_mode(then_block, mode));
                if let Some(else_id) = else_block {
                    let else_h = s.scoped_block(else_id, s.branch_mode(else_id, mode));
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
                s.check_branch_forall(then_block, mode);
                let then_h = s.scoped_block(then_block, s.branch_mode(then_block, mode));
                if let Some(else_id) = else_block {
                    s.check_branch_forall(else_id, mode);
                    let else_h = s.scoped_block(else_id, s.branch_mode(else_id, mode));
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
        let nested_exist_mode = if mode.binds_choice_slots() {
            Mode::Reach
        } else {
            Mode::Exist
        };
        match kind {
            BlockKind::Assume => {
                let body = self.scoped_block(block_id, nested_exist_mode);
                match mode {
                    Mode::Univ | Mode::UnivLvl => {
                        let antecedent = self.drain_guards_over(body);
                        HAssert::imp(antecedent, self.t_stmts(rest, mode))
                    }
                    Mode::Exist | Mode::Reach => HAssert::and(body, self.t_stmts(rest, mode)),
                }
            }
            BlockKind::Regular => {
                let body = self.scoped_block(block_id, mode);
                self.t_structural(body, rest, mode)
            }
            BlockKind::Forall => {
                if mode.binds_choice_slots() {
                    self.error_reach_forall(self.arena[block_id].location);
                }
                // Under an existential context the nested block is the
                // alternation itself: its statements read universally while its
                // `@`s bind logical variables, so the claim comes back as a
                // `Hall` the enclosing `HA_ex` encloses — the one shape a free
                // slot could not express, since the outer judgment would
                // quantify it and swap the two quantifiers.
                let body = self.scoped_block(block_id, self.branch_mode(block_id, mode));
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
        let Def::Constant {
            name, value, ty, ..
        } = &self.arena[def_id].kind
        else {
            return self.t_stmts(rest, mode);
        };
        let (name, value, ty) = (*name, *value, *ty);
        let base = self.pending.len();
        // An aggregate `const` binds a value tree exactly like an aggregate
        // pure `let`.
        if self.type_is_aggregate(ty) {
            let agg = if self.agg_shape_of_type(ty).is_none() {
                let _ = self.term(value, mode);
                AggValue::Sentinel
            } else {
                self.agg_value(value, mode)
            };
            let group = self.split_pending(base);
            self.env
                .insert(self.arena[name].name.clone(), Binding::Aggregate(agg));
            return self.scoped_over_rest(group, rest, mode);
        }
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
            // A negated aggregate comparison flips to its leafwise dual; the
            // generic atom below would demand the aggregate as a single term.
            Expr::Binary {
                left,
                right,
                op: op @ (OperatorKind::Eq | OperatorKind::Ne),
            } if self.expr_is_aggregate(*left) || self.expr_is_aggregate(*right) => {
                let (left, right) = (*left, *right);
                let negated = matches!(op, OperatorKind::Eq);
                return self.aggregate_comparison(left, right, negated, mode);
            }
            _ => {}
        }
        // Atom: the strict positive zero-equality.
        HAssert::eqz(self.term(expr, mode))
    }

    /// A comparison in assertion position. An aggregate `==`/`!=` leaves
    /// first, through [`Self::aggregate_comparison`], which is leafwise
    /// `term_eq` in every mode; the mode dispatch below governs *scalar*
    /// comparison alone.
    ///
    /// Among scalar comparisons, `==` is the one operator whose encoding
    /// depends on the mode: strict `term_eq` on the existential and
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
    ///
    /// That leaves scalar `!=` in a relop where scalar `==` is in a `term_eq`,
    /// and the asymmetry is kept deliberately rather than by omission. A
    /// downstream proof pays for it — one that assumes `a[0] != 3` and concludes
    /// over a leafwise aggregate disequality has to bridge "the `ne` relop is
    /// nonzero" to "these values differ" — and matching `!=` to `term_eq` would
    /// remove that step.
    ///
    /// The rule that keeps it is **positional, not operator-wise**. The
    /// existential and reachability modes are the positions that *pin* a value:
    /// an existential `@`'s witness, and — since an `assume` body translates in
    /// [`Mode::Exist`] even inside a universal function — the antecedent of a
    /// universal claim. `term_eq` is the pinning form, so `==` takes it there
    /// and the refutable relop in a claim position, which is exactly why the
    /// same operator flips between the two. `!=` never pins anything: there is
    /// no value it names, only a computation whose result must be nonzero, so it
    /// stays with the operator the program executes, at that operator's own
    /// width and signedness. Encoding it as `¬term_eq` would state a
    /// disequality of mathematical values where the program compares two i32
    /// registers — the same class of divergence the eager `&&`/`||` term had.
    ///
    /// The aggregate arm above is not an exception to that rule but a different
    /// question answered by the language: `==` at aggregate type compares
    /// *values*, and an aggregate's value is its ordered scalar leaves, so it is
    /// leafwise in every mode. The compiled comparison of frame pointers is the
    /// side that must change, tracked separately.
    fn p_comparison(
        &mut self,
        left: ExprId,
        right: ExprId,
        op: &OperatorKind,
        mode: Mode,
    ) -> HAssert {
        if matches!(op, OperatorKind::Eq | OperatorKind::Ne)
            && (self.expr_is_aggregate(left) || self.expr_is_aggregate(right))
        {
            let negated = matches!(op, OperatorKind::Ne);
            return self.aggregate_comparison(left, right, negated, mode);
        }
        let (num_ty, unsigned) = self.operand_class(left);
        let ta = self.term(left, mode);
        let tb = self.term(right, mode);
        match op {
            OperatorKind::Eq => match mode {
                // Both universal modes take the refutable relop reading; only
                // the binder channel differs between them, and an `==` reads no
                // binder. Folding either into the existential arm would flip
                // the polarity of every equality in a universal claim.
                Mode::Univ | Mode::UnivLvl => HAssert::nz(relop(num_ty, HRelop::Eq, ta, tb)),
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
            Expr::ArrayIndexAccess { .. } | Expr::MemberAccess { .. } => {
                // A scalar read out of an aggregate: resolve the access chain
                // against the value tree and take the leaf's term.
                match self.agg_value(expr, mode) {
                    AggValue::Scalar(term) => term,
                    AggValue::Sentinel => zero_sentinel(),
                    AggValue::Array(_) | AggValue::Struct(_) => {
                        // An aggregate-valued access read in scalar term
                        // position — ill-typed source the checker rejects
                        // before translation; kept as a diagnostic rather
                        // than an invariant so a checker gap degrades softly.
                        self.error_non_scalar_expr(expr);
                        zero_sentinel()
                    }
                }
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
                    non_scalar_term_message(&"unit"),
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
                    non_scalar_term_message(&other),
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
                unknown_type_term_message(),
            );
            zero_sentinel()
        }
    }

    /// An identifier, resolved through the environment. A universal slot becomes
    /// `T_local`; an existential variable becomes `T_lvar` at its level (finalized
    /// later); a pure `let` inlines its stored term. An aggregate binding read
    /// whole in scalar term position is rejected — an aggregate is not a term —
    /// except for the sentinel of an already-rejected aggregate, which stays
    /// silent so one mistake yields one message.
    fn identifier(&mut self, expr: ExprId, ident_id: IdentId) -> HTerm {
        let name = &self.arena[ident_id].name;
        match self.env.get(name) {
            Some(Binding::Slot(n)) => HTerm::Local(*n),
            Some(Binding::Level(level)) => HTerm::LVar(*level),
            Some(Binding::Term(term) | Binding::Aggregate(AggValue::Scalar(term))) => term.clone(),
            Some(Binding::Aggregate(AggValue::Sentinel)) => zero_sentinel(),
            Some(Binding::Aggregate(_)) | None => {
                self.error_non_scalar_expr(expr);
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
            Ok(callee) => match self.result_class(call_expr, callee.def_id()) {
                ResultClass::Scalar => {
                    let symbol = callee.symbol();
                    let arg_terms = self.arg_terms(&args, mode);
                    HTerm::App(HFnRef(symbol), arg_terms)
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
            Ok(callee) => {
                let symbol = callee.symbol();
                let arg_terms = self.arg_terms(&args, mode);
                HAssert::AppOk(HFnRef(symbol), arg_terms)
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
    /// binder (universal or existential, per the context) to be wrapped around
    /// the enclosing statement, or — in a reachability body — the choice
    /// parameter the pre-scan planned for it. An anonymous `@` has no declared
    /// type, so its guard width comes from the type recorded for the argument.
    ///
    /// Unlike a short-circuit witness, this binder carries no defining
    /// constraint: `@` *is* the free choice, so pinning it to a value would be
    /// the opposite of what it means. A nested-universal binder still carries
    /// its typing guard, which assumes rather than pins.
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
            Mode::UnivLvl => {
                let width = self.expr_class(arg);
                self.bind_universal(width)
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

    /// Resolves a call's callee to the [`Callee`] its application names, or the
    /// [`CalleeError`] the call site raises.
    ///
    /// Mirrors code generation's resolution: an `external fn` visible in the
    /// enclosing scope resolves by declaration; a bare same-file call (including
    /// a spec-sibling helper) is resolved spec-first then by the current file's
    /// free key; a cross-file item import, a `::`-qualified free function, and an
    /// associated function use the type-checker-recorded target; an instance
    /// method has no term encoding.
    fn resolve_callee(&self, function: ExprId) -> Result<Callee, CalleeError> {
        match &self.arena[function].kind {
            Expr::Identifier(ident_id) => {
                let name = self.arena[*ident_id].name.clone();
                // Ahead of the defined-function arms, and safe there only
                // because the type checker rejects a spec-inner function that
                // shadows a top-level one of the same name — extern or not. A
                // name reaching this point is therefore an extern or a defined
                // function, never both, so the order decides nothing. If that
                // rule is ever relaxed, this must become a real innermost-first
                // walk over both kinds: a file-scope extern would otherwise
                // hide a spec-sibling function that shadows it.
                if let Some(decl) = self.externs.lookup(self.module_path, self.spec_name, &name) {
                    return self.resolve_external(decl);
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
                    "it does not resolve to a function this module defines or links",
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

    /// Confirms a `FnKey` names a function compiled from source and validates
    /// its body.
    fn validate_defined(&self, key: FnKey) -> Result<Callee, CalleeError> {
        match self.callee.get(&key) {
            Some(def_id) => self.validate_body(key, def_id),
            None => Err(CalleeError::NotApplicable(
                "it does not resolve to a function this module defines or links",
            )),
        }
    }

    /// Rejects a callee whose body contains non-deterministic constructs — it can
    /// carry no realized claim.
    fn validate_body(&self, key: FnKey, def_id: DefId) -> Result<Callee, CalleeError> {
        if self.arena.def_is_non_det(def_id) {
            return Err(CalleeError::NotApplicable(
                "its body is non-deterministic and has no executable meaning",
            ));
        }
        Ok(Callee::Defined { key, def_id })
    }

    /// Resolves an `external fn` declaration to the symbol its linked body
    /// carries, or rejects it when nothing binds the declaration to a module.
    ///
    /// A bound extern is a legitimate specification subject: the static merge
    /// splices its body into the emitted module, where the downstream
    /// realization obligation reduces it like any other. An *unbound* one is
    /// not — no module supplies a body, so an application of it would name a
    /// function the proof can never reach.
    fn resolve_external(&self, decl: DefId) -> Result<Callee, CalleeError> {
        let Some(origin) = self.ctx.extern_origin_by_decl(decl) else {
            return Err(CalleeError::NotApplicable(
                "it is an external function with no `use … from` binding, so no module supplies \
                 the body an obligation about it would reduce",
            ));
        };
        Ok(Callee::External {
            symbol: merged_name::root(&origin.logical_module, &origin.export_field),
            decl,
        })
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
            Def::Function { returns, .. } | Def::ExternFunction { returns, .. } => match returns {
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
        wrap_binders(atom, group)
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

    /// The translation mode of a nested block — an `if` branch or a block
    /// statement — from the block's own kind and the enclosing mode: an
    /// `exists` block translates existentially, a `forall` block inside an
    /// existential context translates in the nested universal mode, and every
    /// other block inherits the enclosing mode.
    ///
    /// A reachability body is the exception on both counts and keeps its own
    /// mode throughout: its `@`s are choice parameters the enclosing function
    /// already planned, so neither a fresh existential binder nor a universal
    /// one can name them.
    fn branch_mode(&self, block_id: BlockId, outer: Mode) -> Mode {
        if outer.binds_choice_slots() {
            return outer;
        }
        match self.arena[block_id].block_kind {
            BlockKind::Exists => Mode::Exist,
            BlockKind::Forall if outer == Mode::Exist => Mode::UnivLvl,
            _ => outer,
        }
    }

    /// Records [`PCode::P007`] when a `forall` `if` branch appears inside a
    /// reachability body, where a universal binder over an operationally
    /// quantified choice has no representation. Under an ordinary existential
    /// context the branch translates instead, in [`Mode::UnivLvl`].
    fn check_branch_forall(&mut self, block_id: BlockId, mode: Mode) {
        if mode.binds_choice_slots() && self.arena[block_id].block_kind == BlockKind::Forall {
            self.error_reach_forall(self.arena[block_id].location);
        }
    }

    /// The number class and signedness of an operand, read from its type exactly
    /// as `lower_binary_expression` reads the left operand's.
    fn operand_class(&self, expr: ExprId) -> (HNumType, bool) {
        let kind = self.ctx.get_node_typeinfo(node_expr(expr)).map(|t| t.kind);
        (num_class(kind.as_ref()), kind_is_unsigned(kind.as_ref()))
    }

    /// Whether an expression's recorded type is an unsigned number. An
    /// expression the checker left untyped reads signed, the same latitude
    /// [`Self::operand_class`] takes for its number class.
    fn expr_is_unsigned(&self, expr: ExprId) -> bool {
        kind_is_unsigned(
            self.ctx
                .get_node_typeinfo(node_expr(expr))
                .map(|t| t.kind)
                .as_ref(),
        )
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
        self.kind_is_scalar_in(
            &TypeInfo::from_type_id(self.arena, ty).kind,
            self.module_path,
        )
    }

    /// Whether a type kind is a scalar as referenced from the file whose
    /// module path is `module_path` — the resolution scope matters because a
    /// bare enum name resolves relative to its referencing file, and a struct
    /// *field's* type resolves relative to the struct's defining file.
    fn kind_is_scalar_in(&self, kind: &TypeInfoKind, module_path: &[String]) -> bool {
        match kind {
            TypeInfoKind::Bool | TypeInfoKind::Number(_) | TypeInfoKind::Enum(_, _) => true,
            TypeInfoKind::Custom(name) => self.ctx.lookup_enum_in(name, module_path).is_some(),
            TypeInfoKind::Qualified(path) => {
                let segments: Vec<String> = path.split("::").map(str::to_string).collect();
                self.ctx.qualified_path_is_enum(&segments, module_path)
            }
            _ => false,
        }
    }

    /// Whether a declared type spells an aggregate (an array or a struct),
    /// supported shape or not.
    fn type_is_aggregate(&self, ty: TypeId) -> bool {
        self.kind_is_aggregate(&TypeInfo::from_type_id(self.arena, ty).kind)
    }

    /// Whether an expression's recorded type is an aggregate.
    fn expr_is_aggregate(&self, expr: ExprId) -> bool {
        self.ctx
            .get_node_typeinfo(node_expr(expr))
            .is_some_and(|t| self.kind_is_aggregate(&t.kind))
    }

    /// Whether a type kind is an aggregate under any of its spellings —
    /// `Array`, resolved `Struct`, or a `Custom`/`Qualified` name that
    /// resolves to a struct. Spelling-total on purpose: a cross-module struct
    /// type must classify exactly like an unqualified one.
    fn kind_is_aggregate(&self, kind: &TypeInfoKind) -> bool {
        match kind {
            TypeInfoKind::Array(_, _) | TypeInfoKind::Struct(_, _) => true,
            _ => self.struct_info_of_kind(kind).is_some(),
        }
    }

    /// Resolves a type kind to the struct it names, across every spelling a
    /// struct type reaches this pass under: a resolved `Struct` by its
    /// canonical key, a bare `Custom` name relative to the current file, and a
    /// `::`-qualified path walked from the current file. A name that resolves
    /// to an enum is a scalar, never a struct.
    fn struct_info_of_kind(
        &self,
        kind: &TypeInfoKind,
    ) -> Option<inference_type_checker::StructInfo> {
        match kind {
            TypeInfoKind::Struct(_, key) => self.ctx.lookup_struct(key),
            TypeInfoKind::Custom(name) => {
                if self.ctx.lookup_enum_in(name, self.module_path).is_some() {
                    return None;
                }
                self.ctx.lookup_struct_in(name, self.module_path)
            }
            TypeInfoKind::Qualified(path) => {
                let segments: Vec<String> = path.split("::").map(str::to_string).collect();
                if self.ctx.qualified_path_is_enum(&segments, self.module_path) {
                    return None;
                }
                self.ctx
                    .lookup_struct_by_qualified_path(&segments, self.module_path)
            }
            _ => None,
        }
    }

    /// The leaf skeleton of a declared type, when it is in the supported
    /// aggregate surface — `None` for scalars and for out-of-surface shapes.
    fn agg_shape_of_type(&self, ty: TypeId) -> Option<AggShape> {
        self.agg_shape_of_kind(&TypeInfo::from_type_id(self.arena, ty).kind)
    }

    /// The leaf skeleton of a type kind, when it is in the supported aggregate
    /// surface: a scalar array of any rank, or a flat struct whose fields are
    /// scalars or 1-D scalar arrays — the same boundary analysis rules
    /// A027/A028 keep for the executable `@` unrolling, so the specification
    /// surface never exceeds the executable one.
    ///
    /// A zero-leaf shape (a field-less struct, a zero-length array) reports as
    /// out-of-surface rather than as an empty leaf list: the ⊤-absorbing
    /// constructors would silently collapse an empty introduction into a
    /// vacuous obligation, and A045 / the positive-array-length rule exclude
    /// those types from the executable surface anyway.
    fn agg_shape_of_kind(&self, kind: &TypeInfoKind) -> Option<AggShape> {
        let shape = if let TypeInfoKind::Array(elem, len) = kind {
            AggShape::Array(Box::new(self.scalar_array_shape(&elem.kind)?), *len)
        } else {
            let info = self.struct_info_of_kind(kind)?;
            self.flat_struct_shape(&info)?
        };
        (shape.leaf_count() > 0).then_some(shape)
    }

    /// The shape of a scalar array's element: scalar leaves at any rank; an
    /// element that is (or contains) a struct is out of the surface (A028).
    fn scalar_array_shape(&self, kind: &TypeInfoKind) -> Option<AggShape> {
        match kind {
            TypeInfoKind::Array(elem, len) => Some(AggShape::Array(
                Box::new(self.scalar_array_shape(&elem.kind)?),
                *len,
            )),
            _ if self.kind_is_scalar_in(kind, self.module_path) => {
                Some(AggShape::Scalar(num_class(Some(kind))))
            }
            _ => None,
        }
    }

    /// The shape of a flat struct: fields in declaration order — the order
    /// `compute_struct_field_layout` also lays them out in, and the order the
    /// runtime unrolling enumerates — each a scalar or a 1-D scalar array.
    /// Field types resolve relative to the struct's *defining* file, exactly
    /// as the layout computation resolves them (#63). A struct field or a
    /// multidimensional-array field puts the whole struct out of the surface
    /// (A027).
    fn flat_struct_shape(&self, info: &inference_type_checker::StructInfo) -> Option<AggShape> {
        let defining = self.ctx.module_path_of_scope(info.definition_scope_id);
        let mut fields = Vec::with_capacity(info.fields.len());
        for field in &info.fields {
            let kind = &field.type_info.kind;
            let shape = if self.kind_is_scalar_in(kind, &defining) {
                AggShape::Scalar(num_class(Some(kind)))
            } else if let TypeInfoKind::Array(elem, len) = kind
                && self.kind_is_scalar_in(&elem.kind, &defining)
            {
                AggShape::Array(
                    Box::new(AggShape::Scalar(num_class(Some(&elem.kind)))),
                    *len,
                )
            } else {
                return None;
            };
            fields.push((field.name.clone(), shape));
        }
        Some(AggShape::Struct(fields))
    }

    /// Whether admitting `count` more quantified leaves would overrun the
    /// per-function budget. Checked before anything is materialized.
    fn leaf_budget_exceeded(&self, count: u32) -> bool {
        debug_assert!(
            count > 0,
            "zero-leaf aggregates are out of the supported surface (A045 rejects field-less \
             structs as value types; array lengths are positive), so an introduction always \
             brings at least one leaf"
        );
        self.leaves_introduced.saturating_add(count) > SPEC_FN_MAX_QUANTIFIED_LEAVES
    }

    /// Materializes a universal aggregate introduction: one fresh slot and one
    /// typing guard per scalar leaf, in enumeration order.
    fn univ_agg_value(&mut self, shape: &AggShape) -> AggValue {
        match shape {
            AggShape::Scalar(width) => {
                let slot = self.next_slot();
                self.push_univ_guard(slot, *width);
                AggValue::Scalar(HTerm::Local(slot))
            }
            AggShape::Array(elem, len) => {
                AggValue::Array((0..*len).map(|_| self.univ_agg_value(elem)).collect())
            }
            AggShape::Struct(fields) => AggValue::Struct(
                fields
                    .iter()
                    .map(|(name, field)| (name.clone(), self.univ_agg_value(field)))
                    .collect(),
            ),
        }
    }

    /// Materializes a nested-universal aggregate introduction: consecutive
    /// absolute binder levels from `*next_level`, one per scalar leaf in
    /// enumeration order, each with the typing guard its readers depend on.
    ///
    /// The guards go through the pending channel, so the caller's
    /// [`HAssert::all`] wraps must enclose the translation that drains them.
    fn univ_lvl_agg_value(&mut self, shape: &AggShape, next_level: &mut u32) -> AggValue {
        match shape {
            AggShape::Scalar(width) => {
                let level = *next_level;
                *next_level += 1;
                self.push_lvar_guard(level, *width);
                AggValue::Scalar(HTerm::LVar(level))
            }
            AggShape::Array(elem, len) => AggValue::Array(
                (0..*len)
                    .map(|_| self.univ_lvl_agg_value(elem, next_level))
                    .collect(),
            ),
            AggShape::Struct(fields) => AggValue::Struct(
                fields
                    .iter()
                    .map(|(name, field)| (name.clone(), self.univ_lvl_agg_value(field, next_level)))
                    .collect(),
            ),
        }
    }

    /// Resolves an aggregate-position expression to its value tree: a bound
    /// aggregate, a field or constant-index read of one, or a literal. Any
    /// other expression keeps the diagnostics its scalar-position translation
    /// raises (a compound call result stays [`PCode::P005`], a string literal
    /// [`PCode::P002`], …) and resolves to the sentinel.
    fn agg_value(&mut self, expr: ExprId, mode: Mode) -> AggValue {
        match &self.arena[expr].kind {
            Expr::Parenthesized { expr: inner } => {
                let inner = *inner;
                self.agg_value(inner, mode)
            }
            Expr::Identifier(ident_id) => {
                let name = self.arena[*ident_id].name.clone();
                if let Some(Binding::Aggregate(value)) = self.env.get(&name) {
                    value.clone()
                } else {
                    self.error_non_scalar_expr(expr);
                    AggValue::Sentinel
                }
            }
            Expr::MemberAccess { .. } | Expr::ArrayIndexAccess { .. } => {
                self.access_chain(expr, mode)
            }
            Expr::ArrayLiteral { .. } | Expr::StructLiteral { .. } => {
                self.literal_introduction(expr, mode)
            }
            _ => {
                let _ = self.term(expr, mode);
                AggValue::Sentinel
            }
        }
    }

    /// Resolves a whole field/index access chain against the value tree its
    /// base resolves to.
    ///
    /// The chain is walked outward from the base rather than folded inward per
    /// node, because a non-constant index does not resolve to a value at the
    /// step that reads it: it selects among the elements, and the steps after
    /// it apply to every candidate before the case split names one. `m[i][0]`
    /// is therefore one split over the rows with `[0]` applied inside it, not
    /// a split whose result is then indexed again.
    fn access_chain(&mut self, expr: ExprId, mode: Mode) -> AggValue {
        let (base_expr, steps) = self.split_access_chain(expr);
        let mut current = ChainValue::One(self.agg_value(base_expr, mode));
        for step in &steps {
            match self.walk_step(current, step, mode) {
                Some(next) => current = next,
                None => return AggValue::Sentinel,
            }
        }
        match current {
            ChainValue::One(value) => value,
            ChainValue::Split {
                index,
                class,
                candidates,
            } => self.symbolic_element(expr, &index, class, &candidates),
        }
    }

    /// Peels an access chain into the expression it reads from and its steps,
    /// outermost last. Parentheses are transparent at every level.
    fn split_access_chain(&self, expr: ExprId) -> (ExprId, Vec<AccessStep>) {
        let mut steps = Vec::new();
        let mut current = expr;
        loop {
            match &self.arena[current].kind {
                Expr::Parenthesized { expr: inner } => current = *inner,
                Expr::MemberAccess { expr: base, name } => {
                    steps.push(AccessStep::Field {
                        at: current,
                        name: *name,
                    });
                    current = *base;
                }
                Expr::ArrayIndexAccess { array, index } => {
                    steps.push(AccessStep::Index {
                        at: current,
                        index: *index,
                    });
                    current = *array;
                }
                _ => break,
            }
        }
        steps.reverse();
        (current, steps)
    }

    /// Walks one step of a chain. `None` means the step was rejected and its
    /// diagnostic recorded, or that the chain already carries a sentinel.
    fn walk_step(
        &mut self,
        current: ChainValue,
        step: &AccessStep,
        mode: Mode,
    ) -> Option<ChainValue> {
        if matches!(current.sample(), AggValue::Sentinel) {
            // Already reported where the aggregate was introduced.
            return None;
        }
        match *step {
            AccessStep::Field { at, name } => {
                let field = self.arena[name].name.clone();
                let position = match current.sample() {
                    AggValue::Struct(fields) => fields.iter().position(|(n, _)| *n == field),
                    _ => None,
                };
                let Some(position) = position else {
                    // A field read of a non-struct, or a field the struct does
                    // not have — ill-typed source the checker rejects before
                    // translation, kept as a diagnostic so a checker gap
                    // degrades softly.
                    self.error_non_scalar_expr(at);
                    return None;
                };
                Some(current.map(|value| field_child(value, position)))
            }
            AccessStep::Index { at, index } => {
                let AggValue::Array(children) = current.sample() else {
                    self.error_non_scalar_expr(at);
                    return None;
                };
                let len = children.len();
                match self.fold_const_index(index) {
                    Some(k) => {
                        let position = usize::try_from(k).ok().filter(|p| *p < len);
                        let Some(position) = position else {
                            return self.reject_out_of_bounds(index, k, len);
                        };
                        Some(current.map(|value| element_child(value, position)))
                    }
                    None => self.split_at_symbolic_index(current, at, index, mode),
                }
            }
        }
    }

    /// A constant index outside the array: the same fact `A037` states for a
    /// direct-literal index, at the folded-constant path `A037`'s pattern
    /// cannot see — and the no-analysis code generation paths make this the
    /// only guard for either spelling.
    fn reject_out_of_bounds(&mut self, index: ExprId, k: i128, len: usize) -> Option<ChainValue> {
        self.error(
            PCode::P014,
            self.arena[index].location,
            format!(
                "array index {k} is out of bounds for array of length {len}; valid indices are \
                 0..{len}"
            ),
        );
        None
    }

    /// Takes the chain's one non-constant index step: the array's elements
    /// become the candidates a later case split chooses among.
    fn split_at_symbolic_index(
        &mut self,
        current: ChainValue,
        at: ExprId,
        index: ExprId,
        mode: Mode,
    ) -> Option<ChainValue> {
        let candidates = match current {
            ChainValue::One(AggValue::Array(candidates)) => candidates,
            ChainValue::One(_) => {
                unreachable!("the caller rejects an index step that does not read an array")
            }
            ChainValue::Split { .. } => {
                // A second non-constant index in one chain. The case split it
                // would need is the product of the two extents, which is the
                // only way one access can multiply an obligation's size — and
                // nothing in the supported idioms asks for it.
                self.error(
                    PCode::P002,
                    self.arena[at].location,
                    "an access chain with more than one non-constant index has no assertion \
                     encoding: each non-constant index defines the element by cases, and one \
                     obligation supports one such case split per chain; make all but one index \
                     constant, or assert over the constant-index elements directly"
                        .to_string(),
                );
                return None;
            }
        };
        // The index is translated once, before the element's binder is
        // allocated, so a witness the index expression itself introduces sits
        // outside the element's own.
        let class = num_class(
            self.ctx
                .get_node_typeinfo(node_expr(index))
                .map(|t| t.kind)
                .as_ref(),
        );
        let term = self.term(index, mode);
        Some(ChainValue::Split {
            index: term,
            class,
            candidates,
        })
    }

    /// Pins the element a non-constant index selects: a fresh binder defined
    /// by the index's range and one case per element.
    ///
    /// The range bound leads the definition — it is what a reader of a failing
    /// goal should meet first — and it is the single unsigned comparison
    /// `i <u N`: at unsigned width a negative index is a huge value, so there
    /// is no lower bound to add. An out-of-range index leaves the definition
    /// unsatisfiable, refuting the atom that reads the element; the element is
    /// defined only where it exists.
    fn symbolic_element(
        &mut self,
        chain: ExprId,
        index: &HTerm,
        class: HNumType,
        candidates: &[AggValue],
    ) -> AggValue {
        let mut leaves = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            match candidate {
                AggValue::Scalar(term) => leaves.push(term.clone()),
                AggValue::Sentinel => return AggValue::Sentinel,
                AggValue::Array(_) | AggValue::Struct(_) => {
                    // The chain does not end at a scalar: a case split pins
                    // one value, and an aggregate element would need one
                    // binder per leaf of the selected sub-tree.
                    self.error(
                        PCode::P002,
                        self.arena[chain].location,
                        "an access chain whose non-constant index selects an aggregate has no \
                         assertion encoding: only a scalar leaf can be named by cases, and every \
                         candidate element here would itself be an aggregate — there is no single \
                         term to define; index through to a scalar (`m[i][0]`), or make the index \
                         constant"
                            .to_string(),
                    );
                    return AggValue::Sentinel;
                }
            }
        }
        let extent = index_const(class, leaves.len());
        let range = HAssert::nz(relop(class, HRelop::LtU, index.clone(), extent));
        let index = index.clone();
        let element = self.bind_witness(move |v| {
            let cases = leaves
                .into_iter()
                .enumerate()
                .map(|(case, leaf)| {
                    HAssert::imp(
                        HAssert::TermEq(index.clone(), index_const(class, case)),
                        HAssert::TermEq(v.clone(), leaf),
                    )
                })
                .collect();
            HAssert::and(range, conjoin(cases))
        });
        AggValue::Scalar(element)
    }

    /// An array/struct literal in aggregate position: an introduction of
    /// scalar leaves, checked against the leaf budget from its recorded type
    /// *before* any child is translated. Out-of-surface literal shapes keep
    /// the pre-existing [`PCode::P002`] rejection.
    ///
    /// A literal's leaves are constants — they bind no variable and carry no
    /// typing guard — and still cost the full budget: a leafwise comparison
    /// nests one conjunct per leaf whichever side the leaf came from, and that
    /// nesting is what the budget bounds.
    fn literal_introduction(&mut self, expr: ExprId, mode: Mode) -> AggValue {
        let construct = match &self.arena[expr].kind {
            Expr::StructLiteral { .. } => "a struct literal",
            _ => "an array literal",
        };
        let Some(shape) = self
            .ctx
            .get_node_typeinfo(node_expr(expr))
            .and_then(|info| self.agg_shape_of_kind(&info.kind))
        else {
            // Its own call site rather than the shared no-encoding template:
            // a literal of a supported shape encodes now, so the template's
            // "has no encoding" would name a rule the language dropped, and
            // its "move the logic into an executable helper" remedy dead-ends
            // — a compound call result is `P005` and a compound argument
            // `P004`.
            self.error(
                PCode::P002,
                self.arena[expr].location,
                format!(
                    "{construct} of this shape has no assertion encoding: a literal becomes one \
                     term per scalar leaf, which reaches through arrays of scalars at any rank \
                     and structs whose fields are scalars or one-dimensional arrays of those, \
                     and no deeper; build the components you need as separate values"
                ),
            );
            return AggValue::Sentinel;
        };
        let count = shape.leaf_count();
        if self.leaf_budget_exceeded(count) {
            let rendered = self
                .ctx
                .get_node_typeinfo(node_expr(expr))
                .map_or_else(|| "this value".to_string(), |t| t.to_string());
            self.error(
                PCode::P013,
                self.arena[expr].location,
                format!(
                    "this `{rendered}` literal has {count} scalar leaves, and this \
                     specification already quantifies {} of the \
                     {SPEC_FN_MAX_QUANTIFIED_LEAVES} one function may hold: each leaf becomes a \
                     term of its own, a comparison against the value nests one conjunct per \
                     leaf, and the assertion encoding caps how deeply one obligation may nest; \
                     build a smaller value, or state the property over the elements it reads",
                    self.leaves_introduced,
                ),
            );
            return AggValue::Sentinel;
        }
        self.leaves_introduced += count;
        self.literal_value(expr, &shape, mode)
    }

    /// Builds a literal's value tree against its shape. Children are built in
    /// shape order — a struct literal's fields are reordered from source order
    /// to field-layout order, so one canonical leaf order holds everywhere;
    /// access is by name, so the reordering is unobservable.
    fn literal_value(&mut self, expr: ExprId, shape: &AggShape, mode: Mode) -> AggValue {
        match &self.arena[expr].kind {
            Expr::ArrayLiteral { elements } => {
                let elements = elements.clone();
                let AggShape::Array(elem_shape, len) = shape else {
                    self.error_non_scalar_expr(expr);
                    return AggValue::Sentinel;
                };
                debug_assert_eq!(
                    elements.len(),
                    *len as usize,
                    "a type-checked array literal has exactly its type's element count"
                );
                AggValue::Array(
                    elements
                        .iter()
                        .map(|element| self.literal_child(*element, elem_shape, mode))
                        .collect(),
                )
            }
            Expr::StructLiteral { fields, .. } => {
                let fields = fields.clone();
                let AggShape::Struct(field_shapes) = shape else {
                    self.error_non_scalar_expr(expr);
                    return AggValue::Sentinel;
                };
                let mut children = Vec::with_capacity(field_shapes.len());
                for (field_name, field_shape) in field_shapes {
                    let field_expr = fields
                        .iter()
                        .find(|(id, _)| self.arena[*id].name == *field_name)
                        .map_or_else(
                            || {
                                panic!(
                                    "struct literal reached hassert translation without field \
                                     `{field_name}` — the type checker rejects an incomplete \
                                     literal"
                                )
                            },
                            |&(_, field_expr)| field_expr,
                        );
                    children.push((
                        field_name.clone(),
                        self.literal_child(field_expr, field_shape, mode),
                    ));
                }
                AggValue::Struct(children)
            }
            _ => {
                self.error_non_scalar_expr(expr);
                AggValue::Sentinel
            }
        }
    }

    /// One literal child against its shape: a scalar child is an ordinary
    /// term (its witnesses pend as usual and scope over wherever the enclosing
    /// binding is read); a nested literal is part of the enclosing
    /// introduction, so it bypasses the budget re-count; anything else
    /// aggregate-shaped resolves as a value.
    fn literal_child(&mut self, expr: ExprId, shape: &AggShape, mode: Mode) -> AggValue {
        let mut child = expr;
        while let Expr::Parenthesized { expr: inner } = &self.arena[child].kind {
            child = *inner;
        }
        if let AggShape::Scalar(_) = shape {
            let term = self.term(child, mode);
            return AggValue::Scalar(term);
        }
        match &self.arena[child].kind {
            Expr::ArrayLiteral { .. } | Expr::StructLiteral { .. } => {
                self.literal_value(child, shape, mode)
            }
            _ => self.agg_value(child, mode),
        }
    }

    /// Folds an index expression to the integer it denotes: a number literal,
    /// an identifier inlined as a constant term (a `const` or a pure `let` of a
    /// literal), or arithmetic over operands that themselves fold. Any other
    /// index is non-constant.
    ///
    /// Arithmetic folds because an index the source computes from constants is
    /// as statically certain as one it writes out, and an index this returns
    /// `None` for is defined by cases instead — which for a certainly
    /// out-of-range index would bury an authoring error in an unprovable goal
    /// rather than name it.
    ///
    /// The result is the number the source names, read at its recorded type's
    /// signedness — not the bit pattern the term language carries it at, under
    /// which an unsigned index would fold to a negative number no reader could
    /// find in the program. `i128` holds every width at either reading, so no
    /// intermediate result wraps unseen.
    ///
    /// An operation whose result leaves the width the source computes it at
    /// stays unfolded, as does one with no total reading (division by zero, and
    /// the bitwise and shift operators, whose meaning is a bit pattern rather
    /// than a number this fold could name). The symbolic path is the faithful
    /// answer there: the index becomes a closed term the assertion language
    /// evaluates at the source's own width, wrapping exactly where the source
    /// wraps.
    fn fold_const_index(&self, index: ExprId) -> Option<i128> {
        match &self.arena[index].kind {
            Expr::Parenthesized { expr } => self.fold_const_index(*expr),
            Expr::NumberLiteral { value } => {
                let Some(TypeInfoKind::Number(width)) =
                    self.ctx.get_node_typeinfo(node_expr(index)).map(|t| t.kind)
                else {
                    return None;
                };
                Some(number_value(width, value))
            }
            Expr::Identifier(ident_id) => {
                let Some(Binding::Term(HTerm::Const(constant))) =
                    self.env.get(&self.arena[*ident_id].name)
                else {
                    return None;
                };
                Some(const_value(*constant, self.expr_is_unsigned(index)))
            }
            Expr::Binary { left, right, op } => {
                let left = self.fold_const_index(*left)?;
                let right = self.fold_const_index(*right)?;
                let folded = match op {
                    OperatorKind::Add => left.checked_add(right),
                    OperatorKind::Sub => left.checked_sub(right),
                    OperatorKind::Mul => left.checked_mul(right),
                    OperatorKind::Div => left.checked_div(right),
                    OperatorKind::Mod => left.checked_rem(right),
                    _ => None,
                }?;
                self.at_source_width(index, folded)
            }
            Expr::PrefixUnary {
                expr,
                op: UnaryOperatorKind::Neg,
            } => {
                let value = self.fold_const_index(*expr)?;
                self.at_source_width(index, value.checked_neg()?)
            }
            _ => None,
        }
    }

    /// A folded value, kept only when the width the source computes it at can
    /// name it — an expression the checker left untyped names nothing, so it
    /// does not fold either.
    fn at_source_width(&self, expr: ExprId, value: i128) -> Option<i128> {
        let Some(TypeInfoKind::Number(width)) =
            self.ctx.get_node_typeinfo(node_expr(expr)).map(|t| t.kind)
        else {
            return None;
        };
        width_names(width, value).then_some(value)
    }

    /// An aggregate `==`/`!=` in assertion position: leafwise equality over
    /// the two value trees — the conjunction of per-leaf `term_eq` for
    /// equality; for inequality (and for a negated equality) the De Morgan
    /// dual, a disjunction of negated per-leaf equalities.
    ///
    /// Leafwise is the language rule: `==` compares values, and an
    /// aggregate's value is exactly its ordered scalar leaves. The executable
    /// lowering currently compares frame *pointers* for the same source
    /// expression, contradicting the language's value-copy semantics — a
    /// defect on the executable side, tracked separately, not a precedent
    /// this encoding follows. Aggregate comparison in *term* position stays
    /// rejected.
    fn aggregate_comparison(
        &mut self,
        left: ExprId,
        right: ExprId,
        negated: bool,
        mode: Mode,
    ) -> HAssert {
        let left_value = self.agg_value(left, mode);
        let right_value = self.agg_value(right, mode);
        let mut left_leaves = Vec::new();
        let mut right_leaves = Vec::new();
        if !left_value.collect_leaves(&mut left_leaves)
            || !right_value.collect_leaves(&mut right_leaves)
        {
            // A sentinel operand already carries its diagnostic; a claim
            // built over it would be about a value the source never defined.
            return HAssert::True;
        }
        debug_assert_eq!(
            left_leaves.len(),
            right_leaves.len(),
            "a type-checked aggregate comparison has operands of one shape"
        );
        let pairs = left_leaves.into_iter().zip(right_leaves);
        if negated {
            disjoin(
                pairs
                    .map(|(l, r)| HAssert::Not(Box::new(HAssert::TermEq(l, r))))
                    .collect(),
            )
        } else {
            conjoin(pairs.map(|(l, r)| HAssert::TermEq(l, r)).collect())
        }
    }

    /// The non-scalar-in-term-position diagnostic, rendered from the
    /// expression's recorded type.
    ///
    /// An aggregate reaching here is a different mistake from a `unit` or a
    /// function type reaching here, and gets a different message. Its type is
    /// perfectly nameable in a specification — it just is not a *term*, and the
    /// shared wording, which ends by listing the aggregates a specification
    /// names, would reject `[i32; 2]` while saying arrays of integers are
    /// nameable. The commonest way to arrive is passing an aggregate to a call,
    /// which stays rejected because the callee's real signature takes a pointer.
    /// The untyped fallback keeps the shared wording: nothing is known there,
    /// least of all that the value is an aggregate — and it is defensive, since
    /// the type checker records a type for every expression it accepts.
    fn error_non_scalar_expr(&mut self, expr: ExprId) {
        let message = match self.ctx.get_node_typeinfo(node_expr(expr)) {
            Some(info) if self.kind_is_aggregate(&info.kind) => aggregate_not_a_term_message(&info),
            Some(info) => non_scalar_term_message(&info),
            None => unknown_type_term_message(),
        };
        self.error(PCode::P004, self.arena[expr].location, message);
    }

    /// Renders the diagnostic message for a non-scalar type in term/parameter
    /// position.
    fn non_scalar_message(&self, ty: TypeId) -> String {
        non_scalar_term_message(&TypeInfo::from_type_id(self.arena, ty))
    }

    /// Emits the right diagnostic for a `@` at a non-scalar type: [`PCode::P008`]
    /// for a compound (array/struct) type, [`PCode::P004`] otherwise. The
    /// `P008` wording is mode-aware — in a reachability body the reason a
    /// compound `@` is impossible is different (a choice arrives as one scalar
    /// parameter of the run the obligation is about), while a universal body
    /// reaches this at all only for a shape outside the representable surface.
    /// The universal wording says so: a compound `@` has an encoding now, so a
    /// message claiming it has none would name a rule the language dropped.
    ///
    /// Which of the two a type gets is decided by the spelling-total
    /// [`Self::kind_is_aggregate`], the same classifier that chooses the
    /// sentinel binding at the call site: a struct named through a
    /// `::`-qualified path is the same rejection as the unqualified spelling
    /// of the same struct, never a different diagnostic.
    fn emit_non_scalar_uzumaki(&mut self, ty: TypeId, location: Location, mode: Mode) {
        let type_info = TypeInfo::from_type_id(self.arena, ty);
        if self.kind_is_aggregate(&type_info.kind) {
            let rendered = type_info.to_string();
            let message = if mode.binds_choice_slots() {
                self.reach_uzumaki_message(&rendered)
            } else {
                format!(
                    "uzumaki (@) over compound type `{rendered}` quantifies a shape the \
                     assertion encoding cannot take apart: an aggregate becomes one quantified \
                     variable per scalar leaf, which reaches through arrays of scalars at any \
                     rank and structs whose fields are scalars or one-dimensional arrays of \
                     those, and no deeper; quantify the components you need individually"
                )
            };
            self.error(PCode::P008, location, message);
        } else {
            self.error(PCode::P004, location, non_scalar_term_message(&type_info));
        }
    }

    /// The reachability-body [`PCode::P008`] message, shared by the `let` form
    /// and the call-argument form so the two spellings of the same mistake read
    /// alike. Both remedies it offers are real: a component-wise `@` works here,
    /// and a `forall`-bodied spec function takes the aggregate whole.
    fn reach_uzumaki_message(&self, rendered: &str) -> String {
        let kind = self.reach_kind();
        let article = quantifier_article(kind);
        format!(
            "uzumaki (@) over compound type `{rendered}` cannot be a reachability choice: this \
             is {article} `{kind}`-quantified spec function, whose obligation is about an actual \
             run of its own body — each choice arrives as one scalar parameter of that run, and \
             a `{rendered}` value lives in linear memory; bind one `@` per scalar component \
             here, or state the property in a `forall`-bodied spec function, where an aggregate \
             `@` quantifies one variable per scalar leaf"
        )
    }

    /// The `@` the pre-scan did not plan, reached in call-argument position of
    /// a reachability body. The pre-scan plans every scalar `@`, so an
    /// unplanned one is at a non-scalar type: a compound gets the reachability
    /// [`PCode::P008`] wording, anything else the standard non-scalar
    /// [`PCode::P004`] text.
    ///
    /// Only the compound arm is reachable from source; the other two are
    /// defensive and deliberately untested. A non-scalar, non-aggregate
    /// argument type would have to be a `string`, a `unit` or a function type,
    /// none of which survives code generation's own value-type check to reach
    /// this pass, and an argument with no recorded type would be a type-checker
    /// gap. They stay as honest diagnostics rather than `unreachable!` so such
    /// a gap degrades into a message instead of aborting the compiler.
    fn emit_unplanned_reach_argument(&mut self, arg: ExprId) {
        let location = self.arena[arg].location;
        let kind = self.ctx.get_node_typeinfo(node_expr(arg)).map(|t| t.kind);
        match kind {
            Some(
                kind @ (TypeInfoKind::Array(_, _)
                | TypeInfoKind::Struct(_, _)
                | TypeInfoKind::Custom(_)),
            ) => {
                let message = self.reach_uzumaki_message(&kind.to_string());
                self.error(PCode::P008, location, message);
            }
            Some(kind) => {
                self.error(PCode::P004, location, non_scalar_term_message(&kind));
            }
            None => {
                self.error(PCode::P004, location, unknown_type_term_message());
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
        self.pending.push(PendingBinder {
            quant: Binder::Ex,
            definition,
        });
        witness
    }

    /// Allocates the next pending binder as a *universal* one, carrying the
    /// typing guard its readers depend on.
    ///
    /// The guard rides with the binder rather than through the `univ_guards`
    /// channel because this binder is wrapped around the enclosing statement's
    /// atom, while that channel drains around the statement — outside the
    /// wrap, where the variable is no longer bound.
    fn bind_universal(&mut self, width: HNumType) -> HTerm {
        let variable = HTerm::LVar(self.depth + self.pending_len());
        self.pending.push(PendingBinder {
            quant: Binder::All,
            definition: HAssert::HasType(variable.clone(), width),
        });
        variable
    }

    /// Removes the binders allocated since `base` as one group.
    fn split_pending(&mut self, base: usize) -> PendingGroup {
        PendingGroup {
            base_level: self.depth + level_count(base),
            binders: self.pending.split_off(base),
        }
    }

    /// Runs `f` and takes away the *definitions* of every existential binder it
    /// introduced, returning them conjoined in allocation order.
    ///
    /// This is how a constraint reaches the arm that evaluates it. Only the
    /// definitions move: each binder stays pending with a `⊤` definition, so
    /// the levels allocated inside `f` remain valid and the `HA_ex`s still hoist
    /// to the enclosing atom. A binder left unconstrained that way is exactly
    /// right — on the arm the source skips, nothing reads it.
    ///
    /// A *universal* binder keeps its definition where it is. That definition is
    /// a typing guard, which assumes rather than demands, so leaving it at the
    /// wrap costs the skipped arm nothing; moving it would turn the assumption
    /// into a conjunct the proof has to establish for every value.
    fn capture_definitions<T, F>(&mut self, f: F) -> (T, HAssert)
    where
        F: FnOnce(&mut Self) -> T,
    {
        let base = self.pending.len();
        let value = f(self);
        let taken: Vec<HAssert> = self.pending[base..]
            .iter_mut()
            .filter(|binder| binder.quant == Binder::Ex)
            .map(|binder| std::mem::replace(&mut binder.definition, HAssert::True))
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

    /// The quantifier word of the body being translated, for a diagnostic that
    /// explains a restriction specific to reachability. Only a reachability
    /// body has one, and every message that interpolates it is raised on a
    /// path [`Mode::binds_choice_slots`] already guarded, so the fallback is
    /// unreachable rather than a default worth choosing.
    fn reach_kind(&self) -> &'static str {
        match &self.reach {
            Some(reach) if reach.unique => "unique",
            _ => "exists",
        }
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

    /// The one [`PCode::P007`] message, raised from the block form and from the
    /// `if`-branch form alike. The restriction it reports is specific to a
    /// reachability body — inside a `forall`/plain one the same nesting binds
    /// a universal logical variable per `@` and translates — so the wording
    /// names the quantifier that makes it impossible rather than the shape.
    fn error_reach_forall(&mut self, location: Location) {
        let kind = self.reach_kind();
        let article = quantifier_article(kind);
        self.error(
            PCode::P007,
            location,
            format!(
                "a `forall` block has no encoding inside {article} `{kind}`-quantified spec \
                 function: this function's obligation is about one actual run of its own body, \
                 where every `@` is a choice that run makes, so there is no way to also range \
                 over all values inside it; move the universal claim into its own \
                 `forall`-bodied spec function"
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
                let article = quantifier_article(kind);
                self.error(
                    PCode::P011,
                    self.arena[function].location,
                    format!(
                        "call to `{name}` is not allowed: `{name}` is {article} \
                         `{kind}`-quantified spec function, and its obligation is a claim about \
                         running its own body with its own choices — there is no predicate to \
                         apply here; state the property you want directly in this body, or move \
                         the shared part into an ordinary function both spec functions can call"
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
//
// Diagnostic messages live at their call sites — `diag.rs` carries codes and
// their rationale, never text. The few message builders below are the
// exception, and the rule for joining them is narrow: a wording moves here only
// when the *same fact* is stated from more than one site, so that the sites
// cannot drift apart on it. A wording used once stays where it is raised, even
// when it is long, because that is where a reader looks for it.

/// A distinct signed-relop selector, so [`signed_relop`] reads clearly.
#[derive(Clone, Copy)]
enum Ordered {
    Lt,
    Le,
    Gt,
    Ge,
}
use Ordered::{Ge, Gt, Le, Lt};

/// The indefinite article a quantifier word takes when a diagnostic names it:
/// *an* `exists`-quantified function, *a* `unique`-quantified one. A reader
/// speaks the word even though it renders in backticks, so the article follows
/// its sound — which is why the two words are spelled out here rather than
/// tested for a leading vowel letter, the rule that gets `unique` wrong. The
/// only producers are [`SpecFnTranslator::reach_kind`] and
/// [`SpecFnTranslator::reachability_kind`], both of which yield `"exists"` or
/// `"unique"`.
fn quantifier_article(kind: &str) -> &'static str {
    match kind {
        "exists" => "an",
        _ => "a",
    }
}

/// The one wording for a type with no place in a specification term, rendered
/// from whichever spelling of the type the site has to hand.
///
/// The tail names the *whole* representable surface rather than the scalar part
/// of it. Aggregates became nameable when the leaf encoding landed, so a message
/// claiming that only scalars can appear would now be read as a rule the
/// language does not have — the parameter site accepts `[i32; 3]` and rejects
/// `[Point; 2]`, and the reason is the shape, not aggregation. Naming the
/// supported shapes is also the whole remedy: there is nothing to rewrite for a
/// `unit` or a function type, and for an over-deep aggregate the fix is to
/// flatten it.
fn non_scalar_term_message(ty: &impl std::fmt::Display) -> String {
    format!("type `{ty}` cannot appear in a specification term; {TERM_SURFACE}")
}

/// [`non_scalar_term_message`] for a site whose expression the type checker left
/// untyped, where naming the type is not an option.
fn unknown_type_term_message() -> String {
    format!("type of this value cannot appear in a specification term; {TERM_SURFACE}")
}

/// The diagnostic for an aggregate read whole where a scalar term is required —
/// most often an aggregate passed to a call, whose compiled callee takes a
/// pointer. Distinct from [`non_scalar_term_message`] because the type is not
/// the problem: a specification names this very type, just never as one value.
fn aggregate_not_a_term_message(ty: &impl std::fmt::Display) -> String {
    format!(
        "type `{ty}` is an aggregate, and a term is one scalar value: a specification names an \
         aggregate by its scalar leaves rather than as a value of its own, so there is nothing \
         here for the whole of it to denote; name the component you mean, such as `a[0]` or \
         `p.x`"
    )
}

/// The representable surface, as one clause both messages above end with.
///
/// The rank asymmetry is real and must be spelled out: an array of scalars
/// nests to any depth, while a struct field may be a scalar or a
/// one-dimensional array of scalars and no deeper — the same boundary analysis
/// rules A027/A028 draw for the executable aggregate `@`, which this surface is
/// deliberately equal to. A tail that said "such arrays" for the struct case
/// would name `[[i32; 2]; 2]` as a legal field while rejecting the struct that
/// has one.
const TERM_SURFACE: &str = "a term is a bool, an integer, or an enum value, and the only \
                            aggregates a specification names are arrays of those at any rank \
                            and structs whose fields are those or one-dimensional arrays of \
                            those";

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

/// An index constant at the class the chain's index term rides in, so the
/// range comparison and the per-case equalities are well-typed at the width
/// the index itself was translated at.
///
/// Every value passed here is an array extent or a position within one, and an
/// array that reached a value tree is inside the per-function leaf budget — so
/// the width conversion is an invariant of the introduction rules rather than a
/// case to handle.
fn index_const(class: HNumType, value: usize) -> HTerm {
    let value = i64::try_from(value).expect("an array extent is within the spec leaf budget");
    HTerm::Const(match class {
        HNumType::I32 => HConst::I32(
            i32::try_from(value).expect("an array extent is within the spec leaf budget"),
        ),
        HNumType::I64 => HConst::I64(value),
    })
}

/// The field at `position` of a struct candidate. A candidate of another shape
/// carries a sentinel from its own introduction — siblings of one element type
/// cannot differ in shape.
fn field_child(value: &AggValue, position: usize) -> AggValue {
    match value {
        AggValue::Struct(fields) => fields
            .get(position)
            .map_or(AggValue::Sentinel, |(_, child)| child.clone()),
        _ => AggValue::Sentinel,
    }
}

/// The element at `position` of an array candidate, on the same terms as
/// [`field_child`].
fn element_child(value: &AggValue, position: usize) -> AggValue {
    match value {
        AggValue::Array(children) => children
            .get(position)
            .cloned()
            .unwrap_or(AggValue::Sentinel),
        _ => AggValue::Sentinel,
    }
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

/// The number a literal names, read at the width recorded for it. This is the
/// value the author wrote; [`number_const`] is the same literal as the *term*
/// encoding carries it, which for an unsigned width is a different number with
/// the same bit pattern — the constant to compute with, never the one to name
/// back to the author.
fn number_value(width: NumberType, value: &str) -> i128 {
    match width {
        NumberType::I8 | NumberType::I16 | NumberType::I32 => {
            i128::from(parse_at::<i32>(value, width))
        }
        NumberType::U8 => i128::from(parse_at::<u8>(value, width)),
        NumberType::U16 => i128::from(parse_at::<u16>(value, width)),
        NumberType::U32 => i128::from(parse_at::<u32>(value, width)),
        NumberType::I64 => i128::from(parse_at::<i64>(value, width)),
        NumberType::U64 => i128::from(parse_at::<u64>(value, width)),
    }
}

/// Whether a width can name a number, at the same reading [`number_value`]
/// gives: an unsigned width names from zero up, a signed one names its
/// two's-complement range.
fn width_names(width: NumberType, value: i128) -> bool {
    let range = match width {
        NumberType::I8 => i128::from(i8::MIN)..=i128::from(i8::MAX),
        NumberType::I16 => i128::from(i16::MIN)..=i128::from(i16::MAX),
        NumberType::I32 => i128::from(i32::MIN)..=i128::from(i32::MAX),
        NumberType::I64 => i128::from(i64::MIN)..=i128::from(i64::MAX),
        NumberType::U8 => 0..=i128::from(u8::MAX),
        NumberType::U16 => 0..=i128::from(u16::MAX),
        NumberType::U32 => 0..=i128::from(u32::MAX),
        NumberType::U64 => 0..=i128::from(u64::MAX),
    };
    range.contains(&value)
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

/// Whether a type kind is one of the unsigned number widths — the widths whose
/// values the term language carries at their signed bit pattern, so that both
/// the choice of relational operator and the reading back of a constant depend
/// on it. An unrecorded type reads signed.
fn kind_is_unsigned(kind: Option<&TypeInfoKind>) -> bool {
    matches!(
        kind,
        Some(TypeInfoKind::Number(
            NumberType::U8 | NumberType::U16 | NumberType::U32 | NumberType::U64
        ))
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

/// Right-folds assertions into one disjunction, `⊥` for none:
/// `a₀ ∨ (a₁ ∨ (… ∨ aₙ))`. The primitive `Or` node, not the ⊤-absorbing smart
/// constructor: the clauses here are never `⊤`, and the fold must not collapse.
fn disjoin(assertions: Vec<HAssert>) -> HAssert {
    assertions
        .into_iter()
        .rev()
        .reduce(|acc, assertion| HAssert::Or(Box::new(assertion), Box::new(acc)))
        .unwrap_or(HAssert::False)
}

/// Materializes an existential aggregate introduction: consecutive absolute
/// binder levels starting at `*next_level`, one per scalar leaf, in
/// enumeration order.
fn exist_agg_value(shape: &AggShape, next_level: &mut u32) -> AggValue {
    match shape {
        AggShape::Scalar(_) => {
            let level = *next_level;
            *next_level += 1;
            AggValue::Scalar(HTerm::LVar(level))
        }
        AggShape::Array(elem, len) => AggValue::Array(
            (0..*len)
                .map(|_| exist_agg_value(elem, next_level))
                .collect(),
        ),
        AggShape::Struct(fields) => AggValue::Struct(
            fields
                .iter()
                .map(|(name, field)| (name.clone(), exist_agg_value(field, next_level)))
                .collect(),
        ),
    }
}

/// A stored constant read back as the number its type names. An unsigned
/// constant rides in the term language at its signed bit pattern, so reading it
/// as the source spelled it undoes that reinterpretation.
fn const_value(constant: HConst, unsigned: bool) -> i128 {
    match (constant, unsigned) {
        (HConst::I32(v), false) => i128::from(v),
        (HConst::I32(v), true) => i128::from(v.cast_unsigned()),
        (HConst::I64(v), false) => i128::from(v),
        (HConst::I64(v), true) => i128::from(v.cast_unsigned()),
    }
}

/// Wraps `body` in one quantifier per entry of `binders`, which occupy levels
/// `base_level ..` in allocation order. Folding innermost-first puts the
/// first-allocated binder outermost, so a later definition may name an earlier
/// binder: `∃v₀. (def₀ ∧ ∃v₁. (def₁ ∧ … ∧ body))`.
///
/// The two quantifiers attach their definitions differently, because the
/// definitions mean different things: an existential binder is *pinned* by its
/// constraint (a conjunct), while a universal binder is *typed* by its guard (an
/// antecedent). Both readings keep the definition inside its own binder, which
/// is what lets the variable appear in it at all.
///
/// A binder whose variable does not occur in the accumulated body is emitted
/// *without* its definition. A definition pins a value, so keeping one for a
/// variable nothing reads would turn a specification that claims nothing into a
/// refutable claim — `let unused: bool = 10 / x == 0 || true;` alone must stay
/// `HA_true`. Only the definition is dropped, never the binder: dropping the
/// binder would shift the level of every binder allocated inside it. The
/// innermost-first order lets one dropped definition cascade outward, and
/// [`HAssert::ex`]/[`HAssert::all`] collapse the resulting `∃x. ⊤`/`∀x. ⊤` away.
fn wrap_binders(body: HAssert, group: PendingGroup) -> HAssert {
    let PendingGroup {
        binders,
        base_level,
    } = group;
    let mut level = base_level + level_count(binders.len());
    let mut acc = body;
    for binder in binders.into_iter().rev() {
        level -= 1;
        let read = assert_mentions_level(&acc, level);
        acc = match (binder.quant, read) {
            (Binder::Ex, true) => HAssert::ex(HAssert::and(binder.definition, acc)),
            (Binder::Ex, false) => HAssert::ex(acc),
            (Binder::All, true) => HAssert::all(HAssert::imp(binder.definition, acc)),
            (Binder::All, false) => HAssert::all(acc),
        };
    }
    acc
}

/// Whether `assertion` reads the logical variable bound at absolute `level`.
/// Levels are position-independent, so no shifting is needed under `HA_ex`.
fn assert_mentions_level(assertion: &HAssert, level: u32) -> bool {
    match assertion {
        HAssert::True | HAssert::False => false,
        HAssert::Not(inner) | HAssert::Ex(inner) | HAssert::All(inner) => {
            assert_mentions_level(inner, level)
        }
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
/// binder — `HA_ex` and `Hall` alike, since both bind index 0 in their body.
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
        HAssert::All(body) => HAssert::All(Box::new(lower_assert(body, depth + 1))),
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
        let externs = ExternIndex::build(ctx.arena());
        let mut translator = SpecFnTranslator::new(&ctx, &[], "S", &callee, &externs);
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
        let externs = ExternIndex::build(ctx.arena());
        let mut translator = SpecFnTranslator::new(&ctx, &[], "S", &callee, &externs);
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

    /// One literal has two readings, and the pair must not be confused:
    /// [`number_const`] is the term encoding, where an unsigned value rides at
    /// its signed bit pattern, while [`number_value`] is the number the source
    /// names — the only one a diagnostic may quote back. The all-ones patterns
    /// alone would not discriminate, so each width also takes a value where the
    /// two readings differ without being `-1`.
    #[test]
    fn a_literal_reads_one_way_as_a_term_and_another_as_a_number() {
        assert_eq!(number_const(NumberType::U32, "4294967295"), HConst::I32(-1));
        assert_eq!(number_value(NumberType::U32, "4294967295"), 4_294_967_295);
        assert_eq!(
            number_const(NumberType::U32, "2147483648"),
            HConst::I32(i32::MIN)
        );
        assert_eq!(number_value(NumberType::U32, "2147483648"), 2_147_483_648);
        assert_eq!(
            number_const(NumberType::U64, "18446744073709551615"),
            HConst::I64(-1)
        );
        assert_eq!(
            number_value(NumberType::U64, "18446744073709551615"),
            i128::from(u64::MAX)
        );
        // Signed widths and the small unsigned widths read alike both ways.
        assert_eq!(
            number_value(NumberType::I32, "-2147483648"),
            i128::from(i32::MIN)
        );
        assert_eq!(
            number_value(NumberType::I64, "-9223372036854775808"),
            i128::from(i64::MIN)
        );
        assert_eq!(number_value(NumberType::U8, "255"), 255);
        assert_eq!(number_value(NumberType::U16, "65535"), 65535);
    }

    /// The same reinterpretation in reverse, for a constant already stored as a
    /// term: it reads back at the signedness of the type that named it.
    #[test]
    fn a_stored_constant_reads_back_at_its_declared_signedness() {
        assert_eq!(const_value(HConst::I32(-1), true), i128::from(u32::MAX));
        assert_eq!(const_value(HConst::I32(-1), false), -1);
        assert_eq!(const_value(HConst::I64(-1), true), i128::from(u64::MAX));
        assert_eq!(const_value(HConst::I64(-1), false), -1);
        assert_eq!(const_value(HConst::I32(255), true), 255);
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
