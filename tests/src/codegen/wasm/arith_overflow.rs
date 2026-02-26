// Arithmetic overflow/wrapping behavior tests.
//
// WASM integers use two's complement wrapping arithmetic with no overflow traps.
// These tests verify that boundary arithmetic (max+1, min-1, negating min, etc.)
// wraps correctly for i32, i64, and u32 types.
//
// Expected wrapping behavior:
//   i32_max_plus_one:  2147483647 + 1  = -2147483648  (wraps to i32::MIN)
//   i32_min_minus_one: -2147483648 - 1 = 2147483647   (wraps to i32::MAX)
//   i64_max_plus_one:  i64::MAX + 1    = i64::MIN     (wraps to i64::MIN)
//   i64_min_minus_one: i64::MIN - 1    = i64::MAX     (wraps to i64::MAX)
//   u32_max_plus_one:  4294967295 + 1  = 0            (wraps to zero)
//   i32_mul_overflow:  2147483647 * 2  = -2            (truncated to i32)
//   i32_neg_min:       -(-2147483648)  = -2147483648   (negating MIN wraps to MIN)
//   i64_neg_min:       -(i64::MIN)     = i64::MIN      (negating MIN wraps to MIN)
//
// Total: 6 binary expressions, 2 prefix unary (neg), 11 constant definitions.

#[cfg(test)]
mod arith_overflow_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn arith_overflow_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 6);
        cov_mark::check_count!(wasm_codegen_emit_prefix_unary_expression, 2);
        cov_mark::check_count!(wasm_codegen_emit_unary_neg, 2);
        cov_mark::check_count!(wasm_codegen_emit_constant_definition, 11);
        let test_name = "arith_overflow";
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
    fn arith_overflow_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "arith_overflow";
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

        // --- i32 wrapping ---

        // i32::MAX + 1 wraps to i32::MIN
        call!("i32_max_plus_one", i32, (), i32::MIN);
        // i32::MIN - 1 wraps to i32::MAX
        call!("i32_min_minus_one", i32, (), i32::MAX);

        // --- i64 wrapping ---

        // i64::MAX + 1 wraps to i64::MIN
        call!("i64_max_plus_one", i64, (), i64::MIN);
        // i64::MIN - 1 wraps to i64::MAX
        call!("i64_min_minus_one", i64, (), i64::MAX);

        // --- u32 wrapping (returned as i32 in WASM) ---

        // u32::MAX + 1 wraps to 0
        call!("u32_max_plus_one", i32, (), 0_i32);

        // --- Multiplication overflow ---

        // 2147483647 * 2 = 4294967294 = -2 as i32
        call!("i32_mul_overflow", i32, (), -2_i32);

        // --- Negation of MIN wraps back to MIN ---

        // -i32::MIN = i32::MIN (two's complement: no positive representation)
        call!("i32_neg_min", i32, (), i32::MIN);
        // -i64::MIN = i64::MIN
        call!("i64_neg_min", i64, (), i64::MIN);
    }
}

/// Test data regeneration helper.
///
/// Regenerates the expected `.wasm` and `.wat` golden files from the current compiler output.
/// Run with `--ignored` flag:
///
/// ```bash
/// cargo test -p inference-tests codegen::wasm::arith_overflow::regenerate -- --ignored
/// ```
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("arith_overflow")
    }

    #[test]
    #[ignore]
    fn regenerate_arith_overflow_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("arith_overflow.inf"))
            .expect("Failed to read arith_overflow.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("arith_overflow.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "arith_overflow");
    }
}
