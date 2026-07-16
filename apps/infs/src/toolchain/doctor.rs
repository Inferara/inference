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
//! - Bundled `inference-lsp` language server presence

use std::fmt::Write as _;
use std::path::Path;

use super::conflict::enumerate_infc_on_path;
use super::resolver::{self, find_infc_with_source};
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
///
/// Order matters: the VS Code extension parses the printed check lines in
/// order. New checks must be appended, never inserted mid-list, so the
/// extension's append-only parsing contract remains stable.
pub fn run_all_checks() -> Vec<DoctorCheck> {
    let mut checks = vec![
        check_infs_binary(),
        check_platform(),
        check_toolchain_directory(),
        check_default_toolchain(),
        check_infc(),
        check_resolved_infc(),
    ];
    if let Some(ambiguity) = check_resolution_ambiguity() {
        checks.push(ambiguity);
    }
    checks.push(crate::commands::wasm_opt::doctor_check());
    checks.push(check_inference_lsp());
    checks
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
///
/// Enumerates *every* `infc` on `PATH` (not just the first hit) so
/// developers can see shadowed copies at a glance. The output stays
/// on one line per the VS Code `[OK|WARN|FAIL] <name>: <msg>`
/// contract; duplicates are inlined with `; ` separators.
#[must_use]
pub fn check_infc() -> DoctorCheck {
    let Ok(platform) = Platform::detect() else {
        return DoctorCheck::error("infc", "Cannot detect platform");
    };

    let binary_with_ext = format!("infc{}", platform.executable_extension());

    let on_path = enumerate_infc_on_path();
    match on_path.len() {
        0 => {} // Fall through to managed-toolchain check below.
        1 => {
            return DoctorCheck::ok("infc", format!("Found {binary_with_ext} in PATH"));
        }
        _ => {
            let enumerated = on_path
                .iter()
                .enumerate()
                .map(|(idx, p)| format!("{}. {}", idx + 1, p.display()))
                .collect::<Vec<_>>()
                .join("; ");
            return DoctorCheck::warning(
                "infc",
                format!(
                    "{} {binary_with_ext} binaries on PATH (first wins): {enumerated}",
                    on_path.len()
                ),
            );
        }
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

/// Reports which `infc` binary `infs build` will actually invoke, and
/// which priority in [`find_infc_with_source`] fired.
///
/// Complements [`check_infc`], which only confirms *availability*. This
/// check tells the user *why* one binary was selected over another —
/// critical for developers whose machine has both a workspace sibling and
/// a managed toolchain installed.
#[must_use]
pub fn check_resolved_infc() -> DoctorCheck {
    match find_infc_with_source() {
        Ok((path, source)) => DoctorCheck::ok(
            "Resolved infc",
            format!("{} (source: {})", path.display(), source.label()),
        ),
        Err(err) => DoctorCheck::warning("Resolved infc", err.to_string()),
    }
}

/// Warns when both a workspace sibling and a managed toolchain `infc`
/// exist — the exact ambiguity the priority-2 workspace-sibling rule was
/// introduced to handle silently. Surfacing it here gives developers a
/// clear knob: either intentionally run from the workspace, or remove the
/// stale managed install.
///
/// Returns `None` when no ambiguity exists, so the check is elided from
/// output in the common single-source case.
#[must_use]
pub fn check_resolution_ambiguity() -> Option<DoctorCheck> {
    let sibling = resolver::workspace_sibling_infc()?;
    let managed = resolver::managed_toolchain_infc()?;
    Some(DoctorCheck::warning(
        "Resolution ambiguity",
        format!(
            "both workspace sibling ({}) and managed toolchain ({}) exist; \
             workspace sibling wins. Remove the stale managed install or \
             set INFC_PATH to pin your choice.",
            sibling.display(),
            managed.display()
        ),
    ))
}

/// Checks whether the managed `inference-lsp` language server is available.
///
/// Newer toolchain archives bundle the LSP server alongside `infc`, so the
/// managed copy lands in the default toolchain directory (and is symlinked
/// into `bin/`). The VS Code extension resolves the server through a PATH tier
/// as well, so any `inference-lsp` also visible on `PATH` is noted on the same
/// line.
///
/// Statuses, chosen so `infs doctor` still exits zero when the optional server
/// is simply absent:
/// - **OK** — the default toolchain bundles the server; the resolved path is
///   reported, with a PATH hit appended when one exists.
/// - **OK** — no default toolchain is set at all (deferring to
///   [`check_default_toolchain`], which owns that diagnosis).
/// - **Warning** — a default toolchain is set but predates the bundling; the
///   message hints at upgrading and notes any PATH fallback.
#[must_use]
pub fn check_inference_lsp() -> DoctorCheck {
    let Ok(platform) = Platform::detect() else {
        return DoctorCheck::error("inference-lsp", "Cannot detect platform");
    };
    let binary_with_ext = format!("inference-lsp{}", platform.executable_extension());
    let path_hit = which::which(&binary_with_ext).ok();

    let Ok(paths) = ToolchainPaths::new() else {
        return DoctorCheck::error("inference-lsp", "Cannot determine toolchain paths");
    };

    inference_lsp_check(&paths, &binary_with_ext, path_hit.as_deref())
}

/// Pure decision logic for [`check_inference_lsp`], separated from the
/// environment reads (platform, PATH, toolchain root) so every branch is
/// unit-testable with a `ToolchainPaths` rooted at a temp directory.
fn inference_lsp_check(
    paths: &ToolchainPaths,
    binary_with_ext: &str,
    path_hit: Option<&Path>,
) -> DoctorCheck {
    let default_version = match paths.get_default_version() {
        Ok(Some(v)) => v,
        Ok(None) => return DoctorCheck::ok("inference-lsp", "No toolchain installed"),
        Err(e) => {
            return DoctorCheck::error("inference-lsp", format!("Cannot read default version: {e}"));
        }
    };

    let binary_path = paths.binary_path(&default_version, binary_with_ext);
    if binary_path.exists() {
        let mut message = format!("Found at {}", binary_path.display());
        if let Some(hit) = path_hit {
            let _ = write!(message, "; also on PATH at {}", hit.display());
        }
        DoctorCheck::ok("inference-lsp", message)
    } else {
        let mut message = format!(
            "toolchain {default_version} does not include inference-lsp; \
             install a newer toolchain to add the language server"
        );
        if let Some(hit) = path_hit {
            let _ = write!(message, "; a copy is available on PATH at {}", hit.display());
        }
        DoctorCheck::warning("inference-lsp", message)
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
        // Base checks: infs, platform, toolchain dir, default toolchain,
        // infc, resolved infc, wasm-opt, inference-lsp. Ambiguity check is
        // conditional (0 or 1).
        assert!(
            checks.len() == 8 || checks.len() == 9,
            "unexpected check count: {}",
            checks.len()
        );
    }

    #[test]
    fn check_platform_returns_result() {
        let check = check_platform();
        assert!(!check.name.is_empty());
        assert!(!check.message.is_empty());
    }

    #[test]
    fn check_infc_returns_valid_doctor_check() {
        let check = check_infc();
        assert_eq!(check.name, "infc");
        assert!(!check.message.is_empty());
        // On dev machines, infc may or may not be available.
        // We verify the check returns a valid status regardless of installation state.
        assert!(
            check.status == DoctorCheckStatus::Ok
                || check.status == DoctorCheckStatus::Warning
                || check.status == DoctorCheckStatus::Error
        );
    }

    #[test]
    fn check_infs_binary_returns_valid_doctor_check() {
        let check = check_infs_binary();
        assert!(!check.name.is_empty());
        assert!(!check.message.is_empty());
    }

    #[test]
    fn check_toolchain_directory_returns_valid_doctor_check() {
        let check = check_toolchain_directory();
        assert!(!check.name.is_empty());
        assert!(!check.message.is_empty());
    }

    #[test]
    fn check_default_toolchain_returns_valid_doctor_check() {
        let check = check_default_toolchain();
        assert!(!check.name.is_empty());
        assert!(!check.message.is_empty());
    }

    #[test]
    fn no_default_toolchain_message_with_no_versions() {
        let temp_dir = std::env::temp_dir().join("infs_test_doctor_no_default");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let paths = ToolchainPaths::with_root(temp_dir.clone());
        std::fs::create_dir_all(&paths.toolchains).unwrap();

        let msg = no_default_toolchain_message(&paths);
        assert!(msg.contains("infs install"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    #[serial_test::serial]
    fn check_resolved_infc_returns_valid_doctor_check() {
        // find_infc_with_source may succeed (Ok) or fail (Warning) depending
        // on the test environment — both are valid outcomes. We verify the
        // returned DoctorCheck is well-formed and uses the expected name.
        let check = check_resolved_infc();
        assert_eq!(check.name, "Resolved infc");
        assert!(!check.message.is_empty());
        assert!(
            check.status == DoctorCheckStatus::Ok
                || check.status == DoctorCheckStatus::Warning,
            "unexpected status: {:?}",
            check.status
        );
        // When resolution succeeds, the message must contain the "source:"
        // tag so users can see which priority fired.
        if check.status == DoctorCheckStatus::Ok {
            assert!(
                check.message.contains("(source: "),
                "ok message missing source tag: {}",
                check.message
            );
        }
    }

    #[test]
    fn no_default_toolchain_message_with_installed_versions() {
        let temp_dir = std::env::temp_dir().join("infs_test_doctor_no_default_installed");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let paths = ToolchainPaths::with_root(temp_dir.clone());
        std::fs::create_dir_all(paths.toolchain_dir("0.1.0")).unwrap();

        let msg = no_default_toolchain_message(&paths);
        assert!(msg.contains("infs default"));
        assert!(msg.contains("0.1.0"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    /// The bundled `inference-lsp` binary name for the running platform.
    fn lsp_binary_name() -> String {
        let ext = Platform::detect().unwrap().executable_extension();
        format!("inference-lsp{ext}")
    }

    #[test]
    fn check_inference_lsp_returns_valid_doctor_check() {
        let check = check_inference_lsp();
        assert_eq!(check.name, "inference-lsp");
        assert!(!check.message.is_empty());
        assert!(
            check.status == DoctorCheckStatus::Ok
                || check.status == DoctorCheckStatus::Warning
                || check.status == DoctorCheckStatus::Error
        );
    }

    #[test]
    fn inference_lsp_check_ok_when_no_default_toolchain() {
        let temp_dir = std::env::temp_dir().join("infs_test_lsp_no_default");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let paths = ToolchainPaths::with_root(temp_dir.clone());

        let check = inference_lsp_check(&paths, &lsp_binary_name(), None);
        assert_eq!(check.status, DoctorCheckStatus::Ok);
        assert_eq!(check.message, "No toolchain installed");

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn inference_lsp_check_ok_when_toolchain_bundles_server() {
        let temp_dir = std::env::temp_dir().join("infs_test_lsp_present");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let paths = ToolchainPaths::with_root(temp_dir.clone());
        let binary = lsp_binary_name();

        std::fs::create_dir_all(paths.toolchain_dir("0.2.0")).unwrap();
        std::fs::write(paths.binary_path("0.2.0", &binary), b"lsp").unwrap();
        paths.set_default_version("0.2.0").unwrap();

        let check = inference_lsp_check(&paths, &binary, None);
        assert_eq!(check.status, DoctorCheckStatus::Ok);
        assert!(check.message.starts_with("Found at"));
        assert!(!check.message.contains("also on PATH"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn inference_lsp_check_notes_path_hit_when_present() {
        let temp_dir = std::env::temp_dir().join("infs_test_lsp_present_path");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let paths = ToolchainPaths::with_root(temp_dir.clone());
        let binary = lsp_binary_name();

        std::fs::create_dir_all(paths.toolchain_dir("0.2.0")).unwrap();
        std::fs::write(paths.binary_path("0.2.0", &binary), b"lsp").unwrap();
        paths.set_default_version("0.2.0").unwrap();

        let hit = Path::new("/usr/local/bin/inference-lsp");
        let check = inference_lsp_check(&paths, &binary, Some(hit));
        assert_eq!(check.status, DoctorCheckStatus::Ok);
        assert!(check.message.contains("also on PATH at /usr/local/bin/inference-lsp"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn inference_lsp_check_warns_when_toolchain_predates_bundling() {
        let temp_dir = std::env::temp_dir().join("infs_test_lsp_absent");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let paths = ToolchainPaths::with_root(temp_dir.clone());

        // Default toolchain exists but does not bundle inference-lsp.
        std::fs::create_dir_all(paths.toolchain_dir("0.1.0")).unwrap();
        paths.set_default_version("0.1.0").unwrap();

        let check = inference_lsp_check(&paths, &lsp_binary_name(), None);
        assert_eq!(check.status, DoctorCheckStatus::Warning);
        assert!(check.message.contains("does not include inference-lsp"));
        assert!(check.message.contains("0.1.0"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn inference_lsp_check_warns_but_notes_path_fallback() {
        let temp_dir = std::env::temp_dir().join("infs_test_lsp_absent_path");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let paths = ToolchainPaths::with_root(temp_dir.clone());

        std::fs::create_dir_all(paths.toolchain_dir("0.1.0")).unwrap();
        paths.set_default_version("0.1.0").unwrap();

        let hit = Path::new("/opt/tools/inference-lsp");
        let check = inference_lsp_check(&paths, &lsp_binary_name(), Some(hit));
        assert_eq!(check.status, DoctorCheckStatus::Warning);
        assert!(check.message.contains("does not include inference-lsp"));
        assert!(check.message.contains("a copy is available on PATH at /opt/tools/inference-lsp"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
