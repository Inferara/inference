//! AST builder that converts tree-sitter concrete syntax trees (CST) into typed AST nodes.
//!
//! The `Builder` processes tree-sitter parse trees and constructs a typed Abstract Syntax Tree
//! stored in an `Arena`. It handles:
//!
//! - Converting CST nodes to typed AST nodes
//! - Assigning unique sequential IDs to each node
//! - Recording parent-child relationships in the arena
//! - Collecting parse errors from malformed syntax
//! - Extracting source location information
//!
//! # Example
//!
//! ```no_run
//! use inference_ast::builder::Builder;
//! use tree_sitter::Parser;
//!
//! let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
//! let mut parser = Parser::new();
//! parser.set_language(&tree_sitter_inference::language()).unwrap();
//! let tree = parser.parse(source, None).unwrap();
//!
//! let mut builder = Builder::new();
//! builder.add_source_code(tree.root_node(), source.as_bytes());
//! let arena = builder.build_ast().unwrap();
//! ```
//!
//! # Error Handling
//!
//! The builder collects errors during construction by checking for tree-sitter ERROR nodes.
//! If any errors are found, `build_ast()` prints them to stderr and returns an error:
//!
//! ```text
//! AST Builder Error: Syntax error at line 5
//! AST Builder Error: Unexpected token at line 10
//! Error: AST building failed due to errors
//! ```
//!
//! # Node ID Assignment
//!
//! Node IDs are assigned sequentially starting from 1 using an atomic counter:
//!
//! - **Deterministic ordering**: IDs match parse order for easier debugging
//! - **Thread-safe**: Uses `AtomicU32` with relaxed ordering
//! - **Zero is reserved**: ID 0 represents invalid/uninitialized nodes
//! - **Sentinel value**: `u32::MAX` represents "no ID" for non-node types
//!
//! # Implementation Details
//!
//! The builder walks the tree-sitter CST depth-first, creating typed AST nodes:
//!
//! 1. For each CST node, determine its kind (e.g., `function_definition`)
//! 2. Extract relevant child nodes by field name (e.g., "name", "body")
//! 3. Recursively build child AST nodes
//! 4. Create the parent AST node with references to children
//! 5. Add to arena with parent-child relationship
//!
//! The builder also calls `collect_errors()` for each processed node to identify
//! tree-sitter ERROR nodes from parse failures.

use std::{
    rc::Rc,
    sync::atomic::{AtomicU32, Ordering},
};

use crate::nodes::{
    ArgumentType, Ast, Directive, IgnoreArgument, Misc, ModuleDefinition, SelfReference,
    StructExpression, TypeMemberAccessExpression, Visibility,
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
        ReturnStatement, SimpleTypeKind, SourceFile, SpecDefinition, Statement, StringLiteral,
        StructDefinition, StructField, Type, TypeArray, TypeDefinition, TypeDefinitionStatement,
        TypeQualifiedName, UnaryOperatorKind, UnitLiteral, UseDirective, UzumakiExpression,
        VariableDefinitionStatement,
    },
};
use tree_sitter::Node;

pub struct Builder<'a> {
    arena: Arena,
    source_code: Vec<(Node<'a>, &'a [u8])>,
    errors: Vec<anyhow::Error>,
}

impl Default for Builder<'_> {
    fn default() -> Self {
        Builder::new()
    }
}

impl<'a> Builder<'a> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            arena: Arena::default(),
            source_code: Vec::new(),
            errors: Vec::new(),
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
    pub fn build_ast(&'_ mut self) -> anyhow::Result<Arena> {
        for (root, code) in &self.source_code.clone() {
            let id = Self::get_node_id();
            let location = Self::get_location(root, code);
            let source = String::from_utf8_lossy(code);
            debug_assert!(
                !source.contains('\u{FFFD}'),
                "Source code contains invalid UTF-8"
            );
            let source = source.into_owned();
            let mut ast = SourceFile::new(id, location, source);

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
            self.arena
                .add_node(AstNode::Ast(Ast::SourceFile(Rc::new(ast))), u32::MAX);
            if !self.errors.is_empty() {
                for err in &self.errors {
                    eprintln!("AST Builder Error: {err}");
                }
                return Err(anyhow::anyhow!("AST building failed due to errors"));
            }
        }
        Ok(self.arena.clone())
    }

    fn build_use_directive(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<UseDirective> {
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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

        let node = Rc::new(SpecDefinition::new(
            id,
            Visibility::default(),
            name,
            definitions,
            location,
        ));
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
        self.collect_errors(node, code);
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

        let node = Rc::new(EnumDefinition::new(
            id,
            Self::get_visibility(node),
            name,
            variants,
            location,
        ));
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
            "ERROR" => {
                self.errors.push(anyhow::anyhow!(
                    "Syntax error at {}: unexpected or malformed token",
                    Self::get_location(node, code)
                ));
                Self::create_error_definition(node, code)
            }
            _ => {
                self.errors.push(anyhow::anyhow!(
                    "Unexpected definition kind '{}' at {}",
                    node.kind(),
                    Self::get_location(node, code)
                ));
                Self::create_error_definition(node, code)
            }
        }
    }

    /// Creates a placeholder function definition for error recovery.
    /// This preserves AST structure with location info while marking the node as erroneous.
    fn create_error_definition(node: &Node, code: &[u8]) -> Definition {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let name = Rc::new(Identifier::new(
            Self::get_node_id(),
            "<error>".to_string(),
            location,
        ));
        let body = BlockType::Block(Rc::new(Block::new(Self::get_node_id(), location, vec![])));
        Definition::Function(Rc::new(FunctionDefinition::new(
            id,
            Visibility::Private,
            name,
            None,
            None,
            None,
            body,
            location,
        )))
    }

    fn build_struct_definition(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<StructDefinition> {
        self.collect_errors(node, code);
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
            .filter(|n| n.kind() == "function_definition")
            .map(|segment| self.build_function_definition(id, &segment, code));
        let methods: Vec<Rc<FunctionDefinition>> = founded_methods.collect();

        let node = Rc::new(StructDefinition::new(
            id,
            Self::get_visibility(node),
            name,
            fields,
            methods,
            location,
        ));
        self.arena.add_node(
            AstNode::Definition(Definition::Struct(node.clone())),
            parent_id,
        );
        node
    }

    fn build_struct_field(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> Rc<StructField> {
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let ty = self.build_type(id, &node.child_by_field_name("type").unwrap(), code);
        let name = self.build_identifier(id, &node.child_by_field_name("name").unwrap(), code);
        let value = self.build_literal(id, &node.child_by_field_name("value").unwrap(), code);

        let node = Rc::new(ConstantDefinition::new(
            id,
            Self::get_visibility(node),
            name,
            ty,
            value,
            location,
        ));
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
        self.collect_errors(node, code);
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
        let Some(name_node) = node.child_by_field_name("name") else {
            self.errors.push(anyhow::anyhow!(
                "Missing function name at {}",
                Self::get_location(node, code)
            ));
            let placeholder_name = Rc::new(Identifier::new(
                Self::get_node_id(),
                "<error>".to_string(),
                location,
            ));
            let placeholder_body = BlockType::Block(Rc::new(Block::new(
                Self::get_node_id(),
                location,
                Vec::new(),
            )));
            return Rc::new(FunctionDefinition::new(
                id,
                Visibility::default(),
                placeholder_name,
                None,
                None,
                None,
                placeholder_body,
                location,
            ));
        };
        let name = self.build_identifier(id, &name_node, code);
        let body = if let Some(body_node) = node.child_by_field_name("body") {
            self.build_block(id, &body_node, code)
        } else {
            self.errors.push(anyhow::anyhow!(
                "Missing function body at {}",
                Self::get_location(node, code)
            ));
            BlockType::Block(Rc::new(Block::new(
                Self::get_node_id(),
                Self::get_location(node, code),
                Vec::new(),
            )))
        };
        let node = Rc::new(FunctionDefinition::new(
            id,
            Self::get_visibility(node),
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
        self.collect_errors(node, code);
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
            id,
            Visibility::default(),
            name,
            arguments,
            returns,
            location,
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
        self.collect_errors(node, code);
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let ty = self.build_type(id, &node.child_by_field_name("type").unwrap(), code);
        let name = self.build_identifier(id, &node.child_by_field_name("name").unwrap(), code);
        let node = Rc::new(TypeDefinition::new(
            id,
            Self::get_visibility(node),
            name,
            ty,
            location,
        ));
        self.arena.add_node(
            AstNode::Definition(Definition::Type(node.clone())),
            parent_id,
        );
        node
    }

    /// Builds a module definition node.
    ///
    /// # Not Yet Implemented
    ///
    /// Module parsing requires tree-sitter grammar support for module declarations.
    /// The Inference grammar does not currently support `mod name;` or `mod name { ... }`
    /// syntax. When grammar support is added, this function will:
    ///
    /// 1. Parse the module name from the CST node
    /// 2. Determine if it's an external (`mod name;`) or inline (`mod name { ... }`) module
    /// 3. Build the `ModuleDefinition` AST node
    /// 4. Add it to the arena
    ///
    /// See `ParserContext::process_module()` for the planned integration point.
    #[allow(dead_code)]
    fn build_module_definition(
        &mut self,
        _parent_id: u32,
        _node: &Node,
        _code: &[u8],
    ) -> Rc<ModuleDefinition> {
        unimplemented!("Module definitions are not yet supported in the grammar")
    }

    fn build_argument_type(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> ArgumentType {
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        match node.kind() {
            "assume_block" => {
                let statements = node
                    .child_by_field_name("body")
                    .map(|body_node| self.build_block_statements(id, &body_node, code))
                    .unwrap_or_default();
                let node = Rc::new(Block::new(id, location, statements));
                self.arena.add_node(
                    AstNode::Statement(Statement::Block(BlockType::Assume(node.clone()))),
                    parent_id,
                );
                BlockType::Assume(node)
            }
            "forall_block" => {
                let statements = node
                    .child_by_field_name("body")
                    .map(|body_node| self.build_block_statements(id, &body_node, code))
                    .unwrap_or_default();
                let node = Rc::new(Block::new(id, location, statements));
                self.arena.add_node(
                    AstNode::Statement(Statement::Block(BlockType::Forall(node.clone()))),
                    parent_id,
                );
                BlockType::Forall(node)
            }
            "exists_block" => {
                let statements = node
                    .child_by_field_name("body")
                    .map(|body_node| self.build_block_statements(id, &body_node, code))
                    .unwrap_or_default();
                let node = Rc::new(Block::new(id, location, statements));
                self.arena.add_node(
                    AstNode::Statement(Statement::Block(BlockType::Exists(node.clone()))),
                    parent_id,
                );
                BlockType::Exists(node)
            }
            "unique_block" => {
                let statements = node
                    .child_by_field_name("body")
                    .map(|body_node| self.build_block_statements(id, &body_node, code))
                    .unwrap_or_default();
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
            "ERROR" => {
                self.errors.push(anyhow::anyhow!(
                    "Syntax error in block at {}",
                    Self::get_location(node, code)
                ));
                self.create_error_block(node, code, parent_id)
            }
            _ => {
                self.errors.push(anyhow::anyhow!(
                    "Unexpected block type '{}' at {}",
                    node.kind(),
                    Self::get_location(node, code)
                ));
                self.create_error_block(node, code, parent_id)
            }
        }
    }

    /// Creates a placeholder empty block for error recovery.
    fn create_error_block(&mut self, node: &Node, code: &[u8], parent_id: u32) -> BlockType {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let block = Rc::new(Block::new(id, location, vec![]));
        self.arena.add_node(
            AstNode::Statement(Statement::Block(BlockType::Block(block.clone()))),
            parent_id,
        );
        BlockType::Block(block)
    }

    fn build_block_statements(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Vec<Statement> {
        let mut statements = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            self.collect_errors(&child, code);

            if child.is_named() {
                let stmt = self.build_statement(parent_id, &child, code);
                statements.push(stmt);
            }
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
                if let Some(expr_node) = node.child(0) {
                    Statement::Expression(self.build_expression(parent_id, &expr_node, code))
                } else {
                    self.create_error_statement(node, code, parent_id)
                }
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
            "ERROR" => {
                self.errors.push(anyhow::anyhow!(
                    "Syntax error in statement at {}",
                    Self::get_location(node, code)
                ));
                self.create_error_statement(node, code, parent_id)
            }
            _ => {
                self.errors.push(anyhow::anyhow!(
                    "Unexpected statement type '{}' at {}",
                    node.kind(),
                    Self::get_location(node, code)
                ));
                self.create_error_statement(node, code, parent_id)
            }
        }
    }

    /// Creates a placeholder expression statement for error recovery.
    fn create_error_statement(&mut self, node: &Node, code: &[u8], parent_id: u32) -> Statement {
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let error_ident = Rc::new(Identifier::new(id, "<error>".to_string(), location));
        let stmt = Statement::Expression(Expression::Identifier(error_ident.clone()));
        self.arena.add_node(
            AstNode::Expression(Expression::Identifier(error_ident)),
            parent_id,
        );
        stmt
    }

    fn build_return_statement(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<ReturnStatement> {
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let condition = node
            .child_by_field_name("condition")
            .map(|n| self.build_expression(id, &n, code));
        let body = if let Some(body_block) = node.child_by_field_name("body") {
            self.build_block(id, &body_block, code)
        } else {
            self.errors.push(anyhow::anyhow!(
                "Missing loop body at {}",
                Self::get_location(node, code)
            ));
            BlockType::Block(Rc::new(Block::new(Self::get_node_id(), location, vec![])))
        };
        let node = Rc::new(LoopStatement::new(id, location, condition, body));
        self.arena
            .add_node(AstNode::Statement(Statement::Loop(node.clone())), parent_id);
        node
    }

    fn build_if_statement(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> Rc<IfStatement> {
        self.collect_errors(node, code);
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let condition = if let Some(condition_node) = node.child_by_field_name("condition") {
            self.build_expression(id, &condition_node, code)
        } else {
            self.errors.push(anyhow::anyhow!(
                "Missing if condition at {}",
                Self::get_location(node, code)
            ));
            Expression::Identifier(Rc::new(Identifier::new(
                Self::get_node_id(),
                "<error>".to_string(),
                location,
            )))
        };
        let if_arm = if let Some(if_arm_node) = node.child_by_field_name("if_arm") {
            self.build_block(id, &if_arm_node, code)
        } else {
            self.errors.push(anyhow::anyhow!(
                "Missing if body at {}",
                Self::get_location(node, code)
            ));
            BlockType::Block(Rc::new(Block::new(Self::get_node_id(), location, vec![])))
        };
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
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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
            "ERROR" => {
                self.errors.push(anyhow::anyhow!(
                    "Syntax error in expression at {}",
                    Self::get_location(node, code)
                ));
                let location = Self::get_location(node, code);
                Expression::Identifier(Rc::new(Identifier::new(
                    Self::get_node_id(),
                    "<error>".to_string(),
                    location,
                )))
            }
            _ => {
                self.errors.push(anyhow::anyhow!(
                    "Unexpected expression node kind '{}' at {}",
                    node_kind,
                    Self::get_location(node, code)
                ));
                let location = Self::get_location(node, code);
                Expression::Identifier(Rc::new(Identifier::new(
                    Self::get_node_id(),
                    "<error>".to_string(),
                    location,
                )))
            }
        }
    }

    fn build_assign_statement(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<AssignStatement> {
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let expression = self.build_expression(id, &node.child(1).unwrap(), code);

        let operator_node = node.child_by_field_name("operator").unwrap();
        let operator = match operator_node.kind() {
            "unary_not" => UnaryOperatorKind::Not,
            "unary_minus" => UnaryOperatorKind::Neg,
            "unary_bitnot" => UnaryOperatorKind::BitNot,
            other => unreachable!("Unexpected unary operator node: {other}"),
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
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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
            "/" => OperatorKind::Div,
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
            _ => {
                self.errors.push(anyhow::anyhow!(
                    "Unexpected operator '{}' at {}",
                    operator_kind,
                    Self::get_location(node, code)
                ));
                OperatorKind::Add
            }
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
            _ => {
                self.errors.push(anyhow::anyhow!(
                    "Unexpected literal type '{}' at {}",
                    node.kind(),
                    Self::get_location(node, code)
                ));
                Literal::Unit(Rc::new(UnitLiteral::new(
                    Self::get_node_id(),
                    Self::get_location(node, code),
                )))
            }
        }
    }

    fn build_array_literal(
        &mut self,
        parent_id: u32,
        node: &Node,
        code: &[u8],
    ) -> Rc<ArrayLiteral> {
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let text = node.utf8_text(code).unwrap_or("");
        let value = match text {
            "true" => true,
            "false" => false,
            _ => {
                self.errors.push(anyhow::anyhow!(
                    "Unexpected boolean literal value '{}' at {}",
                    text,
                    Self::get_location(node, code)
                ));
                false
            }
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
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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
            "type_unit" => Type::Simple(SimpleTypeKind::Unit),
            "type_bool" => Type::Simple(SimpleTypeKind::Bool),
            "type_i8" => Type::Simple(SimpleTypeKind::I8),
            "type_i16" => Type::Simple(SimpleTypeKind::I16),
            "type_i32" => Type::Simple(SimpleTypeKind::I32),
            "type_i64" => Type::Simple(SimpleTypeKind::I64),
            "type_u8" => Type::Simple(SimpleTypeKind::U8),
            "type_u16" => Type::Simple(SimpleTypeKind::U16),
            "type_u32" => Type::Simple(SimpleTypeKind::U32),
            "type_u64" => Type::Simple(SimpleTypeKind::U64),
            "type_array" => Type::Array(self.build_type_array(parent_id, node, code)),
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
            "ERROR" => {
                self.errors.push(anyhow::anyhow!(
                    "Syntax error in type at {}",
                    Self::get_location(node, code)
                ));
                Type::Simple(SimpleTypeKind::Unit)
            }
            _ => {
                self.errors.push(anyhow::anyhow!(
                    "Unexpected type '{}' at {}",
                    node_kind,
                    Self::get_location(node, code)
                ));
                Type::Simple(SimpleTypeKind::Unit)
            }
        }
    }

    fn build_type_array(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> Rc<TypeArray> {
        self.collect_errors(node, code);
        let id = Self::get_node_id();
        let location = Self::get_location(node, code);
        let element_type = self.build_type(id, &node.child_by_field_name("type").unwrap(), code);
        let length_node = node.child_by_field_name("length").unwrap();
        let size = self.build_expression(id, &length_node, code);

        let node = Rc::new(TypeArray::new(id, location, element_type, size));
        self.arena.add_node(
            AstNode::Expression(Expression::Type(Type::Array(node.clone()))),
            parent_id,
        );
        node
    }

    fn build_generic_type(&mut self, parent_id: u32, node: &Node, code: &[u8]) -> Rc<GenericType> {
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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
        self.collect_errors(node, code);
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

    /// Generate a unique node ID using an atomic counter.
    ///
    /// Uses a global atomic counter to ensure unique IDs across all AST nodes.
    /// Starting from 1 (0 is reserved as invalid/uninitialized).
    fn get_node_id() -> u32 {
        static COUNTER: AtomicU32 = AtomicU32::new(1);
        COUNTER.fetch_add(1, Ordering::Relaxed)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn get_location(node: &Node, _code: &[u8]) -> Location {
        let offset_start = node.start_byte() as u32;
        let offset_end = node.end_byte() as u32;
        let start_position = node.start_position();
        let end_position = node.end_position();
        let start_line = start_position.row as u32 + 1;
        let start_column = start_position.column as u32 + 1;
        let end_line = end_position.row as u32 + 1;
        let end_column = end_position.column as u32 + 1;

        Location {
            offset_start,
            offset_end,
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }

    fn collect_errors(&mut self, node: &Node, code: &[u8]) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.is_error() {
                let location = Self::get_location(&child, code);
                let source_snippet = String::from_utf8_lossy(
                    &code[location.offset_start as usize..location.offset_end as usize],
                );
                self.errors.push(anyhow::anyhow!(
                    "Parse error: invalid syntax at line {}:{} near '{}'",
                    location.start_line,
                    location.start_column,
                    source_snippet.chars().take(30).collect::<String>()
                ));
            }
        }
    }

    /// Extracts visibility modifier from a definition CST node.
    /// Returns `Visibility::Public` if a "visibility" child field is present,
    /// otherwise returns `Visibility::Private` (the default).
    fn get_visibility(node: &Node) -> Visibility {
        node.child_by_field_name("visibility")
            .map(|_| Visibility::Public)
            .unwrap_or_default()
    }
}
