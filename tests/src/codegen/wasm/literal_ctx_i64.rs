/// WASM bytecode verification for contextually typed integer literals.
///
/// Every function here spells its constants as bare literals in the position
/// the issue reported failing — a shift count, an arithmetic operand, a
/// comparison operand, a call argument, a `return` operand — and each one is
/// verified to compute at the declared width rather than at `i32`:
/// - `shift_by_literal`         -> i64.shl with an i64.const count
/// - `add_literal`              -> i64.add
/// - `compare_with_literal`     -> i64.lt_s
/// - `call_with_literal`        -> i64 argument to `scale`
/// - `return_literal`           -> i64.const
/// - `return_glued_negative`    -> i64.const (the glued `-42` is one token)
/// - `return_spaced_negation`   -> i64.sub against an i64 operand
/// - `complement_literal`       -> i64.xor against -1
/// - `parenthesized_literal`    -> i64.const through parentheses
/// - `shift_of_two_literals`    -> both operands typed by the return type
/// - `nested_literal_expression`-> descent through parens, `-` and operators
/// - `max_u64_argument`         -> a value with no i32 reading at all
/// - `narrow_peer`              -> u8 peer typing
/// - `fixed_*`                  -> a Q16.16 kernel whose scale and rounding
///   term are bare literals
///
/// The unsigned and narrow cases pin opcode *selection*, which the literal's
/// width decides — a signed reading of the same source computes differently:
/// - `udiv_right`, `udiv_left`  -> i64.div_u (signed would differ on u64::MAX)
/// - `ucmp_left`                -> i64.gt_u  (`1000 > u64::MAX` is false;
///   signed would read the operand as -1 and answer true)
/// - `ushr_max`                 -> i64.shr_u on a literal left operand
/// - `narrow_div_left`          -> i32.div_u at u8 width
/// - `narrow_wrap_const`        -> `200 + 100` evaluated at u8, wrapping to 44
#[cfg(test)]
mod literal_ctx_i64_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, regenerate_wat, wasm_codegen,
    };

    const TEST_NAME: &str = "literal_ctx_i64";

    fn source() -> String {
        let path = get_test_file_path(module_path!(), TEST_NAME);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {path:?}"))
    }

    #[test]
    fn literal_ctx_i64_test() {
        let actual = wasm_codegen(&source());
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {e}"));
        let expected = get_test_wasm_path(module_path!(), TEST_NAME);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {TEST_NAME}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), TEST_NAME);
    }

    #[test]
    fn literal_ctx_i64_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let wasm_bytes = wasm_codegen(&source());
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

        // shift_by_literal: a << 16 at 64 bits — an i32 shift would wrap the
        // count and lose the high bits entirely.
        call!("shift_by_literal", i64, 3_i64, 196_608_i64);
        call!("shift_by_literal", i64, 1_i64, 65_536_i64);
        call!("shift_by_literal", i64, 1_i64 << 40, 1_i64 << 56);
        call!("shift_by_literal", i64, 0_i64, 0_i64);

        // add_literal: a + 65536
        call!("add_literal", i64, 1_i64, 65_537_i64);
        call!("add_literal", i64, 1_i64 << 40, (1_i64 << 40) + 65_536);
        call!("add_literal", i64, -65_536_i64, 0_i64);

        // compare_with_literal: a < 65536
        call!("compare_with_literal", i64, 1_i64, 1_i64);
        call!("compare_with_literal", i64, 65_535_i64, 1_i64);
        call!("compare_with_literal", i64, 65_536_i64, 0_i64);
        call!("compare_with_literal", i64, 1_i64 << 40, 0_i64);
        call!("compare_with_literal", i64, i64::MIN, 1_i64);

        // call_with_literal: scale(a, 65536)
        call!("call_with_literal", i64, 2_i64, 131_072_i64);
        call!("call_with_literal", i64, 0_i64, 0_i64);
        call!("call_with_literal", i64, -3_i64, -196_608_i64);

        call!("return_literal", i64, (), 65_536_i64);
        call!("return_glued_negative", i64, (), -42_i64);
        call!("return_spaced_negation", i64, (), -42_i64);
        call!("parenthesized_literal", i64, (), 65_536_i64);
        call!("complement_literal", i64, (), -1_i64);

        // shift_of_two_literals: 1 << 40 — at i32 this shift is meaningless.
        call!("shift_of_two_literals", i64, (), 1_099_511_627_776_i64);

        // nested_literal_expression: -(65536 + (1 << 40))
        call!(
            "nested_literal_expression",
            i64,
            (),
            -(65_536_i64 + (1_i64 << 40))
        );

        // max_u64_argument: u64::MAX has no i32 reading; it reaches the
        // parameter only because the parameter's type reaches it.
        call!("max_u64_argument", i64, (), -1_i64);

        // narrow_peer: x + 1 at u8, where the literal takes the narrow width.
        call!("narrow_peer", i32, 200_i32, 201_i32);
        call!("narrow_peer", i32, 0_i32, 1_i32);

        // Q16.16 fixed point.
        call!("fixed_one", i64, (), 65_536_i64);
        call!("fixed_from_int", i64, 3_i64, 196_608_i64);
        call!("fixed_from_int", i64, -1_i64, -65_536_i64);
        // 2.0 * 3.0 = 6.0
        call!("fixed_mul", i64, (131_072_i64, 196_608_i64), 393_216_i64);
        // 6.0 / 3.0 = 2.0
        call!("fixed_div", i64, (393_216_i64, 196_608_i64), 131_072_i64);
        // 2.0 rounds to 2; 2.5 rounds to 3.
        call!("fixed_round_to_int", i64, 131_072_i64, 2_i64);
        call!("fixed_round_to_int", i64, 163_840_i64, 3_i64);

        // Unsigned and narrow widths. Wasmtime carries u64 as i64, so -1 is
        // u64::MAX; each of these answers differently under a signed opcode.
        // u64::MAX / 3
        call!("udiv_right", i64, -1_i64, 6_148_914_691_236_517_205_i64);
        call!("udiv_right", i64, 9_i64, 3_i64);
        // 1000 > u64::MAX is false; signed would compare against -1 and say true.
        call!("ucmp_left", i32, -1_i64, 0_i32);
        call!("ucmp_left", i32, 999_i64, 1_i32);
        // u64::MAX >> 1
        call!("ushr_max", i64, 1_i64, 9_223_372_036_854_775_807_i64);
        call!("ushr_max", i64, 0_i64, -1_i64);
        call!("udiv_left", i64, 7_i64, 142_i64);
        call!("narrow_div_left", i32, 3_i32, 66_i32);
        // 200 + 100 at u8 wraps to 44.
        call!("narrow_wrap_const", i32, (), 44_i32);
    }

    #[test]
    #[ignore]
    fn regenerate_literal_ctx_i64_wasm() {
        let actual = wasm_codegen(&source());
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {e}"));
        let wasm_path = get_test_wasm_path(module_path!(), TEST_NAME);
        let dir = wasm_path.parent().unwrap();
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, dir, TEST_NAME);
    }
}
