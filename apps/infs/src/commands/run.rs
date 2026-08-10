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
//! Single-file mode is not manifest-blind: it walks up to the nearest
//! `Inference.toml` and honors both `[build] wasm-features` and
//! `[wasm-dependencies]`, so running one file of a project cannot execute a
//! module at a different WebAssembly instruction level than `infs build` would
//! produce for it, and cannot fail to resolve an external that the same build
//! links. This path overwrites the very artifact `infs build` produces, so any
//! divergence would be observable as one command destroying the other's output.
//!
//! ## External-module search directories
//!
//! `-L`/`--wasm-lib-dir` is accepted in both modes, spelled exactly as on `infs
//! build`, but the two modes anchor a relative directory differently — and both
//! land on "it means what it meant at the shell":
//!
//! - **Single-file mode forwards each directory verbatim.** `infc` inherits the
//!   invocation working directory here, so a relative dir already resolves
//!   against the directory the user typed it in.
//! - **Project mode anchors first**, because the shared helper re-anchors `infc`
//!   to the project root; see [`crate::commands::project_build`].
//!
//! ```bash
//! infs run                                    # project mode: build + invoke main
//! infs run program.inf                        # single-file: invoke main()
//! infs run program.inf --entry-point helper   # single-file: invoke helper()
//! infs run program.inf -L libs                # single-file: search libs/ for externals
//! infs run -L libs                            # project mode: same, for the whole project
//! ```
//!
//! Options must be placed **before** the first bare trailing token: [`RunArgs`]
//! collects trailing var-args for the invoked function, so `infs run f.inf 1 -L
//! libs` passes `-L` and `libs` to the program rather than parsing them.
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
//!   project-build helper. Single-file `run` keeps its prior no-handshake
//!   behavior except when the enclosing manifest requests `wasm-features`, where
//!   a capability probe is the only way to refuse a request the compiler cannot
//!   honor. Neither `--wasm-dep` nor `--wasm-lib-dir` is capability-gated on
//!   either path: both arrived with external-module support itself rather than
//!   at a distinguishable ABI minor, so the handshake has nothing to check.
//! - **Resolves `[wasm-dependencies]`**, also via the shared helper, and
//!   forwards every `-L` it was passed: the project it runs is the one `infs
//!   build` would produce, externals included. A project binding `use { … } from
//!   <module>` is otherwise unrunnable, since `infc` resolves externals from
//!   forwarded flags only.
//! - **Always builds in compile mode**, regardless of the manifest's
//!   `[build] mode`. `run` executes the WASM, and proof-mode WASM embeds the
//!   custom non-deterministic opcodes (the `0xfc` family) that wasmtime cannot
//!   execute. So project `run` ignores `[build] mode` and
//!   `[verification] output-dir` entirely: the artifact is always an executable
//!   under `<root>/out/`. Use `infs build` to produce proof artifacts.
//! - **Applies `[build.wasm-opt]`** when the manifest declares it: `run` builds
//!   an executable in compile mode, so the same post-build optimization `build`
//!   performs runs here too (`run` executes exactly what it ships). Pass
//!   `--no-wasm-opt` to skip it.
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

use crate::commands::build::{
    enclosing_manifest, format_wasm_dep_arg, manifest_wasm_dependencies, manifest_wasm_features,
};
use crate::commands::project_build::{
    forward_wasm_features, probe_compiler_compatibility, run_project_build,
};
use crate::errors::InfsError;
use crate::project::manifest::MANIFEST_FILE_NAME;
use crate::project::{self, ProjectContext};
use crate::toolchain::resolver::{ResolutionSource, find_infc_with_source};
use inference_compiler_interface::WasmFeatureName;

/// The entry point invoked in project mode and the default for single-file mode.
const DEFAULT_ENTRY_POINT: &str = "main";

/// Arguments for the run command.
///
/// The run command compiles source to WASM and executes it with wasmtime.
///
/// [`RunArgs::args`] is a trailing var-arg, which sets the ordering contract for
/// the whole struct: options are consumed wherever they appear *before* the first
/// bare token that is not the source path, and everything from that token onward
/// is handed to the invoked function untouched. `infs run f.inf -L libs 1` passes
/// `libs` to the compiler and `1` to the program; `infs run f.inf 1 -L libs`
/// passes all three of `1`, `-L`, `libs` to the program.
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

    /// Directory to search for external `.wasm` modules referenced by
    /// `use { … } from <module>;`. Repeatable; forwarded as `--wasm-lib-dir` in
    /// both single-file and project mode, spelled exactly as on `infs build`. A
    /// relative dir always means what it meant at the shell: single-file `infc`
    /// inherits the invocation directory, and the project path anchors the dir to
    /// that directory before forwarding, because it moves `infc` to the project
    /// root.
    ///
    /// Must appear before the first bare trailing token, which starts the
    /// arguments handed to the invoked function: `infs run f.inf -L libs 1`
    /// searches `libs`, `infs run f.inf 1 -L libs` does not.
    // Kept ahead of `no_wasm_opt` to mirror `BuildArgs`: clap lists options in
    // declaration order, so the two flags shared with `infs build` must be declared
    // in the same relative order for both `--help` screens to present them alike.
    #[clap(short = 'L', long = "wasm-lib-dir", value_name = "DIR")]
    pub wasm_lib_dirs: Vec<PathBuf>,

    /// Skip the `[build.wasm-opt]` post-build optimization for this build.
    ///
    /// Project mode only: `run` executes the artifact it builds, so this makes
    /// it run exactly what `infc` emitted. No effect in single-file mode or when
    /// no `[build.wasm-opt]` table is present.
    #[clap(long = "no-wasm-opt")]
    pub no_wasm_opt: bool,

    /// Arguments to pass to the invoked function.
    ///
    /// For functions other than `main`, these are passed directly as function arguments.
    /// For `main`, these are ignored (argc=0, argv=0 is always used). These only
    /// apply in single-file mode: the first bare token binds to `path`, so
    /// project mode (no path) never receives trailing args.
    ///
    /// Collection starts at the first bare token after the source path and takes
    /// everything from there, options included — so `infs run f.inf 1 -L libs`
    /// yields `["1", "-L", "libs"]` rather than parsing `-L`. Place every option
    /// before that token, or separate the program's arguments with `--`
    /// (`infs run f.inf -- -L x` yields `["-L", "x"]`).
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
/// 3. Resolves the enclosing project's `[build] wasm-features` and
///    `[wasm-dependencies]`, if any
/// 4. Locates the infc compiler
/// 5. Compiles source to WASM via infc subprocess, forwarding those settings
///    alongside every `-L` the user passed
/// 6. Executes WASM with wasmtime, invoking `--entry-point`
/// 7. Propagates exit code from wasmtime
///
/// The enclosing manifest is honored here for the same reason `infs build
/// <path>` honors it: one project must not emit modules at two different
/// WebAssembly instruction levels, or bind one module name to two different
/// `.wasm` files, depending on how the build was invoked — and this path
/// overwrites the very artifact `infs build` produces.
///
/// Every manifest-derived setting comes off the single [`enclosing_manifest`]
/// call above, and that resolution happens *before* the compiler lookup so a
/// malformed manifest is reported without first probing the toolchain.
///
/// ## Errors
///
/// Returns an error if:
/// - The source file does not exist
/// - wasmtime is not found in PATH
/// - a `[wasm-dependencies]` key is not a well-formed logical module name, or a
///   resolved dependency path is not valid UTF-8
/// - infc compiler cannot be found
/// - the enclosing manifest requests `wasm-features` the resolved `infc` cannot
///   honor (which is also the only case that runs the ABI handshake here)
/// - Compilation fails
/// - WASM execution fails
fn execute_single_file(path: &Path, args: &RunArgs) -> Result<()> {
    if !path.exists() {
        bail!("Path not found: {}", path.display());
    }

    check_wasmtime_availability()?;

    let enclosing = enclosing_manifest(path)?;
    let features = manifest_wasm_features(enclosing.as_ref().map(|(_, manifest)| manifest))?;
    let deps = manifest_wasm_dependencies(enclosing.as_ref())?;
    let manifest_path = enclosing
        .as_ref()
        .map(|(dir, _)| dir.join(MANIFEST_FILE_NAME));

    let (infc_path, infc_source) = find_infc_with_source()?;

    let wasm_path = compile_to_wasm(
        &infc_path,
        infc_source,
        path,
        &args.wasm_lib_dirs,
        &deps,
        &features,
        manifest_path.as_deref(),
    )?;

    run_wasmtime(&wasm_path, &args.entry_point, &args.args)
}

/// Builds and runs a discovered project (project mode).
///
/// Resolves the project from the current directory, performs the shared project
/// build (which runs the `infc` compatibility handshake and forwards the
/// `-L` directories given here), then invokes `main` on `<root>/out/main.wasm`
/// via wasmtime. Project mode always invokes `main`; a non-`main` `--entry-point`
/// is rejected. Trailing var-args cannot reach this path (the first token binds
/// to `path`); the warning is a defensive guard documenting the ignore-args
/// policy. `-L` is the one flag that *can* reach here, since it takes its own
/// value rather than a bare token.
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
/// - The project build fails (missing entry point, ABI handshake,
///   external-module forwarding, infc error)
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
    // mode/output-dir resolution lives only in `build`'s project path. The lib
    // dirs pass straight through; the helper anchors them to the invocation
    // directory because it moves `infc` to the project root. The
    // `[build.wasm-opt]` optimization still applies (unless `--no-wasm-opt`) so
    // `run` executes exactly what `build` would ship.
    run_project_build(
        &ctx,
        false,
        None,
        None,
        &args.wasm_lib_dirs,
        args.no_wasm_opt,
    )?;

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
/// Calls infc with `--parse --codegen -o` to generate the WASM file in the
/// `out/` directory, forwarding everything the artifact this command then
/// executes must be built with. The wire order is
///
/// ```text
/// <source> --parse --codegen -o [--wasm-lib-dir <dir>]* [--wasm-dep <name>=<path>]* [--wasm-features <list>]
/// ```
///
/// which is the relative order single-file `infs build` uses, so the two
/// commands present one project to `infc` identically.
///
/// `wasm_lib_dirs` is forwarded **verbatim**, deliberately: this path never sets
/// the child's working directory, so `infc` inherits the invocation directory and
/// a relative dir still names what the user typed. Anchoring them would be wrong
/// here, and is required only where the child is re-anchored to the project root —
/// see [`run_project_build`].
///
/// `deps` arrives already resolved to absolute paths, so it needs no anchoring
/// under any working directory.
///
/// The compatibility handshake runs only when there is a feature request to gate.
/// Single-file `run` otherwise keeps its historical handshake-free behavior: the
/// probe exists to refuse an unhonorable request, and paying for it on every run
/// would add ABI warnings to invocations that ask nothing of the compiler.
/// Neither `--wasm-lib-dir` nor `--wasm-dep` is gated: both arrived with
/// external-module support itself rather than at a distinguishable ABI minor, so
/// there is no capability to probe. An `infc` too old to accept them is therefore
/// reported by `infc`'s own argument parser rather than with remediation from
/// here.
///
/// # Errors
///
/// Returns an error if a resolved dependency path is not valid UTF-8 (it cannot
/// round-trip through the single-`String` `--wasm-dep` argument), if the resolved
/// `infc` cannot honor a requested feature, if `infc` exits non-zero, or if the
/// expected artifact is absent afterwards.
fn compile_to_wasm(
    infc_path: &Path,
    infc_source: ResolutionSource,
    source_path: &Path,
    wasm_lib_dirs: &[PathBuf],
    deps: &[(String, PathBuf)],
    features: &[WasmFeatureName],
    manifest_path: Option<&Path>,
) -> Result<PathBuf> {
    let mut cmd = Command::new(infc_path);
    cmd.arg(source_path)
        .arg("--parse")
        .arg("--codegen")
        .arg("-o");

    for dir in wasm_lib_dirs {
        cmd.arg("--wasm-lib-dir").arg(dir);
    }

    for (name, path) in deps {
        cmd.arg("--wasm-dep").arg(format_wasm_dep_arg(name, path)?);
    }

    if !features.is_empty() {
        let compat = probe_compiler_compatibility(infc_path, infc_source)?;
        forward_wasm_features(&mut cmd, compat, features, manifest_path)?;
    }

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
mod cli_surface_tests {
    use super::*;
    use clap::Parser;

    /// A minimal parser wrapping [`RunArgs`], standing in for the real CLI so
    /// the flag surface can be exercised without spawning the binary.
    #[derive(Parser)]
    struct RunCli {
        #[command(flatten)]
        args: RunArgs,
    }

    /// Parses `argv` (with the command name prepended) or panics with the clap
    /// error, so a failed parse is diagnosed rather than reported as a bad field.
    fn parse(argv: &[&str]) -> RunArgs {
        let mut full = vec!["run"];
        full.extend_from_slice(argv);
        RunCli::try_parse_from(full)
            .unwrap_or_else(|err| panic!("`infs {}` must parse: {err}", argv.join(" ")))
            .args
    }

    /// The lib-dir flag is an *option*, not a second positional. Were the
    /// `short`/`long` attributes lost, `wasm_lib_dirs` would become positional #2
    /// and silently swallow the trailing-var-arg slot, so the source path and the
    /// (empty) program arguments are asserted alongside the directory.
    #[test]
    fn lib_dir_flag_is_an_option_not_a_second_positional() {
        let args = parse(&["f.inf", "-L", "libs"]);
        assert_eq!(args.path.as_deref(), Some(Path::new("f.inf")));
        assert_eq!(args.wasm_lib_dirs, [PathBuf::from("libs")]);
        assert!(args.args.is_empty());
    }

    /// The flag is positionally free relative to the source path, as any option
    /// is: it binds the same whether it precedes or follows the path.
    #[test]
    fn lib_dir_flag_may_precede_the_source_path() {
        let args = parse(&["-L", "libs", "f.inf"]);
        assert_eq!(args.path.as_deref(), Some(Path::new("f.inf")));
        assert_eq!(args.wasm_lib_dirs, [PathBuf::from("libs")]);
        assert!(args.args.is_empty());
    }

    /// Both spellings parse, repeat, mix, and preserve the order given. The
    /// order is contractual, not cosmetic: `infc` searches the directories in the
    /// order received and the first hit wins, so a parse that reordered them
    /// would change which `.wasm` a module resolves to.
    #[test]
    fn lib_dir_flag_accepts_both_spellings_and_preserves_order() {
        let args = parse(&[
            "-L",
            "first",
            "--wasm-lib-dir",
            "second",
            "-L",
            "third",
            "f.inf",
        ]);
        assert_eq!(
            args.wasm_lib_dirs,
            [
                PathBuf::from("first"),
                PathBuf::from("second"),
                PathBuf::from("third")
            ]
        );
        assert_eq!(args.path.as_deref(), Some(Path::new("f.inf")));
    }

    /// Project mode is the only mode `-L` can reach without a source path: any
    /// bare token would bind to `path` and select single-file mode instead, so a
    /// project-mode lib dir must arrive through the option's own value.
    #[test]
    fn lib_dir_flag_reaches_project_mode_without_a_source_path() {
        let args = parse(&["-L", "libs"]);
        assert!(args.path.is_none(), "no bare token means project mode");
        assert_eq!(args.wasm_lib_dirs, [PathBuf::from("libs")]);
        assert!(args.args.is_empty());
    }

    /// Options placed between the source path and the first bare token are still
    /// parsed as options; collection of the program's arguments starts at that
    /// bare token.
    #[test]
    fn options_before_the_first_bare_token_are_parsed() {
        let args = parse(&["f.inf", "--entry-point", "helper", "-L", "libs", "1"]);
        assert_eq!(args.path.as_deref(), Some(Path::new("f.inf")));
        assert_eq!(args.entry_point, "helper");
        assert_eq!(args.wasm_lib_dirs, [PathBuf::from("libs")]);
        assert_eq!(args.args, ["1"]);
    }

    /// The ordering contract, stated as the failure it produces: once a bare
    /// trailing token has been seen, everything after it — flags included — goes
    /// to the invoked function verbatim. This is not a parse bug to be fixed but
    /// the property that lets a program take arguments that look like `infs`
    /// flags; it is pinned so a future flag rearrangement cannot silently change
    /// which side of the boundary an argument lands on.
    #[test]
    fn a_lib_dir_after_the_first_bare_token_becomes_a_program_argument() {
        let args = parse(&["f.inf", "1", "-L", "libs"]);
        assert!(
            args.wasm_lib_dirs.is_empty(),
            "`-L` after a bare token is the program's, not the compiler's"
        );
        assert_eq!(args.args, ["1", "-L", "libs"]);
    }

    /// `--` is the explicit form of the same boundary, for a program whose first
    /// argument itself looks like a flag.
    #[test]
    fn a_double_dash_hands_flag_shaped_arguments_to_the_program() {
        let args = parse(&["f.inf", "--", "-L", "x"]);
        assert_eq!(args.path.as_deref(), Some(Path::new("f.inf")));
        assert!(args.wasm_lib_dirs.is_empty());
        assert_eq!(args.args, ["-L", "x"]);
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
            no_wasm_opt: false,
            wasm_lib_dirs: Vec::new(),
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
            no_wasm_opt: false,
            wasm_lib_dirs: Vec::new(),
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
