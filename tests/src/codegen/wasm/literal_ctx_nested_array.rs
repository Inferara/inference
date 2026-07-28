/// WASM bytecode verification for nested array literals under a nested
/// annotation.
///
/// `[[i64; 2]; 2]` expected of `[[..], [..]]` is `[i64; 2]` expected of each
/// row and `i64` expected of each element, so the literals are stored at 64
/// bits. Reading them back at the same width is what these execution
/// assertions check: an `i32`-typed element would truncate every value here.
#[cfg(test)]
mod literal_ctx_nested_array_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, regenerate_wat, wasm_codegen,
    };

    const TEST_NAME: &str = "literal_ctx_nested_array";

    fn source() -> String {
        let path = get_test_file_path(module_path!(), TEST_NAME);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {path:?}"))
    }

    #[test]
    fn literal_ctx_nested_array_test() {
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
    fn literal_ctx_nested_array_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let wasm_bytes = wasm_codegen(&source());
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        macro_rules! call {
            ($name:expr, $ty:ty, $expected:expr) => {{
                let f: TypedFunc<(), $ty> = instance
                    .get_typed_func(&mut store, $name)
                    .unwrap_or_else(|e| panic!("Failed to get '{}': {e}", $name));
                let result = f
                    .call(&mut store, ())
                    .unwrap_or_else(|e| panic!("Call to '{}' failed: {e}", $name));
                assert_eq!(result, $expected, "{}() expected {:?}", $name, $expected);
            }};
        }

        call!("grid_first", i64, 1_099_511_627_776_i64);
        call!("grid_last", i64, 4_398_046_511_104_i64);
        call!("grid_sum", i64, 10_i64);
        call!("grid_of_expressions", i64, (1_i64 << 40) + 1);
        call!("grid_complement_element", i64, -1_i64);
        // u64::MAX read back through a nested element.
        call!("grid_unsigned_max", i64, -1_i64);
    }

    #[test]
    #[ignore]
    fn regenerate_literal_ctx_nested_array_wasm() {
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
