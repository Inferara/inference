use inference_ast::{builder::Builder, t_ast::TypedAst};

#[allow(dead_code)]
pub(crate) fn build_ast(source_code: String) -> TypedAst {
    let inference_language = tree_sitter_inference::language();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&inference_language)
        .expect("Error loading Inference grammar");
    let tree = parser.parse(source_code.clone(), None).unwrap();
    let code = source_code.as_bytes();
    let root_node = tree.root_node();
    let mut builder = Builder::new();
    builder.add_source_code(root_node, code);
    let builder = builder.build_ast().unwrap();
    builder.t_ast()
}
