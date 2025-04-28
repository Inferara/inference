#![warn(clippy::pedantic)]

use core::panicking::panic;
use std::{collections::HashMap, marker::PhantomData, rc::Rc};

use crate::{
    arena::Arena,
    types::{
        ArrayIndexAccessExpression, ArrayLiteral, AssertStatement, AssignExpression, AstNode,
        BinaryExpression, Block, BoolLiteral, BreakStatement, ConstantDefinition, Definition,
        EnumDefinition, Expression, ExpressionStatement, ExternalFunctionDefinition,
        FunctionCallExpression, FunctionDefinition, FunctionType, GenericType, Identifier,
        IfStatement, Literal, Location, LoopStatement, MemberAccessExpression, NumberLiteral,
        OperatorKind, Parameter, ParenthesizedExpression, PrefixUnaryExpression, QualifiedName,
        ReturnStatement, SimpleType, SourceFile, SpecDefinition, Statement, StringLiteral,
        StructDefinition, StructField, Type, TypeArray, TypeDefinition, TypeDefinitionStatement,
        TypeQualifiedName, UnaryOperatorKind, UnitLiteral, UseDirective, UzumakiExpression,
        VariableDefinitionStatement,
    },
};
use tree_sitter::Node;

use super::types::BlockType;

#[allow(dead_code)]
trait BuilderInit {}
#[allow(dead_code)]
trait BuilderComplete {}

pub struct InitState;
impl BuilderInit for InitState {}
pub struct CompleteState;
impl BuilderComplete for CompleteState {}

pub enum Scope {
    Global,
    Inner(String),
}

#[allow(dead_code)]
pub struct Builder<'a, S> {
    arena: Arena,
    source_code: Vec<(Node<'a>, &'a [u8])>,
    types_table: HashMap<String, Scope>,
    t_ast: Vec<SourceFile>,
    _state: PhantomData<S>,
}

impl<'a> Builder<'a, InitState> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            arena: Arena::default(),
            source_code: Vec::new(),
            types_table: HashMap::new(),
            t_ast: Vec::new(),
            _state: PhantomData,
        }
    }

    /// Adds a source code and CST to the builder.
    ///
    /// # Panics
    ///
    /// This function will panioc if the `root` node is not of type `source_file`.
    pub fn add_source_code(&mut self, root: Node<'a>, code: &'a [u8]) {
        assert!(
            root.kind() == "source_file",
            "Expected a root node of type `source_file`"
        );
        self.source_code.push((root, code));
    }

    /// Builds the AST from the root node and source code.
    ///
    /// # Panics
    ///
    /// This function will panic if the `source_file` is malformed and a valid AST cannot be constructed.
    ///
    /// # Errors
    ///
    /// This function will return an error if the `source_file` is malformed and a valid AST cannot be constructed.
    pub fn build_ast(&mut self) -> anyhow::Result<Builder<CompleteState>> {
        let mut res = Vec::new();
        for (root, code) in &self.source_code.clone() {
            let location = Self::get_location(&root, code);
            let mut ast = SourceFile::new(location);

            for i in 0..root.child_count() {
                if let Some(child) = root.child(i) {
                    let child_kind = child.kind();

                    match child_kind {
                        "use_directive" => {
                            ast.add_use_directive(self.build_use_directive(&child, code))
                        }
                        _ => {
                            if let Ok(definition) = self.build_definition(&child, code) {
                                ast.add_definition(definition);
                            } else {
                                return Err(anyhow::anyhow!(
                                    "Unexpected child of type {child_kind}"
                                ));
                            }
                        }
                    }
                }
            }
            res.push(ast);
        }
        Ok(Builder {
            arena: self.arena.clone(),
            source_code: Vec::new(),
            types_table: HashMap::new(),
            t_ast: res,
            _state: PhantomData,
        })
    }

    fn build_use_directive(&mut self, node: &Node, code: &[u8]) -> UseDirective {
        let location = Self::get_location(node, code);
        let mut segments = None;
        let mut imported_types = None;
        let mut from = None;
        let mut cursor = node.walk();

        if let Some(from_literal) = node.child_by_field_name("from_literal") {
            from = Some(self.build_string_literal(&from_literal, code).value);
        } else {
            let founded_segments = node
                .children_by_field_name("segment", &mut cursor)
                .map(|segment| self.build_identifier(&segment, code));
            let founded_segments: Vec<Identifier> = founded_segments.collect();
            if !founded_segments.is_empty() {
                segments = Some(founded_segments);
            }
        }

        cursor = node.walk();
        let founded_imported_types = node
            .children_by_field_name("imported_type", &mut cursor)
            .map(|imported_type| self.build_identifier(&imported_type, code));
        let founded_imported_types: Vec<Identifier> = founded_imported_types.collect();
        if !founded_imported_types.is_empty() {
            imported_types = Some(founded_imported_types);
        }

        UseDirective::new(imported_types, segments, from, location)
    }

    fn build_spec_definition(&mut self, node: &Node, code: &[u8]) -> SpecDefinition {
        let location = Self::get_location(node, code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
        let mut definitions = Vec::new();

        for i in 0..node.child_count() {
            let child = node.child(i).unwrap();
            if let Ok(definition) = self.build_definition(&child, code) {
                definitions.push(definition);
            }
        }

        SpecDefinition::new(name, definitions, location)
    }

    fn build_enum_definition(&mut self, node: &Node, code: &[u8]) -> EnumDefinition {
        let location = Self::get_location(node, code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
        let mut variants = Vec::new();

        let mut cursor = node.walk();
        let founded_variants = node
            .children_by_field_name("variant", &mut cursor)
            .map(|segment| self.build_identifier(&segment, code));
        let founded_variants: Vec<Identifier> = founded_variants.collect();
        if !founded_variants.is_empty() {
            variants = founded_variants;
        }

        EnumDefinition::new(name, variants, location)
    }

    fn build_definition(&mut self, node: &Node, code: &[u8]) -> Definition {
        let kind = node.kind();
        match kind {
            "spec_definition" => Definition::Spec(self.build_spec_definition(node, code)),
            "struct_definition" => {
                let struct_definition = self.build_struct_definition(node, code)?;
                Definition::Struct(struct_definition)
            }
            "enum_definition" => Definition::Enum(self.build_enum_definition(node, code)),
            "constant_definition" => {
                Definition::Constant(self.build_constant_definition(node, code))
            }
            "function_definition" => {
                Definition::Function(self.build_function_definition(node, code)?)
            }
            "external_function_definition" => {
                Definition::ExternalFunction(self.build_external_function_definition(node, code))
            }
            "type_definition_statement" => Definition::Type(self.build_type_definition(node, code)),
            _ => panic!("Unexpected definition type: {}", kind),
        }
    }

    fn build_struct_definition(&mut self, node: &Node, code: &[u8]) -> Rc<StructDefinition> {
        let location = Self::get_location(node, code);
        let name = self.build_identifier(&node.child_by_field_name("struct_name").unwrap(), code);
        let mut fields = Vec::new();

        let mut cursor = node.walk();
        let founded_fields = node
            .children_by_field_name("field", &mut cursor)
            .map(|segment| self.build_struct_field(&segment, code));
        let founded_fields: Vec<Rc<StructField>> = founded_fields.collect();
        if !founded_fields.is_empty() {
            fields = founded_fields;
        }

        cursor = node.walk();
        let founded_methods = node
            .children_by_field_name("method", &mut cursor)
            .map(|segment| self.build_function_definition(&segment, code));
        let methods: Vec<Rc<FunctionDefinition>> = founded_methods.collect();

        let node = Rc::new(StructDefinition::new(name, fields, methods, location));
        self.arena.add_node(AstNode::StructDefinition(node.clone()));
        node
    }

    fn build_struct_field(&mut self, node: &Node, code: &[u8]) -> Rc<StructField> {
        let location = Self::get_location(node, code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
        let type_ = self.build_type(&node.child_by_field_name("type").unwrap(), code);

        let node = Rc::new(StructField::new(name, type_, location));
        self.arena.add_node(AstNode::StructField(node.clone()));
        node
    }

    fn build_constant_definition(&mut self, node: &Node, code: &[u8]) -> Rc<ConstantDefinition> {
        let location = Self::get_location(node, code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
        let type_ = self.build_type(&node.child_by_field_name("type").unwrap(), code);
        let value = self.build_literal(&node.child_by_field_name("value").unwrap(), code);

        let node = Rc::new(ConstantDefinition::new(name, type_, value, location));
        self.arena
            .add_node(AstNode::ConstantDefinition(node.clone()));
        node
    }

    fn build_function_definition(&mut self, node: &Node, code: &[u8]) -> Rc<FunctionDefinition> {
        let location = Self::get_location(node, code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
        let mut arguments = None;
        let mut returns = None;

        if let Some(argument_list_node) = node.child_by_field_name("argument_list") {
            let mut cursor = argument_list_node.walk();
            let founded_arguments = argument_list_node
                .children_by_field_name("argument", &mut cursor)
                .map(|segment| self.build_argument(&segment, code));
            let founded_arguments: Vec<Rc<Parameter>> = founded_arguments.collect();
            if !founded_arguments.is_empty() {
                arguments = Some(founded_arguments);
            }
        }

        if let Some(returns_node) = node.child_by_field_name("returns") {
            returns = Some(self.build_type(&returns_node, code));
        }
        let body_node = node.child_by_field_name("body").unwrap();
        let body = self.build_block(&body_node, code);
        let node = Rc::new(FunctionDefinition::new(
            name, arguments, returns, body, location,
        ));
        node
    }

    fn build_external_function_definition(
        &mut self,
        node: &Node,
        code: &[u8],
    ) -> Rc<ExternalFunctionDefinition> {
        let location = Self::get_location(node, code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
        let mut arguments = None;
        let mut returns = None;

        let mut cursor = node.walk();

        let founded_arguments = node
            .children_by_field_name("argument", &mut cursor)
            .map(|segment| self.build_identifier(&segment, code));
        let founded_arguments: Vec<Rc<Identifier>> = founded_arguments.collect();
        if !founded_arguments.is_empty() {
            arguments = Some(founded_arguments);
        }

        if let Some(returns_node) = node.child_by_field_name("returns") {
            returns = Some(self.build_type(&returns_node, code));
        }

        let node = Rc::new(ExternalFunctionDefinition::new(
            name, arguments, returns, location,
        ));
        self.arena
            .add_node(AstNode::ExternalFunctionDefinition(node.clone()));
        node
    }

    fn build_type_definition(&mut self, node: &Node, code: &[u8]) -> Rc<TypeDefinition> {
        let location = Self::get_location(node, code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
        let type_ = self.build_type(&node.child_by_field_name("type").unwrap(), code);

        let node = Rc::new(TypeDefinition::new(name, type_, location));
        self.arena.add_node(AstNode::TypeDefinition(node.clone()));
        node
    }

    fn build_argument(&mut self, node: &Node, code: &[u8]) -> Rc<Parameter> {
        let location = Self::get_location(node, code);
        let name_node = node.child_by_field_name("name").unwrap();
        let name = self.build_identifier(&name_node, code);
        let type_node = node.child_by_field_name("type").unwrap();
        let type_ = self.build_type(&type_node, code);
        let node = Rc::new(Parameter::new(location, name, type_));
        self.arena.add_node(AstNode::Parameter(node.clone()));
        node
    }

    fn build_block(&mut self, node: &Node, code: &[u8]) -> BlockType {
        let location = Self::get_location(node, code);
        match node.kind() {
            "assume_block" => BlockType::Assume(Rc::new(Block::new(
                location,
                self.build_block_statements(&node.child_by_field_name("body").unwrap(), code),
            ))),
            "forall_block" => BlockType::Forall(Rc::new(Block::new(
                location,
                self.build_block_statements(&node.child_by_field_name("body").unwrap(), code),
            ))),
            "exists_block" => BlockType::Exists(Rc::new(Block::new(
                location,
                self.build_block_statements(&node.child_by_field_name("body").unwrap(), code),
            ))),
            "unique_block" => BlockType::Unique(Rc::new(Block::new(
                location,
                self.build_block_statements(&node.child_by_field_name("body").unwrap(), code),
            ))),
            _ => BlockType::Block(Rc::new(Block::new(
                location,
                self.build_block_statements(node, code),
            ))),
        }
    }

    fn build_block_statements(&mut self, node: &Node, code: &[u8]) -> Vec<Statement> {
        let mut statements = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            statements.push(self.build_statement(&child, code));
        }
        statements
    }

    fn build_statement(&mut self, node: &Node, code: &[u8]) -> Statement {
        match node.kind() {
            "block" | "forall_block" | "assume_block" | "exists_block" | "unique_block" => {
                Statement::Block(self.build_block(node, code))
            }
            "expression_statement" => {
                Statement::Expression(self.build_expression_statement(node, code))
            }
            "return_statement" => Statement::Return(self.build_return_statement(node, code)),
            "loop_statement" => Statement::Loop(self.build_loop_statement(node, code)),
            "if_statement" => Statement::If(self.build_if_statement(node, code)),
            "variable_definition_statement" => {
                Statement::VariableDefinition(self.build_variable_definition_statement(node, code))
            }
            "type_definition_statement" => {
                Statement::TypeDefinition(self.build_type_definition_statement(node, code))
            }
            "assert_statement" => Statement::Assert(self.build_assert_statement(node, code)),
            "break_statement" => Statement::Break(self.build_break_statement(node, code)),
            "constant_definition" => {
                Statement::ConstantDefinition(self.build_constant_definition(node, code))
            }
            _ => panic!(
                "Unexpected statement type: {}, {}",
                node.kind(),
                Self::get_location(node, code)
            ),
        }
    }

    fn build_expression_statement(&mut self, node: &Node, code: &[u8]) -> ExpressionStatement {
        let location = Self::get_location(node, code);
        let expression = self.build_expression(&node.child(0).unwrap(), code);

        ExpressionStatement::new(location, expression)
    }

    fn build_return_statement(&mut self, node: &Node, code: &[u8]) -> Rc<ReturnStatement> {
        let location = Self::get_location(node, code);
        let expression =
            self.build_expression(&node.child_by_field_name("expression").unwrap(), code);

        let node = Rc::new(ReturnStatement::new(location, expression));
        self.arena.add_node(AstNode::ReturnStatement(node.clone()));
        node
    }

    fn build_loop_statement(&mut self, node: &Node, code: &[u8]) -> Rc<LoopStatement> {
        let location = Self::get_location(node, code);
        let condition = node
            .child_by_field_name("condition")
            .map(|n| self.build_expression(&n, code));
        let body_block = node.child_by_field_name("body").unwrap();
        let body = self.build_block(&body_block, code);
        let node = Rc::new(LoopStatement::new(location, condition, body));
        self.arena.add_node(AstNode::LoopStatement(node.clone()));
        node
    }

    fn build_if_statement(&mut self, node: &Node, code: &[u8]) -> Rc<IfStatement> {
        let location = Self::get_location(node, code);
        let condition_node = node.child_by_field_name("condition").unwrap();
        let condition = self.build_expression(&condition_node, code);
        let if_arm_node = node.child_by_field_name("if_arm").unwrap();
        let if_arm = self.build_block(&if_arm_node, code);
        let else_arm = node
            .child_by_field_name("else_arm")
            .map(|n| self.build_block(&n, code));
        let node = Rc::new(IfStatement::new(location, condition, if_arm, else_arm));
        self.arena.add_node(AstNode::IfStatement(node.clone()));
        node
    }

    fn build_variable_definition_statement(
        &mut self,
        node: &Node,
        code: &[u8],
    ) -> Rc<VariableDefinitionStatement> {
        let location = Self::get_location(node, code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
        let type_ = self.build_type(&node.child_by_field_name("type").unwrap(), code);
        let value = node
            .child_by_field_name("value")
            .map(|n| self.build_expression(&n, code));
        let is_undef = node.child_by_field_name("undef").is_some();

        let node = Rc::new(VariableDefinitionStatement::new(
            location, name, type_, value, is_undef,
        ));
        self.arena
            .add_node(AstNode::VariableDefinitionStatement(node.clone()));
        node
    }

    fn build_type_definition_statement(
        &mut self,
        node: &Node,
        code: &[u8],
    ) -> Rc<TypeDefinitionStatement> {
        let location = Self::get_location(node, code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
        let type_ = self.build_type(&node.child_by_field_name("type").unwrap(), code);

        let node = Rc::new(TypeDefinitionStatement::new(location, name, type_));
        self.arena
            .add_node(AstNode::TypeDefinitionStatement(node.clone()));
        node
    }

    fn build_expression(&mut self, node: &Node, code: &[u8]) -> Expression {
        let node_kind = node.kind();
        match node_kind {
            "assign_expression" => Expression::Assign(self.build_assign_expression(node, code)),
            "array_index_access_expression" => {
                Expression::ArrayIndexAccess(self.build_array_index_access_expression(node, code))
            }
            "member_access_expression" => {
                Expression::MemberAccess(self.build_member_access_expression(node, code))
            }
            "function_call_expression" => {
                Expression::FunctionCall(self.build_function_call_expression(node, code))
            }
            "prefix_unary_expression" => {
                Expression::PrefixUnary(self.build_prefix_unary_expression(node, code))
            }
            "parenthesized_expression" => {
                Expression::Parenthesized(self.build_parenthesized_expression(node, code))
            }
            "binary_expression" => Expression::Binary(self.build_binary_expression(node, code)),
            "bool_literal" | "string_literal" | "number_literal" | "array_literal"
            | "unit_literal" => Expression::Literal(self.build_literal(node, code)),
            "uzumaki_keyword" => Expression::Uzumaki(self.build_uzumaki_expression(node, code)),
            "identifier" => Expression::Identifier(self.build_identifier(node, code)),
            _ => Expression::Type(self.build_type(node, code)),
        }
    }

    fn build_assign_expression(&mut self, node: &Node, code: &[u8]) -> Rc<AssignExpression> {
        let location = Self::get_location(node, code);
        let left = self.build_expression(&node.child_by_field_name("left").unwrap(), code);
        let right = self.build_expression(&node.child_by_field_name("right").unwrap(), code);

        let node = Rc::new(AssignExpression::new(location, left, right));
        self.arena.add_node(AstNode::AssignExpression(node.clone()));
        node
    }

    fn build_array_index_access_expression(
        &mut self,
        node: &Node,
        code: &[u8],
    ) -> Rc<ArrayIndexAccessExpression> {
        let location = Self::get_location(node, code);
        let array = self.build_expression(&node.named_child(0).unwrap(), code);
        let index = self.build_expression(&node.named_child(1).unwrap(), code);

        let node = Rc::new(ArrayIndexAccessExpression::new(location, array, index));
        self.arena
            .add_node(AstNode::ArrayIndexAccessExpression(node.clone()));
        node
    }

    fn build_member_access_expression(
        &mut self,
        node: &Node,
        code: &[u8],
    ) -> Rc<MemberAccessExpression> {
        let location = Self::get_location(node, code);
        let expression =
            self.build_expression(&node.child_by_field_name("expression").unwrap(), code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);

        let node = Rc::new(MemberAccessExpression::new(location, expression, name));
        self.arena
            .add_node(AstNode::MemberAccessExpression(node.clone()));
        node
    }

    fn build_function_call_expression(
        &mut self,
        node: &Node,
        code: &[u8],
    ) -> Rc<FunctionCallExpression> {
        let location = Self::get_location(node, code);
        let function = self.build_expression(&node.child_by_field_name("function").unwrap(), code);
        let mut argument_name_expression_map: Vec<(Rc<Identifier>, Expression)> = Vec::new();
        let mut pending_name: Option<Rc<Identifier>> = None;
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if let Some(field) = cursor.field_name() {
                    match field {
                        "argument_name" => {
                            if let Expression::Identifier(id) = self.build_expression(&child, code)
                            {
                                pending_name = Some(id);
                            }
                        }
                        "argument" => {
                            let expr = self.build_expression(&child, code);
                            let name = pending_name.take().unwrap_or_default();
                            argument_name_expression_map.push((name, expr));
                        }
                        _ => {}
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        let arguments = if argument_name_expression_map.is_empty() {
            None
        } else {
            Some(argument_name_expression_map)
        };

        let node = Rc::new(FunctionCallExpression::new(location, function, arguments));
        self.arena
            .add_node(AstNode::FunctionCallExpression(node.clone()));
        node
    }

    fn build_prefix_unary_expression(
        &mut self,
        node: &Node,
        code: &[u8],
    ) -> Rc<PrefixUnaryExpression> {
        let location = Self::get_location(node, code);
        let expression = self.build_expression(&node.child(1).unwrap(), code);

        let operator_node = node.child_by_field_name("operator").unwrap();
        let operator = match operator_node.kind() {
            "unary_not" => UnaryOperatorKind::Neg,
            _ => panic!("Unexpected operator node"),
        };

        let node = Rc::new(PrefixUnaryExpression::new(location, expression, operator));
        self.arena
            .add_node(AstNode::PrefixUnaryExpression(node.clone()));
        node
    }

    fn build_assert_statement(&mut self, node: &Node, code: &[u8]) -> Rc<AssertStatement> {
        let location = Self::get_location(node, code);
        let expression = self.build_expression(&node.child(1).unwrap(), code);
        let node = Rc::new(AssertStatement::new(location, expression));
        self.arena.add_node(AstNode::AssertStatement(node.clone()));
        node
    }

    fn build_break_statement(&mut self, node: &Node, code: &[u8]) -> Rc<BreakStatement> {
        let location = Self::get_location(node, code);
        let node = Rc::new(BreakStatement::new(location));
        self.arena.add_node(AstNode::BreakStatement(node.clone()));
        node
    }

    fn build_parenthesized_expression(
        &mut self,
        node: &Node,
        code: &[u8],
    ) -> Rc<ParenthesizedExpression> {
        let location = Self::get_location(node, code);
        let expression = self.build_expression(&node.child(1).unwrap(), code);

        let node = Rc::new(ParenthesizedExpression::new(location, expression));
        self.arena
            .add_node(AstNode::ParenthesizedExpression(node.clone()));
        node
    }

    fn build_binary_expression(&mut self, node: &Node, code: &[u8]) -> Rc<BinaryExpression> {
        let location = Self::get_location(node, code);
        let left = self.build_expression(&node.child_by_field_name("left").unwrap(), code);
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

        let right = self.build_expression(&node.child_by_field_name("right").unwrap(), code);

        let node = Rc::new(BinaryExpression::new(location, left, operator, right));
        self.arena.add_node(AstNode::BinaryExpression(node.clone()));
        node
    }

    fn build_literal(&mut self, node: &Node, code: &[u8]) -> Literal {
        match node.kind() {
            "array_literal" => Literal::Array(self.build_array_literal(node, code)),
            "bool_literal" => Literal::Bool(self.build_bool_literal(node, code)),
            "string_literal" => Literal::String(self.build_string_literal(node, code)),
            "number_literal" => Literal::Number(self.build_number_literal(node, code)),
            "unit_literal" => Literal::Unit(self.build_unit_literal(node, code)),
            _ => panic!("Unexpected literal type: {}", node.kind()),
        }
    }

    fn build_array_literal(&mut self, node: &Node, code: &[u8]) -> Rc<ArrayLiteral> {
        let location = Self::get_location(node, code);
        let mut elements = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            elements.push(self.build_expression(&child, code));
        }

        let node = Rc::new(ArrayLiteral::new(location, elements));
        self.arena.add_node(AstNode::ArrayLiteral(node.clone()));
        node
    }

    fn build_bool_literal(&mut self, node: &Node, code: &[u8]) -> Rc<BoolLiteral> {
        let location = Self::get_location(node, code);
        let value = match node.utf8_text(code).unwrap() {
            "true" => true,
            "false" => false,
            _ => panic!("Unexpected boolean literal value"),
        };

        let node = Rc::new(BoolLiteral::new(location, value));
        self.arena.add_node(AstNode::BoolLiteral(node.clone()));
        node
    }

    fn build_string_literal(&mut self, node: &Node, code: &[u8]) -> Rc<StringLiteral> {
        let location = Self::get_location(node, code);
        let value = node.utf8_text(code).unwrap().to_string();

        let node = Rc::new(StringLiteral::new(location, value));
        self.arena.add_node(AstNode::StringLiteral(node.clone()));
        node
    }

    fn build_number_literal(&mut self, node: &Node, code: &[u8]) -> Rc<NumberLiteral> {
        let location = Self::get_location(node, code);
        let value = node.utf8_text(code).unwrap().to_string();

        //FIXME hack
        let node = Rc::new(NumberLiteral::new(
            location,
            value,
            Type::Simple(Rc::new(SimpleType::new(
                Location::default(),
                "i32".to_string(),
            ))),
        ));
        self.arena.add_node(AstNode::NumberLiteral(node.clone()));
        node
    }

    fn build_unit_literal(&mut self, node: &Node, code: &[u8]) -> Rc<UnitLiteral> {
        let location = Self::get_location(node, code);
        let node = Rc::new(UnitLiteral::new(location));
        self.arena.add_node(AstNode::UnitLiteral(node.clone()));
        node
    }

    fn build_type(&mut self, node: &Node, code: &[u8]) -> Type {
        let node_kind = node.kind();
        match node_kind {
            "type_array" => Type::Array(self.build_type_array(node, code)),
            "type_i8" | "type_i16" | "type_i32" | "type_i64" | "type_u8" | "type_u16"
            | "type_u32" | "type_u64" | "type_bool" | "type_unit" => {
                Type::Simple(self.build_simple_type(node, code))
            }
            "generic_type" | "generic_name" => Type::Generic(self.build_generic_type(node, code)),
            "type_qualified_name" => Type::Qualified(self.build_type_qualified_name(node, code)),
            "qualified_name" => Type::QualifiedName(self.build_qualified_name(node, code)),
            "type_fn" => Type::Function(self.build_function_type(node, code)),
            "identifier" => Type::Identifier(self.build_identifier(node, code)),
            _ => {
                let location = Self::get_location(node, code);
                panic!("Unexpected type: {node_kind}, {location}")
            }
        }
    }

    fn build_type_array(&mut self, node: &Node, code: &[u8]) -> Rc<TypeArray> {
        let location = Self::get_location(node, code);
        let element_type = self.build_type(&node.child_by_field_name("type").unwrap(), code);
        let size = node
            .child_by_field_name("length")
            .map(|n| Box::new(self.build_expression(&n, code)));

        let node = Rc::new(TypeArray::new(location, Box::new(element_type), size));
        self.arena.add_node(AstNode::TypeArray(node.clone()));
        node
    }

    fn build_simple_type(&mut self, node: &Node, code: &[u8]) -> Rc<SimpleType> {
        let location = Self::get_location(node, code);
        let name = node.utf8_text(code).unwrap().to_string();
        self.types_table.insert(name.clone(), Scope::Global);
        let node = Rc::new(SimpleType::new(location, name));
        self.arena.add_node(AstNode::SimpleType(node.clone()));
        node
    }

    fn build_generic_type(&mut self, node: &Node, code: &[u8]) -> Rc<GenericType> {
        let location = Self::get_location(node, code);
        let base = self.build_identifier(&node.child_by_field_name("base_type").unwrap(), code);

        let args = node.child(1).unwrap();

        let mut cursor = args.walk();

        let types = args
            .children_by_field_name("type", &mut cursor)
            .map(|segment| self.build_type(&segment, code));
        let parameters: Vec<Type> = types.collect();

        let node = Rc::new(GenericType::new(location, base, parameters));
        self.arena.add_node(AstNode::GenericType(node.clone()));
        node
    }

    fn build_function_type(&mut self, node: &Node, code: &[u8]) -> Rc<FunctionType> {
        let location = Self::get_location(node, code);
        let mut arguments = None;
        let mut cursor = node.walk();

        let founded_arguments = node
            .children_by_field_name("argument", &mut cursor)
            .map(|segment| self.build_type(&segment, code));
        let founded_arguments: Vec<Type> = founded_arguments.collect();
        if !founded_arguments.is_empty() {
            arguments = Some(founded_arguments);
        }

        let returns =
            Box::new(self.build_type(&node.child_by_field_name("returns").unwrap(), code));

        let node = Rc::new(FunctionType::new(location, arguments, returns));
        self.arena.add_node(AstNode::FunctionType(node.clone()));
        node
    }

    fn build_type_qualified_name(&mut self, node: &Node, code: &[u8]) -> Rc<TypeQualifiedName> {
        let location = Self::get_location(node, code);
        let alias = self.build_identifier(&node.child_by_field_name("alias").unwrap(), code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);

        let node = Rc::new(TypeQualifiedName::new(location, alias, name));
        self.arena
            .add_node(AstNode::TypeQualifiedName(node.clone()));
        node
    }

    fn build_qualified_name(&mut self, node: &Node, code: &[u8]) -> Rc<QualifiedName> {
        let location = Self::get_location(node, code);
        let qualifier =
            self.build_identifier(&node.child_by_field_name("qualifier").unwrap(), code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);

        let node = Rc::new(QualifiedName::new(location, qualifier, name));
        self.arena.add_node(AstNode::QualifiedName(node.clone()));
        node
    }

    fn build_uzumaki_expression(&mut self, node: &Node, code: &[u8]) -> Rc<UzumakiExpression> {
        let location = Self::get_location(node, code);
        let node = Rc::new(UzumakiExpression::new(location));
        self.arena
            .add_node(AstNode::UzumakiExpression(node.clone()));
        node
    }

    fn build_identifier(&mut self, node: &Node, code: &[u8]) -> Rc<Identifier> {
        let location = Self::get_location(node, code);
        let name = node.utf8_text(code).unwrap().to_string();
        //TODO deduce type
        let node = Rc::new(Identifier::new(name, location));
        self.arena.add_node(AstNode::Identifier(node.clone()));
        node
    }

    #[allow(clippy::cast_possible_truncation)]
    fn get_location(node: &Node, code: &[u8]) -> Location {
        let offset_start = node.start_byte() as u32;
        let offset_end = node.end_byte() as u32;
        let start_position = node.start_position();
        let end_position = node.end_position();
        let start_line = start_position.row as u32 + 1;
        let start_column = start_position.column as u32 + 1;
        let end_line = end_position.row as u32 + 1;
        let end_column = end_position.column as u32 + 1;
        let source = node.utf8_text(code).unwrap().to_string();

        Location {
            offset_start,
            offset_end,
            start_line,
            start_column,
            end_line,
            end_column,
            source,
        }
    }
}

impl Builder<'_, CompleteState> {
    /// Returns typed AST
    #[must_use]
    pub fn t_ast(self) -> Vec<SourceFile> {
        self.t_ast
    }
}
