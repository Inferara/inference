//! Lowering from the owned CST to an [`AstArena`] (issue #62, Phase 5).
//!
//! This is a near-mechanical 1:1 port of `inference_ast::builder::Builder`, the
//! legacy tree-sitter CST→AST lowering. The parity law (design §0) requires the
//! produced `AstArena` to be **byte-identical** to the one `Builder` produces, so
//! every method here mirrors a `build_*` method in `builder.rs`: same
//! decomposition, same arena `alloc` call **order** (arena IDs are sequential
//! `la_arena` indices, so order *is* identity) and the same [`Location`].
//!
//! Where `builder.rs` navigates the tree-sitter CST by field name or named-child
//! index, this navigates our owned CST ([`SyntaxNode`]) by node kind and
//! position (design §7). The CST mirrors grammar.js rule names, hidden `_rules`
//! are inlined, and node [`Location`]s already carry tree-sitter's byte span and
//! 1-based line/byte-column (Phase 1/3), so locations come straight from the
//! node's own `loc`.
//!
//! For valid inputs there are no error/placeholder nodes; the `lower_error_*`
//! paths exist only to keep the dispatch total and the lowering panic-free, which
//! the never-panic guarantee depends on (the oracle compares only valid sources).

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, ExprId, IdentId, StmtId, TypeId};
use inference_ast::nodes::{
    ArgData, ArgKind, BlockData, BlockKind, Def, DefData, Directive, Expr, ExprData, Field, Ident,
    Location, OperatorKind, SimpleTypeKind, SourceFileData, Stmt, StmtData, TypeData, TypeNode,
    UnaryOperatorKind, UseDirective, Visibility,
};

use crate::errors::ParseError;
use crate::syntax_kind::SyntaxKind;
use crate::syntax_tree::SyntaxNode;

/// Lowers an owned CST into an [`AstArena`], mirroring `builder.rs` allocation
/// order so the arena is byte-identical to the legacy tree-sitter path.
pub(crate) struct Lowering<'s> {
    arena: AstArena,
    errors: Vec<ParseError>,
    src: &'s str,
}

impl<'s> Lowering<'s> {
    /// Creates a lowering over `src`, the original source string (needed to slice
    /// identifier/literal text and to store as `SourceFileData.source`).
    pub(crate) fn new(src: &'s str) -> Self {
        Self {
            arena: AstArena::default(),
            errors: Vec::new(),
            src,
        }
    }

    /// Lowers `root` (a `SourceFile` node) and returns the arena plus any errors
    /// the lowering itself collected (malformed dispatch only — valid sources
    /// produce none).
    pub(crate) fn lower(mut self, root: &SyntaxNode) -> (AstArena, Vec<ParseError>) {
        self.lower_source_file(root);
        (self.arena, self.errors)
    }

    /// Mirrors `Builder::build_ast`: iterate the root's children, routing
    /// `use_directive`s to directives and everything else to `lower_definition`,
    /// then alloc the `SourceFileData` **last** (after all defs/directives).
    fn lower_source_file(&mut self, root: &SyntaxNode) {
        let location = self.whole_source_location();
        let source = self.src.to_string();

        let mut defs = Vec::new();
        let mut directives = Vec::new();

        for child in root.node_children() {
            match child.kind {
                SyntaxKind::UseDirective => {
                    directives.push(Directive::Use(self.lower_use_directive(child)));
                }
                _ => {
                    let def_id = self.lower_definition(child);
                    defs.push(def_id);
                }
            }
        }

        self.arena.source_files.alloc(SourceFileData {
            location,
            source,
            defs,
            directives,
        });
    }

    /// Mirrors `Builder::build_use_directive`. The two grammar forms are
    /// distinguished by a `from` string literal:
    /// - braced form `use { types } from "lit";`: `from` holds the literal text,
    ///   `segments` is empty, `imported_types` are the braced identifiers;
    /// - path form `use a::b::{ types };`: `from` is `None`, `segments` are the
    ///   `::`-separated identifiers before the brace list, `imported_types` are
    ///   the braced identifiers (or empty when there is no brace list).
    ///
    /// The two identifier roles are tree-sitter's `segment` vs `imported_type`
    /// fields; our CST tags neither, so we split on the `{` that opens the
    /// imported-type list: identifiers before it are segments, those after it are
    /// imported types. Alloc order matches `builder.rs`: for the `from` form only
    /// imported types are allocated; for the path form the segments are allocated
    /// first, then the imported types.
    fn lower_use_directive(&mut self, node: &SyntaxNode) -> UseDirective {
        use crate::syntax_tree::SyntaxElement;
        let location = node.loc;

        let from = node
            .child(SyntaxKind::StringLiteral)
            .map(|from_literal| self.lower_string_literal_value(from_literal));

        let mut before_brace: Vec<&SyntaxNode> = Vec::new();
        let mut after_brace: Vec<&SyntaxNode> = Vec::new();
        let mut in_braces = false;
        for element in &node.children {
            match element {
                SyntaxElement::Token(t) if t.kind == SyntaxKind::LBrace => in_braces = true,
                SyntaxElement::Node(n) if n.kind == SyntaxKind::Identifier => {
                    if in_braces {
                        after_brace.push(n);
                    } else {
                        before_brace.push(n);
                    }
                }
                _ => {}
            }
        }

        let segments: Vec<IdentId> = if from.is_some() {
            Vec::new()
        } else {
            before_brace
                .into_iter()
                .map(|segment| self.lower_identifier(segment))
                .collect()
        };
        let imported_types: Vec<IdentId> = after_brace
            .into_iter()
            .map(|imported_type| self.lower_identifier(imported_type))
            .collect();

        UseDirective {
            location,
            imported_types,
            segments,
            from,
        }
    }

    /// Mirrors `Builder::build_definition`: dispatch on node kind.
    fn lower_definition(&mut self, node: &SyntaxNode) -> DefId {
        match node.kind {
            SyntaxKind::SpecDefinition => self.lower_spec_definition(node),
            SyntaxKind::StructDefinition => self.lower_struct_definition(node),
            SyntaxKind::EnumDefinition => self.lower_enum_definition(node),
            SyntaxKind::ConstantDefinition => self.lower_constant_definition(node),
            SyntaxKind::FunctionDefinition => self.lower_function_definition(node),
            SyntaxKind::ExternalFunctionDefinition => self.lower_external_function_definition(node),
            SyntaxKind::TypeDefinitionStatement => self.lower_type_alias_definition(node),
            _ => {
                self.push_error(
                    node,
                    format!("Unexpected definition kind '{:?}'", node.kind),
                );
                self.create_error_definition(node)
            }
        }
    }

    /// Mirrors `Builder::create_error_definition`: an `<error>` identifier and an
    /// empty regular block under a placeholder private function.
    fn create_error_definition(&mut self, node: &SyntaxNode) -> DefId {
        let location = node.loc;
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

    /// Mirrors `Builder::build_spec_definition`: the name identifier first, then
    /// each nested definition, then the `DefData`. Spec visibility is hardcoded
    /// to the default (`Private`), matching `builder.rs`.
    fn lower_spec_definition(&mut self, node: &SyntaxNode) -> DefId {
        let location = node.loc;
        let name = self.lower_name_or_error(self.first_identifier(node), node);
        let mut defs = Vec::new();

        // The name is the first named child; nested definitions follow it
        // (mirrors `builder.rs`'s `named_child(1..)`).
        for child in node.node_children().skip(1) {
            let def_id = self.lower_definition(child);
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

    /// Mirrors `Builder::build_enum_definition`: name identifier first, then the
    /// variant identifiers, then the `DefData`. The name is the first
    /// `Identifier` child; the remaining `Identifier` children are variants.
    fn lower_enum_definition(&mut self, node: &SyntaxNode) -> DefId {
        let location = node.loc;
        let mut idents = node.children_of(SyntaxKind::Identifier);
        // The name is the first `Identifier` child (parsed via `types::identifier`,
        // which always completes a node), and the rest are variants. A nameless
        // `enum {` still emits a zero-width name node, but fall back to `<error>`
        // for totality should one ever be absent (design §8).
        let name_node = idents.next();
        let variant_nodes: Vec<&SyntaxNode> = idents.collect();
        let name = self.lower_name_or_error(name_node, node);
        let variants: Vec<IdentId> = variant_nodes
            .into_iter()
            .map(|variant| self.lower_identifier(variant))
            .collect();

        self.arena.defs.alloc(DefData {
            location,
            kind: Def::Enum {
                name,
                vis: self.visibility(node),
                variants,
            },
        })
    }

    /// Mirrors `Builder::build_struct_definition`: name identifier, then each
    /// field (`type` then `name`, see `lower_struct_field`), then each method
    /// (a `function_definition`), then the `DefData`.
    fn lower_struct_definition(&mut self, node: &SyntaxNode) -> DefId {
        let location = node.loc;
        let name = self.lower_name_or_error(self.first_identifier(node), node);

        let field_nodes: Vec<&SyntaxNode> = node.children_of(SyntaxKind::StructField).collect();
        let fields: Vec<Field> = field_nodes
            .into_iter()
            .map(|field| self.lower_struct_field(field))
            .collect();

        let method_nodes: Vec<&SyntaxNode> =
            node.children_of(SyntaxKind::FunctionDefinition).collect();
        let methods: Vec<DefId> = method_nodes
            .into_iter()
            .map(|method| self.lower_function_definition(method))
            .collect();

        self.arena.defs.alloc(DefData {
            location,
            kind: Def::Struct {
                name,
                vis: self.visibility(node),
                fields,
                methods,
            },
        })
    }

    /// Mirrors `Builder::build_struct_field`: **type then name**. The field's CST
    /// is `Identifier : Type`, so the name is the `Identifier` and the type is the
    /// remaining (type) node.
    fn lower_struct_field(&mut self, node: &SyntaxNode) -> Field {
        let name_ident = self.first_identifier(node);
        // The type follows `:`; a malformed field (`a: ;` / `a:`) has no type
        // node because the grammar's `type_(p)` error path completes nothing, so
        // fall back to a `Unit` placeholder (design §8).
        let ty = self.lower_type_after_colon_or_unit(node, node.loc);
        let name = self.lower_name_or_error(name_ident, node);
        Field { name, ty }
    }

    /// Mirrors `Builder::build_constant_definition`: **type, then name, then
    /// value**. The CST holds an `Identifier` (name), a type node, and a value
    /// expression. On a malformed (error-recovery) const that is missing its type
    /// or value, a unit placeholder is synthesized so lowering stays total.
    fn lower_constant_definition(&mut self, node: &SyntaxNode) -> DefId {
        let location = node.loc;
        let ty = self.lower_type_after_colon_or_unit(node, location);
        let name = self.lower_name_or_error(self.first_identifier(node), node);
        let value = self.lower_value_after_eq_or_unit(node, location);

        self.arena.defs.alloc(DefData {
            location,
            kind: Def::Constant {
                name,
                vis: self.visibility(node),
                ty,
                value,
            },
        })
    }

    /// Mirrors `Builder::build_function_definition` exactly. Alloc order: each
    /// argument (each `argument_declaration`: **type then name**), then the
    /// type-parameter identifiers, then the `returns` type, then the function
    /// `name` identifier, then the body block, and finally the `DefData`.
    fn lower_function_definition(&mut self, node: &SyntaxNode) -> DefId {
        let location = node.loc;
        let mut args = Vec::new();
        let mut returns = None;
        let mut type_params = Vec::new();

        if let Some(argument_list_node) = node.child(SyntaxKind::ArgumentList) {
            let arg_nodes: Vec<&SyntaxNode> = argument_list_node.node_children().collect();
            args = arg_nodes
                .into_iter()
                .map(|arg| self.lower_argument_data(arg))
                .collect();
        }

        if let Some(type_params_node) = node.child(SyntaxKind::TypeArgumentListDefinition) {
            let tp_nodes: Vec<&SyntaxNode> = type_params_node
                .children_of(SyntaxKind::Identifier)
                .collect();
            type_params = tp_nodes
                .into_iter()
                .map(|tp| self.lower_identifier(tp))
                .collect();
        }

        if let Some(returns_node) = self.function_return_type(node) {
            returns = Some(self.lower_type(returns_node));
        }

        let name = self.lower_name_or_error(self.first_identifier(node), node);
        let body = if let Some(body_node) = self.function_body(node) {
            self.lower_block(body_node)
        } else {
            self.push_error(node, "Missing function body".to_string());
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
                vis: self.visibility(node),
                type_params,
                args,
                returns,
                body,
            },
        })
    }

    /// Mirrors `Builder::build_external_function_definition`: name identifier
    /// first, then each argument, then the `returns` type, then the `DefData`.
    /// External-function visibility is hardcoded to the default (`Private`).
    fn lower_external_function_definition(&mut self, node: &SyntaxNode) -> DefId {
        let location = node.loc;
        let name = self.lower_name_or_error(self.first_identifier(node), node);
        let mut returns = None;

        let args: Vec<ArgData> =
            if let Some(argument_list_node) = node.child(SyntaxKind::ArgumentList) {
                let arg_nodes: Vec<&SyntaxNode> = argument_list_node.node_children().collect();
                arg_nodes
                    .into_iter()
                    .map(|arg| self.lower_argument_data(arg))
                    .collect()
            } else {
                Vec::new()
            };

        if let Some(returns_node) = self.function_return_type(node) {
            returns = Some(self.lower_type(returns_node));
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

    /// Mirrors `Builder::build_type_alias_definition`: **type then name**, then
    /// the `DefData`. A `type X = T;` aliases the type after `=` (there is no
    /// `:`), so the type is anchored on `=`, not `:`. A malformed alias missing
    /// its type lowers to a unit placeholder so lowering stays total.
    fn lower_type_alias_definition(&mut self, node: &SyntaxNode) -> DefId {
        let location = node.loc;
        let ty = match self.node_after_token(node, SyntaxKind::Eq) {
            Some(type_node) => self.lower_type(type_node),
            None => self.alloc_simple_type(location, SimpleTypeKind::Unit),
        };
        let name = self.lower_name_or_error(self.first_identifier(node), node);

        self.arena.defs.alloc(DefData {
            location,
            kind: Def::TypeAlias {
                name,
                vis: self.visibility(node),
                ty,
            },
        })
    }

    /// Mirrors `Builder::build_argument_data`, dispatching on the argument node
    /// kind. For `argument_declaration` the alloc order is **type then name**.
    fn lower_argument_data(&mut self, node: &SyntaxNode) -> ArgData {
        let location = node.loc;
        match node.kind {
            SyntaxKind::ArgumentDeclaration => {
                let name_node = self.first_identifier(node);
                // The type follows `:`; a malformed argument (`a: )`) has no type
                // node, so fall back to a `Unit` placeholder (design §8).
                let ty = self.lower_type_after_colon_or_unit(node, location);
                let is_mut = node.child(SyntaxKind::MutKeyword).is_some();
                let name = self.lower_name_or_error(name_node, node);
                ArgData {
                    location,
                    kind: ArgKind::Named { name, ty, is_mut },
                }
            }
            SyntaxKind::SelfReference => {
                let is_mut = node.child(SyntaxKind::MutKeyword).is_some();
                ArgData {
                    location,
                    kind: ArgKind::SelfRef { is_mut },
                }
            }
            SyntaxKind::IgnoreArgument => {
                // The type follows `:`; a malformed ignore argument (`_: )`) has
                // no type node, so fall back to a `Unit` placeholder (design §8).
                let ty = self.lower_type_after_colon_or_unit(node, location);
                ArgData {
                    location,
                    kind: ArgKind::Ignored { ty },
                }
            }
            _ => {
                let ty = self.lower_type(node);
                ArgData {
                    location,
                    kind: ArgKind::TypeOnly(ty),
                }
            }
        }
    }

    /// Mirrors `Builder::build_block`, dispatching on block kind. Non-det blocks
    /// (`assume`/`forall`/`exists`/`unique`) wrap a `block`; the inner block's
    /// statements become the wrapper's statements with the matching `BlockKind`.
    fn lower_block(&mut self, node: &SyntaxNode) -> BlockId {
        let location = node.loc;
        match node.kind {
            SyntaxKind::AssumeBlock => self.lower_nondet_block(node, location, BlockKind::Assume),
            SyntaxKind::ForallBlock => self.lower_nondet_block(node, location, BlockKind::Forall),
            SyntaxKind::ExistsBlock => self.lower_nondet_block(node, location, BlockKind::Exists),
            SyntaxKind::UniqueBlock => self.lower_nondet_block(node, location, BlockKind::Unique),
            SyntaxKind::Block => {
                let stmts = self.lower_block_statements(node);
                self.arena.blocks.alloc(BlockData {
                    location,
                    block_kind: BlockKind::Regular,
                    stmts,
                })
            }
            _ => {
                self.push_error(node, format!("Unexpected block type '{:?}'", node.kind));
                self.create_error_block(node)
            }
        }
    }

    /// Lowers a non-det block: its body is the inner `Block` child (matching
    /// tree-sitter's `body` field), defaulting to no statements if absent.
    fn lower_nondet_block(
        &mut self,
        node: &SyntaxNode,
        location: Location,
        block_kind: BlockKind,
    ) -> BlockId {
        let stmts = node
            .child(SyntaxKind::Block)
            .map(|body_node| self.lower_block_statements(body_node))
            .unwrap_or_default();
        self.arena.blocks.alloc(BlockData {
            location,
            block_kind,
            stmts,
        })
    }

    /// Mirrors `Builder::create_error_block`: an empty regular block.
    fn create_error_block(&mut self, node: &SyntaxNode) -> BlockId {
        let location = node.loc;
        self.arena.blocks.alloc(BlockData {
            location,
            block_kind: BlockKind::Regular,
            stmts: vec![],
        })
    }

    /// Mirrors `Builder::build_block_statements`: lower each named child of the
    /// block in source order. (Our CST node-children are exactly tree-sitter's
    /// named children.)
    fn lower_block_statements(&mut self, node: &SyntaxNode) -> Vec<StmtId> {
        let stmt_nodes: Vec<&SyntaxNode> = node.node_children().collect();
        stmt_nodes
            .into_iter()
            .map(|child| self.lower_statement(child))
            .collect()
    }

    /// Mirrors `Builder::build_statement`, dispatching on node kind. Replicates
    /// `builder.rs` alloc order per arm — notably `return;` with no expression
    /// still allocates a `UnitLiteral` expression, and `variable_definition` is
    /// **type, then name, then value**.
    #[allow(clippy::too_many_lines)]
    fn lower_statement(&mut self, node: &SyntaxNode) -> StmtId {
        let location = node.loc;
        match node.kind {
            SyntaxKind::AssignStatement => {
                let mut exprs = node.node_children();
                // guaranteed: `expression_or_assign_statement` is reached only at
                // an expression-start token, so the left-hand expression node is
                // always present.
                let left_node = exprs.next().expect("assign has a left expression");
                let left = self.lower_expression(left_node);
                // The right-hand side follows `=`; an EOF-truncated `x =` leaves
                // no node, so fall back to an `<error>` expression (design §8).
                let right = self.lower_expression_or_error(exprs.next(), node);
                self.arena.stmts.alloc(StmtData {
                    location,
                    kind: Stmt::Assign { left, right },
                })
            }
            SyntaxKind::Block
            | SyntaxKind::ForallBlock
            | SyntaxKind::AssumeBlock
            | SyntaxKind::ExistsBlock
            | SyntaxKind::UniqueBlock => {
                let block_id = self.lower_block(node);
                self.arena.stmts.alloc(StmtData {
                    location,
                    kind: Stmt::Block(block_id),
                })
            }
            SyntaxKind::ExpressionStatement => {
                if let Some(expr_node) = node.nth_node(0) {
                    let expr_id = self.lower_expression(expr_node);
                    self.arena.stmts.alloc(StmtData {
                        location,
                        kind: Stmt::Expr(expr_id),
                    })
                } else {
                    self.create_error_statement(node)
                }
            }
            SyntaxKind::ReturnStatement => {
                let expr_id = if let Some(expr_node) = node.nth_node(0) {
                    self.lower_expression(expr_node)
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
            SyntaxKind::LoopStatement => {
                let condition = self.loop_condition(node).map(|n| self.lower_expression(n));
                let body = if let Some(body_block) = self.loop_body(node) {
                    self.lower_block(body_block)
                } else {
                    self.push_error(node, "Missing loop body".to_string());
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
            SyntaxKind::IfStatement => self.lower_if_statement(node, location),
            SyntaxKind::VariableDefinitionStatement => {
                let ty = self.lower_type_after_colon_or_unit(node, location);
                let name = self.lower_name_or_error(self.first_identifier(node), node);
                let is_mut = node.child(SyntaxKind::MutKeyword).is_some();
                let value = self
                    .value_expression_child(node)
                    .map(|n| self.lower_expression(n));

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
            SyntaxKind::TypeDefinitionStatement => {
                let ty = match self.node_after_token(node, SyntaxKind::Eq) {
                    Some(type_node) => self.lower_type(type_node),
                    None => self.alloc_simple_type(location, SimpleTypeKind::Unit),
                };
                let name = self.lower_name_or_error(self.first_identifier(node), node);
                self.arena.stmts.alloc(StmtData {
                    location,
                    kind: Stmt::TypeDef { name, ty },
                })
            }
            SyntaxKind::AssertStatement => {
                // `assert_statement` parses its expression unconditionally, so an
                // EOF-truncated `assert` leaves no expression node; fall back to
                // an `<error>` expression (design §8).
                let expr_id = self.lower_expression_or_error(node.nth_node(0), node);
                self.arena.stmts.alloc(StmtData {
                    location,
                    kind: Stmt::Assert { expr: expr_id },
                })
            }
            SyntaxKind::BreakStatement => self.arena.stmts.alloc(StmtData {
                location,
                kind: Stmt::Break,
            }),
            SyntaxKind::ConstantDefinition => {
                let def_id = self.lower_constant_definition(node);
                self.arena.stmts.alloc(StmtData {
                    location,
                    kind: Stmt::ConstDef(def_id),
                })
            }
            _ => {
                self.push_error(node, format!("Unexpected statement type '{:?}'", node.kind));
                self.create_error_statement(node)
            }
        }
    }

    /// Mirrors `Builder::build_statement`'s `if_statement` arm. Alloc order:
    /// condition expression, then the `if_arm` block, then the optional
    /// `else_arm` block. Our CST lays out child nodes as condition, then `Block`s
    /// for each arm in source order; the first `Block` is the then-arm and (for a
    /// plain `if/else`) the last is the else-arm.
    fn lower_if_statement(&mut self, node: &SyntaxNode, location: Location) -> StmtId {
        let condition = if let Some(condition_node) = self.if_condition(node) {
            self.lower_expression(condition_node)
        } else {
            self.push_error(node, "Missing if condition".to_string());
            self.create_error_expr(node)
        };
        let blocks: Vec<&SyntaxNode> = self.if_arm_blocks(node);
        let then_block = if let Some(then_node) = blocks.first() {
            self.lower_block(then_node)
        } else {
            self.push_error(node, "Missing if body".to_string());
            self.arena.blocks.alloc(BlockData {
                location,
                block_kind: BlockKind::Regular,
                stmts: vec![],
            })
        };
        let else_block = self.if_else_block(node).map(|n| self.lower_block(n));
        self.arena.stmts.alloc(StmtData {
            location,
            kind: Stmt::If {
                condition,
                then_block,
                else_block,
            },
        })
    }

    /// Mirrors `Builder::create_error_statement`: an `<error>` identifier
    /// expression wrapped in an `Expr` statement.
    fn create_error_statement(&mut self, node: &SyntaxNode) -> StmtId {
        let location = node.loc;
        let error_expr = self.create_error_expr(node);
        self.arena.stmts.alloc(StmtData {
            location,
            kind: Stmt::Expr(error_expr),
        })
    }

    /// Mirrors `Builder::create_error_expr`: an `<error>` identifier and an
    /// identifier expression referencing it.
    fn create_error_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let location = node.loc;
        let error_ident = self.error_ident(location);
        self.arena.exprs.alloc(ExprData {
            location,
            kind: Expr::Identifier(error_ident),
        })
    }

    /// Lowers an optional operand expression, falling back to an `<error>`
    /// expression (located at `parent`) when the operand node is absent.
    ///
    /// The grammar's expression rules can leave an operand slot empty on an
    /// EOF-truncated input (`a +`, `-`, `(`, `a[`, `x =`, `assert` with nothing
    /// following), because `err_recover` at end of input records an error without
    /// emitting an `Error` node to fill the slot. This keeps lowering total
    /// (design §8); a well-formed expression always supplies the operand, so the
    /// fallback never perturbs valid-input arenas.
    fn lower_expression_or_error(
        &mut self,
        operand: Option<&SyntaxNode>,
        parent: &SyntaxNode,
    ) -> ExprId {
        match operand {
            Some(operand) => self.lower_expression(operand),
            None => {
                self.push_error(parent, "Expression is missing an operand".to_string());
                self.create_error_expr(parent)
            }
        }
    }

    /// Allocates an `<error>` placeholder [`Ident`] at `location`, mirroring the
    /// `<error>`-identifier idiom used by `create_error_definition`,
    /// `create_error_statement` and `create_error_expr`.
    ///
    /// Used where the grammar's error-recovery path can leave a required name
    /// child absent (a member name after `.`/`::`, a qualified name after `::`),
    /// so lowering stays total and never panics (design §8). It never fires on
    /// valid input — those CSTs always carry the name — so arena parity holds.
    fn error_ident(&mut self, location: Location) -> IdentId {
        self.arena.idents.alloc(Ident {
            location,
            name: "<error>".to_string(),
        })
    }

    /// Mirrors `Builder::build_expression`, dispatching on node kind. Replicates
    /// `builder.rs` alloc order per arm — `binary` is **left then right**, the
    /// postfix accesses are **expr then name**, etc.
    fn lower_expression(&mut self, node: &SyntaxNode) -> ExprId {
        let location = node.loc;
        match node.kind {
            SyntaxKind::ArrayIndexAccessExpression => {
                let mut children = node.node_children();
                // guaranteed: the postfix `array_index` rule completes this node
                // only after its base operand `lhs`, so the array node is present.
                let array_node = children.next().expect("index access has an array expr");
                let array = self.lower_expression(array_node);
                // The index follows `[`; an EOF-truncated `a[` leaves no node, so
                // fall back to an `<error>` expression (design §8).
                let index = self.lower_expression_or_error(children.next(), node);
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::ArrayIndexAccess { array, index },
                })
            }
            SyntaxKind::GenericName => {
                let type_id = self.lower_type(node);
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::Type(type_id),
                })
            }
            SyntaxKind::TypeQualifiedName => self.lower_qualified_name_expression(node),
            SyntaxKind::MemberAccessExpression => {
                // guaranteed: the postfix `member_access` rule completes this
                // node only after its base operand `lhs`, so the base node is
                // always present.
                let expr_node = node.nth_node(0).expect("member access has an expression");
                let expr = self.lower_expression(expr_node);
                let name = self.member_name_or_error(node);
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::MemberAccess { expr, name },
                })
            }
            SyntaxKind::TypeMemberAccessExpression => {
                // guaranteed: the postfix `type_member_access` rule completes
                // this node only after its base operand `lhs`.
                let expr_node = node
                    .nth_node(0)
                    .expect("type-member access has an expression");
                let expr = self.lower_expression(expr_node);
                let name = self.member_name_or_error(node);
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::TypeMemberAccess { expr, name },
                })
            }
            SyntaxKind::FunctionCallExpression => self.lower_function_call_expression(node),
            SyntaxKind::StructExpression => self.lower_struct_expression(node),
            SyntaxKind::PrefixUnaryExpression => {
                // guaranteed: `unary_expr` completes the prefix node only after
                // emitting the operator node (`op_marker.complete`), so the
                // operator child is always present.
                let operator_node = node
                    .node_children()
                    .find(|n| is_unary_operator_node(n.kind))
                    .expect("prefix unary has an operator node");
                let op = match operator_node.kind {
                    SyntaxKind::UnaryNot => UnaryOperatorKind::Not,
                    SyntaxKind::UnaryMinus => UnaryOperatorKind::Neg,
                    SyntaxKind::UnaryBitnot => UnaryOperatorKind::BitNot,
                    // guaranteed: the `find` above admits only these three kinds.
                    other => unreachable!("Unexpected unary operator node: {other:?}"),
                };
                // The operand follows the operator; an EOF-truncated `-` leaves no
                // operand node, so fall back to an `<error>` expression (§8).
                let inner_node = node
                    .node_children()
                    .find(|n| !is_unary_operator_node(n.kind));
                let inner = self.lower_expression_or_error(inner_node, node);
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::PrefixUnary { expr: inner, op },
                })
            }
            SyntaxKind::ParenthesizedExpression => {
                // The inner expression follows `(`; an EOF-truncated `(` leaves no
                // node, so fall back to an `<error>` expression (design §8).
                let inner = self.lower_expression_or_error(node.nth_node(0), node);
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::Parenthesized { expr: inner },
                })
            }
            SyntaxKind::BinaryExpression => self.lower_binary_expression(node),
            SyntaxKind::BoolLiteral
            | SyntaxKind::StringLiteral
            | SyntaxKind::NumberLiteral
            | SyntaxKind::ArrayLiteral
            | SyntaxKind::UnitLiteral => self.lower_literal(node),
            SyntaxKind::UzumakiKeyword => self.arena.exprs.alloc(ExprData {
                location,
                kind: Expr::Uzumaki,
            }),
            SyntaxKind::Identifier => {
                let ident_id = self.lower_identifier(node);
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::Identifier(ident_id),
                })
            }
            _ => {
                self.push_error(
                    node,
                    format!("Unexpected expression node kind '{:?}'", node.kind),
                );
                self.create_error_expr(node)
            }
        }
    }

    /// Lowers a `TypeQualifiedName` (`alias :: name`) standing in **expression**
    /// position as a `TypeMemberAccess`, matching `builder.rs`.
    ///
    /// The new grammar groups `a::B` into a single `TypeQualifiedName` node,
    /// whereas the legacy tree-sitter grammar nests it as a
    /// `type_member_access_expression` whose base is the `a` identifier. To keep
    /// the arena byte-identical, `a::B` in expression position lowers to
    /// `TypeMemberAccess { expr: Identifier(a), name: B }`. Alloc order mirrors
    /// the legacy CST walk: the alias identifier expression first (its `Ident`
    /// then its `ExprData`), then the `name` identifier, then the
    /// `TypeMemberAccess` expression. The base expression's location spans only
    /// the alias, and the access spans the whole node.
    fn lower_qualified_name_expression(&mut self, node: &SyntaxNode) -> ExprId {
        let location = node.loc;
        let mut idents = node.children_of(SyntaxKind::Identifier);
        // guaranteed: `name_expr` parses the alias via `identifier`, which always
        // completes an `Identifier` node (even on its error path), before the `::`.
        let alias_node = idents.next().expect("qualified name has an alias");
        // The trailing name comes from `simple_name`, whose error path completes
        // no node (`a::` with nothing after). Use the alias node's location for
        // the synthesized `<error>` placeholder so lowering stays total (§8).
        let name_node = idents.next();

        let alias_ident = self.lower_identifier(alias_node);
        let base = self.arena.exprs.alloc(ExprData {
            location: alias_node.loc,
            kind: Expr::Identifier(alias_ident),
        });
        let name = match name_node {
            Some(name_node) => self.lower_identifier(name_node),
            None => {
                self.push_error(node, "Qualified name is missing a name".to_string());
                self.error_ident(location)
            }
        };
        self.arena.exprs.alloc(ExprData {
            location,
            kind: Expr::TypeMemberAccess { expr: base, name },
        })
    }

    /// Mirrors `Builder::build_function_call_expression`. Alloc order: the
    /// function expression, then, **for each argument**, if a `name :` prefix is
    /// present its `_name` is built **as an expression** (allocating an
    /// `Identifier` and its `ExprData`) before the argument expression; finally
    /// the type-parameter identifiers and the `ExprData`.
    ///
    /// The grammar emits, for a named argument, the name `_name` node and then the
    /// argument expression node, in that order; our CST keeps both as sibling node
    /// children. We pair them by walking the children: a name node immediately
    /// preceding an argument that has a `:` between them is the argument name.
    fn lower_function_call_expression(&mut self, node: &SyntaxNode) -> ExprId {
        let location = node.loc;
        // guaranteed: the postfix `function_call` rule completes this node only
        // after its callee operand `lhs`, so the function node is always present.
        let function_node = node.nth_node(0).expect("call has a function expression");
        let function = self.lower_expression(function_node);

        let mut args: Vec<(Option<IdentId>, ExprId)> = Vec::new();
        let mut pending_name: Option<IdentId> = None;

        for (child, is_name) in self.call_argument_children(node) {
            if is_name {
                let expr_id = self.lower_expression(child);
                if let Expr::Identifier(ident_id) = self.arena[expr_id].kind {
                    pending_name = Some(ident_id);
                }
            } else {
                let expr_id = self.lower_expression(child);
                let name = pending_name.take();
                args.push((name, expr_id));
            }
        }

        // The current grammar never emits a `type_parameters` field on a call, so
        // this list is always empty; kept to mirror `builder.rs`'s shape.
        let type_params = Vec::new();

        self.arena.exprs.alloc(ExprData {
            location,
            kind: Expr::FunctionCall {
                function,
                type_params,
                args,
            },
        })
    }

    /// Mirrors `Builder::build_struct_expression`. Alloc order: the struct name
    /// identifier, then **for each field** the field-name `_name` built **as an
    /// expression** (allocating an `Identifier` and its `ExprData`) followed by
    /// the field value expression; finally the `ExprData`.
    fn lower_struct_expression(&mut self, node: &SyntaxNode) -> ExprId {
        let location = node.loc;
        let name = self.lower_name_or_error(self.struct_expr_name(node), node);

        let mut fields: Vec<(IdentId, ExprId)> = Vec::new();
        let mut pending_name: Option<IdentId> = None;

        for (child, is_field_name) in self.struct_field_children(node) {
            if is_field_name {
                let expr_id = self.lower_expression(child);
                if let Expr::Identifier(ident_id) = self.arena[expr_id].kind {
                    pending_name = Some(ident_id);
                }
            } else {
                let expr_id = self.lower_expression(child);
                // A well-formed struct literal always has a `field_name :` before
                // each value, so `pending_name` is set. On a malformed
                // error-recovery tree it may be absent; we drop the orphaned value
                // rather than panic, keeping `parse` total (design §8).
                if let Some(field_name) = pending_name.take() {
                    fields.push((field_name, expr_id));
                }
            }
        }

        self.arena.exprs.alloc(ExprData {
            location,
            kind: Expr::StructLiteral { name, fields },
        })
    }

    /// Mirrors `Builder::build_binary_expression`: **left, then right**; the
    /// operator token is mapped to `OperatorKind` but not allocated.
    fn lower_binary_expression(&mut self, node: &SyntaxNode) -> ExprId {
        let location = node.loc;
        let mut children = node.node_children();
        // guaranteed: the Pratt loop folds an already-parsed `lhs` into the
        // binary node, so the left operand node is always present.
        let left_node = children.next().expect("binary has a left operand");
        let left = self.lower_expression(left_node);
        let op = self.binary_operator(node);
        // The right operand follows the operator; an EOF-truncated `a +` leaves
        // no node, so fall back to an `<error>` expression (design §8).
        let right = self.lower_expression_or_error(children.next(), node);

        self.arena.exprs.alloc(ExprData {
            location,
            kind: Expr::Binary { left, right, op },
        })
    }

    /// Maps a `binary_expression`'s operator token to `OperatorKind`, mirroring
    /// `Builder::build_binary_expression`'s match. An unrecognized operator
    /// records an error and falls back to `Add`, as `builder.rs` does.
    fn binary_operator(&mut self, node: &SyntaxNode) -> OperatorKind {
        let op_token = node.first_token_of_any(&BINARY_OPERATOR_TOKENS);
        match op_token.map(|t| t.kind) {
            Some(SyntaxKind::StarStar) => OperatorKind::Pow,
            Some(SyntaxKind::AmpAmp) => OperatorKind::And,
            Some(SyntaxKind::PipePipe) => OperatorKind::Or,
            Some(SyntaxKind::Plus) => OperatorKind::Add,
            Some(SyntaxKind::Minus) => OperatorKind::Sub,
            Some(SyntaxKind::Star) => OperatorKind::Mul,
            Some(SyntaxKind::Slash) => OperatorKind::Div,
            Some(SyntaxKind::Percent) => OperatorKind::Mod,
            Some(SyntaxKind::Lt) => OperatorKind::Lt,
            Some(SyntaxKind::Le) => OperatorKind::Le,
            Some(SyntaxKind::EqEq) => OperatorKind::Eq,
            Some(SyntaxKind::Ne) => OperatorKind::Ne,
            Some(SyntaxKind::Ge) => OperatorKind::Ge,
            Some(SyntaxKind::Gt) => OperatorKind::Gt,
            Some(SyntaxKind::Shl) => OperatorKind::Shl,
            Some(SyntaxKind::Shr) => OperatorKind::Shr,
            Some(SyntaxKind::Caret) => OperatorKind::BitXor,
            Some(SyntaxKind::Amp) => OperatorKind::BitAnd,
            Some(SyntaxKind::Pipe) => OperatorKind::BitOr,
            _ => {
                self.push_error(node, "Unexpected binary operator".to_string());
                OperatorKind::Add
            }
        }
    }

    /// Mirrors `Builder::build_literal`. Array literals lower each element
    /// expression in order; bool/string/number store the raw source text (string
    /// includes the quotes, number includes a leading `-`); unit allocs a
    /// `UnitLiteral`.
    fn lower_literal(&mut self, node: &SyntaxNode) -> ExprId {
        let location = node.loc;
        match node.kind {
            SyntaxKind::ArrayLiteral => {
                let elem_nodes: Vec<&SyntaxNode> = node.node_children().collect();
                let elements: Vec<ExprId> = elem_nodes
                    .into_iter()
                    .map(|elem| self.lower_expression(elem))
                    .collect();
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::ArrayLiteral { elements },
                })
            }
            SyntaxKind::BoolLiteral => {
                let text = node.text(self.src);
                let value = match text {
                    "true" => true,
                    "false" => false,
                    _ => {
                        self.push_error(node, format!("Unexpected boolean literal value '{text}'"));
                        false
                    }
                };
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::BoolLiteral { value },
                })
            }
            SyntaxKind::StringLiteral => {
                let value = node.text(self.src).to_string();
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::StringLiteral { value },
                })
            }
            SyntaxKind::NumberLiteral => {
                let value = node.text(self.src).to_string();
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::NumberLiteral { value },
                })
            }
            SyntaxKind::UnitLiteral => self.arena.exprs.alloc(ExprData {
                location,
                kind: Expr::UnitLiteral,
            }),
            _ => {
                self.push_error(node, format!("Unexpected literal type '{:?}'", node.kind));
                self.arena.exprs.alloc(ExprData {
                    location,
                    kind: Expr::UnitLiteral,
                })
            }
        }
    }

    /// Mirrors `Builder::build_string_literal_value`: the literal's raw source
    /// text, quotes included (used for the `from` of a use directive).
    fn lower_string_literal_value(&mut self, node: &SyntaxNode) -> String {
        node.text(self.src).to_string()
    }

    /// Mirrors `Builder::build_type`, dispatching on node kind. Primitive type
    /// keywords map to `SimpleTypeKind`; arrays lower **element then length**;
    /// generics lower the base identifier then the argument identifiers; qualified
    /// names lower alias then name; a bare `identifier` is a `Custom` type.
    #[allow(clippy::too_many_lines)]
    fn lower_type(&mut self, node: &SyntaxNode) -> TypeId {
        let location = node.loc;
        match node.kind {
            SyntaxKind::TypeUnit => self.alloc_simple_type(location, SimpleTypeKind::Unit),
            SyntaxKind::TypeBool => self.alloc_simple_type(location, SimpleTypeKind::Bool),
            SyntaxKind::TypeI8 => self.alloc_simple_type(location, SimpleTypeKind::I8),
            SyntaxKind::TypeI16 => self.alloc_simple_type(location, SimpleTypeKind::I16),
            SyntaxKind::TypeI32 => self.alloc_simple_type(location, SimpleTypeKind::I32),
            SyntaxKind::TypeI64 => self.alloc_simple_type(location, SimpleTypeKind::I64),
            SyntaxKind::TypeU8 => self.alloc_simple_type(location, SimpleTypeKind::U8),
            SyntaxKind::TypeU16 => self.alloc_simple_type(location, SimpleTypeKind::U16),
            SyntaxKind::TypeU32 => self.alloc_simple_type(location, SimpleTypeKind::U32),
            SyntaxKind::TypeU64 => self.alloc_simple_type(location, SimpleTypeKind::U64),
            SyntaxKind::TypeArray => {
                // The element type comes from `array_type`'s `type_(p)`, whose
                // error path (`[` with no following type, e.g. `[ = 0`) completes
                // no node. Synthesize a `Unit` element type so lowering stays
                // total (design §8); valid arrays always carry the element, so
                // this never perturbs valid-input arenas.
                let element = match self.first_type_child(node) {
                    Some(element_node) => self.lower_type(element_node),
                    None => {
                        self.push_error(node, "Array type is missing an element type".to_string());
                        self.alloc_simple_type(location, SimpleTypeKind::Unit)
                    }
                };
                // The length is optional in the grammar (`[T]` has none). The
                // legacy `Builder` unwraps a missing length and panics on such
                // inputs; we instead stay total (the never-panic contract,
                // design §8) by synthesizing a `UnitLiteral` size. This only
                // affects inputs the legacy parser cannot handle, so it never
                // perturbs oracle parity (those sources are skipped).
                let size = match self.array_length(node) {
                    Some(length_node) => self.lower_expression(length_node),
                    None => self.arena.exprs.alloc(ExprData {
                        location,
                        kind: Expr::UnitLiteral,
                    }),
                };
                self.arena.types.alloc(TypeData {
                    location,
                    kind: TypeNode::Array { element, size },
                })
            }
            SyntaxKind::GenericName => {
                // guaranteed: a `GenericName` node is only ever completed by the
                // grammar's generic-name path, which emits the base `identifier`
                // and the `type_argument_list` before completing, so both are
                // always present. The base still routes through the total
                // name-or-error helper for uniformity.
                let base = self.lower_name_or_error(self.first_identifier(node), node);
                let arg_list = node
                    .child(SyntaxKind::TypeArgumentList)
                    .expect("generic name has a type-argument list");
                // Mirrors `builder.rs`'s `generic_type` arm, which calls
                // `build_identifier` on each `type` argument — so each parameter
                // is recorded as an *identifier* carrying the type argument's raw
                // source text (e.g. `i32`, `ns::String`), regardless of the
                // argument's actual node kind. The `TypeArgumentList` node holds
                // one type node per argument (`TypeI32`, `TypeQualifiedName`, …);
                // we lower each as an identifier.
                let arg_nodes: Vec<&SyntaxNode> = arg_list.node_children().collect();
                let params: Vec<IdentId> = arg_nodes
                    .into_iter()
                    .map(|param| self.lower_identifier(param))
                    .collect();
                self.arena.types.alloc(TypeData {
                    location,
                    kind: TypeNode::Generic { base, params },
                })
            }
            SyntaxKind::TypeQualifiedName => {
                let mut idents = node.children_of(SyntaxKind::Identifier);
                // guaranteed: `name` parses the alias via `identifier`, which
                // always completes an `Identifier` node before the `::`.
                let alias_node = idents.next().expect("qualified name has an alias");
                let alias = self.lower_identifier(alias_node);
                // The name comes from `qualified_simple_name`, whose error path
                // completes no node; synthesize an `<error>` ident so lowering
                // stays total (design §8) without perturbing valid-input arenas.
                let name = match idents.next() {
                    Some(name_node) => self.lower_identifier(name_node),
                    None => {
                        self.push_error(node, "Qualified name is missing a name".to_string());
                        self.error_ident(location)
                    }
                };
                self.arena.types.alloc(TypeData {
                    location,
                    kind: TypeNode::Qualified { alias, name },
                })
            }
            SyntaxKind::TypeFn => {
                // Mirrors `builder.rs`'s `type_fn` arm, which reads the
                // `argument` field on the `type_fn` node itself. In the
                // tree-sitter grammar that field lives on the nested
                // `argument_list`, not on `type_fn`, so the lookup finds nothing
                // and the parameter list is always empty. We replicate that:
                // `fn(...)` type parameters are *not* lowered (a long-standing
                // quirk the AST parity contract pins). Only the return type is
                // lowered, from after the `->` arrow.
                let params: Vec<TypeId> = Vec::new();
                let ret = self
                    .function_return_type(node)
                    .map(|ret_node| self.lower_type(ret_node));
                self.arena.types.alloc(TypeData {
                    location,
                    kind: TypeNode::Function { params, ret },
                })
            }
            SyntaxKind::Identifier => {
                let ident_id = self.lower_identifier(node);
                self.arena.types.alloc(TypeData {
                    location,
                    kind: TypeNode::Custom(ident_id),
                })
            }
            _ => {
                self.push_error(node, format!("Unexpected type '{:?}'", node.kind));
                self.arena.types.alloc(TypeData {
                    location,
                    kind: TypeNode::Simple(SimpleTypeKind::Unit),
                })
            }
        }
    }

    /// Allocates a primitive `Simple` type at `location`.
    fn alloc_simple_type(&mut self, location: Location, kind: SimpleTypeKind) -> TypeId {
        self.arena.types.alloc(TypeData {
            location,
            kind: TypeNode::Simple(kind),
        })
    }

    /// The `name : type` binding's type, lowered; or a unit placeholder when the
    /// type is missing (a malformed error-recovery node). Keeps lowering total.
    fn lower_type_after_colon_or_unit(&mut self, node: &SyntaxNode, location: Location) -> TypeId {
        match self.first_type_child(node) {
            Some(type_node) => self.lower_type(type_node),
            None => self.alloc_simple_type(location, SimpleTypeKind::Unit),
        }
    }

    /// The `… = value` binding's initializer, lowered; or a unit-literal
    /// placeholder when the value is missing. Keeps lowering total.
    fn lower_value_after_eq_or_unit(&mut self, node: &SyntaxNode, location: Location) -> ExprId {
        match self.value_expression_child(node) {
            Some(value_node) => self.lower_expression(value_node),
            None => self.arena.exprs.alloc(ExprData {
                location,
                kind: Expr::UnitLiteral,
            }),
        }
    }

    /// Mirrors `Builder::build_identifier`: alloc an `Ident` carrying the node's
    /// location and its raw source text.
    fn lower_identifier(&mut self, node: &SyntaxNode) -> IdentId {
        let location = node.loc;
        let name = node.text(self.src).to_string();
        self.arena.idents.alloc(Ident { location, name })
    }

    /// The location spanning the whole source, matching tree-sitter's root node.
    ///
    /// Tree-sitter's `source_file` node always covers the entire input, including
    /// any trailing trivia (e.g. a final newline), so the legacy `Builder`'s
    /// `SourceFileData.location` does too. Our `SourceFile` CST node, by contrast,
    /// spans only its first..last non-trivia descendant. To keep the arena
    /// byte-identical we recompute the file location from the raw source: bytes
    /// `0..len`, line/column `1`-based with the column counted in bytes and reset
    /// after each `\n` — the same convention the lexer uses.
    #[allow(clippy::cast_possible_truncation)]
    fn whole_source_location(&self) -> Location {
        let mut line = 1u32;
        let mut column = 1u32;
        for &byte in self.src.as_bytes() {
            if byte == b'\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
        let len = self.src.len() as u32;
        Location::new(0, len, 1, 1, line, column)
    }

    // -- CST navigation helpers (replace tree-sitter field-name lookups) -------

    /// The first direct `Identifier` child of `node`, if any.
    ///
    /// Every grammar rule that requires a name parses it via `types::identifier`,
    /// which always completes an `Identifier` node (even on its error path), so
    /// for valid input this is always `Some`. It is an `Option` so callers stay
    /// total against any future rule that could omit it (design §8).
    fn first_identifier<'n>(&self, node: &'n SyntaxNode) -> Option<&'n SyntaxNode> {
        node.child(SyntaxKind::Identifier)
    }

    /// Lowers `name` (the result of [`Self::first_identifier`]) to an `Ident`, or
    /// synthesizes an `<error>` placeholder ident located at `parent` when the
    /// name child is absent. Keeps name lowering total without perturbing
    /// valid-input arenas (valid CSTs always carry the name).
    fn lower_name_or_error(&mut self, name: Option<&SyntaxNode>, parent: &SyntaxNode) -> IdentId {
        match name {
            Some(name) => self.lower_identifier(name),
            None => {
                self.push_error(parent, "Definition is missing a name".to_string());
                self.error_ident(parent.loc)
            }
        }
    }

    /// The type node in a binding: the node child immediately after the first `:`
    /// token (`name : type` in `struct_field`, `argument_declaration`,
    /// `ignore_argument`, `constant_definition`, `variable_definition_statement`,
    /// `type_definition_statement`). When there is no `:` — the `array_type`
    /// element, spelled directly after `[` — it is the first node child instead.
    /// Anchoring on `:` matters: a bare-`identifier` name is indistinguishable
    /// from an `identifier`-as-custom-type by kind alone, so the `:` separator is
    /// what tells them apart (design §7).
    fn first_type_child<'n>(&self, node: &'n SyntaxNode) -> Option<&'n SyntaxNode> {
        self.node_after_token(node, SyntaxKind::Colon)
            .or_else(|| node.node_children().next())
    }

    /// The initializer expression in a `… = value` binding: the node child
    /// immediately after the `=` token (`constant_definition`,
    /// `variable_definition_statement`). Anchoring on `=` is required because the
    /// preceding type may itself be a bare `identifier` node, which shares a kind
    /// with an identifier expression.
    fn value_expression_child<'n>(&self, node: &'n SyntaxNode) -> Option<&'n SyntaxNode> {
        self.node_after_token(node, SyntaxKind::Eq)
    }

    /// The return type of a function/external-function/function-type node: the
    /// node child immediately after the `->` arrow token (`None` when there is no
    /// declared return).
    fn function_return_type<'n>(&self, node: &'n SyntaxNode) -> Option<&'n SyntaxNode> {
        self.node_after_token(node, SyntaxKind::Arrow)
    }

    /// The body block of a function definition: the trailing `_block` child (a
    /// plain `Block` or a non-det block).
    fn function_body<'n>(&self, node: &'n SyntaxNode) -> Option<&'n SyntaxNode> {
        node.node_children()
            .filter(|n| is_block_node(n.kind))
            .last()
    }

    /// The optional loop condition expression: an expression child preceding the
    /// loop body block.
    fn loop_condition<'n>(&self, node: &'n SyntaxNode) -> Option<&'n SyntaxNode> {
        node.node_children().find(|n| is_expression_node(n.kind))
    }

    /// The loop body block: the trailing `_block` child.
    fn loop_body<'n>(&self, node: &'n SyntaxNode) -> Option<&'n SyntaxNode> {
        node.node_children()
            .filter(|n| is_block_node(n.kind))
            .last()
    }

    /// The `if` head condition: the first expression child, which sits before the
    /// then-arm block. Further `else if` conditions follow their own arm blocks
    /// and are matched positionally by [`Self::if_else_block`].
    fn if_condition<'n>(&self, node: &'n SyntaxNode) -> Option<&'n SyntaxNode> {
        node.node_children().find(|n| is_expression_node(n.kind))
    }

    /// The arm blocks of an `if` statement, in source order. The first is the
    /// then-arm.
    fn if_arm_blocks<'n>(&self, node: &'n SyntaxNode) -> Vec<&'n SyntaxNode> {
        node.node_children()
            .filter(|n| is_block_node(n.kind))
            .collect()
    }

    /// The trailing bare `else` arm block, if present. The grammar produces one
    /// arm block per condition (`if`/`else if`) plus an optional final block for
    /// a bare `else`; so an extra block beyond the condition count is the
    /// else-arm.
    fn if_else_block<'n>(&self, node: &'n SyntaxNode) -> Option<&'n SyntaxNode> {
        let conditions = node
            .node_children()
            .filter(|n| is_expression_node(n.kind))
            .count();
        let blocks: Vec<&SyntaxNode> = self.if_arm_blocks(node);
        if blocks.len() > conditions {
            blocks.last().copied()
        } else {
            None
        }
    }

    /// The member/type-member access name, lowered; or an `<error>` placeholder
    /// when the name is absent.
    ///
    /// The name is the `_simple_name` child after the `.`/`::` accessor token. On
    /// a malformed access (`a.`, `a::` with nothing after the accessor) the
    /// grammar's `simple_name` error path completes no `Identifier`, so the child
    /// is genuinely absent; we synthesize an `<error>` ident and record an error
    /// rather than panic (design §8). A well-formed access always has the name, so
    /// this never perturbs valid-input arenas.
    fn member_name_or_error(&mut self, node: &SyntaxNode) -> IdentId {
        let name_node = self
            .node_after_token(node, SyntaxKind::Dot)
            .or_else(|| self.node_after_token(node, SyntaxKind::ColonColon));
        match name_node {
            Some(name_node) => self.lower_identifier(name_node),
            None => {
                self.push_error(node, "Member access is missing a name".to_string());
                self.error_ident(node.loc)
            }
        }
    }

    /// The array type's length expression: the node child after the `;`
    /// separator. Optional in the grammar (`[T]` has none).
    fn array_length<'n>(&self, node: &'n SyntaxNode) -> Option<&'n SyntaxNode> {
        self.node_after_token(node, SyntaxKind::Semi)
    }

    /// The struct-expression name node: the first node child, which the grammar
    /// (`name_atom`) always spells before the `{`. An `Option` for totality; valid
    /// struct literals always carry the name.
    fn struct_expr_name<'n>(&self, node: &'n SyntaxNode) -> Option<&'n SyntaxNode> {
        node.node_children().next()
    }

    /// The first direct child *node* appearing immediately after the first
    /// occurrence of token `kind` among `node`'s direct children (skipping
    /// trivia). Returns `None` if the token is absent or no node follows it.
    fn node_after_token<'n>(
        &self,
        node: &'n SyntaxNode,
        kind: SyntaxKind,
    ) -> Option<&'n SyntaxNode> {
        use crate::syntax_tree::SyntaxElement;
        let mut after = false;
        for element in &node.children {
            match element {
                SyntaxElement::Token(t) if t.kind == kind => after = true,
                SyntaxElement::Node(n) if after => return Some(n),
                _ => {}
            }
        }
        None
    }

    /// Pairs each call-argument child with whether it is an argument *name*.
    ///
    /// The grammar emits, per argument, an optional `_name` (followed by `:`) and
    /// then the argument expression. The function expression is the first child
    /// and is skipped here. A child is an argument name iff a `:` token follows it
    /// among the node's direct children — i.e. it occupies the `name :` slot.
    fn call_argument_children<'n>(&self, node: &'n SyntaxNode) -> Vec<(&'n SyntaxNode, bool)> {
        let arg_children: Vec<&SyntaxNode> = node.node_children().skip(1).collect();
        self.tag_name_value_pairs(node, &arg_children)
    }

    /// Pairs each struct-field child with whether it is a *field name*. Each field
    /// is `field_name : field_value`; a child is a field name iff a `:` follows it
    /// among the node's direct children. The struct name (first child) is skipped.
    fn struct_field_children<'n>(&self, node: &'n SyntaxNode) -> Vec<(&'n SyntaxNode, bool)> {
        let field_children: Vec<&SyntaxNode> = node.node_children().skip(1).collect();
        self.tag_name_value_pairs(node, &field_children)
    }

    /// Tags each of `children` (a subset of `node`'s child nodes) with whether it
    /// is immediately followed by a `:` token in `node`'s child sequence, marking
    /// it as a name in a `name : value` pair.
    fn tag_name_value_pairs<'n>(
        &self,
        node: &'n SyntaxNode,
        children: &[&'n SyntaxNode],
    ) -> Vec<(&'n SyntaxNode, bool)> {
        children
            .iter()
            .map(|&child| (child, self.followed_by_colon(node, child)))
            .collect()
    }

    /// Whether the child node `child` is immediately followed by a `:` token
    /// among `node`'s direct children (the only intervening elements being
    /// trivia).
    fn followed_by_colon(&self, node: &SyntaxNode, child: &SyntaxNode) -> bool {
        use crate::syntax_tree::SyntaxElement;
        let mut seen = false;
        for element in &node.children {
            match element {
                SyntaxElement::Node(n) => {
                    if seen {
                        return false;
                    }
                    if std::ptr::eq(n, child) {
                        seen = true;
                    }
                }
                SyntaxElement::Token(t) => {
                    if t.kind.is_trivia() {
                        continue;
                    }
                    if seen {
                        return t.kind == SyntaxKind::Colon;
                    }
                }
            }
        }
        false
    }

    /// The visibility of `node`: `Public` iff a `Visibility` child is present,
    /// else the default (`Private`). Mirrors `Builder::get_visibility`.
    fn visibility(&self, node: &SyntaxNode) -> Visibility {
        if node.child(SyntaxKind::Visibility).is_some() {
            Visibility::Public
        } else {
            Visibility::default()
        }
    }

    /// Records a lowering error at `node`'s location. Lowering errors only arise
    /// from malformed dispatch; valid sources produce none.
    fn push_error(&mut self, node: &SyntaxNode, message: String) {
        self.errors.push(ParseError {
            span: node.loc,
            message,
        });
    }
}

/// The binary-operator token kinds, in the order `binary_operator` matches them.
const BINARY_OPERATOR_TOKENS: [SyntaxKind; 19] = [
    SyntaxKind::StarStar,
    SyntaxKind::AmpAmp,
    SyntaxKind::PipePipe,
    SyntaxKind::Plus,
    SyntaxKind::Minus,
    SyntaxKind::Star,
    SyntaxKind::Slash,
    SyntaxKind::Percent,
    SyntaxKind::Lt,
    SyntaxKind::Le,
    SyntaxKind::EqEq,
    SyntaxKind::Ne,
    SyntaxKind::Ge,
    SyntaxKind::Gt,
    SyntaxKind::Shl,
    SyntaxKind::Shr,
    SyntaxKind::Caret,
    SyntaxKind::Amp,
    SyntaxKind::Pipe,
];

/// Whether `kind` is a prefix-unary operator node (`!`, `-`, `~`).
fn is_unary_operator_node(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::UnaryNot | SyntaxKind::UnaryMinus | SyntaxKind::UnaryBitnot
    )
}

/// Whether `kind` is a `_block` node (a plain block or a non-det block).
fn is_block_node(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Block
            | SyntaxKind::AssumeBlock
            | SyntaxKind::ForallBlock
            | SyntaxKind::ExistsBlock
            | SyntaxKind::UniqueBlock
    )
}

/// Whether `kind` is an expression node kind.
fn is_expression_node(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::BinaryExpression
            | SyntaxKind::PrefixUnaryExpression
            | SyntaxKind::ParenthesizedExpression
            | SyntaxKind::FunctionCallExpression
            | SyntaxKind::ArrayIndexAccessExpression
            | SyntaxKind::MemberAccessExpression
            | SyntaxKind::TypeMemberAccessExpression
            | SyntaxKind::StructExpression
            | SyntaxKind::ArrayLiteral
            | SyntaxKind::BoolLiteral
            | SyntaxKind::StringLiteral
            | SyntaxKind::NumberLiteral
            | SyntaxKind::UnitLiteral
            | SyntaxKind::UzumakiKeyword
            | SyntaxKind::Identifier
            | SyntaxKind::GenericName
            | SyntaxKind::TypeQualifiedName
    )
}

#[cfg(test)]
mod tests {
    //! Direct lowering tests: drive the public [`crate::parse`] pipeline and
    //! assert on the resulting [`AstArena`] so most match arms in this module
    //! execute. Sources are kept compact (one line) per the unit-test
    //! convention; assertions check structural facts (Def/Stmt/Expr/Type kinds,
    //! names, counts, visibility) rather than golden trees.

    use crate::parse;
    use inference_ast::arena::AstArena;
    use inference_ast::ids::{BlockId, DefId, TypeId};
    use inference_ast::nodes::{
        ArgKind, BlockKind, Def, Directive, Expr, OperatorKind, SimpleTypeKind, Stmt, TypeNode,
        UnaryOperatorKind, Visibility,
    };

    /// Parses `src`, asserts the parse produced no errors, and returns the arena.
    /// Used for the valid-input tests; resilience tests call [`parse`] directly.
    fn lower(src: &str) -> AstArena {
        let result = parse(src);
        assert!(
            result.errors.is_empty(),
            "unexpected parse errors for {src:?}: {:?}",
            result.errors
        );
        result.arena
    }

    /// The single top-level definition's `kind`. Asserts exactly one source file
    /// with exactly one top-level def.
    fn single_def(arena: &AstArena) -> &Def {
        let files: Vec<_> = arena.source_files().collect();
        assert_eq!(files.len(), 1, "expected exactly one source file");
        let defs = &files[0].defs;
        assert_eq!(defs.len(), 1, "expected exactly one top-level def");
        &arena[defs[0]].kind
    }

    /// The body block ID of the single top-level function definition.
    fn fn_body(arena: &AstArena) -> BlockId {
        match single_def(arena) {
            Def::Function { body, .. } => *body,
            other => panic!("expected a function definition, got {other:?}"),
        }
    }

    /// The statements of the single top-level function's body block.
    fn fn_stmts(arena: &AstArena) -> Vec<&Stmt> {
        let body = fn_body(arena);
        arena[body].stmts.iter().map(|&s| &arena[s].kind).collect()
    }

    /// The single statement of the single top-level function's body.
    fn single_stmt(arena: &AstArena) -> &Stmt {
        let stmts = fn_stmts(arena);
        assert_eq!(stmts.len(), 1, "expected exactly one body statement");
        stmts[0]
    }

    /// The single expression-statement's expression `kind`. Many expression
    /// tests wrap the expression in `fn f() { <expr> }`.
    fn single_expr(arena: &AstArena) -> &Expr {
        match single_stmt(arena) {
            Stmt::Expr(expr_id) => &arena[*expr_id].kind,
            other => panic!("expected an expression statement, got {other:?}"),
        }
    }

    /// Reads a `TypeNode` by ID (keeps the match arms below readable).
    fn type_kind(arena: &AstArena, ty: TypeId) -> &TypeNode {
        &arena[ty].kind
    }

    // -- Items ---------------------------------------------------------------

    #[test]
    fn lowers_function_without_return() {
        let arena = lower("fn f() { }");
        match single_def(&arena) {
            Def::Function {
                name,
                vis,
                type_params,
                args,
                returns,
                ..
            } => {
                assert_eq!(arena.ident_name(*name), "f");
                assert_eq!(*vis, Visibility::Private);
                assert!(type_params.is_empty());
                assert!(args.is_empty());
                assert!(returns.is_none());
            }
            other => panic!("expected function, got {other:?}"),
        }
    }

    #[test]
    fn lowers_function_with_return_and_params() {
        let arena = lower("fn add(a: i32, b: i32) -> i32 { return a + b; }");
        match single_def(&arena) {
            Def::Function {
                name,
                args,
                returns,
                ..
            } => {
                assert_eq!(arena.ident_name(*name), "add");
                assert_eq!(args.len(), 2);
                let returns = returns.expect("declared return type");
                assert!(matches!(
                    arena[returns].kind,
                    TypeNode::Simple(SimpleTypeKind::I32)
                ));
                match &args[0].kind {
                    ArgKind::Named { name, ty, is_mut } => {
                        assert_eq!(arena.ident_name(*name), "a");
                        assert!(!is_mut);
                        assert!(matches!(
                            arena[*ty].kind,
                            TypeNode::Simple(SimpleTypeKind::I32)
                        ));
                    }
                    other => panic!("expected named arg, got {other:?}"),
                }
            }
            other => panic!("expected function, got {other:?}"),
        }
    }

    #[test]
    fn lowers_mut_arg() {
        let arena = lower("fn f(mut x: i32) { }");
        match single_def(&arena) {
            Def::Function { args, .. } => match &args[0].kind {
                ArgKind::Named { is_mut, .. } => assert!(*is_mut),
                other => panic!("expected named mut arg, got {other:?}"),
            },
            other => panic!("expected function, got {other:?}"),
        }
    }

    #[test]
    fn lowers_self_arg() {
        let arena = lower("fn f(self) { }");
        match single_def(&arena) {
            Def::Function { args, .. } => match &args[0].kind {
                ArgKind::SelfRef { is_mut } => assert!(!is_mut),
                other => panic!("expected self arg, got {other:?}"),
            },
            other => panic!("expected function, got {other:?}"),
        }
    }

    #[test]
    fn lowers_mut_self_arg() {
        let arena = lower("fn f(mut self) { }");
        match single_def(&arena) {
            Def::Function { args, .. } => match &args[0].kind {
                ArgKind::SelfRef { is_mut } => assert!(*is_mut),
                other => panic!("expected mut self arg, got {other:?}"),
            },
            other => panic!("expected function, got {other:?}"),
        }
    }

    #[test]
    fn lowers_ignored_arg() {
        let arena = lower("fn f(_: i32) { }");
        match single_def(&arena) {
            Def::Function { args, .. } => match &args[0].kind {
                ArgKind::Ignored { ty } => assert!(matches!(
                    type_kind(&arena, *ty),
                    TypeNode::Simple(SimpleTypeKind::I32)
                )),
                other => panic!("expected ignored arg, got {other:?}"),
            },
            other => panic!("expected function, got {other:?}"),
        }
    }

    #[test]
    fn lowers_function_type_params() {
        // Type parameters are space-separated before the parameter list, each
        // marked with a trailing `'` (e.g. `fn foo T' U'(...)`), not
        // angle-bracketed. The `'` is a separate token, so the lowered `Ident`
        // carries just the bare name (`T`).
        let arena = lower("fn foo T'(a: i32) { }");
        match single_def(&arena) {
            Def::Function { type_params, .. } => {
                assert_eq!(type_params.len(), 1);
                assert_eq!(arena.ident_name(type_params[0]), "T");
            }
            other => panic!("expected function, got {other:?}"),
        }
    }

    #[test]
    fn lowers_external_function() {
        let arena = lower("external fn ext(a: i32) -> i32;");
        match single_def(&arena) {
            Def::ExternFunction {
                name,
                vis,
                args,
                returns,
            } => {
                assert_eq!(arena.ident_name(*name), "ext");
                assert_eq!(*vis, Visibility::Private);
                assert_eq!(args.len(), 1);
                assert!(returns.is_some());
            }
            other => panic!("expected extern function, got {other:?}"),
        }
    }

    #[test]
    fn lowers_spec_with_nested_defs() {
        let arena = lower("spec S { fn f() { } }");
        match single_def(&arena) {
            Def::Spec { name, vis, defs } => {
                assert_eq!(arena.ident_name(*name), "S");
                assert_eq!(*vis, Visibility::Private);
                assert_eq!(defs.len(), 1);
                assert!(matches!(arena[defs[0]].kind, Def::Function { .. }));
            }
            other => panic!("expected spec, got {other:?}"),
        }
    }

    #[test]
    fn lowers_struct_fields_and_methods() {
        // Struct fields are `;`-terminated; methods follow the fields.
        let arena = lower("struct Point { x: i32; y: i32; fn m(self) { } }");
        match single_def(&arena) {
            Def::Struct {
                name,
                fields,
                methods,
                ..
            } => {
                assert_eq!(arena.ident_name(*name), "Point");
                assert_eq!(fields.len(), 2);
                assert_eq!(arena.ident_name(fields[0].name), "x");
                assert_eq!(arena.ident_name(fields[1].name), "y");
                assert_eq!(methods.len(), 1);
                assert_eq!(arena.def_name(methods[0]), "m");
            }
            other => panic!("expected struct, got {other:?}"),
        }
    }

    #[test]
    fn lowers_enum_variants() {
        let arena = lower("enum Color { Red, Green, Blue }");
        match single_def(&arena) {
            Def::Enum { name, variants, .. } => {
                assert_eq!(arena.ident_name(*name), "Color");
                let names: Vec<&str> = variants.iter().map(|&v| arena.ident_name(v)).collect();
                assert_eq!(names, ["Red", "Green", "Blue"]);
            }
            other => panic!("expected enum, got {other:?}"),
        }
    }

    #[test]
    fn lowers_const() {
        let arena = lower("const PI: i32 = 3;");
        match single_def(&arena) {
            Def::Constant {
                name, ty, value, ..
            } => {
                assert_eq!(arena.ident_name(*name), "PI");
                assert!(matches!(
                    arena[*ty].kind,
                    TypeNode::Simple(SimpleTypeKind::I32)
                ));
                assert!(matches!(arena[*value].kind, Expr::NumberLiteral { .. }));
            }
            other => panic!("expected constant, got {other:?}"),
        }
    }

    #[test]
    fn lowers_type_alias() {
        let arena = lower("type Id = i32;");
        match single_def(&arena) {
            Def::TypeAlias { name, ty, .. } => {
                assert_eq!(arena.ident_name(*name), "Id");
                assert!(matches!(
                    arena[*ty].kind,
                    TypeNode::Simple(SimpleTypeKind::I32)
                ));
            }
            other => panic!("expected type alias, got {other:?}"),
        }
    }

    #[test]
    fn lowers_public_visibility() {
        let arena = lower("pub fn f() { }");
        match single_def(&arena) {
            Def::Function { vis, .. } => assert_eq!(*vis, Visibility::Public),
            other => panic!("expected function, got {other:?}"),
        }
    }

    #[test]
    fn lowers_use_directive_path_form() {
        let arena = lower("use a::b::{ X, Y };");
        let files: Vec<_> = arena.source_files().collect();
        let directives = &files[0].directives;
        assert_eq!(directives.len(), 1);
        let Directive::Use(directive) = &directives[0];
        assert!(directive.from.is_none());
        let segments: Vec<&str> = directive
            .segments
            .iter()
            .map(|&s| arena.ident_name(s))
            .collect();
        assert_eq!(segments, ["a", "b"]);
        let imported: Vec<&str> = directive
            .imported_types
            .iter()
            .map(|&t| arena.ident_name(t))
            .collect();
        assert_eq!(imported, ["X", "Y"]);
    }

    #[test]
    fn lowers_use_directive_from_form() {
        let arena = lower("use { X, Y } from \"lib\";");
        let files: Vec<_> = arena.source_files().collect();
        let Directive::Use(directive) = &files[0].directives[0];
        assert_eq!(directive.from.as_deref(), Some("\"lib\""));
        assert!(directive.segments.is_empty());
        let imported: Vec<&str> = directive
            .imported_types
            .iter()
            .map(|&t| arena.ident_name(t))
            .collect();
        assert_eq!(imported, ["X", "Y"]);
    }

    // -- Statements ----------------------------------------------------------

    #[test]
    fn lowers_let_with_and_without_value() {
        let with_value = lower("fn f() { let x: i32 = 1; }");
        match single_stmt(&with_value) {
            Stmt::VarDef {
                name,
                ty,
                value,
                is_mut,
            } => {
                assert_eq!(with_value.ident_name(*name), "x");
                assert!(!is_mut);
                assert!(matches!(
                    with_value[*ty].kind,
                    TypeNode::Simple(SimpleTypeKind::I32)
                ));
                assert!(value.is_some());
            }
            other => panic!("expected var def, got {other:?}"),
        }

        let no_value = lower("fn f() { let x: i32; }");
        match single_stmt(&no_value) {
            Stmt::VarDef { value, .. } => assert!(value.is_none()),
            other => panic!("expected var def, got {other:?}"),
        }
    }

    #[test]
    fn lowers_let_mut() {
        let arena = lower("fn f() { let mut x: i32 = 1; }");
        match single_stmt(&arena) {
            Stmt::VarDef { is_mut, .. } => assert!(*is_mut),
            other => panic!("expected var def, got {other:?}"),
        }
    }

    #[test]
    fn lowers_assign() {
        let arena = lower("fn f() { x = 1; }");
        match single_stmt(&arena) {
            Stmt::Assign { left, right } => {
                assert!(matches!(arena[*left].kind, Expr::Identifier(_)));
                assert!(matches!(arena[*right].kind, Expr::NumberLiteral { .. }));
            }
            other => panic!("expected assign, got {other:?}"),
        }
    }

    #[test]
    fn lowers_return_with_expr() {
        let arena = lower("fn f() { return a; }");
        match single_stmt(&arena) {
            Stmt::Return { expr } => {
                assert!(matches!(arena[*expr].kind, Expr::Identifier(_)));
            }
            other => panic!("expected return, got {other:?}"),
        }
    }

    #[test]
    fn lowers_bare_return_as_unit_literal() {
        // `return;` with no expression must still allocate a `UnitLiteral`.
        let arena = lower("fn f() { return; }");
        match single_stmt(&arena) {
            Stmt::Return { expr } => {
                assert!(matches!(arena[*expr].kind, Expr::UnitLiteral));
            }
            other => panic!("expected return, got {other:?}"),
        }
    }

    #[test]
    fn lowers_loop_with_and_without_condition() {
        let conditional = lower("fn f() { loop c { break; } }");
        match single_stmt(&conditional) {
            Stmt::Loop { condition, body } => {
                let condition = condition.expect("loop condition");
                assert!(matches!(conditional[condition].kind, Expr::Identifier(_)));
                assert_eq!(conditional[*body].block_kind, BlockKind::Regular);
            }
            other => panic!("expected loop, got {other:?}"),
        }

        let unconditional = lower("fn f() { loop { break; } }");
        match single_stmt(&unconditional) {
            Stmt::Loop { condition, .. } => assert!(condition.is_none()),
            other => panic!("expected loop, got {other:?}"),
        }
    }

    #[test]
    fn lowers_if_else() {
        let arena = lower("fn f() { if c { x = 1; } else { y = 2; } }");
        match single_stmt(&arena) {
            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                assert!(matches!(arena[*condition].kind, Expr::Identifier(_)));
                assert_eq!(arena[*then_block].block_kind, BlockKind::Regular);
                assert!(else_block.is_some());
            }
            other => panic!("expected if, got {other:?}"),
        }
    }

    #[test]
    fn lowers_if_else_if() {
        // `else if` is flattened by the grammar into the single if-statement
        // (no nested if-node); lowering keeps the head condition and the trailing
        // bare-`else` block as `else_block`.
        let arena =
            lower("fn f() { if a { return 1; } else if b { return 2; } else { return 3; } }");
        match single_stmt(&arena) {
            Stmt::If {
                condition,
                else_block,
                ..
            } => {
                match &arena[*condition].kind {
                    Expr::Identifier(id) => assert_eq!(arena.ident_name(*id), "a"),
                    other => panic!("expected identifier condition, got {other:?}"),
                }
                let else_block = else_block.expect("trailing else arm");
                let inner_stmts = &arena[else_block].stmts;
                assert_eq!(inner_stmts.len(), 1);
                assert!(matches!(arena[inner_stmts[0]].kind, Stmt::Return { .. }));
            }
            other => panic!("expected if, got {other:?}"),
        }
    }

    #[test]
    fn lowers_assert() {
        let arena = lower("fn f() { assert x; }");
        match single_stmt(&arena) {
            Stmt::Assert { expr } => {
                assert!(matches!(arena[*expr].kind, Expr::Identifier(_)));
            }
            other => panic!("expected assert, got {other:?}"),
        }
    }

    #[test]
    fn lowers_break() {
        let arena = lower("fn f() { break; }");
        assert!(matches!(single_stmt(&arena), Stmt::Break));
    }

    #[test]
    fn lowers_const_in_body() {
        let arena = lower("fn f() { const C: i32 = 1; }");
        match single_stmt(&arena) {
            Stmt::ConstDef(def_id) => {
                assert!(matches!(arena[*def_id].kind, Def::Constant { .. }));
            }
            other => panic!("expected const def stmt, got {other:?}"),
        }
    }

    #[test]
    fn lowers_type_in_body() {
        let arena = lower("fn f() { type T = i32; }");
        match single_stmt(&arena) {
            Stmt::TypeDef { name, ty } => {
                assert_eq!(arena.ident_name(*name), "T");
                assert!(matches!(
                    arena[*ty].kind,
                    TypeNode::Simple(SimpleTypeKind::I32)
                ));
            }
            other => panic!("expected type def stmt, got {other:?}"),
        }
    }

    #[test]
    fn lowers_nondet_blocks() {
        // Each non-det block (`forall`/`exists`/`unique`/`assume`) lowers to a
        // block statement carrying the matching `BlockKind`.
        let forall = lower("fn f() { forall { return (); } }");
        assert_eq!(nondet_block_kind(&forall), BlockKind::Forall);

        let exists = lower("fn f() { exists { return (); } }");
        assert_eq!(nondet_block_kind(&exists), BlockKind::Exists);

        let unique = lower("fn f() { unique { return (); } }");
        assert_eq!(nondet_block_kind(&unique), BlockKind::Unique);

        let assume = lower("fn f() { assume { return (); } }");
        assert_eq!(nondet_block_kind(&assume), BlockKind::Assume);
    }

    /// The `BlockKind` of the single block statement in the function body.
    fn nondet_block_kind(arena: &AstArena) -> BlockKind {
        match single_stmt(arena) {
            Stmt::Block(block_id) => arena[*block_id].block_kind,
            other => panic!("expected block statement, got {other:?}"),
        }
    }

    // -- Expressions ---------------------------------------------------------

    #[test]
    fn lowers_binary_operators() {
        let add = lower("fn f() { a + b; }");
        match single_expr(&add) {
            Expr::Binary { op, .. } => assert_eq!(*op, OperatorKind::Add),
            other => panic!("expected binary, got {other:?}"),
        }

        let mul = lower("fn f() { a * b; }");
        match single_expr(&mul) {
            Expr::Binary { op, .. } => assert_eq!(*op, OperatorKind::Mul),
            other => panic!("expected binary, got {other:?}"),
        }

        let eq = lower("fn f() { a == b; }");
        match single_expr(&eq) {
            Expr::Binary { op, .. } => assert_eq!(*op, OperatorKind::Eq),
            other => panic!("expected binary, got {other:?}"),
        }
    }

    #[test]
    fn lowers_prefix_unary_operators() {
        let not = lower("fn f() { !flag; }");
        match single_expr(&not) {
            Expr::PrefixUnary { op, .. } => assert_eq!(*op, UnaryOperatorKind::Not),
            other => panic!("expected prefix unary, got {other:?}"),
        }

        let neg = lower("fn f() { -x; }");
        match single_expr(&neg) {
            Expr::PrefixUnary { op, .. } => assert_eq!(*op, UnaryOperatorKind::Neg),
            other => panic!("expected prefix unary, got {other:?}"),
        }

        let bitnot = lower("fn f() { ~bits; }");
        match single_expr(&bitnot) {
            Expr::PrefixUnary { op, .. } => assert_eq!(*op, UnaryOperatorKind::BitNot),
            other => panic!("expected prefix unary, got {other:?}"),
        }
    }

    #[test]
    fn lowers_parenthesized() {
        let arena = lower("fn f() { (1 + 2); }");
        match single_expr(&arena) {
            Expr::Parenthesized { expr } => {
                assert!(matches!(arena[*expr].kind, Expr::Binary { .. }));
            }
            other => panic!("expected parenthesized, got {other:?}"),
        }
    }

    #[test]
    fn lowers_function_call_with_named_and_positional_args() {
        let arena = lower("fn f() { g(x: 1, 2); }");
        match single_expr(&arena) {
            Expr::FunctionCall { function, args, .. } => {
                assert!(matches!(arena[*function].kind, Expr::Identifier(_)));
                assert_eq!(args.len(), 2);
                let (first_name, _) = &args[0];
                let first_name = first_name.expect("named first argument");
                assert_eq!(arena.ident_name(first_name), "x");
                let (second_name, _) = &args[1];
                assert!(second_name.is_none());
            }
            other => panic!("expected function call, got {other:?}"),
        }
    }

    #[test]
    fn lowers_array_index() {
        let arena = lower("fn f() { arr[0]; }");
        match single_expr(&arena) {
            Expr::ArrayIndexAccess { array, index } => {
                assert!(matches!(arena[*array].kind, Expr::Identifier(_)));
                assert!(matches!(arena[*index].kind, Expr::NumberLiteral { .. }));
            }
            other => panic!("expected array index, got {other:?}"),
        }
    }

    #[test]
    fn lowers_member_access() {
        let arena = lower("fn f() { s.field; }");
        match single_expr(&arena) {
            Expr::MemberAccess { expr, name } => {
                assert!(matches!(arena[*expr].kind, Expr::Identifier(_)));
                assert_eq!(arena.ident_name(*name), "field");
            }
            other => panic!("expected member access, got {other:?}"),
        }
    }

    #[test]
    fn lowers_qualified_name_expression() {
        // `a::b` in expression position lowers to `TypeMemberAccess`, mirroring
        // the legacy builder.
        let arena = lower("fn f() { a::b; }");
        match single_expr(&arena) {
            Expr::TypeMemberAccess { expr, name } => {
                assert!(matches!(arena[*expr].kind, Expr::Identifier(_)));
                assert_eq!(arena.ident_name(*name), "b");
            }
            other => panic!("expected type-member access, got {other:?}"),
        }
    }

    #[test]
    fn lowers_struct_literal() {
        let arena = lower("fn f() { S { a: 1 }; }");
        match single_expr(&arena) {
            Expr::StructLiteral { name, fields } => {
                assert_eq!(arena.ident_name(*name), "S");
                assert_eq!(fields.len(), 1);
                let (field_name, field_value) = &fields[0];
                assert_eq!(arena.ident_name(*field_name), "a");
                assert!(matches!(
                    arena[*field_value].kind,
                    Expr::NumberLiteral { .. }
                ));
            }
            other => panic!("expected struct literal, got {other:?}"),
        }
    }

    #[test]
    fn lowers_identifier() {
        let arena = lower("fn f() { x; }");
        match single_expr(&arena) {
            Expr::Identifier(id) => assert_eq!(arena.ident_name(*id), "x"),
            other => panic!("expected identifier, got {other:?}"),
        }
    }

    #[test]
    fn lowers_number_literal_keeps_raw_text() {
        let arena = lower("fn f() { 42; }");
        match single_expr(&arena) {
            Expr::NumberLiteral { value } => assert_eq!(value, "42"),
            other => panic!("expected number literal, got {other:?}"),
        }
    }

    #[test]
    fn lowers_negative_number_literal_keeps_leading_minus() {
        // `-42` is lexed as a single number literal, not a unary minus, so the
        // stored raw text includes the leading `-`.
        let arena = lower("fn f() { -42; }");
        match single_expr(&arena) {
            Expr::NumberLiteral { value } => assert_eq!(value, "-42"),
            other => panic!("expected number literal, got {other:?}"),
        }
    }

    #[test]
    fn lowers_bool_literals() {
        let t = lower("fn f() { true; }");
        match single_expr(&t) {
            Expr::BoolLiteral { value } => assert!(*value),
            other => panic!("expected bool literal, got {other:?}"),
        }

        let f = lower("fn f() { false; }");
        match single_expr(&f) {
            Expr::BoolLiteral { value } => assert!(!value),
            other => panic!("expected bool literal, got {other:?}"),
        }
    }

    #[test]
    fn lowers_string_literal_keeps_quotes() {
        let arena = lower("fn f() { \"hello\"; }");
        match single_expr(&arena) {
            Expr::StringLiteral { value } => assert_eq!(value, "\"hello\""),
            other => panic!("expected string literal, got {other:?}"),
        }
    }

    #[test]
    fn lowers_array_literal() {
        let arena = lower("fn f() { [1, 2, 3]; }");
        match single_expr(&arena) {
            Expr::ArrayLiteral { elements } => assert_eq!(elements.len(), 3),
            other => panic!("expected array literal, got {other:?}"),
        }
    }

    #[test]
    fn lowers_unit_literal() {
        let arena = lower("fn f() { (); }");
        assert!(matches!(single_expr(&arena), Expr::UnitLiteral));
    }

    #[test]
    fn lowers_uzumaki() {
        let arena = lower("fn f() { @; }");
        assert!(matches!(single_expr(&arena), Expr::Uzumaki));
    }

    #[test]
    fn lowers_generic_name_expression() {
        // A generic name in expression position lowers to `Expr::Type` wrapping
        // a `TypeNode::Generic`.
        let arena = lower("fn f() { Vec i32'; }");
        match single_expr(&arena) {
            Expr::Type(type_id) => match &arena[*type_id].kind {
                TypeNode::Generic { base, params } => {
                    assert_eq!(arena.ident_name(*base), "Vec");
                    assert_eq!(params.len(), 1);
                    assert_eq!(arena.ident_name(params[0]), "i32");
                }
                other => panic!("expected generic type, got {other:?}"),
            },
            other => panic!("expected type expression, got {other:?}"),
        }
    }

    // -- Types ---------------------------------------------------------------

    /// The annotated type `kind` of `fn f() { let v: <ty> = x; }`.
    fn let_type(arena: &AstArena) -> &TypeNode {
        match single_stmt(arena) {
            Stmt::VarDef { ty, .. } => &arena[*ty].kind,
            other => panic!("expected var def, got {other:?}"),
        }
    }

    #[test]
    fn lowers_primitive_types() {
        let cases = [
            ("fn f() { let v: i8 = x; }", SimpleTypeKind::I8),
            ("fn f() { let v: i16 = x; }", SimpleTypeKind::I16),
            ("fn f() { let v: i32 = x; }", SimpleTypeKind::I32),
            ("fn f() { let v: i64 = x; }", SimpleTypeKind::I64),
            ("fn f() { let v: u8 = x; }", SimpleTypeKind::U8),
            ("fn f() { let v: u16 = x; }", SimpleTypeKind::U16),
            ("fn f() { let v: u32 = x; }", SimpleTypeKind::U32),
            ("fn f() { let v: u64 = x; }", SimpleTypeKind::U64),
            ("fn f() { let v: bool = x; }", SimpleTypeKind::Bool),
        ];
        for (src, expected) in cases {
            let arena = lower(src);
            match let_type(&arena) {
                TypeNode::Simple(kind) => {
                    assert_eq!(*kind, expected, "type mismatch for {src:?}");
                }
                other => panic!("expected simple type for {src:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn lowers_unit_type() {
        // The unit type is spelled `()` and lowers to `SimpleTypeKind::Unit`.
        let arena = lower("fn f() -> () { return; }");
        match single_def(&arena) {
            Def::Function { returns, .. } => {
                let returns = returns.expect("declared return type");
                assert!(matches!(
                    arena[returns].kind,
                    TypeNode::Simple(SimpleTypeKind::Unit)
                ));
            }
            other => panic!("expected function, got {other:?}"),
        }
    }

    #[test]
    fn lowers_array_type() {
        let arena = lower("fn f() { let v: [i32; 3] = x; }");
        match let_type(&arena) {
            TypeNode::Array { element, size } => {
                assert!(matches!(
                    arena[*element].kind,
                    TypeNode::Simple(SimpleTypeKind::I32)
                ));
                assert!(matches!(arena[*size].kind, Expr::NumberLiteral { .. }));
            }
            other => panic!("expected array type, got {other:?}"),
        }
    }

    #[test]
    fn lowers_fn_type() {
        let arena = lower("fn f() { let v: fn(i32) -> i32 = h; }");
        match let_type(&arena) {
            TypeNode::Function { params, ret } => {
                // `fn(...)` params are not lowered (a parity quirk); only the
                // return type is.
                assert!(params.is_empty());
                let ret = ret.expect("declared return type");
                assert!(matches!(
                    arena[ret].kind,
                    TypeNode::Simple(SimpleTypeKind::I32)
                ));
            }
            other => panic!("expected function type, got {other:?}"),
        }
    }

    #[test]
    fn lowers_generic_type() {
        let arena = lower("fn f() { let v: Vec i32' = x; }");
        match let_type(&arena) {
            TypeNode::Generic { base, params } => {
                assert_eq!(arena.ident_name(*base), "Vec");
                assert_eq!(params.len(), 1);
                assert_eq!(arena.ident_name(params[0]), "i32");
            }
            other => panic!("expected generic type, got {other:?}"),
        }
    }

    #[test]
    fn lowers_qualified_type() {
        let arena = lower("fn f() { let v: ns::Type = x; }");
        match let_type(&arena) {
            TypeNode::Qualified { alias, name } => {
                assert_eq!(arena.ident_name(*alias), "ns");
                assert_eq!(arena.ident_name(*name), "Type");
            }
            other => panic!("expected qualified type, got {other:?}"),
        }
    }

    #[test]
    fn lowers_custom_type() {
        let arena = lower("fn f() { let v: MyType = x; }");
        match let_type(&arena) {
            TypeNode::Custom(id) => assert_eq!(arena.ident_name(*id), "MyType"),
            other => panic!("expected custom type, got {other:?}"),
        }
    }

    // -- Location parity -----------------------------------------------------

    #[test]
    fn number_literal_location_is_sensible() {
        // `42` sits at byte offset 9 in `fn f() { 42; }`; offsets must form a
        // valid non-empty span on 1-based line 1.
        let src = "fn f() { 42; }";
        let arena = lower(src);
        let stmts = &arena[fn_body(&arena)].stmts;
        let Stmt::Expr(expr_id) = arena[stmts[0]].kind else {
            panic!("expected expression statement");
        };
        let loc = arena[expr_id].location;
        assert_eq!(loc.start_line, 1);
        assert!(loc.start_column >= 1);
        assert!(loc.offset_start < loc.offset_end);
        assert_eq!(
            &src[loc.offset_start as usize..loc.offset_end as usize],
            "42"
        );
    }

    #[test]
    fn source_file_location_spans_whole_input() {
        let src = "fn f() { }\n";
        let arena = lower(src);
        let files: Vec<_> = arena.source_files().collect();
        let loc = files[0].location;
        assert_eq!(loc.offset_start, 0);
        assert_eq!(loc.offset_end as usize, src.len());
        assert_eq!(loc.start_line, 1);
        assert_eq!(loc.start_column, 1);
    }

    // -- Resilience: error-recovery / fallback arms --------------------------

    #[test]
    fn parse_recovers_from_member_access_without_name() {
        let result = parse("fn f() { a. }");
        // No panic, and the missing member name is reported.
        assert!(!result.errors.is_empty());
        // The arena still contains the function definition.
        assert_eq!(function_count(&result.arena), 1);
    }

    #[test]
    fn parse_recovers_from_qualified_name_without_name() {
        let result = parse("fn f() { a:: }");
        assert!(!result.errors.is_empty());
        assert_eq!(function_count(&result.arena), 1);
    }

    #[test]
    fn parse_recovers_from_array_type_without_element() {
        let result = parse("fn f() { let x: [ = 0; }");
        assert!(!result.errors.is_empty());
        assert_eq!(function_count(&result.arena), 1);
    }

    #[test]
    fn parse_recovers_from_truncated_operand() {
        let result = parse("fn f() { a +");
        assert!(!result.errors.is_empty());
    }

    /// The number of top-level function definitions across all source files.
    fn function_count(arena: &AstArena) -> usize {
        let ids: Vec<DefId> = arena.function_def_ids();
        ids.len()
    }
}
