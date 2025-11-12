use crate::utils::build_ast;

#[test]
fn test_parse_simple_function() {
    let source = r#"
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);

    let definitions = &source_files[0].definitions;
    assert_eq!(definitions.len(), 1);
}

#[test]
fn test_parse_function_no_params() {
    let source = r#"
fn func() -> i32 {
    return 42;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);

    let definitions = &source_files[0].definitions;
    assert_eq!(definitions.len(), 1);
}

#[test]
fn test_parse_function_no_return() {
    let source = r#"
fn func() {
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_multiple_functions() {
    let source = r#"
fn func1() -> i32 {
    return 1;
}

fn func2() -> i32 {
    return 2;
}

fn func3(x: i32) -> i32 {
    return x;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);

    let definitions = &source_files[0].definitions;
    assert_eq!(definitions.len(), 3);
}

#[test]
fn test_parse_constant_i32() {
    let source = r#"
const X: i32 = 42;
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);

    let definitions = &source_files[0].definitions;
    assert_eq!(definitions.len(), 1);
}

#[test]
fn test_parse_constant_negative() {
    let source = r#"
const X: i32 = -1;
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_constant_i64() {
    let source = r#"
const MAX_MEM: i64 = 1000;
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_constant_unit() {
    let source = r#"
const UNIT: () = ();
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_constant_array() {
    let source = r#"
const arr: [i32; 3] = [1, 2, 3];
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
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
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_enum_definition() {
    let source = r#"
enum Arch {
    Wasm,
    Evm
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_struct_definition() {
    let source = r#"
struct Point {
    x: i32,
    y: i32
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_use_directive_simple() {
    let source = r#"
use inference::std;
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);

    let directives = &source_files[0].directives;
    assert_eq!(directives.len(), 1);
}

#[test]
fn test_parse_use_directive_with_imports() {
    let source = r#"
use inference::std::collections::{ Array, Set };
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_multiple_use_directives() {
    let source = r#"
use inference::std;
use inference::std::types::Address;
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);

    let directives = &source_files[0].directives;
    assert_eq!(directives.len(), 2);
}

#[ignore]
#[test]
fn test_parse_context_definition() {
    let source = r#"
context A {}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_binary_expression_add() {
    let source = r#"
fn test() -> i32 {
    return 1 + 2;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_binary_expression_multiply() {
    let source = r#"
fn test() -> i32 {
    return 3 * 4;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_binary_expression_subtract() {
    let source = r#"
fn test() -> i32 {
    return 10 - 5;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_binary_expression_divide() {
    let source = r#"
fn test() -> i32 {
    return 20 / 4;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_binary_expression_complex() {
    let source = r#"
fn test() -> i32 {
    return a + b * c;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_comparison_less_than() {
    let source = r#"
fn test() -> bool {
    return a < b;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_comparison_greater_than() {
    let source = r#"
fn test() -> bool {
    return a > b;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_comparison_less_equal() {
    let source = r#"
fn test() -> bool {
    return a <= b;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_comparison_greater_equal() {
    let source = r#"
fn test() -> bool {
    return a >= b;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_comparison_equal() {
    let source = r#"
fn test() -> bool {
    return a == b;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_comparison_not_equal() {
    let source = r#"
fn test() -> bool {
    return a != b;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_logical_and() {
    let source = r#"
fn test() -> bool {
    return a && b;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_logical_or() {
    let source = r#"
fn test() -> bool {
    return a || b;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_unary_not() {
    let source = r#"
fn test() -> bool {
    return !a;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore]
#[test]
fn test_parse_unary_negate() {
    let source = r#"
fn test() -> i32 {
    return -x;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_variable_declaration() {
    let source = r#"
fn test() {
    let x: i32 = 5;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_variable_declaration_no_init() {
    let source = r#"
fn test() {
    let x: i32;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_assignment() {
    let source = r#"
fn test() {
    x = 10;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_array_index_access() {
    let source = r#"
fn test() -> i32 {
    return arr[0];
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_array_index_expression() {
    let source = r#"
fn test() -> i32 {
    return arr[i + 1];
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_function_call_no_args() {
    let source = r#"
fn test() {
    foo();
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_function_call_one_arg() {
    let source = r#"
fn test() {
    foo(42);
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_function_call_multiple_args() {
    let source = r#"
fn test() {
    add(1, 2);
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_if_statement() {
    let source = r#"
fn test() {
    if (x > 0) {
        return x;
    }
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_if_else_statement() {
    let source = r#"
fn test() -> i32 {
    if (x > 0) {
        return x;
    } else {
        return 0;
    }
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_loop_statement() {
    let source = r#"
fn test() {
    loop {
        break;
    }
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore]
#[test]
fn test_parse_for_loop() {
    let source = r#"
fn test() {
    for (let i: i32 = 0; i < 10; i = i + 1) {
        foo(i);
    }
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_break_statement() {
    let source = r#"
fn test() {
    loop {
        break;
    }
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_assert_statement() {
    let source = r#"
fn test() {
    assert x > 0;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_assert_with_complex_expr() {
    let source = r#"
fn test() {
    assert a < b && b < c;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_parenthesized_expression() {
    let source = r#"
fn test() -> i32 {
    return (a + b) * c;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_bool_literal_true() {
    let source = r#"
fn test() -> bool {
    return true;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_bool_literal_false() {
    let source = r#"
fn test() -> bool {
    return false;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_string_literal() {
    let source = r#"
fn test() -> str {
    return "hello";
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_array_literal_empty() {
    let source = r#"
fn test() -> [i32; 0] {
    return [];
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_array_literal_values() {
    let source = r#"
fn test() -> [i32; 3] {
    return [1, 2, 3];
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_member_access() {
    let source = r#"
fn test() -> i32 {
    return obj.field;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore]
#[test]
fn test_parse_chained_member_access() {
    let source = r#"
fn test() -> i32 {
    return obj.field.subfield;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_struct_expression() {
    let source = r#"
fn test() -> Point {
    return Point { x: 1, y: 2 };
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_external_function() {
    let source = r#"
external fn sorting_function(Address, Address) -> Address;
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_type_alias() {
    let source = r#"
type sf = typeof(sorting_function);
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_generic_type() {
    let source = r#"
fn test() -> Array<i32> {
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_function_type_param() {
    let source = r#"
fn test(func: sf) {
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_empty_block() {
    let source = r#"
fn test() {
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_block_multiple_statements() {
    let source = r#"
fn test() {
    let x: i32 = 1;
    let y: i32 = 2;
    return x + y;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_nested_blocks() {
    let source = r#"
fn test() {
    {
        let x: i32 = 1;
    }
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_power_operator() {
    let source = r#"
fn test() -> i32 {
    return 2 ** 16;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_modulo_operator() {
    let source = r#"
fn test() -> i32 {
    return a % 4;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_comments() {
    let source = r#"
// This is a comment
fn test() -> i32 {
    // Another comment
    return 42;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}
#[ignore]
#[test]
fn test_parse_multiline_comments() {
    let source = r#"
/* This is a
   multiline comment */
fn test() -> i32 {
    return 42;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}
