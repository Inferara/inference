# inference-ide-db

The semantic database for the Inference IDE stack. `ide-db` sits above `vfs`
(path ↔ id ↔ content overlay) and `base-db` (line index and position PODs) and
below the feature layer (`ide/ide`). It answers one question — *"what does
this open file mean?"* — by analyzing each open document as its own project
entry and caching the result.

## Where It Sits

```
apps/lsp
    |
ide/ide
    |
ide/ide-db  -----consumes-----> core/project-model, core/analysis,
    |                            core/type-checker, core/ast, core/parser
ide/base-db
    |
ide/vfs
```

`ide-db` is the first layer in the IDE stack that depends on the compiler. It
is also the *only* IDE layer that does: `ide/ide` above it never names a
compiler type in its public API, and everything below it (`vfs`, `base-db`) is
compiler-independent plumbing. It reaches the compiler's front end through the
leaf `core/project-model` crate rather than the `inference` orchestration crate,
so the IDE stack never links the WASM/Rocq backend.

## What It Owns

- **[`RootDatabase`]** — the open-document overlay (a `Vfs`) plus a
  Salsa-memoized [`FileAnalysis`] per entry file, with closure-aware
  invalidation so a keystroke in one buffer does not force every other open
  buffer to re-analyze. Open documents' analyses are never evicted; a closed
  document's overlay-derived analysis is freed (recomputed from disk on demand),
  and analyses memoized for never-opened paths (feature requests on arbitrary
  URIs) are FIFO-capped, with the evicted ones freed too. Salsa 0.27 has no
  per-memo eviction, so freeing works by a two-step sentinel swap: an evicted
  entry recomputes to a tiny sentinel, which pushes the superseded analysis onto
  Salsa's deleted list to be dropped at the next revision boundary. Resident full
  analyses are bounded by *open documents + [`MAX_UNOPENED_ANALYSES`] + a
  one-write-lagged transient* (a superseded or swapped memo freed at the next
  Salsa write). A small per-path residue (input slots, memo headers, `Vfs` ids)
  is session-permanent because Salsa 0.27 has no input removal — the same steady
  state rust-analyzer ships. Serving hits from a cloned `Storage` on a background
  thread is separate later work (issue #292).
- **[`FileAnalysis`]** — the merged arena (reached through its `TypedContext`),
  per-file parse errors, structured type diagnostics, unresolved-import
  problems, tagged analysis findings, and per-closure-file line indexes and
  paths.
- **[`hit_test`]** — position → AST node resolution, scoped to one file.
- **[`file_defs`]** — a pre-order walk of every definition in one file,
  including struct methods and spec-nested definitions.

## What It Does Not Do

It leaks no protocol types: every result is either a plain struct defined here
or a compiler type re-exported verbatim (`TypeCheckError`, `AnalysisDiagnostic`,
`NodeId`, `Location`, …). The feature layer above (`ide/ide`) translates these
into editor-terminology PODs; the protocol layer above that (`apps/lsp`)
translates *those* into LSP JSON. Import resolution is **not** reimplemented
here — the closure walk lives in `core/project-model` behind a `FileLoader` seam,
and `ide-db` drives it with an overlay-then-disk loader, so the compiler and
the IDE resolve imports identically by construction.

## Design: Every Open File Is Its Own Project Entry

`RootDatabase` does not model a single fixed project the way a build tool
does. Each file the editor opens is analyzed **as its own project entry** — its
own directory is the source root its imports resolve against — and the
resulting `FileAnalysis` answers every query for that document, including
goto-definition into a file it imports. This means:

- Opening one file in a multi-file project is enough to get diagnostics,
  hover, and navigation for it; there is no "open the workspace root first"
  step.
- The same imported file, if also opened directly by the editor, is analyzed a
  *second* time as its own separate entry. This is deliberate duplication, not
  a cache miss: v1 has no shared, project-wide semantic index, only
  per-entry-file analyses. It is simple, always correct, and the duplicated
  work is bounded by how many files the editor happens to have open.

### Salsa memoization

Per-entry analyses are memoized by [Salsa](https://github.com/salsa-rs/salsa)
(pinned to `0.27`, matching rust-analyzer). `RootDatabase` is a `#[salsa::db]`;
a single tracked query runs the whole `FileAnalysis::compute` body, so a
repeated request returns the framework's memo rather than recomputing. The
query surface still takes `&mut self` — a read memoizes in place, driven from
the single-threaded LSP loop — with a shared read-handle model left to later
work.

### Cancellation

`RootDatabase::bind_cancellation` couples the database handle to an
[`AnalysisCancelSource`] — a monotonic write epoch plus the handle's cancellation
token. A request through the source unwinds the handle's in-flight analysis at
its next checkpoint: the tracked query polls at the fetch entry and passes
`FileAnalysis::compute` a hook it invokes between the load, type-check, and
rule-running stages, so a long analysis is interruptible rather than run to
completion. The unwind delivers the framework's cancellation payload;
[`is_cancellation`] recognizes it so a caller can tell a superseded analysis
apart from a genuine panic without naming the framework. A cancelled compute
writes no result — its pre-query bookkeeping is idempotent setup that reconverges
on retry — so the entry is left in the consistent invalidated shape and the
framework auto-resets the consumed signal, leaving a retry free to recompute.
Both `AnalysisCancelSource` and `is_cancellation` are re-exported through
`ide/ide`.

Salsa does **not** see the file reads themselves: the import closure is read
through the `Vfs` overlay-then-disk loader, which stays outside Salsa storage
so the compiler and IDE keep resolving imports through one seam. What Salsa
*can* see is supplied for it. The analysis query, once it knows the closure it
just read, registers a **per-file change-stamp** edge for every file in that
closure, plus a single **availability-epoch** edge when an import went
unresolved. The write path then bumps those inputs on the matching editor event
(`bump_file_stamp` on any overlay mutation, `bump_availability_epoch` on an open
that makes content newly available), so a change the loader seam hides still
forces exactly the affected memos to recompute. Recompute forcing lives entirely
in these Salsa edges — not in dropping a hand-rolled map entry.

Alongside the edges, the write path keeps an eager **mirror**: each entry's
latest analysis, cleared to `None` the moment a change makes it stale
(`RootDatabase::note_stale_entries`). The mirror forces no recompute; it exists
so the editor-facing bookkeeping that must answer *before* any query re-runs —
`is_analyzed`, the protocol layer's republish sweep, the closure-donor search,
and the never-opened cap — has a write-time view of what a change invalidated.
Its predicate reads the very `closure_paths`/`had_missing_import` fields the
query registered its edges from, so the mirror and the edges cannot disagree (a
debug assertion in `RootDatabase::analysis` machine-checks the alignment).

A stale memo is only *marked*, so "invalidated" means "will be recomputed before
it is served again". Reclaiming an entry's memory is a separate, explicit act:
closing a document, a cap eviction, or a change that stales a never-opened entry
sets an `evicted` flag on the entry's input and queues a **sentinel swap**. The
next `analysis` call recomputes the evicted entry to a roughly two-word sentinel,
which pushes the superseded analysis onto Salsa's deleted list; Salsa frees that
list at the next revision boundary (a version-pinned 0.27 behavior). So at most
one fat memo is ever pending, and a requery un-evicts the entry with a single
false-write that forces exactly one fresh recompute. Resident full analyses are
bounded by *open documents + [`MAX_UNOPENED_ANALYSES`] + a one-write-lagged
transient*; a small per-path metadata residue is session-permanent (Salsa 0.27
has no input removal). Serving hits from a cloned `Storage` snapshot is separate
later work (issue #292).

### Closure-aware invalidation

A keystroke in one buffer must not force every other open buffer to
re-analyze. Every `FileAnalysis` records the absolute paths of every file in
its import closure, and the query registers a change-stamp edge for each, so a
content change to path `P` recomputes only the analyses whose closure contains
`P` — the write path bumps `P`'s stamp and Salsa re-runs exactly those memos.
`RootDatabase::note_stale_entries` clears the same set's mirror in the same turn.

One extra case a per-file stamp cannot cover: opening a **previously unseen**
path can satisfy an import that was missing before, but a missing import was
never in any closure (there was no file to record, and so no stamp to bump). So
the query registers one more edge — the availability epoch — whenever it
recorded an unresolved import, and an open that makes overlay content newly
available bumps that epoch, recomputing every analysis that had a missing
import. This is a deliberately coarse over-approximation — it may recompute an
analysis whose specific missing import is unrelated to the newly-opened file —
chosen because it is simple and always correct: the edge is read only by memos
whose last compute actually had a missing import, so a resolved analysis stops
reacting to it. A file that appears on disk without being opened is not
observed; there is no filesystem watch in v1.

Analyses and the overlay are keyed by exact path spelling. A caller that may
refer to one file by two spellings must canonicalize before calling in, so the
same file always arrives under one path (the LSP layer does this once, per
`ide/vfs`'s path-identity contract).

### The overlay-then-disk `FileLoader`

`core/project-model` exposes import resolution behind a `FileLoader` trait — a
seam with exactly two methods, `exists` and `read` — so the same closure-walk
logic drives both the compiler (`DiskLoader`, straight to `std::fs`) and the IDE.
This crate's `VfsLoader` implements that trait by consulting the editor's `Vfs`
overlay first and falling back to disk, so an open, unsaved buffer shadows its
on-disk contents while an import the editor has never opened is still read
from disk. Driving `inference_project_model::load_project_resilient` through this
loader is what guarantees the compiler and the IDE can never disagree about which
files a program imports — there is exactly one resolution algorithm, parameterized
over where bytes come from.

`FileAnalysis::compute` builds the loader, calls `load_project_resilient_with_root`
(which never fails fast — every file is parsed resiliently and every problem,
from a broken import to a syntax error, is collected as data rather than
aborting), then type-checks the merged arena losslessly with
`inference_type_checker::check_with_diagnostics` and runs every registered
analysis rule (`inference_analysis::rules::all_rules()`) over the resulting
`TypedContext`.

## Per-File-Local Offsets

In the merged multi-file arena, every file's byte offsets start at zero — so an
offset alone never names a file. Both `hit_test` and `file_defs` are therefore
always scoped to one `SourceFileId`: the walk starts at that file's own
top-level definitions and descends only through the ids they own, never
crossing into another file. A naive arena-wide scan by offset would return
false hits from same-numbered positions in an unrelated file; a two-file test
in `hit_test.rs` (`scoped_to_one_file_in_a_two_file_arena`) exists specifically
to guard this.

## Key Types

| Type | Role |
|---|---|
| `RootDatabase` | Owns the `Vfs` overlay and the per-entry-file `FileAnalysis` cache |
| `FileAnalysis` | One entry file's memoized analysis: arena, `TypedContext`, parse errors, type errors, import problems, findings |
| `ClosureFile` | One file's path, source text, and `LineIndex` within an analysis closure |
| `AnalysisFinding` | One rule finding, tagged with its rule id (`"A035"`) and `Severity` |
| `NodeHit` | Result of [`hit_test`]: the smallest covering node plus its ancestor chain, outermost first |

## Usage

```rust
use std::path::Path;
use inference_ide_db::RootDatabase;

let mut db = RootDatabase::default();
let path = Path::new("/project/src/main.inf");
db.open_document(path, "fn main() -> i32 { return 0; }");

// Computed lazily on first request, memoized until invalidated.
let analysis = db.analysis(path);
assert!(analysis.type_errors().is_empty());
assert!(analysis.findings().is_empty());

// A subsequent edit invalidates only analyses whose closure contains `path`.
db.change_document(path, "fn main() -> i32 { return x; }");
assert!(!db.analysis(path).type_errors().is_empty());
```

## Testing

Unit tests live alongside each module (`analysis.rs`, `hit_test.rs`,
`symbols.rs`, `loader.rs`). Integration tests in `tests/database.rs` exercise
`RootDatabase` end-to-end: overlay-beats-disk precedence, cross-file
`typed_context` queries into an imported file, missing-import recording with
location and module path, a broken imported file still leaving the entry
analyzable, `use root;` and self-import deduplication, mutually-importing
files terminating, and every closure-invalidation case (a change to an
imported file invalidates the entry, a change to an unrelated open file does
not, and opening a previously-unseen file re-triggers analyses that had a
missing import).

```
cargo test -p inference-ide-db
```

## Related Resources

- [`ide/vfs`](../vfs/README.md) — the path/overlay store `RootDatabase` wraps
- [`ide/base-db`](../base-db/README.md) — `LineIndex` and the position PODs re-exported here
- [`ide/ide`](../ide/README.md) — the feature layer built on `FileAnalysis`
- [`core/project-model`](../../core/project-model/README.md) — `load_project_resilient`, `FileLoader`, and manifest source-root discovery, the leaf front end this crate drives
- [`core/type-checker`](../../core/type-checker/README.md) — `check_with_diagnostics`, the lossless type-check `FileAnalysis` runs
- [`core/analysis`](../../core/analysis/README.md) — the rules run over every `FileAnalysis`
