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
//!
//! ## Project-mode manifest semantics
//!
//! In project mode the manifest's `[build] mode` and `[verification]
//! output-dir` become consumed configuration, with CLI flags overriding:
//!
//! - **Effective mode** = CLI `--mode` if present, else manifest `[build]
//!   mode`. Manifest `proof` forwards `--mode proof`; manifest `compile`
//!   (explicit or defaulted) forwards *nothing* so that `infc`'s `-v` ⇄ proof
//!   implication stays the single source of truth in `infc::normalize_args`.
//!   `infs` never forwards `--mode compile`.
//! - **`output-dir`** is honored *only in effective-proof mode* and is
//!   normalized (relative-only, trailing separator stripped) before forwarding
//!   as `--out-dir`, which moves both `.wasm` and `.v`. In compile mode it is
//!   ignored entirely — the default `proofs/` must never relocate
//!   `out/main.wasm`. The default proof-mode `output-dir` is `proofs/`, so a
//!   default proof build writes both artifacts under `<root>/proofs/`.
//! - **`-v` alone** (no `--mode`, compile-mode manifest) is *not* treated as
//!   proof by `infs`: only the explicitly-owned mode signal triggers
//!   effective-proof mode. `infs build -v` forwards just `-v`; `infc` derives
//!   proof internally and writes both artifacts to `out/` (no `--out-dir`).
//! - **`--out-dir` is forwarded only to an `infc` that supports it**;
//!   pairing a non-default `output-dir` with an older `infc` hard-errors with
//!   remediation rather than failing opaquely in the subprocess.

use anyhow::{Context, Result, bail};
use clap::{Args, ValueEnum};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::commands::project_build::{check_compiler_compatibility, mode_flag, run_project_build};
use crate::errors::InfsError;
use crate::project::{self, ProjectContext};
use crate::toolchain::find_infc;

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
/// Resolves the *effective* build configuration from the CLI flags and the
/// manifest's `[build] mode` / `[verification] output-dir`, then delegates to
/// [`run_project_build`] (which owns the shared spawn, handshake, and exit-code
/// propagation). The forwarding rules are documented on
/// [`resolve_effective_mode`] and [`resolve_out_dir`].
///
/// ## Errors
///
/// Propagates every error [`run_project_build`] can return (missing entry
/// point, compiler lookup, ABI handshake, `--out-dir` capability gate, non-zero
/// infc exit), plus `output-dir` normalization failures.
fn execute_project(ctx: &ProjectContext, args: &BuildArgs) -> Result<()> {
    let effective_mode = resolve_effective_mode(args.mode, &ctx.manifest.build.mode);
    let out_dir = resolve_out_dir(effective_mode, &ctx.manifest.verification)?;

    run_project_build(ctx, args.generate_v_output, effective_mode, out_dir.as_deref())
}

/// Resolves the effective `--mode` to forward to `infc`, or `None` to forward
/// nothing.
///
/// Precedence:
/// - CLI `--mode` always wins when present (`compile` or `proof`).
/// - Otherwise, manifest `[build] mode = "proof"` forwards `--mode proof`.
/// - Manifest `"compile"` (explicit or defaulted) forwards **nothing**.
///
/// Why never forward `--mode compile`: `infs` does not own the `-v` ⇄ `--mode
/// proof` implication — `infc::normalize_args` does, and it is the single
/// source of truth. Forwarding an explicit `--mode compile` when the user
/// passed only `-v` would suppress that implication inside `infc` (turning
/// `-v`-alone into a spec-stripped `.v`), reintroducing exactly the drift that
/// single source of truth avoids. Forwarding nothing for the compile case
/// leaves `infc` free to derive proof from `-v`.
///
/// The manifest string is already validated to `compile`/`proof` on load, so
/// the fallback maps any non-`proof` value to "forward nothing".
fn resolve_effective_mode(cli_mode: Option<BuildMode>, manifest_mode: &str) -> Option<BuildMode> {
    if let Some(mode) = cli_mode {
        return Some(mode);
    }
    if manifest_mode == "proof" {
        return Some(BuildMode::Proof);
    }
    None
}

/// Resolves the `--out-dir` to forward, honoring `[verification] output-dir`
/// **only in effective-proof mode**.
///
/// In compile mode (or when no explicit proof mode is in effect) the manifest
/// `output-dir` is ignored entirely and `None` is returned — a default-manifest
/// build must never relocate `out/main.wasm` into the `proofs/` default, and
/// `--out-dir` cannot isolate the `.v` from the `.wasm` anyway (it moves both).
///
/// In effective-proof mode the manifest string is normalized through `PathBuf`
/// (relative-only, trailing separator stripped) and returned for forwarding.
/// The default `"proofs/"` normalizes to `proofs`, so a default proof-mode
/// build writes both artifacts under `<root>/proofs/`.
///
/// Note: this keys off the mode `infs` explicitly owns (CLI `--mode proof` or
/// manifest `mode = "proof"`). It deliberately does **not** treat `-v`-alone as
/// proof: that implication belongs to `infc::normalize_args`, so `infs build -v`
/// on a compile-mode manifest forwards only `-v` and lets `infc` write both
/// artifacts to `out/` — `output-dir` is not consulted.
///
/// # Errors
///
/// Returns an error if the manifest `output-dir` is empty or absolute.
fn resolve_out_dir(
    effective_mode: Option<BuildMode>,
    verification: &crate::project::manifest::VerificationConfig,
) -> Result<Option<PathBuf>> {
    if effective_mode == Some(BuildMode::Proof) {
        Ok(Some(verification.normalized_output_dir()?))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::manifest::VerificationConfig;

    #[test]
    fn cli_mode_overrides_manifest() {
        // CLI proof wins over manifest compile, and CLI compile wins over
        // manifest proof — the CLI is always authoritative when present.
        assert_eq!(
            resolve_effective_mode(Some(BuildMode::Proof), "compile"),
            Some(BuildMode::Proof)
        );
        assert_eq!(
            resolve_effective_mode(Some(BuildMode::Compile), "proof"),
            Some(BuildMode::Compile)
        );
    }

    #[test]
    fn manifest_proof_forwards_proof() {
        assert_eq!(resolve_effective_mode(None, "proof"), Some(BuildMode::Proof));
    }

    #[test]
    fn manifest_compile_forwards_nothing() {
        // Compile (explicit or defaulted) must forward nothing so infc's
        // `-v` ⇄ proof implication stays the single source of truth.
        assert_eq!(resolve_effective_mode(None, "compile"), None);
    }

    #[test]
    fn compile_mode_ignores_output_dir() {
        // Even a non-default output-dir must be ignored in compile mode,
        // so out/main.wasm is never relocated into proofs/.
        let verification = VerificationConfig {
            output_dir: String::from("artifacts"),
        };
        assert_eq!(resolve_out_dir(None, &verification).unwrap(), None);
        assert_eq!(
            resolve_out_dir(Some(BuildMode::Compile), &verification).unwrap(),
            None
        );
    }

    #[test]
    fn proof_mode_forwards_default_output_dir_normalized() {
        // The default "proofs/" must normalize to `proofs` and be forwarded.
        let verification = VerificationConfig::default();
        assert_eq!(
            resolve_out_dir(Some(BuildMode::Proof), &verification).unwrap(),
            Some(PathBuf::from("proofs"))
        );
    }

    #[test]
    fn proof_mode_forwards_custom_output_dir() {
        let verification = VerificationConfig {
            output_dir: String::from("artifacts/"),
        };
        assert_eq!(
            resolve_out_dir(Some(BuildMode::Proof), &verification).unwrap(),
            Some(PathBuf::from("artifacts"))
        );
    }

    #[test]
    fn proof_mode_propagates_output_dir_validation_error() {
        // An absolute output-dir is rejected — but only when proof mode actually
        // consults it (in compile mode the bad value is never read).
        let abs = if cfg!(windows) { r"C:\x" } else { "/x" };
        let verification = VerificationConfig {
            output_dir: String::from(abs),
        };
        assert!(resolve_out_dir(Some(BuildMode::Proof), &verification).is_err());
        assert!(
            resolve_out_dir(None, &verification).is_ok(),
            "compile mode must not even read a malformed output-dir"
        );
    }
}
