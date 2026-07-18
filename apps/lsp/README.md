# inference-lsp

The Language Server Protocol server for Inference. A synchronous,
single-threaded stdio binary built on `lsp-server` that answers diagnostics,
hover, goto-definition, completion, document symbols, and inlay hints for
`.inf` source, delegating all analysis to the `ide` stack and confining every
protocol concern (framing, position encoding, URIs) to this crate.

## Where It Sits

This is the top of the IDE stack — the only crate here that speaks JSON-RPC
and `lsp-types`. Everything below it speaks Rust structs, byte offsets, and
`Path`s:

```
apps/lsp            (this crate)
  |  lsp-server 0.8 stdio loop, lsp-types 0.97 protocol values
  |  URI <-> Path, byte offset <-> LSP Position conversion
  v
ide/ide              AnalysisHost / Analysis: feature API, editor-terminology PODs
  |
  v
ide/ide-db           RootDatabase: per-open-file FileAnalysis, closure-aware invalidation
  |             \
  v              \-> core/inference, core/analysis, core/type-checker, core/ast, core/parser
ide/base-db          LineIndex, TextRange, LineCol, FilePosition, FileRange
  |
  v
ide/vfs              FileId interning + open-document overlay (no file I/O)
```

Only `ide-db` depends on the compiler crates; `apps/lsp` and `ide/ide` never
name a compiler type. A change to a compiler internal (a new AST node kind, a
new type-checker error variant) is therefore contained to `ide-db` unless it
also needs a new editor-facing feature.

## Why Single-Threaded

`inference_type_checker::typed_context::TypedContext` — which every
`FileAnalysis` holds — is `!Send`. `ide::Analysis` methods take `&mut self` for
exactly this reason: the whole stack is designed around one thread owning one
`AnalysisHost` and answering one LSP message at a time. `server::run` reflects
this directly: it is a plain `for message in &connection.receiver` loop with no
worker pool, no `tokio`, and no interior mutability wider than what
`ServerState` itself needs. This is a deliberate v1 simplicity trade-off, not
an oversight — a request that is slow to answer (a large file's full
re-analysis) blocks the next message, but every analysis here is bounded by one
open file's import closure, not a whole workspace.

## Capability Surface

Advertised once, in `capabilities::server_capabilities()`, during the
`initialize` handshake:

| Capability | Value |
|---|---|
| `textDocumentSync` | `Full` — every `didChange` carries the whole new document text, not incremental edits |
| `hoverProvider` | `true` |
| `definitionProvider` | `true` |
| `completionProvider` | trigger characters `.` and `:`, `resolveProvider: false` |
| `documentSymbolProvider` | `true` (hierarchical or flat, negotiated — see below) |
| `inlayHintProvider` | `true` |
| `positionEncoding` | left unset, so the client falls back to the LSP-default UTF-16 — the only encoding this server converts to or from |

Full-text sync is the deliberate choice for v1: `AnalysisHost::change_document`
takes the complete new text and re-derives everything from it, so there is no
incremental-edit bookkeeping to keep consistent with the analysis cache — the
closure-aware invalidation in `ide-db` already makes a full reanalysis cheap
enough (it only touches analyses whose import closure includes the changed
path).

Document symbols are negotiated: `server::hierarchical_symbol_support` reads
`initialize`'s `textDocument.documentSymbol.hierarchicalDocumentSymbolSupport`
capability, and the server replies with a nested `DocumentSymbol` tree when the
client supports it or a flattened `SymbolInformation` list (each carrying its
enclosing symbol's name as `containerName`) when it does not.

## Message Loop

`server::ServerState` holds the `AnalysisHost` plus a map of open documents
(`Uri -> Document { path, version }`) and turns one request into one
`Response`, or one notification into the diagnostics to publish — with no I/O
of its own, which is what makes it directly unit-testable (see `server.rs`'s
own tests). `server::run` owns the transport: it reads messages off
`connection.receiver`, handles the `shutdown`/`exit` handshake inline, routes
everything else through `ServerState`, and writes results back. An unknown
request method is `MethodNotFound`; params that fail to deserialize are
`InvalidParams` — neither ever panics or disturbs the loop, so one malformed
request cannot take the server down.

The shutdown handshake is handled in the loop rather than delegated to
`lsp-server`'s `Connection::handle_shutdown` (which consumes the next message
itself and turns anything but `exit` into a fatal protocol error): a `shutdown`
request is answered and flips a `shutting_down` flag, after which every further
request is answered with `InvalidRequest` (`-32600`, the spec's behaviour for a
request received between `shutdown` and `exit`) and every notification but
`exit` is dropped, until `exit` ends the loop.

A single document notification republishes **every** open document, not just
the notified one. Editing one file can invalidate another open document whose
import closure includes it (`ide-db` drops exactly those analyses), and the
client would otherwise keep rendering the dependent's stale diagnostics. This
is bounded: an unaffected document's analysis is still memoized, so its
republish recomputes nothing, and an editor keeps only a handful of files open.

`handlers.rs` holds one function per LSP method. Each resolves the document's
path from its URI, converts the LSP position(s) to a byte offset using the
*correct* file's `LineIndex` (a cross-file goto-definition target is converted
with the target file's own index, fetched via `Analysis::closure_line_index`
without re-analyzing that file as its own entry), asks the `ide` layer, and
converts the answer back with `convert.rs`. A URI this server cannot map to a
file — a non-`file` scheme, an untitled buffer — yields a null result and no
diagnostics, never a panic.

Nothing in this crate ever writes to stdout except the framed JSON-RPC
messages themselves; all logging goes to stderr (see `main.rs`). This is
required by the stdio transport — anything else on stdout corrupts the
protocol stream — and is asserted directly by an end-to-end test (below).

## Resilience and Known Limitations

- **Deep-nesting stack headroom.** The analysis pipeline (type-checker,
  analysis passes) recurses with the input's nesting depth, so a pathological
  or generated document could overflow the default stack and abort the whole
  process — losing every open document's state. `main.rs` runs the server loop
  on a dedicated thread with a 64 MiB stack (mirroring rust-analyzer), which
  clears realistic deep nesting by a wide margin (a document that overflowed the
  default main-thread stack at ~800 levels survives past 5000 with the larger
  stack). A stack overflow *aborts* rather than unwinds, so a worker thread
  cannot catch it; the mitigation is headroom, not isolation. An input deep
  enough to exhaust even 64 MiB would still abort — bounding the recursion in
  the shared pipeline (out of scope for this crate) would be the complete fix.

- **An unwinding analysis panic is contained, not fatal.** An ordinary panic in
  the analysis stack (a `todo!` or `unwrap` in the type-checker or analysis
  passes) *unwinds* — unlike a stack overflow — so the message loop catches it:
  each request and notification is dispatched inside `std::panic::catch_unwind`.
  A panicking request is answered with a JSON-RPC `InternalError` carrying its
  original id; a panicking notification publishes nothing and rebuilds the
  analysis host from the tracked open documents, so the session continues from
  consistent state instead of aborting and letting the client crash-loop the
  server into a permanent outage. The panic still reaches stderr through the
  default hook (never stdout, the protocol channel). This is the recoverable
  counterpart to the stack-overflow case above.

- **A malformed transport frame is fatal.** `lsp-server`'s stdio reader treats
  any framing or body parse failure — an empty body (`Content-Length: 0`), a
  non-JSON body, or an unparsable `Content-Length` — as fatal to the connection.
  It owns the stdin reader thread and exposes no seam to answer JSON-RPC
  `-32700` and resync, so `io_threads.join()` surfaces the failure and the
  process exits rather than skipping the bad frame. Recovering would require
  replacing `Connection::stdio()` with a vendored reader; rust-analyzer accepts
  the same limitation on `lsp-server`, and a well-behaved client does not emit
  malformed frames, so this is documented rather than worked around.

## URI Handling

`uri.rs` is the one place in the server that reasons about `file://`
percent-encoding and Windows drive letters (`lsp-types` models a URI with
`fluent-uri`, which offers no path helpers of its own). It supports local
`file://` URIs with an empty or `localhost` authority only; a non-`file` scheme,
a remote/UNC authority, or a URI carrying a query or fragment maps to `None`,
which every caller treats as "not a document this server can analyze" rather
than an error. A query/fragment is *rejected* rather than stripped: a raw `?` or
`#` cannot be a literal path byte (a literal one arrives percent-encoded), so a
URI carrying one is not a plain document spelling and answering from a
`?`-truncated path would only mint a wrong document identity.

Drive-letter handling is host-shaped, because the `/X:/…` form is a Windows
drive path *only on Windows* — on POSIX it is a genuine absolute path whose
first component is a directory named `X:`. The string core takes an explicit
`windows` flag (`cfg!(windows)` in the public wrappers, a parameter so both
behaviours stay testable from either host):

- On **Windows**, `file:///c%3A/…` and `file:///C:/…` both canonicalize to
  `C:/…` (leading slash dropped, drive letter upper-cased to the std canonical
  spelling), so a mixed-case client spelling names one interned document rather
  than two; and a backslash in a path is normalized to `/`.
- On **POSIX**, `/c:/…` maps to itself (still absolute, case preserved) and a
  backslash is an ordinary filename byte (percent-encoded `%5C`), so a real file
  named `a\b.inf` round-trips exactly.

Round-tripped forms include: POSIX absolute paths, paths containing spaces or
non-ASCII characters (percent-encoded as raw UTF-8 bytes, so a multi-byte
character split across several `%XX` escapes still decodes correctly), Windows
drive paths with the URI's required leading slash (`C:/Users/x` ⇄
`file:///C:/Users/x`), and the percent-encoded drive colon some clients (e.g.
VS Code) send (`file:///c%3A/...`).

## Wiring an Editor

Any LSP client that can spawn a child process and speak stdio JSON-RPC can
drive this server. The essential configuration, in client-agnostic form:

```jsonc
{
  "command": "inference-lsp",
  "args": [],
  // No CLI flags: the server takes no arguments and speaks LSP purely over
  // stdio, logging only to stderr.
  "filetypes": ["inference"],
  "rootUri": null,  // no workspace-wide indexing: each opened file is
                     // analyzed as its own project entry (see ide-db)
  "initializationOptions": {},
  "settings": {}
}
```

Practical notes for wiring a real client:

- Register the server for `*.inf` files with language id `"inference"` — this
  is the id the server's own e2e test suite and unit tests use for `didOpen`.
- The server advertises `TextDocumentSyncKind::Full`, so a client configured
  for incremental sync must still be told to send full-document
  `contentChanges` for this server (most LSP client libraries expose a sync
  kind negotiated automatically from server capabilities; this is rarely a
  manual setting).
- No `rootUri` / workspace-folder behavior is required or used: since every
  file is analyzed as its own entry, the server works identically whether the
  client provides a workspace root or opens a single loose file.
- Declaring
  `textDocument.documentSymbol.hierarchicalDocumentSymbolSupport: true` in the
  client's `initialize` capabilities gets the nested `DocumentSymbol` tree
  instead of the flattened list.

## End-to-End Test Suite

`tests/e2e.rs` spawns the actual compiled `inference-lsp` binary (via
`env!("CARGO_BIN_EXE_inference-lsp")`) and drives a full protocol session
through a minimal hand-written client (`tests/harness/mod.rs`), asserting on
raw JSON rather than a typed re-encoding of it — so a test failure reflects
what a real client would actually see on the wire. Every read is bounded by a
10-second timeout, so a regression that hangs the server fails the test
instead of stalling the run; fixtures live in per-test unique temp
directories, never at a filesystem root and never inside the repo, so the
suite is parallel-safe.

The suite is organized into twenty-one scenarios:

1. **Initialize handshake** — the exact capability set is advertised, no
   `positionEncoding` is negotiated, no `serverInfo` is sent
2. **`didOpen` on a clean file** — publishes an empty diagnostics set
3. **`didOpen` with a syntax error** — publishes a `"syntax"` diagnostic
4. **`didChange` fixing the error** — diagnostics clear
5. **`didChange` introducing a type error** — the new diagnostic carries a
   location
6. **An analysis-rule finding (A041)** — a duplicate local surfaces with the
   rule's code
7. **Hover** — over a local variable's type, and over `forall` (explains the
   non-det construct)
8. **Goto-definition, same file** — reaches a same-file function
9. **Cross-file goto-definition** — importing a sibling file *on disk*
   resolves into it
10. **Missing import** — reported on the `use` directive
11. **`documentSymbol`** — both the hierarchical tree and the flattened form
    for a non-hierarchical client
12. **Completion** — top-level keywords/defs, and member-only completions
    after `.`
13. **Inlay hints** — every non-det construct in a file gets annotated
14. **UTF-16 positions** — positions resolve correctly past a multi-byte
    string literal
15. **`didClose`** — clears the document's diagnostics
16. **Robustness** — an unknown method, malformed params, and a non-`file` URI
    each fail gracefully and leave the server usable for the next request
17. **Shutdown / exit** — both the well-behaved sequence and `exit` without a
    prior `shutdown`
18. **Stdout hygiene** — a full session (open, edit, hover, goto, complete,
    inlay hints) writes *only* well-framed protocol to stdout — nothing else
    ever leaks onto the transport
19. **Cross-file republish** — editing an imported file republishes the open
    dependent document, so its diagnostics never go stale
20. **Post-shutdown request** — a request arriving after `shutdown` is answered
    `InvalidRequest` (`-32600`), and the server still exits cleanly on `exit`
21. **Query/fragment URI** — a `file://` URI carrying a query is ignored, never
    interned as a garbage path, and the server stays responsive

Unit tests for the protocol-adjacent logic live alongside their modules:
`capabilities.rs` (the advertised JSON shape), `convert.rs` (every PDO ⇄
`lsp_types` mapping, including UTF-16/surrogate-pair round-trips), `uri.rs`
(the URI ⇄ path table above), and `server.rs` (routing, error codes, and the
did-open/did-close diagnostics-publish contract), plus `handlers.rs`'s own
integration with `ServerState`.

```
cargo test -p inference-lsp
```

## Related Resources

- [`ide/ide`](../../ide/ide/README.md) — the feature API this crate's handlers call
- [`ide/ide-db`](../../ide/ide-db/README.md) — per-file analysis and closure-aware invalidation
- [`ide/base-db`](../../ide/base-db/README.md) — `LineIndex`, the byte-offset ⇄ LSP-position bridge
- [`ide/vfs`](../../ide/vfs/README.md) — the open-document overlay `AnalysisHost` wraps
- [Language Server Protocol Specification](https://microsoft.github.io/language-server-protocol/) — the wire protocol this crate implements
- [`lsp-server`](https://docs.rs/lsp-server) / [`lsp-types`](https://docs.rs/lsp-types) — the crates this server is built on
