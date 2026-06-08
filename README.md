![alt text](./assets/inference-logo-oulined-shaped-font.svg)

<div align="center">
   
[![Build](https://github.com/Inferara/inference/actions/workflows/build_main.yml/badge.svg?branch=main)](https://github.com/Inferara/inference/actions/workflows/build_main.yml)
[![Miri Check](https://github.com/Inferara/inference/actions/workflows/miri.yml/badge.svg)](https://github.com/Inferara/inference/actions/workflows/miri.yml)
[![codecov](https://codecov.io/gh/Inferara/inference/branch/main/graph/badge.svg)](https://codecov.io/gh/Inferara/inference)

</div>

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

The `infs build` command compiles Inference source through three phases:

1. **Parse** (`--parse`) – Build the typed AST with the `inference-parser`
2. **Analyze** (`--analyze`) – Perform type checking, static analysis, and semantic validation
3. **Codegen** (`--codegen`) – Emit WebAssembly binary with optional Rocq translation

Phases run in canonical order (parse → analyze → codegen). When no phase flag is given, `infs build` defaults to full compilation and writes the WASM binary to disk.

`infs build` operates in two modes. Given a path, it compiles that single file. With no path, it runs in **project mode**: it discovers the project's `Inference.toml` by walking up from the current directory and compiles `src/main.inf`, with output rooted at the project directory. `infs run` mirrors the same two modes.

### Basic Usage

```bash
# Full compilation (default — no flags needed)
./target/debug/infs build path/to/file.inf

# Parse only (syntax check)
cargo run -p infs -- build path/to/file.inf --parse
```

### Compilation Modes

The compiler supports two modes that control optimization and verification behavior:

1. **`compile`** produces optimized production binaries. Non-deterministic `spec` nodes are stripped since they have no runtime meaning.
2. **`proof`** produces WASM for formal verification. Spec functions (containing non-deterministic operations) are compiled unoptimized to preserve structural correspondence with the source code for Rocq formalization. Execution functions use the target's release optimization so that proofs cover the actual deployed code.

Read more about [compilation modes in the book](./book/src/compilation_targets.md).

### Output Flags

- `-o` – Generate WASM binary file in `out/` directory
- `-v` – Generate Rocq (.v) translation file in `out/` directory

### Show Version

```bash
infs version
infs --version
```

### Exit Codes

| Code | Meaning                    |
| ---- | -------------------------- |
| 0    | Success                    |
| 1    | Usage / IO / Parse failure |

### Future Commands (Planned)

- `infs install` – Download and install toolchain versions
- `infs new` – Scaffold new projects
- `infs doctor` – Verify installation health
- `infs` (no args) – Launch TUI interface

## Distribution

Prebuilt binaries are available for each release. Two CLI tools are distributed:

- **`infs`** - Full-featured toolchain CLI (recommended for all users)
- **`infc`** - Standalone compiler CLI

### Release Artifacts

| Platform    | infs                              | infc                              |
| ----------- | --------------------------------- | --------------------------------- |
| Linux x64   | `infs-linux-x64.tar.gz`           | `infc-linux-x64.tar.gz`           |
| Windows x64 | `infs-windows-x64.zip`            | `infc-windows-x64.zip`            |
| macOS ARM64 | `infs-macos-apple-silicon.tar.gz` | `infc-macos-apple-silicon.tar.gz` |

### Directory Structure

```
<distribution-folder>/
└── infs (or infc)          # The CLI binary
```

The CLI binaries are self-contained and require no external dependencies.

## Building from Source

To build Inference from source:

For detailed platform-specific setup instructions, see:

- [Linux Development Setup](book/installation_linux.md)
- [macOS Development Setup](book/installation_macos.md)
- [Windows Development Setup](book/installation_windows.md)

### Dependencies

No external binaries are required. The compiler generates WebAssembly directly via `wasm-encoder`.

### Build Steps

1. Clone the repository:

   ```bash
   git clone https://github.com/Inferara/inference.git
   cd inference
   ```

2. Build the project:

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
