#![warn(clippy::pedantic)]

use crate::compiler::Compiler;
use inference_hir::hir::Hir;
use inkwell::{
    context::Context,
    targets::{InitializationConfig, Target},
};

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
pub fn codegen(hir: &Hir) -> anyhow::Result<Vec<u8>> {
    Target::initialize_webassembly(&InitializationConfig::default());
    let context = Context::create();
    let compiler = Compiler::new(&context, "wasm_module");

    if hir.arena.sources.is_empty() {
        return compiler.compile_to_wasm("output.wasm", 3);
    }
    if hir.arena.sources.len() > 1 {
        todo!("Multi-file support not yet implemented");
    }

    traverse_hir_with_compiler(hir, &compiler);
    let wasm_bytes = compiler.compile_to_wasm("output.wasm", 3)?;
    Ok(wasm_bytes)
}

fn traverse_hir_with_compiler(hir: &Hir, compiler: &Compiler) {
    for source_file in &hir.arena.sources {
        for func_def in source_file.function_definitions() {
            compiler.visit_function_definition(&func_def);
        }
    }
}
