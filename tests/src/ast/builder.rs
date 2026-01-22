use crate::utils::{
    assert_constant_def, assert_enum_def, assert_function_signature, assert_single_binary_op,
    assert_single_unary_op, assert_struct_def, assert_variable_def, build_ast, try_build_ast,
};
use inference_ast::nodes::{
    AstNode, Definition, Expression, Literal, OperatorKind, Statement, UnaryOperatorKind,
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

    let definitions = &source_files[0].definitions;
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
    let structs =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Struct(_))));
    assert_eq!(structs.len(), 1, "Expected 1 struct definition");

    if let AstNode::Definition(Definition::Struct(struct_def)) = &structs[0] {
        assert_eq!(struct_def.name.name, "Counter");
        assert_eq!(struct_def.fields.len(), 1, "Expected 1 field");
        assert_eq!(struct_def.methods.len(), 1, "Expected 1 method");
        assert_eq!(struct_def.methods[0].name.name, "get");
    }
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
use always_assert::always;
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
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let binary_exprs =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Binary(_))));
    assert_eq!(
        binary_exprs.len(),
        2,
        "Chained division should produce 2 binary expressions"
    );
}

#[test]
fn test_parse_binary_expression_divide_with_multiply() {
    let source = r#"fn test() -> i32 { return a * b / c; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let binary_exprs =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Binary(_))));
    assert_eq!(
        binary_exprs.len(),
        2,
        "Mixed operators should produce 2 binary expressions"
    );
}

#[test]
fn test_parse_binary_expression_divide_precedence() {
    let source = r#"fn test() -> i32 { return a + b / c; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let binary_exprs =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Binary(_))));
    assert_eq!(
        binary_exprs.len(),
        2,
        "Precedence expression should produce 2 binary expressions"
    );
}

#[test]
fn test_parse_binary_expression_complex() {
    let source = r#"fn test() -> i32 { return a + b * c; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let binary_exprs =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Binary(_))));
    assert_eq!(
        binary_exprs.len(),
        2,
        "Complex expression should produce 2 binary expressions"
    );
}

#[test]
fn test_parse_comparison_less_than() {
    let source = r#"fn test() -> bool { return a < b; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_single_binary_op(&arena, OperatorKind::Lt);
}

#[test]
fn test_parse_comparison_greater_than() {
    let source = r#"fn test() -> bool { return a > b; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_single_binary_op(&arena, OperatorKind::Gt);
}

#[test]
fn test_parse_comparison_less_equal() {
    let source = r#"fn test() -> bool { return a <= b; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_single_binary_op(&arena, OperatorKind::Le);
}

#[test]
fn test_parse_comparison_greater_equal() {
    let source = r#"fn test() -> bool { return a >= b; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_single_binary_op(&arena, OperatorKind::Ge);
}

#[test]
fn test_parse_comparison_equal() {
    let source = r#"fn test() -> bool { return a == b; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_single_binary_op(&arena, OperatorKind::Eq);
}

#[test]
fn test_parse_comparison_not_equal() {
    let source = r#"fn test() -> bool { return a != b; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_single_binary_op(&arena, OperatorKind::Ne);
}

#[test]
fn test_parse_logical_and() {
    let source = r#"fn test() -> bool { return a && b; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_single_binary_op(&arena, OperatorKind::And);
}

#[test]
fn test_parse_logical_or() {
    let source = r#"fn test() -> bool { return a || b; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_single_binary_op(&arena, OperatorKind::Or);
}

#[test]
fn test_parse_unary_not() {
    let source = r#"fn test() -> bool { return !a; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_single_unary_op(&arena, UnaryOperatorKind::Not);
}

#[test]
fn test_parse_unary_negate() {
    let source = r#"fn test() -> i32 { return -x; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_single_unary_op(&arena, UnaryOperatorKind::Neg);
}

#[test]
fn test_parse_negative_literal() {
    // Note: tree-sitter-inference parses `-42` as a negative literal, not as unary minus
    // applied to `42`. This is grammar-level behavior - the minus is part of the literal.
    let source = r#"fn test() -> i32 { return -42; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);

    let prefix_exprs =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::PrefixUnary(_))));
    // Grammar parses -42 as a negative literal, not a prefix unary expression
    assert_eq!(
        prefix_exprs.len(),
        0,
        "Negative literal is not a prefix unary expression"
    );
}

#[test]
fn test_parse_unary_negate_parenthesized() {
    let source = r#"fn test() -> i32 { return -(42); }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);

    let prefix_exprs =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::PrefixUnary(_))));
    assert_eq!(
        prefix_exprs.len(),
        1,
        "Should find 1 prefix unary expression"
    );

    if let AstNode::Expression(Expression::PrefixUnary(unary_expr)) = &prefix_exprs[0] {
        assert_eq!(unary_expr.operator, UnaryOperatorKind::Neg);
    } else {
        panic!("Expected prefix unary expression");
    }
}

#[test]
fn test_parse_unary_bitnot() {
    let source = r#"fn test() -> i32 { return ~flags; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);

    let prefix_exprs =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::PrefixUnary(_))));
    assert_eq!(
        prefix_exprs.len(),
        1,
        "Should find 1 prefix unary expression"
    );

    if let AstNode::Expression(Expression::PrefixUnary(unary_expr)) = &prefix_exprs[0] {
        assert_eq!(unary_expr.operator, UnaryOperatorKind::BitNot);
    } else {
        panic!("Expected prefix unary expression");
    }
}

#[test]
fn test_parse_unary_double_negate() {
    let source = r#"fn test() -> i32 { return --x; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);

    let prefix_exprs =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::PrefixUnary(_))));
    assert_eq!(
        prefix_exprs.len(),
        2,
        "Should find 2 prefix unary expressions"
    );
}

#[test]
fn test_parse_unary_negate_bitnot() {
    let source = r#"fn test() -> i32 { return -~x; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);

    let prefix_exprs =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::PrefixUnary(_))));
    assert_eq!(
        prefix_exprs.len(),
        2,
        "Should find 2 prefix unary expressions"
    );
}

#[test]
fn test_parse_unary_bitnot_negate() {
    let source = r#"fn test() -> i32 { return ~-x; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);

    let prefix_exprs =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::PrefixUnary(_))));
    assert_eq!(
        prefix_exprs.len(),
        2,
        "Should find 2 prefix unary expressions"
    );
}

// --- Statement Tests ---

#[test]
fn test_parse_variable_declaration() {
    let source = r#"fn test() { let x: i32 = 5; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_variable_def(&arena, "x");
}

#[test]
fn test_parse_variable_declaration_no_init() {
    let source = r#"fn test() { let x: i32; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_variable_def(&arena, "x");
}

#[test]
fn test_parse_assignment() {
    let source = r#"fn test() { x = 10; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let assigns =
        arena.filter_nodes(|node| matches!(node, AstNode::Statement(Statement::Assign(_))));
    assert_eq!(assigns.len(), 1, "Should find 1 assignment statement");
}

#[test]
fn test_parse_array_index_access() {
    let source = r#"fn test() -> i32 { return arr[0]; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let accesses = arena
        .filter_nodes(|node| matches!(node, AstNode::Expression(Expression::ArrayIndexAccess(_))));
    assert_eq!(accesses.len(), 1, "Should find 1 array index access");
}

#[test]
fn test_parse_array_index_expression() {
    let source = r#"fn test() -> i32 { return arr[i + 1]; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let accesses = arena
        .filter_nodes(|node| matches!(node, AstNode::Expression(Expression::ArrayIndexAccess(_))));
    assert_eq!(accesses.len(), 1, "Should find 1 array index access");
}

#[test]
fn test_parse_function_call_no_args() {
    let source = r#"fn test() { foo(); }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let calls =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::FunctionCall(_))));
    assert_eq!(calls.len(), 1, "Should find 1 function call");
}

#[test]
fn test_parse_function_call_one_arg() {
    let source = r#"fn test() { foo(42); }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let calls =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::FunctionCall(_))));
    assert_eq!(calls.len(), 1, "Should find 1 function call");
}

#[test]
fn test_parse_function_call_multiple_args() {
    let source = r#"fn test() { add(1, 2); }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let calls =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::FunctionCall(_))));
    assert_eq!(calls.len(), 1, "Should find 1 function call");
}

#[test]
fn test_parse_if_statement() {
    let source = r#"fn test() { if (x > 0) { return x; } }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let ifs = arena.filter_nodes(|node| matches!(node, AstNode::Statement(Statement::If(_))));
    assert_eq!(ifs.len(), 1, "Should find 1 if statement");
}

#[test]
fn test_parse_if_else_statement() {
    let source = r#"fn test() -> i32 { if (x > 0) { return x; } else { return 0; } }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let ifs = arena.filter_nodes(|node| matches!(node, AstNode::Statement(Statement::If(_))));
    assert_eq!(ifs.len(), 1, "Should find 1 if statement");

    if let AstNode::Statement(Statement::If(if_stmt)) = &ifs[0] {
        always!(
            if_stmt.else_arm.is_some(),
            "If statement should have else arm"
        );
    }
}

#[test]
fn test_parse_loop_statement() {
    let source = r#"fn test() { loop { break; } }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let loops = arena.filter_nodes(|node| matches!(node, AstNode::Statement(Statement::Loop(_))));
    assert_eq!(loops.len(), 1, "Should find 1 loop statement");
}

#[test]
fn test_parse_for_loop() {
    let source = r#"fn test() { let mut i: i32 = 0; loop i < 10 { foo(i); i = i + 1; } }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let loops = arena.filter_nodes(|node| matches!(node, AstNode::Statement(Statement::Loop(_))));
    assert_eq!(loops.len(), 1, "Should find 1 loop statement");
}

#[test]
fn test_parse_break_statement() {
    let source = r#"fn test() { loop { break; } }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let breaks = arena.filter_nodes(|node| matches!(node, AstNode::Statement(Statement::Break(_))));
    assert_eq!(breaks.len(), 1, "Should find 1 break statement");
}

#[test]
fn test_parse_assert_statement() {
    let source = r#"fn test() { assert x > 0; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let asserts =
        arena.filter_nodes(|node| matches!(node, AstNode::Statement(Statement::Assert(_))));
    assert_eq!(asserts.len(), 1, "Should find 1 assert statement");
}

#[test]
fn test_parse_assert_with_complex_expr() {
    let source = r#"fn test() { assert a < b && b < c; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let asserts =
        arena.filter_nodes(|node| matches!(node, AstNode::Statement(Statement::Assert(_))));
    assert_eq!(asserts.len(), 1, "Should find 1 assert statement");
}

#[test]
fn test_parse_parenthesized_expression() {
    let source = r#"fn test() -> i32 { return (a + b) * c; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let parens = arena
        .filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Parenthesized(_))));
    always!(!parens.is_empty(), "Should find parenthesized expression");
}

#[test]
fn test_parse_bool_literal_true() {
    let source = r#"fn test() -> bool { return true; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let bool_literals = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::Expression(Expression::Literal(Literal::Bool(_)))
        )
    });
    assert_eq!(bool_literals.len(), 1, "Should find 1 bool literal");

    if let AstNode::Expression(Expression::Literal(Literal::Bool(lit))) = &bool_literals[0] {
        always!(lit.value, "Bool literal should be true");
    } else {
        panic!("Expected bool literal");
    }
}

#[test]
fn test_parse_bool_literal_false() {
    let source = r#"fn test() -> bool { return false; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let bool_literals = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::Expression(Expression::Literal(Literal::Bool(_)))
        )
    });
    assert_eq!(bool_literals.len(), 1, "Should find 1 bool literal");

    if let AstNode::Expression(Expression::Literal(Literal::Bool(lit))) = &bool_literals[0] {
        always!(!lit.value, "Bool literal should be false");
    } else {
        panic!("Expected bool literal");
    }
}

#[test]
fn test_parse_string_literal() {
    let source = r#"fn test() -> str { return "hello"; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let string_literals = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::Expression(Expression::Literal(Literal::String(_)))
        )
    });
    assert_eq!(string_literals.len(), 1, "Should find 1 string literal");

    if let AstNode::Expression(Expression::Literal(Literal::String(lit))) = &string_literals[0] {
        always!(
            lit.value.contains("hello"),
            "String literal should contain 'hello'"
        );
    } else {
        panic!("Expected string literal");
    }
}

#[test]
fn test_parse_array_literal_empty() {
    let source = r#"fn test() -> [i32; 0] { return []; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let array_literals = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::Expression(Expression::Literal(Literal::Array(_)))
        )
    });
    assert_eq!(array_literals.len(), 1, "Should find 1 array literal");

    if let AstNode::Expression(Expression::Literal(Literal::Array(lit))) = &array_literals[0] {
        let is_empty = lit.elements.as_ref().is_none_or(Vec::is_empty);
        always!(is_empty, "Array literal should be empty");
    } else {
        panic!("Expected array literal");
    }
}

#[test]
fn test_parse_array_literal_values() {
    let source = r#"fn test() -> [i32; 3] { return [1, 2, 3]; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let array_literals = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::Expression(Expression::Literal(Literal::Array(_)))
        )
    });
    assert_eq!(array_literals.len(), 1, "Should find 1 array literal");

    if let AstNode::Expression(Expression::Literal(Literal::Array(lit))) = &array_literals[0] {
        let count = lit.elements.as_ref().map_or(0, |v| v.len());
        assert_eq!(count, 3, "Array literal should have 3 elements");
    } else {
        panic!("Expected array literal");
    }
}

#[test]
fn test_parse_member_access() {
    let source = r#"fn test() -> i32 { return obj.field; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let member_accesses =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::MemberAccess(_))));
    assert_eq!(member_accesses.len(), 1, "Should find 1 member access");

    if let AstNode::Expression(Expression::MemberAccess(ma)) = &member_accesses[0] {
        assert_eq!(ma.name.name, "field", "Member access should access 'field'");
    } else {
        panic!("Expected member access expression");
    }
}

#[test]
fn test_parse_chained_member_access() {
    let source = r#"fn test() -> i32 { return obj.field.subfield; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let member_accesses =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::MemberAccess(_))));
    always!(
        !member_accesses.is_empty(),
        "Should find at least 1 member access"
    );

    if let AstNode::Expression(Expression::MemberAccess(ma)) = &member_accesses[0] {
        assert_eq!(
            ma.name.name, "subfield",
            "Outermost member access should be 'subfield'"
        );
    } else {
        panic!("Expected member access expression");
    }
}

#[test]
fn test_parse_struct_expression() {
    let source = r#"fn test() -> Point { return Point { x: 1, y: 2 }; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let struct_exprs =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Struct(_))));
    assert_eq!(struct_exprs.len(), 1, "Should find 1 struct expression");

    if let AstNode::Expression(Expression::Struct(se)) = &struct_exprs[0] {
        assert_eq!(se.name.name, "Point", "Struct expression should be 'Point'");
    } else {
        panic!("Expected struct expression");
    }
}

#[test]
fn test_parse_external_function() {
    let source = r#"external fn sorting_function(Address, Address) -> Address;"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let ext_funcs = arena
        .filter_nodes(|node| matches!(node, AstNode::Definition(Definition::ExternalFunction(_))));
    assert_eq!(ext_funcs.len(), 1, "Should find 1 external function");

    if let AstNode::Definition(Definition::ExternalFunction(ef)) = &ext_funcs[0] {
        assert_eq!(
            ef.name.name, "sorting_function",
            "External function should be 'sorting_function'"
        );
    } else {
        panic!("Expected external function definition");
    }
}

#[test]
fn test_parse_type_alias() {
    let source = r#"type sf = sorting_function;"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let type_defs =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Type(_))));
    assert_eq!(type_defs.len(), 1, "Should find 1 type definition");

    if let AstNode::Definition(Definition::Type(td)) = &type_defs[0] {
        assert_eq!(td.name.name, "sf", "Type alias should be 'sf'");
    } else {
        panic!("Expected type definition");
    }
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
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_function_signature(&arena, "test", Some(0), false);

    let functions = arena.functions();
    let func = &functions[0];
    always!(
        func.body.statements().is_empty(),
        "Empty function should have no statements"
    );
}

#[test]
fn test_parse_block_multiple_statements() {
    let source = r#"fn test() { let x: i32 = 1; let y: i32 = 2; return x + y; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let functions = arena.functions();
    let func = &functions[0];
    assert_eq!(
        func.body.statements().len(),
        3,
        "Function should have 3 statements"
    );
}

#[test]
fn test_parse_nested_blocks() {
    let source = r#"fn test() { { let x: i32 = 1; } }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let blocks = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::Statement(Statement::Block(inference_ast::nodes::BlockType::Block(_)))
        )
    });
    always!(
        !blocks.is_empty(),
        "Should find at least 1 nested block statement"
    );
    assert_variable_def(&arena, "x");
}

#[test]
fn test_parse_power_operator() {
    let source = r#"fn test() -> i32 { return 2 ** 16; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_single_binary_op(&arena, OperatorKind::Pow);
}

#[test]
fn test_parse_modulo_operator() {
    let source = r#"fn test() -> i32 { return a % 4; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
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
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
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
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_function_signature(&arena, "test", Some(0), true);
}

#[test]
fn test_parse_function_with_bool_return() {
    let source = r#"fn is_positive(x: i32) -> bool { return x > 0; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_function_signature(&arena, "is_positive", Some(1), true);
}

#[test]
fn test_parse_custom_struct_type() {
    let source = r#"struct Point { x: i32; y: i32; }
fn test(p: Point) -> Point { return p; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
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
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_constant_def(&arena, "FLAG");
    assert_constant_def(&arena, "NUM");
}

#[test]
fn test_parse_unit_return_type() {
    let source = r#"fn test() { assert(true); }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_function_signature(&arena, "test", Some(0), false);
}

#[test]
fn test_parse_function_multiple_params() {
    let source = r#"fn test(a: i32, b: i32, c: i32, d: i32) -> i32 { return a + b + c + d; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_function_signature(&arena, "test", Some(4), true);
}
