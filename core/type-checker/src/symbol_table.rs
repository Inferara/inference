//! Symbol Table
//!
//! Tree-based symbol table for managing scopes and symbols during type
//! checking. It supports:
//!
//! - Hierarchical scopes with parent-child relationships
//! - Type alias, struct, enum, spec, and function symbol registration
//! - Variable tracking within scopes
//! - Method resolution on types
//! - Import registration and resolution
//! - Visibility checking for access control
//!
//! Scopes form a tree where each scope can have multiple child scopes. Each
//! child holds a `Weak` reference to its parent; the [`SymbolTable`] owns the
//! strong references via its `scopes` map.
//!
//! ## Scope Tree Traversal
//!
//! `lookup_symbol`, `lookup_variable`, `lookup_variable_is_mut`, and
//! `lookup_method` first check the current scope locally; on a miss they
//! upgrade the parent `Weak` and recurse, terminating when either a match is
//! found or the root scope (which has no parent) is reached. Cross-scope
//! variable lookup is what lets an inner block read an outer-scope variable;
//! it does *not* enable shadowing — re-declaring an existing name in an
//! inner scope is rejected by the type checker as
//! [`TypeCheckError::VariableShadowed`].
//!
//! ## Default Return Types
//!
//! Functions without an explicit return type default to the unit type,
//! represented as `TypeInfo { kind: TypeInfoKind::Unit, type_params: vec![] }`.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Weak;

use std::sync::Arc;

use anyhow::bail;

use crate::type_info::{TypeInfo, TypeInfoKind};
use inference_ast::arena::AstArena;
use inference_ast::ids::DefId;
use inference_ast::nodes::{ArgKind, Def, Location, Visibility};
use rustc_hash::FxHashMap;

pub(crate) type ScopeRef = Arc<RefCell<Scope>>;
pub(crate) type WeakScopeRef = Weak<RefCell<Scope>>;

/// Provenance of an `external fn` declaration: the logical module that exports
/// it, the export field name to bind against, and (once the driver resolves it)
/// the concrete `.wasm` path.
///
/// `logical_module` and `export_field` are platform-independent: they come from
/// the `use { field } from logical::module;` clause that names the extern, not
/// from any filesystem path. `resolved_path` stays `None` until the driver maps
/// the logical module to a file; later phases populate it for the linker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternOrigin {
    /// Logical, `::`-joined module reference from the binding `use` clause
    /// (e.g. `"crypto::sha256"`). Never a filesystem path.
    pub logical_module: String,
    /// Export field name to bind against in the resolved module. Equals the
    /// extern's declared name; carried explicitly so renaming-on-import can
    /// diverge the two in a later phase without changing the data model.
    pub export_field: String,
    /// The `external fn` declaration this binding attaches to.
    ///
    /// Two same-named externs (e.g. a top-level and a spec-inner `sort` with
    /// divergent signatures) would otherwise collapse together when keyed by
    /// bare name. Carrying the declaring [`DefId`] lets the driver recover the
    /// exact declared signature to validate against — never a same-named
    /// sibling — and lets analysis resolve each call to the specific extern it
    /// names.
    pub decl: DefId,
    /// Concrete `.wasm` path once the driver resolves `logical_module`.
    /// `None` during type checking; populated downstream.
    pub resolved_path: Option<PathBuf>,
}

/// Whether a registered function is local or an `external fn`, and — for an
/// extern — whether it was bound to a source module via a `use … from` clause.
///
/// This discriminates the three states that otherwise collapse together: a
/// local function, an unbound extern (declared without a binding `use`), and a
/// bound extern (carrying its [`ExternOrigin`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum FuncKind {
    /// An ordinary function defined in this program.
    #[default]
    Local,
    /// An `external fn`. `Some` once bound to a source module via a
    /// `use … from` clause; `None` while unbound.
    Extern(Option<ExternOrigin>),
}

#[derive(Debug, Clone)]
pub(crate) struct FuncInfo {
    pub(crate) name: String,
    pub(crate) type_params: Vec<String>,
    pub(crate) param_types: Vec<TypeInfo>,
    pub(crate) return_type: TypeInfo,
    pub(crate) visibility: Visibility,
    pub(crate) definition_scope_id: u32,
    /// Source location of the declaration, used to point a cross-file
    /// private-access diagnostic at the definition site. `Default` (a zero span)
    /// for synthetic functions that have no source (builtins, externals loaded
    /// from a prelude, test fixtures).
    pub(crate) definition_location: Location,
    /// Local function, unbound extern, or bound extern. See [`FuncKind`].
    pub(crate) kind: FuncKind,
}

impl FuncInfo {
    /// Returns true if this is an `external fn`, bound or unbound.
    #[must_use = "this is a pure check with no side effects"]
    pub(crate) fn is_extern(&self) -> bool {
        matches!(self.kind, FuncKind::Extern(_))
    }

    /// Returns the provenance of this function if it is a *bound* extern.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn extern_origin(&self) -> Option<&ExternOrigin> {
        match &self.kind {
            FuncKind::Extern(origin) => origin.as_ref(),
            FuncKind::Local => None,
        }
    }
}

/// Information about a struct field.
///
/// Fields carry no visibility of their own: a field is accessible exactly when
/// its enclosing struct is accessible from the referencing file (#63). The
/// access check therefore consults [`StructInfo::visibility`] and
/// [`StructInfo::definition_scope_id`], never a per-field flag.
#[derive(Debug, Clone)]
pub struct StructFieldInfo {
    pub name: String,
    pub type_info: TypeInfo,
}

/// Information about a struct type. Visibility and definition_scope_id are used
/// for visibility checking during member access.
#[derive(Debug, Clone)]
pub struct StructInfo {
    pub name: String,
    pub fields: Vec<StructFieldInfo>,
    pub type_params: Vec<String>,
    pub visibility: Visibility,
    pub definition_scope_id: u32,
    /// Source location of the declaration, used to point a cross-file
    /// private-access diagnostic at the definition site. `Default` for
    /// synthetic structs without source (test fixtures, prelude externals).
    pub definition_location: Location,
}

impl StructInfo {
    pub fn get_field_info_by_name(&self, name: &str) -> Option<&StructFieldInfo> {
        self.fields.iter().find(|f| f.name == name)
    }
}

/// Information about an enum type including its variants.
/// Simple unit variants only - associated data support is out of scope.
/// Visibility and definition_scope_id are used for visibility checking during variant access.
///
/// Variants are stored as a `Vec<String>` in declaration order to ensure
/// deterministic zero-based tag assignment for WASM codegen.
#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub name: String,
    pub variants: Vec<String>,
    pub visibility: Visibility,
    pub definition_scope_id: u32,
    /// Source location of the declaration, used to point a cross-file
    /// private-access diagnostic at the definition site. `Default` for
    /// synthetic enums without source (test fixtures, prelude externals).
    pub definition_location: Location,
}

impl EnumInfo {
    /// Returns the zero-based tag index for a variant name, or `None` if
    /// the variant does not belong to this enum.
    #[inline]
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn variant_index(&self, variant_name: &str) -> Option<usize> {
        self.variants.iter().position(|v| v == variant_name)
    }
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

/// The kind of import statement.
///
/// A brace-free `use a::b;` is a [`Self::Plain`] *file* import (it binds the
/// namespace `b`); a braced `use a::b::{x, y};` is a [`Self::Partial`] *item*
/// import. The two are unambiguous from syntax alone — the resolver never
/// consults the filesystem to classify a directive. Glob imports are rejected at
/// the parser, so this enum carries no glob variant.
#[derive(Debug, Clone)]
pub(crate) enum ImportKind {
    /// File import: `use a::b;` — binds the namespace named by the last segment.
    Plain,
    /// Item import: `use a::b::{x, y}` — binds each listed item for bare use.
    Partial(Vec<ImportItem>),
}

/// Represents an unresolved import in a scope.
#[derive(Debug, Clone)]
pub(crate) struct Import {
    /// The path segments of the import (e.g., ["lib", "arith"]).
    pub(crate) path: Vec<String>,
    /// The kind of import
    pub(crate) kind: ImportKind,
    /// Whether the directive is `pub use` (re-exported from the importing file).
    pub(crate) visibility: Visibility,
    /// Source location of the import statement
    pub(crate) location: Location,
}

/// What a resolved import name binds to in the importing file's scope.
#[derive(Debug, Clone)]
pub(crate) enum ResolvedImportTarget {
    /// A file import (`use a::b;`) binds the name to the namespace scope `a::b`,
    /// so a later `b::item` access resolves `item` inside that scope.
    Namespace { scope_id: u32 },
    /// An item import (`use a::b::{x};`) binds the name to the item `x` itself,
    /// usable bare. The scope id is the item's defining scope (for diagnostics).
    /// The symbol is boxed because it is far larger than a namespace's scope id.
    Item {
        symbol: Box<Symbol>,
        definition_scope_id: u32,
    },
}

/// Represents a resolved import binding in a scope.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedImport {
    /// The local name the import binds (the last path segment, or an item name).
    pub(crate) local_name: String,
    /// What the name resolves to (a namespace scope or a concrete item).
    pub(crate) target: ResolvedImportTarget,
    /// `true` when the directive is `pub use`: the binding is re-exported, so an
    /// importer of *this* file may traverse through it. A plain `use` binding is
    /// private to the importing file and is not followed across files.
    pub(crate) reexported: bool,
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

    /// Source location of this symbol's declaration, used to point a cross-file
    /// private-access diagnostic at the definition. Type aliases and specs carry
    /// no location and report a zero span.
    #[must_use = "the location is the return value"]
    pub(crate) fn definition_location(&self) -> Location {
        match self {
            Symbol::Struct(info) => info.definition_location,
            Symbol::Enum(info) => info.definition_location,
            Symbol::Function(sig) => sig.definition_location,
            Symbol::TypeAlias(_) | Symbol::Spec(_) => Location::default(),
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
    pub(crate) parent: Option<WeakScopeRef>,
    pub(crate) children: Vec<ScopeRef>,
    pub(crate) symbols: FxHashMap<String, Symbol>,
    pub(crate) variables: FxHashMap<String, (u32, TypeInfo, bool)>,
    pub(crate) methods: FxHashMap<String, Vec<MethodInfo>>,
    /// Unresolved imports registered in this scope
    pub(crate) imports: Vec<Import>,
    /// Resolved import bindings (populated after resolution phase)
    pub(crate) resolved_imports: FxHashMap<String, ResolvedImport>,
}

impl Scope {
    #[allow(clippy::arc_with_non_send_sync)]
    #[must_use = "scope constructor returns a new scope that should be used"]
    pub(crate) fn new(
        id: u32,
        name: &str,
        full_path: String,
        visibility: Visibility,
        parent: Option<WeakScopeRef>,
    ) -> ScopeRef {
        Arc::new(RefCell::new(Self {
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
        if let Some(parent) = self.parent.as_ref().and_then(|p| p.upgrade()) {
            return parent.borrow().lookup_symbol(name);
        }
        None
    }

    pub(crate) fn insert_variable(
        &mut self,
        name: &str,
        node_id: u32,
        ty: TypeInfo,
        is_mut: bool,
    ) -> anyhow::Result<()> {
        if self.variables.contains_key(name) {
            bail!("Variable `{name}` already declared in this scope");
        }
        self.variables
            .insert(name.to_string(), (node_id, ty, is_mut));
        Ok(())
    }

    #[must_use = "this is a pure lookup with no side effects"]
    fn lookup_variable_local(&self, name: &str) -> Option<(u32, TypeInfo, bool)> {
        self.variables.get(name).cloned()
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_variable(&self, name: &str) -> Option<TypeInfo> {
        if let Some((_, ty, _)) = self.lookup_variable_local(name) {
            return Some(ty);
        }
        if let Some(parent) = self.parent.as_ref().and_then(|p| p.upgrade()) {
            return parent.borrow().lookup_variable(name);
        }
        None
    }

    #[must_use = "this is a pure lookup with no side effects"]
    fn lookup_variable_is_mut_local(&self, name: &str) -> Option<bool> {
        self.variables.get(name).map(|(_, _, is_mut)| *is_mut)
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_variable_is_mut(&self, name: &str) -> Option<bool> {
        if let Some(is_mut) = self.lookup_variable_is_mut_local(name) {
            return Some(is_mut);
        }
        if let Some(parent) = self.parent.as_ref().and_then(|p| p.upgrade()) {
            return parent.borrow().lookup_variable_is_mut(name);
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
        if let Some(parent) = self.parent.as_ref().and_then(|p| p.upgrade()) {
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
    spec_scopes: FxHashMap<String, ScopeRef>,
    root_scope: Option<ScopeRef>,
    current_scope: Option<ScopeRef>,
    next_scope_id: u32,
}

impl Default for SymbolTable {
    fn default() -> Self {
        let mut table = SymbolTable {
            scopes: FxHashMap::default(),
            mod_scopes: FxHashMap::default(),
            spec_scopes: FxHashMap::default(),
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
        self.scopes.insert(self.next_scope_id, Arc::clone(&root));
        self.mod_scopes.insert(String::new(), Arc::clone(&root));
        self.next_scope_id += 1;
        self.root_scope = Some(Arc::clone(&root));
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

    /// Create a new child scope under `current_scope` and switch into it.
    ///
    /// Allocates the next `scope_id`, builds the dotted `full_path` from the
    /// parent's path (e.g. `"foo::bar"`), inserts the new scope into both
    /// `scopes` and the parent's `children`, and reassigns `current_scope` to
    /// the new scope. Returns the new `scope_id`.
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

        let new_scope = Scope::new(
            scope_id,
            name,
            full_path,
            visibility,
            parent.as_ref().map(Arc::downgrade),
        );

        if let Some(current) = &parent {
            current.borrow_mut().add_child(Arc::clone(&new_scope));
        }

        self.scopes.insert(scope_id, Arc::clone(&new_scope));
        self.current_scope = Some(new_scope);
        scope_id
    }

    /// Reassign `current_scope` to the parent of the current scope, if any.
    ///
    /// No-op when `current_scope` is `None` or has no parent (i.e. root).
    /// Counterpart to [`Self::push_scope_with_name`].
    pub(crate) fn pop_scope(&mut self) {
        if let Some(current) = &self.current_scope {
            let parent = current.borrow().parent.as_ref().and_then(|p| p.upgrade());
            self.current_scope = parent;
        }
    }

    pub(crate) fn register_type(&mut self, name: &str, ty: Option<TypeInfo>) -> anyhow::Result<()> {
        if let Some(scope) = &self.current_scope {
            let type_info = ty.unwrap_or_else(|| TypeInfo {
                kind: crate::type_info::TypeInfoKind::Custom(name.to_string()),
                type_params: vec![],
            });
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
        fields: &[(String, TypeInfo)],
        type_params: Vec<String>,
        visibility: Visibility,
        location: Location,
    ) -> anyhow::Result<()> {
        if let Some(scope) = &self.current_scope {
            let scope_id = scope.borrow().id;
            let fields = fields
                .iter()
                .map(|(field_name, field_type)| StructFieldInfo {
                    name: field_name.clone(),
                    type_info: field_type.clone(),
                })
                .collect();
            let struct_info = StructInfo {
                name: name.to_string(),
                fields,
                type_params,
                visibility,
                definition_scope_id: scope_id,
                definition_location: location,
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
        location: Location,
    ) -> anyhow::Result<()> {
        if let Some(scope) = &self.current_scope {
            let scope_id = scope.borrow().id;
            let enum_info = EnumInfo {
                name: name.to_string(),
                variants: variants.iter().map(|s| (*s).to_string()).collect(),
                visibility,
                definition_scope_id: scope_id,
                definition_location: location,
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

    /// Resolve `TypeInfoKind::Custom(name)` to `Struct(name)` or `Enum(name)`
    /// by looking up the name in the symbol table. Falls through to `Custom`
    /// if the name is not found (e.g., forward references in nested modules).
    /// Recurses into array element types.
    #[must_use = "returns the resolved type; discarding it loses the resolution"]
    pub(crate) fn resolve_custom_type(&self, mut ti: TypeInfo) -> TypeInfo {
        match &ti.kind {
            TypeInfoKind::Custom(name) => {
                if self.lookup_struct(name).is_some() {
                    ti.kind = TypeInfoKind::Struct(name.clone());
                } else if self.lookup_enum(name).is_some() {
                    ti.kind = TypeInfoKind::Enum(name.clone());
                }
                ti
            }
            TypeInfoKind::Array(elem, size) => {
                let resolved_elem = self.resolve_custom_type(*elem.clone());
                ti.kind = TypeInfoKind::Array(Box::new(resolved_elem), *size);
                ti
            }
            _ => ti,
        }
    }

    /// Registers a private local function. A thin default-visibility wrapper
    /// over [`Self::register_function_with_visibility`], kept for test setup that
    /// does not care about visibility.
    #[cfg(test)]
    pub(crate) fn register_function(
        &mut self,
        name: &str,
        type_params: Vec<String>,
        param_types: Vec<TypeInfo>,
        return_type: TypeInfo,
    ) -> Result<(), String> {
        self.register_function_with_visibility(
            name,
            type_params,
            param_types,
            return_type,
            Visibility::Private,
            Location::default(),
        )
    }

    pub(crate) fn register_function_with_visibility(
        &mut self,
        name: &str,
        type_params: Vec<String>,
        param_types: Vec<TypeInfo>,
        return_type: TypeInfo,
        visibility: Visibility,
        location: Location,
    ) -> Result<(), String> {
        self.insert_func_symbol(
            name,
            type_params,
            param_types,
            return_type,
            visibility,
            location,
            FuncKind::Local,
        )
    }

    /// Registers an `external fn`, discriminating it from a local function.
    ///
    /// `origin` carries the binding module and export field when the extern is
    /// named by a `use … from` clause; it is `None` for an extern declared
    /// without a binding `use`. Either way the function is recorded as
    /// [`FuncKind::Extern`], so an unbound extern stays distinguishable from a
    /// local function.
    pub(crate) fn register_extern_function(
        &mut self,
        name: &str,
        param_types: Vec<TypeInfo>,
        return_type: TypeInfo,
        origin: Option<ExternOrigin>,
    ) -> Result<(), String> {
        self.insert_func_symbol(
            name,
            vec![],
            param_types,
            return_type,
            Visibility::Private,
            Location::default(),
            FuncKind::Extern(origin),
        )
    }

    fn insert_func_symbol(
        &mut self,
        name: &str,
        type_params: Vec<String>,
        param_types: Vec<TypeInfo>,
        return_type: TypeInfo,
        visibility: Visibility,
        location: Location,
        kind: FuncKind,
    ) -> Result<(), String> {
        if let Some(scope) = &self.current_scope {
            let scope_id = scope.borrow().id;
            let sig = FuncInfo {
                name: name.to_string(),
                type_params,
                param_types: param_types
                    .into_iter()
                    .map(|ti| self.resolve_custom_type(ti))
                    .collect(),
                return_type: self.resolve_custom_type(return_type),
                visibility,
                definition_scope_id: scope_id,
                definition_location: location,
                kind,
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
        is_mut: bool,
    ) -> anyhow::Result<()> {
        if let Some(scope) = &self.current_scope {
            scope
                .borrow_mut()
                .insert_variable(name, 0, var_type, is_mut)
        } else {
            bail!("No active scope to push variable")
        }
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_type(&self, name: &str) -> Option<TypeInfo> {
        if let Some(scope) = &self.current_scope
            && let Some(symbol) = scope.borrow().lookup_symbol(name)
        {
            return symbol.as_type_info();
        }
        // A bare type reference may name a struct/enum brought in by
        // `use a::b::{T};`; the import binds `T` as a resolved item in the file
        // scope, so consult those when no like-named symbol is in scope.
        self.lookup_imported_item_symbol(name)
            .and_then(|symbol| symbol.as_type_info())
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_variable(&self, name: &str) -> Option<TypeInfo> {
        self.current_scope
            .as_ref()
            .and_then(|scope| scope.borrow().lookup_variable(name))
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_variable_is_mut(&self, name: &str) -> Option<bool> {
        self.current_scope
            .as_ref()
            .and_then(|scope| scope.borrow().lookup_variable_is_mut(name))
    }

    /// Checks whether a variable exists in any parent scope (skipping the current scope).
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_variable_in_parent_scopes(&self, name: &str) -> Option<TypeInfo> {
        self.current_scope.as_ref().and_then(|scope| {
            let scope = scope.borrow();
            scope
                .parent
                .as_ref()
                .and_then(|p| p.upgrade())
                .and_then(|parent| parent.borrow().lookup_variable(name))
        })
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_function(&self, name: &str) -> Option<FuncInfo> {
        let from_symbol = self
            .current_scope
            .as_ref()
            .and_then(|scope| scope.borrow().lookup_symbol(name))
            .and_then(|symbol| symbol.as_function().cloned());
        if from_symbol.is_some() {
            return from_symbol;
        }
        // A bare call may name an item brought in by `use a::b::{f};`; the import
        // binds `f` as a resolved item in the file scope, so consult those when
        // no like-named symbol is in scope.
        self.lookup_imported_item_symbol(name)
            .and_then(|symbol| symbol.as_function().cloned())
    }

    /// Looks up a resolved item import named `name`, walking the current scope's
    /// parent chain. Returns the imported symbol, letting bare references resolve
    /// the items an `use a::b::{x};` brought into the file.
    #[must_use = "this is a pure lookup with no side effects"]
    fn lookup_imported_item_symbol(&self, name: &str) -> Option<Symbol> {
        let mut scope = self.current_scope.clone();
        while let Some(s) = scope {
            if let Some(resolved) = s.borrow().resolved_imports.get(name)
                && let ResolvedImportTarget::Item { symbol, .. } = &resolved.target
            {
                return Some((**symbol).clone());
            }
            scope = s.borrow().parent.as_ref().and_then(|p| p.upgrade());
        }
        None
    }

    /// Looks up a function by name in the root scope only, without walking
    /// the parent chain. Used to detect spec-inner / top-level shadowing
    /// independently of the current scope cursor.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_function_in_root(&self, name: &str) -> Option<FuncInfo> {
        let root = self.scopes.get(&0)?;
        let symbol = root.borrow().lookup_symbol_local(name).cloned()?;
        symbol.as_function().cloned()
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_struct(&self, name: &str) -> Option<StructInfo> {
        let from_symbol = self
            .current_scope
            .as_ref()
            .and_then(|scope| scope.borrow().lookup_symbol(name))
            .and_then(|symbol| symbol.as_struct().cloned());
        if from_symbol.is_some() {
            return from_symbol;
        }
        // Resolve a struct brought in by `use a::b::{S};` for bare use.
        self.lookup_imported_item_symbol(name)
            .and_then(|symbol| symbol.as_struct().cloned())
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_enum(&self, name: &str) -> Option<EnumInfo> {
        let from_symbol = self
            .current_scope
            .as_ref()
            .and_then(|scope| scope.borrow().lookup_symbol(name))
            .and_then(|symbol| symbol.as_enum().cloned());
        if from_symbol.is_some() {
            return from_symbol;
        }
        // Resolve an enum brought in by `use a::b::{E};` for bare use.
        self.lookup_imported_item_symbol(name)
            .and_then(|symbol| symbol.as_enum().cloned())
    }

    /// Looks up a struct by name across **all** registered scopes, in ascending
    /// scope-id order (root wins on a name clash).
    ///
    /// This is a deliberate global escape hatch, used only where a name must be
    /// checked for existence regardless of file scope — e.g. rejecting a
    /// spec-inner struct that shadows any other struct. Type-reference and
    /// codegen resolution go through [`Self::resolve_struct_in_scope`] /
    /// canonical keys instead, so they never collapse same-named structs from
    /// different files.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_struct_anywhere(&self, name: &str) -> Option<StructInfo> {
        let mut ids: Vec<u32> = self.scopes.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            if let Some(scope) = self.scopes.get(&id)
                && let Some(symbol) = scope.borrow().lookup_symbol_local(name)
                && let Some(info) = symbol.as_struct()
            {
                return Some(info.clone());
            }
        }
        None
    }

    /// Builds the canonical key for a type named `name` defined in `scope_id`:
    /// the `::`-joined path of the type's enclosing **file** prefixed onto the
    /// name, or the bare name when that file is the entry file (root, empty path).
    ///
    /// The entry file maps to the root scope, so its types keep bare keys and a
    /// single-file program's keys are byte-identical to the pre-file-scope
    /// world. A non-entry file `["lib", "arith"]` defines `Point` under scope
    /// `lib::arith`, yielding the canonical key `lib::arith::Point` — distinct
    /// from a same-named `Point` in any other file.
    ///
    /// A type defined inside a `spec` is keyed by its enclosing file, not the
    /// spec's sub-scope: spec types are unique by bare name within a file (a
    /// same-named spec struct is rejected at registration), so they qualify by
    /// file exactly like top-level types and stay reachable by bare name in a
    /// single-file program.
    #[must_use = "the canonical key is the return value"]
    pub(crate) fn canonical_key_for_scope(&self, scope_id: u32, name: &str) -> String {
        let path = self.enclosing_file_path(scope_id);
        if path.is_empty() {
            name.to_string()
        } else {
            format!("{path}::{name}")
        }
    }

    /// Returns the `::`-joined module path of the nearest enclosing **file**
    /// scope of `scope_id`, walking up the parent chain. A file scope is one
    /// registered in `mod_scopes` (the entry file is the root, keyed by the empty
    /// string). Spec and anonymous block scopes are transparent — their types
    /// belong to the enclosing file for canonical-key purposes.
    fn enclosing_file_path(&self, scope_id: u32) -> String {
        let file_ids: BTreeSet<u32> = self.mod_scopes.values().map(|s| s.borrow().id).collect();
        let mut current = self.get_scope(scope_id);
        while let Some(scope) = current {
            let id = scope.borrow().id;
            if file_ids.contains(&id) {
                return scope.borrow().full_path.clone();
            }
            current = scope.borrow().parent.as_ref().and_then(|p| p.upgrade());
        }
        String::new()
    }

    /// Enumerates every registered struct paired with its canonical key, in
    /// ascending scope-id order. The post-type-check store ([`TypedContext`])
    /// folds this into a key-indexed map so codegen and analysis resolve a type
    /// reference to exactly the layout of its defining file — never a same-named
    /// struct from another file.
    #[must_use = "the enumeration is the return value"]
    pub(crate) fn structs_with_canonical_keys(&self) -> Vec<(String, StructInfo)> {
        let mut ids: Vec<u32> = self.scopes.keys().copied().collect();
        ids.sort_unstable();
        let mut out = Vec::new();
        for id in ids {
            let Some(scope) = self.scopes.get(&id) else {
                continue;
            };
            for symbol in scope.borrow().symbols.values() {
                if let Some(info) = symbol.as_struct() {
                    out.push((self.canonical_key_for_scope(id, &info.name), info.clone()));
                }
            }
        }
        out
    }

    /// Enumerates every registered enum paired with its canonical key. Mirrors
    /// [`Self::structs_with_canonical_keys`].
    #[must_use = "the enumeration is the return value"]
    pub(crate) fn enums_with_canonical_keys(&self) -> Vec<(String, EnumInfo)> {
        let mut ids: Vec<u32> = self.scopes.keys().copied().collect();
        ids.sort_unstable();
        let mut out = Vec::new();
        for id in ids {
            let Some(scope) = self.scopes.get(&id) else {
                continue;
            };
            for symbol in scope.borrow().symbols.values() {
                if let Some(info) = symbol.as_enum() {
                    out.push((self.canonical_key_for_scope(id, &info.name), info.clone()));
                }
            }
        }
        out
    }

    /// Resolves a struct type reference `name` as seen from `from_scope_id`,
    /// returning the struct and its canonical key. Resolution honors file
    /// boundaries and visibility: the name is looked up along the referencing
    /// scope's parent chain (own file, then root), so a non-entry file sees its
    /// own private structs but never another file's private struct. A bare name
    /// that names an item brought in by `use a::b::{S};` resolves through that
    /// import binding.
    ///
    /// The canonical key is computed from the struct's *defining* scope, so two
    /// same-named structs in different files resolve to distinct keys.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn resolve_struct_in_scope(
        &self,
        name: &str,
        from_scope_id: u32,
    ) -> Option<(StructInfo, String)> {
        let scope = self.get_scope(from_scope_id)?;
        if let Some(symbol) = scope.borrow().lookup_symbol(name)
            && let Some(info) = symbol.as_struct()
        {
            let key = self.canonical_key_for_scope(info.definition_scope_id, &info.name);
            return Some((info.clone(), key));
        }
        if let Symbol::Struct(info) = self.lookup_imported_item_symbol_from(name, from_scope_id)? {
            let key = self.canonical_key_for_scope(info.definition_scope_id, &info.name);
            return Some((info, key));
        }
        None
    }

    /// Resolves an enum type reference. Mirrors [`Self::resolve_struct_in_scope`].
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn resolve_enum_in_scope(
        &self,
        name: &str,
        from_scope_id: u32,
    ) -> Option<(EnumInfo, String)> {
        let scope = self.get_scope(from_scope_id)?;
        if let Some(symbol) = scope.borrow().lookup_symbol(name)
            && let Some(info) = symbol.as_enum()
        {
            let key = self.canonical_key_for_scope(info.definition_scope_id, &info.name);
            return Some((info.clone(), key));
        }
        if let Symbol::Enum(info) = self.lookup_imported_item_symbol_from(name, from_scope_id)? {
            let key = self.canonical_key_for_scope(info.definition_scope_id, &info.name);
            return Some((info, key));
        }
        None
    }

    /// Looks up a resolved item import named `name`, walking the parent chain of
    /// `from_scope_id` (rather than the symbol table's current cursor). Returns
    /// the imported symbol so a type reference can resolve an item brought in by
    /// `use a::b::{S};`.
    #[must_use = "this is a pure lookup with no side effects"]
    fn lookup_imported_item_symbol_from(&self, name: &str, from_scope_id: u32) -> Option<Symbol> {
        let mut scope = self.get_scope(from_scope_id);
        while let Some(s) = scope {
            if let Some(resolved) = s.borrow().resolved_imports.get(name)
                && let ResolvedImportTarget::Item { symbol, .. } = &resolved.target
            {
                return Some((**symbol).clone());
            }
            scope = s.borrow().parent.as_ref().and_then(|p| p.upgrade());
        }
        None
    }

    /// Looks up an enum by name across **all** registered scopes. Mirrors
    /// [`Self::lookup_struct_anywhere`].
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_enum_anywhere(&self, name: &str) -> Option<EnumInfo> {
        let mut ids: Vec<u32> = self.scopes.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            if let Some(scope) = self.scopes.get(&id)
                && let Some(symbol) = scope.borrow().lookup_symbol_local(name)
                && let Some(info) = symbol.as_enum()
            {
                return Some(info.clone());
            }
        }
        None
    }

    /// Collects the provenance of every **bound** `external fn` across all
    /// scopes, deduplicated by `(logical_module, export_field)`.
    ///
    /// The driver consumes this to resolve and validate each external `.wasm`
    /// before linking. Unbound bare externs (declared without a binding `use`)
    /// carry no origin and are skipped — they never reach the linker.
    #[must_use = "this enumeration has no side effects"]
    pub(crate) fn extern_origins(&self) -> Vec<ExternOrigin> {
        let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
        let mut origins = Vec::new();
        let mut ids: Vec<u32> = self.scopes.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let Some(scope) = self.scopes.get(&id) else {
                continue;
            };
            for symbol in scope.borrow().symbols.values() {
                let Some(info) = symbol.as_function() else {
                    continue;
                };
                let Some(origin) = info.extern_origin() else {
                    continue;
                };
                let key = (origin.logical_module.clone(), origin.export_field.clone());
                if seen.insert(key) {
                    origins.push(origin.clone());
                }
            }
        }
        origins
    }

    /// Returns the provenance of the **bound** `external fn` declared by
    /// `decl`, resolving strictly by declaration identity rather than by name.
    ///
    /// Two same-named externs (a top-level and a spec-inner `f`) register under
    /// the same bare name in different scopes; a name keyed lookup would return
    /// whichever the scope walk reaches first, masking which declaration is
    /// actually bound. Keying on the declaring [`DefId`] lets a caller ask the
    /// precise question "is *this* extern bound?" — the basis for resolving each
    /// call site to the specific extern it names.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn extern_origin_by_decl(&self, decl: DefId) -> Option<ExternOrigin> {
        let mut ids: Vec<u32> = self.scopes.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let Some(scope) = self.scopes.get(&id) else {
                continue;
            };
            for symbol in scope.borrow().symbols.values() {
                let Some(info) = symbol.as_function() else {
                    continue;
                };
                if let Some(origin) = info.extern_origin()
                    && origin.decl == decl
                {
                    return Some(origin.clone());
                }
            }
        }
        None
    }

    /// Looks up a function by name across **all** registered scopes. Mirrors
    /// [`Self::lookup_struct_anywhere`]; the returned [`FuncInfo`] carries the
    /// extern provenance, so post-type-check phases can read it scope-agnostically.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_function_anywhere(&self, name: &str) -> Option<FuncInfo> {
        let mut ids: Vec<u32> = self.scopes.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            if let Some(scope) = self.scopes.get(&id)
                && let Some(symbol) = scope.borrow().lookup_symbol_local(name)
                && let Some(info) = symbol.as_function()
            {
                return Some(info.clone());
            }
        }
        None
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

    /// Enters the scope for spec `name`, creating it on first entry and
    /// re-entering the same scope on subsequent calls. Re-entry preserves
    /// the original scope id so symbols registered across the type checker's
    /// three phases (`register_types`, `collect_function_and_constant_definitions`,
    /// `infer_def`) all land in the same logical scope and are mutually visible.
    pub(crate) fn enter_spec(&mut self, name: &str) -> u32 {
        if let Some(existing) = self.spec_scopes.get(name) {
            let scope_id = existing.borrow().id;
            self.current_scope = Some(Arc::clone(existing));
            return scope_id;
        }
        let scope_id = self.push_scope_with_name(name, Visibility::Public);
        if let Some(scope) = self.scopes.get(&scope_id) {
            self.spec_scopes
                .insert(name.to_string(), Arc::clone(scope));
        }
        scope_id
    }

    /// Enters the scope of the source file named by `module_path`, creating the
    /// chain of named child scopes on first entry and re-entering the leaf on
    /// subsequent calls.
    ///
    /// The entry file (empty `module_path`) maps to the root scope, so a
    /// single-file program registers every definition exactly as it did before
    /// file scopes existed — keeping that path byte-identical. A non-entry file
    /// `["lib", "arith"]` yields a child scope `lib` of root containing a child
    /// scope `arith`; the file's definitions register inside `arith`, and
    /// [`Self::resolve_qualified_name`] walks `lib::arith::add` straight to them.
    ///
    /// Idempotency across the type checker's registration passes
    /// (`process_directives`, `register_types`,
    /// `collect_function_and_constant_definitions`) is essential: each pass
    /// re-enters the same logical leaf so symbols accumulate in one scope rather
    /// than spawning a fresh chain per pass. Scopes are keyed in `mod_scopes` by
    /// their `::`-joined path, the same key [`Self::find_module_scope`] reads.
    ///
    /// Returns the leaf scope id and leaves `current_scope` pointing at it; the
    /// caller restores the previous scope with [`Self::reset_to_root`] (or by
    /// re-entering another file).
    pub(crate) fn enter_file_scope(&mut self, module_path: &[String]) -> u32 {
        self.reset_to_root();
        if module_path.is_empty() {
            return self.current_scope_id().unwrap_or(0);
        }
        for segment in module_path {
            if let Some(existing) = self.child_scope_named(segment) {
                let id = existing.borrow().id;
                self.current_scope = Some(existing);
                let _ = id;
            } else {
                let id = self.push_scope_with_name(segment, Visibility::Public);
                if let Some(scope) = self.scopes.get(&id) {
                    let full_path = scope.borrow().full_path.clone();
                    self.mod_scopes.insert(full_path, Arc::clone(scope));
                }
            }
        }
        self.current_scope_id().unwrap_or(0)
    }

    /// Reassigns `current_scope` to the root scope. Counterpart to the
    /// per-file/per-spec scope walks, letting a registration pass return to a
    /// known anchor before entering the next file.
    pub(crate) fn reset_to_root(&mut self) {
        self.current_scope = self.root_scope.clone();
    }

    /// Returns the direct child scope of `current_scope` whose name matches
    /// `name`, if one exists. Used to make file-scope creation idempotent across
    /// registration passes without re-walking from the root each time.
    #[must_use = "this is a pure lookup with no side effects"]
    fn child_scope_named(&self, name: &str) -> Option<ScopeRef> {
        let current = self.current_scope.as_ref()?;
        current
            .borrow()
            .children
            .iter()
            .find(|c| c.borrow().name == name)
            .cloned()
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn find_module_scope(&self, path: &[String]) -> Option<u32> {
        let key = path.join("::");
        self.mod_scopes.get(&key).map(|s| s.borrow().id)
    }

    /// Returns the `::`-joined module path of the scope `scope_id` — its cached
    /// `full_path`. For a top-level definition's scope this is its defining
    /// file's path (`lib::arith`); the empty string for the entry file (root).
    /// Used to name the defining file in a cross-file private-access diagnostic.
    #[must_use = "the module path is the return value"]
    pub(crate) fn module_path_of_scope(&self, scope_id: u32) -> String {
        self.scopes
            .get(&scope_id)
            .map(|s| s.borrow().full_path.clone())
            .unwrap_or_default()
    }

    /// Returns the scope ancestry of `scope_id` from itself up to the root, in
    /// near-to-far order. Used to resolve a bare definition reference the way
    /// name lookup would: own scope first, then enclosing scopes, then root.
    #[must_use = "the ancestry is the return value"]
    pub(crate) fn scope_ancestry(&self, scope_id: u32) -> Vec<u32> {
        let mut chain = Vec::new();
        let mut current = self.get_scope(scope_id);
        while let Some(scope) = current {
            chain.push(scope.borrow().id);
            current = scope.borrow().parent.as_ref().and_then(|p| p.upgrade());
        }
        chain
    }

    /// Whether any non-entry file namespace exists, i.e. whether the program was
    /// assembled from more than one file. The entry file maps to the root scope
    /// (the empty `mod_scopes` key); a non-empty key is a real imported file.
    /// A file-form `use` that fails to resolve here means no project context, not
    /// a typo.
    #[must_use = "this is a pure check with no side effects"]
    pub(crate) fn has_file_namespaces(&self) -> bool {
        self.mod_scopes.keys().any(|k| !k.is_empty())
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

    /// Resolves a `::`-separated path to the symbol it names and the id of the
    /// scope that defines it, starting the walk from `from_scope_id`.
    ///
    /// The first segment is resolved relative to the accessing scope: it may be
    /// a child namespace scope, a namespace bound by a `use a::b;` in that scope,
    /// an item bound by a `use a::b::{x};`, or — for an absolute path — a child
    /// of the root. Each subsequent intermediate segment must name a child scope
    /// or a **re-exported** (`pub use`) namespace import of the current scope:
    /// re-exports are what let `math::arith::add` reach `lib::arith` after
    /// `math.inf` writes `pub use lib::arith;`. A plain (non-`pub`) import is
    /// visible only to its own file, so it is followed for the *first* segment
    /// but never traversed across a file boundary as an intermediate hop.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn resolve_qualified_name(
        &self,
        path: &[String],
        from_scope_id: u32,
    ) -> Option<(Symbol, u32)> {
        if path.is_empty() {
            return None;
        }

        let (mut current_scope, module_path) = self.start_qualified_walk(path, from_scope_id)?;

        for (i, segment) in module_path.iter().enumerate() {
            let is_last = i == module_path.len() - 1;
            if is_last {
                let scope = current_scope.borrow();
                if let Some(symbol) = scope.lookup_symbol_local(segment) {
                    return Some((symbol.clone(), scope.id));
                }
                if let Some(resolved) = scope.resolved_imports.get(segment)
                    && let ResolvedImportTarget::Item {
                        symbol,
                        definition_scope_id,
                    } = &resolved.target
                {
                    return Some(((**symbol).clone(), *definition_scope_id));
                }
                return None;
            }

            let next = self.next_namespace_scope(&current_scope, segment)?;
            current_scope = next;
        }

        None
    }

    /// Selects the start scope and remaining path for a qualified-name walk.
    ///
    /// A leading `self` anchors the walk at the accessing scope; any other first
    /// segment that names a namespace bound in the accessing scope (a `use a::b;`
    /// import) anchors there too, so a file can reach an imported namespace by
    /// the bound name. Otherwise the walk starts at the root, treating the path
    /// as absolute. The returned slice is the path with any consumed `self`
    /// stripped.
    fn start_qualified_walk<'p>(
        &self,
        path: &'p [String],
        from_scope_id: u32,
    ) -> Option<(ScopeRef, &'p [String])> {
        let first_segment = &path[0];
        if first_segment == "self" {
            return Some((self.get_scope(from_scope_id)?, &path[1..]));
        }
        if let Some(from) = self.get_scope(from_scope_id) {
            let redirect = {
                let from = from.borrow();
                match from.resolved_imports.get(first_segment) {
                    Some(resolved) => match &resolved.target {
                        ResolvedImportTarget::Namespace { scope_id } => Some(*scope_id),
                        ResolvedImportTarget::Item { .. } => None,
                    },
                    None => None,
                }
            };
            if let Some(scope_id) = redirect
                && let Some(target) = self.get_scope(scope_id)
            {
                return Some((target, &path[1..]));
            }
        }
        Some((self.root_scope.clone()?, path))
    }

    /// Advances one intermediate hop of a qualified-name walk: the next scope is
    /// a direct child named `segment`, or a **re-exported** namespace import
    /// named `segment` in `current`.
    ///
    /// The accessing file's own import (which may be private) is consumed by
    /// [`Self::start_qualified_walk`] as the entry point, so by the time this
    /// runs every hop crosses into another file's namespace; a plain (non-`pub`)
    /// import there is private to its own file and is never followed. That is
    /// what makes `math::arith::add` resolve only when `math` writes
    /// `pub use lib::arith;`, not a plain `use`.
    fn next_namespace_scope(&self, current: &ScopeRef, segment: &str) -> Option<ScopeRef> {
        let scope = current.borrow();
        if let Some(child) = scope.children.iter().find(|c| c.borrow().name == segment) {
            return Some(child.clone());
        }
        if let Some(resolved) = scope.resolved_imports.get(segment)
            && let ResolvedImportTarget::Namespace { scope_id } = &resolved.target
            && resolved.reexported
        {
            return self.get_scope(*scope_id);
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
        arena: &AstArena,
    ) -> anyhow::Result<u32> {
        let scope_id = self.push_scope_with_name(module_name, Visibility::Public);

        if let Some(scope) = self.scopes.get(&scope_id) {
            let full_path = scope.borrow().full_path.clone();
            self.mod_scopes.insert(full_path, Arc::clone(scope));
        }

        for sf in arena.source_files() {
            for &def_id in &sf.defs {
                self.register_definition_from_external(module_name, arena, def_id)?;
            }
        }

        self.pop_scope();

        Ok(scope_id)
    }

    /// Register a definition from an external module into the current scope.
    ///
    /// `module_name` is the logical name of the module being loaded; an
    /// `external fn` registered here is bound to it by construction, so its
    /// [`ExternOrigin`] names this module.
    #[allow(dead_code)]
    fn register_definition_from_external(
        &mut self,
        module_name: &str,
        arena: &AstArena,
        def_id: DefId,
    ) -> anyhow::Result<()> {
        let def_data = &arena[def_id];
        let location = def_data.location;
        match &def_data.kind {
            Def::Struct {
                name, vis, fields, ..
            } => {
                let field_infos: Vec<(String, TypeInfo)> = fields
                    .iter()
                    .map(|f| {
                        (
                            arena[f.name].name.clone(),
                            TypeInfo::from_type_id(arena, f.ty),
                        )
                    })
                    .collect();
                self.register_struct(
                    &arena[*name].name,
                    &field_infos,
                    vec![],
                    vis.clone(),
                    location,
                )?;
            }
            Def::Enum {
                name,
                vis,
                variants,
            } => {
                let variant_names: Vec<&str> =
                    variants.iter().map(|v| arena[*v].name.as_str()).collect();
                self.register_enum(&arena[*name].name, &variant_names, vis.clone(), location)?;
            }
            Def::Spec { name, .. } => {
                self.register_spec(&arena[*name].name)?;
            }
            Def::Function {
                name,
                vis,
                type_params,
                args,
                returns,
                ..
            } => {
                let tp_names: Vec<String> =
                    type_params.iter().map(|p| arena[*p].name.clone()).collect();
                let param_types: Vec<TypeInfo> = args
                    .iter()
                    .filter_map(|a| match &a.kind {
                        ArgKind::Named { ty, .. } => Some(TypeInfo::from_type_id_with_type_params(
                            arena, *ty, &tp_names,
                        )),
                        ArgKind::Ignored { ty } => Some(TypeInfo::from_type_id_with_type_params(
                            arena, *ty, &tp_names,
                        )),
                        ArgKind::TypeOnly(ty) => Some(TypeInfo::from_type_id_with_type_params(
                            arena, *ty, &tp_names,
                        )),
                        ArgKind::SelfRef { .. } => None,
                    })
                    .collect();
                let return_type = returns
                    .map(|r| TypeInfo::from_type_id_with_type_params(arena, r, &tp_names))
                    .unwrap_or_default();

                self.register_function_with_visibility(
                    &arena[*name].name,
                    tp_names,
                    param_types,
                    return_type,
                    vis.clone(),
                    location,
                )
                .map_err(|e| anyhow::anyhow!(e))?;
            }
            Def::TypeAlias { name, ty, .. } => {
                self.register_type(&arena[*name].name, Some(TypeInfo::from_type_id(arena, *ty)))?;
            }
            Def::ExternFunction {
                name,
                args,
                returns,
                ..
            } => {
                let extern_name = arena[*name].name.clone();
                let param_types: Vec<TypeInfo> = args
                    .iter()
                    .filter_map(|a| match &a.kind {
                        ArgKind::SelfRef { .. } => None,
                        ArgKind::Named { ty, .. }
                        | ArgKind::Ignored { ty }
                        | ArgKind::TypeOnly(ty) => Some(TypeInfo::from_type_id(arena, *ty)),
                    })
                    .collect();
                let return_type = returns
                    .map(|r| TypeInfo::from_type_id(arena, r))
                    .unwrap_or_default();
                let origin = ExternOrigin {
                    logical_module: module_name.to_string(),
                    export_field: extern_name.clone(),
                    decl: def_id,
                    resolved_path: None,
                };
                self.register_extern_function(
                    &extern_name,
                    param_types,
                    return_type,
                    Some(origin),
                )
                .map_err(|e| anyhow::anyhow!(e))?;
            }
            Def::Constant { .. } => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::type_info::{NumberType, TypeInfoKind};
    use rustc_hash::FxHashSet;

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
            let mut table = SymbolTable::default();
            let type_info = TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            };
            let result = table.register_type("MyInt", Some(type_info));
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
            let child_parent = child_scope
                .borrow()
                .parent
                .as_ref()
                .and_then(|p| p.upgrade());
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
                    definition_location: Location::default(),
                    kind: FuncKind::Local,
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
                    definition_location: Location::default(),
                    kind: FuncKind::Local,
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
                definition_location: Location::default(),
                kind: FuncKind::Local,
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
                definition_location: Location::default(),
                kind: FuncKind::Local,
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
                    definition_location: Location::default(),
                    kind: FuncKind::Local,
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
                    definition_location: Location::default(),
                    kind: FuncKind::Local,
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

    mod enum_info_tests {
        use super::*;

        #[test]
        fn variant_index_returns_correct_position() {
            let info = EnumInfo {
                name: "Color".into(),
                variants: vec!["Red".into(), "Green".into(), "Blue".into()],
                visibility: Visibility::Public,
                definition_scope_id: 0,
                definition_location: Location::default(),
            };
            assert_eq!(info.variant_index("Red"), Some(0));
            assert_eq!(info.variant_index("Green"), Some(1));
            assert_eq!(info.variant_index("Blue"), Some(2));
        }

        #[test]
        fn variant_index_returns_none_for_unknown() {
            let info = EnumInfo {
                name: "Color".into(),
                variants: vec!["Red".into(), "Green".into(), "Blue".into()],
                visibility: Visibility::Public,
                definition_scope_id: 0,
                definition_location: Location::default(),
            };
            assert_eq!(info.variant_index("Yellow"), None);
        }
    }

    mod extern_registration {
        use super::*;

        fn i32_type() -> TypeInfo {
            TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            }
        }

        fn origin(module: &str, field: &str) -> ExternOrigin {
            ExternOrigin {
                logical_module: module.to_string(),
                export_field: field.to_string(),
                decl: inference_ast::ids::idx_from_u32(0),
                resolved_path: None,
            }
        }

        #[test]
        fn bound_extern_carries_origin_and_is_discriminated() {
            let mut table = SymbolTable::default();
            table
                .register_extern_function(
                    "sort",
                    vec![i32_type()],
                    i32_type(),
                    Some(origin("collections", "sort")),
                )
                .expect("registering a bound extern should succeed");

            let info = table
                .lookup_function_anywhere("sort")
                .expect("sort should be registered");
            assert!(info.is_extern(), "a registered extern must be discriminated");
            let found = info.extern_origin().expect("bound extern carries origin");
            assert_eq!(found.logical_module, "collections");
            assert_eq!(found.export_field, "sort");
        }

        #[test]
        fn unbound_extern_is_extern_without_origin() {
            let mut table = SymbolTable::default();
            table
                .register_extern_function("add", vec![i32_type()], i32_type(), None)
                .expect("registering an unbound extern should succeed");

            let info = table
                .lookup_function_anywhere("add")
                .expect("add should be registered");
            assert!(
                info.is_extern(),
                "an unbound extern stays distinguishable from a local function"
            );
            assert!(
                info.extern_origin().is_none(),
                "an unbound extern has no provenance"
            );
        }

        #[test]
        fn local_function_is_not_extern() {
            let mut table = SymbolTable::default();
            table
                .register_function("helper", vec![], vec![], i32_type())
                .expect("registering a local function should succeed");

            let info = table
                .lookup_function_anywhere("helper")
                .expect("helper should be registered");
            assert!(!info.is_extern());
            assert!(info.extern_origin().is_none());
        }

        #[test]
        fn extern_origins_collects_only_bound_externs() {
            let mut table = SymbolTable::default();
            table
                .register_extern_function(
                    "sort",
                    vec![i32_type()],
                    i32_type(),
                    Some(origin("collections", "sort")),
                )
                .unwrap();
            table
                .register_extern_function("unbound", vec![], i32_type(), None)
                .unwrap();
            table
                .register_function("helper", vec![], vec![], i32_type())
                .unwrap();

            let origins = table.extern_origins();
            assert_eq!(
                origins.len(),
                1,
                "only the bound extern contributes an origin, got {origins:?}"
            );
            assert_eq!(origins[0].logical_module, "collections");
            assert_eq!(origins[0].export_field, "sort");
        }

        #[test]
        fn extern_origins_dedups_repeated_module_field_pairs() {
            // Two distinct externs that name the same module+field collapse to a
            // single resolution unit; the driver should resolve that `.wasm` once.
            let mut table = SymbolTable::default();
            table
                .register_extern_function(
                    "sort",
                    vec![i32_type()],
                    i32_type(),
                    Some(origin("collections", "sort")),
                )
                .unwrap();
            let _ = table.push_scope_with_name("nested", Visibility::Public);
            table
                .register_extern_function(
                    "sort",
                    vec![i32_type()],
                    i32_type(),
                    Some(origin("collections", "sort")),
                )
                .unwrap();

            let origins = table.extern_origins();
            assert_eq!(
                origins.len(),
                1,
                "identical (module, field) pairs dedup to one origin, got {origins:?}"
            );
        }
    }
}
