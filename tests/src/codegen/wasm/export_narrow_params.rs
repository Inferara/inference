// Entry parameter normalization for exported functions (see
// `export_narrow_params.inf`).
//
// An exported function is the module's WebAssembly ABI boundary, where a host
// may pass any i32 bit pattern for a narrow scalar parameter. Each exported
// function canonicalizes its narrow parameters in the entry prologue before the
// body runs: `u8`/`u16` take the argument's low bits, `i8`/`i16` sign-extend
// from the low bits, and `bool` normalizes by truthiness (any nonzero host
// value is `true`). In-language callers always pass canonical values and every
// normalization is a fixed point on them, so a private helper — which is never
// an ABI boundary — carries no prologue, and an in-domain argument is unchanged.
//
// The execution suite is the behavioral point: it feeds genuinely out-of-domain
// arguments (`300` for a `u8`, `2` for a `bool`, `70000` for a `u16`) through
// the exported entry and pins that each is canonicalized before the body sees
// it — every row would have observed the raw host value before this change.

#[cfg(test)]
mod export_narrow_params_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn export_narrow_params_test() {
        // 13 prologue-emitting parameters: id_* (5) + gt100_u8 + is_neg_i8 +
        // bool_if + bool_eq_true + bool_and_pass + call_helper (6) + mixed's `a`
        // and `b` (2). The private `helper_u8` is not an export, so it emits none.
        cov_mark::check_count!(wasm_codegen_entry_param_normalization, 13);
        let test_name = "export_narrow_params";
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
    fn export_narrow_params_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "export_narrow_params";
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

        // u8 masks to the low byte (zero-extend).
        call!("id_u8", 300_i32, 44_i32);
        call!("id_u8", 255_i32, 255_i32);
        call!("id_u8", 256_i32, 0_i32);
        call!("id_u8", -1_i32, 255_i32);

        // i8 sign-extends from the low byte.
        call!("id_i8", 200_i32, -56_i32);
        call!("id_i8", -200_i32, 56_i32);
        call!("id_i8", 128_i32, -128_i32);
        call!("id_i8", -128_i32, -128_i32);
        call!("id_i8", 127_i32, 127_i32);

        // u16 masks to the low 16 bits.
        call!("id_u16", 65536_i32, 0_i32);
        call!("id_u16", 70000_i32, 4464_i32);
        call!("id_u16", 65535_i32, 65535_i32);

        // i16 sign-extends from the low 16 bits.
        call!("id_i16", 32768_i32, -32768_i32);
        call!("id_i16", 70000_i32, 4464_i32);
        call!("id_i16", -32768_i32, -32768_i32);

        // bool normalizes by truthiness: any nonzero host value is `true`.
        call!("id_bool", 0_i32, 0_i32);
        call!("id_bool", 1_i32, 1_i32);
        call!("id_bool", 2_i32, 1_i32);
        call!("id_bool", -2_i32, 1_i32);
        call!("id_bool", i32::MIN, 1_i32);

        // The normalized value flows into the body: `v > 100` sees 44, not 300.
        call!("gt100_u8", 300_i32, 0_i32);
        call!("gt100_u8", 150_i32, 1_i32);
        call!("gt100_u8", 100_i32, 0_i32);

        // Sign-extension flows into the body: `200` sign-extends to -56 (< 0).
        call!("is_neg_i8", 200_i32, 1_i32);
        call!("is_neg_i8", 56_i32, 0_i32);
        call!("is_neg_i8", -200_i32, 0_i32);
        call!("is_neg_i8", -1_i32, 1_i32);

        // A host `bool` of 2 is consistent across every bool consumer: `if b`,
        // `b == true`, and the `&&` pass-through all see canonical 1.
        call!("bool_if", 2_i32, 1_i32);
        call!("bool_eq_true", 2_i32, 1_i32);
        call!("bool_and_pass", 2_i32, 1_i32);
        call!("bool_if", 0_i32, 0_i32);
        call!("bool_eq_true", 0_i32, 0_i32);
        call!("bool_and_pass", 0_i32, 0_i32);
        call!("bool_if", 1_i32, 1_i32);
        call!("bool_eq_true", 1_i32, 1_i32);
        call!("bool_and_pass", 1_i32, 1_i32);

        // Normalization happens at the exported boundary, not in the private
        // helper: `call_helper(300)` masks `a` to 44, then calls `helper_u8`.
        call!("call_helper", 300_i32, 44_i32);
        call!("call_helper", 7_i32, 7_i32);

        // `mixed(a: u8, x: i32, b: bool)`: the i32 parameter `x` is passed
        // through unchanged (never normalized), while `a` and `b` are. A host
        // `b` of 2 takes the `return x` branch; a canonicalized `a` of 44 (from
        // 300) is `> 0`.
        call!("mixed", (5_i32, 42_i32, 2_i32), 42_i32);
        call!("mixed", (300_i32, 42_i32, 1_i32), 42_i32);
        call!("mixed", (300_i32, 42_i32, 0_i32), 1_i32);
        call!("mixed", (0_i32, 42_i32, 0_i32), 0_i32);
    }
}

#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("export_narrow_params")
    }

    #[test]
    #[ignore]
    fn regenerate_export_narrow_params_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("export_narrow_params.inf"))
            .expect("Failed to read export_narrow_params.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("export_narrow_params.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "export_narrow_params");
    }
}
