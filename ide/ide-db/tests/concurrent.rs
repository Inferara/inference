//! Concurrent snapshot reads (#292): the worker mints a [`ReadSnapshot`], a second
//! thread serves it off a cloned database handle, and the result folds back — with
//! the eligibility routing, the cross-thread memo sharing, the epoch/overlay
//! guards, and the write-quiesces-readers cancellation all pinned here.
//!
//! The gated tests hold a reader in flight with the in-process gate seam (bounded
//! by a 5s escape, cancellation-polled every 25ms) and interrupt it deterministically
//! — never with a wall-clock assertion. A gate fires only on a *recompute*, so those
//! fixtures use a stale entry under a cached (manifest) source root.
//!
//! The debug seam counters (live snapshots, rendezvous meets, gate entries) are
//! process-global, so every test in this file holds a shared serial lock: the
//! deterministic counter assertions require that no two tests mint or serve
//! simultaneously.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use inference_ide_db::{
    AnalysisCancelSource, ConcurrentReadPlan, FileAnalysis, MAX_UNOPENED_ANALYSES, ReadServe,
    ReadSnapshot, RootDatabase, is_cancellation,
};

/// Serializes the whole file: the debug counters are process-global, so the
/// deterministic assertions need one test minting/serving at a time.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial_guard() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// A throwaway source tree under the system temp dir, removed on drop.
struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "inference-ide-db-concurrent-{tag}-{}-{nanos}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create temp tree root");
        TempTree { root }
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let dest = self.root.join(relative);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).expect("create source parent dir");
        }
        std::fs::write(&dest, contents).expect("write source file");
        dest
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A minimal valid `Inference.toml` (a `[package]` with a name and version), enough
/// for manifest discovery to cache a tier-1 source root — which is what makes a
/// stale entry pool-eligible.
const MANIFEST: &str = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n";

/// Spins until `cond` holds or the 5s bound elapses, so a gated test waits for a
/// reader to be parked without a wall-clock assertion.
fn wait_until(mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !cond() {
        assert!(
            Instant::now() < deadline,
            "condition not met within the bound"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn serve_on_thread(snapshot: ReadSnapshot) -> ReadServe {
    std::thread::spawn(move || snapshot.serve())
        .join()
        .expect("serve thread joins")
}

/// The entry's own source text, for a post-write content check.
fn entry_source(db: &mut RootDatabase, path: &std::path::Path) -> String {
    db.analysis(path)
        .file(&[])
        .expect("entry closure file")
        .source()
        .to_owned()
}

// --- Routing (release-safe: no debug seams) ----------------------------------

#[test]
fn plan_concurrent_read_routing_table() {
    let _serial = serial_guard();
    let tree = TempTree::new("routing");
    tree.write("Inference.toml", MANIFEST);
    let main = tree.write(
        "src/main.inf",
        "use lib;\nfn main() -> i32 { return lib::v(); }",
    );
    let lib = tree.write("src/lib.inf", "pub fn v() -> i32 { return 1; }");
    let mut db = RootDatabase::default();
    let source = AnalysisCancelSource::detached();

    // A path with no entry: Serial.
    assert!(
        matches!(
            db.plan_concurrent_read(&tree.path("src/absent.inf"), &source),
            ConcurrentReadPlan::Serial
        ),
        "a missing entry routes serial"
    );

    db.open_document(&main, "use lib;\nfn main() -> i32 { return lib::v(); }");
    db.open_document(&lib, "pub fn v() -> i32 { return 1; }");
    let _ = db.analysis(&main).generation();

    // A memoized (hit) entry: Concurrent.
    assert!(
        matches!(
            db.plan_concurrent_read(&main, &source),
            ConcurrentReadPlan::Concurrent(_)
        ),
        "a mirror hit routes concurrent"
    );

    // Stale under a cached (manifest) root: still Concurrent.
    db.change_document(&lib, "pub fn v() -> i32 { return 2; }");
    assert!(!db.is_analyzed(&main), "the lib change staled main");
    assert!(
        matches!(
            db.plan_concurrent_read(&main, &source),
            ConcurrentReadPlan::Concurrent(_)
        ),
        "a stale entry under a cached root routes concurrent"
    );
}

#[test]
fn plan_concurrent_read_stale_uncached_root_is_serial() {
    // A bare (project-less) file resolves its root by the own-directory tier, which
    // is deliberately not cached — so once staled it must recompute serially, to keep
    // the donor/manifest upgrade path exactly as it is today.
    let _serial = serial_guard();
    let tree = TempTree::new("routing-uncached");
    let bare = tree.write("bare.inf", "fn main() -> i32 { return 1; }");
    let mut db = RootDatabase::default();
    let source = AnalysisCancelSource::detached();

    db.open_document(&bare, "fn main() -> i32 { return 1; }");
    let _ = db.analysis(&bare).generation();
    // A hit is still concurrent even for a bare file.
    assert!(matches!(
        db.plan_concurrent_read(&bare, &source),
        ConcurrentReadPlan::Concurrent(_)
    ));
    // Stale it: with no cached root, it must route serial.
    db.change_document(&bare, "fn main() -> i32 { return 2; }");
    assert!(!db.is_analyzed(&bare));
    assert!(
        matches!(
            db.plan_concurrent_read(&bare, &source),
            ConcurrentReadPlan::Serial
        ),
        "a stale entry with an uncached (tier-3) root routes serial"
    );
}

#[test]
fn plan_concurrent_read_evicted_entry_is_serial() {
    let _serial = serial_guard();
    let tree = TempTree::new("routing-evicted");
    let doc = tree.write("doc.inf", "fn main() -> i32 { return 1; }");
    let mut db = RootDatabase::default();
    let source = AnalysisCancelSource::detached();

    db.open_document(&doc, "fn main() -> i32 { return 1; }");
    let _ = db.analysis(&doc).generation();
    // Closing evicts the entry; a plan for it must route serial (the recompute is
    // worker-only).
    db.close_document(&doc);
    assert!(
        matches!(
            db.plan_concurrent_read(&doc, &source),
            ConcurrentReadPlan::Serial
        ),
        "an evicted entry routes serial"
    );
}

#[test]
fn apply_concurrent_read_epoch_guard_leaves_mirror_none() {
    // A write between dispatch and apply bumps the source epoch past the snapshot's,
    // so the fold-back is skipped: the mirror stays None and the next worker analysis
    // recomputes (a strictly-greater generation than the pre-stale one), with the
    // alignment tripwire never firing. The worker is left unbound so bumping the
    // epoch does not fire its cancellation token.
    let _serial = serial_guard();
    let tree = TempTree::new("epoch-guard");
    tree.write("Inference.toml", MANIFEST);
    let main = tree.write(
        "src/main.inf",
        "use lib;\nfn main() -> i32 { return lib::v(); }",
    );
    let lib = tree.write("src/lib.inf", "pub fn v() -> i32 { return 1; }");
    let mut db = RootDatabase::default();
    let source = AnalysisCancelSource::detached();

    db.open_document(&main, "use lib;\nfn main() -> i32 { return lib::v(); }");
    db.open_document(&lib, "pub fn v() -> i32 { return 1; }");
    let original_gen = db.analysis(&main).generation();
    db.change_document(&lib, "pub fn v() -> i32 { return 2; }");
    assert!(!db.is_analyzed(&main));

    let ConcurrentReadPlan::Concurrent(snapshot) = db.plan_concurrent_read(&main, &source) else {
        panic!("stale + cached root must be concurrent");
    };
    let dispatch_epoch = snapshot.dispatch_epoch();
    let ReadServe::Ready { analysis, .. } = serve_on_thread(snapshot) else {
        panic!("must serve Ready");
    };

    // A write superseded the read before its result folds back (the reader is already
    // deregistered, so no token fires).
    let _ = source.request_cancellation();
    db.apply_concurrent_read(&main, &analysis, dispatch_epoch, &source);
    assert!(
        !db.is_analyzed(&main),
        "a superseded fold-back must not store the mirror"
    );

    let after = db.analysis(&main).generation();
    assert!(
        after > original_gen,
        "the recompute mints a fresh generation ({original_gen} -> {after})"
    );
}

#[test]
fn donor_upgrade_stays_serial() {
    // A file first resolved by the own-directory tier keeps an uncached root, so a
    // stale recompute routes serial — the reason tier-3 recomputes stay serial, so a
    // later donor/manifest upgrade lands exactly as it does today.
    let _serial = serial_guard();
    let tree = TempTree::new("donor-upgrade");
    let lib = tree.write("proj/src/lib.inf", "pub fn v() -> i32 { return 1; }");
    let mut db = RootDatabase::default();
    let source = AnalysisCancelSource::detached();

    db.open_document(&lib, "pub fn v() -> i32 { return 1; }");
    let _ = db.analysis(&lib).generation();
    db.change_document(&lib, "pub fn v() -> i32 { return 3; }");
    assert!(
        matches!(
            db.plan_concurrent_read(&lib, &source),
            ConcurrentReadPlan::Serial
        ),
        "an uncached-root stale entry routes serial so a root upgrade can still land"
    );
    assert!(db.analysis(&lib).generation() > 0, "the serial recompute succeeds");
}

// --- Debug-only: cross-thread memo sharing, guards, cancellation -------------

#[cfg(debug_assertions)]
mod gated {
    use super::*;
    use inference_ide_db::{
        RootDatabase, debug_arm_gate, debug_arm_rendezvous, debug_disarm_gate,
        debug_disarm_rendezvous, debug_gate_entered, debug_live_snapshots, debug_rendezvous_meets,
    };
    use std::sync::atomic::AtomicUsize;

    fn probed() -> (Arc<AtomicUsize>, RootDatabase) {
        let probe = Arc::new(AtomicUsize::new(0));
        let db = RootDatabase::with_execute_probe(Arc::clone(&probe));
        (probe, db)
    }

    /// Opens a manifested `main` importing `lib`, memoizes `main` (caching its root),
    /// then stales it by changing `lib` — leaving `main` stale under a cached root,
    /// so a plan is `Concurrent` and a serve recomputes (hitting the gate seam).
    fn stale_cached_main(db: &mut RootDatabase, tree: &TempTree) -> PathBuf {
        tree.write("Inference.toml", MANIFEST);
        let main = tree.write(
            "src/main.inf",
            "use lib;\nfn main() -> i32 { return lib::v(); }",
        );
        let lib = tree.write("src/lib.inf", "pub fn v() -> i32 { return 1; }");
        db.open_document(&main, "use lib;\nfn main() -> i32 { return lib::v(); }");
        db.open_document(&lib, "pub fn v() -> i32 { return 1; }");
        let _ = db.analysis(&main).generation();
        db.change_document(&lib, "pub fn v() -> i32 { return 2; }");
        assert!(!db.is_analyzed(&main));
        main
    }

    #[test]
    fn a_hit_serves_the_mirror_arc_with_zero_executions() {
        let _serial = serial_guard();
        let tree = TempTree::new("hit-serve");
        let doc = tree.write("doc.inf", "fn main() -> i32 { return 1; }");
        let (probe, mut db) = probed();
        let source = AnalysisCancelSource::detached();

        db.open_document(&doc, "fn main() -> i32 { return 1; }");
        let worker_gen = db.analysis(&doc).generation();
        let worker_ptr = db.analysis(&doc) as *const FileAnalysis;
        let before = probe.load(Ordering::SeqCst);

        let ConcurrentReadPlan::Concurrent(snapshot) = db.plan_concurrent_read(&doc, &source) else {
            panic!("a hit routes concurrent");
        };
        let ReadServe::Ready {
            analysis,
            recomputed,
        } = serve_on_thread(snapshot)
        else {
            panic!("a hit serves Ready");
        };

        assert!(!recomputed, "a hit is not a recompute");
        assert_eq!(analysis.generation(), worker_gen, "same memoized generation");
        assert_eq!(
            Arc::as_ptr(&analysis),
            worker_ptr,
            "the served Arc is the mirror's Arc"
        );
        assert_eq!(
            probe.load(Ordering::SeqCst),
            before,
            "a hit fires zero WillExecute"
        );
    }

    #[test]
    fn a_stale_recompute_executes_on_the_reader_and_the_worker_hits_it() {
        let _serial = serial_guard();
        let tree = TempTree::new("stale-recompute");
        let (probe, mut db) = probed();
        let source = AnalysisCancelSource::detached();
        let main = stale_cached_main(&mut db, &tree);

        let executes_before = probe.load(Ordering::SeqCst);
        let ConcurrentReadPlan::Concurrent(snapshot) = db.plan_concurrent_read(&main, &source)
        else {
            panic!("stale + cached root must be concurrent");
        };
        let dispatch_epoch = snapshot.dispatch_epoch();
        let ReadServe::Ready {
            analysis,
            recomputed,
        } = serve_on_thread(snapshot)
        else {
            panic!("must serve Ready");
        };
        assert!(recomputed, "a stale entry recomputes");
        let reader_gen = analysis.generation();
        assert_eq!(
            probe.load(Ordering::SeqCst),
            executes_before + 1,
            "the reader executed exactly once"
        );

        // Fold back, then the worker's next fetch memo-hits the reader-inserted memo.
        db.apply_concurrent_read(&main, &analysis, dispatch_epoch, &source);
        let worker_gen = db.analysis(&main).generation();
        assert_eq!(
            worker_gen, reader_gen,
            "the worker fetch hits the reader-inserted memo"
        );
        assert_eq!(
            probe.load(Ordering::SeqCst),
            executes_before + 1,
            "the worker fetch executes nothing"
        );
    }

    #[test]
    fn both_constructors_create_the_availability_epoch_eagerly() {
        let _serial = serial_guard();
        let db = RootDatabase::default();
        assert!(
            db.debug_availability_epoch_exists(),
            "Default creates the singleton eagerly"
        );
        let probe = Arc::new(AtomicUsize::new(0));
        let probed = RootDatabase::with_execute_probe(probe);
        assert!(
            probed.debug_availability_epoch_exists(),
            "with_execute_probe creates the singleton eagerly"
        );
    }

    #[test]
    fn the_live_snapshot_counter_returns_to_zero_after_serve() {
        let _serial = serial_guard();
        let tree = TempTree::new("live-snapshots");
        let doc = tree.write("doc.inf", "fn main() -> i32 { return 1; }");
        let mut db = RootDatabase::default();
        let source = AnalysisCancelSource::detached();
        db.open_document(&doc, "fn main() -> i32 { return 1; }");
        let _ = db.analysis(&doc).generation();

        assert_eq!(debug_live_snapshots(), 0, "no snapshots before minting");
        let ConcurrentReadPlan::Concurrent(snapshot) = db.plan_concurrent_read(&doc, &source) else {
            panic!("a hit routes concurrent");
        };
        assert_eq!(debug_live_snapshots(), 1, "one snapshot after minting");
        let _ = snapshot.serve();
        assert_eq!(
            debug_live_snapshots(),
            0,
            "the snapshot drops when serve returns (before any I/O)"
        );
    }

    #[test]
    fn a_reader_token_fire_unwinds_the_gate_held_compute_and_the_worker_recovers() {
        let _serial = serial_guard();
        let tree = TempTree::new("reader-token");
        let mut db = RootDatabase::default();
        // The worker is left unbound so request_cancellation fires only the reader
        // token, not the worker's own — keeping the worker's later fetch clean.
        let source = AnalysisCancelSource::detached();
        let main = stale_cached_main(&mut db, &tree);

        debug_arm_gate("main.inf");
        let ConcurrentReadPlan::Concurrent(snapshot) = db.plan_concurrent_read(&main, &source)
        else {
            panic!("must be concurrent");
        };
        let reader = std::thread::spawn(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| snapshot.serve()))
        });
        wait_until(|| debug_gate_entered() >= 1);

        // Fire cancellation: the reader's registered token unwinds its compute.
        let _ = source.request_cancellation();
        let payload = reader
            .join()
            .expect("reader thread joins")
            .err()
            .expect("the reader must unwind");
        assert!(
            is_cancellation(payload.as_ref()),
            "the reader unwinds with a cancellation payload"
        );
        debug_disarm_gate();

        // The worker recovers the key: a fresh serial analysis succeeds.
        assert!(db.analysis(&main).generation() > 0);
    }

    #[test]
    fn a_write_during_a_gate_held_reader_unwinds_it_then_applies() {
        // Acceptance bullet 2 at the library layer: a change_document during a
        // gate-held reader serve unwinds the reader cancelled (the setter's own
        // quiesce), the setter completes, and a post-write read sees the new content.
        let _serial = serial_guard();
        let tree = TempTree::new("write-during-read");
        let mut db = RootDatabase::default();
        let source = AnalysisCancelSource::detached();
        let main = stale_cached_main(&mut db, &tree);

        debug_arm_gate("main.inf");
        let ConcurrentReadPlan::Concurrent(snapshot) = db.plan_concurrent_read(&main, &source)
        else {
            panic!("must be concurrent");
        };
        let reader = std::thread::spawn(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| snapshot.serve()))
        });
        wait_until(|| debug_gate_entered() >= 1);

        // A write during the gate-held reader: its setter blocks until the reader
        // unwinds and drops, then applies the new overlay.
        db.change_document(&main, "use lib;\nfn main() -> i32 { return 42; }");
        let payload = reader
            .join()
            .expect("reader thread joins")
            .err()
            .expect("the reader must unwind");
        assert!(is_cancellation(payload.as_ref()));
        debug_disarm_gate();

        assert!(
            entry_source(&mut db, &main).contains("42"),
            "the post-write read sees the new content"
        );
    }

    #[test]
    fn three_gate_held_readers_and_a_write_all_unwind_and_apply() {
        // Acceptance bullet 3 / no-deadlock at N above the pool size: three reader
        // snapshots held in the gate, then a write — all three unwind, the write
        // applies, and every thread joins within the bound.
        let _serial = serial_guard();
        let tree = TempTree::new("n3-readers");
        tree.write("Inference.toml", MANIFEST);
        let shared = tree.write("src/shared.inf", "pub fn v() -> i32 { return 1; }");
        let mut db = RootDatabase::default();
        let source = AnalysisCancelSource::detached();
        db.open_document(&shared, "pub fn v() -> i32 { return 1; }");

        let mut docs = Vec::new();
        for i in 0..3 {
            let name = format!("src/f{i}.inf");
            let src = "use shared;\nfn main() -> i32 { return shared::v(); }";
            let path = tree.write(&name, src);
            db.open_document(&path, src);
            let _ = db.analysis(&path).generation();
            docs.push(path);
        }
        // One change to the shared import stales all three under their cached roots.
        db.change_document(&shared, "pub fn v() -> i32 { return 2; }");
        for path in &docs {
            assert!(!db.is_analyzed(path), "each dependent staled");
        }

        debug_arm_gate("src/f");
        let mut readers = Vec::new();
        for path in &docs {
            let ConcurrentReadPlan::Concurrent(snapshot) = db.plan_concurrent_read(path, &source)
            else {
                panic!("stale + cached routes concurrent");
            };
            readers.push(std::thread::spawn(move || {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| snapshot.serve()))
            }));
        }
        wait_until(|| debug_gate_entered() >= 3);

        // A write quiesces every reader: its setter waits for all three to drop.
        db.change_document(&docs[0], "use shared;\nfn main() -> i32 { return 9; }");
        for reader in readers {
            let payload = reader
                .join()
                .expect("reader thread joins")
                .err()
                .expect("each reader unwinds");
            assert!(is_cancellation(payload.as_ref()));
        }
        debug_disarm_gate();

        assert!(
            entry_source(&mut db, &docs[0]).contains('9'),
            "the write applied and the worker keeps serving"
        );
    }

    #[test]
    fn the_rendezvous_seam_witnesses_two_reads_overlapping() {
        // Two distinct-path stale reads served simultaneously record a meet; a read
        // served alone (disarmed) records none.
        let _serial = serial_guard();
        let tree = TempTree::new("rendezvous");
        tree.write("Inference.toml", MANIFEST);
        let shared = tree.write("src/shared.inf", "pub fn v() -> i32 { return 1; }");
        let a = tree.write("src/a.inf", "use shared;\nfn main() -> i32 { return shared::v(); }");
        let b = tree.write("src/b.inf", "use shared;\nfn main() -> i32 { return shared::v(); }");
        let mut db = RootDatabase::default();
        let source = AnalysisCancelSource::detached();
        db.open_document(&shared, "pub fn v() -> i32 { return 1; }");
        db.open_document(&a, "use shared;\nfn main() -> i32 { return shared::v(); }");
        db.open_document(&b, "use shared;\nfn main() -> i32 { return shared::v(); }");
        let _ = db.analysis(&a).generation();
        let _ = db.analysis(&b).generation();

        let control = debug_rendezvous_meets();
        // Disarmed control: a lone serve records no meet.
        db.change_document(&shared, "pub fn v() -> i32 { return 2; }");
        let ConcurrentReadPlan::Concurrent(snap_a) = db.plan_concurrent_read(&a, &source) else {
            panic!("concurrent");
        };
        let _ = serve_on_thread(snap_a);
        db.apply_unopened_read_bookkeeping(&a); // no-op (a is open); keeps flow simple
        assert_eq!(
            debug_rendezvous_meets(),
            control,
            "a serialized serve records no meet"
        );

        // Armed: two reads for distinct stale paths overlap and record a meet.
        db.change_document(&shared, "pub fn v() -> i32 { return 3; }");
        debug_arm_rendezvous("src/");
        let ConcurrentReadPlan::Concurrent(snap_a) = db.plan_concurrent_read(&a, &source) else {
            panic!("concurrent");
        };
        let ConcurrentReadPlan::Concurrent(snap_b) = db.plan_concurrent_read(&b, &source) else {
            panic!("concurrent");
        };
        let ra = std::thread::spawn(move || snap_a.serve());
        let rb = std::thread::spawn(move || snap_b.serve());
        let _ = ra.join().expect("a joins");
        let _ = rb.join().expect("b joins");
        debug_disarm_rendezvous();
        assert!(
            debug_rendezvous_meets() > control,
            "two overlapping reads record at least one meet"
        );
    }

    #[test]
    fn the_never_opened_cap_holds_through_the_concurrent_path() {
        // Never-opened manifested files are worker-first-analysed (cap-counted); a
        // concurrent recompute plus deferred bookkeeping keeps the resident bound at
        // MAX_UNOPENED_ANALYSES and frees the evicted memos.
        let _serial = serial_guard();
        let tree = TempTree::new("cap-concurrent");
        tree.write("Inference.toml", MANIFEST);
        let mut db = RootDatabase::default();
        let source = AnalysisCancelSource::detached();

        let mut paths = Vec::new();
        for i in 0..(MAX_UNOPENED_ANALYSES + 2) {
            let path = tree.write(
                &format!("src/f{i}.inf"),
                &format!("fn f{i}() -> i32 {{ return {i}; }}"),
            );
            let _ = db.analysis(&path).generation();
            paths.push(path);
        }

        // A recently analysed never-opened file is a hit → concurrent path; serving,
        // folding back, and running the deferred bookkeeping keeps the bound.
        let recent = &paths[MAX_UNOPENED_ANALYSES + 1];
        let ConcurrentReadPlan::Concurrent(snapshot) = db.plan_concurrent_read(recent, &source)
        else {
            panic!("a hit routes concurrent");
        };
        let dispatch_epoch = snapshot.dispatch_epoch();
        if let ReadServe::Ready { analysis, .. } = serve_on_thread(snapshot) {
            db.apply_concurrent_read(recent, &analysis, dispatch_epoch, &source);
        }
        db.apply_unopened_read_bookkeeping(recent);

        let live = db.debug_live_analyses();
        assert!(
            live <= MAX_UNOPENED_ANALYSES + 1,
            "resident analyses stay bounded through the concurrent path (got {live})"
        );
    }

    #[test]
    fn apply_unopened_bookkeeping_skips_an_opened_document() {
        // A didOpen between serve and apply must not enroll an open document in the
        // never-opened FIFO: the apply re-checks the overlay first.
        let _serial = serial_guard();
        let tree = TempTree::new("unopened-recheck");
        let doc = tree.write("doc.inf", "fn main() -> i32 { return 1; }");
        let mut db = RootDatabase::default();

        // Analyse it never-opened first (enrolls it in the cap FIFO).
        let _ = db.analysis(&doc).generation();
        // A didOpen arrives (now it has an overlay).
        db.open_document(&doc, "fn main() -> i32 { return 2; }");
        let _ = db.analysis(&doc).generation();
        // Applying the deferred bookkeeping must leave the open document analysed and
        // never evict it.
        db.apply_unopened_read_bookkeeping(&doc);
        assert!(
            db.is_analyzed(&doc),
            "an open document is never enrolled in the never-opened cap"
        );
    }
}
