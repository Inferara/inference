#![warn(clippy::pedantic)]
//! Inference is a programming language that is designed to be easy to learn and use.

/// Compiles the given source code to WebAssembly Text format (WAT).
///
/// # Panics
///
/// This function will panic if there is an error loading the Inference grammar.
/// Compiles the given source code to WebAssembly Text format (WAT).
///
/// # Errors
///
/// This function will return an error if the source code cannot be parsed or if there is an error during the AST building process.
///
/// # Panics
///
/// This function will panic if there is an error loading the Inference grammar.
#[must_use = "This function returns the compiled WebAssembly Text format as a string"]
pub fn compile_to_wat(source_code: &str) -> anyhow::Result<String> {
    let inference_language = tree_sitter_inference::language();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&inference_language)
        .expect("Error loading Inference grammar");
    let tree = parser.parse(source_code, None).unwrap();
    let code = source_code.as_bytes();
    let root_node = tree.root_node();
    let ast = inference_ast::builder::build_ast(root_node, code)?;
    Ok(inference_wat_codegen::wat_generator::generate_string_for_source_file(&ast))
}

/// Converts WebAssembly Text format (WAT) to WebAssembly binary format (WASM).
///
/// # Panics
///
/// This function will panic if the WAT string cannot be parsed.
/// Converts WebAssembly Text format (WAT) to WebAssembly binary format (WASM).
///
/// # Errors
///
/// This function will return an error if the WAT string cannot be parsed or if there is an error during the encoding process.
#[must_use = "This function returns the WebAssembly binary format as a vector of bytes"]
pub fn wat_to_wasm(wat: &str) -> anyhow::Result<Vec<u8>> {
    let buf = inf_wast::parser::ParseBuffer::new(wat)?;
    let mut module = inf_wast::parser::parse::<inf_wast::Wat>(&buf)?;
    let wasm = module.encode()?;
    Ok(wasm)
}

/// Compiles the given source code to WebAssembly binary format (WASM).
///
/// # Errors
///
/// This function will return an error if the WAT string cannot be parsed or if there is an error during the compilation process.
///
/// # Panics
///
/// This function will panic if the WAT string cannot be parsed.
#[must_use = "This function compiles Inference source code to the WebAssembly binary format as a vector of bytes"]
pub fn compile_to_wasm(source_code: &str) -> anyhow::Result<Vec<u8>> {
    let wat = compile_to_wat(source_code)?;
    wat_to_wasm(&wat)
}

/// Translates WebAssembly binary format (WASM) to Coq format.
///
/// # Errors
///
/// This function will return an error if the translation process fails.
pub fn wasm_to_v(wasm: &Vec<u8>) -> anyhow::Result<String> {
    if let Ok(v) =
        inference_wasm_coq_translator::wasm_parser::translate_bytes(&String::new(), wasm.as_slice())
    {
        Ok(v)
    } else {
        Err(anyhow::anyhow!("Error translating WebAssembly to V"))
    }
}
