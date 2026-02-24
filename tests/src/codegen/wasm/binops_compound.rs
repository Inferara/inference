// Compound expression tests: multi-operator expressions exercising the codegen pipeline
// with complex expression trees (4+ operators per function).
//
// WASM bytecode analysis:
//
// quadratic(a, b, c, x) -> a*x*x + b*x + c
//   local.get $a, local.get $x, i32.mul, local.get $x, i32.mul,
//   local.get $b, local.get $x, i32.mul, i32.add, local.get $c, i32.add, end
//   (5 binary ops: 3 mul + 2 add)
//
// compound_arith_chain(a, b) -> ((a+b)*(a-b)) + (a*a - b*b)
//   local.get $a, local.get $b, i32.add, local.get $a, local.get $b, i32.sub,
//   i32.mul, local.get $a, local.get $a, i32.mul, local.get $b, local.get $b,
//   i32.mul, i32.sub, i32.add, end
//   (7 binary ops: 3 mul + 2 sub + 2 add; equals 2*(a^2-b^2) by algebraic identity)
//
// Total across all 16 functions: 73 binary expressions, 34 parenthesized expressions,
// 7 variable definitions (manhattan_distance: 4, multi_let_chain: 3), 38 function params.

#[cfg(test)]
mod binops_compound_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, get_test_file_path, get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn binops_compound_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 73);
        cov_mark::check_count!(wasm_codegen_emit_parenthesized_expression, 34);
        cov_mark::check_count!(wasm_codegen_emit_variable_definition, 7);
        cov_mark::check_count!(wasm_codegen_emit_function_params, 38);
        let test_name = "binops_compound";
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
    fn binops_compound_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "binops_compound";
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

        // --- quadratic: a*x*x + b*x + c ---

        // Roots of x^2 - 3x + 2: quadratic(1, -3, 2, x) = 0 for x=1, x=2
        call!("quadratic", i32, (1_i32, -3_i32, 2_i32, 1_i32), 0_i32);
        call!("quadratic", i32, (1_i32, -3_i32, 2_i32, 2_i32), 0_i32);
        // x^2 at x=5
        call!("quadratic", i32, (1_i32, 0_i32, 0_i32, 5_i32), 25_i32);
        // 2x^2 + 3x + 1 at x=0
        call!("quadratic", i32, (2_i32, 3_i32, 1_i32, 0_i32), 1_i32);
        // 2x^2 + 3x + 1 at x=10
        call!("quadratic", i32, (2_i32, 3_i32, 1_i32, 10_i32), 231_i32);
        // Negative coefficient: -x^2 at x=3
        call!("quadratic", i32, (-1_i32, 0_i32, 0_i32, 3_i32), -9_i32);

        // --- manhattan_distance: |x2-x1| + |y2-y1| via bitwise abs ---

        call!("manhattan_distance", i32, (0_i32, 0_i32, 3_i32, 4_i32), 7_i32);
        call!("manhattan_distance", i32, (0_i32, 0_i32, 0_i32, 0_i32), 0_i32);
        call!("manhattan_distance", i32, (3_i32, 4_i32, 0_i32, 0_i32), 7_i32);
        call!("manhattan_distance", i32, (-1_i32, -1_i32, 1_i32, 1_i32), 4_i32);
        call!("manhattan_distance", i32, (5_i32, 5_i32, 5_i32, 5_i32), 0_i32);

        // --- bitwise_avg: (a & b) + ((a ^ b) >> 1) ---

        call!("bitwise_avg", i32, (4_i32, 6_i32), 5_i32);
        call!("bitwise_avg", i32, (10_i32, 10_i32), 10_i32);
        call!("bitwise_avg", i32, (0_i32, 0_i32), 0_i32);
        call!("bitwise_avg", i32, (100_i32, 200_i32), 150_i32);
        call!("bitwise_avg", i32, (1_i32, 2_i32), 1_i32);

        // --- swap_nibbles: ((x & 0xF) << 4) | ((x >> 4) & 0xF) ---

        call!("swap_nibbles", i32, 0xAB_i32, 0xBA_i32);
        call!("swap_nibbles", i32, 0x00_i32, 0x00_i32);
        call!("swap_nibbles", i32, 0xFF_i32, 0xFF_i32);
        call!("swap_nibbles", i32, 0x12_i32, 0x21_i32);

        // --- count_high_bits: (x >> 16) & 65535 ---

        call!("count_high_bits", i32, 0x0000_i32, 0_i32);
        call!("count_high_bits", i32, 0x0001_0000_i32, 1_i32);
        call!("count_high_bits", i32, 0x00FF_0000_i32, 0xFF_i32);

        // --- pack_bytes / unpack_hi / unpack_lo ---

        call!("pack_bytes", i32, (0xAB_i32, 0xCD_i32), 0xABCD_i32);
        call!("pack_bytes", i32, (0x00_i32, 0x00_i32), 0x0000_i32);
        call!("pack_bytes", i32, (0xFF_i32, 0xFF_i32), 0xFFFF_i32);
        call!("pack_bytes", i32, (0x12_i32, 0x34_i32), 0x1234_i32);

        call!("unpack_hi", i32, 0xABCD_i32, 0xAB_i32);
        call!("unpack_hi", i32, 0x0000_i32, 0x00_i32);
        call!("unpack_hi", i32, 0xFFFF_i32, 0xFF_i32);

        call!("unpack_lo", i32, 0xABCD_i32, 0xCD_i32);
        call!("unpack_lo", i32, 0x0000_i32, 0x00_i32);
        call!("unpack_lo", i32, 0xFFFF_i32, 0xFF_i32);

        // --- dot_product: ax*bx + ay*by ---

        // Perpendicular unit vectors: dot = 0
        call!("dot_product", i32, (1_i32, 0_i32, 0_i32, 1_i32), 0_i32);
        // Parallel unit vectors: dot = 1
        call!("dot_product", i32, (1_i32, 0_i32, 1_i32, 0_i32), 1_i32);
        // (3,4) . (3,4) = 9 + 16 = 25
        call!("dot_product", i32, (3_i32, 4_i32, 3_i32, 4_i32), 25_i32);
        // Anti-parallel: (1,0) . (-1,0) = -1
        call!("dot_product", i32, (1_i32, 0_i32, -1_i32, 0_i32), -1_i32);

        // --- cross_product: ax*by - ay*bx ---

        // (1,0) x (0,1) = 1
        call!("cross_product", i32, (1_i32, 0_i32, 0_i32, 1_i32), 1_i32);
        // (0,1) x (1,0) = -1
        call!("cross_product", i32, (0_i32, 1_i32, 1_i32, 0_i32), -1_i32);
        // Parallel vectors: cross = 0
        call!("cross_product", i32, (2_i32, 3_i32, 2_i32, 3_i32), 0_i32);

        // --- bit_reverse_nibble: reverse 4-bit nibble ---

        // 0b0001 -> 0b1000
        call!("bit_reverse_nibble", i32, 0x1_i32, 0x8_i32);
        // 0b1010 -> 0b0101
        call!("bit_reverse_nibble", i32, 0xA_i32, 0x5_i32);
        // 0b1111 -> 0b1111
        call!("bit_reverse_nibble", i32, 0xF_i32, 0xF_i32);
        // 0b0000 -> 0b0000
        call!("bit_reverse_nibble", i32, 0x0_i32, 0x0_i32);
        // 0b0110 -> 0b0110
        call!("bit_reverse_nibble", i32, 0x6_i32, 0x6_i32);
        // 0b0100 -> 0b0010
        call!("bit_reverse_nibble", i32, 0x4_i32, 0x2_i32);

        // --- fibonacci_step: a + b ---

        call!("fibonacci_step", i32, (0_i32, 1_i32), 1_i32);
        call!("fibonacci_step", i32, (1_i32, 1_i32), 2_i32);
        call!("fibonacci_step", i32, (5_i32, 8_i32), 13_i32);

        // --- compound_arith_chain: ((a+b)*(a-b)) + (a*a - b*b) = 2*(a^2 - b^2) ---

        call!("compound_arith_chain", i32, (5_i32, 3_i32), 32_i32);
        call!("compound_arith_chain", i32, (3_i32, 3_i32), 0_i32);
        call!("compound_arith_chain", i32, (0_i32, 0_i32), 0_i32);
        call!("compound_arith_chain", i32, (10_i32, 1_i32), 198_i32);

        // --- multi_let_chain: x = a+b, y = x*c, z = y-a, return z+b ---

        // a=2, b=3, c=4: x=5, y=20, z=18, return 21
        call!("multi_let_chain", i32, (2_i32, 3_i32, 4_i32), 21_i32);
        // a=0, b=0, c=0: x=0, y=0, z=0, return 0
        call!("multi_let_chain", i32, (0_i32, 0_i32, 0_i32), 0_i32);
        // a=1, b=1, c=1: x=2, y=2, z=1, return 2
        call!("multi_let_chain", i32, (1_i32, 1_i32, 1_i32), 2_i32);
        // a=10, b=20, c=3: x=30, y=90, z=80, return 100
        call!("multi_let_chain", i32, (10_i32, 20_i32, 3_i32), 100_i32);

        // --- i64_compound: (a+b)*c - (a*b + c) ---

        // a=1, b=2, c=3: (3)*3 - (2+3) = 9-5 = 4
        call!("i64_compound", i64, (1_i64, 2_i64, 3_i64), 4_i64);
        // a=0, b=0, c=0: 0*0 - (0+0) = 0
        call!("i64_compound", i64, (0_i64, 0_i64, 0_i64), 0_i64);
        // a=10, b=20, c=5: 30*5 - (200+5) = 150-205 = -55
        call!("i64_compound", i64, (10_i64, 20_i64, 5_i64), -55_i64);

        // --- mixed_cmp_arith: (a*b + c) > (a + b*c) ---

        // a=5, b=2, c=1: (10+1)=11 > (5+2)=7 -> true
        call!("mixed_cmp_arith", i32, (5_i32, 2_i32, 1_i32), 1_i32);
        // a=1, b=1, c=1: (1+1)=2 > (1+1)=2 -> false (not strictly greater)
        call!("mixed_cmp_arith", i32, (1_i32, 1_i32, 1_i32), 0_i32);
        // a=1, b=2, c=5: (2+5)=7 > (1+10)=11 -> false
        call!("mixed_cmp_arith", i32, (1_i32, 2_i32, 5_i32), 0_i32);
        // a=10, b=1, c=1: (10+1)=11 > (10+1)=11 -> false
        call!("mixed_cmp_arith", i32, (10_i32, 1_i32, 1_i32), 0_i32);
        // a=3, b=2, c=1: (6+1)=7 > (3+2)=5 -> true
        call!("mixed_cmp_arith", i32, (3_i32, 2_i32, 1_i32), 1_i32);
    }
}

/// Test data regeneration helper.
///
/// Regenerates the expected `.wasm` golden file from the current compiler output.
/// Run with `--ignored` flag:
///
/// ```bash
/// cargo test -p inference-tests codegen::wasm::binops_compound::regenerate -- --ignored
/// ```
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, wasm_codegen};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("binops_compound")
            .join("binops_compound")
    }

    #[test]
    #[ignore]
    fn regenerate_binops_compound_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("binops_compound.inf"))
            .expect("Failed to read binops_compound.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("binops_compound.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
    }
}
