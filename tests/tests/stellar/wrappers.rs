//! Measures the `Val` marshalling sequences against the host.
//!
//! Every sequence in this module is assembled, uploaded and executed. The
//! assertions are of two kinds: a byte pin, so the exact encoding recorded in
//! `MEASURED_ABI.md` cannot silently drift, and a round trip through the real
//! host, so a sequence that encodes cleanly but marshals wrongly cannot pass.

use wasm_encoder::{BlockType, Encode, Function, Instruction, ValType};

use crate::support::{
    ContractModule, Decoded, FuncDef, TAG_I32VAL, TAG_U32VAL, bool_val, call, decode, describe,
    hex, host, i32_val, session, u32_val, upload, void_val,
};

// ---------------------------------------------------------------------------
// The sequences under measurement
// ---------------------------------------------------------------------------

/// Reads the scalar out of parameter `index`, trapping unless its tag is `tag`.
///
/// The `if` block is empty-blocktype and consumes only its own condition, so
/// scalars already pushed by earlier unwraps are untouched by it.
fn unwrap_scalar(index: u32, tag: i64) -> Vec<Instruction<'static>> {
    vec![
        Instruction::LocalGet(index),
        Instruction::I64Const(0xff),
        Instruction::I64And,
        Instruction::I64Const(tag),
        Instruction::I64Ne,
        Instruction::If(BlockType::Empty),
        Instruction::Unreachable,
        Instruction::End,
        Instruction::LocalGet(index),
        Instruction::I64Const(32),
        Instruction::I64ShrU,
        Instruction::I32WrapI64,
    ]
}

fn unwrap_u32(index: u32) -> Vec<Instruction<'static>> {
    unwrap_scalar(index, TAG_U32VAL.cast_signed())
}

/// Identical to the `u32` unwrap but for the tag: `shr_u 32` then `wrap` yields
/// the raw 32-bit pattern, which is already the two's-complement `i32`.
fn unwrap_i32(index: u32) -> Vec<Instruction<'static>> {
    unwrap_scalar(index, TAG_I32VAL.cast_signed())
}

/// Reads the boolean out of parameter `index`.
///
/// `False` and `True` are tags 0 and 1 with an empty body, so the tag *is* the
/// value once it is known to be in range; the guard is an unsigned `>` rather
/// than the scalar unwrap's `!=`.
fn unwrap_bool(index: u32) -> Vec<Instruction<'static>> {
    vec![
        Instruction::LocalGet(index),
        Instruction::I64Const(0xff),
        Instruction::I64And,
        Instruction::I64Const(1),
        Instruction::I64GtU,
        Instruction::If(BlockType::Empty),
        Instruction::Unreachable,
        Instruction::End,
        Instruction::LocalGet(index),
        Instruction::I64Const(0xff),
        Instruction::I64And,
        Instruction::I32WrapI64,
    ]
}

/// Wraps the `i32` on the stack as a scalar `Val` with tag `tag`.
///
/// The payload lands at bit 32, which is what leaves the minor bits clear and
/// therefore what makes the returned word pass the host's `is_good()` check.
fn wrap_scalar(tag: i64) -> Vec<Instruction<'static>> {
    vec![
        Instruction::I64ExtendI32U,
        Instruction::I64Const(32),
        Instruction::I64Shl,
        Instruction::I64Const(tag),
        Instruction::I64Or,
    ]
}

fn wrap_u32() -> Vec<Instruction<'static>> {
    wrap_scalar(TAG_U32VAL.cast_signed())
}

fn wrap_i32() -> Vec<Instruction<'static>> {
    wrap_scalar(TAG_I32VAL.cast_signed())
}

/// Wraps the `i32` on the stack as `True`/`False`.
///
/// The `i32.const 0; i32.ne` is not decoration: `True` is tag 1 with an *empty*
/// body, so a source-level boolean carrying any other nonzero pattern must be
/// canonicalized to 1 before it becomes a tag.
fn wrap_bool() -> Vec<Instruction<'static>> {
    vec![
        Instruction::I32Const(0),
        Instruction::I32Ne,
        Instruction::I64ExtendI32U,
    ]
}

/// Wraps a call that left nothing on the stack. `Void` is tag 2, empty body.
fn wrap_void() -> Vec<Instruction<'static>> {
    vec![Instruction::I64Const(2)]
}

// ---------------------------------------------------------------------------
// Assembly helpers
// ---------------------------------------------------------------------------

fn encoded(seq: &[Instruction<'static>]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for instruction in seq {
        instruction.encode(&mut bytes);
    }
    bytes
}

fn body(seq: &[Instruction<'static>]) -> Function {
    let mut func = Function::new([]);
    for instruction in seq {
        func.instruction(instruction);
    }
    func.instruction(&Instruction::End);
    func
}

/// A module whose export `f` unwraps one `Val` parameter, calls an inner
/// function taking and returning raw scalars, and wraps the result.
///
/// The inner call is part of the measurement on purpose: it is the shape a
/// wrapper generated around already-emitted code must have, and it is what
/// proves the unwrapped operands survive the guard blocks that precede the call.
fn one_arg_module(
    unwrap: Vec<Instruction<'static>>,
    inner: &[Instruction<'static>],
    inner_result: Option<ValType>,
    wrap: Vec<Instruction<'static>>,
) -> Vec<u8> {
    let module = ContractModule::new().func(FuncDef::internal(
        vec![ValType::I32],
        inner_result.into_iter().collect(),
        body(inner),
    ));
    let inner_index = 0;
    let mut wrapper = unwrap;
    wrapper.push(Instruction::Call(inner_index));
    wrapper.extend(wrap);
    module
        .func(FuncDef::exported("f", vec![ValType::I64], vec![ValType::I64], body(&wrapper)))
        .finish()
}

fn identity() -> Vec<Instruction<'static>> {
    vec![Instruction::LocalGet(0)]
}

fn invoke(wasm: &[u8], arg: soroban_env_host::Val) -> Decoded {
    let _guard = session();
    let host = host();
    let contract = upload(&host, wasm).expect("module uploads");
    let result = call(&host, contract, "f", &[arg]).expect("invocation succeeds");
    decode(result)
}

fn invoke_expecting_trap(wasm: &[u8], arg: soroban_env_host::Val) -> String {
    let _guard = session();
    let host = host();
    let contract = upload(&host, wasm).expect("module uploads");
    match call(&host, contract, "f", &[arg]) {
        Ok(val) => panic!("expected a trap, got {:?}", decode(val)),
        Err(err) => describe(&err),
    }
}

// ---------------------------------------------------------------------------
// Byte pins — these are the sequences MEASURED_ABI.md records
// ---------------------------------------------------------------------------

#[test]
fn measured_sequences_encode_to_the_recorded_bytes() {
    let pins: [(&str, Vec<u8>, &[u8]); 7] = [
        (
            "unwrap u32 (local 0)",
            encoded(&unwrap_u32(0)),
            &[
                0x20, 0x00, 0x42, 0xff, 0x01, 0x83, 0x42, 0x04, 0x52, 0x04, 0x40, 0x00, 0x0b,
                0x20, 0x00, 0x42, 0x20, 0x88, 0xa7,
            ],
        ),
        (
            "unwrap i32 (local 0)",
            encoded(&unwrap_i32(0)),
            &[
                0x20, 0x00, 0x42, 0xff, 0x01, 0x83, 0x42, 0x05, 0x52, 0x04, 0x40, 0x00, 0x0b,
                0x20, 0x00, 0x42, 0x20, 0x88, 0xa7,
            ],
        ),
        (
            "unwrap bool (local 0)",
            encoded(&unwrap_bool(0)),
            &[
                0x20, 0x00, 0x42, 0xff, 0x01, 0x83, 0x42, 0x01, 0x56, 0x04, 0x40, 0x00, 0x0b,
                0x20, 0x00, 0x42, 0xff, 0x01, 0x83, 0xa7,
            ],
        ),
        ("wrap u32", encoded(&wrap_u32()), &[0xad, 0x42, 0x20, 0x86, 0x42, 0x04, 0x84]),
        ("wrap i32", encoded(&wrap_i32()), &[0xad, 0x42, 0x20, 0x86, 0x42, 0x05, 0x84]),
        ("wrap bool", encoded(&wrap_bool()), &[0x41, 0x00, 0x47, 0xad]),
        ("wrap void", encoded(&wrap_void()), &[0x42, 0x02]),
    ];

    for (name, actual, expected) in pins {
        assert_eq!(
            actual,
            expected,
            "{name}: encoding moved; MEASURED_ABI.md records `{}` but this build produced `{}`",
            hex(expected),
            hex(&actual)
        );
    }
}

// ---------------------------------------------------------------------------
// Round trips
// ---------------------------------------------------------------------------

#[test]
fn u32_round_trips_in_both_positions() {
    let wasm = one_arg_module(unwrap_u32(0), &identity(), Some(ValType::I32), wrap_u32());
    for value in [0u32, 1, 7, 0x7fff_ffff, u32::MAX] {
        assert_eq!(invoke(&wasm, u32_val(value)), Decoded::U32(value));
    }
}

#[test]
fn i32_round_trips_in_both_positions() {
    let wasm = one_arg_module(unwrap_i32(0), &identity(), Some(ValType::I32), wrap_i32());
    for value in [0i32, 1, -1, i32::MAX, i32::MIN] {
        assert_eq!(invoke(&wasm, i32_val(value)), Decoded::I32(value));
    }
}

#[test]
fn bool_round_trips_in_both_positions() {
    let wasm = one_arg_module(unwrap_bool(0), &identity(), Some(ValType::I32), wrap_bool());
    assert_eq!(invoke(&wasm, bool_val(true)), Decoded::Bool(true));
    assert_eq!(invoke(&wasm, bool_val(false)), Decoded::Bool(false));
}

#[test]
fn u32_parameter_feeds_a_bool_return() {
    let wasm = one_arg_module(unwrap_u32(0), &identity(), Some(ValType::I32), wrap_bool());
    assert_eq!(invoke(&wasm, u32_val(0)), Decoded::Bool(false));
    assert_eq!(invoke(&wasm, u32_val(1)), Decoded::Bool(true));
}

#[test]
fn bool_parameter_feeds_a_u32_return() {
    let wasm = one_arg_module(unwrap_bool(0), &identity(), Some(ValType::I32), wrap_u32());
    assert_eq!(invoke(&wasm, bool_val(false)), Decoded::U32(0));
    assert_eq!(invoke(&wasm, bool_val(true)), Decoded::U32(1));
}

#[test]
fn unit_return_yields_void() {
    let wasm = one_arg_module(unwrap_u32(0), &[], None, wrap_void());
    assert_eq!(invoke(&wasm, u32_val(42)), Decoded::Void);
}

/// The normalization in the boolean wrap is load-bearing, not defensive.
#[test]
fn bool_wrap_normalizes_a_nonzero_that_is_not_one() {
    let inner = vec![Instruction::I32Const(42)];
    let wasm = one_arg_module(unwrap_u32(0), &inner, Some(ValType::I32), wrap_bool());
    assert_eq!(invoke(&wasm, u32_val(0)), Decoded::Bool(true));
}

/// The same module without the normalization, in the two distinct ways an
/// un-normalized boolean goes wrong.
///
/// An inner `42` widens to a word whose low byte is 42, which is not a tag at
/// all. An inner `257` widens to low byte 1 — `True` — with a set bit in the
/// body, and *that* is the mechanism the ABI rule is about: `True` is tag 1 with
/// an empty body, so a word carrying tag 1 and anything else is refused. The
/// host refuses both, so the two instructions are mandatory rather than
/// stylistic whichever value the source produces.
#[test]
fn bool_wrap_without_normalization_is_refused_by_the_host() {
    let cases = [
        (42i32, Decoded::Other { tag: 42, body: 0 }),
        (257i32, Decoded::Malformed { tag: 1, body: 1 }),
    ];
    for (inner_value, classified) in cases {
        let word = u64::from(inner_value.cast_unsigned());
        assert_eq!(
            decode(soroban_env_host::Val::from_payload(word)),
            classified,
            "the fixture for {inner_value} must exercise the mechanism it is named for"
        );

        let inner = vec![Instruction::I32Const(inner_value)];
        let wasm = one_arg_module(
            unwrap_u32(0),
            &inner,
            Some(ValType::I32),
            vec![Instruction::I64ExtendI32U],
        );
        let rendered = invoke_expecting_trap(&wasm, u32_val(0));
        assert!(
            rendered.contains("Error(Value, UnexpectedType)")
                && rendered.contains("contract call failed"),
            "expected the host to refuse the word {inner_value} widens to, got: {rendered}"
        );
    }
}

// ---------------------------------------------------------------------------
// Adversarial arguments
// ---------------------------------------------------------------------------

#[test]
fn a_void_argument_where_a_u32_is_expected_traps() {
    let wasm = one_arg_module(unwrap_u32(0), &identity(), Some(ValType::I32), wrap_u32());
    let rendered = invoke_expecting_trap(&wasm, void_val());
    assert!(
        rendered.contains("UnreachableCodeReached"),
        "expected the tag guard to trap, got: {rendered}"
    );
    assert!(
        rendered.contains("Error(WasmVm, InvalidAction)"),
        "expected (WasmVm, InvalidAction), got: {rendered}"
    );
}

#[test]
fn an_i32_argument_where_a_u32_is_expected_traps() {
    let wasm = one_arg_module(unwrap_u32(0), &identity(), Some(ValType::I32), wrap_u32());
    let rendered = invoke_expecting_trap(&wasm, i32_val(7));
    assert!(
        rendered.contains("UnreachableCodeReached"),
        "expected the tag guard to trap, got: {rendered}"
    );
}

#[test]
fn a_bool_argument_whose_tag_is_out_of_range_traps() {
    let wasm = one_arg_module(unwrap_bool(0), &identity(), Some(ValType::I32), wrap_bool());

    let rendered = invoke_expecting_trap(&wasm, void_val());
    assert!(
        rendered.contains("UnreachableCodeReached"),
        "expected the range guard to trap on tag 2, got: {rendered}"
    );

    let rendered = invoke_expecting_trap(&wasm, u32_val(0));
    assert!(
        rendered.contains("UnreachableCodeReached"),
        "expected the range guard to trap on tag 4, got: {rendered}"
    );
}
