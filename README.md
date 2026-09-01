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

`infs build` operates in two modes. Given a path, it compiles that single file. With no path, it runs in **project mode**: it discovers the project's `Inference.toml` by walking up from the current directory and compiles `src/main.inf` together with every file it reaches through `use` imports — the project's module hierarchy — with output rooted at the project directory. `infs run` mirrors the same two modes.

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

### Docker-only Rocq discharge development gate

The local emitted-Rocq discharge gate is orchestrated by Docker; it does not use a host Rust or Rocq toolchain. Its public interface is:

```bash
./ci/rocq-discharge-docker.sh \
  --wasm-verifier /absolute/path/to/wasm-verifier \
  --container wasm-verifier-coq \
  [--adapter batch|single|both] \
  [--full]
```

`--adapter` defaults to `both`. `batch` sends one immutable exchange volume to the verifier bridge. `single` makes the five ordered per-case bridge calls with a new empty `0700` receipt directory for each call. `both` verifies the batch receipts, removes only the validated receipt set, runs the five single calls, and verifies the replacement receipts. Inference fingerprints `request.json` and all five ordered raw Rocq inputs before and after every bridge call; it never parses verifier-private proof logs.

The wrapper composes [`ci/rocq-rust-docker.sh`](ci/rocq-rust-docker.sh), preserving that lane's target lock, source snapshot, persistent Cargo registry/target volumes, pinned Rust image and explicit Rust `1.98.0` toolchain. [`ci/rocq-discharge.cargo-lock`](ci/rocq-discharge.cargo-lock) is the authoritative tracked lock for this lane. The ignored root `Cargo.lock` is excluded from the snapshot and is never an input to the gate. Fetch is the only networked Rust step; all compilation and execution after fetch are locked, offline, socket-free, and run with a read-only root filesystem, dropped capabilities, `no-new-privileges`, and a private `/tmp`.

`--full` first runs the focused dischargeability tests, then the adapter flow, then the complete `inference-tests` crate (not the whole workspace). The clean Docker floor is exactly five Cargo `test result:` lines with at least 3,075 passed tests in aggregate and zero failed or filtered tests. Empty, single-binary, malformed, filtered, and under-floor logs fail closed.

The verifier input must be an absolute canonical, clean `wasm-verifier` checkout whose `HEAD` equals the revision in [`core/wasm-to-v/wasm-verifier-pin.txt`](core/wasm-to-v/wasm-verifier-pin.txt). The supplied running container must implement the future verifier-side bridge contract: its strict `ci/discharge/container-pin.json` records protocol 1, the image reference and ID, `coq` user, repository mount destination, and exact supported Coq 8.20 patch version. Live inspection must also show that the canonical checkout is the mount source, `coq` has nonzero uid/gid, and the verifier revision plus observed `coq-wasm` tag/revision match Inference's pin.

Phase A intentionally ships before those verifier-side bridge scripts, container inspection script, and `container-pin.json` exist. A normal invocation therefore fails closed with a bounded missing-prerequisite diagnostic; this phase does **not** claim real end-to-end discharge success. The deterministic fake self-test freezes the future contract and can be run without mounting `docker.sock`:

```bash
docker run --rm --read-only --network none --cap-drop ALL \
  --security-opt no-new-privileges \
  --tmpfs /tmp:rw,mode=1777 --tmpfs /work:rw,exec,mode=1777 \
  --user 65532:65532 -e TMPDIR=/work \
  --mount type=bind,src="$PWD",dst=/workspace,readonly \
  --workdir /workspace \
  busybox:1.37.0@sha256:9db7b59979c38555a39def84a31fb98b5296952f9e3afd4f6f11f05b07adfab0 \
  sh ci/rocq-discharge-docker-self-test.sh
```

Every bridge inherits `INFERENCE_WASM_VERIFIER_EVIDENCE_DIR`, the one wrapper-created host `0700` evidence directory. A future bridge must write `verifier.log` there before returning nonzero and must keep its public output bounded. The wrapper captures bridge stdout/stderr privately, validates the directory and regular `0600` log, removes transient staging and owned volumes, and prints exactly one sanitized evidence-directory locator. On success it deletes that evidence directory. Inference retains no raw private proof source or receipt contents.

## Roadmap

Check out open [issues](https://github.com/Inferara/inference/issues).

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for details.
