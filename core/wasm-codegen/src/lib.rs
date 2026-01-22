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
//! # Example Usage
//!
//! ```no_run
//! use inference_wasm_codegen::codegen;
//! use inference_type_checker::typed_context::TypedContext;
//!
//! fn compile(typed_context: &TypedContext) -> anyhow::Result<Vec<u8>> {
//!     // Generate WASM bytecode from typed AST
//!     let wasm_bytes = codegen(typed_context)?;
//!     Ok(wasm_bytes)
//! }
//! ```
//!
//! # Module Organization
//!
//! - [`compiler`] - LLVM IR generation and intrinsic handling (private)
//! - [`utils`] - External toolchain invocation and environment setup (private)
//! - [`codegen`] - Public API for WebAssembly generation

#![warn(clippy::pedantic)]

use inference_ast::nodes::Definition;
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
/// Supports multiple source files by traversing all parsed modules.
///
/// Returns an error if code generation fails.
pub fn codegen(typed_context: &TypedContext) -> anyhow::Result<Vec<u8>> {
    Target::initialize_webassembly(&InitializationConfig::default());
    let context = Context::create();
    let compiler = Compiler::new(&context, "wasm_module");

    if typed_context.source_files().is_empty() {
        return compiler.compile_to_wasm("output.wasm", 3);
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
/// - Module name mangling is not yet implemented for nested functions
fn traverse_t_ast_with_compiler(typed_context: &TypedContext, compiler: &Compiler) {
    for source_file in &typed_context.source_files() {
        compile_definitions(&source_file.definitions, typed_context, compiler);
    }
}

fn compile_definitions(
    definitions: &[Definition],
    typed_context: &TypedContext,
    compiler: &Compiler,
) {
    for definition in definitions {
        match definition {
            Definition::Function(func_def) => {
                compiler.visit_function_definition(func_def, typed_context);
            }
            Definition::Module(module_def) => {
                if let Some(body) = module_def.body.borrow().as_ref() {
                    compile_definitions(body, typed_context, compiler);
                }
            }
            _ => {}
        }
    }
}
