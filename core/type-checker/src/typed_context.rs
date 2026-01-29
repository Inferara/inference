//! Typed Context - Type Annotation Storage for AST Nodes
//!
//! This module provides [`TypedContext`], the central data structure that stores
//! type information for all value expressions in the AST after type checking completes.
//!
//! ## Overview
//!
//! The [`TypedContext`] serves as the bridge between the AST and the type checker,
//! providing:
//! - Storage for inferred type information keyed by AST node ID
//! - Access to the original AST arena for node traversal
//! - Convenience methods for common type queries
//! - Symbol table with type and function definitions
//!
//! ## Architecture
//!
//! ```text
//! TypedContext
//! ├─ Arena (original AST)
//! │  └─ Source files with AST nodes
//! ├─ node_types: HashMap<NodeID, TypeInfo>
//! │  └─ Type annotations for value expressions
//! └─ SymbolTable
//!    ├─ Type definitions (structs, enums, specs)
//!    ├─ Function signatures
//!    └─ Scope hierarchy
//! ```
//!
//! ## Node ID to Type Mapping
//!
//! The `TypedContext` associates AST node IDs (`u32`) with their inferred [`TypeInfo`]:
//!
//! ```ignore
//! // Get type info for a node
//! if let Some(type_info) = typed_context.get_node_typeinfo(node_id) {
//!     println!("Node {} has type: {}", node_id, type_info);
//! }
//! ```
//!
//! **Important**: Only value expressions have type information. Structural nodes like
//! type annotations (`Expression::Type`), names in declarations, and certain identifiers
//! (like function names, struct names, field names) are not value expressions and will
//! not have entries in `node_types`.
//!
//! ## Value vs. Structural Expressions
//!
//! The type checker distinguishes between:
//!
//! **Value Expressions** (have TypeInfo):
//! - Binary operations: `a + b`, `x == y`
//! - Function calls: `foo(1, 2)`
//! - Struct literals: `Point { x: 10, y: 20 }`
//! - Array literals: `[1, 2, 3]`
//! - Member access: `p.x`
//! - Array indexing: `arr[0]`
//! - Variable references in value positions
//!
//! **Structural Expressions** (no TypeInfo):
//! - Type annotations: `fn foo() -> i32` (the `i32` is structural)
//! - Names in declarations: `let x: i32` (the identifier `x` is structural)
//! - Function/struct/field names (not references to values)
//!
//! ## Query Methods
//!
//! The [`TypedContext`] provides several query methods:
//!
//! - [`get_node_typeinfo`](TypedContext::get_node_typeinfo) - Get type info for a node
//! - [`is_node_i32`](TypedContext::is_node_i32) - Check if node is i32
//! - [`is_node_i64`](TypedContext::is_node_i64) - Check if node is i64
//! - [`filter_nodes`](TypedContext::filter_nodes) - Find nodes matching predicate
//! - [`source_files`](TypedContext::source_files) - Get all source files
//! - [`functions`](TypedContext::functions) - Get all function definitions
//!
//! ## Arena Integration
//!
//! The `TypedContext` wraps the original AST [`Arena`] to provide both structure
//! and type annotations. This design ensures:
//! - Node IDs remain consistent between AST and type info
//! - No need to copy or transform the AST after type checking
//! - Direct access to AST structure for traversal and queries

use std::rc::Rc;

use crate::{
    symbol_table::SymbolTable,
    type_info::{NumberType, TypeInfo, TypeInfoKind},
};
use inference_ast::{
    arena::Arena,
    nodes::{AstNode, Expression, FunctionDefinition, Location, SourceFile},
};
use rustc_hash::FxHashMap;

#[derive(Default)]
pub struct TypedContext {
    pub(crate) symbol_table: SymbolTable,
    node_types: FxHashMap<u32, TypeInfo>,
    arena: Arena,
}

impl TypedContext {
    pub(crate) fn new(arena: Arena) -> Self {
        Self {
            symbol_table: SymbolTable::default(),
            node_types: FxHashMap::default(),
            arena,
        }
    }

    /// Returns all source files in the arena.
    ///
    /// Each source file contains its definitions (functions, structs, enums, etc.)
    /// and can be traversed to access the AST structure.
    ///
    /// # Example
    ///
    /// ```ignore
    /// for source_file in typed_context.source_files() {
    ///     println!("File: {}", source_file.name);
    ///     for definition in &source_file.definitions {
    ///         // Process each definition
    ///     }
    /// }
    /// ```
    #[must_use = "returns source files without side effects"]
    pub fn source_files(&self) -> Vec<Rc<SourceFile>> {
        self.arena.source_files()
    }

    /// Returns all function definitions across all source files.
    ///
    /// This is a convenience method that collects functions from all source files
    /// without needing to iterate manually.
    ///
    /// # Example
    ///
    /// ```ignore
    /// for func in typed_context.functions() {
    ///     println!("Function: {}", func.name());
    ///     if let Some(return_type_node) = &func.returns {
    ///         let return_type = typed_context.get_node_typeinfo(return_type_node.id());
    ///         println!("  Returns: {:?}", return_type);
    ///     }
    /// }
    /// ```
    #[must_use = "returns function definitions without side effects"]
    pub fn functions(&self) -> Vec<Rc<FunctionDefinition>> {
        self.arena.functions()
    }

    /// Filters AST nodes using a predicate function.
    ///
    /// This method traverses all nodes in the arena and returns those that match
    /// the provided predicate. Useful for finding specific node types or patterns.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Find all binary operations
    /// let binary_ops = typed_context.filter_nodes(|node| {
    ///     matches!(node, AstNode::Expression(Expression::Binary(_)))
    /// });
    ///
    /// // Find all function calls
    /// let calls = typed_context.filter_nodes(|node| {
    ///     matches!(node, AstNode::Expression(Expression::FunctionCall(_)))
    /// });
    ///
    /// // Find numeric literals over 100
    /// let large_numbers = typed_context.filter_nodes(|node| {
    ///     if let AstNode::Expression(Expression::Literal(Literal::Number(n))) = node {
    ///         n.value.parse::<i32>().unwrap_or(0) > 100
    ///     } else {
    ///         false
    ///     }
    /// });
    /// ```
    #[must_use = "returns filtered nodes without side effects"]
    pub fn filter_nodes<T: Fn(&AstNode) -> bool>(&self, fn_predicate: T) -> Vec<AstNode> {
        self.arena.filter_nodes(fn_predicate)
    }

    /// Checks if a node has type `i32`.
    ///
    /// This is a convenience method for the common case of checking if a node
    /// is a 32-bit signed integer.
    ///
    /// Returns `false` if the node has no type info or has a different type.
    ///
    /// # Example
    ///
    /// ```ignore
    /// if typed_context.is_node_i32(node_id) {
    ///     // Generate i32-specific code
    /// }
    /// ```
    #[must_use = "this is a pure type check with no side effects"]
    pub fn is_node_i32(&self, node_id: u32) -> bool {
        self.is_node_type(node_id, |kind| {
            matches!(kind, TypeInfoKind::Number(NumberType::I32))
        })
    }

    /// Checks if a node has type `i64`.
    ///
    /// This is a convenience method for the common case of checking if a node
    /// is a 64-bit signed integer.
    ///
    /// Returns `false` if the node has no type info or has a different type.
    ///
    /// # Example
    ///
    /// ```ignore
    /// if typed_context.is_node_i64(node_id) {
    ///     // Generate i64-specific code
    /// }
    /// ```
    #[must_use = "this is a pure type check with no side effects"]
    pub fn is_node_i64(&self, node_id: u32) -> bool {
        self.is_node_type(node_id, |kind| {
            matches!(kind, TypeInfoKind::Number(NumberType::I64))
        })
    }

    /// Gets the type information for a given node ID.
    ///
    /// Returns `Some(TypeInfo)` if the node is a value expression with type information,
    /// or `None` if:
    /// - The node is structural (type annotation, name in declaration, etc.)
    /// - The node doesn't exist
    /// - Type checking failed for this node
    ///
    /// # Example
    ///
    /// ```ignore
    /// match typed_context.get_node_typeinfo(node_id) {
    ///     Some(type_info) => {
    ///         println!("Type: {}", type_info);
    ///         if type_info.is_number() {
    ///             // Handle numeric type
    ///         }
    ///     }
    ///     None => {
    ///         // Node has no type info (structural or error)
    ///     }
    /// }
    /// ```
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn get_node_typeinfo(&self, node_id: u32) -> Option<TypeInfo> {
        self.node_types.get(&node_id).cloned()
    }

    /// Gets the parent node of a given node ID.
    ///
    /// Returns `Some(AstNode)` if the node has a parent, or `None` if:
    /// - The node is a root node (no parent)
    /// - The node doesn't exist
    ///
    /// Useful for traversing up the AST tree to understand context.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Walk up the tree to find enclosing function
    /// let mut current_id = node_id;
    /// loop {
    ///     match typed_context.get_parent_node(current_id) {
    ///         Some(AstNode::Definition(Definition::Function(func))) => {
    ///             println!("Found enclosing function: {}", func.name());
    ///             break;
    ///         }
    ///         Some(parent) => {
    ///             current_id = parent.id();
    ///         }
    ///         None => break,
    ///     }
    /// }
    /// ```
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn get_parent_node(&self, id: u32) -> Option<AstNode> {
        self.arena
            .find_parent_node(id)
            .and_then(|parent_id| self.arena.find_node(parent_id))
    }

    pub(crate) fn set_node_typeinfo(&mut self, node_id: u32, type_info: TypeInfo) {
        self.node_types.insert(node_id, type_info);
    }

    fn is_node_type<T>(&self, node_id: u32, type_checker: T) -> bool
    where
        T: Fn(&TypeInfoKind) -> bool,
    {
        if let Some(type_info) = self.get_node_typeinfo(node_id) {
            type_checker(&type_info.kind)
        } else {
            false
        }
    }

    /// Verifies that all value Expression nodes in the arena have corresponding TypeInfo entries.
    ///
    /// Returns a list of expressions that are missing from `node_types`.
    /// An empty list indicates all value expressions have been typed.
    ///
    /// Note: Excludes structural expressions (Expression::Type for type annotations and
    /// Expression::Identifier which can be either structural names or value references)
    /// since the type checker only visits expressions in value positions.
    #[must_use = "returns list of missing expression types for verification"]
    #[track_caller]
    pub fn find_untyped_expressions(&self) -> Vec<MissingExpressionType> {
        self.arena
            .filter_nodes(
                |node| matches!(node, AstNode::Expression(expr) if Self::is_value_expression(expr)),
            )
            .into_iter()
            .filter_map(|node| {
                if let AstNode::Expression(expr) = &node {
                    let id = expr.id();
                    if !self.node_types.contains_key(&id) {
                        return Some(MissingExpressionType {
                            id,
                            kind: Self::expression_kind_name(expr),
                            location: expr.location(),
                        });
                    }
                }
                None
            })
            .collect()
    }

    /// Checks if an expression is a value expression that should have TypeInfo.
    ///
    /// Excludes structural expressions that are not value computations:
    /// - Expression::Type (type annotations in signatures and declarations)
    /// - Expression::Identifier (can be structural names like function/struct/field names,
    ///   which are stored in the arena but not all are visited by the type inference pass.
    ///   Value identifier references DO get type info when processed by infer_expression.)
    /// - Expression::Literal that may be structural (like array sizes in type annotations
    ///   `[i32; 5]` where `5` is a structural size, not a computed value)
    ///
    /// Note: Value identifiers and literals DO get type info when processed by `infer_expression`.
    /// The exclusions here avoid false positives from structural elements stored in the arena
    /// that are never passed to `infer_expression`.
    ///
    /// TODO: A more precise approach would be to track value vs structural positions during
    /// AST construction or type checking, rather than excluding entire expression kinds.
    fn is_value_expression(expr: &Expression) -> bool {
        !matches!(
            expr,
            Expression::Type(_) | Expression::Identifier(_) | Expression::Literal(_)
        )
    }

    fn expression_kind_name(expr: &Expression) -> String {
        match expr {
            Expression::ArrayIndexAccess(_) => "ArrayIndexAccess",
            Expression::Binary(_) => "Binary",
            Expression::MemberAccess(_) => "MemberAccess",
            Expression::TypeMemberAccess(_) => "TypeMemberAccess",
            Expression::FunctionCall(_) => "FunctionCall",
            Expression::Struct(_) => "Struct",
            Expression::PrefixUnary(_) => "PrefixUnary",
            Expression::Parenthesized(_) => "Parenthesized",
            Expression::Literal(_) => "Literal",
            Expression::Identifier(_) => "Identifier",
            Expression::Type(_) => "Type",
            Expression::Uzumaki(_) => "Uzumaki",
        }
        .to_string()
    }
}

/// Information about an expression missing its type after type checking.
#[derive(Debug)]
pub struct MissingExpressionType {
    pub id: u32,
    pub kind: String,
    pub location: Location,
}
