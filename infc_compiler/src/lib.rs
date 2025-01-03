use ast::builder::build_ast;
pub mod ast;
pub mod wasm_to_coq_translator;
pub mod wat_codegen;

pub fn compile_to_wat(source_code: &str) -> String {
    let inference_language = tree_sitter_inference::language();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&inference_language)
        .expect("Error loading Inference grammar");
    let tree = parser.parse(source_code, None).unwrap();
    let code = source_code.as_bytes();
    let root_node = tree.root_node();
    let ast = build_ast(root_node, code);
    wat_codegen::wat_generator::generate_for_source_file(&ast)
}
