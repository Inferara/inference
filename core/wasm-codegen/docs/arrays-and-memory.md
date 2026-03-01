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
3. Align each array offset to element type's natural alignment (e.g., 4 bytes for i32, 8 bytes for i64)
4. Compute total frame size, aligned to 16 bytes
5. Allocate a synthetic WASM local `__frame_ptr` to hold the frame base address

```
+----------- (frame pointer + 12)
| Array 2  |  offset = 4, length = 2, elem_size = 4 (i32)
+----------- (frame pointer + 4)
| padding  |  1 byte (align i32 to 4-byte boundary)
+----------- (frame pointer + 3)
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

#### Local Registration for Arrays

During the `pre_scan_locals()` phase (which walks all statements before instruction emission), array variables are registered as **i32 WASM locals**, identical to any other scalar variable. The type-checker sets `TypeInfoKind::Array(...)` on the variable node, and `pre_scan_locals` treats it as a non-i64 type and assigns an `i32` local.

Later, during instruction emission, when an array literal is initialized, `lower_literal()` stores array elements in linear memory and pushes the frame pointer (pointer to the array data) onto the WASM stack, which is then assigned to the local via `local.set`.

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

**Important**: This phase allocates a **synthetic `__frame_ptr` local** (separate from array variable locals) to hold the frame base address. This local is used internally for all frame addressing and is not visible to source code.

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

Lowers `arr[i]` (read access) to a load instruction sequence. The emitted instructions depend on whether the index is zero, a non-zero compile-time constant, or a runtime expression:

**Zero index** (`arr[0]`) — no offset computation:
```wasm
<lower array expr>      ;; push base pointer
i32.load / i64.load / ... ;; load element at base address
```

**Constant non-zero index** (`arr[N]`) — offset folded at compile time:
```wasm
<lower array expr>      ;; push base pointer
i32.const <N * elem_size>
i32.add                 ;; address = base + compile-time-constant
i32.load / i64.load / ... ;; load element
```

**Variable index** (`arr[i]`) — offset computed at runtime:
```wasm
<lower array expr>      ;; push base pointer
<lower index expr>      ;; push i32 index
i32.const <elem_size>
i32.mul                 ;; byte_offset = index * elem_size
i32.add                 ;; address = base + byte_offset
i32.load / i64.load / ... ;; load element
```

**What "<lower array expr>" does**: For an identifier like `arr`, this emits `local.get $arr`, which loads the i32 pointer from the array variable's local. That pointer is the base address of the array data in linear memory.

**Type dispatch**: The type-checker sets the node's type info to the **element type**, not the array type. We query this to select the correct load instruction (e.g., `i32.load8_s` for `i8` elements).

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

Lowers `arr[i] = value` using the same three-case index specialization as `lower_array_index_access()`:

**Zero index** (`arr[0] = x`):
```wasm
<lower array expr>      ;; push base pointer
<lower right side>      ;; push value
i32.store / i64.store / ... ;; store at base address
```

**Constant non-zero index** (`arr[N] = x`):
```wasm
<lower array expr>      ;; push base pointer
i32.const <N * elem_size>
i32.add                 ;; address = base + compile-time-constant
<lower right side>      ;; push value
i32.store / i64.store / ... ;; store element
```

**Variable index** (`arr[i] = x`):
```wasm
<lower array expr>      ;; push base pointer
<lower index expr>      ;; push index
i32.const <elem_size>
i32.mul
i32.add                 ;; address computed at runtime
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

## Array Variables as WASM Locals

Each array variable (whether declared with `let` or passed as a parameter) becomes a **WASM local variable** of type `i32`. This local holds a **pointer** to the array's data in linear memory, not the data itself.

**Example compilation**:

```inference
let arr: [i32; 3] = [10, 20, 30];
let x: i32 = arr[0];
```

Becomes:

```wasm
(local $arr i32)          ;; local for the pointer
(local $x i32)            ;; local for the i32 result
(local $__frame_ptr i32)  ;; synthetic frame pointer

;; Prologue: allocate frame
...
(local.set $__frame_ptr ...)

;; Initialize array: store elements at frame + offset
(local.get $__frame_ptr)
(i32.const 0)
(i32.add)
(i32.const 10)
(i32.store)  ;; arr[0] = 10
...

;; Push array pointer to local
(local.get $__frame_ptr)
(local.set $arr)  ;; arr now holds pointer

;; Read arr[0]: index is zero, so no offset computation (constant-index folding)
(local.get $arr)
(i32.load)       ;; load arr[0] directly at base address
(local.set $x)
```

**Key insight**: The variable `arr` itself is just an `i32` local; the actual array data is stored in linear memory at the address held by that local.

## Sub-i32 Element Types

Arrays of small types (`bool`, `i8`, `u8`, `i16`, `u16`) are stored as-is in memory, not promoted to i32:

- `[u8; 5]` uses 5 bytes (not 20)
- Reads use sign/zero-extending load instructions (`i32.load8_s`, `i32.load8_u`)
- Stores use sub-word store instructions (`i32.store8`, `i32.store16`)

This matches Rust/LLVM conventions and is memory-efficient.

## Frame Alignment

All frames are aligned to 16 bytes (matching LLVM/Rust WASM). This is:

- Not required by WASM (alignment in `MemArg` is a hint per spec section 4.5.4 — misaligned access must succeed)
- A convention for consistency with other compilers
- Applied after computing total array sizes

Each array within a frame is aligned to its element type's natural alignment, matching the LLVM/Rust/BasicCABI convention. For example, a `[bool; 3]` array (1-byte elements) followed by a `[i32; 2]` array (4-byte elements) will have 1 byte of padding inserted so the i32 array starts at offset 4 (a 4-byte boundary). This makes `MemArg` alignment hints truthful and enables better hardware optimization on runtimes that use alignment hints for instruction selection (e.g., SSE-aligned loads on x86). Padding bytes are automatically zeroed by the `memory.fill` in the prologue.

Example:

```
Arrays:  [bool; 3] (3 bytes) + 1 byte padding + [i32; 2] (8 bytes) = 12 bytes
Aligned: (12 + 15) & ~15 = 16 bytes
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

## Constant Index Folding

Array index access is specialized based on whether the index is a compile-time constant.

**Case 1 — Index is zero (`arr[0]`)**:

No offset computation at all. The base pointer already points to the first element:

```wasm
<lower array expr>      ;; push base pointer
i32.load / i64.load / ...  ;; load element at address = base
```

**Case 2 — Index is a non-zero constant (`arr[N]`)**:

The byte offset `N * elem_size` is computed at compile time and folded into a single `i32.const`:

```wasm
<lower array expr>      ;; push base pointer
i32.const <N * elem_size>
i32.add                 ;; address = base + (compile-time constant)
i32.load / i64.load / ...  ;; load element
```

**Case 3 — Index is a runtime variable (`arr[i]`)**:

The offset is computed at runtime using a multiply:

```wasm
<lower array expr>      ;; push base pointer
<lower index expr>      ;; push i32 index
i32.const <elem_size>
i32.mul                 ;; byte_offset = index * elem_size (runtime)
i32.add                 ;; address = base + byte_offset
i32.load / i64.load / ...  ;; load element
```

The same three-case specialization applies to array index writes (`arr[i] = x`): zero-index emits no offset instruction, constant non-zero index folds to a single `i32.const`, and variable index uses runtime multiply.

## Array Return Types (sret Calling Convention)

Functions that return array types use the **sret** (struct-return) calling convention. Returning a raw pointer to the callee's stack frame would produce a dangling pointer — the frame is freed in the epilogue before the caller can read the data. The sret convention avoids this by letting the caller own the destination storage.

### The Problem

```inference
pub fn make_array() -> [i32; 3] {
    return [10, 20, 30];
}
```

A naive implementation would return an `i32` pointer into `make_array`'s frame. But the epilogue restores `__stack_pointer` before the function returns, so that memory is immediately reusable by the next call. Any read by the caller would be a use-after-return.

### The sret Solution

The caller allocates space in its **own** frame and passes a pointer to that space as a hidden first argument. The callee writes its return data to that pointer before returning. The WASM return type becomes `void`.

```
; Inference source:                   ; WASM signature:
fn foo() -> [i32; 3]                  func $foo (param $sret i32) ;; no result
```

- The hidden `$sret` parameter is always inserted at **index 0**.
- All user-defined parameters shift up by one (index 1, 2, ...).
- This transformation is applied at `build_func_name_to_idx` time and stored in `func_array_returns`.

### WASM Signature Example

```inference
pub fn double_elements(arr: [i32; 3]) -> [i32; 3] {
    return [arr[0] * 2, arr[1] * 2, arr[2] * 2];
}
```

Compiles to:

```wasm
;; WASM type: (param i32 i32) (result)
;;             ^sret  ^arr
(func $double_elements (param $sret i32) (param $arr i32)
  ;; Write each element to sret destination:
  local.get $sret
  i32.const 0     ;; byte offset for element 0
  i32.add
  ...             ;; compute arr[0] * 2
  i32.store

  local.get $sret
  i32.const 4     ;; byte offset for element 1
  i32.add
  ...
  i32.store

  ;; Epilogue + return (no value pushed)
)
```

### Three Return Cases

`lower_sret_return()` handles three forms of return expression:

**1. Identifier** (`return arr`):

Uses `memory.copy` to copy the source array's data to the sret destination:

```wasm
local.get $sret     ;; destination
local.get $arr      ;; source (pointer to callee's frame copy)
i32.const 12        ;; byte_size = length * elem_size
memory.copy
```

**2. Array literal** (`return [1, 2, 3]`):

Writes each element directly to the sret destination with individual stores:

```wasm
local.get $sret
i32.const 0         ;; byte offset for element 0
i32.add
i32.const 1
i32.store

local.get $sret
i32.const 4
i32.add
i32.const 2
i32.store
;; ... and so on
```

**3. Chained function call** (`return inner(x)`):

Forwards the sret pointer to the inner call — zero-copy:

```wasm
local.get $sret     ;; pass our sret as inner's sret arg
<lower user args>
call $inner
```

This works correctly only when `inner` is also an sret function (returns an array of the same type). A non-sret callee in this position causes a compile-time panic.

### Standalone sret Calls

When an sret function is called as a standalone statement (result discarded), the caller has no destination frame slot. The compiler injects a dummy pointer (`i32.const 0`) as the sret argument. Address 0 is valid in the stack-first layout because the stack fills the entire 64 KB page, so the write is safe (it overwrites the very bottom of the stack) and the value is immediately discarded.

```wasm
i32.const 0         ;; dummy sret destination
<lower user args>
call $foo           ;; result written to address 0, then ignored
```

### Caller Side: `let b: [i32; 3] = foo()`

When the result is captured, the caller allocates space in its own frame for `b`, then passes a pointer to that slot as the sret argument:

```wasm
local.get $__frame_ptr
i32.const <b_offset>    ;; offset of b in caller's frame
i32.add                 ;; sret destination pointer
<lower user args>
call $foo               ;; foo writes into caller's frame
;; After call: set local b to point to caller's frame slot
local.get $__frame_ptr
i32.const <b_offset>
i32.add
local.set $b
```

### `ArrayReturnInfo` and `func_array_returns`

```rust
struct ArrayReturnInfo {
    elem_kind: TypeInfoKind,  // element type kind (for store instruction selection)
    elem_size: u32,           // bytes per element
    length: u32,              // number of elements
}
```

`func_array_returns: FxHashMap<String, ArrayReturnInfo>` is populated during `build_func_name_to_idx` (before any code emission) so that both callers and callees see consistent sret metadata.

## Known Limitations

1. **Nested arrays**: `[[i32; 3]; 2]` not yet supported (type-checker restriction)
2. **Array member types**: Structs/custom types as array elements not yet supported
3. **Partial initialization**: `let arr: [i32; 5] = [1, 2, _, _, _];` not yet supported (would require optional elements or sparse initialization)
4. **Recursion with arrays**: Functions using arrays cannot currently recurse (no stack overflow protection, analysis pass needed)

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

  ;; Read arr[1]: constant index 1 folded to 1*4=4 at compile time
  (local.get 0)            ;; frame (base pointer)
  (i32.const 4)            ;; compile-time offset = 1 * elem_size
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
