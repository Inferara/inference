# The Language Server

This chapter explains how Inference ships IDE support: the `inference-lsp`
server in `apps/lsp` and the four-crate IDE stack under `ide/` that it is built
on. It covers the layering that keeps the protocol, the features, and the
compiler apart; the thread architecture that keeps typing responsive even though
analysis is strictly serial; the Salsa-based memoization that makes repeated
queries cheap; and the resilience story — what happens when the compiler panics
underneath an editor session.

## Design goals

Four constraints shape everything in the stack:

- **Real compiler answers.** Diagnostics, hovers, and completions come from the
  same parser, type checker, and analysis rules that `infc` runs — not from a
  parallel re-implementation that would drift. What the editor underlines is
  exactly what the build would reject.
- **The editor's buffer is the source of truth.** The user's unsaved text — the
  *overlay* — takes priority over whatever is on disk, for the open document
  and for every file its imports reach.
- **Serial analysis, responsive editing.** The semantic stack holds `!Send`
  state, so analysis is strictly serial: one computation at a time. A keystroke
  must nevertheless never wait behind a stale request, which forces an
  interruption mechanism rather than a concurrency one (issue #157).
- **A compiler bug must not take the session down.** The type checker and the
  analysis passes are under active development; a `todo!()` reached through
  some half-typed input has to cost one request, not the editor session.

## The layered stack

The server is the thin protocol shell on top of a strictly layered set of
crates:

```text
  editors/vscode      LanguageClient, speaks LSP over stdio
        │
  apps/lsp            inference-lsp — router / analysis worker / read pool
        │
  ide/ide             inference-ide — feature API (AnalysisHost, Analysis)
        │
  ide/ide-db          inference-ide-db — Salsa database, memoized analyses
        │                              │
  ide/base-db         positions        core/… — the compiler front end
  ide/vfs             file identity      (parser, type-checker, analysis)
```

| Crate | Responsibility |
|-------|----------------|
| `ide/vfs` | Path interning (`FileId`) and the open-document overlay. No file I/O of its own; paths are stored as given, not canonicalized. |
| `ide/base-db` | The position vocabulary: byte-offset `TextRange` on the compiler side, 0-based UTF-16 `LineCol` on the LSP side, and the `LineIndex` that converts between them. |
| `ide/ide-db` | The Salsa database (`RootDatabase`): open-document bookkeeping, the memoized per-file analysis, eviction, and cancellation. The only IDE crate that depends on compiler crates. |
| `ide/ide` | The feature layer: `AnalysisHost` / `Analysis` and one module per feature (diagnostics, hover, goto definition, completions, document symbols, inlay hints). |
| `apps/lsp` | The `inference-lsp` binary: JSON-RPC over stdio, request routing, threads, and lifecycle. |

Two dependency firewalls hold the layering in place:

- **The protocol layer never sees the semantic machinery.** `apps/lsp` depends
  on `inference-ide` alone and talks to it in paths, byte offsets, and
  `ide`-owned result types — no compiler type crosses the boundary. A guard
  test (`apps/lsp/tests/no_salsa_in_lsp.rs`) fails the build if any source line
  in the crate so much as mentions Salsa, naming the offending `file:line`.
- **The IDE never links the backend.** `ide-db` reaches the compiler through
  the leaf `inference-project-model` crate plus the parser, type-checker, and
  analysis crates — not through the `inference` orchestration crate — so WASM
  code generation and the Rocq translator are never compiled into the editor
  toolchain.

## Protocol choices

The server is built on [`lsp-server`](https://crates.io/crates/lsp-server) —
the same minimal, synchronous crate rust-analyzer uses — rather than an
async framework like `tower-lsp`. Analysis is serial and CPU-bound; an async
runtime would add scheduling machinery without adding concurrency where it
matters. Threads and crossbeam channels model the actual shape of the work.

The transport is stdio: stdout carries framed JSON-RPC exclusively, and all
logging goes to stderr. The capability set advertised at `initialize`
(`apps/lsp/src/capabilities.rs`) is deliberately small and fully implemented:

| Capability | Detail |
|-----------|--------|
| Text sync | **Full** — the client sends the whole document on every change |
| Diagnostics | Push (`textDocument/publishDiagnostics`) |
| Hover | Markdown or plain text, negotiated from client capabilities |
| Goto definition | Cross-file, within the document's import closure |
| Completions | Triggered on `.` and `:` |
| Document symbols | Hierarchical or flat, negotiated |
| Inlay hints | Non-deterministic block annotations |

Position encoding is left unnegotiated, so the LSP default of UTF-16 applies —
the one encoding the `convert` module translates the compiler's byte offsets
into.

Full-text sync is a deliberate v1 choice, not an oversight: the
[resilient parser](parser.md) re-parses a document in well under a millisecond,
`AnalysisHost::change_document` replaces the overlay wholesale, and the
closure-aware invalidation in `ide-db` already makes reanalysis cheap. An
incremental protocol would add a delta-application layer with nothing
downstream able to exploit the deltas.

## A router, a worker, and a read pool

`apps/lsp/src/server.rs` splits a session across three kinds of thread, joined
under `std::thread::scope`:

```text
  stdin ──► router ──Job{epoch,message}──► analysis worker ──► responses,
              │                              │        ▲          publishes
              │ $/cancelRequest,             │        │ WorkerEvent
              │ cancellation firing      ReadTask     │
              │                              ▼        │
              └──────────────────────────  read pool (2 threads)
```

- The **router** reads the transport and forwards every message to the worker
  *instantly* over an unbounded channel. It handles inline only what must not
  wait behind an analysis: request-id bookkeeping, `$/cancelRequest`, and —
  for a document write it adopts, and for shutdown/exit — firing cancellation
  of the worker's in-flight analysis *before* forwarding the message.
- The **analysis worker** owns the `ServerState` (the `AnalysisHost` plus
  per-document bookkeeping) and processes jobs one at a time, in arrival
  order. Every response and every published diagnostic leaves from here, with
  one exception: the router answers a cancelled request's `-32800` itself.
- The **read pool** — two threads — serves pure read requests off database
  snapshots concurrently with the worker (covered [below](#serving-pure-reads-concurrently)).

Why this shape? Analysis cannot be parallelized (the semantic state is
`!Send`), but it can be *interrupted*. The router is the always-listening ear:
because it never computes, it can always fire the cancellation flag the moment
a newer write makes the in-flight computation moot. The worker is "the message
loop, one thread over" — the serial semantics are preserved exactly, but a
stale analysis now dies in microseconds instead of finishing on principle.

Every thread that runs analysis gets a 64 MiB stack (mirroring
rust-analyzer's main-loop stack). The pipeline recurses with the input's
nesting depth, and a stack overflow *aborts* the process — it cannot be caught
— so the only mitigation is headroom.

### Cancellation: one epoch, two meanings

Cancellation is driven by a single monotonic **write epoch** paired with a
Salsa cancellation token (`AnalysisCancelSource` in `ide/ide-db`). The router
bumps the epoch and fires the token *before* forwarding an adopted write, and
stamps the forwarded job with the post-bump epoch. The analysis polls the token
at checkpoints between pipeline stages and unwinds when it is set.

When the worker catches a cancellation unwind, the epoch disambiguates it:

- **Superseded** — the source's epoch is newer than the job's: a write landed
  after this job was routed. The request is answered `ContentModified`
  (`-32801`) so the client retries against the new content; the cache is left
  intact.
- **Residual self-cancel** — the epochs match: the unwind consumed a signal
  meant for earlier work (the write's own eager publish, for example). The
  work is simply retried; a genuinely newer write always carries a newer
  epoch, which bounds the retry.

Stamping the write's job with the *post*-bump epoch is the crux of the
protocol: it is what lets the worker classify the write's own follow-up work
as current rather than cancelling it with the very signal the write fired.

A client's `$/cancelRequest` is deliberately weaker, matching rust-analyzer: a
still-pending request is answered `-32800` immediately and its late response
suppressed, but the in-flight compute is not interrupted. Only writes preempt
computation, because only writes make it wrong.

## Coalescing keystrokes and deferring dependents

The unbounded job channel is the buffer a typing burst accumulates in. At
dequeue, the worker drains whatever has piled up and collapses **consecutive
`didChange` notifications for the same document** into the final text
(`coalesced_job_batch`), so a burst of keystrokes runs the pipeline a handful
of times instead of once each. The collapse is conservative: a `didOpen` or
`didClose` for that document, or *any* request, is a barrier the coalescer
never reorders across, and no non-`didChange` job is ever dropped.

A change to one file can also invalidate another open document whose import
closure includes it. The worker publishes eagerly only for the changed
document; every other invalidated document goes into a pending-republish set
that drains when the loop next goes idle — after the interactive request that
arrived right behind the keystroke has been answered. A feature request that
hits a queued document publishes it fresh immediately, so the client never
keeps a stale diagnostic set; documents the change did not touch keep their
memoized analysis and are never republished at all.

## Memoizing analyses with Salsa

`ide-db` is built on [Salsa](https://github.com/salsa-rs/salsa) (pinned to
`0.27`, matching rust-analyzer) — but it uses Salsa very differently than
rust-analyzer does.

### One coarse query

There is a single memoized query, `analyze_entry`: its body is the *entire*
front-end pipeline for one document — resilient project load through the
overlay-then-disk loader, lossless type check, all analysis rules — producing
one `FileAnalysis`. rust-analyzer decomposes analysis into hundreds of
fine-grained queries so an edit recomputes only slivers; Inference's pipeline
is fast enough to run whole-document, so the memoization boundary sits at the
document instead. That trades incremental granularity for a radically simpler
invariant: a document's analysis is either memoized and current, or it is
recomputed from scratch.

### Inputs and edges

Salsa can only track what goes through its inputs, and file *content*
deliberately does not: the analysis reads files through the same
overlay-then-disk `Vfs` loader seam the compiler uses, which must stay
Salsa-free so compiler and IDE share one import-resolution path. The database
therefore represents change signals, not content, as inputs:

- `EntryInput { path, src_root, evicted }` — a project entry's identity, plus
  the eviction lever (below);
- `FileStamp { stamp }` — an opaque monotonic counter per reachable file,
  bumped on any overlay write to that path;
- `AvailabilityEpoch` — a singleton bumped when a `didOpen` makes overlay
  content available where there was none.

Invalidation is **edge-driven**: after computing, the query registers a
dependency on the stamp of every file in the import closure it actually
resolved. Bumping one path's stamp then invalidates exactly the memos whose
closure contains that path. A file that failed to resolve names no path at
all, so the query additionally reads the availability epoch *only when its
parse recorded an unresolved import* — a deliberately coarse edge that
re-fires exactly the analyses a newly opened file might fix.

### Eviction by sentinel swap

Salsa 0.27 has no per-memo eviction: a memoized value lives as long as its
input. But a closed document's analysis — or one computed for a never-opened
path a feature request touched — must be releasable. The database frees them
with a **sentinel swap**: setting the entry's `evicted` input invalidates the
memo, and the next recompute routes to a tiny sentinel value, which pushes the
fat analysis onto Salsa's deleted list to be freed at the next revision
boundary. Un-evicting takes one input write back and forces exactly one full
recompute.

Open documents are never evicted. Never-opened analyses are capped at
`MAX_UNOPENED_ANALYSES = 8` in FIFO order, which bounds the resident set of a
session at roughly *open documents + 8* fat analyses.

## Serving pure reads concurrently

Serial analysis has one more cost: with a single worker, a slow request blocks
a fast one even when both only *read*. The read pool removes that without
touching the serial-write invariant.

For the five read-only methods (hover, goto definition, completions, document
symbols, inlay hints), the worker asks the database for a `ReadPlan`. If the
document's analysis is memoized — or stale in a way that can be recomputed
against a cached source root — the plan carries an `AnalysisSnapshot`: a
second database handle cloned from the same Salsa storage, sharing the overlay
and the memo table. The request is handed to a pool thread, which serves it
off the snapshot and sends the response itself; the worker moves on
immediately. Everything else — writes, diagnostics publishes, and any read
that cannot be planned concurrently — stays on the serial path.

The safety argument leans on Salsa's own synchronization: an overlay write
bumps the file stamp *first*, and Salsa's setter blocks until every
outstanding snapshot handle is dropped — while the fired cancellation token
makes the readers holding those handles unwind at their next checkpoint. So a
write can stall only microseconds behind a reader, and a reader can never
observe a half-applied write. A read that loses this race is *routed back* to
the worker and served serially under its original epoch, preserving the exact
supersede-or-answer classification it would have had on the serial path.

Two pool threads are enough to overlap an interactive request with a slow one,
while bounding the wasted partial computes when a write cancels the pool
mid-flight.

## Resilience: containing the compiler

Every request and every notification is dispatched inside `catch_unwind`, and
the catch classifies the unwind: a *cancellation* follows the epoch protocol
above; a *panic* is contained.

A panicking request is answered `InternalError` with its original id, so the
client can correlate the failure. A panicking notification publishes nothing.
In both cases the analysis host — which the unwinding computation may have
left half-updated — is discarded and **rebuilt from the tracked open
documents' last-seen text**, without reading anything from disk (the overlay
may never have been saved). The first query afterwards recomputes from
scratch; every other document keeps working. Before this boundary existed, a
panic during `didOpen` was fatal — and because clients re-send the same
`didOpen` on restart, the server crash-looped until the client gave up.

Two deliberate non-recoveries: a stack overflow aborts the process (the 64 MiB
stacks are the mitigation, not a catch), and a `didChange` for a document that
was never opened — a protocol violation some clients commit in tab-close races
— is logged and dropped rather than guessed at.

## Shutdown

After answering `shutdown`, the server performs **no further idle work**: the
pending-republish queue is abandoned rather than flushed, because LSP 3.17
forbids notifications after `shutdown` — the client can no longer act on a
publish, and the router fired cancellation ahead of the `shutdown` request, so
draining the queue would recompute stale entries under a set cancellation flag
and stall teardown behind doomed analyses.

What is *not* abandoned is a response owed to a pre-shutdown request: a read
still parked in the route-back path is answered `ContentModified` rather than
dropped. A response is not a notification — it stays protocol-legal after
`shutdown`, and dropping it would leave a request id dangling in the client.
The `exit` notification then ends the loop; the scoped threads join on every
path, so teardown cannot hang.

## Diagnostics come from the real front end

Publishing diagnostics for a document runs the full pipeline: project load
(overlay first, disk second) rooted at the document's source root, the
lossless type check, and every analysis rule. Four diagnostic sources merge
into one sorted, deduplicated list, each tagged with a stable `code`:

| Source | Code | Example |
|--------|------|---------|
| Parser | `syntax` | unterminated string, missing `;` |
| Import resolution | `import` | unresolved `use`, broken imported file |
| Type checker | `type` | mismatched types, unknown name |
| Analysis rules | `A001`…`A047` | non-det block constraints (see [Static Analysis](static-analysis.md)) |

Only the entry document's own diagnostics are published — errors inside an
imported file are that file's diagnostics when *it* is open, though a broken
import is still summarized on the `use` directive that names it.

The analysis model is **per-document**: each open file is analyzed as its own
project entry together with its import closure, and there is no shared
project-wide index in v1 — two open documents that import the same file each
analyze it independently. That costs duplicate work but keeps a hard
simplicity: no cross-document consistency protocol, no workspace indexing
phase, and eviction that follows document lifecycle directly.

One feature is worth singling out: hovering a non-deterministic construct —
`forall`, `exists`, `unique`, `assume`, or `@` — answers with its
*verification* meaning, including how it lowers into Rocq (for example, that a
`forall` block becomes a `BI_forall` obligation). The prose is authored once,
in `ide/ide/src/nondet_docs.rs`, and inlay hints annotate the same constructs.
For a language whose semantics live partly in the proof world, hover is
documentation infrastructure, not a convenience.

## The VS Code extension

The extension (`editors/vscode`) is a thin `vscode-languageclient` shell. It
resolves the server binary in strict order: the `inference.lsp.path` setting
(used verbatim — a configured but non-executable path is an error, not a
fallback), then the managed toolchain at `<INFERENCE_HOME>/bin/inference-lsp`,
then `PATH`. Missing everywhere means the language features quietly stay off;
the editor remains usable.

Restarts are imperative and serialized: switching or installing a toolchain
version through the extension's own commands, changing any `inference.lsp.*`
setting, or invoking the manual restart command all funnel into one
stop-then-start path. What does *not* restart the server is mutating the
toolchain behind the extension's back (an `infs default` in a terminal) — the
extension deliberately does not watch the filesystem for that.

## Comparison with rust-analyzer

The stack borrows rust-analyzer's load-bearing decisions and diverges where
Inference's scale allows something simpler:

**Borrowed:** the `lsp-server` crate and its synchronous connection model; a
Salsa database at the semantic core; the 64 MiB analysis stack; the
cancel-and-retry discipline where writes preempt reads and superseded requests
answer `ContentModified`; `$/cancelRequest` as bookkeeping only; the
VFS-with-overlay file model.

**Diverged:** one coarse memoized query per document instead of a fine-grained
query graph — Inference's whole front end runs in the time rust-analyzer
budgets for a fraction of one feature, so incrementality below the document
level is not yet worth its complexity. A fixed two-thread read pool with
explicit route-back instead of a snapshot per request. Full-text sync instead
of incremental edits. Per-document projects instead of a workspace-wide crate
graph. And no filesystem watching in v1: what the editor has not opened, the
server sees only through disk reads at analysis time.

## Testing and verification

The server's logic lives in `ServerState`, which does no I/O — one request in,
one response out — so the unit tests in `apps/lsp/src/server.rs` drive
routing, coalescing, cancellation classification, panic recovery, and shutdown
semantics directly, without a transport.

End to end, `apps/lsp/tests/e2e.rs` spawns the *compiled server binary* and
speaks framed JSON-RPC over its real stdio through a timeout-guarded test
client (`tests/harness/`). The scenarios cover the full lifecycle —
initialize through exit — plus the awkward cases: malformed frames, requests
before initialization, cancellation races, panic containment, and stdout
hygiene (a full session must write nothing to stdout that is not framed
protocol).

Two guard tests keep the architecture honest: the no-Salsa-in-`apps/lsp` scan
described above, and a drift guard tying the pool-eligible method list to the
pool dispatcher. Debug-only seams (deliberate slow-downs, panics, and
rendezvous points injected into the analysis path) let the concurrency tests
force the interleavings — a write landing mid-read, a pool panic, a shutdown
during a drain — that real editors produce only rarely and never on demand.

## Summary

The language server is a thin, synchronous protocol shell over a strictly
layered IDE stack. A router thread keeps the server listening while a single
analysis worker preserves the compiler's serial semantics; a write epoch
threaded through every job turns cancellation into a precise
supersede-or-retry decision; and a small read pool serves memoized reads
concurrently without weakening the serial-write invariant. Salsa memoizes one
coarse analysis per document, invalidated edge-wise through per-file change
stamps and evicted by sentinel swap. Diagnostics run the real front end, so
the editor and the build can never disagree. And the whole stack is built to
survive its own compiler: a panic costs one request and one cache, never the
session.

## References

- [LSP 3.17 specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/)
- [rust-analyzer architecture](https://github.com/rust-lang/rust-analyzer/blob/master/docs/book/src/contributing/architecture.md)
- [`lsp-server`](https://crates.io/crates/lsp-server) — rust-analyzer's synchronous LSP scaffold
- [Salsa](https://github.com/salsa-rs/salsa) — incremental computation framework
- `apps/lsp/README.md`, `ide/ide-db/README.md`, `ide/ide/README.md` — the per-crate documentation this chapter condenses
