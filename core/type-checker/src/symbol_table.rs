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
//! Scopes form a tree where each scope can have multiple child scopes. Scopes
//! live in an index arena owned by the [`SymbolTable`]; each scope refers to its
//! parent and children by [`ScopeId`] rather than by pointer, so the tree carries
//! no interior mutability.
//!
//! ## Scope Tree Traversal
//!
//! `lookup_variable`, `lookup_variable_is_mut`, and `lookup_method` first check
//! the current scope locally; on a miss they follow the parent [`ScopeId`] and
//! recurse, terminating when either a match is found or the root scope (which has
//! no parent) is reached. Symbol resolution adds a file boundary on top of this
//! (see [`SymbolTable::lookup_symbol_file_scoped`]): a non-entry file does not
//! reach the entry file's user items by bare name. Cross-scope
//! variable lookup is what lets an inner block read an outer-scope variable;
//! it does *not* enable shadowing — re-declaring an existing name in an
//! inner scope is rejected by the type checker as
//! [`TypeCheckError::VariableShadowed`].
//!
//! ## Default Return Types
//!
//! Functions without an explicit return type default to the unit type,
//! represented as `TypeInfo { kind: TypeInfoKind::Unit, type_params: vec![] }`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use anyhow::bail;

use crate::type_info::{TypeInfo, TypeInfoKind};
use inference_ast::arena::AstArena;
use inference_ast::ids::DefId;
use inference_ast::nodes::{ArgKind, Def, Location, Visibility};
use rustc_hash::FxHashMap;

/// Handle to a [`Scope`] stored in [`SymbolTable::scopes`].
///
/// Scopes are held in an index arena: ids are dense and allocation-ordered, so a
/// `ScopeId` is exactly the storage index. The scope-tree links (`parent`,
/// `children`) and the table's scope maps are plain ids rather than
/// reference-counted, interior-mutable pointers — which is what keeps the whole
/// symbol table (and thus [`crate::typed_context::TypedContext`]) `Send + Sync`
/// (#157).
///
/// The identifier is a bare `u32` at every public boundary — method parameters,
/// and the `definition_scope_id` on [`StructInfo`]/[`EnumInfo`]/[`FuncInfo`] that
/// downstream phases read — so this newtype stays internal to the scope tree and
/// converts at the edge through [`ScopeId::as_u32`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ScopeId(u32);

impl ScopeId {
    /// The storage index of this scope in [`SymbolTable::scopes`].
    #[inline]
    #[must_use = "the storage index is the return value"]
    fn index(self) -> usize {
        self.0 as usize
    }

    /// The bare `u32` identifier, for the public boundary that speaks `u32`.
    #[inline]
    #[must_use = "the scope id is the return value"]
    fn as_u32(self) -> u32 {
        self.0
    }
}

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
    /// The name each parameter binds, one entry per entry in `param_types` and
    /// built by the same filter, so the two stay index-aligned: a receiver is
    /// absent from both, and `param_names[i]` names the parameter whose type is
    /// `param_types[i]`. `None` where a parameter binds no name at all — `_: T`
    /// and a bare positional type in an `external fn` declaration.
    ///
    /// Read when checking the labels a call writes against the parameters they
    /// claim to name; a label can never match a `None`, so one aimed at an
    /// anonymous parameter names no parameter anywhere.
    pub(crate) param_names: Vec<Option<String>>,
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

/// A nominal type (struct or enum) resolved from a reference, paired with its
/// canonical key. Returned by [`SymbolTable::resolve_qualified_type_path`] so the
/// caller can both rewrite the type to its canonical identity and report the
/// resolved declaration's visibility against the access site.
#[derive(Debug, Clone)]
pub(crate) enum ResolvedNominalType {
    Struct(StructInfo, String),
    Enum(EnumInfo, String),
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
/// A receiver declared in any later position is reported as
/// [`TypeCheckError::SelfReferenceNotFirstParameter`], which is fatal at the
/// `build_typed_context` boundary, so `has_self = true` means the receiver is
/// the first declared parameter in every program that passes type checking.
/// Registration stores the flag verbatim, so a context recovered through
/// `check_with_diagnostics` can still carry a receiver in a later position;
/// code generation asserts the position itself rather than trusting this.
///
/// # Fields
///
/// - `signature`: Function information including name, parameters, and return type
/// - `visibility`: Access control for the method
/// - `scope_id`: The scope where this method is defined (for visibility checking)
/// - `has_self`: Whether this method takes `self` as first argument
///
/// [`TypeCheckError::SelfReferenceNotFirstParameter`]: crate::errors::TypeCheckError::SelfReferenceNotFirstParameter
#[derive(Debug, Clone)]
pub(crate) struct MethodInfo {
    pub(crate) signature: FuncInfo,
    pub(crate) visibility: Visibility,
    pub(crate) scope_id: u32,
    pub(crate) has_self: bool,
}

impl MethodInfo {
    /// Returns true if this method takes `self`, which is its first declared
    /// parameter in every program that passes type checking.
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

/// How an intermediate namespace hop reached the next scope, which selects the
/// licensing discipline that hop is subject to. A hop into a raw child scope is
/// *file-nesting structural descent* (`a` → `a::b`, the scopes
/// [`SymbolTable::enter_file_scope`] nests) and must be gated by the accessing
/// file's own `use` manifest; a hop through a re-exported (`pub use`) namespace
/// import is the orthogonal re-export mechanism, already disciplined by the
/// `reexported` flag, and must not be re-gated by the manifest (#63).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReachedVia {
    /// A direct child scope — file-nesting descent, subject to the manifest gate.
    Child,
    /// A `pub use` re-export import — disciplined by the re-export flag, not the
    /// manifest.
    Reexport,
}

/// The diagnosis of a `::`-qualified path whose namespace portion the accessing
/// file did not import. Carried out of [`SymbolTable::unimported_namespace_prefix`]
/// so the caller emits the precise diagnostic for each case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UnimportedNamespace {
    /// The namespace portion is a registered project namespace (it is in the
    /// compilation closure) the accessing file simply did not `use`. The fix is
    /// exact: `use {namespace};`. `item` is the type-access tail the path reads
    /// inside that namespace (`Point::make` for `lib::b::Point::make`, or just the
    /// leaf `area` for `lib::geom::area`), so the diagnostic names the full path the
    /// import unlocks rather than dropping the type segment. Maps to
    /// [`crate::errors::TypeCheckError::UnimportedAbsoluteNamespacePath`].
    Confident { namespace: String, item: String },
    /// No registered namespace covers the path's namespace portion — the target
    /// file is most likely uncompiled, so its existence cannot be proven here.
    /// The namespace portion is offered as a hedged best-guess `use`. Maps to
    /// [`crate::errors::TypeCheckError::UnresolvedNamespacePath`].
    Hedged { namespace: String },
}

/// Information about a type alias (`type X = Y;`) or a builtin type binding.
///
/// Aliases carry real visibility (#63): a `pub type` is item-importable and
/// reachable across files, while a private one is rejected at the file boundary
/// exactly like a private fn/struct/enum. Builtin bindings (i32, bool, …) are
/// always public.
#[derive(Debug, Clone)]
pub(crate) struct TypeAliasInfo {
    pub(crate) type_info: TypeInfo,
    pub(crate) visibility: Visibility,
    /// Source location of the declaration, for the definition note on a
    /// cross-file private-access / private-import diagnostic. `Default` for
    /// builtin bindings, which have no source.
    pub(crate) definition_location: Location,
}

/// Information about a top-level `const` definition.
///
/// A top-level const registers both as a scope variable (so an intra-file use
/// site resolves it by value) and as this symbol (so it is item-importable and
/// reachable by a qualified `a::b::C` path, with visibility enforced at the file
/// boundary, #63).
#[derive(Debug, Clone)]
pub(crate) struct ConstInfo {
    pub(crate) type_info: TypeInfo,
    pub(crate) visibility: Visibility,
    /// Source location of the declaration, for the definition note on a
    /// cross-file private-access / private-import diagnostic.
    pub(crate) definition_location: Location,
}

#[derive(Debug, Clone)]
pub(crate) enum Symbol {
    /// A type alias mapping a name to another type (`type X = Y;`).
    /// Also used for builtin type bindings (i32, bool, etc.).
    TypeAlias(TypeAliasInfo),
    Struct(StructInfo),
    Enum(EnumInfo),
    Spec(String),
    Function(FuncInfo),
    /// A top-level `const` definition, carrying its value type and visibility so
    /// it can be item-imported and reached by a qualified path across files.
    Constant(ConstInfo),
}

impl Symbol {
    /// A display string for this symbol — **not** a reliable declared identifier.
    ///
    /// `Struct`, `Enum`, `Spec`, and `Function` carry their declared name and
    /// return it verbatim. `TypeAlias` and `Constant` do not store a name — it
    /// lives only as the scope-map key under which they are registered (#63) — so
    /// for those arms this falls back to the wrapped type's string form (e.g.
    /// `"i32"`), which is a type, not the declared identifier. A caller that needs
    /// the user-facing identifier already holds it as that lookup key and must use
    /// it directly rather than surfacing this value in a diagnostic.
    #[allow(dead_code)]
    #[must_use = "discarding the name has no effect"]
    pub(crate) fn name(&self) -> String {
        match self {
            Symbol::TypeAlias(info) => info.type_info.to_string(),
            Symbol::Struct(info) => info.name.clone(),
            Symbol::Enum(info) => info.name.clone(),
            Symbol::Spec(name) => name.clone(),
            Symbol::Function(sig) => sig.name.clone(),
            Symbol::Constant(info) => info.type_info.to_string(),
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

    /// Projects this symbol to the [`TypeInfo`] of the type it names, or `None`
    /// for a function or constant. `table` supplies the canonical key of a struct
    /// or enum from its *defining* scope, so a resolved struct/enum type carries
    /// its file identity rather than just its bare name (two same-named types from
    /// different files project to distinct, non-assignable [`TypeInfo`]s).
    #[must_use = "this is a pure conversion with no side effects"]
    pub(crate) fn as_type_info(&self, table: &SymbolTable) -> Option<TypeInfo> {
        match self {
            Symbol::TypeAlias(info) => Some(info.type_info.clone()),
            Symbol::Struct(info) => {
                let key = table.canonical_key_for_scope(info.definition_scope_id, &info.name);
                Some(TypeInfo {
                    kind: crate::type_info::TypeInfoKind::Struct(info.name.clone(), key),
                    type_params: info.type_params.clone(),
                })
            }
            Symbol::Enum(info) => {
                let key = table.canonical_key_for_scope(info.definition_scope_id, &info.name);
                Some(TypeInfo {
                    kind: crate::type_info::TypeInfoKind::Enum(info.name.clone(), key),
                    type_params: vec![],
                })
            }
            Symbol::Spec(name) => Some(TypeInfo {
                kind: crate::type_info::TypeInfoKind::Spec(name.clone()),
                type_params: vec![],
            }),
            Symbol::Function(_) | Symbol::Constant(_) => None,
        }
    }

    /// The value type of a top-level `const`, or `None` for any other symbol.
    /// Lets a bare or qualified reference to an imported const resolve its type.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn as_constant_type(&self) -> Option<TypeInfo> {
        if let Symbol::Constant(info) = self {
            Some(info.type_info.clone())
        } else {
            None
        }
    }

    /// Source location of this symbol's declaration, used to point a cross-file
    /// private-access diagnostic at the definition. Specs carry no location and
    /// report a zero span.
    #[must_use = "the location is the return value"]
    pub(crate) fn definition_location(&self) -> Location {
        match self {
            Symbol::Struct(info) => info.definition_location,
            Symbol::Enum(info) => info.definition_location,
            Symbol::Function(sig) => sig.definition_location,
            Symbol::TypeAlias(info) => info.definition_location,
            Symbol::Constant(info) => info.definition_location,
            Symbol::Spec(_) => Location::default(),
        }
    }

    /// Check if this symbol has public visibility.
    ///
    /// Structs, enums, functions, type aliases, and consts respect their
    /// visibility field. Specs take no visibility modifier and are treated as
    /// public (cross-file spec access is governed by import + `pub` elsewhere).
    #[must_use = "this is a pure check with no side effects"]
    pub(crate) fn is_public(&self) -> bool {
        match self {
            Symbol::TypeAlias(info) => matches!(info.visibility, Visibility::Public),
            Symbol::Struct(info) => matches!(info.visibility, Visibility::Public),
            Symbol::Enum(info) => matches!(info.visibility, Visibility::Public),
            Symbol::Spec(_) => true,
            Symbol::Function(sig) => matches!(sig.visibility, Visibility::Public),
            Symbol::Constant(info) => matches!(info.visibility, Visibility::Public),
        }
    }

    /// Whether this is a compiler-provided builtin type binding (`i32`, `bool`,
    /// …) rather than a user definition.
    ///
    /// Builtins are the only symbols a non-entry file may reach by bare name when
    /// its lookup walks into the root scope: every *user* item the entry file
    /// declares — `pub` or private — is hidden behind the file boundary (Inference
    /// has no ambient cross-file visibility; entry `pub` items are reached only
    /// through `use root;`). Builtins are registered as type aliases with a default
    /// (sourceless) location, which distinguishes them from a user `type` alias.
    #[must_use = "this is a pure check with no side effects"]
    pub(crate) fn is_builtin_binding(&self) -> bool {
        matches!(
            self,
            Symbol::TypeAlias(info) if info.definition_location == Location::default()
        )
    }
}

/// A scope in the symbol table tree.
#[derive(Debug, Clone)]
pub(crate) struct Scope {
    pub(crate) id: ScopeId,
    pub(crate) name: String,
    /// Full path from root (e.g., "mod1::mod2::mod3"), cached at creation time for O(1) lookup.
    pub(crate) full_path: String,
    #[allow(dead_code)]
    pub(crate) visibility: Visibility,
    pub(crate) parent: Option<ScopeId>,
    pub(crate) children: Vec<ScopeId>,
    pub(crate) symbols: FxHashMap<String, Symbol>,
    pub(crate) variables: FxHashMap<String, (u32, TypeInfo, bool)>,
    pub(crate) methods: FxHashMap<String, Vec<MethodInfo>>,
    /// Unresolved imports registered in this scope
    pub(crate) imports: Vec<Import>,
    /// Resolved import bindings (populated after resolution phase)
    pub(crate) resolved_imports: FxHashMap<String, ResolvedImport>,
}

impl Scope {
    #[must_use = "scope constructor returns a new scope that should be used"]
    pub(crate) fn new(
        id: ScopeId,
        name: &str,
        full_path: String,
        visibility: Visibility,
        parent: Option<ScopeId>,
    ) -> Self {
        Self {
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
        }
    }

    pub(crate) fn add_child(&mut self, child: ScopeId) {
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

    /// The type of a variable declared *directly* in this scope, ignoring
    /// parents. Lets a boundary-aware walk decide per-scope whether to read a
    /// variable (the entry file's root-scope consts are filtered from non-entry
    /// files).
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_variable_local_type(&self, name: &str) -> Option<TypeInfo> {
        self.variables.get(name).map(|(_, ty, _)| ty.clone())
    }

    #[must_use = "this is a pure lookup with no side effects"]
    fn lookup_variable_is_mut_local(&self, name: &str) -> Option<bool> {
        self.variables.get(name).map(|(_, _, is_mut)| *is_mut)
    }

    pub(crate) fn insert_method(&mut self, type_name: &str, method_info: MethodInfo) {
        self.methods
            .entry(type_name.to_string())
            .or_default()
            .push(method_info);
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

/// Index-arena symbol table.
///
/// Scopes are owned by `scopes`, a dense `Vec` keyed by [`ScopeId`]: the id of a
/// scope equals its index, and ids are handed out in allocation order by
/// `next_scope_id`. The scope maps and the current/root cursors hold ids, so the
/// whole structure is free of `Arc`/`RefCell` and is `Send + Sync` (#157).
#[derive(Clone)]
pub(crate) struct SymbolTable {
    scopes: Vec<Scope>,
    mod_scopes: FxHashMap<String, ScopeId>,
    spec_scopes: FxHashMap<String, ScopeId>,
    root_scope: Option<ScopeId>,
    current_scope: Option<ScopeId>,
    next_scope_id: u32,
}

// Compile-time assertion: SymbolTable is Send + Sync. The scope tree is a plain
// index arena with no interior mutability, so the property holds structurally, and
// is anchored here at the type that maintains it (#157). `TypedContext`, whose only
// non-trivially-`Send` field is this table, re-asserts it in `typed_context.rs`.
const _: () = {
    const fn assert_send<T: Send>() {}
    const fn assert_sync<T: Sync>() {}
    assert_send::<SymbolTable>();
    assert_sync::<SymbolTable>();
};

impl Default for SymbolTable {
    fn default() -> Self {
        let mut table = SymbolTable {
            scopes: Vec::new(),
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
        let id = ScopeId(self.next_scope_id);
        let root = Scope::new(id, "root", String::new(), Visibility::Public, None);
        self.scopes.push(root);
        self.mod_scopes.insert(String::new(), id);
        self.next_scope_id += 1;
        self.root_scope = Some(id);
        self.current_scope = Some(id);
    }

    fn init_builtin_types(&mut self) {
        use crate::type_info::{NumberType, TypeInfoKind};

        let Some(current) = self.current_scope else {
            return;
        };
        let scope = &mut self.scopes[current.index()];

        for number_type in NumberType::ALL {
            let type_info = TypeInfo {
                kind: TypeInfoKind::Number(*number_type),
                type_params: vec![],
            };
            let _ = scope.insert_symbol(
                number_type.as_str(),
                Symbol::TypeAlias(TypeAliasInfo {
                    type_info,
                    visibility: Visibility::Public,
                    definition_location: Location::default(),
                }),
            );
        }

        for (name, kind) in TypeInfoKind::NON_NUMERIC_BUILTINS {
            let type_info = TypeInfo {
                kind: kind.clone(),
                type_params: vec![],
            };
            let _ = scope.insert_symbol(
                name,
                Symbol::TypeAlias(TypeAliasInfo {
                    type_info,
                    visibility: Visibility::Public,
                    definition_location: Location::default(),
                }),
            );
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
        let parent = self.current_scope;
        let id = ScopeId(self.next_scope_id);
        self.next_scope_id += 1;

        let full_path = match parent {
            Some(p) => {
                let parent_path = &self.scopes[p.index()].full_path;
                if parent_path.is_empty() {
                    name.to_string()
                } else {
                    format!("{parent_path}::{name}")
                }
            }
            None => name.to_string(),
        };

        let new_scope = Scope::new(id, name, full_path, visibility, parent);
        self.scopes.push(new_scope);

        if let Some(p) = parent {
            self.scopes[p.index()].add_child(id);
        }

        self.current_scope = Some(id);
        id.as_u32()
    }

    /// Reassign `current_scope` to the parent of the current scope, if any.
    ///
    /// No-op when `current_scope` is `None` or has no parent (i.e. root).
    /// Counterpart to [`Self::push_scope_with_name`].
    pub(crate) fn pop_scope(&mut self) {
        if let Some(current) = self.current_scope {
            self.current_scope = self.scopes[current.index()].parent;
        }
    }

    /// Registers a type alias with default (private) visibility and no source
    /// location. A thin wrapper over [`Self::register_type_with_visibility`] for
    /// local (statement-level) `type` defs and test setup, where cross-file
    /// visibility is irrelevant.
    pub(crate) fn register_type(&mut self, name: &str, ty: Option<TypeInfo>) -> anyhow::Result<()> {
        self.register_type_with_visibility(name, ty, Visibility::Private, Location::default())
    }

    /// Registers a type alias carrying its visibility and declaration location, so
    /// a `pub type` is item-importable and reachable across files while a private
    /// one is rejected at the file boundary (#63).
    pub(crate) fn register_type_with_visibility(
        &mut self,
        name: &str,
        ty: Option<TypeInfo>,
        visibility: Visibility,
        location: Location,
    ) -> anyhow::Result<()> {
        if let Some(current) = self.current_scope {
            let type_info = ty.unwrap_or_else(|| TypeInfo {
                kind: crate::type_info::TypeInfoKind::Custom(name.to_string()),
                type_params: vec![],
            });
            self.scopes[current.index()].insert_symbol(
                name,
                Symbol::TypeAlias(TypeAliasInfo {
                    type_info,
                    visibility,
                    definition_location: location,
                }),
            )
        } else {
            bail!("No active scope to register type")
        }
    }

    /// Registers a top-level `const` as a symbol carrying its value type and
    /// visibility, so it is item-importable and reachable by a qualified path
    /// across files (#63). The intra-file use-site resolution still goes through
    /// the scope variable registered alongside it.
    ///
    /// Registration is a no-op when the name is already taken by another symbol in
    /// the scope (e.g. a same-named function): the const stays resolvable
    /// intra-file through its scope variable, and a doubly-defined name could not
    /// be unambiguously imported anyway. This keeps the const-symbol pass from
    /// turning a pre-existing latent name clash into a hard error.
    pub(crate) fn register_constant(
        &mut self,
        name: &str,
        type_info: TypeInfo,
        visibility: Visibility,
        location: Location,
    ) -> anyhow::Result<()> {
        if let Some(current) = self.current_scope {
            if self.scopes[current.index()]
                .lookup_symbol_local(name)
                .is_some()
            {
                return Ok(());
            }
            self.scopes[current.index()].insert_symbol(
                name,
                Symbol::Constant(ConstInfo {
                    type_info,
                    visibility,
                    definition_location: location,
                }),
            )
        } else {
            bail!("No active scope to register constant")
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
        if let Some(current) = self.current_scope {
            let scope_id = current.as_u32();
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
            self.scopes[current.index()].insert_symbol(name, Symbol::Struct(struct_info))
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
        if let Some(current) = self.current_scope {
            let scope_id = current.as_u32();
            let enum_info = EnumInfo {
                name: name.to_string(),
                variants: variants.iter().map(|s| (*s).to_string()).collect(),
                visibility,
                definition_scope_id: scope_id,
                definition_location: location,
            };
            self.scopes[current.index()].insert_symbol(name, Symbol::Enum(enum_info))
        } else {
            bail!("No active scope to register enum")
        }
    }

    pub(crate) fn register_spec(&mut self, name: &str) -> anyhow::Result<()> {
        if let Some(current) = self.current_scope {
            self.scopes[current.index()].insert_symbol(name, Symbol::Spec(name.to_string()))
        } else {
            bail!("No active scope to register spec")
        }
    }

    /// Resolve `TypeInfoKind::Custom(name)` to `Struct(name, key)` or
    /// `Enum(name, key)` by looking up the name from the current scope. Falls
    /// through to `Custom` if the name is not found (e.g., forward references in
    /// nested modules). Recurses into array element types.
    ///
    /// The canonical key is taken from the type's *defining* scope (via
    /// [`Self::resolve_struct_in_scope`]), so a bare reference resolves to the
    /// same key whether it is written in the defining file or in a file that
    /// item-imports the type — and a same-named type from a different file gets a
    /// distinct key. This is the single chokepoint that gives a resolved struct or
    /// enum type its file identity; the current scope is the file scope the type
    /// checker is walking, set by `enter_file_scope` / `renormalize_signatures`.
    #[must_use = "returns the resolved type; discarding it loses the resolution"]
    pub(crate) fn resolve_custom_type(&self, ti: TypeInfo) -> TypeInfo {
        let from_scope = self.current_scope_id().unwrap_or(0);
        self.resolve_custom_type_in_scope(ti, from_scope)
    }

    /// Resolves `TypeInfoKind::Custom(name)` as [`Self::resolve_custom_type`]
    /// does, but against an explicit `from_scope` rather than the current cursor.
    ///
    /// A re-resolution pass that is not walking the scope tree — re-normalizing
    /// an item-imported function's stored signature against the function's own
    /// defining file — needs to resolve names from that defining scope, where the
    /// signature's item-imports are bound, not from wherever the cursor happens to
    /// rest. The bare-name and import lookups both honor `from_scope`'s file
    /// boundary and imports, so the resolved key is identical to the one the
    /// in-place signature receives during `renormalize_signatures`.
    #[must_use = "returns the resolved type; discarding it loses the resolution"]
    pub(crate) fn resolve_custom_type_in_scope(&self, mut ti: TypeInfo, from_scope: u32) -> TypeInfo {
        match &ti.kind {
            TypeInfoKind::Custom(name) => {
                if let Some((_, key)) = self.resolve_struct_in_scope(name, from_scope) {
                    ti.kind = TypeInfoKind::Struct(name.clone(), key);
                } else if let Some((_, key)) = self.resolve_enum_in_scope(name, from_scope) {
                    ti.kind = TypeInfoKind::Enum(name.clone(), key);
                }
                ti
            }
            // A `::`-qualified annotation (`geo::Level`, `lib::geom::Point`)
            // carries its full path as the variant string. Resolving it to the
            // same canonical `Struct`/`Enum` a constructor produces is what makes
            // the annotation's type *equal* the value's type — without this the
            // annotation stays an opaque `Qualified(..)` that never unifies. The
            // bare leaf name is preserved as the first field (codegen reads it and
            // re-qualifies); identity is the canonical key.
            TypeInfoKind::Qualified(path) | TypeInfoKind::QualifiedName(path) => {
                let segments: Vec<String> = path.split("::").map(ToString::to_string).collect();
                match self.resolve_qualified_type_path(&segments, from_scope) {
                    Some(ResolvedNominalType::Struct(info, key)) => {
                        ti.kind = TypeInfoKind::Struct(info.name, key);
                    }
                    Some(ResolvedNominalType::Enum(info, key)) => {
                        ti.kind = TypeInfoKind::Enum(info.name, key);
                    }
                    None => {}
                }
                ti
            }
            TypeInfoKind::Array(elem, size) => {
                let resolved_elem = self.resolve_custom_type_in_scope(*elem.clone(), from_scope);
                ti.kind = TypeInfoKind::Array(Box::new(resolved_elem), *size);
                ti
            }
            _ => ti,
        }
    }

    /// Registers a private local function whose parameters bind no names. A thin
    /// wrapper over [`Self::register_function_with_visibility`], kept for test
    /// setup that cares about neither visibility nor argument labels.
    #[cfg(test)]
    pub(crate) fn register_function(
        &mut self,
        name: &str,
        type_params: Vec<String>,
        param_types: Vec<TypeInfo>,
        return_type: TypeInfo,
    ) -> Result<(), String> {
        let param_names = vec![None; param_types.len()];
        self.register_function_with_visibility(
            name,
            type_params,
            param_types,
            param_names,
            return_type,
            Visibility::Private,
            Location::default(),
        )
    }

    // Same shape as `insert_func_symbol` below: each signature field is an
    // independent input, and `param_types`/`param_names` are two halves of one
    // index-aligned pair that a parameter struct would only obscure.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn register_function_with_visibility(
        &mut self,
        name: &str,
        type_params: Vec<String>,
        param_types: Vec<TypeInfo>,
        param_names: Vec<Option<String>>,
        return_type: TypeInfo,
        visibility: Visibility,
        location: Location,
    ) -> Result<(), String> {
        self.insert_func_symbol(
            name,
            type_params,
            param_types,
            param_names,
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
        param_names: Vec<Option<String>>,
        return_type: TypeInfo,
        origin: Option<ExternOrigin>,
    ) -> Result<(), String> {
        self.insert_func_symbol(
            name,
            vec![],
            param_types,
            param_names,
            return_type,
            Visibility::Private,
            Location::default(),
            FuncKind::Extern(origin),
        )
    }

    // The signature fields (name, type params, param types/names, return type,
    // visibility, source location, local-vs-extern kind) are each independent
    // inputs to a single private constructor; grouping them into a parameter
    // struct would add a one-use type without clarifying anything.
    #[allow(clippy::too_many_arguments)]
    fn insert_func_symbol(
        &mut self,
        name: &str,
        type_params: Vec<String>,
        param_types: Vec<TypeInfo>,
        param_names: Vec<Option<String>>,
        return_type: TypeInfo,
        visibility: Visibility,
        location: Location,
        kind: FuncKind,
    ) -> Result<(), String> {
        if let Some(current) = self.current_scope {
            let scope_id = current.as_u32();
            let sig = FuncInfo {
                name: name.to_string(),
                type_params,
                param_types: param_types
                    .into_iter()
                    .map(|ti| self.resolve_custom_type(ti))
                    .collect(),
                param_names,
                return_type: self.resolve_custom_type(return_type),
                visibility,
                definition_scope_id: scope_id,
                definition_location: location,
                kind,
            };
            self.scopes[current.index()]
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
        if let Some(current) = self.current_scope {
            self.scopes[current.index()].insert_variable(name, 0, var_type, is_mut)
        } else {
            bail!("No active scope to push variable")
        }
    }

    /// Resolves a bare symbol name from the current scope, walking the parent
    /// chain but honoring the file boundary at root: a non-entry file does **not**
    /// see the entry file's user symbols by bare name.
    ///
    /// The entry file maps to the root scope, so a non-entry file's chain crosses
    /// into root when it walks past its own file namespace. Root holds the entry
    /// file's definitions *and* the program's builtins. Inference has no ambient
    /// cross-file visibility (Zig-aligned): every entry *user* item — `pub` or
    /// private — is hidden at the boundary, reachable from a non-entry file only
    /// through the explicit `use root;` handle. Only builtins (`i32`, `bool`, …)
    /// stay bare-reachable. This stops a private `struct Secret` *and* a `pub`
    /// `fn helper` in `main.inf` from being reached by bare name in an imported
    /// file. The entry file's own lookups start at root and never cross the
    /// boundary, so they see their own items unchanged.
    #[must_use = "this is a pure lookup with no side effects"]
    fn lookup_symbol_file_scoped(&self, name: &str) -> Option<Symbol> {
        let root_id = self.root_scope;
        let mut cursor = self.current_scope;
        let mut crossed_file_boundary = false;
        while let Some(id) = cursor {
            let s = self.scope(id)?;
            let is_root = Some(id) == root_id;
            if let Some(symbol) = s.lookup_symbol_local(name) {
                // A user symbol in the entry file (root) is invisible to a lookup
                // that originated inside a non-entry file; only builtins pass.
                if is_root && crossed_file_boundary && !symbol.is_builtin_binding() {
                    return None;
                }
                return Some(symbol.clone());
            }
            // Leaving a non-entry file namespace means the next hop into root is a
            // cross-file access; record it before advancing.
            if self.is_non_entry_file_scope(id.as_u32()) {
                crossed_file_boundary = true;
            }
            cursor = s.parent;
        }
        None
    }

    /// Resolves a bare symbol name starting from an explicit `from_scope_id`,
    /// walking its parent chain but honoring the file boundary at root exactly as
    /// [`Self::lookup_symbol_file_scoped`] does for the current cursor.
    ///
    /// A type/method resolver that is handed a `from_scope` (rather than walking
    /// the cursor) must apply the same boundary: an entry-file user item — `pub`
    /// or private — is not ambiently visible by bare name to a non-entry file, so
    /// a bare `Color`/`Gizmo` written in an imported file must not silently
    /// resolve to the entry's same-named type. Only builtins (`i32`, …) pass the
    /// boundary; the file's own definitions and an own-file import are reached
    /// before the boundary is crossed (#63).
    #[must_use = "this is a pure lookup with no side effects"]
    fn lookup_symbol_file_scoped_from(&self, name: &str, from_scope_id: u32) -> Option<Symbol> {
        let root_id = self.root_scope;
        let mut cursor = Some(ScopeId(from_scope_id));
        let mut crossed_file_boundary = false;
        while let Some(id) = cursor {
            let s = self.scope(id)?;
            let is_root = Some(id) == root_id;
            if let Some(symbol) = s.lookup_symbol_local(name) {
                if is_root && crossed_file_boundary && !symbol.is_builtin_binding() {
                    return None;
                }
                return Some(symbol.clone());
            }
            if self.is_non_entry_file_scope(id.as_u32()) {
                crossed_file_boundary = true;
            }
            cursor = s.parent;
        }
        None
    }

    /// Whether `scope_id` is a non-entry file namespace — a scope registered in
    /// `mod_scopes` under a non-empty `::`-joined path. The entry file is the root
    /// (empty key) and is excluded.
    #[must_use = "this is a pure check with no side effects"]
    fn is_non_entry_file_scope(&self, scope_id: u32) -> bool {
        self.mod_scopes
            .iter()
            .any(|(key, id)| !key.is_empty() && id.as_u32() == scope_id)
    }

    /// The id of the file scope enclosing `scope_id`: the nearest ancestor (or
    /// `scope_id` itself) that is a file boundary — a non-entry file namespace, or
    /// the root scope (the entry file). Spec, block, and function scopes resolve to
    /// the file that contains them.
    ///
    /// Two scopes are "in the same file" iff this returns the same id for both,
    /// which is the correct same-file privacy test: the scope-descendant test is
    /// too permissive for entry-file (root) items, since *every* scope descends
    /// from root, so a non-entry file would falsely count as same-file with the
    /// entry.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn enclosing_file_scope(&self, scope_id: u32) -> u32 {
        let root_id = self.root_scope;
        let mut cursor = Some(ScopeId(scope_id));
        while let Some(id) = cursor {
            let Some(s) = self.scope(id) else { break };
            if Some(id) == root_id || self.is_non_entry_file_scope(id.as_u32()) {
                return id.as_u32();
            }
            cursor = s.parent;
        }
        scope_id
    }

    /// Whether `access_scope` and `definition_scope` belong to the same source
    /// file — the same-file privacy test (see [`Self::enclosing_file_scope`]).
    #[must_use = "this is a pure check with no side effects"]
    pub(crate) fn same_file(&self, access_scope: u32, definition_scope: u32) -> bool {
        self.enclosing_file_scope(access_scope) == self.enclosing_file_scope(definition_scope)
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_type(&self, name: &str) -> Option<TypeInfo> {
        if let Some(symbol) = self.lookup_symbol_file_scoped(name) {
            return symbol.as_type_info(self);
        }
        // A bare type reference may name a struct/enum brought in by
        // `use a::b::{T};`; the import binds `T` as a resolved item in the file
        // scope, so consult those when no like-named symbol is in scope.
        self.lookup_imported_item_symbol(name)
            .and_then(|symbol| symbol.as_type_info(self))
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_variable(&self, name: &str) -> Option<TypeInfo> {
        // The only variables ever held in the root scope are the entry file's
        // top-level `const`s (registered as scope variables for intra-file use).
        // A non-entry file must not reach them by bare name — cross-file const
        // access goes through `lookup_constant`, which gates on `pub`. So a
        // lookup that originated inside a non-entry file stops before reading a
        // root-scope variable.
        let root_id = self.root_scope;
        let mut cursor = self.current_scope;
        let mut crossed_file_boundary = false;
        while let Some(id) = cursor {
            let s = self.scope(id)?;
            let is_root = Some(id) == root_id;
            if !(is_root && crossed_file_boundary)
                && let Some(ty) = s.lookup_variable_local_type(name)
            {
                return Some(ty);
            }
            if self.is_non_entry_file_scope(id.as_u32()) {
                crossed_file_boundary = true;
            }
            cursor = s.parent;
        }
        None
    }

    /// Resolves the value type of a top-level `const` named `name`, including one
    /// brought in bare by `use a::b::{C};`.
    ///
    /// An intra-file const resolves through the scope variable registered
    /// alongside its symbol; this covers the imported case, where only the symbol
    /// (not a variable) crosses the file boundary.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_constant(&self, name: &str) -> Option<TypeInfo> {
        let from_symbol = self
            .lookup_symbol_file_scoped(name)
            .and_then(|symbol| symbol.as_constant_type());
        if from_symbol.is_some() {
            return from_symbol;
        }
        self.lookup_imported_item_symbol(name)
            .and_then(|symbol| symbol.as_constant_type())
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_variable_is_mut(&self, name: &str) -> Option<bool> {
        let mut cursor = self.current_scope;
        while let Some(id) = cursor {
            let s = self.scope(id)?;
            if let Some(is_mut) = s.lookup_variable_is_mut_local(name) {
                return Some(is_mut);
            }
            cursor = s.parent;
        }
        None
    }

    /// Checks whether a variable exists in any parent scope (skipping the current scope).
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_variable_in_parent_scopes(&self, name: &str) -> Option<TypeInfo> {
        let mut cursor = self.scope(self.current_scope?)?.parent;
        while let Some(id) = cursor {
            let s = self.scope(id)?;
            if let Some((_, ty, _)) = s.lookup_variable_local(name) {
                return Some(ty);
            }
            cursor = s.parent;
        }
        None
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_function(&self, name: &str) -> Option<FuncInfo> {
        let from_symbol = self
            .lookup_symbol_file_scoped(name)
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
        let mut cursor = self.current_scope;
        while let Some(id) = cursor {
            let s = self.scope(id)?;
            if let Some(resolved) = s.resolved_imports.get(name)
                && let ResolvedImportTarget::Item { symbol, .. } = &resolved.target
            {
                return Some((**symbol).clone());
            }
            cursor = s.parent;
        }
        None
    }

    /// Looks up a function by name in `scope_id`'s local symbols only, without
    /// walking the parent chain. The spec-shadow check resolves the colliding
    /// top-level name in the spec's *own* file scope rather than the entry file's
    /// root scope, since a spec-inner function shadows a top-level function only
    /// when both live in the same file. The root-only variant above would miss a
    /// same-file collision in a non-entry file and falsely flag an entry-file
    /// top-level name that a spec in a different file happens to repeat.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_function_in_scope(&self, scope_id: u32, name: &str) -> Option<FuncInfo> {
        self.scope(ScopeId(scope_id))?
            .lookup_symbol_local(name)?
            .as_function()
            .cloned()
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_struct(&self, name: &str) -> Option<StructInfo> {
        let from_symbol = self
            .lookup_symbol_file_scoped(name)
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
            .lookup_symbol_file_scoped(name)
            .and_then(|symbol| symbol.as_enum().cloned());
        if from_symbol.is_some() {
            return from_symbol;
        }
        // Resolve an enum brought in by `use a::b::{E};` for bare use.
        self.lookup_imported_item_symbol(name)
            .and_then(|symbol| symbol.as_enum().cloned())
    }

    /// Looks up a struct by its file-qualified canonical key (`lib::geo::Point`,
    /// or the bare name for an entry-file struct), searching all scopes.
    ///
    /// The key uniquely identifies a struct's defining file, so this never
    /// collapses same-named structs from different files. Unlike a bare-name
    /// lookup it ignores the file-visibility boundary: the caller already holds a
    /// value of this struct's type (the key came from its `TypeInfo`), so its
    /// layout is needed regardless of whether the bare name is in scope at the use
    /// site — e.g. a field read on a value built via `root::` or a namespace path.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_struct_by_key(&self, key: &str) -> Option<StructInfo> {
        let leaf = key.rsplit("::").next()?;
        for scope in &self.scopes {
            if let Some(symbol) = scope.lookup_symbol_local(leaf)
                && let Some(info) = symbol.as_struct()
                && self.canonical_key_for_scope(info.definition_scope_id, &info.name) == key
            {
                return Some(info.clone());
            }
        }
        None
    }

    /// Looks up an enum by its file-qualified canonical key. Mirrors
    /// [`Self::lookup_struct_by_key`]: the key uniquely identifies the enum's
    /// defining file, so same-named enums from different files never collapse.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_enum_by_key(&self, key: &str) -> Option<EnumInfo> {
        let leaf = key.rsplit("::").next()?;
        for scope in &self.scopes {
            if let Some(symbol) = scope.lookup_symbol_local(leaf)
                && let Some(info) = symbol.as_enum()
                && self.canonical_key_for_scope(info.definition_scope_id, &info.name) == key
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
        let file_ids: BTreeSet<u32> = self.mod_scopes.values().map(|id| id.as_u32()).collect();
        let mut cursor = Some(ScopeId(scope_id));
        while let Some(id) = cursor {
            let Some(scope) = self.scope(id) else { break };
            if file_ids.contains(&id.as_u32()) {
                return scope.full_path.clone();
            }
            cursor = scope.parent;
        }
        String::new()
    }

    /// Returns the source-root-relative module path segments of the nearest
    /// enclosing **file** scope of `scope_id`. The entry file yields an empty
    /// vector; an imported file `lib/arith.inf` yields `["lib", "arith"]`.
    ///
    /// Code generation calls this to file-qualify a resolved symbol: given the
    /// `definition_scope_id` of a function or struct, it recovers the defining
    /// file's module path so the function's flat WASM name and the struct's
    /// layout key can be qualified by their defining file rather than the
    /// referencing one.
    #[must_use = "the module path is the return value"]
    pub(crate) fn file_module_path_of_scope(&self, scope_id: u32) -> Vec<String> {
        let path = self.enclosing_file_path(scope_id);
        if path.is_empty() {
            Vec::new()
        } else {
            path.split("::").map(ToString::to_string).collect()
        }
    }

    /// Enumerates every registered struct paired with its canonical key, in
    /// ascending scope-id order. The post-type-check store ([`TypedContext`])
    /// folds this into a key-indexed map so codegen and analysis resolve a type
    /// reference to exactly the layout of its defining file — never a same-named
    /// struct from another file.
    #[must_use = "the enumeration is the return value"]
    pub(crate) fn structs_with_canonical_keys(&self) -> Vec<(String, StructInfo)> {
        let mut out = Vec::new();
        for scope in &self.scopes {
            let id = scope.id.as_u32();
            for symbol in scope.symbols.values() {
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
        let mut out = Vec::new();
        for scope in &self.scopes {
            let id = scope.id.as_u32();
            for symbol in scope.symbols.values() {
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
        if let Some(symbol) = self.lookup_symbol_file_scoped_from(name, from_scope_id)
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
        if let Some(symbol) = self.lookup_symbol_file_scoped_from(name, from_scope_id)
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

    /// Resolves a method `method_name` on the type `type_name` as referenced from
    /// `from_scope_id`, honoring file boundaries.
    ///
    /// Methods register in the struct's *defining* scope, not the accessing scope,
    /// so a parent-chain walk from the call site (the cursor-based
    /// [`Self::lookup_method`]) never reaches a cross-file struct's methods. This
    /// instead resolves the struct's defining scope first — via the same
    /// file-scoped resolution a type reference uses — then looks the method up in
    /// that scope's method table. Visibility of the method is enforced by the
    /// caller against the returned [`MethodInfo::scope_id`].
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn resolve_method_in_scope(
        &self,
        type_name: &str,
        method_name: &str,
        from_scope_id: u32,
    ) -> Option<MethodInfo> {
        // A bare or imported type name resolves to the struct's defining scope;
        // the method lives in that scope's method table. Falling back to the
        // cursor-based lookup keeps single-file resolution working when no file
        // scope is found (e.g. spec-inner method calls).
        if let Some((info, _)) = self.resolve_struct_in_scope(type_name, from_scope_id)
            && let Some(method) =
                self.method_in_defining_scope(info.definition_scope_id, type_name, method_name)
        {
            return Some(method);
        }
        // The struct did not resolve to a defining scope (e.g. a spec-inner type
        // whose name is not bare-reachable). A same-file parent-walk still finds
        // its method, but the walk must stop at the file boundary: an entry-file
        // type's method is not ambiently reachable by bare name from a non-entry
        // file (it requires `use root;` → `root::Type::m()`), matching the bare
        // type-name boundary in [`Self::lookup_symbol_file_scoped_from`] (#63).
        self.lookup_method_file_scoped_from(type_name, method_name, from_scope_id)
    }

    /// Resolves a method on the struct identified by `canonical_key`, ignoring the
    /// call site's scope entirely.
    ///
    /// A method dispatch must follow the **receiver's** struct identity, not a
    /// bare type name re-resolved from the call site: two files may each define a
    /// same-named struct, so resolving the bare name where the call is written can
    /// pick a foreign struct that merely shares the name. The receiver value
    /// carries its canonical key (`lib::geo::Inner`, or the bare name for an
    /// entry-file struct), which uniquely identifies the defining struct; this
    /// looks the method up directly in that struct's defining scope.
    ///
    /// Returns `None` when no struct has that key, or when the keyed struct
    /// genuinely has no such method — the caller reports the latter as a clean
    /// "method not found" rather than silently dispatching to a same-named
    /// foreign method.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn resolve_method_by_canonical_key(
        &self,
        canonical_key: &str,
        method_name: &str,
    ) -> Option<MethodInfo> {
        let info = self.lookup_struct_by_key(canonical_key)?;
        self.method_in_defining_scope(info.definition_scope_id, &info.name, method_name)
    }

    /// Looks up `method_name` on the struct `struct_bare_name` in its own defining
    /// scope's method table. Shared tail of [`Self::resolve_method_in_scope`] and
    /// [`Self::resolve_method_by_canonical_key`] so the bare-name and canonical-key
    /// entry points cannot drift: methods register under the struct's bare name in
    /// the scope where the struct is defined.
    #[must_use = "this is a pure lookup with no side effects"]
    fn method_in_defining_scope(
        &self,
        definition_scope_id: u32,
        struct_bare_name: &str,
        method_name: &str,
    ) -> Option<MethodInfo> {
        self.scope(ScopeId(definition_scope_id))?
            .methods
            .get(struct_bare_name)
            .and_then(|methods| methods.iter().find(|m| m.signature.name == method_name))
            .cloned()
    }

    /// Looks up a method on `type_name` by walking `from_scope_id`'s parent chain,
    /// but honoring the file boundary at root exactly as
    /// [`Self::lookup_symbol_file_scoped_from`] does for bare symbols: a non-entry
    /// file's walk stops before reading the entry file's (root-scope) methods, so
    /// an entry type's method is not ambiently reachable by bare name (#63).
    #[must_use = "this is a pure lookup with no side effects"]
    fn lookup_method_file_scoped_from(
        &self,
        type_name: &str,
        method_name: &str,
        from_scope_id: u32,
    ) -> Option<MethodInfo> {
        let root_id = self.root_scope;
        let mut cursor = Some(ScopeId(from_scope_id));
        let mut crossed_file_boundary = false;
        while let Some(id) = cursor {
            let s = self.scope(id)?;
            let is_root = Some(id) == root_id;
            if !(is_root && crossed_file_boundary)
                && let Some(method) = s
                    .methods
                    .get(type_name)
                    .and_then(|methods| methods.iter().find(|m| m.signature.name == method_name))
            {
                return Some(method.clone());
            }
            if self.is_non_entry_file_scope(id.as_u32()) {
                crossed_file_boundary = true;
            }
            cursor = s.parent;
        }
        None
    }

    /// Resolves a struct type reference `name` *inside* the namespace scope
    /// `ns_scope_id`, reached by traversing a namespace prefix from
    /// `accessor_scope_id`. A struct defined directly in the namespace resolves
    /// unconditionally; one brought in by an item import is followed only when
    /// the re-export gate permits ([`Self::lookup_imported_item_symbol_gated`]),
    /// so a plain (non-`pub use`) intermediate import blocks traversal to its
    /// type members exactly as it blocks free functions (#63).
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn resolve_struct_in_namespace(
        &self,
        name: &str,
        ns_scope_id: u32,
        accessor_scope_id: u32,
    ) -> Option<(StructInfo, String)> {
        if let Some(symbol) = self.lookup_symbol_file_scoped_from(name, ns_scope_id)
            && let Some(info) = symbol.as_struct()
        {
            let key = self.canonical_key_for_scope(info.definition_scope_id, &info.name);
            return Some((info.clone(), key));
        }
        if let Symbol::Struct(info) =
            self.lookup_imported_item_symbol_gated(name, ns_scope_id, accessor_scope_id)?
        {
            let key = self.canonical_key_for_scope(info.definition_scope_id, &info.name);
            return Some((info, key));
        }
        None
    }

    /// Resolves an enum type reference inside a traversed namespace scope.
    /// Mirrors [`Self::resolve_struct_in_namespace`].
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn resolve_enum_in_namespace(
        &self,
        name: &str,
        ns_scope_id: u32,
        accessor_scope_id: u32,
    ) -> Option<(EnumInfo, String)> {
        if let Some(symbol) = self.lookup_symbol_file_scoped_from(name, ns_scope_id)
            && let Some(info) = symbol.as_enum()
        {
            let key = self.canonical_key_for_scope(info.definition_scope_id, &info.name);
            return Some((info.clone(), key));
        }
        if let Symbol::Enum(info) =
            self.lookup_imported_item_symbol_gated(name, ns_scope_id, accessor_scope_id)?
        {
            let key = self.canonical_key_for_scope(info.definition_scope_id, &info.name);
            return Some((info, key));
        }
        None
    }

    /// Resolves a method on `type_name` inside a traversed namespace scope,
    /// honoring the re-export gate on the struct it belongs to. Mirrors
    /// [`Self::resolve_method_in_scope`] but routes the struct resolution through
    /// [`Self::resolve_struct_in_namespace`] so a plain intermediate import blocks
    /// the method too (#63).
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn resolve_method_in_namespace(
        &self,
        type_name: &str,
        method_name: &str,
        ns_scope_id: u32,
        accessor_scope_id: u32,
    ) -> Option<MethodInfo> {
        let (info, _) =
            self.resolve_struct_in_namespace(type_name, ns_scope_id, accessor_scope_id)?;
        self.scope(ScopeId(info.definition_scope_id))?
            .methods
            .get(type_name)
            .and_then(|methods| methods.iter().find(|m| m.signature.name == method_name))
            .cloned()
    }


    /// Resolves a `::`-qualified type path (`geo::Level`, `root::Pt`,
    /// `lib::geom::Point`) to the struct or enum it names, paired with its
    /// canonical key, as referenced from `from_scope_id`.
    ///
    /// The leading segments name a chain of file namespaces and the final segment
    /// is the leaf type. [`Self::resolve_longest_namespace_prefix`] consumes the
    /// namespace run (anchoring on a `use a::b;` binding, `use root;`, or a root
    /// child) and the single remaining segment resolves as a type member *inside*
    /// that namespace's file scope — through [`Self::resolve_struct_in_namespace`]
    /// / [`Self::resolve_enum_in_namespace`], so cross-file `pub`-ness and
    /// re-export gates are honored exactly as the qualified-call path is. The
    /// returned key is the type's defining-file identity, so a qualified
    /// annotation gains the same nominal identity a bare reference or a
    /// constructor would (#63).
    ///
    /// Returns `None` when the prefix is not a namespace chain (e.g. an
    /// `Enum::Variant` mis-written in type position) or the leaf is not a struct
    /// or enum, leaving the unresolved annotation untouched so resolution
    /// fails safe rather than inventing an identity.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn resolve_qualified_type_path(
        &self,
        path: &[String],
        from_scope_id: u32,
    ) -> Option<ResolvedNominalType> {
        // The final segment is the leaf type; one trailing type-access segment.
        let (ns_scope, consumed) = self.resolve_longest_namespace_prefix(path, from_scope_id, 1)?;
        // A type path is a namespace chain plus a single leaf; a different shape
        // (a remaining `Type::assoc` pair, or no leaf at all) is not a type
        // reference we resolve here.
        if path.len() - consumed != 1 {
            return None;
        }
        let leaf = &path[consumed];
        if let Some((info, key)) = self.resolve_struct_in_namespace(leaf, ns_scope, from_scope_id) {
            return Some(ResolvedNominalType::Struct(info, key));
        }
        if let Some((info, key)) = self.resolve_enum_in_namespace(leaf, ns_scope, from_scope_id) {
            return Some(ResolvedNominalType::Enum(info, key));
        }
        None
    }

    /// Looks up a resolved item import named `name`, walking the parent chain of
    /// `from_scope_id` (rather than the symbol table's current cursor). Returns
    /// the imported symbol so a type reference can resolve an item brought in by
    /// `use a::b::{S};`.
    #[must_use = "this is a pure lookup with no side effects"]
    fn lookup_imported_item_symbol_from(&self, name: &str, from_scope_id: u32) -> Option<Symbol> {
        let mut cursor = Some(ScopeId(from_scope_id));
        while let Some(id) = cursor {
            let s = self.scope(id)?;
            if let Some(resolved) = s.resolved_imports.get(name)
                && let ResolvedImportTarget::Item { symbol, .. } = &resolved.target
            {
                return Some((**symbol).clone());
            }
            cursor = s.parent;
        }
        None
    }

    /// Looks up a resolved item import named `name` along `ns_scope_id`'s parent
    /// chain, but **honors the re-export gate** when the import is followed across
    /// a file boundary from `accessor_scope_id`.
    ///
    /// This is the type-member twin of the item-import branch in
    /// [`Self::resolve_qualified_name`]. A namespace type-member access
    /// (`mid::Point::raw()`) resolves the type *inside* the intermediate file
    /// `ns_scope_id`, which is reached by traversing a namespace prefix from the
    /// accessing file. An item brought into that intermediate file by a plain
    /// `use` (not `pub use`) is private to it, so following it from another file
    /// is a public-surface leak — exactly what the free-function path rejects.
    /// The gate applies only when the import's enclosing scope is *not* in the
    /// accessor's ancestry (a genuine boundary crossing); an own-file item import
    /// stays resolvable.
    #[must_use = "this is a pure lookup with no side effects"]
    fn lookup_imported_item_symbol_gated(
        &self,
        name: &str,
        ns_scope_id: u32,
        accessor_scope_id: u32,
    ) -> Option<Symbol> {
        let accessor_ancestry = self.scope_ancestry(accessor_scope_id);
        let mut cursor = Some(ScopeId(ns_scope_id));
        while let Some(id) = cursor {
            let s = self.scope(id)?;
            if let Some(resolved) = s.resolved_imports.get(name)
                && let ResolvedImportTarget::Item { symbol, .. } = &resolved.target
            {
                let crosses_file_boundary = !accessor_ancestry.contains(&id.as_u32());
                if crosses_file_boundary && !resolved.reexported {
                    return None;
                }
                return Some((**symbol).clone());
            }
            cursor = s.parent;
        }
        None
    }

    /// Collects the provenance of every **bound** `external fn` across all
    /// scopes, deduplicated by `(logical_module, export_field, decl)`.
    ///
    /// The driver consumes this to resolve and validate each external `.wasm`
    /// before linking. Unbound bare externs (declared without a binding `use`)
    /// carry no origin and are skipped — they never reach the linker.
    ///
    /// The declaration is part of the key because the driver validates the
    /// external library against the *declared* signature it recovers from
    /// `decl`. Two files may each declare and bind the same `(module, field)`,
    /// and the linker satisfies an import on `(module, field)` alone with no
    /// signature comparison of its own — so dropping either declaration here
    /// would leave its signature unchecked and let a disagreement ship as a
    /// silently mis-linked artifact instead of a rejection. Only a genuinely
    /// repeated registration of one declaration (the same extern reachable from
    /// more than one scope) collapses.
    ///
    /// The order is deterministic: a scope's symbols are stored by name in a
    /// hash map, so the traversal alone would leave the order — and hence which
    /// of several failing externs is reported first — unstable across runs.
    #[must_use = "this enumeration has no side effects"]
    pub(crate) fn extern_origins(&self) -> Vec<ExternOrigin> {
        let mut origins: BTreeMap<(String, String, DefId), ExternOrigin> = BTreeMap::new();
        for scope in &self.scopes {
            for symbol in scope.symbols.values() {
                let Some(info) = symbol.as_function() else {
                    continue;
                };
                let Some(origin) = info.extern_origin() else {
                    continue;
                };
                let key = (
                    origin.logical_module.clone(),
                    origin.export_field.clone(),
                    origin.decl,
                );
                origins.entry(key).or_insert_with(|| origin.clone());
            }
        }
        origins.into_values().collect()
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
        for scope in &self.scopes {
            for symbol in scope.symbols.values() {
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

    pub(crate) fn register_method(
        &mut self,
        type_name: &str,
        signature: FuncInfo,
        visibility: Visibility,
        has_self: bool,
    ) -> anyhow::Result<()> {
        if let Some(current) = self.current_scope {
            let method_info = MethodInfo {
                signature,
                visibility,
                scope_id: current.as_u32(),
                has_self,
            };
            self.scopes[current.index()].insert_method(type_name, method_info);
            Ok(())
        } else {
            bail!("No active scope to register method")
        }
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn lookup_method(&self, type_name: &str, method_name: &str) -> Option<MethodInfo> {
        let mut cursor = self.current_scope;
        while let Some(id) = cursor {
            let s = self.scope(id)?;
            if let Some(method_info) = s
                .methods
                .get(type_name)
                .and_then(|methods| methods.iter().find(|m| m.signature.name == method_name))
            {
                return Some(method_info.clone());
            }
            cursor = s.parent;
        }
        None
    }

    /// Re-resolves every registered function and method signature, and every
    /// struct's field types, against its own scope, after imports are bound.
    ///
    /// Signatures and field types are first resolved at registration time, before
    /// imports are bound, so an item-imported or `::`-qualified type
    /// (`use a::b::{Point};` / `lib::geom::Point`) used in a param, return, or
    /// field position stays an unresolved `Custom`/`Qualified` name — while a call
    /// or construction site infers `Struct("Point", key)`. The two would then fail
    /// to unify (`expected Point, found Point`), and a qualified field type would
    /// reach code generation unresolved. Running once more here, with each item's
    /// own scope active so its imports and namespace bindings are visible, rewrites
    /// those names to canonical `Struct`/`Enum`, matching what use sites infer.
    pub(crate) fn renormalize_signatures(&mut self) {
        let previous = self.current_scope;
        for index in 0..self.scopes.len() {
            let id = ScopeId(index as u32);
            self.current_scope = Some(id);

            let func_names: Vec<String> = self.scopes[index]
                .symbols
                .iter()
                .filter(|(_, sym)| sym.as_function().is_some())
                .map(|(name, _)| name.clone())
                .collect();
            for name in func_names {
                let sig = self.scopes[index]
                    .lookup_symbol_local(&name)
                    .and_then(|s| s.as_function().cloned());
                if let Some(mut sig) = sig {
                    sig.param_types = sig
                        .param_types
                        .into_iter()
                        .map(|ti| self.resolve_custom_type(ti))
                        .collect();
                    sig.return_type = self.resolve_custom_type(sig.return_type);
                    self.scopes[index]
                        .symbols
                        .insert(name, Symbol::Function(sig));
                }
            }

            let method_keys: Vec<String> = self.scopes[index].methods.keys().cloned().collect();
            for type_name in method_keys {
                let mut methods = self.scopes[index]
                    .methods
                    .get(&type_name)
                    .cloned()
                    .unwrap_or_default();
                for method in &mut methods {
                    method.signature.param_types = std::mem::take(&mut method.signature.param_types)
                        .into_iter()
                        .map(|ti| self.resolve_custom_type(ti))
                        .collect();
                    method.signature.return_type =
                        self.resolve_custom_type(std::mem::take(&mut method.signature.return_type));
                }
                self.scopes[index].methods.insert(type_name, methods);
            }

            let struct_names: Vec<String> = self.scopes[index]
                .symbols
                .iter()
                .filter(|(_, sym)| sym.as_struct().is_some())
                .map(|(name, _)| name.clone())
                .collect();
            for name in struct_names {
                let info = self.scopes[index]
                    .lookup_symbol_local(&name)
                    .and_then(|s| s.as_struct().cloned());
                if let Some(mut info) = info {
                    for field in &mut info.fields {
                        field.type_info =
                            self.resolve_custom_type(std::mem::take(&mut field.type_info));
                    }
                    self.scopes[index].symbols.insert(name, Symbol::Struct(info));
                }
            }
        }
        self.current_scope = previous;
    }

    /// Re-normalizes the signatures stored in item-import bindings, the sibling of
    /// [`Self::renormalize_signatures`] and run immediately after it.
    ///
    /// An item import (`use a::b::{f};`) captures an independent *copy* of the
    /// imported function symbol at resolution time — before signatures are
    /// re-normalized — so its param and return types stay bare `Custom` names even
    /// after `renormalize_signatures` canonicalizes the scope-stored original. A
    /// reader of the binding (`f(x)`, or `b::f(x)`) would then compare a bare
    /// `Custom` param against a canonical `Struct`/`Enum` argument and falsely
    /// reject the call. Re-resolving each bound function's signature against the
    /// function's *own* defining scope — where its item-imports are bound — makes
    /// the copy carry the same canonical keys as the original (#63).
    ///
    /// Unlike [`Self::renormalize_signatures`], this resolves through an explicit
    /// `definition_scope_id` rather than activating each scope as the cursor, so it
    /// leaves `current_scope` untouched.
    pub(crate) fn renormalize_resolved_imports(&mut self) {
        for index in 0..self.scopes.len() {
            let names: Vec<String> = self.scopes[index]
                .resolved_imports
                .keys()
                .cloned()
                .collect();
            for name in names {
                let binding = self.scopes[index].resolved_imports.get(&name).cloned();
                let Some(mut binding) = binding else {
                    continue;
                };
                let ResolvedImportTarget::Item {
                    symbol,
                    definition_scope_id,
                } = &mut binding.target
                else {
                    continue;
                };
                let Symbol::Function(sig) = symbol.as_mut() else {
                    continue;
                };
                let def_scope = *definition_scope_id;
                sig.param_types = std::mem::take(&mut sig.param_types)
                    .into_iter()
                    .map(|ti| self.resolve_custom_type_in_scope(ti, def_scope))
                    .collect();
                sig.return_type = self
                    .resolve_custom_type_in_scope(std::mem::take(&mut sig.return_type), def_scope);
                self.scopes[index].resolved_imports.insert(name, binding);
            }
        }
    }

    /// Enters the scope for spec `name`, creating it on first entry and
    /// re-entering the same scope on subsequent calls. Re-entry preserves
    /// the original scope id so symbols registered across the type checker's
    /// three phases (`register_types`, `collect_function_and_constant_definitions`,
    /// `infer_def`) all land in the same logical scope and are mutually visible.
    ///
    /// Spec scopes are keyed by their **file-qualified** path — the enclosing
    /// file scope's `full_path` joined with the spec name (`lib::a::Sp` vs
    /// `lib::b::Sp`), mirroring how struct/enum type identity is file-qualified
    /// (#63). Keying by the bare name would make two files that each declare
    /// `spec Sp` share one scope: re-entering a's `Sp` while processing b's `Sp`
    /// lets b's spec see a's spec-private items (a privacy leak that also
    /// surfaces as a codegen "function not found" panic in proof mode), and
    /// falsely rejects same-named inner functions as duplicates. The caller
    /// enters the file scope before entering the spec, so `current_scope` is the
    /// file scope and its `full_path` is the qualifying prefix.
    pub(crate) fn enter_spec(&mut self, name: &str) -> u32 {
        let key = self.spec_scope_key(name);
        if let Some(&existing) = self.spec_scopes.get(&key) {
            self.current_scope = Some(existing);
            return existing.as_u32();
        }
        let scope_id = self.push_scope_with_name(name, Visibility::Public);
        self.spec_scopes.insert(key, ScopeId(scope_id));
        scope_id
    }

    /// File-qualified key for the spec `name` hanging off the current (file)
    /// scope: the enclosing scope's `full_path` joined with the spec name, or
    /// the bare name when the enclosing scope is the entry file (root). Mirrors
    /// the `full_path` [`Self::push_scope_with_name`] would assign the spec
    /// scope, so the key is stable across re-entry within the same file and
    /// distinct between files.
    #[must_use = "this is a pure key computation with no side effects"]
    fn spec_scope_key(&self, name: &str) -> String {
        match self.current_scope.and_then(|id| self.scope(id)) {
            Some(scope) => {
                let parent_path = &scope.full_path;
                if parent_path.is_empty() {
                    name.to_string()
                } else {
                    format!("{parent_path}::{name}")
                }
            }
            None => name.to_string(),
        }
    }

    /// Whether `scope_id` is the scope of a `spec` block.
    ///
    /// `spec_scopes` is the single source of truth for spec-ness: a scope is a
    /// spec scope iff it was registered there by [`Self::enter_spec`]. A function
    /// whose `definition_scope_id` is a spec scope is a spec-inner (proof-only)
    /// function, so this lets a caller reject reaching it through a qualified
    /// path — codegen never assigns spec functions an executable index.
    #[must_use = "this is a pure check with no side effects"]
    pub(crate) fn is_spec_scope(&self, scope_id: u32) -> bool {
        self.spec_scopes.values().any(|id| id.as_u32() == scope_id)
    }

    /// Whether `scope_id` is a spec scope or is nested inside one, walking the
    /// parent chain. A function or method whose defining scope satisfies this is
    /// proof-only: it lives in a `spec` (directly, or inside a `spec`-inner
    /// struct), so codegen never assigns it an executable index. This is the
    /// transitive form of [`Self::is_spec_scope`], needed because a `spec`-inner
    /// *struct*'s associated function registers in the struct's own scope, whose
    /// parent — not itself — is the spec scope, so the direct check would miss it.
    #[must_use = "this is a pure check with no side effects"]
    pub(crate) fn scope_is_within_spec(&self, scope_id: u32) -> bool {
        let mut cursor = Some(ScopeId(scope_id));
        while let Some(id) = cursor {
            let Some(scope) = self.scope(id) else { break };
            if self.is_spec_scope(id.as_u32()) {
                return true;
            }
            cursor = scope.parent;
        }
        false
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
                self.current_scope = Some(existing);
            } else {
                let id = ScopeId(self.push_scope_with_name(segment, Visibility::Public));
                let full_path = self.scopes[id.index()].full_path.clone();
                self.mod_scopes.insert(full_path, id);
            }
        }
        self.current_scope_id().unwrap_or(0)
    }

    /// Reassigns `current_scope` to the root scope. Counterpart to the
    /// per-file/per-spec scope walks, letting a registration pass return to a
    /// known anchor before entering the next file.
    pub(crate) fn reset_to_root(&mut self) {
        self.current_scope = self.root_scope;
    }

    /// Returns the direct child scope of `current_scope` whose name matches
    /// `name`, if one exists. Used to make file-scope creation idempotent across
    /// registration passes without re-walking from the root each time.
    #[must_use = "this is a pure lookup with no side effects"]
    fn child_scope_named(&self, name: &str) -> Option<ScopeId> {
        let children = &self.scope(self.current_scope?)?.children;
        children
            .iter()
            .copied()
            .find(|&c| self.scope(c).is_some_and(|s| s.name == name))
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn find_module_scope(&self, path: &[String]) -> Option<u32> {
        let key = path.join("::");
        self.mod_scopes.get(&key).map(|id| id.as_u32())
    }

    /// Returns the `::`-joined module path of the scope `scope_id` — its cached
    /// `full_path`. For a top-level definition's scope this is its defining
    /// file's path (`lib::arith`); the empty string for the entry file (root).
    /// Used to name the defining file in a cross-file private-access diagnostic.
    #[must_use = "the module path is the return value"]
    pub(crate) fn module_path_of_scope(&self, scope_id: u32) -> String {
        self.scope(ScopeId(scope_id))
            .map(|s| s.full_path.clone())
            .unwrap_or_default()
    }

    /// Returns the scope ancestry of `scope_id` from itself up to the root, in
    /// near-to-far order. Used to resolve a bare definition reference the way
    /// name lookup would: own scope first, then enclosing scopes, then root.
    #[must_use = "the ancestry is the return value"]
    pub(crate) fn scope_ancestry(&self, scope_id: u32) -> Vec<u32> {
        let mut chain = Vec::new();
        let mut cursor = Some(ScopeId(scope_id));
        while let Some(id) = cursor {
            let Some(scope) = self.scope(id) else { break };
            chain.push(id.as_u32());
            cursor = scope.parent;
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

    /// Whether `name` is the terminal segment of some file in the project (a file
    /// whose namespace could be bound by `use ...::{name};`) but is not currently
    /// reachable as a namespace from `from_scope_id`. This distinguishes a bare
    /// `name::fn()` call whose head names a real-but-unimported file namespace —
    /// where the fix is to add a `use` — from one whose head is a genuinely
    /// unknown type. Used to sharpen a missing-import diagnostic; it never feeds
    /// real resolution.
    #[must_use = "this is a pure check with no side effects"]
    pub(crate) fn name_is_unimported_namespace(&self, name: &str, from_scope_id: u32) -> bool {
        if self.namespace_binding_scope(name, from_scope_id).is_some() {
            return false;
        }
        self.mod_scopes
            .keys()
            .filter(|k| !k.is_empty())
            .any(|k| k.rsplit("::").next() == Some(name))
    }

    /// Whether `from_scope_id`'s file imported the namespace whose `::`-joined
    /// module-path key is `key` **exactly**. This reads the accessing file's own
    /// `use` manifest ([`Self::imported_namespace_keys`], which stops at the file
    /// boundary), so no namespace another file dragged into the compilation
    /// closure counts. The exact-equality primitive shared by the tail-precedence
    /// decision in [`Self::resolve_longest_namespace_prefix`] (which asks about the
    /// walked scope's parent file) and the terminal surface-read gate
    /// ([`Self::may_read_namespace_surface`]). Contrast
    /// [`Self::file_imports_namespace_at_or_under`], the prefix form used for
    /// pass-through hops.
    #[must_use = "this is a pure check with no side effects"]
    fn file_imports_namespace_key(&self, from_scope_id: u32, key: &str) -> bool {
        self.imported_namespace_keys(from_scope_id)
            .iter()
            .any(|k| k == key)
    }

    /// Whether `from_scope_id`'s file imported the namespace `dest`, or any
    /// namespace nested under it. `use lib::geom::sub;` returns `true` for `dest`
    /// of `lib`, `lib::geom`, and `lib::geom::sub` — the import licenses the file
    /// to walk *through* the pass-through hops `lib` and `lib::geom` to reach the
    /// sub-file it actually named, so the long form `lib::geom::sub::deep` resolves
    /// (the prefix form, not equality). It does **not** license a sibling the file
    /// never named: `lib::geom::other` is not `dest`-or-under any import, and
    /// `use a;` does not license descending into `a::b` (`a` is not a prefix of the
    /// hop key `a::b`).
    ///
    /// This is the cross-file descent gate: every hop that crosses from one file's
    /// scope into another's is admitted only when the accessing file's own manifest
    /// reaches the destination, so a file sees another file's surface strictly
    /// through its own `use`. Reading only `from_scope_id`'s manifest makes the
    /// decision closure-independent — no namespace another file dragged into the
    /// compilation closure can flip it (#63).
    #[must_use = "this is a pure check with no side effects"]
    fn file_imports_namespace_at_or_under(&self, from_scope_id: u32, dest: &str) -> bool {
        let prefix = format!("{dest}::");
        self.imported_namespace_keys(from_scope_id)
            .iter()
            .any(|k| k == dest || k.starts_with(&prefix))
    }

    /// Diagnoses a `::`-qualified `dir::file::item` path the accessing file is not
    /// licensed to spell, distinguishing a *confident* missing-import (the deepest
    /// file the path descends into is in the closure; the exact `use` is known)
    /// from a *hedged* one (the descent fell short of a real file, so the target's
    /// existence is unproven). Returns `None` when the path is not a leak the
    /// diagnostic should pre-empt: the head names a local type, the path is the
    /// entry's own root-scope surface, the file already imports the namespace, or
    /// no project namespace is involved (a genuine typo, left to report as
    /// undefined).
    ///
    /// The namespace is found by a *structural*, gate-free, import-free walk
    /// ([`Self::deepest_namespace_scope_structural`]): it shares the resolver's
    /// anchor ([`Self::start_qualified_walk`]) and then hops **child** file scopes
    /// only ([`ReachedVia::Child`], stopping at a re-export hop) for as long as each
    /// segment names a real file, reaching the deepest file `deep` the path descends
    /// into. The descent gate is deliberately *not* applied here — the diagnostic
    /// must see the structure that resolution refused. When the walk reaches the
    /// path's actual target namespace (it consumed the whole namespace portion, or
    /// stopped because the next segment names a type defined in `deep`), the
    /// suggestion is exact: `use {deep}` ([`UnimportedNamespace::Confident`], whose
    /// namespace is always a parseable file, never `…::Type`, with the type-access
    /// tail named as the `item`). When it falls short — the next segment is neither a
    /// child file nor a type there, so the target sub-file was not compiled — the
    /// deepest proven file plus the next plausible segment (`path[..k + 1]`) is
    /// offered as a best-guess ([`UnimportedNamespace::Hedged`]), still a parseable
    /// file namespace rather than a path carrying a type segment.
    #[must_use = "this is a pure check with no side effects"]
    pub(crate) fn unimported_namespace_prefix(
        &self,
        path: &[String],
        from_scope_id: u32,
    ) -> Option<UnimportedNamespace> {
        // A head that names a type or enum defined in the accessing file is a
        // local type-member (`Color::Red`, `Vec::new`), not an absolute file path,
        // even when a same-named sibling file is in the import closure. The
        // qualified-path resolvers give the local type precedence at the head, so
        // this missing-import diagnostic must not pre-empt it. The pre-empt applies
        // only when the head can be a type-access for the whole path — at most two
        // segments (the type plus an optional member); a head with two or more
        // further segments is a namespace path whose missing import should report.
        if path.len() <= 2 && self.head_type_preempts_sibling_file(&path[0], from_scope_id) {
            return None;
        }
        // An absolute path whose head is *not* a directory namespace names the
        // entry's own root-scope surface (`MySpec::fn`, `MyType::assoc`), which the
        // entry owns and may always spell — not a cross-file leak. This is the
        // entry-own-surface half of the structural anchor router; a directory-
        // descent path is exactly the diagnostic's input and is not suppressed here.
        if Some(self.enclosing_file_scope(from_scope_id)) == self.root_scope_id()
            && !self.path_descends_into_directory_namespace(path)
        {
            return None;
        }
        if path.len() < 2 {
            return None;
        }
        // The deepest real file the path's leading segments descend into (following
        // child file scopes only), and the number of segments that walk consumed.
        let (deep_id, k) = self.deepest_namespace_scope_structural(path, from_scope_id)?;
        let deep_key = self.module_path_of_scope(deep_id);
        // The diagnostic suppresses (defers to a different, more precise error) only
        // when the file actually reaches `deep_key` (imports it) AND one of two
        // shapes holds. When `deep_key` is *not* imported, the path is a genuine
        // missing-import leak the hint must report — importing an *ancestor*
        // (`use a;` for `a::b`) does not read the descendant's surface, so it does
        // not suppress (that is the very leak the cross-file descent gate closes).
        if self.file_imports_namespace_key(from_scope_id, &deep_key) {
            // (a) The leaf is read *directly* in the imported `deep_key` (the walk
            // consumed every segment but the leaf). A genuinely-unknown leaf there
            // (`geo::Nope` with `use geo;`, `math::nope` with `use math;`) is not a
            // missing import — defer to the unknown-leaf / undefined-fn diagnostic.
            if k == path.len() - 1 {
                return None;
            }
            // (b) The next segment names something already present on `deep_key`'s
            // surface — a sub-file, a namespace or item import, or a local
            // definition. The path reaches *into* imported territory, so the failure
            // is downstream (a re-export blocked by a plain `use`, an item-imported
            // enum reached as a namespace, a member that does not exist), not a
            // missing file import. Defer to the more precise diagnostic for that
            // shape (`math::arith::add` where `math` plain-imports `lib::arith`;
            // `lib::mid::Color::Red` where `mid` item-imports `Color`).
            //
            // EXCEPT a same-named struct/enum at `path[k]` that the path descends
            // *past* non-viably: when `path[k]` is a type defined in `deep` but the
            // remaining suffix is not a viable type-access on it (the same
            // [`Self::type_shadow_viable_for_suffix`] decision the resolver applies
            // to the type-shadow break), the resolver consumes `path[k]` as a
            // sub-file namespace and descends into a deeper, *uncompiled* file — so the leak is a
            // missing import of that deeper file, which the shadowing type must not
            // mask. `lib::b::Point::make` where `lib` defines `struct b` and the
            // un-imported `lib/b.inf` defines `Point::make`: `b::Point::make` is not
            // a viable access on `struct b`, so the hint must name `lib::b`, not
            // suppress. A viable type-access (`lib::Point::new`) or a non-type
            // surface member still defers.
            let descends_past_shadow_type = self.scope_defines_type(&path[k], deep_id)
                && !self.type_shadow_viable_for_suffix(path, k, deep_id, from_scope_id);
            if self.scope_surface_contains(deep_id, &path[k]) && !descends_past_shadow_type {
                return None;
            }
            // Otherwise `deep_key` is imported but the path descends past it into a
            // segment absent from its surface — the deeper target file is uncompiled
            // (outside the closure). Its existence cannot be proven, so fall through
            // to the Hedged best-guess below rather than suppressing into a
            // misleading "undefined function" / "unknown type".
        }
        // The deepest real file is `deep_key`, and it is not imported. The fix is
        // exact (`use {deep_key}`, always a parseable file namespace) when the walk
        // reached the path's actual target namespace — either it consumed the whole
        // namespace portion as real files (`lib::geom::area`, `a::b::Type`: the leaf
        // is read directly in `deep`), or it stopped because `path[k]` names a type
        // in `deep` (`a::b::Type::make`: the remaining segments are a type-access
        // into the real file). The `item` is the type-access tail read inside
        // `deep` — `path[k..]` — so the message names the full path the import
        // unlocks (`lib::b::Point::make`), not just the leaf member.
        let namespace_portion = path[..path.len() - 1].join("::");
        if deep_key == namespace_portion
            || (k < path.len()
                && self.scope_defines_type(&path[k], deep_id)
                && self.type_shadow_viable_for_suffix(path, k, deep_id, from_scope_id))
        {
            return Some(UnimportedNamespace::Confident {
                namespace: deep_key,
                item: path[k..].join("::"),
            });
        }
        // The walk fell short of a real file (the target sub-file was not compiled).
        // Offer the deepest proven file plus the next segment the path tried to
        // descend into — `path[..k + 1]` — as the longest plausible *file* namespace.
        // This is one hop past what the walk proved (never the shallower directory
        // `deep_key` alone), and never includes a type segment: the walk stopped at
        // `k` because `path[k]` is not a child file, and the Confident arm above
        // already excluded `path[k]` being a type in `deep`, so the appended segment
        // is plausibly a file. For a type-access value path (`lib::b::Point::make`)
        // this names `lib::b`, never the unparseable `lib::b::Point`. The bound is
        // clamped: a path whose every segment (leaf included) named a real child
        // file reaches here with `k == len`, and the clamp keeps the slice in range.
        let plausible_namespace_end = (k + 1).min(path.len());
        Some(UnimportedNamespace::Hedged {
            namespace: path[..plausible_namespace_end].join("::"),
        })
    }

    /// The deepest file scope `path`'s leading segments structurally descend into
    /// from `from_scope_id`, and the number of segments consumed reaching it. A
    /// *structural*, gate-free walk used only by the missing-import diagnostic
    /// ([`Self::unimported_namespace_prefix`]): it must see the file structure even
    /// where the descent gate refused resolution.
    ///
    /// The anchor is shared with the resolver ([`Self::start_qualified_walk`]) so
    /// the two never disagree on where a path begins; each subsequent hop follows a
    /// **child** file scope only ([`ReachedVia::Child`]), stopping at a re-export
    /// hop. A re-export target is a *reachable* namespace, not a missing one, so it
    /// is never a candidate for a "you forgot to import this" hint — only raw
    /// file-nesting descent names the deepest file the `use` suggestion should
    /// point at. Returns `None` only when the path cannot anchor at all (no bound
    /// namespace, no directory head, not the entry surface).
    #[must_use = "the scope and count are the return value"]
    fn deepest_namespace_scope_structural(
        &self,
        path: &[String],
        from_scope_id: u32,
    ) -> Option<(u32, usize)> {
        let (anchor, remaining) = self.start_qualified_walk(path, from_scope_id)?;
        let mut current = anchor;
        let mut consumed = path.len() - remaining.len();
        for segment in remaining {
            match self.next_namespace_scope(current, segment) {
                Some((next, ReachedVia::Child)) if self.scope_is_file_namespace(next.as_u32()) => {
                    current = next;
                    consumed += 1;
                }
                _ => break,
            }
        }
        Some((current.as_u32(), consumed))
    }

    /// Whether `name` appears anywhere on the *surface* of scope `scope_id` — as a
    /// child file scope, a resolved import (namespace or item, re-exported or plain),
    /// or a local definition. Used by the missing-import diagnostic to decide
    /// whether a path that descends past an imported namespace reaches *into* that
    /// namespace's surface (so the failure is downstream — a blocked re-export, an
    /// item reached as a namespace, an unknown member — and the missing-import hint
    /// must defer) or names a genuinely-absent deeper file (so the uncompiled-target
    /// Hedged best-guess applies). It is intentionally permissive: any presence at
    /// all means "not a missing file import".
    #[must_use = "this is a pure check with no side effects"]
    fn scope_surface_contains(&self, scope_id: u32, name: &str) -> bool {
        let Some(scope) = self.scope(ScopeId(scope_id)) else {
            return false;
        };
        scope
            .children
            .iter()
            .any(|&c| self.scope(c).is_some_and(|s| s.name == name))
            || scope.resolved_imports.contains_key(name)
            || scope.lookup_symbol_local(name).is_some()
    }

    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn current_scope_id(&self) -> Option<u32> {
        self.current_scope.map(ScopeId::as_u32)
    }

    /// The root scope id — the program scope that holds the entry file's
    /// definitions. `use root;` (Inference's `@import("root")`) binds its name to
    /// this scope so a non-entry file can reach the entry's `pub` surface.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn root_scope_id(&self) -> Option<u32> {
        self.root_scope.map(ScopeId::as_u32)
    }

    /// Borrows the scope with `u32` id `scope_id`, or `None` when out of range.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn get_scope(&self, scope_id: u32) -> Option<&Scope> {
        self.scope(ScopeId(scope_id))
    }

    /// Mutably borrows the scope with `u32` id `scope_id`, or `None` when out of
    /// range. The `&mut` counterpart of [`Self::get_scope`] for the crate-internal
    /// callers that bind a resolved import into an existing scope.
    #[must_use = "the mutable scope borrow is the return value; dropping it makes the call a no-op"]
    pub(crate) fn get_scope_mut(&mut self, scope_id: u32) -> Option<&mut Scope> {
        self.scopes.get_mut(ScopeId(scope_id).index())
    }

    /// Borrows the scope with id `id`, or `None` when the id is out of range.
    /// The single indexing primitive the scope-tree walks are built on.
    #[must_use = "this is a pure lookup with no side effects"]
    fn scope(&self, id: ScopeId) -> Option<&Scope> {
        self.scopes.get(id.index())
    }

    pub(crate) fn register_import(&mut self, import: Import) -> anyhow::Result<()> {
        if let Some(current) = self.current_scope {
            self.scopes[current.index()].add_import(import);
            Ok(())
        } else {
            bail!("No active scope to register import")
        }
    }

    /// Get all scope IDs for iteration, in ascending (allocation) order.
    #[must_use = "discarding the scope IDs has no effect"]
    pub(crate) fn all_scope_ids(&self) -> Vec<u32> {
        (0..self.scopes.len() as u32).collect()
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

        let (start_scope, module_path) = self.start_qualified_walk(path, from_scope_id)?;
        self.walk_qualified_from(start_scope, module_path, from_scope_id, true)
    }

    /// Walks the namespace chain `module_path` from `start_scope`, returning the
    /// symbol the final segment names (a local definition or a followable item
    /// import) and its defining scope. The intermediate hops follow child scopes
    /// and re-exported namespace imports; the leaf may also resolve through an
    /// item import, gated by the same re-export rule when it crosses a file
    /// boundary out of `from_scope_id`'s file. Shared by
    /// [`Self::resolve_qualified_name`] and [`Self::resolve_import_path`] so the
    /// two differ only in how the start scope is anchored.
    ///
    /// `gate_descent` applies the cross-file import discipline to the walk: each
    /// intermediate *child* hop must pass [`Self::may_descend_through_namespace`]
    /// and the terminal namespace (when reached by a child hop) must pass
    /// [`Self::may_read_namespace_surface`], so a body reference reaches another
    /// file's surface only through this file's own `use` or a `pub use` re-export
    /// chain. Re-export hops carry their own discipline (the `reexported` flag in
    /// [`Self::next_namespace_scope`]) and are never re-gated by the manifest — a
    /// terminal reached by a re-export hop is exempt from the surface gate, or
    /// `pub use` would be defeated. [`Self::resolve_qualified_name`] (body
    /// references) passes `true`; [`Self::resolve_import_path`] passes `false`
    /// because an import declaration's own path *is* the dependency it declares —
    /// gating it would make `use lib::geom::{val};` fail to resolve its own prefix
    /// (#63).
    #[must_use = "this is a pure lookup with no side effects"]
    fn walk_qualified_from(
        &self,
        start_scope: ScopeId,
        module_path: &[String],
        from_scope_id: u32,
        gate_descent: bool,
    ) -> Option<(Symbol, u32)> {
        let mut current_scope = start_scope;
        // How the most recent hop reached `current_scope`; `None` for the anchor.
        // A terminal namespace reached by a re-export hop is exempt from the
        // surface gate (the re-export licensed it).
        let mut last_via: Option<ReachedVia> = None;
        for (i, segment) in module_path.iter().enumerate() {
            let is_last = i == module_path.len() - 1;
            if is_last {
                // The leaf is read in the terminal namespace `current_scope`. When
                // the terminal was reached by child-descent (or is the anchor), a
                // cross-file surface read requires an exact import of that namespace.
                // A terminal reached by a re-export hop is already licensed.
                if gate_descent
                    && last_via != Some(ReachedVia::Reexport)
                    && !self.may_read_namespace_surface(from_scope_id, current_scope)
                {
                    return None;
                }
                let scope = self.scope(current_scope)?;
                if let Some(symbol) = scope.lookup_symbol_local(segment) {
                    return Some((symbol.clone(), scope.id.as_u32()));
                }
                if let Some(resolved) = scope.resolved_imports.get(segment)
                    && let ResolvedImportTarget::Item {
                        symbol,
                        definition_scope_id,
                    } = &resolved.target
                {
                    // An item brought in by `use a::b::{x};` is private to the
                    // file that wrote the import unless it is re-exported. Reaching
                    // it as `b::x` from another file traverses across a file
                    // boundary, so it is followed only when the import is a
                    // `pub use` — mirroring the intermediate-hop rule in
                    // [`Self::next_namespace_scope`]. The accessing file reaching
                    // its own item import (the import scope is in the accessor's
                    // ancestry) is not a boundary crossing and stays allowed.
                    let crosses_file_boundary = !self
                        .scope_ancestry(from_scope_id)
                        .contains(&scope.id.as_u32());
                    if !crosses_file_boundary || resolved.reexported {
                        return Some(((**symbol).clone(), *definition_scope_id));
                    }
                    return None;
                }
                return None;
            }

            let (next, via) = self.next_namespace_scope(current_scope, segment)?;
            // Only file-nesting child descent is gated by the manifest; a re-export
            // hop carries its own discipline and is followed freely.
            if gate_descent
                && via == ReachedVia::Child
                && !self.may_descend_through_namespace(from_scope_id, current_scope, next)
            {
                return None;
            }
            current_scope = next;
            last_via = Some(via);
        }

        None
    }

    /// Selects the start scope and remaining path for a qualified-name walk.
    ///
    /// A leading `self` anchors the walk at the accessing scope; any other first
    /// segment that names a namespace bound in the accessing scope (a `use a::b;`
    /// import) anchors there too, so a file can reach an imported namespace by
    /// the bound name. Otherwise the path is treated as an absolute `dir::file`
    /// chain rooted at the entry file when it *structurally* can be — when the head
    /// names a directory namespace, or the accessing file is the entry naming its
    /// own root surface ([`Self::file_may_anchor_absolute_path`], a pure structural
    /// router). Anchoring no longer enforces the cross-file import discipline: that
    /// is applied per-hop by [`Self::may_descend_through_namespace`] and at the leaf
    /// by [`Self::may_read_namespace_surface`] during the subsequent walk, so a file
    /// that never imported the head namespace fails not at the anchor but at the
    /// first cross-file hop. The returned slice is the path with any consumed `self`
    /// stripped.
    fn start_qualified_walk<'p>(
        &self,
        path: &'p [String],
        from_scope_id: u32,
    ) -> Option<(ScopeId, &'p [String])> {
        let first_segment = &path[0];
        if first_segment == "self" {
            let id = ScopeId(from_scope_id);
            self.scope(id)?;
            return Some((id, &path[1..]));
        }
        if let Some(scope_id) = self.namespace_binding_scope(first_segment, from_scope_id)
            && self.scope(ScopeId(scope_id)).is_some()
        {
            return Some((ScopeId(scope_id), &path[1..]));
        }
        if self.file_may_anchor_absolute_path(path, from_scope_id) {
            return Some((self.root_scope?, path));
        }
        None
    }

    /// Resolves a `::`-separated path the way [`Self::resolve_qualified_name`]
    /// does, but always permits an absolute anchor at the entry file regardless of
    /// the accessing file's imports. This is the resolver for an **import
    /// declaration's own path** (`use lib::geom::{val};`): the act of importing is
    /// what declares the dependency, so the file-boundary import discipline that
    /// gates body references must not apply to the declaration itself — a file
    /// importing `lib::geom::val` has not yet recorded `lib::geom` as imported at
    /// the moment its prefix is resolved.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn resolve_import_path(
        &self,
        path: &[String],
        from_scope_id: u32,
    ) -> Option<(Symbol, u32)> {
        if path.is_empty() {
            return None;
        }
        let (start_scope, remaining) = if path[0] == "self" {
            let id = ScopeId(from_scope_id);
            self.scope(id)?;
            (id, &path[1..])
        } else if let Some(scope_id) = self.namespace_binding_scope(&path[0], from_scope_id)
            && self.scope(ScopeId(scope_id)).is_some()
        {
            (ScopeId(scope_id), &path[1..])
        } else {
            (self.root_scope?, path)
        };
        self.walk_qualified_from(start_scope, remaining, from_scope_id, false)
    }

    /// Whether `path` may *structurally* anchor at the root scope as an absolute
    /// chain — that is, whether root has somewhere to start the walk. This is a
    /// pure structural router, not an import gate: the file-scoped import
    /// discipline lives entirely in the descent gate
    /// ([`Self::may_descend_through_namespace`]), which every cross-file hop of the
    /// subsequent walk must pass. Anchoring merely picks the starting scope; the
    /// first hop that crosses a file boundary is where disclosure is decided.
    ///
    /// Root is a valid anchor in exactly two cases:
    ///
    /// - The head names a **directory namespace** (`lib::geom::…`, the root child
    ///   that begins another file's path). The walk may *begin* there; whether it
    ///   may *descend* into the target file is the descent gate's call, applied at
    ///   the hop. This is why anchoring no longer needs the accessing file's
    ///   manifest — admitting the anchor discloses nothing on its own.
    /// - The accessing file **is the entry** (its enclosing file scope is root), so
    ///   an absolute head names the entry's own root-scope surface (an entry
    ///   `spec`, type, fn, or const reached as `MySpec::fn` / `MyType::assoc`). A
    ///   non-entry file's own top-level items live in its *own* file scope, not at
    ///   root, so this never reaches another file's surface.
    #[must_use = "this is a pure check with no side effects"]
    fn file_may_anchor_absolute_path(&self, path: &[String], from_scope_id: u32) -> bool {
        self.path_descends_into_directory_namespace(path)
            || Some(self.enclosing_file_scope(from_scope_id)) == self.root_scope_id()
    }

    /// Whether `path`'s head names a directory namespace — a file or directory
    /// whose `::`-joined module path is a [`Self::mod_scopes`] key (`lib`,
    /// `lib::geom`). Such a head means the absolute path descends into another
    /// file's surface, so the import discipline applies. A head that matches no
    /// directory-namespace key instead names the entry's own root-scope definition
    /// (an entry `spec`/type/fn/const), which is not subject to import discipline.
    #[must_use = "this is a pure check with no side effects"]
    fn path_descends_into_directory_namespace(&self, path: &[String]) -> bool {
        let Some(head) = path.first() else {
            return false;
        };
        self.mod_scopes
            .keys()
            .filter(|k| !k.is_empty())
            .any(|k| k.split("::").next() == Some(head.as_str()))
    }

    /// The `::`-joined module-path keys of every namespace this file imported,
    /// collected by walking `from_scope_id`'s parent chain up to (but not across)
    /// the file boundary. A non-entry file's parent chain runs into the entry
    /// file, so an enclosing file's imports must not count as this file's; the
    /// boundary is tracked exactly as [`Self::namespace_binding_scope`] does.
    #[must_use = "the keys are the return value"]
    fn imported_namespace_keys(&self, from_scope_id: u32) -> Vec<String> {
        let mut keys = Vec::new();
        let mut cursor = Some(ScopeId(from_scope_id));
        let mut crossed_file_boundary = false;
        while let Some(id) = cursor {
            let Some(s) = self.scope(id) else { break };
            if !crossed_file_boundary {
                for resolved in s.resolved_imports.values() {
                    if let ResolvedImportTarget::Namespace { scope_id } = &resolved.target {
                        keys.push(self.module_path_of_scope(*scope_id));
                    }
                }
            }
            if self.is_non_entry_file_scope(id.as_u32()) {
                crossed_file_boundary = true;
            }
            cursor = s.parent;
        }
        keys
    }

    /// Finds the scope a `use a::b;` namespace import named `name` redirects to,
    /// walking `from_scope_id`'s parent chain — but only within the originating
    /// file. The binding lives in the file scope, while a reference (`b::fn()`) is
    /// written inside a function body scope, so the lookup must climb to the file
    /// scope to find it — mirroring [`Self::lookup_imported_item_symbol`] for bare
    /// item imports.
    ///
    /// The walk stops at the file boundary. A non-entry file's parent chain runs
    /// up into the entry file (root); without the boundary, an entry-file
    /// `use lib::Point;` would bind `Point` as a namespace that a *different*
    /// imported file's bare `Point::new()` then resolves through — silently
    /// hijacking that file's own local `struct Point`. A namespace import is
    /// private to the file that wrote it: a file resolves a bare qualified call
    /// against its own scope and its own imports, never an enclosing file's. This
    /// is the same file-scoped discipline as [`Self::lookup_symbol_file_scoped_from`]
    /// (#63).
    #[must_use = "this is a pure lookup with no side effects"]
    fn namespace_binding_scope(&self, name: &str, from_scope_id: u32) -> Option<u32> {
        let mut cursor = Some(ScopeId(from_scope_id));
        let mut crossed_file_boundary = false;
        while let Some(id) = cursor {
            let s = self.scope(id)?;
            // Once the walk has climbed out of the originating file, the imports it
            // now sees belong to an enclosing file and must not bind this file's
            // names.
            if !crossed_file_boundary
                && let Some(resolved) = s.resolved_imports.get(name)
                && let ResolvedImportTarget::Namespace { scope_id } = &resolved.target
            {
                return Some(*scope_id);
            }
            // Leaving a non-entry file namespace means the next hop crosses into an
            // enclosing file; record it before advancing. The entry file is the
            // root scope, so its own lookups never cross and keep seeing their own
            // imports.
            if self.is_non_entry_file_scope(id.as_u32()) {
                crossed_file_boundary = true;
            }
            cursor = s.parent;
        }
        None
    }

    /// Advances one intermediate hop of a qualified-name walk: the next scope is
    /// a direct child named `segment`, or a **re-exported** namespace import
    /// named `segment` in `current`. The returned [`ReachedVia`] records which,
    /// so the caller applies the manifest descent gate to child hops only and
    /// leaves re-export hops to their own discipline.
    ///
    /// The accessing file's own import (which may be private) is consumed by
    /// [`Self::start_qualified_walk`] as the entry point, so by the time this
    /// runs every hop crosses into another file's namespace; a plain (non-`pub`)
    /// import there is private to its own file and is never followed. That is
    /// what makes `math::arith::add` resolve only when `math` writes
    /// `pub use lib::arith;`, not a plain `use`.
    fn next_namespace_scope(
        &self,
        current: ScopeId,
        segment: &str,
    ) -> Option<(ScopeId, ReachedVia)> {
        let scope = self.scope(current)?;
        if let Some(&child) = scope
            .children
            .iter()
            .find(|&&c| self.scope(c).is_some_and(|s| s.name == segment))
        {
            return Some((child, ReachedVia::Child));
        }
        if let Some(resolved) = scope.resolved_imports.get(segment)
            && let ResolvedImportTarget::Namespace { scope_id } = &resolved.target
            && resolved.reexported
        {
            return Some((ScopeId(*scope_id), ReachedVia::Reexport));
        }
        None
    }

    /// Whether a namespace walk in `from_scope_id`'s file may take the *intermediate
    /// hop* from `current` into `next` — passing through `next` toward a deeper
    /// target. A hop that crosses a file boundary is admitted when the accessing
    /// file's own `use` manifest reaches `next` **or any namespace under it**
    /// ([`Self::file_imports_namespace_at_or_under`], the prefix form): importing
    /// `lib::geom::sub` licenses passing through the hops `lib` and `lib::geom` to
    /// reach it in long form. A hop that stays within the current file (a directory
    /// scope, or the entry's own surface), or descends into a non-file scope,
    /// carries no cross-file disclosure and is always admitted.
    ///
    /// This is the pass-through half of the cross-file discipline; the *terminal*
    /// half ([`Self::may_read_namespace_surface`]) is stricter — passing through a
    /// namespace is not licence to read its surface. All three namespace-walk loops
    /// consult this on every intermediate hop — [`Self::resolve_longest_namespace_prefix`]
    /// (the type-path resolver), [`Self::prefix_is_namespace`] (the call-shape
    /// probe), and [`Self::walk_qualified_from`] (the value/function resolver) — so
    /// they agree by construction rather than by parallel re-derivation. Without it,
    /// `next_namespace_scope` would descend from one file's scope into a child file
    /// scope with no import check, letting a file importing only a parent reach an
    /// un-imported sibling file's surface (#63).
    #[must_use = "this is a pure check with no side effects"]
    fn may_descend_through_namespace(
        &self,
        from_scope_id: u32,
        current: ScopeId,
        next: ScopeId,
    ) -> bool {
        let next_id = next.as_u32();
        if !self.is_non_entry_file_scope(next_id) {
            return true;
        }
        let current_id = current.as_u32();
        if self.enclosing_file_scope(next_id) == self.enclosing_file_scope(current_id) {
            return true;
        }
        self.file_imports_namespace_at_or_under(from_scope_id, &self.module_path_of_scope(next_id))
    }

    /// Whether `from_scope_id`'s file may read the *surface* of `terminal` — the
    /// namespace scope a qualified path's leaf segment resolves in. This is the
    /// terminal half of the cross-file discipline, and it is stricter than the
    /// pass-through hop gate ([`Self::may_descend_through_namespace`]): a file may
    /// read another file's surface only through an **exact** import of that
    /// namespace ([`Self::file_imports_namespace_key`], equality), never through a
    /// deeper or shallower import it merely walked across.
    ///
    /// `use lib::geom::sub;` licenses reading `lib::geom::sub`'s own leaves
    /// (`lib::geom::sub::deep`) but not its parent `lib::geom`'s surface
    /// (`lib::geom::area`) — passing *through* `lib::geom` to reach the sub-file is
    /// not licence to read `lib::geom` itself. Symmetrically `use lib::geom;` does
    /// not reach the deeper `lib::geom::sub::deep`. Each `use` grants exactly its
    /// own namespace's surface; ancestors serve only pass-through spelling.
    ///
    /// A leaf read in the accessing file's *own* file scope needs no import, so the
    /// gate fires only on cross-file reads (the `enclosing_file_scope` inequality),
    /// mirroring the pass-through gate's same-file exemption.
    #[must_use = "this is a pure check with no side effects"]
    fn may_read_namespace_surface(&self, from_scope_id: u32, terminal: ScopeId) -> bool {
        let terminal_id = terminal.as_u32();
        if !self.is_non_entry_file_scope(terminal_id) {
            return true;
        }
        if self.enclosing_file_scope(terminal_id) == self.enclosing_file_scope(from_scope_id) {
            return true;
        }
        self.file_imports_namespace_key(from_scope_id, &self.module_path_of_scope(terminal_id))
    }

    /// Whether `path` names a **function** reachable from `from_scope_id` when the
    /// re-export gate is ignored — both on intermediate namespace hops and on the
    /// final item import. This is the gate-free twin of
    /// [`Self::resolve_qualified_name`], used only to sharpen a diagnostic: it
    /// distinguishes "the leaf exists but an intermediate file used a plain `use`
    /// instead of `pub use`" from "the leaf genuinely does not exist". It must
    /// never feed real resolution — resolution still honors the gate.
    ///
    /// Returns `true` only when the leaf is a function, so a struct or const
    /// reached gate-free does not trigger the "add `pub use`" hint for a call that
    /// would not be callable anyway.
    #[must_use = "this is a pure check with no side effects"]
    pub(crate) fn qualified_function_reachable_ignoring_reexport(
        &self,
        path: &[String],
        from_scope_id: u32,
    ) -> bool {
        if path.is_empty() {
            return false;
        }
        let Some((mut current_scope, module_path)) = self.start_qualified_walk(path, from_scope_id)
        else {
            return false;
        };
        for (i, segment) in module_path.iter().enumerate() {
            let is_last = i == module_path.len() - 1;
            if is_last {
                let Some(scope) = self.scope(current_scope) else {
                    return false;
                };
                if let Some(symbol) = scope.lookup_symbol_local(segment) {
                    return symbol.as_function().is_some();
                }
                if let Some(resolved) = scope.resolved_imports.get(segment)
                    && let ResolvedImportTarget::Item { symbol, .. } = &resolved.target
                {
                    return symbol.as_function().is_some();
                }
                return false;
            }
            match self.next_namespace_scope_ignoring_reexport(current_scope, segment) {
                Some(next) => current_scope = next,
                None => return false,
            }
        }
        false
    }

    /// Advances one intermediate hop like [`Self::next_namespace_scope`] but
    /// follows a namespace import even when it is a plain (non-`pub use`) binding.
    /// Used only by [`Self::qualified_function_reachable_ignoring_reexport`] to
    /// probe whether a path would resolve if the missing `pub use` were added.
    fn next_namespace_scope_ignoring_reexport(
        &self,
        current: ScopeId,
        segment: &str,
    ) -> Option<ScopeId> {
        let scope = self.scope(current)?;
        if let Some(&child) = scope
            .children
            .iter()
            .find(|&&c| self.scope(c).is_some_and(|s| s.name == segment))
        {
            return Some(child);
        }
        if let Some(resolved) = scope.resolved_imports.get(segment)
            && let ResolvedImportTarget::Namespace { scope_id } = &resolved.target
        {
            return Some(ScopeId(*scope_id));
        }
        None
    }

    /// Whether `path` (excluding its final segment) walks a chain of namespaces
    /// from `from_scope_id`. `["lib", "vals", "X"]` returns `true` when `lib::vals`
    /// is a reachable namespace, regardless of whether `X` exists in it.
    ///
    /// This lets a diagnostic distinguish a genuine namespace path with a bad
    /// final segment (`cannot resolve lib::vals::X`) from an `Enum::Variant`
    /// access, whose single qualifier never names a namespace.
    #[must_use = "this is a pure check with no side effects"]
    pub(crate) fn prefix_is_namespace(&self, path: &[String], from_scope_id: u32) -> bool {
        if path.len() < 2 {
            return false;
        }
        let prefix = &path[..path.len() - 1];
        // A struct/enum defined in the accessing file pre-empts a same-named
        // sibling file at the head: `foo::pick()` in a file with `struct foo` is
        // the struct's associated fn, not a sibling `foo.inf`. Without this veto a
        // sibling's private `use foo;` would silently rebind this file's own
        // `foo::` — the same precedence the type-path resolver applies, so the two
        // resolvers stay in agreement.
        //
        // A call's type-access shape is `Type::assoc()` — exactly one qualifier
        // ahead of the member — so the head can be that type only when the prefix
        // is a single segment. A longer prefix (`geom::sub::pick()`) cannot be a
        // type-access, so the head must not pre-empt and the namespace walk decides.
        if prefix.len() <= 1 && self.head_type_preempts_sibling_file(&prefix[0], from_scope_id) {
            return false;
        }
        let Some((mut current_scope, remaining)) =
            self.start_qualified_walk(prefix, from_scope_id)
        else {
            return false;
        };
        for segment in remaining {
            match self.next_namespace_scope(current_scope, segment) {
                Some((next, via)) => {
                    if via == ReachedVia::Child
                        && !self.may_descend_through_namespace(from_scope_id, current_scope, next)
                    {
                        return false;
                    }
                    current_scope = next;
                }
                None => return false,
            }
        }
        true
    }

    /// Resolves the longest leading run of `path` that names a chain of file
    /// namespaces from `from_scope_id`, returning the file scope it reaches and the
    /// number of segments consumed.
    ///
    /// `["geo", "Point", "new"]` (`type_access_len` 2) with a bound `use geo;`
    /// returns the `geo` file scope and a count of 1: `geo` is a namespace, `Point`
    /// is a struct within it, not a sub-namespace. The remaining segments
    /// (`Point::new`) are then resolved as a type member *inside* the returned file
    /// scope. Returns `None` when the first segment is not a namespace (so
    /// `Type::assoc()` / `Enum::Variant`, whose head is a type, fall through to the
    /// existing single-file handling).
    ///
    /// `type_access_len` is how many *trailing* segments the caller will read as a
    /// type-access (the type itself, plus any member), and is what disambiguates the
    /// type-vs-sub-file precedence: only the segment that *begins* the type-access
    /// portion (`path[path.len() - type_access_len]`) may stop the namespace walk on
    /// a same-named local type. A caller that pre-splits the leaf and passes a
    /// pure-namespace prefix (`["lib", "geom"]`) uses `0`, so no segment is treated
    /// as a type and the whole prefix is consumed; a type-annotation full path
    /// (`["lib", "geom", "Point"]`) uses `1`, so only the leaf `Point` may stop;
    /// an associated call (`["lib", "Point", "new"]`) uses `2`, so the struct
    /// `Point` may stop and the trailing `new` stays the member. This is what fixes
    /// `lib::geom::Point` where the parent file also defines a `struct geom` that
    /// collides with the *intermediate* segment: `geom` is not the type-access start
    /// (the leaf `Point` is), so it is consumed as the sub-file namespace.
    #[must_use = "this is a pure lookup with no side effects"]
    pub(crate) fn resolve_longest_namespace_prefix(
        &self,
        path: &[String],
        from_scope_id: u32,
        type_access_len: usize,
    ) -> Option<(u32, usize)> {
        if path.is_empty() {
            return None;
        }
        // The index at which the caller's type-access portion begins; a same-named
        // local type may pre-empt the namespace walk only at this one position.
        // Saturating so a pre-split prefix (`type_access_len` 0) yields an index past
        // the end — no segment is ever treated as the type.
        let type_access_start = path.len().saturating_sub(type_access_len);
        // A struct/enum defined in the accessing file pre-empts a same-named
        // sibling file at the head, so the path is left to single-file type
        // resolution (`Type::assoc()` / `Enum::Variant`). This must be decided
        // against the accessing scope — the head's meaning belongs to the file
        // that wrote the path, not the scope the walk happens to land in. The
        // per-segment loop below applies the *tail* precedence against the walked
        // scope (`lib::Point::new` where the walked-into `lib` defines `Point`),
        // which is a different decision.
        //
        // The pre-empt only applies when the head *is* the type-access start —
        // `foo::pick()` (head `foo` is the type) or `Pt` as a bare leaf. A head
        // ahead of the type-access portion (`lib::geom::Point` where the leaf
        // `Point` is the type, or `geom::sub::Point`) is a namespace, so the local
        // type must not stop it.
        if type_access_start == 0 && self.head_type_preempts_sibling_file(&path[0], from_scope_id) {
            return None;
        }
        // The first segment must name a file namespace (`use a::b;` binding or a
        // root child); a head that is a type or enum — `Type::assoc()` /
        // `Enum::Variant` — is left to single-file resolution. The walk through
        // `start_qualified_walk` anchors there and consumes it.
        let (start_scope, remaining) = self.start_qualified_walk(path, from_scope_id)?;
        let consumed_by_start = path.len() - remaining.len();
        if consumed_by_start == 0 {
            // An absolute path that did not anchor on a bound namespace: the first
            // segment must itself be a root child namespace, else this is not a
            // namespace path.
            self.next_namespace_scope(start_scope, &path[0])?;
        } else if !self.scope_is_file_namespace(start_scope.as_u32()) {
            return None;
        }

        let mut current_scope = start_scope;
        let mut consumed = consumed_by_start;
        // How the most recent hop reached `current_scope`; `None` for the anchor.
        // A terminal namespace reached by a re-export hop is exempt from the
        // surface gate applied after the loop.
        let mut last_via: Option<ReachedVia> = None;
        // Continue while subsequent segments still name sub-namespaces, stopping at
        // the first that does not (a type, variant, or the leaf member).
        //
        // A segment that names both a sub-file namespace and a type defined in the
        // current file is resolved as the type, not the sub-file: `lib::Point::new`
        // where `lib.inf` defines `struct Point` *and* a sibling `lib/Point.inf`
        // exists means the struct's associated `new`. Consuming `Point` as a
        // namespace would otherwise make the meaning depend on whether the sibling
        // file happens to be in the import closure, so a same-named type defined
        // here wins and the remaining `Point::member` is left to type resolution.
        //
        // The type only wins at the *type-access start* — the position the caller
        // declared as the beginning of its type-access (`type_access_start`). A
        // same-named type at an earlier, intermediate segment (`geom` in
        // `lib::geom::Point`, whose leaf `Point` is the actual type) cannot be a
        // type-member access — a struct has no member that is itself a type — so it
        // is consumed as the sub-file namespace. Tying the stop to the caller's
        // declared boundary, rather than a positional guess, keeps each consuming
        // context (pure prefix, type leaf, `Type::member`) resolving the shape it
        // actually reads.
        //
        // The tail type wins only when the accessing file imported the *parent*
        // file the walk currently stands in. `a::mid::make()` written with
        // `use a::mid;` (the sub-file, not its parent `a`) reaches `make` through
        // the sub-file `a/mid.inf`; that the walk lands in `a.inf` — which happens
        // to define a `struct mid` — must not flip the meaning to that struct's
        // associated `make`, because the accessing file never imported `a`. Without
        // this conjunct the flip would depend on whether a *third* file dragged
        // `a.inf` into the closure (its `use a;`), making the value non-
        // deterministic. The accessing file's own `use a::mid;` expresses intent
        // toward the sub-file; the struct may pre-empt only when the file actually
        // imported the parent (`use a;` / `use lib;`), exactly as the head
        // precedence requires for the head segment (#63).
        for (i, segment) in path.iter().enumerate().skip(consumed) {
            let scope_id = current_scope.as_u32();
            // A same-named type pre-empts the sub-file at the type-access start only
            // when the type interpretation is *viable* for the suffix (the type is
            // the leaf, or `Type::member` names a real assoc fn / enum variant).
            // Without the viability conjunct, an intermediate segment that merely
            // shares a name with a parent struct — `geom` in `lib::geom::mk()`,
            // where `mk` is a free fn in `lib/geom.inf`, not an assoc of
            // `struct geom` — would break here and be mis-resolved as an undefined
            // associated function instead of consuming `geom` as the sub-file (#63).
            if i == type_access_start
                && self.scope_defines_type(segment, scope_id)
                && self.file_imports_namespace_key(
                    from_scope_id,
                    &self.module_path_of_scope(scope_id),
                )
                && self.type_shadow_viable_for_suffix(path, i, scope_id, from_scope_id)
            {
                break;
            }
            match self.next_namespace_scope(current_scope, segment) {
                Some((next, via)) => {
                    if via == ReachedVia::Child
                        && !self.may_descend_through_namespace(from_scope_id, current_scope, next)
                    {
                        break;
                    }
                    current_scope = next;
                    consumed += 1;
                    last_via = Some(via);
                }
                None => break,
            }
        }
        if consumed == 0 {
            return None;
        }
        // The leaf type is read inside the namespace the walk landed in, so reading
        // it is a cross-file surface read subject to the terminal equality gate: a
        // file may name `lib::geom::Point` only by importing `lib::geom` exactly,
        // never a deeper (`lib::geom::sub`) or shallower import it walked across.
        // A terminal reached by a re-export hop is already licensed by that hop and
        // is exempt. This mirrors the value/function resolver's terminal gate so the
        // type path and the value path enforce the identical rule.
        if last_via != Some(ReachedVia::Reexport)
            && !self.may_read_namespace_surface(from_scope_id, current_scope)
        {
            return None;
        }
        Some((current_scope.as_u32(), consumed))
    }

    /// Whether `name` is a struct or enum defined in the file enclosing
    /// `scope_id` (its own definition, not one merely imported there). Used to let
    /// a type win over a same-named sub-file when resolving a qualified path: the
    /// import that pulled the sub-file into the closure must not change what
    /// `parent::Type::member` means.
    #[must_use = "this is a pure check with no side effects"]
    fn scope_defines_type(&self, name: &str, scope_id: u32) -> bool {
        self.lookup_symbol_file_scoped_from(name, scope_id)
            .is_some_and(|symbol| symbol.as_struct().is_some() || symbol.as_enum().is_some())
    }

    /// Whether interpreting `path[type_index]` as a type defined in `ns_scope` is
    /// *viable* for the rest of the path — the precondition for letting a same-named
    /// type pre-empt a sub-file at an intermediate segment of a qualified call.
    ///
    /// A type interpretation is viable only when the suffix is a shape that type
    /// resolution can actually consume:
    /// - the type is the **leaf** (`type_index == path.len() - 1`): `lib::Point`
    ///   in type position, trivially viable; or
    /// - the type is followed by **exactly one** trailing member that is a real
    ///   **associated function** (no `self`) of that struct, or a real variant of
    ///   that enum (`Type::assoc`, `Enum::Variant`).
    ///
    /// Any other suffix — a deeper path, a single member the type does not have, or
    /// an **instance** method (which needs a receiver and so is not a valid
    /// `Type::member()` associated-access) — is *not* a type-access, so the segment
    /// must be consumed as a sub-file namespace instead. Without this check, the
    /// type-shadow break fired on the mere existence of a same-named struct, turning
    /// `lib::geom::mk()` (where `lib` defines `struct geom` but `mk` is a free fn in
    /// the sub-file `lib/geom.inf`) into `lib::(struct geom)::mk` — an undefined or
    /// receiver-requiring member — instead of resolving the sub-file's free `mk`.
    /// Requiring an *associated* function (not an instance method) is what keeps
    /// `geom::mk(self)` from pre-empting the sub-file: an instance method could
    /// never satisfy `Type::mk()` anyway, so the break must not fire on it (#63).
    ///
    /// The viability probe reuses the *same* lookups the consuming handlers run
    /// ([`Self::resolve_method_in_namespace`], [`Self::resolve_enum_in_namespace`]),
    /// so the break and the handler can never disagree on whether a member exists.
    #[must_use = "this is a pure check with no side effects"]
    fn type_shadow_viable_for_suffix(
        &self,
        path: &[String],
        type_index: usize,
        ns_scope: u32,
        from_scope_id: u32,
    ) -> bool {
        let type_name = &path[type_index];
        if type_index == path.len() - 1 {
            return true;
        }
        if type_index + 1 != path.len() - 1 {
            return false;
        }
        let member = &path[type_index + 1];
        if self
            .resolve_method_in_namespace(type_name, member, ns_scope, from_scope_id)
            .is_some_and(|m| !m.is_instance_method())
        {
            return true;
        }
        self.resolve_enum_in_namespace(type_name, ns_scope, from_scope_id)
            .is_some_and(|(info, _)| info.variants.contains(member))
    }

    /// The single precedence decision shared by both qualified-path resolvers: a
    /// struct or enum defined in the *accessing* file pre-empts a same-named
    /// sibling file at the **head** of a `::` path.
    ///
    /// `foo::pick()` written in a file that defines `struct foo` means the local
    /// struct's associated `pick`, even when an unrelated sibling drags a
    /// root-child `foo.inf` into the import closure. The accessing file never
    /// imported `foo.inf`, so a sibling's private `use foo;` must not silently
    /// change what this file's own `foo::` means — that would make a value depend
    /// on code the file cannot see. The same holds for `Color::Red` (enum
    /// variant) and `Vec::new()` (associated fn).
    ///
    /// This is keyed on `from_scope_id` (the accessing scope), not the scope the
    /// walk lands in: the head's meaning belongs to the file that wrote the path.
    /// Both [`Self::prefix_is_namespace`] (the qualified-CALL gate) and
    /// [`Self::resolve_longest_namespace_prefix`] (the type-path resolver) consult
    /// this for the head, so the two can never disagree on it. Tail precedence —
    /// `lib::Point::new` where the walked-into file defines `Point` — is a
    /// distinct decision against the *walked* scope and stays in the resolver's
    /// own per-segment loop.
    ///
    /// A file that defines `struct foo` *and* writes `use foo;` is already a hard
    /// import-collision error, so this veto only ever fires on a sibling this file
    /// never imported — exactly when the local type should win.
    #[must_use = "this is a pure check with no side effects"]
    fn head_type_preempts_sibling_file(&self, head: &str, from_scope_id: u32) -> bool {
        self.scope_defines_type(head, from_scope_id)
    }

    /// Whether `scope_id` is a file namespace — the root (entry file) or a
    /// non-entry file scope registered in `mod_scopes`.
    #[must_use = "this is a pure check with no side effects"]
    fn scope_is_file_namespace(&self, scope_id: u32) -> bool {
        self.root_scope_id() == Some(scope_id) || self.is_non_entry_file_scope(scope_id)
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

        let full_path = self.scopes[ScopeId(scope_id).index()].full_path.clone();
        self.mod_scopes.insert(full_path, ScopeId(scope_id));

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
                let (param_types, param_names): (Vec<TypeInfo>, Vec<Option<String>>) = args
                    .iter()
                    .filter_map(|a| match &a.kind {
                        ArgKind::Named {
                            ty,
                            name: param_name,
                            ..
                        } => Some((
                            TypeInfo::from_type_id_with_type_params(arena, *ty, &tp_names),
                            Some(arena[*param_name].name.clone()),
                        )),
                        ArgKind::Ignored { ty } => Some((
                            TypeInfo::from_type_id_with_type_params(arena, *ty, &tp_names),
                            None,
                        )),
                        ArgKind::TypeOnly(ty) => Some((
                            TypeInfo::from_type_id_with_type_params(arena, *ty, &tp_names),
                            None,
                        )),
                        ArgKind::SelfRef { .. } => None,
                    })
                    .unzip();
                let return_type = returns
                    .map(|r| TypeInfo::from_type_id_with_type_params(arena, r, &tp_names))
                    .unwrap_or_default();

                self.register_function_with_visibility(
                    &arena[*name].name,
                    tp_names,
                    param_types,
                    param_names,
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
                let (param_types, param_names): (Vec<TypeInfo>, Vec<Option<String>>) = args
                    .iter()
                    .filter_map(|a| match &a.kind {
                        ArgKind::SelfRef { .. } => None,
                        ArgKind::Named {
                            ty,
                            name: param_name,
                            ..
                        } => Some((
                            TypeInfo::from_type_id(arena, *ty),
                            Some(arena[*param_name].name.clone()),
                        )),
                        ArgKind::Ignored { ty } | ArgKind::TypeOnly(ty) => {
                            Some((TypeInfo::from_type_id(arena, *ty), None))
                        }
                    })
                    .unzip();
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
                    param_names,
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

        /// Builds a public type-alias symbol wrapping `type_info`.
        fn public_alias(type_info: TypeInfo) -> Symbol {
            Symbol::TypeAlias(TypeAliasInfo {
                type_info,
                visibility: Visibility::Public,
                definition_location: Location::default(),
            })
        }

        #[test]
        fn name_returns_type_info_string_representation() {
            let type_info = TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            };
            let symbol = public_alias(type_info);
            let name = symbol.name();
            assert_eq!(name, "i32");
        }

        #[test]
        fn as_type_info_returns_clone_of_wrapped_type() {
            let type_info = TypeInfo {
                kind: TypeInfoKind::Number(NumberType::U64),
                type_params: vec![],
            };
            let symbol = public_alias(type_info);
            let result = symbol.as_type_info(&SymbolTable::default());
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
            let symbol = public_alias(type_info);
            let result = symbol.as_type_info(&SymbolTable::default());
            assert!(result.is_some());
            let result_type = result.unwrap();
            assert!(matches!(result_type.kind, TypeInfoKind::Custom(ref s) if s == "MyType"));
        }

        #[test]
        fn is_public_follows_alias_visibility() {
            let type_info = TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            };
            assert!(public_alias(type_info.clone()).is_public());
            let private = Symbol::TypeAlias(TypeAliasInfo {
                type_info,
                visibility: Visibility::Private,
                definition_location: Location::default(),
            });
            assert!(!private.is_public());
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
            let symbol = public_alias(type_info);
            assert!(symbol.as_function().is_none());
        }

        #[test]
        fn as_struct_returns_none_for_type_alias() {
            let type_info = TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            };
            let symbol = public_alias(type_info);
            assert!(symbol.as_struct().is_none());
        }

        #[test]
        fn as_enum_returns_none_for_type_alias() {
            let type_info = TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            };
            let symbol = public_alias(type_info);
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
                scope1.name, scope2.name,
                "Consecutive anonymous scopes should have unique names"
            );
        }

        #[test]
        fn name_includes_scope_id() {
            let mut table = SymbolTable::default();
            let scope_id = table.push_scope();
            let scope = table.get_scope(scope_id).unwrap();
            assert!(
                scope.name.starts_with("anonymous_"),
                "Anonymous scope name should start with 'anonymous_'"
            );
            let expected_name = format!("anonymous_{scope_id}");
            assert_eq!(
                scope.name, expected_name,
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
                inner1.full_path, inner2.full_path,
                "Nested anonymous scopes should have different full_paths"
            );
            assert!(
                inner1.full_path.contains("test_func"),
                "Full path should include parent function name"
            );
        }

        #[test]
        fn anonymous_scopes_not_in_mod_scopes() {
            let mut table = SymbolTable::default();
            let scope_id = table.push_scope();
            let scope = table.get_scope(scope_id).unwrap();
            let full_path = scope.full_path.clone();
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
                let expected_depth = i + 1;
                let path_parts: Vec<&str> = scope.full_path.split("::").collect();
                assert_eq!(
                    path_parts.len(),
                    expected_depth,
                    "Deeply nested scope at level {i} should have correct path depth"
                );
                assert!(
                    scope.name.starts_with("anonymous_"),
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
                sibling1.name.clone(),
                sibling2.name.clone(),
                sibling3.name.clone(),
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
            let child_parent = child_scope.parent;
            assert!(
                child_parent.is_some(),
                "Anonymous child scope should have parent"
            );
            let child_parent_id = child_parent.unwrap().as_u32();
            assert_eq!(
                child_parent_id, parent_id,
                "Anonymous scope's parent should be the enclosing scope"
            );
            let parent_children = &parent_scope.children;
            assert_eq!(
                parent_children.len(),
                1,
                "Parent should have the anonymous child in its children list"
            );
            assert_eq!(
                parent_children[0].as_u32(),
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
                matches!(scope.visibility, Visibility::Private),
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
            let full_path = anon_scope.full_path.clone();
            let name = anon_scope.name.clone();
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
            let full_path = scope.full_path.clone();
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

    mod scope_arena_invariants {
        use super::*;

        /// Every allocated scope's stored id equals the id it is retrieved by.
        /// This is the index-equals-id invariant the arena's O(1) lookup relies
        /// on: `ScopeId(id).index()` is only a valid storage index because ids are
        /// dense and allocation-ordered. A mix of file, anonymous, and popped
        /// scopes must not perturb it.
        #[test]
        fn stored_id_equals_retrieval_id() {
            let mut table = SymbolTable::default();
            table.push_scope_with_name("a", Visibility::Public);
            table.push_scope();
            table.pop_scope();
            table.push_scope_with_name("b", Visibility::Public);
            table.enter_spec("Sp");

            let ids = table.all_scope_ids();
            assert_eq!(
                ids,
                (0..ids.len() as u32).collect::<Vec<_>>(),
                "scope ids must be the dense range 0..n"
            );
            for id in ids {
                let scope = table.get_scope(id).expect("every allocated id resolves");
                assert_eq!(
                    scope.id.as_u32(),
                    id,
                    "a scope's stored id must equal the id it is fetched by"
                );
            }
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
                    param_names: vec![],
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
                    param_names: vec![],
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
                param_names: vec![],
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
                param_names: vec![],
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
                    param_names: vec![],
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
                    param_names: vec![],
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

        fn i64_type() -> TypeInfo {
            TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I64),
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
                    vec![None],
                    i32_type(),
                    Some(origin("collections", "sort")),
                )
                .expect("registering a bound extern should succeed");

            let info = table
                .lookup_function("sort")
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
                .register_extern_function("add", vec![i32_type()], vec![None], i32_type(), None)
                .expect("registering an unbound extern should succeed");

            let info = table
                .lookup_function("add")
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
                .lookup_function("helper")
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
                    vec![None],
                    i32_type(),
                    Some(origin("collections", "sort")),
                )
                .unwrap();
            table
                .register_extern_function("unbound", vec![], vec![], i32_type(), None)
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
        fn extern_origins_dedups_one_declaration_reached_from_two_scopes() {
            // One declaration registered in two scopes is one binding; the driver
            // should resolve and validate that `.wasm` once.
            let mut table = SymbolTable::default();
            table
                .register_extern_function(
                    "sort",
                    vec![i32_type()],
                    vec![None],
                    i32_type(),
                    Some(origin("collections", "sort")),
                )
                .unwrap();
            let _ = table.push_scope_with_name("nested", Visibility::Public);
            table
                .register_extern_function(
                    "sort",
                    vec![i32_type()],
                    vec![None],
                    i32_type(),
                    Some(origin("collections", "sort")),
                )
                .unwrap();

            let origins = table.extern_origins();
            assert_eq!(
                origins.len(),
                1,
                "one declaration reached twice is one origin, got {origins:?}"
            );
        }

        #[test]
        fn extern_origins_keeps_two_declarations_of_one_module_field() {
            // Two files may each declare and bind `collections::sort`. The driver
            // validates the resolved library against the signature *each*
            // declaration states, and the linker satisfies the import on
            // `(module, field)` alone with no signature comparison of its own —
            // so a dropped declaration is a signature that is never checked and a
            // mismatch that ships as a mis-linked artifact.
            let mut table = SymbolTable::default();
            let mut first = origin("collections", "sort");
            first.decl = inference_ast::ids::idx_from_u32(1);
            let mut second = origin("collections", "sort");
            second.decl = inference_ast::ids::idx_from_u32(2);
            table
                .register_extern_function(
                    "sort",
                    vec![i32_type()],
                    vec![None],
                    i32_type(),
                    Some(first),
                )
                .unwrap();
            let _ = table.push_scope_with_name("other_file", Visibility::Public);
            table
                .register_extern_function(
                    "sort",
                    vec![i64_type()],
                    vec![None],
                    i64_type(),
                    Some(second),
                )
                .unwrap();

            let origins = table.extern_origins();
            assert_eq!(
                origins.len(),
                2,
                "both declarations must reach validation, got {origins:?}"
            );
            assert_eq!(
                origins.iter().map(|o| o.decl).collect::<Vec<_>>(),
                vec![
                    inference_ast::ids::idx_from_u32(1),
                    inference_ast::ids::idx_from_u32(2)
                ],
                "the enumeration order is deterministic, got {origins:?}"
            );
        }
    }
}