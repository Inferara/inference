/// WASM bytecode verification for array-element typing in the positions that
/// previously supplied no element type at all.
///
/// Only an annotated `let` used to reach an array literal's elements, so
/// `a = [1, 2];`, `const A: [i64; 2] = [1, 2];` and `Holder { values: [1, 2] }`
/// all rejected wide values. Each function here stores a value that does not
/// fit `i32` through one of those positions and reads it back.
#[cfg(test)]
mod literal_ctx_array_elements_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, regenerate_wat, wasm_codegen,
    };

    const TEST_NAME: &str = "literal_ctx_array_elements";

    fn source() -> String {
        let path = get_test_file_path(module_path!(), TEST_NAME);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {path:?}"))
    }

    #[test]
    fn literal_ctx_array_elements_test() {
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
    fn literal_ctx_array_elements_execution_test() {
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

        // Assignment position.
        call!(
            "assigned_elements",
            i64,
            (),
            1_099_511_627_776_i64 + 2_199_023_255_552_i64
        );
        call!("reassigned_first_element", i64, (), 4_398_046_511_104_i64);

        // `const` position.
        call!("const_array_element", i64, (), 4_398_046_511_104_i64);
        // u64::MAX stored through a const array element.
        call!("const_array_unsigned_max", i64, (), -1_i64);

        // Struct-literal field position.
        call!("struct_field_array_element", i64, (), 2_199_023_255_552_i64);

        // Elements that are literal expressions rather than bare literals.
        call!("element_expressions", i64, (), 2_199_023_255_552_i64);

        // An element peer-typed against a variable in the same literal:
        // `[v + 1, 2]` summed is `v + 3`.
        call!(
            "peer_typed_element",
            i64,
            1_099_511_627_775_i64,
            1_099_511_627_778_i64
        );
        call!("peer_typed_element", i64, 0_i64, 3_i64);
    }

    #[test]
    #[ignore]
    fn regenerate_literal_ctx_array_elements_wasm() {
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
