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
//! version              -- 1 = legacy (indices only), 2 = with kind bytes
//! count                -- number of (spec_name, indices) pairs
//! repeat `count` times:
//!   name_len  name_bytes(utf-8)
//!   idx_count repeat `idx_count` times: func_idx
//!   -- version 2 only: one obligation-kind byte per index, same order:
//!   repeat `idx_count` times (v2 only): kind_byte (u8)
//! ```
//!
//! The format mirrors `inference_wasm_codegen::spec_section`; the linker keeps a
//! self-contained copy rather than depend on the codegen crate. The obligation
//! kind selects the downstream Rocq predicate (`ValidSpec`/`ValidExistsSpec`/
//! `ValidUniqueSpec`); the linker does not interpret it — it carries each byte
//! through verbatim alongside the index it remaps. The decoder is fully
//! bounds-checked: a malformed external `.wasm` (or a corrupt main module) must
//! surface a clean [`LinkError`], never a panic or an unbounded allocation.

use inf_wasmparser::BinaryReader;

use crate::LinkError;

/// The custom-section name carrying per-spec function indices. Kept in
/// lock-step with `inference_wasm_codegen`'s emitter and the `wasm-to-v`
/// decoder; the linker keeps its own copy to avoid depending on the codegen
/// crate.
pub(crate) const SECTION_NAME: &str = "inference.spec_funcs";

/// Legacy wire-format version: function indices only. Kept in lock-step with
/// the codegen emitter.
const VERSION: u32 = 1;

/// Wire-format version that additionally carries one obligation-kind byte per
/// index. Kept in lock-step with the codegen emitter. The decoder accepts both
/// versions; [`encode`] re-emits whichever the kinds require.
const VERSION_WITH_KINDS: u32 = 2;

/// A decoded spec entry: the spec name and its `(func_idx, kind_byte)` pairs.
/// The kind byte is carried verbatim (0 for legacy v1 inputs); the linker
/// remaps the index but never the kind.
pub(crate) type SpecEntry = (String, Vec<(u32, u8)>);

/// Defensive upper bound on a single spec name's length, matching the decoder
/// in `wasm-to-v`. A hand-crafted payload could advertise a far longer name;
/// cap it so the per-name allocation stays bounded.
const MAX_SPEC_NAME_LEN: usize = 255;

/// Decodes the `inference.spec_funcs` payload into [`SpecEntry`] pairs,
/// preserving the encoded order so a round-trip is byte-stable.
///
/// Accepts both [`VERSION`] (legacy, indices only — every kind byte defaults to
/// `0`) and [`VERSION_WITH_KINDS`] (a trailing kind byte per index).
///
/// # Errors
///
/// Returns [`LinkError::Parse`] on any malformed input: an unrecognised
/// version, a truncated LEB128, invalid UTF-8 in a spec name, an
/// over-advertised pair/index count, or a name exceeding [`MAX_SPEC_NAME_LEN`].
pub(crate) fn decode(data: &[u8]) -> Result<Vec<SpecEntry>, LinkError> {
    let mut reader = BinaryReader::new(data, 0);

    let version = reader
        .read_var_u32()
        .map_err(|e| LinkError::Parse(format!("spec_funcs section: truncated version: {e}")))?;
    let has_kinds = match version {
        VERSION => false,
        VERSION_WITH_KINDS => true,
        other => {
            return Err(LinkError::Parse(format!(
                "spec_funcs section: unsupported version {other} \
                 (expected {VERSION} or {VERSION_WITH_KINDS})"
            )));
        }
    };

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

    let mut out: Vec<SpecEntry> = Vec::with_capacity(count as usize);
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
        // Each index consumes at least one payload byte; in v2 each also needs a
        // trailing kind byte, so the entry needs `idx_count` (v1) or
        // `2 * idx_count` (v2) bytes. Bound by the smaller v1 figure before
        // allocating — the per-index reads below still fail closed if the kind
        // bytes are truncated.
        if idx_count as usize > reader.bytes_remaining() {
            return Err(LinkError::Parse(
                "spec_funcs section: declared index count exceeds remaining payload".into(),
            ));
        }

        let mut indices: Vec<(u32, u8)> = Vec::with_capacity(idx_count as usize);
        for _ in 0..idx_count {
            let idx = reader.read_var_u32().map_err(|e| {
                LinkError::Parse(format!("spec_funcs section: truncated index: {e}"))
            })?;
            indices.push((idx, 0));
        }
        if has_kinds {
            for slot in &mut indices {
                let kind = reader.read_u8().map_err(|e| {
                    LinkError::Parse(format!("spec_funcs section: truncated kind byte: {e}"))
                })?;
                slot.1 = kind;
            }
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

/// Encodes [`SpecEntry`] pairs into the canonical payload bytes.
///
/// Emits [`VERSION`] (indices only) when every kind byte is `0`, so a v1 input
/// round-trips byte-identically; emits [`VERSION_WITH_KINDS`] (a trailing kind
/// byte per index) when any kind is non-zero. The encoded order matches the
/// input order; the merge preserves the decoded order, which is the encoder's
/// sorted-by-name order, so a decode/remap/encode round-trip stays byte-stable.
pub(crate) fn encode(pairs: &[SpecEntry]) -> Vec<u8> {
    use wasm_encoder::Encode;

    let has_kinds = pairs
        .iter()
        .any(|(_, indices)| indices.iter().any(|&(_, kind)| kind != 0));

    let mut payload = Vec::new();
    if has_kinds {
        VERSION_WITH_KINDS.encode(&mut payload);
    } else {
        VERSION.encode(&mut payload);
    }
    let count = u32::try_from(pairs.len()).expect("more than u32::MAX specs");
    count.encode(&mut payload);

    for (name, indices) in pairs {
        let name_bytes = name.as_bytes();
        let name_len = u32::try_from(name_bytes.len()).expect("spec name longer than u32::MAX");
        let idx_count = u32::try_from(indices.len()).expect("more than u32::MAX indices per spec");

        name_len.encode(&mut payload);
        payload.extend_from_slice(name_bytes);
        idx_count.encode(&mut payload);
        for &(idx, _) in indices {
            idx.encode(&mut payload);
        }
        if has_kinds {
            for &(_, kind) in indices {
                payload.push(kind);
            }
        }
    }

    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_two_spec_payload() {
        // All-zero kinds → legacy v1 layout, byte-identical to the pre-kinds
        // encoder so a v1 input survives a decode/encode round-trip unchanged.
        let pairs: Vec<SpecEntry> = vec![
            ("A".to_string(), vec![(2, 0), (3, 0)]),
            ("B".to_string(), vec![(5, 0)]),
        ];
        let bytes = encode(&pairs);
        // version=1, count=2, len=1 'A', idxc=2, 2,3, len=1 'B', idxc=1, 5
        assert_eq!(bytes, vec![1, 2, 1, b'A', 2, 2, 3, 1, b'B', 1, 5]);
        assert_eq!(decode(&bytes).unwrap(), pairs);
    }

    #[test]
    fn round_trips_a_v2_payload_with_kinds() {
        // A non-zero kind byte promotes the payload to v2; the kinds survive a
        // decode/encode round-trip alongside their (unremapped here) indices.
        let pairs: Vec<SpecEntry> = vec![
            ("A".to_string(), vec![(2, 0)]),
            ("B".to_string(), vec![(5, 1), (6, 2)]),
        ];
        let bytes = encode(&pairs);
        // v2, count=2,
        //   'A': len=1 'A', idxc=1, idx=2, kind=0
        //   'B': len=1 'B', idxc=2, idx=5, idx=6, kind=1, kind=2
        assert_eq!(
            bytes,
            vec![2, 2, 1, b'A', 1, 2, 0, 1, b'B', 2, 5, 6, 1, 2]
        );
        assert_eq!(decode(&bytes).unwrap(), pairs);
    }

    #[test]
    fn decodes_a_legacy_v1_payload_with_zero_kinds() {
        // A hand-crafted v1 payload (no kind bytes) decodes with every kind 0.
        let bytes = vec![1, 1, 1, b'S', 2, 3, 4];
        assert_eq!(
            decode(&bytes).unwrap(),
            vec![("S".to_string(), vec![(3, 0), (4, 0)])]
        );
    }

    #[test]
    fn empty_payload_round_trips() {
        let pairs: Vec<SpecEntry> = Vec::new();
        let bytes = encode(&pairs);
        assert_eq!(bytes, vec![1, 0]);
        assert_eq!(decode(&bytes).unwrap(), pairs);
    }

    #[test]
    fn rejects_an_unsupported_version() {
        // version=3 is neither v1 nor v2.
        let err = decode(&[3, 0]).unwrap_err();
        assert!(matches!(err, LinkError::Parse(_)), "got {err:?}");
    }

    #[test]
    fn rejects_a_truncated_v2_kind_byte() {
        // v2, count=1, name_len=1 'S', idx_count=1, idx=0, <missing kind byte>
        let err = decode(&[2, 1, 1, b'S', 1, 0]).unwrap_err();
        assert!(
            matches!(&err, LinkError::Parse(msg) if msg.contains("kind")),
            "expected a Parse error naming the missing kind byte, got {err:?}"
        );
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
        let mut bytes = encode(&[("S".to_string(), vec![(0, 0)])]);
        bytes.extend_from_slice(&[0xff, 0xff]);
        let err = decode(&bytes).unwrap_err();
        assert!(
            matches!(&err, LinkError::Parse(msg) if msg.contains("trailing")),
            "expected a Parse error naming the trailing bytes, got {err:?}"
        );
    }
}
