//! Multi-file project front end: walk the import-reachable closure of `.inf`
//! files from an entry point and lower them all into a single [`AstArena`].
//!
//! # Model
//!
//! The **source root** is the entry file's parent directory. A path-form `use`
//! directive names a file relative to that root: `use a::b;` and
//! `use a::b::{x, y};` both refer to `<root>/a/b.inf` (a braced item import names
//! the same file as the brace-free form — the braces only select items). The
//! `from`-form (`use … from M;`, external WASM modules) is not a source import
//! and is ignored here.
//!
//! Discovery is breadth-first with a visited set keyed by canonical module path,
//! so import cycles terminate. Each file is read and parsed exactly once — into
//! the shared arena as it is discovered. After the walk the files are reordered
//! into **canonical order** — entry first, then imported files sorted
//! lexicographically by module path — which downstream phases consume as their
//! single source of truth for ordering.
//!
//! All filesystem access lives here; `core/parser` stays I/O-free and is driven
//! through [`inference_parser::parse_into`].

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use anyhow::anyhow;
use inference_ast::arena::AstArena;
use inference_ast::nodes::{Directive, SourceFileData, UseDirective};
use rustc_hash::FxHashSet;

use crate::errors::InferenceError;

/// File extension of an Inference source file.
const SOURCE_EXTENSION: &str = "inf";

/// Maximum edit distance at which a sibling filename is offered as a
/// "did you mean" suggestion for a missing import.
const SUGGESTION_MAX_DISTANCE: usize = 2;

/// Outcome of parsing a project: the unified arena plus any non-fatal warnings.
///
/// Warnings are returned rather than printed so library code stays silent; the
/// caller (a CLI) decides how to surface them.
#[derive(Debug)]
#[must_use = "a project parse carries both the arena and any warnings"]
pub struct ProjectParse {
    /// All reachable files lowered into one arena, in canonical order.
    pub arena: AstArena,
    /// Non-fatal findings collected during the walk.
    pub warnings: Vec<ProjectWarning>,
}

/// A non-fatal finding from a project parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectWarning {
    /// A `.inf` file under the source root is reachable from no import chain and
    /// will therefore not be compiled.
    UnreachableFile { path: PathBuf },
}

impl std::fmt::Display for ProjectWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectWarning::UnreachableFile { path } => write!(
                f,
                "warning: `{}` is not imported by any reachable file and will not be compiled",
                path.display()
            ),
        }
    }
}

/// Parses a project starting from `entry`, returning one arena holding every
/// import-reachable file plus any unreachable-file warnings.
///
/// The source root is `entry`'s parent directory. The entry is parsed first;
/// its path-form `use` directives are followed breadth-first, each mapped to a
/// file under the root, until the reachable closure is exhausted. Each reachable
/// file is read and parsed exactly once, into the shared arena as it is
/// discovered. Import cycles are permitted and terminate via a visited set.
///
/// # Errors
///
/// Returns [`InferenceError::NoSourceRoot`] if `entry` has no parent directory,
/// [`InferenceError::Io`] if a file cannot be read,
/// [`InferenceError::InvalidImportSegment`] for a malformed `use` path segment,
/// [`InferenceError::ImportFileNotFound`] (with a nearest-match suggestion) for a
/// `use` naming a non-existent file, and [`InferenceError::ImportedFileParse`]
/// for a file with syntax errors.
pub fn parse_project(entry: &Path) -> anyhow::Result<ProjectParse> {
    let src_root = entry
        .parent()
        .ok_or_else(|| InferenceError::NoSourceRoot(entry.to_path_buf()))?
        .to_path_buf();

    let arena = parse_reachable_files(entry, &src_root)?;
    let warnings = collect_unreachable_warnings(&arena, &src_root, entry);

    Ok(ProjectParse { arena, warnings })
}

/// Breadth-first walk of the import closure, parsing every reachable file exactly
/// once into a shared arena. Each file is keyed by its canonical module path so a
/// file reached twice (including through a cycle) is parsed once.
///
/// Files accumulate in discovery (BFS) order; the walk ends by reordering them
/// into canonical order (see [`AstArena::canonicalize_source_file_order`]), the
/// single source of truth downstream phases consume for ordering.
fn parse_reachable_files(entry: &Path, src_root: &Path) -> anyhow::Result<AstArena> {
    let mut arena = AstArena::default();
    let mut visited: FxHashSet<Vec<String>> = FxHashSet::default();
    let mut queue: VecDeque<(Vec<String>, PathBuf)> = VecDeque::new();

    // The entry is the one file with an empty module path. It is keyed by the
    // empty segment list, but a `use main;` that resolves to the entry file
    // carries the segments `["main"]`, which the visited set would not catch — so
    // a path resolving to the entry file is recognized separately, below.
    let entry_canonical = std::fs::canonicalize(entry).ok();
    visited.insert(Vec::new());
    queue.push_back((Vec::new(), entry.to_path_buf()));

    while let Some((module_path, file_path)) = queue.pop_front() {
        let source = read_source(&file_path)?;
        // `module_path` is moved into the arena by `parse_into` but still needed by
        // the parse-error arm below, so clone it for the move.
        let parsed = inference_parser::parse_into(arena, &source, module_path.clone());
        arena = parsed.arena;

        // Surface a file's own syntax errors before resolving its imports. A
        // rejected `use a::b::*;` still lowers to the segments `a::b`, so without
        // this guard the glob would be probed as the file `a/b.inf`; when that
        // file is absent, the "file not found" lookup would mask the educational
        // glob diagnostic. Reporting the parse error first means the user sees why
        // their directive is invalid rather than a misleading missing-file path.
        if !parsed.errors.is_empty() {
            return Err(parse_error(&module_path, entry, &parsed.errors));
        }

        // `parse_into` lowers the file's `SourceFileData` after all of its defs and
        // directives, so the file just parsed is the last one stored (pinned by
        // `parse_into_allocates_the_new_file_last` in `core/parser`).
        let file = arena
            .last_source_file()
            .expect("parse_into stores the file it just lowered");

        for segments in path_form_imports(&arena, file)? {
            if visited.contains(&segments) {
                continue;
            }
            let dep_path = module_file_path(src_root, &segments);
            if !dep_path.is_file() {
                return Err(missing_import_error(&segments, &dep_path));
            }
            // A `use` that names the entry file itself (e.g. `use main;` when the
            // entry is `src/main.inf`) is a self-import: the entry is already in
            // the closure under the empty module path, so re-discovering it here
            // would lower its definitions into the arena twice and emit every
            // entry function twice. Skip it, mirroring the reserved `use root;`
            // handle. The reserved-handle name is the intended way to reach the
            // entry; a literal self-import resolving to it is just deduplicated.
            // Canonicalization failures fall through to normal handling so a real
            // distinct file is never wrongly dropped.
            if let (Some(entry_path), Ok(dep_canonical)) =
                (entry_canonical.as_ref(), std::fs::canonicalize(&dep_path))
                && *entry_path == dep_canonical
            {
                continue;
            }
            visited.insert(segments.clone());
            queue.push_back((segments, dep_path));
        }
    }

    // Files were parsed in discovery (BFS) order; downstream phases consume
    // canonical order as their single source of truth, so reorder now — before any
    // `SourceFileId` is handed out.
    arena.canonicalize_source_file_order();

    // Discovery deduplicates files by module path (and self-imports of the entry),
    // so each file appears exactly once. A duplicate would lower the same
    // definitions twice and emit them twice in codegen; assert the invariant so a
    // future discovery regression is caught here rather than in the output.
    debug_assert!(
        arena
            .source_files()
            .collect::<Vec<_>>()
            .windows(2)
            .all(|w| w[0].module_path != w[1].module_path),
        "discovered files must have unique module paths after deduplication"
    );

    Ok(arena)
}

/// Builds a parse-failure error for `errors`. An imported (non-entry) file is
/// named by its canonical `module_path`; the entry file is named by its real
/// `entry` path with non-"imported" wording, because it is the file the user
/// compiled.
fn parse_error(
    module_path: &[String],
    entry: &Path,
    errors: &[inference_parser::ParseError],
) -> anyhow::Error {
    let details = errors
        .iter()
        .map(|error| {
            format!(
                "  {}:{}: {}",
                error.span.start_line, error.span.start_column, error.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    // The entry file has the empty module path. `file_label` returns `None` for
    // it, signalling the entry case: report it with its real path so the user is
    // pointed at the file they named rather than at the `<entry>` placeholder.
    match inference_ast::nodes::file_label(module_path) {
        Some(label) => anyhow!(InferenceError::ImportedFileParse {
            module_path: label,
            details,
        }),
        None => anyhow!(InferenceError::EntryFileParse {
            path: entry.to_path_buf(),
            details,
        }),
    }
}

/// Extracts the path-form `use` imports of an already-parsed file as canonical
/// module-path segment lists. A braced item import (`use a::b::{x};`) maps to the
/// file `a::b` — the segments *before* the brace list. The `from`-form is skipped.
///
/// The caller passes the just-lowered file explicitly, because the shared arena
/// holds every file walked so far; `arena` is still needed to resolve the
/// directives' segment identifiers. The caller parses the file and surfaces any
/// syntax errors first, so only the directive shapes of a cleanly-parsed file
/// reach here.
fn path_form_imports(
    arena: &AstArena,
    source_file: &SourceFileData,
) -> anyhow::Result<Vec<Vec<String>>> {
    let mut imports = Vec::new();
    for directive in &source_file.directives {
        let Directive::Use(use_dir) = directive;
        if use_dir.from.is_some() {
            continue;
        }
        let segments = use_directive_segments(arena, use_dir)?;
        // `use root;` / `use root::{x};` is the reserved handle for the program
        // entry file (Inference's `@import("root")`), not a file to load: the entry
        // is already in the closure. A literal `src/root.inf` is shadowed by the
        // reserved name and would surface as an unreachable-file warning instead.
        if is_root_handle(&segments) {
            continue;
        }
        if !segments.is_empty() {
            imports.push(segments);
        }
    }
    Ok(imports)
}

/// Resolves a path-form `use` directive's segment identifiers to validated owned
/// strings. Rejects a segment that is not a usable file/directory name.
fn use_directive_segments(
    arena: &AstArena,
    use_dir: &UseDirective,
) -> anyhow::Result<Vec<String>> {
    let mut segments = Vec::with_capacity(use_dir.segments.len());
    for &ident in &use_dir.segments {
        let segment = arena.ident_name(ident).to_string();
        if !is_valid_segment(&segment) {
            let referenced_as = use_dir
                .segments
                .iter()
                .map(|&id| arena.ident_name(id))
                .collect::<Vec<_>>()
                .join("::");
            return Err(anyhow!(InferenceError::InvalidImportSegment {
                referenced_as,
                segment,
            }));
        }
        segments.push(segment);
    }
    Ok(segments)
}

/// Whether `segments` is the reserved single-segment `root` handle — the entry
/// file (Inference's `@import("root")`) — which names no file on disk.
fn is_root_handle(segments: &[String]) -> bool {
    segments.len() == 1 && segments[0] == "root"
}

/// Whether `segment` is a plain file/directory name usable in a filesystem path.
fn is_valid_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && !segment.contains('/')
        && !segment.contains('\\')
}

/// Maps canonical module-path segments to the file they name under `src_root`:
/// `["lib", "arith"]` ⇒ `<src_root>/lib/arith.inf`.
fn module_file_path(src_root: &Path, segments: &[String]) -> PathBuf {
    let mut path = src_root.to_path_buf();
    for segment in segments {
        path.push(segment);
    }
    path.set_extension(SOURCE_EXTENSION);
    path
}

/// Reads a source file into a string, mapping IO failures to [`InferenceError::Io`].
fn read_source(path: &Path) -> anyhow::Result<String> {
    std::fs::read_to_string(path).map_err(|source| {
        anyhow!(InferenceError::Io {
            path: path.to_path_buf(),
            source,
        })
    })
}

/// Builds a missing-import error, offering the nearest sibling `.inf` stem as a
/// suggestion when one is within [`SUGGESTION_MAX_DISTANCE`] edits. The
/// suggestion is searched in the directory the missing file would have lived in.
fn missing_import_error(segments: &[String], expected_path: &Path) -> anyhow::Error {
    let referenced_as = segments.join("::");
    let suggestion = expected_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|target| nearest_sibling(expected_path, target));
    anyhow!(InferenceError::ImportFileNotFound {
        referenced_as,
        expected_path: expected_path.to_path_buf(),
        suggestion,
    })
}

/// Finds the closest-named sibling `.inf` file to `target` (the missing file's
/// stem) in the directory `missing` would have lived in, by edit distance.
fn nearest_sibling(missing: &Path, target: &str) -> Option<String> {
    let dir = missing.parent()?;
    let entries = std::fs::read_dir(dir).ok()?;

    // Collect the candidate stems and sort them so the suggestion is stable when
    // two siblings tie on edit distance. `read_dir` yields entries in an
    // OS-dependent order, so without this the strict `<` below would otherwise
    // pick whichever tied stem the OS happened to surface first.
    let mut stems: Vec<String> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some(SOURCE_EXTENSION))
        .filter_map(|path| path.file_stem().and_then(|s| s.to_str()).map(str::to_string))
        .collect();
    stems.sort();

    let mut best: Option<(usize, String)> = None;
    for stem in stems {
        let distance = edit_distance(target, &stem);
        if distance == 0 {
            continue;
        }
        if best.as_ref().is_none_or(|(d, _)| distance < *d) {
            best = Some((distance, stem));
        }
    }

    best.filter(|(d, _)| *d <= SUGGESTION_MAX_DISTANCE)
        .map(|(_, name)| name)
}

/// Levenshtein edit distance between two strings (insert/delete/substitute), used
/// to rank near-miss filename suggestions. Operates on chars to stay
/// Unicode-correct for non-ASCII filenames.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];

    for (i, &ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// Enumerates every `.inf` file under `src_root` and warns about those reached by
/// no import chain (i.e. not present in the arena's canonical file set). The
/// entry file is always reachable and never warned about.
fn collect_unreachable_warnings(
    arena: &AstArena,
    src_root: &Path,
    entry: &Path,
) -> Vec<ProjectWarning> {
    let reachable: FxHashSet<PathBuf> = arena
        .source_files()
        .map(|sf| {
            if sf.module_path.is_empty() {
                entry.to_path_buf()
            } else {
                module_file_path(src_root, &sf.module_path)
            }
        })
        // Fall back to the un-canonicalized path rather than dropping the entry:
        // a reachable (compiled) file whose canonicalization fails must still land
        // in this set, or the on-disk scan below — which uses the same fallback —
        // would flag it as unreachable when it was actually built.
        .map(|path| std::fs::canonicalize(&path).unwrap_or(path))
        .collect();

    let mut warnings = Vec::new();
    let mut on_disk = enumerate_source_files(src_root);
    // Sort so the warning order is deterministic regardless of directory-read
    // order (which the OS does not guarantee).
    on_disk.sort();
    for path in on_disk {
        let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if !reachable.contains(&canonical) {
            warnings.push(ProjectWarning::UnreachableFile { path });
        }
    }
    warnings
}

/// Recursively collects every `.inf` file under `root`. Returns an empty vector
/// if `root` is unreadable, so an unreachable-file scan failure never aborts an
/// otherwise-successful parse.
fn enumerate_source_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some(SOURCE_EXTENSION) {
                out.push(path);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway source tree under the system temp dir, removed on drop.
    struct TempProject {
        root: PathBuf,
    }

    impl TempProject {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "inference-project-{tag}-{}-{nanos}",
                std::process::id()
            ));
            std::fs::create_dir_all(&root).expect("create temp project root");
            TempProject { root }
        }

        /// Writes `source` to `<root>/<relative>`, creating parent directories,
        /// and returns the absolute path.
        fn write(&self, relative: &str, source: &str) -> PathBuf {
            let dest = self.root.join(relative);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).expect("create source parent dir");
            }
            std::fs::write(&dest, source).expect("write source file");
            dest
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// The canonical module paths of an arena's files, in stored order.
    fn module_paths(parse: &ProjectParse) -> Vec<Vec<String>> {
        parse
            .arena
            .source_files()
            .map(|sf| sf.module_path.clone())
            .collect()
    }

    #[test]
    fn single_file_is_entry_with_empty_module_path() {
        let project = TempProject::new("single");
        let entry = project.write("main.inf", "pub fn main() -> i32 { return 0; }");

        let parse = parse_project(&entry).expect("single file parses");

        assert_eq!(module_paths(&parse), vec![Vec::<String>::new()]);
        assert!(parse.arena.source_files().next().unwrap().is_entry());
        assert!(parse.warnings.is_empty());
    }

    #[test]
    fn three_file_project_parses_into_one_arena() {
        let project = TempProject::new("three");
        let entry = project.write(
            "main.inf",
            "use math;\npub fn main() -> i32 { return 0; }",
        );
        project.write("math.inf", "use lib::arith;\npub fn foo() {}");
        project.write("lib/arith.inf", "pub fn add(a: i32, b: i32) -> i32 { return a + b; }");

        let parse = parse_project(&entry).expect("project parses");

        // Entry first, then imported files sorted lexicographically by path:
        // ["lib","arith"] < ["math"].
        assert_eq!(
            module_paths(&parse),
            vec![
                Vec::<String>::new(),
                vec!["lib".to_string(), "arith".to_string()],
                vec!["math".to_string()],
            ]
        );
        assert!(parse.warnings.is_empty());
    }

    #[test]
    fn import_cycle_terminates() {
        let project = TempProject::new("cycle");
        let entry = project.write("main.inf", "use a;\npub fn main() {}");
        project.write("a.inf", "use b;\npub fn fa() {}");
        project.write("b.inf", "use a;\npub fn fb() {}");

        let parse = parse_project(&entry).expect("cyclic imports terminate and parse");

        assert_eq!(
            module_paths(&parse),
            vec![
                Vec::<String>::new(),
                vec!["a".to_string()],
                vec!["b".to_string()],
            ]
        );
    }

    #[test]
    fn shared_dependency_parsed_once() {
        let project = TempProject::new("shared");
        let entry = project.write("main.inf", "use a;\nuse b;\npub fn main() {}");
        project.write("a.inf", "use common;\npub fn fa() {}");
        project.write("b.inf", "use common;\npub fn fb() {}");
        project.write("common.inf", "pub fn shared() {}");

        let parse = parse_project(&entry).expect("project parses");

        let common_count = parse
            .arena
            .source_files()
            .filter(|sf| sf.module_path == vec!["common".to_string()])
            .count();
        assert_eq!(common_count, 1, "a shared dependency is parsed exactly once");
    }

    #[test]
    fn braced_item_import_maps_to_file() {
        let project = TempProject::new("braced");
        let entry = project.write(
            "main.inf",
            "use lib::arith::{add};\npub fn main() {}",
        );
        project.write("lib/arith.inf", "pub fn add(a: i32, b: i32) -> i32 { return a + b; }");

        let parse = parse_project(&entry).expect("braced item import resolves to a file");

        assert_eq!(
            module_paths(&parse),
            vec![
                Vec::<String>::new(),
                vec!["lib".to_string(), "arith".to_string()],
            ]
        );
    }

    #[test]
    fn missing_import_file_errors_with_expected_path() {
        let project = TempProject::new("missing");
        let entry = project.write("main.inf", "use nope;\npub fn main() {}");

        let err = parse_project(&entry).expect_err("missing import must error");
        let inference_err = err
            .downcast_ref::<InferenceError>()
            .expect("error is an InferenceError");
        match inference_err {
            InferenceError::ImportFileNotFound {
                referenced_as,
                expected_path,
                ..
            } => {
                assert_eq!(referenced_as, "nope");
                assert!(expected_path.ends_with("nope.inf"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn syntax_error_in_imported_file_reports_module_name() {
        // A broken imported file is still discovered (the resilient parser
        // produces directive shapes), then rejected when it is lowered, named by
        // its module path rather than the opaque entry placeholder.
        let project = TempProject::new("imported-parse");
        let entry = project.write("main.inf", "use lib::broken;\npub fn main() {}");
        project.write("lib/broken.inf", "pub fn oops( { return 1; }");

        let err = parse_project(&entry).expect_err("a syntax error in an imported file must error");
        let inference_err = err
            .downcast_ref::<InferenceError>()
            .expect("error is an InferenceError");
        match inference_err {
            InferenceError::ImportedFileParse {
                module_path,
                details,
            } => {
                assert_eq!(module_path, "lib::broken");
                assert!(!details.is_empty(), "the syntax errors must be reported");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn glob_import_surfaces_educational_message_over_missing_file() {
        // `use a::b::*;` lowers to the segments `a::b` (the glob `*` is rejected
        // after them), so it would otherwise be probed as the file `a/b.inf`.
        // With that file absent, discovery must report the file's own parse error
        // (the educational glob message) rather than a misleading "file not found"
        // for `a/b.inf` — the user wrote a glob, not a path import.
        let project = TempProject::new("glob-missing");
        let entry = project.write("main.inf", "use a::b::*;\npub fn main() {}");

        let err = parse_project(&entry).expect_err("a glob import must error");
        let inference_err = err
            .downcast_ref::<InferenceError>()
            .expect("error is an InferenceError");
        match inference_err {
            // The entry file's own parse error is surfaced through the entry
            // template (named by its real path), not the imported-file wording.
            InferenceError::EntryFileParse { path, details } => {
                assert_eq!(path, &entry);
                assert!(
                    details.contains("glob imports are not supported"),
                    "the educational glob message must be surfaced, got: {details}"
                );
            }
            InferenceError::ImportFileNotFound { .. } => {
                panic!("the missing-file lookup masked the glob diagnostic");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn syntax_error_in_entry_file_reports_real_path_not_imported_wording() {
        let project = TempProject::new("entry-parse");
        let entry = project.write("main.inf", "pub fn main( { return 0; }");

        let err = parse_project(&entry).expect_err("a syntax error in the entry must error");
        let inference_err = err.downcast_ref::<InferenceError>().unwrap();
        match inference_err {
            // The entry must name its real path and must NOT be reported as an
            // "imported file" — it is the file the user compiled.
            InferenceError::EntryFileParse { path, .. } => {
                assert_eq!(path, &entry);
                assert!(
                    !err.to_string().contains("imported file"),
                    "the entry parse error must not use the imported-file wording, got: {err}"
                );
                assert!(
                    err.to_string().contains(&entry.display().to_string()),
                    "the entry parse error must name the real entry path, got: {err}"
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn missing_import_suggests_near_match() {
        let project = TempProject::new("suggest");
        let entry = project.write("main.inf", "use arith;\npub fn main() {}");
        // A sibling one edit away from the missing `arith.inf`.
        project.write("arithh.inf", "pub fn add(a: i32, b: i32) -> i32 { return a + b; }");

        let err = parse_project(&entry).expect_err("missing import must error");
        let inference_err = err.downcast_ref::<InferenceError>().unwrap();
        match inference_err {
            InferenceError::ImportFileNotFound { suggestion, .. } => {
                assert_eq!(suggestion.as_deref(), Some("arithh"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn suggestion_is_deterministic_on_distance_tie() {
        // Two siblings are an equal edit distance from the missing `arith.inf`
        // (`brith` and `zrith`, each one substitution away). `read_dir` order is
        // OS-dependent, so the suggestion must be pinned by sorting candidates:
        // the lexicographically first tied stem (`brith`) always wins. Run the
        // resolution repeatedly to catch any order-dependence.
        let project = TempProject::new("suggest-tie");
        let entry = project.write("main.inf", "use arith;\npub fn main() {}");
        project.write("brith.inf", "pub fn b() {}");
        project.write("zrith.inf", "pub fn z() {}");

        for _ in 0..16 {
            let err = parse_project(&entry).expect_err("missing import must error");
            let inference_err = err.downcast_ref::<InferenceError>().unwrap();
            match inference_err {
                InferenceError::ImportFileNotFound { suggestion, .. } => {
                    assert_eq!(
                        suggestion.as_deref(),
                        Some("brith"),
                        "tie must resolve to the lexicographically first sibling"
                    );
                }
                other => panic!("unexpected error: {other:?}"),
            }
        }
    }

    #[test]
    fn segment_validation_rejects_filesystem_traversal() {
        // The lexer only ever produces plain identifiers, so an invalid segment
        // is not reachable from valid source; the predicate is a fail-safe that
        // keeps a `use` from ever naming `.`, `..`, or a separator-bearing path.
        assert!(!is_valid_segment(""));
        assert!(!is_valid_segment("."));
        assert!(!is_valid_segment(".."));
        assert!(!is_valid_segment("a/b"));
        assert!(!is_valid_segment("a\\b"));
        assert!(is_valid_segment("arith"));
        assert!(is_valid_segment("_private"));
    }

    #[test]
    fn unreachable_file_warns() {
        let project = TempProject::new("unreachable");
        let entry = project.write("main.inf", "pub fn main() {}");
        let orphan = project.write("orphan.inf", "pub fn orphan() {}");

        let parse = parse_project(&entry).expect("project parses");

        assert_eq!(parse.warnings.len(), 1, "exactly one unreachable file");
        let ProjectWarning::UnreachableFile { path } = &parse.warnings[0];
        assert_eq!(
            std::fs::canonicalize(path).unwrap(),
            std::fs::canonicalize(&orphan).unwrap()
        );
    }

    #[test]
    fn entry_named_main_among_imports_still_qualified() {
        // An imported file literally named `main.inf` (in a subdirectory) must
        // receive its real module path, never the empty entry path.
        let project = TempProject::new("main-name");
        let entry = project.write("app.inf", "use sub::main;\npub fn main() {}");
        project.write("sub/main.inf", "pub fn helper() {}");

        let parse = parse_project(&entry).expect("project parses");

        assert_eq!(
            module_paths(&parse),
            vec![
                Vec::<String>::new(),
                vec!["sub".to_string(), "main".to_string()],
            ]
        );
        // The single entry is `app.inf`, not the imported `sub/main.inf`.
        let entry_files = parse
            .arena
            .source_files()
            .filter(|sf| sf.is_entry())
            .count();
        assert_eq!(entry_files, 1);
    }

    #[test]
    fn canonical_order_independent_of_discovery_order() {
        // Two projects with the same files but imports listed in different orders
        // must produce the same canonical file order.
        let forward = TempProject::new("order-fwd");
        let fwd_entry = forward.write("main.inf", "use a;\nuse b;\npub fn main() {}");
        forward.write("a.inf", "pub fn fa() {}");
        forward.write("b.inf", "pub fn fb() {}");

        let backward = TempProject::new("order-bwd");
        let bwd_entry = backward.write("main.inf", "use b;\nuse a;\npub fn main() {}");
        backward.write("a.inf", "pub fn fa() {}");
        backward.write("b.inf", "pub fn fb() {}");

        let fwd = parse_project(&fwd_entry).expect("forward parses");
        let bwd = parse_project(&bwd_entry).expect("backward parses");

        assert_eq!(module_paths(&fwd), module_paths(&bwd));
        assert_eq!(
            module_paths(&fwd),
            vec![
                Vec::<String>::new(),
                vec!["a".to_string()],
                vec!["b".to_string()],
            ]
        );
    }

    #[test]
    fn from_form_import_is_not_a_source_dependency() {
        // `use { x } from M;` is an external WASM import, not a file import, so it
        // must not be followed as a source dependency.
        let project = TempProject::new("from-form");
        let entry = project.write(
            "main.inf",
            "use { sort } from sorting;\npub fn main() {}",
        );

        let parse = parse_project(&entry).expect("from-form does not trigger file resolution");
        assert_eq!(module_paths(&parse), vec![Vec::<String>::new()]);
    }

    #[test]
    fn edit_distance_basic() {
        assert_eq!(edit_distance("arith", "arith"), 0);
        assert_eq!(edit_distance("arith", "arithh"), 1);
        assert_eq!(edit_distance("arith", "airth"), 2);
        assert_eq!(edit_distance("", "abc"), 3);
    }

    // Comprehensive matrix. Broadens the smoke tests above
    // along axes they do not cover: deep nesting, diamond/cycle combos,
    // self-import, multi-file canonical ordering, `pub use` discovery, mixed
    // directives, missing-file edge cases, and unreachable-warning edge cases.

    /// Convenience constructor for an owned `["a", "b", ...]` module path.
    fn mp(segments: &[&str]) -> Vec<String> {
        segments.iter().map(ToString::to_string).collect()
    }

    // Axis: deep nesting

    #[test]
    fn three_directory_levels_resolve() {
        // `use a::b::c;` must map to `<root>/a/b/c.inf`, three directories deep.
        let project = TempProject::new("deep-three");
        let entry = project.write("main.inf", "use a::b::c;\npub fn main() {}");
        project.write("a/b/c.inf", "pub fn deep() {}");

        let parse = parse_project(&entry).expect("three-level path resolves");

        assert_eq!(
            module_paths(&parse),
            vec![Vec::<String>::new(), mp(&["a", "b", "c"])],
        );
        assert!(parse.warnings.is_empty());
    }

    #[test]
    fn multiple_files_in_same_directory() {
        // Several siblings in one nested directory are each pulled in by path.
        let project = TempProject::new("same-dir");
        let entry = project.write(
            "main.inf",
            "use lib::a;\nuse lib::b;\nuse lib::c;\npub fn main() {}",
        );
        project.write("lib/a.inf", "pub fn fa() {}");
        project.write("lib/b.inf", "pub fn fb() {}");
        project.write("lib/c.inf", "pub fn fc() {}");

        let parse = parse_project(&entry).expect("sibling files in one dir resolve");

        assert_eq!(
            module_paths(&parse),
            vec![
                Vec::<String>::new(),
                mp(&["lib", "a"]),
                mp(&["lib", "b"]),
                mp(&["lib", "c"]),
            ],
        );
    }

    #[test]
    fn sibling_import_from_nested_file_is_src_root_relative() {
        // Paths are resolved from the SRC ROOT regardless of the importer's
        // location: `use lib::arith;` written inside `deep/inner.inf` must still
        // resolve to `<root>/lib/arith.inf`, not `<root>/deep/lib/arith.inf`.
        let project = TempProject::new("nested-sibling");
        let entry = project.write("main.inf", "use deep::inner;\npub fn main() {}");
        project.write("deep/inner.inf", "use lib::arith;\npub fn inner() {}");
        project.write(
            "lib/arith.inf",
            "pub fn add(a: i32, b: i32) -> i32 { return a + b; }",
        );

        let parse = parse_project(&entry).expect("src-root-relative resolution from nested file");

        assert_eq!(
            module_paths(&parse),
            vec![
                Vec::<String>::new(),
                mp(&["deep", "inner"]),
                mp(&["lib", "arith"]),
            ],
        );
        assert!(
            parse.warnings.is_empty(),
            "every file is reachable, so no unreachable warning"
        );
    }

    // Axis: diamond + cycle combos

    #[test]
    fn diamond_dependency_parses_shared_apex_once() {
        // A -> B -> D, A -> C -> D, plus the direct A -> D edge (diamond). D is
        // reached by three paths but parsed once.
        let project = TempProject::new("diamond");
        let entry = project.write(
            "main.inf",
            "use b;\nuse c;\nuse d;\npub fn main() {}",
        );
        project.write("b.inf", "use d;\npub fn fb() {}");
        project.write("c.inf", "use d;\npub fn fc() {}");
        project.write("d.inf", "pub fn fd() {}");

        let parse = parse_project(&entry).expect("diamond parses");

        let d_count = parse
            .arena
            .source_files()
            .filter(|sf| sf.module_path == mp(&["d"]))
            .count();
        assert_eq!(d_count, 1, "the diamond apex is parsed exactly once");
        assert_eq!(
            module_paths(&parse),
            vec![
                Vec::<String>::new(),
                mp(&["b"]),
                mp(&["c"]),
                mp(&["d"]),
            ],
        );
    }

    #[test]
    fn two_node_cycle_with_third_file_terminates() {
        // A -> B -> A (a back-edge cycle) while A also pulls in an acyclic C.
        // The visited set makes the cycle terminate and C is still discovered.
        let project = TempProject::new("cycle-plus-third");
        let entry = project.write("main.inf", "use a;\nuse c;\npub fn main() {}");
        project.write("a.inf", "use b;\npub fn fa() {}");
        project.write("b.inf", "use a;\npub fn fb() {}");
        project.write("c.inf", "pub fn fc() {}");

        let parse = parse_project(&entry).expect("cycle with a third file terminates");

        assert_eq!(
            module_paths(&parse),
            vec![
                Vec::<String>::new(),
                mp(&["a"]),
                mp(&["b"]),
                mp(&["c"]),
            ],
        );
    }

    #[test]
    fn self_import_is_accepted_and_parsed_once() {
        // `use myself;` written inside `myself.inf`. PINNED BEHAVIOR: a file
        // importing itself is accepted (file cycles are legal) and the
        // file appears exactly once in the arena. The entry's own self-edge is
        // a no-op because the entry's empty module path is already visited.
        let project = TempProject::new("self-import");
        let entry = project.write("main.inf", "use myself;\npub fn main() {}");
        project.write("myself.inf", "use myself;\npub fn loops() {}");

        let parse = parse_project(&entry).expect("self-import is accepted");

        let myself_count = parse
            .arena
            .source_files()
            .filter(|sf| sf.module_path == mp(&["myself"]))
            .count();
        assert_eq!(myself_count, 1, "a self-importing file is parsed once");
        assert_eq!(
            module_paths(&parse),
            vec![Vec::<String>::new(), mp(&["myself"])],
        );
    }

    // Axis: canonical order

    #[test]
    fn five_files_sorted_lexicographically_regardless_of_discovery() {
        // Discovery order (the `use` list) is deliberately the reverse of the
        // canonical lexicographic order. The stored arena order must still be
        // entry-first then lexicographic by module path.
        let project = TempProject::new("five-sorted");
        let entry = project.write(
            "main.inf",
            "use zebra;\nuse mango;\nuse delta;\nuse charlie;\nuse alpha;\npub fn main() {}",
        );
        for name in ["alpha", "charlie", "delta", "mango", "zebra"] {
            project.write(&format!("{name}.inf"), "pub fn f() {}");
        }

        let parse = parse_project(&entry).expect("five files parse");

        assert_eq!(
            module_paths(&parse),
            vec![
                Vec::<String>::new(),
                mp(&["alpha"]),
                mp(&["charlie"]),
                mp(&["delta"]),
                mp(&["mango"]),
                mp(&["zebra"]),
            ],
        );
    }

    #[test]
    fn arena_order_is_byte_stable_across_two_runs() {
        // Determinism: the same project parsed twice yields identical canonical
        // order. (The smoke suite checks two *different* discovery orders agree;
        // this pins that a single project is stable run-to-run, guarding against
        // hash-set iteration leaking into the stored order.)
        let project = TempProject::new("stable");
        let entry = project.write(
            "main.inf",
            "use b;\nuse a;\nuse lib::z;\npub fn main() {}",
        );
        project.write("a.inf", "pub fn fa() {}");
        project.write("b.inf", "pub fn fb() {}");
        project.write("lib/z.inf", "pub fn fz() {}");

        let first = parse_project(&entry).expect("first run parses");
        let second = parse_project(&entry).expect("second run parses");

        assert_eq!(module_paths(&first), module_paths(&second));
        assert_eq!(
            module_paths(&first),
            vec![
                Vec::<String>::new(),
                mp(&["a"]),
                mp(&["b"]),
                mp(&["lib", "z"]),
            ],
        );
    }

    #[test]
    fn entry_sorts_first_even_when_its_name_sorts_last() {
        // The entry's identity is its empty module path, not its filename. An
        // entry file literally named `zzz.inf` (which would sort last among the
        // imported names) must still be stored first.
        let project = TempProject::new("entry-last-name");
        let entry = project.write("zzz.inf", "use aaa;\nuse mmm;\npub fn main() {}");
        project.write("aaa.inf", "pub fn fa() {}");
        project.write("mmm.inf", "pub fn fm() {}");

        let parse = parse_project(&entry).expect("project parses");

        let paths = module_paths(&parse);
        assert!(
            paths[0].is_empty(),
            "the entry (empty path) is stored first regardless of filename"
        );
        assert_eq!(
            paths,
            vec![Vec::<String>::new(), mp(&["aaa"]), mp(&["mmm"])],
        );
        assert!(parse.arena.source_files().next().unwrap().is_entry());
    }

    #[test]
    fn nested_paths_sort_below_their_first_segment() {
        // Lexicographic ordering on the segment vectors: ["lib"] is not a file
        // here, but ["lib","a"] < ["lib","b"] < ["zed"], and a top-level
        // ["alpha"] sorts before any ["lib", _].
        let project = TempProject::new("nested-order");
        let entry = project.write(
            "main.inf",
            "use zed;\nuse lib::b;\nuse lib::a;\nuse alpha;\npub fn main() {}",
        );
        project.write("alpha.inf", "pub fn fa() {}");
        project.write("lib/a.inf", "pub fn la() {}");
        project.write("lib/b.inf", "pub fn lb() {}");
        project.write("zed.inf", "pub fn fz() {}");

        let parse = parse_project(&entry).expect("project parses");

        assert_eq!(
            module_paths(&parse),
            vec![
                Vec::<String>::new(),
                mp(&["alpha"]),
                mp(&["lib", "a"]),
                mp(&["lib", "b"]),
                mp(&["zed"]),
            ],
        );
    }

    #[test]
    fn defs_stay_attached_to_their_files_when_bfs_and_canonical_orders_differ() {
        // Discovery visits entry -> zebra -> alpha (BFS), but the canonical order
        // is entry, alpha, zebra, so the post-walk reorder genuinely moves files.
        // After it, every file's `defs` must still resolve to that file's own
        // function: a reorder that shuffled files without keeping their defs would
        // cross-wire them, and this is the only test targeting that failure mode.
        let project = TempProject::new("bfs-vs-canonical");
        let entry = project.write("main.inf", "use zebra;\npub fn main() {}");
        project.write("zebra.inf", "use alpha;\npub fn zebra_fn() {}");
        project.write("alpha.inf", "pub fn alpha_fn() {}");

        let parse = parse_project(&entry).expect("project parses");

        assert_eq!(
            module_paths(&parse),
            vec![Vec::<String>::new(), mp(&["alpha"]), mp(&["zebra"])],
        );

        let def_names = |module_path: Vec<String>| -> Vec<String> {
            let file = parse
                .arena
                .source_files()
                .find(|sf| sf.module_path == module_path)
                .expect("file present in arena");
            file.defs
                .iter()
                .map(|&def_id| parse.arena.def_name(def_id).to_string())
                .collect()
        };

        assert_eq!(def_names(Vec::new()), vec!["main".to_string()]);
        assert_eq!(def_names(mp(&["alpha"])), vec!["alpha_fn".to_string()]);
        assert_eq!(def_names(mp(&["zebra"])), vec!["zebra_fn".to_string()]);
    }

    // Axis: `pub use` in the closure walk

    #[test]
    fn pub_use_pulls_file_into_closure() {
        // A `pub use` re-export must drive discovery exactly like a plain `use`:
        // `math.inf` re-exports `lib::arith`, so `arith` is in the closure even
        // though only `main` -> `math` is a plain import.
        let project = TempProject::new("pub-use");
        let entry = project.write("main.inf", "use math;\npub fn main() {}");
        project.write("math.inf", "pub use lib::arith;\npub fn foo() {}");
        project.write(
            "lib/arith.inf",
            "pub fn add(a: i32, b: i32) -> i32 { return a + b; }",
        );

        let parse = parse_project(&entry).expect("pub use drives discovery");

        assert_eq!(
            module_paths(&parse),
            vec![
                Vec::<String>::new(),
                mp(&["lib", "arith"]),
                mp(&["math"]),
            ],
        );
        assert!(
            parse.warnings.is_empty(),
            "the re-exported file is reachable, so no unreachable warning"
        );
    }

    #[test]
    fn pub_use_braced_item_form_pulls_file() {
        // `pub use lib::arith::{add};` re-exports items but still names the file
        // `lib/arith.inf` for discovery, identically to the brace-free form.
        let project = TempProject::new("pub-use-braced");
        let entry = project.write("main.inf", "use math;\npub fn main() {}");
        project.write("math.inf", "pub use lib::arith::{add};\npub fn foo() {}");
        project.write(
            "lib/arith.inf",
            "pub fn add(a: i32, b: i32) -> i32 { return a + b; }",
        );

        let parse = parse_project(&entry).expect("braced pub use drives discovery");

        assert_eq!(
            module_paths(&parse),
            vec![
                Vec::<String>::new(),
                mp(&["lib", "arith"]),
                mp(&["math"]),
            ],
        );
    }

    // Axis: mixed directives in one file

    #[test]
    fn mixed_directive_forms_only_path_form_drives_discovery() {
        // One file carrying all three shapes: a path-form file import, a braced
        // path-form item import, and a from-form external. Only the two
        // path-form directives name source files; the `from`-form is skipped.
        let project = TempProject::new("mixed");
        let entry = project.write(
            "main.inf",
            "use plain;\nuse lib::arith::{add};\nuse { sort } from sorting;\npub fn main() {}",
        );
        project.write("plain.inf", "pub fn fp() {}");
        project.write(
            "lib/arith.inf",
            "pub fn add(a: i32, b: i32) -> i32 { return a + b; }",
        );

        let parse = parse_project(&entry).expect("mixed directives parse");

        // No `sorting.inf` file exists, yet there is no missing-file error,
        // proving the from-form was never resolved as a source dependency.
        assert_eq!(
            module_paths(&parse),
            vec![
                Vec::<String>::new(),
                mp(&["lib", "arith"]),
                mp(&["plain"]),
            ],
        );
    }

    #[test]
    fn pub_use_and_plain_use_mix_in_one_file() {
        // A file mixing `pub use` (re-export) and plain `use` (private) — both
        // path forms drive discovery regardless of visibility.
        let project = TempProject::new("mix-vis");
        let entry = project.write(
            "main.inf",
            "pub use exported;\nuse internal;\npub fn main() {}",
        );
        project.write("exported.inf", "pub fn fe() {}");
        project.write("internal.inf", "pub fn fi() {}");

        let parse = parse_project(&entry).expect("mixed-visibility imports parse");

        assert_eq!(
            module_paths(&parse),
            vec![
                Vec::<String>::new(),
                mp(&["exported"]),
                mp(&["internal"]),
            ],
        );
    }

    // Axis: missing-file edge cases

    #[test]
    fn suggestion_offered_at_distance_exactly_two() {
        // The nearest sibling is exactly `SUGGESTION_MAX_DISTANCE` (2) edits
        // away — the boundary case that must still produce a suggestion.
        let project = TempProject::new("suggest-two");
        let entry = project.write("main.inf", "use arith;\npub fn main() {}");
        // "arith" -> "airth": two substitutions (transposition counts as two).
        project.write("airth.inf", "pub fn f() {}");

        let err = parse_project(&entry).expect_err("missing import must error");
        let inference_err = err.downcast_ref::<InferenceError>().unwrap();
        match inference_err {
            InferenceError::ImportFileNotFound { suggestion, .. } => {
                assert_eq!(suggestion.as_deref(), Some("airth"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn no_suggestion_when_nothing_is_close() {
        // A sibling exists but is far past the distance threshold, so no
        // suggestion is offered (the message would just name the expected path).
        let project = TempProject::new("suggest-none");
        let entry = project.write("main.inf", "use arith;\npub fn main() {}");
        project.write("completely_unrelated.inf", "pub fn f() {}");

        let err = parse_project(&entry).expect_err("missing import must error");
        let inference_err = err.downcast_ref::<InferenceError>().unwrap();
        match inference_err {
            InferenceError::ImportFileNotFound { suggestion, .. } => {
                assert_eq!(
                    suggestion.as_deref(),
                    None,
                    "no sibling within distance 2 means no suggestion"
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn missing_file_from_nested_importer_names_full_expected_path() {
        // A `use` written in a deeply nested file that points at a missing file
        // must report the full src-root-relative expected path, not a path
        // relative to the importer.
        let project = TempProject::new("missing-nested");
        let entry = project.write("main.inf", "use deep::inner;\npub fn main() {}");
        project.write("deep/inner.inf", "use lib::gone;\npub fn inner() {}");
        // `lib/` exists (so the parent dir is real) but `gone.inf` does not.
        project.write("lib/present.inf", "pub fn p() {}");

        let err = parse_project(&entry).expect_err("missing nested import must error");
        let inference_err = err.downcast_ref::<InferenceError>().unwrap();
        match inference_err {
            InferenceError::ImportFileNotFound {
                referenced_as,
                expected_path,
                ..
            } => {
                assert_eq!(referenced_as, "lib::gone");
                assert!(
                    expected_path.ends_with(std::path::Path::new("lib").join("gone.inf")),
                    "expected path must be src-root-relative `lib/gone.inf`, got {expected_path:?}"
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn directory_exists_but_file_missing_errors() {
        // The directory named by the leading segments is present, but the leaf
        // `.inf` file is absent — still a missing-import error.
        let project = TempProject::new("dir-no-file");
        let entry = project.write("main.inf", "use lib::absent;\npub fn main() {}");
        // Materialize `lib/` via an unrelated sibling, so the directory exists.
        project.write("lib/other.inf", "pub fn f() {}");

        let err = parse_project(&entry).expect_err("missing leaf file must error");
        let inference_err = err.downcast_ref::<InferenceError>().unwrap();
        match inference_err {
            InferenceError::ImportFileNotFound {
                referenced_as,
                expected_path,
                ..
            } => {
                assert_eq!(referenced_as, "lib::absent");
                assert!(expected_path.ends_with("absent.inf"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn missing_file_suggestion_searches_only_the_target_directory() {
        // A near-miss sibling in a DIFFERENT directory must not be suggested:
        // the suggestion search is scoped to the directory the missing file
        // would have lived in. `lib/arithh.inf` is one edit from the missing
        // `lib/arith.inf`; a same-named `arithh.inf` at the root must be ignored.
        let project = TempProject::new("suggest-scoped");
        let entry = project.write("main.inf", "use lib::arith;\npub fn main() {}");
        project.write("lib/sibling.inf", "pub fn f() {}");
        // A close name, but at the root rather than under `lib/`.
        project.write("arithh.inf", "pub fn f() {}");

        let err = parse_project(&entry).expect_err("missing import must error");
        let inference_err = err.downcast_ref::<InferenceError>().unwrap();
        match inference_err {
            InferenceError::ImportFileNotFound { suggestion, .. } => {
                assert_ne!(
                    suggestion.as_deref(),
                    Some("arithh"),
                    "a near-miss outside the target directory must not be suggested"
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    // Axis: unreachable warnings

    #[test]
    fn multiple_unreachable_files_warn_in_sorted_order() {
        // Three orphans created in a non-sorted order; the warnings must come
        // back sorted by path so output is deterministic.
        let project = TempProject::new("unreachable-many");
        let entry = project.write("main.inf", "pub fn main() {}");
        let zeta = project.write("zeta.inf", "pub fn fz() {}");
        let alpha = project.write("alpha.inf", "pub fn fa() {}");
        let mango = project.write("mango.inf", "pub fn fm() {}");

        let parse = parse_project(&entry).expect("project parses");

        assert_eq!(parse.warnings.len(), 3, "three orphan files");
        let warned: Vec<PathBuf> = parse
            .warnings
            .iter()
            .map(|ProjectWarning::UnreachableFile { path }| path.clone())
            .collect();
        let mut expected_sorted = vec![alpha, mango, zeta];
        expected_sorted.sort();
        assert_eq!(
            warned, expected_sorted,
            "unreachable warnings must be path-sorted"
        );
    }

    #[test]
    fn unreachable_file_in_nested_directory_warns() {
        // An orphan that lives several directories deep is still found by the
        // recursive `src/**/*.inf` scan and warned about.
        let project = TempProject::new("unreachable-nested");
        let entry = project.write("main.inf", "pub fn main() {}");
        let orphan = project.write("deep/nested/orphan.inf", "pub fn f() {}");

        let parse = parse_project(&entry).expect("project parses");

        assert_eq!(parse.warnings.len(), 1);
        let ProjectWarning::UnreachableFile { path } = &parse.warnings[0];
        assert_eq!(
            std::fs::canonicalize(path).unwrap(),
            std::fs::canonicalize(&orphan).unwrap(),
        );
    }

    #[test]
    fn no_warnings_when_closure_covers_everything() {
        // Every `.inf` under the root is reachable through imports, so the
        // unreachable scan must produce zero warnings — including a nested file.
        let project = TempProject::new("no-orphans");
        let entry = project.write("main.inf", "use a;\nuse lib::b;\npub fn main() {}");
        project.write("a.inf", "pub fn fa() {}");
        project.write("lib/b.inf", "pub fn fb() {}");

        let parse = parse_project(&entry).expect("project parses");

        assert!(
            parse.warnings.is_empty(),
            "a fully-covered closure must warn about nothing, got {:?}",
            parse.warnings
        );
    }

    #[test]
    fn unreachable_scan_ignores_non_inf_files() {
        // The src tree contains README.md and a .wasm artifact next to the
        // entry; neither is a `.inf` source, so neither may be warned about.
        let project = TempProject::new("non-inf");
        let entry = project.write("main.inf", "pub fn main() {}");
        project.write("README.md", "# not source");
        project.write("notes.txt", "scratch");
        // A `.wasm` next to the source must also be ignored.
        std::fs::write(project.root.join("out.wasm"), b"\0asm").unwrap();

        let parse = parse_project(&entry).expect("project parses");

        assert!(
            parse.warnings.is_empty(),
            "only .inf files are eligible for unreachable warnings, got {:?}",
            parse.warnings
        );
    }

    #[test]
    fn reachable_file_with_orphan_sibling_warns_only_the_orphan() {
        // A mixed tree: one imported (reachable) file and one orphan in the same
        // directory. Exactly the orphan is warned about.
        let project = TempProject::new("mixed-reach");
        let entry = project.write("main.inf", "use lib::used;\npub fn main() {}");
        project.write("lib/used.inf", "pub fn fu() {}");
        let orphan = project.write("lib/unused.inf", "pub fn fo() {}");

        let parse = parse_project(&entry).expect("project parses");

        assert_eq!(parse.warnings.len(), 1, "only the orphan warns");
        let ProjectWarning::UnreachableFile { path } = &parse.warnings[0];
        assert_eq!(
            std::fs::canonicalize(path).unwrap(),
            std::fs::canonicalize(&orphan).unwrap(),
        );
    }

    // Axis: error precedence

    #[test]
    fn missing_file_aborts_before_unreachable_scan() {
        // A missing import is a hard error; it must surface as
        // `ImportFileNotFound` even though an orphan sibling also exists (the
        // unreachable scan only runs on a successful discovery).
        let project = TempProject::new("missing-wins");
        let entry = project.write("main.inf", "use gone;\npub fn main() {}");
        project.write("orphan.inf", "pub fn fo() {}");

        let err = parse_project(&entry).expect_err("missing import wins over orphan scan");
        assert!(matches!(
            err.downcast_ref::<InferenceError>(),
            Some(InferenceError::ImportFileNotFound { .. })
        ));
    }

    // Axis: entry-level error variants
    // The `Io` and `NoSourceRoot` arms guard the entry itself, ahead of the
    // import walk; the rest of the matrix only ever exercises healthy entries.

    #[test]
    fn entry_at_filesystem_root_has_no_source_root() {
        // The source root is the entry's parent directory; a filesystem root
        // (`/`) has no parent, so no root can be derived. This is read-only — it
        // touches no files and never reads `/`.
        let root = std::path::Path::new(std::path::MAIN_SEPARATOR_STR);

        let err = parse_project(root).expect_err("a parentless entry must error");
        match err.downcast_ref::<InferenceError>() {
            Some(InferenceError::NoSourceRoot(path)) => {
                assert_eq!(path, root);
            }
            other => panic!("expected NoSourceRoot, got {other:?}"),
        }
    }

    #[test]
    fn unreadable_entry_file_surfaces_io_error() {
        // The entry is read before the per-import `is_file` guard, so an entry
        // that cannot be read as a file (here: a directory in the entry slot)
        // surfaces as `Io`, not as a missing-import error. A directory exists but
        // `read_to_string` on it fails deterministically.
        let project = TempProject::new("io-entry");
        let entry_as_dir = project.root.join("not_a_file.inf");
        std::fs::create_dir_all(&entry_as_dir).expect("create the directory-in-entry-slot");

        let err = parse_project(&entry_as_dir).expect_err("an unreadable entry must error");
        match err.downcast_ref::<InferenceError>() {
            Some(InferenceError::Io { path, .. }) => {
                assert_eq!(path, &entry_as_dir);
            }
            other => panic!("expected Io, got {other:?}"),
        }
    }

    // Axis: self-import of the entry file
    // A `use main;` from a non-entry file resolves to the entry's own path. The
    // entry is already in the closure under the empty module path, so it must not
    // be discovered a second time — re-adding it would lower (and emit) every
    // entry definition twice.

    #[test]
    fn self_import_of_entry_does_not_duplicate_it() {
        let project = TempProject::new("self-import");
        let entry = project.write(
            "main.inf",
            "use lib::helper;\npub fn entry_fn() -> i32 { return 7; }\npub fn main() -> i32 { return helper::doubled(); }",
        );
        project.write(
            "lib/helper.inf",
            "use main;\npub fn doubled() -> i32 { return 14; }",
        );

        let parse = parse_project(&entry).expect("self-import deduplicates rather than failing");

        // The entry appears exactly once (empty path); the `use main;` self-import
        // did not re-add it under a `["main"]` path.
        let entry_count = parse
            .arena
            .source_files()
            .filter(|sf| sf.module_path.is_empty())
            .count();
        assert_eq!(entry_count, 1, "the entry file is discovered exactly once");
        assert_eq!(
            module_paths(&parse),
            vec![
                Vec::<String>::new(),
                vec!["lib".to_string(), "helper".to_string()],
            ],
            "no spurious [\"main\"] module is added for the self-import"
        );
    }

    #[test]
    fn import_of_non_entry_file_named_main_loads_normally() {
        // When the entry is `app.inf`, a sibling `main.inf` is an ordinary file;
        // `use main;` must load it (its path differs from the entry's), so the
        // self-import guard keys on the actual entry path, not the literal name.
        let project = TempProject::new("named-main");
        let entry = project.write(
            "app.inf",
            "use main;\npub fn run() -> i32 { return main::value(); }",
        );
        project.write("main.inf", "pub fn value() -> i32 { return 42; }");

        let parse = parse_project(&entry).expect("a real non-entry main.inf loads");

        assert_eq!(
            module_paths(&parse),
            vec![Vec::<String>::new(), vec!["main".to_string()]],
            "a non-entry file named main is discovered like any other import"
        );
    }
}
