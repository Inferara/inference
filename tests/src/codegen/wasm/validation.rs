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
        codegen_output, codegen_output_with_mode, codegen_with_full_config,
        codegen_with_target_mode,
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
        cov_mark::check!(wasm_codegen_proof_mode_rejected_non_wasm32);
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
        cov_mark::check!(wasm_codegen_soroban_rejects_nondet_function);
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

    // --- i64 and bool coverage tests ---

    #[test]
    fn wasm_contains_i64_uzumaki_opcode() {
        let source = "pub fn get_i64_uzumaki() -> i64 { return @; }";
        let output = codegen_output(source);
        let wasm = output.wasm();
        assert!(
            wasm_contains_bytes(wasm, &[0xfc, 0x32]),
            "WASM should contain i64.uzumaki opcode (0xfc 0x32)"
        );
    }

    #[test]
    fn bool_literal_produces_valid_wasm() {
        let source = r#"
            pub fn get_true() -> bool { return true; }
            pub fn get_false() -> bool { return false; }
        "#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        assert!(!wasm.is_empty(), "Bool literal WASM should be non-empty");
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Bool literal WASM is invalid: {e}"));
    }

    #[test]
    fn private_function_not_exported() {
        let source = r#"
            fn private_helper() -> i32 { return 1; }
            pub fn public_caller() -> i32 { return 42; }
        "#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        assert!(
            wasm_contains_bytes(wasm, b"public_caller"),
            "Public function should appear in WASM"
        );
        use wasmtime::{Engine, Module, Store};
        let engine = Engine::default();
        let module = Module::new(&engine, wasm)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));
        assert!(
            instance.get_func(&mut store, "public_caller").is_some(),
            "public_caller should be exported"
        );
        assert!(
            instance.get_func(&mut store, "private_helper").is_none(),
            "private_helper should NOT be exported"
        );
    }

    #[test]
    fn i64_return_produces_valid_wasm() {
        let source = "pub fn get_i64() -> i64 { return @; }";
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("i64 return WASM is invalid: {e}"));
    }

    // --- Nested non-det block tests (Bug #3: Drop emission uses parent_blocks_stack.last()) ---

    #[test]
    fn nested_nondet_forall_inside_exists_produces_valid_wasm() {
        let source =
            r#"pub fn nested_forall_in_exists() { exists { forall { const a: i32 = 42; } } }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Nested non-det WASM is invalid: {e}"));
        assert!(
            wasm_contains_bytes(wasm, &[0xfc, 0x3b]),
            "WASM should contain exists opcode (0xfc 0x3b)"
        );
        assert!(
            wasm_contains_bytes(wasm, &[0xfc, 0x3a]),
            "WASM should contain forall opcode (0xfc 0x3a)"
        );
    }

    #[test]
    fn nested_nondet_forall_inside_forall_produces_valid_wasm() {
        let source = r#"pub fn nested_forall() { forall { forall { const a: i32 = 42; } } }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Nested forall-in-forall WASM is invalid: {e}"));
        let forall_count = wasm.windows(2).filter(|w| w == &[0xfc, 0x3a]).count();
        assert_eq!(
            forall_count, 2,
            "WASM should contain exactly 2 forall opcodes for nested forall blocks"
        );
    }

    #[test]
    fn nested_nondet_expression_drop_uses_innermost_block() {
        let source = r#"pub fn nested_drop_test() -> i32 { exists { forall { const x: i32 = 99; } } return 0; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Nested non-det with const WASM is invalid: {e}"));
        assert!(
            wasm_contains_bytes(wasm, &[0xfc, 0x3b]),
            "WASM should contain exists opcode"
        );
        assert!(
            wasm_contains_bytes(wasm, &[0xfc, 0x3a]),
            "WASM should contain forall opcode"
        );
    }

    // --- i64 literal tests (Bug #4: lower_literal dispatches I64Const for i64/u64) ---

    #[test]
    fn i64_literal_in_return_rejected_by_type_checker() {
        //FIXME: The type checker infers bare number literals as i32, so `return 100`
        // in an i64 function is rejected. Once the type checker supports i64 literal
        // inference (e.g., via bidirectional type checking that propagates the expected
        // return type into the literal), this test should be updated to verify that the
        // WASM output contains i64.const (opcode 0x42) instead of i32.const (opcode 0x41).
        // The lower_literal fix in compiler.rs correctly dispatches I64Const for i64-typed
        // literals, but the type checker prevents this code path from being reached today.
        let source = "pub fn get_i64_value() -> i64 { return 100; }";
        let arena = crate::utils::build_ast(source.to_string());
        let type_check_result =
            inference_type_checker::TypeCheckerBuilder::build_typed_context(arena);
        match type_check_result {
            Ok(_) => panic!("Type checker should reject i32 literal in i64 return position"),
            Err(err) => {
                let err_msg = err.to_string();
                assert!(
                    err_msg.contains("type mismatch"),
                    "Error should be a type mismatch. Got: {err_msg}"
                );
            }
        }
    }

    #[test]
    fn i64_uzumaki_in_return_emits_i64_uzumaki_opcode() {
        let source = "pub fn get_i64() -> i64 { return @; }";
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("i64 uzumaki return WASM is invalid: {e}"));
        assert!(
            wasm_contains_bytes(wasm, &[0xfc, 0x32]),
            "WASM should contain i64.uzumaki opcode (0xfc 0x32)"
        );
    }

    #[test]
    fn nondet_void_block_trailing_expression_emits_drop() {
        let source = r#"pub fn drop_test() { forall { const a: i32 = 42; a; } }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm).unwrap_or_else(|e| panic!("Drop-path WASM is invalid: {e}"));
        assert!(
            wasm_contains_bytes(wasm, &[0x1a]),
            "WASM should contain Drop opcode (0x1a) for trailing expression in non-det void block"
        );
    }

    // --- Unsigned integer literal tests (lower_literal U8/U16/U32/U64 arms) ---

    #[test]
    fn u8_literal_emits_i32const() {
        let source = r#"pub fn u8_test() { const x: u8 = 255; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("u8 literal WASM is invalid: {e}"));
        assert!(
            wasm_contains_bytes(wasm, &[0x41]),
            "WASM should contain i32.const (0x41) for u8 literal"
        );
    }

    #[test]
    fn u16_literal_emits_i32const() {
        let source = r#"pub fn u16_test() { const x: u16 = 60000; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("u16 literal WASM is invalid: {e}"));
        assert!(
            wasm_contains_bytes(wasm, &[0x41]),
            "WASM should contain i32.const (0x41) for u16 literal"
        );
    }

    #[test]
    fn u32_literal_emits_i32const() {
        let source = r#"pub fn u32_test() { const x: u32 = 3000000000; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("u32 literal WASM is invalid: {e}"));
        assert!(
            wasm_contains_bytes(wasm, &[0x41]),
            "WASM should contain i32.const (0x41) for u32 literal"
        );
    }

    #[test]
    fn u64_literal_emits_i64const() {
        let source = r#"pub fn u64_test() { const x: u64 = 9000000000000000000; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("u64 literal WASM is invalid: {e}"));
        assert!(
            wasm_contains_bytes(wasm, &[0x42]),
            "WASM should contain i64.const (0x42) for u64 literal"
        );
    }

    // --- Variable definition codegen tests ---

    #[test]
    fn variable_definition_i32_literal_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_variable_definition);
        let source = r#"pub fn let_i32_test() -> i32 { let x: i32 = 42; return x; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Variable definition i32 WASM is invalid: {e}"));
    }

    #[test]
    fn variable_definition_bool_literal_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_variable_definition);
        let source = r#"pub fn let_bool_test() -> bool { let f: bool = true; return f; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Variable definition bool WASM is invalid: {e}"));
    }

    #[test]
    fn variable_definition_uzumaki_i32_emits_uzumaki_opcode() {
        cov_mark::check!(wasm_codegen_emit_variable_definition);
        cov_mark::check!(wasm_codegen_variable_definition_uzumaki_i32);
        let source = r#"pub fn let_uzumaki_i32_test() -> i32 { let a: i32 = @; return a; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Variable definition uzumaki i32 WASM is invalid: {e}"));
        assert!(
            wasm_contains_bytes(wasm, &[0xfc, 0x31]),
            "WASM should contain i32.uzumaki opcode (0xfc 0x31)"
        );
    }

    #[test]
    fn variable_definition_uzumaki_i64_emits_uzumaki_opcode() {
        cov_mark::check!(wasm_codegen_emit_variable_definition);
        cov_mark::check!(wasm_codegen_variable_definition_uzumaki_i64);
        let source = r#"pub fn let_uzumaki_i64_test() -> i64 { let b: i64 = @; return b; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Variable definition uzumaki i64 WASM is invalid: {e}"));
        assert!(
            wasm_contains_bytes(wasm, &[0xfc, 0x32]),
            "WASM should contain i64.uzumaki opcode (0xfc 0x32)"
        );
    }

    #[test]
    fn variable_definition_identifier_init_produces_valid_wasm() {
        cov_mark::check_count!(wasm_codegen_emit_variable_definition, 2);
        let source =
            r#"pub fn let_ident_test() -> i32 { let x: i32 = 10; let y: i32 = x; return y; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Variable definition identifier init WASM is invalid: {e}"));
    }

    #[test]
    fn variable_definition_i8_literal_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_variable_definition);
        let source = r#"pub fn let_i8_test() -> i8 { let x: i8 = 100; return x; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Variable definition i8 WASM is invalid: {e}"));
    }

    #[test]
    fn variable_definition_i16_literal_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_variable_definition);
        let source = r#"pub fn let_i16_test() -> i16 { let y: i16 = 1000; return y; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Variable definition i16 WASM is invalid: {e}"));
    }

    #[test]
    fn variable_definition_u8_literal_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_variable_definition);
        let source = r#"pub fn let_u8_test() -> u8 { let z: u8 = 200; return z; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Variable definition u8 WASM is invalid: {e}"));
    }

    #[test]
    fn variable_definition_u16_literal_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_variable_definition);
        let source = r#"pub fn let_u16_test() -> u16 { let w: u16 = 40000; return w; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Variable definition u16 WASM is invalid: {e}"));
    }

    // --- Helper functions ---

    /// Checks if a byte slice contains a given subsequence of bytes.
    fn wasm_contains_bytes(wasm: &[u8], needle: &[u8]) -> bool {
        wasm.windows(needle.len()).any(|window| window == needle)
    }
}
