use anyhow::bail;

use crate::type_info::{NumberTypeKindNumberType, TypeInfo, TypeInfoKind};
use inference_ast::nodes::Type;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone)]
pub(crate) struct FuncSignature {
    pub(crate) type_params: Vec<String>,
    pub(crate) param_types: Vec<TypeInfo>,
    pub(crate) return_type: TypeInfo,
}

#[derive(Default, Clone)]
pub(crate) struct SymbolTable {
    types: FxHashMap<String, TypeInfo>, // map of type name -> type info
    functions: FxHashMap<String, FuncSignature>, // map of function name -> signature
    variables: Vec<FxHashMap<String, TypeInfo>>, // stack of variable name -> type for each scope
}

impl SymbolTable {
    pub(crate) fn new() -> Self {
        let mut table = SymbolTable {
            types: FxHashMap::default(),
            functions: FxHashMap::default(),
            variables: Vec::default(),
        };
        table.types.insert(
            "i8".to_string(),
            TypeInfo {
                kind: TypeInfoKind::Number(NumberTypeKindNumberType::I8),
                type_params: vec![],
            },
        );
        table.types.insert(
            "i16".to_string(),
            TypeInfo {
                kind: TypeInfoKind::Number(NumberTypeKindNumberType::I16),
                type_params: vec![],
            },
        );
        table.types.insert(
            "i32".to_string(),
            TypeInfo {
                kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
                type_params: vec![],
            },
        );
        table.types.insert(
            "i64".to_string(),
            TypeInfo {
                kind: TypeInfoKind::Number(NumberTypeKindNumberType::I64),
                type_params: vec![],
            },
        );
        table.types.insert(
            "u8".to_string(),
            TypeInfo {
                kind: TypeInfoKind::Number(NumberTypeKindNumberType::U8),
                type_params: vec![],
            },
        );
        table.types.insert(
            "u16".to_string(),
            TypeInfo {
                kind: TypeInfoKind::Number(NumberTypeKindNumberType::U16),
                type_params: vec![],
            },
        );
        table.types.insert(
            "u32".to_string(),
            TypeInfo {
                kind: TypeInfoKind::Number(NumberTypeKindNumberType::U32),
                type_params: vec![],
            },
        );
        table.types.insert(
            "u64".to_string(),
            TypeInfo {
                kind: TypeInfoKind::Number(NumberTypeKindNumberType::U64),
                type_params: vec![],
            },
        );
        table.types.insert(
            "bool".to_string(),
            TypeInfo {
                kind: TypeInfoKind::Bool,
                type_params: vec![],
            },
        );
        table.types.insert(
            "string".to_string(),
            TypeInfo {
                kind: TypeInfoKind::String,
                type_params: vec![],
            },
        );
        table
    }

    pub(crate) fn push_scope(&mut self) {
        self.variables.push(FxHashMap::default());
    }

    pub(crate) fn pop_scope(&mut self) {
        self.variables.pop();
    }

    pub(crate) fn push_variable_to_scope(
        &mut self,
        name: String,
        var_type: TypeInfo,
    ) -> anyhow::Result<()> {
        if let Some(scope) = self.variables.last_mut() {
            if scope.contains_key(&name) {
                bail!("Variable `{name}` already declared in this scope");
            }
            scope.insert(name, var_type);
            Ok(())
        } else {
            bail!("No active scope to push variables".to_string())
        }
    }

    pub(crate) fn register_type(&mut self, name: &String, ty: Option<&Type>) -> anyhow::Result<()> {
        if self.types.contains_key(name) {
            bail!("Type `{name}` is already defined")
        }
        if let Some(ty) = ty {
            self.types.insert(name.clone(), TypeInfo::new(ty));
        } else {
            self.types.insert(
                name.clone(),
                TypeInfo {
                    kind: TypeInfoKind::Custom(name.clone()),
                    type_params: vec![],
                },
            );
        }
        Ok(())
    }

    pub(crate) fn register_struct(&mut self, name: &String) -> anyhow::Result<()> {
        if self.types.contains_key(name) {
            bail!("Struct `{name}` is already defined")
        }
        self.types.insert(
            name.clone(),
            TypeInfo {
                kind: TypeInfoKind::Struct(name.clone()),
                type_params: vec![],
            },
        );
        Ok(())
    }

    pub(crate) fn register_enum(&mut self, name: &String) -> anyhow::Result<()> {
        if self.types.contains_key(name) {
            bail!("Enum `{name}` is already defined")
        }
        self.types.insert(
            name.clone(),
            TypeInfo {
                kind: TypeInfoKind::Enum(name.clone()),
                type_params: vec![],
            },
        );
        Ok(())
    }

    pub(crate) fn register_spec(&mut self, name: &String) -> anyhow::Result<()> {
        if self.types.contains_key(name) {
            bail!("Spec `{name}` is already defined")
        }
        self.types.insert(
            name.clone(),
            TypeInfo {
                kind: TypeInfoKind::Spec(name.clone()),
                type_params: vec![],
            },
        );
        Ok(())
    }

    pub(crate) fn register_function(
        &mut self,
        name: &String,
        type_params: Vec<String>,
        param_types: &[Type],
        return_type: &Type,
    ) -> Result<(), String> {
        if self.functions.contains_key(name) {
            return Err(format!("Function `{name}` is already defined"));
        }
        self.functions.insert(
            name.clone(),
            FuncSignature {
                type_params,
                param_types: param_types.iter().map(TypeInfo::new).collect(),
                return_type: TypeInfo::new(return_type),
            },
        );
        Ok(())
    }

    pub(crate) fn lookup_type(&self, name: &str) -> Option<&TypeInfo> {
        self.types.get(name)
    }

    pub(crate) fn lookup_variable(&self, name: &String) -> Option<TypeInfo> {
        for scope in self.variables.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty.clone());
            }
        }
        None
    }

    pub(crate) fn lookup_function(&self, name: &String) -> Option<&FuncSignature> {
        self.functions.get(name)
    }
}

// /// Errors during type inference
// #[derive(Debug)]
// pub enum TypeError {
//     Mismatch {
//         expected: Type,
//         found: Type,
//         loc: Location,
//     },
//     UnknownIdentifier(String, Location),
//     Other(String, Location),
// }
