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
//!    - Supports non-deterministic instructions (uzumaki, forall, exists)
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
//! infc example.inf        # parse → codegen → write out/example.wasm
//! infc example.inf -v     # parse → codegen → write out/example.wasm + out/example.v
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
use parser::Cli;
use std::{
    fs,
    path::PathBuf,
    process::{self},
};
use toolchain::BuildProfile;

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
    if !args.path.exists() {
        eprintln!("Error: path not found");
        process::exit(1);
    }

    // Default normalization: no phase flags → full pipeline + WASM output.
    // This makes `infc file.inf` behave like `infc file.inf --codegen -o`.
    if !args.parse && !args.analyze && !args.codegen {
        args.codegen = true;
        args.generate_wasm_output = true;
    }

    let output_path = PathBuf::from("out");
    let need_parse = args.parse;
    let need_analyze = args.analyze;
    let need_codegen = args.codegen;

    let source_code = match fs::read_to_string(&args.path) {
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
                println!("Parsed: {}", args.path.display());
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
                typed_context = Some(tctx);
                if let Err(e) = analyze(typed_context.as_ref().unwrap()) {
                    eprintln!("Analysis failed: {e}");
                    process::exit(1);
                }
                println!("Analyzed: {}", args.path.display());
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
        let mode = inference_wasm_codegen::CompilationMode::default();
        let opt_level = profile.resolve_opt_level(target, mode);
        let codegen_output = match inference_wasm_codegen::codegen(&tctx, target, mode, opt_level) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("Codegen failed: {e}");
                process::exit(1);
            }
        };
        println!("Codegen complete");
        let source_fname = args
            .path
            .file_stem()
            .unwrap_or_else(|| std::ffi::OsStr::new("module"))
            .to_str()
            .unwrap();

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
            match wasm_to_v(source_fname, wasm_bytes) {
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
                    eprintln!("WASM->V translation failed: {e}");
                    process::exit(1);
                }
            }
        }
    }
    process::exit(0);
}

/// Unit test helpers for the CLI module.
///
/// Most CLI testing is done through integration tests in `tests/cli_integration.rs`
/// which spawn the actual binary. This module contains helper functions and
/// placeholder tests for future unit-level testing needs.
#[cfg(test)]
mod test {

    // Commented out test for WASM to Rocq translation.
    // This test is currently disabled as it depends on specific test data setup
    // and may fail in CI environments.
    //
    // #[test]
    // fn test_wasm_to_coq() {
    //     if std::env::var("GITHUB_ACTIONS").is_ok() {
    //         eprintln!("Skipping test on GitHub Actions");
    //         return;
    //     }
    //     let path = get_test_data_path().join("wasm").join("comments.0.wasm");
    //     let absolute_path = path.canonicalize().unwrap();
    //
    //     let bytes = std::fs::read(absolute_path).unwrap();
    //     let mod_name = String::from("index");
    //     let coq = inference_wasm_coq_translator::wasm_parser::translate_bytes(
    //         &mod_name,
    //         bytes.as_slice(),
    //     );
    //     assert!(coq.is_ok());
    //     let coq_file_path = get_out_path().join("test_wasm_to_coq.v");
    //     std::fs::write(coq_file_path, coq.unwrap()).unwrap();
    // }

    /// Returns the path to the test data directory.
    ///
    /// Navigates from the current working directory to the workspace root
    /// and locates the `test_data` directory.
    ///
    /// This helper is used for locating test input files in unit tests.
    #[allow(dead_code)]
    pub(crate) fn get_test_data_path() -> std::path::PathBuf {
        let current_dir = std::env::current_dir().unwrap();
        current_dir
            .parent() // inference
            .unwrap()
            .join("test_data")
    }

    /// Returns the path to the output directory for test artifacts.
    ///
    /// Located at `<workspace_root>/out/`, this directory is where test-generated
    /// WASM and Rocq files are written during testing.
    #[allow(dead_code)]
    fn get_out_path() -> std::path::PathBuf {
        get_test_data_path().parent().unwrap().join("out")
    }
}
