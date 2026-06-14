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

/// Upper bound, in bytes, on a single spec name embedded in the
/// `inference.spec_funcs` payload.
///
/// Both decoders reject any longer name: the linker
/// (`core/wasm-linker/src/spec_funcs.rs`) and the Rocq translator
/// (`core/wasm-to-v/src/wasm_parser.rs`) each cap at the same value, the
/// latter inheriting it from `validate_rocq_identifier`'s `TooLong` rule.
/// Enforcing the cap here keeps codegen from emitting an artifact that would
/// fail its own downstream link/translate step.
pub(crate) const MAX_SPEC_NAME_LEN: usize = 255;

/// Verifies that every spec name in `map` fits within [`MAX_SPEC_NAME_LEN`].
///
/// The encoder writes names verbatim, so an over-long name would produce a
/// `.wasm` artifact that both downstream decoders reject. Checking here lets
/// codegen surface a clean diagnostic instead of deferring the failure to the
/// linker or translator.
///
/// # Errors
///
/// Returns the offending name and its byte length when any name exceeds the
/// cap, sorted-first by name for a deterministic message.
pub(crate) fn check_spec_name_lengths(
    map: &FxHashMap<String, Vec<u32>>,
) -> Result<(), SpecNameTooLong> {
    let mut over_long: Vec<&str> = map
        .keys()
        .filter(|name| name.len() > MAX_SPEC_NAME_LEN)
        .map(String::as_str)
        .collect();
    over_long.sort_unstable();
    match over_long.first() {
        Some(name) => Err(SpecNameTooLong {
            name: (*name).to_string(),
            len: name.len(),
        }),
        None => Ok(()),
    }
}

/// A spec name exceeded [`MAX_SPEC_NAME_LEN`] bytes during codegen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpecNameTooLong {
    pub(crate) name: String,
    pub(crate) len: usize,
}

/// Returns a human-readable reason if `qualified` is not a legal Rocq identifier,
/// or `None` when it is valid.
///
/// The file-qualified spec name is emitted verbatim into the
/// `<module>__<spec>_specs` definition and `valid_<module>__<spec>` theorem the
/// Rocq translator produces, so it must satisfy the translator's identifier
/// rules. This mirrors the load-bearing rules of
/// `wasm-to-v`'s `validate_rocq_identifier` — leading letter, allowed chars, and
/// no `__` run — so codegen can reject a bad name *before* writing any artifact
/// (the translator runs downstream of the `.wasm` write). The rules are
/// duplicated rather than imported because codegen sits upstream of `wasm-to-v`
/// in the pipeline and must not depend on it; the length and stdlib/keyword
/// denylist checks stay in their existing places ([`check_spec_name_lengths`] and
/// the translator) so this covers only the structural rules a spec name can trip
/// at the source level — chiefly the `__` run a leading-underscore spec name
/// (`spec _S` → `lib_geo__S`) produces after the module-path join.
#[must_use = "the reason is the return value"]
pub(crate) fn spec_name_rocq_invalidity_reason(qualified: &str) -> Option<String> {
    let mut chars = qualified.chars();
    match chars.next() {
        None => return Some("name is empty".to_string()),
        Some(first) if !first.is_ascii_alphabetic() => {
            return Some(format!("must start with a letter, found `{first}`"));
        }
        Some(_) => {}
    }
    for c in chars {
        if !(c.is_ascii_alphanumeric() || c == '_') {
            return Some(format!("contains the invalid character `{c}`"));
        }
    }
    if qualified.contains("__") {
        return Some("contains a `__` run, which Rocq reserves as the module/spec separator".to_string());
    }
    None
}

/// Encodes the spec map into the canonical payload bytes.
pub(crate) fn encode_payload(map: &FxHashMap<String, Vec<u32>>) -> Vec<u8> {
    let mut entries: Vec<(&str, &[u32])> = map
        .iter()
        .map(|(name, indices)| (name.as_str(), indices.as_slice()))
        .collect();
    entries.sort_unstable_by(|a, b| a.0.cmp(b.0));

    let count = u32::try_from(entries.len())
        .expect("more than u32::MAX specs cannot fit in a WASM custom section");

    let mut payload = Vec::new();
    SECTION_VERSION.encode(&mut payload);
    count.encode(&mut payload);

    for (spec_name, indices) in entries {
        let name_bytes = spec_name.as_bytes();
        let name_len = u32::try_from(name_bytes.len())
            .expect("spec name longer than u32::MAX bytes");
        let idx_count = u32::try_from(indices.len())
            .expect("more than u32::MAX function indices per spec");

        name_len.encode(&mut payload);
        payload.extend_from_slice(name_bytes);
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
    fn name_within_cap_passes_check() {
        let mut map: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        map.insert("a".repeat(MAX_SPEC_NAME_LEN), vec![0]);
        assert_eq!(check_spec_name_lengths(&map), Ok(()));
    }

    #[test]
    fn over_long_name_is_rejected() {
        let mut map: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let name = "a".repeat(MAX_SPEC_NAME_LEN + 1);
        map.insert(name.clone(), vec![0]);
        assert_eq!(
            check_spec_name_lengths(&map),
            Err(SpecNameTooLong {
                name,
                len: MAX_SPEC_NAME_LEN + 1,
            })
        );
    }

    #[test]
    fn reports_first_over_long_name_deterministically() {
        let mut map: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        // Two names share the over-cap length; the lexicographically smaller
        // one must be reported so the diagnostic is stable across hash orders.
        let long_b = format!("b{}", "x".repeat(MAX_SPEC_NAME_LEN));
        let long_a = format!("a{}", "x".repeat(MAX_SPEC_NAME_LEN));
        map.insert(long_b, vec![0]);
        map.insert(long_a.clone(), vec![1]);
        let err = check_spec_name_lengths(&map).expect_err("over-long names must reject");
        assert_eq!(err.name, long_a);
    }

    #[test]
    fn cap_matches_decoder_contract() {
        // Mirrors the cap both `inference.spec_funcs` decoders enforce.
        assert_eq!(MAX_SPEC_NAME_LEN, 255);
    }

    #[test]
    fn payload_starts_with_version_byte() {
        let map: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let payload = encode_payload(&map);
        let expected = u8::try_from(SECTION_VERSION).expect("version fits in a byte");
        assert_eq!(
            payload.first().copied(),
            Some(expected),
            "payload must lead with the version byte"
        );
    }
}
