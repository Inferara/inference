//! The `Val` word, and the instruction sequences that move a scalar across it.
//!
//! A Soroban contract method takes and returns 64-bit words. Every word is a
//! tagged union: its low byte is a tag naming what the word holds, and the
//! remaining bits are that tag's body. Three tags carry a scalar the M1 type set
//! can express, and one carries nothing:
//!
//! | Tag | Value | Body |
//! |---|---|---|
//! | `False` | 0 | none — the tag *is* the value |
//! | `True` | 1 | none — the tag *is* the value |
//! | `Void` | 2 | none |
//! | `U32Val` | 4 | the 32-bit payload in bits 32..64 |
//! | `I32Val` | 5 | the 32-bit payload in bits 32..64, two's complement |
//!
//! Every sequence below was executed against `soroban-env-host` 28.0.2 and is
//! recorded byte for byte in `tests/tests/stellar/MEASURED_ABI.md`. The tests at
//! the bottom of this module pin the same bytes, so the two records cannot drift
//! apart silently.
//!
//! # Why an unwrap traps rather than reporting
//!
//! A tag the wrapper did not expect is a caller error with no channel to report
//! it on: the method's result is one `Val`, and every `Val` is a legitimate
//! value of some type. The host renders the resulting trap as
//! `VM call trapped: UnreachableCodeReached` and carries no further
//! discrimination — measured identical for a wrong tag, a wrong argument type
//! and an out-of-range boolean — so a caller learns that the call trapped and
//! nothing more. That is what `soroban-sdk` produces too.

use inference_wasm_codegen::{AbiReturn, AbiType};
use wasm_encoder::{BlockType, Encode, Instruction};

/// The low byte of a `Val`, which names what the rest of the word holds.
const TAG_MASK: i64 = 0xff;

/// Tag `True`. Also the largest tag a boolean may carry, which is what the
/// boolean unwrap's guard compares against.
const TAG_TRUE: i64 = 1;

/// Tag `Void`, the whole word: an empty body means the tag alone is the value.
const TAG_VOID: i64 = 2;

/// Tag `U32Val`.
const TAG_U32: i64 = 4;

/// Tag `I32Val`.
const TAG_I32: i64 = 5;

/// Where a 32-bit payload sits inside the word.
const BODY_SHIFT: i64 = 32;

/// A scalar the Val ABI can carry across a contract boundary.
///
/// The M1 set. Everything else the source can declare at an export — a 64-bit
/// integer, a narrow integer, a struct, an array, an enum — is refused rather
/// than encoded, because each needs a decision this layer has no authority to
/// take: a host object for the compound types, and a truncation-or-refuse rule
/// for the narrow ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValScalar {
    U32,
    I32,
    Bool,
}

impl ValScalar {
    /// The scalar `ty` maps to, or `None` when the Val ABI cannot carry it.
    pub(crate) fn from_abi(ty: &AbiType) -> Option<Self> {
        match ty {
            AbiType::U32 => Some(Self::U32),
            AbiType::I32 => Some(Self::I32),
            AbiType::Bool => Some(Self::Bool),
            _ => None,
        }
    }
}

/// What one exported function gives back across the boundary: a scalar, or
/// nothing at all, which the ABI still encodes as a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValReturn {
    Void,
    Scalar(ValScalar),
}

impl ValReturn {
    /// The return this ABI gives back for `ret`, or `None` when it cannot carry
    /// it. A compound return is `None` here as well: its hidden pointer
    /// parameter is a linear-memory convention with no Val counterpart.
    pub(crate) fn from_abi(ret: &AbiReturn) -> Option<Self> {
        match ret {
            AbiReturn::Unit => Some(Self::Void),
            AbiReturn::Scalar(ty) => ValScalar::from_abi(ty).map(Self::Scalar),
            AbiReturn::Sret(_) => None,
        }
    }
}

/// The instructions that turn the `Val` in local `local` into the WebAssembly
/// `i32` the inner function's parameter expects, trapping when the word does not
/// hold what the signature declared.
///
/// The guard is an empty-blocktype `if` consuming only its own condition, so the
/// operands earlier unwraps left on the stack survive it — which is what lets a
/// multi-parameter wrapper unwrap its arguments in order without a scratch
/// local.
pub(crate) fn unwrap_parameter(scalar: ValScalar, local: u32) -> Vec<Instruction<'static>> {
    match scalar {
        ValScalar::U32 => unwrap_tagged_payload(local, TAG_U32),
        ValScalar::I32 => unwrap_tagged_payload(local, TAG_I32),
        ValScalar::Bool => vec![
            Instruction::LocalGet(local),
            Instruction::I64Const(TAG_MASK),
            Instruction::I64And,
            Instruction::I64Const(TAG_TRUE),
            Instruction::I64GtU,
            Instruction::If(BlockType::Empty),
            Instruction::Unreachable,
            Instruction::End,
            Instruction::LocalGet(local),
            Instruction::I64Const(TAG_MASK),
            Instruction::I64And,
            Instruction::I32WrapI64,
        ],
    }
}

/// The `U32Val`/`I32Val` unwrap, which differ only in the tag they demand.
///
/// `shr_u` then `wrap` leaves the raw 32-bit pattern, which for `I32Val` is
/// already the two's-complement `i32`; no sign extension is wanted or emitted.
fn unwrap_tagged_payload(local: u32, tag: i64) -> Vec<Instruction<'static>> {
    vec![
        Instruction::LocalGet(local),
        Instruction::I64Const(TAG_MASK),
        Instruction::I64And,
        Instruction::I64Const(tag),
        Instruction::I64Ne,
        Instruction::If(BlockType::Empty),
        Instruction::Unreachable,
        Instruction::End,
        Instruction::LocalGet(local),
        Instruction::I64Const(BODY_SHIFT),
        Instruction::I64ShrU,
        Instruction::I32WrapI64,
    ]
}

/// The instructions that turn what the inner function left on the stack into the
/// `Val` word the host reads back.
///
/// The boolean case normalizes before it tags, and that is not defensive. A
/// returned `bool` is only guaranteed nonzero, and `True` is tag 1 with an empty
/// body, so extending an arbitrary truthy `i32` produces a word the host refuses
/// at invoke time: `42` lands on no tag at all, and `257` lands on `True` with a
/// dirty body. Both were measured as `Error(Value, UnexpectedType)`. Dropping
/// these two instructions ships a contract that uploads cleanly and then fails
/// every call that returns a truthy value other than exactly 1.
pub(crate) fn wrap_return(ret: ValReturn) -> Vec<Instruction<'static>> {
    match ret {
        ValReturn::Void => vec![Instruction::I64Const(TAG_VOID)],
        ValReturn::Scalar(ValScalar::U32) => wrap_tagged_payload(TAG_U32),
        ValReturn::Scalar(ValScalar::I32) => wrap_tagged_payload(TAG_I32),
        ValReturn::Scalar(ValScalar::Bool) => vec![
            Instruction::I32Const(0),
            Instruction::I32Ne,
            Instruction::I64ExtendI32U,
        ],
    }
}

/// The `U32Val`/`I32Val` wrap, which differ only in the tag they write.
fn wrap_tagged_payload(tag: i64) -> Vec<Instruction<'static>> {
    vec![
        Instruction::I64ExtendI32U,
        Instruction::I64Const(BODY_SHIFT),
        Instruction::I64Shl,
        Instruction::I64Const(tag),
        Instruction::I64Or,
    ]
}

/// The encoding of `instructions`, with no body prologue and no trailing `end`.
pub(crate) fn encode(instructions: &[Instruction<'static>]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for instruction in instructions {
        instruction.encode(&mut bytes);
    }
    bytes
}

/// How an inadmissible source type is named back to the user: the spelling the
/// source would have used, so the message quotes something the reader wrote.
///
/// A struct or an enum is named by the label the descriptor carries, which is
/// the spelling at the position that used it rather than a canonical identity —
/// good enough for a diagnostic, and the only thing available here.
pub(crate) fn render_type(ty: &AbiType) -> String {
    match ty {
        AbiType::Bool => "bool".to_string(),
        AbiType::I8 => "i8".to_string(),
        AbiType::U8 => "u8".to_string(),
        AbiType::I16 => "i16".to_string(),
        AbiType::U16 => "u16".to_string(),
        AbiType::I32 => "i32".to_string(),
        AbiType::U32 => "u32".to_string(),
        AbiType::I64 => "i64".to_string(),
        AbiType::U64 => "u64".to_string(),
        AbiType::Enum { name } | AbiType::Struct { name } => name.clone(),
        AbiType::Array { elem, len } => format!("[{}; {len}]", render_type(elem)),
    }
}

/// How a return type is named back to the user.
pub(crate) fn render_return(ret: &AbiReturn) -> String {
    match ret {
        AbiReturn::Unit => "()".to_string(),
        AbiReturn::Scalar(ty) | AbiReturn::Sret(ty) => render_type(ty),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The measured bytes, as `MEASURED_ABI.md` records them. Written as hex
    /// text rather than as a byte array so that a reviewer can compare a golden
    /// against that document by eye, one line at a time.
    fn hex(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn u32_parameter_unwrap_matches_the_measured_bytes() {
        assert_eq!(
            hex(&encode(&unwrap_parameter(ValScalar::U32, 0))),
            "20 00 42 ff 01 83 42 04 52 04 40 00 0b 20 00 42 20 88 a7"
        );
    }

    #[test]
    fn i32_parameter_unwrap_matches_the_measured_bytes() {
        assert_eq!(
            hex(&encode(&unwrap_parameter(ValScalar::I32, 0))),
            "20 00 42 ff 01 83 42 05 52 04 40 00 0b 20 00 42 20 88 a7"
        );
    }

    #[test]
    fn bool_parameter_unwrap_matches_the_measured_bytes() {
        assert_eq!(
            hex(&encode(&unwrap_parameter(ValScalar::Bool, 0))),
            "20 00 42 ff 01 83 42 01 56 04 40 00 0b 20 00 42 ff 01 83 a7"
        );
    }

    /// The two tagged unwraps differ in exactly one byte, the tag. Pinning that
    /// separately keeps a future edit to the shared helper from moving both
    /// goldens together in a way each one alone would still accept.
    #[test]
    fn the_two_tagged_unwraps_differ_only_in_the_tag_byte() {
        let u32_bytes = encode(&unwrap_parameter(ValScalar::U32, 0));
        let i32_bytes = encode(&unwrap_parameter(ValScalar::I32, 0));
        let differing: Vec<usize> = u32_bytes
            .iter()
            .zip(&i32_bytes)
            .enumerate()
            .filter(|(_, (left, right))| left != right)
            .map(|(index, _)| index)
            .collect();
        assert_eq!(differing, vec![7]);
        assert_eq!(u32_bytes[7], 4);
        assert_eq!(i32_bytes[7], 5);
    }

    /// Every `local.get` in an unwrap names the local the caller asked for, so a
    /// second parameter reads the second word rather than re-reading the first.
    #[test]
    fn an_unwrap_reads_the_local_it_was_given() {
        assert_eq!(
            hex(&encode(&unwrap_parameter(ValScalar::U32, 1))),
            "20 01 42 ff 01 83 42 04 52 04 40 00 0b 20 01 42 20 88 a7"
        );
        assert_eq!(
            hex(&encode(&unwrap_parameter(ValScalar::Bool, 3))),
            "20 03 42 ff 01 83 42 01 56 04 40 00 0b 20 03 42 ff 01 83 a7"
        );
    }

    #[test]
    fn u32_return_wrap_matches_the_measured_bytes() {
        assert_eq!(
            hex(&encode(&wrap_return(ValReturn::Scalar(ValScalar::U32)))),
            "ad 42 20 86 42 04 84"
        );
    }

    #[test]
    fn i32_return_wrap_matches_the_measured_bytes() {
        assert_eq!(
            hex(&encode(&wrap_return(ValReturn::Scalar(ValScalar::I32)))),
            "ad 42 20 86 42 05 84"
        );
    }

    /// `41 00 47` is the normalization the host requires and the obvious test
    /// cannot see; `ad` alone would be the broken form.
    #[test]
    fn bool_return_wrap_matches_the_measured_bytes_including_the_normalization() {
        let bytes = encode(&wrap_return(ValReturn::Scalar(ValScalar::Bool)));
        assert_eq!(hex(&bytes), "41 00 47 ad");
        assert_eq!(
            &bytes[..3],
            &[0x41, 0x00, 0x47],
            "the normalization must precede the extension, not follow it"
        );
    }

    #[test]
    fn void_return_wrap_matches_the_measured_bytes() {
        assert_eq!(hex(&encode(&wrap_return(ValReturn::Void))), "42 02");
    }

    #[test]
    fn only_the_three_m1_scalars_map_to_a_val() {
        assert_eq!(ValScalar::from_abi(&AbiType::U32), Some(ValScalar::U32));
        assert_eq!(ValScalar::from_abi(&AbiType::I32), Some(ValScalar::I32));
        assert_eq!(ValScalar::from_abi(&AbiType::Bool), Some(ValScalar::Bool));
        for ty in [
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
        ] {
            assert_eq!(ValScalar::from_abi(&ty), None, "{ty:?} must not map");
        }
    }

    #[test]
    fn a_compound_return_maps_to_no_val_return() {
        assert_eq!(ValReturn::from_abi(&AbiReturn::Unit), Some(ValReturn::Void));
        assert_eq!(
            ValReturn::from_abi(&AbiReturn::Scalar(AbiType::Bool)),
            Some(ValReturn::Scalar(ValScalar::Bool))
        );
        assert_eq!(
            ValReturn::from_abi(&AbiReturn::Sret(AbiType::Struct {
                name: "Point".to_string()
            })),
            None
        );
    }

    #[test]
    fn a_type_is_rendered_as_the_source_spells_it() {
        assert_eq!(render_type(&AbiType::U64), "u64");
        assert_eq!(
            render_type(&AbiType::Struct {
                name: "geom::Point".to_string()
            }),
            "geom::Point"
        );
        assert_eq!(
            render_type(&AbiType::Array {
                elem: Box::new(AbiType::Array {
                    elem: Box::new(AbiType::I32),
                    len: 2
                }),
                len: 3
            }),
            "[[i32; 2]; 3]"
        );
        assert_eq!(render_return(&AbiReturn::Unit), "()");
        assert_eq!(
            render_return(&AbiReturn::Sret(AbiType::Array {
                elem: Box::new(AbiType::U32),
                len: 8
            })),
            "[u32; 8]"
        );
    }
}
