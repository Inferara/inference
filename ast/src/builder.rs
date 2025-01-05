#![warn(clippy::pedantic)]

use crate::types::{
    ArrayIndexAccessExpression, ArrayLiteral, AssertStatement, AssignExpression, BinaryExpression,
    Block, BoolLiteral, BreakStatement, ConstantDefinition, Definition, EnumDefinition, Expression,
    ExpressionStatement, ExternalFunctionDefinition, FunctionCallExpression, FunctionDefinition,
    FunctionType, GenericType, Identifier, IfStatement, Literal, Location, LoopStatement,
    MemberAccessExpression, NumberLiteral, OperatorKind, Parameter, ParenthesizedExpression,
    Position, PrefixUnaryExpression, QualifiedName, ReturnStatement, SimpleType, SourceFile,
    SpecDefinition, Statement, StringLiteral, StructDefinition, StructField, Type, TypeArray,
    TypeDefinition, TypeDefinitionStatement, TypeQualifiedName, UnaryOperatorKind, UnitLiteral,
    UseDirective, UzumakiExpression, VariableDefinitionStatement,
};
use tree_sitter::Node;

use super::types::BlockType;

/// Builds the AST from the root node and source code.
///
/// # Panics
///
/// This function will panioc if the `root` node is not of type `source_file`.
/// This function will panic if the `source_file` is malformed and a valid AST cannot be constructed.
#[must_use]
pub fn build_ast(root: Node, code: &[u8]) -> SourceFile {
    assert!(
        root.kind() == "source_file",
        "Expected a root node of type `source_file`"
    );

    let location = get_location(&root);
    let mut ast = SourceFile::new(location);

    for i in 0..root.child_count() {
        if let Some(child) = root.child(i) {
            let child_kind = child.kind();

            match child_kind {
                "use_directive" => build_use_directive(&mut ast, &child, code),
                _ => {
                    if let Some(definition) = build_definition(&child, code) {
                        ast.add_definition(definition);
                    } else {
                        panic!("{}", format!("Unexpected child of type {child_kind}"));
                    }
                }
            };
        }
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

fn build_spec_definition(node: &Node, code: &[u8]) -> SpecDefinition {
    let location = get_location(node);
    let name = build_identifier(&node.child_by_field_name("name").unwrap(), code);
    let mut definitions = Vec::new();

    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if let Some(definition) = build_definition(&child, code) {
            definitions.push(definition);
        }
    }

    SpecDefinition {
        location,
        name,
        definitions,
    }
}

fn build_enum_definition(node: &Node, code: &[u8]) -> EnumDefinition {
    let location = get_location(node);
    let name = build_identifier(&node.child_by_field_name("name").unwrap(), code);
    let mut variants = Vec::new();

    let mut cursor = node.walk();
    let founded_variants = node
        .children_by_field_name("variant", &mut cursor)
        .map(|segment| build_identifier(&segment, code));
    let founded_variants: Vec<Identifier> = founded_variants.collect();
    if !founded_variants.is_empty() {
        variants = founded_variants;
    }

    EnumDefinition {
        location,
        name,
        variants,
    }
}

fn build_definition(node: &Node, code: &[u8]) -> Option<Definition> {
    let kind = node.kind();
    match kind {
        "spec_definition" => Some(Definition::Spec(build_spec_definition(node, code))),
        "struct_definition" => Some(Definition::Struct(build_struct_definition(node, code))),
        "enum_definition" => Some(Definition::Enum(build_enum_definition(node, code))),
        "constant_definition" => Some(Definition::Constant(build_constant_definition(node, code))),
        "function_definition" => Some(Definition::Function(build_function_definition(node, code))),
        "external_function_definition" => Some(Definition::ExternalFunction(
            build_external_function_definition(node, code),
        )),
        "type_definition_statement" => Some(Definition::Type(build_type_definition(node, code))),
        _ => None,
    }
}

fn build_struct_definition(node: &Node, code: &[u8]) -> StructDefinition {
    let location = get_location(node);
    let name = build_identifier(&node.child_by_field_name("struct_name").unwrap(), code);
    let mut fields = Vec::new();

    let mut cursor = node.walk();
    let founded_fields = node
        .children_by_field_name("field", &mut cursor)
        .map(|segment| build_struct_field(&segment, code));
    let founded_fields: Vec<StructField> = founded_fields.collect();
    if !founded_fields.is_empty() {
        fields = founded_fields;
    }

    cursor = node.walk();
    let founded_methods = node
        .children_by_field_name("method", &mut cursor)
        .map(|segment| build_function_definition(&segment, code));
    let methods: Vec<FunctionDefinition> = founded_methods.collect();

    StructDefinition {
        location,
        name,
        fields,
        methods,
    }
}

fn build_struct_field(node: &Node, code: &[u8]) -> StructField {
    let location = get_location(node);
    let name = build_identifier(&node.child_by_field_name("name").unwrap(), code);
    let type_ = build_type(&node.child_by_field_name("type").unwrap(), code);

    StructField {
        location,
        name,
        type_,
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

    if let Some(argument_list_node) = node.child_by_field_name("argument_list") {
        let mut cursor = argument_list_node.walk();
        let founded_arguments = argument_list_node
            .children_by_field_name("argument", &mut cursor)
            .map(|segment| build_argument(&segment, code));
        let founded_arguments: Vec<Parameter> = founded_arguments.collect();
        if !founded_arguments.is_empty() {
            arguments = Some(founded_arguments);
        }
    }

    if let Some(returns_node) = node.child_by_field_name("returns") {
        returns = Some(build_type(&returns_node, code));
    }
    let body = build_block(&node.child_by_field_name("body").unwrap(), code);

    FunctionDefinition {
        location,
        name,
        parameters: arguments,
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
    let type_ = build_type(&node.child_by_field_name("type").unwrap(), code);

    TypeDefinition {
        location,
        name,
        type_,
    }
}

fn build_argument(node: &Node, code: &[u8]) -> Parameter {
    let location = get_location(node);
    let name = build_identifier(&node.child_by_field_name("name").unwrap(), code);
    let type_ = build_type(&node.child_by_field_name("type").unwrap(), code);

    Parameter {
        location,
        name,
        type_,
    }
}

fn build_block(node: &Node, code: &[u8]) -> BlockType {
    let location = get_location(node);

    match node.kind() {
        "block" => BlockType::Block(Block {
            location,
            statements: build_block_statements(node, code),
        }),
        "assume_block" => BlockType::Assume(Block {
            location,
            statements: build_block_statements(&node.child_by_field_name("body").unwrap(), code),
        }),
        "forall_block" => BlockType::Forall(Block {
            location,
            statements: build_block_statements(&node.child_by_field_name("body").unwrap(), code),
        }),
        "exists_block" => BlockType::Exists(Block {
            location,
            statements: build_block_statements(&node.child_by_field_name("body").unwrap(), code),
        }),
        "unique_block" => BlockType::Unique(Block {
            location,
            statements: build_block_statements(&node.child_by_field_name("body").unwrap(), code),
        }),
        _ => panic!("Unexpected block type: {}", node.kind()),
    }
}

fn build_block_statements(node: &Node, code: &[u8]) -> Vec<Statement> {
    let mut statements = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        statements.push(build_statement(&child, code));
    }

    statements
}

fn build_statement(node: &Node, code: &[u8]) -> Statement {
    match node.kind() {
        "block" | "forall_block" | "assume_block" | "exists_block" | "unique_block" => {
            Statement::Block(build_block(node, code))
        }
        "expression_statement" => Statement::Expression(build_expression_statement(node, code)),
        "return_statement" => Statement::Return(build_return_statement(node, code)),
        "loop_statement" => Statement::Loop(build_loop_statement(node, code)),
        "if_statement" => Statement::If(build_if_statement(node, code)),
        "variable_definition_statement" => {
            Statement::VariableDefinition(build_variable_definition_statement(node, code))
        }
        "type_definition_statement" => {
            Statement::TypeDefinition(build_type_definition_statement(node, code))
        }
        "assert_statement" => Statement::Assert(build_assert_statement(node, code)),
        "break_statement" => Statement::Break(build_break_statement(node, code)),
        "constant_definition" => {
            Statement::ConstantDefinition(build_constant_definition(node, code))
        }
        _ => panic!(
            "Unexpected statement type: {}, {}",
            node.kind(),
            get_location(node)
        ),
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

fn build_loop_statement(node: &Node, code: &[u8]) -> LoopStatement {
    let location = get_location(node);
    let condition = node
        .child_by_field_name("condition")
        .map(|n| build_expression(&n, code));
    let body = build_block(&node.child_by_field_name("body").unwrap(), code);

    LoopStatement {
        location,
        condition,
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
    let is_undef = node.child_by_field_name("undef").is_some();

    VariableDefinitionStatement {
        location,
        name,
        type_,
        value,
        is_undef,
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
    let node_kind = node.kind();
    match node_kind {
        "assign_expression" => Expression::Assign(Box::new(build_assign_expression(node, code))),
        "array_index_access_expression" => {
            Expression::ArrayIndexAccess(Box::new(build_array_index_access_expression(node, code)))
        }
        "member_access_expression" => {
            Expression::MemberAccess(Box::new(build_member_access_expression(node, code)))
        }
        "function_call_expression" => {
            Expression::FunctionCall(Box::new(build_function_call_expression(node, code)))
        }
        "prefix_unary_expression" => {
            Expression::PrefixUnary(Box::new(build_prefix_unary_expression(node, code)))
        }
        "parenthesized_expression" => {
            Expression::Parenthesized(Box::new(build_parenthesized_expression(node, code)))
        }
        "binary_expression" => Expression::Binary(Box::new(build_binary_expression(node, code))),
        "bool_literal" | "string_literal" | "number_literal" | "array_literal" | "unit_literal" => {
            Expression::Literal(build_literal(node, code))
        }
        "uzumaki_keyword" => Expression::Uzumaki(build_uzumaki_expression(node, code)),
        "identifier" => Expression::Identifier(build_identifier(node, code)),
        _ => Expression::Type(Box::new(build_type(node, code))),
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

fn build_array_index_access_expression(node: &Node, code: &[u8]) -> ArrayIndexAccessExpression {
    let location = get_location(node);
    let array = Box::new(build_expression(&node.named_child(0).unwrap(), code));
    let index = Box::new(build_expression(&node.named_child(1).unwrap(), code));

    ArrayIndexAccessExpression {
        location,
        array,
        index,
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
    let mut arguments = None;
    let mut argument_name_expression_map: Vec<(Identifier, Expression)> = vec![];
    let mut cursor = node.walk();
    let mut argument_name = None;
    for child in node.children(&mut cursor) {
        match child.kind() {
            "argument_name" => {
                argument_name = Some(build_identifier(&child, code));
            }
            "argument" => {
                let argument_expression = build_expression(&child, code);
                if let Some(name) = argument_name.take() {
                    argument_name_expression_map.push((name, argument_expression));
                } else {
                    argument_name_expression_map.push((Identifier::default(), argument_expression));
                }
            }
            _ => {}
        }
    }

    if !argument_name_expression_map.is_empty() {
        arguments = Some(argument_name_expression_map);
    }

    FunctionCallExpression {
        location,
        function,
        arguments,
    }
}

fn build_prefix_unary_expression(node: &Node, code: &[u8]) -> PrefixUnaryExpression {
    let location = get_location(node);
    let expression = Box::new(build_expression(&node.child(1).unwrap(), code));

    let operator_node = node.child_by_field_name("operator").unwrap();
    let operator = match operator_node.kind() {
        "unary_not" => UnaryOperatorKind::Neg,
        _ => panic!("Unexpected operator node"),
    };

    PrefixUnaryExpression {
        location,
        expression,
        operator,
    }
}

fn build_assert_statement(node: &Node, code: &[u8]) -> AssertStatement {
    let location = get_location(node);
    let expression = Box::new(build_expression(&node.child(1).unwrap(), code));

    AssertStatement {
        location,
        expression,
    }
}

fn build_break_statement(node: &Node, _: &[u8]) -> BreakStatement {
    let location = get_location(node);
    BreakStatement { location }
}

fn build_parenthesized_expression(node: &Node, code: &[u8]) -> ParenthesizedExpression {
    let location = get_location(node);
    let expression = Box::new(build_expression(&node.child(1).unwrap(), code));

    ParenthesizedExpression {
        location,
        expression,
    }
}

fn build_binary_expression(node: &Node, code: &[u8]) -> BinaryExpression {
    let location = get_location(node);
    let left = Box::new(build_expression(
        &node.child_by_field_name("left").unwrap(),
        code,
    ));

    let operator_node = node.child_by_field_name("operator").unwrap();
    let operator_kind = operator_node.kind();
    let operator = match operator_kind {
        "pow_operator" => OperatorKind::Pow,
        "and_operator" => OperatorKind::And,
        "or_operator" => OperatorKind::Or,
        "add_operator" => OperatorKind::Add,
        "sub_operator" => OperatorKind::Sub,
        "mul_operator" => OperatorKind::Mul,
        "mod_operator" => OperatorKind::Mod,
        "less_operator" => OperatorKind::Lt,
        "less_equal_operator" => OperatorKind::Le,
        "equals_operator" => OperatorKind::Eq,
        "not_equals_operator" => OperatorKind::Ne,
        "greater_equal_operator" => OperatorKind::Ge,
        "greater_operator" => OperatorKind::Gt,
        "shift_left_operator" => OperatorKind::Shl,
        "shift_right_operator" => OperatorKind::Shr,
        "bit_xor_operator" => OperatorKind::BitXor,
        "bit_and_operator" => OperatorKind::BitAnd,
        "bit_or_operator" => OperatorKind::BitOr,
        _ => panic!("Unexpected operator node: {operator_kind}"),
    };

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
        "array_literal" => Literal::Array(build_array_literal(node, code)),
        "bool_literal" => Literal::Bool(build_bool_literal(node, code)),
        "string_literal" => Literal::String(build_string_literal(node, code)),
        "number_literal" => Literal::Number(build_number_literal(node, code)),
        "unit_literal" => Literal::Unit(build_unit_literal(node, code)),
        _ => panic!("Unexpected literal type: {}", node.kind()),
    }
}

fn build_array_literal(node: &Node, code: &[u8]) -> ArrayLiteral {
    let location = get_location(node);
    let mut elements = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        elements.push(build_expression(&child, code));
    }

    ArrayLiteral { location, elements }
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
    let value = node.utf8_text(code).unwrap().to_string();

    //determine number literal type based on value
    let type_ = if value.starts_with('-') {
        Type::Simple(SimpleType {
            location: Location::default(),
            name: "i32".to_string(),
        })
    } else {
        Type::Simple(SimpleType {
            location: Location::default(),
            name: "u32".to_string(),
        })
    };

    NumberLiteral {
        location,
        value,
        type_,
    }
}

fn build_unit_literal(node: &Node, _: &[u8]) -> UnitLiteral {
    let location = get_location(node);

    UnitLiteral { location }
}

fn build_type(node: &Node, code: &[u8]) -> Type {
    let node_kind = node.kind();
    match node_kind {
        "type_array" => Type::Array(Box::new(build_type_array(node, code))),
        "type_i8" | "type_i16" | "type_i32" | "type_i64" | "type_u8" | "type_u16" | "type_u32"
        | "type_u64" | "type_bool" | "type_unit" => Type::Simple(build_simple_type(node, code)),
        "generic_type" | "generic_name" => Type::Generic(build_generic_type(node, code)),
        "type_qualified_name" => Type::Qualified(build_type_qualified_name(node, code)),
        "qualified_name" => Type::QualifiedName(build_qualified_name(node, code)),
        "type_fn" => Type::Function(build_function_type(node, code)),
        "identifier" => Type::Identifier(build_identifier(node, code)),
        _ => {
            let location = get_location(node);
            panic!("Unexpected type: {node_kind}, {location}")
        }
    }
}

fn build_type_array(node: &Node, code: &[u8]) -> TypeArray {
    let location = get_location(node);
    let element_type = build_type(&node.child_by_field_name("type").unwrap(), code);
    let size = node
        .child_by_field_name("length")
        .map(|n| Box::new(build_expression(&n, code)));

    TypeArray {
        location,
        element_type: Box::new(element_type),
        size,
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

    let args = node.child(1).unwrap();

    let mut cursor = args.walk();

    let types = args
        .children_by_field_name("type", &mut cursor)
        .map(|segment| build_type(&segment, code));
    let parameters: Vec<Type> = types.collect();

    GenericType {
        location,
        base,
        parameters,
    }
}

fn build_function_type(node: &Node, code: &[u8]) -> FunctionType {
    let location = get_location(node);
    let mut arguments = None;
    let mut cursor = node.walk();

    let founded_arguments = node
        .children_by_field_name("argument", &mut cursor)
        .map(|segment| build_type(&segment, code));
    let founded_arguments: Vec<Type> = founded_arguments.collect();
    if !founded_arguments.is_empty() {
        arguments = Some(founded_arguments);
    }

    let returns = Box::new(build_type(
        &node.child_by_field_name("returns").unwrap(),
        code,
    ));

    FunctionType {
        location,
        parameters: arguments,
        returns,
    }
}

fn build_type_qualified_name(node: &Node, code: &[u8]) -> TypeQualifiedName {
    let location = get_location(node);
    let alias = build_identifier(&node.child_by_field_name("alias").unwrap(), code);
    let name = build_identifier(&node.child_by_field_name("name").unwrap(), code);

    TypeQualifiedName {
        location,
        alias,
        name,
    }
}

fn build_qualified_name(node: &Node, code: &[u8]) -> QualifiedName {
    let location = get_location(node);
    let qualifier = build_identifier(&node.child_by_field_name("qualifier").unwrap(), code);
    let name = build_identifier(&node.child_by_field_name("name").unwrap(), code);

    QualifiedName {
        location,
        qualifier,
        name,
    }
}

fn build_uzumaki_expression(node: &Node, _: &[u8]) -> UzumakiExpression {
    let location = get_location(node);

    UzumakiExpression { location }
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
