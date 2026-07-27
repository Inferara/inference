// Sub-i32 binary operation tests for i8, u8, i16, u16 types.
//
// All sub-i32 types map to i32 in WASM (WASM has no i8/i16 types).
// The type-checker selects signed vs unsigned WASM instruction variants
// based on the declared Inference type:
//
// Verified WASM bytecode (via analyze-wasm-bytecode):
//   - div_i8  -> i32.div_s  (signed division)
//   - div_u8  -> i32.div_u  (unsigned division)
//   - lt_i8   -> i32.lt_s   (signed less-than)
//   - lt_u8   -> i32.lt_u   (unsigned less-than)
//   - shr_u8  -> i32.shr_u  (unsigned shift right)
//   - div_i16 -> i32.div_s  (signed division)
//   - lt_i16  -> i32.lt_s   (signed less-than)
//   - div_u16 -> i32.div_u  (unsigned division)
//   - gt_u16  -> i32.gt_u   (unsigned greater-than)
//   - mod_u16 -> i32.rem_u  (unsigned remainder)

#[cfg(test)]
mod binops_sub_i32_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn binops_sub_i32_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 16);
        // div_i8 and div_i16 are the two signed narrow divisions in the fixture,
        // each emitting one overflow guard; the unsigned narrow divisions
        // (div_u8, div_u16) emit none.
        cov_mark::check_count!(wasm_codegen_narrow_div_overflow_guard, 2);
        let test_name = "binops_sub_i32";
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
    fn binops_sub_i32_execution_test() {
        use wasmtime::{Engine, Module, Store, Trap, TypedFunc};

        let test_name = "binops_sub_i32";
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

        macro_rules! call_trap {
            ($name:expr, $args:expr, $expected_trap:expr) => {{
                let f: TypedFunc<_, i32> = instance
                    .get_typed_func(&mut store, $name)
                    .unwrap_or_else(|e| panic!("Failed to get '{}': {e}", $name));
                let err = f
                    .call(&mut store, $args)
                    .expect_err(&format!("{}({:?}) expected to trap", $name, $args));
                let trap = err.downcast_ref::<Trap>().unwrap_or_else(|| {
                    panic!("expected a wasmtime Trap from '{}', got: {err:?}", $name)
                });
                assert_eq!(
                    *trap, $expected_trap,
                    "{}({:?}) expected trap {:?}",
                    $name, $args, $expected_trap
                );
            }};
        }

        // i8 signed arithmetic
        call!("add_i8", (100_i32, 27_i32), 127_i32);
        call!("add_i8", (-100_i32, 50_i32), -50_i32);
        call!("sub_i8", (10_i32, 3_i32), 7_i32);
        call!("mul_i8", (6_i32, 7_i32), 42_i32);
        call!("div_i8", (-10_i32, 3_i32), -3_i32);
        call!("div_i8", (10_i32, 3_i32), 3_i32);

        // i8 signed comparison
        call!("lt_i8", (-1_i32, 0_i32), 1_i32);
        call!("lt_i8", (0_i32, -1_i32), 0_i32);
        call!("lt_i8", (5_i32, 5_i32), 0_i32);

        // u8 unsigned arithmetic
        call!("add_u8", (100_i32, 55_i32), 155_i32);
        call!("div_u8", (254_i32, 2_i32), 127_i32);
        call!("div_u8", (10_i32, 3_i32), 3_i32);

        // u8 unsigned comparison (255 is large positive, not -1)
        call!("lt_u8", (255_i32, 0_i32), 0_i32);
        call!("lt_u8", (0_i32, 255_i32), 1_i32);
        call!("lt_u8", (100_i32, 200_i32), 1_i32);

        // u8 unsigned shift right
        call!("shr_u8", (128_i32, 1_i32), 64_i32);
        call!("shr_u8", (255_i32, 4_i32), 15_i32);

        // i16 signed arithmetic
        call!("add_i16", (1000_i32, 2000_i32), 3000_i32);
        call!("add_i16", (-1000_i32, 500_i32), -500_i32);
        call!("div_i16", (-100_i32, 3_i32), -33_i32);

        // i16 signed comparison
        call!("lt_i16", (-1_i32, 0_i32), 1_i32);
        call!("lt_i16", (0_i32, -1_i32), 0_i32);
        call!("lt_i16", (100_i32, 100_i32), 0_i32);

        // u16 unsigned arithmetic
        call!("add_u16", (30000_i32, 5000_i32), 35000_i32);
        call!("div_u16", (65535_i32, 2_i32), 32767_i32);
        call!("div_u16", (100_i32, 3_i32), 33_i32);

        // u16 unsigned comparison (65535 is large positive, not -1)
        call!("gt_u16", (65535_i32, 0_i32), 1_i32);
        call!("gt_u16", (0_i32, 65535_i32), 0_i32);
        call!("gt_u16", (100_i32, 100_i32), 0_i32);

        // u16 unsigned remainder
        call!("mod_u16", (65535_i32, 1000_i32), 535_i32);
        call!("mod_u16", (10_i32, 3_i32), 1_i32);

        // Narrow signed division overflow: the promoted quotient +128 / +32768 is
        // reachable only from (MIN, -1), where the guard traps as `unreachable`.
        call_trap!("div_i8", (-128_i32, -1_i32), Trap::UnreachableCodeReached);
        call_trap!("div_i16", (-32768_i32, -1_i32), Trap::UnreachableCodeReached);

        // The guard runs after `div_s`, so a zero divisor still trips wasm's
        // native divide-by-zero trap (a distinct trap code) — the guard did not
        // preempt it.
        call_trap!("div_i8", (1_i32, 0_i32), Trap::IntegerDivisionByZero);
        call_trap!("div_i16", (1_i32, 0_i32), Trap::IntegerDivisionByZero);

        // Non-overflowing signed narrow divisions around the ±MIN/±MAX edges do
        // NOT trap: only the exact (MIN, -1) quotient equals the width bound.
        call!("div_i8", (-128_i32, 1_i32), -128_i32);
        call!("div_i8", (127_i32, -1_i32), -127_i32);
        call!("div_i8", (-127_i32, -1_i32), 127_i32);
        call!("div_i8", (-128_i32, -128_i32), 1_i32);
        call!("div_i16", (-32768_i32, 1_i32), -32768_i32);
        call!("div_i16", (32767_i32, -1_i32), -32767_i32);
        call!("div_i16", (-32768_i32, -2_i32), 16384_i32);

        // Unsigned narrow division is never guarded: a quotient equal to the
        // signed width bound (128 / 32768) must return, not trap — pins the
        // signed-only gating.
        call!("div_u8", (128_i32, 1_i32), 128_i32);
        call!("div_u16", (32768_i32, 1_i32), 32768_i32);
    }
}

#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("binops_sub_i32")
    }

    #[test]
    #[ignore]
    fn regenerate_binops_sub_i32_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("binops_sub_i32.inf"))
            .expect("Failed to read binops_sub_i32.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("binops_sub_i32.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "binops_sub_i32");
    }
}
