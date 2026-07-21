//! Integration tests for the `RootDatabase` → `FileAnalysis` pipeline: closure
//! loading through the overlay-then-disk loader, closure-aware invalidation, and
//! the partial results a broken program still yields.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use inference_ide_db::{
    AnalysisCancelSource, FileAnalysis, NodeId, RootDatabase, Severity, is_cancellation,
};

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
            "inference-ide-db-{tag}-{}-{nanos}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create temp tree root");
        TempTree { root }
    }

    /// Writes `contents` to `<root>/<relative>`, creating parent directories, and
    /// returns the absolute path.
    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let dest = self.root.join(relative);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).expect("create source parent dir");
        }
        std::fs::write(&dest, contents).expect("write source file");
        dest
    }

    /// Writes raw `bytes` to `<root>/<relative>`, creating parent directories, and
    /// returns the absolute path. Planting invalid UTF-8 makes the file exist for
    /// an `is_file` probe yet fail `read_to_string` deterministically on every
    /// platform — a reachable-but-unreadable import.
    fn write_bytes(&self, relative: &str, bytes: &[u8]) -> PathBuf {
        let dest = self.root.join(relative);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).expect("create source parent dir");
        }
        std::fs::write(&dest, bytes).expect("write source bytes");
        dest
    }

    /// The absolute path a relative source name would occupy, without writing it.
    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Whether the analysis's merged arena contains a closure file at `module_path`
/// that defines a top-level item named `name`. Used to assert that an imported
/// file's symbols are present (or absent) in an analysis without re-querying the
/// database while a borrow is live.
fn closure_defines(analysis: &FileAnalysis, module_path: &[String], name: &str) -> bool {
    let arena = analysis.arena();
    arena
        .source_files()
        .filter(|sf| sf.module_path == module_path)
        .any(|sf| sf.defs.iter().any(|&d| arena.def_name(d) == name))
}

/// The definition names of the closure file named by `module_path`, in order.
fn def_names(db: &mut RootDatabase, entry: &Path, module_path: &[String]) -> Vec<String> {
    let analysis = db.analysis(entry);
    let arena = analysis.arena();
    arena
        .source_files()
        .find(|sf| sf.module_path == module_path)
        .map(|sf| {
            sf.defs
                .iter()
                .map(|&d| arena.def_name(d).to_string())
                .collect()
        })
        .unwrap_or_default()
}

// Closure loading

#[test]
fn overlay_text_beats_disk_contents() {
    let tree = TempTree::new("overlay-wins");
    let entry = tree.write("main.inf", "pub fn disk_fn() {}");
    let mut db = RootDatabase::default();

    // Open the same file with different in-memory text; analysis must use it.
    db.open_document(&entry, "pub fn overlay_fn() {}");

    assert_eq!(def_names(&mut db, &entry, &[]), vec!["overlay_fn"]);
}

#[test]
fn import_is_resolved_from_disk() {
    let tree = TempTree::new("import-disk");
    let entry = tree.write(
        "main.inf",
        "use lib::helper;\npub fn main() -> i32 { return 0; }",
    );
    let helper = tree.write("lib/helper.inf", "pub fn help() -> i32 { return 7; }");
    let mut db = RootDatabase::default();

    let lib_mod = vec!["lib".to_string(), "helper".to_string()];
    let analysis = db.analysis(&entry);
    // The imported file is in the closure, with its path recovered.
    let file = analysis.file(&lib_mod).expect("imported file in closure");
    assert_eq!(file.path(), helper.as_path());
    assert!(analysis.source_file_id(&lib_mod).is_some());
}

#[test]
fn closure_maps_each_imported_file_to_its_source_line_index_and_a_hittable_arena() {
    // Cross-file features need the target file's own source text, line index, and
    // arena file id — a goto-def into an import must resolve against the imported
    // file, not the one the request came from. This checks the full round-trip.
    let tree = TempTree::new("cross-file-map");
    let entry = tree.write(
        "main.inf",
        "use lib::helper;\npub fn main() -> i32 { return 0; }",
    );
    // Two lines so the line index has real work to do.
    let helper_src = "pub fn help() -> i32 {\n    return 7;\n}";
    tree.write("lib/helper.inf", helper_src);
    let mut db = RootDatabase::default();

    let lib_mod = vec!["lib".to_string(), "helper".to_string()];
    let analysis = db.analysis(&entry);
    let file = analysis.file(&lib_mod).expect("imported file in closure");

    // The imported file's own source text is retained verbatim.
    assert_eq!(file.source(), helper_src);

    // Its line index converts an offset within the imported file correctly: the
    // `return` on line 1 (0-based) starts after the first newline.
    let return_offset = helper_src.find("return").unwrap() as u32;
    let position = file.line_index().line_col(return_offset);
    assert_eq!(position.line, 1);
    assert_eq!(position.character, 4);

    // Hit-testing the imported file's own arena id finds a node inside it, using
    // the imported file's per-file-local offset (not the entry's).
    let lib_file = analysis
        .source_file_id(&lib_mod)
        .expect("imported file has an arena id");
    let help_offset = helper_src.find("help").unwrap() as u32;
    let hit = analysis
        .hit_test(lib_file, help_offset)
        .expect("a node covers `help` in the imported file");
    if let NodeId::Ident(ident) = hit.node {
        assert_eq!(analysis.arena().ident_name(ident), "help");
    } else {
        panic!("expected the `help` identifier, got {:?}", hit.node);
    }
}

#[test]
fn typed_context_answers_for_a_node_in_an_imported_file() {
    // Cross-file identity has historically hidden bugs (#63): confirm the merged
    // type context answers `get_node_typeinfo` for an expression that lives in an
    // imported file, addressed by that file's own per-file-local offset.
    let tree = TempTree::new("cross-file-typeinfo");
    let entry = tree.write(
        "main.inf",
        "use lib::helper;\npub fn main() -> i32 { return 0; }",
    );
    let helper_src = "pub fn help() -> i32 { return 7; }";
    tree.write("lib/helper.inf", helper_src);
    let mut db = RootDatabase::default();

    let lib_mod = vec!["lib".to_string(), "helper".to_string()];
    let analysis = db.analysis(&entry);
    let lib_file = analysis
        .source_file_id(&lib_mod)
        .expect("imported file has an arena id");

    // The `7` literal in the imported file is typed against its `i32` return.
    let literal_offset = helper_src.find('7').unwrap() as u32;
    let hit = analysis
        .hit_test(lib_file, literal_offset)
        .expect("a node covers the literal in the imported file");
    assert!(
        analysis
            .typed_context()
            .get_node_typeinfo(hit.node)
            .is_some(),
        "the type context must answer for the imported file's literal"
    );
}

#[test]
fn missing_import_is_recorded_with_location_and_module_path() {
    let tree = TempTree::new("missing-import");
    // A header line pushes the `use` directive off byte 0 and onto line 2, so the
    // recorded location is distinguishable from a dropped `Location::default()`
    // (which is all zeros). A byte-0 fixture cannot tell the two apart.
    let src = "// header\nuse nope;\npub fn main() {}";
    let entry = tree.write("main.inf", src);
    let mut db = RootDatabase::default();

    let analysis = db.analysis(&entry);
    assert_eq!(analysis.import_problems().len(), 1);
    let problem = &analysis.import_problems()[0];
    assert_eq!(problem.referenced_as, "nope");
    assert!(problem.importing_module_path.is_empty());

    // The location spans exactly the `use nope;` directive, at a nonzero offset on
    // the second line — not the origin a lost location would default to.
    let use_start = src
        .find("use nope;")
        .expect("fixture contains the directive") as u32;
    assert!(use_start > 0, "the directive must not start at byte 0");
    assert_eq!(problem.location.offset_start, use_start);
    assert_eq!(
        problem.location.offset_end,
        use_start + "use nope;".len() as u32
    );
    assert_eq!(problem.location.start_line, 2);
    assert_eq!(problem.location.start_column, 1);
}

#[test]
fn broken_imported_file_yields_labeled_parse_errors_and_entry_still_analyzed() {
    let tree = TempTree::new("broken-import");
    let entry = tree.write(
        "main.inf",
        "use broken;\npub fn main() -> i32 { return 0; }",
    );
    tree.write("broken.inf", "pub fn oops( { return 1; }");
    let mut db = RootDatabase::default();

    let analysis = db.analysis(&entry);
    // The broken file's syntax errors are collected, labeled with its module.
    assert_eq!(analysis.parse_errors().len(), 1);
    assert_eq!(
        analysis.parse_errors()[0].module_path,
        vec!["broken".to_string()]
    );
    assert!(!analysis.parse_errors()[0].errors.is_empty());

    // The entry is still fully analyzed.
    assert_eq!(def_names(&mut db, &entry, &[]), vec!["main"]);
}

#[test]
fn use_root_handle_is_not_a_missing_import() {
    let tree = TempTree::new("root-handle");
    let entry = tree.write("main.inf", "use root;\npub fn main() {}");
    let mut db = RootDatabase::default();

    let analysis = db.analysis(&entry);
    assert!(
        analysis.import_problems().is_empty(),
        "`use root;` names the entry, not a file to load"
    );
}

#[test]
fn self_import_of_entry_is_deduplicated() {
    let tree = TempTree::new("self-import");
    let entry = tree.write(
        "main.inf",
        "use lib::helper;\npub fn main() -> i32 { return 0; }",
    );
    tree.write("lib/helper.inf", "use main;\npub fn help() {}");
    let mut db = RootDatabase::default();

    let analysis = db.analysis(&entry);
    let entry_files = analysis
        .arena()
        .source_files()
        .filter(|sf| sf.module_path.is_empty())
        .count();
    assert_eq!(entry_files, 1, "the entry is analyzed exactly once");
    assert!(analysis.import_problems().is_empty());
}

#[test]
fn mutually_importing_files_terminate() {
    let tree = TempTree::new("mutual");
    let entry = tree.write("main.inf", "use a;\npub fn main() {}");
    tree.write("a.inf", "use b;\npub fn fa() {}");
    tree.write("b.inf", "use a;\npub fn fb() {}");
    let mut db = RootDatabase::default();

    let analysis = db.analysis(&entry);
    let modules: Vec<Vec<String>> = analysis
        .arena()
        .source_files()
        .map(|sf| sf.module_path.clone())
        .collect();
    assert_eq!(
        modules,
        vec![
            Vec::<String>::new(),
            vec!["a".to_string()],
            vec!["b".to_string()],
        ],
    );
    assert!(analysis.import_problems().is_empty());
}

// Closure-aware invalidation

#[test]
fn change_to_imported_file_invalidates_entry_analysis() {
    let tree = TempTree::new("invalidate-dep");
    let entry = tree.write(
        "main.inf",
        "use lib::helper;\npub fn main() -> i32 { return 0; }",
    );
    let helper = tree.write("lib/helper.inf", "pub fn help() -> i32 { return 1; }");
    let mut db = RootDatabase::default();
    db.open_document(
        &entry,
        "use lib::helper;\npub fn main() -> i32 { return 0; }",
    );

    let first = db.analysis(&entry).generation();
    // Changing a file in the closure must drop the entry's memoized analysis.
    db.change_document(&helper, "pub fn help() -> i32 { return 2; }");
    let second = db.analysis(&entry).generation();

    assert!(
        second > first,
        "changing an imported file must force a recompute ({first} -> {second})"
    );
}

#[test]
fn change_to_unrelated_file_does_not_invalidate_entry_analysis() {
    let tree = TempTree::new("invalidate-unrelated");
    let entry = tree.write("main.inf", "pub fn main() -> i32 { return 0; }");
    let unrelated = tree.path("other.inf");
    let mut db = RootDatabase::default();
    db.open_document(&entry, "pub fn main() -> i32 { return 0; }");

    let first = db.analysis(&entry).generation();
    // A file outside the entry's closure must not disturb its memoized analysis.
    db.change_document(&unrelated, "pub fn other() {}");
    let second = db.analysis(&entry).generation();

    assert_eq!(
        first, second,
        "an unrelated change must not recompute the entry ({first} -> {second})"
    );
}

#[test]
fn opening_a_previously_unseen_file_reanalyzes_a_missing_import() {
    let tree = TempTree::new("resolve-missing");
    let entry = tree.write("main.inf", "use future;\npub fn main() {}");
    let mut db = RootDatabase::default();
    db.open_document(&entry, "use future;\npub fn main() {}");

    let first = db.analysis(&entry);
    let first_gen = first.generation();
    assert_eq!(first.import_problems().len(), 1, "future.inf is missing");

    // Open the previously-unseen file the import was looking for.
    let future = tree.path("future.inf");
    db.open_document(&future, "pub fn soon() {}");

    let second = db.analysis(&entry);
    assert!(
        second.generation() > first_gen,
        "a newly-opened file must re-analyze an entry with a missing import"
    );
    assert!(
        second.import_problems().is_empty(),
        "the import now resolves to the opened overlay"
    );
}

#[test]
fn reopening_a_closed_file_reresolves_a_missing_import() {
    let tree = TempTree::new("reopen-missing");
    // `main.inf` imports `lib`, which exists only as an editor buffer, never on
    // disk. Interning survives `didClose`, so a widening keyed on "path never
    // interned" would fire on the first open but not the reopen, leaving the
    // import stale until the entry itself is edited.
    let src = "use lib;\npub fn main() -> i32 { return 0; }";
    let entry = tree.write("main.inf", src);
    let lib = tree.path("lib.inf");
    let mut db = RootDatabase::default();
    db.open_document(&entry, src);

    assert_eq!(
        db.analysis(&entry).import_problems().len(),
        1,
        "lib is missing before it is opened"
    );

    let lib_src = "pub fn helper() -> i32 { return 7; }";
    db.open_document(&lib, lib_src);
    assert!(
        db.analysis(&entry).import_problems().is_empty(),
        "the first open of lib resolves the import"
    );

    db.close_document(&lib);
    assert_eq!(
        db.analysis(&entry).import_problems().len(),
        1,
        "closing lib removes its overlay, so the import is missing again"
    );

    let before_reopen = db.analysis(&entry).generation();
    // Reopening lib with the same content must re-resolve the import, not serve
    // the stale analysis computed while it was closed.
    db.open_document(&lib, lib_src);
    let reopened = db.analysis(&entry);
    assert!(
        reopened.import_problems().is_empty(),
        "reopening lib must re-resolve the import"
    );
    assert!(
        reopened.generation() > before_reopen,
        "reopening lib must recompute the entry ({before_reopen} -> {})",
        reopened.generation()
    );
}

#[test]
fn analysis_of_an_unreadable_entry_recovers_after_didopen() {
    let tree = TempTree::new("unreadable-entry");
    // The entry is never on disk. An early, out-of-order request (a stray hover,
    // say) races ahead of `didOpen` and memoizes an analysis of the unreadable
    // entry — an empty arena with no missing-import record. Unless the entry is
    // part of its own closure, no later event can ever evict that poisoned result.
    let entry = tree.path("ghost.inf");
    let mut db = RootDatabase::default();

    let (first_gen, first_files) = {
        let first = db.analysis(&entry);
        (first.generation(), first.arena().source_files().count())
    };
    assert_eq!(first_files, 0, "an unreadable entry yields an empty arena");

    // `didOpen` now supplies the content; the poisoned analysis must recompute.
    let source = "pub fn f() -> i32 { return broken; }";
    db.open_document(&entry, source);
    let (recovered_gen, recovered_files, has_type_errors) = {
        let recovered = db.analysis(&entry);
        (
            recovered.generation(),
            recovered.arena().source_files().count(),
            !recovered.type_errors().is_empty(),
        )
    };
    assert!(
        recovered_gen > first_gen,
        "didOpen of the entry must recompute its poisoned analysis"
    );
    assert_eq!(recovered_files, 1, "the opened overlay is now analyzed");
    assert!(
        has_type_errors,
        "the undeclared variable must surface once analysis actually runs"
    );

    // A subsequent `didChange` likewise recomputes — the entry stays evictable.
    let before_change = db.analysis(&entry).generation();
    db.change_document(&entry, "pub fn f() -> i32 { return 0; }");
    assert!(
        db.analysis(&entry).generation() > before_change,
        "didChange of the entry must recompute"
    );
}

#[test]
fn analysis_of_an_unreadable_import_recovers_after_didopen() {
    // The non-entry twin of the unreadable-entry case. `main.inf` imports `lib`;
    // `lib.inf` exists on disk but is invalid UTF-8, so `read_to_string` fails.
    // The import is not "missing" (the file exists) and leaves no `LoadedFile`, so
    // before the fix nothing recorded lib's path and no later event could evict
    // main's stale, symbol-less analysis.
    let tree = TempTree::new("unreadable-import-didopen");
    let src = "use lib;\npub fn main() -> i32 { return lib::seven(); }";
    let entry = tree.write("main.inf", src);
    let lib = tree.write_bytes("lib.inf", b"\xFF\xFE\xFA");
    let mut db = RootDatabase::default();
    db.open_document(&entry, src);

    let lib_mod = vec!["lib".to_string()];
    let first_gen = {
        let analysis = db.analysis(&entry);
        assert!(
            !closure_defines(analysis, &lib_mod, "seven"),
            "an unreadable import contributes no symbols"
        );
        analysis.generation()
    };

    // `didOpen` supplies valid text for the previously-unreadable import; main's
    // stale analysis must recompute and now see lib's symbols.
    db.open_document(&lib, "pub fn seven() -> i32 { return 7; }");
    let analysis = db.analysis(&entry);
    assert!(
        analysis.generation() > first_gen,
        "didOpen of the unreadable import must recompute main ({first_gen} -> {})",
        analysis.generation()
    );
    assert!(
        closure_defines(analysis, &lib_mod, "seven"),
        "the recomputed analysis must see the now-readable import's symbols"
    );
}

#[test]
fn analysis_of_an_unreadable_import_recovers_after_didchange() {
    // Recovery must also fire on `didChange`, which the closure mechanism covers
    // but the missing-import widening would not: a change carries
    // `newly_available == false`, so only a closure hit can invalidate. `lib` is
    // unreadable on disk with no overlay, and the change is the first event to
    // supply valid content for it — proving the closure records the read-failed
    // path rather than relying on the overlay-availability widening.
    let tree = TempTree::new("unreadable-import-didchange");
    let src = "use lib;\npub fn main() -> i32 { return lib::seven(); }";
    let entry = tree.write("main.inf", src);
    let lib = tree.write_bytes("lib.inf", b"\xFF\xFE\xFA");
    let mut db = RootDatabase::default();
    db.open_document(&entry, src);

    let lib_mod = vec!["lib".to_string()];
    let before_change = db.analysis(&entry).generation();
    assert!(
        !closure_defines(db.analysis(&entry), &lib_mod, "seven"),
        "the import is unreadable before the change"
    );

    db.change_document(&lib, "pub fn seven() -> i32 { return 7; }");
    let analysis = db.analysis(&entry);
    assert!(
        analysis.generation() > before_change,
        "didChange of the unreadable import must recompute main ({before_change} -> {})",
        analysis.generation()
    );
    assert!(
        closure_defines(analysis, &lib_mod, "seven"),
        "the recomputed analysis must see the changed import's symbols"
    );
}

#[test]
fn analysis_recovers_when_a_transitively_unreadable_import_becomes_readable() {
    // The read-failed file need not be a direct import. `main` imports `a`, `a`
    // imports `b`, and only `b.inf` is unreadable. The walk enqueues `b` while
    // analyzing `main`, so `b`'s read-failed path must land in main's closure and
    // a later `didOpen` of `b` must re-analyze `main`.
    let tree = TempTree::new("unreadable-import-transitive");
    let main_src = "use a;\npub fn main() -> i32 { return a::mid(); }";
    let entry = tree.write("main.inf", main_src);
    tree.write(
        "a.inf",
        "use b;\npub fn mid() -> i32 { return b::deep(); }",
    );
    let b = tree.write_bytes("b.inf", b"\xFF\xFE\xFA");
    let mut db = RootDatabase::default();
    db.open_document(&entry, main_src);

    let b_mod = vec!["b".to_string()];
    let first_gen = {
        let analysis = db.analysis(&entry);
        assert!(
            !closure_defines(analysis, &b_mod, "deep"),
            "the transitively-unreadable file contributes no symbols"
        );
        analysis.generation()
    };

    db.open_document(&b, "pub fn deep() -> i32 { return 42; }");
    let analysis = db.analysis(&entry);
    assert!(
        analysis.generation() > first_gen,
        "didOpen of a transitively-unreadable import must recompute main ({first_gen} -> {})",
        analysis.generation()
    );
    assert!(
        closure_defines(analysis, &b_mod, "deep"),
        "the recomputed analysis must see the now-readable transitive import"
    );
}

#[test]
fn opening_an_unrelated_file_does_not_invalidate_an_unreadable_import_analysis() {
    // The negative: recording the read-failed path in the closure must stay
    // precise. Only an event touching that exact path invalidates; an unrelated
    // file's `didOpen` must not — the read failure is not folded into
    // `had_missing_import`, so it does not trigger the coarse missing-import
    // widening.
    let tree = TempTree::new("unreadable-import-unrelated");
    let src = "use lib;\npub fn main() -> i32 { return 0; }";
    let entry = tree.write("main.inf", src);
    tree.write_bytes("lib.inf", b"\xFF\xFE\xFA");
    let mut db = RootDatabase::default();
    db.open_document(&entry, src);

    let first_gen = db.analysis(&entry).generation();

    // A file outside main's closure — neither the entry, an import, nor the
    // unreadable file — must not disturb main's memoized analysis.
    let unrelated = tree.path("unrelated.inf");
    db.open_document(&unrelated, "pub fn other() {}");
    assert_eq!(
        db.analysis(&entry).generation(),
        first_gen,
        "an unrelated file's didOpen must not recompute an unreadable-import analysis"
    );
}

// Salsa dependency-edge invalidation (issue #157): content changes force
// recomputes through per-file change-stamp inputs and a conditional availability
// epoch registered by the analysis query, while a write-path mirror keeps
// `is_analyzed` answering before any query re-runs. These pin the seams that make
// the edges and the mirror agree.

#[test]
fn an_edit_before_any_analysis_still_wires_the_dependency() {
    // The write-path stamp bump is get-or-create: editing a file before anything is
    // analyzed mints its stamp, and the first analysis of a dependent must find that
    // SAME input when it registers its closure edges — not a second, unbumped one.
    // If the two sites used separate registries (or a lookup-only bump), the later
    // edit would register on an input the query never read, and the dependent would
    // serve a stale analysis.
    let tree = TempTree::new("edit-before-analysis");
    let entry = tree.path("main.inf");
    let helper = tree.path("lib/helper.inf");
    let mut db = RootDatabase::default();

    // A change to helper with no analyses in the db: mints and bumps helper's stamp
    // on the write path, before any query has created it.
    db.change_document(&helper, "pub fn help_v1() -> i32 { return 1; }");

    // Open the dependent and analyze it: the in-query registration must reuse
    // helper's already-minted stamp via the shared registry.
    db.open_document(
        &entry,
        "use lib::helper;\npub fn main() -> i32 { return 0; }",
    );
    let lib_mod = vec!["lib".to_string(), "helper".to_string()];
    let first_gen = {
        let analysis = db.analysis(&entry);
        assert!(
            closure_defines(analysis, &lib_mod, "help_v1"),
            "the first analysis sees the pre-edit helper symbol"
        );
        analysis.generation()
    };

    // A second edit to helper must recompute the dependent through that shared stamp
    // edge, surfacing the new symbol.
    db.change_document(&helper, "pub fn help_v2() -> i32 { return 2; }");
    let analysis = db.analysis(&entry);
    assert!(
        analysis.generation() > first_gen,
        "an edit to a helper wired before any analysis must recompute the dependent \
         ({first_gen} -> {})",
        analysis.generation()
    );
    assert!(
        closure_defines(analysis, &lib_mod, "help_v2"),
        "the recompute must see the newly-edited helper symbol"
    );
}

#[test]
fn a_disk_edit_without_an_editor_event_is_invisible_until_the_next_event() {
    // The v1 invalidation contract: editor events (didOpen/didChange/didClose) are
    // the ONLY channel that invalidates an analysis — there is no filesystem watch.
    // A file edited on disk with no corresponding event stays invisible until the
    // next event touches the closure, and recovery keys on the event, not on a
    // content diff. A `didChangeWatchedFiles` feature must flip this test
    // deliberately.
    let tree = TempTree::new("disk-edit-invisible");
    let entry = tree.write(
        "main.inf",
        "use lib::helper;\npub fn main() -> i32 { return 0; }",
    );
    let helper = tree.write("lib/helper.inf", "pub fn help_old() -> i32 { return 1; }");
    let mut db = RootDatabase::default();
    // Open the entry so its analysis is part of the working set, independent of the
    // never-opened cap.
    db.open_document(
        &entry,
        "use lib::helper;\npub fn main() -> i32 { return 0; }",
    );

    let lib_mod = vec!["lib".to_string(), "helper".to_string()];
    let first_gen = {
        let analysis = db.analysis(&entry);
        assert!(
            closure_defines(analysis, &lib_mod, "help_old"),
            "the disk helper's symbol is visible before the silent edit"
        );
        analysis.generation()
    };

    // Edit the helper on disk with NO editor event.
    std::fs::write(&helper, "pub fn help_new() -> i32 { return 2; }").expect("rewrite helper");

    let after_silent = db.analysis(&entry);
    assert_eq!(
        after_silent.generation(),
        first_gen,
        "a silent disk edit must not recompute — no event, no invalidation"
    );
    assert!(
        closure_defines(after_silent, &lib_mod, "help_old"),
        "the memo still holds the pre-edit content (invisibility is content-level, \
         not merely stamp equality)"
    );
    assert!(
        !closure_defines(after_silent, &lib_mod, "help_new"),
        "the silently-written symbol is not visible without an event"
    );

    // The next event on the entry — even one that leaves its text unchanged — is the
    // sole invalidation channel; the recompute re-reads the helper from disk.
    db.change_document(
        &entry,
        "use lib::helper;\npub fn main() -> i32 { return 0; }",
    );
    let recovered = db.analysis(&entry);
    assert!(
        recovered.generation() > first_gen,
        "the didChange event must recompute the entry ({first_gen} -> {})",
        recovered.generation()
    );
    assert!(
        closure_defines(recovered, &lib_mod, "help_new"),
        "the post-event recompute re-reads the disk helper and sees the new symbol"
    );
}

#[test]
fn a_change_to_an_import_flips_is_analyzed_before_any_refetch() {
    // The write-path mirror gives the protocol layer write-time observability the
    // Salsa edges cannot: the instant a change lands, `is_analyzed` must report the
    // affected entry as stale and an unrelated entry as still analyzed — before any
    // query re-runs. This is the deterministic form of the republish-selectivity
    // contract the e2e suite otherwise observes only through timing.
    let tree = TempTree::new("mirror-flip");
    let entry = tree.write(
        "main.inf",
        "use lib::helper;\npub fn main() -> i32 { return 0; }",
    );
    tree.write("lib/helper.inf", "pub fn help() -> i32 { return 1; }");
    let unrelated = tree.write("other.inf", "pub fn other() -> i32 { return 0; }");
    let helper = tree.path("lib/helper.inf");
    let mut db = RootDatabase::default();
    db.open_document(
        &entry,
        "use lib::helper;\npub fn main() -> i32 { return 0; }",
    );
    db.open_document(&unrelated, "pub fn other() -> i32 { return 0; }");

    // Memoize both.
    let _ = db.analysis(&entry).generation();
    let _ = db.analysis(&unrelated).generation();
    assert!(db.is_analyzed(&entry) && db.is_analyzed(&unrelated));

    // A change to the shared import flips the mirror immediately, before any fetch.
    db.change_document(&helper, "pub fn help() -> i32 { return 2; }");
    assert!(
        !db.is_analyzed(&entry),
        "the dependent is marked stale in the mirror the instant the change lands"
    );
    assert!(
        db.is_analyzed(&unrelated),
        "an unrelated open buffer stays analyzed — the mirror clear is selective"
    );

    // The next fetch recomputes and restores the analyzed state.
    let _ = db.analysis(&entry);
    assert!(
        db.is_analyzed(&entry),
        "fetching the stale entry recomputes and re-marks it analyzed"
    );
}

#[test]
fn two_entries_sharing_an_import_recompute_lazily_and_independently() {
    // Two open entries import a shared file. Editing it marks both stale, but the
    // recompute is lazy and per-entry: fetching A recomputes only A, leaving B stale
    // in the mirror until B is itself fetched. Pins that the salsa-driven recompute
    // is demand-driven, not eager across every dependent.
    let tree = TempTree::new("shared-lazy-independent");
    let a = tree.write(
        "a.inf",
        "use shared;\npub fn a() -> i32 { return shared::v(); }",
    );
    let b = tree.write(
        "b.inf",
        "use shared;\npub fn b() -> i32 { return shared::v(); }",
    );
    let shared = tree.write("shared.inf", "pub fn v() -> i32 { return 7; }");
    let mut db = RootDatabase::default();
    db.open_document(&a, "use shared;\npub fn a() -> i32 { return shared::v(); }");
    db.open_document(&b, "use shared;\npub fn b() -> i32 { return shared::v(); }");

    let a_first = db.analysis(&a).generation();
    let b_first = db.analysis(&b).generation();

    db.change_document(&shared, "pub fn v() -> i32 { return 8; }");
    assert!(
        !db.is_analyzed(&a) && !db.is_analyzed(&b),
        "both dependents are marked stale in the mirror"
    );

    // Fetch A only: it recomputes with a fresh generation, B stays stale (lazy).
    let a_second = db.analysis(&a).generation();
    assert!(
        a_second > a_first,
        "fetching A recomputes it ({a_first} -> {a_second})"
    );
    assert!(
        !db.is_analyzed(&b),
        "B is not recomputed just because A was — the recompute is lazy and per-entry"
    );

    // Fetch B: it recomputes independently, on its own demand.
    let b_second = db.analysis(&b).generation();
    assert!(
        b_second > b_first,
        "fetching B recomputes it independently ({b_first} -> {b_second})"
    );
}

#[test]
fn an_unrelated_newly_available_open_refires_a_missing_import_entry() {
    // The availability-epoch edge is a deliberately coarse over-approximation: an
    // entry with an unresolved import reads the epoch, so ANY newly-available open —
    // even of a file unrelated to the missing import — recomputes it. This wastes
    // work but never serves a stale result, and is the price of having no file to
    // name for a missing import.
    let tree = TempTree::new("epoch-over-approximation");
    let entry = tree.write("main.inf", "use future;\npub fn main() {}");
    let mut db = RootDatabase::default();
    db.open_document(&entry, "use future;\npub fn main() {}");

    let first_gen = {
        let analysis = db.analysis(&entry);
        assert_eq!(analysis.import_problems().len(), 1, "future is missing");
        analysis.generation()
    };

    // Open a genuinely unrelated file — not the missing import, not in the entry's
    // closure — that had no overlay before. The epoch bump still recomputes the
    // missing-import entry.
    let unrelated = tree.path("unrelated.inf");
    db.open_document(&unrelated, "pub fn other() {}");
    let after = db.analysis(&entry);
    assert!(
        after.generation() > first_gen,
        "an unrelated newly-available open must refire a missing-import entry via the \
         epoch edge ({first_gen} -> {})",
        after.generation()
    );
    assert_eq!(
        after.import_problems().len(),
        1,
        "the unrelated open did not resolve the missing import — only forced a recompute"
    );
}

#[test]
fn a_resolved_import_stops_reacting_to_the_availability_epoch() {
    // The epoch edge is conditional on the LAST compute having a missing import.
    // Once the import resolves, the recomputed memo no longer reads the epoch, so a
    // later unrelated newly-available open leaves it untouched. This is what keeps
    // the unrelated-change equalities holding
    // (`change_to_unrelated_file_does_not_invalidate_entry_analysis`,
    // `opening_an_unrelated_file_does_not_invalidate_an_unreadable_import_analysis`):
    // a "simplified" unconditional epoch read would recompute every memo on every
    // newly-available open and break both.
    let tree = TempTree::new("epoch-drops-after-recovery");
    let entry = tree.write("main.inf", "use future;\npub fn main() {}");
    let mut db = RootDatabase::default();
    db.open_document(&entry, "use future;\npub fn main() {}");

    assert_eq!(
        db.analysis(&entry).import_problems().len(),
        1,
        "future is missing before it is opened"
    );

    // Resolve the missing import by opening the awaited file.
    let future = tree.path("future.inf");
    db.open_document(&future, "pub fn soon() {}");
    let resolved_gen = {
        let analysis = db.analysis(&entry);
        assert!(
            analysis.import_problems().is_empty(),
            "the import now resolves to the opened overlay"
        );
        analysis.generation()
    };

    // A later unrelated newly-available open must NOT recompute the now-resolved
    // entry: its last compute had no missing import, so it dropped the epoch edge.
    let unrelated = tree.path("unrelated.inf");
    db.open_document(&unrelated, "pub fn other() {}");
    assert_eq!(
        db.analysis(&entry).generation(),
        resolved_gen,
        "a resolved entry no longer reacts to the availability epoch"
    );
}

// Partial semantics: a broken program still answers

#[test]
fn type_error_still_leaves_a_queryable_typed_context() {
    let tree = TempTree::new("type-error");
    // `bad` returns bool where i32 is declared; `ok` is well-typed.
    let source = "fn ok() -> i32 { return 1; } fn bad() -> i32 { return true; }";
    let entry = tree.write("main.inf", source);
    let mut db = RootDatabase::default();
    db.open_document(&entry, source);

    let analysis = db.analysis(&entry);
    assert!(
        !analysis.type_errors().is_empty(),
        "the bool/i32 mismatch must surface as a type diagnostic"
    );

    // The typed context still answers for the well-typed part: the `1` literal.
    let entry_file = analysis.source_file_id(&[]).expect("entry file present");
    let offset = source.find("return 1").unwrap() as u32 + "return ".len() as u32;
    let hit = analysis
        .hit_test(entry_file, offset)
        .expect("a node covers the literal `1`");
    assert!(matches!(hit.node, NodeId::Expr(_)));
    assert!(
        analysis
            .typed_context()
            .get_node_typeinfo(hit.node)
            .is_some(),
        "get_node_typeinfo must answer for the well-typed literal"
    );
}

#[test]
fn parse_error_in_entry_still_runs_type_checking() {
    let tree = TempTree::new("parse-error");
    // A missing `)` — the resilient parser recovers and the checker still runs.
    let source = "fn main( { return 0; }";
    let entry = tree.write("main.inf", source);
    let mut db = RootDatabase::default();
    db.open_document(&entry, source);

    let analysis = db.analysis(&entry);
    // The entry's own syntax error is collected under the empty module path.
    assert!(
        analysis
            .parse_errors()
            .iter()
            .any(|fe| fe.module_path.is_empty() && !fe.errors.is_empty()),
        "the entry's parse error must be recorded"
    );

    // Type checking ran on the recovered body, not merely the parser: the
    // recovered `fn main` has no return type (Unit) yet `return 0;` yields i32, so
    // the checker must report that mismatch. Querying the always-present typed
    // context proves nothing — this must observe a fact that only running the
    // checker produces, or the test stays green even if checking were skipped on
    // every file with a syntax error (i.e. every file mid-edit).
    assert!(
        !analysis.type_errors().is_empty(),
        "the recovered `fn main` must be type-checked; `return 0` in a unit fn is a mismatch"
    );

    // The checker also populated node types for the recovered body: the `0`
    // literal has an inferred type, which a bare `TypedContext::new` would lack.
    let entry_file = analysis.source_file_id(&[]).expect("entry file present");
    let literal_offset = source
        .find("return 0")
        .expect("recovered body has `return 0`") as u32
        + "return ".len() as u32;
    let hit = analysis
        .hit_test(entry_file, literal_offset)
        .expect("a node covers the recovered literal `0`");
    assert!(
        analysis
            .typed_context()
            .get_node_typeinfo(hit.node)
            .is_some(),
        "type checking must have inferred a type for the recovered body's literal"
    );
}

#[test]
fn analysis_of_trivially_broken_source_does_not_panic() {
    // Each source lowers to two or more error-placeholder functions on the
    // resilient parse path, so the whole-program call graph shared by A035/A036
    // sees duplicate (non-injective) `FnKey`s. Building it must tolerate that
    // deterministically rather than aborting the analysis (in debug builds, the
    // whole LSP process). Reaching `findings()` proves every rule ran.
    for (tag, source) in [
        ("stray-forall", "forall { }"),
        ("two-semicolons", ";;"),
        ("two-idents", "foo bar"),
        (
            "valid-then-two-stray",
            "fn f() -> i32 { return 1; }\nforall { }\nexists { }",
        ),
    ] {
        let tree = TempTree::new(tag);
        let entry = tree.write("main.inf", source);
        let mut db = RootDatabase::default();
        db.open_document(&entry, source);

        let analysis = db.analysis(&entry);
        let _ = analysis.findings();
        assert!(
            analysis
                .parse_errors()
                .iter()
                .any(|fe| !fe.errors.is_empty()),
            "{tag}: the broken source must still surface parse errors"
        );
    }
}

// Analysis findings are tagged with rule id and severity

#[test]
fn analysis_findings_are_tagged_with_rule_id_and_severity() {
    let tree = TempTree::new("findings");
    // `break` outside a loop is analysis rule A001 (an error).
    let source = "pub fn main() { break; }";
    let entry = tree.write("main.inf", source);
    let mut db = RootDatabase::default();
    db.open_document(&entry, source);

    let analysis = db.analysis(&entry);
    let break_finding = analysis
        .findings()
        .iter()
        .find(|f| f.rule_id == "A001")
        .expect("break outside loop must be reported as A001");
    assert_eq!(break_finding.severity, Severity::Error);
}

// Def walk over the public API

#[test]
fn file_defs_covers_struct_methods_spec_fns_and_constants() {
    let tree = TempTree::new("def-walk");
    let source = "const N: i32 = 1; struct P { x: i32; fn get() -> i32 { return self.x; } } spec S { fn prop() {} }";
    let entry = tree.write("main.inf", source);
    let mut db = RootDatabase::default();
    db.open_document(&entry, source);

    let analysis = db.analysis(&entry);
    let entry_file = analysis.source_file_id(&[]).expect("entry file present");
    let names: Vec<&str> = analysis
        .file_defs(entry_file)
        .iter()
        .map(|&d| analysis.arena().def_name(d))
        .collect();
    assert_eq!(names, vec!["N", "P", "get", "S", "prop"]);
}

// Per-entry source root (issue #243): a non-entry file opened standalone must
// resolve its imports against the project's real source root, not its own
// directory.

/// A minimal valid `Inference.toml`, enough for manifest discovery to treat the
/// directory as a project root (`[package]` with `name` and `version`).
const MANIFEST: &str = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n";

#[test]
fn subdirectory_file_resolves_imports_against_manifest_source_root() {
    // The exact issue-#243 repro under a manifest: opening `src/lib/a.inf`
    // standalone must resolve its `use lib::b;` against `<root>/src` (the source
    // root the compiler uses), reaching `<root>/src/lib/b.inf` rather than the
    // nonexistent `<root>/src/lib/lib/b.inf` an own-directory root would probe.
    let tree = TempTree::new("manifest-subdir");
    tree.write("Inference.toml", MANIFEST);
    tree.write("src/main.inf", "use lib::a;\npub fn main() -> i32 { return 0; }");
    let a = tree.write("src/lib/a.inf", "use lib::b;\npub fn a() {}");
    tree.write("src/lib/b.inf", "pub fn seven() -> i32 { return 7; }");
    let mut db = RootDatabase::default();

    // As if the user navigated into the file and the editor opened it.
    db.open_document(&a, "use lib::b;\npub fn a() {}");

    let analysis = db.analysis(&a);
    assert!(
        analysis.import_problems().is_empty(),
        "lib::b must resolve against the manifest source root, got: {:?}",
        analysis.import_problems()
    );
    let b_mod = vec!["lib".to_string(), "b".to_string()];
    assert!(
        closure_defines(analysis, &b_mod, "seven"),
        "symbols from lib/b.inf must be visible in the standalone analysis"
    );
}

#[test]
fn nested_manifest_nearest_wins_for_source_root() {
    // Two manifests: an outer one at the tree root and a nearer one at `sub/`.
    // Opening `sub/src/lib/a.inf` must use the nearest manifest's source root
    // (`<root>/sub/src`); the outer manifest's `<root>/src` would not contain
    // `lib/b.inf` and would report a false missing import.
    let tree = TempTree::new("manifest-nested");
    tree.write("Inference.toml", MANIFEST);
    tree.write("sub/Inference.toml", MANIFEST);
    let a = tree.write("sub/src/lib/a.inf", "use lib::b;\npub fn a() {}");
    tree.write("sub/src/lib/b.inf", "pub fn seven() -> i32 { return 7; }");
    let mut db = RootDatabase::default();

    let analysis = db.analysis(&a);
    assert!(
        analysis.import_problems().is_empty(),
        "the nearest manifest's source root must resolve lib::b, got: {:?}",
        analysis.import_problems()
    );
    let b_mod = vec!["lib".to_string(), "b".to_string()];
    assert!(closure_defines(analysis, &b_mod, "seven"));
}

#[test]
fn file_outside_manifest_src_falls_through_to_own_directory() {
    // A file under the project root but outside its `src` source tree is not
    // governed by the manifest source root, so resolution falls through to the
    // file's own directory. Here `use dep;` resolves against `<root>/scratch`,
    // where `dep.inf` sits — proving the manifest tier declined (its `<root>/src`
    // root would not find `dep`).
    let tree = TempTree::new("manifest-outside-src");
    tree.write("Inference.toml", MANIFEST);
    let a = tree.write("scratch/a.inf", "use dep;\npub fn a() {}");
    tree.write("scratch/dep.inf", "pub fn d() {}");
    let mut db = RootDatabase::default();

    let analysis = db.analysis(&a);
    assert!(
        analysis.import_problems().is_empty(),
        "own-directory resolution must resolve the sibling import, got: {:?}",
        analysis.import_problems()
    );
    let dep_mod = vec!["dep".to_string()];
    assert!(closure_defines(analysis, &dep_mod, "d"));
}

#[test]
fn closure_fallback_reuses_an_analyzed_entrys_source_root() {
    // No manifest. The project entry `main.inf` is analyzed first, pulling
    // `lib/a.inf` and `lib/b.inf` into its closure against `<root>`. Opening
    // `lib/a.inf` standalone then reuses that root (it is in main's closure), so
    // `use lib::b;` resolves to `<root>/lib/b.inf` — the closure fallback tier.
    let tree = TempTree::new("closure-donor");
    let main = tree.write("main.inf", "use lib::a;\npub fn main() {}");
    let a = tree.write("lib/a.inf", "use lib::b;\npub fn a() {}");
    tree.write("lib/b.inf", "pub fn seven() -> i32 { return 7; }");
    let mut db = RootDatabase::default();

    // Memoize the entry's analysis so its closure is available to donate.
    assert!(
        db.analysis(&main).import_problems().is_empty(),
        "the entry resolves its whole closure against its own directory"
    );

    let analysis = db.analysis(&a);
    assert!(
        analysis.import_problems().is_empty(),
        "lib/a.inf must reuse the entry's source root, got: {:?}",
        analysis.import_problems()
    );
    let b_mod = vec!["lib".to_string(), "b".to_string()];
    assert!(closure_defines(analysis, &b_mod, "seven"));
}

#[test]
fn a_memoized_entry_keeps_its_root_when_a_donor_appears_later() {
    // The ordering counterpart to the test above. There the donor entry is
    // analyzed *first*, so `lib/a.inf` adopts its root on the very first (miss)
    // resolution. Here `lib/a.inf` is analyzed first and falls to tier 3 (its own
    // directory), where `use lib::b;` cannot resolve. Analyzing `main.inf`
    // afterwards makes a donor available — but a source root is resolved only when
    // an analysis is actually recomputed, so re-querying `lib/a.inf` serves the
    // memoized result unchanged rather than silently re-resolving it under the
    // donor that appeared since. This pins the resolution to the miss path: doing
    // it on every request would also re-walk the filesystem on each keystroke.
    let tree = TempTree::new("donor-appears-later");
    let main = tree.write("main.inf", "use lib::a;\npub fn main() {}");
    let a = tree.write("lib/a.inf", "use lib::b;\npub fn a() {}");
    tree.write("lib/b.inf", "pub fn seven() -> i32 { return 7; }");
    let mut db = RootDatabase::default();

    // Tier 3: a.inf resolves against its own directory, so `lib::b` is missing.
    let first_gen = {
        let analysis = db.analysis(&a);
        assert_eq!(
            analysis.import_problems().len(),
            1,
            "an own-directory root cannot resolve a source-root-relative import"
        );
        analysis.generation()
    };

    // A donor now exists: main's closure covers both lib/a.inf and lib/b.inf.
    assert!(db.analysis(&main).import_problems().is_empty());

    let analysis = db.analysis(&a);
    assert_eq!(
        analysis.generation(),
        first_gen,
        "a memo hit must not recompute a.inf just because a donor appeared"
    );
    assert_eq!(
        analysis.import_problems().len(),
        1,
        "the memoized result still reflects the own-directory root it was computed against"
    );
}

#[test]
fn closure_fallback_does_not_reuse_an_unrelated_entrys_root() {
    // The negative of the closure fallback: an analyzed entry whose closure does
    // not contain the file must not donate its root. `other.inf` is a lone entry;
    // opening `lib/a.inf` (absent from other's closure) must fall through to
    // own-directory resolution, where `use lib::b;` cannot be found.
    let tree = TempTree::new("closure-unrelated");
    let other = tree.write("other.inf", "pub fn other() {}");
    let a = tree.write("lib/a.inf", "use lib::b;\npub fn a() {}");
    tree.write("lib/b.inf", "pub fn seven() -> i32 { return 7; }");
    let mut db = RootDatabase::default();

    // Memoize the unrelated entry's analysis.
    let _ = db.analysis(&other).generation();

    let analysis = db.analysis(&a);
    assert_eq!(
        analysis.import_problems().len(),
        1,
        "an unrelated entry must not donate its source root"
    );
    assert_eq!(analysis.import_problems()[0].referenced_as, "lib::b");
}

#[test]
fn standalone_subdirectory_file_without_manifest_uses_own_directory() {
    // Tier 3, pinned: with no manifest and nothing else analyzed, a subdirectory
    // file is analyzed against its own directory. A source-root-relative import
    // then cannot resolve — the pre-#243 fallback the manifest and closure tiers
    // improve upon. Kept to guard the tier ordering (own directory is last).
    let tree = TempTree::new("own-dir-standalone");
    let a = tree.write("lib/a.inf", "use lib::b;\npub fn a() {}");
    tree.write("lib/b.inf", "pub fn seven() -> i32 { return 7; }");
    let mut db = RootDatabase::default();

    let analysis = db.analysis(&a);
    assert_eq!(
        analysis.import_problems().len(),
        1,
        "an own-directory root cannot resolve a source-root-relative import"
    );
    assert_eq!(analysis.import_problems()[0].referenced_as, "lib::b");
}

#[test]
fn didchange_under_manifest_root_invalidates_and_recovers() {
    // Invalidation must work under the manifest-derived root: opening
    // `src/lib/a.inf` pulls `src/lib/b.inf` into its closure, so a `didChange` of
    // `b.inf` (a different directory of the same project) must evict a.inf's
    // analysis, and the recompute must still resolve against the manifest root.
    let tree = TempTree::new("manifest-invalidation");
    tree.write("Inference.toml", MANIFEST);
    tree.write("src/main.inf", "use lib::a;\npub fn main() {}");
    let a = tree.write("src/lib/a.inf", "use lib::b;\npub fn a() {}");
    let b = tree.write("src/lib/b.inf", "pub fn seven() -> i32 { return 7; }");
    let mut db = RootDatabase::default();
    db.open_document(&a, "use lib::b;\npub fn a() {}");

    let b_mod = vec!["lib".to_string(), "b".to_string()];
    let first_gen = {
        let analysis = db.analysis(&a);
        assert!(
            analysis.import_problems().is_empty(),
            "lib::b resolves under the manifest root before the change"
        );
        assert!(closure_defines(analysis, &b_mod, "seven"));
        analysis.generation()
    };

    db.change_document(&b, "pub fn seven() -> i32 { return 8; }");
    let analysis = db.analysis(&a);
    assert!(
        analysis.generation() > first_gen,
        "a change to a closure file must recompute a.inf ({first_gen} -> {})",
        analysis.generation()
    );
    assert!(
        analysis.import_problems().is_empty() && closure_defines(analysis, &b_mod, "seven"),
        "the recompute must still resolve lib::b under the manifest root"
    );
}

#[test]
fn closure_donor_root_survives_shared_dep_change_when_dependent_reanalyzes_first() {
    // No manifest. The project entry `main.inf` pulls `lib/a.inf` and the shared
    // `lib/b.inf` into its closure against `<root>`; analyzing `lib/a.inf`
    // standalone then adopts that root via the closure-donor tier. A change to the
    // shared `lib/b.inf` evicts BOTH main's analysis (the donor) and a.inf's in one
    // call. Re-analyzing a.inf *before* main leaves no memoized donor — yet the
    // adopted root must survive the donor's eviction, or `use lib::b;` falls back to
    // a.inf's own directory and reports a false missing import (the pre-fix bug).
    let tree = TempTree::new("closure-donor-survives-shared-change");
    let main = tree.write("main.inf", "use lib::a;\npub fn main() {}");
    let a = tree.write("lib/a.inf", "use lib::b;\npub fn a() {}");
    let b = tree.write("lib/b.inf", "pub fn seven() -> i32 { return 7; }");
    let mut db = RootDatabase::default();

    // Memoize the entry so its closure can donate, then adopt its root for a.inf.
    assert!(db.analysis(&main).import_problems().is_empty());
    let b_mod = vec!["lib".to_string(), "b".to_string()];
    let first_gen = {
        let analysis = db.analysis(&a);
        assert!(
            analysis.import_problems().is_empty(),
            "lib/a.inf adopts the entry's source root, got: {:?}",
            analysis.import_problems()
        );
        analysis.generation()
    };

    // The shared-dependency change evicts both the donor and a.inf.
    db.change_document(&b, "pub fn seven() -> i32 { return 8; }");

    // Re-analyze a.inf first: the donor is gone, so only the sticky adopted root
    // keeps lib::b resolving.
    let analysis = db.analysis(&a);
    assert!(
        analysis.generation() > first_gen,
        "the shared-dependency change must recompute a.inf ({first_gen} -> {})",
        analysis.generation()
    );
    assert!(
        analysis.import_problems().is_empty(),
        "the adopted root must survive the donor's eviction, got: {:?}",
        analysis.import_problems()
    );
    assert!(
        closure_defines(analysis, &b_mod, "seven"),
        "lib/b.inf's symbols must remain visible after the recompute"
    );
}

// Eviction (issue #247): closing a document drops its overlay-derived analysis,
// and analyses memoized for never-opened paths are capped so a long session
// cannot grow the map without bound. Open documents are never evicted.

#[test]
fn closing_a_document_drops_its_memoized_entry_analysis() {
    let tree = TempTree::new("close-drops-entry");
    let entry = tree.write("main.inf", "pub fn main() -> i32 { return 0; }");
    let mut db = RootDatabase::default();
    db.open_document(&entry, "pub fn main() -> i32 { return 0; }");

    let before = db.analysis(&entry).generation();
    db.close_document(&entry);
    // The overlay is gone, so the analysis computed from it must not be served
    // again: the next query recomputes from disk.
    let after = db.analysis(&entry).generation();
    assert!(
        after > before,
        "closing a document must drop its memoized analysis ({before} -> {after})"
    );
}

#[test]
fn a_closed_file_remains_available_through_an_open_dependents_closure() {
    // `main` imports `lib`; both are opened, and `lib` also exists on disk. Closing
    // `lib` drops its own entry, but the still-open dependent `main` re-reads it
    // from disk on its next query, so `lib`'s symbols stay visible.
    let tree = TempTree::new("close-dependent-disk");
    let main = tree.write(
        "main.inf",
        "use lib;\npub fn main() -> i32 { return lib::helper(); }",
    );
    let lib = tree.write("lib.inf", "pub fn helper() -> i32 { return 7; }");
    let mut db = RootDatabase::default();
    db.open_document(&lib, "pub fn helper() -> i32 { return 7; }");
    db.open_document(
        &main,
        "use lib;\npub fn main() -> i32 { return lib::helper(); }",
    );

    let lib_mod = vec!["lib".to_string()];
    let before = {
        let analysis = db.analysis(&main);
        assert!(closure_defines(analysis, &lib_mod, "helper"));
        analysis.generation()
    };

    db.close_document(&lib);
    let analysis = db.analysis(&main);
    assert!(
        analysis.generation() > before,
        "closing an imported file must invalidate the open dependent ({before} -> {})",
        analysis.generation()
    );
    assert!(
        closure_defines(analysis, &lib_mod, "helper"),
        "the dependent must re-read the closed file from disk and still see its symbols"
    );
}

#[test]
fn closing_a_document_drops_its_sticky_source_root() {
    // The sticky root must not outlive the open document. `lib/a.inf` adopts the
    // entry `main.inf`'s root via the closure-donor tier; closing a.inf evicts its
    // analysis and, because a.inf is in main's closure, main's too — so no donor
    // remains. Re-analyzing a.inf must then re-resolve from scratch, falling to its
    // own directory (where `use lib::b;` cannot be found) rather than serving the
    // dropped adopted root.
    let tree = TempTree::new("close-drops-sticky-root");
    let main = tree.write("main.inf", "use lib::a;\npub fn main() {}");
    let a = tree.write("lib/a.inf", "use lib::b;\npub fn a() {}");
    tree.write("lib/b.inf", "pub fn seven() -> i32 { return 7; }");
    let mut db = RootDatabase::default();

    assert!(db.analysis(&main).import_problems().is_empty());
    assert!(
        db.analysis(&a).import_problems().is_empty(),
        "lib/a.inf adopts the entry's source root before the close"
    );

    db.close_document(&a);

    let analysis = db.analysis(&a);
    assert_eq!(
        analysis.import_problems().len(),
        1,
        "with the sticky root dropped and no donor left, lib::b cannot resolve"
    );
    assert_eq!(analysis.import_problems()[0].referenced_as, "lib::b");
}

#[test]
fn never_opened_analyses_are_capped_with_fifo_eviction() {
    // The cap is a small constant (8). Memoizing analyses for ten never-opened
    // files evicts the two oldest, so re-querying an oldest one recomputes while a
    // recent one is still a cache hit.
    let tree = TempTree::new("unopened-cap");
    let mut db = RootDatabase::default();

    let mut paths = Vec::new();
    let mut first_gen = Vec::new();
    for i in 0..10 {
        let path = tree.write(
            &format!("f{i}.inf"),
            &format!("pub fn f{i}() -> i32 {{ return {i}; }}"),
        );
        first_gen.push(db.analysis(&path).generation());
        paths.push(path);
    }

    // The oldest never-opened analysis was evicted: re-querying it recomputes with
    // a fresh, larger generation stamp.
    let requeried_oldest = db.analysis(&paths[0]).generation();
    assert!(
        requeried_oldest > first_gen[9],
        "the oldest never-opened analysis must be evicted and recomputed ({} -> {requeried_oldest})",
        first_gen[0]
    );

    // A recently memoized never-opened analysis is retained: re-querying is a cache
    // hit that returns its original generation.
    assert_eq!(
        db.analysis(&paths[9]).generation(),
        first_gen[9],
        "a recently memoized never-opened analysis must be retained"
    );
}

#[test]
fn open_documents_are_never_evicted_by_the_never_opened_cap() {
    let tree = TempTree::new("open-never-evicted");
    let opened = tree.write("open.inf", "pub fn kept() -> i32 { return 0; }");
    let mut db = RootDatabase::default();
    db.open_document(&opened, "pub fn kept() -> i32 { return 0; }");
    let open_gen = db.analysis(&opened).generation();

    // Flood the cap with far more never-opened files than it retains.
    for i in 0..20 {
        let path = tree.write(&format!("f{i}.inf"), &format!("pub fn f{i}() {{}}"));
        let _ = db.analysis(&path).generation();
    }

    assert_eq!(
        db.analysis(&opened).generation(),
        open_gen,
        "an open document's analysis must survive the never-opened cap untouched"
    );
}

#[test]
fn opening_a_previously_unopened_analysis_exempts_it_from_the_cap() {
    // A path first seen via a feature request while never opened is capped; once
    // the editor opens it, it joins the working set and is exempt even as the cap
    // churns with other never-opened files.
    let tree = TempTree::new("promote-out-of-cap");
    let promoted = tree.write("promoted.inf", "pub fn kept() -> i32 { return 0; }");
    let mut db = RootDatabase::default();

    let _ = db.analysis(&promoted).generation();
    db.open_document(&promoted, "pub fn kept() -> i32 { return 0; }");
    let open_gen = db.analysis(&promoted).generation();

    for i in 0..20 {
        let path = tree.write(&format!("f{i}.inf"), &format!("pub fn f{i}() {{}}"));
        let _ = db.analysis(&path).generation();
    }

    assert_eq!(
        db.analysis(&promoted).generation(),
        open_gen,
        "a document opened after first being seen unopened must be exempt from the cap"
    );
}

// Selectivity across coexisting analyses (issue #254): the headline invalidation
// contract — "a keystroke in one buffer must not force every other open buffer to
// re-analyze" — is a statement about *several* live analyses, so it is asserted
// here with more than one memoized entry present at once, observing each entry's
// generation independently. A cache hit returns an analysis's original generation
// unchanged, so an entry whose generation is stable across a re-query is one that
// was not recomputed.

#[test]
fn a_keystroke_in_one_open_buffer_leaves_unrelated_open_buffers_intact() {
    // Three independent entries are open and memoized at once. Editing one must
    // recompute only that entry; the other two keep their exact analyses (their
    // generations are unchanged on re-query), so a burst of typing in one file
    // never re-runs the pipeline for unrelated buffers.
    let tree = TempTree::new("selectivity-independent");
    let a = tree.write("a.inf", "pub fn a() -> i32 { return 1; }");
    let b = tree.write("b.inf", "pub fn b() -> i32 { return 2; }");
    let c = tree.write("c.inf", "pub fn c() -> i32 { return 3; }");
    let mut db = RootDatabase::default();
    db.open_document(&a, "pub fn a() -> i32 { return 1; }");
    db.open_document(&b, "pub fn b() -> i32 { return 2; }");
    db.open_document(&c, "pub fn c() -> i32 { return 3; }");

    // Memoize all three coexisting analyses.
    let a_first = db.analysis(&a).generation();
    let b_first = db.analysis(&b).generation();
    let c_first = db.analysis(&c).generation();

    // A keystroke in a.inf.
    db.change_document(&a, "pub fn a() -> i32 { return 11; }");

    assert!(
        db.analysis(&a).generation() > a_first,
        "the edited buffer recomputes"
    );
    assert_eq!(
        db.analysis(&b).generation(),
        b_first,
        "an unrelated open buffer is not re-analyzed by a keystroke in another"
    );
    assert_eq!(
        db.analysis(&c).generation(),
        c_first,
        "a second unrelated open buffer is likewise untouched"
    );
}

#[test]
fn editing_a_shared_import_recomputes_every_dependent_but_not_an_independent_buffer() {
    // Two open entries both import a shared on-disk lib; a third open entry is
    // independent. Editing the shared lib must recompute *both* dependents (their
    // closures contain it) while leaving the independent buffer's analysis
    // memoized — invalidation is precise to the closure, across several live
    // analyses at once.
    let tree = TempTree::new("selectivity-shared");
    let a = tree.write("a.inf", "use shared;\npub fn a() -> i32 { return shared::v(); }");
    let b = tree.write("b.inf", "use shared;\npub fn b() -> i32 { return shared::v(); }");
    let indep = tree.write("indep.inf", "pub fn indep() -> i32 { return 0; }");
    let shared = tree.write("shared.inf", "pub fn v() -> i32 { return 7; }");
    let mut db = RootDatabase::default();
    db.open_document(&a, "use shared;\npub fn a() -> i32 { return shared::v(); }");
    db.open_document(&b, "use shared;\npub fn b() -> i32 { return shared::v(); }");
    db.open_document(&indep, "pub fn indep() -> i32 { return 0; }");

    let a_first = db.analysis(&a).generation();
    let b_first = db.analysis(&b).generation();
    let indep_first = db.analysis(&indep).generation();

    db.change_document(&shared, "pub fn v() -> i32 { return 8; }");

    assert!(
        db.analysis(&a).generation() > a_first,
        "the first dependent recomputes when the shared import changes"
    );
    assert!(
        db.analysis(&b).generation() > b_first,
        "the second dependent recomputes too"
    );
    assert_eq!(
        db.analysis(&indep).generation(),
        indep_first,
        "the independent buffer, outside the shared closure, is not recomputed"
    );
}

#[test]
fn a_transitive_import_change_invalidates_the_root_entry() {
    // A readable transitive dependency: `main` imports `a`, `a` imports `b`, and
    // all three read cleanly. Editing `b` (two hops from the entry) must invalidate
    // `main`'s memoized analysis, and the recompute must still resolve the whole
    // chain. The existing transitive coverage only exercises the *unreadable → readable*
    // recovery path; this pins plain transitive-closure invalidation.
    let tree = TempTree::new("transitive-invalidate");
    let main_src = "use a;\npub fn main() -> i32 { return a::mid(); }";
    let entry = tree.write("main.inf", main_src);
    tree.write("a.inf", "use b;\npub fn mid() -> i32 { return b::deep(); }");
    let b = tree.write("b.inf", "pub fn deep() -> i32 { return 42; }");
    let mut db = RootDatabase::default();
    db.open_document(&entry, main_src);

    let b_mod = vec!["b".to_string()];
    let first = {
        let analysis = db.analysis(&entry);
        assert!(closure_defines(analysis, &b_mod, "deep"));
        analysis.generation()
    };

    db.change_document(&b, "pub fn deep() -> i32 { return 43; }");
    let analysis = db.analysis(&entry);
    assert!(
        analysis.generation() > first,
        "editing a transitive import (main -> a -> b) must recompute main ({first} -> {})",
        analysis.generation()
    );
    assert!(
        analysis.import_problems().is_empty() && closure_defines(analysis, &b_mod, "deep"),
        "the recompute still resolves the whole transitive chain"
    );
}

#[test]
fn editing_a_member_of_an_import_cycle_invalidates_the_entry() {
    // `main` imports `a`; `a` and `b` import each other (an a <-> b cycle). The
    // closure walk terminates and records both cycle members, so editing either one
    // must invalidate `main` — not only the direct import `a`, but the cyclic `b`
    // reached through it. Only termination over a cycle was pinned before.
    let tree = TempTree::new("cycle-invalidate");
    let main_src = "use a;\npub fn main() -> i32 { return a::fa(); }";
    let entry = tree.write("main.inf", main_src);
    let a = tree.write("a.inf", "use b;\npub fn fa() -> i32 { return b::fb(); }");
    let b = tree.write("b.inf", "use a;\npub fn fb() -> i32 { return 1; }");
    let mut db = RootDatabase::default();
    db.open_document(&entry, main_src);

    let after_open = db.analysis(&entry).generation();

    // Editing the cyclic member `b` (reached transitively, itself importing `a`).
    db.change_document(&b, "use a;\npub fn fb() -> i32 { return 2; }");
    let after_b = db.analysis(&entry).generation();
    assert!(
        after_b > after_open,
        "editing a cycle member `b` must recompute main ({after_open} -> {after_b})"
    );

    // Editing the direct import `a`, the other member of the cycle.
    db.change_document(&a, "use b;\npub fn fa() -> i32 { return b::fb() + 1; }");
    let after_a = db.analysis(&entry).generation();
    assert!(
        after_a > after_b,
        "editing the other cycle member `a` must recompute main again ({after_b} -> {after_a})"
    );
    assert!(
        db.analysis(&entry).import_problems().is_empty(),
        "the cyclic project still resolves after the edits"
    );
}

// close_document disk-fallback with divergent overlay/disk content (issue #254):
// `overlay_text_beats_disk_contents` pins only the open direction (overlay wins
// while open). This pins the close direction: once the overlay is dropped, the
// next analysis must read the *disk* text, even when it diverges from what the
// buffer held.

#[test]
fn closing_a_document_falls_back_to_divergent_disk_content() {
    // Disk and overlay define different top-level functions. While open, the
    // analysis sees the overlay; after `didClose` drops the overlay, the next
    // analysis must recompute against the divergent disk text — proving the
    // fallback re-reads disk rather than serving the vanished buffer.
    let tree = TempTree::new("close-divergent-disk");
    let disk_src = "pub fn disk_fn() -> i32 { return 1; }";
    let overlay_src = "pub fn overlay_fn() -> i32 { return 2; }";
    let entry = tree.write("main.inf", disk_src);
    let mut db = RootDatabase::default();

    db.open_document(&entry, overlay_src);
    assert_eq!(
        def_names(&mut db, &entry, &[]),
        vec!["overlay_fn"],
        "while open, the overlay text wins over the disk text"
    );

    db.close_document(&entry);
    assert_eq!(
        def_names(&mut db, &entry, &[]),
        vec!["disk_fn"],
        "after close, the analysis falls back to the divergent disk content"
    );
}

#[test]
fn a_closed_dependent_reads_a_divergent_import_from_disk() {
    // The cross-file twin of the close fallback. `main` imports `lib`; `lib` is open
    // with overlay text that diverges from its disk text (a different function
    // name). While `lib` is open, `main`'s closure sees the overlay symbol; closing
    // `lib` must make `main` re-read `lib` from disk and see the disk symbol
    // instead — the still-open dependent falls back to divergent disk content.
    let tree = TempTree::new("close-dependent-divergent");
    let main_src = "use lib;\npub fn main() -> i32 { return 0; }";
    let entry = tree.write("main.inf", main_src);
    let lib = tree.write("lib.inf", "pub fn on_disk() -> i32 { return 1; }");
    let mut db = RootDatabase::default();
    db.open_document(&entry, main_src);
    db.open_document(&lib, "pub fn in_overlay() -> i32 { return 2; }");

    let lib_mod = vec!["lib".to_string()];
    assert!(
        closure_defines(db.analysis(&entry), &lib_mod, "in_overlay"),
        "while lib is open, main's closure sees the overlay symbol"
    );

    db.close_document(&lib);
    let analysis = db.analysis(&entry);
    assert!(
        closure_defines(analysis, &lib_mod, "on_disk"),
        "closing lib makes the open dependent re-read the divergent disk symbol"
    );
    assert!(
        !closure_defines(analysis, &lib_mod, "in_overlay"),
        "the vanished overlay symbol is no longer visible to the dependent"
    );
}

#[test]
fn a_cancelled_analysis_unwinds_cleanly_and_the_retry_recomputes() {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    // A cancellation requested before an analysis runs must unwind that analysis
    // (delivering the semantic layer's cancellation payload, which `is_cancellation`
    // recognizes and an unrelated payload does not), leave the entry un-analyzed so
    // no stale result is served, and let a retry recompute cleanly. The final memo
    // hit is the tripwire: the framework auto-resets the consumed cancellation on
    // the unwinding attempt's exit, so a later read is a cache hit — an internal
    // behavior a dependency upgrade could silently change.
    let path = PathBuf::from("/inf-test/cancellable.inf");
    let mut db = RootDatabase::default();
    db.open_document(&path, "pub fn f() -> i32 { return 1; }");

    let source = AnalysisCancelSource::detached();
    db.bind_cancellation(&source);
    let _epoch = source.request_cancellation();

    let unwound = catch_unwind(AssertUnwindSafe(|| {
        let _ = db.analysis(&path);
    }));
    let payload = unwound.expect_err("a pre-fired cancellation unwinds the analysis");
    assert!(
        is_cancellation(payload.as_ref()),
        "the caught payload is the semantic layer's cancellation signal"
    );
    let unrelated: Box<dyn std::any::Any + Send> = Box::new("an ordinary panic payload");
    assert!(
        !is_cancellation(unrelated.as_ref()),
        "a non-cancellation payload is not classified as a cancellation"
    );
    assert!(
        !db.is_analyzed(&path),
        "the cancelled compute left no memoized analysis behind"
    );

    // The consumed signal auto-reset, so the retry recomputes and memoizes.
    let generation = db.analysis(&path).generation();
    assert!(
        db.is_analyzed(&path),
        "the retry recomputes the previously-cancelled analysis"
    );
    assert_eq!(
        db.analysis(&path).generation(),
        generation,
        "a second read is a memo hit (equal generation), not a recompute"
    );
}
