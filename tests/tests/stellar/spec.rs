//! The contract spec and meta sections, read back the way the tooling reads
//! them.
//!
//! `contracts` shows that a compiled contract runs; this module shows that the
//! tooling around it can describe it. `stellar contract invoke` turns
//! `-- add --a 2 --b 40` into typed arguments by decoding the contract's
//! `contractspecv0` section. The `stellar` CLI 28.0.0 reads a spec through two
//! readers, and every fixture is decoded with both here:
//!
//! - `soroban-spec-tools`' `Spec::new`, which `invoke` runs over the
//!   *deployed* contract's code to build the arguments it sends, and
//!   `stellar contract info interface` runs over a file. It decodes the
//!   environment metadata, the contract metadata and the spec, each as a run of
//!   whole entries, and fails the command if any one of the three does not
//!   decode. It is mirrored here with the same `stellar-xdr` readers and depth
//!   limit rather than taken as a second dependency.
//! - `soroban_spec::read::from_wasm`, which `stellar contract bindings rust
//!   --wasm <file>` runs over a local file, as the Soroban SDK's
//!   `contractimport!` does; it is this binary's dependency. `invoke` has no
//!   `--wasm` option in 28.0.0 and never runs it over a user's file. It is
//!   asserted because it is the reader client code is generated from
//!   (`stellar contract bindings rust`, the SDK's `contractimport!`).
//!
//! Two derivations of the same bytes are held against each other. The
//! compiler's section is hand-encoded in `core/stellar-abi`. The expected
//! section is built here, from the export descriptor the rewrite consumed —
//! kept from the one code generation run the contract was written from — out
//! of the `stellar-xdr` types the host is built on, and written by that crate's
//! own encoder. The two must be byte-identical, which makes the hand
//! encoder's correctness a measurement rather than a reading of the XDR
//! definitions. The descriptor's source types are mapped to spec types by this
//! module's own match, not by the crate's table, so a wrong code in the crate
//! cannot be copied into the expectation.
//!
//! The two name bounds both gates refuse against, 32 bytes of method name and
//! 30 of parameter name, are pinned here too. Every other statement of them is
//! a copy of the XDR written out by hand; this tier holds the `stellar-xdr`
//! field types themselves, so it is the one place a copy can be compared with
//! the original.

use std::io::Cursor;

use inference_stellar_abi::{
    CONTRACT_META_SECTION_NAME, CONTRACT_META_TOOLCHAIN_VERSION, ENV_META_SECTION_NAME,
    MAX_EXPORT_NAME_BYTES, MAX_INPUT_NAME_BYTES, SPEC_SECTION_NAME, StellarAbiError,
};
use inference_tests::corpus::{
    codegen_for_target, compile_for_stellar, rewrite_default_build_for_stellar,
    try_compile_for_stellar_with_descriptor, wasm_for_target,
};
use inference_wasm_codegen::{AbiReturn, AbiType, ExportSignature, Target};
use soroban_env_host::xdr::{
    Error as XdrError, Limited, Limits, ReadXdr, ScEnvMetaEntry, ScEnvMetaEntryInterfaceVersion,
    ScMetaEntry, ScMetaV0, ScSpecEntry, ScSpecFunctionInputV0, ScSpecFunctionV0, ScSpecTypeDef,
    ScSymbol, StringM,
};
use soroban_spec::read::{FromWasmError, from_wasm, parse_raw, raw_from_wasm};

use crate::contracts::{fixture_names, fixture_source};
use crate::support::{
    CONTRACT_META_SECTION, ENV_META_SECTION, MEASURED_ENV_META_TAIL, SPEC_SECTION, TOOLCHAIN_KEY,
    contract_meta_entry, custom_sections, function_spec_entry, gathered, hex, xdr,
};

/// The recursion limit `Spec::new` decodes all three sections under:
/// `SPEC_XDR_DEPTH_LIMIT` in stellar-cli 28.0.0,
/// `cmd/crates/soroban-spec-tools/src/contract.rs`.
const CLI_XDR_DEPTH_LIMIT: u32 = 500;

/// `add(a: u32, b: u32) -> u32` as one `SCSpecEntry`: the sixty bytes
/// `MEASURED_ABI.md` annotates field by field.
const ADD_ENTRY: [u8; 60] = [
    0x00, 0x00, 0x00, 0x00, // SC_SPEC_ENTRY_FUNCTION_V0
    0x00, 0x00, 0x00, 0x00, // doc: ""
    0x00, 0x00, 0x00, 0x03, 0x61, 0x64, 0x64, 0x00, // name: "add", one byte of padding
    0x00, 0x00, 0x00, 0x02, // two inputs
    0x00, 0x00, 0x00, 0x00, // doc: ""
    0x00, 0x00, 0x00, 0x01, 0x61, 0x00, 0x00, 0x00, // name: "a", three bytes of padding
    0x00, 0x00, 0x00, 0x04, // SC_SPEC_TYPE_U32
    0x00, 0x00, 0x00, 0x00, // doc: ""
    0x00, 0x00, 0x00, 0x01, 0x62, 0x00, 0x00, 0x00, // name: "b"
    0x00, 0x00, 0x00, 0x04, // SC_SPEC_TYPE_U32
    0x00, 0x00, 0x00, 0x01, // one output
    0x00, 0x00, 0x00, 0x04, // SC_SPEC_TYPE_U32
];

/// `tick()` as one `SCSpecEntry`: twenty-four bytes, ending in an empty inputs
/// vector and an empty outputs vector.
const TICK_ENTRY: [u8; 24] = [
    0x00, 0x00, 0x00, 0x00, // SC_SPEC_ENTRY_FUNCTION_V0
    0x00, 0x00, 0x00, 0x00, // doc: ""
    0x00, 0x00, 0x00, 0x04, 0x74, 0x69, 0x63, 0x6b, // name: "tick", no padding
    0x00, 0x00, 0x00, 0x00, // no inputs
    0x00, 0x00, 0x00, 0x00, // no outputs
];

/// One fixture compiled for the Stellar target: the contract, and the export
/// descriptor the rewrite consumed to write it, kept from the same code
/// generation run rather than recomputed beside it.
struct Compiled {
    fixture: &'static str,
    contract: Vec<u8>,
    exports: Vec<ExportSignature>,
}

/// Every fixture of the case table in `contracts`, compiled: the thirteen
/// `MEASURED_ABI.md` counts, so every sweep over them proves it swept.
fn compiled_fixtures() -> Vec<Compiled> {
    let compiled: Vec<Compiled> = fixture_names().into_iter().map(compiled).collect();
    assert_eq!(compiled.len(), 13, "MEASURED_ABI.md says thirteen fixtures");
    compiled
}

/// One fixture, compiled by the route `infc --target stellar` takes, in one
/// code generation run.
fn compiled(fixture: &'static str) -> Compiled {
    let (contract, output) = try_compile_for_stellar_with_descriptor(&fixture_source(fixture))
        .unwrap_or_else(|e| panic!("{fixture} must compile for the Stellar target: {e}"));
    Compiled { fixture, contract, exports: output.export_signatures().to_vec() }
}

/// The spec type a source type is described by, decided by this tier rather
/// than read from the crate under test.
///
/// No wildcard arm: a type added to the descriptor stops this module compiling
/// until it is placed.
///
/// # Panics
///
/// Panics, naming `fixture`, on a type outside the scalar set a contract method
/// may carry. Every fixture here compiled, so meeting one means the Stellar
/// gate let it through.
fn spec_type(fixture: &str, ty: &AbiType) -> ScSpecTypeDef {
    match ty {
        AbiType::Bool => ScSpecTypeDef::Bool,
        AbiType::U32 => ScSpecTypeDef::U32,
        AbiType::I32 => ScSpecTypeDef::I32,
        AbiType::I8
        | AbiType::U8
        | AbiType::I16
        | AbiType::U16
        | AbiType::I64
        | AbiType::U64
        | AbiType::Enum { .. }
        | AbiType::Struct { .. }
        | AbiType::Array { .. } => {
            panic!("{fixture}: `{ty:?}` is outside the scalar set a contract method may carry")
        }
    }
}

/// The outputs a return is described by: none for a unit return, the one
/// scalar otherwise.
///
/// # Panics
///
/// Panics, naming `fixture`, on a compound return, which the Stellar gate
/// refuses.
fn spec_outputs(fixture: &str, ret: &AbiReturn) -> Vec<ScSpecTypeDef> {
    match ret {
        AbiReturn::Unit => Vec::new(),
        AbiReturn::Scalar(ty) => vec![spec_type(fixture, ty)],
        AbiReturn::Sret(ty) => panic!("{fixture}: a compound return `{ty:?}` reached a contract"),
    }
}

/// The spec entry the descriptor says one method has, built with
/// `stellar-xdr`'s own types.
///
/// # Panics
///
/// Panics, naming `fixture`, on an unnamed parameter, which the Stellar gate
/// refuses.
fn expected_entry(fixture: &str, signature: &ExportSignature) -> ScSpecEntry {
    let method = &signature.name;
    let inputs: Vec<(&str, ScSpecTypeDef)> = signature
        .params
        .iter()
        .enumerate()
        .map(|(index, param)| {
            let name = param.name.as_deref().unwrap_or_else(|| {
                panic!("{fixture}::{method}: parameter {} is unnamed", index + 1)
            });
            (name, spec_type(fixture, &param.ty))
        })
        .collect();
    let output = spec_outputs(fixture, &signature.ret).into_iter().next();
    function_spec_entry(method, &inputs, output)
}

/// The ways one decoded method description disagrees with the descriptor
/// entry it was written from, each naming the fixture, the method and the
/// field; empty when they agree.
fn disagreements(
    fixture: &str,
    function: &ScSpecFunctionV0,
    signature: &ExportSignature,
) -> Vec<String> {
    let method = &signature.name;
    let mut found = Vec::new();
    if !function.doc.is_empty() {
        found.push(format!("{fixture}::{method}: the method's doc is not empty"));
    }
    if function.inputs.len() != signature.params.len() {
        found.push(format!(
            "{fixture}::{method}: {} inputs described, {} parameters declared",
            function.inputs.len(),
            signature.params.len()
        ));
    }
    for (index, (input, param)) in function.inputs.iter().zip(&signature.params).enumerate() {
        let position = index + 1;
        let described = input.name.to_utf8_string_lossy();
        if param.name.as_deref() != Some(described.as_str()) {
            found.push(format!(
                "{fixture}::{method}: input {position} is named `{described}`, the parameter {:?}",
                param.name
            ));
        }
        if !input.doc.is_empty() {
            found.push(format!("{fixture}::{method}: input {position}'s doc is not empty"));
        }
        let expected = spec_type(fixture, &param.ty);
        if input.type_ != expected {
            found.push(format!(
                "{fixture}::{method}: input {position} `{described}` is described as {:?}, \
                 declared {:?}, which is {expected:?}",
                input.type_, param.ty
            ));
        }
    }
    let outputs = function.outputs.to_vec();
    let expected = spec_outputs(fixture, &signature.ret);
    if outputs != expected {
        found.push(format!(
            "{fixture}::{method}: outputs {outputs:?}, but the return {:?} is {expected:?}",
            signature.ret
        ));
    }
    found
}

/// The disagreements between one compiled contract's spec, decoded by the
/// CLI's reader, and the descriptor it was built from.
fn spec_disagreements(compiled: &Compiled) -> Vec<String> {
    let Compiled { fixture, contract, exports } = compiled;
    let entries = match from_wasm(contract) {
        Ok(entries) => entries,
        Err(e) => return vec![format!("{fixture}: the CLI's reader refused the spec: {e} ({e:?})")],
    };
    let mut functions = Vec::new();
    for entry in &entries {
        let ScSpecEntry::FunctionV0(function) = entry else {
            return vec![format!(
                "{fixture}: an entry describes something other than a method: {entry:?}"
            )];
        };
        functions.push(function);
    }
    let described: Vec<String> =
        functions.iter().map(|function| function.name.0.to_utf8_string_lossy()).collect();
    let exported: Vec<&str> = exports.iter().map(|signature| signature.name.as_str()).collect();
    if described != exported {
        return vec![format!(
            "{fixture}: the spec describes {described:?} and the descriptor lists {exported:?}"
        )];
    }
    functions
        .into_iter()
        .zip(exports)
        .flat_map(|(function, signature)| disagreements(fixture, function, signature))
        .collect()
}

/// Decodes `body` as a run of whole `T` entries, the way `Spec::new` decodes
/// each of the three sections: `read_xdr_iter` under the CLI's depth limit.
///
/// # Errors
///
/// Returns the reader's own error on a body that is not a run of whole
/// entries.
fn decode_run<T: ReadXdr>(body: &[u8]) -> Result<Vec<T>, XdrError> {
    let limits = Limits::depth(CLI_XDR_DEPTH_LIMIT);
    T::read_xdr_iter(&mut Limited::new(Cursor::new(body), limits)).collect()
}

/// A contract with an ordinary `add` and one more method, `method`, taking a
/// single `u32` named `input` and returning it.
///
/// `add` comes first, so a reader refusing the second method's entry is seen
/// to lose the first one's with it.
fn add_and_one_named(method: &str, input: &str) -> String {
    format!(
        "pub fn add(a: u32, b: u32) -> u32 {{ return a + b; }}\n\
         pub fn {method}({input}: u32) -> u32 {{ return {input}; }}"
    )
}

/// The spec [`add_and_one_named`] describes, built from the names as written
/// rather than read off a descriptor.
fn add_and_one_named_spec(method: &str, input: &str) -> Vec<ScSpecEntry> {
    use ScSpecTypeDef::U32;
    vec![
        function_spec_entry("add", &[("a", U32), ("b", U32)], Some(U32)),
        function_spec_entry(method, &[(input, U32)], Some(U32)),
    ]
}

/// A method name and a parameter name each exactly as long as the gates
/// admit.
fn names_at_the_bounds() -> (String, String) {
    ("m".repeat(MAX_EXPORT_NAME_BYTES), "n".repeat(MAX_INPUT_NAME_BYTES))
}

/// What the rewriter says about `source` when the source gate never sees it:
/// the default build, linked, handed to the rewriter with its descriptor by
/// [`rewrite_default_build_for_stellar`] — the route `stellar_abi_parity` puts
/// a refused program in front of the rewriter by.
///
/// # Panics
///
/// Panics if the default build or the link fails.
fn rewriter_refusal(source: &str) -> Option<StellarAbiError> {
    rewrite_default_build_for_stellar(source)
        .unwrap_or_else(|e| panic!("the default build and its link must succeed: {e}"))
        .err()
}

/// `section` with the one XDR string spelling `old` replaced by one spelling
/// `new`, length word and padding included.
///
/// Both strings are written by `stellar-xdr` as strings with no bound, so
/// `new` may be wider than the field it lands in: no gate lets a contract
/// carry such a name, and this is how the reader is shown one anyway.
///
/// # Panics
///
/// Panics unless `section` spells `old` exactly once.
fn respelled(section: &[u8], old: &str, new: &str) -> Vec<u8> {
    let unbounded = |name: &str| -> Vec<u8> {
        let string: StringM = name.try_into().expect("an unbounded XDR string holds any name");
        xdr(&string)
    };
    let (old, new) = (unbounded(old), unbounded(new));
    let starts: Vec<usize> = section
        .windows(old.len())
        .enumerate()
        .filter_map(|(start, window)| (window == old.as_slice()).then_some(start))
        .collect();
    let [start] = starts.as_slice() else {
        panic!("the section spells `{}` {} times, not once", hex(&old), starts.len())
    };
    [&section[..*start], &new, &section[start + old.len()..]].concat()
}

/// Every compiled contract's spec decodes with `soroban_spec::read::from_wasm`,
/// the reader client code is generated from, and what it decodes to is the
/// descriptor, method by method — the names in export order, every input's
/// name and type, the outputs, the empty doc strings.
///
/// Every fixture is checked before the test fails, so a failure lists every
/// fixture that disagrees rather than the first.
#[test]
fn every_fixture_spec_decodes_with_the_cli_reader_into_its_descriptor() {
    let failures: Vec<String> = compiled_fixtures().iter().flat_map(spec_disagreements).collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The control for the test above: neither the default build of a fixture nor
/// its code generation output for the Stellar target, before the link and the
/// rewrite, carries a spec or a metadata section, so a decoded spec is one the
/// Stellar rewrite wrote, never one code generation left behind.
#[test]
fn the_default_build_of_every_fixture_carries_no_spec_and_no_meta_section() {
    for fixture in fixture_names() {
        let source = fixture_source(fixture);
        let stellar_codegen = codegen_for_target(&source, Target::Stellar)
            .unwrap_or_else(|e| panic!("{fixture} must compile for the Stellar target: {e}"));
        let builds = [
            ("the default build", wasm_for_target(&source, Target::Wasm32)),
            ("the Stellar code generation output", stellar_codegen.wasm().to_vec()),
        ];
        for (build, wasm) in builds {
            let read = from_wasm(&wasm);
            assert!(
                matches!(read, Err(FromWasmError::NotFound)),
                "{fixture}: {build}'s spec must be absent, got {read:?}"
            );
            let names: Vec<String> =
                custom_sections(&wasm).into_iter().map(|(name, _)| name).collect();
            assert!(
                !names.iter().any(|name| {
                    [SPEC_SECTION, CONTRACT_META_SECTION, ENV_META_SECTION].contains(&name.as_str())
                }),
                "{fixture}: {build} carries {names:?}"
            );
        }
    }
}

/// The oracle. The expected section is built from the descriptor with
/// `stellar-xdr`'s own types, written by its own encoder, entry after entry in
/// export order, and it must equal the compiled contract's section byte for
/// byte. The hand encoder and the reference encoder are independent
/// derivations, so agreement here is a measurement of the former.
#[test]
fn every_fixture_spec_section_is_stellar_xdrs_own_encoding_of_its_descriptor() {
    let mut failures = Vec::new();
    for Compiled { fixture, contract, exports } in compiled_fixtures() {
        let section = raw_from_wasm(&contract)
            .unwrap_or_else(|e| panic!("{fixture}: the contract carries no spec section: {e}"));
        let expected: Vec<u8> =
            exports.iter().flat_map(|signature| xdr(&expected_entry(fixture, signature))).collect();
        if section != expected {
            failures.push(format!(
                "{fixture}:\n  compiled:    {}\n  stellar-xdr: {}",
                hex(&section),
                hex(&expected)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The two runs `MEASURED_ABI.md` annotates byte by byte, measured rather than
/// copied: each is what `stellar-xdr` writes for the method, what the compiled
/// contract carries at that method's place in its section, and what the CLI's
/// reader decodes back into that one method. The place is the one the record
/// states too: `add` at offset 48, after `identity`'s forty-eight bytes, and
/// `tick` at the start of its section.
#[test]
fn the_add_and_tick_entries_are_the_runs_the_measured_record_annotates() {
    for (fixture, method, recorded_offset, pinned) in [
        ("u32_methods", "add", 48, &ADD_ENTRY[..]),
        ("zero_parameter", "tick", 0, &TICK_ENTRY[..]),
    ] {
        let Compiled { contract, exports, .. } = compiled(fixture);
        let section = raw_from_wasm(&contract).expect("the contract carries a spec section");

        let mut offset = 0;
        let mut located = None;
        for signature in &exports {
            let entry = expected_entry(fixture, signature);
            let encoded = xdr(&entry);
            let length = encoded.len();
            if signature.name == method {
                located = Some((offset, encoded, entry));
            }
            offset += length;
        }
        let (offset, oracle, entry) =
            located.unwrap_or_else(|| panic!("{fixture} exports no `{method}`"));
        assert_eq!(
            offset, recorded_offset,
            "{fixture}::{method}: the entry starts at offset {offset} of its section; \
             MEASURED_ABI.md records {recorded_offset}"
        );

        assert_eq!(
            oracle,
            pinned,
            "{fixture}::{method}: stellar-xdr writes `{}`; MEASURED_ABI.md records `{}`",
            hex(&oracle),
            hex(pinned)
        );
        let carried = section.get(offset..offset + pinned.len());
        assert_eq!(
            carried,
            Some(pinned),
            "{fixture}::{method}: the compiled contract carries `{}` where MEASURED_ABI.md \
             records `{}`",
            hex(carried.unwrap_or_default()),
            hex(pinned)
        );
        assert_eq!(
            parse_raw(pinned).expect("the CLI's reader decodes the pinned run"),
            vec![entry],
            "{fixture}::{method}: the pinned run decodes to something else"
        );
    }
}

/// The table's widest and narrowest methods, named rather than swept: the
/// widest has all thirty-two inputs, `p0` to `p31` in order, and the two
/// methods that return nothing have no outputs at all rather than one `Void`.
#[test]
fn the_widest_method_lists_every_input_and_a_unit_method_lists_no_output() {
    let only_method = |fixture: &'static str| -> ScSpecFunctionV0 {
        let entries = from_wasm(&compiled(fixture).contract)
            .unwrap_or_else(|e| panic!("{fixture}: the CLI's reader refused the spec: {e}"));
        match <[ScSpecEntry; 1]>::try_from(entries) {
            Ok([ScSpecEntry::FunctionV0(function)]) => function,
            other => panic!("{fixture}: expected one method entry, got {other:?}"),
        }
    };

    let widest = only_method("max_arity");
    assert_eq!(widest.name.0.to_utf8_string_lossy(), "widest");
    let names: Vec<String> =
        widest.inputs.iter().map(|input| input.name.to_utf8_string_lossy()).collect();
    let declared: Vec<String> = (0..32).map(|index| format!("p{index}")).collect();
    assert_eq!(names, declared, "max_arity's thirty-two inputs, in declaration order");
    assert!(
        widest.inputs.iter().all(|input| input.type_ == ScSpecTypeDef::U32),
        "every one of max_arity's inputs is a u32: {:?}",
        widest.inputs
    );
    assert_eq!(widest.outputs.to_vec(), vec![ScSpecTypeDef::U32]);

    for (fixture, method, inputs) in [("zero_parameter", "tick", 0), ("params_only", "record", 3)] {
        let function = only_method(fixture);
        assert_eq!(function.name.0.to_utf8_string_lossy(), method);
        assert_eq!(function.inputs.len(), inputs, "{fixture}::{method}");
        assert!(
            function.outputs.is_empty(),
            "{fixture}::{method} returns nothing and must list no output, not {:?}",
            function.outputs
        );
    }
}

/// `contractmetav0` holds exactly one entry, the toolchain's version under
/// `infver`, decoded with `stellar-xdr`'s reader and equal byte for byte to
/// what its writer produces for that entry.
///
/// The value is the crate's published constant, never a spelled-out version,
/// and that constant is the workspace version: this crate inherits the same
/// one.
#[test]
fn every_fixture_meta_section_is_one_toolchain_version_entry() {
    assert_eq!(
        CONTRACT_META_TOOLCHAIN_VERSION,
        env!("CARGO_PKG_VERSION"),
        "the toolchain version is the workspace version, which this crate inherits too"
    );
    let expected = xdr(&contract_meta_entry(TOOLCHAIN_KEY, CONTRACT_META_TOOLCHAIN_VERSION));

    for Compiled { fixture, contract, .. } in compiled_fixtures() {
        let bodies: Vec<Vec<u8>> = custom_sections(&contract)
            .into_iter()
            .filter(|(name, _)| name == CONTRACT_META_SECTION)
            .map(|(_, body)| body)
            .collect();
        let [body] = bodies.as_slice() else {
            panic!("{fixture}: expected one {CONTRACT_META_SECTION} section, not {}", bodies.len())
        };
        let entries = decode_run::<ScMetaEntry>(body)
            .unwrap_or_else(|e| panic!("{fixture}: the meta section does not decode: {e}"));
        let [ScMetaEntry::ScMetaV0(ScMetaV0 { key, val })] = entries.as_slice() else {
            panic!("{fixture}: expected exactly one meta entry, got {entries:?}")
        };
        assert_eq!(key.to_utf8_string_lossy(), TOOLCHAIN_KEY, "{fixture}");
        assert_eq!(val.to_utf8_string_lossy(), CONTRACT_META_TOOLCHAIN_VERSION, "{fixture}");
        assert_eq!(
            hex(body),
            hex(&expected),
            "{fixture}: the meta section is not stellar-xdr's own encoding of its entry"
        );
    }
}

/// The three sections the rewrite appends are the last three custom sections
/// of every contract, each once, in the order spec, meta, environment metadata:
/// the environment metadata stays last, and every contract ends with exactly
/// the thirty-two bytes `envelope` pins, [`MEASURED_ENV_META_TAIL`].
#[test]
fn every_contract_ends_with_the_spec_then_the_meta_then_the_environment_metadata() {
    let tail = [SPEC_SECTION, CONTRACT_META_SECTION, ENV_META_SECTION];
    assert_eq!(
        [SPEC_SECTION_NAME, CONTRACT_META_SECTION_NAME, ENV_META_SECTION_NAME],
        tail,
        "the crate's section names are the ones the tooling and the host spell"
    );

    for Compiled { fixture, contract, .. } in compiled_fixtures() {
        let names: Vec<String> =
            custom_sections(&contract).into_iter().map(|(name, _)| name).collect();
        let last_three = names.len().checked_sub(tail.len()).map(|start| &names[start..]);
        assert_eq!(last_three, Some(&tail.map(str::to_owned)[..]), "{fixture} carries {names:?}");
        for name in tail {
            assert_eq!(
                names.iter().filter(|carried| *carried == name).count(),
                1,
                "{fixture} carries {name} more than once: {names:?}"
            );
        }
        let last_32 = contract.len().checked_sub(32).map(|start| &contract[start..]);
        assert_eq!(
            last_32,
            Some(&MEASURED_ENV_META_TAIL[..]),
            "{fixture}: the contract's last thirty-two bytes are not the environment metadata \
             section MEASURED_ABI.md records"
        );
    }
}

/// The reader that shapes a real invocation: `invoke` against a deployed
/// contract, and `stellar contract info interface`, read the contract with
/// `Spec::new`, which decodes all three sections and fails the command if any
/// one of them does not decode — so a meta section it could not read would
/// leave the contract with no interface at all, however good its spec.
///
/// Mirrored with the same readers and depth limit: the environment metadata is
/// one interface-version entry declaring protocol 20 with a zero pre-release,
/// the contract metadata is one entry, and the spec is the descriptor's methods
/// in export order.
///
/// The protocol is spelled here as the literals `envelope` pins — its
/// `DECLARED_PROTOCOL` and the thirty-two bytes of
/// [`MEASURED_ENV_META_TAIL`] — rather than read from the crate under test, so
/// a changed crate constant fails this tier instead of moving it.
#[test]
fn every_contract_decodes_the_way_the_cli_reads_a_deployed_contract() {
    for Compiled { fixture, contract, exports } in compiled_fixtures() {
        let env_meta = decode_run::<ScEnvMetaEntry>(&gathered(&contract, ENV_META_SECTION))
            .unwrap_or_else(|e| panic!("{fixture}: the environment metadata does not decode: {e}"));
        assert_eq!(
            env_meta,
            [ScEnvMetaEntry::ScEnvMetaKindInterfaceVersion(ScEnvMetaEntryInterfaceVersion {
                protocol: 20,
                pre_release: 0,
            })],
            "{fixture}"
        );

        let meta = decode_run::<ScMetaEntry>(&gathered(&contract, CONTRACT_META_SECTION))
            .unwrap_or_else(|e| panic!("{fixture}: the contract metadata does not decode: {e}"));
        assert_eq!(meta.len(), 1, "{fixture}: {meta:?}");

        let spec = decode_run::<ScSpecEntry>(&gathered(&contract, SPEC_SECTION))
            .unwrap_or_else(|e| panic!("{fixture}: the spec does not decode: {e}"));
        let expected: Vec<ScSpecEntry> =
            exports.iter().map(|signature| expected_entry(fixture, signature)).collect();
        assert_eq!(spec, expected, "{fixture}");
    }
}

/// The widths of the two spec fields a name is recorded in, read off the
/// fields' own types: the input-name field holds [`MAX_INPUT_NAME_BYTES`]
/// bytes and not one more, and the method-name symbol holds
/// [`MAX_EXPORT_NAME_BYTES`] and not one more.
///
/// No width is typed here. Each probe builds the field's value with
/// `try_into`, and the field's type decides whether it fits, so the constants
/// the gates refuse against are compared with the XDR itself rather than with
/// another hand-typed copy of it.
#[test]
fn the_name_bounds_the_gates_refuse_against_are_the_widths_of_the_spec_fields() {
    let input_named = |len: usize| -> Result<ScSpecFunctionInputV0, XdrError> {
        Ok(ScSpecFunctionInputV0 {
            doc: StringM::default(),
            name: "n".repeat(len).try_into()?,
            type_: ScSpecTypeDef::U32,
        })
    };
    let method_named =
        |len: usize| -> Result<ScSymbol, XdrError> { Ok(ScSymbol("m".repeat(len).try_into()?)) };

    assert!(
        input_named(MAX_INPUT_NAME_BYTES).is_ok(),
        "a parameter name of MAX_INPUT_NAME_BYTES ({MAX_INPUT_NAME_BYTES}) bytes fits the \
         spec's input-name field"
    );
    assert_eq!(
        input_named(MAX_INPUT_NAME_BYTES + 1).err(),
        Some(XdrError::LengthExceedsMax),
        "a parameter name one byte over MAX_INPUT_NAME_BYTES ({MAX_INPUT_NAME_BYTES}) does not \
         fit the spec's input-name field"
    );
    assert!(
        method_named(MAX_EXPORT_NAME_BYTES).is_ok(),
        "a method name of MAX_EXPORT_NAME_BYTES ({MAX_EXPORT_NAME_BYTES}) bytes fits the spec's \
         method-name symbol"
    );
    assert_eq!(
        method_named(MAX_EXPORT_NAME_BYTES + 1).err(),
        Some(XdrError::LengthExceedsMax),
        "a method name one byte over MAX_EXPORT_NAME_BYTES ({MAX_EXPORT_NAME_BYTES}) does not \
         fit the spec's method-name symbol"
    );
}

/// A method whose name and whose parameter's name are each exactly as long as
/// the gates admit compiles, the CLI's reader decodes its spec into exactly
/// the methods the source declares, and the section is byte for byte what
/// `stellar-xdr` writes for them.
///
/// The at-bound half of the pin above, taken through the whole compiler. No
/// fixture comes near either bound — the longest names there are
/// `passthrough` and `index` — so without this contract no spec carrying a
/// name at either width is ever decoded.
#[test]
fn a_contract_whose_names_are_at_both_bounds_decodes_into_the_methods_it_declares() {
    let (method, input) = names_at_the_bounds();
    let contract = compile_for_stellar(&add_and_one_named(&method, &input));

    let entries = from_wasm(&contract).unwrap_or_else(|e| {
        panic!(
            "the CLI's reader refused the spec of a contract whose method name is \
             {MAX_EXPORT_NAME_BYTES} bytes and whose parameter name is {MAX_INPUT_NAME_BYTES}, \
             both of which the gates admitted: {e} ({e:?})"
        )
    });
    let expected = add_and_one_named_spec(&method, &input);
    assert_eq!(entries, expected, "the methods the source declares");
    let section = raw_from_wasm(&contract).expect("the contract carries a spec section");
    let oracle: Vec<u8> = expected.iter().flat_map(xdr).collect();
    assert_eq!(hex(&section), hex(&oracle), "the section is not stellar-xdr's own encoding");
}

/// A parameter written with a leading underscore keeps it: `MEASURED_ABI.md`'s
/// `keep` fixture compiles, the CLI's reader decodes its one method with inputs
/// named `_x` and `y`, and the section is byte for byte what `stellar-xdr`
/// writes for that method.
///
/// No fixture of the case table names a parameter with a leading underscore,
/// so without this contract no spec carrying one is ever decoded.
#[test]
fn a_leading_underscore_parameter_decodes_under_the_name_as_written() {
    let contract = compile_for_stellar("pub fn keep(_x: u32, y: u32) -> u32 { return _x + y; }");

    let entries = from_wasm(&contract)
        .unwrap_or_else(|e| panic!("the CLI's reader refused the spec: {e} ({e:?})"));
    let [ScSpecEntry::FunctionV0(function)] = entries.as_slice() else {
        panic!("expected one method entry, got {entries:?}")
    };
    let names: Vec<String> =
        function.inputs.iter().map(|input| input.name.to_utf8_string_lossy()).collect();
    assert_eq!(names, ["_x", "y"], "the inputs are named as the source spells them");

    let expected = function_spec_entry(
        "keep",
        &[("_x", ScSpecTypeDef::U32), ("y", ScSpecTypeDef::U32)],
        Some(ScSpecTypeDef::U32),
    );
    let section = raw_from_wasm(&contract).expect("the contract carries a spec section");
    assert_eq!(
        hex(&section),
        hex(&xdr(&expected)),
        "the section is not stellar-xdr's own encoding"
    );
}

/// One byte over either bound is refused by both gates before a contract is
/// produced, and a spec carrying such a name is refused by the CLI's reader —
/// the whole section, the ordinary `add` before it included.
///
/// The gates are asked apart: the source gate through code generation at the
/// Stellar target, the rewriter through [`rewriter_refusal`], which the source
/// gate never sees. Since no gate lets such a name into a contract, the reader
/// is handed the at-bound contract's section with one name respelled one byte
/// longer. Respelled one byte shorter instead, the same section decodes into
/// exactly the methods with that name, which is the control that the splice
/// itself changes nothing the reader objects to.
#[test]
fn one_byte_over_either_bound_is_refused_by_both_gates_and_by_the_reader() {
    let (method, input) = names_at_the_bounds();
    let (long_method, long_input) = (format!("{method}m"), format!("{input}n"));

    let refusals = [
        (
            add_and_one_named(&long_method, &input),
            format!(
                "the exported function '{long_method}' has a name of {} bytes",
                MAX_EXPORT_NAME_BYTES + 1
            ),
            StellarAbiError::ExportNameTooLong {
                export: long_method.clone(),
                len: MAX_EXPORT_NAME_BYTES + 1,
            },
        ),
        (
            add_and_one_named(&method, &long_input),
            format!("parameter 1 '{long_input}' has a name of {} bytes", MAX_INPUT_NAME_BYTES + 1),
            StellarAbiError::ParameterNameTooLong {
                export: method.clone(),
                position: 1,
                name: long_input.clone(),
                len: MAX_INPUT_NAME_BYTES + 1,
            },
        ),
    ];
    for (source, source_gate_says, rewriter_says) in refusals {
        let refusal = codegen_for_target(&source, Target::Stellar).err().map(|e| e.to_string());
        assert!(
            refusal.as_deref().is_some_and(|message| message.contains(&source_gate_says)),
            "the source gate must refuse `{source}` saying `{source_gate_says}`, and said \
             {refusal:?}"
        );
        assert_eq!(rewriter_refusal(&source), Some(rewriter_says), "the rewriter, on `{source}`");
    }

    let section = raw_from_wasm(&compile_for_stellar(&add_and_one_named(&method, &input)))
        .expect("the contract carries a spec section");
    let respellings = [
        (&method, &method[1..], &long_method, add_and_one_named_spec(&method[1..], &input)),
        (&input, &input[1..], &long_input, add_and_one_named_spec(&method, &input[1..])),
    ];
    for (name, shorter, longer, shortened_spec) in respellings {
        assert_eq!(
            parse_raw(&respelled(&section, name, shorter)),
            Ok(shortened_spec),
            "the control: `{name}` respelled a byte shorter"
        );
        assert_eq!(
            parse_raw(&respelled(&section, name, longer)),
            Err(XdrError::LengthExceedsMax),
            "`{name}` respelled a byte longer must cost the whole section"
        );
    }
}
