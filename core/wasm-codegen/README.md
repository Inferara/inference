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
3. **Array Frame Layout** - For functions with array-typed variables or parameters, compute
   a stack frame layout by walking the entire function body and collecting array declarations
   and parameter types. This pre-computation determines memory offsets for each array and
   allocates a synthetic `__frame_ptr` WASM local. See [docs/arrays-and-memory.md](docs/arrays-and-memory.md).
4. **Local Pre-scan** - Walk the entire function body once to collect all `let` and `const`
   declarations and assign them sequential WASM local indices before any instructions are
   emitted. This step is mandatory because the WebAssembly binary format requires all local
   declarations to appear at the very start of a function body, before the instruction
   sequence. See [docs/local-variables-lowering.md](docs/local-variables-lowering.md) for a
   detailed explanation.
5. **Instruction Emission** - Lower functions, statements, and expressions to WASM
   instructions. `let` definitions are lowered via a push instruction followed by
   `local.set`; `const` definitions use the same path. Supported initializer expression
   kinds are literals, identifiers, uzumaki (`@`) expressions, function calls, and array
   literals. Array variables automatically get frame allocation code (prologue) and deallocation
   code (epilogue). Array index access (read/write) compiles to load/store instructions with
   computed addresses. Function calls push arguments in positional order and emit a `call <func_idx>`
   instruction. Assignment statements (`x = value;` where `x` is declared `mut`) are lowered by
   evaluating the right-hand side expression and emitting `local.set` to store the result.
   Array index assignment (`arr[i] = value;`) computes the element address and emits a store instruction.
   `if`/`else` statements emit WASM structured `if`/`else`/`end` blocks with
   `BlockType::Empty` because Inference `if` is a statement, not an expression.
   Non-void functions emit an `unreachable` instruction before the function `end` to
   satisfy the WASM validator when all paths exit through explicit `return` instructions.
   See [docs/conditionals-lowering.md](docs/conditionals-lowering.md).
6. **Module Assembly** - Assemble TypeSection, FunctionSection, ExportSection, CodeSection,
   NameSection, and (if arrays are present) MemorySection and GlobalSection into a complete
   WASM binary. Memory and globals are only emitted when at least one function uses arrays.

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

WebAssembly only supports `i32`, `i64`, `f32`, and `f64` as value types. Smaller integer types use `i32` with appropriate truncation and extension during operations. Arrays are represented as i32 pointers to linear memory; the compiler manages a shadow stack and emits prologue/epilogue code for frame allocation.

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
- **Control flow** - `loop` and `break` statements are not yet implemented (`todo!()`). Assignment statements (`x = value;`) are supported for identifier targets and array index targets; assignments to struct fields (member access) are deferred to struct support.
- **Expression types** - Limited support for complex expressions (binary operations, structs). Fixed-size arrays with scalar element types are now supported, including array-returning functions via the sret calling convention. Nested arrays, arrays of structs/custom types, partial initialization syntax, and mutable array parameters are not yet implemented. Plain identifier-based function calls are supported; method calls (`obj.method()`), associated function calls (`Type::func()`), and higher-order function calls are not yet implemented.
- **Compound types** - Structs and custom types are not yet implemented
- **Type system** - Generic types and function types are not yet fully implemented
- **Recursion with arrays** - Functions using arrays cannot currently recurse (no stack overflow analysis). Recursion detection and stack bounds checking are future work.
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
  stack infrastructure for fixed-size arrays, including frame layout computation, prologue/epilogue
  emission, load/store instruction selection, and copy-on-entry semantics for array parameters.

## Module Organization

- `lib.rs` - Public API and AST traversal
- `compiler.rs` - WASM instruction emission, module assembly, and array frame layout computation
- `memory.rs` - Shadow stack infrastructure: `FrameLayout`, `ArraySlot`, prologue/epilogue emission, load/store instruction selection
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
- `algo_converge.inf` - Convergence algorithms using i32/i64 arithmetic with loops (planned)
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

## Related Resources

- [Inference Language Specification](https://github.com/Inferara/inference-language-spec)
- [Inference Book](https://github.com/Inferara/book)

## License

See the [repository license](https://github.com/Inferara/inference#license) for details.
