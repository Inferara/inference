//! WebAssembly code generation for the Inference compiler.
//!
//! This crate provides WebAssembly binary generation from Inference's typed AST
//! using `wasm-encoder`. The generated WASM binary is returned as a [`CodegenOutput`]
//! struct, which carries the binary bytes along with compilation metadata.
//!
//! # Architecture
//!
//! ```text
//! Typed AST (TypedContext)
//!         |
//!         v
//!   codegen(tc, target, mode, opt_level)
//!         |
//!         v
//!   CodegenOutput { wasm, target, mode, opt_level, module_name, has_main }
//! ```
//!
//! # Non-Deterministic Extensions
//!
//! The compiler supports Inference's non-deterministic constructs through custom
//! WebAssembly instructions in the 0xfc prefix space:
//!
//! - `uzumaki()` - Non-deterministic value generation
//! - `forall { }` - Universal quantification blocks
//! - `exists { }` - Existential quantification blocks
//! - `assume { }` - Precondition assumption blocks
//! - `unique { }` - Uniqueness constraint blocks
//!
//! These extensions enable formal verification by preserving non-deterministic semantics
//! through the compilation pipeline.
//!
//! # Compilation Modes
//!
//! - **`Compile`** mode: Produces production binaries. Spec nodes are stripped.
//! - **`Proof`** mode: Produces WASM for Rocq formalization. All code is emitted,
//!   including spec functions with non-deterministic instructions. Always uses
//!   `Wasm32` target (Decision #32).
//!
//! # Module Organization
//!
//! - [`compiler`] - WASM binary generation via wasm-encoder (private)
//! - [`memory`] - Linear memory infrastructure for stack-allocated compound types (private)
//! - [`output`] - `CodegenOutput` struct definition
//! - [`target`] - `Target`, `CompilationMode`, and `OptLevel` enums

#![warn(clippy::pedantic)]

use inference_ast::arena::AstArena;
use inference_ast::ids::DefId;
use inference_ast::nodes::Def;
use inference_type_checker::typed_context::TypedContext;

use crate::compiler::{Compiler, FunctionOrigin};
use crate::errors::CodegenError;

mod compiler;
mod errors;
mod memory;
pub mod output;
mod spec_section;
pub mod target;

pub use output::CodegenOutput;
pub use target::{CompilationMode, OptLevel, Target};

/// Single source of truth for the custom WASM section name that carries
/// per-spec function indices. Consumed by both `core/wasm-codegen` (encoder)
/// and `core/wasm-to-v` (decoder) so the wire-format constant lives in one
/// place.
pub use crate::spec_section::SECTION_NAME as SPEC_FUNCS_SECTION_NAME;

/// Wire-format version of the `inference.spec_funcs` custom section payload.
/// Decoders must reject payloads whose leading varuint32 does not equal this
/// constant; bumping the value is a breaking change to the section format.
pub use crate::spec_section::SECTION_VERSION as SPEC_FUNCS_SECTION_VERSION;

/// Generates WebAssembly binary from a typed AST for the specified target and compilation mode.
///
/// `module_name` is written into the WASM module-name subsection and flows
/// downstream to the Rocq translator, which uses it as the top-level module
/// identifier. The CLI derives this from the input file stem; library
/// callers can pass any [`validate_rocq_identifier`]-compatible name.
///
/// [`validate_rocq_identifier`]: inference_wasm_to_v_translator::validate_rocq_identifier
///
/// # Errors
///
/// Returns an error if:
/// - More than one source file is present (multi-file not yet implemented)
/// - Validation fails (proof + non-Wasm32, or Soroban + non-det)
/// - Code generation fails
pub fn codegen(
    typed_context: &TypedContext,
    target: Target,
    mode: CompilationMode,
    opt_level: OptLevel,
    module_name: &str,
) -> anyhow::Result<CodegenOutput> {
    if mode == CompilationMode::Proof && !target.supports_proof_mode() {
        cov_mark::hit!(wasm_codegen_proof_mode_rejected_non_wasm32);
        return Err(anyhow::anyhow!(
            "Proof mode requires Wasm32 target. Proof mode emits custom 0xfc \
             non-deterministic instructions that only the Wasm32 target supports; \
             the {target:?} target cannot process these."
        ));
    }

    let arena = typed_context.arena();

    if target == Target::Soroban {
        for source_file in typed_context.source_files() {
            for &def_id in &source_file.defs {
                if arena.def_is_non_det(def_id) {
                    cov_mark::hit!(wasm_codegen_soroban_rejects_nondet_function);
                    let fn_name = arena.def_name(def_id);
                    return Err(anyhow::anyhow!(
                        "Soroban target does not support non-deterministic operations. \
                         Function '{fn_name}' contains non-deterministic constructs (uzumaki, \
                         forall, exists, assume, or unique blocks) that produce custom \
                         0xfc WebAssembly instructions incompatible with the Soroban VM.",
                    ));
                }
            }
        }
    }

    let mut compiler = Compiler::new(module_name);

    // Runtime array bounds checks are emitted for every Compile-mode build
    // (Debug and Release, Wasm32 and Soroban): the executed/deployed artifact is
    // always checked so a dynamic out-of-range access traps cleanly instead of
    // corrupting adjacent frame slots. `OptLevel` no longer influences this.
    // Proof mode is left unguarded pending the proof-obligation path (#212),
    // which discharges dynamic bounds as Rocq obligations rather than runtime
    // traps; the `emit_index_offset` choke point is the seam where it hooks in.
    compiler.set_emit_bounds_checks(mode == CompilationMode::Compile);

    if typed_context.source_files().len() > 1 {
        todo!("Multi-file support not yet implemented");
    }

    if typed_context.source_files().len() > 0 {
        traverse_t_ast_with_compiler(typed_context, &mut compiler, mode)?;
    }

    // Reject any spec name that would overflow the byte cap both
    // `inference.spec_funcs` decoders enforce, before the section is emitted.
    // Surfacing it here yields a clean codegen diagnostic instead of an
    // artifact that fails its own downstream link/translate step.
    if let Err(too_long) = spec_section::check_spec_name_lengths(compiler.spec_func_indices()) {
        cov_mark::hit!(wasm_codegen_spec_name_too_long);
        return Err(CodegenError::SpecNameTooLong {
            name: too_long.name,
            len: too_long.len,
            max: spec_section::MAX_SPEC_NAME_LEN,
        }
        .into());
    }

    // Snapshot `has_main` before `finish_and_take` consumes the compiler:
    // the section is emitted in a single pass that moves out the recorded
    // spec map alongside the WASM bytes.
    let has_main = compiler.has_main();
    let (wasm, spec_func_indices_by_spec, frame_sizes) = compiler.finish_and_take();
    debug_assert!(
        mode != CompilationMode::Compile || spec_func_indices_by_spec.is_empty(),
        "compile mode must not record any spec function indices"
    );

    Ok(CodegenOutput::new(
        wasm,
        target,
        mode,
        opt_level,
        module_name.to_string(),
        has_main,
        spec_func_indices_by_spec,
    )
    .with_frame_sizes(frame_sizes))
}

/// Traverses the typed AST and compiles all function and method definitions.
///
/// The traversal proceeds in two stages to ensure all WASM function indices are
/// known before any body is compiled (required for forward references):
///
/// 1. **Index registration** -- `build_func_name_to_idx` registers top-level
///    functions, then `build_method_name_to_idx` registers struct methods with
///    mangled names (`TypeName.method_name`).
/// 2. **Body compilation** -- top-level functions are compiled first, then
///    method bodies are compiled with `method_struct_name` passed so that
///    `self` parameter handling (Phase 3+) knows which struct type is in scope.
fn traverse_t_ast_with_compiler(
    typed_context: &TypedContext,
    compiler: &mut Compiler,
    mode: CompilationMode,
) -> Result<(), CodegenError> {
    let arena = typed_context.arena();
    for source_file in typed_context.source_files() {
        let buckets = collect_emittable_functions(arena, &source_file.defs, mode)?;

        // Register every visited spec (even with zero emittable inner defs) so
        // user-authored `spec MySpec { }` still surfaces a per-spec entry that
        // the Rocq translator turns into `Definition output__MySpec_specs` and
        // `Theorem valid_output__MySpec`.
        for spec_name in &buckets.visited_spec_names {
            compiler.ensure_spec_registered(spec_name);
        }

        register_function_indices(arena, compiler, typed_context, &buckets)?;

        // Stage 2: Compile bodies in the same order as registration.
        for &def_id in &buckets.funcs {
            compiler.visit_function_definition(
                def_id,
                arena,
                typed_context,
                None,
                &FunctionOrigin::TopLevel,
            )?;
        }
        for (struct_name, method_def_id) in &buckets.methods {
            compiler.visit_function_definition(
                *method_def_id,
                arena,
                typed_context,
                Some(struct_name),
                &FunctionOrigin::TopLevel,
            )?;
        }
        for (spec_name, def_id) in &buckets.spec_funcs {
            compiler.visit_function_definition(
                *def_id,
                arena,
                typed_context,
                None,
                &FunctionOrigin::SpecInner(spec_name.clone()),
            )?;
        }
        for (spec_name, struct_name, method_def_id) in &buckets.spec_methods {
            compiler.visit_function_definition(
                *method_def_id,
                arena,
                typed_context,
                Some(struct_name),
                &FunctionOrigin::SpecInner(spec_name.clone()),
            )?;
        }
    }
    Ok(())
}

/// Stage 1: register every WASM function index up front so forward references
/// resolve correctly during body compilation. Index order:
///   imports (base 0) → regular fns → regular methods → spec fns → spec methods.
///
/// Imported `external fn`s occupy the lowest WASM function indices, so every
/// local function is shifted by the import count. `set_local_func_base` seeds the
/// body-compilation index counter past the imports to keep it in lockstep with
/// the `func_name_to_idx` entries.
fn register_function_indices(
    arena: &AstArena,
    compiler: &mut Compiler,
    typed_context: &TypedContext,
    buckets: &EmittableFunctions,
) -> Result<(), CodegenError> {
    let import_count = compiler.register_imports(arena, &buckets.imports, typed_context)?;
    compiler.set_local_func_base(import_count);

    let toplevel_count =
        u32::try_from(buckets.funcs.len()).expect("more than u32::MAX top-level functions");
    let method_count =
        u32::try_from(buckets.methods.len()).expect("more than u32::MAX top-level methods");

    compiler.build_func_name_to_idx(arena, &buckets.funcs, typed_context, import_count)?;
    let method_base_idx = compiler.func_idx_after_toplevel(toplevel_count);
    compiler.build_method_name_to_idx(
        arena,
        &buckets.methods,
        typed_context,
        method_base_idx,
    )?;

    let spec_func_base = import_count + toplevel_count + method_count;
    let spec_func_indices = compiler.build_func_name_to_idx_with_spec_names(
        arena,
        &buckets.spec_funcs,
        typed_context,
        spec_func_base,
    )?;
    assert_eq!(
        buckets.spec_funcs.len(),
        spec_func_indices.len(),
        "spec-funcs zip length mismatch: bucket has {} entries, registration returned {} indices",
        buckets.spec_funcs.len(),
        spec_func_indices.len(),
    );
    for ((spec_name, _), assigned_idx) in
        buckets.spec_funcs.iter().zip(spec_func_indices.iter())
    {
        compiler.record_spec_index(spec_name, *assigned_idx);
    }

    let spec_func_indices_len = u32::try_from(spec_func_indices.len())
        .expect("more than u32::MAX spec-inner functions");
    let spec_method_base = spec_func_base + spec_func_indices_len;
    let spec_method_indices = compiler.build_method_name_to_idx_with_spec_names(
        arena,
        &buckets.spec_methods,
        typed_context,
        spec_method_base,
    )?;
    assert_eq!(
        buckets.spec_methods.len(),
        spec_method_indices.len(),
        "spec-methods zip length mismatch: bucket has {} entries, registration returned {} indices",
        buckets.spec_methods.len(),
        spec_method_indices.len(),
    );
    for ((spec_name, _, _), assigned_idx) in
        buckets.spec_methods.iter().zip(spec_method_indices.iter())
    {
        compiler.record_spec_index(spec_name, *assigned_idx);
    }

    // Verify Stage 1 produced the expected number of index entries.
    // Catches index calculation bugs before they manifest as wrong `call` targets.
    debug_assert_eq!(
        compiler.registered_function_count(),
        buckets.funcs.len()
            + buckets.methods.len()
            + buckets.spec_funcs.len()
            + buckets.spec_methods.len(),
        "func_name_to_idx entry count after Stage 1 registration does not match \
         expected count (top-level functions: {}, methods: {}, spec functions: {}, \
         spec methods: {})",
        buckets.funcs.len(),
        buckets.methods.len(),
        buckets.spec_funcs.len(),
        buckets.spec_methods.len(),
    );
    Ok(())
}

struct EmittableFunctions {
    /// Top-level `external fn` declarations, emitted as WASM function imports
    /// at indices `0..N` ahead of every local function (see
    /// [`Compiler::register_imports`]).
    imports: Vec<DefId>,
    funcs: Vec<DefId>,
    methods: Vec<(String, DefId)>,
    /// Each entry: `(spec_name, def_id)`.
    spec_funcs: Vec<(String, DefId)>,
    /// Each entry: `(spec_name, struct_name, method_def_id)`.
    spec_methods: Vec<(String, String, DefId)>,
    /// Every spec block visited in proof mode, even if it contributes no
    /// `spec_funcs` / `spec_methods` entries. Drives `ensure_spec_registered`
    /// so an empty user `spec MySpec { }` still surfaces a per-spec
    /// `Definition` and `Theorem` in the Rocq output.
    visited_spec_names: Vec<String>,
}

/// Sorts top-level defs into the five buckets used by Stage 1 registration.
///
/// Top-level `Def::ExternFunction` declarations land in the `imports` bucket and
/// are emitted as WASM function imports at indices `0..N` ahead of every local
/// function. Spec-inner externs are still skipped — when they are wired through,
/// they will need to either join `spec_funcs` or be surfaced in a sibling
/// `<mod>_spec_imports` list in the Rocq output.
///
/// In `compile` mode the spec buckets stay empty (specs are stripped). In `proof`
/// mode, top-level `Def::Spec.defs` is recursed one level deep to surface inner
/// functions and inner struct methods. Nested specs and module-nested specs are
/// out of scope until those constructs are wired through codegen.
fn collect_emittable_functions(
    arena: &AstArena,
    defs: &[DefId],
    mode: CompilationMode,
) -> Result<EmittableFunctions, CodegenError> {
    let mut buckets = EmittableFunctions {
        imports: Vec::new(),
        funcs: Vec::new(),
        methods: Vec::new(),
        spec_funcs: Vec::new(),
        spec_methods: Vec::new(),
        visited_spec_names: Vec::new(),
    };

    for &def_id in defs {
        match &arena[def_id].kind {
            Def::ExternFunction { .. } => buckets.imports.push(def_id),
            Def::Function { .. } => buckets.funcs.push(def_id),
            Def::Struct { name, methods, .. } => {
                let struct_name = arena[*name].name.clone();
                for &method_def_id in methods {
                    buckets.methods.push((struct_name.clone(), method_def_id));
                }
            }
            Def::Spec {
                name,
                defs: inner,
                ..
            } if mode == CompilationMode::Proof => {
                let spec_name = arena[*name].name.clone();
                buckets.visited_spec_names.push(spec_name.clone());
                for &inner_id in inner {
                    match &arena[inner_id].kind {
                        Def::Function { .. } => {
                            buckets.spec_funcs.push((spec_name.clone(), inner_id));
                        }
                        Def::Struct { name, methods, .. } => {
                            let struct_name = arena[*name].name.clone();
                            for &method_def_id in methods {
                                buckets.spec_methods.push((
                                    spec_name.clone(),
                                    struct_name.clone(),
                                    method_def_id,
                                ));
                            }
                        }
                        Def::Spec { name: inner_name, .. } => {
                            return Err(CodegenError::NestedSpecsNotSupported {
                                outer_spec: spec_name,
                                inner_spec: arena[*inner_name].name.clone(),
                            });
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    Ok(buckets)
}
