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
/// A thin wrapper over [`run_project_build`], which owns the shared
/// project-build logic (entry-point resolution, extra-file warnings, the `infc`
/// handshake, spawning `infc` with `current_dir(root)`, and exit-code
/// propagation) so that `infs run` can reuse it. This function only forwards
/// the `build`-specific CLI flags.
///
/// ## Errors
///
/// Propagates every error [`run_project_build`] can return (missing entry
/// point, compiler lookup, ABI handshake, non-zero infc exit).
fn execute_project(ctx: &ProjectContext, args: &BuildArgs) -> Result<()> {
    run_project_build(ctx, args.generate_v_output, args.mode)
}
