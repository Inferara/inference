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
        codegen_with_full_config_no_analysis, codegen_with_target_mode,
        codegen_with_target_mode_no_analysis, wasm_codegen, wasm_codegen_no_analysis,
        wasm_codegen_with_layout,
    };
    use inf_wasmparser::{Operator, Parser, Payload};
    use inference_wasm_codegen::{
        CompilationMode, MemoryLayout, MemoryLayoutSource, OptLevel, Target,
    };

    // WASM content tests ---

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

    // Memory layout tests ---

    /// A program that allocates an array frame, so both places the layout is
    /// read are emitted: the memory section exists only for a module that needs
    /// memory, and `__stack_pointer` only accompanies it.
    const FRAME_ALLOCATING_SOURCE: &str = r#"
pub fn read_first() -> i32 {
    let arr: [i32; 4] = [1, 2, 3, 4];
    return arr[0];
}
"#;

    /// A requested layout must survive the whole pipeline and reach the emitted
    /// module, in both of the numbers it carries.
    ///
    /// The page count and the stack size are asserted together because they are
    /// read independently — the memory section takes one, the stack-pointer
    /// global takes the other — so pinning a single number would leave the other
    /// free to be ignored. A layout whose stack is half its memory is what
    /// separates them: under the default the two are numerically equal, and an
    /// emitter that confused one for the other would still look correct.
    ///
    /// The default-layout half is the control. It is what makes the first half
    /// evidence about the layout rather than about this particular source: the
    /// same program compiled twice differs in exactly these two numbers, and
    /// differs in them only because the layout asked it to.
    #[test]
    fn a_configured_layout_reaches_the_emitted_module() {
        let configured = wat_of(&wasm_codegen_with_layout(
            FRAME_ALLOCATING_SOURCE,
            MemoryLayout::resolve(Some(2), Some(32_768), MemoryLayoutSource::Flag)
                .expect("a half-page stack in two pages is admissible"),
        ));
        assert!(
            configured.contains("(memory (;0;) 2 2)"),
            "the memory section must declare the requested 2 fixed pages:\n{configured}"
        );
        assert!(
            configured.contains("(global (;0;) (mut i32) i32.const 32768)"),
            "the stack pointer must start at the requested stack size:\n{configured}"
        );

        let default = wat_of(&wasm_codegen_with_layout(
            FRAME_ALLOCATING_SOURCE,
            MemoryLayout::default(),
        ));
        assert!(
            default.contains("(memory (;0;) 1 1)"),
            "the same source under the default layout must declare one page:\n{default}"
        );
        assert!(
            default.contains("(global (;0;) (mut i32) i32.const 65536)"),
            "the same source under the default layout must start the stack pointer at one \
             page:\n{default}"
        );
    }

    // Proof mode tests ---

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

    // Target validation tests ---

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

    // Compile mode non-det tests ---

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

    /// A spec-wrapped non-det program built in compile mode (analysis ON) must not
    /// leak any verification operator into the executable module: the `spec` is
    /// proof-only and stripped, so a well-formed compile-mode artifact carries none
    /// of the custom `0xfc` opcodes that fail standard wasm validation. Walk every
    /// function body's operators and assert none is one of the six verification
    /// operators. The i32 + i64 uzumaki cover both uzumaki opcodes; `forall` and
    /// `exists` cover the block operators.
    #[test]
    fn compile_mode_spec_nondet_emits_no_verification_operators() {
        let source = r#"
            spec S {
                fn check() -> i32 {
                    forall {
                        let x: i32 = @;
                    }
                    exists {
                        let y: i64 = @;
                    }
                    return 0;
                }
            }
            pub fn main() -> i32 {
                return 0;
            }
        "#;
        let wasm = wasm_codegen(source);
        for payload in Parser::new(0).parse_all(&wasm) {
            let payload = payload.unwrap_or_else(|e| panic!("failed to parse emitted wasm: {e}"));
            let Payload::CodeSectionEntry(body) = payload else {
                continue;
            };
            let operators = body
                .get_operators_reader()
                .unwrap_or_else(|e| panic!("failed to read a function body: {e}"));
            for op in operators {
                let op = op.unwrap_or_else(|e| panic!("failed to decode an operator: {e}"));
                assert!(
                    !matches!(
                        op,
                        Operator::Forall { .. }
                            | Operator::Exists { .. }
                            | Operator::Assume { .. }
                            | Operator::Unique { .. }
                            | Operator::I32Uzumaki { .. }
                            | Operator::I64Uzumaki { .. }
                    ),
                    "compile-mode module must contain no verification operators, found: {op:?}"
                );
            }
        }
    }

    // Proof mode non-det tests ---

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

    #[test]
    fn proof_mode_spec_fn_calls_sibling_spec_fn_by_bare_name() {
        // A spec function calling a sibling spec function by its bare name is the
        // supported intra-spec call form. Proof mode emits the spec bodies, so this
        // is where the call is actually lowered; it must produce a valid module and
        // never miss its callee index. (The qualified `Spec::fn()` form is rejected
        // at type-check, so it never reaches codegen.)
        //
        // Both functions carry a claim because a spec free function that only
        // computes has no obligation and is rejected before any WASM is emitted;
        // `inner` still returns a value, which is what keeps the sibling call in
        // term position where the callee index has to be resolved.
        let source = r#"
            spec Check {
                fn inner() -> i32 { assert(42 == 42); return 42; }
                fn outer() forall { assert(inner() == 42); }
            }
            pub fn main() -> i32 { return 0; }
        "#;
        let output = codegen_output_with_mode(source, CompilationMode::Proof);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("Proof-mode spec sibling-call WASM is invalid: {e}"));
    }

    #[test]
    fn proof_mode_qualified_spec_call_stops_at_type_check_not_codegen() {
        // End-to-end regression guard for the original bug: a qualified call to a
        // spec function from executable code used to type-check and then PANIC in
        // codegen ("not found in func_name_to_idx") because spec functions get no
        // executable index. Proof mode is the mode that actually lowers spec bodies,
        // so it is the mode in which the panic could fire — the codegen guard alone
        // is invisible unless the type checker is verified to stop the program first.
        // Driving the real pipeline (type-check -> Proof codegen) confirms the
        // failure now surfaces as a clean type error and codegen is never entered.
        let source = "spec Check { fn verify_inner() -> i32 { return 42; } } \
                      pub fn run() -> i32 { return Check::verify_inner(); }";
        let arena = crate::utils::build_ast(source.to_string());
        let tc = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena);
        let err = tc
            .err()
            .expect("qualified spec call must fail type-check before codegen")
            .to_string();
        assert!(
            err.contains("cannot call spec function `Check::verify_inner`")
                && err.contains("proof-only"),
            "expected the proof-only spec diagnostic, got: {err}"
        );
    }

    // has_main detection tests ---

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

    // CodegenOutput metadata tests ---

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

    // i64 and bool coverage tests ---

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

    // Nested non-det block tests (Bug #3: Drop emission uses parent_blocks_stack.last()) ---

    #[test]
    fn nested_nondet_forall_inside_exists_produces_valid_wasm() {
        let source =
            r#"pub fn nested_forall_in_exists() { exists { forall { const a: i32 = 42; } } }"#;
        let output = codegen_output_no_analysis(source);
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
        let output = codegen_output_no_analysis(source);
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
        let output = codegen_output_no_analysis(source);
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

    // i64 literal tests (Bug #4: lower_literal dispatches I64Const for i64/u64) ---

    /// The declared return type is what the bare literal denotes, so `return
    /// 100` in an `-> i64` function emits `i64.const 100` (`0x42`) rather than
    /// the `i32.const` the literal's old default forced.
    #[test]
    fn i64_literal_in_return_emits_i64_const() {
        let source = "pub fn get_i64_value() -> i64 { return 100; }";
        let output = codegen_output_no_analysis(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm)
            .unwrap_or_else(|e| panic!("i64 literal return WASM is invalid: {e}"));
        assert!(
            wasm_contains_bytes(wasm, &[0x42, 0xE4, 0x00]),
            "WASM should contain i64.const 100 (0x42 0xE4 0x00)"
        );
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
        let output = codegen_output_no_analysis(source);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm).unwrap_or_else(|e| panic!("Drop-path WASM is invalid: {e}"));
        assert!(
            wasm_contains_bytes(wasm, &[0x1a]),
            "WASM should contain Drop opcode (0x1a) for trailing expression in non-det void block"
        );
    }

    // Unsigned integer literal tests (lower_literal U8/U16/U32/U64 arms) ---

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

    // Variable definition codegen tests ---

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

    // Function parameter tests ---

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

    // Function call codegen tests ---

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
        let output = codegen_output_no_analysis(source);
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
        let wasm = wasm_codegen_no_analysis(source);
        inf_wasmparser::validate(&wasm)
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
        let wasm = wasm_codegen_no_analysis(source);
        inf_wasmparser::validate(&wasm)
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
        let wasm = codegen_output_no_analysis(source).wasm().to_vec();
        inf_wasmparser::validate(&wasm).unwrap_or_else(|e| {
            panic!("Value-returning call in non-det block with let WASM is invalid: {e}")
        });
    }

    // Binary expression validation tests ---

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

    // Unary expression validation tests ---

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

    // Parenthesized expression validation tests ---

    #[test]
    fn parenthesized_expr_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_parenthesized_expression);
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        let source = r#"pub fn paren(a: i32, b: i32) -> i32 { return (a + b); }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("parenthesized expr WASM is invalid: {e}"));
    }

    // Compound expression validation tests ---

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

    // Variable definition with binary initializer validation tests ---

    #[test]
    fn binary_as_let_init_produces_valid_wasm() {
        cov_mark::check!(wasm_codegen_emit_binary_expression);
        cov_mark::check!(wasm_codegen_emit_variable_definition);
        let source = r#"pub fn binop_let(a: i32, b: i32) -> i32 { let r: i32 = a + b; return r; }"#;
        let output = codegen_output(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("binary as let init WASM is invalid: {e}"));
    }

    // Method codegen: name mangling and indexing tests ---

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

    // Method codegen: self parameter handling tests (Phase 3) ---

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
            !get_x_func.contains("__frame_ptr"),
            "Immutable self method reads the caller's struct in place, so it allocates \
             no frame and copies nothing into one:\n{get_x_func}"
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
        // A region copy moves untyped bytes, so its accesses carry the
        // conservative one-byte alignment hint that a typed field store never
        // does — which is what distinguishes the frame copy from the body's own
        // `self.value = v` store.
        assert!(
            set_value_func.contains("i32.store align=1"),
            "Mutable self method should copy the caller's struct into its frame:\n{set_value_func}"
        );
        assert!(
            set_value_func.contains("local.set $self"),
            "...and rebind `self` to that copy so the body mutates it:\n{set_value_func}"
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

    // Degenerate struct shapes ---

    /// A field-less struct parameter that the body *assigns* still lowers.
    ///
    /// Such a struct lays out to zero bytes and so is given no frame slot, which
    /// makes it the one parameter whose missing slot does not mean "nothing ever
    /// writes it". A guard that read slot presence alone as the verdict of the
    /// write scan aborts here.
    ///
    /// A045 now rejects this shape one phase earlier — a field-less struct has no
    /// value representation, so no such parameter reaches codegen from a program
    /// the analysis pass accepts (#332). Analysis is therefore skipped here, which
    /// keeps the test doing its original job: pinning that the codegen guard
    /// beneath the rule stays quiet rather than aborting.
    #[test]
    fn field_less_struct_parameter_assigned_in_body_lowers_without_tripping_the_guard() {
        let source = r#"
struct Nothing { }
pub fn take(mut e: Nothing) -> i32 { e = e; return 0; }
"#;
        let output = codegen_output_no_analysis(source);
        inf_wasmparser::validate(output.wasm())
            .unwrap_or_else(|e| panic!("Field-less struct parameter WASM is invalid: {e}"));
    }

    // Wasm 1.0 corpus invariants ---

    /// The bulk-memory operators, none of which codegen may emit.
    ///
    /// `memory.fill` and `memory.copy` are the two the compiler used to emit for
    /// frame initialization and compound copies; `memory.init` and `data.drop`
    /// belong to the same proposal and are listed so that reintroducing bulk
    /// memory by another route is caught here too.
    ///
    /// This has to be an operator walk rather than a byte scan: the bulk opcodes
    /// share the `0xfc` prefix with the LEB immediates of ordinary instructions
    /// and with the compiler's own non-deterministic opcodes, so scanning for the
    /// prefix reports matches in modules that contain no bulk op at all.
    fn assert_no_bulk_memory_operator(wasm: &[u8], label: &str) {
        for payload in Parser::new(0).parse_all(wasm) {
            let payload = payload.unwrap_or_else(|e| panic!("failed to parse {label}: {e}"));
            let Payload::CodeSectionEntry(body) = payload else {
                continue;
            };
            let operators = body
                .get_operators_reader()
                .unwrap_or_else(|e| panic!("failed to read a function body of {label}: {e}"));
            for op in operators {
                let op =
                    op.unwrap_or_else(|e| panic!("failed to decode an operator of {label}: {e}"));
                assert!(
                    !matches!(
                        op,
                        Operator::MemoryFill { .. }
                            | Operator::MemoryCopy { .. }
                            | Operator::MemoryInit { .. }
                            | Operator::DataDrop { .. }
                    ),
                    "{label} must contain no bulk-memory operator, found: {op:?}"
                );
            }
        }
    }

    /// Whether a module carries any of the compiler's custom verification
    /// opcodes, which is what separates a proof-mode or analysis-skipped artifact
    /// from an ordinary executable one.
    ///
    /// Fixture names do not answer this — `nondet` and `array_nondet` are compiled
    /// in compile mode with analysis skipped, and a proof-mode module need not be
    /// named for it — so the classification reads the operators themselves.
    fn contains_verification_operator(wasm: &[u8]) -> bool {
        for payload in Parser::new(0).parse_all(wasm) {
            let Ok(Payload::CodeSectionEntry(body)) = payload else {
                continue;
            };
            let Ok(operators) = body.get_operators_reader() else {
                continue;
            };
            for op in operators {
                let Ok(op) = op else { continue };
                if matches!(
                    op,
                    Operator::Forall { .. }
                        | Operator::Exists { .. }
                        | Operator::Assume { .. }
                        | Operator::Unique { .. }
                        | Operator::I32Uzumaki { .. }
                        | Operator::I64Uzumaki { .. }
                ) {
                    return true;
                }
            }
        }
        false
    }

    /// Asserts a module carries at least one bulk-memory operator.
    ///
    /// The inverse of [`assert_no_bulk_memory_operator`], and it earns its keep
    /// for the same reason the corpus gate does: a golden family whose whole
    /// purpose is to hold the opt-in instruction level would still pass a byte
    /// comparison if the opt-in silently stopped taking effect and the goldens
    /// were regenerated. Requiring the operator makes that regeneration fail.
    fn assert_has_bulk_memory_operator(wasm: &[u8], label: &str) {
        for payload in Parser::new(0).parse_all(wasm) {
            let payload = payload.unwrap_or_else(|e| panic!("failed to parse {label}: {e}"));
            let Payload::CodeSectionEntry(body) = payload else {
                continue;
            };
            let operators = body
                .get_operators_reader()
                .unwrap_or_else(|e| panic!("failed to read a function body of {label}: {e}"));
            for op in operators {
                let op =
                    op.unwrap_or_else(|e| panic!("failed to decode an operator of {label}: {e}"));
                if matches!(
                    op,
                    Operator::MemoryFill { .. }
                        | Operator::MemoryCopy { .. }
                        | Operator::MemoryInit { .. }
                        | Operator::DataDrop { .. }
                ) {
                    return;
                }
            }
        }
        panic!("{label} carries no bulk-memory operator, so it is not an opt-in artifact");
    }

    /// Root of the opt-in golden family: modules the compiler produced with a
    /// post-MVP feature requested, so they are deliberately not Wasm 1.0.
    ///
    /// Both partitions are defined by this one path — the default gates exclude
    /// the subtree and the opt-in gates cover exactly it — so the two can only
    /// disagree about which side a golden belongs to, never leave one ungated.
    fn bulk_memory_golden_root() -> std::path::PathBuf {
        crate::utils::get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("bulk_memory_golden")
    }

    /// Every golden `.wasm` under `tests/test_data/codegen`, both partitions,
    /// sorted so failures name the same file across runs.
    ///
    /// `out` directories are skipped: they are the gitignored landing place for
    /// `infs build` run by hand against a fixture tree, so whatever they hold is
    /// whichever compiler someone last pointed at it, not a golden this suite
    /// maintains.
    fn all_golden_wasm_artifacts() -> Vec<std::path::PathBuf> {
        fn collect(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
            let entries = std::fs::read_dir(dir)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()));
            for entry in entries {
                let path = entry.expect("failed to read a directory entry").path();
                if path.is_dir() {
                    if path.file_name().is_some_and(|name| name == "out") {
                        continue;
                    }
                    collect(&path, found);
                } else if path.extension().is_some_and(|ext| ext == "wasm") {
                    found.push(path);
                }
            }
        }
        let mut found = Vec::new();
        collect(
            &crate::utils::get_test_data_path().join("codegen"),
            &mut found,
        );
        found.sort();
        found
    }

    /// The goldens the WebAssembly 1.0 invariants below apply to: everything
    /// except the opt-in family.
    fn golden_wasm_artifacts() -> Vec<std::path::PathBuf> {
        let root = bulk_memory_golden_root();
        all_golden_wasm_artifacts()
            .into_iter()
            .filter(|path| !path.starts_with(&root))
            .collect()
    }

    /// The opt-in family, which the inverse gates below apply to.
    fn bulk_memory_golden_artifacts() -> Vec<std::path::PathBuf> {
        let root = bulk_memory_golden_root();
        all_golden_wasm_artifacts()
            .into_iter()
            .filter(|path| path.starts_with(&root))
            .collect()
    }

    /// Every codegen fixture that compiles as a stand-alone file.
    ///
    /// Multi-file fixtures keep their sources under a `src` directory and are
    /// only meaningful as a tree, so they are excluded here; their merged output
    /// is covered through [`golden_wasm_artifacts`].
    fn single_file_corpus_sources() -> Vec<(String, String)> {
        fn collect(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
            let entries = std::fs::read_dir(dir)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()));
            for entry in entries {
                let path = entry.expect("failed to read a directory entry").path();
                if path.is_dir() {
                    if path.file_name().is_some_and(|name| name == "src") {
                        continue;
                    }
                    collect(&path, found);
                } else if path.extension().is_some_and(|ext| ext == "inf") {
                    found.push(path);
                }
            }
        }
        let mut paths = Vec::new();
        collect(
            &crate::utils::get_test_data_path().join("codegen"),
            &mut paths,
        );
        paths.sort();
        paths
            .into_iter()
            .map(|path| {
                let source = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
                (path.display().to_string(), source)
            })
            .collect()
    }

    /// No golden artifact in the corpus carries a bulk-memory operator.
    ///
    /// The goldens are the compiler's own output for every shape the suite
    /// covers — frames of every size, compound parameters, sret returns, and the
    /// non-deterministic modules that analysis would otherwise reject — so this
    /// is the broadest statement of the invariant that can be made from
    /// artifacts alone.
    #[test]
    fn corpus_goldens_contain_no_bulk_memory_operator() {
        let artifacts = golden_wasm_artifacts();
        assert!(
            artifacts.len() >= 100,
            "expected the corpus to hold at least 100 golden modules, found {}; \
             a collector that silently found nothing would pass this test vacuously",
            artifacts.len()
        );
        for path in &artifacts {
            let wasm = std::fs::read(path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            assert_no_bulk_memory_operator(&wasm, &path.display().to_string());
        }
    }

    /// Nor does any module the compiler produces in proof mode.
    ///
    /// Proof mode lowers `spec` bodies that compile mode drops, so it reaches
    /// emission sites the goldens never exercise. Recompiling every single-file
    /// fixture in proof mode covers those sites without needing a golden for
    /// each. Analysis is skipped so that fixtures written to exercise a construct
    /// analysis rejects still reach codegen.
    #[test]
    fn proof_mode_corpus_contains_no_bulk_memory_operator() {
        let sources = single_file_corpus_sources();
        assert!(
            sources.len() >= 100,
            "expected at least 100 single-file fixtures, found {}",
            sources.len()
        );
        let mut compiled = 0usize;
        for (label, source) in &sources {
            let Ok(output) = codegen_with_full_config_no_analysis(
                source,
                Target::Wasm32,
                CompilationMode::Proof,
                Target::Wasm32.default_opt_level(),
            ) else {
                continue;
            };
            assert_no_bulk_memory_operator(output.wasm(), label);
            compiled += 1;
        }
        assert!(
            compiled >= 100,
            "expected at least 100 fixtures to reach proof-mode codegen, only {compiled} did"
        );
    }

    /// Validates a module at the WebAssembly 1.0 feature level, through the
    /// *upstream* `wasmparser` rather than this workspace's `inf-wasmparser`.
    ///
    /// The two crates in this file are deliberate, not an oversight.
    /// `inf-wasmparser` is a fork taught to accept Inference's custom `0xfc`
    /// opcodes, which makes it the right tool for walking a proof-mode module —
    /// and the wrong one for this question, because a parser that has been
    /// extended to accept the compiler's own inventions cannot testify that the
    /// compiler's output is standard WebAssembly. Running an unmodified upstream
    /// validator is the stronger statement: an independent implementation, which
    /// knows nothing about this project, accepts the artifact as genuine Wasm 1.0.
    ///
    /// `WasmFeatures::WASM1` is the W3C Wasm 1.0 level: the MVP plus mutable
    /// globals. `MVP` alone is the wrong gate — it predates mutable globals, and
    /// every module the compiler emits exports `__stack_pointer` as a mutable
    /// global, so `MVP` would reject the whole corpus for a reason that has
    /// nothing to do with bulk memory. `WASM1` is what an MVP-class embedded
    /// interpreter accepts, which is the level this compiler now targets.
    ///
    /// Bulk memory is a post-1.0 proposal, so a reintroduced `memory.fill` or
    /// `memory.copy` fails here as well as in the operator walk above — stating
    /// the invariant positively, in terms of what the artifact is rather than
    /// what it lacks.
    fn validate_as_wasm_1_0(wasm: &[u8], label: &str) {
        use wasmparser::{Validator, WasmFeatures};

        Validator::new_with_features(WasmFeatures::WASM1)
            .validate_all(wasm)
            .unwrap_or_else(|e| panic!("{label} does not validate as WebAssembly 1.0: {e}"));
    }

    /// [`validate_as_wasm_1_0`] with the bulk-memory proposal added — the exact
    /// level a `bulk-memory` build claims to target.
    ///
    /// Stating the opt-in positively is what makes the family's goldens more than
    /// a byte record: the same unmodified upstream validator that rejects them at
    /// Wasm 1.0 must accept them here, so an artifact carrying some *other*
    /// post-MVP instruction that crept in fails rather than passing as "not 1.0,
    /// therefore fine".
    fn validate_with_bulk_memory(wasm: &[u8], label: &str) {
        use wasmparser::{Validator, WasmFeatures};

        Validator::new_with_features(WasmFeatures::WASM1.union(WasmFeatures::BULK_MEMORY))
            .validate_all(wasm)
            .unwrap_or_else(|e| {
                panic!("{label} does not validate as WebAssembly 1.0 + bulk memory: {e}")
            });
    }

    /// Every executable golden validates at the WebAssembly 1.0 feature level.
    ///
    /// Modules carrying the compiler's custom verification opcodes are excluded:
    /// those opcodes are not WebAssembly at all and are only ever consumed by the
    /// proof toolchain.
    #[test]
    fn compile_mode_goldens_validate_as_wasm_1_0() {
        let artifacts = golden_wasm_artifacts();
        let mut validated = 0usize;
        for path in &artifacts {
            let wasm = std::fs::read(path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            if contains_verification_operator(&wasm) {
                continue;
            }
            validate_as_wasm_1_0(&wasm, &path.display().to_string());
            validated += 1;
        }
        assert!(
            validated >= 100,
            "expected at least 100 executable goldens to validate as Wasm 1.0, only {validated} did"
        );
    }

    /// The freshly generated module for every single-file fixture validates as
    /// WebAssembly 1.0 too, so the invariant holds for what the compiler emits
    /// now and not only for what was checked in.
    #[test]
    fn compile_mode_corpus_validates_as_wasm_1_0() {
        let sources = single_file_corpus_sources();
        let mut validated = 0usize;
        for (label, source) in &sources {
            let Ok(output) = codegen_with_full_config_no_analysis(
                source,
                Target::Wasm32,
                CompilationMode::Compile,
                Target::Wasm32.default_opt_level(),
            ) else {
                continue;
            };
            if contains_verification_operator(output.wasm()) {
                continue;
            }
            validate_as_wasm_1_0(output.wasm(), label);
            validated += 1;
        }
        assert!(
            validated >= 100,
            "expected at least 100 fixtures to compile and validate as Wasm 1.0, only {validated} did"
        );
    }

    // Opt-in feature family invariants ---

    /// The two partitions together are the whole corpus, and they do not overlap.
    ///
    /// Total cover is structural today — one partition is the walk and the other
    /// is the walk minus a subtree — and this test is what keeps it that way. The
    /// arrangement that would break it is the natural one to reach for as opt-in
    /// families multiply: redefining either side as an explicit list of roots, or
    /// giving one side a skip rule the other lacks. Either turns the subtraction
    /// into two independent walks, and the first golden that falls outside both is
    /// then gated by nothing — the Wasm 1.0 tests skip what is not theirs and the
    /// opt-in tests only look inside their own root.
    ///
    /// Comparing merged sorted paths rather than counts states set equality, so a
    /// golden that swapped sides is caught too; the floors keep a collector that
    /// silently found nothing from passing vacuously.
    #[test]
    fn golden_partitions_cover_every_artifact() {
        let all = all_golden_wasm_artifacts();
        let default_level = golden_wasm_artifacts();
        let opt_in = bulk_memory_golden_artifacts();

        assert!(
            all.len() >= 180,
            "expected the corpus to hold at least 180 golden modules across both \
             partitions, found {}",
            all.len()
        );
        assert!(
            opt_in.len() >= 50,
            "expected the opt-in family to hold at least 50 golden modules, found {}",
            opt_in.len()
        );

        let mut merged: Vec<std::path::PathBuf> =
            default_level.iter().chain(opt_in.iter()).cloned().collect();
        merged.sort();
        assert_eq!(
            merged, all,
            "every golden must belong to exactly one partition; a path missing from \
             the merged list is gated by neither the Wasm 1.0 nor the opt-in tests"
        );
    }

    /// Every artifact in the opt-in family carries a bulk-memory operator.
    #[test]
    fn bulk_memory_goldens_all_carry_a_bulk_memory_operator() {
        let artifacts = bulk_memory_golden_artifacts();
        assert!(
            artifacts.len() >= 50,
            "expected at least 50 opt-in goldens, found {}",
            artifacts.len()
        );
        for path in &artifacts {
            let wasm = std::fs::read(path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            assert_has_bulk_memory_operator(&wasm, &path.display().to_string());
        }
    }

    /// And every executable one validates at exactly Wasm 1.0 plus bulk memory.
    ///
    /// Modules carrying the compiler's custom verification opcodes are excluded
    /// for the same reason as in the Wasm 1.0 gate: those opcodes are not
    /// WebAssembly at any feature level.
    #[test]
    fn bulk_memory_goldens_validate_at_wasm_1_0_plus_bulk_memory() {
        let artifacts = bulk_memory_golden_artifacts();
        let mut validated = 0usize;
        for path in &artifacts {
            let wasm = std::fs::read(path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            if contains_verification_operator(&wasm) {
                continue;
            }
            validate_with_bulk_memory(&wasm, &path.display().to_string());
            validated += 1;
        }
        assert!(
            validated >= 40,
            "expected at least 40 executable opt-in goldens to validate at Wasm 1.0 + \
             bulk memory, only {validated} did"
        );
    }

    // Helper functions ---

    /// Renders a module as WAT, for assertions about a section's shape rather
    /// than its bytes.
    fn wat_of(wasm: &[u8]) -> String {
        wasmprinter::print_bytes(wasm).expect("layout fixtures are printable WebAssembly 1.0")
    }

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
