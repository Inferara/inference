[![Build](https://github.com/Inferara/inference/actions/workflows/build.yml/badge.svg?branch=main)](https://github.com/Inferara/inference/actions/workflows/build.yml)
[![Coverage](https://github.com/Inferara/inference/actions/workflows/coverage.yml/badge.svg?branch=main)](https://github.com/Inferara/inference/actions/workflows/coverage.yml)

# 🌀 Inference Programming Language

Inference is a programming language designed for building verifiable software. It is featured with static typing, explicit semantics, and formal verification capabilities available out of the box.

> [!IMPORTANT]
> The project is in early development. Internal design and implementation are subject to change. So please be patient with us as we build out the language and tools.

## Learn

- Access our Inference [book](https://inference-lang.org/book) for how to get started guide
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

## Roadmap

Check out open [issues](https://github.com/Inferara/inference/issues).

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for details.
