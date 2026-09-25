//! Run command for the infs CLI.
//!
//! Compiles Inference source and executes the resulting WASM in a single step.
//! Compilation is delegated to the `infc` compiler via subprocess; where the
//! artifact then runs is decided by the build's `[build] target`:
//!
//! - **`wasm32`** runs under the `wasmtime` CLI, which must be on PATH.
//! - **`spacewasm`** runs in process under the `SpaceWasm` flight interpreter,
//!   with the F´ (F Prime) reference hosts of its reference embedder and no
//!   `wasmtime` at all; see [`crate::commands::interpreter`].
//! - **`stellar`** runs nowhere locally: a contract is invoked by a Soroban
//!   host, and the command refuses it before anything is built.
//!
//! [`local_runtime`] is that decision, one exhaustive match, so a target added
//! to the vocabulary does not compile until it says where it runs.
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
//! `Inference.toml` and honors `[wasm-dependencies]`, `[build] target`,
//! `[build] wasm-features`, `[memory]` and `[host-imports]`, so running one
//! file of a project cannot execute a module built for a different runtime, at
//! a different WebAssembly instruction level or with a different memory layout
//! than `infs build` would produce for it, cannot fail to resolve an external
//! that the same build links, and cannot skip the host-import policy the same
//! build applies. This path overwrites the very artifact `infs build` produces,
//! so any divergence would be observable as one command destroying the other's
//! output.
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
//! infs run --fuel 1000000                     # spacewasm: stop after 10^6 instructions
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
//! - **Passes `main` no arguments**, so a `main` that takes any runs only in
//!   single-file mode, with its arguments after the path. That is a `main`
//!   declaring parameters, and also one returning a struct or array, which
//!   takes its result address as a hidden first parameter. Both routes read
//!   `main`'s parameters off the artifact before invoking it and refuse such a
//!   `main`, naming what it takes and the single-file command that passes it.
//!   Project mode is structurally arg-free: the first bare token on the command
//!   line binds to the positional `path` and therefore selects *single-file*
//!   mode, so trailing args cannot actually reach project mode through the CLI.
//!   The warning naming any that did is retained as a defensive,
//!   self-documenting guard should the argument layout ever change.
//! - **Gains the `infc` compatibility handshake** for free via the shared
//!   project-build helper. Single-file `run` keeps its prior no-handshake
//!   behavior except when the enclosing manifest asks for something an older
//!   compiler could not honor — a target, `wasm-features`, a `[memory]` table,
//!   or a `[host-imports]` table — where a capability probe is the only way to
//!   refuse the request rather than drop it. Neither `--wasm-dep` nor
//!   `--wasm-lib-dir` is capability-gated on either path: both arrived with
//!   external-module support itself rather than at a distinguishable ABI minor,
//!   so the handshake has nothing to check.
//! - **Resolves `[wasm-dependencies]`**, also via the shared helper, and
//!   forwards every `-L` it was passed: the project it runs is the one `infs
//!   build` would produce, externals included. A project binding `use { … } from
//!   <module>` is otherwise unrunnable, since `infc` resolves externals from
//!   forwarded flags only.
//! - **Always builds in compile mode**, regardless of the manifest's
//!   `[build] mode`. `run` executes the WASM, and proof-mode WASM embeds the
//!   custom non-deterministic opcodes (the `0xfc` family) that neither wasmtime
//!   nor the `SpaceWasm` interpreter can decode. So project `run` ignores
//!   `[build] mode` and `[verification] output-dir` entirely: the artifact is
//!   always an executable under `<root>/out/`. Use `infs build` to produce proof
//!   artifacts.
//! - **Applies `[build.wasm-opt]`** when the manifest declares it: `run` builds
//!   an executable in compile mode, so the same post-build optimization `build`
//!   performs runs here too (`run` executes exactly what it ships). Pass
//!   `--no-wasm-opt` to skip it.
//! - **Missing-WASM guard:** if the build reports success but
//!   `<root>/out/main.wasm` is absent, `run` errors before executing anything,
//!   mirroring the single-file `compile_to_wasm` guard.
//!
//! ## Host imports
//!
//! Both modes judge the imports of the artifact they built, after the build
//! and before anything runs. A `wasm32` build that imports any function is
//! refused: wasmtime runs it with no host functions registered (see
//! `refuse_an_artifact_that_imports_a_function`). A `spacewasm` build may import
//! the F´ reference hosts at their reference signatures and nothing else, which
//! the runner checks before it decodes the module.
//!
//! ## Prerequisites
//!
//! This command requires:
//! - `infc` compiler (via toolchain or PATH)
//! - for a `wasm32` build, the `wasmtime` WebAssembly runtime (in PATH); a
//!   `spacewasm` build runs in process and needs no runtime of its own

use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use std::ffi::OsString;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::artifact::{
    ArtifactScan, VERIFICATION_CONSTRUCTS_BELONG_IN_SPECS, exported_function_signature,
    scan_artifact,
};
use crate::commands::build::{
    EnclosingSettings, enclosing_manifest, format_wasm_dep_arg, manifest_host_imports,
    manifest_memory, manifest_target, manifest_wasm_dependencies, manifest_wasm_features,
};
use crate::commands::interpreter::{
    self, Arguments, Invocation, main_takes_arguments_in_project_mode,
};
use crate::commands::project_build::{
    forward_host_imports, forward_memory_layout, forward_target, forward_wasm_features,
    probe_compiler_compatibility, run_project_build,
};
use crate::errors::InfsError;
use crate::project::manifest::{InferenceToml, MANIFEST_FILE_NAME};
use crate::project::{self, ProjectContext};
use crate::toolchain::resolver::{ResolutionSource, find_infc_with_source};
use inference_compiler_interface::TargetName;
use inference_spacewasm_runner::{ExportedFunction, LIMITS_FROM, ValType, fprime};

/// The entry point invoked in project mode and the default for single-file mode.
const DEFAULT_ENTRY_POINT: &str = "main";

/// Arguments for the run command.
///
/// The run command compiles source to WASM and executes it: under wasmtime for
/// a `wasm32` build, in process under the `SpaceWasm` interpreter for a
/// `spacewasm` build.
///
/// [`RunArgs::args`] is a trailing var-arg, which sets the ordering contract for
/// the whole struct: options are consumed wherever they appear *before* the first
/// bare token that is not the source path, and everything from that token onward
/// is handed to the invoked function untouched. `infs run f.inf -L libs 1` passes
/// `libs` to the compiler and `1` to the program; `infs run f.inf 1 -L libs`
/// passes all three of `1`, `-L`, `libs` to the program.
// The field docs are `infs run --help` text, which prints a backtick as written,
// so the interpreter's name stays bare there rather than code-quoted.
#[allow(clippy::doc_markdown)]
#[derive(Args)]
pub struct RunArgs {
    /// Path to the source file to run.
    ///
    /// When omitted, `run` operates in project mode: it discovers the project's
    /// `Inference.toml` by walking up from the current directory, builds
    /// `<root>/src/main.inf`, and invokes `main`. Provide a path to run a
    /// single file directly.
    pub path: Option<PathBuf>,

    /// Function to invoke. Defaults to `main`.
    ///
    /// It must be exported: declared `pub` at the top level of the entry file.
    /// `main` is an ordinary entry point, receiving the arguments after the path
    /// as any other function does. Project mode invokes `main` only; to invoke
    /// another function, pass the entry file's path (`infs run src/main.inf
    /// --entry-point add 2 40`).
    #[clap(long, default_value = DEFAULT_ENTRY_POINT)]
    pub entry_point: String,

    /// Stop a `spacewasm` run after N interpreter instructions.
    ///
    /// The SpaceWasm interpreter counts the instructions of its own compiled form
    /// of the module, not WebAssembly instructions, so a budget depends on the
    /// interpreter version and is not comparable with wasmtime's fuel. A run that
    /// exhausts it stops and exits with status 1; without this flag a run has no
    /// budget. Refused, before anything is built, for a build that does not run
    /// under the SpaceWasm interpreter.
    ///
    /// Like every option, it goes before the first argument passed to the
    /// function: `infs run f.inf --fuel 100 5`, not `infs run f.inf 5 --fuel 100`.
    // Declared right after `entry_point`, the other option about the call rather
    // than the build, so `--help` lists the two together.
    #[clap(long, value_name = "N", value_parser = parse_fuel)]
    pub fuel: Option<NonZeroUsize>,

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

    /// Arguments passed to the invoked function, as decimal integers.
    ///
    /// Single-file mode only: without a path, the first bare token would be taken
    /// as the path. Under the SpaceWasm interpreter (`spacewasm`) each argument is
    /// converted to the WebAssembly type of the matching parameter, i32 or i64,
    /// accepting the signed and the unsigned range, and their count must match the
    /// function's parameters; under wasmtime (`wasm32`) they are handed to wasmtime
    /// as written. A narrower integer keeps its low bits, and a `bool` is true
    /// for any non-zero value. Those are the parameters of the compiled
    /// function, which differ from the source's in two ways: an array or struct
    /// parameter receives the number as an address, and a function returning an
    /// array or struct takes the address to write it to as a hidden first
    /// parameter and prints no value. Neither address is checked, so do not
    /// invoke such a function from the command line.
    ///
    /// Collection starts at the first bare token after the source path and takes
    /// everything from there, options included — so `infs run f.inf 1 -L libs`
    /// yields `["1", "-L", "libs"]` rather than parsing `-L`. Place every option
    /// before that token, or separate the program's arguments with `--`
    /// (`infs run f.inf -- -5` passes `-5`).
    #[clap(trailing_var_arg = true)]
    pub args: Vec<String>,
}

/// Reads `--fuel`'s value: a positive number of instructions.
///
/// A budget of zero would stop the run before its first instruction, which no
/// one asks for on purpose, and "no budget" is spelled by leaving the flag out,
/// so zero is refused with that spelling rather than read as either.
fn parse_fuel(text: &str) -> Result<NonZeroUsize, String> {
    let budget: usize = text.parse().map_err(|error| format!("{error}"))?;
    NonZeroUsize::new(budget)
        .ok_or_else(|| "`--fuel` must be at least 1; omit it to run without a budget".to_string())
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
/// Propagates errors from the selected mode (missing file, a target no local
/// runtime runs, `--fuel` outside the interpreter, missing wasmtime, compiler
/// lookup, compilation failure, a refused artifact, a failed or trapped run,
/// or — in project mode — discovery, entry-point resolution, and
/// `--entry-point` rejection).
pub fn execute(args: &RunArgs) -> Result<()> {
    if let Some(path) = &args.path {
        return execute_single_file(path, args);
    }

    execute_project(args)
}

/// Where `infs run` executes a build, decided by the build's target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalRuntime {
    /// The `wasmtime` CLI, found on PATH, with no host function registered.
    Wasmtime,
    /// The `SpaceWasm` flight interpreter, in process, with the F´ reference
    /// hosts registered.
    SpaceWasmInterpreter,
    /// Nowhere: the build is refused before anything is compiled.
    Unavailable,
}

/// The runtime a `target` build runs under.
///
/// An exhaustive match with no wildcard arm, so a name added to the target
/// vocabulary is a compile error here until it says where it runs, rather than
/// inheriting a runtime nobody chose for it. The question has no counterpart in
/// the compiler: which runtime an artifact is *executed* under is this
/// command's business, not the build's.
///
/// - `wasm32` is generic WebAssembly, and `wasmtime` is a generic runtime.
/// - `spacewasm` builds the `wasm32` bytes for the `SpaceWasm` flight interpreter,
///   so it runs under that interpreter itself, embedded, rather than under a
///   stand-in.
/// - `stellar` builds a Soroban contract, whose exports take and return the
///   host's 64-bit tagged word: only a Soroban host invokes one correctly, and
///   any runtime at hand would pass a decimal and return a wrong answer.
fn local_runtime(target: TargetName) -> LocalRuntime {
    match target {
        TargetName::Wasm32 => LocalRuntime::Wasmtime,
        TargetName::SpaceWasm => LocalRuntime::SpaceWasmInterpreter,
        TargetName::Stellar => LocalRuntime::Unavailable,
    }
}

/// Where the source `infs run` builds sits, as the refusals that suggest a
/// manifest edit have to know.
#[derive(Debug, Clone, Copy)]
enum Scope<'a> {
    /// In a project, discovered or enclosing the file: its `Inference.toml`
    /// decides the target, and editing it changes the build.
    Project {
        /// The project's manifest, whose other `[build]` keys a suggested
        /// target may refuse.
        manifest: &'a InferenceToml,
    },
    /// A file outside any project, which builds at the default target and has
    /// no manifest to edit.
    NoProject {
        /// The file, as the command line named it.
        file: &'a Path,
    },
}

/// Runs a single explicit source file (single-file mode).
///
/// ## Execution Flow
///
/// 1. Validates source file exists
/// 2. Resolves the enclosing project's `[build] target`, and with it the
///    runtime the artifact runs under; refuses a target no local runtime runs
/// 3. Under wasmtime only: refuses `--fuel`, then checks for wasmtime
/// 4. Resolves the rest of the enclosing project's settings — `[build]
///    wasm-features`, `[memory]`, `[host-imports]`, `[wasm-dependencies]` — if
///    any
/// 5. Locates the infc compiler
/// 6. Compiles source to WASM via infc subprocess, forwarding those settings
///    alongside every `-L` the user passed
/// 7. Under wasmtime: refuses the artifact if it imports a function, executes
///    it invoking `--entry-point`, and propagates wasmtime's exit code. Under
///    the `SpaceWasm` interpreter: loads it against the F´ reference hosts and
///    invokes `--entry-point` in process
///
/// The enclosing manifest is honored here for the same reason `infs build
/// <path>` honors it: one project must not emit modules at two different
/// WebAssembly instruction levels, or bind one module name to two different
/// `.wasm` files, depending on how the build was invoked — and this path
/// overwrites the very artifact `infs build` produces.
///
/// Every manifest-derived setting comes off the single [`enclosing_manifest`]
/// call above, and that resolution happens *before* the compiler lookup so a
/// malformed manifest is reported without first probing the toolchain. The
/// target read off it is ordered ahead of everything else that can refuse,
/// because each of those refusals depends on where the build would run: a
/// Stellar build is refused before a user who lacks wasmtime is sent to install
/// it, `--fuel` is refused before a build it could not apply to is spent, and a
/// `spacewasm` build never looks for wasmtime at all. The import refusal cannot
/// be ordered that way, because it reads the artifact, and deliberately stays
/// behind the probe rather than pulling the probe behind the build — why is
/// `refuse_an_artifact_that_imports_a_function`'s to say.
///
/// ## Errors
///
/// Returns an error if:
/// - The source file does not exist
/// - the enclosing manifest names a target no local runtime runs
/// - `--fuel` is given for a build that does not run under the interpreter
/// - wasmtime is not found in PATH, for a build that runs under it
/// - a `[wasm-dependencies]` key is not a well-formed logical module name or its
///   first segment is the reserved `host`, or a resolved dependency path is not
///   valid UTF-8
/// - infc compiler cannot be found
/// - the enclosing manifest names a `target`, requests `wasm-features`, or
///   declares a `[memory]` or `[host-imports]` table the resolved `infc` cannot
///   honor (which are also the only cases that run the ABI handshake here)
/// - Compilation fails
/// - the artifact carries a verification construct, or imports what its
///   runtime does not provide: any function under wasmtime, anything but the
///   F´ reference hosts at their signatures under the interpreter
/// - WASM execution fails, or the interpreter refuses the call or ends it by a
///   trap or an exhausted `--fuel` budget
fn execute_single_file(path: &Path, args: &RunArgs) -> Result<()> {
    if !path.exists() {
        bail!("Path not found: {}", path.display());
    }

    let enclosing = enclosing_manifest(path)?;
    let target = manifest_target(enclosing.as_ref().map(|(_, manifest)| manifest))?;
    let scope = match &enclosing {
        Some((_, manifest)) => Scope::Project { manifest },
        None => Scope::NoProject { file: path },
    };
    match local_runtime(target) {
        LocalRuntime::Unavailable => Err(no_local_runtime(target)),
        LocalRuntime::Wasmtime => {
            if args.fuel.is_some() {
                return Err(fuel_outside_the_interpreter(target, scope));
            }
            check_wasmtime_availability()?;
            let wasm_path = compile_single_file(path, args, enclosing.as_ref(), target)?;
            let wasm = read_artifact(&wasm_path, &wasm_path)?;
            refuse_an_artifact_that_imports_a_function(&wasm, &wasm_path, target, scope)?;
            run_wasmtime(&wasm_path, &args.entry_point, &args.args)
        }
        LocalRuntime::SpaceWasmInterpreter => {
            let wasm_path = compile_single_file(path, args, enclosing.as_ref(), target)?;
            let wasm = read_artifact(&wasm_path, &wasm_path)?;
            // The runner judges the imports itself, before it decodes a byte;
            // the scan is asked only whether a verification construct leaked.
            executable_function_imports(&wasm, &wasm_path, INTERPRETER)?;
            interpreter::run(&Invocation {
                wasm: &wasm,
                shown_as: &wasm_path,
                entry_point: &args.entry_point,
                arguments: Arguments::Given(&args.args),
                fuel: args.fuel,
            })
        }
    }
}

/// Compiles the single source file `path` with the settings of the manifest
/// enclosing it, if any, and answers where the artifact landed.
///
/// # Errors
///
/// A setting the manifest holds that cannot be resolved or forwarded, a
/// missing `infc`, and every failure of [`compile_to_wasm`].
fn compile_single_file(
    path: &Path,
    args: &RunArgs,
    enclosing: Option<&(PathBuf, InferenceToml)>,
    target: TargetName,
) -> Result<PathBuf> {
    let manifest = enclosing.map(|(_, manifest)| manifest);
    let features = manifest_wasm_features(manifest)?;
    let memory = manifest_memory(manifest);
    let host_imports = manifest_host_imports(manifest);
    let deps = manifest_wasm_dependencies(enclosing)?;
    let manifest_path = enclosing.map(|(dir, _)| dir.join(MANIFEST_FILE_NAME));

    let (infc_path, infc_source) = find_infc_with_source()?;

    compile_to_wasm(
        &infc_path,
        infc_source,
        path,
        &args.wasm_lib_dirs,
        &EnclosingSettings {
            deps: &deps,
            target,
            features: &features,
            memory: &memory,
            host_imports,
            manifest_path: manifest_path.as_deref(),
        },
    )
}

/// Builds and runs a discovered project (project mode).
///
/// Resolves the project from the current directory, performs the shared project
/// build (which runs the `infc` compatibility handshake and forwards the
/// `-L` directories given here), then invokes `main` on `<root>/out/main.wasm`
/// with no arguments: via wasmtime for a `wasm32` build, in process under the
/// `SpaceWasm` interpreter for a `spacewasm` one. Project mode always invokes
/// `main`; a non-`main` `--entry-point` is rejected, and so is a `main` that
/// takes arguments, with the single-file command that passes them. Trailing
/// var-args cannot reach this path (the first token binds to `path`); the
/// warning is a defensive guard documenting the ignore-args policy. `-L` is the
/// one flag that *can* reach here, since it takes its own value rather than a
/// bare token.
///
/// The project is discovered and its target resolved before anything else can
/// refuse, for the reasons [`execute_single_file`] gives: a target this command
/// cannot run is not a missing-runtime problem, `--fuel` is refused before a
/// build it cannot apply to, and only a `wasm32` build looks for wasmtime —
/// before any compilation, so an environment lacking the runtime fails fast
/// without first spending a build, and so ahead of the import refusal, which
/// only a finished artifact can answer. Why that one refusal is left behind the
/// probe is `refuse_an_artifact_that_imports_a_function`'s to say.
///
/// ## Errors
///
/// Returns an error if:
/// - `--entry-point` is set to a non-`main` value (project mode invokes `main`)
/// - No `Inference.toml` is found in the current directory or any ancestor
/// - The project names a target no local runtime runs
/// - `--fuel` is given for a build that does not run under the interpreter
/// - wasmtime is not found in PATH, for a build that runs under it
/// - The project build fails (missing entry point, ABI handshake,
///   external-module forwarding, infc error)
/// - The build succeeds but `<root>/out/main.wasm` is absent
/// - the artifact carries a verification construct, or imports what its
///   runtime does not provide
/// - `main` takes arguments, which project mode does not pass
/// - WASM execution fails, or the interpreter refuses the call or ends it by a
///   trap or an exhausted `--fuel` budget
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
        eprintln!("{}", ignored_project_arguments_warning(&args.args));
    }

    let cwd =
        std::env::current_dir().context("Failed to determine the current working directory")?;
    let ctx = project::discover_and_load(&cwd)?;
    let target = ctx.manifest.build.resolved_target()?;
    let shown_as = Path::new("out").join("main.wasm");
    let entry_file = entry_file_as_typed(&ctx, &cwd);
    let scope = Scope::Project {
        manifest: &ctx.manifest,
    };
    match local_runtime(target) {
        LocalRuntime::Unavailable => Err(no_local_runtime(target)),
        LocalRuntime::Wasmtime => {
            if args.fuel.is_some() {
                return Err(fuel_outside_the_interpreter(target, scope));
            }
            check_wasmtime_availability()?;
            let wasm_path = build_project(&ctx, args)?;
            let wasm = read_artifact(&wasm_path, &shown_as)?;
            refuse_an_artifact_that_imports_a_function(&wasm, &shown_as, target, scope)?;
            refuse_a_main_that_takes_arguments(&wasm, &shown_as, &entry_file)?;
            run_wasmtime(&wasm_path, DEFAULT_ENTRY_POINT, &[])
        }
        LocalRuntime::SpaceWasmInterpreter => {
            let wasm_path = build_project(&ctx, args)?;
            let wasm = read_artifact(&wasm_path, &shown_as)?;
            // As in single-file mode: the imports are the runner's to judge.
            executable_function_imports(&wasm, &shown_as, INTERPRETER)?;
            interpreter::run(&Invocation {
                wasm: &wasm,
                shown_as: &shown_as,
                entry_point: DEFAULT_ENTRY_POINT,
                arguments: Arguments::Project {
                    entry_file: &entry_file,
                },
                fuel: args.fuel,
            })
        }
    }
}

/// Builds the project for `run` and answers where its artifact landed.
///
/// Project `run` always builds an executable (compile mode) in `out/`,
/// regardless of `[build] mode` in the manifest: proof-mode WASM embeds the
/// custom non-deterministic opcodes (0xfc family) that no runtime `run` uses can
/// decode. Hence `mode = None` and `out_dir = None` here — manifest
/// mode/output-dir resolution lives only in `build`'s project path. The lib
/// dirs pass straight through; the helper anchors them to the invocation
/// directory because it moves `infc` to the project root. The
/// `[build.wasm-opt]` optimization still applies (unless `--no-wasm-opt`) so
/// `run` executes exactly what `build` would ship.
///
/// # Errors
///
/// Every failure of [`run_project_build`], and a build that reports success
/// without writing `<root>/out/main.wasm`.
fn build_project(ctx: &ProjectContext, args: &RunArgs) -> Result<PathBuf> {
    run_project_build(
        ctx,
        false,
        None,
        None,
        &args.wasm_lib_dirs,
        args.no_wasm_opt,
    )?;

    let wasm_path = project_wasm_path(ctx);
    if !wasm_path.exists() {
        bail!(
            "Compilation succeeded but WASM file not found at: {}",
            wasm_path.display()
        );
    }
    Ok(wasm_path)
}

/// The project's entry file as the reader would type it from `cwd`: relative
/// to it when the file is beneath it, `src/main.inf` from the root, and the
/// absolute path otherwise.
///
/// The project root is found from the canonical form of the working directory,
/// which need not be the form the operating system reports it in — a symlinked
/// temporary directory on macOS, a verbatim `\\?\` path on Windows — so the
/// entry file is sought beneath both.
fn entry_file_as_typed(ctx: &ProjectContext, cwd: &Path) -> PathBuf {
    let canonical = cwd.canonicalize().ok();
    std::iter::once(cwd)
        .chain(canonical.as_deref())
        .find_map(|base| ctx.entry_point.strip_prefix(base).ok())
        .map_or_else(|| ctx.entry_point.clone(), Path::to_path_buf)
}

/// The warning project mode prints for trailing arguments: it passes `main`
/// none, so it names each one it drops, space-separated in the order given.
fn ignored_project_arguments_warning(args: &[String]) -> String {
    format!(
        "warning: project mode passes no arguments to `main`; these are ignored: {}",
        args.join(" ")
    )
}

/// The conventional project output path: `<root>/out/main.wasm`.
///
/// `out/` is `infc`'s default output directory and the build spawns `infc` with
/// its working directory set to the project root, so the WASM lands here. Built
/// with [`Path::join`] so the separator is platform-correct (never a literal `/`).
fn project_wasm_path(ctx: &ProjectContext) -> PathBuf {
    ctx.root.join("out").join("main.wasm")
}

/// The runtime a verification-construct refusal names under wasmtime.
const WASMTIME: &str = "wasmtime";

/// The runtime a verification-construct refusal names under the interpreter.
const INTERPRETER: &str = "the SpaceWasm interpreter";

/// The refusal of a target no local runtime runs, before anything is built.
///
/// That is a Stellar contract, whose exports are not something a runtime that
/// invokes an export by name and passes each argument as a decimal can
/// meaningfully call. Every method takes and returns the host's 64-bit tagged
/// word, so a `5` typed on the command line arrives as a word whose low byte is
/// read as the tag and whose payload is empty — decoding, silently, to a
/// zero-valued integer rather than to five. `main` is no exception, since its
/// value-ABI wrapper's parameters are tagged words like every other method's,
/// and neither is its result: wasmtime prints the returned word itself, so a
/// `main` that takes nothing and returns 42 prints 180388626437, the word
/// carrying 42 in its upper half and the tag in its low byte.
///
/// Both outcomes are wrong answers rather than missing features, which is why
/// this is a refusal and not a warning. Nothing here can be fixed by passing
/// different arguments: a contract is invoked by a Soroban host, which encodes
/// its arguments into that word, and by nothing else.
///
/// The message explains the tagged word because Stellar is the only target
/// [`local_runtime`] answers [`LocalRuntime::Unavailable`] for, and a second
/// such target would be describing a different convention entirely. It renders
/// the refused target from the argument, so the name is right the moment one
/// arrives; the wording around it is what a second such arm has to bring.
///
/// Both call sites return this *before* anything else can refuse, `wasmtime`'s
/// probe included. The refusal is about what would be built, not about what is
/// installed, so a machine without the runtime must hear the refusal rather than
/// an install prompt for a tool that would change nothing.
fn no_local_runtime(target: TargetName) -> anyhow::Error {
    anyhow!(
        "`infs run` cannot run a `{}` build. A Stellar contract is invoked by a \
         Soroban host, which encodes each argument into the host's 64-bit tagged \
         word; a plain WebAssembly runtime such as wasmtime passes a decimal \
         instead, which the contract decodes as a different value entirely — a \
         wrong answer rather than an error. Build it with `infs build` and \
         deploy it, or run the same source at the `{}` target to execute it \
         locally. See the book's Compilation Targets chapter.",
        target.as_str(),
        TargetName::DEFAULT.as_str(),
    )
}

/// The refusal of `--fuel` for a build that runs under wasmtime, which `infs
/// run` executes with no instruction budget at all.
///
/// Both call sites return it when `--fuel` is set, right after the target is
/// resolved and before `wasmtime` is looked for or anything is built: whether
/// the flag applies depends on the target alone, and a build spent on a run
/// that was going to be refused is a build wasted. A `stellar` build never
/// reaches this, since it is refused first, so the message is about the one
/// runtime that does. For a file outside any project, the remedy is only to
/// drop the flag: there is no manifest to name a target in.
fn fuel_outside_the_interpreter(target: TargetName, scope: Scope<'_>) -> anyhow::Error {
    let spacewasm = TargetName::SpaceWasm.as_str();
    match scope {
        Scope::Project { .. } => anyhow!(
            "`--fuel` applies only to a `{spacewasm}` build, and this project builds at the \
             `{}` target, which `infs run` executes under wasmtime without an instruction \
             budget. Remove `--fuel`, or set `target = \"{spacewasm}\"` under `[build]` in \
             {MANIFEST_FILE_NAME} to run the program under the SpaceWasm interpreter, which \
             counts its instructions.",
            target.as_str()
        ),
        Scope::NoProject { file } => anyhow!(
            "`--fuel` applies only to a `{spacewasm}` build, and {} is outside any project, so \
             it builds at the default `{}` target, which `infs run` executes under wasmtime \
             without an instruction budget. Remove `--fuel`.",
            file.display(),
            TargetName::DEFAULT.as_str()
        ),
    }
}

/// The bytes of the artifact the build wrote at `wasm_path`, which the
/// messages name `shown_as`.
fn read_artifact(wasm_path: &Path, shown_as: &Path) -> Result<Vec<u8>> {
    std::fs::read(wasm_path).with_context(|| format!("Failed to read {}", shown_as.display()))
}

/// The functions `wasm` imports, as `(module, field)` pairs in import order,
/// or the refusal of a verification-only construct the scan found in it, which
/// `runtime` could not decode.
///
/// The scan answers the verification-construct question first, so an artifact
/// carrying one is refused on either route, in the words `[build.wasm-opt]`
/// uses for the same find. It is a backstop, as the optimizer's is: A042
/// rejects a non-deterministic block outside a `spec` (and A006 an `@` outside
/// such a block), and compile-mode builds strip `spec` blocks, so a well-formed
/// build never reaches it — but an artifact that did would otherwise be handed
/// to a runtime to fail on an opcode it cannot decode.
///
/// # Errors
///
/// The refusal of a verification construct, and an error when the artifact
/// cannot be parsed.
fn executable_function_imports(
    wasm: &[u8],
    shown_as: &Path,
    runtime: &str,
) -> Result<Vec<(String, String)>> {
    match scan_artifact(wasm, shown_as)? {
        ArtifactScan::VerificationConstruct(construct) => bail!(
            "`infs run` cannot execute this program: {} contains the verification-only \
             construct `{construct}`, which {runtime} cannot decode. \
             {VERIFICATION_CONSTRUCTS_BELONG_IN_SPECS}; move the construct into a `spec` block.",
            shown_as.display()
        ),
        ArtifactScan::Executable {
            function_imports, ..
        } => Ok(function_imports),
    }
}

/// Refuses a `wasm32` artifact that imports a function: wasmtime runs it with
/// no host function registered, so a program with host imports runs under the
/// embedder that supplies them, or — when every import is an F´ reference host
/// at its reference signature — as a `spacewasm` build, which `infs run`
/// provides those hosts to.
///
/// This is a policy, not a runtime fact. The `wasmtime` CLI registers none of a
/// program's host modules — an `env.clock_ms` import fails to instantiate
/// there, in wasmtime's words, which name an unknown import and read as a broken
/// build — but it does link WASI on its own, so an artifact whose only imports
/// are WASI functions would instantiate. It is refused all the same. A host
/// import is a promise a particular embedder keeps, and which of those promises
/// wasmtime happens to keep is a property of the runtime rather than of the
/// program; executing against a stand-in would make whether a program runs
/// depend on which functions the stand-in provides.
///
/// Which remedy the refusal offers is asked of the runner's own import check,
/// on the same bytes the `SpaceWasm` route would load: a program whose imports
/// the reference hosts all provide as declared is told to build for the target
/// that provides them — through `infs init` first when the file has no project
/// to name a target in, and removing any `[build]` key the project's manifest
/// sets that the target refuses — and any other is told only where it does
/// run.
///
/// Decided from the artifact, which is why it follows the build rather than
/// preceding it as [`no_local_runtime`] does. Whether a program binds a host
/// import is a property of its source, and a manifest cannot answer it:
/// `[host-imports]` is an allowlist, not a declaration — a project may list
/// functions it never binds, and bind them with no table at all. It is asked of
/// the bytes that will run, too, which in project mode are the bytes
/// `[build.wasm-opt]` left behind: the optimizer may remove an import nothing
/// calls, and a program whose only host import is uncalled then runs where its
/// unoptimized build would be refused.
///
/// Following the build also puts it behind the wasmtime probe, which both call
/// sites run before they build, so a machine without wasmtime is told to install
/// it before it can hear this refusal — the one install prompt the target
/// refusal's ordering exists to avoid. The order is kept on purpose. The probe
/// could only move behind the build, and there every run on such a machine, of
/// any program, would spend a compile before learning the runtime is missing,
/// to spare the programs that bind a host import one prompt.
///
/// `shown_as` is the conventional relative spelling the message names the
/// artifact by, as the other `run` messages do; `target` is the one the build
/// was made for, and `scope` whether a manifest can be edited to change it —
/// and, when one can, the manifest itself.
///
/// # Errors
///
/// Returns the refusal when the artifact imports any function, a refusal naming
/// the construct when it carries a verification-only one, and an error when it
/// cannot be parsed.
fn refuse_an_artifact_that_imports_a_function(
    wasm: &[u8],
    shown_as: &Path,
    target: TargetName,
    scope: Scope<'_>,
) -> Result<()> {
    let mut imports = executable_function_imports(wasm, shown_as, WASMTIME)?;
    if imports.is_empty() {
        return Ok(());
    }
    imports.sort();
    imports.dedup();
    let listed = imports
        .iter()
        .map(|(module, field)| format!("  {module}.{field}"))
        .collect::<Vec<_>>()
        .join("\n");
    let count = imports.len();
    let (functions, these_functions, them) = if count == 1 {
        ("function", "this function", "it")
    } else {
        ("functions", "these functions", "them")
    };
    let target = target.as_str();
    let spacewasm = TargetName::SpaceWasm.as_str();
    let book = "See the book's External Functions and WASM Linking chapter (\"Running a program \
                that binds host imports\").";
    let header = format!(
        "`infs run` cannot execute this program at the `{target}` target: {} imports {count} \
         {functions} that its embedder must supply.\n{listed}\n",
        shown_as.display()
    );
    if fprime::check_imports(wasm).is_err() {
        let unmatched = if count == 1 {
            "which this program's import does not match"
        } else {
            "which this program's imports do not all match"
        };
        bail!(
            "{header}A `{target}` build runs under wasmtime, and `infs run` registers no host \
             functions there — not even the WASI functions the wasmtime CLI provides on its own \
             — so it executes no `{target}` artifact that imports a function. `infs run` \
             provides host functions only to a `{spacewasm}` build, and only the F Prime \
             reference set at its reference signatures, {unmatched}. Run the program from the \
             embedder that supplies {these_functions}. {book}"
        );
    }
    let reference_hosts = match imports.as_slice() {
        [(module, field)] => format!("`{module}.{field}` is an F Prime reference host"),
        [_, _] => "Both are F Prime reference hosts".to_string(),
        _ => format!("All {} are F Prime reference hosts", number_word(count)),
    };
    let remedy = match scope {
        Scope::Project { manifest } => {
            let removed = match build_keys_a_spacewasm_build_refuses(manifest).as_slice() {
                [] => String::new(),
                keys => format!(", remove {}, which that target refuses,", keys.join(" and ")),
            };
            format!(
                ": set `target = \"{spacewasm}\"` under `[build]` in {MANIFEST_FILE_NAME}{removed} \
                 and run it again to execute the program under the SpaceWasm interpreter."
            )
        }
        Scope::NoProject { .. } => format!(
            ". This file is not inside a project, so it builds at the default `{}` target: run \
             `infs init` in its directory, set `target = \"{spacewasm}\"` under `[build]` in the \
             {MANIFEST_FILE_NAME} it writes, and run it again.",
            TargetName::DEFAULT.as_str()
        ),
    };
    bail!(
        "{header}A `{target}` build runs under wasmtime, where `infs run` registers no host \
         functions. {reference_hosts}, which `infs run` provides to a `{spacewasm}` \
         build{remedy} Otherwise, run it from the embedder that supplies {them}. {book}"
    )
}

/// The `[build]` keys `manifest` sets that the `spacewasm` target refuses —
/// `mode = "proof"` and a non-empty `wasm-features` list — spelled as the
/// remedy that suggests the target names them for removal.
///
/// Asked of the predicates on [`TargetName`] that the manifest's own pairing
/// check asks, so a remedy cannot promise a run that loading the edited
/// manifest would refuse.
fn build_keys_a_spacewasm_build_refuses(manifest: &InferenceToml) -> Vec<&'static str> {
    let spacewasm = TargetName::SpaceWasm;
    let mut keys = Vec::new();
    if manifest.build.mode == "proof" && !spacewasm.supports_proof_mode() {
        keys.push("`mode = \"proof\"`");
    }
    if !manifest.build.wasm_features.is_empty() && !spacewasm.permits_bulk_memory() {
        keys.push("`wasm-features`");
    }
    keys
}

/// `count` spelled as a word, for the counts of reference hosts a program can
/// import — three to six — and as digits past them.
fn number_word(count: usize) -> String {
    match count {
        3 => "three".to_string(),
        4 => "four".to_string(),
        5 => "five".to_string(),
        6 => "six".to_string(),
        count => count.to_string(),
    }
}

/// Refuses, before wasmtime is invoked, a `main` that takes arguments, which
/// project mode does not pass.
///
/// wasmtime would refuse the call itself, after `Invoking 'main'` is printed,
/// in its own words about a missing argument; this says what `main` takes and
/// the single-file command that passes it, in the words the `SpaceWasm` route
/// uses for the same `main`. A `main` returning a struct or an array is one of
/// these: it takes the address to write its result to as a hidden parameter.
///
/// A `main` the artifact does not export, or one with a parameter outside the
/// four numeric types, is left to wasmtime, whose own error names it.
///
/// # Errors
///
/// The refusal, and an error when the artifact cannot be parsed.
fn refuse_a_main_that_takes_arguments(
    wasm: &[u8],
    shown_as: &Path,
    entry_file: &Path,
) -> Result<()> {
    let Some(ty) = exported_function_signature(wasm, DEFAULT_ENTRY_POINT, shown_as)? else {
        return Ok(());
    };
    match as_exported_function(DEFAULT_ENTRY_POINT, &ty) {
        Some(main) if !main.params.is_empty() => {
            bail!(main_takes_arguments_in_project_mode(&main, entry_file))
        }
        Some(_) | None => Ok(()),
    }
}

/// The function `name` of type `ty` as the runner describes an export, so the
/// two routes word a refusal of its arguments alike; `None` when a parameter or
/// the result is not one of the four numeric types, or there is more than one
/// result, none of which an Inference artifact declares.
fn as_exported_function(name: &str, ty: &inf_wasmparser::FuncType) -> Option<ExportedFunction> {
    let numeric = |ty: &inf_wasmparser::ValType| match ty {
        inf_wasmparser::ValType::I32 => Some(ValType::I32),
        inf_wasmparser::ValType::I64 => Some(ValType::I64),
        inf_wasmparser::ValType::F32 => Some(ValType::F32),
        inf_wasmparser::ValType::F64 => Some(ValType::F64),
        inf_wasmparser::ValType::V128 | inf_wasmparser::ValType::Ref(_) => None,
    };
    let params = ty.params().iter().map(numeric).collect::<Option<Vec<_>>>()?;
    let result = match ty.results() {
        [] => None,
        [result] => Some(numeric(result)?),
        _ => return None,
    };
    Some(ExportedFunction {
        name: name.to_string(),
        params,
        result,
    })
}

/// The long help of `infs run`: how each target's build runs, with the F´
/// reference hosts listed from the runner's table and the interpreter release
/// read from the limits it was built against, so neither is restated here.
pub(crate) fn long_about() -> String {
    format!(
        "Build and run an Inference program.

With a path, compiles that source file and invokes `--entry-point`
(default `main`), passing it the arguments after the path. With no path,
runs in project mode: discovers `Inference.toml`, builds
`<root>/out/main.wasm`, and invokes `main` with no arguments.

The build's `[build] target` decides where it runs. A `wasm32` build runs
under the `wasmtime` CLI, which must be on PATH, and may import no function.
A `spacewasm` build runs in process under the SpaceWasm flight interpreter
({LIMITS_FROM}) and needs no wasmtime; it may import these F Prime
reference hosts and no others, each of which but `env.clock_ms` logs its
calls to stderr:

{}

A `stellar` build cannot be run locally. The return value is printed on
stdout, and the exit status is 0 whenever the call returns. A refused
import exits with status 1, and so does a trap or an exhausted `--fuel`
budget under the interpreter; a trap under wasmtime exits with wasmtime's
own status.",
        fprime::reference_table_lines().join("\n")
    )
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
/// <source> --parse --codegen -o
///     [--wasm-lib-dir <dir>]* [--wasm-dep <name>=<path>]*
///     [--target <name>] [--wasm-features <list>]
///     [--memory-pages <n>] [--stack-size <n>] [--host-imports=<list>]
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
/// The compatibility handshake runs only when the manifest asks the compiler for
/// something an older one could refuse: a non-default target, a feature request,
/// a `[memory]` table, or a `[host-imports]` table. Single-file `run` otherwise
/// keeps its historical handshake-free behavior: the probe exists to refuse an
/// unhonorable request, and paying for it on every run would add ABI warnings to
/// invocations that ask nothing of the compiler.
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
    settings: &EnclosingSettings<'_>,
) -> Result<PathBuf> {
    let EnclosingSettings {
        deps,
        target,
        features,
        memory,
        host_imports,
        manifest_path,
    } = *settings;

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

    // Each disjunct is one thing this build asks of `infc` that an older one
    // could not honor. The target belongs here for the same reason the other two
    // do, and it is the disjunct a reader is most likely to leave out: without
    // it a project naming a target but declaring neither a feature nor a
    // `[memory]` table would skip the whole block, and the target would be
    // dropped silently — producing an artifact for the default runtime under a
    // manifest that named another. A `[host-imports]` table is one thing more,
    // tested for presence rather than content: a content test would skip this
    // block for the declared-empty table, the strictest policy there is, and
    // the build would run under no policy at all.
    if !features.is_empty()
        || !memory.is_default()
        || target != TargetName::DEFAULT
        || host_imports.is_some()
    {
        let compat = probe_compiler_compatibility(infc_path, infc_source)?;
        forward_target(&mut cmd, compat, target, manifest_path)?;
        forward_wasm_features(&mut cmd, compat, features, manifest_path)?;
        forward_memory_layout(&mut cmd, compat, memory, manifest_path)?;
        forward_host_imports(&mut cmd, compat, host_imports, manifest_path)?;
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
/// Calls `entry_point` with `args`, on the command line [`wasmtime_argv`]
/// builds.
///
/// Stderr is captured and only displayed if wasmtime fails, to suppress
/// the experimental feature warnings about `--invoke` that appear on success.
///
/// Returns `Ok(())` on success, or `Err(InfsError::ProcessExitCode)` if wasmtime
/// exits with a non-zero code. This allows the caller to propagate the exit code
/// without bypassing RAII cleanup.
fn run_wasmtime(wasm_path: &Path, entry_point: &str, args: &[String]) -> Result<()> {
    println!("Invoking '{entry_point}' with wasmtime...");

    let output = Command::new("wasmtime")
        .args(wasmtime_argv(wasm_path, entry_point, args))
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

/// The arguments `run` hands the `wasmtime` CLI: `--invoke <entry_point>
/// <wasm_path>`, then `args` exactly as the user wrote them.
///
/// `main` is not special-cased: it takes the trailing arguments as any other
/// export does, and wasmtime parses each into the type of the matching
/// parameter of the compiled function. Code generation lowers `main` like every
/// other function, so those are the parameters its source declares, except that
/// a struct or array parameter is an `i32` address and a struct or array result
/// is written through an address taken as a hidden first parameter:
/// `pub fn main() -> i32` is `(func (result i32))`, while `pub fn main() ->
/// [i32; 4]` is `(func (param i32))`. Neither difference is detected here; the
/// argument for an address parameter is the user's to write like any other.
/// Nothing is rewritten or dropped, an argument shaped like a flag included:
/// wasmtime reads everything after the module path as the invoked function's
/// arguments.
fn wasmtime_argv(wasm_path: &Path, entry_point: &str, args: &[String]) -> Vec<OsString> {
    let mut command_line = vec![
        OsString::from("--invoke"),
        OsString::from(entry_point),
        OsString::from(wasm_path),
    ];
    command_line.extend(args.iter().map(OsString::from));
    command_line
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

    /// The `N` in `--fuel N`, as a budget.
    fn budget(n: usize) -> NonZeroUsize {
        NonZeroUsize::new(n).expect("a test budget is positive")
    }

    /// A command line, the path it names, the budget it sets, and the
    /// arguments it passes the function.
    type FuelRow = (
        &'static [&'static str],
        Option<&'static str>,
        Option<NonZeroUsize>,
        &'static [&'static str],
    );

    /// `--fuel` is an option wherever an option may stand — before the path,
    /// between the path and the first bare token, in project mode with no path
    /// at all — and absent it is no budget, not a budget of zero.
    ///
    /// Fails if the flag loses `long`, which would turn it into a positional
    /// that swallows the path or the program's first argument.
    #[test]
    fn fuel_is_an_option_before_the_first_bare_token() {
        let rows: [FuelRow; 5] = [
            (&["--fuel", "7", "f.inf"], Some("f.inf"), Some(budget(7)), &[]),
            (&["f.inf", "--fuel", "7", "1", "2"], Some("f.inf"), Some(budget(7)), &["1", "2"]),
            (&["f.inf", "--fuel=1"], Some("f.inf"), Some(budget(1)), &[]),
            (&["--fuel", "18446744073709551615"], None, Some(budget(usize::MAX)), &[]),
            (&["f.inf", "1"], Some("f.inf"), None, &["1"]),
        ];
        for (argv, path, fuel, rest) in rows {
            let args = parse(argv);
            assert_eq!(args.path.as_deref(), path.map(Path::new), "{argv:?}");
            assert_eq!(args.fuel, fuel, "{argv:?}");
            assert_eq!(args.args, rest, "{argv:?}");
        }
    }

    /// After the first bare token, `--fuel` is the program's, as `-L` is: the
    /// trailing-argument contract holds for the new option too.
    #[test]
    fn a_fuel_after_the_first_bare_token_becomes_a_program_argument() {
        let args = parse(&["f.inf", "5", "--fuel", "3"]);
        assert_eq!(args.fuel, None, "`--fuel` after a bare token is the program's");
        assert_eq!(args.args, ["5", "--fuel", "3"]);
    }

    /// `--fuel 0` is refused by the parser with the spelling of "no budget",
    /// and a value that is not a count is refused too.
    ///
    /// Fails if zero is read as no budget or as a budget that stops the run
    /// before its first instruction, or if the refusal stops saying how to ask
    /// for no budget.
    #[test]
    fn fuel_refuses_zero_and_what_is_not_a_count() {
        let refusal = |argv: &[&str]| {
            let mut full = vec!["run"];
            full.extend_from_slice(argv);
            match RunCli::try_parse_from(full) {
                Ok(_) => panic!("`infs {}` must not parse", argv.join(" ")),
                Err(err) => err.to_string(),
            }
        };
        let zero = refusal(&["f.inf", "--fuel", "0"]);
        assert!(
            zero.contains("`--fuel` must be at least 1; omit it to run without a budget"),
            "got: {zero}"
        );
        for argv in [
            &["--fuel=-1"][..],
            &["--fuel", "ten"][..],
            &["--fuel", "1.5"][..],
            &["--fuel", "18446744073709551616"][..],
        ] {
            let error = refusal(argv);
            assert!(
                error.contains("invalid value") && error.contains("--fuel"),
                "`{}` must be refused as an invalid budget, got: {error}",
                argv.join(" ")
            );
        }
        assert_eq!(
            parse_fuel("0"),
            Err("`--fuel` must be at least 1; omit it to run without a budget".to_string())
        );
        assert_eq!(parse_fuel("1"), Ok(NonZeroUsize::MIN));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{
        module_exporting, module_with_function_imports, module_with_imports, module_with_raw_body,
    };
    use std::sync::LazyLock;
    use wasm_encoder::{EntityType, MemoryType};

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
            fuel: None,
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

    /// Each target runs where its build is meant to: `wasm32` under wasmtime,
    /// `spacewasm` under the `SpaceWasm` interpreter, `stellar` nowhere. Every name
    /// in the vocabulary is asked, and each answer is pinned by value rather than
    /// by a second copy of the match.
    ///
    /// Unit-tested beside the end-to-end rows because those reach the wasmtime
    /// route only where wasmtime is installed: this pin holds on every machine.
    ///
    /// Fails if a target is routed to another runtime — `spacewasm` handed to
    /// wasmtime, as it was before it had an interpreter of its own — or if a
    /// name is added to the vocabulary without a row here.
    #[test]
    fn each_target_runs_under_the_runtime_its_build_is_for() {
        let rows = [
            (TargetName::Wasm32, LocalRuntime::Wasmtime),
            (TargetName::SpaceWasm, LocalRuntime::SpaceWasmInterpreter),
            (TargetName::Stellar, LocalRuntime::Unavailable),
        ];
        for (target, runtime) in rows {
            assert_eq!(local_runtime(target), runtime, "`{}`", target.as_str());
        }
        for target in TargetName::ALL {
            assert!(
                rows.iter().any(|(named, _)| *named == target),
                "`{}` has no row",
                target.as_str()
            );
        }
    }

    /// The refusal of a target no local runtime runs names that target and the
    /// one to run the same source at instead, and says what a runtime at hand
    /// would do wrong.
    #[test]
    fn the_refusal_of_a_target_nothing_runs_names_it_and_the_default() {
        let refusal = no_local_runtime(TargetName::Stellar).to_string();
        assert_eq!(
            refusal,
            "`infs run` cannot run a `stellar` build. A Stellar contract is invoked by a Soroban \
             host, which encodes each argument into the host's 64-bit tagged word; a plain \
             WebAssembly runtime such as wasmtime passes a decimal instead, which the contract \
             decodes as a different value entirely — a wrong answer rather than an error. Build \
             it with `infs build` and deploy it, or run the same source at the `wasm32` target \
             to execute it locally. See the book's Compilation Targets chapter."
        );
    }

    /// `--fuel` on a build that runs under wasmtime is refused, in the words of
    /// the scope the build came from: a project is told it may name the
    /// interpreter's target in its manifest, a file outside any project only to
    /// drop the flag.
    ///
    /// Fails if a file with no manifest is told to edit one, or if either
    /// refusal stops naming the target the build runs at.
    #[test]
    fn fuel_outside_the_interpreter_is_refused_in_the_words_of_its_scope() {
        let file = Path::new("sensor.inf");
        let project = fuel_outside_the_interpreter(TargetName::Wasm32, in_a_project()).to_string();
        assert_eq!(
            project,
            "`--fuel` applies only to a `spacewasm` build, and this project builds at the \
             `wasm32` target, which `infs run` executes under wasmtime without an instruction \
             budget. Remove `--fuel`, or set `target = \"spacewasm\"` under `[build]` in \
             Inference.toml to run the program under the SpaceWasm interpreter, which counts its \
             instructions."
        );
        let loose =
            fuel_outside_the_interpreter(TargetName::Wasm32, Scope::NoProject { file }).to_string();
        assert_eq!(
            loose,
            "`--fuel` applies only to a `spacewasm` build, and sensor.inf is outside any \
             project, so it builds at the default `wasm32` target, which `infs run` executes \
             under wasmtime without an instruction budget. Remove `--fuel`."
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
            fuel: None,
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

    /// The command line `run` hands wasmtime for `entry_point` and `args`, on the
    /// artifact project mode runs, as strings: every entry these tests build is
    /// UTF-8.
    fn wasmtime_argv_of(entry_point: &str, args: &[&str]) -> Vec<String> {
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        wasmtime_argv(&Path::new("out").join("main.wasm"), entry_point, &args)
            .into_iter()
            .map(|entry| entry.into_string().expect("every entry is UTF-8"))
            .collect()
    }

    /// `--invoke <entry_point> out/main.wasm`, the part of the command line every
    /// invocation starts with.
    fn invocation_of(entry_point: &str) -> Vec<String> {
        vec![
            "--invoke".to_string(),
            entry_point.to_string(),
            Path::new("out").join("main.wasm").display().to_string(),
        ]
    }

    /// A `main` given no arguments gets none: nothing follows the module path.
    /// The command line once appended `0 0` to every `main` call. A `main` that
    /// takes nothing ran regardless, because the wasmtime CLI ignores arguments
    /// beyond the last parameter; one taking an `i32` ran with it set to 0, and
    /// one returning a struct or array took the first 0 as its result address.
    ///
    /// Fails if `main` is special-cased again with any appended argument.
    #[test]
    fn main_gets_nothing_after_the_module_path_when_given_no_arguments() {
        for entry_point in [DEFAULT_ENTRY_POINT, "helper"] {
            assert_eq!(
                wasmtime_argv_of(entry_point, &[]),
                invocation_of(entry_point),
                "`{entry_point}` given no arguments must get none"
            );
        }
    }

    /// `main` is an ordinary entry point: the trailing arguments follow the
    /// module path, in the order given, exactly as they do for any other export.
    /// Each row is asked of `main` and of two other names, so a command line
    /// that treated `main` differently in any row reads differently from theirs.
    ///
    /// Fails if `main`'s arguments are dropped, replaced or reordered, or if any
    /// entry point's are.
    #[test]
    fn every_entry_point_receives_the_trailing_arguments_in_order() {
        let rows: [&[&str]; 5] = [&["5"], &["2", "40"], &["0", "0"], &["-7", "8", "9"], &["x"]];
        for args in rows {
            for entry_point in [DEFAULT_ENTRY_POINT, "add", "main2"] {
                let mut expected = invocation_of(entry_point);
                expected.extend(args.iter().map(|arg| (*arg).to_string()));
                assert_eq!(
                    wasmtime_argv_of(entry_point, args),
                    expected,
                    "`{entry_point}` must receive {args:?} after the module path"
                );
            }
        }
    }

    /// An argument shaped like a flag reaches wasmtime as written, after the
    /// module path, where wasmtime reads it as the function's rather than its
    /// own. That includes `--invoke` and `--`, which `run` must neither interpret
    /// nor strip: clap has already consumed the `--` that separated them on the
    /// `infs` command line, so any that remain are the program's.
    ///
    /// Fails if an argument is filtered, split or re-spelled on its way through.
    #[test]
    fn flag_shaped_arguments_are_passed_verbatim() {
        let rows: [&[&str]; 5] = [
            &["-L", "libs"],
            &["--entry-point", "helper"],
            &["--invoke", "other"],
            &["--", "-5"],
            &["-5", "--no-wasm-opt"],
        ];
        for args in rows {
            for entry_point in [DEFAULT_ENTRY_POINT, "add"] {
                let argv = wasmtime_argv_of(entry_point, args);
                assert_eq!(
                    argv[3..],
                    *args,
                    "`{entry_point}` must receive {args:?} verbatim, got argv: {argv:?}"
                );
                assert_eq!(
                    argv[..3],
                    invocation_of(entry_point),
                    "the invocation must precede every argument, got argv: {argv:?}"
                );
            }
        }
    }

    /// The module path is one argv entry, whatever it contains, and an argument
    /// containing a space stays one entry too: nothing is joined into a string
    /// and re-split on the way to wasmtime.
    #[test]
    fn a_path_or_argument_with_a_space_stays_one_entry() {
        let wasm = Path::new("my project").join("out").join("main.wasm");
        let argv = wasmtime_argv(&wasm, DEFAULT_ENTRY_POINT, &["1 2".to_string()]);
        assert_eq!(
            argv,
            [
                OsString::from("--invoke"),
                OsString::from(DEFAULT_ENTRY_POINT),
                OsString::from(&wasm),
                OsString::from("1 2"),
            ]
        );
    }

    /// Project mode's warning for trailing arguments names each one it drops,
    /// in the order given, and says what `main` receives instead: nothing.
    ///
    /// Unit-tested because no command line reaches it (the first bare token
    /// binds to the source path), so without this pin its text could go false
    /// with nothing turning red.
    #[test]
    fn the_project_mode_warning_names_every_ignored_argument() {
        let rows: [(&[&str], &str); 3] = [
            (&["5"], "5"),
            (&["2", "40"], "2 40"),
            (&["1", "-L", "libs"], "1 -L libs"),
        ];
        for (args, listed) in rows {
            let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
            assert_eq!(
                ignored_project_arguments_warning(&args),
                format!(
                    "warning: project mode passes no arguments to `main`; these are ignored: \
                     {listed}"
                )
            );
        }
    }

    /// `out/main.wasm`, the artifact the refusals below name.
    fn out_main() -> PathBuf {
        Path::new("out").join("main.wasm")
    }

    /// A project whose manifest sets nothing but its name, so no `[build]` key
    /// stands in the way of any target.
    fn in_a_project() -> Scope<'static> {
        static MANIFEST: LazyLock<InferenceToml> = LazyLock::new(|| InferenceToml::new("demo"));
        Scope::Project {
            manifest: &MANIFEST,
        }
    }

    /// Asks the `wasm32` import refusal about `module`, named as project mode
    /// names its artifact, from `scope`.
    fn import_refusal_of(module: &[u8], scope: Scope<'_>) -> Result<()> {
        refuse_an_artifact_that_imports_a_function(module, &out_main(), TargetName::Wasm32, scope)
    }

    /// The refusal of `module`, which must be refused.
    fn refusal_text(module: &[u8], scope: Scope<'_>) -> String {
        import_refusal_of(module, scope)
            .expect_err("an artifact importing a function is refused")
            .to_string()
    }

    /// One function import: its module, its field, and its signature.
    type TypedImport = (
        &'static str,
        &'static str,
        &'static [wasm_encoder::ValType],
        &'static [wasm_encoder::ValType],
    );

    /// The six F´ reference hosts, each at its reference signature.
    const REFERENCE: [TypedImport; 6] = {
        use wasm_encoder::ValType::{I32, I64};
        [
            ("fprime_core", "panic", &[I32, I32, I32], &[]),
            ("fprime_core", "rsleep", &[I64], &[]),
            ("fprime_core", "command", &[I32, I32], &[I32]),
            ("fprime_core", "message", &[I32, I32], &[]),
            ("fprime_core", "telemetry", &[I32, I32, I32, I32, I32], &[I32]),
            ("env", "clock_ms", &[], &[I64]),
        ]
    };

    /// The reference host registered as `module.field`.
    fn reference(module: &str, field: &str) -> TypedImport {
        REFERENCE
            .into_iter()
            .find(|host| host.0 == module && host.1 == field)
            .expect("a reference host")
    }

    /// The closing sentence both variants of the refusal point at the book with.
    const BOOK: &str = "See the book's External Functions and WASM Linking chapter (\"Running \
                        a program that binds host imports\").";

    /// An artifact that imports no function is what `run` executes: one with no
    /// imports at all, and one whose only import is a memory, which any host can
    /// allocate — from a project and from a file outside one alike.
    #[test]
    fn an_artifact_importing_no_function_is_let_through() {
        let memory_only = module_with_imports(&[(
            "env",
            "memory",
            EntityType::Memory(MemoryType {
                minimum: 1,
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            }),
        )]);
        let file = Path::new("sensor.inf");
        for module in [module_with_raw_body(&[0x0b]), memory_only] {
            for scope in [in_a_project(), Scope::NoProject { file }] {
                import_refusal_of(&module, scope)
                    .expect("an artifact importing no function is executable");
            }
        }
    }

    /// An artifact whose imports are not all F´ reference hosts at their
    /// reference signatures is refused as a `wasm32` artifact with host imports,
    /// listing each imported function once, sorted, and stating a policy rather
    /// than a runtime failure — WASI imports included, which the wasmtime CLI
    /// would satisfy itself, so a sentence claiming the imports cannot be
    /// satisfied would be false of them. It offers no target to switch to, and
    /// reads the same from a project and from a file outside one.
    ///
    /// Unit-tested because the end-to-end rows need both `infc` and wasmtime,
    /// and a machine without the runtime would otherwise pin none of this text.
    /// The WASI import is listed twice and out of order, so a refusal that kept
    /// import-section order, or counted a repeated pair twice, reads differently.
    #[test]
    fn imports_outside_the_reference_hosts_are_refused_with_no_target_to_switch_to() {
        let function = EntityType::Function(0);
        let proc_exit = ("wasi_snapshot_preview1", "proc_exit", function);
        let clock_ms = ("env", "clock_ms", function);
        let module = module_with_imports(&[proc_exit, clock_ms, proc_exit]);
        let expected = format!(
            "`infs run` cannot execute this program at the `wasm32` target: {} imports 2 \
             functions that its embedder must supply.\n  env.clock_ms\n  \
             wasi_snapshot_preview1.proc_exit\nA `wasm32` build runs under wasmtime, and `infs \
             run` registers no host functions there — not even the WASI functions the wasmtime \
             CLI provides on its own — so it executes no `wasm32` artifact that imports a \
             function. `infs run` provides host functions only to a `spacewasm` build, and only \
             the F Prime reference set at its reference signatures, which this program's imports \
             do not all match. Run the program from the embedder that supplies these functions. \
             {BOOK}",
            out_main().display()
        );
        let file = Path::new("sensor.inf");
        for scope in [in_a_project(), Scope::NoProject { file }] {
            assert_eq!(refusal_text(&module, scope), expected);
        }

        let msg = refusal_text(&module_with_imports(&[proc_exit]), in_a_project());
        for fragment in [
            "imports 1 function that its embedder must supply.\n  \
             wasi_snapshot_preview1.proc_exit\n",
            "which this program's import does not match.",
            "the embedder that supplies this function.",
        ] {
            assert!(
                msg.contains(fragment),
                "one import is named in the singular, carrying `{fragment}`, got: {msg}"
            );
        }
        assert!(
            !msg.contains("instantiation fails") && !msg.contains("unsatisfied"),
            "wasmtime would instantiate a WASI import, so the refusal must not say it \
             cannot, got: {msg}"
        );
    }

    /// Reference names are not enough: a reference host at another signature,
    /// among reference hosts at theirs, is refused with no target to switch to,
    /// since the `SpaceWasm` route would refuse it too.
    ///
    /// Fails if the choice between the two refusals reads names and not
    /// signatures.
    #[test]
    fn a_reference_host_at_another_signature_is_refused_with_no_target_to_switch_to() {
        use wasm_encoder::ValType::I32;
        let mut imports = REFERENCE.to_vec();
        imports[5] = ("env", "clock_ms", &[], &[I32]);
        let msg = refusal_text(&module_with_function_imports(&imports), in_a_project());
        assert!(
            msg.contains("which this program's imports do not all match."),
            "got: {msg}"
        );
        assert!(!msg.contains("set `target"), "no target is offered, got: {msg}");
    }

    /// An artifact whose every import is an F´ reference host at its reference
    /// signature is told the target that provides them, in the grammar its
    /// count takes: one host by name, two as both, more as all of them, a
    /// repeated pair counted once.
    ///
    /// Fails if the choice is inverted, if a count is spelled in the wrong
    /// grammar, or if the remedy stops naming the manifest key to set.
    #[test]
    fn reference_hosts_are_refused_with_the_target_that_provides_them() {
        let clock = reference("env", "clock_ms");
        let command = reference("fprime_core", "command");
        let telemetry = reference("fprime_core", "telemetry");
        let remedy = "which `infs run` provides to a `spacewasm` build: set `target = \
                      \"spacewasm\"` under `[build]` in Inference.toml and run it again to \
                      execute the program under the SpaceWasm interpreter.";
        let rows: [(&[TypedImport], &str, &str); 5] = [
            (&[clock], "`env.clock_ms` is an F Prime reference host", "it"),
            (&[clock, clock], "`env.clock_ms` is an F Prime reference host", "it"),
            (&[command, clock], "Both are F Prime reference hosts", "them"),
            (&[telemetry, command, clock], "All three are F Prime reference hosts", "them"),
            (&REFERENCE, "All six are F Prime reference hosts", "them"),
        ];
        for (imports, hosts, them) in rows {
            let msg = refusal_text(&module_with_function_imports(imports), in_a_project());
            let expected = format!(
                "A `wasm32` build runs under wasmtime, where `infs run` registers no host \
                 functions. {hosts}, {remedy} Otherwise, run it from the embedder that supplies \
                 {them}. {BOOK}"
            );
            assert!(
                msg.ends_with(&expected),
                "{imports:?} must end:\n{expected}\ngot:\n{msg}"
            );
        }

        let msg = refusal_text(
            &module_with_function_imports(&[telemetry, command, clock]),
            in_a_project(),
        );
        assert_eq!(
            msg,
            format!(
                "`infs run` cannot execute this program at the `wasm32` target: {} imports 3 \
                 functions that its embedder must supply.\n  env.clock_ms\n  \
                 fprime_core.command\n  fprime_core.telemetry\nA `wasm32` build runs under \
                 wasmtime, where `infs run` registers no host functions. All three are F Prime \
                 reference hosts, {remedy} Otherwise, run it from the embedder that supplies \
                 them. {BOOK}",
                out_main().display()
            )
        );
    }

    /// A file outside any project is told to make one before it can name the
    /// target, since it has no manifest to set a key in.
    ///
    /// Fails if the file is told to edit an `Inference.toml` it does not have.
    #[test]
    fn reference_hosts_in_a_file_outside_any_project_are_told_to_init_one_first() {
        let file = Path::new("sensor.inf");
        let rows: [(&[TypedImport], &str, &str); 2] = [
            (
                &[reference("env", "clock_ms")],
                "`env.clock_ms` is an F Prime reference host",
                "it",
            ),
            (&REFERENCE[..3], "All three are F Prime reference hosts", "them"),
        ];
        for (imports, hosts, them) in rows {
            let msg = refusal_text(
                &module_with_function_imports(imports),
                Scope::NoProject { file },
            );
            let expected = format!(
                "A `wasm32` build runs under wasmtime, where `infs run` registers no host \
                 functions. {hosts}, which `infs run` provides to a `spacewasm` build. This file \
                 is not inside a project, so it builds at the default `wasm32` target: run `infs \
                 init` in its directory, set `target = \"spacewasm\"` under `[build]` in the \
                 Inference.toml it writes, and run it again. Otherwise, run it from the embedder \
                 that supplies {them}. {BOOK}"
            );
            assert!(
                msg.ends_with(&expected),
                "{imports:?} must end:\n{expected}\ngot:\n{msg}"
            );
        }
    }

    /// A project whose manifest sets a `[build]` key the `spacewasm` target
    /// refuses is told to remove it as well as to set the target, naming
    /// exactly the keys it sets: `mode = "proof"`, a non-empty `wasm-features`
    /// list, or both. An empty `wasm-features` list and compile mode are keys
    /// the target accepts, so they keep the remedy as it reads without them.
    ///
    /// Fails if a remedy promises a run that loading the edited manifest would
    /// refuse, names a key the manifest does not set, or drops one it does.
    #[test]
    fn reference_hosts_in_a_project_are_told_to_remove_the_keys_the_target_refuses() {
        let module = module_with_function_imports(&[reference("env", "clock_ms")]);
        let then = "and run it again to execute the program under the SpaceWasm interpreter. \
                    Otherwise, run it from the embedder that supplies it.";
        let rows: [(&str, &[&str], &str); 4] = [
            ("proof", &[], ", remove `mode = \"proof\"`, which that target refuses,"),
            ("compile", &["bulk-memory"], ", remove `wasm-features`, which that target refuses,"),
            (
                "proof",
                &["bulk-memory"],
                ", remove `mode = \"proof\"` and `wasm-features`, which that target refuses,",
            ),
            ("compile", &[], ""),
        ];
        for (mode, features, removed) in rows {
            let mut manifest = InferenceToml::new("demo");
            manifest.build.mode = mode.to_string();
            manifest.build.wasm_features =
                features.iter().map(|feature| (*feature).to_string()).collect();
            let msg = refusal_text(&module, Scope::Project { manifest: &manifest });
            let expected = format!(
                "`env.clock_ms` is an F Prime reference host, which `infs run` provides to a \
                 `spacewasm` build: set `target = \"spacewasm\"` under `[build]` in \
                 Inference.toml{removed} {then}"
            );
            assert!(
                msg.contains(&expected),
                "mode {mode:?}, wasm-features {features:?} must read:\n{expected}\ngot:\n{msg}"
            );
        }
    }

    /// A verification construct in the artifact is refused before any runtime
    /// sees it, naming the construct and the runtime that could not decode it,
    /// in the clause `[build.wasm-opt]` refuses the same find with. A backstop —
    /// the analyzer keeps such constructs out of a compile-mode build — so no
    /// end-to-end route reaches it.
    #[test]
    fn a_leaked_verification_construct_is_refused_naming_the_runtime() {
        let uzumaki = module_with_raw_body(&[0x00, 0xfc, 0x31, 0x1a, 0x0b]);
        let expected = |runtime: &str| {
            format!(
                "`infs run` cannot execute this program: {} contains the verification-only \
                 construct `i32.uzumaki`, which {runtime} cannot decode. Verification \
                 constructs (forall/exists/assume/unique and `@`/uzumaki) belong in `spec` \
                 blocks, which compile-mode builds strip; move the construct into a `spec` \
                 block.",
                out_main().display()
            )
        };
        assert_eq!(
            refusal_text(&uzumaki, in_a_project()),
            expected("wasmtime"),
            "the wasm32 route asks the construct question first"
        );
        assert_eq!(
            executable_function_imports(&uzumaki, &out_main(), INTERPRETER)
                .expect_err("a leaked construct is refused")
                .to_string(),
            expected("the SpaceWasm interpreter")
        );
    }

    /// The wasmtime route reads `main`'s compiled parameters off the artifact
    /// and refuses a `main` that takes any — a parameter it declares, or the
    /// hidden result address of a struct or array return — and lets through a
    /// `main` that takes none, and an artifact exporting no `main`, which
    /// wasmtime reports itself.
    ///
    /// Fails if the check reads the wrong export, stops counting parameters, or
    /// fires on a `main` wasmtime can call with nothing.
    #[test]
    fn the_wasmtime_route_refuses_a_main_that_takes_arguments() {
        use wasm_encoder::ValType::{I32, I64};
        let entry_file = Path::new("src").join("main.inf");
        let refused = |module: &[u8]| {
            refuse_a_main_that_takes_arguments(module, &out_main(), &entry_file)
                .expect_err("a `main` taking arguments is refused")
                .to_string()
        };
        assert_eq!(
            refused(&module_exporting(&[("add", &[], &[I32]), ("main", &[I32], &[I32])])),
            format!(
                "`main` takes 1 argument (i32), and project mode passes none. Run the entry file \
                 with its arguments after the path: `infs run {} <i32>`.",
                entry_file.display()
            )
        );
        assert!(
            refused(&module_exporting(&[("main", &[I32], &[])]))
                .starts_with("`main` takes 1 argument (i32)"),
            "a struct or array return is a hidden address parameter"
        );
        assert!(
            refused(&module_exporting(&[("main", &[I64, I32], &[I64])]))
                .starts_with("`main` takes 2 arguments (i64, i32)"),
        );
        for module in [
            module_exporting(&[("main", &[], &[I32])]),
            module_exporting(&[("main", &[], &[])]),
            module_exporting(&[("add", &[I32, I32], &[I32])]),
            module_with_raw_body(&[0x0b]),
        ] {
            refuse_a_main_that_takes_arguments(&module, &out_main(), &entry_file)
                .expect("a `main` taking nothing, or none at all, is wasmtime's to run");
        }
    }

    /// A compiled signature reads as the runner's description of an export, and
    /// a type no Inference artifact declares reads as nothing, which leaves
    /// the call to wasmtime.
    #[test]
    fn a_compiled_signature_reads_as_the_runners_export() {
        use inf_wasmparser::{FuncType, RefType, ValType as Wasm};
        let read = |params: &[Wasm], results: &[Wasm]| {
            let ty = FuncType::new(params.iter().copied(), results.iter().copied());
            as_exported_function("f", &ty)
        };
        assert_eq!(
            read(&[Wasm::I32, Wasm::I64, Wasm::F32, Wasm::F64], &[Wasm::I64]),
            Some(ExportedFunction {
                name: "f".to_string(),
                params: vec![ValType::I32, ValType::I64, ValType::F32, ValType::F64],
                result: Some(ValType::I64),
            })
        );
        assert_eq!(
            read(&[], &[]).map(|function| (function.params, function.result)),
            Some((Vec::new(), None))
        );
        assert_eq!(read(&[Wasm::V128], &[]), None);
        assert_eq!(read(&[], &[Wasm::Ref(RefType::FUNCREF)]), None);
        assert_eq!(read(&[], &[Wasm::I32, Wasm::I32]), None);
    }

    /// The entry file a refusal names is the one the reader can type from where
    /// they stand: relative beneath the working directory, absolute otherwise.
    #[test]
    fn the_entry_file_is_named_as_it_is_typed_from_the_working_directory() {
        let root = Path::new("work").join("demo");
        let ctx = ProjectContext {
            root: root.clone(),
            manifest: crate::project::manifest::InferenceToml::new("demo"),
            entry_point: root.join("src").join("main.inf"),
        };
        assert_eq!(entry_file_as_typed(&ctx, &root), Path::new("src").join("main.inf"));
        assert_eq!(entry_file_as_typed(&ctx, &root.join("src")), Path::new("main.inf"));
        assert_eq!(
            entry_file_as_typed(&ctx, &Path::new("elsewhere").join("demo")),
            ctx.entry_point
        );
    }

    /// A project discovered from a working directory in another spelling than
    /// its canonical one still names its entry file relative to it.
    ///
    /// The temporary directory is such a spelling wherever it sits behind a
    /// symlink, as on macOS, and discovery canonicalizes; on Windows the
    /// canonical spelling is a verbatim path the working directory is not.
    ///
    /// Fails if the entry file is sought beneath the working directory as
    /// reported only, which names the absolute path wherever the two differ.
    #[test]
    fn the_entry_file_is_named_relative_to_a_working_directory_in_any_spelling() {
        let dir = assert_fs::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(MANIFEST_FILE_NAME),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\ninfc_version = \"0.1.0\"\n",
        )
        .unwrap();
        let ctx = project::discover_and_load(dir.path()).expect("the project is discovered");
        assert_eq!(
            entry_file_as_typed(&ctx, dir.path()),
            Path::new("src").join("main.inf")
        );
    }
}

#[cfg(all(test, unix))]
mod forwarding_tests {
    use super::*;
    use crate::project::manifest::HostImports;
    use assert_fs::prelude::*;
    use std::os::unix::fs::PermissionsExt;

    /// Writes an executable `infc` stub under `dir` that reports a mismatched
    /// commit and the current ABI, and appends every non-probe argv entry to
    /// `log`.
    ///
    /// The mismatched commit is what makes the stub useful: it forces the ABI
    /// probe to run, which is the branch a gated forward depends on.
    fn write_stub(dir: &assert_fs::TempDir, log: &Path) -> PathBuf {
        let stub = dir.child("infc_stub");
        stub.write_str(&format!(
            "#!/bin/sh\n\
             case \"$1\" in\n\
               --commit-hash) printf 'nope\\n'; exit 0 ;;\n\
               --abi-version) printf '{}.{}\\n'; exit 0 ;;\n\
               *) printf '%s\\n' \"$@\" >> '{}'; exit 0 ;;\n\
             esac\n",
            inference_compiler_interface::COMPILER_ABI_MAJOR,
            inference_compiler_interface::COMPILER_ABI_MINOR,
            log.display()
        ))
        .unwrap();
        let mut perms = std::fs::metadata(stub.path()).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(stub.path(), perms).unwrap();
        stub.path().to_path_buf()
    }

    /// Runs the single-file compile step for a project whose only manifest
    /// settings are `target` and `host_imports`, and returns the argv the
    /// compiler was handed.
    ///
    /// The stub cannot compile, so the call returns the missing-artifact error;
    /// the log is written before that and is what the assertion reads.
    fn compile_argv(target: TargetName, host_imports: Option<&HostImports>) -> Vec<String> {
        let temp = assert_fs::TempDir::new().unwrap();
        let log = temp.child("argv.log");
        let stub = write_stub(&temp, log.path());

        let memory = crate::project::manifest::MemoryConfig::default();
        let _ = compile_to_wasm(
            &stub,
            ResolutionSource::InfcPathEnv,
            Path::new("main.inf"),
            &[],
            &EnclosingSettings {
                deps: &[],
                target,
                features: &[],
                memory: &memory,
                host_imports,
                manifest_path: None,
            },
        );

        std::fs::read_to_string(log.path())
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The regression the third disjunct of the handshake condition exists to
    /// prevent: a project that names a target and declares neither a feature nor
    /// a `[memory]` table must still have its target forwarded.
    ///
    /// The condition originally read `!features.is_empty() || !memory.is_default()`,
    /// under which this build skips the whole block — no probe, no forward — and
    /// silently produces an artifact for the default runtime under a manifest
    /// that named another. Nothing else can catch it: every other forwarding
    /// fixture declares a feature or a memory table, so the block runs for some
    /// other reason and the target rides along.
    ///
    /// Driven through `compile_to_wasm` directly rather than through `infs run`:
    /// one of the names covered here is refused by `run` before this code is
    /// reached, and the one that is not sends `run` to the real compiler, where
    /// argv is unobservable. That refusal is about how a contract is invoked; the
    /// forward is about what gets built, and the two are independent.
    ///
    /// Every forwarded name is driven rather than one, because a forward that is
    /// gated on the name — rather than on the target being non-default — would
    /// pass with the name it was written against and drop the next one.
    #[test]
    fn a_non_default_target_is_forwarded_with_nothing_else_declared() {
        let forwarded: Vec<TargetName> = TargetName::ALL
            .into_iter()
            .filter(|target| *target != TargetName::DEFAULT)
            .collect();
        assert!(
            !forwarded.is_empty(),
            "this test is a loop over the forwarded names, so an empty list \
             reports `ok` while asserting nothing"
        );

        for target in forwarded {
            let argv = compile_argv(target, None);
            let position = argv
                .iter()
                .position(|entry| entry == "--target")
                .unwrap_or_else(|| {
                    panic!(
                        "`--target` must be forwarded for `{}`, got argv: {argv:?}",
                        target.as_str()
                    )
                });
            assert_eq!(
                argv.get(position + 1).map(String::as_str),
                Some(target.as_str()),
                "the flag's value must be the next argv entry, got argv: {argv:?}"
            );
        }
    }

    /// The control for the test above: with the default target and nothing else
    /// declared, no flag is forwarded — so the assertion there is about the
    /// target having been read, not about a flag that is always present.
    #[test]
    fn the_default_target_is_not_forwarded() {
        let argv = compile_argv(TargetName::DEFAULT, None);
        assert!(
            !argv.iter().any(|entry| entry == "--target"),
            "the default target must not be forwarded, got argv: {argv:?}"
        );
        assert!(
            !argv.iter().any(|entry| entry.starts_with("--host-imports")),
            "no table is no policy, and forwards no flag, got argv: {argv:?}"
        );
    }

    /// The fourth disjunct of the handshake condition, pinned the way the third
    /// is: a project whose only setting is a `[host-imports]` table must still
    /// have its policy forwarded.
    ///
    /// The declared-empty table is the row that matters most. It forbids every
    /// host import, and a condition that tested the table's *content* rather than
    /// its presence would skip the block for exactly that table — silently
    /// turning the strictest policy into none.
    #[test]
    fn a_host_imports_table_is_forwarded_with_nothing_else_declared() {
        let mut listed = HostImports::default();
        listed
            .modules
            .insert("env".to_string(), vec!["clock_ms".to_string()]);

        for (table, expected) in [
            (HostImports::default(), "--host-imports="),
            (listed, "--host-imports=env.clock_ms"),
        ] {
            let argv = compile_argv(TargetName::DEFAULT, Some(&table));
            let forwarded: Vec<&String> = argv
                .iter()
                .filter(|entry| entry.starts_with("--host-imports"))
                .collect();
            assert_eq!(
                forwarded,
                [expected],
                "the policy must be forwarded as one token, got argv: {argv:?}"
            );
        }
    }
}
