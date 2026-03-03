//! Memory infrastructure for stack-allocated compound types (arrays, structs).
//!
//! This module provides the data structures and helpers for managing linear memory
//! in the WebAssembly codegen pipeline. Arrays are stored in linear memory using a
//! shadow stack with a `__stack_pointer` global that grows downward from the top of
//! the first memory page.
//!
//! # Memory Layout
//!
//! ```text
//! Stack-first layout (1 page = 64KB, no data sections yet)
//! +--------------------------------------------+  0x10000 (64KB)
//! |              (free space)                   |
//! |     (future: data sections, heap)           |
//! +-- __stack_pointer --------------------------+  STACK_SIZE
//! |                                             |
//! |         Stack (grows downward)              |
//! |                                             |
//! +--------------------------------------------+  0x00000
//! overflow below 0 = WASM OOB trap
//! ```
//!
//! # Responsibilities
//!
//! | Concern                 | Location                            |
//! |-------------------------|-------------------------------------|
//! | WASM local registration | `pre_scan_locals()` in compiler.rs  |
//! | Frame layout computation| `compute_frame_layout()` here       |
//! | Load/store helpers      | `store_instruction()` / `load_instruction()` here |
//! | Prologue/epilogue       | `emit_stack_prologue()` / `emit_stack_epilogue()` here |
//! | Section assembly        | `finish()` in compiler.rs           |

use inference_type_checker::type_info::{NumberType, TypeInfoKind};
use rustc_hash::FxHashMap;
use wasm_encoder::{Function, Instruction, MemArg};

/// One WASM memory page in bytes.
pub(crate) const PAGE_SIZE: u32 = 65536;

/// Size of the stack region in bytes.
///
/// In the stack-first layout, the stack occupies addresses `[0, STACK_SIZE)` and grows
/// downward from `STACK_SIZE` toward 0. Overflow below address 0 traps automatically
/// via WASM out-of-bounds memory access — specifically, the `memory.fill` in the
/// prologue fails its bounds check when the wrapped SP is used as a destination address.
///
/// Must not exceed `PAGE_SIZE`. When data sections are added (constant arrays, strings),
/// reduce this to leave room above the stack region: `STACK_SIZE + data_size <= PAGE_SIZE`.
pub(crate) const STACK_SIZE: u32 = PAGE_SIZE;

/// Initial value for `__stack_pointer`: one past the last valid stack address.
///
/// This is a "past-the-end" value (like C++ `vector::end()`). Address `STACK_SIZE`
/// itself is never accessed — the prologue subtracts `frame_size` before any memory
/// operation, so the first actual access is at `STACK_SIZE - frame_size`.
#[allow(clippy::cast_possible_wrap)]
pub(crate) const STACK_POINTER_INIT: i32 = STACK_SIZE as i32;

/// Stack frame alignment in bytes (matches LLVM/Rust WASM convention).
pub(crate) const FRAME_ALIGNMENT: u32 = 16;

/// WASM global index for `__stack_pointer` (the only global in the module).
const STACK_POINTER_GLOBAL: u32 = 0;

/// WASM memory index (only one linear memory per module).
const MEMORY_INDEX: u32 = 0;

/// Describes a single array's location within a stack frame.
#[derive(Debug, Clone)]
pub(crate) struct ArraySlot {
    /// Byte offset from the frame pointer to the start of this array.
    pub offset: u32,
    /// Size in bytes of each element.
    pub elem_size: u32,
    /// Number of elements in the array.
    pub length: u32,
}

/// Per-function stack frame layout for array allocations.
///
/// Stored on `Compiler` as per-function state: set at the start of
/// `visit_function_definition` and cleared after the function body is compiled.
#[derive(Debug)]
pub(crate) struct FrameLayout {
    /// Total frame size in bytes, rounded up to [`FRAME_ALIGNMENT`].
    pub total_size: u32,
    /// Maps source-level array variable names to their frame slots.
    pub array_offsets: FxHashMap<String, ArraySlot>,
    /// WASM local index of the synthetic `__frame_ptr` local.
    pub frame_ptr_local: u32,
}

/// Returns the byte size of a single element for the given type.
///
/// Used by `compute_frame_layout` and store/load instruction selection.
#[must_use = "returns element size in bytes"]
pub(crate) fn element_size(kind: &TypeInfoKind) -> u32 {
    match kind {
        TypeInfoKind::Bool | TypeInfoKind::Number(NumberType::I8 | NumberType::U8) => 1,
        TypeInfoKind::Number(NumberType::I16 | NumberType::U16) => 2,
        TypeInfoKind::Number(NumberType::I32 | NumberType::U32) => 4,
        TypeInfoKind::Number(NumberType::I64 | NumberType::U64) => 8,
        // The type checker restricts array element types to: bool, i8, u8, i16, u16,
        // i32, u32, i64, u64. This arm is unreachable for valid programs. When
        // struct/string array elements are supported, this will need to be extended.
        _ => todo!("Unsupported array element type: {kind:?}"),
    }
}

/// Rounds `offset` up to the nearest multiple of `alignment`.
///
/// Used to align array offsets within a frame to their element type's
/// natural alignment, matching LLVM/Rust convention.
#[must_use = "returns the aligned offset"]
pub(crate) fn align_to(offset: u32, alignment: u32) -> u32 {
    debug_assert!(
        alignment.is_power_of_two(),
        "alignment must be a power of two"
    );
    (offset + alignment - 1) & !(alignment - 1)
}

/// Rounds `size` up to the nearest multiple of [`FRAME_ALIGNMENT`].
#[must_use = "returns the aligned size"]
pub(crate) fn align_to_frame(size: u32) -> u32 {
    align_to(size, FRAME_ALIGNMENT)
}

/// Selects the appropriate WASM store instruction based on an `ArraySlot`'s element size.
///
/// This avoids needing a `TypeInfoKind` at the call site (useful when the array
/// literal node may not have type info, but the slot was pre-computed from the
/// variable definition).
#[must_use = "returns the WASM store instruction"]
pub(crate) fn store_instruction_from_slot(slot: &ArraySlot) -> Instruction<'static> {
    store_instruction_for_size(slot.elem_size)
}

/// Selects the appropriate WASM store instruction for an element byte size.
fn store_instruction_for_size(size: u32) -> Instruction<'static> {
    let align = match size {
        1 => 0,
        2 => 1,
        4 => 2,
        8 => 3,
        _ => unreachable!("Invalid element size: {size}"),
    };
    let memarg = MemArg {
        offset: 0,
        align,
        memory_index: MEMORY_INDEX,
    };
    match size {
        1 => Instruction::I32Store8(memarg),
        2 => Instruction::I32Store16(memarg),
        4 => Instruction::I32Store(memarg),
        8 => Instruction::I64Store(memarg),
        _ => unreachable!("Invalid element size: {size}"),
    }
}

/// Selects the appropriate WASM store instruction for an element type.
///
/// The `MemArg` uses offset 0 and the natural alignment for the element size:
/// - 1 byte: align=0 (2^0 = 1)
/// - 2 bytes: align=1 (2^1 = 2)
/// - 4 bytes: align=2 (2^2 = 4)
/// - 8 bytes: align=3 (2^3 = 8)
#[must_use = "returns the WASM store instruction"]
pub(crate) fn store_instruction(elem_type: &TypeInfoKind) -> Instruction<'static> {
    let memarg = MemArg {
        offset: 0,
        align: natural_alignment(elem_type),
        memory_index: MEMORY_INDEX,
    };
    match elem_type {
        TypeInfoKind::Bool | TypeInfoKind::Number(NumberType::I8 | NumberType::U8) => {
            Instruction::I32Store8(memarg)
        }
        TypeInfoKind::Number(NumberType::I16 | NumberType::U16) => Instruction::I32Store16(memarg),
        TypeInfoKind::Number(NumberType::I32 | NumberType::U32) => Instruction::I32Store(memarg),
        TypeInfoKind::Number(NumberType::I64 | NumberType::U64) => Instruction::I64Store(memarg),
        // The type checker restricts array element types to: bool, i8, u8, i16, u16,
        // i32, u32, i64, u64. This arm is unreachable for valid programs. When
        // struct/string array elements are supported, this will need to be extended.
        _ => todo!("Unsupported array element type for store: {elem_type:?}"),
    }
}

/// Selects the appropriate WASM load instruction for an element type.
///
/// Uses sign-appropriate extension for sub-i32 types:
/// - Signed types (`i8`, `i16`): `i32.load8_s`, `i32.load16_s` (sign-extending)
/// - Unsigned types (`u8`, `u16`, `bool`): `i32.load8_u`, `i32.load16_u` (zero-extending)
#[must_use = "returns the WASM load instruction"]
pub(crate) fn load_instruction(elem_type: &TypeInfoKind) -> Instruction<'static> {
    let memarg = MemArg {
        offset: 0,
        align: natural_alignment(elem_type),
        memory_index: MEMORY_INDEX,
    };
    match elem_type {
        TypeInfoKind::Bool | TypeInfoKind::Number(NumberType::U8) => Instruction::I32Load8U(memarg),
        TypeInfoKind::Number(NumberType::I8) => Instruction::I32Load8S(memarg),
        TypeInfoKind::Number(NumberType::U16) => Instruction::I32Load16U(memarg),
        TypeInfoKind::Number(NumberType::I16) => Instruction::I32Load16S(memarg),
        TypeInfoKind::Number(NumberType::I32 | NumberType::U32) => Instruction::I32Load(memarg),
        TypeInfoKind::Number(NumberType::I64 | NumberType::U64) => Instruction::I64Load(memarg),
        // The type checker restricts array element types to: bool, i8, u8, i16, u16,
        // i32, u32, i64, u64. This arm is unreachable for valid programs. When
        // struct/string array elements are supported, this will need to be extended.
        _ => todo!("Unsupported array element type for load: {elem_type:?}"),
    }
}

/// Emits sub-i32 narrowing instructions to truncate an i32 value to the
/// width of a sub-i32 type after arithmetic operations.
///
/// - Signed (i8, i16): shift-left then arithmetic-shift-right (sign-extend)
/// - Unsigned (u8, u16): AND with bitmask (zero-extend)
/// - i32/u32/i64/u64/bool: no-op, returns false
///
/// Returns `true` if narrowing instructions were emitted.
pub(crate) fn emit_sub_i32_narrowing(func: &mut Function, kind: &TypeInfoKind) -> bool {
    match kind {
        TypeInfoKind::Number(NumberType::I8) => {
            // Sign-extend from 8 bits: (x << 24) >>s 24
            func.instruction(&Instruction::I32Const(24));
            func.instruction(&Instruction::I32Shl);
            func.instruction(&Instruction::I32Const(24));
            func.instruction(&Instruction::I32ShrS);
            true
        }
        TypeInfoKind::Number(NumberType::I16) => {
            // Sign-extend from 16 bits: (x << 16) >>s 16
            func.instruction(&Instruction::I32Const(16));
            func.instruction(&Instruction::I32Shl);
            func.instruction(&Instruction::I32Const(16));
            func.instruction(&Instruction::I32ShrS);
            true
        }
        TypeInfoKind::Number(NumberType::U8) => {
            // Zero-extend from 8 bits: x & 0xFF
            func.instruction(&Instruction::I32Const(0xFF));
            func.instruction(&Instruction::I32And);
            true
        }
        TypeInfoKind::Number(NumberType::U16) => {
            // Zero-extend from 16 bits: x & 0xFFFF
            func.instruction(&Instruction::I32Const(0xFFFF));
            func.instruction(&Instruction::I32And);
            true
        }
        _ => false,
    }
}

/// Returns the natural alignment exponent for an element type.
///
/// WASM encodes alignment as `log2(byte_alignment)`:
/// - 1-byte types: 0 (2^0 = 1)
/// - 2-byte types: 1 (2^1 = 2)
/// - 4-byte types: 2 (2^2 = 4)
/// - 8-byte types: 3 (2^3 = 8)
fn natural_alignment(elem_type: &TypeInfoKind) -> u32 {
    match element_size(elem_type) {
        1 => 0,
        2 => 1,
        4 => 2,
        8 => 3,
        _ => unreachable!("element_size returns only 1, 2, 4, or 8"),
    }
}

/// Emits the stack prologue for a function with stack-allocated arrays.
///
/// The prologue decrements `__stack_pointer`, saves the frame pointer, and
/// zero-initializes the entire frame via `memory.fill`.
///
/// # Stack overflow protection
///
/// `i32.sub` uses modular arithmetic and never traps. If the subtraction wraps
/// (SP goes "below 0"), the result is a large unsigned value (e.g., `0xFFFFFFF0`).
/// The subsequent `memory.fill` uses this wrapped value as the destination address,
/// which fails the WASM bounds check (`addr + size > mem_size`) and traps. This is
/// the mechanism behind the stack-first "free trap" — the computed (possibly wrapped)
/// stack pointer value must be used as the destination for `memory.fill` without being
/// modified or bounds-checked first for this protection to hold.
///
/// **Optimization opportunity**: When all array elements are explicitly initialized
/// (e.g., `let arr: [i32; 3] = [1, 2, 3]`), the `memory.fill` is redundant since
/// every byte will be overwritten. This is intentionally not optimized to ensure
/// deterministic behavior for partially-initialized arrays and to simplify the
/// implementation. A future optimization pass could skip `memory.fill` when all
/// arrays in the frame are fully initialized — but must preserve the overflow trap
/// by emitting an explicit guard (`if SP < 0 then unreachable`).
///
/// ```text
/// global.get $__stack_pointer
/// i32.const <frame_size>
/// i32.sub
/// local.tee $__frame_ptr
/// global.set $__stack_pointer
/// local.get $__frame_ptr
/// i32.const 0
/// i32.const <frame_size>
/// memory.fill
/// ```
pub(crate) fn emit_stack_prologue(func: &mut Function, layout: &FrameLayout) {
    assert!(
        layout.total_size > 0,
        "emit_stack_prologue called with zero-size frame; memory.fill would trap per WASM spec"
    );
    cov_mark::hit!(wasm_codegen_emit_stack_prologue);
    #[allow(clippy::cast_possible_wrap)]
    let frame_size = layout.total_size as i32;
    func.instruction(&Instruction::GlobalGet(STACK_POINTER_GLOBAL));
    func.instruction(&Instruction::I32Const(frame_size));
    func.instruction(&Instruction::I32Sub);
    func.instruction(&Instruction::LocalTee(layout.frame_ptr_local));
    func.instruction(&Instruction::GlobalSet(STACK_POINTER_GLOBAL));
    func.instruction(&Instruction::LocalGet(layout.frame_ptr_local));
    func.instruction(&Instruction::I32Const(0));
    func.instruction(&Instruction::I32Const(frame_size));
    func.instruction(&Instruction::MemoryFill(MEMORY_INDEX));
}

/// Threshold: arrays with more than this many elements use `memory.copy`
/// instead of unrolled element-by-element copying.
const UNROLL_THRESHOLD: u32 = 16;

/// Emits copy-on-entry code for one array-typed parameter.
///
/// Copies `slot.length` elements from the caller's pointer (`param_local`) into
/// the callee's frame at `slot.offset` from `__frame_ptr`, then updates
/// `param_local` to point to the callee's copy.
///
/// For arrays with N <= 16 elements, the copy is unrolled element by element.
/// For larger arrays (N > 16), a single `memory.copy` instruction is used.
///
/// After the copy, the parameter local is overwritten with the callee's frame
/// address so that subsequent reads/writes inside the function operate on the
/// local copy (value semantics).
///
/// ```text
/// ;; unrolled copy (N <= 16):
/// local.get $__frame_ptr
/// i32.const <offset + i * elem_size>
/// i32.add
/// local.get $param_ptr
/// i32.const <i * elem_size>
/// i32.add
/// i32.load / i32.load8_s / ...     ;; load source element
/// i32.store / i32.store8 / ...     ;; store to destination
/// ;; ... repeat for each element
///
/// ;; bulk copy (N > 16):
/// local.get $__frame_ptr
/// i32.const <offset>
/// i32.add                          ;; destination
/// local.get $param_ptr             ;; source
/// i32.const <byte_size>
/// memory.copy
///
/// ;; update param local:
/// local.get $__frame_ptr
/// i32.const <offset>
/// i32.add
/// local.set $param_ptr
/// ```
pub(crate) fn emit_array_param_copy(
    func: &mut Function,
    layout: &FrameLayout,
    slot: &ArraySlot,
    param_local: u32,
    elem_type: &TypeInfoKind,
) {
    cov_mark::hit!(wasm_codegen_emit_array_param_copy);

    let byte_size = slot.elem_size * slot.length;

    if slot.length > UNROLL_THRESHOLD {
        // Bulk copy via memory.copy
        func.instruction(&Instruction::LocalGet(layout.frame_ptr_local));
        if slot.offset > 0 {
            #[allow(clippy::cast_possible_wrap)]
            func.instruction(&Instruction::I32Const(slot.offset as i32));
            func.instruction(&Instruction::I32Add);
        }
        func.instruction(&Instruction::LocalGet(param_local));
        #[allow(clippy::cast_possible_wrap)]
        func.instruction(&Instruction::I32Const(byte_size as i32));
        func.instruction(&Instruction::MemoryCopy {
            src_mem: MEMORY_INDEX,
            dst_mem: MEMORY_INDEX,
        });
    } else {
        // Unrolled element-by-element copy
        let load_instr = load_instruction(elem_type);
        let store_instr = store_instruction(elem_type);
        for i in 0..slot.length {
            #[allow(clippy::cast_possible_wrap)]
            let byte_offset = (slot.offset + i * slot.elem_size) as i32;
            #[allow(clippy::cast_possible_wrap)]
            let src_offset = (i * slot.elem_size) as i32;

            // destination address
            func.instruction(&Instruction::LocalGet(layout.frame_ptr_local));
            func.instruction(&Instruction::I32Const(byte_offset));
            func.instruction(&Instruction::I32Add);

            // load from source (param pointer + element offset)
            func.instruction(&Instruction::LocalGet(param_local));
            if src_offset > 0 {
                func.instruction(&Instruction::I32Const(src_offset));
                func.instruction(&Instruction::I32Add);
            }
            func.instruction(&load_instr);

            // store to destination
            func.instruction(&store_instr);
        }
    }

    // Update the parameter local to point to the callee's copy
    func.instruction(&Instruction::LocalGet(layout.frame_ptr_local));
    if slot.offset > 0 {
        #[allow(clippy::cast_possible_wrap)]
        func.instruction(&Instruction::I32Const(slot.offset as i32));
        func.instruction(&Instruction::I32Add);
    }
    func.instruction(&Instruction::LocalSet(param_local));
}

/// Emits a `memory.copy` from a source pointer to the sret destination.
///
/// Used in `return arr` inside an sret function: copies the array data from
/// the callee's frame slot to the caller-provided sret pointer.
///
/// ```text
/// local.get $sret
/// local.get $source
/// i32.const <byte_size>
/// memory.copy
/// ```
pub(crate) fn emit_sret_copy(
    func: &mut Function,
    sret_local: u32,
    source_local: u32,
    byte_size: u32,
) {
    func.instruction(&Instruction::LocalGet(sret_local));
    func.instruction(&Instruction::LocalGet(source_local));
    #[allow(clippy::cast_possible_wrap)]
    func.instruction(&Instruction::I32Const(byte_size as i32));
    func.instruction(&Instruction::MemoryCopy {
        src_mem: MEMORY_INDEX,
        dst_mem: MEMORY_INDEX,
    });
}

/// Emits the address computation `sret + byte_offset` onto the WASM stack.
///
/// Used when writing individual elements to the sret destination (e.g., for
/// `return [1, 2, 3]` in an sret function).
///
/// ```text
/// local.get $sret
/// i32.const <byte_offset>   ;; omitted when offset is 0
/// i32.add                   ;; omitted when offset is 0
/// ```
pub(crate) fn emit_sret_element_addr(func: &mut Function, sret_local: u32, byte_offset: u32) {
    func.instruction(&Instruction::LocalGet(sret_local));
    if byte_offset > 0 {
        #[allow(clippy::cast_possible_wrap)]
        func.instruction(&Instruction::I32Const(byte_offset as i32));
        func.instruction(&Instruction::I32Add);
    }
}

/// Emits the stack epilogue to restore `__stack_pointer` before exiting.
///
/// ```text
/// local.get $__frame_ptr
/// i32.const <frame_size>
/// i32.add
/// global.set $__stack_pointer
/// ```
pub(crate) fn emit_stack_epilogue(func: &mut Function, layout: &FrameLayout) {
    cov_mark::hit!(wasm_codegen_emit_stack_epilogue);
    #[allow(clippy::cast_possible_wrap)]
    let frame_size = layout.total_size as i32;
    func.instruction(&Instruction::LocalGet(layout.frame_ptr_local));
    func.instruction(&Instruction::I32Const(frame_size));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::GlobalSet(STACK_POINTER_GLOBAL));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_to_frame_zero() {
        assert_eq!(align_to_frame(0), 0);
    }

    #[test]
    fn align_to_frame_exact_multiple() {
        assert_eq!(align_to_frame(16), 16);
        assert_eq!(align_to_frame(32), 32);
    }

    #[test]
    fn align_to_frame_rounds_up() {
        assert_eq!(align_to_frame(1), 16);
        assert_eq!(align_to_frame(12), 16);
        assert_eq!(align_to_frame(17), 32);
        assert_eq!(align_to_frame(33), 48);
    }

    #[test]
    fn stack_pointer_init_equals_stack_size() {
        assert_eq!(STACK_SIZE, 65536);
        assert_eq!(STACK_POINTER_INIT, STACK_SIZE.cast_signed());
    }

    #[test]
    fn stack_size_fits_in_one_page() {
        const _: () = assert!(STACK_SIZE <= PAGE_SIZE);
    }

    #[test]
    fn stack_pointer_init_fits_in_i32() {
        assert!(
            i32::try_from(STACK_SIZE).is_ok(),
            "STACK_SIZE must fit in i32 for STACK_POINTER_INIT cast"
        );
    }

    #[test]
    fn frame_alignment_is_sixteen() {
        assert_eq!(FRAME_ALIGNMENT, 16);
    }

    #[test]
    fn array_slot_fields_accessible() {
        let slot = ArraySlot {
            offset: 0,
            elem_size: 4,
            length: 3,
        };
        assert_eq!(slot.offset, 0);
        assert_eq!(slot.elem_size, 4);
        assert_eq!(slot.length, 3);
    }

    #[test]
    fn frame_layout_fields_accessible() {
        let layout = FrameLayout {
            total_size: 16,
            array_offsets: FxHashMap::default(),
            frame_ptr_local: 0,
        };
        assert_eq!(layout.total_size, 16);
        assert!(layout.array_offsets.is_empty());
        assert_eq!(layout.frame_ptr_local, 0);
    }

    #[test]
    fn element_size_bool() {
        assert_eq!(element_size(&TypeInfoKind::Bool), 1);
    }

    #[test]
    fn element_size_i8() {
        assert_eq!(element_size(&TypeInfoKind::Number(NumberType::I8)), 1);
    }

    #[test]
    fn element_size_u8() {
        assert_eq!(element_size(&TypeInfoKind::Number(NumberType::U8)), 1);
    }

    #[test]
    fn element_size_i16() {
        assert_eq!(element_size(&TypeInfoKind::Number(NumberType::I16)), 2);
    }

    #[test]
    fn element_size_i32() {
        assert_eq!(element_size(&TypeInfoKind::Number(NumberType::I32)), 4);
    }

    #[test]
    fn element_size_i64() {
        assert_eq!(element_size(&TypeInfoKind::Number(NumberType::I64)), 8);
    }

    #[test]
    fn element_size_u64() {
        assert_eq!(element_size(&TypeInfoKind::Number(NumberType::U64)), 8);
    }

    #[test]
    fn natural_alignment_1_byte() {
        assert_eq!(natural_alignment(&TypeInfoKind::Bool), 0);
        assert_eq!(natural_alignment(&TypeInfoKind::Number(NumberType::I8)), 0);
    }

    #[test]
    fn natural_alignment_2_byte() {
        assert_eq!(natural_alignment(&TypeInfoKind::Number(NumberType::I16)), 1);
    }

    #[test]
    fn natural_alignment_4_byte() {
        assert_eq!(natural_alignment(&TypeInfoKind::Number(NumberType::I32)), 2);
    }

    #[test]
    fn natural_alignment_8_byte() {
        assert_eq!(natural_alignment(&TypeInfoKind::Number(NumberType::I64)), 3);
    }

    #[test]
    fn align_to_identity() {
        assert_eq!(align_to(0, 4), 0);
        assert_eq!(align_to(4, 4), 4);
        assert_eq!(align_to(8, 8), 8);
    }

    #[test]
    fn align_to_rounds_up() {
        assert_eq!(align_to(1, 4), 4);
        assert_eq!(align_to(3, 4), 4);
        assert_eq!(align_to(5, 8), 8);
        assert_eq!(align_to(7, 8), 8);
        assert_eq!(align_to(9, 4), 12);
    }

    #[test]
    fn align_to_one_byte() {
        assert_eq!(align_to(0, 1), 0);
        assert_eq!(align_to(1, 1), 1);
        assert_eq!(align_to(7, 1), 7);
        assert_eq!(align_to(100, 1), 100);
    }

    #[test]
    fn emit_sub_i32_narrowing_i8() {
        let mut func = Function::new(vec![]);
        let emitted = emit_sub_i32_narrowing(&mut func, &TypeInfoKind::Number(NumberType::I8));
        assert!(emitted, "should emit narrowing for i8");
    }

    #[test]
    fn emit_sub_i32_narrowing_i16() {
        let mut func = Function::new(vec![]);
        let emitted = emit_sub_i32_narrowing(&mut func, &TypeInfoKind::Number(NumberType::I16));
        assert!(emitted, "should emit narrowing for i16");
    }

    #[test]
    fn emit_sub_i32_narrowing_u8() {
        let mut func = Function::new(vec![]);
        let emitted = emit_sub_i32_narrowing(&mut func, &TypeInfoKind::Number(NumberType::U8));
        assert!(emitted, "should emit narrowing for u8");
    }

    #[test]
    fn emit_sub_i32_narrowing_u16() {
        let mut func = Function::new(vec![]);
        let emitted = emit_sub_i32_narrowing(&mut func, &TypeInfoKind::Number(NumberType::U16));
        assert!(emitted, "should emit narrowing for u16");
    }

    #[test]
    fn emit_sub_i32_narrowing_i32_noop() {
        let mut func = Function::new(vec![]);
        let emitted = emit_sub_i32_narrowing(&mut func, &TypeInfoKind::Number(NumberType::I32));
        assert!(!emitted, "should not emit narrowing for i32");
    }

    #[test]
    fn emit_sub_i32_narrowing_u32_noop() {
        let mut func = Function::new(vec![]);
        let emitted = emit_sub_i32_narrowing(&mut func, &TypeInfoKind::Number(NumberType::U32));
        assert!(!emitted, "should not emit narrowing for u32");
    }

    #[test]
    fn emit_sub_i32_narrowing_i64_noop() {
        let mut func = Function::new(vec![]);
        let emitted = emit_sub_i32_narrowing(&mut func, &TypeInfoKind::Number(NumberType::I64));
        assert!(!emitted, "should not emit narrowing for i64");
    }

    #[test]
    fn emit_sub_i32_narrowing_u64_noop() {
        let mut func = Function::new(vec![]);
        let emitted = emit_sub_i32_narrowing(&mut func, &TypeInfoKind::Number(NumberType::U64));
        assert!(!emitted, "should not emit narrowing for u64");
    }

    #[test]
    fn emit_sub_i32_narrowing_bool_noop() {
        let mut func = Function::new(vec![]);
        let emitted = emit_sub_i32_narrowing(&mut func, &TypeInfoKind::Bool);
        assert!(!emitted, "should not emit narrowing for bool");
    }
}
