//! The wire format two custom sections share: a list of function indices in one
//! module's instantiated function space, or the admission that its producer can
//! no longer name them.
//!
//! `inference.checked` (the functions that trap on arithmetic overflow, see
//! [`crate::checked`]) and `inference.bounds_elided` (the functions holding an
//! array access emitted without its bounds guard, see [`crate::bounds_elided`])
//! record different facts in the same shape and meet the same fate: each names
//! functions by index, so each survives a merge only once its indices are
//! rewritten into the output space, and each stops being true when a post-build
//! optimizer renumbers the functions it names. One codec keeps both forms, and
//! every refusal of a malformed payload, the same for the two.
//!
//! ## Payload format (LEB128 u32 throughout)
//!
//! ```text
//! version = 1          -- the exact form
//! count                -- number of listed functions
//! repeat `count` times: func_idx   -- strictly ascending
//! ```
//!
//! ```text
//! version = 2          -- the opaque form; nothing follows
//! ```
//!
//! The decoder is fully bounds-checked: a malformed external `.wasm` must
//! surface a clean [`LinkError`], never a panic or an unbounded allocation.

use inf_wasmparser::BinaryReader;

use crate::LinkError;

/// Wire-format version of the exact form, which names every listed function.
pub(crate) const EXACT_VERSION: u32 = 1;

/// Wire-format version of the opaque form, which names none of them. Code
/// generation never writes it: the compiler always knows which of its own
/// functions a fact holds of, and this version exists for the producers that
/// no longer do.
pub(crate) const OPAQUE_VERSION: u32 = 2;

/// What one module's function-list section says about its own functions.
///
/// The two variants are the two wire forms, and the difference matters to every
/// consumer: [`Self::Exact`] names the functions the section's fact holds of, so
/// a walk can ask whether it reached one of them, while [`Self::Opaque`] says
/// only that the module has at least one and that its producer can no longer
/// say which — which leaves every function of that module a candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FuncList {
    /// Exactly these functions, in the module's own index space.
    Exact(Vec<u32>),
    /// Some function of the module; which is no longer recorded.
    Opaque,
}

/// Decodes a function-list payload into what its module claims. `section`
/// opens every diagnostic, naming which of the sections sharing the format was
/// malformed.
///
/// # Errors
///
/// Returns [`LinkError::Parse`] on any malformed input: an unrecognised
/// version, a truncated LEB128, an over-advertised index count, indices that are
/// not strictly ascending, trailing bytes the count does not cover, or any byte
/// at all after an opaque payload's version.
///
/// The ascending requirement is enforced rather than assumed. The producer emits
/// the list sorted and de-duplicated, so a payload that is not is either corrupt
/// or from a producer that does not agree with this one about the format — and
/// silently accepting it would let the same function be listed twice, or an
/// index be read in an order the merged section could not reproduce.
///
/// An opaque payload is required to be exactly its version byte rather than
/// merely to *begin* with it. The two forms differ by what follows the version,
/// so a payload carrying an index list under the opaque version is a producer
/// that disagrees with this one about which form it wrote, and reading it as
/// opaque would discard the list it went to the trouble of writing.
pub(crate) fn decode(data: &[u8], section: &str) -> Result<FuncList, LinkError> {
    let mut reader = BinaryReader::new(data, 0);

    let version = reader
        .read_var_u32()
        .map_err(|e| LinkError::Parse(format!("{section}: truncated version: {e}")))?;
    if version == OPAQUE_VERSION {
        if reader.bytes_remaining() != 0 {
            return Err(LinkError::Parse(format!(
                "{section}: {} byte(s) follow the opaque version {OPAQUE_VERSION}, \
                 which carries no index list",
                reader.bytes_remaining()
            )));
        }
        return Ok(FuncList::Opaque);
    }
    if version != EXACT_VERSION {
        return Err(LinkError::Parse(format!(
            "{section}: unsupported version {version} \
             (expected {EXACT_VERSION} or {OPAQUE_VERSION})"
        )));
    }

    let count = reader
        .read_var_u32()
        .map_err(|e| LinkError::Parse(format!("{section}: truncated count: {e}")))?;
    // Each index consumes at least one payload byte, so a count exceeding the
    // remaining payload is a malformed advertisement; reject before allocating.
    if count as usize > reader.bytes_remaining() {
        return Err(LinkError::Parse(format!(
            "{section}: declared index count exceeds remaining payload"
        )));
    }

    let mut indices: Vec<u32> = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let idx = reader
            .read_var_u32()
            .map_err(|e| LinkError::Parse(format!("{section}: truncated index: {e}")))?;
        if let Some(&previous) = indices.last()
            && idx <= previous
        {
            return Err(LinkError::Parse(format!(
                "{section}: function index {idx} follows {previous}; the list must be \
                 strictly ascending"
            )));
        }
        indices.push(idx);
    }

    if reader.bytes_remaining() != 0 {
        return Err(LinkError::Parse(format!(
            "{section}: {} trailing byte(s) after {count} declared indices",
            reader.bytes_remaining()
        )));
    }

    Ok(FuncList::Exact(indices))
}

/// Encodes function indices into the canonical exact payload bytes.
///
/// The caller supplies them in any order; they are sorted and de-duplicated
/// here, because the merged list gathers the main module's indices and every
/// external's through different mappings and the result has to be one ascending
/// list either way.
#[must_use = "the payload is the section's contents"]
pub(crate) fn encode(indices: &[u32]) -> Vec<u8> {
    use wasm_encoder::Encode;

    let mut sorted: Vec<u32> = indices.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    let mut payload = Vec::new();
    EXACT_VERSION.encode(&mut payload);
    let count = u32::try_from(sorted.len()).expect(
        "a module's listed functions are a subset of its functions, whose count the WASM \
         format already caps at u32::MAX",
    );
    count.encode(&mut payload);
    for idx in sorted {
        idx.encode(&mut payload);
    }
    payload
}

/// The opaque payload: the version and nothing else.
///
/// Emitted only for a merge that absorbed an opaque input. Conservative by
/// construction — the merged indices of the exact inputs are known and could be
/// listed, but a list that named only them would say the fact holds of none of
/// the *other* module's bodies, which is the one thing an opaque input denies.
#[must_use = "the payload is the section's contents"]
pub(crate) fn encode_opaque() -> Vec<u8> {
    use wasm_encoder::Encode;

    let mut payload = Vec::new();
    OPAQUE_VERSION.encode(&mut payload);
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every diagnostic names the section it was decoding, so a malformed
    /// payload is reported against the section that carried it.
    #[test]
    fn a_diagnostic_names_the_section_it_decodes() {
        let err = decode(&[1, 1], "bounds_elided section").unwrap_err();
        assert!(
            matches!(&err, LinkError::Parse(msg) if msg.starts_with("bounds_elided section:")),
            "got {err:?}"
        );
        let err = decode(&[1, 9, 1], "bounds_elided section").unwrap_err();
        assert!(
            matches!(&err, LinkError::Parse(msg) if msg.starts_with("bounds_elided section:")),
            "got {err:?}"
        );
    }

    #[test]
    fn round_trips_an_ascending_list() {
        let indices = vec![0, 3, 200];
        let bytes = encode(&indices);
        // version=1, count=3, 0, 3, 200 (two LEB128 bytes)
        assert_eq!(bytes, vec![1, 3, 0, 3, 0xC8, 0x01]);
        assert_eq!(
            decode(&bytes, "checked section").unwrap(),
            FuncList::Exact(indices)
        );
    }

    #[test]
    fn the_encoder_sorts_and_deduplicates() {
        assert_eq!(encode(&[7, 2, 7, 2, 5]), encode(&[2, 5, 7]));
    }

    #[test]
    fn an_empty_list_round_trips() {
        let bytes = encode(&[]);
        assert_eq!(bytes, vec![1, 0]);
        assert_eq!(
            decode(&bytes, "checked section").unwrap(),
            FuncList::Exact(Vec::new())
        );
    }

    #[test]
    fn rejects_an_unsupported_version() {
        // version=3, count=0
        let err = decode(&[3, 0], "checked section").unwrap_err();
        assert!(
            matches!(&err, LinkError::Parse(msg) if msg.contains("unsupported version 3")),
            "got {err:?}"
        );
    }

    #[test]
    fn the_opaque_payload_round_trips() {
        let bytes = encode_opaque();
        assert_eq!(bytes, vec![2]);
        assert_eq!(decode(&bytes, "checked section").unwrap(), FuncList::Opaque);
    }

    #[test]
    fn rejects_an_opaque_payload_carrying_an_index_list() {
        // version=2 followed by what a version=1 payload would put there. The
        // two forms are told apart by the version alone, so a payload that
        // writes both is a producer disagreeing with this one about which it
        // wrote -- reading it as opaque would silently discard the list.
        let err = decode(&[2, 1, 4], "checked section").unwrap_err();
        assert!(
            matches!(&err, LinkError::Parse(msg) if msg.contains("opaque version 2")),
            "got {err:?}"
        );
    }

    #[test]
    fn the_two_forms_do_not_share_a_payload() {
        // A one-byte guard against a future edit that gave the opaque form the
        // exact form's version, which would make every exact payload decode as
        // "guarded everywhere" and every link fail closed for no reason.
        assert_ne!(encode_opaque(), encode(&[]));
    }

    #[test]
    fn rejects_an_over_advertised_index_count() {
        // version=1, count=255 in a payload holding one more byte.
        let err = decode(&[1, 255, 1], "checked section").unwrap_err();
        assert!(matches!(err, LinkError::Parse(_)), "got {err:?}");
    }

    #[test]
    fn rejects_a_truncated_index() {
        // version=1, count=1, <missing index>
        let err = decode(&[1, 1], "checked section").unwrap_err();
        assert!(matches!(err, LinkError::Parse(_)), "got {err:?}");
    }

    #[test]
    fn rejects_a_repeated_index() {
        // version=1, count=2, 4, 4
        let err = decode(&[1, 2, 4, 4], "checked section").unwrap_err();
        assert!(
            matches!(&err, LinkError::Parse(msg) if msg.contains("strictly ascending")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_a_descending_index() {
        // version=1, count=2, 9, 4
        let err = decode(&[1, 2, 9, 4], "checked section").unwrap_err();
        assert!(
            matches!(&err, LinkError::Parse(msg) if msg.contains("strictly ascending")),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_trailing_bytes_after_the_declared_indices() {
        let mut bytes = encode(&[0]);
        bytes.extend_from_slice(&[0xff, 0xff]);
        let err = decode(&bytes, "checked section").unwrap_err();
        assert!(
            matches!(&err, LinkError::Parse(msg) if msg.contains("trailing")),
            "got {err:?}"
        );
    }
}
