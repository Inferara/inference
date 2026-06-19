# inference

Core orchestration crate for the Inference compiler pipeline.

## Overview

This crate provides the main entry points for compiling Inference source code through a multi-phase pipeline: parsing, type checking, semantic analysis, code generation, and optional translation to Rocq formal verification language.

```text
.inf source → inference-parser → Typed AST → Type Check → WASM → Rocq (.v)
```

## Quick Start

Add this crate as a dependency:

```toml
[dependencies]
inference = "0.1.0"
```

Compile a single-file Inference source to WebAssembly:

```rust
use inference::{parse, type_check, codegen};
use inference_wasm_codegen::CodegenOutput;

fn compile(source_code: &str) -> anyhow::Result<CodegenOutput> {
    // Phase 1: Parse source into AST
    let arena = parse(source_code)?;

    // Phase 2: Type check the AST
    let typed_context = type_check(arena)?;

    // Phase 3: Generate WASM bytecode
    let codegen_output = codegen(&typed_context, "module")?;

    Ok(codegen_output)
}
```

Compile a multi-file project starting from its entry point:

```rust
use std::path::Path;
use inference::{parse_project, type_check, codegen};

fn compile_project(entry: &Path) -> anyhow::Result<Vec<u8>> {
    // Walk the import-reachable closure from the entry file
    let project = parse_project(entry)?;

    // Surface any unreachable-file warnings
    for warning in &project.warnings {
        eprintln!("{warning}");
    }

    // Type check the unified arena
    let typed_context = type_check(project.arena)?;

    // Generate a single flat WASM module
    let output = codegen(&typed_context, "main")?;
    Ok(output.wasm().to_vec())
}
```

## API Functions

The crate exposes the following primary functions:

| Function | Input | Output | Purpose |
|----------|-------|--------|---------|
| [`parse`] | `&str` (source code) | `AstArena` | Parse a single source string into an arena-based AST |
| [`parse_project`] | `&Path` (entry file) | `ProjectParse` | Walk the import-reachable closure into one arena |
| [`type_check`] | `AstArena` | `TypedContext` | Type check and infer types (single- or multi-file) |
| [`analyze`] | `&TypedContext` | `AnalysisResult` | Semantic analysis (rule-based) |
| [`codegen`] | `&TypedContext`, `&str` | `CodegenOutput` | Generate a single flat WebAssembly module |
| [`wasm_to_v`] | `&str`, `&[u8]`, `&FxHashMap<String, Vec<u32>>` | `String` | Translate WASM to Rocq |

`ProjectParse` holds both the unified `arena` and a `Vec<ProjectWarning>`. The only current warning variant is `UnreachableFile`, emitted when a `.inf` file under the source root is not reachable from any import chain.

## Compilation Pipeline

### Phase 1: Parsing

Two entry points cover single-file and multi-file use cases.

**Single source string** — [`parse`]:

```rust
use inference::parse;

let source = r#"
    fn add(a: i32, b: i32) -> i32 {
        return a + b;
    }
"#;

let arena = parse(source)?;
```

A path-form `use` directive inside a single-string parse (no filesystem context) causes
a type-check-time error; use `parse_project` for multi-file programs.

**Multi-file project** — [`parse_project`]:

```rust
use std::path::Path;
use inference::parse_project;

let entry = Path::new("src/main.inf");
let project = parse_project(entry)?;

// Unreachable files under the source root produce warnings.
for w in &project.warnings {
    eprintln!("{w}");
}

let arena = project.arena;  // All reachable files in one arena.
```

`parse_project` derives the source root from the entry file's parent directory and
performs a breadth-first walk of the import-reachable closure. Files are stored in
the arena in canonical order — entry first, then imported files lexicographically by
module path — which downstream phases use as their single source of truth. Import
cycles terminate via a visited set. A missing imported file returns
`InferenceError::ImportFileNotFound` with a nearest-match suggestion.

The parser (`inference-parser`) is a resilient recursive-descent parser that lowers
source directly into a typed AST with O(1) node lookups via arena allocation.

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

The [`codegen`] function generates WebAssembly bytecode directly via wasm-encoder:

```rust
use inference::{parse, type_check, codegen};
use std::fs;

let arena = parse(source)?;
let typed_context = type_check(arena)?;
let codegen_output = codegen(&typed_context)?;

fs::write("output.wasm", codegen_output.wasm())?;
```

The code generator supports Inference's non-deterministic extensions via custom WebAssembly instructions in the `0xfc` prefix space:

| Construct | Opcode | Purpose |
|-----------|--------|---------|
| `@` (uzumaki) | `0xfc 0x31` / `0xfc 0x32` | Non-deterministic value generation |
| `forall { }` | `0xfc 0x3a` | Universal quantification block |
| `exists { }` | `0xfc 0x3b` | Existential quantification block |
| `assume { }` | `0xfc 0x3c` | Precondition filtering |
| `unique { }` | `0xfc 0x3d` | Uniqueness constraint |

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
let codegen_output = codegen(&typed_context)?;
let rocq_code = wasm_to_v(
    "EvenChecker",
    codegen_output.wasm(),
    codegen_output.spec_func_indices_by_spec(),
)?;

fs::write("even_checker.v", rocq_code)?;
```

The generated Rocq code can be used with the Rocq proof assistant to verify program properties.

## Architecture

This crate is a thin orchestration layer delegating to specialized crates:

- **[`inference_ast`]** - Arena-based AST data model
- **[`inference_parser`]** - Lexer + recursive-descent parser
- **[`inference_type_checker`]** - Bidirectional type checking with error recovery
- **[`inference_wasm_codegen`]** - WebAssembly code generation via wasm-encoder
- **[`inference_wasm_to_v_translator`]** - WASM to Rocq translation

## Dependencies

Code generation uses the `wasm-encoder` crate for WebAssembly binary emission. No external binaries or LLVM installation required.

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

- **Top-level `const` in codegen**: Top-level `const` declarations do not reach codegen (analysis rule A032 / issue #171). Cross-file `const` type-checking works and will feed into codegen when #171 lands.
- **No import aliasing**: `use a::b as c;` is not yet supported.
- **Error recovery**: Some parse errors prevent AST construction.

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
    let codegen_output = codegen(&typed_context)?;

    fs::write(output_path, codegen_output.wasm())?;
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
    let codegen_output = codegen(&typed_context)?;
    let rocq = wasm_to_v(
        module_name,
        codegen_output.wasm(),
        codegen_output.spec_func_indices_by_spec(),
    )?;

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
