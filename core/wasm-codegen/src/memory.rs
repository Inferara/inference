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
//! Linear Memory (1 page = 64KB initially)
//! +--------------------------------------------+  0x10000 (64KB)
//! |                                            |
//! |         Stack (grows downward)             |
//! |                                            |
//! +-- __stack_pointer -------------------------+  (mutable global i32)
//! |                                            |
//! |              (free space)                  |
//! |                                            |
//! +--------------------------------------------+  0x00000
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

/// Initial value for `__stack_pointer`: top of the first 64 KB memory page.
pub(crate) const STACK_POINTER_INIT: i32 = 65536;

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
/// Passed as a parameter to `lower_statement` / `lower_expression` (not stored on
/// `Compiler`, because frame layout is per-function, not per-module).
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
pub(crate) fn element_size(kind: &TypeInfoKind) -> u32 {
    match kind {
        TypeInfoKind::Bool | TypeInfoKind::Number(NumberType::I8 | NumberType::U8) => 1,
        TypeInfoKind::Number(NumberType::I16 | NumberType::U16) => 2,
        TypeInfoKind::Number(NumberType::I32 | NumberType::U32) => 4,
        TypeInfoKind::Number(NumberType::I64 | NumberType::U64) => 8,
        _ => todo!("Unsupported array element type: {kind:?}"),
    }
}

/// Rounds `size` up to the nearest multiple of [`FRAME_ALIGNMENT`].
pub(crate) fn align_to_frame(size: u32) -> u32 {
    (size + FRAME_ALIGNMENT - 1) & !(FRAME_ALIGNMENT - 1)
}

/// Selects the appropriate WASM store instruction based on an `ArraySlot`'s element size.
///
/// This avoids needing a `TypeInfoKind` at the call site (useful when the array
/// literal node may not have type info, but the slot was pre-computed from the
/// variable definition).
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
#[allow(dead_code)] // Prepared for Phase 3 (array element access)
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
        TypeInfoKind::Number(NumberType::I16 | NumberType::U16) => {
            Instruction::I32Store16(memarg)
        }
        TypeInfoKind::Number(NumberType::I32 | NumberType::U32) => Instruction::I32Store(memarg),
        TypeInfoKind::Number(NumberType::I64 | NumberType::U64) => Instruction::I64Store(memarg),
        _ => todo!("Unsupported array element type for store: {elem_type:?}"),
    }
}

/// Selects the appropriate WASM load instruction for an element type.
///
/// Uses signed extension for sub-i32 types (`i32.load8_s`, `i32.load16_s`)
/// to preserve the sign bit. This matches the convention that sub-i32 values
/// are stored truncated and loaded with sign extension.
#[allow(dead_code)] // Prepared for Phase 3 (array element access)
pub(crate) fn load_instruction(elem_type: &TypeInfoKind) -> Instruction<'static> {
    let memarg = MemArg {
        offset: 0,
        align: natural_alignment(elem_type),
        memory_index: MEMORY_INDEX,
    };
    match elem_type {
        TypeInfoKind::Bool | TypeInfoKind::Number(NumberType::U8) => {
            Instruction::I32Load8U(memarg)
        }
        TypeInfoKind::Number(NumberType::I8) => Instruction::I32Load8S(memarg),
        TypeInfoKind::Number(NumberType::U16) => Instruction::I32Load16U(memarg),
        TypeInfoKind::Number(NumberType::I16) => Instruction::I32Load16S(memarg),
        TypeInfoKind::Number(NumberType::I32 | NumberType::U32) => Instruction::I32Load(memarg),
        TypeInfoKind::Number(NumberType::I64 | NumberType::U64) => Instruction::I64Load(memarg),
        _ => todo!("Unsupported array element type for load: {elem_type:?}"),
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
    fn stack_pointer_init_is_page_size() {
        assert_eq!(STACK_POINTER_INIT, 65536);
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
        assert_eq!(
            natural_alignment(&TypeInfoKind::Number(NumberType::I8)),
            0
        );
    }

    #[test]
    fn natural_alignment_2_byte() {
        assert_eq!(
            natural_alignment(&TypeInfoKind::Number(NumberType::I16)),
            1
        );
    }

    #[test]
    fn natural_alignment_4_byte() {
        assert_eq!(
            natural_alignment(&TypeInfoKind::Number(NumberType::I32)),
            2
        );
    }

    #[test]
    fn natural_alignment_8_byte() {
        assert_eq!(
            natural_alignment(&TypeInfoKind::Number(NumberType::I64)),
            3
        );
    }
}
