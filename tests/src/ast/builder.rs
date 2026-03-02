use crate::utils::{
    assert_constant_def, assert_enum_def, assert_function_signature, assert_single_binary_op,
    assert_single_unary_op, assert_struct_def, assert_variable_def, build_ast,
    collect_exprs_matching, find_function_by_name, try_build_ast,
};
use inference_ast::ids::*;
use inference_ast::nodes::{
    ArgKind, Def, Expr, OperatorKind, Stmt, TypeNode, UnaryOperatorKind,
};

// --- Definition Tests ---

#[test]
fn test_parse_simple_function() {
    let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_function_signature(&arena, "add", Some(2), true);
}

#[test]
fn test_parse_function_no_params() {
    let source = r#"fn func() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_function_signature(&arena, "func", Some(0), true);
}

#[test]
fn test_parse_function_no_return() {
    let source = r#"fn func() {}"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_function_signature(&arena, "func", Some(0), false);
}

#[test]
fn test_parse_multiple_functions() {
    let source = r#"
fn func1() -> i32 {return 1;}
fn func2() -> i32 {return 2;}
fn func3(x: i32) -> i32 {return x;}
"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);

    let definitions = &source_files[0].defs;
    assert_eq!(definitions.len(), 3);
}

#[test]
fn test_parse_constant_i32() {
    let source = r#"const X: i32 = 42;"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_constant_def(&arena, "X");
}

#[test]
fn test_parse_constant_negative() {
    let source = r#"const X: i32 = -1;"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_constant_def(&arena, "X");
}

#[test]
fn test_parse_constant_i64() {
    let source = r#"const MAX_MEM: i64 = 1000;"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_constant_def(&arena, "MAX_MEM");
}

#[test]
fn test_parse_constant_unit() {
    let source = r#"const UNIT: () = ();"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_constant_def(&arena, "UNIT");
}

#[test]
fn test_parse_constant_array() {
    let source = r#"const arr: [i32; 3] = [1, 2, 3];"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_constant_def(&arena, "arr");
}

#[test]
fn test_parse_constant_nested_array() {
    let source = r#"
const EMPTY_BOARD: [[bool; 3]; 3] =
  [[false, false, false],
   [false, false, false],
   [false, false, false]];
"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_constant_def(&arena, "EMPTY_BOARD");
}

#[test]
fn test_parse_enum_definition() {
    let source = r#"enum Arch { Wasm, Evm }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_enum_def(&arena, "Arch", Some(2));
}

#[test]
fn test_parse_struct_definition() {
    let source = r#"struct Point { x: i32; y: i32; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_struct_def(&arena, "Point", Some(2));
}

#[test]
fn test_parse_struct_with_methods() {
    let source = r#"
    struct Counter {
        value: i32;

        fn get() -> i32 { return 42; }
    }
    "#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    let struct_def = source_files[0].defs.iter().find_map(|&def_id| {
        if let Def::Struct { name, fields, methods, .. } = &arena[def_id].kind {
            Some((name, fields, methods))
        } else {
            None
        }
    });
    let (name, fields, methods) = struct_def.expect("Should find struct definition");
    assert_eq!(arena[*name].name, "Counter");
    assert_eq!(fields.len(), 1, "Expected 1 field");
    assert_eq!(methods.len(), 1, "Expected 1 method");
    assert_eq!(arena.def_name(methods[0]), "get");
}

// --- Directive Tests ---

#[test]
fn test_parse_use_directive_simple() {
    let source = r#"use inference::std;"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);

    let directives = &source_files[0].directives;
    assert_eq!(directives.len(), 1);
}

#[test]
fn test_parse_use_directive_with_imports() {
    let source = r#"use inference::std::collections::{ Array, Set };"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let source_files = arena.source_files();
    let directives = &source_files[0].directives;
    assert_eq!(directives.len(), 1, "Should find 1 use directive");
}

#[test]
fn test_parse_multiple_use_directives() {
    let source = r#"use inference::std;
use inference::std::types::Address;"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);

    let directives = &source_files[0].directives;
    assert_eq!(directives.len(), 2);
}

// --- Expression Tests ---

#[test]
fn test_parse_binary_expression_add() {
    let source = r#"fn test() -> i32 { return 1 + 2; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_single_binary_op(&arena, OperatorKind::Add);
}

#[test]
fn test_parse_binary_expression_multiply() {
    let source = r#"fn test() -> i32 { return 3 * 4; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_single_binary_op(&arena, OperatorKind::Mul);
}

#[test]
fn test_parse_binary_expression_subtract() {
    let source = r#"fn test() -> i32 { return 10 - 5; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_single_binary_op(&arena, OperatorKind::Sub);
}

#[test]
fn test_parse_binary_expression_divide() {
    let source = r#"fn test() -> i32 { return 20 / 4; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_single_binary_op(&arena, OperatorKind::Div);
}

#[test]
fn test_parse_binary_expression_divide_chained() {
    let source = r#"fn test() -> i32 { return 10 / 2 / 1; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs =
            collect_exprs_matching(&arena, *body, &|e| matches!(e, Expr::Binary { .. }));
        assert_eq!(
            exprs.len(),
            2,
            "Chained division should produce 2 binary expressions"
        );
    }
}

#[test]
fn test_parse_binary_expression_divide_with_multiply() {
    let source = r#"fn test() -> i32 { return a * b / c; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs =
            collect_exprs_matching(&arena, *body, &|e| matches!(e, Expr::Binary { .. }));
        assert_eq!(
            exprs.len(),
            2,
            "Mixed operators should produce 2 binary expressions"
        );
    }
}

#[test]
fn test_parse_binary_expression_divide_precedence() {
    let source = r#"fn test() -> i32 { return a + b / c; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs =
            collect_exprs_matching(&arena, *body, &|e| matches!(e, Expr::Binary { .. }));
        assert_eq!(
            exprs.len(),
            2,
            "Precedence expression should produce 2 binary expressions"
        );
    }
}

#[test]
fn test_parse_binary_expression_complex() {
    let source = r#"fn test() -> i32 { return a + b * c; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs =
            collect_exprs_matching(&arena, *body, &|e| matches!(e, Expr::Binary { .. }));
        assert_eq!(
            exprs.len(),
            2,
            "Complex expression should produce 2 binary expressions"
        );
    }
}

#[test]
fn test_parse_comparison_less_than() {
    let source = r#"fn test() -> bool { return a < b; }"#;
    let arena = build_ast(source.to_string());
    assert_single_binary_op(&arena, OperatorKind::Lt);
}

#[test]
fn test_parse_comparison_greater_than() {
    let source = r#"fn test() -> bool { return a > b; }"#;
    let arena = build_ast(source.to_string());
    assert_single_binary_op(&arena, OperatorKind::Gt);
}

#[test]
fn test_parse_comparison_less_equal() {
    let source = r#"fn test() -> bool { return a <= b; }"#;
    let arena = build_ast(source.to_string());
    assert_single_binary_op(&arena, OperatorKind::Le);
}

#[test]
fn test_parse_comparison_greater_equal() {
    let source = r#"fn test() -> bool { return a >= b; }"#;
    let arena = build_ast(source.to_string());
    assert_single_binary_op(&arena, OperatorKind::Ge);
}

#[test]
fn test_parse_comparison_equal() {
    let source = r#"fn test() -> bool { return a == b; }"#;
    let arena = build_ast(source.to_string());
    assert_single_binary_op(&arena, OperatorKind::Eq);
}

#[test]
fn test_parse_comparison_not_equal() {
    let source = r#"fn test() -> bool { return a != b; }"#;
    let arena = build_ast(source.to_string());
    assert_single_binary_op(&arena, OperatorKind::Ne);
}

#[test]
fn test_parse_logical_and() {
    let source = r#"fn test() -> bool { return a && b; }"#;
    let arena = build_ast(source.to_string());
    assert_single_binary_op(&arena, OperatorKind::And);
}

#[test]
fn test_parse_logical_or() {
    let source = r#"fn test() -> bool { return a || b; }"#;
    let arena = build_ast(source.to_string());
    assert_single_binary_op(&arena, OperatorKind::Or);
}

#[test]
fn test_parse_unary_not() {
    let source = r#"fn test() -> bool { return !a; }"#;
    let arena = build_ast(source.to_string());
    assert_single_unary_op(&arena, UnaryOperatorKind::Not);
}

#[test]
fn test_parse_unary_negate() {
    let source = r#"fn test() -> i32 { return -x; }"#;
    let arena = build_ast(source.to_string());
    assert_single_unary_op(&arena, UnaryOperatorKind::Neg);
}

#[test]
fn test_parse_negative_literal() {
    let source = r#"fn test() -> i32 { return -42; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::PrefixUnary { .. })
        });
        // Grammar parses -42 as a negative literal, not a prefix unary expression
        assert_eq!(
            exprs.len(),
            0,
            "Negative literal is not a prefix unary expression"
        );
    }
}

#[test]
fn test_parse_unary_negate_parenthesized() {
    let source = r#"fn test() -> i32 { return -(42); }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::PrefixUnary { .. })
        });
        assert_eq!(
            exprs.len(),
            1,
            "Should find 1 prefix unary expression"
        );

        if let Expr::PrefixUnary { op, .. } = &arena[exprs[0]].kind {
            assert_eq!(*op, UnaryOperatorKind::Neg);
        } else {
            panic!("Expected prefix unary expression");
        }
    }
}

#[test]
fn test_parse_unary_bitnot() {
    let source = r#"fn test() -> i32 { return ~flags; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::PrefixUnary { .. })
        });
        assert_eq!(exprs.len(), 1, "Should find 1 prefix unary expression");

        if let Expr::PrefixUnary { op, .. } = &arena[exprs[0]].kind {
            assert_eq!(*op, UnaryOperatorKind::BitNot);
        }
    }
}

#[test]
fn test_parse_unary_double_negate() {
    let source = r#"fn test() -> i32 { return --x; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::PrefixUnary { .. })
        });
        assert_eq!(exprs.len(), 2, "Should find 2 prefix unary expressions");
    }
}

#[test]
fn test_parse_unary_negate_bitnot() {
    let source = r#"fn test() -> i32 { return -~x; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::PrefixUnary { .. })
        });
        assert_eq!(exprs.len(), 2, "Should find 2 prefix unary expressions");
    }
}

#[test]
fn test_parse_unary_bitnot_negate() {
    let source = r#"fn test() -> i32 { return ~-x; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::PrefixUnary { .. })
        });
        assert_eq!(exprs.len(), 2, "Should find 2 prefix unary expressions");
    }
}

// --- Statement Tests ---

#[test]
fn test_parse_variable_declaration() {
    let source = r#"fn test() { let x: i32 = 5; }"#;
    let arena = build_ast(source.to_string());
    assert_variable_def(&arena, "x");
}

#[test]
fn test_parse_variable_declaration_no_init() {
    let source = r#"fn test() { let x: i32; }"#;
    let arena = build_ast(source.to_string());
    assert_variable_def(&arena, "x");
}

#[test]
fn test_parse_variable_mutable() {
    let source = r#"fn test() { let mut x: i32 = 42; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        let var_defs: Vec<_> = block
            .stmts
            .iter()
            .filter(|&&s| matches!(arena[s].kind, Stmt::VarDef { .. }))
            .collect();
        assert_eq!(var_defs.len(), 1, "Should find 1 variable definition");

        if let Stmt::VarDef {
            name, is_mut, ..
        } = &arena[*var_defs[0]].kind
        {
            assert_eq!(arena[*name].name, "x");
            assert!(*is_mut, "Variable declared with 'mut' should have is_mut == true");
        }
    }
}

#[test]
fn test_parse_variable_immutable() {
    let source = r#"fn test() { let x: i32 = 42; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        let var_defs: Vec<_> = block
            .stmts
            .iter()
            .filter(|&&s| matches!(arena[s].kind, Stmt::VarDef { .. }))
            .collect();
        assert_eq!(var_defs.len(), 1);

        if let Stmt::VarDef {
            name, is_mut, ..
        } = &arena[*var_defs[0]].kind
        {
            assert_eq!(arena[*name].name, "x");
            assert!(!*is_mut, "Variable declared without 'mut' should have is_mut == false");
        }
    }
}

#[test]
fn test_parse_variable_mutable_no_init() {
    let source = r#"fn test() { let mut y: i64; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        if let Stmt::VarDef {
            name,
            is_mut,
            value,
            ..
        } = &arena[block.stmts[0]].kind
        {
            assert_eq!(arena[*name].name, "y");
            assert!(*is_mut);
            assert!(value.is_none(), "Uninitialized variable should have no value");
        }
    }
}

#[test]
fn test_parse_assignment() {
    let source = r#"fn test() { x = 10; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        let assign_count = block
            .stmts
            .iter()
            .filter(|&&s| matches!(arena[s].kind, Stmt::Assign { .. }))
            .count();
        assert_eq!(assign_count, 1, "Should find 1 assignment statement");
    }
}

#[test]
fn test_parse_array_index_access() {
    let source = r#"fn test() -> i32 { return arr[0]; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::ArrayIndexAccess { .. })
        });
        assert_eq!(exprs.len(), 1, "Should find 1 array index access");
    }
}

#[test]
fn test_parse_array_index_expression() {
    let source = r#"fn test() -> i32 { return arr[i + 1]; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::ArrayIndexAccess { .. })
        });
        assert_eq!(exprs.len(), 1, "Should find 1 array index access");
    }
}

#[test]
fn test_parse_function_call_no_args() {
    let source = r#"fn test() { foo(); }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::FunctionCall { .. })
        });
        assert_eq!(exprs.len(), 1, "Should find 1 function call");
    }
}

#[test]
fn test_parse_function_call_one_arg() {
    let source = r#"fn test() { foo(42); }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::FunctionCall { .. })
        });
        assert_eq!(exprs.len(), 1, "Should find 1 function call");
    }
}

#[test]
fn test_parse_function_call_multiple_args() {
    let source = r#"fn test() { add(1, 2); }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::FunctionCall { .. })
        });
        assert_eq!(exprs.len(), 1, "Should find 1 function call");
    }
}

#[test]
fn test_parse_if_statement() {
    let source = r#"fn test() { if (x > 0) { return x; } }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        let if_count = block
            .stmts
            .iter()
            .filter(|&&s| matches!(arena[s].kind, Stmt::If { .. }))
            .count();
        assert_eq!(if_count, 1, "Should find 1 if statement");
    }
}

#[test]
fn test_parse_if_else_statement() {
    let source = r#"fn test() -> i32 { if (x > 0) { return x; } else { return 0; } }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        let if_stmt = block.stmts.iter().find(|&&s| matches!(arena[s].kind, Stmt::If { .. }));
        assert!(if_stmt.is_some(), "Should find if statement");

        if let Stmt::If { else_block, .. } = &arena[*if_stmt.unwrap()].kind {
            assert!(else_block.is_some(), "If statement should have else arm");
        }
    }
}

#[test]
fn test_parse_loop_statement() {
    let source = r#"fn test() { loop { break; } }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        let loop_count = block
            .stmts
            .iter()
            .filter(|&&s| matches!(arena[s].kind, Stmt::Loop { .. }))
            .count();
        assert_eq!(loop_count, 1, "Should find 1 loop statement");
    }
}

#[test]
fn test_parse_for_loop() {
    let source = r#"fn test() { let mut i: i32 = 0; loop i < 10 { foo(i); i = i + 1; } }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        let loop_count = block
            .stmts
            .iter()
            .filter(|&&s| matches!(arena[s].kind, Stmt::Loop { .. }))
            .count();
        assert_eq!(loop_count, 1, "Should find 1 loop statement");
    }
}

#[test]
fn test_parse_break_statement() {
    let source = r#"fn test() { loop { break; } }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|_| false);
        // Check for break in loop body
        let block = &arena[*body];
        if let Stmt::Loop { body: loop_body, .. } = &arena[block.stmts[0]].kind {
            let loop_block = &arena[*loop_body];
            let break_count = loop_block
                .stmts
                .iter()
                .filter(|&&s| matches!(arena[s].kind, Stmt::Break))
                .count();
            assert_eq!(break_count, 1, "Should find 1 break statement");
        }
        let _ = exprs; // suppress unused warning
    }
}

#[test]
fn test_parse_assert_statement() {
    let source = r#"fn test() { assert x > 0; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        let assert_count = block
            .stmts
            .iter()
            .filter(|&&s| matches!(arena[s].kind, Stmt::Assert { .. }))
            .count();
        assert_eq!(assert_count, 1, "Should find 1 assert statement");
    }
}

#[test]
fn test_parse_assert_with_complex_expr() {
    let source = r#"fn test() { assert a < b && b < c; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        let assert_count = block
            .stmts
            .iter()
            .filter(|&&s| matches!(arena[s].kind, Stmt::Assert { .. }))
            .count();
        assert_eq!(assert_count, 1, "Should find 1 assert statement");
    }
}

#[test]
fn test_parse_parenthesized_expression() {
    let source = r#"fn test() -> i32 { return (a + b) * c; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::Parenthesized { .. })
        });
        assert!(!exprs.is_empty(), "Should find parenthesized expression");
    }
}

#[test]
fn test_parse_bool_literal_true() {
    let source = r#"fn test() -> bool { return true; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::BoolLiteral { .. })
        });
        assert_eq!(exprs.len(), 1, "Should find 1 bool literal");

        if let Expr::BoolLiteral { value } = &arena[exprs[0]].kind {
            assert!(*value, "Bool literal should be true");
        }
    }
}

#[test]
fn test_parse_bool_literal_false() {
    let source = r#"fn test() -> bool { return false; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::BoolLiteral { .. })
        });
        assert_eq!(exprs.len(), 1, "Should find 1 bool literal");

        if let Expr::BoolLiteral { value } = &arena[exprs[0]].kind {
            assert!(!*value, "Bool literal should be false");
        }
    }
}

#[test]
fn test_parse_string_literal() {
    let source = r#"fn test() -> str { return "hello"; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::StringLiteral { .. })
        });
        assert_eq!(exprs.len(), 1, "Should find 1 string literal");

        if let Expr::StringLiteral { value } = &arena[exprs[0]].kind {
            assert!(
                value.contains("hello"),
                "String literal should contain 'hello'"
            );
        }
    }
}

#[test]
fn test_parse_array_literal_empty() {
    let source = r#"fn test() -> [i32; 0] { return []; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::ArrayLiteral { .. })
        });
        assert_eq!(exprs.len(), 1, "Should find 1 array literal");

        if let Expr::ArrayLiteral { elements } = &arena[exprs[0]].kind {
            assert!(elements.is_empty(), "Array literal should be empty");
        }
    }
}

#[test]
fn test_parse_array_literal_values() {
    let source = r#"fn test() -> [i32; 3] { return [1, 2, 3]; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::ArrayLiteral { .. })
        });
        assert_eq!(exprs.len(), 1, "Should find 1 array literal");

        if let Expr::ArrayLiteral { elements } = &arena[exprs[0]].kind {
            assert_eq!(elements.len(), 3, "Array literal should have 3 elements");
        }
    }
}

#[test]
fn test_parse_member_access() {
    let source = r#"fn test() -> i32 { return obj.field; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::MemberAccess { .. })
        });
        assert_eq!(exprs.len(), 1, "Should find 1 member access");

        if let Expr::MemberAccess { name, .. } = &arena[exprs[0]].kind {
            assert_eq!(arena[*name].name, "field");
        }
    }
}

#[test]
fn test_parse_chained_member_access() {
    let source = r#"fn test() -> i32 { return obj.field.subfield; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::MemberAccess { .. })
        });
        assert!(!exprs.is_empty(), "Should find at least 1 member access");
    }
}

#[test]
fn test_parse_struct_expression() {
    let source = r#"fn test() -> Point { return Point { x: 1, y: 2 }; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::StructLiteral { .. })
        });
        assert_eq!(exprs.len(), 1, "Should find 1 struct expression");

        if let Expr::StructLiteral { name, .. } = &arena[exprs[0]].kind {
            assert_eq!(arena[*name].name, "Point");
        }
    }
}

#[test]
fn test_parse_external_function() {
    let source = r#"external fn sorting_function(Address, Address) -> Address;"#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    let ext_func = source_files[0].defs.iter().find_map(|&def_id| {
        if let Def::ExternFunction { name, .. } = &arena[def_id].kind {
            Some(name)
        } else {
            None
        }
    });
    let name_id = ext_func.expect("Should find external function");
    assert_eq!(arena[*name_id].name, "sorting_function");
}

#[test]
fn test_parse_type_alias() {
    let source = r#"type sf = sorting_function;"#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    let type_alias = source_files[0].defs.iter().find_map(|&def_id| {
        if let Def::TypeAlias { name, .. } = &arena[def_id].kind {
            Some(name)
        } else {
            None
        }
    });
    let name_id = type_alias.expect("Should find type definition");
    assert_eq!(arena[*name_id].name, "sf");
}

#[test]
fn test_parse_generic_type() {
    let source = r#"fn test() -> Array i32' {}"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_function_signature(&arena, "test", Some(0), true);
}

#[test]
fn test_parse_function_type_param() {
    let source = r#"fn test(func: sf) {}"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_function_signature(&arena, "test", Some(1), false);
}

#[test]
fn test_parse_empty_block() {
    let source = r#"fn test() {}"#;
    let arena = build_ast(source.to_string());
    assert_function_signature(&arena, "test", Some(0), false);

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        assert!(
            block.stmts.is_empty(),
            "Empty function should have no statements"
        );
    }
}

#[test]
fn test_parse_block_multiple_statements() {
    let source = r#"fn test() { let x: i32 = 1; let y: i32 = 2; return x + y; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        assert_eq!(block.stmts.len(), 3, "Function should have 3 statements");
    }
}

#[test]
fn test_parse_nested_blocks() {
    let source = r#"fn test() { { let x: i32 = 1; } }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        assert!(!block.stmts.is_empty(), "Should have at least 1 statement");
        assert!(
            matches!(arena[block.stmts[0]].kind, Stmt::Block(_)),
            "First statement should be a nested block"
        );
    }
    assert_variable_def(&arena, "x");
}

#[test]
fn test_parse_power_operator() {
    let source = r#"fn test() -> i32 { return 2 ** 16; }"#;
    let arena = build_ast(source.to_string());
    assert_single_binary_op(&arena, OperatorKind::Pow);
}

#[test]
fn test_parse_modulo_operator() {
    let source = r#"fn test() -> i32 { return a % 4; }"#;
    let arena = build_ast(source.to_string());
    assert_single_binary_op(&arena, OperatorKind::Mod);
}

#[test]
fn test_parse_comments() {
    let source = r#"// This is a comment
fn test() -> i32 {
    // Another comment
    return 42;
}"#;
    let arena = build_ast(source.to_string());
    assert_function_signature(&arena, "test", Some(0), true);
}

#[test]
fn test_parse_multiline_comments() {
    let source = r#"// This is a
//   multiline comment
fn test() -> i32 {
    return 42;
}"#;
    let arena = build_ast(source.to_string());
    assert_function_signature(&arena, "test", Some(0), true);
}

#[test]
fn test_parse_function_with_bool_return() {
    let source = r#"fn is_positive(x: i32) -> bool { return x > 0; }"#;
    let arena = build_ast(source.to_string());
    assert_function_signature(&arena, "is_positive", Some(1), true);
}

#[test]
fn test_parse_custom_struct_type() {
    let source = r#"struct Point { x: i32; y: i32; }
fn test(p: Point) -> Point { return p; }"#;
    let arena = build_ast(source.to_string());
    assert_struct_def(&arena, "Point", Some(2));
    assert_function_signature(&arena, "test", Some(1), true);
}

#[test]
fn test_parse_constant_declarations() {
    let source = r#"
const FLAG: bool = true;
const NUM: i32 = 42;
"#;
    let arena = build_ast(source.to_string());
    assert_constant_def(&arena, "FLAG");
    assert_constant_def(&arena, "NUM");
}

#[test]
fn test_parse_unit_return_type() {
    let source = r#"fn test() { assert(true); }"#;
    let arena = build_ast(source.to_string());
    assert_function_signature(&arena, "test", Some(0), false);
}

#[test]
fn test_parse_function_multiple_params() {
    let source = r#"fn test(a: i32, b: i32, c: i32, d: i32) -> i32 { return a + b + c + d; }"#;
    let arena = build_ast(source.to_string());
    assert_function_signature(&arena, "test", Some(4), true);
}

// --- Error Recovery Coverage Tests ---

#[test]
fn test_error_definition_recovery() {
    cov_mark::check!(ast_builder_error_definition_recovery);
    let _ = try_build_ast("$$$".to_string());
}

#[test]
fn test_error_statement_recovery() {
    cov_mark::check!(ast_builder_error_statement_recovery);
    let _ = try_build_ast("fn f() { $$$; }".to_string());
}
