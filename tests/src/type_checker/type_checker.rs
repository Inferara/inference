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
                always!(
                    literal_type.is_some(),
                    "Number literal should have type info"
                );
                always!(
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
                always!(type_info.is_some(), "Bool literal should have type info");
                always!(
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
            always!(func.returns.is_some(), "Function should have return type");
            let return_type = typed_context.get_node_typeinfo(func.returns.as_ref().unwrap().id());
            always!(
                return_type.is_some(),
                "Function return type should have type info"
            );
            always!(
                matches!(return_type.unwrap().kind, TypeInfoKind::String),
                "Function return type should be String"
            );
            if let Some(arguments) = &func.arguments {
                always!(!arguments.is_empty(), "Function should have arguments");
                let param_type = typed_context.get_node_typeinfo(arguments[0].id());
                always!(
                    param_type.is_some(),
                    "Function parameter should have type info"
                );
                let param_type = param_type.unwrap();
                always!(
                    matches!(param_type.kind, TypeInfoKind::String),
                    "Function parameter should have String type"
                );
            } else {
                panic!("Function should have arguments");
            }
        }

        #[test]
        fn test_variable_type_inference() {
            let source = r#"fn test() {let x: i32 = 10;let y: bool = true;}"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            assert_eq!(typed_context.source_files().len(), 1);
            let var_defs = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Statement(Statement::VariableDefinition(_)))
            });
            assert_eq!(var_defs.len(), 2, "Expected 2 variable definitions");
            for var_node in &var_defs {
                if let AstNode::Statement(Statement::VariableDefinition(var_def)) = var_node {
                    let type_info = typed_context.get_node_typeinfo(var_def.id);
                    always!(
                        type_info.is_some(),
                        "Variable '{}' should have type info",
                        var_def.name.name
                    );
                    match var_def.name.name.as_str() {
                        "x" => always!(
                            matches!(
                                type_info.unwrap().kind,
                                TypeInfoKind::Number(NumberType::I32)
                            ),
                            "Variable x should have i32 type"
                        ),
                        "y" => always!(
                            matches!(type_info.unwrap().kind, TypeInfoKind::Bool),
                            "Variable y should have bool type"
                        ),
                        _ => panic!("Unexpected variable name: {}", var_def.name.name),
                    }
                }
            }
        }

        #[test]
        fn test_all_numeric_types_type_check() {
            use inference_ast::nodes::ArgumentType;
            for expected_type in NumberType::ALL {
                let type_name = expected_type.as_str();
                let source = format!("fn test(x: {type_name}) -> {type_name} {{ return x; }}");
                let typed_context = try_type_check(&source)
                    .expect("Type checking should succeed for numeric types");
                assert_eq!(
                    typed_context.source_files().len(),
                    1,
                    "Type checking should succeed for {} type",
                    type_name
                );
                let functions = typed_context.functions();
                assert_eq!(functions.len(), 1, "Expected 1 function for {}", type_name);
                let func = &functions[0];
                always!(
                    func.returns.is_some(),
                    "Function should have return type for {}",
                    type_name
                );
                let return_type =
                    typed_context.get_node_typeinfo(func.returns.as_ref().unwrap().id());
                always!(
                    return_type.is_some(),
                    "Return type should have type info for {}",
                    type_name
                );
                always!(
                    matches!(
                        return_type.unwrap().kind,
                        TypeInfoKind::Number(n) if n == *expected_type
                    ),
                    "Return type should be {} for {}",
                    type_name,
                    type_name
                );
                if let Some(arguments) = &func.arguments {
                    assert_eq!(arguments.len(), 1, "Expected 1 argument for {}", type_name);
                    if let ArgumentType::Argument(arg) = &arguments[0] {
                        let arg_type = typed_context.get_node_typeinfo(arg.id);
                        always!(
                            arg_type.is_some(),
                            "Argument should have type info for {}",
                            type_name
                        );
                        always!(
                            matches!(
                                arg_type.unwrap().kind,
                                TypeInfoKind::Number(n) if n == *expected_type
                            ),
                            "Argument should have {} type for {}",
                            type_name,
                            type_name
                        );
                    } else {
                        panic!("Expected Argument for {}", type_name);
                    }
                } else {
                    panic!("Function should have arguments for {}", type_name);
                }
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
                    always!(arg_type.is_some(), "Argument node should have type info");
                    always!(
                        matches!(
                            arg_type.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32)
                        ),
                        "Argument should have i32 type"
                    );
                    let name_type = typed_context.get_node_typeinfo(arg.name.id);
                    always!(name_type.is_some(), "Argument name should have type info");
                    always!(
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
                        always!(
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
                        always!(
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
                    always!(
                        arg_type.is_some(),
                        "IgnoreArgument node should have type info"
                    );
                    always!(
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
                        always!(
                            arg_type.is_some(),
                            "IgnoreArgument should have type info for {:?}",
                            expected_type
                        );
                        always!(
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
                    always!(arg_type.is_some(), "First argument should have type info");
                    always!(
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
                    always!(
                        arg_type.is_some(),
                        "Second argument (ignore) should have type info"
                    );
                    always!(
                        matches!(arg_type.unwrap().kind, TypeInfoKind::Bool),
                        "Second argument should be bool"
                    );
                } else {
                    panic!("Expected IgnoreArgument at position 1");
                }
                if let ArgumentType::Argument(arg) = &arguments[2] {
                    let arg_type = typed_context.get_node_typeinfo(arg.id);
                    always!(arg_type.is_some(), "Third argument should have type info");
                    always!(
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
                    always!(
                        arg_type.is_some(),
                        "IgnoreArgument with String should have type info"
                    );
                    always!(
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
                    always!(
                        arg_type.is_some(),
                        "IgnoreArgument with bool should have type info"
                    );
                    always!(
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
                    always!(arg_type.is_some(), "Array parameter should have type info");
                    if let TypeInfoKind::Array(element_type, size) = &arg_type.unwrap().kind {
                        always!(
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
                always!(
                    type_info.is_some(),
                    "Binary add expression should have type info"
                );
                always!(
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
                always!(type_info.is_some(), "Comparison should have type info");
                always!(
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
                always!(
                    type_info.is_some(),
                    "Logical AND expression should have type info"
                );
                always!(
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
                    always!(
                        type_info.is_some(),
                        "Nested binary expression should have type info"
                    );
                    always!(
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
                always!(
                    call.name() == "helper",
                    "Function call should be to 'helper'"
                );
                let type_info = typed_context.get_node_typeinfo(call.id);
                always!(
                    type_info.is_some(),
                    "Function call should have return type info"
                );
                always!(
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
                always!(call.name() == "add", "Function call should be to 'add'");
                let type_info = typed_context.get_node_typeinfo(call.id);
                always!(
                    type_info.is_some(),
                    "Function call with args should have return type info"
                );
                always!(
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
                    always!(
                        type_info.is_some(),
                        "Chained function call should have return type info"
                    );
                    always!(
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

            let if_statements = typed_context
                .filter_nodes(|node| matches!(node, AstNode::Statement(Statement::If(_))));
            assert_eq!(if_statements.len(), 1, "Expected 1 if statement");

            if let AstNode::Statement(Statement::If(if_stmt)) = &if_statements[0] {
                let condition = if_stmt.condition.borrow();
                if let Expression::Binary(bin_expr) = &*condition {
                    let cond_type = typed_context.get_node_typeinfo(bin_expr.id);
                    always!(
                        cond_type.is_some(),
                        "If condition (comparison) should have type info"
                    );
                    always!(
                        matches!(cond_type.unwrap().kind, TypeInfoKind::Bool),
                        "Comparison expression should have bool type"
                    );
                } else {
                    panic!("Expected Binary expression as condition");
                }
            } else {
                panic!("Expected IfStatement");
            }
        }

        #[test]
        fn test_if_statement_with_bool_condition() {
            use inference_ast::nodes::ArgumentType;

            let source =
                r#"fn test(flag: bool) -> i32 { if flag { return 1; } else { return 0; } }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            assert_eq!(typed_context.source_files().len(), 1);

            let if_statements = typed_context
                .filter_nodes(|node| matches!(node, AstNode::Statement(Statement::If(_))));
            assert_eq!(if_statements.len(), 1, "Expected 1 if statement");

            if let AstNode::Statement(Statement::If(if_stmt)) = &if_statements[0] {
                let condition = if_stmt.condition.borrow();
                if let Expression::Identifier(id) = &*condition {
                    assert_eq!(id.name, "flag", "Condition should be the 'flag' identifier");
                    let cond_type = typed_context.get_node_typeinfo(id.id);
                    always!(
                        cond_type.is_some(),
                        "If condition (identifier) should have type info"
                    );
                    always!(
                        matches!(cond_type.unwrap().kind, TypeInfoKind::Bool),
                        "Identifier 'flag' should have bool type"
                    );
                } else {
                    panic!("Expected Identifier expression as condition");
                }
            } else {
                panic!("Expected IfStatement");
            }

            let functions = typed_context.functions();
            assert_eq!(functions.len(), 1, "Expected 1 function");
            let func = &functions[0];
            if let Some(arguments) = &func.arguments {
                assert_eq!(arguments.len(), 1, "Expected 1 argument");
                if let ArgumentType::Argument(arg) = &arguments[0] {
                    let arg_type = typed_context.get_node_typeinfo(arg.id);
                    always!(arg_type.is_some(), "Parameter 'flag' should have type info");
                    always!(
                        matches!(arg_type.unwrap().kind, TypeInfoKind::Bool),
                        "Parameter 'flag' should have bool type"
                    );
                }
            }
        }

        #[test]
        fn test_loop_with_break() {
            let source = r#"fn test() { loop { break; } }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            assert_eq!(typed_context.source_files().len(), 1);

            let loop_statements = typed_context
                .filter_nodes(|node| matches!(node, AstNode::Statement(Statement::Loop(_))));
            assert_eq!(loop_statements.len(), 1, "Expected 1 loop statement");

            let break_statements = typed_context
                .filter_nodes(|node| matches!(node, AstNode::Statement(Statement::Break(_))));
            assert_eq!(break_statements.len(), 1, "Expected 1 break statement");

            let functions = typed_context.functions();
            assert_eq!(functions.len(), 1, "Expected 1 function");
            always!(
                functions[0].returns.is_none(),
                "Function with loop should have no explicit return type"
            );
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
            let assign_statements = typed_context
                .filter_nodes(|node| matches!(node, AstNode::Statement(Statement::Assign(_))));
            assert_eq!(
                assign_statements.len(),
                1,
                "Expected 1 assignment statement"
            );
            if let AstNode::Statement(Statement::Assign(assign_stmt)) = &assign_statements[0] {
                let right = assign_stmt.right.borrow();
                if let Expression::Literal(Literal::Number(num_lit)) = &*right {
                    let rhs_type = typed_context.get_node_typeinfo(num_lit.id);
                    always!(
                        rhs_type.is_some(),
                        "RHS of assignment should have type info"
                    );
                    always!(
                        matches!(
                            rhs_type.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32)
                        ),
                        "RHS should be i32 to match variable type"
                    );
                } else {
                    panic!("Expected number literal as RHS");
                }
                let left = assign_stmt.left.borrow();
                if let Expression::Identifier(id) = &*left {
                    let lhs_type = typed_context.get_node_typeinfo(id.id);
                    always!(
                        lhs_type.is_some(),
                        "LHS of assignment should have type info"
                    );
                    always!(
                        matches!(
                            lhs_type.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32)
                        ),
                        "LHS should be i32 to match variable type"
                    );
                } else {
                    panic!("Expected identifier as LHS");
                }
            } else {
                panic!("Expected AssignStatement");
            }

            let var_defs = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Statement(Statement::VariableDefinition(_)))
            });
            assert_eq!(var_defs.len(), 1, "Expected 1 variable definition");
            if let AstNode::Statement(Statement::VariableDefinition(var_def)) = &var_defs[0] {
                let type_info = typed_context.get_node_typeinfo(var_def.id);
                always!(type_info.is_some(), "Variable 'x' should have type info");
                always!(
                    matches!(
                        type_info.unwrap().kind,
                        TypeInfoKind::Number(NumberType::I32)
                    ),
                    "Variable 'x' should have i32 type"
                );
            }
        }
    }

    /// Tests for array type inference
    mod arrays {
        use inference_ast::nodes::Definition;

        use super::*;

        // FIXME: Array indexing (arr[0]) type inference is not fully implemented.
        // Currently parsing succeeds but type inference may not correctly resolve
        // the element type when accessing array elements.
        // Expected behavior: arr[0] on [i32; 1] should infer as i32.
        #[test]
        fn test_array_type() {
            let source = r#"fn get_first(arr: [i32; 1]) -> i32 { return arr[0]; }"#;
            let arena = build_ast(source.to_string());
            assert_eq!(arena.source_files().len(), 1);
        }

        #[test]
        fn test_nested_arrays() {
            use inference_ast::nodes::ArgumentType;

            let source = r#"fn test(matrix: [[bool; 2]; 1]) { assert(true); }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            assert_eq!(typed_context.source_files().len(), 1);

            let functions = typed_context.functions();
            assert_eq!(functions.len(), 1, "Expected 1 function");
            let func = &functions[0];

            if let Some(arguments) = &func.arguments {
                assert_eq!(arguments.len(), 1, "Expected 1 argument");
                if let ArgumentType::Argument(arg) = &arguments[0] {
                    let arg_type = typed_context.get_node_typeinfo(arg.id);
                    always!(
                        arg_type.is_some(),
                        "Nested array parameter should have type info"
                    );

                    if let TypeInfoKind::Array(outer_elem, outer_size) = &arg_type.unwrap().kind {
                        assert_eq!(*outer_size, 1, "Outer array size should be 1");

                        if let TypeInfoKind::Array(inner_elem, inner_size) = &outer_elem.kind {
                            assert_eq!(*inner_size, 2, "Inner array size should be 2");
                            always!(
                                matches!(inner_elem.kind, TypeInfoKind::Bool),
                                "Inner array element should be bool"
                            );
                        } else {
                            panic!("Expected inner array type");
                        }
                    } else {
                        panic!("Expected outer array type");
                    }
                } else {
                    panic!("Expected Argument");
                }
            } else {
                panic!("Function should have arguments");
            }
        }
    }

    /// Tests for Uzumaki (@) expression type inference
    mod uzumaki {
        use super::*;

        #[test]
        fn test_uzumaki_numeric_type_inference() {
            let source_code = r#"
            fn foo() {
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
            always!(
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
                    always!(
                        typed_context.get_node_typeinfo(uzumaki.id).unwrap().kind
                            == expected_types[i],
                        "Expected type {} for UzumakiExpression, found {:?}",
                        expected_types[i],
                        typed_context.get_node_typeinfo(uzumaki.id).unwrap().kind
                    );
                }
            }

            for c in "abcdefgh".to_string().chars() {
                for identifier in typed_context.filter_nodes(|node| {
                    matches!(node, AstNode::Expression(Expression::Identifier(id)) if id.name == c.to_string())
                }) {
                    if let AstNode::Expression(Expression::Identifier(id)) = identifier {
                        let type_info = typed_context.get_node_typeinfo(id.id);
                        always!(
                            type_info.is_some(),
                            "Identifier '{}' should have type info",
                            c
                        );
                        let expected_type = match c {
                            'a' => TypeInfoKind::Number(NumberType::I8),
                            'b' => TypeInfoKind::Number(NumberType::I16),
                            'c' => TypeInfoKind::Number(NumberType::I32),
                            'd' => TypeInfoKind::Number(NumberType::I64),
                            'e' => TypeInfoKind::Number(NumberType::U8),
                            'f' => TypeInfoKind::Number(NumberType::U16),
                            'g' => TypeInfoKind::Number(NumberType::U32),
                            'h' => TypeInfoKind::Number(NumberType::U64),
                            _ => panic!("Unexpected identifier"),
                        };
                        always!(
                            type_info.unwrap().kind == expected_type,
                            "Identifier '{}' should have type {:?}",
                            c,
                            expected_type
                        );
                    }
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
                always!(
                    type_info.is_some(),
                    "Uzumaki in return should have type info"
                );
                always!(
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
            use inference_ast::nodes::ArgumentType;

            let source = r#"fn test(x: i32, y: i32) -> bool { return x > y; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let identifiers = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::Identifier(_)))
            });
            always!(!identifiers.is_empty(), "Expected identifier expressions");

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
            always!(found_identifier, "Should have found identifiers x or y");

            let functions = typed_context.functions();
            assert_eq!(functions.len(), 1, "Expected 1 function");
            let func = &functions[0];
            if let Some(arguments) = &func.arguments {
                assert_eq!(arguments.len(), 2, "Expected 2 arguments");
                for (i, arg_type) in arguments.iter().enumerate() {
                    if let ArgumentType::Argument(arg) = arg_type {
                        let arg_type_info = typed_context.get_node_typeinfo(arg.id);
                        always!(
                            arg_type_info.is_some(),
                            "Argument {} should have type info",
                            i
                        );
                        always!(
                            matches!(
                                arg_type_info.unwrap().kind,
                                TypeInfoKind::Number(NumberType::I32)
                            ),
                            "Argument {} should have i32 type",
                            i
                        );
                    }
                }
            }

            let binary_exprs = typed_context
                .filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Binary(_))));
            assert_eq!(binary_exprs.len(), 1, "Expected 1 binary comparison");

            if let AstNode::Expression(Expression::Binary(bin_expr)) = &binary_exprs[0] {
                let type_info = typed_context.get_node_typeinfo(bin_expr.id);
                always!(type_info.is_some(), "Comparison should have type info");
                always!(
                    matches!(type_info.unwrap().kind, TypeInfoKind::Bool),
                    "Comparison should return bool"
                );
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
            always!(found_flag, "Should have found identifier 'flag'");

            let var_defs = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Statement(Statement::VariableDefinition(_)))
            });
            assert_eq!(var_defs.len(), 1, "Expected 1 variable definition");

            if let AstNode::Statement(Statement::VariableDefinition(var_def)) = &var_defs[0] {
                let type_info = typed_context.get_node_typeinfo(var_def.id);
                always!(type_info.is_some(), "Variable 'flag' should have type info");
                always!(
                    matches!(type_info.unwrap().kind, TypeInfoKind::Bool),
                    "Variable 'flag' should have bool type"
                );
                assert_eq!(var_def.name.name, "flag", "Variable name should be 'flag'");
            }

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
                always!(type_info.is_some(), "Bool literal should have type info");
                always!(
                    matches!(type_info.unwrap().kind, TypeInfoKind::Bool),
                    "Bool literal should have Bool type"
                );
            }
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
                always!(field_type.is_some(), "Field access should have type info");
                always!(
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
                    always!(
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
                    always!(
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
            always!(
                result.is_err(),
                "Type checker should detect access to non-existent field"
            );

            if let Err(error) = result {
                let error_msg = error.to_string();
                always!(
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
            always!(
                result.is_err(),
                "Type checker should detect member access on non-struct type"
            );

            if let Err(error) = result {
                let error_msg = error.to_string();
                always!(
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
                always!(
                    field_type.is_some(),
                    "Field access in expression should have type info"
                );
                always!(
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
                    always!(
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
                    always!(
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

            always!(found_level2, "Should find level2 field access");
            always!(found_level3, "Should find level3 field access");
            always!(found_value, "Should find value field access");
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
                always!(
                    field_type.is_some(),
                    "Field access in variable definition should have type info"
                );
                always!(
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
                always!(
                    return_type.is_some(),
                    "Method call should have return type info"
                );
                always!(
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
                always!(
                    return_type.is_some(),
                    "Method call with parameter should have return type info"
                );
                always!(
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
                always!(
                    return_type.is_some(),
                    "Method call should have return type info"
                );
                always!(
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

            for call_node in &fn_calls {
                if let AstNode::Expression(Expression::FunctionCall(call)) = call_node {
                    let return_type = typed_context.get_node_typeinfo(call.id);
                    always!(
                        return_type.is_some(),
                        "Method call should have return type info"
                    );
                    always!(
                        matches!(
                            return_type.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32)
                        ),
                        "Method should return i32"
                    );
                }
            }
        }

        #[test]
        fn test_method_call_error_nonexistent_method() {
            let source = r#"
            struct Empty {}
            fn test(e: Empty) -> i32 { return e.nonexistent(); }
            "#;
            let arena = build_ast(source.to_string());
            let result = TypeCheckerBuilder::build_typed_context(arena);
            always!(
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
                always!(
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
            if let AstNode::Expression(Expression::FunctionCall(call)) = &fn_calls[0] {
                let return_type = typed_context.get_node_typeinfo(call.id);
                always!(
                    return_type.is_some(),
                    "Method call with self should have return type info"
                );
                always!(
                    matches!(
                        return_type.unwrap().kind,
                        TypeInfoKind::Number(NumberType::I32)
                    ),
                    "Method process() should return i32"
                );
            }
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
            always!(
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
            always!(
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
            always!(
                !member_accesses.is_empty(),
                "Expected at least 1 member access expression for self.data"
            );

            let mut found_data_field = false;
            for ma_node in &member_accesses {
                if let AstNode::Expression(Expression::MemberAccess(ma)) = ma_node
                    && ma.name.name == "data"
                {
                    let field_type = typed_context.get_node_typeinfo(ma.id);
                    always!(field_type.is_some(), "Field access should have type info");
                    always!(
                        matches!(
                            field_type.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32)
                        ),
                        "Field data should have type i32"
                    );
                    found_data_field = true;
                }
            }
            always!(found_data_field, "Should have found self.data access");
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
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let member_accesses = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::MemberAccess(_)))
            });
            always!(
                member_accesses.len() >= 2,
                "Expected at least 2 member access expressions for self.x and self.y"
            );

            let mut found_x = false;
            let mut found_y = false;
            for ma_node in &member_accesses {
                if let AstNode::Expression(Expression::MemberAccess(ma)) = ma_node {
                    match ma.name.name.as_str() {
                        "x" => {
                            let field_type = typed_context.get_node_typeinfo(ma.id);
                            always!(field_type.is_some(), "Field x should have type info");
                            always!(
                                matches!(
                                    field_type.unwrap().kind,
                                    TypeInfoKind::Number(NumberType::I32)
                                ),
                                "Field x should have type i32"
                            );
                            found_x = true;
                        }
                        "y" => {
                            let field_type = typed_context.get_node_typeinfo(ma.id);
                            always!(field_type.is_some(), "Field y should have type info");
                            always!(
                                matches!(
                                    field_type.unwrap().kind,
                                    TypeInfoKind::Number(NumberType::I32)
                                ),
                                "Field y should have type i32"
                            );
                            found_y = true;
                        }
                        _ => {} // Allow other member accesses (like method calls)
                    }
                }
            }
            always!(found_x, "Should have found self.x access");
            always!(found_y, "Should have found self.y access");

            let binary_exprs = typed_context
                .filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Binary(_))));
            assert_eq!(
                binary_exprs.len(),
                1,
                "Expected 1 binary expression (x + y)"
            );

            if let AstNode::Expression(Expression::Binary(bin_expr)) = &binary_exprs[0] {
                let type_info = typed_context.get_node_typeinfo(bin_expr.id);
                always!(
                    type_info.is_some(),
                    "Binary expression should have type info"
                );
                always!(
                    matches!(
                        type_info.unwrap().kind,
                        TypeInfoKind::Number(NumberType::I32)
                    ),
                    "Binary expression should have type i32"
                );
            }
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
            let typed_context = try_type_check(source).expect("Type checking should succeed");

            let fn_calls = typed_context.filter_nodes(|node| {
                matches!(node, AstNode::Expression(Expression::FunctionCall(_)))
            });
            // 3 function calls: c.doubled() and two self.get_value() inside doubled
            assert_eq!(fn_calls.len(), 3, "Expected 3 function call expressions");

            for call_node in &fn_calls {
                if let AstNode::Expression(Expression::FunctionCall(call)) = call_node {
                    let return_type = typed_context.get_node_typeinfo(call.id);
                    always!(
                        return_type.is_some(),
                        "Method call should have return type info"
                    );
                    always!(
                        matches!(
                            return_type.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32)
                        ),
                        "All methods should return i32"
                    );
                }
            }

            let binary_exprs = typed_context
                .filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Binary(_))));
            assert_eq!(
                binary_exprs.len(),
                1,
                "Expected 1 binary expression (get_value() + get_value())"
            );

            if let AstNode::Expression(Expression::Binary(bin_expr)) = &binary_exprs[0] {
                let type_info = typed_context.get_node_typeinfo(bin_expr.id);
                always!(
                    type_info.is_some(),
                    "Binary expression should have type info"
                );
                always!(
                    matches!(
                        type_info.unwrap().kind,
                        TypeInfoKind::Number(NumberType::I32)
                    ),
                    "Binary expression should have type i32"
                );
            }
        }

        #[test]
        fn test_self_in_standalone_function_error() {
            let source = r#"fn method(self, x: i32) -> i32 { return x; }"#;
            let arena = build_ast(source.to_string());
            let result = TypeCheckerBuilder::build_typed_context(arena);
            always!(
                result.is_err(),
                "Expected error for self in standalone function"
            );
            let err_msg = result.err().unwrap().to_string();
            always!(
                err_msg.contains("self reference not allowed in standalone function"),
                "Expected SelfReferenceInFunction error, got: {err_msg}"
            );
        }
    }
}

/// Tests for unary operator type checking
#[cfg(test)]
mod unary_operator_tests {
    use crate::utils::build_ast;
    use inference_type_checker::TypeCheckerBuilder;

    fn try_type_check(
        source: &str,
    ) -> anyhow::Result<inference_type_checker::typed_context::TypedContext> {
        let arena = build_ast(source.to_string());
        Ok(TypeCheckerBuilder::build_typed_context(arena)?.typed_context())
    }

    mod negation_operator {
        use super::*;

        #[test]
        fn test_negate_i8_succeeds() {
            let source = r#"fn test(x: i8) -> i8 { return -(x); }"#;
            let result = try_type_check(source);
            always!(
                result.is_ok(),
                "Negation of i8 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_negate_i16_succeeds() {
            let source = r#"fn test(x: i16) -> i16 { return -(x); }"#;
            let result = try_type_check(source);
            always!(
                result.is_ok(),
                "Negation of i16 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_negate_i32_succeeds() {
            let source = r#"fn test(x: i32) -> i32 { return -(x); }"#;
            let result = try_type_check(source);
            always!(
                result.is_ok(),
                "Negation of i32 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_negate_i64_succeeds() {
            let source = r#"fn test(x: i64) -> i64 { return -(x); }"#;
            let result = try_type_check(source);
            always!(
                result.is_ok(),
                "Negation of i64 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_negate_u8_produces_error() {
            let source = r#"fn test(x: u8) -> u8 { return -(x); }"#;
            let result = try_type_check(source);
            always!(result.is_err(), "Negation of u8 should produce error");
            let err_msg = result.err().unwrap().to_string();
            always!(
                err_msg.contains("Neg") && err_msg.contains("signed integers"),
                "Error should mention Neg operator and signed integers, got: {}",
                err_msg
            );
        }

        #[test]
        fn test_negate_u16_produces_error() {
            let source = r#"fn test(x: u16) -> u16 { return -(x); }"#;
            let result = try_type_check(source);
            always!(result.is_err(), "Negation of u16 should produce error");
            let err_msg = result.err().unwrap().to_string();
            always!(
                err_msg.contains("Neg") && err_msg.contains("signed integers"),
                "Error should mention Neg operator and signed integers, got: {}",
                err_msg
            );
        }

        #[test]
        fn test_negate_u32_produces_error() {
            let source = r#"fn test(x: u32) -> u32 { return -(x); }"#;
            let result = try_type_check(source);
            always!(result.is_err(), "Negation of u32 should produce error");
            let err_msg = result.err().unwrap().to_string();
            always!(
                err_msg.contains("Neg") && err_msg.contains("signed integers"),
                "Error should mention Neg operator and signed integers, got: {}",
                err_msg
            );
        }

        #[test]
        fn test_negate_u64_produces_error() {
            let source = r#"fn test(x: u64) -> u64 { return -(x); }"#;
            let result = try_type_check(source);
            always!(result.is_err(), "Negation of u64 should produce error");
            let err_msg = result.err().unwrap().to_string();
            always!(
                err_msg.contains("Neg") && err_msg.contains("signed integers"),
                "Error should mention Neg operator and signed integers, got: {}",
                err_msg
            );
        }

        #[test]
        fn test_negate_bool_produces_error() {
            let source = r#"fn test(x: bool) -> bool { return -(x); }"#;
            let result = try_type_check(source);
            always!(result.is_err(), "Negation of bool should produce error");
            let err_msg = result.err().unwrap().to_string();
            always!(
                err_msg.contains("Neg") && err_msg.contains("signed integers"),
                "Error should mention Neg operator and signed integers, got: {}",
                err_msg
            );
        }

        #[test]
        fn test_negate_parenthesized_expression() {
            let source = r#"fn test(a: i32, b: i32) -> i32 { return -(a + b); }"#;
            let result = try_type_check(source);
            always!(
                result.is_ok(),
                "Negation of parenthesized expression should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_double_negate() {
            let source = r#"fn test(x: i32) -> i32 { return --(x); }"#;
            let result = try_type_check(source);
            always!(
                result.is_ok(),
                "Double negation should succeed, got: {:?}",
                result.err()
            );
        }
    }

    mod bitnot_operator {
        use super::*;

        #[test]
        fn test_bitnot_i8_succeeds() {
            let source = r#"fn test(x: i8) -> i8 { return ~x; }"#;
            let result = try_type_check(source);
            always!(
                result.is_ok(),
                "Bitwise NOT of i8 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_bitnot_i16_succeeds() {
            let source = r#"fn test(x: i16) -> i16 { return ~x; }"#;
            let result = try_type_check(source);
            always!(
                result.is_ok(),
                "Bitwise NOT of i16 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_bitnot_i32_succeeds() {
            let source = r#"fn test(x: i32) -> i32 { return ~x; }"#;
            let result = try_type_check(source);
            always!(
                result.is_ok(),
                "Bitwise NOT of i32 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_bitnot_i64_succeeds() {
            let source = r#"fn test(x: i64) -> i64 { return ~x; }"#;
            let result = try_type_check(source);
            always!(
                result.is_ok(),
                "Bitwise NOT of i64 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_bitnot_u8_succeeds() {
            let source = r#"fn test(x: u8) -> u8 { return ~x; }"#;
            let result = try_type_check(source);
            always!(
                result.is_ok(),
                "Bitwise NOT of u8 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_bitnot_u16_succeeds() {
            let source = r#"fn test(x: u16) -> u16 { return ~x; }"#;
            let result = try_type_check(source);
            always!(
                result.is_ok(),
                "Bitwise NOT of u16 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_bitnot_u32_succeeds() {
            let source = r#"fn test(x: u32) -> u32 { return ~x; }"#;
            let result = try_type_check(source);
            always!(
                result.is_ok(),
                "Bitwise NOT of u32 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_bitnot_u64_succeeds() {
            let source = r#"fn test(x: u64) -> u64 { return ~x; }"#;
            let result = try_type_check(source);
            always!(
                result.is_ok(),
                "Bitwise NOT of u64 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_bitnot_bool_produces_error() {
            let source = r#"fn test(x: bool) -> bool { return ~x; }"#;
            let result = try_type_check(source);
            always!(result.is_err(), "Bitwise NOT of bool should produce error");
            let err_msg = result.err().unwrap().to_string();
            always!(
                err_msg.contains("BitNot") && err_msg.contains("integers"),
                "Error should mention BitNot operator and integers, got: {}",
                err_msg
            );
        }

        #[test]
        fn test_bitnot_combined_with_negate() {
            let source = r#"fn test(x: i32) -> i32 { return ~-(x); }"#;
            let result = try_type_check(source);
            always!(
                result.is_ok(),
                "Combining BitNot and Neg should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_negate_combined_with_bitnot() {
            let source = r#"fn test(x: i32) -> i32 { return -(~x); }"#;
            let result = try_type_check(source);
            always!(
                result.is_ok(),
                "Combining Neg and BitNot should succeed, got: {:?}",
                result.err()
            );
        }
    }

    mod logical_not_operator {
        use super::*;

        #[test]
        fn test_logical_not_bool_succeeds() {
            let source = r#"fn test(x: bool) -> bool { return !x; }"#;
            let result = try_type_check(source);
            always!(
                result.is_ok(),
                "Logical NOT of bool should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_logical_not_i32_produces_error() {
            let source = r#"fn test(x: i32) -> bool { return !x; }"#;
            let result = try_type_check(source);
            always!(result.is_err(), "Logical NOT of i32 should produce error");
            let err_msg = result.err().unwrap().to_string();
            always!(
                err_msg.contains("Not") && err_msg.contains("booleans"),
                "Error should mention Not operator and booleans, got: {}",
                err_msg
            );
        }

        #[test]
        fn test_double_logical_not() {
            let source = r#"fn test(x: bool) -> bool { return !!x; }"#;
            let result = try_type_check(source);
            always!(
                result.is_ok(),
                "Double logical NOT should succeed, got: {:?}",
                result.err()
            );
        }
    }
}

/// Tests for binary division operator type checking
#[cfg(test)]
mod division_operator_tests {
    use crate::utils::build_ast;
    use inference_type_checker::TypeCheckerBuilder;

use always_assert::always;
    fn try_type_check(
        source: &str,
    ) -> anyhow::Result<inference_type_checker::typed_context::TypedContext> {
        let arena = build_ast(source.to_string());
        Ok(TypeCheckerBuilder::build_typed_context(arena)?.typed_context())
    }

    #[test]
    fn test_divide_i32_succeeds() {
        let source = r#"fn test(a: i32, b: i32) -> i32 { return a / b; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Division of i32 should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_divide_i64_succeeds() {
        let source = r#"fn test(a: i64, b: i64) -> i64 { return a / b; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Division of i64 should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_divide_u32_succeeds() {
        let source = r#"fn test(a: u32, b: u32) -> u32 { return a / b; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Division of u32 should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_divide_mixed_types_produces_error() {
        let source = r#"fn test(a: i32, b: i64) -> i32 { return a / b; }"#;
        let result = try_type_check(source);
        always!(
            result.is_err(),
            "Division of mixed types should produce error"
        );
    }

    #[test]
    fn test_divide_bool_produces_error() {
        let source = r#"fn test(a: bool, b: bool) -> bool { return a / b; }"#;
        let result = try_type_check(source);
        always!(result.is_err(), "Division of bool should produce error");
        let err_msg = result.err().unwrap().to_string();
        always!(
            err_msg.contains("arithmetic") || err_msg.contains("Div"),
            "Error should mention arithmetic operator or division, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_divide_chained() {
        let source = r#"fn test(a: i32, b: i32, c: i32) -> i32 { return a / b / c; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Chained division should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_divide_with_multiply() {
        let source = r#"fn test(a: i32, b: i32, c: i32) -> i32 { return a * b / c; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Division combined with multiplication should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_divide_with_addition_precedence() {
        let source = r#"fn test(a: i32, b: i32, c: i32) -> i32 { return a + b / c; }"#;
        let result = try_type_check(source);
        always!(
            result.is_ok(),
            "Division with addition (precedence) should succeed, got: {:?}",
            result.err()
        );
    }
}
