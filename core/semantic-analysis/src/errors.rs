//! Error types for semantic analysis.
//!
//! This module defines the error enum for semantic analysis failures.
//! Currently minimal as the crate uses `SemanticResult` with diagnostics
//! rather than returning errors directly.

use thiserror::Error;

/// Errors that can occur during semantic analysis.
///
/// Currently minimal as most semantic issues are reported as diagnostics
/// with varying severity levels rather than hard errors.
#[derive(Debug, Error)]
pub enum SemanticAnalysisError {
    /// General semantic analysis error.
    #[error("semantic analysis error: {0}")]
    General(String),
}
