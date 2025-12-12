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
        let actual = wasm_codegen(&source_code);
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        // let test_dir = std::path::Path::new(&test_file_path).parent().unwrap();
        // std::fs::write(test_dir.join("actual.wasm"), &actual)
        // .unwrap_or_else(|e| panic!("Failed to write actual.wasm: {}", e));
        assert_wasms_modules_equivalence(&expected, &actual);
    }

    #[test]
    fn trivial_test_execution() {
        use wasmtime::{Engine, Linker, Memory, MemoryType, Module, Store, TypedFunc};

        let test_name = "trivial";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {}", e));

        let mut store = Store::new(&engine, ());

        let mut linker = Linker::new(&engine);
        let memory_type = MemoryType::new(1, None);
        let memory = Memory::new(&mut store, memory_type)
            .unwrap_or_else(|e| panic!("Failed to create memory: {}", e));
        linker
            .define(&mut store, "env", "__linear_memory", memory)
            .unwrap_or_else(|e| panic!("Failed to define memory import: {}", e));

        let instance = linker
            .instantiate(&mut store, &module)
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {}", e));

        let hello_world_func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "hello_world")
            .unwrap_or_else(|e| panic!("Failed to get 'hello_world' function: {}", e));

        let result = hello_world_func
            .call(&mut store, ())
            .unwrap_or_else(|e| panic!("Failed to execute 'hello_world' function: {}", e));

        assert_eq!(result, 42, "Expected 'hello_world' function to return 42");
    }

    #[test]
    fn nondet_test() {
        let test_name = "nondet";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        // let expected = get_test_wasm_path(module_path!(), test_name);
        // let expected = std::fs::read(&expected)
        //     .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        let test_dir = std::path::Path::new(&test_file_path).parent().unwrap();
        std::fs::write(test_dir.join("actual-nondet.wasm"), &actual)
            .unwrap_or_else(|e| panic!("Failed to write actual-nondet.wasm: {}", e));
        // assert_wasms_modules_equivalence(&expected, &actual);
    }
}
