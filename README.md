![alt text](./assets/inference-logo-oulined-shaped-font.svg)

<div align="center">
   
[![Build](https://github.com/Inferara/inference/actions/workflows/build_main.yml/badge.svg?branch=main)](https://github.com/Inferara/inference/actions/workflows/build_main.yml)
[![Miri Check](https://github.com/Inferara/inference/actions/workflows/miri.yml/badge.svg)](https://github.com/Inferara/inference/actions/workflows/miri.yml)
[![codecov](https://codecov.io/gh/Inferara/inference/branch/main/graph/badge.svg)](https://codecov.io/gh/Inferara/inference)

[![target: SpaceWasm](https://img.shields.io/badge/target-SpaceWasm-654FF0?logo=webassembly&logoColor=white)](book/src/compilation_targets.md#spacewasm)
[![target: Stellar](https://img.shields.io/badge/target-Stellar-000000)](book/src/compilation_targets.md#stellar)

</div>

# 🌀 Inference Programming Language

Inference is a programming language designed for building verifiable software. It is featured with static typing, explicit semantics, and formal verification capabilities available out of the box.

**Inference allows for mathematically verifying code correctness without learning provers. Keep the implementation correct, even with vibecode.**

> [!IMPORTANT]
> The project is in early development. Internal design and implementation are subject to change. So please be patient with us as we build out the language and tools.

## Install

On Linux x64 or macOS Apple Silicon:

```bash
curl -fsSL https://inference-lang.org/install.sh | sh
```

On Windows x64:

```powershell
powershell -ExecutionPolicy Bypass -Command "irm https://inference-lang.org/install.ps1 | iex"
```

The installer puts `infs` in `~/.inference/bin` and adds it to your `PATH`. Then run `infs install` to download the compiler. Prebuilt archives are also attached to every [release](https://github.com/Inferara/inference/releases).

## Editor Support

The official VS Code extension provides syntax highlighting and runs the language server:

[![VS Code Marketplace](https://vsmarketplacebadges.dev/version-short/inference-lang.inference.svg?label=VS%20Code%20Marketplace)](https://marketplace.visualstudio.com/items?itemName=inference-lang.inference)

## Learn

- Inference [homepage](https://inference-lang.org)
- Access our Inference [book](https://inference-lang.org/book) for a guide on how to get started
- Inference Programming Language [specification](https://github.com/Inferara/inference-language-spec)

## Quick Start

```bash
infs new hello && cd hello
infs build    # compiles src/main.inf to out/main.wasm
infs run      # builds, then calls main
```

`infs run` executes a `wasm32` build with [`wasmtime`](https://wasmtime.dev), which must be on your `PATH`, and a `spacewasm` build in process.

## Inference Suite CLI (`infs`)

`infs` is the unified toolchain CLI for Inference.

| Command                                                       | Purpose                                                        |
| ------------------------------------------------------------- | -------------------------------------------------------------- |
| `infs new <name>`, `infs init`                                | Create a project: `Inference.toml` and `src/main.inf`          |
| `infs build [file.inf]`                                       | Compile one file, or the whole project when no file is given   |
| `infs run [file.inf]`                                         | Build, then call `main` (or `--entry-point <name>`)            |
| `infs install`, `uninstall`, `list`, `versions`, `default`    | Manage installed toolchain versions                            |
| `infs component`                                              | Manage optional components such as `wasm-opt`                  |
| `infs self update`                                            | Update `infs` itself                                           |
| `infs doctor`                                                 | Check the installation                                         |
| `infs version`, `infs --version`                              | Show version information                                       |

Run with no arguments in a terminal, `infs` opens an interactive interface.

In project mode, `infs build` finds `Inference.toml` by walking up from the current directory and compiles `src/main.inf` together with every file it reaches through `use` imports. It always runs the full pipeline (parse, analyze, codegen) and writes the WASM binary. Its flags:

- `-v` also writes a Rocq (`.v`) translation, and implies `--mode proof` unless `--mode` is given
- `--mode compile|proof` selects the compilation mode
- `-L <dir>` adds a directory to search for linked `.wasm` modules
- `--no-wasm-opt` skips the project's `[build.wasm-opt]` step

### Compilation Modes

1. **`compile`** produces optimized production binaries. Non-deterministic `spec` nodes are stripped since they have no runtime meaning.
2. **`proof`** produces WASM for formal verification. Spec functions (containing non-deterministic operations) are compiled unoptimized to preserve structural correspondence with the source code for Rocq formalization. Execution functions use the target's release optimization so that proofs cover the actual deployed code.

### Targets

Choose a target with `[build] target` in `Inference.toml`, or with `infc --target`:

| Target             | Output                                                                                                                                                                     |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `wasm32` (default) | A WebAssembly module                                                                                                                                                       |
| `spacewasm`        | The same module, checked against the limits of NASA JPL's [SpaceWasm](https://github.com/nasa/spacewasm) flight interpreter; `infs run` executes it in that interpreter |
| `stellar`          | A deployable Soroban smart contract for the Stellar network                                                                                                                |

Read more about [compilation modes and targets in the book](./book/src/compilation_targets.md).

### Exit Codes

| Code | Meaning                                                  |
| ---- | -------------------------------------------------------- |
| 0    | Success                                                  |
| 1    | Failure: a compile error, an I/O error, a failed download |
| 2    | Invalid command-line arguments                           |

When `infc` or `wasmtime` fails under `infs build` or `infs run`, `infs` exits with its code.

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
├── infs (or infc)          # The CLI binary
├── inference-lsp           # The language server (infc archive only)
└── licenses/               # Third-party license texts
```

The CLI binaries are self-contained and require no external dependencies.

`licenses/` holds the license texts of the SpaceWasm interpreter, which `infs` compiles in to run `spacewasm` builds, and of the one crate the interpreter depends on, with the interpreter's `NOTICE` file. Every archive carries it; [`licenses/README.md`](licenses/README.md) says where each file comes from. `infs self update` and `infs install` keep these notices beside the toolchain they install.

## Building from Source

No external binaries are required. The compiler generates WebAssembly directly via `wasm-encoder`.

```bash
git clone https://github.com/Inferara/inference.git
cd inference
cargo build --release
```

The binaries land in `target/release/`: `infs`, `infc` and `inference-lsp`.

- **`cargo build`** / **`cargo test`** - The `core/`, `ide/` and `apps/` crates and the `tests/` integration suite
- **`cargo build-full`** / **`cargo test-full`** - The whole workspace, including `tools/`

[CONTRIBUTING.md](CONTRIBUTING.md) covers the test conventions and the Docker-only Rocq discharge gate.

## Roadmap

Check out open [issues](https://github.com/Inferara/inference/issues).

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for details.
