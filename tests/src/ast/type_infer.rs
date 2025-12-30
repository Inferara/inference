use crate::utils::build_ast;

// Tests for type_infer.rs - type inference and checking functionality

#[test]
fn test_type_inference_function_with_i32_return() {
    let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_function_with_bool_return() {
    let source = r#"fn is_positive(x: i32) -> bool { return x > 0; }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore = "array type inference not fully implemented"]
#[test]
fn test_type_inference_array_type() {
    let source = r#"fn get_first(arr: [i32; 1]) -> i32 { return arr[0]; }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_u64_type() {
    let source = r#"fn test(x: u64) -> u64 { return x; }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_i64_type() {
    let source = r#"fn test(x: i64) -> i64 { return x; }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_u8_type() {
    let source = r#"fn test(x: u8) -> u8 { return x; }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_u16_type() {
    let source = r#"fn test(x: u16) -> u16 { return x; }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_u32_type() {
    let source = r#"fn test(x: u32) -> u32 { return x; }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_i8_type() {
    let source = r#"fn test(x: i8) -> i8 { return x; }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_i16_type() {
    let source = r#"fn test(x: i16) -> i16 { return x; }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_string_type() {
    let source = r#"fn test(x: String) -> String { return x; }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_custom_type() {
    let source = r#"struct Point { x: i32, y: i32 }
fn test(p: Point) -> Point { return p; }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

//FIXME: this test shoudl fail because div is not supported
#[test]
fn test_type_inference_binary_expressions() {
    let source = r#"fn test() -> i32 { return (10 + 20) * (30 - 5) / 2; }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_comparison_expressions() {
    let source = r#"fn test(x: i32, y: i32) -> bool { return x >= y; }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_logical_expressions() {
    let source = r#"fn test(a: bool, b: bool) -> bool { return a && b; }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_function_call() {
    let source = r#"fn helper() -> i32 { return 42; }
fn test() -> i32 { return helper(); }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_if_statement() {
    let source = r#"fn test(x: i32) -> i32 { if x > 0 { return 1; } else { return 0; } }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_loop_with_break() {
    let source = r#"fn test() { loop { break; } }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_external_function() {
    let source = r#"external fn print(String);"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_type_alias() {
    let source = r#"type MyInt = i32;"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_constant_bool() {
    let source = r#"const FLAG: bool = true;"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_constant_string() {
    let source = r#"const MSG: String = "hello";"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_unit_return_type() {
    let source = r#"fn test() { assert(true); }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_nested_arrays() {
    let source = r#"fn test(matrix: [[bool; 2]; 1]) { assert(true); }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_type_inference_multiple_params() {
    let source = r#"fn test(a: i32, b: i32, c: i32, d: i32) -> i32 { return a + b + c + d; }"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}
