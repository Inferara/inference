//! Tests for the `inference::codegen()` convenience wrapper.
//!
//! These tests verify that `inference::codegen()` correctly delegates to
//! `inference_wasm_codegen::codegen()` with default settings (Wasm32, Compile, O3).

#[cfg(test)]
mod inference_codegen_wrapper_tests {
    use inference_wasm_codegen::{CompilationMode, OptLevel, Target};

    #[test]
    fn inference_codegen_wrapper_returns_output() {
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let arena = inference::parse(source).unwrap();
        let typed_context = inference::type_check(arena).unwrap();
        let output = inference::codegen(&typed_context, "output").unwrap();

        assert!(!output.wasm().is_empty());
    }

    #[test]
    fn inference_codegen_wrapper_uses_wasm32_o3() {
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let arena = inference::parse(source).unwrap();
        let typed_context = inference::type_check(arena).unwrap();
        let output = inference::codegen(&typed_context, "output").unwrap();

        assert_eq!(output.target(), Target::Wasm32);
        assert_eq!(output.mode(), CompilationMode::Compile);
        assert_eq!(output.opt_level(), OptLevel::O3);
    }
}
