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
use inference::{compile_to_wat, wasm_to_v, wat_to_wasm};
use parser::Cli;
use std::{
    fs,
    path::PathBuf,
    process::{self},
};

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

    let output_path = match args.output {
        Some(path) => {
            if !path.exists() {
                fs::create_dir_all(&path).expect("Error creating output directory");
            }
            path
        }
        None => PathBuf::from("out"),
    };
    let generate = match args.generate {
        Some(g) => match g.as_str().to_lowercase().as_str() {
            "wat" => "wat",
            "wasm" => "wasm",
            "f" | "full" => "f",
            _ => "v",
        },
        None => "v",
    };
    let source = match args.source {
        Some(s) => match s.as_str().to_lowercase().as_str() {
            "wat" => "wat",
            //"wasm" => "wasm",
            _ => "inf",
        },
        None => "inf",
    };
    let source_code = fs::read_to_string(&args.path).expect("Error reading source file");
    let mod_name = args.path.file_stem().unwrap().to_str().unwrap().to_string();
    let wat = if source == "inf" {
        compile_to_wat(source_code.as_str()).unwrap()
    } else {
        source_code
    };
    if generate == "wat" {
        let wat_file_path = output_path.join("out.wat");
        fs::write(wat_file_path.clone(), wat).expect("Error writing WAT file");
        println!("WAT generated at: {}", wat_file_path.to_str().unwrap());
        process::exit(0);
    }
    let wasm = wat_to_wasm(wat.as_str()).unwrap();
    if generate == "wasm" {
        let wasm_file_path = output_path.join("out.wasm");
        fs::write(wasm_file_path.clone(), wasm).expect("Error writing WASM file");
        println!("WASM generated at: {}", wasm_file_path.to_str().unwrap());
        process::exit(0);
    }
    let v = wasm_to_v(mod_name.as_str(), &wasm).unwrap();
    if generate == "v" {
        let v_file_path = output_path.join("out.v");
        fs::write(v_file_path.clone(), v).expect("Error writing V file");
        println!("V generated at: {}", v_file_path.to_str().unwrap());
        process::exit(0);
    }
    if generate == "f" {
        let v_file_path = output_path.join("out.v");
        fs::write(v_file_path.clone(), v).expect("Error writing V file");
        println!("V generated at: {}", v_file_path.to_str().unwrap());
        let wat_file_path = output_path.join("out.wat");
        fs::write(wat_file_path.clone(), wat).expect("Error writing WAT file");
        println!("WAT generated at: {}", wat_file_path.to_str().unwrap());
        let wasm_file_path = output_path.join("out.wasm");
        fs::write(wasm_file_path.clone(), wasm).expect("Error writing WASM file");
        println!("WASM generated at: {}", wasm_file_path.to_str().unwrap());
        process::exit(0);
    }
    eprintln!("Error: invalid generate option");
    process::exit(1);
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
