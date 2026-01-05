use inference_ast::{arena::Arena, builder::Builder};

pub(crate) fn get_test_data_path() -> std::path::PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap());
    manifest_dir.join("test_data")
}

pub(crate) fn build_ast(source_code: String) -> Arena {
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
    builder.arena()
}

pub(crate) fn wasm_codegen(source_code: &str) -> Vec<u8> {
    let arena = build_ast(source_code.to_string());
    let typed_context = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
        .unwrap()
        .typed_context();
    inference_wasm_codegen::codegen(&typed_context).unwrap()
}

/// Automatically resolves a test data file path based on the test's module path and name.
///
/// # Example
/// For a test at `tests/src/codegen/wasm/base.rs::trivial_test`,
/// this will resolve to `tests/test_data/codegen/wasm/base/trivial.inf`
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
/// this will resolve to `tests/test_data/codegen/wasm/base/trivial.wasm`
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

    parts
        .iter()
        .skip(1) // skip "tests"
        .filter(|p| !p.ends_with("_tests")) // skip test module names
        .copied()
        .collect()
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
