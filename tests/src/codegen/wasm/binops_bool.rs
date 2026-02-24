// WASM bytecode verified via binary inspection (opcode hex values):
//
// and3(a,b,c):        local.get 0, local.get 1, i32.and(0x71), local.get 2, i32.and(0x71)
//                     Two i32.and for chained &&.
// or3(a,b,c):         local.get 0, local.get 1, i32.or(0x72),  local.get 2, i32.or(0x72)
//                     Two i32.or for chained ||.
// de_morgan_and(a,b): local.get 0, local.get 1, i32.and(0x71), i32.eqz(0x45)
//                     Single i32.eqz for outer !, applied to i32.and result.
// de_morgan_or(a,b):  local.get 0, local.get 1, i32.or(0x72),  i32.eqz(0x45)
//                     Single i32.eqz for outer !, applied to i32.or result.
// not_and_or(a,b,c):  local.get 0, i32.eqz(0x45), local.get 1, local.get 2, i32.or(0x72), i32.and(0x71)
//                     i32.eqz on first operand (!a), then i32.or and i32.and.
// implies(a,b):       local.get 0, i32.eqz(0x45), local.get 1, i32.or(0x72)
//                     !a lowered as i32.eqz, then i32.or with b.
// xor_bool(a,b):      (a||b) lowered as i32.or, then !(a&&b) lowered as i32.and + i32.eqz,
//                     final i32.and combines the two sub-results.
// between(x,lo,hi):   i32.ge_s(0x4E) + i32.le_s(0x4C) + i32.and(0x71)
// all_same_sign(a,b):  Two i32.ge_s(0x4E) comparisons with 0, then i32.eq(0x46)

#[cfg(test)]
mod binops_bool_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, get_test_file_path, get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn binops_bool_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 38);
        cov_mark::check_count!(wasm_codegen_emit_prefix_unary_expression, 5);
        cov_mark::check_count!(wasm_codegen_emit_parenthesized_expression, 20);
        let test_name = "binops_bool";
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
    fn binops_bool_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "binops_bool";
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

        // and3: exhaustive truth table for 3-input AND
        call!("and3", i32, (0_i32, 0_i32, 0_i32), 0_i32);
        call!("and3", i32, (0_i32, 0_i32, 1_i32), 0_i32);
        call!("and3", i32, (0_i32, 1_i32, 0_i32), 0_i32);
        call!("and3", i32, (0_i32, 1_i32, 1_i32), 0_i32);
        call!("and3", i32, (1_i32, 0_i32, 0_i32), 0_i32);
        call!("and3", i32, (1_i32, 0_i32, 1_i32), 0_i32);
        call!("and3", i32, (1_i32, 1_i32, 0_i32), 0_i32);
        call!("and3", i32, (1_i32, 1_i32, 1_i32), 1_i32);

        // or3: exhaustive truth table for 3-input OR
        call!("or3", i32, (0_i32, 0_i32, 0_i32), 0_i32);
        call!("or3", i32, (0_i32, 0_i32, 1_i32), 1_i32);
        call!("or3", i32, (0_i32, 1_i32, 0_i32), 1_i32);
        call!("or3", i32, (1_i32, 0_i32, 0_i32), 1_i32);
        call!("or3", i32, (1_i32, 1_i32, 1_i32), 1_i32);

        // and_or: (a && b) || c
        call!("and_or", i32, (0_i32, 0_i32, 0_i32), 0_i32);
        call!("and_or", i32, (0_i32, 0_i32, 1_i32), 1_i32);
        call!("and_or", i32, (1_i32, 1_i32, 0_i32), 1_i32);
        call!("and_or", i32, (1_i32, 0_i32, 0_i32), 0_i32);
        call!("and_or", i32, (1_i32, 1_i32, 1_i32), 1_i32);

        // or_and: a || (b && c)
        call!("or_and", i32, (0_i32, 0_i32, 0_i32), 0_i32);
        call!("or_and", i32, (1_i32, 0_i32, 0_i32), 1_i32);
        call!("or_and", i32, (0_i32, 1_i32, 1_i32), 1_i32);
        call!("or_and", i32, (0_i32, 1_i32, 0_i32), 0_i32);
        call!("or_and", i32, (0_i32, 0_i32, 1_i32), 0_i32);

        // not_and_or: !a && (b || c)
        call!("not_and_or", i32, (0_i32, 0_i32, 0_i32), 0_i32);
        call!("not_and_or", i32, (0_i32, 1_i32, 0_i32), 1_i32);
        call!("not_and_or", i32, (0_i32, 0_i32, 1_i32), 1_i32);
        call!("not_and_or", i32, (0_i32, 1_i32, 1_i32), 1_i32);
        call!("not_and_or", i32, (1_i32, 1_i32, 1_i32), 0_i32);
        call!("not_and_or", i32, (1_i32, 0_i32, 0_i32), 0_i32);

        // de_morgan_and: !(a && b) == (!a || !b)
        call!("de_morgan_and", i32, (0_i32, 0_i32), 1_i32);
        call!("de_morgan_and", i32, (0_i32, 1_i32), 1_i32);
        call!("de_morgan_and", i32, (1_i32, 0_i32), 1_i32);
        call!("de_morgan_and", i32, (1_i32, 1_i32), 0_i32);

        // de_morgan_or: !(a || b) == (!a && !b)
        call!("de_morgan_or", i32, (0_i32, 0_i32), 1_i32);
        call!("de_morgan_or", i32, (0_i32, 1_i32), 0_i32);
        call!("de_morgan_or", i32, (1_i32, 0_i32), 0_i32);
        call!("de_morgan_or", i32, (1_i32, 1_i32), 0_i32);

        // cmp_and_cmp: (a < b) && (c < d)
        call!("cmp_and_cmp", i32, (1_i32, 2_i32, 3_i32, 4_i32), 1_i32);
        call!("cmp_and_cmp", i32, (2_i32, 1_i32, 3_i32, 4_i32), 0_i32);
        call!("cmp_and_cmp", i32, (1_i32, 2_i32, 4_i32, 3_i32), 0_i32);
        call!("cmp_and_cmp", i32, (5_i32, 5_i32, 3_i32, 4_i32), 0_i32);

        // cmp_or_cmp: (a > b) || (c > d)
        call!("cmp_or_cmp", i32, (2_i32, 1_i32, 4_i32, 3_i32), 1_i32);
        call!("cmp_or_cmp", i32, (1_i32, 2_i32, 3_i32, 4_i32), 0_i32);
        call!("cmp_or_cmp", i32, (2_i32, 1_i32, 3_i32, 4_i32), 1_i32);
        call!("cmp_or_cmp", i32, (1_i32, 2_i32, 4_i32, 3_i32), 1_i32);

        // between: (x >= lo) && (x <= hi) — range check
        call!("between", i32, (5_i32, 1_i32, 10_i32), 1_i32);
        call!("between", i32, (1_i32, 1_i32, 10_i32), 1_i32);
        call!("between", i32, (10_i32, 1_i32, 10_i32), 1_i32);
        call!("between", i32, (0_i32, 1_i32, 10_i32), 0_i32);
        call!("between", i32, (11_i32, 1_i32, 10_i32), 0_i32);
        call!("between", i32, (-1_i32, 0_i32, 100_i32), 0_i32);

        // not_between: (x < lo) || (x > hi)
        call!("not_between", i32, (5_i32, 1_i32, 10_i32), 0_i32);
        call!("not_between", i32, (0_i32, 1_i32, 10_i32), 1_i32);
        call!("not_between", i32, (11_i32, 1_i32, 10_i32), 1_i32);
        call!("not_between", i32, (1_i32, 1_i32, 10_i32), 0_i32);
        call!("not_between", i32, (10_i32, 1_i32, 10_i32), 0_i32);

        // all_same_sign: (a >= 0) == (b >= 0)
        call!("all_same_sign", i32, (5_i32, 10_i32), 1_i32);
        call!("all_same_sign", i32, (-5_i32, -10_i32), 1_i32);
        call!("all_same_sign", i32, (5_i32, -10_i32), 0_i32);
        call!("all_same_sign", i32, (-5_i32, 10_i32), 0_i32);
        call!("all_same_sign", i32, (0_i32, 0_i32), 1_i32);
        call!("all_same_sign", i32, (0_i32, -1_i32), 0_i32);

        // xor_bool: (a || b) && !(a && b) — logical XOR
        call!("xor_bool", i32, (0_i32, 0_i32), 0_i32);
        call!("xor_bool", i32, (0_i32, 1_i32), 1_i32);
        call!("xor_bool", i32, (1_i32, 0_i32), 1_i32);
        call!("xor_bool", i32, (1_i32, 1_i32), 0_i32);

        // implies: !a || b — logical implication
        call!("implies", i32, (0_i32, 0_i32), 1_i32);
        call!("implies", i32, (0_i32, 1_i32), 1_i32);
        call!("implies", i32, (1_i32, 0_i32), 0_i32);
        call!("implies", i32, (1_i32, 1_i32), 1_i32);

        // bool_majority3: (a && b) || (b && c) || (a && c) — majority vote
        call!("bool_majority3", i32, (0_i32, 0_i32, 0_i32), 0_i32);
        call!("bool_majority3", i32, (1_i32, 0_i32, 0_i32), 0_i32);
        call!("bool_majority3", i32, (0_i32, 1_i32, 0_i32), 0_i32);
        call!("bool_majority3", i32, (0_i32, 0_i32, 1_i32), 0_i32);
        call!("bool_majority3", i32, (1_i32, 1_i32, 0_i32), 1_i32);
        call!("bool_majority3", i32, (1_i32, 0_i32, 1_i32), 1_i32);
        call!("bool_majority3", i32, (0_i32, 1_i32, 1_i32), 1_i32);
        call!("bool_majority3", i32, (1_i32, 1_i32, 1_i32), 1_i32);

        // eq_bool: a == b (bool equality)
        call!("eq_bool", i32, (1_i32, 1_i32), 1_i32);
        call!("eq_bool", i32, (1_i32, 0_i32), 0_i32);
        call!("eq_bool", i32, (0_i32, 0_i32), 1_i32);

        // ne_bool: a != b (bool inequality)
        call!("ne_bool", i32, (1_i32, 0_i32), 1_i32);
        call!("ne_bool", i32, (1_i32, 1_i32), 0_i32);
    }

    #[test]
    #[ignore]
    fn regenerate_binops_bool_wasm() {
        use crate::utils::{get_test_data_path, wasm_codegen};

        let dir = get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("binops_bool")
            .join("binops_bool");
        let source_code = std::fs::read_to_string(dir.join("binops_bool.inf"))
            .expect("Failed to read binops_bool.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("binops_bool.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
    }
}
