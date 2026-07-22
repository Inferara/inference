# Projects & the infs Toolchain

## Motivation

`infc` is a single-file compiler. Given one `.inf` source file it parses,
type-checks, analyses, and emits a WASM binary. That model is fine for
self-contained examples, but real codebases need more: a place to declare
external `.wasm` dependencies, a way to express whether a build is meant to
produce an executable or a Rocq proof, and a project root that tools can agree
on so artifacts land in predictable locations.

`infs` is the unified toolchain entry point that provides all of this. It
discovers a project, reads its manifest, resolves the configuration, and spawns
`infc` with the right arguments and working directory. The split is deliberate:
`infc` stays a pure compiler that knows nothing about project structure; `infs`
is the orchestrator that wraps it.

## Project Layout

`infs new myproject` scaffolds the following structure:

```text
myproject/
+-- Inference.toml       # manifest
+-- src/
|   +-- main.inf         # entry point (project mode always compiles this)
+-- out/                 # created by the first build (gitignored)
|   +-- main.wasm
+-- tests/
|   +-- .gitkeep
+-- proofs/              # proof-mode artifacts land here by default
|   +-- .gitkeep
+-- .gitignore
```

The layout mirrors Cargo's conventions: manifest at the root, sources under
`src/`, build outputs under `out/`. The `out/` directory is not committed; it
is created automatically by `infc` (relative to its working directory, which
`infs` sets to the project root). `proofs/` is tracked via `.gitkeep` so that
curated hand-authored proof sources stay in version control even when generated
`.wasm`/`.v` artifacts are gitignored by extension.

## The Manifest (`Inference.toml`)

`Inference.toml` is the project manifest. Only `[package]` is required;
all other sections default if absent.

```toml
[package]
name = "myproject"
version = "0.1.0"
infc_version = "0.1.0"

# Optional package fields:
# description = "A brief description"
# authors = ["Name <email@example.com>"]
# license = "MIT"

[build]
# "compile" (default) or "proof"
mode = "compile"
# target and optimize are recognized but not yet consumed.

[verification]
# Output directory for proof artifacts (honored only in proof mode).
# Defaults to "proofs/" if this section is omitted.
# output-dir = "proofs/"

[wasm-dependencies]
# Logical module name -> compiled .wasm, resolved relative to this file.
arith = { path = "libs/arith.wasm" }
```

The fields:

| Field | Section | Type | Default | Description |
|-------|---------|------|---------|-------------|
| `name` | `[package]` | string | — | Project name; see name rules below |
| `version` | `[package]` | string | — | Semver project version |
| `infc_version` | `[package]` | string | detected | `infc` version used when scaffolding |
| `description` | `[package]` | string | absent | Optional description |
| `authors` | `[package]` | array | absent | Optional author list |
| `license` | `[package]` | string | absent | Optional SPDX identifier |
| `mode` | `[build]` | `"compile"` \| `"proof"` | `"compile"` | Build mode (see below) |
| `output-dir` | `[verification]` | path string | `"proofs/"` | Proof artifact directory; proof mode only |
| `<name>` | `[wasm-dependencies]` | `{ path = "…" }` | — | External `.wasm` module dependency |

`mode` is case-sensitive: `"Proof"` is rejected. The field is validated on
load; an invalid value is an immediate error with the allowed set named in the
message.

### Reserved Project Names

Project names must start with a letter or underscore and contain only
alphanumeric characters, underscores, or hyphens. Names that match Inference
language keywords or conventional directory names are reserved and rejected
(case-insensitively). The reserved set includes: `fn`, `let`, `mut`, `if`,
`else`, `match`, `return`, `type`, `struct`, `impl`, `trait`, `pub`, `use`,
`mod`, `ndet`, `assume`, `assert`, `forall`, `exists`, `spec`, `requires`,
`ensures`, `invariant`, `const`, `enum`, `loop`, `break`, `continue`,
`external`, `unique`; and the directory names `src`, `out`, `target`,
`proofs`, `tests`, `self`, `super`, `crate`.

### Real Manifest Examples

From `scratch/raytracing-in-one-weekend/Inference.toml` (single WASM
dependency):

```toml
[package]
name = "raytracing-in-one-weekend"
version = "0.1.0"
infc_version = "0.1.0"

[wasm-dependencies]
fixmath = { path = "libs/fixmath.wasm" }
```

From `scratch/linker-e2e/Inference.toml` (multiple dependencies):

```toml
[package]
name = "linker-e2e"
version = "0.1.0"
infc_version = "0.1.0"

[wasm-dependencies]
arith = { path = "libs/arith.wasm" }
memlib = { path = "libs/memlib.wasm" }
sortlib = { path = "libs/sortlib.wasm" }
```

Neither example sets `[build]` or `[verification]` — those sections are omitted
when all values are at their defaults.

## Project Discovery

When `infs build` or `infs run` receives no path argument, it walks **up** the
directory tree from the current working directory looking for `Inference.toml`.
The nearest ancestor containing the file wins — the same convention Cargo uses,
so a nested project's manifest shadows an outer one. The walk stops at the
filesystem root; if no manifest is found, the command errors with a remediation
message naming `infs new` and `infs init`.

After discovery, `infc` is spawned with its **working directory set to the
project root**. This means `out/` always lands at the root regardless of where
the command was invoked from inside the project tree, and all source-relative
paths in `.inf` files resolve from the same stable base.

```text
~/projects/myproject/src/utils/
$ infs build           # walks up, finds ~/projects/myproject/Inference.toml
                       # infc CWD = ~/projects/myproject/
                       # artifact  = ~/projects/myproject/out/main.wasm
```

## `infs build`

`infs build` has two modes of operation:

```bash
infs build                           # project mode: discovers Inference.toml, compiles src/main.inf
infs build path/to/file.inf          # single-file mode: compiles that file directly
```

Flags:

| Flag | Description |
|------|-------------|
| `-v` | Generate Rocq `.v` translation in addition to `.wasm` |
| `--mode {compile,proof}` | Override compilation mode |
| `-L <DIR>` / `--wasm-lib-dir <DIR>` | Add a directory to search for external `.wasm` modules; repeatable |

**Mode resolution** (precedence, highest first):

1. CLI `--mode` when present.
2. Manifest `[build] mode = "proof"` — forwards `--mode proof` to `infc`.
3. Manifest `[build] mode = "compile"` (explicit or defaulted) — forwards
   **nothing**, leaving `infc`'s own `-v` ↔ proof implication intact.

The rule about forwarding nothing for compile mode is deliberate: `infc`'s
`normalize_args` function owns the `-v` ↔ `--mode proof` implication as its
single source of truth. If `infs` also forwarded `--mode compile` it would
suppress that implication for users who pass only `-v`, turning a spec-aware
proof build into a spec-stripped one.

**`--out-dir` and `[verification] output-dir`**: in effective proof mode (CLI
`--mode proof` or manifest `mode = "proof"`), `infs` reads `[verification]
output-dir` (defaulting to `proofs/`), normalizes it to a project-relative
path, and forwards it to `infc` as `--out-dir`. This relocates both `.wasm`
and `.v` artifacts to that directory. In compile mode, `[verification]
output-dir` is ignored entirely — the compile artifact must always land under
`out/`.

Forwarding `--out-dir` requires `infc` ABI ≥ 1.1 (see [Relationship to
`infc`](#relationship-to-infc) below). Using a non-default `output-dir` with
an older `infc` is a hard error with remediation.

### Artifacts per Mode

| Mode | `-v` | `.wasm` location | `.v` location |
|------|------|-----------------|--------------|
| compile | no | `out/` | — |
| compile | yes | `out/` | `out/` |
| proof | (implied) | `[verification] output-dir` | `[verification] output-dir` |

In single-file mode (`infs build path/to/file.inf`) the output always goes to
`out/` relative to `infc`'s inherited CWD (the invoking shell's current
directory), with no manifest `output-dir` forwarded.

## `infs run`

`infs run` builds and then executes the resulting WASM via wasmtime:

```bash
infs run                                 # project mode: build + invoke main
infs run program.inf                     # single-file: compile and invoke main
infs run program.inf --entry-point helper  # single-file: invoke helper()
```

In **project mode** (no path given):

- Always builds in compile mode, regardless of `[build] mode` in the manifest.
  Proof-mode WASM embeds custom non-deterministic opcodes (`0xfc` family) that
  wasmtime cannot execute.
- Always invokes `main`. Passing `--entry-point` to anything other than `main`
  is rejected with guidance to use single-file mode instead.
- Checks wasmtime availability before starting the build, failing fast if the
  runtime is absent.
- `out/main.wasm` is the expected artifact; if the build succeeds but the file
  is absent, `run` errors before invoking wasmtime.

In **single-file mode** (path given), `--entry-point` (default `main`) selects
which exported function to invoke. `main` is called with `argc=0, argv=0`
automatically; other functions receive the trailing arguments from the command
line.

Both modes require `wasmtime` in `PATH`. Installation instructions are printed
when it is not found.

## Scaffolding

`infs new <name>` creates a new project directory with the layout shown above
and optionally initializes a git repository (pass `--no-git` to skip).

`infs init [name]` initializes the current directory as a project — it writes
`Inference.toml` and `src/main.inf` without creating a new parent directory.
The name defaults to the current directory's name. If `.git/` is present,
`infs init` also creates `.gitignore` and `.gitkeep` files without overwriting
anything that already exists.

Both commands scaffold `src/main.inf` with a minimal entry point:

```inference
// Entry point for the Inference program

pub fn main() -> i32 {
    return 0;
}
```

## Toolchain Management

Project workflow is only half of `infs`'s surface. The other half is a
rustup-style toolchain manager — the part that makes "install Inference" a
one-command operation and lets several compiler versions coexist:

| Command | What it does |
|---------|--------------|
| `infs install [version]` | Download and install a toolchain (latest stable when no version is given) |
| `infs uninstall <version>` | Remove an installed toolchain |
| `infs list` | List installed toolchains, marking the default |
| `infs versions` | Fetch the release manifest and list what is available |
| `infs default <version>` | Set the default toolchain used for compilation |
| `infs component` | Install, list, or remove optional components such as `wasm-opt` (Binaryen), the optimizer behind the `[build.wasm-opt]` manifest table |
| `infs doctor` | Verify the installation and report issues with suggested fixes |
| `infs self` | Update or manage the `infs` binary itself |
| `infs version` | Show the CLI's own version (`-v` adds build and platform detail) |

Toolchains live under `INFERENCE_HOME` (default `~/.inference`), and the
release manifest is fetched from the distribution server (overridable via
`INFS_DIST_SERVER`). `infs build` and `infs run` resolve the `infc` they spawn
through this installation — the resolution order is described in the next
section.

## Relationship to `infc`

`infs` is a thin orchestrator. It does not link, parse, or type-check anything
itself — all of that is `infc`'s responsibility. The relationship:

```text
infs build
    |
    +-- discover_and_load(Inference.toml)
    |
    +-- find_infc()          # INFC_PATH > system PATH > managed toolchain
    |
    +-- compatibility handshake
    |       infc --commit-hash    # short-circuit: same-build binaries always compatible
    |       infc --abi-version    # major mismatch → hard error; minor mismatch → warning
    |
    +-- spawn infc
            CWD = <project root>
            arg: src/main.inf
            arg: --mode proof           (if effective proof mode)
            arg: -v                     (if .v requested)
            arg: --out-dir <dir>        (if effective proof mode + ABI ≥ 1.1)
            arg: --wasm-dep name=path   (one per [wasm-dependencies] entry)
            arg: -L <dir>               (repeated for each --wasm-lib-dir)
```

**`infc` flags** confirmed against `core/cli/src/parser.rs`:

| Flag | Description |
|------|-------------|
| `--parse` | Run only the parse phase |
| `--analyze` | Run parse + analyze phases |
| `--codegen` | Run parse + analyze + codegen; no output without `-o` or `-v` |
| `-o` | Write `.wasm` binary to output directory |
| `-v` | Write Rocq `.v` translation; implies full pipeline and, without explicit `--mode`, implies `--mode proof` |
| `--mode {compile,proof}` | Select compilation mode |
| `--out-dir <path>` | Override output directory (default `out/` relative to CWD); both `.wasm` and `.v` land here |
| `-L <dir>` / `--wasm-lib-dir <dir>` | Add external `.wasm` search directory; repeatable |
| `--wasm-dep <name>=<path>` | Bind a logical module name directly to a `.wasm` file; takes precedence over `-L` |
| `--commit-hash` | Print the build commit hash and exit; used by the `infs` handshake |
| `--abi-version` | Print `<major>.<minor>` ABI version and exit; used by the `infs` handshake |

> **Note:** The current ABI version is `1.1`.

The default behavior when no phase flag is supplied is full compilation with
WASM output written to disk — equivalent to `--codegen -o`.

### ABI Versioning and the `--out-dir` Gate

`infs` and `infc` communicate their compatibility through two probes run before
every project build. First, `infc --commit-hash` is compared with `infs`'s
build commit: a match means the two binaries came from the same tree and are
guaranteed compatible, skipping further checks. Otherwise, `infc --abi-version`
is parsed as `<major>.<minor>`:

- **Major mismatch**: hard error with remediation (rebuild or set `INFC_PATH`).
- **Minor mismatch**: warning only; compilation proceeds.
- **Unknown/old** (`infc` exits non-zero or prints `unknown`): silent; treated
  as graceful skip, equivalent to ABI unknown.

The current ABI is `1.1` (`COMPILER_ABI_MAJOR = 1`, `COMPILER_ABI_MINOR = 1`
in `core/compiler-interface/src/lib.rs`). The `--out-dir` flag was introduced
at minor 1. `infs` gates its use: `--out-dir` is forwarded only to an `infc`
that reports ABI minor ≥ 1 (or matches by commit hash). Pairing a manifest
with a non-default `[verification] output-dir` against an older `infc` is a
hard error:

```text
error: the resolved infc does not support `--out-dir` (requires infc ABI ≥ 1.1);
       update the toolchain or remove `[verification] output-dir` from Inference.toml.
```

### Compiler Resolution

`infs` locates `infc` by checking, in order:

1. `INFC_PATH` environment variable — an explicit override, useful for
   development and CI.
2. System `PATH` (`which infc`).
3. The managed toolchain directory (`~/.inference/toolchains/<version>/infc`).

`INFERENCE_HOME` overrides the managed toolchain root (default `~/.inference`).

## Comparison with Cargo

The parallels to Cargo are intentional:

| Concern | Cargo | Inference |
|---------|-------|-----------|
| Manifest | `Cargo.toml` | `Inference.toml` |
| Entry source | `src/main.rs` (binary) | `src/main.inf` |
| Artifact directory | `target/` | `out/` |
| Discovery | walk up to `Cargo.toml` | walk up to `Inference.toml` |
| Nearest manifest wins | yes | yes |
| New project | `cargo new` | `infs new` |
| Init in-place | `cargo init` | `infs init` |
| External deps | crates via `Cargo.toml [dependencies]` | compiled `.wasm` via `[wasm-dependencies]` |
| Build tool / compiler split | `cargo` / `rustc` | `infs` / `infc` |

Where Inference is deliberately simpler: there are no build profiles beyond the
compile/proof axis (debug vs. release optimization is not yet user-configurable
via the manifest), no workspace support, and no package registry — external
`.wasm` modules are referenced by filesystem path.

## Current Limitations

- **Fixed entry point in project mode.** Project mode always compiles
  `src/main.inf` and invokes `main`. Custom entry files and custom exported
  functions are only available in single-file mode.
- **Single entry file.** `infc` follows the import-reachable closure from
  `src/main.inf`; there is no mechanism to specify additional top-level files.
- **No workspaces.** A single `Inference.toml` defines one project. Multi-crate
  workspace support is not yet implemented.
- **No package registry.** `[wasm-dependencies]` accepts only local filesystem
  paths. Version-pinned or registry-sourced dependencies are reserved for a
  future manifest extension.

## Related Resources

- [Compilation Targets](compilation_targets.md) — compile vs. proof modes,
  optimization levels, and the WASM target matrix; the `--mode` flag is
  specified in detail there.
- [External Functions and WASM Linking](external-functions-and-wasm-linking.md)
  — how `use { f } from <module>;` binds to `[wasm-dependencies]` entries and
  how the linker merges them.
- [The WASM Linker](the-wasm-linker.md) — the static merge algorithm that
  folds external `.wasm` bodies into the output module.
- [Module Hierarchy and Multi-File Compilation](module-hierarchy-and-multi-file-compilation.md)
  — how `infc` follows imports across files within a project.
