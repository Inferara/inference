#![warn(clippy::pedantic)]

use inference_ast::ast::Ast;
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
pub fn codegen(t_ast: &Ast) -> anyhow::Result<Vec<u8>> {
    Target::initialize_webassembly(&InitializationConfig::default());
    let context = Context::create();
    let compiler = Compiler::new(&context, "wasm_module");

    if t_ast.source_files.is_empty() {
        return compiler.compile_to_wasm("output.wasm", 3);
    }
    if t_ast.source_files.len() > 1 {
        todo!("Multi-file support not yet implemented");
    }

    traverse_t_ast_with_compiler(t_ast, &compiler);

    let wasm_bytes = compiler.compile_to_wasm("output.wasm", 3)?;
    Ok(wasm_bytes)
}

fn traverse_t_ast_with_compiler(t_ast: &Ast, compiler: &Compiler) {
    for source_file in &t_ast.source_files {
        for func_def in source_file.function_definitions() {
            compiler.visit_function_definition(&func_def);
        }
    }
}
