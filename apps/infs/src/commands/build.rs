//! Build command for the infs CLI.
//!
//! Compiles Inference source files by delegating to the `infc` compiler
//! via subprocess. This module acts as a lightweight bootstrapper, validating
//! arguments and forwarding them to infc.
//!
//! ## Behavior
//!
//! `infs build` always performs full compilation (parse, analyze, codegen)
//! and writes the WASM binary to disk. The `-v` flag additionally generates
//! a Rocq (.v) translation file. `--mode proof` selects proof mode (specs
//! preserved unoptimized for Rocq translation) and implicitly enables `-v`
//! since the `.v` artifact is the proof-mode deliverable. Symmetrically, `-v`
//! with no explicit `--mode` forwards `--mode proof` to `infc` so the emitted
//! `.v` contains per-spec definitions (`compile` mode strips them).
//!
//! ## Single-file vs. project mode
//!
//! The positional path is optional. When a path is given, `build` operates in
//! **single-file mode** (the historical behavior): it compiles exactly that
//! file with `infc` inheriting the current working directory. When the path is
//! omitted, `build` operates in **project mode**: it discovers the project's
//! `Inference.toml` by walking up from the current directory, compiles
//! `<root>/src/main.inf` with `infc`'s working directory set to the project
//! root (so `out/` always lands at the root), and warns about any other
//! `src/*.inf` files (multi-file compilation is gated on #63).
//!
//! ```bash
//! infs build                             # project mode: build <root>/src/main.inf
//! infs build example.inf                 # single-file: parse -> codegen -> out/example.wasm
//! infs build example.inf -v              # also writes out/example.v (proof mode)
//! infs build example.inf --mode proof    # proof mode; writes both .wasm and .v
//! infs build example.inf --mode compile -v   # compile mode + .v (specs stripped)
//! ```

use anyhow::{Context, Result, bail};
use clap::{Args, ValueEnum};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::errors::InfsError;
use crate::project::{self, ProjectContext};
use crate::toolchain::find_infc;
use inference_compiler_interface::{COMPILER_ABI_MAJOR, COMPILER_ABI_MINOR};

/// Compilation mode forwarded to `infc --mode <…>`.
///
/// Mirrors `inference_wasm_codegen::CompilationMode` locally so the `infs`
/// binary does not need to depend on the codegen crate just to parse a CLI
/// flag it only forwards as a string.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildMode {
    Compile,
    Proof,
}

/// Arguments for the build command.
///
/// Always performs full compilation (parse, analyze, codegen) and writes
/// the WASM binary to disk. Use `-v` to also generate a Rocq (.v) file.
///
/// `mode` is `Option<BuildMode>` so the absence of `--mode` is distinguishable
/// from `--mode compile`; this lets `-v` alone forward `--mode proof` to `infc`
/// while `--mode compile -v` is left untouched.
#[derive(Args, Clone)]
pub struct BuildArgs {
    /// Path to the source file to compile.
    ///
    /// When omitted, `build` runs in project mode: it discovers the project's
    /// `Inference.toml` by walking up from the current directory and compiles
    /// `<root>/src/main.inf`. Provide a path to compile a single file directly.
    pub path: Option<PathBuf>,

    /// Generate Rocq (.v) translation file in addition to the WASM binary.
    ///
    /// When `--mode` is omitted, `-v` also forwards `--mode proof` to `infc`
    /// (specs are preserved). Pass `--mode compile -v` to opt out.
    #[clap(short = 'v', action = clap::ArgAction::SetTrue)]
    pub generate_v_output: bool,

    /// Compilation mode (`compile` or `proof`). When omitted, defaults to
    /// `compile` unless `-v` is also passed, in which case it resolves to
    /// `proof` (the `.v` artifact requires preserved specs to be useful).
    /// `proof` preserves spec functions unoptimized for Rocq translation
    /// and implies `-v`.
    #[clap(long = "mode", value_enum)]
    pub mode: Option<BuildMode>,
}

/// Executes the build command with the given arguments.
///
/// Dispatches on the presence of a positional path:
/// - `Some(path)` → [`execute_single_file`] (the historical behavior).
/// - `None` → [`execute_project`]: discover `Inference.toml` from the current
///   directory upward and build `<root>/src/main.inf`.
///
/// ## Errors
///
/// Propagates errors from the selected mode (missing file, compiler lookup,
/// ABI handshake, non-zero infc exit, or — in project mode — discovery and
/// entry-point resolution failures).
pub fn execute(args: &BuildArgs) -> Result<()> {
    if let Some(path) = &args.path {
        return execute_single_file(path, args);
    }

    let cwd =
        std::env::current_dir().context("Failed to determine the current working directory")?;
    let ctx = project::discover_and_load(&cwd)?;
    execute_project(&ctx, args)
}

/// Compiles a single explicit source file (single-file mode).
///
/// ## Execution Flow
///
/// 1. Validates that the source file exists
/// 2. Locates the infc compiler binary
/// 3. Runs the `infc` compatibility handshake (git hash + ABI version)
/// 4. Builds and executes the infc command, forwarding `-v` if requested
/// 5. Propagates exit code from infc
///
/// ## Errors
///
/// Returns an error if:
/// - The source file does not exist
/// - infc compiler cannot be found
/// - infc reports a *major* ABI version mismatch (hard error with remediation)
/// - infc exits with non-zero code (as `InfsError::ProcessExitCode`)
fn execute_single_file(path: &Path, args: &BuildArgs) -> Result<()> {
    if !path.exists() {
        bail!("Path not found: {}", path.display());
    }

    let infc_path = find_infc()?;
    check_compiler_compatibility(&infc_path)?;

    let mut cmd = Command::new(&infc_path);
    cmd.arg(path);

    if args.generate_v_output {
        cmd.arg("-v");
    }

    // Forward only what the user explicitly passed. `infc::normalize_args`
    // owns the `-v` ↔ `--mode proof` implication; mirroring it here would
    // create a second source of truth that could silently drift.
    if let Some(mode) = args.mode {
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

/// Compiles the entry point of a discovered project (project mode).
///
/// Resolves the conventional `src/main.inf` entry point, warns about any other
/// `src/*.inf` files (multi-file compilation is gated on #63), then spawns
/// `infc` with its working directory set to the project root so that `out/`
/// lands at the root regardless of where `infs build` was invoked.
///
/// The entry-point source is passed *relative to the root* (`src/main.inf`),
/// matching the CWD-relativity contract between `infs` and `infc`: `infc`
/// resolves both its source argument and `out/` against the inherited CWD.
///
/// ## Errors
///
/// Returns an error if:
/// - The entry point `<root>/src/main.inf` does not exist
/// - infc compiler cannot be found
/// - infc reports a *major* ABI version mismatch (hard error with remediation)
/// - infc exits with non-zero code (as `InfsError::ProcessExitCode`)
fn execute_project(ctx: &ProjectContext, args: &BuildArgs) -> Result<()> {
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

    if args.generate_v_output {
        cmd.arg("-v");
    }

    // Forward only what the user explicitly passed (see `execute_single_file`).
    if let Some(mode) = args.mode {
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
/// error here — the missing-entry-point check in `execute_project` already
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
fn mode_flag(mode: BuildMode) -> &'static str {
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
fn check_compiler_compatibility(infc_path: &Path) -> Result<()> {
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

    fn build_args() -> BuildArgs {
        BuildArgs {
            path: None,
            generate_v_output: false,
            mode: None,
        }
    }

    /// `execute_project` must fail with a remediation error before doing any
    /// compiler lookup when `src/main.inf` is absent. This is platform
    /// independent — it errors before spawning `infc`.
    #[test]
    fn execute_project_errors_when_entry_missing() {
        let dir = assert_fs::TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        // Manifest present, but no src/main.inf.
        let ctx = ProjectContext {
            root: root.clone(),
            manifest: InferenceToml::new("demo"),
            entry_point: root.join("src").join("main.inf"),
        };

        let err = execute_project(&ctx, &build_args()).unwrap_err();
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
