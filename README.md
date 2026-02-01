[![Build](https://github.com/Inferara/inference/actions/workflows/build_main.yml/badge.svg?branch=main)](https://github.com/Inferara/inference/actions/workflows/build_main.yml)
[![codecov](https://codecov.io/gh/Inferara/inference/branch/main/graph/badge.svg)](https://codecov.io/gh/Inferara/inference)

# 🌀 Inference Programming Language

Inference is a programming language designed for building verifiable software. It is featured with static typing, explicit semantics, and formal verification capabilities available out of the box.

**Inference allows for mathematically verifying code correctness without learning provers. Keep the implementation correct, even with vibecode.**

> [!IMPORTANT]
> The project is in early development. Internal design and implementation are subject to change. So please be patient with us as we build out the language and tools.

## Editor Support

Install the official VS Code extension for syntax highlighting:

[![VS Code Marketplace](https://img.shields.io/visual-studio-marketplace/v/inference-lang.inference?label=VS%20Code%20Marketplace)](https://marketplace.visualstudio.com/items?itemName=inference-lang.inference)

## Learn

- Inference [homepage](https://inference-lang.org)
- Access our Inference [book](https://inference-lang.org/book) for a guide on how to get started
- Inference Programming Language [specification](https://github.com/Inferara/inference-language-spec)

## Inference Suite CLI (`infs`)

`infs` is the unified toolchain CLI for Inference. It provides subcommands for building, managing, and working with Inference projects.

### Build Command

The `infs build` command compiles a single `.inf` source file through three phases:

1. **Parse** (`--parse`) – Build the typed AST using tree-sitter
2. **Analyze** (`--analyze`) – Perform type checking and semantic validation (WIP)
3. **Codegen** (`--codegen`) – Emit WebAssembly binary with optional Rocq translation

You must specify at least one phase flag; phases run in canonical order (parse → analyze → codegen).

### Basic Usage

```bash
# Via cargo
cargo run -p infs -- build path/to/file.inf --parse

# After building, call the binary directly
./target/debug/infs build path/to/file.inf --codegen -o
```

### Compilation Modes

The compiler supports two modes that control optimization and verification behavior:

| Mode | Has non-det operations | Behavior |
|------|------------------------|----------|
| `compile` | no | Compile with the chosen target and its default optimization |
| `compile` | yes | Exclude `spec` nodes from codegen, compile remaining code normally |
| `proof` | yes | Compile all code strictly without LLVM optimizations for formalization |
| `proof` | no | Same as above — all code unoptimized for formalization |

**`compile`** produces optimized production binaries. Non-deterministic `spec` nodes are stripped since they have no runtime meaning.

**`proof`** produces literal, unoptimized WASM that preserves structural correspondence with the source code for Rocq formal verification.

### Output Flags

- `-o` – Generate WASM binary file in `out/` directory
- `-v` – Generate Rocq (.v) translation file in `out/` directory

### Show Version

```bash
infs version
infs --version
```

### Exit Codes

| Code | Meaning                         |
|------|---------------------------------|
| 0    | Success                         |
| 1    | Usage / IO / Parse failure      |

### Future Commands (Planned)

- `infs install` – Download and install toolchain versions
- `infs new` – Scaffold new projects
- `infs doctor` – Verify installation health
- `infs` (no args) – Launch TUI interface

## Distribution

Prebuilt binaries are available for each release. Two CLI tools are distributed:

- **`infs`** - Full-featured toolchain CLI (recommended for all users)
- **`infc`** - Standalone compiler CLI (for direct compilation)

### Release Artifacts

| Platform | infs | infc |
|----------|------|------|
| Linux x64 | `infs-linux-x64.tar.gz` | `infc-linux-x64.tar.gz` |
| Windows x64 | `infs-windows-x64.zip` | `infc-windows-x64.zip` |
| macOS ARM64 | `infs-macos-apple-silicon.tar.gz` | `infc-macos-apple-silicon.tar.gz` |

### Directory Structure

```
<distribution-folder>/
├── infs (or infc)          # The CLI binary
├── bin/
│   ├── inf-llc            # LLVM compiler with Inference intrinsics
│   └── rust-lld           # WebAssembly linker
└── lib/                   # (Linux only)
    └── libLLVM.so.*       # LLVM shared library
```

**Notes:**
- On Linux, the LLVM shared library must be in the `lib/` directory.
- On Windows, all required DLL files should be placed in the `bin/` directory next to the executables.
- The CLI binaries automatically locate dependencies relative to their own location.
- No system LLVM installation is required for end users.

## Building from Source

To build Inference from source, you'll need the required binary dependencies for your platform.

For detailed platform-specific setup instructions, see:
- [Linux Development Setup](book/installation_linux.md)
- [macOS Development Setup](book/installation_macos.md)
- [Windows Development Setup](book/installation_windows.md)

### Required Binaries

Download the following files for your platform and place them in the specified directories:

#### Linux
- **inf-llc**: [Download](https://storage.googleapis.com/external_binaries/linux/bin/inf-llc.zip) → Extract to `external/bin/linux/`
- **rust-lld**: [Download](https://storage.googleapis.com/external_binaries/linux/bin/rust-lld.zip) → Extract to `external/bin/linux/`
- **libLLVM**: [Download](https://storage.googleapis.com/external_binaries/linux/lib/libLLVM.so.21.1-rust-1.94.0-nightly.zip) → Extract to `external/lib/linux/`

#### macOS
- **inf-llc**: [Download](https://storage.googleapis.com/external_binaries/macos/bin/inf-llc.zip) → Extract to `external/bin/macos/`
- **rust-lld**: [Download](https://storage.googleapis.com/external_binaries/macos/bin/rust-lld.zip) → Extract to `external/bin/macos/`

#### Windows
- **inf-llc.exe**: [Download](https://storage.googleapis.com/external_binaries/windows/bin/inf-llc.zip) → Extract to `external/bin/windows/`
- **rust-lld.exe**: [Download](https://storage.googleapis.com/external_binaries/windows/bin/rust-lld.zip) → Extract to `external/bin/windows/`

### Build Steps

1. Clone the repository:
   ```bash
   git clone https://github.com/Inferara/inference.git
   cd inference
   ```

2. Download and extract the required binaries for your platform (see links above)

3. Make the binaries executable (Linux/macOS only):
   ```bash
   chmod +x external/bin/linux/inf-llc external/bin/linux/rust-lld    # Linux
   chmod +x external/bin/macos/inf-llc external/bin/macos/rust-lld    # macOS
   ```

4. Build the project:
   ```bash
   cargo build --release
   ```

The compiled binaries will be in `target/release/` (`infs` and `infc`).

### Build Commands

The workspace is configured for efficient development:

- **`cargo build`** - Builds only the `core/` crates (faster for core development)
- **`cargo build-full`** - Builds the entire workspace, including tools and tests
- **`cargo test`** - Runs tests for `core/` crates and the `tests/` integration suite
- **`cargo test-full`** - Runs tests for all workspace members, including tools

## Roadmap

Check out open [issues](https://github.com/Inferara/inference/issues).

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for details.
