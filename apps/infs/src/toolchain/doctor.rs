//! Doctor checks for toolchain health verification.
//!
//! This module provides health checks for the Inference toolchain installation.
//! It is used by both the CLI `doctor` command and the TUI doctor view.
//!
//! ## Checks Performed
//!
//! - `infs` binary availability in PATH
//! - Platform detection
//! - Toolchain directory existence
//! - Default toolchain configuration
//! - `infc` compiler binary presence

use super::{Platform, ToolchainPaths};

/// Generates a message for when no default toolchain is set.
///
/// Checks installed versions and suggests the appropriate action:
/// - If no versions installed: suggests running `infs install`
/// - If versions exist: suggests running `infs default <latest>` to set one
fn no_default_toolchain_message(paths: &ToolchainPaths) -> String {
    let installed = paths.list_installed_versions().unwrap_or_default();
    if installed.is_empty() {
        "No default toolchain set. Run 'infs install' first.".to_string()
    } else {
        // Safety: `installed` is non-empty due to the guard above
        let latest = installed
            .last()
            .expect("installed list is non-empty due to guard above");
        format!("No default toolchain set. Run 'infs default {latest}' to set one.")
    }
}

/// Status of a doctor check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorCheckStatus {
    /// Check passed.
    Ok,
    /// Check passed with warnings.
    Warning,
    /// Check failed.
    Error,
}

/// Result of a single doctor check.
#[derive(Debug, Clone)]
pub struct DoctorCheck {
    /// Name of the check.
    pub name: String,
    /// Status of the check.
    pub status: DoctorCheckStatus,
    /// Descriptive message.
    pub message: String,
}

impl DoctorCheck {
    /// Creates a new check with Ok status.
    #[must_use]
    pub fn ok(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: DoctorCheckStatus::Ok,
            message: message.into(),
        }
    }

    /// Creates a new check with Warning status.
    #[must_use]
    pub fn warning(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: DoctorCheckStatus::Warning,
            message: message.into(),
        }
    }

    /// Creates a new check with Error status.
    #[must_use]
    pub fn error(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: DoctorCheckStatus::Error,
            message: message.into(),
        }
    }

    /// Returns the CLI prefix for this check status.
    #[must_use]
    pub fn prefix(&self) -> &'static str {
        match self.status {
            DoctorCheckStatus::Ok => "[OK]",
            DoctorCheckStatus::Warning => "[WARN]",
            DoctorCheckStatus::Error => "[FAIL]",
        }
    }
}

/// Runs all doctor checks and returns the results.
///
/// This function aggregates all health checks into a single vector.
pub fn run_all_checks() -> Vec<DoctorCheck> {
    vec![
        check_infs_binary(),
        check_platform(),
        check_toolchain_directory(),
        check_default_toolchain(),
        check_infc(),
    ]
}

/// Checks if the infs binary is accessible in PATH.
#[must_use]
pub fn check_infs_binary() -> DoctorCheck {
    match std::env::current_exe() {
        Ok(path) => {
            if which::which("infs").is_ok() {
                DoctorCheck::ok("infs binary", format!("Found at {}", path.display()))
            } else {
                DoctorCheck::warning(
                    "infs binary",
                    format!(
                        "Found at {} but not in PATH. Add {} to your PATH.",
                        path.display(),
                        path.parent()
                            .map_or_else(String::new, |p| p.display().to_string())
                    ),
                )
            }
        }
        Err(e) => DoctorCheck::error("infs binary", format!("Cannot determine path: {e}")),
    }
}

/// Checks platform detection.
#[must_use]
pub fn check_platform() -> DoctorCheck {
    match Platform::detect() {
        Ok(platform) => DoctorCheck::ok("Platform", format!("Detected {platform}")),
        Err(e) => DoctorCheck::error("Platform", format!("Detection failed: {e}")),
    }
}

/// Checks if the toolchain directory exists.
#[must_use]
pub fn check_toolchain_directory() -> DoctorCheck {
    match ToolchainPaths::new() {
        Ok(paths) => {
            if paths.root.exists() {
                DoctorCheck::ok(
                    "Toolchain directory",
                    format!("Found at {}", paths.root.display()),
                )
            } else {
                DoctorCheck::warning(
                    "Toolchain directory",
                    format!(
                        "Not found at {}. Run 'infs install' to create it.",
                        paths.root.display()
                    ),
                )
            }
        }
        Err(e) => DoctorCheck::error("Toolchain directory", format!("Cannot determine path: {e}")),
    }
}

/// Checks if a default toolchain is set.
#[must_use]
pub fn check_default_toolchain() -> DoctorCheck {
    let paths = match ToolchainPaths::new() {
        Ok(p) => p,
        Err(e) => {
            return DoctorCheck::error("Default toolchain", format!("Cannot check: {e}"));
        }
    };

    match paths.get_default_version() {
        Ok(Some(version)) => {
            if paths.is_version_installed(&version) {
                DoctorCheck::ok("Default toolchain", format!("Set to {version}"))
            } else {
                DoctorCheck::error(
                    "Default toolchain",
                    format!("{version} is set as default but not installed"),
                )
            }
        }
        Ok(None) => DoctorCheck::warning("Default toolchain", no_default_toolchain_message(&paths)),
        Err(e) => DoctorCheck::error("Default toolchain", format!("Cannot read: {e}")),
    }
}

/// Checks if the infc compiler binary is available.
#[must_use]
pub fn check_infc() -> DoctorCheck {
    let Ok(platform) = Platform::detect() else {
        return DoctorCheck::error("infc", "Cannot detect platform");
    };

    let binary_with_ext = format!("infc{}", platform.executable_extension());

    if which::which(&binary_with_ext).is_ok() {
        return DoctorCheck::ok("infc", format!("Found {binary_with_ext} in PATH"));
    }

    let Ok(paths) = ToolchainPaths::new() else {
        return DoctorCheck::error("infc", "Cannot determine toolchain paths");
    };

    let default_version = match paths.get_default_version() {
        Ok(Some(v)) => v,
        Ok(None) => {
            return DoctorCheck::warning("infc", no_default_toolchain_message(&paths));
        }
        Err(_) => {
            return DoctorCheck::error("infc", "Cannot read default version");
        }
    };

    let binary_path = paths.binary_path(&default_version, &binary_with_ext);
    if binary_path.exists() {
        DoctorCheck::ok("infc", format!("Found at {}", binary_path.display()))
    } else {
        DoctorCheck::error(
            "infc",
            format!(
                "Not found. Expected at {}. Run 'infs install' to install the toolchain.",
                binary_path.display()
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_check_constructors_set_correct_status() {
        let ok = DoctorCheck::ok("test", "message");
        assert_eq!(ok.status, DoctorCheckStatus::Ok);

        let warn = DoctorCheck::warning("test", "message");
        assert_eq!(warn.status, DoctorCheckStatus::Warning);

        let err = DoctorCheck::error("test", "message");
        assert_eq!(err.status, DoctorCheckStatus::Error);
    }

    #[test]
    fn doctor_check_prefix_returns_correct_strings() {
        let ok = DoctorCheck::ok("test", "message");
        assert_eq!(ok.prefix(), "[OK]");

        let warn = DoctorCheck::warning("test", "message");
        assert_eq!(warn.prefix(), "[WARN]");

        let err = DoctorCheck::error("test", "message");
        assert_eq!(err.prefix(), "[FAIL]");
    }

    #[test]
    fn run_all_checks_returns_expected_count() {
        let checks = run_all_checks();
        // Checks: infs, platform, toolchain dir, default toolchain, infc
        assert_eq!(checks.len(), 5);
    }

    #[test]
    fn check_platform_returns_result() {
        let check = check_platform();
        assert!(!check.name.is_empty());
        assert!(!check.message.is_empty());
    }
}
