//! Codegen-side emission of the `inference.checked` custom WASM section: which
//! of this module's functions carry an overflow guard.
//!
//! ## What the section says, and what its absence says
//!
//! A listed function's body traps rather than wrapping when a `+`, `-`, `*` or
//! unary `-` leaves its type. **Absence means no Inference-emitted overflow
//! guard anywhere in the module** — which is exactly true of a module produced
//! by any other toolchain, since none of them emits one: a `.wasm` from `rustc`
//! wraps at every width in release builds, so reading "no section" as "nothing
//! here traps on overflow" is right for a foreign module rather than merely
//! safe. The section is therefore never emitted empty; presence and a non-empty
//! list are the same statement.
//!
//! ## Why it exists
//!
//! An `exists`/`unique`-quantified specification body is reduced by the
//! downstream judgment, whole activation included, so a trap in anything it
//! reaches through calls empties the observation set at the entry that reaches
//! it — making the claim false rather than narrower. Code generation rejects
//! that when it can see the callee's body (`P018`), and it cannot see an
//! `external fn`'s: `codegen` takes a typed context and a module name, and the
//! dependency bytes do not exist in the process until `link` is handed them, one
//! phase later (`core/inference/src/lib.rs`). So the compiler records what its
//! own bodies carry, and the linker — the phase that holds every module's
//! bytes — is the sole judge of a merged one.
//!
//! ## Payload format
//!
//! ```text
//! version              : LEB128 u32  -- format version (this crate always writes 1)
//! count                : LEB128 u32  -- number of guarded functions
//! repeated `count` times:
//!   func_idx           : LEB128 u32  -- strictly ascending
//! ```
//!
//! Indices name the instantiated function space — imports first, then the
//! module's own functions — the same space `inference.spec_funcs` uses, so the
//! linker rewrites both through one index mapping. They are emitted strictly
//! ascending, which makes the payload byte-stable and lets a decoder reject a
//! duplicate or an out-of-order entry rather than carry it.
//!
//! The leading `version` lets a future revision break compatibility loudly, for
//! the reason `spec_section` records: a consumer that read an unrecognised
//! payload would take the next varuint32 for a count and misparse the rest.
//!
//! ## The second form, which this crate never writes
//!
//! Version 2 is the same section with no index list at all — the version and
//! nothing else. It says that some function of the module carries a guard and
//! that which ones can no longer be told, and it exists for one producer: `infs`
//! writes it over an artifact Binaryen has just optimized. Binaryen carries an
//! unknown custom section through untouched while inlining, removing and
//! reordering the functions the indices name, so the exact list survives the
//! optimizer and stops being true; the marker replaces it with what is still
//! true. The static merge linker decodes both forms and reads the second as
//! "every function of that module", which refuses the verification it can no
//! longer perform.
//!
//! Code generation has no reason to write it. A module the compiler produced
//! knows its own guarded set — the list is recorded at the point the guard's
//! scratch slot is reserved — so the exact form is always available here, and
//! emitting the opaque one would throw away a fact this phase holds.

use wasm_encoder::{CustomSection, Encode, Section, SectionId};

/// Name of the custom WASM section listing the guarded functions. Re-exported
/// from the crate root as `CHECKED_SECTION_NAME`, and copied by hand into the
/// static merge linker and into `infs`, neither of which depends on this crate.
pub const SECTION_NAME: &str = "inference.checked";

/// Wire-format version emitted at the head of the `inference.checked` payload.
/// Consumers must reject unrecognised values.
///
/// Re-exported from the crate root as `CHECKED_SECTION_VERSION`. The static
/// merge linker's decoder holds a copy of this number rather than reading this
/// one — it does not depend on this crate — so the two are held together by a
/// test in the test suite rather than by the type system.
pub const SECTION_VERSION: u32 = 1;

/// Encodes `indices` into the canonical payload bytes.
///
/// The caller hands over the guarded functions in any order; the payload is
/// sorted and de-duplicated here so a module's bytes do not depend on the order
/// code generation happened to record them in.
#[must_use = "the payload is the section's contents"]
pub(crate) fn encode_payload(indices: &[u32]) -> Vec<u8> {
    let mut sorted: Vec<u32> = indices.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    let count = u32::try_from(sorted.len())
        .expect("a module cannot hold more than u32::MAX guarded functions");

    let mut payload = Vec::new();
    SECTION_VERSION.encode(&mut payload);
    count.encode(&mut payload);
    for idx in sorted {
        idx.encode(&mut payload);
    }
    payload
}

/// A [`wasm_encoder::Section`] carrying the encoded `inference.checked`
/// payload.
///
/// Mirrors [`crate::spec_section::SpecFuncSection`] and
/// [`crate::hspecs_section::HspecsSection`], the module's two other
/// verification-side custom sections.
pub(crate) struct CheckedSection {
    payload: Vec<u8>,
}

impl CheckedSection {
    #[must_use = "constructs the section to append to the module"]
    pub(crate) fn new(indices: &[u32]) -> Self {
        Self {
            payload: encode_payload(indices),
        }
    }
}

impl Encode for CheckedSection {
    fn encode(&self, sink: &mut Vec<u8>) {
        CustomSection {
            name: SECTION_NAME.into(),
            data: (&self.payload[..]).into(),
        }
        .encode(sink);
    }
}

impl Section for CheckedSection {
    fn id(&self) -> u8 {
        SectionId::Custom.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_index_encodes_version_count_and_index() {
        // version=1, count=1, idx=3
        assert_eq!(encode_payload(&[3]), vec![1, 1, 3]);
    }

    #[test]
    fn indices_are_sorted_and_deduplicated() {
        // The recorded order is whatever the compiler's own set iterated in, so
        // the payload has to impose one or the same program would produce two
        // different modules.
        assert_eq!(encode_payload(&[7, 2, 7, 2, 5]), vec![1, 3, 2, 5, 7]);
    }

    #[test]
    fn an_empty_list_still_encodes_a_well_formed_payload() {
        // Never actually emitted -- the module leaves the section out when
        // nothing is guarded -- but the encoder is total, so its answer for the
        // empty list is pinned rather than left to whatever it happens to do.
        assert_eq!(encode_payload(&[]), vec![1, 0]);
    }

    #[test]
    fn a_multibyte_index_is_leb128_encoded() {
        // Index 200 does not fit one LEB128 byte, so the count that precedes it
        // cannot be read as a byte offset into the payload.
        assert_eq!(encode_payload(&[200]), vec![1, 1, 0xC8, 0x01]);
    }

    #[test]
    fn the_payload_leads_with_the_version_byte() {
        let payload = encode_payload(&[0]);
        let expected = u8::try_from(SECTION_VERSION).expect("the version fits in a byte");
        assert_eq!(payload.first().copied(), Some(expected));
    }
}
