//! Symbol Table
//!
//! This module implements a tree-based symbol table for managing scopes and symbols
//! during type checking. It supports:
//!
//! - Hierarchical scopes with parent-child relationships
//! - Type alias, struct, enum, spec, and function symbol registration
//! - Variable tracking within scopes
//! - Method resolution on types
//! - Import registration and resolution
//! - Visibility checking for access control
//!
//! Scopes form a tree structure where each scope can have multiple child scopes.
//! Symbol lookup walks up the tree from current scope to root until a match is found.
//!
//! ## Default Return Types
//!
//! Functions without an explicit return type default to the unit type. This is
//! represented using `Type::Simple(SimpleTypeKind::Unit)`, which provides a
//! lightweight value-based representation without heap allocation.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::bail;

use crate::type_info::TypeInfo;
use inference_ast::arena::Arena;
use inference_ast::nodes::{
    ArgumentType, Definition, Location, ModuleDefinition, SimpleTypeKind, Type, Visibility,
};
use rustc_hash::{FxHashMap, FxHashSet};

pub(crate) type ScopeRef = Rc<RefCell<Scope>>;

#[derive(Debug, Clone)]
pub(crate) struct FuncInfo {
    pub(crate) name: String,
    pub(crate) type_params: Vec<String>,
    pub(crate) param_types: Vec<TypeInfo>,
    pub(crate) return_type: TypeInfo,
    pub(crate) visibility: Visibility,
    pub(crate) definition_scope_id: u32,
}

/// Information about a struct field.
#[derive(Debug, Clone)]
pub(crate) struct StructFieldInfo {
    #[allow(dead_code)]
    pub(crate) name: String,
    pub(crate) type_info: TypeInfo,
    pub(crate) visibility: Visibility,
}

/// Information about a struct type. Visibility and definition_scope_id are used
/// for visibility checking during member access.
#[derive(Debug, Clone)]
pub(crate) struct StructInfo {
    pub(crate) name: String,
    pub(crate) fields: FxHashMap<String, StructFieldInfo>,
    pub(crate) type_params: Vec<String>,
    pub(crate) visibility: Visibility,
    pub(crate) definition_scope_id: u32,
}

/// Information about an enum type including its variants.
/// Simple unit variants only - associated data support is out of scope.
/// Visibility and definition_scope_id are used for visibility checking during variant access.
#[derive(Debug, Clone)]
pub(crate) struct EnumInfo {
    pub(crate) name: String,
    pub(crate) variants: FxHashSet<String>,
    pub(crate) visibility: Visibility,
    pub(crate) definition_scope_id: u32,
}

/// Information about a method defined on a type.
///
/// # Instance Methods vs Associated Functions
///
/// Methods are distinguished by whether they take `self` as the first argument:
///
/// - **Instance methods** (`has_self = true`): Take `self`, `&self`, or `&mut self`
///   as the first parameter. Called via `instance.method(args)`.
///
/// - **Associated functions** (`has_self = false`): Do not take `self`.
///   Typically constructors like `new()`. Called via `Type::function(args)`.
///
/// # Fields
///
/// - `signature`: Function information including name, parameters, and return type
/// - `visibility`: Access control for the method
/// - `scope_id`: The scope where this method is defined (for visibility checking)
/// - `has_self`: Whether this method takes `self` as first argument
#[derive(Debug, Clone)]
pub(crate) struct MethodInfo {
    pub(crate) signature: FuncInfo,
    pub(crate) visibility: Visibility,
    pub(crate) scope_id: u32,
    pub(crate) has_self: bool,
}

impl MethodInfo {
    /// Returns true if this method takes `self` as first argument.
    ///
    /// Instance methods (`has_self = true`) are called via `instance.method()`.
    /// Associated functions (`has_self = false`) are called via `Type::function()`.
    #[must_use = "this is a pure check with no side effects"]
    pub(crate) fn is_instance_method(&self) -> bool {
        self.has_self
    }
}

/// A single item in an import statement
#[derive(Debug, Clone)]
pub(crate) struct ImportItem {
    /// The name being imported
    pub(crate) name: String,
    /// Optional alias (for `use path::item as alias`)
    pub(crate) alias: Option<String>,
}

/// The kind of import statement
#[derive(Debug, Clone)]
pub(crate) enum ImportKind {
    /// Plain import: `use path::item`
    Plain,
    /// Glob import: `use path::*`
    #[allow(dead_code)]
    Glob,
    /// Partial import with multiple items: `use path::{a, b as c}`
    Partial(Vec<ImportItem>),
}

/// Represents an unresolved import in a scope
#[derive(Debug, Clone)]
pub(crate) struct Import {
    /// The path segments of the import (e.g., ["std", "io", "File"])
    pub(crate) path: Vec<String>,
    /// The kind of import
    pub(crate) kind: ImportKind,
    /// Source location of the import statement
    pub(crate) location: Location,
}

/// Represents a resolved import binding.
/// Fields `symbol` and `definition_scope_id` are used in future phases
/// for visibility checking and resolved name lookup.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedImport {
    /// The local name (either original or alias)
    pub(crate) local_name: String,
    /// The resolved symbol
    #[allow(dead_code)]
    pub(crate) symbol: Symbol,
    /// The scope where the symbol is defined (for visibility checking)
    #[allow(dead_code)]
    pub(crate) definition_scope_id: u32,
}

#[derive(Debug, Clone)]
pub(crate) enum Symbol {
    /// A type alias mapping a name to another type (`type X = Y;`).
    /// Also used for builtin type bindings (i32, bool, etc.).
    TypeAlias(TypeInfo),
    Struct(StructInfo),
    Enum(EnumInfo),
    Spec(String),
    Function(FuncInfo),
}

impl Symbol {
    #[allow(dead_code)]
    #[must_use = "discarding the name has no effect"]
    pub(crate) fn name(&self) -> String {
        match self {
            Symbol::TypeAlias(ti) => ti.to_string(),
            Symbol::Struct(info) => info.name.clone(),
            Symbol::Enum(info) => info.name.clone(),
            Symbol::Spec(name) => name.clone(),
            Symbol::Function(sig) => sig.name.clone(),
        }
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn as_function(&self) -> Option<&FuncInfo> {
        if let Symbol::Function(sig) = self {
            Some(sig)
        } else {
            None
        }
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn as_struct(&self) -> Option<&StructInfo> {
        if let Symbol::Struct(info) = self {
            Some(info)
        } else {
            None
        }
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn as_enum(&self) -> Option<&EnumInfo> {
        if let Symbol::Enum(info) = self {
            Some(info)
        } else {
            None
        }
    }

    #[must_use = "this is a pure conversion with no side effects"]
    pub(crate) fn as_type_info(&self) -> Option<TypeInfo> {
        match self {
            Symbol::TypeAlias(ti) => Some(ti.clone()),
            Symbol::Struct(info) => Some(TypeInfo {
                kind: crate::type_info::TypeInfoKind::Struct(info.name.clone()),
                type_params: info.type_params.clone(),
            }),
            Symbol::Enum(info) => Some(TypeInfo {
                kind: crate::type_info::TypeInfoKind::Enum(info.name.clone()),
                type_params: vec![],
            }),
            Symbol::Spec(name) => Some(TypeInfo {
                kind: crate::type_info::TypeInfoKind::Spec(name.clone()),
                type_params: vec![],
            }),
            Symbol::Function(_) => None,
        }
    }

    /// Check if this symbol has public visibility.
    ///
    /// Structs, Enums, and Functions respect their visibility field.
    /// Type aliases and Specs are currently treated as public.
    #[must_use = "this is a pure check with no side effects"]
    pub(crate) fn is_public(&self) -> bool {
        match self {
            Symbol::TypeAlias(_) => true,
            Symbol::Struct(info) => matches!(info.visibility, Visibility::Public),
            Symbol::Enum(info) => matches!(info.visibility, Visibility::Public),
            Symbol::Spec(_) => true,
            Symbol::Function(sig) => matches!(sig.visibility, Visibility::Public),
        }
    }
}

/// A scope in the symbol table tree.
#[derive(Debug)]
pub(crate) struct Scope {
    pub(crate) id: u32,
    pub(crate) name: String,
    /// Full path from root (e.g., "mod1::mod2::mod3"), cached at creation time for O(1) lookup.
    pub(crate) full_path: String,
    #[allow(dead_code)]
    pub(crate) visibility: Visibility,
    pub(crate) parent: Option<ScopeRef>,
    pub(crate) children: Vec<ScopeRef>,
    pub(crate) symbols: FxHashMap<String, Symbol>,
    pub(crate) variables: FxHashMap<String, (u32, TypeInfo)>,
    pub(crate) methods: FxHashMap<String, Vec<MethodInfo>>,
    /// Unresolved imports registered in this scope
    pub(crate) imports: Vec<Import>,
    /// Resolved import bindings (populated after resolution phase)
    pub(crate) resolved_imports: FxHashMap<String, ResolvedImport>,
}

impl Scope {
    #[must_use = "scope constructor returns a new scope that should be used"]
    pub(crate) fn new(
        id: u32,
        name: &str,
        full_path: String,
        visibility: Visibility,
        parent: Option<ScopeRef>,
    ) -> ScopeRef {
        Rc::new(RefCell::new(Self {
            id,
            name: name.to_string(),
            full_path,
            visibility,
            parent,
            children: Vec::new(),
            symbols: FxHashMap::default(),
            variables: FxHashMap::default(),
            methods: FxHashMap::default(),
            imports: Vec::new(),
            resolved_imports: FxHashMap::default(),
        }))
    }

    pub(crate) fn add_child(&mut self, child: ScopeRef) {
        self.children.push(child);
    }

    pub(crate) fn insert_symbol(&mut self, name: &str, symbol: Symbol) -> anyhow::Result<()> {
        if self.symbols.contains_key(name) {
            bail!("Symbol `{name}` already exists in this scope");
        }
        self.symbols.insert(name.to_string(), symbol);
        Ok(())
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_symbol_local(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_symbol(&self, name: &str) -> Option<Symbol> {
        if let Some(symbol) = self.lookup_symbol_local(name) {
            return Some(symbol.clone());
        }
        if let Some(parent) = &self.parent {
            return parent.borrow().lookup_symbol(name);
        }
        None
    }

    pub(crate) fn insert_variable(
        &mut self,
        name: &str,
        node_id: u32,
        ty: TypeInfo,
    ) -> anyhow::Result<()> {
        if self.variables.contains_key(name) {
            bail!("Variable `{name}` already declared in this scope");
        }
        self.variables.insert(name.to_string(), (node_id, ty));
        Ok(())
    }

    #[must_use = "this is a pure lookup with no side effects"]
    fn lookup_variable_local(&self, name: &str) -> Option<(u32, TypeInfo)> {
        self.variables.get(name).cloned()
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_variable(&self, name: &str) -> Option<TypeInfo> {
        if let Some((_, ty)) = self.lookup_variable_local(name) {
            return Some(ty);
        }
        if let Some(parent) = &self.parent {
            return parent.borrow().lookup_variable(name);
        }
        None
    }

    pub(crate) fn insert_method(&mut self, type_name: &str, method_info: MethodInfo) {
        self.methods
            .entry(type_name.to_string())
            .or_default()
            .push(method_info);
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_method(&self, type_name: &str, method_name: &str) -> Option<MethodInfo> {
        if let Some(method_info) = self
            .methods
            .get(type_name)
            .and_then(|methods| methods.iter().find(|m| m.signature.name == method_name))
        {
            return Some(method_info.clone());
        }
        if let Some(parent) = &self.parent {
            return parent.borrow().lookup_method(type_name, method_name);
        }
        None
    }

    /// Add an unresolved import to this scope
    pub(crate) fn add_import(&mut self, import: Import) {
        self.imports.push(import);
    }

    /// Add a resolved import binding
    pub(crate) fn add_resolved_import(&mut self, resolved: ResolvedImport) {
        self.resolved_imports
            .insert(resolved.local_name.clone(), resolved);
    }

    #[allow(dead_code)]
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_resolved_import(&self, name: &str) -> Option<&ResolvedImport> {
        self.resolved_imports.get(name)
    }
}

#[derive(Clone)]
pub(crate) struct SymbolTable {
    scopes: FxHashMap<u32, ScopeRef>,
    mod_scopes: FxHashMap<String, ScopeRef>,
    root_scope: Option<ScopeRef>,
    current_scope: Option<ScopeRef>,
    next_scope_id: u32,
}

impl Default for SymbolTable {
    fn default() -> Self {
        let mut table = SymbolTable {
            scopes: FxHashMap::default(),
            mod_scopes: FxHashMap::default(),
            root_scope: None,
            current_scope: None,
            next_scope_id: 0,
        };
        table.init_root_scope();
        table.init_builtin_types();
        table
    }
}

impl SymbolTable {
    fn init_root_scope(&mut self) {
        let root = Scope::new(
            self.next_scope_id,
            "root",
            String::new(),
            Visibility::Public,
            None,
        );
        self.scopes.insert(self.next_scope_id, Rc::clone(&root));
        self.mod_scopes.insert(String::new(), Rc::clone(&root));
        self.next_scope_id += 1;
        self.root_scope = Some(Rc::clone(&root));
        self.current_scope = Some(root);
    }

    fn init_builtin_types(&mut self) {
        use crate::type_info::{NumberType, TypeInfoKind};

        if let Some(scope) = &self.current_scope {
            let mut scope_mut = scope.borrow_mut();

            for number_type in NumberType::ALL {
                let type_info = TypeInfo {
                    kind: TypeInfoKind::Number(*number_type),
                    type_params: vec![],
                };
                let _ = scope_mut.insert_symbol(number_type.as_str(), Symbol::TypeAlias(type_info));
            }

            for (name, kind) in TypeInfoKind::NON_NUMERIC_BUILTINS {
                let type_info = TypeInfo {
                    kind: kind.clone(),
                    type_params: vec![],
                };
                let _ = scope_mut.insert_symbol(name, Symbol::TypeAlias(type_info));
            }
        }
    }

    pub(crate) fn push_scope(&mut self) -> u32 {
        let name = format!("anonymous_{}", self.next_scope_id);
        self.push_scope_with_name(&name, Visibility::Private)
    }

    pub(crate) fn push_scope_with_name(&mut self, name: &str, visibility: Visibility) -> u32 {
        let parent = self.current_scope.clone();
        let scope_id = self.next_scope_id;
        self.next_scope_id += 1;

        let full_path = match &parent {
            Some(p) => {
                let parent_path = &p.borrow().full_path;
                if parent_path.is_empty() {
                    name.to_string()
                } else {
                    format!("{parent_path}::{name}")
                }
            }
            None => name.to_string(),
        };

        let new_scope = Scope::new(scope_id, name, full_path, visibility, parent.clone());

        if let Some(current) = &parent {
            current.borrow_mut().add_child(Rc::clone(&new_scope));
        }

        self.scopes.insert(scope_id, Rc::clone(&new_scope));
        self.current_scope = Some(new_scope);
        scope_id
    }

    pub(crate) fn pop_scope(&mut self) {
        if let Some(current) = &self.current_scope {
            let parent = current.borrow().parent.clone();
            self.current_scope = parent;
        }
    }

    pub(crate) fn register_type(&mut self, name: &str, ty: Option<&Type>) -> anyhow::Result<()> {
        if let Some(scope) = &self.current_scope {
            let type_info = if let Some(ty) = ty {
                TypeInfo::new(ty)
            } else {
                TypeInfo {
                    kind: crate::type_info::TypeInfoKind::Custom(name.to_string()),
                    type_params: vec![],
                }
            };
            scope
                .borrow_mut()
                .insert_symbol(name, Symbol::TypeAlias(type_info))
        } else {
            bail!("No active scope to register type")
        }
    }

    pub(crate) fn register_struct(
        &mut self,
        name: &str,
        fields: &[(String, TypeInfo, Visibility)],
        type_params: Vec<String>,
        visibility: Visibility,
    ) -> anyhow::Result<()> {
        if let Some(scope) = &self.current_scope {
            let scope_id = scope.borrow().id;
            let mut field_map = FxHashMap::default();
            for (field_name, field_type, field_visibility) in fields {
                field_map.insert(
                    field_name.clone(),
                    StructFieldInfo {
                        name: field_name.clone(),
                        type_info: field_type.clone(),
                        visibility: field_visibility.clone(),
                    },
                );
            }
            let struct_info = StructInfo {
                name: name.to_string(),
                fields: field_map,
                type_params,
                visibility,
                definition_scope_id: scope_id,
            };
            scope
                .borrow_mut()
                .insert_symbol(name, Symbol::Struct(struct_info))
        } else {
            bail!("No active scope to register struct")
        }
    }

    pub(crate) fn register_enum(
        &mut self,
        name: &str,
        variants: &[&str],
        visibility: Visibility,
    ) -> anyhow::Result<()> {
        if let Some(scope) = &self.current_scope {
            let scope_id = scope.borrow().id;
            let enum_info = EnumInfo {
                name: name.to_string(),
                variants: variants.iter().map(|s| (*s).to_string()).collect(),
                visibility,
                definition_scope_id: scope_id,
            };
            scope
                .borrow_mut()
                .insert_symbol(name, Symbol::Enum(enum_info))
        } else {
            bail!("No active scope to register enum")
        }
    }

    pub(crate) fn register_spec(&mut self, name: &str) -> anyhow::Result<()> {
        if let Some(scope) = &self.current_scope {
            scope
                .borrow_mut()
                .insert_symbol(name, Symbol::Spec(name.to_string()))
        } else {
            bail!("No active scope to register spec")
        }
    }

    pub(crate) fn register_function(
        &mut self,
        name: &str,
        type_params: Vec<String>,
        param_types: &[Type],
        return_type: &Type,
    ) -> Result<(), String> {
        self.register_function_with_visibility(
            name,
            type_params,
            param_types,
            return_type,
            Visibility::Private,
        )
    }

    pub(crate) fn register_function_with_visibility(
        &mut self,
        name: &str,
        type_params: Vec<String>,
        param_types: &[Type],
        return_type: &Type,
        visibility: Visibility,
    ) -> Result<(), String> {
        if let Some(scope) = &self.current_scope {
            let scope_id = scope.borrow().id;
            // Use type_params when constructing TypeInfo so that
            // type parameters like T, U are recognized as Generic types
            let sig = FuncInfo {
                name: name.to_string(),
                type_params: type_params.clone(),
                param_types: param_types
                    .iter()
                    .map(|t| TypeInfo::new_with_type_params(t, &type_params))
                    .collect(),
                return_type: TypeInfo::new_with_type_params(return_type, &type_params),
                visibility,
                definition_scope_id: scope_id,
            };
            scope
                .borrow_mut()
                .insert_symbol(name, Symbol::Function(sig))
                .map_err(|e| e.to_string())
        } else {
            Err("No active scope to register function".to_string())
        }
    }

    pub(crate) fn push_variable_to_scope(
        &mut self,
        name: &str,
        var_type: TypeInfo,
    ) -> anyhow::Result<()> {
        if let Some(scope) = &self.current_scope {
            scope.borrow_mut().insert_variable(name, 0, var_type)
        } else {
            bail!("No active scope to push variable")
        }
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_type(&self, name: &str) -> Option<TypeInfo> {
        if let Some(scope) = &self.current_scope {
            if let Some(symbol) = scope.borrow().lookup_symbol(name) {
                return symbol.as_type_info();
            }
            if let Some(symbol) = scope.borrow().lookup_symbol(&name.to_lowercase()) {
                return symbol.as_type_info();
            }
        }
        None
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_variable(&self, name: &str) -> Option<TypeInfo> {
        self.current_scope
            .as_ref()
            .and_then(|scope| scope.borrow().lookup_variable(name))
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_function(&self, name: &str) -> Option<FuncInfo> {
        self.current_scope
            .as_ref()
            .and_then(|scope| scope.borrow().lookup_symbol(name))
            .and_then(|symbol| symbol.as_function().cloned())
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_struct(&self, name: &str) -> Option<StructInfo> {
        self.current_scope
            .as_ref()
            .and_then(|scope| scope.borrow().lookup_symbol(name))
            .and_then(|symbol| symbol.as_struct().cloned())
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_enum(&self, name: &str) -> Option<EnumInfo> {
        self.current_scope
            .as_ref()
            .and_then(|scope| scope.borrow().lookup_symbol(name))
            .and_then(|symbol| symbol.as_enum().cloned())
    }

    pub(crate) fn register_method(
        &mut self,
        type_name: &str,
        signature: FuncInfo,
        visibility: Visibility,
        has_self: bool,
    ) -> anyhow::Result<()> {
        if let Some(scope) = &self.current_scope {
            let scope_id = scope.borrow().id;
            let method_info = MethodInfo {
                signature,
                visibility,
                scope_id,
                has_self,
            };
            scope.borrow_mut().insert_method(type_name, method_info);
            Ok(())
        } else {
            bail!("No active scope to register method")
        }
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_method(&self, type_name: &str, method_name: &str) -> Option<MethodInfo> {
        self.current_scope
            .as_ref()
            .and_then(|scope| scope.borrow().lookup_method(type_name, method_name))
    }

    #[must_use = "returns the scope ID which may be needed for later reference"]
    pub(crate) fn enter_module(&mut self, module: &Rc<ModuleDefinition>) -> u32 {
        let scope_id = self.push_scope_with_name(&module.name(), module.visibility.clone());
        if let Some(scope) = self.scopes.get(&scope_id) {
            let full_path = scope.borrow().full_path.clone();
            self.mod_scopes.insert(full_path, Rc::clone(scope));
        }
        scope_id
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn find_module_scope(&self, path: &[String]) -> Option<u32> {
        let key = path.join("::");
        self.mod_scopes.get(&key).map(|s| s.borrow().id)
    }

    /// Get all public symbols from a scope (for glob imports).
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn get_public_symbols_from_scope(&self, scope_id: u32) -> Vec<(String, Symbol)> {
        self.get_scope(scope_id)
            .map(|scope| {
                scope
                    .borrow()
                    .symbols
                    .iter()
                    .filter(|(_, sym)| sym.is_public())
                    .map(|(name, sym)| (name.clone(), sym.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn current_scope_id(&self) -> Option<u32> {
        self.current_scope.as_ref().map(|s| s.borrow().id)
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn get_scope(&self, scope_id: u32) -> Option<ScopeRef> {
        self.scopes.get(&scope_id).cloned()
    }

    pub(crate) fn register_import(&mut self, import: Import) -> anyhow::Result<()> {
        if let Some(scope) = &self.current_scope {
            scope.borrow_mut().add_import(import);
            Ok(())
        } else {
            bail!("No active scope to register import")
        }
    }

    /// Get all scope IDs for iteration
    #[must_use = "discarding the scope IDs has no effect"]
    pub(crate) fn all_scope_ids(&self) -> Vec<u32> {
        self.scopes.keys().copied().collect()
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn resolve_qualified_name(
        &self,
        path: &[String],
        from_scope_id: u32,
    ) -> Option<(Symbol, u32)> {
        if path.is_empty() {
            return None;
        }

        let first_segment = &path[0];

        let start_scope = if first_segment == "self" {
            self.get_scope(from_scope_id)?
        } else {
            self.root_scope.clone()?
        };

        let mut current_scope = start_scope;

        let module_path = if first_segment == "self" {
            &path[1..]
        } else {
            path
        };

        for (i, segment) in module_path.iter().enumerate() {
            if i == module_path.len() - 1 {
                let scope = current_scope.borrow();
                if let Some(symbol) = scope.lookup_symbol_local(segment) {
                    return Some((symbol.clone(), scope.id));
                }
                return None;
            }

            let scope = current_scope.borrow();
            let child = scope
                .children
                .iter()
                .find(|c| c.borrow().name == *segment)
                .cloned();

            match child {
                Some(c) => {
                    drop(scope);
                    current_scope = c;
                }
                None => return None,
            }
        }

        None
    }

    /// Load an external module's symbols into the symbol table.
    ///
    /// Creates a virtual child scope of root containing the module's public symbols.
    /// The module is accessible via `mod_scopes` using the module name as key.
    ///
    /// # Arguments
    /// * `module_name` - Name of the external module
    /// * `arena` - The parsed AST arena of the external module
    ///
    /// # Returns
    /// The scope ID of the created module scope
    ///
    /// # Errors
    /// Returns an error if symbol registration fails
    #[allow(dead_code)]
    pub(crate) fn load_external_module(
        &mut self,
        module_name: &str,
        arena: &Arena,
    ) -> anyhow::Result<u32> {
        let scope_id = self.push_scope_with_name(module_name, Visibility::Public);

        if let Some(scope) = self.scopes.get(&scope_id) {
            let full_path = scope.borrow().full_path.clone();
            self.mod_scopes.insert(full_path, Rc::clone(scope));
        }

        for source_file in arena.source_files() {
            for definition in &source_file.definitions {
                self.register_definition_from_external(definition)?;
            }
        }

        self.pop_scope();

        Ok(scope_id)
    }

    /// Register a definition from an external module into the current scope.
    ///
    /// Currently handles: Struct, Enum, Spec, Function, Type, Constant, ExternalFunction, Module.
    #[allow(dead_code)]
    fn register_definition_from_external(&mut self, definition: &Definition) -> anyhow::Result<()> {
        match definition {
            Definition::Struct(s) => {
                let fields: Vec<(String, TypeInfo, Visibility)> = s
                    .fields
                    .iter()
                    .map(|f| {
                        (
                            f.name.name.clone(),
                            TypeInfo::new(&f.type_),
                            Visibility::Private,
                        )
                    })
                    .collect();
                self.register_struct(&s.name(), &fields, vec![], s.visibility.clone())?;
            }
            Definition::Enum(e) => {
                let variants: Vec<&str> = e.variants.iter().map(|v| v.name.as_str()).collect();
                self.register_enum(&e.name(), &variants, e.visibility.clone())?;
            }
            Definition::Spec(sp) => {
                self.register_spec(&sp.name())?;
            }
            Definition::Function(f) => {
                let type_params = f
                    .type_parameters
                    .as_ref()
                    .map(|tps| tps.iter().map(|p| p.name()).collect())
                    .unwrap_or_default();
                let param_types: Vec<_> = f
                    .arguments
                    .as_ref()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|a| match a {
                        ArgumentType::Argument(arg) => Some(arg.ty.clone()),
                        ArgumentType::IgnoreArgument(ig) => Some(ig.ty.clone()),
                        ArgumentType::Type(t) => Some(t.clone()),
                        ArgumentType::SelfReference(_) => None,
                    })
                    .collect();
                let return_type = f
                    .returns
                    .clone()
                    .unwrap_or(Type::Simple(SimpleTypeKind::Unit));

                self.register_function_with_visibility(
                    &f.name(),
                    type_params,
                    &param_types,
                    &return_type,
                    f.visibility.clone(),
                )
                .map_err(|e| anyhow::anyhow!(e))?;
            }
            Definition::Type(t) => {
                self.register_type(&t.name(), Some(&t.ty))?;
            }
            Definition::Constant(constant_definition) => {
                self.push_variable_to_scope(
                    &constant_definition.name(),
                    TypeInfo::new(&constant_definition.ty),
                )?;
            }
            Definition::ExternalFunction(external_function_definition) => {
                let param_types: Vec<_> = external_function_definition
                    .arguments
                    .as_ref()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|param| match param {
                        ArgumentType::SelfReference(_) => None,
                        ArgumentType::IgnoreArgument(ignore_argument) => {
                            Some(ignore_argument.ty.clone())
                        }
                        ArgumentType::Argument(argument) => Some(argument.ty.clone()),
                        ArgumentType::Type(ty) => Some(ty.clone()),
                    })
                    .collect();
                let return_type = external_function_definition
                    .returns
                    .clone()
                    .unwrap_or(Type::Simple(SimpleTypeKind::Unit));

                self.register_function_with_visibility(
                    &external_function_definition.name(),
                    vec![],
                    &param_types,
                    &return_type,
                    external_function_definition.visibility.clone(),
                )
                .map_err(|e| anyhow::anyhow!(e))?;
            }
            Definition::Module(module_definition) => {
                let _scope_id = self.enter_module(module_definition);
                if let Some(body) = &module_definition.body {
                    for definition in body {
                        self.register_definition_from_external(definition)?;
                    }
                }
                self.pop_scope();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::type_info::{NumberType, TypeInfoKind};

    mod symbol_type_alias {
        use super::*;

        #[test]
        fn name_returns_type_info_string_representation() {
            let type_info = TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            };
            let symbol = Symbol::TypeAlias(type_info);
            let name = symbol.name();
            assert_eq!(name, "i32");
        }

        #[test]
        fn as_type_info_returns_clone_of_wrapped_type() {
            let type_info = TypeInfo {
                kind: TypeInfoKind::Number(NumberType::U64),
                type_params: vec![],
            };
            let symbol = Symbol::TypeAlias(type_info.clone());
            let result = symbol.as_type_info();
            assert!(result.is_some());
            let result_type = result.unwrap();
            assert!(matches!(
                result_type.kind,
                TypeInfoKind::Number(NumberType::U64)
            ));
        }

        #[test]
        fn as_type_info_with_custom_type() {
            let type_info = TypeInfo {
                kind: TypeInfoKind::Custom("MyType".to_string()),
                type_params: vec![],
            };
            let symbol = Symbol::TypeAlias(type_info);
            let result = symbol.as_type_info();
            assert!(result.is_some());
            let result_type = result.unwrap();
            assert!(matches!(result_type.kind, TypeInfoKind::Custom(ref s) if s == "MyType"));
        }

        #[test]
        fn is_public_always_returns_true() {
            let type_info = TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            };
            let symbol = Symbol::TypeAlias(type_info);
            assert!(symbol.is_public());
        }

        #[test]
        fn register_type_creates_type_alias_with_provided_type() {
            use inference_ast::nodes::SimpleTypeKind;
            let mut table = SymbolTable::default();
            let simple_type = Type::Simple(SimpleTypeKind::I32);
            let result = table.register_type("MyInt", Some(&simple_type));
            assert!(result.is_ok());
            let lookup = table.lookup_type("MyInt");
            assert!(lookup.is_some());
        }

        #[test]
        fn register_type_creates_custom_type_when_none_provided() {
            let mut table = SymbolTable::default();
            let result = table.register_type("MyCustomType", None);
            assert!(result.is_ok());
            let lookup = table.lookup_type("MyCustomType");
            assert!(lookup.is_some());
            let type_info = lookup.unwrap();
            assert!(matches!(type_info.kind, TypeInfoKind::Custom(ref s) if s == "MyCustomType"));
        }

        #[test]
        fn builtin_types_are_registered_as_type_aliases() {
            let table = SymbolTable::default();
            assert!(table.lookup_type("i8").is_some());
            assert!(table.lookup_type("i16").is_some());
            assert!(table.lookup_type("i32").is_some());
            assert!(table.lookup_type("i64").is_some());
            assert!(table.lookup_type("u8").is_some());
            assert!(table.lookup_type("u16").is_some());
            assert!(table.lookup_type("u32").is_some());
            assert!(table.lookup_type("u64").is_some());
            assert!(table.lookup_type("bool").is_some());
            assert!(table.lookup_type("unit").is_some());
            assert!(table.lookup_type("string").is_some());
        }

        #[test]
        fn lookup_type_returns_type_alias_info() {
            let mut table = SymbolTable::default();
            table.register_type("TestType", None).unwrap();
            let result = table.lookup_type("TestType");
            assert!(result.is_some());
        }

        #[test]
        fn as_function_returns_none_for_type_alias() {
            let type_info = TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            };
            let symbol = Symbol::TypeAlias(type_info);
            assert!(symbol.as_function().is_none());
        }

        #[test]
        fn as_struct_returns_none_for_type_alias() {
            let type_info = TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            };
            let symbol = Symbol::TypeAlias(type_info);
            assert!(symbol.as_struct().is_none());
        }

        #[test]
        fn as_enum_returns_none_for_type_alias() {
            let type_info = TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            };
            let symbol = Symbol::TypeAlias(type_info);
            assert!(symbol.as_enum().is_none());
        }
    }

    mod anonymous_scope_naming {
        use super::*;

        #[test]
        fn unique_names_for_consecutive_scopes() {
            let mut table = SymbolTable::default();
            let scope1_id = table.push_scope();
            let scope2_id = table.push_scope();
            let scope1 = table.get_scope(scope1_id).unwrap();
            let scope2 = table.get_scope(scope2_id).unwrap();
            assert_ne!(
                scope1.borrow().name,
                scope2.borrow().name,
                "Consecutive anonymous scopes should have unique names"
            );
        }

        #[test]
        fn name_includes_scope_id() {
            let mut table = SymbolTable::default();
            let scope_id = table.push_scope();
            let scope = table.get_scope(scope_id).unwrap();
            assert!(
                scope.borrow().name.starts_with("anonymous_"),
                "Anonymous scope name should start with 'anonymous_'"
            );
            let expected_name = format!("anonymous_{scope_id}");
            assert_eq!(
                scope.borrow().name,
                expected_name,
                "Anonymous scope name should match pattern anonymous_{{scope_id}}"
            );
        }

        #[test]
        fn nested_scopes_have_distinguishable_paths() {
            let mut table = SymbolTable::default();
            table.push_scope_with_name("test_func", Visibility::Private);
            let inner1_id = table.push_scope();
            let inner2_id = table.push_scope();
            let inner1 = table.get_scope(inner1_id).unwrap();
            let inner2 = table.get_scope(inner2_id).unwrap();
            assert_ne!(
                inner1.borrow().full_path,
                inner2.borrow().full_path,
                "Nested anonymous scopes should have different full_paths"
            );
            assert!(
                inner1.borrow().full_path.contains("test_func"),
                "Full path should include parent function name"
            );
        }

        #[test]
        fn anonymous_scopes_not_in_mod_scopes() {
            let mut table = SymbolTable::default();
            let scope_id = table.push_scope();
            let scope = table.get_scope(scope_id).unwrap();
            let full_path = scope.borrow().full_path.clone();
            let path_segments: Vec<String> = full_path.split("::").map(String::from).collect();
            assert!(
                table.find_module_scope(&path_segments).is_none(),
                "Anonymous scopes should not be registered in mod_scopes"
            );
        }

        #[test]
        fn pop_push_maintains_correct_ids() {
            let mut table = SymbolTable::default();
            let scope1_id = table.push_scope();
            table.pop_scope();
            let scope2_id = table.push_scope();
            assert_ne!(
                scope1_id, scope2_id,
                "Popping and pushing should create new scope with different ID"
            );
            assert_eq!(
                scope2_id,
                scope1_id + 1,
                "Scope IDs should increment sequentially even after pop"
            );
        }

        #[test]
        fn deeply_nested_anonymous_scopes() {
            let mut table = SymbolTable::default();
            let depth = 10;
            let mut scope_ids = Vec::new();
            for _ in 0..depth {
                scope_ids.push(table.push_scope());
            }
            for (i, scope_id) in scope_ids.iter().enumerate() {
                let scope = table.get_scope(*scope_id).unwrap();
                let scope_borrow = scope.borrow();
                let expected_depth = i + 1;
                let path_parts: Vec<&str> = scope_borrow.full_path.split("::").collect();
                assert_eq!(
                    path_parts.len(),
                    expected_depth,
                    "Deeply nested scope at level {i} should have correct path depth"
                );
                assert!(
                    scope_borrow.name.starts_with("anonymous_"),
                    "All nested scopes should have anonymous_ prefix"
                );
            }
        }

        #[test]
        fn sibling_anonymous_scopes_have_unique_names() {
            let mut table = SymbolTable::default();
            table.push_scope_with_name("parent", Visibility::Private);
            let sibling1_id = table.push_scope();
            table.pop_scope();
            let sibling2_id = table.push_scope();
            table.pop_scope();
            let sibling3_id = table.push_scope();
            let sibling1 = table.get_scope(sibling1_id).unwrap();
            let sibling2 = table.get_scope(sibling2_id).unwrap();
            let sibling3 = table.get_scope(sibling3_id).unwrap();
            let names = [
                sibling1.borrow().name.clone(),
                sibling2.borrow().name.clone(),
                sibling3.borrow().name.clone(),
            ];
            assert_eq!(
                names.len(),
                names.iter().collect::<FxHashSet<_>>().len(),
                "All sibling anonymous scopes should have unique names"
            );
        }

        #[test]
        fn anonymous_scope_parent_relationship() {
            let mut table = SymbolTable::default();
            let parent_id = table.push_scope_with_name("parent_func", Visibility::Private);
            let child_id = table.push_scope();
            let child_scope = table.get_scope(child_id).unwrap();
            let parent_scope = table.get_scope(parent_id).unwrap();
            let child_parent = child_scope.borrow().parent.clone();
            assert!(
                child_parent.is_some(),
                "Anonymous child scope should have parent"
            );
            let child_parent_id = child_parent.unwrap().borrow().id;
            assert_eq!(
                child_parent_id, parent_id,
                "Anonymous scope's parent should be the enclosing scope"
            );
            let parent_children = &parent_scope.borrow().children;
            assert_eq!(
                parent_children.len(),
                1,
                "Parent should have the anonymous child in its children list"
            );
            assert_eq!(
                parent_children[0].borrow().id,
                child_id,
                "Parent's child should be the anonymous scope"
            );
        }

        #[test]
        fn anonymous_scope_visibility_is_private() {
            let mut table = SymbolTable::default();
            let scope_id = table.push_scope();
            let scope = table.get_scope(scope_id).unwrap();
            assert!(
                matches!(scope.borrow().visibility, Visibility::Private),
                "Anonymous scopes should have private visibility"
            );
        }

        #[test]
        fn multiple_anonymous_scopes_increment_id_correctly() {
            let mut table = SymbolTable::default();
            let count = 20;
            let mut scope_ids = Vec::new();
            for _ in 0..count {
                scope_ids.push(table.push_scope());
            }
            for i in 1..count {
                assert_eq!(
                    scope_ids[i],
                    scope_ids[i - 1] + 1,
                    "Scope IDs should increment by 1 for consecutive anonymous scopes"
                );
            }
        }

        #[test]
        fn anonymous_scope_full_path_construction() {
            let mut table = SymbolTable::default();
            table.push_scope_with_name("mod1", Visibility::Private);
            table.push_scope_with_name("mod2", Visibility::Private);
            let anon_id = table.push_scope();
            let anon_scope = table.get_scope(anon_id).unwrap();
            let full_path = anon_scope.borrow().full_path.clone();
            let name = anon_scope.borrow().name.clone();
            let expected_path = format!("mod1::mod2::{name}");
            assert_eq!(
                full_path, expected_path,
                "Anonymous scope full_path should include all parent module names"
            );
            assert!(
                full_path.contains("::anonymous_"),
                "Full path should contain the anonymous scope name with separator"
            );
        }

        #[test]
        fn root_level_anonymous_scope_no_separator_in_path() {
            let mut table = SymbolTable::default();
            let scope_id = table.push_scope();
            let scope = table.get_scope(scope_id).unwrap();
            let full_path = scope.borrow().full_path.clone();
            assert!(
                !full_path.starts_with("::"),
                "Root-level anonymous scope should not start with ::"
            );
            assert!(
                full_path.starts_with("anonymous_"),
                "Root-level anonymous scope full_path should be just the name"
            );
        }
    }

    mod method_info_tests {
        use super::*;
        #[test]
        fn is_instance_method_returns_true_when_has_self() {
            let method_info = MethodInfo {
                signature: FuncInfo {
                    name: "get_value".to_string(),
                    type_params: vec![],
                    param_types: vec![],
                    return_type: TypeInfo::default(),
                    visibility: Visibility::Private,
                    definition_scope_id: 0,
                },
                visibility: Visibility::Private,
                scope_id: 0,
                has_self: true,
            };
            assert!(method_info.is_instance_method());
        }

        #[test]
        fn is_instance_method_returns_false_for_associated_function() {
            let method_info = MethodInfo {
                signature: FuncInfo {
                    name: "new".to_string(),
                    type_params: vec![],
                    param_types: vec![],
                    return_type: TypeInfo::default(),
                    visibility: Visibility::Public,
                    definition_scope_id: 0,
                },
                visibility: Visibility::Public,
                scope_id: 0,
                has_self: false,
            };
            assert!(!method_info.is_instance_method());
        }

        #[test]
        fn register_method_stores_has_self_true_correctly() {
            let mut table = SymbolTable::default();
            table.push_scope_with_name("TestType", Visibility::Public);
            let sig = FuncInfo {
                name: "instance_method".to_string(),
                type_params: vec![],
                param_types: vec![],
                return_type: TypeInfo::default(),
                visibility: Visibility::Public,
                definition_scope_id: 0,
            };
            let result = table.register_method("TestType", sig, Visibility::Public, true);
            assert!(result.is_ok());
            let method_info = table.lookup_method("TestType", "instance_method");
            assert!(method_info.is_some());
            let method_info = method_info.unwrap();
            assert!(method_info.has_self);
            assert!(method_info.is_instance_method());
        }

        #[test]
        fn register_method_stores_has_self_false_correctly() {
            let mut table = SymbolTable::default();
            table.push_scope_with_name("TestType", Visibility::Public);
            let sig = FuncInfo {
                name: "constructor".to_string(),
                type_params: vec![],
                param_types: vec![],
                return_type: TypeInfo::default(),
                visibility: Visibility::Public,
                definition_scope_id: 0,
            };
            let result = table.register_method("TestType", sig, Visibility::Public, false);
            assert!(result.is_ok());
            let method_info = table.lookup_method("TestType", "constructor");
            assert!(method_info.is_some());
            let method_info = method_info.unwrap();
            assert!(!method_info.has_self);
            assert!(!method_info.is_instance_method());
        }

        #[test]
        fn method_info_accessor_consistent_with_field() {
            let instance_method = MethodInfo {
                signature: FuncInfo {
                    name: "test".to_string(),
                    type_params: vec![],
                    param_types: vec![],
                    return_type: TypeInfo::default(),
                    visibility: Visibility::Private,
                    definition_scope_id: 0,
                },
                visibility: Visibility::Private,
                scope_id: 0,
                has_self: true,
            };
            let associated_fn = MethodInfo {
                signature: FuncInfo {
                    name: "test".to_string(),
                    type_params: vec![],
                    param_types: vec![],
                    return_type: TypeInfo::default(),
                    visibility: Visibility::Private,
                    definition_scope_id: 0,
                },
                visibility: Visibility::Private,
                scope_id: 0,
                has_self: false,
            };
            // Verify accessor returns same value as field
            assert_eq!(
                instance_method.is_instance_method(),
                instance_method.has_self
            );
            assert_eq!(associated_fn.is_instance_method(), associated_fn.has_self);
        }
    }
}
