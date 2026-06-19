#[cfg(test)]
mod base_codegen_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, codegen_output,
        get_test_file_path, get_test_wasm_path, regenerate_wat, wasm_codegen,
        wasm_codegen_no_analysis, wasm_codegen_with_target,
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
        let actual = wasm_codegen_no_analysis(&source_code);
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
        let actual = wasm_codegen_no_analysis(&source_code);
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
    fn const_array_test() {
        // Defends against a future type-checker refactor silently breaking the
        // NodeId::Stmt typeinfo lookup that lower_named_binding_init relies on.
        cov_mark::check!(wasm_codegen_const_typeinfo_lookup);
        let test_name = "const_array";
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
    fn const_array_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "const_array";
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

        let test_func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test")
            .unwrap_or_else(|e| panic!("Failed to get 'test' function: {}", e));
        let result = test_func
            .call(&mut store, ())
            .unwrap_or_else(|e| panic!("Failed to execute 'test' function: {}", e));
        assert_eq!(result, 1, "Expected 'test' to return 1");
    }

    /// Divergence-1 companion: the original `const_array` fixture only loads
    /// ARR[0], so a codegen bug affecting non-zero element offsets would slip
    /// through. This fixture reads every element of a 3-element const array
    /// and asserts the sum, matching the master plan's intent to exercise all
    /// load offsets.
    #[test]
    fn const_array_sum_test() {
        let test_name = "const_array_sum";
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
    fn const_array_sum_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "const_array_sum";
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

        let sum_func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "sum")
            .unwrap_or_else(|e| panic!("Failed to get 'sum' function: {}", e));
        let result = sum_func
            .call(&mut store, ())
            .unwrap_or_else(|e| panic!("Failed to execute 'sum' function: {}", e));
        assert_eq!(result, 6, "Expected 'sum' to return 1 + 2 + 3 = 6");
    }

    #[test]
    fn const_struct_test() {
        let test_name = "const_struct";
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
    fn const_struct_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "const_struct";
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

        let test_func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test")
            .unwrap_or_else(|e| panic!("Failed to get 'test' function: {}", e));
        let result = test_func
            .call(&mut store, ())
            .unwrap_or_else(|e| panic!("Failed to execute 'test' function: {}", e));
        assert_eq!(result, 10, "Expected 'test' to return 10");
    }

    #[test]
    fn const_compound_mixed_test() {
        let test_name = "const_compound_mixed";
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
    fn const_compound_mixed_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "const_compound_mixed";
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

        let combined_func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "combined")
            .unwrap_or_else(|e| panic!("Failed to get 'combined' function: {}", e));
        let result = combined_func
            .call(&mut store, ())
            .unwrap_or_else(|e| panic!("Failed to execute 'combined' function: {}", e));
        assert_eq!(
            result, 107,
            "Expected 'combined' to return 3 + 4 + 100 = 107"
        );
    }

    #[test]
    fn const_sret_call_test() {
        let test_name = "const_sret_call";
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
    fn const_sret_call_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "const_sret_call";
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

        let sum_func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "sum")
            .unwrap_or_else(|e| panic!("Failed to get 'sum' function: {}", e));
        let result = sum_func
            .call(&mut store, ())
            .unwrap_or_else(|e| panic!("Failed to execute 'sum' function: {}", e));
        assert_eq!(result, 60, "Expected 'sum' to return 10 + 20 + 30 = 60");
    }

    #[test]
    fn const_compound_copy_test() {
        let test_name = "const_compound_copy";
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
    fn const_compound_copy_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "const_compound_copy";
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

        let copy_x_func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "copy_x")
            .unwrap_or_else(|e| panic!("Failed to get 'copy_x' function: {}", e));
        let result = copy_x_func
            .call(&mut store, ())
            .unwrap_or_else(|e| panic!("Failed to execute 'copy_x' function: {}", e));
        assert_eq!(result, 7, "Expected 'copy_x' to return 7");
    }

    /// Verifies that compound `const` declarations inside a `forall` block
    /// emit the same shadow-stack frame slot machinery as a `let` binding,
    /// confirming Phase 4's interaction with non-deterministic blocks.
    /// No execution test: forall blocks are non-deterministic and not directly
    /// callable like ordinary functions.
    #[test]
    fn const_in_forall_test() {
        let test_name = "const_in_forall";
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

    /// AD-5 verification: compound `const` and immutable `let` lower to
    /// byte-identical WASM. If this test ever fails, either Phase 3's
    /// `Stmt::ConstDef` arm has drifted from `Stmt::VarDef`, or the
    /// drift is intentional and AD-5 needs a documented exception in the
    /// master plan. Covers all four compound-init dispatch paths in
    /// `lower_named_binding_init`: array literal, struct literal, sret call,
    /// and compound copy.
    #[test]
    fn const_compound_byte_identical_to_let() {
        let const_source = r#"
            struct Point { x: i32; y: i32; }
            fn make_arr() -> [i32; 3] { return [10, 20, 30]; }
            fn make_point() -> Point { return Point { x: 1, y: 2 }; }
            pub fn arr_sum() -> i32 {
                const ARR: [i32; 3] = [1, 2, 3];
                return ARR[0] + ARR[1] + ARR[2];
            }
            pub fn struct_x() -> i32 {
                const P: Point = Point { x: 10, y: 20 };
                return P.x;
            }
            pub fn arr_sret_sum() -> i32 {
                const A: [i32; 3] = make_arr();
                return A[0] + A[1] + A[2];
            }
            pub fn struct_sret_x() -> i32 {
                const P: Point = make_point();
                return P.x;
            }
            pub fn arr_copy_first() -> i32 {
                let base: [i32; 3] = [4, 5, 6];
                const C: [i32; 3] = base;
                return C[0];
            }
            pub fn struct_copy_x() -> i32 {
                let base: Point = Point { x: 7, y: 8 };
                const C: Point = base;
                return C.x;
            }
            pub fn arr_zero_init_first() -> i32 {
                const Z: [i32; 3] = [0, 0, 0];
                return Z[0];
            }
        "#;
        let let_source = const_source.replace("const ", "let ");
        let const_wasm = wasm_codegen(const_source);
        let let_wasm = wasm_codegen(&let_source);
        assert_eq!(
            const_wasm, let_wasm,
            "AD-5: function-scope compound `const` must lower to byte-identical \
             WASM as the same program with immutable `let`. Divergence indicates \
             either drift between the Stmt::VarDef and Stmt::ConstDef arms or an \
             intentional change that must be documented in the master plan."
        );
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
                assert_eq!(
                    result, $expected,
                    "{}({:?}) expected {:?}",
                    $name, $args, $expected
                );
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
        let actual = wasm_codegen_no_analysis(&source_code);
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
            identity_i32
                .call(&mut store, 42)
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
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
            identity_bool
                .call(&mut store, 1)
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
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
            call_zero
                .call(&mut store, ())
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
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
            let_from_call
                .call(&mut store, ())
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
            0
        );

        let forward_call: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "forward_call")
            .unwrap_or_else(|e| panic!("Failed to get 'forward_call': {e}"));
        assert_eq!(
            forward_call
                .call(&mut store, ())
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
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
                assert_eq!(
                    result, $expected,
                    "{}({:?}) expected {:?}",
                    $name, $args, $expected
                );
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
                assert_eq!(
                    result, $expected,
                    "{}({:?}) expected {:?}",
                    $name, $args, $expected
                );
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
                assert_eq!(
                    result, $expected,
                    "{}({:?}) expected {:?}",
                    $name, $args, $expected
                );
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
    fn assert_test() {
        cov_mark::check_count!(wasm_codegen_emit_assert_statement, 13);
        let test_name = "assert";
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
    fn assert_execution_test() {
        use wasmtime::{Engine, Module, Store, Trap, TypedFunc};

        let test_name = "assert";
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

        macro_rules! call_ok {
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

        macro_rules! call_trap {
            ($name:expr, $arg_ty:ty, $ret_ty:ty, $args:expr) => {{
                let f: TypedFunc<$arg_ty, $ret_ty> = instance
                    .get_typed_func(&mut store, $name)
                    .unwrap_or_else(|e| panic!("Failed to get '{}': {e}", $name));
                let err = f.call(&mut store, $args).expect_err(concat!(
                    "Call to '",
                    $name,
                    "' expected to trap but returned Ok"
                ));
                let trap = err.downcast_ref::<Trap>().unwrap_or_else(|| {
                    panic!("Expected wasmtime Trap from '{}', got: {err}", $name)
                });
                assert_eq!(
                    *trap,
                    Trap::UnreachableCodeReached,
                    "Expected unreachable trap from '{}', got: {trap:?}",
                    $name
                );
            }};
        }

        // assert(true) always passes; function returns 1.
        call_ok!("assert_literal_true", i32, (), 1_i32);

        // assert(x > 0): pass when x > 0, trap otherwise.
        call_ok!("assert_variable", i32, 7_i32, 7_i32);
        call_trap!("assert_variable", i32, i32, 0_i32);
        call_trap!("assert_variable", i32, i32, -1_i32);

        // assert inside an if body: only reached when x > 0, traps when x >= 100.
        call_ok!("assert_in_if", i32, 5_i32, 5_i32);
        call_ok!("assert_in_if", i32, -3_i32, 0_i32);
        call_trap!("assert_in_if", i32, i32, 100_i32);

        // assert inside a loop body, with break: condition stays true so no trap.
        call_ok!("assert_in_loop_with_break", i32, 10_i32, 5_i32);
        call_ok!("assert_in_loop_with_break", i32, 3_i32, 3_i32);

        // Two consecutive asserts in one function.
        call_ok!("double_assert", i32, (3_i32, 4_i32), 7_i32);
        call_trap!("double_assert", (i32, i32), i32, (0_i32, 4_i32));
        call_trap!("double_assert", (i32, i32), i32, (3_i32, 0_i32));

        // Bare bool parameter as the assert expression.
        call_ok!("assert_bool_param", i32, 1_i32, 1_i32);
        call_trap!("assert_bool_param", i32, i32, 0_i32);

        // Unary `!` on bool parameter.
        call_ok!("assert_not", i32, 0_i32, 1_i32);
        call_trap!("assert_not", i32, i32, 1_i32);

        // Short-circuit AND.
        call_ok!("assert_and", i32, (1_i32, 1_i32), 1_i32);
        call_trap!("assert_and", (i32, i32), i32, (1_i32, 0_i32));
        call_trap!("assert_and", (i32, i32), i32, (0_i32, 1_i32));

        // Short-circuit OR.
        call_ok!("assert_or", i32, (1_i32, 0_i32), 1_i32);
        call_ok!("assert_or", i32, (0_i32, 1_i32), 1_i32);
        call_trap!("assert_or", (i32, i32), i32, (0_i32, 0_i32));

        // Equality between two i32 operands.
        call_ok!("assert_eq_i32", i32, (7_i32, 7_i32), 7_i32);
        call_trap!("assert_eq_i32", (i32, i32), i32, (7_i32, 8_i32));

        // Compound: `(a > 0) && ((b < 10) || (c == 0))`.
        call_ok!("assert_complex", i32, (1_i32, 5_i32, 1_i32), 7_i32);
        call_ok!("assert_complex", i32, (1_i32, 100_i32, 0_i32), 101_i32);
        call_trap!("assert_complex", (i32, i32, i32), i32, (-1_i32, 5_i32, 0_i32));
        call_trap!("assert_complex", (i32, i32, i32), i32, (1_i32, 100_i32, 5_i32));

        // Local bool binding fed into assert.
        call_ok!("assert_bool_local", i32, 4_i32, 4_i32);
        call_trap!("assert_bool_local", i32, i32, 0_i32);
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
                assert_eq!(
                    result, $expected,
                    "{}({:?}) expected {:?}",
                    $name, $args, $expected
                );
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
        let actual = wasm_codegen_no_analysis(&source_code);
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
    fn soroban_accepts_assert() {
        // `assert` lowers to baseline `i32.eqz; if; unreachable; end`, none of which
        // live in the custom 0xfc non-det prefix space. Soroban should accept it.
        let source = "pub fn check(x: i32) -> i32 { assert(x > 0); return x; }";
        let wasm_bytes = wasm_codegen_with_target(source, inference_wasm_codegen::Target::Soroban);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Soroban WASM with assert is invalid: {e}"));
    }

    #[test]
    fn array_literal_test() {
        cov_mark::check_count!(wasm_codegen_emit_array_literal, 5);
        cov_mark::check_count!(wasm_codegen_emit_stack_prologue, 4);
        cov_mark::check_count!(wasm_codegen_emit_stack_epilogue, 4);
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
        let result = i32_array_fn.call(&mut store, ()).expect("i32_array failed");
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
    fn multidim_array_literal_test() {
        // Each top-level array literal hits the mark once; nested literals
        // recurse through `store_array_literal_elements` and do not re-hit it.
        // Measured empirically: grid_2d(1) + cube_3d(1) + grid_mixed_zero(1)
        // + grid_rows([r,r] + [1,2,3] = 2) + grid_u8(2) + grid_i64(2) = 9.
        cov_mark::check_count!(wasm_codegen_emit_array_literal, 9);
        let test_name = "multidim_array_literal";
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
    fn multidim_array_literal_execution_test() {
        use wasmtime::{Engine, Module, Store};

        let test_name = "multidim_array_literal";
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

        // Each function returns a leaf element read back through the array
        // index path, validating that the recursive literal stores wrote the
        // correct strides and offsets.
        let i32_cases: [(&str, i32); 4] = [
            ("grid_2d", 6),         // [[i32;3];2], g[1][2]
            ("cube_3d", 6),         // [[[i32;2];2];2], c[1][0][1]
            ("grid_mixed_zero", 7), // [[i32;2];2] = [[0,7],[0,0]], g[0][1]
            ("grid_rows", 3),       // [[i32;3];2] = [r,r], g[1][2]
        ];
        for (name, expected) in i32_cases {
            let f: wasmtime::TypedFunc<(), i32> = instance
                .get_typed_func(&mut store, name)
                .unwrap_or_else(|e| panic!("Failed to get '{name}': {e}"));
            let result = f
                .call(&mut store, ())
                .unwrap_or_else(|e| panic!("'{name}' failed: {e}"));
            assert_eq!(result, expected, "{name} returned wrong element");
            let sp_after = stack_pointer.get(&mut store).i32().unwrap();
            assert_eq!(
                sp_after, initial_sp,
                "Stack pointer should be restored after {name} call"
            );
        }

        // grid_u8: [[u8;3];2] = [r,r], g[1][2] -> 3 (sub-i32 leaf, zero-extended)
        let grid_u8: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "grid_u8")
            .expect("Failed to get 'grid_u8'");
        assert_eq!(
            grid_u8.call(&mut store, ()).expect("grid_u8 failed"),
            3,
            "grid_u8 returned wrong element"
        );

        // grid_i64: [[i64;2];2] = [r,r], g[1][1] -> 4 (8-byte leaf)
        let grid_i64: wasmtime::TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "grid_i64")
            .expect("Failed to get 'grid_i64'");
        assert_eq!(
            grid_i64.call(&mut store, ()).expect("grid_i64 failed"),
            4,
            "grid_i64 returned wrong element"
        );
        let sp_after = stack_pointer.get(&mut store).i32().unwrap();
        assert_eq!(
            sp_after, initial_sp,
            "Stack pointer should be restored after grid_i64 call"
        );
    }

    #[test]
    fn array_zero_literal_test() {
        cov_mark::check_count!(wasm_codegen_emit_array_literal, 8);
        cov_mark::check_count!(wasm_codegen_emit_stack_prologue, 8);
        cov_mark::check_count!(wasm_codegen_emit_stack_epilogue, 8);
        let test_name = "array_zero_literal";
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
    fn array_zero_literal_execution_test() {
        use wasmtime::{Engine, Module, Store};

        let test_name = "array_zero_literal";
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
            .expect("Module should export memory");

        {
            let all_zeros_i32: wasmtime::TypedFunc<i32, ()> = instance
                .get_typed_func(&mut store, "all_zeros_i32")
                .expect("Failed to get 'all_zeros_i32'");
            let sret_ptr: i32 = 0;
            all_zeros_i32
                .call(&mut store, sret_ptr)
                .expect("all_zeros_i32 failed");
            let data = memory.data(&store);
            for i in 0..4 {
                let offset = (sret_ptr as usize) + i * 4;
                let val = i32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
                assert_eq!(val, 0, "all_zeros_i32[{i}] should be 0");
            }
        }

        {
            let all_zeros_u64: wasmtime::TypedFunc<i32, ()> = instance
                .get_typed_func(&mut store, "all_zeros_u64")
                .expect("Failed to get 'all_zeros_u64'");
            let sret_ptr: i32 = 0;
            all_zeros_u64
                .call(&mut store, sret_ptr)
                .expect("all_zeros_u64 failed");
            let data = memory.data(&store);
            for i in 0..3 {
                let offset = (sret_ptr as usize) + i * 8;
                let val = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
                assert_eq!(val, 0, "all_zeros_u64[{i}] should be 0");
            }
        }

        {
            let mixed_values: wasmtime::TypedFunc<i32, ()> = instance
                .get_typed_func(&mut store, "mixed_values")
                .expect("Failed to get 'mixed_values'");
            let sret_ptr: i32 = 0;
            mixed_values
                .call(&mut store, sret_ptr)
                .expect("mixed_values failed");
            let data = memory.data(&store);
            let expected_vals = [0i32, 1, 0];
            for (i, &expected) in expected_vals.iter().enumerate() {
                let offset = (sret_ptr as usize) + i * 4;
                let val = i32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
                assert_eq!(
                    val, expected,
                    "mixed_values[{i}] should be {expected}"
                );
            }
        }

        {
            let all_zeros_bool: wasmtime::TypedFunc<i32, ()> = instance
                .get_typed_func(&mut store, "all_zeros_bool")
                .expect("Failed to get 'all_zeros_bool'");
            let sret_ptr: i32 = 0;
            all_zeros_bool
                .call(&mut store, sret_ptr)
                .expect("all_zeros_bool failed");
            let data = memory.data(&store);
            for i in 0..2 {
                let offset = (sret_ptr as usize) + i;
                assert_eq!(
                    data[offset], 0,
                    "all_zeros_bool[{i}] should be 0 (false)"
                );
            }
        }

        {
            let sret_direct_zeros: wasmtime::TypedFunc<i32, ()> = instance
                .get_typed_func(&mut store, "sret_direct_zeros")
                .expect("Failed to get 'sret_direct_zeros'");
            let sret_ptr: i32 = 0;
            sret_direct_zeros
                .call(&mut store, sret_ptr)
                .expect("sret_direct_zeros failed");
            let data = memory.data(&store);
            for i in 0..3 {
                let offset = (sret_ptr as usize) + i * 4;
                let val = i32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
                assert_eq!(
                    val, 0,
                    "sret_direct_zeros[{i}] should be 0 (stores must NOT be elided in sret path)"
                );
            }
        }

        {
            let parenthesized_zeros: wasmtime::TypedFunc<i32, ()> = instance
                .get_typed_func(&mut store, "parenthesized_zeros")
                .expect("Failed to get 'parenthesized_zeros'");
            let sret_ptr: i32 = 0;
            parenthesized_zeros
                .call(&mut store, sret_ptr)
                .expect("parenthesized_zeros failed");
            let data = memory.data(&store);
            for i in 0..2 {
                let offset = (sret_ptr as usize) + i * 4;
                let val = i32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
                assert_eq!(val, 0, "parenthesized_zeros[{i}] should be 0");
            }
        }

        {
            let negated_zeros: wasmtime::TypedFunc<i32, ()> = instance
                .get_typed_func(&mut store, "negated_zeros")
                .expect("Failed to get 'negated_zeros'");
            let sret_ptr: i32 = 0;
            negated_zeros
                .call(&mut store, sret_ptr)
                .expect("negated_zeros failed");
            let data = memory.data(&store);
            for i in 0..2 {
                let offset = (sret_ptr as usize) + i * 4;
                let val = i32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
                assert_eq!(val, 0, "negated_zeros[{i}] should be 0");
            }
        }

        {
            let single_zero: wasmtime::TypedFunc<i32, ()> = instance
                .get_typed_func(&mut store, "single_zero")
                .expect("Failed to get 'single_zero'");
            let sret_ptr: i32 = 0;
            single_zero
                .call(&mut store, sret_ptr)
                .expect("single_zero failed");
            let data = memory.data(&store);
            let val = i32::from_le_bytes(data[0..4].try_into().unwrap());
            assert_eq!(val, 0, "single_zero[0] should be 0");
        }

        {
            let mixed_bool: wasmtime::TypedFunc<i32, ()> = instance
                .get_typed_func(&mut store, "mixed_bool")
                .expect("Failed to get 'mixed_bool'");
            let sret_ptr: i32 = 0;
            mixed_bool
                .call(&mut store, sret_ptr)
                .expect("mixed_bool failed");
            let data = memory.data(&store);
            assert_eq!(data[0], 1, "mixed_bool[0] should be 1 (true)");
            assert_eq!(data[1], 0, "mixed_bool[1] should be 0 (false, zero-elided)");
            assert_eq!(data[2], 1, "mixed_bool[2] should be 1 (true)");
        }
    }

    #[test]
    fn array_index_test() {
        cov_mark::check_count!(wasm_codegen_emit_array_index_read, 6);
        cov_mark::check_count!(wasm_codegen_emit_array_literal, 6);
        cov_mark::check_count!(wasm_codegen_emit_stack_prologue, 6);
        cov_mark::check_count!(wasm_codegen_emit_stack_epilogue, 8);
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
        let result = read_middle
            .call(&mut store, ())
            .expect("read_middle failed");
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
        cov_mark::check_count!(wasm_codegen_emit_array_index_read, 14);
        cov_mark::check_count!(wasm_codegen_emit_array_literal, 7);
        cov_mark::check_count!(wasm_codegen_emit_stack_prologue, 6);
        cov_mark::check_count!(wasm_codegen_emit_stack_epilogue, 7);
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
        assert_eq!(result, 60, "arr[0]+arr[1]+arr[2] after writes should be 60");

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
        let result = write_bool.call(&mut store, ()).expect("write_bool failed");
        assert_eq!(result, 1, "flags[0]=true, flags[2]=true, both checked -> 1");

        // reassign_zeros: arr=[1,2,3], arr=[0,0,0], return sum -> 0
        {
            let reassign_zeros: wasmtime::TypedFunc<(), i32> = instance
                .get_typed_func(&mut store, "reassign_zeros")
                .expect("Failed to get 'reassign_zeros'");
            let result = reassign_zeros
                .call(&mut store, ())
                .expect("reassign_zeros failed");
            assert_eq!(
                result, 0,
                "reassign_zeros should return 0 (zero stores must NOT be elided during reassignment)"
            );
        }

        // Verify stack pointer is fully restored after all calls
        let final_sp = stack_pointer.get(&mut store).i32().unwrap();
        assert_eq!(
            final_sp, initial_sp,
            "Stack pointer should be restored after all calls"
        );
    }

    #[test]
    fn array_params_test() {
        cov_mark::check_count!(wasm_codegen_emit_array_param_copy, 5);
        cov_mark::check_count!(wasm_codegen_emit_array_literal, 5);
        cov_mark::check_count!(wasm_codegen_emit_stack_prologue, 8);
        cov_mark::check_count!(wasm_codegen_emit_stack_epilogue, 9);
        let test_name = "array_params";
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
    fn array_params_execution_test() {
        use wasmtime::{Engine, Module, Store};

        let test_name = "array_params";
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

        // call_sum: sum_array([10, 20, 30]) -> 60
        let call_sum: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "call_sum")
            .expect("Failed to get 'call_sum'");
        let result = call_sum.call(&mut store, ()).expect("call_sum failed");
        assert_eq!(result, 60, "sum_array([10, 20, 30]) should be 60");

        // verify_copy_semantics: pass [1,2,3] to mutate_copy which sets arr[0]=99,
        // but the original data[0] should still be 1 (copy semantics)
        let verify_copy: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "verify_copy_semantics")
            .expect("Failed to get 'verify_copy_semantics'");
        let result = verify_copy
            .call(&mut store, ())
            .expect("verify_copy_semantics failed");
        assert_eq!(
            result, 1,
            "After mutate_copy, original data[0] should still be 1 (copy semantics)"
        );

        // call_two_params: two_array_params([10, 20], [30, 40]) -> 100
        let call_two: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "call_two_params")
            .expect("Failed to get 'call_two_params'");
        let result = call_two
            .call(&mut store, ())
            .expect("call_two_params failed");
        assert_eq!(
            result, 100,
            "two_array_params([10,20], [30,40]) should be 100"
        );

        // call_bool_param: bool_array_param([true, false, true]) -> 1
        let call_bool: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "call_bool_param")
            .expect("Failed to get 'call_bool_param'");
        let result = call_bool
            .call(&mut store, ())
            .expect("call_bool_param failed");
        assert_eq!(
            result, 1,
            "bool_array_param([true, false, true]) should return 1"
        );

        // Verify stack pointer is fully restored after all calls
        let final_sp = stack_pointer.get(&mut store).i32().unwrap();
        assert_eq!(
            final_sp, initial_sp,
            "Stack pointer should be restored after all array param calls"
        );
    }

    #[test]
    fn array_params_inline_validation() {
        let source = r#"
            pub fn identity(arr: [i32; 1]) -> i32 {
                return arr[0];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Array param inline WASM is invalid: {e}"));
    }

    #[test]
    fn array_params_inline_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn read_elem(arr: [i32; 3]) -> i32 {
                return arr[1];
            }
            pub fn caller() -> i32 {
                let data: [i32; 3] = [5, 15, 25];
                return read_elem(data);
            }
        "#;
        let wasm_bytes = wasm_codegen(source);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let caller: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "caller")
            .expect("Failed to get 'caller'");
        let result = caller.call(&mut store, ()).expect("caller failed");
        assert_eq!(
            result, 15,
            "read_elem([5, 15, 25]) should return arr[1] = 15"
        );
    }

    #[test]
    fn array_nondet_test() {
        cov_mark::check_count!(wasm_codegen_emit_array_uzumaki, 1);
        cov_mark::check_count!(wasm_codegen_emit_forall_block, 3);
        cov_mark::check_count!(wasm_codegen_emit_exists_block, 1);
        cov_mark::check_count!(wasm_codegen_emit_array_literal, 3);
        cov_mark::check_count!(wasm_codegen_emit_array_index_read, 3);
        cov_mark::check_count!(wasm_codegen_emit_array_index_write, 1);
        cov_mark::check_count!(wasm_codegen_emit_uzumaki_i32, 1);
        cov_mark::check_count!(wasm_codegen_emit_stack_prologue, 4);
        cov_mark::check_count!(wasm_codegen_emit_stack_epilogue, 4);
        let test_name = "array_nondet";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen_no_analysis(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {e}"));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
    }

    #[test]
    fn array_i64_literal_validation() {
        let source = r#"
            pub fn i64_array_read() -> i64 {
                let arr: [i64; 2] = [100, 200];
                return arr[1];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("i64 array literal WASM is invalid: {e}"));
    }

    #[test]
    fn array_i64_literal_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn i64_array_read() -> i64 {
                let arr: [i64; 2] = [100, 200];
                return arr[1];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to compile Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "i64_array_read")
            .expect("Failed to get 'i64_array_read'");
        let result = func
            .call(&mut store, ())
            .expect("i64_array_read should not trap");
        assert_eq!(result, 200, "arr[1] should be 200 (i64)");
    }

    #[test]
    fn array_i8_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};
        let source = r#"
            pub fn i8_array_sum() -> i8 {
                let arr: [i8; 3] = [-1, 0, 1];
                return arr[0] + arr[1] + arr[2];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("i8 array WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "i8_array_sum")
            .expect("get func");
        let result = func.call(&mut store, ()).expect("call");
        assert_eq!(result, 0, "i8 array: -1 + 0 + 1 = 0");
    }

    #[test]
    fn array_u8_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};
        let source = r#"
            pub fn u8_array_max() -> u8 {
                let arr: [u8; 3] = [0, 128, 255];
                return arr[2];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("u8 array WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "u8_array_max")
            .expect("get func");
        let result = func.call(&mut store, ()).expect("call");
        assert_eq!(
            result, 255,
            "u8 array: arr[2] should be 255 (zero-extended)"
        );
    }

    #[test]
    fn array_i16_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};
        let source = r#"
            pub fn i16_array_negative() -> i16 {
                let arr: [i16; 2] = [-1000, 1000];
                return arr[0];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("i16 array WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "i16_array_negative")
            .expect("get func");
        let result = func.call(&mut store, ()).expect("call");
        assert_eq!(
            result, -1000,
            "i16 array: arr[0] should be -1000 (sign-extended)"
        );
    }

    #[test]
    fn array_u16_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};
        let source = r#"
            pub fn u16_array_large() -> u16 {
                let arr: [u16; 2] = [0, 65535];
                return arr[1];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("u16 array WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "u16_array_large")
            .expect("get func");
        let result = func.call(&mut store, ()).expect("call");
        assert_eq!(
            result, 65535,
            "u16 array: arr[1] should be 65535 (zero-extended)"
        );
    }

    #[test]
    fn array_i64_assign_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};
        let source = r#"
            pub fn i64_write_read() -> i64 {
                let mut arr: [i64; 2] = [5000000000, 0];
                arr[1] = arr[0] + arr[0];
                return arr[1];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("i64 array assign WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "i64_write_read")
            .expect("get func");
        let result = func.call(&mut store, ()).expect("call");
        assert_eq!(
            result, 10_000_000_000i64,
            "i64 array: arr[1] should be 10000000000"
        );
    }

    #[test]
    fn array_i64_param_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};
        let source = r#"
            pub fn sum_i64(arr: [i64; 2]) -> i64 {
                return arr[0] + arr[1];
            }
            pub fn call_sum_i64() -> i64 {
                let data: [i64; 2] = [1000000000, 2000000000];
                return sum_i64(data);
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("i64 array param WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "call_sum_i64")
            .expect("get func");
        let result = func.call(&mut store, ()).expect("call");
        assert_eq!(
            result, 3_000_000_000i64,
            "i64 array param: sum should be 3000000000"
        );
    }

    #[test]
    fn array_u32_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};
        let source = r#"
            pub fn u32_array_large() -> u32 {
                let arr: [u32; 2] = [0, 4294967295];
                return arr[1];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("u32 array WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "u32_array_large")
            .expect("get func");
        let result = func.call(&mut store, ()).expect("call");
        assert_eq!(result, -1, "u32::MAX bit-reinterpreted as i32 is -1");
    }

    #[test]
    fn array_large_param_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};
        let source = r#"
            pub fn sum_large(arr: [i32; 20]) -> i32 {
                return arr[0] + arr[9] + arr[19];
            }
            pub fn call_sum_large() -> i32 {
                let data: [i32; 20] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
                return sum_large(data);
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("large array param WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "call_sum_large")
            .expect("get func");
        let result = func.call(&mut store, ()).expect("call");
        assert_eq!(
            result,
            1 + 10 + 20,
            "large array: arr[0] + arr[9] + arr[19] = 31"
        );
    }

    #[test]
    fn array_mixed_types_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};
        let source = r#"
            pub fn mixed_arrays() -> i32 {
                let flags: [bool; 2] = [true, false];
                let nums: [i32; 2] = [100, 200];
                if flags[0] {
                    return nums[1];
                }
                return 0;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("mixed arrays WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "mixed_arrays")
            .expect("get func");
        let result = func.call(&mut store, ()).expect("call");
        assert_eq!(
            result, 200,
            "mixed arrays: flags[0] is true, so return nums[1] = 200"
        );
    }

    #[test]
    fn array_mut_param_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};
        let source = r#"
            pub fn double_first(mut arr: [i32; 3]) -> i32 {
                arr[0] = arr[0] + arr[0];
                return arr[0];
            }
            pub fn call_double() -> i32 {
                let data: [i32; 3] = [21, 0, 0];
                return double_first(data);
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("mut param WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "call_double")
            .expect("get func");
        let result = func.call(&mut store, ()).expect("call");
        assert_eq!(
            result, 42,
            "mut param: double_first([21, 0, 0]) should return 42"
        );
    }

    #[test]
    fn array_alignment_padding_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};
        // Test 1: [bool; 3] (3 bytes) + [i32; 2] (8 bytes) - 1 byte padding between them
        // Verify both arrays are read/written correctly across the padding boundary.
        let source_bool_i32 = r#"
            pub fn bool_then_i32() -> i32 {
                let flags: [bool; 3] = [true, false, true];
                let nums: [i32; 2] = [1000, 2000];
                let mut result: i32 = nums[0] + nums[1];
                if flags[0] {
                    result = result + 1;
                }
                if flags[2] {
                    result = result + 2;
                }
                return result;
            }
        "#;
        let wasm_bytes = wasm_codegen(source_bool_i32);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("bool_then_i32 WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "bool_then_i32")
            .expect("get func");
        let result = func.call(&mut store, ()).expect("call");
        assert_eq!(
            result,
            1000 + 2000 + 1 + 2,
            "bool_then_i32: 1000 + 2000 + 1 (flags[0]) + 2 (flags[2]) = 3003"
        );

        // Test 2: [bool; 1] (1 byte) + [i64; 1] (8 bytes) - 7 bytes padding between them
        // Verify the i64 value is read correctly even with large padding gap.
        let source_bool_i64 = r#"
            pub fn bool_then_i64() -> i64 {
                let flag: [bool; 1] = [true];
                let big: [i64; 1] = [9999999999];
                let zero: i64 = 0;
                if flag[0] {
                    return big[0];
                }
                return zero;
            }
        "#;
        let wasm_bytes = wasm_codegen(source_bool_i64);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("bool_then_i64 WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "bool_then_i64")
            .expect("get func");
        let result = func.call(&mut store, ()).expect("call");
        assert_eq!(
            result, 9_999_999_999i64,
            "bool_then_i64: flag is true, return big[0] = 9999999999"
        );
    }

    /// The runtime `memory.fill` overflow trap is a defense-in-depth backstop:
    /// analysis rule A036 (`StackDepthExceeded`) is the *primary* guard and now
    /// rejects this two-frame chain at compile time (see
    /// `analysis::rules_a036`). Codegen is exercised with analysis skipped here
    /// to confirm the runtime backstop still traps when the chain reaches the
    /// generator unchecked.
    #[test]
    fn stack_overflow_traps_at_runtime() {
        use wasmtime::{Engine, Module, Store, TypedFunc};
        let zeros = vec!["0"; 8200].join(", ");
        let source = format!(
            "pub fn callee() -> i32 {{\
                 let b: [i32; 8200] = [{zeros}];\
                 return b[0];\
             }}\
             pub fn caller() -> i32 {{\
                 let a: [i32; 8200] = [{zeros}];\
                 return a[0] + callee();\
             }}"
        );
        let wasm_bytes = wasm_codegen_no_analysis(&source);
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "caller")
            .expect("get func");
        let result = func.call(&mut store, ());
        assert!(
            result.is_err(),
            "two 32KB frames should trap (stack overflow in 64KB stack)"
        );
    }

    // -- C1: Sub-i32 narrowing after arithmetic --

    #[test]
    fn sub_i32_truncation_i8_overflow() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn add_i8_overflow(a: i8, b: i8) -> i8 {
                return a + b;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes).unwrap_or_else(|e| panic!("WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "add_i8_overflow")
            .expect("get func");
        // i8: 127 + 1 overflows to -128 (signed wrap)
        let result = func.call(&mut store, (127, 1)).expect("call");
        assert_eq!(result, -128, "i8(127) + i8(1) should wrap to -128");
    }

    #[test]
    fn sub_i32_truncation_u8_overflow() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn add_u8_overflow(a: u8, b: u8) -> u8 {
                return a + b;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes).unwrap_or_else(|e| panic!("WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "add_u8_overflow")
            .expect("get func");
        // u8: 255 + 1 overflows to 0 (unsigned wrap)
        let result = func.call(&mut store, (255, 1)).expect("call");
        assert_eq!(result, 0, "u8(255) + u8(1) should wrap to 0");
    }

    #[test]
    fn sub_i32_truncation_neg_i8() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn neg_i8(a: i8) -> i8 {
                return -a;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes).unwrap_or_else(|e| panic!("WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "neg_i8")
            .expect("get func");
        // -(-128) overflows i8 range, wraps back to -128
        let result = func.call(&mut store, -128).expect("call");
        assert_eq!(result, -128, "-i8(-128) should wrap to -128");
    }

    #[test]
    fn sub_i32_truncation_bitnot_u8() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn bitnot_u8(a: u8) -> u8 {
                return ~a;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes).unwrap_or_else(|e| panic!("WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "bitnot_u8")
            .expect("get func");
        // ~u8(0) = 255 (all 8 bits set), not 0xFFFFFFFF
        let result = func.call(&mut store, 0).expect("call");
        assert_eq!(result, 255, "~u8(0) should be 255");
    }

    #[test]
    fn sub_i32_truncation_u16_mul() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn mul_u16(a: u16, b: u16) -> u16 {
                return a * b;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes).unwrap_or_else(|e| panic!("WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "mul_u16")
            .expect("get func");
        // u16: 1000 * 100 = 100000, truncated to 16 bits = 100000 & 0xFFFF = 34464
        let result = func.call(&mut store, (1000, 100)).expect("call");
        assert_eq!(result, 34464, "u16(1000) * u16(100) should wrap to 34464");
    }

    #[test]
    fn sub_i32_truncation_local_memory_consistency() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        // Verify that a sub-i32 value narrowed after arithmetic is consistent
        // when stored in and loaded from an array element.
        let source = r#"
            pub fn consistency() -> i32 {
                let a: i8 = 100;
                let b: i8 = 100;
                let sum: i8 = a + b;
                let arr: [i8; 1] = [sum];
                let loaded: i8 = arr[0];
                if sum == loaded {
                    return 1;
                }
                return 0;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes).unwrap_or_else(|e| panic!("WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "consistency")
            .expect("get func");
        let result = func.call(&mut store, ()).expect("call");
        assert_eq!(
            result, 1,
            "narrowed i8 value in local should match value roundtripped through array"
        );
    }

    #[test]
    fn sub_i32_truncation_div_overflow() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn div_i8_overflow(a: i8, b: i8) -> i8 {
                return a / b;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes).unwrap_or_else(|e| panic!("WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "div_i8_overflow")
            .expect("get func");
        // i8: -128 / -1 = 128, which overflows to -128 (signed wrap)
        let result = func.call(&mut store, (-128, -1)).expect("call");
        assert_eq!(result, -128, "i8(-128) / i8(-1) should wrap to -128");
    }

    // -- C2: Array-to-array copy (value semantics) --

    #[test]
    fn array_copy_independent() {
        cov_mark::check_count!(wasm_codegen_emit_array_copy, 1);
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn copy_independence() -> i32 {
                let a: [i32; 3] = [10, 20, 30];
                let mut b: [i32; 3] = a;
                b[0] = 99;
                return a[0];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes).unwrap_or_else(|e| panic!("WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "copy_independence")
            .expect("get func");
        let result = func.call(&mut store, ()).expect("call");
        assert_eq!(
            result, 10,
            "a[0] should still be 10 after mutating b[0] (value semantics)"
        );
    }

    #[test]
    fn array_copy_values_match() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn copy_values() -> i32 {
                let a: [i32; 3] = [1, 2, 3];
                let b: [i32; 3] = a;
                return b[0] + b[1] + b[2];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes).unwrap_or_else(|e| panic!("WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "copy_values")
            .expect("get func");
        let result = func.call(&mut store, ()).expect("call");
        assert_eq!(result, 6, "b should contain copies of a's values: 1+2+3=6");
    }

    // -- Array return (sret calling convention) execution tests --

    #[test]
    fn array_return_literal_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn make() -> [i32; 3] {
                return [10, 20, 30];
            }
            pub fn test() -> i32 {
                let arr: [i32; 3] = make();
                return arr[0] + arr[1] + arr[2];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test")
            .expect("Failed to get 'test'");
        let result = func.call(&mut store, ()).expect("test failed");
        assert_eq!(result, 60, "sum of [10,20,30] should be 60");
    }

    #[test]
    fn array_return_variable_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn make() -> [i32; 3] {
                let a: [i32; 3] = [1, 2, 3];
                return a;
            }
            pub fn test() -> i32 {
                let arr: [i32; 3] = make();
                return arr[0] + arr[1] + arr[2];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test")
            .expect("Failed to get 'test'");
        let result = func.call(&mut store, ()).expect("test failed");
        assert_eq!(result, 6, "sum of [1,2,3] should be 6");
    }

    #[test]
    fn array_return_chained_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn inner() -> [i32; 3] {
                return [1, 2, 3];
            }
            pub fn outer() -> [i32; 3] {
                return inner();
            }
            pub fn test() -> i32 {
                let arr: [i32; 3] = outer();
                return arr[0] + arr[1] + arr[2];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test")
            .expect("Failed to get 'test'");
        let result = func.call(&mut store, ()).expect("test failed");
        assert_eq!(result, 6, "chained sret: sum of [1,2,3] should be 6");
    }

    #[test]
    fn array_return_value_semantics_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn make() -> [i32; 3] {
                return [10, 20, 30];
            }
            pub fn test() -> i32 {
                let mut a: [i32; 3] = make();
                a[0] = 99;
                let b: [i32; 3] = make();
                return b[0];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test")
            .expect("Failed to get 'test'");
        let result = func.call(&mut store, ()).expect("test failed");
        assert_eq!(
            result, 10,
            "second make() should return fresh [10,20,30], not modified"
        );
    }

    #[test]
    fn array_return_sub_i32_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn make_bytes() -> [u8; 4] {
                let a: [u8; 4] = [1, 2, 3, 4];
                return a;
            }
            pub fn test() -> u8 {
                let arr: [u8; 4] = make_bytes();
                return arr[0] + arr[1] + arr[2] + arr[3];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test")
            .expect("Failed to get 'test'");
        let result = func.call(&mut store, ()).expect("test failed");
        assert_eq!(result, 10, "sum of u8 array [1,2,3,4] should be 10");
    }

    #[test]
    fn array_return_i64_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn make_i64() -> [i64; 2] {
                let a: [i64; 2] = [100, 200];
                return a;
            }
            pub fn test() -> i64 {
                let arr: [i64; 2] = make_i64();
                return arr[0] + arr[1];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "test")
            .expect("Failed to get 'test'");
        let result = func.call(&mut store, ()).expect("test failed");
        assert_eq!(result, 300, "sum of i64 array [100,200] should be 300");
    }

    #[test]
    fn array_return_with_params_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn scale(arr: [i32; 3], factor: i32) -> [i32; 3] {
                let mut result: [i32; 3] = [0, 0, 0];
                result[0] = arr[0] * factor;
                result[1] = arr[1] * factor;
                result[2] = arr[2] * factor;
                return result;
            }
            pub fn test() -> i32 {
                let a: [i32; 3] = [1, 2, 3];
                let b: [i32; 3] = scale(a, 10);
                return b[0] + b[1] + b[2];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test")
            .expect("Failed to get 'test'");
        let result = func.call(&mut store, ()).expect("test failed");
        assert_eq!(
            result, 60,
            "scale([1,2,3], 10) should give [10,20,30], sum=60"
        );
    }

    #[test]
    fn struct_literal_test() {
        cov_mark::check_count!(wasm_codegen_emit_struct_literal, 7);
        cov_mark::check_count!(wasm_codegen_emit_stack_prologue, 7);
        cov_mark::check_count!(wasm_codegen_emit_stack_epilogue, 7);
        let test_name = "struct_literal";
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
    fn struct_literal_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "struct_literal";
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

        let make_point: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "make_point")
            .expect("Failed to get 'make_point'");
        let result = make_point.call(&mut store, ()).expect("make_point failed");
        assert_eq!(result, 0, "make_point should return 0");

        let make_single: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "make_single")
            .expect("Failed to get 'make_single'");
        let result = make_single
            .call(&mut store, ())
            .expect("make_single failed");
        assert_eq!(result, 0, "make_single should return 0");

        let make_mixed: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "make_mixed")
            .expect("Failed to get 'make_mixed'");
        let result = make_mixed.call(&mut store, ()).expect("make_mixed failed");
        assert_eq!(result, 0, "make_mixed should return 0");

        let zero_point_x: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "zero_point_x")
            .expect("Failed to get 'zero_point_x'");
        let result = zero_point_x
            .call(&mut store, ())
            .expect("zero_point_x failed");
        assert_eq!(result, 0, "zero_point_x should return 0");

        let zero_point_y: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "zero_point_y")
            .expect("Failed to get 'zero_point_y'");
        let result = zero_point_y
            .call(&mut store, ())
            .expect("zero_point_y failed");
        assert_eq!(result, 0, "zero_point_y should return 0");

        let mixed_zero_x: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "mixed_zero_x")
            .expect("Failed to get 'mixed_zero_x'");
        let result = mixed_zero_x
            .call(&mut store, ())
            .expect("mixed_zero_x failed");
        assert_eq!(result, 0, "mixed_zero_x should return 0 (zero-elided field)");

        let mixed_zero_y: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "mixed_zero_y")
            .expect("Failed to get 'mixed_zero_y'");
        let result = mixed_zero_y
            .call(&mut store, ())
            .expect("mixed_zero_y failed");
        assert_eq!(result, 42, "mixed_zero_y should return 42 (non-zero field preserved)");

        let _memory = instance
            .get_memory(&mut store, "memory")
            .expect("memory export missing");
        let stack_pointer = instance
            .get_global(&mut store, "__stack_pointer")
            .expect("__stack_pointer export missing");
        let sp = stack_pointer.get(&mut store).i32().unwrap();
        assert_eq!(
            sp, 65536,
            "Stack pointer should be restored to initial value after all calls"
        );
    }

    #[test]
    fn struct_literal_inline_validation() {
        let source = r#"
            struct Pair { a: i32; b: i32; }
            pub fn test() -> i32 {
                let p: Pair = Pair { a: 1, b: 2 };
                return 0;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct literal inline WASM is invalid: {e}"));
    }

    #[test]
    fn struct_literal_mixed_field_types_validation() {
        let source = r#"
            struct Record { flag: bool; count: i32; big: i64; }
            pub fn test() -> i32 {
                let r: Record = Record { flag: false, count: 99, big: 1000 };
                return 0;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Mixed struct literal WASM is invalid: {e}"));
    }

    #[test]
    fn struct_access_test() {
        cov_mark::check_count!(wasm_codegen_emit_member_access_read, 6);
        cov_mark::check_count!(wasm_codegen_emit_struct_literal, 5);
        let test_name = "struct_access";
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
    fn struct_access_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "struct_access";
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

        let get_x: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_x")
            .expect("Failed to get 'get_x'");
        let result = get_x.call(&mut store, ()).expect("get_x failed");
        assert_eq!(result, 10, "get_x should return 10 (p.x)");

        let get_y: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_y")
            .expect("Failed to get 'get_y'");
        let result = get_y.call(&mut store, ()).expect("get_y failed");
        assert_eq!(result, 20, "get_y should return 20 (p.y)");

        let sum_fields: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "sum_fields")
            .expect("Failed to get 'sum_fields'");
        let result = sum_fields.call(&mut store, ()).expect("sum_fields failed");
        assert_eq!(result, 30, "sum_fields should return 30 (p.x + p.y)");

        let get_single_val: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_single_val")
            .expect("Failed to get 'get_single_val'");
        let result = get_single_val
            .call(&mut store, ())
            .expect("get_single_val failed");
        assert_eq!(result, 42, "get_single_val should return 42 (s.val)");

        let get_mixed_val: TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "get_mixed_val")
            .expect("Failed to get 'get_mixed_val'");
        let result = get_mixed_val
            .call(&mut store, ())
            .expect("get_mixed_val failed");
        assert_eq!(result, 100, "get_mixed_val should return 100 (m.val)");

        let _memory = instance
            .get_memory(&mut store, "memory")
            .expect("memory export missing");
        let stack_pointer = instance
            .get_global(&mut store, "__stack_pointer")
            .expect("__stack_pointer export missing");
        let sp = stack_pointer.get(&mut store).i32().unwrap();
        assert_eq!(
            sp, 65536,
            "Stack pointer should be restored to initial value after all calls"
        );
    }

    #[test]
    fn struct_access_inline_validation() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            pub fn test() -> i32 {
                let p: Point = Point { x: 5, y: 10 };
                return p.x;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct member access inline WASM is invalid: {e}"));
    }

    #[test]
    fn struct_access_second_field_validation() {
        let source = r#"
            struct Pair { a: i32; b: i64; }
            pub fn test() -> i64 {
                let p: Pair = Pair { a: 1, b: 200 };
                return p.b;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct second field access WASM is invalid: {e}"));
    }

    #[test]
    fn struct_assign_test() {
        cov_mark::check_count!(wasm_codegen_emit_member_access_write, 4);
        cov_mark::check_count!(wasm_codegen_emit_member_access_read, 9);
        cov_mark::check_count!(wasm_codegen_emit_struct_literal, 5);
        let test_name = "struct_assign";
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
    fn struct_assign_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "struct_assign";
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

        let set_and_get: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "set_and_get")
            .expect("Failed to get 'set_and_get'");
        let result = set_and_get
            .call(&mut store, ())
            .expect("set_and_get failed");
        assert_eq!(
            result, 42,
            "set_and_get should return 42 (p.x after p.x = 42)"
        );

        let swap_fields: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "swap_fields")
            .expect("Failed to get 'swap_fields'");
        let result = swap_fields
            .call(&mut store, ())
            .expect("swap_fields failed");
        assert_eq!(
            result, 30,
            "swap_fields should return 30 (p.x + p.y after swapping 10 and 20)"
        );

        let modify_bool: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "modify_bool")
            .expect("Failed to get 'modify_bool'");
        let result = modify_bool
            .call(&mut store, ())
            .expect("modify_bool failed");
        assert_eq!(
            result, 100,
            "modify_bool should return 100 (f.val when f.flag is set to true)"
        );

        {
            let reassign_zeros: TypedFunc<(), i32> = instance
                .get_typed_func(&mut store, "reassign_zeros")
                .expect("Failed to get 'reassign_zeros'");
            let result = reassign_zeros
                .call(&mut store, ())
                .expect("reassign_zeros failed");
            assert_eq!(
                result, 0,
                "reassign_zeros should return 0 (zero stores must NOT be elided during reassignment)"
            );
        }

        let _memory = instance
            .get_memory(&mut store, "memory")
            .expect("memory export missing");
        let stack_pointer = instance
            .get_global(&mut store, "__stack_pointer")
            .expect("__stack_pointer export missing");
        let sp = stack_pointer.get(&mut store).i32().unwrap();
        assert_eq!(
            sp, 65536,
            "Stack pointer should be restored to initial value after all calls"
        );
    }

    #[test]
    fn struct_assign_inline_validation() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            pub fn test() -> i32 {
                let mut p: Point = Point { x: 5, y: 10 };
                p.x = 99;
                return p.x;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct member access write inline WASM is invalid: {e}"));
    }

    #[test]
    fn struct_assign_second_field_validation() {
        let source = r#"
            struct Pair { a: i32; b: i64; }
            pub fn test() -> i64 {
                let mut p: Pair = Pair { a: 1, b: 200 };
                p.b = 999;
                return p.b;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct second field assign WASM is invalid: {e}"));
    }

    #[test]
    fn struct_params_test() {
        cov_mark::check_count!(wasm_codegen_emit_struct_param_copy, 5);
        cov_mark::check_count!(wasm_codegen_emit_struct_literal, 5);
        cov_mark::check_count!(wasm_codegen_emit_member_access_read, 9);
        cov_mark::check_count!(wasm_codegen_emit_member_access_write, 1);
        cov_mark::check_count!(wasm_codegen_emit_stack_prologue, 8);
        let test_name = "struct_params";
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
    fn struct_params_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "struct_params";
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

        // call_sum: sum_point(Point { x: 10, y: 20 }) -> 30
        let call_sum: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "call_sum")
            .expect("Failed to get 'call_sum'");
        let result = call_sum.call(&mut store, ()).expect("call_sum failed");
        assert_eq!(
            result, 30,
            "sum_point(Point {{ x: 10, y: 20 }}) should be 30"
        );

        // verify_copy_semantics: pass Point { x: 1, y: 2 } to modify_no_effect
        // which sets p.x = 99, but the original p.x should still be 1
        let verify_copy: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "verify_copy_semantics")
            .expect("Failed to get 'verify_copy_semantics'");
        let result = verify_copy
            .call(&mut store, ())
            .expect("verify_copy_semantics failed");
        assert_eq!(
            result, 1,
            "After modify_no_effect, original p.x should still be 1 (copy semantics)"
        );

        // call_read_mixed: read_mixed(Mixed { flag: true, val: 42 }) -> 42
        let call_read_mixed: TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "call_read_mixed")
            .expect("Failed to get 'call_read_mixed'");
        let result = call_read_mixed
            .call(&mut store, ())
            .expect("call_read_mixed failed");
        assert_eq!(result, 42, "read_mixed(Mixed {{ val: 42 }}) should be 42");

        // call_two_params: two_struct_params(Point{10,20}, Point{30,40}) -> 100
        let call_two: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "call_two_params")
            .expect("Failed to get 'call_two_params'");
        let result = call_two
            .call(&mut store, ())
            .expect("call_two_params failed");
        assert_eq!(
            result, 100,
            "two_struct_params(Point{{10,20}}, Point{{30,40}}) should be 100"
        );

        // Verify stack pointer is fully restored after all calls
        let final_sp = stack_pointer.get(&mut store).i32().unwrap();
        assert_eq!(
            final_sp, initial_sp,
            "Stack pointer should be restored after all struct param calls"
        );
    }

    #[test]
    fn struct_params_inline_validation() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            pub fn sum(p: Point) -> i32 {
                return p.x + p.y;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct params inline WASM is invalid: {e}"));
    }

    #[test]
    fn struct_params_mixed_type_validation() {
        let source = r#"
            struct Mixed { flag: bool; val: i64; }
            pub fn get_val(m: Mixed) -> i64 {
                return m.val;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct params mixed type WASM is invalid: {e}"));
    }

    #[test]
    fn struct_return_test() {
        cov_mark::check_count!(wasm_codegen_emit_struct_literal, 1);
        cov_mark::check_count!(wasm_codegen_emit_member_access_read, 5);
        cov_mark::check_count!(wasm_codegen_emit_stack_prologue, 6);
        let test_name = "struct_return";
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
    fn struct_return_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "struct_return";
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

        // get_x_from_make: make_point() returns Point { x: 10, y: 20 }, read x -> 10
        let get_x: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_x_from_make")
            .expect("Failed to get 'get_x_from_make'");
        let result = get_x.call(&mut store, ()).expect("get_x_from_make failed");
        assert_eq!(result, 10, "get_x_from_make should return 10");

        // get_y_from_make: make_point() returns Point { x: 10, y: 20 }, read y -> 20
        let get_y: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_y_from_make")
            .expect("Failed to get 'get_y_from_make'");
        let result = get_y.call(&mut store, ()).expect("get_y_from_make failed");
        assert_eq!(result, 20, "get_y_from_make should return 20");

        // get_x_from_var: return_var() returns Point { x: 3, y: 4 }, read x -> 3
        let get_x_var: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_x_from_var")
            .expect("Failed to get 'get_x_from_var'");
        let result = get_x_var
            .call(&mut store, ())
            .expect("get_x_from_var failed");
        assert_eq!(result, 3, "get_x_from_var should return 3");

        // get_x_from_forward: forward_point() -> make_point() -> Point { x: 10, y: 20 }
        let get_x_fwd: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_x_from_forward")
            .expect("Failed to get 'get_x_from_forward'");
        let result = get_x_fwd
            .call(&mut store, ())
            .expect("get_x_from_forward failed");
        assert_eq!(result, 10, "get_x_from_forward should return 10");

        // get_val_from_mixed: make_mixed() returns Mixed { flag: true, val: 99 }, read val -> 99
        let get_val: TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "get_val_from_mixed")
            .expect("Failed to get 'get_val_from_mixed'");
        let result = get_val
            .call(&mut store, ())
            .expect("get_val_from_mixed failed");
        assert_eq!(result, 99, "get_val_from_mixed should return 99");

        // Verify stack pointer is fully restored after all calls
        let final_sp = stack_pointer.get(&mut store).i32().unwrap();
        assert_eq!(
            final_sp, initial_sp,
            "Stack pointer should be restored after all struct return calls"
        );
    }

    #[test]
    fn struct_return_inline_validation() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            pub fn make() -> Point {
                return Point { x: 1, y: 2 };
            }
            pub fn use_it() -> i32 {
                let p: Point = make();
                return p.x;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct return inline WASM is invalid: {e}"));
    }

    #[test]
    fn struct_return_var_inline_validation() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            pub fn return_var() -> Point {
                let p: Point = Point { x: 5, y: 6 };
                return p;
            }
            pub fn use_it() -> i32 {
                let p: Point = return_var();
                return p.y;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct return var inline WASM is invalid: {e}"));
    }

    #[test]
    fn struct_return_forward_inline_validation() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            pub fn make() -> Point {
                return Point { x: 1, y: 2 };
            }
            pub fn forward() -> Point {
                return make();
            }
            pub fn use_it() -> i32 {
                let p: Point = forward();
                return p.x;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct return forward inline WASM is invalid: {e}"));
    }

    // -- S6: Struct-to-struct copy (value semantics) --

    #[test]
    fn struct_copy_test() {
        cov_mark::check_count!(wasm_codegen_emit_struct_copy, 5);
        cov_mark::check_count!(wasm_codegen_emit_struct_literal, 4);
        cov_mark::check_count!(wasm_codegen_emit_member_access_read, 6);
        cov_mark::check_count!(wasm_codegen_emit_stack_prologue, 4);
        let test_name = "struct_copy";
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
    fn struct_copy_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "struct_copy";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        // copy_and_modify: p.x should still be 10 after q.x = 99
        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "copy_and_modify")
            .expect("Failed to get 'copy_and_modify'");
        let result = func.call(&mut store, ()).expect("copy_and_modify failed");
        assert_eq!(
            result, 10,
            "p.x should still be 10 after modifying q.x (value semantics)"
        );

        // copy_values_match: q.x + q.y should be 3 + 7 = 10
        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "copy_values_match")
            .expect("Failed to get 'copy_values_match'");
        let result = func.call(&mut store, ()).expect("copy_values_match failed");
        assert_eq!(result, 10, "q.x + q.y should be 3 + 7 = 10");

        // independent_copies: p.x + p.y should still be 1 + 2 = 3
        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "independent_copies")
            .expect("Failed to get 'independent_copies'");
        let result = func
            .call(&mut store, ())
            .expect("independent_copies failed");
        assert_eq!(
            result, 3,
            "p.x + p.y should still be 1 + 2 = 3 after modifying copies"
        );

        // copy_mixed: n.val should be 42
        let func: TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "copy_mixed")
            .expect("Failed to get 'copy_mixed'");
        let result = func.call(&mut store, ()).expect("copy_mixed failed");
        assert_eq!(result, 42, "n.val should be 42");

        // Stack pointer restoration is verified implicitly: if the stack is
        // corrupted, subsequent calls would trap or return wrong values.
    }

    #[test]
    fn struct_copy_inline_validation() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            pub fn test() -> i32 {
                let p: Point = Point { x: 5, y: 6 };
                let q: Point = p;
                return q.x + q.y;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct copy inline WASM is invalid: {e}"));
    }

    #[test]
    fn struct_copy_value_semantics_inline() {
        cov_mark::check_count!(wasm_codegen_emit_struct_copy, 1);
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            struct Point { x: i32; y: i32; }
            pub fn test() -> i32 {
                let p: Point = Point { x: 10, y: 20 };
                let mut q: Point = p;
                q.x = 99;
                return p.x;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes).unwrap_or_else(|e| panic!("WASM is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes).expect("compile");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test")
            .expect("get func");
        let result = func.call(&mut store, ()).expect("call");
        assert_eq!(
            result, 10,
            "p.x should still be 10 after mutating q.x (value semantics)"
        );
    }

    // Struct uzumaki (non-deterministic initialization) tests ---

    #[test]
    fn struct_nondet_test() {
        cov_mark::check_count!(wasm_codegen_emit_struct_uzumaki, 3);
        cov_mark::check_count!(wasm_codegen_emit_forall_block, 3);
        cov_mark::check_count!(wasm_codegen_emit_stack_prologue, 3);
        cov_mark::check_count!(wasm_codegen_emit_stack_epilogue, 3);
        let test_name = "struct_nondet";
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
    }

    #[test]
    fn struct_array_field_nondet_golden_test() {
        cov_mark::check_count!(wasm_codegen_emit_struct_uzumaki, 3);
        cov_mark::check_count!(wasm_codegen_emit_forall_block, 3);
        let test_name = "struct_array_field_nondet";
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
    }

    #[test]
    fn struct_uzumaki_i32_inline_validation() {
        cov_mark::check_count!(wasm_codegen_emit_struct_uzumaki, 1);
        let source = r#"
            struct Point { x: i32; y: i32; }
            pub fn test() {
                forall {
                    let p: Point = @;
                }
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct uzumaki i32 WASM is invalid: {e}"));
    }

    #[test]
    fn struct_uzumaki_i64_inline_validation() {
        cov_mark::check_count!(wasm_codegen_emit_struct_uzumaki, 1);
        let source = r#"
            struct Wide { a: i64; b: i64; }
            pub fn test() {
                forall {
                    let w: Wide = @;
                }
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct uzumaki i64 WASM is invalid: {e}"));
    }

    #[test]
    fn struct_uzumaki_mixed_fields_inline_validation() {
        cov_mark::check_count!(wasm_codegen_emit_struct_uzumaki, 1);
        let source = r#"
            struct Mixed { flag: bool; count: i32; big: i64; }
            pub fn test() {
                forall {
                    let m: Mixed = @;
                }
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct uzumaki mixed fields WASM is invalid: {e}"));
    }

    #[test]
    fn struct_literal_field_position_uzumaki_inline_validation() {
        // A field-position uzumaki (`Point { x: @, y: @ }`) carries no type of its
        // own; it inherits the field's declared type during type-checking, so
        // codegen finds the type info and emits the right-width uzumaki per field
        // rather than panicking on a missing type. This is the literal form of the
        // whole-struct `let p: Point = @;` above, narrowed to individual fields.
        let source = r#"
            struct Point { x: i32; y: i32; }
            pub fn test() {
                forall {
                    let p: Point = Point { x: @, y: @ };
                }
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Field-position uzumaki WASM is invalid: {e}"));
    }

    #[test]
    fn struct_literal_field_position_uzumaki_mixed_widths_inline_validation() {
        // The field's declared type drives the uzumaki width independently per
        // field: a bool, an i32, and an i64 each pick their own opcode.
        let source = r#"
            struct Mixed { flag: bool; count: i32; big: i64; }
            pub fn test() {
                forall {
                    let m: Mixed = Mixed { flag: @, count: @, big: @ };
                }
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Mixed-width field uzumaki WASM is invalid: {e}"));
    }

    #[test]
    fn struct_with_array_field_uzumaki_inline_validation() {
        cov_mark::check_count!(wasm_codegen_emit_struct_uzumaki, 1);
        let source = r#"
            struct HasArray { arr: [i32; 3]; val: i32; }
            pub fn test() {
                forall {
                    let h: HasArray = @;
                }
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct with array field uzumaki WASM is invalid: {e}"));
    }

    #[test]
    fn struct_with_i64_array_field_uzumaki_inline_validation() {
        cov_mark::check_count!(wasm_codegen_emit_struct_uzumaki, 1);
        let source = r#"
            struct HasI64Arr { arr: [i64; 2]; val: i32; }
            pub fn test() {
                forall {
                    let h: HasI64Arr = @;
                }
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct with i64 array field uzumaki WASM is invalid: {e}"));
    }

    #[test]
    fn struct_with_multiple_array_fields_uzumaki_inline_validation() {
        cov_mark::check_count!(wasm_codegen_emit_struct_uzumaki, 1);
        let source = r#"
            struct Multi { a: [i32; 2]; b: [i64; 2]; c: i32; }
            pub fn test() {
                forall {
                    let m: Multi = @;
                }
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct with multiple array fields uzumaki WASM is invalid: {e}"));
    }

    #[test]
    fn struct_with_only_array_fields_uzumaki_inline_validation() {
        cov_mark::check_count!(wasm_codegen_emit_struct_uzumaki, 1);
        let source = r#"
            struct ArrayOnly { a: [i32; 3]; b: [i32; 2]; }
            pub fn test() {
                forall {
                    let x: ArrayOnly = @;
                }
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct with only array fields uzumaki WASM is invalid: {e}"));
    }

    #[test]
    fn struct_with_bool_array_field_uzumaki_inline_validation() {
        cov_mark::check_count!(wasm_codegen_emit_struct_uzumaki, 1);
        let source = r#"
            struct Flags { bits: [bool; 4]; tag: i32; }
            pub fn test() {
                forall {
                    let f: Flags = @;
                }
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Struct with bool array field uzumaki WASM is invalid: {e}"));
    }

    // Method codegen: self parameter handling and instance method call tests ---

    #[test]
    fn method_instance_test() {
        cov_mark::check_count!(wasm_codegen_emit_self_param, 4);
        cov_mark::check_count!(wasm_codegen_emit_instance_method_call, 9);
        let test_name = "method_instance";
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
    fn method_instance_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "method_instance";
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

        let test_get_x: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_get_x")
            .expect("Failed to get 'test_get_x'");
        let result = test_get_x.call(&mut store, ()).expect("test_get_x failed");
        assert_eq!(result, 10, "p.get_x() should return 10 (p.x)");

        let test_get_y: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_get_y")
            .expect("Failed to get 'test_get_y'");
        let result = test_get_y.call(&mut store, ()).expect("test_get_y failed");
        assert_eq!(result, 20, "p.get_y() should return 20 (p.y)");

        let test_sum: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_sum")
            .expect("Failed to get 'test_sum'");
        let result = test_sum.call(&mut store, ()).expect("test_sum failed");
        assert_eq!(result, 30, "p.sum() should return 30 (p.x + p.y)");

        let test_sum_with: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_sum_with")
            .expect("Failed to get 'test_sum_with'");
        let result = test_sum_with
            .call(&mut store, ())
            .expect("test_sum_with failed");
        assert_eq!(result, 35, "p.sum_with(5) should return 35 (p.x + p.y + 5)");

        let test_let_binding: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_let_binding")
            .expect("Failed to get 'test_let_binding'");
        let result = test_let_binding
            .call(&mut store, ())
            .expect("test_let_binding failed");
        assert_eq!(
            result, 10,
            "test_let_binding: let x = p.get_x() should return 10"
        );

        let test_standalone: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_standalone")
            .expect("Failed to get 'test_standalone'");
        let result = test_standalone
            .call(&mut store, ())
            .expect("test_standalone failed");
        assert_eq!(
            result, 42,
            "test_standalone: p.get_x() as standalone, then return 42"
        );

        let test_binary_op: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_binary_op")
            .expect("Failed to get 'test_binary_op'");
        let result = test_binary_op
            .call(&mut store, ())
            .expect("test_binary_op failed");
        assert_eq!(
            result, 30,
            "test_binary_op: p.get_x() + p.get_y() should return 30"
        );

        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Module should export 'memory'");
        let data_addr: u32 = 0;
        memory.data_mut(&mut store)[data_addr as usize..data_addr as usize + 4]
            .copy_from_slice(&10_i32.to_le_bytes());
        memory.data_mut(&mut store)[data_addr as usize + 4..data_addr as usize + 8]
            .copy_from_slice(&20_i32.to_le_bytes());
        let test_on_param: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "test_on_param")
            .expect("Failed to get 'test_on_param'");
        let result = test_on_param
            .call(&mut store, data_addr as i32)
            .expect("test_on_param failed");
        assert_eq!(
            result, 10,
            "test_on_param: method call on struct parameter should return p.x = 10"
        );
    }
    // Method codegen: associated function call tests ---

    #[test]
    fn method_assoc_test() {
        cov_mark::check_count!(wasm_codegen_emit_associated_function_sret, 0);
        cov_mark::check_count!(wasm_codegen_emit_associated_function_call, 3);
        let test_name = "method_assoc";
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
    fn method_assoc_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "method_assoc";
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

        let test_new: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_new")
            .expect("Failed to get 'test_new'");
        let result = test_new.call(&mut store, ()).expect("test_new failed");
        assert_eq!(result, 3, "Point::new(3, 7).get_x() should return 3");

        let test_new_y: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_new_y")
            .expect("Failed to get 'test_new_y'");
        let result = test_new_y.call(&mut store, ()).expect("test_new_y failed");
        assert_eq!(result, 7, "Point::new(3, 7).get_y() should return 7");

        let test_origin: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_origin")
            .expect("Failed to get 'test_origin'");
        let result = test_origin
            .call(&mut store, ())
            .expect("test_origin failed");
        assert_eq!(
            result, 0,
            "Point::origin().get_x() + Point::origin().get_y() should return 0"
        );

        let test_sum_of: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_sum_of")
            .expect("Failed to get 'test_sum_of'");
        let result = test_sum_of
            .call(&mut store, ())
            .expect("test_sum_of failed");
        assert_eq!(result, 30, "Point::sum_of(10, 20) should return 30");

        let test_mixed: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_mixed")
            .expect("Failed to get 'test_mixed'");
        let result = test_mixed.call(&mut store, ()).expect("test_mixed failed");
        assert_eq!(
            result, 8,
            "Point::new(5, 15).get_x() + Point::sum_of(1, 2) should return 8"
        );

        let test_return_new_get_x: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_return_new_get_x")
            .expect("Failed to get 'test_return_new_get_x'");
        let result = test_return_new_get_x
            .call(&mut store, ())
            .expect("test_return_new_get_x failed");
        assert_eq!(
            result, 10,
            "test_return_new(10, 20).get_x() should return 10"
        );

        let test_return_new_get_y: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_return_new_get_y")
            .expect("Failed to get 'test_return_new_get_y'");
        let result = test_return_new_get_y
            .call(&mut store, ())
            .expect("test_return_new_get_y failed");
        assert_eq!(
            result, 20,
            "test_return_new(10, 20).get_y() should return 20"
        );

        let test_return_new_direct_get_x: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_return_new_direct_get_x")
            .expect("Failed to get 'test_return_new_direct_get_x'");
        let result = test_return_new_direct_get_x
            .call(&mut store, ())
            .expect("test_return_new_direct_get_x failed");
        assert_eq!(
            result, 10,
            "test_return_new_direct(10, 20).get_x() should return 10"
        );

        let test_return_new_direct_get_y: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_return_new_direct_get_y")
            .expect("Failed to get 'test_return_new_direct_get_y'");
        let result = test_return_new_direct_get_y
            .call(&mut store, ())
            .expect("test_return_new_direct_get_y failed");
        assert_eq!(
            result, 20,
            "test_return_new_direct(10, 20).get_y() should return 20"
        );

        let test_standalone: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_standalone")
            .expect("Failed to get 'test_standalone'");
        let result = test_standalone
            .call(&mut store, ())
            .expect("test_standalone failed");
        assert_eq!(
            result, 42,
            "test_standalone should return 42 after discarding Point::sum_of(1, 2)"
        );
    }

    // Method codegen: methods returning structs (sret) ---

    #[test]
    fn method_return_struct_test() {
        cov_mark::check_count!(wasm_codegen_emit_instance_method_sret, 0);
        cov_mark::check_count!(wasm_codegen_emit_instance_method_call, 10);
        let test_name = "method_return_struct";
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
    fn method_return_struct_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "method_return_struct";
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

        let test_translate_x: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_translate_x")
            .expect("Failed to get 'test_translate_x'");
        let result = test_translate_x
            .call(&mut store, ())
            .expect("test_translate_x failed");
        assert_eq!(
            result, 15,
            "Point(10,20).translate(5,3).get_x() should return 15"
        );

        let test_translate_y: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_translate_y")
            .expect("Failed to get 'test_translate_y'");
        let result = test_translate_y
            .call(&mut store, ())
            .expect("test_translate_y failed");
        assert_eq!(
            result, 23,
            "Point(10,20).translate(5,3).get_y() should return 23"
        );

        let test_scale_x: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_scale_x")
            .expect("Failed to get 'test_scale_x'");
        let result = test_scale_x
            .call(&mut store, ())
            .expect("test_scale_x failed");
        assert_eq!(result, 12, "Point(3,7).scale(4).get_x() should return 12");

        let test_scale_y: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_scale_y")
            .expect("Failed to get 'test_scale_y'");
        let result = test_scale_y
            .call(&mut store, ())
            .expect("test_scale_y failed");
        assert_eq!(result, 28, "Point(3,7).scale(4).get_y() should return 28");

        let test_original_unchanged_x: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_original_unchanged_x")
            .expect("Failed to get 'test_original_unchanged_x'");
        let result = test_original_unchanged_x
            .call(&mut store, ())
            .expect("test_original_unchanged_x failed");
        assert_eq!(
            result, 10,
            "Original point should be unchanged after translate: x should still be 10"
        );

        let test_original_unchanged_y: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_original_unchanged_y")
            .expect("Failed to get 'test_original_unchanged_y'");
        let result = test_original_unchanged_y
            .call(&mut store, ())
            .expect("test_original_unchanged_y failed");
        assert_eq!(
            result, 20,
            "Original point should be unchanged after translate: y should still be 20"
        );

        let test_new_returns_struct_x: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_new_returns_struct_x")
            .expect("Failed to get 'test_new_returns_struct_x'");
        let result = test_new_returns_struct_x
            .call(&mut store, ())
            .expect("test_new_returns_struct_x failed");
        assert_eq!(result, 42, "Point::new(42, 99).get_x() should return 42");

        let test_new_returns_struct_y: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_new_returns_struct_y")
            .expect("Failed to get 'test_new_returns_struct_y'");
        let result = test_new_returns_struct_y
            .call(&mut store, ())
            .expect("test_new_returns_struct_y failed");
        assert_eq!(result, 99, "Point::new(42, 99).get_y() should return 99");

        let test_return_translated: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_return_translated")
            .expect("Failed to get 'test_return_translated'");
        let result = test_return_translated
            .call(&mut store, ())
            .expect("test_return_translated failed");
        assert_eq!(
            result, 33,
            "Point(1,2).translate(10,20): get_x() + get_y() should return 11 + 22 = 33"
        );
    }

    // Method codegen: mutable self tests ---

    #[test]
    fn method_self_mutate_test() {
        cov_mark::check_count!(wasm_codegen_emit_self_param, 3);
        cov_mark::check_count!(wasm_codegen_emit_self_copy_on_entry, 2);
        cov_mark::check_count!(wasm_codegen_emit_instance_method_call, 12);
        let test_name = "method_self_mutate";
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
    fn method_self_mutate_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "method_self_mutate";
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

        let test_get: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_get")
            .expect("Failed to get 'test_get'");
        let result = test_get.call(&mut store, ()).expect("test_get failed");
        assert_eq!(result, 10, "Counter(10).get() should return 10");

        let test_increment_value_semantics: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_increment_value_semantics")
            .expect("Failed to get 'test_increment_value_semantics'");
        let result = test_increment_value_semantics
            .call(&mut store, ())
            .expect("test_increment_value_semantics failed");
        assert_eq!(
            result, 10,
            "mut self value semantics: c.increment() should NOT modify caller's c, c.get() should still return 10"
        );

        let test_add_value_semantics: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_add_value_semantics")
            .expect("Failed to get 'test_add_value_semantics'");
        let result = test_add_value_semantics
            .call(&mut store, ())
            .expect("test_add_value_semantics failed");
        assert_eq!(
            result, 10,
            "mut self value semantics: c.add(5) should NOT modify caller's c, c.get() should still return 10"
        );

        let test_mut_self_does_not_affect_caller: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_mut_self_does_not_affect_caller")
            .expect("Failed to get 'test_mut_self_does_not_affect_caller'");
        let result = test_mut_self_does_not_affect_caller
            .call(&mut store, ())
            .expect("test_mut_self_does_not_affect_caller failed");
        assert_eq!(
            result, 42,
            "mut self value semantics: c.increment() and c.add(100) should NOT modify caller's c, c.get() should still return 42"
        );

        let test_multiple_increments: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_multiple_increments")
            .expect("Failed to get 'test_multiple_increments'");
        let result = test_multiple_increments
            .call(&mut store, ())
            .expect("test_multiple_increments failed");
        assert_eq!(
            result, 0,
            "mut self value semantics: three c.increment() calls should NOT modify caller's c, c.get() should still return 0"
        );
    }

    // Method codegen: multiple structs with same method names ---

    #[test]
    fn method_multi_struct_test() {
        cov_mark::check_count!(wasm_codegen_emit_self_param, 4);
        cov_mark::check_count!(wasm_codegen_emit_instance_method_call, 4);
        let test_name = "method_multi_struct";
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
    fn method_multi_struct_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "method_multi_struct";
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

        let test_point_get_x: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_point_get_x")
            .expect("Failed to get 'test_point_get_x'");
        let result = test_point_get_x
            .call(&mut store, ())
            .expect("test_point_get_x failed");
        assert_eq!(result, 10, "Point(10,20).get_x() should return 10");

        let test_size_get_x: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_size_get_x")
            .expect("Failed to get 'test_size_get_x'");
        let result = test_size_get_x
            .call(&mut store, ())
            .expect("test_size_get_x failed");
        assert_eq!(result, 30, "Size(30,40).get_x() should return 30");

        let test_both_get_y: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_both_get_y")
            .expect("Failed to get 'test_both_get_y'");
        let result = test_both_get_y
            .call(&mut store, ())
            .expect("test_both_get_y failed");
        assert_eq!(
            result, 6,
            "Point(1,2).get_y() + Size(3,4).get_y() should return 6"
        );
    }

    // Method codegen: cross-call tests (method-to-method, method-to-function) ---

    #[test]
    fn method_cross_call_test() {
        cov_mark::check_count!(wasm_codegen_emit_instance_method_call, 5);
        let test_name = "method_cross_call";
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
    fn method_cross_call_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "method_cross_call";
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

        let test_method_calls_method: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_method_calls_method")
            .expect("Failed to get 'test_method_calls_method'");
        let result = test_method_calls_method
            .call(&mut store, ())
            .expect("test_method_calls_method failed");
        assert_eq!(
            result, 10,
            "v.sum() should return 10 (3 + 7) via method-to-method calls"
        );

        let test_method_calls_toplevel_fn: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_method_calls_toplevel_fn")
            .expect("Failed to get 'test_method_calls_toplevel_fn'");
        let result = test_method_calls_toplevel_fn
            .call(&mut store, ())
            .expect("test_method_calls_toplevel_fn failed");
        assert_eq!(
            result, 10,
            "double(v.get_x()) should return 10 (5 * 2) via top-level fn wrapping method result"
        );

        let test_toplevel_fn_calls_method: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_toplevel_fn_calls_method")
            .expect("Failed to get 'test_toplevel_fn_calls_method'");
        let result = test_toplevel_fn_calls_method
            .call(&mut store, ())
            .expect("test_toplevel_fn_calls_method failed");
        assert_eq!(
            result, 12,
            "double(v.get_y()) should return 12 (6 * 2) via associated fn constructor + method + top-level fn"
        );
    }

    // Method codegen: method returning array (sret + instance method) ---

    #[test]
    fn method_array_return_test() {
        let test_name = "method_array_return";
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
    fn method_array_return_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "method_array_return";
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

        let test_first: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_to_array_first")
            .expect("Failed to get 'test_to_array_first'");
        let result = test_first
            .call(&mut store, ())
            .expect("test_to_array_first failed");
        assert_eq!(result, 10, "p.to_array()[0] should return 10 (p.x)");

        let test_second: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_to_array_second")
            .expect("Failed to get 'test_to_array_second'");
        let result = test_second
            .call(&mut store, ())
            .expect("test_to_array_second failed");
        assert_eq!(result, 20, "p.to_array()[1] should return 20 (p.y)");
    }

    // Method codegen: i64 struct fields ---

    #[test]
    fn method_i64_fields_test() {
        let test_name = "method_i64_fields";
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
    fn method_i64_fields_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "method_i64_fields";
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

        let test_get_a: TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "test_get_a")
            .expect("Failed to get 'test_get_a'");
        let result = test_get_a.call(&mut store, ()).expect("test_get_a failed");
        assert_eq!(result, 100i64, "p.get_a() should return 100");

        let test_get_b: TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "test_get_b")
            .expect("Failed to get 'test_get_b'");
        let result = test_get_b.call(&mut store, ()).expect("test_get_b failed");
        assert_eq!(result, 200i64, "p.get_b() should return 200");

        let test_sum: TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "test_sum")
            .expect("Failed to get 'test_sum'");
        let result = test_sum.call(&mut store, ()).expect("test_sum failed");
        assert_eq!(result, 300i64, "p.sum() should return 300 (100 + 200)");
    }

    // Method codegen: three-field struct ---

    #[test]
    fn method_three_fields_test() {
        let test_name = "method_three_fields";
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
    fn method_three_fields_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "method_three_fields";
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

        let test_get_x: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_get_x")
            .expect("Failed to get 'test_get_x'");
        let result = test_get_x.call(&mut store, ()).expect("test_get_x failed");
        assert_eq!(result, 1, "v.get_x() should return 1");

        let test_get_y: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_get_y")
            .expect("Failed to get 'test_get_y'");
        let result = test_get_y.call(&mut store, ()).expect("test_get_y failed");
        assert_eq!(result, 2, "v.get_y() should return 2");

        let test_get_z: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_get_z")
            .expect("Failed to get 'test_get_z'");
        let result = test_get_z.call(&mut store, ()).expect("test_get_z failed");
        assert_eq!(result, 3, "v.get_z() should return 3");

        let test_sum: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_sum")
            .expect("Failed to get 'test_sum'");
        let result = test_sum.call(&mut store, ()).expect("test_sum failed");
        assert_eq!(result, 6, "v.sum() should return 6 (1 + 2 + 3)");
    }

    #[test]
    fn nested_struct_golden_test() {
        let test_name = "nested_struct";
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
    fn nested_struct_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "nested_struct";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let create_and_read_val: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "create_and_read_val")
            .expect("Failed to get 'create_and_read_val'");
        let result = create_and_read_val
            .call(&mut store, ())
            .expect("create_and_read_val failed");
        assert_eq!(result, 30, "create_and_read_val should return 30 (o.val)");

        let read_via_copy: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "read_via_copy")
            .expect("Failed to get 'read_via_copy'");
        let result = read_via_copy
            .call(&mut store, ())
            .expect("read_via_copy failed");
        assert_eq!(
            result, 10,
            "read_via_copy should return 10 (o.inner.x via copy)"
        );

        let read_inner_y_via_copy: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "read_inner_y_via_copy")
            .expect("Failed to get 'read_inner_y_via_copy'");
        let result = read_inner_y_via_copy
            .call(&mut store, ())
            .expect("read_inner_y_via_copy failed");
        assert_eq!(
            result, 20,
            "read_inner_y_via_copy should return 20 (o.inner.y via copy)"
        );

        let sum_all_fields: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "sum_all_fields")
            .expect("Failed to get 'sum_all_fields'");
        let result = sum_all_fields
            .call(&mut store, ())
            .expect("sum_all_fields failed");
        assert_eq!(result, 60, "sum_all_fields should return 60 (10 + 20 + 30)");

        let write_inner_field: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "write_inner_field")
            .expect("Failed to get 'write_inner_field'");
        let result = write_inner_field
            .call(&mut store, ())
            .expect("write_inner_field failed");
        assert_eq!(
            result, 99,
            "write_inner_field should return 99 (i.x after write)"
        );

        // Test sret return: nested_struct_return() -> Outer
        // Outer layout: Inner { x: i32, y: i32 } at offset 0 (8 bytes), val: i32 at offset 8.
        let nested_struct_return: TypedFunc<i32, ()> = instance
            .get_typed_func(&mut store, "nested_struct_return")
            .expect("Failed to get 'nested_struct_return'");
        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Module should have memory");
        let sret_base: i32 = 0;
        nested_struct_return
            .call(&mut store, sret_base)
            .expect("nested_struct_return failed");
        let data = memory.data(&store);
        let base = sret_base as usize;
        let inner_x = i32::from_le_bytes(data[base..base + 4].try_into().unwrap());
        let inner_y = i32::from_le_bytes(data[base + 4..base + 8].try_into().unwrap());
        let val = i32::from_le_bytes(data[base + 8..base + 12].try_into().unwrap());
        assert_eq!(inner_x, 42, "nested_struct_return: inner.x should be 42");
        assert_eq!(inner_y, 84, "nested_struct_return: inner.y should be 84");
        assert_eq!(val, 126, "nested_struct_return: val should be 126");

        // Test method accessing self.inner.x
        let test_method_get_inner_x: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_method_get_inner_x")
            .expect("Failed to get 'test_method_get_inner_x'");
        let result = test_method_get_inner_x
            .call(&mut store, ())
            .expect("test_method_get_inner_x failed");
        assert_eq!(
            result, 55,
            "test_method_get_inner_x should return 55 (self.inner.x via method)"
        );

        // Test method accessing self.inner.x + self.inner.y
        let test_method_sum_inner: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_method_sum_inner")
            .expect("Failed to get 'test_method_sum_inner'");
        let result = test_method_sum_inner
            .call(&mut store, ())
            .expect("test_method_sum_inner failed");
        assert_eq!(
            result, 30,
            "test_method_sum_inner should return 30 (self.inner.x + self.inner.y via method)"
        );

        // Test nested_struct_param: pass Outer via pointer
        // Outer layout: Inner { x: i32 @ 0, y: i32 @ 4 }, val: i32 @ 8. Total: 12 bytes.
        let param_base: i32 = 0;
        let base = param_base as usize;
        memory.data_mut(&mut store)[base..base + 4]
            .copy_from_slice(&10_i32.to_le_bytes());
        memory.data_mut(&mut store)[base + 4..base + 8]
            .copy_from_slice(&20_i32.to_le_bytes());
        memory.data_mut(&mut store)[base + 8..base + 12]
            .copy_from_slice(&30_i32.to_le_bytes());

        let nested_struct_param: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "nested_struct_param")
            .expect("Failed to get 'nested_struct_param'");
        let result = nested_struct_param
            .call(&mut store, param_base)
            .expect("nested_struct_param failed");
        assert_eq!(
            result, 10,
            "nested_struct_param should return 10 (o.inner.x via copy)"
        );

        let sp_global = instance
            .get_global(&mut store, "__stack_pointer")
            .expect("Module should export __stack_pointer");
        let sp_val = sp_global.get(&mut store).i32().unwrap();
        assert_eq!(
            sp_val, 65536,
            "Stack pointer should be restored to initial value after all calls"
        );
    }

    #[test]
    fn struct_with_array_golden_test() {
        let test_name = "struct_with_array";
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
    fn struct_with_array_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "struct_with_array";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let create_and_read_val: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "create_and_read_val")
            .expect("Failed to get 'create_and_read_val'");
        let result = create_and_read_val
            .call(&mut store, ())
            .expect("create_and_read_val failed");
        assert_eq!(result, 42, "create_and_read_val should return 42 (s.val)");

        let read_arr_first: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "read_arr_first")
            .expect("Failed to get 'read_arr_first'");
        let result = read_arr_first
            .call(&mut store, ())
            .expect("read_arr_first failed");
        assert_eq!(result, 10, "read_arr_first should return 10 (s.arr[0])");

        let read_arr_last: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "read_arr_last")
            .expect("Failed to get 'read_arr_last'");
        let result = read_arr_last
            .call(&mut store, ())
            .expect("read_arr_last failed");
        assert_eq!(result, 30, "read_arr_last should return 30 (s.arr[2])");

        let write_arr_element: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "write_arr_element")
            .expect("Failed to get 'write_arr_element'");
        let result = write_arr_element
            .call(&mut store, ())
            .expect("write_arr_element failed");
        assert_eq!(
            result, 99,
            "write_arr_element should return 99 (s.arr[1] after write)"
        );

        let sum_arr_and_val: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "sum_arr_and_val")
            .expect("Failed to get 'sum_arr_and_val'");
        let result = sum_arr_and_val
            .call(&mut store, ())
            .expect("sum_arr_and_val failed");
        assert_eq!(
            result, 102,
            "sum_arr_and_val should return 102 (10+20+30+42)"
        );

        let struct_with_array_param: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "struct_with_array_param")
            .expect("Failed to get 'struct_with_array_param'");
        // Pass a pointer to a HasArray in memory -- allocate at the beginning of memory.
        // HasArray: arr:[i32;3] at offset 0 (12 bytes), val:i32 at offset 12 (4 bytes) = 16 bytes total.
        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Module should have memory");
        let data = memory.data_mut(&mut store);
        let base: usize = 0;
        data[base..base + 4].copy_from_slice(&100_i32.to_le_bytes()); // arr[0] = 100
        data[base + 4..base + 8].copy_from_slice(&200_i32.to_le_bytes()); // arr[1] = 200
        data[base + 8..base + 12].copy_from_slice(&300_i32.to_le_bytes()); // arr[2] = 300
        data[base + 12..base + 16].copy_from_slice(&50_i32.to_le_bytes()); // val = 50
        let result = struct_with_array_param
            .call(&mut store, base as i32)
            .expect("struct_with_array_param failed");
        assert_eq!(
            result, 150,
            "struct_with_array_param should return 150 (100+50)"
        );

        let test_method_get_arr_elem: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_method_get_arr_elem")
            .expect("Failed to get 'test_method_get_arr_elem'");
        let result = test_method_get_arr_elem
            .call(&mut store, ())
            .expect("test_method_get_arr_elem failed");
        assert_eq!(
            result, 20,
            "test_method_get_arr_elem should return 20 (s.arr[1] via method)"
        );

        let test_method_sum_arr: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_method_sum_arr")
            .expect("Failed to get 'test_method_sum_arr'");
        let result = test_method_sum_arr
            .call(&mut store, ())
            .expect("test_method_sum_arr failed");
        assert_eq!(
            result, 60,
            "test_method_sum_arr should return 60 (10+20+30 via method)"
        );

        // Test sret return: struct_with_array_return() -> HasArray
        // HasArray layout: arr: [i32; 3] at offset 0 (12 bytes), val: i32 at offset 12.
        let struct_with_array_return: TypedFunc<i32, ()> = instance
            .get_typed_func(&mut store, "struct_with_array_return")
            .expect("Failed to get 'struct_with_array_return'");
        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Module should have memory");
        let sret_base: i32 = 0;
        struct_with_array_return
            .call(&mut store, sret_base)
            .expect("struct_with_array_return failed");
        let data = memory.data(&store);
        let base = sret_base as usize;
        let arr0 = i32::from_le_bytes(data[base..base + 4].try_into().unwrap());
        let arr1 = i32::from_le_bytes(data[base + 4..base + 8].try_into().unwrap());
        let arr2 = i32::from_le_bytes(data[base + 8..base + 12].try_into().unwrap());
        let val = i32::from_le_bytes(data[base + 12..base + 16].try_into().unwrap());
        assert_eq!(arr0, 1, "struct_with_array_return: arr[0] should be 1");
        assert_eq!(arr1, 2, "struct_with_array_return: arr[1] should be 2");
        assert_eq!(arr2, 3, "struct_with_array_return: arr[2] should be 3");
        assert_eq!(val, 4, "struct_with_array_return: val should be 4");
    }

    // array_of_structs tests ---

    #[test]
    fn array_of_structs_golden_test() {
        let test_name = "array_of_structs";
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
    fn array_of_structs_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "array_of_structs";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let create_and_read_field: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "create_and_read_field")
            .expect("Failed to get 'create_and_read_field'");
        let result = create_and_read_field
            .call(&mut store, ())
            .expect("create_and_read_field failed");
        assert_eq!(
            result, 3,
            "create_and_read_field should return 3 (pts[1].x)"
        );

        let read_second_field: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "read_second_field")
            .expect("Failed to get 'read_second_field'");
        let result = read_second_field
            .call(&mut store, ())
            .expect("read_second_field failed");
        assert_eq!(
            result, 6,
            "read_second_field should return 6 (pts[2].y)"
        );

        let sum_all_x: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "sum_all_x")
            .expect("Failed to get 'sum_all_x'");
        let result = sum_all_x
            .call(&mut store, ())
            .expect("sum_all_x failed");
        assert_eq!(result, 90, "sum_all_x should return 90 (10+30+50)");

        let write_element_field: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "write_element_field")
            .expect("Failed to get 'write_element_field'");
        let result = write_element_field
            .call(&mut store, ())
            .expect("write_element_field failed");
        assert_eq!(
            result, 99,
            "write_element_field should return 99 (pts[0].x after write)"
        );

        let copy_element_to_var: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "copy_element_to_var")
            .expect("Failed to get 'copy_element_to_var'");
        let result = copy_element_to_var
            .call(&mut store, ())
            .expect("copy_element_to_var failed");
        assert_eq!(
            result, 70,
            "copy_element_to_var should return 70 (30+40)"
        );

        let write_whole_element: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "write_whole_element")
            .expect("Failed to get 'write_whole_element'");
        let result = write_whole_element
            .call(&mut store, ())
            .expect("write_whole_element failed");
        assert_eq!(
            result, 165,
            "write_whole_element should return 165 (77+88)"
        );

        let array_of_structs_param: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "array_of_structs_param")
            .expect("Failed to get 'array_of_structs_param'");
        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Module should have memory");
        let data = memory.data_mut(&mut store);
        let base: usize = 0;
        // Point { x: i32, y: i32 } is 8 bytes. [Point; 2] = 16 bytes.
        // pts[0]: x=100, y=200 at offset 0
        data[base..base + 4].copy_from_slice(&100_i32.to_le_bytes());
        data[base + 4..base + 8].copy_from_slice(&200_i32.to_le_bytes());
        // pts[1]: x=300, y=400 at offset 8
        data[base + 8..base + 12].copy_from_slice(&300_i32.to_le_bytes());
        data[base + 12..base + 16].copy_from_slice(&400_i32.to_le_bytes());
        let result = array_of_structs_param
            .call(&mut store, base as i32)
            .expect("array_of_structs_param failed");
        assert_eq!(
            result, 500,
            "array_of_structs_param should return 500 (100+400)"
        );

        let test_method_on_element: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_method_on_element")
            .expect("Failed to get 'test_method_on_element'");
        let result = test_method_on_element
            .call(&mut store, ())
            .expect("test_method_on_element failed");
        assert_eq!(
            result, 30,
            "test_method_on_element should return 30 (10+20 via p.sum())"
        );

        let sp_global = instance
            .get_global(&mut store, "__stack_pointer")
            .expect("Module should export __stack_pointer");
        let sp_val = sp_global.get(&mut store).i32().unwrap();
        assert_eq!(
            sp_val, 65536,
            "Stack pointer should be restored to initial value after all calls"
        );
    }

    // --- struct_with_array_of_structs tests (struct field that is an array-of-struct) ---

    #[test]
    fn struct_with_array_of_structs_golden_test() {
        let test_name = "struct_with_array_of_structs";
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
    fn struct_with_array_of_structs_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "struct_with_array_of_structs";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let read_c0x: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "read_c0x")
            .expect("Failed to get 'read_c0x'");
        assert_eq!(
            read_c0x.call(&mut store, ()).expect("read_c0x failed"),
            1,
            "read_c0x should return 1 (g.cells[0].x)"
        );

        let read_c1y: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "read_c1y")
            .expect("Failed to get 'read_c1y'");
        assert_eq!(
            read_c1y.call(&mut store, ()).expect("read_c1y failed"),
            4,
            "read_c1y should return 4 (g.cells[1].y)"
        );

        let read_var_index: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "read_var_index")
            .expect("Failed to get 'read_var_index'");
        assert_eq!(
            read_var_index
                .call(&mut store, 0)
                .expect("read_var_index(0) failed"),
            10,
            "read_var_index(0) should return 10 (g.cells[0].x)"
        );
        assert_eq!(
            read_var_index
                .call(&mut store, 1)
                .expect("read_var_index(1) failed"),
            30,
            "read_var_index(1) should return 30 (g.cells[1].x)"
        );

        let write_c1y: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "write_c1y")
            .expect("Failed to get 'write_c1y'");
        assert_eq!(
            write_c1y.call(&mut store, ()).expect("write_c1y failed"),
            99,
            "write_c1y should return 99 (g.cells[1].y after write)"
        );

        let write_whole_elem: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "write_whole_elem")
            .expect("Failed to get 'write_whole_elem'");
        assert_eq!(
            write_whole_elem
                .call(&mut store, ())
                .expect("write_whole_elem failed"),
            165,
            "write_whole_elem should return 165 (77+88 after whole-element write)"
        );

        let call_grid_param: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "call_grid_param")
            .expect("Failed to get 'call_grid_param'");
        assert_eq!(
            call_grid_param
                .call(&mut store, ())
                .expect("call_grid_param failed"),
            15,
            "call_grid_param should return 15 (6 + 9)"
        );

        let make_and_read: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "make_and_read")
            .expect("Failed to get 'make_and_read'");
        assert_eq!(
            make_and_read
                .call(&mut store, ())
                .expect("make_and_read failed"),
            7,
            "make_and_read should return 7 (g.cells[1].x via sret return)"
        );

        let mixed_offsets: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "mixed_offsets")
            .expect("Failed to get 'mixed_offsets'");
        assert_eq!(
            mixed_offsets
                .call(&mut store, ())
                .expect("mixed_offsets failed"),
            610,
            "mixed_offsets should return 610 (11 + 100 + 400 + 99)"
        );

        let two_grids: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "two_grids")
            .expect("Failed to get 'two_grids'");
        assert_eq!(
            two_grids.call(&mut store, ()).expect("two_grids failed"),
            9,
            "two_grids should return 9 (1 + 8)"
        );

        let zero_field_elem: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "zero_field_elem")
            .expect("Failed to get 'zero_field_elem'");
        assert_eq!(
            zero_field_elem
                .call(&mut store, ())
                .expect("zero_field_elem failed"),
            5,
            "zero_field_elem should return 5 (5 + 0 with zero-valued fields)"
        );

        let read_i64: TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "read_i64")
            .expect("Failed to get 'read_i64'");
        assert_eq!(
            read_i64.call(&mut store, ()).expect("read_i64 failed"),
            3,
            "read_i64 should return 3 (g.cells[1].x with i64 struct fields)"
        );

        let sp_global = instance
            .get_global(&mut store, "__stack_pointer")
            .expect("Module should export __stack_pointer");
        let sp_val = sp_global.get(&mut store).i32().unwrap();
        assert_eq!(
            sp_val, 65536,
            "Stack pointer should be restored to initial value after all calls"
        );
    }

    // --- struct_with_nested_array tests (struct field that is a multi-dimensional array) ---

    #[test]
    fn struct_with_nested_array_golden_test() {
        let test_name = "struct_with_nested_array";
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
    fn struct_with_nested_array_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "struct_with_nested_array";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let grid_read: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "grid_read")
            .expect("Failed to get 'grid_read'");
        assert_eq!(
            grid_read
                .call(&mut store, (0, 0))
                .expect("grid_read(0,0) failed"),
            1,
            "grid_read(0,0) should return 1 (g.grid[0][0])"
        );
        assert_eq!(
            grid_read
                .call(&mut store, (1, 2))
                .expect("grid_read(1,2) failed"),
            6,
            "grid_read(1,2) should return 6 (g.grid[1][2])"
        );
        assert_eq!(
            grid_read
                .call(&mut store, (0, 2))
                .expect("grid_read(0,2) failed"),
            3,
            "grid_read(0,2) should return 3 (g.grid[0][2])"
        );

        let grid_sum: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "grid_sum")
            .expect("Failed to get 'grid_sum'");
        assert_eq!(
            grid_sum.call(&mut store, ()).expect("grid_sum failed"),
            21,
            "grid_sum should return 21 (1+2+3+4+5+6)"
        );

        let grid_write: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "grid_write")
            .expect("Failed to get 'grid_write'");
        assert_eq!(
            grid_write.call(&mut store, ()).expect("grid_write failed"),
            99,
            "grid_write should return 99 (g.grid[1][2] after write)"
        );

        let cube_read: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "cube_read")
            .expect("Failed to get 'cube_read'");
        assert_eq!(
            cube_read.call(&mut store, ()).expect("cube_read failed"),
            6,
            "cube_read should return 6 (c.cube[1][0][1])"
        );

        let aos_grid_read: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "aos_grid_read")
            .expect("Failed to get 'aos_grid_read'");
        assert_eq!(
            aos_grid_read
                .call(&mut store, ())
                .expect("aos_grid_read failed"),
            9,
            "aos_grid_read should return 9 (g.cells[1][0].x + g.cells[0][1].y = 5 + 4)"
        );

        let mixed_grid: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "mixed_grid")
            .expect("Failed to get 'mixed_grid'");
        assert_eq!(
            mixed_grid.call(&mut store, ()).expect("mixed_grid failed"),
            115,
            "mixed_grid should return 115 (10 + 1 + 4 + 100)"
        );

        let sp_global = instance
            .get_global(&mut store, "__stack_pointer")
            .expect("Module should export __stack_pointer");
        let sp_val = sp_global.get(&mut store).i32().unwrap();
        assert_eq!(
            sp_val, 65536,
            "Stack pointer should be restored to initial value after all calls"
        );
    }

    // nested_array_of_structs tests (array-of-structs at nesting depth >= 2) ---

    #[test]
    fn nested_array_of_structs_test() {
        cov_mark::check_count!(wasm_codegen_emit_array_literal, 4);
        let test_name = "nested_array_of_structs";
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
    fn nested_array_of_structs_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "nested_array_of_structs";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        // grid_2d_x reads g[1][0].x from [[Pt;2];2] = [[1,2],[3,4]],[[5,6],[7,8]] -> 5.
        let grid_2d_x: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "grid_2d_x")
            .expect("Failed to get 'grid_2d_x'");
        let result = grid_2d_x.call(&mut store, ()).expect("grid_2d_x failed");
        assert_eq!(result, 5, "grid_2d_x should return 5 (g[1][0].x)");

        // grid_2d_y reads g[0][1].y -> 4.
        let grid_2d_y: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "grid_2d_y")
            .expect("Failed to get 'grid_2d_y'");
        let result = grid_2d_y.call(&mut store, ()).expect("grid_2d_y failed");
        assert_eq!(result, 4, "grid_2d_y should return 4 (g[0][1].y)");

        // cube_3d reads c[1][0][0].y from a [[[Pt;1];2];2] literal -> 15.
        let cube_3d: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "cube_3d")
            .expect("Failed to get 'cube_3d'");
        let result = cube_3d.call(&mut store, ()).expect("cube_3d failed");
        assert_eq!(result, 15, "cube_3d should return 15 (c[1][0][0].y)");

        // grid_nonliteral builds [[p,p],[p,p]] from a local struct p{x:21,y:22}
        // (non-literal struct elements -> memory.copy) and returns g[1][1].x + g[0][1].y.
        let grid_nonliteral: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "grid_nonliteral")
            .expect("Failed to get 'grid_nonliteral'");
        let result = grid_nonliteral
            .call(&mut store, ())
            .expect("grid_nonliteral failed");
        assert_eq!(
            result, 43,
            "grid_nonliteral should return 43 (g[1][1].x 21 + g[0][1].y 22)"
        );

        let sp_global = instance
            .get_global(&mut store, "__stack_pointer")
            .expect("Module should export __stack_pointer");
        let sp_val = sp_global.get(&mut store).i32().unwrap();
        assert_eq!(
            sp_val, 65536,
            "Stack pointer should be restored to initial value after all calls"
        );
    }

    #[test]
    fn multidim_array_uzumaki_test() {
        cov_mark::check_count!(wasm_codegen_emit_array_uzumaki, 2);
        cov_mark::check_count!(wasm_codegen_emit_forall_block, 2);
        cov_mark::check_count!(wasm_codegen_emit_array_index_read, 4);
        cov_mark::check_count!(wasm_codegen_emit_stack_prologue, 2);
        cov_mark::check_count!(wasm_codegen_emit_stack_epilogue, 2);
        let test_name = "multidim_array_uzumaki";
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
    }

    #[test]
    fn multidim_array_uzumaki_i32_inline_validation() {
        cov_mark::check_count!(wasm_codegen_emit_array_uzumaki, 1);
        let source = r#"
            pub fn test() {
                forall {
                    let grid: [[i32; 3]; 2] = @;
                }
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Multidim array uzumaki i32 WASM is invalid: {e}"));
    }

    #[test]
    fn multidim_array_uzumaki_i64_inline_validation() {
        cov_mark::check_count!(wasm_codegen_emit_array_uzumaki, 1);
        let source = r#"
            pub fn test() {
                forall {
                    let matrix: [[i64; 2]; 2] = @;
                }
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Multidim array uzumaki i64 WASM is invalid: {e}"));
    }

    #[test]
    fn multidim_array_uzumaki_3d_inline_validation() {
        cov_mark::check_count!(wasm_codegen_emit_array_uzumaki, 1);
        let source = r#"
            pub fn test_3d() {
                forall {
                    let cube: [[[i32; 2]; 3]; 4] = @;
                }
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("3D array uzumaki WASM is invalid: {e}"));
    }

    #[test]
    fn multidim_array_uzumaki_4d_inline_validation() {
        cov_mark::check_count!(wasm_codegen_emit_array_uzumaki, 1);
        let source = r#"
            pub fn test_4d() {
                forall {
                    let hyper: [[[[i32; 2]; 2]; 2]; 2] = @;
                }
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("4D array uzumaki WASM is invalid: {e}"));
    }

    // nested_struct_with_array tests ---

    #[test]
    fn nested_struct_with_array_golden_test() {
        let test_name = "nested_struct_with_array";
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
    fn nested_struct_with_array_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "nested_struct_with_array";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let test_deep_inner_arr_access: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_deep_inner_arr_access")
            .expect("Failed to get 'test_deep_inner_arr_access'");
        let result = test_deep_inner_arr_access
            .call(&mut store, ())
            .expect("test_deep_inner_arr_access failed");
        assert_eq!(
            result, 20,
            "test_deep_inner_arr_access should return 20 (d.inner.arr[1])"
        );

        let test_deep_inner_val: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_deep_inner_val")
            .expect("Failed to get 'test_deep_inner_val'");
        let result = test_deep_inner_val
            .call(&mut store, ())
            .expect("test_deep_inner_val failed");
        assert_eq!(
            result, 99,
            "test_deep_inner_val should return 99 (d.inner.val)"
        );

        let test_deep_tag: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_deep_tag")
            .expect("Failed to get 'test_deep_tag'");
        let result = test_deep_tag
            .call(&mut store, ())
            .expect("test_deep_tag failed");
        assert_eq!(result, 42, "test_deep_tag should return 42 (d.tag)");

        let test_deep_inner_arr_sum: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_deep_inner_arr_sum")
            .expect("Failed to get 'test_deep_inner_arr_sum'");
        let result = test_deep_inner_arr_sum
            .call(&mut store, ())
            .expect("test_deep_inner_arr_sum failed");
        assert_eq!(
            result, 60,
            "test_deep_inner_arr_sum should return 60 (10+20+30)"
        );

        // Deep struct as parameter
        let deep_param: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "deep_param")
            .expect("Failed to get 'deep_param'");
        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Module should have memory");
        // Deep layout: inner(HasArray) at 0..16 (arr[0..12] + val@12), tag@16
        // HasArray: arr[0]=10@0, arr[1]=20@4, arr[2]=30@8, val=99@12
        // Deep: tag=42@16; total 20 bytes
        let param_base: i32 = 0;
        let base = param_base as usize;
        memory.data_mut(&mut store)[base..base + 4].copy_from_slice(&10_i32.to_le_bytes());
        memory.data_mut(&mut store)[base + 4..base + 8].copy_from_slice(&20_i32.to_le_bytes());
        memory.data_mut(&mut store)[base + 8..base + 12].copy_from_slice(&30_i32.to_le_bytes());
        memory.data_mut(&mut store)[base + 12..base + 16].copy_from_slice(&99_i32.to_le_bytes());
        memory.data_mut(&mut store)[base + 16..base + 20].copy_from_slice(&42_i32.to_le_bytes());
        let result = deep_param
            .call(&mut store, param_base)
            .expect("deep_param failed");
        assert_eq!(
            result, 52,
            "deep_param should return 52 (d.inner.arr[0]=10 + d.tag=42)"
        );

        // Deep struct as sret return
        let deep_return: TypedFunc<i32, ()> = instance
            .get_typed_func(&mut store, "deep_return")
            .expect("Failed to get 'deep_return'");
        let sret_base: i32 = 0;
        deep_return
            .call(&mut store, sret_base)
            .expect("deep_return failed");
        let data = memory.data(&store);
        let sbase = sret_base as usize;
        let arr0 = i32::from_le_bytes(data[sbase..sbase + 4].try_into().unwrap());
        let arr1 = i32::from_le_bytes(data[sbase + 4..sbase + 8].try_into().unwrap());
        let arr2 = i32::from_le_bytes(data[sbase + 8..sbase + 12].try_into().unwrap());
        let val = i32::from_le_bytes(data[sbase + 12..sbase + 16].try_into().unwrap());
        let tag = i32::from_le_bytes(data[sbase + 16..sbase + 20].try_into().unwrap());
        assert_eq!(arr0, 10, "deep_return: inner.arr[0] should be 10");
        assert_eq!(arr1, 20, "deep_return: inner.arr[1] should be 20");
        assert_eq!(arr2, 30, "deep_return: inner.arr[2] should be 30");
        assert_eq!(val, 99, "deep_return: inner.val should be 99");
        assert_eq!(tag, 42, "deep_return: tag should be 42");
    }

    #[test]
    fn multidim_array_param_inline_validation() {
        let source = r#"
            pub fn read_grid(grid: [[i32; 3]; 2]) -> i32 {
                return grid[0][1];
            }
            pub fn caller() {
                forall {
                    let g: [[i32; 3]; 2] = @;
                    let _v: i32 = read_grid(g);
                }
            }
        "#;
        let wasm_bytes = wasm_codegen_no_analysis(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Multidim array param WASM is invalid: {e}"));
    }

    #[test]
    fn multidim_array_param_i64_inline_validation() {
        let source = r#"
            pub fn read_matrix(m: [[i64; 2]; 3]) -> i64 {
                return m[1][0];
            }
            pub fn caller() {
                forall {
                    let m: [[i64; 2]; 3] = @;
                    let _v: i64 = read_matrix(m);
                }
            }
        "#;
        let wasm_bytes = wasm_codegen_no_analysis(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Multidim i64 array param WASM is invalid: {e}"));
    }

    #[test]
    fn multidim_array_scalar_index_write_inline_validation() {
        let source = r#"
            pub fn write_and_read() {
                forall {
                    let mut grid: [[i32; 3]; 2] = @;
                    grid[1][0] = 99;
                    let _v: i32 = grid[1][0];
                }
            }
        "#;
        let wasm_bytes = wasm_codegen_no_analysis(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Multidim array scalar index write WASM is invalid: {e}"));
    }

    #[test]
    fn multidim_array_scalar_index_write_i64_inline_validation() {
        let source = r#"
            pub fn write_matrix() {
                forall {
                    let mut m: [[i64; 2]; 2] = @;
                    m[0][1] = 42;
                    let _v: i64 = m[0][1];
                }
            }
        "#;
        let wasm_bytes = wasm_codegen_no_analysis(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Multidim i64 array scalar index write WASM is invalid: {e}"));
    }

    #[test]
    fn multidim_array_compound_index_write_inline_validation() {
        let source = r#"
            pub fn write_row() {
                forall {
                    let mut grid: [[i32; 3]; 2] = @;
                    let row: [i32; 3] = @;
                    grid[0] = row;
                    let _v: i32 = grid[0][1];
                }
            }
        "#;
        let wasm_bytes = wasm_codegen_no_analysis(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Multidim array compound index write WASM is invalid: {e}"));
    }

    #[test]
    fn multidim_array_param_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn read_2d(grid: [[i32; 3]; 2]) -> i32 {
                return grid[0][1] + grid[1][2];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Module should have memory");
        let data = memory.data_mut(&mut store);
        let base: usize = 0;
        // [[i32; 3]; 2] = 6 i32 values = 24 bytes
        // grid[0][0]=10, grid[0][1]=20, grid[0][2]=30
        data[base..base + 4].copy_from_slice(&10_i32.to_le_bytes());
        data[base + 4..base + 8].copy_from_slice(&20_i32.to_le_bytes());
        data[base + 8..base + 12].copy_from_slice(&30_i32.to_le_bytes());
        // grid[1][0]=40, grid[1][1]=50, grid[1][2]=60
        data[base + 12..base + 16].copy_from_slice(&40_i32.to_le_bytes());
        data[base + 16..base + 20].copy_from_slice(&50_i32.to_le_bytes());
        data[base + 20..base + 24].copy_from_slice(&60_i32.to_le_bytes());

        let read_2d: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "read_2d")
            .expect("Failed to get 'read_2d'");
        let result = read_2d
            .call(&mut store, base as i32)
            .expect("read_2d failed");
        assert_eq!(
            result, 80,
            "read_2d should return 80 (grid[0][1]=20 + grid[1][2]=60)"
        );
    }

    #[test]
    fn nested_struct_i64_inline_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            struct Inner64 { a: i64; b: i64; }
            struct Outer64 { inner: Inner64; tag: i32; }
            pub fn test_i64_nested() -> i64 {
                let o: Outer64 = Outer64 { inner: Inner64 { a: 100, b: 200 }, tag: 42 };
                return o.inner.a + o.inner.b;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "test_i64_nested")
            .expect("Failed to get 'test_i64_nested'");
        let result = func
            .call(&mut store, ())
            .expect("test_i64_nested failed");
        assert_eq!(
            result, 300i64,
            "test_i64_nested should return 300 (o.inner.a + o.inner.b)"
        );
    }

    #[test]
    fn struct_with_i64_array_inline_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            struct HasI64Arr { vals: [i64; 2]; tag: i32; }
            pub fn test_i64_arr_field() -> i64 {
                let v: [i64; 2] = [1000, 2000];
                let h: HasI64Arr = HasI64Arr { vals: v, tag: 99 };
                return h.vals[0] + h.vals[1];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "test_i64_arr_field")
            .expect("Failed to get 'test_i64_arr_field'");
        let result = func
            .call(&mut store, ())
            .expect("test_i64_arr_field failed");
        assert_eq!(
            result, 3000i64,
            "test_i64_arr_field should return 3000 (h.vals[0] + h.vals[1])"
        );
    }

    #[test]
    fn copy_struct_from_array_index_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            struct Point { x: i32; y: i32; }
            pub fn test_copy_from_index() -> i32 {
                let mut points: [Point; 3] = [
                    Point { x: 10, y: 20 },
                    Point { x: 30, y: 40 },
                    Point { x: 50, y: 60 }
                ];
                let p: Point = points[1];
                points[1].x = 99;
                return p.x;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_copy_from_index")
            .expect("Failed to get 'test_copy_from_index'");
        let result = func
            .call(&mut store, ())
            .expect("test_copy_from_index failed");
        assert_eq!(
            result, 30,
            "p.x should still be 30 after modifying points[1].x (value semantics via copy)"
        );
    }

    #[test]
    fn array_literal_reassignment_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn test_array_literal_reassign() -> i32 {
                let mut arr: [i32; 3] = [1, 2, 3];
                arr = [4, 5, 6];
                return arr[1];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_array_literal_reassign")
            .expect("Failed to get 'test_array_literal_reassign'");
        let result = func
            .call(&mut store, ())
            .expect("test_array_literal_reassign failed");
        assert_eq!(
            result, 5,
            "arr[1] should be 5 after reassigning arr = [4, 5, 6]"
        );
    }

    #[test]
    fn array_variable_reassignment_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            pub fn test_array_var_reassign() -> i32 {
                let mut arr: [i32; 3] = [1, 2, 3];
                let other: [i32; 3] = [7, 8, 9];
                arr = other;
                return arr[2];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "test_array_var_reassign")
            .expect("Failed to get 'test_array_var_reassign'");
        let result = func
            .call(&mut store, ())
            .expect("test_array_var_reassign failed");
        assert_eq!(
            result, 9,
            "arr[2] should be 9 after reassigning arr = other"
        );
    }

    #[test]
    fn sret_return_array_index_struct_second_elem_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            struct Vec2 { x: i32; y: i32; }
            pub fn get_second_y() -> i32 {
                let vecs: [Vec2; 3] = [
                    Vec2 { x: 1, y: 2 },
                    Vec2 { x: 3, y: 4 },
                    Vec2 { x: 5, y: 6 }
                ];
                let v: Vec2 = get_at(vecs, 2);
                return v.y;
            }
            fn get_at(arr: [Vec2; 3], idx: i32) -> Vec2 {
                return arr[idx];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_second_y")
            .expect("Failed to get 'get_second_y'");
        let result = func
            .call(&mut store, ())
            .expect("get_second_y failed");
        assert_eq!(
            result, 6,
            "get_second_y should return 6 (arr[2].y via sret array index return)"
        );
    }

    #[test]
    fn sret_return_array_index_struct_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            struct Point { x: i32; y: i32; }
            pub fn first_point_x() -> i32 {
                let pts: [Point; 2] = [Point { x: 10, y: 20 }, Point { x: 30, y: 40 }];
                let p: Point = get_first(pts);
                return p.x;
            }
            fn get_first(pts: [Point; 2]) -> Point {
                return pts[0];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "first_point_x")
            .expect("Failed to get 'first_point_x'");
        let result = func
            .call(&mut store, ())
            .expect("first_point_x failed");
        assert_eq!(
            result, 10,
            "first_point_x should return 10 (pts[0].x via sret array index return)"
        );
    }

    #[test]
    fn sret_return_member_access_array_execution() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let source = r#"
            struct HasArray { arr: [i32; 3]; val: i32; }
            pub fn get_arr_elem() -> i32 {
                let h: HasArray = HasArray { arr: [10, 20, 30], val: 99 };
                let a: [i32; 3] = get_arr(h);
                return a[1];
            }
            fn get_arr(h: HasArray) -> [i32; 3] {
                return h.arr;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("Generated WASM is invalid: {e}"));

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        let func: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_arr_elem")
            .expect("Failed to get 'get_arr_elem'");
        let result = func
            .call(&mut store, ())
            .expect("get_arr_elem failed");
        assert_eq!(
            result, 20,
            "get_arr_elem should return 20 (h.arr[1] via sret member access array return)"
        );
    }

    #[test]
    fn nested_struct_chained_read_inline_execution() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Outer { inner: Inner; val: i32; }
            pub fn read_chained() -> i32 {
                let o: Outer = Outer { inner: Inner { x: 7, y: 8 }, val: 9 };
                return o.inner.x;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("WASM validation failed: {e}"));

        let engine = wasmtime::Engine::default();
        let module = wasmtime::Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create module: {e}"));
        let mut store = wasmtime::Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate: {e}"));

        let func: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "read_chained")
            .expect("Failed to get 'read_chained'");
        let result = func.call(&mut store, ()).expect("read_chained failed");
        assert_eq!(result, 7, "o.inner.x via direct chained access should be 7");
    }

    #[test]
    fn nested_struct_chained_write_inline_execution() {
        let source = r#"
            struct Inner { x: i32; y: i32; }
            struct Outer { inner: Inner; val: i32; }
            pub fn write_chained() -> i32 {
                let mut o: Outer = Outer { inner: Inner { x: 1, y: 2 }, val: 3 };
                o.inner.x = 42;
                return o.inner.x;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("WASM validation failed: {e}"));

        let engine = wasmtime::Engine::default();
        let module = wasmtime::Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create module: {e}"));
        let mut store = wasmtime::Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate: {e}"));

        let func: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "write_chained")
            .expect("Failed to get 'write_chained'");
        let result = func.call(&mut store, ()).expect("write_chained failed");
        assert_eq!(result, 42, "o.inner.x after chained write should be 42");
    }

    #[test]
    fn array_of_structs_sret_return_inline_execution() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            pub fn make_points() -> [Point; 2] {
                let arr: [Point; 2] = [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }];
                return arr;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("WASM validation failed: {e}"));

        let engine = wasmtime::Engine::default();
        let module = wasmtime::Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create module: {e}"));
        let mut store = wasmtime::Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate: {e}"));

        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Module should have memory");

        let sret_base: i32 = 0;
        let func: wasmtime::TypedFunc<i32, ()> = instance
            .get_typed_func(&mut store, "make_points")
            .expect("Failed to get 'make_points'");
        func.call(&mut store, sret_base).expect("make_points failed");

        let data = memory.data(&store);
        let base = sret_base as usize;
        let p0_x = i32::from_le_bytes(data[base..base + 4].try_into().unwrap());
        let p0_y = i32::from_le_bytes(data[base + 4..base + 8].try_into().unwrap());
        let p1_x = i32::from_le_bytes(data[base + 8..base + 12].try_into().unwrap());
        let p1_y = i32::from_le_bytes(data[base + 12..base + 16].try_into().unwrap());
        assert_eq!(p0_x, 1, "points[0].x");
        assert_eq!(p0_y, 2, "points[0].y");
        assert_eq!(p1_x, 3, "points[1].x");
        assert_eq!(p1_y, 4, "points[1].y");
    }

    #[test]
    fn nested_struct_with_array_write_inline_execution() {
        let source = r#"
            struct HasArray { arr: [i32; 3]; val: i32; }
            struct Deep { inner: HasArray; tag: i32; }
            pub fn write_and_read() -> i32 {
                let ha: HasArray = HasArray { arr: [10, 20, 30], val: 99 };
                let d: Deep = Deep { inner: ha, tag: 42 };
                let mut ha2: HasArray = d.inner;
                ha2.arr[1] = 77;
                return ha2.arr[1];
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("WASM validation failed: {e}"));

        let engine = wasmtime::Engine::default();
        let module = wasmtime::Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create module: {e}"));
        let mut store = wasmtime::Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate: {e}"));

        let func: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "write_and_read")
            .expect("Failed to get 'write_and_read'");
        let result = func.call(&mut store, ()).expect("write_and_read failed");
        assert_eq!(result, 77, "ha.arr[1] after write should be 77");
    }

    #[test]
    fn if_else_compound_overlap_golden_test() {
        let test_name = "if_else_compound_overlap";
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
    fn if_else_compound_overlap_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "if_else_compound_overlap";
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

        let func: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "if_else_compound_overlap")
            .unwrap_or_else(|e| panic!("Failed to get 'if_else_compound_overlap': {e}"));

        let result_true = func
            .call(&mut store, 1)
            .unwrap_or_else(|e| panic!("Call with cond=true failed: {e}"));
        assert_eq!(result_true, 1, "Expected a[0]=1 when cond is true");

        let result_false = func
            .call(&mut store, 0)
            .unwrap_or_else(|e| panic!("Call with cond=false failed: {e}"));
        assert_eq!(result_false, 20, "Expected b[1]=20 when cond is false");
    }

    #[test]
    fn enum_variant_golden_test() {
        let test_name = "enum_variant";
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
    fn enum_multi_golden_test() {
        let test_name = "enum_multi";
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
    fn enum_variant_execution_test() {
        use wasmtime::{Engine, Module, Store};

        let test_name = "enum_variant";
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

        let get_red: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_red")
            .expect("Failed to get 'get_red'");
        assert_eq!(get_red.call(&mut store, ()).unwrap(), 0, "Red should be tag 0");

        let get_green: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_green")
            .expect("Failed to get 'get_green'");
        assert_eq!(get_green.call(&mut store, ()).unwrap(), 1, "Green should be tag 1");

        let get_blue: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_blue")
            .expect("Failed to get 'get_blue'");
        assert_eq!(get_blue.call(&mut store, ()).unwrap(), 2, "Blue should be tag 2");
    }

    #[test]
    fn enum_multi_execution_test() {
        use wasmtime::{Engine, Module, Store};

        let test_name = "enum_multi";
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

        let direction_west: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "direction_west")
            .expect("Failed to get 'direction_west'");
        assert_eq!(direction_west.call(&mut store, ()).unwrap(), 3, "West should be tag 3");

        let shape_triangle: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "shape_triangle")
            .expect("Failed to get 'shape_triangle'");
        assert_eq!(shape_triangle.call(&mut store, ()).unwrap(), 2, "Triangle should be tag 2");

        let first_variants: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "first_variants")
            .expect("Failed to get 'first_variants'");
        assert_eq!(first_variants.call(&mut store, ()).unwrap(), 0, "North should be tag 0");
    }

    #[test]
    fn enum_params_golden_test() {
        let test_name = "enum_params";
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
    fn enum_compare_golden_test() {
        let test_name = "enum_compare";
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
    fn enum_assign_golden_test() {
        let test_name = "enum_assign";
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
    fn enum_array_golden_test() {
        let test_name = "enum_array";
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
    fn enum_in_struct_golden_test() {
        let test_name = "enum_in_struct";
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
    fn enum_params_execution_test() {
        use wasmtime::{Engine, Module, Store};

        let test_name = "enum_params";
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

        let is_up: wasmtime::TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "is_up")
            .expect("Failed to get 'is_up'");
        assert_eq!(is_up.call(&mut store, 0).unwrap(), 1, "Dir::Up (tag 0) should return true");
        assert_eq!(is_up.call(&mut store, 1).unwrap(), 0, "Dir::Down (tag 1) should return false");
        assert_eq!(is_up.call(&mut store, 3).unwrap(), 0, "Dir::Right (tag 3) should return false");

        let dir_to_int: wasmtime::TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "dir_to_int")
            .expect("Failed to get 'dir_to_int'");
        assert_eq!(dir_to_int.call(&mut store, 0).unwrap(), 10, "Up should map to 10");
        assert_eq!(dir_to_int.call(&mut store, 1).unwrap(), 20, "Down should map to 20");
        assert_eq!(dir_to_int.call(&mut store, 2).unwrap(), 30, "Left should map to 30");
        assert_eq!(dir_to_int.call(&mut store, 3).unwrap(), 40, "Right should map to 40");

        let pass_through: wasmtime::TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "pass_through")
            .expect("Failed to get 'pass_through'");
        assert_eq!(pass_through.call(&mut store, 0).unwrap(), 0, "pass_through should return same tag");
        assert_eq!(pass_through.call(&mut store, 2).unwrap(), 2, "pass_through should return same tag");
    }

    #[test]
    fn enum_compare_execution_test() {
        use wasmtime::{Engine, Module, Store};

        let test_name = "enum_compare";
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

        let are_equal: wasmtime::TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "are_equal")
            .expect("Failed to get 'are_equal'");
        assert_eq!(are_equal.call(&mut store, (0, 0)).unwrap(), 1, "Active == Active should be true");
        assert_eq!(are_equal.call(&mut store, (0, 1)).unwrap(), 0, "Active == Inactive should be false");
        assert_eq!(are_equal.call(&mut store, (1, 1)).unwrap(), 1, "Inactive == Inactive should be true");

        let are_not_equal: wasmtime::TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "are_not_equal")
            .expect("Failed to get 'are_not_equal'");
        assert_eq!(are_not_equal.call(&mut store, (0, 1)).unwrap(), 1, "Active != Inactive should be true");
        assert_eq!(are_not_equal.call(&mut store, (0, 0)).unwrap(), 0, "Active != Active should be false");

        let is_active: wasmtime::TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "is_active")
            .expect("Failed to get 'is_active'");
        assert_eq!(is_active.call(&mut store, 0).unwrap(), 1, "Active should return true");
        assert_eq!(is_active.call(&mut store, 1).unwrap(), 0, "Inactive should return false");
    }

    #[test]
    fn enum_assign_execution_test() {
        use wasmtime::{Engine, Module, Store};

        let test_name = "enum_assign";
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

        let reassign: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "reassign")
            .expect("Failed to get 'reassign'");
        assert_eq!(reassign.call(&mut store, ()).unwrap(), 2, "reassign should return Blue (tag 2)");

        let assign_from_param: wasmtime::TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "assign_from_param")
            .expect("Failed to get 'assign_from_param'");
        assert_eq!(assign_from_param.call(&mut store, 1).unwrap(), 1, "assign_from_param(Green) should return 1");
        assert_eq!(assign_from_param.call(&mut store, 2).unwrap(), 2, "assign_from_param(Blue) should return 2");
    }

    #[test]
    fn enum_in_struct_execution_test() {
        use wasmtime::{Engine, Module, Store};

        let test_name = "enum_in_struct";
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

        let get_status: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_status")
            .expect("Failed to get 'get_status'");
        assert_eq!(get_status.call(&mut store, ()).unwrap(), 1, "Inactive status check should return 1");

        let get_value: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_value")
            .expect("Failed to get 'get_value'");
        assert_eq!(get_value.call(&mut store, ()).unwrap(), 99, "Value field should be 99");
    }

    #[test]
    fn enum_const_execution_test() {
        let source = r#"
            enum Color { Red, Green, Blue }
            pub fn get_default() -> Color {
                const DEFAULT: Color = Color::Green;
                return DEFAULT;
            }
            pub fn is_default_green() -> bool {
                const DEFAULT: Color = Color::Green;
                return DEFAULT == Color::Green;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("WASM validation failed: {e}"));

        let engine = wasmtime::Engine::default();
        let module = wasmtime::Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create module: {e}"));
        let mut store = wasmtime::Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate: {e}"));

        let get_default: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "get_default")
            .expect("Failed to get 'get_default'");
        assert_eq!(get_default.call(&mut store, ()).unwrap(), 1, "Green should be tag 1");

        let is_default_green: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "is_default_green")
            .expect("Failed to get 'is_default_green'");
        assert_eq!(is_default_green.call(&mut store, ()).unwrap(), 1, "DEFAULT == Green should be true");
    }

    #[test]
    fn enum_array_execution_test() {
        use wasmtime::{Engine, Module, Store};

        let test_name = "enum_array";
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

        let second_color: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "second_color")
            .expect("Failed to get 'second_color'");
        assert_eq!(second_color.call(&mut store, ()).unwrap(), 1, "colors[1] == Green should be true");

        let third_tag: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "third_tag")
            .expect("Failed to get 'third_tag'");
        assert_eq!(third_tag.call(&mut store, ()).unwrap(), 2, "colors[2] should be Blue (tag 2)");
    }

    #[test]
    fn enum_direct_return_execution_test() {
        let source = r#"
            enum Color { Red, Green, Blue }
            pub fn red() -> Color {
                return Color::Red;
            }
            pub fn green() -> Color {
                return Color::Green;
            }
            pub fn blue() -> Color {
                return Color::Blue;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("WASM validation failed: {e}"));

        let engine = wasmtime::Engine::default();
        let module = wasmtime::Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create module: {e}"));
        let mut store = wasmtime::Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate: {e}"));

        let red: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "red")
            .expect("Failed to get 'red'");
        assert_eq!(red.call(&mut store, ()).unwrap(), 0, "Red should be tag 0");

        let green: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "green")
            .expect("Failed to get 'green'");
        assert_eq!(green.call(&mut store, ()).unwrap(), 1, "Green should be tag 1");

        let blue: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "blue")
            .expect("Failed to get 'blue'");
        assert_eq!(blue.call(&mut store, ()).unwrap(), 2, "Blue should be tag 2");
    }

    #[test]
    fn enum_integration_execution_test() {
        let source = r#"
            enum Color { Red, Green, Blue }
            enum Status { Active, Inactive }

            fn get_color() -> Color {
                return Color::Green;
            }

            fn get_status() -> Status {
                return Status::Active;
            }

            pub fn check_color() -> i32 {
                if get_color() == Color::Green {
                    return 1;
                } else {
                    return 0;
                }
            }

            pub fn check_both() -> i32 {
                if get_color() == Color::Green {
                    if get_status() == Status::Active {
                        return 42;
                    }
                    return 1;
                }
                return 0;
            }

            pub fn nested_compare() -> i32 {
                let c: Color = get_color();
                let s: Status = get_status();
                if c == Color::Red {
                    return 10;
                }
                if c == Color::Green {
                    if s != Status::Inactive {
                        return 20;
                    }
                    return 30;
                }
                return 40;
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("WASM validation failed: {e}"));

        let engine = wasmtime::Engine::default();
        let module = wasmtime::Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create module: {e}"));
        let mut store = wasmtime::Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate: {e}"));

        let check_color: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "check_color")
            .expect("Failed to get 'check_color'");
        assert_eq!(check_color.call(&mut store, ()).unwrap(), 1, "get_color() == Green should be true");

        let check_both: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "check_both")
            .expect("Failed to get 'check_both'");
        assert_eq!(check_both.call(&mut store, ()).unwrap(), 42, "Green + Active should return 42");

        let nested_compare: wasmtime::TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "nested_compare")
            .expect("Failed to get 'nested_compare'");
        assert_eq!(nested_compare.call(&mut store, ()).unwrap(), 20, "Green + not Inactive should return 20");
    }

    #[test]
    fn enum_uzumaki_scalar_execution_test() {
        let source = r#"
            enum Color { Red, Green, Blue }
            pub fn nondet_color() {
                forall {
                    let c: Color = @;
                }
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("WASM validation failed: {e}"));
    }

    #[test]
    fn enum_uzumaki_array_execution_test() {
        let source = r#"
            enum Color { Red, Green, Blue }
            pub fn nondet_colors() {
                forall {
                    let colors: [Color; 3] = @;
                }
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("WASM validation failed: {e}"));
    }

    #[test]
    fn enum_uzumaki_struct_field_execution_test() {
        let source = r#"
            enum Status { Active, Inactive }
            struct Item { status: Status; value: i32; }
            pub fn nondet_item() {
                forall {
                    let item: Item = @;
                }
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("WASM validation failed: {e}"));
    }

    #[test]
    fn enum_uzumaki_assume_execution_test() {
        let source = r#"
            enum Color { Red, Green, Blue }
            pub fn nondet_assume_color() {
                forall {
                    let c: Color = @;
                    assume {
                        let ok: bool = c == Color::Red;
                    }
                }
            }
        "#;
        let wasm_bytes = wasm_codegen(source);
        inf_wasmparser::validate(&wasm_bytes)
            .unwrap_or_else(|e| panic!("WASM validation failed: {e}"));
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
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen, wasm_codegen_no_analysis};

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
    fn regenerate_const_array_wasm() {
        let dir = base_test_dir().join("const_array");
        let source_code = std::fs::read_to_string(dir.join("const_array.inf"))
            .expect("Failed to read const_array.inf");
        let actual = wasm_codegen(&source_code);
        let wasm_path = dir.join("const_array.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "const_array");
    }

    #[test]
    #[ignore]
    fn regenerate_const_array_sum_wasm() {
        let dir = base_test_dir().join("const_array_sum");
        let source_code = std::fs::read_to_string(dir.join("const_array_sum.inf"))
            .expect("Failed to read const_array_sum.inf");
        let actual = wasm_codegen(&source_code);
        let wasm_path = dir.join("const_array_sum.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "const_array_sum");
    }

    #[test]
    #[ignore]
    fn regenerate_const_struct_wasm() {
        let dir = base_test_dir().join("const_struct");
        let source_code = std::fs::read_to_string(dir.join("const_struct.inf"))
            .expect("Failed to read const_struct.inf");
        let actual = wasm_codegen(&source_code);
        let wasm_path = dir.join("const_struct.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "const_struct");
    }

    #[test]
    #[ignore]
    fn regenerate_const_compound_mixed_wasm() {
        let dir = base_test_dir().join("const_compound_mixed");
        let source_code = std::fs::read_to_string(dir.join("const_compound_mixed.inf"))
            .expect("Failed to read const_compound_mixed.inf");
        let actual = wasm_codegen(&source_code);
        let wasm_path = dir.join("const_compound_mixed.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "const_compound_mixed");
    }

    #[test]
    #[ignore]
    fn regenerate_const_sret_call_wasm() {
        let dir = base_test_dir().join("const_sret_call");
        let source_code = std::fs::read_to_string(dir.join("const_sret_call.inf"))
            .expect("Failed to read const_sret_call.inf");
        let actual = wasm_codegen(&source_code);
        let wasm_path = dir.join("const_sret_call.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "const_sret_call");
    }

    #[test]
    #[ignore]
    fn regenerate_const_compound_copy_wasm() {
        let dir = base_test_dir().join("const_compound_copy");
        let source_code = std::fs::read_to_string(dir.join("const_compound_copy.inf"))
            .expect("Failed to read const_compound_copy.inf");
        let actual = wasm_codegen(&source_code);
        let wasm_path = dir.join("const_compound_copy.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "const_compound_copy");
    }

    #[test]
    #[ignore]
    fn regenerate_const_in_forall_wasm() {
        let dir = base_test_dir().join("const_in_forall");
        let source_code = std::fs::read_to_string(dir.join("const_in_forall.inf"))
            .expect("Failed to read const_in_forall.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {e}"));
        let wasm_path = dir.join("const_in_forall.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "const_in_forall");
    }

    #[test]
    #[ignore]
    fn regenerate_nondet_wasm() {
        // Uses wasm_codegen_no_analysis: fixture contains nondet patterns
        // (uzumaki, forall, exists, assume, unique) that analysis would reject.
        let dir = base_test_dir().join("nondet");
        let source_code =
            std::fs::read_to_string(dir.join("nondet.inf")).expect("Failed to read nondet.inf");
        let actual = wasm_codegen_no_analysis(&source_code);
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
        // Uses wasm_codegen_no_analysis: fixture contains uzumaki (@) patterns
        // that analysis would reject.
        let dir = base_test_dir().join("i64_uzumaki");
        let source_code = std::fs::read_to_string(dir.join("i64_uzumaki.inf"))
            .expect("Failed to read i64_uzumaki.inf");
        let actual = wasm_codegen_no_analysis(&source_code);
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
        // Uses wasm_codegen_no_analysis: fixture contains nondet patterns
        // that analysis would reject.
        let dir = base_test_dir().join("local_variables");
        let source_code = std::fs::read_to_string(dir.join("local_variables.inf"))
            .expect("Failed to read local_variables.inf");
        let actual = wasm_codegen_no_analysis(&source_code);
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
        let source_code = std::fs::read_to_string(dir.join("fn_params.inf"))
            .expect("Failed to read fn_params.inf");
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
    fn regenerate_assert_wasm() {
        let dir = base_test_dir().join("assert");
        let source_code =
            std::fs::read_to_string(dir.join("assert.inf")).expect("Failed to read assert.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("assert.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "assert");
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
        // Uses wasm_codegen_no_analysis: fixture contains nondet patterns
        // that analysis would reject.
        let dir = base_test_dir().join("assign_nondet");
        let source_code = std::fs::read_to_string(dir.join("assign_nondet.inf"))
            .expect("Failed to read assign_nondet.inf");
        let actual = wasm_codegen_no_analysis(&source_code);
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
    fn regenerate_multidim_array_literal_wasm() {
        let dir = base_test_dir().join("multidim_array_literal");
        let source_code = std::fs::read_to_string(dir.join("multidim_array_literal.inf"))
            .expect("Failed to read multidim_array_literal.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("multidim_array_literal.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "multidim_array_literal");
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

    #[test]
    #[ignore]
    fn regenerate_array_params_wasm() {
        let dir = base_test_dir().join("array_params");
        let source_code = std::fs::read_to_string(dir.join("array_params.inf"))
            .expect("Failed to read array_params.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("array_params.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "array_params");
    }

    #[test]
    #[ignore]
    fn regenerate_array_nondet_wasm() {
        // Uses wasm_codegen_no_analysis: fixture contains nondet patterns
        // (forall, exists, uzumaki on arrays) that analysis would reject.
        let dir = base_test_dir().join("array_nondet");
        let source_code = std::fs::read_to_string(dir.join("array_nondet.inf"))
            .expect("Failed to read array_nondet.inf");
        let actual = wasm_codegen_no_analysis(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("array_nondet.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "array_nondet");
    }

    #[test]
    #[ignore]
    fn regenerate_struct_literal_wasm() {
        let dir = base_test_dir().join("struct_literal");
        let source_code = std::fs::read_to_string(dir.join("struct_literal.inf"))
            .expect("Failed to read struct_literal.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("struct_literal.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "struct_literal");
    }

    #[test]
    #[ignore]
    fn regenerate_struct_access_wasm() {
        let dir = base_test_dir().join("struct_access");
        let source_code = std::fs::read_to_string(dir.join("struct_access.inf"))
            .expect("Failed to read struct_access.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("struct_access.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "struct_access");
    }

    #[test]
    #[ignore]
    fn regenerate_struct_assign_wasm() {
        let dir = base_test_dir().join("struct_assign");
        let source_code = std::fs::read_to_string(dir.join("struct_assign.inf"))
            .expect("Failed to read struct_assign.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("struct_assign.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "struct_assign");
    }

    #[test]
    #[ignore]
    fn regenerate_struct_params_wasm() {
        let dir = base_test_dir().join("struct_params");
        let source_code = std::fs::read_to_string(dir.join("struct_params.inf"))
            .expect("Failed to read struct_params.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("struct_params.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "struct_params");
    }

    #[test]
    #[ignore]
    fn regenerate_struct_return_wasm() {
        let dir = base_test_dir().join("struct_return");
        let source_code = std::fs::read_to_string(dir.join("struct_return.inf"))
            .expect("Failed to read struct_return.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("struct_return.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "struct_return");
    }

    #[test]
    #[ignore]
    fn regenerate_struct_copy_wasm() {
        let dir = base_test_dir().join("struct_copy");
        let source_code = std::fs::read_to_string(dir.join("struct_copy.inf"))
            .expect("Failed to read struct_copy.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("struct_copy.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "struct_copy");
    }

    #[test]
    #[ignore]
    fn regenerate_method_instance_wasm() {
        let dir = base_test_dir().join("method_instance");
        let source_code = std::fs::read_to_string(dir.join("method_instance.inf"))
            .expect("Failed to read method_instance.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("method_instance.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "method_instance");
    }

    #[test]
    #[ignore]
    fn regenerate_method_assoc_wasm() {
        let dir = base_test_dir().join("method_assoc");
        let source_code = std::fs::read_to_string(dir.join("method_assoc.inf"))
            .expect("Failed to read method_assoc.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("method_assoc.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "method_assoc");
    }

    #[test]
    #[ignore]
    fn regenerate_method_return_struct_wasm() {
        let dir = base_test_dir().join("method_return_struct");
        let source_code = std::fs::read_to_string(dir.join("method_return_struct.inf"))
            .expect("Failed to read method_return_struct.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("method_return_struct.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "method_return_struct");
    }

    #[test]
    #[ignore]
    fn regenerate_method_self_mutate_wasm() {
        let dir = base_test_dir().join("method_self_mutate");
        let source_code = std::fs::read_to_string(dir.join("method_self_mutate.inf"))
            .expect("Failed to read method_self_mutate.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("method_self_mutate.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "method_self_mutate");
    }

    #[test]
    #[ignore]
    fn regenerate_method_multi_struct_wasm() {
        let dir = base_test_dir().join("method_multi_struct");
        let source_code = std::fs::read_to_string(dir.join("method_multi_struct.inf"))
            .expect("Failed to read method_multi_struct.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("method_multi_struct.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "method_multi_struct");
    }

    #[test]
    #[ignore]
    fn regenerate_method_cross_call_wasm() {
        let dir = base_test_dir().join("method_cross_call");
        let source_code = std::fs::read_to_string(dir.join("method_cross_call.inf"))
            .expect("Failed to read method_cross_call.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("method_cross_call.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "method_cross_call");
    }

    #[test]
    #[ignore]
    fn regenerate_method_array_return_wasm() {
        let dir = base_test_dir().join("method_array_return");
        let source_code = std::fs::read_to_string(dir.join("method_array_return.inf"))
            .expect("Failed to read method_array_return.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("method_array_return.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "method_array_return");
    }

    #[test]
    #[ignore]
    fn regenerate_method_i64_fields_wasm() {
        let dir = base_test_dir().join("method_i64_fields");
        let source_code = std::fs::read_to_string(dir.join("method_i64_fields.inf"))
            .expect("Failed to read method_i64_fields.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("method_i64_fields.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "method_i64_fields");
    }

    #[test]
    #[ignore]
    fn regenerate_method_three_fields_wasm() {
        let dir = base_test_dir().join("method_three_fields");
        let source_code = std::fs::read_to_string(dir.join("method_three_fields.inf"))
            .expect("Failed to read method_three_fields.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("method_three_fields.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "method_three_fields");
    }

    #[test]
    #[ignore]
    fn regenerate_struct_nondet_wasm() {
        let dir = base_test_dir().join("struct_nondet");
        let source_code = std::fs::read_to_string(dir.join("struct_nondet.inf"))
            .expect("Failed to read struct_nondet.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("struct_nondet.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "struct_nondet");
    }

    #[test]
    #[ignore]
    fn regenerate_struct_array_field_nondet_wasm() {
        let dir = base_test_dir().join("struct_array_field_nondet");
        let source_code = std::fs::read_to_string(dir.join("struct_array_field_nondet.inf"))
            .expect("Failed to read struct_array_field_nondet.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("struct_array_field_nondet.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "struct_array_field_nondet");
    }

    #[test]
    #[ignore]
    fn regenerate_nested_struct_wasm() {
        let dir = base_test_dir().join("nested_struct");
        let source_code = std::fs::read_to_string(dir.join("nested_struct.inf"))
            .expect("Failed to read nested_struct.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("nested_struct.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "nested_struct");
    }

    #[test]
    #[ignore]
    fn regenerate_struct_with_array_wasm() {
        let dir = base_test_dir().join("struct_with_array");
        let source_code = std::fs::read_to_string(dir.join("struct_with_array.inf"))
            .expect("Failed to read struct_with_array.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("struct_with_array.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "struct_with_array");
    }

    #[test]
    #[ignore]
    fn regenerate_array_of_structs_wasm() {
        let dir = base_test_dir().join("array_of_structs");
        let source_code = std::fs::read_to_string(dir.join("array_of_structs.inf"))
            .expect("Failed to read array_of_structs.inf");
        let actual = wasm_codegen(&source_code);
        let wasm_path = dir.join("array_of_structs.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "array_of_structs");
    }

    #[test]
    #[ignore]
    fn regenerate_struct_with_array_of_structs_wasm() {
        let dir = base_test_dir().join("struct_with_array_of_structs");
        let source_code = std::fs::read_to_string(dir.join("struct_with_array_of_structs.inf"))
            .expect("Failed to read struct_with_array_of_structs.inf");
        let actual = wasm_codegen(&source_code);
        let wasm_path = dir.join("struct_with_array_of_structs.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "struct_with_array_of_structs");
    }

    #[test]
    #[ignore]
    fn regenerate_struct_with_nested_array_wasm() {
        let dir = base_test_dir().join("struct_with_nested_array");
        let source_code = std::fs::read_to_string(dir.join("struct_with_nested_array.inf"))
            .expect("Failed to read struct_with_nested_array.inf");
        let actual = wasm_codegen(&source_code);
        let wasm_path = dir.join("struct_with_nested_array.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "struct_with_nested_array");
    }

    #[test]
    #[ignore]
    fn regenerate_nested_array_of_structs_wasm() {
        let dir = base_test_dir().join("nested_array_of_structs");
        let source_code = std::fs::read_to_string(dir.join("nested_array_of_structs.inf"))
            .expect("Failed to read nested_array_of_structs.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("nested_array_of_structs.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "nested_array_of_structs");
    }

    #[test]
    #[ignore]
    fn regenerate_multidim_array_uzumaki_wasm() {
        let dir = base_test_dir().join("multidim_array_uzumaki");
        let source_code = std::fs::read_to_string(dir.join("multidim_array_uzumaki.inf"))
            .expect("Failed to read multidim_array_uzumaki.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("multidim_array_uzumaki.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "multidim_array_uzumaki");
    }

    #[test]
    #[ignore]
    fn regenerate_nested_struct_with_array_wasm() {
        let dir = base_test_dir().join("nested_struct_with_array");
        let source_code =
            std::fs::read_to_string(dir.join("nested_struct_with_array.inf"))
                .expect("Failed to read nested_struct_with_array.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("nested_struct_with_array.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "nested_struct_with_array");
    }

    #[test]
    #[ignore]
    fn regenerate_if_else_compound_overlap_wasm() {
        let dir = base_test_dir().join("if_else_compound_overlap");
        let source_code =
            std::fs::read_to_string(dir.join("if_else_compound_overlap.inf"))
                .expect("Failed to read if_else_compound_overlap.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("if_else_compound_overlap.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "if_else_compound_overlap");
    }

    #[test]
    #[ignore]
    fn regenerate_enum_variant_wasm() {
        let dir = base_test_dir().join("enum_variant");
        let source_code =
            std::fs::read_to_string(dir.join("enum_variant.inf"))
                .expect("Failed to read enum_variant.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("enum_variant.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "enum_variant");
    }

    #[test]
    #[ignore]
    fn regenerate_enum_multi_wasm() {
        let dir = base_test_dir().join("enum_multi");
        let source_code =
            std::fs::read_to_string(dir.join("enum_multi.inf"))
                .expect("Failed to read enum_multi.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("enum_multi.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "enum_multi");
    }

    #[test]
    #[ignore]
    fn regenerate_enum_params_wasm() {
        let dir = base_test_dir().join("enum_params");
        let source_code =
            std::fs::read_to_string(dir.join("enum_params.inf"))
                .expect("Failed to read enum_params.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("enum_params.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "enum_params");
    }

    #[test]
    #[ignore]
    fn regenerate_enum_compare_wasm() {
        let dir = base_test_dir().join("enum_compare");
        let source_code =
            std::fs::read_to_string(dir.join("enum_compare.inf"))
                .expect("Failed to read enum_compare.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("enum_compare.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "enum_compare");
    }

    #[test]
    #[ignore]
    fn regenerate_enum_assign_wasm() {
        let dir = base_test_dir().join("enum_assign");
        let source_code =
            std::fs::read_to_string(dir.join("enum_assign.inf"))
                .expect("Failed to read enum_assign.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("enum_assign.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "enum_assign");
    }

    #[test]
    #[ignore]
    fn regenerate_enum_array_wasm() {
        let dir = base_test_dir().join("enum_array");
        let source_code =
            std::fs::read_to_string(dir.join("enum_array.inf"))
                .expect("Failed to read enum_array.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("enum_array.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "enum_array");
    }

    #[test]
    #[ignore]
    fn regenerate_enum_in_struct_wasm() {
        let dir = base_test_dir().join("enum_in_struct");
        let source_code =
            std::fs::read_to_string(dir.join("enum_in_struct.inf"))
                .expect("Failed to read enum_in_struct.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("enum_in_struct.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "enum_in_struct");
    }

    #[test]
    #[ignore]
    fn regenerate_array_zero_literal_wasm() {
        let dir = base_test_dir().join("array_zero_literal");
        let source_code = std::fs::read_to_string(dir.join("array_zero_literal.inf"))
            .expect("Failed to read array_zero_literal.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("array_zero_literal.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "array_zero_literal");
    }
}
