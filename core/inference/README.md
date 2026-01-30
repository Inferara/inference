# inference

Core orchestration crate for the Inference compiler pipeline.

## Overview

This crate provides the main entry points for compiling Inference source code through a multi-phase pipeline: parsing, type checking, semantic analysis, code generation, and optional translation to Rocq formal verification language.

```text
.inf source → tree-sitter → Typed AST → Type Check → LLVM IR → WASM → Rocq (.v)
```

## Quick Start

Add this crate as a dependency:

```toml
[dependencies]
inference = "0.1.0"
```

Compile Inference source to WebAssembly:

```rust
use inference::{parse, type_check, codegen};

fn compile(source_code: &str) -> anyhow::Result<Vec<u8>> {
    // Phase 1: Parse source into AST
    let arena = parse(source_code)?;

    // Phase 2: Type check the AST
    let typed_context = type_check(arena)?;

    // Phase 3: Generate WASM bytecode
    let wasm_bytes = codegen(&typed_context)?;

    Ok(wasm_bytes)
}
```

## API Functions

The crate exposes five primary functions:

| Function | Input | Output | Purpose |
|----------|-------|--------|---------|
| [`parse`] | `&str` (source code) | `Arena` | Parse source into arena-based AST |
| [`type_check`] | `Arena` | `TypedContext` | Type check and infer types |
| [`analyze`] | `&TypedContext` | `()` | Semantic analysis (WIP) |
| [`codegen`] | `&TypedContext` | `Vec<u8>` | Generate WebAssembly bytecode |
| [`wasm_to_v`] | `&str`, `&Vec<u8>` | `String` | Translate WASM to Rocq |

## Compilation Pipeline

### Phase 1: Parsing

The [`parse`] function transforms source code into an arena-based Abstract Syntax Tree:

```rust
use inference::parse;

let source = r#"
    fn factorial(n: i32) -> i32 {
        if n <= 1 {
            return 1;
        } else {
            return n * factorial(n - 1);
        }
    }
"#;

let arena = parse(source)?;
let functions = arena.functions();
assert_eq!(functions.len(), 1);
```

The parser uses tree-sitter for concrete syntax tree construction, then builds a typed AST with O(1) node lookups via arena allocation.

### Phase 2: Type Checking

The [`type_check`] function performs bidirectional type inference:

```rust
use inference::{parse, type_check};

let source = r#"
    fn add(x: i32, y: i32) -> i32 {
        return x + y;
    }
"#;

let arena = parse(source)?;
let typed_context = type_check(arena)?;

// Access typed AST nodes
let functions = typed_context.functions();
```

Type checking operates in five phases:
1. Process directives (register imports)
2. Register types (collect struct/enum definitions)
3. Resolve imports (bind import paths)
4. Collect functions (register signatures)
5. Infer variables (type-check bodies)

### Phase 3: Semantic Analysis

The [`analyze`] function is a placeholder for future semantic analysis:

```rust
use inference::{parse, type_check, analyze};

let arena = parse(source)?;
let typed_context = type_check(arena)?;
analyze(&typed_context)?; // Currently a no-op
```

**Status**: Work in progress. Will include dead code detection, unreachable code analysis, and control flow validation.

### Phase 4: Code Generation

The [`codegen`] function generates WebAssembly bytecode using LLVM IR:

```rust
use inference::{parse, type_check, codegen};
use std::fs;

let arena = parse(source)?;
let typed_context = type_check(arena)?;
let wasm_bytes = codegen(&typed_context)?;

fs::write("output.wasm", &wasm_bytes)?;
```

The code generator supports Inference's non-deterministic extensions via custom LLVM intrinsics:

| Construct | Opcode | Purpose |
|-----------|--------|---------|
| `@` (uzumaki) | `0xfc 0x3c` | Non-deterministic value generation |
| `forall { }` | `0xfc 0x3a` | Universal quantification block |
| `exists { }` | `0xfc 0x3b` | Existential quantification block |
| `assume { }` | `0xfc 0x3d` | Precondition filtering |
| `unique { }` | `0xfc 0x3e` | Uniqueness constraint |

#### Example: Non-Deterministic Code

```rust
let source = r#"
    pub fn verify_sorted() {
        forall {
            let x: i32 = @;
            let y: i32 = @;
            assume {
                assert(x <= y);
            }
            assert(x <= y);
        }
    }
"#;

let arena = parse(source)?;
let typed_context = type_check(arena)?;
let wasm = codegen(&typed_context)?;
```

### Phase 5: Rocq Translation

The [`wasm_to_v`] function translates WebAssembly to Rocq verification code:

```rust
use inference::{parse, type_check, codegen, wasm_to_v};
use std::fs;

let source = r#"
    fn is_even(n: i32) -> bool {
        return n % 2 == 0;
    }
"#;

let arena = parse(source)?;
let typed_context = type_check(arena)?;
let wasm_bytes = codegen(&typed_context)?;
let rocq_code = wasm_to_v("EvenChecker", &wasm_bytes)?;

fs::write("even_checker.v", rocq_code)?;
```

The generated Rocq code can be used with the Rocq proof assistant to verify program properties.

## Architecture

This crate is a thin orchestration layer delegating to specialized crates:

- **[`inference_ast`]** - Arena-based AST with tree-sitter parsing
- **[`inference_type_checker`]** - Bidirectional type checking with error recovery
- **[`inference_wasm_codegen`]** - LLVM-based WebAssembly code generation
- **[`inference_wasm_to_v_translator`]** - WASM to Rocq translation

## External Dependencies

Code generation requires platform-specific binaries in the `external/bin/` directory:

- **inf-llc** - LLVM compiler with Inference intrinsic support
- **rust-lld** - WebAssembly linker

See the [repository README](https://github.com/Inferara/inference) for download links.

## Platform Support

- Linux x86-64
- macOS Apple Silicon (M1/M2/M3)
- Windows x86-64

## Error Handling

All functions return `anyhow::Result` with detailed error messages. Each compilation phase collects multiple errors before failing, enabling developers to see all issues at once.

```rust
match parse(source) {
    Ok(arena) => println!("Parsed {} nodes", arena.nodes().len()),
    Err(e) => eprintln!("Parse errors:\n{}", e),
}
```

## Limitations

- **Single-file compilation**: Multi-file projects are not yet supported
- **Analyze phase**: Semantic analysis is work-in-progress
- **Error recovery**: Some parse errors prevent AST construction

## Examples

### Complete Compilation Pipeline

```rust
use inference::{parse, type_check, analyze, codegen};
use std::fs;

fn compile_file(input_path: &str, output_path: &str) -> anyhow::Result<()> {
    let source = fs::read_to_string(input_path)?;

    let arena = parse(&source)?;
    let typed_context = type_check(arena)?;
    analyze(&typed_context)?;
    let wasm_bytes = codegen(&typed_context)?;

    fs::write(output_path, &wasm_bytes)?;
    println!("Compiled {} to {}", input_path, output_path);

    Ok(())
}
```

### Verification Workflow

```rust
use inference::{parse, type_check, codegen, wasm_to_v};
use std::fs;

fn verify_program(source_path: &str, module_name: &str) -> anyhow::Result<()> {
    let source = fs::read_to_string(source_path)?;

    let arena = parse(&source)?;
    let typed_context = type_check(arena)?;
    let wasm = codegen(&typed_context)?;
    let rocq = wasm_to_v(module_name, &wasm)?;

    let output = format!("{}.v", module_name.to_lowercase());
    fs::write(&output, rocq)?;
    println!("Generated verification file: {}", output);

    Ok(())
}
```

## Related Crates

- **[`inference-cli`]** - Legacy `infc` command-line interface
- **[`infs`]** - Modern unified CLI toolchain
- **[`inference-ast`]** - AST data structures
- **[`inference-type-checker`]** - Type system implementation

## Documentation

- [Inference Language Specification](https://github.com/Inferara/inference-language-spec)
- [Inference Book](https://github.com/Inferara/book)
- [API Documentation](https://docs.rs/inference)

## License

See LICENSE file in repository root.
