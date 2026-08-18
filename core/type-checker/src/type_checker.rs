//! Type Checker Implementation
//!
//! Core type checking logic that infers and validates types throughout the
//! AST. The type checker operates in five phases, executed in order:
//!
//! 1. **`process_directives`** — register raw imports from `use` statements
//! 2. **`register_types`** — collect type/struct/enum/spec definitions
//! 3. **`collect_function_and_constant_definitions`** — register function
//!    signatures and constant declarations
//! 4. **`resolve_imports`** — bind import paths to the symbols they refer to
//! 5. **`infer_variables`** — type-check function bodies and method bodies
//!
//! Phase ordering is load-bearing: type definitions must be in the symbol table
//! before functions can mention them in the signatures
//! `collect_function_and_constant_definitions` registers, and imports must be
//! resolved before name lookup runs during body inference. This is what lets
//! Inference support forward references — a function can refer to a type or
//! another function defined later in the source file. The constraints are stated
//! against the passes rather than their positions, so a later reordering cannot
//! leave this paragraph disagreeing with the list above it.
//!
//! The list is a summary, not the whole of `check_collecting`: signatures are
//! also re-normalized and validated by later statements of that function, so
//! "signatures" is not a concern that finishes when they are registered. Which
//! statement performs a given check is worth confirming at the call site rather
//! than inferring from this list — the checks are spread across the function,
//! and not all of them run where a reading of the phase order would suggest.
//!
//! Errors are not fatal: the checker collects them in `self.errors` and
//! keeps walking the AST so a single run reports as many issues as
//! possible. Duplicate entries are filtered via `reported_errors`.
//!
//! ## Generics
//!
//! Generic type parameters declared on a function (`fn foo<T>(...)`) are
//! recorded on the signature when it is registered. At a call site during body
//! inference, `infer_type_params_from_args` derives concrete substitutions for
//! each `T` from the call's argument types and reports
//! `ConflictingTypeInference` / `CannotInferTypeParameter` when the
//! substitution can't be determined unambiguously.

use inference_ast::arena::AstArena;
use inference_ast::extern_prelude::ExternPrelude;
use inference_ast::ids::{DefId, ExprId, IdentId, NodeId, StmtId, TypeId};
use inference_ast::nodes::{
    ArgData, ArgKind, Def, Directive, Expr, Location, OperatorKind, Stmt, TypeNode,
    UnaryOperatorKind, Visibility,
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    definition_graph::{self, DefNode, GraphOutcome},
    errors::{DedupKind, RegistrationKind, TypeCheckError, TypeMismatchContext, VisibilityContext},
    symbol_table::{
        ExternOrigin, FuncInfo, FuncKind, Import, ImportItem, ImportKind, ResolvedImport,
        ResolvedImportTarget, ResolvedNominalType, SymbolTable, UnimportedNamespace,
    },
    type_info::{NumberType, TypeInfo, TypeInfoKind},
    typed_context::{CallTarget, TypedContext},
};

/// A pair of imports in one scope that bind the same local name, deferred for a
/// canonical-target comparison after the fixpoint. The pair is a *benign*
/// duplicate when both resolve to the same target (the same item reached
/// directly and through a `pub use` re-export); only a genuine clash (different
/// targets) is reported. See [`TypeChecker::report_genuine_name_clashes`].
struct DuplicateNameImport {
    scope_id: u32,
    local_name: String,
    /// The declaration-order-first import claiming `local_name` (the binder).
    first: Import,
    /// A later import claiming the same name (excluded from binding).
    later: Import,
}

/// The canonical target an import binds a name to, used to distinguish a benign
/// duplicate import from a genuine name clash.
#[derive(PartialEq, Eq)]
enum ImportTargetIdentity {
    /// A file import (`use a::b;`) — identified by its target namespace scope id.
    Namespace(u32),
    /// An item import (`use a::b::{x};`) — identified by the resolved item's
    /// defining scope, kind discriminant, and source name. Stable across
    /// `pub use` re-export hops because the defining scope is preserved.
    Item {
        def_scope: u32,
        kind: u8,
        name: String,
    },
}

/// What a position requires of the expression in it: the type, and the position
/// itself.
///
/// The two always travel together. The type is what an integer literal denotes
/// when it reaches a leaf; the position is what a diagnostic needs in order to
/// say *why* the literal has that type, since the type is written somewhere the
/// literal is not. Carrying them separately would let a descent forward one
/// without the other and leave a literal typed with no explanation.
///
/// Borrowed and [`Copy`], so the transparent forms forward it unchanged and a
/// literal under `-( 1 + 2 )` reports the position that typed the whole thing.
#[derive(Clone, Copy, Debug)]
struct Expected<'a> {
    ty: &'a TypeInfo,
    source: &'a TypeMismatchContext,
}

impl<'a> Expected<'a> {
    /// Pairs the type a position requires with the position requiring it.
    fn new(ty: &'a TypeInfo, source: &'a TypeMismatchContext) -> Self {
        Self { ty, source }
    }
}

/// The position of argument `arg_index` in a call to `function_name`.
///
/// The argument name is synthesized: a function signature records parameter
/// types but not their names. The mismatch message has rendered that
/// placeholder for a long time; [`TypeMismatchContext::literal_typing_reason`]
/// drops it rather than repeat it in a new message class.
fn function_arg_context(function_name: &str, arg_index: usize) -> TypeMismatchContext {
    TypeMismatchContext::FunctionArgument {
        function_name: function_name.to_string(),
        arg_name: format!("arg{arg_index}"),
        arg_index,
    }
}

/// The position of argument `arg_index` in a call to `type_name::method_name`.
/// The argument name is synthesized — see [`function_arg_context`].
fn method_arg_context(type_name: &str, method_name: &str, arg_index: usize) -> TypeMismatchContext {
    TypeMismatchContext::MethodArgument {
        type_name: type_name.to_string(),
        method_name: method_name.to_string(),
        arg_name: format!("arg{arg_index}"),
        arg_index,
    }
}

#[derive(Default)]
pub(crate) struct TypeChecker {
    symbol_table: SymbolTable,
    /// Collected errors paired with the file each was produced in: the `::`-joined
    /// module-path label, or `None` for the entry file. Source locations are
    /// per-file-local in the merged arena, so an error from an imported file must
    /// name its file or the user is misdirected to the entry file. The label is
    /// captured from [`Self::current_file_label`] at push time.
    errors: Vec<(Option<String>, TypeCheckError)>,
    /// File label of the file currently being checked: the `::`-joined module
    /// path, or `None` for the entry file. Set when entering a file's scope and
    /// cleared at the root, so every error pushed while a file is open is stamped
    /// with that file's identity. Cross-file passes that run at the root (e.g.
    /// definition-cycle detection) leave it `None`, which is acceptable: a cycle
    /// spanning files has no single home file and its message already names the
    /// members.
    current_file_label: Option<String>,
    /// Deduplication set for diagnostics that the registration and inference
    /// passes can emit twice for the same symbol. The key carries the file label
    /// the diagnostic was produced in (`None` for the entry, or a cross-file pass
    /// running at the root) so the *same* diagnostic in two *different* files is
    /// kept distinct — two files each calling an undefined `missing()` must both
    /// report, not collapse to the importer's single message. Same-site repeats
    /// within one file still dedup because they share file label, kind, and name.
    reported_errors: FxHashSet<(Option<String>, DedupKind, String)>,
    /// Type parameter names for the function/method body currently being inferred.
    /// Set before walking the body, cleared after. Used by `infer_statement` to
    /// pass type param context to `validate_type` and `TypeInfo::from_type_id_with_type_params`.
    current_type_params: Vec<String>,
    /// Declaring extern [`DefId`] → provenance, derived from `use … from`
    /// directives before externs are registered.
    ///
    /// Keyed by the *declaration*, not the bare name: a `use { f } from m;`
    /// directive is file-wide, so it binds only the **top-level** `external fn
    /// f` and never a same-named extern declared inside a `spec` — the file's top
    /// level and a `spec` within it being the only two places a declaration can
    /// sit. Keying by [`DefId`] keeps that inner scope's externs unbound (and so
    /// A024-rejected) even when they share a name with a bound top-level extern.
    ///
    /// Holds only unambiguously-bound externs; an extern named by conflicting
    /// modules is reported as [`TypeCheckError::AmbiguousExternModule`] and
    /// omitted here so it falls back to an unbound registration.
    extern_module_bindings: FxHashMap<DefId, ExternOrigin>,
    /// `(scope_id, local_name)` of every import binding flagged as a collision
    /// during [`Self::report_import_collisions`]. The fixpoint import resolution
    /// skips these so a colliding import never produces a binding, no matter
    /// whether its target resolves on a later pass.
    colliding_imports: FxHashSet<(u32, String)>,
    /// Calls already reported for mixing named and positional arguments.
    ///
    /// A call expression is not memoized the way most inferred expressions are,
    /// and `infer_type_params_from_args` re-infers an argument that fills a
    /// generic parameter slot, so a call written as such an argument reaches
    /// inference twice. The set records the report, not the visit, so the
    /// diagnostic is written once per call site while two genuinely different
    /// bad calls to the same callee both report.
    mixed_argument_calls_reported: FxHashSet<ExprId>,
    /// Calls whose argument labels have already been checked against the
    /// callee's parameter names. Guards the same repeated visit as
    /// [`Self::mixed_argument_calls_reported`].
    labelled_argument_calls_checked: FxHashSet<ExprId>,
    /// Top-level `external fn` declarations whose name is also a top-level
    /// function **in the same file**, reported by
    /// [`Self::check_extern_function_name_collisions`].
    ///
    /// Registration consults this to leave the extern out of the file's symbol
    /// table: the two declarations claim one symbol, and inserting the second
    /// raises a generic registration failure that names neither site. Skipping
    /// the extern leaves the purpose-built collision diagnostic as the only
    /// message — and leaves an unrelated duplicate of either name still able to
    /// report its own.
    same_file_extern_collisions: FxHashSet<DefId>,
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
    /// Call this before `check_collecting()` to make external modules available.
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
    /// Runs every type-check phase and returns the populated symbol table paired
    /// with the structured errors collected along the way (empty on success).
    ///
    /// This is the single implementation of the phase pipeline; both entry points
    /// go through it. It never fails: errors are collected rather than fatal (see
    /// the module docs), so every phase runs to completion and the returned symbol
    /// table is as complete as the checker could make it even when some bodies
    /// failed to type-check. The caller decides how to surface the errors —
    /// [`check_with_diagnostics`](crate::check_with_diagnostics) returns them
    /// structured, while
    /// [`TypeCheckerBuilder::build_typed_context`](crate::TypeCheckerBuilder::build_typed_context)
    /// renders and joins them into one aggregated [`anyhow::Error`].
    ///
    /// Phase ordering:
    /// 1. `process_directives()` - Register raw imports in scopes
    /// 2. `register_types()` - Collect type definitions into symbol table
    /// 3. `collect_function_and_constant_definitions()` - Register functions
    /// 4. `resolve_imports()` - Bind import paths to symbols
    /// 5. Infer variable types in function bodies
    pub(crate) fn check_collecting(
        &mut self,
        ctx: &mut TypedContext,
    ) -> (SymbolTable, Vec<(Option<String>, TypeCheckError)>) {
        self.process_directives(ctx);
        self.collect_extern_bindings(ctx);
        // Runs before any registration so a same-file collision is reported by
        // the purpose-built diagnostic rather than by the symbol table refusing
        // the second insert.
        self.check_extern_function_name_collisions(ctx);
        self.register_types(ctx);
        self.collect_function_and_constant_definitions(ctx);
        // Top-level consts become importable / qualified-resolvable symbols after
        // functions and structs register, so a same-named function registers first
        // and the const symbol is skipped rather than clashing (#63).
        self.register_constant_symbols(ctx);
        // Imports resolve after types, functions, and const symbols are registered
        // so an item import (`use a::b::{f};` / `use a::b::{C};`) can bind a
        // function, type, or const; import binding never feeds the registration
        // passes, so this ordering is safe.
        self.resolve_imports();
        // Signatures were resolved at registration, before imports bound, so an
        // item-imported type stayed a bare `Custom` name. Re-resolve them now that
        // each file's imports are visible, so a param/return of an imported struct
        // type matches what its call sites infer (`Struct`, not `Custom`).
        self.symbol_table.renormalize_signatures();
        // An item import binds a copy of the imported symbol captured before
        // signatures were re-normalized, so re-normalize those copies too — else a
        // bare or qualified call through the import compares a stale `Custom` param
        // against a canonical argument and falsely rejects (#63).
        self.symbol_table.renormalize_resolved_imports();
        // Recursive-struct detection runs after signatures are re-normalized so
        // every struct's stored field types carry their canonical, file-qualified
        // keys. The cycle test compares those keys, so a cycle that closes across
        // files is caught here (before codegen) and same-named structs in
        // different files are not mistaken for one (#63).
        self.check_recursive_struct_definitions(ctx);
        // Function signature types are validated only now, after imports resolve,
        // so an item-imported type works in a param/return position (#63).
        self.validate_signatures(ctx);
        let has_value_cycle = self.check_definition_cycles(ctx);
        // Const initializers are checked after cycles are detected: a value cycle
        // must report only `CircularDefinition`, not a downstream resolution error
        // from evaluating a member of the cycle.
        if !has_value_cycle {
            self.check_const_initializers(ctx);
        }
        self.check_spec_function_shadows_top_level(ctx);
        // Continue to inference phase even if registration had errors
        // to collect all errors before returning. Each file's bodies are
        // inferred inside that file's scope so name resolution and visibility
        // checks see the file's own definitions and imports.
        for (module_path, defs) in Self::files_with_defs(ctx) {
            self.enter_file(&module_path);
            for def_id in defs {
                self.infer_def(def_id, ctx);
            }
        }
        self.exit_files();
        (self.symbol_table.clone(), std::mem::take(&mut self.errors))
    }

    fn infer_def(&mut self, def_id: DefId, ctx: &mut TypedContext) {
        let kind = ctx.arena()[def_id].kind.clone();
        match &kind {
            Def::Function { .. } => {
                self.infer_variables(def_id, ctx);
            }
            Def::Struct { name, methods, .. } => {
                let struct_name = ctx.arena()[*name].name.clone();
                // Resolve the receiver `self` type through the symbol table so it
                // carries the struct's canonical key (its file identity), letting
                // a method body distinguish this struct from a same-named one in
                // another file. Falls back to a bare key if resolution fails.
                let struct_type = self
                    .symbol_table
                    .lookup_type(&struct_name)
                    .unwrap_or(TypeInfo {
                        kind: TypeInfoKind::Struct(struct_name.clone(), struct_name.clone()),
                        type_params: vec![],
                    });
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
    ///
    /// Returns `true` when a value cycle was found, so the caller can skip the
    /// const-initializer check (whose member resolution would otherwise emit a
    /// confusing secondary error for a definition that is part of the cycle).
    fn check_definition_cycles(&mut self, ctx: &mut TypedContext) -> bool {
        let nodes = self.collect_definition_nodes(ctx);
        if nodes.is_empty() {
            return false;
        }
        match definition_graph::analyze(ctx.arena(), &self.symbol_table, &nodes) {
            GraphOutcome::Acyclic { topo_order } => {
                ctx.set_definition_order(topo_order);
                false
            }
            GraphOutcome::Cyclic {
                cycle,
                location,
                scope_id,
            } => {
                // Stamp the cycle with the file the entry member is defined in.
                // The graph runs at the root cursor (the per-file walk has been
                // exited), so a cycle entirely within a non-entry file would
                // otherwise render a bare `line:col` and misattribute to the entry.
                // An entry-file cycle's scope is the root, whose label is `None`,
                // keeping the entry case bare as every other entry diagnostic is.
                self.push_error_for_scope(
                    scope_id,
                    TypeCheckError::CircularDefinition {
                        cycle: cycle.join(" -> "),
                        location,
                    },
                );
                true
            }
        }
    }

    /// Builds a [`DefNode`] for every top-level `const` and `type` alias across
    /// all files, recording the scope each registered in (its file scope) and its
    /// scope ancestry so the value graph can resolve references by name.
    fn collect_definition_nodes(&mut self, ctx: &TypedContext) -> Vec<DefNode> {
        let mut nodes = Vec::new();
        for (module_path, defs) in Self::files_with_defs(ctx) {
            let scope_id = self.enter_file(&module_path);
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
        self.exit_files();
        nodes
    }

    /// Registers `Def::TypeAlias`, `Def::Struct`, `Def::Enum`, and `Def::Spec`.
    ///
    /// Within each file, top-level definitions are registered before that file's
    /// `spec` blocks. A spec-inner struct/enum is keyed by its enclosing file
    /// exactly like a top-level one, so the two collapse to a single canonical key
    /// when they share a bare name in the same file (and codegen would index one
    /// key against two distinct layouts). Registering top-level types first lets
    /// [`Self::reject_duplicate_spec_struct_or_enum`] see the top-level definition
    /// and reject the spec one — independently of their source order, so the
    /// collision is a clean type-check error whether the spec is written before or
    /// after the top-level type (#63).
    fn register_types(&mut self, ctx: &mut TypedContext) {
        for (module_path, defs) in Self::files_with_defs(ctx) {
            self.enter_file(&module_path);
            let (specs, non_specs): (Vec<DefId>, Vec<DefId>) = defs
                .into_iter()
                .partition(|def_id| matches!(ctx.arena()[*def_id].kind, Def::Spec { .. }));
            for def_id in non_specs {
                self.register_type_for_def(def_id, ctx);
            }
            for def_id in specs {
                self.register_type_for_def(def_id, ctx);
            }
        }
        self.exit_files();
    }

    fn register_type_for_def(&mut self, def_id: DefId, ctx: &mut TypedContext) {
        let arena = ctx.arena();
        let def_data = &arena[def_id];
        let location = def_data.location;
        match &def_data.kind {
            Def::TypeAlias { name, ty, vis } => {
                let type_name = arena[*name].name.clone();
                let type_info = TypeInfo::from_type_id(arena, *ty);
                let alias_vis = vis.clone();
                self.symbol_table
                    .register_type_with_visibility(&type_name, Some(type_info), alias_vis, location)
                    .unwrap_or_else(|_| {
                        self.push_error(TypeCheckError::RegistrationFailed {
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
                self.report_type_declaration_diagnostics(def_id, ctx);
                let method_ids: Vec<DefId> = methods.clone();
                let vis_clone = vis.clone();
                self.symbol_table
                    .register_struct(&struct_name, &field_infos, vec![], vis_clone, location)
                    .unwrap_or_else(|_| {
                        self.push_error(TypeCheckError::RegistrationFailed {
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
                        // A receiver in any position sets `has_self`, so the
                        // function stays an instance method for the remainder of
                        // checking: classifying a misplaced receiver as an
                        // associated function instead would cascade an
                        // `AssociatedFunctionCalledAsMethod` at every call site of
                        // a method that is already rejected. The misplacement
                        // itself is reported by `validate_signature_for_def`,
                        // which unlike this pass also reaches a spec-inner struct
                        // whose name collides with an already-registered one.
                        let has_self = args
                            .iter()
                            .any(|a| matches!(a.kind, ArgKind::SelfRef { .. }));

                        let tp_names: Vec<String> =
                            type_params.iter().map(|p| arena[*p].name.clone()).collect();
                        // Types and names are produced by one pass so the receiver
                        // is dropped from both, keeping them index-aligned with
                        // the arguments a call site writes.
                        let (param_types, param_names): (Vec<TypeInfo>, Vec<Option<String>>) = args
                            .iter()
                            .filter_map(|a| match &a.kind {
                                ArgKind::SelfRef { .. } => None,
                                ArgKind::Named { ty, name, .. } => Some((
                                    self.symbol_table.resolve_custom_type(
                                        TypeInfo::from_type_id_with_type_params(
                                            arena, *ty, &tp_names,
                                        ),
                                    ),
                                    Some(arena[*name].name.clone()),
                                )),
                                ArgKind::Ignored { ty } | ArgKind::TypeOnly(ty) => Some((
                                    self.symbol_table.resolve_custom_type(
                                        TypeInfo::from_type_id_with_type_params(
                                            arena, *ty, &tp_names,
                                        ),
                                    ),
                                    None,
                                )),
                            })
                            .unzip();

                        let return_type = returns
                            .map(|r| TypeInfo::from_type_id_with_type_params(arena, r, &tp_names))
                            .map(|ti| self.symbol_table.resolve_custom_type(ti))
                            .unwrap_or_default();

                        let definition_scope_id = self.symbol_table.current_scope_id().unwrap_or(0);
                        let m_name = arena[*method_name].name.clone();
                        let signature = FuncInfo {
                            name: m_name.clone(),
                            type_params: tp_names,
                            param_types,
                            param_names,
                            return_type,
                            visibility: method_vis.clone(),
                            definition_scope_id,
                            definition_location: method_location,
                            kind: FuncKind::Local,
                        };

                        self.symbol_table
                            .register_method(&struct_name, signature, method_vis.clone(), has_self)
                            .unwrap_or_else(|err| {
                                self.push_error(TypeCheckError::RegistrationFailed {
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
                self.report_type_declaration_diagnostics(def_id, ctx);
                self.symbol_table
                    .register_enum(&enum_name, &variant_names, vis.clone(), location)
                    .unwrap_or_else(|_| {
                        self.push_error(TypeCheckError::RegistrationFailed {
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
                        self.push_error(TypeCheckError::RegistrationFailed {
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
                        guard.report_type_declaration_diagnostics(inner_id, ctx);
                        continue;
                    }
                    guard.register_type_for_def(inner_id, ctx);
                }
            }
            Def::Constant { .. } | Def::Function { .. } | Def::ExternFunction { .. } => {}
        }
    }

    /// Rejects a spec-inner `struct` or `enum` whose name collides with one
    /// already registered **in the same file** — another spec in that file, or a
    /// top-level type. Returns `true` when the def was rejected so the caller
    /// skips the recursive registration.
    ///
    /// The collision is keyed by the candidate's canonical (file-qualified) key,
    /// not its bare name, so a spec helper only conflicts within its own file: a
    /// same-named type in another file resolves to a distinct key and is left
    /// alone. The candidate is registered into the current spec scope, whose
    /// enclosing file the canonical key walks to, so the check sees exactly the
    /// key the candidate will receive.
    ///
    /// Cross-spec mangling of same-file structs/enums would require carrying spec
    /// context through every type access (field projection, sret layouts, method
    /// dispatch). Rejecting at registration time avoids that blast radius and
    /// surfaces a clear diagnostic instead of the previous silent behavior where
    /// the first-registered layout was used for both specs.
    fn reject_duplicate_spec_struct_or_enum(&mut self, def_id: DefId, ctx: &TypedContext) -> bool {
        // Without a current scope there is no file to key the candidate against,
        // so it cannot collide with a same-file sibling — never reject. Defaulting
        // to the root scope here would re-key every spec helper as if it lived in
        // the entry file, resurrecting the cross-file over-rejection this guards.
        let Some(scope_id) = self.symbol_table.current_scope_id() else {
            return false;
        };
        let arena = ctx.arena();
        let def_data = &arena[def_id];
        let location = def_data.location;
        match &def_data.kind {
            Def::Struct { name, .. } => {
                let struct_name = arena[*name].name.clone();
                let key = self
                    .symbol_table
                    .canonical_key_for_scope(scope_id, &struct_name);
                if self.symbol_table.lookup_struct_by_key(&key).is_some() {
                    self.push_error(TypeCheckError::RegistrationFailed {
                        kind: RegistrationKind::Struct,
                        name: struct_name,
                        reason: Some(
                            "duplicate definition within a file's spec scopes is not supported"
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
                let key = self
                    .symbol_table
                    .canonical_key_for_scope(scope_id, &enum_name);
                if self.symbol_table.lookup_enum_by_key(&key).is_some() {
                    self.push_error(TypeCheckError::RegistrationFailed {
                        kind: RegistrationKind::Enum,
                        name: enum_name,
                        reason: Some(
                            "duplicate definition within a file's spec scopes is not supported"
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

    /// Reports the diagnostics a `struct` or `enum` declaration carries in its own
    /// right — a repeated field, a repeated variant — independently of whether the
    /// declaration goes on to register.
    ///
    /// A spec-inner type refused registration by
    /// [`Self::reject_duplicate_spec_struct_or_enum`] would otherwise take its
    /// declaration errors down with it. The collision is fatal, so the user would
    /// never see the second mistake, not even after renaming: nothing beyond the
    /// collision was ever reported for that declaration.
    fn report_type_declaration_diagnostics(&mut self, def_id: DefId, ctx: &TypedContext) {
        let arena = ctx.arena();
        match &arena[def_id].kind {
            Def::Struct { name, fields, .. } => {
                let struct_name = arena[*name].name.clone();
                let mut seen_fields = FxHashSet::default();
                for field in fields {
                    let field_name = arena[field.name].name.clone();
                    if !seen_fields.insert(field_name.clone()) {
                        self.push_error(TypeCheckError::DuplicateStructFieldDefinition {
                            struct_name: struct_name.clone(),
                            field_name,
                            location: arena[field.name].location,
                        });
                    }
                }
            }
            Def::Enum { name, variants, .. } => {
                let enum_name = arena[*name].name.clone();
                let mut seen_variants = FxHashSet::default();
                for variant_id in variants {
                    let variant_name = arena[*variant_id].name.as_str();
                    if !seen_variants.insert(variant_name) {
                        self.push_error(TypeCheckError::DuplicateEnumVariant {
                            enum_name: enum_name.clone(),
                            variant_name: variant_name.to_string(),
                            location: arena[*variant_id].location,
                        });
                    }
                }
            }
            _ => {}
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
        for (module_path, defs) in Self::files_with_defs(ctx) {
            let file_scope = self.enter_file(&module_path);
            self.check_spec_shadows_in_file(ctx.arena(), &defs, file_scope);
        }
        self.exit_files();
    }

    /// Reports each spec-inner function in `def_ids` that shadows a top-level
    /// function **of the same file**. Shadowing is a same-file relationship: the
    /// colliding top-level name is resolved in the spec's own file scope
    /// (`file_scope`), not the entry file's root scope, so a spec in one file
    /// never collides with an unrelated top-level name in another file and a
    /// genuine collision inside a non-entry file is still caught. The diagnostic
    /// is pushed while the file is open, so it carries that file's label.
    fn check_spec_shadows_in_file(&mut self, arena: &AstArena, def_ids: &[DefId], file_scope: u32) {
        for &def_id in def_ids {
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
                        .lookup_function_in_scope(file_scope, &fn_name)
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
    /// This traverses into nested containers (specs) so that structs defined
    /// inside them are also checked.
    ///
    /// Identity is by **canonical key**, not bare name: two distinct same-named
    /// structs in different files are not a cycle, and a genuine cycle that closes
    /// across files (a struct transitively containing itself by key) is caught
    /// here rather than slipping through to a codegen-time layout failure. Each
    /// file is entered so a field's qualified type resolves against the struct's
    /// own defining file, and the struct's own key is taken from that scope.
    fn check_recursive_struct_definitions(&mut self, ctx: &TypedContext) {
        for (module_path, defs) in Self::files_with_defs(ctx) {
            self.enter_file(&module_path);
            self.check_recursive_structs_in_defs(ctx.arena(), &defs);
        }
        self.exit_files();
    }

    fn check_recursive_structs_in_defs(&mut self, arena: &AstArena, def_ids: &[DefId]) {
        let from_scope = self.symbol_table.current_scope_id().unwrap_or(0);
        for &def_id in def_ids {
            match &arena[def_id].kind {
                Def::Struct { name, fields, .. } => {
                    let struct_name = arena[*name].name.clone();
                    // The struct's own canonical key — its identity for the cycle
                    // test. Keying by the enclosing file (not bare name) makes a
                    // same-named struct in another file a distinct key, so it never
                    // counts as the same struct. The key is built from the current
                    // scope rather than resolved by name so a `spec`-inner struct —
                    // not reachable by bare name from the file scope — is still
                    // keyed and checked.
                    let target_key = self
                        .symbol_table
                        .canonical_key_for_scope(from_scope, &struct_name);
                    // The registered struct carries its fields' canonical, scope-
                    // correct types (resolved per-scope by `renormalize_signatures`,
                    // including `spec`-inner scopes the file cursor cannot reach), so
                    // read field types from it. The AST field is kept only for the
                    // diagnostic's name and source location.
                    let Some(info) = self.symbol_table.lookup_struct_by_key(&target_key) else {
                        continue;
                    };
                    for field in fields {
                        let field_name = arena[field.name].name.clone();
                        let Some(field_info) = info.get_field_info_by_name(&field_name) else {
                            continue;
                        };
                        let field_kind = &field_info.type_info.kind;
                        if self.struct_type_contains(
                            field_kind,
                            &target_key,
                            &mut FxHashSet::default(),
                        ) {
                            self.push_error(TypeCheckError::RecursiveStructDefinition {
                                struct_name: struct_name.clone(),
                                field_name,
                                field_type: field_info.type_info.to_string(),
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

    /// Returns true if `kind` is or transitively contains the struct identified by
    /// `target_key` (a canonical, file-qualified key). Containment is traced
    /// through the contained structs' already-canonicalized field types, so a
    /// cycle that closes across files is detected and same-named structs in
    /// different files stay distinct.
    fn struct_type_contains(
        &self,
        kind: &TypeInfoKind,
        target_key: &str,
        visited: &mut FxHashSet<String>,
    ) -> bool {
        match kind {
            TypeInfoKind::Struct(_, key) => {
                if key == target_key {
                    return true;
                }
                if !visited.insert(key.clone()) {
                    return false;
                }
                // Fields were canonicalized by `renormalize_signatures`, so each
                // field's `Struct` kind already carries its own key — no
                // re-resolution against a (possibly wrong) cursor scope is needed.
                self.symbol_table
                    .lookup_struct_by_key(key)
                    .is_some_and(|info| {
                        info.fields.iter().any(|f| {
                            self.struct_type_contains(&f.type_info.kind, target_key, visited)
                        })
                    })
            }
            // A field whose type stayed unresolved (`Custom`) is a type-alias name:
            // a struct/enum would have canonicalized to `Struct`/`Enum` already.
            // Follow the alias to its underlying type so a cycle that runs through
            // `type X = SomeStruct` is still detected. (Pure alias→alias and
            // const cycles are caught earlier by the definition-graph check; this
            // covers a struct field reaching back through an alias.)
            TypeInfoKind::Custom(name) => {
                if !visited.insert(name.clone()) {
                    return false;
                }
                self.symbol_table.lookup_type(name).is_some_and(|resolved| {
                    self.struct_type_contains(&resolved.kind, target_key, visited)
                })
            }
            TypeInfoKind::Array(elem, _) => {
                self.struct_type_contains(&elem.kind, target_key, visited)
            }
            _ => false,
        }
    }

    /// Registers `Def::Function`, `Def::ExternFunction`, and `Def::Constant`
    fn collect_function_and_constant_definitions(&mut self, ctx: &mut TypedContext) {
        for (module_path, defs) in Self::files_with_defs(ctx) {
            self.enter_file(&module_path);
            for def_id in defs {
                self.collect_for_def(def_id, ctx);
            }
        }
        self.exit_files();
    }

    /// Registers each top-level `const` as a symbol carrying its value type and
    /// visibility, so it is item-importable and reachable by a qualified path
    /// across files (#63).
    ///
    /// Run after functions and structs are registered, and tolerant of a
    /// pre-existing same-named symbol (the const keeps its intra-file scope
    /// variable), so adding the const symbol never turns a latent name clash into
    /// a hard error. Each const's value type is resolved against its own file's
    /// scope so a custom type name resolves correctly.
    fn register_constant_symbols(&mut self, ctx: &mut TypedContext) {
        for (module_path, defs) in Self::files_with_defs(ctx) {
            self.enter_file(&module_path);
            for def_id in defs {
                let (location, kind) = {
                    let def_data = &ctx.arena()[def_id];
                    (def_data.location, def_data.kind.clone())
                };
                if let Def::Constant { name, ty, vis, .. } = &kind {
                    let const_name = ctx.arena()[*name].name.clone();
                    let const_type = self
                        .symbol_table
                        .resolve_custom_type(TypeInfo::from_type_id(ctx.arena(), *ty));
                    if let Err(err) = self.symbol_table.register_constant(
                        &const_name,
                        const_type,
                        vis.clone(),
                        location,
                    ) {
                        self.push_error(TypeCheckError::RegistrationFailed {
                            kind: RegistrationKind::Variable,
                            name: const_name,
                            reason: Some(err.to_string()),
                            location,
                        });
                    }
                }
            }
        }
        self.exit_files();
    }

    /// Type-checks each top-level `const`'s initializer, run after imports and
    /// const symbols are bound.
    ///
    /// A `const`'s value may reference another `const` — including one in a
    /// different file brought in bare by `use a::b::{C};` or named by a qualified
    /// `a::b::C` path. Those resolve only after `resolve_imports` and
    /// `register_constant_symbols`, so the initializer check waits until here;
    /// each file is entered first so its imports are in scope. An acyclic
    /// cross-file `const` chain (guaranteed acyclic by `check_definition_cycles`)
    /// therefore type-checks.
    fn check_const_initializers(&mut self, ctx: &mut TypedContext) {
        for (module_path, defs) in Self::files_with_defs(ctx) {
            self.enter_file(&module_path);
            for def_id in defs {
                let (location, kind) = {
                    let def_data = &ctx.arena()[def_id];
                    (def_data.location, def_data.kind.clone())
                };
                if let Def::Constant { ty, value, .. } = &kind {
                    let const_type = self
                        .symbol_table
                        .resolve_custom_type(TypeInfo::from_type_id(ctx.arena(), *ty));
                    self.check_const_initializer(*value, &const_type, location, ctx);
                }
            }
        }
        self.exit_files();
    }

    /// Validates function and method signature types, run after import resolution.
    ///
    /// Each file is entered before its functions are checked, so a type named in a
    /// param or return position resolves against that file's imports — an
    /// item-imported struct (`use a::b::{T};`) is recognized in a signature
    /// exactly as in a `let` binding (#63). Methods inside a struct, and functions
    /// inside a spec, are validated in the same defining scope they register in.
    fn validate_signatures(&mut self, ctx: &mut TypedContext) {
        for (module_path, defs) in Self::files_with_defs(ctx) {
            self.enter_file(&module_path);
            for def_id in defs {
                self.validate_signature_for_def(def_id, None, ctx);
            }
        }
        self.exit_files();
    }

    /// Validates one definition's signature, recursing into a struct's methods and
    /// a spec's inner definitions.
    ///
    /// `owner` names the struct a method is declared in and is `None` for a free,
    /// spec-inner or `external` function. It is threaded syntactically rather than
    /// re-derived from the symbol table because a spec-inner struct whose name
    /// collides with an already-registered one never registers its methods, and a
    /// collided declaration is exactly the case these diagnostics exist to reach.
    fn validate_signature_for_def(
        &mut self,
        def_id: DefId,
        owner: Option<&str>,
        ctx: &mut TypedContext,
    ) {
        let kind = ctx.arena()[def_id].kind.clone();
        match &kind {
            Def::Function {
                name,
                type_params,
                args,
                returns,
                ..
            } => {
                let label = {
                    let fn_name = &ctx.arena()[*name].name;
                    owner.map_or_else(|| fn_name.clone(), |o| format!("{o}::{fn_name}"))
                };
                self.report_duplicate_parameters(args, &label, ctx);
                // A receiver is only a method receiver when the function is
                // declared inside a struct; elsewhere `SelfReferenceInFunction`
                // and `SelfReferenceOutsideMethod` own the case.
                if owner.is_some()
                    && let Some(position) = args
                        .iter()
                        .position(|a| matches!(a.kind, ArgKind::SelfRef { .. }))
                    && position > 0
                {
                    self.push_error(TypeCheckError::SelfReferenceNotFirstParameter {
                        function_name: label,
                        location: args[position].location,
                    });
                }
                let tp_names: Vec<String> = type_params
                    .iter()
                    .map(|p| ctx.arena()[*p].name.clone())
                    .collect();
                for arg in args {
                    match &arg.kind {
                        ArgKind::Named { ty, .. }
                        | ArgKind::Ignored { ty }
                        | ArgKind::TypeOnly(ty) => {
                            self.validate_type(ctx.arena(), *ty, &tp_names);
                        }
                        ArgKind::SelfRef { .. } => {}
                    }
                }
                if let Some(return_type_id) = returns {
                    self.validate_type(ctx.arena(), *return_type_id, &tp_names);
                }
            }
            // Signature *types* of an extern are validated in `collect_for_def`,
            // which has no type parameters to thread and so needs no second pass;
            // only the parameter names are checked here, where every function-like
            // declaration is reached.
            Def::ExternFunction { name, args, .. } => {
                let func_name = ctx.arena()[*name].name.clone();
                self.report_duplicate_parameters(args, &func_name, ctx);
            }
            Def::Struct {
                name,
                fields,
                methods,
                ..
            } => {
                // Field types are validated here, after imports resolve, so a
                // field declared with an item-imported or `::`-qualified type is
                // recognized exactly as a signature type is. A bad field type
                // (a typo'd qualifier or leaf) is reported rather than silently
                // accepted and later panicking code generation (#63).
                for field in fields {
                    self.validate_type(ctx.arena(), field.ty, &[]);
                }
                let struct_name = ctx.arena()[*name].name.clone();
                let method_ids: Vec<DefId> = methods.clone();
                for method_id in method_ids {
                    self.validate_signature_for_def(method_id, Some(&struct_name), ctx);
                }
            }
            Def::Spec { name, defs, .. } => {
                let spec_name = ctx.arena()[*name].name.clone();
                let inner: Vec<DefId> = defs.clone();
                let mut guard = SpecScopeGuard::enter(self, &spec_name);
                for inner_id in inner {
                    guard.validate_signature_for_def(inner_id, None, ctx);
                }
            }
            _ => {}
        }
    }

    /// Reports each parameter name bound more than once in one function-like
    /// declaration, anchored at the parameter that repeats it.
    ///
    /// `_: T` and a bare positional type bind no name, so repeats of those are
    /// legal and skipped. A receiver participates under the name `self`, which no
    /// named parameter can claim — the parser rejects `self:` outright — so one set
    /// covers both spellings.
    fn report_duplicate_parameters(
        &mut self,
        args: &[ArgData],
        function_name: &str,
        ctx: &TypedContext,
    ) {
        let mut bound = FxHashSet::default();
        for arg in args {
            let parameter_name = match &arg.kind {
                ArgKind::Named { name, .. } => ctx.arena()[*name].name.clone(),
                ArgKind::SelfRef { .. } => "self".to_string(),
                ArgKind::Ignored { .. } | ArgKind::TypeOnly(_) => continue,
            };
            if !bound.insert(parameter_name.clone()) {
                self.push_error(TypeCheckError::DuplicateParameterName {
                    function_name: function_name.to_string(),
                    parameter_name,
                    location: arg.location,
                });
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn collect_for_def(&mut self, def_id: DefId, ctx: &mut TypedContext) {
        let (location, kind) = {
            let arena = ctx.arena();
            let def_data = &arena[def_id];
            (def_data.location, def_data.kind.clone())
        };
        match &kind {
            Def::Constant { name, ty, .. } => {
                let const_name = ctx.arena()[*name].name.clone();
                let const_type = self
                    .symbol_table
                    .resolve_custom_type(TypeInfo::from_type_id(ctx.arena(), *ty));
                // Register as a scope variable so an intra-file use site resolves
                // the const by value. The importable / qualified-resolvable const
                // *symbol* is added in a later pass (`register_constant_symbols`),
                // so it never makes a same-named function/struct registration fail.
                //
                // The initializer itself is type-checked later, in
                // `check_const_initializers` after imports resolve, so a `const`
                // may reference a cross-file `const` brought in by `use a::b::{C};`
                // or named by a qualified path.
                if let Err(err) =
                    self.symbol_table
                        .push_variable_to_scope(&const_name, const_type, false)
                {
                    self.push_error(TypeCheckError::RegistrationFailed {
                        kind: RegistrationKind::Variable,
                        name: const_name,
                        reason: Some(err.to_string()),
                        location,
                    });
                }
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

                // Signature types are validated in a dedicated pass after import
                // resolution (`validate_signatures`), so an item-imported type
                // (`use a::b::{T};`) is recognized in a param/return position the
                // same as in a `let` binding. Registration below keeps unresolved
                // `Custom` names; the validation pass reports any that never
                // resolve.
                for arg in args {
                    match &arg.kind {
                        ArgKind::SelfRef { .. } => {
                            self.push_error(TypeCheckError::SelfReferenceInFunction {
                                function_name: func_name.clone(),
                                location: arg.location,
                            });
                        }
                        ArgKind::Named {
                            name: arg_name, ty, ..
                        } => {
                            let type_info = TypeInfo::from_type_id_with_type_params(
                                ctx.arena(),
                                *ty,
                                &tp_names,
                            );
                            ctx.set_node_typeinfo(NodeId::Ident(*arg_name), type_info);
                        }
                        ArgKind::Ignored { .. } | ArgKind::TypeOnly(_) => {}
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
                    let return_type_info = TypeInfo::from_type_id_with_type_params(
                        ctx.arena(),
                        *return_type_id,
                        &tp_names,
                    );
                    ctx.set_node_typeinfo(NodeId::Type(*return_type_id), return_type_info);
                }
                // Register function even if parameter validation had errors.
                // Types and names are produced by one pass so the receiver is
                // dropped from both, keeping them index-aligned with the
                // arguments a call site writes.
                let (param_types, param_names): (Vec<TypeInfo>, Vec<Option<String>>) = args
                    .iter()
                    .filter_map(|a| match &a.kind {
                        ArgKind::SelfRef { .. } => None,
                        ArgKind::Named { ty, name, .. } => Some((
                            TypeInfo::from_type_id_with_type_params(ctx.arena(), *ty, &tp_names),
                            Some(ctx.arena()[*name].name.clone()),
                        )),
                        ArgKind::Ignored { ty } | ArgKind::TypeOnly(ty) => Some((
                            TypeInfo::from_type_id_with_type_params(ctx.arena(), *ty, &tp_names),
                            None,
                        )),
                    })
                    .unzip();
                let return_type = returns
                    .map(|r| TypeInfo::from_type_id_with_type_params(ctx.arena(), r, &tp_names))
                    .unwrap_or_default();
                if let Err(err) = self.symbol_table.register_function_with_visibility(
                    &func_name,
                    tp_names,
                    param_types,
                    param_names,
                    return_type,
                    func_vis,
                    location,
                ) {
                    self.push_error(TypeCheckError::RegistrationFailed {
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
                            self.push_error(TypeCheckError::SelfReferenceInFunction {
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
                // Types and names are produced by one pass so a (rejected)
                // receiver is dropped from both, keeping them index-aligned with
                // the arguments a call site writes. An extern written by type
                // alone binds no name, so its labels are all `None`.
                let (param_types, param_names): (Vec<TypeInfo>, Vec<Option<String>>) = args
                    .iter()
                    .filter_map(|a| match &a.kind {
                        ArgKind::SelfRef { .. } => None,
                        ArgKind::Named { ty, name, .. } => Some((
                            TypeInfo::from_type_id(ctx.arena(), *ty),
                            Some(ctx.arena()[*name].name.clone()),
                        )),
                        ArgKind::Ignored { ty } | ArgKind::TypeOnly(ty) => {
                            Some((TypeInfo::from_type_id(ctx.arena(), *ty), None))
                        }
                    })
                    .unzip();
                let return_type = returns
                    .map(|r| TypeInfo::from_type_id(ctx.arena(), r))
                    .unwrap_or_default();
                // A declaration that collides with a same-file function is left
                // out of the symbol table: the function already holds the
                // symbol, and inserting over it would raise a second, generic
                // diagnostic for a collision already reported precisely.
                if self.same_file_extern_collisions.contains(&def_id) {
                    return;
                }
                let origin = self.extern_module_bindings.get(&def_id).cloned();
                if let Err(err) = self.symbol_table.register_extern_function(
                    &func_name,
                    param_types,
                    param_names,
                    return_type,
                    origin,
                ) {
                    self.push_error(TypeCheckError::RegistrationFailed {
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
    /// The declared type is what a bare integer literal initializer denotes;
    /// anything that cannot denote it is reported as a mismatch. A matching
    /// initializer is re-stamped with the *resolved* constant type, which
    /// normalizes an unresolved `Custom` to its canonical-keyed form.
    fn check_const_initializer(
        &mut self,
        value_id: ExprId,
        const_type: &TypeInfo,
        location: Location,
        ctx: &mut TypedContext,
    ) {
        // A `const` array initializer may carry a `@` element (`const A:
        // [i32; 2] = [0, @]`), which inherits the constant's element type.
        // Thread it before inference (a no-op for non-array/non-uzumaki
        // values) so the `@` is typed; this initializer is otherwise lowered
        // element-by-element with no enclosing variable, so a `@` element
        // panics codegen. A compound element is rejected by analysis (A040).
        if matches!(ctx.arena()[value_id].kind, Expr::ArrayLiteral { .. })
            && matches!(const_type.kind, TypeInfoKind::Array(_, _))
        {
            self.thread_array_uzumaki_types(ctx, value_id, const_type);
        }
        let source = TypeMismatchContext::VariableDefinition;
        let init_type = self.infer_expression_expecting(
            value_id,
            Some(Expected::new(const_type, &source)),
            ctx,
        );
        match init_type {
            Some(init) if self.symbol_table.resolve_custom_type(init.clone()) != *const_type => {
                self.push_error(TypeCheckError::TypeMismatch {
                    expected: const_type.clone(),
                    found: init,
                    context: source,
                    location,
                });
            }
            Some(_) => {
                ctx.set_node_typeinfo(NodeId::Expr(value_id), const_type.clone());
            }
            None => {}
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
            TypeNode::Function { .. } | TypeNode::QualifiedName { .. } => {}
            TypeNode::Qualified { .. } => {
                if let Some(path) = type_data.kind.qualified_segments(arena) {
                    self.validate_qualified_type(&path, location);
                }
            }
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

    /// Validates a `::`-qualified type annotation (`geo::Level`,
    /// `lib::geom::Point`): that the path names a struct or enum reachable from the
    /// current scope, and that the named type is visible at the access site.
    ///
    /// A path that does not resolve to a nominal type is reported as an
    /// [`TypeCheckError::UnknownType`] — the same diagnostic a bad bare type gets —
    /// so a typo in the qualifier or leaf fails cleanly instead of silently leaving
    /// an unresolved annotation. When the failure is instead a missing import — the
    /// path's namespace prefix names a real project file the accessing file never
    /// imported — it is reported as that missing import (mirroring the call and
    /// const sites), so a leaked `lib::geom::Point` annotation points at the `use`
    /// to add rather than cascading a misleading "struct Point is not defined". A
    /// resolved-but-private type is reported through the shared visibility gate,
    /// pointing at its declaration, so reaching another file's private type by
    /// qualifier is rejected the way every other cross-file private access is.
    fn validate_qualified_type(&mut self, path: &[String], location: Location) {
        let from_scope = self.symbol_table.current_scope_id().unwrap_or(0);
        match self
            .symbol_table
            .resolve_qualified_type_path(path, from_scope)
        {
            Some(ResolvedNominalType::Struct(info, _)) => {
                self.check_and_report_visibility(
                    &info.visibility,
                    info.definition_scope_id,
                    info.definition_location,
                    &location,
                    VisibilityContext::Struct { name: info.name },
                );
            }
            Some(ResolvedNominalType::Enum(info, _)) => {
                self.check_and_report_visibility(
                    &info.visibility,
                    info.definition_scope_id,
                    info.definition_location,
                    &location,
                    VisibilityContext::Enum { name: info.name },
                );
            }
            None => {
                if let Some(diagnosis) = self
                    .symbol_table
                    .unimported_namespace_prefix(path, from_scope)
                {
                    self.report_unimported_namespace(diagnosis, path, location);
                } else {
                    self.push_error_dedup(TypeCheckError::UnknownType {
                        name: path.join("::"),
                        location,
                    });
                }
            }
        }
    }

    /// Validates that an array type annotation has a valid size.
    ///
    /// A literal size must be a positive `u32`; zero or an out-of-range value is
    /// reported as `InvalidArraySize` against the whole array type. A named
    /// constant (`[i32; N]`) is reported as `NonLiteralArraySize` against the size
    /// identifier itself, since compile-time constant evaluation of array sizes is
    /// not yet implemented (#79) — this is the diagnostic that replaces the former
    /// `todo!` panic (#240). Any other size expression is unreachable from accepted
    /// syntax (the parser rejects arithmetic in size position) and is left to the
    /// `0` sentinel [`TypeInfo::from_type_id`](crate::type_info::TypeInfo) records.
    fn validate_array_size(
        &mut self,
        arena: &AstArena,
        size_expr_id: ExprId,
        type_location: Location,
    ) {
        match &arena[size_expr_id].kind {
            Expr::NumberLiteral { value } => {
                let value = value.clone();
                match value.parse::<u32>() {
                    Ok(1..) => {}
                    Ok(0) | Err(_) => {
                        self.push_error_dedup(TypeCheckError::InvalidArraySize {
                            size: value,
                            location: type_location,
                        });
                    }
                }
            }
            Expr::Identifier(ident_id) => {
                let name = arena[*ident_id].name.clone();
                let location = arena[size_expr_id].location;
                self.push_error(TypeCheckError::NonLiteralArraySize { name, location });
            }
            _ => {}
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

        // A repeated parameter name is reported once, at the signature, by
        // `report_duplicate_parameters`. Only the first of the repeats is bound, so
        // this pass neither re-reports the collision nor lets a body reference go
        // unresolved; a body binding that collides with a parameter is a distinct
        // mistake and still reaches `push_variable_to_scope` below.
        let mut bound = FxHashSet::default();
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
                    if !bound.insert(name_str.clone()) {
                        continue;
                    }
                    if let Err(err) = self
                        .symbol_table
                        .push_variable_to_scope(&name_str, arg_type, *is_mut)
                    {
                        self.push_error(TypeCheckError::RegistrationFailed {
                            kind: RegistrationKind::Variable,
                            name: name_str,
                            reason: Some(err.to_string()),
                            location: arg.location,
                        });
                    }
                }
                ArgKind::SelfRef { .. } => {
                    self.push_error(TypeCheckError::SelfReferenceOutsideMethod {
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
        // As in `infer_variables`: a repeated parameter name — `self` included — is
        // reported at the signature, and binding only the first of the repeats
        // leaves the collision out of this pass without unresolving the body.
        let mut bound = FxHashSet::default();
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
                    if !bound.insert(name_str.clone()) {
                        continue;
                    }
                    if let Err(err) = self
                        .symbol_table
                        .push_variable_to_scope(&name_str, arg_type, *is_mut)
                    {
                        self.push_error(TypeCheckError::RegistrationFailed {
                            kind: RegistrationKind::Variable,
                            name: name_str,
                            reason: Some(err.to_string()),
                            location: arg.location,
                        });
                    }
                }
                ArgKind::SelfRef { is_mut } => {
                    if !bound.insert("self".to_string()) {
                        continue;
                    }
                    if let Err(err) =
                        self.symbol_table
                            .push_variable_to_scope("self", self_type.clone(), *is_mut)
                    {
                        self.push_error(TypeCheckError::RegistrationFailed {
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

    /// Threads `expected` onto the uzumaki (`@`) leaves of an array-literal
    /// initializer so codegen can choose an opcode/width for each `@`. Recurses
    /// through nested array literals; a `@` element of a multidimensional array is
    /// typed as its (array) element type, which lets analysis (A040) reject it as
    /// a compound element. Only `@` leaves receive type info — number literals and
    /// every other element kind keep their existing bottom-up inference, so no
    /// previously-compiling program changes its emitted bytes (a `@` element never
    /// compiled before; it panicked codegen).
    ///
    /// `expected` must already be scope-resolved (a `Struct`/`Enum` carries a
    /// canonical key, not a bare `Custom`/`Qualified`): all four call sites pass
    /// a type whose array element was resolved against the file that owns it (the
    /// annotation's file for `let`/`const`/assignment, the defining file for a
    /// struct field). The inner `resolve_custom_type` is therefore an idempotent no-op
    /// kept as a guard; it must not be relied on to resolve a still-`Custom`
    /// element, which it would resolve against the current cursor scope.
    fn thread_array_uzumaki_types(
        &mut self,
        ctx: &mut TypedContext,
        expr_id: ExprId,
        expected: &TypeInfo,
    ) {
        match ctx.arena()[expr_id].kind.clone() {
            Expr::Uzumaki => ctx.set_node_typeinfo(NodeId::Expr(expr_id), expected.clone()),
            Expr::ArrayLiteral { elements } => {
                if let TypeInfoKind::Array(elem_type, _) = &expected.kind {
                    let elem_type = self.symbol_table.resolve_custom_type((**elem_type).clone());
                    for e in elements {
                        self.thread_array_uzumaki_types(ctx, e, &elem_type);
                    }
                }
            }
            _ => {}
        }
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
                        self.push_error(TypeCheckError::AssignToImmutable { name, location });
                    }
                } else {
                    self.push_error(TypeCheckError::InvalidAssignmentTarget { location });
                }
                let target_type = self.infer_expression(left, ctx);
                let arena = ctx.arena();
                if let Expr::Uzumaki = &arena[right].kind {
                    if let Some(target) = &target_type {
                        ctx.set_node_typeinfo(NodeId::Expr(right), target.clone());
                    } else {
                        cov_mark::hit!(type_checker_uzumaki_cannot_infer_type);
                        self.push_error(TypeCheckError::CannotInferUzumakiType {
                            location: ctx.arena()[right].location,
                        });
                    }
                } else {
                    // An array-literal RHS may carry a `@` element (`a = [0, @]`),
                    // which inherits the assignment target's element type. Thread
                    // it before inference (a no-op for non-array/non-uzumaki RHS) so
                    // the `@` is typed; this assignment position is otherwise
                    // unguarded and a `@` element panics codegen. A compound element
                    // is rejected by analysis (A040).
                    if let Some(target) = &target_type
                        && matches!(ctx.arena()[right].kind, Expr::ArrayLiteral { .. })
                    {
                        self.thread_array_uzumaki_types(ctx, right, target);
                    }
                    // The target's type is what a bare integer literal on the
                    // right denotes; a value that cannot denote it is reported
                    // once, by the mismatch check below.
                    let source = TypeMismatchContext::Assignment;
                    let value_type = self.infer_expression_expecting(
                        right,
                        target_type.as_ref().map(|ty| Expected::new(ty, &source)),
                        ctx,
                    );
                    // Compound-return-in-assignment check moved to analysis rule A017.
                    if let (Some(target), Some(val)) = (target_type, value_type)
                        && target != val
                    {
                        self.push_error(TypeCheckError::TypeMismatch {
                            expected: target,
                            found: val,
                            context: source,
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
                    // The declared return type is what a bare integer literal
                    // operand denotes; a value that cannot denote it is reported
                    // once, by the mismatch check below.
                    let source = TypeMismatchContext::Return;
                    let value_type = self.infer_expression_expecting(
                        expr,
                        Some(Expected::new(return_type, &source)),
                        ctx,
                    );
                    // Comparing a real return value against a declared return array
                    // whose size was already rejected would only add a confusing
                    // second error, so the mismatch is skipped for it.
                    let size_rejected = return_type.has_rejected_array_size();
                    if !size_rejected && *return_type != value_type.clone().unwrap_or_default() {
                        self.push_error(TypeCheckError::TypeMismatch {
                            expected: return_type.clone(),
                            found: value_type.unwrap_or_default(),
                            context: source,
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
                // The initializer-consistency checks below would only stack
                // follow-on errors onto a declared array whose size was already
                // rejected, so they are skipped for it.
                let size_rejected = target_type.has_rejected_array_size();
                if let Some(expr_id) = value {
                    let expr_kind = ctx.arena()[expr_id].kind.clone();
                    if let Expr::ArrayLiteral { elements } = &expr_kind
                        && let TypeInfoKind::Array(_, expected_size) = target_type.kind
                        && !size_rejected
                        && elements.len() != expected_size as usize
                    {
                        self.push_error(TypeCheckError::ArrayLiteralSizeMismatch {
                            expected: expected_size,
                            actual: elements.len(),
                            location,
                        });
                    }
                    // An array-literal initializer may carry a `@` element (`let a:
                    // [i32; 2] = [0, @]`), which inherits the variable's element
                    // type. Thread it after the number-literal pass and before
                    // inference so the `@` is typed; an array-element `@` is
                    // otherwise lowered with no enclosing variable and panics
                    // codegen. A compound element is rejected by analysis (A040).
                    if matches!(ctx.arena()[expr_id].kind, Expr::ArrayLiteral { .. })
                        && matches!(target_type.kind, TypeInfoKind::Array(_, _))
                    {
                        self.thread_array_uzumaki_types(ctx, expr_id, &target_type);
                    }
                    let source = TypeMismatchContext::VariableDefinition;
                    let arena = ctx.arena();
                    if let Expr::Uzumaki = &arena[expr_id].kind {
                        ctx.set_node_typeinfo(NodeId::Expr(expr_id), target_type.clone());
                    } else if let Some(init_type) = self.infer_expression_expecting(
                        expr_id,
                        Some(Expected::new(&target_type, &source)),
                        ctx,
                    ) && !size_rejected
                        && self.symbol_table.resolve_custom_type(init_type.clone()) != target_type
                    {
                        self.push_error(TypeCheckError::TypeMismatch {
                            expected: target_type.clone(),
                            found: init_type,
                            context: source,
                            location,
                        });
                    }
                }
                if self
                    .symbol_table
                    .lookup_variable_in_parent_scopes(&var_name)
                    .is_some()
                {
                    self.push_error(TypeCheckError::VariableShadowed {
                        name: var_name.clone(),
                        location,
                    });
                }
                if let Err(err) =
                    self.symbol_table
                        .push_variable_to_scope(&var_name, target_type.clone(), is_mut)
                {
                    self.push_error(TypeCheckError::RegistrationFailed {
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
                    self.push_error(TypeCheckError::RegistrationFailed {
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
                        self.push_error(TypeCheckError::VariableShadowed {
                            name: const_name.clone(),
                            location,
                        });
                    }
                    if let Err(err) = self.symbol_table.push_variable_to_scope(
                        &const_name,
                        constant_type.clone(),
                        false,
                    ) {
                        self.push_error(TypeCheckError::RegistrationFailed {
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
            self.push_error(TypeCheckError::TypeMismatch {
                expected: TypeInfo::boolean(),
                found: expr_type.unwrap_or_default(),
                context,
                location: ctx.arena()[expr_id].location,
            });
        }
    }

    /// Infers the type of `expr_id` with nothing expected of it by its
    /// surroundings — the form used by every position that has no type to
    /// require, such as an expression statement or a condition.
    fn infer_expression(&mut self, expr_id: ExprId, ctx: &mut TypedContext) -> Option<TypeInfo> {
        self.infer_expression_expecting(expr_id, None, ctx)
    }

    /// Whether `expr_id` is built entirely out of integer literals, so that a
    /// type expected of it can reach every one of its leaves.
    ///
    /// Closure is syntactic and needs no types: an integer literal is closed,
    /// and parentheses, `-`, `~` and the arithmetic, bitwise and shift
    /// operators preserve closure. Nothing else does — `!` takes a boolean
    /// operand, and comparison, equality and logical operators produce `bool`
    /// whatever their operands are, so a type expected of them says nothing
    /// about the operands.
    fn is_literal_closed(arena: &AstArena, expr_id: ExprId) -> bool {
        match &arena[expr_id].kind {
            Expr::NumberLiteral { .. } => true,
            Expr::Parenthesized { expr } => Self::is_literal_closed(arena, *expr),
            Expr::PrefixUnary { expr, op } => {
                matches!(op, UnaryOperatorKind::Neg | UnaryOperatorKind::BitNot)
                    && Self::is_literal_closed(arena, *expr)
            }
            Expr::Binary { left, right, op } => {
                Self::operator_preserves_operand_type(op)
                    && Self::is_literal_closed(arena, *left)
                    && Self::is_literal_closed(arena, *right)
            }
            _ => false,
        }
    }

    /// Whether `op` yields a value of its operands' own type, which is what
    /// makes a type expected of the whole expression also expected of both
    /// operands. The comparison, equality and logical operators do not.
    fn operator_preserves_operand_type(op: &OperatorKind) -> bool {
        matches!(
            op,
            OperatorKind::Add
                | OperatorKind::Sub
                | OperatorKind::Mul
                | OperatorKind::Div
                | OperatorKind::Mod
                | OperatorKind::BitAnd
                | OperatorKind::BitOr
                | OperatorKind::BitXor
                | OperatorKind::Shl
                | OperatorKind::Shr
        )
    }

    /// Infers both operands of a binary expression, peer-typing a
    /// literal-closed operand against the other one.
    ///
    /// A literal-closed operand has no type of its own to contribute, so it
    /// takes the type of its peer — `a << 16` shifts an `i64` by an `i64` when
    /// `a` is `i64`. The peer is inferred first for that reason, and because it
    /// keeps the diagnostic for the ordinary mistake at the binding rather than
    /// at the operator: `let a: i32 = 3; let x: i64 = a + 1;` still reports the
    /// variable definition, not a mismatch between operand types.
    ///
    /// When both operands are literal-closed neither can inform the other, and
    /// the type expected of the whole expression is what fixes them — but only
    /// for an operator that yields its operands' type. When neither is
    /// literal-closed both operands already carry their own types.
    ///
    /// A peer-typed operand is attributed to the operator rather than to
    /// whatever the surroundings expected, because the operator is what put its
    /// neighbour's type on it; an operand typed by descent keeps the outer
    /// attribution, which is where its type really came from.
    fn infer_binary_operands(
        &mut self,
        left: ExprId,
        right: ExprId,
        op: &OperatorKind,
        expected: Option<Expected<'_>>,
        ctx: &mut TypedContext,
    ) -> (Option<TypeInfo>, Option<TypeInfo>) {
        let arena = ctx.arena();
        let left_closed = Self::is_literal_closed(arena, left);
        let right_closed = Self::is_literal_closed(arena, right);
        match (left_closed, right_closed) {
            (true, true) => {
                let descended = expected
                    .filter(|e| e.ty.kind.is_number() && Self::operator_preserves_operand_type(op));
                (
                    self.infer_expression_expecting(left, descended, ctx),
                    self.infer_expression_expecting(right, descended, ctx),
                )
            }
            (true, false) => {
                let right_type = self.infer_expression(right, ctx);
                let peer_source = TypeMismatchContext::BinaryPeerOperand(op.clone());
                let peer = right_type
                    .as_ref()
                    .filter(|t| t.kind.is_number())
                    .map(|ty| Expected::new(ty, &peer_source));
                let left_type = self.infer_expression_expecting(left, peer, ctx);
                (left_type, right_type)
            }
            (false, true) => {
                let left_type = self.infer_expression(left, ctx);
                let peer_source = TypeMismatchContext::BinaryPeerOperand(op.clone());
                let peer = left_type
                    .as_ref()
                    .filter(|t| t.kind.is_number())
                    .map(|ty| Expected::new(ty, &peer_source));
                let right_type = self.infer_expression_expecting(right, peer, ctx);
                (left_type, right_type)
            }
            (false, false) => (
                self.infer_expression(left, ctx),
                self.infer_expression(right, ctx),
            ),
        }
    }

    /// Infers the type of `expr_id` under the type its position requires.
    ///
    /// An integer literal has no intrinsic type; it denotes whatever integer
    /// type the surrounding position asks for, and only falls back to `i32` when
    /// nothing is asked of it. Positions that know the type they require — the
    /// initializer of an annotated `let`/`const`, the right-hand side of an
    /// assignment, a struct-literal field value, an element of an array
    /// literal, a call argument and the operand of `return` — pass it as
    /// `expected` and then run their ordinary post-inference mismatch check,
    /// which reports the one diagnostic when the value cannot denote that type.
    ///
    /// An expected type descends unchanged through the forms that yield their
    /// operand's own type — `( e )`, `-e`, `~e`, and both operands of an
    /// arithmetic, bitwise or shift operator when neither operand has a type of
    /// its own. Nothing else carries it: an array index, the operands of a
    /// comparison, equality or logical operator, a receiver and a call result
    /// all keep the types they already have.
    ///
    /// This is not a coercion: nothing is converted and no expression that
    /// already has a type ever changes it.
    #[allow(clippy::too_many_lines)]
    fn infer_expression_expecting(
        &mut self,
        expr_id: ExprId,
        expected: Option<Expected<'_>>,
        ctx: &mut TypedContext,
    ) -> Option<TypeInfo> {
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
                        self.push_error(TypeCheckError::ArrayIndexNotNumeric {
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
                            self.push_error(TypeCheckError::ExpectedArrayType {
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
                    // The receiver type carries the struct's bare name and its
                    // file-qualified canonical key. The struct is resolved by that
                    // key so a value whose struct lives in another file — including
                    // one reached via `root::` / a namespace, which is not bare-
                    // visible at the access site — still finds its layout (#63).
                    let struct_name = match &object_type.kind {
                        TypeInfoKind::Struct(name, _) => Some(name.clone()),
                        TypeInfoKind::Custom(name) => {
                            if self.symbol_table.lookup_struct(name).is_some() {
                                Some(name.clone())
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    let struct_key = match &object_type.kind {
                        TypeInfoKind::Struct(_, key) => Some(key.clone()),
                        _ => None,
                    };

                    if let Some(struct_name) = struct_name {
                        let field_name = ctx.arena()[name].name.clone();
                        let resolved_struct = struct_key
                            .as_deref()
                            .and_then(|key| self.symbol_table.lookup_struct_by_key(key))
                            .or_else(|| self.symbol_table.lookup_struct(&struct_name));
                        if let Some(struct_info) = resolved_struct {
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
                                // A struct field's type is written in the file
                                // that *defines* the struct, so its bare name must
                                // resolve against that file's scope and imports —
                                // not the access site's. Resolving here stamps the
                                // field's canonical key from the defining file, so
                                // a nested cross-file struct field lays out and
                                // reads at the correct offset (#63).
                                let field_type = self.symbol_table.resolve_custom_type_in_scope(
                                    field_info.type_info.clone(),
                                    struct_info.definition_scope_id,
                                );
                                ctx.set_node_typeinfo(NodeId::Expr(expr_id), field_type.clone());
                                Some(field_type)
                            } else {
                                self.push_error(TypeCheckError::FieldNotFound {
                                    struct_name,
                                    field_name,
                                    location,
                                });
                                None
                            }
                        } else {
                            self.push_error(TypeCheckError::FieldNotFound {
                                struct_name,
                                field_name,
                                location,
                            });
                            None
                        }
                    } else {
                        self.push_error(TypeCheckError::ExpectedStructType {
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

                // A `::`-qualified path that names a top-level `const` in another
                // file (`limits::MAX`) resolves through the file scope tree before
                // the enum-variant handling below; that path can never name an enum
                // variant, so resolving it here does not shadow variant access.
                if let Some(result) = self.try_infer_qualified_const(expr_id, ctx) {
                    return result;
                }

                // A namespace-qualified enum variant (`geo::Color::Green`) reaches
                // an enum *inside* an imported file; resolve it before the local
                // enum-variant handling, which only understands a single qualifier.
                if let Some(result) = self.try_infer_namespace_qualified_enum_variant(expr_id, ctx)
                {
                    return result;
                }

                // A path through a known namespace whose final segment is not a
                // value (`lib::vals::X`, `lib::vals::add` in value position) is a
                // namespace-access error, not an enum-variant access. Diagnosing it
                // here avoids the misleading "enum `lib` is not defined" the
                // variant fallback would emit for the namespace head.
                if let Some(result) = self.try_diagnose_qualified_path(expr_id, ctx) {
                    return result;
                }

                let arena = ctx.arena();
                let enum_name = match &arena[inner_expr].kind {
                    Expr::Type(ty_id) => {
                        let type_data = &arena[*ty_id];
                        match &type_data.kind {
                            TypeNode::Custom(ident_id) => arena[*ident_id].name.clone(),
                            _ => {
                                let type_info = TypeInfo::from_type_id(arena, *ty_id);
                                self.push_error(TypeCheckError::ExpectedEnumType {
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
                            TypeInfoKind::Enum(name, _) => name.clone(),
                            _ => {
                                self.push_error(TypeCheckError::ExpectedEnumType {
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
                        // Key the enum literal's type by the enum's defining file
                        // so `Signal::Go` from one file is not assignable where a
                        // same-named enum from another file is expected.
                        let enum_key = self
                            .symbol_table
                            .canonical_key_for_scope(enum_info.definition_scope_id, &enum_name);
                        let enum_type = TypeInfo {
                            kind: TypeInfoKind::Enum(enum_name, enum_key),
                            type_params: vec![],
                        };
                        ctx.set_node_typeinfo(NodeId::Expr(expr_id), enum_type.clone());
                        Some(enum_type)
                    } else {
                        cov_mark::hit!(type_checker_variant_not_found);
                        self.push_error(TypeCheckError::VariantNotFound {
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
                let qualified_name = ctx.arena()[name].name.clone();
                // A namespace-qualified literal (`geo::Point { .. }`) resolves the
                // struct through the imported file; the bare struct name is what the
                // field checks and diagnostics report. A plain name resolves locally.
                let (struct_name, resolved) =
                    self.resolve_struct_literal_target(&qualified_name, location);
                let struct_type = resolved.as_ref().map(|(_, ty)| ty.clone());
                if let Some(struct_type) = struct_type {
                    if let Some((struct_info, _)) = resolved {
                        let fields_copy: Vec<_> =
                            fields.iter().map(|(id, expr)| (*id, *expr)).collect();
                        let mut seen_fields = FxHashSet::default();
                        for (field_name_id, field_value_expr) in &fields_copy {
                            let field_name = ctx.arena()[*field_name_id].name.clone();
                            let field_loc = ctx.arena()[*field_name_id].location;
                            if !seen_fields.insert(field_name.clone()) {
                                self.push_error(TypeCheckError::DuplicateStructField {
                                    struct_name: struct_name.clone(),
                                    field_name,
                                    location: field_loc,
                                });
                                continue;
                            }
                            if let Some(field_info) =
                                struct_info.get_field_info_by_name(&field_name)
                            {
                                // A field's type is written in the struct's
                                // defining file; resolve its bare name against that
                                // file's scope so a cross-file struct literal checks
                                // (and keys) the field against the same type the
                                // defining file sees (#63).
                                let field_type = self.symbol_table.resolve_custom_type_in_scope(
                                    field_info.type_info.clone(),
                                    struct_info.definition_scope_id,
                                );
                                let (field_expr_kind, field_expr_loc) = {
                                    let arena = ctx.arena();
                                    (
                                        arena[*field_value_expr].kind.clone(),
                                        arena[*field_value_expr].location,
                                    )
                                };
                                if let Expr::Uzumaki = field_expr_kind {
                                    // A field-position uzumaki (`Point { x: @ }`)
                                    // carries no type of its own; it takes the
                                    // field's declared type. Threading it here is
                                    // what lets codegen emit the right-width uzumaki
                                    // for the field — the same way an argument-
                                    // position uzumaki inherits its parameter type.
                                    // Without this the node has no `TypeInfo` and
                                    // proof-mode codegen cannot pick an opcode.
                                    ctx.set_node_typeinfo(
                                        NodeId::Expr(*field_value_expr),
                                        field_type.clone(),
                                    );
                                } else {
                                    // An array-typed field whose value is an array
                                    // literal may carry a `@` element (`Holder { arr:
                                    // [0, @] }`), which inherits the field's element
                                    // type. Thread it before inference (a no-op for a
                                    // non-array/non-uzumaki value) so the `@` is typed;
                                    // a compound element is rejected by analysis (A040).
                                    self.thread_array_uzumaki_types(
                                        ctx,
                                        *field_value_expr,
                                        &field_type,
                                    );
                                    let source = TypeMismatchContext::StructField {
                                        struct_name: struct_name.clone(),
                                        field_name: field_name.clone(),
                                    };
                                    let init_type = self.infer_expression_expecting(
                                        *field_value_expr,
                                        Some(Expected::new(&field_type, &source)),
                                        ctx,
                                    );
                                    if let Some(init) = init_type
                                        && init != field_type
                                    {
                                        self.push_error(TypeCheckError::TypeMismatch {
                                            expected: field_type.clone(),
                                            found: init,
                                            context: source,
                                            location: field_expr_loc,
                                        });
                                    }
                                }
                            } else {
                                self.push_error(TypeCheckError::UnknownStructField {
                                    struct_name: struct_name.clone(),
                                    field_name,
                                    location: field_loc,
                                });
                            }
                        }
                        for field_info in &struct_info.fields {
                            if !seen_fields.contains(&field_info.name) {
                                self.push_error(TypeCheckError::MissingStructField {
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
                // A qualified struct literal (`lib::geom::Point { .. }`) that did not
                // resolve may be an unimported-namespace leak rather than an unknown
                // struct: when the namespace prefix names a real project file the
                // accessing file never imported, report the missing import so the fix
                // points at the `use` — keeping the literal position uniform with the
                // call, type-annotation, and const positions, none of which cascade a
                // misleading "struct `Point` is not defined" (#63).
                let from_scope = self.symbol_table.current_scope_id().unwrap_or(0);
                let segments: Vec<String> =
                    qualified_name.split("::").map(str::to_string).collect();
                if segments.len() >= 2
                    && let Some(diagnosis) = self
                        .symbol_table
                        .unimported_namespace_prefix(&segments, from_scope)
                {
                    self.report_unimported_namespace(diagnosis, &segments, location);
                    return None;
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
                        self.push_error(TypeCheckError::InvalidUnaryOperand {
                            operator: UnaryOperatorKind::Not,
                            expected_type: "booleans",
                            found_type: expression_type,
                            location,
                        });
                    }
                    None
                }
                UnaryOperatorKind::Neg => {
                    // `-` yields its operand's type, so a type expected of the
                    // negation is expected of the operand. The signedness check
                    // below still runs on what the operand came back as, which
                    // is what rejects `-` under an unsigned expected type.
                    let expression_type_op = self.infer_expression_expecting(expr, expected, ctx);
                    if let Some(expression_type) = expression_type_op {
                        if expression_type.is_signed_integer() {
                            ctx.set_node_typeinfo(NodeId::Expr(expr_id), expression_type.clone());
                            return Some(expression_type);
                        }
                        self.push_error(TypeCheckError::InvalidUnaryOperand {
                            operator: UnaryOperatorKind::Neg,
                            expected_type: "signed integers (i8, i16, i32, i64)",
                            found_type: expression_type,
                            location,
                        });
                    }
                    None
                }
                UnaryOperatorKind::BitNot => {
                    // `~` yields its operand's type, so a type expected of the
                    // complement is expected of the operand.
                    let expression_type_op = self.infer_expression_expecting(expr, expected, ctx);
                    if let Some(expression_type) = expression_type_op {
                        if expression_type.is_number() {
                            ctx.set_node_typeinfo(NodeId::Expr(expr_id), expression_type.clone());
                            return Some(expression_type);
                        }
                        self.push_error(TypeCheckError::InvalidUnaryOperand {
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
                // Parentheses group; they do not change what is expected.
                let inner_type = self.infer_expression_expecting(expr, expected, ctx);
                if let Some(ref type_info) = inner_type {
                    ctx.set_node_typeinfo(NodeId::Expr(expr_id), type_info.clone());
                }
                inner_type
            }
            Expr::Binary { left, right, op } => {
                // A recorded type answers unconditionally. Re-deriving it would
                // re-run every check below and report the subtree's diagnostics
                // a second time, and it could not change an outcome: the only
                // visit that records a type before an expected type exists is
                // the generic-argument pre-pass, and where what it recorded
                // disagrees with what is later expected it has already rejected
                // the call with `ConflictingTypeInference`.
                if let Some(type_info) = ctx.get_node_typeinfo(NodeId::Expr(expr_id)) {
                    return Some(type_info);
                }
                let (left_type, right_type) =
                    self.infer_binary_operands(left, right, &op, expected, ctx);
                // NOTE: Only detects division by literal zero (e.g., `x / 0`).
                // Constant expressions and const-declared zero values are not detected.
                if matches!(op, OperatorKind::Div | OperatorKind::Mod) {
                    let right_expr = &ctx.arena()[right].kind;
                    if let Expr::NumberLiteral { value } = right_expr
                        && value.parse::<i128>().ok() == Some(0)
                    {
                        self.push_error(TypeCheckError::DivisionByZero {
                            location: ctx.arena()[right].location,
                        });
                    }
                }
                // `**` has no code generation (no native WASM power
                // instruction and no lowering yet), so every use is rejected
                // here, before codegen can be reached. The check runs before
                // the two-`Some` operand guard below so it also fires when an
                // operand's type fails to infer (e.g. a `@` operand, which
                // infers `None` without an error of its own). Operand-shape
                // diagnostics are deliberately skipped for `**`: the operator
                // itself is the error regardless of its operands. The
                // expression falls back to the left operand's type so
                // surrounding inference can continue.
                if matches!(op, OperatorKind::Pow) {
                    self.push_error(TypeCheckError::PowOperatorNotSupported { location });
                    if let Some(res_type) = left_type {
                        ctx.set_node_typeinfo(NodeId::Expr(expr_id), res_type.clone());
                        return Some(res_type);
                    }
                    return None;
                }
                if let (Some(left_type), Some(right_type)) = (left_type, right_type) {
                    if left_type != right_type {
                        self.push_error(TypeCheckError::BinaryOperandTypeMismatch {
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
                                self.push_error(TypeCheckError::InvalidBinaryOperand {
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
                                self.push_error(TypeCheckError::InvalidBinaryOperand {
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
                        OperatorKind::Pow => {
                            unreachable!(
                                "`**` is rejected above with PowOperatorNotSupported before operand checks"
                            )
                        }
                        OperatorKind::Add
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
                                self.push_error(TypeCheckError::InvalidBinaryOperand {
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
                // A recorded type answers unconditionally, for the same reason
                // as `Binary` above: re-deriving it would report the elements'
                // diagnostics twice without changing any outcome.
                if let Some(type_info) = ctx.get_node_typeinfo(NodeId::Expr(expr_id)) {
                    return Some(type_info);
                }
                // `[T; N]` expected of the literal is `T` expected of every one
                // of its elements, and an element that is itself an array
                // literal receives the inner array type — so a nested
                // initializer types all the way down.
                let element_expected = match expected.map(|e| &e.ty.kind) {
                    Some(TypeInfoKind::Array(element_type, _)) => Some((**element_type).clone()),
                    _ => None,
                };
                let element_source = TypeMismatchContext::ArrayElement;
                let element_expected = element_expected
                    .as_ref()
                    .map(|ty| Expected::new(ty, &element_source));
                // Compound-literal-position check moved to analysis rule A015.
                if !elements.is_empty()
                    && let Some(element_type_info) =
                        self.infer_expression_expecting(elements[0], element_expected, ctx)
                {
                    for &element_id in &elements[1..] {
                        let element_type =
                            self.infer_expression_expecting(element_id, element_expected, ctx);
                        if let Some(element_type) = element_type
                            && element_type != element_type_info
                        {
                            self.push_error(TypeCheckError::ArrayElementTypeMismatch {
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
                // An expected integer type outranks any type already recorded:
                // the recorded one may be this arm's own `i32` fallback from an
                // earlier visit — the generic-argument pre-pass reaches
                // literals before any expected type exists — and the position
                // the literal appears in is the authority on which type it
                // denotes. The position is recorded with the type so a range
                // error can say where the type came from.
                if let Some(expected) = expected
                    && expected.ty.kind.is_number()
                {
                    ctx.set_node_typeinfo(NodeId::Expr(expr_id), expected.ty.clone());
                    ctx.set_literal_type_source(expr_id, expected.source.clone());
                    return Some(expected.ty.clone());
                }
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
                if let Some(var_ty) = self
                    .symbol_table
                    .lookup_variable(&name)
                    .or_else(|| self.symbol_table.lookup_constant(&name))
                {
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

    /// Resolves a `::`-separated path that names a top-level `const` in another
    /// file to its value type, returning `Some(result)` when the path resolves to
    /// a constant and `None` otherwise (so the caller falls through to
    /// enum-variant handling).
    ///
    /// Two path shapes reach a cross-file const: an absolute `lib::vals::MAX`
    /// (three segments) and a file-import-relative `vals::MAX` (two segments, when
    /// the file wrote `use lib::vals;`). Both are tried here. A two-segment path
    /// that does *not* resolve to a constant — notably `Enum::Variant` — returns
    /// `None` and is left to the variant code, so enum access is unaffected. The
    /// target's `pub`-ness is enforced against the accessing file, matching the
    /// qualified-call path's visibility gate.
    fn try_infer_qualified_const(
        &mut self,
        expr_id: ExprId,
        ctx: &mut TypedContext,
    ) -> Option<Option<TypeInfo>> {
        let segments = Self::flatten_type_member_path(ctx.arena(), expr_id)?;
        if segments.len() < 2 {
            return None;
        }
        let location = ctx.arena()[expr_id].location;
        let from_scope = self.symbol_table.current_scope_id().unwrap_or(0);
        let (symbol, def_scope_id) = self
            .symbol_table
            .resolve_qualified_name(&segments, from_scope)?;
        let crate::symbol_table::Symbol::Constant(info) = &symbol else {
            return None;
        };
        let const_type = info.type_info.clone();
        self.check_and_report_visibility(
            &info.visibility,
            def_scope_id,
            info.definition_location,
            &location,
            VisibilityContext::Constant {
                name: segments.join("::"),
            },
        );
        ctx.set_node_typeinfo(NodeId::Expr(expr_id), const_type.clone());
        Some(Some(const_type))
    }

    /// Reports a precise diagnostic for a `::`-qualified path in value position
    /// whose prefix names a known namespace but whose final segment is not a
    /// value, returning `Some(None)` when it does so and `None` to fall through.
    ///
    /// `try_infer_qualified_const` has already handled the case where the path
    /// resolves to a constant. What remains for a namespace path is either a
    /// final segment that names a non-value item (a function) or one that names
    /// nothing. Either way the enum-variant fallback would treat the namespace
    /// head as an undefined enum (`enum \`lib\` is not defined`), so this emits
    /// `cannot resolve \`lib::vals::X\`` (or names the function) instead. A path
    /// whose prefix is *not* a namespace — notably a single-qualifier
    /// `Enum::Variant` — is left untouched for the variant code.
    fn try_diagnose_qualified_path(
        &mut self,
        expr_id: ExprId,
        ctx: &mut TypedContext,
    ) -> Option<Option<TypeInfo>> {
        let segments = Self::flatten_type_member_path(ctx.arena(), expr_id)?;
        if segments.len() < 2 {
            return None;
        }
        let from_scope = self.symbol_table.current_scope_id().unwrap_or(0);
        let location = ctx.arena()[expr_id].location;
        // An absolute `dir::file::item` value path whose namespace prefix names a
        // real project file this file never imported is an encapsulation leak: the
        // namespace stops resolving (so `prefix_is_namespace` is false), and the
        // enum-variant fallback would emit the misleading "enum `lib` is not
        // defined". Diagnose it as the missing import it is, pointing at the `use`.
        if let Some(diagnosis) = self
            .symbol_table
            .unimported_namespace_prefix(&segments, from_scope)
        {
            self.report_unimported_namespace(diagnosis, &segments, location);
            return Some(None);
        }
        if !self.symbol_table.prefix_is_namespace(&segments, from_scope) {
            return None;
        }
        let path = segments.join("::");
        let names = self
            .symbol_table
            .resolve_qualified_name(&segments, from_scope)
            .and_then(|(symbol, _)| symbol.as_function().map(|_| "a function".to_string()));
        self.push_error(TypeCheckError::QualifiedPathNotAValue {
            path,
            names,
            location,
        });
        Some(None)
    }

    /// Resolves a namespace-qualified associated function call
    /// (`geo::Point::new(...)`, `lib::geo::Point::new(...)`): the leading segments
    /// name an imported file namespace, the next a `pub` struct inside that file,
    /// and the last its associated function. Returns `Some(result)` when the path
    /// has this shape (resolved or diagnosed) and `None` when it does not — a
    /// plain namespace function (`math::arith::add`) or a single-file
    /// `Type::assoc()` — so the caller falls through unchanged.
    ///
    /// The struct, its method, and visibility are resolved *as referenced from the
    /// namespace's file scope*, so cross-file `pub`-ness is enforced and the
    /// method's mangled identity is keyed by the struct's defining file. The
    /// resolved target is recorded so codegen reaches the right file-qualified
    /// method without re-walking the namespace.
    fn try_infer_namespace_qualified_assoc_call(
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
        let from_scope = self.symbol_table.current_scope_id().unwrap_or(0);
        // The trailing two segments are the struct and its associated function
        // (`type_access_len` 2), so a same-named struct at that boundary stops the
        // namespace walk.
        let (ns_scope, consumed) = self
            .symbol_table
            .resolve_longest_namespace_prefix(&segments, from_scope, 2)?;
        // After the namespace prefix exactly two segments must remain — the struct
        // and its associated function. A different shape (e.g. a deeper namespace
        // function `a::b::c::fn`) is not ours.
        if segments.len() - consumed != 2 {
            return None;
        }
        let type_name = segments[consumed].clone();
        let method_name = segments[consumed + 1].clone();

        let (struct_info, _key) = self
            .symbol_table
            .resolve_struct_in_namespace(&type_name, ns_scope, from_scope)?;
        let location = ctx.arena()[call_expr_id].location;
        let path = segments.join("::");

        self.check_and_report_visibility(
            &struct_info.visibility,
            struct_info.definition_scope_id,
            struct_info.definition_location,
            &location,
            VisibilityContext::Struct {
                name: type_name.clone(),
            },
        );

        let Some(method_info) = self.symbol_table.resolve_method_in_namespace(
            &type_name,
            &method_name,
            ns_scope,
            from_scope,
        ) else {
            // The method does not resolve on the same-named struct. When the path
            // instead descends into a sub-file the accessing file never imported —
            // `a::b::deep` where `a` defines `struct b` *and* a sibling `a/b.inf`
            // exists, but only `use a;` was written — committing a bare
            // `undefined function` here hides the real fix (the missing `use a::b;`).
            // Consult the single missing-import diagnostic first; fall to the bare
            // error only when it is not a missing-import shape.
            if let Some(diagnosis) = self
                .symbol_table
                .unimported_namespace_prefix(&segments, from_scope)
            {
                self.report_unimported_namespace(diagnosis, &segments, location);
            } else {
                self.push_error_dedup(TypeCheckError::UndefinedFunction {
                    name: path,
                    location,
                });
            }
            for arg in call_args {
                self.infer_expression(arg.1, ctx);
            }
            return Some(Some(TypeInfo::default()));
        };

        // An associated function on a `spec`-inner struct reached through a
        // namespace path (`lib::specs::Spec::Helper::make()`) is proof-only:
        // codegen assigns it no executable index, so the call would type-check and
        // then have no callee to lower. Reject it here, matching the same
        // proof-only boundary the plain qualified-call path enforces — without
        // this it reaches codegen and panics on a missing function index.
        if self.symbol_table.scope_is_within_spec(method_info.scope_id) {
            self.push_error(TypeCheckError::SpecFunctionNotCallable {
                path: path.clone(),
                function_name: method_name.clone(),
                location,
            });
            for arg in call_args {
                self.infer_expression(arg.1, ctx);
            }
            let return_type = method_info.signature.return_type.clone();
            ctx.set_node_typeinfo(NodeId::Expr(call_expr_id), return_type.clone());
            return Some(Some(return_type));
        }

        if method_info.is_instance_method() {
            self.push_error(TypeCheckError::InstanceMethodCalledAsAssociated {
                type_name: type_name.clone(),
                method_name: method_name.clone(),
                location,
            });
        }

        self.check_and_report_visibility(
            &method_info.visibility,
            method_info.scope_id,
            method_info.signature.definition_location,
            &location,
            VisibilityContext::Method {
                type_name: type_name.clone(),
                method_name: method_name.clone(),
            },
        );

        let signature = method_info.signature.clone();
        if call_args.len() != signature.param_types.len() {
            self.push_error(TypeCheckError::ArgumentCountMismatch {
                kind: "method",
                name: format!("{type_name}::{method_name}"),
                expected: signature.param_types.len(),
                found: call_args.len(),
                location,
            });
        }
        self.check_argument_labels(
            call_expr_id,
            "method",
            &format!("{type_name}::{method_name}"),
            &signature.param_names,
            call_args,
            ctx,
        );
        let sig_param_types = signature.param_types.clone();
        for (i, arg) in call_args.iter().enumerate() {
            self.propagate_arg_uzumaki_type(arg.1, sig_param_types.get(i), ctx);
            // The parameter's declared type is what a bare integer literal
            // argument denotes; the mismatch check below reports the one
            // diagnostic when the argument cannot denote it.
            let source = method_arg_context(&type_name, &method_name, i);
            let arg_type = self.infer_expression_expecting(
                arg.1,
                sig_param_types.get(i).map(|ty| Expected::new(ty, &source)),
                ctx,
            );
            if let Some(arg_type) = arg_type
                && i < sig_param_types.len()
                && arg_type != sig_param_types[i]
            {
                self.push_error(TypeCheckError::TypeMismatch {
                    expected: sig_param_types[i].clone(),
                    found: arg_type,
                    context: source,
                    location,
                });
            }
        }

        let return_type = signature.return_type.clone();
        ctx.set_node_typeinfo(
            NodeId::Expr(function_expr_id),
            TypeInfo {
                kind: TypeInfoKind::Function(format!("{type_name}::{method_name}")),
                type_params: vec![],
            },
        );
        ctx.set_node_typeinfo(NodeId::Expr(call_expr_id), return_type.clone());
        let module_path = self
            .symbol_table
            .file_module_path_of_scope(method_info.scope_id);
        ctx.set_call_target(
            function_expr_id,
            CallTarget {
                module_path,
                name: method_name,
                receiver_struct: Some(type_name),
            },
        );
        Some(Some(return_type))
    }

    /// Resolves a namespace-qualified enum variant (`geo::Color::Green`): the
    /// leading segments name an imported file namespace, the next a `pub` enum
    /// inside it, and the last a variant. Returns `Some(result)` when the path has
    /// this shape (resolved or diagnosed) and `None` otherwise — a single-file
    /// `Enum::Variant` or a non-namespace path — so the caller falls through.
    ///
    /// The enum literal's type is keyed by the enum's defining file, so a variant
    /// from one file is not assignable where a same-named enum from another file
    /// is expected (B1 identity).
    fn try_infer_namespace_qualified_enum_variant(
        &mut self,
        expr_id: ExprId,
        ctx: &mut TypedContext,
    ) -> Option<Option<TypeInfo>> {
        let segments = Self::flatten_type_member_path(ctx.arena(), expr_id)?;
        if segments.len() < 3 {
            return None;
        }
        let from_scope = self.symbol_table.current_scope_id().unwrap_or(0);
        // The trailing two segments are the enum and its variant (`type_access_len`
        // 2), so a same-named enum at that boundary stops the namespace walk.
        let (ns_scope, consumed) = self
            .symbol_table
            .resolve_longest_namespace_prefix(&segments, from_scope, 2)?;
        if segments.len() - consumed != 2 {
            return None;
        }
        let enum_name = segments[consumed].clone();
        let variant_name = segments[consumed + 1].clone();

        let (enum_info, enum_key) = self
            .symbol_table
            .resolve_enum_in_namespace(&enum_name, ns_scope, from_scope)?;
        let location = ctx.arena()[expr_id].location;

        self.check_and_report_visibility(
            &enum_info.visibility,
            enum_info.definition_scope_id,
            enum_info.definition_location,
            &location,
            VisibilityContext::Enum {
                name: enum_name.clone(),
            },
        );

        if !enum_info.variants.contains(&variant_name) {
            self.push_error(TypeCheckError::VariantNotFound {
                enum_name,
                variant_name,
                location,
            });
            return Some(None);
        }

        let enum_type = TypeInfo {
            kind: TypeInfoKind::Enum(enum_name, enum_key),
            type_params: vec![],
        };
        ctx.set_node_typeinfo(NodeId::Expr(expr_id), enum_type.clone());
        Some(Some(enum_type))
    }

    /// Resolves the struct named by a struct-literal head, which may be bare
    /// (`Point`) or namespace-qualified (`geo::Point`). Returns the bare struct
    /// name (for field diagnostics) and, when the name resolves to a struct, its
    /// [`StructInfo`] and the file-keyed [`TypeInfo`] of the literal.
    ///
    /// A namespace-qualified head reaches a struct inside an imported file; the
    /// struct must be `pub` (enforced here with a dual-location diagnostic) and is
    /// keyed by its defining file, so a value built via `geo::Point { .. }` is
    /// assignable exactly where the item-imported `Point` is, and not where a
    /// same-named struct from another file is expected (B1 identity).
    fn resolve_struct_literal_target(
        &mut self,
        qualified_name: &str,
        location: Location,
    ) -> (String, Option<(crate::symbol_table::StructInfo, TypeInfo)>) {
        let Some((prefix, type_name)) = qualified_name.rsplit_once("::") else {
            let info = self
                .symbol_table
                .lookup_struct(qualified_name)
                .zip(self.symbol_table.lookup_type(qualified_name));
            return (qualified_name.to_string(), info);
        };

        let bare = type_name.to_string();
        let from_scope = self.symbol_table.current_scope_id().unwrap_or(0);
        let segments: Vec<String> = prefix.split("::").map(str::to_string).collect();
        // The leaf type was already split off above, so `segments` is a pure
        // namespace prefix: no segment is a type-access and the whole prefix must be
        // consumed (`type_access_len` 0).
        let Some((ns_scope, consumed)) = self
            .symbol_table
            .resolve_longest_namespace_prefix(&segments, from_scope, 0)
        else {
            return (bare, None);
        };
        // The whole prefix must be the namespace; a leftover segment would mean the
        // head is not a clean `namespace::Type`.
        if consumed != segments.len() {
            return (bare, None);
        }
        let Some((struct_info, key)) = self
            .symbol_table
            .resolve_struct_in_namespace(&bare, ns_scope, from_scope)
        else {
            return (bare, None);
        };
        self.check_and_report_visibility(
            &struct_info.visibility,
            struct_info.definition_scope_id,
            struct_info.definition_location,
            &location,
            VisibilityContext::Struct { name: bare.clone() },
        );
        let ty = TypeInfo {
            kind: TypeInfoKind::Struct(bare.clone(), key),
            type_params: struct_info.type_params.clone(),
        };
        (bare, Some((struct_info, ty)))
    }

    /// Resolves a `::`-separated call target to a function in another file,
    /// returning `Some(result)` when the path names a function and `None` when it
    /// does not (so the caller falls through to method / enum / plain-call
    /// handling).
    ///
    /// Two shapes resolve here:
    /// - **Multi-qualifier** (`math::arith::add(...)`, three or more segments) is
    ///   unambiguously a file-qualified path: no method/enum/plain handler can
    ///   resolve a multi-hop path, so a failure to resolve is reported here.
    /// - **Single-qualifier** (`util::helper()`, exactly two segments) is the
    ///   basic file-import call shape, but `A::b` also spells `Enum::Variant` and
    ///   `Type::assoc_fn()`. It is taken only when the first segment is a bound
    ///   namespace import in the accessing scope (`use util;`), which a type or
    ///   enum name never is; otherwise the path falls through so the existing
    ///   method/enum code keeps its dedicated diagnostics.
    fn try_infer_qualified_function_call(
        &mut self,
        call_expr_id: ExprId,
        function_expr_id: ExprId,
        call_args: &[(Option<IdentId>, ExprId)],
        ctx: &mut TypedContext,
    ) -> Option<Option<TypeInfo>> {
        let segments = Self::flatten_type_member_path(ctx.arena(), function_expr_id)?;
        if segments.len() < 2 {
            return None;
        }

        let from_scope = self.symbol_table.current_scope_id().unwrap_or(0);
        // A two-segment path is overloaded with `Enum::Variant` and
        // `Type::assoc_fn()`; only treat it as a namespace call when the first
        // segment actually names an imported namespace. Three-or-more-segment
        // paths are unambiguous and always handled here.
        if segments.len() == 2 && !self.symbol_table.prefix_is_namespace(&segments, from_scope) {
            return None;
        }

        let location = ctx.arena()[call_expr_id].location;
        let path = segments.join("::");

        // By here the path is a committed namespace call: either three or more
        // segments (unambiguously file-qualified), or two segments whose head is
        // a bound namespace import. If it does not resolve to a callable function
        // — the name is wrong, or a hop crosses a non-re-exported (private)
        // import — this is the call's error to report, not a fall-through: no
        // method/enum/plain-call handler below can resolve a namespace path, so
        // silently falling through would accept it.
        let signature = match self
            .symbol_table
            .resolve_qualified_name(&segments, from_scope)
        {
            Some((symbol, def_scope)) if symbol.as_function().is_some() => {
                let sig = symbol.as_function().expect("checked above").clone();
                // A spec-inner function reached through a qualified path is
                // proof-only: codegen assigns it no executable index, so the call
                // would type-check and then have no callee to lower. Reject it
                // before the visibility check — `pub` does not make a spec function
                // callable (it is rejected by `spec` being the visibility unit), so
                // a "private function" diagnostic would point at the wrong fix. The
                // spec scope is the one resolution walked into, not the signature's
                // recorded definition scope, which is the spec's enclosing file.
                // Pushed raw (not via `push_error_dedup`): this fires once per call
                // site during single-pass expression inference, never from the
                // multi-pass registration walk the deduped variants guard against.
                //
                // The transitive `scope_is_within_spec` (not the direct
                // `is_spec_scope`) catches a `spec`-inner *struct*'s associated
                // function too (`Spec::Struct::assoc()`): its resolution scope is
                // the struct's own scope, nested inside the spec, so the direct
                // check would let it slip through to codegen, which has no
                // executable index for it.
                if self.symbol_table.scope_is_within_spec(def_scope) {
                    self.push_error(TypeCheckError::SpecFunctionNotCallable {
                        path: path.clone(),
                        function_name: sig.name.clone(),
                        location,
                    });
                    for arg in call_args {
                        self.infer_expression(arg.1, ctx);
                    }
                    // Type the call as the spec function's declared return type so
                    // the rejected call does not cascade a misleading
                    // "expected T, found Unit" at an enclosing `return` or `let`.
                    let return_type = sig.return_type.clone();
                    ctx.set_node_typeinfo(NodeId::Expr(call_expr_id), return_type.clone());
                    return Some(Some(return_type));
                }
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
                // The path did not resolve. When its namespace prefix names a real
                // project file the accessing file never imported, the absolute path
                // is an encapsulation leak the file-boundary discipline blocks — the
                // fix is to add the `use`, so point at that rather than reporting a
                // bare "undefined function" that hides the missing import.
                if let Some(diagnosis) = self
                    .symbol_table
                    .unimported_namespace_prefix(&segments, from_scope)
                {
                    self.report_unimported_namespace(diagnosis, &segments, location);
                } else if self
                    .symbol_table
                    .qualified_function_reachable_ignoring_reexport(&segments, from_scope)
                {
                    // The leaf function exists but is reached through an intermediate
                    // file's plain `use` (not `pub use`); say so and point at the fix.
                    self.push_error_dedup(TypeCheckError::QualifiedPathNotReexported {
                        path,
                        location,
                    });
                } else {
                    self.push_error_dedup(TypeCheckError::UndefinedFunction {
                        name: path,
                        location,
                    });
                }
                for arg in call_args {
                    self.infer_expression(arg.1, ctx);
                }
                return Some(None);
            }
        };

        if call_args.len() != signature.param_types.len() {
            self.push_error(TypeCheckError::ArgumentCountMismatch {
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
        self.check_argument_labels(
            call_expr_id,
            "function",
            &path,
            &signature.param_names,
            call_args,
            ctx,
        );

        let sig_param_types = signature.param_types.clone();
        for (i, arg) in call_args.iter().enumerate() {
            self.propagate_arg_uzumaki_type(arg.1, sig_param_types.get(i), ctx);
            // The parameter's declared type is what a bare integer literal
            // argument denotes; the mismatch check below reports the one
            // diagnostic when the argument cannot denote it.
            let source = function_arg_context(&path, i);
            let arg_type = self.infer_expression_expecting(
                arg.1,
                sig_param_types.get(i).map(|ty| Expected::new(ty, &source)),
                ctx,
            );
            if let Some(arg_type) = arg_type
                && i < sig_param_types.len()
                && arg_type != sig_param_types[i]
            {
                self.push_error(TypeCheckError::TypeMismatch {
                    expected: sig_param_types[i].clone(),
                    found: arg_type,
                    context: source,
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
        let module_path = self
            .symbol_table
            .file_module_path_of_scope(signature.definition_scope_id);
        ctx.set_call_target(
            function_expr_id,
            CallTarget {
                module_path,
                name: signature.name.clone(),
                receiver_struct: None,
            },
        );
        Some(Some(signature.return_type))
    }

    /// Reports a call that writes a label on some of its arguments and not on
    /// the rest.
    ///
    /// The labelling of the first argument sets the shape the rest must follow,
    /// and the diagnostic points at the first argument that departs from it: at
    /// the label token when that argument is labelled, at the argument
    /// expression when it is not. The two are never combined into one span — a
    /// [`Location`] covers a single token, and trivia may sit between a label
    /// and its colon.
    ///
    /// Purely syntactic, so it runs before the callee is resolved: a partly
    /// labelled call is malformed whatever the callee turns out to be, including
    /// one that never resolves. That is also why it reports beside an arity
    /// mismatch where [`Self::check_argument_labels`] stays silent — the count a
    /// call passes says nothing about whether it was written coherently, so both
    /// complaints are the author's to answer.
    fn check_argument_labelling_shape(
        &mut self,
        call_expr_id: ExprId,
        call_args: &[(Option<IdentId>, ExprId)],
        ctx: &TypedContext,
    ) {
        let Some((first_label, _)) = call_args.first() else {
            return;
        };
        let first_is_labelled = first_label.is_some();
        let Some((label, expr)) = call_args
            .iter()
            .find(|(label, _)| label.is_some() != first_is_labelled)
        else {
            return;
        };
        if !self.mixed_argument_calls_reported.insert(call_expr_id) {
            return;
        }
        let location = match label {
            Some(label) => ctx.arena()[*label].location,
            None => ctx.arena()[*expr].location,
        };
        self.push_error(TypeCheckError::MixedNamedAndPositionalArguments { location });
    }

    /// Reports each argument label that does not name the parameter it is
    /// written opposite: one the callee declares in another position is out of
    /// order, any other names no parameter at all.
    ///
    /// `param_names` is index-aligned with the arguments a call site writes (a
    /// receiver is absent from both), so the comparison is positional. That
    /// keeps it independent of the duplicate-parameter recovery path, which
    /// registers a declaration whose names repeat, and it needs no name-to-index
    /// map. The search for a matching declaration cannot return the written
    /// position itself, since a label equal to the parameter there was already
    /// accepted.
    ///
    /// Reports nothing unless every argument carries a label and the arity
    /// matches. A partly labelled call belongs to
    /// [`Self::check_argument_labelling_shape`], and an arity mismatch already
    /// has its own diagnostic — three of the five callee branches push it and
    /// still walk their arguments, so the length gate is what keeps that call to
    /// one report.
    fn check_argument_labels(
        &mut self,
        call_expr_id: ExprId,
        kind: &'static str,
        name: &str,
        param_names: &[Option<String>],
        call_args: &[(Option<IdentId>, ExprId)],
        ctx: &TypedContext,
    ) {
        if call_args.is_empty()
            || param_names.len() != call_args.len()
            || !call_args.iter().all(|(label, _)| label.is_some())
        {
            return;
        }
        if !self.labelled_argument_calls_checked.insert(call_expr_id) {
            return;
        }
        for (position, (label, _)) in call_args.iter().enumerate() {
            let Some(label_id) = *label else {
                continue;
            };
            let written = ctx.arena()[label_id].name.clone();
            if param_names[position].as_deref() == Some(written.as_str()) {
                continue;
            }
            let location = ctx.arena()[label_id].location;
            let declared = param_names
                .iter()
                .position(|declared| declared.as_deref() == Some(written.as_str()));
            let error = match declared {
                Some(declared_position) => TypeCheckError::ArgumentLabelOutOfOrder {
                    kind,
                    name: name.to_string(),
                    label: written,
                    expected_position: declared_position + 1,
                    found_position: position + 1,
                    location,
                },
                None => TypeCheckError::UnknownArgumentLabel {
                    kind,
                    name: name.to_string(),
                    label: written,
                    location,
                },
            };
            self.push_error(error);
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
        self.check_argument_labelling_shape(call_expr_id, call_args, ctx);

        // A `::`-separated path that names a function in another file
        // (`math::arith::add(...)`) resolves through the file scope tree and the
        // importing file's resolved imports. This is tried before the
        // `Type::function()` handling below, which only covers single-qualifier
        // method/enum access; a path naming a struct method or enum variant
        // returns no function symbol here and falls through unchanged.
        // A namespace-qualified associated call (`geo::Point::new(...)`,
        // `lib::geo::Point::new(...)`) reaches a struct *inside* an imported file.
        // It must be tried before the plain qualified-call path, which would treat
        // the whole path as a free function and reject it.
        if let Some(result) = self.try_infer_namespace_qualified_assoc_call(
            call_expr_id,
            function_expr_id,
            call_args,
            ctx,
        ) {
            return result;
        }

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
                    TypeNode::Qualified { qualifier: _, name } => {
                        Some(ctx.arena()[*name].name.clone())
                    }
                    _ => None,
                },
                Expr::Identifier(ident_id) => Some(ctx.arena()[*ident_id].name.clone()),
                _ => None,
            };

            if let Some(type_name) = type_name {
                let method_name = ctx.arena()[method_name_id].name.clone();

                // First check if this is an enum variant - can't call variants like functions
                let from_scope = self.symbol_table.current_scope_id().unwrap_or(0);
                if self.symbol_table.lookup_enum(&type_name).is_some() {
                    // Fall through to standard function handling
                } else if let Some(method_info) =
                    self.symbol_table
                        .resolve_method_in_scope(&type_name, &method_name, from_scope)
                {
                    if method_info.is_instance_method() {
                        cov_mark::hit!(type_checker_instance_method_called_as_associated);
                        self.push_error(TypeCheckError::InstanceMethodCalledAsAssociated {
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
                        self.push_error(TypeCheckError::ArgumentCountMismatch {
                            kind: "method",
                            name: format!("{}::{}", type_name, method_name),
                            expected: signature.param_types.len(),
                            found: arg_count,
                            location,
                        });
                    }
                    self.check_argument_labels(
                        call_expr_id,
                        "method",
                        &format!("{}::{}", type_name, method_name),
                        &signature.param_names,
                        call_args,
                        ctx,
                    );

                    let sig_param_types = signature.param_types.clone();
                    let sig_return_type = signature.return_type.clone();
                    for (i, arg) in call_args.iter().enumerate() {
                        self.propagate_arg_uzumaki_type(arg.1, sig_param_types.get(i), ctx);
                        // The parameter's declared type is what a bare integer
                        // literal argument denotes; the mismatch check below
                        // reports the one diagnostic when it cannot denote it.
                        let source = method_arg_context(&type_name, &method_name, i);
                        let arg_type = self.infer_expression_expecting(
                            arg.1,
                            sig_param_types.get(i).map(|ty| Expected::new(ty, &source)),
                            ctx,
                        );

                        if let Some(arg_type) = arg_type
                            && i < sig_param_types.len()
                            && arg_type != sig_param_types[i]
                        {
                            self.push_error(TypeCheckError::TypeMismatch {
                                expected: sig_param_types[i].clone(),
                                found: arg_type,
                                context: source,
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
                    // Record the resolved target so the call graph (recursion and
                    // stack-depth analysis) can find the cross-file edge. A bare
                    // `Type::assoc()` reaches a same-file or item-imported struct;
                    // the associated function lowers to the struct's file-qualified
                    // method, so qualify by the method's defining file (its struct's
                    // file) and carry the bare struct name as the receiver.
                    let module_path = self
                        .symbol_table
                        .file_module_path_of_scope(method_info.scope_id);
                    ctx.set_call_target(
                        function_expr_id,
                        CallTarget {
                            module_path,
                            name: method_name,
                            receiver_struct: Some(type_name),
                        },
                    );
                    return Some(sig_return_type);
                } else if !type_name.contains("::") {
                    // A bare `Type::method()` whose method did not resolve and is
                    // not an enum cannot be a free function (its callee is a
                    // `TypeMemberAccess`, which the standard handling below only
                    // processes for `Identifier` callees). Falling through would
                    // silently drop it. This is also where an entry-file type
                    // reached by bare name from a non-entry file lands now that the
                    // boundary blocks it — reporting `MethodNotFound` keeps the
                    // leak from sneaking through as an untyped call (#63).
                    //
                    // When the head instead names a file in the project that was
                    // never imported, the call is a namespace call missing its
                    // `use` — not a method on a type. Report the missing import so
                    // the fix points at the `use`, not at a nonexistent method.
                    if self
                        .symbol_table
                        .name_is_unimported_namespace(&type_name, from_scope)
                    {
                        self.push_error(TypeCheckError::UnimportedNamespaceCall {
                            namespace: type_name.clone(),
                            function: method_name.clone(),
                            location: ctx.arena()[function_expr_id].location,
                        });
                    } else {
                        self.push_error(TypeCheckError::MethodNotFound {
                            type_name: type_name.clone(),
                            method_name: method_name.clone(),
                            location: ctx.arena()[function_expr_id].location,
                        });
                    }
                    for arg in call_args {
                        self.infer_expression(arg.1, ctx);
                    }
                    return None;
                }
                // A qualified `a::b::method` that is not an enum and not a method
                // falls through to standard function handling.
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
                // The bare name is used for diagnostics; the canonical key (when the
                // receiver carries one) drives resolution so dispatch follows the
                // receiver's struct identity, not a same-named struct that happens
                // to be in scope at the call site.
                let type_name = match &receiver_type.kind {
                    TypeInfoKind::Struct(name, _) => Some(name.clone()),
                    TypeInfoKind::Custom(name) => {
                        if self.symbol_table.lookup_struct(name).is_some() {
                            Some(name.clone())
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                let canonical_key = match &receiver_type.kind {
                    TypeInfoKind::Struct(_, key) => Some(key.clone()),
                    _ => None,
                };

                if let Some(type_name) = type_name {
                    let method_name = ctx.arena()[method_name_id].name.clone();
                    let from_scope = self.symbol_table.current_scope_id().unwrap_or(0);
                    // A keyed receiver resolves by its canonical struct identity; a
                    // keyless one (a spec-inner or forward-referenced `Custom`) falls
                    // back to the bare-name, scope-relative resolution.
                    let resolved = match &canonical_key {
                        Some(key) => self
                            .symbol_table
                            .resolve_method_by_canonical_key(key, &method_name),
                        None => self.symbol_table.resolve_method_in_scope(
                            &type_name,
                            &method_name,
                            from_scope,
                        ),
                    };
                    if let Some(method_info) = resolved {
                        if !method_info.is_instance_method() {
                            cov_mark::hit!(type_checker_associated_function_called_as_method);
                            self.push_error(TypeCheckError::AssociatedFunctionCalledAsMethod {
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
                            self.push_error(TypeCheckError::ArgumentCountMismatch {
                                kind: "method",
                                name: format!("{}::{}", type_name, method_name),
                                expected: signature.param_types.len(),
                                found: arg_count,
                                location,
                            });
                        }
                        self.check_argument_labels(
                            call_expr_id,
                            "method",
                            &format!("{}::{}", type_name, method_name),
                            &signature.param_names,
                            call_args,
                            ctx,
                        );

                        let sig_param_types = signature.param_types.clone();
                        let sig_return_type = signature.return_type.clone();
                        for (i, arg) in call_args.iter().enumerate() {
                            self.propagate_arg_uzumaki_type(arg.1, sig_param_types.get(i), ctx);
                            // The parameter's declared type is what a bare
                            // integer literal argument denotes; the mismatch
                            // check below reports the one diagnostic when it
                            // cannot denote it.
                            let source = method_arg_context(&type_name, &method_name, i);
                            let arg_type = self.infer_expression_expecting(
                                arg.1,
                                sig_param_types.get(i).map(|ty| Expected::new(ty, &source)),
                                ctx,
                            );

                            if let Some(arg_type) = arg_type
                                && i < sig_param_types.len()
                                && arg_type != sig_param_types[i]
                            {
                                self.push_error(TypeCheckError::TypeMismatch {
                                    expected: sig_param_types[i].clone(),
                                    found: arg_type,
                                    context: source,
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
                        // Record the resolved target so the call graph (recursion and
                        // stack-depth analysis) sees the cross-file edge. Dispatch
                        // already followed the receiver's canonical struct identity
                        // (`resolve_method_by_canonical_key`), so qualify by the
                        // method's defining file — the struct's file, not the call
                        // site's — and carry the bare struct name as the receiver.
                        let module_path = self
                            .symbol_table
                            .file_module_path_of_scope(method_info.scope_id);
                        ctx.set_call_target(
                            function_expr_id,
                            CallTarget {
                                module_path,
                                name: method_name,
                                receiver_struct: Some(type_name),
                            },
                        );
                        return Some(sig_return_type);
                    }
                    self.push_error(TypeCheckError::MethodNotFound {
                        type_name,
                        method_name,
                        location: ctx.arena()[function_expr_id].location,
                    });
                    return None;
                }
                self.push_error(TypeCheckError::MethodCallOnNonStruct {
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
            // A bare call resolves either to a same-file function or to an item
            // import (`use lib::arith::{add};`); `definition_scope_id` is the
            // callee's defining file either way, so codegen file-qualifies the
            // WASM name correctly even when it differs from the calling file.
            // Externs carry no local body, so an extern call is resolved at
            // code generation time by looking its bare name up in the scope the
            // call is written in — the declaring file, and the `spec` block
            // within it. That bare-identifier shape is also what
            // `Compiler::param_escapes_to_extern` keys on when it decides
            // whether a compound parameter must keep its private entry copy
            // because the callee may write through the pointer. Leaving
            // externs unrecorded keeps "an extern call carries no target" a
            // contract codegen can rely on, rather than a value it would have
            // to know to ignore.
            if !s.is_extern() {
                let module_path = self
                    .symbol_table
                    .file_module_path_of_scope(s.definition_scope_id);
                ctx.set_call_target(
                    function_expr_id,
                    CallTarget {
                        module_path,
                        name: s.name.clone(),
                        receiver_struct: None,
                    },
                );
            }
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
            self.push_error(TypeCheckError::ArgumentCountMismatch {
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
        self.check_argument_labels(
            call_expr_id,
            "function",
            &func_name,
            &signature.param_names,
            call_args,
            ctx,
        );

        // Build substitution map for generic functions
        let substitutions = if !signature.type_params.is_empty() {
            if !call_type_params.is_empty() {
                if call_type_params.len() != signature.type_params.len() {
                    self.push_error(TypeCheckError::TypeParameterCountMismatch {
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
                    self.push_error(TypeCheckError::MissingTypeParameters {
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
            // The parameter's declared type — after substitution, which is the
            // form the argument is compared against — is what a bare integer
            // literal argument denotes. A parameter whose type is still generic
            // is inert: only a concrete integer type is ever consumed.
            let expected = sig_param_types
                .get(i)
                .map(|param_type| param_type.substitute(&substitutions));
            let source = function_arg_context(&func_name, i);
            // Only a parameter the signature *declares* can explain a literal's
            // type. Where substitution changed the parameter, the type was
            // inferred from the other arguments — often from this literal's own
            // `i32` default, which the type-parameter pre-pass observed before
            // any expected type existed — so naming the parameter would assert
            // a cause that is not there. The expected type is withheld with it:
            // no accepted program's types change, because the pre-pass binds a
            // type parameter from the first argument that mentions it and has
            // already rejected any call where a later argument disagrees.
            let declared_expected = expected.as_ref().filter(|substituted| {
                sig_param_types
                    .get(i)
                    .is_some_and(|declared| declared == *substituted)
            });
            let arg_type = self.infer_expression_expecting(
                arg.1,
                declared_expected.map(|ty| Expected::new(ty, &source)),
                ctx,
            );
            if let Some(arg_type) = arg_type
                && let Some(expected) = expected
                && arg_type != expected
            {
                self.push_error(TypeCheckError::TypeMismatch {
                    expected,
                    found: arg_type,
                    context: source,
                    location,
                });
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
            self.enter_file(&module_path);
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
                            self.push_error(TypeCheckError::ImportResolutionFailed {
                                path,
                                location: use_directive.location,
                            });
                        }
                    }
                }
            }
        }
        self.exit_files();
    }

    /// Binds each `external fn` to the source module named by a `use … from`
    /// clause, populating [`Self::extern_module_bindings`].
    ///
    /// A `use … from` clause is file-scoped: it names fields of a logical module
    /// and binds the *declaring file's own* top-level `external fn`s of those
    /// names. Accumulation and resolution are therefore both per file — a
    /// directive in one file can neither bind nor conflict with a declaration in
    /// another, and the diagnostics below all describe one file's own text.
    ///
    /// For every `use { fields } from module;` directive, each field is paired
    /// with `module`. The resulting bindings are validated:
    ///
    /// - A field imported from two or more distinct modules *by the same file*
    ///   is reported as [`TypeCheckError::AmbiguousExternModule`] and left
    ///   unbound. Two files may bind the same name to different modules; the
    ///   declarations are distinct and each keeps its own origin.
    /// - A field imported from a module but not declared as an `external fn` at
    ///   the importing file's top level is reported as
    ///   [`TypeCheckError::ExternImportNotDeclared`].
    /// - A field imported from exactly one module and declared as an extern is
    ///   recorded as a bound [`ExternOrigin`].
    ///
    /// An `external fn` with no binding `use` is left unbound (no error): a bare
    /// extern declaration is valid; analysis rule A024 governs whether *calling*
    /// an unlinked extern is allowed.
    ///
    /// Diagnostics are emitted in source order. The per-file map is drained in
    /// hash order, so each file's errors are sorted by source position before
    /// they are pushed, and the files themselves are visited in arena order —
    /// making the reported order the order the user reads.
    fn collect_extern_bindings(&mut self, ctx: &TypedContext) {
        let arena = ctx.arena();
        let index = ctx.extern_index();

        for sf in arena.source_files() {
            let owner_label = inference_ast::nodes::file_label(&sf.module_path);

            // field name → (distinct modules in first-seen order, first import
            // location), over this file's directives alone.
            let mut imports: FxHashMap<String, (Vec<String>, Location)> = FxHashMap::default();
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

            let mut errors: Vec<(Location, TypeCheckError)> = Vec::new();
            for (field, (modules, location)) in imports {
                let Some(decl) = index.lookup_top_level(&sf.module_path, &field) else {
                    errors.push((
                        location,
                        TypeCheckError::ExternImportNotDeclared {
                            name: field,
                            module: modules.join(", "),
                            location,
                        },
                    ));
                    continue;
                };
                if modules.len() > 1 {
                    let module_list = modules
                        .iter()
                        .map(|m| format!("`{m}`"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    errors.push((
                        location,
                        TypeCheckError::AmbiguousExternModule {
                            name: field,
                            modules: module_list,
                            location,
                        },
                    ));
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

            errors.sort_by_key(|(location, _)| (location.offset_start, location.offset_end));
            for (_, error) in errors {
                self.push_error_with_label(owner_label.clone(), error);
            }
        }
    }

    /// Rejects every top-level `external fn` whose name is also a top-level
    /// function's, anywhere in the program.
    ///
    /// The two resolve perfectly well: an `external fn` is identified by its
    /// declaration and a bare call resolves in the scope it is written in, so
    /// such a program has one callee per call site and would run correctly. The
    /// pair is rejected as a rule about the name — a local function shadowing a
    /// foreign-boundary declaration is hard to read, because a call site does
    /// not say whether the callee is compiled here or linked in. That is a
    /// property of the spelling, so the rule spans the whole program rather than
    /// one file; the same-file half additionally replaces the symbol table's
    /// refusal of the second insert, which named neither declaration.
    ///
    /// Only the two top-level kinds take part. A method is namespaced under its
    /// receiver type and a `spec`-inner declaration under its `spec`, so neither
    /// is written as the bare name the rule is about.
    ///
    /// One diagnostic per colliding `external fn`, naming the first colliding
    /// function in arena order; files are walked in that same order and each
    /// file's declarations in source order, so a program with several collisions
    /// reports them the way the user reads them.
    fn check_extern_function_name_collisions(&mut self, ctx: &TypedContext) {
        let arena = ctx.arena();
        let mut functions: FxHashMap<&str, (Location, Option<String>)> = FxHashMap::default();
        for sf in arena.source_files() {
            let label = inference_ast::nodes::file_label(&sf.module_path);
            for &def_id in &sf.defs {
                if let Def::Function { name, .. } = &arena[def_id].kind {
                    functions
                        .entry(&arena[*name].name)
                        .or_insert_with(|| (arena[def_id].location, label.clone()));
                }
            }
        }
        if functions.is_empty() {
            return;
        }

        for sf in arena.source_files() {
            let label = inference_ast::nodes::file_label(&sf.module_path);
            // The registration skip is a same-file question, so it is decided by
            // this file's own function names rather than by the program-wide map
            // above, whose entry for a name may belong to another file entirely.
            let own: FxHashSet<&str> = sf
                .defs
                .iter()
                .filter_map(|&def_id| match &arena[def_id].kind {
                    Def::Function { name, .. } => Some(arena[*name].name.as_str()),
                    _ => None,
                })
                .collect();
            for &def_id in &sf.defs {
                let Def::ExternFunction { name, .. } = &arena[def_id].kind else {
                    continue;
                };
                let name = &arena[*name].name;
                let Some((function_location, function_file)) = functions.get(name.as_str()) else {
                    continue;
                };
                if own.contains(name.as_str()) {
                    self.same_file_extern_collisions.insert(def_id);
                }
                self.push_error_with_label(
                    label.clone(),
                    TypeCheckError::ExternFunctionNameCollision {
                        name: name.clone(),
                        location: arena[def_id].location,
                        function_location: *function_location,
                        function_file: function_file.clone(),
                    },
                );
            }
        }
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
            self.push_error(TypeCheckError::EmptyImportList {
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
    ///
    /// Imports resolve to a **fixpoint** rather than a single ordered pass: an
    /// item import `use math::{add}` of a name that `math.inf` itself only
    /// exposes via `pub use lib::arith::{add}` can only resolve once math's own
    /// re-export binding exists. Scopes are visited in `FxHashMap` order, so a
    /// downstream file may be processed before the file it re-imports from; a
    /// single pass would then spuriously reject the re-import. Repeating the pass
    /// until no new binding appears makes resolution order-independent, matching
    /// the body-check path that already resolves `math::add(...)` against math's
    /// resolved re-exports. A final reporting pass surfaces diagnostics only for
    /// imports that remain unresolved at the fixpoint.
    fn resolve_imports(&mut self) {
        let scope_ids: Vec<u32> = self.symbol_table.all_scope_ids();

        // Structural collisions are independent of resolution order and reported
        // up front: an import whose bound name clashes with a local definition or
        // a builtin type. The later of two same-name imports is also excluded
        // from binding here (so the declaration-order-first import always wins the
        // bare name — never whichever happens to resolve first across the
        // fixpoint), but whether that exclusion is an *error* is decided only
        // after resolution: two imports under one name that resolve to the
        // *identical* canonical target are a benign duplicate (e.g. the same item
        // reached directly and through a `pub use` re-export), not a clash. Those
        // same-name pairs are recorded and checked once the fixpoint has bound
        // every reachable target.
        let mut duplicate_name_imports: Vec<DuplicateNameImport> = Vec::new();
        for &scope_id in &scope_ids {
            self.report_import_collisions(scope_id, &mut duplicate_name_imports);
        }

        // Silent passes bind every non-colliding import that *can* resolve,
        // ignoring failures so a re-export that resolves on a later pass is not
        // prematurely rejected. The binding count strictly increases until the
        // fixpoint, so the loop is bounded by the total number of bindings; the
        // explicit cap is a belt-and-braces termination guard.
        let max_passes = scope_ids.len().saturating_add(1).max(1);
        for _ in 0..max_passes {
            let before = self.total_resolved_imports();
            for &scope_id in &scope_ids {
                self.resolve_imports_in_scope(scope_id, false);
            }
            if self.total_resolved_imports() == before {
                break;
            }
        }

        // Now that every reachable target is bound, a same-name pair is a real
        // clash only when its two imports name different canonical targets.
        self.report_genuine_name_clashes(&duplicate_name_imports);

        // Final reporting pass: anything still unresolved is a genuine error
        // (missing file, missing/private item).
        for &scope_id in &scope_ids {
            self.resolve_imports_in_scope(scope_id, true);
        }
    }

    /// Total number of resolved import bindings across all scopes — the fixpoint
    /// progress measure for [`Self::resolve_imports`].
    #[must_use = "the count is the progress measure"]
    fn total_resolved_imports(&self) -> usize {
        self.symbol_table
            .all_scope_ids()
            .into_iter()
            .filter_map(|id| self.symbol_table.get_scope(id))
            .map(|s| s.resolved_imports.len())
            .sum()
    }

    /// Detects import-name collisions in `scope_id`, in declaration order: the
    /// first import (or local definition) claiming a name wins; a later import
    /// binding the same name is excluded from binding so the first claimant always
    /// owns the bare name.
    ///
    /// Two collision kinds are reported here directly because they are structural
    /// — independent of what the import resolves to:
    /// - an import whose bound name clashes with a *local definition*;
    /// - an import whose bound name equals a *builtin type* (`i32`, `string`, …).
    ///   Builtins live only in the entry (root) scope, so a non-entry file's
    ///   collision is invisible to `lookup_symbol_local`; checking the builtin
    ///   name set makes the rejection identical in entry and non-entry files
    ///   (otherwise a builtin-named import is rejected in the entry file but
    ///   silently shadowed by the builtin in a non-entry file).
    ///
    /// An import whose name clashes with *another import* is **not** reported
    /// here: two imports under one name may both resolve to the identical
    /// canonical target (the same item reached directly and through a `pub use`
    /// re-export), which is a benign duplicate. The pair is pushed to
    /// `duplicates` and adjudicated by [`Self::report_genuine_name_clashes`] once
    /// the fixpoint has bound every target. The later import is still excluded
    /// from binding so a genuine clash never lets the second import win the name.
    fn report_import_collisions(
        &mut self,
        scope_id: u32,
        duplicates: &mut Vec<DuplicateNameImport>,
    ) {
        let imports = {
            let Some(scope) = self.symbol_table.get_scope(scope_id) else {
                return;
            };
            scope.imports.clone()
        };
        // The first import to claim each name, kept so a later same-name import
        // can be paired with its first claimant for the canonical-target check. A
        // real local definition is detected directly via `lookup_symbol_local`,
        // so it does not need separate tracking.
        let mut first_claimant: FxHashMap<String, Import> = FxHashMap::default();

        for import in &imports {
            for local_name in Self::import_bound_names(import) {
                // A builtin type name is checked first so the message is the same
                // in the entry and non-entry files: in the entry (root) scope the
                // builtin is also a local symbol, but reporting it as "a builtin
                // type" everywhere keeps the diagnostic identical regardless of
                // which file the import sits in.
                let clashes_builtin = Self::name_is_builtin_type(&local_name);
                let clashes_local = !clashes_builtin
                    && self
                        .symbol_table
                        .get_scope(scope_id)
                        .is_some_and(|s| s.lookup_symbol_local(&local_name).is_some());
                if clashes_builtin || clashes_local {
                    let with = if clashes_builtin {
                        "a builtin type"
                    } else {
                        "a local definition"
                    };
                    self.push_error_for_scope(
                        scope_id,
                        TypeCheckError::ImportNameCollision {
                            name: local_name.clone(),
                            with: with.to_string(),
                            location: import.location,
                        },
                    );
                    self.colliding_imports.insert((scope_id, local_name));
                } else if let Some(first) = first_claimant.get(&local_name) {
                    // Defer the verdict: benign iff both resolve to the same
                    // canonical target. The later import is left out of
                    // `colliding_imports` so the first claimant can still bind the
                    // name during the fixpoint — a name-keyed `resolved_imports`
                    // already keeps the later import from overwriting that binding,
                    // and a genuine clash is rejected outright (so which target
                    // "would have" bound never reaches accepted code).
                    duplicates.push(DuplicateNameImport {
                        scope_id,
                        local_name: local_name.clone(),
                        first: first.clone(),
                        later: import.clone(),
                    });
                } else {
                    first_claimant.insert(local_name, import.clone());
                }
            }
        }
    }

    /// Whether `name` is the name of a builtin type (`i32`, `bool`, `string`, …).
    /// Builtin type names are reserved: an import may not bind one (see
    /// [`Self::report_import_collisions`]).
    #[must_use = "this is a pure check with no side effects"]
    fn name_is_builtin_type(name: &str) -> bool {
        crate::type_info::TypeInfoKind::from_builtin_str(name).is_some()
    }

    /// Reports a [`TypeCheckError::ImportNameCollision`] for every same-name
    /// import pair whose two imports resolve to *different* canonical targets.
    ///
    /// Run after the fixpoint so every reachable target (including re-exports) is
    /// bound. Two imports under one name that resolve to the *same* canonical
    /// target are a benign duplicate and report nothing — the first claimant
    /// already bound the name to that exact target. Two that resolve to
    /// *different* targets are a genuine clash.
    ///
    /// A pair where one or both sides do not resolve at all is left to the final
    /// reporting pass, which surfaces the precise `ImportedItemNotFound` /
    /// `ImportResolutionFailed` for each unresolved import. Adding a collision
    /// message on top would be misleading — there is nothing to collide with when
    /// a side names no target.
    fn report_genuine_name_clashes(&mut self, duplicates: &[DuplicateNameImport]) {
        for dup in duplicates {
            let Some(first_target) =
                self.import_target_identity(dup.scope_id, &dup.first, &dup.local_name)
            else {
                continue;
            };
            let Some(later_target) =
                self.import_target_identity(dup.scope_id, &dup.later, &dup.local_name)
            else {
                continue;
            };
            if first_target != later_target {
                self.push_error_for_scope(
                    dup.scope_id,
                    TypeCheckError::ImportNameCollision {
                        name: dup.local_name.clone(),
                        with: "another import".to_string(),
                        location: dup.later.location,
                    },
                );
            }
        }
    }

    /// The canonical target an import binds `local_name` to, used to tell a benign
    /// duplicate (two imports of the same item) from a genuine clash (two
    /// different items under one name).
    ///
    /// A namespace import (`use a::b;`) is identified by its target scope id; an
    /// item import (`use a::b::{x};`) by the resolved item's defining scope, kind,
    /// and name. The same item reached directly and through a `pub use` re-export
    /// yields the same identity because `definition_scope_id` is preserved
    /// unchanged across re-export hops.
    #[must_use = "the identity is the return value"]
    fn import_target_identity(
        &self,
        scope_id: u32,
        import: &Import,
        local_name: &str,
    ) -> Option<ImportTargetIdentity> {
        match &import.kind {
            ImportKind::Plain => {
                let target = if Self::is_root_handle(&import.path) {
                    self.symbol_table.root_scope_id()?
                } else {
                    self.symbol_table.find_module_scope(&import.path)?
                };
                Some(ImportTargetIdentity::Namespace(target))
            }
            ImportKind::Partial(items) => {
                let item = items
                    .iter()
                    .find(|it| it.alias.as_deref().unwrap_or(&it.name) == local_name)?;
                let resolved_name = if Self::is_root_handle(&import.path) {
                    vec![item.name.clone()]
                } else {
                    let mut full = import.path.clone();
                    full.push(item.name.clone());
                    full
                };
                let from_scope = if Self::is_root_handle(&import.path) {
                    self.symbol_table.root_scope_id().unwrap_or(scope_id)
                } else {
                    scope_id
                };
                let (symbol, def_scope) = self
                    .symbol_table
                    .resolve_import_path(&resolved_name, from_scope)?;
                Some(ImportTargetIdentity::Item {
                    def_scope,
                    kind: Self::symbol_kind_discriminant(&symbol),
                    name: item.name.clone(),
                })
            }
        }
    }

    /// A stable discriminant for a symbol's kind, so a struct and a like-named
    /// function in the same defining scope are never treated as the same target.
    #[must_use = "the discriminant is the return value"]
    fn symbol_kind_discriminant(symbol: &crate::symbol_table::Symbol) -> u8 {
        use crate::symbol_table::Symbol;
        match symbol {
            Symbol::TypeAlias(_) => 0,
            Symbol::Struct(_) => 1,
            Symbol::Enum(_) => 2,
            Symbol::Spec(_) => 3,
            Symbol::Function(_) => 4,
            Symbol::Constant(_) => 5,
        }
    }

    /// The local names an import binds: the last path segment for a file import,
    /// or each item's alias-or-name for an item import.
    #[must_use = "the names are the return value"]
    fn import_bound_names(import: &Import) -> Vec<String> {
        match &import.kind {
            ImportKind::Plain => import.path.last().cloned().into_iter().collect(),
            ImportKind::Partial(items) => items
                .iter()
                .map(|item| item.alias.clone().unwrap_or_else(|| item.name.clone()))
                .collect(),
        }
    }

    /// Resolve the imports registered in a single file scope.
    ///
    /// A file import (`use a::b;`) binds the namespace `b` to the scope `a::b`;
    /// an item import (`use a::b::{x};`) binds each named item, which must exist
    /// in `a::b` and be `pub`. A `pub use` marks the resulting binding as
    /// re-exported, so an importer of *this* file may traverse through it.
    ///
    /// `report` distinguishes a silent fixpoint pass (bind what resolves, stay
    /// quiet on failure) from the final reporting pass (emit diagnostics for what
    /// is still unresolved). Colliding and already-bound names are skipped on
    /// every pass so a successful binding is never re-attempted.
    fn resolve_imports_in_scope(&mut self, scope_id: u32, report: bool) {
        let imports = {
            let scope = match self.symbol_table.get_scope(scope_id) {
                Some(s) => s,
                None => return,
            };
            scope.imports.clone()
        };

        for import in imports {
            let reexported = matches!(import.visibility, Visibility::Public);
            match &import.kind {
                ImportKind::Plain => {
                    self.resolve_file_import(scope_id, &import, reexported, report)
                }
                ImportKind::Partial(items) => {
                    self.resolve_item_imports(scope_id, &import, items, reexported, report);
                }
            }
        }
    }

    /// Whether the name `local_name` in `scope_id` should be skipped during import
    /// resolution: either it already bound on an earlier fixpoint pass, or it was
    /// flagged as a collision (the first claimer wins; a later import does not
    /// bind). Keeps a converged binding from being re-attempted and a colliding
    /// import from ever producing a binding.
    #[must_use = "this is a pure check with no side effects"]
    fn import_already_resolved(&self, scope_id: u32, local_name: &str) -> bool {
        if self
            .colliding_imports
            .contains(&(scope_id, local_name.to_string()))
        {
            return true;
        }
        self.symbol_table
            .get_scope(scope_id)
            .is_some_and(|s| s.resolved_imports.contains_key(local_name))
    }

    /// Whether `path` is the reserved single-segment `root` handle — `use root;`
    /// or `use root::{x};` — which names the program entry file rather than a file
    /// on disk.
    #[must_use = "this is a pure check with no side effects"]
    fn is_root_handle(path: &[String]) -> bool {
        path.len() == 1 && path[0] == "root"
    }

    /// Resolves a file import (`use a::b;`): the last segment names a namespace
    /// scope `a::b` and is bound under that name in `scope_id`. A binding that
    /// collides with a local definition or an existing import of the same name is
    /// rejected.
    fn resolve_file_import(
        &mut self,
        scope_id: u32,
        import: &Import,
        reexported: bool,
        report: bool,
    ) {
        let Some(local_name) = import.path.last().cloned() else {
            return;
        };
        if self.import_already_resolved(scope_id, &local_name) {
            return;
        }

        // `use root;` is the reserved handle for the program entry file (Inference's
        // `@import("root")`). It is not a file on disk — the entry already lives in
        // the root scope — so it binds `root` directly to that scope as a namespace,
        // exposing the entry's `pub` items as `root::item`. Visibility is enforced
        // at each access site exactly as for any other namespace.
        let target_scope_id = if Self::is_root_handle(&import.path) {
            let Some(id) = self.symbol_table.root_scope_id() else {
                return;
            };
            id
        } else {
            let Some(id) = self.symbol_table.find_module_scope(&import.path) else {
                if report {
                    self.report_unresolvable_file_import(scope_id, import);
                }
                return;
            };
            id
        };

        let resolved = ResolvedImport {
            local_name,
            target: ResolvedImportTarget::Namespace {
                scope_id: target_scope_id,
            },
            reexported,
        };
        if let Some(scope) = self.symbol_table.get_scope_mut(scope_id) {
            scope.add_resolved_import(resolved);
        }
    }

    /// Reports an unresolvable file import, stamped with the importing file's
    /// label so a non-entry file's broken `use` reads `lib::a:line:col` like its
    /// other diagnostics. Without a project context (the string-parse and REPL
    /// paths) the only file is the entry, so a path-form `use` can never name an
    /// existing namespace; that case gets a dedicated, actionable message rather
    /// than the generic resolution failure.
    fn report_unresolvable_file_import(&mut self, scope_id: u32, import: &Import) {
        let path = import.path.join("::");
        if !self.symbol_table.has_file_namespaces() {
            self.push_error_for_scope(
                scope_id,
                TypeCheckError::FileImportWithoutProjectContext {
                    path,
                    location: import.location,
                },
            );
        } else {
            self.push_error_for_scope(
                scope_id,
                TypeCheckError::ImportResolutionFailed {
                    path,
                    location: import.location,
                },
            );
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
        report: bool,
    ) {
        let file_path = import.path.join("::");
        // `use root::{x};` imports a `pub` item from the entry file (Inference's
        // `@import("root")`). The entry lives in the root scope, so each item is
        // resolved by its bare name directly against root rather than as a path
        // through a non-existent `root` file namespace.
        let is_root_handle = Self::is_root_handle(&import.path);
        for item in items {
            let local_name = item.alias.clone().unwrap_or_else(|| item.name.clone());
            if self.import_already_resolved(scope_id, &local_name) {
                continue;
            }

            let resolved_name = if is_root_handle {
                vec![item.name.clone()]
            } else {
                let mut full_path = import.path.clone();
                full_path.push(item.name.clone());
                full_path
            };
            let from_scope = if is_root_handle {
                self.symbol_table.root_scope_id().unwrap_or(scope_id)
            } else {
                scope_id
            };

            let Some((symbol, def_scope_id)) = self
                .symbol_table
                .resolve_import_path(&resolved_name, from_scope)
            else {
                // Unresolved on this pass: a re-exported item (the source file's
                // own `pub use`) may bind on a later fixpoint pass, so only the
                // final reporting pass treats a still-missing item as an error.
                // Deduped by `(item, file)`: a cyclic or transitive unresolvable
                // re-export is hit from several import sites that all name the same
                // target file, so the identical "not found" message is recorded
                // once. Stamped with the importing file's label so it reads like
                // every other diagnostic in that file.
                if report {
                    // A direct (non-`pub use`) import is deduped per importing
                    // file, so two files each missing the same item both report; a
                    // re-export collapses across the chain naming one target.
                    self.push_import_error_for_scope(
                        scope_id,
                        !reexported,
                        TypeCheckError::ImportedItemNotFound {
                            item: item.name.clone(),
                            file: file_path.clone(),
                            location: import.location,
                        },
                    );
                }
                continue;
            };

            if !symbol.is_public() {
                if report {
                    self.push_import_error_for_scope(
                        scope_id,
                        !reexported,
                        TypeCheckError::ImportedItemPrivate {
                            item: item.name.clone(),
                            file: file_path.clone(),
                            location: import.location,
                            definition_location: Self::symbol_definition_location(&symbol),
                        },
                    );
                }
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
            if let Some(scope) = self.symbol_table.get_scope_mut(scope_id) {
                scope.add_resolved_import(resolved);
            }
        }
    }

    /// Source location of an imported symbol's declaration, for the definition
    /// note on a private-import diagnostic.
    fn symbol_definition_location(symbol: &crate::symbol_table::Symbol) -> Location {
        symbol.definition_location()
    }

    /// Whether an item is accessible from the current scope (#63).
    ///
    /// An item is accessible iff (a) the access happens within the item's
    /// defining file — including that file's spec scopes and nested blocks — or
    /// (b) the item is `pub`. `pub` items reached across a file boundary travel
    /// through the import machinery, which already gates each hop on
    /// `pub`/re-export, so a single `Public` check suffices here.
    ///
    /// The same-file test is `same_file`, not scope-descendant: every scope
    /// descends from root, so for an entry-file (root) item the descendant test
    /// would wrongly count a non-entry file as same-file, letting a private entry
    /// item leak through `root::secret()`.
    fn check_visibility(
        &self,
        visibility: &Visibility,
        definition_scope: u32,
        access_scope: u32,
    ) -> bool {
        match visibility {
            Visibility::Public => true,
            Visibility::Private => self.symbol_table.same_file(access_scope, definition_scope),
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
            self.push_error(TypeCheckError::PrivateAccessViolation {
                context,
                location: *location,
                definition_location,
                definition_file,
            });
            false
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
                            self.push_error(TypeCheckError::ConflictingTypeInference {
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
                self.push_error(TypeCheckError::CannotInferTypeParameter {
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
            Expr::ArrayIndexAccess { array, .. } => self.extract_root_variable_name(arena, *array),
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
        if let Some((kind, name)) = error.dedup_key() {
            // The current file is part of the dedup key so a per-call-site
            // diagnostic (an undefined function, struct, variable, …) is recorded
            // once per file rather than once per program: the same undefined name
            // used in two imported files must surface in both. Cross-file passes
            // that report at the root carry a `None` label uniformly, so a key
            // that already file-qualifies itself (the cyclic-reexport
            // `ImportedItemNotFound`, keyed by `item@file`) keeps its intended
            // single-message-per-target dedup.
            let key = (self.current_file_label.clone(), kind, name);
            if !self.reported_errors.insert(key) {
                cov_mark::hit!(type_checker_error_dedup_skips_duplicate);
                return;
            }
            cov_mark::hit!(type_checker_error_dedup_first_occurrence);
        }
        self.push_error(error);
    }

    /// Reports the missing-import diagnosis for a `::`-qualified path
    /// [`SymbolTable::unimported_namespace_prefix`] returned, choosing the precise
    /// variant for each case: a confident missing-import names the exact `use`,
    /// while a hedged one (the target file is not in the closure) offers the
    /// namespace portion as a best-guess. The three value/type sites that gate an
    /// absolute path through the missing-import diagnostic share this so the
    /// wording stays consistent.
    fn report_unimported_namespace(
        &mut self,
        diagnosis: UnimportedNamespace,
        path: &[String],
        location: Location,
    ) {
        match diagnosis {
            UnimportedNamespace::Confident { namespace, item } => {
                self.push_error_dedup(TypeCheckError::UnimportedAbsoluteNamespacePath {
                    namespace,
                    item,
                    location,
                });
            }
            UnimportedNamespace::Hedged { namespace } => {
                self.push_error_dedup(TypeCheckError::UnresolvedNamespacePath {
                    path: path.join("::"),
                    namespace,
                    location,
                });
            }
        }
    }

    /// Records an error, stamping it with the file currently being checked so the
    /// aggregated report names the file the error belongs to. This is the single
    /// chokepoint where the current file is attached; every error push routes
    /// through here.
    fn push_error(&mut self, error: TypeCheckError) {
        self.errors.push((self.current_file_label.clone(), error));
    }

    /// Records an error stamped with the file that *owns* `scope_id` rather than
    /// the file currently open. Import-collision reporting runs at the root (the
    /// current label is `None`), yet a collision belongs to the importing file —
    /// without this it would render a bare `line:col` while ordinary errors in the
    /// same non-entry file carry the file prefix. The entry file's collisions stay
    /// bare, matching every other entry diagnostic.
    fn push_error_for_scope(&mut self, scope_id: u32, error: TypeCheckError) {
        let module_path = self.symbol_table.file_module_path_of_scope(scope_id);
        let label = inference_ast::nodes::file_label(&module_path);
        self.errors.push((label, error));
    }

    /// Records an error stamped with an explicit file label rather than the file
    /// currently open. Extern-binding collection runs at the root (the current
    /// label is `None`) while iterating the `use … from` directives of every
    /// file in the closure, so a dangling or ambiguous import belonging to an
    /// imported file must carry that file's label — computed from the owning
    /// file's module path at scan time — or its per-file-local location is
    /// misattributed to the entry document. The entry file's own directives pass
    /// `None`, matching every other entry diagnostic.
    fn push_error_with_label(&mut self, label: Option<String>, error: TypeCheckError) {
        self.errors.push((label, error));
    }

    /// Records an import-resolution diagnostic stamped with the *importing* file's
    /// label (so an unresolvable re-export reads `lib::a:line:col`, not a bare
    /// `line:col`).
    ///
    /// `dedup_by_importer` selects the dedup scope, which differs by import kind:
    ///
    /// - A **direct** item import (`use a::b::{x};`, `dedup_by_importer = true`)
    ///   keys on the importing file, so two different files that each fail to find
    ///   the same item in the same target both report — each importer's mistake is
    ///   its own. A single importer naming the same item twice still collapses,
    ///   since both share the importer's label.
    /// - A **re-export** (`pub use`, `dedup_by_importer = false`) keys on
    ///   `item@target_file` alone, so a cyclic or transitive unresolvable
    ///   re-export reached from several sites that all name the same target
    ///   collapses to one message. A direct importer of the same target shares the
    ///   importer-independent key, so the original import and its failed re-export
    ///   chain do not both report.
    ///
    /// The import passes run with `current_file_label` unset, so the label always
    /// comes from `scope_id`, never the cursor.
    fn push_import_error_for_scope(
        &mut self,
        scope_id: u32,
        dedup_by_importer: bool,
        error: TypeCheckError,
    ) {
        if let Some((kind, name)) = error.dedup_key() {
            let importer_label = if dedup_by_importer {
                let module_path = self.symbol_table.file_module_path_of_scope(scope_id);
                inference_ast::nodes::file_label(&module_path)
            } else {
                None
            };
            let key = (importer_label, kind, name);
            if !self.reported_errors.insert(key) {
                return;
            }
        }
        self.push_error_for_scope(scope_id, error);
    }

    /// Enters the symbol-table scope of the file named by `module_path` and marks
    /// it as the current file for diagnostics, so every error pushed while it is
    /// open names that file. Returns the entered scope id. Pair with
    /// [`Self::exit_files`].
    fn enter_file(&mut self, module_path: &[String]) -> u32 {
        self.current_file_label = inference_ast::nodes::file_label(module_path);
        self.symbol_table.enter_file_scope(module_path)
    }

    /// Returns the symbol table to the root scope and clears the current-file
    /// marker, so errors from a subsequent cross-file pass are not attributed to
    /// the last file walked.
    fn exit_files(&mut self) {
        self.current_file_label = None;
        self.symbol_table.reset_to_root();
    }
}

/// Literal-closure is the syntactic question "can a type expected of this
/// expression reach every leaf of it", so it is decided on the AST alone and
/// tested on hand-built expression nodes rather than through a parse.
#[cfg(test)]
mod literal_closure_tests {
    use super::TypeChecker;
    use inference_ast::arena::AstArena;
    use inference_ast::ids::ExprId;
    use inference_ast::nodes::{Expr, ExprData, Location, OperatorKind, UnaryOperatorKind};

    fn push(arena: &mut AstArena, kind: Expr) -> ExprId {
        arena.exprs.alloc(ExprData {
            location: Location::default(),
            kind,
        })
    }

    fn number(arena: &mut AstArena) -> ExprId {
        push(
            arena,
            Expr::NumberLiteral {
                value: "1".to_string(),
            },
        )
    }

    fn boolean(arena: &mut AstArena) -> ExprId {
        push(arena, Expr::BoolLiteral { value: true })
    }

    fn identifier(arena: &mut AstArena) -> ExprId {
        let ident = arena.idents.alloc(inference_ast::nodes::Ident {
            location: Location::default(),
            name: "a".to_string(),
        });
        push(arena, Expr::Identifier(ident))
    }

    fn binary(arena: &mut AstArena, left: ExprId, op: OperatorKind, right: ExprId) -> ExprId {
        push(arena, Expr::Binary { left, right, op })
    }

    fn unary(arena: &mut AstArena, op: UnaryOperatorKind, expr: ExprId) -> ExprId {
        push(arena, Expr::PrefixUnary { expr, op })
    }

    #[test]
    fn a_bare_literal_is_closed() {
        let mut arena = AstArena::default();
        let literal = number(&mut arena);
        assert!(TypeChecker::is_literal_closed(&arena, literal));
    }

    #[test]
    fn an_identifier_is_not_closed() {
        let mut arena = AstArena::default();
        let name = identifier(&mut arena);
        assert!(!TypeChecker::is_literal_closed(&arena, name));
    }

    #[test]
    fn a_non_numeric_literal_is_not_closed() {
        let mut arena = AstArena::default();
        let literal = boolean(&mut arena);
        assert!(!TypeChecker::is_literal_closed(&arena, literal));
    }

    #[test]
    fn parentheses_preserve_closure() {
        let mut arena = AstArena::default();
        let literal = number(&mut arena);
        let parenthesized = push(&mut arena, Expr::Parenthesized { expr: literal });
        assert!(TypeChecker::is_literal_closed(&arena, parenthesized));

        let name = identifier(&mut arena);
        let around_name = push(&mut arena, Expr::Parenthesized { expr: name });
        assert!(!TypeChecker::is_literal_closed(&arena, around_name));
    }

    #[test]
    fn negation_and_complement_preserve_closure_but_logical_not_does_not() {
        let mut arena = AstArena::default();
        let literal = number(&mut arena);
        let negated = unary(&mut arena, UnaryOperatorKind::Neg, literal);
        let complemented = unary(&mut arena, UnaryOperatorKind::BitNot, literal);
        let notted = unary(&mut arena, UnaryOperatorKind::Not, literal);
        assert!(TypeChecker::is_literal_closed(&arena, negated));
        assert!(TypeChecker::is_literal_closed(&arena, complemented));
        assert!(!TypeChecker::is_literal_closed(&arena, notted));
    }

    #[test]
    fn every_arithmetic_bitwise_and_shift_operator_preserves_closure() {
        let mut arena = AstArena::default();
        let left = number(&mut arena);
        let right = number(&mut arena);
        for op in [
            OperatorKind::Add,
            OperatorKind::Sub,
            OperatorKind::Mul,
            OperatorKind::Div,
            OperatorKind::Mod,
            OperatorKind::BitAnd,
            OperatorKind::BitOr,
            OperatorKind::BitXor,
            OperatorKind::Shl,
            OperatorKind::Shr,
        ] {
            let expr = binary(&mut arena, left, op.clone(), right);
            assert!(
                TypeChecker::is_literal_closed(&arena, expr),
                "`{op:?}` should preserve literal closure"
            );
        }
    }

    #[test]
    fn comparison_equality_logical_and_pow_do_not_preserve_closure() {
        let mut arena = AstArena::default();
        let left = number(&mut arena);
        let right = number(&mut arena);
        for op in [
            OperatorKind::Eq,
            OperatorKind::Ne,
            OperatorKind::Lt,
            OperatorKind::Le,
            OperatorKind::Gt,
            OperatorKind::Ge,
            OperatorKind::And,
            OperatorKind::Or,
            OperatorKind::Pow,
        ] {
            let expr = binary(&mut arena, left, op.clone(), right);
            assert!(
                !TypeChecker::is_literal_closed(&arena, expr),
                "`{op:?}` should not preserve literal closure"
            );
        }
    }

    #[test]
    fn a_binary_operator_needs_both_operands_closed() {
        let mut arena = AstArena::default();
        let literal = number(&mut arena);
        let name = identifier(&mut arena);
        let literal_left = binary(&mut arena, literal, OperatorKind::Add, name);
        let literal_right = binary(&mut arena, name, OperatorKind::Add, literal);
        assert!(!TypeChecker::is_literal_closed(&arena, literal_left));
        assert!(!TypeChecker::is_literal_closed(&arena, literal_right));
    }

    #[test]
    fn closure_is_recursive_through_nested_forms() {
        let mut arena = AstArena::default();
        let one = number(&mut arena);
        let two = number(&mut arena);
        let sum = binary(&mut arena, one, OperatorKind::Add, two);
        let grouped = push(&mut arena, Expr::Parenthesized { expr: sum });
        let negated = unary(&mut arena, UnaryOperatorKind::Neg, grouped);
        let three = number(&mut arena);
        let shifted = binary(&mut arena, negated, OperatorKind::Shl, three);
        assert!(TypeChecker::is_literal_closed(&arena, shifted));

        // One non-closed leaf anywhere breaks closure for the whole tree.
        let name = identifier(&mut arena);
        let tainted = binary(&mut arena, name, OperatorKind::Mul, three);
        let grouped_taint = push(&mut arena, Expr::Parenthesized { expr: tainted });
        let outer = binary(&mut arena, negated, OperatorKind::Add, grouped_taint);
        assert!(!TypeChecker::is_literal_closed(&arena, outer));
    }

    #[test]
    fn a_call_or_index_is_never_closed() {
        let mut arena = AstArena::default();
        let literal = number(&mut arena);
        let name = identifier(&mut arena);
        let indexed = push(
            &mut arena,
            Expr::ArrayIndexAccess {
                array: name,
                index: literal,
            },
        );
        assert!(!TypeChecker::is_literal_closed(&arena, indexed));
        let elements = push(
            &mut arena,
            Expr::ArrayLiteral {
                elements: vec![literal],
            },
        );
        assert!(!TypeChecker::is_literal_closed(&arena, elements));
    }
}
