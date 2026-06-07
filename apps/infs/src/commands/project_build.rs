//! Shared project-build helper for the infs CLI.
//!
//! Both `infs build` (project mode) and `infs run` (project mode) need to
//! perform the *same* project compilation: resolve the conventional
//! `src/main.inf` entry point, warn about other `src/*.inf` files, run the
//! `infc` compatibility handshake, and spawn `infc` with its working directory
//! set to the project root so `out/` lands at the root. This module owns that
//! shared logic so the two command modules do not duplicate it (and so `run`
//! inherits the handshake "for free" — issue #222, AD-7/AD-11).
//!
//! It lives under `commands/` rather than `project/` because it is
//! command-execution logic (subprocess spawning, exit-code propagation, the
//! ABI handshake), the same category as [`crate::commands::build`] and
//! [`crate::commands::run`]. The `project/` module is deliberately scoped to
//! filesystem walking and manifest parsing; placing subprocess-spawning code
//! there would blur that boundary.
//!
//! The compatibility handshake ([`check_compiler_compatibility`]) also lives
//! here: it is part of "running a project build", and keeping it beside the
//! single spawning site keeps the coupling tight.

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::commands::build::BuildMode;
use crate::errors::InfsError;
use crate::project::ProjectContext;
use crate::toolchain::find_infc;
use inference_compiler_interface::{COMPILER_ABI_MAJOR, COMPILER_ABI_MINOR};

/// Compiles the entry point of a discovered project (project mode).
///
/// Shared by `infs build` and `infs run`. Resolves the conventional
/// `src/main.inf` entry point, warns about any other `src/*.inf` files
/// (multi-file compilation is gated on #63), runs the `infc` compatibility
/// handshake, then spawns `infc` with its working directory set to the project
/// root so that `out/` lands at the root regardless of where the command was
/// invoked.
///
/// The entry-point source is passed *relative to the root* (`src/main.inf`),
/// matching the CWD-relativity contract between `infs` and `infc`: `infc`
/// resolves both its source argument and `out/` against the inherited CWD.
///
/// The forwarded flags (`generate_v_output`, `mode`) are passed explicitly
/// rather than as a `BuildArgs` so `run` need not depend on `build`'s argument
/// struct. Only what the caller explicitly passed is forwarded;
/// `infc::normalize_args` owns the `-v` ↔ `--mode proof` implication, so
/// mirroring it here would create a second source of truth that could drift.
///
/// ## Errors
///
/// Returns an error if:
/// - The entry point `<root>/src/main.inf` does not exist
/// - infc compiler cannot be found
/// - infc reports a *major* ABI version mismatch (hard error with remediation)
/// - infc exits with non-zero code (as `InfsError::ProcessExitCode`)
pub(crate) fn run_project_build(
    ctx: &ProjectContext,
    generate_v_output: bool,
    mode: Option<BuildMode>,
) -> Result<()> {
    if !ctx.entry_point.exists() {
        bail!(
            "Missing entry point: expected `{}`. Project mode compiles \
             `src/main.inf` by convention; create it, or pass a source file \
             path (`infs build path/to/file.inf`).",
            ctx.entry_point.display()
        );
    }

    warn_extra_src_files(ctx);

    let infc_path = find_infc()?;
    check_compiler_compatibility(&infc_path)?;

    let entry_relative = ProjectContext::entry_relative();

    let mut cmd = Command::new(&infc_path);
    cmd.current_dir(&ctx.root).arg(&entry_relative);

    if generate_v_output {
        cmd.arg("-v");
    }

    // Forward only what the caller explicitly passed (see module docs).
    if let Some(mode) = mode {
        cmd.arg("--mode").arg(mode_flag(mode));
    }

    let status = cmd
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to execute infc at {}", infc_path.display()))?;

    if status.success() {
        Ok(())
    } else {
        let code = status.code().unwrap_or(1);
        Err(InfsError::process_exit_code(code).into())
    }
}

/// Emits a stderr warning for each `src/*.inf` file other than `main.inf`.
///
/// Project mode compiles only `src/main.inf` until multi-file support lands
/// (#63). Silently dropping helper files would be a debugging footgun, so each
/// excluded file is named. A missing or unreadable `src/` directory is not an
/// error here — the missing-entry-point check in [`run_project_build`] already
/// reports the meaningful failure.
fn warn_extra_src_files(ctx: &ProjectContext) {
    let src_dir = ctx.root.join("src");
    let Ok(entries) = std::fs::read_dir(&src_dir) else {
        return;
    };

    let mut extras: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "inf")
                && path.file_name().is_some_and(|name| name != "main.inf")
            {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(String::from)
            } else {
                None
            }
        })
        .collect();
    extras.sort();

    for name in extras {
        eprintln!(
            "warning: `src/{name}` is not part of the build; project mode \
             compiles only `src/main.inf` (multi-file support is pending)."
        );
    }
}

/// Maps a [`BuildMode`] to the `infc --mode` flag value.
///
/// Shared by the project path and the single-file path in
/// [`crate::commands::build`].
pub(crate) fn mode_flag(mode: BuildMode) -> &'static str {
    match mode {
        BuildMode::Proof => "proof",
        BuildMode::Compile => "compile",
    }
}

/// Runs a compatibility handshake against the resolved `infc` binary.
///
/// Sequence:
/// 1. Query `infc --commit-hash`. If it equals `INFS_GIT_COMMIT`, short-circuit —
///    the two binaries were built from the same source tree and the ABI is
///    guaranteed compatible.
/// 2. Otherwise query `infc --abi-version` and compare against the major/minor
///    constants from `inference-compiler-interface`. Major mismatch is a hard
///    error; minor mismatch is a warning; exact match is silent.
///
/// Old binaries that do not understand the flags (non-zero exit, empty output,
/// or the literal `unknown`) are treated as graceful skips — we neither warn
/// nor error on them. The L1/L2 resolver fixes remain the correctness
/// guarantee; this handshake is a safety net against residual drift.
pub(crate) fn check_compiler_compatibility(infc_path: &Path) -> Result<()> {
    // --commit-hash / --abi-version print and exit 0 immediately; no timeout needed.
    let local_commit = env!("INFS_GIT_COMMIT");
    let remote_commit = probe_flag(infc_path, "--commit-hash");

    if let Some(hash) = &remote_commit
        && hash == local_commit
    {
        return Ok(());
    }

    let Some(abi_raw) = probe_flag(infc_path, "--abi-version") else {
        return Ok(());
    };

    let Some((infc_major, infc_minor)) = parse_abi_version(&abi_raw) else {
        return Ok(());
    };

    let local_major = COMPILER_ABI_MAJOR;
    let local_minor = COMPILER_ABI_MINOR;

    if infc_major != local_major {
        bail!(
            "infs ABI {local_major}.{local_minor} but infc reported ABI \
             {infc_major}.{infc_minor}; rebuild the workspace or set \
             INFC_PATH to a matching binary."
        );
    }

    match infc_minor.cmp(&local_minor) {
        std::cmp::Ordering::Greater => {
            eprintln!(
                "warning: infc ABI {infc_major}.{infc_minor} is newer than \
                 infs ABI {local_major}.{local_minor}; infs may not \
                 recognize features emitted by infc."
            );
        }
        std::cmp::Ordering::Less => {
            eprintln!(
                "warning: infs ABI {local_major}.{local_minor} is newer \
                 than infc ABI {infc_major}.{infc_minor}; infs may request \
                 features infc does not provide."
            );
        }
        std::cmp::Ordering::Equal => {}
    }

    Ok(())
}

/// Runs `<infc_path> <flag>` with stdin/stderr suppressed and returns the
/// trimmed stdout on success. Returns `None` for any failure mode that an old
/// `infc` lacking the flag would produce: spawn error, non-zero exit, empty
/// stdout, or the literal `unknown`.
fn probe_flag(infc_path: &Path, flag: &str) -> Option<String> {
    let output = Command::new(infc_path)
        .arg(flag)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() || value == "unknown" {
        return None;
    }
    Some(value)
}

/// Parses a `"<major>.<minor>"` string into `(major, minor)`. Returns `None`
/// on any parse failure — callers treat that as "skip the ABI check".
fn parse_abi_version(raw: &str) -> Option<(u32, u32)> {
    let (major, minor) = raw.split_once('.')?;
    let major: u32 = major.parse().ok()?;
    let minor: u32 = minor.parse().ok()?;
    Some((major, minor))
}

#[cfg(test)]
mod project_tests {
    use super::*;
    use crate::project::manifest::InferenceToml;

    /// `run_project_build` must fail with a remediation error before doing any
    /// compiler lookup when `src/main.inf` is absent. This is platform
    /// independent — it errors before spawning `infc`.
    #[test]
    fn run_project_build_errors_when_entry_missing() {
        let dir = assert_fs::TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        // Manifest present, but no src/main.inf.
        let ctx = ProjectContext {
            root: root.clone(),
            manifest: InferenceToml::new("demo"),
            entry_point: root.join("src").join("main.inf"),
        };

        let err = run_project_build(&ctx, false, None).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("Missing entry point") && msg.contains("main.inf"),
            "expected missing-entry remediation, got: {msg}"
        );
    }

    /// The entry point is resolved as `<root>/src/main.inf` using path joins,
    /// so the resolved path always ends in the platform-correct components.
    #[test]
    fn entry_point_resolves_to_src_main_inf() {
        let dir = assert_fs::TempDir::new().unwrap();
        InferenceToml::new("demo")
            .write_to_file(&dir.path().join(crate::project::manifest::MANIFEST_FILE_NAME))
            .unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("main.inf"), "pub fn main() -> i32 { return 0; }\n").unwrap();

        let ctx = crate::project::discover_and_load(dir.path()).unwrap();
        assert_eq!(ctx.entry_point, ctx.root.join("src").join("main.inf"));
        assert!(ctx.entry_point.exists());
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use assert_fs::prelude::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    /// Writes an executable `infc` stub that prints fixed strings for
    /// `--commit-hash` and `--abi-version`. The stub exits 0 by default but
    /// can be configured to exit 1 instead.
    fn write_stub(
        dir: &assert_fs::TempDir,
        commit_stdout: &str,
        abi_stdout: &str,
        exit_nonzero: bool,
    ) -> PathBuf {
        let stub = dir.child("infc");
        let exit_code = i32::from(exit_nonzero);
        let script = format!(
            "#!/bin/sh\n\
             case \"$1\" in\n\
               --commit-hash)\n\
                 printf '%s\\n' \"{commit_stdout}\"\n\
                 exit {exit_code}\n\
                 ;;\n\
               --abi-version)\n\
                 printf '%s\\n' \"{abi_stdout}\"\n\
                 exit {exit_code}\n\
                 ;;\n\
               *)\n\
                 exit 0\n\
                 ;;\n\
             esac\n",
        );
        stub.write_str(&script).unwrap();
        let mut perms = std::fs::metadata(stub.path()).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(stub.path(), perms).unwrap();
        stub.path().to_path_buf()
    }

    #[test]
    fn abi_major_mismatch_is_hard_error() {
        let dir = assert_fs::TempDir::new().unwrap();
        let stub = write_stub(&dir, "nottherightcommit", "2.0", false);
        let err = check_compiler_compatibility(&stub).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("ABI") && msg.contains("rebuild"),
            "expected remediation message, got: {msg}"
        );
    }

    #[test]
    fn abi_minor_difference_warns_only() {
        let dir = assert_fs::TempDir::new().unwrap();
        // Exercise the "infc minor newer than infs" path. The stub reports a
        // far-future minor ("1.5") so the comparison resolves to Greater for
        // any plausible local COMPILER_ABI_MINOR; the branch warns but does not
        // hard-error.
        let stub = write_stub(&dir, "nottherightcommit", "1.5", false);
        let result = check_compiler_compatibility(&stub);
        assert!(result.is_ok(), "minor mismatch should not hard-error");
    }

    #[test]
    fn abi_minor_infs_newer_than_infc_warns_only() {
        let dir = assert_fs::TempDir::new().unwrap();
        // Reverse branch: infs newer than infc. This path only became reachable
        // once COMPILER_ABI_MINOR was bumped above 0. A stub reporting the same
        // major but minor "0" (one below the local minor of 1) exercises the
        // Less arm — it must warn, not hard-error. Constructed against the live
        // major constant so it stays valid across future major bumps.
        let abi = format!("{COMPILER_ABI_MAJOR}.0");
        let stub = write_stub(&dir, "nottherightcommit", &abi, false);
        let result = check_compiler_compatibility(&stub);
        assert!(
            result.is_ok(),
            "infs-newer-than-infc minor mismatch must warn, not hard-error; got: {:?}",
            result.err().map(|e| e.to_string()),
        );
    }

    #[test]
    fn exact_abi_match_with_differing_commit_is_silent_ok() {
        let dir = assert_fs::TempDir::new().unwrap();
        // Commit hashes differ (so the short-circuit in step 1 does NOT fire),
        // forcing the ABI comparison to run. The stub reports EXACTLY the local
        // major.minor, so the comparison resolves to `Ordering::Equal` — the
        // arm that neither warns nor errors. Constructed from the live
        // constants so it tracks future bumps. This is the only direct test of
        // the equal-minor branch; the warn-only tests cover Greater and Less.
        let abi = format!("{COMPILER_ABI_MAJOR}.{COMPILER_ABI_MINOR}");
        let stub = write_stub(&dir, "nottherightcommit", &abi, false);
        let result = check_compiler_compatibility(&stub);
        assert!(
            result.is_ok(),
            "exact ABI match (differing commit) must be a silent Ok via the Equal arm; got: {:?}",
            result.err().map(|e| e.to_string()),
        );
    }

    #[test]
    fn matching_commit_hash_skips_abi_check() {
        let dir = assert_fs::TempDir::new().unwrap();
        // ABI "9.9" would trigger a major mismatch if the ABI check ran.
        // A matching commit hash must short-circuit before that.
        let stub = write_stub(&dir, env!("INFS_GIT_COMMIT"), "9.9", false);
        let result = check_compiler_compatibility(&stub);
        assert!(
            result.is_ok(),
            "matching commit hash must short-circuit ABI check, got: {:?}",
            result.err().map(|e| e.to_string()),
        );
    }

    #[test]
    fn unknown_commit_and_unknown_abi_is_silent() {
        let dir = assert_fs::TempDir::new().unwrap();
        let stub = write_stub(&dir, "unknown", "unknown", false);
        let result = check_compiler_compatibility(&stub);
        assert!(result.is_ok(), "unknown outputs must be graceful");
    }

    #[test]
    fn old_infc_returns_nonzero_for_flags_is_graceful() {
        let dir = assert_fs::TempDir::new().unwrap();
        let stub = write_stub(&dir, "anything", "anything", true);
        let result = check_compiler_compatibility(&stub);
        assert!(
            result.is_ok(),
            "non-zero exit from flag probes must be graceful"
        );
    }

    #[test]
    fn parse_abi_version_accepts_valid() {
        assert_eq!(parse_abi_version("1.0"), Some((1, 0)));
        assert_eq!(parse_abi_version("2.7"), Some((2, 7)));
    }

    #[test]
    fn parse_abi_version_rejects_garbage() {
        assert_eq!(parse_abi_version(""), None);
        assert_eq!(parse_abi_version("1"), None);
        assert_eq!(parse_abi_version("1.x"), None);
        assert_eq!(parse_abi_version("x.1"), None);
        assert_eq!(parse_abi_version("1.0.0"), None);
    }
}
