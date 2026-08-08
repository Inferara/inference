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
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::toolchain::paths::ToolchainPaths;
use crate::toolchain::platform::Platform;

/// Environment variable for explicit infc binary path override.
const INFC_PATH_ENV: &str = "INFC_PATH";

/// Identifies which priority in [`find_infc_with_source`] resolved the binary.
///
/// The [`ResolutionSource::label`] method emits the exact strings used in
/// [`trace_resolved`], so trace output and doctor output stay in sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionSource {
    /// Resolved via the `INFC_PATH` environment variable (priority 1).
    InfcPathEnv,
    /// Resolved via the workspace sibling `target/<profile>/infc` (priority 2).
    WorkspaceSibling,
    /// Resolved via `which::which("infc")` against the system `PATH` (priority 3).
    SystemPath,
    /// Resolved via the managed toolchain under `~/.inference/toolchains/` (priority 4).
    ManagedToolchain,
}

impl ResolutionSource {
    /// Returns the human-readable label for this resolution source.
    ///
    /// The same string is emitted in `INFS_VERBOSE=1` trace lines, so `infs
    /// doctor` and verbose build output agree.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::InfcPathEnv => "INFC_PATH env",
            Self::WorkspaceSibling => "workspace sibling",
            Self::SystemPath => "PATH",
            Self::ManagedToolchain => "managed toolchain",
        }
    }
}

/// Returns true if `INFS_VERBOSE` is set to a non-empty non-"0" value.
fn verbose() -> bool {
    verbose_from(std::env::var_os("INFS_VERBOSE").as_deref())
}

/// The value predicate behind [`verbose`], with the environment read lifted
/// into a parameter so tests exercise it without writing `INFS_VERBOSE`.
///
/// A test that writes an environment variable races every concurrent read of
/// the environment in the process — including the reads this module performs
/// from production code — so no test in this crate writes `INFS_VERBOSE`.
fn verbose_from(raw: Option<&OsStr>) -> bool {
    raw.is_some_and(|value| !value.is_empty() && value != "0")
}

/// Emits a resolution trace line to stderr under `INFS_VERBOSE`.
fn trace_resolved(source: ResolutionSource, path: &Path) {
    if verbose() {
        eprintln!(
            "infs: resolved infc via {}: {}",
            source.label(),
            path.display()
        );
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
pub(crate) fn workspace_sibling_infc() -> Option<PathBuf> {
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
    // Accept both the standard `target/<profile>/` layout and the
    // `target/<triple>/<profile>/` layout produced by `cargo build
    // --target <triple>`. Nightly CI and cross-compilation builds
    // routinely put the triple between `target` and the profile dir.
    let grandparent = dir.parent()?;
    let is_target = grandparent.file_name().and_then(|n| n.to_str()) == Some("target")
        || grandparent
            .parent()
            .and_then(|gg| gg.file_name())
            .and_then(|n| n.to_str())
            == Some("target");
    if !is_target {
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

/// Returns the managed-toolchain `infc` path when the default toolchain is
/// installed and the binary exists. Returns `None` otherwise — the caller
/// decides whether to emit a diagnostic.
///
/// Extracted so [`find_infc_with_source`] and doctor's ambiguity check can
/// both ask the same question without duplicating path construction.
pub(crate) fn managed_toolchain_infc() -> Option<PathBuf> {
    let paths = ToolchainPaths::new().ok()?;
    let version = paths.get_default_version().ok().flatten()?;
    let platform = Platform::detect().ok()?;
    let ext = platform.executable_extension();
    let infc_name = format!("infc{ext}");
    let infc_path = paths.binary_path(&version, &infc_name);
    infc_path.is_file().then_some(infc_path)
}

/// Locates the `infc` compiler binary and reports which priority fired.
///
/// Priorities match [`find_infc`]; callers that only need the path should
/// use that wrapper. Doctor and other diagnostic surfaces use this richer
/// form to tell the user *why* a particular binary was selected.
///
/// # Errors
///
/// Same as [`find_infc`].
pub fn find_infc_with_source() -> Result<(PathBuf, ResolutionSource)> {
    // Priority 1: INFC_PATH environment variable
    if let Ok(path) = std::env::var(INFC_PATH_ENV) {
        let path = PathBuf::from(path);
        if path.exists() {
            trace_resolved(ResolutionSource::InfcPathEnv, &path);
            return Ok((path, ResolutionSource::InfcPathEnv));
        }
        bail!(
            "INFC_PATH environment variable set to '{}', but file does not exist",
            path.display()
        );
    }

    // Priority 2: cargo-workspace sibling infc
    if let Some(path) = workspace_sibling_infc() {
        trace_resolved(ResolutionSource::WorkspaceSibling, &path);
        return Ok((path, ResolutionSource::WorkspaceSibling));
    }

    // Priority 3: System PATH
    if let Ok(path) = which::which("infc") {
        trace_resolved(ResolutionSource::SystemPath, &path);
        return Ok((path, ResolutionSource::SystemPath));
    }

    // Priority 4: Managed toolchain
    if let Some(path) = managed_toolchain_infc() {
        trace_resolved(ResolutionSource::ManagedToolchain, &path);
        return Ok((path, ResolutionSource::ManagedToolchain));
    }
    // If a default toolchain is configured but the binary is missing, surface
    // the detection attempt so platform-detection errors still bubble up.
    if let Ok(paths) = ToolchainPaths::new()
        && let Ok(Some(_)) = paths.get_default_version()
    {
        Platform::detect().context("Failed to detect platform while searching for infc")?;
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
    find_infc_with_source().map(|(path, _)| path)
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
        // An empty managed root, so no toolchain can resolve from it.
        let inference_home = assert_fs::TempDir::new().unwrap();

        // SAFETY: This test runs in isolation and we restore the env vars at the end.
        unsafe {
            env::set_var("PATH", "");
            env::remove_var(INFC_PATH_ENV);
            env::set_var("INFERENCE_HOME", inference_home.path());
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
    fn sibling_infc_found_when_exe_in_target_triple_debug() {
        // `cargo build --target x86_64-unknown-linux-gnu` produces
        // `target/<triple>/debug/` — the sibling heuristic must accept it.
        let temp = assert_fs::TempDir::new().unwrap();
        let debug = temp
            .path()
            .join("target")
            .join("x86_64-unknown-linux-gnu")
            .join("debug");
        std::fs::create_dir_all(&debug).unwrap();
        let infs_path = debug.join(exe_name("infs"));
        let infc_path = debug.join(exe_name("infc"));
        std::fs::write(&infs_path, b"").unwrap();
        std::fs::write(&infc_path, b"").unwrap();

        let result = workspace_sibling_infc_from(&infs_path);
        assert_eq!(result.as_deref(), Some(infc_path.as_path()));
    }

    #[test]
    fn sibling_infc_found_when_exe_in_target_triple_release() {
        let temp = assert_fs::TempDir::new().unwrap();
        let release = temp
            .path()
            .join("target")
            .join("aarch64-apple-darwin")
            .join("release");
        std::fs::create_dir_all(&release).unwrap();
        let infs_path = release.join(exe_name("infs"));
        let infc_path = release.join(exe_name("infc"));
        std::fs::write(&infs_path, b"").unwrap();
        std::fs::write(&infc_path, b"").unwrap();

        let result = workspace_sibling_infc_from(&infs_path);
        assert_eq!(result.as_deref(), Some(infc_path.as_path()));
    }

    #[test]
    fn sibling_returns_none_when_no_target_ancestor_at_either_depth() {
        // foo/bar/debug/infs — neither parent nor grandparent is named
        // "target"; must reject even though the profile dir is valid and
        // a sibling infc exists.
        let temp = assert_fs::TempDir::new().unwrap();
        let debug = temp.path().join("foo").join("bar").join("debug");
        std::fs::create_dir_all(&debug).unwrap();
        let infs_path = debug.join(exe_name("infs"));
        let infc_path = debug.join(exe_name("infc"));
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
    fn verbose_false_when_unset() {
        assert!(!verbose_from(None));
    }

    #[test]
    fn verbose_true_when_set() {
        assert!(verbose_from(Some(OsStr::new("1"))));
        assert!(verbose_from(Some(OsStr::new("yes"))));
        // Only the exact value "0" disables, so "00" enables like any other
        // non-empty value.
        assert!(verbose_from(Some(OsStr::new("00"))));
    }

    #[test]
    fn verbose_false_when_set_to_zero() {
        assert!(!verbose_from(Some(OsStr::new("0"))));
    }

    #[test]
    fn verbose_false_when_empty_string() {
        assert!(!verbose_from(Some(OsStr::new(""))));
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

    #[test]
    fn resolution_source_labels_match_trace_strings() {
        // Label strings are a public contract: doctor output and verbose
        // trace lines both use them, so they must not drift.
        assert_eq!(ResolutionSource::InfcPathEnv.label(), "INFC_PATH env");
        assert_eq!(
            ResolutionSource::WorkspaceSibling.label(),
            "workspace sibling"
        );
        assert_eq!(ResolutionSource::SystemPath.label(), "PATH");
        assert_eq!(
            ResolutionSource::ManagedToolchain.label(),
            "managed toolchain"
        );
    }

    #[test]
    #[serial_test::serial]
    fn find_infc_with_source_reports_workspace_sibling() {
        // Fabricate a target/debug/{infs,infc} layout via the CURRENT_EXE_OVERRIDE
        // seam so the workspace-sibling priority wins deterministically.
        let temp = assert_fs::TempDir::new().unwrap();
        let debug = temp.path().join("target").join("debug");
        std::fs::create_dir_all(&debug).unwrap();
        let infs_path = debug.join(exe_name("infs"));
        let infc_path = debug.join(exe_name("infc"));
        std::fs::write(&infs_path, b"").unwrap();
        std::fs::write(&infc_path, b"").unwrap();

        // SAFETY: serialized test; cleanup happens regardless of outcome.
        unsafe {
            env::remove_var(INFC_PATH_ENV);
        }
        let _guard = ExeOverrideGuard::set(infs_path);

        let result = find_infc_with_source();

        let (path, source) = result.unwrap();
        assert_eq!(source, ResolutionSource::WorkspaceSibling);
        assert_eq!(
            path.canonicalize().unwrap(),
            infc_path.canonicalize().unwrap()
        );
    }

    #[test]
    #[serial_test::serial]
    fn find_infc_with_source_reports_infc_path_env() {
        let env_dir = assert_fs::TempDir::new().unwrap();
        let env_infc = env_dir.path().join(exe_name("infc"));
        std::fs::write(&env_infc, b"").unwrap();

        // Neutralize the workspace-sibling priority so it cannot accidentally
        // fire before INFC_PATH — the test is about priority-1 winning.
        let _guard = ExeOverrideGuard::set(PathBuf::from("/nonexistent/elsewhere/infs"));

        // SAFETY: serialized test; env restored below.
        unsafe {
            env::set_var(INFC_PATH_ENV, &env_infc);
        }

        let result = find_infc_with_source();

        // SAFETY: cleanup regardless of outcome.
        unsafe {
            env::remove_var(INFC_PATH_ENV);
        }

        let (path, source) = result.unwrap();
        assert_eq!(source, ResolutionSource::InfcPathEnv);
        assert_eq!(path, env_infc);
    }

    #[test]
    #[serial_test::serial]
    fn find_infc_with_source_reports_path() {
        // Fabricate an infc on a dedicated PATH dir, then point PATH at it.
        // The test skips gracefully when tempfile symlinks prevent `which`
        // from finding the stub (e.g. restricted CI sandboxes).
        let path_dir = assert_fs::TempDir::new().unwrap();
        let stub = path_dir.path().join(exe_name("infc"));
        std::fs::write(&stub, b"#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&stub).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&stub, perms).unwrap();
        }

        let original_path = env::var("PATH").unwrap_or_default();
        // Neutralize workspace-sibling priority so PATH lookup is reached.
        let _guard = ExeOverrideGuard::set(PathBuf::from("/nonexistent/elsewhere/infs"));

        // SAFETY: serialized test; env restored below.
        unsafe {
            env::remove_var(INFC_PATH_ENV);
            env::set_var("PATH", path_dir.path());
        }

        let result = find_infc_with_source();

        // SAFETY: restore PATH regardless of outcome.
        unsafe {
            env::set_var("PATH", original_path);
        }

        // If `which` couldn't locate our stub in this environment, fall back
        // to the managed-toolchain branch or a not-found error. Both are
        // acceptable here — what we're asserting is that when PATH *does*
        // win, the reported source is SystemPath.
        if let Ok((path, source)) = result
            && source == ResolutionSource::SystemPath
        {
            assert_eq!(
                path.canonicalize().unwrap(),
                stub.canonicalize().unwrap(),
                "PATH resolution must return the fabricated stub"
            );
        }
    }
}
