# Inference Compiler CLI (infc)

The standalone compiler command-line interface for the Inference programming language.

## Overview

`infc` is the core compiler binary that compiles Inference source files (`.inf`) to WebAssembly. It operates as a multi-phase compiler with explicit phase control, allowing developers to run only the phases they need.

## Relationship to `infs`

Two CLI tools are available in the Inference ecosystem:

- **`infc`** (this crate) - Standalone compiler
- **`infs`** - Unified toolchain CLI that wraps `infc` and provides additional features (project management, toolchain installation, TUI)

For most users, `infs` is the recommended interface. Use `infc` directly when:
- You need fine-grained control over compilation phases
- You're integrating Inference compilation into build systems or scripts
- You're developing or testing the compiler itself

## Compilation Phases

The Inference compiler operates in three distinct phases:

### 1. Parse (`--parse`)

Builds the typed AST using the hand-written `inference-parser`.

**What it does:**
- Reads the source file
- Runs the `inference-parser` lexer + recursive-descent parser
- Constructs arena-allocated AST nodes
- Validates syntax and basic structure
- Reports parsing errors if any

**Example:**
```bash
infc example.inf --parse
```

### 2. Analyze (`--analyze`)

Performs type checking and semantic validation.

**What it does:**
- Type inference and checking
- Symbol resolution
- Semantic validation
- Reports type errors and semantic issues

**Note**: The analyze phase is work-in-progress.

**Example:**
```bash
infc example.inf --analyze
```

### 3. Codegen (`--codegen`)

Emits WebAssembly binary.

**What it does:**
- Generates WebAssembly binary directly from typed AST
- Supports non-deterministic instructions (uzumaki `@`, forall, exists, assume, unique)
- Optionally translates to Rocq (.v) format for formal verification

**Example:**
```bash
infc example.inf --codegen -o
```

## Phase Execution

Phases execute in canonical order (parse → analyze → codegen) regardless of the order flags appear on the command line. Each phase depends on the previous:

- `--parse` runs standalone
- `--analyze` automatically runs parse first
- `--codegen` automatically runs parse and analyze first

**Default behavior**: When no phase flag is given, `infc` defaults to full compilation
(`--codegen -o`), writing `out/<name>.wasm`. Supplying any explicit phase flag overrides
this default.

## Output Flags

### `-o` - Generate WASM Binary

Writes the compiled WebAssembly binary to `out/<source_name>.wasm` relative to the current working directory.

Only takes effect when `--codegen` is specified.

**Example:**
```bash
infc example.inf --codegen -o
# Creates: out/example.wasm
```

### `-v` - Generate Rocq Translation

Writes the Rocq (Coq) translation to `out/<source_name>.v` relative to the current working directory.

This enables formal verification of the compiled program using the Rocq proof assistant.

Only takes effect when `--codegen` is specified.

**Example:**
```bash
infc example.inf --codegen -v
# Creates: out/example.v
```

Both flags can be used together, or the simplified default form with `-v`:

```bash
infc example.inf -v
# Creates: out/example.wasm and out/example.v

# Explicit equivalent:
infc example.inf --codegen -o -v
```

## Output Directory

All output files are written to an `out/` directory relative to the current working directory.

The output directory is created automatically if it doesn't exist.

**Current limitation**: Output directory is relative to CWD, not source file location.

## Usage Examples

### Parse Only (Syntax Check)

```bash
infc example.inf --parse
```

**Output:**
```
Parsed: example.inf
```

### Type Check Without Codegen

```bash
infc example.inf --analyze
```

**Output:**
```
Parsed: example.inf
Analyzed: example.inf
```

### Full Compilation to WebAssembly (default)

```bash
infc example.inf
```

**Output:**
```
Parsed: example.inf
Analyzed: example.inf
Codegen complete
WASM generated at: out/example.wasm
```

Explicit equivalent:

```bash
infc example.inf --codegen -o
```

### Generate Only Rocq (No WASM File)

```bash
infc example.inf --codegen -v
```

**Output:**
```
Parsed: example.inf
Analyzed: example.inf
Codegen complete
V generated at: out/example.v
```

### Full Pipeline with Both Outputs

```bash
infc example.inf -v
# Creates: out/example.wasm and out/example.v

# Explicit equivalent:
infc example.inf --codegen -o -v
```

## Error Handling

The compiler reports errors to stderr with descriptive messages.

### Error Categories

**Parse errors**: Syntax errors, malformed AST nodes
```
Parse error: unexpected token at line 5
```

**Type errors**: Type mismatches, undefined symbols
```
Type checking failed: undefined variable 'x'
```

**Codegen errors**: WebAssembly generation failures
```
Codegen failed: unsupported expression type
```

**IO errors**: File not found, permission issues
```
Error: path not found
Error reading source file: permission denied
Failed to create output directory: permission denied
```

All errors cause the process to exit with code 1.

## Exit Codes

| Code | Meaning                                    |
|------|--------------------------------------------|
| 0    | Success - all requested phases completed   |
| 1    | Failure - usage, IO, or compilation error  |

## Current Limitations

- **Single-file compilation only**: Multi-file projects not yet supported
- **Output directory**: Relative to CWD, not source file location
- **Analysis phase**: Work-in-progress, not fully implemented

## Building

Build the compiler from the workspace root:

```bash
cargo build -p inference-cli
```

The compiled binary will be at `target/debug/infc`.

For release builds:

```bash
cargo build -p inference-cli --release
```

## Testing

### Integration Tests

Integration tests spawn the actual binary and validate behavior through stdout, stderr, and exit codes.

```bash
cargo test -p inference-cli
```

Tests verify:
- Flag validation and error handling
- Phase execution correctness
- Output file generation
- Error message formatting

### Test Data

Test data files are located at `<workspace_root>/tests/test_data/inf/`.

## Architecture

### Dependencies

- **`inference`** - Main compiler library (`parse`, `type_check`, `analyze`, `wasm_to_v`)
- **`inference-wasm-codegen`** - WebAssembly code generator (`codegen`, `Target`, `CompilationMode`, `OptLevel`)
- **`clap`** - Command-line argument parsing
- **`anyhow`** - Error handling

### Module Structure

```
core/cli/
├── src/
│   ├── main.rs              # Entry point and phase orchestration
│   ├── parser.rs            # CLI argument parsing with clap
│   └── toolchain/
│       ├── mod.rs           # Toolchain module exports
│       └── profile.rs       # BuildProfile enum and OptLevel resolution
├── tests/
│   └── cli_integration.rs   # Integration tests
└── README.md                # This file
```

### Phase Orchestration

The `main()` function coordinates the compilation pipeline:

1. Parse command line arguments
2. Validate input (file exists)
3. Apply default normalization (no phase flags → `--codegen -o`)
4. Execute phases in canonical order:
   - Parse: `inference::parse()`
   - Analyze: `inference::type_check()` + `inference::analyze()`
   - Codegen: resolve `OptLevel` via `BuildProfile`, then `inference_wasm_codegen::codegen()` + optional `inference::wasm_to_v()`
5. Generate output files (if requested)
6. Exit with appropriate code

### Error Propagation

The compiler uses `anyhow::Result` for error propagation from library functions. All errors are caught in `main()`, reported to stderr, and cause `process::exit(1)`.

No panics occur during normal operation.

## Related Documentation

- **Main README**: `/README.md` - Project overview and build instructions
- **`infs` CLI**: `/apps/infs/README.md` - Unified toolchain interface
- **Language Spec**: [Inference Language Specification](https://github.com/Inferara/inference-language-spec)
- **Inference Book**: [Getting Started Guide](https://inference-lang.org/book)

## Non-deterministic Programming

Inference provides first-class support for non-deterministic programming patterns through special syntax and WASM instructions.

### Dictionary

- **`@` (Uzumaki)** - Rvalue indicating a variable holds all possible values of the specified type (used only inside non-deterministic blocks)
- **`forall`** - Forall block (all computation paths are reachable)
- **`exists`** - Exists block (at least one computation path is reachable)
- **`assume`** - Assume statement (filters execution paths inside blocks)
- **`unique`** - Unique block (exactly one computation path is reachable)

### WebAssembly Non-deterministic Instructions

The compiler emits custom WebAssembly instructions in the 0xfc prefix space for non-deterministic constructs. See `core/wasm-codegen/src/compiler.rs` documentation.

## Contributing

Contributions are welcome! Please see the main [CONTRIBUTING.md](/CONTRIBUTING.md) for details.

When working on the CLI:
- Maintain comprehensive doc comments for all public interfaces
- Add integration tests for new features
- Follow the existing error handling patterns
- Keep the README synchronized with code changes
