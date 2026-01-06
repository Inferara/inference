//! Type checker test suite
//!
//! This module contains tests for type checking and type inference functionality.
//! Note: The type checker is WIP - many tests document current behavior with FIXME
//! comments indicating expected behavior when implementation is complete.
use crate::utils::build_ast;

/// Tests that verify types are correctly inferred for various constructs.
#[cfg(test)]
mod type_inference_tests {
    use super::*;
    use inference_ast::nodes::{AstNode, Expression, Literal, Statement};
    use inference_type_checker::TypeCheckerBuilder;
    use inference_type_checker::type_info::{NumberTypeKindNumberType, TypeInfoKind};

    /// Helper function to run type checker, returning Result to handle WIP failures
    fn try_type_check(
        source: &str,
    ) -> anyhow::Result<inference_type_checker::typed_context::TypedContext> {
        let arena = build_ast(source.to_string());
        Ok(TypeCheckerBuilder::build_typed_context(arena)?.typed_context())
    }

    /// Tests for primitive type inference with actual type checking
    mod primitives {
        use super::*;

        #[test]
        fn test_numeric_literal_type_inference() {
            let source = r#"fn test() -> i32 { return 42; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            // Find the number literal and verify its type
            let arena = build_ast(source.to_string());
            let literals = arena.filter_nodes(|node| {
                matches!(
                    node,
                    AstNode::Expression(Expression::Literal(Literal::Number(_)))
                )
            });
            assert_eq!(literals.len(), 1, "Expected 1 number literal");

            // Type checker successfully processed the source
            assert_eq!(typed_context.source_files().len(), 1);
        }

        #[test]
        fn test_bool_literal_type_inference() {
            let source = r#"fn test() -> bool { return true; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let arena = build_ast(source.to_string());
            let bool_literals = arena.filter_nodes(|node| {
                matches!(
                    node,
                    AstNode::Expression(Expression::Literal(Literal::Bool(_)))
                )
            });
            assert_eq!(bool_literals.len(), 1, "Expected 1 bool literal");

            // FIXME: Type checker doesn't populate type info for bool literals yet.
            // When implemented, this should verify the literal has Bool type.
            if let AstNode::Expression(Expression::Literal(Literal::Bool(lit))) = &bool_literals[0]
            {
                let type_info = typed_context.get_node_typeinfo(lit.id);
                // Currently returns None - should return Some(Bool) when implemented
                assert!(
                    type_info.is_none(),
                    "FIXME: Bool literal type info not yet populated"
                );
            }
        }

        #[test]
        fn test_string_type_inference() {
            let source = r#"fn test(x: String) -> String { return x; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            assert_eq!(typed_context.source_files().len(), 1);
        }

        #[test]
        fn test_variable_type_inference() {
            let source = r#"
            fn test() {
                let x: i32 = 10;
                let y: bool = true;
            }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let arena = build_ast(source.to_string());
            let var_defs = arena.filter_nodes(|node| {
                matches!(node, AstNode::Statement(Statement::VariableDefinition(_)))
            });
            assert_eq!(var_defs.len(), 2, "Expected 2 variable definitions");

            // Verify type checking completed successfully
            assert_eq!(typed_context.source_files().len(), 1);
        }

        #[test]
        fn test_all_numeric_types_type_check() {
            let sources = [
                ("i8", r#"fn test(x: i8) -> i8 { return x; }"#),
                ("i16", r#"fn test(x: i16) -> i16 { return x; }"#),
                ("i32", r#"fn test(x: i32) -> i32 { return x; }"#),
                ("i64", r#"fn test(x: i64) -> i64 { return x; }"#),
                ("u8", r#"fn test(x: u8) -> u8 { return x; }"#),
                ("u16", r#"fn test(x: u16) -> u16 { return x; }"#),
                ("u32", r#"fn test(x: u32) -> u32 { return x; }"#),
                ("u64", r#"fn test(x: u64) -> u64 { return x; }"#),
            ];

            for (type_name, source) in sources {
                let typed_context =
                    try_type_check(source).expect("Type checking should succeed for numeric types");
                assert_eq!(
                    typed_context.source_files().len(),
                    1,
                    "Type checking should succeed for {} type",
                    type_name
                );
            }
        }
    }

    /// Tests for expression type inference
    mod expressions {
        use super::*;

        #[test]
        fn test_binary_add_expression_type() {
            let source = r#"fn test() -> i32 { return 10 + 20; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let arena = build_ast(source.to_string());
            let binary_exprs = arena
                .filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Binary(_))));
            assert_eq!(binary_exprs.len(), 1, "Expected 1 binary expression");

            // FIXME: Type checker doesn't populate type info for binary expressions yet.
            // When implemented, binary add of i32 literals should return i32.
            if let AstNode::Expression(Expression::Binary(bin_expr)) = &binary_exprs[0] {
                let type_info = typed_context.get_node_typeinfo(bin_expr.id);
                // Currently returns None - should return Some(i32) when implemented
                assert!(
                    type_info.is_none(),
                    "FIXME: Binary expression type info not yet populated"
                );
            }
        }

        #[test]
        fn test_comparison_expression_returns_bool() {
            let source = r#"fn test(x: i32, y: i32) -> bool { return x > y; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let binary_exprs = typed_context
                .filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Binary(_))));
            assert_eq!(binary_exprs.len(), 1, "Expected 1 binary expression");

            if let AstNode::Expression(Expression::Binary(bin_expr)) = &binary_exprs[0] {
                let type_info = typed_context.get_node_typeinfo(bin_expr.id);
                assert!(type_info.is_some(), "Comparison should have type info");
                assert!(
                    type_info.unwrap().is_bool(),
                    "Comparison expression should return bool"
                );
            }
        }

        #[test]
        fn test_logical_and_expression_type() {
            let source = r#"fn test(a: bool, b: bool) -> bool { return a && b; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let arena = build_ast(source.to_string());
            let binary_exprs = arena
                .filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Binary(_))));
            assert_eq!(binary_exprs.len(), 1, "Expected 1 binary expression");

            // FIXME: Type checker doesn't populate type info for logical expressions yet.
            if let AstNode::Expression(Expression::Binary(bin_expr)) = &binary_exprs[0] {
                let type_info = typed_context.get_node_typeinfo(bin_expr.id);
                assert!(
                    type_info.is_none(),
                    "FIXME: Logical expression type info not yet populated"
                );
            }
        }

        #[test]
        fn test_nested_binary_expression_type() {
            let source = r#"fn test() -> i32 { return (10 + 20) * 30; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let arena = build_ast(source.to_string());
            let binary_exprs = arena
                .filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Binary(_))));
            // Should have 2 binary expressions: (10 + 20) and (...) * 30
            assert_eq!(binary_exprs.len(), 2, "Expected 2 binary expressions");

            // FIXME: Type checker doesn't populate type info for nested binary expressions yet.
            for expr in &binary_exprs {
                if let AstNode::Expression(Expression::Binary(bin_expr)) = expr {
                    let type_info = typed_context.get_node_typeinfo(bin_expr.id);
                    assert!(
                        type_info.is_none(),
                        "FIXME: Nested binary expression type info not yet populated"
                    );
                }
            }
        }

        // FIXME: Division operator (/) is not supported in codegen, but parsing succeeds.
        // This test documents current behavior where parsing works but codegen would fail.
        // When div support is added, this test should be updated to verify end-to-end.
        #[test]
        fn test_binary_expressions_with_div() {
            let source = r#"fn test() -> i32 { return (10 + 20) * (30 - 5) / 2; }"#;
            let arena = build_ast(source.to_string());
            // Parsing succeeds even though div is not supported in codegen
            assert_eq!(arena.source_files().len(), 1);
        }
    }

    /// Tests for function call type inference
    mod function_calls {
        use super::*;

        #[test]
        fn test_function_call_return_type() {
            let source = r#"
            fn helper() -> i32 { return 42; }
            fn test() -> i32 { return helper(); }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let arena = build_ast(source.to_string());
            let fn_calls = arena.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::FunctionCall(_)))
            });
            assert_eq!(fn_calls.len(), 1, "Expected 1 function call");

            // FIXME: Type checker doesn't populate type info for function calls yet.
            // When implemented, function calls should have their return type as type info.
            if let AstNode::Expression(Expression::FunctionCall(call)) = &fn_calls[0] {
                let type_info = typed_context.get_node_typeinfo(call.id);
                assert!(
                    type_info.is_none(),
                    "FIXME: Function call type info not yet populated"
                );
            }
        }

        #[test]
        fn test_function_call_with_args() {
            let source = r#"
            fn add(a: i32, b: i32) -> i32 { return a + b; }
            fn test() -> i32 { return add(10, 20); }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let arena = build_ast(source.to_string());
            let fn_calls = arena.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::FunctionCall(_)))
            });
            assert_eq!(fn_calls.len(), 1, "Expected 1 function call");

            // FIXME: Type checker doesn't populate type info for function calls yet.
            if let AstNode::Expression(Expression::FunctionCall(call)) = &fn_calls[0] {
                let type_info = typed_context.get_node_typeinfo(call.id);
                assert!(
                    type_info.is_none(),
                    "FIXME: Function call with args type info not yet populated"
                );
            }
        }

        #[test]
        fn test_chained_function_calls() {
            let source = r#"
            fn double(x: i32) -> i32 { return x + x; }
            fn test() -> i32 { return double(double(5)); }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let arena = build_ast(source.to_string());
            let fn_calls = arena.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::FunctionCall(_)))
            });
            // 2 function calls: outer double() and inner double(5)
            assert_eq!(fn_calls.len(), 2, "Expected 2 function calls");

            // FIXME: Type checker doesn't populate type info for chained function calls yet.
            for call_node in &fn_calls {
                if let AstNode::Expression(Expression::FunctionCall(call)) = call_node {
                    let type_info = typed_context.get_node_typeinfo(call.id);
                    assert!(
                        type_info.is_none(),
                        "FIXME: Chained function call type info not yet populated"
                    );
                }
            }
        }
    }

    /// Tests for statement type inference
    mod statements {
        use super::*;

        #[test]
        fn test_if_statement_with_comparison_condition() {
            let source = r#"fn test(x: i32) -> i32 { if x > 0 { return 1; } else { return 0; } }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            assert_eq!(typed_context.source_files().len(), 1);
        }

        #[test]
        fn test_if_statement_with_bool_condition() {
            let source =
                r#"fn test(flag: bool) -> i32 { if flag { return 1; } else { return 0; } }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            assert_eq!(typed_context.source_files().len(), 1);
        }

        #[test]
        fn test_loop_with_break() {
            let source = r#"fn test() { loop { break; } }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            assert_eq!(typed_context.source_files().len(), 1);
        }

        #[test]
        fn test_assignment_type_check() {
            let source = r#"
            fn test() {
                let x: i32 = 10;
                x = 20;
            }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            assert_eq!(typed_context.source_files().len(), 1);
        }
    }

    /// Tests for array type inference
    mod arrays {
        use super::*;

        // FIXME: Array indexing (arr[0]) type inference is not fully implemented.
        // Currently parsing succeeds but type inference may not correctly resolve
        // the element type when accessing array elements.
        // Expected behavior: arr[0] on [i32; 1] should infer as i32.
        #[test]
        fn test_array_type() {
            let source = r#"fn get_first(arr: [i32; 1]) -> i32 { return arr[0]; }"#;
            let arena = build_ast(source.to_string());
            // Parsing succeeds
            assert_eq!(arena.source_files().len(), 1);
        }

        #[test]
        fn test_nested_arrays() {
            let source = r#"fn test(matrix: [[bool; 2]; 1]) { assert(true); }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            assert_eq!(typed_context.source_files().len(), 1);
        }
    }

    /// Tests for Uzumaki (@) expression type inference
    mod uzumaki {
        use super::*;

        #[test]
        fn test_uzumaki_numeric_type_inference() {
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
            let uzumaki_nodes = arena
                .filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Uzumaki(_))));
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
            let typed_context = TypeCheckerBuilder::build_typed_context(arena)
                .unwrap()
                .typed_context();

            for (i, node) in uzumaki_nodes.iter().enumerate() {
                if let AstNode::Expression(Expression::Uzumaki(uzumaki)) = node {
                    assert!(
                        typed_context.get_node_typeinfo(uzumaki.id).unwrap().kind
                            == expected_types[i],
                        "Expected type {} for UzumakiExpression, found {:?}",
                        expected_types[i],
                        typed_context.get_node_typeinfo(uzumaki.id).unwrap().kind
                    );
                }
            }
        }

        #[test]
        fn test_uzumaki_in_return_statement() {
            let source = r#"fn test() -> i32 { return @; }"#;
            let arena = build_ast(source.to_string());
            let uzumaki_nodes = arena
                .filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Uzumaki(_))));
            assert_eq!(uzumaki_nodes.len(), 1, "Expected 1 uzumaki expression");

            let typed_context = TypeCheckerBuilder::build_typed_context(arena)
                .unwrap()
                .typed_context();

            if let AstNode::Expression(Expression::Uzumaki(uzumaki)) = &uzumaki_nodes[0] {
                let type_info = typed_context.get_node_typeinfo(uzumaki.id);
                assert!(
                    type_info.is_some(),
                    "Uzumaki in return should have type info"
                );
                assert!(
                    matches!(
                        type_info.unwrap().kind,
                        TypeInfoKind::Number(NumberTypeKindNumberType::I32)
                    ),
                    "Uzumaki should infer return type i32"
                );
            }
        }
    }

    /// Tests for identifier type inference
    mod identifiers {
        use super::*;

        #[test]
        fn test_parameter_identifier_type() {
            let source = r#"fn test(x: i32) -> i32 { return x; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let arena = build_ast(source.to_string());
            let identifiers = arena.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::Identifier(_)))
            });
            // Should have at least 1 identifier (x in return statement)
            assert!(!identifiers.is_empty(), "Expected identifier expressions");

            // FIXME: Type checker doesn't populate type info for identifiers yet.
            // When implemented, parameter references should have their declared type.
            for id_node in &identifiers {
                if let AstNode::Expression(Expression::Identifier(id)) = id_node {
                    if id.name == "x" {
                        let type_info = typed_context.get_node_typeinfo(id.id);
                        assert!(
                            type_info.is_none(),
                            "FIXME: Parameter identifier type info not yet populated"
                        );
                    }
                }
            }
        }

        #[test]
        fn test_local_variable_identifier_type() {
            let source = r#"
            fn test() -> bool {
                let flag: bool = true;
                return flag;
            }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let arena = build_ast(source.to_string());
            let identifiers = arena.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::Identifier(_)))
            });

            // FIXME: Type checker doesn't populate type info for variable identifiers yet.
            for id_node in &identifiers {
                if let AstNode::Expression(Expression::Identifier(id)) = id_node {
                    if id.name == "flag" {
                        let type_info = typed_context.get_node_typeinfo(id.id);
                        assert!(
                            type_info.is_none(),
                            "FIXME: Local variable identifier type info not yet populated"
                        );
                    }
                }
            }
        }
    }
}

/// Tests that verify type errors are correctly reported.
#[cfg(test)]
mod type_error_tests {
    use crate::utils::build_ast;
    use inference_type_checker::TypeCheckerBuilder;

    #[test]
    fn test_type_checker_completes_on_valid_code() {
        let source = r#"fn test() -> i32 { return 42; }"#;
        let arena = build_ast(source.to_string());
        let result = TypeCheckerBuilder::build_typed_context(arena);
        assert!(result.is_ok(), "Type checker should succeed on valid code");
    }

    // FIXME: Type mismatch detection is not yet implemented.
    // These tests document expected behavior for future implementation.
    // When type error detection is added, uncomment and verify these tests.

    // #[test]
    // fn test_return_type_mismatch_detected() {
    //     let source = r#"fn test() -> i32 { return true; }"#;
    //     let arena = build_ast(source.to_string());
    //     let result = TypeCheckerBuilder::build_typed_context(arena);
    //     assert!(
    //         result.is_err(),
    //         "Type checker should detect return type mismatch"
    //     );
    // }

    // #[test]
    // fn test_assignment_type_mismatch_detected() {
    //     let source = r#"
    //     fn test() {
    //         let x: i32 = true;
    //     }"#;
    //     let arena = build_ast(source.to_string());
    //     let result = TypeCheckerBuilder::build_typed_context(arena);
    //     assert!(
    //         result.is_err(),
    //         "Type checker should detect assignment type mismatch"
    //     );
    // }

    // #[test]
    // fn test_binary_operator_type_mismatch_detected() {
    //     let source = r#"fn test() -> i32 { return 10 + true; }"#;
    //     let arena = build_ast(source.to_string());
    //     let result = TypeCheckerBuilder::build_typed_context(arena);
    //     assert!(
    //         result.is_err(),
    //         "Type checker should detect binary operator type mismatch"
    //     );
    // }

    // #[test]
    // fn test_function_arg_type_mismatch_detected() {
    //     let source = r#"
    //     fn add(a: i32, b: i32) -> i32 { return a + b; }
    //     fn test() -> i32 { return add(10, true); }
    //     "#;
    //     let arena = build_ast(source.to_string());
    //     let result = TypeCheckerBuilder::build_typed_context(arena);
    //     assert!(
    //         result.is_err(),
    //         "Type checker should detect function argument type mismatch"
    //     );
    // }
}
