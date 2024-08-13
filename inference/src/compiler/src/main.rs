#![warn(clippy::pedantic)]

mod ast;
mod cli;
mod wasm_to_coq_translator;

use ast::builder::build_ast;
use clap::Parser;
use cli::parser::Cli;
use std::{fs, path::Path, process};

/// Inference compiler entry point
///
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
