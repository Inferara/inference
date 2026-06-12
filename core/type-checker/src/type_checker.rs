//! Type Checker Implementation
//!
//! Core type checking logic that infers and validates types throughout the
//! AST. The type checker operates in five phases, executed in order:
//!
//! 1. **`process_directives`** — register raw imports from `use` statements
//! 2. **`register_types`** — collect type/struct/enum/spec definitions
//! 3. **`resolve_imports`** — bind import paths to the symbols they refer to
//! 4. **`collect_function_and_constant_definitions`** — register function
//!    signatures and constant declarations
//! 5. **`infer_variables`** — type-check function bodies and method bodies
//!
//! Phase ordering is load-bearing: type definitions (phase 2) must be in the
//! symbol table before functions can mention them in signatures (phase 4),
//! and imports (phase 3) must be resolved before name lookup runs during
//! body inference (phase 5). This is what lets Inference support forward
//! references — a function can refer to a type or another function defined
//! later in the source file.
//!
//! Errors are not fatal: the checker collects them in `self.errors` and
//! keeps walking the AST so a single run reports as many issues as
//! possible. Duplicate entries are filtered via `reported_errors`.
//!
//! ## Generics
//!
//! Generic type parameters declared on a function (`fn foo<T>(...)`) are
//! recorded on the signature in phase 4. At a call site in phase 5,
//! `infer_type_params_from_args` derives concrete substitutions for each
//! `T` from the call's argument types and reports
//! `ConflictingTypeInference` / `CannotInferTypeParameter` when the
//! substitution can't be determined unambiguously.

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
    definition_graph::{self, DefNode, GraphOutcome},
    errors::{DedupKind, RegistrationKind, TypeCheckError, TypeMismatchContext, VisibilityContext},
    symbol_table::{
        ExternOrigin, FuncInfo, FuncKind, Import, ImportItem, ImportKind, ResolvedImport,
        ResolvedImportTarget, SymbolTable,
    },
    type_info::{NumberType, TypeInfo, TypeInfoKind},
    typed_context::TypedContext,
};

#[derive(Default)]
pub(crate) struct TypeChecker {
    symbol_table: SymbolTable,
    errors: Vec<TypeCheckError>,
    reported_errors: FxHashSet<(DedupKind, String)>,
    /// Type parameter names for the function/method body currently being inferred.
    /// Set before walking the body, cleared after. Used by `infer_statement` to
    /// pass type param context to `validate_type` and `TypeInfo::from_type_id_with_type_params`.
    current_type_params: Vec<String>,
    /// Declaring extern [`DefId`] → provenance, derived from `use … from`
    /// directives before externs are registered.
    ///
    /// Keyed by the *declaration*, not the bare name: a `use { f } from m;`
    /// directive is file-global, so it binds only the **top-level** `external fn
    /// f` and never a same-named extern declared inside a `spec` or `module`.
    /// Keying by [`DefId`] keeps those scopes' externs unbound (and so
    /// A024-rejected) even when they share a name with a bound top-level extern.
    ///
    /// Holds only unambiguously-bound externs; an extern named by conflicting
    /// modules is reported as [`TypeCheckError::AmbiguousExternModule`] and
    /// omitted here so it falls back to an unbound registration.
    extern_module_bindings: FxHashMap<DefId, ExternOrigin>,
}

/// RAII guard that enters a spec scope on construction and pops it on drop.
///
/// Wraps `&mut TypeChecker` and forwards `Deref`/`DerefMut` so the guard
/// substitutes for `&mut self` throughout a spec-body recursive walk.
/// Pop happens on every exit path, including panic unwind, removing the
/// previous open-coded `enter_spec` / `pop_scope` pairs that had to be
/// kept in lockstep at three call sites.
struct SpecScopeGuard<'a> {
    tc: &'a mut TypeChecker,
}

impl<'a> SpecScopeGuard<'a> {
    fn enter(tc: &'a mut TypeChecker, spec_name: &str) -> Self {
        let _ = tc.symbol_table.enter_spec(spec_name);
        Self { tc }
    }
}

impl std::ops::Deref for SpecScopeGuard<'_> {
    type Target = TypeChecker;
    fn deref(&self) -> &TypeChecker {
        self.tc
    }
}

impl std::ops::DerefMut for SpecScopeGuard<'_> {
    fn deref_mut(&mut self) -> &mut TypeChecker {
        self.tc
    }
}

impl Drop for SpecScopeGuard<'_> {
    fn drop(&mut self) {
        self.tc.symbol_table.pop_scope();
    }
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
        self.collect_extern_bindings(ctx);
        self.register_types(ctx);
        self.collect_function_and_constant_definitions(ctx);
        // Imports resolve after both types and functions are registered so an
        // item import (`use a::b::{f};`) can bind a function as well as a type;
        // import binding never feeds the registration passes, so this ordering
        // is safe.
        self.resolve_imports();
        self.check_definition_cycles(ctx);
        self.check_spec_function_shadows_top_level(ctx);
        // Continue to inference phase even if registration had errors
        // to collect all errors before returning. Each file's bodies are
        // inferred inside that file's scope so name resolution and visibility
        // checks see the file's own definitions and imports.
        for (module_path, defs) in Self::files_with_defs(ctx) {
            self.symbol_table.enter_file_scope(&module_path);
            for def_id in defs {
                self.infer_def(def_id, ctx);
            }
        }
        self.symbol_table.reset_to_root();
        if !self.errors.is_empty() {
            let error_messages: Vec<String> = std::mem::take(&mut self.errors)
                .into_iter()
                .map(|e| e.to_string())
                .collect();
            bail!(error_messages.join("; "))
        }
        Ok(self.symbol_table.clone())
    }

    fn infer_def(&mut self, def_id: DefId, ctx: &mut TypedContext) {
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
            Def::Spec { name, defs, .. } => {
                let spec_name = ctx.arena()[*name].name.clone();
                let inner: Vec<DefId> = defs.clone();
                let mut guard = SpecScopeGuard::enter(self, &spec_name);
                for inner_id in inner {
                    guard.infer_def(inner_id, ctx);
                }
            }
            _ => {}
        }
    }

    /// Collects each source file's `(module_path, defs)` in canonical arena
    /// order (entry first, then by module path). Every registration pass walks
    /// this list, entering the file's scope before processing its definitions so
    /// each file's symbols land in their own namespace; the entry file's scope is
    /// the root, keeping single-file programs unchanged.
    fn files_with_defs(ctx: &TypedContext) -> Vec<(Vec<String>, Vec<DefId>)> {
        ctx.arena()
            .source_files()
            .map(|sf| (sf.module_path.clone(), sf.defs.clone()))
            .collect()
    }

    /// Detects value cycles among top-level `const` initializers and `type`
    /// aliases across all files, emitting [`TypeCheckError::CircularDefinition`]
    /// on a cycle. When acyclic, records the dependency-first topological order on
    /// the context for a later phase to emit constants in a computable order.
    ///
    /// File-to-file import cycles are unaffected — they are allowed (#63). Only a
    /// cycle in the *values* of definitions, which has no evaluation order, is an
    /// error.
    fn check_definition_cycles(&mut self, ctx: &mut TypedContext) {
        let nodes = self.collect_definition_nodes(ctx);
        if nodes.is_empty() {
            return;
        }
        match definition_graph::analyze(ctx.arena(), &nodes) {
            GraphOutcome::Acyclic { topo_order } => ctx.set_definition_order(topo_order),
            GraphOutcome::Cyclic { cycle, location } => {
                self.errors.push(TypeCheckError::CircularDefinition {
                    cycle: cycle.join(" -> "),
                    location,
                });
            }
        }
    }

    /// Builds a [`DefNode`] for every top-level `const` and `type` alias across
    /// all files, recording the scope each registered in (its file scope) and its
    /// scope ancestry so the value graph can resolve references by name.
    fn collect_definition_nodes(&mut self, ctx: &TypedContext) -> Vec<DefNode> {
        let mut nodes = Vec::new();
        for (module_path, defs) in Self::files_with_defs(ctx) {
            let scope_id = self.symbol_table.enter_file_scope(&module_path);
            let scope_chain = self.symbol_table.scope_ancestry(scope_id);
            let file_path = module_path.join("::");
            for def_id in defs {
                let def_data = &ctx.arena()[def_id];
                let location = def_data.location;
                let name_id = match &def_data.kind {
                    Def::Constant { name, .. } | Def::TypeAlias { name, .. } => *name,
                    _ => continue,
                };
                nodes.push(DefNode {
                    def_id,
                    scope_id,
                    name: ctx.arena()[name_id].name.clone(),
                    file_path: file_path.clone(),
                    location,
                    scope_chain: scope_chain.clone(),
                });
            }
        }
        self.symbol_table.reset_to_root();
        nodes
    }

    /// Registers `Def::TypeAlias`, `Def::Struct`, `Def::Enum`, and `Def::Spec`
    fn register_types(&mut self, ctx: &mut TypedContext) {
        for (module_path, defs) in Self::files_with_defs(ctx) {
            self.symbol_table.enter_file_scope(&module_path);
            for def_id in defs {
                self.register_type_for_def(def_id, ctx);
            }
        }
        self.symbol_table.reset_to_root();

        self.check_recursive_struct_definitions(ctx);
    }

    fn register_type_for_def(&mut self, def_id: DefId, ctx: &mut TypedContext) {
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
                let field_infos: Vec<(String, TypeInfo)> = fields
                    .iter()
                    .map(|f| {
                        (
                            arena[f.name].name.clone(),
                            TypeInfo::from_type_id(arena, f.ty),
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
                let vis_clone = vis.clone();
                self.symbol_table
                    .register_struct(&struct_name, &field_infos, vec![], vis_clone, location)
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
                            definition_location: method_location,
                            kind: FuncKind::Local,
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
                    .register_enum(&enum_name, &variant_names, vis.clone(), location)
                    .unwrap_or_else(|_| {
                        self.errors.push(TypeCheckError::RegistrationFailed {
                            kind: RegistrationKind::Enum,
                            name: enum_name,
                            reason: None,
                            location,
                        });
                    });
            }
            Def::Spec { name, defs, .. } => {
                let spec_name = arena[*name].name.clone();
                // Register the spec name in the parent scope BEFORE entering the
                // spec scope, so bare references to the spec resolve via the parent.
                self.symbol_table
                    .register_spec(&spec_name)
                    .unwrap_or_else(|_| {
                        self.errors.push(TypeCheckError::RegistrationFailed {
                            kind: RegistrationKind::Spec,
                            name: spec_name.clone(),
                            reason: None,
                            location,
                        });
                    });
                let inner: Vec<DefId> = defs.clone();
                let mut guard = SpecScopeGuard::enter(self, &spec_name);
                for inner_id in inner {
                    if guard.reject_duplicate_spec_struct_or_enum(inner_id, ctx) {
                        continue;
                    }
                    guard.register_type_for_def(inner_id, ctx);
                }
            }
            Def::Constant { .. } | Def::Function { .. } | Def::ExternFunction { .. } => {}
        }
    }

    /// Rejects a spec-inner `struct` or `enum` whose name collides with one
    /// already registered in any other scope (the top-level/root scope, or a
    /// previously-registered spec scope). Returns `true` when the def was
    /// rejected so the caller skips the recursive registration.
    ///
    /// Cross-spec mangling of structs/enums would require carrying spec
    /// context through every type access (field projection, sret layouts,
    /// method dispatch). Rejecting at registration time avoids that blast
    /// radius and surfaces a clear diagnostic instead of the previous silent
    /// behavior where the first-registered layout was used for both specs.
    fn reject_duplicate_spec_struct_or_enum(
        &mut self,
        def_id: DefId,
        ctx: &TypedContext,
    ) -> bool {
        let arena = ctx.arena();
        let def_data = &arena[def_id];
        let location = def_data.location;
        match &def_data.kind {
            Def::Struct { name, .. } => {
                let struct_name = arena[*name].name.clone();
                if self.symbol_table.lookup_struct_anywhere(&struct_name).is_some() {
                    self.errors.push(TypeCheckError::RegistrationFailed {
                        kind: RegistrationKind::Struct,
                        name: struct_name,
                        reason: Some(
                            "duplicate definition across spec scopes is not supported"
                                .to_string(),
                        ),
                        location,
                    });
                    return true;
                }
                false
            }
            Def::Enum { name, .. } => {
                let enum_name = arena[*name].name.clone();
                if self.symbol_table.lookup_enum_anywhere(&enum_name).is_some() {
                    self.errors.push(TypeCheckError::RegistrationFailed {
                        kind: RegistrationKind::Enum,
                        name: enum_name,
                        reason: Some(
                            "duplicate definition across spec scopes is not supported"
                                .to_string(),
                        ),
                        location,
                    });
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Rejects spec-inner functions whose bare name shadows a top-level
    /// function of the same name. Runs after both phases have populated
    /// the symbol table so the check is independent of source order.
    ///
    /// Without this check, codegen and the type-checker silently disagree on
    /// which `foo` is invoked from inside a spec: the type checker types the
    /// call against the closest binding while codegen prefers the spec-mangled
    /// key. Banning the collision keeps both layers consistent.
    fn check_spec_function_shadows_top_level(&mut self, ctx: &TypedContext) {
        let arena = ctx.arena();
        let source_files: Vec<_> = arena
            .source_files()
            .flat_map(|sf| sf.defs.iter().copied())
            .collect();
        for def_id in source_files {
            let Def::Spec {
                name: spec_name_id,
                defs: inner_defs,
                ..
            } = &arena[def_id].kind
            else {
                continue;
            };
            let spec_name = arena[*spec_name_id].name.clone();
            for &inner_id in inner_defs {
                if let Def::Function {
                    name: fn_name_id, ..
                } = &arena[inner_id].kind
                {
                    let fn_name = arena[*fn_name_id].name.clone();
                    if self
                        .symbol_table
                        .lookup_function_in_root(&fn_name)
                        .is_some()
                    {
                        self.push_error_dedup(TypeCheckError::SpecFunctionShadowsTopLevel {
                            spec_name: spec_name.clone(),
                            function_name: fn_name,
                            location: arena[inner_id].location,
                        });
                    }
                }
            }
        }
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
    fn collect_function_and_constant_definitions(&mut self, ctx: &mut TypedContext) {
        for (module_path, defs) in Self::files_with_defs(ctx) {
            self.symbol_table.enter_file_scope(&module_path);
            for def_id in defs {
                self.collect_for_def(def_id, ctx);
            }
        }
        self.symbol_table.reset_to_root();
    }

    #[allow(clippy::too_many_lines)]
    fn collect_for_def(&mut self, def_id: DefId, ctx: &mut TypedContext) {
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
                vis,
                type_params,
                args,
                returns,
                ..
            } => {
                let func_name = ctx.arena()[*name].name.clone();
                let name_ident_id = *name;
                let func_vis = vis.clone();
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
                if let Err(err) = self.symbol_table.register_function_with_visibility(
                    &func_name,
                    tp_names,
                    param_types,
                    return_type,
                    func_vis,
                    location,
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
                // Externs declare no type parameters, so every type in the
                // signature must resolve against the surrounding scope. Validate
                // them up front (mirroring `Def::Function`): an undeclared
                // `Custom` type would otherwise pass the signature-only extern
                // validator and `todo!()`-panic codegen (H6). A `self` receiver
                // is meaningless on an extern and is rejected here (H7), matching
                // how standalone functions reject it.
                for arg in args {
                    match &arg.kind {
                        ArgKind::SelfRef { .. } => {
                            self.errors.push(TypeCheckError::SelfReferenceInFunction {
                                function_name: func_name.clone(),
                                location: arg.location,
                            });
                        }
                        ArgKind::Named { ty, .. }
                        | ArgKind::Ignored { ty }
                        | ArgKind::TypeOnly(ty) => {
                            self.validate_type(ctx.arena(), *ty, &[]);
                        }
                    }
                }
                if let Some(return_type_id) = returns {
                    self.validate_type(ctx.arena(), *return_type_id, &[]);
                }
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
                let origin = self.extern_module_bindings.get(&def_id).cloned();
                if let Err(err) = self.symbol_table.register_extern_function(
                    &func_name,
                    param_types,
                    return_type,
                    origin,
                ) {
                    self.errors.push(TypeCheckError::RegistrationFailed {
                        kind: RegistrationKind::Function,
                        name: func_name,
                        reason: Some(err),
                        location,
                    });
                }
            }
            Def::Spec { name, defs, .. } => {
                let spec_name = ctx.arena()[*name].name.clone();
                let inner: Vec<DefId> = defs.clone();
                let mut guard = SpecScopeGuard::enter(self, &spec_name);
                for inner_id in inner {
                    guard.collect_for_def(inner_id, ctx);
                }
            }
            Def::Struct { .. } | Def::Enum { .. } | Def::TypeAlias { .. } => {}
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

    /// Type-check the body of a free function (phase 5, top-level functions).
    ///
    /// Pushes a fresh scope, registers each named argument as a local
    /// variable in that scope, computes the declared return type, then
    /// walks the body statement-by-statement via `infer_statement`. The
    /// function's type parameters are passed through `tp_names` so that
    /// occurrences of those names in argument or return types are treated
    /// as generic placeholders rather than unresolved `Custom` types.
    ///
    /// Example — for `fn id<T>(x: T) -> T { return x; }`, `tp_names`
    /// contains `["T"]`, so both the parameter `x: T` and the return type
    /// `T` are recorded as `TypeInfoKind::Generic("T")`; the concrete type
    /// is substituted at each call site, not here.
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
                    self.validate_bool_expression(
                        condition_expr_id,
                        TypeMismatchContext::Condition,
                        ctx,
                    );
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
                self.validate_bool_expression(condition, TypeMismatchContext::Condition, ctx);

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
                self.validate_bool_expression(expr, TypeMismatchContext::Assert, ctx);
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

    /// Enforces that `expr_id` evaluates to `bool`. On failure, records a
    /// `TypeMismatch` error tagged with `context` (so `if`/`loop` and `assert`
    /// can render distinct diagnostics) and located at the expression itself,
    /// not the enclosing statement — the user sees a caret on the offending
    /// sub-expression.
    fn validate_bool_expression(
        &mut self,
        expr_id: ExprId,
        context: TypeMismatchContext,
        ctx: &mut TypedContext,
    ) {
        let expr_type = self.infer_expression(expr_id, ctx);
        if expr_type.is_none() || expr_type.as_ref().unwrap().kind != TypeInfoKind::Bool {
            self.errors.push(TypeCheckError::TypeMismatch {
                expected: TypeInfo::boolean(),
                found: expr_type.unwrap_or_default(),
                context,
                location: ctx.arena()[expr_id].location,
            });
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
                                // A field is accessible exactly when its struct
                                // is: fields carry no visibility of their own, so
                                // the check consults the struct's visibility and
                                // defining scope (#63).
                                self.check_and_report_visibility(
                                    &struct_info.visibility,
                                    struct_info.definition_scope_id,
                                    struct_info.definition_location,
                                    &location,
                                    VisibilityContext::Field {
                                        struct_name: struct_name.clone(),
                                        field_name: field_name.clone(),
                                    },
                                );
                                let field_type = self.symbol_table.resolve_custom_type(field_info.type_info.clone());
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
                        let expr_type = self.infer_expression(inner_expr, ctx)?;
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
                    }
                };

                let variant_name = ctx.arena()[name].name.clone();

                if let Some(enum_info) = self.symbol_table.lookup_enum(&enum_name) {
                    if enum_info.variants.contains(&variant_name) {
                        self.check_and_report_visibility(
                            &enum_info.visibility,
                            enum_info.definition_scope_id,
                            enum_info.definition_location,
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
                                let field_type =
                                    self.symbol_table.resolve_custom_type(field_info.type_info.clone());
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

    /// Flattens a chain of `TypeMemberAccess` nodes whose deepest base is an
    /// identifier into its `::`-separated path segments
    /// (`math::arith::add` ⇒ `["math", "arith", "add"]`).
    ///
    /// Returns `None` if the base is anything other than an identifier — a value
    /// expression like `foo().bar::baz` is not a static path and is left to the
    /// other call-resolution paths.
    fn flatten_type_member_path(arena: &AstArena, expr_id: ExprId) -> Option<Vec<String>> {
        match &arena[expr_id].kind {
            Expr::Identifier(ident_id) => Some(vec![arena[*ident_id].name.clone()]),
            Expr::TypeMemberAccess { expr, name } => {
                let mut segments = Self::flatten_type_member_path(arena, *expr)?;
                segments.push(arena[*name].name.clone());
                Some(segments)
            }
            _ => None,
        }
    }

    /// Resolves a `::`-separated call target (`math::arith::add(...)`) to a
    /// function in another file, returning `Some(result)` when the path names a
    /// function and `None` when it does not (so the caller falls through to
    /// method / enum / plain-call handling).
    ///
    /// Only multi-qualifier paths (three or more segments) are handled here; a
    /// single-qualifier `Type::name` is left to the existing method/enum code so
    /// associated-function and variant calls keep their dedicated diagnostics.
    fn try_infer_qualified_function_call(
        &mut self,
        call_expr_id: ExprId,
        function_expr_id: ExprId,
        call_args: &[(Option<IdentId>, ExprId)],
        ctx: &mut TypedContext,
    ) -> Option<Option<TypeInfo>> {
        let segments = Self::flatten_type_member_path(ctx.arena(), function_expr_id)?;
        if segments.len() < 3 {
            return None;
        }

        let location = ctx.arena()[call_expr_id].location;
        let path = segments.join("::");

        // A three-or-more-segment call target is unambiguously a file-qualified
        // function path. If it does not resolve to a callable function — the
        // name is wrong, or a hop crosses a non-re-exported (private) import —
        // this is the call's error to report, not a fall-through: the
        // method/enum/plain-call handlers below can never resolve a multi-hop
        // path, so silently falling through would accept it.
        let from_scope = self.symbol_table.current_scope_id().unwrap_or(0);
        let signature = match self.symbol_table.resolve_qualified_name(&segments, from_scope) {
            Some((symbol, _)) if symbol.as_function().is_some() => {
                let sig = symbol.as_function().expect("checked above").clone();
                // A qualified path may name a private function in another file
                // (`lib::arith::secret()`). Resolution reaches it through the
                // scope tree, so the final symbol's `pub`-ness must be enforced
                // against the accessing file — the re-export gate only guards
                // intermediate hops, not the target itself.
                self.check_and_report_visibility(
                    &sig.visibility,
                    sig.definition_scope_id,
                    sig.definition_location,
                    &location,
                    VisibilityContext::Function { name: path.clone() },
                );
                sig
            }
            _ => {
                self.push_error_dedup(TypeCheckError::UndefinedFunction {
                    name: path,
                    location,
                });
                for arg in call_args {
                    self.infer_expression(arg.1, ctx);
                }
                return Some(None);
            }
        };

        if call_args.len() != signature.param_types.len() {
            self.errors.push(TypeCheckError::ArgumentCountMismatch {
                kind: "function",
                name: path,
                expected: signature.param_types.len(),
                found: call_args.len(),
                location,
            });
            for arg in call_args {
                self.infer_expression(arg.1, ctx);
            }
            return Some(None);
        }

        let sig_param_types = signature.param_types.clone();
        for (i, arg) in call_args.iter().enumerate() {
            self.propagate_arg_uzumaki_type(arg.1, sig_param_types.get(i), ctx);
            let arg_type = self.infer_expression(arg.1, ctx);
            if let Some(arg_type) = arg_type
                && i < sig_param_types.len()
                && arg_type != sig_param_types[i]
            {
                self.errors.push(TypeCheckError::TypeMismatch {
                    expected: sig_param_types[i].clone(),
                    found: arg_type,
                    context: TypeMismatchContext::FunctionArgument {
                        function_name: path.clone(),
                        arg_name: format!("arg{i}"),
                        arg_index: i,
                    },
                    location,
                });
            }
        }

        ctx.set_node_typeinfo(
            NodeId::Expr(function_expr_id),
            TypeInfo {
                kind: TypeInfoKind::Function(path),
                type_params: vec![],
            },
        );
        ctx.set_node_typeinfo(NodeId::Expr(call_expr_id), signature.return_type.clone());
        Some(Some(signature.return_type))
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
        // A `::`-separated path that names a function in another file
        // (`math::arith::add(...)`) resolves through the file scope tree and the
        // importing file's resolved imports. This is tried before the
        // `Type::function()` handling below, which only covers single-qualifier
        // method/enum access; a path naming a struct method or enum variant
        // returns no function symbol here and falls through unchanged.
        if let Some(result) =
            self.try_infer_qualified_function_call(call_expr_id, function_expr_id, call_args, ctx)
        {
            return result;
        }

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
                        method_info.signature.definition_location,
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
                            method_info.signature.definition_location,
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
                s.definition_location,
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

    /// Process all use directives (Phase A of import resolution): registers each
    /// file's `use` directives as unresolved imports in that file's scope, so
    /// resolution later binds them against the importing file's namespace.
    fn process_directives(&mut self, ctx: &mut TypedContext) {
        let per_file: Vec<(Vec<String>, Vec<Directive>)> = ctx
            .arena()
            .source_files()
            .map(|sf| (sf.module_path.clone(), sf.directives.clone()))
            .collect();
        for (module_path, directives) in per_file {
            self.symbol_table.enter_file_scope(&module_path);
            for directive in &directives {
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
        self.symbol_table.reset_to_root();
    }

    /// Binds each `external fn` to the source module named by a `use … from`
    /// clause, populating [`Self::extern_module_bindings`].
    ///
    /// For every `use { fields } from module;` directive, each field is paired
    /// with `module`. The resulting bindings are validated:
    ///
    /// - A field imported from two or more distinct modules is reported as
    ///   [`TypeCheckError::AmbiguousExternModule`] and left unbound.
    /// - A field imported from a module but never declared as an `external fn`
    ///   is reported as [`TypeCheckError::ExternImportNotDeclared`].
    /// - A field imported from exactly one module and declared as an extern is
    ///   recorded as a bound [`ExternOrigin`].
    ///
    /// An `external fn` with no binding `use` is left unbound (no error): a bare
    /// extern declaration is valid; analysis rule A024 governs whether *calling*
    /// an unlinked extern is allowed.
    fn collect_extern_bindings(&mut self, ctx: &TypedContext) {
        let arena = ctx.arena();

        let extern_decls = Self::collect_top_level_extern_decls(arena);

        // field name → (distinct modules in first-seen order, first import location)
        let mut imports: FxHashMap<String, (Vec<String>, Location)> = FxHashMap::default();
        for sf in arena.source_files() {
            for directive in &sf.directives {
                let Directive::Use(use_dir) = directive;
                let Some(module_ref) = &use_dir.from else {
                    continue;
                };
                let module = module_ref
                    .segments
                    .iter()
                    .map(|s| arena[*s].name.as_str())
                    .collect::<Vec<_>>()
                    .join("::");
                for &field_id in &use_dir.imported_types {
                    let field = arena[field_id].name.clone();
                    let entry = imports
                        .entry(field)
                        .or_insert_with(|| (Vec::new(), use_dir.location));
                    if !entry.0.contains(&module) {
                        entry.0.push(module.clone());
                    }
                }
            }
        }

        for (field, (modules, location)) in imports {
            let Some(&decl) = extern_decls.get(&field) else {
                self.errors.push(TypeCheckError::ExternImportNotDeclared {
                    name: field,
                    module: modules.join(", "),
                    location,
                });
                continue;
            };
            if modules.len() > 1 {
                let module_list = modules
                    .iter()
                    .map(|m| format!("`{m}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.errors.push(TypeCheckError::AmbiguousExternModule {
                    name: field,
                    modules: module_list,
                    location,
                });
                continue;
            }
            let logical_module = modules.into_iter().next().expect("one module");
            self.extern_module_bindings.insert(
                decl,
                ExternOrigin {
                    logical_module,
                    export_field: field,
                    decl,
                    resolved_path: None,
                },
            );
        }
    }

    /// Collects every **top-level** `external fn` declaration, mapping its name
    /// to its declaring [`DefId`].
    ///
    /// A `use … from` clause is file-global and binds only top-level externs,
    /// so this deliberately does **not** descend into `spec` or `module` bodies:
    /// a same-named extern declared in a spec or module is left out, stays
    /// unbound, and remains A024-rejected when called. Descending here (the prior
    /// behavior) let a top-level `use` silently bind a spec-inner extern,
    /// suppressing A024 and miscompiling proof-mode codegen.
    fn collect_top_level_extern_decls(arena: &AstArena) -> FxHashMap<String, DefId> {
        let mut decls = FxHashMap::default();
        for sf in arena.source_files() {
            for &def_id in &sf.defs {
                if let Def::ExternFunction { name, .. } = &arena[def_id].kind {
                    decls.insert(arena[*name].name.clone(), def_id);
                }
            }
        }
        decls
    }

    /// Process a use statement (Phase A: registration only).
    /// Converts UseDirective AST to Import and registers in current scope.
    ///
    /// A `use … from <module>` clause binds an `external fn` to its source
    /// module; it is not a symbol import to resolve against the local scope
    /// tree. Such directives are handled by [`Self::collect_extern_bindings`]
    /// and skipped here, so their imported names are not mistaken for dangling
    /// path imports.
    fn process_use_statement(
        &mut self,
        arena: &AstArena,
        use_stmt: &inference_ast::nodes::UseDirective,
    ) -> anyhow::Result<()> {
        if use_stmt.from.is_some() {
            return Ok(());
        }

        let path: Vec<String> = use_stmt
            .segments
            .iter()
            .map(|s| arena[*s].name.clone())
            .collect();

        // A braced `use a::b::{ ... }` with no items is parseable but means
        // nothing; reject it with guidance rather than registering an empty
        // import that resolves to nothing. The brace presence (recorded by the
        // parser as `braced`) is what distinguishes it from a file import, since
        // both leave `imported_types` empty.
        if use_stmt.braced && use_stmt.imported_types.is_empty() {
            self.errors.push(TypeCheckError::EmptyImportList {
                path: path.join("::"),
                location: use_stmt.location,
            });
            return Ok(());
        }

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
            visibility: use_stmt.vis.clone(),
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

    /// Resolve the imports registered in a single file scope.
    ///
    /// A file import (`use a::b;`) binds the namespace `b` to the scope `a::b`;
    /// an item import (`use a::b::{x};`) binds each named item, which must exist
    /// in `a::b` and be `pub`. A `pub use` marks the resulting binding as
    /// re-exported, so an importer of *this* file may traverse through it.
    fn resolve_imports_in_scope(&mut self, scope_id: u32) {
        let imports = {
            let scope = match self.symbol_table.get_scope(scope_id) {
                Some(s) => s,
                None => return,
            };
            scope.borrow().imports.clone()
        };

        for import in imports {
            let reexported = matches!(import.visibility, Visibility::Public);
            match &import.kind {
                ImportKind::Plain => self.resolve_file_import(scope_id, &import, reexported),
                ImportKind::Partial(items) => {
                    self.resolve_item_imports(scope_id, &import, items, reexported);
                }
            }
        }
    }

    /// Resolves a file import (`use a::b;`): the last segment names a namespace
    /// scope `a::b` and is bound under that name in `scope_id`. A binding that
    /// collides with a local definition or an existing import of the same name is
    /// rejected.
    fn resolve_file_import(&mut self, scope_id: u32, import: &Import, reexported: bool) {
        let Some(local_name) = import.path.last().cloned() else {
            return;
        };

        let Some(target_scope_id) = self.symbol_table.find_module_scope(&import.path) else {
            self.report_unresolvable_file_import(import);
            return;
        };

        if self.report_import_name_collision(scope_id, &local_name, &import.location) {
            return;
        }

        let resolved = ResolvedImport {
            local_name,
            target: ResolvedImportTarget::Namespace {
                scope_id: target_scope_id,
            },
            reexported,
        };
        if let Some(scope) = self.symbol_table.get_scope(scope_id) {
            scope.borrow_mut().add_resolved_import(resolved);
        }
    }

    /// Reports an unresolvable file import. Without a project context (the
    /// string-parse and REPL paths) the only file is the entry, so a path-form
    /// `use` can never name an existing namespace; that case gets a dedicated,
    /// actionable message rather than the generic resolution failure.
    fn report_unresolvable_file_import(&mut self, import: &Import) {
        let path = import.path.join("::");
        if !self.symbol_table.has_file_namespaces() {
            self.errors
                .push(TypeCheckError::FileImportWithoutProjectContext {
                    path,
                    location: import.location,
                });
        } else {
            self.errors.push(TypeCheckError::ImportResolutionFailed {
                path,
                location: import.location,
            });
        }
    }

    /// Resolves an item import (`use a::b::{x, y};`): each item must exist in the
    /// namespace `a::b` and be `pub`. Found items are bound for bare use.
    fn resolve_item_imports(
        &mut self,
        scope_id: u32,
        import: &Import,
        items: &[ImportItem],
        reexported: bool,
    ) {
        let file_path = import.path.join("::");
        for item in items {
            let mut full_path = import.path.clone();
            full_path.push(item.name.clone());
            let local_name = item.alias.clone().unwrap_or_else(|| item.name.clone());

            let Some((symbol, def_scope_id)) = self
                .symbol_table
                .resolve_qualified_name(&full_path, scope_id)
            else {
                self.errors.push(TypeCheckError::ImportedItemNotFound {
                    item: item.name.clone(),
                    file: file_path.clone(),
                    location: import.location,
                });
                continue;
            };

            if !symbol.is_public() {
                self.errors.push(TypeCheckError::ImportedItemPrivate {
                    item: item.name.clone(),
                    file: file_path.clone(),
                    location: import.location,
                    definition_location: Self::symbol_definition_location(&symbol),
                });
            }

            if self.report_import_name_collision(scope_id, &local_name, &import.location) {
                continue;
            }

            let resolved = ResolvedImport {
                local_name,
                target: ResolvedImportTarget::Item {
                    symbol: Box::new(symbol),
                    definition_scope_id: def_scope_id,
                },
                reexported,
            };
            if let Some(scope) = self.symbol_table.get_scope(scope_id) {
                scope.borrow_mut().add_resolved_import(resolved);
            }
        }
    }

    /// Reports an [`TypeCheckError::ImportNameCollision`] when `local_name`
    /// already names a local definition or a previously-resolved import in
    /// `scope_id`. Returns `true` when a collision was reported so the caller
    /// skips binding the name.
    fn report_import_name_collision(
        &mut self,
        scope_id: u32,
        local_name: &str,
        location: &Location,
    ) -> bool {
        let Some(scope) = self.symbol_table.get_scope(scope_id) else {
            return false;
        };
        let (clashes_local, clashes_import) = {
            let scope = scope.borrow();
            (
                scope.lookup_symbol_local(local_name).is_some(),
                scope.resolved_imports.contains_key(local_name),
            )
        };
        if clashes_local || clashes_import {
            let with = if clashes_local {
                "a local definition"
            } else {
                "another import"
            };
            self.errors.push(TypeCheckError::ImportNameCollision {
                name: local_name.to_string(),
                with: with.to_string(),
                location: *location,
            });
            return true;
        }
        false
    }

    /// Source location of an imported symbol's declaration, for the definition
    /// note on a private-import diagnostic.
    fn symbol_definition_location(symbol: &crate::symbol_table::Symbol) -> Location {
        symbol.definition_location()
    }

    /// Whether an item is accessible from the current scope (#63).
    ///
    /// An item is accessible iff (a) the access happens within the item's
    /// defining file — including that file's spec scopes and nested blocks, which
    /// are descendants of the defining scope — or (b) the item is `pub`.
    /// `pub` items reached across a file boundary travel through the import
    /// machinery, which already gates each hop on `pub`/re-export, so a single
    /// `Public` check suffices here. With per-file scopes, the same-file case is
    /// exactly "access scope is a descendant of the defining scope".
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

    /// Check visibility and report a dual-location error if access is denied.
    /// Returns true if access is allowed, false if an error was reported.
    ///
    /// The reported error names both the use site (`location`) and the
    /// definition site (`definition_location` in the defining file), so a
    /// private cross-file access tells the user exactly where to add `pub`.
    fn check_and_report_visibility(
        &mut self,
        visibility: &Visibility,
        definition_scope: u32,
        definition_location: Location,
        location: &Location,
        context: VisibilityContext,
    ) -> bool {
        let access_scope = self.symbol_table.current_scope_id().unwrap_or(0);
        if self.check_visibility(visibility, definition_scope, access_scope) {
            true
        } else {
            let definition_file = self.symbol_table.module_path_of_scope(definition_scope);
            self.errors.push(TypeCheckError::PrivateAccessViolation {
                context,
                location: *location,
                definition_location,
                definition_file,
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

    /// Infer concrete substitutions for a generic function's type
    /// parameters from the types of the actual arguments at a call site.
    ///
    /// Walks the parameter list; whenever a parameter type is
    /// `TypeInfoKind::Generic(T)`, infers the corresponding argument's type
    /// and binds `T` to it in the returned map. Two edge cases are detected
    /// and reported:
    ///
    /// 1. **Conflicting inference.** When the same `T` appears in multiple
    ///    parameters but the corresponding arguments have different types.
    ///    Example: `fn pair<T>(a: T, b: T)` called as `pair(1, true)` —
    ///    `T` is inferred as `i32` from `a` and as `bool` from `b`. The
    ///    first binding wins, and a
    ///    [`TypeCheckError::ConflictingTypeInference`] is emitted at the
    ///    call site.
    /// 2. **Unresolvable parameter.** When a type parameter declared in the
    ///    signature is not reachable from any argument. Example:
    ///    `fn make<T>() -> T` — `T` only appears in the return position, so
    ///    no argument carries information about it. A
    ///    [`TypeCheckError::CannotInferTypeParameter`] is emitted.
    ///
    /// Returns the substitution map (possibly empty). The caller is
    /// responsible for applying it to the return type before propagating.
    fn infer_type_params_from_args(
        &mut self,
        signature: &FuncInfo,
        arguments: &[(Option<IdentId>, ExprId)],
        call_location: &Location,
        ctx: &mut TypedContext,
    ) -> FxHashMap<String, TypeInfo> {
        let mut substitutions: FxHashMap<String, TypeInfo> = FxHashMap::default();

        for (i, param_type) in signature.param_types.iter().enumerate() {
            if i >= arguments.len() {
                break;
            }

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

    /// Push an error, deduplicating per `(DedupKind, name)` so the same
    /// diagnostic for the same symbol is never recorded twice across the
    /// registration and inference passes.
    ///
    /// Errors whose [`TypeCheckError::dedup_key`] returns `None` are always
    /// recorded as-is.
    fn push_error_dedup(&mut self, error: TypeCheckError) {
        if let Some(key) = error.dedup_key() {
            if !self.reported_errors.insert(key) {
                cov_mark::hit!(type_checker_error_dedup_skips_duplicate);
                return;
            }
            cov_mark::hit!(type_checker_error_dedup_first_occurrence);
        }
        self.errors.push(error);
    }

}
