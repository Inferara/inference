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
/// # Query model: read-through-`&mut self`, single-threaded
///
/// Because a query computes and memoizes its analysis in place,
/// [`analysis`](Self::analysis) — a *read* — takes `&mut self`, and so does every
/// feature query layered on it. This is correct and sufficient for the LSP main
/// loop, which drives the database from a single thread. It is also a deliberate
/// constraint: reading through `&mut self` forecloses request cancellation and
/// parallel reads, which would require memoizing behind shared interior
/// mutability (a per-query cell, dependency tracking, and cancellation) rather
/// than a plain `&mut`-guarded map. Adopting Salsa (issue #157) is the planned
/// path to that model; until then callers must serialize access, and no query may
/// assume it can run concurrently with another.
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
/// # Sticky per-document source root
///
/// The tiers run only once per open document: a *definitive* root (a manifest or
/// a closure donor) is cached and reused on every later recompute, so a root
/// adopted from a donor survives that donor's eviction. Without the cache, a
/// change to a file shared by the donor entry and a non-entry file evicts both
/// analyses in one call; re-analyzing the non-entry file first would then find no
/// memoized donor and wrongly fall back to its own directory — the very tier-3
/// mistake #243 removes, reappearing intermittently after any shared-file change.
/// The own-directory fallback is *not* cached, so a file first resolved
/// provisionally can still be upgraded to a donor's root once a governing entry
/// is analyzed. `didClose` drops the cached root, so the next open re-resolves
/// from scratch.
///
/// A manifest created or edited *after* a file's root was cached is therefore not
/// observed until the document is closed and reopened: there is no filesystem
/// watch in v1 (see the `inference_project_model::manifest` module).
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
///
/// # Eviction
///
/// Open documents' analyses are never evicted — they are the editor's working
/// set and are invalidated (not dropped) as their closures change. Two other
/// sources of memoized analyses are bounded so a long session cannot grow the map
/// without limit:
///
/// * **Closing a document** removes its overlay, so its memoized entry analysis
///   (computed from that overlay) is dropped rather than left to serve vanished
///   buffer text; a later query recomputes it from disk. Closure-aware
///   invalidation already covers this — a document is always part of its own
///   closure — and any still-open dependent that imported the closed file
///   re-reads it from disk on its next query.
/// * **Feature requests on never-opened paths** (a hover or goto against a URI the
///   editor never sent a `didOpen` for reaches disk through the loader) each
///   memoize an entry that no document change ever invalidates. These are capped
///   at [`MAX_UNOPENED_ANALYSES`] with FIFO eviction of the oldest, so navigating
///   through an unbounded number of dependency files cannot grow the map forever.
#[derive(Default)]
pub struct RootDatabase {
    vfs: Vfs,
    analyses: FxHashMap<PathBuf, FileAnalysis>,
    /// Per-document sticky source root, keyed by entry path. Populated the first
    /// time an entry resolves to a *definitive* root (a manifest or a closure
    /// donor) and reused on every recompute until the document is closed, so an
    /// adopted donor root outlives that donor's eviction. The own-directory
    /// fallback is deliberately absent, keeping the upgrade path alive.
    source_roots: FxHashMap<PathBuf, PathBuf>,
    /// Monotonic source of per-analysis generation stamps.
    generation: u64,
    /// Entry paths of memoized analyses for documents the editor never opened, in
    /// the order they were memoized (oldest first). Bounds the map against feature
    /// requests on arbitrary URIs; see [`MAX_UNOPENED_ANALYSES`].
    unopened_order: Vec<PathBuf>,
}

/// The most memoized analyses to retain for documents that were never opened.
///
/// A feature request against a URI the editor never opened memoizes an analysis
/// that no document change invalidates, so without a bound they accumulate for the
/// life of the session. Eight is comfortably more than the handful of dependency
/// files a single navigation touches, while keeping the retained set small; the
/// eviction is FIFO over never-opened entries only, so open documents are never
/// affected.
const MAX_UNOPENED_ANALYSES: usize = 8;

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
        // An opened document is part of the editor's working set, never a
        // never-opened entry subject to the eviction cap.
        self.unopened_order.retain(|tracked| tracked != path);
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
    /// closure includes `path` recompute and read it from disk next time. The
    /// closed document's own entry analysis is dropped too — it was computed from
    /// the now-removed overlay, so serving it afterwards would return stale buffer
    /// text — and a later query recomputes it from disk. Closure-aware
    /// invalidation removes both, since a document is always part of its own
    /// closure; the explicit removal here states that intent directly and keeps
    /// the never-opened tracking consistent.
    pub fn close_document(&mut self, path: &Path) {
        if let Some(id) = self.vfs.file_id(path) {
            self.vfs.remove_contents(id);
        }
        // Drop the sticky source root so the next open re-resolves from scratch,
        // observing a manifest created or a governing entry opened meanwhile.
        self.source_roots.remove(path);
        self.analyses.remove(path);
        self.unopened_order.retain(|tracked| tracked != path);
        self.invalidate(path, false);
    }

    /// Whether an analysis for `path` is currently memoized.
    ///
    /// Used by the protocol layer to tell which open documents a change actually
    /// invalidated (their analyses were dropped) from those left untouched.
    #[must_use = "the analyzed state is the reason to call this"]
    pub fn is_analyzed(&self, path: &Path) -> bool {
        self.analyses.contains_key(path)
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
            if self.vfs.contents_of_path(path).is_none() {
                // Memoized for a path the editor never opened; bound how many such
                // entries accumulate over a session.
                self.record_unopened_analysis(path.to_path_buf());
            }
        }
        &self.analyses[path]
    }

    /// Records `path` as the most-recently memoized never-opened analysis and
    /// evicts the oldest ones beyond [`MAX_UNOPENED_ANALYSES`].
    ///
    /// The FIFO list is first pruned of paths that are no longer never-opened
    /// memoized entries (opened since, or dropped by invalidation), so the cap
    /// counts only entries actually held for never-opened documents.
    fn record_unopened_analysis(&mut self, path: PathBuf) {
        let mut kept = Vec::with_capacity(self.unopened_order.len() + 1);
        for tracked in std::mem::take(&mut self.unopened_order) {
            if tracked != path
                && self.analyses.contains_key(&tracked)
                && self.vfs.contents_of_path(&tracked).is_none()
            {
                kept.push(tracked);
            }
        }
        kept.push(path);
        self.unopened_order = kept;

        while self.unopened_order.len() > MAX_UNOPENED_ANALYSES {
            let evicted = self.unopened_order.remove(0);
            self.analyses.remove(&evicted);
        }
    }

    /// Resolves the source root `entry`'s import closure should resolve against,
    /// in three tiers (see the type-level docs and issue #243), caching a
    /// definitive result so it survives later invalidation.
    ///
    /// A cached root (from an earlier resolution of the same open document) wins
    /// outright. Otherwise manifest discovery (tier 1) reads the nearest
    /// `Inference.toml` from disk, and the closure fallback (tier 2) only ever
    /// donates the root of an entry whose import closure already contains `entry`,
    /// so an unrelated open file never lends its root. Both are *definitive* and
    /// are cached: reused on every recompute and dropped only on `didClose`.
    /// Caching the donor root is the point — it lets an adopted root outlive the
    /// donor's eviction, which a change to a file shared by the donor and this
    /// file would otherwise lose (re-resolving would find no donor and wrongly
    /// fall to tier 3). The own-directory fallback (tier 3) is *not* cached, so a
    /// file resolved provisionally can still be upgraded once a governing entry is
    /// analyzed.
    fn resolve_source_root(&mut self, entry: &Path) -> PathBuf {
        if let Some(root) = self.source_roots.get(entry) {
            return root.clone();
        }
        if let Some(root) = inference_project_model::manifest_source_root(entry) {
            self.source_roots.insert(entry.to_path_buf(), root.clone());
            return root;
        }
        if let Some(root) = self.closure_donor_source_root(entry) {
            self.source_roots.insert(entry.to_path_buf(), root.clone());
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
