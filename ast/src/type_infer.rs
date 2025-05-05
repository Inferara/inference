use anyhow::bail;

use crate::types::{Definition, FunctionDefinition, Identifier, Statement, TypeInfo};
use crate::types::{Expression, Location, SimpleType, SourceFile, Type};
use std::cell::RefCell;
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
    variables: Vec<HashMap<String, TypeInfo>>, // stack of variable name -> type for each scope
}

impl SymbolTable {
    fn new() -> Self {
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

    fn push_scope(&mut self) {
        self.variables.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.variables.pop();
    }

    fn push_variable_to_scope(&mut self, name: String, var_type: TypeInfo) -> anyhow::Result<()> {
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

    fn register_type(&mut self, name: String, type_params: Vec<String>) -> anyhow::Result<()> {
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
            return Err(format!("Function `{name}` is already defined"));
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

    fn lookup_variable(&self, name: &String) -> Option<TypeInfo> {
        for scope in self.variables.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty.clone());
            }
        }
        None
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
        // Infer types for each function in each source file
        for source_file in program {
            // Directly iterate over definitions to ensure we operate on the actual AST nodes
            for def in &source_file.definitions {
                if let Definition::Function(function_definition) = def {
                    // Clone the Rc to share the underlying FunctionDefinition
                    self.infer_variables(function_definition.clone());
                }
            }
        }
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
                                .map(|param| param.name())
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

    fn collect_function_and_constant_definitions(&mut self, program: &mut Vec<SourceFile>) {
        for sf in program {
            for definition in &sf.definitions {
                match definition {
                    Definition::Constant(constant_definition) => todo!(),
                    Definition::Function(function_definition) => {
                        for param in function_definition.arguments.as_ref().unwrap_or(&vec![]) {
                            self.validate_type(
                                &param.ty,
                                function_definition.type_parameters.as_ref(),
                            );
                        }
                        if let Some(return_type) = &function_definition.returns {
                            self.validate_type(
                                return_type,
                                function_definition.type_parameters.as_ref(),
                            );
                        }
                        if !self.errors.is_empty() {
                            continue;
                        }
                        if let Err(err) = self.symbol_table.register_function(
                            function_definition.name(),
                            function_definition
                                .type_parameters
                                .as_ref()
                                .unwrap_or(&vec![])
                                .iter()
                                .map(|param| param.name())
                                .collect(),
                            function_definition
                                .arguments
                                .as_ref()
                                .unwrap_or(&vec![])
                                .iter()
                                .map(|param| param.ty.clone())
                                .collect(),
                            function_definition
                                .returns
                                .as_ref()
                                .unwrap_or(&Type::Simple(Rc::new(SimpleType::new(
                                    0,
                                    Location::default(),
                                    "Unit".into(),
                                ))))
                                .clone(),
                        ) {
                            self.errors.push(err);
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

    fn validate_type(&mut self, ty: &Type, type_parameters: Option<&Vec<Rc<Identifier>>>) {
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
                    if type_params.len() != generic_type.parameters.len() {
                        self.errors.push(format!(
                            "Type parameter count mismatch for `{}`: expected {}, found {}",
                            generic_type.base.name(),
                            generic_type.parameters.len(),
                            type_params.len()
                        ));
                    }
                    let generic_param_names: Vec<String> = generic_type
                        .parameters
                        .iter()
                        .map(|param| param.name())
                        .collect();
                    for param in &generic_type.parameters {
                        if !generic_param_names.contains(&param.name()) {
                            self.errors.push(format!(
                                "Type parameter `{}` not found in `{}`",
                                param.name(),
                                generic_type.base.name()
                            ));
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

    #[allow(clippy::needless_pass_by_value)]
    fn infer_variables(&mut self, function_definition: Rc<FunctionDefinition>) {
        self.symbol_table.push_scope();
        if let Some(arguments) = &function_definition.arguments {
            for argument in arguments {
                if let Err(err) = self
                    .symbol_table
                    .push_variable_to_scope(argument.name(), TypeInfo::new(&argument.ty))
                {
                    self.errors.push(err.to_string());
                }
            }
        }
        for stmt in &mut function_definition.body.statements() {
            self.infer_statement(
                stmt,
                function_definition.returns.clone(),
                &function_definition
                    .type_parameters
                    .as_ref()
                    .unwrap_or(&vec![])
                    .iter()
                    .map(|p| p.name())
                    .collect(),
            );
        }
        self.symbol_table.pop_scope();
    }

    fn infer_statement(
        &mut self,
        statement: &mut Statement,
        return_type: Option<Type>,
        type_parameters: &Vec<String>,
    ) {
        match statement {
            Statement::Assign(assign_statement) => {
                // infer target type from left-hand side
                let target_type = self.infer_expression(&mut assign_statement.left.borrow_mut());
                // handle Uzumaki expression on right-hand side
                let mut right_expr = assign_statement.right.borrow_mut();
                if let Expression::Uzumaki(uzumaki_rc) = &*right_expr {
                    // annotate type_info on the UzumakiExpression
                    *uzumaki_rc.type_info.borrow_mut() = target_type.clone();
                } else {
                    let value_type = self.infer_expression(&mut right_expr);
                    if let (Some(target), Some(val)) = (target_type, value_type) {
                        if target != val {
                            self.errors.push(format!(
                                "Cannot assign value of type {val:?} to variable of type {target:?}"
                            ));
                        }
                    }
                }
            }
            Statement::Block(block_type) => {
                self.symbol_table.push_scope();
                for stmt in &mut block_type.statements() {
                    self.infer_statement(stmt, return_type.clone(), type_parameters);
                }
                self.symbol_table.pop_scope();
            }
            Statement::Expression(expression) => {
                self.infer_expression(expression);
            }
            Statement::Return(return_statement) => todo!(),
            Statement::Loop(loop_statement) => todo!(),
            Statement::Break(break_statement) => todo!(),
            Statement::If(if_statement) => todo!(),
            Statement::VariableDefinition(variable_definition_statement) => {
                let target_type = TypeInfo::new(&variable_definition_statement.ty);
                if let Some(initial_value) = variable_definition_statement.value.as_ref() {
                    // check for Uzumaki initializer
                    let mut expr_ref = initial_value.borrow_mut();
                    if let Expression::Uzumaki(uzumaki_rc) = &mut *expr_ref {
                        println!("Uzumaki: {uzumaki_rc:?}\n");
                        *uzumaki_rc.type_info.borrow_mut() = Some(target_type.clone());
                    } else if let Some(init_type) = self.infer_expression(&mut expr_ref) {
                        if init_type != TypeInfo::new(&variable_definition_statement.ty) {
                            self.errors.push(format!(
                                "Type mismatch in variable definition: expected {:?}, found {:?}",
                                variable_definition_statement.ty, init_type
                            ));
                        }
                    }
                }
                if let Err(err) = self.symbol_table.push_variable_to_scope(
                    variable_definition_statement.name(),
                    TypeInfo::new(&variable_definition_statement.ty),
                ) {
                    self.errors.push(err.to_string());
                }
                //TODO handle the case when the variable is not initialized
            }
            Statement::TypeDefinition(type_definition_statement) => todo!(),
            Statement::Assert(assert_statement) => todo!(),
            Statement::ConstantDefinition(constant_definition) => todo!(),
        }
    }

    fn infer_expression(&mut self, expression: &mut Expression) -> Option<TypeInfo> {
        match expression {
            Expression::ArrayIndexAccess(array_index_access_expression) => {
                todo!()
            }
            Expression::MemberAccess(member_access_expression) => todo!(),
            Expression::FunctionCall(function_call_expression) => todo!(),
            Expression::PrefixUnary(prefix_unary_expression) => todo!(),
            Expression::Parenthesized(parenthesized_expression) => {
                self.infer_expression(&mut parenthesized_expression.expression.borrow_mut())
            }
            Expression::Binary(binary_expression) => todo!(),
            Expression::Literal(literal) => todo!(),
            Expression::Identifier(identifier) => {
                if let Some(var_ty) = self.symbol_table.lookup_variable(&identifier.name) {
                    *identifier.type_info.borrow_mut() = Some(var_ty.clone());
                    Some(var_ty)
                } else {
                    self.errors
                        .push(format!("Use of undeclared variable `{}`", identifier.name));
                    None
                }
            }
            Expression::Type(type_expr) => todo!(),
            Expression::Uzumaki(uzumaki) => uzumaki.type_info.borrow().clone(),
        }
    }

    #[allow(dead_code)]
    fn types_equal(left: &Type, right: &Type) -> bool {
        match (left, right) {
            (Type::Array(left), Type::Array(right)) => {
                Self::types_equal(&left.element_type, &right.element_type)
            }
            (Type::Simple(left), Type::Simple(right)) => left.name == right.name,
            (Type::Generic(left), Type::Generic(right)) => {
                left.base.name() == right.base.name() && left.parameters == right.parameters
            }
            (Type::Qualified(left), Type::Qualified(right)) => left.name() == right.name(),
            (Type::QualifiedName(left), Type::QualifiedName(right)) => {
                left.qualifier() == right.qualifier() && left.name() == right.name()
            }
            (Type::Custom(left), Type::Custom(right)) => left.name() == right.name(),
            (Type::Function(left), Type::Function(right)) => {
                let left_has_return_type = left.returns.is_some();
                let right_has_return_type = right.returns.is_some();
                if left_has_return_type != right_has_return_type {
                    return false;
                }
                if left_has_return_type {
                    if let (Some(left_return_type), Some(right_return_type)) =
                        (&left.returns, &right.returns)
                    {
                        if !Self::types_equal(left_return_type, right_return_type) {
                            return false;
                        }
                    }
                }
                let left_has_parameters = left.parameters.is_some();
                let right_has_parameters = right.parameters.is_some();
                if left_has_parameters != right_has_parameters {
                    return false;
                }
                if left_has_parameters {
                    if let (Some(left_parameters), Some(right_parameters)) =
                        (&left.parameters, &right.parameters)
                    {
                        if left_parameters.len() != right_parameters.len() {
                            return false;
                        }
                        for (left_param, right_param) in
                            left_parameters.iter().zip(right_parameters.iter())
                        {
                            if !Self::types_equal(left_param, right_param) {
                                return false;
                            }
                        }
                    }
                }
                true
            }
            _ => false,
        }
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
