#[cfg(test)]
mod base_codegen_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, get_test_file_path, get_test_wasm_path, wasm_codegen,
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
    }

    #[test]
    fn local_variables_test() {
        cov_mark::check_count!(wasm_codegen_emit_variable_definition, 12);
        cov_mark::check_count!(wasm_codegen_variable_definition_uzumaki_i32, 1);
        cov_mark::check_count!(wasm_codegen_variable_definition_uzumaki_i64, 1);
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
            i64::MAX
        );

        let f: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "let_i8_literal")
            .unwrap_or_else(|e| panic!("Failed to get 'let_i8_literal': {e}"));
        assert_eq!(
            f.call(&mut store, ())
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
            127_i32
        );

        let f: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "let_i16_literal")
            .unwrap_or_else(|e| panic!("Failed to get 'let_i16_literal': {e}"));
        assert_eq!(
            f.call(&mut store, ())
                .unwrap_or_else(|e| panic!("Call failed: {e}")),
            32767_i32
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
    use crate::utils::{get_test_data_path, wasm_codegen};

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
        let dir = base_test_dir();
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
    }

    #[test]
    #[ignore]
    fn regenerate_const_wasm() {
        let dir = base_test_dir();
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
    }

    #[test]
    #[ignore]
    fn regenerate_nondet_wasm() {
        let dir = base_test_dir();
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
    }

    #[test]
    #[ignore]
    fn regenerate_i64_uzumaki_wasm() {
        let dir = base_test_dir();
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
    }

    #[test]
    #[ignore]
    fn regenerate_bool_literal_wasm() {
        let dir = base_test_dir();
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
    }

    #[test]
    #[ignore]
    fn regenerate_mixed_visibility_wasm() {
        let dir = base_test_dir();
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
    }

    #[test]
    #[ignore]
    fn regenerate_bool_const_wasm() {
        let dir = base_test_dir();
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
    }

    #[test]
    #[ignore]
    fn regenerate_numeric_literals_wasm() {
        let dir = base_test_dir();
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
    }

    #[test]
    #[ignore]
    fn regenerate_local_variables_wasm() {
        let dir = base_test_dir();
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
    }
}
