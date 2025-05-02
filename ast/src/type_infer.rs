use anyhow::bail;

use crate::types::{Definition, Identifier, TypeInfo};
#[allow(clippy::all, unused_imports, dead_code)]
use crate::types::{
    Expression, Literal, Location, OperatorKind, SimpleType, SourceFile, Type, TypeArray,
};
use crate::{arena::Arena, types::GenericType};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone)]
struct FuncSignature {
    name: String,
    type_params: Vec<String>,
    param_types: Vec<Type>,
    return_type: Type,
}

struct SymbolTable {
    types: HashMap<String, TypeInfo>, // map of type name -> type info
    functions: HashMap<String, FuncSignature>, // map of function name -> signature
    variables: Vec<HashMap<String, Type>>, // stack of variable name -> type for each scope
}

impl SymbolTable {
    pub fn new() -> Self {
        let mut table = SymbolTable {
            types: HashMap::default(),
            functions: HashMap::default(),
            variables: Vec::default(),
        };
        for size in ["8", "16", "32", "64"] {
            table.types.insert(
                format!("i{size}"),
                TypeInfo {
                    name: format!("i{size}"),
                    type_params: vec![],
                },
            );
            table.types.insert(
                format!("u{size}"),
                TypeInfo {
                    name: format!("u{size}"),
                    type_params: vec![],
                },
            );
        }
        table.types.insert(
            "bool".to_string(),
            TypeInfo {
                name: "bool".to_string(),
                type_params: vec![],
            },
        );
        table.types.insert(
            "string".to_string(),
            TypeInfo {
                name: "string".to_string(),
                type_params: vec![],
            },
        );
        table
    }

    fn register_type(&mut self, name: String, type_params: Vec<TypeInfo>) -> anyhow::Result<()> {
        if self.types.contains_key(&name) {
            bail!("Type `{name}` is already defined")
        }
        self.types
            .insert(name.clone(), TypeInfo { name, type_params });
        Ok(())
    }

    fn register_function(
        &mut self,
        name: String,
        type_params: Vec<String>,
        param_types: Vec<Type>,
        return_type: Type,
    ) -> Result<(), String> {
        if self.functions.contains_key(&name) {
            return Err(format!("Function `{}` is already defined", name));
        }
        self.functions.insert(
            name.clone(),
            FuncSignature {
                name,
                type_params,
                param_types,
                return_type,
            },
        );
        Ok(())
    }

    fn lookup_type(&self, name: &str) -> Option<&TypeInfo> {
        self.types.get(name)
    }
}

pub(crate) struct TypeChecker {
    symbol_table: SymbolTable,
    errors: Vec<String>,
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker {
            symbol_table: SymbolTable::new(),
            errors: vec![],
        }
    }

    //TODO `program` should be flattened to a single vector of definitions?
    pub fn infer_types(&mut self, program: &mut Vec<SourceFile>) -> anyhow::Result<()> {
        self.register_types(program);
        self.collect_function_and_constant_definitions(program);
        if !self.errors.is_empty() {
            bail!(std::mem::take(&mut self.errors).join("; ")) //TODO: handle it better
        }
        Ok(())
    }

    fn register_types(&mut self, program: &mut Vec<SourceFile>) {
        for source_file in program {
            for definition in &source_file.definitions {
                match definition {
                    Definition::Type(type_definition) => match &type_definition.ty {
                        Type::Generic(generic_type) => {
                            let type_params = generic_type
                                .parameters
                                .iter()
                                .map(|param| {
                                    if let Type::Generic(generic_param) = param {
                                        Self::construct_generic_type_info(generic_param.clone())
                                    } else {
                                        panic!("Expected a generic type parameter")
                                    }
                                })
                                .collect();
                            self.symbol_table
                                .register_type(type_definition.name(), type_params)
                                .unwrap_or_else(|_| {
                                    self.errors.push(format!(
                                        "Error registering type `{}`",
                                        type_definition.name()
                                    ));
                                });
                        }
                        _ => {
                            self.symbol_table
                                .register_type(type_definition.name(), vec![])
                                .unwrap_or_else(|_| {
                                    self.errors.push(format!(
                                        "Error registering type `{}`",
                                        type_definition.name()
                                    ));
                                });
                        }
                    },
                    Definition::Struct(struct_definition) => {
                        self.symbol_table
                            .register_type(struct_definition.name(), vec![])
                            .unwrap_or_else(|_| {
                                self.errors.push(format!(
                                    "Error registering type `{}`",
                                    struct_definition.name()
                                ));
                            });
                    }
                    Definition::Enum(enum_definition) => {
                        self.symbol_table
                            .register_type(enum_definition.name(), vec![])
                            .unwrap_or_else(|_| {
                                self.errors.push(format!(
                                    "Error registering type `{}`",
                                    enum_definition.name()
                                ));
                            });
                    }
                    Definition::Spec(spec_definition) => {
                        self.symbol_table
                            .register_type(spec_definition.name(), vec![])
                            .unwrap_or_else(|_| {
                                self.errors.push(format!(
                                    "Error registering type `{}`",
                                    spec_definition.name()
                                ));
                            });
                    }
                    _ => {
                        // Functions and constants are handled in `collect_function_and_constant_definitions`
                    }
                }
            }
        }
    }

    fn construct_generic_type_info(generic_type_definition: Rc<GenericType>) -> TypeInfo {
        Self::construct_type_info(generic_type_definition, vec![])
    }

    #[allow(clippy::needless_pass_by_value)]
    fn construct_type_info(
        generic_type_definition: Rc<GenericType>,
        type_params: Vec<TypeInfo>,
    ) -> TypeInfo {
        let name = generic_type_definition.base.name();
        let mut type_info = TypeInfo { name, type_params };
        for param in &generic_type_definition.parameters {
            if let Type::Generic(generic_param) = param {
                let param_info = Self::construct_generic_type_info(generic_param.clone());
                type_info.type_params.push(param_info);
            }
        }
        type_info
    }

    //TODO continue implementing this function
    fn collect_function_and_constant_definitions(&mut self, program: &mut Vec<SourceFile>) {
        for sf in program {
            for definition in &sf.definitions {
                match definition {
                    Definition::Constant(constant_definition) => todo!(),
                    Definition::Function(function_definition) => {
                        for param in function_definition.arguments.as_ref().unwrap_or(&vec![]) {
                            self.validate_type(&param.ty, None);
                        }
                        if let Some(return_type) = &function_definition.returns {
                            self.validate_type(
                                return_type,
                                function_definition.type_parameters.clone(),
                            );
                        }
                    }
                    Definition::ExternalFunction(external_function_definition) => {
                        todo!()
                    }
                    _ => {
                        // Already registered in `register_types`
                    }
                }
            }
        }
    }

    fn validate_type(&mut self, ty: &Type, type_parameters: Option<Vec<Rc<Type>>>) {
        match ty {
            Type::Array(type_array) => self.validate_type(&type_array.element_type, None),
            Type::Simple(simple_type) => {
                if self.symbol_table.lookup_type(&simple_type.name).is_none() {
                    self.errors
                        .push(format!("Unknown type `{}`", simple_type.name));
                }
            }
            Type::Generic(generic_type) => {
                if self
                    .symbol_table
                    .lookup_type(&generic_type.base.name())
                    .is_none()
                {
                    self.errors
                        .push(format!("Unknown type `{}`", generic_type.base.name()));
                }
                if let Some(type_params) = &type_parameters {
                    for param in &generic_type.parameters {
                        if let Type::Generic(generic_param) = param {
                            if !type_params
                                .iter()
                                .any(|tp| tp.name == generic_param.base.name)
                            {
                                self.errors.push(format!(
                                    "Unknown type parameter `{}` in generic type `{}`",
                                    generic_param.base.name, generic_type.base.name
                                ));
                            }
                        }
                    }
                }
            }
            Type::Function(_) => {
                //REVISIT Skip becase this is a call ABI
            }
            Type::QualifiedName(_) | Type::Qualified(_) => {
                //Skip qualified names for now
            }
            Type::Custom(identifier) => {
                if self.symbol_table.lookup_type(&identifier.name).is_none() {
                    self.errors
                        .push(format!("Unknown type `{}`", identifier.name));
                }
            }
        }
    }
}

// pub struct TypeContext<'a> {
//     pub symbols: &'a SymbolTable,
// }

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

// pub fn infer_expr(expr: &Expression, ctx: &TypeContext) -> Result<Type, TypeError> {
//     match expr {
//         Expression::Literal(lit, _) => match lit {
//             Literal::Bool(_) => Ok(Type::Simple(Rc::new(SimpleType::new(
//                 0,
//                 Location::default(),
//                 "Bool".into(),
//             )))),
//             Literal::String(_) => Ok(Type::Simple(Rc::new(SimpleType::new(
//                 0,
//                 Location::default(),
//                 "String".into(),
//             )))),
//             Literal::Number(_) => Ok(Type::Simple(Rc::new(SimpleType::new(
//                 0,
//                 Location::default(),
//                 "Number".into(),
//             )))),
//             Literal::Unit(_) => Ok(Type::Simple(Rc::new(SimpleType::new(
//                 0,
//                 Location::default(),
//                 "Unit".into(),
//             )))),
//             Literal::Array(arr) => {
//                 let mut elem_ty: Option<Type> = None;
//                 let arr_node = arr.as_ref();
//                 for e in &arr_node.elements {
//                     let ty = infer_expr(e, ctx)?;
//                     if let Some(prev) = &elem_ty {
//                         if *prev != ty {
//                             return Err(TypeError::Mismatch {
//                                 expected: prev.clone(),
//                                 found: ty.clone(),
//                                 loc: arr.location.clone(),
//                             });
//                         }
//                     } else {
//                         elem_ty = Some(ty.clone());
//                     }
//                 }
//                 let element = elem_ty.unwrap_or_else(|| {
//                     Type::Simple(Rc::new(SimpleType::new(
//                         0,
//                         Location::default(),
//                         "Unit".into(),
//                     )))
//                 });
//                 Ok(Type::Array(Rc::new(TypeArray::new(
//                     0,
//                     Location::default(),
//                     element,
//                     None,
//                 ))))
//             }
//         },
//         Expression::Identifier(id, _) => {
//             let name = &id.name;
//             if let Some(ty) = ctx.symbols.lookup(name) {
//                 Ok(ty)
//             } else {
//                 Err(TypeError::UnknownIdentifier(
//                     name.clone(),
//                     id.location.clone(),
//                 ))
//             }
//         }
//         Expression::Binary(bin, _) => {
//             let left_ty = infer_expr(&bin.left, ctx)?;
//             let right_ty = infer_expr(&bin.right, ctx)?;
//             if left_ty != right_ty {
//                 return Err(TypeError::Mismatch {
//                     expected: left_ty.clone(),
//                     found: right_ty.clone(),
//                     loc: bin.location.clone(),
//                 });
//             }
//             let res_ty = match &bin.operator {
//                 OperatorKind::Add | OperatorKind::Sub | OperatorKind::Mul | OperatorKind::Div => {
//                     left_ty.clone()
//                 }
//                 OperatorKind::Eq
//                 | OperatorKind::Ne
//                 | OperatorKind::Lt
//                 | OperatorKind::Le
//                 | OperatorKind::Gt
//                 | OperatorKind::Ge => Type::Simple(Rc::new(SimpleType::new(
//                     0,
//                     bin.location.clone(),
//                     "Bool".into(),
//                 ))),
//                 op => {
//                     return Err(TypeError::Other(
//                         format!("Operator {op:?} not supported"),
//                         bin.location.clone(),
//                     ))
//                 }
//             };
//             Ok(res_ty)
//         }
//         _ => Err(TypeError::Other(
//             "Type inference not implemented for this expression variant".into(),
//             Location::default(),
//         )),
//     }
// }

// pub fn traverse_source_files(
//     source_files: &[crate::types::SourceFile],
//     symbols: &SymbolTable,
// ) -> Result<(), TypeError> {
//     let ctx = TypeContext { symbols };
//     for sf in source_files {
//         for def in &sf.definitions {
//             if let crate::types::Definition::Function(func_rc) = def {
//                 traverse_function(func_rc, &ctx)?;
//             }
//         }
//     }
//     Ok(())
// }

// fn traverse_function(
//     func_rc: &std::rc::Rc<crate::types::FunctionDefinition>,
//     ctx: &TypeContext,
// ) -> Result<(), TypeError> {
//     let func = func_rc.as_ref();
//     // TODO: insert parameter types into ctx.symbols if needed
//     traverse_block(&func.body, ctx)
// }

// fn traverse_block(
//     block_type: &crate::types::BlockType,
//     ctx: &TypeContext,
// ) -> Result<(), TypeError> {
//     use crate::types::BlockType;
//     if let BlockType::Block(b_rc) = block_type {
//         let block = b_rc.as_ref();
//         for stmt in &block.statements {
//             traverse_statement(stmt, ctx)?;
//         }
//     }
//     Ok(())
// }

// fn traverse_statement(stmt: &crate::types::Statement, ctx: &TypeContext) -> Result<(), TypeError> {
//     use crate::types::Statement;
//     match stmt {
//         Statement::Expression(expr) => {
//             infer_expr(expr, ctx)?;
//         }
//         Statement::Return(ret_rc) => {
//             infer_expr(&ret_rc.expression, ctx)?;
//         }
//         Statement::Assert(assert_rc) => {
//             infer_expr(&assert_rc.expression, ctx)?;
//         }
//         Statement::If(if_rc) => {
//             infer_expr(&if_rc.condition, ctx)?;
//             traverse_block(&if_rc.if_arm, ctx)?;
//             if let Some(else_arm) = &if_rc.else_arm {
//                 traverse_block(else_arm, ctx)?;
//             }
//         }
//         Statement::Loop(loop_rc) => {
//             if let Some(cond) = &loop_rc.condition {
//                 infer_expr(cond, ctx)?;
//             }
//             traverse_block(&loop_rc.body, ctx)?;
//         }
//         Statement::VariableDefinition(vd_rc) => {
//             if let Some(init) = &vd_rc.value {
//                 infer_expr(init, ctx)?;
//             }
//         }
//         Statement::ConstantDefinition(_cd_rc) => {
//             // constant definitions have a Literal value; skip or handle separately
//         }
//         Statement::Block(block_type) => {
//             traverse_block(block_type, ctx)?;
//         }
//         _ => {}
//     }
//     Ok(())
// }
