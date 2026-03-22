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

use inference_ast::ids::DefId;
use inference_ast::nodes::Def;
use inference_type_checker::typed_context::TypedContext;

use crate::compiler::Compiler;

mod compiler;
mod errors;
mod memory;
pub mod output;
pub mod target;

pub use output::CodegenOutput;
pub use target::{CompilationMode, OptLevel, Target};

/// Generates WebAssembly binary from a typed AST for the specified target and compilation mode.
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

    let module_name = "output";
    let mut compiler = Compiler::new(module_name);

    if typed_context.source_files().len() > 1 {
        todo!("Multi-file support not yet implemented");
    }

    if typed_context.source_files().len() != 0 {
        traverse_t_ast_with_compiler(typed_context, &mut compiler);
    }

    let wasm = compiler.finish();
    let has_main = compiler.has_main();

    Ok(CodegenOutput::new(
        wasm,
        target,
        mode,
        opt_level,
        module_name.to_string(),
        has_main,
    ))
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
///    method bodies are compiled with `current_method_struct` set so that
///    `self` parameter handling (Phase 3+) knows which struct type is in scope.
fn traverse_t_ast_with_compiler(typed_context: &TypedContext, compiler: &mut Compiler) {
    let arena = typed_context.arena();
    for source_file in typed_context.source_files() {
        // Collect top-level function DefIds
        let func_def_ids: Vec<DefId> = source_file
            .defs
            .iter()
            .copied()
            .filter(|&def_id| matches!(arena[def_id].kind, Def::Function { .. }))
            .collect();

        // Collect method DefIds with their parent struct name
        let mut method_defs: Vec<(String, DefId)> = Vec::new();
        for &def_id in &source_file.defs {
            if let Def::Struct {
                name, methods, ..
            } = &arena[def_id].kind
            {
                let struct_name = arena[*name].name.clone();
                for &method_def_id in methods {
                    method_defs.push((struct_name.clone(), method_def_id));
                }
            }
        }

        // Stage 1: Register all indices before any body compilation
        compiler.build_func_name_to_idx(arena, &func_def_ids, typed_context);
        #[allow(clippy::cast_possible_truncation)]
        let method_base_idx = compiler.func_idx_after_toplevel(func_def_ids.len() as u32);
        compiler.build_method_name_to_idx(
            arena,
            &method_defs,
            typed_context,
            method_base_idx,
        );

        // Stage 2: Compile top-level function bodies
        for &def_id in &func_def_ids {
            compiler.visit_function_definition(def_id, arena, typed_context);
        }

        // Stage 2b: Compile method bodies
        for (struct_name, method_def_id) in &method_defs {
            compiler.set_current_method_struct(Some(struct_name.clone()));
            compiler.visit_function_definition(*method_def_id, arena, typed_context);
            compiler.set_current_method_struct(None);
        }
    }
}
