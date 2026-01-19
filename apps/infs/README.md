# infs - Inference Unified CLI

Unified command-line interface for the Inference compiler toolchain.

## Status

Phase 1 implemented. The `build` and `version` commands are functional.

## Implemented Features

- `infs build [file.inf]` - Compile Inference source files
- `infs version` - Display version information
- `infs --help` - Show available commands

## Planned Features (Future Phases)

- `infs install [version]` - Download and install toolchain versions
- `infs uninstall [version]` - Remove toolchain versions
- `infs list` - List installed toolchain versions
- `infs default <version>` - Set default toolchain version
- `infs doctor` - Verify installation health
- `infs new <project>` - Scaffold new projects
- `infs init` - Initialize project in existing directory
- `infs` (no args) - Launch TUI interface
- `infs self update` - Update infs itself

## Usage

### Build Command

```bash
# Parse only
infs build example.inf --parse

# Full compilation with WASM output
infs build example.inf --codegen -o

# Full compilation with Rocq translation
infs build example.inf --codegen -o -v
```

### Build Flags

| Flag | Description |
|------|-------------|
| `--parse` | Run the parse phase to build the typed AST |
| `--analyze` | Run the analyze phase for type checking |
| `--codegen` | Run the codegen phase to emit WebAssembly |
| `-o` | Generate WASM binary file in `out/` directory |
| `-v` | Generate Rocq (.v) translation file |

At least one of `--parse`, `--analyze`, or `--codegen` must be specified.

### Headless Mode

```bash
# For CI/scripting environments
infs --headless
```

When `--headless` is specified or `CI=true` environment variable is detected, TUI mode is disabled.

## Architecture

This crate is a thin binary wrapper that orchestrates:
- `core/inference` - Compilation pipeline (parse, type_check, analyze, codegen, wasm_to_v)

## Building

```bash
cargo build -p infs
```

## Testing

```bash
cargo test -p infs
```

11 integration tests verify:
- Error handling (missing file, no phase selected)
- Build phases (parse, analyze, codegen)
- Output generation (WASM, Rocq)
- Version and help commands
- Byte-identical output compared to legacy `infc`
