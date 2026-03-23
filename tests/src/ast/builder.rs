use crate::ast::helpers::{
    assert_array_index, assert_array_literal, assert_array_type, assert_assign,
    assert_assert_stmt, assert_binary, assert_block, assert_block_stmt, assert_bool,
    assert_break, assert_const_def, assert_custom_type, assert_enum_def, assert_expr_stmt,
    assert_extern_function_def, assert_fn_call, assert_function_def, assert_generic_type,
    assert_ident_expr, assert_if, assert_loop, assert_member_access, assert_named_arg,
    assert_number, assert_parens, assert_prefix_unary, assert_return, assert_simple_type,
    assert_string_literal, assert_struct_def, assert_struct_literal, assert_type_alias_def,
    assert_type_expr, assert_type_only_arg, assert_unit_literal, assert_var_def, parse_defs,
    parse_one,
};
use crate::utils::try_build_ast;
use inference_ast::nodes::{
    BlockKind, OperatorKind, SimpleTypeKind, TypeNode, UnaryOperatorKind, Visibility,
};

// ---------------------------------------------------------------------------
// Function definitions
// ---------------------------------------------------------------------------

#[test]
fn test_parse_simple_function() {
    let (arena, defs) = parse_defs("fn add(a: i32, b: i32) -> i32 { return a + b; }");
    assert_eq!(defs.len(), 1);

    let (args, ret, body) =
        assert_function_def(&arena, defs[0], "add", Visibility::Private, 2, true, 1);

    let a_ty = assert_named_arg(&arena, &args[0], "a", false);
    assert_simple_type(&arena, a_ty, SimpleTypeKind::I32);
    let b_ty = assert_named_arg(&arena, &args[1], "b", false);
    assert_simple_type(&arena, b_ty, SimpleTypeKind::I32);

    assert_simple_type(&arena, ret.unwrap(), SimpleTypeKind::I32);

    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Add);
    assert_ident_expr(&arena, left, "a");
    assert_ident_expr(&arena, right, "b");
}

#[test]
fn test_parse_function_no_params() {
    let (arena, defs) = parse_defs("fn func() -> i32 { return 42; }");
    assert_eq!(defs.len(), 1);

    let (args, ret, body) =
        assert_function_def(&arena, defs[0], "func", Visibility::Private, 0, true, 1);
    assert!(args.is_empty());
    assert_simple_type(&arena, ret.unwrap(), SimpleTypeKind::I32);

    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    assert_number(&arena, ret_expr, "42");
}

#[test]
fn test_parse_function_no_return() {
    let (arena, defs) = parse_defs("fn func() {}");
    assert_eq!(defs.len(), 1);

    let (args, ret, body) =
        assert_function_def(&arena, defs[0], "func", Visibility::Private, 0, false, 0);
    assert!(args.is_empty());
    assert!(ret.is_none());
    let stmts = assert_block(&arena, body, BlockKind::Regular, 0);
    assert!(stmts.is_empty());
}

#[test]
fn test_parse_multiple_functions() {
    let source = r#"
fn func1() -> i32 { return 1; }
fn func2() -> i32 { return 2; }
fn func3(x: i32) -> i32 { return x; }
"#;
    let (arena, defs) = parse_defs(source);
    assert_eq!(defs.len(), 3);

    let (_, ret1, body1) =
        assert_function_def(&arena, defs[0], "func1", Visibility::Private, 0, true, 1);
    assert_simple_type(&arena, ret1.unwrap(), SimpleTypeKind::I32);
    let stmts1 = assert_block(&arena, body1, BlockKind::Regular, 1);
    let ret_expr1 = assert_return(&arena, stmts1[0]);
    assert_number(&arena, ret_expr1, "1");

    let (_, ret2, body2) =
        assert_function_def(&arena, defs[1], "func2", Visibility::Private, 0, true, 1);
    assert_simple_type(&arena, ret2.unwrap(), SimpleTypeKind::I32);
    let stmts2 = assert_block(&arena, body2, BlockKind::Regular, 1);
    let ret_expr2 = assert_return(&arena, stmts2[0]);
    assert_number(&arena, ret_expr2, "2");

    let (args3, ret3, body3) =
        assert_function_def(&arena, defs[2], "func3", Visibility::Private, 1, true, 1);
    let x_ty = assert_named_arg(&arena, &args3[0], "x", false);
    assert_simple_type(&arena, x_ty, SimpleTypeKind::I32);
    assert_simple_type(&arena, ret3.unwrap(), SimpleTypeKind::I32);
    let stmts3 = assert_block(&arena, body3, BlockKind::Regular, 1);
    let ret_expr3 = assert_return(&arena, stmts3[0]);
    assert_ident_expr(&arena, ret_expr3, "x");
}

#[test]
fn test_parse_function_multiple_params() {
    let source = "fn test(a: i32, b: i32, c: i32, d: i32) -> i32 { return a + b + c + d; }";
    let (arena, defs) = parse_defs(source);
    assert_eq!(defs.len(), 1);

    let (args, ret, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 4, true, 1);

    let a_ty = assert_named_arg(&arena, &args[0], "a", false);
    assert_simple_type(&arena, a_ty, SimpleTypeKind::I32);
    let b_ty = assert_named_arg(&arena, &args[1], "b", false);
    assert_simple_type(&arena, b_ty, SimpleTypeKind::I32);
    let c_ty = assert_named_arg(&arena, &args[2], "c", false);
    assert_simple_type(&arena, c_ty, SimpleTypeKind::I32);
    let d_ty = assert_named_arg(&arena, &args[3], "d", false);
    assert_simple_type(&arena, d_ty, SimpleTypeKind::I32);

    assert_simple_type(&arena, ret.unwrap(), SimpleTypeKind::I32);

    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    // a + b + c + d parses left-associatively: ((a + b) + c) + d
    let (lhs3, d_expr) = assert_binary(&arena, ret_expr, OperatorKind::Add);
    assert_ident_expr(&arena, d_expr, "d");
    let (lhs2, c_expr) = assert_binary(&arena, lhs3, OperatorKind::Add);
    assert_ident_expr(&arena, c_expr, "c");
    let (a_expr, b_expr) = assert_binary(&arena, lhs2, OperatorKind::Add);
    assert_ident_expr(&arena, a_expr, "a");
    assert_ident_expr(&arena, b_expr, "b");
}

#[test]
fn test_parse_function_with_bool_return() {
    let source = "fn is_positive(x: i32) -> bool { return x > 0; }";
    let (arena, defs) = parse_defs(source);
    assert_eq!(defs.len(), 1);

    let (args, ret, body) =
        assert_function_def(&arena, defs[0], "is_positive", Visibility::Private, 1, true, 1);

    let x_ty = assert_named_arg(&arena, &args[0], "x", false);
    assert_simple_type(&arena, x_ty, SimpleTypeKind::I32);
    assert_simple_type(&arena, ret.unwrap(), SimpleTypeKind::Bool);

    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Gt);
    assert_ident_expr(&arena, left, "x");
    assert_number(&arena, right, "0");
}

#[test]
fn test_parse_function_custom_struct_param() {
    let source = r#"struct Point { x: i32; y: i32; }
fn test(p: Point) -> Point { return p; }"#;
    let (arena, defs) = parse_defs(source);
    assert_eq!(defs.len(), 2);

    let (fields, methods) =
        assert_struct_def(&arena, defs[0], "Point", Visibility::Private, 2, 0);
    assert!(methods.is_empty());
    assert_eq!(arena[fields[0].name].name, "x");
    assert_simple_type(&arena, fields[0].ty, SimpleTypeKind::I32);
    assert_eq!(arena[fields[1].name].name, "y");
    assert_simple_type(&arena, fields[1].ty, SimpleTypeKind::I32);

    let (args, ret, body) =
        assert_function_def(&arena, defs[1], "test", Visibility::Private, 1, true, 1);
    let p_ty = assert_named_arg(&arena, &args[0], "p", false);
    assert_custom_type(&arena, p_ty, "Point");
    assert_custom_type(&arena, ret.unwrap(), "Point");

    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    assert_ident_expr(&arena, ret_expr, "p");
}

#[test]
fn test_parse_unit_return_type() {
    // `assert(true)` parses as Stmt::Assert with a parenthesized expression
    let source = "fn test() { assert(true); }";
    let (arena, defs) = parse_defs(source);
    assert_eq!(defs.len(), 1);

    let (_, ret, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 1);
    assert!(ret.is_none());

    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let expr = assert_assert_stmt(&arena, stmts[0]);
    let inner = assert_parens(&arena, expr);
    assert_bool(&arena, inner, true);
}

// ---------------------------------------------------------------------------
// Constant definitions
// ---------------------------------------------------------------------------

#[test]
fn test_parse_constant_i32() {
    let (arena, defs) = parse_defs("const X: i32 = 42;");
    assert_eq!(defs.len(), 1);

    let (ty, value) = assert_const_def(&arena, defs[0], "X", Visibility::Private);
    assert_simple_type(&arena, ty, SimpleTypeKind::I32);
    assert_number(&arena, value, "42");
}

#[test]
fn test_parse_constant_negative() {
    let (arena, defs) = parse_defs("const X: i32 = -1;");
    assert_eq!(defs.len(), 1);

    let (ty, value) = assert_const_def(&arena, defs[0], "X", Visibility::Private);
    assert_simple_type(&arena, ty, SimpleTypeKind::I32);
    assert_number(&arena, value, "-1");
}

#[test]
fn test_parse_constant_i64() {
    let (arena, defs) = parse_defs("const MAX_MEM: i64 = 1000;");
    assert_eq!(defs.len(), 1);

    let (ty, value) = assert_const_def(&arena, defs[0], "MAX_MEM", Visibility::Private);
    assert_simple_type(&arena, ty, SimpleTypeKind::I64);
    assert_number(&arena, value, "1000");
}

#[test]
fn test_parse_constant_unit() {
    let (arena, defs) = parse_defs("const UNIT: () = ();");
    assert_eq!(defs.len(), 1);

    let (ty, value) = assert_const_def(&arena, defs[0], "UNIT", Visibility::Private);
    assert_simple_type(&arena, ty, SimpleTypeKind::Unit);
    assert_unit_literal(&arena, value);
}

#[test]
fn test_parse_constant_array() {
    let (arena, defs) = parse_defs("const arr: [i32; 3] = [1, 2, 3];");
    assert_eq!(defs.len(), 1);

    let (ty, value) = assert_const_def(&arena, defs[0], "arr", Visibility::Private);
    let (elem_ty, size_expr) = assert_array_type(&arena, ty);
    assert_simple_type(&arena, elem_ty, SimpleTypeKind::I32);
    assert_number(&arena, size_expr, "3");

    let elems = assert_array_literal(&arena, value, 3);
    assert_number(&arena, elems[0], "1");
    assert_number(&arena, elems[1], "2");
    assert_number(&arena, elems[2], "3");
}

#[test]
fn test_parse_constant_nested_array() {
    let source = r#"
const EMPTY_BOARD: [[bool; 3]; 3] =
  [[false, false, false],
   [false, false, false],
   [false, false, false]];
"#;
    let (arena, defs) = parse_defs(source);
    assert_eq!(defs.len(), 1);

    let (ty, value) = assert_const_def(&arena, defs[0], "EMPTY_BOARD", Visibility::Private);

    // Outer array type: [[bool; 3]; 3]
    let (inner_ty, outer_size) = assert_array_type(&arena, ty);
    assert_number(&arena, outer_size, "3");

    // Inner array type: [bool; 3]
    let (elem_ty, inner_size) = assert_array_type(&arena, inner_ty);
    assert_simple_type(&arena, elem_ty, SimpleTypeKind::Bool);
    assert_number(&arena, inner_size, "3");

    // Outer array literal: 3 rows
    let rows = assert_array_literal(&arena, value, 3);
    for row in &rows {
        let cells = assert_array_literal(&arena, *row, 3);
        for cell in &cells {
            assert_bool(&arena, *cell, false);
        }
    }
}

#[test]
fn test_parse_constant_declarations_multiple() {
    let source = r#"
const FLAG: bool = true;
const NUM: i32 = 42;
"#;
    let (arena, defs) = parse_defs(source);
    assert_eq!(defs.len(), 2);

    let (ty0, val0) = assert_const_def(&arena, defs[0], "FLAG", Visibility::Private);
    assert_simple_type(&arena, ty0, SimpleTypeKind::Bool);
    assert_bool(&arena, val0, true);

    let (ty1, val1) = assert_const_def(&arena, defs[1], "NUM", Visibility::Private);
    assert_simple_type(&arena, ty1, SimpleTypeKind::I32);
    assert_number(&arena, val1, "42");
}

// ---------------------------------------------------------------------------
// Enum definitions
// ---------------------------------------------------------------------------

#[test]
fn test_parse_enum_definition() {
    let (arena, defs) = parse_defs("enum Arch { Wasm, Evm }");
    assert_eq!(defs.len(), 1);

    assert_enum_def(&arena, defs[0], "Arch", Visibility::Private, &["Wasm", "Evm"]);
}

// ---------------------------------------------------------------------------
// Struct definitions
// ---------------------------------------------------------------------------

#[test]
fn test_parse_struct_definition() {
    let (arena, defs) = parse_defs("struct Point { x: i32; y: i32; }");
    assert_eq!(defs.len(), 1);

    let (fields, methods) =
        assert_struct_def(&arena, defs[0], "Point", Visibility::Private, 2, 0);
    assert!(methods.is_empty());

    assert_eq!(arena[fields[0].name].name, "x");
    assert_simple_type(&arena, fields[0].ty, SimpleTypeKind::I32);
    assert_eq!(arena[fields[1].name].name, "y");
    assert_simple_type(&arena, fields[1].ty, SimpleTypeKind::I32);
}

#[test]
fn test_parse_struct_with_methods() {
    let source = r#"
    struct Counter {
        value: i32;

        fn get() -> i32 { return 42; }
    }
    "#;
    let (arena, defs) = parse_defs(source);
    assert_eq!(defs.len(), 1);

    let (fields, methods) =
        assert_struct_def(&arena, defs[0], "Counter", Visibility::Private, 1, 1);

    assert_eq!(arena[fields[0].name].name, "value");
    assert_simple_type(&arena, fields[0].ty, SimpleTypeKind::I32);

    let (_, ret, body) =
        assert_function_def(&arena, methods[0], "get", Visibility::Private, 0, true, 1);
    assert_simple_type(&arena, ret.unwrap(), SimpleTypeKind::I32);

    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    assert_number(&arena, ret_expr, "42");
}

// ---------------------------------------------------------------------------
// Directive tests
// ---------------------------------------------------------------------------

#[test]
fn test_parse_use_directive_simple() {
    let arena = parse_one("use inference::std;");
    let sf = arena.source_files().next().unwrap();
    assert!(sf.defs.is_empty());
    assert_eq!(sf.directives.len(), 1);
}

#[test]
fn test_parse_use_directive_with_imports() {
    let arena = parse_one("use inference::std::collections::{ Array, Set };");
    let sf = arena.source_files().next().unwrap();
    assert_eq!(sf.directives.len(), 1);
}

#[test]
fn test_parse_multiple_use_directives() {
    let source = "use inference::std;\nuse inference::std::types::Address;";
    let arena = parse_one(source);
    let sf = arena.source_files().next().unwrap();
    assert_eq!(sf.directives.len(), 2);
}

// ---------------------------------------------------------------------------
// Binary expression tests
// ---------------------------------------------------------------------------

#[test]
fn test_parse_binary_expression_add() {
    let (arena, defs) = parse_defs("fn test() -> i32 { return 1 + 2; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Add);
    assert_number(&arena, left, "1");
    assert_number(&arena, right, "2");
}

#[test]
fn test_parse_binary_expression_subtract() {
    let (arena, defs) = parse_defs("fn test() -> i32 { return 10 - 5; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Sub);
    assert_number(&arena, left, "10");
    assert_number(&arena, right, "5");
}

#[test]
fn test_parse_binary_expression_multiply() {
    let (arena, defs) = parse_defs("fn test() -> i32 { return 3 * 4; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Mul);
    assert_number(&arena, left, "3");
    assert_number(&arena, right, "4");
}

#[test]
fn test_parse_binary_expression_divide() {
    let (arena, defs) = parse_defs("fn test() -> i32 { return 20 / 4; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Div);
    assert_number(&arena, left, "20");
    assert_number(&arena, right, "4");
}

#[test]
fn test_parse_binary_expression_divide_chained() {
    // 10 / 2 / 1 parses as (10 / 2) / 1 (left-associative)
    let (arena, defs) = parse_defs("fn test() -> i32 { return 10 / 2 / 1; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);

    let (inner, one) = assert_binary(&arena, ret_expr, OperatorKind::Div);
    assert_number(&arena, one, "1");
    let (ten, two) = assert_binary(&arena, inner, OperatorKind::Div);
    assert_number(&arena, ten, "10");
    assert_number(&arena, two, "2");
}

#[test]
fn test_parse_binary_expression_divide_with_multiply() {
    // a * b / c  parses as (a * b) / c
    let (arena, defs) = parse_defs("fn test() -> i32 { return a * b / c; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);

    let (mul_expr, c_expr) = assert_binary(&arena, ret_expr, OperatorKind::Div);
    assert_ident_expr(&arena, c_expr, "c");
    let (a_expr, b_expr) = assert_binary(&arena, mul_expr, OperatorKind::Mul);
    assert_ident_expr(&arena, a_expr, "a");
    assert_ident_expr(&arena, b_expr, "b");
}

#[test]
fn test_parse_binary_expression_divide_precedence() {
    // a + b / c  parses as a + (b / c)  because / binds tighter than +
    let (arena, defs) = parse_defs("fn test() -> i32 { return a + b / c; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);

    let (a_expr, div_expr) = assert_binary(&arena, ret_expr, OperatorKind::Add);
    assert_ident_expr(&arena, a_expr, "a");
    let (b_expr, c_expr) = assert_binary(&arena, div_expr, OperatorKind::Div);
    assert_ident_expr(&arena, b_expr, "b");
    assert_ident_expr(&arena, c_expr, "c");
}

#[test]
fn test_parse_binary_expression_complex_precedence() {
    // a + b * c  parses as a + (b * c)
    let (arena, defs) = parse_defs("fn test() -> i32 { return a + b * c; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);

    let (a_expr, mul_expr) = assert_binary(&arena, ret_expr, OperatorKind::Add);
    assert_ident_expr(&arena, a_expr, "a");
    let (b_expr, c_expr) = assert_binary(&arena, mul_expr, OperatorKind::Mul);
    assert_ident_expr(&arena, b_expr, "b");
    assert_ident_expr(&arena, c_expr, "c");
}

#[test]
fn test_parse_comparison_less_than() {
    let (arena, defs) = parse_defs("fn test() -> bool { return a < b; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Lt);
    assert_ident_expr(&arena, left, "a");
    assert_ident_expr(&arena, right, "b");
}

#[test]
fn test_parse_comparison_greater_than() {
    let (arena, defs) = parse_defs("fn test() -> bool { return a > b; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Gt);
    assert_ident_expr(&arena, left, "a");
    assert_ident_expr(&arena, right, "b");
}

#[test]
fn test_parse_comparison_less_equal() {
    let (arena, defs) = parse_defs("fn test() -> bool { return a <= b; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Le);
    assert_ident_expr(&arena, left, "a");
    assert_ident_expr(&arena, right, "b");
}

#[test]
fn test_parse_comparison_greater_equal() {
    let (arena, defs) = parse_defs("fn test() -> bool { return a >= b; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Ge);
    assert_ident_expr(&arena, left, "a");
    assert_ident_expr(&arena, right, "b");
}

#[test]
fn test_parse_comparison_equal() {
    let (arena, defs) = parse_defs("fn test() -> bool { return a == b; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Eq);
    assert_ident_expr(&arena, left, "a");
    assert_ident_expr(&arena, right, "b");
}

#[test]
fn test_parse_comparison_not_equal() {
    let (arena, defs) = parse_defs("fn test() -> bool { return a != b; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Ne);
    assert_ident_expr(&arena, left, "a");
    assert_ident_expr(&arena, right, "b");
}

#[test]
fn test_parse_logical_and() {
    let (arena, defs) = parse_defs("fn test() -> bool { return a && b; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::And);
    assert_ident_expr(&arena, left, "a");
    assert_ident_expr(&arena, right, "b");
}

#[test]
fn test_parse_logical_or() {
    let (arena, defs) = parse_defs("fn test() -> bool { return a || b; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Or);
    assert_ident_expr(&arena, left, "a");
    assert_ident_expr(&arena, right, "b");
}

#[test]
fn test_parse_power_operator() {
    let (arena, defs) = parse_defs("fn test() -> i32 { return 2 ** 16; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Pow);
    assert_number(&arena, left, "2");
    assert_number(&arena, right, "16");
}

#[test]
fn test_parse_modulo_operator() {
    let (arena, defs) = parse_defs("fn test() -> i32 { return a % 4; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Mod);
    assert_ident_expr(&arena, left, "a");
    assert_number(&arena, right, "4");
}

// ---------------------------------------------------------------------------
// Unary expression tests
// ---------------------------------------------------------------------------

#[test]
fn test_parse_unary_not() {
    let (arena, defs) = parse_defs("fn test() -> bool { return !a; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let inner = assert_prefix_unary(&arena, ret_expr, UnaryOperatorKind::Not);
    assert_ident_expr(&arena, inner, "a");
}

#[test]
fn test_parse_unary_negate() {
    let (arena, defs) = parse_defs("fn test() -> i32 { return -x; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let inner = assert_prefix_unary(&arena, ret_expr, UnaryOperatorKind::Neg);
    assert_ident_expr(&arena, inner, "x");
}

#[test]
fn test_parse_negative_literal() {
    // Grammar parses -42 as a negative number literal, not PrefixUnary(Neg, 42)
    let (arena, defs) = parse_defs("fn test() -> i32 { return -42; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    assert_number(&arena, ret_expr, "-42");
}

#[test]
fn test_parse_unary_negate_parenthesized() {
    // -(42) is PrefixUnary(Neg, Parenthesized(42))
    let (arena, defs) = parse_defs("fn test() -> i32 { return -(42); }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let inner = assert_prefix_unary(&arena, ret_expr, UnaryOperatorKind::Neg);
    let paren_inner = assert_parens(&arena, inner);
    assert_number(&arena, paren_inner, "42");
}

#[test]
fn test_parse_unary_bitnot() {
    let (arena, defs) = parse_defs("fn test() -> i32 { return ~flags; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let inner = assert_prefix_unary(&arena, ret_expr, UnaryOperatorKind::BitNot);
    assert_ident_expr(&arena, inner, "flags");
}

#[test]
fn test_parse_unary_double_negate() {
    // --x parses as Neg(Neg(x))
    let (arena, defs) = parse_defs("fn test() -> i32 { return --x; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let outer = assert_prefix_unary(&arena, ret_expr, UnaryOperatorKind::Neg);
    let inner = assert_prefix_unary(&arena, outer, UnaryOperatorKind::Neg);
    assert_ident_expr(&arena, inner, "x");
}

#[test]
fn test_parse_unary_negate_bitnot() {
    // -~x parses as Neg(BitNot(x))
    let (arena, defs) = parse_defs("fn test() -> i32 { return -~x; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let outer = assert_prefix_unary(&arena, ret_expr, UnaryOperatorKind::Neg);
    let inner = assert_prefix_unary(&arena, outer, UnaryOperatorKind::BitNot);
    assert_ident_expr(&arena, inner, "x");
}

#[test]
fn test_parse_unary_bitnot_negate() {
    // ~-x parses as BitNot(Neg(x))
    let (arena, defs) = parse_defs("fn test() -> i32 { return ~-x; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let outer = assert_prefix_unary(&arena, ret_expr, UnaryOperatorKind::BitNot);
    let inner = assert_prefix_unary(&arena, outer, UnaryOperatorKind::Neg);
    assert_ident_expr(&arena, inner, "x");
}

// ---------------------------------------------------------------------------
// Statement tests
// ---------------------------------------------------------------------------

#[test]
fn test_parse_variable_declaration() {
    let (arena, defs) = parse_defs("fn test() { let x: i32 = 5; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let (ty, val) = assert_var_def(&arena, stmts[0], "x", false, true);
    assert_simple_type(&arena, ty, SimpleTypeKind::I32);
    assert_number(&arena, val.unwrap(), "5");
}

#[test]
fn test_parse_variable_declaration_no_init() {
    let (arena, defs) = parse_defs("fn test() { let x: i32; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let (ty, val) = assert_var_def(&arena, stmts[0], "x", false, false);
    assert_simple_type(&arena, ty, SimpleTypeKind::I32);
    assert!(val.is_none());
}

#[test]
fn test_parse_variable_mutable() {
    let (arena, defs) = parse_defs("fn test() { let mut x: i32 = 42; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let (ty, val) = assert_var_def(&arena, stmts[0], "x", true, true);
    assert_simple_type(&arena, ty, SimpleTypeKind::I32);
    assert_number(&arena, val.unwrap(), "42");
}

#[test]
fn test_parse_variable_immutable() {
    let (arena, defs) = parse_defs("fn test() { let x: i32 = 42; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let (ty, val) = assert_var_def(&arena, stmts[0], "x", false, true);
    assert_simple_type(&arena, ty, SimpleTypeKind::I32);
    assert_number(&arena, val.unwrap(), "42");
}

#[test]
fn test_parse_variable_mutable_no_init() {
    let (arena, defs) = parse_defs("fn test() { let mut y: i64; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let (ty, val) = assert_var_def(&arena, stmts[0], "y", true, false);
    assert_simple_type(&arena, ty, SimpleTypeKind::I64);
    assert!(val.is_none());
}

#[test]
fn test_parse_assignment() {
    let (arena, defs) = parse_defs("fn test() { x = 10; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let (left, right) = assert_assign(&arena, stmts[0]);
    assert_ident_expr(&arena, left, "x");
    assert_number(&arena, right, "10");
}

#[test]
fn test_parse_if_statement() {
    let (arena, defs) = parse_defs("fn test() { if (x > 0) { return x; } }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let (cond, then_block, else_block) = assert_if(&arena, stmts[0], false);
    assert!(else_block.is_none());

    let paren_inner = assert_parens(&arena, cond);
    let (left, right) = assert_binary(&arena, paren_inner, OperatorKind::Gt);
    assert_ident_expr(&arena, left, "x");
    assert_number(&arena, right, "0");

    let then_stmts = assert_block(&arena, then_block, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, then_stmts[0]);
    assert_ident_expr(&arena, ret_expr, "x");
}

#[test]
fn test_parse_if_else_statement() {
    let source = "fn test() -> i32 { if (x > 0) { return x; } else { return 0; } }";
    let (arena, defs) = parse_defs(source);
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let (cond, then_block, else_block) = assert_if(&arena, stmts[0], true);

    let paren_inner = assert_parens(&arena, cond);
    let (left, right) = assert_binary(&arena, paren_inner, OperatorKind::Gt);
    assert_ident_expr(&arena, left, "x");
    assert_number(&arena, right, "0");

    let then_stmts = assert_block(&arena, then_block, BlockKind::Regular, 1);
    let ret1 = assert_return(&arena, then_stmts[0]);
    assert_ident_expr(&arena, ret1, "x");

    let else_stmts = assert_block(&arena, else_block.unwrap(), BlockKind::Regular, 1);
    let ret2 = assert_return(&arena, else_stmts[0]);
    assert_number(&arena, ret2, "0");
}

#[test]
fn test_parse_loop_statement() {
    let (arena, defs) = parse_defs("fn test() { loop { break; } }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let (cond, loop_body) = assert_loop(&arena, stmts[0], false);
    assert!(cond.is_none());

    let loop_stmts = assert_block(&arena, loop_body, BlockKind::Regular, 1);
    assert_break(&arena, loop_stmts[0]);
}

#[test]
fn test_parse_for_loop() {
    let source = "fn test() { let mut i: i32 = 0; loop i < 10 { foo(i); i = i + 1; } }";
    let (arena, defs) = parse_defs(source);
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 2);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 2);

    let (ty, val) = assert_var_def(&arena, stmts[0], "i", true, true);
    assert_simple_type(&arena, ty, SimpleTypeKind::I32);
    assert_number(&arena, val.unwrap(), "0");

    let (cond, loop_body) = assert_loop(&arena, stmts[1], true);
    let (left, right) = assert_binary(&arena, cond.unwrap(), OperatorKind::Lt);
    assert_ident_expr(&arena, left, "i");
    assert_number(&arena, right, "10");

    let loop_stmts = assert_block(&arena, loop_body, BlockKind::Regular, 2);
    let call_expr = assert_expr_stmt(&arena, loop_stmts[0]);
    let call_args = assert_fn_call(&arena, call_expr, "foo", 1);
    assert_ident_expr(&arena, call_args[0], "i");

    let (assign_left, assign_right) = assert_assign(&arena, loop_stmts[1]);
    assert_ident_expr(&arena, assign_left, "i");
    let (add_left, add_right) = assert_binary(&arena, assign_right, OperatorKind::Add);
    assert_ident_expr(&arena, add_left, "i");
    assert_number(&arena, add_right, "1");
}

#[test]
fn test_parse_break_statement() {
    let (arena, defs) = parse_defs("fn test() { loop { break; } }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let (_, loop_body) = assert_loop(&arena, stmts[0], false);
    let loop_stmts = assert_block(&arena, loop_body, BlockKind::Regular, 1);
    assert_break(&arena, loop_stmts[0]);
}

#[test]
fn test_parse_assert_statement() {
    let (arena, defs) = parse_defs("fn test() { assert x > 0; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let expr = assert_assert_stmt(&arena, stmts[0]);
    let (left, right) = assert_binary(&arena, expr, OperatorKind::Gt);
    assert_ident_expr(&arena, left, "x");
    assert_number(&arena, right, "0");
}

#[test]
fn test_parse_assert_with_complex_expr() {
    let (arena, defs) = parse_defs("fn test() { assert a < b && b < c; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let expr = assert_assert_stmt(&arena, stmts[0]);
    // a < b && b < c  parses as (a < b) && (b < c)
    let (left_cmp, right_cmp) = assert_binary(&arena, expr, OperatorKind::And);
    let (a_expr, b1_expr) = assert_binary(&arena, left_cmp, OperatorKind::Lt);
    assert_ident_expr(&arena, a_expr, "a");
    assert_ident_expr(&arena, b1_expr, "b");
    let (b2_expr, c_expr) = assert_binary(&arena, right_cmp, OperatorKind::Lt);
    assert_ident_expr(&arena, b2_expr, "b");
    assert_ident_expr(&arena, c_expr, "c");
}

// ---------------------------------------------------------------------------
// Expression tests (non-binary/unary)
// ---------------------------------------------------------------------------

#[test]
fn test_parse_array_index_access() {
    let (arena, defs) = parse_defs("fn test() -> i32 { return arr[0]; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (array, index) = assert_array_index(&arena, ret_expr);
    assert_ident_expr(&arena, array, "arr");
    assert_number(&arena, index, "0");
}

#[test]
fn test_parse_array_index_expression() {
    let (arena, defs) = parse_defs("fn test() -> i32 { return arr[i + 1]; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (array, index) = assert_array_index(&arena, ret_expr);
    assert_ident_expr(&arena, array, "arr");
    let (left, right) = assert_binary(&arena, index, OperatorKind::Add);
    assert_ident_expr(&arena, left, "i");
    assert_number(&arena, right, "1");
}

#[test]
fn test_parse_function_call_no_args() {
    let (arena, defs) = parse_defs("fn test() { foo(); }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let call_expr = assert_expr_stmt(&arena, stmts[0]);
    let call_args = assert_fn_call(&arena, call_expr, "foo", 0);
    assert!(call_args.is_empty());
}

#[test]
fn test_parse_function_call_one_arg() {
    let (arena, defs) = parse_defs("fn test() { foo(42); }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let call_expr = assert_expr_stmt(&arena, stmts[0]);
    let call_args = assert_fn_call(&arena, call_expr, "foo", 1);
    assert_number(&arena, call_args[0], "42");
}

#[test]
fn test_parse_function_call_multiple_args() {
    let (arena, defs) = parse_defs("fn test() { add(1, 2); }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let call_expr = assert_expr_stmt(&arena, stmts[0]);
    let call_args = assert_fn_call(&arena, call_expr, "add", 2);
    assert_number(&arena, call_args[0], "1");
    assert_number(&arena, call_args[1], "2");
}

#[test]
fn test_parse_parenthesized_expression() {
    // (a + b) * c
    let (arena, defs) = parse_defs("fn test() -> i32 { return (a + b) * c; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Mul);
    assert_ident_expr(&arena, right, "c");
    let inner = assert_parens(&arena, left);
    let (a_expr, b_expr) = assert_binary(&arena, inner, OperatorKind::Add);
    assert_ident_expr(&arena, a_expr, "a");
    assert_ident_expr(&arena, b_expr, "b");
}

#[test]
fn test_parse_bool_literal_true() {
    let (arena, defs) = parse_defs("fn test() -> bool { return true; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    assert_bool(&arena, ret_expr, true);
}

#[test]
fn test_parse_bool_literal_false() {
    let (arena, defs) = parse_defs("fn test() -> bool { return false; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    assert_bool(&arena, ret_expr, false);
}

#[test]
fn test_parse_string_literal() {
    let (arena, defs) = parse_defs(r#"fn test() -> str { return "hello"; }"#);
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    // The parser stores the string with surrounding quotes
    assert_string_literal(&arena, ret_expr, "\"hello\"");
}

#[test]
fn test_parse_array_literal_empty() {
    let (arena, defs) = parse_defs("fn test() -> [i32; 0] { return []; }");
    let (_, ret, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);

    let (elem_ty, size) = assert_array_type(&arena, ret.unwrap());
    assert_simple_type(&arena, elem_ty, SimpleTypeKind::I32);
    assert_number(&arena, size, "0");

    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let elems = assert_array_literal(&arena, ret_expr, 0);
    assert!(elems.is_empty());
}

#[test]
fn test_parse_array_literal_values() {
    let (arena, defs) = parse_defs("fn test() -> [i32; 3] { return [1, 2, 3]; }");
    let (_, ret, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);

    let (elem_ty, size) = assert_array_type(&arena, ret.unwrap());
    assert_simple_type(&arena, elem_ty, SimpleTypeKind::I32);
    assert_number(&arena, size, "3");

    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let elems = assert_array_literal(&arena, ret_expr, 3);
    assert_number(&arena, elems[0], "1");
    assert_number(&arena, elems[1], "2");
    assert_number(&arena, elems[2], "3");
}

#[test]
fn test_parse_member_access() {
    let (arena, defs) = parse_defs("fn test() -> i32 { return obj.field; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let base = assert_member_access(&arena, ret_expr, "field");
    assert_ident_expr(&arena, base, "obj");
}

#[test]
fn test_parse_chained_member_access() {
    // obj.field.subfield: parser creates QualifiedName("obj", "field") for the
    // first two segments, then MemberAccess(.subfield) on top.
    let (arena, defs) = parse_defs("fn test() -> i32 { return obj.field.subfield; }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);

    // Outer: MemberAccess { expr, name: "subfield" }
    let base = assert_member_access(&arena, ret_expr, "subfield");

    // Inner: Expr::Type(QualifiedName { qualifier: "obj", name: "field" })
    let qualified_ty = assert_type_expr(&arena, base);
    let ty = &arena[qualified_ty];
    let TypeNode::QualifiedName { qualifier, name } = &ty.kind else {
        panic!("expected TypeNode::QualifiedName, got {:?}", ty.kind);
    };
    assert_eq!(arena[*qualifier].name, "obj");
    assert_eq!(arena[*name].name, "field");
}

#[test]
fn test_parse_struct_expression() {
    let (arena, defs) = parse_defs("fn test() -> Point { return Point { x: 1, y: 2 }; }");
    let (_, ret, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    assert_custom_type(&arena, ret.unwrap(), "Point");

    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    let fields = assert_struct_literal(&arena, ret_expr, "Point", 2);
    assert_eq!(fields[0].0, "x");
    assert_number(&arena, fields[0].1, "1");
    assert_eq!(fields[1].0, "y");
    assert_number(&arena, fields[1].1, "2");
}

// ---------------------------------------------------------------------------
// Type tests
// ---------------------------------------------------------------------------

#[test]
fn test_parse_generic_type() {
    let (arena, defs) = parse_defs("fn test() -> Array i32' {}");
    let (_, ret, _) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 0);
    let params = assert_generic_type(&arena, ret.unwrap(), "Array", 1);
    assert_eq!(arena[params[0]].name, "i32");
}

#[test]
fn test_parse_function_type_param() {
    let (arena, defs) = parse_defs("fn test(func: sf) {}");
    let (args, _, _) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 1, false, 0);
    let ty = assert_named_arg(&arena, &args[0], "func", false);
    assert_custom_type(&arena, ty, "sf");
}

// ---------------------------------------------------------------------------
// External function definitions
// ---------------------------------------------------------------------------

#[test]
fn test_parse_external_function() {
    let (arena, defs) =
        parse_defs("external fn sorting_function(Address, Address) -> Address;");
    assert_eq!(defs.len(), 1);

    let (args, ret) = assert_extern_function_def(
        &arena,
        defs[0],
        "sorting_function",
        Visibility::Private,
        2,
        true,
    );

    let ty0 = assert_type_only_arg(&arena, &args[0]);
    assert_custom_type(&arena, ty0, "Address");
    let ty1 = assert_type_only_arg(&arena, &args[1]);
    assert_custom_type(&arena, ty1, "Address");

    assert_custom_type(&arena, ret.unwrap(), "Address");
}

// ---------------------------------------------------------------------------
// Type alias definitions
// ---------------------------------------------------------------------------

#[test]
fn test_parse_type_alias() {
    let (arena, defs) = parse_defs("type sf = sorting_function;");
    assert_eq!(defs.len(), 1);

    let ty = assert_type_alias_def(&arena, defs[0], "sf", Visibility::Private);
    assert_custom_type(&arena, ty, "sorting_function");
}

// ---------------------------------------------------------------------------
// Block tests
// ---------------------------------------------------------------------------

#[test]
fn test_parse_empty_block() {
    let (arena, defs) = parse_defs("fn test() {}");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 0);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 0);
    assert!(stmts.is_empty());
}

#[test]
fn test_parse_block_multiple_statements() {
    let source = "fn test() { let x: i32 = 1; let y: i32 = 2; return x + y; }";
    let (arena, defs) = parse_defs(source);
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 3);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 3);

    let (ty0, val0) = assert_var_def(&arena, stmts[0], "x", false, true);
    assert_simple_type(&arena, ty0, SimpleTypeKind::I32);
    assert_number(&arena, val0.unwrap(), "1");

    let (ty1, val1) = assert_var_def(&arena, stmts[1], "y", false, true);
    assert_simple_type(&arena, ty1, SimpleTypeKind::I32);
    assert_number(&arena, val1.unwrap(), "2");

    let ret_expr = assert_return(&arena, stmts[2]);
    let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Add);
    assert_ident_expr(&arena, left, "x");
    assert_ident_expr(&arena, right, "y");
}

#[test]
fn test_parse_nested_blocks() {
    let (arena, defs) = parse_defs("fn test() { { let x: i32 = 1; } }");
    let (_, _, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, false, 1);
    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);

    let inner_block = assert_block_stmt(&arena, stmts[0], BlockKind::Regular, 1);
    let inner_stmts = assert_block(&arena, inner_block, BlockKind::Regular, 1);
    let (ty, val) = assert_var_def(&arena, inner_stmts[0], "x", false, true);
    assert_simple_type(&arena, ty, SimpleTypeKind::I32);
    assert_number(&arena, val.unwrap(), "1");
}

// ---------------------------------------------------------------------------
// Comments (should be transparent to AST)
// ---------------------------------------------------------------------------

#[test]
fn test_parse_comments() {
    let source = r#"// This is a comment
fn test() -> i32 {
    // Another comment
    return 42;
}"#;
    let (arena, defs) = parse_defs(source);
    assert_eq!(defs.len(), 1);

    let (_, ret, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    assert_simple_type(&arena, ret.unwrap(), SimpleTypeKind::I32);

    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    assert_number(&arena, ret_expr, "42");
}

#[test]
fn test_parse_multiline_comments() {
    let source = r#"// This is a
//   multiline comment
fn test() -> i32 {
    return 42;
}"#;
    let (arena, defs) = parse_defs(source);
    let (_, ret, body) =
        assert_function_def(&arena, defs[0], "test", Visibility::Private, 0, true, 1);
    assert_simple_type(&arena, ret.unwrap(), SimpleTypeKind::I32);

    let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
    let ret_expr = assert_return(&arena, stmts[0]);
    assert_number(&arena, ret_expr, "42");
}

// ---------------------------------------------------------------------------
// Error recovery coverage tests
// ---------------------------------------------------------------------------

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
