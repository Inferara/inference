# inference-wasm-codegen

WebAssembly code generation for the Inference compiler.

## Overview

This crate compiles Inference's typed AST directly to WebAssembly binary bytecode using the `wasm-encoder` crate. It supports standard WebAssembly instructions plus custom extensions for non-deterministic operations required for formal verification.

## Architecture

The compilation is performed entirely in-process with no external tool invocation:

```text
Typed AST (TypedContext)
        ↓
    Compiler  ← wasm-encoder
        ↓
  WASM Module (.wasm)
```

### Compilation Phases

1. **AST Traversal** - Walk typed AST and visit function definitions
2. **Function name pre-scan** - Build `func_name_to_idx` map from function names to WASM
   function section indices before the main compilation pass. This enables forward references
   — a caller defined before its callee in source can still emit a valid `call` instruction.
   See [docs/function-calls-lowering.md](docs/function-calls-lowering.md).
3. **Compound Frame Layout** - For functions with array- or struct-typed variables or parameters,
   compute a stack frame layout by walking the entire function body and collecting array and struct
   declarations and parameter types. This pre-computation determines memory offsets for each compound
   value and allocates a synthetic `__frame_ptr` WASM local. Struct fields are laid out with C-compatible
   natural alignment (`compute_struct_field_layout`). See [docs/arrays-and-memory.md](docs/arrays-and-memory.md).
4. **Local Pre-scan** - Walk the entire function body once to collect all `let` and `const`
   declarations and assign them sequential WASM local indices before any instructions are
   emitted. This step is mandatory because the WebAssembly binary format requires all local
   declarations to appear at the very start of a function body, before the instruction
   sequence. See [docs/local-variables-lowering.md](docs/local-variables-lowering.md) for a
   detailed explanation.
5. **Instruction Emission** - Lower functions, statements, and expressions to WASM
   instructions. `let` definitions are lowered via a push instruction followed by
   `local.set`; `const` definitions use the same path. Supported initializer expression
   kinds are literals, identifiers, uzumaki (`@`) expressions, function calls, array
   literals, and struct literals. Array and struct variables automatically get frame
   allocation code (prologue) and deallocation code (epilogue). Array index access
   (read/write) compiles to load/store instructions with computed addresses. Struct field
   access (`p.x`) compiles to a load at `struct_pointer + field_offset`; struct field
   assignment (`p.x = v`) compiles to a store at the same address. Function calls push
   arguments in positional order and emit a `call <func_idx>` instruction. Struct-typed
   parameters are copied into the callee's frame on entry (value semantics).
   Assignment statements (`x = value;` where `x` is declared `mut`) are lowered by
   evaluating the right-hand side expression and emitting `local.set` to store the result.
   Array index assignment (`arr[i] = value;`) computes the element address and emits a store
   instruction. `if`/`else` statements emit WASM structured `if`/`else`/`end` blocks with
   `BlockType::Empty` because Inference `if` is a statement, not an expression.
   Loop statements emit the standard WASM `block`+`loop` double-nesting pattern with a
   `br_if` exit check for conditional loops and `br 0` unconditional back-edge; `break`
   statements lower to `br <depth>` targeting the enclosing loop's exit block.
   See [docs/loops-lowering.md](docs/loops-lowering.md).
   Non-void functions emit an `unreachable` instruction before the function `end` to
   satisfy the WASM validator when all paths exit through explicit `return` instructions.
   See [docs/conditionals-lowering.md](docs/conditionals-lowering.md).
6. **Module Assembly** - Assemble TypeSection, FunctionSection, ExportSection, CodeSection,
   NameSection, and (if any function uses linear memory) MemorySection and GlobalSection into
   a complete WASM binary. Memory and globals are only emitted when at least one function uses
   arrays or structs.

## Non-Deterministic Extensions

Inference supports non-deterministic constructs for formal verification through custom WebAssembly instructions in the `0xfc` prefix space, emitted via `wasm_encoder::Function::raw()`:

### Uzumaki (`@`)

Non-deterministic value generation. Represents a variable that can hold any value of its type.

```inference
pub fn example() -> i32 {
    return @;  // Returns any i32 value
}
```

**Custom opcodes:**
- `i32.uzumaki` → `0xfc 0x31`
- `i64.uzumaki` → `0xfc 0x32`

### Forall Block

Universal quantification - all execution paths inside the block must be reachable.

```inference
pub fn example() {
    forall {
        const a: i32 = 42;
    }
}
```

**Custom opcode:** `0xfc 0x3a` + `0x40` (block type) + body + `0x0b` (end)

### Exists Block

Existential quantification - at least one execution path inside the block must be reachable.

```inference
pub fn example() {
    exists {
        const a: i32 = 42;
    }
}
```

**Custom opcode:** `0xfc 0x3b` + `0x40` (block type) + body + `0x0b` (end)

### Assume Block

Precondition assumption - filters execution paths based on assumptions.

```inference
pub fn example() {
    assume {
        const a: i32 = 42;
    }
}
```

**Custom opcode:** `0xfc 0x3c` + `0x40` (block type) + body + `0x0b` (end)

### Unique Block

Uniqueness constraint - exactly one execution path is reachable inside the block.

```inference
pub fn example() {
    unique {
        const a: i32 = 42;
    }
}
```

**Custom opcode:** `0xfc 0x3d` + `0x40` (block type) + body + `0x0b` (end)

## Type Mapping

Inference types map to WebAssembly types:

| Inference Type | WASM Type |
|----------------|-----------|
| `unit`         | -         |
| `bool`         | i32       |
| `i8`, `u8`     | i32       |
| `i16`, `u16`   | i32       |
| `i32`, `u32`   | i32       |
| `i64`, `u64`   | i64       |
| `[T; N]`       | i32       |
| `struct S`     | i32       |

WebAssembly only supports `i32`, `i64`, `f32`, and `f64` as value types. Smaller integer types use `i32` with appropriate truncation and extension during operations. Arrays and structs are represented as i32 pointers to linear memory; the compiler manages a shadow stack and emits prologue/epilogue code for frame allocation. Struct fields are laid out with C-compatible natural alignment.

## WebAssembly Execution Model

Inference uses the **reactor model** rather than the command model:

### Command Model (Typical for WASI)

Languages like Rust and Zig targeting `wasm32-wasi` generate a `_start` entry point:

```text
_start() → runtime initialization → main() → exit
```

Execution: `wasmtime module.wasm`

### Reactor Model (Inference)

Inference produces reactor-style modules where all `pub` functions are exported and callable individually:

```text
pub fn main() → exported as "main"
pub fn foo()  → exported as "foo"
fn bar()      → not exported (private)
```

Execution: `wasmtime --invoke main module.wasm`

**Why Reactor Model?**
- **Simplicity** - No runtime initialization overhead
- **Flexibility** - Multiple entry points; caller chooses which function to invoke
- **Embedding** - Better suited for embedding in host applications
- **Verification** - Functions are verified individually in formal verification

> **Planned:** When a `pub fn main` is present, the compiler will also emit a `_start` entry point so that the module can be executed as a regular program (e.g., `wasmtime module.wasm` without `--invoke`). The `has_main` detection is already implemented.

## Usage

```rust
use inference_wasm_codegen::codegen;
use inference_type_checker::typed_context::TypedContext;

fn compile(typed_context: &TypedContext) -> anyhow::Result<Vec<u8>> {
    // Generate WASM bytecode from typed AST
    let wasm_bytes = codegen(typed_context)?;
    Ok(wasm_bytes)
}
```

The `codegen` function:
1. Creates a compiler instance
2. Traverses the typed AST and emits WASM instructions for function definitions
3. Assembles the WASM module sections
4. Returns the resulting WASM bytecode

## Current Limitations

- **Multi-file support** - Only single-file compilation is fully implemented
- **Top-level constructs** - Only function definitions are compiled; type definitions, constants at module level, and other top-level items are not yet supported
- **Control flow** - `loop` and `break` statements are now supported (conditional loops, infinite loops, nested loops, and break from any nesting depth). Assignment statements (`x = value;`) are supported for identifier targets, array index targets, and struct field targets (`p.x = v`).
- **Expression types** - Fixed-size arrays with scalar element types are supported, including array-returning functions via the sret calling convention. Structs with scalar fields are supported: struct literals, member access read/write, struct parameters (copy-on-entry), and struct-returning functions via sret. Nested arrays, arrays of structs, arrays of arrays, partial initialization syntax, and mutable array parameters are not yet implemented. Plain identifier-based function calls are supported; method calls (`obj.method()`), associated function calls (`Type::func()`), and higher-order function calls are not yet implemented.
- **Type system** - Generic types and function types are not yet fully implemented
- **Recursion with compound types** - Functions using arrays or structs cannot currently recurse (no stack overflow analysis). Recursion detection and stack bounds checking are future work.
- **Return-path analysis** - The compiler does not yet emit a compile-time error for non-void functions missing a return on all paths. An `unreachable` trap is emitted as a runtime safety net; see [docs/conditionals-lowering.md](docs/conditionals-lowering.md).

## Documentation

Detailed design documents live in `docs/`:

- [docs/local-variables-lowering.md](docs/local-variables-lowering.md) - The two-pass
  approach for lowering `let`/`const` locals, supported initializer kinds, and the
  `lower_literal` type-dispatch logic for sub-i32 types.
- [docs/assignment-lowering.md](docs/assignment-lowering.md) - How assignment statements
  (`x = expr;`) are lowered to WASM local.set instructions, local index resolution, and
  current limitations on target forms.
- [docs/function-calls-lowering.md](docs/function-calls-lowering.md) - Forward-reference
  pre-scan, parameter index interlock with locals, call lowering pipeline, drop emission
  rules, and known limitations.
- [docs/conditionals-lowering.md](docs/conditionals-lowering.md) - How `if`/`else`
  statements are lowered to WASM structured control flow and why `unreachable` is emitted
  before the `end` of every non-void function.
- [docs/arrays-and-memory.md](docs/arrays-and-memory.md) - Stack allocation and shadow
  stack infrastructure for fixed-size arrays and structs, including frame layout computation,
  prologue/epilogue emission, load/store instruction selection, copy-on-entry semantics for
  array and struct parameters, struct field layout (`compute_struct_field_layout`), member
  access lowering, and struct literal lowering.
- [docs/loops-lowering.md](docs/loops-lowering.md) - How `loop`/`break` statements are
  lowered to WASM structured control flow (`block`/`loop`/`br`), `LoopContext` depth
  tracking, and interaction with non-det blocks, if-statements, and array frames.

## Module Organization

- `lib.rs` - Public API and AST traversal
- `compiler.rs` - WASM instruction emission, module assembly, and array frame layout computation
- `memory.rs` - Shadow stack infrastructure: `FrameLayout`, `ArraySlot`, `StructSlot`, `StructFieldSlot`, `compute_struct_field_layout`, prologue/epilogue emission, load/store instruction selection, `emit_struct_param_copy`
- `errors.rs` - `CodegenError` enum for function call lowering failures
- `output.rs` - `CodegenOutput` containing WASM bytes and metadata
- `target.rs` - Compilation target definitions (`Wasm32`, `Soroban`)

## Testing

Tests are located in `tests/src/codegen/wasm/`:

```bash
# Run all codegen tests
cargo test -p inference-tests

# Run specific test
cargo test -p inference-tests trivial_test
```

Test data includes:
- `trivial.inf` - Simple function returning a constant
- `const.inf` - Constant definitions
- `nondet.inf` - Non-deterministic constructs (uzumaki, forall, exists, assume, unique)
- `local_variables.inf` - All `let` binding forms: every numeric type, bool, uzumaki, and
  identifier initializers (validated against `inf_wasmparser` and compared byte-for-byte)
- `local_variables_exec.inf` - Wasmtime execution tests that verify the correct WASM value
  is returned for each `let` binding form
- `fn_params.inf` - Functions with typed parameters (i32, i64, bool, multi-param); verifies
  parameter-to-local-index mapping and WASM type signatures
- `fn_calls.inf` - Function call scenarios including no-arg calls, arg passing, forward
  references, and `let`-from-call; validated and executed via wasmtime
- `if_else.inf` - `if`-only, `if`/`else`, nested `if`, locals inside arms, and void
  `if`; validated against `inf_wasmparser` and byte-compared, executed via wasmtime
- `if_nondet.inf` - `if` nested inside a `forall` non-deterministic block
- `if_bool_exprs.inf` - Comprehensive boolean condition coverage: direct bool params,
  comparison + logical operators, complex nested conditions, boolean locals, boolean
  equality/inequality, and if/else with complex conditions; validated and executed via
  wasmtime
- `assign.inf` - Assignment statement tests including simple assignments (i32, i64, bool),
  assignments from expressions, parameters, function calls, multiple assignments, and
  assignments inside `if` blocks; validated against `inf_wasmparser` and executed via
  wasmtime
- `assign_nondet.inf` - Assignment statements inside non-deterministic blocks (forall,
  exists, assume, unique)
- `algo_bitwise.inf` - 12 functions implementing bitwise algorithms (popcount, power-of-2
  checks, bit manipulation, rotation, byte swapping) demonstrating combined use of binary
  operations, prefix unary expressions, conditionals, and recursive function calls
- `algo_i64_mixed.inf` - Demonstrates i64 operations in context of classic algorithms
  (factorial, fibonacci, GCD) with variable definitions and recursive calls
- `algo_converge.inf` - Convergence algorithms using i32/i64 arithmetic with loops
  and recursive patterns
- `algo_recursive_math.inf` - Various mathematical algorithms using recursion and arithmetic
- `array_literal.inf` - Fixed-size array literal declarations with i32 and bool element types,
  including single-element and multi-array cases; validated against `inf_wasmparser` and
  executed via wasmtime
- `array_index.inf` - Array index read access (both constant and variable indices) with i32
  and bool arrays, including reading array elements for use in conditions; validated and
  executed via wasmtime
- `array_assign.inf` - Array element write operations including simple writes, multiple
  assignments, element swapping, writes with computed indices, and bool array mutations;
  validated and executed via wasmtime
- `array_params.inf` - Array-typed function parameters, copy-on-entry semantics, value
  semantics verification (callee mutations don't affect caller's array), multi-parameter
  functions, and bool array parameters; validated and executed via wasmtime
- `array_nondet.inf` - Arrays inside non-deterministic blocks (forall, exists) and
  non-deterministic array initialization (`@`) inside blocks; validated against `inf_wasmparser`
  (non-det modules skip WAT comparison)
- `struct_literal.inf` - Struct literal initialization: simple structs with i32 fields, single-field
  structs, and mixed-type structs (`bool`, `i64`); validated against `inf_wasmparser` and executed
  via wasmtime
- `struct_access.inf` - Struct field read access (`p.x`, `p.y`, `p.x + p.y`) for i32, bool, and
  i64 field types; validated and executed via wasmtime
- `struct_assign.inf` - Assignment to struct fields on mutable struct variables (`p.x = 42`,
  field swapping, bool field mutation); validated and executed via wasmtime
- `struct_params.inf` - Struct-typed function parameters: copy-on-entry value semantics
  (callee mutations don't affect caller's struct), mixed-type struct params, multiple struct
  params; validated and executed via wasmtime
- `struct_return.inf` - Functions returning struct types via sret convention: return of struct
  literal, return of a variable, chained calls (`return make_point()`), and mixed-type struct
  returns; validated and executed via wasmtime
- `struct_copy.inf` - Struct-to-struct copy (`let b = a;`) preserving value semantics:
  modifications to the copy do not affect the original; validated and executed via wasmtime
- Loop test fixtures in `tests/test_data/codegen/wasm/loops/`:
  - `simple_loop.inf` - Basic conditional loops (`loop COND { body }`) with counter patterns
  - `infinite_loop_break.inf` - Infinite loops (`loop { body }`) with `break` exit
  - `nested_loop.inf` - Nested conditional and infinite loops with inner break
  - `loop_with_if.inf` - Conditional loops with if/else bodies
  - `loop_accumulator.inf` - Accumulator patterns (sum, factorial, power)
  - `loop_break_early.inf` - Conditional loop with early break from if
  - `break_nested_if.inf` - Break inside nested if conditions
  - `void_loop.inf` - Void-returning function with loop
  - `loop_zero_iters.inf` - Loop with always-false condition (zero iterations)
  - `loop_with_array.inf` - Loops with array variables (frame layout + prologue/epilogue)
  - `loop_in_nondet.inf` - Loops inside forall/exists non-deterministic blocks
  - `nondet_then_break.inf` - Non-det block inside loop followed by break
  - `loop_return_array.inf` - Early return from loop with active array frame layout

## Related Resources

- [Inference Language Specification](https://github.com/Inferara/inference-language-spec)
- [Inference Book](https://github.com/Inferara/book)

## License

See the [repository license](https://github.com/Inferara/inference#license) for details.
