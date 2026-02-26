// Recursive math algorithm tests: classic recursive algorithms exercising recursion,
// if/else, function calls, arithmetic, comparisons, negation, let variables, and
// parenthesized expressions.
//
// WASM bytecode analysis:
//
// factorial(n) -> n * factorial(n - 1)
//   local.get $n, i32.const 1, i32.le_s, if, i32.const 1, return, end,
//   local.get $n, local.get $n, i32.const 1, i32.sub, call $factorial, i32.mul,
//   return, unreachable
//   (3 binary ops: le_s, sub, mul; 1 recursive call; 1 if branch)
//
// power(base, exp) -> exponentiation by squaring
//   local.get $exp, i32.const 0, i32.le_s, if, i32.const 1, return, end,
//   local.get $exp, i32.const 2, i32.rem_s, i32.const 0, i32.eq, if,
//   local.get $base, local.get $exp, i32.const 2, i32.div_s, call $power,
//   local.set $half, local.get $half, local.get $half, i32.mul, return, end,
//   local.get $base, local.get $base, local.get $exp, i32.const 1, i32.sub,
//   call $power, i32.mul, return, unreachable
//   (7 binary ops: le_s, rem_s, eq, div_s, mul, sub, mul; 2 recursive calls; 2 if branches; 1 local)
//
// abs_i32(x) -> negation via (0 - x)
//   local.get $x, i32.const 0, i32.lt_s, if, i32.const 0, local.get $x, i32.sub,
//   return, end, local.get $x, return, unreachable
//   (2 ops: lt_s, sub; negation lowered as 0-x)
//
// Total across all 7 functions: 25 binary expressions, 9 if statements, 11 function calls,
// 3 variable definitions, 2 parenthesized expressions, 1 prefix unary negation, 9 function params.

#[cfg(test)]
mod algo_recursive_math_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn algo_recursive_math_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 25);
        cov_mark::check_count!(wasm_codegen_emit_if_statement, 9);
        cov_mark::check_count!(wasm_codegen_emit_function_call, 11);
        cov_mark::check_count!(wasm_codegen_emit_variable_definition, 3);
        cov_mark::check_count!(wasm_codegen_emit_parenthesized_expression, 2);
        cov_mark::check_count!(wasm_codegen_emit_prefix_unary_expression, 1);
        cov_mark::check_count!(wasm_codegen_emit_unary_neg, 1);
        cov_mark::check_count!(wasm_codegen_emit_function_params, 9);
        let test_name = "algo_recursive_math";
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
    fn algo_recursive_math_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "algo_recursive_math";
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

        // --- factorial ---
        call!("factorial", i32, 0_i32, 1_i32);
        call!("factorial", i32, 1_i32, 1_i32);
        call!("factorial", i32, 5_i32, 120_i32);
        call!("factorial", i32, 10_i32, 3628800_i32);

        // --- fibonacci ---
        call!("fibonacci", i32, 0_i32, 0_i32);
        call!("fibonacci", i32, 1_i32, 1_i32);
        call!("fibonacci", i32, 2_i32, 1_i32);
        call!("fibonacci", i32, 10_i32, 55_i32);

        // --- gcd ---
        call!("gcd", i32, (12_i32, 8_i32), 4_i32);
        call!("gcd", i32, (17_i32, 13_i32), 1_i32);
        call!("gcd", i32, (100_i32, 0_i32), 100_i32);
        call!("gcd", i32, (0_i32, 5_i32), 5_i32);
        call!("gcd", i32, (-12_i32, 8_i32), 4_i32);
        call!("gcd", i32, (48_i32, 18_i32), 6_i32);

        // --- power (exponentiation by squaring) ---
        call!("power", i32, (2_i32, 0_i32), 1_i32);
        call!("power", i32, (2_i32, 10_i32), 1024_i32);
        call!("power", i32, (3_i32, 5_i32), 243_i32);
        call!("power", i32, (5_i32, 3_i32), 125_i32);
        call!("power", i32, (1_i32, 100_i32), 1_i32);

        // --- digit_sum ---
        call!("digit_sum", i32, 0_i32, 0_i32);
        call!("digit_sum", i32, 123_i32, 6_i32);
        call!("digit_sum", i32, 9999_i32, 36_i32);
        call!("digit_sum", i32, -42_i32, 6_i32);
        call!("digit_sum", i32, 7_i32, 7_i32);

        // --- digit_count ---
        call!("digit_count", i32, 0_i32, 1_i32);
        call!("digit_count", i32, 9_i32, 1_i32);
        call!("digit_count", i32, 99_i32, 2_i32);
        call!("digit_count", i32, 12345_i32, 5_i32);
        call!("digit_count", i32, -999_i32, 3_i32);
    }
}

/// Test data regeneration helper.
///
/// Regenerates the expected `.wasm` golden file from the current compiler output.
/// Run with `--ignored` flag:
///
/// ```bash
/// cargo test -p inference-tests codegen::wasm::algo_recursive_math::regenerate -- --ignored
/// ```
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("algo_recursive_math")
    }

    #[test]
    #[ignore]
    fn regenerate_algo_recursive_math_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("algo_recursive_math.inf"))
            .expect("Failed to read algo_recursive_math.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("algo_recursive_math.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "algo_recursive_math");
    }
}
