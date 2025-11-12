use inference_ast::{builder::Builder, t_ast::TypedAst};

pub(crate) fn get_test_data_path() -> std::path::PathBuf {
    let current_dir = std::env::current_dir().unwrap();
    current_dir
        .parent() // inference
        .unwrap()
        .join("test_data")
}

pub(crate) fn build_ast(source_code: String) -> TypedAst {
    let inference_language = tree_sitter_inference::language();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&inference_language)
        .expect("Error loading Inference grammar");
    let tree = parser.parse(source_code.clone(), None).unwrap();
    let code = source_code.as_bytes();
    let root_node = tree.root_node();
    let mut builder = Builder::new();
    builder.add_source_code(root_node, code);
    let builder = builder.build_ast().unwrap();
    builder.t_ast()
}

pub(crate) fn wasm_codegen(source_code: &str) -> Vec<u8> {
    let ast = build_ast(source_code.to_string());
    inference_wasm_codegen::codegen(&ast).unwrap()
}

/// Automatically resolves a test data file path based on the test's module path and name.
///
/// # Example
/// For a test at `tests/src/codegen/wasm/base.rs::trivial_test`,
/// this will resolve to `test_data/codegen/wasm/base/trivial.inf`
///
/// # Arguments
/// * `module_path` - The module path (use `module_path!()`)
/// * `test_name` - The test function name (without `_test` suffix)
pub(crate) fn get_test_file_path(module_path: &str, test_name: &str) -> std::path::PathBuf {
    let path_parts = get_test_path_parts(module_path);

    let mut path = get_test_data_path();
    for part in path_parts {
        path = path.join(part);
    }

    path.join(format!("{test_name}.inf"))
}

/// Automatically resolves a WASM test data file path based on the test's module path and name.
///
/// # Example
/// For a test at `tests/src/codegen/wasm/base.rs::trivial_test`,
/// this will resolve to `test_data/codegen/wasm/base/trivial.wasm`
///
/// # Arguments
/// * `module_path` - The module path (use `module_path!()`)
/// * `test_name` - The test function name (without `_test` suffix)
pub(crate) fn get_test_wasm_path(module_path: &str, test_name: &str) -> std::path::PathBuf {
    let path_parts = get_test_path_parts(module_path);

    let mut path = get_test_data_path();
    for part in path_parts {
        path = path.join(part);
    }

    path.join(format!("{test_name}.wasm"))
}

fn get_test_path_parts(module_path: &str) -> Vec<&str> {
    let parts: Vec<&str> = module_path.split("::").collect();

    let path_parts = parts
        .iter()
        .skip(1) // skip "tests"
        .filter(|p| !p.ends_with("_tests")) // skip test module names
        .copied()
        .collect();

    path_parts
}

pub(crate) fn assert_wasms_modules_equivalence(expected: &[u8], actual: &[u8]) {
    assert_eq!(
        expected.len(),
        actual.len(),
        "WASM bytecode length mismatch"
    );
    assert_eq!(expected, actual, "WASM bytecode content mismatch");
    for (i, (exp_byte, act_byte)) in expected.iter().zip(actual.iter()).enumerate() {
        assert_eq!(
            exp_byte, act_byte,
            "WASM bytecode mismatch at byte index {i}: expected {exp_byte:02x}, got {act_byte:02x}"
        );
    }
}
