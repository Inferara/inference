use core::fmt;
use std::fmt::{Display, Formatter};

use crate::types::Type;

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum TypeInfoKind {
    Unit,
    Bool,
    String,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    Array(String),
    Custom(String),
    Generic(String),
    QualifiedName(String),
    Qualified(String),
    Function(String),
}

impl Display for TypeInfoKind {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            TypeInfoKind::Unit => write!(f, "Unit"),
            TypeInfoKind::Bool => write!(f, "Bool"),
            TypeInfoKind::String => write!(f, "String"),
            TypeInfoKind::I8 => write!(f, "I8"),
            TypeInfoKind::I16 => write!(f, "I16"),
            TypeInfoKind::I32 => write!(f, "I32"),
            TypeInfoKind::I64 => write!(f, "I64"),
            TypeInfoKind::U8 => write!(f, "U8"),
            TypeInfoKind::U16 => write!(f, "U16"),
            TypeInfoKind::U32 => write!(f, "U32"),
            TypeInfoKind::U64 => write!(f, "U64"),
            TypeInfoKind::Array(ty) => write!(f, "[{ty}]"),
            TypeInfoKind::Custom(ty)
            | TypeInfoKind::QualifiedName(ty)
            | TypeInfoKind::Qualified(ty)
            | TypeInfoKind::Function(ty) => write!(f, "{ty}"),
            TypeInfoKind::Generic(ty) => write!(f, "<{ty}>"),
        }
    }
}

impl TypeInfoKind {
    pub fn is_number(&self) -> bool {
        matches!(
            self,
            TypeInfoKind::I8
                | TypeInfoKind::I16
                | TypeInfoKind::I32
                | TypeInfoKind::I64
                | TypeInfoKind::U8
                | TypeInfoKind::U16
                | TypeInfoKind::U32
                | TypeInfoKind::U64
        )
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
                kind: TypeInfoKind::Array(format!(
                    "[{};TODO_arr_size]",
                    Self::new(&array.element_type)
                )),
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
        match simple_type_name {
            "bool" => TypeInfoKind::Bool,
            "string" => TypeInfoKind::String,
            "i8" => TypeInfoKind::I8,
            "i16" => TypeInfoKind::I16,
            "i32" => TypeInfoKind::I32,
            "i64" => TypeInfoKind::I64,
            "u8" => TypeInfoKind::U8,
            "u16" => TypeInfoKind::U16,
            "u32" => TypeInfoKind::U32,
            "u64" => TypeInfoKind::U64,
            _ => panic!("Unknown simple type: {simple_type_name}"),
        }
    }

    pub fn is_number(&self) -> bool {
        self.kind.is_number()
    }

    pub fn is_array(&self) -> bool {
        matches!(self.kind, TypeInfoKind::Array(_))
    }
}
