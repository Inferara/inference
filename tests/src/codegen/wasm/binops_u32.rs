// u32 unsigned binary operation tests.
//
// All u32 values map to i32 in WASM (WASM has no native u32 type).
// The type-checker selects unsigned WASM instruction variants for u32 operands,
// which produces different results than signed variants when the high bit is set.
//
// Verified WASM bytecode (via analyze-wasm-bytecode):
//   - div_u32          -> i32.div_u  (unsigned division)
//   - mod_u32          -> i32.rem_u  (unsigned remainder)
//   - lt_u32           -> i32.lt_u   (unsigned less-than)
//   - le_u32           -> i32.le_u   (unsigned less-or-equal)
//   - gt_u32           -> i32.gt_u   (unsigned greater-than)
//   - ge_u32           -> i32.ge_u   (unsigned greater-or-equal)
//   - shr_u32          -> i32.shr_u  (unsigned logical right shift)
//   - add_u32          -> i32.add    (no signed/unsigned distinction)
//   - mul_u32          -> i32.mul    (no signed/unsigned distinction)
//   - eq_u32           -> i32.eq     (no signed/unsigned distinction)
//   - high_bit_div_u32 -> i32.div_u  (unsigned division, high-bit focused tests)
//   - high_bit_lt_u32  -> i32.gt_u   (unsigned greater-than comparison)

#[cfg(test)]
mod binops_u32_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, get_test_file_path, get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn binops_u32_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 12);
        let test_name = "binops_u32";
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
    fn binops_u32_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "binops_u32";
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
            ($name:expr, $args:expr, $expected:expr) => {{
                let f: TypedFunc<_, i32> = instance
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

        // --- Division (i32.div_u) ---

        // Normal unsigned division
        call!("div_u32", (10_i32, 3_i32), 3_i32);
        call!("div_u32", (100_i32, 10_i32), 10_i32);
        call!("div_u32", (0_i32, 1_i32), 0_i32);

        // High-bit values: u32::MAX (0xFFFFFFFF) as i32 is -1
        // Unsigned: 4294967295 / 2 = 2147483647 (0x7FFFFFFF)
        // Signed would trap or give -1/2 = 0
        call!("div_u32", (u32::MAX as i32, 2_i32), 0x7FFF_FFFF_i32);

        // u32::MAX / 3 = 1431655765 (0x55555555)
        call!("div_u32", (u32::MAX as i32, 3_i32), 0x5555_5555_i32);

        // 0x80000000 / 2 = 0x40000000 (unsigned: 2147483648 / 2 = 1073741824)
        // Signed would give: i32::MIN / 2 = -1073741824
        call!("div_u32", (i32::MIN, 2_i32), 0x4000_0000_i32);

        // --- Remainder (i32.rem_u) ---

        call!("mod_u32", (10_i32, 3_i32), 1_i32);
        call!("mod_u32", (7_i32, 4_i32), 3_i32);
        call!("mod_u32", (0_i32, 5_i32), 0_i32);

        // u32::MAX % 3 = 4294967295 % 3 = 0 (since 4294967295 = 3 * 1431655765)
        call!("mod_u32", (u32::MAX as i32, 3_i32), 0_i32);

        // 0x80000000 % 3 = 2147483648 % 3 = 2
        call!("mod_u32", (i32::MIN, 3_i32), 2_i32);

        // --- Less-than (i32.lt_u) ---

        call!("lt_u32", (1_i32, 2_i32), 1_i32);
        call!("lt_u32", (2_i32, 1_i32), 0_i32);
        call!("lt_u32", (5_i32, 5_i32), 0_i32);

        // Critical unsigned semantics: -1 as u32 = u32::MAX, so u32::MAX is NOT < 0
        call!("lt_u32", (-1_i32, 0_i32), 0_i32);

        // 0 IS less than u32::MAX
        call!("lt_u32", (0_i32, -1_i32), 1_i32);

        // i32::MIN as u32 = 0x80000000 = 2147483648, which IS > 0x7FFFFFFF = 2147483647
        call!("lt_u32", (i32::MIN, i32::MAX), 0_i32);
        call!("lt_u32", (i32::MAX, i32::MIN), 1_i32);

        // --- Less-or-equal (i32.le_u) ---

        call!("le_u32", (1_i32, 2_i32), 1_i32);
        call!("le_u32", (5_i32, 5_i32), 1_i32);
        call!("le_u32", (2_i32, 1_i32), 0_i32);
        call!("le_u32", (-1_i32, -1_i32), 1_i32);
        call!("le_u32", (0_i32, -1_i32), 1_i32);
        call!("le_u32", (-1_i32, 0_i32), 0_i32);

        // --- Greater-than (i32.gt_u) ---

        call!("gt_u32", (2_i32, 1_i32), 1_i32);
        call!("gt_u32", (1_i32, 2_i32), 0_i32);
        call!("gt_u32", (5_i32, 5_i32), 0_i32);

        // u32::MAX > 0 unsigned
        call!("gt_u32", (-1_i32, 0_i32), 1_i32);
        call!("gt_u32", (0_i32, -1_i32), 0_i32);

        // --- Greater-or-equal (i32.ge_u) ---

        call!("ge_u32", (2_i32, 1_i32), 1_i32);
        call!("ge_u32", (5_i32, 5_i32), 1_i32);
        call!("ge_u32", (1_i32, 2_i32), 0_i32);
        call!("ge_u32", (-1_i32, 0_i32), 1_i32);
        call!("ge_u32", (0_i32, -1_i32), 0_i32);

        // --- Shift right unsigned (i32.shr_u) ---

        call!("shr_u32", (8_i32, 1_i32), 4_i32);
        call!("shr_u32", (1_i32, 1_i32), 0_i32);

        // 0x80000000 >> 1 = 0x40000000 (logical shift fills with 0)
        // Signed shift would give 0xC0000000 (-1073741824)
        call!("shr_u32", (i32::MIN, 1_i32), 0x4000_0000_i32);

        // 0xFFFFFFFF >> 1 = 0x7FFFFFFF
        call!("shr_u32", (-1_i32, 1_i32), 0x7FFF_FFFF_i32);

        // Shift by 31: extracts the high bit
        call!("shr_u32", (i32::MIN, 31_i32), 1_i32);
        call!("shr_u32", (-1_i32, 31_i32), 1_i32);
        call!("shr_u32", (i32::MAX, 31_i32), 0_i32);

        // --- Addition (i32.add, same for signed/unsigned) ---

        call!("add_u32", (1_i32, 2_i32), 3_i32);
        call!("add_u32", (0_i32, 0_i32), 0_i32);
        call!("add_u32", (-1_i32, 1_i32), 0_i32);
        call!("add_u32", (i32::MAX, 1_i32), i32::MIN);

        // --- Multiplication (i32.mul, same for signed/unsigned) ---

        call!("mul_u32", (6_i32, 7_i32), 42_i32);
        call!("mul_u32", (0_i32, 100_i32), 0_i32);
        call!("mul_u32", (-1_i32, 2_i32), -2_i32);

        // --- Equality (i32.eq, no sign distinction) ---

        call!("eq_u32", (5_i32, 5_i32), 1_i32);
        call!("eq_u32", (5_i32, 6_i32), 0_i32);
        call!("eq_u32", (-1_i32, -1_i32), 1_i32);
        call!("eq_u32", (0_i32, 0_i32), 1_i32);

        // --- high_bit_div_u32: a / b with unsigned semantics (high-bit focused) ---

        // 0x80000000 / 2 = 0x40000000 (unsigned: positive result)
        call!("high_bit_div_u32", (i32::MIN, 2_i32), 0x4000_0000_i32);
        call!("high_bit_div_u32", (10_i32, 2_i32), 5_i32);
        call!("high_bit_div_u32", (0_i32, 1_i32), 0_i32);

        // u32::MAX / 2 = 0x7FFFFFFF
        call!("high_bit_div_u32", (-1_i32, 2_i32), 0x7FFF_FFFF_i32);

        // --- high_bit_lt_u32: a > b with unsigned semantics ---

        // u32::MAX (as -1_i32) > 0x7FFFFFFF (as i32::MAX) is true unsigned
        call!("high_bit_lt_u32", (-1_i32, i32::MAX), 1_i32);

        // 0 > u32::MAX is false unsigned
        call!("high_bit_lt_u32", (0_i32, -1_i32), 0_i32);

        // u32::MAX > 0 is true
        call!("high_bit_lt_u32", (-1_i32, 0_i32), 1_i32);

        // Equal values: not greater
        call!("high_bit_lt_u32", (5_i32, 5_i32), 0_i32);
    }
}

#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, wasm_codegen};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("binops_u32")
            .join("binops_u32")
    }

    #[test]
    #[ignore]
    fn regenerate_binops_u32_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("binops_u32.inf"))
            .expect("Failed to read binops_u32.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("binops_u32.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
    }
}
