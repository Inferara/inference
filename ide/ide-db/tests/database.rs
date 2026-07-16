//! Integration tests for the `RootDatabase` → `FileAnalysis` pipeline: closure
//! loading through the overlay-then-disk loader, closure-aware invalidation, and
//! the partial results a broken program still yields.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use inference_ide_db::{NodeId, RootDatabase, Severity};

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
