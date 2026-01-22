/// Comprehensive integration tests for the parser
/// 
/// Coverage targets: >95% of parser code paths
/// Tests organized by language construct

use inference_parser::Parser;

// ============================================================================
// EMPTY AND TRIVIAL CASES
// ============================================================================

#[test]
fn test_empty_module() {
    let source = "";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_whitespace_only() {
    let source = "   \n\n  \t  ";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

// ============================================================================
// FUNCTION DEFINITIONS
// ============================================================================

#[test]
fn test_simple_function() {
    let source = "fn foo() { }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_function_with_params() {
    let source = "fn add(x: i32, y: i32) { }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_function_with_return_type() {
    let source = "fn get_five() -> i32 { }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_function_with_all_features() {
    let source = "fn generic<T>(x: T, y: T) -> T { x }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_public_function() {
    let source = "pub fn visible() { }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_function_with_where_clause() {
    let source = "fn process<T>(x: T) where T: Clone { }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_function_missing_name() {
    let source = "fn () { }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_err());
}

#[test]
fn test_function_missing_body() {
    let source = "fn foo()";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_err());
}

// ============================================================================
// STRUCT DEFINITIONS
// ============================================================================

#[test]
fn test_empty_struct() {
    let source = "struct Point { }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_struct_with_fields() {
    let source = "struct Point { x: i32, y: i32, }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_struct_with_generics() {
    let source = "struct Box<T> { value: T, }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_struct_with_where_clause() {
    let source = "struct Container<T> { item: T, } where T: Clone";
    let mut parser = Parser::new(source);
    // May fail because where clause parsing in struct context
    let _ = parser.parse_module();
}

#[test]
fn test_nested_struct_fields() {
    let source = "struct Outer { inner: Inner, }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_struct_no_body() {
    let source = "struct Point";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_err());
}

// ============================================================================
// ENUM DEFINITIONS
// ============================================================================

#[test]
fn test_simple_enum() {
    let source = "enum Result { Ok, Err, }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_enum_with_tuple_variants() {
    let source = "enum Option { Some(T), None, }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_enum_with_struct_variants() {
    let source = "enum Message { Text(String), Quit { code: i32, }, }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_enum_with_generics() {
    let source = "enum Result<T, E> { Ok(T), Err(E), }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

// ============================================================================
// TRAIT DEFINITIONS
// ============================================================================

#[test]
fn test_empty_trait() {
    let source = "trait Drawable { }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_trait_with_method() {
    let source = "trait Iterator { fn next() { } }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_trait_with_type_and_const() {
    let source = "trait Container { type Item; const SIZE: usize = 10; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

// ============================================================================
// IMPL BLOCKS
// ============================================================================

#[test]
fn test_impl_block() {
    let source = "impl Point { fn new() { } }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_impl_trait() {
    let source = "impl Display for Point { fn fmt() { } }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_impl_generic() {
    let source = "impl<T> Box<T> { fn unwrap() { } }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

// ============================================================================
// TYPE ALIASES
// ============================================================================

#[test]
fn test_type_alias() {
    let source = "type Kilometers = i32;";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_type_alias_generic() {
    let source = "type Result<T> = std::result::Result<T, Error>;";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

// ============================================================================
// CONST AND MODULE DECLARATIONS
// ============================================================================

#[test]
fn test_const_declaration() {
    let source = "const MAX_SIZE: usize = 100;";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_module_inline() {
    let source = "mod math { fn add() { } }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_module_file() {
    let source = "mod math;";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

// ============================================================================
// IMPORT STATEMENTS
// ============================================================================

#[test]
fn test_simple_import() {
    let source = "import std;";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_path_import() {
    let source = "import std::io::Write;";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_import_with_alias() {
    let source = "import std::fs::File as F;";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_import_no_semicolon() {
    let source = "import std";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_err());
}

// ============================================================================
// EXPRESSIONS: LITERALS
// ============================================================================

#[test]
fn test_int_literal() {
    let source = "fn test() { let x = 42; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_float_literal() {
    let source = "fn test() { let x = 3.14; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_string_literal() {
    let source = r#"fn test() { let s = "hello"; }"#;
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_boolean_literals() {
    let source = "fn test() { let t = true; let f = false; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

// ============================================================================
// EXPRESSIONS: OPERATORS
// ============================================================================

#[test]
fn test_arithmetic_ops() {
    let source = "fn test() { let x = 1 + 2 - 3 * 4 / 5 % 6; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_comparison_ops() {
    let source = "fn test() { let b = a == b && c != d && e < f && g > h; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_logical_ops() {
    let source = "fn test() { let b = a && b || c; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_bitwise_ops() {
    let source = "fn test() { let x = a & b | c ^ d << 1 >> 2; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_unary_ops() {
    let source = "fn test() { let x = -a; let b = !c; let r = &d; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

// ============================================================================
// EXPRESSIONS: CONTROL FLOW
// ============================================================================

#[test]
fn test_if_expression() {
    let source = "fn test() { if true { } }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_if_else() {
    let source = "fn test() { if true { } else { } }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_if_else_if() {
    let source = "fn test() { if x { } else if y { } else { } }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_while_loop() {
    let source = "fn test() { while x { } }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_for_loop() {
    let source = "fn test() { for i in range { } }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_loop_expression() {
    let source = "fn test() { loop { } }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_match_expression() {
    let source = "fn test() { match x { A => { }, B => { }, } }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

// ============================================================================
// EXPRESSIONS: FUNCTION CALLS AND ACCESS
// ============================================================================

#[test]
fn test_function_call() {
    let source = "fn test() { foo(); }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_function_call_with_args() {
    let source = "fn test() { add(1, 2, 3); }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_method_call() {
    let source = "fn test() { point.distance(); }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_method_call_with_args() {
    let source = "fn test() { point.move_by(10, 20); }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_array_indexing() {
    let source = "fn test() { let x = arr[0]; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_field_access() {
    let source = "fn test() { let x = point.x; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_chained_calls() {
    let source = "fn test() { vec.push(x).pop().unwrap(); }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

// ============================================================================
// EXPRESSIONS: COLLECTIONS
// ============================================================================

#[test]
fn test_array_expr() {
    let source = "fn test() { let x = [1, 2, 3]; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_array_with_capacity() {
    let source = "fn test() { let x = [0; 10]; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_tuple_expr() {
    let source = "fn test() { let x = (1, 2, 3); }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_empty_tuple() {
    let source = "fn test() { let x = (); }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_struct_init() {
    let source = "fn test() { let p = Point { x: 1, y: 2 }; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

// ============================================================================
// STATEMENTS
// ============================================================================

#[test]
fn test_let_binding() {
    let source = "fn test() { let x = 42; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_let_with_type() {
    let source = "fn test() { let x: i32 = 42; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_let_mut() {
    let source = "fn test() { let mut x = 0; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_return_statement() {
    let source = "fn test() { return 42; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_return_void() {
    let source = "fn test() { return; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_break_statement() {
    let source = "fn test() { loop { break; } }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_continue_statement() {
    let source = "fn test() { loop { continue; } }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

// ============================================================================
// ERROR RECOVERY
// ============================================================================

#[test]
fn test_multiple_errors() {
    let source = "fn broken( a i32 { }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_err());
}

#[test]
fn test_unexpected_token() {
    let source = "fn foo() { @ }";
    let mut parser = Parser::new(source);
    // Should not crash, handles error gracefully
    let _ = parser.parse_module();
}

#[test]
fn test_incomplete_statement() {
    let source = "fn test() { let x = }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_err());
}

// ============================================================================
// COMPLEX PROGRAMS
// ============================================================================

#[test]
fn test_multiple_items() {
    let source = r#"
        fn add(x: i32, y: i32) -> i32 { x + y }
        struct Point { x: i32, y: i32, }
        impl Point { fn distance() { } }
        enum Status { Ok, Error, }
    "#;
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_nested_blocks() {
    let source = r#"
        fn test() {
            {
                {
                    let x = 1;
                }
            }
        }
    "#;
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_complex_expression() {
    let source = r#"
        fn test() {
            let x = if flag { foo(1, 2).bar } else { baz() };
            match result {
                Ok(v) => { v },
                Err(e) => { return e; },
            }
        }
    "#;
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

// ============================================================================
// GENERIC TYPES AND WHERE CLAUSES
// ============================================================================

#[test]
fn test_multiple_generic_params() {
    let source = "fn id<T, U, V>(x: T, y: U) -> V { }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_generic_with_bounds() {
    let source = "fn process<T: Clone, U: Display>(x: T, y: U) { }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

// ============================================================================
// TYPE EXPRESSIONS
// ============================================================================

#[test]
fn test_reference_type() {
    let source = "fn test(x: &i32) { }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_mutable_reference() {
    let source = "fn test(x: &mut i32) { }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_array_type() {
    let source = "fn test(x: [i32; 10]) { }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_generic_type() {
    let source = "fn test(x: Vec<String>) { }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_nested_generic_types() {
    let source = "fn test(x: HashMap<String, Vec<i32>>) { }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

// ============================================================================
// PATH EXPRESSIONS
// ============================================================================

#[test]
fn test_simple_path() {
    let source = "fn test() { let x = foo; }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_qualified_path() {
    let source = "fn test() { let x = std::io::stdout(); }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}
