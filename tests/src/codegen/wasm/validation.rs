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
    use crate::utils::{codegen_ir, codegen_ir_with_mode, codegen_with_target_mode};
    use inference_wasm_codegen::{CompilationMode, Target};

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
        let compile_output = codegen_with_target_mode(
            source,
            Target::Wasm32,
            CompilationMode::Compile,
        )
        .unwrap();
        let proof_output = codegen_with_target_mode(
            source,
            Target::Wasm32,
            CompilationMode::Proof,
        )
        .unwrap();

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
        assert!(result.is_err(), "Proof mode with Soroban should be rejected");
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
        let output = codegen_with_target_mode(source, Target::Soroban, CompilationMode::Compile)
            .unwrap();
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
