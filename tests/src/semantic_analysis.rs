use inference::{analyze, parse, type_check};

fn try_analyze(source: &str) -> anyhow::Result<()> {
    let arena = parse(source)?;
    let typed_context = type_check(arena)?;
    analyze(&typed_context)
}

#[test]
fn test_uzumaki_variable_declaration_succeeds() {
    let source = r#"
    fn test() {
        let x: i32 = @;
        let y: u8 = @;
    }
    "#;
    let result = try_analyze(source);
    assert!(
        result.is_ok(),
        "Uzumaki in variable declaration should succeed"
    );
}

#[test]
fn test_uzumaki_in_assignment_fails() {
    let source = r#"
    fn test() {
        let x: i32 = 0;
        x = @;
    }
    "#;
    let result = try_analyze(source);
    assert!(result.is_err(), "Uzumaki in assignment should fail");
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("uzumaki can only be used in variable declaration statements")
    );
}

#[test]
fn test_uzumaki_in_return_fails() {
    let source = r#"
    fn test() -> i32 {
        return @;
    }
    "#;
    let result = try_analyze(source);
    assert!(result.is_err(), "Uzumaki in return should fail");
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("uzumaki can only be used in variable declaration statements")
    );
}

#[test]
fn test_uzumaki_in_expression_fails() {
    let source = r#"
    fn test() {
        let x: i32 = 1 + @;
    }
    "#;
    let result = try_analyze(source);
    assert!(result.is_err(), "Uzumaki in expression should fail");
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("uzumaki can only be used in variable declaration statements")
    );
}
