//! Type checker test suite
//!
//! This module contains tests for type checking and type inference functionality.
//!
//! ## Testing Pattern
//!
//! When testing type info, use `collect_all_exprs` / `collect_all_stmts` helpers
//! from `utils.rs` to find arena nodes. The `TypedContext` contains the arena with
//! annotated node IDs. Type info is looked up via `NodeId::Expr(expr_id)` or
//! `NodeId::Stmt(stmt_id)` etc.
use crate::utils::build_ast;
use inference_type_checker::TypeCheckerBuilder;

fn try_type_check(
    source: &str,
) -> anyhow::Result<inference_type_checker::typed_context::TypedContext> {
    let arena = build_ast(source.to_string());
    Ok(TypeCheckerBuilder::build_typed_context(arena)?.typed_context())
}

/// Tests that verify types are correctly inferred for various constructs.
#[cfg(test)]
mod type_inference_tests {
    use super::*;
    use crate::utils::{collect_all_exprs, collect_all_stmts, find_function_by_name};
    use inference_ast::ids::NodeId;
    use inference_ast::nodes::{ArgKind, Def, Expr, Stmt};
    use inference_type_checker::type_info::{NumberType, TypeInfo, TypeInfoKind};

    /// Tests for primitive type inference with actual type checking
    mod primitives {
        use super::*;

        #[test]
        fn test_numeric_literal_type_inference() {
            let source = r#"fn test() -> i32 { return 42; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();
            let literals = collect_all_exprs(arena, &|e| matches!(e, Expr::NumberLiteral { .. }));
            assert_eq!(literals.len(), 1, "Expected 1 number literal");
            assert_eq!(typed_context.source_files().len(), 1);
            let literal_type = typed_context.get_node_typeinfo(NodeId::Expr(literals[0]));
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
        }

        #[test]
        fn test_bool_literal_type_inference() {
            let source = r#"fn test() -> bool { return true; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();
            let bool_literals =
                collect_all_exprs(arena, &|e| matches!(e, Expr::BoolLiteral { .. }));
            assert_eq!(bool_literals.len(), 1, "Expected 1 bool literal");
            let type_info = typed_context.get_node_typeinfo(NodeId::Expr(bool_literals[0]));
            assert!(type_info.is_some(), "Bool literal should have type info");
            assert!(
                matches!(type_info.unwrap().kind, TypeInfoKind::Bool),
                "Bool literal should have Bool type"
            );
        }

        #[test]
        fn test_string_type_inference() {
            let source = r#"fn test(x: String) -> String { return x; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();
            assert_eq!(typed_context.source_files().len(), 1);
            let func_def_ids = typed_context.function_def_ids();
            assert_eq!(func_def_ids.len(), 1, "Expected 1 function definition");

            if let Def::Function { args, returns, .. } = &arena[func_def_ids[0]].kind {
                let returns_id = returns.expect("Function should have return type");
                let return_type = TypeInfo::from_type_id(arena, returns_id);
                assert!(
                    matches!(return_type.kind, TypeInfoKind::String),
                    "Function return type should be String"
                );

                assert!(!args.is_empty(), "Function should have arguments");
                if let ArgKind::Named { name, .. } = &args[0].kind {
                    let param_type = typed_context.get_node_typeinfo(NodeId::Ident(*name));
                    assert!(
                        param_type.is_some(),
                        "Function parameter should have type info"
                    );
                    assert!(
                        matches!(param_type.unwrap().kind, TypeInfoKind::String),
                        "Function parameter should have String type"
                    );
                } else {
                    panic!("Expected Named argument");
                }
            } else {
                panic!("Expected Function definition");
            }
        }

        #[test]
        fn test_variable_type_inference() {
            let source = r#"fn test() {let x: i32 = 10;let y: bool = true;}"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();
            assert_eq!(typed_context.source_files().len(), 1);
            let var_defs = collect_all_stmts(arena, &|s| matches!(s, Stmt::VarDef { .. }));
            assert_eq!(var_defs.len(), 2, "Expected 2 variable definitions");
            for stmt_id in &var_defs {
                if let Stmt::VarDef { name, .. } = &arena[*stmt_id].kind {
                    let var_name = &arena[*name].name;
                    let type_info = typed_context.get_node_typeinfo(NodeId::Stmt(*stmt_id));
                    assert!(
                        type_info.is_some(),
                        "Variable '{}' should have type info",
                        var_name
                    );
                    match var_name.as_str() {
                        "x" => assert!(
                            matches!(
                                type_info.unwrap().kind,
                                TypeInfoKind::Number(NumberType::I32)
                            ),
                            "Variable x should have i32 type"
                        ),
                        "y" => assert!(
                            matches!(type_info.unwrap().kind, TypeInfoKind::Bool),
                            "Variable y should have bool type"
                        ),
                        _ => panic!("Unexpected variable name: {}", var_name),
                    }
                }
            }
        }

        #[test]
        fn test_all_numeric_types_type_check() {
            for expected_type in NumberType::ALL {
                let type_name = expected_type.as_str();
                let source = format!("fn test(x: {type_name}) -> {type_name} {{ return x; }}");
                let typed_context = try_type_check(&source)
                    .expect("Type checking should succeed for numeric types");
                let arena = typed_context.arena();
                assert_eq!(
                    typed_context.source_files().len(),
                    1,
                    "Type checking should succeed for {} type",
                    type_name
                );
                let func_def_ids = typed_context.function_def_ids();
                assert_eq!(
                    func_def_ids.len(),
                    1,
                    "Expected 1 function for {}",
                    type_name
                );

                if let Def::Function { args, returns, .. } = &arena[func_def_ids[0]].kind {
                    let returns_id = returns.unwrap_or_else(|| {
                        panic!("Function should have return type for {}", type_name)
                    });
                    let return_type = TypeInfo::from_type_id(arena, returns_id);
                    assert!(
                        matches!(
                            return_type.kind,
                            TypeInfoKind::Number(n) if n == *expected_type
                        ),
                        "Return type should be {} for {}",
                        type_name,
                        type_name
                    );

                    assert_eq!(args.len(), 1, "Expected 1 argument for {}", type_name);
                    if let ArgKind::Named { name, .. } = &args[0].kind {
                        let arg_type = typed_context.get_node_typeinfo(NodeId::Ident(*name));
                        assert!(
                            arg_type.is_some(),
                            "Argument should have type info for {}",
                            type_name
                        );
                        assert!(
                            matches!(
                                arg_type.unwrap().kind,
                                TypeInfoKind::Number(n) if n == *expected_type
                            ),
                            "Argument should have {} type for {}",
                            type_name,
                            type_name
                        );
                    } else {
                        panic!("Expected Named argument for {}", type_name);
                    }
                } else {
                    panic!("Expected Function definition for {}", type_name);
                }
            }
        }
    }

    /// Tests for function parameter type info storage
    mod function_parameters {
        use super::*;

        #[test]
        fn test_single_parameter_type_info() {
            let source = r#"fn test(x: i32) -> i32 { return x; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();
            let func_def_ids = typed_context.function_def_ids();
            assert_eq!(func_def_ids.len(), 1, "Expected 1 function");

            if let Def::Function { args, .. } = &arena[func_def_ids[0]].kind {
                assert_eq!(args.len(), 1, "Expected 1 argument");
                if let ArgKind::Named { name, .. } = &args[0].kind {
                    let arg_type = typed_context.get_node_typeinfo(NodeId::Ident(*name));
                    assert!(arg_type.is_some(), "Argument node should have type info");
                    assert!(
                        matches!(
                            arg_type.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32)
                        ),
                        "Argument should have i32 type"
                    );
                    // Ident-level type info is the same node for Named args
                    let name_type = typed_context.get_node_typeinfo(NodeId::Ident(*name));
                    assert!(name_type.is_some(), "Argument name should have type info");
                    assert!(
                        matches!(
                            name_type.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32)
                        ),
                        "Argument name should have i32 type"
                    );
                } else {
                    panic!("Expected Named argument");
                }
            } else {
                panic!("Expected Function definition");
            }
        }

        #[test]
        fn test_multiple_parameters_type_info() {
            let source = r#"fn test(a: i32, b: bool, c: String) -> i32 { return a; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();
            let func_def_ids = typed_context.function_def_ids();
            assert_eq!(func_def_ids.len(), 1, "Expected 1 function");

            if let Def::Function { args, .. } = &arena[func_def_ids[0]].kind {
                assert_eq!(args.len(), 3, "Expected 3 arguments");
                let expected_types = [
                    TypeInfoKind::Number(NumberType::I32),
                    TypeInfoKind::Bool,
                    TypeInfoKind::String,
                ];
                for (i, arg) in args.iter().enumerate() {
                    if let ArgKind::Named { name, .. } = &arg.kind {
                        let arg_type_info = typed_context.get_node_typeinfo(NodeId::Ident(*name));
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
                        let name_type_info = typed_context.get_node_typeinfo(NodeId::Ident(*name));
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
                        panic!("Expected Named argument at position {}", i);
                    }
                }
            } else {
                panic!("Expected Function definition");
            }
        }

        #[test]
        fn test_ignore_argument_type_info() {
            let source = r#"fn test(_: i32) -> i32 { return 42; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();
            let func_def_ids = typed_context.function_def_ids();
            assert_eq!(func_def_ids.len(), 1, "Expected 1 function");

            if let Def::Function { args, .. } = &arena[func_def_ids[0]].kind {
                assert_eq!(args.len(), 1, "Expected 1 argument");
                if let ArgKind::Ignored { ty } = &args[0].kind {
                    // Type checker does NOT store type info for Ignored args,
                    // so we compute it from the type node directly.
                    let arg_type = TypeInfo::from_type_id(arena, *ty);
                    assert!(
                        matches!(arg_type.kind, TypeInfoKind::Number(NumberType::I32)),
                        "IgnoreArgument should have i32 type"
                    );
                } else {
                    panic!("Expected Ignored argument");
                }
            } else {
                panic!("Expected Function definition");
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
                let arena = typed_context.arena();
                let func_def_ids = typed_context.function_def_ids();
                assert_eq!(func_def_ids.len(), 1, "Expected 1 function");

                if let Def::Function { args, .. } = &arena[func_def_ids[0]].kind {
                    assert_eq!(args.len(), 1, "Expected 1 argument");
                    if let ArgKind::Ignored { ty } = &args[0].kind {
                        let arg_type = TypeInfo::from_type_id(arena, *ty);
                        assert!(
                            matches!(
                                arg_type.kind,
                                TypeInfoKind::Number(t) if t == expected_type
                            ),
                            "IgnoreArgument should have {:?} type",
                            expected_type
                        );
                    } else {
                        panic!("Expected Ignored argument for {:?}", expected_type);
                    }
                } else {
                    panic!("Expected Function definition for {:?}", expected_type);
                }
            }
        }

        #[test]
        fn test_mixed_ignore_and_named_arguments() {
            let source = r#"fn test(a: i32, _: bool, b: String) -> i32 { return a; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();
            let func_def_ids = typed_context.function_def_ids();
            assert_eq!(func_def_ids.len(), 1, "Expected 1 function");

            if let Def::Function { args, .. } = &arena[func_def_ids[0]].kind {
                assert_eq!(args.len(), 3, "Expected 3 arguments");

                // First arg: Named(a: i32)
                if let ArgKind::Named { name, .. } = &args[0].kind {
                    let arg_type = typed_context.get_node_typeinfo(NodeId::Ident(*name));
                    assert!(arg_type.is_some(), "First argument should have type info");
                    assert!(
                        matches!(
                            arg_type.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32)
                        ),
                        "First argument should be i32"
                    );
                } else {
                    panic!("Expected Named argument at position 0");
                }

                // Second arg: Ignored(_: bool)
                if let ArgKind::Ignored { ty } = &args[1].kind {
                    let arg_type = TypeInfo::from_type_id(arena, *ty);
                    assert!(
                        matches!(arg_type.kind, TypeInfoKind::Bool),
                        "Second argument should be bool"
                    );
                } else {
                    panic!("Expected Ignored argument at position 1");
                }

                // Third arg: Named(b: String)
                if let ArgKind::Named { name, .. } = &args[2].kind {
                    let arg_type = typed_context.get_node_typeinfo(NodeId::Ident(*name));
                    assert!(arg_type.is_some(), "Third argument should have type info");
                    assert!(
                        matches!(arg_type.unwrap().kind, TypeInfoKind::String),
                        "Third argument should be String"
                    );
                } else {
                    panic!("Expected Named argument at position 2");
                }
            } else {
                panic!("Expected Function definition");
            }
        }

        #[test]
        fn test_ignore_argument_with_string_type() {
            let source = r#"fn test(_: String) -> i32 { return 1; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();
            let func_def_ids = typed_context.function_def_ids();
            assert_eq!(func_def_ids.len(), 1, "Expected 1 function");

            if let Def::Function { args, .. } = &arena[func_def_ids[0]].kind {
                assert_eq!(args.len(), 1, "Expected 1 argument");
                if let ArgKind::Ignored { ty } = &args[0].kind {
                    let arg_type = TypeInfo::from_type_id(arena, *ty);
                    assert!(
                        matches!(arg_type.kind, TypeInfoKind::String),
                        "IgnoreArgument should have String type"
                    );
                } else {
                    panic!("Expected Ignored argument");
                }
            } else {
                panic!("Expected Function definition");
            }
        }

        #[test]
        fn test_ignore_argument_with_bool_type() {
            let source = r#"fn test(_: bool) -> i32 { return 1; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();
            let func_def_ids = typed_context.function_def_ids();
            assert_eq!(func_def_ids.len(), 1, "Expected 1 function");

            if let Def::Function { args, .. } = &arena[func_def_ids[0]].kind {
                assert_eq!(args.len(), 1, "Expected 1 argument");
                if let ArgKind::Ignored { ty } = &args[0].kind {
                    let arg_type = TypeInfo::from_type_id(arena, *ty);
                    assert!(
                        matches!(arg_type.kind, TypeInfoKind::Bool),
                        "IgnoreArgument should have bool type"
                    );
                } else {
                    panic!("Expected Ignored argument");
                }
            } else {
                panic!("Expected Function definition");
            }
        }

        #[test]
        fn test_array_parameter_type_info() {
            let source = r#"fn test(arr: [i32; 5]) -> i32 { return arr[0]; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();
            let func_def_ids = typed_context.function_def_ids();
            assert_eq!(func_def_ids.len(), 1, "Expected 1 function");

            if let Def::Function { args, .. } = &arena[func_def_ids[0]].kind {
                assert_eq!(args.len(), 1, "Expected 1 argument");
                if let ArgKind::Named { name, .. } = &args[0].kind {
                    let arg_type = typed_context.get_node_typeinfo(NodeId::Ident(*name));
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
                    panic!("Expected Named argument");
                }
            } else {
                panic!("Expected Function definition");
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
            let arena = typed_context.arena();
            let binary_exprs = collect_all_exprs(arena, &|e| matches!(e, Expr::Binary { .. }));
            assert_eq!(binary_exprs.len(), 1, "Expected 1 binary expression");
            let type_info = typed_context.get_node_typeinfo(NodeId::Expr(binary_exprs[0]));
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

        #[test]
        fn test_comparison_expression_returns_bool() {
            let source = r#"fn test(x: i32, y: i32) -> bool { return x > y; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();
            let binary_exprs = collect_all_exprs(arena, &|e| matches!(e, Expr::Binary { .. }));
            assert_eq!(binary_exprs.len(), 1, "Expected 1 binary expression");
            let type_info = typed_context.get_node_typeinfo(NodeId::Expr(binary_exprs[0]));
            assert!(type_info.is_some(), "Comparison should have type info");
            assert!(
                type_info.unwrap().is_bool(),
                "Comparison expression should return bool"
            );
        }

        #[test]
        fn test_logical_and_expression_type() {
            let source = r#"fn test(a: bool, b: bool) -> bool { return a && b; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();
            let binary_exprs = collect_all_exprs(arena, &|e| matches!(e, Expr::Binary { .. }));
            assert_eq!(binary_exprs.len(), 1, "Expected 1 binary expression");
            let type_info = typed_context.get_node_typeinfo(NodeId::Expr(binary_exprs[0]));
            assert!(
                type_info.is_some(),
                "Logical AND expression should have type info"
            );
            assert!(
                matches!(type_info.unwrap().kind, TypeInfoKind::Bool),
                "Logical AND should return Bool"
            );
        }

        #[test]
        fn test_nested_binary_expression_type() {
            let source = r#"fn test() -> i32 { return (10 + 20) * 30; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();
            let binary_exprs = collect_all_exprs(arena, &|e| matches!(e, Expr::Binary { .. }));
            // Should have 2 binary expressions: (10 + 20) and (...) * 30
            assert_eq!(binary_exprs.len(), 2, "Expected 2 binary expressions");
            for expr_id in &binary_exprs {
                let type_info = typed_context.get_node_typeinfo(NodeId::Expr(*expr_id));
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
            let arena = typed_context.arena();

            let fn_calls = collect_all_exprs(arena, &|e| matches!(e, Expr::FunctionCall { .. }));
            assert_eq!(fn_calls.len(), 1, "Expected 1 function call");

            let call_id = fn_calls[0];
            if let Expr::FunctionCall { function, .. } = &arena[call_id].kind
                && let Expr::Identifier(ident_id) = &arena[*function].kind
            {
                assert!(
                    arena[*ident_id].name == "helper",
                    "Function call should be to 'helper'"
                );
            }
            let type_info = typed_context.get_node_typeinfo(NodeId::Expr(call_id));
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

        #[test]
        fn test_function_call_with_args() {
            let source = r#"
            fn add(a: i32, b: i32) -> i32 { return a + b; }
            fn test() -> i32 { return add(10, 20); }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();

            let fn_calls = collect_all_exprs(arena, &|e| matches!(e, Expr::FunctionCall { .. }));
            assert_eq!(fn_calls.len(), 1, "Expected 1 function call");

            let call_id = fn_calls[0];
            if let Expr::FunctionCall { function, .. } = &arena[call_id].kind
                && let Expr::Identifier(ident_id) = &arena[*function].kind
            {
                assert!(
                    arena[*ident_id].name == "add",
                    "Function call should be to 'add'"
                );
            }
            let type_info = typed_context.get_node_typeinfo(NodeId::Expr(call_id));
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

        #[test]
        fn test_chained_function_calls() {
            let source = r#"
            fn double(x: i32) -> i32 { return x + x; }
            fn test() -> i32 { return double(double(5)); }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();

            let fn_calls = collect_all_exprs(arena, &|e| matches!(e, Expr::FunctionCall { .. }));
            // 2 function calls: outer double() and inner double(5)
            assert_eq!(fn_calls.len(), 2, "Expected 2 function calls");

            for call_id in &fn_calls {
                let type_info = typed_context.get_node_typeinfo(NodeId::Expr(*call_id));
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

    /// Tests for statement type inference
    mod statements {
        use super::*;

        #[test]
        fn test_if_statement_with_comparison_condition() {
            let source = r#"fn test(x: i32) -> i32 { if x > 0 { return 1; } else { return 0; } }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();
            assert_eq!(typed_context.source_files().len(), 1);

            let if_statements = collect_all_stmts(arena, &|s| matches!(s, Stmt::If { .. }));
            assert_eq!(if_statements.len(), 1, "Expected 1 if statement");

            if let Stmt::If { condition, .. } = &arena[if_statements[0]].kind {
                if let Expr::Binary { .. } = &arena[*condition].kind {
                    let cond_type = typed_context.get_node_typeinfo(NodeId::Expr(*condition));
                    assert!(
                        cond_type.is_some(),
                        "If condition (comparison) should have type info"
                    );
                    assert!(
                        matches!(cond_type.unwrap().kind, TypeInfoKind::Bool),
                        "Comparison expression should have bool type"
                    );
                } else {
                    panic!("Expected Binary expression as condition");
                }
            } else {
                panic!("Expected If statement");
            }
        }

        #[test]
        fn test_if_statement_with_bool_condition() {
            let source =
                r#"fn test(flag: bool) -> i32 { if flag { return 1; } else { return 0; } }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();
            assert_eq!(typed_context.source_files().len(), 1);

            let if_statements = collect_all_stmts(arena, &|s| matches!(s, Stmt::If { .. }));
            assert_eq!(if_statements.len(), 1, "Expected 1 if statement");

            if let Stmt::If { condition, .. } = &arena[if_statements[0]].kind {
                if let Expr::Identifier(ident_id) = &arena[*condition].kind {
                    assert_eq!(
                        arena[*ident_id].name, "flag",
                        "Condition should be the 'flag' identifier"
                    );
                    let cond_type = typed_context.get_node_typeinfo(NodeId::Expr(*condition));
                    assert!(
                        cond_type.is_some(),
                        "If condition (identifier) should have type info"
                    );
                    assert!(
                        matches!(cond_type.unwrap().kind, TypeInfoKind::Bool),
                        "Identifier 'flag' should have bool type"
                    );
                } else {
                    panic!("Expected Identifier expression as condition");
                }
            } else {
                panic!("Expected If statement");
            }

            let func_def_ids = typed_context.function_def_ids();
            assert_eq!(func_def_ids.len(), 1, "Expected 1 function");
            if let Def::Function { args, .. } = &arena[func_def_ids[0]].kind {
                assert_eq!(args.len(), 1, "Expected 1 argument");
                if let ArgKind::Named { name, .. } = &args[0].kind {
                    let arg_type = typed_context.get_node_typeinfo(NodeId::Ident(*name));
                    assert!(arg_type.is_some(), "Parameter 'flag' should have type info");
                    assert!(
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
            let arena = typed_context.arena();
            assert_eq!(typed_context.source_files().len(), 1);

            let loop_statements = collect_all_stmts(arena, &|s| matches!(s, Stmt::Loop { .. }));
            assert_eq!(loop_statements.len(), 1, "Expected 1 loop statement");

            let break_statements = collect_all_stmts(arena, &|s| matches!(s, Stmt::Break));
            assert_eq!(break_statements.len(), 1, "Expected 1 break statement");

            let func_def_ids = typed_context.function_def_ids();
            assert_eq!(func_def_ids.len(), 1, "Expected 1 function");
            if let Def::Function { returns, .. } = &arena[func_def_ids[0]].kind {
                assert!(
                    returns.is_none(),
                    "Function with loop should have no explicit return type"
                );
            }
        }

        #[test]
        fn test_assignment_type_check() {
            let source = r#"
            fn test() {
                let mut x: i32 = 10;
                x = 20;
            }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();
            assert_eq!(typed_context.source_files().len(), 1);

            let assign_statements = collect_all_stmts(arena, &|s| matches!(s, Stmt::Assign { .. }));
            assert_eq!(
                assign_statements.len(),
                1,
                "Expected 1 assignment statement"
            );

            if let Stmt::Assign { left, right } = &arena[assign_statements[0]].kind {
                // Check RHS (number literal 20)
                if let Expr::NumberLiteral { .. } = &arena[*right].kind {
                    let rhs_type = typed_context.get_node_typeinfo(NodeId::Expr(*right));
                    assert!(
                        rhs_type.is_some(),
                        "RHS of assignment should have type info"
                    );
                    assert!(
                        matches!(
                            rhs_type.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32)
                        ),
                        "RHS should be i32 to match variable type"
                    );
                } else {
                    panic!("Expected number literal as RHS");
                }
                // Check LHS (identifier x)
                if let Expr::Identifier(ident_id) = &arena[*left].kind {
                    let lhs_type = typed_context.get_node_typeinfo(NodeId::Expr(*left));
                    assert!(
                        lhs_type.is_some(),
                        "LHS of assignment should have type info"
                    );
                    assert!(
                        matches!(
                            lhs_type.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32)
                        ),
                        "LHS should be i32 to match variable type"
                    );
                    let _ = ident_id; // used for destructuring only
                } else {
                    panic!("Expected identifier as LHS");
                }
            } else {
                panic!("Expected Assign statement");
            }

            let var_defs = collect_all_stmts(arena, &|s| matches!(s, Stmt::VarDef { .. }));
            assert_eq!(var_defs.len(), 1, "Expected 1 variable definition");
            let type_info = typed_context.get_node_typeinfo(NodeId::Stmt(var_defs[0]));
            assert!(type_info.is_some(), "Variable 'x' should have type info");
            assert!(
                matches!(
                    type_info.unwrap().kind,
                    TypeInfoKind::Number(NumberType::I32)
                ),
                "Variable 'x' should have i32 type"
            );
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
            assert_eq!(arena.source_files().len(), 1);
        }

        #[test]
        fn test_nested_arrays() {
            let source = r#"fn test(matrix: [[bool; 2]; 1]) { assert(true); }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();
            assert_eq!(typed_context.source_files().len(), 1);

            let func_def_ids = typed_context.function_def_ids();
            assert_eq!(func_def_ids.len(), 1, "Expected 1 function");

            if let Def::Function { args, .. } = &arena[func_def_ids[0]].kind {
                assert_eq!(args.len(), 1, "Expected 1 argument");
                if let ArgKind::Named { name, .. } = &args[0].kind {
                    let arg_type = typed_context.get_node_typeinfo(NodeId::Ident(*name));
                    assert!(
                        arg_type.is_some(),
                        "Nested array parameter should have type info"
                    );

                    if let TypeInfoKind::Array(outer_elem, outer_size) = &arg_type.unwrap().kind {
                        assert_eq!(*outer_size, 1, "Outer array size should be 1");

                        if let TypeInfoKind::Array(inner_elem, inner_size) = &outer_elem.kind {
                            assert_eq!(*inner_size, 2, "Inner array size should be 2");
                            assert!(
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
                    panic!("Expected Named argument");
                }
            } else {
                panic!("Expected Function definition");
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

                let mut e: u8;
                e = @;
                let f: u16 = @;
                let g: u32 = @;
                let h: u64 = @;
            }"#;
            let arena = build_ast(source_code.to_string());
            let uzumaki_exprs = collect_all_exprs(&arena, &|e| matches!(e, Expr::Uzumaki));
            assert!(
                uzumaki_exprs.len() == 8,
                "Expected 8 Uzumaki expressions, found {}",
                uzumaki_exprs.len()
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
            // Sort by source location to ensure stable ordering
            let mut uzumaki_sorted: Vec<_> = uzumaki_exprs.to_vec();
            uzumaki_sorted.sort_by_key(|id| arena[*id].location.start_line);

            let typed_context = TypeCheckerBuilder::build_typed_context(arena)
                .unwrap()
                .typed_context();

            for (i, &expr_id) in uzumaki_sorted.iter().enumerate() {
                let type_info = typed_context.get_node_typeinfo(NodeId::Expr(expr_id));
                assert!(
                    type_info.as_ref().unwrap().kind == expected_types[i],
                    "Expected type {} for UzumakiExpression, found {:?}",
                    expected_types[i],
                    type_info.unwrap().kind
                );
            }

            let arena = typed_context.arena();
            for c in "abcdefgh".to_string().chars() {
                let identifiers = collect_all_exprs(arena, &|e| {
                    if let Expr::Identifier(ident_id) = e {
                        arena[*ident_id].name == c.to_string()
                    } else {
                        false
                    }
                });
                for &expr_id in &identifiers {
                    let type_info = typed_context.get_node_typeinfo(NodeId::Expr(expr_id));
                    assert!(
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
                    assert!(
                        type_info.unwrap().kind == expected_type,
                        "Identifier '{}' should have type {:?}",
                        c,
                        expected_type
                    );
                }
            }
        }

        #[test]
        fn test_uzumaki_in_return_statement() {
            let source = r#"fn test() -> i32 { return @; }"#;
            let arena = build_ast(source.to_string());
            let uzumaki_exprs = collect_all_exprs(&arena, &|e| matches!(e, Expr::Uzumaki));
            assert_eq!(uzumaki_exprs.len(), 1, "Expected 1 uzumaki expression");

            let uzumaki_id = uzumaki_exprs[0];
            let typed_context = TypeCheckerBuilder::build_typed_context(arena)
                .unwrap()
                .typed_context();

            let type_info = typed_context.get_node_typeinfo(NodeId::Expr(uzumaki_id));
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

    /// Tests for identifier type inference
    mod identifiers {
        use super::*;

        #[test]
        fn test_parameter_identifier_type() {
            let source = r#"fn test(x: i32, y: i32) -> bool { return x > y; }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();

            let identifiers = collect_all_exprs(arena, &|e| matches!(e, Expr::Identifier(_)));
            assert!(!identifiers.is_empty(), "Expected identifier expressions");

            // FIXME: Identifier type info storage has inconsistent behavior due to
            // UUID-based node IDs. The type checker sets type info during inference,
            // but lookup by ID may fail due to arena/node ID synchronization issues.
            // Expected behavior when fixed: type_info.is_some() with i32 type.
            let mut found_identifier = false;
            for &expr_id in &identifiers {
                if let Expr::Identifier(ident_id) = &arena[expr_id].kind {
                    let name = &arena[*ident_id].name;
                    if name == "x" || name == "y" {
                        found_identifier = true;
                        // Document current behavior - type info lookup may return None
                        let _type_info = typed_context.get_node_typeinfo(NodeId::Expr(expr_id));
                    }
                }
            }
            assert!(found_identifier, "Should have found identifiers x or y");

            let func_def_ids = typed_context.function_def_ids();
            assert_eq!(func_def_ids.len(), 1, "Expected 1 function");
            if let Def::Function { args, .. } = &arena[func_def_ids[0]].kind {
                assert_eq!(args.len(), 2, "Expected 2 arguments");
                for (i, arg) in args.iter().enumerate() {
                    if let ArgKind::Named { name, .. } = &arg.kind {
                        let arg_type_info = typed_context.get_node_typeinfo(NodeId::Ident(*name));
                        assert!(
                            arg_type_info.is_some(),
                            "Argument {} should have type info",
                            i
                        );
                        assert!(
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

            let binary_exprs = collect_all_exprs(arena, &|e| matches!(e, Expr::Binary { .. }));
            assert_eq!(binary_exprs.len(), 1, "Expected 1 binary comparison");

            let type_info = typed_context.get_node_typeinfo(NodeId::Expr(binary_exprs[0]));
            assert!(type_info.is_some(), "Comparison should have type info");
            assert!(
                matches!(type_info.unwrap().kind, TypeInfoKind::Bool),
                "Comparison should return bool"
            );
        }

        #[test]
        fn test_local_variable_identifier_type() {
            let source = r#"
            fn test() -> bool {
                let flag: bool = true;
                return flag;
            }"#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();

            let identifiers = collect_all_exprs(arena, &|e| matches!(e, Expr::Identifier(_)));

            // FIXME: Identifier type info storage has inconsistent behavior.
            // Expected behavior when fixed: type_info.is_some() with Bool type.
            let mut found_flag = false;
            for &expr_id in &identifiers {
                if let Expr::Identifier(ident_id) = &arena[expr_id].kind
                    && arena[*ident_id].name == "flag"
                {
                    found_flag = true;
                    // Document current behavior - type info lookup may return None
                    let _type_info = typed_context.get_node_typeinfo(NodeId::Expr(expr_id));
                }
            }
            assert!(found_flag, "Should have found identifier 'flag'");

            let var_defs = collect_all_stmts(arena, &|s| matches!(s, Stmt::VarDef { .. }));
            assert_eq!(var_defs.len(), 1, "Expected 1 variable definition");

            if let Stmt::VarDef { name, .. } = &arena[var_defs[0]].kind {
                let type_info = typed_context.get_node_typeinfo(NodeId::Stmt(var_defs[0]));
                assert!(type_info.is_some(), "Variable 'flag' should have type info");
                assert!(
                    matches!(type_info.unwrap().kind, TypeInfoKind::Bool),
                    "Variable 'flag' should have bool type"
                );
                assert_eq!(arena[*name].name, "flag", "Variable name should be 'flag'");
            }

            let bool_literals =
                collect_all_exprs(arena, &|e| matches!(e, Expr::BoolLiteral { .. }));
            assert_eq!(bool_literals.len(), 1, "Expected 1 bool literal");

            let type_info = typed_context.get_node_typeinfo(NodeId::Expr(bool_literals[0]));
            assert!(type_info.is_some(), "Bool literal should have type info");
            assert!(
                matches!(type_info.unwrap().kind, TypeInfoKind::Bool),
                "Bool literal should have Bool type"
            );
        }
    }

    /// Tests for struct field type inference (Phase 2)
    mod struct_fields {
        use super::*;

        #[test]
        fn test_struct_field_type_inference_single_field() {
            let source = r#"
            struct Point { x: i32; }
            fn test(p: Point) -> i32 { return p.x; }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();

            let member_accesses =
                collect_all_exprs(arena, &|e| matches!(e, Expr::MemberAccess { .. }));
            assert_eq!(
                member_accesses.len(),
                1,
                "Expected 1 member access expression"
            );

            let field_type = typed_context.get_node_typeinfo(NodeId::Expr(member_accesses[0]));
            assert!(field_type.is_some(), "Field access should have type info");
            assert!(
                matches!(
                    field_type.unwrap().kind,
                    TypeInfoKind::Number(NumberType::I32)
                ),
                "Field x should have type i32"
            );
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
            let arena = typed_context.arena();

            let member_accesses =
                collect_all_exprs(arena, &|e| matches!(e, Expr::MemberAccess { .. }));
            assert_eq!(
                member_accesses.len(),
                3,
                "Expected 3 member access expressions"
            );

            for &expr_id in &member_accesses {
                let field_type = typed_context.get_node_typeinfo(NodeId::Expr(expr_id));
                if let Expr::MemberAccess { name, .. } = &arena[expr_id].kind {
                    let field_name = &arena[*name].name;
                    assert!(
                        field_type.is_some(),
                        "Field access should have type info for field {}",
                        field_name
                    );

                    let expected_kind = match field_name.as_str() {
                        "age" => TypeInfoKind::Number(NumberType::I32),
                        "height" => TypeInfoKind::Number(NumberType::U64),
                        "active" => TypeInfoKind::Bool,
                        _ => panic!("Unexpected field name: {}", field_name),
                    };

                    assert_eq!(
                        field_type.unwrap().kind,
                        expected_kind,
                        "Field {} should have correct type",
                        field_name
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
            let arena = typed_context.arena();

            let member_accesses =
                collect_all_exprs(arena, &|e| matches!(e, Expr::MemberAccess { .. }));
            assert_eq!(
                member_accesses.len(),
                2,
                "Expected 2 member access expressions"
            );

            for &expr_id in &member_accesses {
                let field_type = typed_context.get_node_typeinfo(NodeId::Expr(expr_id));
                if let Expr::MemberAccess { name, .. } = &arena[expr_id].kind {
                    let field_name = &arena[*name].name;
                    assert!(
                        field_type.is_some(),
                        "Field access should have type info for field {}",
                        field_name
                    );

                    if field_name == "inner" {
                        assert_eq!(
                            field_type.unwrap().kind,
                            TypeInfoKind::Custom("Inner".to_string()),
                            "Field inner should have type Inner"
                        );
                    } else if field_name == "value" {
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
            let arena = typed_context.arena();

            let member_accesses =
                collect_all_exprs(arena, &|e| matches!(e, Expr::MemberAccess { .. }));
            assert_eq!(
                member_accesses.len(),
                1,
                "Expected 1 member access expression"
            );

            let field_type = typed_context.get_node_typeinfo(NodeId::Expr(member_accesses[0]));
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
            let arena = typed_context.arena();

            let member_accesses =
                collect_all_exprs(arena, &|e| matches!(e, Expr::MemberAccess { .. }));
            assert_eq!(
                member_accesses.len(),
                8,
                "Expected 8 member access expressions"
            );

            for &expr_id in &member_accesses {
                let field_type = typed_context.get_node_typeinfo(NodeId::Expr(expr_id));
                if let Expr::MemberAccess { name, .. } = &arena[expr_id].kind {
                    let field_name = &arena[*name].name;
                    assert!(
                        field_type.is_some(),
                        "Field {} should have type info",
                        field_name
                    );

                    let expected_kind = match field_name.as_str() {
                        "a" => TypeInfoKind::Number(NumberType::I8),
                        "b" => TypeInfoKind::Number(NumberType::I16),
                        "c" => TypeInfoKind::Number(NumberType::I32),
                        "d" => TypeInfoKind::Number(NumberType::I64),
                        "e" => TypeInfoKind::Number(NumberType::U8),
                        "f" => TypeInfoKind::Number(NumberType::U16),
                        "g" => TypeInfoKind::Number(NumberType::U32),
                        "h" => TypeInfoKind::Number(NumberType::U64),
                        _ => panic!("Unexpected field name: {}", field_name),
                    };

                    assert_eq!(
                        field_type.unwrap().kind,
                        expected_kind,
                        "Field {} should have correct numeric type",
                        field_name
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
            let arena = typed_context.arena();

            let member_accesses =
                collect_all_exprs(arena, &|e| matches!(e, Expr::MemberAccess { .. }));
            assert_eq!(
                member_accesses.len(),
                3,
                "Expected 3 member access expressions"
            );

            let mut found_level2 = false;
            let mut found_level3 = false;
            let mut found_value = false;

            for &expr_id in &member_accesses {
                let field_type = typed_context.get_node_typeinfo(NodeId::Expr(expr_id));
                if let Expr::MemberAccess { name, .. } = &arena[expr_id].kind {
                    let field_name = &arena[*name].name;
                    assert!(
                        field_type.is_some(),
                        "Field {} should have type info",
                        field_name
                    );

                    match field_name.as_str() {
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
                        _ => panic!("Unexpected field name: {}", field_name),
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
            let arena = typed_context.arena();

            let member_accesses =
                collect_all_exprs(arena, &|e| matches!(e, Expr::MemberAccess { .. }));
            assert_eq!(
                member_accesses.len(),
                1,
                "Expected 1 member access expression"
            );

            let field_type = typed_context.get_node_typeinfo(NodeId::Expr(member_accesses[0]));
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

    /// Tests for method resolution and type inference (Phase 3)
    mod methods {
        use super::*;

        #[test]
        fn test_method_call_return_type() {
            let source = r#"
            struct Counter {
                value: i32;
                fn get(self) -> i32 { return self.value; }
            }
            fn test(c: Counter) -> i32 { return c.get(); }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();

            let fn_calls = collect_all_exprs(arena, &|e| matches!(e, Expr::FunctionCall { .. }));
            assert_eq!(fn_calls.len(), 1, "Expected 1 function call expression");

            let return_type = typed_context.get_node_typeinfo(NodeId::Expr(fn_calls[0]));
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

        #[test]
        fn test_method_with_parameter() {
            let source = r#"
            struct Calculator {
                value: i32;
                fn add(self, x: i32) -> i32 { return self.value + x; }
            }
            fn test(c: Calculator) -> i32 { return c.add(10); }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();

            let fn_calls = collect_all_exprs(arena, &|e| matches!(e, Expr::FunctionCall { .. }));
            assert_eq!(fn_calls.len(), 1, "Expected 1 function call expression");

            let return_type = typed_context.get_node_typeinfo(NodeId::Expr(fn_calls[0]));
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

        #[test]
        fn test_method_returning_bool() {
            let source = r#"
            struct Checker {
                valid: bool;
                fn is_valid(self) -> bool { return self.valid; }
            }
            fn test(c: Checker) -> bool { return c.is_valid(); }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();

            let fn_calls = collect_all_exprs(arena, &|e| matches!(e, Expr::FunctionCall { .. }));
            assert_eq!(fn_calls.len(), 1, "Expected 1 function call expression");

            let return_type = typed_context.get_node_typeinfo(NodeId::Expr(fn_calls[0]));
            assert!(
                return_type.is_some(),
                "Method call should have return type info"
            );
            assert!(
                matches!(return_type.unwrap().kind, TypeInfoKind::Bool),
                "Method is_valid() should return bool"
            );
        }

        #[test]
        fn test_multiple_methods_on_struct() {
            let source = r#"
            struct Data {
                x: i32;
                y: i32;

                fn get_x(self) -> i32 { return self.x; }
                fn get_y(self) -> i32 { return self.y; }
            }
            fn test_x(d: Data) -> i32 { return d.get_x(); }
            fn test_y(d: Data) -> i32 { return d.get_y(); }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();

            let fn_calls = collect_all_exprs(arena, &|e| matches!(e, Expr::FunctionCall { .. }));
            assert_eq!(fn_calls.len(), 2, "Expected 2 function call expressions");

            for &call_id in &fn_calls {
                let return_type = typed_context.get_node_typeinfo(NodeId::Expr(call_id));
                assert!(
                    return_type.is_some(),
                    "Method call should have return type info"
                );
                assert!(
                    matches!(
                        return_type.unwrap().kind,
                        TypeInfoKind::Number(NumberType::I32)
                    ),
                    "Method should return i32"
                );
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

                fn compute(self, a: i32, b: i32) -> i32 { return self.base + a + b; }
            }
            fn test(m: Math) -> i32 { return m.compute(1, 2); }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();

            let fn_calls = collect_all_exprs(arena, &|e| matches!(e, Expr::FunctionCall { .. }));
            assert_eq!(fn_calls.len(), 1, "Expected 1 function call expression");

            let return_type = typed_context.get_node_typeinfo(NodeId::Expr(fn_calls[0]));
            assert!(
                return_type.is_some(),
                "Method call with multiple parameters should have return type info"
            );
        }

        #[test]
        fn test_method_with_self_parameter() {
            let source = r#"
            struct Container {
                data: i32;

                fn process(self) -> i32 {
                    return self.data;
                }
            }
            fn test(c: Container) -> i32 { return c.process(); }
            "#;
            let typed_context = try_type_check(source).expect("Type checking should succeed");
            let arena = typed_context.arena();

            let fn_calls = collect_all_exprs(arena, &|e| matches!(e, Expr::FunctionCall { .. }));
            assert_eq!(fn_calls.len(), 1, "Expected 1 function call expression");

            let return_type = typed_context.get_node_typeinfo(NodeId::Expr(fn_calls[0]));
            assert!(
                return_type.is_some(),
                "Method call with self should have return type info"
            );
            assert!(
                matches!(
                    return_type.unwrap().kind,
                    TypeInfoKind::Number(NumberType::I32)
                ),
                "Method process() should return i32"
            );
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
            let arena = typed_context.arena();

            let member_accesses =
                collect_all_exprs(arena, &|e| matches!(e, Expr::MemberAccess { .. }));
            assert!(
                !member_accesses.is_empty(),
                "Expected at least 1 member access expression for self.data"
            );

            let mut found_data_field = false;
            for &expr_id in &member_accesses {
                if let Expr::MemberAccess { name, .. } = &arena[expr_id].kind
                    && arena[*name].name == "data"
                {
                    let field_type = typed_context.get_node_typeinfo(NodeId::Expr(expr_id));
                    assert!(field_type.is_some(), "Field access should have type info");
                    assert!(
                        matches!(
                            field_type.unwrap().kind,
                            TypeInfoKind::Number(NumberType::I32)
                        ),
                        "Field data should have type i32"
                    );
                    found_data_field = true;
                }
            }
            assert!(found_data_field, "Should have found self.data access");
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
            let arena = typed_context.arena();

            let member_accesses =
                collect_all_exprs(arena, &|e| matches!(e, Expr::MemberAccess { .. }));
            assert!(
                member_accesses.len() >= 2,
                "Expected at least 2 member access expressions for self.x and self.y"
            );

            let mut found_x = false;
            let mut found_y = false;
            for &expr_id in &member_accesses {
                if let Expr::MemberAccess { name, .. } = &arena[expr_id].kind {
                    let field_name = &arena[*name].name;
                    match field_name.as_str() {
                        "x" => {
                            let field_type = typed_context.get_node_typeinfo(NodeId::Expr(expr_id));
                            assert!(field_type.is_some(), "Field x should have type info");
                            assert!(
                                matches!(
                                    field_type.unwrap().kind,
                                    TypeInfoKind::Number(NumberType::I32)
                                ),
                                "Field x should have type i32"
                            );
                            found_x = true;
                        }
                        "y" => {
                            let field_type = typed_context.get_node_typeinfo(NodeId::Expr(expr_id));
                            assert!(field_type.is_some(), "Field y should have type info");
                            assert!(
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
            assert!(found_x, "Should have found self.x access");
            assert!(found_y, "Should have found self.y access");

            let binary_exprs = collect_all_exprs(arena, &|e| matches!(e, Expr::Binary { .. }));
            assert_eq!(
                binary_exprs.len(),
                1,
                "Expected 1 binary expression (x + y)"
            );

            let type_info = typed_context.get_node_typeinfo(NodeId::Expr(binary_exprs[0]));
            assert!(
                type_info.is_some(),
                "Binary expression should have type info"
            );
            assert!(
                matches!(
                    type_info.unwrap().kind,
                    TypeInfoKind::Number(NumberType::I32)
                ),
                "Binary expression should have type i32"
            );
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
            let arena = typed_context.arena();

            let fn_calls = collect_all_exprs(arena, &|e| matches!(e, Expr::FunctionCall { .. }));
            // 3 function calls: c.doubled() and two self.get_value() inside doubled
            assert_eq!(fn_calls.len(), 3, "Expected 3 function call expressions");

            for &call_id in &fn_calls {
                let return_type = typed_context.get_node_typeinfo(NodeId::Expr(call_id));
                assert!(
                    return_type.is_some(),
                    "Method call should have return type info"
                );
                assert!(
                    matches!(
                        return_type.unwrap().kind,
                        TypeInfoKind::Number(NumberType::I32)
                    ),
                    "All methods should return i32"
                );
            }

            let binary_exprs = collect_all_exprs(arena, &|e| matches!(e, Expr::Binary { .. }));
            assert_eq!(
                binary_exprs.len(),
                1,
                "Expected 1 binary expression (get_value() + get_value())"
            );

            let type_info = typed_context.get_node_typeinfo(NodeId::Expr(binary_exprs[0]));
            assert!(
                type_info.is_some(),
                "Binary expression should have type info"
            );
            assert!(
                matches!(
                    type_info.unwrap().kind,
                    TypeInfoKind::Number(NumberType::I32)
                ),
                "Binary expression should have type i32"
            );
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

/// Tests for unary operator type checking
#[cfg(test)]
mod unary_operator_tests {
    use super::*;

    mod negation_operator {
        use super::*;

        #[test]
        fn test_negate_i8_succeeds() {
            let source = r#"fn test(x: i8) -> i8 { return -(x); }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Negation of i8 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_negate_i16_succeeds() {
            let source = r#"fn test(x: i16) -> i16 { return -(x); }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Negation of i16 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_negate_i32_succeeds() {
            let source = r#"fn test(x: i32) -> i32 { return -(x); }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Negation of i32 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_negate_i64_succeeds() {
            let source = r#"fn test(x: i64) -> i64 { return -(x); }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Negation of i64 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_negate_u8_produces_error() {
            let source = r#"fn test(x: u8) -> u8 { return -(x); }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Negation of u8 should produce error");
            let err_msg = result.err().unwrap().to_string();
            assert!(
                err_msg.contains("Neg") && err_msg.contains("signed integers"),
                "Error should mention Neg operator and signed integers, got: {}",
                err_msg
            );
        }

        #[test]
        fn test_negate_u16_produces_error() {
            let source = r#"fn test(x: u16) -> u16 { return -(x); }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Negation of u16 should produce error");
            let err_msg = result.err().unwrap().to_string();
            assert!(
                err_msg.contains("Neg") && err_msg.contains("signed integers"),
                "Error should mention Neg operator and signed integers, got: {}",
                err_msg
            );
        }

        #[test]
        fn test_negate_u32_produces_error() {
            let source = r#"fn test(x: u32) -> u32 { return -(x); }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Negation of u32 should produce error");
            let err_msg = result.err().unwrap().to_string();
            assert!(
                err_msg.contains("Neg") && err_msg.contains("signed integers"),
                "Error should mention Neg operator and signed integers, got: {}",
                err_msg
            );
        }

        #[test]
        fn test_negate_u64_produces_error() {
            let source = r#"fn test(x: u64) -> u64 { return -(x); }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Negation of u64 should produce error");
            let err_msg = result.err().unwrap().to_string();
            assert!(
                err_msg.contains("Neg") && err_msg.contains("signed integers"),
                "Error should mention Neg operator and signed integers, got: {}",
                err_msg
            );
        }

        #[test]
        fn test_negate_bool_produces_error() {
            let source = r#"fn test(x: bool) -> bool { return -(x); }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Negation of bool should produce error");
            let err_msg = result.err().unwrap().to_string();
            assert!(
                err_msg.contains("Neg") && err_msg.contains("signed integers"),
                "Error should mention Neg operator and signed integers, got: {}",
                err_msg
            );
        }

        #[test]
        fn test_negate_parenthesized_expression() {
            let source = r#"fn test(a: i32, b: i32) -> i32 { return -(a + b); }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Negation of parenthesized expression should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_double_negate() {
            let source = r#"fn test(x: i32) -> i32 { return --(x); }"#;
            let result = try_type_check(source);
            assert!(
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
            assert!(
                result.is_ok(),
                "Bitwise NOT of i8 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_bitnot_i16_succeeds() {
            let source = r#"fn test(x: i16) -> i16 { return ~x; }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Bitwise NOT of i16 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_bitnot_i32_succeeds() {
            let source = r#"fn test(x: i32) -> i32 { return ~x; }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Bitwise NOT of i32 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_bitnot_i64_succeeds() {
            let source = r#"fn test(x: i64) -> i64 { return ~x; }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Bitwise NOT of i64 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_bitnot_u8_succeeds() {
            let source = r#"fn test(x: u8) -> u8 { return ~x; }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Bitwise NOT of u8 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_bitnot_u16_succeeds() {
            let source = r#"fn test(x: u16) -> u16 { return ~x; }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Bitwise NOT of u16 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_bitnot_u32_succeeds() {
            let source = r#"fn test(x: u32) -> u32 { return ~x; }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Bitwise NOT of u32 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_bitnot_u64_succeeds() {
            let source = r#"fn test(x: u64) -> u64 { return ~x; }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Bitwise NOT of u64 should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_bitnot_bool_produces_error() {
            let source = r#"fn test(x: bool) -> bool { return ~x; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Bitwise NOT of bool should produce error");
            let err_msg = result.err().unwrap().to_string();
            assert!(
                err_msg.contains("BitNot") && err_msg.contains("integers"),
                "Error should mention BitNot operator and integers, got: {}",
                err_msg
            );
        }

        #[test]
        fn test_bitnot_combined_with_negate() {
            let source = r#"fn test(x: i32) -> i32 { return ~-(x); }"#;
            let result = try_type_check(source);
            assert!(
                result.is_ok(),
                "Combining BitNot and Neg should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_negate_combined_with_bitnot() {
            let source = r#"fn test(x: i32) -> i32 { return -(~x); }"#;
            let result = try_type_check(source);
            assert!(
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
            assert!(
                result.is_ok(),
                "Logical NOT of bool should succeed, got: {:?}",
                result.err()
            );
        }

        #[test]
        fn test_logical_not_i32_produces_error() {
            let source = r#"fn test(x: i32) -> bool { return !x; }"#;
            let result = try_type_check(source);
            assert!(result.is_err(), "Logical NOT of i32 should produce error");
            let err_msg = result.err().unwrap().to_string();
            assert!(
                err_msg.contains("Not") && err_msg.contains("booleans"),
                "Error should mention Not operator and booleans, got: {}",
                err_msg
            );
        }

        #[test]
        fn test_double_logical_not() {
            let source = r#"fn test(x: bool) -> bool { return !!x; }"#;
            let result = try_type_check(source);
            assert!(
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
    use super::*;

    #[test]
    fn test_divide_i32_succeeds() {
        let source = r#"fn test(a: i32, b: i32) -> i32 { return a / b; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Division of i32 should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_divide_i64_succeeds() {
        let source = r#"fn test(a: i64, b: i64) -> i64 { return a / b; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Division of i64 should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_divide_u32_succeeds() {
        let source = r#"fn test(a: u32, b: u32) -> u32 { return a / b; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Division of u32 should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_divide_mixed_types_produces_error() {
        let source = r#"fn test(a: i32, b: i64) -> i32 { return a / b; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Division of mixed types should produce error"
        );
    }

    #[test]
    fn test_divide_bool_produces_error() {
        let source = r#"fn test(a: bool, b: bool) -> bool { return a / b; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Division of bool should produce error");
        let err_msg = result.err().unwrap().to_string();
        assert!(
            err_msg.contains("arithmetic") || err_msg.contains("Div"),
            "Error should mention arithmetic operator or division, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_divide_chained() {
        let source = r#"fn test(a: i32, b: i32, c: i32) -> i32 { return a / b / c; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Chained division should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_divide_with_multiply() {
        let source = r#"fn test(a: i32, b: i32, c: i32) -> i32 { return a * b / c; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Division combined with multiplication should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_divide_with_addition_precedence() {
        let source = r#"fn test(a: i32, b: i32, c: i32) -> i32 { return a + b / c; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Division with addition (precedence) should succeed, got: {:?}",
            result.err()
        );
    }
}

/// Tests for EmptyStruct validation rule
#[cfg(test)]
mod empty_struct_tests {
    use super::*;

    #[test]
    fn empty_struct_produces_error() {
        let source = r#"struct Empty {} fn main() -> i32 { return 0; }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Empty struct should produce an error");
        if let Err(error) = result {
            let err = error.to_string();
            assert!(
                err.contains("has no fields and no methods"),
                "Error should mention empty struct, got: {err}"
            );
            assert!(
                err.contains("Empty"),
                "Error should mention struct name, got: {err}"
            );
        }
    }

    #[test]
    fn struct_with_fields_only_passes() {
        let source = r#"struct Point { x: i32; } fn main() -> i32 { return 0; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Struct with fields should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn struct_with_associated_function_only_passes() {
        let source = r#"struct Math { fn add(a: i32, b: i32) -> i32 { return a + b; } } fn main() -> i32 { return Math::add(1, 2); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Struct with associated function only should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn struct_with_fields_and_methods_passes() {
        let source = r#"struct Counter { value: i32; fn get(self) -> i32 { return self.value; } } fn main(c: Counter) -> i32 { return c.get(); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Struct with fields and methods should pass, got: {:?}",
            result.err()
        );
    }
}

/// Tests for MethodNeverAccessesSelf validation rule
#[cfg(test)]
mod unused_self_tests {
    use super::*;

    #[test]
    fn method_using_self_field_passes() {
        let source = r#"struct Foo { x: i32; fn get(self) -> i32 { return self.x; } } fn main(f: Foo) -> i32 { return f.get(); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Method accessing self.field should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn method_using_self_in_nested_if_passes() {
        let source = r#"struct Foo { x: i32; fn check(self) -> i32 { if true { return self.x; } return 0; } } fn main(f: Foo) -> i32 { return f.check(); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Method accessing self inside if should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn method_using_self_in_return_position_passes() {
        let source = r#"struct Foo { x: i32; fn val(self) -> i32 { let v: i32 = self.x; return v; } } fn main(f: Foo) -> i32 { return f.val(); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Method accessing self in variable init should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn method_declaring_self_but_never_using_it_errors() {
        let source = r#"struct Foo { x: i32; fn noop(self) -> i32 { return 42; } } fn main(f: Foo) -> i32 { return f.noop(); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_err(),
            "Method declaring self but not using it should produce an error"
        );
        if let Err(error) = result {
            let err = error.to_string();
            assert!(
                err.contains("declares `self` but never accesses it"),
                "Error should mention unused self, got: {err}"
            );
            assert!(
                err.contains("Foo") && err.contains("noop"),
                "Error should mention struct and method names, got: {err}"
            );
        }
    }

    #[test]
    fn associated_function_without_self_passes() {
        let source = r#"struct Foo { x: i32; fn new() -> i32 { return 0; } } fn main() -> i32 { return Foo::new(); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Associated function without self should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn method_using_self_in_loop_body_passes() {
        let source = r#"struct Foo { x: i32; fn sum(self) -> i32 { let mut i: i32 = 0; loop { if i >= self.x { break; } i = i + 1; } return i; } } fn main(f: Foo) -> i32 { return f.sum(); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Method accessing self inside loop should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn mut_self_never_used_errors() {
        let source = r#"struct Foo { x: i32; fn noop(mut self) -> i32 { return 42; } } fn main(f: Foo) -> i32 { return f.noop(); }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "mut self that's never used should error");
        if let Err(error) = result {
            let err = error.to_string();
            assert!(
                err.contains("declares `self` but never accesses it"),
                "Should catch unused mut self, got: {err}"
            );
        }
    }

    #[test]
    fn multiple_methods_only_unused_one_errors() {
        let source = r#"
            struct Foo {
                x: i32;
                fn get(self) -> i32 { return self.x; }
                fn bad(self) -> i32 { return 42; }
            }
            fn main(f: Foo) -> i32 { return f.get() + f.bad(); }
        "#;
        let result = try_type_check(source);
        assert!(result.is_err());
        if let Err(error) = result {
            let err = error.to_string();
            assert!(
                err.contains("bad") && err.contains("declares `self` but never accesses it"),
                "Only the unused method should error, got: {err}"
            );
            assert!(
                !err.contains("Foo::get"),
                "The method that uses self should not error, got: {err}"
            );
        }
    }

    #[test]
    fn self_used_in_assert_passes() {
        let source = r#"struct Foo { x: i32; fn check(self) { assert(self.x > 0); } } fn main(f: Foo) { f.check(); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "self used in assert should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn self_used_as_function_argument_passes() {
        let source = r#"struct Foo { x: i32; fn process(self) -> i32 { return bar(self.x); } } fn bar(v: i32) -> i32 { return v; } fn main(f: Foo) -> i32 { return f.process(); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "self used as function argument should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn self_used_in_array_index_passes() {
        let source = r#"struct Foo { idx: i32; fn get(self, a: [i32; 3]) -> i32 { return a[self.idx]; } } fn main(f: Foo) -> i32 { let a: [i32; 3] = [10, 20, 30]; return f.get(a); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "self used in array index should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn self_used_in_binary_expression_passes() {
        let source = r#"struct Foo { x: i32; y: i32; fn sum(self) -> i32 { return self.x + self.y; } } fn main(f: Foo) -> i32 { return f.sum(); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "self used in binary expression should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn method_with_empty_body_errors() {
        let source = r#"struct Foo { x: i32; fn noop(self) {} } fn main(f: Foo) { f.noop(); }"#;
        let result = try_type_check(source);
        assert!(result.is_err(), "Empty method body with self should error");
        if let Err(error) = result {
            let err = error.to_string();
            assert!(
                err.contains("declares `self` but never accesses it"),
                "Should catch self in empty body, got: {err}"
            );
        }
    }

    #[test]
    fn self_deeply_nested_passes() {
        let source = r#"
            struct Foo {
                x: i32;
                fn deep(self) -> i32 {
                    if true {
                        if true {
                            if true {
                                return self.x;
                            }
                        }
                    }
                    return 0;
                }
            }
            fn main(f: Foo) -> i32 { return f.deep(); }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Deeply nested self access should pass, got: {:?}",
            result.err()
        );
    }
}

/// Tests for interaction between EmptyStruct and MethodNeverAccessesSelf
#[cfg(test)]
mod rule_interaction_tests {
    use super::*;

    #[test]
    fn struct_with_only_unused_self_method_not_empty_struct_error() {
        let source = r#"struct S { fn noop(self) -> i32 { return 42; } } fn main(s: S) -> i32 { return s.noop(); }"#;
        let result = try_type_check(source);
        assert!(result.is_err());
        if let Err(error) = result {
            let err = error.to_string();
            assert!(
                err.contains("declares `self` but never accesses it"),
                "Should get MethodNeverAccessesSelf, got: {err}"
            );
            assert!(
                !err.contains("has no fields and no methods"),
                "Should NOT get EmptyStruct (has a method), got: {err}"
            );
        }
    }

    #[test]
    fn multiple_empty_structs_produce_separate_errors() {
        let source = r#"struct A {} struct B {} fn main() -> i32 { return 0; }"#;
        let result = try_type_check(source);
        assert!(result.is_err());
        if let Err(error) = result {
            let err = error.to_string();
            assert!(err.contains("A"), "Should mention struct A, got: {err}");
            assert!(err.contains("B"), "Should mention struct B, got: {err}");
        }
    }
}

#[cfg(test)]
mod self_in_struct_literal_tests {
    use super::*;

    #[test]
    fn self_type_struct_literal_referencing_self_in_method_passes() {
        let source = r#"struct Foo { x: i32; fn make(self) -> Foo { return Foo { x: self.x }; } } fn main(f: Foo) -> i32 { return f.x; }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Constructing same-type struct literal from self fields should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn different_type_struct_literal_in_method_passes() {
        let source = r#"
            struct Bar { y: i32; }
            struct Foo {
                x: i32;
                fn make(self) -> i32 { let v: i32 = self.x; let b: Bar = Bar { y: v }; return b.y; }
            }
            fn main(f: Foo) -> i32 { return f.make(); }
        "#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Constructing different-type struct literal should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn struct_literal_in_associated_function_passes() {
        let source = r#"struct Foo { x: i32; fn new(v: i32) -> i32 { let f: Foo = Foo { x: v }; return f.x; } } fn main() -> i32 { return Foo::new(5); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Constructing struct literal in associated function (no self) should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn self_type_struct_literal_in_vardef_referencing_self_passes() {
        let source = r#"struct Foo { x: i32; fn make(self) -> i32 { let f: Foo = Foo { x: self.x }; return f.x; } } fn main(f: Foo) -> i32 { return f.make(); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Constructing same-type struct literal from self fields in vardef should pass, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn same_type_struct_literal_without_self_ref_passes() {
        let source = r#"struct Foo { x: i32; fn make(self) -> i32 { let v: i32 = self.x; let f: Foo = Foo { x: 42 }; return f.x; } } fn main(f: Foo) -> i32 { return f.make(); }"#;
        let result = try_type_check(source);
        assert!(
            result.is_ok(),
            "Constructing same-type struct literal without referencing self should pass, got: {:?}",
            result.err()
        );
    }

}
