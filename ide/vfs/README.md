# inference-vfs

In-memory file identity and an open-document content overlay for the Inference
IDE stack. This is the lowest layer of the `ide/` crates: it gives every other
IDE component a small, `Copy` [`FileId`] to key maps and diagnostics on instead
of passing `PathBuf`s around, and it holds the in-memory text of whatever files
the editor currently has open.

## Where It Sits

```
apps/lsp
    |
ide/ide -> ide/ide-db -> ide/base-db -> ide/vfs
```

`vfs` depends on nothing but `rustc-hash` (for `FxHashMap`). Every other `ide/`
crate depends on it, directly or transitively, for `FileId`.

## What It Owns

- **Identity** — [`Vfs::intern`] maps a `Path` to a stable `FileId`. Interning is
  idempotent: interning the same path twice returns the same id, and ids are
  dense, zero-based, and allocation-ordered, so a downstream crate can use one
  as a vector index if it wants to.
- **Overlay** — while a document is open in the editor, its authoritative text
  lives in the editor's buffer, not on disk. [`Vfs::set_contents`] stores that
  text as an `Arc<str>`; [`Vfs::contents`] and [`Vfs::contents_of_path`] read it
  back. A file with no overlay entry means "not open" — the caller is expected
  to fall back to disk one layer up.

## Why No File I/O Happens Here

This crate never touches `std::fs`. It only remembers paths and buffers callers
hand it. The LSP layer feeds it absolute paths derived from client URIs and the
current editor text on `didOpen`/`didChange`; the disk-fallback read for an
import the editor has never opened lives in `ide-db`'s `VfsLoader`, which
consults the overlay first and falls back to `std::fs::read_to_string`. Keeping
I/O out of this crate makes it trivially deterministic and testable — every test
in `src/lib.rs` runs against `PathBuf`s that were never expected to exist on
disk.

## Why Paths Are Stored, Not Canonicalized

Paths are interned exactly as given. This crate never calls
`std::fs::canonicalize`, so it never resolves symlinks or `..` components and
never touches the disk to do so. Identity is `std::path::Path` identity, which
compares by path component: it already ignores redundant `.` segments and
separator noise, but leaves `..` unresolved, so `/src/../src/a.inf` and
`/src/a.inf` intern as two distinct files.

A caller that needs canonical identity — so that one file is never
accidentally interned twice under two spellings — must normalize before
calling `intern`. The LSP layer does this: it derives one canonical absolute
path per `file://` URI and interns every file under that single spelling, so
`FileId` equality is reliable for every consumer built on top of this crate.

## Public API

```rust
use std::path::PathBuf;
use std::sync::Arc;
use inference_vfs::Vfs;

let mut vfs = Vfs::default();

// Interning is idempotent and returns dense, allocation-ordered ids.
let main = vfs.intern(&PathBuf::from("/project/src/main.inf"));
assert_eq!(vfs.intern(&PathBuf::from("/project/src/main.inf")), main);

// A file is "closed" (read from disk elsewhere) until it gets overlay text.
assert!(vfs.contents(main).is_none());

// An editor `didOpen` installs the buffer's current text.
vfs.set_contents(main, Arc::from("fn main() -> i32 { return 0; }"));
assert!(vfs.contents(main).is_some());

// An editor `didClose` drops the overlay; the path stays interned.
vfs.remove_contents(main);
assert_eq!(vfs.path(main), PathBuf::from("/project/src/main.inf"));
```

| Type / Method | Role |
|---|---|
| `FileId` | Opaque, `Copy` handle to an interned path; `.index()` exposes the dense `u32` backing it |
| `Vfs::intern(&Path) -> FileId` | Interns a path (idempotent) |
| `Vfs::file_id(&Path) -> Option<FileId>` | Looks up an already-interned path without allocating a new id |
| `Vfs::path(FileId) -> &Path` | Resolves an id back to its path |
| `Vfs::set_contents(FileId, Arc<str>)` | Installs or replaces overlay text (`didOpen` / `didChange`) |
| `Vfs::remove_contents(FileId)` | Drops overlay text, a no-op if absent (`didClose`) |
| `Vfs::contents(FileId) -> Option<Arc<str>>` | Reads current overlay text, if the document is open |
| `Vfs::contents_of_path(&Path) -> Option<Arc<str>>` | Same, addressed by path instead of id |
| `Vfs::overlays() -> impl Iterator<Item = (FileId, &str)>` | Every currently-open file, order unspecified |

A `FileId` is only meaningful to the `Vfs` that minted it — mixing ids from two
different `Vfs` instances is a caller error the type system does not catch.

## Testing

The crate's tests live inline in `src/lib.rs` and cover: idempotent interning,
distinct paths receiving distinct dense ids, path round-tripping, the
`..`-is-never-resolved guarantee, and every overlay transition (absent → set →
replaced → removed → absent), including that closing a document keeps the path
interned so a later `didOpen` of the same path returns the same `FileId`.

## Related Resources

- [`ide/base-db`](../base-db/README.md) — position primitives (`LineIndex`,
  `TextRange`, `FilePosition`, `FileRange`) built on top of this crate's `FileId`
- [`ide/ide-db`](../ide-db/README.md) — `RootDatabase`, which owns a `Vfs` and
  drives the overlay-then-disk `FileLoader` used for import resolution
- [Language Server Protocol Specification](https://microsoft.github.io/language-server-protocol/) — `textDocument/didOpen` / `didChange` / `didClose`, the editor lifecycle this crate's overlay mirrors
