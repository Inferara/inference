//! PATH conflict detection module.
//!
//! This module provides functionality to detect when binaries in the user's PATH
//! shadow the managed toolchain binaries. This helps users understand why the
//! managed toolchain might not be used when they run commands.
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

/// Detects PATH conflicts for managed binaries.
///
/// Checks if any of the managed binaries (`infc`, `inf-llc`, `rust-lld`) are found
/// in PATH at a location different from the managed bin directory.
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

    for name in ToolchainPaths::MANAGED_BINARIES {
        let binary_with_ext = format!("{name}{ext}");
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
    }

    conflicts
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
                binary: "inf-llc".to_string(),
                found: PathBuf::from("/opt/bin/inf-llc"),
                expected: PathBuf::from("/home/user/.inference/bin/inf-llc"),
            },
            PathConflict {
                binary: "rust-lld".to_string(),
                found: PathBuf::from("/another/path/rust-lld"),
                expected: PathBuf::from("/home/user/.inference/bin/rust-lld"),
            },
        ];

        let warning = format_conflict_warning(&conflicts);

        assert!(warning.contains("'infc' found at: /usr/local/bin/infc"));
        assert!(warning.contains("'inf-llc' found at: /opt/bin/inf-llc"));
        assert!(warning.contains("'rust-lld' found at: /another/path/rust-lld"));
        assert!(warning.contains("Expected:        /home/user/.inference/bin/infc"));
        assert!(warning.contains("Expected:        /home/user/.inference/bin/inf-llc"));
        assert!(warning.contains("Expected:        /home/user/.inference/bin/rust-lld"));
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
                binary: "inf-llc".to_string(),
                found: PathBuf::from("/opt/bin/inf-llc"),
                expected: PathBuf::from("/home/user/.inference/bin/inf-llc"),
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
                .any(|l| l.contains("'inf-llc' resolves to /opt/bin/inf-llc"))
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("managed version is at /home/user/.inference/bin/infc"))
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("managed version is at /home/user/.inference/bin/inf-llc"))
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
}
