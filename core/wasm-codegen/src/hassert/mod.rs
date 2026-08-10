//! Proof-mode translation of specification functions into `hassert`
//! verification obligations.
//!
//! In `Proof` mode, after every function body has been compiled to WASM, this
//! pass reads the typed AST and the emittable-function buckets — never the
//! compiler's byte output — and turns each `forall`-quantified (or plain)
//! specification *free* function into one [`inference_hassert::HAssert`]. The
//! obligations are grouped by folded specification name into an
//! [`inference_hassert::HSpecMap`], which code generation attaches to its
//! [`CodegenOutput`](crate::CodegenOutput). Because the pass is read-only over
//! the AST and type information, proof-mode WASM bytes are unchanged by
//! construction.
//!
//! The translation scheme lives in [`translate`]; the diagnostics registry in
//! [`diag`].
//!
//! ## Untranslatable specifications are fatal
//!
//! A specification function that cannot be encoded as an obligation (a
//! quantified body modifier, a struct or method value, an `external` call, …)
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
//! ## Obligation depth and the encoding cap
//!
//! The `inference.hspecs` codec caps assertion-tree depth at
//! [`inference_hassert::MAX_TREE_DEPTH`] (256). The right-folded statement
//! translator spends one `And`/`Imp` level per structural statement, and a
//! statement whose slots drain a typing guard spends two — `Imp(guard, And(…))`
//! — so the practical statement budget for a guard-heavy body is roughly half
//! the cap. Overrunning it is not an encoder hazard: the pre-encode gate
//! ([`check_payload`](crate::hspecs_section::check_payload)) already turns an
//! over-deep tree into a
//! [`CodegenError::HspecTreeTooDeep`](crate::errors::CodegenError::HspecTreeTooDeep)
//! naming the offending specification and function.

mod diag;
mod translate;

#[cfg(test)]
mod tests;

use inference_ast::arena::AstArena;
use inference_ast::ids::DefId;
use inference_ast::nodes::{BlockKind, Def};
use inference_fn_key::FnKey;
use inference_hassert::{HFnRef, HSpecEntry, HSpecMap};
use inference_type_checker::typed_context::TypedContext;
use rustc_hash::FxHashMap;

use crate::EmittableFunctions;

pub(crate) use diag::HassertDiagnostic;
use diag::PCode;

/// A map from a function's structured key to its definition, for every
/// module-defined function a specification term may call.
///
/// Imports (`external fn`s) are deliberately excluded: a `T_app` names a
/// *defined* function only, so an extern call is rejected rather than resolved.
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
/// unsound); the pass itself keeps going so it can collect every diagnostic in
/// one pass, and the caller ([`crate::codegen`]) turns a non-empty diagnostic
/// list into a hard error.
pub(crate) fn translate_spec_fns(
    ctx: &TypedContext,
    buckets: &EmittableFunctions,
) -> (HSpecMap, Vec<HassertDiagnostic>) {
    let arena = ctx.arena();
    let callee = CalleeIndex::build(arena, buckets);
    let mut map = HSpecMap::default();
    let mut diagnostics = Vec::new();

    for entry in &buckets.spec_funcs {
        let mut translator =
            translate::SpecFnTranslator::new(ctx, &entry.module_path, &entry.spec_name, &callee);
        let hassert = translator.translate_fn(entry.def_id);
        let fn_diagnostics = translator.take_diagnostics();
        if !fn_diagnostics.is_empty() {
            // An untranslatable spec function yields no obligation rather than a
            // partial (unsound) one.
            diagnostics.extend(fn_diagnostics);
            continue;
        }

        let symbol = FnKey::spec_free_folded(
            &entry.module_path,
            &entry.spec_name,
            arena.def_name(entry.def_id),
        )
        .to_string();
        let spec_key = inference_fn_key::fold_spec_name(&entry.module_path, &entry.spec_name);
        map.entry(spec_key)
            .or_default()
            .push(HSpecEntry::new(HFnRef(symbol), hassert));
    }

    // A quantified specification *method* carries a proof obligation that has no
    // milestone-1 encoding. Flagging it (rather than silently dropping it) keeps
    // the contract honest; a plain (`Regular`) spec method stays a helper.
    for method in &buckets.spec_methods {
        if let Some(diagnostic) = quantified_method_diagnostic(arena, method) {
            diagnostics.push(diagnostic);
        }
    }

    (map, diagnostics)
}

/// A [`PCode::P009`] for a quantified specification method, or `None` for a
/// plain one.
fn quantified_method_diagnostic(
    arena: &AstArena,
    method: &crate::EmittableSpecMethod,
) -> Option<HassertDiagnostic> {
    let Def::Function { body, .. } = &arena[method.def_id].kind else {
        return None;
    };
    let kind = arena[*body].block_kind;
    if matches!(kind, BlockKind::Regular) {
        return None;
    }
    let name = arena.def_name(method.def_id);
    Some(HassertDiagnostic::new(
        PCode::P009,
        arena[method.def_id].location,
        method.module_path.clone(),
        format!(
            "spec method `{}.{name}` is `{}`-quantified; a quantified spec method carries a \
             proof obligation that cannot yet be translated to a verification assertion — move \
             the property into a `forall` spec function",
            method.struct_name,
            quantifier_word(kind)
        ),
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
