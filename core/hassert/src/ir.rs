//! The `hassert` obligation IR: terms, assertions, and the per-program map.
//!
//! Every type here mirrors a wasm-verifier inductive from its
//! `theories/Assertions.v`, restricted to what an Inference specification can
//! express. The variant doc comments name each Coq counterpart.

use rustc_hash::FxHashMap;

/// Integer number type of an interpreted term operator.
///
/// Mirrors wasm-verifier's `number_type` restricted to the two widths
/// Inference emits — the language has no floating-point types, so `F32`/`F64`
/// are unrepresentable rather than merely unused.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum HNumType {
    /// `T_i32`.
    I32,
    /// `T_i64`.
    I64,
}

/// Binary operator of a [`HTerm::Binop`], mirroring `WasmCert`'s `Binop_i`
/// family. Signedness is baked into the divide/remainder/shift variants
/// (`DivS`/`DivU`, `RemS`/`RemU`, `ShrS`/`ShrU`) exactly as codegen chooses it
/// from the left operand's type.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HBinop {
    Add,
    Sub,
    Mul,
    DivS,
    DivU,
    RemS,
    RemU,
    And,
    Or,
    Xor,
    Shl,
    ShrS,
    ShrU,
}

/// Relational operator of a [`HTerm::Relop`], mirroring `WasmCert`'s `Relop_i`
/// family. Ordered comparisons carry their signedness (`LtS`/`LtU`, …).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HRelop {
    Eq,
    Ne,
    LtS,
    LtU,
    GtS,
    GtU,
    LeS,
    LeU,
    GeS,
    GeU,
}

/// A numeric constant. The raw bit pattern is stored; a downstream renderer
/// decides the `Vi32`/`Vi64` spelling. Mirrors `T_const (Vi32 _)` /
/// `T_const (Vi64 _)`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HConst {
    I32(i32),
    I64(i64),
}

/// A symbolic reference to a module-defined function: its WASM name-section
/// symbol (what codegen writes via `FnKey::Display`, e.g. `is_prime`,
/// `lib.arith.add`, `Point.new`).
///
/// This crate treats the string as opaque and non-empty; it never resolves it.
/// `wasm-to-v` maps the symbol to a `mod_funcs` (defined-function) index after
/// linking. The reference is guaranteed by the producer never to name an
/// import.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct HFnRef(pub String);

/// A term of the assertion language, mirroring the `term` inductive
/// (`Assertions.v`).
///
/// There is deliberately no `T_global` variant: an Inference specification
/// cannot reference a global.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum HTerm {
    /// `T_const` — a numeric constant.
    Const(HConst),
    /// `T_lvar` — a logical variable, as a de Bruijn index.
    LVar(u32),
    /// `T_local` — a synthesized `forall`-slot (local) variable.
    Local(u32),
    /// `T_app` — an (uninterpreted) function symbol applied to arguments.
    App(HFnRef, Vec<HTerm>),
    /// `T_binop` — an interpreted binary operator.
    Binop(HNumType, HBinop, Box<HTerm>, Box<HTerm>),
    /// `T_relop` — an interpreted relational operator (result is an i32 truth
    /// value).
    Relop(HNumType, HRelop, Box<HTerm>, Box<HTerm>),
}

/// An assertion, mirroring the `hassert` inductive plus its three transparent
/// sugars `Himpl`/`Hor`/`Hall` (`Assertions.v`).
///
/// The heap fragment (`HA_emp`/`HA_star`/`HA_iter`/`HA_pto`/`HA_size`) and the
/// general `HA_pred` are omitted: memory constructs are never produced, and
/// [`Self::TermEq`] is the only predicate form (the `pred_eq`/2 discipline).
///
/// **Declaration order is the wire format.** `codec.rs` assigns each variant a
/// tag by its position here, and those tags are part of the `inference.hspecs`
/// format — so a new variant is *appended*, never inserted beside a relative it
/// reads like ([`Self::All`] belongs next to [`Self::Ex`] by meaning and last
/// by contract).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum HAssert {
    /// `HA_true` — ⊤.
    True,
    /// `HA_false` — ⊥.
    False,
    /// `HA_not` — ¬H.
    Not(Box<HAssert>),
    /// `HA_and` — H ∧ H'.
    And(Box<HAssert>, Box<HAssert>),
    /// `Himpl p q` — p → q, the transparent De Morgan sugar
    /// `HA_not (HA_and p (HA_not q))`. Kept explicit so the printer renders
    /// `Himpl` without matching an encoding.
    Imp(Box<HAssert>, Box<HAssert>),
    /// `Hor p q` — p ∨ q, the transparent sugar
    /// `HA_not (HA_and (HA_not p) (HA_not q))`. Kept explicit like [`Self::Imp`].
    Or(Box<HAssert>, Box<HAssert>),
    /// `HA_ex` — ∃x. H, binding de Bruijn logical variable 0 in its body.
    Ex(Box<HAssert>),
    /// `term_eq a b` (= `HA_pred pred_eq [a; b]`) — the only predicate form.
    TermEq(HTerm, HTerm),
    /// `HA_has_type τ t` — τ denotes a value of numeric type `t`.
    HasType(HTerm, HNumType),
    /// `HA_defined τ` — τ denotes (is not `None`).
    Defined(HTerm),
    /// `HA_app_ok f τs` — the application `f(τs)` is realized, at any result
    /// arity (including a `void` result, where the scalar `T_app` is never
    /// defined).
    AppOk(HFnRef, Vec<HTerm>),
    /// `Hall` — ∀x. H, the transparent sugar `HA_not (HA_ex (HA_not H))`,
    /// binding de Bruijn logical variable 0 in its body exactly as
    /// [`Self::Ex`] does. Kept explicit like [`Self::Imp`]/[`Self::Or`] so the
    /// printer renders `Hall` without pattern-matching an encoding.
    ///
    /// Declared last on purpose, against meaning: see the type's own note —
    /// the tag order is the declaration order and is part of the wire format.
    All(Box<HAssert>),
}

impl HAssert {
    /// Conjunction with ⊤ as the (absorbed) identity: `⊤ ∧ x = x`,
    /// `x ∧ ⊤ = x`. Right-folding a statement list through this constructor is
    /// what keeps a translated obligation free of trailing `∧ ⊤` noise.
    #[must_use = "builds an assertion"]
    pub fn and(a: HAssert, b: HAssert) -> HAssert {
        match (a, b) {
            (HAssert::True, b) => b,
            (a, HAssert::True) => a,
            (a, b) => HAssert::And(Box::new(a), Box::new(b)),
        }
    }

    /// Implication with ⊤ simplified away: `⊤ → q = q`, `p → ⊤ = ⊤`.
    #[must_use = "builds an assertion"]
    pub fn imp(p: HAssert, q: HAssert) -> HAssert {
        match (p, q) {
            (_, HAssert::True) => HAssert::True,
            (HAssert::True, q) => q,
            (p, q) => HAssert::Imp(Box::new(p), Box::new(q)),
        }
    }

    /// Disjunction with ⊤ as the absorbing element (the dual of ⊤ being the
    /// identity for [`Self::and`]): `⊤ ∨ x = ⊤`, `x ∨ ⊤ = ⊤`.
    #[must_use = "builds an assertion"]
    pub fn or(a: HAssert, b: HAssert) -> HAssert {
        match (a, b) {
            (HAssert::True, _) | (_, HAssert::True) => HAssert::True,
            (a, b) => HAssert::Or(Box::new(a), Box::new(b)),
        }
    }

    /// Existential quantification, simplifying `∃x. ⊤ = ⊤`. The body binds de
    /// Bruijn logical variable 0.
    #[must_use = "builds an assertion"]
    pub fn ex(body: HAssert) -> HAssert {
        match body {
            HAssert::True => HAssert::True,
            body => HAssert::Ex(Box::new(body)),
        }
    }

    /// Universal quantification, simplifying `∀x. ⊤ = ⊤` — the dual of
    /// [`Self::ex`], and sound for the same reason: the assertion language's
    /// domain is never empty, so a body that claims nothing claims nothing
    /// under either quantifier. The body binds de Bruijn logical variable 0.
    #[must_use = "builds an assertion"]
    pub fn all(body: HAssert) -> HAssert {
        match body {
            HAssert::True => HAssert::True,
            body => HAssert::All(Box::new(body)),
        }
    }

    /// "τ is non-zero": `¬(τ = 0)`, mirroring the canonical `nz` helper. The
    /// zero constant is always i32 — every truthiness position (a relop
    /// result, a bool, a `&&`/`||` result, a bool-returning call) is i32 in
    /// WASM.
    #[must_use = "builds an assertion"]
    pub fn nz(t: HTerm) -> HAssert {
        HAssert::Not(Box::new(HAssert::TermEq(t, HTerm::Const(HConst::I32(0)))))
    }

    /// "τ is zero": `τ = 0` (strict term equality), the positive dual of
    /// [`Self::nz`].
    #[must_use = "builds an assertion"]
    pub fn eqz(t: HTerm) -> HAssert {
        HAssert::TermEq(t, HTerm::Const(HConst::I32(0)))
    }
}

/// Frame metadata of a reachability obligation — an `exists`- or
/// `unique`-quantified specification function whose body is retained in the
/// emitted module and reduced under vanilla semantics.
///
/// Mirrors the non-payload fields of wasm-verifier's `reachability_spec`
/// record (its `theories/Exists.v`): `reach_entry_arity` and
/// `reach_visible_locs`. The function reference itself stays symbolic (the
/// enclosing [`HSpecEntry::fn_symbol`]); `wasm-to-v` resolves it to the
/// record's `reach_func` index.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ReachMeta {
    /// Number of source-declared parameters, before the hidden trailing
    /// choice parameters codegen appends for each scalar `@`. Carried on the
    /// wire rather than re-derived from the function type, so producer drift
    /// surfaces as a loud consistency error downstream instead of an
    /// unprovable theorem.
    pub entry_arity: u32,
    /// The producer-declared source-visible frame slots, strictly ascending.
    /// `unique` compares exit states projected through this list; hidden
    /// choice parameters and compiler temporaries are deliberately excluded.
    pub visible_locs: Vec<u32>,
}

/// The quantifier kind of one obligation: which downstream judgment consumes
/// the entry's assertion.
///
/// A `Forall` payload is a free logical formula discharged denotationally
/// (`ValidSpec`, over unconstrained valuations); an `Exists`/`Unique` payload
/// is evaluated against the frame an actual execution reaches
/// (`ValidExistsSpec`/`ValidUniqueSpec`), so those kinds carry the
/// [`ReachMeta`] that judgment needs — and only those kinds can: a universal
/// entry with stray reachability metadata is unrepresentable.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SpecKind {
    /// A universally quantified obligation (`ValidSpec`).
    Forall,
    /// An at-least-one-path reachability obligation (`ValidExistsSpec`).
    Exists(ReachMeta),
    /// An exactly-one-observation reachability obligation
    /// (`ValidUniqueSpec`).
    Unique(ReachMeta),
}

/// One specification function's translated obligation: the function's own
/// symbol paired with its assertion and its quantifier kind.
///
/// The symbol lets `wasm-to-v` align an obligation with its emitted function
/// without depending on position — a spec block may also contain methods, so
/// entries are *not* index-aligned with the `inference.spec_funcs` list.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct HSpecEntry {
    /// The obligation's own function symbol.
    pub fn_symbol: HFnRef,
    /// The translated assertion.
    pub hassert: HAssert,
    /// The quantifier kind selecting the downstream judgment, with the
    /// reachability metadata for the non-universal kinds.
    pub kind: SpecKind,
}

impl HSpecEntry {
    #[must_use = "constructs an entry"]
    pub fn new(fn_symbol: HFnRef, hassert: HAssert, kind: SpecKind) -> Self {
        Self {
            fn_symbol,
            hassert,
            kind,
        }
    }
}

/// A whole program's obligations, keyed by folded specification name
/// (`fold_spec_name`), each mapping to its entries in source order.
pub type HSpecMap = FxHashMap<String, Vec<HSpecEntry>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn and_absorbs_true_on_either_side() {
        let p = HAssert::Defined(HTerm::Local(0));
        assert_eq!(HAssert::and(HAssert::True, p.clone()), p);
        assert_eq!(HAssert::and(p.clone(), HAssert::True), p);
        assert_eq!(
            HAssert::and(p.clone(), p.clone()),
            HAssert::And(Box::new(p.clone()), Box::new(p))
        );
    }

    #[test]
    fn imp_simplifies_true_antecedent_and_consequent() {
        let q = HAssert::Defined(HTerm::Local(0));
        // ⊤ → q = q
        assert_eq!(HAssert::imp(HAssert::True, q.clone()), q);
        // p → ⊤ = ⊤, even when p is non-trivial
        assert_eq!(HAssert::imp(q.clone(), HAssert::True), HAssert::True);
        // ⊤ → ⊤ = ⊤
        assert_eq!(HAssert::imp(HAssert::True, HAssert::True), HAssert::True);
        // otherwise an explicit Imp node
        assert_eq!(
            HAssert::imp(q.clone(), q.clone()),
            HAssert::Imp(Box::new(q.clone()), Box::new(q))
        );
    }

    #[test]
    fn or_absorbs_true_and_is_otherwise_explicit() {
        let p = HAssert::Defined(HTerm::Local(0));
        assert_eq!(HAssert::or(HAssert::True, p.clone()), HAssert::True);
        assert_eq!(HAssert::or(p.clone(), HAssert::True), HAssert::True);
        assert_eq!(
            HAssert::or(p.clone(), p.clone()),
            HAssert::Or(Box::new(p.clone()), Box::new(p))
        );
    }

    #[test]
    fn ex_simplifies_a_trivial_body() {
        assert_eq!(HAssert::ex(HAssert::True), HAssert::True);
        let body = HAssert::Defined(HTerm::LVar(0));
        assert_eq!(HAssert::ex(body.clone()), HAssert::Ex(Box::new(body)));
    }

    #[test]
    fn all_simplifies_a_trivial_body() {
        assert_eq!(HAssert::all(HAssert::True), HAssert::True);
        let body = HAssert::Defined(HTerm::LVar(0));
        assert_eq!(HAssert::all(body.clone()), HAssert::All(Box::new(body)));
    }

    /// The two binders are distinct nodes over the same body: nothing in the
    /// IR lets an `Ex` stand in for an `All`, which is the whole reason the
    /// universal binder is explicit rather than the `¬∃¬` encoding.
    #[test]
    fn all_is_not_ex_over_the_same_body() {
        let body = HAssert::Defined(HTerm::LVar(0));
        assert_ne!(HAssert::all(body.clone()), HAssert::ex(body));
    }

    #[test]
    fn nz_and_eqz_compare_against_the_i32_zero() {
        let t = HTerm::Local(3);
        assert_eq!(
            HAssert::nz(t.clone()),
            HAssert::Not(Box::new(HAssert::TermEq(
                t.clone(),
                HTerm::Const(HConst::I32(0))
            )))
        );
        assert_eq!(
            HAssert::eqz(t.clone()),
            HAssert::TermEq(t, HTerm::Const(HConst::I32(0)))
        );
    }

    /// Transcription of wasm-verifier's `prime_hspec1`
    /// (its `theories/examples/PrimeExample.v`): the single most important
    /// expressiveness anchor. Building it through the smart constructors must
    /// reproduce the hand-spelled explicit tree node-for-node — pinning slot
    /// numbering, the `assume`→`Himpl` / exists-arm→`HA_and` polarity split,
    /// the `&&` split into two `nz`, the `nz`/`term_eq` guard pair, the `HA_ex`
    /// placement, the `T_lvar 0` / `SX_S` choices, and the `HA_has_type`
    /// argument-typing guard each universal slot leads its antecedent with (the
    /// exists arm's witness needs none).
    #[test]
    fn canonical_prime_hspec1_structure() {
        // Shared sub-terms (PrimeExample.v).
        let n = || HTerm::Local(0); // n_term = T_local 0
        let m_then = || HTerm::Local(1); // m_then = T_local 1
        let m_ex = || HTerm::LVar(0); // exists-arm m = T_lvar 0
        let one = || HTerm::Const(HConst::I32(1));
        let gt =
            |l: HTerm, r: HTerm| HTerm::Relop(HNumType::I32, HRelop::GtS, Box::new(l), Box::new(r));
        let lt =
            |l: HTerm, r: HTerm| HTerm::Relop(HNumType::I32, HRelop::LtS, Box::new(l), Box::new(r));
        let rem = |l: HTerm, r: HTerm| {
            HTerm::Binop(HNumType::I32, HBinop::RemS, Box::new(l), Box::new(r))
        };
        let is_prime_call = || HTerm::App(HFnRef("is_prime".to_string()), vec![n()]);

        let built = HAssert::imp(
            HAssert::and(
                HAssert::HasType(n(), HNumType::I32),
                HAssert::nz(gt(n(), one())),
            ),
            HAssert::and(
                HAssert::imp(
                    HAssert::nz(is_prime_call()),
                    HAssert::imp(
                        HAssert::and(
                            HAssert::HasType(m_then(), HNumType::I32),
                            HAssert::and(
                                HAssert::nz(gt(m_then(), one())),
                                HAssert::nz(lt(m_then(), n())),
                            ),
                        ),
                        HAssert::nz(gt(rem(n(), m_then()), HTerm::Const(HConst::I32(0)))),
                    ),
                ),
                HAssert::imp(
                    HAssert::TermEq(is_prime_call(), HTerm::Const(HConst::I32(0))),
                    HAssert::ex(HAssert::and(
                        HAssert::and(HAssert::nz(gt(m_ex(), one())), HAssert::nz(lt(m_ex(), n()))),
                        HAssert::TermEq(rem(n(), m_ex()), HTerm::Const(HConst::I32(0))),
                    )),
                ),
            ),
        );

        // The same tree spelled with primitive constructors, mirroring the Coq
        // source line-for-line.
        let nz =
            |t: HTerm| HAssert::Not(Box::new(HAssert::TermEq(t, HTerm::Const(HConst::I32(0)))));
        let expected = HAssert::Imp(
            Box::new(HAssert::And(
                Box::new(HAssert::HasType(n(), HNumType::I32)),
                Box::new(nz(gt(n(), one()))),
            )),
            Box::new(HAssert::And(
                Box::new(HAssert::Imp(
                    Box::new(nz(is_prime_call())),
                    Box::new(HAssert::Imp(
                        Box::new(HAssert::And(
                            Box::new(HAssert::HasType(m_then(), HNumType::I32)),
                            Box::new(HAssert::And(
                                Box::new(nz(gt(m_then(), one()))),
                                Box::new(nz(lt(m_then(), n()))),
                            )),
                        )),
                        Box::new(nz(gt(rem(n(), m_then()), HTerm::Const(HConst::I32(0))))),
                    )),
                )),
                Box::new(HAssert::Imp(
                    Box::new(HAssert::TermEq(
                        is_prime_call(),
                        HTerm::Const(HConst::I32(0)),
                    )),
                    Box::new(HAssert::Ex(Box::new(HAssert::And(
                        Box::new(HAssert::And(
                            Box::new(nz(gt(m_ex(), one()))),
                            Box::new(nz(lt(m_ex(), n()))),
                        )),
                        Box::new(HAssert::TermEq(
                            rem(n(), m_ex()),
                            HTerm::Const(HConst::I32(0)),
                        )),
                    )))),
                )),
            )),
        );

        assert_eq!(built, expected);
    }
}
