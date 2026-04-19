//! PATH conflict detection module.
//!
//! This module provides functionality to detect when a binary in the user's PATH
//! shadows the managed toolchain binary. This helps users understand why the
//! managed toolchain might not be used when they run commands.
//!
//! It also enumerates *all* `infc` binaries on `PATH` so developers can
//! see duplicates — e.g. a stale `~/bin/infc` shadowed by a fresh
//! `/usr/local/bin/infc`. This is a common failure mode that single-hit
//! `which::which` lookups hide.
//!
//! ## Usage
//!
//! ```ignore
//! use infs::toolchain::conflict::{detect_path_conflicts, format_conflict_warning};
//! use std::path::Path;
//!
//! let bin_dir = Path::new("/home/user/.inference/bin");
//! let conflicts = detect_path_conflicts(bin_dir);
//! if !conflicts.is_empty() {
//!     eprintln!("{}", format_conflict_warning(&conflicts));
//! }
//! ```

use std::path::{Path, PathBuf};

use super::Platform;
use super::paths::ToolchainPaths;

/// Represents a conflict where a binary in PATH shadows the managed version.
#[derive(Debug, Clone)]
pub struct PathConflict {
    /// Name of the binary (e.g., "infc").
    pub binary: String,
    /// Path where the binary was found in PATH.
    pub found: PathBuf,
    /// Expected path within the managed toolchain.
    pub expected: PathBuf,
}

/// Detects PATH conflicts for the managed `infc` binary.
///
/// Checks if the managed binary is found in PATH at a location different
/// from the managed bin directory.
///
/// A conflict is reported when:
/// 1. The binary is found in PATH
/// 2. The found path differs from the expected managed path
/// 3. The expected managed binary actually exists
///
/// # Arguments
///
/// * `bin_dir` - The managed toolchain bin directory (e.g., `~/.inference/bin`)
///
/// # Returns
///
/// A vector of `PathConflict` for each binary that has a conflict.
#[must_use]
pub fn detect_path_conflicts(bin_dir: &Path) -> Vec<PathConflict> {
    let Ok(platform) = Platform::detect() else {
        return vec![];
    };
    let ext = platform.executable_extension();

    let mut conflicts = Vec::new();

    let binary_with_ext = format!("{}{ext}", ToolchainPaths::MANAGED_BINARY);
    let expected = bin_dir.join(&binary_with_ext);

    if let Ok(found_path) = which::which(&binary_with_ext)
        && found_path != expected
        && expected.exists()
    {
        conflicts.push(PathConflict {
            binary: binary_with_ext,
            found: found_path,
            expected,
        });
    }

    conflicts
}

/// Enumerates every `infc` binary visible on the current `PATH`, in
/// first-wins order (the same order `which::which` would traverse).
///
/// Returns an empty vector when nothing is found or when platform
/// detection fails. More than one entry means the later entries are
/// shadowed; `infs build` will invoke the first one.
///
/// Uses [`which::which_all`] rather than [`which::which`] so both
/// active and shadowed binaries are visible — the common pitfall a
/// single-hit lookup hides. No new crate dependency: `which` is
/// already in the tree and v8 exposes `which_all` directly.
#[must_use]
pub fn enumerate_infc_on_path() -> Vec<PathBuf> {
    let Ok(platform) = Platform::detect() else {
        return vec![];
    };
    let binary_with_ext = format!("{}{}", ToolchainPaths::MANAGED_BINARY, platform.executable_extension());
    which::which_all(&binary_with_ext)
        .map(Iterator::collect)
        .unwrap_or_default()
}

/// Formats a multi-line warning describing duplicate `infc` binaries
/// on `PATH`. Returns an empty vector when `paths.len() <= 1`; the
/// caller is expected to check for the duplicate case before
/// rendering.
///
/// The first entry is labelled `(active)` and the rest `(shadowed)`
/// because `which::which_all` iterates in the order `PATH` would
/// resolve — which is also the order `infs build` effectively uses
/// when the `PATH` priority fires.
#[must_use]
pub fn format_duplicate_path_warning(paths: &[PathBuf]) -> Vec<String> {
    if paths.len() <= 1 {
        return Vec::new();
    }

    let mut lines = Vec::new();
    lines.push("Multiple infc binaries found on PATH (first wins):".to_string());
    for (idx, path) in paths.iter().enumerate() {
        let tag = if idx == 0 { "active" } else { "shadowed" };
        lines.push(format!("  {}. {} ({})", idx + 1, path.display(), tag));
    }
    lines.push("Use INFC_PATH to pin a specific binary.".to_string());
    lines
}

/// Formats a user-friendly warning message for PATH conflicts.
///
/// The message includes:
/// - A header explaining that PATH conflicts were detected
/// - Details for each conflict showing the found and expected paths
/// - Suggestions for how to fix the conflicts
///
/// # Arguments
///
/// * `conflicts` - Slice of `PathConflict` to format
///
/// # Returns
///
/// A formatted multi-line warning string.
#[must_use]
pub fn format_conflict_warning(conflicts: &[PathConflict]) -> String {
    if conflicts.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();

    lines.push("Warning: PATH conflict detected".to_string());

    for conflict in conflicts {
        lines.push(format!(
            "  '{}' found at: {}",
            conflict.binary,
            conflict.found.display()
        ));
        lines.push(format!(
            "  Expected:        {}",
            conflict.expected.display()
        ));
    }

    lines.push(String::new());
    lines.push("The managed toolchain may not be used. To fix:".to_string());

    if let Some(first_conflict) = conflicts.first()
        && let Some(parent) = first_conflict.found.parent()
    {
        lines.push(format!(
            "  - Remove {} from your PATH, or",
            parent.display()
        ));
    }

    if let Some(first_conflict) = conflicts.first()
        && let Some(parent) = first_conflict.expected.parent()
    {
        lines.push(format!(
            "  - Ensure {} comes before other paths in $PATH",
            parent.display()
        ));
    }

    lines.push("  - Run 'infs doctor' for more information".to_string());

    lines.join("\n")
}

/// Formats a conflict warning for the doctor command output.
///
/// This produces a more compact format suitable for display alongside
/// other doctor checks.
///
/// # Arguments
///
/// * `conflicts` - Slice of `PathConflict` to format
///
/// # Returns
///
/// A formatted warning string for doctor output.
#[must_use]
pub fn format_doctor_conflict_warning(conflicts: &[PathConflict]) -> Vec<String> {
    let mut lines = Vec::new();

    for conflict in conflicts {
        lines.push(format!(
            "'{}' resolves to {}",
            conflict.binary,
            conflict.found.display()
        ));
        lines.push(format!(
            "  but managed version is at {}",
            conflict.expected.display()
        ));
    }

    if let Some(first_conflict) = conflicts.first()
        && let Some(parent) = first_conflict.expected.parent()
    {
        lines.push(String::new());
        lines.push(format!(
            "  Fix: Ensure {} comes before other paths in $PATH",
            parent.display()
        ));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn detect_conflicts_returns_empty_for_nonexistent_binaries() {
        let temp_dir = env::temp_dir().join("infs_conflict_test_empty");
        let conflicts = detect_path_conflicts(&temp_dir);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn format_warning_returns_empty_for_no_conflicts() {
        let conflicts: Vec<PathConflict> = vec![];
        let warning = format_conflict_warning(&conflicts);
        assert!(warning.is_empty());
    }

    #[test]
    fn format_warning_includes_conflict_details() {
        let conflicts = vec![PathConflict {
            binary: "infc".to_string(),
            found: PathBuf::from("/usr/local/bin/infc"),
            expected: PathBuf::from("/home/user/.inference/bin/infc"),
        }];

        let warning = format_conflict_warning(&conflicts);

        assert!(warning.contains("Warning: PATH conflict detected"));
        assert!(warning.contains("'infc' found at: /usr/local/bin/infc"));
        assert!(warning.contains("Expected:        /home/user/.inference/bin/infc"));
        assert!(warning.contains("managed toolchain may not be used"));
    }

    #[test]
    fn format_warning_includes_fix_suggestions() {
        let conflicts = vec![PathConflict {
            binary: "infc".to_string(),
            found: PathBuf::from("/usr/local/bin/infc"),
            expected: PathBuf::from("/home/user/.inference/bin/infc"),
        }];

        let warning = format_conflict_warning(&conflicts);

        assert!(warning.contains("Remove /usr/local/bin from your PATH"));
        assert!(warning.contains("Ensure /home/user/.inference/bin comes before other paths"));
        assert!(warning.contains("Run 'infs doctor'"));
    }

    #[test]
    fn format_doctor_warning_formats_correctly() {
        let conflicts = vec![PathConflict {
            binary: "infc".to_string(),
            found: PathBuf::from("/usr/local/bin/infc"),
            expected: PathBuf::from("/home/user/.inference/bin/infc"),
        }];

        let lines = format_doctor_conflict_warning(&conflicts);

        assert!(!lines.is_empty());
        assert!(lines.iter().any(|l| l.contains("'infc' resolves to")));
        assert!(lines.iter().any(|l| l.contains("managed version is at")));
        assert!(lines.iter().any(|l| l.contains("Fix:")));
    }

    #[test]
    fn path_conflict_struct_fields_accessible() {
        let conflict = PathConflict {
            binary: "test".to_string(),
            found: PathBuf::from("/a/b/test"),
            expected: PathBuf::from("/c/d/test"),
        };

        assert_eq!(conflict.binary, "test");
        assert_eq!(conflict.found, PathBuf::from("/a/b/test"));
        assert_eq!(conflict.expected, PathBuf::from("/c/d/test"));
    }

    #[test]
    fn format_conflict_warning_handles_multiple_conflicts() {
        let conflicts = vec![
            PathConflict {
                binary: "infc".to_string(),
                found: PathBuf::from("/usr/local/bin/infc"),
                expected: PathBuf::from("/home/user/.inference/bin/infc"),
            },
            PathConflict {
                binary: "infs".to_string(),
                found: PathBuf::from("/opt/bin/infs"),
                expected: PathBuf::from("/home/user/.inference/bin/infs"),
            },
        ];

        let warning = format_conflict_warning(&conflicts);

        assert!(warning.contains("'infc' found at: /usr/local/bin/infc"));
        assert!(warning.contains("'infs' found at: /opt/bin/infs"));
        assert!(warning.contains("Expected:        /home/user/.inference/bin/infc"));
        assert!(warning.contains("Expected:        /home/user/.inference/bin/infs"));
    }

    #[test]
    fn format_doctor_conflict_warning_returns_empty_for_no_conflicts() {
        let conflicts: Vec<PathConflict> = vec![];
        let lines = format_doctor_conflict_warning(&conflicts);
        assert!(lines.is_empty());
    }

    #[test]
    fn format_doctor_conflict_warning_handles_multiple_conflicts() {
        let conflicts = vec![
            PathConflict {
                binary: "infc".to_string(),
                found: PathBuf::from("/usr/local/bin/infc"),
                expected: PathBuf::from("/home/user/.inference/bin/infc"),
            },
            PathConflict {
                binary: "infs".to_string(),
                found: PathBuf::from("/opt/bin/infs"),
                expected: PathBuf::from("/home/user/.inference/bin/infs"),
            },
        ];

        let lines = format_doctor_conflict_warning(&conflicts);

        assert!(
            lines
                .iter()
                .any(|l| l.contains("'infc' resolves to /usr/local/bin/infc"))
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("'infs' resolves to /opt/bin/infs"))
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("managed version is at /home/user/.inference/bin/infc"))
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("managed version is at /home/user/.inference/bin/infs"))
        );
        assert!(lines.iter().any(|l| l.contains("Fix:")));
    }

    #[test]
    fn path_conflict_is_clone() {
        let conflict = PathConflict {
            binary: "test".to_string(),
            found: PathBuf::from("/a/b/test"),
            expected: PathBuf::from("/c/d/test"),
        };
        let cloned = conflict.clone();
        assert_eq!(cloned.binary, conflict.binary);
        assert_eq!(cloned.found, conflict.found);
        assert_eq!(cloned.expected, conflict.expected);
    }

    #[test]
    fn path_conflict_is_debug() {
        let conflict = PathConflict {
            binary: "test".to_string(),
            found: PathBuf::from("/a/b/test"),
            expected: PathBuf::from("/c/d/test"),
        };
        let debug_str = format!("{conflict:?}");
        assert!(debug_str.contains("PathConflict"));
        assert!(debug_str.contains("test"));
    }

    /// Creates an executable `infc[.exe]` stub in `dir` so `which::which_all`
    /// will count it as a match.
    fn write_executable_infc_stub(dir: &Path) -> PathBuf {
        let platform = Platform::detect().unwrap();
        let name = format!("{}{}", ToolchainPaths::MANAGED_BINARY, platform.executable_extension());
        let stub = dir.join(&name);
        std::fs::write(&stub, b"#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&stub).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&stub, perms).unwrap();
        }
        stub
    }

    #[test]
    #[serial_test::serial]
    fn multiple_infc_on_path_all_reported() {
        let dir_a = assert_fs::TempDir::new().unwrap();
        let dir_b = assert_fs::TempDir::new().unwrap();
        let stub_a = write_executable_infc_stub(dir_a.path());
        let stub_b = write_executable_infc_stub(dir_b.path());

        let original_path = env::var("PATH").unwrap_or_default();
        let joined = env::join_paths([dir_a.path(), dir_b.path()]).unwrap();

        // SAFETY: serialized; cleaned up below regardless of outcome.
        unsafe {
            env::set_var("PATH", &joined);
        }

        let found = enumerate_infc_on_path();

        // SAFETY: restore PATH before assertions so a panic doesn't leak state.
        unsafe {
            env::set_var("PATH", original_path);
        }

        // Some CI sandboxes resolve symlinks asymmetrically; accept either
        // the raw tempdir path or its canonical form, but require PATH order.
        let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        assert_eq!(found.len(), 2, "expected both infc stubs to be enumerated: {found:?}");
        assert_eq!(canon(&found[0]), canon(&stub_a), "first hit must be the first PATH entry");
        assert_eq!(canon(&found[1]), canon(&stub_b), "second hit must be the second PATH entry");
    }

    #[test]
    #[serial_test::serial]
    fn single_infc_on_path_no_duplicate_warning() {
        let dir = assert_fs::TempDir::new().unwrap();
        write_executable_infc_stub(dir.path());

        let original_path = env::var("PATH").unwrap_or_default();

        // SAFETY: serialized; cleaned up below regardless of outcome.
        unsafe {
            env::set_var("PATH", dir.path());
        }

        let found = enumerate_infc_on_path();

        // SAFETY: restore PATH before assertions.
        unsafe {
            env::set_var("PATH", original_path);
        }

        assert_eq!(found.len(), 1, "exactly one infc should be visible: {found:?}");
        // A single entry must not trigger the duplicate-warning block.
        let warning = format_duplicate_path_warning(&found);
        assert!(
            warning.is_empty(),
            "single PATH entry must not produce a duplicate warning: {warning:?}"
        );
    }

    #[test]
    #[serial_test::serial]
    fn no_infc_on_path_no_warning() {
        let empty_dir = assert_fs::TempDir::new().unwrap();
        let original_path = env::var("PATH").unwrap_or_default();

        // SAFETY: serialized; cleaned up below regardless of outcome.
        unsafe {
            env::set_var("PATH", empty_dir.path());
        }

        let found = enumerate_infc_on_path();

        // SAFETY: restore PATH before assertions.
        unsafe {
            env::set_var("PATH", original_path);
        }

        assert!(
            found.is_empty(),
            "empty PATH directory must yield no infc matches: {found:?}"
        );
        let warning = format_duplicate_path_warning(&found);
        assert!(warning.is_empty(), "empty enumeration must not warn");
    }

    #[test]
    fn format_duplicate_path_warning_lists_active_and_shadowed() {
        let paths = vec![
            PathBuf::from("/usr/local/bin/infc"),
            PathBuf::from("/home/user/bin/infc"),
        ];
        let lines = format_duplicate_path_warning(&paths);

        assert!(!lines.is_empty());
        assert!(lines.iter().any(|l| l.contains("Multiple infc binaries")));
        assert!(lines.iter().any(|l| l.contains("1. /usr/local/bin/infc (active)")));
        assert!(lines.iter().any(|l| l.contains("2. /home/user/bin/infc (shadowed)")));
        assert!(lines.iter().any(|l| l.contains("INFC_PATH")));
    }

    #[test]
    fn format_duplicate_path_warning_empty_for_zero_paths() {
        let lines = format_duplicate_path_warning(&[]);
        assert!(lines.is_empty());
    }
}
