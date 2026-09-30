//! Codegen-side emission of the `inference.bounds_elided` custom WASM section:
//! which of this module's functions hold an array access emitted without its
//! runtime bounds guard.
//!
//! ## What the section says, and what its absence says
//!
//! A listed function contains at least one array access whose index analysis
//! rule A056 proved in bounds and whose guard a build under
//! `BoundsChecks::OmitProven` therefore left out: at that access, safety rests
//! on the range analysis alone, and a flaw in it would read or write the
//! neighbouring linear memory rather than trap. **Absence means every dynamic
//! array access Inference emitted in the module keeps its guard**, which is what
//! every build under the default policy produces. A module another toolchain
//! produced has no Inference guard to omit, so its missing section says nothing
//! about how it indexes memory: the record is about this compiler's lowering,
//! not a verdict on the module. The section is never emitted empty.
//!
//! ## Why it exists
//!
//! The policy is a build setting, and nothing in the instructions says which
//! policy produced them: an access emitted without its guard is an ordinary load
//! or store. Whoever holds only the `.wasm` — an auditor, or a project that
//! keeps every guard and links a library that did not — could otherwise not
//! tell. Recording it per function rather than per module keeps the record true
//! across a link that splices in only some of a library's functions: the static
//! merge linker rewrites the indices into the merged module, as it does for
//! `inference.checked`, and keeps only the entries whose functions it kept.
//!
//! The record is emitted in both compilation modes, like the omission itself: a
//! proof build and a compile build under one policy stay the same bytes.
//!
//! ## Payload format
//!
//! The exact form of `inference.checked`, under its own name:
//!
//! ```text
//! version              : LEB128 u32  -- format version (this crate always writes 1)
//! count                : LEB128 u32  -- number of listed functions
//! repeated `count` times:
//!   func_idx           : LEB128 u32  -- strictly ascending
//! ```
//!
//! Indices name the instantiated function space — imports first, then the
//! module's own functions. As for `inference.checked`, version 2 is the same
//! section with no index list at all, which `infs` writes over an artifact
//! Binaryen has just renumbered; this crate never writes it.

use wasm_encoder::{Encode, Section, SectionId};

/// Name of the custom WASM section listing the functions that hold an access
/// emitted without its bounds guard. Re-exported from the crate root as
/// `BOUNDS_ELIDED_SECTION_NAME`, and copied by hand into the static merge linker
/// and into `infs`, neither of which depends on this crate.
pub const SECTION_NAME: &str = "inference.bounds_elided";

/// Wire-format version emitted at the head of the payload. Consumers must
/// reject unrecognised values. Re-exported from the crate root as
/// `BOUNDS_ELIDED_SECTION_VERSION`.
pub const SECTION_VERSION: u32 = 1;

/// Encodes `indices` into the canonical payload bytes, sorted and
/// de-duplicated so a module's bytes do not depend on the order code
/// generation recorded them in — one function can omit several guards.
#[must_use = "the payload is the section's contents"]
pub(crate) fn encode_payload(indices: &[u32]) -> Vec<u8> {
    let mut sorted: Vec<u32> = indices.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    let count =
        u32::try_from(sorted.len()).expect("a module cannot hold more than u32::MAX functions");

    let mut payload = Vec::new();
    SECTION_VERSION.encode(&mut payload);
    count.encode(&mut payload);
    for idx in sorted {
        idx.encode(&mut payload);
    }
    payload
}

/// A [`wasm_encoder::Section`] carrying the encoded `inference.bounds_elided`
/// payload.
pub(crate) struct BoundsElidedSection {
    payload: Vec<u8>,
}

impl BoundsElidedSection {
    #[must_use = "constructs the section to append to the module"]
    pub(crate) fn new(indices: &[u32]) -> Self {
        Self {
            payload: encode_payload(indices),
        }
    }
}

impl Encode for BoundsElidedSection {
    fn encode(&self, sink: &mut Vec<u8>) {
        wasm_encoder::CustomSection {
            name: SECTION_NAME.into(),
            data: (&self.payload[..]).into(),
        }
        .encode(sink);
    }
}

impl Section for BoundsElidedSection {
    fn id(&self) -> u8 {
        SectionId::Custom.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_function_listed_twice_is_listed_once() {
        // One function omitting two guards records its index twice.
        assert_eq!(encode_payload(&[4, 4, 1]), vec![1, 2, 1, 4]);
    }

    #[test]
    fn the_payload_shares_the_checked_sections_exact_form() {
        // The linker decodes both sections with one codec, so the two encoders
        // must agree on every list.
        for indices in [&[][..], &[0], &[3, 200, 7]] {
            assert_eq!(
                encode_payload(indices),
                crate::checked_section::encode_payload(indices)
            );
        }
    }
}
