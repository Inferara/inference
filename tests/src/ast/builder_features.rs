use crate::utils::{
    assert_constant_def, assert_function_signature, assert_variable_def, build_ast, try_build_ast,
};
use inference_ast::builder::Builder;
use inference_ast::nodes::{
    AstNode, Definition, Expression, Literal, OperatorKind, Statement, Visibility,
};

// --- Parse Error Detection Tests ---

#[test]
fn test_check_treesitter_errors() {
    fn check_source(source: &str) -> (bool, String) {
        let inference_language = tree_sitter_inference::language();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&inference_language).unwrap();
        let tree = parser.parse(source, None).unwrap();
        (tree.root_node().has_error(), tree.root_node().to_sexp())
    }

    let source = r#"fn test() { return >= 0; }"#;
    let (has_error, sexp) = check_source(source);
    println!("Source: {}", source);
    println!("has_error: {}", has_error);
    println!("Tree: {}", sexp);
}

#[test]
fn test_invalid_syntax_return_missing_left_operand_is_rejected() {
    let source = r#"fn test() { return >= 0; }"#;
    let result = std::panic::catch_unwind(|| {
        build_ast(source.to_string());
    });
    assert!(
        result.is_err(),
        "Invalid syntax 'return >= 0;' should panic during parsing"
    );
}

#[test]
fn test_invalid_syntax_in_forall_block_is_rejected() {
    let source =
        r#"fn sum(items: [i32; 10]) -> i32 { forall { return >= 0; } let result: i32 = 0; }"#;
    let result = std::panic::catch_unwind(|| {
        build_ast(source.to_string());
    });
    assert!(
        result.is_err(),
        "Invalid syntax inside forall block should panic during parsing"
    );
}

// FIXME: Missing semicolons are marked as MISSING nodes by tree-sitter, not ERROR nodes.
// Our current error detection only catches ERROR nodes. To properly detect missing
// semicolons, we would need to also check for MISSING nodes, but that requires care
// to avoid false positives (some MISSING nodes are intentional grammar recovery).
// For now, this test documents the current (unfixed) behavior where missing semicolons
// are silently accepted.
#[test]
fn test_missing_semicolon_not_yet_detected() {
    let source = r#"fn test() { let x: i32 = 5 }"#;
    let result = std::panic::catch_unwind(|| {
        build_ast(source.to_string());
    });
    // FIXME: This should fail (is_err()), but currently passes because
    // MISSING nodes are not detected. Update this assertion when the
    // issue is fixed.
    assert!(
        result.is_ok(),
        "FIXME: Missing semicolon is currently NOT detected (uses MISSING node, not ERROR)"
    );
}

#[test]
fn test_valid_syntax_is_accepted() {
    let source = r#"fn test() { return 0 >= 0; }"#;
    // If this panics, the test fails - valid syntax should be accepted
    let _arena = build_ast(source.to_string());
}

// --- Location and Source Tests ---

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

// --- Builder API Tests ---

#[test]
fn test_builder_default_creates_empty_builder() {
    let builder: Builder<'_> = Builder::default();
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
    let arena = builder.build_ast().unwrap();

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

// Note: Basic function definition tests are in builder.rs (test_parse_function_no_params,
// test_parse_simple_function, test_parse_function_multiple_params)

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

// Note: Basic variable definition tests are in builder.rs (test_parse_variable_declaration,
// test_parse_variable_declaration_no_init)

// --- Non-Deterministic Block Tests ---

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
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    crate::utils::assert_single_binary_op(&arena, OperatorKind::BitAnd);
}

#[test]
fn test_parse_bitwise_or() {
    let source = r#"fn test() -> i32 { return a | b; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    crate::utils::assert_single_binary_op(&arena, OperatorKind::BitOr);
}

#[test]
fn test_parse_bitwise_xor() {
    let source = r#"fn test() -> i32 { return a ^ b; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    crate::utils::assert_single_binary_op(&arena, OperatorKind::BitXor);
}

#[test]
fn test_parse_shift_left() {
    let source = r#"fn test() -> i32 { return a << 2; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    crate::utils::assert_single_binary_op(&arena, OperatorKind::Shl);
}

#[test]
fn test_parse_shift_right() {
    let source = r#"fn test() -> i32 { return a >> 2; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    crate::utils::assert_single_binary_op(&arena, OperatorKind::Shr);
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

/// Tests for qualified names and type qualified names

#[test]
fn test_parse_qualified_name_type() {
    let source = r#"fn test(x: std::i32) {}"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_function_signature(&arena, "test", Some(1), false);
}

#[test]
fn test_parse_function_type_parameter() {
    let source = r#"fn apply(f: fn(i32) -> i32, x: i32) -> i32 { return f(x); }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_function_signature(&arena, "apply", Some(2), true);
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

// --- Visibility Tests ---

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
fn test_parse_external_function_visibility_private() {
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
fn test_parse_spec_definition_visibility_private() {
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

// --- Additional Definition and Non-Deterministic Block Tests ---

/// Tests parsing a function with forall block followed by a variable definition.
#[test]
fn test_parse_function_with_forall_and_variable() {
    let source =
        r#"fn sum(items: [i32; 10]) -> i32 { forall { assert true; } let result: i32 = 0; }"#;
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
    let source = r#"fn add(a: i32, b: i32) -> i32 { forall { let x: i32 = @; return @ + b; } return a + b; }"#;
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

// FIXME: tree-sitter grammar does not support typeof() syntax yet.
// When grammar support is added, this test should verify typeof parsing with external functions.
#[test]
fn test_parse_typeof_expression() {
    let source = r#"external fn sorting_function(a: Address, b: Address) -> Address;
type sf = sorting_function;"#;
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
    let source = r#"const x: i32 = 5;type mytype = I32_EX;"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_constant_def(&arena, "x");

    let type_defs =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Type(_))));
    assert_eq!(type_defs.len(), 1, "Should find 1 type definition");
}

#[test]
fn test_parse_method_call_expression() {
    let source = r#"fn test() { let result: i32 = object.method(); }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_variable_def(&arena, "result");
}

#[test]
fn test_parse_method_call_with_args() {
    let source = r#"fn test() { let result: u64 = object.method(arg1, arg2); }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_variable_def(&arena, "result");
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
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_variable_def(&arena, "point");

    let struct_exprs =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Struct(_))));
    assert_eq!(struct_exprs.len(), 1, "Should find 1 struct expression");
}

#[test]
fn test_parse_nested_struct_expression() {
    let source = r#"fn test() {
    let rect: Rectangle = Rectangle {
        top_left: Point { x: 0, y: 0 },
        bottom_right: Point { x: 100, y: 100 }
    };}"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_variable_def(&arena, "rect");

    let struct_exprs =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Struct(_))));
    assert!(
        !struct_exprs.is_empty(),
        "Should find at least 1 struct expression"
    );
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
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_function_signature(&arena, "test", Some(0), true);

    let calls =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::FunctionCall(_))));
    assert_eq!(calls.len(), 3, "Should find 3 nested function calls");
}

#[test]
fn test_parse_if_elseif_else() {
    let source = r#"fn test(x: i32) -> i32 { if x > 10 { return 1; } else if x > 5 { return 2; } else { return 3; } }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_function_signature(&arena, "test", Some(1), true);

    let ifs = arena.filter_nodes(|node| matches!(node, AstNode::Statement(Statement::If(_))));
    assert!(!ifs.is_empty(), "Should find at least 1 if statement");
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
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_function_signature(&arena, "test", Some(2), true);

    let ifs = arena.filter_nodes(|node| matches!(node, AstNode::Statement(Statement::If(_))));
    assert_eq!(ifs.len(), 2, "Should find 2 nested if statements");
}

#[test]
fn test_parse_use_from_directive() {
    let source = r#"use { HashMap } from "./collections.wasm";"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let source_files = arena.source_files();
    assert_eq!(
        source_files[0].directives.len(),
        1,
        "Should find 1 use directive"
    );
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
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let var_defs = arena
        .filter_nodes(|node| matches!(node, AstNode::Statement(Statement::VariableDefinition(_))));
    assert_eq!(var_defs.len(), 4, "Should find 4 variable definitions");
}

#[test]
fn test_parse_variable_with_type_annotation() {
    let source = r#"fn test() { let x: i32 = 42; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_variable_def(&arena, "x");
}

#[test]
fn test_parse_assignment_to_member() {
    let source = r#"fn test() { point.x = 10; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let assigns =
        arena.filter_nodes(|node| matches!(node, AstNode::Statement(Statement::Assign(_))));
    assert_eq!(assigns.len(), 1, "Should find 1 assignment statement");
}

#[test]
fn test_parse_assignment_to_array_index() {
    let source = r#"fn test() { arr[0] = 42; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");

    let assigns =
        arena.filter_nodes(|node| matches!(node, AstNode::Statement(Statement::Assign(_))));
    assert_eq!(assigns.len(), 1, "Should find 1 assignment statement");
}

#[test]
fn test_parse_array_of_arrays() {
    let source = r#"fn test() { let matrix: [[i32; 2]; 2] = [[1, 2], [3, 4]]; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_variable_def(&arena, "matrix");

    let array_literals = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::Expression(Expression::Literal(Literal::Array(_)))
        )
    });
    assert!(
        array_literals.len() >= 3,
        "Should find at least 3 array literals (outer + 2 inner)"
    );
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
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_function_signature(&arena, "test", Some(1), true);
}

#[test]
fn test_parse_function_with_mixed_params() {
    let source = r#"fn test(a: i32, _: i32, c: i32) -> i32 { return a + c; }"#;
    let arena = build_ast(source.to_string());
    assert_eq!(arena.source_files().len(), 1, "Should have 1 source file");
    assert_function_signature(&arena, "test", Some(3), true);
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
