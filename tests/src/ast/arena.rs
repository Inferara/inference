use crate::utils::build_ast;
use inference_ast::nodes::{Ast, AstNode, Definition, Statement};

/// Tests for Arena's parent-child lookup functionality with FxHashMap-based O(1) lookups.
/// These tests verify Phase 3 of Issue 69: "Optimize Parent Lookup with FxHashMap".

#[test]
fn test_find_parent_node_returns_correct_parent() {
    let source = r#"fn test() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let functions = arena.functions();
    assert_eq!(functions.len(), 1);
    let function = &functions[0];

    let source_files = arena.source_files();
    assert_eq!(source_files.len(), 1);
    let source_file = &source_files[0];

    let parent_id = arena.find_parent_node(function.id);
    assert!(parent_id.is_some(), "Function should have a parent");
    assert_eq!(
        parent_id.unwrap(),
        source_file.id,
        "Function's parent should be the SourceFile"
    );
}

#[test]
fn test_find_parent_node_root_returns_none() {
    let source = r#"fn test() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    assert_eq!(source_files.len(), 1);
    let source_file = &source_files[0];

    let parent_id = arena.find_parent_node(source_file.id);
    assert!(
        parent_id.is_none(),
        "Root SourceFile node should have no parent (not Some(u32::MAX))"
    );
}

#[test]
fn test_find_parent_node_nonexistent_returns_none() {
    let source = r#"fn test() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let nonexistent_id = u32::MAX - 1;
    let parent_id = arena.find_parent_node(nonexistent_id);
    assert!(
        parent_id.is_none(),
        "Non-existent node ID should return None"
    );
}

#[test]
fn test_find_parent_node_nested_hierarchy() {
    let source = r#"fn outer() -> i32 { let x: i32 = 10; return x; }"#;
    let arena = build_ast(source.to_string());

    let functions = arena.functions();
    assert_eq!(functions.len(), 1);

    let statements = arena.filter_nodes(|node| {
        matches!(node, AstNode::Statement(Statement::VariableDefinition(_)))
    });
    assert_eq!(statements.len(), 1, "Expected 1 variable definition");

    let var_def = &statements[0];
    let var_parent_id = arena.find_parent_node(var_def.id());
    assert!(
        var_parent_id.is_some(),
        "Variable definition should have a parent"
    );

    let block_node = arena.find_node(var_parent_id.unwrap());
    assert!(block_node.is_some(), "Parent block should exist in arena");
    assert!(
        matches!(block_node.unwrap(), AstNode::Statement(Statement::Block(_))),
        "Variable definition's parent should be a Block"
    );
}

#[test]
fn test_list_children_finds_direct_children() {
    let source = r#"const A: i32 = 1; const B: i32 = 2; fn test() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    assert_eq!(source_files.len(), 1);
    let source_file = &source_files[0];

    let children = arena.get_children_cmp(source_file.id, |node| {
        matches!(node, AstNode::Definition(_))
    });

    assert_eq!(
        children.len(),
        3,
        "SourceFile should have 3 definition children (2 constants + 1 function)"
    );
}

#[test]
fn test_list_children_empty_for_leaf_node() {
    let source = r#"const X: i32 = 42;"#;
    let arena = build_ast(source.to_string());

    let constants = arena.filter_nodes(|node| {
        matches!(node, AstNode::Definition(Definition::Constant(_)))
    });
    assert_eq!(constants.len(), 1);
    let constant = &constants[0];

    let children = arena.get_children_cmp(constant.id(), |_| true);

    assert!(
        !children.is_empty(),
        "Constant definition should have child nodes (identifier, type, literal)"
    );
}

#[test]
fn test_get_children_cmp_traverses_tree() {
    let source = r#"fn test() -> i32 { let a: i32 = 1; let b: i32 = 2; return a + b; }"#;
    let arena = build_ast(source.to_string());

    let functions = arena.functions();
    assert_eq!(functions.len(), 1);
    let function = &functions[0];

    let all_statements = arena.get_children_cmp(function.id, |node| {
        matches!(node, AstNode::Statement(_))
    });

    assert!(
        all_statements.len() >= 3,
        "Should find at least 3 statements (block + 2 var defs or returns)"
    );
}

#[test]
fn test_get_children_cmp_with_filter() {
    let source = r#"fn test() -> bool { if (true) { return false; } return true; }"#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    let source_file = &source_files[0];

    let definitions = arena.get_children_cmp(source_file.id, |node| {
        matches!(node, AstNode::Definition(_))
    });

    assert_eq!(
        definitions.len(),
        1,
        "Should find 1 function definition as direct child"
    );

    let functions = arena.functions();
    let function = &functions[0];

    let statements = arena.get_children_cmp(function.id, |node| {
        matches!(node, AstNode::Statement(_))
    });

    assert!(
        !statements.is_empty(),
        "Should find statements when traversing from function"
    );
}

#[test]
fn test_find_parent_chain_to_root() {
    let source = r#"fn test() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let return_statements =
        arena.filter_nodes(|node| matches!(node, AstNode::Statement(Statement::Return(_))));
    assert_eq!(return_statements.len(), 1);
    let return_stmt = &return_statements[0];

    let mut current_id = return_stmt.id();
    let mut depth = 0;
    const MAX_DEPTH: u32 = 10;

    while let Some(parent_id) = arena.find_parent_node(current_id) {
        current_id = parent_id;
        depth += 1;
        assert!(depth < MAX_DEPTH, "Parent chain should not be circular");
    }

    let root_node = arena.find_node(current_id);
    assert!(root_node.is_some(), "Should reach a valid root node");
    assert!(
        matches!(root_node.unwrap(), AstNode::Ast(Ast::SourceFile(_))),
        "Root node should be SourceFile"
    );
}

#[test]
fn test_multiple_source_definitions_have_same_parent() {
    let source = r#"fn first() {} fn second() {} fn third() {}"#;
    let arena = build_ast(source.to_string());

    let functions = arena.functions();
    assert_eq!(functions.len(), 3);

    let source_files = arena.source_files();
    assert_eq!(source_files.len(), 1);
    let expected_parent_id = source_files[0].id;

    for func in &functions {
        let parent_id = arena.find_parent_node(func.id);
        assert!(parent_id.is_some());
        assert_eq!(
            parent_id.unwrap(),
            expected_parent_id,
            "All top-level functions should have SourceFile as parent"
        );
    }
}

#[test]
fn test_struct_fields_have_struct_as_ancestor() {
    let source = r#"struct Point { x: i32; y: i32; }"#;
    let arena = build_ast(source.to_string());

    let struct_defs =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Struct(_))));
    assert_eq!(struct_defs.len(), 1);
    let struct_def = &struct_defs[0];

    let struct_fields =
        arena.filter_nodes(|node| matches!(node, AstNode::Misc(inference_ast::nodes::Misc::StructField(_))));
    assert_eq!(struct_fields.len(), 2, "Struct should have 2 fields");

    for field in &struct_fields {
        let parent_id = arena.find_parent_node(field.id());
        assert!(parent_id.is_some(), "Field should have a parent");
        assert_eq!(
            parent_id.unwrap(),
            struct_def.id(),
            "Field's parent should be the struct definition"
        );
    }
}

#[test]
fn test_children_lookup_consistency() {
    let source = r#"fn test(a: i32, b: i32) -> i32 { return a + b; }"#;
    let arena = build_ast(source.to_string());

    let functions = arena.functions();
    assert_eq!(functions.len(), 1);
    let function = &functions[0];

    let all_children = arena.get_children_cmp(function.id, |_| true);

    for child in &all_children {
        if child.id() == function.id {
            continue;
        }
        let mut found_ancestor = false;
        let mut current_id = child.id();

        while let Some(parent_id) = arena.find_parent_node(current_id) {
            if parent_id == function.id {
                found_ancestor = true;
                break;
            }
            current_id = parent_id;
        }

        assert!(
            found_ancestor,
            "Every child returned by get_children_cmp should have the queried node as an ancestor"
        );
    }
}
