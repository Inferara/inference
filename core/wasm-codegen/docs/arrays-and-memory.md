# Arrays and Linear Memory Lowering

## Overview

This document explains how Inference compiles fixed-size array types to WebAssembly linear memory with a shadow stack (similar to Rust/LLVM).

Arrays are **stack-allocated** using a frame pointer and stack pointer mechanism. Each function that uses arrays:
1. Computes a frame layout at compile time
2. Emits a prologue to allocate the frame on entry
3. Reads/writes elements via load/store instructions
4. Emits an epilogue to deallocate the frame on exit

## Compilation Phases

### Phase 0: Type-Checking (not in wasm-codegen)

The `core/type-checker` crate validates:
- Array element types are scalar: `bool`, `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64`
- Array lengths are positive compile-time constants
- Array variables, parameters, and literals have correct types

### Phase 1: Frame Layout Computation

Before instruction emission, `compute_frame_layout()` walks the entire function body to:

1. Allocate space for array-typed **parameters** (copy space for callee)
2. Collect array variable declarations (nested arbitrarily in blocks, `if`, `loop`)
3. Assign byte offsets to each array within the frame
4. Compute total frame size, aligned to 16 bytes
5. Allocate a synthetic WASM local `__frame_ptr` to hold the frame base address

```
+----------- (frame pointer + frame size)
| Array 2  |  offset = 8, length = 2, elem_size = 4 (i32)
+----------- (frame pointer + 8)
| Array 1  |  offset = 0, length = 3, elem_size = 1 (bool)
+----------- (frame pointer)
```

**Key insight**: Array-typed parameters get copy space in the callee's frame to enforce **value semantics**. When a function is called with an array argument, the caller passes a pointer; the callee copies the data into its own frame slot so mutations don't affect the caller's data.

### Phase 2: Instruction Emission

During `lower_statement()` and `lower_expression()`, arrays are lowered to:

- **Array variable definition** (`let arr: [i32; 3] = [1, 2, 3]`)
  - Lowered to store instructions at frame addresses
  - Supported initializers: array literals, uzumaki (`@`)

- **Array index read** (`x = arr[i]`)
  - Compute address: `base + i * elem_size`
  - Emit load instruction (sign/zero-extending as needed)

- **Array index write** (`arr[i] = x`)
  - Compute address: `base + i * elem_size`
  - Lower RHS expression to WASM value
  - Emit store instruction

- **Array parameter copy** (automatic on function entry)
  - For each array-typed parameter, copy caller's data into callee's frame
  - Optimizes small arrays (≤ 16 elements) as unrolled element copies
  - Large arrays use `memory.copy` instruction

### Phase 3: Module Assembly

In `finish()`, if any function uses arrays (`self.has_memory == true`):

1. **Memory Section** - Declares 1 linear memory page (64 KB initial)
2. **Global Section** - Exports `__stack_pointer` (mutable i32 global)
   - Initialized to `0x10000` (65536 = STACK_SIZE, top of the stack region)
   - Stack grows downward toward address 0 (stack-first layout)

## Memory Layout

WebAssembly linear memory is a flat byte array accessed via load/store instructions.

```
+------ 0x10000 (65536 bytes = 1 page)
|
|  [ Free space: future data sections, heap ]
|
+-- STACK_SIZE (65536) = __stack_pointer initial value
|
|  [ Stack grows downward ]
|
+------ 0x00000 (memory start)
  overflow below 0 = WASM OOB trap
```

This is the stack-first layout used by Rust and Zig: the stack occupies the bottom of the address space so that any overflow that pushes `__stack_pointer` below address 0 triggers a WASM out-of-bounds memory trap automatically. Future data sections will be placed above the stack region, starting at STACK_SIZE.

**Frame allocation**:

```
; Function entry (prologue)
global.get $__stack_pointer    ;; load current stack top
i32.const <frame_size>
i32.sub                        ;; decrement stack pointer
local.tee $__frame_ptr         ;; save new frame base, duplicate on stack
global.set $__stack_pointer    ;; update global
local.get $__frame_ptr         ;; reload for memory.fill
i32.const 0                    ;; fill with zeros
i32.const <frame_size>
memory.fill                    ;; zero-initialize entire frame

; Function body: arrays are accessed via $__frame_ptr + offset

; Function exit (epilogue)
local.get $__frame_ptr
i32.const <frame_size>
i32.add                        ;; increment back up
global.set $__stack_pointer    ;; restore stack pointer
```

## Implementation Details

### `memory.rs` Module

This module contains all memory-related helpers:

| Function/Type | Purpose |
|---|---|
| `FrameLayout` | Data structure: `total_size`, `array_offsets`, `frame_ptr_local` |
| `ArraySlot` | Per-array metadata: `offset`, `elem_size`, `length` |
| `element_size()` | Map `TypeInfoKind` → byte size (1, 2, 4, or 8) |
| `align_to_frame()` | Round up to 16-byte boundary |
| `store_instruction()` | Select `i32.store8`, `i32.store16`, `i32.store`, or `i64.store` |
| `load_instruction()` | Select appropriate load (sign/zero-extending as needed) |
| `emit_stack_prologue()` | Generate frame allocation code |
| `emit_stack_epilogue()` | Generate frame deallocation code |
| `emit_array_param_copy()` | Copy caller's array data into callee's frame |

### `compiler.rs` Additions

#### `compute_frame_layout()`

```rust
fn compute_frame_layout(
    block: &BlockType,
    ctx: &TypedContext,
    frame_ptr_local_idx: u32,
    arguments: Option<&[ArgumentType]>,
) -> Option<FrameLayout>
```

Returns `None` if no arrays are present (no frame needed).

**Algorithm**:
1. Iterate parameters: if any are array-typed, allocate copy space
2. Recursively walk block statements, collecting array variables
3. Sum byte sizes and align to 16 bytes
4. Return `FrameLayout` or `None`

#### `lower_array_index_access()`

```rust
fn lower_array_index_access(
    &self,
    aiae: &ArrayIndexAccessExpression,
    ctx: &TypedContext,
    func: &mut Function,
    locals_map: &FxHashMap<String, (u32, ValType)>,
    frame_layout: Option<&FrameLayout>,
)
```

Lowers `arr[i]` to a load instruction sequence:

```wasm
<lower array expr>      ;; push base pointer (i32)
<lower index expr>      ;; push index (i32)
i32.const <elem_size>
i32.mul                 ;; byte_offset = index * elem_size
i32.add                 ;; address = base + byte_offset
i32.load / i64.load / ... ;; load element
```

**Type dispatch**: The type-checker sets the node's type info to the **element type**, not the array type. We query this to select the correct load instruction.

#### `lower_array_index_write()`

```rust
fn lower_array_index_write(
    &self,
    aiae: &ArrayIndexAccessExpression,
    assign_stmt: &AssignStatement,
    ctx: &TypedContext,
    func: &mut Function,
    locals_map: &FxHashMap<String, (u32, ValType)>,
    frame_layout: Option<&FrameLayout>,
)
```

Lowers `arr[i] = value` to:

```wasm
<lower array expr>      ;; push base pointer
<lower index expr>      ;; push index
i32.const <elem_size>
i32.mul
i32.add                 ;; address computed
<lower right side>      ;; push value
i32.store / i64.store / ... ;; store element
```

#### `lower_array_uzumaki()`

```rust
fn lower_array_uzumaki(
    &self,
    uzumaki_id: u32,
    elem_type: &TypeInfo,
    length: u32,
    ctx: &TypedContext,
    func: &mut Function,
    frame_layout: Option<&FrameLayout>,
)
```

Lowers `let arr: [i32; 3] = @` to element-wise non-deterministic stores:

```wasm
local.get $__frame_ptr
i32.const 0
i32.add
i32.const <0xfc 0x31>     ;; i32.uzumaki
i32.store                  ;; store random value

local.get $__frame_ptr
i32.const 4
i32.add
i32.const <0xfc 0x31>
i32.store

; ... repeat for each element
```

**Invariants**:
- Only reachable for array-typed variables (type-checker enforces)
- `find_enclosing_variable_name()` locates the parent variable
- `compute_frame_layout()` pre-computes all array offsets
- Result: all elements of the array hold non-deterministic values

## Array Type Representation

In the type-checker's `TypeInfoKind`:

```rust
TypeInfoKind::Array(Box<TypeInfo>, u32)
              ↑                      ↑
              element type           length (compile-time constant)
```

The WASM type of an array-valued **expression** is `i32` (a pointer). But the type-checker tracks the full array type for memory layout computation.

Examples:

```inference
let arr: [i32; 3] = [1, 2, 3];  // TypeInfoKind::Array(i32, 3)
let x: i32 = arr[1];            // arr[1] has TypeInfoKind::Number(I32)
                                // arr itself (if passed to fn) has TypeInfoKind::Array(i32, 3)
```

## Sub-i32 Element Types

Arrays of small types (`bool`, `i8`, `u8`, `i16`, `u16`) are stored as-is in memory, not promoted to i32:

- `[u8; 5]` uses 5 bytes (not 20)
- Reads use sign/zero-extending load instructions (`i32.load8_s`, `i32.load8_u`)
- Stores use sub-word store instructions (`i32.store8`, `i32.store16`)

This matches Rust/LLVM conventions and is memory-efficient.

## Frame Alignment

All frames are aligned to 16 bytes (matching LLVM/Rust WASM). This is:

- Not required by WASM (memory access has no alignment enforcement)
- A convention for consistency with other compilers
- Applied after computing total array sizes

Example:

```
Arrays:  bool (1 byte) + i32 (4 bytes) = 5 bytes
Aligned: (5 + 15) & ~15 = 16 bytes
```

## Copy-on-Entry for Array Parameters

When an array-typed parameter is passed to a function:

**Caller**: Passes a pointer (i32) to the array data in linear memory.

**Callee**:
1. Allocates space in its frame (computed by `compute_frame_layout()`)
2. Copies caller's data element-by-element or via `memory.copy`
3. Updates the parameter local to point to the copy
4. All subsequent reads/writes operate on the local copy

**Benefit**: Mutations inside the callee don't affect the caller's array (value semantics).

**Optimization**: Arrays with ≤ 16 elements are copied element-by-element (avoids `memory.copy` overhead). Larger arrays use `memory.copy`.

```wasm
; Caller side (unrolled):
local.get $src_ptr           ; pass pointer
call $foo                    ;; foo([i32; 3])

; Callee side:
; Prologue allocates frame with 3 i32s = 12 bytes
; Copy-on-entry loop (N=3):
local.get $__frame_ptr       ; dest = frame + 0
i32.const 0
i32.add
local.get $param_ptr         ; src = param
i32.const 0
i32.add
i32.load                     ; load element 0
i32.store                    ; store element 0

; ... repeat for elements 1, 2 ...

; Update param to point to frame copy:
local.get $__frame_ptr
local.set $param_ptr
```

## Load/Store Instruction Selection

The helpers in `memory.rs` select WASM instructions based on element type and size:

| Element Type | Size | Load | Store |
|---|---|---|---|
| `bool` | 1 | `i32.load8_u` | `i32.store8` |
| `i8` | 1 | `i32.load8_s` | `i32.store8` |
| `u8` | 1 | `i32.load8_u` | `i32.store8` |
| `i16` | 2 | `i32.load16_s` | `i32.store16` |
| `u16` | 2 | `i32.load16_u` | `i32.store16` |
| `i32` | 4 | `i32.load` | `i32.store` |
| `u32` | 4 | `i32.load` | `i32.store` |
| `i64` | 8 | `i64.load` | `i64.store` |
| `u64` | 8 | `i64.load` | `i64.store` |

**Alignment** is set based on element size (log2):
- 1 byte → align=0 (2^0 = 1)
- 2 bytes → align=1 (2^1 = 2)
- 4 bytes → align=2 (2^2 = 4)
- 8 bytes → align=3 (2^3 = 8)

WASM does not enforce alignment; these hints assist runtimes with optimization.

## Constant Index Optimization (TODO)

Currently, array index access always emits:

```wasm
<base_ptr>
<index>
i32.const <elem_size>
i32.mul
i32.add
```

Future optimization: Detect constant indices at compile time and emit:

```wasm
<base_ptr>
i32.const <base_offset + constant_index * elem_size>
i32.add
```

Tracked in issue #148.

## Known Limitations

1. **Nested arrays**: `[[i32; 3]; 2]` not yet supported (type-checker restriction)
2. **Array member types**: Structs/custom types as array elements not yet supported
3. **Partial initialization**: `let arr: [i32; 5] = [1, 2, _, _, _];` not yet supported (would require optional elements or sparse initialization)
4. **Mutable array parameters**: Parameters are immutable by default; tracking mutable parameters is future work
5. **Recursion with arrays**: Functions using arrays cannot currently recurse (no stack overflow protection, analysis pass needed)

## Cov Mark Coverage

Coverage marks for testing array-related code:

| Mark | Location | Meaning |
|---|---|---|
| `wasm_codegen_emit_stack_prologue` | `emit_stack_prologue()` | Frame allocation code emitted |
| `wasm_codegen_emit_stack_epilogue` | `emit_stack_epilogue()` | Frame deallocation code emitted |
| `wasm_codegen_emit_array_param_copy` | `emit_array_param_copy()` | Array parameter copied to frame |
| `wasm_codegen_emit_array_index_read` | `lower_array_index_access()` | Array element read via load |
| `wasm_codegen_emit_array_index_write` | `lower_array_index_write()` | Array element written via store |
| `wasm_codegen_emit_array_uzumaki` | `lower_array_uzumaki()` | Non-deterministic array initialization |

## Examples

### Simple Array Literal

**Inference:**
```inference
pub fn get_array() -> i32 {
    let arr: [i32; 3] = [10, 20, 30];
    return arr[1];  // returns 20
}
```

**Generated WASM (conceptual):**
```wasm
(func (export "get_array") (result i32)
  (local $__frame_ptr i32)

  ;; Prologue
  (global.get 0)           ;; __stack_pointer
  (i32.const 12)           ;; frame_size for 3 i32s
  (i32.sub)
  (local.tee 0)            ;; $__frame_ptr = new top
  (global.set 0)           ;; update stack pointer
  (local.get 0)
  (i32.const 0)
  (i32.const 12)
  (memory.fill)            ;; zero-fill frame

  ;; Initialize array
  (local.get 0)            ;; frame + 0
  (i32.const 10)
  (i32.store)              ;; arr[0] = 10

  (local.get 0)
  (i32.const 4)
  (i32.add)                ;; frame + 4
  (i32.const 20)
  (i32.store)              ;; arr[1] = 20

  (local.get 0)
  (i32.const 8)
  (i32.add)                ;; frame + 8
  (i32.const 30)
  (i32.store)              ;; arr[2] = 30

  ;; Read arr[1]
  (local.get 0)            ;; frame
  (i32.const 1)            ;; index
  (i32.const 4)            ;; elem_size
  (i32.mul)                ;; byte_offset = 4
  (i32.add)                ;; address = frame + 4
  (i32.load)               ;; load element (value 20 on stack)

  ;; Epilogue
  (local.get 0)
  (i32.const 12)
  (i32.add)
  (global.set 0)           ;; restore stack pointer

  (return)
)
```

### Array Parameter

**Inference:**
```inference
pub fn sum_array(arr: [i32; 3]) -> i32 {
    return arr[0] + arr[1] + arr[2];
}
```

**Key codegen points**:
1. Parameter `arr` gets local index 0 (pointer)
2. `compute_frame_layout()` allocates 12 bytes (frame size for copy)
3. Prologue allocates frame, then `emit_array_param_copy()` copies 3 elements
4. Each `arr[i]` read loads from frame base + offset
5. Epilogue restores stack pointer

## Related Resources

- `core/wasm-codegen/src/memory.rs` - Implementation
- `core/wasm-codegen/src/compiler.rs` - `compute_frame_layout()`, array lowering methods
- `core/type-checker` - Type validation for array types
- WASM Memory spec: https://webassembly.org/docs/modules/#memory-section
- WASM Load/Store spec: https://webassembly.org/docs/semantics/#memory-operators
