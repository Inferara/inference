#[cfg(test)]
mod expression_tests {
    use inference_ast::nodes::{AstNode, Expression};
    use inference_type_checker::type_info::{NumberTypeKindNumberType, TypeInfoKind};

    use crate::utils::build_ast;

    #[test]
    fn test_type_inference_1() {
        let source_code = r#"
        fn a() {
            let a: i8 = @;
            let b: i16 = @;
            let c: i32 = @;
            let d: i64 = @;

            let e: u8;
            e = @;
            let f: u16 = @;
            let g: u32 = @;
            let h: u64 = @;
        }"#;
        let arena = build_ast(source_code.to_string());
        let uzumaki_nodes =
            arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Uzumaki(_))));
        assert!(
            uzumaki_nodes.len() == 8,
            "Expected 8 UzumakiExpression nodes, found {}",
            uzumaki_nodes.len()
        );
        let expected_types = [
            TypeInfoKind::Number(NumberTypeKindNumberType::I8),
            TypeInfoKind::Number(NumberTypeKindNumberType::I16),
            TypeInfoKind::Number(NumberTypeKindNumberType::I32),
            TypeInfoKind::Number(NumberTypeKindNumberType::I64),
            TypeInfoKind::Number(NumberTypeKindNumberType::U8),
            TypeInfoKind::Number(NumberTypeKindNumberType::U16),
            TypeInfoKind::Number(NumberTypeKindNumberType::U32),
            TypeInfoKind::Number(NumberTypeKindNumberType::U64),
        ];
        let mut uzumaki_nodes = uzumaki_nodes.iter().collect::<Vec<_>>();
        uzumaki_nodes.sort_by_key(|node| node.start_line());
        let typed_context = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .unwrap()
            .typed_context();

        for (i, node) in uzumaki_nodes.iter().enumerate() {
            if let AstNode::Expression(Expression::Uzumaki(uzumaki)) = node {
                assert!(
                    typed_context.get_node_typeinfo(uzumaki.id).unwrap().kind == expected_types[i],
                    "Expected type {} for UzumakiExpression, found {:?}",
                    expected_types[i],
                    typed_context.get_node_typeinfo(uzumaki.id).unwrap().kind
                );
            }
        }
    }
}
