use core::fmt;
use std::fmt::{Display, Formatter};

use inference_ast::nodes::Type;

#[derive(Debug, Eq, PartialEq, Clone)]
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

#[derive(Debug, Eq, PartialEq, Clone)]
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

#[derive(Debug, Eq, PartialEq, Clone)]
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
    #[must_use]
    pub fn new(ty: &Type) -> Self {
        match ty {
            Type::Simple(simple) => Self {
                kind: Self::type_kind_from_simple_type(&simple.name),
                type_params: vec![],
            },
            Type::Generic(generic) => Self {
                kind: TypeInfoKind::Generic(generic.base.name().clone()),
                type_params: generic
                    .parameters
                    .iter()
                    .map(|p| p.name().clone())
                    .collect(),
            },
            Type::QualifiedName(qualified_name) => Self {
                kind: TypeInfoKind::QualifiedName(format!(
                    "{}::{}",
                    qualified_name.qualifier.name(),
                    qualified_name.name.name()
                )),
                type_params: vec![],
            },
            Type::Qualified(qualified) => Self {
                kind: TypeInfoKind::Qualified(qualified.name.name().clone()),
                type_params: vec![],
            },
            Type::Array(array) => Self {
                kind: TypeInfoKind::Array(Box::new(Self::new(&array.element_type)), None),
                type_params: vec![],
            },
            Type::Function(func) => {
                //REVISIT
                let param_types = func
                    .parameters
                    .as_ref()
                    .map(|params| params.iter().map(TypeInfo::new).collect::<Vec<_>>())
                    .unwrap_or_default();
                let return_type = if func.returns.is_some() {
                    func.returns.as_ref().map(TypeInfo::new).unwrap_or_default()
                } else {
                    Self {
                        kind: TypeInfoKind::Unit,
                        type_params: vec![],
                    }
                };
                Self {
                    kind: TypeInfoKind::Function(format!(
                        "Function<{}, {}>",
                        param_types.len(),
                        return_type.kind
                    )),
                    type_params: vec![],
                }
            }
            Type::Custom(custom) => Self {
                kind: TypeInfoKind::Custom(custom.name.clone()),
                type_params: vec![],
            },
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
            _ => panic!("Unknown simple type: {simple_type_name}"),
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
}
