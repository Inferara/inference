# inference-ide

The feature layer of the Inference IDE stack: plain-old-data answers to the
questions an editor asks about a document. [`AnalysisHost`] owns the
open-document state (delegating to `ide-db`'s `RootDatabase`); [`Analysis`]
borrows it to answer feature queries — diagnostics, document symbols, hover,
goto-definition, completions, and inlay hints.

## Where It Sits

```
apps/lsp
    |
ide/ide  -----re-exports position PODs from-----> ide/ide-db
    |
ide/ide-db -> ide/base-db -> ide/vfs
```

Every result this crate returns is a plain struct in editor terminology
(`Diagnostic`, `DocumentSymbol`, `Hover`, `NavigationTarget`, `CompletionItem`,
`InlayHint`) — **no compiler type crosses this boundary**. The protocol layer
above (`apps/lsp`) maps these straight onto LSP responses without needing to
know anything about `inference_ast`, `inference_type_checker`, or
`inference_analysis`.

## Coordinates

Positions in and out of this crate are **byte offsets** into a document's
current text, and ranges are byte ranges — not LSP line/character. The
protocol layer converts them with the [`LineIndex`] this crate re-exports from
`ide-db`. An open document is addressed by its path; the entry file's own
module path is the empty slice, which is how a query reaches the document it
was asked about rather than one of its imports.

## Design: Single Document, Single Thread

Each open file is analyzed as its own project entry (its import closure
resolved through the overlay-then-disk loader in `ide-db`), and the resulting
analysis answers every query for that document — including goto-definition
into an imported file, whose `NavigationTarget` carries that file's real path
and ranges in that file's own coordinates (`closure_line_index` fetches the
right `LineIndex` for it without re-analyzing the target as its own entry).

A query borrows the database with `&mut self` because the analysis is computed
lazily and memoized on first use. This is not an accident of implementation —
it is exactly the access pattern the LSP main loop needs: it is
single-threaded by design (see `apps/lsp`), so there is never a second caller
to conflict with the mutable borrow.

## Feature-Per-Module Layout

| Module | Feature | Depends on |
|---|---|---|
| `diagnostics.rs` | Merged, sorted `Diagnostic`s: syntax, import, type, and analysis-rule findings | `FileAnalysis` |
| `document_symbols.rs` | The definition hierarchy (functions, structs with fields/methods, enums with variants, specs) | `FileAnalysis`, `syntax.rs` |
| `hover.rs` | Type and documentation for the position under the cursor | `type_render.rs`, `nondet_docs.rs`, `syntax.rs` |
| `goto_definition.rs` | Resolves an identifier to its declaration, possibly in another file | `syntax.rs` |
| `completions.rs` | Keyword / local / top-level-def / imported-module suggestions, or struct-member-only after `.` | `type_render.rs`, `syntax.rs` |
| `inlay_hints.rs` | Non-det block and uzumaki (`@`) annotations | `nondet_docs.rs`, `syntax.rs` |
| `nondet_docs.rs` | The verbatim hover/inlay text for `forall`/`exists`/`unique`/`assume`/`@` | — |
| `syntax.rs` | Shared per-file AST navigation: child enumeration, name lookup, signature extraction | `ide-db` (`NodeHit`, `file_defs`) |
| `type_render.rs` | Renders a checked `TypeInfo` as a source-like string for hovers and completions | `inference-type-checker` |

`lib.rs` wires these together: `AnalysisHost` owns the `RootDatabase`;
`Analysis<'_>` is a thin borrowing façade whose methods each resolve the
document's `FileAnalysis` and delegate to the matching module.

## Features

### Diagnostics

`Analysis::diagnostics(path)` merges four sources into one sorted list, each
tagged with a `code`: `"syntax"` (resilient parser errors), `"import"`
(unresolved or broken imports — a broken *imported* file surfaces as one
summary diagnostic anchored on the `use` directive that pulls it in, not as its
own per-file-local errors), `"type"` (structured type-check diagnostics via
`inference::type_check_with_diagnostics`), and an analysis rule id (`"A001"`
through `"A041"`, see `core/analysis`). Only the *entry* file's own diagnostics
are returned — an imported file's offsets are local to that file and would be
misplaced if surfaced directly.

### Hover

`Analysis::hover(path, offset)` dispatches on what covers the position:

- A non-det block keyword (`forall` / `exists` / `unique` / `assume`) returns
  its **verification meaning** — what proof obligation the block introduces —
  authored once in `nondet_docs.rs` and served verbatim, so the wording never
  drifts between the hover and the inlay hint. For example, hovering `forall`
  explains that it fans out one computation path per value of every `@` inside
  it and requires *all* paths to succeed, and that it lowers to the `BI_forall`
  quantifier constructor in the generated Rocq.
- A `@` (uzumaki) expression gets its own explanation: that it is not a random
  pick but a value standing for *every* value of its type at once, quantified
  by the enclosing non-det block.
- An identifier resolves by its syntactic role: a definition's own name shows
  its one-line signature; a parameter or field shows its type; a call callee
  (free function, method, or `::`-qualified) shows the target's signature,
  resolved cross-file when the target is defined in an imported module; a
  struct-literal name or field shows the struct's signature or the field's
  type.
- A type annotation or a typed expression falls back to rendering its checked
  `TypeInfo` (`type_render.rs`).

### Goto Definition

`Analysis::goto_definition(path, offset)` resolves the identifier at a
position to its declaration, wherever it lives. A `NavigationTarget` always
carries the target file's own absolute path and byte ranges in that file's own
coordinates, because offsets are per-file-local in the merged arena — a
cross-file jump into an imported struct's field, an imported constant, or a
`lib::helper()` call all resolve correctly, using the type checker's recorded
call target rather than re-deriving it from source text.

### Document Symbols

`Analysis::document_symbols(path)` builds the outline of the entry file: every
top-level definition, with struct fields and methods, enum variants, and
spec-nested definitions as children. Each symbol carries both a whole-`range`
(for "reveal declaration") and a narrower `selection_range` spanning just the
name (for "highlight this identifier").

### Completions

`Analysis::completions(path, offset)` distinguishes two contexts. Right after
a `.` whose receiver has a known struct type, only that struct's fields and
*instance* methods (those taking `self`) are offered — an associated function
reachable only as `Type::make()` is excluded. Everywhere else, the suggestions
are every reserved keyword (kept in sync with the parser's keyword table),
locals in scope at the cursor (params, plus `let` bindings declared strictly
before it — Inference forbids shadowing, so this is unambiguous), the
document's own top-level definitions, and the modules it imports together with
their `pub` top-level definitions.

### Inlay Hints

`Analysis::inlay_hints(path, range)` places a short annotation right after
every non-det block's opening keyword (e.g. `▸ every path must succeed` after
`forall`) and right after every `@`, with the uzumaki's concrete declared type
appended when known (`▸ ranges over every value of its type (i32)`). An
optional `range` clips the result to an editor's visible viewport.

## Usage

```rust
use std::path::PathBuf;
use inference_ide::AnalysisHost;

let mut host = AnalysisHost::default();
let path = PathBuf::from("/project/src/main.inf");
host.open_document(&path, "fn add(a: i32, b: i32) -> i32 { return a + b; }");

let mut analysis = host.analysis();
assert!(analysis.diagnostics(&path).is_empty());
assert_eq!(analysis.document_symbols(&path).len(), 1);

let offset = "fn add".len() as u32 - 3; // the `add` identifier
assert!(analysis.hover(&path, offset).is_some());
```

## Testing

Each feature module carries its own unit tests, built on the shared
`test_utils.rs` helpers (`single`, `with_lib`, and byte-offset finders `at` /
`after` / `nth` that read the offset out of the source text rather than
hardcoding it). `lib.rs` additionally tests the `AnalysisHost` lifecycle:
open → query, change → reanalyze, close → still usable, and
`closure_line_index` serving an imported file's line index without
re-analyzing it as its own entry.

```
cargo test -p inference-ide
```

## Related Resources

- [`ide/ide-db`](../ide-db/README.md) — `RootDatabase`, `FileAnalysis`, `hit_test`, `file_defs`
- [`ide/base-db`](../base-db/README.md) — `LineIndex`, `TextRange`, re-exported here
- [`apps/lsp`](../../apps/lsp/README.md) — the protocol layer that maps this crate's PODs onto LSP
- [`core/type-checker`](../../core/type-checker/README.md) — `TypeInfo`, rendered by `type_render.rs`
