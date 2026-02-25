// WASM bytecode verification (via analyze-wasm-bytecode):
//
// bitand_i64:           local.get 0, local.get 1, i64.and
// bitor_i64:            local.get 0, local.get 1, i64.or
// bitxor_i64:           local.get 0, local.get 1, i64.xor
// shl_i64:              local.get 0, local.get 1, i64.shl
// shr_i64_signed:       local.get 0, local.get 1, i64.shr_s  (arithmetic shift)
// shr_u64:              local.get 0, local.get 1, i64.shr_u  (logical shift)
// bitnot_i64:           local.get 0, i64.const -1, i64.xor   (unary ~x = x ^ -1)
// bitand_mask_i64:      i64.const 255, local.set 1, local.get 0, local.get 1, i64.and
// bitor_set_bit_i64:    i64.const 1, local.set 1, local.get 0, local.get 1, i64.or
// bitxor_flip_i64:      i64.const -1, local.set 1, local.get 0, local.get 1, i64.xor
// shl_by_one_i64:       i64.const 1, local.set 1, local.get 0, local.get 1, i64.shl
// shr_signed_negative:  i64.const 1, local.set 1, local.get 0, local.get 1, i64.shr_s
// shr_unsigned_high:    i64.const 1, local.set 1, local.get 0, local.get 1, i64.shr_u
// bitand_bitor_chain:   local.get 0, local.get 1, i64.and, local.get 2, i64.or
// mask_and_shift:       i64.const 4, local.set 2, i64.const 15, local.set 3,
//                       local.get 0, local.get 2, i64.shr_s, local.get 3, i64.and

#[cfg(test)]
mod binops_i64_bitwise_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn binops_i64_bitwise_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 16);
        cov_mark::check_count!(wasm_codegen_emit_prefix_unary_expression, 1);
        cov_mark::check!(wasm_codegen_emit_unary_bitnot);
        cov_mark::check_count!(wasm_codegen_emit_variable_definition, 8);
        cov_mark::check_count!(wasm_codegen_emit_parenthesized_expression, 2);
        let test_name = "binops_i64_bitwise";
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
    fn binops_i64_bitwise_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "binops_i64_bitwise";
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

        // bitand_i64: basic AND
        call!("bitand_i64", i64, (0xFF00FF00_i64, 0x00FF00FF_i64), 0_i64);
        call!("bitand_i64", i64, (-1_i64, 0x1234_i64), 0x1234_i64);
        call!("bitand_i64", i64, (0_i64, -1_i64), 0_i64);
        call!("bitand_i64", i64, (-1_i64, -1_i64), -1_i64);

        // bitor_i64: basic OR
        call!("bitor_i64", i64, (0xFF00_i64, 0x00FF_i64), 0xFFFF_i64);
        call!("bitor_i64", i64, (0_i64, 0_i64), 0_i64);
        call!("bitor_i64", i64, (-1_i64, 0_i64), -1_i64);
        call!("bitor_i64", i64, (0x1000_i64, 0x0001_i64), 0x1001_i64);

        // bitxor_i64: basic XOR
        call!("bitxor_i64", i64, (0xFFFF_i64, 0xFF00_i64), 0x00FF_i64);
        call!("bitxor_i64", i64, (0_i64, 0_i64), 0_i64);
        call!("bitxor_i64", i64, (-1_i64, -1_i64), 0_i64);
        call!("bitxor_i64", i64, (0xAAAA_i64, 0x5555_i64), 0xFFFF_i64);

        // shl_i64: left shift
        call!("shl_i64", i64, (1_i64, 0_i64), 1_i64);
        call!("shl_i64", i64, (1_i64, 1_i64), 2_i64);
        call!("shl_i64", i64, (1_i64, 63_i64), i64::MIN);
        call!("shl_i64", i64, (0xFF_i64, 8_i64), 0xFF00_i64);

        // shr_i64_signed: arithmetic right shift (sign-extends)
        call!("shr_i64_signed", i64, (i64::MIN, 63_i64), -1_i64);
        call!("shr_i64_signed", i64, (-2_i64, 1_i64), -1_i64);
        call!("shr_i64_signed", i64, (0x7FFF_FFFF_FFFF_FFFF_i64, 63_i64), 0_i64);
        call!("shr_i64_signed", i64, (16_i64, 2_i64), 4_i64);

        // shr_u64: logical right shift (zero-fills)
        // Note: wasmtime TypedFunc uses i64 for both i64 and u64 at WASM level
        call!("shr_u64", i64, (i64::MIN, 63_i64), 1_i64);
        call!("shr_u64", i64, (-1_i64, 1_i64), 0x7FFF_FFFF_FFFF_FFFF_i64);
        call!("shr_u64", i64, (0xFF00_i64, 8_i64), 0xFF_i64);
        call!("shr_u64", i64, (0_i64, 10_i64), 0_i64);

        // bitnot_i64: unary bitwise NOT (~x = x ^ -1)
        call!("bitnot_i64", i64, 0_i64, -1_i64);
        call!("bitnot_i64", i64, -1_i64, 0_i64);
        call!("bitnot_i64", i64, 1_i64, -2_i64);
        call!("bitnot_i64", i64, i64::MIN, i64::MAX);

        // bitand_mask_i64: mask lower byte (a & 0xFF)
        call!("bitand_mask_i64", i64, 0x1234_i64, 0x34_i64);
        call!("bitand_mask_i64", i64, 0xFF_i64, 0xFF_i64);
        call!("bitand_mask_i64", i64, 0x100_i64, 0_i64);
        call!("bitand_mask_i64", i64, -1_i64, 0xFF_i64);

        // bitor_set_bit_i64: set lowest bit (a | 1)
        call!("bitor_set_bit_i64", i64, 0_i64, 1_i64);
        call!("bitor_set_bit_i64", i64, 2_i64, 3_i64);
        call!("bitor_set_bit_i64", i64, 1_i64, 1_i64);
        call!("bitor_set_bit_i64", i64, -2_i64, -1_i64);

        // bitxor_flip_i64: flip all bits (a ^ -1, equivalent to ~a)
        call!("bitxor_flip_i64", i64, 0_i64, -1_i64);
        call!("bitxor_flip_i64", i64, -1_i64, 0_i64);
        call!("bitxor_flip_i64", i64, i64::MIN, i64::MAX);

        // shl_by_one_i64: multiply by 2 (a << 1)
        call!("shl_by_one_i64", i64, 1_i64, 2_i64);
        call!("shl_by_one_i64", i64, 0_i64, 0_i64);
        call!("shl_by_one_i64", i64, 21_i64, 42_i64);
        call!("shl_by_one_i64", i64, i64::MAX, -2_i64);

        // shr_signed_negative_i64: signed shift right by 1 (a >> 1)
        call!("shr_signed_negative_i64", i64, -1_i64, -1_i64);
        call!("shr_signed_negative_i64", i64, -100_i64, -50_i64);
        call!("shr_signed_negative_i64", i64, 100_i64, 50_i64);
        call!("shr_signed_negative_i64", i64, i64::MIN, -4611686018427387904_i64);

        // shr_unsigned_high_bit_i64: unsigned shift right by 1 (should clear high bit)
        call!("shr_unsigned_high_bit_i64", i64, i64::MIN, 0x4000_0000_0000_0000_i64);
        call!("shr_unsigned_high_bit_i64", i64, -1_i64, 0x7FFF_FFFF_FFFF_FFFF_i64);
        call!("shr_unsigned_high_bit_i64", i64, 2_i64, 1_i64);

        // bitand_bitor_chain_i64: (a & b) | c
        call!("bitand_bitor_chain_i64", i64, (0xFF_i64, 0x0F_i64, 0xF0_i64), 0xFF_i64);
        call!("bitand_bitor_chain_i64", i64, (0_i64, 0_i64, 0_i64), 0_i64);
        call!("bitand_bitor_chain_i64", i64, (-1_i64, 0_i64, 0xFF_i64), 0xFF_i64);

        // mask_and_shift_i64: extract nibble at position 4: (a >> 4) & 0xF
        call!("mask_and_shift_i64", i64, 0xABCD_i64, 0xC_i64);
        call!("mask_and_shift_i64", i64, 0_i64, 0_i64);
        call!("mask_and_shift_i64", i64, 0xFFFF_i64, 0xF_i64);
        call!("mask_and_shift_i64", i64, 0x1234_5678_i64, 0x7_i64);
    }

    #[test]
    #[ignore]
    fn regenerate_binops_i64_bitwise_wasm() {
        use crate::utils::{get_test_data_path, regenerate_wat};

        let dir = get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("binops_i64_bitwise")
            .join("binops_i64_bitwise");
        let source_code = std::fs::read_to_string(dir.join("binops_i64_bitwise.inf"))
            .expect("Failed to read binops_i64_bitwise.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {e}"));
        let wasm_path = dir.join("binops_i64_bitwise.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "binops_i64_bitwise");
    }
}
