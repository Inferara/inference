use core::fmt;
use std::fmt::{Display, Formatter};

use inference_ast::nodes::Type;
use rustc_hash::FxHashMap;

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum NumberTypeKindNumberType {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum TypeInfoKind {
    Unit,
    Bool,
    String,
    Number(NumberTypeKindNumberType),
    Custom(String),
    Array(Box<TypeInfo>, Option<u32>),
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
            TypeInfoKind::Number(number_type) => match number_type {
                NumberTypeKindNumberType::I8 => write!(f, "i8"),
                NumberTypeKindNumberType::I16 => write!(f, "i16"),
                NumberTypeKindNumberType::I32 => write!(f, "i32"),
                NumberTypeKindNumberType::I64 => write!(f, "i64"),
                NumberTypeKindNumberType::U8 => write!(f, "u8"),
                NumberTypeKindNumberType::U16 => write!(f, "u16"),
                NumberTypeKindNumberType::U32 => write!(f, "u32"),
                NumberTypeKindNumberType::U64 => write!(f, "u64"),
            },
            TypeInfoKind::Array(ty, length) => {
                if let Some(length) = length {
                    return write!(f, "[{ty}; {length}]");
                }
                write!(f, "[{ty}]")
            }
            TypeInfoKind::Custom(ty)
            | TypeInfoKind::Spec(ty)
            | TypeInfoKind::Struct(ty)
            | TypeInfoKind::Enum(ty)
            | TypeInfoKind::QualifiedName(ty)
            | TypeInfoKind::Qualified(ty)
            | TypeInfoKind::Function(ty) => write!(f, "{ty}"),
            TypeInfoKind::Generic(ty) => write!(f, "<{ty}>"),
        }
    }
}

impl TypeInfoKind {
    #[must_use]
    pub fn is_number(&self) -> bool {
        matches!(self, TypeInfoKind::Number(_))
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
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "{}<{}>", self.kind, type_params)
    }
}

impl TypeInfo {
    pub fn boolean() -> Self {
        Self {
            kind: TypeInfoKind::Bool,
            type_params: vec![],
        }
    }

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
    pub fn new_with_type_params(ty: &Type, type_param_names: &[String]) -> Self {
        match ty {
            Type::Simple(simple) => {
                // Check if this is a declared type parameter
                if type_param_names.contains(&simple.name) {
                    return Self {
                        kind: TypeInfoKind::Generic(simple.name.clone()),
                        type_params: vec![],
                    };
                }
                Self {
                    kind: Self::type_kind_from_simple_type(&simple.name),
                    type_params: vec![],
                }
            }
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
            Type::Array(array) => Self {
                kind: TypeInfoKind::Array(
                    Box::new(Self::new_with_type_params(&array.element_type, type_param_names)),
                    None,
                ),
                type_params: vec![],
            },
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
                    kind: TypeInfoKind::Custom(custom.name.clone()),
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

    fn type_kind_from_simple_type(simple_type_name: &str) -> TypeInfoKind {
        match simple_type_name.to_lowercase().as_str() {
            "bool" => TypeInfoKind::Bool,
            "string" => TypeInfoKind::String,
            "unit" => TypeInfoKind::Unit,
            "i8" => TypeInfoKind::Number(NumberTypeKindNumberType::I8),
            "i16" => TypeInfoKind::Number(NumberTypeKindNumberType::I16),
            "i32" => TypeInfoKind::Number(NumberTypeKindNumberType::I32),
            "i64" => TypeInfoKind::Number(NumberTypeKindNumberType::I64),
            "u8" => TypeInfoKind::Number(NumberTypeKindNumberType::U8),
            "u16" => TypeInfoKind::Number(NumberTypeKindNumberType::U16),
            "u32" => TypeInfoKind::Number(NumberTypeKindNumberType::U32),
            "u64" => TypeInfoKind::Number(NumberTypeKindNumberType::U64),
            _ => TypeInfoKind::Custom(simple_type_name.to_string()),
        }
    }
}
