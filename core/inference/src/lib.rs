#![warn(clippy::pedantic)]
//! Inference is a programming language that is designed to be easy to learn and use.

use inference_ast::{arena::Arena, builder::Builder};
use inference_type_checker::typed_context::TypedContext;

/// Parses the given source code and returns a Typed AST.
///
/// # Errors
///
/// This function will return an error if the source code cannot be parsed or if there is an error during the AST building process.
///
/// # Panics
///
/// This function will panic if there is an error loading the Inference grammar.
pub fn parse(source_code: &str) -> anyhow::Result<Arena> {
    let inference_language = tree_sitter_inference::language();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&inference_language)
        .expect("Error loading Inference grammar");
    let tree = parser.parse(source_code, None).unwrap();
    let code = source_code.as_bytes();
    let root_node = tree.root_node();
    let mut builder = Builder::new();
    builder.add_source_code(root_node, code);
    let builder = builder.build_ast()?;
    Ok(builder.arena())
}

/// Performs type checking on the given AST and returns a typed context.
///
/// # Errors
///
/// This function will return an error if type checking fails while building the typed context.
pub fn type_check(arena: Arena) -> anyhow::Result<TypedContext> {
    let type_checker_builder =
        inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)?;
    Ok(type_checker_builder.typed_context())
}

/// Analyzes the given Typed AST for type correctness.
///
/// # Errors
///
/// This function will return an error if the type analysis fails.
pub fn analyze(_: &TypedContext) -> anyhow::Result<()> {
    // todo!("Type analysis not yet implemented");
    Ok(())
}

/// Generates WebAssembly binary format (WASM) from the given Typed AST.
///
/// # Errors
///
/// This function will return an error if the code generation fails.
pub fn codegen(typed_context: &TypedContext) -> anyhow::Result<Vec<u8>> {
    inference_wasm_codegen::codegen(typed_context)
}

/// Translates WebAssembly binary format (WASM) to Coq format.
///
/// # Errors
///
/// This function will return an error if the translation process fails.
pub fn wasm_to_v(mod_name: &str, wasm: &Vec<u8>) -> anyhow::Result<String> {
    if let Ok(v) =
        inference_wasm_to_v_translator::wasm_parser::translate_bytes(mod_name, wasm.as_slice())
    {
        Ok(v)
    } else {
        Err(anyhow::anyhow!("Error translating WebAssembly to V"))
    }
}
