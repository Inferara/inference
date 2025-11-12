use crate::utils::build_ast;

// Extended comprehensive tests for advanced AST features

#[ignore = "spec/total definition not supported"]
#[test]
fn test_parse_spec_definition() {
    let source = r#"
total fn sum(items: []i32) -> i32 {
    filter {
        return >= 0;
    }
    let result = 0;
    for item in items {
        result = result + item;
    }
    return result;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore = "forall with empty return not parsing correctly"]
#[test]
fn test_parse_function_with_forall() {
    let source = r#"
fn test() -> () forall {
    return;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_function_with_assume() {
    let source = r#"
fn test() -> () forall {
    assume {
        a = valid_Address();
    }
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore = "filter block not supported"]
#[test]
fn test_parse_function_with_filter() {
    let source = r#"
total fn add(a: i32, b: i32) -> i32 {
    filter {
        return >= 0;
    }
    return a + b;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore = "constructor function keyword not supported"]
#[test]
fn test_parse_constructor_function() {
    let source = r#"
constructor fn create_spec() -> SpecType {
    return SpecType { value: 42 };
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_qualified_type() {
    let source = r#"
use collections::HashMap;
fn test() -> HashMap {
    return HashMap {};
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_typeof_expression() {
    let source = r#"
external fn sorting_function(Address, Address) -> Address;
type sf = typeof(sorting_function);
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore = "let outside function not supported"]
#[test]
fn test_parse_typeof_with_identifier() {
    let source = r#"
const x: i32 = 5;
type mytype = typeof(x);
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore = "method call syntax not fully supported"]
#[test]
fn test_parse_method_call_expression() {
    let source = r#"
fn test() {
    let result = object.method();
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore = "method call with args not fully supported"]
#[test]
fn test_parse_method_call_with_args() {
    let source = r#"
fn test() {
    let result = object.method(arg1, arg2);
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_struct_with_multiple_fields() {
    let source = r#"
struct Point {
    x: i32,
    y: i32,
    z: i32,
    label: String
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_enum_with_variants() {
    let source = r#"
enum Color {
    Red,
    Green,
    Blue,
    Custom
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore = "multiline struct expressions not supported"]
#[test]
fn test_parse_complex_struct_expression() {
    let source = r#"
fn test() {
    let point = Point { 
        x: 10, 
        y: 20,
        z: 30,
        label: "origin"
    };
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore = "nested struct expressions not supported"]
#[test]
fn test_parse_nested_struct_expression() {
    let source = r#"
fn test() {
    let rect = Rectangle {
        top_left: Point { x: 0, y: 0 },
        bottom_right: Point { x: 100, y: 100 }
    };
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_complex_binary_expression() {
    let source = r#"
fn test() -> i32 {
    return (a + b) * (c - d) / e;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_bitwise_and() {
    let source = r#"
fn test() -> i32 {
    return a & b;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_bitwise_or() {
    let source = r#"
fn test() -> i32 {
    return a | b;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_bitwise_xor() {
    let source = r#"
fn test() -> i32 {
    return a ^ b;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_left_shift() {
    let source = r#"
fn test() -> i32 {
    return a << 2;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_right_shift() {
    let source = r#"
fn test() -> i32 {
    return a >> 2;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_nested_function_calls() {
    let source = r#"
fn test() -> i32 {
    return foo(bar(baz(x)));
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_if_elseif_else() {
    let source = r#"
fn test(x: i32) -> i32 {
    if x > 10 {
        return 1;
    } else if x > 5 {
        return 2;
    } else {
        return 3;
    }
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_nested_if_statements() {
    let source = r#"
fn test(x: i32, y: i32) -> i32 {
    if x > 0 {
        if y > 0 {
            return 1;
        } else {
            return 2;
        }
    } else {
        return 3;
    }
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_use_from_directive() {
    let source = r#"
use std::collections::HashMap from "std";
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_builder_multiple_source_files() {
    let source = r#"
fn test1() -> i32 { return 1; }
fn test2() -> i32 { return 2; }
fn test3() -> i32 { return 3; }
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
    assert_eq!(source_files[0].definitions.len(), 3);
}

#[ignore = "numeric literals parsing issue"]
#[test]
fn test_parse_multiple_variable_declarations() {
    let source = r#"
fn test() {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore = "type inference syntax not supported"]
#[test]
fn test_parse_variable_with_type_inference() {
    let source = r#"
fn test() {
    let x := 42;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore = "struct literals in const not supported"]
#[test]
fn test_parse_multiple_definitions() {
    let source = r#"
struct Point { x: i32, y: i32 }
enum Color { Red, Green, Blue }
fn create_point(x: i32, y: i32) -> Point {
    return Point { x: x, y: y };
}
type Coordinate = Point;
const ORIGIN: Point = Point { x: 0, y: 0 };
external fn print(String);
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
    assert_eq!(source_files[0].definitions.len(), 6);
}

#[test]
fn test_parse_assignment_to_member() {
    let source = r#"
fn test() {
    point.x = 10;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_assignment_to_array_index() {
    let source = r#"
fn test() {
    arr[0] = 42;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore = "nested array literals not supported"]
#[test]
fn test_parse_array_of_arrays() {
    let source = r#"
fn test() {
    let matrix = [[1, 2], [3, 4]];
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore = "self parameter not yet implemented"]
#[test]
fn test_parse_function_with_self_param() {
    let source = r#"
fn method(self, x: i32) -> i32 {
    return x;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[test]
fn test_parse_function_with_ignore_param() {
    let source = r#"
fn test(_: i32) -> i32 {
    return 0;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore = "empty array literal syntax not supported"]
#[test]
fn test_parse_empty_array_literal() {
    let source = r#"
fn test() {
    let arr = [];
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore = "ignore parameter not fully implemented"]
#[test]
fn test_parse_function_with_mixed_params() {
    let source = r#"
fn test(a: i32, _: i32, c: i32) -> i32 {
    return a + c;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}

#[ignore = "bitwise NOT not yet supported"]
#[test]
fn test_parse_bitwise_not() {
    let source = r#"
fn test() -> i32 {
    return ~a;
}
"#;
    let ast = build_ast(source.to_string());
    let source_files = &ast.source_files;
    assert_eq!(source_files.len(), 1);
}
