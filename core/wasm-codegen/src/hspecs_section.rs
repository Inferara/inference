//! Codegen-side emission of the `inference.hspecs` custom WASM section.
//!
//! The wire format and its [`HSpecMap`] data model live in the shared
//! `inference-hassert` leaf crate, so the linker (which carries the section
//! verbatim) and the Rocq translator (which consumes it) share one codec rather
//! than each keeping a copy. This module wraps [`inference_hassert::encode`] in
//! the [`wasm_encoder::Section`] the compiler appends after
//! `inference.spec_funcs`, and adds the fail-closed pre-encode gate the
//! infallible encoder cannot enforce itself.
//!
//! The obligation map is proof-mode-only and additive: proof-mode function
//! bodies are unchanged, so the section is a purely trailing addition to the
//! module.

use inference_hassert::{HSPECS_SECTION_NAME, HSpecMap, PayloadError};
use wasm_encoder::{CustomSection, Encode, Section, SectionId};

use crate::errors::CodegenError;

/// A [`wasm_encoder::Section`] carrying the encoded `inference.hspecs` payload.
///
/// Mirrors [`crate::spec_section::SpecFuncSection`], but the payload comes from
/// the shared `inference-hassert` codec rather than a local encoder.
pub(crate) struct HspecsSection {
    payload: Vec<u8>,
}

impl HspecsSection {
    /// Encodes `map` into the canonical `inference.hspecs` payload. The caller
    /// must have cleared the [`check_payload`] gate first: `encode` panics on a
    /// map its own decoder would reject (an over-deep tree or an out-of-range
    /// name), so the gate turns those into a clean [`CodegenError`] before the
    /// section is built.
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

/// Fail-closed gate against writing an `inference.hspecs` section the codec's
/// own decoder would reject.
///
/// [`inference_hassert::encode`] is infallible, but
/// [`inference_hassert::decode`] enforces an input contract — bounded tree
/// depth, non-empty names within a byte cap — so an unchecked map could
/// serialize into a section that fails its own round-trip in the linker and the
/// Rocq translator (a corrupt artifact), or overflow the encoder's stack on a
/// pathologically deep tree. This delegates to [`inference_hassert::validate`],
/// the single source of truth for that contract, and lifts the first violation
/// into a [`CodegenError`] naming the offending spec and identifier — so codegen
/// refuses before any bytes are written, rather than tripping `encode`'s
/// contract panic. Realistic specifications never approach the limits; only a
/// pathologically long statement chain or identifier can.
pub(crate) fn check_payload(map: &HSpecMap) -> Result<(), CodegenError> {
    inference_hassert::validate(map).map_err(payload_error_to_codegen)
}

/// Lifts an `inference-hassert` [`PayloadError`] into the corresponding
/// [`CodegenError`], attaching the numeric caps the diagnostics report.
fn payload_error_to_codegen(err: PayloadError) -> CodegenError {
    match err {
        PayloadError::TreeTooDeep { spec, function } => CodegenError::HspecTreeTooDeep {
            spec,
            function,
            max: inference_hassert::MAX_TREE_DEPTH,
        },
        PayloadError::SpecName { name, len } => CodegenError::HspecNameTooLong {
            spec: name.clone(),
            name,
            len,
            max: inference_hassert::MAX_NAME_LEN,
        },
        PayloadError::FunctionSymbol { spec, symbol, len } => CodegenError::HspecNameTooLong {
            spec,
            name: symbol,
            len,
            max: inference_hassert::MAX_NAME_LEN,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inference_hassert::{HAssert, HFnRef, HSpecEntry, MAX_NAME_LEN, MAX_TREE_DEPTH};

    fn map_with(spec: &str, function: &str, hassert: HAssert) -> HSpecMap {
        let mut map = HSpecMap::default();
        map.insert(
            spec.to_string(),
            vec![HSpecEntry::new(HFnRef(function.to_string()), hassert)],
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

    #[test]
    fn accepts_an_empty_map() {
        assert!(check_payload(&HSpecMap::default()).is_ok());
    }

    #[test]
    fn accepts_a_well_formed_obligation() {
        let map = map_with("S", "f", assert_spine(MAX_TREE_DEPTH));
        assert!(check_payload(&map).is_ok());
    }

    #[test]
    fn maps_an_over_deep_tree_to_hspec_tree_too_deep() {
        let map = map_with("S", "f", assert_spine(MAX_TREE_DEPTH + 1));
        let err = check_payload(&map).expect_err("an over-deep obligation must be rejected");
        assert!(
            matches!(
                err,
                CodegenError::HspecTreeTooDeep { ref spec, ref function, max }
                    if spec == "S" && function == "f" && max == MAX_TREE_DEPTH
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn maps_an_over_long_name_to_hspec_name_too_long() {
        let long = "z".repeat(MAX_NAME_LEN + 1);
        let map = map_with("S", &long, HAssert::True);
        let err = check_payload(&map).expect_err("an over-long symbol must be rejected");
        assert!(
            matches!(
                err,
                CodegenError::HspecNameTooLong { ref spec, ref name, len, max }
                    if spec == "S" && name == &long && len == MAX_NAME_LEN + 1 && max == MAX_NAME_LEN
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn the_reported_offender_is_deterministic() {
        // Two specs each carry an over-deep obligation; the validator's
        // sorted-name visit order makes the reported offender stable regardless
        // of map iteration order.
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
        let err = check_payload(&map).expect_err("both specs are over-deep");
        assert!(
            matches!(err, CodegenError::HspecTreeTooDeep { ref spec, .. } if spec == "alpha"),
            "the lexicographically first spec is named, got {err:?}"
        );
    }
}
