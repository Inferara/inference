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
    checks.extend(check_optional_managed_binaries());
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
    let binary_with_ext = format!("infc{}", std::env::consts::EXE_SUFFIX);

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
/// critical for developers whose machine has both an `infc` beside their
/// `infs` and a managed toolchain installed.
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

/// Warns when an `infc` sits beside the running `infs` *and* a managed
/// toolchain `infc` exists — the exact ambiguity the priority-2 sibling
/// rule resolves silently. Surfacing it here gives developers a clear
/// knob: either intentionally run the neighbouring build, or remove the
/// stale managed install.
///
/// Returns `None` when no ambiguity exists, so the check is elided from
/// output in the common single-source case.
#[must_use]
pub fn check_resolution_ambiguity() -> Option<DoctorCheck> {
    let sibling = resolver::sibling_infc()?;
    let managed = resolver::managed_toolchain_infc()?;
    resolution_ambiguity_between(&sibling, &managed)
}

/// The decision behind [`check_resolution_ambiguity`], separated from the
/// process state it reads so both outcomes are unit-testable.
///
/// A managed install puts `infs` next to the managed `infc` link in
/// `<INFERENCE_HOME>/bin`, so both priorities find the same compiler by two
/// different routes. That is not an ambiguity, and warning about it would
/// send every ordinary user chasing a conflict between one file and itself.
/// Canonicalizing collapses the symlink hop that makes the two spellings
/// differ; when a path cannot be canonicalized it does not exist as named,
/// and the literal paths are all there is to compare.
fn resolution_ambiguity_between(sibling: &Path, managed: &Path) -> Option<DoctorCheck> {
    let same_file = match (sibling.canonicalize(), managed.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => sibling == managed,
    };
    if same_file {
        return None;
    }
    Some(DoctorCheck::warning(
        "Resolution ambiguity",
        format!(
            "both a sibling of infs ({}) and a managed toolchain ({}) exist; \
             the sibling wins. Remove the stale managed install or \
             set INFC_PATH to pin your choice.",
            sibling.display(),
            managed.display()
        ),
    ))
}

/// Checks every optional managed binary (currently just `inference-lsp`)
/// listed in [`ToolchainPaths::OPTIONAL_MANAGED_BINARIES`].
///
/// Iterating the list rather than hardcoding a single name means a future
/// optional binary gains doctor coverage automatically. One [`DoctorCheck`]
/// is produced per binary, in declaration order, so the VS Code extension's
/// append-only line-parsing contract stays stable.
///
/// The check mirrors how the editor actually resolves the binary. The VS Code
/// extension looks only at `<INFERENCE_HOME>/bin/<binary>` (the managed
/// symlink) and then `PATH`; it never reads the toolchain directory directly.
/// So verifying that the toolchain *bundles* the binary is not enough — the
/// `bin/` symlink must also exist and resolve.
///
/// Statuses, chosen so `infs doctor` still exits zero when an optional binary
/// is simply absent:
/// - **OK** — the default toolchain bundles the binary and the `bin/` symlink
///   resolves; the linked path is reported, with a separate PATH copy appended
///   when one exists outside the managed `bin/` directory.
/// - **OK** — no default toolchain is set at all (deferring to
///   [`check_default_toolchain`], which owns that diagnosis).
/// - **Warning** — the toolchain bundles the binary but the `bin/` link is
///   missing or broken, so the editor cannot find it; the message names
///   `infs default <version>` as the repair.
/// - **Warning** — a default toolchain is set but predates the bundling; the
///   message hints at upgrading and notes any PATH fallback.
#[must_use]
pub fn check_optional_managed_binaries() -> Vec<DoctorCheck> {
    let ext = std::env::consts::EXE_SUFFIX;

    let Ok(paths) = ToolchainPaths::new() else {
        return ToolchainPaths::OPTIONAL_MANAGED_BINARIES
            .iter()
            .map(|name| DoctorCheck::error(*name, "Cannot determine toolchain paths"))
            .collect();
    };

    ToolchainPaths::OPTIONAL_MANAGED_BINARIES
        .iter()
        .map(|name| {
            let binary_with_ext = format!("{name}{ext}");
            let path_hit = which::which(&binary_with_ext).ok();
            optional_binary_check(name, &paths, &binary_with_ext, path_hit.as_deref())
        })
        .collect()
}

/// Returns `true` when `hit` lives directly in the managed `bin/` directory,
/// i.e. it is `infs`'s own symlink rather than a genuinely separate copy.
///
/// The comparison is on the parent directory so the symlink target (which
/// points into `toolchains/`) is never followed. Paths are canonicalized when
/// both sides exist so that home-directory symlinks do not produce a spurious
/// mismatch; otherwise a plain comparison is used (the common case in tests,
/// where the directory need not exist on disk).
fn hit_is_managed_symlink(hit: &Path, managed_bin: &Path) -> bool {
    let Some(parent) = hit.parent() else {
        return false;
    };
    match (parent.canonicalize(), managed_bin.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => parent == managed_bin,
    }
}

/// Pure decision logic for [`check_optional_managed_binaries`], separated from
/// the environment reads (platform, PATH, toolchain root) so every branch is
/// unit-testable with a `ToolchainPaths` rooted at a temp directory.
///
/// `name` is the display name for the check (the binary base name without the
/// platform extension); `binary_with_ext` carries the extension used for the
/// on-disk lookups.
fn optional_binary_check(
    name: &str,
    paths: &ToolchainPaths,
    binary_with_ext: &str,
    path_hit: Option<&Path>,
) -> DoctorCheck {
    let default_version = match paths.get_default_version() {
        Ok(Some(v)) => v,
        Ok(None) => return DoctorCheck::ok(name, "No toolchain installed"),
        Err(e) => {
            return DoctorCheck::error(name, format!("Cannot read default version: {e}"));
        }
    };

    // A PATH hit inside the managed bin directory is infs's own symlink, which
    // the extension already prepends to PATH before invoking doctor — noting it
    // as a "separate copy" would be misleading. Only report a copy elsewhere.
    let external_hit = path_hit.filter(|hit| !hit_is_managed_symlink(hit, &paths.bin));
    let append_path_note = |message: &mut String, phrasing: &str| {
        if let Some(hit) = external_hit {
            let _ = write!(message, "; {phrasing} {}", hit.display());
        }
    };

    let binary_path = paths.binary_path(&default_version, binary_with_ext);
    if !binary_path.exists() {
        let mut message = format!(
            "toolchain {default_version} does not include {name}; \
             install a newer toolchain to add it"
        );
        append_path_note(&mut message, "a copy is available on PATH at");
        return DoctorCheck::warning(name, message);
    }

    // The toolchain bundles the binary, but the editor resolves it through the
    // bin/ symlink (or PATH) — never the toolchain directory. Verify the link
    // exists and resolves; exists() is false for a dangling symlink, while
    // symlink_metadata() succeeds even when the target is missing.
    let symlink_path = paths.symlink_path(binary_with_ext);
    let link_present = symlink_path.symlink_metadata().is_ok();
    let link_resolves = symlink_path.exists();

    if link_resolves {
        let mut message = format!("Linked at {}", symlink_path.display());
        append_path_note(&mut message, "also on PATH at");
        return DoctorCheck::ok(name, message);
    }

    let broken_or_missing = if link_present {
        format!("the bin/ link at {} is broken", symlink_path.display())
    } else {
        format!("it is not linked into bin/ at {}", symlink_path.display())
    };
    let mut message = format!(
        "toolchain {default_version} bundles {name} but {broken_or_missing}; \
         run 'infs default {default_version}' to repair it"
    );
    append_path_note(&mut message, "a copy is available on PATH at");
    DoctorCheck::warning(name, message)
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
        // Fixed checks: infs, platform, toolchain dir, default toolchain,
        // infc, resolved infc, wasm-opt. Then one check per optional managed
        // binary. The ambiguity check is conditional (0 or 1).
        let base = 7 + ToolchainPaths::OPTIONAL_MANAGED_BINARIES.len();
        assert!(
            checks.len() == base || checks.len() == base + 1,
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
    #[serial_test::serial]
    fn check_infc_names_the_platform_binary_found_on_path() {
        // A single `infc` on PATH is the one branch whose message quotes the
        // composed binary name, so it pins that the extension comes from the
        // build target rather than from a host allow-list.
        let path_dir = assert_fs::TempDir::new().unwrap();
        let expected_name = format!("infc{}", std::env::consts::EXE_SUFFIX);
        let stub = path_dir.path().join(&expected_name);
        std::fs::write(&stub, b"#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let original_path = std::env::var("PATH").unwrap_or_default();
        // SAFETY: serialized; restored below before assertions.
        unsafe {
            std::env::set_var("PATH", path_dir.path());
        }

        let check = check_infc();

        // SAFETY: restore PATH before assertions so a panic cannot leak it.
        unsafe {
            std::env::set_var("PATH", original_path);
        }

        assert_eq!(check.status, DoctorCheckStatus::Ok);
        assert_eq!(check.message, format!("Found {expected_name} in PATH"));
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
        let (_temp, paths) = fresh_paths();
        std::fs::create_dir_all(&paths.toolchains).unwrap();

        let msg = no_default_toolchain_message(&paths);
        assert!(msg.contains("infs install"));
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
    fn resolution_ambiguity_reported_for_two_distinct_compilers() {
        let temp = assert_fs::TempDir::new().unwrap();
        let sibling = temp.path().join("sibling_infc");
        let managed = temp.path().join("managed_infc");
        std::fs::write(&sibling, b"infc").unwrap();
        std::fs::write(&managed, b"infc").unwrap();

        let check = resolution_ambiguity_between(&sibling, &managed).expect("two distinct files");
        assert_eq!(check.status, DoctorCheckStatus::Warning);
        assert!(check.message.contains(&sibling.display().to_string()));
        assert!(check.message.contains(&managed.display().to_string()));
        assert!(check.message.contains("INFC_PATH"));
    }

    #[cfg(unix)]
    #[test]
    fn resolution_ambiguity_silent_when_symlink_resolves_to_same_file() {
        // The shape a plain managed install produces: `infs` sits in bin/
        // beside the managed `infc` symlink, so the sibling and managed
        // priorities name one compiler by two routes.
        let temp = assert_fs::TempDir::new().unwrap();
        let real = temp.path().join("toolchain_infc");
        std::fs::write(&real, b"infc").unwrap();
        let link = temp.path().join("bin_infc");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        assert_ne!(link, real, "the two paths must differ literally");
        assert!(resolution_ambiguity_between(&link, &real).is_none());
    }

    #[test]
    fn resolution_ambiguity_silent_for_identical_paths() {
        let temp = assert_fs::TempDir::new().unwrap();
        let infc = temp.path().join("infc");
        std::fs::write(&infc, b"infc").unwrap();

        assert!(resolution_ambiguity_between(&infc, &infc).is_none());
    }

    #[test]
    fn resolution_ambiguity_falls_back_to_raw_compare_when_uncanonicalizable() {
        // Both paths name children of an empty directory, so canonicalize()
        // fails on each and only the literal spellings can be compared.
        let temp = assert_fs::TempDir::new().unwrap();
        let missing = temp.path().join("bin").join("infc");
        let other = temp.path().join("toolchains").join("0.1.0").join("infc");

        assert!(resolution_ambiguity_between(&missing, &missing).is_none());
        let check = resolution_ambiguity_between(&missing, &other).expect("distinct raw paths");
        assert_eq!(check.status, DoctorCheckStatus::Warning);
    }

    #[test]
    fn no_default_toolchain_message_with_installed_versions() {
        let (_temp, paths) = fresh_paths();
        std::fs::create_dir_all(paths.toolchain_dir("0.1.0")).unwrap();

        let msg = no_default_toolchain_message(&paths);
        assert!(msg.contains("infs default"));
        assert!(msg.contains("0.1.0"));
    }

    /// The bundled `inference-lsp` binary name for the running platform.
    fn lsp_binary_name() -> String {
        format!("inference-lsp{}", std::env::consts::EXE_SUFFIX)
    }

    /// Builds a `ToolchainPaths` rooted at a fresh, empty temporary directory.
    ///
    /// The returned `TempDir` owns that directory and deletes it on drop, so
    /// callers must keep it bound for the whole test. Every call yields a
    /// distinct path, so neither parallel test threads nor concurrent test
    /// processes can observe each other's files.
    fn fresh_paths() -> (assert_fs::TempDir, ToolchainPaths) {
        let temp = assert_fs::TempDir::new().unwrap();
        let paths = ToolchainPaths::with_root(temp.path().to_path_buf());
        (temp, paths)
    }

    /// Installs the optional binary into `toolchains/<version>/` and sets it as
    /// the default, mimicking a toolchain archive that bundles the server.
    fn install_bundled(paths: &ToolchainPaths, version: &str, binary: &str) {
        std::fs::create_dir_all(paths.toolchain_dir(version)).unwrap();
        std::fs::write(paths.binary_path(version, binary), b"lsp").unwrap();
        paths.set_default_version(version).unwrap();
    }

    #[test]
    fn check_optional_managed_binaries_covers_every_entry() {
        let checks = check_optional_managed_binaries();
        assert_eq!(
            checks.len(),
            ToolchainPaths::OPTIONAL_MANAGED_BINARIES.len(),
            "one check must be produced per optional managed binary"
        );
        for (check, expected) in checks
            .iter()
            .zip(ToolchainPaths::OPTIONAL_MANAGED_BINARIES)
        {
            assert_eq!(&check.name, expected);
            assert!(!check.message.is_empty());
            assert!(matches!(
                check.status,
                DoctorCheckStatus::Ok | DoctorCheckStatus::Warning | DoctorCheckStatus::Error
            ));
            // The check derives its extension from the build target, so no
            // host can turn every optional binary into a detection failure.
            assert!(
                !check.message.contains("Cannot detect platform"),
                "platform detection must not gate this check: {}",
                check.message
            );
        }
    }

    #[test]
    fn optional_binary_check_ok_when_no_default_toolchain() {
        let (_temp, paths) = fresh_paths();

        let check = optional_binary_check("inference-lsp", &paths, &lsp_binary_name(), None);
        assert_eq!(check.status, DoctorCheckStatus::Ok);
        assert_eq!(check.message, "No toolchain installed");
    }

    #[cfg(unix)]
    #[test]
    fn optional_binary_check_ok_when_bundled_and_linked() {
        let (_temp, paths) = fresh_paths();
        let binary = lsp_binary_name();
        install_bundled(&paths, "0.2.0", &binary);

        std::fs::create_dir_all(&paths.bin).unwrap();
        std::os::unix::fs::symlink(
            paths.binary_path("0.2.0", &binary),
            paths.symlink_path(&binary),
        )
        .unwrap();

        let check = optional_binary_check("inference-lsp", &paths, &binary, None);
        assert_eq!(check.status, DoctorCheckStatus::Ok);
        assert!(check.message.starts_with("Linked at"));
        assert!(!check.message.contains("also on PATH"));
    }

    #[test]
    fn optional_binary_check_warns_when_bundled_but_link_missing() {
        let (_temp, paths) = fresh_paths();
        let binary = lsp_binary_name();
        install_bundled(&paths, "0.2.0", &binary);

        // Toolchain has the server, but bin/ was never linked (the exact
        // rollout state the issue describes).
        let check = optional_binary_check("inference-lsp", &paths, &binary, None);
        assert_eq!(check.status, DoctorCheckStatus::Warning);
        assert!(check.message.contains("not linked into bin/"));
        assert!(
            check.message.contains("infs default 0.2.0"),
            "remediation hint must name the healing command: {}",
            check.message
        );
    }

    #[cfg(unix)]
    #[test]
    fn optional_binary_check_warns_when_bundled_but_link_broken() {
        let (temp, paths) = fresh_paths();
        let binary = lsp_binary_name();
        install_bundled(&paths, "0.2.0", &binary);

        std::fs::create_dir_all(&paths.bin).unwrap();
        std::os::unix::fs::symlink(temp.path().join("gone_binary"), paths.symlink_path(&binary))
            .unwrap();

        let check = optional_binary_check("inference-lsp", &paths, &binary, None);
        assert_eq!(check.status, DoctorCheckStatus::Warning);
        assert!(check.message.contains("is broken"));
        assert!(
            check.message.contains("infs default 0.2.0"),
            "remediation hint must name the healing command: {}",
            check.message
        );
    }

    #[test]
    fn optional_binary_check_warns_when_toolchain_predates_bundling() {
        let (_temp, paths) = fresh_paths();
        std::fs::create_dir_all(paths.toolchain_dir("0.1.0")).unwrap();
        paths.set_default_version("0.1.0").unwrap();

        let check = optional_binary_check("inference-lsp", &paths, &lsp_binary_name(), None);
        assert_eq!(check.status, DoctorCheckStatus::Warning);
        assert!(check.message.contains("does not include inference-lsp"));
        assert!(check.message.contains("0.1.0"));
    }

    #[cfg(unix)]
    #[test]
    fn optional_binary_check_predates_bundling_ignores_symlink_state() {
        // A stale valid symlink from an earlier bundled toolchain must not
        // mask the fact that the *current* default lacks the binary.
        let (temp, paths) = fresh_paths();
        let binary = lsp_binary_name();
        std::fs::create_dir_all(paths.toolchain_dir("0.1.0")).unwrap();
        paths.set_default_version("0.1.0").unwrap();

        let stale_target = temp.path().join("stale_lsp");
        std::fs::write(&stale_target, b"stale").unwrap();
        std::fs::create_dir_all(&paths.bin).unwrap();
        std::os::unix::fs::symlink(&stale_target, paths.symlink_path(&binary)).unwrap();

        let check = optional_binary_check("inference-lsp", &paths, &binary, None);
        assert_eq!(check.status, DoctorCheckStatus::Warning);
        assert!(check.message.contains("does not include inference-lsp"));
    }

    #[test]
    fn optional_binary_check_notes_external_path_copy() {
        let (_temp, paths) = fresh_paths();
        std::fs::create_dir_all(paths.toolchain_dir("0.1.0")).unwrap();
        paths.set_default_version("0.1.0").unwrap();

        let hit = Path::new("/opt/tools/inference-lsp");
        let check = optional_binary_check("inference-lsp", &paths, &lsp_binary_name(), Some(hit));
        assert_eq!(check.status, DoctorCheckStatus::Warning);
        assert!(check.message.contains("a copy is available on PATH at /opt/tools/inference-lsp"));
    }

    #[test]
    fn optional_binary_check_excludes_managed_bin_from_path_note() {
        // The extension prepends <INFERENCE_HOME>/bin to PATH, so a PATH hit
        // that resolves to infs's own symlink is not a separate copy and must
        // not be reported as one.
        let (_temp, paths) = fresh_paths();
        let binary = lsp_binary_name();
        std::fs::create_dir_all(paths.toolchain_dir("0.1.0")).unwrap();
        paths.set_default_version("0.1.0").unwrap();

        let managed_hit = paths.symlink_path(&binary);
        let check =
            optional_binary_check("inference-lsp", &paths, &binary, Some(managed_hit.as_path()));
        assert_eq!(check.status, DoctorCheckStatus::Warning);
        assert!(
            !check.message.contains("on PATH"),
            "managed symlink must be excluded from the PATH-copy note: {}",
            check.message
        );
    }

    #[cfg(unix)]
    #[test]
    fn optional_binary_check_ok_notes_external_path_copy() {
        let (_temp, paths) = fresh_paths();
        let binary = lsp_binary_name();
        install_bundled(&paths, "0.2.0", &binary);

        std::fs::create_dir_all(&paths.bin).unwrap();
        std::os::unix::fs::symlink(
            paths.binary_path("0.2.0", &binary),
            paths.symlink_path(&binary),
        )
        .unwrap();

        let hit = Path::new("/usr/local/bin/inference-lsp");
        let check = optional_binary_check("inference-lsp", &paths, &binary, Some(hit));
        assert_eq!(check.status, DoctorCheckStatus::Ok);
        assert!(check.message.starts_with("Linked at"));
        assert!(check.message.contains("also on PATH at /usr/local/bin/inference-lsp"));
    }

    #[cfg(unix)]
    #[test]
    fn optional_binary_check_ok_excludes_managed_symlink_from_path_note() {
        // Healthy install: the PATH hit *is* the managed symlink. The OK line
        // must not imply a duplicate copy exists.
        let (_temp, paths) = fresh_paths();
        let binary = lsp_binary_name();
        install_bundled(&paths, "0.2.0", &binary);

        std::fs::create_dir_all(&paths.bin).unwrap();
        let managed = paths.symlink_path(&binary);
        std::os::unix::fs::symlink(paths.binary_path("0.2.0", &binary), &managed).unwrap();

        let check = optional_binary_check("inference-lsp", &paths, &binary, Some(managed.as_path()));
        assert_eq!(check.status, DoctorCheckStatus::Ok);
        assert!(
            !check.message.contains("also on PATH"),
            "managed symlink must be excluded from the PATH-copy note: {}",
            check.message
        );
    }
}
