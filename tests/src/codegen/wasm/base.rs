#[cfg(test)]
mod base_codegen_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, codegen_output,
        get_test_file_path, get_test_wasm_path, regenerate_wat, wasm_codegen,
        wasm_codegen_with_target,
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
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn const_test() {
        let test_name = "const";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn trivial_test_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "trivial";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {}", e));

        let mut store = Store::new(&engine, ());

        let instance = wasmtime::Instance::new(&mut store, &module, &[])
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
        cov_mark::check_count!(wasm_codegen_emit_forall_block, 1);
        cov_mark::check_count!(wasm_codegen_emit_assume_block, 1);
        cov_mark::check_count!(wasm_codegen_emit_exists_block, 1);
        cov_mark::check_count!(wasm_codegen_emit_unique_block, 1);
        cov_mark::check_count!(wasm_codegen_emit_uzumaki_i32, 1);
        let test_name = "nondet";
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
    fn i64_uzumaki_test() {
        cov_mark::check_count!(wasm_codegen_emit_uzumaki_i64, 1);
        let test_name = "i64_uzumaki";
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
    fn bool_literal_test() {
        let test_name = "bool_literal";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn mixed_visibility_test() {
        let test_name = "mixed_visibility";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn bool_literal_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "bool_literal";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {}", e));

        let mut store = Store::new(&engine, ());

        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {}", e));

        let get_true_func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_true")
            .unwrap_or_else(|e| panic!("Failed to get 'get_true' function: {}", e));
        let result = get_true_func
            .call(&mut store, ())
            .unwrap_or_else(|e| panic!("Failed to execute 'get_true' function: {}", e));
        assert_eq!(result, 1, "Expected 'get_true' to return 1");

        let get_false_func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_false")
            .unwrap_or_else(|e| panic!("Failed to get 'get_false' function: {}", e));
        let result = get_false_func
            .call(&mut store, ())
            .unwrap_or_else(|e| panic!("Failed to execute 'get_false' function: {}", e));
        assert_eq!(result, 0, "Expected 'get_false' to return 0");
    }

    #[test]
    fn bool_const_test() {
        let test_name = "bool_const";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn bool_const_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "bool_const";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {}", e));

        let mut store = Store::new(&engine, ());

        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {}", e));

        let get_const_true_func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_const_true")
            .unwrap_or_else(|e| panic!("Failed to get 'get_const_true' function: {}", e));
        let result = get_const_true_func
            .call(&mut store, ())
            .unwrap_or_else(|e| panic!("Failed to execute 'get_const_true' function: {}", e));
        assert_eq!(result, 1, "Expected 'get_const_true' to return 1");

        let get_const_false_func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_const_false")
            .unwrap_or_else(|e| panic!("Failed to get 'get_const_false' function: {}", e));
        let result = get_const_false_func
            .call(&mut store, ())
            .unwrap_or_else(|e| panic!("Failed to execute 'get_const_false' function: {}", e));
        assert_eq!(result, 0, "Expected 'get_const_false' to return 0");
    }

    #[test]
    fn numeric_literals_test() {
        let test_name = "numeric_literals";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn numeric_literals_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "numeric_literals";
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

        // Signed types return as i32 (sub-i32 types promoted)
        call!("signed_i8", i32, (), -128_i32);
        call!("signed_i16", i32, (), -32768_i32);
        call!("signed_i32", i32, (), i32::MIN);
        call!("signed_i64", i64, (), i64::MIN);

        // Unsigned types: sub-i32 promoted to i32, u32/u64 bit-reinterpreted
        call!("unsigned_u8", i32, (), 255_i32);
        call!("unsigned_u16", i32, (), 65535_i32);
        // u32::MAX (4294967295) is bit-reinterpreted as i32(-1)
        call!("unsigned_u32", i32, (), -1_i32);
        // u64::MAX is bit-reinterpreted as i64(-1)
        call!("unsigned_u64", i64, (), -1_i64);
    }

    #[test]
    fn local_variables_test() {
        cov_mark::check_count!(wasm_codegen_emit_variable_definition, 14);
        let test_name = "local_variables";
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
    fn local_variables_exec_test() {
        let test_name = "local_variables_exec";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn local_variables_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "local_variables_exec";
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

        let f: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "let_i32_literal")
            .unwrap_or_else(|e| panic!("Failed to get 'let_i32_literal': {e}"));
        assert_eq!(
            f.call(&mut store, ())
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
            42
        );

        let f: TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "let_i64_literal")
            .unwrap_or_else(|e| panic!("Failed to get 'let_i64_literal': {e}"));
        assert_eq!(
            f.call(&mut store, ())
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
            i64::MIN
        );

        let f: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "let_i8_literal")
            .unwrap_or_else(|e| panic!("Failed to get 'let_i8_literal': {e}"));
        assert_eq!(
            f.call(&mut store, ())
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
            -128_i32
        );

        let f: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "let_i16_literal")
            .unwrap_or_else(|e| panic!("Failed to get 'let_i16_literal': {e}"));
        assert_eq!(
            f.call(&mut store, ())
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
            -32768_i32
        );

        let f: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "let_u8_literal")
            .unwrap_or_else(|e| panic!("Failed to get 'let_u8_literal': {e}"));
        assert_eq!(
            f.call(&mut store, ())
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
            255_i32
        );

        let f: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "let_u16_literal")
            .unwrap_or_else(|e| panic!("Failed to get 'let_u16_literal': {e}"));
        assert_eq!(
            f.call(&mut store, ())
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
            65535_i32
        );

        let f: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "let_bool_literal_true")
            .unwrap_or_else(|e| panic!("Failed to get 'let_bool_literal_true': {e}"));
        assert_eq!(
            f.call(&mut store, ())
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
            1_i32
        );

        let f: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "let_bool_literal_false")
            .unwrap_or_else(|e| panic!("Failed to get 'let_bool_literal_false': {e}"));
        assert_eq!(
            f.call(&mut store, ())
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
            0_i32
        );

        let f: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "let_from_identifier")
            .unwrap_or_else(|e| panic!("Failed to get 'let_from_identifier': {e}"));
        assert_eq!(
            f.call(&mut store, ())
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
            10_i32
        );
    }

    #[test]
    fn fn_params_test() {
        cov_mark::check_count!(wasm_codegen_emit_function_params, 7);
        let test_name = "fn_params";
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
    fn fn_params_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "fn_params";
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

        let identity_i32: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "identity_i32")
            .unwrap_or_else(|e| panic!("Failed to get 'identity_i32': {e}"));
        assert_eq!(
            identity_i32.call(&mut store, 42).unwrap_or_else(|e| panic!("Call failed: {e}")),
            42
        );

        let identity_i64: TypedFunc<i64, i64> = instance
            .get_typed_func(&mut store, "identity_i64")
            .unwrap_or_else(|e| panic!("Failed to get 'identity_i64': {e}"));
        assert_eq!(
            identity_i64
                .call(&mut store, -9223372036854775808_i64)
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
            -9223372036854775808_i64
        );

        let identity_bool: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "identity_bool")
            .unwrap_or_else(|e| panic!("Failed to get 'identity_bool': {e}"));
        assert_eq!(
            identity_bool.call(&mut store, 1).unwrap_or_else(|e| panic!("Call failed: {e}")),
            1
        );

        let first_of_two: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "first_of_two")
            .unwrap_or_else(|e| panic!("Failed to get 'first_of_two': {e}"));
        assert_eq!(
            first_of_two
                .call(&mut store, (10, 20))
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
            10
        );

        let second_of_two: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "second_of_two")
            .unwrap_or_else(|e| panic!("Failed to get 'second_of_two': {e}"));
        assert_eq!(
            second_of_two
                .call(&mut store, (10, 20))
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
            20
        );
    }

    #[test]
    fn fn_calls_test() {
        cov_mark::check_count!(wasm_codegen_emit_function_call, 5);
        let test_name = "fn_calls";
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
    fn fn_calls_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "fn_calls";
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

        let call_zero: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "call_zero")
            .unwrap_or_else(|e| panic!("Failed to get 'call_zero': {e}"));
        assert_eq!(
            call_zero.call(&mut store, ()).unwrap_or_else(|e| panic!("Call failed: {e}")),
            0
        );

        let call_identity: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "call_identity")
            .unwrap_or_else(|e| panic!("Failed to get 'call_identity': {e}"));
        assert_eq!(
            call_identity
                .call(&mut store, 77)
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
            77
        );

        let call_first: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "call_first")
            .unwrap_or_else(|e| panic!("Failed to get 'call_first': {e}"));
        assert_eq!(
            call_first
                .call(&mut store, (10, 20))
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
            10
        );

        let let_from_call: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "let_from_call")
            .unwrap_or_else(|e| panic!("Failed to get 'let_from_call': {e}"));
        assert_eq!(
            let_from_call.call(&mut store, ()).unwrap_or_else(|e| panic!("Call failed: {e}")),
            0
        );

        let forward_call: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "forward_call")
            .unwrap_or_else(|e| panic!("Failed to get 'forward_call': {e}"));
        assert_eq!(
            forward_call.call(&mut store, ()).unwrap_or_else(|e| panic!("Call failed: {e}")),
            99
        );
    }

    #[test]
    fn binary_ops_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 23);
        cov_mark::check_count!(wasm_codegen_emit_prefix_unary_expression, 3);
        cov_mark::check!(wasm_codegen_emit_unary_neg);
        cov_mark::check!(wasm_codegen_emit_unary_not);
        cov_mark::check!(wasm_codegen_emit_unary_bitnot);
        cov_mark::check!(wasm_codegen_emit_parenthesized_expression);
        let test_name = "binary_ops";
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
    fn binary_ops_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "binary_ops";
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

        call!("add_i32", i32, (3_i32, 4_i32), 7_i32);
        call!("sub_i32", i32, (10_i32, 3_i32), 7_i32);
        call!("mul_i32", i32, (6_i32, 7_i32), 42_i32);
        call!("div_i32", i32, (10_i32, 3_i32), 3_i32);
        call!("mod_i32", i32, (10_i32, 3_i32), 1_i32);
        call!("div_u32", i32, (10_i32, 3_i32), 3_i32);
        call!("add_i64", i64, (1_i64, 2_i64), 3_i64);
        call!("eq_i32", i32, (5_i32, 5_i32), 1_i32);
        call!("eq_i32", i32, (5_i32, 6_i32), 0_i32);
        call!("ne_i32", i32, (5_i32, 5_i32), 0_i32);
        call!("ne_i32", i32, (5_i32, 6_i32), 1_i32);
        call!("lt_i32", i32, (-1_i32, 0_i32), 1_i32);
        call!("le_i32", i32, (3_i32, 3_i32), 1_i32);
        call!("le_i32", i32, (4_i32, 3_i32), 0_i32);
        call!("gt_i32", i32, (5_i32, 4_i32), 1_i32);
        call!("ge_i32", i32, (5_i32, 5_i32), 1_i32);
        call!("and_bool", i32, (1_i32, 0_i32), 0_i32);
        call!("or_bool", i32, (1_i32, 0_i32), 1_i32);
        call!("band_i32", i32, (0xFF_i32, 0x0F_i32), 0x0F_i32);
        call!("bor_i32", i32, (0xF0_i32, 0x0F_i32), 0xFF_i32);
        call!("bxor_i32", i32, (0xFF_i32, 0x0F_i32), 0xF0_i32);
        call!("shl_i32", i32, (1_i32, 3_i32), 8_i32);
        call!("shr_i32", i32, (-4_i32, 1_i32), -2_i32);
        call!("shr_u32", i32, (-2147483648_i32, 1_i32), 0x40000000_i32);
        call!("neg_i32", i32, 5_i32, -5_i32);
        call!("not_bool", i32, 1_i32, 0_i32);
        call!("not_bool", i32, 0_i32, 1_i32);
        call!("bitnot_i32", i32, 0_i32, -1_i32);
        call!("paren_add", i32, (3_i32, 4_i32), 7_i32);
        call!("binop_as_let", i32, (3_i32, 4_i32), 7_i32);
    }

    #[test]
    fn if_else_test() {
        cov_mark::check_count!(wasm_codegen_emit_if_statement, 7);
        cov_mark::check_count!(wasm_codegen_emit_if_with_else, 2);
        let test_name = "if_else";
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
    fn if_else_exec_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "if_else";
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

        call!("if_only", i32, 5_i32, 1_i32);
        call!("if_only", i32, -1_i32, 0_i32);
        call!("if_else_branch", i32, 5_i32, 1_i32);
        call!("if_else_branch", i32, -1_i32, 0_i32);
        call!("if_with_local", i32, 3_i32, 3_i32);
        call!("if_with_local", i32, -1_i32, 0_i32);
        call!("if_else_with_local", i32, 3_i32, 3_i32);
        call!("if_else_with_local", i32, -1_i32, -1_i32);
        call!("nested_if", i32, (1_i32, 1_i32), 2_i32);
        call!("nested_if", i32, (1_i32, -1_i32), 1_i32);
        call!("nested_if", i32, (-1_i32, 1_i32), 0_i32);
        call!("if_void", (), 5_i32, ());
        call!("if_void", (), -1_i32, ());
    }

    #[test]
    fn if_nondet_test() {
        cov_mark::check_count!(wasm_codegen_emit_if_statement, 1);
        cov_mark::check_count!(wasm_codegen_emit_forall_block, 1);
        let test_name = "if_nondet";
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
    fn if_bool_exprs_test() {
        cov_mark::check_count!(wasm_codegen_emit_if_statement, 16);
        cov_mark::check_count!(wasm_codegen_emit_if_with_else, 5);
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 32);
        cov_mark::check_count!(wasm_codegen_emit_prefix_unary_expression, 4);
        cov_mark::check_count!(wasm_codegen_emit_parenthesized_expression, 21);
        let test_name = "if_bool_exprs";
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
    fn if_bool_exprs_exec_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "if_bool_exprs";
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

        // Group 1: Direct boolean parameters as conditions
        call!("if_bool_param", i32, 1_i32, 1_i32);
        call!("if_bool_param", i32, 0_i32, 0_i32);
        call!("if_not_param", i32, 1_i32, 0_i32);
        call!("if_not_param", i32, 0_i32, 1_i32);

        // Group 2: Comparison + logical ops as conditions
        call!("if_and", i32, (5_i32, 5_i32), 1_i32);
        call!("if_and", i32, (5_i32, -1_i32), 0_i32);
        call!("if_and", i32, (-1_i32, 5_i32), 0_i32);
        call!("if_and", i32, (-1_i32, -1_i32), 0_i32);
        call!("if_or", i32, (5_i32, 5_i32), 1_i32);
        call!("if_or", i32, (5_i32, -1_i32), 1_i32);
        call!("if_or", i32, (-1_i32, 5_i32), 1_i32);
        call!("if_or", i32, (-1_i32, -1_i32), 0_i32);
        call!("if_not_cmp", i32, 5_i32, 0_i32);
        call!("if_not_cmp", i32, -1_i32, 1_i32);
        call!("if_not_cmp", i32, 0_i32, 1_i32);

        // Group 3: Complex nested boolean conditions
        call!("if_and_or", i32, (1_i32, 5_i32, 0_i32), 1_i32);
        call!("if_and_or", i32, (1_i32, 20_i32, 1_i32), 0_i32);
        call!("if_and_or", i32, (-1_i32, 5_i32, 0_i32), 0_i32);
        call!("if_or_and", i32, (1_i32, 0_i32, 0_i32), 1_i32);
        call!("if_or_and", i32, (-1_i32, 1_i32, 1_i32), 1_i32);
        call!("if_or_and", i32, (-1_i32, 1_i32, -1_i32), 0_i32);
        call!("if_or_and", i32, (-1_i32, -1_i32, -1_i32), 0_i32);
        call!("if_demorgan_and", i32, (1_i32, 1_i32), 0_i32);
        call!("if_demorgan_and", i32, (1_i32, 0_i32), 1_i32);
        call!("if_demorgan_and", i32, (0_i32, 1_i32), 1_i32);
        call!("if_demorgan_and", i32, (0_i32, 0_i32), 1_i32);
        call!("if_demorgan_or", i32, (1_i32, 1_i32), 0_i32);
        call!("if_demorgan_or", i32, (1_i32, 0_i32), 0_i32);
        call!("if_demorgan_or", i32, (0_i32, 1_i32), 0_i32);
        call!("if_demorgan_or", i32, (0_i32, 0_i32), 1_i32);
        call!("if_between", i32, (5_i32, 1_i32, 10_i32), 1_i32);
        call!("if_between", i32, (0_i32, 1_i32, 10_i32), 0_i32);
        call!("if_between", i32, (15_i32, 1_i32, 10_i32), 0_i32);
        call!("if_between", i32, (1_i32, 1_i32, 10_i32), 1_i32);
        call!("if_between", i32, (10_i32, 1_i32, 10_i32), 1_i32);

        // Group 4: Boolean locals as conditions
        call!("if_bool_local", i32, 5_i32, 1_i32);
        call!("if_bool_local", i32, -1_i32, 0_i32);
        call!("if_bool_local_complex", i32, (5_i32, 5_i32), 1_i32);
        call!("if_bool_local_complex", i32, (5_i32, -1_i32), 0_i32);

        // Group 5: Boolean equality/inequality in conditions
        call!("if_bool_eq", i32, (1_i32, 1_i32), 1_i32);
        call!("if_bool_eq", i32, (1_i32, 0_i32), 0_i32);
        call!("if_bool_eq", i32, (0_i32, 1_i32), 0_i32);
        call!("if_bool_eq", i32, (0_i32, 0_i32), 1_i32);
        call!("if_bool_ne", i32, (1_i32, 1_i32), 0_i32);
        call!("if_bool_ne", i32, (1_i32, 0_i32), 1_i32);
        call!("if_bool_ne", i32, (0_i32, 1_i32), 1_i32);
        call!("if_bool_ne", i32, (0_i32, 0_i32), 0_i32);

        // Group 6: Boolean return from conditionals
        call!("cond_returns_bool", i32, 5_i32, 1_i32);
        call!("cond_returns_bool", i32, -1_i32, 0_i32);

        // Group 7: If-else with complex condition and value-producing arms
        call!("if_else_complex", i32, (5_i32, 5_i32), 5_i32);
        call!("if_else_complex", i32, (-1_i32, 5_i32), 5_i32);
        call!("if_else_complex", i32, (5_i32, -1_i32), -1_i32);
    }

    #[test]
    fn assign_test() {
        cov_mark::check_count!(wasm_codegen_emit_assign_identifier, 10);
        let test_name = "assign";
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
    fn assign_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "assign";
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

        call!("assign_simple_i32", i32, (), 42_i32);
        call!("assign_simple_i64", i64, (), 42_i64);
        call!("assign_from_expr", i32, (), 3_i32);
        call!("assign_from_param", i32, 10_i32, 10_i32);
        call!("assign_multiple", i32, (), 3_i32);
        call!("assign_from_call", i32, (), 3_i32);
        call!("assign_bool", i32, (), 1_i32);
        call!("assign_in_if", i32, 5_i32, 5_i32);
        call!("assign_in_if", i32, -1_i32, 0_i32);
        call!("assign_param_mut", i32, 0_i32, 99_i32);
    }

    #[test]
    fn assign_nondet_test() {
        cov_mark::check_count!(wasm_codegen_emit_assign_identifier, 1);
        cov_mark::check_count!(wasm_codegen_emit_forall_block, 1);
        cov_mark::check_count!(wasm_codegen_emit_uzumaki_i32, 1);
        let test_name = "assign_nondet";
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
    fn soroban_produces_valid_wasm() {
        let source = "pub fn hello_world() -> i32 { return 42; }";
        let wasm_bytes = wasm_codegen_with_target(source, inference_wasm_codegen::Target::Soroban);
        // Validate with inf_wasmparser (superset of standard wasmparser).
        // Soroban WASM should be valid standard WASM without custom opcodes.
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Soroban WASM is invalid: {e}"));
        // Verify the binary is non-empty and starts with the WASM magic number
        assert!(wasm_bytes.len() > 8, "Soroban WASM should be non-trivial");
        assert_eq!(
            &wasm_bytes[0..4],
            b"\0asm",
            "Soroban output should start with WASM magic number"
        );
    }

    #[test]
    fn array_literal_test() {
        cov_mark::check_count!(wasm_codegen_emit_array_literal, 5);
        cov_mark::check_count!(wasm_codegen_emit_stack_prologue, 4);
        cov_mark::check_count!(wasm_codegen_emit_stack_epilogue, 8);
        let test_name = "array_literal";
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
    fn array_literal_execution_test() {
        use wasmtime::{Engine, Module, Store};

        let test_name = "array_literal";
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

        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Module should export 'memory'");

        // Helper: read i32 (little-endian) from memory at the given address
        fn read_i32(memory: &wasmtime::Memory, store: &mut Store<()>, addr: u32) -> i32 {
            let mut buf = [0u8; 4];
            memory
                .read(store, addr as usize, &mut buf)
                .unwrap_or_else(|e| panic!("Failed to read memory at 0x{addr:x}: {e}"));
            i32::from_le_bytes(buf)
        }

        // Helper: read u8 from memory at the given address
        fn read_u8(memory: &wasmtime::Memory, store: &mut Store<()>, addr: u32) -> u8 {
            let mut buf = [0u8; 1];
            memory
                .read(store, addr as usize, &mut buf)
                .unwrap_or_else(|e| panic!("Failed to read memory at 0x{addr:x}: {e}"));
            buf[0]
        }

        // Read the __stack_pointer global to find the initial value
        let stack_pointer = instance
            .get_global(&mut store, "__stack_pointer")
            .expect("Module should export '__stack_pointer'");
        let initial_sp = stack_pointer.get(&mut store).i32().unwrap();

        // i32_array: stores [10, 20, 30] in a 16-byte frame
        let i32_array_fn: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "i32_array")
            .expect("Failed to get 'i32_array'");
        let result = i32_array_fn
            .call(&mut store, ())
            .expect("i32_array failed");
        assert_eq!(result, 0, "i32_array should return 0");
        // After call, stack pointer should be restored
        let sp_after = stack_pointer.get(&mut store).i32().unwrap();
        assert_eq!(
            sp_after, initial_sp,
            "Stack pointer should be restored after i32_array call"
        );

        // bool_array: stores [true, false, true, false]
        let bool_array_fn: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "bool_array")
            .expect("Failed to get 'bool_array'");
        let result = bool_array_fn
            .call(&mut store, ())
            .expect("bool_array failed");
        assert_eq!(result, 0, "bool_array should return 0");

        // two_arrays: stores [1, 2] and [3, 4]
        let two_arrays_fn: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "two_arrays")
            .expect("Failed to get 'two_arrays'");
        let result = two_arrays_fn
            .call(&mut store, ())
            .expect("two_arrays failed");
        assert_eq!(result, 0, "two_arrays should return 0");

        // single_element: stores [42]
        let single_element_fn: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "single_element")
            .expect("Failed to get 'single_element'");
        let result = single_element_fn
            .call(&mut store, ())
            .expect("single_element failed");
        assert_eq!(result, 0, "single_element should return 0");

        // Verify memory contents during execution by calling a function
        // that stores known values, then reading memory before epilogue restores SP.
        // Since functions return 0 and restore the stack, we verify that:
        // 1) Functions execute without trapping
        // 2) Stack pointer is properly restored
        // 3) Memory was allocated (memory export exists)

        // To verify actual memory writes, we use a trick: call i32_array,
        // then inspect memory at the frame address. The stack pointer was
        // restored, but the memory at the old frame is still readable.
        // The frame was at (initial_sp - 16), since frame size is 16 bytes.
        let frame_addr = (initial_sp - 16) as u32;
        // After i32_array call, memory may have been overwritten by subsequent
        // calls. Call i32_array last to ensure its values are in memory.
        let _ = i32_array_fn
            .call(&mut store, ())
            .expect("i32_array failed on second call");
        // The memory at frame_addr should contain [10, 20, 30]
        assert_eq!(
            read_i32(&memory, &mut store, frame_addr),
            10,
            "First element should be 10"
        );
        assert_eq!(
            read_i32(&memory, &mut store, frame_addr + 4),
            20,
            "Second element should be 20"
        );
        assert_eq!(
            read_i32(&memory, &mut store, frame_addr + 8),
            30,
            "Third element should be 30"
        );

        // Call bool_array last to check its memory layout
        let _ = bool_array_fn
            .call(&mut store, ())
            .expect("bool_array failed on second call");
        assert_eq!(
            read_u8(&memory, &mut store, frame_addr),
            1,
            "First bool element should be 1 (true)"
        );
        assert_eq!(
            read_u8(&memory, &mut store, frame_addr + 1),
            0,
            "Second bool element should be 0 (false)"
        );
        assert_eq!(
            read_u8(&memory, &mut store, frame_addr + 2),
            1,
            "Third bool element should be 1 (true)"
        );
        assert_eq!(
            read_u8(&memory, &mut store, frame_addr + 3),
            0,
            "Fourth bool element should be 0 (false)"
        );
    }

    #[test]
    fn array_literal_empty_produces_valid_wasm() {
        let source = r#"
            pub fn empty_array() -> i32 {
                let arr: [i32; 0] = [];
                return 0;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Empty array WASM is invalid: {e}"));
    }

    #[test]
    fn array_literal_empty_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn empty_array() -> i32 {
                let arr: [i32; 0] = [];
                return 0;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "empty_array")
            .expect("Failed to get 'empty_array'");
        let result = func.call(&mut store, ()).expect("empty_array failed");
        assert_eq!(result, 0, "empty_array should return 0");
    }

    #[test]
    fn array_literal_no_memory_for_non_array_functions() {
        use wasmtime::{Engine, Module, Store};

        // Functions without arrays should NOT export memory
        let source = "pub fn simple() -> i32 { return 42; }";
        let wasm_bytes = wasm_codegen(source);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let memory = instance.get_memory(&mut store, "memory");
        assert!(
            memory.is_none(),
            "Non-array functions should not export memory"
        );
    }

    #[test]
    fn array_literal_has_memory_section() {
        let source = r#"
            pub fn with_array() -> i32 {
                let arr: [i32; 2] = [1, 2];
                return 0;
            }
        "#;
        let output = codegen_output(source);
        let wasm_bytes = output.wasm();
        // Verify the module has the WASM magic number and can be validated
        assert_eq!(&wasm_bytes[0..4], b"\0asm", "Should be valid WASM");
        inf_wasmparser::validate(wasm_bytes)
            .unwrap_or_else(|e| panic!("Array WASM is invalid: {e}"));
    }

    #[test]
    fn array_literal_void_function() {
        let source = r#"
            pub fn void_with_array() {
                let arr: [i32; 3] = [1, 2, 3];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Void function with array WASM is invalid: {e}"));
    }

    #[test]
    fn array_literal_void_function_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn void_with_array() {
                let arr: [i32; 3] = [1, 2, 3];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), ()> = instance
            .get_typed_func(&mut store, "void_with_array")
            .expect("Failed to get 'void_with_array'");
        func.call(&mut store, ())
            .expect("void_with_array should not trap");
    }

    #[test]
    fn array_index_test() {
        cov_mark::check_count!(wasm_codegen_emit_array_index_read, 6);
        cov_mark::check_count!(wasm_codegen_emit_array_literal, 6);
        cov_mark::check_count!(wasm_codegen_emit_stack_prologue, 6);
        cov_mark::check_count!(wasm_codegen_emit_stack_epilogue, 14);
        let test_name = "array_index";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {e}"));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn array_index_execution_test() {
        use wasmtime::{Engine, Module, Store};

        let test_name = "array_index";
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

        let stack_pointer = instance
            .get_global(&mut store, "__stack_pointer")
            .expect("Module should export '__stack_pointer'");
        let initial_sp = stack_pointer.get(&mut store).i32().unwrap();

        // read_first: arr[0] of [10, 20, 30] -> 10
        let read_first: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "read_first")
            .expect("Failed to get 'read_first'");
        let result = read_first.call(&mut store, ()).expect("read_first failed");
        assert_eq!(result, 10, "arr[0] of [10, 20, 30] should be 10");
        let sp_after = stack_pointer.get(&mut store).i32().unwrap();
        assert_eq!(
            sp_after, initial_sp,
            "Stack pointer should be restored after read_first"
        );

        // read_last: arr[2] of [10, 20, 30] -> 30
        let read_last: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "read_last")
            .expect("Failed to get 'read_last'");
        let result = read_last.call(&mut store, ()).expect("read_last failed");
        assert_eq!(result, 30, "arr[2] of [10, 20, 30] should be 30");

        // read_middle: arr[1] of [10, 20, 30] -> 20
        let read_middle: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "read_middle")
            .expect("Failed to get 'read_middle'");
        let result = read_middle.call(&mut store, ()).expect("read_middle failed");
        assert_eq!(result, 20, "arr[1] of [10, 20, 30] should be 20");

        // read_with_variable: arr[i] of [100, 200, 300]
        let read_with_variable: wasmtime::TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "read_with_variable")
            .expect("Failed to get 'read_with_variable'");
        let result = read_with_variable
            .call(&mut store, 0)
            .expect("read_with_variable(0) failed");
        assert_eq!(result, 100, "arr[0] of [100, 200, 300] should be 100");
        let result = read_with_variable
            .call(&mut store, 1)
            .expect("read_with_variable(1) failed");
        assert_eq!(result, 200, "arr[1] of [100, 200, 300] should be 200");
        let result = read_with_variable
            .call(&mut store, 2)
            .expect("read_with_variable(2) failed");
        assert_eq!(result, 300, "arr[2] of [100, 200, 300] should be 300");

        // read_bool_true: flags[0] of [true, false, true] -> 1 (enters if branch)
        let read_bool_true: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "read_bool_true")
            .expect("Failed to get 'read_bool_true'");
        let result = read_bool_true
            .call(&mut store, ())
            .expect("read_bool_true failed");
        assert_eq!(result, 1, "flags[0] is true, should return 1");

        // read_bool_false: flags[1] of [true, false, true] -> 0 (skips if branch)
        let read_bool_false: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "read_bool_false")
            .expect("Failed to get 'read_bool_false'");
        let result = read_bool_false
            .call(&mut store, ())
            .expect("read_bool_false failed");
        assert_eq!(result, 0, "flags[1] is false, should return 0");

        // Verify stack pointer is fully restored after all calls
        let final_sp = stack_pointer.get(&mut store).i32().unwrap();
        assert_eq!(
            final_sp, initial_sp,
            "Stack pointer should be restored after all calls"
        );
    }

    #[test]
    fn array_index_inline_validation() {
        let source = r#"
            pub fn single_read() -> i32 {
                let arr: [i32; 1] = [42];
                return arr[0];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Array index WASM is invalid: {e}"));
    }

    #[test]
    fn array_index_inline_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn single_read() -> i32 {
                let arr: [i32; 1] = [42];
                return arr[0];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "single_read")
            .expect("Failed to get 'single_read'");
        let result = func.call(&mut store, ()).expect("single_read failed");
        assert_eq!(result, 42, "arr[0] of [42] should be 42");
    }

    #[test]
    fn array_assign_test() {
        cov_mark::check_count!(wasm_codegen_emit_array_index_write, 9);
        cov_mark::check_count!(wasm_codegen_emit_array_index_read, 11);
        cov_mark::check_count!(wasm_codegen_emit_array_literal, 5);
        cov_mark::check_count!(wasm_codegen_emit_stack_prologue, 5);
        cov_mark::check_count!(wasm_codegen_emit_stack_epilogue, 11);
        let test_name = "array_assign";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {e}"));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn array_assign_execution_test() {
        use wasmtime::{Engine, Module, Store};

        let test_name = "array_assign";
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

        let stack_pointer = instance
            .get_global(&mut store, "__stack_pointer")
            .expect("Module should export '__stack_pointer'");
        let initial_sp = stack_pointer.get(&mut store).i32().unwrap();

        // write_and_read: arr[0] = 42, return arr[0] -> 42
        let write_and_read: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "write_and_read")
            .expect("Failed to get 'write_and_read'");
        let result = write_and_read
            .call(&mut store, ())
            .expect("write_and_read failed");
        assert_eq!(result, 42, "arr[0] = 42; arr[0] should be 42");

        // write_multiple: arr[0]=10, arr[1]=20, arr[2]=30, return sum -> 60
        let write_multiple: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "write_multiple")
            .expect("Failed to get 'write_multiple'");
        let result = write_multiple
            .call(&mut store, ())
            .expect("write_multiple failed");
        assert_eq!(
            result, 60,
            "arr[0]+arr[1]+arr[2] after writes should be 60"
        );

        // swap_elements: arr=[1,2], swap -> arr=[2,1], return arr[0]*10+arr[1] -> 21
        let swap_elements: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "swap_elements")
            .expect("Failed to get 'swap_elements'");
        let result = swap_elements
            .call(&mut store, ())
            .expect("swap_elements failed");
        assert_eq!(
            result, 21,
            "After swap [1,2]->[2,1], arr[0]*10+arr[1] should be 21"
        );

        // write_computed_index(1): arr[1+1]=arr[2]=99, return arr[2] -> 99
        let write_computed_index: wasmtime::TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "write_computed_index")
            .expect("Failed to get 'write_computed_index'");
        let result = write_computed_index
            .call(&mut store, 1)
            .expect("write_computed_index(1) failed");
        assert_eq!(result, 99, "arr[i+1] where i=1 should write to arr[2]=99");

        // write_bool: flags[0]=true, flags[2]=true, check both -> 1
        let write_bool: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "write_bool")
            .expect("Failed to get 'write_bool'");
        let result = write_bool
            .call(&mut store, ())
            .expect("write_bool failed");
        assert_eq!(
            result, 1,
            "flags[0]=true, flags[2]=true, both checked -> 1"
        );

        // Verify stack pointer is fully restored after all calls
        let final_sp = stack_pointer.get(&mut store).i32().unwrap();
        assert_eq!(
            final_sp, initial_sp,
            "Stack pointer should be restored after all calls"
        );
    }
}

/// Test data regeneration helpers.
///
/// These functions regenerate the expected `.wasm` test data files from the current
/// compiler output. Run with `--ignored` flag to execute:
///
/// ```bash
/// cargo test -p inference-tests codegen::wasm::base::regenerate -- --ignored
/// ```
///
/// These are gated behind `#[ignore]` so they do not run during normal test execution.
/// Use them when the codegen pipeline changes produce functionally correct but
/// byte-different WASM output (e.g., after architecture refactoring).
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    /// Base directory for codegen/wasm/base test data.
    fn base_test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("base")
    }

    #[test]
    #[ignore]
    fn regenerate_trivial_wasm() {
        let dir = base_test_dir().join("trivial");
        let source_code =
            std::fs::read_to_string(dir.join("trivial.inf")).expect("Failed to read trivial.inf");
        let actual = wasm_codegen(&source_code);
        let wasm_path = dir.join("trivial.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "trivial");
    }

    #[test]
    #[ignore]
    fn regenerate_const_wasm() {
        let dir = base_test_dir().join("const");
        let source_code =
            std::fs::read_to_string(dir.join("const.inf")).expect("Failed to read const.inf");
        let actual = wasm_codegen(&source_code);
        let wasm_path = dir.join("const.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "const");
    }

    #[test]
    #[ignore]
    fn regenerate_nondet_wasm() {
        let dir = base_test_dir().join("nondet");
        let source_code =
            std::fs::read_to_string(dir.join("nondet.inf")).expect("Failed to read nondet.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("nondet.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "nondet");
    }

    #[test]
    #[ignore]
    fn regenerate_i64_uzumaki_wasm() {
        let dir = base_test_dir().join("i64_uzumaki");
        let source_code = std::fs::read_to_string(dir.join("i64_uzumaki.inf"))
            .expect("Failed to read i64_uzumaki.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("i64_uzumaki.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "i64_uzumaki");
    }

    #[test]
    #[ignore]
    fn regenerate_bool_literal_wasm() {
        let dir = base_test_dir().join("bool_literal");
        let source_code = std::fs::read_to_string(dir.join("bool_literal.inf"))
            .expect("Failed to read bool_literal.inf");
        let actual = wasm_codegen(&source_code);
        let wasm_path = dir.join("bool_literal.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "bool_literal");
    }

    #[test]
    #[ignore]
    fn regenerate_mixed_visibility_wasm() {
        let dir = base_test_dir().join("mixed_visibility");
        let source_code = std::fs::read_to_string(dir.join("mixed_visibility.inf"))
            .expect("Failed to read mixed_visibility.inf");
        let actual = wasm_codegen(&source_code);
        let wasm_path = dir.join("mixed_visibility.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "mixed_visibility");
    }

    #[test]
    #[ignore]
    fn regenerate_bool_const_wasm() {
        let dir = base_test_dir().join("bool_const");
        let source_code = std::fs::read_to_string(dir.join("bool_const.inf"))
            .expect("Failed to read bool_const.inf");
        let actual = wasm_codegen(&source_code);
        let wasm_path = dir.join("bool_const.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "bool_const");
    }

    #[test]
    #[ignore]
    fn regenerate_numeric_literals_wasm() {
        let dir = base_test_dir().join("numeric_literals");
        let source_code = std::fs::read_to_string(dir.join("numeric_literals.inf"))
            .expect("Failed to read numeric_literals.inf");
        let actual = wasm_codegen(&source_code);
        let wasm_path = dir.join("numeric_literals.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "numeric_literals");
    }

    #[test]
    #[ignore]
    fn regenerate_local_variables_wasm() {
        let dir = base_test_dir().join("local_variables");
        let source_code = std::fs::read_to_string(dir.join("local_variables.inf"))
            .expect("Failed to read local_variables.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("local_variables.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "local_variables");
    }

    #[test]
    #[ignore]
    fn regenerate_local_variables_exec_wasm() {
        let dir = base_test_dir().join("local_variables_exec");
        let source_code = std::fs::read_to_string(dir.join("local_variables_exec.inf"))
            .expect("Failed to read local_variables_exec.inf");
        let actual = wasm_codegen(&source_code);
        let wasm_path = dir.join("local_variables_exec.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "local_variables_exec");
    }

    #[test]
    #[ignore]
    fn regenerate_fn_params_wasm() {
        let dir = base_test_dir().join("fn_params");
        let source_code =
            std::fs::read_to_string(dir.join("fn_params.inf")).expect("Failed to read fn_params.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("fn_params.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "fn_params");
    }

    #[test]
    #[ignore]
    fn regenerate_binary_ops_wasm() {
        let dir = base_test_dir().join("binary_ops");
        let source_code = std::fs::read_to_string(dir.join("binary_ops.inf"))
            .expect("Failed to read binary_ops.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("binary_ops.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "binary_ops");
    }

    #[test]
    #[ignore]
    fn regenerate_fn_calls_wasm() {
        let dir = base_test_dir().join("fn_calls");
        let source_code =
            std::fs::read_to_string(dir.join("fn_calls.inf")).expect("Failed to read fn_calls.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("fn_calls.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "fn_calls");
    }

    #[test]
    #[ignore]
    fn regenerate_if_else_wasm() {
        let dir = base_test_dir().join("if_else");
        let source_code =
            std::fs::read_to_string(dir.join("if_else.inf")).expect("Failed to read if_else.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("if_else.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "if_else");
    }

    #[test]
    #[ignore]
    fn regenerate_if_bool_exprs_wasm() {
        let dir = base_test_dir().join("if_bool_exprs");
        let source_code = std::fs::read_to_string(dir.join("if_bool_exprs.inf"))
            .expect("Failed to read if_bool_exprs.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("if_bool_exprs.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "if_bool_exprs");
    }

    #[test]
    #[ignore]
    fn regenerate_if_nondet_wasm() {
        let dir = base_test_dir().join("if_nondet");
        let source_code = std::fs::read_to_string(dir.join("if_nondet.inf"))
            .expect("Failed to read if_nondet.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("if_nondet.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "if_nondet");
    }

    #[test]
    #[ignore]
    fn regenerate_assign_wasm() {
        let dir = base_test_dir().join("assign");
        let source_code =
            std::fs::read_to_string(dir.join("assign.inf")).expect("Failed to read assign.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("assign.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "assign");
    }

    #[test]
    #[ignore]
    fn regenerate_assign_nondet_wasm() {
        let dir = base_test_dir().join("assign_nondet");
        let source_code = std::fs::read_to_string(dir.join("assign_nondet.inf"))
            .expect("Failed to read assign_nondet.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("assign_nondet.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "assign_nondet");
    }

    #[test]
    #[ignore]
    fn regenerate_array_literal_wasm() {
        let dir = base_test_dir().join("array_literal");
        let source_code = std::fs::read_to_string(dir.join("array_literal.inf"))
            .expect("Failed to read array_literal.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("array_literal.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "array_literal");
    }

    #[test]
    #[ignore]
    fn regenerate_array_index_wasm() {
        let dir = base_test_dir().join("array_index");
        let source_code = std::fs::read_to_string(dir.join("array_index.inf"))
            .expect("Failed to read array_index.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("array_index.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "array_index");
    }

    #[test]
    #[ignore]
    fn regenerate_array_assign_wasm() {
        let dir = base_test_dir().join("array_assign");
        let source_code = std::fs::read_to_string(dir.join("array_assign.inf"))
            .expect("Failed to read array_assign.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("array_assign.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "array_assign");
    }
}
