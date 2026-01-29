/// Minimal parser API tests
/// 
/// Tests focus on exercising the Parser public API methods:
/// - Parser::new()
/// - at(SyntaxKind)
/// - bump()
/// - expect(SyntaxKind)
/// - at_eof()
/// - current()
/// - error()
/// - parse_module()

use inference_parser::{Parser, SyntaxKind};

// ============================================================================
// PARSER CONSTRUCTION
// ============================================================================

#[test]
fn parser_new_empty() {
    let _parser = Parser::new("");
}

#[test]
fn parser_new_with_tokens() {
    let _parser = Parser::new("fn foo() {}");
}

// ============================================================================
// AT() METHOD - Check current token kind
// ============================================================================

#[test]
fn at_returns_true_for_matching_kind() {
    let parser = Parser::new("fn");
    assert!(parser.at(SyntaxKind::FN));
}

#[test]
fn at_returns_false_for_non_matching_kind() {
    let parser = Parser::new("fn");
    assert!(!parser.at(SyntaxKind::STRUCT));
}

#[test]
fn at_eof_on_empty() {
    let parser = Parser::new("");
    assert!(parser.at_eof());
}

// ============================================================================
// BUMP() METHOD - Advance position
// ============================================================================

#[test]
fn bump_advances_position() {
    let mut parser = Parser::new("fn foo");
    assert!(parser.at(SyntaxKind::FN));
    parser.bump();
    assert!(!parser.at(SyntaxKind::FN));
}

#[test]
fn bump_on_eof_does_not_panic() {
    let mut parser = Parser::new("");
    parser.bump(); // Should not panic
    parser.bump();
    parser.bump();
}

// ============================================================================
// CURRENT() METHOD - Get current token kind
// ============================================================================

#[test]
fn current_returns_current_kind() {
    let parser = Parser::new("fn");
    assert_eq!(parser.current(), SyntaxKind::FN);
}

#[test]
fn current_returns_eof_when_exhausted() {
    let parser = Parser::new("");
    assert_eq!(parser.current(), SyntaxKind::EOF);
}

// ============================================================================
// EXPECT() METHOD - Expect and consume specific kind
// ============================================================================

#[test]
fn expect_succeeds_on_match() {
    let mut parser = Parser::new("fn struct");
    assert!(parser.expect(SyntaxKind::FN));
    assert!(parser.at(SyntaxKind::STRUCT));
}

#[test]
fn expect_fails_on_mismatch() {
    let mut parser = Parser::new("fn");
    assert!(!parser.expect(SyntaxKind::STRUCT));
}

// ============================================================================
// AT_EOF() METHOD - Check if at end of input
// ============================================================================

#[test]
fn at_eof_true_when_empty() {
    let parser = Parser::new("");
    assert!(parser.at_eof());
}

#[test]
fn at_eof_false_with_tokens() {
    let parser = Parser::new("fn");
    assert!(!parser.at_eof());
}

#[test]
fn at_eof_true_after_consuming_all() {
    let mut parser = Parser::new("fn");
    parser.bump();
    assert!(parser.at_eof());
}

// ============================================================================
// ERROR() METHOD - Collect errors
// ============================================================================

#[test]
fn error_method_collects_errors() {
    let mut parser = Parser::new("invalid");
    parser.error("test error");
    let errors = parser.errors();
    assert!(!errors.is_empty());
}

#[test]
fn multiple_errors_collected() {
    let mut parser = Parser::new("invalid");
    parser.error("error 1");
    parser.error("error 2");
    let errors = parser.errors();
    assert_eq!(errors.len(), 2);
}

// ============================================================================
// PARSE_MODULE() METHOD - Main parsing API
// ============================================================================

#[test]
fn parse_module_empty_input() {
    let mut parser = Parser::new("");
    assert!(parser.parse_module().is_ok());
}

#[test]
fn parse_module_simple_function() {
    let mut parser = Parser::new("fn foo() {}");
    assert!(parser.parse_module().is_ok());
}

#[test]
fn parse_module_struct_definition() {
    let mut parser = Parser::new("struct Foo { x: i32 }");
    assert!(parser.parse_module().is_ok());
}

#[test]
fn parse_module_nested_braces() {
    let mut parser = Parser::new("fn f() { if true { let x = { 1 + 2 }; } }");
    assert!(parser.parse_module().is_ok());
}

#[test]
fn parse_module_multiple_items() {
    let mut parser = Parser::new("fn a() {} fn b() {} struct C {}");
    assert!(parser.parse_module().is_ok());
}

#[test]
fn parse_module_does_not_panic_on_garbage() {
    let mut parser = Parser::new("@#$%^&*()");
    let _ = parser.parse_module(); // Should not panic
}

// ============================================================================
// AT_CONTEXTUAL_KW() METHOD - Check contextual keywords
// ============================================================================

#[test]
fn at_contextual_kw_with_identifier() {
    let parser = Parser::new("identifier");
    assert!(parser.at_contextual_kw("identifier"));
}

#[test]
fn at_contextual_kw_with_keyword() {
    let parser = Parser::new("fn");
    assert!(!parser.at_contextual_kw("fn"));
}

// ============================================================================
// INTEGRATION - Parser state consistency
// ============================================================================

#[test]
fn parser_state_remains_consistent() {
    let mut parser = Parser::new("fn foo struct bar");
    
    // Initial state
    assert_eq!(parser.current(), SyntaxKind::FN);
    assert!(!parser.at_eof());
    
    // After bump
    parser.bump();
    assert_ne!(parser.current(), SyntaxKind::FN);
    
    // Expect works
    let result = parser.expect(SyntaxKind::STRUCT);
    
    // State is consistent
    if result {
        assert!(parser.at(SyntaxKind::STRUCT) || parser.at_eof());
    }
}

#[test]
fn parser_complete_sequence() {
    let mut parser = Parser::new("fn test() {}");
    
    assert!(parser.at(SyntaxKind::FN));
    parser.bump();
    
    assert!(!parser.at_eof());
    
    let current = parser.current();
    assert_ne!(current, SyntaxKind::EOF);
    
    while !parser.at_eof() {
        parser.bump();
    }
    
    assert!(parser.at_eof());
}
