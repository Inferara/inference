//! AST builder that converts tree-sitter concrete syntax trees (CST) into typed AST nodes.
//!
//! The `Builder` processes tree-sitter parse trees and constructs a typed Abstract Syntax Tree
//! stored in an `AstArena`. It handles:
//!
//! - Converting CST nodes to typed AST nodes
//! - Arena allocation via typed ID indices
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

use crate::arena::AstArena;
use crate::ids::{BlockId, DefId, ExprId, IdentId, StmtId, TypeId};
use crate::nodes::{
    ArgData, ArgKind, BlockData, BlockKind, Def, DefData, Directive, Expr, ExprData, Field, Ident,
    Location, OperatorKind, SimpleTypeKind, SourceFileData, Stmt, StmtData, TypeData, TypeNode,
    UnaryOperatorKind, UseDirective, Visibility,
};
use tree_sitter::Node;

pub struct Builder<'a> {
    arena: AstArena,
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
            arena: AstArena::default(),
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
    /// # Errors
    ///
    /// Returns an error if the source contains syntax errors.
    ///
    /// # Panics
    ///
    /// This function may panic if the CST structure does not match expected patterns,
    /// but it will attempt to recover from syntax errors by inserting placeholder nodes and collecting error messages.
    #[allow(clippy::single_match_else)]
    pub fn build_ast(&mut self) -> anyhow::Result<AstArena> {
        for (root, code) in &self.source_code.clone() {
            let location = Self::get_location(root, code);
            let source = String::from_utf8_lossy(code);
            debug_assert!(
                !source.contains('\u{FFFD}'),
                "Source code contains invalid UTF-8"
            );
            let source = source.into_owned();

            let mut defs = Vec::new();
            let mut directives = Vec::new();

            for i in 0..root.child_count() {
                if let Some(child) = root.child(u32::try_from(i).unwrap()) {
                    let child_kind = child.kind();

                    match child_kind {
                        "use_directive" => {
                            directives.push(Directive::Use(self.build_use_directive(&child, code)));
                        }
                        _ => {
                            let def_id = self.build_definition(&child, code);
                            defs.push(def_id);
                        }
                    }
                }
            }

            self.arena.source_files.alloc(SourceFileData {
                location,
                source,
                defs,
                directives,
            });

            if !self.errors.is_empty() {
                for err in &self.errors {
                    eprintln!("AST Builder Error: {err}");
                }
                return Err(anyhow::anyhow!("AST building failed due to errors"));
            }
        }
        Ok(std::mem::take(&mut self.arena))
    }

    fn build_use_directive(&mut self, node: &Node, code: &[u8]) -> UseDirective {
        self.collect_errors(node, code);
        let location = Self::get_location(node, code);
        let mut segments = Vec::new();
        let mut from = None;
        let mut cursor = node.walk();

        if let Some(from_literal) = node.child_by_field_name("from_literal") {
            from = Some(self.build_string_literal_value(&from_literal, code));
        } else {
            segments = node
                .children_by_field_name("segment", &mut cursor)
                .map(|segment| self.build_identifier(&segment, code))
                .collect();
        }

        cursor = node.walk();
        let imported_types: Vec<IdentId> = node
            .children_by_field_name("imported_type", &mut cursor)
            .map(|imported_type| self.build_identifier(&imported_type, code))
            .collect();

        UseDirective {
            location,
            imported_types,
            segments,
            from,
        }
    }

    fn build_spec_definition(&mut self, node: &Node, code: &[u8]) -> DefId {
        self.collect_errors(node, code);
        let location = Self::get_location(node, code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
        let mut defs = Vec::new();

        for i in 1..node.named_child_count() {
            let child = node.named_child(u32::try_from(i).unwrap()).unwrap();
            let def_id = self.build_definition(&child, code);
            defs.push(def_id);
        }

        self.arena.defs.alloc(DefData {
            location,
            kind: Def::Spec {
                name,
                vis: Visibility::default(),
                defs,
            },
        })
    }

    fn build_enum_definition(&mut self, node: &Node, code: &[u8]) -> DefId {
        self.collect_errors(node, code);
        let location = Self::get_location(node, code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);

        let mut cursor = node.walk();
        let variants: Vec<IdentId> = node
            .children_by_field_name("variant", &mut cursor)
            .map(|segment| self.build_identifier(&segment, code))
            .collect();

        self.arena.defs.alloc(DefData {
            location,
            kind: Def::Enum {
                name,
                vis: Self::get_visibility(node),
                variants,
            },
        })
    }

    fn build_definition(&mut self, node: &Node, code: &[u8]) -> DefId {
        let kind = node.kind();
        match kind {
            "spec_definition" => self.build_spec_definition(node, code),
            "struct_definition" => self.build_struct_definition(node, code),
            "enum_definition" => self.build_enum_definition(node, code),
            "constant_definition" => self.build_constant_definition(node, code),
            "function_definition" => self.build_function_definition(node, code),
            "external_function_definition" => self.build_external_function_definition(node, code),
            "type_definition_statement" => self.build_type_alias_definition(node, code),
            "ERROR" => {
                cov_mark::hit!(ast_builder_error_definition_recovery);
                self.errors.push(anyhow::anyhow!(
                    "Syntax error at {}: unexpected or malformed token",
                    Self::get_location(node, code)
                ));
                self.create_error_definition(node, code)
            }
            _ => {
                self.errors.push(anyhow::anyhow!(
                    "Unexpected definition kind '{}' at {}",
                    node.kind(),
                    Self::get_location(node, code)
                ));
                self.create_error_definition(node, code)
            }
        }
    }

    fn create_error_definition(&mut self, node: &Node, code: &[u8]) -> DefId {
        let location = Self::get_location(node, code);
        let name = self.arena.idents.alloc(Ident {
            location,
            name: "<error>".to_string(),
        });
        let body = self.arena.blocks.alloc(BlockData {
            location,
            block_kind: BlockKind::Regular,
            stmts: vec![],
        });
        self.arena.defs.alloc(DefData {
            location,
            kind: Def::Function {
                name,
                vis: Visibility::Private,
                type_params: vec![],
                args: vec![],
                returns: None,
                body,
            },
        })
    }

    fn build_struct_definition(&mut self, node: &Node, code: &[u8]) -> DefId {
        self.collect_errors(node, code);
        let location = Self::get_location(node, code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);

        let mut cursor = node.walk();
        let fields: Vec<Field> = node
            .children_by_field_name("field", &mut cursor)
            .map(|segment| self.build_struct_field(&segment, code))
            .collect();

        cursor = node.walk();
        let methods: Vec<DefId> = node
            .children_by_field_name("method", &mut cursor)
            .filter(|n| n.kind() == "function_definition")
            .map(|segment| self.build_function_definition(&segment, code))
            .collect();

        self.arena.defs.alloc(DefData {
            location,
            kind: Def::Struct {
                name,
                vis: Self::get_visibility(node),
                fields,
                methods,
            },
        })
    }

    fn build_struct_field(&mut self, node: &Node, code: &[u8]) -> Field {
        self.collect_errors(node, code);
        let ty = self.build_type(&node.child_by_field_name("type").unwrap(), code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
        Field { name, ty }
    }

    /// Builds a `const` definition. The initializer is routed through
    /// `build_expression` so that const RHS accepts the same grammar as `let`
    /// bindings: struct literals, array literals, identifier copies, function
    /// calls, etc. Semantic restrictions on what a const may actually contain
    /// are enforced by later passes (type checker and analysis rules), not
    /// here.
    fn build_constant_definition(&mut self, node: &Node, code: &[u8]) -> DefId {
        self.collect_errors(node, code);
        let location = Self::get_location(node, code);
        let ty = self.build_type(&node.child_by_field_name("type").unwrap(), code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
        let value = self.build_expression(&node.child_by_field_name("value").unwrap(), code);

        self.arena.defs.alloc(DefData {
            location,
            kind: Def::Constant {
                name,
                vis: Self::get_visibility(node),
                ty,
                value,
            },
        })
    }

    fn build_function_definition(&mut self, node: &Node, code: &[u8]) -> DefId {
        self.collect_errors(node, code);
        let location = Self::get_location(node, code);
        let mut args = Vec::new();
        let mut returns = None;
        let mut type_params = Vec::new();

        if let Some(argument_list_node) = node.child_by_field_name("argument_list") {
            let mut cursor = argument_list_node.walk();
            args = argument_list_node
                .children_by_field_name("argument", &mut cursor)
                .map(|segment| self.build_argument_data(&segment, code))
                .collect();
        }

        if let Some(type_params_node) = node.child_by_field_name("type_parameters") {
            let mut cursor = type_params_node.walk();
            type_params = type_params_node
                .children_by_field_name("type", &mut cursor)
                .map(|segment| self.build_identifier(&segment, code))
                .collect();
        }

        if let Some(returns_node) = node.child_by_field_name("returns") {
            returns = Some(self.build_type(&returns_node, code));
        }

        let Some(name_node) = node.child_by_field_name("name") else {
            self.errors.push(anyhow::anyhow!(
                "Missing function name at {}",
                Self::get_location(node, code)
            ));
            let placeholder_name = self.arena.idents.alloc(Ident {
                location,
                name: "<error>".to_string(),
            });
            let placeholder_body = self.arena.blocks.alloc(BlockData {
                location,
                block_kind: BlockKind::Regular,
                stmts: vec![],
            });
            return self.arena.defs.alloc(DefData {
                location,
                kind: Def::Function {
                    name: placeholder_name,
                    vis: Visibility::default(),
                    type_params: vec![],
                    args: vec![],
                    returns: None,
                    body: placeholder_body,
                },
            });
        };

        let name = self.build_identifier(&name_node, code);
        let body = if let Some(body_node) = node.child_by_field_name("body") {
            self.build_block(&body_node, code)
        } else {
            self.errors.push(anyhow::anyhow!(
                "Missing function body at {}",
                Self::get_location(node, code)
            ));
            self.arena.blocks.alloc(BlockData {
                location,
                block_kind: BlockKind::Regular,
                stmts: vec![],
            })
        };

        self.arena.defs.alloc(DefData {
            location,
            kind: Def::Function {
                name,
                vis: Self::get_visibility(node),
                type_params,
                args,
                returns,
                body,
            },
        })
    }

    fn build_external_function_definition(&mut self, node: &Node, code: &[u8]) -> DefId {
        self.collect_errors(node, code);
        let location = Self::get_location(node, code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
        let mut returns = None;

        let args: Vec<ArgData> =
            if let Some(argument_list_node) = node.child_by_field_name("argument_list") {
                let mut cursor = argument_list_node.walk();
                argument_list_node
                    .children_by_field_name("argument", &mut cursor)
                    .map(|segment| self.build_argument_data(&segment, code))
                    .collect()
            } else {
                let mut cursor = node.walk();
                node.children_by_field_name("argument", &mut cursor)
                    .map(|segment| self.build_argument_data(&segment, code))
                    .collect()
            };

        if let Some(returns_node) = node.child_by_field_name("returns") {
            returns = Some(self.build_type(&returns_node, code));
        }

        self.arena.defs.alloc(DefData {
            location,
            kind: Def::ExternFunction {
                name,
                vis: Visibility::default(),
                args,
                returns,
            },
        })
    }

    fn build_type_alias_definition(&mut self, node: &Node, code: &[u8]) -> DefId {
        self.collect_errors(node, code);
        let location = Self::get_location(node, code);
        let ty = self.build_type(&node.child_by_field_name("type").unwrap(), code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);

        self.arena.defs.alloc(DefData {
            location,
            kind: Def::TypeAlias {
                name,
                vis: Self::get_visibility(node),
                ty,
            },
        })
    }

    /// Module definitions are not yet supported in the grammar.
    #[allow(dead_code)]
    fn build_module_definition(&mut self, _node: &Node, _code: &[u8]) -> DefId {
        unimplemented!("Module definitions are not yet supported in the grammar")
    }

    fn build_argument_data(&mut self, node: &Node, code: &[u8]) -> ArgData {
        self.collect_errors(node, code);
        let location = Self::get_location(node, code);
        match node.kind() {
            "argument_declaration" => {
                let name_node = node.child_by_field_name("name").unwrap();
                let type_node = node.child_by_field_name("type").unwrap();
                let ty = self.build_type(&type_node, code);
                let is_mut = node.child_by_field_name("mut").is_some();
                let name = self.build_identifier(&name_node, code);
                ArgData {
                    location,
                    kind: ArgKind::Named { name, ty, is_mut },
                }
            }
            "self_reference" => {
                let is_mut = node.child_by_field_name("mut").is_some();
                ArgData {
                    location,
                    kind: ArgKind::SelfRef { is_mut },
                }
            }
            "ignore_argument" => {
                let ty = self.build_type(&node.child_by_field_name("type").unwrap(), code);
                ArgData {
                    location,
                    kind: ArgKind::Ignored { ty },
                }
            }
            _ => {
                let ty = self.build_type(node, code);
                ArgData {
                    location,
                    kind: ArgKind::TypeOnly(ty),
                }
            }
        }
    }

    fn build_block(&mut self, node: &Node, code: &[u8]) -> BlockId {
        self.collect_errors(node, code);
        let location = Self::get_location(node, code);
        match node.kind() {
            "assume_block" => {
                let stmts = node
                    .child_by_field_name("body")
                    .map(|body_node| self.build_block_statements(&body_node, code))
                    .unwrap_or_default();
                self.arena.blocks.alloc(BlockData {
                    location,
                    block_kind: BlockKind::Assume,
                    stmts,
                })
            }
            "forall_block" => {
                let stmts = node
                    .child_by_field_name("body")
                    .map(|body_node| self.build_block_statements(&body_node, code))
                    .unwrap_or_default();
                self.arena.blocks.alloc(BlockData {
                    location,
                    block_kind: BlockKind::Forall,
                    stmts,
                })
            }
            "exists_block" => {
                let stmts = node
                    .child_by_field_name("body")
                    .map(|body_node| self.build_block_statements(&body_node, code))
                    .unwrap_or_default();
                self.arena.blocks.alloc(BlockData {
                    location,
                    block_kind: BlockKind::Exists,
                    stmts,
                })
            }
            "unique_block" => {
                let stmts = node
                    .child_by_field_name("body")
                    .map(|body_node| self.build_block_statements(&body_node, code))
                    .unwrap_or_default();
                self.arena.blocks.alloc(BlockData {
                    location,
                    block_kind: BlockKind::Unique,
                    stmts,
                })
            }
            "block" => {
                let stmts = self.build_block_statements(node, code);
                self.arena.blocks.alloc(BlockData {
                    location,
                    block_kind: BlockKind::Regular,
                    stmts,
                })
            }
            "ERROR" => {
                cov_mark::hit!(ast_builder_error_block_recovery);
                self.errors.push(anyhow::anyhow!(
                    "Syntax error in block at {}",
                    Self::get_location(node, code)
                ));
                self.create_error_block(node, code)
            }
            _ => {
                self.errors.push(anyhow::anyhow!(
                    "Unexpected block type '{}' at {}",
                    node.kind(),
                    Self::get_location(node, code)
                ));
                self.create_error_block(node, code)
            }
        }
    }

    fn create_error_block(&mut self, node: &Node, code: &[u8]) -> BlockId {
        let location = Self::get_location(node, code);
        self.arena.blocks.alloc(BlockData {
            location,
            block_kind: BlockKind::Regular,
            stmts: vec![],
        })
    }

    fn build_block_statements(&mut self, node: &Node, code: &[u8]) -> Vec<StmtId> {
        let mut stmts = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            self.collect_errors(&child, code);

            if child.is_named() {
                let stmt_id = self.build_statement(&child, code);
                stmts.push(stmt_id);
            }
        }
        stmts
    }

    #[allow(clippy::too_many_lines)]
    fn build_statement(&mut self, node: &Node, code: &[u8]) -> StmtId {
        let location = Self::get_location(node, code);
        match node.kind() {
            "assign_statement" => {
                let left = self.build_expression(&node.child_by_field_name("left").unwrap(), code);
                let right =
                    self.build_expression(&node.child_by_field_name("right").unwrap(), code);
                self.arena.stmts.alloc(StmtData {
                    location,
                    kind: Stmt::Assign { left, right },
                })
            }
            "block" | "forall_block" | "assume_block" | "exists_block" | "unique_block" => {
                let block_id = self.build_block(node, code);
                self.arena.stmts.alloc(StmtData {
                    location,
                    kind: Stmt::Block(block_id),
                })
            }
            "expression_statement" => {
                if let Some(expr_node) = node.child(0) {
                    let expr_id = self.build_expression(&expr_node, code);
                    self.arena.stmts.alloc(StmtData {
                        location,
                        kind: Stmt::Expr(expr_id),
                    })
                } else {
                    self.create_error_statement(node, code)
                }
            }
            "return_statement" => {
                let expr_id = if let Some(expr_node) = node.child_by_field_name("expression") {
                    self.build_expression(&expr_node, code)
                } else {
                    self.arena.exprs.alloc(ExprData {
                        location,
                        kind: Expr::UnitLiteral,
                    })
                };
                self.arena.stmts.alloc(StmtData {
                    location,
                    kind: Stmt::Return { expr: expr_id },
                })
            }
            "loop_statement" => {
                let condition = node
                    .child_by_field_name("condition")
                    .map(|n| self.build_expression(&n, code));
                let body = if let Some(body_block) = node.child_by_field_name("body") {
                    self.build_block(&body_block, code)
                } else {
                    self.errors.push(anyhow::anyhow!(
                        "Missing loop body at {}",
                        Self::get_location(node, code)
                    ));
                    self.arena.blocks.alloc(BlockData {
                        location,
                        block_kind: BlockKind::Regular,
                        stmts: vec![],
                    })
                };
                self.arena.stmts.alloc(StmtData {
                    location,
                    kind: Stmt::Loop { condition, body },
                })
            }
            "if_statement" => {
                let condition = if let Some(condition_node) = node.child_by_field_name("condition")
                {
                    self.build_expression(&condition_node, code)
                } else {
                    self.errors.push(anyhow::anyhow!(
                        "Missing if condition at {}",
                        Self::get_location(node, code)
                    ));
                    self.create_error_expr(node, code)
                };
                let then_block = if let Some(if_arm_node) = node.child_by_field_name("if_arm") {
                    self.build_block(&if_arm_node, code)
                } else {
                    self.errors.push(anyhow::anyhow!(
                        "Missing if body at {}",
                        Self::get_location(node, code)
                    ));
                    self.arena.blocks.alloc(BlockData {
                        location,
                        block_kind: BlockKind::Regular,
                        stmts: vec![],
                    })
                };
                let else_block = node
                    .child_by_field_name("else_arm")
                    .map(|n| self.build_block(&n, code));
                self.arena.stmts.alloc(StmtData {
                    location,
                    kind: Stmt::If {
                        condition,
                        then_block,
                        else_block,
                    },
                })
            }
            "variable_definition_statement" => {
                let ty = self.build_type(&node.child_by_field_name("type").unwrap(), code);
                let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
                let is_mut = node.child_by_field_name("mut").is_some();
                let value = node
                    .child_by_field_name("value")
                    .map(|n| self.build_expression(&n, code));

                self.arena.stmts.alloc(StmtData {
                    location,
                    kind: Stmt::VarDef {
                        name,
                        ty,
                        value,
                        is_mut,
                    },
                })
            }
            "type_definition_statement" => {
                let ty = self.build_type(&node.child_by_field_name("type").unwrap(), code);
                let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
                self.arena.stmts.alloc(StmtData {
                    location,
                    kind: Stmt::TypeDef { name, ty },
                })
            }
            "assert_statement" => {
                let expr_id = self.build_expression(&node.child(1).unwrap(), code);
                self.arena.stmts.alloc(StmtData {
                    location,
                    kind: Stmt::Assert { expr: expr_id },
                })
            }
            "break_statement" => self.arena.stmts.alloc(StmtData {
                location,
                kind: Stmt::Break,
            }),
            "constant_definition" => {
                let def_id = self.build_constant_definition(node, code);
                self.arena.stmts.alloc(StmtData {
                    location,
                    kind: Stmt::ConstDef(def_id),
                })
            }
            "ERROR" => {
                cov_mark::hit!(ast_builder_error_statement_recovery);
                self.errors.push(anyhow::anyhow!(
                    "Syntax error in statement at {}",
                    Self::get_location(node, code)
                ));
                self.create_error_statement(node, code)
            }
            _ => {
                self.errors.push(anyhow::anyhow!(
                    "Unexpected statement type '{}' at {}",
                    node.kind(),
                    Self::get_location(node, code)
                ));
                self.create_error_statement(node, code)
            }
        }
    }

    fn create_error_statement(&mut self, node: &Node, code: &[u8]) -> StmtId {
        let location = Self::get_location(node, code);
        let error_expr = self.create_error_expr(node, code);
        self.arena.stmts.alloc(StmtData {
            location,
            kind: Stmt::Expr(error_expr),
        })
    }

    fn create_error_expr(&mut self, node: &Node, code: &[u8]) -> ExprId {
        let location = Self::get_location(node, code);
        let error_ident = self.arena.idents.alloc(Ident {
            location,
            name: "<error>".to_string(),
        });
        self.arena.exprs.alloc(ExprData {
            location,
            kind: Expr::Identifier(error_ident),
        })
    }

    fn build_expression(&mut self, node: &Node, code: &[u8]) -> ExprId {
        let location = Self::get_location(node, code);
        let node_kind = node.kind();
        match node_kind {
            "array_index_access_expression" => {
                self.collect_errors(node, code);
                let array = self.build_expression(&node.named_child(0).unwrap(), code);
                let index = self.build_expression(&node.named_child(1).unwrap(), code);
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::ArrayIndexAccess { array, index },
                })
            }
            "generic_name" | "qualified_name" | "type" => {
                let type_id = self.build_type(node, code);
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::Type(type_id),
                })
            }
            "member_access_expression" => {
                self.collect_errors(node, code);
                let expr =
                    self.build_expression(&node.child_by_field_name("expression").unwrap(), code);
                let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::MemberAccess { expr, name },
                })
            }
            "type_member_access_expression" => {
                self.collect_errors(node, code);
                let expr =
                    self.build_expression(&node.child_by_field_name("expression").unwrap(), code);
                let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::TypeMemberAccess { expr, name },
                })
            }
            "function_call_expression" => self.build_function_call_expression(node, code),
            "struct_expression" => self.build_struct_expression(node, code),
            "prefix_unary_expression" => {
                self.collect_errors(node, code);
                let inner = self.build_expression(&node.child(1).unwrap(), code);
                let operator_node = node.child_by_field_name("operator").unwrap();
                let op = match operator_node.kind() {
                    "unary_not" => UnaryOperatorKind::Not,
                    "unary_minus" => UnaryOperatorKind::Neg,
                    "unary_bitnot" => UnaryOperatorKind::BitNot,
                    other => unreachable!("Unexpected unary operator node: {other}"),
                };
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::PrefixUnary { expr: inner, op },
                })
            }
            "parenthesized_expression" => {
                self.collect_errors(node, code);
                let inner = self.build_expression(&node.child(1).unwrap(), code);
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::Parenthesized { expr: inner },
                })
            }
            "binary_expression" => self.build_binary_expression(node, code),
            "bool_literal" | "string_literal" | "number_literal" | "array_literal"
            | "unit_literal" => self.build_literal(node, code),
            "uzumaki_keyword" => {
                self.collect_errors(node, code);
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::Uzumaki,
                })
            }
            "identifier" => {
                let ident_id = self.build_identifier(node, code);
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::Identifier(ident_id),
                })
            }
            "ERROR" => {
                cov_mark::hit!(ast_builder_error_expression_recovery);
                self.errors.push(anyhow::anyhow!(
                    "Syntax error in expression at {}",
                    Self::get_location(node, code)
                ));
                self.create_error_expr(node, code)
            }
            _ => {
                self.errors.push(anyhow::anyhow!(
                    "Unexpected expression node kind '{}' at {}",
                    node_kind,
                    Self::get_location(node, code)
                ));
                self.create_error_expr(node, code)
            }
        }
    }

    fn build_function_call_expression(&mut self, node: &Node, code: &[u8]) -> ExprId {
        self.collect_errors(node, code);
        let location = Self::get_location(node, code);
        let function = self.build_expression(&node.child_by_field_name("function").unwrap(), code);
        let mut args: Vec<(Option<IdentId>, ExprId)> = Vec::new();
        let mut type_params = Vec::new();
        let mut pending_name: Option<IdentId> = None;
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if let Some(field) = cursor.field_name() {
                    match field {
                        "argument_name" => {
                            let expr_id = self.build_expression(&child, code);
                            if let Expr::Identifier(ident_id) = self.arena[expr_id].kind {
                                pending_name = Some(ident_id);
                            }
                        }
                        "argument" => {
                            let expr_id = self.build_expression(&child, code);
                            let name = pending_name.take();
                            args.push((name, expr_id));
                        }
                        _ => {}
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        if let Some(type_parameters_node) = node.child_by_field_name("type_parameters") {
            let mut cursor = type_parameters_node.walk();
            type_params = type_parameters_node
                .children_by_field_name("type", &mut cursor)
                .map(|segment| self.build_identifier(&segment, code))
                .collect();
        }

        self.arena.exprs.alloc(ExprData {
            location,
            kind: Expr::FunctionCall {
                function,
                type_params,
                args,
            },
        })
    }

    fn build_struct_expression(&mut self, node: &Node, code: &[u8]) -> ExprId {
        self.collect_errors(node, code);
        let location = Self::get_location(node, code);
        let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
        let mut fields: Vec<(IdentId, ExprId)> = Vec::new();
        let mut pending_name: Option<IdentId> = None;
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if let Some(field) = cursor.field_name() {
                    match field {
                        "field_name" => {
                            let expr_id = self.build_expression(&child, code);
                            if let Expr::Identifier(ident_id) = self.arena[expr_id].kind {
                                pending_name = Some(ident_id);
                            }
                        }
                        "field_value" => {
                            let expr_id = self.build_expression(&child, code);
                            let field_name = pending_name
                                .take()
                                .expect("pending_name is not initialized");
                            fields.push((field_name, expr_id));
                        }
                        _ => {}
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        self.arena.exprs.alloc(ExprData {
            location,
            kind: Expr::StructLiteral { name, fields },
        })
    }

    fn build_binary_expression(&mut self, node: &Node, code: &[u8]) -> ExprId {
        self.collect_errors(node, code);
        let location = Self::get_location(node, code);
        let left = self.build_expression(&node.child_by_field_name("left").unwrap(), code);
        let operator_node = node.child_by_field_name("operator").unwrap();
        let operator_kind = operator_node.kind();
        let op = match operator_kind {
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
        let right = self.build_expression(&node.child_by_field_name("right").unwrap(), code);

        self.arena.exprs.alloc(ExprData {
            location,
            kind: Expr::Binary { left, right, op },
        })
    }

    fn build_literal(&mut self, node: &Node, code: &[u8]) -> ExprId {
        let location = Self::get_location(node, code);
        match node.kind() {
            "array_literal" => {
                self.collect_errors(node, code);
                let mut elements = Vec::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    elements.push(self.build_expression(&child, code));
                }
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::ArrayLiteral { elements },
                })
            }
            "bool_literal" => {
                self.collect_errors(node, code);
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
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::BoolLiteral { value },
                })
            }
            "string_literal" => {
                self.collect_errors(node, code);
                let value = node.utf8_text(code).unwrap().to_string();
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::StringLiteral { value },
                })
            }
            "number_literal" => {
                self.collect_errors(node, code);
                let value = node.utf8_text(code).unwrap().to_string();
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::NumberLiteral { value },
                })
            }
            "unit_literal" => {
                self.collect_errors(node, code);
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::UnitLiteral,
                })
            }
            "type_member_access_expression" => self.build_expression(node, code),
            _ => {
                self.errors.push(anyhow::anyhow!(
                    "Unexpected literal type '{}' at {}",
                    node.kind(),
                    Self::get_location(node, code)
                ));
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::UnitLiteral,
                })
            }
        }
    }

    /// Extracts just the string value from a string literal node (used for `from` in use directives).
    fn build_string_literal_value(&mut self, node: &Node, code: &[u8]) -> String {
        self.collect_errors(node, code);
        node.utf8_text(code).unwrap().to_string()
    }

    #[allow(clippy::too_many_lines)]
    fn build_type(&mut self, node: &Node, code: &[u8]) -> TypeId {
        let location = Self::get_location(node, code);
        let node_kind = node.kind();
        match node_kind {
            "type_unit" => self.arena.types.alloc(TypeData {
                location,
                kind: TypeNode::Simple(SimpleTypeKind::Unit),
            }),
            "type_bool" => self.arena.types.alloc(TypeData {
                location,
                kind: TypeNode::Simple(SimpleTypeKind::Bool),
            }),
            "type_i8" => self.arena.types.alloc(TypeData {
                location,
                kind: TypeNode::Simple(SimpleTypeKind::I8),
            }),
            "type_i16" => self.arena.types.alloc(TypeData {
                location,
                kind: TypeNode::Simple(SimpleTypeKind::I16),
            }),
            "type_i32" => self.arena.types.alloc(TypeData {
                location,
                kind: TypeNode::Simple(SimpleTypeKind::I32),
            }),
            "type_i64" => self.arena.types.alloc(TypeData {
                location,
                kind: TypeNode::Simple(SimpleTypeKind::I64),
            }),
            "type_u8" => self.arena.types.alloc(TypeData {
                location,
                kind: TypeNode::Simple(SimpleTypeKind::U8),
            }),
            "type_u16" => self.arena.types.alloc(TypeData {
                location,
                kind: TypeNode::Simple(SimpleTypeKind::U16),
            }),
            "type_u32" => self.arena.types.alloc(TypeData {
                location,
                kind: TypeNode::Simple(SimpleTypeKind::U32),
            }),
            "type_u64" => self.arena.types.alloc(TypeData {
                location,
                kind: TypeNode::Simple(SimpleTypeKind::U64),
            }),
            "type_array" => {
                self.collect_errors(node, code);
                let element = self.build_type(&node.child_by_field_name("type").unwrap(), code);
                let length_node = node.child_by_field_name("length").unwrap();
                let size = self.build_expression(&length_node, code);
                self.arena.types.alloc(TypeData {
                    location,
                    kind: TypeNode::Array { element, size },
                })
            }
            "generic_type" | "generic_name" => {
                self.collect_errors(node, code);
                let base =
                    self.build_identifier(&node.child_by_field_name("base_type").unwrap(), code);
                let args = node.child(1).unwrap();
                let mut cursor = args.walk();
                let params: Vec<IdentId> = args
                    .children_by_field_name("type", &mut cursor)
                    .map(|segment| self.build_identifier(&segment, code))
                    .collect();
                self.arena.types.alloc(TypeData {
                    location,
                    kind: TypeNode::Generic { base, params },
                })
            }
            "type_qualified_name" => {
                self.collect_errors(node, code);
                let alias =
                    self.build_identifier(&node.child_by_field_name("alias").unwrap(), code);
                let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
                self.arena.types.alloc(TypeData {
                    location,
                    kind: TypeNode::Qualified { alias, name },
                })
            }
            "qualified_name" => {
                self.collect_errors(node, code);
                let qualifier =
                    self.build_identifier(&node.child_by_field_name("qualifier").unwrap(), code);
                let name = self.build_identifier(&node.child_by_field_name("name").unwrap(), code);
                self.arena.types.alloc(TypeData {
                    location,
                    kind: TypeNode::QualifiedName { qualifier, name },
                })
            }
            "type_fn" => {
                self.collect_errors(node, code);
                let mut cursor = node.walk();
                let params: Vec<TypeId> = node
                    .children_by_field_name("argument", &mut cursor)
                    .map(|segment| self.build_type(&segment, code))
                    .collect();
                let ret = node
                    .child_by_field_name("returns")
                    .map(|returns_type_node| self.build_type(&returns_type_node, code));
                self.arena.types.alloc(TypeData {
                    location,
                    kind: TypeNode::Function { params, ret },
                })
            }
            "identifier" => {
                let ident_id = self.build_identifier(node, code);
                self.arena.types.alloc(TypeData {
                    location,
                    kind: TypeNode::Custom(ident_id),
                })
            }
            "ERROR" => {
                cov_mark::hit!(ast_builder_error_type_recovery);
                self.errors.push(anyhow::anyhow!(
                    "Syntax error in type at {}",
                    Self::get_location(node, code)
                ));
                self.arena.types.alloc(TypeData {
                    location,
                    kind: TypeNode::Simple(SimpleTypeKind::Unit),
                })
            }
            _ => {
                self.errors.push(anyhow::anyhow!(
                    "Unexpected type '{}' at {}",
                    node_kind,
                    Self::get_location(node, code)
                ));
                self.arena.types.alloc(TypeData {
                    location,
                    kind: TypeNode::Simple(SimpleTypeKind::Unit),
                })
            }
        }
    }

    fn build_identifier(&mut self, node: &Node, code: &[u8]) -> IdentId {
        self.collect_errors(node, code);
        let location = Self::get_location(node, code);
        let name = node.utf8_text(code).unwrap().to_string();
        self.arena.idents.alloc(Ident { location, name })
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

    fn get_visibility(node: &Node) -> Visibility {
        node.child_by_field_name("visibility")
            .map(|_| Visibility::Public)
            .unwrap_or_default()
    }
}
