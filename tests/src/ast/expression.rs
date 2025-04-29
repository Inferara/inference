#[cfg(test)]
mod expression_tests {
    use inference_ast::{builder::Builder, symbols::SymbolType, t_ast::TypedAst, types::AstNode};

    fn build_ast(source_code: String) -> TypedAst {
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

    #[test]
    fn test_uzumaki_type_in_block() {
        let source_code = r#"
        fn a() {
            let a: i8 = @;
            let b: i16 = @;
            let c: i32 = @;
            let d: i64 = @;

            let e: u8 = @;
            let f: u16 = @;
            let g: u32 = @;
            let h: u64 = @;
        }"#;
        let ast = build_ast(source_code.to_string());
        let uzumaki_nodes = ast.filter_nodes(|node| matches!(node, AstNode::UzumakiExpression(_)));
        assert!(
            uzumaki_nodes.len() == 8,
            "Expected 8 UzumakiExpression nodes, found {}",
            uzumaki_nodes.len()
        );
        let expected_types = ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64"];
        let mut uzumaki_nodes = uzumaki_nodes.iter().collect::<Vec<_>>();
        uzumaki_nodes.sort_by_key(|node| node.start_line());
        for (i, node) in uzumaki_nodes.iter().enumerate() {
            if let AstNode::UzumakiExpression(expr) = node {
                assert!(
                    expr.ty == SymbolType::Global(expected_types[i].to_string()),
                    "Expected type {} for UzumakiExpression, found {:?}",
                    expected_types[i],
                    expr.ty
                );
            }
        }
    }
}
