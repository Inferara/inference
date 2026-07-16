//! Type Information and Representation
//!
//! This module defines the type representation system used throughout the type checker
//! for semantic analysis, type inference, and type checking.
//!
//! ## Type Categories
//!
//! The Inference language type system includes:
//!
//! **Primitive Types**:
//! - `unit` - The unit type (similar to void)
//! - `bool` - Boolean type with values `true` and `false`
//! - `string` - UTF-8 encoded strings (partial support)
//! - Numeric types: `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`
//!
//! **Compound Types**:
//! - Arrays: `[T; N]` with element type `T` and fixed size `N`
//! - Structs: User-defined types with named fields
//! - Enums: User-defined types with named variants (unit variants only currently)
//! - Functions: Function types with parameter and return types
//!
//! **Generic Types**:
//! - Type parameters: Unbound type variables that can be substituted
//! - Generic arrays: `[T; N]` where `T` is a type parameter
//! - Generic functions: Functions with type parameters

use core::fmt;
use std::fmt::{Display, Formatter};

use inference_ast::arena::AstArena;
use inference_ast::ids::TypeId;
use inference_ast::nodes::{Expr, SimpleTypeKind, TypeNode};
use rustc_hash::FxHashMap;

#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash)]
pub enum NumberType {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
}

impl NumberType {
    /// All numeric type variants for iteration.
    pub const ALL: &'static [NumberType] = &[
        NumberType::I8,
        NumberType::I16,
        NumberType::I32,
        NumberType::I64,
        NumberType::U8,
        NumberType::U16,
        NumberType::U32,
        NumberType::U64,
    ];

    /// Returns the canonical lowercase string representation of this numeric type.
    #[must_use = "returns the string representation without modifying self"]
    pub const fn as_str(&self) -> &'static str {
        match self {
            NumberType::I8 => "i8",
            NumberType::I16 => "i16",
            NumberType::I32 => "i32",
            NumberType::I64 => "i64",
            NumberType::U8 => "u8",
            NumberType::U16 => "u16",
            NumberType::U32 => "u32",
            NumberType::U64 => "u64",
        }
    }

    #[must_use = "this is a pure check with no side effects"]
    pub const fn is_signed(&self) -> bool {
        matches!(
            self,
            NumberType::I8 | NumberType::I16 | NumberType::I32 | NumberType::I64
        )
    }
}

impl std::str::FromStr for NumberType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .find(|nt| nt.as_str() == s)
            .copied()
            .ok_or(())
    }
}

/// Discriminates the semantic category of a [`TypeInfo`] value.
///
/// ## Struct and enum identity
///
/// The [`Struct`](Self::Struct) and [`Enum`](Self::Enum) variants carry two
/// strings: the bare type name and the type's *canonical key*. The bare name is
/// what name resolution and code generation read (codegen re-qualifies it by the
/// referencing file's module path); the canonical key is the type's defining-file
/// identity (`a::alpha::Inner` for a non-entry file, the bare name for the entry
/// file). Type identity — `PartialEq`, `Eq`, and `Hash` — is the canonical key
/// alone, so two same-named structs from different files are distinct types while
/// a single-file program's bare name *is* its key and behaves exactly as before.
/// This is what stops a value of one file's `Inner` being accepted where another
/// file's same-named `Inner` is expected.
#[derive(Debug, Clone)]
pub enum TypeInfoKind {
    Unit,
    Bool,
    String,
    Number(NumberType),
    Custom(String),
    Array(Box<TypeInfo>, u32),
    Generic(String),
    QualifiedName(String),
    Qualified(String),
    Function(String),
    /// A struct type: `(bare_name, canonical_key)`. Identity is the canonical key.
    Struct(String, String),
    /// An enum type: `(bare_name, canonical_key)`. Identity is the canonical key.
    Enum(String, String),
    Spec(String),
}

/// Equality and hashing key the [`Struct`](TypeInfoKind::Struct) and
/// [`Enum`](TypeInfoKind::Enum) variants on their canonical key alone (the second
/// field), so a same-named type from a different file is a distinct type. Every
/// other variant compares and hashes exactly as a derive would. For a single-file
/// program the canonical key equals the bare name, so this is byte-identical to
/// the previous derived behavior.
impl PartialEq for TypeInfoKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (TypeInfoKind::Unit, TypeInfoKind::Unit)
            | (TypeInfoKind::Bool, TypeInfoKind::Bool)
            | (TypeInfoKind::String, TypeInfoKind::String) => true,
            (TypeInfoKind::Number(a), TypeInfoKind::Number(b)) => a == b,
            (TypeInfoKind::Custom(a), TypeInfoKind::Custom(b))
            | (TypeInfoKind::Generic(a), TypeInfoKind::Generic(b))
            | (TypeInfoKind::QualifiedName(a), TypeInfoKind::QualifiedName(b))
            | (TypeInfoKind::Qualified(a), TypeInfoKind::Qualified(b))
            | (TypeInfoKind::Function(a), TypeInfoKind::Function(b))
            | (TypeInfoKind::Spec(a), TypeInfoKind::Spec(b)) => a == b,
            (TypeInfoKind::Array(a, an), TypeInfoKind::Array(b, bn)) => a == b && an == bn,
            (TypeInfoKind::Struct(_, a_key), TypeInfoKind::Struct(_, b_key))
            | (TypeInfoKind::Enum(_, a_key), TypeInfoKind::Enum(_, b_key)) => a_key == b_key,
            _ => false,
        }
    }
}

impl Eq for TypeInfoKind {}

impl std::hash::Hash for TypeInfoKind {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            TypeInfoKind::Unit | TypeInfoKind::Bool | TypeInfoKind::String => {}
            TypeInfoKind::Number(n) => n.hash(state),
            TypeInfoKind::Custom(s)
            | TypeInfoKind::Generic(s)
            | TypeInfoKind::QualifiedName(s)
            | TypeInfoKind::Qualified(s)
            | TypeInfoKind::Function(s)
            | TypeInfoKind::Spec(s) => s.hash(state),
            TypeInfoKind::Array(elem, len) => {
                elem.hash(state);
                len.hash(state);
            }
            TypeInfoKind::Struct(_, key) | TypeInfoKind::Enum(_, key) => key.hash(state),
        }
    }
}

impl Display for TypeInfoKind {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            TypeInfoKind::Unit => write!(f, "Unit"),
            TypeInfoKind::Bool => write!(f, "Bool"),
            TypeInfoKind::String => write!(f, "String"),
            TypeInfoKind::Number(number_type) => write!(f, "{}", number_type.as_str()),
            TypeInfoKind::Array(ty, length) => write!(f, "[{ty}; {length}]"),
            // Render structs and enums by their canonical key so a cross-file type
            // mismatch reads `a::alpha::Inner` vs `b::beta::Inner` rather than a
            // confusing `Inner` vs `Inner`. The key equals the bare name in a
            // single-file program, so single-file diagnostics are unchanged.
            TypeInfoKind::Struct(_, key) | TypeInfoKind::Enum(_, key) => write!(f, "{key}"),
            TypeInfoKind::Custom(ty)
            | TypeInfoKind::Spec(ty)
            | TypeInfoKind::QualifiedName(ty)
            | TypeInfoKind::Qualified(ty)
            | TypeInfoKind::Function(ty) => write!(f, "{ty}"),
            TypeInfoKind::Generic(ty) => write!(f, "{ty}'"),
        }
    }
}

impl TypeInfoKind {
    pub const NON_NUMERIC_BUILTINS: &'static [(&'static str, TypeInfoKind)] = &[
        ("unit", TypeInfoKind::Unit),
        ("bool", TypeInfoKind::Bool),
        ("string", TypeInfoKind::String),
        ("String", TypeInfoKind::String),
    ];

    #[must_use = "this is a pure check with no side effects"]
    pub fn is_number(&self) -> bool {
        matches!(self, TypeInfoKind::Number(_))
    }

    #[must_use = "returns the builtin name without modifying self"]
    pub fn as_builtin_str(&self) -> Option<&'static str> {
        match self {
            TypeInfoKind::Unit => Some("unit"),
            TypeInfoKind::Bool => Some("bool"),
            TypeInfoKind::String => Some("string"),
            TypeInfoKind::Number(nt) => Some(nt.as_str()),
            _ => None,
        }
    }

    #[must_use = "parsing result should be checked; returns None if not a builtin"]
    pub fn from_builtin_str(s: &str) -> Option<Self> {
        if let Ok(number_type) = s.parse::<NumberType>() {
            return Some(TypeInfoKind::Number(number_type));
        }
        Self::NON_NUMERIC_BUILTINS
            .iter()
            .find(|(name, _)| *name == s)
            .map(|(_, kind)| kind.clone())
    }
}

/// The semantic type of a value expression after type checking.
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct TypeInfo {
    pub kind: TypeInfoKind,
    pub type_params: Vec<String>,
}

impl Default for TypeInfo {
    fn default() -> Self {
        Self {
            kind: TypeInfoKind::Unit,
            type_params: vec![],
        }
    }
}

impl Display for TypeInfo {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if self.type_params.is_empty() {
            return write!(f, "{}", self.kind);
        }
        let type_params = self
            .type_params
            .iter()
            .map(|tp| format!("{tp}'"))
            .collect::<Vec<_>>()
            .join(" ");
        write!(f, "{} {}", self.kind, type_params)
    }
}

impl TypeInfo {
    #[must_use]
    pub fn boolean() -> Self {
        Self {
            kind: TypeInfoKind::Bool,
            type_params: vec![],
        }
    }

    #[must_use]
    pub fn string() -> Self {
        Self {
            kind: TypeInfoKind::String,
            type_params: vec![],
        }
    }

    /// Convert an arena-allocated type node to its semantic `TypeInfo` representation.
    #[must_use]
    pub fn from_type_id(arena: &AstArena, ty_id: TypeId) -> Self {
        Self::from_type_id_with_type_params(arena, ty_id, &[])
    }

    /// Create `TypeInfo` from an arena type ID, with awareness of type parameters.
    #[must_use]
    pub fn from_type_id_with_type_params(
        arena: &AstArena,
        ty_id: TypeId,
        type_param_names: &[String],
    ) -> Self {
        let type_data = &arena[ty_id];
        match &type_data.kind {
            TypeNode::Simple(simple) => Self {
                kind: Self::type_kind_from_simple_type_kind(simple),
                type_params: vec![],
            },
            TypeNode::Generic { base, params } => Self {
                kind: TypeInfoKind::Generic(arena[*base].name.clone()),
                type_params: params.iter().map(|p| arena[*p].name.clone()).collect(),
            },
            TypeNode::QualifiedName { qualifier, name } => Self {
                kind: TypeInfoKind::QualifiedName(format!(
                    "{}::{}",
                    arena[*qualifier].name,
                    arena[*name].name
                )),
                type_params: vec![],
            },
            TypeNode::Qualified { .. } => {
                // Carry the full `::`-joined path (`lib::geom::Point`), not just
                // the leaf: the qualifier names the file-namespace chain the type
                // is reached through, and resolution needs every segment to walk
                // it. The string is a pre-resolution carrier; `resolve_custom_type`
                // rewrites it to a canonical `Struct`/`Enum` once the path is bound.
                Self {
                    kind: TypeInfoKind::Qualified(
                        type_data.kind.qualified_path(arena).unwrap_or_default(),
                    ),
                    type_params: vec![],
                }
            }
            TypeNode::Array { element, size } => {
                let array_size = extract_array_size_from_arena(arena, *size);
                Self {
                    kind: TypeInfoKind::Array(
                        Box::new(Self::from_type_id_with_type_params(
                            arena,
                            *element,
                            type_param_names,
                        )),
                        array_size,
                    ),
                    type_params: vec![],
                }
            }
            TypeNode::Function { params, ret } => {
                let param_types: Vec<TypeInfo> = params
                    .iter()
                    .map(|p| TypeInfo::from_type_id_with_type_params(arena, *p, type_param_names))
                    .collect();
                let return_type = ret
                    .map(|r| TypeInfo::from_type_id_with_type_params(arena, r, type_param_names))
                    .unwrap_or_default();
                let params = param_types
                    .iter()
                    .map(source_like_spelling)
                    .collect::<Vec<_>>()
                    .join(", ");
                Self {
                    kind: TypeInfoKind::Function(format!(
                        "fn({params}) -> {}",
                        source_like_spelling(&return_type)
                    )),
                    type_params: vec![],
                }
            }
            TypeNode::Custom(ident_id) => {
                let name = &arena[*ident_id].name;
                if type_param_names.contains(name) {
                    return Self {
                        kind: TypeInfoKind::Generic(name.clone()),
                        type_params: vec![],
                    };
                }
                Self {
                    kind: Self::type_kind_from_simple_type(name),
                    type_params: vec![],
                }
            }
        }
    }

    #[must_use]
    pub fn is_number(&self) -> bool {
        self.kind.is_number()
    }

    #[must_use]
    pub fn is_array(&self) -> bool {
        matches!(self.kind, TypeInfoKind::Array(_, _))
    }

    #[must_use]
    pub fn is_bool(&self) -> bool {
        matches!(self.kind, TypeInfoKind::Bool)
    }

    #[must_use]
    pub fn is_struct(&self) -> bool {
        matches!(self.kind, TypeInfoKind::Struct(_, _))
    }

    #[must_use]
    pub fn is_generic(&self) -> bool {
        matches!(self.kind, TypeInfoKind::Generic(_))
    }

    #[must_use = "this is a pure check with no side effects"]
    pub fn is_signed_integer(&self) -> bool {
        if let TypeInfoKind::Number(nt) = &self.kind {
            nt.is_signed()
        } else {
            false
        }
    }

    #[must_use = "substitution returns a new TypeInfo, original is unchanged"]
    pub fn substitute(&self, substitutions: &FxHashMap<String, TypeInfo>) -> TypeInfo {
        match &self.kind {
            TypeInfoKind::Generic(name) => {
                if let Some(concrete) = substitutions.get(name) {
                    concrete.clone()
                } else {
                    self.clone()
                }
            }
            TypeInfoKind::Array(elem_type, length) => {
                let substituted_elem = elem_type.substitute(substitutions);
                TypeInfo {
                    kind: TypeInfoKind::Array(Box::new(substituted_elem), *length),
                    type_params: vec![],
                }
            }
            TypeInfoKind::Unit
            | TypeInfoKind::Bool
            | TypeInfoKind::String
            | TypeInfoKind::Number(_)
            | TypeInfoKind::Custom(_)
            | TypeInfoKind::QualifiedName(_)
            | TypeInfoKind::Qualified(_)
            | TypeInfoKind::Function(_)
            | TypeInfoKind::Struct(_, _)
            | TypeInfoKind::Enum(_, _)
            | TypeInfoKind::Spec(_) => self.clone(),
        }
    }

    #[must_use = "this is a pure check with no side effects"]
    pub fn has_unresolved_params(&self) -> bool {
        match &self.kind {
            TypeInfoKind::Generic(_) => true,
            TypeInfoKind::Array(elem_type, _) => elem_type.has_unresolved_params(),
            TypeInfoKind::Unit
            | TypeInfoKind::Bool
            | TypeInfoKind::String
            | TypeInfoKind::Number(_)
            | TypeInfoKind::Custom(_)
            | TypeInfoKind::QualifiedName(_)
            | TypeInfoKind::Qualified(_)
            | TypeInfoKind::Function(_)
            | TypeInfoKind::Struct(_, _)
            | TypeInfoKind::Enum(_, _)
            | TypeInfoKind::Spec(_) => false,
        }
    }

    fn type_kind_from_simple_type(simple_type_name: &str) -> TypeInfoKind {
        TypeInfoKind::from_builtin_str(simple_type_name)
            .unwrap_or_else(|| TypeInfoKind::Custom(simple_type_name.to_string()))
    }

    fn type_kind_from_simple_type_kind(kind: &SimpleTypeKind) -> TypeInfoKind {
        match kind {
            SimpleTypeKind::Unit => TypeInfoKind::Unit,
            SimpleTypeKind::Bool => TypeInfoKind::Bool,
            SimpleTypeKind::I8 => TypeInfoKind::Number(NumberType::I8),
            SimpleTypeKind::I16 => TypeInfoKind::Number(NumberType::I16),
            SimpleTypeKind::I32 => TypeInfoKind::Number(NumberType::I32),
            SimpleTypeKind::I64 => TypeInfoKind::Number(NumberType::I64),
            SimpleTypeKind::U8 => TypeInfoKind::Number(NumberType::U8),
            SimpleTypeKind::U16 => TypeInfoKind::Number(NumberType::U16),
            SimpleTypeKind::U32 => TypeInfoKind::Number(NumberType::U32),
            SimpleTypeKind::U64 => TypeInfoKind::Number(NumberType::U64),
        }
    }
}

/// A source-like spelling of `ty` for embedding in a function-type carrier
/// (`fn(i32, bool) -> i32`). Built-in scalars use their lowercase source names,
/// so the carrier reads as it was written rather than as the checker's
/// capitalized [`Display`] (`Bool`/`Unit`/`String`); every other kind uses its
/// `Display`, which already reads as source (a struct/enum by its canonical key,
/// a generic primed, a nested function type by this same spelling).
fn source_like_spelling(ty: &TypeInfo) -> String {
    match ty.kind.as_builtin_str() {
        Some(builtin) => builtin.to_string(),
        None => ty.to_string(),
    }
}

/// Extracts the array size from an expression stored in the arena.
fn extract_array_size_from_arena(arena: &AstArena, size_expr_id: inference_ast::ids::ExprId) -> u32 {
    let expr_data = &arena[size_expr_id];
    if let Expr::NumberLiteral { value } = &expr_data.kind {
        return value.parse::<u32>().unwrap_or(0);
    }
    if let Expr::Identifier(ident_id) = &expr_data.kind {
        todo!(
            "Constant identifiers for array sizes not yet implemented: {}",
            arena[*ident_id].name
        );
    }
    0
}
