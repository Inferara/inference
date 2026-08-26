# Arrays and Linear Memory Lowering

## Overview

This document explains how Inference compiles fixed-size array types, struct types, and nested compound types to WebAssembly linear memory with a shadow stack (similar to Rust/LLVM).

Every whole-region memory operation this produces — frame zero-initialization, array and
struct parameter copies, sret returns, struct-to-struct assignment — is by default lowered
to plain loads and stores rather than the bulk-memory proposal's
`memory.fill`/`memory.copy`, so generated modules stay within the WebAssembly 1.0 (MVP)
instruction set. A build that opts into bulk memory gets the instructions instead. See
[Region Fill and Copy Lowering](#region-fill-and-copy-lowering).

Arrays are **stack-allocated** using a frame pointer and stack pointer mechanism. Each function that needs frame memory (a compound local, a written or extern-forwarded
compound parameter, or scratch space — a read-only compound parameter alone needs none):
1. Computes a frame layout at compile time
2. Emits a prologue to allocate the frame on entry
3. Reads/writes elements via load/store instructions
4. Emits an epilogue to deallocate the frame on exit

## Compilation Phases

### Phase 0: Type-Checking and Analysis (not in wasm-codegen)

The `core/type-checker` crate validates:
- Array lengths are positive compile-time constants
- Array variables, parameters, and literals have correct types

The `core/analysis` crate enforces codegen constraints:
- Multidimensional scalar arrays (`[[i32; 3]; 2]`) support full read/write access at any depth, literal initialization, uzumaki initialization, and parameter passing
- Struct-element arrays (`[Point; 3]`) are supported at one level of nesting
- Struct fields whose type is itself a compound type (another struct or an array) are rejected by rule A026 when the nesting would exceed one level (e.g., a struct field of type `[[i32; 3]; 2]` or a struct field of a struct that itself has compound fields)

### Phase 1: Frame Layout Computation

Before instruction emission, `compute_frame_layout()` walks the entire function body to:

1. Allocate space for array-typed **parameters** that need a private copy (see below)
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

**Key insight**: an array-typed parameter gets copy space in the callee's frame only when
something can write through it, which is what enforces **value semantics**. The caller always
passes a pointer. If the callee's body assigns through the parameter, or forwards it to an
`external fn` whose declaration marks a parameter `mut` — the statement that its foreign body
may store through the pointer — the callee copies the data into
its own frame slot so those writes cannot reach the caller's data. If neither is true, the
parameter is read where it lies and gets no slot at all — the copy would be unobservable, and
skipping it can leave the function with no frame whatsoever. `compound_param_is_by_reference()`
makes this decision, and it is evaluated here and nowhere else: the entry-copy loop emits a copy
exactly when a slot exists, so the two cannot disagree.

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

- **Array parameter copy** (on function entry, for parameters that were given a slot)
  - Copy the caller's data into the callee's frame
  - Optimizes small arrays (≤ 16 elements) as unrolled element copies
  - Larger arrays, and any array with compound (struct) elements, are copied as an
    untyped byte region — see [Region Fill and Copy Lowering](#region-fill-and-copy-lowering)

### Phase 3: Module Assembly

In `finish()`, if any function uses arrays (`self.has_memory == true`):

1. **Memory Section** - Declares the linear memory, with minimum and maximum both set to the configured page count (one 64 KB page by default), so the memory is fixed rather than growable
2. **Global Section** - Exports `__stack_pointer` (mutable i32 global)
   - Initialized to the configured stack size — `0x10000` (65536) by default, the top of the stack region
   - Stack grows downward toward address 0 (stack-first layout)

## Memory Layout

WebAssembly linear memory is a flat byte array accessed via load/store instructions. Its
size and the share of it given to the stack come from `MemoryLayout` (`target.rs`), whose
default is a single page that is entirely stack.

```
+------ pages * 65536 bytes  (default: 0x10000, one page)
|
|  [ Data region: future data sections, heap — empty by default ]
|
+-- stack size (default 65536) = __stack_pointer initial value
|
|  [ Stack grows downward ]
|
+------ 0x00000 (memory start)
  overflow below 0 = WASM OOB trap
```

This is the stack-first layout used by Rust and Zig: the stack occupies the bottom of the address space so that any overflow that pushes `__stack_pointer` below address 0 triggers a WASM out-of-bounds memory trap automatically. Future data sections will be placed above the stack region, starting at the stack size.

**Frame allocation**:

```
; Function entry (prologue)
global.get $__stack_pointer    ;; load current stack top
i32.const <frame_size>
i32.sub                        ;; decrement stack pointer
local.tee $__frame_ptr         ;; save new frame base, duplicate on stack
global.set $__stack_pointer    ;; update global
;; zero-initialize the entire frame: a straight-line sequence of i64.store 0
;; for small frames, or an index-driven loop for large ones — see
;; "Region Fill and Copy Lowering" below

; Function body: arrays are accessed via $__frame_ptr + offset

; Function exit (epilogue)
local.get $__frame_ptr
i32.const <frame_size>
i32.add                        ;; increment back up
global.set $__stack_pointer    ;; restore stack pointer
```

## Region Fill and Copy Lowering

Two operations move or clear an entire region of linear memory without caring about its
contents: zero-initializing a fresh stack frame in the prologue, and copying a compound
value (array or struct) between two addresses. Each is naturally a single bulk-memory
instruction (`memory.fill` / `memory.copy`), and by default each is lowered to plain loads
and stores instead, so that generated modules stay within the WebAssembly 1.0 (MVP)
instruction set — no post-MVP proposal is required to run Inference-compiled code.

### Opting Back In

A build that permits bulk memory — `EmitFeatures { bulk_memory: true }` on `codegen()`,
which `infc --wasm-features bulk-memory` sets — emits the single instruction at each site
instead of its lowering. The choice is made at the first statement of
`emit_frame_zero_fill`, `emit_memcpy_via_locals` and `emit_memcpy_via_stack`, ahead of
every early return and every scratch allocation; nothing upstream of those three
functions differs, so frame layouts and the analysis rules that reason about them
(A035, A036) are unaffected. The bulk form allocates no scratch local, which `RegionEmit`
asserts, and the resulting bytes reproduce what the compiler emitted before this lowering
existed. The feature applies identically in Compile and Proof mode, so the `.v` always
describes the same program as the `.wasm`.

### Two Shapes: Straight-Line and Looped

Both the frame zero-fill (`emit_frame_zero_fill`) and the region copy
(`emit_memcpy_via_locals`) pick one of two shapes based on the region's byte size
against `BULK_UNROLL_LIMIT_BYTES` (128 bytes — sixteen 8-byte stores):

- **Straight-line** (`byte_size <= 128`): the region is covered by a fixed sequence of
  stores (fill) or load/store pairs (copy) with no branch. A copy decomposes the region
  into whole 8-byte units followed by at most one 4-, 2-, and 1-byte unit
  (`CopyWidth::DESCENDING`), so any byte count is covered exactly. Every access folds its
  constant displacement into the instruction's `offset` immediate, so an unrolled copy
  emits no `i32.add` at all.
- **Looped** (`byte_size > 128`): a `loop` with an induction variable and a `br_if`
  back-edge (no enclosing `block` — the loop exits by falling through) covers the
  bulk of the region (16 bytes per iteration for a fill, 8 bytes for a copy), followed by
  the same straight-line tail for the remainder. Frame sizes are always multiples of
  `FRAME_ALIGNMENT` (16), so the fill loop never needs a tail.

This threshold is unrelated to `UNROLL_THRESHOLD` (16), which gates a different
decision — whether an *array parameter copy* is unrolled element-by-element with the
element's own typed load/store, or handed to the region copy as untyped bytes.
`UNROLL_THRESHOLD` counts elements; `BULK_UNROLL_LIMIT_BYTES` counts bytes and picks the
shape of an untyped copy, including the untyped copy that a large or compound-element
array falls back to.

### `MemAddr` and `CopyWidth`

`emit_memcpy_via_locals(func, dst, src, byte_size, scratch)` takes both endpoints as a
`MemAddr { local, offset }` — a WASM local holding a base pointer plus a constant byte
displacement — rather than four loose `u32` parameters, so a call site cannot pair one
endpoint's local with the other's displacement. `CopyWidth` enumerates the four unit
widths (`I64`, `I32`, `I16`, `I8`) with their load/store instruction and byte count;
narrow loads are zero-extending (`i32.load16_u`, `i32.load8_u`) because a copy
reproduces raw bytes, not a sign-extended value. Every copy memory access uses a
one-byte alignment hint (`copy_memarg`), since a copy's base may be a struct field or
array element aligned to less than the natural alignment that `store_instruction` /
`load_instruction` assume for typed accesses.

### `RegionEmit`: Lazily Allocated Locals

The loop and copy shapes need scratch i32 locals — an induction variable, and (for
`emit_memcpy_via_stack`, the entry point used by every body-level compound copy) a
destination and source base local to hold the two addresses popped off the WASM
operand stack. These are **not** part of the eagerly computed declaration list built by
`pre_scan_locals` and the frame-pointer/bounds-check/narrow-division reservations,
because whether — and how many times — a function needs them is a property of the
instructions emitted while lowering its body, not something that can be predicted ahead
of emission without duplicating the emission logic.

`RegionEmit` hands out indices for `dst`, `src`, and `counter` in first-use order, starting
one past the last eagerly reserved local. A function that never emits a loop or a
body-level compound copy allocates nothing, and a build that permits bulk memory never
allocates at all. To make this possible, the compiler
builds the function body into a `wasm_encoder::Function` created with an *empty* locals
vector (`Function::new([])`) instead of the final declarations, and only once the body
is fully emitted does `Compiler::take_completed_function` splice the complete
declaration list — the named locals followed by one `(1, ValType::I32)` entry per
allocated scratch slot — onto the front of the already-encoded raw body bytes. A
function that allocates no scratch is byte-identical to one built with the final
declarations from the start. See
[docs/local-variables-lowering.md](local-variables-lowering.md#local-declarations-are-finalized-after-the-body-is-built)
for how this interacts with the local pre-scan.

The state, like the function body itself, is rebuilt at the start of every function
(`RegionEmit::new(first_free_index, features)`), and its slots are shared across every copy and
loop emitted in that function — safe because each loop or copy is emitted atomically
(see below), so none is ever live across another.

### Loop Emission Is Atomic

Both the fill loop and the copy loop are emitted as a single, self-contained unit — no
user expression is lowered inside them. This means they cannot contain a user `break`
or `continue`, so they need no entry in the compiler's `LoopContext` / block-depth
bookkeeping (which exists to route those statements to the right structured-control-flow
target); see [docs/loops-lowering.md](loops-lowering.md) for that bookkeeping.

### Overlap and the Forward-Copy Argument

`memory.copy` guarantees `memmove`-style correctness for overlapping regions; a plain
forward loop of loads and stores does not, in general. `emit_memcpy_via_locals` is a
forward copy — safe only because every call site reaches it with either disjoint
regions or two addresses that are *identical*, never partially overlapping:

- Array/struct parameter copies read the caller's argument and write the callee's
  freshly decremented frame — disjoint, or identical when a callee returns its own
  parameter through the caller's slot.
- The sret return copy writes the caller-provided destination from the callee frame —
  disjoint for the same reason, identical when a method returns the value it was called
  on.
- Body-level compound copies (via `emit_memcpy_via_stack`) move between whole named
  slots, bounds-checked array elements, or layout-disjoint struct fields; a
  self-referential reassignment is routed through the frame's scratch region first, so a
  copy never reads a slot it is concurrently writing.

A parameter passed by reference does alias the caller's region, and two of them can name
the same region or one a sub-range of the other (`f(x, x)`, `f(s, s.f)`). That never
reaches this helper as *partial* overlap. A parameter is passed by reference precisely
because nothing writes it, so it is never a copy destination; and naming a strict
sub-range of a region takes a projection, which appears only as a source here. What is
left is identical endpoints, where each byte's read and write coincide — which a forward
copy handles.

Inference has value semantics and no references, so two distinct compound values can
never partially alias — the case a forward copy would get wrong never arises.

### Why the Overflow Trap Still Holds

The stack-first layout's "free trap" (see [Memory Layout](#memory-layout) above) relied
on `memory.fill`'s up-front bounds check: a wrapped, out-of-range frame pointer failed
that check before any byte was written. The replacement fill has no single up-front
check, so the ordering of its own stores has to reproduce the same property. Both the
unrolled and looped fill shapes emit the store at frame offset 0 first — WebAssembly
computes `base + offset` without 32-bit wraparound, so a frame pointer that wrapped past
the end of memory fails that first store's bounds check and traps before
writing anything, exactly as `memory.fill` did.

This is defense in depth, not the primary safeguard: analysis rules A035 (recursion) and
A036 (stack depth) statically reject any program whose frames could exhaust the stack,
and `compute_frame_layout` independently asserts a single frame fits within
the configured stack size, so an accepted program never reaches the wrapping case at all.

## Implementation Details

### `memory.rs` Module

This module contains all memory-related helpers:

| Function/Type | Purpose |
|---|---|
| `FrameLayout` | Data structure: `total_size`, `array_offsets`, `struct_offsets`, `frame_ptr_local` |
| `ArraySlot` | Per-array metadata: `offset`, `elem_size`, `length`, `element_layout` (optional inner struct layout for struct-element arrays) |
| `StructSlot` | Per-struct metadata: `offset`, `total_size`, `fields` (Vec of `StructFieldSlot`) |
| `StructFieldSlot` | Per-field metadata: `name`, `offset` (from struct base), `type_kind`, `layout` (`CompoundFieldLayout`) |
| `CompoundFieldLayout` | Enum describing a struct field's compound layout: `Scalar` (primitive), `NestedStruct { fields, total_size }`, or `NestedArray { elem_kind, elem_size, length }` |
| `compute_struct_field_layout()` | Compute C-compatible field offsets and total size for a struct; now takes `&TypedContext` (for nested struct lookup) and returns `Result` |
| `type_byte_size()` | Map `TypeInfoKind` → total byte size, including structs and arrays; requires `&TypedContext` for struct size lookup |
| `natural_alignment_for_type()` | Return the natural alignment (in bytes) of a type; uses the maximum field alignment for structs |
| `element_size()` | Map scalar `TypeInfoKind` → byte size (1, 2, 4, or 8); does not handle structs |
| `align_to_frame()` | Round up to 16-byte boundary |
| `emit_ptr_offset_addr()` | Emit `local.get $ptr; i32.const offset; i32.add` for a base-pointer + byte-offset address |
| `store_instruction()` | Select `i32.store8`, `i32.store16`, `i32.store`, or `i64.store` |
| `load_instruction()` | Select appropriate load (sign/zero-extending as needed) |
| `emit_stack_prologue()` | Generate frame allocation code, including the zero-fill (delegates to `emit_frame_zero_fill()`) |
| `emit_stack_epilogue()` | Generate frame deallocation code |
| `emit_array_param_copy()` | Copy caller's array data into callee's frame |
| `emit_struct_param_copy()` | Copy caller's struct data into callee's frame via a region copy |
| `RegionEmit` | The permitted WebAssembly features plus the lazily allocated i32 scratch locals (`dst`, `src`, `counter`) the lowered forms need, numbered from the first free local after the eager declarations |
| `emit_frame_zero_fill()` | Zero-fills a frame: one `memory.fill` when the build permits bulk memory, otherwise straight-line `i64.store`s below `BULK_UNROLL_LIMIT_BYTES` and an index loop above it |
| `emit_memcpy_via_locals()` | Copies `byte_size` bytes between two `MemAddr` endpoints: one `memory.copy` when the build permits bulk memory, otherwise straight-line loads/stores below `BULK_UNROLL_LIMIT_BYTES` and a loop with a static tail above it |
| `emit_memcpy_via_stack()` | The entry point for every body-level compound copy, taking its destination and source from the WASM operand stack: appends the size and one `memory.copy` when the build permits bulk memory, otherwise pops both addresses into scratch locals and calls `emit_memcpy_via_locals()` |
| `MemAddr` | A copy endpoint: a WASM local holding a base pointer plus a constant byte displacement |
| `CopyWidth` | The four copy unit widths (`I64`, `I32`, `I16`, `I8`) with their load/store instruction and byte count |

### `compiler.rs` Additions

#### Local Registration for Arrays

During the `pre_scan_locals()` phase (which walks all statements before instruction emission), array variables are registered as **i32 WASM locals**, identical to any other scalar variable. The type-checker sets `TypeInfoKind::Array(...)` on the variable node, and `pre_scan_locals` treats it as a non-i64 type and assigns an `i32` local.

Later, during instruction emission, when an array literal is initialized, `lower_array_literal()` stores array elements in linear memory and pushes the frame pointer (pointer to the array data) onto the WASM stack, which is then assigned to the local via `local.set`.

For struct-element arrays the `else` branch delegates to `lower_array_literal_struct_elements`. For scalar-element arrays — including multi-dimensional ones (`[[i32; 3]; 2]`) — it delegates to `store_array_literal_elements`, which recurses on the declared array type (derived from the literal's `expr_id` type info) and stores each scalar leaf at `slot_offset + Σ idxᵢ · strideᵢ`, mirroring `emit_array_uzumaki_recursive`. A nested array element that is itself a literal recurses; a non-literal array element (an identifier or call, e.g. `let g = [r, r];`) is copied with a region copy for its full sub-array byte size (see [Region Fill and Copy Lowering](#region-fill-and-copy-lowering)). Single-dimensional scalar arrays only reach the scalar leaf path and emit byte-identical output to before.

Nested **array-of-structs** literals (`let g: [[Pt; 2]; 2] = [[Pt{..}, Pt{..}], [..]]`) are also handled by `store_array_literal_elements`: the outer array's element is itself an array, so the `Array` arm recurses until it reaches a struct leaf. The struct leaf reuses the single-dimensional AoS machinery — its field layout is computed once via `compute_struct_field_layout`, then each element writes its fields at `base + i·stride + field_offset` through `lower_struct_literal_fields` (for a `StructLiteral` element) or a full-struct region copy (for a non-literal element such as `let p = Pt{..}; let g = [[p, p], [p, p]];`). An **enum** leaf (`[[Color; 2]; 2]`) is scalar-sized, so `lookup_struct` returns `None` and the element falls through to the scalar leaf path. Single-dimensional AoS (`[Pt; 3]`) never enters this helper — it takes the `lower_array_literal_struct_elements` branch in `lower_array_literal` — so its output is unaffected.

#### `compute_frame_layout()`

```rust
fn compute_frame_layout(input: &FrameLayoutInput<'_>) -> Result<Option<FrameLayout>, CodegenError>
```

`FrameLayoutInput` carries the arena, the body block, the `TypedContext`, the module path, the
`__frame_ptr` local index, the argument list, the enclosing struct name when compiling a method
body, and the `ExternCallScope` (`extern_scope`) the escape gate consults to decide which
`external fn` a bare callee name denotes here.

Returns `None` when nothing needs memory — no frame, and therefore no `__frame_ptr`, no
prologue, no epilogue and no `__stack_pointer` mutation. That now includes a function whose only
compound values are parameters passed by reference. The struct name should be `Some("TypeName")`
when compiling a method body, so a receiver that needs a slot can look up the struct layout.

**Algorithm**:
1. Iterate parameters: allocate copy space for each compound parameter that is not passed by
   reference
2. Recursively walk block statements, collecting array variables
3. Sum byte sizes and align to 16 bytes
4. Return `FrameLayout` or `None`

**Important**: This phase allocates a **synthetic `__frame_ptr` local** (separate from array variable locals) to hold the frame base address. This local is used internally for all frame addressing and is not visible to source code.

#### `lower_array_index_access()`

```rust
fn lower_array_index_access(
    &mut self,
    arena: &AstArena,
    aiae_expr_id: ExprId,
    array_expr_id: ExprId,
    index_expr_id: ExprId,
    ctx: &TypedContext,
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
    &mut self,
    arena: &AstArena,
    aiae_expr_id: ExprId,
    array_expr_id: ExprId,
    index_expr_id: ExprId,
    right_expr_id: ExprId,
    ctx: &TypedContext,
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
    &mut self,
    _arena: &AstArena,
    elem_type: &TypeInfo,
    length: u32,
    enclosing_var_name: &str,
) -> Result<(), CodegenError>
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
- The variable name is threaded explicitly from the caller (no parent chain walking)
- `compute_frame_layout()` pre-computes all array offsets
- Result: all elements of the array hold non-deterministic values

**Domain constraint for `bool`/enum elements**: a `bool`- or enum-typed leaf
element gets an `and 1` / `rem_u <variant count>` constraint between the draw
and the store (`emit_compound_uzumaki_domain_constraint`, inserted where the
example above shows `i32.store`). A narrow-int leaf (`i8`/`u8`/`i16`/`u16`)
needs no such constraint: its `store8`/`store16` truncation plus the
sign/zero-extending typed load used to read it back already realize the
domain on every round trip through memory.

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

Each array within a frame is aligned to its element type's natural alignment, matching the LLVM/Rust/BasicCABI convention. For example, a `[bool; 3]` array (1-byte elements) followed by a `[i32; 2]` array (4-byte elements) will have 1 byte of padding inserted so the i32 array starts at offset 4 (a 4-byte boundary). This makes `MemArg` alignment hints truthful and enables better hardware optimization on runtimes that use alignment hints for instruction selection (e.g., SSE-aligned loads on x86). Padding bytes are automatically zeroed by the prologue's zero-fill (see [Region Fill and Copy Lowering](#region-fill-and-copy-lowering)).

Example:

```
Arrays:  [bool; 3] (3 bytes) + 1 byte padding + [i32; 2] (8 bytes) = 12 bytes
Aligned: (12 + 15) & ~15 = 16 bytes
```

## Parameter Passing: By Reference or Copy-on-Entry

**Caller**: always passes a pointer (i32) to the array data in linear memory.

**Callee**: what it does with that pointer depends on whether anything can write through it.
`compound_param_is_by_reference()` decides, from the callee's own body:

- The parameter is **copied on entry** if the body assigns through it (`arr[0] = 9`, `p.x = 9`,
  `p = @`, whole-binding reassignment — `Stmt::Assign` is the language's only write statement),
  or if it reaches an argument of an `external fn` **whose declaration marks any parameter
  `mut`**. Such a foreign body shares the same linear memory and has declared that it may store
  through the pointer it is handed.
- Otherwise it is **passed by reference**: no frame slot, no copy, and reads go straight to the
  caller's memory.

The declaration is what makes the second clause decidable. `mut` on an `external fn` parameter
declares that the foreign body may store through the address that parameter denotes, and the
linker checks the merged bytes against that declaration; a declaration marking nothing `mut`
declares the empty write set, and the link then requires the merged body to record no store at
all. Reaching such an external therefore cannot disturb anything, and the copy is dropped.

The classification is whole-declaration, not per-argument-position. The link proves which
declared parameter a store's *address derives from*, never which bytes it reaches, so a body
admitted for `writer(mut a, b)` may store outside `a`'s region — and narrowing the gate to the
`mut` positions would hand `b`'s caller storage to a body free to land in it. Lifting that needs
a containment analysis the linker does not have (#420).

The copy path is:
1. Allocate space in the frame (computed by `compute_frame_layout()`)
2. Copy the caller's data element-by-element or via a region copy
3. Update the parameter local to point to the copy
4. All subsequent reads/writes operate on the local copy

Step 3 is what makes the rest of the body lowering identical either way: a field or element
address is computed from the parameter local the same way on both paths, and only the
local's value differs between them.

**Benefit**: mutations inside the callee don't affect the caller's array (value semantics).
Where no mutation is possible the copy is unobservable, so eliding it preserves the same
guarantee — and when nothing else in the function needs memory, it removes the frame,
the prologue, the epilogue and the `__stack_pointer` mutation as well.

**Note**: the `mut` marker is not consulted. It is a type-checker contract about what the body
is permitted to do; the copy decision is about what the body actually does. A `mut` parameter
that is never assigned is passed by reference.

**Optimization**: Arrays with ≤ 16 elements are copied element-by-element with the element's own typed load/store, avoiding the untyped region copy's overhead. Larger arrays, and any array with compound elements, use the region copy described in [Region Fill and Copy Lowering](#region-fill-and-copy-lowering).

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

## Runtime Bounds Checking

A dynamic (runtime-variable) index can address memory outside the array's bounds and silently corrupt adjacent frame slots. Codegen emits a guard before the offset multiply so an out-of-range access traps cleanly instead.

"Constant" here means exactly what `try_const_index_byte_offset` folds: a **direct number literal**, and nothing else. `arr[3]` folds to a static offset and is not guarded; `arr[K]` for a `const K`, and `arr[1 + 1]`, do not fold and take the guarded dynamic path. The static half — analysis rule `A037` (see `core/analysis`) — matches the same narrow shape, a literal directly under the access, so the two halves cover every index between them, but a named or computed constant lands in the *dynamic* half rather than the static one.

The guard is emitted for **every build**: Compile and Proof mode, Debug and Release, Wasm32 and Soroban. The `codegen()` entry point sets `Compiler::emit_bounds_checks` unconditionally, so the deployed artifact is always checked and the artifact a proof is written about is byte-for-byte the artifact that ships. `OptLevel` does not influence bounds checks.

Proof mode was the one unguarded path until the obligation side of it existed. A guard is what gives the emitted `.v` a trap site to be *about*: an `HA_app_ok`/`T_app` realization claim over a function containing one forces `index <u length` there, because realization runs through `interp_realized` to `fun_computes`, a total-correctness judgment no trapping reduction satisfies (`core/wasm-to-v/ROCQ_CONTRACT.md`, "Trap-freedom"). Emitting the guard in one mode and not the other would have meant proving a program that is not the one that ships.

The `emit_bounds_checks` flag survives the flip rather than being folded away, though not as a hook for #215: that elision is decided per *access*, inside `emit_index_offset`, and leaves the flag `true` for every module. `emit_index_offset` is the seam it would hook into — one place every guarded access passes through. What keeps the flag is that `Compiler` is a library type whose in-crate tests exercise both settings.

For Case 3 (`arr[i]`) the guard is inserted between the index push and the offset multiply. The index is single-evaluated into a scratch i32 local via `local.tee`, so an index expression with side effects runs exactly once. The scratch is reserved per function **iff the body actually contains a dynamic array index** (`body_has_dynamic_array_index`), independent of whether the function has a frame. That predicate and the emission site classify an index through one shared call to `try_const_index_byte_offset`: an index is dynamic exactly when its byte offset does not fold, so a guard can never be emitted into a function that reserved no scratch. Restating the condition — "the index is not a `NumberLiteral`" — is what the two used to do, and they disagreed on a literal whose `index * elem_size` overflows. This keeps constant-index-only functions byte-identical to an unchecked build, and reserves the scratch even for an immutable-`self` method like `self.arr[idx]` that needs no frame slot. The scratch sits at the next free local after the named locals and the optional frame-pointer temp:

```wasm
<lower array expr>      ;; push base pointer
<lower index expr>      ;; push i32 index
local.tee   $scratch    ;; [base, index]; $scratch = index
local.get   $scratch    ;; [base, index, index]
i32.const   <length>
i32.ge_u                ;; index >= length ?  (unsigned: also traps negatives, which arrive as huge u32)
if (empty)
  unreachable           ;; trap on out-of-bounds
end                     ;; [base, index]
i32.const   <elem_size>
i32.mul
i32.add                 ;; address = base + index * elem_size
i32.load / i64.load / ...
```

The empty-result `if` consumes only the comparison result and leaves `base` and `index` on the stack, so the offset computation proceeds unchanged. The `unreachable` trap reuses the `assert` lowering idiom and maps to `BI_unreachable` in the Rocq translator, so guarded code remains translatable. Both the read path (`lower_array_index_access`) and the write path (`lower_array_index_write`) share the single `emit_index_offset` choke point, so reads and writes are guarded identically.

## Zero-Store Elision During Initialization

The function prologue zero-initializes the entire stack frame before any instructions run (see [Region Fill and Copy Lowering](#region-fill-and-copy-lowering)). This means that every byte of the frame is already zero at the point where the first `let` or `const` initializer executes. Any store of a zero value into that freshly-zeroed memory is therefore redundant.

### The Optimization

During variable initialization (inside a `Stmt::VarDef` handler), the compiler sets the `init_zero_elision` flag on the `Compiler` struct to `true` before calling the expression lowering path, and resets it to `false` immediately after. While this flag is set:

- **Scalar array elements** — if `is_syntactic_zero(element_expr)` returns `true`, the element's store is skipped entirely. For multi-dimensional scalar arrays this check runs at each leaf inside `store_array_literal_elements`, so an inner literal such as `[[0, 7], [0, 0]]` stores only the single non-zero leaf.
- **Struct scalar fields** — if `is_syntactic_zero(field_value_expr)` returns `true`, the field's store is skipped.
- **Struct nested-array field elements** — each element is checked individually; zero elements are skipped.

The flag is threaded into recursive helpers as the `skip_zero_stores: bool` parameter on `lower_struct_literal_fields`, `store_array_literal_elements` (the scalar-element array path in `lower_array_literal`), and the struct-element array path in `lower_array_literal`.

### Recognized Zero Patterns

`is_syntactic_zero` recognizes the following syntactic forms as producing a zero value:

| Expression | Reason |
|---|---|
| `NumberLiteral { value: "0" }` | Literal zero |
| `NumberLiteral { value: "-0" }` | Negative-zero literal (same bit pattern) |
| `BoolLiteral { value: false }` | Stored as `i32` 0 |
| `Parenthesized(e)` where `e` is zero | Transparent wrapper |
| `PrefixUnary { op: Neg, expr: e }` where `e` is zero | `-(0)` == 0 |

This is a conservative check: only false negatives are possible (e.g., `0x0` or `0_0` are not recognized and will emit a redundant store). False positives — incorrectly skipping a non-zero store — cannot occur.

### Why Initialization Only

The optimization applies exclusively to `VarDef` (`let`/`const`) initialization, never to assignment (`x = value;`). During assignment, the destination slot may hold non-zero data from a prior operation. Emitting no store would leave stale data in memory — a correctness bug. The `init_zero_elision` flag is never set during the assignment path, which guarantees that `lower_struct_literal_fields` with `skip_zero_stores = true` is only called when the destination is a freshly-zeroed frame slot.

As a defense-in-depth assertion, both `lower_struct_literal_fields` and the struct-element array path assert that `skip_zero_stores` is only `true` when `frame_ptr_local` equals the function's own `frame_layout.frame_ptr_local`. This catches any future caller that incorrectly passes a non-frame pointer with elision enabled.

The sret return path (`lower_array_sret_return`, `lower_struct_literal` in sret context) always passes `skip_zero_stores = false` because the destination is the caller's frame, not the callee's zero-filled frame.

### Effect on Code Size

The elision eliminates a cluster of instructions for each zero element:

```wasm
;; Without elision (arr[0] = 0, frame already zero):
local.get $__frame_ptr
i32.const 0
i32.add
i32.const 0
i32.store          ;; redundant — frame was already 0

;; With elision: no instructions emitted for this element
```

For a `[i32; 8]` array initialized to all zeros, eight 4-instruction sequences (32 instructions total) are eliminated. For struct literals with many zero fields the savings scale proportionally.

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

Copies the source array's data to the sret destination via `emit_sret_copy`, which
delegates to the region copy described in [Region Fill and Copy Lowering](#region-fill-and-copy-lowering)
(straight-line loads/stores for this 12-byte example):

```wasm
;; conceptually: emit_memcpy_via_locals(dst = $sret, src = $arr, byte_size = 12)
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

### `StructReturnInfo` and `func_struct_returns`

```rust
struct StructReturnInfo {
    total_size: u32,
    field_slots: Vec<StructFieldSlot>,
}
```

`func_struct_returns: FxHashMap<String, StructReturnInfo>` is populated in parallel with `func_array_returns` during the same pre-scan phase. When the return type is a `Custom` name that resolves to a struct in the symbol table, `compute_struct_field_layout` is called and the result cached here. Both callers and callees use this map to emit correct sret code.

## Struct Layout and Field Access

Structs in Inference are stack-allocated in the same shadow stack frame as arrays. Each struct variable gets a `StructSlot` entry in `FrameLayout::struct_offsets`.

### Field Layout

`compute_struct_field_layout` visits fields in declaration order and assigns each field a byte offset aligned to its natural alignment (matching C `repr(C)` rules). It accepts a `&TypedContext` so that nested struct fields can be recursively laid out. The function now returns `Result<(u32, Vec<StructFieldSlot>), CodegenError>` — errors occur only for recursive struct definitions (defense-in-depth; the type checker's `RecursiveStructDefinition` error prevents them) or missing type-context entries.

```
struct Point { x: i32; y: i32; }   →  total_size=8
  field x: offset=0, size=4
  field y: offset=4, size=4

struct Mixed { flag: bool; val: i64; }  →  total_size=16
  field flag:  offset=0,  size=1
  (7 bytes padding)
  field val:   offset=8,  size=8
```

The total size is rounded up to the struct's maximum field alignment (e.g., `Mixed` above aligns to 8 because `i64` requires 8-byte alignment).

### Struct Literal Lowering (`lower_struct_literal`)

A struct literal (`Point { x: 10, y: 20 }`) is lowered by storing each field at `frame_ptr + struct_offset + field_offset`, then pushing the struct base pointer onto the WASM stack:

```wasm
local.get $__frame_ptr
i32.const <struct_offset + field_x_offset>
i32.add
i32.const 10
i32.store                    ;; p.x = 10

local.get $__frame_ptr
i32.const <struct_offset + field_y_offset>
i32.add
i32.const 20
i32.store                    ;; p.y = 20

local.get $__frame_ptr       ;; push struct pointer
i32.const <struct_offset>
i32.add
local.set $p
```

### Member Access Read (`lower_member_access`)

For scalar fields, reading `p.x` loads the field value from the struct's memory location:

```wasm
local.get $p               ;; struct base pointer
i32.const <field_offset>   ;; omitted when offset is 0
i32.add
i32.load                   ;; or i64.load, i32.load8_u, etc.
```

For compound fields (nested structs and array-typed fields), the read omits the load instruction and instead leaves a pointer to the field's memory location on the WASM stack:

```wasm
local.get $p               ;; struct base pointer
i32.const <field_offset>   ;; omitted when offset is 0
i32.add
                           ;; no load — result is an i32 pointer to the compound field
```

This pointer semantics enables chaining: `outer.inner.x` lowers as two member-access address computations followed by a single scalar load for `x`. Similarly, `s.arr[1]` uses the pointer as the base for the array index calculation.

`resolve_struct_field_offset` now returns a `ResolvedField` struct containing the offset, type kind, and `CompoundFieldLayout` so that callers can decide whether to emit a load or leave a pointer. The function first checks `frame_layout.struct_offsets` for O(1) lookup when the struct expression is a simple identifier. It falls back to recomputing via `compute_struct_field_layout` for parameters or complex expressions.

### Member Access Write (`lower_member_access_write`)

For scalar fields, writing `p.x = v` stores the RHS value at the same address:

```wasm
local.get $p               ;; struct base pointer
i32.const <field_offset>
i32.add
<lower RHS expression>
i32.store
```

For compound fields (nested structs or array-typed fields), the write emits a region copy from the RHS pointer to the destination field address:

```wasm
local.get $p               ;; struct base pointer (destination)
i32.const <field_offset>
i32.add
<lower RHS expression>     ;; RHS is a pointer to compound data (source)
;; region copy of <compound_size> bytes — see Region Fill and Copy Lowering
```

The total byte size (`compound_size`) comes from `CompoundFieldLayout::byte_size()`: `NestedStruct.total_size` for nested structs, and `elem_size * length` for nested arrays.

### Struct Parameter Copy (`emit_struct_param_copy`)

Struct-typed parameters arrive as i32 pointers (the caller's copy). When the parameter was given a frame slot — see [Parameter Passing](#parameter-passing-by-reference-or-copy-on-entry) — the callee copies the data into that slot with a region copy (`emit_memcpy_via_locals`, see [Region Fill and Copy Lowering](#region-fill-and-copy-lowering)), then updates the parameter local to point to the callee's copy:

```wasm
;; region copy: destination = frame_ptr + slot_offset, source = $param, size = total_size

local.get $__frame_ptr
i32.const <slot_offset>
i32.add
local.set $param           ;; update param to point to callee's copy
```

This enforces value semantics: mutations inside the callee do not affect the caller's struct. A struct parameter with no slot skips all of the above and is read through the caller's pointer.

### Struct-to-Struct Copy (`lower_struct_copy_var_init`)

When a struct variable is initialized from another struct identifier (`let b = a;`), a region copy moves the source's data into the destination's frame slot:

```wasm
;; region copy: destination = frame_ptr + dest_offset, source = $a, size = total_size

local.get $__frame_ptr
i32.const <dest_offset>
i32.add
local.set $b               ;; b now points to its own copy
```

### Struct Uzumaki (`lower_struct_uzumaki`)

A struct variable initialized with uzumaki (`let p: Point = @;`) is filled field-by-field with non-deterministic values. For each field in the struct layout, the compiler emits the appropriate uzumaki opcode followed by a store at the field's memory offset:

```wasm
local.get $__frame_ptr
i32.const <struct_offset + field_x_offset>
i32.add
i32.uzumaki                ;; 0xfc 0x31 — non-deterministic i32 value
i32.store                  ;; p.x = @

local.get $__frame_ptr
i32.const <struct_offset + field_y_offset>
i32.add
i32.uzumaki
i32.store                  ;; p.y = @

local.get $__frame_ptr
i32.const <struct_offset>
i32.add
local.set $p
```

Fields with `i64` types emit `i64.uzumaki` (0xfc 0x32) followed by `i64.store`. Field types use the same type dispatch as regular struct literal stores.

A `bool`- or enum-typed field gets a domain-constraint sequence between the uzumaki draw and the store — `and 1` for `bool`, `rem_u <variant count>` for a non-empty enum (an empty enum is left unconstrained, since it is uninhabited and `rem_u 0` would trap). Narrow-int fields (`i8`/`u8`/`i16`/`u16`) need no such constraint: the field's `store8`/`store16` truncation and its sign/zero-extending typed load already realize the domain on every round trip through memory. See `emit_compound_uzumaki_domain_constraint` in `compiler.rs`.

### Struct Uzumaki with Array Fields (`lower_struct_uzumaki`)

When a struct has array-typed fields (e.g., `struct HasArray { arr: [i32; 3]; val: i32; }`), uzumaki initialization stores non-deterministic values element-by-element for each array field. For each element of the array field, the compiler emits the appropriate uzumaki opcode and a store at the element's computed address within the field:

```wasm
;; For `let h: HasArray = @;` inside a forall block:
;; field arr (offset=0, [i32; 3]):
local.get $__frame_ptr
i32.const <struct_offset + 0>   ;; arr[0] byte offset
i32.add
i32.uzumaki
i32.store

local.get $__frame_ptr
i32.const <struct_offset + 4>   ;; arr[1] byte offset
i32.add
i32.uzumaki
i32.store

;; ... arr[2] ...

;; field val (scalar, offset=12):
local.get $__frame_ptr
i32.const <struct_offset + 12>
i32.add
i32.uzumaki
i32.store
```

The total element count across all fields is bounded by `MAX_UZUMAKI_UNROLL_ELEMENTS` (65 536). If a struct's fields collectively exceed this limit, `CodegenError::ArrayTooLargeForUzumaki` is returned.

As with a scalar struct field, a `bool`- or enum-typed array element gets its
`and 1` / `rem_u <variant count>` domain constraint between the draw and the
store; narrow-int elements need none, for the same store-truncation reason.

## Compound Field Layout

The `CompoundFieldLayout` enum describes how a struct field's memory should be handled during initialization and access. It is stored in each `StructFieldSlot` and is computed by `compute_struct_field_layout`:

```rust
pub(crate) enum CompoundFieldLayout {
    Scalar,                                      // scalar field — load/store directly
    NestedStruct { fields: Vec<StructFieldSlot>, total_size: u32 }, // another struct
    NestedArray  { elem_kind: TypeInfoKind, elem_size: u32, length: u32 }, // array field
}
```

`CompoundFieldLayout::is_compound()` returns true for `NestedStruct` and `NestedArray`. `byte_size()` returns the total byte count for compound variants.

**Why this matters**: During `lower_struct_literal_fields`, the dispatch on `CompoundFieldLayout` determines:
- `Scalar`: emit a single store instruction.
- `NestedStruct`: if the RHS is a struct literal, recurse into `lower_struct_literal_fields` for the inner fields; otherwise emit a region copy.
- `NestedArray`: if the RHS is an array literal, emit element-by-element stores; otherwise emit a region copy.

The same layout is consulted during member access read/write to decide whether to emit a scalar load or leave a pointer on the stack.

## Arrays of Structs

An array whose element type is a struct (e.g., `[Point; 3]`) is laid out in the shadow-stack frame with each element occupying `struct_total_size` bytes. The `ArraySlot` for such an array stores an `element_layout: Some(Vec<StructFieldSlot>)` so that element-level field accesses can resolve offsets without recomputing the struct layout on every read.

### Initialization

An array-of-structs literal (e.g., `[Point{x:1,y:2}, Point{x:3,y:4}]`) initializes each element in order. For each element:
- If the element is a struct literal, `lower_struct_literal_fields` is called at `base + index * elem_size`.
- If the element is a struct identifier, a region copy of `elem_size` bytes copies the source into position.

### Element Field Access (`pts[1].x`)

Reading a field from an indexed element compiles as:

```wasm
local.get $pts              ;; array base pointer
i32.const <1 * elem_size>   ;; byte offset of element 1
i32.add                     ;; pointer to element 1 (a struct pointer)
i32.const <field_offset>    ;; field x offset within the struct
i32.add
i32.load                    ;; load scalar field value
```

Writing (`pts[0].x = 99`) uses the same address calculation followed by a store instruction.

### Element Copy (`let p: Point = pts[1]`)

Copying a whole struct element to a variable emits a region copy of `elem_size` bytes from the element's address into the destination's frame slot, then sets the variable local to point to the copy.

## Limitations

1. **Nested compound depth**: Nesting beyond one level (e.g., a struct containing a struct that itself contains a struct) is rejected by analysis rule A026 and cannot be lowered.
2. **Partial initialization**: `let arr: [i32; 5] = [1, 2, _, _, _];` is not supported.
3. **Recursion with arrays or structs**: Functions using compound types cannot recurse.
4. **Uzumaki element count limit**: `lower_struct_uzumaki` and `lower_array_uzumaki` return `Err(CodegenError::ArrayTooLargeForUzumaki)` if the total element count exceeds `MAX_UZUMAKI_UNROLL_ELEMENTS` (65 536). This is a compile-time bound to prevent instruction explosion; practical struct and array sizes are far below this limit.

## Cov Mark Coverage

Coverage marks for testing array- and struct-related code:

| Mark | Location | Meaning |
|---|---|---|
| `wasm_codegen_emit_stack_prologue` | `emit_stack_prologue()` | Frame allocation code emitted |
| `wasm_codegen_emit_stack_epilogue` | `emit_stack_epilogue()` | Frame deallocation code emitted |
| `wasm_codegen_emit_array_param_copy` | `emit_array_param_copy()` | Array parameter that was given a frame slot copied into it |
| `wasm_codegen_emit_array_index_read` | `lower_array_index_access()` | Array element read via load |
| `wasm_codegen_emit_array_index_write` | `lower_array_index_write()` | Array element written via store |
| `wasm_codegen_emit_bounds_check` | `emit_bounds_check_guard()` | Runtime bounds-check guard emitted for a dynamic index (every build, Compile and Proof) |
| `wasm_codegen_emit_array_uzumaki` | `lower_array_uzumaki()` | Non-deterministic array initialization |
| `wasm_codegen_emit_struct_literal` | `lower_struct_literal()` | Struct literal stored field-by-field |
| `wasm_codegen_emit_struct_param_copy` | `emit_struct_param_copy()` | Struct parameter that was given a frame slot copied into it |
| `wasm_codegen_emit_struct_copy` | `lower_struct_copy_var_init()` | Struct-to-struct copy via a region copy |
| `wasm_codegen_emit_member_access_read` | `lower_member_access()` | Struct field read via load |
| `wasm_codegen_emit_member_access_write` | `lower_member_access_write()` | Struct field write via store |
| `wasm_codegen_emit_struct_uzumaki` | `lower_struct_uzumaki()` | Non-deterministic struct initialization (field-wise uzumaki stores) |
| `wasm_codegen_uzumaki_domain_bool` | `emit_compound_uzumaki_domain_constraint()` → `emit_uzumaki_domain_constraint()` | A `bool` array or struct-field uzumaki leaf was constrained via `and 1` before its store |
| `wasm_codegen_uzumaki_domain_enum` | `emit_compound_uzumaki_domain_constraint()` → `emit_uzumaki_domain_constraint()` | A non-empty-enum array or struct-field uzumaki leaf was constrained via `rem_u <variant count>` before its store |
| `wasm_codegen_frame_fill_unrolled` | `emit_frame_zero_fill()` | A frame at or below `BULK_UNROLL_LIMIT_BYTES` was zero-filled with a straight-line sequence of `i64.store`s |
| `wasm_codegen_frame_fill_loop` | `emit_frame_zero_fill()` | A frame above `BULK_UNROLL_LIMIT_BYTES` was zero-filled with a 16-byte-stride loop |
| `wasm_codegen_memcpy_unrolled` | `emit_memcpy_via_locals()` | A region at or below `BULK_UNROLL_LIMIT_BYTES` was copied with straight-line loads/stores |
| `wasm_codegen_memcpy_loop` | `emit_memcpy_via_locals()` | A region above `BULK_UNROLL_LIMIT_BYTES` was copied with an 8-byte-stride loop plus a static tail |
| `wasm_codegen_frame_fill_bulk` | `emit_frame_zero_fill()` | A frame was zero-filled with one `memory.fill` because the build permits bulk memory |
| `wasm_codegen_memcpy_bulk` | `emit_memcpy_via_locals()` | A region was copied with one `memory.copy` because the build permits bulk memory |
| `wasm_codegen_memcpy_via_stack_bulk` | `emit_memcpy_via_stack()` | A body-level compound copy consumed its two pushed addresses with one `memory.copy` |
| `wasm_codegen_emit_sret_copy` | `emit_sret_copy()` | A whole compound value was moved into the caller-provided sret destination |
| `wasm_codegen_param_by_reference` | `compound_param_is_by_reference()` | A compound parameter is not written, and every `external fn` argument it reaches (if any) declares nothing `mut`, so it gets no slot and no copy |
| `wasm_codegen_param_written_in_body` | `compound_param_is_by_reference()` | An assignment rooted at the parameter was found, so it keeps its slot and copy |
| `wasm_codegen_param_escapes_to_extern` | `compound_param_is_by_reference()` | The parameter reaches an argument of an `external fn` declaring a `mut` parameter, so it keeps its slot and copy |
| `wasm_codegen_param_reaches_read_only_extern` | `compound_param_is_by_reference()` | The parameter reaches an `external fn` argument, but every declaration it reaches marks nothing `mut`, so it is still by reference |
| `wasm_codegen_self_escapes_to_extern` | `compound_param_is_by_reference()` | The escaping parameter was an immutable `self` receiver |

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
  (i32.const 16)           ;; frame_size: 3 i32s = 12 bytes, aligned to 16
  (i32.sub)
  (local.tee 0)            ;; $__frame_ptr = new top
  (global.set 0)           ;; update stack pointer

  ;; Zero-fill the 16-byte frame: below BULK_UNROLL_LIMIT_BYTES (128), so this
  ;; is a straight-line sequence of i64.store 0 rather than a loop.
  (local.get 0)
  (i64.const 0)
  (i64.store offset=0)
  (local.get 0)
  (i64.const 0)
  (i64.store offset=8)

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
  (i32.const 16)
  (i32.add)
  (global.set 0)           ;; restore stack pointer

  (return)
)
```

### Array Parameter, Read Only

**Inference:**
```inference
pub fn sum_array(arr: [i32; 3]) -> i32 {
    return arr[0] + arr[1] + arr[2];
}
```

**Key codegen points**:
1. Parameter `arr` gets local index 0 (pointer)
2. Nothing assigns through `arr` and it reaches no `external fn` at all, so
   `compound_param_is_by_reference()` returns `true` and it gets no frame slot
3. Nothing else in the function needs memory, so `compute_frame_layout()` returns `None` —
   no `__frame_ptr` local, no prologue, no epilogue, no `__stack_pointer` mutation
4. Each `arr[i]` read loads from the caller's pointer plus the element offset

**Generated WASM:**
```wasm
(func $sum_array (param $arr i32) (result i32)
  local.get $arr
  i32.load                 ;; arr[0], read from the caller's memory
  local.get $arr
  i32.const 4
  i32.add
  i32.load                 ;; arr[1]
  i32.add
  local.get $arr
  i32.const 8
  i32.add
  i32.load                 ;; arr[2]
  i32.add
  return
  unreachable              ;; function-end sentinel after the terminal return
)
```

### Array Parameter, Written

**Inference:**
```inference
pub fn mutate_copy(mut arr: [i32; 3]) -> i32 {
    arr[0] = 99;
    return arr[0];
}
```

**Key codegen points**:
1. Parameter `arr` gets local index 0 (pointer)
2. `arr[0] = 99` is an assignment rooted at `arr`, so the parameter earns a frame slot
3. `compute_frame_layout()` allocates 16 bytes (3 i32s = 12 bytes, aligned to 16)
4. Prologue allocates and zero-fills the frame, then `emit_array_param_copy()` copies 3
   elements and rebinds `$arr` to the frame slot
5. The write and every later `arr[i]` read address the callee's copy
6. Epilogue restores the stack pointer

## Related Resources

- `core/wasm-codegen/src/memory.rs` - Implementation
- `core/wasm-codegen/src/compiler.rs` - `compute_frame_layout()`, `compound_param_is_by_reference()`, array lowering methods
- `core/type-checker` - Type validation for array types
- WASM Memory spec: https://webassembly.org/docs/modules/#memory-section
- WASM Load/Store spec: https://webassembly.org/docs/semantics/#memory-operators
