//! Re-encoding a copied function body under a new index space.
//!
//! When a function body is moved from an external module into the main module,
//! every index it references shifts: a `call N` now means a different function,
//! a `call_indirect (type T)` a different type, and so on. This pass walks the
//! operator stream and re-emits it, copying each operator's bytes verbatim
//! *except* the index-bearing operators, which it re-encodes with the remapped
//! index via `wasm-encoder` (so the opcode encoding stays canonical).
//!
//! The verbatim-copy default keeps the body byte-identical wherever no index
//! changes, which both minimizes surface area and makes round-trips exact for
//! the common case where the only remapped operator is `call`.
//!
//! That default is also the pass's one sharp edge, and it cuts hardest on the
//! operators whose index space the output *rearranges*. `call`, `call_indirect`,
//! `global.get` and `global.set` each name a space the merge rebuilds, so each
//! needs an arm of its own; an operator that quietly lacked one would be copied
//! with a stale index that still resolves — against the wrong entity. Adding a
//! remapped index space to the merge therefore means adding an arm here in the
//! same change, never afterwards.

use inf_wasmparser::{BinaryReader, FunctionBody, Operator, ValType};
use wasm_encoder::{Encode, Function, Instruction};

use crate::safety::{check_operator, is_verification_only, opens_control_frame, MAX_CONTROL_DEPTH};
use crate::LinkError;

/// Where a body being re-encoded comes from, which decides how the
/// verification-only non-det/uzumaki opcodes are treated.
///
/// The main module in proof mode legitimately carries these opcodes as Rocq
/// proof scaffolding, so they are copied through verbatim. An external module's
/// body is merged into an executable binary, where the same opcodes have no
/// runtime meaning; they are rejected rather than copied. (The external closure
/// scan rejects them first via [`check_operator`], so this arm is defence in
/// depth for any external body re-encoded directly.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BodyOrigin {
    /// The main module's own body. Verification-only opcodes are proof
    /// scaffolding and pass through unaltered.
    Main,
    /// An external module's body merged into the output. Verification-only
    /// opcodes are rejected as non-executable.
    External,
}

/// `0xfc`-prefix byte shared by the Inference non-deterministic block opcodes
/// (`forall`/`exists`/`assume`/`unique`), mirroring the codegen encoding.
const NONDET_OPCODE_PREFIX: u8 = 0xfc;

/// `0xfc` sub-opcode for `forall`, matching the codegen and `inf-wasmparser`
/// decoder.
const FORALL_SUBOPCODE: u8 = 0x3a;
/// `0xfc` sub-opcode for `exists`.
const EXISTS_SUBOPCODE: u8 = 0x3b;
/// `0xfc` sub-opcode for `assume`.
const ASSUME_SUBOPCODE: u8 = 0x3c;
/// `0xfc` sub-opcode for `unique`.
const UNIQUE_SUBOPCODE: u8 = 0x3d;

/// Maps an index from the source module's space into the merged module's space.
pub(crate) struct IndexMap<'a> {
    /// `source_func_idx -> merged_func_idx` for every function in the closure.
    pub func: &'a dyn Fn(u32) -> u32,
    /// `source_type_idx -> merged_type_idx` for every type the closure uses.
    ///
    /// Fallible: a body can reference a type index the merge never interned
    /// (e.g. a function-typed block over an unused signature, or an out-of-range
    /// index in an adversarial body), which must surface as a [`LinkError`]
    /// rather than panic.
    pub ty: &'a dyn Fn(u32) -> Result<u32, LinkError>,
    /// `source_global_idx -> merged_global_idx` for every global the body may
    /// name.
    ///
    /// Fallible, following `ty` rather than `func`. The distinction between the
    /// other two is not stylistic: `func`'s infallible signature cannot report a
    /// missing mapping, so both call sites in [`crate::merge`] smuggle the error
    /// out through a `RefCell` — a workaround this field has no reason to
    /// inherit. Fallibility is also what makes a *dropped* global space safe. The
    /// merge contributes an external's globals only when its closure reads or
    /// writes one; for every other external the mapping is empty, so a body that
    /// named a global against the closure scanner's verdict fails this lookup
    /// with a clean [`LinkError`] instead of rebinding onto the main module's
    /// state. An infallible `global` would have to invent an index there, which
    /// is the silent miscompile this whole path exists to prevent.
    pub global: &'a dyn Fn(u32) -> Result<u32, LinkError>,
}

/// Re-encodes one function body under `map`, returning a `wasm-encoder`
/// [`Function`] ready to append to the output code section.
///
/// The input `body` is the raw code-section body (locals vector followed by the
/// operator stream, no length prefix), exactly as stored in
/// [`crate::parse::LocalFunc::body`].
///
/// `origin` selects how the verification-only non-det/uzumaki opcodes are
/// handled: passed through verbatim for the main module's proof scaffolding,
/// rejected for an external body merged into the executable output (see
/// [`BodyOrigin`]).
pub(crate) fn reencode_body(
    body: &[u8],
    map: &IndexMap,
    origin: BodyOrigin,
) -> Result<Function, LinkError> {
    let reader = BinaryReader::new(body, 0);
    let func_body = FunctionBody::new(reader);

    let locals = read_locals(&func_body)?;
    let mut function = Function::new(locals);

    // Collect operators with their start offsets so each operator's byte span
    // can be sliced for verbatim copying. The reader starts at 0, so offsets
    // are absolute into `body`; operator `i` spans `[offset_i, offset_{i+1})`,
    // and the last (the body-terminating `end`) runs to `body.len()`.
    let mut ops = Vec::new();
    for item in func_body
        .get_operators_reader()
        .map_err(|e| LinkError::Parse(e.to_string()))?
        .into_iter_with_offsets()
    {
        let (op, offset) = item.map_err(|e| LinkError::Parse(e.to_string()))?;
        ops.push((op, offset));
    }

    // Bound structured-control-flow nesting on this path too. The closure scan
    // gates external bodies, but the main module's body is re-encoded here without
    // passing through that scan; an over-nested main body would link and only fail
    // in the downstream wasm-to-v translator (which recurses one frame per level),
    // violating the invariant that anything the linker emits is translatable.
    // Rejecting here at the same cap the closure scan and the translator use keeps
    // the three passes in agreement. A `block`/`loop`/`if`/non-det op opens a
    // frame; an `End` closes the innermost one.
    let mut control_depth: usize = 0;
    for (i, (op, offset)) in ops.iter().enumerate() {
        if opens_control_frame(op) {
            control_depth += 1;
            if control_depth >= MAX_CONTROL_DEPTH {
                return Err(LinkError::UnsupportedConstruct(format!(
                    "function body nests structured control flow at least {MAX_CONTROL_DEPTH} levels deep"
                )));
            }
        } else if matches!(op, Operator::End) {
            control_depth = control_depth.saturating_sub(1);
        }
        let end = ops.get(i + 1).map_or(body.len(), |(_, o)| *o);
        let span = &body[*offset..end];
        emit_operator(&mut function, op, span, map, origin)?;
    }

    Ok(function)
}

/// Reads the locals declarations from a body into the `(count, ValType)` form
/// `wasm-encoder::Function::new` expects.
fn read_locals(body: &FunctionBody) -> Result<Vec<(u32, wasm_encoder::ValType)>, LinkError> {
    let mut locals_reader = body
        .get_locals_reader()
        .map_err(|e| LinkError::Parse(e.to_string()))?;
    let count = locals_reader.get_count();
    let mut locals = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (n, ty) = locals_reader
            .read()
            .map_err(|e| LinkError::Parse(e.to_string()))?;
        locals.push((n, map_val_type(ty)?));
    }
    Ok(locals)
}

/// Emits a single operator, re-encoding the index-bearing ones and copying the
/// rest verbatim from their original bytes.
///
/// `origin` decides the treatment of the verification-only non-det/uzumaki
/// opcodes: an external body rejects them as non-executable, the main module's
/// body passes them through as proof scaffolding (see [`BodyOrigin`]).
fn emit_operator(
    function: &mut Function,
    op: &Operator,
    span: &[u8],
    map: &IndexMap,
    origin: BodyOrigin,
) -> Result<(), LinkError> {
    // An external body must never carry a verification-only opcode into the
    // executable output. The external closure scan already rejects such a body
    // via `check_operator`, so reaching here means a body re-encoded outside
    // that scan; reject it the same way rather than emit a non-executable block.
    if origin == BodyOrigin::External && is_verification_only(op) {
        check_operator(op)?;
    }
    match op {
        Operator::Call { function_index } => {
            function.instruction(&Instruction::Call((map.func)(*function_index)));
        }
        Operator::RefFunc { function_index } => {
            function.instruction(&Instruction::RefFunc((map.func)(*function_index)));
        }
        Operator::CallIndirect {
            type_index,
            table_index,
        } => {
            function.instruction(&Instruction::CallIndirect {
                type_index: (map.ty)(*type_index)?,
                table_index: *table_index,
            });
        }
        // The global accessors must be re-indexed for the same reason `call` is,
        // and they are the index-bearing operators where falling through to the
        // verbatim arm is *least* visible. The output global space puts the main
        // module's globals first and appends each contributing external's after
        // them, so a copied `global.get 0` from an external would name main's
        // first global — which in real toolchain output is the stack pointer.
        // Two `i32` globals agree in type, so the merged module still validates
        // and still runs: wrong value, no diagnostic anywhere.
        Operator::GlobalGet { global_index } => {
            function.instruction(&Instruction::GlobalGet((map.global)(*global_index)?));
        }
        Operator::GlobalSet { global_index } => {
            function.instruction(&Instruction::GlobalSet((map.global)(*global_index)?));
        }
        // The tail-call forms (`return_call` / `return_call_indirect`) have no
        // arm of their own: the Rocq translator has no lowering for them, and
        // Inference codegen never emits them. They fall through to the final arm,
        // which rejects them via the fail-closed allow-list — closing the bypass
        // that previously re-indexed and copied a tail call on the main path.

        // Block-type operators can carry a type index in their multi-value
        // form. Inference codegen only emits the empty and value block types,
        // but a Tier-A/B external body could use a function block type, so
        // re-encode those defensively rather than copy a now-stale index.
        Operator::Block { blockty }
        | Operator::Loop { blockty }
        | Operator::If { blockty } => {
            emit_block(function, op, *blockty, map)?;
        }
        // The Inference non-det block operators carry the identical `blockty`
        // payload, so their function block-type index must be remapped exactly
        // like `Block`/`Loop`/`If`. `wasm-encoder` models no custom opcode, so
        // they are re-emitted as raw bytes (prefix + sub-opcode + re-encoded
        // block type) rather than via an `Instruction`.
        Operator::Forall { blockty } => {
            emit_nondet_block(function, FORALL_SUBOPCODE, *blockty, map)?;
        }
        Operator::Exists { blockty } => {
            emit_nondet_block(function, EXISTS_SUBOPCODE, *blockty, map)?;
        }
        Operator::Assume { blockty } => {
            emit_nondet_block(function, ASSUME_SUBOPCODE, *blockty, map)?;
        }
        Operator::Unique { blockty } => {
            emit_nondet_block(function, UNIQUE_SUBOPCODE, *blockty, map)?;
        }
        // The main module's verification-only opcodes (the uzumaki rvalues, and
        // any non-det block reached here) are proof scaffolding with no
        // executable meaning: they are copied through verbatim and must bypass
        // the fail-closed allow-list, which rejects them for the executable
        // merge. (An external body's verification-only opcodes were already
        // rejected at the top of this function.)
        _ if origin == BodyOrigin::Main && is_verification_only(op) => {
            function.raw(span.iter().copied());
        }
        _ => {
            // Every other operator carries no index that the merge changes
            // (locals, constants, arithmetic, control flow targets, and
            // memargs over the single shared memory all stay valid), so it is
            // copied verbatim — but only after the fail-closed allow-list
            // confirms the merge models it. An atomic, SIMD, exception-handling,
            // typed-reference, or multi-memory operator is rejected here rather
            // than copied into a structurally-invalid output, even for a body
            // (e.g. the main module's) the closure scanner never walked.
            check_operator(op)?;
            function.raw(span.iter().copied());
        }
    }
    Ok(())
}

fn emit_block(
    function: &mut Function,
    op: &Operator,
    blockty: inf_wasmparser::BlockType,
    map: &IndexMap,
) -> Result<(), LinkError> {
    let encoded = map_block_type(blockty, map)?;
    let instr = match op {
        Operator::Block { .. } => Instruction::Block(encoded),
        Operator::Loop { .. } => Instruction::Loop(encoded),
        Operator::If { .. } => Instruction::If(encoded),
        _ => unreachable!("emit_block called with non-block operator"),
    };
    function.instruction(&instr);
    Ok(())
}

/// Re-emits an Inference non-det block operator (`forall`/`exists`/`assume`/
/// `unique`) with its block-type index remapped into the merged type space.
///
/// `wasm-encoder` has no `Instruction` for the `0xfc`-prefixed custom opcodes,
/// so the operator is written as raw bytes: the `0xfc` prefix, the `sub_opcode`,
/// then the canonical encoding of the remapped block type. The block-type remap
/// is the same fail-closed [`map_block_type`] used by `Block`/`Loop`/`If`, so a
/// function block type whose index the merge never interned (or a reference-typed
/// result) surfaces as a clean [`LinkError`] rather than a verbatim-copied stale
/// index.
fn emit_nondet_block(
    function: &mut Function,
    sub_opcode: u8,
    blockty: inf_wasmparser::BlockType,
    map: &IndexMap,
) -> Result<(), LinkError> {
    let encoded = map_block_type(blockty, map)?;
    let mut bytes = vec![NONDET_OPCODE_PREFIX, sub_opcode];
    encoded.encode(&mut bytes);
    function.raw(bytes);
    Ok(())
}

fn map_block_type(
    blockty: inf_wasmparser::BlockType,
    map: &IndexMap,
) -> Result<wasm_encoder::BlockType, LinkError> {
    Ok(match blockty {
        inf_wasmparser::BlockType::Empty => wasm_encoder::BlockType::Empty,
        // A value block type maps to a single result. A reference-typed result
        // is an unsupported construct (surfaced by `map_val_type`), not a silent
        // fallback to `Empty` — eliding a block's result would corrupt the body.
        inf_wasmparser::BlockType::Type(ty) => {
            wasm_encoder::BlockType::Result(map_val_type(ty)?)
        }
        inf_wasmparser::BlockType::FuncType(type_idx) => {
            wasm_encoder::BlockType::FunctionType((map.ty)(type_idx)?)
        }
    })
}

/// Maps an `inf-wasmparser` value type to the `wasm-encoder` equivalent.
///
/// Rejects floating-point value types: the Inference language has no `f32`/`f64`
/// types, so a float local or float block result cannot appear in a body the
/// merge models. The feature gate rejects a float-using external before its body
/// is re-encoded, but the main-module re-encode path bypasses that gate, so this
/// is the float backstop on the value-type axis (the operator-stream backstop is
/// [`crate::safety::is_float`]). `v128` is rejected for the same reason: the
/// language has no SIMD types and every SIMD operator is rejected, so the type
/// axis must stay consistent. Reference types are likewise unsupported; only the
/// integer value types map through.
fn map_val_type(ty: ValType) -> Result<wasm_encoder::ValType, LinkError> {
    Ok(match ty {
        ValType::I32 => wasm_encoder::ValType::I32,
        ValType::I64 => wasm_encoder::ValType::I64,
        ValType::F32 | ValType::F64 => {
            return Err(LinkError::UnsupportedConstruct(
                "floating-point value type (f32/f64) in merged function body: \
                 the Inference language has no f32/f64 types"
                    .into(),
            ));
        }
        ValType::V128 => {
            return Err(LinkError::UnsupportedConstruct(
                "v128 value type in merged function body: \
                 the Inference language has no SIMD types"
                    .into(),
            ));
        }
        ValType::Ref(_) => {
            return Err(LinkError::UnsupportedConstruct(
                "reference-typed value in merged function body".into(),
            ));
        }
    })
}

#[cfg(test)]
mod tests {
    //! Direct unit tests for the body re-encoder.
    //!
    //! `reencode_body` handles index-bearing operators that the *public* `link`
    //! API never reaches — a body using `call_indirect`, `ref.func`, or a
    //! function-typed block belongs to a module the tier classifier rejects
    //! before any body is re-encoded. These tests drive the re-encoder directly
    //! with synthetic index maps so those defensive arms are exercised and their
    //! remapping verified.

    use super::*;
    use inf_wasmparser::{Parser, Payload};

    /// Extracts the raw body bytes (locals vector + operator stream, no length
    /// prefix) of the function at `func_idx` from a complete module's bytes.
    fn body_bytes(module: &[u8], func_idx: usize) -> Vec<u8> {
        let mut idx = 0;
        for payload in Parser::new(0).parse_all(module) {
            if let Payload::CodeSectionEntry(body) = payload.unwrap() {
                if idx == func_idx {
                    return body.as_bytes().to_vec();
                }
                idx += 1;
            }
        }
        panic!("no body at index {func_idx}");
    }

    /// The operators of the function at `func_idx` of a re-encoded module.
    fn operators(module: &[u8], func_idx: usize) -> Vec<Operator<'_>> {
        let mut idx = 0;
        for payload in Parser::new(0).parse_all(module) {
            if let Payload::CodeSectionEntry(body) = payload.unwrap() {
                if idx == func_idx {
                    return body
                        .get_operators_reader()
                        .unwrap()
                        .into_iter()
                        .map(|op| op.unwrap())
                        .collect();
                }
                idx += 1;
            }
        }
        panic!("no body at index {func_idx}");
    }

    /// Wraps a re-encoded `Function` into a one-function module so it can be
    /// parsed back and inspected. The single type is `() -> ()`; the test bodies
    /// here are validated structurally (operator stream), not type-checked.
    fn wrap(function: &Function) -> Vec<u8> {
        let mut module = wasm_encoder::Module::new();
        let mut types = wasm_encoder::TypeSection::new();
        types.ty().function([], []);
        module.section(&types);
        let mut funcs = wasm_encoder::FunctionSection::new();
        funcs.function(0);
        module.section(&funcs);
        let mut code = wasm_encoder::CodeSection::new();
        code.function(function);
        module.section(&code);
        module.finish()
    }

    /// Shifts every function index by 10, every type index by 100 and every
    /// global index by 1000. The shifts differ by an order of magnitude apiece so
    /// that a remapped index is unmistakable in the output *and* names its own
    /// space: an arm that fed an operand through the wrong map would land on a
    /// value no assertion accepts, rather than on a plausible one.
    static SHIFT_FUNC: fn(u32) -> u32 = |f| f + 10;
    static SHIFT_TYPE: fn(u32) -> Result<u32, LinkError> = |t| Ok(t + 100);
    static SHIFT_GLOBAL: fn(u32) -> Result<u32, LinkError> = |g| Ok(g + 1000);

    /// The index map the re-encoder tests drive, ready to pass to
    /// [`reencode_body`].
    fn shifting_map() -> IndexMap<'static> {
        IndexMap {
            func: &SHIFT_FUNC,
            ty: &SHIFT_TYPE,
            global: &SHIFT_GLOBAL,
        }
    }

    #[test]
    fn reencodes_call_indirect_type_index() {
        // call_indirect's *type* index must be remapped; its table index stays.
        let module = wat::parse_str(
            r#"
            (module
              (type (;0;) (func))
              (table (;0;) 1 funcref)
              (func (;0;) (type 0)
                i32.const 0
                call_indirect (type 0))
              (export "f" (func 0)))
            "#,
        )
        .unwrap();
        let body = body_bytes(&module, 0);

        let map = shifting_map();
        let out = reencode_body(&body, &map, BodyOrigin::External).expect("re-encode call_indirect");
        let wrapped = wrap(&out);

        let has_remapped = operators(&wrapped, 0).into_iter().any(|op| {
            matches!(op, Operator::CallIndirect { type_index, .. } if type_index == 100)
        });
        assert!(has_remapped, "call_indirect type index must be remapped to 100");
    }

    #[test]
    fn reencodes_ref_func_function_index() {
        let module = wat::parse_str(
            r#"
            (module
              (type (;0;) (func))
              (func (;0;) (type 0)
                ref.func 0
                drop)
              (export "f" (func 0)))
            "#,
        )
        .unwrap();
        let body = body_bytes(&module, 0);

        let map = shifting_map();
        let out = reencode_body(&body, &map, BodyOrigin::External).expect("re-encode ref.func");
        let wrapped = wrap(&out);

        let has_remapped = operators(&wrapped, 0)
            .into_iter()
            .any(|op| matches!(op, Operator::RefFunc { function_index } if function_index == 10));
        assert!(has_remapped, "ref.func function index must be remapped to 10");
    }

    #[test]
    fn reencodes_global_get_and_set_indices() {
        // The output global space is rebuilt — main's globals first, each
        // contributing external's appended — so both accessors must carry the
        // remapped index. Two distinct source indices are used, and each is
        // checked against its own image, so an arm that remapped the operand of
        // one accessor onto the other's would not pass.
        let module = wat::parse_str(
            r#"
            (module
              (type (;0;) (func))
              (global (;0;) (mut i32) (i32.const 0))
              (global (;1;) (mut i32) (i32.const 0))
              (func (;0;) (type 0)
                global.get 0
                global.set 1)
              (export "f" (func 0)))
            "#,
        )
        .unwrap();
        let body = body_bytes(&module, 0);

        let map = shifting_map();
        let out = reencode_body(&body, &map, BodyOrigin::External)
            .expect("re-encode the global accessors");
        let wrapped = wrap(&out);

        let indices: Vec<(&str, u32)> = operators(&wrapped, 0)
            .into_iter()
            .filter_map(|op| match op {
                Operator::GlobalGet { global_index } => Some(("get", global_index)),
                Operator::GlobalSet { global_index } => Some(("set", global_index)),
                _ => None,
            })
            .collect();
        assert_eq!(
            indices,
            vec![("get", 1000), ("set", 1001)],
            "both global accessors must carry their own remapped index"
        );
    }

    #[test]
    fn unmapped_global_index_surfaces_a_clean_error() {
        // The fail-closed half of the global remap. The merge leaves the mapping
        // empty for an external whose closure was admitted as touching no global,
        // so a body that names one anyway must surface a `LinkError` — never fall
        // back to an index that would resolve against the main module's state.
        let module = wat::parse_str(
            r#"
            (module
              (type (;0;) (func))
              (global (;0;) (mut i32) (i32.const 0))
              (func (;0;) (type 0)
                global.get 0
                drop)
              (export "f" (func 0)))
            "#,
        )
        .unwrap();
        let body = body_bytes(&module, 0);

        let func = |f: u32| f;
        let ty = |t: u32| Ok(t);
        let global = |idx: u32| {
            Err::<u32, LinkError>(LinkError::UnsupportedConstruct(format!(
                "unmapped global {idx}"
            )))
        };
        let map = IndexMap { func: &func, ty: &ty, global: &global };
        let err = reencode_body(&body, &map, BodyOrigin::External)
            .expect_err("an unmapped global must error");
        assert!(
            matches!(&err, LinkError::UnsupportedConstruct(msg) if msg.contains("unmapped global")),
            "expected the map's own error to propagate, got {err:?}"
        );
    }

    #[test]
    fn tail_call_indirect_is_rejected_not_reencoded() {
        // `return_call_indirect` is a tail call: the Rocq translator has no
        // lowering for it, and Inference codegen never emits it. The re-encoder
        // must not have a dedicated arm that re-indexes and copies it; it falls
        // through to the fail-closed allow-list and is rejected. This closes the
        // main-module bypass that previously copied a tail call verbatim.
        let module = wat::parse_str(
            r#"
            (module
              (type (;0;) (func))
              (table (;0;) 1 funcref)
              (func (;0;) (type 0)
                i32.const 0
                return_call_indirect (type 0))
              (export "f" (func 0)))
            "#,
        );
        // return_call_indirect needs the tail-call feature in `wat`; skip if the
        // fixture cannot be assembled in this build.
        let Ok(module) = module else { return };
        let body = body_bytes(&module, 0);

        let map = shifting_map();
        let err = reencode_body(&body, &map, BodyOrigin::External)
            .expect_err("return_call_indirect must be rejected");
        assert!(
            matches!(err, LinkError::UnsupportedConstruct(_)),
            "expected UnsupportedConstruct, got {err:?}"
        );
    }

    #[test]
    fn tail_call_is_rejected_not_reencoded() {
        // `return_call` likewise has no re-encoder arm: it falls through to the
        // allow-list and is rejected, on both the external and the main re-encode
        // path. This is the direct-call counterpart to the bypass closure above.
        let module = wat::parse_str(
            r#"
            (module
              (type (;0;) (func (param i32) (result i32)))
              (func (;0;) (type 0) (param i32) (result i32)
                local.get 0
                return_call 1)
              (func (;1;) (type 0) (param i32) (result i32)
                local.get 0)
              (export "f" (func 0)))
            "#,
        );
        let Ok(module) = module else { return };
        let body = body_bytes(&module, 0);

        let map = shifting_map();
        for origin in [BodyOrigin::External, BodyOrigin::Main] {
            let err = reencode_body(&body, &map, origin).expect_err("return_call must be rejected");
            assert!(
                matches!(err, LinkError::UnsupportedConstruct(_)),
                "{origin:?}: expected UnsupportedConstruct, got {err:?}"
            );
        }
    }

    #[test]
    fn reencodes_function_typed_block() {
        // A block whose type is a function type (multi-value form) must have its
        // type index remapped, not copied stale.
        let module = wat::parse_str(
            r#"
            (module
              (type (;0;) (func))
              (type (;1;) (func (param i32) (result i32)))
              (func (;0;) (type 0)
                i32.const 7
                (block (type 1) (param i32) (result i32))
                drop)
              (export "f" (func 0)))
            "#,
        );
        let Ok(module) = module else { return };
        let body = body_bytes(&module, 0);

        let map = shifting_map();
        let out = reencode_body(&body, &map, BodyOrigin::External)
            .expect("re-encode function-typed block");
        let wrapped = wrap(&out);

        let has_remapped = operators(&wrapped, 0).into_iter().any(|op| {
            matches!(
                op,
                Operator::Block {
                    blockty: inf_wasmparser::BlockType::FuncType(t)
                } if t == 101
            )
        });
        assert!(has_remapped, "function-typed block index must be remapped to 101");
    }

    #[test]
    fn preserves_empty_and_value_block_types() {
        // The non-index block forms (empty + value result) must round-trip
        // unchanged through the re-encoder.
        let module = wat::parse_str(
            r#"
            (module
              (type (;0;) (func))
              (func (;0;) (type 0)
                (block)
                (block (result i32) i32.const 1) drop)
              (export "f" (func 0)))
            "#,
        )
        .unwrap();
        let body = body_bytes(&module, 0);

        let map = shifting_map();
        let out = reencode_body(&body, &map, BodyOrigin::External).expect("re-encode plain blocks");
        let wrapped = wrap(&out);

        let ops = operators(&wrapped, 0);
        assert!(
            ops.iter()
                .any(|op| matches!(op, Operator::Block { blockty: inf_wasmparser::BlockType::Empty })),
            "an empty block must round-trip"
        );
        assert!(
            ops.iter().any(|op| matches!(
                op,
                Operator::Block { blockty: inf_wasmparser::BlockType::Type(ValType::I32) }
            )),
            "an i32-result block must round-trip"
        );
    }

    #[test]
    fn reference_typed_local_is_unsupported() {
        // A body declaring a `funcref` local cannot be re-encoded: the static
        // merge models no reference types. `read_locals` must surface the error.
        let module = wat::parse_str(
            r#"
            (module
              (type (;0;) (func))
              (func (;0;) (type 0)
                (local funcref))
              (export "f" (func 0)))
            "#,
        )
        .unwrap();
        let body = body_bytes(&module, 0);

        let map = shifting_map();
        let err = reencode_body(&body, &map, BodyOrigin::External)
            .expect_err("ref-typed local must be rejected");
        assert!(
            matches!(err, LinkError::UnsupportedConstruct(_)),
            "expected UnsupportedConstruct, got {err:?}"
        );
    }

    #[test]
    fn unmapped_block_type_index_surfaces_a_clean_error() {
        // A function-typed block whose type index has no mapping must propagate
        // the `ty` closure's error through re-encoding, not panic. This models
        // the merge feeding a body whose block type was never interned.
        let module = wat::parse_str(
            r#"
            (module
              (type (;0;) (func))
              (type (;1;) (func (param i32) (result i32)))
              (func (;0;) (type 0)
                i32.const 7
                (block (type 1) (param i32) (result i32))
                drop)
              (export "f" (func 0)))
            "#,
        );
        let Ok(module) = module else { return };
        let body = body_bytes(&module, 0);

        let func = |f: u32| f;
        let ty = |idx: u32| {
            Err::<u32, LinkError>(LinkError::UnsupportedConstruct(format!(
                "unmapped type {idx}"
            )))
        };
        let global = |idx: u32| Ok(idx);
        let map = IndexMap { func: &func, ty: &ty, global: &global };
        let err = reencode_body(&body, &map, BodyOrigin::External)
            .expect_err("unmapped block type must error");
        assert!(
            matches!(err, LinkError::UnsupportedConstruct(_)),
            "expected UnsupportedConstruct, got {err:?}"
        );
    }

    #[test]
    fn reencodes_supported_value_type_locals() {
        // Locals of every *supported* value type (the integer types) must map onto
        // the encoder equivalents, covering those arms of `map_val_type`. The float
        // and `v128` locals are exercised separately below: they are rejected,
        // since the Inference language has no `f32`/`f64` or SIMD types.
        let module = wat::parse_str(
            r#"
            (module
              (type (;0;) (func))
              (func (;0;) (type 0)
                (local i32 i64))
              (export "f" (func 0)))
            "#,
        )
        .unwrap();
        let body = body_bytes(&module, 0);

        let map = shifting_map();
        let out = reencode_body(&body, &map, BodyOrigin::External)
            .expect("re-encode supported value-type locals");
        let wrapped = wrap(&out);

        let locals: Vec<_> = {
            let mut idx = 0;
            let mut found = Vec::new();
            for payload in Parser::new(0).parse_all(&wrapped) {
                if let Payload::CodeSectionEntry(b) = payload.unwrap() {
                    if idx == 0 {
                        for e in b.get_locals_reader().unwrap() {
                            found.push(e.unwrap());
                        }
                    }
                    idx += 1;
                }
            }
            found
        };
        let types: Vec<ValType> = locals.iter().map(|(_, t)| *t).collect();
        assert_eq!(
            types,
            vec![ValType::I32, ValType::I64],
            "every supported value-type local must survive re-encoding"
        );
    }

    #[test]
    fn v128_local_is_rejected() {
        // A `v128` local cannot be re-encoded: the Inference language has no SIMD
        // types and every SIMD operator is rejected, so the value-type chokepoint
        // must reject the SIMD type too. This is the value-type backstop on the
        // main-module path that bypasses the feature gate.
        let module = wat::parse_str(
            r#"
            (module
              (type (;0;) (func))
              (func (;0;) (type 0)
                (local v128))
              (export "f" (func 0)))
            "#,
        )
        .unwrap();
        let body = body_bytes(&module, 0);

        let map = shifting_map();
        let err = reencode_body(&body, &map, BodyOrigin::External)
            .expect_err("v128 local must be rejected");
        assert!(
            matches!(&err, LinkError::UnsupportedConstruct(msg) if msg.contains("v128")),
            "expected a v128 UnsupportedConstruct, got {err:?}"
        );
    }

    #[test]
    fn float_local_is_rejected() {
        // An `f32` or `f64` local cannot be re-encoded: the Inference language has
        // no `f32`/`f64` types, so the value-type chokepoint rejects it. This is
        // the value-type backstop on the main-module path that bypasses the
        // feature gate; the operator-stream backstop is `safety::is_float`.
        for ty in ["f32", "f64"] {
            let module = wat::parse_str(format!(
                r#"
                (module
                  (type (;0;) (func))
                  (func (;0;) (type 0)
                    (local {ty}))
                  (export "f" (func 0)))
                "#,
            ))
            .unwrap();
            let body = body_bytes(&module, 0);

            let map = shifting_map();
            let err = reencode_body(&body, &map, BodyOrigin::External)
                .expect_err("float local must be rejected");
            assert!(
                matches!(&err, LinkError::UnsupportedConstruct(msg) if msg.contains("floating-point")),
                "{ty}: expected a floating-point UnsupportedConstruct, got {err:?}"
            );
        }
    }

    /// Hand-encodes a single-function body whose only operator is one of the
    /// Inference non-det blocks (`forall`/`exists`/`assume`/`unique`) carrying a
    /// `FuncType(type_idx)` block type. The `wat` crate cannot assemble the
    /// custom `0xfc`-prefixed opcodes, so the body is built byte-by-byte: an
    /// empty locals vector, the non-det opcode with a single-byte positive `s33`
    /// type index, the block-closing `end`, and the function-closing `end`.
    fn nondet_block_body(sub_opcode: u8, type_idx: u8) -> Vec<u8> {
        assert!(type_idx < 0x40, "type index must be a single positive s33 byte");
        vec![
            0x00, // zero locals
            0xfc,
            sub_opcode,
            type_idx, // s33-encoded function block-type index
            0x0b,     // end (closes the non-det block)
            0x0b,     // end (closes the function)
        ]
    }

    /// The `(sub_opcode)` for each non-det block operator, matching the codegen
    /// and `inf-wasmparser` decoder.
    const NONDET_OPS: &[(u8, &str)] = &[
        (0x3a, "forall"),
        (0x3b, "exists"),
        (0x3c, "assume"),
        (0x3d, "unique"),
    ];

    /// Hand-encodes a single-function body whose only operator is an uzumaki
    /// rvalue (`i32.uzumaki` = `0xfc 0x31`, `i64.uzumaki` = `0xfc 0x32`), which
    /// pushes a value and is immediately dropped to keep the stack balanced.
    fn uzumaki_body(sub_opcode: u8) -> Vec<u8> {
        vec![
            0x00, // zero locals
            0xfc, sub_opcode, // i32/i64.uzumaki
            0x1a, // drop
            0x0b, // end (closes the function)
        ]
    }

    /// The `(sub_opcode)` for each uzumaki rvalue.
    const UZUMAKI_OPS: &[(u8, &str)] = &[(0x31, "i32.uzumaki"), (0x32, "i64.uzumaki")];

    #[test]
    fn external_nondet_block_is_rejected_as_non_executable() {
        // H-2 (corrected): a forall/exists/assume/unique block is verification-
        // only and has no executable semantics, so an *external* body merged into
        // the output must reject it — never remap and copy it. Both the empty
        // (codegen) form and the function-typed form reject identically.
        for &(sub_opcode, name) in NONDET_OPS {
            for body in [
                nondet_block_body(sub_opcode, 1),
                vec![0x00, 0xfc, sub_opcode, 0x40, 0x0b, 0x0b],
            ] {
                let map = shifting_map();
                let err = reencode_body(&body, &map, BodyOrigin::External)
                    .err()
                    .unwrap_or_else(|| panic!("external {name} block must be rejected"));
                assert!(
                    matches!(err, LinkError::UnsupportedConstruct(_)),
                    "{name}: expected UnsupportedConstruct, got {err:?}"
                );
            }
        }
    }

    #[test]
    fn external_uzumaki_is_rejected_as_non_executable() {
        // H-2 (corrected): the uzumaki rvalues are verification-only and have no
        // executable semantics; an external body merged into the output must
        // reject them rather than copy them verbatim.
        for &(sub_opcode, name) in UZUMAKI_OPS {
            let body = uzumaki_body(sub_opcode);
            let map = shifting_map();
            let err = reencode_body(&body, &map, BodyOrigin::External)
                .err()
                .unwrap_or_else(|| panic!("external {name} must be rejected"));
            assert!(
                matches!(err, LinkError::UnsupportedConstruct(_)),
                "{name}: expected UnsupportedConstruct, got {err:?}"
            );
        }
    }

    #[test]
    fn main_nondet_block_passes_through_as_proof_scaffolding() {
        // The main module in proof mode legitimately carries non-det blocks as
        // Rocq scaffolding. They must pass through the main re-encode path: the
        // empty (codegen) form round-trips unchanged, and a function-typed form
        // has only its block-type index remapped — never rejected.
        for &(sub_opcode, name) in NONDET_OPS {
            // Empty form round-trips unchanged.
            let empty_body = vec![0x00, 0xfc, sub_opcode, 0x40, 0x0b, 0x0b];
            let map = shifting_map();
            let out = reencode_body(&empty_body, &map, BodyOrigin::Main)
                .unwrap_or_else(|e| panic!("main empty {name} block: {e:?}"));
            let wrapped = wrap(&out);
            let empty = operators(&wrapped, 0).into_iter().any(|op| {
                let blockty = match op {
                    Operator::Forall { blockty }
                    | Operator::Exists { blockty }
                    | Operator::Assume { blockty }
                    | Operator::Unique { blockty } => Some(blockty),
                    _ => None,
                };
                matches!(blockty, Some(inf_wasmparser::BlockType::Empty))
            });
            assert!(empty, "main empty {name} block must round-trip unchanged");

            // Function-typed form has its block-type index remapped (+100).
            let functype_body = nondet_block_body(sub_opcode, 1);
            let map = shifting_map();
            let out = reencode_body(&functype_body, &map, BodyOrigin::Main)
                .unwrap_or_else(|e| panic!("main function-typed {name} block: {e:?}"));
            let wrapped = wrap(&out);
            let remapped = operators(&wrapped, 0).into_iter().any(|op| {
                let blockty = match op {
                    Operator::Forall { blockty }
                    | Operator::Exists { blockty }
                    | Operator::Assume { blockty }
                    | Operator::Unique { blockty } => Some(blockty),
                    _ => None,
                };
                matches!(blockty, Some(inf_wasmparser::BlockType::FuncType(t)) if t == 101)
            });
            assert!(
                remapped,
                "main function-typed {name} block index must remap to 101"
            );
        }
    }

    #[test]
    fn main_uzumaki_passes_through_verbatim() {
        // The main module's uzumaki rvalues are proof scaffolding: they must be
        // copied through the main re-encode path verbatim, never rejected.
        for &(sub_opcode, name) in UZUMAKI_OPS {
            let body = uzumaki_body(sub_opcode);
            let map = shifting_map();
            let out = reencode_body(&body, &map, BodyOrigin::Main)
                .unwrap_or_else(|e| panic!("main {name}: {e:?}"));
            let wrapped = wrap(&out);
            let survives = operators(&wrapped, 0)
                .into_iter()
                .any(|op| matches!(op, Operator::I32Uzumaki { .. } | Operator::I64Uzumaki { .. }));
            assert!(survives, "main {name} must survive re-encoding verbatim");
        }
    }
}

