// Deeply nested expression codegen tests (8+ nesting levels).
//
// Verifies that the compiler correctly lowers expression trees with deep nesting,
// including chained arithmetic, mixed operator precedence across nesting levels,
// nested comparisons with boolean connectives, and function calls embedded in
// nested expressions.
//
// Key patterns tested:
// - 8-level left-associative addition chain: ((((((((1+2)+3)+4)+5)+6)+7)+8)+9) = 45
// - Mixed arithmetic in nested groups: ((a+b)*(c-d)) + ((a-b)*(c+d))
// - Short-circuit boolean connectives over nested comparisons: (a>b) && ((c<d) || (a==c))
// - Function calls as subexpressions: (f(x) + f(x+1)) * 2
// - 4-level left-associative parenthesized addition: ((((1+2)+3)+4)+5) = 15

#[cfg(test)]
mod expr_deep_nesting_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn expr_deep_nesting_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 28);
        cov_mark::check_count!(wasm_codegen_emit_parenthesized_expression, 23);
        cov_mark::check_count!(wasm_codegen_emit_prefix_unary_expression, 0);
        cov_mark::check_count!(wasm_codegen_emit_unary_neg, 0);
        cov_mark::check_count!(wasm_codegen_emit_function_call, 2);
        cov_mark::check_count!(wasm_codegen_emit_function_params, 10);
        let test_name = "expr_deep_nesting";
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
    fn expr_deep_nesting_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "expr_deep_nesting";
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

        // nest_8_add: 1+2+3+4+5+6+7+8+9 = 45
        call!("nest_8_add", i32, (), 45_i32);

        // nest_mixed_ops(2,3,5,1): (2+3)*(5-1) + (2-3)*(5+1) = 20 + (-6) = 14
        call!("nest_mixed_ops", i32, (2_i32, 3_i32, 5_i32, 1_i32), 14_i32);

        // nest_comparison(5,3,2,4): (5>3) && ((2<4)||(5==2)) = true && true = true
        call!("nest_comparison", i32, (5_i32, 3_i32, 2_i32, 4_i32), 1_i32);

        // nest_comparison(1,3,2,4): (1>3) && ... = false
        call!("nest_comparison", i32, (1_i32, 3_i32, 2_i32, 4_i32), 0_i32);

        // nest_call_in_expr(3): (9 + 16) * 2 = 50
        call!("nest_call_in_expr", i32, 3_i32, 50_i32);

        // nest_paren_deep: ((((1+2)+3)+4)+5) = 15
        call!("nest_paren_deep", i32, (), 15_i32);
    }
}

/// Test data regeneration helper.
///
/// Regenerates the expected `.wasm` test data file from the current compiler output.
/// Run with `--ignored` flag:
///
/// ```bash
/// cargo test -p inference-tests codegen::wasm::expr_deep_nesting::regenerate::regenerate_expr_deep_nesting_wasm -- --ignored
/// ```
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("expr_deep_nesting")
    }

    #[test]
    #[ignore]
    fn regenerate_expr_deep_nesting_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("expr_deep_nesting.inf"))
            .expect("Failed to read expr_deep_nesting.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("expr_deep_nesting.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "expr_deep_nesting");
    }
}
