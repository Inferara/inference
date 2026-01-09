#![warn(clippy::pedantic)]

//! # Inference Compiler CLI
//!
//! Command line interface for the Inference toolchain.
//!
//! 1. Parse  (`--parse`)   – build the typed AST.
//! 2. Analyze (`--analyze`) – perform type / semantic inference and validation.
//! 3. Codegen (`--codegen`) – emit WebAssembly, and optionally translate to V (`-o`).
//!
//! At least one of the phase flags must be supplied; the phases that are requested will be
//! executed in the canonical order even if specified out of order on the command line.
//!
//! Output artifacts are written to an `out/` directory relative to the current working directory.
//! When `-o` is passed together with `--codegen` the produced WASM module is further translated
//! into V source and saved as `out/out.v`.
//!
//! ## Exit codes
//! * 0 – success.
//! * 1 – usage / IO / phase failure.
//!
//! ## Example
//! ```bash
//! infc examples/hello.inf --codegen -o
//! ```
//!
//! ## Tests
//! Integration tests exercise flag validation and the happy path compilation pipeline.

mod parser;
use clap::Parser;
use inference::{analyze, codegen, parse, type_check, wasm_to_v};
use parser::Cli;
use std::{
    fs,
    path::PathBuf,
    process::{self},
};

/// Entry point for the CLI executable.
///
/// Responsibilities:
/// * Parse flags.
/// * Validate that the input path exists and at least one phase is selected.
/// * Run requested phases (parse -> analyze -> codegen).
/// * Optionally translate emitted WASM into V source when `-o` is set.
///
/// On any failure a diagnostic is printed to stderr and the process exits with code `1`.
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

    let source_code = fs::read_to_string(&args.path).expect("Error reading source file");
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

#[cfg(test)]
mod test {

    // #[test]
    // fn test_wasm_to_coq() {
    //     if std::env::var("GITHUB_ACTIONS").is_ok() {
    //         eprintln!("Skipping test on GitHub Actions");
    //         return;
    //     }
    //     let path = get_test_data_path().join("wasm").join("comments.0.wasm");
    //     let absolute_path = path.canonicalize().unwrap();

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

    #[allow(dead_code)]
    pub(crate) fn get_test_data_path() -> std::path::PathBuf {
        let current_dir = std::env::current_dir().unwrap();
        current_dir
            .parent() // inference
            .unwrap()
            .join("test_data")
    }

    #[allow(dead_code)]
    fn get_out_path() -> std::path::PathBuf {
        get_test_data_path().parent().unwrap().join("out")
    }
}
