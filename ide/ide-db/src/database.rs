//! [`RootDatabase`]: the open-document store plus memoized per-file analyses.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use inference_vfs::Vfs;
use rustc_hash::FxHashMap;

use crate::analysis::FileAnalysis;

/// Owns the editor's open-document overlay and the per-entry-file analyses
/// derived from it.
///
/// Each open file is analyzed as its own project entry. Analyses are computed
/// lazily on first request and memoized until a document change invalidates them.
///
/// # Per-entry source root
///
/// Path-form imports resolve relative to a project's single **source root**, not
/// to the importing file's own directory: a `use lib::b;` written anywhere in a
/// project resolves to `<src_root>/lib/b.inf`. Analyzing an opened file with its
/// own directory as the root therefore probes the wrong locations for any file in
/// a subdirectory, yielding false missing-import diagnostics on a program the
/// compiler accepts. Each entry's source root is resolved in three tiers so the
/// IDE agrees with the compiler (see issue #243):
///
/// 1. **Manifest walk-up** — the nearest ancestor `Inference.toml` names the
///    project; its source root (`<manifest_dir>/src`) is used when the opened file
///    lives under it. This is what makes the IDE and `infs` resolve identically.
/// 2. **Closure fallback** — with no manifest, if the file is already part of an
///    analyzed entry's import closure, that entry's source root is reused, so a
///    file navigated into from its project entry resolves the same way the entry
///    resolved it.
/// 3. **Own directory** — otherwise the file's own directory, the behavior for a
///    bare, project-less file.
///
/// A manifest created or edited *after* a file was opened is not observed until
/// that file's analysis is recomputed for another reason: there is no filesystem
/// watch in v1 (see the `inference::manifest` module).
///
/// # Closure-aware invalidation
///
/// A keystroke in one buffer must not force every other open buffer to
/// re-analyze. Each [`FileAnalysis`] records the absolute paths of every file in
/// its import closure, so a content change to path `P` invalidates only the
/// analyses whose closure contains `P`. The recorded closure is wider than the
/// files that loaded cleanly: it also holds the entry itself and any reachable
/// file that exists but could not be read (invalid UTF-8, a lock, a permission
/// error), so a later event that makes such a file readable still invalidates the
/// analyses computed without it.
///
/// One remaining case the closure cannot cover: opening a path that had **no
/// overlay content before** can satisfy an import that was *missing* — and a
/// missing import names no file on disk, so there is no path to record in any
/// closure. So an open that newly makes overlay content available additionally
/// invalidates every analysis that recorded an unresolved import. Keying this on
/// the overlay (not on whether the path was ever interned) is what makes a
/// `didClose` then `didOpen` re-fire: interning survives a close, but the overlay
/// does not. This is a deliberately coarse over-approximation — it may recompute
/// an analysis whose specific missing import is unrelated to the new file — but an
/// over-recompute only wastes work and never serves a stale result. Files that
/// appear on disk without being opened are not observed (there is no filesystem
/// watch in v1).
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
    ///
    /// The import closure resolves against the source root chosen by
    /// [`resolve_source_root`](Self::resolve_source_root), so a file in a
    /// subdirectory of a manifested project resolves its imports as the compiler
    /// would rather than against its own directory.
    pub fn analysis(&mut self, path: &Path) -> &FileAnalysis {
        if !self.analyses.contains_key(path) {
            let src_root = self.resolve_source_root(path);
            self.generation += 1;
            let analysis = FileAnalysis::compute(&self.vfs, path, &src_root, self.generation);
            self.analyses.insert(path.to_path_buf(), analysis);
        }
        &self.analyses[path]
    }

    /// Resolves the source root `entry`'s import closure should resolve against,
    /// in three tiers (see the type-level docs and issue #243).
    ///
    /// Manifest discovery (tier 1) reads the nearest `Inference.toml` from disk;
    /// the closure fallback (tier 2) only ever donates the root of an entry whose
    /// import closure already contains `entry`, so an unrelated open file never
    /// lends its root.
    fn resolve_source_root(&self, entry: &Path) -> PathBuf {
        if let Some(root) = inference::manifest_source_root(entry) {
            return root;
        }
        if let Some(root) = self.closure_donor_source_root(entry) {
            return root;
        }
        entry
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf()
    }

    /// The source root of an already-memoized entry whose import closure contains
    /// `file`, or `None` when no such entry exists.
    ///
    /// The donor's closure is exactly the set of files it reached from its own
    /// source root, so reusing that root resolves `file`'s imports the same way
    /// the donor resolved them. `file` itself is never its own donor. When several
    /// entries qualify the one with the lexicographically smallest entry path
    /// wins, so the choice is deterministic across repeated analyses.
    fn closure_donor_source_root(&self, file: &Path) -> Option<PathBuf> {
        self.analyses
            .iter()
            .filter(|(entry, analysis)| {
                entry.as_path() != file && analysis.closure_contains(file)
            })
            .min_by(|(a, _), (b, _)| a.cmp(b))
            .map(|(_, analysis)| analysis.source_root().to_path_buf())
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
