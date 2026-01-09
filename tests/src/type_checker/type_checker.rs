//! Type checker test suite
//!
//! This module contains tests for type checking and type inference functionality.
//!
//! ## Testing Pattern
//!
//! When testing type info, always use `typed_context.filter_nodes()` instead of
//! creating a separate arena with `build_ast()`. The `TypedContext` contains the
//! arena with annotated node IDs, and using a separate arena creates ID mismatches.
use crate::utils::build_ast;

/// Tests that verify types are correctly inferred for various constructs.
#[cfg(test)]
mod type_inference_tests {
    use super::*;
    use inference_ast::nodes::{AstNode, Expression, Literal, Statement};
    use inference_type_checker::TypeCheckerBuilder;
    use inference_type_checker::type_info::{NumberType, TypeInfoKind};

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
            let literals = typed_context.filter_nodes(|node| {
                matches!(
                    node,
                    AstNode::Expression(Expression::Literal(Literal::Number(_)))
                )
            });
            assert_eq!(literals.len(), 1, "Expected 1 number literal");
            assert_eq!(typed_context.source_files().len(), 1);
            if let AstNode::Expression(Expression::Literal(Literal::Number(lit))) = &literals[0] {
                let literal_type = typed_context.get_node_typeinfo(lit.id);
                assert!(
                    literal_type.is_some(),
                    "Number literal should have type info"
                );
                assert!(
                    matches!(
                        literal_type.unwrap().kind,
                        TypeInfoKind::Number(NumberType::I32)
                    ),
                    "Number literal should have type i32"
                );
            } else {
                panic!("Expected number literal");
            }
        }

        #[test]
        fn test_bool_literal_type_inference() {
            let source = r#"fn test() -> bool { return true; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let bool_literals = typed_context.filter_nodes(|node| {
                matches!(
                    node,
                    AstNode::Expression(Expression::Literal(Literal::Bool(_)))
                )
            });
            assert_eq!(bool_literals.len(), 1, "Expected 1 bool literal");
            if let AstNode::Expression(Expression::Literal(Literal::Bool(lit))) = &bool_literals[0]
            {
                let type_info = typed_context.get_node_typeinfo(lit.id);
                assert!(type_info.is_some(), "Bool literal should have type info");
                assert!(
                    matches!(type_info.unwrap().kind, TypeInfoKind::Bool),
                    "Bool literal should have Bool type"
                );
            } else {
                panic!("Expected bool literal");
            }
        }

        #[test]
        fn test_string_type_inference() {
            let source = r#"fn test(x: String) -> String { return x; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            assert_eq!(typed_context.source_files().len(), 1);
            let functions = typed_context.functions();
            assert_eq!(functions.len(), 1, "Expected 1 function definition");
            let func = &functions[0];
            assert!(func.returns.is_some(), "Function should have return type");
            let return_type = typed_context.get_node_typeinfo(func.returns.as_ref().unwrap().id());
            assert!(
                return_type.is_some(),
                "Function return type should have type info"
            );
            assert!(
                matches!(return_type.unwrap().kind, TypeInfoKind::String),
                "Function return type should be String"
            );
            if let Some(arguments) = &func.arguments {
                assert!(!arguments.is_empty(), "Function should have arguments");
                let param_type = typed_context.get_node_typeinfo(arguments[0].id());
                assert!(
                    param_type.is_some(),
                    "Function parameter should have type info"
                );
                let param_type = param_type.unwrap();
                assert!(
                    matches!(param_type.kind, TypeInfoKind::String),
                    "Function parameter should have String type"
                );
            } else {
                panic!("Function should have arguments");
            }
        }

        #[test]
        fn test_variable_type_inference() {
            let source = r#"
            fn test() {
                let x: i32 = 10;
                let y: bool = true;
            }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let var_defs = typed_context.filter_nodes(|node| {
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

    /// Tests for function parameter type info storage
    mod function_parameters {
        use super::*;
        use inference_ast::nodes::{ArgumentType, Definition};

        #[test]
        fn test_single_parameter_type_info() {
            let source = r#"fn test(x: i32) -> i32 { return x; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let functions = typed_context.functions();
            assert_eq!(functions.len(), 1, "Expected 1 function");
            let func = &functions[0];
            if let Some(arguments) = &func.arguments {
                assert_eq!(arguments.len(), 1, "Expected 1 argument");
                if let ArgumentType::Argument(arg) = &arguments[0] {
                    let arg_type = typed_context.get_node_typeinfo(arg.id);
                    assert!(arg_type.is_some(), "Argument node should have type info");
                    assert!(
                        matches!(
                            arg_type.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32)
                        ),
                        "Argument should have i32 type"
                    );
                    let name_type = typed_context.get_node_typeinfo(arg.name.id);
                    assert!(name_type.is_some(), "Argument name should have type info");
                    assert!(
                        matches!(
                            name_type.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32)
                        ),
                        "Argument name should have i32 type"
                    );
                } else {
                    panic!("Expected Argument");
                }
            } else {
                panic!("Expected arguments");
            }
        }

        #[test]
        fn test_multiple_parameters_type_info() {
            let source = r#"fn test(a: i32, b: bool, c: String) -> i32 { return a; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let functions = typed_context.functions();
            assert_eq!(functions.len(), 1, "Expected 1 function");
            let func = &functions[0];
            if let Some(arguments) = &func.arguments {
                assert_eq!(arguments.len(), 3, "Expected 3 arguments");
                let expected_types = [
                    TypeInfoKind::Number(NumberType::I32),
                    TypeInfoKind::Bool,
                    TypeInfoKind::String,
                ];
                for (i, arg_type) in arguments.iter().enumerate() {
                    if let ArgumentType::Argument(arg) = arg_type {
                        let arg_type_info = typed_context.get_node_typeinfo(arg.id);
                        assert!(
                            arg_type_info.is_some(),
                            "Argument {} should have type info",
                            i
                        );
                        assert_eq!(
                            arg_type_info.unwrap().kind,
                            expected_types[i],
                            "Argument {} should have correct type",
                            i
                        );
                        let name_type_info = typed_context.get_node_typeinfo(arg.name.id);
                        assert!(
                            name_type_info.is_some(),
                            "Argument name {} should have type info",
                            i
                        );
                        assert_eq!(
                            name_type_info.unwrap().kind,
                            expected_types[i],
                            "Argument name {} should have correct type",
                            i
                        );
                    } else {
                        panic!("Expected Argument at position {}", i);
                    }
                }
            } else {
                panic!("Expected arguments");
            }
        }

        #[test]
        fn test_ignore_argument_type_info() {
            let source = r#"fn test(_: i32) -> i32 { return 42; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let functions = typed_context.functions();
            assert_eq!(functions.len(), 1, "Expected 1 function");
            let func = &functions[0];
            if let Some(arguments) = &func.arguments {
                assert_eq!(arguments.len(), 1, "Expected 1 argument");
                if let ArgumentType::IgnoreArgument(ignore_arg) = &arguments[0] {
                    let arg_type = typed_context.get_node_typeinfo(ignore_arg.id);
                    assert!(
                        arg_type.is_some(),
                        "IgnoreArgument node should have type info"
                    );
                    assert!(
                        matches!(
                            arg_type.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32)
                        ),
                        "IgnoreArgument should have i32 type"
                    );
                } else {
                    panic!("Expected IgnoreArgument");
                }
            } else {
                panic!("Expected arguments");
            }
        }

        #[test]
        fn test_ignore_argument_with_different_types() {
            let sources = [
                (NumberType::I8, r#"fn test(_: i8) -> i32 { return 1; }"#),
                (NumberType::I16, r#"fn test(_: i16) -> i32 { return 1; }"#),
                (NumberType::I32, r#"fn test(_: i32) -> i32 { return 1; }"#),
                (NumberType::I64, r#"fn test(_: i64) -> i32 { return 1; }"#),
                (NumberType::U8, r#"fn test(_: u8) -> i32 { return 1; }"#),
                (NumberType::U16, r#"fn test(_: u16) -> i32 { return 1; }"#),
                (NumberType::U32, r#"fn test(_: u32) -> i32 { return 1; }"#),
                (NumberType::U64, r#"fn test(_: u64) -> i32 { return 1; }"#),
            ];
            for (expected_type, source) in sources {
                let typed_context = try_type_check(source).expect("Type checking should succeed");
                let functions = typed_context.functions();
                assert_eq!(functions.len(), 1, "Expected 1 function");
                let func = &functions[0];
                if let Some(arguments) = &func.arguments {
                    assert_eq!(arguments.len(), 1, "Expected 1 argument");
                    if let ArgumentType::IgnoreArgument(ignore_arg) = &arguments[0] {
                        let arg_type = typed_context.get_node_typeinfo(ignore_arg.id);
                        assert!(
                            arg_type.is_some(),
                            "IgnoreArgument should have type info for {:?}",
                            expected_type
                        );
                        assert!(
                            matches!(
                                arg_type.unwrap().kind,
                                TypeInfoKind::Number(t) if t == expected_type
                            ),
                            "IgnoreArgument should have {:?} type",
                            expected_type
                        );
                    } else {
                        panic!("Expected IgnoreArgument for {:?}", expected_type);
                    }
                } else {
                    panic!("Expected arguments for {:?}", expected_type);
                }
            }
        }

        #[test]
        fn test_mixed_ignore_and_named_arguments() {
            let source = r#"fn test(a: i32, _: bool, b: String) -> i32 { return a; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let functions = typed_context.functions();
            assert_eq!(functions.len(), 1, "Expected 1 function");
            let func = &functions[0];
            if let Some(arguments) = &func.arguments {
                assert_eq!(arguments.len(), 3, "Expected 3 arguments");
                if let ArgumentType::Argument(arg) = &arguments[0] {
                    let arg_type = typed_context.get_node_typeinfo(arg.id);
                    assert!(arg_type.is_some(), "First argument should have type info");
                    assert!(
                        matches!(
                            arg_type.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32)
                        ),
                        "First argument should be i32"
                    );
                } else {
                    panic!("Expected Argument at position 0");
                }
                if let ArgumentType::IgnoreArgument(ignore_arg) = &arguments[1] {
                    let arg_type = typed_context.get_node_typeinfo(ignore_arg.id);
                    assert!(
                        arg_type.is_some(),
                        "Second argument (ignore) should have type info"
                    );
                    assert!(
                        matches!(arg_type.unwrap().kind, TypeInfoKind::Bool),
                        "Second argument should be bool"
                    );
                } else {
                    panic!("Expected IgnoreArgument at position 1");
                }
                if let ArgumentType::Argument(arg) = &arguments[2] {
                    let arg_type = typed_context.get_node_typeinfo(arg.id);
                    assert!(arg_type.is_some(), "Third argument should have type info");
                    assert!(
                        matches!(arg_type.unwrap().kind, TypeInfoKind::String),
                        "Third argument should be String"
                    );
                } else {
                    panic!("Expected Argument at position 2");
                }
            } else {
                panic!("Expected arguments");
            }
        }

        #[test]
        fn test_ignore_argument_with_string_type() {
            let source = r#"fn test(_: String) -> i32 { return 1; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let functions = typed_context.functions();
            assert_eq!(functions.len(), 1, "Expected 1 function");
            let func = &functions[0];
            if let Some(arguments) = &func.arguments {
                assert_eq!(arguments.len(), 1, "Expected 1 argument");
                if let ArgumentType::IgnoreArgument(ignore_arg) = &arguments[0] {
                    let arg_type = typed_context.get_node_typeinfo(ignore_arg.id);
                    assert!(
                        arg_type.is_some(),
                        "IgnoreArgument with String should have type info"
                    );
                    assert!(
                        matches!(arg_type.unwrap().kind, TypeInfoKind::String),
                        "IgnoreArgument should have String type"
                    );
                } else {
                    panic!("Expected IgnoreArgument");
                }
            } else {
                panic!("Expected arguments");
            }
        }

        #[test]
        fn test_ignore_argument_with_bool_type() {
            let source = r#"fn test(_: bool) -> i32 { return 1; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let functions = typed_context.functions();
            assert_eq!(functions.len(), 1, "Expected 1 function");
            let func = &functions[0];
            if let Some(arguments) = &func.arguments {
                assert_eq!(arguments.len(), 1, "Expected 1 argument");
                if let ArgumentType::IgnoreArgument(ignore_arg) = &arguments[0] {
                    let arg_type = typed_context.get_node_typeinfo(ignore_arg.id);
                    assert!(
                        arg_type.is_some(),
                        "IgnoreArgument with bool should have type info"
                    );
                    assert!(
                        matches!(arg_type.unwrap().kind, TypeInfoKind::Bool),
                        "IgnoreArgument should have bool type"
                    );
                } else {
                    panic!("Expected IgnoreArgument");
                }
            } else {
                panic!("Expected arguments");
            }
        }

        #[test]
        fn test_array_parameter_type_info() {
            let source = r#"fn test(arr: [i32; 5]) -> i32 { return arr[0]; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let functions = typed_context.functions();
            assert_eq!(functions.len(), 1, "Expected 1 function");
            let func = &functions[0];
            if let Some(arguments) = &func.arguments {
                assert_eq!(arguments.len(), 1, "Expected 1 argument");
                if let ArgumentType::Argument(arg) = &arguments[0] {
                    let arg_type = typed_context.get_node_typeinfo(arg.id);
                    assert!(arg_type.is_some(), "Array parameter should have type info");
                    if let TypeInfoKind::Array(element_type, size) = &arg_type.unwrap().kind {
                        assert!(
                            matches!(element_type.kind, TypeInfoKind::Number(NumberType::I32)),
                            "Array element should be i32"
                        );
                        assert_eq!(*size, 5, "Array size should be 5");
                    } else {
                        panic!("Expected Array type");
                    }
                } else {
                    panic!("Expected Argument");
                }
            } else {
                panic!("Expected arguments");
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

            let binary_exprs = typed_context
                .filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Binary(_))));
            assert_eq!(binary_exprs.len(), 1, "Expected 1 binary expression");

            if let AstNode::Expression(Expression::Binary(bin_expr)) = &binary_exprs[0] {
                let type_info = typed_context.get_node_typeinfo(bin_expr.id);
                assert!(
                    type_info.is_some(),
                    "Binary add expression should have type info"
                );
                assert!(
                    matches!(
                        type_info.unwrap().kind,
                        TypeInfoKind::Number(NumberType::I32)
                    ),
                    "Binary add of i32 literals should return i32"
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

            let binary_exprs = typed_context
                .filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Binary(_))));
            assert_eq!(binary_exprs.len(), 1, "Expected 1 binary expression");

            if let AstNode::Expression(Expression::Binary(bin_expr)) = &binary_exprs[0] {
                let type_info = typed_context.get_node_typeinfo(bin_expr.id);
                assert!(
                    type_info.is_some(),
                    "Logical AND expression should have type info"
                );
                assert!(
                    matches!(type_info.unwrap().kind, TypeInfoKind::Bool),
                    "Logical AND should return Bool"
                );
            }
        }

        #[test]
        fn test_nested_binary_expression_type() {
            let source = r#"fn test() -> i32 { return (10 + 20) * 30; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let binary_exprs = typed_context
                .filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Binary(_))));
            // Should have 2 binary expressions: (10 + 20) and (...) * 30
            assert_eq!(binary_exprs.len(), 2, "Expected 2 binary expressions");

            for expr in &binary_exprs {
                if let AstNode::Expression(Expression::Binary(bin_expr)) = expr {
                    let type_info = typed_context.get_node_typeinfo(bin_expr.id);
                    assert!(
                        type_info.is_some(),
                        "Nested binary expression should have type info"
                    );
                    assert!(
                        matches!(
                            type_info.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32)
                        ),
                        "Nested arithmetic expression should return i32"
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

            let fn_calls = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::FunctionCall(_)))
            });
            assert_eq!(fn_calls.len(), 1, "Expected 1 function call");

            if let AstNode::Expression(Expression::FunctionCall(call)) = &fn_calls[0] {
                let type_info = typed_context.get_node_typeinfo(call.id);
                assert!(
                    type_info.is_some(),
                    "Function call should have return type info"
                );
                assert!(
                    matches!(
                        type_info.unwrap().kind,
                        TypeInfoKind::Number(NumberType::I32)
                    ),
                    "helper() should return i32"
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

            let fn_calls = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::FunctionCall(_)))
            });
            assert_eq!(fn_calls.len(), 1, "Expected 1 function call");

            if let AstNode::Expression(Expression::FunctionCall(call)) = &fn_calls[0] {
                let type_info = typed_context.get_node_typeinfo(call.id);
                assert!(
                    type_info.is_some(),
                    "Function call with args should have return type info"
                );
                assert!(
                    matches!(
                        type_info.unwrap().kind,
                        TypeInfoKind::Number(NumberType::I32)
                    ),
                    "add() should return i32"
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

            let fn_calls = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::FunctionCall(_)))
            });
            // 2 function calls: outer double() and inner double(5)
            assert_eq!(fn_calls.len(), 2, "Expected 2 function calls");

            for call_node in &fn_calls {
                if let AstNode::Expression(Expression::FunctionCall(call)) = call_node {
                    let type_info = typed_context.get_node_typeinfo(call.id);
                    assert!(
                        type_info.is_some(),
                        "Chained function call should have return type info"
                    );
                    assert!(
                        matches!(
                            type_info.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32)
                        ),
                        "double() should return i32"
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
                TypeInfoKind::Number(NumberType::I8),
                TypeInfoKind::Number(NumberType::I16),
                TypeInfoKind::Number(NumberType::I32),
                TypeInfoKind::Number(NumberType::I64),
                TypeInfoKind::Number(NumberType::U8),
                TypeInfoKind::Number(NumberType::U16),
                TypeInfoKind::Number(NumberType::U32),
                TypeInfoKind::Number(NumberType::U64),
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
                        TypeInfoKind::Number(NumberType::I32)
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
            // Test that parameter identifiers are found in the AST
            let source = r#"fn test(x: i32, y: i32) -> bool { return x > y; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let identifiers = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::Identifier(_)))
            });
            assert!(!identifiers.is_empty(), "Expected identifier expressions");

            // FIXME: Identifier type info storage has inconsistent behavior due to
            // UUID-based node IDs. The type checker sets type info during inference,
            // but lookup by ID may fail due to arena/node ID synchronization issues.
            // Expected behavior when fixed: type_info.is_some() with i32 type.
            let mut found_identifier = false;
            for id_node in &identifiers {
                if let AstNode::Expression(Expression::Identifier(id)) = id_node
                    && (id.name == "x" || id.name == "y")
                {
                    found_identifier = true;
                    // Document current behavior - type info lookup may return None
                    let _type_info = typed_context.get_node_typeinfo(id.id);
                }
            }
            assert!(found_identifier, "Should have found identifiers x or y");
        }

        #[test]
        fn test_local_variable_identifier_type() {
            // Test that local variable identifiers are found in the AST
            let source = r#"
            fn test() -> bool {
                let flag: bool = true;
                return flag;
            }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let identifiers = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::Identifier(_)))
            });

            // FIXME: Identifier type info storage has inconsistent behavior.
            // Expected behavior when fixed: type_info.is_some() with Bool type.
            let mut found_flag = false;
            for id_node in &identifiers {
                if let AstNode::Expression(Expression::Identifier(id)) = id_node
                    && id.name == "flag"
                {
                    found_flag = true;
                    // Document current behavior - type info lookup may return None
                    let _type_info = typed_context.get_node_typeinfo(id.id);
                }
            }
            assert!(found_flag, "Should have found identifier 'flag'");
        }
    }

    /// Tests for struct field type inference (Phase 2)
    mod struct_fields {
        use super::*;
        use inference_ast::nodes::MemberAccessExpression;

        #[test]
        fn test_struct_field_type_inference_single_field() {
            let source = r#"
            struct Point { x: i32; }
            fn test(p: Point) -> i32 { return p.x; }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let member_access = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::MemberAccess(_)))
            });
            assert_eq!(
                member_access.len(),
                1,
                "Expected 1 member access expression"
            );

            if let AstNode::Expression(Expression::MemberAccess(ma)) = &member_access[0] {
                let field_type = typed_context.get_node_typeinfo(ma.id);
                assert!(field_type.is_some(), "Field access should have type info");
                assert!(
                    matches!(
                        field_type.unwrap().kind,
                        TypeInfoKind::Number(NumberType::I32)
                    ),
                    "Field x should have type i32"
                );
            }
        }

        #[test]
        fn test_struct_field_type_inference_multiple_fields() {
            let source = r#"
            struct Person { age: i32; height: u64; active: bool; }
            fn get_age(p: Person) -> i32 { return p.age; }
            fn get_height(p: Person) -> u64 { return p.height; }
            fn get_active(p: Person) -> bool { return p.active; }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let member_accesses = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::MemberAccess(_)))
            });
            assert_eq!(
                member_accesses.len(),
                3,
                "Expected 3 member access expressions"
            );

            for ma_node in &member_accesses {
                if let AstNode::Expression(Expression::MemberAccess(ma)) = ma_node {
                    let field_type = typed_context.get_node_typeinfo(ma.id);
                    assert!(
                        field_type.is_some(),
                        "Field access should have type info for field {}",
                        ma.name.name
                    );

                    let expected_kind = match ma.name.name.as_str() {
                        "age" => TypeInfoKind::Number(NumberType::I32),
                        "height" => TypeInfoKind::Number(NumberType::U64),
                        "active" => TypeInfoKind::Bool,
                        _ => panic!("Unexpected field name: {}", ma.name.name),
                    };

                    assert_eq!(
                        field_type.unwrap().kind,
                        expected_kind,
                        "Field {} should have correct type",
                        ma.name.name
                    );
                }
            }
        }

        // FIXME: Nested struct field access (e.g., o.inner.value) is currently parsed as a
        // QualifiedName expression instead of nested MemberAccess expressions.
        // The parser needs to be updated to properly handle chained member access.
        // This test documents the current behavior.
        #[test]
        fn test_nested_struct_field_access() {
            let source = r#"
            struct Inner { value: i32; }
            struct Outer { inner: Inner; }
            fn test(o: Outer) -> i32 {
                let temp: Inner = o.inner;
                return temp.value;
            }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let member_accesses = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::MemberAccess(_)))
            });
            assert_eq!(
                member_accesses.len(),
                2,
                "Expected 2 member access expressions"
            );

            for ma_node in &member_accesses {
                if let AstNode::Expression(Expression::MemberAccess(ma)) = ma_node {
                    let field_type = typed_context.get_node_typeinfo(ma.id);
                    assert!(
                        field_type.is_some(),
                        "Field access should have type info for field {}",
                        ma.name.name
                    );

                    if ma.name.name == "inner" {
                        assert_eq!(
                            field_type.unwrap().kind,
                            TypeInfoKind::Custom("Inner".to_string()),
                            "Field inner should have type Inner"
                        );
                    } else if ma.name.name == "value" {
                        assert_eq!(
                            field_type.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32),
                            "Field value should have type i32"
                        );
                    }
                }
            }
        }

        #[test]
        fn test_invalid_field_access_nonexistent_field() {
            let source = r#"
            struct Point { x: i32; }
            fn test(p: Point) -> i32 { return p.y; }
            "#;
            let result = try_type_check(source);
            assert!(
                result.is_err(),
                "Type checker should detect access to non-existent field"
            );

            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("field `y` not found on struct `Point`"),
                    "Error message should mention the missing field, got: {}",
                    error_msg
                );
            }
        }

        #[test]
        fn test_invalid_field_access_on_non_struct() {
            let source = r#"
            fn test(x: i32) -> i32 { return x.field; }
            "#;
            let result = try_type_check(source);
            assert!(
                result.is_err(),
                "Type checker should detect member access on non-struct type"
            );

            if let Err(error) = result {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("member access requires a struct type"),
                    "Error message should mention struct requirement, got: {}",
                    error_msg
                );
            }
        }

        #[test]
        fn test_struct_field_in_expression() {
            let source = r#"
            struct Counter { count: i32; }
            fn increment(c: Counter) -> i32 { return c.count + 1; }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let member_accesses = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::MemberAccess(_)))
            });
            assert_eq!(
                member_accesses.len(),
                1,
                "Expected 1 member access expression"
            );

            if let AstNode::Expression(Expression::MemberAccess(ma)) = &member_accesses[0] {
                let field_type = typed_context.get_node_typeinfo(ma.id);
                assert!(
                    field_type.is_some(),
                    "Field access in expression should have type info"
                );
                assert!(
                    matches!(
                        field_type.unwrap().kind,
                        TypeInfoKind::Number(NumberType::I32)
                    ),
                    "Field count should have type i32"
                );
            }
        }

        #[test]
        fn test_struct_with_different_numeric_types() {
            let source = r#"
            struct Numbers { a: i8; b: i16; c: i32; d: i64; e: u8; f: u16; g: u32; h: u64; }
            fn get_i8(n: Numbers) -> i8 { return n.a; }
            fn get_i16(n: Numbers) -> i16 { return n.b; }
            fn get_i32(n: Numbers) -> i32 { return n.c; }
            fn get_i64(n: Numbers) -> i64 { return n.d; }
            fn get_u8(n: Numbers) -> u8 { return n.e; }
            fn get_u16(n: Numbers) -> u16 { return n.f; }
            fn get_u32(n: Numbers) -> u32 { return n.g; }
            fn get_u64(n: Numbers) -> u64 { return n.h; }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let member_accesses = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::MemberAccess(_)))
            });
            assert_eq!(
                member_accesses.len(),
                8,
                "Expected 8 member access expressions"
            );

            for ma_node in &member_accesses {
                if let AstNode::Expression(Expression::MemberAccess(ma)) = ma_node {
                    let field_type = typed_context.get_node_typeinfo(ma.id);
                    assert!(
                        field_type.is_some(),
                        "Field {} should have type info",
                        ma.name.name
                    );

                    let expected_kind = match ma.name.name.as_str() {
                        "a" => TypeInfoKind::Number(NumberType::I8),
                        "b" => TypeInfoKind::Number(NumberType::I16),
                        "c" => TypeInfoKind::Number(NumberType::I32),
                        "d" => TypeInfoKind::Number(NumberType::I64),
                        "e" => TypeInfoKind::Number(NumberType::U8),
                        "f" => TypeInfoKind::Number(NumberType::U16),
                        "g" => TypeInfoKind::Number(NumberType::U32),
                        "h" => TypeInfoKind::Number(NumberType::U64),
                        _ => panic!("Unexpected field name: {}", ma.name.name),
                    };

                    assert_eq!(
                        field_type.unwrap().kind,
                        expected_kind,
                        "Field {} should have correct numeric type",
                        ma.name.name
                    );
                }
            }
        }

        // FIXME: Deeply nested struct field access (e.g., l1.level2.level3.value) is currently
        // parsed as a QualifiedName expression instead of nested MemberAccess expressions.
        // The parser needs to be updated to properly handle chained member access.
        // This test documents the current behavior using intermediate variables.
        #[test]
        fn test_deeply_nested_struct_access() {
            let source = r#"
            struct Level3 { value: i32; }
            struct Level2 { level3: Level3; }
            struct Level1 { level2: Level2; }
            fn test(l1: Level1) -> i32 {
                let l2: Level2 = l1.level2;
                let l3: Level3 = l2.level3;
                return l3.value;
            }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let member_accesses = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::MemberAccess(_)))
            });
            assert_eq!(
                member_accesses.len(),
                3,
                "Expected 3 member access expressions"
            );

            let mut found_level2 = false;
            let mut found_level3 = false;
            let mut found_value = false;

            for ma_node in &member_accesses {
                if let AstNode::Expression(Expression::MemberAccess(ma)) = ma_node {
                    let field_type = typed_context.get_node_typeinfo(ma.id);
                    assert!(
                        field_type.is_some(),
                        "Field {} should have type info",
                        ma.name.name
                    );

                    match ma.name.name.as_str() {
                        "level2" => {
                            assert_eq!(
                                field_type.unwrap().kind,
                                TypeInfoKind::Custom("Level2".to_string()),
                                "Field level2 should have type Level2"
                            );
                            found_level2 = true;
                        }
                        "level3" => {
                            assert_eq!(
                                field_type.unwrap().kind,
                                TypeInfoKind::Custom("Level3".to_string()),
                                "Field level3 should have type Level3"
                            );
                            found_level3 = true;
                        }
                        "value" => {
                            assert_eq!(
                                field_type.unwrap().kind,
                                TypeInfoKind::Number(NumberType::I32),
                                "Field value should have type i32"
                            );
                            found_value = true;
                        }
                        _ => panic!("Unexpected field name: {}", ma.name.name),
                    }
                }
            }

            assert!(found_level2, "Should find level2 field access");
            assert!(found_level3, "Should find level3 field access");
            assert!(found_value, "Should find value field access");
        }

        #[test]
        fn test_struct_field_in_variable_definition() {
            let source = r#"
            struct Data { value: i32; }
            fn test(d: Data) {
                let x: i32 = d.value;
            }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let member_accesses = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::MemberAccess(_)))
            });
            assert_eq!(
                member_accesses.len(),
                1,
                "Expected 1 member access expression"
            );

            if let AstNode::Expression(Expression::MemberAccess(ma)) = &member_accesses[0] {
                let field_type = typed_context.get_node_typeinfo(ma.id);
                assert!(
                    field_type.is_some(),
                    "Field access in variable definition should have type info"
                );
                assert!(
                    matches!(
                        field_type.unwrap().kind,
                        TypeInfoKind::Number(NumberType::I32)
                    ),
                    "Field value should have type i32"
                );
            }
        }
    }

    /// Tests for method resolution and type inference (Phase 3)
    mod methods {
        use super::*;

        #[test]
        fn test_method_call_return_type() {
            let source = r#"
            struct Counter {
                value: i32;

                fn get(self) -> i32 { return 42; }
            }
            fn test(c: Counter) -> i32 { return c.get(); }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let fn_calls = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::FunctionCall(_)))
            });
            assert_eq!(fn_calls.len(), 1, "Expected 1 function call expression");

            if let AstNode::Expression(Expression::FunctionCall(call)) = &fn_calls[0] {
                let return_type = typed_context.get_node_typeinfo(call.id);
                assert!(
                    return_type.is_some(),
                    "Method call should have return type info"
                );
                assert!(
                    matches!(
                        return_type.unwrap().kind,
                        TypeInfoKind::Number(NumberType::I32)
                    ),
                    "Method get() should return i32"
                );
            }
        }

        #[test]
        fn test_method_with_parameter() {
            let source = r#"
            struct Calculator {
                value: i32;

                fn add(self, x: i32) -> i32 { return x; }
            }
            fn test(c: Calculator) -> i32 { return c.add(10); }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let fn_calls = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::FunctionCall(_)))
            });
            assert_eq!(fn_calls.len(), 1, "Expected 1 function call expression");

            if let AstNode::Expression(Expression::FunctionCall(call)) = &fn_calls[0] {
                let return_type = typed_context.get_node_typeinfo(call.id);
                assert!(
                    return_type.is_some(),
                    "Method call with parameter should have return type info"
                );
                assert!(
                    matches!(
                        return_type.unwrap().kind,
                        TypeInfoKind::Number(NumberType::I32)
                    ),
                    "Method add() should return i32"
                );
            }
        }

        #[test]
        fn test_method_returning_bool() {
            let source = r#"
            struct Checker {
                valid: bool;

                fn is_valid(self) -> bool { return true; }
            }
            fn test(c: Checker) -> bool { return c.is_valid(); }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let fn_calls = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::FunctionCall(_)))
            });
            assert_eq!(fn_calls.len(), 1, "Expected 1 function call expression");

            if let AstNode::Expression(Expression::FunctionCall(call)) = &fn_calls[0] {
                let return_type = typed_context.get_node_typeinfo(call.id);
                assert!(
                    return_type.is_some(),
                    "Method call should have return type info"
                );
                assert!(
                    matches!(return_type.unwrap().kind, TypeInfoKind::Bool),
                    "Method is_valid() should return bool"
                );
            }
        }

        #[test]
        fn test_multiple_methods_on_struct() {
            let source = r#"
            struct Data {
                x: i32;
                y: i32;

                fn get_x(self) -> i32 { return 1; }
                fn get_y(self) -> i32 { return 2; }
            }
            fn test_x(d: Data) -> i32 { return d.get_x(); }
            fn test_y(d: Data) -> i32 { return d.get_y(); }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let fn_calls = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::FunctionCall(_)))
            });
            assert_eq!(fn_calls.len(), 2, "Expected 2 function call expressions");
        }

        #[test]
        fn test_method_call_error_nonexistent_method() {
            let source = r#"
            struct Empty {}
            fn test(e: Empty) -> i32 { return e.nonexistent(); }
            "#;
            let arena = build_ast(source.to_string());
            let result = TypeCheckerBuilder::build_typed_context(arena);
            assert!(
                result.is_err(),
                "Type checker should report error for nonexistent method"
            );
        }

        #[test]
        fn test_method_with_multiple_parameters() {
            let source = r#"
            struct Math {
                base: i32;

                fn compute(self, a: i32, b: i32) -> i32 { return a; }
            }
            fn test(m: Math) -> i32 { return m.compute(1, 2); }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let fn_calls = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::FunctionCall(_)))
            });
            assert_eq!(fn_calls.len(), 1, "Expected 1 function call expression");

            if let AstNode::Expression(Expression::FunctionCall(call)) = &fn_calls[0] {
                let return_type = typed_context.get_node_typeinfo(call.id);
                assert!(
                    return_type.is_some(),
                    "Method call with multiple parameters should have return type info"
                );
            }
        }

        #[test]
        fn test_method_with_self_parameter() {
            let source = r#"
            struct Container {
                data: i32;

                fn process(self) -> i32 {
                    return 42;
                }
            }
            fn test(c: Container) -> i32 { return c.process(); }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let fn_calls = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::FunctionCall(_)))
            });
            assert_eq!(fn_calls.len(), 1, "Expected 1 function call expression");
        }

        #[test]
        fn test_method_wrong_argument_count_error() {
            let source = r#"
            struct Test {
                value: i32;

                fn needs_one(x: i32) -> i32 { return x; }
            }
            fn test(t: Test) -> i32 { return t.needs_one(); }
            "#;
            let arena = build_ast(source.to_string());
            let result = TypeCheckerBuilder::build_typed_context(arena);
            assert!(
                result.is_err(),
                "Type checker should report error for wrong argument count"
            );
        }

        #[test]
        fn test_method_call_on_non_struct_type_error() {
            let source = r#"
            fn test(x: i32) -> i32 { return x.method(); }
            "#;
            let arena = build_ast(source.to_string());
            let result = TypeCheckerBuilder::build_typed_context(arena);
            assert!(
                result.is_err(),
                "Type checker should report error for method call on non-struct type"
            );
        }

        #[test]
        fn test_self_access_in_method_body() {
            let source = r#"
            struct Container {
                data: i32;

                fn process(self) -> i32 {
                    let x: i32 = self.data;
                    return x;
                }
            }
            fn test(c: Container) -> i32 { return c.process(); }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let member_accesses = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::MemberAccess(_)))
            });
            assert!(
                !member_accesses.is_empty(),
                "Expected at least 1 member access expression for self.data"
            );
        }

        #[test]
        fn test_multiple_self_usages_in_method() {
            let source = r#"
            struct Point {
                x: i32;
                y: i32;

                fn sum(self) -> i32 {
                    return self.x + self.y;
                }
            }
            fn test(p: Point) -> i32 { return p.sum(); }
            "#;
            try_type_check(source).expect("Type checking should succeed");
        }

        #[test]
        fn test_self_method_call() {
            let source = r#"
            struct Counter {
                value: i32;

                fn get_value(self) -> i32 {
                    return self.value;
                }

                fn doubled(self) -> i32 {
                    return self.get_value() + self.get_value();
                }
            }
            fn test(c: Counter) -> i32 { return c.doubled(); }
            "#;
            try_type_check(source).expect("Type checking should succeed");
        }

        #[test]
        fn test_self_in_standalone_function_error() {
            let source = r#"fn method(self, x: i32) -> i32 { return x; }"#;
            let arena = build_ast(source.to_string());
            let result = TypeCheckerBuilder::build_typed_context(arena);
            assert!(
                result.is_err(),
                "Expected error for self in standalone function"
            );
            let err_msg = result.err().unwrap().to_string();
            assert!(
                err_msg.contains("self reference not allowed in standalone function"),
                "Expected SelfReferenceInFunction error, got: {err_msg}"
            );
        }
    }
}
