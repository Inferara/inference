//! Typed Context - Type Annotation Storage for AST Nodes
//!
//! This module provides [`TypedContext`], the central data structure that stores
//! type information for all value expressions in the AST after type checking completes.

use crate::{
    symbol_table::SymbolTable,
    type_info::{NumberType, TypeInfo, TypeInfoKind},
};

use inference_ast::{
    arena::AstArena,
    ids::{DefId, NodeId},
    nodes::{Location, SourceFileData, Stmt},
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
    #[must_use = "returns source files without side effects"]
    pub fn source_files(&self) -> &[SourceFileData] {
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

    pub(crate) fn set_node_typeinfo(&mut self, node_id: NodeId, type_info: TypeInfo) {
        self.node_types.insert(node_id, type_info);
    }

    /// Walks the parent chain from `node_id` up to the nearest `Stmt::VarDef`
    /// and returns the variable name.
    ///
    /// Used by the codegen pass to find the enclosing variable name for array
    /// literals and uzumaki expressions.
    #[must_use = "returns the enclosing variable name without side effects"]
    pub fn find_enclosing_variable_name(&self, node_id: NodeId) -> Option<String> {
        let mut current = node_id;
        loop {
            if let NodeId::Stmt(stmt_id) = current {
                if let Stmt::VarDef { name, .. } = &self.arena[stmt_id].kind {
                    return Some(self.arena[*name].name.clone());
                }
            }
            current = self.arena.find_parent(current)?;
        }
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

/// Describes a value expression that has no [`TypeInfo`] entry after type checking.
#[derive(Debug)]
pub struct MissingExpressionType {
    /// AST node ID of the untyped expression.
    pub node_id: NodeId,
    /// Human-readable name of the expression variant (e.g. `"Binary"`, `"FunctionCall"`).
    pub kind: String,
    /// Source location of the expression, for diagnostic output.
    pub location: Location,
}
