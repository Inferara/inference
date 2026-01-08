//! Error types for the AST crate.
//!
//! This module defines structured errors for AST parsing and module resolution.

use std::path::PathBuf;

use thiserror::Error;

/// Errors that can occur during external module parsing and resolution.
#[derive(Debug, Error)]
#[must_use = "errors must not be silently ignored"]
pub enum AstError {
    /// No module root file found in the expected locations.
    #[error("no module root found in {path}. Expected {expected}")]
    ModuleRootNotFound { path: PathBuf, expected: String },

    /// Failed to read a source file.
    #[error("failed to read {path}: {source}")]
    FileReadError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Failed to parse source code with tree-sitter.
    #[error("failed to parse {}", path.display())]
    ParseError { path: PathBuf },

    /// Failed to build AST from parsed tree.
    #[error("failed to build AST for {path}: {reason}")]
    AstBuildError { path: PathBuf, reason: String },
}
