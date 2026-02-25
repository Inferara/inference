/// WASM bytecode verification for i64 arithmetic binary operations.
///
/// Instruction dispatch verified via execution semantics:
/// - `add_i64`         -> i64.add
/// - `sub_i64`         -> i64.sub
/// - `mul_i64`         -> i64.mul
/// - `div_i64_signed`  -> i64.div_s  (signed: -10/3 = -3, truncates toward zero)
/// - `div_u64`         -> i64.div_u  (unsigned: u64::MAX/2 = i64::MAX)
/// - `mod_i64_signed`  -> i64.rem_s  (signed: -10%3 = -1, sign follows dividend)
/// - `mod_u64`         -> i64.rem_u  (unsigned: u64::MAX%3 = 0)
/// - `sub_chain_i64`   -> i64.sub, i64.sub  (left-associative)
/// - `mul_add_i64`     -> i64.mul, i64.add  (precedence: mul before add)
#[cfg(test)]
mod binops_i64_arith_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, regenerate_wat, wasm_codegen,
    };

    #[test]
    fn binops_i64_arith_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 14);
        let test_name = "binops_i64_arith";
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
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn binops_i64_arith_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "binops_i64_arith";
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

        // add_i64
        call!("add_i64", i64, (100_i64, 200_i64), 300_i64);
        call!("add_i64", i64, (0_i64, 0_i64), 0_i64);
        call!("add_i64", i64, (-1_i64, 1_i64), 0_i64);
        call!("add_i64", i64, (i64::MAX, 0_i64), i64::MAX);
        call!("add_i64", i64, (i64::MIN, 0_i64), i64::MIN);

        // sub_i64
        call!("sub_i64", i64, (1000_i64, 999_i64), 1_i64);
        call!("sub_i64", i64, (0_i64, 1_i64), -1_i64);
        call!("sub_i64", i64, (0_i64, 0_i64), 0_i64);
        call!("sub_i64", i64, (i64::MIN, 0_i64), i64::MIN);
        call!("sub_i64", i64, (-100_i64, -200_i64), 100_i64);

        // mul_i64
        call!("mul_i64", i64, (1_000_000_i64, 1_000_000_i64), 1_000_000_000_000_i64);
        call!("mul_i64", i64, (0_i64, i64::MAX), 0_i64);
        call!("mul_i64", i64, (1_i64, i64::MAX), i64::MAX);
        call!("mul_i64", i64, (-1_i64, 1_i64), -1_i64);
        call!("mul_i64", i64, (2_i64, -3_i64), -6_i64);

        // div_i64_signed: truncates toward zero
        call!("div_i64_signed", i64, (-10_i64, 3_i64), -3_i64);
        call!("div_i64_signed", i64, (10_i64, 3_i64), 3_i64);
        call!("div_i64_signed", i64, (100_i64, 10_i64), 10_i64);
        call!("div_i64_signed", i64, (7_i64, -2_i64), -3_i64);
        call!("div_i64_signed", i64, (-7_i64, -2_i64), 3_i64);
        call!("div_i64_signed", i64, (0_i64, 1_i64), 0_i64);

        // div_u64: unsigned division (u64::MAX = 18446744073709551615)
        // u64::MAX as i64 = -1, so -1_i64 / 2_i64 unsigned = 9223372036854775807
        call!("div_u64", i64, (-1_i64, 2_i64), i64::MAX);
        call!("div_u64", i64, (10_i64, 3_i64), 3_i64);
        call!("div_u64", i64, (0_i64, 1_i64), 0_i64);

        // mod_i64_signed: sign of result follows dividend
        call!("mod_i64_signed", i64, (-10_i64, 3_i64), -1_i64);
        call!("mod_i64_signed", i64, (10_i64, 3_i64), 1_i64);
        call!("mod_i64_signed", i64, (10_i64, -3_i64), 1_i64);
        call!("mod_i64_signed", i64, (-10_i64, -3_i64), -1_i64);
        call!("mod_i64_signed", i64, (0_i64, 5_i64), 0_i64);
        call!("mod_i64_signed", i64, (7_i64, 7_i64), 0_i64);

        // mod_u64: unsigned remainder
        // u64::MAX % 3 = 18446744073709551615 % 3 = 0
        call!("mod_u64", i64, (-1_i64, 3_i64), 0_i64);
        call!("mod_u64", i64, (10_i64, 3_i64), 1_i64);
        call!("mod_u64", i64, (0_i64, 1_i64), 0_i64);

        // add_i64_with_let: let binding
        call!("add_i64_with_let", i64, (100_i64, 200_i64), 300_i64);
        call!("add_i64_with_let", i64, (-50_i64, 50_i64), 0_i64);

        // sub_chain_i64: left-associative (a - b) - c
        call!("sub_chain_i64", i64, (100_i64, 30_i64, 20_i64), 50_i64);
        call!("sub_chain_i64", i64, (0_i64, 0_i64, 0_i64), 0_i64);
        call!("sub_chain_i64", i64, (10_i64, 20_i64, 30_i64), -40_i64);

        // mul_add_i64: a * b + c (mul has higher precedence)
        call!("mul_add_i64", i64, (2_i64, 3_i64, 4_i64), 10_i64);
        call!("mul_add_i64", i64, (0_i64, 100_i64, 5_i64), 5_i64);
        call!("mul_add_i64", i64, (-1_i64, 10_i64, 10_i64), 0_i64);

        // add_max_i64: 9223372036854775806 + 1 = i64::MAX
        {
            let f: TypedFunc<(), i64> = instance
                .get_typed_func(&mut store, "add_max_i64")
                .unwrap_or_else(|e| panic!("Failed to get 'add_max_i64': {e}"));
            let result = f
                .call(&mut store, ())
                .unwrap_or_else(|e| panic!("Call to 'add_max_i64' failed: {e}"));
            assert_eq!(result, i64::MAX, "add_max_i64() expected i64::MAX");
        }

        // sub_neg_result_i64
        call!("sub_neg_result_i64", i64, (5_i64, 10_i64), -5_i64);
        call!("sub_neg_result_i64", i64, (0_i64, i64::MAX), -i64::MAX);
    }

    #[test]
    #[ignore]
    fn regenerate_binops_i64_arith_wasm() {
        let test_name = "binops_i64_arith";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {e}"));
        let wasm_path = get_test_wasm_path(module_path!(), test_name);
        let dir = wasm_path.parent().unwrap();
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, dir, test_name);
    }
}
