//! Pins the host's acceptance envelope for a module's shape.
//!
//! These are the constraints an emitter has to satisfy before any question of
//! marshalling arises: the mandatory metadata section, the declared protocol,
//! the wasmi feature set, and the well-formedness check the host runs over
//! whatever word an export returns.

use wasm_encoder::{Function, Instruction, ValType};

use crate::support::{
    ContractModule, Decoded, ENV_META_SECTION, FuncDef, call, decode, describe, host, session,
    symbol_for, u32_val, upload,
};

/// The protocol the Stellar target declares: the oldest protocol any Soroban
/// network has run, and therefore the one uploadable to all of them.
const DECLARED_PROTOCOL: u32 = 20;

/// The whole contract: unwrap two `U32Val` arguments, add, wrap the sum.
///
/// This is the smallest thing that is simultaneously a valid Soroban contract
/// and a real use of the ABI, and its size is the floor an emitted contract can
/// be measured against.
fn add_contract(protocol: u32) -> Vec<u8> {
    let mut body = Function::new([]);
    for index in 0..2 {
        for instruction in [
            Instruction::LocalGet(index),
            Instruction::I64Const(0xff),
            Instruction::I64And,
            Instruction::I64Const(4),
            Instruction::I64Ne,
            Instruction::If(wasm_encoder::BlockType::Empty),
            Instruction::Unreachable,
            Instruction::End,
            Instruction::LocalGet(index),
            Instruction::I64Const(32),
            Instruction::I64ShrU,
            Instruction::I32WrapI64,
        ] {
            body.instruction(&instruction);
        }
    }
    for instruction in [
        Instruction::I32Add,
        Instruction::I64ExtendI32U,
        Instruction::I64Const(32),
        Instruction::I64Shl,
        Instruction::I64Const(4),
        Instruction::I64Or,
        Instruction::End,
    ] {
        body.instruction(&instruction);
    }

    ContractModule::with_protocol(protocol, 0)
        .func(FuncDef::exported(
            "add",
            vec![ValType::I64, ValType::I64],
            vec![ValType::I64],
            body,
        ))
        .finish()
}

/// A module whose sole export returns a fixed 64-bit word.
fn returning(word: i64) -> Vec<u8> {
    let mut body = Function::new([]);
    body.instruction(&Instruction::I64Const(word));
    body.instruction(&Instruction::End);
    ContractModule::new()
        .func(FuncDef::exported("f", Vec::new(), vec![ValType::I64], body))
        .finish()
}

#[test]
fn the_smallest_real_contract_uploads_and_invokes() {
    let wasm = add_contract(DECLARED_PROTOCOL);
    assert_eq!(
        wasm.len(),
        114,
        "the reference contract's size moved; MEASURED_ABI.md records 114 bytes"
    );

    let _guard = session();
    let host = host();
    let contract = upload(&host, &wasm).expect("the reference contract uploads");
    let result = call(&host, contract, "add", &[u32_val(7), u32_val(9)])
        .expect("the reference contract invokes");
    assert_eq!(decode(result), Decoded::U32(16));
}

#[test]
fn a_module_without_the_metadata_section_is_refused() {
    let mut body = Function::new([]);
    body.instruction(&Instruction::I64Const(2));
    body.instruction(&Instruction::End);
    let wasm = ContractModule::without_meta()
        .func(FuncDef::exported("f", Vec::new(), vec![ValType::I64], body))
        .finish();

    let _guard = session();
    let host = host();
    let err = upload(&host, &wasm).expect_err("a module with no metadata section must be refused");
    let rendered = describe(&err);
    assert!(
        rendered.contains("Error(WasmVm, InvalidInput)")
            && rendered.contains("contract missing metadata section"),
        "expected the missing-metadata refusal, got: {rendered}"
    );
}

#[test]
fn a_protocol_newer_than_the_host_is_refused() {
    let wasm = add_contract(99);

    let _guard = session();
    let host = host();
    let err = upload(&host, &wasm).expect_err("a future protocol must be refused");
    let rendered = describe(&err);
    assert!(
        rendered.contains("Error(WasmVm, InvalidInput)")
            && rendered.contains("contract protocol number is newer than host"),
        "expected the future-protocol refusal, got: {rendered}"
    );
}

/// The emitter declares protocol 20 so its artifacts reach every network that
/// has ever run Soroban. If a newer host stopped accepting it, that choice would
/// have to change, so it is pinned here rather than assumed.
#[test]
fn the_declared_protocol_is_accepted_by_this_host() {
    let wasm = add_contract(DECLARED_PROTOCOL);

    let _guard = session();
    let host = host();
    upload(&host, &wasm).expect("protocol 20 must remain uploadable");
}

/// The trap the whole wrap recipe exists to avoid: a word with the right tag but
/// junk in the reserved minor bits uploads fine and dies at invocation.
#[test]
fn a_returned_word_with_nonzero_minor_bits_is_refused() {
    let word = (5i64 << 32) | (1 << 8) | 4;
    assert_eq!(decode(soroban_env_host::Val::from_payload(word.cast_unsigned())),
        Decoded::Malformed { tag: 4, body: (word >> 8).cast_unsigned() },
        "the fixture must be a word this harness also reads as malformed");
    let wasm = returning(word);

    let _guard = session();
    let host = host();
    let contract = upload(&host, &wasm).expect("a malformed return value still uploads");
    let err = call(&host, contract, "f", &[])
        .expect_err("a word with nonzero minor bits must be refused");
    let rendered = describe(&err);
    assert!(
        rendered.contains("Error(Value, UnexpectedType)"),
        "expected (Value, UnexpectedType), got: {rendered}"
    );
}

#[test]
fn a_floating_point_operation_is_refused() {
    let mut body = Function::new([]);
    body.instruction(&Instruction::F64Const(1.1f64.into()));
    body.instruction(&Instruction::F64Const(2.2f64.into()));
    body.instruction(&Instruction::F64Add);
    body.instruction(&Instruction::Drop);
    body.instruction(&Instruction::I64Const(2));
    body.instruction(&Instruction::End);
    let wasm = ContractModule::new()
        .func(FuncDef::exported("f", Vec::new(), vec![ValType::I64], body))
        .finish();

    let _guard = session();
    let host = host();
    let err = upload(&host, &wasm).expect_err("a float must be refused");
    let rendered = describe(&err);
    assert!(
        rendered.contains("Error(WasmVm, InvalidAction)")
            && rendered.contains("floating-point instruction disallowed"),
        "expected the float refusal, got: {rendered}"
    );
}

/// Multi-value is off in the host's wasmi configuration, so an export with two
/// results cannot exist — which is why the ABI gives every wrapper exactly one.
#[test]
fn a_two_result_export_is_refused() {
    let mut body = Function::new([]);
    body.instruction(&Instruction::I64Const(2));
    body.instruction(&Instruction::I64Const(2));
    body.instruction(&Instruction::End);
    let wasm = ContractModule::new()
        .func(FuncDef::exported(
            "f",
            Vec::new(),
            vec![ValType::I64, ValType::I64],
            body,
        ))
        .finish();

    let _guard = session();
    let host = host();
    let err = upload(&host, &wasm).expect_err("a two-result export must be refused");
    let rendered = describe(&err);
    assert!(
        rendered.contains("Error(WasmVm, InvalidAction)")
            && rendered.contains("multi-value feature is not enabled"),
        "expected the multi-value refusal, got: {rendered}"
    );
}

/// The emitter exports a mutable `__stack_pointer` global for every module that
/// uses linear memory. Nothing about that has to change for Stellar.
#[test]
fn an_exported_mutable_global_is_accepted() {
    let mut body = Function::new([]);
    body.instruction(&Instruction::GlobalGet(0));
    body.instruction(&Instruction::Drop);
    body.instruction(&Instruction::I64Const(2));
    body.instruction(&Instruction::End);
    let wasm = ContractModule::new()
        .exported_global_i64("__stack_pointer", 0x1_0000, true)
        .func(FuncDef::exported("f", Vec::new(), vec![ValType::I64], body))
        .finish();

    let _guard = session();
    let host = host();
    let contract = upload(&host, &wasm).expect("an exported mutable global must be accepted");
    let result = call(&host, contract, "f", &[]).expect("the contract still invokes");
    assert_eq!(decode(result), Decoded::Void);
}


// ---------------------------------------------------------------------------
// Export identity
//
// None of these rules is checked at upload: the only export check the host runs
// there is on argument count. An unusable name therefore produces a module that
// uploads cleanly and is broken at call time — or, for a name that is not a
// `Symbol` at all, one that no caller can even address.
// ---------------------------------------------------------------------------

/// A module whose sole export is `name` and returns `Void`.
fn exporting(name: &str) -> Vec<u8> {
    let mut body = Function::new([]);
    body.instruction(&Instruction::I64Const(2));
    body.instruction(&Instruction::End);
    ContractModule::new()
        .func(FuncDef::exported(name, Vec::new(), vec![ValType::I64], body))
        .finish()
}

/// The longest name a caller can express. 32 bytes is the boundary, so the
/// accepted side of it is pinned here beside the rejected one.
#[test]
fn an_export_name_of_exactly_32_bytes_is_nameable_and_invokes() {
    let name = "a".repeat(32);
    let wasm = exporting(&name);

    let _guard = session();
    let host = host();
    let contract = upload(&host, &wasm).expect("a 32-byte export name uploads");
    symbol_for(&host, &name).expect("a 32-byte name is a valid Symbol");
    let result = call(&host, contract, &name, &[]).expect("a 32-byte name invokes");
    assert_eq!(decode(result), Decoded::Void);
}

/// One byte over the `Symbol` limit. The module is accepted; the name is not
/// expressible, so the export is unreachable rather than refused.
#[test]
fn an_export_name_of_33_bytes_uploads_and_cannot_be_named() {
    let name = "a".repeat(33);
    let wasm = exporting(&name);

    let _guard = session();
    let host = host();
    let contract = upload(&host, &wasm).expect("an over-long export name still uploads");

    let err = symbol_for(&host, &name).expect_err("33 bytes must not be a Symbol");
    let rendered = describe(&err);
    assert!(
        rendered.contains("Error(Value, InvalidInput)")
            && rendered.contains("DebugInfo not available"),
        "expected the Symbol constructor to refuse 33 bytes with no event log, got: {rendered}"
    );

    let err = call(&host, contract, &name, &[]).expect_err("an unnameable export cannot be called");
    assert!(
        describe(&err).contains("Error(Value, InvalidInput)"),
        "the call must fail at the same Symbol step, got: {}",
        describe(&err)
    );
}

/// A `__`-prefixed name is a perfectly good `Symbol`; what the host refuses is
/// invoking it. That refusal is at call time, so a module carrying one uploads.
#[test]
fn a_double_underscore_export_uploads_and_is_refused_at_call() {
    let wasm = exporting("__reserved");

    let _guard = session();
    let host = host();
    let contract = upload(&host, &wasm).expect("a reserved export name still uploads");
    symbol_for(&host, "__reserved").expect("a reserved name is a valid Symbol");

    let err = call(&host, contract, "__reserved", &[])
        .expect_err("a reserved export must not be invocable");
    let rendered = describe(&err);
    assert!(
        rendered.contains("Error(Context, InvalidAction)")
            && rendered.contains("can't invoke a reserved function directly"),
        "expected the reserved-name refusal, got: {rendered}"
    );
}

/// A hyphen is outside the `Symbol` charset, so the failure is the over-long
/// one's: the module uploads and no caller can name the export.
#[test]
fn an_export_name_outside_the_symbol_charset_uploads_and_cannot_be_named() {
    let wasm = exporting("has-hyphen");

    let _guard = session();
    let host = host();
    let contract = upload(&host, &wasm).expect("a hyphenated export name still uploads");

    let err = symbol_for(&host, "has-hyphen").expect_err("a hyphen must not be a Symbol");
    let rendered = describe(&err);
    assert!(
        rendered.contains("Error(Value, InvalidInput)")
            && rendered.contains("DebugInfo not available"),
        "expected the Symbol constructor to refuse a hyphen with no event log, got: {rendered}"
    );

    let err =
        call(&host, contract, "has-hyphen", &[]).expect_err("an unnameable export cannot be called");
    assert!(
        describe(&err).contains("Error(Value, InvalidInput)"),
        "the call must fail at the same Symbol step, got: {}",
        describe(&err)
    );
}

/// A raw `(i32) -> i32` export left beside a wrapped one does not stop the
/// module uploading and does not disturb the wrapped export. It is reachable and
/// always fails, in two ways depending on how many arguments a caller passes.
#[test]
fn a_non_val_export_beside_a_wrapped_one_is_accepted() {
    let mut raw = Function::new([]);
    raw.instruction(&Instruction::LocalGet(0));
    raw.instruction(&Instruction::End);
    let mut wrapped = Function::new([]);
    wrapped.instruction(&Instruction::I64Const(2));
    wrapped.instruction(&Instruction::End);
    let wasm = ContractModule::new()
        .func(FuncDef::exported("raw", vec![ValType::I32], vec![ValType::I32], raw))
        .func(FuncDef::exported("f", Vec::new(), vec![ValType::I64], wrapped))
        .finish();

    let _guard = session();
    let host = host();
    let contract = upload(&host, &wasm).expect("a non-Val export must not block upload");

    let result = call(&host, contract, "f", &[]).expect("the wrapped export still invokes");
    assert_eq!(decode(result), Decoded::Void);

    let err = call(&host, contract, "raw", &[]).expect_err("a raw export cannot be invoked");
    let rendered = describe(&err);
    assert!(
        rendered.contains("Error(WasmVm, UnexpectedSize)")
            && rendered.contains("MismatchingParameterLen"),
        "expected an arity mismatch on the raw export, got: {rendered}"
    );

    let err =
        call(&host, contract, "raw", &[u32_val(7)]).expect_err("a raw export cannot be invoked");
    let rendered = describe(&err);
    assert!(
        rendered.contains("Error(WasmVm, UnexpectedType)")
            && rendered.contains("MismatchingParameterType"),
        "expected a value-type mismatch on the raw export, got: {rendered}"
    );
}

/// What actually forces the original export out of the way: two exports may not
/// share a name. A wrapper that takes the source function's name must remove or
/// rename that function's own export; a wrapper under a different name need not.
#[test]
fn two_exports_of_the_same_name_are_refused() {
    let mut raw = Function::new([]);
    raw.instruction(&Instruction::LocalGet(0));
    raw.instruction(&Instruction::End);
    let mut wrapped = Function::new([]);
    wrapped.instruction(&Instruction::I64Const(2));
    wrapped.instruction(&Instruction::End);
    let wasm = ContractModule::new()
        .func(FuncDef::exported("f", vec![ValType::I32], vec![ValType::I32], raw))
        .func(FuncDef::exported("f", Vec::new(), vec![ValType::I64], wrapped))
        .finish();

    let _guard = session();
    let host = host();
    let err = upload(&host, &wasm).expect_err("a duplicate export name must be refused");
    let rendered = describe(&err);
    assert!(
        rendered.contains("Error(WasmVm, InvalidAction)"),
        "expected the duplicate-export refusal, got: {rendered}"
    );
}

// ---------------------------------------------------------------------------
// Linear memory and data segments
//
// Every contract the compiler actually produces carries both, so the shape has
// to be measured even though a scalar-only fixture does not need it.
// ---------------------------------------------------------------------------

/// Reads the first four bytes of memory and wraps them as a `U32Val`.
fn load_first_word() -> Function {
    let mut body = Function::new([]);
    for instruction in [
        Instruction::I32Const(0),
        Instruction::I32Load(wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 }),
        Instruction::I64ExtendI32U,
        Instruction::I64Const(32),
        Instruction::I64Shl,
        Instruction::I64Const(4),
        Instruction::I64Or,
        Instruction::End,
    ] {
        body.instruction(&instruction);
    }
    body
}

#[test]
fn a_one_page_memory_with_a_data_segment_uploads_and_invokes() {
    let wasm = ContractModule::new()
        .memory(1, "memory")
        .data(0, &42u32.to_le_bytes())
        .func(FuncDef::exported("f", Vec::new(), vec![ValType::I64], load_first_word()))
        .finish();

    let _guard = session();
    let host = host();
    let contract = upload(&host, &wasm).expect("a module with memory and data uploads");
    let result = call(&host, contract, "f", &[]).expect("the contract invokes");
    assert_eq!(
        decode(result),
        Decoded::U32(42),
        "the data segment must have been written into the instantiated memory"
    );
}

/// The `memory` export name is not enforced: the host only ever *looks up* that
/// literal, so a misnamed memory is a silent absence rather than a refusal. A
/// scalar contract never asks the host to read its memory, so nothing here goes
/// wrong — which is exactly why the emitter cannot rely on being told.
#[test]
fn a_memory_exported_under_another_name_is_not_refused() {
    let wasm = ContractModule::new()
        .memory(1, "mem")
        .data(0, &42u32.to_le_bytes())
        .func(FuncDef::exported("f", Vec::new(), vec![ValType::I64], load_first_word()))
        .finish();

    let _guard = session();
    let host = host();
    let contract = upload(&host, &wasm).expect("a misnamed memory export is not refused");
    let result = call(&host, contract, "f", &[]).expect("the contract still invokes");
    assert_eq!(decode(result), Decoded::U32(42));
}

/// A data segment running past the declared initial memory is refused at
/// *upload*, because the upload instantiates a throwaway VM and instantiation is
/// where a data segment is written.
#[test]
fn a_data_segment_past_the_declared_initial_memory_is_refused() {
    let wasm = ContractModule::new()
        .memory(1, "memory")
        .data(65_534, &[1, 2, 3, 4])
        .func(FuncDef::exported("f", Vec::new(), vec![ValType::I64], load_first_word()))
        .finish();

    let _guard = session();
    let host = host();
    let err = upload(&host, &wasm).expect_err("an out-of-bounds data segment must be refused");
    let rendered = describe(&err);
    assert!(
        rendered.contains("Error(WasmVm, IndexBounds)")
            && rendered.contains("Memory(OutOfBoundsAccess)"),
        "expected the out-of-bounds data refusal, got: {rendered}"
    );
}

/// The metadata section is the one byte sequence the rewriter must reproduce
/// verbatim and cannot derive from the module it is rewriting, so it is pinned
/// as bytes rather than as a length.
#[test]
fn the_reference_contract_ends_with_the_exact_metadata_section() {
    let wasm = add_contract(DECLARED_PROTOCOL);
    let expected: [u8; 32] = [
        0x00, 0x1e, 0x11, 0x63, 0x6f, 0x6e, 0x74, 0x72, 0x61, 0x63, 0x74, 0x65, 0x6e, 0x76, 0x6d,
        0x65, 0x74, 0x61, 0x76, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00,
        0x00, 0x00,
    ];
    assert_eq!(
        &wasm[wasm.len() - 32..],
        &expected,
        "the contractenvmetav0 section moved; MEASURED_ABI.md records these 32 bytes"
    );
    assert_eq!(
        std::str::from_utf8(&expected[3..20]),
        Ok(ENV_META_SECTION),
        "the pinned bytes must spell the section name the host looks for"
    );
}
