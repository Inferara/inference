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

`--adapter` defaults to `both`. `batch` sends one immutable exchange volume to the verifier bridge. `single` makes the six ordered per-case bridge calls with a new empty `0700` receipt directory for each call. `both` verifies the batch receipts, removes only the validated receipt set, runs the six single calls, and verifies the replacement receipts. Inference fingerprints `request.json` and all six ordered raw Rocq inputs before and after every bridge call; it never parses verifier-private proof logs.

The wrapper composes [`ci/rocq-rust-docker.sh`](ci/rocq-rust-docker.sh), preserving that lane's target lock, source snapshot, persistent Cargo registry/target volumes, pinned Rust image and explicit Rust `1.98.0` toolchain. [`ci/rocq-discharge.cargo-lock`](ci/rocq-discharge.cargo-lock) is the authoritative tracked lock for this lane. The ignored root `Cargo.lock` is excluded from the snapshot and is never an input to the gate. Fetch is the only networked Rust step; all compilation and execution after fetch are locked, offline, socket-free, and run with a read-only root filesystem, dropped capabilities, `no-new-privileges`, and a private `/tmp`.

`--full` first runs the focused dischargeability tests, then the adapter flow, then the complete `inference-tests` crate (not the whole workspace). The clean Docker floor is exactly five Cargo `test result:` lines with at least 3,075 passed tests in aggregate and zero failed or filtered tests. Empty, single-binary, malformed, filtered, and under-floor logs fail closed.

The verifier input must be an absolute canonical, clean `wasm-verifier` checkout whose `HEAD` equals the pinned revision `181cd676662453182b9753d1b19ca933c68770c3` in [`core/wasm-to-v/wasm-verifier-pin.txt`](core/wasm-to-v/wasm-verifier-pin.txt). That revision supplies the live verifier-side bridge contract. Its `ci/discharge/container-pin.json` is exact canonical eight-line JSON, including field order and commas:

```json
{
  "protocol": 1,
  "image_reference": "<pinned reference>",
  "image_id": "sha256:<64 lowercase hex>",
  "coq_user": "coq",
  "repository_mount": "/workspaces/wasm-verifier",
  "coq_version": "8.20.1"
}
```

Immediately before and after every bridge, the wrapper rechecks the clean exact checkout, the identities and Git content of `container-pin.json`, `inspect-container.sh`, the configured public adapters, and the required shared executable `docker-bridge.sh`. Every contract file must remain a regular nonsymlink file with exactly one hard link. Inspection must show exactly one container mount total: a `bind` from the canonical verifier checkout to the pinned repository destination, with no extra bind, volume, socket, or alias mount. It must also show canonical positive-decimal `coq` uid/gid values, exact Coq `8.20.1`, the pinned verifier revision and `coq-wasm` tag/revision, and exact origin `https://github.com/WasmCert/WasmCert-Coq.git`. The inspector must exit zero and emit exactly those eight canonical provenance lines with no extras or duplicates. Batch receives an empty wrapper-owned receipt setting; each single call receives only its new wrapper-owned receipt directory, regardless of ambient environment values.

The six-case gate requires exactly thirteen positive endpoints plus the negative false-spec certificate. The pinned verifier B certifies the first eleven; the lane stays red until the pin is bumped to a B that also proves `spec_linked_extern.v`. That sixth case, `linked-extern`, is the only one whose raw artifact is the translation of a *merged* module, so it is the only endpoint that will be proved about a body the compiler never emitted. The deterministic fake self-test freezes the bridge contract and can be run without mounting `docker.sock`:

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

Every bridge inherits `INFERENCE_WASM_VERIFIER_EVIDENCE_DIR`, the one wrapper-created host `0700` evidence directory. The pinned bridge writes `verifier.log` there before returning nonzero and keeps its public output bounded. The wrapper holds the original identity-checked, single-link `0600` capture through a parent file descriptor and never reopens its bridge-visible path. With `--full`, it likewise creates an unpredictable identity-checked, single-link `0600` Cargo log before any bridge, writes and parses only through retained parent descriptors, and rejects path replacement or added hard links. Evidence logs and receipt files must also be regular, single-link `0600` files. On a valid bridge failure it retains exactly the private evidence directory and prints one sanitized locator; on success, verified cleanup uses exact-name volume enumeration, removes the capture, evidence, transient staging, and owned source/exchange volumes, and confirms absence before the sole pass marker. Identity uncertainty preserves the suspect path and fails closed. Inference retains no raw private proof source or receipt contents.

## Roadmap

Check out open [issues](https://github.com/Inferara/inference/issues).

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for details.
