# inference-base-db

Text-positioning primitives shared across the Inference IDE stack. This crate
holds the small, dependency-light plain-old-data (POD) types that let the
higher IDE layers talk about *where* something is in a file, plus the
[`LineIndex`] that converts between the compiler's byte offsets and the
line/character positions the Language Server Protocol speaks.

## Where It Sits

```
apps/lsp
    |
ide/ide -> ide/ide-db -> ide/base-db -> ide/vfs
```

`base-db` depends only on `ide/vfs` (for `FileId`). It has no dependency on any
compiler crate, and no compiler crate depends on it — it is IDE-only plumbing
that `ide-db` and everything above it build position handling on top of.

## Two Coordinate Systems

The compiler and the editor disagree about how to name a position in a file:

- The compiler's `Location` (in `core/ast`) carries byte `offset_start` /
  `offset_end` **and** 1-based line / 1-based *byte* column fields. The IDE
  stack uses only the byte **offsets**; it never reads the compiler's
  line/column fields, because they are 1-based and column-in-bytes, which is
  neither what LSP wants nor cheap to translate without re-scanning the line.
- LSP positions are 0-based line and 0-based **UTF-16 code unit** character —
  this is the position encoding [every LSP client must support](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocuments), regardless of what encoding the server would prefer.

[`LineIndex`] is the single bridge between the two: byte offset ⇄ [`LineCol`].
Everything else in this crate is a POD carrying a [`FileId`] alongside an
offset or range, with no conversion logic of its own.

Keeping this crate free of compiler dependencies is deliberate: it converts
offsets and nothing more, so it can be built, tested, and reasoned about in
isolation from parsing, type checking, or analysis.

## Key Types

| Type | Role |
|---|---|
| `TextRange { start: u32, end: u32 }` | A half-open byte range, `start..end`, `end` exclusive |
| `LineCol { line: u32, character: u32 }` | 0-based line, 0-based UTF-16 column — exactly LSP's `Position` shape |
| `FilePosition { file_id: FileId, offset: u32 }` | A byte offset scoped to a specific file |
| `FileRange { file_id: FileId, range: TextRange }` | A byte range scoped to a specific file |
| `LineIndex` | Owns a copy of one file's text; converts `u32` offset ⇄ `LineCol` |

All five types are `Clone + Copy` (`LineIndex` is `Clone` only, since it owns
text) and comparable with `PartialEq`/`Eq`, so they behave like the plain data
they are — no reference into an arena, no lifetime.

## `LineIndex`

`LineIndex::new(text: &str)` builds the index once by recording the byte offset
where every line starts (splitting on `'\n'`; a `'\r'` before it is *not* a line
terminator on its own and stays part of the preceding line's content). From
then on:

- `line_col(offset: u32) -> LineCol` converts a byte offset to a line/UTF-16
  column. An offset past the end of the text clamps to the end position; an
  offset that lands inside a multi-byte character rounds down to that
  character's start.
- `offset(line_col: LineCol) -> Option<u32>` converts the other way. It returns
  `None` only when the line itself is out of range; a `character` past the end
  of a valid line clamps to the line's end (the LSP-specified behavior), and a
  `character` that lands inside a character rounds down to that character's
  start.

### Why the index holds the text

UTF-16 column arithmetic needs to inspect the actual characters on a line — any
character above U+FFFF (an astral character, e.g. an emoji) is a surrogate pair
and counts as **two** UTF-16 units, not one. `LineIndex` therefore keeps an
owned copy of the source text and scans the relevant line on demand rather than
precomputing every column. This duplicates bytes already held elsewhere (in the
`Vfs` overlay or a `ClosureFile`), a deliberate v1 trade-off of a small amount
of memory for a self-contained, obviously-correct conversion with no dependency
on a second data structure being in sync.

## Usage

```rust
use inference_base_db::{LineIndex, LineCol};

let source = "fn main() -> i32 {\n    return 42;\n}";
let index = LineIndex::new(source);

// Byte offset of `42` -> LSP line/character.
let offset = source.find("42").unwrap() as u32;
assert_eq!(index.line_col(offset), LineCol { line: 1, character: 11 });

// And back: an LSP position -> byte offset.
assert_eq!(index.offset(LineCol { line: 1, character: 11 }), Some(offset));
```

## Testing

Unit tests live inline in `line_index.rs` and `lib.rs`. `line_index.rs` covers:
empty text, no trailing newline, a trailing newline producing an empty final
line, consecutive newlines, `\r` staying inside its line's content, two- and
three-byte UTF-8 characters that are one UTF-16 unit, four-byte astral
characters that are a UTF-16 surrogate pair (`😀`, `𝔘`), multi-byte content
spanning several lines, and a round-trip property test that every char-boundary
offset in a set of representative strings survives `line_col` → `offset`
unchanged. `lib.rs` covers the PODs' `Copy`/equality behavior.

## Related Resources

- [`ide/vfs`](../vfs/README.md) — the `FileId` these PODs are keyed on
- [`ide/ide-db`](../ide-db/README.md) — builds one `LineIndex` per file in an
  analysis closure (`ClosureFile::line_index`)
- [LSP Specification — Position](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#position) — the 0-based line / UTF-16 column contract `LineIndex` implements
