use anyhow::bail;

use crate::nodes::{
    ArgumentType, Definition, FunctionDefinition, Identifier, Literal, OperatorKind, Statement,
    UnaryOperatorKind,
};
use crate::nodes::{Expression, Location, SimpleType, SourceFile, Type};
use crate::type_info::{NumberTypeKindNumberType, TypeInfo, TypeInfoKind};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone)]
struct FuncSignature {
    type_params: Vec<String>,
    param_types: Vec<TypeInfo>,
    return_type: TypeInfo,
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

    fn register_type(&mut self, name: &String, ty: Option<&Type>) -> anyhow::Result<()> {
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

    fn register_struct(&mut self, name: &String) -> anyhow::Result<()> {
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

    fn register_enum(&mut self, name: &String) -> anyhow::Result<()> {
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

    fn register_spec(&mut self, name: &String) -> anyhow::Result<()> {
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

    fn register_function(
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

    fn lookup_function(&self, name: &String) -> Option<&FuncSignature> {
        self.functions.get(name)
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
                    Definition::Type(type_definition) => {
                        self.symbol_table
                            .register_type(&type_definition.name(), Some(&type_definition.ty))
                            .unwrap_or_else(|_| {
                                self.errors.push(format!(
                                    "Error registering type `{}`",
                                    type_definition.name()
                                ));
                            });
                    }
                    Definition::Struct(struct_definition) => {
                        self.symbol_table
                            .register_struct(&struct_definition.name())
                            .unwrap_or_else(|_| {
                                self.errors.push(format!(
                                    "Error registering type `{}`",
                                    struct_definition.name()
                                ));
                            });
                    }
                    Definition::Enum(enum_definition) => {
                        self.symbol_table
                            .register_enum(&enum_definition.name())
                            .unwrap_or_else(|_| {
                                self.errors.push(format!(
                                    "Error registering type `{}`",
                                    enum_definition.name()
                                ));
                            });
                    }
                    Definition::Spec(spec_definition) => {
                        self.symbol_table
                            .register_spec(&spec_definition.name())
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

    #[allow(clippy::too_many_lines)]
    fn collect_function_and_constant_definitions(&mut self, program: &mut Vec<SourceFile>) {
        for sf in program {
            for definition in &sf.definitions {
                match definition {
                    Definition::Constant(constant_definition) => {
                        if let Err(err) = self.symbol_table.push_variable_to_scope(
                            constant_definition.name(),
                            TypeInfo::new(&constant_definition.ty),
                        ) {
                            self.errors.push(err.to_string());
                        }
                    }
                    Definition::Function(function_definition) => {
                        for param in function_definition.arguments.as_ref().unwrap_or(&vec![]) {
                            match param {
                                ArgumentType::SelfReference(_) => {
                                    todo!() //TODO handle self reference
                                }
                                ArgumentType::IgnoreArgument(ignore_argument) => {
                                    self.validate_type(
                                        &ignore_argument.ty,
                                        function_definition.type_parameters.as_ref(),
                                    );
                                }
                                ArgumentType::Argument(arg) => {
                                    self.validate_type(
                                        &arg.ty,
                                        function_definition.type_parameters.as_ref(),
                                    );
                                }
                                ArgumentType::Type(ty) => {
                                    self.validate_type(
                                        ty,
                                        function_definition.type_parameters.as_ref(),
                                    );
                                }
                            }
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
                            &function_definition.name(),
                            function_definition
                                .type_parameters
                                .as_ref()
                                .unwrap_or(&vec![])
                                .iter()
                                .map(|param| param.name())
                                .collect::<Vec<_>>(),
                            &function_definition
                                .arguments
                                .as_ref()
                                .unwrap_or(&vec![])
                                .iter()
                                .map(|param| match param {
                                    ArgumentType::SelfReference(_) => todo!(),
                                    ArgumentType::IgnoreArgument(ignore_argument) => {
                                        ignore_argument.ty.clone()
                                    }
                                    ArgumentType::Argument(argument) => argument.ty.clone(),
                                    ArgumentType::Type(ty) => ty.clone(),
                                })
                                .collect::<Vec<_>>(),
                            &function_definition
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
                        if let Err(err) = self.symbol_table.register_function(
                            &external_function_definition.name(),
                            vec![],
                            &external_function_definition
                                .arguments
                                .as_ref()
                                .unwrap_or(&vec![])
                                .iter()
                                .map(|param| match param {
                                    ArgumentType::SelfReference(_) => todo!(),
                                    ArgumentType::IgnoreArgument(ignore_argument) => {
                                        ignore_argument.ty.clone()
                                    }
                                    ArgumentType::Argument(argument) => argument.ty.clone(),
                                    ArgumentType::Type(ty) => ty.clone(),
                                })
                                .collect::<Vec<_>>(),
                            &external_function_definition
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
            Type::Function(_) | Type::QualifiedName(_) | Type::Qualified(_) => {
                //REVISIT Skip becase this is a call ABI
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
                match argument {
                    ArgumentType::Argument(arg) => {
                        if let Err(err) = self
                            .symbol_table
                            .push_variable_to_scope(arg.name(), TypeInfo::new(&arg.ty))
                        {
                            self.errors.push(err.to_string());
                        }
                    }
                    ArgumentType::SelfReference(_) => todo!(),
                    ArgumentType::IgnoreArgument(_) | ArgumentType::Type(_) => {}
                }
            }
        }
        for stmt in &mut function_definition.body.statements() {
            self.infer_statement(
                stmt,
                &function_definition
                    .returns
                    .as_ref()
                    .map(TypeInfo::new)
                    .unwrap_or_default(),
            );
        }
        self.symbol_table.pop_scope();
    }

    #[allow(clippy::too_many_lines)]
    fn infer_statement(&mut self, statement: &mut Statement, return_type: &TypeInfo) {
        match statement {
            Statement::Assign(assign_statement) => {
                let target_type = self.infer_expression(&mut assign_statement.left.borrow_mut());
                let mut right_expr = assign_statement.right.borrow_mut();
                if let Expression::Uzumaki(uzumaki_rc) = &*right_expr {
                    *uzumaki_rc.type_info.borrow_mut() = target_type;
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
                    self.infer_statement(stmt, return_type);
                }
                self.symbol_table.pop_scope();
            }
            Statement::Expression(expression) => {
                self.infer_expression(expression);
            }
            Statement::Return(return_statement) => {
                let value_type =
                    self.infer_expression(&mut return_statement.expression.borrow_mut());
                if *return_type != value_type.clone().unwrap_or_default() {
                    self.errors.push(format!(
                        "Return type mismatch: expected {return_type:?}, found {value_type:?}"
                    ));
                }
            }
            Statement::Loop(loop_statement) => {
                if let Some(condition) = &mut *loop_statement.condition.borrow_mut() {
                    let condition_type = self.infer_expression(condition);
                    if condition_type.is_none()
                        || condition_type.as_ref().unwrap().kind != TypeInfoKind::Bool
                    {
                        self.errors.push(format!(
                            "Loop condition must be of type `bool`, found {condition_type:?}"
                        ));
                    }
                }
                self.symbol_table.push_scope();
                for stmt in &mut loop_statement.body.statements() {
                    self.infer_statement(stmt, return_type);
                }
                self.symbol_table.pop_scope();
            }
            Statement::Break(_) => {}
            Statement::If(if_statement) => {
                let condition_type =
                    self.infer_expression(&mut if_statement.condition.borrow_mut());
                if condition_type.is_none()
                    || condition_type.as_ref().unwrap().kind != TypeInfoKind::Bool
                {
                    self.errors.push(format!(
                        "If condition must be of type `bool`, found {condition_type:?}"
                    ));
                }

                self.symbol_table.push_scope();
                for stmt in &mut if_statement.if_arm.statements() {
                    self.infer_statement(stmt, return_type);
                }
                self.symbol_table.pop_scope();
                if let Some(else_arm) = &if_statement.else_arm {
                    self.symbol_table.push_scope();
                    for stmt in &mut else_arm.statements() {
                        self.infer_statement(stmt, return_type);
                    }
                    self.symbol_table.pop_scope();
                }
            }
            Statement::VariableDefinition(variable_definition_statement) => {
                let target_type = TypeInfo::new(&variable_definition_statement.ty);
                if let Some(initial_value) = variable_definition_statement.value.as_ref() {
                    // check for Uzumaki initializer
                    let mut expr_ref = initial_value.borrow_mut();
                    if let Expression::Uzumaki(uzumaki_rc) = &mut *expr_ref {
                        *uzumaki_rc.type_info.borrow_mut() = Some(target_type);
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
            }
            Statement::TypeDefinition(type_definition_statement) => {
                let type_name = type_definition_statement.name();
                if let Err(err) = self
                    .symbol_table
                    .register_type(&type_name, Some(&type_definition_statement.ty))
                {
                    self.errors.push(err.to_string());
                }
            }
            Statement::Assert(assert_statement) => {
                let condition_type =
                    self.infer_expression(&mut assert_statement.expression.borrow_mut());
                if condition_type.is_none()
                    || condition_type.as_ref().unwrap().kind != TypeInfoKind::Bool
                {
                    self.errors.push(format!(
                        "If condition must be of type `bool`, found {condition_type:?}"
                    ));
                }
            }
            Statement::ConstantDefinition(constant_definition) => {
                let constant_type = TypeInfo::new(&constant_definition.ty);
                if let Err(err) = self
                    .symbol_table
                    .push_variable_to_scope(constant_definition.name(), constant_type)
                {
                    self.errors.push(err.to_string());
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn infer_expression(&mut self, expression: &mut Expression) -> Option<TypeInfo> {
        match expression {
            Expression::ArrayIndexAccess(array_index_access_expression) => {
                if let Some(type_info) = array_index_access_expression.type_info.borrow().as_ref() {
                    Some(type_info.clone())
                } else if let Some(array_type) =
                    self.infer_expression(&mut array_index_access_expression.array.borrow_mut())
                {
                    if let Some(index_type) =
                        self.infer_expression(&mut array_index_access_expression.index.borrow_mut())
                    {
                        if !index_type.is_number() {
                            self.errors.push(format!(
                                "Array index must be of number type, found {index_type:?}"
                            ));
                        }
                    }
                    if array_type.is_array() {
                        *array_index_access_expression.type_info.borrow_mut() =
                            Some(array_type.clone());
                        Some(array_type.clone())
                    } else {
                        self.errors
                            .push(format!("Expected an array type, found {array_type:?}"));
                        None
                    }
                } else {
                    None
                }
            }
            Expression::MemberAccess(member_access_expression) => {
                if let Some(type_info) = member_access_expression.type_info.borrow().as_ref() {
                    Some(type_info.clone())
                } else if let Some(object_type) =
                    self.infer_expression(&mut member_access_expression.expression.borrow_mut())
                {
                    if object_type.is_struct() {
                        *member_access_expression.type_info.borrow_mut() =
                            Some(object_type.clone());
                        Some(object_type.clone())
                    } else {
                        self.errors.push(format!(
                            "Member access requires a struct type, found {object_type:?}"
                        ));
                        None
                    }
                } else {
                    None
                }
            }
            Expression::FunctionCall(function_call_expression) => {
                let signature = if let Some(s) = self
                    .symbol_table
                    .lookup_function(&function_call_expression.name())
                {
                    s.clone()
                } else {
                    self.errors.push(format!(
                        "Call to undefined function `{}`",
                        function_call_expression.name()
                    ));
                    if let Some(arguments) = &function_call_expression.arguments {
                        for arg in arguments {
                            self.infer_expression(&mut arg.1.borrow_mut());
                        }
                    }
                    return None;
                };
                if let Some(arguments) = &function_call_expression.arguments {
                    if arguments.len() != signature.param_types.len() {
                        self.errors.push(format!(
                            "Function `{}` expects {} arguments, but {} provided",
                            function_call_expression.name(),
                            signature.param_types.len(),
                            arguments.len()
                        ));
                        for arg in arguments {
                            self.infer_expression(&mut arg.1.borrow_mut());
                        }
                        return None;
                    }
                }

                if !signature.type_params.is_empty() {
                    if let Some(type_parameters) = &function_call_expression.type_parameters {
                        if type_parameters.len() != signature.type_params.len() {
                            self.errors.push(format!(
                                "Function `{}` expects {} type parameters, but {} provided",
                                function_call_expression.name(),
                                signature.type_params.len(),
                                type_parameters.len()
                            ));
                        }
                    } else {
                        self.errors.push(format!(
                            "Function `{}` requires {} type parameters, but none were provided",
                            function_call_expression.name(),
                            signature.type_params.len()
                        ));
                    }
                }
                *function_call_expression.type_info.borrow_mut() =
                    Some(signature.return_type.clone());
                Some(signature.return_type.clone())
            }
            Expression::PrefixUnary(prefix_unary_expression) => {
                match prefix_unary_expression.operator {
                    UnaryOperatorKind::Neg => {
                        let expression_type_op = self
                            .infer_expression(&mut prefix_unary_expression.expression.borrow_mut());
                        if let Some(expression_type) = expression_type_op {
                            if expression_type.is_bool() {
                                *prefix_unary_expression.type_info.borrow_mut() =
                                    Some(expression_type.clone());
                                return Some(expression_type);
                            }
                            self.errors.push(format!(
                                "Unary operator `-` can only be applied to numbers, found {expression_type:?}"
                            ));
                        }
                        None
                    }
                }
            }
            Expression::Parenthesized(parenthesized_expression) => {
                self.infer_expression(&mut parenthesized_expression.expression.borrow_mut())
            }
            Expression::Binary(binary_expression) => {
                if let Some(type_info) = binary_expression.type_info.borrow().as_ref() {
                    return Some(type_info.clone());
                }
                let left_type = self.infer_expression(&mut binary_expression.left.borrow_mut());
                let right_type = self.infer_expression(&mut binary_expression.right.borrow_mut());
                if let (Some(left_type), Some(right_type)) = (left_type, right_type) {
                    if left_type != right_type {
                        self.errors.push(format!("Cannot apply operator {:?} to operands of different types: {:?} and {:?}", binary_expression.operator, left_type, right_type));
                    }
                    let res_type = match binary_expression.operator {
                        OperatorKind::And | OperatorKind::Or => {
                            if left_type.is_bool() && right_type.is_bool() {
                                TypeInfo {
                                    kind: TypeInfoKind::Bool,
                                    type_params: vec![],
                                }
                            } else {
                                self.errors.push(format!(
                                    "Logical operator `{:?}` can only be applied to boolean types, found {:?} and {:?}",
                                    binary_expression.operator, left_type, right_type
                                ));
                                return None;
                            }
                        }
                        OperatorKind::Eq
                        | OperatorKind::Ne
                        | OperatorKind::Lt
                        | OperatorKind::Le
                        | OperatorKind::Gt
                        | OperatorKind::Ge => TypeInfo {
                            kind: TypeInfoKind::Bool,
                            type_params: vec![],
                        },
                        OperatorKind::Pow
                        | OperatorKind::Add
                        | OperatorKind::Sub
                        | OperatorKind::Mul
                        | OperatorKind::Div
                        | OperatorKind::Mod
                        | OperatorKind::BitAnd
                        | OperatorKind::BitOr
                        | OperatorKind::BitXor
                        | OperatorKind::BitNot
                        | OperatorKind::Shl
                        | OperatorKind::Shr => {
                            if !left_type.is_number() || !right_type.is_number() {
                                self.errors.push(format!(
                                    "Arithmetic operator `{:?}` can only be applied to number types, found {:?} and {:?}",
                                    binary_expression.operator, left_type, right_type
                                ));
                            }
                            if left_type != right_type {
                                self.errors.push(format!(
                                    "Cannot apply operator `{:?}` to operands of different types: {:?} and {:?}",
                                    binary_expression.operator, left_type, right_type
                                ));
                            }
                            left_type.clone()
                        }
                    };
                    *binary_expression.type_info.borrow_mut() = Some(res_type.clone());
                    Some(left_type)
                } else {
                    None
                }
            }
            Expression::Literal(literal) => match literal {
                Literal::Array(array_literal) => {
                    if array_literal.type_info.borrow().is_some() {
                        return Some(array_literal.type_info.borrow().clone().unwrap());
                    }
                    if let Some(element_type_info) =
                        self.infer_expression(&mut array_literal.elements[0].borrow_mut())
                    {
                        for element in &array_literal.elements[1..] {
                            let element_type = self.infer_expression(&mut element.borrow_mut());
                            if let Some(element_type) = element_type {
                                if element_type != element_type_info {
                                    self.errors.push(format!(
                                        "Array elements must be of the same type, found {element_type:?} and {element_type_info:?}"
                                    ));
                                }
                            }
                        }
                    } else {
                        self.errors
                            .push("Array elements must be of the same type".to_string());
                    }
                    None
                }
                Literal::Bool(_) => Some(TypeInfo {
                    kind: TypeInfoKind::Bool,
                    type_params: vec![],
                }),
                Literal::String(_) => Some(TypeInfo {
                    kind: TypeInfoKind::String,
                    type_params: vec![],
                }),
                Literal::Number(number_literal) => {
                    if number_literal.type_info.borrow().is_some() {
                        return Some(number_literal.type_info.borrow().clone().unwrap());
                    }
                    let res_type = TypeInfo {
                        kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
                        type_params: vec![],
                    };
                    *number_literal.type_info.borrow_mut() = Some(res_type.clone());
                    Some(res_type)
                }
                Literal::Unit(_) => Some(TypeInfo {
                    kind: TypeInfoKind::Unit,
                    type_params: vec![],
                }),
            },
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
            Expression::Type(type_expr) => Some(TypeInfo::new(type_expr)),
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
