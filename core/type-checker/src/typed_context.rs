//! Typed Context - Type Annotation Storage for AST Nodes
//!
//! This module provides [`TypedContext`], the central data structure that stores
//! type information for all value expressions in the AST after type checking completes.

use crate::{
    symbol_table::{EnumInfo, StructInfo, SymbolTable},
    type_info::{NumberType, TypeInfo, TypeInfoKind},
};

use inference_ast::{
    arena::AstArena,
    ids::{DefId, NodeId},
    nodes::{SourceFileData, Visibility},
};
use rustc_hash::FxHashMap;

/// Public metadata about a method defined on a type.
///
/// This is the public projection of the type-checker's internal
/// `MethodInfo`. It exposes only the information that downstream phases
/// (such as WASM code generation, IDE features, and analysis) need:
/// parameter types, return type, whether the method takes `self`, and
/// its visibility.
///
/// Obtained via [`TypedContext::lookup_method`].
#[derive(Debug, Clone)]
pub struct MethodMetadata {
    pub name: String,
    /// Parameter types, excluding `self`. See `has_self` for whether the method takes a receiver.
    pub param_types: Vec<TypeInfo>,
    pub return_type: TypeInfo,
    pub has_self: bool,
    pub visibility: Visibility,
}

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

    /// Looks up a struct by name across the root scope and every spec scope.
    ///
    /// Returns `None` if no struct with the given name exists. Fields in the
    /// returned [`StructInfo`] are in declaration order.
    ///
    /// Post-type-check consumers (analysis, codegen) walk the AST into spec
    /// bodies independently of the symbol table's scope cursor, so this
    /// lookup is scope-agnostic. `register_types` does not currently recurse
    /// into `Def::Module.defs`, so module-nested definitions are absent from
    /// the symbol table; this helper sees only root-scope and spec-scope items.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn lookup_struct(&self, name: &str) -> Option<StructInfo> {
        self.symbol_table.lookup_struct_anywhere(name)
    }

    /// Looks up an enum by name across the root scope and every spec scope.
    ///
    /// Returns `None` if no enum with the given name exists. Variants in the
    /// returned [`EnumInfo`] are in declaration order, which determines their
    /// zero-based integer tag for WASM codegen. `register_types` does not
    /// currently recurse into `Def::Module.defs`, so module-nested definitions
    /// are absent from the symbol table; this helper sees only root-scope and
    /// spec-scope items.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn lookup_enum(&self, name: &str) -> Option<EnumInfo> {
        self.symbol_table.lookup_enum_anywhere(name)
    }

    /// Registers a struct definition in the type context for testing.
    ///
    /// Intended for unit tests in downstream crates (e.g. `wasm-codegen`) that
    /// need a populated `TypedContext` without running the full type-checker.
    #[cfg(feature = "test-utils")]
    #[doc(hidden)]
    pub fn register_test_struct(
        &mut self,
        name: &str,
        fields: &[(String, TypeInfo, Visibility)],
    ) -> anyhow::Result<()> {
        self.symbol_table
            .register_struct(name, fields, vec![], Visibility::Public)
    }

    /// Registers an enum definition in the type context for testing.
    ///
    /// Intended for unit tests in downstream crates (e.g. `wasm-codegen`) that
    /// need a populated `TypedContext` without running the full type-checker.
    #[cfg(feature = "test-utils")]
    #[doc(hidden)]
    pub fn register_test_enum(
        &mut self,
        name: &str,
        variants: &[&str],
    ) -> anyhow::Result<()> {
        self.symbol_table
            .register_enum(name, variants, Visibility::Public)
    }

    /// Looks up a method on the given type by name and returns its metadata.
    ///
    /// Returns `None` if no method with the given name exists on the type.
    /// The returned [`MethodMetadata`] contains parameter types (excluding
    /// `self`), return type, whether the method takes `self`, and visibility.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn lookup_method(&self, type_name: &str, method_name: &str) -> Option<MethodMetadata> {
        self.symbol_table
            .lookup_method(type_name, method_name)
            .map(|info| MethodMetadata {
                name: info.signature.name.clone(),
                param_types: info.signature.param_types.clone(),
                return_type: info.signature.return_type.clone(),
                has_self: info.has_self,
                visibility: info.visibility,
            })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol_table::FuncInfo;
    use crate::type_info::{NumberType, TypeInfo, TypeInfoKind};
    use inference_ast::nodes::Visibility;

    fn make_i32_type() -> TypeInfo {
        TypeInfo {
            kind: TypeInfoKind::Number(NumberType::I32),
            type_params: vec![],
        }
    }

    fn make_typed_context_with_method(
        type_name: &str,
        method_name: &str,
        param_types: Vec<TypeInfo>,
        return_type: TypeInfo,
        visibility: Visibility,
        has_self: bool,
    ) -> TypedContext {
        let arena = AstArena::default();
        let mut ctx = TypedContext::new(arena);
        let sig = FuncInfo {
            name: method_name.to_string(),
            type_params: vec![],
            param_types,
            return_type,
            visibility: visibility.clone(),
            definition_scope_id: 0,
        };
        ctx.symbol_table
            .register_method(type_name, sig, visibility, has_self)
            .expect("register_method should succeed");
        ctx
    }

    #[test]
    fn lookup_method_returns_none_for_missing_method() {
        let ctx = make_typed_context_with_method(
            "Point",
            "get_x",
            vec![],
            make_i32_type(),
            Visibility::Public,
            true,
        );
        assert!(ctx.lookup_method("Point", "nonexistent").is_none());
    }

    #[test]
    fn lookup_method_returns_none_for_missing_type() {
        let ctx = make_typed_context_with_method(
            "Point",
            "get_x",
            vec![],
            make_i32_type(),
            Visibility::Public,
            true,
        );
        assert!(ctx.lookup_method("NoSuchType", "get_x").is_none());
    }

    #[test]
    fn lookup_method_returns_instance_method_metadata() {
        let ctx = make_typed_context_with_method(
            "Point",
            "get_x",
            vec![],
            make_i32_type(),
            Visibility::Public,
            true,
        );
        let meta = ctx
            .lookup_method("Point", "get_x")
            .expect("method should be found");
        assert_eq!(meta.name, "get_x");
        assert!(meta.param_types.is_empty());
        assert!(matches!(
            meta.return_type.kind,
            TypeInfoKind::Number(NumberType::I32)
        ));
        assert!(meta.has_self);
        assert!(matches!(meta.visibility, Visibility::Public));
    }

    #[test]
    fn lookup_method_returns_associated_function_metadata() {
        let params = vec![make_i32_type(), make_i32_type()];
        let ret = TypeInfo {
            kind: TypeInfoKind::Custom("Point".to_string()),
            type_params: vec![],
        };
        let ctx = make_typed_context_with_method(
            "Point",
            "new",
            params,
            ret,
            Visibility::Public,
            false,
        );
        let meta = ctx
            .lookup_method("Point", "new")
            .expect("method should be found");
        assert_eq!(meta.name, "new");
        assert_eq!(meta.param_types.len(), 2);
        assert!(!meta.has_self);
        assert!(matches!(
            meta.return_type.kind,
            TypeInfoKind::Custom(ref name) if name == "Point"
        ));
    }

    #[test]
    fn lookup_method_preserves_visibility() {
        let ctx = make_typed_context_with_method(
            "Counter",
            "internal_helper",
            vec![],
            TypeInfo::default(),
            Visibility::Private,
            true,
        );
        let meta = ctx
            .lookup_method("Counter", "internal_helper")
            .expect("method should be found");
        assert!(matches!(meta.visibility, Visibility::Private));
    }

    #[test]
    fn lookup_method_multiple_methods_on_same_type() {
        let arena = AstArena::default();
        let mut ctx = TypedContext::new(arena);

        let sig_get_x = FuncInfo {
            name: "get_x".to_string(),
            type_params: vec![],
            param_types: vec![],
            return_type: make_i32_type(),
            visibility: Visibility::Public,
            definition_scope_id: 0,
        };
        ctx.symbol_table
            .register_method("Point", sig_get_x, Visibility::Public, true)
            .expect("register get_x should succeed");

        let sig_get_y = FuncInfo {
            name: "get_y".to_string(),
            type_params: vec![],
            param_types: vec![],
            return_type: TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I64),
                type_params: vec![],
            },
            visibility: Visibility::Public,
            definition_scope_id: 0,
        };
        ctx.symbol_table
            .register_method("Point", sig_get_y, Visibility::Public, true)
            .expect("register get_y should succeed");

        let meta_x = ctx
            .lookup_method("Point", "get_x")
            .expect("get_x should be found");
        assert_eq!(meta_x.name, "get_x");
        assert!(matches!(
            meta_x.return_type.kind,
            TypeInfoKind::Number(NumberType::I32)
        ));

        let meta_y = ctx
            .lookup_method("Point", "get_y")
            .expect("get_y should be found");
        assert_eq!(meta_y.name, "get_y");
        assert!(matches!(
            meta_y.return_type.kind,
            TypeInfoKind::Number(NumberType::I64)
        ));
    }

    #[test]
    fn lookup_method_same_name_on_different_types() {
        let arena = AstArena::default();
        let mut ctx = TypedContext::new(arena);

        let sig_point = FuncInfo {
            name: "get_x".to_string(),
            type_params: vec![],
            param_types: vec![],
            return_type: make_i32_type(),
            visibility: Visibility::Public,
            definition_scope_id: 0,
        };
        ctx.symbol_table
            .register_method("Point", sig_point, Visibility::Public, true)
            .expect("register Point::get_x should succeed");

        let sig_vector = FuncInfo {
            name: "get_x".to_string(),
            type_params: vec![],
            param_types: vec![],
            return_type: TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I64),
                type_params: vec![],
            },
            visibility: Visibility::Private,
            definition_scope_id: 0,
        };
        ctx.symbol_table
            .register_method("Vector", sig_vector, Visibility::Private, false)
            .expect("register Vector::get_x should succeed");

        let meta_point = ctx
            .lookup_method("Point", "get_x")
            .expect("Point::get_x should be found");
        assert_eq!(meta_point.name, "get_x");
        assert!(matches!(
            meta_point.return_type.kind,
            TypeInfoKind::Number(NumberType::I32)
        ));
        assert!(meta_point.has_self);
        assert!(matches!(meta_point.visibility, Visibility::Public));

        let meta_vector = ctx
            .lookup_method("Vector", "get_x")
            .expect("Vector::get_x should be found");
        assert_eq!(meta_vector.name, "get_x");
        assert!(matches!(
            meta_vector.return_type.kind,
            TypeInfoKind::Number(NumberType::I64)
        ));
        assert!(!meta_vector.has_self);
        assert!(matches!(meta_vector.visibility, Visibility::Private));
    }
}
