#![warn(clippy::pedantic)]

//! # Inference Compiler CLI
//!
//! Command line interface for the Inference programming language compiler.
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
//!    - Generates LLVM IR from typed AST
//!    - Compiles LLVM IR to WebAssembly
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
//! At least one phase flag must be specified.
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
//! - **Codegen errors**: LLVM compilation failures
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
//! Full compilation to WebAssembly:
//! ```bash
//! infc example.inf --codegen -o
//! ```
//!
//! Compile and generate Rocq translation:
//! ```bash
//! infc example.inf --codegen -o -v
//! ```
//!
//! Only generate Rocq (no WASM file):
//! ```bash
//! infc example.inf --codegen -v
//! ```
//!
//! ## Current Limitations
//!
//! - Module path resolution for nested modules is best-effort
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

mod parser;
use clap::Parser;
use inference::{analyze, codegen, parse_file, type_check, wasm_to_v};
use parser::Cli;
use std::{
    fs,
    path::PathBuf,
    process::{self},
};

/// Entry point for the Inference compiler CLI.
///
/// ## Execution Flow
///
/// 1. **Parse command line arguments** using clap
/// 2. **Validate input**:
///    - Verify source file exists
///    - Ensure at least one phase flag is specified
/// 3. **Execute compilation phases** in canonical order:
///    - Parse: Build typed AST from source using tree-sitter
///    - Analyze: Type check and semantic validation
///    - Codegen: Generate LLVM IR and compile to WebAssembly
/// 4. **Generate output files** (if requested):
///    - Write WASM binary with `-o` flag
///    - Write Rocq translation with `-v` flag
///
/// ## Error Handling
///
/// All errors are reported to stderr with descriptive messages and cause
/// process exit with code 1. Error categories:
///
/// - **Usage errors**: Missing phase flags, invalid arguments
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
fn main() {
    let args = Cli::parse();
    if !args.path.exists() {
        eprintln!("Error: path not found");
        process::exit(1);
    }

    let output_path = PathBuf::from("out");
    let need_parse = args.parse;
    let need_analyze = args.analyze;
    let need_codegen = args.codegen;

    if !(need_parse || need_analyze || need_codegen) {
        eprintln!("Error: at least one of --parse, --analyze, or --codegen must be specified");
        process::exit(1);
    }

    let mut t_ast = None;
    if need_codegen || need_analyze || need_parse {
        match parse_file(&args.path) {
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
        unreachable!("Phase validation guarantees parse ran when required");
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
        let wasm = match codegen(&typed_context.unwrap()) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Codegen failed: {e}");
                process::exit(1);
            }
        };
        println!("WASM generated");
        let source_fname = args
            .path
            .file_stem()
            .unwrap_or_else(|| std::ffi::OsStr::new("module"))
            .to_str()
            .unwrap();
        if args.generate_wasm_output {
            let wasm_file_path = output_path.join(format!("{source_fname}.wasm"));
            if let Err(e) = fs::create_dir_all(&output_path) {
                eprintln!("Failed to create output directory: {e}");
                process::exit(1);
            }
            if let Err(e) = fs::write(&wasm_file_path, &wasm) {
                eprintln!("Failed to write WASM file: {e}");
                process::exit(1);
            }
            println!("WASM generated at: {}", wasm_file_path.to_string_lossy());
        }
        if args.generate_v_output {
            match wasm_to_v(source_fname, &wasm) {
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
