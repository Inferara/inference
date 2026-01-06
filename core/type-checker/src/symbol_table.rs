use std::cell::RefCell;
use std::rc::Rc;

use anyhow::bail;

use crate::type_info::TypeInfo;
use inference_ast::nodes::{ModuleDefinition, Type, Visibility};
use rustc_hash::FxHashMap;

pub(crate) type ScopeRef = Rc<RefCell<Scope>>;

#[derive(Debug, Clone)]
pub(crate) struct FuncSignature {
    pub(crate) name: String,
    pub(crate) type_params: Vec<String>,
    pub(crate) param_types: Vec<TypeInfo>,
    pub(crate) return_type: TypeInfo,
}

/// Information about a struct field. Fields `name` and `visibility` are tracked
/// for future phases (visibility checking, field access validation).
#[derive(Debug, Clone)]
pub(crate) struct StructFieldInfo {
    #[allow(dead_code)]
    pub(crate) name: String,
    pub(crate) type_info: TypeInfo,
    #[allow(dead_code)]
    pub(crate) visibility: Visibility,
}

/// Information about a struct type. Field `visibility` is tracked for Phase 4+
/// visibility checking during member access.
#[derive(Debug, Clone)]
pub(crate) struct StructInfo {
    pub(crate) name: String,
    pub(crate) fields: FxHashMap<String, StructFieldInfo>,
    pub(crate) type_params: Vec<String>,
    #[allow(dead_code)]
    pub(crate) visibility: Visibility,
}

/// Information about a method defined on a type.
/// Fields visibility, scope_id, and has_self are populated for future phases:
/// - visibility: for Phase 4+ visibility checking during method resolution
/// - scope_id: to track which scope defines this method for qualified lookup
/// - has_self: to distinguish instance methods from associated functions
#[derive(Debug, Clone)]
pub(crate) struct MethodInfo {
    pub(crate) signature: FuncSignature,
    #[allow(dead_code)]
    pub(crate) visibility: Visibility,
    #[allow(dead_code)]
    pub(crate) scope_id: u32,
    #[allow(dead_code)]
    pub(crate) has_self: bool,
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
    /// Glob import: `use path::*` (Phase 5 - not yet implemented)
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
    Type(TypeInfo),
    Struct(StructInfo),
    Enum(String),
    Spec(String),
    Function(FuncSignature),
}

impl Symbol {
    #[allow(dead_code)]
    #[must_use = "discarding the name has no effect"]
    pub(crate) fn name(&self) -> String {
        match self {
            Symbol::Type(ti) => ti.to_string(),
            Symbol::Struct(info) => info.name.clone(),
            Symbol::Enum(name) | Symbol::Spec(name) => name.clone(),
            Symbol::Function(sig) => sig.name.clone(),
        }
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn as_function(&self) -> Option<&FuncSignature> {
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

    #[must_use = "this is a pure conversion with no side effects"]
    pub(crate) fn as_type_info(&self) -> Option<TypeInfo> {
        match self {
            Symbol::Type(ti) => Some(ti.clone()),
            Symbol::Struct(info) => Some(TypeInfo {
                kind: crate::type_info::TypeInfoKind::Struct(info.name.clone()),
                type_params: info.type_params.clone(),
            }),
            Symbol::Enum(name) => Some(TypeInfo {
                kind: crate::type_info::TypeInfoKind::Enum(name.clone()),
                type_params: vec![],
            }),
            Symbol::Spec(name) => Some(TypeInfo {
                kind: crate::type_info::TypeInfoKind::Spec(name.clone()),
                type_params: vec![],
            }),
            Symbol::Function(_) => None,
        }
    }
}

/// A scope in the symbol table tree.
#[derive(Debug)]
pub(crate) struct Scope {
    pub(crate) id: u32,
    pub(crate) name: String,
    /// Visibility of this scope (used in Phase 4+ visibility checking)
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
        visibility: Visibility,
        parent: Option<ScopeRef>,
    ) -> ScopeRef {
        Rc::new(RefCell::new(Self {
            id,
            name: name.to_string(),
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
    pub(crate) fn lookup_method(
        &self,
        type_name: &str,
        method_name: &str,
    ) -> Option<MethodInfo> {
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

    /// Look up a name in resolved imports (used in resolve_name for Phase 4+ name lookup)
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
        let root = Scope::new(self.next_scope_id, "root", Visibility::Public, None);
        self.scopes.insert(self.next_scope_id, Rc::clone(&root));
        self.mod_scopes.insert("root".to_string(), Rc::clone(&root));
        self.next_scope_id += 1;
        self.root_scope = Some(Rc::clone(&root));
        self.current_scope = Some(root);
    }

    fn init_builtin_types(&mut self) {
        use crate::type_info::{NumberTypeKindNumberType, TypeInfoKind};

        let builtins = [
            ("i8", TypeInfoKind::Number(NumberTypeKindNumberType::I8)),
            ("i16", TypeInfoKind::Number(NumberTypeKindNumberType::I16)),
            ("i32", TypeInfoKind::Number(NumberTypeKindNumberType::I32)),
            ("i64", TypeInfoKind::Number(NumberTypeKindNumberType::I64)),
            ("u8", TypeInfoKind::Number(NumberTypeKindNumberType::U8)),
            ("u16", TypeInfoKind::Number(NumberTypeKindNumberType::U16)),
            ("u32", TypeInfoKind::Number(NumberTypeKindNumberType::U32)),
            ("u64", TypeInfoKind::Number(NumberTypeKindNumberType::U64)),
            ("bool", TypeInfoKind::Bool),
            ("string", TypeInfoKind::String),
        ];

        if let Some(scope) = &self.current_scope {
            let mut scope_mut = scope.borrow_mut();
            for (name, kind) in builtins {
                let type_info = TypeInfo {
                    kind,
                    type_params: vec![],
                };
                let _ = scope_mut.insert_symbol(name, Symbol::Type(type_info));
            }
        }
    }

    pub(crate) fn push_scope(&mut self) -> u32 {
        self.push_scope_with_name("anonymous", Visibility::Private)
    }

    pub(crate) fn push_scope_with_name(&mut self, name: &str, visibility: Visibility) -> u32 {
        let parent = self.current_scope.clone();
        let scope_id = self.next_scope_id;
        self.next_scope_id += 1;

        let new_scope = Scope::new(scope_id, name, visibility, parent.clone());

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
                .insert_symbol(name, Symbol::Type(type_info))
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
            };
            scope
                .borrow_mut()
                .insert_symbol(name, Symbol::Struct(struct_info))
        } else {
            bail!("No active scope to register struct")
        }
    }

    pub(crate) fn register_enum(&mut self, name: &str) -> anyhow::Result<()> {
        if let Some(scope) = &self.current_scope {
            scope
                .borrow_mut()
                .insert_symbol(name, Symbol::Enum(name.to_string()))
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
        if let Some(scope) = &self.current_scope {
            let sig = FuncSignature {
                name: name.to_string(),
                type_params,
                param_types: param_types.iter().map(TypeInfo::new).collect(),
                return_type: TypeInfo::new(return_type),
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
    pub(crate) fn lookup_function(&self, name: &str) -> Option<FuncSignature> {
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
    pub(crate) fn lookup_struct_field(
        &self,
        struct_name: &str,
        field_name: &str,
    ) -> Option<TypeInfo> {
        self.lookup_struct(struct_name).and_then(|struct_info| {
            struct_info
                .fields
                .get(field_name)
                .map(|f| f.type_info.clone())
        })
    }

    pub(crate) fn register_method(
        &mut self,
        type_name: &str,
        signature: FuncSignature,
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
    pub(crate) fn lookup_method(
        &self,
        type_name: &str,
        method_name: &str,
    ) -> Option<MethodInfo> {
        self.current_scope
            .as_ref()
            .and_then(|scope| scope.borrow().lookup_method(type_name, method_name))
    }

    #[must_use = "returns the scope ID which may be needed for later reference"]
    pub(crate) fn enter_module(&mut self, module: &Rc<ModuleDefinition>) -> u32 {
        let scope_id = self.push_scope_with_name(&module.name(), module.visibility.clone());
        if let Some(scope) = self.scopes.get(&scope_id) {
            self.mod_scopes.insert(module.name(), Rc::clone(scope));
        }
        scope_id
    }

    #[allow(dead_code)]
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn current_scope_id(&self) -> Option<u32> {
        self.current_scope.as_ref().map(|s| s.borrow().id)
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn get_scope(&self, scope_id: u32) -> Option<ScopeRef> {
        self.scopes.get(&scope_id).cloned()
    }

    /// Register an import in the current scope (Phase A: registration)
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

    /// Resolve a qualified name (e.g., ["mod1", "Type"]) from a given scope.
    /// Returns the symbol and its defining scope ID for visibility checking.
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

    /// Resolve a name considering local symbols and resolved imports.
    /// Priority: local symbols > parent symbols > resolved imports.
    /// Uses iteration to avoid stack overflow on deep scope trees.
    /// (Used in Phase 4+ for name resolution with import awareness)
    #[allow(dead_code)]
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn resolve_name(&self, name: &str) -> Option<(Symbol, u32)> {
        let mut current_scope = self.current_scope.clone()?;

        loop {
            {
                let scope_ref = current_scope.borrow();
                if let Some(symbol) = scope_ref.lookup_symbol_local(name) {
                    return Some((symbol.clone(), scope_ref.id));
                }
                if let Some(resolved) = scope_ref.lookup_resolved_import(name) {
                    return Some((resolved.symbol.clone(), resolved.definition_scope_id));
                }
            }

            let parent = current_scope.borrow().parent.clone();
            match parent {
                Some(p) => current_scope = p,
                None => break,
            }
        }

        None
    }
}
