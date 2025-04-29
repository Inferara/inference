//! This module contains various infc end to end tests
mod ast;

#[cfg(test)]
mod general_tests {
    #[allow(dead_code)]
    pub(crate) fn get_test_data_path() -> std::path::PathBuf {
        let current_dir = std::env::current_dir().unwrap();
        current_dir
            .parent() // inference
            .unwrap()
            .join("test_data")
            .join("inf")
    }

    #[test]
    fn test_example_inf_parsing() -> anyhow::Result<()> {
        let test_data_path = get_test_data_path();
        let source_code = std::fs::read_to_string(test_data_path.join("example.inf")).unwrap();
        let inference_language = tree_sitter_inference::language();
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&inference_language)
            .expect("Error loading Inference grammar");
        let tree = parser.parse(&source_code, None).unwrap();
        let code = source_code.as_bytes();
        let root_node = tree.root_node();
        let ast = inference_ast::builder::build_ast(root_node, code)?;
        let json_output = serde_json::to_string_pretty(&ast).unwrap();
        std::fs::write(test_data_path.join("example.json"), json_output)?;
        Ok(())
    }
}
