//! Codegen validation tests for the Inference compiler.
//!
//! These are Tier 1 tests (no external binaries required) that verify:
//! - `codegen()` produces valid `CodegenOutput` with non-empty IR
//! - IR contains expected content (target triple, function definitions)
//! - Proof mode applies barriers only to spec functions (Decision #32)
//! - Target validation (proof + Soroban rejection, Soroban + non-det rejection)
//! - Size optimization attributes for Soroban target
//! - `has_main` detection

#[cfg(test)]
mod codegen_validation_tests {
    use crate::utils::{
        codegen_ir, codegen_ir_with_mode, codegen_with_full_config, codegen_with_target_mode,
    };
    use inference_wasm_codegen::{CompilationMode, OptLevel, Target};

    // --- IR content tests ---

    #[test]
    fn codegen_returns_nonempty_ir() {
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let output = codegen_ir(source);
        assert!(
            !output.ir().is_empty(),
            "CodegenOutput should contain non-empty IR"
        );
    }

    #[test]
    fn ir_contains_target_triple() {
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let output = codegen_ir(source);
        assert!(
            output
                .ir()
                .contains("target triple = \"wasm32-unknown-unknown\""),
            "IR should contain the wasm32-unknown-unknown target triple.\nIR:\n{}",
            output.ir()
        );
    }

    #[test]
    fn ir_contains_function_definition() {
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let output = codegen_ir(source);
        assert!(
            output.ir().contains("define i32 @hello_world()"),
            "IR should contain 'define i32 @hello_world()' function definition.\nIR:\n{}",
            output.ir()
        );
    }

    // --- Proof mode barrier tests (Decision #32) ---

    #[test]
    fn proof_mode_barriers_only_on_nondet_functions() {
        // Source with both a regular function and a non-det function (uzumaki).
        let source = r#"
            pub fn regular() -> i32 { return 42; }
            pub fn with_nondet() -> i32 { return @; }
        "#;
        let output = codegen_ir_with_mode(source, CompilationMode::Proof);
        let ir = output.ir();

        // Parse per-function attributes from the IR.
        // In LLVM IR, function definitions look like:
        //   define i32 @regular() #0 { ... }
        //   define i32 @with_nondet() #1 { ... }
        // And attribute groups look like:
        //   attributes #0 = { ... }
        //   attributes #1 = { optnone noinline ... }

        // with_nondet() should have optnone (it contains non-det operations)
        // regular() should NOT have optnone

        // Strategy: find the attribute group number for each function,
        // then check if that group contains "optnone"
        let regular_has_optnone = function_has_attribute(ir, "regular", "optnone");
        let nondet_has_optnone = function_has_attribute(ir, "with_nondet", "optnone");

        assert!(
            !regular_has_optnone,
            "regular() should NOT have optnone in proof mode (Decision #32).\nIR:\n{}",
            ir
        );
        assert!(
            nondet_has_optnone,
            "with_nondet() SHOULD have optnone in proof mode.\nIR:\n{}",
            ir
        );
    }

    #[test]
    fn proof_mode_without_nondet_matches_compile_mode() {
        // When source has no non_det_operations, proof mode output should use
        // the same optimization as compile mode (Decision #32).
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let compile_output =
            codegen_with_target_mode(source, Target::Wasm32, CompilationMode::Compile).unwrap();
        let proof_output =
            codegen_with_target_mode(source, Target::Wasm32, CompilationMode::Proof).unwrap();

        // Both should use O3 optimization level
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

    // --- Size optimization attribute tests ---

    #[test]
    fn soroban_ir_has_size_optimization_attrs() {
        // Soroban uses Oz which adds minsize+optsize IR attributes
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let output =
            codegen_with_target_mode(source, Target::Soroban, CompilationMode::Compile).unwrap();
        let ir = output.ir();
        assert!(
            ir.contains("optsize"),
            "Soroban IR should contain optsize attribute.\nIR:\n{}",
            ir
        );
        assert!(
            ir.contains("minsize"),
            "Soroban IR should contain minsize attribute.\nIR:\n{}",
            ir
        );
    }

    // --- Compile mode non-det tests ---

    #[test]
    fn compile_mode_with_nondet_compiles_functions_and_preserves_intrinsics() {
        // In compile mode, regular functions with non-det operations are compiled.
        // Non-det functions receive optnone+noinline to prevent LLVM from optimizing
        // away the custom 0xfc intrinsic calls. Regular (non-non-det) functions do NOT
        // receive these barriers. Spec definitions (top-level `spec { }` blocks) are
        // excluded from codegen because traverse_t_ast_with_compiler() only iterates
        // function_definitions().
        let source = r#"
            pub fn with_uzumaki() -> i32 { return @; }
            pub fn regular() -> i32 { return 42; }
        "#;
        let output =
            codegen_with_target_mode(source, Target::Wasm32, CompilationMode::Compile).unwrap();
        let ir = output.ir();

        // Non-det intrinsics should be present (uzumaki is in a regular function)
        assert!(
            ir.contains("llvm.wasm.uzumaki.i32"),
            "Compile mode IR should contain uzumaki intrinsic for non-det function.\nIR:\n{}",
            ir
        );

        // Both functions should be defined
        assert!(
            ir.contains("@with_uzumaki("),
            "Compile mode IR should contain with_uzumaki function.\nIR:\n{}",
            ir
        );
        assert!(
            ir.contains("@regular("),
            "Compile mode IR should contain regular function.\nIR:\n{}",
            ir
        );

        // Non-det function has optnone (preserves intrinsic calls from optimization)
        let uzumaki_has_optnone = function_has_attribute(ir, "with_uzumaki", "optnone");
        assert!(
            uzumaki_has_optnone,
            "with_uzumaki() SHOULD have optnone to preserve intrinsic calls.\nIR:\n{}",
            ir
        );

        // Regular function does NOT have optnone
        let regular_has_optnone = function_has_attribute(ir, "regular", "optnone");
        assert!(
            !regular_has_optnone,
            "regular() should NOT have optnone in compile mode.\nIR:\n{}",
            ir
        );
    }

    // --- Proof mode intrinsic tests ---

    #[test]
    fn proof_mode_ir_contains_expected_intrinsic_calls() {
        // In proof mode, non-det functions produce IR with the expected LLVM intrinsics.
        let source = r#"
            pub fn with_uzumaki() -> i32 { return @; }
            pub fn with_forall() { forall { const a: i32 = 42; } }
        "#;
        let output = codegen_ir_with_mode(source, CompilationMode::Proof);
        let ir = output.ir();

        // Uzumaki intrinsic should be declared
        assert!(
            ir.contains("llvm.wasm.uzumaki.i32"),
            "Proof mode IR should contain uzumaki intrinsic.\nIR:\n{}",
            ir
        );

        // Forall block intrinsics should be declared
        assert!(
            ir.contains("llvm.wasm.forall.start"),
            "Proof mode IR should contain forall.start intrinsic.\nIR:\n{}",
            ir
        );
        assert!(
            ir.contains("llvm.wasm.forall.end"),
            "Proof mode IR should contain forall.end intrinsic.\nIR:\n{}",
            ir
        );
    }

    // --- has_main detection tests ---

    #[test]
    fn has_main_true_for_public_main() {
        let source = "pub fn main() -> i32 { return 0; }";
        let output = codegen_ir(source);
        assert!(
            output.has_main(),
            "has_main should be true when pub fn main() exists"
        );
    }

    #[test]
    fn has_main_false_without_main() {
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let output = codegen_ir(source);
        assert!(
            !output.has_main(),
            "has_main should be false when no main function exists"
        );
    }

    // --- Size optimization attribute tests (compiler.rs coverage) ---

    #[test]
    fn codegen_with_o0_skips_size_attrs() {
        // O0 is not size-optimized, so neither optsize nor minsize should appear
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let output = codegen_with_full_config(
            source,
            Target::Wasm32,
            CompilationMode::Compile,
            OptLevel::O0,
        )
        .unwrap();
        let ir = output.ir();
        assert!(
            !ir.contains("optsize"),
            "O0 should NOT add optsize attribute.\nIR:\n{}",
            ir
        );
        assert!(
            !ir.contains("minsize"),
            "O0 should NOT add minsize attribute.\nIR:\n{}",
            ir
        );
    }

    #[test]
    fn codegen_with_o3_skips_size_attrs() {
        // O3 is not size-optimized, so neither optsize nor minsize should appear
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let output = codegen_with_full_config(
            source,
            Target::Wasm32,
            CompilationMode::Compile,
            OptLevel::O3,
        )
        .unwrap();
        let ir = output.ir();
        assert!(
            !ir.contains("optsize"),
            "O3 should NOT add optsize attribute.\nIR:\n{}",
            ir
        );
        assert!(
            !ir.contains("minsize"),
            "O3 should NOT add minsize attribute.\nIR:\n{}",
            ir
        );
    }

    #[test]
    fn codegen_with_os_adds_optsize_only() {
        // Os adds optsize but NOT minsize
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let output = codegen_with_full_config(
            source,
            Target::Wasm32,
            CompilationMode::Compile,
            OptLevel::Os,
        )
        .unwrap();
        let ir = output.ir();
        assert!(
            ir.contains("optsize"),
            "Os should add optsize attribute.\nIR:\n{}",
            ir
        );
        assert!(
            !ir.contains("minsize"),
            "Os should NOT add minsize attribute.\nIR:\n{}",
            ir
        );
    }

    #[test]
    fn codegen_with_oz_adds_optsize_and_minsize() {
        // Oz adds both optsize and minsize
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let output = codegen_with_full_config(
            source,
            Target::Wasm32,
            CompilationMode::Compile,
            OptLevel::Oz,
        )
        .unwrap();
        let ir = output.ir();
        assert!(
            ir.contains("optsize"),
            "Oz should add optsize attribute.\nIR:\n{}",
            ir
        );
        assert!(
            ir.contains("minsize"),
            "Oz should add minsize attribute.\nIR:\n{}",
            ir
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
        assert_eq!(output.target_triple(), "wasm32-unknown-unknown");
    }

    #[test]
    fn codegen_soroban_compile_succeeds() {
        // Happy path: Soroban compile mode with no non-det operations
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let output =
            codegen_with_target_mode(source, Target::Soroban, CompilationMode::Compile).unwrap();

        assert_eq!(output.target(), Target::Soroban);
        assert_eq!(output.mode(), CompilationMode::Compile);
        assert_eq!(output.opt_level(), OptLevel::Oz);
        assert!(!output.ir().is_empty());
        assert!(output.ir().contains("define i32 @hello_world()"));
    }

    #[test]
    fn codegen_proof_wasm32_succeeds() {
        // Happy path: Proof mode with Wasm32 target
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let output =
            codegen_with_target_mode(source, Target::Wasm32, CompilationMode::Proof).unwrap();

        assert_eq!(output.target(), Target::Wasm32);
        assert_eq!(output.mode(), CompilationMode::Proof);
        assert_eq!(output.opt_level(), OptLevel::O3);
        assert!(!output.ir().is_empty());
    }

    // --- emit_ir target triple test ---

    #[test]
    fn emit_ir_contains_target_triple() {
        // Verify that emit_ir sets the target triple on the module
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let output = codegen_with_full_config(
            source,
            Target::Soroban,
            CompilationMode::Compile,
            OptLevel::Oz,
        )
        .unwrap();
        assert!(
            output
                .ir()
                .contains("target triple = \"wasm32-unknown-unknown\""),
            "Soroban IR should also contain the wasm32-unknown-unknown target triple.\nIR:\n{}",
            output.ir()
        );
    }

    // --- has_main detection edge cases ---

    #[test]
    fn has_main_false_for_private_main() {
        // fn main() (without pub) should NOT set has_main
        let source = "fn main() -> i32 { return 0; }";
        let output = codegen_ir(source);
        assert!(
            !output.has_main(),
            "has_main should be false for private fn main()"
        );
    }

    // --- Helper functions ---

    /// Checks if a function in LLVM IR has a specific attribute.
    ///
    /// Parses the IR text to find the function definition, extracts its attribute
    /// group number, then checks if that attribute group contains the target attribute.
    fn function_has_attribute(ir: &str, function_name: &str, attribute: &str) -> bool {
        // Find the function definition line
        let func_pattern = format!("@{function_name}(");
        for line in ir.lines() {
            if line.contains(&func_pattern) && line.contains("define") {
                // Extract attribute group number (e.g., "#0" from "define i32 @func() #0 {")
                if let Some(attr_group) = extract_attribute_group(line) {
                    // Find the attribute group definition
                    let group_pattern = format!("attributes {attr_group} = ");
                    for attr_line in ir.lines() {
                        if attr_line.starts_with(&group_pattern) {
                            return attr_line.contains(attribute);
                        }
                    }
                }
                // If no attribute group, check inline attributes
                return line.contains(attribute);
            }
        }
        false
    }

    /// Extracts the attribute group identifier (e.g., "#0") from a function definition line.
    fn extract_attribute_group(line: &str) -> Option<String> {
        // Function definitions end like: ") #0 {" or ") #1 {"
        // Find the last '#' before '{'
        if let Some(brace_pos) = line.rfind('{') {
            let before_brace = line[..brace_pos].trim();
            if let Some(hash_pos) = before_brace.rfind('#') {
                let group = before_brace[hash_pos..].trim();
                if group.starts_with('#') {
                    return Some(group.to_string());
                }
            }
        }
        None
    }
}
