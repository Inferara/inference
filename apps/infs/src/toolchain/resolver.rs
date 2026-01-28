//! Compiler binary resolution for the infs CLI.
//!
//! This module provides functionality for locating the `infc` compiler binary
//! across different installation contexts. The search order prioritizes:
//!
//! 1. Explicit override via `INFC_PATH` environment variable
//! 2. System PATH via `which::which("infc")`
//! 3. Managed toolchain at `~/.inference/toolchains/VERSION/bin/infc`
//!
//! ## Environment Variables
//!
//! - `INFC_PATH`: Explicit path to the infc binary (highest priority)
//!
//! ## Example
//!
//! ```rust,ignore
//! use crate::toolchain::resolver::find_infc;
//!
//! let infc_path = find_infc()?;
//! println!("Using infc at: {}", infc_path.display());
//! ```

use anyhow::{Context, Result, bail};
use std::path::PathBuf;

use crate::toolchain::paths::ToolchainPaths;
use crate::toolchain::platform::Platform;

/// Environment variable for explicit infc binary path override.
const INFC_PATH_ENV: &str = "INFC_PATH";

/// Locates the `infc` compiler binary.
///
/// Searches for the infc binary in the following priority order:
///
/// 1. **`INFC_PATH` environment variable** - Explicit override for testing
///    or custom installations
/// 2. **System PATH** - Uses `which::which("infc")` to find infc in PATH
/// 3. **Managed toolchain** - Looks in `~/.inference/toolchains/VERSION/bin/infc`
///    using the default toolchain version if set
///
/// # Errors
///
/// Returns an error if:
/// - `INFC_PATH` is set but the path does not exist
/// - No infc binary could be found in any location
///
/// The error message provides helpful guidance on how to install infc.
///
/// # Example
///
/// ```rust,ignore
/// let infc_path = find_infc()?;
/// std::process::Command::new(&infc_path)
///     .arg("--help")
///     .status()?;
/// ```
pub fn find_infc() -> Result<PathBuf> {
    // Priority 1: INFC_PATH environment variable
    if let Ok(path) = std::env::var(INFC_PATH_ENV) {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
        bail!(
            "INFC_PATH environment variable set to '{}', but file does not exist",
            path.display()
        );
    }

    // Priority 2: System PATH
    if let Ok(path) = which::which("infc") {
        return Ok(path);
    }

    // Priority 3: Managed toolchain
    if let Ok(paths) = ToolchainPaths::new()
        && let Ok(Some(version)) = paths.get_default_version()
    {
        let platform =
            Platform::detect().context("Failed to detect platform while searching for infc")?;
        let ext = platform.executable_extension();
        let infc_name = format!("infc{ext}");
        let infc_path = paths.toolchain_bin_dir(&version).join(&infc_name);

        if infc_path.exists() {
            return Ok(infc_path);
        }
    }

    bail!(
        "infc compiler not found.\n\n\
        The infc compiler is required to build Inference programs.\n\n\
        To install:\n  \
        - Run: infs install latest\n  \
        - Or download from: https://github.com/Inferara/inference/releases\n  \
        - Or set INFC_PATH environment variable to the infc binary path"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    #[serial_test::serial]
    fn infc_path_env_nonexistent_returns_error() {
        // Use a path that definitely doesn't exist
        let path = "/nonexistent/path/to/infc";

        // SAFETY: This test runs in isolation and we restore the env var at the end.
        unsafe {
            env::set_var(INFC_PATH_ENV, path);
        }

        let result = find_infc();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("INFC_PATH"));
        assert!(err.contains("does not exist"));

        // SAFETY: Cleanup - restoring previous state
        unsafe {
            env::remove_var(INFC_PATH_ENV);
        }
    }

    #[test]
    #[serial_test::serial]
    fn error_message_contains_installation_instructions() {
        // Temporarily override PATH to ensure infc is not found
        let original_path = env::var("PATH").unwrap_or_default();

        // SAFETY: This test runs in isolation and we restore the env vars at the end.
        unsafe {
            env::set_var("PATH", "");
            env::remove_var(INFC_PATH_ENV);

            // Use isolated INFERENCE_HOME to ensure no managed toolchain
            let temp_dir = env::temp_dir().join("infs_test_resolver");
            env::set_var("INFERENCE_HOME", &temp_dir);
        }

        let result = find_infc();

        // SAFETY: Cleanup - restoring previous state
        unsafe {
            env::set_var("PATH", original_path);
            env::remove_var("INFERENCE_HOME");
        }

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("infs install") || err.contains("INFC_PATH"),
            "Error should contain installation instructions: {err}"
        );
    }
}
