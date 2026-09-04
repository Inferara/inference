# inference-project-model

The shared project front end for the Inference toolchain: the one place that
turns an entry `.inf` file into the set of files a program is made of. It walks
the import-reachable closure from an entry point, reads and parses each file
exactly once, and lowers them all into a single `AstArena`.

## Where It Sits

```
core/inference (compiler)        ide/ide-db (IDE)
        \                          /
         \                        /
          core/project-model  <--+
                 |
       inference-parser, inference-ast, toml, rustc-hash
```

`project-model` is a **leaf** crate. It depends only on the parser, the AST data
model, `toml`, and `rustc-hash` — never on the type checker, code generator, or
any WASM/Rocq crate. That is deliberate: both the compiler and the IDE stack
depend on it, and keeping it a leaf is what lets `ide-db` reach the project walk
without transitively linking the backend it never uses.

## Why the Compiler and the IDE Share It

A diagnostic the IDE shows about a missing import, and the set of files the
compiler decides a program is made of, must never disagree. This crate makes
that guarantee *structural* rather than a matter of two implementations staying
in sync: there is exactly one closure-walk algorithm, parameterized over where
bytes come from through the [`FileLoader`] seam — a trait with two methods,
`exists` and `read`.

- The **compiler** drives the walk with a [`DiskLoader`] (straight to
  `std::fs`) via [`parse_project`], which fails fast on the first problem,
  preserving the exact errors and ordering it has always produced.
- The **IDE** drives the same walk with an overlay-then-disk loader via
  [`load_project_resilient`], which never fails fast: every file is parsed
  resiliently and every problem — a syntax error, an unresolved import, an
  unreadable file — is collected as data so an editor can serve features on the
  healthy parts of a broken program.

`core/inference` re-exports every public item here, so compiler-side consumers
(`infc`, `infs`, tools, tests) keep reaching them as `inference::…` unchanged.

## What It Owns

- **The closure walk** — breadth-first discovery of the import-reachable set
  from an entry file, keyed by canonical module path so cycles terminate, with
  files lowered into one arena in canonical order (entry first, then imports
  sorted by module path).
- **The [`FileLoader`] seam** and its [`DiskLoader`] implementation.
- **Reading a source file** — [`read_source_file`] / [`strip_utf8_bom`], the one
  reader that both the disk and overlay loaders build on so a leading UTF-8 BOM
  is stripped identically everywhere.
- **Project errors** — [`InferenceError`], the structured errors the fail-fast
  walk raises.
- **The `use` path-segment grammar** — every segment of a *path-form* `use`
  directive, the project-import form, is validated as an Inference identifier
  (`[A-Za-z_][A-Za-z0-9_]*`) before it is turned into a module path, rejecting
  anything else with `InferenceError::InvalidImportSegment`. The `from`-form
  (`use { f } from lib;`) names an external module rather than a project one and
  is skipped here, resolving through `inference::wasm_link` instead. This is the
  one place a *project* module-path segment is minted, so it is the only gate on
  the alphabet everything downstream derives from one: the filesystem path a
  `use` resolves to, and the WASM `name`-section symbol code generation later
  writes for a non-entry-file function (a `.`-join of the module path with the
  item name; a spec-inner function is carved out of that qualification and keeps
  a bare symbol).
  A segment carrying a `:` would let a program function's own name-section
  symbol collide with the `::`-joined naming the static-merge linker uses for
  a merged external body — see
  [`core/fn-key`](../fn-key/README.md#the-wasm-name-section-namespace). The
  lexer already produces only identifiers, so no source a user can write
  reaches this check and fails it; it exists to tie the invariant to where
  segments are created rather than leave it implied by a grammar three crates
  away.
- **Manifest source-root discovery** — [`manifest_source_root`], which derives
  the `<manifest_dir>/src` root an opened file's imports resolve against so the
  IDE resolves exactly as `infs` would.

## Two Outcomes

| Type | Produced by | Shape |
|---|---|---|
| `ProjectParse` | `parse_project` | The merged `arena` plus `ProjectWarning`s for unreachable files |
| `ResilientProjectParse` | `load_project_resilient[_with_root]` | The merged `arena`, per-file parse errors, unresolved-import problems, the loaded-file list, and read failures |

## Key Types

| Type | Role |
|---|---|
| `FileLoader` | The `exists`/`read` seam the walk resolves bytes through |
| `DiskLoader` | The `FileLoader` the compiler uses, reading straight from disk |
| `LoadedFile` | One reachable file: its canonical module path and the path it was read from |
| `FileParseErrors` | A file's own syntax errors, labeled with its module path |
| `ImportProblem` | A `use` that resolved to no file, anchored at its directive for in-place IDE diagnostics |
| `ProjectWarning` | A non-fatal finding (currently only `UnreachableFile`) |
| `InferenceError` | The structured error the fail-fast walk raises |

## Usage

```rust
use std::path::Path;
use inference_project_model::parse_project;

// Compiler front end: fail fast, one arena for the whole program.
let entry = Path::new("/project/src/main.inf");
let project = parse_project(entry)?;
for warning in &project.warnings {
    eprintln!("{warning}");
}
let arena = project.arena; // All reachable files, in canonical order.
# Ok::<(), anyhow::Error>(())
```

The IDE drives the resilient entry points through its own `FileLoader` (see
`ide/ide-db`), so an open, unsaved buffer shadows on-disk contents while both
the compiler and the editor resolve imports the same way.

## Testing

Unit tests live alongside each module (`project.rs`, `manifest.rs`). They cover
the closure walk end to end — cycle termination, canonical ordering, missing
imports with nearest-match suggestions, resilient-vs-fail-fast arena parity,
UTF-8 BOM handling, filesystem-root safety, and manifest source-root derivation.

```
cargo test -p inference-project-model
```

## Related Resources

- [`core/inference`](../inference/README.md) — the orchestration crate that
  re-exports this front end and adds type-checking, codegen, and Rocq translation
- [`core/parser`](../parser/README.md) — the resilient parser the walk drives
- [`ide/ide-db`](../../ide/ide-db/README.md) — the IDE consumer that drives the
  resilient walk through an overlay-then-disk `FileLoader`
