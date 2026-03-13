//! Typed Context - Type Annotation Storage for AST Nodes
//!
//! This module provides [`TypedContext`], the central data structure that stores
//! type information for all value expressions in the AST after type checking completes.

use crate::{
    symbol_table::{StructInfo, SymbolTable},
    type_info::{NumberType, TypeInfo, TypeInfoKind},
};

use inference_ast::{
    arena::AstArena,
    ids::{DefId, NodeId},
    nodes::SourceFileData,
};
use rustc_hash::FxHashMap;

/// Central store produced by type checking.
///
/// `TypedContext` combines the original parsed [`AstArena`] with a map from
/// AST node IDs to their inferred [`TypeInfo`] values and the populated
/// [`SymbolTable`]. It is the primary output of
/// [`TypeCheckerBuilder::build_typed_context`](crate::TypeCheckerBuilder::build_typed_context)
/// and the primary input to subsequent compiler phases such as WASM code generation.
#[derive(Default)]
pub struct TypedContext {
    pub(crate) symbol_table: SymbolTable,
    node_types: FxHashMap<NodeId, TypeInfo>,
    arena: AstArena,
}

impl TypedContext {
    pub(crate) fn new(arena: AstArena) -> Self {
        Self {
            symbol_table: SymbolTable::default(),
            node_types: FxHashMap::default(),
            arena,
        }
    }

    /// Returns a reference to the underlying AST arena.
    #[must_use]
    pub fn arena(&self) -> &AstArena {
        &self.arena
    }

    /// Returns all source files in the arena.
    pub fn source_files(&self) -> impl ExactSizeIterator<Item = &SourceFileData> + '_ {
        self.arena.source_files()
    }

    /// Returns all function definition IDs across all source files.
    #[must_use = "returns function definition IDs without side effects"]
    pub fn function_def_ids(&self) -> Vec<DefId> {
        self.arena.function_def_ids()
    }

    /// Checks if a node has type `i32`.
    #[must_use = "this is a pure type check with no side effects"]
    pub fn is_node_i32(&self, node_id: NodeId) -> bool {
        self.is_node_type(node_id, |kind| {
            matches!(kind, TypeInfoKind::Number(NumberType::I32))
        })
    }

    /// Checks if a node has type `i64`.
    #[must_use = "this is a pure type check with no side effects"]
    pub fn is_node_i64(&self, node_id: NodeId) -> bool {
        self.is_node_type(node_id, |kind| {
            matches!(kind, TypeInfoKind::Number(NumberType::I64))
        })
    }

    /// Gets the type information for a given node ID.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn get_node_typeinfo(&self, node_id: NodeId) -> Option<TypeInfo> {
        self.node_types.get(&node_id).cloned()
    }

    /// Looks up a struct by name and returns its type information.
    ///
    /// Returns `None` if no struct with the given name exists in the current scope.
    /// Fields in the returned [`StructInfo`] are in declaration order.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn lookup_struct(&self, name: &str) -> Option<StructInfo> {
        self.symbol_table.lookup_struct(name)
    }

    pub(crate) fn set_node_typeinfo(&mut self, node_id: NodeId, type_info: TypeInfo) {
        self.node_types.insert(node_id, type_info);
    }

    fn is_node_type<T>(&self, node_id: NodeId, type_checker: T) -> bool
    where
        T: Fn(&TypeInfoKind) -> bool,
    {
        if let Some(type_info) = self.get_node_typeinfo(node_id) {
            type_checker(&type_info.kind)
        } else {
            false
        }
    }
}


