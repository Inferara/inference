//! Merged, editor-ready diagnostics for one open document.

use inference_ast::ids::SourceFileId;
use inference_ast::nodes::{Directive, Location};
use inference_ide_db::{
    FileAnalysis, FileParseErrors, ImportProblem, Severity as DbSeverity, TextRange,
    TypeCheckDiagnostic,
};
use inference_type_checker::errors::TypeCheckError;
use rustc_hash::FxHashSet;

use crate::syntax::text_range;

/// The severity of a [`Diagnostic`], in editor terminology and LSP ordering.
///
/// A local mirror of the analysis crate's severity so the feature API leaks no
/// compiler type; the variant order matches LSP `DiagnosticSeverity`
/// (Error before Warning before Info).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl From<DbSeverity> for Severity {
    fn from(severity: DbSeverity) -> Self {
        match severity {
            DbSeverity::Error => Severity::Error,
            DbSeverity::Warning => Severity::Warning,
            DbSeverity::Info => Severity::Info,
        }
    }
}

/// One diagnostic anchored in the open document, ready to hand to the editor.
///
/// `range` is a byte range in the open file's current text; the LSP layer
/// converts it to line/character with the file's line index. `code` groups
/// diagnostics by source: `"syntax"`, `"import"`, `"type"`, or an analysis rule
/// id (`"A001"`..`"A041"`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: TextRange,
    pub severity: Severity,
    pub code: Option<String>,
    pub message: String,
}

/// Collects every diagnostic that belongs to the entry file of `file`.
///
/// Only the entry file's diagnostics are returned: an imported file's offsets are
/// local to that file and would be misplaced here. An imported file that failed
/// to parse still surfaces — as a single entry-file diagnostic on the `use`
/// directive that pulled it in — so the user sees why analysis is degraded.
#[must_use]
pub(crate) fn diagnostics(file: &FileAnalysis) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    push_entry_syntax_errors(file, &mut out);
    push_import_problems(file, &mut out);
    push_broken_import_summaries(file, &mut out);
    push_entry_type_errors(file, &mut out);
    push_entry_findings(file, &mut out);
    out.sort_by_key(|d| (d.range.start, d.range.end));
    dedup_exact(out)
}

/// Drops exact-duplicate diagnostics (same range, severity, code, and message),
/// keeping the first. A single logical problem must reach the editor once even
/// when an upstream phase pushes it twice (e.g. a checker error emitted on two
/// non-mutually-exclusive paths for one node), so this is a final belt-and-braces
/// pass independent of any upstream de-duplication.
fn dedup_exact(diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut seen: FxHashSet<(TextRange, Severity, Option<String>, String)> = FxHashSet::default();
    diagnostics
        .into_iter()
        .filter(|d| seen.insert((d.range, d.severity, d.code.clone(), d.message.clone())))
        .collect()
}

fn push_entry_syntax_errors(file: &FileAnalysis, out: &mut Vec<Diagnostic>) {
    let Some(entry) = file
        .parse_errors()
        .iter()
        .find(|f| f.module_path.is_empty())
    else {
        return;
    };
    for error in &entry.errors {
        out.push(Diagnostic {
            range: text_range(error.span),
            severity: Severity::Error,
            code: Some("syntax".to_string()),
            message: error.message.clone(),
        });
    }
}

fn push_import_problems(file: &FileAnalysis, out: &mut Vec<Diagnostic>) {
    for problem in file.import_problems() {
        if !problem.importing_module_path.is_empty() {
            continue; // Anchored in an imported file, not the open document.
        }
        out.push(Diagnostic {
            range: text_range(problem.location),
            severity: Severity::Error,
            code: Some("import".to_string()),
            message: import_message(problem),
        });
    }
}

fn import_message(problem: &ImportProblem) -> String {
    let base = format!("cannot find imported module `{}`", problem.referenced_as);
    match &problem.suggestion {
        Some(name) => format!("{base}; did you mean `{name}`?"),
        None => base,
    }
}

/// Surfaces each imported file that failed to parse as one diagnostic on the
/// `use` directive that imports it, so a broken import degrades analysis visibly
/// instead of silently.
fn push_broken_import_summaries(file: &FileAnalysis, out: &mut Vec<Diagnostic>) {
    let Some(entry) = file.source_file_id(&[]) else {
        return;
    };
    for broken in file.parse_errors() {
        if broken.module_path.is_empty() || broken.errors.is_empty() {
            continue;
        }
        let Some(location) = use_directive_location(file, entry, &broken.module_path) else {
            // A transitively-imported broken file has no `use` directive in the
            // open document to anchor on; its own file's diagnostics carry the
            // detail. Skipping keeps the range honest rather than misplacing it.
            continue;
        };
        out.push(Diagnostic {
            range: text_range(location),
            severity: Severity::Error,
            code: Some("import".to_string()),
            message: broken_import_message(broken),
        });
    }
}

fn broken_import_message(broken: &FileParseErrors) -> String {
    let module = broken.module_path.join("::");
    let count = broken.errors.len();
    let plural = if count == 1 { "error" } else { "errors" };
    format!("imported module `{module}` could not be analyzed: {count} syntax {plural}")
}

/// The location of the entry-file `use` directive whose path names `module_path`,
/// or `None` when the module is not directly imported by the open document.
fn use_directive_location(
    file: &FileAnalysis,
    entry: SourceFileId,
    module_path: &[String],
) -> Option<Location> {
    let arena = file.arena();
    let target = module_path.join("::");
    arena[entry].directives.iter().find_map(|directive| {
        let Directive::Use(use_directive) = directive;
        let path = use_directive
            .segments
            .iter()
            .map(|&segment| arena.ident_name(segment))
            .collect::<Vec<_>>()
            .join("::");
        (path == target).then_some(use_directive.location)
    })
}

fn push_entry_type_errors(file: &FileAnalysis, out: &mut Vec<Diagnostic>) {
    let import_locations = entry_import_problem_locations(file);
    for diagnostic in file.type_errors() {
        if diagnostic.file_label.is_some() {
            continue; // Belongs to an imported file.
        }
        if is_redundant_import_error(&diagnostic.error, &import_locations) {
            continue; // The authoritative `import` diagnostic already covers it.
        }
        out.push(Diagnostic {
            range: text_range(*diagnostic.error.location()),
            severity: Severity::Error,
            code: Some("type".to_string()),
            message: type_message(diagnostic),
        });
    }
}

/// The `use`-directive locations of every unresolved import anchored in the entry
/// file. A directive that failed to resolve already carries an authoritative
/// `import` diagnostic; the type checker independently complains about the same
/// directive, and those complaints are suppressed against this set.
fn entry_import_problem_locations(file: &FileAnalysis) -> Vec<Location> {
    file.import_problems()
        .iter()
        .filter(|problem| problem.importing_module_path.is_empty())
        .map(|problem| problem.location)
        .collect()
}

/// Whether a type error merely restates that an import did not resolve at a
/// directive already reported by an `import` diagnostic. Only the two
/// import-resolution variants qualify, and only when their location matches a
/// recorded [`ImportProblem`] — so a genuine, otherwise-unreported type error is
/// never hidden. In an IDE a project context always exists, so the checker's
/// `FileImportWithoutProjectContext` message ("build the project") is both
/// redundant and wrong here.
fn is_redundant_import_error(error: &TypeCheckError, import_locations: &[Location]) -> bool {
    matches!(
        error,
        TypeCheckError::FileImportWithoutProjectContext { .. }
            | TypeCheckError::ImportResolutionFailed { .. }
    ) && import_locations.contains(error.location())
}

/// The type error's message without its leading `line:col:` prefix, which the
/// separate `range` already conveys.
fn type_message(diagnostic: &TypeCheckDiagnostic) -> String {
    let full = diagnostic.error.to_string();
    let prefix = format!("{}: ", diagnostic.error.location());
    full.strip_prefix(&prefix).unwrap_or(&full).to_string()
}

fn push_entry_findings(file: &FileAnalysis, out: &mut Vec<Diagnostic>) {
    for finding in file.findings() {
        if !finding.labeled.module_path.is_empty() {
            continue; // Belongs to an imported file.
        }
        out.push(Diagnostic {
            range: text_range(*finding.labeled.diagnostic.location()),
            severity: finding.severity.into(),
            code: Some(finding.rule_id.to_string()),
            message: finding.labeled.diagnostic.to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, dedup_exact};
    use crate::TextRange;
    use crate::diagnostics::Severity;
    use crate::test_utils::{after, at, nth, single, with_lib};

    #[test]
    fn clean_file_has_no_diagnostics() {
        let src = "fn add(a: i32, b: i32) -> i32 { return a + b; }";
        let (host, path) = single(src);
        assert!(host.analysis().diagnostics(&path).is_empty());
    }

    #[test]
    fn undeclared_variable_is_a_type_diagnostic_at_the_use() {
        let src = "fn f() -> i32 { return x; }";
        let (host, path) = single(src);
        let diagnostics = host.analysis().diagnostics(&path);
        let diagnostic = diagnostics
            .iter()
            .find(|d| d.message.contains("undeclared variable `x`"))
            .expect("an undeclared-variable diagnostic");
        assert_eq!(diagnostic.code.as_deref(), Some("type"));
        assert_eq!(diagnostic.severity, Severity::Error);
        assert_eq!(
            diagnostic.range,
            TextRange {
                start: at(src, "x"),
                end: at(src, "x") + 1,
            }
        );
        // The redundant `line:col:` prefix is stripped from the message.
        assert!(!diagnostic.message.starts_with(char::is_numeric));
    }

    #[test]
    fn duplicate_local_is_an_a041_finding_on_the_second_declaration() {
        let src = "fn f() { let a: i32 = 1; let a: i32 = 2; }";
        let (host, path) = single(src);
        let diagnostics = host.analysis().diagnostics(&path);
        let finding = diagnostics
            .iter()
            .find(|d| d.code.as_deref() == Some("A041"))
            .expect("an A041 finding");
        assert_eq!(finding.severity, Severity::Error);
        assert_eq!(finding.range.start, nth(src, "let a", 1));
        assert!(finding.message.contains("already declared"));
    }

    #[test]
    fn syntax_error_is_a_syntax_diagnostic() {
        let src = "fn f() { let x: i32 = ; }";
        let (host, path) = single(src);
        let diagnostics = host.analysis().diagnostics(&path);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code.as_deref() == Some("syntax") && d.severity == Severity::Error),
            "expected a syntax diagnostic, got {diagnostics:?}"
        );
    }

    #[test]
    fn diagnostics_are_sorted_by_range_start() {
        let src = "fn f() -> i32 { let a: i32 = 1; let a: i32 = 2; return z; }";
        let (host, path) = single(src);
        let diagnostics = host.analysis().diagnostics(&path);
        let starts: Vec<u32> = diagnostics.iter().map(|d| d.range.start).collect();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted, "diagnostics come out ordered by position");
    }

    #[test]
    fn broken_import_surfaces_on_the_use_directive() {
        let entry = "use lib;\nfn main() -> i32 { return 0; }";
        let lib = "fn g() { let x: i32 = ; }";
        let (host, path) = with_lib(entry, lib);
        let diagnostics = host.analysis().diagnostics(&path);
        let diagnostic = diagnostics
            .iter()
            .find(|d| d.code.as_deref() == Some("import"))
            .expect("an import diagnostic");
        assert!(diagnostic.message.contains("lib"), "{}", diagnostic.message);
        assert!(
            diagnostic.range.start >= at(entry, "use lib;")
                && diagnostic.range.end <= after(entry, "use lib;"),
            "range {:?} falls within the use directive",
            diagnostic.range
        );
    }

    #[test]
    fn missing_import_reports_cannot_find_module() {
        // `lib` is never opened and not on disk, so the import does not resolve.
        let src = "use libx;\nfn main() -> i32 { return 0; }";
        let (host, path) = single(src);
        let diagnostics = host.analysis().diagnostics(&path);
        let diagnostic = diagnostics
            .iter()
            .find(|d| d.code.as_deref() == Some("import"))
            .expect("an import diagnostic");
        assert!(
            diagnostic
                .message
                .contains("cannot find imported module `libx`"),
            "{}",
            diagnostic.message
        );
        assert!(
            diagnostic.range.start >= at(src, "use libx;")
                && diagnostic.range.end <= after(src, "use libx;"),
            "anchored on the use directive"
        );
    }

    #[test]
    fn a_missing_import_does_not_also_report_a_project_context_type_error() {
        // `libx` is absent, so the resilient walk records an ImportProblem and the
        // type checker independently emits `FileImportWithoutProjectContext` for
        // the same directive. The `import` diagnostic is authoritative; the
        // contradictory "build the project" type error is suppressed (an IDE
        // always has a project context).
        let src = "use libx;\nfn main() -> i32 { return 0; }";
        let (host, path) = single(src);
        let diagnostics = host.analysis().diagnostics(&path);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code.as_deref() == Some("import")),
            "the missing-import diagnostic remains: {diagnostics:?}"
        );
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.message.contains("file imports require a project context")),
            "the contradictory project-context type error is suppressed: {diagnostics:?}"
        );
    }

    #[test]
    fn exact_duplicate_diagnostics_are_dropped() {
        // A single problem must reach the editor once even when an upstream phase
        // pushes it twice for one node; distinct diagnostics are preserved.
        let base = Diagnostic {
            range: TextRange { start: 5, end: 10 },
            severity: Severity::Error,
            code: Some("type".to_string()),
            message: "type mismatch".to_string(),
        };
        assert_eq!(
            dedup_exact(vec![base.clone(), base.clone()]).len(),
            1,
            "exact duplicates collapse to one"
        );
        let other = Diagnostic {
            message: "a different message".to_string(),
            ..base.clone()
        };
        assert_eq!(
            dedup_exact(vec![base, other]).len(),
            2,
            "diagnostics differing in any field are kept"
        );
    }

    #[test]
    fn imported_file_diagnostics_do_not_leak_into_the_entry() {
        // The entry is clean; only the single import summary should show, never the
        // imported file's own per-file-local diagnostics (they'd be misplaced).
        let entry = "use lib;\nfn main() -> i32 { return 0; }";
        let lib = "fn g() { let x: i32 = ; }";
        let (host, path) = with_lib(entry, lib);
        let diagnostics = host.analysis().diagnostics(&path);
        assert!(
            diagnostics
                .iter()
                .all(|d| d.code.as_deref() == Some("import")),
            "only import summaries belong to the entry, got {diagnostics:?}"
        );
    }
}
