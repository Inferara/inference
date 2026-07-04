//! In-memory file identity and an open-document content overlay for the
//! Inference IDE stack.
//!
//! The [`Vfs`] serves two jobs that together decouple the rest of the IDE stack
//! from the operating system:
//!
//! * **Identity** — every path the IDE touches is interned to a small, `Copy`
//!   [`FileId`]. Downstream crates key their maps and diagnostics on `FileId`
//!   instead of passing paths around, and comparisons become integer
//!   comparisons.
//! * **Overlay** — while a document is open in the editor its authoritative text
//!   lives in the editor's buffer, not on disk. The overlay stores that
//!   in-memory text as an [`Arc<str>`] so an editor edit is reflected before (or
//!   instead of) any file write.
//!
//! # No file I/O
//!
//! This crate never touches `std::fs`. It only remembers paths and buffers that
//! callers hand to it. The LSP layer feeds absolute paths derived from client
//! URIs and the current editor text; the disk-fallback read for unopened
//! imports lives one layer up (in `ide-db`), where the VFS overlay is always
//! consulted first. Keeping I/O out of here makes the store trivially
//! deterministic and testable.
//!
//! # Paths are stored, not canonicalized
//!
//! Paths are interned exactly as given: this crate never calls
//! `std::fs::canonicalize`, so it never resolves symlinks or `..` components and
//! never touches the disk. Identity is std [`Path`] identity, which compares by
//! path component — it already ignores redundant `.` and separator noise but
//! leaves `..` unresolved, so `/src/../src/a.inf` and `/src/a.inf` are distinct
//! files here. Callers that need canonical identity must normalize before
//! interning; the LSP layer derives one canonical absolute path per URI and
//! interns each file under that single spelling.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustc_hash::FxHashMap;

/// An opaque handle identifying a file interned in a [`Vfs`].
///
/// `FileId`s are only ever minted by [`Vfs::intern`] and are dense and
/// allocation-ordered starting at zero, which lets downstream crates use them
/// as vector indices. A `FileId` is only meaningful to the `Vfs` that produced
/// it; mixing ids from different `Vfs` instances is a caller error.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct FileId(u32);

impl FileId {
    /// The dense zero-based index backing this id.
    ///
    /// Useful for indexing side tables keyed by file. Only ids minted by the
    /// same [`Vfs`] index that `Vfs`'s tables in bounds.
    #[must_use = "reading the index is pointless if the value is discarded"]
    pub fn index(self) -> u32 {
        self.0
    }
}

/// Path interner plus open-document content overlay.
///
/// Construct with [`Vfs::default`]. Interning is idempotent, so the same path
/// always maps to the same [`FileId`]; the overlay maps ids to their current
/// in-memory text and is edited independently of interning (a path can be
/// interned long before — or without ever — receiving overlay text).
#[derive(Debug, Default)]
pub struct Vfs {
    /// `FileId` (as index) → interned path. Never removed, so ids stay valid
    /// for the lifetime of the `Vfs`.
    paths: Vec<PathBuf>,
    /// Interned path → its `FileId`, the inverse of `paths`.
    ids: FxHashMap<PathBuf, FileId>,
    /// `FileId` → current in-memory document text. Present only while a document
    /// is open; absence means "not open" (read from disk one layer up).
    overlay: FxHashMap<FileId, Arc<str>>,
}

impl Vfs {
    /// Interns `path`, returning its stable [`FileId`].
    ///
    /// Idempotent: interning a path already known returns the existing id
    /// without allocating a new one. Distinct paths receive distinct ids.
    #[must_use = "the returned FileId is the only handle to the interned path"]
    pub fn intern(&mut self, path: &Path) -> FileId {
        if let Some(&id) = self.ids.get(path) {
            return id;
        }
        let id = FileId(self.paths.len() as u32);
        let owned = path.to_path_buf();
        self.paths.push(owned.clone());
        self.ids.insert(owned, id);
        id
    }

    /// Looks up the [`FileId`] a path was interned under, if any.
    #[must_use = "the lookup result must be inspected to be useful"]
    pub fn file_id(&self, path: &Path) -> Option<FileId> {
        self.ids.get(path).copied()
    }

    /// The path `file_id` was interned under.
    ///
    /// `file_id` must have been minted by this `Vfs`; ids are never invalidated,
    /// so any id this `Vfs` produced resolves here.
    #[must_use = "the resolved path is the reason to call this"]
    pub fn path(&self, file_id: FileId) -> &Path {
        &self.paths[file_id.0 as usize]
    }

    /// Sets or replaces the overlay text for `file_id` (an editor open or edit).
    pub fn set_contents(&mut self, file_id: FileId, contents: Arc<str>) {
        self.overlay.insert(file_id, contents);
    }

    /// Removes the overlay text for `file_id` (an editor close).
    ///
    /// A no-op if the file has no overlay text. The path stays interned; only
    /// the in-memory document is dropped.
    pub fn remove_contents(&mut self, file_id: FileId) {
        self.overlay.remove(&file_id);
    }

    /// The current overlay text for `file_id`, if the document is open.
    ///
    /// The returned [`Arc<str>`] is a cheap handle to the shared buffer.
    #[must_use = "the overlay text is the reason to call this"]
    pub fn contents(&self, file_id: FileId) -> Option<Arc<str>> {
        self.overlay.get(&file_id).cloned()
    }

    /// The current overlay text for `path`, if it is interned and open.
    #[must_use = "the overlay text is the reason to call this"]
    pub fn contents_of_path(&self, path: &Path) -> Option<Arc<str>> {
        self.contents(self.file_id(path)?)
    }

    /// Iterates every file that currently holds overlay text.
    ///
    /// Iteration order is unspecified (the overlay is a hash map). Yields each
    /// open file's id alongside a borrow of its text.
    #[must_use = "the iterator is lazy and does nothing unless consumed"]
    pub fn overlays(&self) -> impl Iterator<Item = (FileId, &str)> {
        self.overlay.iter().map(|(&id, text)| (id, text.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn intern_is_idempotent() {
        let mut vfs = Vfs::default();
        let a1 = vfs.intern(&p("/src/main.inf"));
        let a2 = vfs.intern(&p("/src/main.inf"));
        assert_eq!(a1, a2);
    }

    #[test]
    fn distinct_paths_get_distinct_ids() {
        let mut vfs = Vfs::default();
        let a = vfs.intern(&p("/src/a.inf"));
        let b = vfs.intern(&p("/src/b.inf"));
        let c = vfs.intern(&p("/src/c.inf"));
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
    }

    #[test]
    fn ids_are_dense_and_allocation_ordered() {
        let mut vfs = Vfs::default();
        let a = vfs.intern(&p("/a"));
        let b = vfs.intern(&p("/b"));
        let c = vfs.intern(&p("/c"));
        assert_eq!(a.index(), 0);
        assert_eq!(b.index(), 1);
        assert_eq!(c.index(), 2);
        // Re-interning does not advance the counter.
        let a_again = vfs.intern(&p("/a"));
        assert_eq!(a_again.index(), 0);
        let d = vfs.intern(&p("/d"));
        assert_eq!(d.index(), 3);
    }

    #[test]
    fn file_id_lookup_hits_and_misses() {
        let mut vfs = Vfs::default();
        let a = vfs.intern(&p("/src/a.inf"));
        assert_eq!(vfs.file_id(&p("/src/a.inf")), Some(a));
        assert_eq!(vfs.file_id(&p("/src/missing.inf")), None);
    }

    #[test]
    fn path_round_trips_from_id() {
        let mut vfs = Vfs::default();
        let a = vfs.intern(&p("/src/a.inf"));
        let b = vfs.intern(&p("/nested/dir/b.inf"));
        assert_eq!(vfs.path(a), p("/src/a.inf").as_path());
        assert_eq!(vfs.path(b), p("/nested/dir/b.inf").as_path());
    }

    #[test]
    fn paths_are_not_canonicalized() {
        let mut vfs = Vfs::default();
        let direct = vfs.intern(&p("/src/a.inf"));
        // `..` is never resolved (no fs::canonicalize), so this stays a
        // distinct file rather than collapsing onto "/src/a.inf".
        let indirect = vfs.intern(&p("/src/../src/a.inf"));
        assert_ne!(direct, indirect);
        assert_eq!(vfs.path(indirect), p("/src/../src/a.inf").as_path());
    }

    #[test]
    fn overlay_absent_until_set() {
        let mut vfs = Vfs::default();
        let a = vfs.intern(&p("/a.inf"));
        assert!(vfs.contents(a).is_none());
    }

    #[test]
    fn overlay_set_then_read() {
        let mut vfs = Vfs::default();
        let a = vfs.intern(&p("/a.inf"));
        vfs.set_contents(a, Arc::from("fn main() {}"));
        match vfs.contents(a) {
            Some(text) => assert_eq!(&*text, "fn main() {}"),
            None => panic!("overlay text should be present after set"),
        }
    }

    #[test]
    fn overlay_replace_overwrites() {
        let mut vfs = Vfs::default();
        let a = vfs.intern(&p("/a.inf"));
        vfs.set_contents(a, Arc::from("first"));
        vfs.set_contents(a, Arc::from("second"));
        match vfs.contents(a) {
            Some(text) => assert_eq!(&*text, "second"),
            None => panic!("overlay text should be present after replace"),
        }
    }

    #[test]
    fn overlay_remove_drops_document() {
        let mut vfs = Vfs::default();
        let a = vfs.intern(&p("/a.inf"));
        vfs.set_contents(a, Arc::from("body"));
        vfs.remove_contents(a);
        assert!(vfs.contents(a).is_none());
        // The path stays interned after the document is closed.
        assert_eq!(vfs.file_id(&p("/a.inf")), Some(a));
    }

    #[test]
    fn overlay_remove_is_a_noop_when_absent() {
        let mut vfs = Vfs::default();
        let a = vfs.intern(&p("/a.inf"));
        vfs.remove_contents(a);
        assert!(vfs.contents(a).is_none());
    }

    #[test]
    fn contents_of_path_reads_through_the_overlay() {
        let mut vfs = Vfs::default();
        let a = vfs.intern(&p("/a.inf"));
        vfs.set_contents(a, Arc::from("hello"));
        match vfs.contents_of_path(&p("/a.inf")) {
            Some(text) => assert_eq!(&*text, "hello"),
            None => panic!("overlay text should resolve through the path"),
        }
        // Interned but closed.
        vfs.remove_contents(a);
        assert!(vfs.contents_of_path(&p("/a.inf")).is_none());
        // Never interned.
        assert!(vfs.contents_of_path(&p("/never.inf")).is_none());
    }

    #[test]
    fn overlays_iterates_only_open_files() {
        let mut vfs = Vfs::default();
        let a = vfs.intern(&p("/a.inf"));
        let b = vfs.intern(&p("/b.inf"));
        let _c = vfs.intern(&p("/c.inf")); // interned but never opened
        vfs.set_contents(a, Arc::from("aaa"));
        vfs.set_contents(b, Arc::from("bbb"));

        let mut seen: Vec<(u32, String)> = vfs
            .overlays()
            .map(|(id, text)| (id.index(), text.to_owned()))
            .collect();
        seen.sort();
        assert_eq!(
            seen,
            vec![(a.index(), "aaa".to_owned()), (b.index(), "bbb".to_owned())]
        );
    }

    #[test]
    fn overlays_is_empty_before_any_open() {
        let mut vfs = Vfs::default();
        let _a = vfs.intern(&p("/a.inf"));
        assert_eq!(vfs.overlays().count(), 0);
    }
}
