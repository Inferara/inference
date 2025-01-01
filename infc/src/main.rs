#![warn(clippy::pedantic)]

//! # Inference Compiler
//!
//! This is the entry point for the Inference compiler, which provides functionality to parse and
//! translate `.inf` source files into Coq code (`.v` files).
//!
//! ## Modules
//!
//! - `ast`: Contains types and builders for constructing the AST from parsed source `.inf` files.
//! - `cli`: Contains the command-line interface (CLI) parsing logic using the `clap` crate.
//! - `wasm_to_coq_translator`: Handles the translation of WebAssembly (`.wasm`) files to Coq code (`.v` files).
//!
//! ## Main Functionality
//!
//! The main function parses command-line arguments to determine the operation mode:
//!
//! - If the `--wasm` flag is provided, the program will translate the specified `.wasm` file into `.v` code.
//! - Otherwise, the program will parse the specified `.inf` source file and generate an AST.
//!
//! ### Functions
//!
//! - `main`: The entry point of the program. Handles argument parsing and dispatches to the appropriate function
//!   based on the provided arguments. It handles parses specified in the first CLI argument
//!   and saves the request to the `out/` directory.
//!
//! ### Tests
//!
//! The `test` suite is located in the `main_tests` module and contains tests for the main functionality

mod parser;
use clap::Parser;
use infc_compiler::ast::{builder::build_ast, types::SourceFile};
use infc_compiler::wasm_to_coq_translator::translator::WasmModuleParseError;
use infc_compiler::{wasm_to_coq_translator, wat_codegen};
use parser::Cli;
use std::{fs, path::Path, process};
use walkdir::WalkDir;

/// Inference compiler entry point
///
/// This function parses the command-line arguments to determine whether to parse an `.inf` source file
/// or translate a `.wasm` file into Coq code. Depending on the `--wasm` flag, it either invokes the
/// `wasm_to_coq` function or the `parse_file` function.
fn main() {
    let args = Cli::parse();
    if !args.path.exists() {
        eprintln!("Error: path not found");
        process::exit(1);
    }

    if args.wasm {
        wasm_to_coq(&args.path);
    } else {
        parse_inf_file(args.path.to_str().unwrap());
    }
}

fn parse_inf_file(source_file_path: &str) -> SourceFile {
    let text = fs::read_to_string(source_file_path).expect("Error reading source file");
    parse_inference(&text)
}

fn parse_inference(source_code: &str) -> SourceFile {
    let inference_language = tree_sitter_inference::language();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&inference_language)
        .expect("Error loading Inference grammar");
    let tree = parser.parse(source_code, None).unwrap();
    let code = source_code.as_bytes();
    let root_node = tree.root_node();
    build_ast(root_node, code)
}

fn generate_wasm_s_expression(source_file: &SourceFile) -> String {
    wat_codegen::wat_generator::generate_for_source_file(source_file)
}

fn wasm_to_coq(path: &Path) {
    if path.is_file() {
        let filename = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace(".wasm", "")
            .replace('.', "_");
        if let Err(e) = wasm_to_coq_file(path, None, &filename) {
            eprintln!("{e}");
        }
    } else {
        for entry in WalkDir::new(path)
            .follow_links(true)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            let f_name = entry.file_name().to_string_lossy();

            if f_name.ends_with(".wasm") {
                if let Err(e) = wasm_to_coq_file(
                    entry.path(),
                    Some(
                        entry
                            .path()
                            .strip_prefix(path)
                            .ok()
                            .unwrap()
                            .parent()
                            .unwrap(),
                    ),
                    &f_name.replace(".wasm", "").replace('.', "_"),
                ) {
                    eprintln!("{e}");
                }
            }
        }
    }
}

fn wasm_to_coq_file(
    path: &Path,
    sub_path: Option<&Path>,
    filename: &String,
) -> Result<String, String> {
    let absolute_path = path.canonicalize().unwrap();
    let bytes = std::fs::read(absolute_path).unwrap();
    wasm_bytes_to_coq_file(&bytes, sub_path, filename)
}

fn wasm_bytes_to_coq_file(
    bytes: &Vec<u8>,
    sub_path: Option<&Path>,
    filename: &String,
) -> Result<String, String> {
    let coq = wasm_to_coq_translator::wasm_parser::translate_bytes(filename, bytes.as_slice());

    if let Err(e) = coq {
        let WasmModuleParseError::UnsupportedOperation(error_message) = e;
        let error = format!("Error: {error_message}");
        return Err(error);
    }

    let current_dir = std::env::current_dir().unwrap();
    let target_dir = match sub_path {
        Some(sp) => current_dir.join("out").join(sp),
        None => current_dir.join("out"),
    };
    let coq_file_path = target_dir.join(format!("{filename}.v"));
    fs::create_dir_all(target_dir).unwrap();
    std::fs::write(coq_file_path.clone(), coq.unwrap()).unwrap();
    Ok(coq_file_path.to_str().unwrap().to_owned())
}

#[cfg(test)]
mod test {

    #[test]
    fn test_parse() {
        let path = get_test_data_path().join("inf").join("example.inf");
        let absolute_path = path.canonicalize().unwrap();
        let ast = crate::parse_inf_file(absolute_path.to_str().unwrap());
        assert!(!ast.definitions.is_empty());
        // std::fs::write(
        //     current_dir.join(""),
        //     format!("{ast:#?}"),
        // )
        // .unwrap();
    }

    #[test]
    fn test_wasm_to_coq() {
        if std::env::var("GITHUB_ACTIONS").is_ok() {
            eprintln!("Skipping test on GitHub Actions");
            return;
        }
        let path = get_test_data_path().join("wasm").join("comments.0.wasm");
        let absolute_path = path.canonicalize().unwrap();

        let bytes = std::fs::read(absolute_path).unwrap();
        let mod_name = String::from("index");
        let coq = crate::wasm_to_coq_translator::wasm_parser::translate_bytes(
            &mod_name,
            bytes.as_slice(),
        );
        assert!(coq.is_ok());
        let coq_file_path = get_out_path().join("test_wasm_to_coq.v");
        std::fs::write(coq_file_path, coq.unwrap()).unwrap();
    }

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
