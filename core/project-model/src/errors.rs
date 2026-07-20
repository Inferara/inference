//! Error types for the project front end.
//!
//! Per the project's error-handling convention, every fallible operation in the
//! fail-fast project walk surfaces a variant of [`InferenceError`] wrapped in
//! `anyhow::Result`, so downstream consumers can downcast to a structured error
//! instead of parsing free-form strings. The name is kept for source
//! compatibility: the orchestration crate re-exports it as `inference::InferenceError`.

use std::path::PathBuf;

use thiserror::Error;

/// Errors raised while parsing a multi-file project (the [`crate::parse_project`]
/// front end).
#[derive(Debug, Error)]
pub enum InferenceError {
    /// A `use` directive referenced a file that does not exist on disk.
    ///
    /// `referenced_as` is the `::`-joined import path as written in source;
    /// `expected_path` is the absolute path the resolver looked for. When a
    /// sibling `.inf` file is a near match, `suggestion` carries its stem so the
    /// message can offer a one-keystroke fix.
    #[error("{}", format_missing_file(.referenced_as, .expected_path, .suggestion.as_deref()))]
    ImportFileNotFound {
        referenced_as: String,
        expected_path: PathBuf,
        suggestion: Option<String>,
    },

    /// A `use` path segment is not a valid file/directory name (empty, `.`,
    /// `..`, or carrying a path separator), so it cannot be mapped to a file.
    #[error(
        "invalid import path segment `{segment}` in `use {referenced_as};` — \
         path segments must be plain file or directory names"
    )]
    InvalidImportSegment {
        referenced_as: String,
        segment: String,
    },

    /// An imported (non-entry) project file failed to parse. `module_path` is its
    /// `::`-joined namespace name and `details` aggregates the syntax errors. The
    /// entry file is reported as [`Self::EntryFileParse`] instead, because it is
    /// the file the user named — calling it an "imported file" would be wrong.
    #[error("failed to parse imported file `{module_path}`:\n{details}")]
    ImportedFileParse {
        module_path: String,
        details: String,
    },

    /// The entry file (the one the user named on the command line) failed to
    /// parse. Reported with its real path rather than the imported-file wording so
    /// a single-file build points the user straight at the file they compiled.
    #[error("failed to parse `{}`:\n{details}", path.display())]
    EntryFileParse {
        path: PathBuf,
        details: String,
    },

    /// Reading a project file (the entry or an imported file) failed.
    #[error("failed to read `{}`: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The entry path has no parent directory, so no source root can be derived.
    #[error("entry file `{}` has no parent directory to use as the source root", .0.display())]
    NoSourceRoot(PathBuf),
}

/// Renders the missing-import-file message, appending a nearest-match hint when
/// one is available.
fn format_missing_file(
    referenced_as: &str,
    expected_path: &std::path::Path,
    suggestion: Option<&str>,
) -> String {
    use std::fmt::Write as _;

    let mut message = format!(
        "imported file not found for `use {referenced_as};` (expected `{}`)",
        expected_path.display()
    );
    if let Some(name) = suggestion {
        // Writing to a String is infallible.
        let _ = write!(message, "; did you mean `{name}`?");
    }
    message
}
