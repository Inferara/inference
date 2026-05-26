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
//! 1. **Parse** (`--parse`) – Builds the typed AST using tree-sitter
//!    - Reads the source file
//!    - Runs tree-sitter parser with Inference grammar
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
//! All output files are written to an `out/` directory relative to the current
//! working directory:
//!
//! - `out/<source_name>.wasm` – WebAssembly binary (when `-o` is specified)
//! - `out/<source_name>.v` – Rocq translation (when `-v` is specified)
//!
//! The output directory is created automatically if it doesn't exist.
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
//! - Output directory is relative to CWD, not source file location
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
use inference::{analyze, parse, type_check, wasm_to_v};
use parser::{Cli, CliMode};
use std::{
    fs,
    path::PathBuf,
    process::{self},
};
use toolchain::BuildProfile;

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
    if matches!(args.mode, Some(CliMode::Proof))
        && (args.parse || args.analyze)
        && !args.codegen
    {
        let flag = if args.parse { "--parse" } else { "--analyze" };
        eprintln!(
            "warning: --mode proof is ignored when {flag} is set; no .v will be written"
        );
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
                    "error: this WebAssembly module uses a feature not yet \
                     supported by the Rocq translator: {description}\n\n  \
                     The codegen pipeline emits a subset of WASM that the \
                     translator recognises today; binaries from other toolchains \
                     may include features that have not yet been wired through."
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

/// Entry point for the Inference compiler CLI.
///
/// ## Execution Flow
///
/// 1. **Parse command line arguments** using clap
/// 2. **Validate input**: verify source file exists
/// 3. **Apply default normalization**: when no phase flags are given, defaults
///    to full pipeline (`--codegen -o`) so that `infc file.inf` just works
/// 4. **Execute compilation phases** in canonical order:
///    - Parse: Build typed AST from source using tree-sitter
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
/// Output files are written to `out/` directory relative to CWD:
/// - Directory is created if it doesn't exist
/// - File names are derived from source file stem
/// - Both `-o` and `-v` flags can be used simultaneously
///
/// ## Implementation Notes
///
/// - Uses `anyhow::Result` for error propagation from library functions
/// - Calls `process::exit(1)` explicitly on errors (no panics)
/// - Reads entire source file into memory (limitation: no streaming)
/// - Phase execution is sequential (no parallelization)
#[allow(clippy::too_many_lines)]
fn main() {
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

    let output_path = PathBuf::from("out");
    let need_parse = args.parse;
    let need_analyze = args.analyze;
    let need_codegen = args.codegen;

    let source_code = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading source file: {e}");
            process::exit(1);
        }
    };
    let mut t_ast = None;
    if need_codegen || need_analyze || need_parse {
        match parse(source_code.as_str()) {
            Ok(ast) => {
                println!("Parsed: {}", path.display());
                t_ast = Some(ast);
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
        let source_fname = path
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or("module");
        let codegen_output =
            match inference_wasm_codegen::codegen(&tctx, target, mode, opt_level, source_fname) {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("Codegen failed: {e}");
                    process::exit(1);
                }
            };
        println!("Codegen complete");

        let wasm_bytes = codegen_output.wasm();

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
        if args.generate_v_output {
            match wasm_to_v(
                source_fname,
                wasm_bytes,
                codegen_output.spec_func_indices_by_spec(),
            ) {
                Ok(v_output) => {
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
                Err(e) => {
                    eprint_translation_error(&e);
                    process::exit(1);
                }
            }
        }
    }
    process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_args(parse: bool, analyze: bool, codegen: bool) -> Cli {
        Cli {
            path: Some(PathBuf::from("test.inf")),
            parse,
            analyze,
            codegen,
            generate_wasm_output: false,
            generate_v_output: false,
            mode: None,
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
        assert!(args.codegen, "proof mode should still trigger default codegen");
        assert!(args.generate_wasm_output, "proof mode should still emit wasm");
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
}
