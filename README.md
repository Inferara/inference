[![Build](https://github.com/Inferara/inference/actions/workflows/build_main.yml/badge.svg?branch=main)](https://github.com/Inferara/inference/actions/workflows/build_main.yml)
[![codecov](https://codecov.io/gh/Inferara/inference/branch/main/graph/badge.svg)](https://codecov.io/gh/Inferara/inference)

# 🌀 Inference Programming Language

Inference is a programming language designed for building verifiable software. It is featured with static typing, explicit semantics, and formal verification capabilities available out of the box.

**Inference allows for mathematically verifying code correctness without learning provers. Keep the implementation correct, even with vibecode.**

> [!IMPORTANT]
> The project is in early development. Internal design and implementation are subject to change. So please be patient with us as we build out the language and tools.

## Learn

- Inference [homepage](https://inference-lang.org)
- Access our Inference [book](https://inference-lang.org/book) for a guide on how to get started
- Inference Programming Language [specification](https://github.com/Inferara/inference-language-spec)

## Inference Compiler CLI (`infc`)

`infc` drives the compilation pipeline for a single `.inf` source file. Phases are:

1. Parse (`--parse`) – build the typed AST.
2. Analyze (`--analyze`) – perform semantic/type inference (WIP).
3. Codegen (`--codegen`) – emit WebAssembly and optionally translate to `.v` when `-o` is supplied.

You must specify at least one phase flag; requested phases run in canonical order.

### Basic usage

```bash
cargo run -p inference-cli -- infc path/to/file.inf --parse
```

After building you can call the binary directly:

```bash
./infc path/to/file.inf --codegen -o
```

### Show version

```bash
infc --version
```

### Output artifacts

Artifacts are written to an `out/` directory relative to the working directory. Rocq translation output is `out/out.v`.

### Exit codes

| Code | Meaning                         |
|------|---------------------------------|
| 0    | Success                         |
| 1    | Usage / IO / Parse failure      |

## Distribution

Prebuilt `infc` binaries distributables are arranged in the following directory structure:

```
<distribution-folder>/
├── infc                    # The main CLI binary
├── bin/
│   ├── inf-llc            # LLVM compiler with Inference intrinsics
│   └── rust-lld           # WebAssembly linker
└── lib/                   # (Linux only)
    └── libLLVM.so.*       # LLVM shared library
```

**Notes:**
- On Linux, the LLVM shared library must be in the `lib/` directory.
- On Windows, all required DLL files should be placed in the `bin/` directory next to the executables.
- The `infc` binary automatically locates these dependencies relative to its own location.
- No system LLVM installation is required for end users.

## Building from Source

To build Inference from source, you'll need the required binary dependencies for your platform.

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

The compiled `infc` binary will be in `target/release/infc`.

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
