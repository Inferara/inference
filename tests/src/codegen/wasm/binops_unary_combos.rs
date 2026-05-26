/// Tests for combinations of unary and binary operators.
///
/// Verifies that prefix unary operators (`-`, `!`, `~`) interact correctly
/// with binary operators when composed in the same expression. Each function
/// in the fixture exercises a different combination pattern.
///
/// WASM lowering patterns verified via bytecode analysis:
///
/// - `neg_add(-a + b)`: `i32.const 0; local.get $a; i32.sub; local.get $b; i32.add`
///   Negation lowers as `0 - x`, then binary add follows.
///
/// - `bitnot_and(~a & b)`: `local.get $a; i32.const -1; i32.xor; local.get $b; i32.and`
///   BitNot lowers as `x ^ -1`, then bitwise AND follows.
///
/// - `not_and(!a && b)`: `local.get $a; i32.eqz; local.get $b; i32.and`
///   Logical NOT via `i32.eqz`, then non-short-circuit AND via `i32.and`.
#[cfg(test)]
mod binops_unary_combos_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn binops_unary_combos_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 17);
        cov_mark::check_count!(wasm_codegen_emit_prefix_unary_expression, 18);
        cov_mark::check_count!(wasm_codegen_emit_parenthesized_expression, 6);
        cov_mark::check_count!(wasm_codegen_emit_unary_neg, 7);
        cov_mark::check_count!(wasm_codegen_emit_unary_not, 5);
        cov_mark::check_count!(wasm_codegen_emit_unary_bitnot, 6);
        let test_name = "binops_unary_combos";
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
    fn binops_unary_combos_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "binops_unary_combos";
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
                assert_eq!(
                    result, $expected,
                    "{}({:?}) expected {:?}",
                    $name, $args, $expected
                );
            }};
        }

        // neg_add: -a + b
        call!("neg_add", i32, (3_i32, 5_i32), 2_i32);
        call!("neg_add", i32, (0_i32, 0_i32), 0_i32);
        call!("neg_add", i32, (-1_i32, 1_i32), 2_i32);

        // neg_sub: -a - b
        call!("neg_sub", i32, (3_i32, 5_i32), -8_i32);
        call!("neg_sub", i32, (1_i32, 1_i32), -2_i32);

        // neg_mul: -(a * b)
        call!("neg_mul", i32, (3_i32, 4_i32), -12_i32);
        call!("neg_mul", i32, (0_i32, 100_i32), 0_i32);
        call!("neg_mul", i32, (-2_i32, 3_i32), 6_i32);

        // add_neg: a + (-b)
        call!("add_neg", i32, (5_i32, 3_i32), 2_i32);
        call!("add_neg", i32, (0_i32, 0_i32), 0_i32);
        call!("add_neg", i32, (10_i32, -5_i32), 15_i32);

        // bitnot_and: ~a & b
        call!("bitnot_and", i32, (0xFF_i32, 0x0F_i32), 0_i32);
        call!("bitnot_and", i32, (0_i32, -1_i32), -1_i32);
        call!("bitnot_and", i32, (-1_i32, 0x0F_i32), 0_i32);

        // bitnot_or: ~a | b
        call!("bitnot_or", i32, (0xFF_i32, 0x0F_i32), -241_i32); // ~0xFF | 0x0F = 0xFFFFFF00 | 0x0F = 0xFFFFFF0F
        call!("bitnot_or", i32, (0_i32, 0_i32), -1_i32); // ~0 | 0 = 0xFFFFFFFF

        // and_bitnot: a & ~b
        call!("and_bitnot", i32, (0xFF_i32, 0x0F_i32), 0xF0_i32);
        call!("and_bitnot", i32, (0x0F_i32, 0_i32), 0x0F_i32);

        // xor_bitnot: a ^ ~b
        call!("xor_bitnot", i32, (0_i32, 0_i32), -1_i32);
        call!("xor_bitnot", i32, (-1_i32, -1_i32), -1_i32);

        // not_and: !a && b (bools as i32: 0=false, 1=true)
        call!("not_and", i32, (1_i32, 1_i32), 0_i32);
        call!("not_and", i32, (0_i32, 1_i32), 1_i32);
        call!("not_and", i32, (0_i32, 0_i32), 0_i32);
        call!("not_and", i32, (1_i32, 0_i32), 0_i32);

        // not_or: !a || b
        call!("not_or", i32, (1_i32, 0_i32), 0_i32);
        call!("not_or", i32, (0_i32, 0_i32), 1_i32);
        call!("not_or", i32, (1_i32, 1_i32), 1_i32);
        call!("not_or", i32, (0_i32, 1_i32), 1_i32);

        // and_not: a && !b
        call!("and_not", i32, (1_i32, 1_i32), 0_i32);
        call!("and_not", i32, (1_i32, 0_i32), 1_i32);
        call!("and_not", i32, (0_i32, 1_i32), 0_i32);
        call!("and_not", i32, (0_i32, 0_i32), 0_i32);

        // not_eq: !(a == b)
        call!("not_eq", i32, (5_i32, 5_i32), 0_i32);
        call!("not_eq", i32, (5_i32, 6_i32), 1_i32);
        call!("not_eq", i32, (0_i32, 0_i32), 0_i32);

        // not_lt: !(a < b)
        call!("not_lt", i32, (3_i32, 5_i32), 0_i32);
        call!("not_lt", i32, (5_i32, 3_i32), 1_i32);
        call!("not_lt", i32, (3_i32, 3_i32), 1_i32);

        // neg_i64_add: -a + b (i64)
        call!("neg_i64_add", i64, (3_i64, 5_i64), 2_i64);
        call!("neg_i64_add", i64, (0_i64, 0_i64), 0_i64);
        call!("neg_i64_add", i64, (i64::MAX, 0_i64), -i64::MAX);

        // bitnot_i64_and: ~a & b (i64)
        call!("bitnot_i64_and", i64, (0xFF_i64, 0x0F_i64), 0_i64);
        call!("bitnot_i64_and", i64, (0_i64, -1_i64), -1_i64);

        // neg_shift: (-a) << b
        call!("neg_shift", i32, (1_i32, 3_i32), -8_i32);
        call!("neg_shift", i32, (0_i32, 5_i32), 0_i32);
        call!("neg_shift", i32, (2_i32, 1_i32), -4_i32);

        // bitnot_shift: (~a) >> b (arithmetic shift right for signed i32)
        call!("bitnot_shift", i32, (0_i32, 1_i32), -1_i32);
        call!("bitnot_shift", i32, (-1_i32, 1_i32), 0_i32);
        call!("bitnot_shift", i32, (0xFF_i32, 4_i32), -16_i32);

        // neg_i64: -a (i64 negation)
        call!("neg_i64", i64, 5_i64, -5_i64);
        call!("neg_i64", i64, -3_i64, 3_i64);
        call!("neg_i64", i64, 0_i64, 0_i64);
    }
}

/// Test data regeneration helper for binops_unary_combos.
///
/// Run with `--ignored` flag:
/// ```bash
/// cargo test -p inference-tests codegen::wasm::binops_unary_combos::regenerate -- --ignored
/// ```
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("binops_unary_combos")
    }

    #[test]
    #[ignore]
    fn regenerate_binops_unary_combos_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("binops_unary_combos.inf"))
            .expect("Failed to read binops_unary_combos.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("binops_unary_combos.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "binops_unary_combos");
    }
}
