use std::rc::Rc;

use anyhow::bail;
use inference_ast::nodes::{
    ArgumentType, Definition, Expression, FunctionDefinition, Identifier, Literal, Location,
    ModuleDefinition, OperatorKind, SimpleType, Statement, Type, UnaryOperatorKind, UseDirective,
    Visibility,
};

use crate::{
    symbol_table::SymbolTable,
    type_info::{NumberTypeKindNumberType, TypeInfo, TypeInfoKind},
    typed_context::TypedContext,
};

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

    pub fn infer_types(&mut self, ctx: &mut TypedContext) -> anyhow::Result<SymbolTable> {
        self.register_types(ctx);
        self.collect_function_and_constant_definitions(ctx);
        if !self.errors.is_empty() {
            bail!(std::mem::take(&mut self.errors).join("; ")) //TODO: handle it better
        }
        // Infer types for each function in each source file
        for source_file in ctx.source_files() {
            // Directly iterate over definitions to ensure we operate on the actual AST nodes
            for def in &source_file.definitions {
                if let Definition::Function(function_definition) = def {
                    // Clone the Rc to share the underlying FunctionDefinition
                    self.infer_variables(function_definition.clone(), ctx);
                }
            }
        }
        if !self.errors.is_empty() {
            bail!(std::mem::take(&mut self.errors).join("; ")) //TODO: handle it better
        }
        Ok(self.symbol_table.clone())
    }

    fn register_types(&mut self, ctx: &mut TypedContext) {
        for source_file in ctx.source_files() {
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
    fn collect_function_and_constant_definitions(&mut self, ctx: &mut TypedContext) {
        for sf in ctx.source_files() {
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
    fn infer_variables(
        &mut self,
        function_definition: Rc<FunctionDefinition>,
        ctx: &mut TypedContext,
    ) {
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
                ctx,
            );
        }
        self.symbol_table.pop_scope();
    }

    #[allow(clippy::too_many_lines)]
    fn infer_statement(
        &mut self,
        statement: &mut Statement,
        return_type: &TypeInfo,
        ctx: &mut TypedContext,
    ) {
        match statement {
            Statement::Assign(assign_statement) => {
                let target_type =
                    self.infer_expression(&mut assign_statement.left.borrow_mut(), ctx);
                let mut right_expr = assign_statement.right.borrow_mut();
                if let Expression::Uzumaki(uzumaki_rc) = &*right_expr {
                    if let Some(target) = &target_type {
                        ctx.set_node_typeinfo(uzumaki_rc.id, target.clone());
                    } else {
                        self.errors.push(
                            String::from("Cannot infer type for Uzumaki expression assigned to variable of unknown type")
                        );
                    }
                } else {
                    let value_type = self.infer_expression(&mut right_expr, ctx);
                    if let (Some(target), Some(val)) = (target_type, value_type)
                        && target != val
                    {
                        self.errors.push(format!(
                            "Cannot assign value of type {val:?} to variable of type {target:?}"
                        ));
                    }
                }
            }
            Statement::Block(block_type) => {
                self.symbol_table.push_scope();
                for stmt in &mut block_type.statements() {
                    self.infer_statement(stmt, return_type, ctx);
                }
                self.symbol_table.pop_scope();
            }
            Statement::Expression(expression) => {
                self.infer_expression(expression, ctx);
            }
            Statement::Return(return_statement) => {
                if matches!(
                    &*return_statement.expression.borrow(),
                    Expression::Uzumaki(_)
                ) {
                    ctx.set_node_typeinfo(
                        return_statement.expression.borrow().id(),
                        return_type.clone(),
                    );
                } else {
                    let value_type =
                        self.infer_expression(&mut return_statement.expression.borrow_mut(), ctx);
                    if *return_type != value_type.clone().unwrap_or_default() {
                        self.errors.push(format!(
                            "Return type mismatch: expected {return_type:?}, found {value_type:?}"
                        ));
                    }
                }
            }
            Statement::Loop(loop_statement) => {
                if let Some(condition) = &mut *loop_statement.condition.borrow_mut() {
                    let condition_type = self.infer_expression(condition, ctx);
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
                    self.infer_statement(stmt, return_type, ctx);
                }
                self.symbol_table.pop_scope();
            }
            Statement::Break(_) => {}
            Statement::If(if_statement) => {
                let condition_type =
                    self.infer_expression(&mut if_statement.condition.borrow_mut(), ctx);
                if condition_type.is_none()
                    || condition_type.as_ref().unwrap().kind != TypeInfoKind::Bool
                {
                    self.errors.push(format!(
                        "If condition must be of type `bool`, found {condition_type:?}"
                    ));
                }

                self.symbol_table.push_scope();
                for stmt in &mut if_statement.if_arm.statements() {
                    self.infer_statement(stmt, return_type, ctx);
                }
                self.symbol_table.pop_scope();
                if let Some(else_arm) = &if_statement.else_arm {
                    self.symbol_table.push_scope();
                    for stmt in &mut else_arm.statements() {
                        self.infer_statement(stmt, return_type, ctx);
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
                        ctx.set_node_typeinfo(uzumaki_rc.id, target_type.clone());
                    } else if let Some(init_type) = self.infer_expression(&mut expr_ref, ctx)
                        && init_type != TypeInfo::new(&variable_definition_statement.ty)
                    {
                        self.errors.push(format!(
                            "Type mismatch in variable definition: expected {:?}, found {:?}",
                            variable_definition_statement.ty, init_type
                        ));
                    }
                }
                if let Err(err) = self.symbol_table.push_variable_to_scope(
                    variable_definition_statement.name(),
                    TypeInfo::new(&variable_definition_statement.ty),
                ) {
                    self.errors.push(err.to_string());
                }
                ctx.set_node_typeinfo(variable_definition_statement.id, target_type);
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
                    self.infer_expression(&mut assert_statement.expression.borrow_mut(), ctx);
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
                    .push_variable_to_scope(constant_definition.name(), constant_type.clone())
                {
                    self.errors.push(err.to_string());
                }
                ctx.set_node_typeinfo(constant_definition.id, constant_type);
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn infer_expression(
        &mut self,
        expression: &mut Expression,
        ctx: &mut TypedContext,
    ) -> Option<TypeInfo> {
        match expression {
            Expression::ArrayIndexAccess(array_index_access_expression) => {
                if let Some(type_info) = ctx.get_node_typeinfo(array_index_access_expression.id) {
                    Some(type_info.clone())
                } else if let Some(array_type) = self
                    .infer_expression(&mut array_index_access_expression.array.borrow_mut(), ctx)
                {
                    if let Some(index_type) = self.infer_expression(
                        &mut array_index_access_expression.index.borrow_mut(),
                        ctx,
                    ) && !index_type.is_number()
                    {
                        self.errors.push(format!(
                            "Array index must be of number type, found {index_type:?}"
                        ));
                    }
                    match &array_type.kind {
                        TypeInfoKind::Array(element_type, _) => {
                            ctx.set_node_typeinfo(
                                array_index_access_expression.id,
                                (**element_type).clone(),
                            );
                            Some((**element_type).clone())
                        }
                        _ => {
                            self.errors
                                .push(format!("Expected an array type, found {array_type:?}"));
                            None
                        }
                    }
                } else {
                    None
                }
            }
            Expression::MemberAccess(member_access_expression) => {
                if let Some(type_info) = ctx.get_node_typeinfo(member_access_expression.id) {
                    Some(type_info.clone())
                } else if let Some(object_type) = self
                    .infer_expression(&mut member_access_expression.expression.borrow_mut(), ctx)
                {
                    if object_type.is_struct() {
                        ctx.set_node_typeinfo(member_access_expression.id, object_type.clone());
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
            Expression::TypeMemberAccess(type_member_access_expression) => {
                if let Some(type_info) = ctx.get_node_typeinfo(type_member_access_expression.id) {
                    Some(type_info.clone())
                } else if let Some(type_expression_type) = self.infer_expression(
                    &mut type_member_access_expression.expression.borrow_mut(),
                    ctx,
                ) {
                    // Here we would normally check if the member exists in the type
                    // For simplicity, we assume it does and return the type expression's type
                    ctx.set_node_typeinfo(
                        type_member_access_expression.id,
                        type_expression_type.clone(),
                    );
                    Some(type_expression_type)
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
                            self.infer_expression(&mut arg.1.borrow_mut(), ctx);
                        }
                    }
                    return None;
                };
                if let Some(arguments) = &function_call_expression.arguments
                    && arguments.len() != signature.param_types.len()
                {
                    self.errors.push(format!(
                        "Function `{}` expects {} arguments, but {} provided",
                        function_call_expression.name(),
                        signature.param_types.len(),
                        arguments.len()
                    ));
                    for arg in arguments {
                        self.infer_expression(&mut arg.1.borrow_mut(), ctx);
                    }
                    return None;
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
                ctx.set_node_typeinfo(function_call_expression.id, signature.return_type.clone());
                Some(signature.return_type.clone())
            }
            Expression::Struct(struct_expression) => {
                if let Some(type_info) = ctx.get_node_typeinfo(struct_expression.id) {
                    return Some(type_info.clone());
                }
                let struct_type: Option<TypeInfo> = self
                    .symbol_table
                    .lookup_type(&struct_expression.name())
                    .cloned();
                if let Some(struct_type) = struct_type {
                    ctx.set_node_typeinfo(struct_expression.id, struct_type.clone());
                    return Some(struct_type);
                }
                self.errors.push(format!(
                    "Struct `{}` is not defined",
                    struct_expression.name()
                ));
                None
            }
            Expression::PrefixUnary(prefix_unary_expression) => {
                match prefix_unary_expression.operator {
                    UnaryOperatorKind::Neg => {
                        let expression_type_op = self.infer_expression(
                            &mut prefix_unary_expression.expression.borrow_mut(),
                            ctx,
                        );
                        if let Some(expression_type) = expression_type_op {
                            if expression_type.is_bool() {
                                ctx.set_node_typeinfo(
                                    prefix_unary_expression.id,
                                    expression_type.clone(),
                                );
                                return Some(expression_type);
                            }
                            self.errors.push(format!(
                                "Unary operator `-` can only be applied to booleans, found {expression_type:?}"
                            ));
                        }
                        None
                    }
                }
            }
            Expression::Parenthesized(parenthesized_expression) => {
                self.infer_expression(&mut parenthesized_expression.expression.borrow_mut(), ctx)
            }
            Expression::Binary(binary_expression) => {
                if let Some(type_info) = ctx.get_node_typeinfo(binary_expression.id) {
                    return Some(type_info.clone());
                }
                let left_type =
                    self.infer_expression(&mut binary_expression.left.borrow_mut(), ctx);
                let right_type =
                    self.infer_expression(&mut binary_expression.right.borrow_mut(), ctx);
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
                    ctx.set_node_typeinfo(binary_expression.id, res_type.clone());
                    Some(res_type)
                } else {
                    None
                }
            }
            Expression::Literal(literal) => match literal {
                Literal::Array(array_literal) => {
                    if ctx.get_node_typeinfo(array_literal.id).is_some() {
                        return ctx.get_node_typeinfo(array_literal.id);
                    }
                    if let Some(elements) = &array_literal.elements {
                        if let Some(element_type_info) =
                            self.infer_expression(&mut elements[0].borrow_mut(), ctx)
                        {
                            for element in &elements[1..] {
                                let element_type =
                                    self.infer_expression(&mut element.borrow_mut(), ctx);
                                if let Some(element_type) = element_type
                                    && element_type != element_type_info
                                {
                                    self.errors.push(format!(
                                        "Array elements must be of the same type, found {element_type:?} and {element_type_info:?}"
                                    ));
                                }
                            }
                        } else {
                            self.errors
                                .push("Array elements must be of the same type".to_string());
                        }
                    }

                    None
                }
                Literal::Bool(_) => {
                    ctx.set_node_typeinfo(literal.id(), TypeInfo::boolean());
                    Some(TypeInfo::boolean())
                }
                Literal::String(sl) => {
                    ctx.set_node_typeinfo(sl.id, TypeInfo::string());
                    Some(TypeInfo::string())
                }
                Literal::Number(number_literal) => {
                    if ctx.get_node_typeinfo(number_literal.id).is_some() {
                        return ctx.get_node_typeinfo(number_literal.id);
                    }
                    let res_type = TypeInfo {
                        kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
                        type_params: vec![],
                    };
                    ctx.set_node_typeinfo(number_literal.id, res_type.clone());
                    Some(res_type)
                }
                Literal::Unit(_) => {
                    ctx.set_node_typeinfo(literal.id(), TypeInfo::default());
                    Some(TypeInfo::default())
                }
            },
            Expression::Identifier(identifier) => {
                if let Some(var_ty) = self.symbol_table.lookup_variable(&identifier.name) {
                    ctx.set_node_typeinfo(identifier.id, var_ty.clone());
                    Some(var_ty)
                } else {
                    self.errors
                        .push(format!("Use of undeclared variable `{}`", identifier.name));
                    None
                }
            }
            Expression::Type(type_expr) => Some(TypeInfo::new(type_expr)),
            Expression::Uzumaki(uzumaki) => ctx.get_node_typeinfo(uzumaki.id),
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
                if left_has_return_type
                    && let (Some(left_return_type), Some(right_return_type)) =
                        (&left.returns, &right.returns)
                    && !Self::types_equal(left_return_type, right_return_type)
                {
                    return false;
                }
                let left_has_parameters = left.parameters.is_some();
                let right_has_parameters = right.parameters.is_some();
                if left_has_parameters != right_has_parameters {
                    return false;
                }
                if left_has_parameters
                    && let (Some(left_parameters), Some(right_parameters)) =
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
                true
            }
            _ => false,
        }
    }

    /// Process a module definition
    /// TODO: Implement in Phase 1 when scope tree is ready
    #[allow(dead_code)]
    fn process_module_definition(
        &mut self,
        _module: &Rc<ModuleDefinition>,
        _ctx: &mut TypedContext,
    ) -> anyhow::Result<()> {
        // TODO: Implement me - requires scope tree infrastructure (Phase 1)
        Ok(())
    }

    /// Process a use statement
    /// TODO: Implement in Phase 4 when import system is ready
    #[allow(dead_code)]
    fn process_use_statement(
        &mut self,
        _use_stmt: &Rc<UseDirective>,
        _ctx: &mut TypedContext,
    ) -> anyhow::Result<()> {
        // TODO: Implement me - requires import system (Phase 4)
        Ok(())
    }

    /// Check visibility of a definition from current scope
    /// TODO: Implement in Phase 4 when visibility checking is ready
    #[allow(dead_code)]
    fn check_visibility(
        &self,
        _visibility: &Visibility,
        _definition_scope: u32,
        _access_scope: u32,
    ) -> bool {
        // TODO: Implement me - requires scope tree (Phase 1) and visibility rules (Phase 4)
        // For now, everything is visible
        true
    }
}
