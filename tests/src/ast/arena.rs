use crate::utils::build_ast;
use inference_ast::arena::AstArena;
use inference_ast::ids::*;
use inference_ast::nodes::*;

#[test]
fn test_source_files_parsed_correctly() {
    let source = r#"fn test() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    assert_eq!(source_files.len(), 1, "Should have 1 source file");
    assert_eq!(
        source_files[0].source, source,
        "Source file should contain the original source"
    );
}

#[test]
fn test_function_def_ids_returns_functions() {
    let source = r#"fn first() -> i32 { return 1; } fn second() -> i32 { return 2; }"#;
    let arena = build_ast(source.to_string());

    let func_ids = arena.function_def_ids();
    assert_eq!(func_ids.len(), 2, "Should find 2 function definitions");

    for def_id in &func_ids {
        assert!(
            matches!(arena[*def_id].kind, Def::Function { .. }),
            "DefId should point to a function"
        );
    }
}

#[test]
fn test_def_name_returns_function_name() {
    let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
    let arena = build_ast(source.to_string());

    let func_ids = arena.function_def_ids();
    assert_eq!(func_ids.len(), 1);
    assert_eq!(arena.def_name(func_ids[0]), "add");
}

#[test]
fn test_multiple_definitions_have_correct_names() {
    let source = r#"fn first() {} fn second() {} fn third() {}"#;
    let arena = build_ast(source.to_string());

    let func_ids = arena.function_def_ids();
    assert_eq!(func_ids.len(), 3);

    let names: Vec<&str> = func_ids.iter().map(|&id| arena.def_name(id)).collect();
    assert!(names.contains(&"first"));
    assert!(names.contains(&"second"));
    assert!(names.contains(&"third"));
}

#[test]
fn test_source_file_defs_include_all_definitions() {
    let source = r#"const A: i32 = 1; const B: i32 = 2; fn test() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    assert_eq!(source_files.len(), 1);
    assert_eq!(
        source_files[0].defs.len(),
        3,
        "SourceFile should have 3 definitions (2 constants + 1 function)"
    );
}

#[test]
fn test_struct_definition_has_fields_and_methods() {
    let source = r#"struct Point { x: i32; y: i32; }"#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    assert_eq!(source_files.len(), 1);
    assert_eq!(source_files[0].defs.len(), 1);

    let def_id = source_files[0].defs[0];
    if let Def::Struct { name, fields, .. } = &arena[def_id].kind {
        assert_eq!(arena[*name].name, "Point");
        assert_eq!(fields.len(), 2, "Struct should have 2 fields");
    } else {
        panic!("Expected struct definition");
    }
}

#[test]
fn test_function_body_has_statements() {
    let source = r#"fn test() -> i32 { let x: i32 = 10; return x; }"#;
    let arena = build_ast(source.to_string());

    let func_ids = arena.function_def_ids();
    assert_eq!(func_ids.len(), 1);

    if let Def::Function { body, .. } = &arena[func_ids[0]].kind {
        let block = &arena[*body];
        assert_eq!(
            block.stmts.len(),
            2,
            "Function body should have 2 statements"
        );
    }
}

#[test]
fn test_variable_definition_properties() {
    let source = r#"fn test() { let x: i32 = 10; }"#;
    let arena = build_ast(source.to_string());

    let func_ids = arena.function_def_ids();
    if let Def::Function { body, .. } = &arena[func_ids[0]].kind {
        let block = &arena[*body];
        assert_eq!(block.stmts.len(), 1);

        let stmt_id = block.stmts[0];
        if let Stmt::VarDef { name, ty, value, is_mut } = &arena[stmt_id].kind {
            assert_eq!(arena[*name].name, "x");
            assert!(matches!(arena[*ty].kind, TypeNode::Simple(SimpleTypeKind::I32)));
            assert!(value.is_some());
            assert!(!is_mut);
        } else {
            panic!("Expected variable definition");
        }
    }
}

#[test]
fn test_return_statement_has_expression() {
    let source = r#"fn test() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let func_ids = arena.function_def_ids();
    if let Def::Function { body, .. } = &arena[func_ids[0]].kind {
        let block = &arena[*body];
        assert_eq!(block.stmts.len(), 1);

        if let Stmt::Return { expr } = &arena[block.stmts[0]].kind {
            if let Expr::NumberLiteral { value } = &arena[*expr].kind {
                assert_eq!(value, "42");
            } else {
                panic!("Expected number literal in return");
            }
        } else {
            panic!("Expected return statement");
        }
    }
}

#[test]
fn test_binary_expression_structure() {
    let source = r#"fn calc() -> i32 { return 10 + 20; }"#;
    let arena = build_ast(source.to_string());

    let func_ids = arena.function_def_ids();
    if let Def::Function { body, .. } = &arena[func_ids[0]].kind {
        let block = &arena[*body];
        if let Stmt::Return { expr } = &arena[block.stmts[0]].kind {
            if let Expr::Binary { left, right, op } = &arena[*expr].kind {
                assert_eq!(*op, OperatorKind::Add);
                assert!(matches!(arena[*left].kind, Expr::NumberLiteral { .. }));
                assert!(matches!(arena[*right].kind, Expr::NumberLiteral { .. }));
            } else {
                panic!("Expected binary expression");
            }
        }
    }
}

#[test]
fn test_source_file_source_text() {
    let source = r#"fn main() -> i32 { return 0; }"#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    assert_eq!(source_files.len(), 1);
    assert_eq!(
        source_files[0].source, source,
        "SourceFile source should return the entire source text"
    );
}

#[test]
fn test_location_offsets() {
    let source = r#"fn test() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let func_ids = arena.function_def_ids();
    let func_loc = arena[func_ids[0]].location;
    assert_eq!(func_loc.offset_start, 0);
    assert!(func_loc.offset_end > 0, "Function should have non-zero end offset");
}

/// Tests for constant/struct/enum definitions

#[test]
fn test_constant_definition_structure() {
    let source = r#"const X: i32 = 42;"#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    assert_eq!(source_files[0].defs.len(), 1);

    let def_id = source_files[0].defs[0];
    if let Def::Constant { name, ty, value, .. } = &arena[def_id].kind {
        assert_eq!(arena[*name].name, "X");
        assert!(matches!(arena[*ty].kind, TypeNode::Simple(SimpleTypeKind::I32)));
        assert!(matches!(arena[*value].kind, Expr::NumberLiteral { .. }));
    } else {
        panic!("Expected constant definition");
    }
}

#[test]
fn test_type_alias_definition() {
    let source = r#"type MyInt = i32;"#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    assert_eq!(source_files[0].defs.len(), 1);

    let def_id = source_files[0].defs[0];
    if let Def::TypeAlias { name, ty, .. } = &arena[def_id].kind {
        assert_eq!(arena[*name].name, "MyInt");
        assert!(matches!(arena[*ty].kind, TypeNode::Simple(SimpleTypeKind::I32)));
    } else {
        panic!("Expected type alias definition");
    }
}

#[test]
fn test_multiple_type_aliases() {
    let source = r#"type MyInt = i32;
type MyBool = bool;
type MyArray = [i32; 10];"#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    let type_aliases: Vec<&DefData> = source_files[0]
        .defs
        .iter()
        .map(|&id| &arena[id])
        .filter(|d| matches!(d.kind, Def::TypeAlias { .. }))
        .collect();
    assert_eq!(type_aliases.len(), 3, "Should find 3 type definitions");
}

#[test]
fn test_no_type_aliases_when_only_functions() {
    let source = r#"fn test() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    let type_aliases: Vec<&DefData> = source_files[0]
        .defs
        .iter()
        .map(|&id| &arena[id])
        .filter(|d| matches!(d.kind, Def::TypeAlias { .. }))
        .collect();
    assert!(type_aliases.is_empty(), "Should find no type definitions");
}

#[test]
fn test_mixed_definitions() {
    let source = r#"const X: i32 = 42;
type MyInt = i32;
fn test() -> i32 { return X; }
type MyBool = bool;"#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    assert_eq!(source_files[0].defs.len(), 4, "Should have 4 total definitions");

    let type_alias_count = source_files[0]
        .defs
        .iter()
        .filter(|&&id| matches!(arena[id].kind, Def::TypeAlias { .. }))
        .count();
    assert_eq!(type_alias_count, 2, "Should find 2 type definitions among mixed definitions");
}

/// Tests for empty arena

#[test]
fn test_empty_arena_source_files() {
    let arena = AstArena::default();
    assert!(
        arena.source_files().is_empty(),
        "Empty arena should return no source files"
    );
}

#[test]
fn test_empty_arena_function_def_ids() {
    let arena = AstArena::default();
    assert!(
        arena.function_def_ids().is_empty(),
        "Empty arena should return no functions"
    );
}

/// Tests for AstArena::clone() functionality

#[test]
fn test_arena_clone() {
    let source = r#"fn test() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());
    let cloned_arena = arena.clone();

    assert_eq!(
        arena.source_files().len(),
        cloned_arena.source_files().len(),
        "Cloned arena should have same number of source files"
    );

    assert_eq!(
        arena.function_def_ids().len(),
        cloned_arena.function_def_ids().len(),
        "Cloned arena should have same number of functions"
    );
}

/// Tests for Location

#[test]
fn test_location_default_via_struct() {
    let loc = Location::default();
    assert_eq!(loc.offset_start, 0);
    assert_eq!(loc.offset_end, 0);
    assert_eq!(loc.start_line, 0);
    assert_eq!(loc.start_column, 0);
    assert_eq!(loc.end_line, 0);
    assert_eq!(loc.end_column, 0);
}

/// Tests for alloc and index operations

#[test]
fn test_alloc_and_index_expr() {
    let mut arena = AstArena::default();
    let id = arena.exprs.alloc(ExprData {
        location: Location::default(),
        kind: Expr::NumberLiteral {
            value: "42".to_string(),
        },
    });
    assert!(matches!(arena[id].kind, Expr::NumberLiteral { .. }));
}

#[test]
fn test_alloc_and_index_ident() {
    let mut arena = AstArena::default();
    let id = arena.idents.alloc(Ident {
        location: Location::default(),
        name: "foo".to_string(),
    });
    assert_eq!(arena[id].name, "foo");
}

/// Tests for function with return type and arguments

#[test]
fn test_function_return_type() {
    let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
    let arena = build_ast(source.to_string());

    let func_ids = arena.function_def_ids();
    assert_eq!(func_ids.len(), 1);

    if let Def::Function { returns, .. } = &arena[func_ids[0]].kind {
        let ret_ty = returns.expect("Should have return type");
        assert!(matches!(arena[ret_ty].kind, TypeNode::Simple(SimpleTypeKind::I32)));
    }
}

#[test]
fn test_function_arguments() {
    let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
    let arena = build_ast(source.to_string());

    let func_ids = arena.function_def_ids();
    if let Def::Function { args, .. } = &arena[func_ids[0]].kind {
        assert_eq!(args.len(), 2);
        for arg in args {
            if let ArgKind::Named { ty, .. } = &arg.kind {
                assert!(matches!(arena[*ty].kind, TypeNode::Simple(SimpleTypeKind::I32)));
            }
        }
    }
}

#[test]
fn test_multiple_functions_source() {
    let source = r#"fn first() -> i32 { return 1; } fn second() -> i32 { return 2; }"#;
    let arena = build_ast(source.to_string());

    let func_ids = arena.function_def_ids();
    assert_eq!(func_ids.len(), 2, "Should find 2 functions");

    let names: Vec<&str> = func_ids.iter().map(|&id| arena.def_name(id)).collect();
    assert!(names.contains(&"first"));
    assert!(names.contains(&"second"));
}

/// Test directives are preserved

#[test]
fn test_directives_parsed() {
    let source = r#"use inference::std;"#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    assert_eq!(source_files.len(), 1);
    assert_eq!(source_files[0].directives.len(), 1);
}
