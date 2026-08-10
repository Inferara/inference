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
| `infs build` | Compile project entry point (`src/main.inf`) to WASM (project mode) |
| `infs build <file>` | Compile a single source file to WASM (single-file mode) |
| `infs run` | Build project entry point and execute with wasmtime (project mode) |
| `infs run <file>` | Build and execute a single source file with wasmtime |

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
| `infs component add <name>` | Install a managed component (currently `wasm-opt`, the Binaryen optimizer behind `[build.wasm-opt]`) |
| `infs component list` | List managed components and their install state |
| `infs component remove <name>` | Remove an installed managed component |

### Other

| Command | Description |
|---------|-------------|
| `infs version` | Display version information |
| `infs` (no args) | Launch interactive TUI |

## Usage Examples

### Build Command

`infs build` supports two modes:

**Project mode** (no path): discovers `Inference.toml` by walking up from the current directory and compiles `src/main.inf` together with its full import-reachable closure into a single `out/main.wasm`. The compiler (`infc`) follows every `use` directive transitively; unreachable `src/**/*.inf` files produce a warning from `infc`. The manifest's `[build] mode` and `[verification] output-dir` are consumed as configuration (CLI flags override).

```bash
# Project mode: compile <root>/src/main.inf (+ imported files) -> <root>/out/main.wasm
infs build

# Project mode: proof build using [build] mode = "proof" from Inference.toml
# Both .wasm and .v land under <root>/proofs/ (the default output-dir)
infs build --mode proof

# CLI override: --mode compile wins over any manifest setting
infs build --mode compile
```

**Single-file mode** (path provided): the historical behavior — compiles exactly the given file.

```bash
# Full compilation with WASM output (default — no flags needed)
infs build example.inf

# Full compilation with Rocq translation
infs build example.inf -v

# Parse only (syntax check)
infs build example.inf --parse

# Type checking
infs build example.inf --analyze
```

### Build Flags

| Flag | Description |
|------|-------------|
| `--parse` | Run the parse phase to build the typed AST (overrides default) |
| `--analyze` | Run the analyze phase for type checking (overrides default) |
| `--codegen` | Run the codegen phase to emit WebAssembly |
| `-o` | Generate WASM binary file in `out/` directory |
| `-v` | Generate Rocq (.v) translation file |
| `--mode proof` | Proof mode: preserve non-det specs; implies `-v` inside `infc` |
| `--mode compile` | Compile mode: strip specs for executable WASM |
| `-L <dir>` / `--wasm-lib-dir <dir>` | Directory to search for external `.wasm` modules referenced by `use { … } from <module>;`; repeatable. In project mode a relative dir is anchored to the directory you invoked `infs` from, not the project root |
| `--no-wasm-opt` | Skip `[build.wasm-opt]` post-build optimization (project mode only) |

When no phase flag is given, `infs build` defaults to full compilation and writes the WASM binary to disk — equivalent to `--codegen -o`.

### Project-mode Manifest Semantics

When `infs build` runs in project mode, it reads fields from `Inference.toml` to resolve the build configuration:

| Manifest field | Effect |
|----------------|--------|
| `[build] mode = "proof"` | Forwards `--mode proof` to `infc`; activates `output-dir` |
| `[build] mode = "compile"` (default) | Forwards nothing; `infc` defaults to compile mode |
| `[verification] output-dir` | Honored only in effective-proof mode; relocates both `.wasm` and `.v` |
| `[build] wasm-features` | Opt-in post-MVP WebAssembly proposals (currently `"bulk-memory"`); forwarded as `--wasm-features`, echoed as a `wasm-features:` line, and applied in both compile and proof mode. Empty (the default) means pure WebAssembly 1.0 |
| `[wasm-dependencies]` | Each entry is forwarded as `--wasm-dep <name>=<path>`, with the declared path resolved against the project root; honored on every build and run path, and never capability-gated |
| `[build.wasm-opt]` | Opt-in post-build optimization of `out/main.wasm` via Binaryen `wasm-opt`, resolved from `WASM_OPT_PATH` → PATH → an infs-managed install; absent table is a no-op |

Some of these are also honored in **single-file** mode, by walking up to the nearest `Inference.toml`. `[build] wasm-features` is honored by both `infs build <path>` and `infs run <path>` — deliberately, since those two and project `infs build` all write `out/main.wasm` for the same project and must not disagree about its instruction set. `[wasm-dependencies]` is honored on all four compilation paths — single-file `build`, single-file `run`, project `build`, and project `run` — and is never capability-gated. `[build] wasm-features` requires an `infc` with ABI 1.2 or newer; an older compiler cannot honor the request, so it is refused with remediation instead of being handed the flag.

Every table with a fixed set of fields rejects keys it does not recognize, so a misspelled manifest key is an error naming the offending key and the accepted ones, not a setting that silently does nothing.

CLI flags always override manifest settings. `infs run` ignores `[build] mode` entirely and always builds an executable in `out/` (proof-mode WASM contains non-deterministic opcodes that `wasmtime` cannot execute) — but it does honor `[build.wasm-opt]`, since `run` optimizes exactly what it then executes. `[build.wasm-opt]` applies only to compile-mode artifacts (proof-mode and `-v` builds always skip it) and can be skipped for a single invocation with `--no-wasm-opt`. `wasm-opt` itself does not need to be preinstalled: run `infs component add wasm-opt` to provision the pinned, checksum-verified Binaryen up front, or set `auto-install = true` under `[build.wasm-opt]` to have `infs` download it automatically the first time a build needs it. See [`docs/inference-toml.md`](docs/inference-toml.md) for the full field reference, including the non-deterministic-construct hard error and the complete `wasm-opt` resolution order.

### Run Command

`infs run` supports the same two modes as `build`:

**Project mode** (no path): builds `src/main.inf` in compile mode and invokes `main`.

```bash
# Project mode: build + invoke main
infs run

# Single-file mode: build and invoke main
infs run example.inf

# Single-file mode: invoke a custom entry point
infs run example.inf --entry-point helper

# Single-file mode: search libs/ for external .wasm modules
infs run example.inf -L libs

# Pass arguments to the program (single-file only)
infs run example.inf -- arg1 arg2

# Project mode: build, but skip [build.wasm-opt] for this run
infs run --no-wasm-opt
```

### Run Flags

| Flag | Description |
|------|-------------|
| `--entry-point <name>` | Function to invoke in single-file mode (default `main`); project mode always invokes `main`, and requesting anything else is rejected with guidance to use single-file mode |
| `-L <dir>` / `--wasm-lib-dir <dir>` | Directory to search for external `.wasm` modules referenced by `use { … } from <module>;`; repeatable. In project mode a relative dir is anchored to the directory you invoked `infs` from; in single-file mode no anchoring step runs, because `infc` already inherits that directory as its working directory |
| `--no-wasm-opt` | Skip `[build.wasm-opt]` post-build optimization (project mode only) |

In single-file mode, options must appear before the first bare trailing token: everything from that token onward — including anything that looks like a flag — is passed to the invoked function instead of being parsed by `infs run`. `infs run f.inf -L libs 1` parses `-L libs` and passes `1` to the function; `infs run f.inf 1 -L libs` passes `1 -L libs` verbatim and parses no `-L` at all. Use `--` to pass arguments that themselves start with `-` unambiguously.

Requires `wasmtime` to be installed. In both modes, `infs run` resolves the enclosing or discovered manifest's `[wasm-dependencies]` and forwards any `-L` directories, so a file or project binding `use { … } from <module>` runs without a separate link step. In project mode, `infs run` also applies the same `[build.wasm-opt]` post-build optimization as `infs build` before executing the result.

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

On first install, `infs install` automatically adds the toolchain binary to your system PATH:

- **Unix (Linux/macOS)**: Modifies shell profile (`~/.bashrc`, `~/.zshrc`, or `~/.config/fish/config.fish`)
- **Windows**: Updates user PATH in registry (`HKCU\Environment\Path`)

The toolchain binary is symlinked to `~/.inference/bin/` and made accessible system-wide:
- `infc` - Inference compiler

After installation completes, restart your terminal or run:

```bash
# Linux/macOS with bash
source ~/.bashrc

# Linux/macOS with zsh
source ~/.zshrc

# Windows
# Close and reopen terminal
```

Manual PATH configuration is no longer required. The installed binary will be available in new terminal sessions.

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

### Module Organization

| Module | Description |
|--------|-------------|
| `commands::build` | `infs build`: single-file and project-mode compilation |
| `commands::run` | `infs run`: compile + execute via wasmtime |
| `commands::project_build` | Shared project-build helper (spawn, ABI handshake, `--out-dir` gate) |
| `commands::new` | `infs new`: scaffold a project in a new directory |
| `commands::init` | `infs init`: initialize the current directory as a project |
| `project::manifest` | `Inference.toml` parsing, validation, and discovery |
| `project::scaffold` | File/directory creation for new projects |
| `toolchain` | Toolchain version management and `infc` resolution |

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
| 2 | Sibling of `infs` | An `infc` in the same directory as the running `infs`, whatever that directory is named |
| 3 | System PATH | Searches for `infc` in system PATH via `which` |
| 4 (lowest) | Managed toolchain | Uses `~/.inference/toolchains/VERSION/infc` |

`infs doctor` reports which priority fired on its `Resolved infc` line.

### When to Use Each

**Priority 1 - INFC_PATH**: Use for development, testing, or CI/CD with a pre-built binary:
```bash
export INFC_PATH=/path/to/custom/infc
infs build example.inf
```

**Priority 2 - Sibling of `infs`**: Automatic, and what makes `./target/debug/infs build foo.inf` use the compiler you just built rather than an installed one. Only adjacency is checked, so it holds regardless of `CARGO_TARGET_DIR`, `--target-dir`, the cargo profile, or the host platform. It also applies to installed layouts, where `infs` and `infc` share a directory. Set `INFC_PATH` to override it.

Because this tier assumes the two binaries were built together rather than proving it, the compatibility handshake checks the assumption: if the adjacent `infc` reports a different build commit, the build warns and names both. The other tiers stay silent on a commit mismatch — for a pinned, installed, or managed `infc` a differing commit is normal. The check catches cross-commit drift only; two binaries built from the same commit with different working trees are indistinguishable to it.

**Priority 3 - System PATH**: Automatic if `infc` is installed system-wide (e.g., via package manager).

**Priority 4 - Managed Toolchain**: Default for end users after running `infs install`:
```bash
infs install           # Downloads to ~/.inference/toolchains/
infs default 0.1.0     # Sets default version
infs build example.inf # Uses managed toolchain
```

### Environment Variables

| Variable | Purpose |
|----------|---------|
| `INFS_NO_TUI` | Disable interactive TUI (any value) |
| `INFS_VERBOSE` | Trace `infc` resolution to stderr — which priority resolved it, and when the sibling priority declined (any non-empty value other than `0`) |
| `INFC_PATH` | Explicit path to `infc` binary (priority 1) |
| `INFERENCE_HOME` | Toolchain directory (default: `~/.inference`) |
| `INFS_DIST_SERVER` | Distribution server URL (default: `https://inference-lang.org`) |
| `INFERENCE_TEST_INFC` | Test-only: pins the `infc` the integration suite spawns, overriding its probing |

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

Tests cover:
- Command argument parsing
- Build phases (parse, analyze, codegen)
- Output generation (WASM, Rocq)
- Project scaffolding
- Project-mode build and run (manifest semantics, output-dir, mode override)
- Multi-file project builds: import closure compiled to single `out/main.wasm`, unreachable-file warning surfaced, missing-import error with nearest-match suggestion, `-v` proof-mode output for multi-file projects
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

These tests skip gracefully when external tools or platforms are unavailable, except when `CI` is set to
a non-empty, non-`0` value: a missing `infc` or `wasmtime` then fails the run rather than skipping, because
cargo captures the skip notice and a skipped test is otherwise indistinguishable from a passing one. The
`wasm-opt` gate is the deliberate exception — no CI runner provides Binaryen — so it always skips softly.

### Manual QA Tests

9 tests require manual verification and are documented in `docs/qa-test-suite.md`:
- TUI visual verification
- Verify command (requires coqc)
- Self-update (requires actual distribution server)
- Cross-platform builds (requires CI on each platform)
- Disk full and permission scenarios
