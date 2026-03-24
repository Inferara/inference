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

use anyhow::bail;
use inference_ast::arena::AstArena;
use inference_ast::extern_prelude::ExternPrelude;
use inference_ast::ids::{DefId, ExprId, IdentId, NodeId, StmtId, TypeId};
use inference_ast::nodes::{
    ArgKind, Def, Directive, Expr, Location, OperatorKind, Stmt, TypeNode, UnaryOperatorKind,
    Visibility,
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
    /// Type parameter names for the function/method body currently being inferred.
    /// Set before walking the body, cleared after. Used by `infer_statement` to
    /// pass type param context to `validate_type` and `TypeInfo::from_type_id_with_type_params`.
    current_type_params: Vec<String>,
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
        let arena = ctx.arena();
        let all_def_ids: Vec<DefId> = arena
            .source_files()
            .flat_map(|sf| sf.defs.iter().copied())
            .collect();
        for def_id in all_def_ids {
            let kind = ctx.arena()[def_id].kind.clone();
            match &kind {
                Def::Function { .. } => {
                    self.infer_variables(def_id, ctx);
                }
                Def::Struct { name, methods, .. } => {
                    let struct_name = ctx.arena()[*name].name.clone();
                    let struct_type = TypeInfo {
                        kind: TypeInfoKind::Struct(struct_name.clone()),
                        type_params: vec![],
                    };
                    let method_ids: Vec<DefId> = methods.clone();
                    for method_id in method_ids {
                        self.infer_method_variables(method_id, struct_type.clone(), ctx);
                    }
                }
                _ => {}
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

    /// Registers `Def::TypeAlias`, `Def::Struct`, `Def::Enum`, and `Def::Spec`
    fn register_types(&mut self, ctx: &mut TypedContext) {
        let arena = ctx.arena();
        let all_def_ids: Vec<DefId> = arena
            .source_files()
            .flat_map(|sf| sf.defs.iter().copied())
            .collect();
        for def_id in all_def_ids {
            let arena = ctx.arena();
            let def_data = &arena[def_id];
            let location = def_data.location;
            match &def_data.kind {
                Def::TypeAlias { name, ty, .. } => {
                    let type_name = arena[*name].name.clone();
                    let type_info = TypeInfo::from_type_id(arena, *ty);
                    self.symbol_table
                        .register_type(&type_name, Some(type_info))
                        .unwrap_or_else(|_| {
                            self.errors.push(TypeCheckError::RegistrationFailed {
                                kind: RegistrationKind::Type,
                                name: type_name,
                                reason: None,
                                location,
                            });
                        });
                }
                Def::Struct {
                    name,
                    vis,
                    fields,
                    methods,
                } => {
                    let struct_name = arena[*name].name.clone();
                    let field_infos: Vec<(String, TypeInfo, Visibility)> = fields
                        .iter()
                        .map(|f| {
                            (
                                arena[f.name].name.clone(),
                                TypeInfo::from_type_id(arena, f.ty),
                                Visibility::Private,
                            )
                        })
                        .collect();
                    {
                        let mut seen_fields = FxHashSet::default();
                        for field in fields {
                            let field_name = arena[field.name].name.clone();
                            if !seen_fields.insert(field_name.clone()) {
                                self.errors.push(
                                    TypeCheckError::DuplicateStructFieldDefinition {
                                        struct_name: struct_name.clone(),
                                        field_name,
                                        location: arena[field.name].location,
                                    },
                                );
                            }
                        }
                    }
                    let method_ids: Vec<DefId> = methods.clone();
                    self.symbol_table
                        .register_struct(&struct_name, &field_infos, vec![], vis.clone())
                        .unwrap_or_else(|_| {
                            self.errors.push(TypeCheckError::RegistrationFailed {
                                kind: RegistrationKind::Struct,
                                name: struct_name.clone(),
                                reason: None,
                                location,
                            });
                        });

                    for method_id in method_ids {
                        let arena = ctx.arena();
                        let method_data = &arena[method_id];
                        let method_location = method_data.location;
                        if let Def::Function {
                            name: method_name,
                            vis: method_vis,
                            type_params,
                            args,
                            returns,
                            ..
                        } = &method_data.kind
                        {
                            let has_self = args
                                .iter()
                                .any(|a| matches!(a.kind, ArgKind::SelfRef { .. }));

                            let tp_names: Vec<String> =
                                type_params.iter().map(|p| arena[*p].name.clone()).collect();
                            let param_types: Vec<TypeInfo> = args
                                .iter()
                                .filter_map(|a| match &a.kind {
                                    ArgKind::SelfRef { .. } => None,
                                    ArgKind::Named { ty, .. }
                                    | ArgKind::Ignored { ty }
                                    | ArgKind::TypeOnly(ty) => {
                                        Some(self.symbol_table.resolve_custom_type(
                                            TypeInfo::from_type_id_with_type_params(
                                                arena, *ty, &tp_names,
                                            ),
                                        ))
                                    }
                                })
                                .collect();

                            let return_type = returns
                                .map(|r| {
                                    TypeInfo::from_type_id_with_type_params(arena, r, &tp_names)
                                })
                                .map(|ti| self.symbol_table.resolve_custom_type(ti))
                                .unwrap_or_default();

                            let definition_scope_id =
                                self.symbol_table.current_scope_id().unwrap_or(0);
                            let m_name = arena[*method_name].name.clone();
                            let signature = FuncInfo {
                                name: m_name.clone(),
                                type_params: tp_names,
                                param_types,
                                return_type,
                                visibility: method_vis.clone(),
                                definition_scope_id,
                            };

                            self.symbol_table
                                .register_method(
                                    &struct_name,
                                    signature,
                                    method_vis.clone(),
                                    has_self,
                                )
                                .unwrap_or_else(|err| {
                                    self.errors.push(TypeCheckError::RegistrationFailed {
                                        kind: RegistrationKind::Method,
                                        name: format!("{struct_name}::{m_name}"),
                                        reason: Some(err.to_string()),
                                        location: method_location,
                                    });
                                });
                        }
                    }
                }
                Def::Enum {
                    name,
                    vis,
                    variants,
                } => {
                    let enum_name = arena[*name].name.clone();
                    let variant_names: Vec<&str> =
                        variants.iter().map(|v| arena[*v].name.as_str()).collect();
                    {
                        let mut seen_variants = FxHashSet::default();
                        for variant_id in variants {
                            let variant_name = arena[*variant_id].name.as_str();
                            if !seen_variants.insert(variant_name) {
                                self.errors.push(TypeCheckError::DuplicateEnumVariant {
                                    enum_name: enum_name.clone(),
                                    variant_name: variant_name.to_string(),
                                    location: arena[*variant_id].location,
                                });
                            }
                        }
                    }
                    self.symbol_table
                        .register_enum(&enum_name, &variant_names, vis.clone())
                        .unwrap_or_else(|_| {
                            self.errors.push(TypeCheckError::RegistrationFailed {
                                kind: RegistrationKind::Enum,
                                name: enum_name,
                                reason: None,
                                location,
                            });
                        });
                }
                Def::Spec { name, .. } => {
                    let spec_name = arena[*name].name.clone();
                    self.symbol_table
                        .register_spec(&spec_name)
                        .unwrap_or_else(|_| {
                            self.errors.push(TypeCheckError::RegistrationFailed {
                                kind: RegistrationKind::Spec,
                                name: spec_name,
                                reason: None,
                                location,
                            });
                        });
                }
                Def::Constant { .. }
                | Def::Function { .. }
                | Def::ExternFunction { .. }
                | Def::Module { .. } => {}
            }
        }

        self.check_recursive_struct_definitions(ctx);
    }

    /// Detects recursive struct definitions (direct or transitive cycles).
    ///
    /// A struct that contains itself (or contains another struct that eventually
    /// references it) would have infinite size and cannot be laid out in memory.
    /// This traverses into nested containers (specs and modules) so that structs
    /// defined inside them are also checked.
    fn check_recursive_struct_definitions(&mut self, ctx: &TypedContext) {
        let arena = ctx.arena();
        let all_def_ids: Vec<DefId> = arena
            .source_files()
            .flat_map(|sf| sf.defs.iter().copied())
            .collect();
        self.check_recursive_structs_in_defs(arena, &all_def_ids);
    }

    fn check_recursive_structs_in_defs(&mut self, arena: &AstArena, def_ids: &[DefId]) {
        for &def_id in def_ids {
            match &arena[def_id].kind {
                Def::Struct { name, fields, .. } => {
                    let struct_name = arena[*name].name.clone();
                    for field in fields {
                        let field_name = arena[field.name].name.clone();
                        let field_type = TypeInfo::from_type_id(arena, field.ty);
                        let resolved = self.symbol_table.resolve_custom_type(field_type);
                        if self.struct_type_contains(
                            &resolved.kind,
                            &struct_name,
                            &mut FxHashSet::default(),
                        ) {
                            self.errors.push(TypeCheckError::RecursiveStructDefinition {
                                struct_name: struct_name.clone(),
                                field_name,
                                field_type: resolved.to_string(),
                                location: arena[field.name].location,
                            });
                        }
                    }
                }
                Def::Spec { defs, .. } => {
                    self.check_recursive_structs_in_defs(arena, defs);
                }
                Def::Module {
                    defs: Some(defs), ..
                } => {
                    self.check_recursive_structs_in_defs(arena, defs);
                }
                _ => {}
            }
        }
    }

    /// Returns true if `kind` is or transitively contains struct `target_name`.
    fn struct_type_contains(
        &self,
        kind: &TypeInfoKind,
        target_name: &str,
        visited: &mut FxHashSet<String>,
    ) -> bool {
        match kind {
            TypeInfoKind::Struct(name) | TypeInfoKind::Custom(name) => {
                if name == target_name {
                    return true;
                }
                if !visited.insert(name.clone()) {
                    return false;
                }
                if let Some(info) = self.symbol_table.lookup_struct(name) {
                    info.fields.iter().any(|f| {
                        self.struct_type_contains(&f.type_info.kind, target_name, visited)
                    })
                } else if let Some(resolved) = self.symbol_table.lookup_type(name) {
                    self.struct_type_contains(&resolved.kind, target_name, visited)
                } else {
                    false
                }
            }
            TypeInfoKind::Array(elem, _) => {
                self.struct_type_contains(&elem.kind, target_name, visited)
            }
            _ => false,
        }
    }

    /// Registers `Def::Function`, `Def::ExternFunction`, and `Def::Constant`
    #[allow(clippy::too_many_lines)]
    fn collect_function_and_constant_definitions(&mut self, ctx: &mut TypedContext) {
        let arena = ctx.arena();
        let all_def_ids: Vec<DefId> = arena
            .source_files()
            .flat_map(|sf| sf.defs.iter().copied())
            .collect();
        for def_id in all_def_ids {
            let (location, kind) = {
                let arena = ctx.arena();
                let def_data = &arena[def_id];
                (def_data.location, def_data.kind.clone())
            };
            match &kind {
                Def::Constant {
                    name, ty, value, ..
                } => {
                    let const_name = ctx.arena()[*name].name.clone();
                    let const_type = self
                        .symbol_table
                        .resolve_custom_type(TypeInfo::from_type_id(ctx.arena(), *ty));
                    let value_id = *value;
                    if let Err(err) = self.symbol_table.push_variable_to_scope(
                        &const_name,
                        const_type.clone(),
                        false,
                    ) {
                        self.errors.push(TypeCheckError::RegistrationFailed {
                            kind: RegistrationKind::Variable,
                            name: const_name,
                            reason: Some(err.to_string()),
                            location,
                        });
                    }
                    self.check_const_initializer(value_id, &const_type, location, ctx);
                }
                Def::Function {
                    name,
                    type_params,
                    args,
                    returns,
                    ..
                } => {
                    let func_name = ctx.arena()[*name].name.clone();
                    let name_ident_id = *name;
                    let tp_names: Vec<String> = type_params
                        .iter()
                        .map(|p| ctx.arena()[*p].name.clone())
                        .collect();

                    for arg in args {
                        match &arg.kind {
                            ArgKind::SelfRef { .. } => {
                                self.errors.push(TypeCheckError::SelfReferenceInFunction {
                                    function_name: func_name.clone(),
                                    location: arg.location,
                                });
                            }
                            ArgKind::Ignored { ty } => {
                                self.validate_type(ctx.arena(), *ty, &tp_names);
                            }
                            ArgKind::Named {
                                name: arg_name, ty, ..
                            } => {
                                self.validate_type(ctx.arena(), *ty, &tp_names);
                                let type_info = TypeInfo::from_type_id_with_type_params(
                                    ctx.arena(),
                                    *ty,
                                    &tp_names,
                                );
                                ctx.set_node_typeinfo(NodeId::Ident(*arg_name), type_info);
                            }
                            ArgKind::TypeOnly(ty) => {
                                self.validate_type(ctx.arena(), *ty, &tp_names);
                            }
                        }
                    }
                    ctx.set_node_typeinfo(
                        NodeId::Ident(name_ident_id),
                        TypeInfo {
                            kind: TypeInfoKind::Function(func_name.clone()),
                            type_params: tp_names.clone(),
                        },
                    );
                    if let Some(return_type_id) = returns {
                        self.validate_type(ctx.arena(), *return_type_id, &tp_names);
                        let return_type_info = TypeInfo::from_type_id_with_type_params(
                            ctx.arena(),
                            *return_type_id,
                            &tp_names,
                        );
                        ctx.set_node_typeinfo(NodeId::Type(*return_type_id), return_type_info);
                    }
                    // Register function even if parameter validation had errors
                    let param_types: Vec<TypeInfo> = args
                        .iter()
                        .filter_map(|a| match &a.kind {
                            ArgKind::SelfRef { .. } => None,
                            ArgKind::Named { ty, .. }
                            | ArgKind::Ignored { ty }
                            | ArgKind::TypeOnly(ty) => {
                                Some(TypeInfo::from_type_id_with_type_params(
                                    ctx.arena(),
                                    *ty,
                                    &tp_names,
                                ))
                            }
                        })
                        .collect();
                    let return_type = returns
                        .map(|r| TypeInfo::from_type_id_with_type_params(ctx.arena(), r, &tp_names))
                        .unwrap_or_default();
                    if let Err(err) = self.symbol_table.register_function(
                        &func_name,
                        tp_names,
                        param_types,
                        return_type,
                    ) {
                        self.errors.push(TypeCheckError::RegistrationFailed {
                            kind: RegistrationKind::Function,
                            name: func_name,
                            reason: Some(err),
                            location,
                        });
                    }
                }
                Def::ExternFunction {
                    name,
                    args,
                    returns,
                    ..
                } => {
                    let func_name = ctx.arena()[*name].name.clone();
                    let param_types: Vec<TypeInfo> = args
                        .iter()
                        .filter_map(|a| match &a.kind {
                            ArgKind::SelfRef { .. } => None,
                            ArgKind::Named { ty, .. }
                            | ArgKind::Ignored { ty }
                            | ArgKind::TypeOnly(ty) => {
                                Some(TypeInfo::from_type_id(ctx.arena(), *ty))
                            }
                        })
                        .collect();
                    let return_type = returns
                        .map(|r| TypeInfo::from_type_id(ctx.arena(), r))
                        .unwrap_or_default();
                    if let Err(err) = self.symbol_table.register_function(
                        &func_name,
                        vec![],
                        param_types,
                        return_type,
                    ) {
                        self.errors.push(TypeCheckError::RegistrationFailed {
                            kind: RegistrationKind::Function,
                            name: func_name,
                            reason: Some(err),
                            location,
                        });
                    }
                }
                Def::Spec { .. }
                | Def::Struct { .. }
                | Def::Enum { .. }
                | Def::TypeAlias { .. }
                | Def::Module { .. } => {}
            }
        }
    }

    /// Type-checks a constant initializer expression against the declared type.
    ///
    /// If the initializer is a number literal matching a numeric target type,
    /// sets the expression's type info directly. Otherwise, infers the expression
    /// type and reports a mismatch if it doesn't match. Only sets node type info
    /// when types are compatible.
    fn check_const_initializer(
        &mut self,
        value_id: ExprId,
        const_type: &TypeInfo,
        location: Location,
        ctx: &mut TypedContext,
    ) {
        let value_kind = ctx.arena()[value_id].kind.clone();
        let mut type_ok = false;
        if let Expr::NumberLiteral { .. } = value_kind {
            if const_type.kind.is_number() {
                type_ok = true;
            } else {
                self.errors.push(TypeCheckError::TypeMismatch {
                    expected: const_type.clone(),
                    found: TypeInfo {
                        kind: TypeInfoKind::Number(NumberType::I32),
                        type_params: vec![],
                    },
                    context: TypeMismatchContext::VariableDefinition,
                    location,
                });
            }
        } else {
            let init_type = self.infer_expression(value_id, ctx);
            match init_type {
                Some(init)
                    if self.symbol_table.resolve_custom_type(init.clone()) != *const_type =>
                {
                    self.errors.push(TypeCheckError::TypeMismatch {
                        expected: const_type.clone(),
                        found: init,
                        context: TypeMismatchContext::VariableDefinition,
                        location,
                    });
                }
                Some(_) => {
                    type_ok = true;
                }
                None => {}
            }
        }
        if type_ok {
            ctx.set_node_typeinfo(NodeId::Expr(value_id), const_type.clone());
        }
    }

    /// Validates that a type reference is well-formed.
    ///
    /// Checks that:
    /// - Custom types exist in the symbol table
    /// - Generic type parameters are declared or known types
    /// - Array element types are valid
    ///
    /// Primitive builtin types represented by `TypeNode::Simple(SimpleTypeKind)` are
    /// always valid and require no symbol table lookup. This includes unit, bool,
    /// and numeric types (i8, i16, i32, i64, u8, u16, u32, u64).
    fn validate_type(&mut self, arena: &AstArena, ty_id: TypeId, type_param_names: &[String]) {
        let type_data = &arena[ty_id];
        let location = type_data.location;
        match &type_data.kind {
            TypeNode::Array { element, size } => {
                self.validate_type(arena, *element, type_param_names);
                self.validate_array_size(arena, *size, location);
            }
            TypeNode::Simple(_) => {
                // SimpleTypeKind only contains primitive builtin types - always valid.
            }
            TypeNode::Generic { base, params } => {
                let base_name = arena[*base].name.clone();
                if self.symbol_table.lookup_type(&base_name).is_none() {
                    self.push_error_dedup(TypeCheckError::UnknownType {
                        name: base_name,
                        location: arena[*base].location,
                    });
                }
                for param in params {
                    let param_name = arena[*param].name.clone();
                    if !type_param_names.contains(&param_name)
                        && self.symbol_table.lookup_type(&param_name).is_none()
                    {
                        self.push_error_dedup(TypeCheckError::UnknownType {
                            name: param_name,
                            location: arena[*param].location,
                        });
                    }
                }
            }
            TypeNode::Function { .. }
            | TypeNode::QualifiedName { .. }
            | TypeNode::Qualified { .. } => {}
            TypeNode::Custom(ident_id) => {
                let name = arena[*ident_id].name.clone();
                if type_param_names.contains(&name) {
                    return;
                }
                if self.symbol_table.lookup_type(&name).is_none() {
                    self.push_error_dedup(TypeCheckError::UnknownType {
                        name,
                        location: arena[*ident_id].location,
                    });
                }
            }
        }
    }

    /// Validates that an array type annotation has a valid size (positive, fits in u32).
    ///
    /// Reports `InvalidArraySize` if the size is zero (sentinel from parse failure)
    /// or if the literal text cannot be parsed as a positive u32.
    fn validate_array_size(
        &mut self,
        arena: &AstArena,
        size_expr_id: ExprId,
        type_location: Location,
    ) {
        let expr_data = &arena[size_expr_id];
        if let Expr::NumberLiteral { value } = &expr_data.kind {
            match value.parse::<u32>() {
                Ok(1..) => {}
                Ok(0) | Err(_) => {
                    self.push_error_dedup(TypeCheckError::InvalidArraySize {
                        size: value.clone(),
                        location: type_location,
                    });
                }
            }
        }
    }

    fn infer_variables(&mut self, def_id: DefId, ctx: &mut TypedContext) {
        let arena = ctx.arena();
        let def_data = &arena[def_id];
        let Def::Function {
            type_params,
            args,
            returns,
            body,
            ..
        } = &def_data.kind
        else {
            return;
        };
        let tp_names: Vec<String> = type_params.iter().map(|p| arena[*p].name.clone()).collect();
        let args_snapshot: Vec<_> = args.clone();
        let returns_snapshot = *returns;
        let body_id = *body;

        self.symbol_table.push_scope();

        for arg in &args_snapshot {
            match &arg.kind {
                ArgKind::Named {
                    name: arg_name,
                    ty,
                    is_mut,
                } => {
                    let arena = ctx.arena();
                    let arg_type = self.symbol_table.resolve_custom_type(
                        TypeInfo::from_type_id_with_type_params(arena, *ty, &tp_names),
                    );
                    let name_str = arena[*arg_name].name.clone();
                    if let Err(err) = self
                        .symbol_table
                        .push_variable_to_scope(&name_str, arg_type, *is_mut)
                    {
                        self.errors.push(TypeCheckError::RegistrationFailed {
                            kind: RegistrationKind::Variable,
                            name: name_str,
                            reason: Some(err.to_string()),
                            location: arg.location,
                        });
                    }
                }
                ArgKind::SelfRef { .. } => {
                    self.errors
                        .push(TypeCheckError::SelfReferenceOutsideMethod {
                            location: arg.location,
                        });
                }
                ArgKind::Ignored { .. } | ArgKind::TypeOnly(_) => {}
            }
        }

        let return_type = returns_snapshot
            .map(|r| TypeInfo::from_type_id_with_type_params(ctx.arena(), r, &tp_names))
            .map(|ti| self.symbol_table.resolve_custom_type(ti))
            .unwrap_or_default();

        self.current_type_params = tp_names;
        let stmts: Vec<StmtId> = ctx.arena()[body_id].stmts.clone();
        for stmt_id in stmts {
            self.infer_statement(stmt_id, &return_type, ctx);
        }
        self.current_type_params = Vec::new();
        self.symbol_table.pop_scope();
    }

    fn infer_method_variables(
        &mut self,
        method_def_id: DefId,
        self_type: TypeInfo,
        ctx: &mut TypedContext,
    ) {
        let arena = ctx.arena();
        let def_data = &arena[method_def_id];
        let Def::Function {
            args,
            returns,
            body,
            type_params,
            ..
        } = &def_data.kind
        else {
            return;
        };
        let tp_names: Vec<String> = type_params.iter().map(|p| arena[*p].name.clone()).collect();
        let args_snapshot: Vec<_> = args.clone();
        let returns_snapshot = *returns;
        let body_id = *body;

        self.symbol_table.push_scope();
        for arg in &args_snapshot {
            match &arg.kind {
                ArgKind::Named {
                    name: arg_name,
                    ty,
                    is_mut,
                } => {
                    let arena = ctx.arena();
                    let arg_type = self.symbol_table.resolve_custom_type(
                        TypeInfo::from_type_id_with_type_params(arena, *ty, &tp_names),
                    );
                    let name_str = arena[*arg_name].name.clone();
                    if let Err(err) = self
                        .symbol_table
                        .push_variable_to_scope(&name_str, arg_type, *is_mut)
                    {
                        self.errors.push(TypeCheckError::RegistrationFailed {
                            kind: RegistrationKind::Variable,
                            name: name_str,
                            reason: Some(err.to_string()),
                            location: arg.location,
                        });
                    }
                }
                ArgKind::SelfRef { is_mut } => {
                    if let Err(err) =
                        self.symbol_table
                            .push_variable_to_scope("self", self_type.clone(), *is_mut)
                    {
                        self.errors.push(TypeCheckError::RegistrationFailed {
                            kind: RegistrationKind::Variable,
                            name: "self".to_string(),
                            reason: Some(err.to_string()),
                            location: arg.location,
                        });
                    }
                }
                ArgKind::Ignored { .. } | ArgKind::TypeOnly(_) => {}
            }
        }

        let return_type = returns_snapshot
            .map(|r| TypeInfo::from_type_id_with_type_params(ctx.arena(), r, &tp_names))
            .map(|ti| self.symbol_table.resolve_custom_type(ti))
            .unwrap_or_default();

        self.current_type_params = tp_names;
        let stmts: Vec<StmtId> = ctx.arena()[body_id].stmts.clone();
        for stmt_id in stmts {
            self.infer_statement(stmt_id, &return_type, ctx);
        }
        self.current_type_params = Vec::new();
        self.symbol_table.pop_scope();
    }

    #[allow(clippy::too_many_lines)]
    fn infer_statement(&mut self, stmt_id: StmtId, return_type: &TypeInfo, ctx: &mut TypedContext) {
        let arena = ctx.arena();
        let stmt_data = &arena[stmt_id];
        let location = stmt_data.location;
        // Clone the kind to avoid holding borrow on arena across mutable calls
        let kind = stmt_data.kind.clone();
        match kind {
            Stmt::Assign { left, right } => {
                if let Some(name) = self.extract_root_variable_name(ctx.arena(), left) {
                    if let Some(false) = self.symbol_table.lookup_variable_is_mut(&name) {
                        self.errors
                            .push(TypeCheckError::AssignToImmutable { name, location });
                    }
                } else {
                    self.errors
                        .push(TypeCheckError::InvalidAssignmentTarget { location });
                }
                let target_type = self.infer_expression(left, ctx);
                {
                    let right_kind = ctx.arena()[right].kind.clone();
                    if let Some(target) = &target_type
                        && let Expr::NumberLiteral { .. } = &right_kind
                    {
                        if target.kind.is_number() {
                            ctx.set_node_typeinfo(NodeId::Expr(right), target.clone());

                        } else {
                            self.errors.push(TypeCheckError::TypeMismatch {
                                expected: target.clone(),
                                found: TypeInfo {
                                    kind: TypeInfoKind::Number(NumberType::I32),
                                    type_params: vec![],
                                },
                                context: TypeMismatchContext::Assignment,
                                location,
                            });
                        }
                    }
                }
                let arena = ctx.arena();
                if let Expr::Uzumaki = &arena[right].kind {
                    if let Some(target) = &target_type {
                        ctx.set_node_typeinfo(NodeId::Expr(right), target.clone());
                    } else {
                        cov_mark::hit!(type_checker_uzumaki_cannot_infer_type);
                        self.errors.push(TypeCheckError::CannotInferUzumakiType {
                            location: ctx.arena()[right].location,
                        });
                    }
                } else {
                    let value_type = self.infer_expression(right, ctx);
                    // Compound-return-in-assignment check moved to analysis rule A017.
                    if let (Some(target), Some(val)) = (target_type, value_type)
                        && target != val
                    {
                        self.errors.push(TypeCheckError::TypeMismatch {
                            expected: target,
                            found: val,
                            context: TypeMismatchContext::Assignment,
                            location,
                        });
                    }
                }
            }
            Stmt::Block(block_id) => {
                self.symbol_table.push_scope();
                let stmts: Vec<StmtId> = ctx.arena()[block_id].stmts.clone();
                for s in stmts {
                    self.infer_statement(s, return_type, ctx);
                }
                self.symbol_table.pop_scope();
            }
            Stmt::Expr(expr_id) => {
                // Compound-return-in-expression-position check moved to analysis rule A016.
                self.infer_expression(expr_id, ctx);
            }
            Stmt::Return { expr } => {
                if let Expr::Uzumaki = &ctx.arena()[expr].kind {
                    ctx.set_node_typeinfo(NodeId::Expr(expr), return_type.clone());
                } else {
                    let value_type = self.infer_expression(expr, ctx);
                    if *return_type != value_type.clone().unwrap_or_default() {
                        self.errors.push(TypeCheckError::TypeMismatch {
                            expected: return_type.clone(),
                            found: value_type.unwrap_or_default(),
                            context: TypeMismatchContext::Return,
                            location,
                        });
                    }
                }
            }
            Stmt::Loop { condition, body } => {
                if let Some(condition_expr_id) = condition {
                    let condition_type = self.infer_expression(condition_expr_id, ctx);
                    if condition_type.is_none()
                        || condition_type.as_ref().unwrap().kind != TypeInfoKind::Bool
                    {
                        self.errors.push(TypeCheckError::TypeMismatch {
                            expected: TypeInfo::boolean(),
                            found: condition_type.unwrap_or_default(),
                            context: TypeMismatchContext::Condition,
                            location,
                        });
                    }
                }
                self.symbol_table.push_scope();
                let stmts: Vec<StmtId> = ctx.arena()[body].stmts.clone();
                for s in stmts {
                    self.infer_statement(s, return_type, ctx);
                }
                self.symbol_table.pop_scope();
            }
            Stmt::Break => {}
            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                let condition_type = self.infer_expression(condition, ctx);
                if condition_type.is_none()
                    || condition_type.as_ref().unwrap().kind != TypeInfoKind::Bool
                {
                    self.errors.push(TypeCheckError::TypeMismatch {
                        expected: TypeInfo::boolean(),
                        found: condition_type.unwrap_or_default(),
                        context: TypeMismatchContext::Condition,
                        location,
                    });
                }

                self.symbol_table.push_scope();
                let then_stmts: Vec<StmtId> = ctx.arena()[then_block].stmts.clone();
                for s in then_stmts {
                    self.infer_statement(s, return_type, ctx);
                }
                self.symbol_table.pop_scope();
                if let Some(else_block_id) = else_block {
                    self.symbol_table.push_scope();
                    let else_stmts: Vec<StmtId> = ctx.arena()[else_block_id].stmts.clone();
                    for s in else_stmts {
                        self.infer_statement(s, return_type, ctx);
                    }
                    self.symbol_table.pop_scope();
                }
            }
            Stmt::VarDef {
                name,
                ty,
                value,
                is_mut,
            } => {
                let arena = ctx.arena();
                let var_name = arena[name].name.clone();
                let tp = self.current_type_params.clone();
                self.validate_type(arena, ty, &tp);
                let target_type = self
                    .symbol_table
                    .resolve_custom_type(TypeInfo::from_type_id_with_type_params(arena, ty, &tp));
                // Validate array size if applicable
                if let TypeNode::Array { size, .. } = &arena[ty].kind {
                    self.validate_array_size(ctx.arena(), *size, ctx.arena()[ty].location);
                }
                if let Some(expr_id) = value {
                    let expr_kind = ctx.arena()[expr_id].kind.clone();
                    if let Expr::NumberLiteral { .. } = expr_kind
                    {
                        if target_type.kind.is_number() {
                            ctx.set_node_typeinfo(NodeId::Expr(expr_id), target_type.clone());

                        } else {
                            self.errors.push(TypeCheckError::TypeMismatch {
                                expected: target_type.clone(),
                                found: TypeInfo {
                                    kind: TypeInfoKind::Number(NumberType::I32),
                                    type_params: vec![],
                                },
                                context: TypeMismatchContext::VariableDefinition,
                                location,
                            });
                        }
                    }
                    if let Expr::ArrayLiteral { elements } = &expr_kind
                        && let TypeInfoKind::Array(ref elem_type, expected_size) = target_type.kind
                    {
                        if elements.len() != expected_size as usize {
                            self.errors.push(TypeCheckError::ArrayLiteralSizeMismatch {
                                expected: expected_size,
                                actual: elements.len(),
                                location,
                            });
                        }
                        let elems: Vec<ExprId> = elements.clone();
                        for elem_id in elems {
                            let el_kind = ctx.arena()[elem_id].kind.clone();
                            if let Expr::NumberLiteral { .. } = el_kind {
                                ctx.set_node_typeinfo(NodeId::Expr(elem_id), (**elem_type).clone());
                            }
                        }
                    }
                    let arena = ctx.arena();
                    if let Expr::Uzumaki = &arena[expr_id].kind {
                        ctx.set_node_typeinfo(NodeId::Expr(expr_id), target_type.clone());
                    } else if let Some(init_type) = self.infer_expression(expr_id, ctx)
                        && self.symbol_table.resolve_custom_type(init_type.clone())
                            != target_type
                    {
                        self.errors.push(TypeCheckError::TypeMismatch {
                            expected: target_type.clone(),
                            found: init_type,
                            context: TypeMismatchContext::VariableDefinition,
                            location,
                        });
                    }
                }
                if self
                    .symbol_table
                    .lookup_variable_in_parent_scopes(&var_name)
                    .is_some()
                {
                    self.errors.push(TypeCheckError::VariableShadowed {
                        name: var_name.clone(),
                        location,
                    });
                }
                if let Err(err) =
                    self.symbol_table
                        .push_variable_to_scope(&var_name, target_type.clone(), is_mut)
                {
                    self.errors.push(TypeCheckError::RegistrationFailed {
                        kind: RegistrationKind::Variable,
                        name: var_name,
                        reason: Some(err.to_string()),
                        location,
                    });
                }
                ctx.set_node_typeinfo(NodeId::Ident(name), target_type.clone());
                ctx.set_node_typeinfo(NodeId::Stmt(stmt_id), target_type);
            }
            Stmt::TypeDef { name, ty } => {
                let arena = ctx.arena();
                let type_name = arena[name].name.clone();
                let type_info = TypeInfo::from_type_id(arena, ty);
                if let Err(err) = self.symbol_table.register_type(&type_name, Some(type_info)) {
                    self.errors.push(TypeCheckError::RegistrationFailed {
                        kind: RegistrationKind::Type,
                        name: type_name,
                        reason: Some(err.to_string()),
                        location,
                    });
                }
            }
            Stmt::Assert { expr } => {
                let condition_type = self.infer_expression(expr, ctx);
                if condition_type.is_none()
                    || condition_type.as_ref().unwrap().kind != TypeInfoKind::Bool
                {
                    self.errors.push(TypeCheckError::TypeMismatch {
                        expected: TypeInfo::boolean(),
                        found: condition_type.unwrap_or_default(),
                        context: TypeMismatchContext::Condition,
                        location,
                    });
                }
            }
            Stmt::ConstDef(ref const_def_id) => {
                let cdi = *const_def_id;
                let arena = ctx.arena();
                if let Def::Constant {
                    name, ty, value, ..
                } = &arena[cdi].kind
                {
                    let const_name = arena[*name].name.clone();
                    let constant_type = self
                        .symbol_table
                        .resolve_custom_type(TypeInfo::from_type_id(arena, *ty));
                    let value_id = *value;
                    if self
                        .symbol_table
                        .lookup_variable_in_parent_scopes(&const_name)
                        .is_some()
                    {
                        self.errors.push(TypeCheckError::VariableShadowed {
                            name: const_name.clone(),
                            location,
                        });
                    }
                    if let Err(err) = self.symbol_table.push_variable_to_scope(
                        &const_name,
                        constant_type.clone(),
                        false,
                    ) {
                        self.errors.push(TypeCheckError::RegistrationFailed {
                            kind: RegistrationKind::Variable,
                            name: const_name,
                            reason: Some(err.to_string()),
                            location,
                        });
                    }
                    self.check_const_initializer(value_id, &constant_type, location, ctx);
                    ctx.set_node_typeinfo(NodeId::Def(cdi), constant_type.clone());
                    ctx.set_node_typeinfo(NodeId::Stmt(stmt_id), constant_type);
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn infer_expression(&mut self, expr_id: ExprId, ctx: &mut TypedContext) -> Option<TypeInfo> {
        let arena = ctx.arena();
        let expr_data = &arena[expr_id];
        let location = expr_data.location;
        let kind = expr_data.kind.clone();
        match kind {
            Expr::ArrayIndexAccess { array, index } => {
                if let Some(type_info) = ctx.get_node_typeinfo(NodeId::Expr(expr_id)) {
                    Some(type_info)
                } else if let Some(array_type) = self.infer_expression(array, ctx) {
                    // Compound-return and 64-bit index checks moved to analysis (A016, A019).
                    if let Some(index_type) = self.infer_expression(index, ctx)
                        && !index_type.is_number()
                    {
                        self.errors.push(TypeCheckError::ArrayIndexNotNumeric {
                            found: index_type,
                            location,
                        });
                    }
                    match &array_type.kind {
                        TypeInfoKind::Array(element_type, _) => {
                            ctx.set_node_typeinfo(NodeId::Expr(expr_id), (**element_type).clone());
                            Some((**element_type).clone())
                        }
                        _ => {
                            self.errors.push(TypeCheckError::ExpectedArrayType {
                                found: array_type,
                                location,
                            });
                            None
                        }
                    }
                } else {
                    None
                }
            }
            Expr::MemberAccess { expr, name } => {
                if let Some(type_info) = ctx.get_node_typeinfo(NodeId::Expr(expr_id)) {
                    Some(type_info)
                } else if let Some(object_type) = self.infer_expression(expr, ctx) {
                    // Compound-return-in-expression-position check moved to analysis rule A016.
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
                        let field_name = ctx.arena()[name].name.clone();
                        if let Some(struct_info) = self.symbol_table.lookup_struct(&struct_name) {
                            if let Some(field_info) =
                                struct_info.get_field_info_by_name(&field_name)
                            {
                                self.check_and_report_visibility(
                                    &field_info.visibility,
                                    struct_info.definition_scope_id,
                                    &location,
                                    VisibilityContext::Field {
                                        struct_name: struct_name.clone(),
                                        field_name: field_name.clone(),
                                    },
                                );
                                let field_type = field_info.type_info.clone();
                                ctx.set_node_typeinfo(NodeId::Expr(expr_id), field_type.clone());
                                Some(field_type)
                            } else {
                                self.errors.push(TypeCheckError::FieldNotFound {
                                    struct_name,
                                    field_name,
                                    location,
                                });
                                None
                            }
                        } else {
                            self.errors.push(TypeCheckError::FieldNotFound {
                                struct_name,
                                field_name,
                                location,
                            });
                            None
                        }
                    } else {
                        self.errors.push(TypeCheckError::ExpectedStructType {
                            found: object_type,
                            location,
                        });
                        None
                    }
                } else {
                    None
                }
            }
            Expr::TypeMemberAccess {
                expr: inner_expr,
                name,
            } => {
                if let Some(type_info) = ctx.get_node_typeinfo(NodeId::Expr(expr_id)) {
                    return Some(type_info);
                }

                let arena = ctx.arena();
                let enum_name = match &arena[inner_expr].kind {
                    Expr::Type(ty_id) => {
                        let type_data = &arena[*ty_id];
                        match &type_data.kind {
                            TypeNode::Custom(ident_id) => arena[*ident_id].name.clone(),
                            _ => {
                                let type_info = TypeInfo::from_type_id(arena, *ty_id);
                                self.errors.push(TypeCheckError::ExpectedEnumType {
                                    found: type_info,
                                    location,
                                });
                                return None;
                            }
                        }
                    }
                    Expr::Identifier(ident_id) => arena[*ident_id].name.clone(),
                    _ => {
                        if let Some(expr_type) = self.infer_expression(inner_expr, ctx) {
                            match &expr_type.kind {
                                TypeInfoKind::Enum(name) => name.clone(),
                                _ => {
                                    self.errors.push(TypeCheckError::ExpectedEnumType {
                                        found: expr_type,
                                        location,
                                    });
                                    return None;
                                }
                            }
                        } else {
                            return None;
                        }
                    }
                };

                let variant_name = ctx.arena()[name].name.clone();

                if let Some(enum_info) = self.symbol_table.lookup_enum(&enum_name) {
                    if enum_info.variants.contains(&variant_name) {
                        self.check_and_report_visibility(
                            &enum_info.visibility,
                            enum_info.definition_scope_id,
                            &location,
                            VisibilityContext::Enum {
                                name: enum_name.clone(),
                            },
                        );
                        let enum_type = TypeInfo {
                            kind: TypeInfoKind::Enum(enum_name),
                            type_params: vec![],
                        };
                        ctx.set_node_typeinfo(NodeId::Expr(expr_id), enum_type.clone());
                        Some(enum_type)
                    } else {
                        cov_mark::hit!(type_checker_variant_not_found);
                        self.errors.push(TypeCheckError::VariantNotFound {
                            enum_name,
                            variant_name,
                            location,
                        });
                        None
                    }
                } else {
                    self.push_error_dedup(TypeCheckError::UndefinedEnum {
                        name: enum_name,
                        location,
                    });
                    None
                }
            }
            Expr::FunctionCall {
                function,
                type_params: call_type_params,
                args,
            } => self.infer_function_call(expr_id, function, &call_type_params, &args, ctx),
            Expr::StructLiteral { name, fields } => {
                if let Some(type_info) = ctx.get_node_typeinfo(NodeId::Expr(expr_id)) {
                    return Some(type_info);
                }
                // Compound-literal-position check moved to analysis rule A015.
                let struct_name = ctx.arena()[name].name.clone();
                let struct_type = self.symbol_table.lookup_type(&struct_name);
                if let Some(struct_type) = struct_type {
                    if let Some(struct_info) = self.symbol_table.lookup_struct(&struct_name) {
                        let fields_copy: Vec<_> =
                            fields.iter().map(|(id, expr)| (*id, *expr)).collect();
                        let mut seen_fields = FxHashSet::default();
                        for (field_name_id, field_value_expr) in &fields_copy {
                            let field_name = ctx.arena()[*field_name_id].name.clone();
                            let field_loc = ctx.arena()[*field_name_id].location;
                            if !seen_fields.insert(field_name.clone()) {
                                self.errors
                                    .push(TypeCheckError::DuplicateStructField {
                                        struct_name: struct_name.clone(),
                                        field_name,
                                        location: field_loc,
                                    });
                                continue;
                            }
                            if let Some(field_info) =
                                struct_info.get_field_info_by_name(&field_name)
                            {
                                let field_type = field_info.type_info.clone();
                                let (field_expr_kind, field_expr_loc) = {
                                    let arena = ctx.arena();
                                    (
                                        arena[*field_value_expr].kind.clone(),
                                        arena[*field_value_expr].location,
                                    )
                                };
                                if let Expr::NumberLiteral { .. } = field_expr_kind
                                {
                                    if field_type.kind.is_number() {
                                        ctx.set_node_typeinfo(
                                            NodeId::Expr(*field_value_expr),
                                            field_type.clone(),
                                        );
                                    } else {
                                        self.errors.push(TypeCheckError::TypeMismatch {
                                            expected: field_type.clone(),
                                            found: TypeInfo {
                                                kind: TypeInfoKind::Number(NumberType::I32),
                                                type_params: vec![],
                                            },
                                            context: TypeMismatchContext::VariableDefinition,
                                            location: field_expr_loc,
                                        });
                                    }
                                } else {
            
                                    let init_type =
                                        self.infer_expression(*field_value_expr, ctx);
                                    if let Some(init) = init_type
                                        && init != field_type
                                    {
                                        self.errors.push(TypeCheckError::TypeMismatch {
                                            expected: field_type.clone(),
                                            found: init,
                                            context: TypeMismatchContext::VariableDefinition,
                                            location: field_expr_loc,
                                        });
                                    }
                                }
                            } else {
                                self.errors.push(TypeCheckError::UnknownStructField {
                                    struct_name: struct_name.clone(),
                                    field_name,
                                    location: field_loc,
                                });
                            }
                        }
                        for field_info in &struct_info.fields {
                            if !seen_fields.contains(&field_info.name) {
                                self.errors.push(TypeCheckError::MissingStructField {
                                    struct_name: struct_name.clone(),
                                    field_name: field_info.name.clone(),
                                    location,
                                });
                            }
                        }
                    }
                    ctx.set_node_typeinfo(NodeId::Expr(expr_id), struct_type.clone());
                    return Some(struct_type);
                }
                self.push_error_dedup(TypeCheckError::UndefinedStruct {
                    name: struct_name,
                    location,
                });
                None
            }
            Expr::PrefixUnary { expr, op } => match op {
                UnaryOperatorKind::Not => {
                    let expression_type_op = self.infer_expression(expr, ctx);
                    if let Some(expression_type) = expression_type_op {
                        if expression_type.is_bool() {
                            ctx.set_node_typeinfo(NodeId::Expr(expr_id), expression_type.clone());
                            return Some(expression_type);
                        }
                        self.errors.push(TypeCheckError::InvalidUnaryOperand {
                            operator: UnaryOperatorKind::Not,
                            expected_type: "booleans",
                            found_type: expression_type,
                            location,
                        });
                    }
                    None
                }
                UnaryOperatorKind::Neg => {
                    let expression_type_op = self.infer_expression(expr, ctx);
                    if let Some(expression_type) = expression_type_op {
                        if expression_type.is_signed_integer() {
                            ctx.set_node_typeinfo(NodeId::Expr(expr_id), expression_type.clone());
                            return Some(expression_type);
                        }
                        self.errors.push(TypeCheckError::InvalidUnaryOperand {
                            operator: UnaryOperatorKind::Neg,
                            expected_type: "signed integers (i8, i16, i32, i64)",
                            found_type: expression_type,
                            location,
                        });
                    }
                    None
                }
                UnaryOperatorKind::BitNot => {
                    let expression_type_op = self.infer_expression(expr, ctx);
                    if let Some(expression_type) = expression_type_op {
                        if expression_type.is_number() {
                            ctx.set_node_typeinfo(NodeId::Expr(expr_id), expression_type.clone());
                            return Some(expression_type);
                        }
                        self.errors.push(TypeCheckError::InvalidUnaryOperand {
                            operator: UnaryOperatorKind::BitNot,
                            expected_type: "integers (i8, i16, i32, i64, u8, u16, u32, u64)",
                            found_type: expression_type,
                            location,
                        });
                    }
                    None
                }
            },
            Expr::Parenthesized { expr } => {
                let inner_type = self.infer_expression(expr, ctx);
                if let Some(ref type_info) = inner_type {
                    ctx.set_node_typeinfo(NodeId::Expr(expr_id), type_info.clone());
                }
                inner_type
            }
            Expr::Binary { left, right, op } => {
                if let Some(type_info) = ctx.get_node_typeinfo(NodeId::Expr(expr_id)) {
                    return Some(type_info);
                }
                let left_type = self.infer_expression(left, ctx);
                let right_type = self.infer_expression(right, ctx);
                // NOTE: Only detects division by literal zero (e.g., `x / 0`).
                // Constant expressions and const-declared zero values are not detected.
                if matches!(op, OperatorKind::Div | OperatorKind::Mod) {
                    let right_expr = &ctx.arena()[right].kind;
                    if let Expr::NumberLiteral { value } = right_expr
                        && value.parse::<i128>().ok() == Some(0)
                    {
                        self.errors.push(TypeCheckError::DivisionByZero {
                            location: ctx.arena()[right].location,
                        });
                    }
                }
                if let (Some(left_type), Some(right_type)) = (left_type, right_type) {
                    if left_type != right_type {
                        self.errors.push(TypeCheckError::BinaryOperandTypeMismatch {
                            operator: op.clone(),
                            left: left_type.clone(),
                            right: right_type.clone(),
                            location,
                        });
                    }
                    let res_type = match op {
                        OperatorKind::And | OperatorKind::Or => {
                            if left_type.is_bool() && right_type.is_bool() {
                                TypeInfo {
                                    kind: TypeInfoKind::Bool,
                                    type_params: vec![],
                                }
                            } else {
                                self.errors.push(TypeCheckError::InvalidBinaryOperand {
                                    operator: op.clone(),
                                    expected_kind: "logical",
                                    operand_desc: "non-boolean types",
                                    found_types: (left_type, right_type),
                                    location,
                                });
                                return None;
                            }
                        }
                        OperatorKind::Eq | OperatorKind::Ne => TypeInfo {
                            kind: TypeInfoKind::Bool,
                            type_params: vec![],
                        },
                        OperatorKind::Lt
                        | OperatorKind::Le
                        | OperatorKind::Gt
                        | OperatorKind::Ge => {
                            if !left_type.is_number() || !right_type.is_number() {
                                self.errors.push(TypeCheckError::InvalidBinaryOperand {
                                    operator: op.clone(),
                                    expected_kind: "comparison",
                                    operand_desc: "non-numeric types",
                                    found_types: (left_type, right_type),
                                    location,
                                });
                            }
                            TypeInfo {
                                kind: TypeInfoKind::Bool,
                                type_params: vec![],
                            }
                        }
                        OperatorKind::Pow
                        | OperatorKind::Add
                        | OperatorKind::Sub
                        | OperatorKind::Mul
                        | OperatorKind::Div
                        | OperatorKind::Mod
                        | OperatorKind::BitAnd
                        | OperatorKind::BitOr
                        | OperatorKind::BitXor
                        | OperatorKind::Shl
                        | OperatorKind::Shr => {
                            if !left_type.is_number() || !right_type.is_number() {
                                self.errors.push(TypeCheckError::InvalidBinaryOperand {
                                    operator: op.clone(),
                                    expected_kind: "arithmetic",
                                    operand_desc: "non-number types",
                                    found_types: (left_type.clone(), right_type.clone()),
                                    location,
                                });
                            }
                            left_type.clone()
                        }
                    };
                    ctx.set_node_typeinfo(NodeId::Expr(expr_id), res_type.clone());
                    Some(res_type)
                } else {
                    None
                }
            }
            Expr::ArrayLiteral { elements } => {
                if let Some(type_info) = ctx.get_node_typeinfo(NodeId::Expr(expr_id)) {
                    return Some(type_info);
                }
                // Compound-literal-position check moved to analysis rule A015.
                if !elements.is_empty()
                    && let Some(element_type_info) = self.infer_expression(elements[0], ctx)
                {
                    for &element_id in &elements[1..] {
                        let element_type = self.infer_expression(element_id, ctx);
                        if let Some(element_type) = element_type
                            && element_type != element_type_info
                        {
                            self.errors.push(TypeCheckError::ArrayElementTypeMismatch {
                                expected: element_type_info.clone(),
                                found: element_type,
                                location,
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
                    ctx.set_node_typeinfo(NodeId::Expr(expr_id), array_type.clone());
                    return Some(array_type);
                }
                None
            }
            Expr::BoolLiteral { .. } => {
                ctx.set_node_typeinfo(NodeId::Expr(expr_id), TypeInfo::boolean());
                Some(TypeInfo::boolean())
            }
            Expr::StringLiteral { .. } => {
                ctx.set_node_typeinfo(NodeId::Expr(expr_id), TypeInfo::string());
                Some(TypeInfo::string())
            }
            Expr::NumberLiteral { .. } => {
                if ctx.get_node_typeinfo(NodeId::Expr(expr_id)).is_some() {
                    return ctx.get_node_typeinfo(NodeId::Expr(expr_id));
                }
                let res_type = TypeInfo {
                    kind: TypeInfoKind::Number(NumberType::I32),
                    type_params: vec![],
                };
                ctx.set_node_typeinfo(NodeId::Expr(expr_id), res_type.clone());
                Some(res_type)
            }
            Expr::UnitLiteral => {
                ctx.set_node_typeinfo(NodeId::Expr(expr_id), TypeInfo::default());
                Some(TypeInfo::default())
            }
            Expr::Identifier(ident_id) => {
                let name = ctx.arena()[ident_id].name.clone();
                if let Some(var_ty) = self.symbol_table.lookup_variable(&name) {
                    ctx.set_node_typeinfo(NodeId::Expr(expr_id), var_ty.clone());
                    Some(var_ty)
                } else {
                    self.push_error_dedup(TypeCheckError::UnknownIdentifier { name, location });
                    None
                }
            }
            Expr::Type(type_id) => {
                let type_info = TypeInfo::from_type_id(ctx.arena(), type_id);
                ctx.set_node_typeinfo(NodeId::Expr(expr_id), type_info.clone());
                if let TypeNode::Array { size, .. } = &ctx.arena()[type_id].kind {
                    self.infer_expression(*size, ctx);
                }
                Some(type_info)
            }
            Expr::Uzumaki => ctx.get_node_typeinfo(NodeId::Expr(expr_id)),
        }
    }

    /// Infer types for a function call expression.
    ///
    /// Handles associated function calls (Type::method), instance method calls (obj.method),
    /// and regular function calls.
    #[allow(clippy::too_many_lines)]
    fn infer_function_call(
        &mut self,
        call_expr_id: ExprId,
        function_expr_id: ExprId,
        call_type_params: &[IdentId],
        call_args: &[(Option<IdentId>, ExprId)],
        ctx: &mut TypedContext,
    ) -> Option<TypeInfo> {
        let arena = ctx.arena();
        let location = arena[call_expr_id].location;

        // Handle Type::function() syntax - associated function calls
        if let Expr::TypeMemberAccess {
            expr: inner_expr,
            name: method_name_id,
        } = &arena[function_expr_id].kind
        {
            let inner_expr = *inner_expr;
            let method_name_id = *method_name_id;

            let type_name = match &ctx.arena()[inner_expr].kind {
                Expr::Type(ty_id) => match &ctx.arena()[*ty_id].kind {
                    TypeNode::Custom(ident_id) => Some(ctx.arena()[*ident_id].name.clone()),
                    TypeNode::QualifiedName { qualifier, name } => Some(format!(
                        "{}::{}",
                        ctx.arena()[*qualifier].name,
                        ctx.arena()[*name].name,
                    )),
                    TypeNode::Qualified { alias: _, name } => Some(ctx.arena()[*name].name.clone()),
                    _ => None,
                },
                Expr::Identifier(ident_id) => Some(ctx.arena()[*ident_id].name.clone()),
                _ => None,
            };

            if let Some(type_name) = type_name {
                let method_name = ctx.arena()[method_name_id].name.clone();

                // First check if this is an enum variant - can't call variants like functions
                if self.symbol_table.lookup_enum(&type_name).is_some() {
                    // Fall through to standard function handling
                } else if let Some(method_info) =
                    self.symbol_table.lookup_method(&type_name, &method_name)
                {
                    if method_info.is_instance_method() {
                        cov_mark::hit!(type_checker_instance_method_called_as_associated);
                        self.errors
                            .push(TypeCheckError::InstanceMethodCalledAsAssociated {
                                type_name: type_name.clone(),
                                method_name: method_name.clone(),
                                location: ctx.arena()[function_expr_id].location,
                            });
                    }

                    self.check_and_report_visibility(
                        &method_info.visibility,
                        method_info.scope_id,
                        &ctx.arena()[function_expr_id].location,
                        VisibilityContext::Method {
                            type_name: type_name.clone(),
                            method_name: method_name.clone(),
                        },
                    );

                    let signature = &method_info.signature;
                    let arg_count = call_args.len();

                    if arg_count != signature.param_types.len() {
                        self.errors.push(TypeCheckError::ArgumentCountMismatch {
                            kind: "method",
                            name: format!("{}::{}", type_name, method_name),
                            expected: signature.param_types.len(),
                            found: arg_count,
                            location,
                        });
                    }

                    let sig_param_types = signature.param_types.clone();
                    let sig_return_type = signature.return_type.clone();
                    for (i, arg) in call_args.iter().enumerate() {
                        self.propagate_arg_uzumaki_type(arg.1, sig_param_types.get(i), ctx);
                        let arg_type = self.infer_expression(arg.1, ctx);

                        if let Some(arg_type) = arg_type
                            && i < sig_param_types.len()
                            && arg_type != sig_param_types[i]
                        {
                            let arg_name = format!("arg{i}");
                            self.errors.push(TypeCheckError::TypeMismatch {
                                expected: sig_param_types[i].clone(),
                                found: arg_type,
                                context: TypeMismatchContext::MethodArgument {
                                    type_name: type_name.clone(),
                                    method_name: method_name.clone(),
                                    arg_name,
                                    arg_index: i,
                                },
                                location,
                            });
                        }
                    }

                    ctx.set_node_typeinfo(
                        NodeId::Expr(function_expr_id),
                        TypeInfo {
                            kind: TypeInfoKind::Function(format!("{}::{}", type_name, method_name)),
                            type_params: vec![],
                        },
                    );
                    ctx.set_node_typeinfo(NodeId::Expr(call_expr_id), sig_return_type.clone());
                    return Some(sig_return_type);
                }
                // Not an enum and not a method - fall through to standard function handling
            }
            // Fall through to standard function handling for invalid type expressions
        }

        // Handle instance method calls: obj.method()
        if let Expr::MemberAccess {
            expr: receiver_expr,
            name: method_name_id,
        } = &ctx.arena()[function_expr_id].kind
        {
            let receiver_expr = *receiver_expr;
            let method_name_id = *method_name_id;

            let receiver_type = self.infer_expression(receiver_expr, ctx);

            // Method-call-chain-on-compound-return check moved to analysis rule A018.

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
                    let method_name = ctx.arena()[method_name_id].name.clone();
                    if let Some(method_info) =
                        self.symbol_table.lookup_method(&type_name, &method_name)
                    {
                        if !method_info.is_instance_method() {
                            cov_mark::hit!(type_checker_associated_function_called_as_method);
                            self.errors
                                .push(TypeCheckError::AssociatedFunctionCalledAsMethod {
                                    type_name: type_name.clone(),
                                    method_name: method_name.clone(),
                                    location: ctx.arena()[function_expr_id].location,
                                });
                        }

                        self.check_and_report_visibility(
                            &method_info.visibility,
                            method_info.scope_id,
                            &ctx.arena()[function_expr_id].location,
                            VisibilityContext::Method {
                                type_name: type_name.clone(),
                                method_name: method_name.clone(),
                            },
                        );

                        let signature = &method_info.signature;
                        let arg_count = call_args.len();

                        if arg_count != signature.param_types.len() {
                            self.errors.push(TypeCheckError::ArgumentCountMismatch {
                                kind: "method",
                                name: format!("{}::{}", type_name, method_name),
                                expected: signature.param_types.len(),
                                found: arg_count,
                                location,
                            });
                        }

                        let sig_param_types = signature.param_types.clone();
                        let sig_return_type = signature.return_type.clone();
                        for (i, arg) in call_args.iter().enumerate() {
                            self.propagate_arg_uzumaki_type(arg.1, sig_param_types.get(i), ctx);
                            let arg_type = self.infer_expression(arg.1, ctx);
    
                            if let Some(arg_type) = arg_type
                                && i < sig_param_types.len()
                                && arg_type != sig_param_types[i]
                            {
                                let arg_name = format!("arg{i}");
                                self.errors.push(TypeCheckError::TypeMismatch {
                                    expected: sig_param_types[i].clone(),
                                    found: arg_type,
                                    context: TypeMismatchContext::MethodArgument {
                                        type_name: type_name.clone(),
                                        method_name: method_name.clone(),
                                        arg_name,
                                        arg_index: i,
                                    },
                                    location,
                                });
                            }
                        }

                        ctx.set_node_typeinfo(
                            NodeId::Expr(function_expr_id),
                            TypeInfo {
                                kind: TypeInfoKind::Function(format!(
                                    "{}::{}",
                                    type_name, method_name
                                )),
                                type_params: vec![],
                            },
                        );
                        ctx.set_node_typeinfo(NodeId::Expr(call_expr_id), sig_return_type.clone());
                        return Some(sig_return_type);
                    }
                    self.errors.push(TypeCheckError::MethodNotFound {
                        type_name,
                        method_name,
                        location: ctx.arena()[function_expr_id].location,
                    });
                    return None;
                }
                self.errors.push(TypeCheckError::MethodCallOnNonStruct {
                    found: receiver_type,
                    location,
                });
                for arg in call_args {
                    self.infer_expression(arg.1, ctx);
                }
                return None;
            }
            // Receiver type inference failed; infer arguments for better error recovery
            for arg in call_args {
                self.infer_expression(arg.1, ctx);
            }
            return None;
        }

        // Regular function call
        let func_name = match &ctx.arena()[function_expr_id].kind {
            Expr::Identifier(ident_id) => ctx.arena()[*ident_id].name.clone(),
            _ => {
                for arg in call_args {
                    self.infer_expression(arg.1, ctx);
                }
                return None;
            }
        };

        let signature = if let Some(s) = self.symbol_table.lookup_function(&func_name) {
            self.check_and_report_visibility(
                &s.visibility,
                s.definition_scope_id,
                &location,
                VisibilityContext::Function {
                    name: func_name.clone(),
                },
            );
            s.clone()
        } else {
            self.push_error_dedup(TypeCheckError::UndefinedFunction {
                name: func_name,
                location,
            });
            for arg in call_args {
                self.infer_expression(arg.1, ctx);
            }
            return None;
        };

        if call_args.len() != signature.param_types.len() {
            self.errors.push(TypeCheckError::ArgumentCountMismatch {
                kind: "function",
                name: func_name.clone(),
                expected: signature.param_types.len(),
                found: call_args.len(),
                location,
            });
            for arg in call_args {
                self.infer_expression(arg.1, ctx);
            }
            return None;
        }

        // Build substitution map for generic functions
        let substitutions = if !signature.type_params.is_empty() {
            if !call_type_params.is_empty() {
                if call_type_params.len() != signature.type_params.len() {
                    self.errors
                        .push(TypeCheckError::TypeParameterCountMismatch {
                            name: func_name.clone(),
                            expected: signature.type_params.len(),
                            found: call_type_params.len(),
                            location,
                        });
                    FxHashMap::default()
                } else {
                    {
                        let mut subs: FxHashMap<String, TypeInfo> = FxHashMap::default();
                        for (param_name, type_ident_id) in
                            signature.type_params.iter().zip(call_type_params.iter())
                        {
                            let type_name = ctx.arena()[*type_ident_id].name.clone();
                            let concrete_type = self
                                .symbol_table
                                .lookup_type(&type_name)
                                .unwrap_or_else(|| TypeInfo {
                                    kind: TypeInfoKind::Custom(type_name),
                                    type_params: vec![],
                                });
                            subs.insert(param_name.clone(), concrete_type);
                        }
                        subs
                    }
                }
            } else {
                // Try to infer type parameters from arguments
                let inferred =
                    self.infer_type_params_from_args(&signature, call_args, &location, ctx);
                if inferred.is_empty() && !signature.type_params.is_empty() {
                    self.errors.push(TypeCheckError::MissingTypeParameters {
                        function_name: func_name.clone(),
                        expected: signature.type_params.len(),
                        location,
                    });
                }
                inferred
            }
        } else {
            FxHashMap::default()
        };

        // Apply substitution to return type
        let return_type = signature.return_type.substitute(&substitutions);
        let sig_param_types = signature.param_types.clone();

        // Infer argument types and validate against parameter types
        for (i, arg) in call_args.iter().enumerate() {
            self.propagate_arg_uzumaki_type(arg.1, sig_param_types.get(i), ctx);
            let arg_type = self.infer_expression(arg.1, ctx);
            if let Some(arg_type) = arg_type
                && i < sig_param_types.len()
            {
                let expected = sig_param_types[i].substitute(&substitutions);
                if arg_type != expected {
                    let arg_name = format!("arg{i}");
                    self.errors.push(TypeCheckError::TypeMismatch {
                        expected,
                        found: arg_type,
                        context: TypeMismatchContext::FunctionArgument {
                            function_name: func_name.clone(),
                            arg_name,
                            arg_index: i,
                        },
                        location,
                    });
                }
            }
        }

        ctx.set_node_typeinfo(NodeId::Expr(call_expr_id), return_type.clone());
        Some(return_type)
    }

    /// Propagate uzumaki type from parameter context.
    ///
    /// If the argument is an uzumaki (`@`), sets the parameter type on the
    /// uzumaki node so that `infer_expression` can return the correct type.
    /// Codegen restriction checks (A012-A014) are handled by the analysis pass.
    fn propagate_arg_uzumaki_type(
        &mut self,
        arg_expr_id: ExprId,
        param_type: Option<&TypeInfo>,
        ctx: &mut TypedContext,
    ) {
        if let Expr::Uzumaki = &ctx.arena()[arg_expr_id].kind
            && let Some(pt) = param_type
        {
            ctx.set_node_typeinfo(NodeId::Expr(arg_expr_id), pt.clone());
        }
    }

    // Compound-return-in-arg check moved to analysis rule A016.

    /// Process a module definition.
    ///
    /// Creates a new scope for the module and processes all definitions within it.
    /// After processing, pops back to the parent scope.
    #[allow(dead_code)]
    fn process_module_definition(
        &mut self,
        def_id: DefId,
        ctx: &mut TypedContext,
    ) -> anyhow::Result<()> {
        let arena = ctx.arena();
        let def_data = &arena[def_id];
        let Def::Module { name, vis, defs } = &def_data.kind else {
            return Ok(());
        };
        let module_name = arena[*name].name.clone();
        let defs_snapshot = defs.clone();
        let _scope_id = self.symbol_table.enter_module(&module_name, vis.clone());

        if let Some(body) = &defs_snapshot {
            for &inner_def_id in body {
                let arena = ctx.arena();
                let inner_def = &arena[inner_def_id];
                let inner_location = inner_def.location;
                match &inner_def.kind {
                    Def::TypeAlias { name, ty, .. } => {
                        let type_name = arena[*name].name.clone();
                        let type_info = TypeInfo::from_type_id(arena, *ty);
                        self.symbol_table
                            .register_type(&type_name, Some(type_info))
                            .unwrap_or_else(|_| {
                                self.errors.push(TypeCheckError::RegistrationFailed {
                                    kind: RegistrationKind::Type,
                                    name: type_name,
                                    reason: None,
                                    location: inner_location,
                                });
                            });
                    }
                    Def::Struct {
                        name: struct_name,
                        vis: struct_vis,
                        fields,
                        ..
                    } => {
                        let s_name = arena[*struct_name].name.clone();
                        let field_infos: Vec<(String, TypeInfo, Visibility)> = fields
                            .iter()
                            .map(|f| {
                                (
                                    arena[f.name].name.clone(),
                                    TypeInfo::from_type_id(arena, f.ty),
                                    Visibility::Private,
                                )
                            })
                            .collect();
                        self.symbol_table
                            .register_struct(&s_name, &field_infos, vec![], struct_vis.clone())
                            .unwrap_or_else(|_| {
                                self.errors.push(TypeCheckError::RegistrationFailed {
                                    kind: RegistrationKind::Struct,
                                    name: s_name,
                                    reason: None,
                                    location: inner_location,
                                });
                            });
                    }
                    Def::Enum {
                        name: enum_name,
                        vis: enum_vis,
                        variants,
                    } => {
                        let e_name = arena[*enum_name].name.clone();
                        let variant_names: Vec<&str> =
                            variants.iter().map(|v| arena[*v].name.as_str()).collect();
                        self.symbol_table
                            .register_enum(&e_name, &variant_names, enum_vis.clone())
                            .unwrap_or_else(|_| {
                                self.errors.push(TypeCheckError::RegistrationFailed {
                                    kind: RegistrationKind::Enum,
                                    name: e_name,
                                    reason: None,
                                    location: inner_location,
                                });
                            });
                    }
                    Def::Spec {
                        name: spec_name, ..
                    } => {
                        let sp_name = arena[*spec_name].name.clone();
                        self.symbol_table
                            .register_spec(&sp_name)
                            .unwrap_or_else(|_| {
                                self.errors.push(TypeCheckError::RegistrationFailed {
                                    kind: RegistrationKind::Spec,
                                    name: sp_name,
                                    reason: None,
                                    location: inner_location,
                                });
                            });
                    }
                    Def::Module { .. } => {
                        self.process_module_definition(inner_def_id, ctx)?;
                    }
                    Def::Function { .. } => {
                        self.infer_variables(inner_def_id, ctx);
                    }
                    Def::Constant {
                        name: const_name,
                        ty,
                        value,
                        ..
                    } => {
                        let c_name = arena[*const_name].name.clone();
                        let const_type = self
                            .symbol_table
                            .resolve_custom_type(TypeInfo::from_type_id(arena, *ty));
                        let value_id = *value;
                        if let Err(err) = self.symbol_table.push_variable_to_scope(
                            &c_name,
                            const_type.clone(),
                            false,
                        ) {
                            self.errors.push(TypeCheckError::RegistrationFailed {
                                kind: RegistrationKind::Variable,
                                name: c_name,
                                reason: Some(err.to_string()),
                                location: inner_location,
                            });
                        }
                        self.check_const_initializer(
                            value_id,
                            &const_type,
                            inner_location,
                            ctx,
                        );
                    }
                    Def::ExternFunction {
                        name: ef_name,
                        args,
                        returns,
                        ..
                    } => {
                        let fn_name = arena[*ef_name].name.clone();
                        let param_types: Vec<TypeInfo> = args
                            .iter()
                            .filter_map(|a| match &a.kind {
                                ArgKind::SelfRef { .. } => None,
                                ArgKind::Named { ty, .. }
                                | ArgKind::Ignored { ty }
                                | ArgKind::TypeOnly(ty) => Some(TypeInfo::from_type_id(arena, *ty)),
                            })
                            .collect();
                        let return_type = returns
                            .map(|r| TypeInfo::from_type_id(arena, r))
                            .unwrap_or_default();
                        if let Err(err) = self.symbol_table.register_function(
                            &fn_name,
                            vec![],
                            param_types,
                            return_type,
                        ) {
                            self.errors.push(TypeCheckError::RegistrationFailed {
                                kind: RegistrationKind::Function,
                                name: fn_name,
                                reason: Some(err),
                                location: inner_location,
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
        let arena = ctx.arena();
        let all_directives: Vec<_> = arena
            .source_files()
            .flat_map(|sf| sf.directives.iter())
            .cloned()
            .collect();
        for directive in &all_directives {
            match directive {
                Directive::Use(use_directive) => {
                    let arena = ctx.arena();
                    if let Err(_err) = self.process_use_statement(arena, use_directive) {
                        let path = use_directive
                            .segments
                            .iter()
                            .map(|s| arena[*s].name.as_str())
                            .collect::<Vec<_>>()
                            .join("::");
                        self.errors.push(TypeCheckError::ImportResolutionFailed {
                            path,
                            location: use_directive.location,
                        });
                    }
                }
            }
        }
    }

    /// Process a use statement (Phase A: registration only).
    /// Converts UseDirective AST to Import and registers in current scope.
    fn process_use_statement(
        &mut self,
        arena: &AstArena,
        use_stmt: &inference_ast::nodes::UseDirective,
    ) -> anyhow::Result<()> {
        let path: Vec<String> = use_stmt
            .segments
            .iter()
            .map(|s| arena[*s].name.clone())
            .collect();

        let kind = if use_stmt.imported_types.is_empty() {
            ImportKind::Plain
        } else {
            let items: Vec<ImportItem> = use_stmt
                .imported_types
                .iter()
                .map(|t| ImportItem {
                    name: arena[*t].name.clone(),
                    alias: None,
                })
                .collect();
            ImportKind::Partial(items)
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
            cov_mark::hit!(type_checker_circular_glob_import_detected);
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
                if let Some(parent) = scope.borrow().parent.as_ref().and_then(|p| p.upgrade()) {
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
    fn infer_type_params_from_args(
        &mut self,
        signature: &FuncInfo,
        arguments: &[(Option<IdentId>, ExprId)],
        call_location: &Location,
        ctx: &mut TypedContext,
    ) -> FxHashMap<String, TypeInfo> {
        let mut substitutions: FxHashMap<String, TypeInfo> = FxHashMap::default();

        // For each parameter, check if it contains a type variable
        for (i, param_type) in signature.param_types.iter().enumerate() {
            if i >= arguments.len() {
                break;
            }

            // If the parameter type is a type variable, infer from argument
            if let TypeInfoKind::Generic(type_param_name) = &param_type.kind {
                let arg_type = self.infer_expression(arguments[i].1, ctx);

                if let Some(arg_type) = arg_type {
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

    /// Extracts the root identifier name from a potentially nested access expression.
    ///
    /// Handles array index access (`arr[i]`), member access (`p.x`), and any
    /// nesting thereof (`p.x[0]`, `arr[0].x`, `p.x.y`). Returns `None` for
    /// non-identifier bases.
    fn extract_root_variable_name(&self, arena: &AstArena, expr_id: ExprId) -> Option<String> {
        match &arena[expr_id].kind {
            Expr::Identifier(ident_id) => Some(arena[*ident_id].name.clone()),
            Expr::ArrayIndexAccess { array, .. } => {
                self.extract_root_variable_name(arena, *array)
            }
            Expr::MemberAccess { expr, .. } => self.extract_root_variable_name(arena, *expr),
            _ => None,
        }
    }

    // validate_literal_range moved to analysis rule A022 (LiteralOutOfRange).

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
                cov_mark::hit!(type_checker_error_dedup_skips_duplicate);
                return;
            }
            cov_mark::hit!(type_checker_error_dedup_first_occurrence);
            self.reported_error_keys.insert(key);
        }
        self.errors.push(error);
    }

}
