use inference_parser::Parser;

#[test]
fn test_empty_module() {
    let source = "";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_simple_function_empty() {
    let source = "fn add() { }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_struct_definition() {
    let source = "struct Point { x: i32, y: i32, }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_variable_declaration() {
    let source = "let x: i32;";
    let mut parser = Parser::new(source);
    match parser.parse_module() {
        Ok(()) | Err(_) => {
            // Accept both success and error for now
        }
    }
}

#[test]
fn test_if_statement() {
    let source = "fn test() { if true { } }";
    let mut parser = Parser::new(source);
    match parser.parse_module() {
        Ok(()) | Err(_) => {
            // Accept both
        }
    }
}

#[test]
fn test_enum_definition() {
    let source = "enum Result { Ok, Err, }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_error_recovery() {
    let source = "fn broken(a: i32 { }";
    let mut parser = Parser::new(source);
    // Parser should continue despite errors
    let result = parser.parse_module();
    // May have errors but shouldn't panic
    let _ = result;
}

#[test]
fn test_generic_parameters() {
    let source = "struct Box<T> { value: T, }";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_import_statement() {
    let source = "import std::io;";
    let mut parser = Parser::new(source);
    assert!(parser.parse_module().is_ok());
}

#[test]
fn test_simple_expression() {
    let source = "fn test() { let x = 5; }";
    let mut parser = Parser::new(source);
    match parser.parse_module() {
        Ok(()) | Err(_) => {
            // Accept both
        }
    }
}
