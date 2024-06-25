#![warn(clippy::pedantic)]

use crate::ast::types::{
    ApplyExpression, Argument, AssertExpression, AssignExpression, BinaryExpression, Block,
    BoolLiteral, ConstantDefinition, ContextDefinition, Definition, Expression,
    ExpressionStatement, ExternalFunctionDefinition, FilterStatement, ForStatement,
    FunctionCallExpression, FunctionDefinition, GenericType, Identifier, IfStatement, Literal,
    Location, MemberAccessExpression, NumberLiteral, ParenthesizedExpression, Position,
    PrefixUnaryExpression, QualifiedType, ReturnStatement, SimpleType, SourceFile, Statement,
    StringLiteral, Type, TypeDefinition, TypeDefinitionStatement, TypeOfExpression, UseDirective,
    VariableDefinitionStatement,
};
use tree_sitter::Node;

pub fn build_ast(root: Node, code: &[u8]) -> SourceFile {
    assert!(
        root.kind() == "source_file",
        "Expected a root node of type {}",
        "source_file"
    );

    let location = get_location(&root);
    let mut ast = SourceFile::new(location);

    for i in 0..root.child_count() {
        let child = root.child(i).unwrap();
        let child_kind = child.kind();

        match child_kind {
            "use_directive" => build_use_directive(&mut ast, &child, code),
            "context_definition" => build_context_definition(&mut ast, &child, code),
            _ => {
                if let Some(definition) = build_definition(&child, code) {
                    ast.add_definition(definition);
                } else {
                    panic!("{}", format!("Unexpected child of type {child_kind}"));
                }
            }
        };
    }
    ast
}

fn build_use_directive(parent: &mut SourceFile, node: &Node, code: &[u8]) {
    let location = get_location(node);
    let mut segments = None;
    let mut imported_types = None;
    let mut from = None;
    let mut cursor = node.walk();

    if let Some(from_literal) = node.child_by_field_name("from_literal") {
        from = Some(build_string_literal(&from_literal, code).value);
    } else {
        let founded_segments = node
            .children_by_field_name("segment", &mut cursor)
            .map(|segment| build_identifier(&segment, code));
        let founded_segments: Vec<Identifier> = founded_segments.collect();
        if !founded_segments.is_empty() {
            segments = Some(founded_segments);
        }
    }

    cursor = node.walk();
    let founded_imported_types = node
        .children_by_field_name("imported_type", &mut cursor)
        .map(|imported_type| build_identifier(&imported_type, code));
    let founded_imported_types: Vec<Identifier> = founded_imported_types.collect();
    if !founded_imported_types.is_empty() {
        imported_types = Some(founded_imported_types);
    }

    parent.add_use_directive(UseDirective {
        location,
        imported_types,
        segments,
        from,
    });
}

fn build_context_definition(parent: &mut SourceFile, node: &Node, code: &[u8]) {
    let location = get_location(node);
    let name = build_identifier(&node.child_by_field_name("name").unwrap(), code);
    let mut definitions = Vec::new();

    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if let Some(definition) = build_definition(&child, code) {
            definitions.push(definition);
        }
    }

    parent.add_context_definition(ContextDefinition {
        location,
        name,
        definitions,
    });
}

fn build_definition(node: &Node, code: &[u8]) -> Option<Definition> {
    let kind = node.kind();
    match kind {
        "constant_definition" => Some(Definition::Constant(build_constant_definition(node, code))),
        "function_definition" => Some(Definition::Function(build_function_definition(node, code))),
        "external_function_definition" => Some(Definition::ExternalFunction(
            build_external_function_definition(node, code),
        )),
        "type_definition_statement" => Some(Definition::Type(build_type_definition(node, code))),
        _ => None,
    }
}

fn build_constant_definition(node: &Node, code: &[u8]) -> ConstantDefinition {
    let location = get_location(node);
    let name = build_identifier(&node.child_by_field_name("name").unwrap(), code);
    let type_ = build_type(&node.child_by_field_name("type").unwrap(), code);
    let value = build_literal(&node.child_by_field_name("value").unwrap(), code);

    ConstantDefinition {
        location,
        name,
        type_,
        value,
    }
}

fn build_function_definition(node: &Node, code: &[u8]) -> FunctionDefinition {
    let location = get_location(node);
    let name = build_identifier(&node.child_by_field_name("name").unwrap(), code);
    let mut arguments = None;
    let mut returns = None;

    let mut cursor = node.walk();
    let founded_arguments = node
        .children_by_field_name("argument", &mut cursor)
        .map(|segment| build_argument(&segment, code));
    let founded_arguments: Vec<Argument> = founded_arguments.collect();
    if !founded_arguments.is_empty() {
        arguments = Some(founded_arguments);
    }

    if let Some(returns_node) = node.child_by_field_name("returns") {
        returns = Some(build_type(&returns_node, code));
    }

    let body = build_block(&node.child_by_field_name("body").unwrap(), code);

    FunctionDefinition {
        location,
        name,
        arguments,
        returns,
        body,
    }
}

fn build_external_function_definition(node: &Node, code: &[u8]) -> ExternalFunctionDefinition {
    let location = get_location(node);
    let name = build_identifier(&node.child_by_field_name("name").unwrap(), code);
    let mut arguments = None;
    let mut returns = None;

    let mut cursor = node.walk();

    let founded_arguments = node
        .children_by_field_name("argument", &mut cursor)
        .map(|segment| build_identifier(&segment, code));
    let founded_arguments: Vec<Identifier> = founded_arguments.collect();
    if !founded_arguments.is_empty() {
        arguments = Some(founded_arguments);
    }

    if let Some(returns_node) = node.child_by_field_name("returns") {
        returns = Some(build_type(&returns_node, code));
    }

    ExternalFunctionDefinition {
        location,
        name,
        arguments,
        returns,
    }
}

fn build_type_definition(node: &Node, code: &[u8]) -> TypeDefinition {
    let location = get_location(node);
    let name = build_identifier(&node.child_by_field_name("name").unwrap(), code);
    let typeof_expression_node = node.child_by_field_name("typeof_expression").unwrap();
    let typeof_expression = build_typeof_expression(&typeof_expression_node, code);

    let type_ = Type::Identifier(Identifier {
        name: typeof_expression.typeref.name,
        location: typeof_expression.location,
    });

    TypeDefinition {
        location,
        name,
        type_,
    }
}

fn build_argument(node: &Node, code: &[u8]) -> Argument {
    let location = get_location(node);
    let name = build_identifier(&node.child_by_field_name("name").unwrap(), code);
    let type_ = build_type(&node.child_by_field_name("type").unwrap(), code);

    Argument {
        location,
        name,
        type_,
    }
}

fn build_block(node: &Node, code: &[u8]) -> Block {
    let location = get_location(node);
    let mut statements = Vec::new();

    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        statements.push(build_statement(&child, code));
    }

    Block {
        location,
        statements,
    }
}

fn build_statement(node: &Node, code: &[u8]) -> Statement {
    match node.kind() {
        "block" => Statement::Block(build_block(node, code)),
        "expression_statement" => Statement::Expression(build_expression_statement(node, code)),
        "return_statement" => Statement::Return(build_return_statement(node, code)),
        "filter_statement" => Statement::Filter(build_filter_statement(node, code)),
        "for_statement" => Statement::For(build_for_statement(node, code)),
        "if_statement" => Statement::If(build_if_statement(node, code)),
        "variable_definition_statement" => {
            Statement::VariableDefinition(build_variable_definition_statement(node, code))
        }
        "type_definition_statement" => {
            Statement::TypeDefinition(build_type_definition_statement(node, code))
        }
        _ => panic!("Unexpected statement type: {}", node.kind()),
    }
}

fn build_expression_statement(node: &Node, code: &[u8]) -> ExpressionStatement {
    let location = get_location(node);
    let expression = build_expression(&node.child(0).unwrap(), code);

    ExpressionStatement {
        location,
        expression,
    }
}

fn build_return_statement(node: &Node, code: &[u8]) -> ReturnStatement {
    let location = get_location(node);
    let expression = build_expression(&node.child_by_field_name("expression").unwrap(), code);

    ReturnStatement {
        location,
        expression,
    }
}

fn build_filter_statement(node: &Node, code: &[u8]) -> FilterStatement {
    let location = get_location(node);
    let block = build_block(&node.child_by_field_name("block").unwrap(), code);

    FilterStatement { location, block }
}

fn build_for_statement(node: &Node, code: &[u8]) -> ForStatement {
    let location = get_location(node);
    let initializer = node
        .child_by_field_name("initializer")
        .map(|n| build_variable_definition_statement(&n, code));
    let condition = node
        .child_by_field_name("condition")
        .map(|n| build_expression(&n, code));
    let update = node
        .child_by_field_name("update")
        .map(|n| build_expression(&n, code));
    let body = Box::new(build_statement(
        &node.child_by_field_name("body").unwrap(),
        code,
    ));

    ForStatement {
        location,
        initializer,
        condition,
        update,
        body,
    }
}

fn build_if_statement(node: &Node, code: &[u8]) -> IfStatement {
    let location = get_location(node);
    let condition = build_expression(&node.child_by_field_name("condition").unwrap(), code);
    let if_arm = build_block(&node.child_by_field_name("if_arm").unwrap(), code);
    let else_arm = node
        .child_by_field_name("else_arm")
        .map(|n| build_block(&n, code));

    IfStatement {
        location,
        condition,
        if_arm,
        else_arm,
    }
}

fn build_variable_definition_statement(node: &Node, code: &[u8]) -> VariableDefinitionStatement {
    let location = get_location(node);
    let name = build_identifier(&node.child_by_field_name("name").unwrap(), code);
    let type_ = build_type(&node.child_by_field_name("type").unwrap(), code);
    let value = node
        .child_by_field_name("value")
        .map(|n| build_expression(&n, code));

    VariableDefinitionStatement {
        location,
        name,
        type_,
        value,
    }
}

fn build_type_definition_statement(node: &Node, code: &[u8]) -> TypeDefinitionStatement {
    let location = get_location(node);
    let name = build_identifier(&node.child_by_field_name("name").unwrap(), code);
    let type_ = build_type(&node.child_by_field_name("type").unwrap(), code);

    TypeDefinitionStatement {
        location,
        name,
        type_,
    }
}

fn build_expression(node: &Node, code: &[u8]) -> Expression {
    match node.kind() {
        "assign_expression" => Expression::Assign(build_assign_expression(node, code)),
        "member_access_expression" => {
            Expression::MemberAccess(build_member_access_expression(node, code))
        }
        "function_call_expression" => {
            Expression::FunctionCall(build_function_call_expression(node, code))
        }
        "prefix_unary_expression" => {
            Expression::PrefixUnary(build_prefix_unary_expression(node, code))
        }
        "assert_expression" => Expression::Assert(build_assert_expression(node, code)),
        "apply_expression" => Expression::Apply(build_apply_expression(node, code)),
        "parenthesized_expression" => {
            Expression::Parenthesized(build_parenthesized_expression(node, code))
        }
        "typeof_expression" => Expression::TypeOf(build_typeof_expression(node, code)),
        "binary_expression" => Expression::Binary(build_binary_expression(node, code)),
        "bool_literal" | "string_literal" | "number_literal" => {
            Expression::Literal(build_literal(node, code))
        }
        "identifier" => Expression::Identifier(build_identifier(node, code)),
        "generic_name" => Expression::Type(Type::Generic(build_generic_type(node, code))),
        _ => panic!("Unexpected expression type: {}", node.kind()),
    }
}

fn build_assign_expression(node: &Node, code: &[u8]) -> AssignExpression {
    let location = get_location(node);
    let left = Box::new(build_expression(
        &node.child_by_field_name("left").unwrap(),
        code,
    ));
    let right = Box::new(build_expression(
        &node.child_by_field_name("right").unwrap(),
        code,
    ));

    AssignExpression {
        location,
        left,
        right,
    }
}

fn build_member_access_expression(node: &Node, code: &[u8]) -> MemberAccessExpression {
    let location = get_location(node);
    let expression = Box::new(build_expression(
        &node.child_by_field_name("expression").unwrap(),
        code,
    ));
    let name = build_identifier(&node.child_by_field_name("name").unwrap(), code);

    MemberAccessExpression {
        location,
        expression,
        name,
    }
}

fn build_function_call_expression(node: &Node, code: &[u8]) -> FunctionCallExpression {
    let location = get_location(node);
    let function = Box::new(build_expression(
        &node.child_by_field_name("function").unwrap(),
        code,
    ));
    let mut arguments = Vec::new();

    for i in 0..node.child_by_field_name("arguments").unwrap().child_count() {
        let child = node
            .child_by_field_name("arguments")
            .unwrap()
            .child(i)
            .unwrap();
        arguments.push(build_expression(&child, code));
    }

    FunctionCallExpression {
        location,
        function,
        arguments,
    }
}

fn build_prefix_unary_expression(node: &Node, code: &[u8]) -> PrefixUnaryExpression {
    let location = get_location(node);
    let expression = Box::new(build_expression(
        &node.child_by_field_name("expression").unwrap(),
        code,
    ));

    PrefixUnaryExpression {
        location,
        expression,
    }
}

fn build_assert_expression(node: &Node, code: &[u8]) -> AssertExpression {
    let location = get_location(node);
    let expression = Box::new(build_expression(
        &node.child_by_field_name("expression").unwrap(),
        code,
    ));

    AssertExpression {
        location,
        expression,
    }
}

fn build_apply_expression(node: &Node, code: &[u8]) -> ApplyExpression {
    let location = get_location(node);
    let function_call = Box::new(build_function_call_expression(
        &node.child_by_field_name("function_call").unwrap(),
        code,
    ));

    ApplyExpression {
        location,
        function_call,
    }
}

fn build_parenthesized_expression(node: &Node, code: &[u8]) -> ParenthesizedExpression {
    let location = get_location(node);
    let expression = Box::new(build_expression(
        &node.child_by_field_name("expression").unwrap(),
        code,
    ));

    ParenthesizedExpression {
        location,
        expression,
    }
}

fn build_typeof_expression(node: &Node, code: &[u8]) -> TypeOfExpression {
    let location = get_location(node);
    let typeref = build_identifier(&node.child_by_field_name("typeref").unwrap(), code);

    TypeOfExpression { location, typeref }
}

fn build_binary_expression(node: &Node, code: &[u8]) -> BinaryExpression {
    let location = get_location(node);
    let left = Box::new(build_expression(
        &node.child_by_field_name("left").unwrap(),
        code,
    ));
    let operator = node.utf8_text(code).unwrap().to_string();
    let right = Box::new(build_expression(
        &node.child_by_field_name("right").unwrap(),
        code,
    ));

    BinaryExpression {
        location,
        left,
        operator,
        right,
    }
}

fn build_literal(node: &Node, code: &[u8]) -> Literal {
    match node.kind() {
        "bool_literal" => Literal::Bool(build_bool_literal(node, code)),
        "string_literal" => Literal::String(build_string_literal(node, code)),
        "number_literal" => Literal::Number(build_number_literal(node, code)),
        _ => panic!("Unexpected literal type: {}", node.kind()),
    }
}

fn build_bool_literal(node: &Node, code: &[u8]) -> BoolLiteral {
    let location = get_location(node);
    let value = match node.utf8_text(code).unwrap() {
        "true" => true,
        "false" => false,
        _ => panic!("Unexpected boolean literal value"),
    };

    BoolLiteral { location, value }
}

fn build_string_literal(node: &Node, code: &[u8]) -> StringLiteral {
    let location = get_location(node);
    let value = node.utf8_text(code).unwrap().to_string();

    StringLiteral { location, value }
}

fn build_number_literal(node: &Node, code: &[u8]) -> NumberLiteral {
    let location = get_location(node);
    let value = node.utf8_text(code).unwrap().parse::<i64>().unwrap();

    NumberLiteral { location, value }
}

fn build_type(node: &Node, code: &[u8]) -> Type {
    let node_kind = node.kind();
    match node_kind {
        "type_i32" | "type_i64" | "type_u32" | "type_u64" | "type_bool" | "type_unit" => {
            Type::Simple(build_simple_type(node, code))
        }
        "generic_type" => Type::Generic(build_generic_type(node, code)),
        "qualified_type" => Type::Qualified(build_qualified_type(node, code)),
        "identifier" => Type::Identifier(build_identifier(node, code)),
        _ => panic!("Unexpected type: {}", node.kind()),
    }
}

fn build_simple_type(node: &Node, code: &[u8]) -> SimpleType {
    let location = get_location(node);
    let name = node.utf8_text(code).unwrap().to_string();

    SimpleType { location, name }
}

fn build_generic_type(node: &Node, code: &[u8]) -> GenericType {
    let location = get_location(node);
    let base = build_identifier(&node.child_by_field_name("base_type").unwrap(), code);

    let n2 = node.child(1).unwrap();
    let n2_kind = n2.kind();

    let mut cursor = n2.walk();

    let types = n2
        .children_by_field_name("type", &mut cursor)
        .map(|segment| build_type(&segment, code));
    let parameters: Vec<Type> = types.collect();

    GenericType {
        location,
        base,
        parameters,
    }
}

fn build_qualified_type(node: &Node, code: &[u8]) -> QualifiedType {
    let location = get_location(node);
    let qualifier = build_identifier(&node.child_by_field_name("qualifier").unwrap(), code);
    let name = build_identifier(&node.child_by_field_name("name").unwrap(), code);

    QualifiedType {
        location,
        qualifier,
        name,
    }
}

fn build_identifier(node: &Node, code: &[u8]) -> Identifier {
    let location = get_location(node);
    let name = node.utf8_text(code).unwrap().to_string();

    Identifier { location, name }
}

fn get_location(node: &Node) -> Location {
    Location {
        start: Position {
            row: node.start_position().row,
            column: node.start_position().column,
        },
        end: Position {
            row: node.end_position().row,
            column: node.end_position().column,
        },
    }
}
