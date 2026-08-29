//! Proof-mode translation of specification functions into `hassert`
//! verification obligations.
//!
//! In `Proof` mode, after every function body has been compiled to WASM, this
//! pass reads the typed AST and the emittable-function buckets — never the
//! compiler's byte output — and turns each specification *free* function into
//! one [`inference_hassert::HAssert`]: a `forall`-quantified (or plain) body
//! becomes a universal (`ValidSpec`) obligation, an `exists`/`unique` body a
//! reachability obligation whose entry carries its [`inference_hassert::SpecKind`]
//! and [`inference_hassert::ReachMeta`]. The obligations are grouped by folded
//! specification name into an [`inference_hassert::HSpecMap`], which code
//! generation attaches to its [`CodegenOutput`](crate::CodegenOutput). Because
//! the pass is read-only over the AST and type information, proof-mode WASM
//! bytes are unchanged by construction.
//!
//! The translation scheme lives in [`translate`]; the diagnostics registry in
//! [`diag`].
//!
//! ## Untranslatable specifications are fatal
//!
//! A specification function that cannot be encoded as an obligation (an
//! `assume` body modifier, a struct or method value, an `external` call, …)
//! contributes *no* obligation and records `P0xx` diagnostics. Those
//! diagnostics are collected here but surfaced by the caller
//! ([`crate::codegen`]) as a hard code-generation error: the obligation is a
//! required proof-mode deliverable, so a module whose specifications are
//! silently unverifiable must not be emitted. The pass itself is still read-only
//! over the AST and type information — it computes obligations without touching
//! the compiler's byte output — so a proof-mode `.wasm` that *does* codegen is
//! byte-identical to before; the flip only decides whether codegen succeeds at
//! all. Every diagnostic is gathered before failing so a spec with several
//! mistakes surfaces them all at once.
//!
//! ## A vacuous obligation is fatal
//!
//! Every specification free function must yield a *non-vacuous* obligation. One
//! that collapses to `HA_true` records `P010` and fails code generation, for the
//! same reason an untranslatable one does: an obligation any proof discharges
//! without reading the program is indistinguishable from no verification at all,
//! and a passing proof of it means nothing.
//!
//! The predicate is the translated result, not the body shape. The collapse
//! happens in the ⊤-absorbing smart constructors ([`HAssert::and`],
//! [`HAssert::imp`], …) *after* the statement translator has run, so a body can
//! look like it contributes and still reach `True`: a trailing `assume` block
//! folds to `Imp(p, ⊤) = ⊤`, and an `if` whose branches are both vacuous folds
//! the same way. Checking the value the translator returned catches the whole
//! family exactly, with no shape enumeration to keep in sync.
//!
//! A helper that only computes therefore cannot live in a `spec` block. It
//! belongs at file scope, where a specification function can still apply it as a
//! `T_app`. A plain specification *method* keeps its helper exemption — it
//! produces no obligation either way — but one that carries an `assert`, at any
//! depth, raises `P009` rather than dropping that assertion silently.
//!
//! ## Obligation depth and the encoding cap
//!
//! The `inference.hspecs` codec caps assertion-tree depth at
//! [`inference_hassert::MAX_TREE_DEPTH`] (256), and refuses a tree *deeper*
//! than that, so all 256 levels are usable. The depth counts **assertion nodes
//! only**: the codec's walk hands a term to a separate check on a fresh
//! counter, so however deeply a term nests it never extends the assertion
//! depth. Printed-paren nesting in the emitted `.v` is a different number and
//! must not be read as this one.
//!
//! The right-folded statement translator spends one `And`/`Imp` level per
//! structural statement, and a statement whose introductions drain their
//! hypotheses spends two — `Imp(hypotheses, And(…))` — so the practical
//! statement budget for a guard-heavy body is roughly half the cap. The figures
//! below are all measured on one shape — a body that drains every introduction
//! at a single `assert` — because `N` introductions drained at one statement and
//! `N` introductions each with their own statement are different trees.
//!
//! *Universally*, each introduction contributes exactly one level to the `And`
//! spine of that antecedent, whatever its declared type. A narrow one carries
//! its declared value domain grouped into that single spine entry
//! (`And(guard, bound)`) rather than as a spine entry of its own, which is what
//! keeps the count per introduction the same as a full-width one's: a bound
//! stated as a second entry would halve how many narrow introductions fit. What
//! a narrow entry does add is depth *inside* itself, on a branch the fold does
//! not continue along — one `And`, then one `HA_not` over a `term_eq` for an
//! unsigned bound or an `And` of two of those for a signed one.
//!
//! *Existentially* that equality does not hold, and this is where the real
//! ceiling is. A read narrow binder is emitted as `Ex(And(bound, body))` while
//! a full-width one's `⊤` definition is absorbed and leaves a bare `Ex`, so a
//! narrow binder costs **two** accumulating levels against one. Signedness does
//! not change it: a signed pair's `And` of two bounds sits inside that one
//! conjunct node. At the 64-leaf budget the deepest aggregate is therefore
//! existential and narrow, at 192 of the 256 levels, against 128 for the
//! full-width existential and 66–68 for any universal one.
//!
//! A short-circuit `&&`/`||` costs two more levels for its pinned witness
//! (`Ex` over `And`), plus the depth of the constraint itself. Where the
//! witness belongs to a statement's own atom the cost is local — it does not
//! accumulate down the statement fold — but a witness bound by a pure `let` or
//! a `const` scopes over the rest of its block, so its two levels sit above
//! every statement that follows and a chain of such bindings adds up.
//!
//! An aggregate is the largest single consumer, and what a leaf costs depends
//! on where it comes from: a universal `@` or parameter leaf costs one
//! hypothesis level; a leaf bound under a nested `forall` costs a hypothesis
//! level *and* an `All` level; an existential leaf costs one `Ex` level, plus a
//! second for the `And` a narrow one's bound rides in; and a literal's leaves
//! bind nothing and guard nothing, nesting only one conjunct apiece through a
//! leafwise comparison. All of them accumulate across every aggregate introduction in
//! the function, which is why the leaf budget
//! (`SPEC_FN_MAX_QUANTIFIED_LEAVES`, `P013`) is a per-function running total
//! rather than a per-introduction cap — a cap that let each of four parameters
//! through individually would still reach the encoder's backstop between them.
//! `P013` is checked from the declared type *before* any leaf is materialized,
//! so an over-budget declaration never builds the deep tree it is being
//! rejected for.
//! Overrunning the cap is not an encoder hazard: the pre-encode gate
//! ([`check_payload`](crate::hspecs_section::check_payload)) already turns an
//! over-deep tree into a
//! [`CodegenError::HspecTreeTooDeep`](crate::errors::CodegenError::HspecTreeTooDeep)
//! naming the offending specification and function.

mod claim;
mod diag;
pub(crate) mod reach;
mod translate;

#[cfg(test)]
mod tests;

use inference_ast::arena::AstArena;
use inference_ast::ids::DefId;
use inference_ast::nodes::{BlockKind, Def};
use inference_fn_key::FnKey;
use inference_hassert::{HAssert, HFnRef, HSpecEntry, HSpecMap, ReachMeta, SpecKind};
use inference_type_checker::typed_context::TypedContext;
use rustc_hash::FxHashMap;

use crate::EmittableFunctions;

use claim::Claim;
pub(crate) use diag::HassertDiagnostic;
use diag::PCode;

/// A map from a function's structured key to its definition, for every
/// function compiled from Inference source a specification term may call.
///
/// `external fn`s are deliberately absent: they have no structured key and no
/// body here, and are resolved through
/// [`ExternIndex`](inference_type_checker::ExternIndex) instead.
pub(crate) struct CalleeIndex {
    defs: FxHashMap<FnKey, DefId>,
}

impl CalleeIndex {
    /// Builds the index from the same buckets code generation collected, keying
    /// each function by the identity code generation registered it under.
    fn build(arena: &AstArena, buckets: &EmittableFunctions) -> Self {
        let mut defs = FxHashMap::default();
        for entry in &buckets.funcs {
            defs.insert(
                FnKey::free_in(entry.module_path.clone(), arena.def_name(entry.def_id)),
                entry.def_id,
            );
        }
        for entry in &buckets.methods {
            defs.insert(
                FnKey::method_in(
                    entry.module_path.clone(),
                    entry.struct_name.clone(),
                    arena.def_name(entry.def_id),
                ),
                entry.def_id,
            );
        }
        for entry in &buckets.spec_funcs {
            defs.insert(
                FnKey::spec_free_folded(
                    &entry.module_path,
                    &entry.spec_name,
                    arena.def_name(entry.def_id),
                ),
                entry.def_id,
            );
        }
        for entry in &buckets.spec_methods {
            defs.insert(
                FnKey::spec_method_folded(
                    &entry.module_path,
                    &entry.spec_name,
                    entry.struct_name.clone(),
                    arena.def_name(entry.def_id),
                ),
                entry.def_id,
            );
        }
        Self { defs }
    }

    fn get(&self, key: &FnKey) -> Option<DefId> {
        self.defs.get(key).copied()
    }
}

/// Translates every specification free function into its obligation.
///
/// Returns the obligations grouped by folded specification name in source order,
/// paired with every `P0xx` diagnostic raised. A specification function that
/// raised any diagnostic contributes no obligation (its partial tree would be
/// unsound), and one that translated cleanly to the vacuous `HA_true`
/// contributes none either (it raises `P010` instead). The pass itself keeps
/// going so it can collect every diagnostic in one pass, and the caller
/// ([`crate::codegen`]) turns a non-empty diagnostic list into a hard error.
///
/// `reach_plans` is the reachability view over the choice-lowering plans
/// ([`crate::choice`]) the compiler consumed for its signature suffix and body
/// lowering. An `exists`/`unique` body translates its payload against the same
/// plan — both consumers read one `ExprId`-keyed map, so a payload slot index
/// equals the compiled frame index of the same choice by construction — and its
/// entry carries the [`SpecKind`] and [`ReachMeta`] the downstream reachability
/// judgment needs. A universal body never reaches a plan at all, which is what
/// makes its payload structurally independent of the compiled frame.
pub(crate) fn translate_spec_fns<'a>(
    ctx: &'a TypedContext,
    buckets: &EmittableFunctions,
    reach_plans: &reach::ReachPlans<'a>,
) -> (HSpecMap, Vec<HassertDiagnostic>) {
    let arena = ctx.arena();
    let callee = CalleeIndex::build(arena, buckets);
    let externs = ctx.extern_index();
    let mut map = HSpecMap::default();
    let mut diagnostics = Vec::new();

    for entry in &buckets.spec_funcs {
        let plan = reach_plans.get(entry.def_id);
        let mut translator = translate::SpecFnTranslator::new(
            ctx,
            &entry.module_path,
            &entry.spec_name,
            &callee,
            externs,
        );
        let hassert = translator.translate_fn(entry.def_id, plan);
        let fn_diagnostics = translator.take_diagnostics();
        if !fn_diagnostics.is_empty() {
            // An untranslatable spec function yields no obligation rather than a
            // partial (unsound) one.
            diagnostics.extend(fn_diagnostics);
            continue;
        }
        if hassert == HAssert::True {
            // Checked only once the function is otherwise clean, so `P010` never
            // stacks on top of a `P001`–`P008` the same function already raised.
            diagnostics.push(vacuous_obligation_diagnostic(arena, entry));
            continue;
        }

        let symbol = FnKey::spec_free_folded(
            &entry.module_path,
            &entry.spec_name,
            arena.def_name(entry.def_id),
        )
        .to_string();
        let spec_key = inference_fn_key::fold_spec_name(&entry.module_path, &entry.spec_name);
        map.entry(spec_key).or_default().push(HSpecEntry::new(
            HFnRef(symbol),
            hassert,
            spec_kind(arena, entry.def_id, plan),
        ));
    }

    // A specification *method* never yields an obligation. Flagging one that
    // claims a property (rather than silently dropping it) keeps the contract
    // honest; a method that only computes stays a helper.
    for method in &buckets.spec_methods {
        if let Some(diagnostic) = method_obligation_diagnostic(arena, method) {
            diagnostics.push(diagnostic);
        }
    }

    (map, diagnostics)
}

/// The wire kind of one obligation: [`SpecKind::Forall`] for a universal
/// (`forall`/plain) body, the reachability kinds — carrying the entry arity
/// and source-visible slots the downstream judgment needs — for an
/// `exists`/`unique` body, which is planned iff `plan` is `Some`.
fn spec_kind(arena: &AstArena, def_id: DefId, plan: Option<reach::ReachPlan<'_>>) -> SpecKind {
    let Some(plan) = plan else {
        return SpecKind::Forall;
    };
    let meta = ReachMeta {
        entry_arity: plan.entry_arity(),
        visible_locs: plan.visible_locs(),
    };
    let Def::Function { body, .. } = &arena[def_id].kind else {
        unreachable!("only functions enter the reachability view");
    };
    match arena[*body].block_kind {
        BlockKind::Exists => SpecKind::Exists(meta),
        BlockKind::Unique => SpecKind::Unique(meta),
        other => {
            unreachable!("the reachability view holds only exists/unique bodies, found {other:?}")
        }
    }
}

/// A [`PCode::P010`] for a specification function whose obligation collapsed to
/// `HA_true`.
///
/// The wording is keyed on what the body *claims*, so the remedy names the
/// construct the user actually wrote; the diagnostic itself is already decided
/// by the collapsed obligation.
fn vacuous_obligation_diagnostic(
    arena: &AstArena,
    entry: &crate::EmittableSpecFn,
) -> HassertDiagnostic {
    let name = arena.def_name(entry.def_id);
    let vacuous = "so its obligation is the vacuous `HA_true` that any proof discharges without \
                   reading the program";
    let message = match claim::first_claim(arena, entry.def_id) {
        Some(Claim::Quantifier(kind)) => format!(
            "spec function `{name}` is `{}`-quantified but asserts nothing, {vacuous} — add an \
             `assert` over the values it binds",
            quantifier_word(kind)
        ),
        Some(Claim::NondetBlock(kind)) => {
            let word = quantifier_word(kind);
            format!(
                "spec function `{name}` claims nothing after its `{word}` block, {vacuous} — add \
                 an `assert` after the `{word}` block"
            )
        }
        // No body reaches this arm today: a `Stmt::Assert` always contributes a
        // non-`True` conjunct, and a body whose claim an enclosing construct
        // absorbs reports that construct instead. It stays a real message rather
        // than an `unreachable!` because the arm is a diagnostic fallback — a
        // statement kind that both claims and folds away must still tell the
        // user something actionable rather than abort the compiler.
        Some(Claim::Assert) => format!(
            "spec function `{name}` asserts a property its obligation does not carry, {vacuous} \
             — state the property where it constrains the program"
        ),
        None => format!(
            "spec function `{name}` only computes a value and states no property, {vacuous} — \
             assert a property about the computation, or move the function out of the `spec` block"
        ),
    };
    HassertDiagnostic::new(
        PCode::P010,
        arena[entry.def_id].location,
        entry.module_path.clone(),
        message,
    )
}

/// A [`PCode::P009`] for a specification method that carries an obligation the
/// translation cannot deliver, or `None` for one that only computes or writes a
/// non-deterministic block that asserts nothing.
///
/// What is at stake for a plain body is a *lost assertion*: the method's
/// obligation is never emitted, so an `assert` the author wrote is silently
/// dropped. Such a body is therefore reported only when it actually asserts
/// something — an inline non-deterministic block that asserts nothing drops
/// nothing. A quantified body is reported either way: the quantifier is a proof
/// obligation on its own, whatever the body does with the values it binds.
fn method_obligation_diagnostic(
    arena: &AstArena,
    method: &crate::EmittableSpecMethod,
) -> Option<HassertDiagnostic> {
    let name = arena.def_name(method.def_id);
    let message = match claim::first_claim(arena, method.def_id)? {
        Claim::Quantifier(kind) => format!(
            "spec method `{}.{name}` is `{}`-quantified; a quantified spec method carries a \
             proof obligation that cannot yet be translated to a verification assertion — move \
             the property into a `forall` spec function",
            method.struct_name,
            quantifier_word(kind)
        ),
        Claim::NondetBlock(_) | Claim::Assert => {
            // The walk is a foregone conclusion for a marker that already is an
            // `assert`; asking it of both markers keeps the rule in one place.
            if !claim::states_an_assertion(arena, method.def_id) {
                return None;
            }
            format!(
                "spec method `{}.{name}` states a property, but a spec method carries no \
                 verification obligation — move the property into a `forall` spec function",
                method.struct_name
            )
        }
    };
    Some(HassertDiagnostic::new(
        PCode::P009,
        arena[method.def_id].location,
        method.module_path.clone(),
        message,
    ))
}

fn quantifier_word(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::Exists => "exists",
        BlockKind::Assume => "assume",
        BlockKind::Unique => "unique",
        BlockKind::Forall => "forall",
        BlockKind::Regular => "regular",
    }
}
