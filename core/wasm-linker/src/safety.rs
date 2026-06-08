//! Fail-closed operator allow-list for the static merge.
//!
//! The merge copies external function bodies verbatim (re-indexing only the
//! handful of index-bearing operators). That is sound only for the small,
//! well-understood subset of WebAssembly the merge actually models: the MVP
//! instruction set plus a few additions (sign-extension, saturating
//! conversions, and the bulk-memory `memory.copy`/`memory.fill` forms over the
//! single shared memory).
//!
//! Every *other* operator family — atomics, SIMD, exception handling, typed
//! function references, GC, stack switching — carries semantics the merge
//! cannot satisfy: a shared/atomic memory it does not reconcile, a tag section
//! it drops, a type index it never interns, a reference type it cannot encode.
//! Copying such an operator verbatim produces a structurally-invalid module or
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
/// # Errors
///
/// Returns [`LinkError::UnsupportedConstruct`] for any operator outside the
/// proven-safe set: atomics, SIMD, exception handling, typed references, GC,
/// stack switching, multi-memory access (a non-zero memarg memory index), the
/// verification-only non-det/uzumaki opcodes (which have no executable
/// semantics, see [`is_verification_only`]), and any other operator family the
/// merge does not model.
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

        // -- Direct and tail calls (function index re-encoded) --
        Call { .. } | ReturnCall { .. } => OpEffect::default(),

        // -- Indirect calls touch the table/type space --
        CallIndirect { .. } | ReturnCallIndirect { .. } => OpEffect {
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

        // -- Memory load/store over the single shared memory --
        I32Load { memarg } | I64Load { memarg } | F32Load { memarg } | F64Load { memarg }
        | I32Load8S { memarg } | I32Load8U { memarg } | I32Load16S { memarg }
        | I32Load16U { memarg } | I64Load8S { memarg } | I64Load8U { memarg }
        | I64Load16S { memarg } | I64Load16U { memarg } | I64Load32S { memarg }
        | I64Load32U { memarg } | I32Store { memarg } | I64Store { memarg }
        | F32Store { memarg } | F64Store { memarg } | I32Store8 { memarg }
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
        TableInit { .. } | ElemDrop { .. } | TableCopy { .. } | TableGet { .. }
        | TableSet { .. } | TableGrow { .. } | TableSize { .. } | TableFill { .. } => OpEffect {
            uses_tables: true,
            ..OpEffect::default()
        },
        RefFunc { .. } => OpEffect {
            uses_tables: true,
            ..OpEffect::default()
        },

        // -- Constants --
        I32Const { .. } | I64Const { .. } | F32Const { .. } | F64Const { .. } => {
            OpEffect::default()
        }

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

/// Whether `op` is a pure numeric operator: an integer/float comparison,
/// arithmetic, bitwise, or conversion instruction. These carry no index and no
/// effect, so they are always safe to copy verbatim.
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
        // float comparisons
            | F32Eq | F32Ne | F32Lt | F32Gt | F32Le | F32Ge
            | F64Eq | F64Ne | F64Lt | F64Gt | F64Le | F64Ge
        // i32 arithmetic / bitwise
            | I32Clz | I32Ctz | I32Popcnt | I32Add | I32Sub | I32Mul | I32DivS | I32DivU
            | I32RemS | I32RemU | I32And | I32Or | I32Xor | I32Shl | I32ShrS | I32ShrU | I32Rotl
            | I32Rotr
        // i64 arithmetic / bitwise
            | I64Clz | I64Ctz | I64Popcnt | I64Add | I64Sub | I64Mul | I64DivS | I64DivU
            | I64RemS | I64RemU | I64And | I64Or | I64Xor | I64Shl | I64ShrS | I64ShrU | I64Rotl
            | I64Rotr
        // f32 arithmetic
            | F32Abs | F32Neg | F32Ceil | F32Floor | F32Trunc | F32Nearest | F32Sqrt | F32Add
            | F32Sub | F32Mul | F32Div | F32Min | F32Max | F32Copysign
        // f64 arithmetic
            | F64Abs | F64Neg | F64Ceil | F64Floor | F64Trunc | F64Nearest | F64Sqrt | F64Add
            | F64Sub | F64Mul | F64Div | F64Min | F64Max | F64Copysign
        // conversions
            | I32WrapI64 | I32TruncF32S | I32TruncF32U | I32TruncF64S | I32TruncF64U
            | I64ExtendI32S | I64ExtendI32U | I64TruncF32S | I64TruncF32U | I64TruncF64S
            | I64TruncF64U | F32ConvertI32S | F32ConvertI32U | F32ConvertI64S | F32ConvertI64U
            | F32DemoteF64 | F64ConvertI32S | F64ConvertI32U | F64ConvertI64S | F64ConvertI64U
            | F64PromoteF32 | I32ReinterpretF32 | I64ReinterpretF64 | F32ReinterpretI32
            | F64ReinterpretI64
        // sign-extension proposal
            | I32Extend8S | I32Extend16S | I64Extend8S | I64Extend16S | I64Extend32S
        // saturating float-to-int conversions
            | I32TruncSatF32S | I32TruncSatF32U | I32TruncSatF64S | I32TruncSatF64U
            | I64TruncSatF32S | I64TruncSatF32U | I64TruncSatF64S | I64TruncSatF64U
    )
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
}
