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
//! The `test` module contains unit tests to validate the core functionality of the compiler:
//!
//! - `test_parse`: Tests the parsing of a `.inf` source file into an AST.
//! - `test_wasm_to_coq`: Tests the translation of a WebAssembly (`.wasm`) file into Coq code.
//! - `test_walrys`: Demonstrates reading a WebAssembly (`.wasm`) file using the `walrus` crate, and prints function IDs and names.

mod ast;
mod cli;
mod wasm_to_coq_translator;

use ast::builder::build_ast;
use clap::Parser;
use cli::parser::Cli;
use std::{fs, path::Path, process};

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
        parse_file(args.path.to_str().unwrap());
    }
}

fn parse_file(source_file_path: &str) -> ast::types::SourceFile {
    let text = fs::read_to_string(source_file_path).expect("Error reading source file");
    parse(&text)
}

fn parse(source_code: &str) -> ast::types::SourceFile {
    let inference_language = tree_sitter_inference::language();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&inference_language)
        .expect("Error loading Inference grammar");
    let tree = parser.parse(source_code, None).unwrap();
    let code = source_code.as_bytes();
    let ast = build_ast(tree.root_node(), code);
    ast
}

fn wasm_to_coq(path: &Path) -> String {
    let absolute_path = path.canonicalize().unwrap();
    let filename = path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .split('.')
        .next()
        .unwrap();

    let bytes = std::fs::read(absolute_path).unwrap();
    let coq = wasm_to_coq_translator::wasm_parser::translate_bytes(
        filename.to_string(),
        bytes.as_slice(),
    );
    assert!(!coq.is_empty(), "Failed to parse {filename} to .v");
    let current_dir = std::env::current_dir().unwrap();
    let coq_file_path = current_dir.join(format!("out/{filename}.v"));
    fs::create_dir_all("out").unwrap();
    std::fs::write(coq_file_path.clone(), coq).unwrap();
    coq_file_path.to_str().unwrap().to_owned()
}

#[allow(unused_imports)]
mod test {

    use walrus::Module;

    #[test]
    fn test_parse() {
        let current_dir = std::env::current_dir().unwrap();
        let path = current_dir.join("samples/example.inf");
        let absolute_path = path.canonicalize().unwrap();

        let ast = super::parse_file(absolute_path.to_str().unwrap());
        assert!(!ast.definitions.is_empty());
    }

    #[test]
    fn test_wasm_to_coq() {
        let current_dir = std::env::current_dir().unwrap();
        let path = current_dir.join("samples/audio_bg.wasm");
        let absolute_path = path.canonicalize().unwrap();

        let bytes = std::fs::read(absolute_path).unwrap();
        let mod_name = String::from("index");
        let coq =
            super::wasm_to_coq_translator::wasm_parser::translate_bytes(mod_name, bytes.as_slice());
        assert!(!coq.is_empty());
        //save to file
        let coq_file_path = current_dir.join("samples/test_wasm_to_coq.v");
        std::fs::write(coq_file_path, coq).unwrap();
    }

    #[test]
    fn test_walrys() {
        let current_dir = std::env::current_dir().unwrap();
        let path = current_dir.join("samples/audio_bg.wasm");
        let absolute_path = path.canonicalize().unwrap();
        let module = Module::from_file(absolute_path).unwrap();
        for func in module.funcs.iter() {
            println!("{} : {:?}", func.id().index(), func.name);
        }
    }
}
