# Memory Allocation in WASM Codegen

Inference compiles fixed-size arrays to WebAssembly linear memory using a shadow stack. This document explains how arrays are allocated, initialized, accessed, passed to functions, and deallocated. It covers the WASM instructions emitted for each operation, the rationale behind the design, how other compilers handle the same problems, and the formal verification implications.

## The Problem

WebAssembly's native value model is flat: every local variable is a scalar (`i32`, `i64`, `f32`, `f64`). There is no instruction for declaring a local of type "array of 3 integers." When a language has compound types — arrays, structs, strings — the compiler must place them somewhere in linear memory and manipulate them via pointers and load/store instructions.

Three allocation strategies are available:

1. **Static data segment**: Embed the array in the module's data section. Works for immutable constants but not for stack-scoped mutable arrays.
2. **Heap allocation**: Use a `malloc`/`free` scheme or a garbage collector. Requires a runtime, introduces allocation failure modes, and makes formal verification significantly harder.
3. **Shadow stack**: Reserve a region of linear memory and manage it with a stack pointer global, mirroring how native compilers manage function call frames.

Inference uses option 3. Arrays have stack-scoped lifetimes (they are created when a function is entered and destroyed when it returns), so they fit naturally into a stack discipline. No runtime, no allocator, no GC.

## Linear Memory Layout

WebAssembly linear memory is a contiguous byte array. Inference allocates one page (64 KB) and uses it entirely as a stack:

```text
Linear Memory (1 page = 64KB)
+--------------------------------------------+  0x10000 (65536)
|                                            |
|         Stack (grows downward)             |
|                                            |
+-- __stack_pointer -------------------------+  (mutable global i32)
|                                            |
|              (free space)                  |
|                                            |
+--------------------------------------------+  0x00000
```

`__stack_pointer` is a mutable i32 WebAssembly global initialized to 65536 — the top of the first memory page. When a function with arrays is called, `__stack_pointer` is decremented by the frame size. When the function returns, it is restored. The stack grows downward, matching the convention used by LLVM, Rust, and Zig when targeting WebAssembly.

Programs without arrays do not get a memory section, a global section, or any memory-related exports. The compiler tracks a `has_memory` flag and only emits these sections when at least one function uses arrays. Existing programs produce identical WASM output — zero regression.

## Stack Frame Layout

Each function with arrays gets a `FrameLayout` — a per-function data structure computed before code generation. It maps each array variable to an `ArraySlot` describing its byte offset within the frame, element size, and element count.

Consider a function with two arrays:

```
pub fn two_arrays() -> i32 {
    let a: [i32; 2] = [1, 2];
    let b: [i32; 2] = [3, 4];
    return 0;
}
```

The frame layout computation:
1. `a: [i32; 2]` — element size 4, count 2, total 8 bytes, offset 0
2. `b: [i32; 2]` — element size 4, count 2, total 8 bytes, offset 8
3. Raw size: 16 bytes
4. Aligned to 16-byte boundary: 16 bytes (already aligned)

Frame alignment is 16 bytes, matching the LLVM and Rust WASM convention. A function with a single `[bool; 3]` array (3 bytes raw) gets a 16-byte frame. A function with `[i64; 3]` (24 bytes raw) gets a 32-byte frame.

The frame pointer is stored in a synthetic WASM local named `__frame_ptr`. This local is added during the local pre-scan phase alongside the user's declared locals.

## Function Prologue and Epilogue

Every function with arrays emits a prologue at entry and an epilogue at every exit point.

### Prologue

The prologue decrements `__stack_pointer`, saves the frame pointer, and zero-initializes the entire frame:

```wat
;; Prologue for a 16-byte frame
global.get 0              ;; load __stack_pointer
i32.const 16              ;; frame size
i32.sub                   ;; decrement
local.tee $__frame_ptr    ;; save frame pointer AND keep on stack
global.set 0              ;; update __stack_pointer
local.get $__frame_ptr    ;; destination for memory.fill
i32.const 0               ;; fill value (zero)
i32.const 16              ;; fill length
memory.fill               ;; zero-initialize entire frame
```

The zero-initialization via `memory.fill` prevents uninitialized reads, ensures deterministic behavior across function calls, and eliminates information leakage between stack frames. This matches Zig's approach to stack initialization.

The `memory.fill` is technically redundant when all array elements are explicitly initialized (e.g., `let arr: [i32; 3] = [1, 2, 3]`). The simplicity and safety of unconditional zeroing outweigh the negligible runtime cost. A future optimization pass could skip `memory.fill` when all arrays in the frame are provably fully initialized.

### Epilogue

The epilogue restores `__stack_pointer` to its pre-call value:

```wat
;; Epilogue for a 16-byte frame
local.get $__frame_ptr    ;; saved frame pointer
i32.const 16              ;; frame size
i32.add                   ;; compute original stack pointer
global.set 0              ;; restore __stack_pointer
```

The epilogue is emitted at two sites:
1. Before every `return` statement
2. Before the function-end `unreachable` / `end` sequence

Both sites must emit the epilogue. Missing one would silently corrupt `__stack_pointer`, causing subsequent function calls to allocate overlapping frames.

Here is the complete WAT output for a function that allocates, initializes, and returns:

```wat
(func $i32_array (type 0) (result i32)
  (local $arr i32) (local $__frame_ptr i32)
  ;; --- prologue ---
  global.get 0
  i32.const 16
  i32.sub
  local.tee $__frame_ptr
  global.set 0
  local.get $__frame_ptr
  i32.const 0
  i32.const 16
  memory.fill
  ;; --- store elements ---
  local.get $__frame_ptr
  i32.const 0
  i32.add
  i32.const 10
  i32.store              ;; arr[0] = 10
  local.get $__frame_ptr
  i32.const 4
  i32.add
  i32.const 20
  i32.store              ;; arr[1] = 20
  local.get $__frame_ptr
  i32.const 8
  i32.add
  i32.const 30
  i32.store              ;; arr[2] = 30
  local.get $__frame_ptr
  local.set $arr         ;; arr = frame_ptr (base address)
  ;; --- return with epilogue ---
  i32.const 0
  local.get $__frame_ptr
  i32.const 16
  i32.add
  global.set 0           ;; restore __stack_pointer
  return
  ;; --- function-end epilogue + unreachable ---
  local.get $__frame_ptr
  i32.const 16
  i32.add
  global.set 0
  unreachable
)
```

## Array Literal Lowering

An array literal `[10, 20, 30]` is lowered by storing each element at its computed offset from the frame pointer:

```
;; For element i of an array at frame offset `array_offset`:
local.get $__frame_ptr
i32.const <array_offset + i * elem_size>
i32.add
<push element value>
<store instruction>
```

After all elements are stored, the variable is set to the array's base address:

```
local.get $__frame_ptr
i32.const <array_offset>
i32.add
local.set $arr
```

The array variable itself is an `i32` local holding the base pointer. All subsequent reads and writes go through this pointer.

### Store Instruction Selection

The store instruction depends on the element type:

| Element Type | Size | Store Instruction | Alignment |
|---|---|---|---|
| `bool` | 1 | `i32.store8` | 0 (2^0 = 1) |
| `i8`, `u8` | 1 | `i32.store8` | 0 |
| `i16`, `u16` | 2 | `i32.store16` | 1 (2^1 = 2) |
| `i32`, `u32` | 4 | `i32.store` | 2 (2^2 = 4) |
| `i64`, `u64` | 8 | `i64.store` | 3 (2^3 = 8) |

WASM encodes alignment as `log2(byte_alignment)`. A 4-byte `i32.store` with natural alignment encodes `align=2` because `2^2 = 4`.

For a `[bool; 4]` array, elements are packed byte-by-byte:

```wat
;; let flags: [bool; 4] = [true, false, true, false];
local.get $__frame_ptr
i32.const 0
i32.add
i32.const 1              ;; true
i32.store8               ;; 1 byte at offset 0
local.get $__frame_ptr
i32.const 1
i32.add
i32.const 0              ;; false
i32.store8               ;; 1 byte at offset 1
local.get $__frame_ptr
i32.const 2
i32.add
i32.const 1              ;; true
i32.store8               ;; 1 byte at offset 2
local.get $__frame_ptr
i32.const 3
i32.add
i32.const 0              ;; false
i32.store8               ;; 1 byte at offset 3
```

## Array Index Read

Reading `arr[i]` computes the element address as `base + index * elem_size` and emits a load:

```wat
;; return arr[i]  where arr: [i32; 3]
local.get $arr            ;; push base pointer (i32)
local.get $i              ;; push index (i32)
i32.const 4               ;; element size
i32.mul
i32.add                   ;; address = base + index * 4
i32.load                  ;; load i32 from computed address
```

For constant indices, the same code is emitted with `i32.const` instead of `local.get`. A future optimization could fold constant-index access to `frame_ptr + constant_offset`, eliminating the multiply.

### Load Instruction Selection

The load instruction depends on the element type and its signedness:

| Element Type | Load Instruction | Extension |
|---|---|---|
| `bool` | `i32.load8_u` | Zero-extending |
| `u8` | `i32.load8_u` | Zero-extending |
| `i8` | `i32.load8_s` | Sign-extending |
| `u16` | `i32.load16_u` | Zero-extending |
| `i16` | `i32.load16_s` | Sign-extending |
| `i32`, `u32` | `i32.load` | Full width |
| `i64`, `u64` | `i64.load` | Full width |

Signed types use sign-extending loads (`load8_s`, `load16_s`) to correctly propagate the sign bit from the stored byte/halfword into the full i32 value. Unsigned types and `bool` use zero-extending loads (`load8_u`, `load16_u`).

This distinction matters: an `i8` value of `-1` is stored as `0xFF`. When loaded with `i32.load8_s`, it becomes `0xFFFFFFFF` (-1 as i32). When loaded with `i32.load8_u`, it would become `0x000000FF` (255 as i32). Using the wrong extension would silently corrupt signed values.

## Array Index Write

Writing `arr[i] = value` computes the address the same way and emits a store:

```wat
;; arr[0] = 42  where arr: [i32; 3]
local.get $arr            ;; push base pointer
i32.const 0               ;; push index
i32.const 4               ;; element size
i32.mul
i32.add                   ;; address = base + 0 * 4
i32.const 42              ;; push value
i32.store                 ;; store i32 at computed address
```

The type checker enforces that the array variable is declared `mut` before allowing index assignment. Writing to a non-mutable array is a compile-time error.

## Array Parameter Passing

Arrays are passed to functions by pointer at the WASM level. At call sites, the caller pushes the array's base address (an `i32`) onto the stack:

```
;; Caller: sum_array(data)
local.get $data           ;; push array base pointer
call $sum_array
```

The callee receives this pointer as a regular `i32` parameter. But the callee does not use the caller's memory directly. Instead, it copies the entire array into its own stack frame on entry — a copy-on-entry pattern that provides value semantics.

### Copy-on-Entry

The callee's prologue allocates a frame, then copies each element from the caller's pointer into the callee's frame. After the copy, the parameter local is overwritten with the callee's frame address:

```wat
(func $sum_array (param $arr i32) (result i32)
  (local $__frame_ptr i32)
  ;; --- prologue ---
  global.get 0
  i32.const 16
  i32.sub
  local.tee $__frame_ptr
  global.set 0
  local.get $__frame_ptr
  i32.const 0
  i32.const 16
  memory.fill
  ;; --- copy element 0 ---
  local.get $__frame_ptr
  i32.const 0
  i32.add
  local.get $arr          ;; source: caller's pointer
  i32.load                ;; load element 0 from caller
  i32.store               ;; store into callee's frame
  ;; --- copy element 1 ---
  local.get $__frame_ptr
  i32.const 4
  i32.add
  local.get $arr
  i32.const 4
  i32.add
  i32.load
  i32.store
  ;; --- copy element 2 ---
  local.get $__frame_ptr
  i32.const 8
  i32.add
  local.get $arr
  i32.const 8
  i32.add
  i32.load
  i32.store
  ;; --- redirect parameter to local copy ---
  local.get $__frame_ptr
  local.set $arr          ;; now $arr points to callee's copy
  ;; ... function body uses $arr normally ...
)
```

After the copy, `$arr` points to the callee's own memory. Any mutations the callee makes to `arr` (if it is declared `mut`) affect only the local copy. The caller's array is untouched.

This can be verified with the `verify_copy_semantics` test:

```
pub fn mutate_copy(mut arr: [i32; 3]) -> i32 {
    arr[0] = 99;
    return arr[0];
}

pub fn verify_copy_semantics() -> i32 {
    let data: [i32; 3] = [1, 2, 3];
    let ignored: i32 = mutate_copy(data);
    return data[0];         // returns 1, not 99
}
```

`mutate_copy` modifies its local copy and returns 99. `verify_copy_semantics` passes `data` to `mutate_copy`, then reads `data[0]` — which is still 1. The callee's mutation did not affect the caller.

### Copy Optimization

For arrays with 16 or fewer elements, the copy is unrolled element by element (as shown above). For arrays with more than 16 elements, a single `memory.copy` instruction replaces the unrolled loop:

```wat
;; Bulk copy for arr: [i32; 64] (256 bytes)
local.get $__frame_ptr
i32.const <offset>
i32.add                   ;; destination: callee's frame
local.get $arr            ;; source: caller's pointer
i32.const 256             ;; byte count
memory.copy               ;; bulk copy
```

The threshold of 16 elements balances code size (unrolled copies are larger but avoid call overhead) against simplicity.

### Why Value Semantics

The alternative to copy-on-entry is reference semantics — the callee reads and writes the caller's memory directly. Reference semantics would be faster (no copy) but introduces three problems:

1. **Breaks the `mut` system**: A function receiving `arr: [i32; 3]` (not `mut`) could still mutate the caller's array through the pointer, violating the type system's guarantee.

2. **Breaks referential transparency**: If `f(arr)` can modify `arr`, then `f(arr); g(arr)` and `g(arr); f(arr)` may produce different results. Value semantics ensure function calls are independent.

3. **Complicates formal verification**: Rocq proofs with value parameters model function calls as substitution — the callee operates on a fresh copy. Mutable references require heap effect reasoning and aliasing analysis, dramatically increasing proof complexity.

Inference optimizes for provability, not performance. The copy cost is acceptable.

## Arrays in Non-Deterministic Blocks

Arrays work inside `forall`, `exists`, `assume`, and `unique` blocks. The frame layout scanner recurses into all block types to discover array variables, and prologue/epilogue emission handles them uniformly.

### Uzumaki Arrays

The uzumaki operator `@` can initialize an entire array with non-deterministic values:

```
pub fn array_uzumaki_init() {
    forall {
        let arr: [i32; 3] = @;
        let x: i32 = arr[0];
    }
}
```

This is lowered by emitting an `i32.uzumaki` (opcode `0xfc 0x31`) for each element and storing it at the corresponding offset:

```
;; Element-wise uzumaki for arr: [i32; 3] = @
local.get $__frame_ptr
i32.const <offset + 0>
i32.add
0xfc 0x31                 ;; i32.uzumaki — non-deterministic i32
i32.store

local.get $__frame_ptr
i32.const <offset + 4>
i32.add
0xfc 0x31                 ;; i32.uzumaki
i32.store

local.get $__frame_ptr
i32.const <offset + 8>
i32.add
0xfc 0x31                 ;; i32.uzumaki
i32.store
```

Each element gets an independent non-deterministic value. This means `forall { let arr: [i32; 3] = @; }` quantifies over all possible 3-element i32 arrays — the Cartesian product of all i32 values for each position.

Individual array elements can also be assigned uzumaki values:

```
pub fn array_assign_uzumaki_in_forall() {
    let mut arr: [i32; 2] = [0, 0];
    forall {
        arr[0] = @;
    }
}
```

This emits a single `i32.uzumaki` + `i32.store` at the computed index address.

## Comparison with Other Compilers

### LLVM / Clang to WASM

LLVM uses the same shadow stack pattern with a `__stack_pointer` mutable global. The convention is documented in the [WebAssembly tool conventions](https://github.com/WebAssembly/tool-conventions/blob/main/BasicCABI.md). Key differences:

- LLVM keeps `__stack_pointer` as an internal global (not exported by default). Inference exports it unconditionally, which is useful for test harnesses but exposes internal state. A future `BuildProfile` feature will make this conditional — export in Debug, hide in Release.
- LLVM uses `wasm-ld` to link objects and manage the stack. Inference generates complete modules directly via `wasm-encoder`, with no linker step.
- LLVM may use `memory.copy` and `memory.fill` via the bulk-memory proposal. Inference also uses both instructions.

### Rust to WASM

Rust arrays on WASM use the same LLVM shadow stack. `[i32; 3]` is allocated on the shadow stack with a frame pointer pattern identical to Inference's output. Rust passes small arrays by value (copying into the callee's frame) and large aggregates by reference with compiler-generated memcpy.

The key difference is that Rust has lifetime analysis and borrow checking. A `&[i32; 3]` reference can be passed without copying because the borrow checker guarantees no aliasing. Inference does not have a borrow checker, so it always copies.

### Zig to WASM

Zig uses a shadow stack for stack-allocated arrays when targeting WASM. Zig zero-initializes local arrays (matching Inference's `memory.fill` approach). Zig's safety modes add bounds checks on array access; Inference does not currently emit bounds checks (this is deferred to a future `BuildProfile` feature).

## WASM Section Layout

When `has_memory` is true, the compiler emits three additional sections in the WASM module:

| Section | Contents |
|---|---|
| Memory | 1 page minimum, 1 page maximum: `(memory 1 1)` |
| Global | `__stack_pointer`: mutable i32, init 65536: `(global (mut i32) i32.const 65536)` |
| Export | `"memory"` (memory 0), `"__stack_pointer"` (global 0) |

These sections are ordered according to the WASM specification: Type, Function, Memory, Global, Export, Code, Name. The ordering is mandatory — a misordered module fails validation.

When no function uses arrays, these sections are omitted entirely. The output is byte-identical to what the compiler produced before array support was added.

## Formal Verification Implications

### Modeling Arrays in Rocq

Arrays in linear memory map naturally to Rocq's `list` or `Vector.t n` types. The shadow stack's frame-scoped lifetime means each array has a clear birth (prologue) and death (epilogue), avoiding the need for heap allocation reasoning.

The copy-on-entry semantics for parameters are particularly beneficial for verification. In Rocq, a function `f(arr: [i32; 3])` can be modeled as taking a `Vector.t int32 3` argument by value. The proof does not need to reason about aliasing, mutation through shared pointers, or frame conditions on the caller's state. The callee's array is a fresh value, independent of the caller's.

### Zero-Initialization as a Proof Obligation

The unconditional zero-initialization via `memory.fill` in the prologue establishes a known precondition: every byte in the frame is zero before any user code executes. In Rocq, this can be encoded as:

```
forall (offset : nat), offset < frame_size -> load_byte (frame_ptr + offset) = 0
```

This precondition holds for free — the compiler guarantees it, so the Rocq proof can assume it without additional proof obligation on the programmer.

### Bounds Checking as a Future Proof Obligation

Currently, array index access is unchecked. `arr[i]` with an out-of-bounds `i` reads or writes arbitrary linear memory. This is a known limitation.

A future implementation, gated on `BuildProfile`, will add bounds checking in Debug mode (runtime trap on out-of-bounds access) and treat bounds as proof obligations in proof mode (the programmer must prove `0 <= i < array_length` before the access is allowed in the Rocq translation).

## Known Limitations

1. **No runtime bounds checking**: Out-of-bounds array access reads/writes arbitrary memory. Deferred to a future `BuildProfile`-gated feature.

2. **No stack overflow guard**: Deep call chains with large arrays can silently overflow the 64 KB stack. A prologue trap check (`if __stack_pointer < 0 then unreachable`) is planned.

3. **No nested arrays**: `[[i32; 3]; 2]` is not supported. Interior pointer management and aliasing concerns need design work.

4. **No array return values**: Returning an array from a function requires an ABI transformation (hidden first parameter for the return buffer, known as "sret"). This is a separate feature.

5. **No constant arrays**: `const ARR: [i32; 3] = [1, 2, 3]` is not handled and would panic at codegen time.

6. **Single memory page**: Linear memory is limited to 64 KB (`maximum: 1`). Sufficient for current use cases but will need expansion for heap allocation or large programs.

7. **Packed layout without alignment padding**: Arrays are packed sequentially in the frame without per-type alignment padding between arrays. WASM tolerates misaligned access but some runtimes may impose performance penalties.

## Current Implementation

The memory infrastructure is in `core/wasm-codegen/src/memory.rs` (constants, data structures, store/load helpers, prologue/epilogue emission, parameter copy). Frame layout computation and array lowering methods are in `core/wasm-codegen/src/compiler.rs`.

The implementation uses coverage marks to verify that each code path is exercised by the test suite:

| Coverage Mark | What It Tracks |
|---|---|
| `wasm_codegen_emit_memory_section` | Memory/Global section emission in `finish()` |
| `wasm_codegen_emit_stack_prologue` | Frame allocation at function entry |
| `wasm_codegen_emit_stack_epilogue` | Frame deallocation at all exit points |
| `wasm_codegen_emit_array_literal` | Element stores for array initialization |
| `wasm_codegen_emit_array_index_read` | Element load via base+offset |
| `wasm_codegen_emit_array_index_write` | Element store via base+offset |
| `wasm_codegen_emit_array_param_copy` | Copy-on-entry for array parameters |
| `wasm_codegen_emit_array_uzumaki` | Element-wise uzumaki stores |
