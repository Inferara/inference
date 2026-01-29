//! Error types for the infs CLI.
//!
//! This module defines the `InfsError` enum which consolidates all error variants
//! that can occur during CLI operations. While the current implementation primarily
//! uses `anyhow::Result` for error handling, these typed errors enable more precise
//! error handling and better error messages in specific scenarios.

use std::path::PathBuf;
use thiserror::Error;

/// Consolidated error type for infs CLI operations.
///
/// This enum captures all error variants that can occur during compilation
/// and CLI operations. Each variant includes context-specific information
/// to provide helpful error messages to users.
#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum InfsError {
    /// Source file not found at the specified path.
    #[error("file not found: {path}")]
    FileNotFound {
        /// The path that was not found.
        path: PathBuf,
    },

    /// Error reading or writing files.
    #[error("I/O error: {message}")]
    IoError {
        /// Description of the I/O operation that failed.
        message: String,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Syntax error during parsing phase.
    #[error("parse error: {message}")]
    ParseError {
        /// Description of the parse error.
        message: String,
    },

    /// Type checking failed.
    #[error("type check error: {message}")]
    TypeCheckError {
        /// Description of the type error.
        message: String,
    },

    /// Semantic analysis failed.
    #[error("analysis error: {message}")]
    AnalysisError {
        /// Description of the analysis error.
        message: String,
    },

    /// Code generation failed.
    #[error("codegen error: {message}")]
    CodegenError {
        /// Description of the codegen error.
        message: String,
    },

    /// Invalid command line arguments.
    #[error("invalid arguments: {message}")]
    InvalidArguments {
        /// Description of what was invalid.
        message: String,
    },

    /// Network error during download.
    #[error("download error: {message}")]
    DownloadError {
        /// Description of the download error.
        message: String,
        /// The underlying error.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Checksum verification failed.
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch {
        /// The expected checksum.
        expected: String,
        /// The actual checksum.
        actual: String,
    },

    /// Manifest parsing or fetching failed.
    #[error("manifest error: {message}")]
    ManifestError {
        /// Description of the manifest error.
        message: String,
    },

    /// Toolchain not found.
    #[error("toolchain not found: {version}")]
    ToolchainNotFound {
        /// The version that was not found.
        version: String,
    },

    /// Installation failed.
    #[error("installation failed: {message}")]
    InstallError {
        /// Description of the installation error.
        message: String,
    },

    /// Subprocess exited with non-zero code.
    ///
    /// This variant is used when a subprocess (like wasmtime or coqc) exits
    /// with a non-zero exit code. The exit code should be propagated to the
    /// parent process without printing additional error messages.
    #[error("process exited with code {code}")]
    ProcessExitCode {
        /// The exit code from the subprocess.
        code: i32,
    },
}

#[allow(dead_code)]
impl InfsError {
    /// Creates a new `FileNotFound` error.
    #[must_use]
    pub fn file_not_found(path: PathBuf) -> Self {
        Self::FileNotFound { path }
    }

    /// Creates a new `IoError` from an I/O error with context.
    #[must_use]
    pub fn io_error(message: impl Into<String>, source: std::io::Error) -> Self {
        Self::IoError {
            message: message.into(),
            source,
        }
    }

    /// Creates a new `ParseError`.
    #[must_use]
    pub fn parse_error(message: impl Into<String>) -> Self {
        Self::ParseError {
            message: message.into(),
        }
    }

    /// Creates a new `TypeCheckError`.
    #[must_use]
    pub fn type_check_error(message: impl Into<String>) -> Self {
        Self::TypeCheckError {
            message: message.into(),
        }
    }

    /// Creates a new `AnalysisError`.
    #[must_use]
    pub fn analysis_error(message: impl Into<String>) -> Self {
        Self::AnalysisError {
            message: message.into(),
        }
    }

    /// Creates a new `CodegenError`.
    #[must_use]
    pub fn codegen_error(message: impl Into<String>) -> Self {
        Self::CodegenError {
            message: message.into(),
        }
    }

    /// Creates a new `InvalidArguments` error.
    #[must_use]
    pub fn invalid_arguments(message: impl Into<String>) -> Self {
        Self::InvalidArguments {
            message: message.into(),
        }
    }

    /// Creates a new `DownloadError`.
    #[must_use]
    pub fn download_error(message: impl Into<String>) -> Self {
        Self::DownloadError {
            message: message.into(),
            source: None,
        }
    }

    /// Creates a new `DownloadError` with a source error.
    #[must_use]
    pub fn download_error_with_source(
        message: impl Into<String>,
        source: Box<dyn std::error::Error + Send + Sync>,
    ) -> Self {
        Self::DownloadError {
            message: message.into(),
            source: Some(source),
        }
    }

    /// Creates a new `ChecksumMismatch` error.
    #[must_use]
    pub fn checksum_mismatch(expected: impl Into<String>, actual: impl Into<String>) -> Self {
        Self::ChecksumMismatch {
            expected: expected.into(),
            actual: actual.into(),
        }
    }

    /// Creates a new `ManifestError`.
    #[must_use]
    pub fn manifest_error(message: impl Into<String>) -> Self {
        Self::ManifestError {
            message: message.into(),
        }
    }

    /// Creates a new `ToolchainNotFound` error.
    #[must_use]
    pub fn toolchain_not_found(version: impl Into<String>) -> Self {
        Self::ToolchainNotFound {
            version: version.into(),
        }
    }

    /// Creates a new `InstallError`.
    #[must_use]
    pub fn install_error(message: impl Into<String>) -> Self {
        Self::InstallError {
            message: message.into(),
        }
    }

    /// Creates a new `ProcessExitCode` error.
    #[must_use]
    pub const fn process_exit_code(code: i32) -> Self {
        Self::ProcessExitCode { code }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_not_found_displays_path() {
        let err = InfsError::file_not_found(PathBuf::from("/some/path.inf"));
        assert_eq!(err.to_string(), "file not found: /some/path.inf");
    }

    #[test]
    fn parse_error_displays_message() {
        let err = InfsError::parse_error("unexpected token");
        assert_eq!(err.to_string(), "parse error: unexpected token");
    }

    #[test]
    fn invalid_arguments_displays_message() {
        let err = InfsError::invalid_arguments("missing required flag");
        assert_eq!(err.to_string(), "invalid arguments: missing required flag");
    }

    #[test]
    fn download_error_displays_message() {
        let err = InfsError::download_error("connection timeout");
        assert_eq!(err.to_string(), "download error: connection timeout");
    }

    #[test]
    fn checksum_mismatch_displays_both_values() {
        let err = InfsError::checksum_mismatch("abc123", "def456");
        assert_eq!(
            err.to_string(),
            "checksum mismatch: expected abc123, got def456"
        );
    }

    #[test]
    fn manifest_error_displays_message() {
        let err = InfsError::manifest_error("invalid JSON");
        assert_eq!(err.to_string(), "manifest error: invalid JSON");
    }

    #[test]
    fn toolchain_not_found_displays_version() {
        let err = InfsError::toolchain_not_found("0.1.0");
        assert_eq!(err.to_string(), "toolchain not found: 0.1.0");
    }

    #[test]
    fn install_error_displays_message() {
        let err = InfsError::install_error("extraction failed");
        assert_eq!(err.to_string(), "installation failed: extraction failed");
    }

    #[test]
    fn process_exit_code_displays_code() {
        let err = InfsError::process_exit_code(42);
        assert_eq!(err.to_string(), "process exited with code 42");
    }
}
