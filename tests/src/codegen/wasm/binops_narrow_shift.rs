// Uniform shift-count semantics for narrow integer types (see
// `binops_narrow_shift.inf`).
//
// A shift count is taken modulo the operand type's bit width. WebAssembly masks
// shift counts modulo 32/64 in the promoted width, which for a narrow type
// produces a non-monotonic cliff (`u8 x << 8` is 0 but `x << 32` is `x`). A
// narrow-typed shift masks the count to the declared width (`& 7` / `& 15`)
// before the wasm shift, extending wasm's own mod-width semantics to the type.
//
// The execution expectations are composed: each count parameter is itself an
// exported narrow parameter, so a host count is first canonicalized to its type
// domain (u8/u16 mask, i8/i16 sign-extend), and only then masked by the shift's
// own `& (width - 1)`. The two `*_const` functions pin that the mask is
// unconditional — it is present even for a const-declared count that is provably
// in range.

#[cfg(test)]
mod binops_narrow_shift_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn binops_narrow_shift_test() {
        // 10 shift sites, each masking its count: 8 two-operand shifts + the two
        // `*_const` shifts.
        cov_mark::check_count!(wasm_codegen_shift_count_mask, 10);
        // 18 exported narrow parameters normalized: 8 fns x 2 params + 2 `*_const`
        // fns x 1 param.
        cov_mark::check_count!(wasm_codegen_entry_param_normalization, 18);
        let test_name = "binops_narrow_shift";
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
    fn binops_narrow_shift_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "binops_narrow_shift";
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

        // u8 (mask by 7): in-range control, count == width (mod to 0), count >
        // width, count == 2*width, the old mod-32 cliff, and a -1 host count.
        call!("shl_u8", (165_i32, 3_i32), 40_i32);
        call!("shr_u8", (165_i32, 3_i32), 20_i32);
        call!("shl_u8", (165_i32, 8_i32), 165_i32);
        call!("shr_u8", (165_i32, 8_i32), 165_i32);
        call!("shl_u8", (165_i32, 9_i32), 74_i32);
        call!("shr_u8", (165_i32, 9_i32), 82_i32);
        call!("shl_u8", (165_i32, 32_i32), 165_i32);
        call!("shl_u8", (165_i32, -1_i32), 128_i32);
        call!("shr_u8", (165_i32, -1_i32), 1_i32);

        // i8 (mask by 7): the left operand and result stay signed; a -1 host
        // count normalizes to i8 -1, then masks to 7.
        call!("shl_i8", (-91_i32, 7_i32), -128_i32);
        call!("shr_i8", (-91_i32, 1_i32), -46_i32);
        call!("shl_i8", (-91_i32, 8_i32), -91_i32);
        call!("shr_i8", (-91_i32, -1_i32), -1_i32);

        // u16 (mask by 15).
        call!("shl_u16", (42405_i32, 15_i32), 32768_i32);
        call!("shr_u16", (42405_i32, 15_i32), 1_i32);
        call!("shl_u16", (42405_i32, 16_i32), 42405_i32);
        call!("shr_u16", (42405_i32, 17_i32), 21202_i32);
        call!("shl_u16", (42405_i32, -1_i32), 32768_i32);

        // i16 (mask by 15).
        call!("shl_i16", (-23131_i32, 16_i32), -23131_i32);
        call!("shr_i16", (-23131_i32, 15_i32), -1_i32);
        call!("shr_i16", (-23131_i32, -1_i32), -1_i32);

        // Const-declared counts are masked the same way (3 & 7, 15 & 15).
        call!("shl_u8_const", 165_i32, 40_i32);
        call!("shr_i16_const", -23131_i32, -1_i32);
        call!("shr_i16_const", 100_i32, 0_i32);
    }
}

#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("binops_narrow_shift")
    }

    #[test]
    #[ignore]
    fn regenerate_binops_narrow_shift_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("binops_narrow_shift.inf"))
            .expect("Failed to read binops_narrow_shift.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("binops_narrow_shift.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "binops_narrow_shift");
    }
}
