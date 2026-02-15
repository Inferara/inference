//! Codegen validation tests for the Inference compiler.
//!
//! These tests verify:
//! - `codegen()` produces valid `CodegenOutput` with non-empty WASM bytes
//! - WASM contains expected content (exported functions, custom opcodes)
//! - Target validation (proof + Soroban rejection, Soroban + non-det rejection)
//! - Proof mode metadata matches compile mode for non-det-free code
//! - `has_main` detection

#[cfg(test)]
mod codegen_validation_tests {
    use crate::utils::{
        codegen_output, codegen_output_with_mode, codegen_with_full_config, codegen_with_target_mode,
    };
    use inference_wasm_codegen::{CompilationMode, OptLevel, Target};

    // --- WASM content tests ---

    #[test]
    fn codegen_returns_nonempty_wasm() {
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let output = codegen_output(source);
        assert!(
            !output.wasm().is_empty(),
            "CodegenOutput should contain non-empty WASM bytes"
        );
    }

    #[test]
    fn wasm_exports_function() {
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let output = codegen_output(source);
        let wasm = output.wasm();
        assert!(
            wasm_contains_bytes(wasm, b"hello_world"),
            "WASM should contain 'hello_world' export name"
        );
    }

    // --- Proof mode tests ---

    #[test]
    fn proof_mode_without_nondet_matches_compile_mode() {
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let compile_output =
            codegen_with_target_mode(source, Target::Wasm32, CompilationMode::Compile).unwrap();
        let proof_output =
            codegen_with_target_mode(source, Target::Wasm32, CompilationMode::Proof).unwrap();

        assert_eq!(
            compile_output.opt_level(),
            proof_output.opt_level(),
            "Proof mode without non-det should use the same opt level as compile mode"
        );
    }

    // --- Target validation tests ---

    #[test]
    fn codegen_rejects_proof_with_soroban() {
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let result = codegen_with_target_mode(source, Target::Soroban, CompilationMode::Proof);
        assert!(
            result.is_err(),
            "Proof mode with Soroban should be rejected"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Proof mode requires Wasm32"),
            "Error message should mention Wasm32 requirement. Got: {}",
            err_msg
        );
    }

    #[test]
    fn codegen_rejects_soroban_with_nondet() {
        let source = "pub fn with_nondet() -> i32 { return @; }";
        let result = codegen_with_target_mode(source, Target::Soroban, CompilationMode::Compile);
        assert!(
            result.is_err(),
            "Soroban target with non-det should be rejected"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("non-deterministic"),
            "Error message should mention non-deterministic operations. Got: {}",
            err_msg
        );
    }

    // --- Compile mode non-det tests ---

    #[test]
    fn compile_mode_with_nondet_contains_uzumaki_opcode() {
        let source = r#"
            pub fn with_uzumaki() -> i32 { return @; }
            pub fn regular() -> i32 { return 42; }
        "#;
        let output =
            codegen_with_target_mode(source, Target::Wasm32, CompilationMode::Compile).unwrap();
        let wasm = output.wasm();

        assert!(
            wasm_contains_bytes(wasm, &[0xfc, 0x31]),
            "Compile mode WASM should contain i32.uzumaki opcode (0xfc 0x31)"
        );

        assert!(
            wasm_contains_bytes(wasm, b"with_uzumaki"),
            "WASM should export with_uzumaki function"
        );
        assert!(
            wasm_contains_bytes(wasm, b"regular"),
            "WASM should export regular function"
        );
    }

    // --- Proof mode non-det tests ---

    #[test]
    fn proof_mode_wasm_contains_nondet_opcodes() {
        let source = r#"
            pub fn with_uzumaki() -> i32 { return @; }
            pub fn with_forall() { forall { const a: i32 = 42; } }
        "#;
        let output = codegen_output_with_mode(source, CompilationMode::Proof);
        let wasm = output.wasm();

        assert!(
            wasm_contains_bytes(wasm, &[0xfc, 0x31]),
            "Proof mode WASM should contain i32.uzumaki opcode (0xfc 0x31)"
        );

        assert!(
            wasm_contains_bytes(wasm, &[0xfc, 0x3a]),
            "Proof mode WASM should contain forall opcode (0xfc 0x3a)"
        );
    }

    // --- has_main detection tests ---

    #[test]
    fn has_main_true_for_public_main() {
        let source = "pub fn main() -> i32 { return 0; }";
        let output = codegen_output(source);
        assert!(
            output.has_main(),
            "has_main should be true when pub fn main() exists"
        );
    }

    #[test]
    fn has_main_false_without_main() {
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let output = codegen_output(source);
        assert!(
            !output.has_main(),
            "has_main should be false when no main function exists"
        );
    }

    #[test]
    fn has_main_false_for_private_main() {
        let source = "fn main() -> i32 { return 0; }";
        let output = codegen_output(source);
        assert!(
            !output.has_main(),
            "has_main should be false for private fn main()"
        );
    }

    // --- CodegenOutput metadata tests ---

    #[test]
    fn codegen_output_has_correct_metadata() {
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let output = codegen_with_full_config(
            source,
            Target::Wasm32,
            CompilationMode::Compile,
            OptLevel::O3,
        )
        .unwrap();

        assert_eq!(output.target(), Target::Wasm32);
        assert_eq!(output.mode(), CompilationMode::Compile);
        assert_eq!(output.opt_level(), OptLevel::O3);
        assert_eq!(output.module_name(), "output");
        assert!(!output.has_main());
    }

    #[test]
    fn codegen_soroban_compile_succeeds() {
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let output =
            codegen_with_target_mode(source, Target::Soroban, CompilationMode::Compile).unwrap();

        assert_eq!(output.target(), Target::Soroban);
        assert_eq!(output.mode(), CompilationMode::Compile);
        assert_eq!(output.opt_level(), OptLevel::Oz);
        assert!(!output.wasm().is_empty());
    }

    #[test]
    fn codegen_proof_wasm32_succeeds() {
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let output =
            codegen_with_target_mode(source, Target::Wasm32, CompilationMode::Proof).unwrap();

        assert_eq!(output.target(), Target::Wasm32);
        assert_eq!(output.mode(), CompilationMode::Proof);
        assert_eq!(output.opt_level(), OptLevel::O3);
        assert!(!output.wasm().is_empty());
    }

    // --- Helper functions ---

    /// Checks if a byte slice contains a given subsequence of bytes.
    fn wasm_contains_bytes(wasm: &[u8], needle: &[u8]) -> bool {
        wasm.windows(needle.len()).any(|window| window == needle)
    }
}
