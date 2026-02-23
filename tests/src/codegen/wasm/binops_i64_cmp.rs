/// WASM bytecode verification for i64 comparison binary operations.
///
/// Verified instruction dispatch (via analyze-wasm-bytecode):
/// - `eq_i64`              -> i64.eq
/// - `ne_i64`              -> i64.ne
/// - `lt_i64_signed`       -> i64.lt_s   (signed less-than)
/// - `lt_u64`              -> i64.lt_u   (unsigned less-than)
/// - `le_i64_signed`       -> i64.le_s   (signed less-or-equal)
/// - `le_u64`              -> i64.le_u   (unsigned less-or-equal)
/// - `gt_i64_signed`       -> i64.gt_s   (signed greater-than)
/// - `gt_u64`              -> i64.gt_u   (unsigned greater-than)
/// - `ge_i64_signed`       -> i64.ge_s   (signed greater-or-equal)
/// - `ge_u64`              -> i64.ge_u   (unsigned greater-or-equal)
/// - `cmp_chain_i64`       -> i64.lt_s, i64.lt_s, i32.eq  (chained comparison)
/// - `boundary_signed_i64` -> i64.ge_s   (signed comparison against zero)
#[cfg(test)]
mod binops_i64_cmp_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, get_test_file_path, get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn binops_i64_cmp_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 14);
        let test_name = "binops_i64_cmp";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {e}"));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
    }

    #[test]
    fn binops_i64_cmp_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "binops_i64_cmp";
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

        // eq_i64: equality (signedness irrelevant for eq)
        call!("eq_i64", i32, (0_i64, 0_i64), 1_i32);
        call!("eq_i64", i32, (1_i64, 1_i64), 1_i32);
        call!("eq_i64", i32, (-1_i64, -1_i64), 1_i32);
        call!("eq_i64", i32, (i64::MIN, i64::MIN), 1_i32);
        call!("eq_i64", i32, (i64::MAX, i64::MAX), 1_i32);
        call!("eq_i64", i32, (0_i64, 1_i64), 0_i32);
        call!("eq_i64", i32, (i64::MIN, i64::MAX), 0_i32);
        call!("eq_i64", i32, (-1_i64, 0_i64), 0_i32);

        // ne_i64: inequality
        call!("ne_i64", i32, (0_i64, 1_i64), 1_i32);
        call!("ne_i64", i32, (i64::MAX, i64::MIN), 1_i32);
        call!("ne_i64", i32, (-1_i64, 0_i64), 1_i32);
        call!("ne_i64", i32, (0_i64, 0_i64), 0_i32);
        call!("ne_i64", i32, (i64::MAX, i64::MAX), 0_i32);
        call!("ne_i64", i32, (-1_i64, -1_i64), 0_i32);

        // lt_i64_signed: signed less-than
        // Critical: -1 as signed i64 IS less than 0
        call!("lt_i64_signed", i32, (-1_i64, 0_i64), 1_i32);
        call!("lt_i64_signed", i32, (i64::MIN, 0_i64), 1_i32);
        call!("lt_i64_signed", i32, (i64::MIN, i64::MAX), 1_i32);
        call!("lt_i64_signed", i32, (0_i64, 1_i64), 1_i32);
        call!("lt_i64_signed", i32, (0_i64, 0_i64), 0_i32);
        call!("lt_i64_signed", i32, (1_i64, 0_i64), 0_i32);
        call!("lt_i64_signed", i32, (i64::MAX, i64::MIN), 0_i32);

        // lt_u64: unsigned less-than
        // Critical: -1 as u64 is u64::MAX, which is NOT less than 0
        call!("lt_u64", i32, (-1_i64, 0_i64), 0_i32);
        call!("lt_u64", i32, (0_i64, 1_i64), 1_i32);
        call!("lt_u64", i32, (0_i64, -1_i64), 1_i32);
        // unsigned: i64::MIN (0x8000...) > i64::MAX (0x7FFF...), so MIN is NOT < MAX
        call!("lt_u64", i32, (i64::MIN, i64::MAX), 0_i32);
        call!("lt_u64", i32, (i64::MAX, i64::MIN), 1_i32);
        call!("lt_u64", i32, (0_i64, 0_i64), 0_i32);

        // le_i64_signed: signed less-or-equal
        call!("le_i64_signed", i32, (-1_i64, 0_i64), 1_i32);
        call!("le_i64_signed", i32, (0_i64, 0_i64), 1_i32);
        call!("le_i64_signed", i32, (i64::MIN, i64::MIN), 1_i32);
        call!("le_i64_signed", i32, (1_i64, 0_i64), 0_i32);
        call!("le_i64_signed", i32, (i64::MAX, i64::MIN), 0_i32);

        // le_u64: unsigned less-or-equal
        call!("le_u64", i32, (0_i64, 0_i64), 1_i32);
        call!("le_u64", i32, (0_i64, -1_i64), 1_i32);
        call!("le_u64", i32, (-1_i64, -1_i64), 1_i32);
        call!("le_u64", i32, (-1_i64, 0_i64), 0_i32);
        // unsigned: i64::MIN (0x8000...) > i64::MAX (0x7FFF...), so MIN le MAX is false
        call!("le_u64", i32, (i64::MIN, i64::MAX), 0_i32);
        call!("le_u64", i32, (i64::MAX, i64::MIN), 1_i32);

        // gt_i64_signed: signed greater-than
        // Critical: -1 is NOT greater than 0 signed
        call!("gt_i64_signed", i32, (-1_i64, 0_i64), 0_i32);
        call!("gt_i64_signed", i32, (1_i64, 0_i64), 1_i32);
        call!("gt_i64_signed", i32, (i64::MAX, i64::MIN), 1_i32);
        call!("gt_i64_signed", i32, (0_i64, 0_i64), 0_i32);
        call!("gt_i64_signed", i32, (i64::MIN, 0_i64), 0_i32);

        // gt_u64: unsigned greater-than
        // Critical: -1 as u64 is u64::MAX, which IS greater than 0 unsigned
        call!("gt_u64", i32, (-1_i64, 0_i64), 1_i32);
        call!("gt_u64", i32, (0_i64, -1_i64), 0_i32);
        // unsigned: i64::MIN (0x8000...) > i64::MAX (0x7FFF...)
        call!("gt_u64", i32, (i64::MIN, i64::MAX), 1_i32);
        call!("gt_u64", i32, (i64::MAX, i64::MIN), 0_i32);
        call!("gt_u64", i32, (0_i64, 0_i64), 0_i32);
        call!("gt_u64", i32, (1_i64, 0_i64), 1_i32);

        // ge_i64_signed: signed greater-or-equal
        call!("ge_i64_signed", i32, (0_i64, 0_i64), 1_i32);
        call!("ge_i64_signed", i32, (0_i64, -1_i64), 1_i32);
        call!("ge_i64_signed", i32, (i64::MAX, i64::MIN), 1_i32);
        call!("ge_i64_signed", i32, (-1_i64, 0_i64), 0_i32);
        call!("ge_i64_signed", i32, (i64::MIN, i64::MAX), 0_i32);

        // ge_u64: unsigned greater-or-equal
        call!("ge_u64", i32, (-1_i64, 0_i64), 1_i32);
        call!("ge_u64", i32, (-1_i64, -1_i64), 1_i32);
        call!("ge_u64", i32, (0_i64, 0_i64), 1_i32);
        // unsigned: i64::MIN (0x8000...) > i64::MAX (0x7FFF...)
        call!("ge_u64", i32, (i64::MIN, i64::MAX), 1_i32);
        call!("ge_u64", i32, (i64::MAX, i64::MIN), 0_i32);
        call!("ge_u64", i32, (0_i64, -1_i64), 0_i32);

        // cmp_chain_i64: (a < b) == (b < c)
        // Both true: 1 < 2 and 2 < 3 -> true == true -> 1
        call!("cmp_chain_i64", i32, (1_i64, 2_i64, 3_i64), 1_i32);
        // Both false: 3 < 2 and 2 < 1 -> false == false -> 1
        call!("cmp_chain_i64", i32, (3_i64, 2_i64, 1_i64), 1_i32);
        // Mixed: 1 < 2 (true) and 2 < 1 (false) -> true == false -> 0
        call!("cmp_chain_i64", i32, (1_i64, 2_i64, 1_i64), 0_i32);
        // Mixed: 2 < 1 (false) and 1 < 3 (true) -> false == true -> 0
        call!("cmp_chain_i64", i32, (2_i64, 1_i64, 3_i64), 0_i32);
        // Equal values: 5 < 5 (false) and 5 < 5 (false) -> false == false -> 1
        call!("cmp_chain_i64", i32, (5_i64, 5_i64, 5_i64), 1_i32);
        // Negative chain: -3 < -2 (true) and -2 < -1 (true) -> 1
        call!("cmp_chain_i64", i32, (-3_i64, -2_i64, -1_i64), 1_i32);

        // boundary_signed_i64: a >= 0 (signed comparison against zero)
        call!("boundary_signed_i64", i32, 0_i64, 1_i32);
        call!("boundary_signed_i64", i32, 1_i64, 1_i32);
        call!("boundary_signed_i64", i32, i64::MAX, 1_i32);
        call!("boundary_signed_i64", i32, -1_i64, 0_i32);
        call!("boundary_signed_i64", i32, i64::MIN, 0_i32);
        call!("boundary_signed_i64", i32, -100_i64, 0_i32);
    }

    #[test]
    #[ignore]
    fn regenerate_binops_i64_cmp_wasm() {
        let test_name = "binops_i64_cmp";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {e}"));
        let wasm_path = get_test_wasm_path(module_path!(), test_name);
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
    }
}
