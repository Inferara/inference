#[cfg(test)]
mod base_codegen_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, get_test_file_path, get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn trivial_test() {
        let test_name = "trivial";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        let actual = wasm_codegen(&source_code);
        assert_wasms_modules_equivalence(&expected, &actual);
    }

    #[test]
    fn trivial_test_execution() {
        use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

        let test_name = "trivial";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {}", e));
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {}", e));

        let run_func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "run")
            .unwrap_or_else(|e| panic!("Failed to get 'run' function: {}", e));

        let result = run_func
            .call(&mut store, ())
            .unwrap_or_else(|e| panic!("Failed to execute 'run' function: {}", e));

        assert_eq!(result, 42, "Expected 'run' function to return 42");
    }
}
