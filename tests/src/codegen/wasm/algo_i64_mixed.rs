/// WASM bytecode analysis for mixed-type i64 algorithms.
///
/// The arithmetic functions are iterative (a leading guard, `mut` accumulators, a
/// conditional `loop`, and a single trailing return) to comply with A035, which
/// forbids the recursion the earlier versions relied on.
///
/// Key instruction patterns:
/// - `factorial_i64`:   i64.le_s guard, conditional loop with i64.mul accumulation
/// - `fibonacci_i64`:   i64.le_s/i64.eq guards, conditional loop advancing a two-term window
/// - `gcd_i64`:         i64.lt_s sign normalisation (i64.const 0, i64.sub), conditional loop with i64.rem_s
/// - `lcm_i64`:         i64.eq (zero checks), call gcd_i64, i64.div_s + i64.mul
/// - `is_even`/`is_odd`: i32.and (bitwise mask), i32.eq (compare to 0 or 1)
/// - `abs_i64`:         i64.lt_s (sign check), prefix unary neg (i64.const 0, i64.sub)
/// - `sum_range_i64`:   let mut + assignment (local.set), nested if (i64.le_s), i64.add accumulation
///
/// i64 literals require typed local variables (`let zero: i64 = 0;`) because the type checker
/// does not auto-promote bare integer literals from i32 to i64 in expression context.
#[cfg(test)]
mod algo_i64_mixed_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn algo_i64_mixed_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 39);
        cov_mark::check_count!(wasm_codegen_emit_if_statement, 14);
        cov_mark::check_count!(wasm_codegen_emit_function_call, 1);
        cov_mark::check_count!(wasm_codegen_emit_variable_definition, 20);
        cov_mark::check_count!(wasm_codegen_emit_parenthesized_expression, 3);
        cov_mark::check_count!(wasm_codegen_emit_prefix_unary_expression, 1);
        cov_mark::check_count!(wasm_codegen_emit_assign_identifier, 18);
        cov_mark::check_count!(wasm_codegen_emit_function_params, 10);
        let test_name = "algo_i64_mixed";
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
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn algo_i64_mixed_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "algo_i64_mixed";
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

        // --- factorial_i64 ---
        call!("factorial_i64", i64, 0_i64, 1_i64);
        call!("factorial_i64", i64, 1_i64, 1_i64);
        call!("factorial_i64", i64, 5_i64, 120_i64);
        call!("factorial_i64", i64, 10_i64, 3628800_i64);
        call!("factorial_i64", i64, 20_i64, 2432902008176640000_i64);

        // --- fibonacci_i64 ---
        call!("fibonacci_i64", i64, 0_i64, 0_i64);
        call!("fibonacci_i64", i64, 1_i64, 1_i64);
        call!("fibonacci_i64", i64, 10_i64, 55_i64);

        // --- gcd_i64 ---
        call!("gcd_i64", i64, (12_i64, 8_i64), 4_i64);
        call!("gcd_i64", i64, (17_i64, 13_i64), 1_i64);
        call!("gcd_i64", i64, (100_i64, 0_i64), 100_i64);
        call!("gcd_i64", i64, (-12_i64, 8_i64), 4_i64);

        // --- lcm_i64 ---
        call!("lcm_i64", i64, (4_i64, 6_i64), 12_i64);
        call!("lcm_i64", i64, (0_i64, 5_i64), 0_i64);
        call!("lcm_i64", i64, (7_i64, 13_i64), 91_i64);
        call!("lcm_i64", i64, (12_i64, 18_i64), 36_i64);

        // --- is_even / is_odd ---
        call!("is_even", i32, 0_i32, 1_i32);
        call!("is_even", i32, 1_i32, 0_i32);
        call!("is_even", i32, 42_i32, 1_i32);
        call!("is_even", i32, -3_i32, 0_i32);

        call!("is_odd", i32, 0_i32, 0_i32);
        call!("is_odd", i32, 1_i32, 1_i32);
        call!("is_odd", i32, 42_i32, 0_i32);

        // --- sum_range_i64 (manual unroll, sums 1..=n for n<=5) ---
        call!("sum_range_i64", i64, 0_i64, 0_i64);
        call!("sum_range_i64", i64, 1_i64, 1_i64);
        call!("sum_range_i64", i64, 3_i64, 6_i64);
        call!("sum_range_i64", i64, 5_i64, 15_i64);
    }

}

/// Test data regeneration helper.
///
/// Regenerates the expected `.wasm` golden file from the current compiler output.
/// Run with `--ignored` flag:
///
/// ```bash
/// cargo test -p inference-tests codegen::wasm::algo_i64_mixed::regenerate -- --ignored
/// ```
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("algo_i64_mixed")
    }

    #[test]
    #[ignore]
    fn regenerate_algo_i64_mixed_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("algo_i64_mixed.inf"))
            .expect("Failed to read algo_i64_mixed.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("algo_i64_mixed.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "algo_i64_mixed");
    }
}
