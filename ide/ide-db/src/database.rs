//! [`RootDatabase`]: the open-document store plus per-file analyses memoized by
//! Salsa.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock, RwLockReadGuard};

use inference_vfs::Vfs;
use rustc_hash::FxHashMap;
use salsa::{Database, Setter, Storage};

use crate::analysis::FileAnalysis;
use crate::cancellation::{AnalysisCancelSource, ReaderTokenRegistration};

/// One project entry's Salsa input: its identity plus the eviction lever Salsa's
/// own dependency tracking cannot supply.
///
/// `path` and `src_root` are the compute's real inputs — a query reading them
/// depends on them the ordinary way. `evicted` is the **eviction** lever: an entry
/// with no live overlay (a closed document, or a never-opened path pushed out of
/// the cap) has no file event and so no change stamp to bump, yet its memoized
/// analysis must be released. Setting `evicted` to `true` invalidates the memo and
/// routes the entry to a tiny sentinel result (see [`analyze_entry`] and
/// [`RootDatabase::evict_analysis`]); setting it back to `false` on the next
/// requery forces exactly one full recompute (see
/// [`RootDatabase::clear_eviction`]). Ordinary content changes never flow through
/// `evicted`; they are carried by the per-file change stamps and the availability
/// epoch below, which the query reads once its import closure is known. The query
/// body reads all three, so a change to any of them invalidates its memo.
///
/// The flag needs no monotonic counter: it is only ever toggled, and each toggle
/// is guarded (`evict` by `!state.evicted`, un-evict by `state.evicted`) so a
/// same-value set never occurs — every write is a real change and always
/// invalidates the memo.
#[salsa::input]
struct EntryInput {
    #[returns(ref)]
    path: PathBuf,
    #[returns(ref)]
    src_root: PathBuf,
    evicted: bool,
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
    /// A read guard over the shared editor overlay.
    ///
    /// The overlay lives behind an `Arc<RwLock<Vfs>>` shared with every snapshot
    /// read clone (#292), so this returns a guard rather than a bare `&Vfs`. The
    /// analysis query binds the guard across the whole compute: a concurrent
    /// write cannot mutate the overlay until this guard releases, and the
    /// write-turn choke point (see [`RootDatabase::apply_overlay_write`]) bumps
    /// the change stamp first — quiescing every reader clone through Salsa's
    /// own outstanding-handle wait — so by the time a write takes the write lock
    /// no reader holds this guard, making the write uncontended by construction.
    fn vfs(&self) -> RwLockReadGuard<'_, Vfs>;
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

    /// Debug-only: whether this handle is a snapshot read clone (no worker state)
    /// rather than the worker. The rendezvous test seam parks only on a reader, so
    /// it never blocks the worker's own analyses (#292).
    #[cfg(debug_assertions)]
    fn debug_is_reader(&self) -> bool;
}

/// The memoized result of an entry's analysis: the computed [`FileAnalysis`], or
/// the sentinel an evicted entry memoizes so Salsa releases its superseded memo.
///
/// A tracked function's output must implement [`salsa::Update`], whose blanket
/// impls recurse structurally into a value's fields. `FileAnalysis` wraps the type
/// checker's arena and symbol table, none of which implement `Update`, so no
/// structural impl exists and `Arc<FileAnalysis>` cannot be returned directly.
/// The impl below exists **only** to satisfy that static bound: Salsa never calls
/// `maybe_update` on a tracked function's output — it replaces the memo wholesale
/// and decides backdating purely by comparing values (see `no_eq` on the query).
///
/// [`Evicted`](Self::Evicted) is the roughly two-word sentinel an evicted entry
/// memoizes: recomputing the query while the entry's `evicted` flag is set stores
/// this in place of the fat analysis, which pushes the superseded value onto
/// Salsa's deleted list to be freed at the next revision boundary. It is never
/// served — [`RootDatabase::analysis`] clears the flag before any serving fetch, so
/// a fetch that must return a result always recomputes a [`Computed`](Self::Computed).
#[derive(Clone)]
enum AnalysisResult {
    Computed(Arc<FileAnalysis>),
    Evicted,
}

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
/// The content-change dependencies are registered *after* the compute, once the
/// import closure is known: one change-stamp edge per closure file, plus the
/// availability epoch when an import went unresolved (see the loop below). The
/// generation is taken from the database counter so it advances only when this body
/// actually runs.
///
/// The `evicted` read must be the **first** statement. Reading it before the
/// slow-analysis test seam keeps a landed sentinel swap from ever sleeping, and
/// reading it before the generation mint keeps a sentinel execution from touching
/// the recompute probe — generations count full computes only, which every
/// relational generation assertion relies on. A sentinel memo's sole dependency is
/// this flag, so later stamp or epoch bumps never re-execute it.
///
/// `no_eq`: a `FileAnalysis` has no meaningful equality and is not `PartialEq`.
/// The option removes that requirement and disables backdating — the result is
/// always treated as changed — which costs nothing while no other query depends
/// on this one.
#[salsa::tracked(no_eq)]
fn analyze_entry(db: &dyn IdeDatabase, entry: EntryInput) -> AnalysisResult {
    if entry.evicted(db) {
        return AnalysisResult::Evicted;
    }
    let path = entry.path(db);
    let src_root = entry.src_root(db);
    #[cfg(debug_assertions)]
    test_seams::slow_analysis_seam(db, path);
    #[cfg(debug_assertions)]
    test_seams::gate_seam(db, path);
    #[cfg(debug_assertions)]
    test_seams::rendezvous_seam(db, path);
    let generation = db.next_generation();
    // The database hands the compute a hook that unwinds if a cancellation
    // request has landed, so a long analysis is interruptible at stage
    // boundaries rather than only at the fetch entry.
    let checkpoint = || db.unwind_if_revision_cancelled();
    // The overlay read guard is held across the whole compute. On a snapshot read
    // this runs on a pool thread against the shared overlay; on an unwind (a
    // cancellation landing mid-compute) the guard is released as the stack
    // unwinds, strictly before the snapshot's Storage clone drops — which is why a
    // concurrent write, whose stamp bump waits for that clone to drop, always
    // finds the overlay uncontended (#292).
    let vfs = db.vfs();
    let analysis = FileAnalysis::compute(&vfs, path, src_root, generation, &checkpoint);
    drop(vfs);

    // Register this compute's content-change dependencies now that the closure is
    // known. Reading each closure file's stamp records the per-file input edge that
    // makes a later `change_document`/`didClose` of that file recompute this memo;
    // the availability epoch is read only when an import went unresolved, so exactly
    // the memos a newly-available file might fix carry that edge.
    //
    // Registering AFTER the compute is sound: Salsa records a query's input edges in
    // execution order, and order affects only verification short-circuiting, not
    // which edges exist. No revision can advance mid-query even when this runs on a
    // snapshot read clone: a setter's `cancel_others` cannot complete while any
    // reader clone is alive, so a write that would advance the revision blocks until
    // this compute has unwound or finished and its handle has dropped (#292).
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
    AnalysisResult::Computed(Arc::new(analysis))
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
/// src-root drift bump, and the recompute branch's `evicted`-flag clear) are
/// idempotent setup that re-converges on retry — each is guarded so a re-run is a
/// no-op — and result writes happen only after the query returns, so a cancelled
/// compute leaves the entry in the consistent invalidated shape. A pending sentinel
/// swap is likewise unwind-safe: it is drained on a pop-after-success basis, so a
/// cancelled drain leaves the queue intact for the next read to land. Salsa cannot
/// observe the file reads
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
/// set and are invalidated (not dropped) as their closures change. Three sources
/// of memoized analyses are bounded and their memos actually freed, so a long
/// session cannot grow the tracked set without limit:
///
/// * **Closing a document** removes its overlay, so its analysis (computed from
///   that overlay) must stop serving vanished buffer text; a later query
///   recomputes it from disk. Closure-aware invalidation already forces that
///   recompute — a document is always part of its own closure — and
///   `close_document` additionally evicts the closed entry so its memo is freed.
/// * **Feature requests on never-opened paths** (a hover or goto against a URI the
///   editor never sent a `didOpen` for reaches disk through the loader) each
///   memoize an entry that no document change ever invalidates. These are capped
///   at [`MAX_UNOPENED_ANALYSES`] with FIFO eviction of the oldest.
/// * **Never-opened entries staled by a change** and pruned from the cap's order
///   are evicted too, rather than silently dropped with their memo left resident.
///
/// Salsa 0.27 exposes no per-memo eviction, so freeing works by a two-step
/// sentinel swap (see [`evict_analysis`](Self::evict_analysis) and
/// [`drain_pending_sentinel_swaps`](Self::drain_pending_sentinel_swaps)): the
/// `evicted` flag is set, and the next read recomputes the entry to the tiny
/// [`AnalysisResult::Evicted`] sentinel, which pushes the superseded fat analysis
/// onto Salsa's deleted list to be freed at the next revision boundary. The honest
/// steady-state bound on resident full analyses is
///
/// ```text
/// open documents
///   + MAX_UNOPENED_ANALYSES
///   + (queued swaps not yet drained — cleared at the next analysis() call)
///   + (memos superseded or swapped since the last revision boundary
///      — cleared at the next Salsa write; a deferred-drop lag that exists
///        for every recompute, salsa-0.27.2-version-pinned)
/// ```
///
/// A permanent small residue is accepted per distinct path ever touched: the
/// `EntryState`, the `EntryInput` slot, the `FileStamp` slot, the debug registry
/// entry, and the `Vfs` id — on the order of 100–200 B — because Salsa 0.27 has no
/// input removal. It is unbounded only in the number of distinct paths, the same
/// steady state rust-analyzer ships, and is re-audited on any Salsa upgrade.
///
/// # Concurrent snapshot reads (#292)
///
/// A feature request against an already-memoized (or stale-but-cheaply-recomputed)
/// entry can be served on a background thread without holding the write handle.
/// The worker mints a [`ReadSnapshot`] ([`plan_concurrent_read`](Self::plan_concurrent_read)):
/// a second database handle built by cloning the Salsa `Storage` and sharing the
/// overlay, generation counter, and stamp registry (all behind `Arc`), with no
/// [`WorkerState`] of its own. A pool thread runs the analysis query against that
/// handle ([`ReadSnapshot::serve`]); Salsa's own claim/block serialization means a
/// memo hit re-serves the stored `Arc` and a stale entry re-executes exactly once,
/// with zero writes on the serving path. The worker stays the **sole** minter and
/// the sole mutator of [`WorkerState`]: a snapshot cannot create an entry, evict, or
/// touch the cap, so the resident-memory bound above holds unchanged. A concurrent
/// write quiesces every live snapshot before it mutates — the write's change-stamp
/// bump is a Salsa setter, and a setter waits for every outstanding handle to drop,
/// so an in-flight snapshot unwinds at its next checkpoint and releases its clone
/// before the write proceeds (see [`apply_overlay_write`](Self::apply_overlay_write)).
#[salsa::db]
pub struct RootDatabase {
    storage: Storage<Self>,
    /// Editor overlay, held outside Salsa storage: the analysis query reads it
    /// through the overlay-then-disk loader, the seam the compiler and IDE share.
    /// Behind an `Arc<RwLock<..>>` so a snapshot read clone reads the same overlay
    /// the worker writes (#292); the write-turn choke point keeps the write lock
    /// uncontended (see [`apply_overlay_write`](Self::apply_overlay_write)).
    vfs: Arc<RwLock<Vfs>>,
    /// Monotonic source of per-analysis generation stamps, read inside the query
    /// body (see [`IdeDatabase::next_generation`]). Outside Salsa storage because
    /// it is a side effect of running the compute, not an input to it. Shared by
    /// `Arc` with snapshot read clones so a reader-minted generation and a
    /// worker-minted one are drawn from one sequence — which is what keeps
    /// [`FileAnalysis::generation`] a global recompute probe across threads.
    generation_counter: Arc<AtomicU64>,
    /// Append-only path → [`FileStamp`] registry, shared by the write-path bump and
    /// the analysis query's in-query dependency registration so both observe one
    /// input per path. Behind `Arc<Mutex<..>>` because a snapshot read clone can
    /// create a stamp input for a newly-reached closure file, and the worker's next
    /// bump must observe that same input. Salsa inputs are never destroyed, so
    /// entries are permanent for the session, bounded by the set of files touched.
    file_stamps: Arc<Mutex<FxHashMap<PathBuf, FileStamp>>>,
    /// The worker-exclusive bookkeeping — entries, sticky roots, the never-opened
    /// FIFO, and the pending sentinel swaps. `Some` on the worker handle, `None` on
    /// every snapshot read clone: a reader has no business creating an entry,
    /// evicting, or touching the cap (see the module docs and [`worker_mut`](Self::worker_mut)).
    worker: Option<WorkerState>,
    /// Debug-only weak registry of every full analysis this database has handed out,
    /// so [`debug_live_analyses`](Self::debug_live_analyses) can probe true liveness:
    /// a `Weak` that no longer upgrades has no strong retainer anywhere — memo,
    /// mirror, deleted list, or caller. Behind `Arc<Mutex<..>>` so an analysis a
    /// snapshot read handed out registers in the same registry the worker probes.
    /// `Weak<FileAnalysis>` is `Send + Sync`, so the `Send` assertion on
    /// `RootDatabase` still holds.
    #[cfg(debug_assertions)]
    live_analyses: Arc<Mutex<Vec<std::sync::Weak<FileAnalysis>>>>,
}

/// The worker handle's exclusive bookkeeping (#292).
///
/// Held only by the worker database handle ([`RootDatabase::worker`] is `Some`);
/// a snapshot read clone carries `None` and can reach none of it. Grouping these
/// four tables behind one `Option` is what makes reader misuse a single loud
/// panic ([`RootDatabase::worker_mut`]) rather than a scatter of guards: a path's
/// first compute and every cap/root/mirror mutation structurally happen on the
/// worker, so a reader forking this state is impossible rather than merely
/// locked-against.
#[derive(Default)]
struct WorkerState {
    /// Monotonic source of [`FileStamp`]/[`AvailabilityEpoch`] values. Bumps are
    /// set-only: an input field is written through its setter, never
    /// read-modify-written, since reading an input field outside a query registers
    /// no edge and would only mislead.
    stamp_source: u64,
    /// Per-entry bookkeeping `RootDatabase` owns rather than Salsa: the reusable
    /// input handle plus the latest analysis, whose closure metadata drives the
    /// write-path staleness mirror, the closure-donor search,
    /// [`RootDatabase::is_analyzed`], and the never-opened cap. `analysis` is
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
    /// Entries whose `evicted` flag was just set, awaiting the sentinel-recompute
    /// that releases their superseded memo. Populated by
    /// [`RootDatabase::evict_analysis`] (from the cap, the prune, and
    /// `close_document`) and drained only inside [`RootDatabase::analysis`] or the
    /// idle bookkeeping pass: a notification handler must not fetch, so the write
    /// path queues and the read path lands the swap. Drained last-first with a
    /// pop-after-success so a cancelled fetch leaves the queue intact for the next
    /// read to retry.
    pending_sentinel_swaps: Vec<EntryInput>,
}

/// One overlay mutation for the write-turn choke point
/// ([`RootDatabase::apply_overlay_write`]).
enum OverlayOp {
    Open { text: Arc<str>, newly_available: bool },
    Change { text: Arc<str> },
    Close,
}

#[salsa::db]
impl salsa::Database for RootDatabase {}

#[salsa::db]
impl IdeDatabase for RootDatabase {
    fn vfs(&self) -> RwLockReadGuard<'_, Vfs> {
        self.vfs.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn next_generation(&self) -> u64 {
        // Purely observational, so relaxed ordering suffices. Shared by `Arc` with
        // snapshot read clones, so a reader recompute and a worker recompute mint
        // from one sequence — generation equality across threads is what makes the
        // recompute probe sound.
        self.generation_counter.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn file_stamp(&self, path: &Path) -> FileStamp {
        // Get-or-create so the write path and the in-query registration share one
        // input per path. Callable through the shared `&self` a snapshot read holds
        // — a reader that reaches a new closure file creates its stamp here and the
        // worker's next bump observes the same input. The registry
        // guard is released when this method returns, before any caller reaches a
        // setter (which needs `&mut self`, worker-only).
        let mut registry = self.file_stamps.lock().unwrap_or_else(PoisonError::into_inner);
        *registry
            .entry(path.to_path_buf())
            .or_insert_with(|| FileStamp::new(self, 0))
    }

    fn availability_epoch(&self) -> AvailabilityEpoch {
        // The singleton is created eagerly at construction (see `init`), so on a
        // snapshot read this is always a `try_get` hit — never the `new` branch,
        // which two racing readers would hit simultaneously and panic ("singleton
        // struct may not be duplicated").
        AvailabilityEpoch::try_get(self).unwrap_or_else(|| AvailabilityEpoch::new(self, 0))
    }

    #[cfg(debug_assertions)]
    fn debug_is_reader(&self) -> bool {
        self.worker.is_none()
    }
}

impl Default for RootDatabase {
    fn default() -> Self {
        Self::init(Storage::default())
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
    /// The ide-db-side mirror of the entry's `evicted` Salsa field, so a no-op
    /// double-eviction or a fresh entry's requery can be skipped without
    /// read-modify-writing the input. Set with `analysis` in one atomic step by
    /// [`evict_analysis`](RootDatabase::evict_analysis), establishing the invariant
    /// `evicted ⟹ analysis is None`; only
    /// [`clear_eviction`](RootDatabase::clear_eviction) breaks it, clearing the flag
    /// first. A debug tripwire in [`analysis`](RootDatabase::analysis) checks it
    /// against the Salsa field.
    evicted: bool,
}

/// The most memoized analyses to retain for documents that were never opened.
///
/// A feature request against a URI the editor never opened memoizes an analysis
/// that no document change invalidates, so without a bound they accumulate for the
/// life of the session. Eight is comfortably more than the handful of dependency
/// files a single navigation touches, while keeping the retained set small; the
/// eviction is FIFO over never-opened entries only, so open documents are never
/// affected.
///
/// Public so tests (and the honest resident-memory bound in the crate docs) can
/// derive their limits from this constant rather than a magic number.
pub const MAX_UNOPENED_ANALYSES: usize = 8;

impl RootDatabase {
    /// Builds a worker handle over `storage`, eagerly creating the availability
    /// epoch singleton.
    ///
    /// Every constructor routes through here so the singleton exists before any
    /// query — or any snapshot read clone — can reach the try-get-then-new funnel
    /// ([`IdeDatabase::availability_epoch`]): eager creation is what kills the
    /// reader-side duplicate-singleton race. [`with_execute_probe`](Self::with_execute_probe)
    /// must build through here with its own probing storage, not construct the
    /// fields and inherit a default's singleton, which would create it in a
    /// thrown-away storage.
    fn init(storage: Storage<Self>) -> Self {
        let db = Self {
            storage,
            vfs: Arc::new(RwLock::new(Vfs::default())),
            generation_counter: Arc::new(AtomicU64::new(0)),
            file_stamps: Arc::new(Mutex::new(FxHashMap::default())),
            worker: Some(WorkerState::default()),
            #[cfg(debug_assertions)]
            live_analyses: Arc::new(Mutex::new(Vec::new())),
        };
        // Force the singleton into existence now, on the worker handle, so a reader
        // clone never takes the `new` branch.
        let _ = db.availability_epoch();
        db
    }

    /// The worker-exclusive bookkeeping, or a loud panic on a snapshot read clone.
    ///
    /// A snapshot read handle carries `worker: None` (see the module docs); any
    /// worker-only method reached on such a handle is a bug, so this panics in
    /// release too, not only in debug. Contained by the read pool's catch on the
    /// LSP side; a worker-side occurrence rebuilds the host.
    fn worker(&self) -> &WorkerState {
        self.worker
            .as_ref()
            .expect("worker-only state accessed on a reader snapshot")
    }

    /// The mutable worker-exclusive bookkeeping, or a loud panic on a snapshot
    /// read clone (see [`worker`](Self::worker)).
    fn worker_mut(&mut self) -> &mut WorkerState {
        self.worker
            .as_mut()
            .expect("worker-only state accessed on a reader snapshot")
    }

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
        // still subsumes the truly-first open. Computed before the choke point
        // mutates the overlay.
        let newly_available = self
            .vfs
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .contents_of_path(path)
            .is_none();
        self.apply_overlay_write(
            path,
            &OverlayOp::Open {
                text: text.into(),
                newly_available,
            },
        );
    }

    /// Replaces the in-memory contents of an open `path` (an editor `didChange`).
    ///
    /// A change never introduces a previously-unseen file, so only closures that
    /// contain `path` are recomputed and marked stale.
    pub fn change_document(&mut self, path: &Path, text: impl Into<Arc<str>>) {
        // A change never makes a previously-absent file available, so no epoch
        // bump; the choke point applies the overlay mutation and the paired stamp
        // bump (see `open_document`).
        self.apply_overlay_write(path, &OverlayOp::Change { text: text.into() });
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
    /// Both the overlay text and the closed entry's memoized analysis are freed:
    /// the eviction below sets the entry's `evicted` flag and queues a sentinel
    /// swap, so the superseded memo is released at the next
    /// [`analysis`](Self::analysis) call and dropped at the following revision
    /// boundary. The `EntryState` slot itself persists (Salsa 0.27 has no input
    /// removal).
    pub fn close_document(&mut self, path: &Path) {
        // The choke point removes the overlay, drops the sticky source root so the
        // next open re-resolves from scratch (observing a manifest created or a
        // governing entry opened meanwhile), and evicts the closed entry's memo.
        // Queue-only eviction: the swap's fetch is deferred to the next `analysis`
        // or idle drain because a fetch is a cancellation checkpoint, and a close
        // superseded by a newer write must run to completion. A close of a
        // never-analyzed path is a no-op (no `EntryState`).
        self.apply_overlay_write(path, &OverlayOp::Close);
    }

    /// The write-turn choke point: applies one overlay mutation in a fixed order
    /// that keeps a snapshot read from ever observing a torn state (#292).
    ///
    /// The order is load-bearing:
    ///
    /// 1. **Bump the change stamp first.** The setter is the quiesce point — its
    ///    `cancel_others` fires the pending-write flag, every live snapshot read
    ///    unwinds at its next checkpoint and drops its cloned
    ///    handle, and no new reader can appear because the worker is the sole
    ///    minter and is mid-turn here. Between this bump and the overlay mutation
    ///    the stamp is new while the overlay is old, but no reader exists to
    ///    observe the gap and the worker is not fetching.
    /// 2. **Apply the overlay mutation** under the write lock, uncontended by
    ///    construction (see [`overlay_write`](Self::overlay_write)).
    /// 3. **Bump the availability epoch** when the open newly made content
    ///    available — the one edge that re-fires a missing-import memo.
    /// 4. **Mirror the staleness and run the per-op tail** (the never-opened FIFO
    ///    retain, and for a close the sticky-root drop and memo eviction).
    ///
    /// The setter/revision-boundary count per call is identical to before the
    /// snapshot split — exactly one stamp bump, plus at most the epoch bump and the
    /// eviction setter — so the memory-liveness arithmetic is unchanged.
    fn apply_overlay_write(&mut self, path: &Path, op: &OverlayOp) {
        let newly_available = matches!(
            op,
            OverlayOp::Open {
                newly_available: true,
                ..
            }
        );
        // (1) Stamp bump first — the quiesce point.
        self.bump_file_stamp(path);
        // (2) Overlay mutation under the (uncontended) write lock.
        {
            let mut vfs = self.overlay_write();
            match &op {
                OverlayOp::Open { text, .. } | OverlayOp::Change { text } => {
                    let id = vfs.intern(path);
                    vfs.set_contents(id, Arc::clone(text));
                }
                OverlayOp::Close => {
                    if let Some(id) = vfs.file_id(path) {
                        vfs.remove_contents(id);
                    }
                }
            }
        }
        // (3) Availability epoch for a newly-available open.
        if newly_available {
            self.bump_availability_epoch();
        }
        // (4) Mirror staleness, then the per-op tail.
        self.note_stale_entries(path, newly_available);
        match op {
            OverlayOp::Open { .. } => {
                // An opened document is part of the editor's working set, never a
                // never-opened entry subject to the eviction cap.
                self.worker_mut()
                    .unopened_order
                    .retain(|tracked| tracked != path);
            }
            OverlayOp::Change { .. } => {}
            OverlayOp::Close => {
                self.worker_mut().source_roots.remove(path);
                self.worker_mut()
                    .unopened_order
                    .retain(|tracked| tracked != path);
                self.evict_analysis(path);
            }
        }
    }

    /// Takes the overlay write lock at the choke point.
    ///
    /// The change-stamp bump in [`apply_overlay_write`](Self::apply_overlay_write)
    /// already quiesced every snapshot read (Salsa's setter waits for every
    /// outstanding handle to drop, and a read releases its overlay guard as it
    /// unwinds — strictly before its handle drops), so the write lock is
    /// immediately acquirable. In debug a contended lock here is a real bug: a
    /// write path that reached the overlay without first bumping the stamp.
    fn overlay_write(&self) -> std::sync::RwLockWriteGuard<'_, Vfs> {
        #[cfg(debug_assertions)]
        {
            return match self.vfs.try_write() {
                Ok(guard) => guard,
                Err(std::sync::TryLockError::WouldBlock) => panic!(
                    "overlay write contended at the choke point: a snapshot read \
                     still holds the overlay, so the change-stamp bump did not \
                     precede this overlay mutation (#292)"
                ),
                Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            };
        }
        #[cfg(not(debug_assertions))]
        {
            self.vfs.write().unwrap_or_else(PoisonError::into_inner)
        }
    }

    /// Whether an analysis for `path` is currently memoized (computed and not
    /// invalidated).
    ///
    /// Used by the protocol layer to tell which open documents a change actually
    /// invalidated (their analyses were dropped) from those left untouched.
    #[must_use = "the analyzed state is the reason to call this"]
    pub fn is_analyzed(&self, path: &Path) -> bool {
        self.worker()
            .entries
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

    /// Decides whether a feature request for `path` can be served on a pool thread
    /// off a [`ReadSnapshot`], or must run serially on the worker (#292).
    ///
    /// A read is snapshot-eligible only when its entry is cheap and safe to serve
    /// elsewhere: the entry must exist (a never-analyzed path's first compute is
    /// worker-only, which is what keeps the never-opened cap unbypassable), it must
    /// not be evicted, and it must be either a mirror **hit** (memoized — a pool
    /// serve is a zero-write memo hit) or **stale under a cached definitive source
    /// root** (a pool recompute is then identical to a worker recompute, because
    /// [`resolve_source_root`](Self::resolve_source_root) consults its cache first).
    /// A stale entry whose root is *not* cached (tier-3 provisional) recomputes
    /// serially so a donor/manifest upgrade lands exactly as it does today.
    ///
    /// Worker-only: the returned snapshot carries a cloned Salsa handle, and the
    /// worker is the sole minter (`worker().` panics on a reader). The clone shares
    /// the overlay, generation counter, and stamp registry, and registers its
    /// cancellation token so a later write unwinds it (see the module docs).
    #[must_use = "the plan decides where the read runs"]
    pub fn plan_concurrent_read(
        &self,
        path: &Path,
        source: &AnalysisCancelSource,
    ) -> ConcurrentReadPlan {
        // Only the worker mints snapshots — the clone-drain liveness argument (a
        // write's wait terminates because the reader population strictly decreases)
        // depends on it. `worker()` itself panics loud on a reader.
        debug_assert!(
            self.worker.is_some(),
            "only the worker mints snapshots (#292)"
        );
        let worker = self.worker();
        let Some(state) = worker.entries.get(path) else {
            return ConcurrentReadPlan::Serial;
        };
        if state.evicted {
            return ConcurrentReadPlan::Serial;
        }
        let hit = state.analysis.is_some();
        let cached_root = worker.source_roots.contains_key(path);
        if !(hit || cached_root) {
            return ConcurrentReadPlan::Serial;
        }
        let input = state.input;
        let mirror_generation = state.analysis.as_ref().map(|analysis| analysis.generation());

        let reader_db = RootDatabase {
            storage: self.storage.clone(),
            vfs: Arc::clone(&self.vfs),
            generation_counter: Arc::clone(&self.generation_counter),
            file_stamps: Arc::clone(&self.file_stamps),
            worker: None,
            #[cfg(debug_assertions)]
            live_analyses: Arc::clone(&self.live_analyses),
        };
        let registration = source.register_reader(Database::cancellation_token(&reader_db));
        let dispatch_epoch = source.epoch();
        #[cfg(debug_assertions)]
        debug_snapshots::on_mint();
        ConcurrentReadPlan::Concurrent(ReadSnapshot {
            db: reader_db,
            input,
            dispatch_epoch,
            mirror_generation,
            _source: source.clone(),
            _registration: registration,
        })
    }

    /// Stores a pool-served analysis into the entry mirror, guarded against every
    /// race that could stale it (#292).
    ///
    /// Applied only when nothing moved since the snapshot was dispatched: the
    /// dispatch epoch is still current (no write superseded the read), the entry
    /// still exists, its mirror is still empty, and it is not evicted. A skipped
    /// store is the tolerated over-clear direction — at worst a later spurious
    /// republish — and keeps the `prior_generation` alignment tripwire structurally
    /// unreachable from pool activity. Worker-only.
    pub fn apply_concurrent_read(
        &mut self,
        path: &Path,
        analysis: &Arc<FileAnalysis>,
        dispatch_epoch: u64,
        source: &AnalysisCancelSource,
    ) {
        if source.epoch() != dispatch_epoch {
            return;
        }
        if let Some(state) = self.worker_mut().entries.get_mut(path)
            && state.analysis.is_none()
            && !state.evicted
        {
            state.analysis = Some(Arc::clone(analysis));
        }
    }

    /// Runs the deferred never-opened bookkeeping for a pool-recomputed `path`
    /// (#292).
    ///
    /// A pool read cannot create an entry (first computes are worker-only), so the
    /// resident-analysis bound is never exceeded by pool activity; only the cap's
    /// recency order and stale-prune timing are deferred to here. Re-checks the
    /// overlay first so a `didOpen` that arrived between the read and this apply
    /// never enrolls an open document in the never-opened FIFO, then drains any
    /// cap/prune evictions so the deferred-release window stays bounded.
    ///
    /// Caller contract: only when no concurrent reads are in flight, so the
    /// eviction setters here cannot storm-cancel a sibling pool read. Worker-only.
    pub fn apply_unopened_read_bookkeeping(&mut self, path: &Path) {
        let still_unopened = self
            .vfs
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .contents_of_path(path)
            .is_none();
        if still_unopened {
            self.record_unopened_analysis(path.to_path_buf());
        }
        self.drain_pending_sentinel_swaps();
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
        // Land any sentinel swaps queued since the last read (a close, a prune, a
        // cap eviction). Doing it first releases their superseded memos before this
        // request's work, and an unwind here touches nothing of the requested entry.
        self.drain_pending_sentinel_swaps();
        let recomputed = !self.is_analyzed(path);
        if recomputed {
            let src_root = self.resolve_source_root(path);
            self.sync_entry_input(path, &src_root);
            // The mirror's `evicted` and the Salsa field must agree before the
            // un-evict: a drift means an eviction set one without the other.
            #[cfg(debug_assertions)]
            {
                let input = self.worker().entries[path].input;
                let mirror_evicted = self.worker().entries[path].evicted;
                debug_assert_eq!(
                    mirror_evicted,
                    input.evicted(&*self),
                    "the EntryState.evicted mirror and the Salsa evicted field drifted"
                );
            }
            // A recompute of a previously-evicted entry un-evicts it: the false-write
            // both forces the full re-execution and is the revision boundary that
            // frees the prior deferred memo. A never-evicted entry is a no-op.
            self.clear_eviction(path);
        }
        // Debug-only alignment tripwire: on a mirror hit (the entry still holds an
        // analysis, so `recomputed` is false) the fetch below must be a Salsa memo
        // hit, so its generation must equal the mirror's. A mismatch means a stamp or
        // epoch edge fired while the write path left the mirror intact — the edges
        // and `note_stale_entries` have drifted apart. Captured before the fetch so
        // the mirror value is read while it is still the previous one.
        #[cfg(debug_assertions)]
        let prior_generation = (!recomputed).then(|| {
            self.worker().entries[path]
                .analysis
                .as_ref()
                .expect("a mirror hit holds an analysis")
                .generation()
        });
        let input = self.worker().entries[path].input;
        // Salsa returns the memo when the entry's inputs are unchanged and reruns
        // `analyze_entry` (minting a fresh generation) when a change-stamp, epoch, or
        // evicted-flag bump marked it stale.
        //
        // Contract (recorded invariant, #157): this serving fetch performs zero
        // Salsa writes. Every setter reachable from `analysis` sits on the
        // recompute branch's `clear_eviction` or in `drain_pending_sentinel_swaps`,
        // so a memo hit is write-free — the property the zero-write cap read
        // sequence relies on, and the precondition for answering queries from
        // cloned `Storage` handles on other threads (#292): a write from any
        // handle blocks until every other handle drops, so a serving path that
        // wrote would deadlock its own readers. The `Evicted` arm below is a release-build self-heal
        // that must never run: the recompute branch cleared the flag before this
        // fetch, and `evicted ⟹ mirror is None` means an evicted entry always took
        // the recompute branch.
        let analysis = match analyze_entry(&*self, input) {
            AnalysisResult::Computed(analysis) => analysis,
            AnalysisResult::Evicted => {
                debug_assert!(
                    false,
                    "an evicted sentinel reached a serving fetch: evicted implies the \
                     mirror is None, so the recompute branch clears the flag first"
                );
                self.clear_eviction(path);
                match analyze_entry(&*self, input) {
                    AnalysisResult::Computed(analysis) => analysis,
                    AnalysisResult::Evicted => {
                        unreachable!("the flag was just cleared")
                    }
                }
            }
        };
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
        // Register the handed-out analysis for the liveness probe before it is moved
        // into the mirror, so `debug_live_analyses` can later tell a freed memo from
        // a resident one. Retain-then-push keeps the registry from growing unboundedly.
        #[cfg(debug_assertions)]
        if recomputed {
            let mut registry = self
                .live_analyses
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            registry.retain(|weak| weak.strong_count() > 0);
            registry.push(Arc::downgrade(&analysis));
        }

        let is_unopened = self
            .vfs
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .contents_of_path(path)
            .is_none();
        self.worker_mut()
            .entries
            .get_mut(path)
            .expect("the entry is present on both the hit and the recompute path")
            .analysis = Some(analysis);
        if recomputed && is_unopened {
            // Memoized for a path the editor never opened; bound how many such
            // entries accumulate over a session.
            self.record_unopened_analysis(path.to_path_buf());
        }
        // Land this call's own cap/prune evictions so the resident-memory shape
        // matches the eager cap: the swaps queued just above are released now. An
        // unwind here happens after this entry's result is stored, so the caller
        // could observe `Cancelled` with `is_analyzed() == true`; that only arises
        // from a cross-thread token fire and converges on the next read's top drain.
        self.drain_pending_sentinel_swaps();
        self.worker().entries[path]
            .analysis
            .as_deref()
            .expect("the analysis was stored above")
    }

    /// Ensures an [`EntryInput`] exists for `path` carrying `src_root`.
    ///
    /// A new entry starts un-evicted with no analysis. An existing entry's source
    /// root is updated only when it drifted (a close/reopen re-resolution), which
    /// itself marks the query stale so the recompute uses the new root.
    fn sync_entry_input(&mut self, path: &Path, src_root: &Path) {
        if let Some(input) = self.worker().entries.get(path).map(|state| state.input) {
            if input.src_root(&*self).as_path() != src_root {
                input.set_src_root(self).to(src_root.to_path_buf());
            }
        } else {
            let input = EntryInput::new(&*self, path.to_path_buf(), src_root.to_path_buf(), false);
            self.worker_mut().entries.insert(
                path.to_path_buf(),
                EntryState {
                    input,
                    analysis: None,
                    evicted: false,
                },
            );
        }
    }

    /// The eviction lever: clears `entry`'s mirror analysis, sets its `evicted`
    /// input, and queues the sentinel swap that frees its memo.
    ///
    /// Unlike a content change, an eviction has no file event and so no stamp to
    /// bump; the `evicted` flag is the only thing that can force the recompute.
    /// Folding eviction into the stamps would let a stale memo revalidate and break
    /// the never-opened cap's recompute guarantee, so it stays a distinct lever.
    /// Callers are the never-opened cap overflow and stale prune (see
    /// [`record_unopened_analysis`](Self::record_unopened_analysis)) and
    /// [`close_document`](Self::close_document).
    ///
    /// The gate is `EntryState` existence and `!state.evicted`, deliberately **not**
    /// [`is_analyzed`](Self::is_analyzed): an invalidated-then-closed document (its
    /// mirror already `None`) must still be freed, and a double eviction (a double
    /// close) must be a no-op — no second setter, no duplicate queue entry. The
    /// mirror-clear and flag-set are one atomic step, establishing the invariant
    /// `evicted ⟹ mirror is None`.
    ///
    /// The setter invalidates the fat memo but does **not** free it; the queued
    /// swap's fetch performs the release. Each `set_evicted` is itself a revision
    /// boundary that frees the *previous* eviction's deferred memo — Salsa 0.27.2
    /// clears its deleted list at every new revision, so at most one fat memo is
    /// ever pending (re-derive on any Salsa upgrade).
    ///
    /// A no-op for a path that was never analyzed.
    fn evict_analysis(&mut self, entry: &Path) {
        let Some(input) = self.worker_mut().entries.get_mut(entry).and_then(|state| {
            (!state.evicted).then(|| {
                state.analysis = None;
                state.evicted = true;
                state.input
            })
        }) else {
            return;
        };
        input.set_evicted(self).to(true);
        self.worker_mut().pending_sentinel_swaps.push(input);
    }

    /// Recomputes every queued evicted entry to its sentinel, releasing each
    /// superseded fat memo to Salsa's deleted list.
    ///
    /// Called from [`analysis`](Self::analysis) only — never from a notification
    /// handler, whose fetch could be abandoned half-done when a newer write
    /// supersedes it: the write path queues, the read path lands. Each fetch is a
    /// cancellation checkpoint (Salsa unwinds at fetch entry before any body), so an
    /// unwind leaves the queue intact — this queue *is* the strand repair, and the
    /// next `analysis` self-heals. No `catch_unwind` anywhere in ide-db: a
    /// `Cancelled` unwind must propagate for the protocol layer's superseded/retry
    /// classification. Pop-after-success so a cancelled drain retries the same entry.
    fn drain_pending_sentinel_swaps(&mut self) {
        while let Some(input) = self.worker().pending_sentinel_swaps.last().copied() {
            let result = analyze_entry(&*self, input);
            debug_assert!(
                matches!(result, AnalysisResult::Evicted),
                "a queued sentinel swap must observe evicted == true"
            );
            self.worker_mut().pending_sentinel_swaps.pop();
        }
    }

    /// Clears `path`'s `evicted` flag on the recompute branch, forcing the full
    /// re-execution and freeing the last deferred memo.
    ///
    /// The false-write is (a) the input change that forces one full recompute with a
    /// fresh generation — the requery's cross-entry generation pin — and (b) the
    /// revision boundary that frees the previous eviction's deferred fat memo. A
    /// never-evicted entry (a fresh entry, or a plain content-change recompute) is a
    /// no-op: no same-value set.
    ///
    /// Ordering rule for the whole eviction lifecycle:
    /// `drain_pending_sentinel_swaps` (top of [`analysis`](Self::analysis)) →
    /// `clear_eviction` (recompute branch) → serving fetch → mirror store. Because
    /// the top-of-analysis drain precedes every `clear_eviction`, a queued swap can
    /// never be stale by the time the flag is cleared, so no stale-queue skip logic
    /// is needed — that is by design, not accident, and the `debug_assert` below
    /// machine-checks it.
    fn clear_eviction(&mut self, path: &Path) {
        let Some(input) = self.worker_mut().entries.get_mut(path).and_then(|state| {
            state.evicted.then(|| {
                state.evicted = false;
                state.input
            })
        }) else {
            return;
        };
        debug_assert!(
            !self.worker().pending_sentinel_swaps.contains(&input),
            "the top-of-analysis drain lands every queued swap before any un-evict"
        );
        input.set_evicted(self).to(false);
    }

    /// A fresh, strictly increasing [`FileStamp`]/[`AvailabilityEpoch`] value,
    /// distinct from any previously assigned, so setting it on an input always
    /// registers as a change.
    fn next_stamp(&mut self) -> u64 {
        let worker = self.worker_mut();
        worker.stamp_source += 1;
        worker.stamp_source
    }

    /// Bumps `path`'s change stamp so every memo whose closure contains it
    /// recomputes.
    ///
    /// Get-or-create is load-bearing: an ide-db-level `change_document` can precede
    /// any analysis of `path`, and a never-opened shared import's stamp may have
    /// been minted inside a query — on the worker or on a snapshot read clone, which
    /// share one stamp registry — so a lookup-only variant would silently
    /// under-invalidate. Every bump runs through a setter, whose `cancel_others`
    /// waits for every outstanding read handle to drop: this is precisely the
    /// quiesce that lets the write-turn choke point take the overlay lock
    /// uncontended (see [`apply_overlay_write`](Self::apply_overlay_write)). The
    /// registry guard from [`file_stamp`](Self::file_stamp) is dropped before the
    /// setter runs.
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
    /// The FIFO order is first pruned, splitting the entries dropped from it three
    /// ways: `path` itself is re-added at the back; an entry opened since (its
    /// overlay is now available) is promoted into the working set and **never**
    /// evicted; and an entry still never-opened but no longer memoized — staled by a
    /// change — is collected and evicted, freeing its lingering memo rather than
    /// silently dropping it. That third path closes a leak the cap alone never
    /// covered. Then the existing overflow loop evicts the oldest beyond the cap.
    ///
    /// Pin-safety: a staled entry's memo is already invalid (the mirror and the
    /// stamp/epoch edges move in lockstep — the alignment tripwire machine-checks
    /// it), so setting its flag changes *when* the memory is released, never what a
    /// requery observes: it recomputes with a fresh generation either way. Promoted
    /// (opened-since) entries are left untouched.
    fn record_unopened_analysis(&mut self, path: PathBuf) {
        let mut kept = VecDeque::with_capacity(self.worker().unopened_order.len() + 1);
        let mut pruned_stale = Vec::new();
        for tracked in std::mem::take(&mut self.worker_mut().unopened_order) {
            if tracked == path {
                continue;
            }
            let opened = self
                .vfs
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .contents_of_path(&tracked)
                .is_some();
            if opened {
                // Opened since it was memoized: promoted into the working set,
                // exempt from the cap and never evicted.
                continue;
            }
            if self.is_analyzed(&tracked) {
                kept.push_back(tracked);
            } else {
                // Still never-opened but staled by a change: free its memo.
                pruned_stale.push(tracked);
            }
        }
        kept.push_back(path);
        self.worker_mut().unopened_order = kept;

        for stale in pruned_stale {
            self.evict_analysis(&stale);
        }
        while self.worker().unopened_order.len() > MAX_UNOPENED_ANALYSES {
            let evicted = self.worker_mut().unopened_order.pop_front();
            if let Some(evicted) = evicted {
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
        if let Some(root) = self.worker().source_roots.get(entry) {
            return root.clone();
        }
        if let Some(root) = inference_project_model::manifest_source_root(entry) {
            self.worker_mut()
                .source_roots
                .insert(entry.to_path_buf(), root.clone());
            return root;
        }
        if let Some(root) = self.closure_donor_source_root(entry) {
            self.worker_mut()
                .source_roots
                .insert(entry.to_path_buf(), root.clone());
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
        self.worker()
            .entries
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
        for state in self.worker_mut().entries.values_mut() {
            let stale = state.analysis.as_ref().is_some_and(|analysis| {
                analysis.closure_contains(changed)
                    || (newly_available && analysis.had_missing_import())
            });
            if stale {
                state.analysis = None;
            }
        }
    }

    /// Debug-only liveness probe: the number of distinct full analyses with a live
    /// strong retainer anywhere (a memo, the mirror, Salsa's deleted list, or a
    /// caller).
    ///
    /// A `Weak` that no longer upgrades has been freed, so this distinguishes a
    /// memo that was truly released from one merely unserved — the exit criterion
    /// the memory-bound tests assert against. Upgradable entries are deduped by
    /// `Arc::as_ptr` so a tolerated-direction mirror over-clear that re-registers
    /// the same `Arc` is not double-counted.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    #[must_use = "the live-analysis count is the reason to call this"]
    pub fn debug_live_analyses(&self) -> usize {
        let mut registry = self
            .live_analyses
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        registry.retain(|weak| weak.strong_count() > 0);
        let mut seen: rustc_hash::FxHashSet<*const FileAnalysis> = rustc_hash::FxHashSet::default();
        for weak in registry.iter() {
            if let Some(analysis) = weak.upgrade() {
                seen.insert(Arc::as_ptr(&analysis));
            }
        }
        seen.len()
    }

    /// Debug-only constructor that counts `analyze_entry` executions.
    ///
    /// The `probe` is incremented on every Salsa `WillExecute` event — a full
    /// compute or a sentinel recompute — so a test can assert exact execution
    /// arithmetic (a memo hit fires no event, an eviction fires exactly its
    /// sentinel). Only one tracked function exists today; a test's exact counts
    /// must be revisited if more are added (#280).
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    #[must_use = "the probed database is the constructor's result"]
    pub fn with_execute_probe(probe: Arc<std::sync::atomic::AtomicUsize>) -> Self {
        // Route through `init` with the probing storage: `..Self::default()` would
        // discard a singleton created in the thrown-away default storage, and a
        // later reader clone would race to re-create it (#292).
        Self::init(Storage::new(Some(Box::new(move |event: salsa::Event| {
            if matches!(event.kind, salsa::EventKind::WillExecute { .. }) {
                probe.fetch_add(1, Ordering::Relaxed);
            }
        }))))
    }

    /// Debug-only: whether the availability-epoch singleton already exists, so a
    /// test can assert both constructors create it eagerly (killing the
    /// reader-side singleton-creation race, #292).
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    #[must_use = "the singleton-existence answer is the reason to call this"]
    pub fn debug_availability_epoch_exists(&self) -> bool {
        AvailabilityEpoch::try_get(self).is_some()
    }
}

/// The worker's decision for a snapshot read (#292): serve serially on the worker,
/// or hand a [`ReadSnapshot`] to a pool thread.
///
// The `Concurrent` payload is large (a cloned database handle), but the plan is
// created on the worker and immediately matched, then the snapshot is moved once
// into the read task — boxing would add an allocation on the common path for no
// benefit, so the size gap between the arms is deliberate.
#[allow(clippy::large_enum_variant)]
pub enum ConcurrentReadPlan {
    /// Serve on the worker: a miss (no entry), an evicted entry, or a tier-3
    /// provisional stale entry whose root is not cached.
    Serial,
    /// Serve off this snapshot on a pool thread.
    Concurrent(ReadSnapshot),
}

/// A per-request read handle a pool thread serves off the worker (#292).
///
/// Minted only by [`RootDatabase::plan_concurrent_read`]. `db` is a cloned Salsa
/// handle sharing the overlay, generation counter, and stamp registry; serving
/// runs the analysis query against it and drops it before returning, so the clone
/// never outlives the response.
///
/// `db` is declared **first** so its `Storage` clone drops first even if a later
/// field's `Drop` were to panic — the clone must always release, or a write's
/// outstanding-handle wait would hang.
pub struct ReadSnapshot {
    db: RootDatabase,
    input: EntryInput,
    dispatch_epoch: u64,
    mirror_generation: Option<u64>,
    /// A clone of the write-epoch source this read was dispatched under, held for
    /// the snapshot's lifetime so the source (and thus the reader-token
    /// registration below) outlives the read; not otherwise read.
    _source: AnalysisCancelSource,
    _registration: ReaderTokenRegistration,
}

/// The outcome of serving a [`ReadSnapshot`].
pub enum ReadServe {
    /// The analysis, plus whether serving re-executed the query (a stale recompute)
    /// rather than hitting the stored memo.
    Ready {
        analysis: Arc<FileAnalysis>,
        recomputed: bool,
    },
    /// The entry was evicted between plan and serve — defensively unreachable (an
    /// eviction's setter cannot complete while this clone lives), so the worker
    /// routes the request back for serial service.
    NotServable,
}

impl ReadSnapshot {
    /// The write epoch current when this snapshot was dispatched, used to guard the
    /// worker-side mirror store (see [`RootDatabase::apply_concurrent_read`]).
    #[must_use]
    pub fn dispatch_epoch(&self) -> u64 {
        self.dispatch_epoch
    }

    /// Serves this snapshot on the current (pool) thread, consuming it.
    ///
    /// The cloned `db` — and its `Storage` clone — drop when `self` drops at the end
    /// of this call, before the caller can send any response, so the read never
    /// holds a database clone across I/O. Zero Salsa writes happen here: `db.worker`
    /// is `None`, so any worker-only method would panic, and a memo hit (or a stale
    /// recompute) writes nothing.
    #[must_use = "the served analysis is the reason to serve"]
    pub fn serve(self) -> ReadServe {
        // Explicit early checkpoint so a snapshot minted just before a write, then
        // parked on the pool queue, unwinds at entry rather than after a full
        // compute.
        self.db.unwind_if_revision_cancelled();
        match analyze_entry(&self.db, self.input) {
            AnalysisResult::Computed(analysis) => {
                // A generation differing from the worker's last-known one for this
                // entry means this serve re-executed the query (a stale recompute)
                // rather than hitting the stored memo.
                let recomputed = self.mirror_generation != Some(analysis.generation());
                #[cfg(debug_assertions)]
                if recomputed {
                    let mut registry = self
                        .db
                        .live_analyses
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    registry.retain(|weak| weak.strong_count() > 0);
                    registry.push(Arc::downgrade(&analysis));
                }
                ReadServe::Ready {
                    analysis,
                    recomputed,
                }
            }
            AnalysisResult::Evicted => ReadServe::NotServable,
        }
    }
}

#[cfg(debug_assertions)]
impl Drop for ReadSnapshot {
    fn drop(&mut self) {
        debug_snapshots::on_drop();
    }
}

/// Debug-only live-snapshot counter, backing [`debug_live_snapshots`] and the
/// drop-before-I/O pin (#292): incremented at mint, decremented when a snapshot
/// drops (which is the moment its `Storage` clone releases).
#[cfg(debug_assertions)]
mod debug_snapshots {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static LIVE: AtomicUsize = AtomicUsize::new(0);

    pub(super) fn on_mint() {
        LIVE.fetch_add(1, Ordering::SeqCst);
    }

    pub(super) fn on_drop() {
        LIVE.fetch_sub(1, Ordering::SeqCst);
    }

    pub(super) fn live() -> usize {
        LIVE.load(Ordering::SeqCst)
    }
}

/// Debug-only: the number of live [`ReadSnapshot`]s (minted, not yet dropped), for
/// the drop-before-I/O pin (#292).
#[cfg(debug_assertions)]
#[doc(hidden)]
#[must_use = "the live-snapshot count is the reason to call this"]
pub fn debug_live_snapshots() -> usize {
    debug_snapshots::live()
}

/// Debug-only: how many times two distinct snapshot reads were simultaneously
/// inside the rendezvous seam — the deterministic parallelism witness (#292).
#[cfg(debug_assertions)]
#[doc(hidden)]
#[must_use = "the meets count is the reason to call this"]
pub fn debug_rendezvous_meets() -> u64 {
    test_seams::rendezvous_meets()
}

/// Debug-only: arm the rendezvous seam for snapshot reads whose path contains
/// `substr`, so a test can force two reads to overlap deterministically (#292).
#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn debug_arm_rendezvous(substr: &str) {
    test_seams::arm_rendezvous(substr);
}

/// Debug-only: disarm the rendezvous seam (see [`debug_arm_rendezvous`]).
#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn debug_disarm_rendezvous() {
    test_seams::disarm_rendezvous();
}

/// Debug-only: arm the gate seam for snapshot reads whose path contains `substr`,
/// so a test can hold reads in flight and interrupt them deterministically (#292).
#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn debug_arm_gate(substr: &str) {
    test_seams::arm_gate(substr);
}

/// Debug-only: release and disarm the gate seam.
#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn debug_disarm_gate() {
    test_seams::disarm_gate();
}

/// Debug-only: how many snapshot reads have entered the gate seam since it was
/// armed (#292).
#[cfg(debug_assertions)]
#[doc(hidden)]
#[must_use = "the gate-entered count is the reason to call this"]
pub fn debug_gate_entered() -> usize {
    test_seams::gate_entered()
}

/// Test-only seam: a bounded, checkpointed delay inside the tracked analysis
/// query, so an out-of-process test can hold an analysis in flight long enough
/// for a cancellation to land — deterministically, because every 25ms slice
/// first polls for cancellation and unwinds the moment one is pending. Armed only
/// via the environment (out-of-process e2e); at most 5s on the broken-cancellation
/// path, well under a 30s receive bound.
#[cfg(debug_assertions)]
mod test_seams {
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Mutex, PoisonError};
    use std::time::{Duration, Instant};

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
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// In-process gate seam: parks a snapshot read (a reader clone) in its compute
    /// until the test releases it or a cancellation fires, so the deterministic
    /// concurrency tests can hold a known number of reads in flight and then
    /// interrupt them (#292). Unlike the rendezvous seam it never auto-releases, so
    /// it holds an arbitrary number of readers at once. Reader-only (never blocks the
    /// worker), cancellation-polled every 25ms, bounded by a 5s escape.
    static GATE_ARM: Mutex<Option<String>> = Mutex::new(None);
    static GATE_ENTERED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    static GATE_RELEASE: AtomicBool = AtomicBool::new(false);

    #[doc(hidden)]
    pub fn arm_gate(substr: &str) {
        GATE_RELEASE.store(false, Ordering::SeqCst);
        GATE_ENTERED.store(0, Ordering::SeqCst);
        *GATE_ARM.lock().unwrap_or_else(PoisonError::into_inner) = Some(substr.to_owned());
    }

    #[doc(hidden)]
    pub fn disarm_gate() {
        // Release first so any parked reader leaves promptly, then clear the arm.
        GATE_RELEASE.store(true, Ordering::SeqCst);
        *GATE_ARM.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    #[doc(hidden)]
    #[must_use]
    pub fn gate_entered() -> usize {
        GATE_ENTERED.load(Ordering::SeqCst)
    }

    pub(crate) fn gate_seam(db: &dyn super::IdeDatabase, path: &std::path::Path) {
        if !db.debug_is_reader() {
            return;
        }
        let armed = GATE_ARM
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let Some(substr) = armed else {
            return;
        };
        if substr.is_empty() || !path.to_string_lossy().contains(&substr) {
            return;
        }
        GATE_ENTERED.fetch_add(1, Ordering::SeqCst);
        let deadline = Instant::now() + RENDEZVOUS_ESCAPE;
        loop {
            super::Database::unwind_if_revision_cancelled(db);
            if GATE_RELEASE.load(Ordering::SeqCst) || Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// Environment arm for the rendezvous seam (symmetry with the slow seam); the
    /// in-process arm below is what the deterministic ide-db test uses, since
    /// setting an environment variable from one test thread would race the others.
    pub(crate) const RENDEZVOUS_ENV: &str = "INFERENCE_IDE_TEST_RENDEZVOUS_PATH_SUBSTR";

    /// Process-global in-process arm: the substring a snapshot read's path must
    /// contain to rendezvous. A `Mutex<Option<..>>` (not a thread-local) so the
    /// arm is visible on the pool threads that run the reads.
    static RENDEZVOUS_ARM: Mutex<Option<String>> = Mutex::new(None);
    /// The distinct matching paths currently parked in the seam.
    static RENDEZVOUS_INSIDE: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
    /// Incremented once each time two distinct paths are simultaneously inside.
    static RENDEZVOUS_MEETS: AtomicU64 = AtomicU64::new(0);
    /// Latched when simultaneity is first observed, so a straggler leaves promptly
    /// instead of parking to the escape bound; reset when the seam empties.
    static RENDEZVOUS_LATCH: AtomicBool = AtomicBool::new(false);

    /// The escape bound: a read parks at most this long waiting for a sibling
    /// before proceeding alone, so a mis-set fixture can never hang the suite.
    const RENDEZVOUS_ESCAPE: Duration = Duration::from_secs(5);

    #[doc(hidden)]
    pub fn arm_rendezvous(substr: &str) {
        *RENDEZVOUS_ARM.lock().unwrap_or_else(PoisonError::into_inner) = Some(substr.to_owned());
    }

    #[doc(hidden)]
    pub fn disarm_rendezvous() {
        *RENDEZVOUS_ARM.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    #[doc(hidden)]
    #[must_use]
    pub fn rendezvous_meets() -> u64 {
        RENDEZVOUS_MEETS.load(Ordering::SeqCst)
    }

    fn rendezvous_substr(path: &std::path::Path) -> Option<String> {
        let armed = RENDEZVOUS_ARM
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .or_else(|| std::env::var(RENDEZVOUS_ENV).ok())?;
        let display = path.to_string_lossy();
        (!armed.is_empty() && display.contains(&armed)).then(|| display.into_owned())
    }

    /// Parks a snapshot read until a second distinct matching path is inside too,
    /// recording the simultaneity — the deterministic witness that two reads run
    /// in parallel (#292). Fires only on a reader clone, so the worker never parks.
    pub(crate) fn rendezvous_seam(db: &dyn super::IdeDatabase, path: &std::path::Path) {
        if !db.debug_is_reader() {
            return;
        }
        let Some(key) = rendezvous_substr(path) else {
            return;
        };
        RENDEZVOUS_INSIDE
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.clone());
        let deadline = Instant::now() + RENDEZVOUS_ESCAPE;
        loop {
            super::Database::unwind_if_revision_cancelled(db);
            let distinct = RENDEZVOUS_INSIDE
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .len();
            if distinct >= 2 {
                if !RENDEZVOUS_LATCH.swap(true, Ordering::SeqCst) {
                    RENDEZVOUS_MEETS.fetch_add(1, Ordering::SeqCst);
                }
                break;
            }
            if RENDEZVOUS_LATCH.load(Ordering::SeqCst) || Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let mut inside = RENDEZVOUS_INSIDE
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        inside.remove(&key);
        if inside.is_empty() {
            RENDEZVOUS_LATCH.store(false, Ordering::SeqCst);
        }
    }
}

const _: () = {
    const fn assert_send<T: Send>() {}
    assert_send::<RootDatabase>();
};
