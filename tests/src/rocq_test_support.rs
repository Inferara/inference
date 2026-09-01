use crate::utils::{build_ast, get_test_data_path};
use inference_type_checker::TypeCheckerBuilder;
use inference_wasm_codegen::{CompilationMode, OptLevel, Target};
use rustc_hash::FxHashMap;

/// Compiles one fixture under `tests/test_data/inf/` and returns its WASM.
pub(crate) fn compile_fixture(file: &str, module_name: &str, mode: CompilationMode) -> Vec<u8> {
    let path = get_test_data_path().join("inf").join(file);
    let source =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let arena = build_ast(source);
    let typed_context = TypeCheckerBuilder::build_typed_context(arena)
        .unwrap_or_else(|e| panic!("type check failed for {file}: {e}"))
        .typed_context();
    inference_wasm_codegen::codegen(
        &typed_context,
        module_name,
        inference_wasm_codegen::CodegenOptions {
            target: Target::Wasm32,
            mode,
            opt_level: OptLevel::O3,
            features: inference_wasm_codegen::EmitFeatures::default(),
            layout: inference_wasm_codegen::MemoryLayout::default(),
        },
    )
    .unwrap_or_else(|e| panic!("codegen failed for {file}: {e}"))
    .wasm()
    .to_vec()
}

/// Proof-mode `.v` for one single-file fixture, driven entirely in-process.
pub(crate) fn generate_v(file: &str, module_name: &str) -> String {
    let wasm = compile_fixture(file, module_name, CompilationMode::Proof);
    translate(file, module_name, &wasm)
}

/// Translates a compiled module using its embedded proof metadata.
pub(crate) fn translate(file: &str, module_name: &str, wasm: &[u8]) -> String {
    // Empty explicit maps: the per-spec indices and the hassert obligations
    // both ride along in the embedded `inference.spec_funcs` /
    // `inference.hspecs` custom sections (see ROCQ_CONTRACT.md). For a
    // linked module they are also the only correct source — the linker
    // rewrote the embedded indices into the post-merge space, leaving
    // codegen's own record stale.
    let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
    let empty_hspecs = inference::HSpecMap::default();
    inference::wasm_to_v(module_name, wasm, &empty, &empty_hspecs)
        .unwrap_or_else(|e| panic!("wasm_to_v failed for {file}: {e}"))
}
