//! [`RootDatabase`]: the open-document store plus memoized per-file analyses.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use inference_vfs::Vfs;
use rustc_hash::FxHashMap;

use crate::analysis::FileAnalysis;

/// Owns the editor's open-document overlay and the per-entry-file analyses
/// derived from it.
///
/// Each open file is analyzed as its own project entry (its directory is the
/// source root). Analyses are computed lazily on first request and memoized
/// until a document change invalidates them.
///
/// # Closure-aware invalidation
///
/// A keystroke in one buffer must not force every other open buffer to
/// re-analyze. Each [`FileAnalysis`] records the absolute paths of every file in
/// its import closure, so a content change to path `P` invalidates only the
/// analyses whose closure contains `P`.
///
/// One extra case: opening a path that had **no overlay content before** can
/// satisfy an import that was missing, but a missing import is not in any closure
/// (there is no file to record). So an open that newly makes overlay content
/// available additionally invalidates every analysis that recorded an unresolved
/// import. Keying this on the overlay (not on whether the path was ever interned)
/// is what makes a `didClose` then `didOpen` re-fire: interning survives a close,
/// but the overlay does not. This is a deliberately coarse over-approximation —
/// it may recompute an analysis whose specific missing import is unrelated to the
/// new file — chosen because it is simple and always correct. Files that appear
/// on disk without being opened are not observed (there is no filesystem watch in
/// v1).
///
/// # Path identity
///
/// Analyses and the overlay are keyed by exact path spelling; a caller that may
/// refer to one file by two spellings must canonicalize before calling in, so
/// the same file always arrives under one path.
#[derive(Default)]
pub struct RootDatabase {
    vfs: Vfs,
    analyses: FxHashMap<PathBuf, FileAnalysis>,
    /// Monotonic source of per-analysis generation stamps.
    generation: u64,
}

impl RootDatabase {
    /// Opens `path` with `text` as its in-memory contents (an editor `didOpen`).
    ///
    /// Interns the path if new and installs its overlay text, then invalidates
    /// dependent analyses.
    pub fn open_document(&mut self, path: &Path, text: impl Into<Arc<str>>) {
        // The missing-import widening must fire whenever this open makes content
        // available where there was none — not only on a path's very first open.
        // Interning survives `didClose` (the `Vfs` never drops ids), so keying on
        // "never interned" would miss a close/reopen cycle: reopening a file that
        // satisfies a previously-missing import would leave the importing analysis
        // stale. Keying on "had no overlay before this open" re-fires correctly and
        // still subsumes the truly-first open.
        let newly_available = self.vfs.contents_of_path(path).is_none();
        let id = self.vfs.intern(path);
        self.vfs.set_contents(id, text.into());
        self.invalidate(path, newly_available);
    }

    /// Replaces the in-memory contents of an open `path` (an editor `didChange`).
    ///
    /// A change never introduces a previously-unseen file, so only closures that
    /// contain `path` are invalidated.
    pub fn change_document(&mut self, path: &Path, text: impl Into<Arc<str>>) {
        let id = self.vfs.intern(path);
        self.vfs.set_contents(id, text.into());
        self.invalidate(path, false);
    }

    /// Drops the in-memory contents of `path` (an editor `didClose`).
    ///
    /// The path stays interned; only its overlay is removed, so analyses whose
    /// closure includes `path` recompute and read it from disk next time.
    pub fn close_document(&mut self, path: &Path) {
        if let Some(id) = self.vfs.file_id(path) {
            self.vfs.remove_contents(id);
        }
        self.invalidate(path, false);
    }

    /// The analysis of `path` treated as a project entry, computed on first
    /// request and memoized until invalidated.
    pub fn analysis(&mut self, path: &Path) -> &FileAnalysis {
        if !self.analyses.contains_key(path) {
            self.generation += 1;
            let analysis = FileAnalysis::compute(&self.vfs, path, self.generation);
            self.analyses.insert(path.to_path_buf(), analysis);
        }
        &self.analyses[path]
    }

    /// Drops every memoized analysis affected by a change to `changed`.
    ///
    /// `newly_available` is true when this event made overlay content available
    /// for `changed` where there was none before (a first `didOpen`, or a reopen
    /// after a `didClose`); see the type-level docs for why that widens
    /// invalidation to analyses with an unresolved import.
    fn invalidate(&mut self, changed: &Path, newly_available: bool) {
        self.analyses.retain(|_entry, analysis| {
            let closure_touched = analysis.closure_contains(changed);
            let may_resolve_missing = newly_available && analysis.had_missing_import();
            !(closure_touched || may_resolve_missing)
        });
    }
}
