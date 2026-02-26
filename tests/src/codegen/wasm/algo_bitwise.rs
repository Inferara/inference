// Bitwise algorithm tests: bit manipulation algorithms exercising bitwise ops, bitnot,
// negation, recursion, if/else, let variables, bool return type, parenthesized expressions,
// masks and constants.
//
// WASM bytecode analysis:
//
// clear_bit(x, pos) -> x & (~(1 << pos))
//   local.get $x, i32.const 1, local.get $pos, i32.shl,
//   i32.const -1, i32.xor, i32.and, return
//   (~expr generates: [expr, i32.const -1, i32.xor])
//
// lowest_set_bit(n) -> n & (-n)
//   local.get $n, i32.const 0, local.get $n, i32.sub, i32.and, return
//   (-n generates: [i32.const 0, local.get $n, i32.sub])
//
// popcount_helper(n, acc) -> recursive bit counting via n & (n-1)
//   local.get $n, i32.const 0, i32.eq, if, local.get $acc, return, end,
//   local.get $n, local.get $n, i32.const 1, i32.sub, i32.and,
//   local.get $acc, i32.const 1, i32.add, call $popcount_helper, return
//
// Total across all 12 functions: 35 binary expressions, 17 parenthesized expressions,
// 5 if statements, 4 function calls, 3 variable definitions, 2 prefix unary expressions
// (1 neg, 1 bitnot), 20 function params.

#[cfg(test)]
mod algo_bitwise_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn algo_bitwise_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 35);
        cov_mark::check_count!(wasm_codegen_emit_if_statement, 5);
        cov_mark::check_count!(wasm_codegen_emit_function_call, 4);
        cov_mark::check_count!(wasm_codegen_emit_variable_definition, 3);
        cov_mark::check_count!(wasm_codegen_emit_parenthesized_expression, 17);
        cov_mark::check_count!(wasm_codegen_emit_prefix_unary_expression, 2);
        cov_mark::check_count!(wasm_codegen_emit_unary_neg, 1);
        cov_mark::check_count!(wasm_codegen_emit_unary_bitnot, 1);
        cov_mark::check_count!(wasm_codegen_emit_function_params, 20);
        let test_name = "algo_bitwise";
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
    fn algo_bitwise_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "algo_bitwise";
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

        // --- popcount ---
        call!("popcount", i32, 0_i32, 0_i32);
        call!("popcount", i32, 1_i32, 1_i32);
        call!("popcount", i32, 7_i32, 3_i32);
        call!("popcount", i32, 255_i32, 8_i32);
        call!("popcount", i32, 0x5555_i32, 8_i32);

        // --- is_power_of_2 ---
        call!("is_power_of_2", i32, 0_i32, 0_i32);
        call!("is_power_of_2", i32, 1_i32, 1_i32);
        call!("is_power_of_2", i32, 2_i32, 1_i32);
        call!("is_power_of_2", i32, 3_i32, 0_i32);
        call!("is_power_of_2", i32, 64_i32, 1_i32);
        call!("is_power_of_2", i32, -1_i32, 0_i32);

        // --- get_bit ---
        call!("get_bit", i32, (0b1010_i32, 0_i32), 0_i32);
        call!("get_bit", i32, (0b1010_i32, 1_i32), 1_i32);
        call!("get_bit", i32, (0b1010_i32, 3_i32), 1_i32);

        // --- set_bit ---
        call!("set_bit", i32, (0_i32, 0_i32), 1_i32);
        call!("set_bit", i32, (0_i32, 3_i32), 8_i32);
        call!("set_bit", i32, (0b1010_i32, 0_i32), 0b1011_i32);

        // --- clear_bit ---
        call!("clear_bit", i32, (0b1111_i32, 0_i32), 0b1110_i32);
        call!("clear_bit", i32, (0b1111_i32, 2_i32), 0b1011_i32);

        // --- toggle_bit ---
        call!("toggle_bit", i32, (0b1010_i32, 0_i32), 0b1011_i32);
        call!("toggle_bit", i32, (0b1010_i32, 1_i32), 0b1000_i32);

        // --- lowest_set_bit ---
        call!("lowest_set_bit", i32, 12_i32, 4_i32);
        call!("lowest_set_bit", i32, 1_i32, 1_i32);
        call!("lowest_set_bit", i32, 0_i32, 0_i32);

        // --- rotate_left_8 ---
        call!("rotate_left_8", i32, (0b00000001_i32, 1_i32), 0b00000010_i32);
        call!("rotate_left_8", i32, (0b10000000_i32, 1_i32), 0b00000001_i32);
        call!("rotate_left_8", i32, (0b10110011_i32, 4_i32), 0b00111011_i32);

        // --- count_leading_zeros ---
        call!("count_leading_zeros", i32, 0_i32, 32_i32);
        call!("count_leading_zeros", i32, 1_i32, 31_i32);

        // --- byte_swap_16 ---
        call!("byte_swap_16", i32, 0x1234_i32, 0x3412_i32);
        call!("byte_swap_16", i32, 0x0000_i32, 0x0000_i32);
        call!("byte_swap_16", i32, 0x00FF_i32, 0xFF00_i32);
    }
}

/// Test data regeneration helper.
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("algo_bitwise")
    }

    #[test]
    #[ignore]
    fn regenerate_algo_bitwise_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("algo_bitwise.inf"))
            .expect("Failed to read algo_bitwise.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("algo_bitwise.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "algo_bitwise");
    }
}
