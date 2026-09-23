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
`.wasm`/`.v` artifacts are gitignored by extension. What `coqc` writes when one
of those proofs is checked — a `.vo`, a `.glob`, a `.<module>.aux` — is
gitignored too, and anywhere in the project rather than only under `proofs/`:
a `.v` can be hand-authored, so its location decides whether it is output,
while `coqc`'s products never are and land beside whichever source it is given.

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
# The runtime the module is built for; "wasm32" (the default) is generic
# WebAssembly, "stellar" builds a Soroban smart contract, and "spacewasm"
# builds for the SpaceWasm flight interpreter. Validated on load: an
# unrecognized name is an error.
# target = "wasm32"
# "compile" (default) or "proof"
mode = "compile"
# Post-MVP WebAssembly proposals to opt into; empty (the default) keeps the
# output pure WebAssembly 1.0. Currently supported: "bulk-memory".
# wasm-features = ["bulk-memory"]
# optimize is recognized but not yet consumed.

[verification]
# Output directory for proof artifacts (honored only in proof mode).
# Defaults to "proofs/" if this section is omitted.
# output-dir = "proofs/"

[wasm-dependencies]
# Logical module name -> compiled .wasm, resolved relative to this file.
arith = { path = "libs/arith.wasm" }

# [host-imports]
# The host functions the program may bind, one array per import module; a
# header with no keys admits none. Omit the table to apply no policy.
# fprime_core = ["telemetry", "command"]
# env = ["clock_ms"]
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
| `target` | `[build]` | target name | `"wasm32"` | The runtime the module is built for; `"wasm32"` is generic WebAssembly, `"stellar"` a Soroban smart contract, and `"spacewasm"` the SpaceWasm flight interpreter. Accepted: `"wasm32"`, `"stellar"`, `"spacewasm"` |
| `mode` | `[build]` | `"compile"` \| `"proof"` | `"compile"` | Build mode (see below) |
| `wasm-features` | `[build]` | array of proposal names | `[]` | Post-MVP WebAssembly proposals the artifact may use; `[]` = pure Wasm 1.0. Supported: `"bulk-memory"` |
| `output-dir` | `[verification]` | path string | `"proofs/"` | Proof artifact directory; proof mode only |
| `<name>` | `[wasm-dependencies]` | `{ path = "…" }` | — | External `.wasm` module dependency |
| `<module>` | `[host-imports]` | array of field names | table absent: no policy | The host functions of import module `<module>` the program may bind; a table with no keys admits none. See [The allowlist](external-functions-and-wasm-linking.md#the-allowlist) |

`target` and `mode` are case-sensitive: `"Wasm32"`, `"SpaceWasm"` and `"Proof"`
are all rejected, and neither key trims whitespace. The wire spelling of every
target name is one lower-case word, so `"space-wasm"` and `"space_wasm"` are
rejected too. Both are validated on load; an invalid
value is an immediate error with the allowed set named in the message, never a
fall back to the default. `target` names the same vocabulary as `infc --target`
and is rejected with the same wording; `"soroban"`, the former name of
`"stellar"`, earns a message redirecting to the current spelling rather than the
generic unknown-target one.

`target = "stellar"` narrows the rest of the manifest. It cannot be combined
with a `[build.wasm-opt]` table — that pairing is itself a load error, because
whether an external Binaryen preserves a contract's metadata section depends on
a version nothing here pins — it admits no `wasm-features`, `mode = "proof"`
is a load error too, and `infs run` refuses the project outright, since a
contract is invoked by a Soroban host and not by a plain WebAssembly runtime.
What an exported function may declare is narrowed too; see
[Compilation Targets](compilation_targets.md).

`target = "spacewasm"` narrows it less, and the difference is instructive. It
admits no `wasm-features` and no `mode = "proof"`, on load, for the same reason
— the interpreter decodes neither the post-MVP families nor the custom `0xfc`
instructions. But it keeps `[build.wasm-opt]`, because what that refusal
protects is a contract's wrappers and metadata section and this target's module
has neither, and it keeps `infs run`, because the artifact is plain WebAssembly
whose `main` a runtime can invoke. Nothing narrows what an exported function
may declare: the bytes are the `"wasm32"` build's. The same load-time
strictness applies to
keys: every fixed-schema table rejects a key it does not recognize, naming the
offending key and the fields the table accepts — a misspelled `wasm_features`
fails the build rather than silently shipping a differently-configured
artifact. Three tables accept arbitrary keys, because their keys are the data:
`[dependencies]` and `[wasm-dependencies]`, whose keys name dependencies, and
`[host-imports]`, whose keys name the import modules an embedder registers host
functions under. `wasm-features` entries are WebAssembly *proposal* names
(`"bulk-memory"`), not instruction names, and the setting is honored in project
builds, single-file builds, and single-file `infs run`. `[wasm-dependencies]`
entries have the same reach: project `build`, project `run`, single-file
`build`, and single-file `run` all forward every declared entry to `infc`, and
all four forward a `[host-imports]` table too. The unknown-key refusal is
younger than every released `infs`, though: a release through `v0.0.5` reads
past a root table it does not know with no diagnostic, and for `[host-imports]`
that means building under no policy at all, so a project that relies on its
allowlist has to be built with an `infs` that knows the table. See
`apps/infs/docs/inference-toml.md` for the full reference.

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

### Reserved Module Segment

A module name has one reservation of its own, separate from the project-name set
above and applied in a different position: `host` as the *first* segment of a
`use … from` clause names the embedder, not a module this build links. So
`use { clock_ms } from host::env;` binds a host import of the module `env`, and
a `[wasm-dependencies]` key whose first segment is `host` — `host`, or
`host::io` — is refused on every build and run route before `infc` is asked to
build, because no linked module can be named under it; a host import needs no
dependency entry at all, and belongs under `[host-imports]` if the project keeps
an allowlist. The match is exact and first-segment only: `hostlib`, `Host` and
`a::host` are ordinary module names. Contrast the path grammar's reserved
`root`, which names the entry file in `use root;` and shadows a literal
`src/root.inf` — `root` is reserved among the files of this project, `host`
among the modules outside it. See [What `host` reserves, and what it does
not](external-functions-and-wasm-linking.md#what-host-reserves-and-what-it-does-not).

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
| `-L <DIR>` / `--wasm-lib-dir <DIR>` | Add a directory to search for external `.wasm` modules; repeatable. A relative directory is read against the directory you invoked `infs` from, in both modes |

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
infs run program.inf -L libs             # single-file: search libs/ for external .wasm
```

Flags:

| Flag | Description |
|------|-------------|
| `--entry-point <name>` | Function to invoke in single-file mode (default `main`); rejected for anything but `main` in project mode |
| `-L <DIR>` / `--wasm-lib-dir <DIR>` | Add a directory to search for external `.wasm` modules; repeatable. Anchored to the invocation directory in project mode; in single-file mode no anchoring step runs, since `infc` already inherits that directory |
| `--no-wasm-opt` | Skip `[build.wasm-opt]` post-build optimization (project mode only) |

In single-file mode, `-L` and the other options must appear before the first
bare trailing token: `infs run program.inf -L libs 1` parses `-L libs` and
passes `1` to the invoked function, while `infs run program.inf 1 -L libs`
passes `1 -L libs` verbatim as two trailing arguments, parsing no `-L` at all.
Use `--` to pass arguments that themselves look like flags.

In **project mode** (no path given):

- Always builds in compile mode, regardless of `[build] mode` in the manifest.
  Proof-mode WASM embeds custom non-deterministic opcodes (`0xfc` family) that
  wasmtime cannot execute. The one thing `[build] mode` can still do to `run` is
  stop it before it starts: `mode = "proof"` paired with a target that has no
  proof mode is a load error for every command, `run` included — a value `run`
  would ignore can still refuse to be read.
- Always invokes `main`. Passing `--entry-point` to anything other than `main`
  is rejected with guidance to use single-file mode instead.
- Checks wasmtime availability before starting the build, failing fast if the
  runtime is absent.
- Resolves the manifest's `[wasm-dependencies]` and forwards any `-L`
  directories the same way project `build` does, anchored to the invocation
  directory, so a project binding `use { … } from <module>` runs without a
  separate link step.
- `out/main.wasm` is the expected artifact; if the build succeeds but the file
  is absent, `run` errors before invoking wasmtime.
- Refuses to execute an artifact that imports a function. `run` supplies no
  host functions and does not let wasmtime stand in for the embedder a program
  binding `use { … } from host::…` is written for — not even for the WASI
  functions the wasmtime CLI provides on its own. It builds the program, reads
  the imports off the artifact — after `[build.wasm-opt]`, so the bytes it asks
  about are the ones that would run — and names each one, pointing at the
  embedder that must supply them. Single-file `run` refuses the same way,
  naming `out/<stem>.wasm`. The wasmtime check above still comes first, so a
  machine without the runtime is told to install it before it can hear this
  refusal.

In **single-file mode** (path given), `--entry-point` (default `main`) selects
which exported function to invoke. `main` is called with `argc=0, argv=0`
automatically; other functions receive the trailing arguments from the command
line — anything after the first bare token that options did not consume, or
after `--`. Single-file `run` also resolves the enclosing manifest's
`[wasm-dependencies]` and forwards any `-L` directories verbatim: `infc`
inherits the invoking shell's working directory here, so a relative `-L`
already means what it meant at the shell, with no anchoring step needed.

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
infs build / infs run (project mode)
    |
    +-- discover_and_load(Inference.toml)
    |
    +-- find_infc_with_source()  # INFC_PATH > sibling of infs > system PATH > managed toolchain
    |
    +-- compatibility handshake
    |       infc --commit-hash    # short-circuit: same-build binaries always compatible
    |                             # sibling tier only: differing commits warn (stale neighbour)
    |       infc --abi-version    # major mismatch → hard error; minor mismatch → warning
    |
    +-- spawn infc
            CWD = <project root>
            arg: src/main.inf
            arg: -v                     (if .v requested; `run` never requests it)
            arg: --mode proof           (if effective proof mode; `run` never requests it)
            arg: --out-dir <dir>        (if effective proof mode + ABI ≥ 1.1; `run` never requests it)
            arg: --wasm-lib-dir <dir>   (one per -L/--wasm-lib-dir; a relative
                                         dir is anchored to the invocation
                                         directory, since infc runs at the root —
                                         `build` and `run` both pass their own
                                         `-L` flags through here)
            arg: --wasm-dep name=path   (one per [wasm-dependencies] entry; the
                                         declared path resolved against the root)
            arg: --target <name>        (if [build] target names a non-default
                                         target; gated on that name's own minor)
            arg: --wasm-features <list> (if [build] wasm-features requests any;
                                         requires infc ABI ≥ 1.2. Applies in both
                                         modes — a `.v` describing a different
                                         instruction set than the shipped `.wasm`
                                         would be worthless)
            arg: --memory-pages <n>     (if [memory] declares `pages`; ABI ≥ 1.3)
            arg: --stack-size <n>       (if [memory] declares `stack-size`; ABI ≥ 1.3)
            arg: --adopt-external-specs (if [verification] asks for it on a
                                         proof-artifact build; ABI ≥ 1.4; `run`
                                         never requests it)
            arg: --host-imports=<list>  (if a [host-imports] table is declared,
                                         as one argument — `--host-imports=` for
                                         a table with no keys; requires infc
                                         ABI ≥ 1.8)
```

Project `run` forces compile mode (`mode = None`) and never requests `--out-dir`,
so the `-v`/`--mode proof`/`--out-dir` lines above never fire for it — but it
shares this exact spawn otherwise, including the `-L` and `[wasm-dependencies]`
forwarding.

Single-file `build <path>` and `run <path>` spawn `infc` directly instead of
through the shared project-build helper, and — unlike the two project-mode
paths above — they do **not** send `infc` the same argument list: `build`
forwards only what the user explicitly passed, leaning on `infc`'s own
phase-flag default for the rest, while `run` always requests the full
pipeline explicitly, because it needs the finished WASM artifact in hand to
execute it. What they do agree on: neither sets `current_dir`, so `infc`
inherits the invocation directory; every `-L` is forwarded verbatim, with no
anchoring step; and whichever of `--wasm-lib-dir`, `--wasm-dep`, `--target`,
`--wasm-features`, the memory flags and `--host-imports` a given invocation
sends, they appear in that same relative order.

```text
infs build <path> (single-file mode)
    |
    +-- enclosing_manifest(path)   # walk up from the source file; optional
    |
    +-- compatibility handshake     # unconditional — always runs, before any
    |       infc --commit-hash      # flag below is decided (unlike `run`,
    |       infc --abi-version      # which only handshakes when wasm-features
    |                               # are requested)
    |
    +-- spawn infc
            CWD = inherited from the invoking shell
            arg: <path>
            arg: -v                     (if `-v` was passed; `run` never sends it)
            arg: --mode <mode>          (if `--mode` was passed; `run` never
                                         sends it)
            arg: --wasm-lib-dir <dir>   (one per -L/--wasm-lib-dir, forwarded
                                         verbatim; no anchoring step runs)
            arg: --wasm-dep name=path   (one per [wasm-dependencies] entry from
                                         the enclosing manifest, if any)
            arg: --target <name>        (as the project route sends it)
            arg: --wasm-features <list> (if the enclosing manifest requests any;
                                         requires infc ABI ≥ 1.2)
            arg: --memory-pages <n>     (as the project route sends them)
            arg: --stack-size <n>
            arg: --host-imports=<list>  (if the enclosing manifest declares a
                                         [host-imports] table; one argument;
                                         requires infc ABI ≥ 1.8)
```

`build` never adds `--parse`, `--codegen`, or `-o`: with no phase flag at all,
`infc`'s own default — full compilation, WASM written to disk — already does
what `build` wants.

```text
infs run <path> (single-file mode)
    |
    +-- enclosing_manifest(path)   # walk up from the source file; optional
    |
    +-- compatibility handshake     # conditional — runs only when the
    |       infc --commit-hash      # enclosing manifest asks for something an
    |       infc --abi-version      # older infc could not honor: a target,
    |                               # wasm-features, a [memory] table, or a
    |                               # [host-imports] table (present, even
    |                               # empty); skipped entirely otherwise,
    |                               # unlike `build`
    |
    +-- spawn infc
            CWD = inherited from the invoking shell
            arg: <path>
            arg: --parse                (always; `build` never sends it)
            arg: --codegen              (always; `build` never sends it)
            arg: -o                     (always; `build` never sends it)
            arg: --wasm-lib-dir <dir>   (one per -L/--wasm-lib-dir, forwarded
                                         verbatim; no anchoring step runs)
            arg: --wasm-dep name=path   (one per [wasm-dependencies] entry from
                                         the enclosing manifest, if any)
            arg: --target <name>        (as the project route sends it)
            arg: --wasm-features <list> (if the enclosing manifest requests any;
                                         requires infc ABI ≥ 1.2)
            arg: --memory-pages <n>     (as the project route sends them)
            arg: --stack-size <n>
            arg: --host-imports=<list>  (if the enclosing manifest declares a
                                         [host-imports] table; one argument;
                                         requires infc ABI ≥ 1.8)
```

`run` always requests `--parse --codegen -o` explicitly rather than relying on
`infc`'s default, and neither `-v` nor `--mode` is ever part of its argv: the
`RunArgs` struct (`apps/infs/src/commands/run.rs`) carries no such flags. Once
`infc` exits, both `run` routes read the imports off the artifact they are about
to execute and refuse one that imports any function, before wasmtime is
invoked — see [`infs run`](#infs-run).

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
| `--target <name>` | Name the runtime the module is built for; matched exactly against the same vocabulary `[build] target` uses (`wasm32`, `stellar`, `spacewasm`). Omitted selects `wasm32` |
| `--wasm-features <names>` | Post-MVP WebAssembly proposals emission may use; comma separated, proposal names only |
| `--memory-pages <n>` | Linear memory size of the emitted module, in 64 KiB pages |
| `--stack-size <bytes>` | Shadow stack size, and the budget A036 measures call-chain frame usage against |
| `--adopt-external-specs` | Carry a linked library's universal proof obligations into the program's proof artifact; proof mode only |
| `--host-imports=<list>` | Allowlist the host functions the program may bind, as comma-separated `module.field` pairs; the `=` is required, and `--host-imports=` is an explicit empty allowlist that admits none. Omitted applies no policy |
| `--commit-hash` | Print the build commit hash and exit; used by the `infs` handshake |
| `--abi-version` | Print `<major>.<minor>` ABI version and exit; used by the `infs` handshake |

> **Note:** The current ABI version is `1.8`.

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

The current ABI is `1.8` (`COMPILER_ABI_MAJOR = 1`, `COMPILER_ABI_MINOR = 8` in
`core/compiler-interface/src/lib.rs`). Each additive flag is gated at the minor
it was introduced at, independently: `--out-dir` landed at minor 1,
`--wasm-features` at minor 2, `--memory-pages` and `--stack-size` at minor 3,
`--adopt-external-specs` at minor 4, `--target` at minor 5, and
`--host-imports` at minor 8. `infs` forwards each only to an `infc` that
reports an ABI minor at or above the flag's own (or matches by commit hash) —
an `infc` that reports minor 1, for instance, supports `--out-dir` but not
`--wasm-features`. `--host-imports` is forwarded from the `[host-imports]`
table, and only from a declared one, so a project without the table puts no
minor-8 floor under its compiler; a project with one is refused against an
older `infc` rather than built without the flag, because that compiler would
admit every host import the program binds and write an artifact identical to
the one the table exists to police. `--target` is gated on the *name* rather
than on the flag: each target became requestable at its own minor (`wasm32` at
5, `stellar` at 6, `spacewasm` at 7), and an `infc` that parses the flag but
predates a name would accept it and build for the default runtime — a wrong
artifact rather than a refusal. That is true even where the two builds are the
same bytes: dropping `spacewasm` produces the module a `wasm32` build produces
and drops the target's acceptance envelope with it, so what the name was
written for never runs. The default target is never forwarded at all, so a
project that names none puts no floor under its compiler. Pairing a manifest
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
2. The `infc` sitting in the same directory as the running `infs`. A driver and
   its companion tools ship as one unit, so adjacency identifies the paired
   compiler — the same rule clang and rustc use to find theirs. Nothing about
   the directory is inspected, only that the two binaries share it, so this
   holds for a cargo build under any `CARGO_TARGET_DIR`, `--target-dir`,
   profile, or target triple, and for an unpacked release tarball alike.
3. System `PATH` (`which infc`).
4. The managed toolchain directory (`~/.inference/toolchains/<version>/infc`).

Step 2 assumes a pairing rather than proving one, so the handshake checks it:
when the sibling tier resolved the compiler and `infc --commit-hash` disagrees
with the commit `infs` was built from, the build warns and names both. The other
three tiers stay silent on a commit mismatch, because for an explicitly pinned,
system-installed, or managed `infc` a differing commit is the normal state — only
adjacency claims the two binaries are a pair. The check sees *cross-commit* drift
only: two binaries built from one commit with different working trees report the
same hash.

`INFERENCE_HOME` overrides the managed toolchain root (default `~/.inference`).
`INFS_VERBOSE` traces which of the four resolved, and reports when step 2
found no neighbour and fell through.

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
