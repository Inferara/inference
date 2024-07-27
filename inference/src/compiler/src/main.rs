#![warn(clippy::pedantic)]

mod ast;
mod wasm_to_coq_translator;

use ast::builder::build_ast;
use std::{env, fs, process};

fn main() {
    if env::args().len() != 1 {
        eprintln!("One argument is expected: the source file path");
        process::exit(1);
    }

    let source_file_path = env::args().nth(1).unwrap();
    parse_file(&source_file_path);
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

mod test {

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
        let path = current_dir.join("samples/webassembly_linear_memory_bg.wasm");
        let absolute_path = path.canonicalize().unwrap();

        let bytes = std::fs::read(absolute_path).unwrap();
        let coq = super::wasm_to_coq_translator::translator::translate_bytes(bytes.as_slice());
        assert!(!coq.is_empty());
    }
}
