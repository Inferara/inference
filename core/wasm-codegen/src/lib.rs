//! WebAssembly code generation for the Inference compiler.
//!
//! This crate provides LLVM-based code generation from Inference's typed AST to LLVM IR.
//! The generated IR is returned as a [`CodegenOutput`] struct, which carries the IR text
//! along with metadata needed by the toolchain layer (in `core/cli`) to invoke `inf-llc`
//! and `rust-lld` with the correct target-specific flags.
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
//!   CodegenOutput { ir, target, mode, opt_level, module_name, has_main }
//!         |
//!         v  (CLI / toolchain layer)
//!   inf-llc -> rust-lld -> .wasm
//! ```
//!
//! # Non-Deterministic Extensions
//!
//! The compiler supports Inference's non-deterministic constructs through custom LLVM
//! intrinsics that compile to WebAssembly instructions in the 0xfc prefix space:
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
//! - **`Compile`** mode: Produces optimized production binaries. Spec nodes are stripped.
//!   The target's default optimization level applies (`-O3` for Wasm32, `-Oz` for Soroban).
//! - **`Proof`** mode: Produces literal, unoptimized WASM for Rocq formalization.
//!   All functions receive `optnone`+`noinline` barriers. Always uses `Wasm32` target.
//!
//! # Module Organization
//!
//! - [`compiler`] - LLVM IR generation and intrinsic handling (private)
//! - [`output`] - `CodegenOutput` struct definition
//! - [`target`] - `Target`, `CompilationMode`, and `OptLevel` enums

#![warn(clippy::pedantic)]

use inference_type_checker::typed_context::TypedContext;
use inkwell::{
    context::Context,
    targets::{InitializationConfig, Target as LlvmTarget},
};

use crate::compiler::Compiler;

mod compiler;
pub mod output;
pub mod target;

pub use output::CodegenOutput;
pub use target::{CompilationMode, OptLevel, Target};

/// Generates LLVM IR from a typed AST for the specified target and compilation mode.
///
/// This function performs LLVM IR generation and returns a [`CodegenOutput`] containing
/// the IR text and metadata needed by the toolchain layer to compile the IR to WebAssembly.
///
/// # Validation
///
/// - **`Proof` mode with non-`Wasm32` target**: Rejected. Proof mode emits custom 0xfc
///   intrinsics that only `inf-llc` handles; other targets cannot process these.
/// - **`Soroban` target with non-det operations (other than `spec`)**: Rejected. The
///   Soroban VM cannot execute custom 0xfc WebAssembly instructions. `spec` nodes are
///   safe because they are stripped in `compile` mode.
///
/// # Compilation Mode Behavior
///
/// - **`Proof` mode**: Adds `optnone`+`noinline` barriers on ALL functions (defense-in-depth
///   on top of `-O0`) to preserve 1:1 structural correspondence with source code.
/// - **`Compile` mode**: Uses the provided `opt_level`. When `OptLevel` is `Os`/`Oz`,
///   adds `optsize`/`minsize` IR function attributes (since `llc` does not accept `-Os`/`-Oz`).
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
    // Validate: proof mode requires Wasm32 target
    if mode == CompilationMode::Proof && target != Target::Wasm32 {
        return Err(anyhow::anyhow!(
            "Proof mode requires Wasm32 target. Proof mode emits custom 0xfc intrinsics \
             that only inf-llc handles; the {target:?} target cannot process these."
        ));
    }

    // Validate: Soroban target rejects non-det operations (other than spec)
    if target == Target::Soroban {
        for source_file in &typed_context.source_files() {
            for func_def in source_file.function_definitions() {
                if func_def.is_non_det() {
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

    LlvmTarget::initialize_webassembly(&InitializationConfig::default());
    let context = Context::create();
    let module_name = "output";
    let compiler = Compiler::new(&context, module_name);

    if typed_context.source_files().len() > 1 {
        todo!("Multi-file support not yet implemented");
    }

    // Traverse AST and generate LLVM IR for all function definitions
    if !typed_context.source_files().is_empty() {
        traverse_t_ast_with_compiler(typed_context, &compiler);
    }

    // Apply mode-specific IR attributes
    if mode == CompilationMode::Proof {
        // Proof mode: add optnone+noinline barriers on ALL functions
        // This is defense-in-depth on top of -O0
        compiler.add_proof_mode_barriers();
    } else {
        // Compile mode: add size optimization attributes if applicable
        compiler.add_size_optimization_attrs(opt_level);
    }

    // Emit IR and build output
    let ir = compiler.emit_ir(target);
    let has_main = compiler.has_main();

    Ok(CodegenOutput::new(
        ir,
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
/// LLVM IR for each function definition. Currently, only function definitions at the
/// module level are compiled; other top-level constructs (types, constants, etc.) are
/// not yet supported.
///
/// # Parameters
///
/// - `typed_context` - Typed AST with type information for all nodes
/// - `compiler` - LLVM compiler instance for IR generation
///
/// # Current Limitations
///
/// - Only function definitions are compiled
/// - Type definitions, constants, and other top-level items are ignored
/// - Multi-file compilation is not fully tested (see `codegen` function)
fn traverse_t_ast_with_compiler(typed_context: &TypedContext, compiler: &Compiler) {
    for source_file in &typed_context.source_files() {
        for func_def in source_file.function_definitions() {
            compiler.visit_function_definition(&func_def, typed_context);
        }
    }
}
