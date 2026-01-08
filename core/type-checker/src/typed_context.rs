//! Typed Context
//!
//! This module provides [`TypedContext`], the central data structure that stores
//! type information for AST nodes after type checking completes.
//!
//! The `TypedContext` associates AST node IDs (u32) with their inferred [`TypeInfo`].
//! It wraps the original [`Arena`] to provide both the AST structure and type annotations.

use std::rc::Rc;

use crate::{
    symbol_table::SymbolTable,
    type_info::{NumberType, TypeInfo, TypeInfoKind},
};
use inference_ast::{
    arena::Arena,
    nodes::{AstNode, Expression, Location, SourceFile},
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

    #[must_use = "returns source files without side effects"]
    pub fn source_files(&self) -> Vec<Rc<SourceFile>> {
        self.arena.source_files()
    }

    #[must_use = "returns filtered nodes without side effects"]
    pub fn filter_nodes<T: Fn(&AstNode) -> bool>(&self, fn_predicate: T) -> Vec<AstNode> {
        self.arena.filter_nodes(fn_predicate)
    }

    #[must_use = "this is a pure type check with no side effects"]
    pub fn is_node_i32(&self, node_id: u32) -> bool {
        self.is_node_type(node_id, |kind| {
            matches!(kind, TypeInfoKind::Number(NumberType::I32))
        })
    }

    #[must_use = "this is a pure type check with no side effects"]
    pub fn is_node_i64(&self, node_id: u32) -> bool {
        self.is_node_type(node_id, |kind| {
            matches!(kind, TypeInfoKind::Number(NumberType::I64))
        })
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub fn get_node_typeinfo(&self, node_id: u32) -> Option<TypeInfo> {
        self.node_types.get(&node_id).cloned()
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
            .filter_nodes(|node| {
                matches!(node, AstNode::Expression(expr) if Self::is_value_expression(expr))
            })
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
