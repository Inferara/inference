//! The forms that carry no value: a bare `return;`, a `();` statement, the two
//! spellings of a unit return type, and a body-level `type` alias.
//!
//! None of these produces a value, and the point of the family is that producing
//! nothing is a real lowering rather than a gap. A unit expression occupies no
//! operand stack slot, so `return;` is the epilogue and `return` on an empty
//! stack, and a `();` statement emits neither the value nor the `drop` a
//! value-producing statement would need. A type alias is nominal: the type
//! checker has already resolved every use of it, so the statement contributes no
//! instruction and consumes no local, which the `type_alias_between_bindings`
//! golden pins by keeping the surrounding bindings on consecutive indices.
//!
//! Each row is a golden plus an execution run, because the two catch different
//! mistakes: the golden pins which bytes are emitted, and the run pins that the
//! module a host loads still computes the value the source says it does.

#[cfg(test)]
mod void_forms_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };

    /// Reads a fixture, generates its module, and byte-compares it against the
    /// committed golden (and the `.wat` rendering when one exists).
    fn assert_golden(test_name: &str) -> Vec<u8> {
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {e}"));
        let expected_path = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected_path)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
        actual
    }

    /// Instantiates `wasm_bytes` and calls its exported `main`, asserting the
    /// result. Every fixture in this module exports `main` and takes no
    /// arguments.
    fn assert_main_returns(wasm_bytes: &[u8], expected: i32) {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let engine = Engine::default();
        let module = Module::new(&engine, wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));
        let main: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "main")
            .unwrap_or_else(|e| panic!("Failed to get 'main': {e}"));
        let result = main
            .call(&mut store, ())
            .unwrap_or_else(|e| panic!("Call to 'main' failed: {e}"));
        assert_eq!(result, expected, "'main' returned an unexpected value");
    }

    /// The one `return;` in the fixture is the only unit literal in it, so the
    /// count pins that the arm is reached rather than assumed: were the parser to
    /// stop synthesizing the literal, this row would report zero hits instead of
    /// silently testing nothing.
    #[test]
    fn explicit_void_return_test() {
        cov_mark::check_count!(wasm_codegen_emit_unit_literal, 1);
        assert_golden("explicit_void_return");
    }

    #[test]
    fn explicit_void_return_execution_test() {
        let wasm_bytes = assert_golden("explicit_void_return");
        assert_main_returns(&wasm_bytes, 7);
    }

    #[test]
    fn unit_expression_statement_test() {
        assert_golden("unit_expression_statement");
    }

    #[test]
    fn unit_expression_statement_execution_test() {
        let wasm_bytes = assert_golden("unit_expression_statement");
        assert_main_returns(&wasm_bytes, 7);
    }

    #[test]
    fn void_return_inside_if_test() {
        assert_golden("void_return_inside_if");
    }

    /// Both arms of the `if` are exercised, so the early `return;` and the
    /// fall-through `return;` each restore the frame pointer of a function that
    /// owns one. A missing epilogue on either path leaves the shadow stack
    /// unbalanced, which the second call would observe.
    #[test]
    fn void_return_inside_if_execution_test() {
        let wasm_bytes = assert_golden("void_return_inside_if");
        assert_main_returns(&wasm_bytes, 3);
    }

    #[test]
    fn unit_return_type_spelled_unit_test() {
        assert_golden("unit_return_type_spelled_unit");
    }

    #[test]
    fn unit_return_type_spelled_unit_execution_test() {
        let wasm_bytes = assert_golden("unit_return_type_spelled_unit");
        assert_main_returns(&wasm_bytes, 9);
    }

    #[test]
    fn local_type_alias_test() {
        assert_golden("local_type_alias");
    }

    #[test]
    fn local_type_alias_execution_test() {
        let wasm_bytes = assert_golden("local_type_alias");
        assert_main_returns(&wasm_bytes, 42);
    }

    #[test]
    fn type_alias_between_bindings_test() {
        assert_golden("type_alias_between_bindings");
    }

    #[test]
    fn type_alias_between_bindings_execution_test() {
        let wasm_bytes = assert_golden("type_alias_between_bindings");
        assert_main_returns(&wasm_bytes, 42);
    }
}

/// Regenerates the committed goldens from current compiler output.
///
/// ```bash
/// cargo test -p inference-tests codegen::wasm::void_forms::regenerate -- --ignored
/// ```
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn void_forms_test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("void_forms")
    }

    fn regenerate_one(test_name: &str) {
        let dir = void_forms_test_dir().join(test_name);
        let source_path = dir.join(format!("{test_name}.inf"));
        let source_code = std::fs::read_to_string(&source_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", source_path.display()));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {e}"));
        let wasm_path = dir.join(format!("{test_name}.wasm"));
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, test_name);
    }

    #[test]
    #[ignore]
    fn regenerate_explicit_void_return_wasm() {
        regenerate_one("explicit_void_return");
    }

    #[test]
    #[ignore]
    fn regenerate_unit_expression_statement_wasm() {
        regenerate_one("unit_expression_statement");
    }

    #[test]
    #[ignore]
    fn regenerate_void_return_inside_if_wasm() {
        regenerate_one("void_return_inside_if");
    }

    #[test]
    #[ignore]
    fn regenerate_unit_return_type_spelled_unit_wasm() {
        regenerate_one("unit_return_type_spelled_unit");
    }

    #[test]
    #[ignore]
    fn regenerate_local_type_alias_wasm() {
        regenerate_one("local_type_alias");
    }

    #[test]
    #[ignore]
    fn regenerate_type_alias_between_bindings_wasm() {
        regenerate_one("type_alias_between_bindings");
    }
}
