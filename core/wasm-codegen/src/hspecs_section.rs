//! Codegen-side emission of the `inference.hspecs` custom WASM section.
//!
//! The wire format and its [`HSpecMap`] data model live in the shared
//! `inference-hassert` leaf crate, so the linker (which carries the section
//! verbatim) and the Rocq translator (which consumes it) share one codec rather
//! than each keeping a copy. This module only wraps [`inference_hassert::encode`]
//! in the [`wasm_encoder::Section`] the compiler appends after
//! `inference.spec_funcs`, and adds the fail-closed depth guard the infallible
//! encoder cannot enforce itself.
//!
//! The obligation map is proof-mode-only and additive: proof-mode function
//! bodies are unchanged, so the section is a purely trailing addition to the
//! module.

use inference_hassert::{HAssert, HSPECS_SECTION_NAME, HSpecMap, HTerm, MAX_TREE_DEPTH};
use wasm_encoder::{CustomSection, Encode, Section, SectionId};

/// A [`wasm_encoder::Section`] carrying the encoded `inference.hspecs` payload.
///
/// Mirrors [`crate::spec_section::SpecFuncSection`], but the payload comes from
/// the shared `inference-hassert` codec rather than a local encoder.
pub(crate) struct HspecsSection {
    payload: Vec<u8>,
}

impl HspecsSection {
    /// Encodes `map` into the canonical `inference.hspecs` payload. The caller
    /// must have cleared the depth guard ([`check_tree_depths`]) first, since
    /// `encode` cannot signal an over-deep tree.
    pub(crate) fn new(map: &HSpecMap) -> Self {
        Self {
            payload: inference_hassert::encode(map),
        }
    }
}

impl Encode for HspecsSection {
    fn encode(&self, sink: &mut Vec<u8>) {
        CustomSection {
            name: HSPECS_SECTION_NAME.into(),
            data: (&self.payload[..]).into(),
        }
        .encode(sink);
    }
}

impl Section for HspecsSection {
    fn id(&self) -> u8 {
        SectionId::Custom.into()
    }
}

/// The offending obligation identified by [`check_tree_depths`]: the spec whose
/// obligation is too deep and the function symbol it belongs to.
pub(crate) struct TreeTooDeep {
    pub(crate) spec: String,
    pub(crate) function: String,
}

/// Fail-closed guard against writing an `inference.hspecs` section the codec's
/// own decoder would reject.
///
/// [`inference_hassert::encode`] is infallible, but [`inference_hassert::decode`]
/// caps assertion/term nesting at [`MAX_TREE_DEPTH`] so an adversarial payload
/// cannot overflow the decoder's stack. Emitting a tree past that cap would
/// therefore produce a section that fails its own round-trip in the linker and
/// the Rocq translator — a corrupt artifact. This guard measures each
/// obligation's depth exactly as the decoder counts it and returns the first
/// offender, so codegen can refuse before any bytes are written. Specs are
/// visited in sorted name order so the reported offender is deterministic.
///
/// Realistic specifications are orders of magnitude below the cap; only a
/// pathologically long statement chain (the right-folded `And`/`Imp` spine
/// grows one level per statement) can reach it.
pub(crate) fn check_tree_depths(map: &HSpecMap) -> Result<(), TreeTooDeep> {
    let mut spec_names: Vec<&String> = map.keys().collect();
    spec_names.sort_unstable();
    for name in spec_names {
        for entry in &map[name] {
            if !assert_within_cap(&entry.hassert, 1) {
                return Err(TreeTooDeep {
                    spec: name.clone(),
                    function: entry.fn_symbol.0.clone(),
                });
            }
        }
    }
    Ok(())
}

/// Whether decoding `a` — the root entered at `depth` 1 — stays within
/// [`MAX_TREE_DEPTH`]. Mirrors `decode_assert`: a nested assertion is one level
/// deeper, while descending into a term restarts the counter (the decoder budgets
/// terms independently). The early return bounds this recursion at the cap, so it
/// cannot itself overflow on a deep input.
fn assert_within_cap(a: &HAssert, depth: usize) -> bool {
    if depth > MAX_TREE_DEPTH {
        return false;
    }
    match a {
        HAssert::True | HAssert::False => true,
        HAssert::Not(x) | HAssert::Ex(x) => assert_within_cap(x, depth + 1),
        HAssert::And(l, r) | HAssert::Imp(l, r) | HAssert::Or(l, r) => {
            assert_within_cap(l, depth + 1) && assert_within_cap(r, depth + 1)
        }
        HAssert::TermEq(l, r) => term_within_cap(l, 1) && term_within_cap(r, 1),
        HAssert::HasType(t, _) | HAssert::Defined(t) => term_within_cap(t, 1),
        HAssert::AppOk(_, args) => args.iter().all(|t| term_within_cap(t, 1)),
    }
}

/// Whether decoding `t` — a term tree entered at `depth` 1 from its assertion
/// position — stays within [`MAX_TREE_DEPTH`]. Mirrors `decode_term`: nested
/// terms and call arguments are one level deeper. Bounded like
/// [`assert_within_cap`].
fn term_within_cap(t: &HTerm, depth: usize) -> bool {
    if depth > MAX_TREE_DEPTH {
        return false;
    }
    match t {
        HTerm::Const(_) | HTerm::LVar(_) | HTerm::Local(_) => true,
        HTerm::App(_, args) => args.iter().all(|a| term_within_cap(a, depth + 1)),
        HTerm::Binop(_, _, l, r) | HTerm::Relop(_, _, l, r) => {
            term_within_cap(l, depth + 1) && term_within_cap(r, depth + 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inference_hassert::{HFnRef, HSpecEntry};

    fn map_with(hassert: HAssert) -> HSpecMap {
        let mut map = HSpecMap::default();
        map.insert(
            "S".to_string(),
            vec![HSpecEntry::new(HFnRef("f".to_string()), hassert)],
        );
        map
    }

    /// A right-leaning `Not` spine of `n` assertion nodes over a `True` leaf,
    /// decoded at depths `1..=n`.
    fn assert_spine(n: usize) -> HAssert {
        let mut a = HAssert::True;
        for _ in 1..n {
            a = HAssert::Not(Box::new(a));
        }
        a
    }

    /// A `Not` over a term spine of `n` `Binop` nodes: the assertion tree is
    /// shallow (2 nodes) but the term tree, entered fresh, is `n` deep.
    fn term_spine_assert(n: usize) -> HAssert {
        let mut t = HTerm::Local(0);
        for _ in 1..n {
            t = HTerm::Binop(
                inference_hassert::HNumType::I32,
                inference_hassert::HBinop::Add,
                Box::new(t),
                Box::new(HTerm::Const(inference_hassert::HConst::I32(0))),
            );
        }
        HAssert::Defined(t)
    }

    #[test]
    fn accepts_an_empty_map() {
        assert!(check_tree_depths(&HSpecMap::default()).is_ok());
    }

    #[test]
    fn accepts_an_assertion_spine_at_the_cap() {
        // An assertion tree exactly `MAX_TREE_DEPTH` deep decodes without error,
        // matching the codec's own boundary test.
        assert!(check_tree_depths(&map_with(assert_spine(MAX_TREE_DEPTH))).is_ok());
    }

    #[test]
    fn rejects_an_assertion_spine_past_the_cap() {
        let err = check_tree_depths(&map_with(assert_spine(MAX_TREE_DEPTH + 1)))
            .expect_err("an over-deep assertion tree must be rejected");
        assert_eq!(err.spec, "S");
        assert_eq!(err.function, "f");
    }

    #[test]
    fn a_deep_term_tree_is_measured_independently_of_the_assertion() {
        // The assertion nesting is trivial; the depth lives entirely in the term
        // tree, which the decoder budgets from a fresh counter. A term tree at the
        // cap is fine; one past it is rejected.
        assert!(check_tree_depths(&map_with(term_spine_assert(MAX_TREE_DEPTH))).is_ok());
        assert!(check_tree_depths(&map_with(term_spine_assert(MAX_TREE_DEPTH + 1))).is_err());
    }

    #[test]
    fn the_reported_offender_is_deterministic() {
        // Two specs each carry an over-deep obligation; the sorted-name visit
        // order makes the reported offender stable regardless of map iteration.
        let mut map = HSpecMap::default();
        map.insert(
            "zeta".to_string(),
            vec![HSpecEntry::new(
                HFnRef("z".to_string()),
                assert_spine(MAX_TREE_DEPTH + 1),
            )],
        );
        map.insert(
            "alpha".to_string(),
            vec![HSpecEntry::new(
                HFnRef("a".to_string()),
                assert_spine(MAX_TREE_DEPTH + 1),
            )],
        );
        let err = check_tree_depths(&map).expect_err("both specs are over-deep");
        assert_eq!(
            err.spec, "alpha",
            "the lexicographically first spec is named"
        );
    }
}
