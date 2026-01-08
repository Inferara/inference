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
    nodes::{AstNode, SourceFile},
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
}
