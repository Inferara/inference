#![warn(clippy::pedantic)]

//! # Inference Compiler CLI (infc)
//!
//! Standalone command line interface for the Inference programming language compiler.
//!
//! This is the legacy compiler CLI. For most users, the unified `infs` toolchain
//! CLI is recommended. Use `infc` directly when you need fine-grained control over
//! compilation phases or are integrating Inference compilation into build systems.
//!
//! ## Compilation Phases
//!
//! The Inference compiler operates in three distinct phases:
//!
//! 1. **Parse** (`--parse`) – Builds the typed AST using the custom parser
//!    - Reads the source file
//!    - Runs the Inference parser
//!    - Constructs arena-allocated AST nodes
//!    - Validates syntax and basic structure
//!    - Reports parsing errors if any
//!
//! 2. **Analyze** (`--analyze`) – Performs type checking and semantic validation
//!    - Type inference and checking
//!    - Symbol resolution
//!    - Semantic validation
//!    - Reports type errors and semantic issues
//!
//! 3. **Codegen** (`--codegen`) – Emits WebAssembly binary
//!    - Generates WebAssembly binary from typed AST
//!    - Supports non-deterministic instructions (uzumaki, forall, exists, assume, unique)
//!    - Optionally translates to Rocq (.v) format for formal verification
//!
//! ## Phase Execution
//!
//! Phases execute in canonical order (parse → analyze → codegen) regardless of
//! the order flags appear on the command line. Each phase depends on the previous:
//!
//! - `--parse` runs standalone
//! - `--analyze` automatically runs parse first
//! - `--codegen` automatically runs parse and analyze first
//!
//! ## Default Behavior
//!
//! When no phase flags are given, `infc` defaults to full compilation and writes
//! the WASM binary to disk — equivalent to `--codegen -o`. This matches
//! conventional compiler UX (e.g. `gcc foo.c`).
//!
//! ```bash
//! infc example.inf              # parse → codegen → write out/example.wasm
//! infc example.inf -v           # implies --mode proof → both out/example.wasm and out/example.v
//! infc example.inf --mode proof # proof mode (keeps specs); implies -v → writes both files
//! infc example.inf --mode compile -v # opt back into stripped-spec V output
//! ```
//!
//! Supplying any explicit phase flag overrides the default:
//!
//! ```bash
//! infc example.inf --parse    # parse only, no output files
//! infc example.inf --analyze  # parse + analyze only, no output files
//! ```
//!
//! ## Output Artifacts
//!
//! By default, all output files are written to an `out/` directory relative to
//! the current working directory:
//!
//! - `out/<source_name>.wasm` – WebAssembly binary (when `-o` is specified)
//! - `out/<source_name>.v` – Rocq translation (when `-v` is specified)
//!
//! The `--out-dir <path>` flag overrides the directory (still relative to CWD
//! unless an absolute path is given); it applies to both the `.wasm` and the
//! `.v`. The output directory is created automatically if it doesn't exist.
//!
//! ## Error Handling
//!
//! The compiler reports errors to stderr with descriptive messages:
//!
//! - **Parse errors**: Syntax errors, malformed AST nodes
//! - **Type errors**: Type mismatches, undefined symbols
//! - **Codegen errors**: WebAssembly generation failures
//! - **IO errors**: File not found, permission issues
//!
//! All errors cause the process to exit with code 1.
//!
//! ## Exit Codes
//!
//! | Code | Meaning                                    |
//! |------|--------------------------------------------|
//! | 0    | Success - all requested phases completed   |
//! | 1    | Failure - usage, IO, or compilation error  |
//!
//! ## Examples
//!
//! Parse and validate syntax:
//! ```bash
//! infc example.inf --parse
//! ```
//!
//! Type check without generating code:
//! ```bash
//! infc example.inf --analyze
//! ```
//!
//! Full compilation to WebAssembly (default — no flags needed):
//! ```bash
//! infc example.inf
//! ```
//!
//! Compile and generate Rocq translation:
//! ```bash
//! infc example.inf -v
//! ```
//!
//! Full compilation with explicit flags (equivalent to the default):
//! ```bash
//! infc example.inf --codegen -o
//! ```
//!
//! Only generate Rocq (no WASM file):
//! ```bash
//! infc example.inf --codegen -v
//! ```
//!
//! ## Relationship to `infs`
//!
//! The Inference ecosystem provides two CLI tools:
//!
//! - **`infc`** (this binary) - Standalone compiler
//! - **`infs`** - Unified toolchain CLI with project management and toolchain installation
//!
//! See `apps/infs/README.md` for the full-featured toolchain interface.
//!
//! ## Current Limitations
//!
//! - Single-file compilation only (multi-file projects not yet supported)
//! - Output directory defaults to `out/` relative to CWD (not the source file
//!   location); override with `--out-dir <path>`
//! - Analysis phase is work-in-progress
//!
//! ## Tests
//!
//! Integration tests in `tests/cli_integration.rs` verify:
//! - Flag validation and error handling
//! - Phase execution correctness
//! - Output file generation
//! - Error message formatting
//!
//! See `README.md` in this crate for comprehensive usage documentation.

mod parser;
pub(crate) mod toolchain;
use clap::Parser;
use inference::wasm_link::{
    resolve_external_modules, ManifestDeps, ResolvedExternalModule, SearchPath,
};
use inference::{analyze, link, parse_project, type_check, wasm_to_v};
use inference_wasm_codegen::EmitFeatures;
use parser::{Cli, CliMode};
use std::{
    fs,
    path::PathBuf,
    process::{self},
};
use toolchain::BuildProfile;

/// Environment variable holding a `PATH`-style list of directories to search
/// for external `.wasm` modules, after any `-L` directories.
const WASM_LIB_PATH_ENV: &str = "INFERENCE_WASM_LIB_PATH";

/// Builds the manifest-declared dependency map from `--wasm-dep <name>=<path>`
/// entries.
///
/// `infs build` forwards one entry per `Inference.toml [wasm-dependencies]`
/// declaration; these bind a logical module name directly to a `.wasm` file and
/// take precedence over every search directory. A malformed entry (no `=`, or an
/// empty name) is a hard error so a typo never silently falls through to the
/// search path.
fn parse_manifest_deps(entries: &[String]) -> anyhow::Result<ManifestDeps> {
    let mut deps = ManifestDeps::new();
    for entry in entries {
        let (name, path) = entry.split_once('=').ok_or_else(|| {
            anyhow::anyhow!("invalid --wasm-dep `{entry}`: expected `<name>=<path>`")
        })?;
        if name.is_empty() {
            anyhow::bail!("invalid --wasm-dep `{entry}`: module name is empty");
        }
        deps.insert(name, PathBuf::from(path));
    }
    Ok(deps)
}

/// Resolves and validates every external `.wasm` module the program binds.
///
/// Resolution priority, highest first:
/// 1. manifest dependencies (`--wasm-dep`, forwarded from
///    `Inference.toml [wasm-dependencies]`),
/// 2. `-L` / `--wasm-lib-dir` directories,
/// 3. `INFERENCE_WASM_LIB_PATH` environment directories.
fn resolve_externals(
    typed_context: &inference::TypedContext,
    lib_dirs: &[PathBuf],
    manifest_deps: &ManifestDeps,
) -> anyhow::Result<Vec<ResolvedExternalModule>> {
    let mut search_path = SearchPath::new();
    for dir in lib_dirs {
        search_path.push_lib_dir(dir.clone());
    }
    if let Some(env_path) = std::env::var_os(WASM_LIB_PATH_ENV) {
        for dir in env_search_dirs(&env_path) {
            search_path.push_env_dir(dir);
        }
    }
    Ok(resolve_external_modules(
        typed_context,
        &search_path,
        Some(manifest_deps),
    )?)
}

/// Splits an `INFERENCE_WASM_LIB_PATH`-style value into search directories,
/// dropping empty entries.
///
/// An empty entry (a leading/trailing/interior separator, or a wholly-empty
/// value) would otherwise yield an empty `PathBuf` whose `join(relative)`
/// resolves against the process CWD — silently turning the build directory into
/// a `.wasm` search root. Dropping it makes `""` and `":"` behave exactly like
/// the variable being unset.
fn env_search_dirs(env_path: &std::ffi::OsStr) -> Vec<PathBuf> {
    std::env::split_paths(env_path)
        .filter(|dir| !dir.as_os_str().is_empty())
        .collect()
}

/// Applies default phase normalization to parsed CLI arguments.
///
/// When no phase flag (`--parse`, `--analyze`, `--codegen`) is given, defaults
/// to full pipeline + WASM output — equivalent to `--codegen -o`.
///
/// Mode/`-v` resolution rules (symmetric):
/// - `--mode proof` implies `-v` because the `.v` artifact is what proof mode
///   is for; emitting only `.wasm` in proof mode would silently waste the
///   unoptimized spec preservation work.
/// - `-v` with no explicit `--mode` implies `--mode proof` because `compile`
///   mode strips spec functions and would produce a near-empty `.v` (no
///   per-spec definitions or theorems). Users who legitimately want V output
///   from a spec-stripped WASM can pass `--mode compile -v` explicitly.
///
/// After this function, `args.mode` is always `Some(..)`.
pub(crate) fn normalize_args(args: &mut Cli) {
    // Detect explicit proof-mode combined with a non-codegen phase BEFORE the
    // default-normalization runs, so we can warn that the .v output will not
    // be produced. The warning is purely informational; exit code is unchanged.
    if matches!(args.mode, Some(CliMode::Proof)) && (args.parse || args.analyze) && !args.codegen {
        let flag = if args.parse { "--parse" } else { "--analyze" };
        eprintln!("warning: --mode proof is ignored when {flag} is set; no .v will be written");
    }
    if !args.parse && !args.analyze && !args.codegen {
        args.codegen = true;
        args.generate_wasm_output = true;
    }
    let effective_mode = match (args.mode, args.generate_v_output) {
        (Some(m), _) => m,
        (None, true) => CliMode::Proof,
        (None, false) => CliMode::Compile,
    };
    args.mode = Some(effective_mode);
    if matches!(effective_mode, CliMode::Proof) {
        args.generate_v_output = true;
    }
}

/// Resolves the `--wasm-features` entries into the emission flags code generation
/// takes.
///
/// Validation is [`inference_compiler_interface::resolve_wasm_features`] — the
/// same vocabulary and the same wording `infs` uses for the manifest key, so a
/// name rejected in one place is rejected identically in the other. Its
/// `WasmFeatureError` carries the whole diagnostic, so this surfaces it unchanged.
///
/// The mapping is an exhaustive match with no wildcard arm: a feature name cannot
/// be added to the shared vocabulary without a codegen effect being decided for
/// it here, which is why there is no "recognized but unsupported" state to
/// report.
///
/// # Errors
///
/// Returns the shared diagnostic for the first entry that is not a valid,
/// not-yet-seen feature name.
fn resolve_emit_features(entries: &[String]) -> anyhow::Result<EmitFeatures> {
    use inference_compiler_interface::{WasmFeatureName, WasmFeatureSource};

    let requested =
        inference_compiler_interface::resolve_wasm_features(entries, WasmFeatureSource::Flag)?;
    let mut features = EmitFeatures::default();
    for name in requested {
        match name {
            WasmFeatureName::BulkMemory => features.bulk_memory = true,
        }
    }
    Ok(features)
}

/// Renders a `wasm_to_v` failure with the user-facing diagnostic shape
/// described in plan §6: a dedicated message for Rocq-stdlib shadowing,
/// dedicated guidance for the `__` collision, and a generic invalid-Rocq-identifier
/// fallthrough for the remaining reasons.
///
/// The rejected name can be either the source-derived module name OR a spec
/// name declared in the source (since `translate()` now validates each spec
/// name up-front). The diagnostic uses neutral phrasing because the CLI does
/// not currently have a way to tell which source the name came from —
/// labelling it "source filename" when the offender was a spec name was a
/// wrong guess.
fn eprint_translation_error(e: &anyhow::Error) {
    use inference::{InvalidIdentifierReason, WasmToVError};
    if let Some(wte) = e.downcast_ref::<WasmToVError>() {
        match wte {
            WasmToVError::RocqStdlibShadow { name } => {
                eprintln!(
                    "error: '{name}' would shadow the Rocq stdlib type '{name}'. \
                     Rename the source file or spec to avoid the collision (e.g. \
                     'list_ops', 'my_list')."
                );
                return;
            }
            WasmToVError::InvalidRocqIdentifier {
                name,
                reason: InvalidIdentifierReason::ContainsDoubleUnderscore,
            } => {
                eprintln!(
                    "error: '{name}' contains '__' which is reserved as the \
                     module/spec name separator in the emitted Rocq output. \
                     Use a single underscore or a different name."
                );
                return;
            }
            WasmToVError::InvalidRocqIdentifier {
                reason: InvalidIdentifierReason::EmptyName,
                ..
            } => {
                eprintln!(
                    "error: empty Rocq identifier — the source filename has no \
                     usable stem (e.g. \".inf\" with no name), or a spec block has \
                     no name.\n\n  Rename the source file."
                );
                return;
            }
            WasmToVError::InvalidRocqIdentifier { name, reason } => {
                eprintln!(
                    "error: '{name}' is not a valid Rocq identifier.\n\n  \
                     A Rocq identifier (used for both module names and spec \
                     names) must:\n    \
                     - start with a letter (A-Z or a-z)\n    \
                     - contain only letters, digits, and underscores\n    \
                     - not contain '__' (reserved as the module/spec name separator)\n    \
                     - not collide with Rocq stdlib types or reserved keywords\n\n  \
                     Rename the source file or the spec block (e.g. 'list_utils') \
                     and re-run.\n  (specifically: {reason})"
                );
                return;
            }
            WasmToVError::EmbeddedSpecMismatch { .. } => {
                eprintln!(
                    "error: internal inconsistency — the codegen-emitted spec map \
                     and the embedded `inference.spec_funcs` section disagree.\n\n  \
                     This is a compiler bug; please file an issue with the .inf \
                     source attached."
                );
                return;
            }
            WasmToVError::SpecNameReservesSeparator {
                offender_kind,
                offender,
                joined,
                fix_hint,
            } => {
                eprint_spec_join_boundary_error(offender_kind, offender, joined, fix_hint);
                return;
            }
            WasmToVError::WasmParse(msg) => {
                eprintln!(
                    "error: malformed WebAssembly binary: {msg}\n\n  \
                     The WASM input could not be parsed. If this binary was \
                     produced by `infc`, please file a bug. If it came from \
                     another source, the file may be corrupted or use an \
                     unsupported extension."
                );
                return;
            }
            WasmToVError::UnsupportedFeature { description } => {
                eprintln!(
                    "error: this module cannot be translated to Rocq: {description}\n\n  \
                     The proof model a .v targets describes a subset of \
                     WebAssembly, and this construct falls outside it. That is a \
                     property of the model rather than unfinished work, so no \
                     flag enables it.\n\n  \
                     The module can still be compiled and run — drop '-v' (and \
                     any explicit '--mode proof') to build the .wasm without a \
                     proof artifact."
                );
                return;
            }
            // WasmToVError is #[non_exhaustive]; the wildcard handles future
            // variants by falling through to the generic message below.
            _ => {}
        }
    }
    eprintln!("WASM->V translation failed: {e}");
}

/// Renders the educational diagnostic for a spec/module name whose trailing `_`
/// fabricates Rocq's reserved `__` separator when joined into the proof grammar.
/// The fix differs by which component offended (rename the source file vs. the
/// spec block); both are surfaced as a concrete rename.
fn eprint_spec_join_boundary_error(
    offender_kind: &str,
    offender: &str,
    joined: &str,
    fix_hint: &str,
) {
    let rename = if offender_kind == "output module name" {
        format!("Rename the source file: '{offender}.inf' -> '{fix_hint}.inf'.")
    } else {
        format!("Rename the spec: 'spec {offender}' -> 'spec {fix_hint}'.")
    };
    eprintln!(
        "error: the {offender_kind} '{offender}' ends with '_', so it joins \
         into the reserved '__' run in the Rocq proof name '{joined}'.\n\n  \
         The emitted proof grammar is '<module>__<spec>_specs', where '__' \
         separates the module from the spec; a trailing '_' on either side \
         fabricates that separator. {rename}\n\n  \
         Why not auto-encode: proof-mode names appear verbatim in your .v \
         file, so they are kept readable rather than escaped into noise."
    );
}

/// Removes any pre-existing output artifacts a prior build left, so a compile
/// that is later rejected leaves no runnable stale file behind, and a plain
/// compile does not leave a stale proof describing a since-changed program.
///
/// Both the `.wasm` and the `.v` are cleared together whenever the run will write
/// *at least one* artifact, regardless of which one. A run that writes any output
/// recompiles this source name and may be rejected at codegen or `wasm_to_v`; the
/// would-be `.wasm` and `.v` for that name are stale the moment such a run starts,
/// so a leftover from an earlier build must not survive — a `--codegen -v` run
/// (which writes only `.v`) must still invalidate the `.wasm` an earlier
/// `--codegen -o` or default build wrote, or a rejection would leave a runnable
/// artifact describing the old program. The success path rewrites whichever
/// artifacts this invocation requests, so clearing both up front costs nothing
/// there. A no-output dry run (`--codegen` with neither `-o` nor `-v`) writes
/// nothing, so its caller does not invoke this — a dry run leaves existing
/// artifacts untouched.
///
/// A missing file is not an error (nothing to clear); a removal failure is
/// ignored because the subsequent write would surface any genuine IO problem
/// with a precise message, and a transient failure must not abort an
/// otherwise-valid build. The directory itself is left untouched — it is created
/// on the success path exactly as before.
fn clear_stale_outputs(output_dir: &std::path::Path, source_fname: &str) {
    let _ = fs::remove_file(output_dir.join(format!("{source_fname}.wasm")));
    let _ = fs::remove_file(output_dir.join(format!("{source_fname}.v")));
}

/// Runs the compiler driver on an explicitly sized stack.
///
/// The compiler's phases recurse with the input's syntactic nesting depth, and the
/// platform's default main-thread stack is too small to survive input the front end
/// is expected to accept. Exit codes are what they were: `process::exit` terminates
/// the process identically from the scoped worker thread, and a panic inside the
/// driver is re-raised on the main thread with its original payload, printed once.
/// The only stderr difference is that a panic header now names the compile thread
/// rather than `main`.
fn main() {
    inference::with_compiler_stack(run);
}

/// The Inference compiler CLI driver, run by [`main`] on a compiler-sized stack.
///
/// ## Execution Flow
///
/// 1. **Parse command line arguments** using clap
/// 2. **Validate input**: verify source file exists
/// 3. **Apply default normalization**: when no phase flags are given, defaults
///    to full pipeline (`--codegen -o`) so that `infc file.inf` just works
/// 4. **Execute compilation phases** in canonical order:
///    - Parse: Build typed AST from source using the custom parser
///    - Analyze: Type check and semantic validation
///    - Codegen: Generate WebAssembly binary from typed AST
/// 5. **Generate output files** (if requested):
///    - Write WASM binary with `-o` flag (set by default when no flags given)
///    - Write Rocq translation with `-v` flag
///
/// ## Error Handling
///
/// All errors are reported to stderr with descriptive messages and cause
/// process exit with code 1. Error categories:
///
/// - **Usage errors**: Invalid arguments
/// - **IO errors**: File not found, permission denied, output write failures
/// - **Compilation errors**: Parse errors, type errors, codegen failures
///
/// ## Phase Coordination
///
/// The function ensures correct phase dependencies:
/// - Parse phase always runs first when any phase is requested
/// - Analyze phase requires parse output (typed AST)
/// - Codegen phase requires analyze output (typed context)
///
/// Phase outputs are stored in `Option` types and unwrapped only when
/// guaranteed to be present by prior validation logic.
///
/// ## Output Management
///
/// Output files are written to the output directory (`out/` by default, relative
/// to CWD; overridable via `--out-dir <path>`):
/// - Directory is created if it doesn't exist
/// - File names are derived from source file stem
/// - Both `-o` and `-v` flags can be used simultaneously
/// - `--out-dir` redirects both the `.wasm` and the `.v`
///
/// ## Implementation Notes
///
/// - Uses `anyhow::Result` for error propagation from library functions
/// - Calls `process::exit(1)` explicitly on errors (no panics)
/// - Reads entire source file into memory (limitation: no streaming)
/// - Phase execution is sequential (no parallelization)
#[allow(clippy::too_many_lines)]
fn run() {
    let mut args = Cli::parse();

    if args.commit_hash {
        println!("{}", env!("INFC_GIT_COMMIT"));
        process::exit(0);
    }

    if args.abi_version {
        println!(
            "{}.{}",
            inference_compiler_interface::COMPILER_ABI_MAJOR,
            inference_compiler_interface::COMPILER_ABI_MINOR,
        );
        process::exit(0);
    }

    let Some(path) = args.path.clone() else {
        eprintln!("Error: source file argument required");
        process::exit(1);
    };
    if !path.exists() {
        eprintln!("Error: path not found");
        process::exit(1);
    }

    normalize_args(&mut args);

    // Resolve the requested instruction set before any phase runs: a misspelled
    // feature is a mistake about the artifact, and reporting it after a full
    // parse and type check would bury it under work the user has to discard
    // anyway.
    let emit_features = match resolve_emit_features(&args.wasm_features) {
        Ok(features) => features,
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    };

    let output_path = args
        .out_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("out"));
    let need_parse = args.parse;
    let need_analyze = args.analyze;
    let need_codegen = args.codegen;

    let source_fname = path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("module")
        .to_string();

    // Clear any artifact a previous build left in the output directory before
    // this build runs, so a compile that is later rejected (by type check,
    // analysis, external resolution, codegen, or `wasm_to_v`) never leaves a
    // runnable stale `.wasm` (or its `.v`) on disk for `wasmtime` to execute.
    // Both artifacts are cleared whenever this run will write at least one of
    // them — independent of which one — because the run recompiles this source
    // name and any leftover for it is already stale; a `--codegen -v` run that
    // writes only `.v` must still drop an earlier build's `.wasm`. Clearing up
    // front means every rejection path exits with no artifact without each
    // `process::exit(1)` site having to clean up. A run that writes no output —
    // a parse/analyze-only run, or a `--codegen` dry run with neither `-o` nor
    // `-v` — must not disturb a previous build's artifacts, so clearing is gated
    // on this run actually emitting something.
    if need_codegen && (args.generate_wasm_output || args.generate_v_output) {
        clear_stale_outputs(&output_path, &source_fname);
    }

    let mut t_ast = None;
    if need_codegen || need_analyze || need_parse {
        // Drive the multi-file front end. A single file with no path-form `use`
        // imports yields a one-file arena identical to the legacy single-file
        // parse, so existing single-file behavior is preserved; reachable
        // imported files are folded into the same arena.
        match parse_project(&path) {
            Ok(project) => {
                println!("Parsed: {}", path.display());
                for warning in &project.warnings {
                    eprintln!("{warning}");
                }
                t_ast = Some(project.arena);
            }
            Err(e) => {
                eprintln!("Parse error: {e}");
                process::exit(1);
            }
        }
    }

    let Some(arena) = t_ast else {
        eprintln!("Internal error: parse phase did not produce AST");
        process::exit(1);
    };

    let mut typed_context = None;

    if need_codegen || need_analyze {
        match type_check(arena) {
            Err(e) => {
                eprintln!("Type checking failed: {e}");
                process::exit(1);
            }
            Ok(tctx) => {
                match analyze(&tctx) {
                    Err(e) => {
                        eprintln!("{e}");
                        process::exit(1);
                    }
                    Ok(result) => {
                        if result.has_findings() {
                            eprintln!("{result}");
                        }
                    }
                }
                typed_context = Some(tctx);
                println!("Analyzed: {}", path.display());
            }
        }
    }

    // Resolve every external `.wasm` the program binds, ahead of codegen, so a
    // resolution or validation failure aborts before any output is produced.
    let manifest_deps = match parse_manifest_deps(&args.wasm_deps) {
        Ok(deps) => deps,
        Err(e) => {
            eprintln!("External module resolution failed: {e}");
            process::exit(1);
        }
    };
    let external_modules = match &typed_context {
        Some(tctx) if need_codegen => {
            match resolve_externals(tctx, &args.wasm_lib_dirs, &manifest_deps) {
                Ok(modules) => modules,
                Err(e) => {
                    eprintln!("External module resolution failed: {e}");
                    process::exit(1);
                }
            }
        }
        _ => Vec::new(),
    };
    if need_codegen {
        let Some(tctx) = typed_context else {
            eprintln!("Internal error: type check phase did not produce typed context");
            process::exit(1);
        };
        let profile = BuildProfile::default();
        let target = inference_wasm_codegen::Target::default();
        let mode: inference_wasm_codegen::CompilationMode =
            args.mode.unwrap_or(CliMode::Compile).into();
        let opt_level = profile.resolve_opt_level(target, mode);
        let source_fname = source_fname.as_str();
        let codegen_output = match inference_wasm_codegen::codegen(
            &tctx,
            source_fname,
            inference_wasm_codegen::CodegenOptions {
                target,
                mode,
                opt_level,
                features: emit_features,
            },
        ) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("Codegen failed: {e}");
                process::exit(1);
            }
        };
        println!("Codegen complete");

        // Fold the resolved external modules into the codegen output: a single
        // self-contained module with no cross-module imports. Each external is
        // paired with the logical module it was bound under, so the merge
        // matches each import's recorded `(module, field)` against the right
        // external. With no externs this is a byte-identical pass-through.
        let external_bytes: Vec<(&str, &[u8])> = external_modules
            .iter()
            .map(|m| (m.logical_module.as_str(), m.bytes.as_slice()))
            .collect();
        let wasm_owned = match link(codegen_output.wasm(), &external_bytes) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("Linking external modules failed: {e}");
                process::exit(1);
            }
        };
        if !external_modules.is_empty() {
            println!("Linked {} external module(s)", external_modules.len());
        }
        let wasm_bytes = wasm_owned.as_slice();

        // Run the Rocq translation *before* writing any file: a `wasm_to_v`
        // rejection (e.g. a spec or file named after a Rocq stdlib type) must not
        // leave a runnable `.wasm` on disk at a non-zero exit. The translation
        // output is held in memory and the artifacts are written only once the
        // whole requested pipeline has succeeded, so every rejection path exits
        // with no partial artifact. When `-v` is not requested this is skipped and
        // the `.wasm` write below is the first and only output step.
        let v_output = if args.generate_v_output {
            // The spec-function indices codegen records are in the *pre-link*
            // space; the linker rewrote the embedded `inference.spec_funcs`
            // section into the post-link space. When externals were merged the
            // pre-link map is stale, so defer entirely to the embedded post-link
            // section (an empty explicit map makes the translator adopt it).
            // With no externals the merge is a byte-identical pass-through and
            // the explicit map still cross-checks against the embedded one.
            let empty_spec_funcs = inference::FxHashMap::default();
            let explicit_spec_funcs = if external_modules.is_empty() {
                codegen_output.spec_func_indices_by_spec()
            } else {
                &empty_spec_funcs
            };
            // Same policy for the `inference.hspecs` obligations as for
            // `inference.spec_funcs`: with externals the pre-link map is stale
            // (the linker rewrote the embedded section), so pass an empty map
            // and defer to the embedded post-link section; without externals
            // the merge is a byte-identical pass-through and the explicit map
            // still cross-checks against the embedded one.
            let empty_hspecs = inference::HSpecMap::default();
            let explicit_hspecs = if external_modules.is_empty() {
                codegen_output.hspecs()
            } else {
                &empty_hspecs
            };
            match wasm_to_v(
                source_fname,
                wasm_bytes,
                explicit_spec_funcs,
                explicit_hspecs,
            ) {
                Ok(v) => Some(v),
                Err(e) => {
                    eprint_translation_error(&e);
                    process::exit(1);
                }
            }
        } else {
            None
        };

        if args.generate_wasm_output {
            let wasm_file_path = output_path.join(format!("{source_fname}.wasm"));
            if let Err(e) = fs::create_dir_all(&output_path) {
                eprintln!("Failed to create output directory: {e}");
                process::exit(1);
            }
            if let Err(e) = fs::write(&wasm_file_path, wasm_bytes) {
                eprintln!("Failed to write WASM file: {e}");
                process::exit(1);
            }
            println!("WASM generated at: {}", wasm_file_path.to_string_lossy());
        }
        if let Some(v_output) = v_output {
            let v_file_path = output_path.join(format!("{source_fname}.v"));
            if let Err(e) = fs::create_dir_all(&output_path) {
                eprintln!("Failed to create output directory: {e}");
                process::exit(1);
            }
            if let Err(e) = fs::write(&v_file_path, v_output) {
                eprintln!("Failed to write V file: {e}");
                process::exit(1);
            }
            println!("V generated at: {}", v_file_path.to_string_lossy());
        }
    }
    process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn make_args(parse: bool, analyze: bool, codegen: bool) -> Cli {
        Cli {
            path: Some(PathBuf::from("test.inf")),
            out_dir: None,
            parse,
            analyze,
            codegen,
            generate_wasm_output: false,
            generate_v_output: false,
            mode: None,
            wasm_lib_dirs: Vec::new(),
            wasm_deps: Vec::new(),
            wasm_features: Vec::new(),
            commit_hash: false,
            abi_version: false,
        }
    }

    #[test]
    fn normalize_sets_full_pipeline_when_no_flags() {
        let mut args = make_args(false, false, false);
        normalize_args(&mut args);
        assert!(args.codegen);
        assert!(args.generate_wasm_output);
        assert!(!args.generate_v_output);
    }

    #[test]
    fn normalize_does_not_override_explicit_parse() {
        let mut args = make_args(true, false, false);
        normalize_args(&mut args);
        assert!(!args.codegen);
        assert!(!args.generate_wasm_output);
    }

    #[test]
    fn normalize_does_not_override_explicit_analyze() {
        let mut args = make_args(false, true, false);
        normalize_args(&mut args);
        assert!(!args.codegen);
    }

    #[test]
    fn normalize_does_not_override_explicit_codegen() {
        let mut args = make_args(false, false, true);
        normalize_args(&mut args);
        assert!(args.codegen);
        assert!(!args.generate_wasm_output);
    }

    #[test]
    fn normalize_proof_mode_implies_v_output() {
        let mut args = make_args(false, false, false);
        args.mode = Some(CliMode::Proof);
        normalize_args(&mut args);
        assert!(
            args.codegen,
            "proof mode should still trigger default codegen"
        );
        assert!(
            args.generate_wasm_output,
            "proof mode should still emit wasm"
        );
        assert!(
            args.generate_v_output,
            "proof mode must imply -v so the .v artifact is written"
        );
        assert_eq!(
            args.mode,
            Some(CliMode::Proof),
            "explicit proof mode must be preserved"
        );
    }

    #[test]
    fn normalize_dash_v_implies_proof_mode() {
        let mut args = make_args(false, false, false);
        args.generate_v_output = true;
        normalize_args(&mut args);
        assert_eq!(
            args.mode,
            Some(CliMode::Proof),
            "-v alone must promote effective mode to proof so specs survive codegen"
        );
        assert!(args.generate_v_output);
    }

    #[test]
    fn normalize_explicit_compile_plus_v_keeps_compile() {
        let mut args = make_args(false, false, false);
        args.mode = Some(CliMode::Compile);
        args.generate_v_output = true;
        normalize_args(&mut args);
        assert_eq!(
            args.mode,
            Some(CliMode::Compile),
            "explicit --mode compile must not be overridden by -v"
        );
        assert!(
            args.generate_v_output,
            "explicit -v must be preserved even in compile mode"
        );
    }

    #[test]
    fn normalize_no_flags_resolves_mode_to_compile() {
        let mut args = make_args(false, false, false);
        normalize_args(&mut args);
        assert_eq!(
            args.mode,
            Some(CliMode::Compile),
            "absence of --mode and -v must resolve to compile"
        );
        assert!(!args.generate_v_output);
    }

    /// Returns the path to the test data directory.
    #[allow(dead_code)]
    pub(crate) fn get_test_data_path() -> std::path::PathBuf {
        let current_dir = std::env::current_dir().unwrap();
        current_dir
            .parent() // inference
            .unwrap()
            .join("test_data")
    }

    /// Returns the path to the output directory for test artifacts.
    #[allow(dead_code)]
    fn get_out_path() -> std::path::PathBuf {
        get_test_data_path().parent().unwrap().join("out")
    }

    #[test]
    fn parse_manifest_deps_binds_name_to_path() {
        let deps =
            parse_manifest_deps(&["arith=/libs/arith.wasm".to_string()]).expect("should parse");
        assert_eq!(deps.get("arith"), Some(Path::new("/libs/arith.wasm")));
    }

    #[test]
    fn parse_manifest_deps_accepts_multiple_entries() {
        let deps = parse_manifest_deps(&[
            "arith=/libs/arith.wasm".to_string(),
            "crypto=/vendor/sha256.wasm".to_string(),
        ])
        .expect("should parse");
        assert_eq!(deps.get("arith"), Some(Path::new("/libs/arith.wasm")));
        assert_eq!(deps.get("crypto"), Some(Path::new("/vendor/sha256.wasm")));
    }

    #[test]
    fn parse_manifest_deps_preserves_path_with_equals() {
        // Only the first `=` separates name from path; later ones belong to the
        // path so values like `a=b=c` survive intact.
        let deps = parse_manifest_deps(&["arith=/odd=dir/arith.wasm".to_string()])
            .expect("should parse");
        assert_eq!(deps.get("arith"), Some(Path::new("/odd=dir/arith.wasm")));
    }

    #[test]
    fn parse_manifest_deps_rejects_missing_separator() {
        let err = parse_manifest_deps(&["arith".to_string()]).unwrap_err();
        assert!(err.to_string().contains("expected `<name>=<path>`"));
    }

    #[test]
    fn parse_manifest_deps_rejects_empty_name() {
        let err = parse_manifest_deps(&["=/libs/arith.wasm".to_string()]).unwrap_err();
        assert!(err.to_string().contains("module name is empty"));
    }

    #[test]
    fn parse_manifest_deps_empty_input_yields_empty_map() {
        let deps = parse_manifest_deps(&[]).expect("should parse");
        assert!(deps.get("anything").is_none());
    }

    #[test]
    fn empty_wasm_lib_path_resolves_like_unset() {
        // H5: a wholly-empty value, and a lone separator, must each yield zero
        // search directories — identical to the variable being unset — rather
        // than injecting the process CWD as a silent `.wasm` search root.
        use std::ffi::OsString;

        let empty = env_search_dirs(&OsString::from(""));
        assert!(empty.is_empty(), "empty value yields no dirs: {empty:?}");

        // A lone PATH list separator (`:` on Unix, `;` on Windows) splits into
        // two empty entries; both must be dropped.
        let list_sep = if cfg!(windows) { ";" } else { ":" };
        let bare = env_search_dirs(&OsString::from(list_sep));
        assert!(bare.is_empty(), "a lone list separator yields no dirs: {bare:?}");
    }

    #[test]
    fn wasm_lib_path_keeps_real_dirs_and_drops_empties() {
        // `"/real/dir<sep>"` (a trailing separator) must keep the real directory
        // and drop only the empty trailing entry.
        use std::ffi::OsString;

        let list_sep = if cfg!(windows) { ";" } else { ":" };
        let value = OsString::from(format!("real{list_sep}"));
        let dirs = env_search_dirs(&value);
        assert_eq!(dirs, [PathBuf::from("real")]);
    }

    fn features(raw: &[&str]) -> anyhow::Result<EmitFeatures> {
        resolve_emit_features(&raw.iter().map(|s| (*s).to_string()).collect::<Vec<_>>())
    }

    fn feature_error(raw: &[&str]) -> String {
        features(raw)
            .expect_err("the request must be rejected")
            .to_string()
    }

    #[test]
    fn no_wasm_features_flag_emits_wasm_1_0() {
        assert_eq!(features(&[]).unwrap(), EmitFeatures::default());
    }

    #[test]
    fn bulk_memory_sets_its_emission_flag() {
        assert_eq!(
            features(&["bulk-memory"]).unwrap(),
            EmitFeatures { bulk_memory: true }
        );
    }

    #[test]
    fn unknown_feature_is_rejected_with_the_shared_wording() {
        let err = feature_error(&["simd"]);
        assert!(err.contains("unknown WebAssembly feature"), "{err}");
        // The flag, not the manifest key, is what an `infc` caller must edit.
        assert!(err.contains("`--wasm-features`"), "{err}");
    }

    #[test]
    fn instruction_name_is_rejected_with_the_did_you_mean() {
        let err = feature_error(&["memory.fill"]);
        assert!(err.contains("is an instruction, not a feature"), "{err}");
        assert!(err.contains("write `bulk-memory`"), "{err}");
    }

    #[test]
    fn duplicate_feature_is_rejected() {
        let err = feature_error(&["bulk-memory", "bulk-memory"]);
        assert!(err.contains("listed more than once"), "{err}");
    }

    /// Both accepted spellings reach the same flags: `--wasm-features a,b` is the
    /// canonical form and repetition is accepted, so `infs` can forward one comma
    /// list without callers who repeat the flag getting different output.
    #[test]
    fn comma_list_and_repetition_parse_alike() {
        let comma = Cli::try_parse_from(["infc", "x.inf", "--wasm-features", "bulk-memory"])
            .expect("comma form parses");
        let repeated = Cli::try_parse_from([
            "infc",
            "x.inf",
            "--wasm-features",
            "bulk-memory",
            "--wasm-features",
            "bulk-memory",
        ])
        .expect("repetition parses");
        assert_eq!(comma.wasm_features, ["bulk-memory"]);
        assert_eq!(repeated.wasm_features, ["bulk-memory", "bulk-memory"]);
        assert_eq!(
            resolve_emit_features(&comma.wasm_features).unwrap(),
            EmitFeatures { bulk_memory: true }
        );
        // The same name twice is a duplicate however it was spelled.
        assert!(resolve_emit_features(&repeated.wasm_features).is_err());
    }

    #[test]
    fn comma_separated_entries_split_into_separate_names() {
        let cli = Cli::try_parse_from(["infc", "x.inf", "--wasm-features", "bulk-memory,simd"])
            .expect("a comma list parses into entries");
        assert_eq!(cli.wasm_features, ["bulk-memory", "simd"]);
    }
}
