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
        codegen_output, codegen_output_no_analysis, codegen_output_with_mode,
        codegen_output_with_mode_no_analysis, codegen_with_full_config,
        codegen_with_target_mode, codegen_with_target_mode_no_analysis,
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
        let result = codegen_with_target_mode_no_analysis(source, Target::Soroban, CompilationMode::Compile);
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
            codegen_with_target_mode_no_analysis(source, Target::Wasm32, CompilationMode::Compile).unwrap();
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
        let output = codegen_output_with_mode_no_analysis(source, CompilationMode::Proof);
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
        let output = codegen_output_no_analysis(source);
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
        let output = codegen_output_no_analysis(source);
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
        let output = codegen_output_no_analysis(source);
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
        let source = r#"pub fn let_uzumaki_i32_test() -> i32 { let a: i32 = @; return a; }"#;
        let output = codegen_output_no_analysis(source);
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
        let source = r#"pub fn let_uzumaki_i64_test() -> i64 { let b: i64 = @; return b; }"#;
        let output = codegen_output_no_analysis(source);
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
        let source = r#"pub fn let_i8_test() -> i8 { let a: i8 = -128; return a; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Variable definition i8 WASM is invalid: {e}"));
    }

    #[test]
    fn variable_definition_i16_literal_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_variable_definition);
        let source = r#"pub fn let_i16_test() -> i16 { let b: i16 = -32768; return b; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Variable definition i16 WASM is invalid: {e}"));
    }

    #[test]
    fn variable_definition_u8_literal_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_variable_definition);
        let source = r#"pub fn let_u8_test() -> u8 { let c: u8 = 255; return c; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Variable definition u8 WASM is invalid: {e}"));
    }

    #[test]
    fn variable_definition_u16_literal_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_variable_definition);
        let source = r#"pub fn let_u16_test() -> u16 { let d: u16 = 65535; return d; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Variable definition u16 WASM is invalid: {e}"));
    }

    #[test]
    fn variable_definition_u32_literal_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_variable_definition);
        let source = r#"pub fn let_u32_test() -> u32 { let e: u32 = 4294967295; return e; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Variable definition u32 WASM is invalid: {e}"));
    }

    #[test]
    fn variable_definition_u64_literal_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_variable_definition);
        let source = r#"pub fn let_u64_test() -> u64 { let f: u64 = 18446744073709551615; return f; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Variable definition u64 WASM is invalid: {e}"));
    }

    // --- Function parameter tests ---

    #[test]
    fn function_with_i32_param_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_function_params);
        let source = r#"pub fn identity(x: i32) -> i32 { return x; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Function with i32 param WASM is invalid: {e}"));
    }

    #[test]
    fn function_with_i64_param_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_function_params);
        let source = r#"pub fn identity_i64(x: i64) -> i64 { return x; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Function with i64 param WASM is invalid: {e}"));
    }

    #[test]
    fn function_with_bool_param_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_function_params);
        let source = r#"pub fn identity_bool(x: bool) -> bool { return x; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Function with bool param WASM is invalid: {e}"));
    }

    #[test]
    fn function_with_multiple_params_produces_valid_wasm() {
        cov_mark::check_count!(wasm_codegen_emit_function_params, 2);
        let source = r#"pub fn add_params(a: i32, b: i32) -> i32 { return a; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Function with multiple params WASM is invalid: {e}"));
    }

    #[test]
    fn function_param_accessible_as_local_in_body() {
        cov_mark::check!(wasm_codegen_emit_function_params);
        let source = r#"pub fn identity(x: i32) -> i32 { let y: i32 = x; return y; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Function param as local init WASM is invalid: {e}"));
    }

    // --- Function call codegen tests ---

    #[test]
    fn function_call_no_args_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_function_call);
        let source = r#"
            fn get_value() -> i32 { return 42; }
            pub fn caller() -> i32 { return get_value(); }
        "#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("No-arg function call WASM is invalid: {e}"));
    }

    #[test]
    fn function_call_one_arg_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_function_call);
        let source = r#"
            fn identity(x: i32) -> i32 { return x; }
            pub fn caller(v: i32) -> i32 { return identity(v); }
        "#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("One-arg function call WASM is invalid: {e}"));
    }

    #[test]
    fn function_call_two_args_produces_valid_wasm() {
        cov_mark::check_count!(wasm_codegen_emit_function_call, 1);
        let source = r#"
            fn first(a: i32, b: i32) -> i32 { return a; }
            pub fn caller(x: i32, y: i32) -> i32 { return first(x, y); }
        "#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Two-arg function call WASM is invalid: {e}"));
    }

    #[test]
    fn function_call_as_variable_initializer_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_function_call);
        cov_mark::check!(wasm_codegen_emit_variable_definition);
        let source = r#"
            fn get_value() -> i32 { return 7; }
            pub fn caller() -> i32 { let x: i32 = get_value(); return x; }
        "#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Call-as-var-init WASM is invalid: {e}"));
    }

    #[test]
    fn function_call_forward_reference_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_function_call);
        let source = r#"
            pub fn caller() -> i32 { return callee(); }
            fn callee() -> i32 { return 55; }
        "#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Forward-reference call WASM is invalid: {e}"));
    }

    #[test]
    fn function_call_chained_produces_valid_wasm() {
        cov_mark::check_count!(wasm_codegen_emit_function_call, 2);
        let source = r#"
            fn inner(x: i32) -> i32 { return x; }
            fn middle(x: i32) -> i32 { return inner(x); }
            pub fn outer(x: i32) -> i32 { return middle(x); }
        "#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Chained call WASM is invalid: {e}"));
    }

    #[test]
    fn function_call_with_literal_arg_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_function_call);
        let source = r#"
            fn identity(x: i32) -> i32 { return x; }
            pub fn caller() -> i32 { return identity(42); }
        "#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Call-with-literal WASM is invalid: {e}"));
    }

    #[test]
    fn function_call_with_i64_param_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_function_call);
        let source = r#"
            fn identity_i64(x: i64) -> i64 { return x; }
            pub fn caller(v: i64) -> i64 { return identity_i64(v); }
        "#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("i64 call WASM is invalid: {e}"));
    }

    #[test]
    fn function_call_execution_returns_correct_value() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            fn identity(x: i32) -> i32 { return x; }
            pub fn caller() -> i32 { return identity(123); }
        "#;
        let wasm = codegen_output(source).wasm().to_vec();

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let caller: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "caller")
            .unwrap_or_else(|e| panic!("Failed to get 'caller': {e}"));
        assert_eq!(
            caller.call(&mut store, ()).unwrap_or_else(|e| panic!("Call failed: {e}")),
            123
        );
    }

    #[test]
    fn function_call_forward_reference_executes_correctly() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn caller() -> i32 { return callee(); }
            fn callee() -> i32 { return 77; }
        "#;
        let wasm = codegen_output(source).wasm().to_vec();

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let caller: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "caller")
            .unwrap_or_else(|e| panic!("Failed to get 'caller': {e}"));
        assert_eq!(
            caller.call(&mut store, ()).unwrap_or_else(|e| panic!("Call failed: {e}")),
            77
        );
    }

    #[test]
    fn void_function_call_as_statement_does_not_emit_drop_in_nondet_block() {
        let source = r#"
            fn do_nothing() { }
            pub fn caller() { forall { do_nothing(); } }
        "#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Void call in non-det block WASM is invalid: {e}"));
        let drop_count = wasm.iter().filter(|&&b| b == 0x1a).count();
        assert_eq!(
            drop_count, 0,
            "Void function call should not emit Drop (0x1a)"
        );
    }

    #[test]
    fn value_returning_call_as_statement_emits_drop() {
        let source = r#"
            fn get_value() -> i32 { return 42; }
            pub fn caller() { get_value(); }
        "#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Value-returning call as statement WASM is invalid: {e}"));
        let drop_count = wasm.iter().filter(|&&b| b == 0x1a).count();
        assert_eq!(
            drop_count, 1,
            "Value-returning function call in statement position should emit exactly one Drop (0x1a)"
        );
    }

    #[test]
    fn uzumaki_as_function_argument_produces_valid_wasm() {
        let source = r#"
            fn identity(x: i32) -> i32 { return x; }
            pub fn spec() -> i32 { return identity(@); }
        "#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Uzumaki as function argument WASM is invalid: {e}"));
        let has_uzumaki_opcode = wasm.windows(2).any(|w| w == [0xfc, 0x31]);
        assert!(
            has_uzumaki_opcode,
            "WASM should contain i32.uzumaki opcode (0xfc 0x31)"
        );
    }

    #[test]
    fn uzumaki_i64_as_function_argument_produces_valid_wasm() {
        let source = r#"
            fn identity_i64(x: i64) -> i64 { return x; }
            pub fn spec() -> i64 { return identity_i64(@); }
        "#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("i64 uzumaki as function argument WASM is invalid: {e}"));
        let has_uzumaki_opcode = wasm.windows(2).any(|w| w == [0xfc, 0x32]);
        assert!(
            has_uzumaki_opcode,
            "WASM should contain i64.uzumaki opcode (0xfc 0x32)"
        );
    }

    #[test]
    fn value_returning_call_in_nondet_block_with_let_produces_valid_wasm() {
        let source = r#"
            fn get_value() -> i32 { return 42; }
            pub fn spec() { forall { let x: i32 = get_value(); } }
        "#;
        let wasm = codegen_output(source).wasm().to_vec();
        inf_wasmparser::validate(&wasm).unwrap_or_else(|e| {
            panic!("Value-returning call in non-det block with let WASM is invalid: {e}")
        });
    }

    // --- Binary expression validation tests ---

    #[test]
    fn binary_add_i32_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        let source = r#"pub fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("add i32 WASM is invalid: {e}"));
    }

    #[test]
    fn binary_sub_i32_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        let source = r#"pub fn sub(a: i32, b: i32) -> i32 { return a - b; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("sub i32 WASM is invalid: {e}"));
    }

    #[test]
    fn binary_mul_i32_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        let source = r#"pub fn mul(a: i32, b: i32) -> i32 { return a * b; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("mul i32 WASM is invalid: {e}"));
    }

    #[test]
    fn binary_div_signed_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        let source = r#"pub fn div_s(a: i32, b: i32) -> i32 { return a / b; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("div signed WASM is invalid: {e}"));
    }

    #[test]
    fn binary_div_unsigned_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        let source = r#"pub fn div_u(a: u32, b: u32) -> u32 { return a / b; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("div unsigned WASM is invalid: {e}"));
    }

    #[test]
    fn binary_mod_signed_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        let source = r#"pub fn rem_s(a: i32, b: i32) -> i32 { return a % b; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("mod signed WASM is invalid: {e}"));
    }

    #[test]
    fn binary_eq_i32_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        let source = r#"pub fn eq(a: i32, b: i32) -> bool { return a == b; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("eq i32 WASM is invalid: {e}"));
    }

    #[test]
    fn binary_lt_signed_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        let source = r#"pub fn lt_s(a: i32, b: i32) -> bool { return a < b; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("lt signed WASM is invalid: {e}"));
    }

    #[test]
    fn binary_lt_unsigned_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        let source = r#"pub fn lt_u(a: u32, b: u32) -> bool { return a < b; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("lt unsigned WASM is invalid: {e}"));
    }

    #[test]
    fn binary_and_bool_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        let source = r#"pub fn and(a: bool, b: bool) -> bool { return a && b; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("and bool WASM is invalid: {e}"));
    }

    #[test]
    fn binary_or_bool_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        let source = r#"pub fn or(a: bool, b: bool) -> bool { return a || b; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("or bool WASM is invalid: {e}"));
    }

    #[test]
    fn binary_bitand_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        let source = r#"pub fn bitand(a: i32, b: i32) -> i32 { return a & b; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("bitand WASM is invalid: {e}"));
    }

    #[test]
    fn binary_shr_signed_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        let source = r#"pub fn shr_s(a: i32, b: i32) -> i32 { return a >> b; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("shr signed WASM is invalid: {e}"));
    }

    #[test]
    fn binary_shr_unsigned_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        let source = r#"pub fn shr_u(a: u32, b: u32) -> u32 { return a >> b; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("shr unsigned WASM is invalid: {e}"));
    }

    #[test]
    fn binary_add_i64_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        let source = r#"pub fn add_i64(a: i64, b: i64) -> i64 { return a + b; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("add i64 WASM is invalid: {e}"));
    }

    // --- Unary expression validation tests ---

    #[test]
    fn unary_neg_i32_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_prefix_unary_expression);
        cov_mark::check!(wasm_codegen_emit_unary_neg);
        let source = r#"pub fn neg(a: i32) -> i32 { return -a; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("neg i32 WASM is invalid: {e}"));
    }

    #[test]
    fn unary_not_bool_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_prefix_unary_expression);
        cov_mark::check!(wasm_codegen_emit_unary_not);
        let source = r#"pub fn not(a: bool) -> bool { return !a; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("not bool WASM is invalid: {e}"));
    }

    #[test]
    fn unary_bitnot_i32_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_prefix_unary_expression);
        cov_mark::check!(wasm_codegen_emit_unary_bitnot);
        let source = r#"pub fn bitnot(a: i32) -> i32 { return ~a; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("bitnot i32 WASM is invalid: {e}"));
    }

    // --- Parenthesized expression validation tests ---

    #[test]
    fn parenthesized_expr_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_parenthesized_expression);
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        let source = r#"pub fn paren(a: i32, b: i32) -> i32 { return (a + b); }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("parenthesized expr WASM is invalid: {e}"));
    }

    // --- Compound expression validation tests ---

    #[test]
    fn compound_bitnot_shr_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        cov_mark::check!(wasm_codegen_emit_prefix_unary_expression);
        cov_mark::check!(wasm_codegen_emit_unary_bitnot);
        // `~a >> 2` — parsed as `(~a) >> 2`; verifies bitnot + shr compiles and validates
        let source = r#"pub fn compound(a: i32) -> i32 { return ~a >> 2; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("compound bitnot+shr WASM is invalid: {e}"));
    }

    // --- Variable definition with binary initializer validation tests ---

    #[test]
    fn binary_as_let_init_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        cov_mark::check!(wasm_codegen_emit_variable_definition);
        let source = r#"pub fn binop_let(a: i32, b: i32) -> i32 { let r: i32 = a + b; return r; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("binary as let init WASM is invalid: {e}"));
    }

    // --- Method codegen: name mangling and indexing tests ---

    #[test]
    fn method_associated_function_produces_mangled_name_in_wasm() {
        let source = r#"struct Point { x: i32; y: i32; fn new(x: i32, y: i32) -> Point { return Point { x: x, y: y }; } } pub fn main() -> i32 { return 0; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Method codegen WASM is invalid: {e}"));
        assert!(
            wasm_contains_bytes(wasm, b"Point.new"),
            "WASM should contain mangled method name 'Point.new'"
        );
    }

    #[test]
    fn method_associated_function_body_compiles_and_validates() {
        let source = r#"struct Point { x: i32; y: i32; fn create(x: i32, y: i32) -> Point { return Point { x: x, y: y }; } } pub fn main() -> i32 { return 0; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Method body codegen WASM is invalid: {e}"));
        let wat = wasmprinter::print_bytes(wasm)
            .unwrap_or_else(|e| panic!("Failed to print WAT: {e}"));
        assert!(
            wat.contains("Point.create"),
            "WAT should contain mangled method name 'Point.create'\n{wat}"
        );
    }

    #[test]
    fn method_multiple_associated_functions_produce_distinct_mangled_names() {
        let source = r#"struct Counter { value: i32; fn zero() -> Counter { return Counter { value: 0 }; } fn with_value(v: i32) -> Counter { return Counter { value: v }; } } pub fn main() -> i32 { return 0; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Multiple methods codegen WASM is invalid: {e}"));
        assert!(
            wasm_contains_bytes(wasm, b"Counter.zero"),
            "WASM should contain mangled name 'Counter.zero'"
        );
        assert!(
            wasm_contains_bytes(wasm, b"Counter.with_value"),
            "WASM should contain mangled name 'Counter.with_value'"
        );
    }

    #[test]
    fn method_struct_returning_associated_function_detected_as_sret() {
        let source = r#"struct Point { x: i32; y: i32; fn origin() -> Point { return Point { x: 0, y: 0 }; } } pub fn main() -> i32 { return 0; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("sret method codegen WASM is invalid: {e}"));
        let wat = wasmprinter::print_bytes(wasm)
            .unwrap_or_else(|e| panic!("Failed to print WAT: {e}"));
        assert!(
            wat.contains("Point.origin"),
            "WAT should contain mangled method name 'Point.origin'\n{wat}"
        );
        let origin_line = wat
            .lines()
            .find(|line| line.contains("$Point.origin"))
            .unwrap_or_else(|| panic!("No line with $Point.origin in WAT:\n{wat}"));
        assert!(
            origin_line.contains("(param") && origin_line.contains("i32)"),
            "sret function should have an i32 param (sret pointer):\n{origin_line}"
        );
        assert!(
            !origin_line.contains("(result"),
            "sret function should NOT have a (result ...) return:\n{origin_line}"
        );
    }

    #[test]
    fn method_multiple_structs_produce_correct_mangled_names() {
        let source = r#"struct Point { x: i32; y: i32; fn origin() -> Point { return Point { x: 0, y: 0 }; } } struct Size { w: i32; h: i32; fn zero() -> Size { return Size { w: 0, h: 0 }; } } pub fn main() -> i32 { return 0; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Multi-struct method codegen WASM is invalid: {e}"));
        assert!(
            wasm_contains_bytes(wasm, b"Point.origin"),
            "WASM should contain mangled name 'Point.origin'"
        );
        assert!(
            wasm_contains_bytes(wasm, b"Size.zero"),
            "WASM should contain mangled name 'Size.zero'"
        );
    }

    // --- Method codegen: self parameter handling tests (Phase 3) ---

    #[test]
    fn method_immutable_self_compiles_and_validates() {
        cov_mark::check!(wasm_codegen_emit_self_param);
        let source = r#"struct Point { x: i32; y: i32; fn get_x(self) -> i32 { return self.x; } } pub fn main() -> i32 { return 0; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Immutable self method WASM is invalid: {e}"));
        let wat = wasmprinter::print_bytes(wasm)
            .unwrap_or_else(|e| panic!("Failed to print WAT: {e}"));
        assert!(
            wat.contains("Point.get_x"),
            "WAT should contain mangled method name 'Point.get_x'\n{wat}"
        );
    }

    #[test]
    fn method_mutable_self_compiles_and_validates() {
        cov_mark::check!(wasm_codegen_emit_self_param);
        cov_mark::check!(wasm_codegen_emit_self_copy_on_entry);
        let source = r#"struct Counter { value: i32; fn increment(mut self) { self.value = self.value + 1; } } pub fn main() -> i32 { return 0; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Mutable self method WASM is invalid: {e}"));
        let wat = wasmprinter::print_bytes(wasm)
            .unwrap_or_else(|e| panic!("Failed to print WAT: {e}"));
        assert!(
            wat.contains("Counter.increment"),
            "WAT should contain mangled method name 'Counter.increment'\n{wat}"
        );
    }

    #[test]
    fn method_immutable_self_no_frame_copy() {
        let source = r#"struct Point { x: i32; y: i32; fn get_x(self) -> i32 { return self.x; } } pub fn main() -> i32 { return 0; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Immutable self method WASM is invalid: {e}"));
        let wat = wasmprinter::print_bytes(wasm)
            .unwrap_or_else(|e| panic!("Failed to print WAT: {e}"));
        let get_x_func = extract_function_body(&wat, "Point.get_x");
        assert!(
            !get_x_func.contains("memory.copy"),
            "Immutable self method should NOT have memory.copy (no frame copy):\n{get_x_func}"
        );
    }

    #[test]
    fn method_mutable_self_has_frame_copy() {
        let source = r#"struct Counter { value: i32; fn set_value(mut self, v: i32) { self.value = v; } } pub fn main() -> i32 { return 0; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Mutable self method WASM is invalid: {e}"));
        let wat = wasmprinter::print_bytes(wasm)
            .unwrap_or_else(|e| panic!("Failed to print WAT: {e}"));
        let set_value_func = extract_function_body(&wat, "Counter.set_value");
        assert!(
            set_value_func.contains("memory.copy"),
            "Mutable self method should have memory.copy (frame copy):\n{set_value_func}"
        );
    }

    #[test]
    fn method_self_with_return_value_compiles() {
        let source = r#"struct Point { x: i32; y: i32; fn sum(self) -> i32 { return self.x + self.y; } } pub fn main() -> i32 { return 0; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Self method with return value WASM is invalid: {e}"));
    }

    #[test]
    fn method_self_with_extra_params_compiles() {
        let source = r#"struct Point { x: i32; y: i32; fn add(self, dx: i32, dy: i32) -> i32 { return self.x + dx + self.y + dy; } } pub fn main() -> i32 { return 0; }"#;
        let output = codegen_output(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Self method with extra params WASM is invalid: {e}"));
    }

    // --- Helper functions ---

    /// Checks if a byte slice contains a given subsequence of bytes.
    fn wasm_contains_bytes(wasm: &[u8], needle: &[u8]) -> bool {
        wasm.windows(needle.len()).any(|window| window == needle)
    }

    /// Extracts the WAT text for a specific function by name from a full WAT module.
    ///
    /// NOTE: This uses parenthesis-depth counting, which is fragile if WAT
    /// formatting changes or if a function name is a substring of another.
    /// Sufficient for test code but not a general-purpose WAT parser.
    fn extract_function_body(wat: &str, func_name: &str) -> String {
        let marker = format!("${func_name}");
        let mut in_func = false;
        let mut depth = 0i32;
        let mut lines = Vec::new();
        for line in wat.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("(func ") && trimmed.contains(&marker) {
                in_func = true;
            }
            if in_func {
                lines.push(line);
                depth += line.matches('(').count() as i32;
                depth -= line.matches(')').count() as i32;
                if depth <= 0 {
                    break;
                }
            }
        }
        lines.join("\n")
    }
}
