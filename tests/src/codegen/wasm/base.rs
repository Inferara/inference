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
            .unwrap_or_else(|_| panic!("Failed to read test file: {:?}", test_file_path));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected).unwrap_or_else(|_| {
            panic!("Failed to read expected wasm file for test: {}", test_name)
        });
        let actual = wasm_codegen(&source_code);
        assert_wasms_modules_equivalence(&expected, &actual);
    }
}
