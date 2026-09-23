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
//! - **Refuses an artifact that imports a function**, in both modes, after the
//!   build and before wasmtime: a program binding `use { … } from host::…` is
//!   left to the embedder that supplies those functions, whether or not wasmtime
//!   could satisfy them (see `refuse_an_artifact_that_imports_a_function`).
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

use crate::artifact::{ArtifactScan, VERIFICATION_CONSTRUCTS_BELONG_IN_SPECS, scan_artifact};
use crate::commands::build::{
    EnclosingSettings, enclosing_manifest, format_wasm_dep_arg, manifest_host_imports,
    manifest_memory, manifest_target, manifest_wasm_dependencies, manifest_wasm_features,
};
use crate::commands::project_build::{
    forward_host_imports, forward_memory_layout, forward_target, forward_wasm_features,
    probe_compiler_compatibility, run_project_build,
};
use crate::errors::InfsError;
use crate::project::manifest::MANIFEST_FILE_NAME;
use inference_compiler_interface::TargetName;
use crate::project::{self, ProjectContext};
use crate::toolchain::resolver::{ResolutionSource, find_infc_with_source};

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
/// 2. Resolves the enclosing project's `[build] target` and refuses one whose
///    artifact wasmtime cannot invoke
/// 3. Checks for wasmtime availability
/// 4. Resolves the rest of the enclosing project's settings — `[build]
///    wasm-features`, `[memory]`, `[host-imports]`, `[wasm-dependencies]` — if
///    any
/// 5. Locates the infc compiler
/// 6. Compiles source to WASM via infc subprocess, forwarding those settings
///    alongside every `-L` the user passed
/// 7. Refuses the artifact if it imports a function
/// 8. Executes WASM with wasmtime, invoking `--entry-point`
/// 9. Propagates exit code from wasmtime
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
/// target read off it is ordered ahead of the wasmtime probe for a sharper
/// reason: the refusal below applies to a build wasmtime could never invoke, so
/// a user who lacks the runtime must not first be sent to install it. The
/// import refusal cannot be ordered that way, because it reads the artifact, and
/// deliberately stays behind the probe rather than pulling the probe behind the
/// build — why is `refuse_an_artifact_that_imports_a_function`'s to say.
///
/// ## Errors
///
/// Returns an error if:
/// - The source file does not exist
/// - the enclosing manifest names a target whose artifact wasmtime cannot invoke
/// - wasmtime is not found in PATH
/// - a `[wasm-dependencies]` key is not a well-formed logical module name or its
///   first segment is the reserved `host`, or a resolved dependency path is not
///   valid UTF-8
/// - infc compiler cannot be found
/// - the enclosing manifest names a `target`, requests `wasm-features`, or
///   declares a `[memory]` or `[host-imports]` table the resolved `infc` cannot
///   honor (which are also the only cases that run the ABI handshake here)
/// - Compilation fails
/// - the artifact imports a function, which `run` leaves to an embedder
/// - WASM execution fails
fn execute_single_file(path: &Path, args: &RunArgs) -> Result<()> {
    if !path.exists() {
        bail!("Path not found: {}", path.display());
    }

    let enclosing = enclosing_manifest(path)?;
    let target = manifest_target(enclosing.as_ref().map(|(_, manifest)| manifest))?;
    refuse_target_wasmtime_cannot_invoke(target)?;

    check_wasmtime_availability()?;

    let features = manifest_wasm_features(enclosing.as_ref().map(|(_, manifest)| manifest))?;
    let memory = manifest_memory(enclosing.as_ref().map(|(_, manifest)| manifest));
    let host_imports = manifest_host_imports(enclosing.as_ref().map(|(_, manifest)| manifest));
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
        &EnclosingSettings {
            deps: &deps,
            target,
            features: &features,
            memory: &memory,
            host_imports,
            manifest_path: manifest_path.as_deref(),
        },
    )?;

    refuse_an_artifact_that_imports_a_function(&wasm_path, &wasm_path)?;
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
/// The project is discovered and its target refused before wasmtime is looked
/// for, because a target this command cannot run is not a missing-runtime
/// problem: installing wasmtime would not make the build invokable, so pointing
/// a user at its download page would cost them an install and leave them exactly
/// where they were. wasmtime availability is then checked before any
/// compilation, so an environment lacking the runtime fails fast without first
/// spending a build, matching single-file mode — and so, as there, ahead of the
/// import refusal, which only a finished artifact can answer. Why that one
/// refusal is left behind the probe is
/// `refuse_an_artifact_that_imports_a_function`'s to say.
///
/// ## Errors
///
/// Returns an error if:
/// - `--entry-point` is set to a non-`main` value (project mode invokes `main`)
/// - No `Inference.toml` is found in the current directory or any ancestor
/// - The project names a target whose artifact wasmtime cannot invoke
/// - wasmtime is not found in PATH
/// - The project build fails (missing entry point, ABI handshake,
///   external-module forwarding, infc error)
/// - The build succeeds but `<root>/out/main.wasm` is absent
/// - the artifact imports a function, which `run` leaves to an embedder
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

    let cwd =
        std::env::current_dir().context("Failed to determine the current working directory")?;
    let ctx = project::discover_and_load(&cwd)?;
    refuse_target_wasmtime_cannot_invoke(ctx.manifest.build.resolved_target()?)?;

    check_wasmtime_availability()?;

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

    refuse_an_artifact_that_imports_a_function(&wasm_path, &Path::new("out").join("main.wasm"))?;
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

/// Refuses a target whose artifact is not something `wasmtime --invoke` can
/// meaningfully call.
///
/// `run` compiles and then executes through the `wasmtime` CLI, which invokes an
/// export by name and passes each argument as a decimal it parses into the
/// export's declared parameter type. A Stellar contract's exports are not that
/// shape: every method takes and returns the host's 64-bit tagged word, so a
/// `5` typed on the command line arrives as a word whose low byte is read as the
/// tag and whose payload is empty — decoding, silently, to a zero-valued
/// integer rather than to five. `main` is worse: this path hands it the
/// `argc, argv` pair a C entry point takes, which a value-ABI wrapper does not
/// have, and the call fails on arity for a reason that says nothing about why.
///
/// Both outcomes are wrong answers rather than missing features, which is why
/// this is a refusal and not a warning. Nothing here can be fixed by passing
/// different arguments: a contract is invoked by a Soroban host, which encodes
/// its arguments into that word, and by nothing else.
///
/// The question is asked of [`TargetName::runs_under_a_plain_wasm_runtime`]
/// rather than compared against one variant, so "non-default" and "not
/// runnable" stay separate: a target that narrows what a build may contain
/// while emitting the default's bytes runs here exactly as the default does,
/// and a name added to the vocabulary has to state which of the two it is
/// rather than becoming runnable — or unrunnable — because nobody looked.
///
/// The message is the one exception to that generality: it explains the tagged
/// word because Stellar is the only name that answers `false`, and a second
/// such name would be describing a different convention entirely. It renders
/// the refused target from the argument, so the name is right the moment one
/// arrives; the wording around it is what a second `false` arm has to bring.
///
/// # Errors
///
/// Returns the refusal when `target` names a runtime whose calling convention
/// `wasmtime` does not implement.
///
/// Both call sites run this *before* probing for `wasmtime` itself. The refusal
/// is about what was built, not about what is installed, so a machine without
/// the runtime must hear the refusal rather than an install prompt for a tool
/// that would change nothing.
fn refuse_target_wasmtime_cannot_invoke(target: TargetName) -> Result<()> {
    if target.runs_under_a_plain_wasm_runtime() {
        return Ok(());
    }
    bail!(
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

/// Refuses an artifact that imports a function: a program with host imports runs
/// under the embedder that supplies them, and `infs run` is not one.
///
/// This is a policy, not a runtime fact. `run` executes through the `wasmtime`
/// CLI, which registers none of a program's host modules — an `env.clock_ms`
/// import fails to instantiate there, in wasmtime's words, which name an unknown
/// import and read as a broken build — but which does link WASI on its own, so
/// an artifact whose only imports are WASI functions would instantiate. It is
/// refused all the same. A host import is a promise a particular embedder
/// keeps, and which of those promises wasmtime happens to keep is a property of
/// the runtime `run` uses today rather than of the program; executing against a
/// stand-in would make whether a program runs depend on which functions the
/// stand-in provides. So `run` executes only artifacts that import no function,
/// and says where the others do run.
///
/// Decided from the artifact, which is why it follows the build rather than
/// preceding it as [`refuse_target_wasmtime_cannot_invoke`] does. Whether a
/// program binds a host import is a property of its source, and a manifest
/// cannot answer it: `[host-imports]` is an allowlist, not a declaration — a
/// project may list functions it never binds, and bind them with no table at
/// all. It is asked of the bytes that will run, too, which in project mode are
/// the bytes `[build.wasm-opt]` left behind: the optimizer may remove an import
/// nothing calls, and a program whose only host import is uncalled then runs
/// where its unoptimized build would be refused.
///
/// Following the build also puts it behind the wasmtime probe, which both call
/// sites run before they build, so a machine without wasmtime is told to install
/// it before it can hear this refusal — the one install prompt the target
/// refusal's ordering exists to avoid. The order is kept on purpose. The probe
/// could only move behind the build, and there every run on such a machine, of
/// any program, would spend a compile before learning the runtime is missing,
/// to spare the programs that bind a host import one prompt.
///
/// The scan answers the verification-construct question first, so an artifact
/// carrying one is refused here as well, in the words `[build.wasm-opt]` uses
/// for the same find. It is a backstop, as the optimizer's is: A042 rejects a
/// non-deterministic block outside a `spec` (and A006 an `@` outside such a
/// block), and compile-mode builds strip `spec` blocks, so a well-formed build
/// never reaches it — but an artifact that did would otherwise be handed to
/// wasmtime to fail on an opcode it cannot decode.
///
/// `wasm_path` is the file read; `shown_as` is the conventional relative
/// spelling the message names it by, as the other `run` messages do.
///
/// # Errors
///
/// Returns the refusal when the artifact imports any function, a refusal naming
/// the construct when it carries a verification-only one, and an error when it
/// cannot be read or parsed.
fn refuse_an_artifact_that_imports_a_function(wasm_path: &Path, shown_as: &Path) -> Result<()> {
    let wasm_bytes = std::fs::read(wasm_path)
        .with_context(|| format!("Failed to read {} to check its imports", shown_as.display()))?;
    let mut imports = match scan_artifact(&wasm_bytes, shown_as)? {
        ArtifactScan::VerificationConstruct(construct) => bail!(
            "`infs run` cannot execute this program: {} contains the verification-only \
             construct `{construct}`, which wasmtime cannot decode. \
             {VERIFICATION_CONSTRUCTS_BELONG_IN_SPECS}; move the construct into a `spec` block.",
            shown_as.display()
        ),
        ArtifactScan::Executable {
            function_imports, ..
        } => function_imports,
    };
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
    bail!(
        "`infs run` cannot execute this program: {} imports {count} {functions} that its \
         embedder must supply.\n{listed}\n`infs run` supplies no host functions and does not \
         stand in for an embedder, not even with the WASI functions the wasmtime CLI \
         provides on its own, so it executes no artifact that imports a function. Run the \
         program from your embedder, which supplies {these_functions}. See the book's \
         External Functions and WASM Linking chapter (\"Running a program that binds host \
         imports\") for how an embedder registers {them}.",
        shown_as.display()
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
    use crate::testing::{module_with_imports, module_with_raw_body};
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

    /// Whether `infs run` refuses a target follows the calling convention its
    /// runtime imposes, and not whether the target is the default one. Every name
    /// in the vocabulary is asked here, at the function both call sites go
    /// through.
    ///
    /// Unit-tested rather than left to the end-to-end suite because the arm that
    /// *permits* a non-default target is reached there only with `wasmtime`
    /// installed: on a machine without the runtime, a positional check
    /// reintroduced here would have nothing to catch it, while the refusing arm
    /// would keep its coverage.
    ///
    /// Fails if the refusal reverts to a comparison against `TargetName::DEFAULT`,
    /// which answers the same for every name but the one whose artifact is the
    /// default's.
    #[test]
    fn the_wasmtime_refusal_asks_the_calling_convention_and_not_the_default() {
        for target in TargetName::ALL {
            assert_eq!(
                refuse_target_wasmtime_cannot_invoke(target).is_ok(),
                target.runs_under_a_plain_wasm_runtime(),
                "`{}` must be let through exactly when a plain runtime can invoke \
                 its artifact",
                target.as_str()
            );
        }
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

    /// Writes `module` to a fresh file and asks the import refusal about it,
    /// naming it `out/main.wasm` the way project mode names its artifact.
    fn import_refusal_of(module: &[u8]) -> Result<()> {
        let dir = assert_fs::TempDir::new().unwrap();
        let wasm = dir.path().join("main.wasm");
        std::fs::write(&wasm, module).unwrap();
        refuse_an_artifact_that_imports_a_function(&wasm, &Path::new("out").join("main.wasm"))
    }

    /// An artifact that imports no function is what `run` executes: one with no
    /// imports at all, and one whose only import is a memory, which any host can
    /// allocate.
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
        for module in [module_with_raw_body(&[0x0b]), memory_only] {
            import_refusal_of(&module).expect("an artifact importing no function is executable");
        }
    }

    /// The refusal lists each imported function once, sorted, and is stated as
    /// a policy rather than as a runtime failure — WASI imports included, which
    /// the wasmtime CLI would satisfy itself, so a sentence claiming the imports
    /// cannot be satisfied would be false of them.
    ///
    /// Unit-tested because the end-to-end rows need both `infc` and wasmtime,
    /// and a machine without the runtime would otherwise pin none of this text.
    /// The WASI import is listed twice and out of order, so a refusal that kept
    /// import-section order, or counted a repeated pair twice, reads differently.
    #[test]
    fn the_import_refusal_lists_every_function_and_states_a_policy() {
        let function = EntityType::Function(0);
        let proc_exit = ("wasi_snapshot_preview1", "proc_exit", function);
        let clock_ms = ("env", "clock_ms", function);
        let module = module_with_imports(&[proc_exit, clock_ms, proc_exit]);
        let msg = import_refusal_of(&module)
            .expect_err("an artifact importing a function is refused")
            .to_string();
        assert_eq!(
            msg,
            format!(
                "`infs run` cannot execute this program: {} imports 2 functions that its \
                 embedder must supply.\n  env.clock_ms\n  wasi_snapshot_preview1.proc_exit\n\
                 `infs run` supplies no host functions and does not stand in for an embedder, \
                 not even with the WASI functions the wasmtime CLI provides on its own, so it \
                 executes no artifact that imports a function. Run the program from your \
                 embedder, which supplies these functions. See the book's External Functions \
                 and WASM Linking chapter (\"Running a program that binds host imports\") for \
                 how an embedder registers them.",
                Path::new("out").join("main.wasm").display()
            )
        );

        let msg = import_refusal_of(&module_with_imports(&[proc_exit]))
            .expect_err("a WASI import is refused like any other")
            .to_string();
        for fragment in [
            "imports 1 function that its embedder must supply.\n  \
             wasi_snapshot_preview1.proc_exit\n",
            "which supplies this function.",
            "for how an embedder registers it.",
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

    /// A verification construct in the artifact is refused before wasmtime sees
    /// it, naming the construct, in the clause `[build.wasm-opt]` refuses the
    /// same find with. A backstop — the analyzer keeps such constructs out of a
    /// compile-mode build — so no end-to-end route reaches it.
    #[test]
    fn the_import_refusal_refuses_a_leaked_verification_construct() {
        let uzumaki = module_with_raw_body(&[0x00, 0xfc, 0x31, 0x1a, 0x0b]);
        let msg = import_refusal_of(&uzumaki)
            .expect_err("a leaked construct is refused")
            .to_string();
        assert_eq!(
            msg,
            format!(
                "`infs run` cannot execute this program: {} contains the verification-only \
                 construct `i32.uzumaki`, which wasmtime cannot decode. Verification \
                 constructs (forall/exists/assume/unique and `@`/uzumaki) belong in `spec` \
                 blocks, which compile-mode builds strip; move the construct into a `spec` \
                 block.",
                Path::new("out").join("main.wasm").display()
            )
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
