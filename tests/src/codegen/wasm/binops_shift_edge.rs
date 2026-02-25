// Shift operator edge-case tests.
//
// The Inference type-checker requires both operands of a shift to have the same type.
// For i32, we can use literal shift amounts directly. For u32/i64/u64, we pass the
// shift amount as a parameter of the matching type.
//
// WASM bytecode verification (via analyze-wasm-bytecode):
// - shr_s_i32_by1  emits i32.shr_s (signed arithmetic right shift)
// - shr_u_i32      emits i32.shr_u (unsigned logical right shift)
// - shr_s_i64      emits i64.shr_s (signed arithmetic right shift)
// - shr_u_i64      emits i64.shr_u (unsigned logical right shift)
// - shl_i32_by*    emits i32.shl   (left shift, signedness irrelevant)
// - shl_i64        emits i64.shl   (left shift, signedness irrelevant)

#[cfg(test)]
mod binops_shift_edge_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn binops_shift_edge_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 13);
        let test_name = "binops_shift_edge";
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
    fn binops_shift_edge_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "binops_shift_edge";
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

        // --- i32 left shift (literal shift amounts) ---

        // Shift by 0 = identity
        call!("shl_i32_by0", i32, 42_i32, 42_i32);
        call!("shl_i32_by0", i32, 0_i32, 0_i32);
        call!("shl_i32_by0", i32, -1_i32, -1_i32);

        // Shift by 1 = multiply by 2
        call!("shl_i32_by1", i32, 1_i32, 2_i32);
        call!("shl_i32_by1", i32, 0_i32, 0_i32);
        call!("shl_i32_by1", i32, -1_i32, -2_i32);

        // Shift by 31 = set/clear only sign bit
        call!("shl_i32_by31", i32, 1_i32, i32::MIN);
        call!("shl_i32_by31", i32, 0_i32, 0_i32);
        call!("shl_i32_by31", i32, -1_i32, i32::MIN);
        call!("shl_i32_by31", i32, 2_i32, 0_i32);

        // --- i32 arithmetic right shift (literal shift amounts) ---

        // Shift by 0 = identity
        call!("shr_s_i32_by0", i32, 42_i32, 42_i32);
        call!("shr_s_i32_by0", i32, -1_i32, -1_i32);

        // Shift by 1 = divide by 2 with sign extension
        call!("shr_s_i32_by1", i32, 4_i32, 2_i32);
        call!("shr_s_i32_by1", i32, -4_i32, -2_i32);
        call!("shr_s_i32_by1", i32, 1_i32, 0_i32);
        call!("shr_s_i32_by1", i32, -1_i32, -1_i32);

        // Shift by 31 = all bits become sign bit
        call!("shr_s_i32_by31", i32, -1_i32, -1_i32);
        call!("shr_s_i32_by31", i32, 1_i32, 0_i32);
        call!("shr_s_i32_by31", i32, i32::MIN, -1_i32);
        call!("shr_s_i32_by31", i32, i32::MAX, 0_i32);

        // --- u32 logical right shift (parameterized shift amount) ---
        // WASM u32 is i32 at the binary level; wasmtime uses i32 for both.

        // Logical shift by 1: fills with 0
        call!("shr_u_i32", i32, (i32::MIN, 1_i32), 0x4000_0000_i32);
        call!("shr_u_i32", i32, (-1_i32, 1_i32), 0x7FFF_FFFF_i32);
        call!("shr_u_i32", i32, (2_i32, 1_i32), 1_i32);

        // Logical shift by 31: result is just the high bit
        call!("shr_u_i32", i32, (i32::MIN, 31_i32), 1_i32);
        call!("shr_u_i32", i32, (-1_i32, 31_i32), 1_i32);
        call!("shr_u_i32", i32, (0x7FFF_FFFF_i32, 31_i32), 0_i32);

        // Shift by 0 = identity
        call!("shr_u_i32", i32, (42_i32, 0_i32), 42_i32);

        // --- u32 left shift (parameterized) ---

        call!("shl_u32", i32, (42_i32, 0_i32), 42_i32);
        call!("shl_u32", i32, (-1_i32, 0_i32), -1_i32);
        call!("shl_u32", i32, (1_i32, 1_i32), 2_i32);
        // u32 shift right by 1 clears the high bit
        call!("shr_u_i32", i32, (i32::MIN, 1_i32), 0x4000_0000_i32);

        // --- i64 left shift (parameterized) ---

        // Shift by 0 = identity
        call!("shl_i64", i64, (42_i64, 0_i64), 42_i64);
        call!("shl_i64", i64, (-1_i64, 0_i64), -1_i64);

        // Shift by 63 = only sign bit of result
        call!("shl_i64", i64, (1_i64, 63_i64), i64::MIN);
        call!("shl_i64", i64, (0_i64, 63_i64), 0_i64);
        call!("shl_i64", i64, (-1_i64, 63_i64), i64::MIN);
        call!("shl_i64", i64, (2_i64, 63_i64), 0_i64);

        // Shift by 1 = multiply by 2
        call!("shl_i64", i64, (1_i64, 1_i64), 2_i64);

        // --- i64 arithmetic right shift (parameterized) ---

        call!("shr_s_i64", i64, (4_i64, 1_i64), 2_i64);
        call!("shr_s_i64", i64, (-4_i64, 1_i64), -2_i64);
        call!("shr_s_i64", i64, (-1_i64, 1_i64), -1_i64);

        // Shift by 63 = all bits become sign bit
        call!("shr_s_i64", i64, (i64::MIN, 63_i64), -1_i64);
        call!("shr_s_i64", i64, (i64::MAX, 63_i64), 0_i64);
        call!("shr_s_i64", i64, (-1_i64, 63_i64), -1_i64);
        call!("shr_s_i64", i64, (1_i64, 63_i64), 0_i64);

        // Shift by 0 = identity
        call!("shr_s_i64", i64, (42_i64, 0_i64), 42_i64);

        // --- u64 logical right shift (parameterized) ---

        call!("shr_u_i64", i64, (i64::MIN, 1_i64), 0x4000_0000_0000_0000_i64);
        call!("shr_u_i64", i64, (-1_i64, 1_i64), i64::MAX);
        call!("shr_u_i64", i64, (2_i64, 1_i64), 1_i64);

        // Logical shift by 63: result is just the high bit
        call!("shr_u_i64", i64, (i64::MIN, 63_i64), 1_i64);
        call!("shr_u_i64", i64, (-1_i64, 63_i64), 1_i64);
        call!("shr_u_i64", i64, (i64::MAX, 63_i64), 0_i64);
        call!("shr_u_i64", i64, (1_i64, 63_i64), 0_i64);

        // Shift by 0 = identity
        call!("shr_u_i64", i64, (42_i64, 0_i64), 42_i64);

        // --- Variable shift amounts for i32 (WASM wraps modulo bitwidth) ---

        call!("shl_by_amount", i32, (1_i32, 3_i32), 8_i32);
        call!("shl_by_amount", i32, (1_i32, 0_i32), 1_i32);
        call!("shl_by_amount", i32, (1_i32, 31_i32), i32::MIN);
        // WASM wraps: shift by 32 on i32 = shift by (32 & 31) = shift by 0
        call!("shl_by_amount", i32, (1_i32, 32_i32), 1_i32);
        call!("shl_by_amount", i32, (5_i32, 33_i32), 10_i32);

        call!("shr_by_amount", i32, (-4_i32, 1_i32), -2_i32);
        call!("shr_by_amount", i32, (42_i32, 0_i32), 42_i32);
        // WASM wraps: shift by 32 on i32 = shift by 0
        call!("shr_by_amount", i32, (42_i32, 32_i32), 42_i32);
    }
}

/// Test data regeneration helper.
///
/// Regenerates the expected `.wasm` golden file from the current compiler output.
/// Run with `--ignored` flag:
///
/// ```bash
/// cargo test -p inference-tests codegen::wasm::binops_shift_edge::regenerate -- --ignored
/// ```
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("binops_shift_edge")
            .join("binops_shift_edge")
    }

    #[test]
    #[ignore]
    fn regenerate_binops_shift_edge_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("binops_shift_edge.inf"))
            .expect("Failed to read binops_shift_edge.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("binops_shift_edge.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "binops_shift_edge");
    }
}
