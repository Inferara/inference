//! [`FileAnalysis`]: the memoized result of analyzing one open file as its own
//! project entry.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use inference::{FileParseErrors, ImportProblem, LoadedFile, load_project_resilient};
use inference_analysis::errors::{LabeledDiagnostic, Severity};
use inference_analysis::rules::all_rules;
use inference_ast::arena::AstArena;
use inference_ast::ids::{DefId, SourceFileId};
use inference_base_db::LineIndex;
use inference_type_checker::typed_context::TypedContext;
use inference_type_checker::{TypeCheckDiagnostic, TypeCheckOutcome, check_with_diagnostics};
use inference_vfs::Vfs;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::hit_test::{NodeHit, hit_test};
use crate::loader::VfsLoader;
use crate::symbols::file_defs;

/// One analysis-rule finding, tagged with the producing rule's id and severity.
///
/// Both the id (`A0xx`) and the severity are per-rule, not per-finding: they are
/// read once from the [`Rule`](inference_analysis::rule::Rule) and stamped onto
/// every finding it returns. The wrapped [`LabeledDiagnostic`] carries the
/// finding's own message and the module path of the file it belongs to.
#[derive(Debug, Clone)]
pub struct AnalysisFinding {
    /// The rule's identifier, e.g. `"A035"`.
    pub rule_id: &'static str,
    /// The rule's severity.
    pub severity: Severity,
    /// The finding itself, with its file label and diagnostic.
    pub labeled: LabeledDiagnostic,
}

/// The path, source text, and line index of one file in an analysis closure.
///
/// Cross-file goto-definition resolves a target's module path to its
/// `ClosureFile`, recovering both the file's path (for the returned location's
/// URI) and a line index for a correct byte-offset → line/column conversion in
/// that file rather than in the file the request came from.
#[derive(Debug, Clone)]
pub struct ClosureFile {
    path: PathBuf,
    source: Arc<str>,
    line_index: LineIndex,
}

impl ClosureFile {
    /// The absolute path this file was read from.
    #[must_use = "the path is the reason to call this"]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The file's source text.
    #[must_use = "the source text is the reason to call this"]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The file's line index, for byte-offset ↔ line/column conversion.
    #[must_use = "the line index is the reason to call this"]
    pub fn line_index(&self) -> &LineIndex {
        &self.line_index
    }
}

/// The memoized analysis of one file treated as its own project entry.
///
/// Construction resolves the file's import closure through an overlay-then-disk
/// loader, type-checks the merged program losslessly, and runs every analysis
/// rule — all resiliently, so the result is populated as far as a broken program
/// allows. Every query below is a pure read of that cached result.
///
/// # The arena is the type context's arena
///
/// A `FileAnalysis` stores no separate arena: the merged arena lives inside its
/// [`TypedContext`] and is reached through [`TypedContext::arena`]. Type checking
/// never mutates the arena, so this is the same arena the loader produced — kept
/// exactly once rather than cloned alongside the context.
pub struct FileAnalysis {
    typed: TypedContext,
    parse_errors: Vec<FileParseErrors>,
    type_errors: Vec<TypeCheckDiagnostic>,
    import_problems: Vec<ImportProblem>,
    findings: Vec<AnalysisFinding>,
    /// Per closure file, keyed by module path (empty for the entry).
    files: FxHashMap<Vec<String>, ClosureFile>,
    /// Absolute paths of every file in the closure, for change invalidation.
    /// Always includes the entry path itself, even when the entry could not be
    /// read, so any event touching the entry can invalidate this analysis.
    closure_paths: FxHashSet<PathBuf>,
    /// Whether any import went unresolved, so a newly-opened file might fix it.
    had_missing_import: bool,
    /// Monotonic stamp identifying this computation, so tests (and callers) can
    /// observe whether a query recomputed.
    generation: u64,
}

impl FileAnalysis {
    /// Analyzes `entry` as its own project entry, reading its import closure
    /// through `vfs` (overlay first, then disk).
    ///
    /// `generation` stamps the result; the database bumps it on every compute so
    /// a recompute is observable.
    #[must_use = "the computed analysis must be stored to be of any use"]
    pub(crate) fn compute(vfs: &Vfs, entry: &Path, generation: u64) -> Self {
        let loader = VfsLoader::new(vfs);
        let parse = load_project_resilient(entry, &loader);

        // The entry is always part of its own closure, even when its read failed
        // and the resilient walk recorded no `LoadedFile` for it (an unreadable
        // entry yields an empty `files` list and no missing-import record). Without
        // this, such an analysis has an empty closure and `had_missing_import ==
        // false`, so no later event — not even a `didOpen`/`didChange` of the entry
        // itself — could ever invalidate it, permanently poisoning the entry's
        // cache with empty diagnostics. Inserting the entry unconditionally is a
        // no-op when the read succeeded (its `LoadedFile` path is already present).
        let mut closure_paths: FxHashSet<PathBuf> =
            parse.files.iter().map(|f| f.path.clone()).collect();
        closure_paths.insert(entry.to_path_buf());
        let had_missing_import = !parse.import_problems.is_empty();
        let path_by_module: FxHashMap<Vec<String>, PathBuf> = parse
            .files
            .into_iter()
            .map(|LoadedFile { module_path, path }| (module_path, path))
            .collect();

        // Type-check by moving the loader's arena into the checker (it consumes
        // the arena); everything afterwards, including per-file source, is read
        // back through `typed.arena()`, so the merged arena is stored once.
        let TypeCheckOutcome {
            typed_context,
            errors: type_errors,
        } = check_with_diagnostics(parse.arena);

        let files = build_closure_files(typed_context.arena(), &path_by_module);
        let findings = run_analysis_rules(&typed_context);

        FileAnalysis {
            typed: typed_context,
            parse_errors: parse.parse_errors,
            type_errors,
            import_problems: parse.import_problems,
            findings,
            files,
            closure_paths,
            had_missing_import,
            generation,
        }
    }

    /// The merged arena of the analyzed closure (the type context's arena).
    #[must_use = "the arena is the reason to call this"]
    pub fn arena(&self) -> &AstArena {
        self.typed.arena()
    }

    /// The type context, populated as far as error recovery allowed. Queries such
    /// as `get_node_typeinfo`, `lookup_struct`, and `call_target` answer for the
    /// parts of the program that type-checked, even when errors are present.
    #[must_use = "the type context is the reason to call this"]
    pub fn typed_context(&self) -> &TypedContext {
        &self.typed
    }

    /// Per-file syntax errors, each labeled with its file's module path.
    #[must_use = "the parse errors are the reason to call this"]
    pub fn parse_errors(&self) -> &[FileParseErrors] {
        &self.parse_errors
    }

    /// Structured type-check diagnostics, each carrying its variant, per-file
    /// source location, and optional module-path file label.
    #[must_use = "the type errors are the reason to call this"]
    pub fn type_errors(&self) -> &[TypeCheckDiagnostic] {
        &self.type_errors
    }

    /// `use` imports that did not resolve to a file, anchored at their directive.
    #[must_use = "the import problems are the reason to call this"]
    pub fn import_problems(&self) -> &[ImportProblem] {
        &self.import_problems
    }

    /// Every analysis-rule finding, each tagged with its rule id and severity.
    #[must_use = "the findings are the reason to call this"]
    pub fn findings(&self) -> &[AnalysisFinding] {
        &self.findings
    }

    /// The stamp identifying this computation. A larger value on a later query
    /// for the same entry means the analysis was recomputed after invalidation.
    #[must_use = "the generation is the reason to call this"]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The closure file for `module_path` (empty for the entry file), or `None`
    /// if that module is not in this closure.
    #[must_use = "the closure file is the reason to call this"]
    pub fn file(&self, module_path: &[String]) -> Option<&ClosureFile> {
        self.files.get(module_path)
    }

    /// The line index of the closure file named by `module_path`.
    #[must_use = "the line index is the reason to call this"]
    pub fn line_index(&self, module_path: &[String]) -> Option<&LineIndex> {
        self.files.get(module_path).map(ClosureFile::line_index)
    }

    /// The arena [`SourceFileId`] of the file named by `module_path`.
    ///
    /// Hit-testing and per-file walks key on `SourceFileId`, while cross-file
    /// features name a target by module path; this bridges the two.
    #[must_use = "the resolved file id is the reason to call this"]
    pub fn source_file_id(&self, module_path: &[String]) -> Option<SourceFileId> {
        self.arena()
            .source_file_ids()
            .find(|&id| self.arena().source_file_module_path(id) == Some(module_path))
    }

    /// The smallest node in `file` covering `offset`, with its ancestor chain.
    /// See [`hit_test`].
    #[must_use = "the covering node is the reason to call this"]
    pub fn hit_test(&self, file: SourceFileId, offset: u32) -> Option<NodeHit> {
        hit_test(self.arena(), file, offset)
    }

    /// Every definition in `file` in pre-order, including struct methods and
    /// spec-nested defs. See [`file_defs`].
    #[must_use = "the collected definitions are the reason to call this"]
    pub fn file_defs(&self, file: SourceFileId) -> Vec<DefId> {
        file_defs(self.arena(), file)
    }

    /// Whether `path` is one of the files in this analysis's closure.
    pub(crate) fn closure_contains(&self, path: &Path) -> bool {
        self.closure_paths.contains(path)
    }

    /// Whether any import in this analysis went unresolved.
    pub(crate) fn had_missing_import(&self) -> bool {
        self.had_missing_import
    }
}

/// Builds the module-path → [`ClosureFile`] map: source text comes from each
/// file's `SourceFileData` in `arena` (already read once), the path from the
/// loader's discovery list.
fn build_closure_files(
    arena: &AstArena,
    path_by_module: &FxHashMap<Vec<String>, PathBuf>,
) -> FxHashMap<Vec<String>, ClosureFile> {
    let mut files = FxHashMap::default();
    for source_file in arena.source_files() {
        let Some(path) = path_by_module.get(&source_file.module_path) else {
            // Every lowered file was read through the loader, so it has a path;
            // skip defensively rather than fabricate one.
            continue;
        };
        let source: Arc<str> = Arc::from(source_file.source.as_str());
        let line_index = LineIndex::new(&source);
        files.insert(
            source_file.module_path.clone(),
            ClosureFile {
                path: path.clone(),
                source,
                line_index,
            },
        );
    }
    files
}

/// Runs every registered analysis rule on `typed_context`, tagging each finding
/// with its rule's id and severity.
///
/// Rules run whenever a type context exists — which is always, since the checker
/// recovers from errors — because findings on a partially-typed program are
/// still valid. A rule is trusted not to panic on partial data; a panic here is
/// a compiler bug to surface, not to suppress.
fn run_analysis_rules(typed_context: &TypedContext) -> Vec<AnalysisFinding> {
    let mut findings = Vec::new();
    for rule in all_rules() {
        let rule_id = rule.id();
        let severity = rule.severity();
        for labeled in rule.check(typed_context) {
            findings.push(AnalysisFinding {
                rule_id,
                severity,
                labeled,
            });
        }
    }
    findings
}
