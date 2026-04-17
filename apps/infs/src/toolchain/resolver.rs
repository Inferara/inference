//! Compiler binary resolution for the infs CLI.
//!
//! This module provides functionality for locating the `infc` compiler binary
//! across different installation contexts. The search order prioritizes:
//!
//! 1. Explicit override via `INFC_PATH` environment variable
//! 2. Cargo-workspace sibling at `target/<profile>/infc[.exe]`
//! 3. System PATH via `which::which("infc")`
//! 4. Managed toolchain at `~/.inference/toolchains/VERSION/infc`
//!
//! ## Environment Variables
//!
//! - `INFC_PATH`: Explicit path to the infc binary (highest priority)
//! - `INFS_VERBOSE`: When set to a non-empty, non-"0" value, emits resolution
//!   trace lines to stderr describing which priority resolved `infc`
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
use std::path::{Path, PathBuf};

use crate::toolchain::paths::ToolchainPaths;
use crate::toolchain::platform::Platform;

/// Environment variable for explicit infc binary path override.
const INFC_PATH_ENV: &str = "INFC_PATH";

/// Returns true if `INFS_VERBOSE` is set to a non-empty non-"0" value.
fn verbose() -> bool {
    std::env::var_os("INFS_VERBOSE").is_some_and(|v| !v.is_empty() && v != "0")
}

/// Emits a resolution trace line to stderr under `INFS_VERBOSE`.
fn trace_resolved(source: &str, path: &Path) {
    if verbose() {
        eprintln!("infs: resolved infc via {source}: {}", path.display());
    }
}

// Testable seam for `std::env::current_exe()`. Under `#[cfg(test)]`, a
// thread-local override allows unit tests to simulate arbitrary executable
// locations without touching the real process state. Production builds
// always delegate to `std::env`.
#[cfg(test)]
thread_local! {
    static CURRENT_EXE_OVERRIDE: std::cell::RefCell<Option<PathBuf>>
        = const { std::cell::RefCell::new(None) };
}

fn current_exe_for_resolver() -> std::io::Result<PathBuf> {
    #[cfg(test)]
    if let Some(p) = CURRENT_EXE_OVERRIDE.with(|c| c.borrow().clone()) {
        return Ok(p);
    }
    std::env::current_exe()
}

/// Priority-2 (relative to PATH): when `infs` was itself cargo-built into
/// `target/<profile>/infs[.exe]`, a sibling `infc[.exe]` in the same dir is
/// assumed to be the paired build. Falls through silently on any error.
///
/// Canonicalizes the current exe first for the common case where cargo
/// invokes via a symlink, but falls back to the raw path when
/// `canonicalize()` fails (broken symlinks, restricted ACLs, some
/// container `/proc/self/exe` setups).
fn workspace_sibling_infc() -> Option<PathBuf> {
    let raw = current_exe_for_resolver().ok()?;
    let canonical = raw.canonicalize().ok();
    if canonical.is_none() && verbose() {
        eprintln!(
            "infs: canonicalize failed for {}; trying raw path",
            raw.display()
        );
    }
    workspace_sibling_infc_from(canonical.as_deref().unwrap_or(&raw))
}

fn workspace_sibling_infc_from(exe: &Path) -> Option<PathBuf> {
    let platform = Platform::detect().ok()?;
    let ext = platform.executable_extension();
    let dir = exe.parent()?;
    let profile = dir.file_name()?.to_str()?;
    if profile != "debug" && profile != "release" {
        return None;
    }
    if dir.parent()?.file_name()?.to_str()? != "target" {
        return None;
    }
    let expected_infs = format!("infs{ext}");
    let actual = exe.file_name()?.to_str()?;
    let matches = if platform.is_windows() {
        actual.eq_ignore_ascii_case(&expected_infs)
    } else {
        actual == expected_infs
    };
    if !matches {
        return None;
    }
    let candidate = dir.join(format!("infc{ext}"));
    candidate.is_file().then_some(candidate)
}

/// Locates the `infc` compiler binary.
///
/// Searches for the infc binary in the following priority order:
///
/// 1. **`INFC_PATH` environment variable** - Explicit override for testing
///    or custom installations (hardest override)
/// 2. **Workspace sibling** - When `infs` is running from
///    `target/<profile>/infs[.exe]`, prefer the paired
///    `target/<profile>/infc[.exe]` if present
/// 3. **System PATH** - Uses `which::which("infc")` to find infc in PATH
/// 4. **Managed toolchain** - Looks in `~/.inference/toolchains/VERSION/infc`
///    using the default toolchain version if set
///
/// Fallthrough on any priority's failure is intentional; each priority is
/// best-effort. Set `INFS_VERBOSE=1` to trace which priority resolved
/// `infc` on stderr.
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
            trace_resolved("INFC_PATH env", &path);
            return Ok(path);
        }
        bail!(
            "INFC_PATH environment variable set to '{}', but file does not exist",
            path.display()
        );
    }

    // Priority 2: cargo-workspace sibling infc
    if let Some(path) = workspace_sibling_infc() {
        trace_resolved("workspace sibling", &path);
        return Ok(path);
    }

    // Priority 3: System PATH
    if let Ok(path) = which::which("infc") {
        trace_resolved("PATH", &path);
        return Ok(path);
    }

    // Priority 4: Managed toolchain
    if let Ok(paths) = ToolchainPaths::new()
        && let Ok(Some(version)) = paths.get_default_version()
    {
        let platform =
            Platform::detect().context("Failed to detect platform while searching for infc")?;
        let ext = platform.executable_extension();
        let infc_name = format!("infc{ext}");
        let infc_path = paths.binary_path(&version, &infc_name);

        if infc_path.exists() {
            trace_resolved("managed toolchain", &infc_path);
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

    /// RAII guard that restores `CURRENT_EXE_OVERRIDE` on drop.
    struct ExeOverrideGuard;

    impl ExeOverrideGuard {
        fn set(path: PathBuf) -> Self {
            CURRENT_EXE_OVERRIDE.with(|c| *c.borrow_mut() = Some(path));
            Self
        }
    }

    impl Drop for ExeOverrideGuard {
        fn drop(&mut self) {
            CURRENT_EXE_OVERRIDE.with(|c| *c.borrow_mut() = None);
        }
    }

    fn exe_name(name: &str) -> String {
        let ext = Platform::detect().unwrap().executable_extension();
        format!("{name}{ext}")
    }

    #[test]
    #[serial_test::serial]
    fn infc_path_env_nonexistent_returns_error() {
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
        let original_path = env::var("PATH").unwrap_or_default();

        // SAFETY: This test runs in isolation and we restore the env vars at the end.
        unsafe {
            env::set_var("PATH", "");
            env::remove_var(INFC_PATH_ENV);

            let temp_dir = env::temp_dir().join("infs_test_resolver");
            env::set_var("INFERENCE_HOME", &temp_dir);
        }

        // Also suppress the workspace-sibling priority so the error path fires
        // regardless of how the test binary itself is laid out.
        let _guard = ExeOverrideGuard::set(PathBuf::from("/nonexistent/elsewhere/infs"));

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

    #[test]
    fn sibling_infc_found_when_exe_in_target_debug() {
        let temp = assert_fs::TempDir::new().unwrap();
        let debug = temp.path().join("target").join("debug");
        std::fs::create_dir_all(&debug).unwrap();
        let infs_path = debug.join(exe_name("infs"));
        let infc_path = debug.join(exe_name("infc"));
        std::fs::write(&infs_path, b"").unwrap();
        std::fs::write(&infc_path, b"").unwrap();

        let result = workspace_sibling_infc_from(&infs_path);
        assert_eq!(result.as_deref(), Some(infc_path.as_path()));
    }

    #[test]
    fn sibling_infc_found_when_exe_in_target_release() {
        let temp = assert_fs::TempDir::new().unwrap();
        let release = temp.path().join("target").join("release");
        std::fs::create_dir_all(&release).unwrap();
        let infs_path = release.join(exe_name("infs"));
        let infc_path = release.join(exe_name("infc"));
        std::fs::write(&infs_path, b"").unwrap();
        std::fs::write(&infc_path, b"").unwrap();

        let result = workspace_sibling_infc_from(&infs_path);
        assert_eq!(result.as_deref(), Some(infc_path.as_path()));
    }

    #[test]
    fn sibling_returns_none_when_exe_not_in_target() {
        // No target/<profile>/ ancestor — should reject regardless of file presence.
        let fabricated = PathBuf::from("/usr/local/bin").join(exe_name("infs"));
        let result = workspace_sibling_infc_from(&fabricated);
        assert_eq!(result, None);
    }

    #[test]
    fn sibling_returns_none_when_only_infs_present() {
        let temp = assert_fs::TempDir::new().unwrap();
        let debug = temp.path().join("target").join("debug");
        std::fs::create_dir_all(&debug).unwrap();
        let infs_path = debug.join(exe_name("infs"));
        std::fs::write(&infs_path, b"").unwrap();
        // Deliberately do not create infc.

        let result = workspace_sibling_infc_from(&infs_path);
        assert_eq!(result, None);
    }

    #[test]
    fn sibling_returns_none_when_parent_not_target() {
        let temp = assert_fs::TempDir::new().unwrap();
        // build/debug/ instead of target/debug/ — shape must reject.
        let build = temp.path().join("build").join("debug");
        std::fs::create_dir_all(&build).unwrap();
        let infs_path = build.join(exe_name("infs"));
        let infc_path = build.join(exe_name("infc"));
        std::fs::write(&infs_path, b"").unwrap();
        std::fs::write(&infc_path, b"").unwrap();

        let result = workspace_sibling_infc_from(&infs_path);
        assert_eq!(result, None);
    }

    #[test]
    #[serial_test::serial]
    fn infc_path_env_overrides_sibling() {
        // Build a plausible workspace-sibling layout that would satisfy L1...
        let workspace = assert_fs::TempDir::new().unwrap();
        let debug = workspace.path().join("target").join("debug");
        std::fs::create_dir_all(&debug).unwrap();
        let infs_path = debug.join(exe_name("infs"));
        let sibling_infc = debug.join(exe_name("infc"));
        std::fs::write(&infs_path, b"").unwrap();
        std::fs::write(&sibling_infc, b"").unwrap();

        // ...then also set INFC_PATH to a distinct file.
        let env_dir = assert_fs::TempDir::new().unwrap();
        let env_infc = env_dir.path().join(exe_name("infc"));
        std::fs::write(&env_infc, b"").unwrap();

        let _guard = ExeOverrideGuard::set(infs_path);

        // SAFETY: serialized test; we restore immediately after.
        unsafe {
            env::set_var(INFC_PATH_ENV, &env_infc);
        }

        let result = find_infc();

        // SAFETY: cleanup regardless of assertion outcome.
        unsafe {
            env::remove_var(INFC_PATH_ENV);
        }

        let resolved = result.unwrap();
        assert_eq!(
            resolved, env_infc,
            "INFC_PATH must outrank the workspace sibling"
        );
        assert_ne!(
            resolved, sibling_infc,
            "workspace sibling must not win when INFC_PATH is set"
        );
    }

    #[test]
    #[serial_test::serial]
    fn verbose_false_when_unset() {
        // SAFETY: serialized; state restored.
        unsafe {
            env::remove_var("INFS_VERBOSE");
        }
        assert!(!verbose());
    }

    #[test]
    #[serial_test::serial]
    fn verbose_true_when_set() {
        // SAFETY: serialized; state restored.
        unsafe {
            env::set_var("INFS_VERBOSE", "1");
        }
        let got = verbose();
        unsafe {
            env::remove_var("INFS_VERBOSE");
        }
        assert!(got);
    }

    #[test]
    #[serial_test::serial]
    fn verbose_false_when_set_to_zero() {
        // SAFETY: serialized; state restored.
        unsafe {
            env::set_var("INFS_VERBOSE", "0");
        }
        let got = verbose();
        unsafe {
            env::remove_var("INFS_VERBOSE");
        }
        assert!(!got);
    }

    #[test]
    #[serial_test::serial]
    fn verbose_false_when_empty_string() {
        // SAFETY: serialized; state restored.
        unsafe {
            env::set_var("INFS_VERBOSE", "");
        }
        let got = verbose();
        unsafe {
            env::remove_var("INFS_VERBOSE");
        }
        assert!(!got);
    }

    #[test]
    fn broken_exe_override_falls_through_to_none() {
        // Inject a path that doesn't exist on disk. canonicalize() will fail;
        // the raw path shape (/nonexistent/elsewhere/infs) doesn't match
        // target/<profile>/infs either, so the expected outcome is None.
        let _guard = ExeOverrideGuard::set(PathBuf::from("/nonexistent/elsewhere/infs"));
        assert_eq!(workspace_sibling_infc(), None);
    }

    #[test]
    fn canonicalize_fails_but_raw_matches_shape_without_sibling() {
        // Fabricate a raw path that MATCHES the target/<profile>/infs shape
        // but points to a nonexistent location. canonicalize() fails; raw
        // path passes shape check; but the sibling infc doesn't exist, so
        // the function still returns None. Exercises the raw-path fallback
        // branch end-to-end.
        let fabricated = PathBuf::from("/nonexistent")
            .join("target")
            .join("debug")
            .join(exe_name("infs"));
        let _guard = ExeOverrideGuard::set(fabricated);
        assert_eq!(workspace_sibling_infc(), None);
    }
}
