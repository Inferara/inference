//! The `contractspecv0` and `contractmetav0` custom sections: what a contract
//! tells the tooling around it, as against what it tells the host.
//!
//! The host needs neither. `contractspecv0` is the machine-readable description
//! of every method — its name, each parameter's name and type, and what it
//! returns — and it is what `stellar contract invoke` reads to turn
//! `-- add --a 2 --b 40` into the typed `Val` words a call carries, and what a
//! bindings generator reads to write a client. `contractmetav0` is key-value
//! metadata about the contract: `stellar contract info meta` displays it, and
//! tooling consults it too.
//!
//! One key there changes what tooling does with the spec. `soroban-spec`
//! 28.0.0 (`src/shaking.rs`) reads `rssdk_spec_shaking`, and a value of `"2"`
//! declares that the data section carries a marker for every user-defined type
//! and event entry the contract uses; tooling such as the `stellar` CLI may
//! then strip each such entry that has no marker. Function entries are always
//! kept (`soroban_spec::shaking::filter`), so the key would strip nothing this
//! toolchain writes today. It is never written all the same. The markers are
//! the Rust SDK's own mechanism, which that file says is not part of the
//! SEP-48 contract interface specification, and this toolchain writes none:
//! the claim would be false, and the day the spec describes a struct or an
//! enum, those entries would be stripped without a word. The one entry written
//! here is [`CONTRACT_META_TOOLCHAIN_KEY`].
//!
//! # The encoding
//!
//! Both sections are XDR, encoded by hand here as `meta.rs` encodes the
//! environment metadata. Four rules cover everything below:
//!
//! - every integer — a count, a length, an enum or union discriminant — is four
//!   bytes, big-endian;
//! - a `string<N>` is a `u32` length, the bytes, then zero bytes up to the next
//!   multiple of four;
//! - a vector, `T x<>` or `T x<N>`, is a `u32` count and then the elements;
//! - a union arm whose body is `void` is the discriminant alone.
//!
//! # Provenance
//!
//! Every type code, entry kind and field bound here is published in the
//! stellar-xdr repository at revision `9c9c145953e80990d6ff1ae3a6a973a0ce6d0694`,
//! the revision the `stellar-xdr` 28.0.0 crate vendors (its `xdr-version` file
//! names it). That is the crate `soroban-env-host` 28.0.2 pins and the revision
//! the `stellar` CLI 28.0.0 reports. Each of those constants names the `.x` file
//! that defines it.
//!
//! The two section names are not XDR. They are the names the Soroban tooling
//! agrees on, and each constant says where that tooling spells it. The meta
//! entry's key and value are this toolchain's own.

use wasm_encoder::CustomSection;

use crate::rewrite::MAX_EXPORT_NAME_BYTES;
use crate::val::{ValReturn, ValScalar};

/// Name of the custom section describing the contract's methods.
///
/// The name `soroban_spec::read::raw_from_wasm` (`soroban-spec` 28.0.0,
/// `src/read.rs`) looks for, which is the reader behind
/// `soroban_spec::read::from_wasm`, and the section `soroban-sdk-macros`
/// 27.0.6 writes every spec entry into through `link_section`.
pub const SPEC_SECTION_NAME: &str = "contractspecv0";

/// Name of the custom section carrying the contract's own metadata entries.
///
/// The section `soroban-sdk` 27.0.6's `contractmeta!` writes an `SCMetaV0`
/// entry into (`soroban-sdk-macros` 27.0.6, `link_section`), and the one
/// `soroban-spec` 28.0.0 (`src/shaking.rs`) documents the spec-shaking key in.
pub const CONTRACT_META_SECTION_NAME: &str = "contractmetav0";

/// The longest parameter name a contract method may have, in bytes.
///
/// `string name<30>` on `SCSpecFunctionInputV0`, `Stellar-contract-spec.x`: the
/// width of the field the spec records a parameter's name in. The name has to
/// be the one the author wrote, because `stellar contract invoke` turns it into
/// the `--<name>` flag a caller passes the argument by.
pub const MAX_INPUT_NAME_BYTES: usize = 30;

/// The key of the one `contractmetav0` entry this toolchain writes.
///
/// This toolchain's own choice, in the register of the `rsver` and `rssdkver`
/// keys `soroban-sdk` writes; no XDR definition or reader fixes it.
pub(crate) const CONTRACT_META_TOOLCHAIN_KEY: &str = "infver";

/// The value of that entry: the version the workspace declares
/// (`[workspace.package] version`), which this crate and `infs` both inherit,
/// so it is also what `infs version` prints.
///
/// It is the crate version and not a release identifier: it names a release
/// only as long as the workspace version is moved with each one.
pub const CONTRACT_META_TOOLCHAIN_VERSION: &str = env!("CARGO_PKG_VERSION");

/// `SC_SPEC_ENTRY_FUNCTION_V0`, `enum SCSpecEntryKind` in
/// `Stellar-contract-spec.x`: the `SCSpecEntry` union arm that describes one
/// contract method.
const SC_SPEC_ENTRY_FUNCTION_V0: u32 = 0;

/// `SC_SPEC_TYPE_BOOL`, `enum SCSpecType` in `Stellar-contract-spec.x`.
///
/// The spec type codes for `bool`, `u32` and `i32` coincide with the `Val` tags
/// `True`, `U32Val` and `I32Val` in `val.rs`. They are two different
/// enumerations that happen to agree on these three numbers, so each is stated
/// in its own place.
const SC_SPEC_TYPE_BOOL: u32 = 1;

/// `SC_SPEC_TYPE_VOID`, `enum SCSpecType` in `Stellar-contract-spec.x`,
/// recorded and emitted by nothing.
///
/// A method that returns nothing is described by an **empty** outputs vector,
/// not by a one-element vector holding this code. That is what `soroban-sdk`
/// writes — `derive_spec_fn.rs` maps a function with no return type to no
/// outputs — and what `stellar contract invoke` expects of such a method.
#[allow(dead_code)]
const SC_SPEC_TYPE_VOID: u32 = 2;

/// `SC_SPEC_TYPE_U32`, `enum SCSpecType` in `Stellar-contract-spec.x`.
const SC_SPEC_TYPE_U32: u32 = 4;

/// `SC_SPEC_TYPE_I32`, `enum SCSpecType` in `Stellar-contract-spec.x`.
const SC_SPEC_TYPE_I32: u32 = 5;

/// `SC_META_V0`, `enum SCMetaKind` in `Stellar-contract-meta.x`: the
/// `SCMetaEntry` union arm holding one key-value pair.
const SC_META_V0: u32 = 0;

/// The spec type code for a scalar the Val ABI carries.
///
/// Keyed on [`ValScalar`] rather than on the source type, so the code a spec
/// records is derived from exactly what the wrapper marshals, and a scalar
/// added to the Val ABI does not compile until it has a code here too.
pub(crate) fn spec_type(scalar: ValScalar) -> u32 {
    match scalar {
        ValScalar::Bool => SC_SPEC_TYPE_BOOL,
        ValScalar::U32 => SC_SPEC_TYPE_U32,
        ValScalar::I32 => SC_SPEC_TYPE_I32,
    }
}

/// One `SCSpecEntry` describing one contract method, as the `FunctionV0` arm:
/// the kind, an empty doc string, the method name, one input per parameter —
/// an empty doc string, the parameter's name and its type code — and the
/// outputs.
///
/// Callers have checked the bounds the XDR declares — at most
/// [`MAX_EXPORT_NAME_BYTES`] of method name, at most [`MAX_INPUT_NAME_BYTES`]
/// of parameter name — so the encoding never has to truncate.
///
/// # Panics
///
/// On a name over either bound, in every build. That is a violated internal
/// invariant, so a compiler bug, and a panic is the correct result: the entry
/// written instead would make a reader refuse the whole section, every other
/// method's entry with it.
pub(crate) fn function_entry(
    name: &str,
    inputs: &[(&str, ValScalar)],
    output: ValReturn,
) -> Vec<u8> {
    assert!(name.len() <= MAX_EXPORT_NAME_BYTES, "method name `{name}`");
    assert!(
        inputs
            .iter()
            .all(|(input_name, _)| input_name.len() <= MAX_INPUT_NAME_BYTES),
        "an input name of `{name}`"
    );
    let mut entry = Vec::new();
    put_u32(&mut entry, SC_SPEC_ENTRY_FUNCTION_V0);
    put_string(&mut entry, "");
    put_string(&mut entry, name);
    put_count(&mut entry, inputs.len());
    for (input_name, scalar) in inputs {
        put_string(&mut entry, "");
        put_string(&mut entry, input_name);
        put_u32(&mut entry, spec_type(*scalar));
    }
    match output {
        // No return is an empty outputs vector, never a `SC_SPEC_TYPE_VOID`.
        ValReturn::Void => put_count(&mut entry, 0),
        ValReturn::Scalar(scalar) => {
            put_count(&mut entry, 1);
            put_u32(&mut entry, spec_type(scalar));
        }
    }
    entry
}

/// The one `SCMetaEntry` this toolchain writes: the `SC_META_V0` kind, then the
/// key `infver`, then `version`.
pub(crate) fn contract_meta_payload(version: &str) -> Vec<u8> {
    let mut payload = Vec::new();
    put_u32(&mut payload, SC_META_V0);
    put_string(&mut payload, CONTRACT_META_TOOLCHAIN_KEY);
    put_string(&mut payload, version);
    payload
}

/// The `contractspecv0` section, ready to append to a module.
pub(crate) fn spec_section(payload: &[u8]) -> CustomSection<'_> {
    CustomSection {
        name: SPEC_SECTION_NAME.into(),
        data: payload.into(),
    }
}

/// The `contractmetav0` section, ready to append to a module.
pub(crate) fn contract_meta_section(payload: &[u8]) -> CustomSection<'_> {
    CustomSection {
        name: CONTRACT_META_SECTION_NAME.into(),
        data: payload.into(),
    }
}

/// An XDR `unsigned int`: four bytes, big-endian.
fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

/// An XDR vector's element count, or a string's length.
///
/// Every one written here is bounded far below `u32::MAX` — at most 32
/// parameters, at most one output, a name of at most 32 bytes, a version
/// string — so the saturation is never reached.
fn put_count(out: &mut Vec<u8>, count: usize) {
    put_u32(out, u32::try_from(count).unwrap_or(u32::MAX));
}

/// An XDR `string`: its length, its bytes, then the zero bytes that bring it to
/// a multiple of four.
fn put_string(out: &mut Vec<u8>, text: &str) {
    put_count(out, text.len());
    out.extend_from_slice(text.as_bytes());
    let padding = (4 - text.len() % 4) % 4;
    out.extend(std::iter::repeat_n(0, padding));
}

#[cfg(test)]
mod tests {
    use super::*;
    use inference_wasm_codegen::AbiType;

    fn hex(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn string(text: &str) -> String {
        let mut out = Vec::new();
        put_string(&mut out, text);
        hex(&out)
    }

    /// Every padding case a string can need — none, three, two and one zero
    /// bytes — and the empty string, which is a bare zero length.
    #[test]
    fn a_string_is_its_length_its_bytes_and_zero_padding_to_four() {
        assert_eq!(string(""), "00 00 00 00");
        assert_eq!(string("a"), "00 00 00 01 61 00 00 00");
        assert_eq!(string("ab"), "00 00 00 02 61 62 00 00");
        assert_eq!(string("abc"), "00 00 00 03 61 62 63 00");
        assert_eq!(string("abcd"), "00 00 00 04 61 62 63 64");
        assert_eq!(string("abcde"), "00 00 00 05 61 62 63 64 65 00 00 00");
    }

    /// `add(a: u32, b: u32) -> u32`, the reference contract's method, as the
    /// sixty bytes derived by hand from the XDR definitions.
    #[test]
    fn the_add_entry_is_the_sixty_derived_bytes() {
        let entry = function_entry(
            "add",
            &[("a", ValScalar::U32), ("b", ValScalar::U32)],
            ValReturn::Scalar(ValScalar::U32),
        );
        assert_eq!(
            hex(&entry),
            "00 00 00 00 \
             00 00 00 00 \
             00 00 00 03 61 64 64 00 \
             00 00 00 02 \
             00 00 00 00 00 00 00 01 61 00 00 00 00 00 00 04 \
             00 00 00 00 00 00 00 01 62 00 00 00 00 00 00 04 \
             00 00 00 01 00 00 00 04"
        );
        assert_eq!(entry.len(), 60);
    }

    /// `tick()`: no inputs and no return. The outputs vector is empty — a zero
    /// count and nothing after it — rather than holding `SC_SPEC_TYPE_VOID`.
    #[test]
    fn a_method_returning_nothing_has_no_outputs_rather_than_a_void_one() {
        let entry = function_entry("tick", &[], ValReturn::Void);
        assert_eq!(
            hex(&entry),
            "00 00 00 00 00 00 00 00 00 00 00 04 74 69 63 6b 00 00 00 00 00 00 00 00"
        );
        assert_eq!(entry.len(), 24);

        let with_inputs = function_entry("record", &[("a", ValScalar::U32)], ValReturn::Void);
        assert_eq!(&with_inputs[with_inputs.len() - 4..], &[0, 0, 0, 0]);
        let one_void_output: Vec<u8> = [1, SC_SPEC_TYPE_VOID]
            .iter()
            .flat_map(|word: &u32| word.to_be_bytes())
            .collect();
        assert!(
            !with_inputs.ends_with(&one_void_output),
            "a unit return must not be written as one void output"
        );
    }

    #[test]
    fn a_bool_is_type_code_one_in_both_positions() {
        let entry = function_entry(
            "negate",
            &[("b", ValScalar::Bool)],
            ValReturn::Scalar(ValScalar::Bool),
        );
        assert_eq!(
            hex(&entry),
            "00 00 00 00 00 00 00 00 00 00 00 06 6e 65 67 61 74 65 00 00 00 00 00 01 \
             00 00 00 00 00 00 00 01 62 00 00 00 00 00 00 01 \
             00 00 00 01 00 00 00 01"
        );
    }

    #[test]
    fn an_i32_is_type_code_five_in_both_positions() {
        let entry = function_entry(
            "sub",
            &[("x", ValScalar::I32), ("y", ValScalar::I32)],
            ValReturn::Scalar(ValScalar::I32),
        );
        assert_eq!(
            hex(&entry),
            "00 00 00 00 00 00 00 00 00 00 00 03 73 75 62 00 00 00 00 02 \
             00 00 00 00 00 00 00 01 78 00 00 00 00 00 00 05 \
             00 00 00 00 00 00 00 01 79 00 00 00 00 00 00 05 \
             00 00 00 01 00 00 00 05"
        );
    }

    /// The widest method there can be: `inputs<>` is unbounded, so all
    /// thirty-two parameters are described, each in its declared position.
    #[test]
    fn a_thirty_two_parameter_method_describes_every_input() {
        let names: Vec<String> = (0..32).map(|index| format!("p{index}")).collect();
        let inputs: Vec<(&str, ValScalar)> = names
            .iter()
            .map(|name| (name.as_str(), ValScalar::U32))
            .collect();
        let entry = function_entry("widest", &inputs, ValReturn::Scalar(ValScalar::U32));

        let header = 4 + 4 + (4 + 8);
        assert_eq!(entry[header..header + 4], [0, 0, 0, 32]);
        // Spelled out without the encoder: an empty doc string, the name's
        // length, the name — two or three bytes here, so zero-padded to one
        // word — and the `u32` type code, 4.
        let input = |index: usize| {
            let name = format!("p{index}");
            assert!(name.len() < 4, "every name here fits one word");
            let length = u8::try_from(name.len()).expect("a short name");
            let mut word = name.into_bytes();
            word.resize(4, 0);
            [vec![0, 0, 0, 0], vec![0, 0, 0, length], word, vec![0, 0, 0, 4]].concat()
        };
        let described: Vec<u8> = (0..32).flat_map(input).collect();
        assert_eq!(entry[header + 4..entry.len() - 8], described[..]);
        assert_eq!(entry[entry.len() - 8..], [0, 0, 0, 1, 0, 0, 0, 4]);
    }

    /// A parameter name is written exactly as the author spelled it, a leading
    /// underscore included: the spec must not carry a name the author did not
    /// write.
    #[test]
    fn a_parameter_name_is_written_as_spelled() {
        let entry = function_entry("f", &[("_x", ValScalar::U32)], ValReturn::Void);
        assert!(
            entry
                .windows(8)
                .any(|window| window == [0, 0, 0, 2, b'_', b'x', 0, 0]),
            "{}",
            hex(&entry)
        );
    }

    /// The meta entry for a given version, pinned whole for `0.0.1`. The
    /// version is an argument here, so this pins the layout and not the
    /// toolchain's version.
    #[test]
    fn the_meta_entry_is_kind_key_and_value() {
        assert_eq!(
            hex(&contract_meta_payload("0.0.1")),
            "00 00 00 00 00 00 00 06 69 6e 66 76 65 72 00 00 00 00 00 05 30 2e 30 2e 31 00 00 00"
        );
    }

    /// The value written is the toolchain's own version, whatever its length,
    /// derived here without the encoder's padding rule.
    #[test]
    fn the_meta_value_is_the_toolchain_version() {
        let version = CONTRACT_META_TOOLCHAIN_VERSION.as_bytes();
        assert!(!version.is_empty());

        let payload = contract_meta_payload(CONTRACT_META_TOOLCHAIN_VERSION);
        let value = &payload[16..];
        let length = u32::try_from(version.len()).expect("a short version");
        assert_eq!(value[..4], length.to_be_bytes());
        assert_eq!(&value[4..4 + version.len()], version);
        let padding = &value[4 + version.len()..];
        assert!(padding.iter().all(|byte| *byte == 0));
        assert_eq!((version.len() + padding.len()) % 4, 0);
        assert!(padding.len() < 4);
    }

    /// The spec code a source type earns: one for a type the Val ABI carries,
    /// none for a type it refuses. A match with no wildcard arm, so a type
    /// added to the descriptor stops this test compiling until it is placed.
    fn expected_code(ty: &AbiType) -> Option<u32> {
        match ty {
            AbiType::Bool => Some(SC_SPEC_TYPE_BOOL),
            AbiType::U32 => Some(SC_SPEC_TYPE_U32),
            AbiType::I32 => Some(SC_SPEC_TYPE_I32),
            AbiType::I8
            | AbiType::U8
            | AbiType::I16
            | AbiType::U16
            | AbiType::I64
            | AbiType::U64
            | AbiType::Enum { .. }
            | AbiType::Struct { .. }
            | AbiType::Array { .. } => None,
        }
    }

    /// One value of every source type the descriptor can describe, each
    /// classified by the Val ABI and coded by the spec exactly as
    /// [`expected_code`] says.
    #[test]
    fn a_type_has_a_spec_code_exactly_when_the_val_abi_carries_it() {
        let every_type = [
            AbiType::Bool,
            AbiType::U32,
            AbiType::I32,
            AbiType::I8,
            AbiType::U8,
            AbiType::I16,
            AbiType::U16,
            AbiType::I64,
            AbiType::U64,
            AbiType::Enum {
                name: "Colour".to_string(),
            },
            AbiType::Struct {
                name: "Point".to_string(),
            },
            AbiType::Array {
                elem: Box::new(AbiType::U32),
                len: 4,
            },
        ];
        for ty in every_type {
            assert_eq!(ValScalar::from_abi(&ty).map(spec_type), expected_code(&ty), "{ty:?}");
        }
        assert_eq!(
            [SC_SPEC_TYPE_BOOL, SC_SPEC_TYPE_VOID, SC_SPEC_TYPE_U32, SC_SPEC_TYPE_I32],
            [1, 2, 4, 5],
            "the published codes"
        );
    }
}
