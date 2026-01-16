use crate::utils::build_ast;
use inference_ast::builder::Builder;
use inference_ast::nodes::{
    AstNode, Definition, Expression, OperatorKind, Statement, UnaryOperatorKind, Visibility,
};

#[test]
fn test_parse_simple_function() {
    let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);

    let definitions = &source_files[0].definitions;
    assert_eq!(definitions.len(), 1);
}

#[test]
fn test_parse_function_no_params() {
    let source = r#"fn func() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);

    let definitions = &source_files[0].definitions;
    assert_eq!(definitions.len(), 1);
}

#[test]
fn test_parse_function_no_return() {
    let source = r#"fn func() {}"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
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
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);

    let definitions = &source_files[0].definitions;
    assert_eq!(definitions.len(), 1);
}

#[test]
fn test_parse_constant_negative() {
    let source = r#"const X: i32 = -1;"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_constant_i64() {
    let source = r#"const MAX_MEM: i64 = 1000;"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_constant_unit() {
    let source = r#"const UNIT: () = ();"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_constant_array() {
    let source = r#"const arr: [i32; 3] = [1, 2, 3];"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
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
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_enum_definition() {
    let source = r#"enum Arch { Wasm, Evm }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_struct_definition() {
    let source = r#"struct Point { x: i32, y: i32 }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
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
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
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

#[test]
fn test_parse_binary_expression_add() {
    let source = r#"fn test() -> i32 { return 1 + 2; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_binary_expression_multiply() {
    let source = r#"fn test() -> i32 { return 3 * 4; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_binary_expression_subtract() {
    let source = r#"fn test() -> i32 { return 10 - 5; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_binary_expression_divide() {
    let source = r#"fn test() -> i32 { return 20 / 4; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);

    let binary_exprs =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Binary(_))));
    assert_eq!(binary_exprs.len(), 1, "Should find 1 binary expression");

    if let AstNode::Expression(Expression::Binary(bin_expr)) = &binary_exprs[0] {
        assert_eq!(bin_expr.operator, OperatorKind::Div);
    } else {
        panic!("Expected binary expression");
    }
}

#[test]
fn test_parse_binary_expression_divide_chained() {
    let source = r#"fn test() -> i32 { return 10 / 2 / 1; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_binary_expression_divide_with_multiply() {
    let source = r#"fn test() -> i32 { return a * b / c; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_binary_expression_divide_precedence() {
    let source = r#"fn test() -> i32 { return a + b / c; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_binary_expression_complex() {
    let source = r#"fn test() -> i32 { return a + b * c; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_comparison_less_than() {
    let source = r#"fn test() -> bool { return a < b; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_comparison_greater_than() {
    let source = r#"fn test() -> bool { return a > b; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_comparison_less_equal() {
    let source = r#"fn test() -> bool { return a <= b; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_comparison_greater_equal() {
    let source = r#"fn test() -> bool { return a >= b; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_comparison_equal() {
    let source = r#"fn test() -> bool { return a == b; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_comparison_not_equal() {
    let source = r#"fn test() -> bool { return a != b; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_logical_and() {
    let source = r#"fn test() -> bool { return a && b; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_logical_or() {
    let source = r#"fn test() -> bool { return a || b; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_unary_not() {
    let source = r#"fn test() -> bool { return !a; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_unary_negate() {
    let source = r#"fn test() -> i32 { return -x; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
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

#[test]
fn test_parse_variable_declaration() {
    let source = r#"fn test() { let x: i32 = 5; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_variable_declaration_no_init() {
    let source = r#"fn test() { let x: i32; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_assignment() {
    let source = r#"fn test() { x = 10; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_array_index_access() {
    let source = r#"fn test() -> i32 { return arr[0]; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_array_index_expression() {
    let source = r#"fn test() -> i32 { return arr[i + 1]; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_function_call_no_args() {
    let source = r#"fn test() { foo(); }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_function_call_one_arg() {
    let source = r#"fn test() { foo(42); }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_function_call_multiple_args() {
    let source = r#"fn test() { add(1, 2); }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_if_statement() {
    let source = r#"fn test() { if (x > 0) { return x; } }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_if_else_statement() {
    let source = r#"fn test() -> i32 { if (x > 0) { return x; } else { return 0; } }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_loop_statement() {
    let source = r#"fn test() { loop { break; } }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_for_loop() {
    let source = r#"fn test() { let mut i: i32 = 0; loop i < 10 { foo(i); i = i + 1; } }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_break_statement() {
    let source = r#"fn test() { loop { break; } }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_assert_statement() {
    let source = r#"fn test() { assert x > 0; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_assert_with_complex_expr() {
    let source = r#"fn test() { assert a < b && b < c; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_parenthesized_expression() {
    let source = r#"fn test() -> i32 { return (a + b) * c; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_bool_literal_true() {
    let source = r#"fn test() -> bool { return true; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_bool_literal_false() {
    let source = r#"fn test() -> bool { return false; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_string_literal() {
    let source = r#"fn test() -> str { return "hello"; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_array_literal_empty() {
    let source = r#"fn test() -> [i32; 0] { return []; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_array_literal_values() {
    let source = r#"fn test() -> [i32; 3] { return [1, 2, 3]; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_member_access() {
    let source = r#"fn test() -> i32 { return obj.field; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_chained_member_access() {
    let source = r#"fn test() -> i32 { return obj.field.subfield; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_struct_expression() {
    let source = r#"fn test() -> Point { return Point { x: 1, y: 2 }; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_external_function() {
    let source = r#"external fn sorting_function(Address, Address) -> Address;"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_type_alias() {
    let source = r#"type sf = typeof(sorting_function);"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_generic_type() {
    let source = r#"fn test() -> Array<i32> {}"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_function_type_param() {
    let source = r#"fn test(func: sf) {}"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_empty_block() {
    let source = r#"fn test() {}"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_block_multiple_statements() {
    let source = r#"fn test() { let x: i32 = 1; let y: i32 = 2; return x + y; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_nested_blocks() {
    let source = r#"fn test() { { let x: i32 = 1; } }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_power_operator() {
    let source = r#"fn test() -> i32 { return 2 ** 16; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_modulo_operator() {
    let source = r#"fn test() -> i32 { return a % 4; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_comments() {
    let source = r#"// This is a comment
fn test() -> i32 {
    // Another comment
    return 42;
}"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_multiline_comments() {
    let source = r#"// This is a
//   multiline comment
fn test() -> i32 {
    return 42;
}"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_function_with_i32_return() {
    let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1);
}

#[test]
fn test_function_with_bool_return() {
    let source = r#"fn is_positive(x: i32) -> bool { return x > 0; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1);
}

#[test]
fn test_custom_struct_type() {
    let source = r#"struct Point { x: i32, y: i32 }
fn test(p: Point) -> Point { return p; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1);
}

#[test]
fn test_external_function() {
    let source = r#"external fn print(String);"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1);
}

#[test]
fn test_type_alias() {
    let source = r#"type MyInt = i32;"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1);
}

#[test]
fn test_constant_declarations() {
    let source = r#"
const FLAG: bool = true;
const NUM: i32 = 42;
"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1);
}

#[test]
fn test_unit_return_type() {
    let source = r#"fn test() { assert(true); }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1);
}

#[test]
fn test_multiple_params() {
    let source = r#"fn test(a: i32, b: i32, c: i32, d: i32) -> i32 { return a + b + c + d; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1);
}

#[test]
fn test_source_file_stores_source_correctly() {
    let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    assert_eq!(source_files.len(), 1);
    assert_eq!(source_files[0].source, source);
}

#[test]
fn test_source_file_source_with_multiple_definitions() {
    let source = r#"const X: i32 = 42;
fn add(a: i32, b: i32) -> i32 { return a + b; }
struct Point { x: i32; y: i32; }"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    assert_eq!(source_files.len(), 1);
    assert_eq!(source_files[0].source, source);
}

#[test]
fn test_source_file_source_empty_function() {
    let source = r#"fn empty() {}"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    assert_eq!(source_files[0].source, source);
}

#[test]
fn test_location_offset_extracts_function_definition() {
    let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    let source_file = &source_files[0];

    assert_eq!(source_file.definitions.len(), 1);
    if let Definition::Function(func) = &source_file.definitions[0] {
        let loc = func.location;
        let extracted = &source_file.source[loc.offset_start as usize..loc.offset_end as usize];
        assert_eq!(extracted, source);
    } else {
        panic!("Expected function definition");
    }
}

#[test]
fn test_location_offset_extracts_identifier() {
    let source = r#"fn my_function() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    let source_file = &source_files[0];

    if let Definition::Function(func) = &source_file.definitions[0] {
        let name_loc = func.name.location;
        let extracted =
            &source_file.source[name_loc.offset_start as usize..name_loc.offset_end as usize];
        assert_eq!(extracted, "my_function");
    } else {
        panic!("Expected function definition");
    }
}

#[test]
fn test_location_offset_extracts_struct_definition() {
    let source = r#"struct Point { x: i32; y: i32; }"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    let source_file = &source_files[0];

    if let Definition::Struct(struct_def) = &source_file.definitions[0] {
        let loc = struct_def.location;
        let extracted = &source_file.source[loc.offset_start as usize..loc.offset_end as usize];
        assert_eq!(extracted, source);

        let name_loc = struct_def.name.location;
        let name_extracted =
            &source_file.source[name_loc.offset_start as usize..name_loc.offset_end as usize];
        assert_eq!(name_extracted, "Point");
    } else {
        panic!("Expected struct definition");
    }
}

#[test]
fn test_location_offset_extracts_struct_fields() {
    let source = r#"struct Point { x: i32; y: i32; }"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    let source_file = &source_files[0];

    if let Definition::Struct(struct_def) = &source_file.definitions[0] {
        assert_eq!(struct_def.fields.len(), 2);

        let field_x = &struct_def.fields[0];
        let field_x_name_loc = field_x.name.location;
        let field_x_name = &source_file.source
            [field_x_name_loc.offset_start as usize..field_x_name_loc.offset_end as usize];
        assert_eq!(field_x_name, "x");

        let field_y = &struct_def.fields[1];
        let field_y_name_loc = field_y.name.location;
        let field_y_name = &source_file.source
            [field_y_name_loc.offset_start as usize..field_y_name_loc.offset_end as usize];
        assert_eq!(field_y_name, "y");
    } else {
        panic!("Expected struct definition");
    }
}

#[test]
fn test_location_offset_extracts_constant_definition() {
    let source = r#"const MAX_VALUE: i32 = 100;"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    let source_file = &source_files[0];

    if let Definition::Constant(const_def) = &source_file.definitions[0] {
        let loc = const_def.location;
        let extracted = &source_file.source[loc.offset_start as usize..loc.offset_end as usize];
        assert_eq!(extracted, source);

        let name_loc = const_def.name.location;
        let name_extracted =
            &source_file.source[name_loc.offset_start as usize..name_loc.offset_end as usize];
        assert_eq!(name_extracted, "MAX_VALUE");
    } else {
        panic!("Expected constant definition");
    }
}

#[test]
fn test_location_offset_extracts_enum_definition() {
    let source = r#"enum Color { Red, Green, Blue }"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    let source_file = &source_files[0];

    if let Definition::Enum(enum_def) = &source_file.definitions[0] {
        let loc = enum_def.location;
        let extracted = &source_file.source[loc.offset_start as usize..loc.offset_end as usize];
        assert_eq!(extracted, source);

        let name_loc = enum_def.name.location;
        let name_extracted =
            &source_file.source[name_loc.offset_start as usize..name_loc.offset_end as usize];
        assert_eq!(name_extracted, "Color");

        assert_eq!(enum_def.variants.len(), 3);
        let variant_names: Vec<&str> = enum_def
            .variants
            .iter()
            .map(|v| {
                let loc = v.location;
                &source_file.source[loc.offset_start as usize..loc.offset_end as usize]
            })
            .collect();
        assert_eq!(variant_names, vec!["Red", "Green", "Blue"]);
    } else {
        panic!("Expected enum definition");
    }
}

#[test]
fn test_location_offset_extracts_multiple_definitions() {
    let source = r#"const X: i32 = 10;
fn compute(n: i32) -> i32 { return n * 2; }"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    let source_file = &source_files[0];

    assert_eq!(source_file.definitions.len(), 2);

    if let Definition::Constant(const_def) = &source_file.definitions[0] {
        let name_loc = const_def.name.location;
        let name_extracted =
            &source_file.source[name_loc.offset_start as usize..name_loc.offset_end as usize];
        assert_eq!(name_extracted, "X");
    } else {
        panic!("Expected constant definition");
    }

    if let Definition::Function(func_def) = &source_file.definitions[1] {
        let name_loc = func_def.name.location;
        let name_extracted =
            &source_file.source[name_loc.offset_start as usize..name_loc.offset_end as usize];
        assert_eq!(name_extracted, "compute");
    } else {
        panic!("Expected function definition");
    }
}

#[test]
fn test_location_offset_extracts_function_arguments() {
    let source =
        r#"fn add(first_arg: i32, second_arg: i32) -> i32 { return first_arg + second_arg; }"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    let source_file = &source_files[0];

    if let Definition::Function(func) = &source_file.definitions[0] {
        let args = func.arguments.as_ref().expect("Expected arguments");
        assert_eq!(args.len(), 2);

        if let inference_ast::nodes::ArgumentType::Argument(arg1) = &args[0] {
            let arg1_name_loc = arg1.name.location;
            let arg1_name = &source_file.source
                [arg1_name_loc.offset_start as usize..arg1_name_loc.offset_end as usize];
            assert_eq!(arg1_name, "first_arg");
        } else {
            panic!("Expected Argument type");
        }

        if let inference_ast::nodes::ArgumentType::Argument(arg2) = &args[1] {
            let arg2_name_loc = arg2.name.location;
            let arg2_name = &source_file.source
                [arg2_name_loc.offset_start as usize..arg2_name_loc.offset_end as usize];
            assert_eq!(arg2_name, "second_arg");
        } else {
            panic!("Expected Argument type");
        }
    } else {
        panic!("Expected function definition");
    }
}

#[test]
fn test_location_offset_extracts_use_directive() {
    let source = r#"use inference::std::collections;"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    let source_file = &source_files[0];

    assert_eq!(source_file.directives.len(), 1);
    let inference_ast::nodes::Directive::Use(use_dir) = &source_file.directives[0];
    let loc = use_dir.location;
    let extracted = &source_file.source[loc.offset_start as usize..loc.offset_end as usize];
    assert_eq!(extracted, source);
}

#[test]
fn test_location_offset_with_whitespace_and_comments() {
    let source = r#"// This is a comment
fn   spaced_function  ( ) -> i32 {
    return 42;
}"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    let source_file = &source_files[0];

    assert_eq!(source_file.source, source);

    if let Definition::Function(func) = &source_file.definitions[0] {
        let name_loc = func.name.location;
        let name_extracted =
            &source_file.source[name_loc.offset_start as usize..name_loc.offset_end as usize];
        assert_eq!(name_extracted, "spaced_function");
    } else {
        panic!("Expected function definition");
    }
}

#[test]
fn test_location_offset_extracts_external_function() {
    let source = r#"external fn print_value(i32);"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    let source_file = &source_files[0];

    if let Definition::ExternalFunction(ext_func) = &source_file.definitions[0] {
        let loc = ext_func.location;
        let extracted = &source_file.source[loc.offset_start as usize..loc.offset_end as usize];
        assert_eq!(extracted, source);

        let name_loc = ext_func.name.location;
        let name_extracted =
            &source_file.source[name_loc.offset_start as usize..name_loc.offset_end as usize];
        assert_eq!(name_extracted, "print_value");
    } else {
        panic!("Expected external function definition");
    }
}

#[test]
fn test_location_offset_extracts_type_alias() {
    let source = r#"type MyInt = i32;"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    let source_file = &source_files[0];

    if let Definition::Type(type_def) = &source_file.definitions[0] {
        let loc = type_def.location;
        let extracted = &source_file.source[loc.offset_start as usize..loc.offset_end as usize];
        assert_eq!(extracted, source);

        let name_loc = type_def.name.location;
        let name_extracted =
            &source_file.source[name_loc.offset_start as usize..name_loc.offset_end as usize];
        assert_eq!(name_extracted, "MyInt");
    } else {
        panic!("Expected type definition");
    }
}

#[test]
fn test_source_file_location_covers_entire_source() {
    let source = r#"fn test() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    let source_file = &source_files[0];

    let loc = source_file.location;
    assert_eq!(loc.offset_start, 0);
    assert_eq!(loc.offset_end as usize, source.len());

    let extracted = &source_file.source[loc.offset_start as usize..loc.offset_end as usize];
    assert_eq!(extracted, source);
}

#[test]
fn test_location_offset_extracts_nested_expressions() {
    let source = r#"fn calc() -> i32 { return (1 + 2) * 3; }"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    let source_file = &source_files[0];

    assert_eq!(source_file.source, source);
    assert_eq!(source_file.definitions.len(), 1);
}

/// Tests for Builder::default() - improving coverage

#[test]
fn test_builder_default_creates_empty_builder() {
    let builder: Builder<'_, _> = Builder::default();
    let inference_language = tree_sitter_inference::language();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&inference_language)
        .expect("Error loading Inference grammar");

    let source = r#"fn test() -> i32 { return 42; }"#;
    let tree = parser.parse(source, None).unwrap();
    let code = source.as_bytes();
    let root_node = tree.root_node();

    let mut builder = builder;
    builder.add_source_code(root_node, code);
    let builder = builder.build_ast().unwrap();
    let arena = builder.arena();

    assert_eq!(arena.source_files().len(), 1);
}

/// Tests for struct expressions with fields - improving coverage

#[test]
fn test_parse_struct_expression_finds_correct_node_type() {
    let source = r#"struct Point { x: i32; y: i32; }
fn test() -> Point { return Point { x: 10, y: 20 }; }"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    assert_eq!(source_files.len(), 1);

    let struct_exprs =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Struct(_))));
    assert_eq!(struct_exprs.len(), 1, "Should find 1 struct expression");

    if let AstNode::Expression(Expression::Struct(struct_expr)) = &struct_exprs[0] {
        assert_eq!(struct_expr.name.name, "Point");
    } else {
        panic!("Expected struct expression");
    }
}

#[test]
fn test_parse_struct_expression_empty_struct() {
    let source = r#"struct Empty {}
fn test() -> Empty { return Empty {}; }"#;
    let arena = build_ast(source.to_string());

    let struct_exprs =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Struct(_))));
    assert_eq!(struct_exprs.len(), 1, "Should find 1 struct expression");

    if let AstNode::Expression(Expression::Struct(struct_expr)) = &struct_exprs[0] {
        assert_eq!(struct_expr.name.name, "Empty");
    } else {
        panic!("Expected struct expression");
    }
}

/// Tests for function definitions - improving coverage

#[test]
fn test_parse_function_definition_basic() {
    let source = r#"fn simple() -> i32 { return 1; }"#;
    let arena = build_ast(source.to_string());

    let functions = arena.functions();
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0].name.name, "simple");
}

#[test]
fn test_parse_function_definition_with_arguments() {
    let source = r#"fn with_args(x: i32, y: bool) -> i32 { return x; }"#;
    let arena = build_ast(source.to_string());

    let functions = arena.functions();
    assert_eq!(functions.len(), 1);

    let args = functions[0].arguments.as_ref();
    assert!(args.is_some(), "Should have arguments");
    assert_eq!(args.unwrap().len(), 2);
}

/// Tests for type definition statement - improving coverage

#[test]
fn test_parse_type_definition_in_function_body() {
    let source = r#"fn test() { type LocalInt = i32; }"#;
    let arena = build_ast(source.to_string());

    let type_def_stmts =
        arena.filter_nodes(|node| matches!(node, AstNode::Statement(Statement::TypeDefinition(_))));
    assert_eq!(
        type_def_stmts.len(),
        1,
        "Should find 1 type definition statement"
    );

    if let AstNode::Statement(Statement::TypeDefinition(type_def)) = &type_def_stmts[0] {
        assert_eq!(type_def.name.name, "LocalInt");
    } else {
        panic!("Expected type definition statement");
    }
}

#[test]
fn test_parse_multiple_type_definitions_in_function() {
    let source = r#"fn test() { type A = i32; type B = bool; type C = i64; }"#;
    let arena = build_ast(source.to_string());

    let type_def_stmts =
        arena.filter_nodes(|node| matches!(node, AstNode::Statement(Statement::TypeDefinition(_))));
    assert_eq!(
        type_def_stmts.len(),
        3,
        "Should find 3 type definition statements"
    );
}

/// Tests for variable definitions

#[test]
fn test_parse_variable_definition_basic() {
    let source = r#"fn test() { let x: i32 = 42; }"#;
    let arena = build_ast(source.to_string());

    let var_defs = arena
        .filter_nodes(|node| matches!(node, AstNode::Statement(Statement::VariableDefinition(_))));
    assert_eq!(var_defs.len(), 1, "Should find 1 variable definition");

    if let AstNode::Statement(Statement::VariableDefinition(var_def)) = &var_defs[0] {
        assert_eq!(var_def.name.name, "x");
    } else {
        panic!("Expected variable definition statement");
    }
}

/// Tests for forall, exists, unique, and assume blocks

#[test]
fn test_parse_forall_block() {
    let source = r#"fn test() { forall { assert true; } }"#;
    let arena = build_ast(source.to_string());

    let forall_blocks = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::Statement(Statement::Block(inference_ast::nodes::BlockType::Forall(_)))
        )
    });
    assert_eq!(forall_blocks.len(), 1, "Should find 1 forall block");
}

#[test]
fn test_parse_exists_block() {
    let source = r#"fn test() { exists { assert true; } }"#;
    let arena = build_ast(source.to_string());

    let exists_blocks = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::Statement(Statement::Block(inference_ast::nodes::BlockType::Exists(_)))
        )
    });
    assert_eq!(exists_blocks.len(), 1, "Should find 1 exists block");
}

#[test]
fn test_parse_unique_block() {
    let source = r#"fn test() { unique { assert true; } }"#;
    let arena = build_ast(source.to_string());

    let unique_blocks = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::Statement(Statement::Block(inference_ast::nodes::BlockType::Unique(_)))
        )
    });
    assert_eq!(unique_blocks.len(), 1, "Should find 1 unique block");
}

#[test]
fn test_parse_assume_block() {
    let source = r#"fn test() { assume { assert true; } }"#;
    let arena = build_ast(source.to_string());

    let assume_blocks = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::Statement(Statement::Block(inference_ast::nodes::BlockType::Assume(_)))
        )
    });
    assert_eq!(assume_blocks.len(), 1, "Should find 1 assume block");
}

/// Tests for various binary operators - improving coverage

#[test]
fn test_parse_bitwise_and() {
    let source = r#"fn test() -> i32 { return a & b; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1);
}

#[test]
fn test_parse_bitwise_or() {
    let source = r#"fn test() -> i32 { return a | b; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1);
}

#[test]
fn test_parse_bitwise_xor() {
    let source = r#"fn test() -> i32 { return a ^ b; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1);
}

#[test]
fn test_parse_shift_left() {
    let source = r#"fn test() -> i32 { return a << 2; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1);
}

#[test]
fn test_parse_shift_right() {
    let source = r#"fn test() -> i32 { return a >> 2; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1);
}

/// Tests for function arguments - improving coverage

#[test]
fn test_parse_self_reference_in_method() {
    let source = r#"struct Counter {
        value: i32;
        fn get(self) -> i32 { return 42; }
    }"#;
    let arena = build_ast(source.to_string());

    let self_refs = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::ArgumentType(inference_ast::nodes::ArgumentType::SelfReference(_))
        )
    });
    assert_eq!(self_refs.len(), 1, "Should find 1 self reference");
}

#[test]
fn test_parse_ignore_argument() {
    let source = r#"fn test(_: i32) -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let ignore_args = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::ArgumentType(inference_ast::nodes::ArgumentType::IgnoreArgument(_))
        )
    });
    assert_eq!(ignore_args.len(), 1, "Should find 1 ignore argument");
}

/// Tests for type member access expression

#[test]
fn test_parse_type_member_access() {
    let source = r#"fn test() -> i32 { return Color::Red; }"#;
    let arena = build_ast(source.to_string());

    let type_member_accesses = arena
        .filter_nodes(|node| matches!(node, AstNode::Expression(Expression::TypeMemberAccess(_))));
    assert_eq!(
        type_member_accesses.len(),
        1,
        "Should find 1 type member access"
    );
}

/// Tests for use directives

#[test]
fn test_parse_use_directive_basic() {
    let source = r#"use foo::bar;"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    assert_eq!(source_files.len(), 1);

    let directives = &source_files[0].directives;
    assert_eq!(directives.len(), 1);
}

/// Tests for qualified names and type qualified names

#[test]
fn test_parse_qualified_name_type() {
    let source = r#"fn test(x: std::i32) {}"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1);
}

#[test]
fn test_parse_function_type_parameter() {
    let source = r#"fn apply(f: fn(i32) -> i32, x: i32) -> i32 { return f(x); }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1);
}

/// Test for constant definitions

#[test]
fn test_parse_constant_definition_at_module_level() {
    let source = r#"const GLOBAL: i32 = 42;"#;
    let arena = build_ast(source.to_string());

    let const_defs =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Constant(_))));
    assert_eq!(const_defs.len(), 1, "Should find 1 constant definition");
}

/// Test for arguments

#[test]
fn test_parse_argument_with_type() {
    let source = r#"fn test(x: i32) { }"#;
    let arena = build_ast(source.to_string());

    let args = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::ArgumentType(inference_ast::nodes::ArgumentType::Argument(_))
        )
    });
    assert_eq!(args.len(), 1, "Should find 1 argument");
}

/// Test for external function definitions

#[test]
fn test_parse_external_function_with_return() {
    let source = r#"external fn get_value() -> i32;"#;
    let arena = build_ast(source.to_string());

    let ext_funcs = arena
        .filter_nodes(|node| matches!(node, AstNode::Definition(Definition::ExternalFunction(_))));
    assert_eq!(ext_funcs.len(), 1);

    if let AstNode::Definition(Definition::ExternalFunction(ext_func)) = &ext_funcs[0] {
        assert!(ext_func.returns.is_some(), "Should have return type");
    }
}

#[test]
fn test_parse_external_function_basic() {
    let source = r#"external fn do_something();"#;
    let arena = build_ast(source.to_string());

    let ext_funcs = arena
        .filter_nodes(|node| matches!(node, AstNode::Definition(Definition::ExternalFunction(_))));
    assert_eq!(ext_funcs.len(), 1);

    if let AstNode::Definition(Definition::ExternalFunction(ext_func)) = &ext_funcs[0] {
        assert_eq!(ext_func.name.name, "do_something");
    }
}

/// Tests for visibility parsing from CST

#[test]
fn test_parse_public_function_visibility() {
    let source = r#"pub fn public_function() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());
    let functions = arena.functions();
    assert_eq!(functions.len(), 1, "Should find 1 function");
    assert_eq!(
        functions[0].visibility,
        Visibility::Public,
        "Function should have Public visibility"
    );
}

#[test]
fn test_parse_private_function_visibility() {
    let source = r#"fn private_function() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());
    let functions = arena.functions();
    assert_eq!(functions.len(), 1, "Should find 1 function");
    assert_eq!(
        functions[0].visibility,
        Visibility::Private,
        "Function without pub should have Private visibility"
    );
}

#[test]
fn test_parse_public_struct_visibility() {
    let source = r#"pub struct PublicStruct { x: i32; }"#;
    let arena = build_ast(source.to_string());
    let structs =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Struct(_))));
    assert_eq!(structs.len(), 1, "Should find 1 struct");
    if let AstNode::Definition(Definition::Struct(struct_def)) = &structs[0] {
        assert_eq!(
            struct_def.visibility,
            Visibility::Public,
            "Struct should have Public visibility"
        );
    } else {
        panic!("Expected struct definition");
    }
}

#[test]
fn test_parse_private_struct_visibility() {
    let source = r#"struct PrivateStruct { x: i32; }"#;
    let arena = build_ast(source.to_string());
    let structs =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Struct(_))));
    assert_eq!(structs.len(), 1, "Should find 1 struct");
    if let AstNode::Definition(Definition::Struct(struct_def)) = &structs[0] {
        assert_eq!(
            struct_def.visibility,
            Visibility::Private,
            "Struct without pub should have Private visibility"
        );
    } else {
        panic!("Expected struct definition");
    }
}

#[test]
fn test_parse_public_enum_visibility() {
    let source = r#"pub enum PublicEnum { A, B, C }"#;
    let arena = build_ast(source.to_string());
    let enums = arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Enum(_))));
    assert_eq!(enums.len(), 1, "Should find 1 enum");
    if let AstNode::Definition(Definition::Enum(enum_def)) = &enums[0] {
        assert_eq!(
            enum_def.visibility,
            Visibility::Public,
            "Enum should have Public visibility"
        );
    } else {
        panic!("Expected enum definition");
    }
}

#[test]
fn test_parse_private_enum_visibility() {
    let source = r#"enum PrivateEnum { X, Y, Z }"#;
    let arena = build_ast(source.to_string());
    let enums = arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Enum(_))));
    assert_eq!(enums.len(), 1, "Should find 1 enum");
    if let AstNode::Definition(Definition::Enum(enum_def)) = &enums[0] {
        assert_eq!(
            enum_def.visibility,
            Visibility::Private,
            "Enum without pub should have Private visibility"
        );
    } else {
        panic!("Expected enum definition");
    }
}

#[test]
fn test_parse_public_constant_visibility() {
    let source = r#"pub const MAX_VALUE: i32 = 100;"#;
    let arena = build_ast(source.to_string());
    let consts =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Constant(_))));
    assert_eq!(consts.len(), 1, "Should find 1 constant");
    if let AstNode::Definition(Definition::Constant(const_def)) = &consts[0] {
        assert_eq!(
            const_def.visibility,
            Visibility::Public,
            "Constant should have Public visibility"
        );
    } else {
        panic!("Expected constant definition");
    }
}

#[test]
fn test_parse_private_constant_visibility() {
    let source = r#"const MIN_VALUE: i32 = 0;"#;
    let arena = build_ast(source.to_string());
    let consts =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Constant(_))));
    assert_eq!(consts.len(), 1, "Should find 1 constant");
    if let AstNode::Definition(Definition::Constant(const_def)) = &consts[0] {
        assert_eq!(
            const_def.visibility,
            Visibility::Private,
            "Constant without pub should have Private visibility"
        );
    } else {
        panic!("Expected constant definition");
    }
}

#[test]
fn test_parse_public_type_alias_visibility() {
    let source = r#"pub type MyInt = i32;"#;
    let arena = build_ast(source.to_string());
    let types = arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Type(_))));
    assert_eq!(types.len(), 1, "Should find 1 type alias");
    if let AstNode::Definition(Definition::Type(type_def)) = &types[0] {
        assert_eq!(
            type_def.visibility,
            Visibility::Public,
            "Type alias should have Public visibility"
        );
    } else {
        panic!("Expected type definition");
    }
}

#[test]
fn test_parse_private_type_alias_visibility() {
    let source = r#"type LocalInt = i32;"#;
    let arena = build_ast(source.to_string());
    let types = arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Type(_))));
    assert_eq!(types.len(), 1, "Should find 1 type alias");
    if let AstNode::Definition(Definition::Type(type_def)) = &types[0] {
        assert_eq!(
            type_def.visibility,
            Visibility::Private,
            "Type alias without pub should have Private visibility"
        );
    } else {
        panic!("Expected type definition");
    }
}

#[test]
fn test_parse_mixed_visibility_definitions() {
    let source = r#"
pub fn public_func() {}
fn private_func() {}
pub struct PublicStruct { x: i32; }
struct PrivateStruct { y: i32; }
pub const PUBLIC_CONST: i32 = 1;
const PRIVATE_CONST: i32 = 2;
"#;
    let arena = build_ast(source.to_string());
    let source_files = arena.source_files();
    assert_eq!(source_files.len(), 1);
    assert_eq!(source_files[0].definitions.len(), 6);

    let definitions = &source_files[0].definitions;

    if let Definition::Function(func) = &definitions[0] {
        assert_eq!(func.name.name, "public_func");
        assert_eq!(func.visibility, Visibility::Public);
    } else {
        panic!("Expected function definition");
    }

    if let Definition::Function(func) = &definitions[1] {
        assert_eq!(func.name.name, "private_func");
        assert_eq!(func.visibility, Visibility::Private);
    } else {
        panic!("Expected function definition");
    }

    if let Definition::Struct(struct_def) = &definitions[2] {
        assert_eq!(struct_def.name.name, "PublicStruct");
        assert_eq!(struct_def.visibility, Visibility::Public);
    } else {
        panic!("Expected struct definition");
    }

    if let Definition::Struct(struct_def) = &definitions[3] {
        assert_eq!(struct_def.name.name, "PrivateStruct");
        assert_eq!(struct_def.visibility, Visibility::Private);
    } else {
        panic!("Expected struct definition");
    }

    if let Definition::Constant(const_def) = &definitions[4] {
        assert_eq!(const_def.name.name, "PUBLIC_CONST");
        assert_eq!(const_def.visibility, Visibility::Public);
    } else {
        panic!("Expected constant definition");
    }

    if let Definition::Constant(const_def) = &definitions[5] {
        assert_eq!(const_def.name.name, "PRIVATE_CONST");
        assert_eq!(const_def.visibility, Visibility::Private);
    } else {
        panic!("Expected constant definition");
    }
}

#[test]
fn test_external_function_visibility_is_always_private() {
    let source = r#"external fn extern_func() -> i32;"#;
    let arena = build_ast(source.to_string());
    let externs = arena
        .filter_nodes(|node| matches!(node, AstNode::Definition(Definition::ExternalFunction(_))));
    assert_eq!(externs.len(), 1, "Should find 1 external function");
    if let AstNode::Definition(Definition::ExternalFunction(ext)) = &externs[0] {
        assert_eq!(
            ext.visibility,
            Visibility::Private,
            "External functions should always be private (no grammar support for pub)"
        );
    } else {
        panic!("Expected external function definition");
    }
}

#[test]
fn test_spec_definition_visibility_is_always_private() {
    let source = r#"spec MySpec { fn verify() -> bool { return true; } }"#;
    let arena = build_ast(source.to_string());
    let specs = arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Spec(_))));
    assert_eq!(specs.len(), 1, "Should find 1 spec definition");
    if let AstNode::Definition(Definition::Spec(spec)) = &specs[0] {
        assert_eq!(
            spec.visibility,
            Visibility::Private,
            "Spec definitions should always be private (no grammar support for pub)"
        );
    } else {
        panic!("Expected spec definition");
    }
}

// =============================================================================
// Extended Tests (merged from builder_extended.rs)
// =============================================================================

/// Tests parsing a function with forall block containing a return constraint.
///
/// FIXME: The source `return >= 0;` is invalid syntax. The error is stated in tree-sitter-inference CST
/// need to fing out the reason and fix it.
#[test]
fn test_parse_spec_definition_extended() {
    let source =
        r#"fn sum(items: [i32; 10]) -> i32 { forall { return >= 0; } let result: i32 = 0; }"#;
    let arena = build_ast(source.to_string());
    let source_file = &arena.source_files()[0];
    assert_eq!(source_file.definitions.len(), 1);
    assert_eq!(source_file.function_definitions().len(), 1);
    let func_def = &source_file.function_definitions()[0];
    assert_eq!(func_def.name(), "sum");

    assert!(func_def.has_parameters());
    let args = func_def.arguments.as_ref().expect("Should have arguments");
    assert_eq!(args.len(), 1);
    if let inference_ast::nodes::ArgumentType::Argument(arg) = &args[0] {
        assert_eq!(arg.name.name, "items");
    } else {
        panic!("Expected Argument type");
    }

    assert!(!func_def.is_void());

    let statements = func_def.body.statements();
    assert_eq!(
        statements.len(),
        2,
        "Should have forall block and variable definition"
    );
}

#[test]
fn test_parse_function_with_forall_extended() {
    let source = r#"fn test() -> () forall { return (); }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
    let source_file = &source_files[0];
    assert_eq!(source_file.definitions.len(), 1);
    assert_eq!(source_file.function_definitions().len(), 1);
    let func_def = &source_file.function_definitions()[0];
    assert_eq!(func_def.name(), "test");
    assert!(!func_def.has_parameters());
    assert!(func_def.is_void());
}

#[test]
fn test_parse_function_with_assume_extended() {
    let source = r#"fn test() -> () forall { assume { a = valid_Address(); } }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
    let source_file = &source_files[0];
    assert_eq!(source_file.definitions.len(), 1);
    assert_eq!(source_file.function_definitions().len(), 1);
    let func_def = &source_file.function_definitions()[0];
    assert_eq!(func_def.name(), "test");
    assert!(!func_def.has_parameters());
    assert!(func_def.is_void());
    let statements = func_def.body.statements();
    assert!(!statements.is_empty());
}

#[test]
fn test_parse_function_with_filter() {
    let source = r#"fn add(a: i32, b: i32) -> i32 { filter { let x: i32 = @; return @ + b; } return a + b; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
    let source_file = &source_files[0];
    assert_eq!(source_file.definitions.len(), 1);
    assert_eq!(source_file.function_definitions().len(), 1);
    let func_def = &source_file.function_definitions()[0];
    assert_eq!(func_def.name(), "add");
    assert!(func_def.has_parameters());
    assert_eq!(func_def.arguments.as_ref().unwrap().len(), 2);
    assert!(!func_def.is_void());
    let statements = func_def.body.statements();
    assert!(statements.len() >= 2);
}

#[test]
fn test_parse_qualified_type() {
    let source = r#"use collections::HashMap;
fn test() -> HashMap { return HashMap {}; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
    let source_file = &source_files[0];
    assert_eq!(source_file.definitions.len(), 1);
    assert_eq!(source_file.directives.len(), 1);
    assert_eq!(source_file.function_definitions().len(), 1);
    let use_dirs: Vec<_> = source_file
        .directives
        .iter()
        .filter(|d| matches!(d, inference_ast::nodes::Directive::Use(_)))
        .map(|d| match d {
            inference_ast::nodes::Directive::Use(use_dir) => use_dir.clone(),
        })
        .collect();
    assert_eq!(use_dirs.len(), 1);
    let func_def = &source_file.function_definitions()[0];
    assert_eq!(func_def.name(), "test");
    assert!(!func_def.has_parameters());
    assert!(!func_def.is_void());
    let use_directive = &use_dirs[0];
    assert!(use_directive.imported_types.is_some() || use_directive.segments.is_some());
}

/// Tests parsing a typeof expression with an external function reference.
///
/// Note: External function arguments are parsed but type-only arguments (without names)
/// are captured differently than named arguments. See core/ast/src/builder.rs for
/// external function parsing details.
#[test]
fn test_parse_typeof_expression() {
    let source = r#"external fn sorting_function(a: Address, b: Address) -> Address;
type sf = typeof(sorting_function);"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
    let source_file = &source_files[0];
    assert_eq!(source_file.definitions.len(), 2);
    let ext_funcs: Vec<_> = source_file
        .definitions
        .iter()
        .filter_map(|d| match d {
            inference_ast::nodes::Definition::ExternalFunction(ext) => Some(ext.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(ext_funcs.len(), 1);
    let type_defs: Vec<_> = source_file
        .definitions
        .iter()
        .filter_map(|d| match d {
            inference_ast::nodes::Definition::Type(type_def) => Some(type_def.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(type_defs.len(), 1);
    let external_fn = &ext_funcs[0];
    assert_eq!(external_fn.name(), "sorting_function");
    let type_def = &type_defs[0];
    assert_eq!(type_def.name(), "sf");
}

#[test]
fn test_parse_typeof_with_identifier() {
    let source = r#"const x: i32 = 5;type mytype = typeof(x);"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_method_call_expression() {
    let source = r#"fn test() { let result: i32 = object.method(); }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_method_call_with_args() {
    let source = r#"fn test() { let result: u64 = object.method(arg1, arg2); }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_struct_with_multiple_fields() {
    let source = r#"struct Point { x: i32; y: i32; z: i32; label: String; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
    let source_file = &source_files[0];
    assert_eq!(source_file.definitions.len(), 1);
    let struct_defs: Vec<_> = source_file
        .definitions
        .iter()
        .filter_map(|d| match d {
            inference_ast::nodes::Definition::Struct(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(struct_defs.len(), 1);
    let struct_def = &struct_defs[0];
    assert_eq!(struct_def.name(), "Point");
    assert_eq!(struct_def.fields.len(), 4);
    let field_names: Vec<String> = struct_def.fields.iter().map(|f| f.name.name()).collect();
    assert!(field_names.contains(&"x".to_string()));
    assert!(field_names.contains(&"y".to_string()));
    assert!(field_names.contains(&"z".to_string()));
    assert!(field_names.contains(&"label".to_string()));
}

#[test]
fn test_parse_enum_with_variants() {
    let source = r#"enum Color { Red, Green, Blue, Custom }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
    let source_file = &source_files[0];
    assert_eq!(source_file.definitions.len(), 1);
    let enum_defs: Vec<_> = source_file
        .definitions
        .iter()
        .filter_map(|d| match d {
            inference_ast::nodes::Definition::Enum(e) => Some(e.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(enum_defs.len(), 1);
    let enum_def = &enum_defs[0];
    assert_eq!(enum_def.name(), "Color");
    assert_eq!(enum_def.variants.len(), 4);
    let variant_names: Vec<String> = enum_def.variants.iter().map(|v| v.name()).collect();
    assert!(variant_names.contains(&"Red".to_string()));
    assert!(variant_names.contains(&"Green".to_string()));
    assert!(variant_names.contains(&"Blue".to_string()));
    assert!(variant_names.contains(&"Custom".to_string()));
}

#[test]
fn test_parse_complex_struct_expression() {
    let source =
        r#"fn test() { let point: Point = Point { x: 10, y: 20, z: 30, label: "origin" }; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_nested_struct_expression() {
    let source = r#"fn test() {
    let rect: Rectangle = Rectangle {
        top_left: Point { x: 0, y: 0 },
        bottom_right: Point { x: 100, y: 100 }
    };}"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_complex_binary_expression() {
    let source = r#"fn test() -> i32 { return (a + b) * (c - d) / e; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
    let source_file = &source_files[0];
    assert_eq!(source_file.definitions.len(), 1);
    assert_eq!(source_file.function_definitions().len(), 1);
    let func_def = &source_file.function_definitions()[0];
    assert_eq!(func_def.name(), "test");
    assert!(!func_def.has_parameters());
    assert!(!func_def.is_void());
    let statements = func_def.body.statements();
    assert_eq!(statements.len(), 1);
}

#[test]
fn test_parse_nested_function_calls() {
    let source = r#"fn test() -> i32 { return foo(bar(baz(x))); }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_if_elseif_else() {
    let source = r#"fn test(x: i32) -> i32 { if x > 10 { return 1; } else if x > 5 { return 2; } else { return 3; } }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_nested_if_statements() {
    let source = r#"
fn test(x: i32, y: i32) -> i32 {
    if x > 0 {
        if y > 0 { return 1; }
        else { return 2; }
    } else { return 3; }}"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_use_from_directive() {
    let source = r#"use std::collections::HashMap from "std";"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_builder_multiple_source_files() {
    let source = r#"
fn test1() -> i32 { return 1; }
fn test2() -> i32 { return 2; }
fn test3() -> i32 { return 3; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
    assert_eq!(source_files[0].definitions.len(), 3);
}

#[test]
fn test_parse_multiple_variable_declarations() {
    let source = r#"fn test() { let a: i32 = 1; let b: i64 = 2; let c: u32 = 3; let d: u64 = 4;}"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_variable_with_type_annotation() {
    let source = r#"fn test() { let x: i32 = 42; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_assignment_to_member() {
    let source = r#"fn test() { point.x = 10; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_assignment_to_array_index() {
    let source = r#"fn test() { arr[0] = 42; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_array_of_arrays() {
    let source = r#"fn test() { let matrix: [[i32; 2]; 2] = [[1, 2], [3, 4]]; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_function_with_self_param() {
    let source = r#"fn method(self, x: i32) -> i32 { return x; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);

    if let Some(def) = source_files[0].definitions.first() {
        if let inference_ast::nodes::Definition::Function(func) = def {
            let args = func
                .arguments
                .as_ref()
                .expect("Function should have arguments");
            assert!(
                args.iter()
                    .any(|arg| matches!(arg, inference_ast::nodes::ArgumentType::SelfReference(_))),
                "Function should have a self parameter"
            );
        } else {
            panic!("Expected a function definition");
        }
    } else {
        panic!("Expected at least one definition");
    }
}

#[test]
fn test_parse_function_with_ignore_param() {
    let source = r#"fn test(_: i32) -> i32 { return 0; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_empty_array_literal_extended() {
    let source = r#"fn test() { let arr: [i32; 0] = []; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_function_with_mixed_params() {
    let source = r#"fn test(a: i32, _: i32, c: i32) -> i32 { return a + c; }"#;
    let arena = build_ast(source.to_string());
    let source_files = &arena.source_files();
    assert_eq!(source_files.len(), 1);
}

// =============================================================================
// Known Limitations (documented for future implementation)
// =============================================================================
//
// The following test cases are not included because they cause the parser to panic
// instead of returning proper errors. Per CONTRIBUTING.md, the parser should handle
// invalid input gracefully without panicking. These should be addressed in a future
// issue focused on parser error handling improvements.
//
// 1. Variable declaration without type annotation:
//    `let result = object.method();` - Panics: "Unexpected statement type: ERROR"
//    `let point = Point { x: 10, y: 20 };` - Panics: "Unexpected statement type: ERROR"
//
// 2. Struct expression as constant value:
//    `const ORIGIN: Point = Point { x: 0, y: 0 };` - Panics: "Unexpected literal type: struct_expression"
//
