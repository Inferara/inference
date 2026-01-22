use crate::utils::build_ast;
use inference_ast::arena::Arena;
use inference_ast::nodes::{Ast, AstNode, Definition, Identifier, Location, Statement};

/// Tests for Arena's parent-child lookup functionality with FxHashMap-based O(1) lookups.

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
    always!(parent_id.is_some(), "Function should have a parent");
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
    always!(
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
    always!(
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

    let statements = arena
        .filter_nodes(|node| matches!(node, AstNode::Statement(Statement::VariableDefinition(_))));
    assert_eq!(statements.len(), 1, "Expected 1 variable definition");

    let var_def = &statements[0];
    let var_parent_id = arena.find_parent_node(var_def.id());
    always!(
        var_parent_id.is_some(),
        "Variable definition should have a parent"
    );

    let block_node = arena.find_node(var_parent_id.unwrap());
    always!(block_node.is_some(), "Parent block should exist in arena");
    always!(
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

    let constants =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Constant(_))));
    assert_eq!(constants.len(), 1);
    let constant = &constants[0];

    let children = arena.get_children_cmp(constant.id(), |_| true);

    always!(
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

    let all_statements =
        arena.get_children_cmp(function.id, |node| matches!(node, AstNode::Statement(_)));

    always!(
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

    let statements =
        arena.get_children_cmp(function.id, |node| matches!(node, AstNode::Statement(_)));

    always!(
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
        always!(depth < MAX_DEPTH, "Parent chain should not be circular");
    }

    let root_node = arena.find_node(current_id);
    always!(root_node.is_some(), "Should reach a valid root node");
    always!(
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
        always!(parent_id.is_some());
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

    let struct_fields = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::Misc(inference_ast::nodes::Misc::StructField(_))
        )
    });
    assert_eq!(struct_fields.len(), 2, "Struct should have 2 fields");

    for field in &struct_fields {
        let parent_id = arena.find_parent_node(field.id());
        always!(parent_id.is_some(), "Field should have a parent");
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

        always!(
            found_ancestor,
            "Every child returned by get_children_cmp should have the queried node as an ancestor"
        );
    }
}

/// Tests for Arena's convenience API methods: `find_source_file_for_node` and `get_node_source`.
/// These methods provide efficient source text retrieval for any AST node.

#[test]
fn test_get_node_source_returns_function_source() {
    let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
    let arena = build_ast(source.to_string());

    let functions = arena.functions();
    assert_eq!(functions.len(), 1);
    let function = &functions[0];

    let function_source = arena.get_node_source(function.id);
    always!(
        function_source.is_some(),
        "Function source should be retrievable"
    );
    assert_eq!(
        function_source.unwrap(),
        "fn add(a: i32, b: i32) -> i32 { return a + b; }",
        "Function source should match the original source text"
    );
}

#[test]
fn test_get_node_source_for_nested_identifier() {
    let source = r#"fn test() -> i32 { let value: i32 = 42; return value; }"#;
    let arena = build_ast(source.to_string());

    let identifiers = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::Expression(inference_ast::nodes::Expression::Identifier(_))
        )
    });

    let value_identifier = identifiers.iter().find(|node| {
        if let AstNode::Expression(inference_ast::nodes::Expression::Identifier(ident)) = node {
            ident.name == "value"
        } else {
            false
        }
    });

    always!(value_identifier.is_some(), "Should find 'value' identifier");
    let ident_source = arena.get_node_source(value_identifier.unwrap().id());
    always!(
        ident_source.is_some(),
        "Identifier source should be retrievable"
    );
    assert_eq!(
        ident_source.unwrap(),
        "value",
        "Identifier source should match"
    );
}

#[test]
fn test_get_node_source_for_source_file() {
    let source = r#"fn main() -> i32 { return 0; }"#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    assert_eq!(source_files.len(), 1);
    let source_file = &source_files[0];

    let file_source = arena.get_node_source(source_file.id);
    always!(
        file_source.is_some(),
        "SourceFile source should be retrievable"
    );
    assert_eq!(
        file_source.unwrap(),
        source,
        "SourceFile source should return the entire source text"
    );
}

#[test]
fn test_get_node_source_nonexistent_returns_none() {
    let source = r#"fn test() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let nonexistent_id = u32::MAX - 1;
    let result = arena.get_node_source(nonexistent_id);
    always!(result.is_none(), "Non-existent node ID should return None");
}

#[test]
fn test_get_node_source_for_binary_expression() {
    let source = r#"fn calc() -> i32 { return 10 + 20; }"#;
    let arena = build_ast(source.to_string());

    let binary_expressions = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::Expression(inference_ast::nodes::Expression::Binary(_))
        )
    });

    always!(
        !binary_expressions.is_empty(),
        "Should find binary expression"
    );
    let binary_expr = &binary_expressions[0];

    let expr_source = arena.get_node_source(binary_expr.id());
    always!(
        expr_source.is_some(),
        "Binary expression source should be retrievable"
    );
    assert_eq!(
        expr_source.unwrap(),
        "10 + 20",
        "Binary expression source should match"
    );
}

#[test]
fn test_get_node_source_for_return_statement() {
    let source = r#"fn test() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let return_statements =
        arena.filter_nodes(|node| matches!(node, AstNode::Statement(Statement::Return(_))));

    assert_eq!(return_statements.len(), 1, "Should find 1 return statement");
    let return_stmt = &return_statements[0];

    let stmt_source = arena.get_node_source(return_stmt.id());
    always!(
        stmt_source.is_some(),
        "Return statement source should be retrievable"
    );
    assert_eq!(
        stmt_source.unwrap(),
        "return 42;",
        "Return statement source should match"
    );
}

#[test]
fn test_find_source_file_for_function_returns_correct_id() {
    let source = r#"fn test() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let functions = arena.functions();
    assert_eq!(functions.len(), 1);
    let function = &functions[0];

    let source_files = arena.source_files();
    assert_eq!(source_files.len(), 1);
    let expected_source_file_id = source_files[0].id;

    let found_source_file_id = arena.find_source_file_for_node(function.id);
    always!(
        found_source_file_id.is_some(),
        "Should find SourceFile for function"
    );
    assert_eq!(
        found_source_file_id.unwrap(),
        expected_source_file_id,
        "Should return the correct SourceFile ID"
    );
}

#[test]
fn test_find_source_file_for_source_file_returns_self() {
    let source = r#"fn test() {}"#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    assert_eq!(source_files.len(), 1);
    let source_file = &source_files[0];

    let found_id = arena.find_source_file_for_node(source_file.id);
    always!(found_id.is_some(), "SourceFile should find itself");
    assert_eq!(
        found_id.unwrap(),
        source_file.id,
        "SourceFile should return its own ID when queried"
    );
}

#[test]
fn test_find_source_file_for_nonexistent_returns_none() {
    let source = r#"fn test() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let nonexistent_id = u32::MAX - 1;
    let result = arena.find_source_file_for_node(nonexistent_id);
    always!(result.is_none(), "Non-existent node ID should return None");
}

#[test]
fn test_get_node_source_zero_length_span() {
    let source = r#"fn test() {}"#;
    let arena = build_ast(source.to_string());

    let functions = arena.functions();
    assert_eq!(functions.len(), 1);
    let function = &functions[0];

    let func_source = arena.get_node_source(function.id);
    always!(
        func_source.is_some(),
        "Function with empty body should still have retrievable source"
    );
    assert_eq!(
        func_source.unwrap(),
        "fn test() {}",
        "Function source should match"
    );
}

#[test]
fn test_find_source_file_for_deeply_nested_node() {
    let source =
        r#"fn outer() -> i32 { if (true) { let x: i32 = 1 + 2 + 3; return x; } return 0; }"#;
    let arena = build_ast(source.to_string());

    let source_files = arena.source_files();
    assert_eq!(source_files.len(), 1);
    let expected_source_file_id = source_files[0].id;

    let binary_expressions = arena.filter_nodes(|node| {
        matches!(
            node,
            AstNode::Expression(inference_ast::nodes::Expression::Binary(_))
        )
    });

    always!(
        !binary_expressions.is_empty(),
        "Should find binary expressions"
    );

    for expr in &binary_expressions {
        let found_id = arena.find_source_file_for_node(expr.id());
        always!(
            found_id.is_some(),
            "Deeply nested expression should have SourceFile ancestor"
        );
        assert_eq!(
            found_id.unwrap(),
            expected_source_file_id,
            "All nodes should have the same SourceFile ancestor"
        );
    }
}

#[test]
fn test_get_node_source_for_variable_definition() {
    let source = r#"fn test() { let counter: i32 = 100; }"#;
    let arena = build_ast(source.to_string());

    let var_definitions = arena
        .filter_nodes(|node| matches!(node, AstNode::Statement(Statement::VariableDefinition(_))));

    assert_eq!(
        var_definitions.len(),
        1,
        "Should find 1 variable definition"
    );
    let var_def = &var_definitions[0];

    let def_source = arena.get_node_source(var_def.id());
    always!(
        def_source.is_some(),
        "Variable definition source should be retrievable"
    );
    assert_eq!(
        def_source.unwrap(),
        "let counter: i32 = 100;",
        "Variable definition source should match"
    );
}

#[test]
fn test_get_node_source_for_struct_definition() {
    let source = r#"struct Point { x: i32; y: i32; }"#;
    let arena = build_ast(source.to_string());

    let struct_defs =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Struct(_))));

    assert_eq!(struct_defs.len(), 1, "Should find 1 struct definition");
    let struct_def = &struct_defs[0];

    let struct_source = arena.get_node_source(struct_def.id());
    always!(
        struct_source.is_some(),
        "Struct definition source should be retrievable"
    );
    assert_eq!(
        struct_source.unwrap(),
        "struct Point { x: i32; y: i32; }",
        "Struct definition source should match"
    );
}

#[test]
fn test_get_node_source_multiple_functions() {
    let source = r#"fn first() -> i32 { return 1; } fn second() -> i32 { return 2; }"#;
    let arena = build_ast(source.to_string());

    let functions = arena.functions();
    assert_eq!(functions.len(), 2, "Should find 2 functions");

    let first_source = arena.get_node_source(functions[0].id);
    let second_source = arena.get_node_source(functions[1].id);

    always!(
        first_source.is_some(),
        "First function source should be retrievable"
    );
    always!(
        second_source.is_some(),
        "Second function source should be retrievable"
    );

    let sources: Vec<&str> = vec![first_source.unwrap(), second_source.unwrap()];
    always!(
        sources.contains(&"fn first() -> i32 { return 1; }"),
        "Should find first function source"
    );
    always!(
        sources.contains(&"fn second() -> i32 { return 2; }"),
        "Should find second function source"
    );
}

/// Tests for `list_type_definitions()` method

#[test]
fn test_list_type_definitions_returns_type_aliases() {
    let source = r#"type MyInt = i32;"#;
    let arena = build_ast(source.to_string());

    let type_defs = arena.list_type_definitions();
    assert_eq!(type_defs.len(), 1, "Should find 1 type definition");
    assert_eq!(type_defs[0].name.name, "MyInt");
}

#[test]
fn test_list_type_definitions_multiple() {
    let source = r#"type MyInt = i32;
type MyBool = bool;
type MyArray = [i32; 10];"#;
    let arena = build_ast(source.to_string());

    let type_defs = arena.list_type_definitions();
    assert_eq!(type_defs.len(), 3, "Should find 3 type definitions");

    let names: Vec<&str> = type_defs.iter().map(|td| td.name.name.as_str()).collect();
    always!(names.contains(&"MyInt"));
    always!(names.contains(&"MyBool"));
    always!(names.contains(&"MyArray"));
}

#[test]
fn test_list_type_definitions_empty_when_no_types() {
    let source = r#"fn test() -> i32 { return 42; }"#;
    let arena = build_ast(source.to_string());

    let type_defs = arena.list_type_definitions();
    always!(type_defs.is_empty(), "Should find no type definitions");
}

#[test]
fn test_list_type_definitions_mixed_with_other_definitions() {
    let source = r#"const X: i32 = 42;
type MyInt = i32;
fn test() -> i32 { return X; }
type MyBool = bool;"#;
    let arena = build_ast(source.to_string());

    let type_defs = arena.list_type_definitions();
    assert_eq!(
        type_defs.len(),
        2,
        "Should find 2 type definitions among mixed definitions"
    );
}

/// Tests for edge cases in `get_node_source()` - invalid offsets and edge cases

#[test]
fn test_get_node_source_with_manually_constructed_arena_invalid_source_file() {
    let arena = Arena::default();
    let result = arena.get_node_source(12345);
    always!(result.is_none(), "Empty arena should return None");
}

#[test]
fn test_find_source_file_for_nonexistent_node_in_empty_arena() {
    let arena = Arena::default();
    let result = arena.find_source_file_for_node(99999);
    always!(
        result.is_none(),
        "Non-existent node in empty arena should return None"
    );
}

#[test]
fn test_find_parent_node_in_empty_arena() {
    let arena = Arena::default();
    let result = arena.find_parent_node(12345);
    always!(
        result.is_none(),
        "Empty arena should return None for parent lookup"
    );
}

#[test]
fn test_find_node_in_empty_arena() {
    let arena = Arena::default();
    let result = arena.find_node(12345);
    always!(
        result.is_none(),
        "Empty arena should return None for find_node"
    );
}

#[test]
fn test_get_children_cmp_on_nonexistent_node() {
    let arena = Arena::default();
    let children = arena.get_children_cmp(99999, |_| true);
    always!(
        children.is_empty(),
        "Non-existent node should return empty children"
    );
}

#[test]
fn test_filter_nodes_on_empty_arena() {
    let arena = Arena::default();
    let filtered = arena.filter_nodes(|_| true);
    always!(
        filtered.is_empty(),
        "Empty arena should return no filtered nodes"
    );
}

#[test]
fn test_source_files_on_empty_arena() {
    let arena = Arena::default();
    let source_files = arena.source_files();
    always!(
        source_files.is_empty(),
        "Empty arena should return no source files"
    );
}

#[test]
fn test_functions_on_empty_arena() {
    let arena = Arena::default();
    let functions = arena.functions();
    always!(
        functions.is_empty(),
        "Empty arena should return no functions"
    );
}

#[test]
fn test_list_type_definitions_on_empty_arena() {
    let arena = Arena::default();
    let type_defs = arena.list_type_definitions();
    always!(
        type_defs.is_empty(),
        "Empty arena should return no type definitions"
    );
}

/// Tests for Arena::clone() functionality

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
        arena.functions().len(),
        cloned_arena.functions().len(),
        "Cloned arena should have same number of functions"
    );
}

/// Tests for Location with edge cases

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

/// Tests for Arena::add_node functionality

#[test]
fn test_add_node_valid_succeeds() {
    use std::rc::Rc;

    let mut arena = Arena::default();
    let identifier = Rc::new(Identifier::new(1, "valid".to_string(), Location::default()));
    let node = AstNode::Expression(inference_ast::nodes::Expression::Identifier(identifier));

    arena.add_node(node, u32::MAX);
    always!(
        arena.find_node(1).is_some(),
        "Added node should be retrievable"
    );
}

#[test]
fn test_add_node_with_parent_creates_relationship() {
    use std::rc::Rc;

use always_assert::always;
    let mut arena = Arena::default();

    let parent_ident = Rc::new(Identifier::new(
        1,
        "parent".to_string(),
        Location::default(),
    ));
    let parent_node =
        AstNode::Expression(inference_ast::nodes::Expression::Identifier(parent_ident));
    arena.add_node(parent_node, u32::MAX);

    let child_ident = Rc::new(Identifier::new(2, "child".to_string(), Location::default()));
    let child_node = AstNode::Expression(inference_ast::nodes::Expression::Identifier(child_ident));
    arena.add_node(child_node, 1);

    assert_eq!(
        arena.find_parent_node(2),
        Some(1),
        "Child should have parent"
    );
    assert_eq!(
        arena.find_parent_node(1),
        None,
        "Root node should have no parent"
    );
}
