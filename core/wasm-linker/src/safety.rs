//! Fail-closed operator allow-list for the static merge.
//!
//! The merge copies external function bodies verbatim (re-indexing only the
//! handful of index-bearing operators). That is sound only for the small,
//! well-understood subset of WebAssembly the merge actually models: the integer
//! MVP instruction set plus the bulk-memory `memory.copy`/`memory.fill` forms
//! over the single shared memory.
//!
//! ## No floating point
//!
//! The Inference language has no `f32`/`f64` types, and the Rocq translator
//! (`wasm-to-v`) models no float instruction. The feature gate
//! ([`crate::SUPPORTED_WASM_FEATURES`]) already rejects an external carrying any
//! float type or instruction before its body reaches this allow-list, but the
//! main-module re-encode path does not pass through that gate, so this allow-list
//! is also the float backstop: every float operator (loads/stores, constants,
//! arithmetic, comparisons, conversions, reinterprets) is rejected here with a
//! "floating-point" diagnostic. The merge thus never copies a float instruction,
//! from either module role.
//!
//! The scalar integer set the merge models does extend past the MVP in two
//! places, because the Rocq translator lowers both: the sign-extension operators
//! (`i32.extend8_s`, …), which the proof model spells as ordinary unops, and the
//! three integer-to-integer width conversions (`i32.wrap_i64`,
//! `i64.extend_i32_s/u`), which it spells as `BI_cvtop`. Inference codegen emits
//! neither; they are here so a foreign external compiled by a real toolchain can
//! link.
//!
//! Every *other* operator family — atomics, SIMD, exception handling, typed
//! function references, GC, stack switching, tail calls, saturating
//! float-to-int, and segment-indexed table initialization — carries
//! semantics the merge or the Rocq translator cannot satisfy: a shared/atomic
//! memory it does not reconcile, a tag section it drops, a type index it never
//! interns, a reference type it cannot encode, or a conversion naming a float
//! type the translator has no lowering for. Copying such an operator verbatim
//! produces a structurally-invalid module, an untranslatable proof artifact, or
//! a silent miscompile.
//!
//! ## Verification-only constructs are not executable
//!
//! The Inference non-deterministic blocks (`forall`/`exists`/`assume`/`unique`)
//! and the uzumaki rvalues (`i32.uzumaki`/`i64.uzumaki`) are **proof-only**:
//! they have meaning solely in the Rocq lowering and no executable runtime
//! semantics. A function that gets *merged* into the output is, by construction,
//! part of an executable binary — so a verification-only opcode inside such a
//! body would make the output non-executable (a miscompile). This allow-list
//! therefore **rejects** every non-det/uzumaki opcode: an external whose
//! merged-closure body carries one is surfaced as
//! [`LinkError::UnsupportedConstruct`] rather than copied verbatim. (The main
//! module in proof mode legitimately carries these opcodes as proof scaffolding;
//! it is rebuilt through a separate verbatim path that never consults this
//! allow-list — see [`crate::rewrite`].)
//!
//! This module is the single source of truth for what may cross the merge. It
//! is **fail-closed**: an operator is accepted only if it is explicitly on the
//! safe list, so a future opcode family added to the parser cannot fall through
//! a wildcard arm and be copied silently. Both the closure effect scanner
//! ([`crate::closure`]) and the body re-encoder ([`crate::rewrite`]) gate on
//! [`check_operator`], so an unmergeable construct is rejected the first time
//! it is seen, before any output index is committed.

use inf_wasmparser::{MemArg, Operator};

use crate::LinkError;

/// Maximum structured-control-flow nesting depth a mergeable external body may
/// reach.
///
/// The merge copies bodies verbatim, but the downstream wasm-to-v translator
/// builds and renders an expression tree by self-recursion (one frame per
/// nesting level). A body of thousands of nested blocks overflows the
/// translator's stack — an unrecoverable `abort()` on the `-v` proof path.
/// Rejecting an over-nested body here, during the closure scan that backs the
/// `link`/`-o` path, turns that DoS into a clean [`LinkError`] *before* the
/// body is committed to the merged module, so neither the `-o` nor the `-v`
/// path can reach the translator with a body it cannot render. The bound
/// matches the translator's own cap so the two passes agree on what is
/// admissible, and sits far above any nesting a real Inference function emits.
pub(crate) const MAX_CONTROL_DEPTH: usize = 256;

/// Whether `op` opens a structured-control-flow region (a matching `End`
/// closes it). Used to bound nesting depth during the closure scan.
pub(crate) fn opens_control_frame(op: &Operator) -> bool {
    use Operator::*;
    matches!(
        op,
        Block { .. }
            | Loop { .. }
            | If { .. }
            | Forall { .. }
            | Exists { .. }
            | Assume { .. }
            | Unique { .. }
    )
}

/// Whether `op` is a verification-only construct: an Inference
/// non-deterministic block (`forall`/`exists`/`assume`/`unique`) or an uzumaki
/// rvalue (`i32.uzumaki`/`i64.uzumaki`).
///
/// These opcodes have meaning only in the Rocq lowering and no executable
/// runtime semantics, so they must never appear inside an executable function
/// the merge copies into the output. [`check_operator`] rejects them on the
/// strength of this predicate; the main-module verbatim re-encode path
/// (`crate::rewrite`) uses it to *recognise and pass through* the same opcodes,
/// which are legitimate proof scaffolding there.
pub(crate) fn is_verification_only(op: &Operator) -> bool {
    use Operator::*;
    matches!(
        op,
        Forall { .. }
            | Exists { .. }
            | Assume { .. }
            | Unique { .. }
            | I32Uzumaki { .. }
            | I64Uzumaki { .. }
    )
}

/// What an operator touches, for tier classification. Computed as a side effect
/// of the safety check so the closure scanner and the allow-list never disagree
/// about an operator's category.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct OpEffect {
    /// The operator accesses linear memory by address (load/store/size/grow/
    /// copy/fill). Drives Tier-A vs Tier-B classification.
    pub uses_memory: bool,
    /// The operator grows linear memory (`memory.grow`). Recorded separately
    /// from `uses_memory` so the merge can reconcile (or reject) growth against
    /// the reconciled output memory's maximum.
    pub uses_memory_grow: bool,
    /// The operator reads or writes a global.
    pub uses_globals: bool,
    /// The operator refers to a data segment (`memory.init` / `data.drop`).
    pub uses_data_segments: bool,
    /// The operator touches the table / element space (`call_indirect`,
    /// `table.*`, `ref.func`, `elem.drop`, `memory.init` element forms).
    pub uses_tables: bool,
}

/// Verifies that `op` is one the static merge can soundly copy, returning the
/// effects it carries for tier classification.
///
/// # Contract: every allow-listed operator must be translatable
///
/// An operator admitted here can end up in the linker's output, which the
/// downstream `wasm-to-v` translator must lower to Rocq without panicking. The
/// two instruction sets are kept in lockstep by the integration test
/// `tests/v_alignment.rs`: it links a fixture exercising each allow-listed family
/// and asserts the linked output translates. Any operator family newly added to
/// this allow-list (or to the feature gate in [`crate::SUPPORTED_WASM_FEATURES`])
/// therefore requires a corresponding corpus entry in `tests/v_alignment.rs`,
/// confirming the translator has a lowering for it. Admitting a family the
/// translator hits `todo!()` on yields a clean link followed by an unrecoverable
/// abort on the `-v` proof path.
///
/// # Errors
///
/// Returns [`LinkError::UnsupportedConstruct`] for any operator outside the
/// proven-safe set: any floating-point instruction (the Inference language has
/// no `f32`/`f64` types, see [`is_float`]), atomics, SIMD, exception handling,
/// typed references, GC, stack switching, tail calls, saturating float-to-int,
/// segment-indexed table initialization, multi-memory access (a
/// non-zero memarg memory index), the verification-only non-det/uzumaki opcodes
/// (which have no executable semantics, see [`is_verification_only`]), and any
/// other operator family the merge does not model.
pub(crate) fn check_operator(op: &Operator) -> Result<OpEffect, LinkError> {
    use Operator::*;

    // Verification-only constructs (non-det blocks and uzumaki) carry no
    // executable semantics: they exist solely for the Rocq lowering. A merged
    // function is part of an executable binary, so copying one of these opcodes
    // into it would yield a non-executable output (a miscompile). Reject before
    // any effect classification, so neither the closure scan nor the re-encoder
    // can admit one. The main module's proof scaffolding is rebuilt through a
    // separate verbatim path that never reaches this allow-list.
    if is_verification_only(op) {
        return Err(LinkError::UnsupportedConstruct(format!(
            "verification-only construct {} has no executable semantics and cannot be merged into an executable binary",
            verification_only_family(op)
        )));
    }

    // Reject every floating-point instruction. The Inference language has no
    // `f32`/`f64` types, so its codegen never emits one, and the Rocq translator
    // models no float operator: a merged float instruction would be either an
    // untranslatable proof artifact or a miscompile. The feature gate already
    // rejects a float-using external before its body reaches here, but the
    // main-module re-encode path bypasses that gate, so this is the float
    // backstop on the executable merge path. Reject right after the
    // verification-only check, before any effect classification.
    if is_float(op) {
        return Err(LinkError::UnsupportedConstruct(format!(
            "floating-point instruction `{}` is not supported by the static merge: the Inference language has no f32/f64 types",
            float_mnemonic(op)
        )));
    }

    // Reject any memory access that names a memory other than the single shared
    // memory 0. This closes the multi-memory miscompile (H14) uniformly for
    // every memarg-bearing operator, including ones added to the parser later.
    let reject_nonzero_memory = |memarg: &MemArg| -> Result<(), LinkError> {
        if memarg.memory != 0 {
            return Err(LinkError::UnsupportedConstruct(format!(
                "memory access targets memory {} (multi-memory is not supported by the static merge)",
                memarg.memory
            )));
        }
        Ok(())
    };

    let effect = match op {
        // -- Structured control flow (block types handled by the re-encoder) --
        Unreachable | Nop | Block { .. } | Loop { .. } | If { .. } | Else | End | Br { .. }
        | BrIf { .. } | BrTable { .. } | Return => OpEffect::default(),

        // The Inference non-deterministic block extensions
        // (`forall`/`exists`/`assume`/`unique`) are verification-only and are
        // rejected above by `is_verification_only`; they never reach this match.

        // -- Direct calls (function index re-encoded). The tail-call form
        //    (`return_call`) is rejected as an unmodeled family below: the Rocq
        //    translator has no lowering for it, and Inference codegen never emits
        //    it. --
        Call { .. } => OpEffect::default(),

        // -- Indirect calls touch the table/type space. The tail-call form
        //    (`return_call_indirect`) is rejected as an unmodeled family below
        //    for the same reason as `return_call`. --
        CallIndirect { .. } => OpEffect {
            uses_tables: true,
            ..OpEffect::default()
        },

        // -- Parametric --
        Drop | Select => OpEffect::default(),

        // -- Locals --
        LocalGet { .. } | LocalSet { .. } | LocalTee { .. } => OpEffect::default(),

        // -- Globals --
        GlobalGet { .. } | GlobalSet { .. } => OpEffect {
            uses_globals: true,
            ..OpEffect::default()
        },

        // -- Integer memory load/store over the single shared memory.
        //    The float forms (`f32.load`/`f64.store`/…) are rejected above by
        //    `is_float`; they never reach this match. --
        I32Load { memarg } | I64Load { memarg }
        | I32Load8S { memarg } | I32Load8U { memarg } | I32Load16S { memarg }
        | I32Load16U { memarg } | I64Load8S { memarg } | I64Load8U { memarg }
        | I64Load16S { memarg } | I64Load16U { memarg } | I64Load32S { memarg }
        | I64Load32U { memarg } | I32Store { memarg } | I64Store { memarg }
        | I32Store8 { memarg }
        | I32Store16 { memarg } | I64Store8 { memarg } | I64Store16 { memarg }
        | I64Store32 { memarg } => {
            reject_nonzero_memory(memarg)?;
            OpEffect {
                uses_memory: true,
                ..OpEffect::default()
            }
        }
        MemorySize { mem } => {
            if *mem != 0 {
                return Err(LinkError::UnsupportedConstruct(format!(
                    "memory access targets memory {mem} (multi-memory is not supported by the static merge)"
                )));
            }
            OpEffect {
                uses_memory: true,
                ..OpEffect::default()
            }
        }
        MemoryGrow { mem } => {
            if *mem != 0 {
                return Err(LinkError::UnsupportedConstruct(format!(
                    "memory access targets memory {mem} (multi-memory is not supported by the static merge)"
                )));
            }
            OpEffect {
                uses_memory: true,
                uses_memory_grow: true,
                ..OpEffect::default()
            }
        }

        // -- Bulk memory over the single shared memory --
        MemoryFill { mem } => {
            if *mem != 0 {
                return Err(LinkError::UnsupportedConstruct(format!(
                    "memory.fill targets memory {mem} (multi-memory is not supported by the static merge)"
                )));
            }
            OpEffect {
                uses_memory: true,
                ..OpEffect::default()
            }
        }
        MemoryCopy { dst_mem, src_mem } => {
            if *dst_mem != 0 || *src_mem != 0 {
                return Err(LinkError::UnsupportedConstruct(format!(
                    "memory.copy crosses memories {src_mem} -> {dst_mem} (multi-memory is not supported by the static merge)"
                )));
            }
            OpEffect {
                uses_memory: true,
                ..OpEffect::default()
            }
        }
        // Segment-indexed bulk-memory forms carry their own static data /
        // elements, which the merge cannot relocate: surface them as Tier-C
        // effects (data / table use) rather than copy them.
        MemoryInit { mem, .. } => {
            if *mem != 0 {
                return Err(LinkError::UnsupportedConstruct(format!(
                    "memory.init targets memory {mem} (multi-memory is not supported by the static merge)"
                )));
            }
            OpEffect {
                uses_memory: true,
                uses_data_segments: true,
                ..OpEffect::default()
            }
        }
        DataDrop { .. } => OpEffect {
            uses_data_segments: true,
            ..OpEffect::default()
        },
        // The segment-indexed table forms (`table.init`/`elem.drop`/`table.copy`)
        // carry their own element segments the merge cannot relocate, and the
        // Rocq translator has no lowering for them; they are rejected as an
        // unmodeled family below. The non-segment table accessors are modeled.
        TableGet { .. } | TableSet { .. } | TableGrow { .. } | TableSize { .. }
        | TableFill { .. } => OpEffect {
            uses_tables: true,
            ..OpEffect::default()
        },
        RefFunc { .. } => OpEffect {
            uses_tables: true,
            ..OpEffect::default()
        },

        // -- Integer constants. The float constants (`f32.const`/`f64.const`)
        //    are rejected above by `is_float`; they never reach this match. --
        I32Const { .. } | I64Const { .. } => OpEffect::default(),

        // The Inference uzumaki rvalues (`i32.uzumaki`/`i64.uzumaki`) are
        // verification-only and are rejected above by `is_verification_only`;
        // they never reach this match.

        // -- Numeric (comparisons, arithmetic, conversions) --
        _ if is_numeric(op) => OpEffect::default(),

        // -- Everything else is fail-closed --
        other => {
            return Err(LinkError::UnsupportedConstruct(format!(
                "operator {} is not supported by the static merge",
                operator_family(other)
            )));
        }
    };

    Ok(effect)
}

/// Whether `op` is a pure integer numeric operator: an integer comparison,
/// arithmetic, bitwise, sign-extension, or integer-to-integer width-conversion
/// instruction. These carry no index and no effect, so they are always safe to
/// copy verbatim.
///
/// Float numeric operators are deliberately excluded — they are rejected up
/// front by [`is_float`] (the Inference language has no `f32`/`f64` types). So
/// are the conversions that name a float on either side: saturating and
/// non-saturating float-to-int (`i32.trunc_sat_f32_s`, `i32.trunc_f32_s`, …),
/// int-to-float (`f32.convert_i32_s`, …), the float-to-float
/// `demote`/`promote`, and the reinterprets. The Rocq translator declares no
/// float number type, so a conversion mentioning one has no lowering; it rejects
/// as an unmodeled family rather than copying into a body the `-v` proof path
/// cannot render.
///
/// Inference codegen emits none of the eight admitted here — it narrows sub-i32
/// values with shifts and masks — so they arrive only from a foreign external.
/// Each has a `BI_unop`/`BI_cvtop` lowering in `wasm-to-v`, which is the
/// standard this list is held to: an allow-listed operator must be one the
/// translator can render. `core/wasm-linker/tests/v_alignment.rs` pins that
/// agreement for both groups.
fn is_numeric(op: &Operator) -> bool {
    use Operator::*;
    matches!(
        op,
        // i32 comparisons
        I32Eqz | I32Eq | I32Ne | I32LtS | I32LtU | I32GtS | I32GtU | I32LeS | I32LeU | I32GeS
            | I32GeU
        // i64 comparisons
            | I64Eqz | I64Eq | I64Ne | I64LtS | I64LtU | I64GtS | I64GtU | I64LeS | I64LeU
            | I64GeS | I64GeU
        // i32 arithmetic / bitwise
            | I32Clz | I32Ctz | I32Popcnt | I32Add | I32Sub | I32Mul | I32DivS | I32DivU
            | I32RemS | I32RemU | I32And | I32Or | I32Xor | I32Shl | I32ShrS | I32ShrU | I32Rotl
            | I32Rotr
        // i64 arithmetic / bitwise
            | I64Clz | I64Ctz | I64Popcnt | I64Add | I64Sub | I64Mul | I64DivS | I64DivU
            | I64RemS | I64RemU | I64And | I64Or | I64Xor | I64Shl | I64ShrS | I64ShrU | I64Rotl
            | I64Rotr
        // sign-extension (`BI_unop … (Unop_extend n)` in the proof model, not a
        // conversion — the model groups it with clz/ctz/popcnt)
            | I32Extend8S | I32Extend16S | I64Extend8S | I64Extend16S | I64Extend32S
        // integer-to-integer width conversions (`BI_cvtop`)
            | I32WrapI64 | I64ExtendI32S | I64ExtendI32U
    )
}

/// Whether `op` is a floating-point instruction: a float comparison,
/// arithmetic, conversion, reinterpret, load/store, or constant.
///
/// The Inference language has no `f32`/`f64` types, so its codegen never emits
/// one, and the Rocq translator models none of them. [`check_operator`] rejects
/// every such operator with a "floating-point" diagnostic, the executable-merge
/// backstop to the feature gate (which rejects a float-using external before its
/// body reaches the allow-list, but which the main-module re-encode path does
/// not traverse).
fn is_float(op: &Operator) -> bool {
    use Operator::*;
    matches!(
        op,
        // float comparisons
        F32Eq | F32Ne | F32Lt | F32Gt | F32Le | F32Ge
            | F64Eq | F64Ne | F64Lt | F64Gt | F64Le | F64Ge
        // f32 arithmetic
            | F32Abs | F32Neg | F32Ceil | F32Floor | F32Trunc | F32Nearest | F32Sqrt | F32Add
            | F32Sub | F32Mul | F32Div | F32Min | F32Max | F32Copysign
        // f64 arithmetic
            | F64Abs | F64Neg | F64Ceil | F64Floor | F64Trunc | F64Nearest | F64Sqrt | F64Add
            | F64Sub | F64Mul | F64Div | F64Min | F64Max | F64Copysign
        // float-involving conversions
            | I32TruncF32S | I32TruncF32U | I32TruncF64S | I32TruncF64U
            | I64TruncF32S | I64TruncF32U | I64TruncF64S | I64TruncF64U
            | F32ConvertI32S | F32ConvertI32U | F32ConvertI64S | F32ConvertI64U
            | F32DemoteF64 | F64ConvertI32S | F64ConvertI32U | F64ConvertI64S | F64ConvertI64U
            | F64PromoteF32
        // reinterprets between float and integer
            | I32ReinterpretF32 | I64ReinterpretF64 | F32ReinterpretI32 | F64ReinterpretI64
        // saturating float-to-int conversions
            | I32TruncSatF32S | I32TruncSatF32U | I32TruncSatF64S | I32TruncSatF64U
            | I64TruncSatF32S | I64TruncSatF32U | I64TruncSatF64S | I64TruncSatF64U
        // float loads / stores
            | F32Load { .. } | F64Load { .. } | F32Store { .. } | F64Store { .. }
        // float constants
            | F32Const { .. } | F64Const { .. }
    )
}

/// A human-readable mnemonic for a floating-point operator, for the rejection
/// diagnostic. Only the float opcodes [`is_float`] recognises reach this
/// function.
fn float_mnemonic(op: &Operator) -> &'static str {
    use Operator::*;
    match op {
        F32Eq => "f32.eq",
        F32Ne => "f32.ne",
        F32Lt => "f32.lt",
        F32Gt => "f32.gt",
        F32Le => "f32.le",
        F32Ge => "f32.ge",
        F64Eq => "f64.eq",
        F64Ne => "f64.ne",
        F64Lt => "f64.lt",
        F64Gt => "f64.gt",
        F64Le => "f64.le",
        F64Ge => "f64.ge",
        F32Abs => "f32.abs",
        F32Neg => "f32.neg",
        F32Ceil => "f32.ceil",
        F32Floor => "f32.floor",
        F32Trunc => "f32.trunc",
        F32Nearest => "f32.nearest",
        F32Sqrt => "f32.sqrt",
        F32Add => "f32.add",
        F32Sub => "f32.sub",
        F32Mul => "f32.mul",
        F32Div => "f32.div",
        F32Min => "f32.min",
        F32Max => "f32.max",
        F32Copysign => "f32.copysign",
        F64Abs => "f64.abs",
        F64Neg => "f64.neg",
        F64Ceil => "f64.ceil",
        F64Floor => "f64.floor",
        F64Trunc => "f64.trunc",
        F64Nearest => "f64.nearest",
        F64Sqrt => "f64.sqrt",
        F64Add => "f64.add",
        F64Sub => "f64.sub",
        F64Mul => "f64.mul",
        F64Div => "f64.div",
        F64Min => "f64.min",
        F64Max => "f64.max",
        F64Copysign => "f64.copysign",
        I32TruncF32S => "i32.trunc_f32_s",
        I32TruncF32U => "i32.trunc_f32_u",
        I32TruncF64S => "i32.trunc_f64_s",
        I32TruncF64U => "i32.trunc_f64_u",
        I64TruncF32S => "i64.trunc_f32_s",
        I64TruncF32U => "i64.trunc_f32_u",
        I64TruncF64S => "i64.trunc_f64_s",
        I64TruncF64U => "i64.trunc_f64_u",
        F32ConvertI32S => "f32.convert_i32_s",
        F32ConvertI32U => "f32.convert_i32_u",
        F32ConvertI64S => "f32.convert_i64_s",
        F32ConvertI64U => "f32.convert_i64_u",
        F32DemoteF64 => "f32.demote_f64",
        F64ConvertI32S => "f64.convert_i32_s",
        F64ConvertI32U => "f64.convert_i32_u",
        F64ConvertI64S => "f64.convert_i64_s",
        F64ConvertI64U => "f64.convert_i64_u",
        F64PromoteF32 => "f64.promote_f32",
        I32ReinterpretF32 => "i32.reinterpret_f32",
        I64ReinterpretF64 => "i64.reinterpret_f64",
        F32ReinterpretI32 => "f32.reinterpret_i32",
        F64ReinterpretI64 => "f64.reinterpret_i64",
        I32TruncSatF32S => "i32.trunc_sat_f32_s",
        I32TruncSatF32U => "i32.trunc_sat_f32_u",
        I32TruncSatF64S => "i32.trunc_sat_f64_s",
        I32TruncSatF64U => "i32.trunc_sat_f64_u",
        I64TruncSatF32S => "i64.trunc_sat_f32_s",
        I64TruncSatF32U => "i64.trunc_sat_f32_u",
        I64TruncSatF64S => "i64.trunc_sat_f64_s",
        I64TruncSatF64U => "i64.trunc_sat_f64_u",
        F32Load { .. } => "f32.load",
        F64Load { .. } => "f64.load",
        F32Store { .. } => "f32.store",
        F64Store { .. } => "f64.store",
        F32Const { .. } => "f32.const",
        F64Const { .. } => "f64.const",
        _ => "a floating-point instruction",
    }
}

/// A human-readable label for a verification-only operator, for the rejection
/// diagnostic. Only the non-det/uzumaki opcodes [`is_verification_only`]
/// recognises reach this function.
fn verification_only_family(op: &Operator) -> &'static str {
    use Operator::*;
    match op {
        Forall { .. } => "non-deterministic block `forall`",
        Exists { .. } => "non-deterministic block `exists`",
        Assume { .. } => "non-deterministic block `assume`",
        Unique { .. } => "non-deterministic block `unique`",
        I32Uzumaki { .. } => "uzumaki rvalue `i32.uzumaki`",
        I64Uzumaki { .. } => "uzumaki rvalue `i64.uzumaki`",
        _ => "a verification-only construct",
    }
}

/// A human-readable family label for an unsupported operator, for diagnostics.
/// Keeps the error message stable and meaningful without printing the full
/// (often large) operator debug form.
fn operator_family(op: &Operator) -> &'static str {
    use Operator::*;
    match op {
        // Tail calls. The Rocq translator has no lowering for them, and
        // Inference codegen never emits them (the sret-forwarding path lowers to
        // a plain `call`); an external using either is the only source.
        ReturnCall { .. } => "tail calls (return_call)",
        ReturnCallIndirect { .. } => "tail calls (return_call_indirect)",
        // Segment-indexed table initialization. Carries element segments the
        // merge cannot relocate, and the Rocq translator has no lowering for it.
        TableInit { .. } | ElemDrop { .. } | TableCopy { .. } => {
            "segment-indexed table initialization (table.init / elem.drop / table.copy)"
        }
        // Exception handling (and a defined tag section it implies).
        TryTable { .. } | Throw { .. } | ThrowRef => "exception handling (throw / try_table)",
        Try { .. } | Catch { .. } | Rethrow { .. } | Delegate { .. } | CatchAll => {
            "legacy exception handling"
        }
        // Typed function references.
        CallRef { .. } | ReturnCallRef { .. } | RefAsNonNull | BrOnNull { .. }
        | BrOnNonNull { .. } => "typed function references (call_ref / ref.as_non_null)",
        RefNull { .. } | RefIsNull | TypedSelect { .. } => "reference types (ref.null / select t)",
        // Atomics (0xFE threads family).
        AtomicFence | MemoryAtomicNotify { .. } | MemoryAtomicWait32 { .. }
        | MemoryAtomicWait64 { .. } => "atomic memory operations",
        // SIMD (0xFD family). V128Const carries no memarg but is still SIMD.
        V128Const { .. } => "SIMD (v128)",
        _ => "an unmodeled WASM construct",
    }
}

#[cfg(test)]
mod tests {
    //! Direct unit tests for the fail-closed operator allow-list.
    //!
    //! Each test assembles a one-function module whose body contains the
    //! operator under test, extracts that operator from the code section, and
    //! checks it against [`check_operator`]. The proven-safe operators must
    //! return their expected effect; every unmodeled family must reject with
    //! [`LinkError::UnsupportedConstruct`].

    use super::*;
    use inf_wasmparser::{BinaryReader, FunctionBody, Parser, Payload};

    /// Returns the operators of the first function body in a WAT module.
    fn ops(wat: &str) -> Vec<Operator<'static>> {
        let bytes = wat::parse_str(wat).expect("valid WAT");
        // Leak the bytes so the borrowed operators can outlive this helper; the
        // test process is short-lived and this keeps the call sites terse.
        let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        for payload in Parser::new(0).parse_all(bytes) {
            if let Payload::CodeSectionEntry(body) = payload.expect("payload") {
                let func_body = FunctionBody::new(BinaryReader::new(body.as_bytes(), 0));
                return func_body
                    .get_operators_reader()
                    .expect("operators")
                    .into_iter()
                    .map(|op| op.expect("operator"))
                    .collect();
            }
        }
        panic!("no code section");
    }

    /// Whether any operator of the body is rejected by the allow-list.
    fn body_is_rejected(wat: &str) -> bool {
        ops(wat).iter().any(|op| check_operator(op).is_err())
    }

    #[test]
    fn mvp_arithmetic_is_accepted_with_no_effect() {
        for op in ops(
            r#"(module (func (param i32 i32) (result i32)
                 local.get 0 local.get 1 i32.add) (export "f" (func 0)))"#,
        ) {
            let effect = check_operator(&op).expect("mvp op accepted");
            assert!(!effect.uses_memory && !effect.uses_globals && !effect.uses_tables);
        }
    }

    #[test]
    fn memory_load_marks_memory_use() {
        let any_memory = ops(
            r#"(module (memory 1) (func (param i32) (result i32)
                 local.get 0 i32.load) (export "f" (func 0)))"#,
        )
        .iter()
        .any(|op| check_operator(op).is_ok_and(|e| e.uses_memory));
        assert!(any_memory, "i32.load must mark memory use");
    }

    #[test]
    fn global_access_marks_global_use() {
        let any_global = ops(
            r#"(module (global i32 (i32.const 0)) (func (result i32)
                 global.get 0) (export "f" (func 0)))"#,
        )
        .iter()
        .any(|op| check_operator(op).is_ok_and(|e| e.uses_globals));
        assert!(any_global, "global.get must mark global use");
    }

    #[test]
    fn nonzero_memarg_memory_index_is_rejected() {
        // A store naming memory 1 must reject even though `i32.store` over
        // memory 0 is accepted, closing the multi-memory hole uniformly.
        assert!(body_is_rejected(
            r#"(module (memory 1) (memory 1) (func (param i32 i32)
                 local.get 0 local.get 1 i32.store 1) (export "f" (func 0)))"#,
        ));
    }

    #[test]
    fn atomic_op_is_rejected() {
        assert!(body_is_rejected(
            r#"(module (memory 1 1 shared) (func (param i32 i32) (result i32)
                 local.get 0 local.get 1 i32.atomic.rmw.add) (export "f" (func 0)))"#,
        ));
    }

    #[test]
    fn simd_op_is_rejected() {
        assert!(body_is_rejected(
            r#"(module (memory 1) (func (param i32) (result i32)
                 local.get 0 v128.load drop i32.const 0) (export "f" (func 0)))"#,
        ));
    }

    #[test]
    fn exception_handling_is_rejected() {
        assert!(body_is_rejected(
            r#"(module (type (func)) (tag (type 0)) (func (param i32) (result i32)
                 throw 0) (export "f" (func 0)))"#,
        ));
    }

    #[test]
    fn typed_reference_is_rejected() {
        assert!(body_is_rejected(
            r#"(module (func (param i32) (result i32)
                 ref.null func drop local.get 0) (export "f" (func 0)))"#,
        ));
    }

    #[test]
    fn indirect_call_marks_table_use() {
        let any_table = ops(
            r#"(module (type (func)) (table 1 funcref) (func
                 i32.const 0 call_indirect (type 0)) (export "f" (func 0)))"#,
        )
        .iter()
        .any(|op| check_operator(op).is_ok_and(|e| e.uses_tables));
        assert!(any_table, "call_indirect must mark table use");
    }

    /// Wraps a raw code-section body (locals vector + operator stream, no length
    /// prefix) into a one-function module and returns its operators. `wat`
    /// cannot assemble the custom `0xfc`-prefixed Inference opcodes, so the
    /// bodies that exercise them are built byte-by-byte.
    fn body_ops(body: &[u8]) -> Vec<Operator<'static>> {
        use wasm_encoder::{CodeSection, Function, Module, TypeSection};
        let mut module = Module::new();
        let mut types = TypeSection::new();
        types.ty().function([], []);
        module.section(&types);
        let mut funcs = wasm_encoder::FunctionSection::new();
        funcs.function(0);
        module.section(&funcs);
        let mut code = CodeSection::new();
        let mut f = Function::new([]);
        f.raw(body.iter().copied());
        code.function(&f);
        module.section(&code);
        let bytes: &'static [u8] = Box::leak(module.finish().into_boxed_slice());
        for payload in Parser::new(0).parse_all(bytes) {
            if let Payload::CodeSectionEntry(fb) = payload.expect("payload") {
                let func_body = FunctionBody::new(BinaryReader::new(fb.as_bytes(), 0));
                return func_body
                    .get_operators_reader()
                    .expect("operators")
                    .into_iter()
                    .map(|op| op.expect("operator"))
                    .collect();
            }
        }
        panic!("no code section");
    }

    #[test]
    fn nondet_blocks_are_verification_only_and_rejected() {
        // H-2 (corrected): each non-det block is verification-only and has no
        // executable semantics, so the merge allow-list must reject it.
        for sub_opcode in [0x3a, 0x3b, 0x3c, 0x3d] {
            // `<nondet> (empty) end; end` over a one-byte locals vector.
            let body = [0x00, 0xfc, sub_opcode, 0x40, 0x0b, 0x0b];
            for op in body_ops(&body) {
                if matches!(
                    op,
                    Operator::Forall { .. }
                        | Operator::Exists { .. }
                        | Operator::Assume { .. }
                        | Operator::Unique { .. }
                ) {
                    assert!(is_verification_only(&op), "non-det op must be classified verification-only");
                    let err = check_operator(&op).expect_err("non-det op must be rejected");
                    assert!(
                        matches!(err, LinkError::UnsupportedConstruct(_)),
                        "expected UnsupportedConstruct, got {err:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn uzumaki_rvalues_are_verification_only_and_rejected() {
        // H-2 (corrected): each uzumaki rvalue is verification-only and has no
        // executable semantics, so the merge allow-list must reject it.
        for sub_opcode in [0x31, 0x32] {
            // `<uzumaki> drop; end` over a one-byte locals vector.
            let body = [0x00, 0xfc, sub_opcode, 0x1a, 0x0b];
            for op in body_ops(&body) {
                if matches!(op, Operator::I32Uzumaki { .. } | Operator::I64Uzumaki { .. }) {
                    assert!(is_verification_only(&op), "uzumaki must be classified verification-only");
                    let err = check_operator(&op).expect_err("uzumaki must be rejected");
                    assert!(
                        matches!(err, LinkError::UnsupportedConstruct(_)),
                        "expected UnsupportedConstruct, got {err:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn plain_ops_are_not_verification_only() {
        // A guard against the predicate over-matching: ordinary executable ops
        // (arithmetic, calls, constants) must never be flagged verification-only.
        for op in ops(
            r#"(module (func (param i32 i32) (result i32)
                 local.get 0 local.get 1 i32.add) (export "f" (func 0)))"#,
        ) {
            assert!(
                !is_verification_only(&op),
                "{op:?} must not be classified verification-only"
            );
        }
    }

    /// Asserts that some operator of the body rejects via [`check_operator`] with
    /// a message containing every fragment in `needles`. The body is assembled
    /// from WAT (which does not validate features, so float and post-1.0 opcodes
    /// assemble), letting the allow-list be exercised directly.
    fn assert_body_rejects_with(wat: &str, needles: &[&str]) {
        let rejection = ops(wat).iter().find_map(|op| match check_operator(op) {
            Err(LinkError::UnsupportedConstruct(msg)) => Some(msg),
            _ => None,
        });
        let msg = rejection
            .unwrap_or_else(|| panic!("no operator of `{wat}` rejected via check_operator"));
        for needle in needles {
            assert!(
                msg.contains(needle),
                "rejection message {msg:?} must contain {needle:?}"
            );
        }
    }

    #[test]
    fn float_arithmetic_is_rejected_with_mnemonic() {
        // Float arithmetic carries no executable meaning for Inference (no
        // `f32`/`f64` types) and no Rocq lowering: the allow-list rejects it with
        // a "floating-point" diagnostic naming the exact mnemonic.
        assert_body_rejects_with(
            r#"(module (func (param f32 f32) (result f32)
                 local.get 0 local.get 1 f32.add) (export "f" (func 0)))"#,
            &["floating-point", "f32.add"],
        );
        assert_body_rejects_with(
            r#"(module (func (param f64) (result f64)
                 local.get 0 f64.sqrt) (export "f" (func 0)))"#,
            &["floating-point", "f64.sqrt"],
        );
    }

    #[test]
    fn float_load_store_is_rejected_with_mnemonic() {
        assert_body_rejects_with(
            r#"(module (memory 1) (func (param i32) (result f32)
                 local.get 0 f32.load) (export "f" (func 0)))"#,
            &["floating-point", "f32.load"],
        );
        assert_body_rejects_with(
            r#"(module (memory 1) (func (param i32 f64)
                 local.get 0 local.get 1 f64.store) (export "f" (func 0)))"#,
            &["floating-point", "f64.store"],
        );
    }

    #[test]
    fn float_const_is_rejected_with_mnemonic() {
        assert_body_rejects_with(
            r#"(module (func (result f32) f32.const 1) (export "f" (func 0)))"#,
            &["floating-point", "f32.const"],
        );
    }

    #[test]
    fn float_conversion_is_rejected_with_mnemonic() {
        assert_body_rejects_with(
            r#"(module (func (param f32) (result i32)
                 local.get 0 i32.trunc_f32_s) (export "f" (func 0)))"#,
            &["floating-point", "i32.trunc_f32_s"],
        );
        assert_body_rejects_with(
            r#"(module (func (param i64) (result f64)
                 local.get 0 f64.convert_i64_u) (export "f" (func 0)))"#,
            &["floating-point", "f64.convert_i64_u"],
        );
    }

    #[test]
    fn float_reinterpret_is_rejected_with_mnemonic() {
        assert_body_rejects_with(
            r#"(module (func (param f32) (result i32)
                 local.get 0 i32.reinterpret_f32) (export "f" (func 0)))"#,
            &["floating-point", "i32.reinterpret_f32"],
        );
    }

    #[test]
    fn saturating_truncation_is_rejected_with_mnemonic() {
        // A saturating float-to-int conversion is a float op as far as the
        // allow-list is concerned: the Rocq translator has no lowering for it.
        assert_body_rejects_with(
            r#"(module (func (param f32) (result i32)
                 local.get 0 i32.trunc_sat_f32_s) (export "f" (func 0)))"#,
            &["floating-point", "i32.trunc_sat_f32_s"],
        );
    }

    /// Asserts every operator of the body passes [`check_operator`], and that the
    /// operators `expected` names are among them — so a fixture that quietly
    /// stopped assembling the instruction under test cannot pass by carrying only
    /// `local.get`/`end`. `expected` is matched against each operator's debug
    /// spelling.
    fn assert_body_accepts_including(wat: &str, expected: &[&str]) {
        let ops = ops(wat);
        for op in &ops {
            let effect = check_operator(op)
                .unwrap_or_else(|e| panic!("`{op:?}` must be allow-listed, got {e:?}"));
            assert!(
                !effect.uses_memory
                    && !effect.uses_memory_grow
                    && !effect.uses_globals
                    && !effect.uses_data_segments
                    && !effect.uses_tables,
                "`{op:?}` is a pure numeric operator and must carry no effect, got {effect:?}"
            );
        }
        let spellings: Vec<String> = ops.iter().map(|op| format!("{op:?}")).collect();
        for want in expected {
            assert!(
                spellings.iter().any(|s| s == want),
                "`{wat}` must assemble a `{want}` operator; got {spellings:?}"
            );
        }
    }

    #[test]
    fn sign_extension_ops_are_allow_listed() {
        // The five sign-extension opcodes. The Rocq translator lowers each to
        // `BI_unop t (Unop_extend n)` — the proof model treats them as unops
        // rather than conversions — so they meet the standard every allow-listed
        // operator is held to. Inference codegen still emits none of them; a
        // foreign external compiled by a real toolchain is the source.
        assert_body_accepts_including(
            r#"(module (func (param i32) (param i64) (result i32)
                 local.get 1 i64.extend8_s drop
                 local.get 1 i64.extend16_s drop
                 local.get 1 i64.extend32_s drop
                 local.get 0 i32.extend16_s drop
                 local.get 0 i32.extend8_s) (export "f" (func 0)))"#,
            &[
                "I32Extend8S",
                "I32Extend16S",
                "I64Extend8S",
                "I64Extend16S",
                "I64Extend32S",
            ],
        );
    }

    #[test]
    fn tail_call_op_is_rejected() {
        // `return_call` is an integer-typed control op, rejected as the tail-call
        // family by the fail-closed wildcard.
        assert_body_rejects_with(
            r#"(module
                 (func (param i32) (result i32) local.get 0 return_call 1)
                 (func (param i32) (result i32) local.get 0)
                 (export "f" (func 0)))"#,
            &["tail call", "return_call"],
        );
    }

    #[test]
    fn segment_indexed_table_init_is_rejected() {
        // `table.init` carries an element segment the merge cannot relocate and
        // the Rocq translator cannot lower; it rejects as the segment-indexed
        // table-initialization family.
        assert_body_rejects_with(
            r#"(module (table 1 funcref) (elem func 0)
                 (func i32.const 0 i32.const 0 i32.const 0 table.init 0)
                 (export "f" (func 0)))"#,
            &["table.init"],
        );
    }

    #[test]
    fn integer_width_conversions_are_allow_listed() {
        // The three conversions with an integer on both sides. The proof model
        // declares `CVO_wrap` and `CVO_extend` and the translator emits
        // `BI_cvtop` for each, so the "allow-listed implies lowerable" premise
        // holds at the Rocq level and not merely at the Rust one.
        assert_body_accepts_including(
            r#"(module (func (param i64) (result i32)
                 local.get 0 i64.extend_i32_s drop
                 local.get 0 i32.wrap_i64 i64.extend_i32_u drop
                 local.get 0 i32.wrap_i64) (export "f" (func 0)))"#,
            &["I32WrapI64", "I64ExtendI32S", "I64ExtendI32U"],
        );
    }

    #[test]
    fn float_naming_conversions_stay_rejected() {
        // The other half of the conversion block still has no lowering: each of
        // these names a float type the proof contract does not declare. They are
        // classified as float operators (`is_float`), not as an unmodeled family,
        // so the diagnostic carries the exact mnemonic. Pinned beside the
        // admissions above so a future widening of the allow-list has to step
        // over this test rather than past a gap.
        for (wat, mnemonic) in [
            ("i32.trunc_f32_s", "i32.trunc_f32_s"),
            ("i32.trunc_sat_f32_s", "i32.trunc_sat_f32_s"),
            ("i32.reinterpret_f32", "i32.reinterpret_f32"),
        ] {
            assert_body_rejects_with(
                &format!(
                    r#"(module (func (param f32) (result i32)
                         local.get 0 {wat}) (export "f" (func 0)))"#
                ),
                &["floating-point", mnemonic],
            );
        }
        assert_body_rejects_with(
            r#"(module (func (param i32) (result f32)
                 local.get 0 f32.convert_i32_s) (export "f" (func 0)))"#,
            &["floating-point", "f32.convert_i32_s"],
        );
        assert_body_rejects_with(
            r#"(module (func (param f64) (result f32)
                 local.get 0 f32.demote_f64) (export "f" (func 0)))"#,
            &["floating-point", "f32.demote_f64"],
        );
    }
}
