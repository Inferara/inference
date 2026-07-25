// Convergent algorithm tests: iterative functions with `mut` accumulators and
// conditional loops, non-deterministic specification blocks, bitwise operations in
// arithmetic context, bool return values, and complex conditionals.
//
// All functions are iterative (a leading guard, `mut` accumulators, a conditional
// `loop` with a single trailing return) to comply with A035, which forbids the
// recursion the earlier helper-based versions relied on.
//
// WASM bytecode analysis (no WAT file due to non-det opcodes):
//
// slow_div(a, b) -> i32:
//   let acc = 0, let x = a; loop x >= b { x = x - b; acc = acc + 1; }; return acc
//   (Subtraction-based division accumulating the quotient)
//
// peasant_mul(a, b) -> i32:
//   let acc = 0, x = a, y = b; loop x > 0 { if (x & 1) == 1 { acc = acc + y; }
//     x = x >> 1; y = y << 1; }; return acc
//   (Russian peasant multiplication: shift-and-add with bitwise odd check)
//
// is_prime(n) -> bool (i32):
//   guard n <= 1; let result = true, d = 2; loop d * d <= n {
//     if (n % d) == 0 { result = false; break; } d = d + 1; }; return result
//   (Trial division primality check; bool maps to i32: true=1, false=0)
//
// collatz_steps(n) -> i32:
//   loop x > 1, branching even (x = x >> 1) / odd (x = 3*x + 1), counting steps
//
// collatz_max(n) -> i32:
//   loop x > 1 over the Collatz sequence, tracking the maximum value seen in `best`
//
// spec_division():
//   0xfc 0x3a (forall) + 0x40 (empty block type) +
//     0xfc 0x31 (i32.uzumaki) + local.set $a +
//     0xfc 0x31 (i32.uzumaki) + local.set $b +
//     0xfc 0x3c (assume) + 0x40 +
//       local.get $b, i32.const 0, i32.gt_s + local.set $check +
//     0x0b (end assume) +
//   0x0b (end forall)
//   (Non-det spec: forall a,b with assume b > 0)

#[cfg(test)]
mod algo_converge_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen, wasm_codegen_no_analysis,
    };

    #[test]
    fn algo_converge_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 34);
        cov_mark::check_count!(wasm_codegen_emit_if_statement, 8);
        cov_mark::check_count!(wasm_codegen_emit_function_call, 0);
        cov_mark::check_count!(wasm_codegen_emit_variable_definition, 15);
        cov_mark::check_count!(wasm_codegen_emit_parenthesized_expression, 4);
        cov_mark::check_count!(wasm_codegen_emit_function_params, 9);
        cov_mark::check_count!(wasm_codegen_emit_forall_block, 1);
        cov_mark::check_count!(wasm_codegen_emit_assume_block, 1);
        cov_mark::check_count!(wasm_codegen_emit_uzumaki_i32, 2);
        let test_name = "algo_converge";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        // The fixture's `spec_division` puts a `forall`/`assume` in a plain function,
        // which A042 rejects; this test exercises codegen lowering, so it bypasses
        // analysis (the golden stays byte-identical).
        let actual = wasm_codegen_no_analysis(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn algo_converge_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "algo_converge";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        // Strip the spec_division function which contains non-det opcodes
        // that wasmtime cannot execute (forall, assume, uzumaki).
        let source_code = source_code
            .lines()
            .take_while(|line| !line.starts_with("pub fn spec_division"))
            .collect::<Vec<_>>()
            .join("\n");
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

        // --- slow_div (repeated subtraction) ---
        call!("slow_div", i32, (10_i32, 3_i32), 3_i32);
        call!("slow_div", i32, (20_i32, 4_i32), 5_i32);
        call!("slow_div", i32, (7_i32, 7_i32), 1_i32);
        call!("slow_div", i32, (0_i32, 5_i32), 0_i32);
        call!("slow_div", i32, (100_i32, 10_i32), 10_i32);

        // --- slow_mod ---
        call!("slow_mod", i32, (10_i32, 3_i32), 1_i32);
        call!("slow_mod", i32, (20_i32, 7_i32), 6_i32);
        call!("slow_mod", i32, (9_i32, 3_i32), 0_i32);
        call!("slow_mod", i32, (1_i32, 5_i32), 1_i32);

        // --- peasant_mul (Russian peasant multiplication) ---
        call!("peasant_mul", i32, (6_i32, 7_i32), 42_i32);
        call!("peasant_mul", i32, (0_i32, 100_i32), 0_i32);
        call!("peasant_mul", i32, (1_i32, 99_i32), 99_i32);
        call!("peasant_mul", i32, (13_i32, 17_i32), 221_i32);
        call!("peasant_mul", i32, (255_i32, 255_i32), 65025_i32);

        // --- is_prime ---
        call!("is_prime", i32, 0_i32, 0_i32);  // false
        call!("is_prime", i32, 1_i32, 0_i32);  // false
        call!("is_prime", i32, 2_i32, 1_i32);  // true
        call!("is_prime", i32, 3_i32, 1_i32);  // true
        call!("is_prime", i32, 4_i32, 0_i32);  // false
        call!("is_prime", i32, 17_i32, 1_i32); // true
        call!("is_prime", i32, 25_i32, 0_i32); // false

        // --- collatz_steps ---
        call!("collatz_steps", i32, 1_i32, 0_i32);
        call!("collatz_steps", i32, 2_i32, 1_i32);
        call!("collatz_steps", i32, 6_i32, 8_i32);
        call!("collatz_steps", i32, 27_i32, 111_i32);

        // --- collatz_max ---
        call!("collatz_max", i32, 1_i32, 1_i32);
        call!("collatz_max", i32, 2_i32, 2_i32);
    }
}

/// Test data regeneration helper.
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen_no_analysis};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("algo_converge")
    }

    #[test]
    #[ignore]
    fn regenerate_algo_converge_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("algo_converge.inf"))
            .expect("Failed to read algo_converge.inf");
        let actual = wasm_codegen_no_analysis(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("algo_converge.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "algo_converge");
    }
}
