//! [`RootDatabase`]: the open-document store plus per-file analyses memoized by
//! Salsa.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use inference_vfs::Vfs;
use rustc_hash::FxHashMap;
use salsa::{Database, Setter, Storage};

use crate::analysis::FileAnalysis;

/// One project entry's Salsa input: its identity plus the lever that forces a
/// recompute Salsa's own dependency tracking cannot.
///
/// `path` and `src_root` are the compute's real inputs — a query reading them
/// depends on them the ordinary way. `revision` is the **eviction** lever only:
/// bumping it forces a recompute for the never-opened cap, which has no file event
/// and so no change stamp to bump (see [`RootDatabase::evict_analysis`]). Ordinary
/// content changes no longer flow through `revision`; they are carried by the
/// per-file change stamps and the availability epoch below, which the query reads
/// once its import closure is known. The query body reads all three, so a change
/// to any of them invalidates its memo.
#[salsa::input]
struct EntryInput {
    #[returns(ref)]
    path: PathBuf,
    #[returns(ref)]
    src_root: PathBuf,
    revision: u64,
}

/// One reachable file's change stamp: an opaque, monotonic counter standing in for
/// a content change Salsa cannot observe on its own.
///
/// The analysis query reads the stamp of every file in its import closure, so
/// bumping one path's stamp forces exactly the memos whose closure contains that
/// path to recompute. Content identity stays with the `Vfs` — the loader seam the
/// compiler and IDE share never names Salsa (#157) — so this input carries only
/// the "something under this path changed" signal, not the bytes.
#[salsa::input]
struct FileStamp {
    stamp: u64,
}

/// A single input bumped when an open makes overlay content available where there
/// was none before.
///
/// The analysis query reads it **only** when its parse recorded an unresolved
/// import, so exactly the memos a newly-available file might fix carry the edge. A
/// missing import names no file on disk, so there is nothing to record in any
/// closure; this singleton is the one edge that can re-fire such an analysis when
/// an unrelated `didOpen` may have supplied the file (a deliberately coarse
/// over-approximation — see the type-level docs). Both the query's conditional read
/// and the write-path bump reach it through
/// [`IdeDatabase::availability_epoch`](IdeDatabase::availability_epoch), the sole
/// creation funnel, because a second `new` panics.
#[salsa::input(singleton)]
struct AvailabilityEpoch {
    epoch: u64,
}

/// The database view the analysis query runs against.
///
/// [`vfs`](Self::vfs) and [`next_generation`](Self::next_generation) reach state
/// kept **outside** Salsa storage. The `Vfs` overlay is read directly by the
/// query's overlay-then-disk loader, so it must not live in Salsa — that keeps the
/// single import-resolution seam the compiler and IDE share Salsa-free (#157). The
/// generation counter is read inside the query body so a recompute mints a fresh
/// value while a memo hit (whose body never runs) returns the previously stamped
/// one, which is how [`FileAnalysis::generation`] stays an observable recompute
/// probe.
///
/// [`file_stamp`](Self::file_stamp) and
/// [`availability_epoch`](Self::availability_epoch) instead reach *into* Salsa
/// storage: each returns the Salsa input the query reads to register a
/// content-change dependency the `Vfs` seam otherwise hides from Salsa. Both are
/// get-or-create funnels callable through the shared `&self` the query holds, so
/// the query can register a dependency on a file's stamp the first time it is
/// analyzed and the write path can later bump that same input.
#[salsa::db]
trait IdeDatabase: salsa::Database {
    fn vfs(&self) -> &Vfs;
    fn next_generation(&self) -> u64;
    /// Get-or-create the change-stamp input for `path`.
    ///
    /// The registry this reads dedups because each `FileStamp::new` mints a fresh
    /// input; this is the only funnel through which stamps are created, so the
    /// write-path bump and the in-query dependency registration observe the same
    /// input for one path. A lookup-only variant would let a change register an
    /// edge on an input the bump never touched, silently under-invalidating.
    fn file_stamp(&self, path: &Path) -> FileStamp;
    /// Get-or-create the availability-epoch singleton, the sole creation site.
    ///
    /// A second `AvailabilityEpoch::new` panics ("singleton struct may not be
    /// duplicated"), so both the query's conditional read and the write-path bump
    /// route through here.
    fn availability_epoch(&self) -> AvailabilityEpoch;
}

/// A memoized [`FileAnalysis`] wrapped so Salsa can store it as a query result.
///
/// A tracked function's output must implement [`salsa::Update`], whose blanket
/// impls recurse structurally into a value's fields. `FileAnalysis` wraps the type
/// checker's arena and symbol table, none of which implement `Update`, so no
/// structural impl exists and `Arc<FileAnalysis>` cannot be returned directly.
/// The impl below exists **only** to satisfy that static bound: Salsa never calls
/// `maybe_update` on a tracked function's output — it replaces the memo wholesale
/// and decides backdating purely by comparing values (see `no_eq` on the query).
#[derive(Clone)]
struct AnalysisResult(Arc<FileAnalysis>);

// SAFETY: `maybe_update` overwrites the owned value at `old_pointer` with
// `new_value` and reports a change, exactly as salsa's own `always_update` helper
// does. The trait contract guarantees `old_pointer` references an initialized,
// owned `AnalysisResult`, so the assignment (which drops the old value) is sound.
// No borrowed data is inspected, so the reference-invalidation hazard the trait
// documents cannot arise.
unsafe impl salsa::Update for AnalysisResult {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        unsafe { *old_pointer = new_value };
        true
    }
}

/// The whole per-entry analysis, memoized by Salsa.
///
/// [`FileAnalysis::compute`] is called unchanged; Salsa supplies memoization only.
/// Reading `revision` keeps the eviction lever's edge live. The content-change
/// dependencies are registered *after* the compute, once the import closure is
/// known: one change-stamp edge per closure file, plus the availability epoch when
/// an import went unresolved (see the loop below). The generation is taken from the
/// database counter so it advances only when this body actually runs.
///
/// `no_eq`: a `FileAnalysis` has no meaningful equality and is not `PartialEq`.
/// The option removes that requirement and disables backdating — the result is
/// always treated as changed — which costs nothing while no other query depends
/// on this one.
#[salsa::tracked(no_eq)]
fn analyze_entry(db: &dyn IdeDatabase, entry: EntryInput) -> AnalysisResult {
    let _ = entry.revision(db);
    let path = entry.path(db);
    let src_root = entry.src_root(db);
    #[cfg(debug_assertions)]
    test_seams::slow_analysis_seam(db, path);
    let generation = db.next_generation();
    // The database hands the compute a hook that unwinds if a cancellation
    // request has landed, so a long analysis is interruptible at stage
    // boundaries rather than only at the fetch entry.
    let checkpoint = || db.unwind_if_revision_cancelled();
    let analysis = FileAnalysis::compute(db.vfs(), path, src_root, generation, &checkpoint);

    // Register this compute's content-change dependencies now that the closure is
    // known. Reading each closure file's stamp records the per-file input edge that
    // makes a later `change_document`/`didClose` of that file recompute this memo;
    // the availability epoch is read only when an import went unresolved, so exactly
    // the memos a newly-available file might fix carry that edge.
    //
    // Registering AFTER the compute is sound: Salsa records a query's input edges in
    // execution order, and order affects only verification short-circuiting, not
    // which edges exist. No revision can advance mid-query, because every setter
    // needs `&mut db` and this query runs on the sole shared handle.
    //
    // The epoch read MUST stay conditional. An unconditional read would make every
    // missing-import-free memo depend on the epoch, so every newly-available open
    // would stale every memo — breaking the unrelated-change equality pins
    // (`change_to_unrelated_file_does_not_invalidate_entry_analysis`,
    // `opening_an_unrelated_file_does_not_invalidate_an_unreadable_import_analysis`).
    //
    // This registration and `RootDatabase::note_stale_entries` read the same two
    // `FileAnalysis` fields — the closure paths (here via `closure_paths_ordered`,
    // there via `closure_contains`) and `had_missing_import` — so the
    // recompute-forcing edges and the write-path mirror clear stay in lockstep.
    for file in analysis.closure_paths_ordered() {
        let _ = db.file_stamp(file).stamp(db);
    }
    if analysis.had_missing_import() {
        let _ = db.availability_epoch().epoch(db);
    }
    AnalysisResult(Arc::new(analysis))
}

/// Owns the editor's open-document overlay and the per-entry-file analyses
/// derived from it.
///
/// Each open file is analyzed as its own project entry. Analyses are computed
/// lazily on first request and memoized by Salsa until a document change forces a
/// recompute.
///
/// # Memoization and the query surface
///
/// Per-entry analyses are memoized by Salsa: [`analysis`](Self::analysis) drives a
/// single tracked query whose body is the whole `FileAnalysis` compute, so a
/// repeated request returns the framework's memo rather than recomputing. The
/// query surface still takes `&mut self` — a read memoizes in place, and the LSP
/// main loop drives it from one thread — because the shared read-handle model is
/// later work (#157). Cancellation, though, now unwinds out of
/// [`analysis`](Self::analysis) at the `analyze_entry` call: the pre-query
/// side-table writes (sticky-root insert, entry insert with `analysis: None`,
/// src-root drift bump) are idempotent setup that re-converges on retry, and
/// result writes happen only after the query returns, so a cancelled compute
/// leaves the entry in the consistent invalidated shape. Salsa cannot observe the
/// file reads
/// themselves: the import closure is read through the `Vfs` overlay-then-disk
/// loader, which stays outside Salsa storage so the compiler and IDE resolve
/// imports through one seam. What Salsa *can* see is supplied for it: the query
/// registers a per-file change-stamp edge for every file in the closure it just
/// read, plus an availability-epoch edge when an import went unresolved, and the
/// write path bumps those inputs (see below) so the seam-hidden change forces a
/// recompute. The write path additionally keeps an eager mirror — the entry's
/// latest analysis, cleared the instant it goes stale — for the editor-facing
/// bookkeeping (`is_analyzed`, the republish sweep, the donor search, the cap)
/// that must answer before any query runs.
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
/// its import closure, and the analysis query registers a Salsa change-stamp edge
/// for each of them, so a content change to path `P` recomputes only the analyses
/// whose closure contains `P` — the write path bumps `P`'s stamp
/// ([`bump_file_stamp`](Self::bump_file_stamp)) and Salsa re-runs exactly those
/// memos. The recorded closure is wider than the files that loaded cleanly: it
/// also holds the entry itself and any reachable file that exists but could not be
/// read (invalid UTF-8, a lock, a permission error), so a later event that makes
/// such a file readable still recomputes the analyses computed without it.
///
/// One remaining case a per-file stamp cannot cover: opening a path that had **no
/// overlay content before** can satisfy an import that was *missing* — and a
/// missing import names no file on disk, so there is no path to record in any
/// closure and no stamp to bump. So the query registers one more edge — the
/// availability epoch — whenever it recorded an unresolved import, and an open that
/// newly makes overlay content available bumps that epoch
/// ([`bump_availability_epoch`](Self::bump_availability_epoch)), recomputing every
/// analysis that had a missing import. Keying this on the overlay (not on whether
/// the path was ever interned) is what makes a `didClose` then `didOpen` re-fire:
/// interning survives a close, but the overlay does not. This is a deliberately
/// coarse over-approximation — it may recompute an analysis whose specific missing
/// import is unrelated to the new file — but an over-recompute only wastes work and
/// never serves a stale result. Files that appear on disk without being opened are
/// not observed (there is no filesystem watch in v1).
///
/// Alongside those edges the write path keeps an eager **mirror**
/// ([`note_stale_entries`](Self::note_stale_entries)): it clears the same set's
/// cached `analysis` to `None` the moment a change lands, so the protocol layer's
/// republish sweep and [`is_analyzed`](Self::is_analyzed) can tell which open
/// documents a change invalidated *before* any query re-runs. The mirror forces no
/// recompute — that is the edges' job — and its predicate reads the very
/// `closure_paths`/`had_missing_import` fields the query registered its edges from,
/// so the two never disagree (a debug assertion in [`analysis`](Self::analysis)
/// machine-checks it).
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
/// sources of memoized analyses are bounded so a long session cannot grow the
/// tracked set without limit:
///
/// * **Closing a document** removes its overlay, so its analysis (computed from
///   that overlay) is dropped rather than left to serve vanished buffer text; a
///   later query recomputes it from disk. Closure-aware invalidation already
///   covers this — a document is always part of its own closure — and any
///   still-open dependent that imported the closed file re-reads it from disk on
///   its next query.
/// * **Feature requests on never-opened paths** (a hover or goto against a URI the
///   editor never sent a `didOpen` for reaches disk through the loader) each
///   memoize an entry that no document change ever invalidates. These are capped
///   at [`MAX_UNOPENED_ANALYSES`] with FIFO eviction of the oldest.
///
/// Salsa 0.27 exposes no per-memo eviction, so an evicted entry's compute lingers
/// in Salsa storage until its next recompute; what the cap preserves is the
/// *observable* behavior — an evicted analysis recomputes with a fresh generation
/// stamp on the next request, while a retained one stays a cache hit. Durability
/// tiers and a real parse LRU are later work (#157).
#[salsa::db]
#[derive(Default)]
pub struct RootDatabase {
    storage: Storage<Self>,
    /// Editor overlay, held outside Salsa storage: the analysis query reads it
    /// through the overlay-then-disk loader, the seam the compiler and IDE share.
    vfs: Vfs,
    /// Monotonic source of per-analysis generation stamps, read inside the query
    /// body (see [`IdeDatabase::next_generation`]). Outside Salsa storage because
    /// it is a side effect of running the compute, not an input to it. Atomic
    /// despite the single-threaded loop because the query runs against a shared
    /// `&dyn IdeDatabase`, so the counter can only be advanced through `&self`.
    generation_counter: AtomicU64,
    /// Monotonic source of per-entry `revision` values. Bumping an entry's input
    /// to a fresh value forces Salsa to recompute it; used now only by the eviction
    /// lever (see [`evict_analysis`](Self::evict_analysis)).
    revision_source: u64,
    /// Monotonic source of [`FileStamp`]/[`AvailabilityEpoch`] values, a twin of
    /// `revision_source` kept on a separate counter so the revision machinery (the
    /// eviction lever, retained for #157) can be retired without disturbing the
    /// stamp source. Bumps are set-only: an input field is written through its
    /// setter, never read-modify-written, since reading an input field outside a
    /// query registers no edge and would only mislead.
    stamp_source: u64,
    /// Append-only path → [`FileStamp`] registry, shared by the write-path bump and
    /// the analysis query's in-query dependency registration so both observe one
    /// input per path. A `Mutex` only for interior mutability through the shared
    /// `&dyn IdeDatabase` the query holds — the worker loop is single-threaded.
    /// Salsa inputs are never destroyed, so entries are permanent for the session,
    /// bounded by the set of files touched.
    file_stamps: Mutex<FxHashMap<PathBuf, FileStamp>>,
    /// Per-entry bookkeeping `RootDatabase` owns rather than Salsa: the reusable
    /// input handle plus the latest analysis, whose closure metadata drives the
    /// write-path staleness mirror, the closure-donor search,
    /// [`is_analyzed`](Self::is_analyzed), and the never-opened cap. `analysis` is
    /// `None` between a staleness mark and the next recompute, mirroring an absent
    /// entry in the pre-Salsa memo map.
    entries: FxHashMap<PathBuf, EntryState>,
    /// Per-document sticky source root, keyed by entry path. Populated the first
    /// time an entry resolves to a *definitive* root (a manifest or a closure
    /// donor) and reused on every recompute until the document is closed, so an
    /// adopted donor root outlives that donor's eviction. The own-directory
    /// fallback is deliberately absent, keeping the upgrade path alive.
    source_roots: FxHashMap<PathBuf, PathBuf>,
    /// Entry paths of memoized analyses for documents the editor never opened, in
    /// the order they were memoized (oldest first). Bounds the tracked set against
    /// feature requests on arbitrary URIs; see [`MAX_UNOPENED_ANALYSES`].
    unopened_order: VecDeque<PathBuf>,
}

#[salsa::db]
impl salsa::Database for RootDatabase {}

#[salsa::db]
impl IdeDatabase for RootDatabase {
    fn vfs(&self) -> &Vfs {
        &self.vfs
    }

    fn next_generation(&self) -> u64 {
        // Purely observational, so relaxed ordering suffices; the loop is
        // single-threaded regardless.
        self.generation_counter.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn file_stamp(&self, path: &Path) -> FileStamp {
        // Get-or-create so the write path and the in-query registration share one
        // input per path. The registry guard is released when this method returns —
        // before any caller reaches a setter, which needs `&mut self`.
        let mut registry = self.file_stamps.lock().expect("stamp registry poisoned");
        *registry
            .entry(path.to_path_buf())
            .or_insert_with(|| FileStamp::new(self, 0))
    }

    fn availability_epoch(&self) -> AvailabilityEpoch {
        AvailabilityEpoch::try_get(self).unwrap_or_else(|| AvailabilityEpoch::new(self, 0))
    }
}

/// The reusable Salsa input handle for one entry and its latest analysis.
struct EntryState {
    input: EntryInput,
    /// The latest computed analysis, or `None` once it went stale — cleared by the
    /// write-path mirror ([`note_stale_entries`](RootDatabase::note_stale_entries))
    /// when a change lands, or by [`evict_analysis`](RootDatabase::evict_analysis)
    /// for the cap. The stamp/epoch edges the query registered are what actually
    /// force the recompute; this field is the eager mirror of that staleness.
    analysis: Option<Arc<FileAnalysis>>,
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
    /// Interns the path if new and installs its overlay text, then bumps the
    /// change-stamp (and, when newly available, the availability epoch) that
    /// recomputes dependent analyses and marks them stale in the mirror.
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
        // Every overlay mutation is paired with a stamp bump for the same path in
        // the same turn: the stamp is the Salsa-visible proxy for a content change
        // the loader seam otherwise hides. A newly-available open additionally bumps
        // the availability epoch — the only edge that can re-fire a missing-import
        // memo — and widens the mirror sweep the same way.
        self.bump_file_stamp(path);
        if newly_available {
            self.bump_availability_epoch();
        }
        self.note_stale_entries(path, newly_available);
    }

    /// Replaces the in-memory contents of an open `path` (an editor `didChange`).
    ///
    /// A change never introduces a previously-unseen file, so only closures that
    /// contain `path` are recomputed and marked stale.
    pub fn change_document(&mut self, path: &Path, text: impl Into<Arc<str>>) {
        let id = self.vfs.intern(path);
        self.vfs.set_contents(id, text.into());
        // The overlay mutation's paired stamp bump (see `open_document`); a change
        // never makes a previously-absent file available, so no epoch bump.
        self.bump_file_stamp(path);
        self.note_stale_entries(path, false);
    }

    /// Drops the in-memory contents of `path` (an editor `didClose`).
    ///
    /// The path stays interned; only its overlay is removed, so analyses whose
    /// closure includes `path` recompute and read it from disk next time. The
    /// closed document's own entry analysis stops being served too — it was
    /// computed from the now-removed overlay, so answering from it afterwards
    /// would return stale buffer text — and a later query recomputes it from disk.
    /// Removing the overlay is a content change like any other: the paired stamp
    /// bump forces every memo whose closure contains `path` — the closed entry among
    /// them, since a document is always in its own closure — to recompute, and
    /// `note_stale_entries` clears the same set's mirror.
    ///
    /// What is actually reclaimed is the overlay text. The entry's `EntryState`
    /// and Salsa's memo of the superseded analysis are retained until the next
    /// recompute replaces the memo — Salsa 0.27 offers no per-memo eviction, so
    /// "dropped" here means "no longer served", not "freed" (#157).
    pub fn close_document(&mut self, path: &Path) {
        if let Some(id) = self.vfs.file_id(path) {
            self.vfs.remove_contents(id);
        }
        // Drop the sticky source root so the next open re-resolves from scratch,
        // observing a manifest created or a governing entry opened meanwhile.
        self.source_roots.remove(path);
        self.bump_file_stamp(path);
        self.unopened_order.retain(|tracked| tracked != path);
        self.note_stale_entries(path, false);
    }

    /// Whether an analysis for `path` is currently memoized (computed and not
    /// invalidated).
    ///
    /// Used by the protocol layer to tell which open documents a change actually
    /// invalidated (their analyses were dropped) from those left untouched.
    #[must_use = "the analyzed state is the reason to call this"]
    pub fn is_analyzed(&self, path: &Path) -> bool {
        self.entries
            .get(path)
            .is_some_and(|state| state.analysis.is_some())
    }

    /// Binds `source` to this database handle so a cancellation request
    /// interrupts this handle's in-flight analysis at its next checkpoint.
    ///
    /// The binding is per-handle: a fresh handle mints a fresh token, so a caller
    /// that replaces its handle must rebind.
    pub fn bind_cancellation(&self, source: &crate::cancellation::AnalysisCancelSource) {
        source.bind(Database::cancellation_token(self));
    }

    /// The analysis of `path` treated as a project entry, computed on first
    /// request and memoized by Salsa until invalidated.
    ///
    /// The import closure resolves against the source root chosen by
    /// [`resolve_source_root`](Self::resolve_source_root), so a file in a
    /// subdirectory of a manifested project resolves its imports as the compiler
    /// would rather than against its own directory. That resolution runs only when
    /// the analysis must be recomputed, so a memoized answer costs no filesystem
    /// work: re-resolving per request would walk ancestors looking for an
    /// `Inference.toml` and rescan every analysis for a closure donor on every
    /// keystroke, since the own-directory fallback is deliberately never cached.
    /// Skipping it on a hit also pins a memoized analysis to the root it was
    /// computed against instead of silently re-resolving it under a donor that has
    /// appeared since.
    ///
    /// # Panics
    ///
    /// Panics if `path` has no entry, or its entry holds no analysis, at either
    /// indexing site. Both are unreachable: on the recompute path
    /// [`sync_entry_input`](Self::sync_entry_input) inserts the entry before the
    /// query runs, on the memo-hit path the entry is present by definition of
    /// [`is_analyzed`](Self::is_analyzed), and the analysis is stored just above
    /// the final read.
    pub fn analysis(&mut self, path: &Path) -> &FileAnalysis {
        let recomputed = !self.is_analyzed(path);
        if recomputed {
            let src_root = self.resolve_source_root(path);
            self.sync_entry_input(path, &src_root);
        }
        // Debug-only alignment tripwire: on a mirror hit (the entry still holds an
        // analysis, so `recomputed` is false) the fetch below must be a Salsa memo
        // hit, so its generation must equal the mirror's. A mismatch means a stamp or
        // epoch edge fired while the write path left the mirror intact — the edges
        // and `note_stale_entries` have drifted apart. Captured before the fetch so
        // the mirror value is read while it is still the previous one.
        #[cfg(debug_assertions)]
        let prior_generation = (!recomputed).then(|| {
            self.entries[path]
                .analysis
                .as_ref()
                .expect("a mirror hit holds an analysis")
                .generation()
        });
        let input = self.entries[path].input;
        // Salsa returns the memo when the entry's inputs are unchanged and reruns
        // `analyze_entry` (minting a fresh generation) when a change-stamp, epoch, or
        // src-root/revision bump marked it stale.
        let AnalysisResult(analysis) = analyze_entry(&*self, input);
        #[cfg(debug_assertions)]
        if let Some(previous) = prior_generation {
            debug_assert_eq!(
                previous,
                analysis.generation(),
                "a fetch behind a memoized mirror entry must be a Salsa memo hit; a \
                 recompute here means an input changed without the write path clearing \
                 the mirror — the stamp/epoch bumps and the stale-entry pass have \
                 drifted apart"
            );
        }

        let is_unopened = self.vfs.contents_of_path(path).is_none();
        self.entries
            .get_mut(path)
            .expect("the entry is present on both the hit and the recompute path")
            .analysis = Some(analysis);
        if recomputed && is_unopened {
            // Memoized for a path the editor never opened; bound how many such
            // entries accumulate over a session.
            self.record_unopened_analysis(path.to_path_buf());
        }
        self.entries[path]
            .analysis
            .as_deref()
            .expect("the analysis was stored above")
    }

    /// Ensures an [`EntryInput`] exists for `path` carrying `src_root`.
    ///
    /// A new entry starts at revision zero with no analysis. An existing entry's
    /// source root is updated only when it drifted (a close/reopen re-resolution),
    /// which itself marks the query stale so the recompute uses the new root.
    fn sync_entry_input(&mut self, path: &Path, src_root: &Path) {
        if let Some(input) = self.entries.get(path).map(|state| state.input) {
            if input.src_root(&*self).as_path() != src_root {
                input.set_src_root(self).to(src_root.to_path_buf());
            }
        } else {
            let input = EntryInput::new(&*self, path.to_path_buf(), src_root.to_path_buf(), 0);
            self.entries.insert(
                path.to_path_buf(),
                EntryState {
                    input,
                    analysis: None,
                },
            );
        }
    }

    /// The eviction lever: drops `entry`'s mirror analysis and bumps its `revision`
    /// input so the next query recomputes it.
    ///
    /// Unlike a content change, an eviction has no file event and so no stamp to
    /// bump; the `revision` input is the only thing that can force the recompute.
    /// Folding eviction into the stamps would let a stale memo revalidate and break
    /// the never-opened cap's recompute guarantee, so it stays a distinct lever. The
    /// sole caller is [`record_unopened_analysis`](Self::record_unopened_analysis);
    /// this and the `revision` machinery are retired together with the cap (#157).
    ///
    /// A no-op for a path that was never analyzed.
    fn evict_analysis(&mut self, entry: &Path) {
        let Some(input) = self.entries.get_mut(entry).map(|state| {
            state.analysis = None;
            state.input
        }) else {
            return;
        };
        let next = self.next_revision();
        input.set_revision(self).to(next);
    }

    /// A fresh, strictly increasing `revision` value, distinct from any previously
    /// assigned, so setting it on an input always registers as a change.
    fn next_revision(&mut self) -> u64 {
        self.revision_source += 1;
        self.revision_source
    }

    /// A fresh, strictly increasing [`FileStamp`]/[`AvailabilityEpoch`] value,
    /// distinct from any previously assigned, so setting it on an input always
    /// registers as a change. A twin of [`next_revision`](Self::next_revision) on a
    /// separate counter, so retiring the revision machinery (#157) leaves the stamp
    /// source intact.
    fn next_stamp(&mut self) -> u64 {
        self.stamp_source += 1;
        self.stamp_source
    }

    /// Bumps `path`'s change stamp so every memo whose closure contains it
    /// recomputes.
    ///
    /// Get-or-create is load-bearing: an ide-db-level `change_document` can precede
    /// any analysis of `path`, and a never-opened shared import's stamp may have
    /// been minted inside a query, so a lookup-only variant would silently
    /// under-invalidate. Every bump runs through a setter, so it takes the write
    /// lock (Salsa's `cancel_others` waits for outstanding read handles — trivially
    /// immediate on the sole worker handle); the registry guard from
    /// [`file_stamp`](Self::file_stamp) is dropped before the setter runs.
    fn bump_file_stamp(&mut self, path: &Path) {
        let stamp = self.file_stamp(path);
        let next = self.next_stamp();
        stamp.set_stamp(self).to(next);
    }

    /// Bumps the availability epoch so every memo that recorded an unresolved import
    /// — the only memos that read this input — recomputes. Routed through the
    /// [`availability_epoch`](Self::availability_epoch) funnel so the singleton is
    /// created at most once.
    fn bump_availability_epoch(&mut self) {
        let epoch = self.availability_epoch();
        let next = self.next_stamp();
        epoch.set_epoch(self).to(next);
    }

    /// Records `path` as the most-recently memoized never-opened analysis and
    /// evicts the oldest ones beyond [`MAX_UNOPENED_ANALYSES`].
    ///
    /// The FIFO list is first pruned of paths that are no longer never-opened
    /// memoized entries (opened since, or marked stale by a change), so the cap
    /// counts only entries actually held for never-opened documents. That prune
    /// reads [`is_analyzed`](Self::is_analyzed) as "still memoized" — the same
    /// meaning [`evict_analysis`](Self::evict_analysis) relies on — so the two must
    /// not drift before the cap is retired (#157).
    fn record_unopened_analysis(&mut self, path: PathBuf) {
        let mut kept = VecDeque::with_capacity(self.unopened_order.len() + 1);
        for tracked in std::mem::take(&mut self.unopened_order) {
            if tracked != path
                && self.is_analyzed(&tracked)
                && self.vfs.contents_of_path(&tracked).is_none()
            {
                kept.push_back(tracked);
            }
        }
        kept.push_back(path);
        self.unopened_order = kept;

        while self.unopened_order.len() > MAX_UNOPENED_ANALYSES {
            if let Some(evicted) = self.unopened_order.pop_front() {
                self.evict_analysis(&evicted);
            }
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
        self.entries
            .iter()
            .filter_map(|(entry, state)| Some((entry, state.analysis.as_ref()?)))
            .filter(|(entry, analysis)| entry.as_path() != file && analysis.closure_contains(file))
            .min_by(|(a, _), (b, _)| a.cmp(b))
            .map(|(_, analysis)| analysis.source_root().to_path_buf())
    }

    /// Marks every memoized analysis a change to `changed` made stale by clearing
    /// its mirror `analysis` to `None`.
    ///
    /// Write-time selectivity bookkeeping ONLY — it performs no Salsa write. Forcing
    /// the recompute lives entirely in the change-stamp and epoch edges the query
    /// registered: the caller's [`bump_file_stamp`](Self::bump_file_stamp) stales
    /// every memo whose closure contains `changed`, and a `newly_available` open's
    /// [`bump_availability_epoch`](Self::bump_availability_epoch) stales every memo
    /// that recorded a missing import. What the mirror buys is *write-time*
    /// observability the edges cannot give: [`is_analyzed`](Self::is_analyzed), the
    /// protocol layer's republish sweep, the closure-donor search, and the
    /// never-opened cap all read it before any query re-runs.
    ///
    /// The predicate reads the same two `FileAnalysis` fields the query registered
    /// its edges from — `closure_paths` via
    /// [`closure_contains`](FileAnalysis::closure_contains) and `had_missing_import`
    /// — so the mirror and the edges stay in lockstep. Under-clearing here surfaces
    /// as the alignment `debug_assert` in [`analysis`](Self::analysis); over-clearing
    /// only as a spurious republish.
    ///
    /// `newly_available` is true when this event made overlay content available for
    /// `changed` where there was none before (a first `didOpen`, or a reopen after a
    /// `didClose`); see the type-level docs for why that widens staleness to
    /// analyses with an unresolved import.
    fn note_stale_entries(&mut self, changed: &Path, newly_available: bool) {
        for state in self.entries.values_mut() {
            let stale = state.analysis.as_ref().is_some_and(|analysis| {
                analysis.closure_contains(changed)
                    || (newly_available && analysis.had_missing_import())
            });
            if stale {
                state.analysis = None;
            }
        }
    }
}

/// Test-only seam: a bounded, checkpointed delay inside the tracked analysis
/// query, so an out-of-process test can hold an analysis in flight long enough
/// for a cancellation to land — deterministically, because every 25ms slice
/// first polls for cancellation and unwinds the moment one is pending. Armed only
/// via the environment (out-of-process e2e); at most 5s on the broken-cancellation
/// path, well under a 30s receive bound.
#[cfg(debug_assertions)]
mod test_seams {
    pub(crate) const SLOW_ANALYSIS_ENV: &str = "INFERENCE_IDE_TEST_SLOW_ANALYSIS_PATH_SUBSTR";

    pub(crate) fn slow_analysis_seam(db: &dyn super::IdeDatabase, path: &std::path::Path) {
        let Ok(substr) = std::env::var(SLOW_ANALYSIS_ENV) else {
            return;
        };
        if substr.is_empty() || !path.to_string_lossy().contains(&substr) {
            return;
        }
        for _ in 0..200 {
            super::Database::unwind_if_revision_cancelled(db);
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}

const _: () = {
    const fn assert_send<T: Send>() {}
    assert_send::<RootDatabase>();
};
