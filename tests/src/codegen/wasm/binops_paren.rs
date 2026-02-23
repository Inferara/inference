// Parenthesized expression codegen tests.
//
// Verifies that parentheses correctly group subexpressions and that the WASM
// operand evaluation order matches the grouping. Parenthesized expressions
// emit NO extra instructions -- they only affect the order of operand lowering.
//
// Key cases where parentheses change behavior:
// - `(a + b) * c` emits ADD before MUL (vs `a + b * c` which emits MUL before ADD)
// - `a - (b - c)` emits the inner SUB first, then the outer SUB
// - `a / (b / c)` emits the inner DIV first, then the outer DIV

#[cfg(test)]
mod binops_paren_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, get_test_file_path, get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn binops_paren_test() {
        cov_mark::check_count!(wasm_codegen_emit_parenthesized_expression, 26);
        let test_name = "binops_paren";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
    }

    #[test]
    fn binops_paren_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "binops_paren";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        macro_rules! call {
            ($name:expr, $ty:ty, $args:expr, $expected:expr) => {{
                let f: TypedFunc<_, $ty> = instance
                    .get_typed_func(&mut store, $name)
                    .unwrap_or_else(|e| panic!("Failed to get '{}': {e}", $name));
                let result = f
                    .call(&mut store, $args)
                    .unwrap_or_else(|e| panic!("Call to '{}' failed: {e}", $name));
                assert_eq!(result, $expected, "{}({:?}) expected {:?}", $name, $args, $expected);
            }};
        }

        // paren_override_precedence: (2+3)*4 = 20 (not 2+3*4 = 14)
        call!("paren_override_precedence", i32, (2_i32, 3_i32, 4_i32), 20_i32);

        // paren_vs_no_paren_add_mul: 2+(3*4) = 14 (same as without parens)
        call!("paren_vs_no_paren_add_mul", i32, (2_i32, 3_i32, 4_i32), 14_i32);

        // deep_nested_3: ((1+2)+3) = 6
        call!("deep_nested_3", i32, (1_i32, 2_i32, 3_i32), 6_i32);

        // deep_nested_4: (((1+2)+3)+4) = 10
        call!("deep_nested_4", i32, (1_i32, 2_i32, 3_i32, 4_i32), 10_i32);

        // paren_sub_chain: 10-(3-1) = 8 (not (10-3)-1 = 6)
        call!("paren_sub_chain", i32, (10_i32, 3_i32, 1_i32), 8_i32);

        // paren_div_chain: 100/(10/2) = 100/5 = 20 (not (100/10)/2 = 5)
        call!("paren_div_chain", i32, (100_i32, 10_i32, 2_i32), 20_i32);

        // paren_mixed_arith: (3+4)*(5-2) = 7*3 = 21
        call!("paren_mixed_arith", i32, (3_i32, 4_i32, 5_i32, 2_i32), 21_i32);

        // paren_compare_sum: (3+4) > 6 = 7 > 6 = true
        call!("paren_compare_sum", i32, (3_i32, 4_i32, 6_i32), 1_i32);
        // paren_compare_sum: (3+4) > 7 = 7 > 7 = false
        call!("paren_compare_sum", i32, (3_i32, 4_i32, 7_i32), 0_i32);

        // paren_bool_complex: (5 > 3) == (1 < 2) = true == true = true
        call!("paren_bool_complex", i32, (5_i32, 3_i32, 1_i32, 2_i32), 1_i32);
        // paren_bool_complex: (3 > 5) == (1 < 2) = false == true = false
        call!("paren_bool_complex", i32, (3_i32, 5_i32, 1_i32, 2_i32), 0_i32);

        // paren_bitwise_chain: (0xF0 | 0x0F) & 0xFF = 0xFF & 0xFF = 0xFF
        call!("paren_bitwise_chain", i32, (0xF0_i32, 0x0F_i32, 0xFF_i32), 0xFF_i32);
        // paren_bitwise_chain: (0xF0 | 0x0F) & 0x0F = 0xFF & 0x0F = 0x0F
        call!("paren_bitwise_chain", i32, (0xF0_i32, 0x0F_i32, 0x0F_i32), 0x0F_i32);

        // paren_shift_arith: (3+1) << 2 = 4 << 2 = 16
        call!("paren_shift_arith", i32, (3_i32, 2_i32), 16_i32);

        // paren_negated_sum: -(3+4) = -7
        call!("paren_negated_sum", i32, (3_i32, 4_i32), -7_i32);

        // paren_bitnot_masked: (~0) & 0xFF = -1 & 0xFF = 0xFF
        call!("paren_bitnot_masked", i32, (0_i32, 0xFF_i32), 0xFF_i32);
        // paren_bitnot_masked: (~0xFF) & 0xFF = 0xFFFFFF00 & 0xFF = 0
        call!("paren_bitnot_masked", i32, (0xFF_i32, 0xFF_i32), 0_i32);

        // paren_not_cmp: !(5 == 5) = !true = false (0)
        call!("paren_not_cmp", i32, (5_i32, 5_i32), 0_i32);
        // paren_not_cmp: !(5 == 6) = !false = true (1)
        call!("paren_not_cmp", i32, (5_i32, 6_i32), 1_i32);

        // nested_parens_deep: ((((0+1)+1)+1)+1) = 4
        call!("nested_parens_deep", i32, 0_i32, 4_i32);
        // nested_parens_deep: ((((10+1)+1)+1)+1) = 14
        call!("nested_parens_deep", i32, 10_i32, 14_i32);

        // paren_with_let: (2+3)*4 = 20
        call!("paren_with_let", i32, (2_i32, 3_i32, 4_i32), 20_i32);

        // paren_compare_complex: (3+4) >= (2*2) = 7 >= 4 = true
        call!("paren_compare_complex", i32, (3_i32, 4_i32, 2_i32), 1_i32);
        // paren_compare_complex: (1+1) >= (2*2) = 2 >= 4 = false
        call!("paren_compare_complex", i32, (1_i32, 1_i32, 2_i32), 0_i32);
    }
}

/// Test data regeneration helper.
///
/// Regenerates the expected `.wasm` test data file from the current compiler output.
/// Run with `--ignored` flag:
///
/// ```bash
/// cargo test -p inference-tests codegen::wasm::binops_paren::regenerate::regenerate_binops_paren_wasm -- --ignored
/// ```
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, wasm_codegen};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("binops_paren")
            .join("binops_paren")
    }

    #[test]
    #[ignore]
    fn regenerate_binops_paren_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("binops_paren.inf"))
            .expect("Failed to read binops_paren.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("binops_paren.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
    }
}
