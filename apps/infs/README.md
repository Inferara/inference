# Inferense Start!

Unified command-line interface for the Inference compiler toolchain.

## Features

- **Compilation**: Build and run Inference projects
- **Project Management**: Create and initialize Inference projects
- **Toolchain Management**: Install, uninstall, and switch between toolchain versions
- **Interactive TUI**: Terminal user interface for visual project management
- **Doctor**: Diagnose installation and environment issues

## Installation

```bash
cargo install --path apps/infs
```

Or build from source:

```bash
cargo build -p infs --release
```

## Commands

### Compilation

| Command | Description |
|---------|-------------|
| `infs build <file>` | Compile Inference source files to WASM |
| `infs run <file>` | Build and execute with wasmtime |

### Project Management

| Command | Description |
|---------|-------------|
| `infs new <name>` | Create a new project in a new directory |
| `infs init` | Initialize a project in current directory |

### Toolchain Management

| Command | Description |
|---------|-------------|
| `infs install [version]` | Install a toolchain version (latest stable, or latest if no stable) |
| `infs uninstall <version>` | Remove an installed toolchain |
| `infs list` | List installed toolchains |
| `infs versions` | List available toolchain versions from server |
| `infs default <version>` | Set the default toolchain |
| `infs doctor` | Check installation health with intelligent recommendations |
| `infs self update` | Update infs itself |

### Other

| Command | Description |
|---------|-------------|
| `infs version` | Display version information |
| `infs` (no args) | Launch interactive TUI |

## Usage Examples

### Build Command

```bash
# Parse only (syntax check)
infs build example.inf --parse

# Type checking
infs build example.inf --analyze

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

### Run Command

```bash
# Build and execute
infs run example.inf

# Pass arguments to the program
infs run example.inf -- arg1 arg2
```

Requires `wasmtime` to be installed.

### Project Commands

```bash
# Create a new project (with git initialization)
infs new myproject

# Create a new project without git
infs new myproject --no-git

# Initialize in current directory
# If .git/ exists, creates .gitignore and .gitkeep files
infs init
```

### Toolchain Commands

```bash
# Install latest stable toolchain (or latest if no stable versions exist)
# First install automatically configures PATH
infs install

# Install specific version
infs install 0.1.0

# If a version is already installed but no default is set,
# infs install automatically sets it as default
infs install  # Sets existing toolchain as default if needed

# List installed versions
infs list

# List available versions from server
infs versions

# List only stable versions
infs versions --stable

# Set default version
infs default 0.1.0

# Check installation health
# Provides intelligent suggestions based on your current state
infs doctor
```

**Automatic PATH Configuration:**

On first install, `infs install` automatically adds the toolchain binaries to your system PATH:

- **Unix (Linux/macOS)**: Modifies shell profile (`~/.bashrc`, `~/.zshrc`, or `~/.config/fish/config.fish`)
- **Windows**: Updates user PATH in registry (`HKCU\Environment\Path`)

The toolchain binaries are symlinked to `~/.inference/bin/` and made accessible system-wide:
- `infc` - Inference compiler
- `inf-llc` - LLVM backend
- `rust-lld` - WebAssembly linker

After installation completes, restart your terminal or run:

```bash
# Linux/macOS with bash
source ~/.bashrc

# Linux/macOS with zsh
source ~/.zshrc

# Windows
# Close and reopen terminal
```

Manual PATH configuration is no longer required. The installed binaries will be available in new terminal sessions.

## Interactive TUI

>[!WARNING]
>Experimental

When run without arguments in an interactive terminal, `infs` launches a TUI:

```bash
infs
```

The TUI provides:
- Command menu with keyboard navigation
- Toolchain status and management
- Project overview
- Build/run integration

### TUI Controls

| Key | Action |
|-----|--------|
| `↑`/`↓` or `j`/`k` | Navigate menu |
| `Enter` | Select command |
| `q` or `Esc` | Quit |

### Headless Mode

The TUI is automatically disabled in non-interactive environments:
- When `INFS_NO_TUI` environment variable is set (any value)
- When stdout is not a terminal

Force headless mode explicitly:

```bash
infs --headless
```

Or via environment variable:

```bash
INFS_NO_TUI=1 infs
```

## Architecture

This crate is the unified CLI that orchestrates:

- **`core/inference`** - Compilation pipeline (parse, type_check, analyze, codegen, wasm_to_v)
- **Toolchain management** - Version installation and switching
- **Project scaffolding** - Project creation and initialization

### External Dependencies

Some commands require external tools:

| Command | Requires |
|---------|----------|
| `infs run` | wasmtime |

Run `infs doctor` to check if all dependencies are available.

## Compiler Resolution

When running `build`, `run` commands, `infs` locates the `infc` compiler using the following priority order:

| Priority | Source | Description |
|----------|--------|-------------|
| 1 (highest) | `INFC_PATH` env var | Explicit path to a specific `infc` binary |
| 2 | System PATH | Searches for `infc` in system PATH via `which` |
| 3 (lowest) | Managed toolchain | Uses `~/.inference/toolchains/VERSION/bin/infc` |

### When to Use Each

**Priority 1 - INFC_PATH**: Use for development, testing, or CI/CD with a pre-built binary:
```bash
export INFC_PATH=/path/to/custom/infc
infs build example.inf --codegen -o
```

**Priority 2 - System PATH**: Automatic if `infc` is installed system-wide (e.g., via package manager).

**Priority 3 - Managed Toolchain**: Default for end users after running `infs install`:
```bash
infs install           # Downloads to ~/.inference/toolchains/
infs default 0.1.0     # Sets default version
infs build example.inf # Uses managed toolchain
```

### Environment Variables

| Variable | Purpose |
|----------|---------|
| `INFS_NO_TUI` | Disable interactive TUI (any value) |
| `INFC_PATH` | Explicit path to `infc` binary (priority 1) |
| `INFERENCE_HOME` | Toolchain directory (default: `~/.inference`) |
| `INFS_DIST_SERVER` | Distribution server URL (default: `https://inference-lang.org`) |

### Release Manifest Format

The `releases.json` manifest uses a simplified format with only 2 required fields per file entry:

```json
[
  {
    "version": "0.2.0",
    "stable": true,
    "files": [
      {
        "url": "https://github.com/Inferara/inference/releases/download/v0.2.0/infc-linux-x64.tar.gz",
        "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
      }
    ]
  }
]
```

**Field Descriptions:**

Per-version fields:
- `version` (string): Semantic version string (e.g., `0.2.0`, `0.3.0-alpha`)
- `stable` (boolean): Whether this is a stable release. When running `infs install` without a version argument, the latest stable version is preferred. If no stable versions exist, the latest version is used regardless of stability.

Per-file fields (required):
- `url` (string): Full download URL to the release artifact
- `sha256` (string): SHA256 checksum for integrity verification

Derived fields (extracted from URL automatically):
- `filename`: Last path segment of URL (e.g., `infc-linux-x64.tar.gz`)
- `tool`: First segment of filename before `-` (e.g., `infc`, `infs`)
- `os`: Second segment of filename (e.g., `linux`, `macos`, `windows`)

**Naming Convention:**

Artifact filenames must follow the pattern: `{tool}-{os}-{arch}.{ext}`

Examples:
- `infc-linux-x64.tar.gz`
- `infs-windows-x64.zip`
- `infc-macos-apple-silicon.tar.gz`

This allows the toolchain manager to automatically detect platform compatibility without explicit platform fields in the manifest.

## Development

### Building

```bash
cargo build -p infs
```

### Testing

```bash
cargo test -p infs
```

390 tests (321 unit + 69 integration) cover:
- Command argument parsing
- Build phases (parse, analyze, codegen)
- Output generation (WASM, Rocq)
- Project scaffolding
- Toolchain management operations
- TUI navigation and command execution
- TUI rendering with TestBackend
- Non-deterministic features (forall, exists, assume, unique, uzumaki)
- Error handling and edge cases
- Environment variable handling
- Byte-identical output compared to legacy `infc`

### Test Fixtures

Test fixtures are located in `tests/fixtures/`:

| File | Purpose |
|------|---------|
| `trivial.inf` | Simple valid program |
| `example.inf` | Complex example with multiple functions |
| `nondet.inf` | Non-deterministic features (forall, exists, assume, unique) |
| `syntax_error.inf` | Syntax error handling |
| `type_error.inf` | Type error handling |
| `empty.inf` | Empty file edge case |
| `uzumaki.inf` | Uzumaki operator (`@`) |
| `forall_test.inf` | Forall block compilation |
| `exists_test.inf` | Exists block compilation |
| `assume_test.inf` | Assume block compilation |
| `unique_test.inf` | Unique block compilation |

### Integration Tests

Some integration tests are conditional:
- `run_full_workflow_with_wasmtime` - requires wasmtime
- Unix-specific tests (permissions) - `#[cfg(unix)]`

These tests skip gracefully when external tools or platforms are unavailable.

### Manual QA Tests

9 tests require manual verification and are documented in `docs/qa-test-suite.md`:
- TUI visual verification
- Verify command (requires coqc)
- Self-update (requires actual distribution server)
- Cross-platform builds (requires CI on each platform)
- Disk full and permission scenarios
