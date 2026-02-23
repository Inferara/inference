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
//! - [`output`] - `CodegenOutput` struct definition
//! - [`target`] - `Target`, `CompilationMode`, and `OptLevel` enums

#![warn(clippy::pedantic)]

use inference_type_checker::typed_context::TypedContext;

use crate::compiler::Compiler;

mod compiler;
pub mod output;
pub mod target;

pub use output::CodegenOutput;
pub use target::{CompilationMode, OptLevel, Target};

/// Generates WebAssembly binary from a typed AST for the specified target and compilation mode.
///
/// This function builds a complete WASM module in-process via `wasm-encoder` and returns
/// a [`CodegenOutput`] containing the binary bytes and compilation metadata.
///
/// # Validation
///
/// - **`Proof` mode with non-`Wasm32` target**: Rejected. Proof mode emits custom 0xfc
///   non-deterministic instructions that only the Wasm32 target supports.
/// - **`Soroban` target with non-det operations (other than `spec`)**: Rejected. The
///   Soroban VM cannot execute custom 0xfc WebAssembly instructions. `spec` nodes are
///   safe because they are stripped in `compile` mode.
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

    if target == Target::Soroban {
        for source_file in &typed_context.source_files() {
            for func_def in source_file.function_definitions() {
                if func_def.is_non_det() {
                    cov_mark::hit!(wasm_codegen_soroban_rejects_nondet_function);
                    return Err(anyhow::anyhow!(
                        "Soroban target does not support non-deterministic operations. \
                         Function '{}' contains non-deterministic constructs (uzumaki, \
                         forall, exists, assume, or unique blocks) that produce custom \
                         0xfc WebAssembly instructions incompatible with the Soroban VM.",
                        func_def.name()
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

    if !typed_context.source_files().is_empty() {
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

/// Traverses the typed AST and compiles all function definitions.
///
/// This function iterates through all source files in the typed context and generates
/// WASM bytecode for each function definition. Currently, only function definitions at
/// the module level are compiled; other top-level constructs (types, constants, etc.)
/// are not yet supported.
///
/// # Parameters
///
/// - `typed_context` - Typed AST with type information for all nodes
/// - `compiler` - WASM compiler instance for binary generation
///
/// # Current Limitations
///
/// - Only function definitions are compiled
/// - Type definitions, constants, and other top-level items are ignored
/// - Multi-file compilation is not fully tested (see `codegen` function)
fn traverse_t_ast_with_compiler(typed_context: &TypedContext, compiler: &mut Compiler) {
    for source_file in &typed_context.source_files() {
        let func_defs = source_file.function_definitions();
        // Pre-scan: build function name-to-index map so that forward references
        // (callee defined after caller in source) resolve correctly at call sites.
        compiler.build_func_name_to_idx(&func_defs);
        for func_def in func_defs {
            compiler.visit_function_definition(&func_def, typed_context);
        }
    }
}
