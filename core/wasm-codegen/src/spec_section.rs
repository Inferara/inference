//! Custom WASM section embedding spec-originated function indices.
//!
//! Wraps the `(spec_name -> [func_idx])` map produced by codegen as a
//! `wasm_encoder::Section` named `inference.spec_funcs`. Downstream tools
//! (the Rocq translator) parse this section to recover per-spec function
//! indices from a bare `.wasm` binary without an out-of-band `CodegenOutput`.
//!
//! The section name lives in the vendor-prefixed `inference.*` namespace
//! rather than the `metadata.code.*` namespace reserved by the WebAssembly
//! tool-conventions repo for per-instruction code metadata.
//!
//! ## Payload format
//!
//! ```text
//! version              : LEB128 u32  -- format version (currently 1)
//! count                : LEB128 u32  -- number of (spec_name, indices) pairs
//! repeated `count` times:
//!   spec_name_len      : LEB128 u32
//!   spec_name_bytes    : utf-8       -- not NUL-terminated
//!   indices_count      : LEB128 u32
//!   repeated `indices_count` times:
//!     func_idx         : LEB128 u32
//! ```
//!
//! Entries are emitted sorted by spec name for deterministic, byte-stable
//! output.
//!
//! The leading `version` byte lets future revisions of the wire format break
//! compatibility loudly: a consumer reading a payload whose version it does
//! not recognise must refuse to translate, rather than treating the next
//! varuint32 as a spec count and silently misparsing the rest of the
//! payload.

use rustc_hash::FxHashMap;
use wasm_encoder::{CustomSection, Encode, Section, SectionId};

/// Name of the custom WASM section that carries the per-spec function-index
/// map. Re-exported from the crate root as `SPEC_FUNCS_SECTION_NAME`.
pub const SECTION_NAME: &str = "inference.spec_funcs";

/// Wire-format version emitted into the head of the `inference.spec_funcs`
/// payload. Consumers must reject unrecognised values. Re-exported from the
/// crate root as `SPEC_FUNCS_SECTION_VERSION` and consumed verbatim by the
/// `wasm-to-v` decoder so encoder and decoder share a single source of truth.
pub const SECTION_VERSION: u32 = 1;

/// Encodes the spec map into the canonical payload bytes.
pub(crate) fn encode_payload(map: &FxHashMap<String, Vec<u32>>) -> Vec<u8> {
    let mut entries: Vec<(&String, &Vec<u32>)> = map.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    let mut payload = Vec::new();
    SECTION_VERSION.encode(&mut payload);
    #[allow(clippy::cast_possible_truncation)]
    let count = entries.len() as u32;
    count.encode(&mut payload);

    for (spec_name, indices) in entries {
        let name_bytes = spec_name.as_bytes();
        #[allow(clippy::cast_possible_truncation)]
        let name_len = name_bytes.len() as u32;
        name_len.encode(&mut payload);
        payload.extend_from_slice(name_bytes);

        #[allow(clippy::cast_possible_truncation)]
        let idx_count = indices.len() as u32;
        idx_count.encode(&mut payload);
        for idx in indices {
            idx.encode(&mut payload);
        }
    }

    payload
}

/// A `wasm_encoder::Section` carrying the encoded spec-name → indices map.
pub(crate) struct SpecFuncSection {
    payload: Vec<u8>,
}

impl SpecFuncSection {
    pub(crate) fn new(map: &FxHashMap<String, Vec<u32>>) -> Self {
        Self {
            payload: encode_payload(map),
        }
    }
}

impl Encode for SpecFuncSection {
    fn encode(&self, sink: &mut Vec<u8>) {
        CustomSection {
            name: SECTION_NAME.into(),
            data: (&self.payload[..]).into(),
        }
        .encode(sink);
    }
}

impl Section for SpecFuncSection {
    fn id(&self) -> u8 {
        SectionId::Custom.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_map_encodes_zero_count() {
        let map: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let payload = encode_payload(&map);
        // version=1, count=0
        assert_eq!(payload, vec![1, 0]);
    }

    #[test]
    fn single_spec_round_trip_bytes() {
        let mut map: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        map.insert("S".into(), vec![3, 4]);
        let payload = encode_payload(&map);
        // version=1, count=1, name_len=1, 'S', idx_count=2, 3, 4
        assert_eq!(payload, vec![1, 1, 1, b'S', 2, 3, 4]);
    }

    #[test]
    fn sorted_by_spec_name() {
        let mut map: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        map.insert("B".into(), vec![5]);
        map.insert("A".into(), vec![2]);
        let payload = encode_payload(&map);
        // version=1, count=2, name_len=1, 'A', idx_count=1, 2, name_len=1, 'B', idx_count=1, 5
        assert_eq!(payload, vec![1, 2, 1, b'A', 1, 2, 1, b'B', 1, 5]);
    }

    #[test]
    fn payload_starts_with_version_byte() {
        let map: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let payload = encode_payload(&map);
        assert_eq!(
            payload.first().copied(),
            Some(SECTION_VERSION as u8),
            "payload must lead with the version byte"
        );
    }
}
