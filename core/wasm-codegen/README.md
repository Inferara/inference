# inference-wasm-codegen

LLVM-based WebAssembly code generation for the Inference compiler.

## Overview

This crate compiles Inference's typed AST to WebAssembly bytecode via LLVM IR. It supports standard WebAssembly instructions plus custom extensions for non-deterministic operations required for formal verification.

## Architecture

The compilation pipeline consists of multiple stages:

```text
Typed AST (TypedContext)
        ↓
    Compiler  ← LLVM Context (Inkwell)
        ↓
    LLVM IR (.ll)
        ↓
   inf-llc   ← Modified LLVM compiler with Inference intrinsics
        ↓
  WASM Object (.o)
        ↓
  rust-lld  ← WebAssembly linker (wasm flavor)
        ↓
  WASM Module (.wasm)
```

### Compilation Phases

1. **AST Traversal** - Walk typed AST and visit function definitions
2. **IR Generation** - Lower functions, statements, and expressions to LLVM IR
3. **Intrinsic Injection** - Emit LLVM intrinsic calls for non-deterministic operations
4. **Object Compilation** - Invoke inf-llc to compile IR to WebAssembly object file
5. **Linking** - Invoke rust-lld to link object into final WASM module

## Non-Deterministic Extensions

Inference supports non-deterministic constructs for formal verification through custom LLVM intrinsics that compile to WebAssembly instructions in the `0xfc` prefix space:

### Uzumaki (`@`)

Non-deterministic value generation. Represents a variable that can hold any value of its type.

```inference
pub fn example() -> i32 {
    return @;  // Returns any i32 value
}
```

**LLVM Intrinsics:**
- `llvm.wasm.uzumaki.i32` → WASM instruction `0xfc 0x3a`
- `llvm.wasm.uzumaki.i64` → WASM instruction `0xfc 0x3c`

### Forall Block

Universal quantification - all execution paths inside the block must be reachable.

```inference
pub fn example() {
    forall {
        const a: i32 = 42;
    }
}
```

**LLVM Intrinsics:**
- `llvm.wasm.forall.start` → WASM instruction `0xfc 0x3a`
- `llvm.wasm.forall.end` → WASM instruction `0xfc 0x3b`

### Exists Block

Existential quantification - at least one execution path inside the block must be reachable.

```inference
pub fn example() {
    exists {
        const a: i32 = 42;
    }
}
```

**LLVM Intrinsics:**
- `llvm.wasm.exists.start` → WASM instruction `0xfc 0x3c`
- `llvm.wasm.exists.end` → WASM instruction `0xfc 0x3d`

### Assume Block

Precondition assumption - filters execution paths based on assumptions.

```inference
pub fn example() {
    assume {
        const a: i32 = 42;
    }
}
```

**LLVM Intrinsics:**
- `llvm.wasm.assume.start` → WASM instruction `0xfc 0x3e`
- `llvm.wasm.assume.end` → WASM instruction `0xfc 0x3f`

### Unique Block

Uniqueness constraint - exactly one execution path is reachable inside the block.

```inference
pub fn example() {
    unique {
        const a: i32 = 42;
    }
}
```

**LLVM Intrinsics:**
- `llvm.wasm.unique.start` → WASM instruction `0xfc 0x40`
- `llvm.wasm.unique.end` → WASM instruction `0xfc 0x41`

### Optimization Barriers

Functions containing non-deterministic blocks receive `optnone` and `noinline` LLVM attributes. This prevents optimization passes from eliminating or inlining intrinsic calls, which would break formal verification guarantees.

## Type Mapping

Inference types map to LLVM types and ultimately to WebAssembly types:

| Inference Type | LLVM Type | WASM Type |
|----------------|-----------|-----------|
| `unit`         | void      | -         |
| `bool`         | i1        | i32       |
| `i8`, `u8`     | i8        | i32       |
| `i16`, `u16`   | i16       | i32       |
| `i32`, `u32`   | i32       | i32       |
| `i64`, `u64`   | i64       | i64       |

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

Inference targets `wasm32-unknown-unknown` and produces modules without an implicit entry point. Functions marked `pub` are exported and callable individually:

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

**Linker Flags:**
- `--no-entry` - Tells LLD there is no `_start` function (reactor mode)
- `--export=main` - Explicitly exports `main` if present (LLD creates argc/argv wrapper)

## External Dependencies

This crate requires two external binaries:

- **inf-llc** - Modified LLVM compiler with support for Inference's custom intrinsics
- **rust-lld** - WebAssembly linker from the Rust toolchain

These binaries must be available in the workspace's `external/bin/{platform}/` directory. See the [repository README](https://github.com/Inferara/inference#building-from-source) for download links and setup instructions.

### Platform Support

- **Linux x86-64** - Requires `libLLVM.so.21.1-rust-1.94.0-nightly` in `external/lib/linux/`
- **macOS Apple Silicon** (M1/M2) - Uses bundled or system LLVM
- **Windows x86-64** - DLLs must be in `bin/` directory

### Build-time Setup

The `build.rs` script automatically:
1. Checks for required binaries in `external/bin/{platform}/`
2. Copies binaries to `target/{profile}/bin/`
3. Sets executable permissions on Unix platforms
4. Displays download URLs if binaries are missing

### Runtime Binary Location

The `utils` module locates binaries at runtime using a multi-strategy search:
1. Check build-time hint from `INF_WASM_CODEGEN_BIN_DIR` environment variable
2. Search in `bin/` directory relative to current executable
3. Search in `../bin/` directory (for test executables in `deps/`)

On Linux, `LD_LIBRARY_PATH` is configured to locate bundled LLVM libraries. On macOS, `DYLD_LIBRARY_PATH` is used if `LLVM_SYS_211_PREFIX` is set. Windows requires no environment configuration as DLLs are loaded from the executable directory.

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
1. Initializes LLVM WebAssembly target
2. Creates an LLVM context and compiler instance
3. Traverses the typed AST and generates IR for function definitions
4. Compiles the LLVM module to WebAssembly via external tools
5. Returns the resulting WASM bytecode

## Current Limitations

- **Multi-file support** - Only single-file compilation is fully implemented
- **Top-level constructs** - Only function definitions are compiled; type definitions, constants at module level, and other top-level items are not yet supported
- **Expression types** - Limited support for complex expressions (binary operations, function calls, structs, arrays)
- **Type system** - Generic types, custom types, and function types are not yet fully implemented

## Module Organization

- `lib.rs` - Public API and AST traversal
- `compiler.rs` - LLVM IR generation and intrinsic handling
- `utils.rs` - External toolchain invocation and environment setup
- `build.rs` - Build-time binary setup and validation

## Testing

Tests are located in `tests/src/codegen/wasm/base.rs`:

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
- [Custom LLVM Intrinsics PR](https://github.com/Inferara/llvm-project/pull/2)
- [Modified LLVM Compiler](https://github.com/Inferara/llvm-project)
- [Inference Book](https://github.com/Inferara/book)

## License

See the [repository license](https://github.com/Inferara/inference#license) for details.
