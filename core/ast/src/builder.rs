use std::{marker::PhantomData, rc::Rc};

use crate::nodes::{
    ArgumentType, Directive, IgnoreArgument, Misc, SelfReference, StructExpression,
    TypeMemberAccessExpression,
};
use crate::{
    arena::Arena,
    nodes::{
        Argument, ArrayIndexAccessExpression, ArrayLiteral, AssertStatement, AssignStatement,
        AstNode, BinaryExpression, Block, BlockType, BoolLiteral, BreakStatement,
        ConstantDefinition, Definition, EnumDefinition, Expression, ExternalFunctionDefinition,
        FunctionCallExpression, FunctionDefinition, FunctionType, GenericType, Identifier,
        IfStatement, Literal, Location, LoopStatement, MemberAccessExpression, NumberLiteral,
        OperatorKind, ParenthesizedExpression, PrefixUnaryExpression, QualifiedName,
        ReturnStatement, SimpleType, SourceFile, SpecDefinition, Statement, StringLiteral,
        StructDefinition, StructField, Type, TypeArray, TypeDefinition, TypeDefinitionStatement,
        TypeQualifiedName, UnaryOperatorKind, UnitLiteral, UseDirective, UzumakiExpression,
        VariableDefinitionStatement,
    },
};
use tree_sitter::Node;

#[allow(dead_code)]
trait BuilderInit {}
#[allow(dead_code)]
trait BuilderComplete {}

pub struct InitState;
impl BuilderInit for InitState {}
pub struct CompleteState;
impl BuilderComplete for CompleteState {}

pub type CompletedBuilder<'a> = Builder<'a, CompleteState>;

#[allow(dead_code)]
pub struct Builder<'a, S> {
    arena: Arena,
    source_code: Vec<(Node<'a>, &'a [u8])>,
    _state: PhantomData<S>,
}

impl Default for Builder<'_, InitState> {
    fn default() -> Self {
        Builder::new()
    }
}

impl<'a> Builder<'a, InitState> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            arena: Arena::default(),
            source_code: Vec::new(),
            _state: PhantomData,
        }
    }

    /// Adds a source code and CST to the builder.
    ///
    /// # Panics
    ///
    /// This function will panic if the `root` node is not of type `source_file`.
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
    #[allow(clippy::single_match_else)]
    pub fn build_ast(&'_ mut self) -> anyhow::Result<Builder<'_, CompleteState>> {
        let mut res = Vec::new();
        for (root, code) in &self.source_code.clone() {
            let id = Self::get_node_id();
            let location = Self::get_location(root, code);
            let mut ast = SourceFile::new(id, location);

            for i in 0..root.child_count() {
                if let Some(child) = root.child(u32::try_from(i).unwrap()) {
                    let child_kind = child.kind();

                    match child_kind {
                        "use_directive" => {
                            ast.directives
                                .push(Directive::Use(self.build_use_directive(id, &child, code)));
                        }
                        _ => {
                            let definition = self.build_definition(id, &child, code);
                            ast.definitions.push(definition);
                        }
                    }
                }
            }
            res.push(ast);
        }
        self.arena.sources = res;
        Ok(Builder {
            arena: std::mem::take(&mut self.arena),
            source_code: Vec::new(),
            _state: PhantomData,
        })
    }

    fn build_use_directive(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<UseDirective> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let mut segments = None;
        let mut imported_types = None;
        let mut from = None;
        let mut cursor = node.walk();

        if let Some(from_literal) = node.child_by_field_name("from_literal") {
            from = Some(
                self.build_string_literal(id, &from_literal, code)
                    .value
                    .clone(),
            );
        } else {
            let founded_segments = node
                .children_by_field_name("segment", &mut cursor)
                .map(|segment| self.build_identifier(id, &segment, code));
            let founded_segments: Vec<Rc<Identifier>> = founded_segments.collect();
            if !founded_segments.is_empty() {
                segments = Some(founded_segments);
            }
        }

        cursor = node.walk();
        let founded_imported_types = node
            .children_by_field_name("imported_type", &mut cursor)
            .map(|imported_type| self.build_identifier(id, &imported_type, code));
        let founded_imported_types: Vec<Rc<Identifier>> = founded_imported_types.collect();
        if !founded_imported_types.is_empty() {
            imported_types = Some(founded_imported_types);
        }

        let node = Rc::new(UseDirective::new(
            id,
            imported_types,
            segments,
            from,
            location,
        ));
        self.arena
            .add_node(AstNode::Directive(Directive::Use(node.clone())), parent_id);
        node
    }

    fn build_spec_definition(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<SpecDefinition> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let name = self.build_identifier(id, &node.child_by_field_name("name").unwrap(), code);
        let mut definitions = Vec::new();

        // first child is name
        for i in 1..node.named_child_count() {
            let child = node.named_child(u32::try_from(i).unwrap()).unwrap();
            let definition = self.build_definition(id, &child, code);
            definitions.push(definition);
        }

        let node = Rc::new(SpecDefinition::new(id, name, definitions, location));
        self.arena.add_node(
            AstNode::Definition(Definition::Spec(node.clone())),
            parent_id,
        );
        node
    }

    fn build_enum_definition(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<EnumDefinition> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let name = self.build_identifier(id, &node.child_by_field_name("name").unwrap(), code);
        let mut variants = Vec::new();

        let mut cursor = node.walk();
        let founded_variants = node
            .children_by_field_name("variant", &mut cursor)
            .map(|segment| self.build_identifier(id, &segment, code));
        let founded_variants: Vec<Rc<Identifier>> = founded_variants.collect();
        if !founded_variants.is_empty() {
            variants = founded_variants;
        }

        let node = Rc::new(EnumDefinition::new(id, name, variants, location));
        self.arena.add_node(
            AstNode::Definition(Definition::Enum(node.clone())),
            parent_id,
        );
        node
    }

    fn build_definition(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> Definition {
        let kind = node.kind();
        match kind {
            "spec_definition" => {
                Definition::Spec(self.build_spec_definition(parent_id, node, code))
            }
            "struct_definition" => {
                let struct_definition = self.build_struct_definition(parent_id, node, code);
                Definition::Struct(struct_definition)
            }
            "enum_definition" => {
                Definition::Enum(self.build_enum_definition(parent_id, node, code))
            }
            "constant_definition" => {
                Definition::Constant(self.build_constant_definition(parent_id, node, code))
            }
            "function_definition" => {
                Definition::Function(self.build_function_definition(parent_id, node, code))
            }
            "external_function_definition" => Definition::ExternalFunction(
                self.build_external_function_definition(parent_id, node, code),
            ),
            "type_definition_statement" => {
                Definition::Type(self.build_type_definition(parent_id, node, code))
            }
            _ => panic!(
                "Unexpected definition kind: {}, {}",
                node.kind(),
                Self::get_location(node, code)
            ),
        }
    }

    fn build_struct_definition(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<StructDefinition> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let name = self.build_identifier(id, &node.child_by_field_name("name").unwrap(), code);
        let mut fields = Vec::new();
        let mut cursor = node.walk();
        let founded_fields = node
            .children_by_field_name("field", &mut cursor)
            .map(|segment| self.build_struct_field(id, &segment, code));
        let founded_fields: Vec<Rc<StructField>> = founded_fields.collect();
        if !founded_fields.is_empty() {
            fields = founded_fields;
        }
        cursor = node.walk();
        let founded_methods = node
            .children_by_field_name("method", &mut cursor)
            .map(|segment| self.build_function_definition(id, &segment, code));
        let methods: Vec<Rc<FunctionDefinition>> = founded_methods.collect();

        let node = Rc::new(StructDefinition::new(id, name, fields, methods, location));
        self.arena.add_node(
            AstNode::Definition(Definition::Struct(node.clone())),
            parent_id,
        );
        node
    }

    fn build_struct_field(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> Rc<StructField> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let ty = self.build_type(id, &node.child_by_field_name("type").unwrap(), code);
        let name = self.build_identifier(id, &node.child_by_field_name("name").unwrap(), code);

        let node = Rc::new(StructField::new(id, name, ty, location));
        self.arena
            .add_node(AstNode::Misc(Misc::StructField(node.clone())), parent_id);
        node
    }

    fn build_constant_definition(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<ConstantDefinition> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let ty = self.build_type(id, &node.child_by_field_name("type").unwrap(), code);
        let name = self.build_identifier(id, &node.child_by_field_name("name").unwrap(), code);
        let value = self.build_literal(id, &node.child_by_field_name("value").unwrap(), code);

        let node = Rc::new(ConstantDefinition::new(id, name, ty, value, location));
        self.arena.add_node(
            AstNode::Definition(Definition::Constant(node.clone())),
            parent_id,
        );
        node
    }

    fn build_function_definition(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<FunctionDefinition> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let mut arguments = None;
        let mut returns = None;
        let mut type_parameters = None;

        if let Some(argument_list_node) = node.child_by_field_name("argument_list") {
            let mut cursor = argument_list_node.walk();
            let founded_arguments = argument_list_node
                .children_by_field_name("argument", &mut cursor)
                .map(|segment| self.build_argument_type(id, &segment, code));
            let founded_arguments: Vec<ArgumentType> = founded_arguments.collect();
            if !founded_arguments.is_empty() {
                arguments = Some(founded_arguments);
            }
        }

        if let Some(argument_list_node) = node.child_by_field_name("type_parameters") {
            let mut cursor = argument_list_node.walk();
            let founded_type_parameters = argument_list_node
                .children_by_field_name("type", &mut cursor)
                .map(|segment| self.build_identifier(id, &segment, code));
            let founded_type_parameters: Vec<Rc<Identifier>> = founded_type_parameters.collect();
            if !founded_type_parameters.is_empty() {
                type_parameters = Some(founded_type_parameters);
            }
        }

        if let Some(returns_node) = node.child_by_field_name("returns") {
            returns = Some(self.build_type(id, &returns_node, code));
        }
        let name = self.build_identifier(id, &node.child_by_field_name("name").unwrap(), code);
        let body_node = node.child_by_field_name("body").unwrap();
        let body = self.build_block(id, &body_node, code);
        let node = Rc::new(FunctionDefinition::new(
            id,
            name,
            type_parameters,
            arguments,
            returns,
            body,
            location,
        ));
        self.arena.add_node(
            AstNode::Definition(Definition::Function(node.clone())),
            parent_id,
        );
        node
    }

    fn build_external_function_definition(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<ExternalFunctionDefinition> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let name = self.build_identifier(id, &node.child_by_field_name("name").unwrap(), code);
        let mut arguments = None;
        let mut returns = None;

        let mut cursor = node.walk();

        let founded_arguments = node
            .children_by_field_name("argument", &mut cursor)
            .map(|segment| self.build_argument_type(id, &segment, code));
        let founded_arguments: Vec<ArgumentType> = founded_arguments.collect();
        if !founded_arguments.is_empty() {
            arguments = Some(founded_arguments);
        }

        if let Some(returns_node) = node.child_by_field_name("returns") {
            returns = Some(self.build_type(id, &returns_node, code));
        }

        let node = Rc::new(ExternalFunctionDefinition::new(
            id, name, arguments, returns, location,
        ));
        self.arena.add_node(
            AstNode::Definition(Definition::ExternalFunction(node.clone())),
            parent_id,
        );
        node
    }

    fn build_type_definition(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<TypeDefinition> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let ty = self.build_type(id, &node.child_by_field_name("type").unwrap(), code);
        let name = self.build_identifier(id, &node.child_by_field_name("name").unwrap(), code);
        let node = Rc::new(TypeDefinition::new(id, name, ty, location));
        self.arena.add_node(
            AstNode::Definition(Definition::Type(node.clone())),
            parent_id,
        );
        node
    }

    fn build_argument_type(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> ArgumentType {
        match node.kind() {
            "argument_declaration" => {
                let argument = self.build_argument(parent_id, node, code);
                ArgumentType::Argument(argument)
            }
            "self_reference" => {
                let self_reference = self.build_self_reference(parent_id, node, code);
                ArgumentType::SelfReference(self_reference)
            }
            "ignore_argument" => {
                let ignore_argument = self.build_ignore_argument(parent_id, node, code);
                ArgumentType::IgnoreArgument(ignore_argument)
            }
            _ => ArgumentType::Type(self.build_type(parent_id, node, code)),
        }
    }

    fn build_argument(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> Rc<Argument> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let name_node = node.child_by_field_name("name").unwrap();
        let type_node = node.child_by_field_name("type").unwrap();
        let ty = self.build_type(id, &type_node, code);
        let is_mut = node
            .child_by_field_name("mut")
            .is_some_and(|n| n.kind() == "true");
        let name = self.build_identifier(id, &name_node, code);
        let node = Rc::new(Argument::new(id, location, name, is_mut, ty));
        self.arena.add_node(
            AstNode::ArgumentType(ArgumentType::Argument(node.clone())),
            parent_id,
        );
        node
    }

    fn build_self_reference(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<SelfReference> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let is_mut = node
            .child_by_field_name("mut")
            .is_some_and(|n| n.kind() == "true");
        let node = Rc::new(SelfReference::new(id, location, is_mut));
        self.arena.add_node(
            AstNode::ArgumentType(ArgumentType::SelfReference(node.clone())),
            parent_id,
        );
        node
    }

    fn build_ignore_argument(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<IgnoreArgument> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let ty = self.build_type(id, &node.child_by_field_name("type").unwrap(), code);
        let node = Rc::new(IgnoreArgument::new(id, location, ty));
        self.arena.add_node(
            AstNode::ArgumentType(ArgumentType::IgnoreArgument(node.clone())),
            parent_id,
        );
        node
    }

    fn build_block(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> BlockType {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        match node.kind() {
            "assume_block" => {
                let statements = self.build_block_statements(
                    id,
                    &node.child_by_field_name("body").unwrap(),
                    code,
                );
                let node = Rc::new(Block::new(id, location, statements));
                self.arena.add_node(
                    AstNode::Statement(Statement::Block(BlockType::Assume(node.clone()))),
                    parent_id,
                );
                BlockType::Assume(node)
            }
            "forall_block" => {
                let statements = self.build_block_statements(
                    id,
                    &node.child_by_field_name("body").unwrap(),
                    code,
                );
                let node = Rc::new(Block::new(id, location, statements));
                self.arena.add_node(
                    AstNode::Statement(Statement::Block(BlockType::Forall(node.clone()))),
                    parent_id,
                );
                BlockType::Forall(node)
            }
            "exists_block" => {
                let statements = self.build_block_statements(
                    id,
                    &node.child_by_field_name("body").unwrap(),
                    code,
                );
                let node = Rc::new(Block::new(id, location, statements));
                self.arena.add_node(
                    AstNode::Statement(Statement::Block(BlockType::Exists(node.clone()))),
                    parent_id,
                );
                BlockType::Exists(node)
            }
            "unique_block" => {
                let statements = self.build_block_statements(
                    id,
                    &node.child_by_field_name("body").unwrap(),
                    code,
                );
                let node = Rc::new(Block::new(id, location, statements));
                self.arena.add_node(
                    AstNode::Statement(Statement::Block(BlockType::Unique(node.clone()))),
                    parent_id,
                );
                BlockType::Unique(node)
            }
            "block" => {
                let statements = self.build_block_statements(id, node, code);
                let node = Rc::new(Block::new(id, location, statements));
                self.arena.add_node(
                    AstNode::Statement(Statement::Block(BlockType::Block(node.clone()))),
                    parent_id,
                );
                BlockType::Block(node)
            }
            _ => panic!(
                "Unexpected block type: {}, {}",
                node.kind(),
                Self::get_location(node, code)
            ),
        }
    }

    fn build_block_statements(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Vec<Statement> {
        let mut statements = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            statements.push(self.build_statement(parent_id, &child, code));
        }
        statements
    }

    fn build_statement(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> Statement {
        match node.kind() {
            "assign_statement" => {
                Statement::Assign(self.build_assign_statement(parent_id, node, code))
            }
            "block" | "forall_block" | "assume_block" | "exists_block" | "unique_block" => {
                Statement::Block(self.build_block(parent_id, node, code))
            }
            "expression_statement" => {
                let expr_node = node.child(0).unwrap();
                Statement::Expression(self.build_expression(parent_id, &expr_node, code))
            }
            "return_statement" => {
                Statement::Return(self.build_return_statement(parent_id, node, code))
            }
            "loop_statement" => Statement::Loop(self.build_loop_statement(parent_id, node, code)),
            "if_statement" => Statement::If(self.build_if_statement(parent_id, node, code)),
            "variable_definition_statement" => Statement::VariableDefinition(
                self.build_variable_definition_statement(parent_id, node, code),
            ),
            "type_definition_statement" => Statement::TypeDefinition(
                self.build_type_definition_statement(parent_id, node, code),
            ),
            "assert_statement" => {
                Statement::Assert(self.build_assert_statement(parent_id, node, code))
            }
            "break_statement" => {
                Statement::Break(self.build_break_statement(parent_id, node, code))
            }
            "constant_definition" => {
                Statement::ConstantDefinition(self.build_constant_definition(parent_id, node, code))
            }
            _ => panic!(
                "Unexpected statement type: {}, {}",
                node.kind(),
                Self::get_location(node, code)
            ),
        }
    }

    fn build_return_statement(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<ReturnStatement> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let expr_node = &node.child_by_field_name("expression");
        let expression = if let Some(expr) = expr_node {
            self.build_expression(id, expr, code)
        } else {
            Expression::Literal(Literal::Unit(Rc::new(UnitLiteral::new(
                Self::get_node_id(),
                Self::get_location(node, code),
            ))))
        };
        let node = Rc::new(ReturnStatement::new(id, location, expression));
        self.arena.add_node(
            AstNode::Statement(Statement::Return(node.clone())),
            parent_id,
        );
        node
    }

    fn build_loop_statement(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<LoopStatement> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let condition = node
            .child_by_field_name("condition")
            .map(|n| self.build_expression(id, &n, code));
        let body_block = node.child_by_field_name("body").unwrap();
        let body = self.build_block(id, &body_block, code);
        let node = Rc::new(LoopStatement::new(id, location, condition, body));
        self.arena
            .add_node(AstNode::Statement(Statement::Loop(node.clone())), parent_id);
        node
    }

    fn build_if_statement(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> Rc<IfStatement> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let condition_node = node.child_by_field_name("condition").unwrap();
        let condition = self.build_expression(id, &condition_node, code);
        let if_arm_node = node.child_by_field_name("if_arm").unwrap();
        let if_arm = self.build_block(id, &if_arm_node, code);
        let else_arm = node
            .child_by_field_name("else_arm")
            .map(|n| self.build_block(id, &n, code));
        let node = Rc::new(IfStatement::new(id, location, condition, if_arm, else_arm));
        self.arena
            .add_node(AstNode::Statement(Statement::If(node.clone())), parent_id);
        node
    }

    fn build_variable_definition_statement(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<VariableDefinitionStatement> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let ty = self.build_type(id, &node.child_by_field_name("type").unwrap(), code);
        let name = self.build_identifier(id, &node.child_by_field_name("name").unwrap(), code);
        let value = node
            .child_by_field_name("value")
            .map(|n| self.build_expression(id, &n, code));
        let is_undef = node.child_by_field_name("undef").is_some();

        let node = Rc::new(VariableDefinitionStatement::new(
            id, location, name, ty, value, is_undef,
        ));
        self.arena.add_node(
            AstNode::Statement(Statement::VariableDefinition(node.clone())),
            parent_id,
        );
        node
    }

    fn build_type_definition_statement(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<TypeDefinitionStatement> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let ty = self.build_type(id, &node.child_by_field_name("type").unwrap(), code);
        let name = self.build_identifier(id, &node.child_by_field_name("name").unwrap(), code);

        let node = Rc::new(TypeDefinitionStatement::new(id, location, name, ty));
        self.arena.add_node(
            AstNode::Statement(Statement::TypeDefinition(node.clone())),
            parent_id,
        );
        node
    }

    fn build_expression(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> Expression {
        let node_kind = node.kind();
        match node_kind {
            "array_index_access_expression" => Expression::ArrayIndexAccess(
                self.build_array_index_access_expression(parent_id, node, code),
            ),
            "generic_name" | "qualified_name" | "type" => {
                Expression::Type(self.build_type(parent_id, node, code))
            }
            "member_access_expression" => {
                Expression::MemberAccess(self.build_member_access_expression(parent_id, node, code))
            }
            "type_member_access_expression" => Expression::TypeMemberAccess(
                self.build_type_member_access_expression(parent_id, node, code),
            ),
            "function_call_expression" => {
                Expression::FunctionCall(self.build_function_call_expression(parent_id, node, code))
            }
            "struct_expression" => {
                Expression::Struct(self.build_struct_expression(parent_id, node, code))
            }
            "prefix_unary_expression" => {
                Expression::PrefixUnary(self.build_prefix_unary_expression(parent_id, node, code))
            }
            "parenthesized_expression" => Expression::Parenthesized(
                self.build_parenthesized_expression(parent_id, node, code),
            ),
            "binary_expression" => {
                Expression::Binary(self.build_binary_expression(parent_id, node, code))
            }
            "bool_literal" | "string_literal" | "number_literal" | "array_literal"
            | "unit_literal" => Expression::Literal(self.build_literal(parent_id, node, code)),
            "uzumaki_keyword" => {
                Expression::Uzumaki(self.build_uzumaki_expression(parent_id, node, code))
            }
            "identifier" => Expression::Identifier(self.build_identifier(parent_id, node, code)),
            _ => panic!(
                "Unexpected expression node kind: {node_kind} at {}",
                Self::get_location(node, code)
            ),
        }
    }

    fn build_assign_statement(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<AssignStatement> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let left = self.build_expression(id, &node.child_by_field_name("left").unwrap(), code);
        let right = self.build_expression(id, &node.child_by_field_name("right").unwrap(), code);

        let node = Rc::new(AssignStatement::new(id, location, left, right));
        self.arena.add_node(
            AstNode::Statement(Statement::Assign(node.clone())),
            parent_id,
        );
        node
    }

    fn build_array_index_access_expression(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<ArrayIndexAccessExpression> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let array = self.build_expression(id, &node.named_child(0).unwrap(), code);
        let index = self.build_expression(id, &node.named_child(1).unwrap(), code);

        let node = Rc::new(ArrayIndexAccessExpression::new(id, location, array, index));
        self.arena.add_node(
            AstNode::Expression(Expression::ArrayIndexAccess(node.clone())),
            parent_id,
        );
        node
    }

    fn build_member_access_expression(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<MemberAccessExpression> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let expression =
            self.build_expression(id, &node.child_by_field_name("expression").unwrap(), code);
        let name = self.build_identifier(id, &node.child_by_field_name("name").unwrap(), code);
        let node = Rc::new(MemberAccessExpression::new(id, location, expression, name));
        self.arena.add_node(
            AstNode::Expression(Expression::MemberAccess(node.clone())),
            parent_id,
        );
        node
    }

    fn build_type_member_access_expression(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<TypeMemberAccessExpression> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let expression =
            self.build_expression(id, &node.child_by_field_name("expression").unwrap(), code);
        let name = self.build_identifier(id, &node.child_by_field_name("name").unwrap(), code);
        let node = Rc::new(TypeMemberAccessExpression::new(
            id, location, expression, name,
        ));
        self.arena.add_node(
            AstNode::Expression(Expression::TypeMemberAccess(node.clone())),
            parent_id,
        );
        node
    }

    fn build_function_call_expression(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<FunctionCallExpression> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let function =
            self.build_expression(id, &node.child_by_field_name("function").unwrap(), code);
        let mut argument_name_expression_map: Vec<(Option<Rc<Identifier>>, Expression)> =
            Vec::new();
        let mut type_parameters = None;
        let mut pending_name: Option<Rc<Identifier>> = None;
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if let Some(field) = cursor.field_name() {
                    match field {
                        "argument_name" => {
                            let expr = self.build_expression(id, &child, code);
                            if let Expression::Identifier(ident) = expr {
                                pending_name = Some(ident);
                            }
                        }
                        "argument" => {
                            let expr = self.build_expression(id, &child, code);
                            let name = pending_name.take();
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

        if let Some(type_parameters_node) = node.child_by_field_name("type_parameters") {
            let mut cursor = type_parameters_node.walk();
            let founded_type_parameters = type_parameters_node
                .children_by_field_name("type", &mut cursor)
                .map(|segment| self.build_identifier(id, &segment, code));
            let founded_type_parameters: Vec<Rc<Identifier>> = founded_type_parameters.collect();
            if !founded_type_parameters.is_empty() {
                type_parameters = Some(founded_type_parameters);
            }
        }

        let node = Rc::new(FunctionCallExpression::new(
            id,
            location,
            function,
            type_parameters,
            arguments,
        ));
        self.arena.add_node(
            AstNode::Expression(Expression::FunctionCall(node.clone())),
            parent_id,
        );
        node
    }

    fn build_struct_expression(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<StructExpression> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let name = self.build_identifier(id, &node.child_by_field_name("name").unwrap(), code);
        let mut field_name_expression_map: Vec<(Rc<Identifier>, Expression)> = Vec::new();
        let mut pending_name: Option<Rc<Identifier>> = None;
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if let Some(field) = cursor.field_name() {
                    match field {
                        "field" => {
                            let expr = self.build_expression(id, &child, code);
                            if let Expression::Identifier(ident) = expr {
                                pending_name = Some(ident);
                            }
                        }
                        "value" => {
                            let expr = self.build_expression(id, &child, code);
                            let name = pending_name
                                .take()
                                .expect("pending_name is not initialized");
                            field_name_expression_map.push((name, expr));
                        }
                        _ => {}
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        let fields = if field_name_expression_map.is_empty() {
            None
        } else {
            Some(field_name_expression_map)
        };

        let node = Rc::new(StructExpression::new(id, location, name, fields));
        self.arena.add_node(
            AstNode::Expression(Expression::Struct(node.clone())),
            parent_id,
        );
        node
    }

    fn build_prefix_unary_expression(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<PrefixUnaryExpression> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let expression = self.build_expression(id, &node.child(1).unwrap(), code);

        let operator_node = node.child_by_field_name("operator").unwrap();
        let operator = match operator_node.kind() {
            "unary_not" => UnaryOperatorKind::Neg,
            _ => panic!("Unexpected operator node"),
        };

        let node = Rc::new(PrefixUnaryExpression::new(
            id, location, expression, operator,
        ));
        self.arena.add_node(
            AstNode::Expression(Expression::PrefixUnary(node.clone())),
            parent_id,
        );
        node
    }

    fn build_assert_statement(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<AssertStatement> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let expression = self.build_expression(id, &node.child(1).unwrap(), code);
        let node = Rc::new(AssertStatement::new(id, location, expression));
        self.arena.add_node(
            AstNode::Statement(Statement::Assert(node.clone())),
            parent_id,
        );
        node
    }

    fn build_break_statement(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<BreakStatement> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let node = Rc::new(BreakStatement::new(id, location));
        self.arena.add_node(
            AstNode::Statement(Statement::Break(node.clone())),
            parent_id,
        );
        node
    }

    fn build_parenthesized_expression(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<ParenthesizedExpression> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let expression = self.build_expression(id, &node.child(1).unwrap(), code);

        let node = Rc::new(ParenthesizedExpression::new(id, location, expression));
        self.arena.add_node(
            AstNode::Expression(Expression::Parenthesized(node.clone())),
            parent_id,
        );
        node
    }

    fn build_binary_expression(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<BinaryExpression> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let left = self.build_expression(id, &node.child_by_field_name("left").unwrap(), code);
        let operator_node = node.child_by_field_name("operator").unwrap();
        let operator_kind = operator_node.kind();
        let operator = match operator_kind {
            "**" => OperatorKind::Pow,
            "&&" => OperatorKind::And,
            "||" => OperatorKind::Or,
            "+" => OperatorKind::Add,
            "-" => OperatorKind::Sub,
            "*" => OperatorKind::Mul,
            "%" => OperatorKind::Mod,
            "<" => OperatorKind::Lt,
            "<=" => OperatorKind::Le,
            "==" => OperatorKind::Eq,
            "!=" => OperatorKind::Ne,
            ">=" => OperatorKind::Ge,
            ">" => OperatorKind::Gt,
            "<<" => OperatorKind::Shl,
            ">>" => OperatorKind::Shr,
            "^" => OperatorKind::BitXor,
            "&" => OperatorKind::BitAnd,
            "|" => OperatorKind::BitOr,
            _ => panic!("Unexpected operator node: {operator_kind}"),
        };

        let right = self.build_expression(id, &node.child_by_field_name("right").unwrap(), code);

        let node = Rc::new(BinaryExpression::new(id, location, left, operator, right));
        self.arena.add_node(
            AstNode::Expression(Expression::Binary(node.clone())),
            parent_id,
        );
        node
    }

    fn build_literal(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> Literal {
        match node.kind() {
            "array_literal" => Literal::Array(self.build_array_literal(parent_id, node, code)),
            "bool_literal" => Literal::Bool(self.build_bool_literal(parent_id, node, code)),
            "string_literal" => Literal::String(self.build_string_literal(parent_id, node, code)),
            "number_literal" => Literal::Number(self.build_number_literal(parent_id, node, code)),
            "unit_literal" => Literal::Unit(self.build_unit_literal(parent_id, node, code)),
            _ => panic!("Unexpected literal type: {}", node.kind()),
        }
    }

    fn build_array_literal(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<ArrayLiteral> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let mut elements = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            elements.push(self.build_expression(id, &child, code));
        }

        let elements = if elements.is_empty() {
            None
        } else {
            Some(elements)
        };
        let node = Rc::new(ArrayLiteral::new(id, location, elements));
        self.arena.add_node(
            AstNode::Expression(Expression::Literal(Literal::Array(node.clone()))),
            parent_id,
        );
        node
    }

    fn build_bool_literal(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> Rc<BoolLiteral> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let value = match node.utf8_text(code).unwrap() {
            "true" => true,
            "false" => false,
            _ => panic!("Unexpected boolean literal value"),
        };

        let node = Rc::new(BoolLiteral::new(id, location, value));
        self.arena.add_node(
            AstNode::Expression(Expression::Literal(Literal::Bool(node.clone()))),
            parent_id,
        );
        node
    }

    fn build_string_literal(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<StringLiteral> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let value = node.utf8_text(code).unwrap().to_string();
        let node = Rc::new(StringLiteral::new(id, location, value));
        self.arena.add_node(
            AstNode::Expression(Expression::Literal(Literal::String(node.clone()))),
            parent_id,
        );
        node
    }

    fn build_number_literal(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<NumberLiteral> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let value = node.utf8_text(code).unwrap().to_string();
        let node = Rc::new(NumberLiteral::new(id, location, value));
        self.arena.add_node(
            AstNode::Expression(Expression::Literal(Literal::Number(node.clone()))),
            parent_id,
        );
        node
    }

    fn build_unit_literal(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> Rc<UnitLiteral> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let node = Rc::new(UnitLiteral::new(id, location));
        self.arena.add_node(
            AstNode::Expression(Expression::Literal(Literal::Unit(node.clone()))),
            parent_id,
        );
        node
    }

    fn build_type(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> Type {
        let node_kind = node.kind();
        match node_kind {
            "type_array" => Type::Array(self.build_type_array(parent_id, node, code)),
            "type_i8" | "type_i16" | "type_i32" | "type_i64" | "type_u8" | "type_u16"
            | "type_u32" | "type_u64" | "type_bool" | "type_unit" => {
                Type::Simple(self.build_simple_type(parent_id, node, code))
            }
            "generic_type" | "generic_name" => {
                Type::Generic(self.build_generic_type(parent_id, node, code))
            }
            "type_qualified_name" => {
                Type::Qualified(self.build_type_qualified_name(parent_id, node, code))
            }
            "qualified_name" => {
                Type::QualifiedName(self.build_qualified_name(parent_id, node, code))
            }
            "type_fn" => Type::Function(self.build_function_type(parent_id, node, code)),
            "identifier" => {
                let name = self.build_identifier(parent_id, node, code);
                Type::Custom(name)
            }
            _ => {
                let location = Self::get_location(node, code);
                panic!("Unexpected type: {node_kind}, {location}")
            }
        }
    }

    fn build_type_array(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> Rc<TypeArray> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let element_type = self.build_type(id, &node.child_by_field_name("type").unwrap(), code);
        let size = node
            .child_by_field_name("length")
            .map(|n| self.build_expression(id, &n, code));

        let node = Rc::new(TypeArray::new(id, location, element_type, size));
        self.arena.add_node(
            AstNode::Expression(Expression::Type(Type::Array(node.clone()))),
            parent_id,
        );
        node
    }

    fn build_simple_type(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> Rc<SimpleType> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let name = if node.kind() == "type_unit" {
            String::from("unit")
        } else {
            node.utf8_text(code).unwrap().to_string()
        };
        let node = Rc::new(SimpleType::new(id, location, name));
        self.arena.add_node(
            AstNode::Expression(Expression::Type(Type::Simple(node.clone()))),
            parent_id,
        );
        node
    }

    fn build_generic_type(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> Rc<GenericType> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let base = self.build_identifier(id, &node.child_by_field_name("base_type").unwrap(), code);

        let args = node.child(1).unwrap();

        let mut cursor = args.walk();

        let types = args
            .children_by_field_name("type", &mut cursor)
            .map(|segment| self.build_identifier(id, &segment, code));
        let parameters: Vec<Rc<Identifier>> = types.collect();

        let node = Rc::new(GenericType::new(id, location, base, parameters));
        self.arena.add_node(
            AstNode::Expression(Expression::Type(Type::Generic(node.clone()))),
            parent_id,
        );
        node
    }

    fn build_function_type(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<FunctionType> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let mut arguments = None;
        let mut cursor = node.walk();
        let mut returns = None;

        let founded_arguments = node
            .children_by_field_name("argument", &mut cursor)
            .map(|segment| self.build_type(id, &segment, code));
        let founded_arguments: Vec<Type> = founded_arguments.collect();
        if !founded_arguments.is_empty() {
            arguments = Some(founded_arguments);
        }
        if let Some(returns_type_node) = node.child_by_field_name("returns") {
            returns = Some(self.build_type(id, &returns_type_node, code));
        }
        let node = Rc::new(FunctionType::new(id, location, arguments, returns));
        self.arena.add_node(
            AstNode::Expression(Expression::Type(Type::Function(node.clone()))),
            parent_id,
        );
        node
    }

    fn build_type_qualified_name(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<TypeQualifiedName> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let alias = self.build_identifier(id, &node.child_by_field_name("alias").unwrap(), code);
        let name = self.build_identifier(id, &node.child_by_field_name("name").unwrap(), code);

        let node = Rc::new(TypeQualifiedName::new(id, location, alias, name));
        self.arena.add_node(
            AstNode::Expression(Expression::Type(Type::Qualified(node.clone()))),
            parent_id,
        );
        node
    }

    fn build_qualified_name(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<QualifiedName> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let qualifier =
            self.build_identifier(id, &node.child_by_field_name("qualifier").unwrap(), code);
        let name = self.build_identifier(id, &node.child_by_field_name("name").unwrap(), code);

        let node = Rc::new(QualifiedName::new(id, location, qualifier, name));
        self.arena.add_node(
            AstNode::Expression(Expression::Type(Type::QualifiedName(node.clone()))),
            parent_id,
        );
        node
    }

    fn build_uzumaki_expression(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<UzumakiExpression> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let node = Rc::new(UzumakiExpression::new(id, location));
        self.arena.add_node(
            AstNode::Expression(Expression::Uzumaki(node.clone())),
            parent_id,
        );
        node
    }

    fn build_identifier(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> Rc<Identifier> {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let name = node.utf8_text(code).unwrap().to_string();
        let node = Rc::new(Identifier::new(id, name, location));
        self.arena.add_node(
            AstNode::Expression(Expression::Identifier(node.clone())),
            parent_id,
        );
        node
    }

    #[allow(clippy::cast_possible_truncation)]
    fn get_node_id() -> u32 {
        uuid::Uuid::new_v4().as_u128() as u32
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

impl Builder<'_, CompleteState> {}
