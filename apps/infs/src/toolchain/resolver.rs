//! Compiler binary resolution for the infs CLI.
//!
//! This module provides functionality for locating the `infc` compiler binary
//! across different installation contexts. The search order prioritizes:
//!
//! 1. Explicit override via `INFC_PATH` environment variable
//! 2. Sibling `infc[.exe]` in the directory holding the running `infs`
//! 3. System PATH via `which::which("infc")`
//! 4. Managed toolchain at `~/.inference/toolchains/VERSION/infc`
//!
//! ## Environment Variables
//!
//! - `INFC_PATH`: Explicit path to the infc binary (highest priority)
//! - `INFS_VERBOSE`: When set to a non-empty, non-"0" value, emits resolution
//!   trace lines to stderr describing which priority resolved `infc`, and
//!   reports when the sibling priority declined
//!
//! ## Example
//!
//! ```rust,ignore
//! use crate::toolchain::resolver::find_infc_with_source;
//!
//! let (infc_path, source) = find_infc_with_source()?;
//! println!("Using infc at: {} (via {})", infc_path.display(), source.label());
//! ```

use anyhow::{Result, bail};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::toolchain::paths::ToolchainPaths;

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
    /// Resolved via an `infc` sitting next to the running `infs` (priority 2).
    ExecutableSibling,
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
            Self::ExecutableSibling => "sibling of infs",
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

/// Priority-2 (relative to PATH): the `infc[.exe]` sitting next to the running
/// `infs`, if one is there. Falls through silently on any error, except under
/// `INFS_VERBOSE`, which reports the decline so the PATH fallback is visible.
///
/// Canonicalizes the current exe first for the common case where cargo
/// invokes via a symlink, but falls back to the raw path when
/// `canonicalize()` fails (broken symlinks, restricted ACLs, some
/// container `/proc/self/exe` setups).
pub(crate) fn sibling_infc() -> Option<PathBuf> {
    let raw = current_exe_for_resolver().ok()?;
    let canonical = raw.canonicalize().ok();
    if canonical.is_none() && verbose() {
        eprintln!(
            "infs: canonicalize failed for {}; trying raw path",
            raw.display()
        );
    }
    let exe = canonical.as_deref().unwrap_or(&raw);
    let found = sibling_infc_from(exe);
    if found.is_none() && verbose() {
        // A path with no directory component is reported as itself; naming
        // the useless empty string would be worse than naming the path the
        // probe was handed.
        let looked_in = exe_dir(exe).unwrap_or(exe);
        eprintln!(
            "infs: no infc beside {}; falling back to PATH",
            looked_in.display()
        );
    }
    found
}

/// The directory holding `exe`, or `None` when the path names no directory.
///
/// A path with no directory component has `Some("")` as its parent, not
/// `None`. Refusing it is what stops a candidate from degrading to a bare
/// `infc` probed against the current working directory — a location that has
/// nothing to do with where `infs` lives.
fn exe_dir(exe: &Path) -> Option<&Path> {
    exe.parent().filter(|dir| !dir.as_os_str().is_empty())
}

/// Looks for `infc` in the directory that holds `exe`, and nowhere else.
///
/// A compiler driver and its companion tools ship as one unit, so the copy
/// that belongs to this `infs` is the copy installed alongside it — the same
/// rule clang and rustc use to find their own companions. Naming neither the
/// directory nor the driver's own filename is the point: a cargo target
/// directory redirected by `CARGO_TARGET_DIR` or `--target-dir`, a custom
/// cargo profile, a release tarball unpacked anywhere, and a driver renamed
/// for local development all keep the pairing that matters, which is
/// adjacency.
fn sibling_infc_from(exe: &Path) -> Option<PathBuf> {
    let candidate = exe_dir(exe)?.join(format!("infc{}", std::env::consts::EXE_SUFFIX));
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
    let infc_name = format!("infc{}", std::env::consts::EXE_SUFFIX);
    let infc_path = paths.binary_path(&version, &infc_name);
    infc_path.is_file().then_some(infc_path)
}

/// Locates the `infc` compiler binary and reports which priority fired.
///
/// Searches in the following priority order:
///
/// 1. **`INFC_PATH` environment variable** - Explicit override for testing
///    or custom installations (hardest override)
/// 2. **Sibling of infs** - Prefer the `infc[.exe]` installed in the same
///    directory as the running `infs`, whatever that directory is; companion
///    tools ship together, so adjacency identifies the paired compiler
/// 3. **System PATH** - Uses `which::which("infc")` to find infc in PATH
/// 4. **Managed toolchain** - Looks in `~/.inference/toolchains/VERSION/infc`
///    using the default toolchain version if set
///
/// Fallthrough on any priority's failure is intentional; each priority is
/// best-effort. Set `INFS_VERBOSE=1` to trace which priority resolved
/// `infc` on stderr.
///
/// Every caller wants the source alongside the path: build and run feed it
/// to the compatibility handshake, which holds the sibling tier — and only
/// the sibling tier — to its claim that the two binaries are a pair. Doctor
/// reports it so a user can see *why* one binary was selected over another.
///
/// # Errors
///
/// Returns an error if:
/// - `INFC_PATH` is set but the path does not exist
/// - No infc binary could be found in any location
///
/// The error message provides helpful guidance on how to install infc.
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

    // Priority 2: infc sitting next to the running infs
    if let Some(path) = sibling_infc() {
        trace_resolved(ResolutionSource::ExecutableSibling, &path);
        return Ok((path, ResolutionSource::ExecutableSibling));
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
        format!("{name}{}", std::env::consts::EXE_SUFFIX)
    }

    /// Creates `dir` and writes an empty file per entry in `names`, returning
    /// the directory.
    ///
    /// Callers root `dir` in a fresh `TempDir`, keeping every layout off any
    /// real system path whose contents a test could neither predict nor own.
    fn populate(dir: PathBuf, names: &[&str]) -> PathBuf {
        std::fs::create_dir_all(&dir).unwrap();
        for name in names {
            std::fs::write(dir.join(name), b"").unwrap();
        }
        dir
    }

    /// Asserts that a directory holding `driver` and an `infc` resolves that
    /// `infc` when probed with the driver's path.
    ///
    /// The layout matrix varies only the directory shape and the driver's
    /// name, so naming the layout is the whole test and the assertion never
    /// differs.
    fn assert_driver_finds_sibling(dir: PathBuf, driver: &str) {
        let driver_name = exe_name(driver);
        let infc_name = exe_name("infc");
        let dir = populate(dir, &[&driver_name, &infc_name]);

        let result = sibling_infc_from(&dir.join(driver_name));
        assert_eq!(result.as_deref(), Some(dir.join(infc_name).as_path()));
    }

    #[test]
    #[serial_test::serial]
    fn infc_path_env_nonexistent_returns_error() {
        let path = "/nonexistent/path/to/infc";

        // SAFETY: This test runs in isolation and we restore the env var at the end.
        unsafe {
            env::set_var(INFC_PATH_ENV, path);
        }

        let result = find_infc_with_source().map(|(path, _)| path);
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

        // Also suppress the sibling priority so the error path fires
        // regardless of how the test binary itself is laid out.
        let _guard = ExeOverrideGuard::set(PathBuf::from("/nonexistent/elsewhere/infs"));

        let result = find_infc_with_source().map(|(path, _)| path);

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
    fn sibling_infc_found_in_cargo_default_layout() {
        let temp = assert_fs::TempDir::new().unwrap();
        assert_driver_finds_sibling(temp.path().join("target").join("debug"), "infs");
    }

    #[test]
    fn sibling_infc_found_in_cargo_release_layout() {
        let temp = assert_fs::TempDir::new().unwrap();
        assert_driver_finds_sibling(temp.path().join("target").join("release"), "infs");
    }

    #[test]
    fn sibling_infc_found_when_exe_in_target_triple_debug() {
        // `cargo build --target x86_64-unknown-linux-gnu` produces
        // `target/<triple>/debug/`; cross-compiled runs must keep resolving.
        let temp = assert_fs::TempDir::new().unwrap();
        let dir = temp
            .path()
            .join("target")
            .join("x86_64-unknown-linux-gnu")
            .join("debug");
        assert_driver_finds_sibling(dir, "infs");
    }

    #[test]
    fn sibling_infc_found_when_exe_in_target_triple_release() {
        let temp = assert_fs::TempDir::new().unwrap();
        let dir = temp
            .path()
            .join("target")
            .join("aarch64-apple-darwin")
            .join("release");
        assert_driver_finds_sibling(dir, "infs");
    }

    #[test]
    fn sibling_infc_found_when_target_dir_is_redirected() {
        // `CARGO_TARGET_DIR=/tmp/out` puts the build in `out/debug/`, with no
        // directory named `target` anywhere on the path.
        let temp = assert_fs::TempDir::new().unwrap();
        assert_driver_finds_sibling(temp.path().join("out").join("debug"), "infs");
    }

    #[test]
    fn sibling_infc_found_when_target_dir_is_named_build() {
        // `cargo build --target-dir build` produces `build/debug/`.
        let temp = assert_fs::TempDir::new().unwrap();
        assert_driver_finds_sibling(temp.path().join("build").join("debug"), "infs");
    }

    #[test]
    fn sibling_infc_found_under_custom_cargo_profile() {
        // A profile declared in `[profile.dist]` builds into `target/dist/`,
        // which is neither `debug` nor `release`.
        let temp = assert_fs::TempDir::new().unwrap();
        assert_driver_finds_sibling(temp.path().join("target").join("dist"), "infs");
    }

    #[test]
    fn sibling_infc_found_with_no_recognizable_ancestor() {
        // Nothing about `foo/bar/debug/` says "cargo output"; adjacency is
        // the whole rule, so it resolves like any other directory.
        let temp = assert_fs::TempDir::new().unwrap();
        let dir = temp.path().join("foo").join("bar").join("debug");
        assert_driver_finds_sibling(dir, "infs");
    }

    #[test]
    fn sibling_infc_found_in_flat_install_layout() {
        // An unpacked release tarball holds both binaries in one directory
        // with no profile dir at all. Installed layouts resolve too.
        let temp = assert_fs::TempDir::new().unwrap();
        assert_driver_finds_sibling(temp.path().join("inference").join("bin"), "infs");
    }

    #[test]
    fn sibling_infc_found_when_driver_is_renamed() {
        // The driver's own filename is never inspected, so a locally renamed
        // `infs-dev` still finds the `infc` shipped beside it.
        let temp = assert_fs::TempDir::new().unwrap();
        assert_driver_finds_sibling(temp.path().join("bin"), "infs-dev");
    }

    #[test]
    fn sibling_returns_none_when_only_infs_present() {
        let temp = assert_fs::TempDir::new().unwrap();
        let dir = temp.path().join("target").join("debug");
        let dir = populate(dir, &[&exe_name("infs")]);
        // Deliberately do not create infc.

        let result = sibling_infc_from(&dir.join(exe_name("infs")));
        assert_eq!(result, None);
    }

    #[test]
    fn sibling_returns_none_when_infc_is_a_directory() {
        // A directory that happens to be named `infc` is not a compiler.
        let temp = assert_fs::TempDir::new().unwrap();
        let dir = populate(temp.path().join("bin"), &[&exe_name("infs")]);
        std::fs::create_dir(dir.join(exe_name("infc"))).unwrap();

        let result = sibling_infc_from(&dir.join(exe_name("infs")));
        assert_eq!(result, None);
    }

    #[test]
    fn sibling_returns_none_for_bare_relative_exe_path() {
        // Pins the empty-parent refusal documented on `exe_dir`.
        assert_eq!(sibling_infc_from(Path::new("infs")), None);
    }

    #[test]
    #[serial_test::serial]
    fn infc_path_env_overrides_sibling() {
        // Build a layout the sibling priority would accept...
        let workspace = assert_fs::TempDir::new().unwrap();
        let debug = populate(
            workspace.path().join("target").join("debug"),
            &[&exe_name("infs"), &exe_name("infc")],
        );
        let infs_path = debug.join(exe_name("infs"));
        let sibling_infc = debug.join(exe_name("infc"));

        // ...then also set INFC_PATH to a distinct file.
        let env_dir = assert_fs::TempDir::new().unwrap();
        let env_infc = env_dir.path().join(exe_name("infc"));
        std::fs::write(&env_infc, b"").unwrap();

        let _guard = ExeOverrideGuard::set(infs_path);

        // SAFETY: serialized test; we restore immediately after.
        unsafe {
            env::set_var(INFC_PATH_ENV, &env_infc);
        }

        let result = find_infc_with_source().map(|(path, _)| path);

        // SAFETY: cleanup regardless of assertion outcome.
        unsafe {
            env::remove_var(INFC_PATH_ENV);
        }

        let resolved = result.unwrap();
        assert_eq!(
            resolved, env_infc,
            "INFC_PATH must outrank the sibling infc"
        );
        assert_ne!(
            resolved, sibling_infc,
            "the sibling infc must not win when INFC_PATH is set"
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
        // Inject a path that doesn't exist on disk. canonicalize() will fail,
        // and no infc sits beside the raw path either, so the expected
        // outcome is None.
        let _guard = ExeOverrideGuard::set(PathBuf::from("/nonexistent/elsewhere/infs"));
        assert_eq!(sibling_infc(), None);
    }

    #[test]
    fn canonicalize_fails_and_raw_path_has_no_sibling() {
        // A raw path pointing nowhere: canonicalize() fails, the raw path is
        // used instead, and its directory holds no infc. Exercises the
        // raw-path fallback branch end-to-end.
        let fabricated = PathBuf::from("/nonexistent")
            .join("target")
            .join("debug")
            .join(exe_name("infs"));
        let _guard = ExeOverrideGuard::set(fabricated);
        assert_eq!(sibling_infc(), None);
    }

    #[test]
    fn resolution_source_labels_match_trace_strings() {
        // Label strings are a public contract: doctor output and verbose
        // trace lines both use them, so they must not drift.
        assert_eq!(ResolutionSource::InfcPathEnv.label(), "INFC_PATH env");
        assert_eq!(
            ResolutionSource::ExecutableSibling.label(),
            "sibling of infs"
        );
        assert_eq!(ResolutionSource::SystemPath.label(), "PATH");
        assert_eq!(
            ResolutionSource::ManagedToolchain.label(),
            "managed toolchain"
        );
    }

    #[test]
    #[serial_test::serial]
    fn find_infc_with_source_reports_executable_sibling() {
        // Fabricate an {infs,infc} pair via the CURRENT_EXE_OVERRIDE seam so
        // the sibling priority wins deterministically.
        let temp = assert_fs::TempDir::new().unwrap();
        let dir = populate(
            temp.path().join("target").join("debug"),
            &[&exe_name("infs"), &exe_name("infc")],
        );

        // SAFETY: serialized test; cleanup happens regardless of outcome.
        unsafe {
            env::remove_var(INFC_PATH_ENV);
        }
        let _guard = ExeOverrideGuard::set(dir.join(exe_name("infs")));

        let result = find_infc_with_source();

        let (path, source) = result.unwrap();
        assert_eq!(source, ResolutionSource::ExecutableSibling);
        assert_eq!(
            path.canonicalize().unwrap(),
            dir.join(exe_name("infc")).canonicalize().unwrap()
        );
    }

    #[test]
    #[serial_test::serial]
    fn find_infc_with_source_reports_infc_path_env() {
        let env_dir = assert_fs::TempDir::new().unwrap();
        let env_infc = env_dir.path().join(exe_name("infc"));
        std::fs::write(&env_infc, b"").unwrap();

        // Neutralize the sibling priority so it cannot accidentally fire
        // before INFC_PATH — the test is about priority-1 winning.
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
        // Neutralize the sibling priority so PATH lookup is reached.
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

    #[test]
    #[serial_test::serial]
    fn sibling_overrides_path() {
        // Two different compilers, one beside `infs` and one on PATH. The
        // whole point of the sibling priority is that the neighbour wins, so
        // this asserts unconditionally: if `which` cannot see the stub the
        // sibling still wins, and if it can, the sibling must beat it.
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

        let workspace = assert_fs::TempDir::new().unwrap();
        let dir = populate(
            workspace.path().join("target").join("debug"),
            &[&exe_name("infs"), &exe_name("infc")],
        );

        let original_path = env::var("PATH").unwrap_or_default();
        let _guard = ExeOverrideGuard::set(dir.join(exe_name("infs")));

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

        let (path, source) = result.unwrap();
        assert_eq!(source, ResolutionSource::ExecutableSibling);
        let resolved = path.canonicalize().unwrap();
        assert_eq!(
            resolved,
            dir.join(exe_name("infc")).canonicalize().unwrap(),
            "the sibling infc must outrank the one on PATH"
        );
        assert_ne!(
            resolved,
            stub.canonicalize().unwrap(),
            "the PATH stub must not win when a sibling exists"
        );
    }
}
