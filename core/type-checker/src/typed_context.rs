//! Typed Context - Type Annotation Storage for AST Nodes
//!
//! This module provides [`TypedContext`], the central data structure that stores
//! type information for all value expressions in the AST after type checking completes.

use crate::{
    symbol_table::{EnumInfo, ExternOrigin, StructInfo, SymbolTable},
    type_info::{NumberType, TypeInfo, TypeInfoKind},
};

use inference_ast::{
    arena::AstArena,
    ids::{DefId, ExprId, NodeId},
    nodes::{SourceFileData, Visibility},
};
use rustc_hash::FxHashMap;

/// Builds the file-local canonical key for a bare type name in the file whose
/// module path is `module_path`: the `::`-joined module path followed by the
/// name, or the bare name for the entry file (empty module path). This mirrors
/// how the symbol table keys a type by its enclosing file.
fn file_local_key(bare_name: &str, module_path: &[String]) -> String {
    if module_path.is_empty() {
        bare_name.to_string()
    } else {
        format!("{}::{bare_name}", module_path.join("::"))
    }
}

/// The defining-file identity of a resolved function/method call target.
///
/// Type checking resolves every call — including a cross-file path
/// (`math::arith::add`) that crosses one or more `pub use` re-exports — to the
/// function's actual defining file. Code generation needs that defining file to
/// build the function's file-qualified flat WASM name, but the source-level call
/// path can differ from the defining path because of re-export indirection, and
/// the inferred node type only records the source path string. This struct
/// carries the resolved identity forward so codegen reproduces the same
/// file-qualified name the registration pass assigned, without re-walking the
/// scope tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallTarget {
    /// Source-root-relative segments of the callee's defining file. Empty for a
    /// callee defined in the entry file (its WASM name stays unqualified). For a
    /// method this is the *struct's* defining file.
    pub module_path: Vec<String>,
    /// The callee's bare name (the final path segment), e.g. `add` or `new`.
    pub name: String,
    /// `Some(struct_name)` when the target is an associated function reached
    /// through a namespace (`geo::Point::new`): the call lowers to the struct's
    /// file-qualified method, not a free function. `None` for a free function.
    pub receiver_struct: Option<String>,
}

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
    /// Structs indexed by canonical key (`<defining-file-path>::<name>`, bare
    /// for the entry file). Built once after type checking; the single source of
    /// truth for unambiguous struct-layout resolution in later phases when two
    /// files define a same-named struct.
    structs_by_key: FxHashMap<String, StructInfo>,
    /// Enums indexed by canonical key. Mirrors [`Self::structs_by_key`].
    enums_by_key: FxHashMap<String, EnumInfo>,
    /// Topological order of top-level `const` and `type` alias definitions across
    /// all files, dependencies first. Empty when there are no such definitions.
    /// A later phase emits constant values in this order so a const that reads
    /// another const sees a computed value; the order is well-defined because a
    /// value cycle is rejected during type checking.
    definition_order: Vec<DefId>,
    /// The defining-file identity of each resolved function/method call,
    /// keyed by the call's *function* expression id. Populated during type
    /// checking for calls that resolve to a known function. Code generation
    /// consumes it to file-qualify the WASM call target across re-export
    /// indirection; calls absent from the map (e.g. higher-order, or to an
    /// `external fn` import) fall back to the existing bare-name resolution.
    resolved_call_targets: FxHashMap<ExprId, CallTarget>,
}

impl TypedContext {
    pub(crate) fn new(arena: AstArena) -> Self {
        Self {
            symbol_table: SymbolTable::default(),
            node_types: FxHashMap::default(),
            arena,
            structs_by_key: FxHashMap::default(),
            enums_by_key: FxHashMap::default(),
            definition_order: Vec::new(),
            resolved_call_targets: FxHashMap::default(),
        }
    }

    /// Records the defining-file identity of a resolved call, keyed by its
    /// function expression id. Called during type checking once a call resolves
    /// to a known function or method.
    pub(crate) fn set_call_target(&mut self, function_expr_id: ExprId, target: CallTarget) {
        self.resolved_call_targets.insert(function_expr_id, target);
    }

    /// Returns the resolved defining-file identity of the call whose function
    /// expression is `function_expr_id`, if type checking recorded one.
    ///
    /// Code generation uses this to build the callee's file-qualified WASM name,
    /// reproducing the defining file the registration pass keyed it under even
    /// when the source call path crossed a `pub use` re-export.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn call_target(&self, function_expr_id: ExprId) -> Option<&CallTarget> {
        self.resolved_call_targets.get(&function_expr_id)
    }

    /// Records the topological order of `const`/`type` alias definitions
    /// (dependencies first). Set during type checking once the value graph is
    /// confirmed acyclic.
    pub(crate) fn set_definition_order(&mut self, order: Vec<DefId>) {
        self.definition_order = order;
    }

    /// Topological order of top-level `const` and `type` alias definitions across
    /// all files, dependencies first. A later phase emits constants in this order
    /// so cross-definition reads observe computed values.
    #[must_use = "the ordering is the return value"]
    pub fn definition_order(&self) -> &[DefId] {
        &self.definition_order
    }

    /// Folds the symbol table's structs and enums into canonical-key-indexed
    /// maps. Called once after type checking completes so later phases resolve a
    /// type by its file-qualified canonical key.
    pub(crate) fn build_type_indexes(&mut self) {
        self.structs_by_key = self
            .symbol_table
            .structs_with_canonical_keys()
            .into_iter()
            .collect();
        self.enums_by_key = self
            .symbol_table
            .enums_with_canonical_keys()
            .into_iter()
            .collect();
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

    /// Looks up a struct by its canonical key.
    ///
    /// The canonical key is the struct's file-qualified name
    /// (`lib::arith::Point`), or the bare name for a struct in the entry file
    /// (`Point`). A single-file program defines every struct in the entry file,
    /// so a bare name *is* its canonical key and existing callers keep working
    /// unchanged. Cross-file callers must pass the file-qualified key (available
    /// via [`Self::canonical_struct_key`]) to disambiguate same-named structs.
    ///
    /// Returns `None` if no struct with that canonical key exists. Fields in the
    /// returned [`StructInfo`] are in declaration order.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn lookup_struct(&self, key: &str) -> Option<StructInfo> {
        self.structs_by_key.get(key).cloned()
    }

    /// Looks up an enum by its canonical key. Mirrors [`Self::lookup_struct`].
    ///
    /// Variants in the returned [`EnumInfo`] are in declaration order, which
    /// determines their zero-based integer tag for WASM codegen.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn lookup_enum(&self, key: &str) -> Option<EnumInfo> {
        self.enums_by_key.get(key).cloned()
    }

    /// Looks up the struct named `bare_name` as referenced from the file whose
    /// module path is `from_module_path`, resolving the bare name to its
    /// file-qualified canonical key first.
    ///
    /// This is the multi-file-safe form of [`Self::lookup_struct`]: a bare type
    /// name in a given file may name a different struct than the same bare name
    /// in another file, so code generation passes the file it is emitting for.
    /// For a single-file program the canonical key *is* the bare name, so this
    /// is equivalent to `lookup_struct(bare_name)`.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn lookup_struct_in(
        &self,
        bare_name: &str,
        from_module_path: &[String],
    ) -> Option<StructInfo> {
        if let Some(key) = self.canonical_struct_key(bare_name, from_module_path) {
            return self.lookup_struct(&key);
        }
        // A type defined inside a `spec` block is keyed by its enclosing file
        // but is not reachable by name resolution from the bare file scope, so
        // `canonical_struct_key` cannot find it. Code generating that spec's
        // body knows its file, so try the file-local canonical key directly.
        self.lookup_struct(&file_local_key(bare_name, from_module_path))
    }

    /// Looks up the enum named `bare_name` as referenced from the file whose
    /// module path is `from_module_path`. Mirrors [`Self::lookup_struct_in`].
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn lookup_enum_in(
        &self,
        bare_name: &str,
        from_module_path: &[String],
    ) -> Option<EnumInfo> {
        if let Some(key) = self.canonical_enum_key(bare_name, from_module_path) {
            return self.lookup_enum(&key);
        }
        self.lookup_enum(&file_local_key(bare_name, from_module_path))
    }

    /// Returns the canonical key of the struct named `bare_name` as referenced
    /// from the file whose module path is `from_module_path`.
    ///
    /// This is how a later phase translates a bare struct name appearing in a
    /// given file into the unambiguous canonical key needed to fetch its layout:
    /// the name resolves relative to the referencing file (own file first, then
    /// the program root), so two files each defining a private `Buffer` map to
    /// distinct keys. Returns `None` if the name does not resolve to a struct
    /// visible from that file.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn canonical_struct_key(
        &self,
        bare_name: &str,
        from_module_path: &[String],
    ) -> Option<String> {
        let from_scope = self.symbol_table.find_module_scope(from_module_path)?;
        self.symbol_table
            .resolve_struct_in_scope(bare_name, from_scope)
            .map(|(_, key)| key)
    }

    /// Returns the canonical key of the enum named `bare_name` as referenced
    /// from the file whose module path is `from_module_path`. Mirrors
    /// [`Self::canonical_struct_key`].
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn canonical_enum_key(
        &self,
        bare_name: &str,
        from_module_path: &[String],
    ) -> Option<String> {
        let from_scope = self.symbol_table.find_module_scope(from_module_path)?;
        self.symbol_table
            .resolve_enum_in_scope(bare_name, from_scope)
            .map(|(_, key)| key)
    }

    /// Returns the defining-file module path of the struct named `bare_name` as
    /// referenced from the file whose module path is `from_module_path`.
    ///
    /// A method's mangled WASM name is qualified by its **struct's** defining
    /// file, not the call site's. Code generation resolves the receiver's struct
    /// name and asks for that struct's defining file here. The result is empty
    /// for a struct defined in the entry file (its methods stay unqualified) and
    /// `["lib", "arith"]` for a struct in `lib/arith.inf`. Returns `None` if the
    /// name does not resolve to a struct visible from that file.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn struct_module_path(
        &self,
        bare_name: &str,
        from_module_path: &[String],
    ) -> Option<Vec<String>> {
        let from_scope = self.symbol_table.find_module_scope(from_module_path)?;
        self.symbol_table
            .resolve_struct_in_scope(bare_name, from_scope)
            .map(|(info, _)| {
                self.symbol_table
                    .file_module_path_of_scope(info.definition_scope_id)
            })
    }

    /// Returns the source-root-relative module path of the file that contains the
    /// scope `scope_id`. The entry file yields an empty vector; an imported file
    /// `lib/arith.inf` yields `["lib", "arith"]`.
    ///
    /// A type's layout depends only on its defining file, never the file that
    /// accesses it: two files can each define a same-named struct with different
    /// fields. Code generation derives a struct's own defining path from its
    /// [`StructInfo::definition_scope_id`](crate::StructInfo::definition_scope_id)
    /// and lays its fields out relative to that path, so a nested cross-file field
    /// resolves to the layout of *its* defining file rather than the access site
    /// (#63).
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn module_path_of_scope(&self, scope_id: u32) -> Vec<String> {
        self.symbol_table.file_module_path_of_scope(scope_id)
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
        fields: &[(String, TypeInfo)],
    ) -> anyhow::Result<()> {
        self.symbol_table.register_struct(
            name,
            fields,
            vec![],
            Visibility::Public,
            inference_ast::nodes::Location::default(),
        )?;
        self.build_type_indexes();
        Ok(())
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
        self.symbol_table.register_enum(
            name,
            variants,
            Visibility::Public,
            inference_ast::nodes::Location::default(),
        )?;
        self.build_type_indexes();
        Ok(())
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

    /// Returns the provenance of an `external fn`, or `None` for a local
    /// function or an unbound extern (one declared without a binding `use`).
    ///
    /// The returned [`ExternOrigin`] gives the logical source module and export
    /// field for the named extern. WASM code generation consumes this per call
    /// site to emit an import and lower the call to its import index.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn extern_origin(&self, name: &str) -> Option<ExternOrigin> {
        self.symbol_table
            .lookup_function_anywhere(name)
            .and_then(|info| info.extern_origin().cloned())
    }

    /// Returns the provenance of the **bound** `external fn` declared by
    /// `decl`, resolved by declaration identity rather than by name.
    ///
    /// Analysis uses this to decide whether a specific call resolves to a bound
    /// or unbound extern when two same-named externs exist (e.g. a top-level
    /// and a spec-inner `f`): a name keyed query cannot tell them apart, but the
    /// declaring [`DefId`] can.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn extern_origin_by_decl(&self, decl: DefId) -> Option<ExternOrigin> {
        self.symbol_table.extern_origin_by_decl(decl)
    }

    /// Returns true if the named function is an `external fn` (bound or unbound).
    #[must_use = "this is a pure check with no side effects"]
    pub fn is_extern_function(&self, name: &str) -> bool {
        self.symbol_table
            .lookup_function_anywhere(name)
            .is_some_and(|info| info.is_extern())
    }

    /// Returns the provenance of every **bound** `external fn` in the program,
    /// deduplicated by `(logical_module, export_field)`.
    ///
    /// The build driver consumes this to resolve and validate each external
    /// `.wasm` once before linking. Unbound bare externs carry no origin and do
    /// not appear here.
    #[must_use = "this enumeration has no side effects"]
    pub fn extern_origins(&self) -> Vec<ExternOrigin> {
        self.symbol_table.extern_origins()
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
    use crate::symbol_table::{FuncInfo, FuncKind};
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
            definition_location: inference_ast::nodes::Location::default(),
            kind: FuncKind::Local,
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
            definition_location: inference_ast::nodes::Location::default(),
            kind: FuncKind::Local,
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
            definition_location: inference_ast::nodes::Location::default(),
            kind: FuncKind::Local,
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
            definition_location: inference_ast::nodes::Location::default(),
            kind: FuncKind::Local,
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
            definition_location: inference_ast::nodes::Location::default(),
            kind: FuncKind::Local,
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
