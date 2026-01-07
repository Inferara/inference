use std::rc::Rc;

use anyhow::bail;
use inference_ast::extern_prelude::ExternPrelude;
use inference_ast::nodes::{
    ArgumentType, Definition, Directive, Expression, FunctionDefinition, Identifier, Literal,
    Location, ModuleDefinition, OperatorKind, SimpleType, Statement, Type, UnaryOperatorKind,
    UseDirective, Visibility,
};
use rustc_hash::FxHashSet;

use crate::{
    errors::{RegistrationKind, TypeCheckError, TypeMismatchContext},
    symbol_table::{FuncSignature, Import, ImportItem, ImportKind, ResolvedImport, SymbolTable},
    type_info::{NumberTypeKindNumberType, TypeInfo, TypeInfoKind},
    typed_context::TypedContext,
};

#[derive(Default)]
pub(crate) struct TypeChecker {
    symbol_table: SymbolTable,
    errors: Vec<TypeCheckError>,
    glob_resolution_in_progress: FxHashSet<u32>,
}

impl TypeChecker {
    /// Load external modules from prelude before import resolution.
    ///
    /// The prelude is consumed (moved into symbol table as virtual scopes).
    /// Call this before `infer_types()` to make external modules available.
    ///
    /// # Arguments
    /// * `prelude` - The external prelude containing parsed external modules
    ///
    /// # Errors
    /// Returns an error if symbol registration for any module fails
    #[allow(dead_code)]
    pub fn load_prelude(&mut self, prelude: ExternPrelude) -> anyhow::Result<()> {
        for (name, parsed_module) in prelude {
            self.symbol_table
                .load_external_module(&name, &parsed_module.arena)?;
        }
        Ok(())
    }
}

impl TypeChecker {
    /// Infer types for all definitions in the context.
    ///
    /// Phase ordering:
    /// 1. `process_directives()` - Register raw imports in scopes
    /// 2. `register_types()` - Collect type definitions into symbol table
    /// 3. `resolve_imports()` - Bind import paths to symbols
    /// 4. `collect_function_and_constant_definitions()` - Register functions
    /// 5. Infer variable types in function bodies
    pub fn infer_types(&mut self, ctx: &mut TypedContext) -> anyhow::Result<SymbolTable> {
        self.process_directives(ctx);
        self.register_types(ctx);
        self.resolve_imports();
        self.collect_function_and_constant_definitions(ctx);
        if !self.errors.is_empty() {
            let error_messages: Vec<String> = std::mem::take(&mut self.errors)
                .into_iter()
                .map(|e| e.to_string())
                .collect();
            bail!(error_messages.join("; "))
        }
        for source_file in ctx.source_files() {
            for def in &source_file.definitions {
                match def {
                    Definition::Function(function_definition) => {
                        self.infer_variables(function_definition.clone(), ctx);
                    }
                    Definition::Struct(struct_definition) => {
                        let struct_type = TypeInfo {
                            kind: TypeInfoKind::Struct(struct_definition.name()),
                            type_params: vec![],
                        };
                        for method in &struct_definition.methods {
                            self.infer_method_variables(method.clone(), struct_type.clone(), ctx);
                        }
                    }
                    _ => {}
                }
            }
        }
        if !self.errors.is_empty() {
            let error_messages: Vec<String> = std::mem::take(&mut self.errors)
                .into_iter()
                .map(|e| e.to_string())
                .collect();
            bail!(error_messages.join("; "))
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
                                self.errors.push(TypeCheckError::RegistrationFailed {
                                    kind: RegistrationKind::Type,
                                    name: type_definition.name(),
                                    reason: None,
                                    location: None,
                                });
                            });
                    }
                    Definition::Struct(struct_definition) => {
                        let fields: Vec<(String, TypeInfo, Visibility)> = struct_definition
                            .fields
                            .iter()
                            .map(|f| {
                                (
                                    f.name.name.clone(),
                                    TypeInfo::new(&f.type_),
                                    Visibility::Private,
                                )
                            })
                            .collect();
                        self.symbol_table
                            .register_struct(
                                &struct_definition.name(),
                                &fields,
                                vec![],
                                struct_definition.visibility.clone(),
                            )
                            .unwrap_or_else(|_| {
                                self.errors.push(TypeCheckError::RegistrationFailed {
                                    kind: RegistrationKind::Struct,
                                    name: struct_definition.name(),
                                    reason: None,
                                    location: None,
                                });
                            });

                        let struct_name = struct_definition.name();
                        for method in &struct_definition.methods {
                            let has_self = method.arguments.as_ref().is_some_and(|args| {
                                args.iter()
                                    .any(|arg| matches!(arg, ArgumentType::SelfReference(_)))
                            });

                            let param_types: Vec<TypeInfo> = method
                                .arguments
                                .as_ref()
                                .unwrap_or(&vec![])
                                .iter()
                                .filter_map(|param| match param {
                                    ArgumentType::SelfReference(_) => None,
                                    ArgumentType::IgnoreArgument(ignore_arg) => {
                                        Some(TypeInfo::new(&ignore_arg.ty))
                                    }
                                    ArgumentType::Argument(arg) => Some(TypeInfo::new(&arg.ty)),
                                    ArgumentType::Type(ty) => Some(TypeInfo::new(ty)),
                                })
                                .collect();

                            let return_type = method
                                .returns
                                .as_ref()
                                .map(TypeInfo::new)
                                .unwrap_or_default();

                            let type_params: Vec<String> = method
                                .type_parameters
                                .as_ref()
                                .unwrap_or(&vec![])
                                .iter()
                                .map(|p| p.name())
                                .collect();

                            let signature = FuncSignature {
                                name: method.name(),
                                type_params,
                                param_types,
                                return_type,
                                visibility: method.visibility.clone(),
                            };

                            self.symbol_table
                                .register_method(
                                    &struct_name,
                                    signature,
                                    method.visibility.clone(),
                                    has_self,
                                )
                                .unwrap_or_else(|err| {
                                    self.errors.push(TypeCheckError::RegistrationFailed {
                                        kind: RegistrationKind::Method,
                                        name: format!("{struct_name}::{}", method.name()),
                                        reason: Some(err.to_string()),
                                        location: None,
                                    });
                                });
                        }
                    }
                    Definition::Enum(enum_definition) => {
                        let variants: Vec<&str> = enum_definition
                            .variants
                            .iter()
                            .map(|v| v.name.as_str())
                            .collect();
                        self.symbol_table
                            .register_enum(
                                &enum_definition.name(),
                                &variants,
                                enum_definition.visibility.clone(),
                            )
                            .unwrap_or_else(|_| {
                                self.errors.push(TypeCheckError::RegistrationFailed {
                                    kind: RegistrationKind::Enum,
                                    name: enum_definition.name(),
                                    reason: None,
                                    location: None,
                                });
                            });
                    }
                    Definition::Spec(spec_definition) => {
                        self.symbol_table
                            .register_spec(&spec_definition.name())
                            .unwrap_or_else(|_| {
                                self.errors.push(TypeCheckError::RegistrationFailed {
                                    kind: RegistrationKind::Spec,
                                    name: spec_definition.name(),
                                    reason: None,
                                    location: None,
                                });
                            });
                    }
                    Definition::Constant(_)
                    | Definition::Function(_)
                    | Definition::ExternalFunction(_)
                    | Definition::Module(_) => {}
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
                            &constant_definition.name(),
                            TypeInfo::new(&constant_definition.ty),
                        ) {
                            self.errors.push(TypeCheckError::RegistrationFailed {
                                kind: RegistrationKind::Variable,
                                name: constant_definition.name(),
                                reason: Some(err.to_string()),
                                location: None,
                            });
                        }
                    }
                    Definition::Function(function_definition) => {
                        for param in function_definition.arguments.as_ref().unwrap_or(&vec![]) {
                            match param {
                                ArgumentType::SelfReference(_) => {
                                    self.errors.push(TypeCheckError::SelfReferenceInFunction {
                                        function_name: function_definition.name(),
                                        location: None,
                                    });
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
                                .filter_map(|param| match param {
                                    ArgumentType::SelfReference(_) => None,
                                    ArgumentType::IgnoreArgument(ignore_argument) => {
                                        Some(ignore_argument.ty.clone())
                                    }
                                    ArgumentType::Argument(argument) => Some(argument.ty.clone()),
                                    ArgumentType::Type(ty) => Some(ty.clone()),
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
                            self.errors.push(TypeCheckError::RegistrationFailed {
                                kind: RegistrationKind::Function,
                                name: function_definition.name(),
                                reason: Some(err),
                                location: None,
                            });
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
                                .filter_map(|param| match param {
                                    ArgumentType::SelfReference(_) => None,
                                    ArgumentType::IgnoreArgument(ignore_argument) => {
                                        Some(ignore_argument.ty.clone())
                                    }
                                    ArgumentType::Argument(argument) => Some(argument.ty.clone()),
                                    ArgumentType::Type(ty) => Some(ty.clone()),
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
                            self.errors.push(TypeCheckError::RegistrationFailed {
                                kind: RegistrationKind::Function,
                                name: external_function_definition.name(),
                                reason: Some(err),
                                location: None,
                            });
                        }
                    }
                    Definition::Spec(_)
                    | Definition::Struct(_)
                    | Definition::Enum(_)
                    | Definition::Type(_)
                    | Definition::Module(_) => {}
                }
            }
        }
    }

    fn validate_type(&mut self, ty: &Type, type_parameters: Option<&Vec<Rc<Identifier>>>) {
        match ty {
            Type::Array(type_array) => self.validate_type(&type_array.element_type, None),
            Type::Simple(simple_type) => {
                if self.symbol_table.lookup_type(&simple_type.name).is_none() {
                    self.errors.push(TypeCheckError::UnknownType {
                        name: simple_type.name.clone(),
                        location: None,
                    });
                }
            }
            Type::Generic(generic_type) => {
                if self
                    .symbol_table
                    .lookup_type(&generic_type.base.name())
                    .is_none()
                {
                    self.errors.push(TypeCheckError::UnknownType {
                        name: generic_type.base.name(),
                        location: None,
                    });
                }
                if let Some(type_params) = &type_parameters {
                    if type_params.len() != generic_type.parameters.len() {
                        self.errors
                            .push(TypeCheckError::TypeParameterCountMismatch {
                                name: generic_type.base.name(),
                                expected: generic_type.parameters.len(),
                                found: type_params.len(),
                                location: None,
                            });
                    }
                    let generic_param_names: Vec<String> = generic_type
                        .parameters
                        .iter()
                        .map(|param| param.name())
                        .collect();
                    for param in &generic_type.parameters {
                        if !generic_param_names.contains(&param.name()) {
                            self.errors.push(TypeCheckError::General(format!(
                                "type parameter `{}` not found in `{}`",
                                param.name(),
                                generic_type.base.name()
                            )));
                        }
                    }
                }
            }
            Type::Function(_) | Type::QualifiedName(_) | Type::Qualified(_) => {}
            Type::Custom(identifier) => {
                if self.symbol_table.lookup_type(&identifier.name).is_none() {
                    self.errors.push(TypeCheckError::UnknownType {
                        name: identifier.name.clone(),
                        location: None,
                    });
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
                            .push_variable_to_scope(&arg.name(), TypeInfo::new(&arg.ty))
                        {
                            self.errors.push(TypeCheckError::RegistrationFailed {
                                kind: RegistrationKind::Variable,
                                name: arg.name(),
                                reason: Some(err.to_string()),
                                location: None,
                            });
                        }
                    }
                    ArgumentType::SelfReference(_) => {
                        self.errors
                            .push(TypeCheckError::SelfReferenceOutsideMethod { location: None });
                    }
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

    #[allow(clippy::needless_pass_by_value)]
    fn infer_method_variables(
        &mut self,
        method_definition: Rc<FunctionDefinition>,
        self_type: TypeInfo,
        ctx: &mut TypedContext,
    ) {
        self.symbol_table.push_scope();
        if let Some(arguments) = &method_definition.arguments {
            for argument in arguments {
                match argument {
                    ArgumentType::Argument(arg) => {
                        if let Err(err) = self
                            .symbol_table
                            .push_variable_to_scope(&arg.name(), TypeInfo::new(&arg.ty))
                        {
                            self.errors.push(TypeCheckError::RegistrationFailed {
                                kind: RegistrationKind::Variable,
                                name: arg.name(),
                                reason: Some(err.to_string()),
                                location: None,
                            });
                        }
                    }
                    ArgumentType::SelfReference(_) => {
                        if let Err(err) = self
                            .symbol_table
                            .push_variable_to_scope("self", self_type.clone())
                        {
                            self.errors.push(TypeCheckError::RegistrationFailed {
                                kind: RegistrationKind::Variable,
                                name: "self".to_string(),
                                reason: Some(err.to_string()),
                                location: None,
                            });
                        }
                    }
                    ArgumentType::IgnoreArgument(_) | ArgumentType::Type(_) => {}
                }
            }
        }
        for stmt in &mut method_definition.body.statements() {
            self.infer_statement(
                stmt,
                &method_definition
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
                        self.errors
                            .push(TypeCheckError::CannotInferUzumakiType { location: None });
                    }
                } else {
                    let value_type = self.infer_expression(&mut right_expr, ctx);
                    if let (Some(target), Some(val)) = (target_type, value_type)
                        && target != val
                    {
                        self.errors.push(TypeCheckError::TypeMismatch {
                            expected: target,
                            found: val,
                            context: TypeMismatchContext::Assignment,
                            location: None,
                        });
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
                        self.errors.push(TypeCheckError::TypeMismatch {
                            expected: return_type.clone(),
                            found: value_type.unwrap_or_default(),
                            context: TypeMismatchContext::Return,
                            location: None,
                        });
                    }
                }
            }
            Statement::Loop(loop_statement) => {
                if let Some(condition) = &mut *loop_statement.condition.borrow_mut() {
                    let condition_type = self.infer_expression(condition, ctx);
                    if condition_type.is_none()
                        || condition_type.as_ref().unwrap().kind != TypeInfoKind::Bool
                    {
                        self.errors.push(TypeCheckError::TypeMismatch {
                            expected: TypeInfo::boolean(),
                            found: condition_type.unwrap_or_default(),
                            context: TypeMismatchContext::Condition,
                            location: None,
                        });
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
                    self.errors.push(TypeCheckError::TypeMismatch {
                        expected: TypeInfo::boolean(),
                        found: condition_type.unwrap_or_default(),
                        context: TypeMismatchContext::Condition,
                        location: None,
                    });
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
                    let mut expr_ref = initial_value.borrow_mut();
                    if let Expression::Uzumaki(uzumaki_rc) = &mut *expr_ref {
                        ctx.set_node_typeinfo(uzumaki_rc.id, target_type.clone());
                    } else if let Some(init_type) = self.infer_expression(&mut expr_ref, ctx)
                        && init_type != TypeInfo::new(&variable_definition_statement.ty)
                    {
                        self.errors.push(TypeCheckError::TypeMismatch {
                            expected: target_type.clone(),
                            found: init_type,
                            context: TypeMismatchContext::VariableDefinition,
                            location: None,
                        });
                    }
                }
                if let Err(err) = self.symbol_table.push_variable_to_scope(
                    &variable_definition_statement.name(),
                    TypeInfo::new(&variable_definition_statement.ty),
                ) {
                    self.errors.push(TypeCheckError::RegistrationFailed {
                        kind: RegistrationKind::Variable,
                        name: variable_definition_statement.name(),
                        reason: Some(err.to_string()),
                        location: None,
                    });
                }
                ctx.set_node_typeinfo(variable_definition_statement.id, target_type);
            }
            Statement::TypeDefinition(type_definition_statement) => {
                let type_name = type_definition_statement.name();
                if let Err(err) = self
                    .symbol_table
                    .register_type(&type_name, Some(&type_definition_statement.ty))
                {
                    self.errors.push(TypeCheckError::RegistrationFailed {
                        kind: RegistrationKind::Type,
                        name: type_name,
                        reason: Some(err.to_string()),
                        location: None,
                    });
                }
            }
            Statement::Assert(assert_statement) => {
                let condition_type =
                    self.infer_expression(&mut assert_statement.expression.borrow_mut(), ctx);
                if condition_type.is_none()
                    || condition_type.as_ref().unwrap().kind != TypeInfoKind::Bool
                {
                    self.errors.push(TypeCheckError::TypeMismatch {
                        expected: TypeInfo::boolean(),
                        found: condition_type.unwrap_or_default(),
                        context: TypeMismatchContext::Condition,
                        location: None,
                    });
                }
            }
            Statement::ConstantDefinition(constant_definition) => {
                let constant_type = TypeInfo::new(&constant_definition.ty);
                if let Err(err) = self
                    .symbol_table
                    .push_variable_to_scope(&constant_definition.name(), constant_type.clone())
                {
                    self.errors.push(TypeCheckError::RegistrationFailed {
                        kind: RegistrationKind::Variable,
                        name: constant_definition.name(),
                        reason: Some(err.to_string()),
                        location: None,
                    });
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
                        self.errors.push(TypeCheckError::ArrayIndexNotNumeric {
                            found: index_type,
                            location: None,
                        });
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
                            self.errors.push(TypeCheckError::ExpectedArrayType {
                                found: array_type,
                                location: None,
                            });
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
                    let struct_name = match &object_type.kind {
                        TypeInfoKind::Struct(name) => Some(name.clone()),
                        TypeInfoKind::Custom(name) => {
                            if self.symbol_table.lookup_struct(name).is_some() {
                                Some(name.clone())
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };

                    if let Some(struct_name) = struct_name {
                        let field_name = &member_access_expression.name.name;
                        if let Some(field_type) = self
                            .symbol_table
                            .lookup_struct_field(&struct_name, field_name)
                        {
                            ctx.set_node_typeinfo(member_access_expression.id, field_type.clone());
                            Some(field_type)
                        } else {
                            self.errors.push(TypeCheckError::FieldNotFound {
                                struct_name,
                                field_name: field_name.clone(),
                                location: None,
                            });
                            None
                        }
                    } else {
                        self.errors.push(TypeCheckError::ExpectedStructType {
                            found: object_type,
                            location: None,
                        });
                        None
                    }
                } else {
                    None
                }
            }
            Expression::TypeMemberAccess(type_member_access_expression) => {
                if let Some(type_info) = ctx.get_node_typeinfo(type_member_access_expression.id) {
                    return Some(type_info.clone());
                }

                let inner_expr = type_member_access_expression.expression.borrow();

                // Extract enum name from the expression - handle Type enum properly
                let enum_name = match &*inner_expr {
                    Expression::Type(ty) => {
                        // Type enum does NOT have a .name() method - must match variants
                        match ty {
                            Type::Simple(simple_type) => simple_type.name.clone(),
                            Type::Custom(ident) => ident.name.clone(),
                            _ => {
                                // Array, Generic, Function, QualifiedName, Qualified are not valid for enum access
                                self.errors.push(TypeCheckError::ExpectedEnumType {
                                    found: TypeInfo::new(ty),
                                    location: None,
                                });
                                return None;
                            }
                        }
                    }
                    Expression::Identifier(id) => id.name.clone(),
                    _ => {
                        // For other expressions, try to infer the type
                        drop(inner_expr); // Release borrow before mutable borrow
                        if let Some(expr_type) = self.infer_expression(
                            &mut type_member_access_expression.expression.borrow_mut(),
                            ctx,
                        ) {
                            match &expr_type.kind {
                                TypeInfoKind::Enum(name) => name.clone(),
                                _ => {
                                    self.errors.push(TypeCheckError::ExpectedEnumType {
                                        found: expr_type,
                                        location: None,
                                    });
                                    return None;
                                }
                            }
                        } else {
                            return None;
                        }
                    }
                };

                let variant_name = &type_member_access_expression.name.name;

                // Look up the enum and validate variant
                if let Some(enum_info) = self.symbol_table.lookup_enum(&enum_name) {
                    if enum_info.variants.contains(variant_name) {
                        let enum_type = TypeInfo {
                            kind: TypeInfoKind::Enum(enum_name),
                            type_params: vec![],
                        };
                        ctx.set_node_typeinfo(type_member_access_expression.id, enum_type.clone());
                        Some(enum_type)
                    } else {
                        self.errors.push(TypeCheckError::VariantNotFound {
                            enum_name,
                            variant_name: variant_name.clone(),
                            location: None,
                        });
                        None
                    }
                } else {
                    self.errors.push(TypeCheckError::UndefinedEnum {
                        name: enum_name,
                        location: None,
                    });
                    None
                }
            }
            Expression::FunctionCall(function_call_expression) => {
                if let Expression::MemberAccess(member_access) = &function_call_expression.function
                {
                    let receiver_type =
                        self.infer_expression(&mut member_access.expression.borrow_mut(), ctx);

                    if let Some(receiver_type) = receiver_type {
                        let type_name = match &receiver_type.kind {
                            TypeInfoKind::Struct(name) => Some(name.clone()),
                            TypeInfoKind::Custom(name) => {
                                if self.symbol_table.lookup_struct(name).is_some() {
                                    Some(name.clone())
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        };

                        if let Some(type_name) = type_name {
                            let method_name = &member_access.name.name;
                            if let Some(method_info) =
                                self.symbol_table.lookup_method(&type_name, method_name)
                            {
                                let signature = &method_info.signature;
                                let arg_count = function_call_expression
                                    .arguments
                                    .as_ref()
                                    .map_or(0, Vec::len);

                                if arg_count != signature.param_types.len() {
                                    self.errors.push(TypeCheckError::ArgumentCountMismatch {
                                        kind: "method",
                                        name: format!("{}::{}", type_name, method_name),
                                        expected: signature.param_types.len(),
                                        found: arg_count,
                                        location: None,
                                    });
                                }

                                if let Some(arguments) = &function_call_expression.arguments {
                                    for arg in arguments {
                                        self.infer_expression(&mut arg.1.borrow_mut(), ctx);
                                    }
                                }

                                ctx.set_node_typeinfo(
                                    function_call_expression.id,
                                    signature.return_type.clone(),
                                );
                                return Some(signature.return_type.clone());
                            }
                            self.errors.push(TypeCheckError::MethodNotFound {
                                type_name,
                                method_name: method_name.clone(),
                                location: None,
                            });
                            return None;
                        }
                        self.errors.push(TypeCheckError::MethodCallOnNonStruct {
                            found: receiver_type,
                            location: None,
                        });
                        return None;
                    }
                    return None;
                }

                let signature = if let Some(s) = self
                    .symbol_table
                    .lookup_function(&function_call_expression.name())
                {
                    s.clone()
                } else {
                    self.errors.push(TypeCheckError::UndefinedFunction {
                        name: function_call_expression.name(),
                        location: None,
                    });
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
                    self.errors.push(TypeCheckError::ArgumentCountMismatch {
                        kind: "function",
                        name: function_call_expression.name(),
                        expected: signature.param_types.len(),
                        found: arguments.len(),
                        location: None,
                    });
                    for arg in arguments {
                        self.infer_expression(&mut arg.1.borrow_mut(), ctx);
                    }
                    return None;
                }

                if !signature.type_params.is_empty() {
                    if let Some(type_parameters) = &function_call_expression.type_parameters {
                        if type_parameters.len() != signature.type_params.len() {
                            self.errors
                                .push(TypeCheckError::TypeParameterCountMismatch {
                                    name: function_call_expression.name(),
                                    expected: signature.type_params.len(),
                                    found: type_parameters.len(),
                                    location: None,
                                });
                        }
                    } else {
                        self.errors.push(TypeCheckError::MissingTypeParameters {
                            function_name: function_call_expression.name(),
                            expected: signature.type_params.len(),
                            location: None,
                        });
                    }
                }
                ctx.set_node_typeinfo(function_call_expression.id, signature.return_type.clone());
                Some(signature.return_type.clone())
            }
            Expression::Struct(struct_expression) => {
                if let Some(type_info) = ctx.get_node_typeinfo(struct_expression.id) {
                    return Some(type_info.clone());
                }
                let struct_type = self.symbol_table.lookup_type(&struct_expression.name());
                if let Some(struct_type) = struct_type {
                    ctx.set_node_typeinfo(struct_expression.id, struct_type.clone());
                    return Some(struct_type);
                }
                self.errors.push(TypeCheckError::UndefinedStruct {
                    name: struct_expression.name(),
                    location: None,
                });
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
                            self.errors.push(TypeCheckError::InvalidUnaryOperand {
                                operator: UnaryOperatorKind::Neg,
                                expected_type: "booleans",
                                found_type: expression_type,
                                location: None,
                            });
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
                        self.errors.push(TypeCheckError::BinaryOperandTypeMismatch {
                            operator: binary_expression.operator.clone(),
                            left: left_type.clone(),
                            right: right_type.clone(),
                            location: None,
                        });
                    }
                    let res_type = match binary_expression.operator {
                        OperatorKind::And | OperatorKind::Or => {
                            if left_type.is_bool() && right_type.is_bool() {
                                TypeInfo {
                                    kind: TypeInfoKind::Bool,
                                    type_params: vec![],
                                }
                            } else {
                                self.errors.push(TypeCheckError::InvalidBinaryOperand {
                                    operator: binary_expression.operator.clone(),
                                    expected_kind: "logical",
                                    operand_desc: "non-boolean types",
                                    found_types: (left_type, right_type),
                                    location: None,
                                });
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
                                self.errors.push(TypeCheckError::InvalidBinaryOperand {
                                    operator: binary_expression.operator.clone(),
                                    expected_kind: "arithmetic",
                                    operand_desc: "non-number types",
                                    found_types: (left_type.clone(), right_type.clone()),
                                    location: None,
                                });
                            }
                            if left_type != right_type {
                                self.errors.push(TypeCheckError::BinaryOperandTypeMismatch {
                                    operator: binary_expression.operator.clone(),
                                    left: left_type.clone(),
                                    right: right_type,
                                    location: None,
                                });
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
                                    self.errors.push(TypeCheckError::ArrayElementTypeMismatch {
                                        expected: element_type_info.clone(),
                                        found: element_type,
                                        location: None,
                                    });
                                }
                            }
                        } else {
                            self.errors.push(TypeCheckError::General(
                                "array elements must be of the same type".to_string(),
                            ));
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
                    self.errors.push(TypeCheckError::UnknownIdentifier {
                        name: identifier.name.clone(),
                        location: None,
                    });
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

    /// Process a module definition.
    ///
    /// Creates a new scope for the module and processes all definitions within it.
    /// After processing, pops back to the parent scope.
    #[allow(dead_code)]
    fn process_module_definition(
        &mut self,
        module: &Rc<ModuleDefinition>,
        ctx: &mut TypedContext,
    ) -> anyhow::Result<()> {
        let _scope_id = self.symbol_table.enter_module(module);

        if let Some(body) = &module.body {
            for definition in body {
                match definition {
                    Definition::Type(type_definition) => {
                        self.symbol_table
                            .register_type(&type_definition.name(), Some(&type_definition.ty))
                            .unwrap_or_else(|_| {
                                self.errors.push(TypeCheckError::RegistrationFailed {
                                    kind: RegistrationKind::Type,
                                    name: type_definition.name(),
                                    reason: None,
                                    location: None,
                                });
                            });
                    }
                    Definition::Struct(struct_definition) => {
                        let fields: Vec<(String, TypeInfo, Visibility)> = struct_definition
                            .fields
                            .iter()
                            .map(|f| {
                                (
                                    f.name.name.clone(),
                                    TypeInfo::new(&f.type_),
                                    Visibility::Private,
                                )
                            })
                            .collect();
                        self.symbol_table
                            .register_struct(
                                &struct_definition.name(),
                                &fields,
                                vec![],
                                struct_definition.visibility.clone(),
                            )
                            .unwrap_or_else(|_| {
                                self.errors.push(TypeCheckError::RegistrationFailed {
                                    kind: RegistrationKind::Struct,
                                    name: struct_definition.name(),
                                    reason: None,
                                    location: None,
                                });
                            });
                    }
                    Definition::Enum(enum_definition) => {
                        let variants: Vec<&str> = enum_definition
                            .variants
                            .iter()
                            .map(|v| v.name.as_str())
                            .collect();
                        self.symbol_table
                            .register_enum(
                                &enum_definition.name(),
                                &variants,
                                enum_definition.visibility.clone(),
                            )
                            .unwrap_or_else(|_| {
                                self.errors.push(TypeCheckError::RegistrationFailed {
                                    kind: RegistrationKind::Enum,
                                    name: enum_definition.name(),
                                    reason: None,
                                    location: None,
                                });
                            });
                    }
                    Definition::Spec(spec_definition) => {
                        self.symbol_table
                            .register_spec(&spec_definition.name())
                            .unwrap_or_else(|_| {
                                self.errors.push(TypeCheckError::RegistrationFailed {
                                    kind: RegistrationKind::Spec,
                                    name: spec_definition.name(),
                                    reason: None,
                                    location: None,
                                });
                            });
                    }
                    Definition::Module(nested_module) => {
                        self.process_module_definition(nested_module, ctx)?;
                    }
                    Definition::Function(function_definition) => {
                        self.infer_variables(function_definition.clone(), ctx);
                    }
                    Definition::Constant(constant_definition) => {
                        if let Err(err) = self.symbol_table.push_variable_to_scope(
                            &constant_definition.name(),
                            TypeInfo::new(&constant_definition.ty),
                        ) {
                            self.errors.push(TypeCheckError::RegistrationFailed {
                                kind: RegistrationKind::Variable,
                                name: constant_definition.name(),
                                reason: Some(err.to_string()),
                                location: None,
                            });
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
                                .filter_map(|param| match param {
                                    ArgumentType::SelfReference(_) => None,
                                    ArgumentType::IgnoreArgument(ignore_argument) => {
                                        Some(ignore_argument.ty.clone())
                                    }
                                    ArgumentType::Argument(argument) => Some(argument.ty.clone()),
                                    ArgumentType::Type(ty) => Some(ty.clone()),
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
                            self.errors.push(TypeCheckError::RegistrationFailed {
                                kind: RegistrationKind::Function,
                                name: external_function_definition.name(),
                                reason: Some(err),
                                location: None,
                            });
                        }
                    }
                }
            }
        }

        self.symbol_table.pop_scope();
        Ok(())
    }

    /// Process all use directives in source files (Phase A of import resolution).
    fn process_directives(&mut self, ctx: &mut TypedContext) {
        for source_file in ctx.source_files() {
            for directive in &source_file.directives {
                match directive {
                    Directive::Use(use_directive) => {
                        if let Err(err) = self.process_use_statement(use_directive, ctx) {
                            self.errors.push(TypeCheckError::General(err.to_string()));
                        }
                    }
                }
            }
        }
    }

    /// Process a use statement (Phase A: registration only).
    /// Converts UseDirective AST to Import and registers in current scope.
    fn process_use_statement(
        &mut self,
        use_stmt: &Rc<UseDirective>,
        _ctx: &mut TypedContext,
    ) -> anyhow::Result<()> {
        let path: Vec<String> = use_stmt
            .segments
            .as_ref()
            .map(|segs| segs.iter().map(|s| s.name.clone()).collect())
            .unwrap_or_default();

        let kind = match &use_stmt.imported_types {
            None => ImportKind::Plain,
            Some(types) if types.is_empty() => ImportKind::Plain,
            Some(types) => {
                let items: Vec<ImportItem> = types
                    .iter()
                    .map(|t| ImportItem {
                        name: t.name.clone(),
                        alias: None,
                    })
                    .collect();
                ImportKind::Partial(items)
            }
        };

        let import = Import { path, kind };
        self.symbol_table.register_import(import)
    }

    /// Resolve all imports (Phase B of import resolution).
    /// This runs after register_types() so symbols are available.
    fn resolve_imports(&mut self) {
        let scope_ids: Vec<u32> = self.symbol_table.all_scope_ids();

        for scope_id in scope_ids {
            self.resolve_imports_in_scope(scope_id);
        }
    }

    /// Resolve imports within a single scope
    fn resolve_imports_in_scope(&mut self, scope_id: u32) {
        let imports = {
            let scope = match self.symbol_table.get_scope(scope_id) {
                Some(s) => s,
                None => return,
            };
            scope.borrow().imports.clone()
        };

        for import in imports {
            match &import.kind {
                ImportKind::Plain => {
                    if let Some(symbol_name) = import.path.last() {
                        if let Some((symbol, def_scope_id)) = self
                            .symbol_table
                            .resolve_qualified_name(&import.path, scope_id)
                        {
                            let resolved = ResolvedImport {
                                local_name: symbol_name.clone(),
                                symbol,
                                definition_scope_id: def_scope_id,
                            };
                            if let Some(scope) = self.symbol_table.get_scope(scope_id) {
                                scope.borrow_mut().add_resolved_import(resolved);
                            }
                        } else {
                            self.errors.push(TypeCheckError::ImportResolutionFailed {
                                path: import.path.join("::"),
                                location: None,
                            });
                        }
                    }
                }
                ImportKind::Partial(items) => {
                    for item in items {
                        let mut full_path = import.path.clone();
                        full_path.push(item.name.clone());

                        if let Some((symbol, def_scope_id)) = self
                            .symbol_table
                            .resolve_qualified_name(&full_path, scope_id)
                        {
                            let local_name =
                                item.alias.clone().unwrap_or_else(|| item.name.clone());
                            let resolved = ResolvedImport {
                                local_name,
                                symbol,
                                definition_scope_id: def_scope_id,
                            };
                            if let Some(scope) = self.symbol_table.get_scope(scope_id) {
                                scope.borrow_mut().add_resolved_import(resolved);
                            }
                        } else {
                            self.errors.push(TypeCheckError::ImportResolutionFailed {
                                path: format!("{}::{}", import.path.join("::"), item.name),
                                location: None,
                            });
                        }
                    }
                }
                ImportKind::Glob => {
                    self.resolve_glob_import(&import.path, scope_id);
                }
            }
        }
    }

    /// Resolve a glob import (`use path::*`) by importing all public symbols from the target module.
    fn resolve_glob_import(&mut self, path: &[String], into_scope_id: u32) {
        if path.is_empty() {
            self.errors
                .push(TypeCheckError::EmptyGlobImport { location: None });
            return;
        }

        let target_scope_id = match self.symbol_table.find_module_scope(path) {
            Some(id) => id,
            None => {
                self.errors.push(TypeCheckError::ImportResolutionFailed {
                    path: format!("{}::* - module not found", path.join("::")),
                    location: None,
                });
                return;
            }
        };

        if self.glob_resolution_in_progress.contains(&target_scope_id) {
            self.errors.push(TypeCheckError::CircularImport {
                path: path.join("::"),
                location: None,
            });
            return;
        }

        self.glob_resolution_in_progress.insert(target_scope_id);

        let public_symbols = self
            .symbol_table
            .get_public_symbols_from_scope(target_scope_id);

        if let Some(scope) = self.symbol_table.get_scope(into_scope_id) {
            for (name, symbol) in public_symbols {
                let resolved = ResolvedImport {
                    local_name: name,
                    symbol,
                    definition_scope_id: target_scope_id,
                };
                scope.borrow_mut().add_resolved_import(resolved);
            }
        }

        self.glob_resolution_in_progress.remove(&target_scope_id);
    }

    /// Check visibility of a definition from current scope.
    ///
    /// A private item is visible to the same scope and all descendant scopes.
    /// A public item is visible everywhere.
    #[allow(dead_code)]
    fn check_visibility(
        &self,
        visibility: &Visibility,
        definition_scope: u32,
        access_scope: u32,
    ) -> bool {
        match visibility {
            Visibility::Public => true,
            Visibility::Private => self.is_scope_descendant_of(access_scope, definition_scope),
        }
    }

    /// Check if access_scope is the same as or a descendant of target_scope.
    /// Uses iteration to avoid stack overflow on deep scope trees.
    fn is_scope_descendant_of(&self, access_scope: u32, target_scope: u32) -> bool {
        let mut current = access_scope;
        loop {
            if current == target_scope {
                return true;
            }
            if let Some(scope) = self.symbol_table.get_scope(current) {
                if let Some(parent) = &scope.borrow().parent {
                    current = parent.borrow().id;
                } else {
                    return false;
                }
            } else {
                return false;
            }
        }
    }
}
