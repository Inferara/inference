//! Error types for the infs CLI.

use thiserror::Error;

/// Error type for infs CLI operations.
#[derive(Debug, Error)]
pub enum InfsError {
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

impl InfsError {
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
    fn process_exit_code_displays_code() {
        let err = InfsError::process_exit_code(42);
        assert_eq!(err.to_string(), "process exited with code 42");
    }
}
