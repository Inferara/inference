# Inference.toml Manifest Format

This document describes the `Inference.toml` project manifest format used by Inference projects.

## Overview

Every Inference project contains an `Inference.toml` file in its root directory. This manifest describes the project metadata, dependencies, build configuration, and verification settings.

The manifest uses the [TOML](https://toml.io/) format for human-readable configuration.

## File Location

```
myproject/
├── Inference.toml    ← Project manifest
├── src/
│   └── main.inf
└── proofs/
```

## Basic Structure

```toml
[package]
name = "myproject"
version = "0.1.0"
infc_version = "0.1.0"

[dependencies]
# Future: package dependencies

[build]
target = "wasm32"
optimize = "debug"
mode = "compile"    # "compile" (executable WASM) or "proof" (Rocq translation)
wasm-features = []  # post-MVP WebAssembly proposals to opt into

[build.wasm-opt]        # optional: post-build optimization of the executable
enabled = true          # table presence enables; set false to keep it off
level = "3"             # forwarded as -O<level>: "0".."4", "s", "z"
auto-install = false    # download wasm-opt automatically if it is missing

[verification]
output-dir = "proofs/"   # honored only in proof mode
```

## Unknown Keys

Every table with a fixed set of fields — the manifest root (which table names it
accepts), `[package]`, `[build]`, `[build.wasm-opt]`, `[verification]`, and each
`[wasm-dependencies]` entry — rejects a key it does not recognize. A misspelled
key is a load error naming the offending key, its line and column, and the fields
that table accepts:

```
TOML parse error at line 7, column 1
  |
7 | wasm_features = ["bulk-memory"]
  | ^^^^^^^^^^^^^
unknown field `wasm_features`, expected one of `target`, `optimize`, `mode`, `wasm-features`, `wasm-opt`
```

The exceptions are `[dependencies]` and `[wasm-dependencies]`, where the keys are
the data: they name dependencies, so any well-formed name is accepted.

The trade-off is deliberate. An older `infs` reading a manifest that uses a key
added by a newer toolchain fails rather than ignoring it. That matches how the
compiler ABI gate treats toolchain/manifest skew — an error, never a silent
downgrade that ships a differently-configured artifact than the manifest asked
for.

There is one exception among the keys a build acts on, and it is `[build]
target`. That key was parsed and recorded before it was consumed, so it was
never protected by the unknown-key refusal above: an `infs` predating target
forwarding reads `target = "spacewasm"`, discards it, and builds for the default
runtime with no diagnostic — the silent downgrade this section promises never
happens. The promise therefore holds for `target` only from the release that
introduced target forwarding; every `infs` from that release on refuses an
unrecognized name loudly, and forwards a recognized one or refuses to build.
Every released `infs` through `v0.0.5` predates it. (`[build] optimize` is
parsed and unvalidated in the same way, but nothing reads it, so no build can be
downgraded by it.)

## Settings Honored in Single-File Mode

`infs build path/to/file.inf` and `infs run path/to/file.inf` compile one named
file rather than the project's entry point, but they are not manifest-blind: each
walks up from the source file to the nearest `Inference.toml`. Both commands
honor the same settings:

| Setting | `infs build <path>` | `infs run <path>` |
|---|---|---|
| `[build] wasm-features` | honored | honored |
| `[wasm-dependencies]` | honored | honored |

`[build] wasm-features` is honored by both, with the same validation and the same
ABI gate as project mode. This is deliberate rather than incidental: `infs build`,
`infs build src/main.inf`, and `infs run src/main.inf` all write
`out/main.wasm` for the same project, so if they disagreed about the instruction
set, the artifact you ship would depend on which command last touched it.

`[wasm-dependencies]` is resolved identically everywhere: project `infs build`,
project `infs run`, single-file `infs build <path>`, and single-file `infs run
<path>` all forward every declared entry to `infc` as `--wasm-dep <name>=<path>`,
and none of the four gate it on `infc`'s ABI. Both single-file commands also
accept `-L`/`--wasm-lib-dir` to add extra search directories beyond the
manifest, so a file that imports an external module runs directly without a
separate link step.

Everything else in the manifest is project-mode configuration: `[build] mode`,
`[verification] output-dir`, and `[build.wasm-opt]` are not consulted in
single-file mode. A source file outside any project takes every default and never
errors for want of a manifest.

## Section Reference

### [package]

The `[package]` section defines project metadata.

#### Required Fields

- **`name`** (string): The project name
  - Must start with a letter or underscore
  - Can contain letters, numbers, underscores, and hyphens
  - Cannot be a reserved keyword (e.g., `fn`, `let`, `struct`)
  - Cannot be a reserved directory name (e.g., `src`, `target`, `out`)

- **`version`** (string): The project version in [semver](https://semver.org/) format
  - Example: `"0.1.0"`, `"1.2.3"`

- **`infc_version`** (string): The compiler version used to create this project
  - Automatically detected from `infc --version` when running `infs new` or `infs init`
  - Falls back to the `infs` version if `infc` is not available
  - Example: `"0.1.0"`

#### Optional Fields

- **`description`** (string): A brief project description
  - Example: `"A compiler for mission-critical applications"`

- **`authors`** (array of strings): List of project authors
  - Example: `["Alice <alice@example.com>", "Bob <bob@example.com>"]`

- **`license`** (string): The project license identifier
  - Example: `"MIT"`, `"Apache-2.0"`, `"GPL-3.0"`

#### Example

```toml
[package]
name = "my-inference-app"
version = "1.0.0"
infc_version = "0.1.0"
description = "A verified sorting algorithm implementation"
authors = ["Alice <alice@example.com>"]
license = "MIT"
```

### [dependencies]

The `[dependencies]` section lists project dependencies.

**Status**: Reserved for future package management support.

#### Example (Future)

```toml
[dependencies]
std = "0.1"
some-lib = { version = "1.0", features = ["feature1"] }
```

### [build]

The `[build]` section configures compilation settings.

#### Fields

- **`target`** (string, default: `"wasm32"`): The runtime the module is built
  for. The axis is the runtime, not a compiler back end and not a target triple:
  `"wasm32"` is the generic value, a module for any WebAssembly embedder that
  imposes no ABI of its own, and every further name stands for one specific
  runtime. Those come in two kinds, and the difference decides what a build for
  them produces. One kind imposes an ABI, and its artifact is a rewrite of the
  default's — `"stellar"`. The other imposes none and only narrows what a build
  may *contain*, emitting the default's bytes unchanged — `"spacewasm"`.
  - Accepted values: `"wasm32"`, `"stellar"`, `"spacewasm"`.
  - `"stellar"` builds a Soroban smart contract: every exported method is
    rewritten to take and return the host's 64-bit tagged word, and the module
    carries the environment-metadata section a host will not upload it without.
    It narrows what a project may contain — no `mode = "proof"`, no
    `wasm-features`, no `[build.wasm-opt]` (below), and an exported function's
    parameters and return confined to the scalar set the convention encodes.
    The first three are refused by the manifest itself, at load time, in a
    message naming both keys, so they are invalid for every command rather than
    skipped by some; the last is enforced by the compiler, where the exports
    are. `infs run` also refuses such a project: a contract is invoked by a
    Soroban host, which encodes each argument into that tagged word, and by
    nothing else. See the book's Compilation Targets chapter for the full rule
    set.
  - `"spacewasm"` builds for the SpaceWasm flight interpreter, and adds nothing
    to the module: the artifact is the `"wasm32"` one, byte for byte, with no
    wrappers, no metadata section and no rewrite. What it narrows is what a
    build may contain — no `mode = "proof"` and no `wasm-features`, both refused
    by the manifest on load in a message naming both keys. `[build.wasm-opt]`
    stays available, unlike at `"stellar"`: what that refusal protects is a
    contract's wrappers and metadata section, and this target's module has
    neither. `infs run` is available too, for the same reason — the artifact is
    plain WebAssembly and `main` keeps the shape a runtime invokes, so `infs run`
    builds the module and executes it locally under `wasmtime`. That runtime is
    not the flight interpreter, and running the module here exercises the module
    rather than the environment it was built for: what a local `wasmtime` run
    does not exercise is the interpreter's *environment* — its allocator, its
    registered hosts, and the two const generics an embedder fixes. The
    interpreter's decode-time maxima (parameter and local word counts, name
    lengths, host arity) are not among them: `infc` checks every `spacewasm`
    build against them after linking and before writing the artifact, and
    prints the control-frame and operand-stack budget the module needs. With
    `[build.wasm-opt]` enabled, `infs build` asks the same question again of
    the optimized bytes and reprints the budget, because the artifact that
    ships is that one. See the book's Compilation Targets chapter for the full
    rule set.
  - `"soroban"` is the former name of `"stellar"` and is not accepted; it earns a
    message saying so rather than the generic unknown-target one.
  - Matching is exact and case-sensitive, and whitespace is not trimmed:
    `"Wasm32"` and `"wasm32 "` are both rejected. An unrecognized value is a load
    error listing the accepted set, never a silent fall back to the default — a
    build that quietly targeted something other than what the manifest named
    would be worse than a refusal.
  - `infc --target <name>` is the same vocabulary validated with the same
    wording, so a value rejected here is rejected identically there. `infs` has
    no `--target` flag: a target is a property of the project, and a flag would
    let one project produce artifacts for two runtimes.

- **`optimize`** (string, default: `"debug"`): The optimization level
  - `"debug"`: No optimizations, faster compilation
  - `"release"`: Full optimizations, slower compilation

- **`mode`** (string, default: `"compile"`): The compilation mode
  - `"compile"`: Strips non-deterministic specs; produces executable WASM
  - `"proof"`: Preserves specs for Rocq translation; enables `-v` inside `infc`

  In project mode (`infs build` with no path), this field determines whether
  `infs` forwards `--mode proof` to `infc` and whether `[verification]
  output-dir` is consulted. A CLI `--mode` flag overrides this setting — but
  only once the manifest has loaded: a `mode` and a `target` that cannot be
  built together are refused on load, before any command runs, so `infs build
  --mode compile` on a `mode = "proof"` manifest naming a target with no proof
  mode is refused until `[build] mode` is edited. `infs run` ignores this field
  entirely and always builds in compile mode, and the same load error stops it
  for the same reason — a value `run` would ignore can still stop it.

  The value is case-sensitive: `"Proof"` is rejected. `"proof"` is refused
  outright when `[build] target` names a target that has no proof mode: the
  pairing is reported on load, naming both keys.

- **`wasm-features`** (array of strings, default: `[]`): Post-MVP WebAssembly
  proposals the emitted module opts into. Empty — the default — means the output
  is pure WebAssembly 1.0.
  - Supported today: `"bulk-memory"`.
  - Enabling `bulk-memory` lets code generation use `memory.fill` to zero a stack
    frame and `memory.copy` to copy a struct or array, instead of the explicit
    store/load sequences the 1.0 baseline requires.
  - Names are **proposal** names, not instruction names: write `"bulk-memory"`,
    not `"memory.fill"`. A proposal enables its whole instruction family, and
    which instruction appears where is a code-generation decision.
  - Matching is exact and case-sensitive, and whitespace is not trimmed:
    `"Bulk-Memory"`, `"bulk_memory"` and `"bulk-memory "` are all rejected. A
    silently-ignored typo would change the instruction set of a shipped artifact.
  - Applies identically in compile and proof mode. The `.v` must describe the
    same program as the `.wasm`, so nothing gates this on the build mode.
  - **Changing this value invalidates proof artifacts generated before the
    change**, because the translated instructions differ. That is the reason it
    lives in the versioned manifest rather than in a command-line flag.
  - A non-empty list is refused outright when `[build] target` names a target
    that permits no post-MVP proposal: the pairing is reported on load, naming
    both keys. An empty list asks for nothing and is accepted for every target.
  - Honored by single-file `build` and `run` as well as project mode (see
    [Settings Honored in Single-File Mode](#settings-honored-in-single-file-mode)),
    and requires an `infc` with ABI 1.2 or newer — an older compiler cannot honor
    the request and is refused with remediation rather than handed the flag.

#### Example

```toml
[build]
target = "wasm32"
optimize = "release"
mode = "proof"
wasm-features = ["bulk-memory"]
```

### [build.wasm-opt]

The `[build.wasm-opt]` table is an optional sub-table of `[build]` that enables post-build optimization of the compiled WASM executable via the external [Binaryen](https://github.com/WebAssembly/binaryen) `wasm-opt` binary. `infs` can provision `wasm-opt` for you — `infs component add wasm-opt` installs a pinned, checksum-verified Binaryen release, and `auto-install = true` does the same automatically at build time — or resolve one already on your system; resolution order is covered below.

An `Inference.toml` with no `[build.wasm-opt]` table at all leaves the build pipeline unchanged — this feature is off by default.

#### Fields

- **`enabled`** (boolean, default: `true`): Whether the optimizer runs.
  - Table *presence* is what turns the feature on: an empty `[build.wasm-opt]` table enables optimization at the default level. Set `enabled = false` to keep the table (and a configured `level`) in the manifest while disabling the step.

- **`level`** (string, default: `"3"`): The optimization level, forwarded to `wasm-opt` as `-O<level>`.
  - One of `"0"`, `"1"`, `"2"`, `"3"`, `"4"`, `"s"`, `"z"` — the same levels `wasm-opt` itself accepts (`s` and `z` bias toward size over speed). Any other value is a load error naming the offending value and the allowed set.

- **`auto-install`** (boolean, default: `false`): Whether a missing `wasm-opt` is downloaded automatically at build time.
  - When `true` and `wasm-opt` does not resolve in any of the three tiers below, `infs` downloads the pinned, sha256-verified Binaryen release into `~/.inference/tools/binaryen/<version>/` — the same install `infs component add wasm-opt` performs — before optimizing. The default `false` keeps a build network-free: a missing binary is a hard error with install remediation instead. The opt-in is recorded in the versioned manifest; `infs` has no interactive prompts.
  - An invalid `WASM_OPT_PATH` override is always an error, even with `auto-install = true` — a typo in that variable is surfaced rather than silently downloaded over.

#### Example

```toml
[build.wasm-opt]
enabled = true
level = "z"
```

#### When optimization runs

- **Project mode only, for executable artifacts.** Both `infs build` and `infs run` apply `[build.wasm-opt]` to `out/main.wasm` after a successful compile — `run` optimizes exactly the artifact it then executes, so what you run is what `build` would have shipped. Single-file mode (`infs build file.inf`) never runs the optimizer, whether or not a manifest is present.
- **Proof-mode and `-v` builds are always skipped, silently.** A build counts as proof mode when the effective `[build] mode` is `"proof"`, `--mode proof` is passed, or `-v` is passed at all (even without `--mode`). Their WASM can carry the non-deterministic opcodes (`forall`, `exists`, `assume`, `unique`, `@` uzumaki) that `wasm-opt` cannot parse, and they are a different artifact class from an executable.
- **`target = "stellar"` refuses the table outright, at load time.** Declaring both is an invalid manifest for every command, not a silently-skipped step: a Stellar contract's value-ABI wrappers and its `contractenvmetav0` metadata section are the layer a host is trusted to decode, and whether an external `wasm-opt` preserves them depends on which Binaryen the machine has — a version nothing in the manifest pins. Remove one of the two keys. This is a refusal about those two artifacts and not about non-default targets in general: `target = "spacewasm"` keeps the table, because its module is the `"wasm32"` one and has neither of them.
- **A compile-mode artifact that still contains a non-deterministic opcode is a hard error**, not a silent skip. Compile-mode builds strip `spec` blocks, so a well-formed executable should never carry one of these opcodes — if it does, `infs` scans for it before invoking `wasm-opt` (which would otherwise fail with an opaque parse error) and reports the offending construct by name, with remediation: move it into a `spec` block, or turn optimization off.

#### Disabling optimization for one invocation

Pass `--no-wasm-opt` to skip `[build.wasm-opt]` for a single `infs build` or `infs run` without editing the manifest:

```bash
infs build --no-wasm-opt
infs run --no-wasm-opt
```

#### Resolving the `wasm-opt` binary

`infs` resolves `wasm-opt` through three precedence tiers, in order:

1. **`WASM_OPT_PATH`** environment variable, if set. It must point at an existing file, or the build errors naming the variable and the invalid path — a set override is never silently discarded in favor of a lower tier, even when `auto-install = true`.
2. **PATH** — a standard lookup for `wasm-opt`. This tier wins over the managed tier below, so a system-installed `wasm-opt` on PATH always takes precedence over an infs-managed one.
3. **Managed tools** — the pinned Binaryen installed by `infs component add wasm-opt` (or by `auto-install`, see below) at `~/.inference/tools/binaryen/<version>/`.

Set `INFS_VERBOSE=1` to trace which tier resolved `wasm-opt` to stderr (`infs: resolved wasm-opt via <tier>: <path>`).

If no tier resolves, the build fails with install hints led by `infs component add wasm-opt`, followed by the system package managers (`brew install binaryen`, `apt install binaryen`, `npm install -g binaryen`) and a link to the [Binaryen releases page](https://github.com/WebAssembly/binaryen/releases). Set `auto-install = true` to have `infs` provision Binaryen automatically at build time instead of erroring.

The resolved binary must report **Binaryen 116 or newer** (`wasm-opt --version`); an older version is a hard error naming both the found and required versions. If `--version` cannot be run or its output cannot be parsed, `infs` warns and proceeds rather than blocking the build over an unrecognized binary. This check runs against whichever binary was resolved, managed installs included.

#### Caveats

- **Function names are dropped.** `wasm-opt` strips the WASM names custom section, so stack traces and any tooling that resolves function names from an optimized `out/main.wasm` will not see them. There is currently no flag to preserve it.
- **Deterministic per Binaryen version, not across versions.** The same source, flags, and Binaryen version always produce identical optimized bytes, but upgrading Binaryen can change the output even for unchanged input. Do not treat an optimized `.wasm` as a stable byte-for-byte reference across toolchain upgrades.
- **An optimized library can no longer be verified against precisely.** A compiled module records which of its functions trap rather than wrap on arithmetic overflow, by function index, in the `inference.checked` custom section — and the static-merge linker reads that record out of every library it links, to refuse an `exists`/`unique` specification that reaches a body which traps. `wasm-opt` carries the section through untouched while inlining, removing and reordering the functions its indices name, so the record survives the optimizer and stops being true. That is measured against the Binaryen `infs` manages, not inferred: inline one private guarded helper at `-Oz` and the record comes back byte-identical over a function space it no longer describes.

  `infs` therefore rewrites the record after optimizing, into a form that says only that *some* function of the module traps. A later link reads that as *every* function of the module and refuses any reachability specification reaching it, rather than checking a list that has quietly gone stale. The build prints a line saying so whenever it marks an artifact.

  This costs nothing for a program you build and run. It matters for a library, and it reaches most of them: `+`, `-`, `*` and unary `-` trap unless the source wrote `wrapping(...)` around them, so a library with any arithmetic at all carries the record and is marked once it is optimized. From then on no `exists`- or `unique`-quantified specification in a project that links it may reach it, whatever it calls. If some other project states one over a call into your library, build the library without `[build.wasm-opt]` (or with `--no-wasm-opt`) so its record stays exact. `forall` specifications and ordinary linking are unaffected either way, and a module whose arithmetic is written `wrapping(...)` throughout records nothing and is never marked.

### [verification]

The `[verification]` section configures Rocq (Coq) proof generation.

#### Fields

- **`output-dir`** (string, default: `"proofs/"`): The directory for generated Rocq proofs
  - Path is relative to the project root
  - Honored only when the effective build mode is `proof` (either via `[build]
    mode = "proof"` or `--mode proof` on the CLI). In compile mode this field
    is ignored entirely.
  - In proof mode, `infs build` forwards the normalized path to `infc` as
    `--out-dir`, which moves both the `.wasm` and `.v` artifacts. With the
    default `"proofs/"` a proof build writes both files under `<root>/proofs/`.
  - Must be a relative path inside the project root. Absolute paths, `..`
    traversals, and drive/UNC prefixes are rejected.

#### Example

```toml
[verification]
output-dir = "artifacts/"
```

## Complete Example

```toml
[package]
name = "verified-sort"
version = "2.1.0"
infc_version = "0.1.0"
description = "A formally verified sorting algorithm"
authors = [
    "Alice Johnson <alice@example.com>",
    "Bob Smith <bob@example.com>"
]
license = "MIT"

[dependencies]
# Future: package dependencies

[build]
target = "wasm32"
optimize = "release"
mode = "proof"

[verification]
output-dir = "proofs/"
```

## Field Evolution

### Version History

#### Current Version (0.1.0)

**Package section:**
- `infc_version` (String, semver): Records the compiler version used to create the project
  - Replaces the deprecated `manifest_version` field
  - Automatically detected from `infc --version` or falls back to `infs` version

**Removed fields:**
- `manifest_version` (u32): No longer used
- `edition` (String): Removed, no longer needed

Because `[package]` now rejects keys it does not recognize, a manifest that still
carries one of these fails to load rather than ignoring it. Delete the key; no
replacement is needed.

## Validation Rules

### Project Name Validation

The `name` field is validated according to these rules:

1. Cannot be empty
2. Must start with a letter (`a-z`, `A-Z`) or underscore (`_`)
3. Can only contain:
   - Letters (`a-z`, `A-Z`)
   - Numbers (`0-9`)
   - Underscores (`_`)
   - Hyphens (`-`)
4. Cannot be a reserved keyword:
   - Language keywords: `fn`, `let`, `mut`, `if`, `else`, `match`, `return`, `type`, `struct`, `impl`, `trait`, `pub`, `use`, `mod`, `assume`, `assert`, `forall`, `exists`, `unique`, etc.
   - Directory names: `src`, `out`, `target`, `proofs`, `tests`, `self`, `super`, `crate`

### Version Validation

Both `version` and `infc_version` must be valid [semantic versions](https://semver.org/):
- Format: `MAJOR.MINOR.PATCH` (e.g., `1.0.0`)
- Optional pre-release suffix (e.g., `0.1.0-alpha`)
- Cannot be empty

## Creating a New Project

### Using `infs new`

```bash
infs new myproject
```

Creates a new project with:
- `Inference.toml` manifest
- `src/main.inf` entry point
- `tests/` and `proofs/` directories
- `.gitignore` and `.gitkeep` files
- Initialized git repository

To skip git initialization:

```bash
infs new myproject --no-git
```

This creates only the core project files without `.gitignore`, `.gitkeep`, or running `git init`.

### Using `infs init`

```bash
mkdir myproject
cd myproject
infs init
```

Initializes an `Inference.toml` in an existing directory, using the directory name as the project name.

If a `.git/` directory exists, `infs init` will also create `.gitignore` and `.gitkeep` files (without overwriting existing ones).

### Custom Project Name

```bash
infs init custom-name
```

Creates an `Inference.toml` with `name = "custom-name"` regardless of the directory name.

## Compiler Version Detection

When creating a new project, the `infc_version` field is automatically populated using the following logic:

1. Try to run `infc --version` and parse the output
2. If `infc` is not found or the command fails, use the `infs` version from `CARGO_PKG_VERSION`

This ensures that the manifest always records which compiler version was used to create the project, enabling reproducible builds and compatibility tracking.

All Inference ecosystem crates (`infs`, `infc`, and `core/*` libraries) share the same version number, so using the `infs` version as a fallback is safe and accurate.

## Related Documentation

- [Project Scaffolding Guide](./project-scaffolding.md) (if exists)
- [Build Configuration](./build-config.md) (if exists)
- [Verification Workflow](./verification.md) (if exists)

## References

- [TOML Specification](https://toml.io/)
- [Semantic Versioning](https://semver.org/)
- [Inference Language Specification](https://github.com/Inferara/inference-language-spec)
