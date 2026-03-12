// Iterative scalar algorithm tests with cross-compiler comparison.
//
// 12 iterative algorithms covering bool, u8, i16, u16, i32, and i64 types:
//
// i32 functions:
//   fibonacci_iter(n)       — iterative Fibonacci via accumulator pair
//   gcd_iter(a, b)          — Euclidean GCD with absolute value normalization
//   is_prime_iter(n)        — trial division primality test returning 0/1
//   isqrt(n)                — integer square root via Newton's method
//   pow_iter(base, exp)     — binary exponentiation (square-and-multiply)
//
// i64 functions:
//   fibonacci_iter_i64(n)   — i64 Fibonacci for large values (fib(50))
//   gcd_iter_i64(a, b)      — i64 Euclidean GCD
//   pow_iter_i64(base, exp) — i64 binary exponentiation
//
// Sub-i32 functions (WASM ABI uses i32):
//   gcd_u8(a, b)            — u8 GCD
//   fibonacci_i16(n)        — i16 Fibonacci
//   pow_u16(base, exp)      — u16 binary exponentiation
//
// Bool function (WASM ABI uses i32):
//   is_prime_bool(n)        — primality test returning bool
//
// Total across all 12 functions: 72 binary expressions, 22 if statements, 18 params,
// 34 variable definitions, 9 parenthesized expressions, 12 loops (all conditional),
// 32 assignments, 0 function calls (all iterative).

#[cfg(test)]
mod algo_iter_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn algo_iter_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 72);
        cov_mark::check_count!(wasm_codegen_emit_if_statement, 22);
        cov_mark::check_count!(wasm_codegen_emit_function_params, 18);
        cov_mark::check_count!(wasm_codegen_emit_variable_definition, 46);
        cov_mark::check_count!(wasm_codegen_emit_parenthesized_expression, 9);
        cov_mark::check_count!(wasm_codegen_emit_loop_statement, 12);
        cov_mark::check_count!(wasm_codegen_emit_loop_conditional, 12);
        cov_mark::check_count!(wasm_codegen_emit_assign_identifier, 34);
        let test_name = "algo_iter";
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
    fn algo_iter_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "algo_iter";
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

        // --- fibonacci_iter (i32) ---
        call!("fibonacci_iter", i32, 0_i32, 0_i32);
        call!("fibonacci_iter", i32, 1_i32, 1_i32);
        call!("fibonacci_iter", i32, 2_i32, 1_i32);
        call!("fibonacci_iter", i32, 10_i32, 55_i32);
        call!("fibonacci_iter", i32, 20_i32, 6765_i32);

        // --- gcd_iter (i32) ---
        call!("gcd_iter", i32, (12_i32, 8_i32), 4_i32);
        call!("gcd_iter", i32, (17_i32, 13_i32), 1_i32);
        call!("gcd_iter", i32, (100_i32, 0_i32), 100_i32);
        call!("gcd_iter", i32, (0_i32, 5_i32), 5_i32);
        call!("gcd_iter", i32, (48_i32, 18_i32), 6_i32);

        // --- is_prime_iter (i32, returns 0/1) ---
        call!("is_prime_iter", i32, 0_i32, 0_i32);
        call!("is_prime_iter", i32, 1_i32, 0_i32);
        call!("is_prime_iter", i32, 2_i32, 1_i32);
        call!("is_prime_iter", i32, 3_i32, 1_i32);
        call!("is_prime_iter", i32, 4_i32, 0_i32);
        call!("is_prime_iter", i32, 17_i32, 1_i32);
        call!("is_prime_iter", i32, 25_i32, 0_i32);
        call!("is_prime_iter", i32, 97_i32, 1_i32);

        // --- isqrt (i32) ---
        call!("isqrt", i32, 0_i32, 0_i32);
        call!("isqrt", i32, 1_i32, 1_i32);
        call!("isqrt", i32, 4_i32, 2_i32);
        call!("isqrt", i32, 9_i32, 3_i32);
        call!("isqrt", i32, 10_i32, 3_i32);
        call!("isqrt", i32, 100_i32, 10_i32);
        call!("isqrt", i32, 99_i32, 9_i32);

        // --- pow_iter (i32) ---
        call!("pow_iter", i32, (2_i32, 0_i32), 1_i32);
        call!("pow_iter", i32, (2_i32, 10_i32), 1024_i32);
        call!("pow_iter", i32, (3_i32, 5_i32), 243_i32);
        call!("pow_iter", i32, (1_i32, 100_i32), 1_i32);

        // --- fibonacci_iter_i64 ---
        call!("fibonacci_iter_i64", i64, 0_i64, 0_i64);
        call!("fibonacci_iter_i64", i64, 1_i64, 1_i64);
        call!("fibonacci_iter_i64", i64, 10_i64, 55_i64);
        call!("fibonacci_iter_i64", i64, 50_i64, 12586269025_i64);

        // --- gcd_iter_i64 ---
        call!("gcd_iter_i64", i64, (120_i64, 80_i64), 40_i64);
        call!("gcd_iter_i64", i64, (1000000007_i64, 0_i64), 1000000007_i64);

        // --- pow_iter_i64 ---
        call!("pow_iter_i64", i64, (2_i64, 40_i64), 1099511627776_i64);
        call!("pow_iter_i64", i64, (3_i64, 20_i64), 3486784401_i64);

        // --- gcd_u8 (WASM ABI: i32) ---
        call!("gcd_u8", i32, (12_i32, 8_i32), 4_i32);
        call!("gcd_u8", i32, (17_i32, 13_i32), 1_i32);
        call!("gcd_u8", i32, (100_i32, 0_i32), 100_i32);
        call!("gcd_u8", i32, (255_i32, 85_i32), 85_i32);

        // --- fibonacci_i16 (WASM ABI: i32) ---
        call!("fibonacci_i16", i32, 0_i32, 0_i32);
        call!("fibonacci_i16", i32, 1_i32, 1_i32);
        call!("fibonacci_i16", i32, 10_i32, 55_i32);
        call!("fibonacci_i16", i32, 23_i32, 28657_i32);

        // --- pow_u16 (WASM ABI: i32) ---
        call!("pow_u16", i32, (2_i32, 10_i32), 1024_i32);
        call!("pow_u16", i32, (3_i32, 5_i32), 243_i32);
        call!("pow_u16", i32, (5_i32, 3_i32), 125_i32);

        // --- is_prime_bool (WASM ABI: i32, true=1 false=0) ---
        call!("is_prime_bool", i32, 0_i32, 0_i32);
        call!("is_prime_bool", i32, 1_i32, 0_i32);
        call!("is_prime_bool", i32, 2_i32, 1_i32);
        call!("is_prime_bool", i32, 17_i32, 1_i32);
        call!("is_prime_bool", i32, 25_i32, 0_i32);
        call!("is_prime_bool", i32, 97_i32, 1_i32);
    }

    #[test]
    fn algo_iter_cross_compiler_test() {
        use crate::utils::{get_test_data_path, wasm_codegen};
        use std::process::Command;
        use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

        // Compile Inference source
        let test_name = "algo_iter";
        let test_dir = get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join(test_name);
        let inf_source = std::fs::read_to_string(test_dir.join("algo_iter.inf"))
            .unwrap_or_else(|_| panic!("Failed to read algo_iter.inf"));
        let inf_wasm = wasm_codegen(&inf_source);

        // Compile Rust source
        let rust_source = test_dir.join("algo_iter.rs");
        let rust_out_dir = std::path::PathBuf::from("/tmp/algo_iter_rustc");
        std::fs::create_dir_all(&rust_out_dir)
            .unwrap_or_else(|e| panic!("Failed to create {}: {e}", rust_out_dir.display()));
        let rust_wasm_path = rust_out_dir.join("algo_iter.wasm");
        let rustc_result = Command::new("rustc")
            .arg("--target")
            .arg("wasm32-unknown-unknown")
            .arg("--crate-type")
            .arg("cdylib")
            .arg("-C")
            .arg("opt-level=0")
            .arg("-o")
            .arg(&rust_wasm_path)
            .arg(&rust_source)
            .output();
        let rustc_output = match rustc_result {
            Ok(output) => output,
            Err(_) => {
                println!("Skipping: rustc not available");
                return;
            }
        };
        if !rustc_output.status.success() {
            println!(
                "Skipping: rustc compilation failed: {}",
                String::from_utf8_lossy(&rustc_output.stderr)
            );
            return;
        }
        let rust_wasm = std::fs::read(&rust_wasm_path)
            .unwrap_or_else(|e| panic!("Failed to read Rust wasm output: {e}"));

        // Compile Zig source
        let zig_source = test_dir.join("algo_iter.zig");
        let zig_out_dir = std::path::PathBuf::from("/tmp/algo_iter_zig");
        std::fs::create_dir_all(&zig_out_dir)
            .unwrap_or_else(|e| panic!("Failed to create {}: {e}", zig_out_dir.display()));
        let zig_result = Command::new("zig")
            .arg("build-exe")
            .arg("-target")
            .arg("wasm32-freestanding")
            .arg("-fno-entry")
            .arg("-rdynamic")
            .arg("-OReleaseFast")
            .arg(&zig_source)
            .current_dir(&zig_out_dir)
            .output();
        let zig_output = match zig_result {
            Ok(output) => output,
            Err(_) => {
                println!("Skipping: zig not available");
                return;
            }
        };
        if !zig_output.status.success() {
            println!(
                "Skipping: zig compilation failed: {}",
                String::from_utf8_lossy(&zig_output.stderr)
            );
            return;
        }
        let zig_wasm_path = zig_out_dir.join("algo_iter.wasm");
        let zig_wasm = std::fs::read(&zig_wasm_path)
            .unwrap_or_else(|e| panic!("Failed to read Zig wasm output: {e}"));

        // Load all three modules in wasmtime
        let engine = Engine::default();

        let inf_module = Module::new(&engine, &inf_wasm)
            .unwrap_or_else(|e| panic!("Failed to create Inference Wasm module: {e}"));
        let rust_module = Module::new(&engine, &rust_wasm)
            .unwrap_or_else(|e| panic!("Failed to create Rust Wasm module: {e}"));
        let zig_module = Module::new(&engine, &zig_wasm)
            .unwrap_or_else(|e| panic!("Failed to create Zig Wasm module: {e}"));

        let mut inf_store = Store::new(&engine, ());
        let mut rust_store = Store::new(&engine, ());
        let mut zig_store = Store::new(&engine, ());

        let inf_instance = Instance::new(&mut inf_store, &inf_module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Inference module: {e}"));
        let rust_instance = Instance::new(&mut rust_store, &rust_module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Rust module: {e}"));
        let zig_instance = Instance::new(&mut zig_store, &zig_module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Zig module: {e}"));

        macro_rules! compare {
            ($name:expr, $ty:ty, $args:expr) => {{
                let inf_f: TypedFunc<_, $ty> = inf_instance
                    .get_typed_func(&mut inf_store, $name)
                    .unwrap_or_else(|e| panic!("Inference: failed to get '{}': {e}", $name));
                let rust_f: TypedFunc<_, $ty> = rust_instance
                    .get_typed_func(&mut rust_store, $name)
                    .unwrap_or_else(|e| panic!("Rust: failed to get '{}': {e}", $name));
                let zig_f: TypedFunc<_, $ty> = zig_instance
                    .get_typed_func(&mut zig_store, $name)
                    .unwrap_or_else(|e| panic!("Zig: failed to get '{}': {e}", $name));
                let inf_result = inf_f
                    .call(&mut inf_store, $args)
                    .unwrap_or_else(|e| panic!("Inference call to '{}' failed: {e}", $name));
                let rust_result = rust_f
                    .call(&mut rust_store, $args)
                    .unwrap_or_else(|e| panic!("Rust call to '{}' failed: {e}", $name));
                let zig_result = zig_f
                    .call(&mut zig_store, $args)
                    .unwrap_or_else(|e| panic!("Zig call to '{}' failed: {e}", $name));
                assert_eq!(
                    inf_result, rust_result,
                    "Inference vs Rust mismatch for {}({:?})",
                    $name, $args
                );
                assert_eq!(
                    inf_result, zig_result,
                    "Inference vs Zig mismatch for {}({:?})",
                    $name, $args
                );
            }};
        }

        // --- fibonacci_iter (i32): exhaustive -5..=46, 52 inputs ---
        for n in -5..=46_i32 {
            compare!("fibonacci_iter", i32, n);
        }

        // --- gcd_iter (i32): Cartesian product of 50 edge values, ~2500 inputs ---
        let gcd_edges: &[i32] = &[
            0, 1, 2, 3, 5, 6, 7, 12, 13, 17, 18, 24, 48, 97, 100, 255, 256, 1000, 9999, 10000,
            32767, 32768, 65535, 65536, 100000, 1000000, i32::MAX, -1, -2, -3, -5, -7, -12, -13,
            -17, -48, -97, -100, -255, -1000, -9999, -32767, -32768, -65535, -65536, -100000,
            -1000000, -(i32::MAX),
        ];
        for &a in gcd_edges {
            for &b in gcd_edges {
                compare!("gcd_iter", i32, (a, b));
            }
        }

        // --- is_prime_iter (i32): sweep -10..=10000, 10011 inputs ---
        for n in -10..=10000_i32 {
            compare!("is_prime_iter", i32, n);
        }

        // --- isqrt (i32): sweep -5..=10000 + large values, ~10106 inputs ---
        for n in -5..=10000_i32 {
            compare!("isqrt", i32, n);
        }
        for k in (100..=46300_i32).step_by(100) {
            compare!("isqrt", i32, k * k);
            if k * k > 1 {
                compare!("isqrt", i32, k * k - 1);
                compare!("isqrt", i32, k * k + 1);
            }
        }
        compare!("isqrt", i32, i32::MAX - 1);
        compare!("isqrt", i32, 2_000_000_000_i32);
        compare!("isqrt", i32, 1_999_999_999_i32);

        // --- pow_iter (i32): Cartesian product of bases x exponents, ~1550 inputs ---
        let pow_bases: Vec<i32> = (-20..=20)
            .chain(
                [100, -100, 1000, -1000, i32::MAX, i32::MIN, 46340, -46340, 0x7FFF, -0x7FFF]
                    .iter()
                    .copied(),
            )
            .collect();
        for &base in &pow_bases {
            for exp in 0..=30_i32 {
                compare!("pow_iter", i32, (base, exp));
            }
        }

        // --- fibonacci_iter_i64: exhaustive -5..=92, 98 inputs ---
        for n in -5..=92_i64 {
            compare!("fibonacci_iter_i64", i64, n);
        }

        // --- gcd_iter_i64: Cartesian product of 40 edge values, ~1600 inputs ---
        let gcd_i64_edges: &[i64] = &[
            0, 1, 2, 3, 5, 7, 13, 17, 100, 1000, 65536, 1000000, 1000000007, i64::MAX, -1, -2,
            -3, -5, -7, -13, -17, -100, -1000, -65536, -1000000, -1000000007, -(i64::MAX),
            2147483647, -2147483647, 2147483648, -2147483648, 4294967295, -4294967295, 4294967296,
            -4294967296, 999999999999, -999999999999, 1099511627776, -1099511627776, 3486784401,
        ];
        for &a in gcd_i64_edges {
            for &b in gcd_i64_edges {
                compare!("gcd_iter_i64", i64, (a, b));
            }
        }

        // --- pow_iter_i64: Cartesian product of bases x exponents, ~1260 inputs ---
        let pow_i64_bases: Vec<i64> = (-10..=10_i64)
            .chain(
                [100, -100, 1000, -1000, i64::MAX, i64::MIN, 2147483647, -2147483647]
                    .iter()
                    .copied(),
            )
            .collect();
        for &base in &pow_i64_bases {
            for exp in 0..=62_i64 {
                compare!("pow_iter_i64", i64, (base, exp));
            }
        }

        // --- gcd_u8 (WASM ABI: i32): exhaustive 256x256, 65536 inputs ---
        for a in 0..=255_i32 {
            for b in 0..=255_i32 {
                compare!("gcd_u8", i32, (a, b));
            }
        }

        // --- fibonacci_i16 (WASM ABI: i32): exhaustive -5..=23, 29 inputs ---
        for n in -5..=23_i32 {
            compare!("fibonacci_i16", i32, n);
        }

        // --- pow_u16 (WASM ABI: i32): Cartesian product of bases x exponents, ~240 inputs ---
        let pow_u16_bases: &[i32] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 100, 255, 256, 65535];
        for &base in pow_u16_bases {
            for exp in 0..=15_i32 {
                compare!("pow_u16", i32, (base, exp));
            }
        }

        // --- is_prime_bool (WASM ABI: i32, true=1 false=0): sweep -10..=10000, 10011 inputs ---
        for n in -10..=10000_i32 {
            compare!("is_prime_bool", i32, n);
        }
    }
}

/// Test data regeneration helper.
///
/// Regenerates the expected `.wasm` golden file from the current compiler output.
/// Run with `--ignored` flag:
///
/// ```bash
/// cargo test -p inference-tests codegen::wasm::algo_iter::regenerate -- --ignored
/// ```
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("algo_iter")
    }

    #[test]
    #[ignore]
    fn regenerate_algo_iter_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("algo_iter.inf"))
            .expect("Failed to read algo_iter.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("algo_iter.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "algo_iter");
    }
}
