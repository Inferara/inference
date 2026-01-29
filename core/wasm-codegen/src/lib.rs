//! WebAssembly code generation for the Inference compiler.
//!
//! This crate provides LLVM-based code generation from Inference's typed AST to WebAssembly
//! bytecode. It handles standard WebAssembly instructions as well as custom extensions for
//! non-deterministic operations required for formal verification.
//!
//! # Architecture
//!
//! The code generation pipeline consists of several layers:
//!
//! ```text
//! Typed AST (TypedContext)
//!         ↓
//!     Compiler  ← LLVM Context
//!         ↓
//!     LLVM IR
//!         ↓
//!    inf-llc   ← Modified LLVM compiler
//!         ↓
//!   WASM Object (.o)
//!         ↓
//!   rust-lld  ← WebAssembly linker
//!         ↓
//!   WASM Module (.wasm)
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
//! # External Dependencies
//!
//! This crate requires two external binaries to be available:
//!
//! - **inf-llc** - Modified LLVM compiler with Inference intrinsics support
//! - **rust-lld** - WebAssembly linker from the Rust toolchain
//!
//! These must be located in the `bin/` directory relative to the executable. See the
//! repository README for download links and setup instructions.
//!
//! # Platform Support
//!
//! - Linux x86-64 (requires libLLVM.so in `lib/` directory)
//! - macOS Apple Silicon (M1/M2)
//! - Windows x86-64 (requires DLLs in `bin/` directory)
//!
//! # Module Organization
//!
//! - [`compiler`] - LLVM IR generation and intrinsic handling (private)
//! - [`utils`] - External toolchain invocation and environment setup (private)
//! - [`codegen`] - Public API for WebAssembly generation

#![warn(clippy::pedantic)]

use inference_type_checker::typed_context::TypedContext;
use inkwell::{
    context::Context,
    targets::{InitializationConfig, Target},
};

use crate::compiler::Compiler;

mod compiler;
mod utils;

/// Generates WebAssembly bytecode from a typed AST.
///
/// # Errors
///
/// Returns an error if more than one source file is present in the AST, as multi-file
/// support is not yet implemented.
///
/// Returns an error if code generation fails.
pub fn codegen(typed_context: &TypedContext) -> anyhow::Result<Vec<u8>> {
    Target::initialize_webassembly(&InitializationConfig::default());
    let context = Context::create();
    let compiler = Compiler::new(&context, "wasm_module");

    if typed_context.source_files().is_empty() {
        return compiler.compile_to_wasm("output.wasm", 3);
    }
    if typed_context.source_files().len() > 1 {
        todo!("Multi-file support not yet implemented");
    }

    traverse_t_ast_with_compiler(typed_context, &compiler);
    let wasm_bytes = compiler.compile_to_wasm("output.wasm", 3)?;
    Ok(wasm_bytes)
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
