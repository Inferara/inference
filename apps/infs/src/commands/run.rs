//! Run command for the infs CLI.
//!
//! Compiles Inference source and executes the resulting WASM with wasmtime in a
//! single step. Compilation is delegated to the `infc` compiler via subprocess.
//!
//! ## Single-file vs. project mode
//!
//! The positional path is optional. When a path is given, `run` operates in
//! **single-file mode** (the historical behavior): it compiles exactly that
//! file with `infc` inheriting the current working directory and invokes the
//! requested `--entry-point`. When the path is omitted, `run` operates in
//! **project mode**: it discovers the project's `Inference.toml` by walking up
//! from the current directory, performs the same project build as `infs build`
//! (so `<root>/out/main.wasm` is produced), and invokes `main` by convention.
//!
//! ```bash
//! infs run                                    # project mode: build + invoke main
//! infs run program.inf                        # single-file: invoke main()
//! infs run program.inf --entry-point helper   # single-file: invoke helper()
//! ```
//!
//! ## Project-mode conventions
//!
//! - **Always invokes `main`**. Project mode has no notion of an alternate
//!   entry point yet; a non-`main` `--entry-point` is rejected with guidance to
//!   use single-file mode rather than silently ignored.
//! - **Trailing var-args are ignored**: `main` is always invoked with
//!   `argc=0, argv=0`. Note that project mode is structurally arg-free: the
//!   first bare token on the command line binds to the positional `path` and
//!   therefore selects *single-file* mode, so trailing args cannot actually
//!   reach project mode through the CLI. The warning below is retained as a
//!   defensive, self-documenting guard should the argument layout ever change.
//! - **Gains the `infc` compatibility handshake** for free via the shared
//!   project-build helper. Single-file `run` deliberately keeps its prior
//!   no-handshake behavior to avoid an unrelated behavior change.
//! - **Always builds in compile mode**, regardless of the manifest's
//!   `[build] mode`. `run` executes the WASM, and proof-mode WASM embeds the
//!   custom non-deterministic opcodes (the `0xfc` family) that wasmtime cannot
//!   execute. So project `run` ignores `[build] mode` and
//!   `[verification] output-dir` entirely: the artifact is always an executable
//!   under `<root>/out/`. Use `infs build` to produce proof artifacts.
//! - **Missing-WASM guard:** if the build reports success but
//!   `<root>/out/main.wasm` is absent, `run` errors before invoking wasmtime,
//!   mirroring the single-file `compile_to_wasm` guard.
//!
//! ## Prerequisites
//!
//! This command requires:
//! - `infc` compiler (via toolchain or PATH)
//! - `wasmtime` WebAssembly runtime (in PATH)

use anyhow::{Context, Result, bail};
use clap::Args;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::commands::project_build::run_project_build;
use crate::errors::InfsError;
use crate::project::{self, ProjectContext};
use crate::toolchain::find_infc;

/// The entry point invoked in project mode and the default for single-file mode.
const DEFAULT_ENTRY_POINT: &str = "main";

/// Arguments for the run command.
///
/// The run command compiles source to WASM and executes it with wasmtime.
/// Any arguments after the source path are passed to the invoked function.
#[derive(Args)]
pub struct RunArgs {
    /// Path to the source file to run.
    ///
    /// When omitted, `run` operates in project mode: it discovers the project's
    /// `Inference.toml` by walking up from the current directory, builds
    /// `<root>/src/main.inf`, and invokes `main`. Provide a path to run a
    /// single file directly.
    pub path: Option<PathBuf>,

    /// Function to invoke as entry point.
    ///
    /// Defaults to "main". The function must be exported (marked `pub` in source).
    /// For `main`, argc/argv arguments (0 0) are passed automatically.
    ///
    /// In project mode only `main` is supported; a non-`main` value is an error
    /// (run a single file for custom entry points).
    #[clap(long, default_value = DEFAULT_ENTRY_POINT)]
    pub entry_point: String,

    /// Arguments to pass to the invoked function.
    ///
    /// For functions other than `main`, these are passed directly as function arguments.
    /// For `main`, these are ignored (argc=0, argv=0 is always used). These only
    /// apply in single-file mode: the first bare token binds to `path`, so
    /// project mode (no path) never receives trailing args.
    #[clap(trailing_var_arg = true)]
    pub args: Vec<String>,
}

/// Executes the run command with the given arguments.
///
/// Dispatches on the presence of a positional path:
/// - `Some(path)` → [`execute_single_file`] (the historical behavior, including
///   its no-handshake compilation).
/// - `None` → [`execute_project`]: discover `Inference.toml` from the current
///   directory upward, build the project, and invoke `main`.
///
/// ## Errors
///
/// Propagates errors from the selected mode (missing file, missing wasmtime,
/// compiler lookup, compilation failure, WASM execution failure, or — in
/// project mode — discovery, entry-point resolution, and `--entry-point`
/// rejection).
pub fn execute(args: &RunArgs) -> Result<()> {
    if let Some(path) = &args.path {
        return execute_single_file(path, args);
    }

    execute_project(args)
}

/// Runs a single explicit source file (single-file mode).
///
/// ## Execution Flow
///
/// 1. Validates source file exists
/// 2. Checks for wasmtime availability
/// 3. Locates the infc compiler
/// 4. Compiles source to WASM via infc subprocess (no ABI handshake — single-file
///    `run` deliberately keeps its prior no-handshake behavior)
/// 5. Executes WASM with wasmtime, invoking `--entry-point`
/// 6. Propagates exit code from wasmtime
///
/// ## Errors
///
/// Returns an error if:
/// - The source file does not exist
/// - wasmtime is not found in PATH
/// - infc compiler cannot be found
/// - Compilation fails
/// - WASM execution fails
fn execute_single_file(path: &Path, args: &RunArgs) -> Result<()> {
    if !path.exists() {
        bail!("Path not found: {}", path.display());
    }

    check_wasmtime_availability()?;

    let infc_path = find_infc()?;

    let wasm_path = compile_to_wasm(&infc_path, path)?;

    run_wasmtime(&wasm_path, &args.entry_point, &args.args)
}

/// Builds and runs a discovered project (project mode).
///
/// Resolves the project from the current directory, performs the shared project
/// build (which runs the `infc` compatibility handshake), then invokes `main` on
/// `<root>/out/main.wasm` via wasmtime. Project mode always invokes `main`; a
/// non-`main` `--entry-point` is rejected. Trailing var-args cannot reach this
/// path (the first token binds to `path`); the warning is a defensive guard
/// documenting the ignore-args policy.
///
/// wasmtime availability is checked *first* — before any compilation — so an
/// environment lacking the runtime fails fast, matching single-file mode.
///
/// ## Errors
///
/// Returns an error if:
/// - `--entry-point` is set to a non-`main` value (project mode invokes `main`)
/// - wasmtime is not found in PATH
/// - No `Inference.toml` is found in the current directory or any ancestor
/// - The project build fails (missing entry point, ABI handshake, infc error)
/// - The build succeeds but `<root>/out/main.wasm` is absent
/// - WASM execution fails
fn execute_project(args: &RunArgs) -> Result<()> {
    if args.entry_point != DEFAULT_ENTRY_POINT {
        bail!(
            "Project mode always invokes `main`; `--entry-point {}` is not \
             supported here. To run a custom entry point, pass the source file \
             explicitly (`infs run path/to/file.inf --entry-point {}`).",
            args.entry_point,
            args.entry_point
        );
    }

    if !args.args.is_empty() {
        eprintln!(
            "warning: trailing arguments are ignored in project mode; `main` \
             is invoked with argc=0, argv=0."
        );
    }

    check_wasmtime_availability()?;

    let cwd =
        std::env::current_dir().context("Failed to determine the current working directory")?;
    let ctx = project::discover_and_load(&cwd)?;

    // Project `run` always builds an executable (compile mode) in `out/`,
    // regardless of `[build] mode` in the manifest: proof-mode WASM embeds the
    // custom non-deterministic opcodes (0xfc family) that wasmtime cannot
    // execute. Hence `mode = None` and `out_dir = None` here — manifest
    // mode/output-dir resolution lives only in `build`'s project path.
    run_project_build(&ctx, false, None, None)?;

    let wasm_path = project_wasm_path(&ctx);
    if !wasm_path.exists() {
        bail!(
            "Compilation succeeded but WASM file not found at: {}",
            wasm_path.display()
        );
    }

    run_wasmtime(&wasm_path, DEFAULT_ENTRY_POINT, &[])
}

/// The conventional project output path: `<root>/out/main.wasm`.
///
/// `out/` is `infc`'s default output directory and the build spawns `infc` with
/// its working directory set to the project root, so the WASM lands here. Built
/// with [`Path::join`] so the separator is platform-correct (never a literal `/`).
fn project_wasm_path(ctx: &ProjectContext) -> PathBuf {
    ctx.root.join("out").join("main.wasm")
}

/// Checks if wasmtime is available in PATH.
fn check_wasmtime_availability() -> Result<()> {
    if which::which("wasmtime").is_err() {
        bail!(
            "wasmtime not found in PATH.\n\n\
            wasmtime is a WebAssembly runtime. To install:\n  \
            - macOS: brew install wasmtime\n  \
            - Linux: curl https://wasmtime.dev/install.sh -sSf | bash\n  \
            - Windows: winget install wasmtime\n  \
            - Or visit: https://wasmtime.dev/"
        );
    }
    Ok(())
}

/// Compiles source file to WASM binary using infc subprocess.
///
/// Calls infc with `--parse --codegen -o` flags to generate the WASM file
/// in the `out/` directory.
fn compile_to_wasm(infc_path: &Path, source_path: &Path) -> Result<PathBuf> {
    let mut cmd = Command::new(infc_path);
    cmd.arg(source_path)
        .arg("--parse")
        .arg("--codegen")
        .arg("-o");

    let status = cmd
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to execute infc at {}", infc_path.display()))?;

    if !status.success() {
        let code = status.code().unwrap_or(1);
        return Err(InfsError::process_exit_code(code).into());
    }

    let source_fname = source_path
        .file_stem()
        .unwrap_or_else(|| std::ffi::OsStr::new("module"))
        .to_str()
        .unwrap_or("module");

    let wasm_path = PathBuf::from("out").join(format!("{source_fname}.wasm"));

    if !wasm_path.exists() {
        bail!(
            "Compilation succeeded but WASM file not found at: {}",
            wasm_path.display()
        );
    }

    Ok(wasm_path)
}

/// Runs wasmtime with the given WASM file, invoking a specific function.
///
/// Uses `--invoke <entry_point>` to call the specified exported function.
/// For `main`, automatically passes argc=0, argv=0 arguments.
/// For other functions, passes user-provided arguments.
///
/// Stderr is captured and only displayed if wasmtime fails, to suppress
/// the experimental feature warnings about `--invoke` that appear on success.
///
/// Returns `Ok(())` on success, or `Err(InfsError::ProcessExitCode)` if wasmtime
/// exits with a non-zero code. This allows the caller to propagate the exit code
/// without bypassing RAII cleanup.
fn run_wasmtime(wasm_path: &Path, entry_point: &str, args: &[String]) -> Result<()> {
    println!("Invoking '{entry_point}' with wasmtime...");

    let mut cmd = Command::new("wasmtime");
    cmd.arg("--invoke").arg(entry_point).arg(wasm_path);

    if entry_point == "main" {
        // main(argc: i32, argv: i32) -> i32 requires two arguments
        cmd.arg("0").arg("0");
    } else {
        for arg in args {
            cmd.arg(arg);
        }
    }

    let output = cmd
        .stdin(std::process::Stdio::inherit())
        .output()
        .with_context(|| "Failed to execute wasmtime")?;

    // Print stdout (the function's return value)
    if !output.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }

    if output.status.success() {
        Ok(())
    } else {
        // Only show stderr on failure (hides experimental warnings on success)
        if !output.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }
        let code = output.status.code().unwrap_or(1);
        Err(InfsError::process_exit_code(code).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The project WASM path is `<root>/out/main.wasm`, assembled with path
    /// joins so the components are platform-correct (never a literal `/`).
    #[test]
    fn project_wasm_path_is_root_out_main_wasm() {
        let dir = assert_fs::TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        let ctx = ProjectContext {
            root: root.clone(),
            manifest: crate::project::manifest::InferenceToml::new("demo"),
            entry_point: root.join("src").join("main.inf"),
        };

        let wasm = project_wasm_path(&ctx);
        assert_eq!(wasm, root.join("out").join("main.wasm"));
        assert_eq!(wasm.file_name().unwrap(), "main.wasm");
        assert_eq!(wasm.parent().unwrap().file_name().unwrap(), "out");
    }

    /// A non-`main` `--entry-point` in project mode is rejected before any
    /// external tool is consulted (the check is the first thing `execute_project`
    /// does), with guidance to use single-file mode. This is the only branch of
    /// `execute_project` reachable without `infc`/wasmtime, so it is unit-tested
    /// here; the full build+run paths are covered by the integration suite.
    #[test]
    fn execute_project_rejects_non_main_entry_point() {
        let args = RunArgs {
            path: None,
            entry_point: "helper".to_string(),
            args: Vec::new(),
        };

        let err = execute_project(&args).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("Project mode always invokes `main`")
                && msg.contains("infs run path/to/file.inf"),
            "expected custom-entry-point remediation, got: {msg}"
        );
    }

    /// Explicit `--entry-point main` is the default and must *not* be treated as
    /// a custom entry point — the rejection above must not fire for it. Verified
    /// at the unit level so it does not depend on wasmtime; `execute_project`
    /// proceeds past arg validation (and then to the wasmtime/discovery steps,
    /// which the integration suite exercises end-to-end).
    #[test]
    fn execute_project_accepts_explicit_main_entry_point() {
        // The entry-point guard keys off the string equalling DEFAULT_ENTRY_POINT.
        let args = RunArgs {
            path: None,
            entry_point: DEFAULT_ENTRY_POINT.to_string(),
            args: Vec::new(),
        };

        // We cannot assert the full pipeline here without external tools, but we
        // can assert the guard does not reject `main`: any error must come from a
        // *later* stage (wasmtime/discovery), never the entry-point bail.
        if let Err(err) = execute_project(&args) {
            let msg = format!("{err}");
            assert!(
                !msg.contains("Project mode always invokes `main`"),
                "explicit `main` must not hit the custom-entry-point bail; got: {msg}"
            );
        }
    }
}
