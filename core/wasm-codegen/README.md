# inference-wasm-codegen

Direct WebAssembly code generation for the Inference compiler.

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
2. **Local Pre-scan** - Discover all variable definitions before emission
3. **Instruction Emission** - Lower functions, statements, and expressions to WASM instructions
4. **Module Assembly** - Assemble TypeSection, FunctionSection, ExportSection, CodeSection, and NameSection into a complete WASM binary

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

WebAssembly only supports `i32`, `i64`, `f32`, and `f64` as value types. Smaller integer types use `i32` with appropriate truncation and extension during operations.

## WebAssembly Execution Model

Inference uses the **reactor model** rather than the command model:

### Command Model (Typical for WASI)

Languages like Rust and Zig targeting `wasm32-wasi` generate a `_start` entry point:

```text
_start() → runtime initialization → main() → exit
```

Execution: `wasmtime module.wasm`

### Reactor Model (Inference)

Inference produces modules without an implicit entry point. Functions marked `pub` are exported and callable individually:

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
- **Expression types** - Limited support for complex expressions (binary operations, function calls, structs, arrays)
- **Type system** - Generic types, custom types, and function types are not yet fully implemented

## Module Organization

- `lib.rs` - Public API and AST traversal
- `compiler.rs` - WASM instruction emission and module assembly
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

## Related Resources

- [Inference Language Specification](https://github.com/Inferara/inference-language-spec)
- [Inference Book](https://github.com/Inferara/book)

## License

See the [repository license](https://github.com/Inferara/inference#license) for details.
