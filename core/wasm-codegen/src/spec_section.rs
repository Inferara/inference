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
//! version              : LEB128 u32  -- format version (1 = legacy, 2 = with kinds)
//! count                : LEB128 u32  -- number of (spec_name, indices) pairs
//! repeated `count` times:
//!   spec_name_len      : LEB128 u32
//!   spec_name_bytes    : utf-8       -- not NUL-terminated
//!   indices_count      : LEB128 u32
//!   repeated `indices_count` times:
//!     func_idx         : LEB128 u32
//!   -- version 2 only: one obligation-kind byte per index, same order:
//!   repeated `indices_count` times (v2 only):
//!     kind_byte        : u8          -- 0 = Spec, 1 = Exists, 2 = Unique
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
//!
//! ## Obligation kinds (version 2)
//!
//! Version 1 carried only the function index, implying a single universal
//! ("for-all") proof obligation per spec function (`ValidSpec` downstream).
//! Inference also has `exists`- and `unique`-quantified spec functions, whose
//! downstream obligation is `ValidExistsSpec` / `ValidUniqueSpec` — predicates
//! that assert existential reachability, *strictly more* than the trap-freedom
//! a universal `ValidSpec` asserts. The vanilla WASM body no longer carries the
//! quantifier (the `0xfc` wrapper opcode is suppressed for spec functions), so
//! the obligation kind must travel as metadata here for the translator to pick
//! the right predicate.
//!
//! The encoder emits **version 1** whenever every obligation is the default
//! [`SpecObligationKind::Spec`], so all pre-existing `forall`/regular modules
//! stay byte-identical; it emits **version 2** (with the trailing kind bytes)
//! only when at least one `exists`/`unique` obligation must be carried. Both
//! decoders (the linker and the Rocq translator) accept either version.

use rustc_hash::FxHashMap;
use wasm_encoder::{CustomSection, Encode, Section, SectionId};

/// Name of the custom WASM section that carries the per-spec function-index
/// map. Re-exported from the crate root as `SPEC_FUNCS_SECTION_NAME`.
pub const SECTION_NAME: &str = "inference.spec_funcs";

/// Legacy wire-format version of the `inference.spec_funcs` payload: function
/// indices only, no obligation-kind bytes. Re-exported from the crate root as
/// `SPEC_FUNCS_SECTION_VERSION`. The encoder still emits this version whenever
/// every obligation is the default [`SpecObligationKind::Spec`], so modules
/// without `exists`/`unique` specs stay byte-identical to pre-kinds output.
pub const SECTION_VERSION: u32 = 1;

/// Wire-format version that additionally carries one [`SpecObligationKind`]
/// byte per function index (see the module docs). Emitted only when at least
/// one obligation is not the default `Spec` kind. Both decoders accept it
/// alongside [`SECTION_VERSION`].
pub const SECTION_VERSION_WITH_KINDS: u32 = 2;

/// The downstream proof obligation a spec function carries, recovered from the
/// quantifier on its (now-vanilla) body.
///
/// Selects which Rocq predicate the translator emits: `Spec` → `ValidSpec`
/// (universal trap-freedom), `Exists` → `ValidExistsSpec` (existential
/// reachability), `Unique` → `ValidUniqueSpec` (a unique witness). A `forall`,
/// regular, or `assume` body maps to `Spec`; only `exists`/`unique` bodies need
/// a distinct kind.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SpecObligationKind {
    /// Universal safety obligation — `forall`, regular, or `assume` bodies.
    #[default]
    Spec,
    /// Existential-reachability obligation — an `exists`-quantified body.
    Exists,
    /// Unique-witness obligation — a `unique`-quantified body.
    Unique,
}

impl SpecObligationKind {
    /// Wire encoding of this kind (a single payload byte in version 2).
    #[must_use]
    pub fn to_byte(self) -> u8 {
        match self {
            SpecObligationKind::Spec => 0,
            SpecObligationKind::Exists => 1,
            SpecObligationKind::Unique => 2,
        }
    }

    /// Decodes a wire byte, or `None` for an unrecognised value.
    #[must_use]
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(SpecObligationKind::Spec),
            1 => Some(SpecObligationKind::Exists),
            2 => Some(SpecObligationKind::Unique),
            _ => None,
        }
    }
}

/// Per-spec function obligations: each spec name maps to its `(func_idx, kind)`
/// pairs, in registration order. The payload codec's in-memory shape.
pub(crate) type SpecObligations = FxHashMap<String, Vec<(u32, SpecObligationKind)>>;

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

/// Encodes the spec obligations into the canonical payload bytes.
///
/// Emits the legacy [`SECTION_VERSION`] layout (indices only) when every
/// obligation is the default [`SpecObligationKind::Spec`], so modules without
/// `exists`/`unique` specs stay byte-identical to pre-kinds output; emits
/// [`SECTION_VERSION_WITH_KINDS`] (a trailing kind byte per index) otherwise.
pub(crate) fn encode_payload(map: &SpecObligations) -> Vec<u8> {
    let mut entries: Vec<(&str, &[(u32, SpecObligationKind)])> = map
        .iter()
        .map(|(name, items)| (name.as_str(), items.as_slice()))
        .collect();
    entries.sort_unstable_by(|a, b| a.0.cmp(b.0));

    let has_kinds = entries
        .iter()
        .any(|(_, items)| items.iter().any(|(_, k)| *k != SpecObligationKind::Spec));
    let version = if has_kinds {
        SECTION_VERSION_WITH_KINDS
    } else {
        SECTION_VERSION
    };

    let count = u32::try_from(entries.len())
        .expect("more than u32::MAX specs cannot fit in a WASM custom section");

    let mut payload = Vec::new();
    version.encode(&mut payload);
    count.encode(&mut payload);

    for (spec_name, items) in entries {
        let name_bytes = spec_name.as_bytes();
        let name_len = u32::try_from(name_bytes.len())
            .expect("spec name longer than u32::MAX bytes");
        let idx_count = u32::try_from(items.len())
            .expect("more than u32::MAX function indices per spec");

        name_len.encode(&mut payload);
        payload.extend_from_slice(name_bytes);
        idx_count.encode(&mut payload);
        for (idx, _) in items {
            idx.encode(&mut payload);
        }
        // Version 2 appends the kind bytes after all indices for this spec, in
        // the same order; version 1 omits them entirely.
        if has_kinds {
            for (_, kind) in items {
                payload.push(kind.to_byte());
            }
        }
    }

    payload
}

/// A `wasm_encoder::Section` carrying the encoded spec-name → obligations map.
pub(crate) struct SpecFuncSection {
    payload: Vec<u8>,
}

impl SpecFuncSection {
    pub(crate) fn new(map: &SpecObligations) -> Self {
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

    /// Builds a [`SpecObligations`] map from `(name, [(idx, kind)])` literals.
    fn obligations(
        entries: &[(&str, &[(u32, SpecObligationKind)])],
    ) -> SpecObligations {
        entries
            .iter()
            .map(|(name, items)| ((*name).to_string(), items.to_vec()))
            .collect()
    }

    /// All-`Spec` sugar: `(name, [idx])` pairs with the default obligation kind.
    fn spec_only(entries: &[(&str, &[u32])]) -> SpecObligations {
        entries
            .iter()
            .map(|(name, indices)| {
                let items = indices
                    .iter()
                    .map(|&idx| (idx, SpecObligationKind::Spec))
                    .collect();
                ((*name).to_string(), items)
            })
            .collect()
    }

    #[test]
    fn empty_map_encodes_zero_count() {
        let payload = encode_payload(&SpecObligations::default());
        // version=1, count=0
        assert_eq!(payload, vec![1, 0]);
    }

    #[test]
    fn single_spec_round_trip_bytes() {
        let payload = encode_payload(&spec_only(&[("S", &[3, 4])]));
        // version=1, count=1, name_len=1, 'S', idx_count=2, 3, 4
        assert_eq!(payload, vec![1, 1, 1, b'S', 2, 3, 4]);
    }

    #[test]
    fn sorted_by_spec_name() {
        let payload = encode_payload(&spec_only(&[("B", &[5]), ("A", &[2])]));
        // version=1, count=2, name_len=1, 'A', idx_count=1, 2, name_len=1, 'B', idx_count=1, 5
        assert_eq!(payload, vec![1, 2, 1, b'A', 1, 2, 1, b'B', 1, 5]);
    }

    #[test]
    fn all_spec_kinds_stay_version_one_byte_identical() {
        // An all-`Spec` obligations map must encode identically to the legacy
        // index-only payload, so existing modules are byte-stable.
        let with_kinds = encode_payload(&obligations(&[(
            "S",
            &[(3, SpecObligationKind::Spec), (4, SpecObligationKind::Spec)],
        )]));
        let legacy = encode_payload(&spec_only(&[("S", &[3, 4])]));
        assert_eq!(with_kinds, legacy);
        assert_eq!(with_kinds, vec![1, 1, 1, b'S', 2, 3, 4]);
    }

    #[test]
    fn exists_kind_promotes_to_version_two_with_kind_bytes() {
        let payload = encode_payload(&obligations(&[(
            "S",
            &[(3, SpecObligationKind::Exists)],
        )]));
        // version=2, count=1, name_len=1, 'S', idx_count=1, idx=3, kind=1 (Exists)
        assert_eq!(payload, vec![2, 1, 1, b'S', 1, 3, 1]);
    }

    #[test]
    fn mixed_kinds_emit_kinds_for_every_spec_in_version_two() {
        // One non-`Spec` kind anywhere promotes the whole payload to v2, so even
        // the all-`Spec` spec must carry its (zero) kind bytes.
        let payload = encode_payload(&obligations(&[
            ("A", &[(0, SpecObligationKind::Spec)]),
            ("B", &[(1, SpecObligationKind::Unique)]),
        ]));
        // v2, count=2,
        //   'A': name_len=1 'A', idx_count=1, idx=0, kind=0 (Spec)
        //   'B': name_len=1 'B', idx_count=1, idx=1, kind=2 (Unique)
        assert_eq!(payload, vec![2, 2, 1, b'A', 1, 0, 0, 1, b'B', 1, 1, 2]);
    }

    #[test]
    fn obligation_kind_byte_round_trip() {
        for kind in [
            SpecObligationKind::Spec,
            SpecObligationKind::Exists,
            SpecObligationKind::Unique,
        ] {
            assert_eq!(SpecObligationKind::from_byte(kind.to_byte()), Some(kind));
        }
        assert_eq!(SpecObligationKind::from_byte(3), None);
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
        let payload = encode_payload(&SpecObligations::default());
        let expected = u8::try_from(SECTION_VERSION).expect("version fits in a byte");
        assert_eq!(
            payload.first().copied(),
            Some(expected),
            "payload must lead with the version byte"
        );
    }
}
