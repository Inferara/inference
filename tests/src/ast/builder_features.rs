use crate::utils::{
    assert_constant_def, assert_function_signature, assert_variable_def, build_ast,
    collect_exprs_matching, find_function_by_name, try_build_ast,
};
use inference_ast::arena::AstArena;
use inference_ast::ids::*;
use inference_ast::nodes::{
    ArgKind, BlockKind, Def, Expr, OperatorKind, Stmt, TypeNode, Visibility,
};

// --- Parse Error Detection Tests ---

#[test]
fn test_invalid_syntax_return_missing_left_operand_is_rejected() {
    let source = r#"fn test() { return >= 0; }"#;
    let result = try_build_ast(source.to_string());
    assert!(
        result.is_err(),
        "Invalid syntax 'return >= 0;' should be rejected during parsing"
    );
}

#[test]
fn test_invalid_syntax_in_forall_block_is_rejected() {
    let source =
        r#"fn sum(items: [i32; 10]) -> i32 { forall { return >= 0; } let result: i32 = 0; }"#;
    let result = try_build_ast(source.to_string());
    assert!(
        result.is_err(),
        "Invalid syntax inside forall block should be rejected during parsing"
    );
}

#[test]
fn test_missing_semicolon_is_detected() {
    let source = r#"fn test() { let x: i32 = 5 }"#;
    let result = try_build_ast(source.to_string());
    assert!(
        result.is_err(),
        "Missing semicolon after a variable definition is a syntax error"
    );
}

#[test]
fn test_valid_syntax_is_accepted() {
    let source = r#"fn test() { return 0 >= 0; }"#;
    let _arena = build_ast(source.to_string());
}

// --- Location and Source Tests ---

#[test]
fn test_source_file_stores_source_correctly() {
    let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
    let arena = build_ast(source.to_string());
    let source_files: Vec<_> = arena.source_files().collect();
    assert_eq!(source_files.len(), 1);
    assert_eq!(source_files[0].source, source);
}

#[test]
fn test_source_file_source_with_multiple_definitions() {
    let source = r#"const X: i32 = 42;
fn add(a: i32, b: i32) -> i32 { return a + b; }
struct Point { x: i32; y: i32; }"#;
    let arena = build_ast(source.to_string());
    let source_files: Vec<_> = arena.source_files().collect();
    assert_eq!(source_files.len(), 1);
    assert_eq!(source_files[0].source, source);
}

#[test]
fn test_source_file_source_empty_function() {
    let source = r#"fn empty() {}"#;
    let arena = build_ast(source.to_string());
    let source_files: Vec<_> = arena.source_files().collect();
    assert_eq!(source_files[0].source, source);
}

#[test]
fn test_location_offset_extracts_function_definition() {
    let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
    let arena = build_ast(source.to_string());
    let source_files: Vec<_> = arena.source_files().collect();
    let source_file = &source_files[0];

    assert_eq!(source_file.defs.len(), 1);
    let def_id = source_file.defs[0];
    let loc = arena[def_id].location;
    let extracted = &source_file.source[loc.offset_start as usize..loc.offset_end as usize];
    assert_eq!(extracted, source);
}

#[test]
fn test_location_offset_extracts_identifier() {
    let source = r#"fn my_function() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());
    let source_files: Vec<_> = arena.source_files().collect();
    let source_file = &source_files[0];

    let def_id = source_file.defs[0];
    if let Def::Function { name, .. } = &arena[def_id].kind {
        let name_loc = arena[*name].location;
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
    let source_files: Vec<_> = arena.source_files().collect();
    let source_file = &source_files[0];

    let def_id = source_file.defs[0];
    if let Def::Struct { name, .. } = &arena[def_id].kind {
        let loc = arena[def_id].location;
        let extracted = &source_file.source[loc.offset_start as usize..loc.offset_end as usize];
        assert_eq!(extracted, source);

        let name_loc = arena[*name].location;
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
    let source_files: Vec<_> = arena.source_files().collect();
    let source_file = &source_files[0];

    let def_id = source_file.defs[0];
    if let Def::Struct { fields, .. } = &arena[def_id].kind {
        assert_eq!(fields.len(), 2);

        let field_x_name_loc = arena[fields[0].name].location;
        let field_x_name = &source_file.source
            [field_x_name_loc.offset_start as usize..field_x_name_loc.offset_end as usize];
        assert_eq!(field_x_name, "x");

        let field_y_name_loc = arena[fields[1].name].location;
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
    let source_files: Vec<_> = arena.source_files().collect();
    let source_file = &source_files[0];

    let def_id = source_file.defs[0];
    if let Def::Constant { name, .. } = &arena[def_id].kind {
        let loc = arena[def_id].location;
        let extracted = &source_file.source[loc.offset_start as usize..loc.offset_end as usize];
        assert_eq!(extracted, source);

        let name_loc = arena[*name].location;
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
    let source_files: Vec<_> = arena.source_files().collect();
    let source_file = &source_files[0];

    let def_id = source_file.defs[0];
    if let Def::Enum { name, variants, .. } = &arena[def_id].kind {
        let loc = arena[def_id].location;
        let extracted = &source_file.source[loc.offset_start as usize..loc.offset_end as usize];
        assert_eq!(extracted, source);

        let name_loc = arena[*name].location;
        let name_extracted =
            &source_file.source[name_loc.offset_start as usize..name_loc.offset_end as usize];
        assert_eq!(name_extracted, "Color");

        assert_eq!(variants.len(), 3);
        let variant_names: Vec<&str> = variants
            .iter()
            .map(|&v| {
                let loc = arena[v].location;
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
    let source_files: Vec<_> = arena.source_files().collect();
    let source_file = &source_files[0];

    assert_eq!(source_file.defs.len(), 2);

    let def0 = source_file.defs[0];
    if let Def::Constant { name, .. } = &arena[def0].kind {
        let name_loc = arena[*name].location;
        let name_extracted =
            &source_file.source[name_loc.offset_start as usize..name_loc.offset_end as usize];
        assert_eq!(name_extracted, "X");
    } else {
        panic!("Expected constant definition");
    }

    let def1 = source_file.defs[1];
    if let Def::Function { name, .. } = &arena[def1].kind {
        let name_loc = arena[*name].location;
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
    let source_files: Vec<_> = arena.source_files().collect();
    let source_file = &source_files[0];

    let def_id = source_file.defs[0];
    if let Def::Function { args, .. } = &arena[def_id].kind {
        assert_eq!(args.len(), 2);

        if let ArgKind::Named { name, .. } = &args[0].kind {
            let name_loc = arena[*name].location;
            let arg1_name =
                &source_file.source[name_loc.offset_start as usize..name_loc.offset_end as usize];
            assert_eq!(arg1_name, "first_arg");
        } else {
            panic!("Expected Named argument");
        }

        if let ArgKind::Named { name, .. } = &args[1].kind {
            let name_loc = arena[*name].location;
            let arg2_name =
                &source_file.source[name_loc.offset_start as usize..name_loc.offset_end as usize];
            assert_eq!(arg2_name, "second_arg");
        } else {
            panic!("Expected Named argument");
        }
    } else {
        panic!("Expected function definition");
    }
}

#[test]
fn test_location_offset_extracts_use_directive() {
    let source = r#"use inference::std::collections;"#;
    let arena = build_ast(source.to_string());
    let source_files: Vec<_> = arena.source_files().collect();
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
    let source_files: Vec<_> = arena.source_files().collect();
    let source_file = &source_files[0];

    assert_eq!(source_file.source, source);

    let def_id = source_file.defs[0];
    if let Def::Function { name, .. } = &arena[def_id].kind {
        let name_loc = arena[*name].location;
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
    let source_files: Vec<_> = arena.source_files().collect();
    let source_file = &source_files[0];

    let def_id = source_file.defs[0];
    if let Def::ExternFunction { name, .. } = &arena[def_id].kind {
        let loc = arena[def_id].location;
        let extracted = &source_file.source[loc.offset_start as usize..loc.offset_end as usize];
        assert_eq!(extracted, source);

        let name_loc = arena[*name].location;
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
    let source_files: Vec<_> = arena.source_files().collect();
    let source_file = &source_files[0];

    let def_id = source_file.defs[0];
    if let Def::TypeAlias { name, .. } = &arena[def_id].kind {
        let loc = arena[def_id].location;
        let extracted = &source_file.source[loc.offset_start as usize..loc.offset_end as usize];
        assert_eq!(extracted, source);

        let name_loc = arena[*name].location;
        let name_extracted =
            &source_file.source[name_loc.offset_start as usize..name_loc.offset_end as usize];
        assert_eq!(name_extracted, "MyInt");
    } else {
        panic!("Expected type alias definition");
    }
}

#[test]
fn test_source_file_location_covers_entire_source() {
    let source = r#"fn test() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());
    let source_files: Vec<_> = arena.source_files().collect();
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
    let source_files: Vec<_> = arena.source_files().collect();
    let source_file = &source_files[0];

    assert_eq!(source_file.source, source);
    assert_eq!(source_file.defs.len(), 1);
}

/// Tests for struct expressions with fields

#[test]
fn test_parse_struct_expression_finds_correct_node_type() {
    let source = r#"struct Point { x: i32; y: i32; }
fn test() -> Point { return Point { x: 10, y: 20 }; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs =
            collect_exprs_matching(&arena, *body, &|e| matches!(e, Expr::StructLiteral { .. }));
        assert_eq!(exprs.len(), 1, "Should find 1 struct expression");

        if let Expr::StructLiteral { name, .. } = &arena[exprs[0]].kind {
            assert_eq!(arena[*name].name, "Point");
        }
    }
}

#[test]
fn test_parse_struct_expression_empty_struct() {
    let source = r#"struct Empty {}
fn test() -> Empty { return Empty {}; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs =
            collect_exprs_matching(&arena, *body, &|e| matches!(e, Expr::StructLiteral { .. }));
        assert_eq!(exprs.len(), 1, "Should find 1 struct expression");

        if let Expr::StructLiteral { name, .. } = &arena[exprs[0]].kind {
            assert_eq!(arena[*name].name, "Empty");
        }
    }
}

/// Tests for type definition statement

#[test]
fn test_parse_type_definition_in_function_body() {
    let source = r#"fn test() { type LocalInt = i32; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        let type_defs: Vec<_> = block
            .stmts
            .iter()
            .filter(|&&s| matches!(arena[s].kind, Stmt::TypeDef { .. }))
            .collect();
        assert_eq!(
            type_defs.len(),
            1,
            "Should find 1 type definition statement"
        );

        if let Stmt::TypeDef { name, .. } = &arena[*type_defs[0]].kind {
            assert_eq!(arena[*name].name, "LocalInt");
        }
    }
}

#[test]
fn test_parse_multiple_type_definitions_in_function() {
    let source = r#"fn test() { type A = i32; type B = bool; type C = i64; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        let type_def_count = block
            .stmts
            .iter()
            .filter(|&&s| matches!(arena[s].kind, Stmt::TypeDef { .. }))
            .count();
        assert_eq!(
            type_def_count, 3,
            "Should find 3 type definition statements"
        );
    }
}

// --- Non-Deterministic Block Tests ---

#[test]
fn test_parse_forall_block() {
    let source = r#"fn test() { forall { assert true; } }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        let forall_count = block
            .stmts
            .iter()
            .filter(|&&s| {
                if let Stmt::Block(block_id) = &arena[s].kind {
                    arena[*block_id].block_kind == BlockKind::Forall
                } else {
                    false
                }
            })
            .count();
        assert_eq!(forall_count, 1, "Should find 1 forall block");
    }
}

#[test]
fn test_parse_exists_block() {
    let source = r#"fn test() { exists { assert true; } }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        let exists_count = block
            .stmts
            .iter()
            .filter(|&&s| {
                if let Stmt::Block(block_id) = &arena[s].kind {
                    arena[*block_id].block_kind == BlockKind::Exists
                } else {
                    false
                }
            })
            .count();
        assert_eq!(exists_count, 1, "Should find 1 exists block");
    }
}

#[test]
fn test_parse_unique_block() {
    let source = r#"fn test() { unique { assert true; } }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        let unique_count = block
            .stmts
            .iter()
            .filter(|&&s| {
                if let Stmt::Block(block_id) = &arena[s].kind {
                    arena[*block_id].block_kind == BlockKind::Unique
                } else {
                    false
                }
            })
            .count();
        assert_eq!(unique_count, 1, "Should find 1 unique block");
    }
}

#[test]
fn test_parse_assume_block() {
    let source = r#"fn test() { assume { assert true; } }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let block = &arena[*body];
        let assume_count = block
            .stmts
            .iter()
            .filter(|&&s| {
                if let Stmt::Block(block_id) = &arena[s].kind {
                    arena[*block_id].block_kind == BlockKind::Assume
                } else {
                    false
                }
            })
            .count();
        assert_eq!(assume_count, 1, "Should find 1 assume block");
    }
}

/// Tests for various binary operators

#[test]
fn test_parse_bitwise_and() {
    let source = r#"fn test() -> i32 { return a & b; }"#;
    let arena = build_ast(source.to_string());
    crate::utils::assert_single_binary_op(&arena, OperatorKind::BitAnd);
}

#[test]
fn test_parse_bitwise_or() {
    let source = r#"fn test() -> i32 { return a | b; }"#;
    let arena = build_ast(source.to_string());
    crate::utils::assert_single_binary_op(&arena, OperatorKind::BitOr);
}

#[test]
fn test_parse_bitwise_xor() {
    let source = r#"fn test() -> i32 { return a ^ b; }"#;
    let arena = build_ast(source.to_string());
    crate::utils::assert_single_binary_op(&arena, OperatorKind::BitXor);
}

#[test]
fn test_parse_shift_left() {
    let source = r#"fn test() -> i32 { return a << 2; }"#;
    let arena = build_ast(source.to_string());
    crate::utils::assert_single_binary_op(&arena, OperatorKind::Shl);
}

#[test]
fn test_parse_shift_right() {
    let source = r#"fn test() -> i32 { return a >> 2; }"#;
    let arena = build_ast(source.to_string());
    crate::utils::assert_single_binary_op(&arena, OperatorKind::Shr);
}

/// Tests for function arguments

#[test]
fn test_parse_self_reference_in_method() {
    let source = r#"struct Counter {
        value: i32;
        fn get(self) -> i32 { return 42; }
    }"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::Struct { methods, .. } = &arena[def_id].kind {
        assert_eq!(methods.len(), 1);
        if let Def::Function { args, .. } = &arena[methods[0]].kind {
            let self_count = args
                .iter()
                .filter(|a| matches!(a.kind, ArgKind::SelfRef { .. }))
                .count();
            assert_eq!(self_count, 1, "Should find 1 self reference");
        }
    }
}

#[test]
fn test_parse_ignore_argument() {
    let source = r#"fn test(_: i32) -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { args, .. } = &arena[func_id].kind {
        let ignored_count = args
            .iter()
            .filter(|a| matches!(a.kind, ArgKind::Ignored { .. }))
            .count();
        assert_eq!(ignored_count, 1, "Should find 1 ignore argument");
    }
}

/// Tests for type member access expression

#[test]
fn test_parse_type_member_access() {
    let source = r#"fn test() -> i32 { return Color::Red; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { body, .. } = &arena[func_id].kind {
        let exprs = collect_exprs_matching(&arena, *body, &|e| {
            matches!(e, Expr::TypeMemberAccess { .. })
        });
        assert_eq!(exprs.len(), 1, "Should find 1 type member access");
    }
}

/// Tests for qualified names and type qualified names

#[test]
fn test_parse_qualified_name_type() {
    let source = r#"fn test(x: std::i32) {}"#;
    let arena = build_ast(source.to_string());
    assert_function_signature(&arena, "test", Some(1), false);
}

#[test]
fn test_parse_function_type_parameter() {
    let source = r#"fn apply(f: fn(i32) -> i32, x: i32) -> i32 { return f(x); }"#;
    let arena = build_ast(source.to_string());
    assert_function_signature(&arena, "apply", Some(2), true);
}

/// Test for constant definitions

#[test]
fn test_parse_constant_definition_at_module_level() {
    let source = r#"const GLOBAL: i32 = 42;"#;
    let arena = build_ast(source.to_string());
    assert_constant_def(&arena, "GLOBAL");
}

/// Returns the `value` ExprId of the first `const` statement inside the function
/// named `fn_name`. Panics if not found — intended for test use only.
fn first_function_scope_const_value(arena: &AstArena, fn_name: &str) -> ExprId {
    let func_id = find_function_by_name(arena, fn_name).expect("function not found");
    let Def::Function { body, .. } = &arena[func_id].kind else {
        panic!("{fn_name} is not a function")
    };
    let block = &arena[*body];
    for &stmt_id in &block.stmts {
        if let Stmt::ConstDef(def_id) = arena[stmt_id].kind
            && let Def::Constant { value, .. } = arena[def_id].kind
        {
            return value;
        }
    }
    panic!("no Stmt::ConstDef found in body of {fn_name}")
}

#[test]
fn test_parse_const_initializer_accepts_struct_literal() {
    let source = r#"struct Point { x: i32; y: i32; }
fn test() { const P: Point = Point { x: 1, y: 2 }; }"#;
    let arena = build_ast(source.to_string());
    let value = first_function_scope_const_value(&arena, "test");
    assert!(
        matches!(&arena[value].kind, Expr::StructLiteral { .. }),
        "const initializer should parse as StructLiteral, got {:?}",
        &arena[value].kind
    );
}

#[test]
fn test_parse_const_initializer_accepts_array_literal() {
    let source = r#"fn test() { const ARR: [i32; 3] = [1, 2, 3]; }"#;
    let arena = build_ast(source.to_string());
    let value = first_function_scope_const_value(&arena, "test");
    assert!(
        matches!(&arena[value].kind, Expr::ArrayLiteral { .. }),
        "const initializer should parse as ArrayLiteral, got {:?}",
        &arena[value].kind
    );
}

#[test]
fn test_parse_const_initializer_accepts_identifier_copy() {
    let source = r#"struct Point { x: i32; y: i32; }
fn test() {
    let base: Point = Point { x: 1, y: 2 };
    const P: Point = base;
}"#;
    let arena = build_ast(source.to_string());
    let value = first_function_scope_const_value(&arena, "test");
    assert!(
        matches!(&arena[value].kind, Expr::Identifier(_)),
        "const initializer should parse as Identifier, got {:?}",
        &arena[value].kind
    );
}

#[test]
fn test_parse_const_initializer_accepts_function_call() {
    let source = r#"fn make() -> [i32; 3] { return [1, 2, 3]; }
fn test() { const ARR: [i32; 3] = make(); }"#;
    let arena = build_ast(source.to_string());
    let value = first_function_scope_const_value(&arena, "test");
    assert!(
        matches!(&arena[value].kind, Expr::FunctionCall { .. }),
        "const initializer should parse as FunctionCall, got {:?}",
        &arena[value].kind
    );
}

/// Test for arguments

#[test]
fn test_parse_argument_with_type() {
    let source = r#"fn test(x: i32) { }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "test").unwrap();
    if let Def::Function { args, .. } = &arena[func_id].kind {
        let named_count = args
            .iter()
            .filter(|a| matches!(a.kind, ArgKind::Named { .. }))
            .count();
        assert_eq!(named_count, 1, "Should find 1 argument");
    }
}

/// Test for external function definitions

#[test]
fn test_parse_external_function_with_return() {
    let source = r#"external fn get_value() -> i32;"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::ExternFunction { returns, .. } = &arena[def_id].kind {
        assert!(returns.is_some(), "Should have return type");
    } else {
        panic!("Expected external function definition");
    }
}

#[test]
fn test_parse_external_function_basic() {
    let source = r#"external fn do_something();"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::ExternFunction { name, .. } = &arena[def_id].kind {
        assert_eq!(arena[*name].name, "do_something");
    } else {
        panic!("Expected external function definition");
    }
}

// --- Visibility Tests ---

#[test]
fn test_parse_public_function_visibility() {
    let source = r#"pub fn public_function() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "public_function").unwrap();
    if let Def::Function { vis, .. } = &arena[func_id].kind {
        assert_eq!(
            *vis,
            Visibility::Public,
            "Function should have Public visibility"
        );
    }
}

#[test]
fn test_parse_private_function_visibility() {
    let source = r#"fn private_function() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let func_id = find_function_by_name(&arena, "private_function").unwrap();
    if let Def::Function { vis, .. } = &arena[func_id].kind {
        assert_eq!(
            *vis,
            Visibility::Private,
            "Function without pub should have Private visibility"
        );
    }
}

#[test]
fn test_parse_public_struct_visibility() {
    let source = r#"pub struct PublicStruct { x: i32; }"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::Struct { vis, .. } = &arena[def_id].kind {
        assert_eq!(
            *vis,
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

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::Struct { vis, .. } = &arena[def_id].kind {
        assert_eq!(*vis, Visibility::Private);
    }
}

#[test]
fn test_parse_public_enum_visibility() {
    let source = r#"pub enum PublicEnum { A, B, C }"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::Enum { vis, .. } = &arena[def_id].kind {
        assert_eq!(
            *vis,
            Visibility::Public,
            "Enum should have Public visibility"
        );
    }
}

#[test]
fn test_parse_private_enum_visibility() {
    let source = r#"enum PrivateEnum { X, Y, Z }"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::Enum { vis, .. } = &arena[def_id].kind {
        assert_eq!(*vis, Visibility::Private);
    }
}

#[test]
fn test_parse_public_constant_visibility() {
    let source = r#"pub const MAX_VALUE: i32 = 100;"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::Constant { vis, .. } = &arena[def_id].kind {
        assert_eq!(*vis, Visibility::Public);
    }
}

#[test]
fn test_parse_private_constant_visibility() {
    let source = r#"const MIN_VALUE: i32 = 0;"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::Constant { vis, .. } = &arena[def_id].kind {
        assert_eq!(*vis, Visibility::Private);
    }
}

#[test]
fn test_parse_public_type_alias_visibility() {
    let source = r#"pub type MyInt = i32;"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::TypeAlias { vis, .. } = &arena[def_id].kind {
        assert_eq!(*vis, Visibility::Public);
    }
}

#[test]
fn test_parse_private_type_alias_visibility() {
    let source = r#"type LocalInt = i32;"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::TypeAlias { vis, .. } = &arena[def_id].kind {
        assert_eq!(*vis, Visibility::Private);
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
    let source_files: Vec<_> = arena.source_files().collect();
    assert_eq!(source_files.len(), 1);
    assert_eq!(source_files[0].defs.len(), 6);

    let defs = &source_files[0].defs;

    if let Def::Function { name, vis, .. } = &arena[defs[0]].kind {
        assert_eq!(arena[*name].name, "public_func");
        assert_eq!(*vis, Visibility::Public);
    } else {
        panic!("Expected function definition");
    }

    if let Def::Function { name, vis, .. } = &arena[defs[1]].kind {
        assert_eq!(arena[*name].name, "private_func");
        assert_eq!(*vis, Visibility::Private);
    } else {
        panic!("Expected function definition");
    }

    if let Def::Struct { name, vis, .. } = &arena[defs[2]].kind {
        assert_eq!(arena[*name].name, "PublicStruct");
        assert_eq!(*vis, Visibility::Public);
    } else {
        panic!("Expected struct definition");
    }

    if let Def::Struct { name, vis, .. } = &arena[defs[3]].kind {
        assert_eq!(arena[*name].name, "PrivateStruct");
        assert_eq!(*vis, Visibility::Private);
    } else {
        panic!("Expected struct definition");
    }

    if let Def::Constant { name, vis, .. } = &arena[defs[4]].kind {
        assert_eq!(arena[*name].name, "PUBLIC_CONST");
        assert_eq!(*vis, Visibility::Public);
    } else {
        panic!("Expected constant definition");
    }

    if let Def::Constant { name, vis, .. } = &arena[defs[5]].kind {
        assert_eq!(arena[*name].name, "PRIVATE_CONST");
        assert_eq!(*vis, Visibility::Private);
    } else {
        panic!("Expected constant definition");
    }
}

#[test]
fn test_parse_external_function_visibility_private() {
    let source = r#"external fn extern_func() -> i32;"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::ExternFunction { vis, .. } = &arena[def_id].kind {
        assert_eq!(*vis, Visibility::Private);
    }
}

#[test]
fn test_parse_spec_definition_visibility_private() {
    let source = r#"spec MySpec { fn verify() -> bool { return true; } }"#;
    let arena = build_ast(source.to_string());

    let source_files: Vec<_> = arena.source_files().collect();
    let def_id = source_files[0].defs[0];
    if let Def::Spec { vis, .. } = &arena[def_id].kind {
        assert_eq!(*vis, Visibility::Private);
    }
}
