//! Codec for the `inference.spec_funcs` custom section the merge carries
//! through.
//!
//! Codegen emits this section into the main module to record, per spec, the
//! WASM function indices the Rocq translator must turn into proof obligations.
//! The merge removes imports and shifts function indices, so the embedded
//! indices are stale post-link unless rewritten. This module decodes the
//! payload to `(spec_name, [func_idx])` pairs, the merge remaps each index
//! through `Plan::map_main_func`, and [`encode`] re-emits the canonical bytes.
//!
//! ## Payload format (LEB128 u32 throughout)
//!
//! ```text
//! version              -- format version (must equal `VERSION`)
//! count                -- number of (spec_name, indices) pairs
//! repeat `count` times:
//!   name_len  name_bytes(utf-8)
//!   idx_count repeat `idx_count` times: func_idx
//! ```
//!
//! The format mirrors `inference_wasm_codegen::spec_section`; the linker keeps a
//! self-contained copy rather than depend on the codegen crate. The decoder is
//! fully bounds-checked: a malformed external `.wasm` (or a corrupt main module)
//! must surface a clean [`LinkError`], never a panic or an unbounded allocation.

use inf_wasmparser::BinaryReader;

use crate::LinkError;

/// The custom-section name carrying per-spec function indices. Kept in
/// lock-step with `inference_wasm_codegen`'s emitter and the `wasm-to-v`
/// decoder; the linker keeps its own copy to avoid depending on the codegen
/// crate.
pub(crate) const SECTION_NAME: &str = "inference.spec_funcs";

/// Wire-format version. Kept in lock-step with the codegen emitter.
const VERSION: u32 = 1;

/// Defensive upper bound on a single spec name's length, matching the decoder
/// in `wasm-to-v`. A hand-crafted payload could advertise a far longer name;
/// cap it so the per-name allocation stays bounded.
const MAX_SPEC_NAME_LEN: usize = 255;

/// Decodes the `inference.spec_funcs` payload into `(spec_name, [func_idx])`
/// pairs, preserving the encoded order so a round-trip is byte-stable.
///
/// # Errors
///
/// Returns [`LinkError::Parse`] on any malformed input: an unrecognised
/// version, a truncated LEB128, invalid UTF-8 in a spec name, an
/// over-advertised pair/index count, or a name exceeding [`MAX_SPEC_NAME_LEN`].
pub(crate) fn decode(data: &[u8]) -> Result<Vec<(String, Vec<u32>)>, LinkError> {
    let mut reader = BinaryReader::new(data, 0);

    let version = reader
        .read_var_u32()
        .map_err(|e| LinkError::Parse(format!("spec_funcs section: truncated version: {e}")))?;
    if version != VERSION {
        return Err(LinkError::Parse(format!(
            "spec_funcs section: unsupported version {version} (expected {VERSION})"
        )));
    }

    let count = reader
        .read_var_u32()
        .map_err(|e| LinkError::Parse(format!("spec_funcs section: truncated count: {e}")))?;
    // Each pair consumes at least two payload bytes (a name-length LEB128 and an
    // indices-count LEB128), so a count exceeding half the remaining bytes is a
    // malformed advertisement; reject before allocating.
    if count as usize > reader.bytes_remaining() / 2 {
        return Err(LinkError::Parse(
            "spec_funcs section: declared pair count exceeds remaining payload".into(),
        ));
    }

    let mut out: Vec<(String, Vec<u32>)> = Vec::with_capacity(count as usize);
    for _ in 0..count {
        // `read_string` returns a borrowed `&str` into the payload (no
        // allocation). Enforce the length cap on that borrow *before* copying it
        // into an owned `String`, so a hand-crafted payload advertising a large
        // in-bounds name cannot force a large transient allocation ahead of the
        // cap — keeping the decoder's "bounded allocation" guarantee intact.
        let name = reader.read_string().map_err(|e| {
            LinkError::Parse(format!("spec_funcs section: invalid spec name: {e}"))
        })?;
        if name.len() > MAX_SPEC_NAME_LEN {
            return Err(LinkError::Parse(format!(
                "spec_funcs section: spec name length {} exceeds cap {MAX_SPEC_NAME_LEN}",
                name.len()
            )));
        }
        let name = name.to_string();

        let idx_count = reader.read_var_u32().map_err(|e| {
            LinkError::Parse(format!("spec_funcs section: truncated indices count: {e}"))
        })?;
        // Each index consumes at least one payload byte, so `idx_count` cannot
        // legitimately exceed the remaining payload.
        if idx_count as usize > reader.bytes_remaining() {
            return Err(LinkError::Parse(
                "spec_funcs section: declared index count exceeds remaining payload".into(),
            ));
        }

        let mut indices = Vec::with_capacity(idx_count as usize);
        for _ in 0..idx_count {
            let idx = reader.read_var_u32().map_err(|e| {
                LinkError::Parse(format!("spec_funcs section: truncated index: {e}"))
            })?;
            indices.push(idx);
        }
        out.push((name, indices));
    }

    // Every declared entry has been consumed; any remaining bytes are trailing
    // garbage the count does not cover. A corrupt or version-skewed section would
    // re-encode without them, silently dropping data — reject it, matching the
    // fail-closed posture of the truncation checks above.
    if reader.bytes_remaining() != 0 {
        return Err(LinkError::Parse(format!(
            "spec_funcs section: {} trailing byte(s) after {count} declared entries",
            reader.bytes_remaining()
        )));
    }

    Ok(out)
}

/// Encodes `(spec_name, [func_idx])` pairs into the canonical payload bytes.
///
/// The encoded order matches the input order; the merge preserves the decoded
/// order, which is the encoder's sorted-by-name order, so a decode/remap/encode
/// round-trip stays byte-stable.
pub(crate) fn encode(pairs: &[(String, Vec<u32>)]) -> Vec<u8> {
    use wasm_encoder::Encode;

    let mut payload = Vec::new();
    VERSION.encode(&mut payload);
    let count = u32::try_from(pairs.len()).expect("more than u32::MAX specs");
    count.encode(&mut payload);

    for (name, indices) in pairs {
        let name_bytes = name.as_bytes();
        let name_len = u32::try_from(name_bytes.len()).expect("spec name longer than u32::MAX");
        let idx_count = u32::try_from(indices.len()).expect("more than u32::MAX indices per spec");

        name_len.encode(&mut payload);
        payload.extend_from_slice(name_bytes);
        idx_count.encode(&mut payload);
        for idx in indices {
            idx.encode(&mut payload);
        }
    }

    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_two_spec_payload() {
        let pairs = vec![
            ("A".to_string(), vec![2, 3]),
            ("B".to_string(), vec![5]),
        ];
        let bytes = encode(&pairs);
        // version=1, count=2, len=1 'A', idxc=2, 2,3, len=1 'B', idxc=1, 5
        assert_eq!(bytes, vec![1, 2, 1, b'A', 2, 2, 3, 1, b'B', 1, 5]);
        assert_eq!(decode(&bytes).unwrap(), pairs);
    }

    #[test]
    fn empty_payload_round_trips() {
        let pairs: Vec<(String, Vec<u32>)> = Vec::new();
        let bytes = encode(&pairs);
        assert_eq!(bytes, vec![1, 0]);
        assert_eq!(decode(&bytes).unwrap(), pairs);
    }

    #[test]
    fn rejects_an_unsupported_version() {
        // version=2, count=0
        let err = decode(&[2, 0]).unwrap_err();
        assert!(matches!(err, LinkError::Parse(_)), "got {err:?}");
    }

    #[test]
    fn rejects_an_over_advertised_pair_count() {
        // version=1, count=255 in a 3-byte payload.
        let err = decode(&[1, 255, 1]).unwrap_err();
        assert!(matches!(err, LinkError::Parse(_)), "got {err:?}");
    }

    #[test]
    fn rejects_a_truncated_index() {
        // version=1, count=1, name_len=1 'S', idx_count=1, <missing index>
        let err = decode(&[1, 1, 1, b'S', 1]).unwrap_err();
        assert!(matches!(err, LinkError::Parse(_)), "got {err:?}");
    }

    #[test]
    fn rejects_trailing_bytes_after_the_declared_entries() {
        // A well-formed payload followed by extra bytes the count does not cover.
        // Silently dropping the trailing bytes (the prior behavior) would mask a
        // corrupt or version-skewed section; the decoder must fail closed, matching
        // the other truncation checks in this codec.
        let mut bytes = encode(&[("S".to_string(), vec![0])]);
        bytes.extend_from_slice(&[0xff, 0xff]);
        let err = decode(&bytes).unwrap_err();
        assert!(
            matches!(&err, LinkError::Parse(msg) if msg.contains("trailing")),
            "expected a Parse error naming the trailing bytes, got {err:?}"
        );
    }
}
