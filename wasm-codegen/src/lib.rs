#![warn(clippy::pedantic)]
use inference_ast::t_ast::TypedAst;

pub mod module;

/// Generates WebAssembly bytecode from a typed AST.
///
/// # Errors
///
/// Returns an error if more than one source file is present in the AST, as multi-file
/// support is not yet implemented.
///
/// Returns an error if code generation fails.
pub fn codegen(t_ast: &TypedAst) -> anyhow::Result<Vec<u8>> {
    let mut builder = module::WasmModuleBuilder::new();
    if t_ast.source_files.is_empty() {
        return Ok(builder.finish());
    }
    if t_ast.source_files.len() > 1 {
        todo!("Multi-file support not yet implemented");
    }

    let source_file = &t_ast.source_files[0];
    for func_def in source_file.function_definitions() {
        let _ = builder.push_function(&func_def);
    }

    let wasm_bytes = builder.finish();
    Ok(wasm_bytes)
}
