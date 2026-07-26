// WASM lowering shapes for this fixture, verified by binary inspection and the
// execution truth tables below.
//
// `&&` and `||` short-circuit: each lowers to a valued `if (result i32)` block
// whose right operand runs only when the left operand does not decide the
// result. `a && b` yields 0 without evaluating `b` when `a` is false; `a || b`
// yields 1 without evaluating `b` when `a` is true. Left-associative chains
// lower flat — sequential valued ifs, each block's 0/1 result feeding the next
// if's condition.
//
// and3(a,b,c):        (a && b) && c — two sequential valued ifs.
// or3(a,b,c):         (a || b) || c — two sequential valued ifs.
// de_morgan_and(a,b): the `a && b` valued if, then i32.eqz for the outer `!`.
// de_morgan_or(a,b):  the `a || b` valued if, then i32.eqz for the outer `!`.
// not_and_or(a,b,c):  i32.eqz on a (`!a`) as the `&&` if's condition, its true
//                     arm holding the `b || c` valued if.
// implies(a,b):       i32.eqz on a (`!a`) as the `||` if's condition, with b in
//                     the else arm.
// xor_bool(a,b):      the `a || b` valued if feeds a `&&` valued if whose true
//                     arm holds `!(a && b)` (the `a && b` valued if, then i32.eqz).
// between(x,lo,hi):   (x >= lo) i32.ge_s as the `&&` if's condition, its true arm
//                     holding (x <= hi) i32.le_s.
// all_same_sign(a,b): two i32.ge_s comparisons with 0, then i32.eq — bool
//                     equality, not a logical connective, so unchanged.

#[cfg(test)]
mod binops_bool_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
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
        assert_wat_equivalence(&actual, module_path!(), test_name);
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
        use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

        let dir = get_test_data_path()
            .join("codegen")
            .join("wasm")
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
        regenerate_wat(&actual, &dir, "binops_bool");
    }
}
