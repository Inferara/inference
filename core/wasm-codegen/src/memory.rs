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

use rustc_hash::FxHashMap;

/// Initial value for `__stack_pointer`: top of the first 64 KB memory page.
pub(crate) const STACK_POINTER_INIT: i32 = 65536;

/// Stack frame alignment in bytes (matches LLVM/Rust WASM convention).
pub(crate) const FRAME_ALIGNMENT: u32 = 16;

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

/// Rounds `size` up to the nearest multiple of [`FRAME_ALIGNMENT`].
pub(crate) fn align_to_frame(size: u32) -> u32 {
    (size + FRAME_ALIGNMENT - 1) & !(FRAME_ALIGNMENT - 1)
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
}
