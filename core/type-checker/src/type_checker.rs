//! Type Checker Implementation
//!
//! This module contains the core type checking logic that infers and validates
//! types throughout the AST. The type checker operates in multiple phases:
//!
//! 1. **process_directives** - Register raw imports from use statements
//! 2. **register_types** - Collect type/struct/enum/spec definitions
//! 3. **resolve_imports** - Bind import paths to symbols
//! 4. **collect_function_and_constant_definitions** - Register functions
//! 5. **infer_variables** - Type-check function bodies
//!
//! The type checker continues after encountering errors to collect all issues
//! before returning. Errors are deduplicated to avoid repeated reports.

use std::rc::Rc;

use anyhow::bail;
use inference_ast::extern_prelude::ExternPrelude;
use inference_ast::nodes::{
    ArgumentType, Definition, Directive, Expression, FunctionDefinition, Identifier, Literal,
    Location, ModuleDefinition, OperatorKind, SimpleType, Statement, Type, UnaryOperatorKind,
    UseDirective, Visibility,
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    errors::{RegistrationKind, TypeCheckError, TypeMismatchContext, VisibilityContext},
    symbol_table::{FuncInfo, Import, ImportItem, ImportKind, ResolvedImport, SymbolTable},
    type_info::{NumberType, TypeInfo, TypeInfoKind},
    typed_context::TypedContext,
};

#[derive(Default)]
pub(crate) struct TypeChecker {
    symbol_table: SymbolTable,
    errors: Vec<TypeCheckError>,
    glob_resolution_in_progress: FxHashSet<u32>,
    reported_error_keys: FxHashSet<String>,
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
        // Continue to inference phase even if registration had errors
        // to collect all errors before returning
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

    /// Registers `Definition::Type`, `Definition::Struct`, `Definition::Enum`, and `Definition::Spec`
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
                                    location: type_definition.location,
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
                                    location: struct_definition.location,
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

                            let definition_scope_id =
                                self.symbol_table.current_scope_id().unwrap_or(0);
                            let signature = FuncInfo {
                                name: method.name(),
                                type_params,
                                param_types,
                                return_type,
                                visibility: method.visibility.clone(),
                                definition_scope_id,
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
                                        location: method.location,
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
                                    location: enum_definition.location,
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
                                    location: spec_definition.location,
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

    /// Registers `Definition::Function`, `Definition::ExternalFunction`, and `Definition::Constant`
    #[allow(clippy::too_many_lines)]
    fn collect_function_and_constant_definitions(&mut self, ctx: &mut TypedContext) {
        for sf in ctx.source_files() {
            for definition in &sf.definitions {
                match definition {
                    Definition::Constant(constant_definition) => {
                        let const_type = TypeInfo::new(&constant_definition.ty);
                        if let Err(err) = self
                            .symbol_table
                            .push_variable_to_scope(&constant_definition.name(), const_type.clone())
                        {
                            self.errors.push(TypeCheckError::RegistrationFailed {
                                kind: RegistrationKind::Variable,
                                name: constant_definition.name(),
                                reason: Some(err.to_string()),
                                location: constant_definition.location,
                            });
                        }
                        ctx.set_node_typeinfo(constant_definition.value.id(), const_type);
                    }
                    Definition::Function(function_definition) => {
                        for param in function_definition.arguments.as_ref().unwrap_or(&vec![]) {
                            match param {
                                ArgumentType::SelfReference(self_ref) => {
                                    self.errors.push(TypeCheckError::SelfReferenceInFunction {
                                        function_name: function_definition.name(),
                                        location: self_ref.location,
                                    });
                                }
                                ArgumentType::IgnoreArgument(ignore_argument) => {
                                    self.validate_type(
                                        &ignore_argument.ty,
                                        function_definition.type_parameters.as_ref(),
                                    );
                                    ctx.set_node_typeinfo(
                                        ignore_argument.id,
                                        TypeInfo::new(&ignore_argument.ty),
                                    );
                                }
                                ArgumentType::Argument(arg) => {
                                    self.validate_type(
                                        &arg.ty,
                                        function_definition.type_parameters.as_ref(),
                                    );
                                    let type_info = TypeInfo::new(&arg.ty);
                                    ctx.set_node_typeinfo(arg.id, type_info.clone());
                                    ctx.set_node_typeinfo(arg.name.id, type_info);
                                }
                                ArgumentType::Type(ty) => {
                                    self.validate_type(
                                        ty,
                                        function_definition.type_parameters.as_ref(),
                                    );
                                }
                            }
                        }
                        ctx.set_node_typeinfo(
                            function_definition.name.id,
                            TypeInfo {
                                kind: TypeInfoKind::Function(function_definition.name()),
                                type_params: function_definition
                                    .type_parameters
                                    .as_ref()
                                    .map_or(vec![], |p| p.iter().map(|i| i.name.clone()).collect()),
                            },
                        );
                        if let Some(return_type) = &function_definition.returns {
                            self.validate_type(
                                return_type,
                                function_definition.type_parameters.as_ref(),
                            );
                            ctx.set_node_typeinfo(return_type.id(), TypeInfo::new(return_type));
                        }
                        // Register function even if parameter validation had errors
                        // to allow error recovery and prevent spurious UndefinedFunction errors
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
                                location: function_definition.location,
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
                                location: external_function_definition.location,
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
        // Collect type parameter names for checking
        let type_param_names: Vec<String> = type_parameters
            .map(|params| params.iter().map(|p| p.name()).collect())
            .unwrap_or_default();

        match ty {
            Type::Array(type_array) => {
                self.validate_type(&type_array.element_type, type_parameters)
            }
            Type::Simple(simple_type) => {
                // Type parameters (like T, U) are valid types within the function
                if type_param_names.contains(&simple_type.name) {
                    return;
                }
                if self.symbol_table.lookup_type(&simple_type.name).is_none() {
                    self.push_error_dedup(TypeCheckError::UnknownType {
                        name: simple_type.name.clone(),
                        location: simple_type.location,
                    });
                }
            }
            Type::Generic(generic_type) => {
                if self
                    .symbol_table
                    .lookup_type(&generic_type.base.name())
                    .is_none()
                {
                    self.push_error_dedup(TypeCheckError::UnknownType {
                        name: generic_type.base.name(),
                        location: generic_type.base.location,
                    });
                }
                // Validate each parameter in the generic type
                for param in &generic_type.parameters {
                    // Check if it's a declared type parameter or a known type
                    if !type_param_names.contains(&param.name())
                        && self.symbol_table.lookup_type(&param.name()).is_none()
                    {
                        self.push_error_dedup(TypeCheckError::UnknownType {
                            name: param.name(),
                            location: param.location,
                        });
                    }
                }
            }
            Type::Function(_) | Type::QualifiedName(_) | Type::Qualified(_) => {}
            Type::Custom(identifier) => {
                // Type parameters (like T, U) are valid types within the function
                if type_param_names.contains(&identifier.name) {
                    return;
                }
                if self.symbol_table.lookup_type(&identifier.name).is_none() {
                    self.push_error_dedup(TypeCheckError::UnknownType {
                        name: identifier.name.clone(),
                        location: identifier.location,
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

        // Collect type parameter names for proper TypeInfo construction
        let type_param_names: Vec<String> = function_definition
            .type_parameters
            .as_ref()
            .map(|params| params.iter().map(|p| p.name()).collect())
            .unwrap_or_default();

        if let Some(arguments) = &function_definition.arguments {
            for argument in arguments {
                match argument {
                    ArgumentType::Argument(arg) => {
                        let arg_type = TypeInfo::new_with_type_params(&arg.ty, &type_param_names);
                        if let Err(err) = self
                            .symbol_table
                            .push_variable_to_scope(&arg.name(), arg_type)
                        {
                            self.errors.push(TypeCheckError::RegistrationFailed {
                                kind: RegistrationKind::Variable,
                                name: arg.name(),
                                reason: Some(err.to_string()),
                                location: arg.location,
                            });
                        }
                    }
                    ArgumentType::SelfReference(self_ref) => {
                        self.errors
                            .push(TypeCheckError::SelfReferenceOutsideMethod {
                                location: self_ref.location,
                            });
                    }
                    ArgumentType::IgnoreArgument(_) | ArgumentType::Type(_) => {}
                }
            }
        }

        // Build return type with type parameter awareness
        let return_type = function_definition
            .returns
            .as_ref()
            .map(|r| TypeInfo::new_with_type_params(r, &type_param_names))
            .unwrap_or_default();

        for stmt in &mut function_definition.body.statements() {
            self.infer_statement(stmt, &return_type, ctx);
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
                                location: arg.location,
                            });
                        }
                    }
                    ArgumentType::SelfReference(self_ref) => {
                        if let Err(err) = self
                            .symbol_table
                            .push_variable_to_scope("self", self_type.clone())
                        {
                            self.errors.push(TypeCheckError::RegistrationFailed {
                                kind: RegistrationKind::Variable,
                                name: "self".to_string(),
                                reason: Some(err.to_string()),
                                location: self_ref.location,
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
        statement: &Statement,
        return_type: &TypeInfo,
        ctx: &mut TypedContext,
    ) {
        match statement {
            Statement::Assign(assign_statement) => {
                let target_type = self.infer_expression(&assign_statement.left.borrow(), ctx);
                let right_expr = assign_statement.right.borrow();
                if let Expression::Uzumaki(uzumaki_rc) = &*right_expr {
                    if let Some(target) = &target_type {
                        ctx.set_node_typeinfo(uzumaki_rc.id, target.clone());
                    } else {
                        self.errors.push(TypeCheckError::CannotInferUzumakiType {
                            location: uzumaki_rc.location,
                        });
                    }
                } else {
                    let value_type = self.infer_expression(&right_expr, ctx);
                    if let (Some(target), Some(val)) = (target_type, value_type)
                        && target != val
                    {
                        self.errors.push(TypeCheckError::TypeMismatch {
                            expected: target,
                            found: val,
                            context: TypeMismatchContext::Assignment,
                            location: assign_statement.location,
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
                        self.infer_expression(&return_statement.expression.borrow(), ctx);
                    if *return_type != value_type.clone().unwrap_or_default() {
                        self.errors.push(TypeCheckError::TypeMismatch {
                            expected: return_type.clone(),
                            found: value_type.unwrap_or_default(),
                            context: TypeMismatchContext::Return,
                            location: return_statement.location,
                        });
                    }
                }
            }
            Statement::Loop(loop_statement) => {
                if let Some(condition) = &*loop_statement.condition.borrow() {
                    let condition_type = self.infer_expression(condition, ctx);
                    if condition_type.is_none()
                        || condition_type.as_ref().unwrap().kind != TypeInfoKind::Bool
                    {
                        self.errors.push(TypeCheckError::TypeMismatch {
                            expected: TypeInfo::boolean(),
                            found: condition_type.unwrap_or_default(),
                            context: TypeMismatchContext::Condition,
                            location: loop_statement.location,
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
                let condition_type = self.infer_expression(&if_statement.condition.borrow(), ctx);
                if condition_type.is_none()
                    || condition_type.as_ref().unwrap().kind != TypeInfoKind::Bool
                {
                    self.errors.push(TypeCheckError::TypeMismatch {
                        expected: TypeInfo::boolean(),
                        found: condition_type.unwrap_or_default(),
                        context: TypeMismatchContext::Condition,
                        location: if_statement.location,
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
                    } else if let Some(init_type) = self.infer_expression(&expr_ref, ctx)
                        && init_type != TypeInfo::new(&variable_definition_statement.ty)
                    {
                        self.errors.push(TypeCheckError::TypeMismatch {
                            expected: target_type.clone(),
                            found: init_type,
                            context: TypeMismatchContext::VariableDefinition,
                            location: variable_definition_statement.location,
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
                        location: variable_definition_statement.location,
                    });
                }
                ctx.set_node_typeinfo(variable_definition_statement.name.id, target_type.clone());
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
                        location: type_definition_statement.location,
                    });
                }
            }
            Statement::Assert(assert_statement) => {
                let condition_type =
                    self.infer_expression(&assert_statement.expression.borrow(), ctx);
                if condition_type.is_none()
                    || condition_type.as_ref().unwrap().kind != TypeInfoKind::Bool
                {
                    self.errors.push(TypeCheckError::TypeMismatch {
                        expected: TypeInfo::boolean(),
                        found: condition_type.unwrap_or_default(),
                        context: TypeMismatchContext::Condition,
                        location: assert_statement.location,
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
                        location: constant_definition.location,
                    });
                }
                ctx.set_node_typeinfo(constant_definition.value.id(), constant_type.clone());
                ctx.set_node_typeinfo(constant_definition.id, constant_type);
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn infer_expression(
        &mut self,
        expression: &Expression,
        ctx: &mut TypedContext,
    ) -> Option<TypeInfo> {
        match expression {
            Expression::ArrayIndexAccess(array_index_access_expression) => {
                if let Some(type_info) = ctx.get_node_typeinfo(array_index_access_expression.id) {
                    Some(type_info.clone())
                } else if let Some(array_type) =
                    self.infer_expression(&array_index_access_expression.array.borrow(), ctx)
                {
                    if let Some(index_type) =
                        self.infer_expression(&array_index_access_expression.index.borrow(), ctx)
                        && !index_type.is_number()
                    {
                        self.errors.push(TypeCheckError::ArrayIndexNotNumeric {
                            found: index_type,
                            location: array_index_access_expression.location,
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
                                location: array_index_access_expression.location,
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
                } else if let Some(object_type) =
                    self.infer_expression(&member_access_expression.expression.borrow(), ctx)
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
                        // Look up struct to get field info including visibility
                        if let Some(struct_info) = self.symbol_table.lookup_struct(&struct_name) {
                            if let Some(field_info) = struct_info.fields.get(field_name) {
                                // Check field visibility
                                self.check_and_report_visibility(
                                    &field_info.visibility,
                                    struct_info.definition_scope_id,
                                    &member_access_expression.location,
                                    VisibilityContext::Field {
                                        struct_name: struct_name.clone(),
                                        field_name: field_name.clone(),
                                    },
                                );
                                let field_type = field_info.type_info.clone();
                                ctx.set_node_typeinfo(
                                    member_access_expression.id,
                                    field_type.clone(),
                                );
                                Some(field_type)
                            } else {
                                self.errors.push(TypeCheckError::FieldNotFound {
                                    struct_name,
                                    field_name: field_name.clone(),
                                    location: member_access_expression.location,
                                });
                                None
                            }
                        } else {
                            self.errors.push(TypeCheckError::FieldNotFound {
                                struct_name,
                                field_name: field_name.clone(),
                                location: member_access_expression.location,
                            });
                            None
                        }
                    } else {
                        self.errors.push(TypeCheckError::ExpectedStructType {
                            found: object_type,
                            location: member_access_expression.location,
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
                                    location: type_member_access_expression.location,
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
                            &type_member_access_expression.expression.borrow(),
                            ctx,
                        ) {
                            match &expr_type.kind {
                                TypeInfoKind::Enum(name) => name.clone(),
                                _ => {
                                    self.errors.push(TypeCheckError::ExpectedEnumType {
                                        found: expr_type,
                                        location: type_member_access_expression.location,
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
                        // Check enum visibility (variants inherit the enum's visibility,
                        // unlike struct fields which have per-field visibility)
                        self.check_and_report_visibility(
                            &enum_info.visibility,
                            enum_info.definition_scope_id,
                            &type_member_access_expression.location,
                            VisibilityContext::Enum {
                                name: enum_name.clone(),
                            },
                        );
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
                            location: type_member_access_expression.location,
                        });
                        None
                    }
                } else {
                    self.push_error_dedup(TypeCheckError::UndefinedEnum {
                        name: enum_name,
                        location: type_member_access_expression.location,
                    });
                    None
                }
            }
            Expression::FunctionCall(function_call_expression) => {
                // Handle Type::function() syntax - associated function calls
                if let Expression::TypeMemberAccess(type_member_access) =
                    &function_call_expression.function
                {
                    let inner_expr = type_member_access.expression.borrow();

                    // Extract type name from the expression
                    let type_name = match &*inner_expr {
                        Expression::Type(ty) => match ty {
                            Type::Simple(simple_type) => Some(simple_type.name.clone()),
                            Type::Custom(ident) => Some(ident.name.clone()),
                            _ => None,
                        },
                        Expression::Identifier(id) => Some(id.name.clone()),
                        _ => None,
                    };

                    drop(inner_expr); // Release borrow before continuing

                    if let Some(type_name) = type_name {
                        let method_name = &type_member_access.name.name;

                        // First check if this is an enum variant - can't call variants like functions
                        if self.symbol_table.lookup_enum(&type_name).is_some() {
                            // This is an enum type - TypeMemberAccess on enums is for variants,
                            // not function calls. The enum variant access should be handled by
                            // the TypeMemberAccess expression handler, not here.
                            // Fall through to standard function handling which will report
                            // "undefined function" error.
                        } else if let Some(method_info) =
                            self.symbol_table.lookup_method(&type_name, method_name)
                        {
                            // Found a method - check if it's an instance method or associated function
                            if method_info.is_instance_method() {
                                // Error: calling instance method without receiver
                                self.errors.push(
                                    TypeCheckError::InstanceMethodCalledAsAssociated {
                                        type_name: type_name.clone(),
                                        method_name: method_name.clone(),
                                        location: type_member_access.location,
                                    },
                                );
                                // Continue with type checking for better error recovery
                            }

                            // Check visibility of the method
                            self.check_and_report_visibility(
                                &method_info.visibility,
                                method_info.scope_id,
                                &type_member_access.location,
                                VisibilityContext::Method {
                                    type_name: type_name.clone(),
                                    method_name: method_name.clone(),
                                },
                            );

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
                                    location: function_call_expression.location,
                                });
                            }

                            if let Some(arguments) = &function_call_expression.arguments {
                                for arg in arguments {
                                    self.infer_expression(&arg.1.borrow(), ctx);
                                }
                            }

                            ctx.set_node_typeinfo(
                                type_member_access.id,
                                TypeInfo {
                                    kind: TypeInfoKind::Function(format!(
                                        "{}::{}",
                                        type_name, method_name
                                    )),
                                    type_params: vec![],
                                },
                            );
                            ctx.set_node_typeinfo(
                                function_call_expression.id,
                                signature.return_type.clone(),
                            );
                            return Some(signature.return_type.clone());
                        }
                        // Not an enum and not a method - fall through to standard function handling
                    }
                    // Fall through to standard function handling for invalid type expressions
                }

                if let Expression::MemberAccess(member_access) = &function_call_expression.function
                {
                    let receiver_type =
                        self.infer_expression(&member_access.expression.borrow(), ctx);

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
                                // Check if this is an associated function being called as instance method
                                if !method_info.is_instance_method() {
                                    // Error: calling associated function with receiver
                                    self.errors.push(
                                        TypeCheckError::AssociatedFunctionCalledAsMethod {
                                            type_name: type_name.clone(),
                                            method_name: method_name.clone(),
                                            location: member_access.location,
                                        },
                                    );
                                    // Continue with type checking for better error recovery
                                }

                                // Check visibility of the method
                                self.check_and_report_visibility(
                                    &method_info.visibility,
                                    method_info.scope_id,
                                    &member_access.location,
                                    VisibilityContext::Method {
                                        type_name: type_name.clone(),
                                        method_name: method_name.clone(),
                                    },
                                );

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
                                        location: function_call_expression.location,
                                    });
                                }

                                if let Some(arguments) = &function_call_expression.arguments {
                                    for arg in arguments {
                                        self.infer_expression(&arg.1.borrow(), ctx);
                                    }
                                }

                                ctx.set_node_typeinfo(
                                    member_access.id,
                                    TypeInfo {
                                        kind: TypeInfoKind::Function(format!(
                                            "{}::{}",
                                            type_name, method_name
                                        )),
                                        type_params: vec![],
                                    },
                                );
                                ctx.set_node_typeinfo(
                                    function_call_expression.id,
                                    signature.return_type.clone(),
                                );
                                return Some(signature.return_type.clone());
                            }
                            self.errors.push(TypeCheckError::MethodNotFound {
                                type_name,
                                method_name: method_name.clone(),
                                location: member_access.location,
                            });
                            return None;
                        }
                        self.errors.push(TypeCheckError::MethodCallOnNonStruct {
                            found: receiver_type,
                            location: function_call_expression.location,
                        });
                        // Infer arguments even for non-struct receiver for better error recovery
                        if let Some(arguments) = &function_call_expression.arguments {
                            for arg in arguments {
                                self.infer_expression(&arg.1.borrow(), ctx);
                            }
                        }
                        return None;
                    }
                    // Receiver type inference failed; infer arguments for better error recovery
                    if let Some(arguments) = &function_call_expression.arguments {
                        for arg in arguments {
                            self.infer_expression(&arg.1.borrow(), ctx);
                        }
                    }
                    return None;
                }

                let signature = if let Some(s) = self
                    .symbol_table
                    .lookup_function(&function_call_expression.name())
                {
                    // Check visibility of the function
                    self.check_and_report_visibility(
                        &s.visibility,
                        s.definition_scope_id,
                        &function_call_expression.location,
                        VisibilityContext::Function {
                            name: function_call_expression.name(),
                        },
                    );
                    s.clone()
                } else {
                    self.push_error_dedup(TypeCheckError::UndefinedFunction {
                        name: function_call_expression.name(),
                        location: function_call_expression.location,
                    });
                    if let Some(arguments) = &function_call_expression.arguments {
                        for arg in arguments {
                            self.infer_expression(&arg.1.borrow(), ctx);
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
                        location: function_call_expression.location,
                    });
                    for arg in arguments {
                        self.infer_expression(&arg.1.borrow(), ctx);
                    }
                    return None;
                }

                // Build substitution map for generic functions
                let substitutions = if !signature.type_params.is_empty() {
                    if let Some(type_parameters) = &function_call_expression.type_parameters {
                        if type_parameters.len() != signature.type_params.len() {
                            self.errors
                                .push(TypeCheckError::TypeParameterCountMismatch {
                                    name: function_call_expression.name(),
                                    expected: signature.type_params.len(),
                                    found: type_parameters.len(),
                                    location: function_call_expression.location,
                                });
                            FxHashMap::default()
                        } else {
                            // Build substitution map: type_param_name -> concrete type
                            // Type parameters are identifiers representing type names
                            signature
                                .type_params
                                .iter()
                                .zip(type_parameters.iter())
                                .map(|(param_name, type_ident)| {
                                    // Convert identifier to TypeInfo by looking it up
                                    let concrete_type = self
                                        .symbol_table
                                        .lookup_type(&type_ident.name)
                                        .unwrap_or_else(|| TypeInfo {
                                            kind: TypeInfoKind::Custom(type_ident.name.clone()),
                                            type_params: vec![],
                                        });
                                    (param_name.clone(), concrete_type)
                                })
                                .collect::<FxHashMap<String, TypeInfo>>()
                        }
                    } else {
                        // Try to infer type parameters from arguments
                        let inferred = self.infer_type_params_from_args(
                            &signature,
                            function_call_expression.arguments.as_ref(),
                            &function_call_expression.location,
                            ctx,
                        );
                        if inferred.is_empty() && !signature.type_params.is_empty() {
                            self.errors.push(TypeCheckError::MissingTypeParameters {
                                function_name: function_call_expression.name(),
                                expected: signature.type_params.len(),
                                location: function_call_expression.location,
                            });
                        }
                        inferred
                    }
                } else {
                    FxHashMap::default()
                };

                // Apply substitution to return type
                let return_type = signature.return_type.substitute(&substitutions);

                // Infer argument types
                if let Some(arguments) = &function_call_expression.arguments {
                    for arg in arguments {
                        self.infer_expression(&arg.1.borrow(), ctx);
                    }
                }

                ctx.set_node_typeinfo(function_call_expression.id, return_type.clone());
                Some(return_type)
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
                self.push_error_dedup(TypeCheckError::UndefinedStruct {
                    name: struct_expression.name(),
                    location: struct_expression.location,
                });
                None
            }
            Expression::PrefixUnary(prefix_unary_expression) => {
                match prefix_unary_expression.operator {
                    UnaryOperatorKind::Not => {
                        let expression_type_op = self
                            .infer_expression(&prefix_unary_expression.expression.borrow(), ctx);
                        if let Some(expression_type) = expression_type_op {
                            if expression_type.is_bool() {
                                ctx.set_node_typeinfo(
                                    prefix_unary_expression.id,
                                    expression_type.clone(),
                                );
                                return Some(expression_type);
                            }
                            self.errors.push(TypeCheckError::InvalidUnaryOperand {
                                operator: UnaryOperatorKind::Not,
                                expected_type: "booleans",
                                found_type: expression_type,
                                location: prefix_unary_expression.location,
                            });
                        }
                        None
                    }
                    UnaryOperatorKind::Neg => {
                        let expression_type_op = self
                            .infer_expression(&prefix_unary_expression.expression.borrow(), ctx);
                        if let Some(expression_type) = expression_type_op {
                            if expression_type.is_signed_integer() {
                                ctx.set_node_typeinfo(
                                    prefix_unary_expression.id,
                                    expression_type.clone(),
                                );
                                return Some(expression_type);
                            }
                            self.errors.push(TypeCheckError::InvalidUnaryOperand {
                                operator: UnaryOperatorKind::Neg,
                                expected_type: "signed integers (i8, i16, i32, i64)",
                                found_type: expression_type,
                                location: prefix_unary_expression.location,
                            });
                        }
                        None
                    }
                    UnaryOperatorKind::BitNot => {
                        let expression_type_op = self
                            .infer_expression(&prefix_unary_expression.expression.borrow(), ctx);
                        if let Some(expression_type) = expression_type_op {
                            if expression_type.is_number() {
                                ctx.set_node_typeinfo(
                                    prefix_unary_expression.id,
                                    expression_type.clone(),
                                );
                                return Some(expression_type);
                            }
                            self.errors.push(TypeCheckError::InvalidUnaryOperand {
                                operator: UnaryOperatorKind::BitNot,
                                expected_type: "integers (i8, i16, i32, i64, u8, u16, u32, u64)",
                                found_type: expression_type,
                                location: prefix_unary_expression.location,
                            });
                        }
                        None
                    }
                }
            }
            Expression::Parenthesized(parenthesized_expression) => {
                let inner_type =
                    self.infer_expression(&parenthesized_expression.expression.borrow(), ctx);
                if let Some(ref type_info) = inner_type {
                    ctx.set_node_typeinfo(parenthesized_expression.id, type_info.clone());
                }
                inner_type
            }
            Expression::Binary(binary_expression) => {
                if let Some(type_info) = ctx.get_node_typeinfo(binary_expression.id) {
                    return Some(type_info.clone());
                }
                let left_type = self.infer_expression(&binary_expression.left.borrow(), ctx);
                let right_type = self.infer_expression(&binary_expression.right.borrow(), ctx);
                if let (Some(left_type), Some(right_type)) = (left_type, right_type) {
                    if left_type != right_type {
                        self.errors.push(TypeCheckError::BinaryOperandTypeMismatch {
                            operator: binary_expression.operator.clone(),
                            left: left_type.clone(),
                            right: right_type.clone(),
                            location: binary_expression.location,
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
                                    location: binary_expression.location,
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
                                    location: binary_expression.location,
                                });
                            }
                            if left_type != right_type {
                                self.errors.push(TypeCheckError::BinaryOperandTypeMismatch {
                                    operator: binary_expression.operator.clone(),
                                    left: left_type.clone(),
                                    right: right_type,
                                    location: binary_expression.location,
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
                    if let Some(type_info) = ctx.get_node_typeinfo(array_literal.id) {
                        return Some(type_info);
                    }
                    if let Some(elements) = &array_literal.elements
                        && let Some(element_type_info) =
                            self.infer_expression(&elements[0].borrow(), ctx)
                    {
                        for element in &elements[1..] {
                            let element_type = self.infer_expression(&element.borrow(), ctx);
                            if let Some(element_type) = element_type
                                && element_type != element_type_info
                            {
                                self.errors.push(TypeCheckError::ArrayElementTypeMismatch {
                                    expected: element_type_info.clone(),
                                    found: element_type,
                                    location: array_literal.location,
                                });
                            }
                        }
                        let array_type = TypeInfo {
                            kind: TypeInfoKind::Array(
                                Box::new(element_type_info),
                                elements.len() as u32,
                            ),
                            type_params: vec![],
                        };
                        ctx.set_node_typeinfo(array_literal.id, array_type.clone());
                        return Some(array_type);
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
                        kind: TypeInfoKind::Number(NumberType::I32),
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
                    self.push_error_dedup(TypeCheckError::UnknownIdentifier {
                        name: identifier.name.clone(),
                        location: identifier.location,
                    });
                    None
                }
            }
            Expression::Type(type_expr) => {
                let type_info = TypeInfo::new(type_expr);
                ctx.set_node_typeinfo(type_expr.id(), type_info.clone());
                if let Type::Array(array_type) = type_expr {
                    self.infer_expression(&array_type.size.clone(), ctx);
                }
                Some(type_info)
            }
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
                                    location: type_definition.location,
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
                                    location: struct_definition.location,
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
                                    location: enum_definition.location,
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
                                    location: spec_definition.location,
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
                                location: constant_definition.location,
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
                                location: external_function_definition.location,
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
                        if let Err(_err) = self.process_use_statement(use_directive, ctx) {
                            let path = use_directive
                                .segments
                                .as_ref()
                                .map(|segs| {
                                    segs.iter()
                                        .map(|s| s.name.as_str())
                                        .collect::<Vec<_>>()
                                        .join("::")
                                })
                                .unwrap_or_default();
                            self.errors.push(TypeCheckError::ImportResolutionFailed {
                                path,
                                location: use_directive.location,
                            });
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

        let import = Import {
            path,
            kind,
            location: use_stmt.location,
        };
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
                            // Check if the symbol is public - private symbols can't be imported
                            if !symbol.is_public() {
                                self.check_and_report_visibility(
                                    &Visibility::Private,
                                    def_scope_id,
                                    &import.location,
                                    VisibilityContext::Import {
                                        path: import.path.join("::"),
                                    },
                                );
                            }
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
                                location: import.location,
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
                            // Check if the symbol is public - private symbols can't be imported
                            if !symbol.is_public() {
                                self.check_and_report_visibility(
                                    &Visibility::Private,
                                    def_scope_id,
                                    &import.location,
                                    VisibilityContext::Import {
                                        path: full_path.join("::"),
                                    },
                                );
                            }
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
                                location: import.location,
                            });
                        }
                    }
                }
                ImportKind::Glob => {
                    self.resolve_glob_import(&import.path, &import.location, scope_id);
                }
            }
        }
    }

    /// Resolve a glob import (`use path::*`) by importing all public symbols from the target module.
    fn resolve_glob_import(&mut self, path: &[String], location: &Location, into_scope_id: u32) {
        if path.is_empty() {
            self.errors.push(TypeCheckError::EmptyGlobImport {
                location: *location,
            });
            return;
        }

        let target_scope_id = match self.symbol_table.find_module_scope(path) {
            Some(id) => id,
            None => {
                self.errors.push(TypeCheckError::ImportResolutionFailed {
                    path: format!("{}::* - module not found", path.join("::")),
                    location: *location,
                });
                return;
            }
        };

        if self.glob_resolution_in_progress.contains(&target_scope_id) {
            self.errors.push(TypeCheckError::CircularImport {
                path: path.join("::"),
                location: *location,
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

    /// Check visibility and report error if access is not allowed.
    /// Returns true if access is allowed, false if error was reported.
    fn check_and_report_visibility(
        &mut self,
        visibility: &Visibility,
        definition_scope: u32,
        location: &Location,
        context: VisibilityContext,
    ) -> bool {
        let access_scope = self.symbol_table.current_scope_id().unwrap_or(0);
        if self.check_visibility(visibility, definition_scope, access_scope) {
            true
        } else {
            self.errors.push(TypeCheckError::PrivateAccessViolation {
                context,
                location: *location,
            });
            false
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

    /// Attempt to infer type parameters from argument types.
    ///
    /// For each parameter that is a type variable (Generic), try to find a
    /// concrete type from the corresponding argument.
    ///
    /// Returns a substitution map if inference succeeds, empty map otherwise.
    #[allow(clippy::type_complexity)]
    fn infer_type_params_from_args(
        &mut self,
        signature: &FuncInfo,
        arguments: Option<&Vec<(Option<Rc<Identifier>>, std::cell::RefCell<Expression>)>>,
        call_location: &Location,
        ctx: &mut TypedContext,
    ) -> FxHashMap<String, TypeInfo> {
        let mut substitutions = FxHashMap::default();

        let args = match arguments {
            Some(args) => args,
            None => return substitutions,
        };

        // For each parameter, check if it contains a type variable
        for (i, param_type) in signature.param_types.iter().enumerate() {
            if i >= args.len() {
                break;
            }

            // If the parameter type is a type variable, infer from argument
            if let TypeInfoKind::Generic(type_param_name) = &param_type.kind {
                // Infer the argument type
                let arg_type = self.infer_expression(&args[i].1.borrow(), ctx);

                if let Some(arg_type) = arg_type {
                    // Check for conflicting inference
                    if let Some(existing) = substitutions.get(type_param_name) {
                        if *existing != arg_type {
                            self.errors.push(TypeCheckError::ConflictingTypeInference {
                                param_name: type_param_name.clone(),
                                first: existing.clone(),
                                second: arg_type.clone(),
                                location: *call_location,
                            });
                        }
                    } else {
                        substitutions.insert(type_param_name.clone(), arg_type);
                    }
                }
            }
        }

        // Check if we found substitutions for all type parameters
        for type_param in &signature.type_params {
            if !substitutions.contains_key(type_param) {
                self.errors.push(TypeCheckError::CannotInferTypeParameter {
                    function_name: signature.name.clone(),
                    param_name: type_param.clone(),
                    location: *call_location,
                });
            }
        }

        substitutions
    }

    /// Push an error, deduplicating errors for the same unknown type/function/identifier.
    /// This prevents duplicate errors when registration fails but inference continues.
    fn push_error_dedup(&mut self, error: TypeCheckError) {
        let key = match &error {
            TypeCheckError::UnknownType { name, .. } => Some(format!("UnknownType:{name}")),
            TypeCheckError::UndefinedFunction { name, .. } => {
                Some(format!("UndefinedFunction:{name}"))
            }
            TypeCheckError::UnknownIdentifier { name, .. } => {
                Some(format!("UnknownIdentifier:{name}"))
            }
            TypeCheckError::UndefinedStruct { name, .. } => Some(format!("UndefinedStruct:{name}")),
            TypeCheckError::UndefinedEnum { name, .. } => Some(format!("UndefinedEnum:{name}")),
            _ => None,
        };
        if let Some(key) = key {
            if self.reported_error_keys.contains(&key) {
                return;
            }
            self.reported_error_keys.insert(key);
        }
        self.errors.push(error);
    }
}
