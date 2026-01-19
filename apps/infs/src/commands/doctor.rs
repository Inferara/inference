#![warn(clippy::pedantic)]

//! Doctor command for the infs CLI.
//!
//! Verifies the installation health of the Inference toolchain and
//! reports any issues with suggested remediation steps.
//!
//! ## Usage
//!
//! ```bash
//! infs doctor
//! ```
//!
//! ## Checks Performed
//!
//! - Platform detection
//! - Toolchain directory existence
//! - Default toolchain configuration
//! - inf-llc binary presence
//! - rust-lld binary presence
//! - libLLVM shared library (Linux only)

use anyhow::Result;

use crate::toolchain::{Platform, ToolchainPaths};

/// Result of a single health check.
struct CheckResult {
    name: &'static str,
    status: CheckStatus,
    message: String,
}

/// Status of a health check.
enum CheckStatus {
    Ok,
    Warning,
    Error,
}

impl CheckResult {
    fn ok(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Ok,
            message: message.into(),
        }
    }

    fn warning(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Warning,
            message: message.into(),
        }
    }

    fn error(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Error,
            message: message.into(),
        }
    }

    fn prefix(&self) -> &'static str {
        match self.status {
            CheckStatus::Ok => "[OK]",
            CheckStatus::Warning => "[WARN]",
            CheckStatus::Error => "[FAIL]",
        }
    }
}

/// Executes the doctor command.
///
/// Runs all health checks and displays the results.
///
/// # Errors
///
/// Returns an error if critical checks fail to execute (not if they report failures).
#[allow(clippy::unnecessary_wraps, clippy::unused_async)]
pub async fn execute() -> Result<()> {
    println!("Checking Inference toolchain installation...");
    println!();

    let mut checks = vec![
        check_platform(),
        check_toolchain_directory(),
        check_default_toolchain(),
        check_inf_llc(),
        check_rust_lld(),
    ];

    #[cfg(target_os = "linux")]
    checks.push(check_libllvm());

    let mut has_errors = false;
    let mut has_warnings = false;

    for check in &checks {
        let prefix = check.prefix();
        println!("  {prefix} {}: {}", check.name, check.message);
        match check.status {
            CheckStatus::Ok => {}
            CheckStatus::Warning => has_warnings = true,
            CheckStatus::Error => has_errors = true,
        }
    }

    println!();

    if has_errors {
        println!("Some checks failed. Run 'infs install' to install the toolchain.");
    } else if has_warnings {
        println!("Some warnings were found. The toolchain may work but could have issues.");
    } else {
        println!("All checks passed. The toolchain is ready to use.");
    }

    Ok(())
}

/// Checks platform detection.
fn check_platform() -> CheckResult {
    match Platform::detect() {
        Ok(platform) => CheckResult::ok("Platform", format!("Detected {platform}")),
        Err(e) => CheckResult::error("Platform", format!("Detection failed: {e}")),
    }
}

/// Checks if the toolchain directory exists.
fn check_toolchain_directory() -> CheckResult {
    match ToolchainPaths::new() {
        Ok(paths) => {
            if paths.root.exists() {
                CheckResult::ok(
                    "Toolchain directory",
                    format!("Found at {}", paths.root.display()),
                )
            } else {
                CheckResult::warning(
                    "Toolchain directory",
                    format!(
                        "Not found at {}. Run 'infs install' to create it.",
                        paths.root.display()
                    ),
                )
            }
        }
        Err(e) => CheckResult::error("Toolchain directory", format!("Cannot determine path: {e}")),
    }
}

/// Checks if a default toolchain is set.
fn check_default_toolchain() -> CheckResult {
    let paths = match ToolchainPaths::new() {
        Ok(p) => p,
        Err(e) => return CheckResult::error("Default toolchain", format!("Cannot check: {e}")),
    };

    match paths.get_default_version() {
        Ok(Some(version)) => {
            if paths.is_version_installed(&version) {
                CheckResult::ok("Default toolchain", format!("Set to {version}"))
            } else {
                CheckResult::error(
                    "Default toolchain",
                    format!("{version} is set as default but not installed"),
                )
            }
        }
        Ok(None) => CheckResult::warning(
            "Default toolchain",
            "Not set. Run 'infs install' to install and set a default toolchain.",
        ),
        Err(e) => CheckResult::error("Default toolchain", format!("Cannot read: {e}")),
    }
}

/// Checks if the inf-llc binary is available.
fn check_inf_llc() -> CheckResult {
    check_binary("inf-llc", "inf-llc")
}

/// Checks if the rust-lld binary is available.
fn check_rust_lld() -> CheckResult {
    check_binary("rust-lld", "rust-lld")
}

/// Checks if a binary is available in PATH or the toolchain bin directory.
fn check_binary(name: &'static str, binary_name: &str) -> CheckResult {
    let Ok(platform) = Platform::detect() else {
        return CheckResult::error(name, "Cannot detect platform");
    };

    let binary_with_ext = format!("{binary_name}{}", platform.executable_extension());

    if which::which(&binary_with_ext).is_ok() {
        return CheckResult::ok(name, format!("Found {binary_with_ext} in PATH"));
    }

    let Ok(paths) = ToolchainPaths::new() else {
        return CheckResult::error(name, "Cannot determine toolchain paths");
    };

    let default_version = match paths.get_default_version() {
        Ok(Some(v)) => v,
        Ok(None) => {
            return CheckResult::warning(
                name,
                "No default toolchain set. Run 'infs install' first.",
            )
        }
        Err(_) => return CheckResult::error(name, "Cannot read default version"),
    };

    let binary_path = paths.binary_path(&default_version, &binary_with_ext);
    if binary_path.exists() {
        CheckResult::ok(name, format!("Found at {}", binary_path.display()))
    } else {
        CheckResult::error(
            name,
            format!(
                "Not found. Expected at {}. Run 'infs install' to install the toolchain.",
                binary_path.display()
            ),
        )
    }
}

/// Checks if libLLVM is available (Linux only).
#[cfg(target_os = "linux")]
fn check_libllvm() -> CheckResult {
    let Ok(paths) = ToolchainPaths::new() else {
        return CheckResult::error("libLLVM", "Cannot determine toolchain paths");
    };

    let default_version = match paths.get_default_version() {
        Ok(Some(v)) => v,
        Ok(None) => {
            return CheckResult::warning(
                "libLLVM",
                "No default toolchain set. Run 'infs install' first.",
            )
        }
        Err(_) => return CheckResult::error("libLLVM", "Cannot read default version"),
    };

    let lib_dir = paths.toolchain_dir(&default_version).join("lib");

    if !lib_dir.exists() {
        return CheckResult::warning(
            "libLLVM",
            format!("Library directory not found at {}", lib_dir.display()),
        );
    }

    let Ok(entries) = std::fs::read_dir(&lib_dir) else {
        return CheckResult::warning(
            "libLLVM",
            format!("Cannot read library directory: {}", lib_dir.display()),
        );
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with("libLLVM") && name_str.contains(".so") {
            return CheckResult::ok(
                "libLLVM",
                format!("Found {}", entry.path().display()),
            );
        }
    }

    CheckResult::warning(
        "libLLVM",
        format!(
            "Not found in {}. Some features may not work.",
            lib_dir.display()
        ),
    )
}
