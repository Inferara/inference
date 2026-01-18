//! Type Information
//!
//! This module defines the representation of types used throughout the type checker.
//!
//! The Inference language supports:
//! - Primitive types: bool, unit, i8-i64, u8-u64
//! - Compound types: arrays, structs, enums, functions
//! - Generic types: type parameters that can be substituted
//!
//! Generic types use [`TypeInfoKind::Generic`] for unbound type parameters.
//! The [`TypeInfo::substitute`] method replaces type parameters with concrete types.
//!
//! ## Type Conversion from AST
//!
//! Primitive builtin types in the AST use `Type::Simple(SimpleTypeKind)`, a
//! lightweight enum without heap allocation. The `TypeInfo::new()` method converts
//! these to `TypeInfoKind` variants through direct pattern matching for efficient
//! type checking.

use core::fmt;
use std::{
    fmt::{Display, Formatter},
    panic,
};

use inference_ast::nodes::{Expression, Literal, SimpleTypeKind, Type};
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
    ///
    /// Use this constant to enumerate all supported numeric types without
    /// hardcoding the list in multiple places.
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
    ///
    /// This is the source-code representation (e.g., "i32", "u64").
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

    /// Parses a string into a `NumberType` (case-insensitive).
    ///
    /// Returns `Ok(NumberType)` if the string matches a known numeric type,
    /// or `Err(())` if no match is found.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .find(|nt| nt.as_str().eq_ignore_ascii_case(s))
            .copied()
            .ok_or(())
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
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
    Struct(String),
    Enum(String),
    Spec(String),
}

impl Display for TypeInfoKind {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            TypeInfoKind::Unit => write!(f, "Unit"),
            TypeInfoKind::Bool => write!(f, "Bool"),
            TypeInfoKind::String => write!(f, "String"),
            TypeInfoKind::Number(number_type) => write!(f, "{}", number_type.as_str()),
            TypeInfoKind::Array(ty, length) => write!(f, "[{ty}; {length}]"),
            TypeInfoKind::Custom(ty)
            | TypeInfoKind::Spec(ty)
            | TypeInfoKind::Struct(ty)
            | TypeInfoKind::Enum(ty)
            | TypeInfoKind::QualifiedName(ty)
            | TypeInfoKind::Qualified(ty)
            | TypeInfoKind::Function(ty) => write!(f, "{ty}"),
            TypeInfoKind::Generic(ty) => write!(f, "{ty}'"),
        }
    }
}

impl TypeInfoKind {
    /// Non-numeric primitive builtin type names (case-insensitive lookup).
    ///
    /// This constant provides the canonical mapping from source-code type names
    /// to their corresponding `TypeInfoKind` variants for unit, bool, and string.
    /// Use this to enumerate non-numeric builtins without hardcoding the list.
    pub const NON_NUMERIC_BUILTINS: &'static [(&'static str, TypeInfoKind)] = &[
        ("unit", TypeInfoKind::Unit),
        ("bool", TypeInfoKind::Bool),
        ("string", TypeInfoKind::String),
    ];

    #[must_use = "this is a pure check with no side effects"]
    pub fn is_number(&self) -> bool {
        matches!(self, TypeInfoKind::Number(_))
    }

    /// Returns the canonical lowercase source-code name if this is a primitive builtin type.
    ///
    /// Returns `Some("i32")` for `Number(I32)`, `Some("bool")` for `Bool`, etc.
    /// Returns `None` for compound types like `Array`, `Custom`, `Struct`, etc.
    ///
    /// Note: The `Display` impl outputs capitalized names ("Bool", "String") for
    /// non-numeric builtins, while this method returns lowercase source-code names.
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

    /// Parses a string into a primitive builtin `TypeInfoKind` (case-insensitive).
    ///
    /// Accepts type names like "i32", "I32", "bool", "BOOL", "string", "unit", etc.
    /// Returns `None` if the string does not match any builtin type.
    #[must_use = "parsing result should be checked; returns None if not a builtin"]
    pub fn from_builtin_str(s: &str) -> Option<Self> {
        if let Ok(number_type) = s.parse::<NumberType>() {
            return Some(TypeInfoKind::Number(number_type));
        }
        Self::NON_NUMERIC_BUILTINS
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(s))
            .map(|(_, kind)| kind.clone())
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct TypeInfo {
    pub kind: TypeInfoKind,
    pub type_params: Vec<String>,
    // (Field type information could be added here if needed for struct field checking.)
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

    #[must_use]
    pub fn new(ty: &Type) -> Self {
        Self::new_with_type_params(ty, &[])
    }

    /// Create TypeInfo from an AST Type, with awareness of type parameters.
    ///
    /// When `type_param_names` contains "T" and we see type "T", it becomes
    /// `TypeInfoKind::Generic("T")` instead of `TypeInfoKind::Custom("T")`.
    #[must_use]
    pub fn new_with_type_params(ty: &Type, type_param_names: &[String]) -> Self {
        match ty {
            Type::Simple(simple) => Self {
                kind: Self::type_kind_from_simple_type_kind(simple),
                type_params: vec![],
            },
            Type::Generic(generic) => Self {
                kind: TypeInfoKind::Generic(generic.base.name.clone()),
                type_params: generic.parameters.iter().map(|p| p.name.clone()).collect(),
            },
            Type::QualifiedName(qualified_name) => Self {
                kind: TypeInfoKind::QualifiedName(format!(
                    "{}::{}",
                    qualified_name.qualifier(),
                    qualified_name.name()
                )),
                type_params: vec![],
            },
            Type::Qualified(qualified) => Self {
                kind: TypeInfoKind::Qualified(qualified.name.name.clone()),
                type_params: vec![],
            },
            Type::Array(array) => {
                let size = extract_array_size(array.size.clone());
                Self {
                    kind: TypeInfoKind::Array(
                        Box::new(Self::new_with_type_params(
                            &array.element_type,
                            type_param_names,
                        )),
                        size,
                    ),
                    type_params: vec![],
                }
            }
            Type::Function(func) => {
                let param_types = func
                    .parameters
                    .as_ref()
                    .map(|params| {
                        params
                            .iter()
                            .map(|p| TypeInfo::new_with_type_params(p, type_param_names))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let return_type = func
                    .returns
                    .as_ref()
                    .map(|r| TypeInfo::new_with_type_params(r, type_param_names))
                    .unwrap_or_default();
                Self {
                    kind: TypeInfoKind::Function(format!(
                        "Function<{}, {}>",
                        param_types.len(),
                        return_type.kind
                    )),
                    type_params: vec![],
                }
            }
            Type::Custom(custom) => {
                // Check if this is a declared type parameter
                if type_param_names.contains(&custom.name) {
                    return Self {
                        kind: TypeInfoKind::Generic(custom.name.clone()),
                        type_params: vec![],
                    };
                }
                Self {
                    kind: Self::type_kind_from_simple_type(&custom.name),
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
        matches!(self.kind, TypeInfoKind::Struct(_))
    }

    #[must_use]
    pub fn is_generic(&self) -> bool {
        matches!(self.kind, TypeInfoKind::Generic(_))
    }

    /// Returns true if this is a signed integer type (i8, i16, i32, i64).
    #[must_use = "this is a pure check with no side effects"]
    pub fn is_signed_integer(&self) -> bool {
        if let TypeInfoKind::Number(nt) = &self.kind {
            nt.is_signed()
        } else {
            false
        }
    }

    /// Substitute type parameters using the given mapping.
    ///
    /// If this TypeInfo is a `Generic("T")` and substitutions has `T -> i32`, returns i32.
    /// For compound types (arrays, functions), recursively substitutes.
    /// After successful substitution, `type_params` should be empty.
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
            // Primitive and named types don't need substitution
            TypeInfoKind::Unit
            | TypeInfoKind::Bool
            | TypeInfoKind::String
            | TypeInfoKind::Number(_)
            | TypeInfoKind::Custom(_)
            | TypeInfoKind::QualifiedName(_)
            | TypeInfoKind::Qualified(_)
            | TypeInfoKind::Function(_)
            | TypeInfoKind::Struct(_)
            | TypeInfoKind::Enum(_)
            | TypeInfoKind::Spec(_) => self.clone(),
        }
    }

    /// Check if this type contains any unresolved type parameters.
    #[must_use = "this is a pure check with no side effects"]
    pub fn has_unresolved_params(&self) -> bool {
        match &self.kind {
            TypeInfoKind::Generic(_) => true,
            TypeInfoKind::Array(elem_type, _) => elem_type.has_unresolved_params(),
            // Primitive and named types have no type parameters
            TypeInfoKind::Unit
            | TypeInfoKind::Bool
            | TypeInfoKind::String
            | TypeInfoKind::Number(_)
            | TypeInfoKind::Custom(_)
            | TypeInfoKind::QualifiedName(_)
            | TypeInfoKind::Qualified(_)
            | TypeInfoKind::Function(_)
            | TypeInfoKind::Struct(_)
            | TypeInfoKind::Enum(_)
            | TypeInfoKind::Spec(_) => false,
        }
    }

    /// Converts a string type name to TypeInfoKind.
    ///
    /// Used for `Type::Custom` variants that reference types by name.
    /// Attempts to match against builtin type names, falling back to Custom.
    fn type_kind_from_simple_type(simple_type_name: &str) -> TypeInfoKind {
        TypeInfoKind::from_builtin_str(simple_type_name)
            .unwrap_or_else(|| TypeInfoKind::Custom(simple_type_name.to_string()))
    }

    /// Converts AST SimpleTypeKind to TypeInfoKind.
    ///
    /// This is the efficient path for primitive builtin types. The AST uses
    /// `Type::Simple(SimpleTypeKind)` for primitives, which are lightweight
    /// enum values without heap allocation. This method performs the direct
    /// mapping to the type checker's internal TypeInfoKind representation.
    ///
    /// Handles all primitive types:
    /// - Unit type (implicitly returned by functions without return type)
    /// - Boolean type
    /// - Signed integers: i8, i16, i32, i64
    /// - Unsigned integers: u8, u16, u32, u64
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

/// Extracts the array size from an expression.
///
/// Panics if the size expression is not a numeric literal.
fn extract_array_size(size_expr: Expression) -> u32 {
    if let Expression::Literal(Literal::Number(num_lit)) = size_expr {
        return num_lit.value.parse::<u32>().unwrap();
    }
    if let Expression::Identifier(identifier) = size_expr {
        todo!(
            "Constant identifiers for array sizes not yet implemented: {}",
            identifier.name
        );
    }
    panic!("Array size must be a numeric literal");
}
