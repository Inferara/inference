use inference_ast::{
    arena::Arena,
    builder::Builder,
    nodes::{AstNode, Definition, Expression, OperatorKind, Statement, Type, UnaryOperatorKind},
};

pub(crate) fn get_test_data_path() -> std::path::PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap());
    manifest_dir.join("test_data")
}

pub(crate) fn build_ast(source_code: String) -> Arena {
    try_build_ast(source_code)
        .expect("Failed to build AST - check for syntax errors in the test source")
}

pub(crate) fn try_build_ast(source_code: String) -> anyhow::Result<Arena> {
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
    builder.build_ast()
}

/// Generates codegen output from source code using the default target (`Wasm32`) and mode (`Compile`).
///
/// Returns the [`CodegenOutput`] containing WASM bytes and metadata.
pub(crate) fn codegen_output(source_code: &str) -> inference_wasm_codegen::CodegenOutput {
    let arena = build_ast(source_code.to_string());
    let typed_context = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
        .unwrap()
        .typed_context();
    let target = inference_wasm_codegen::Target::default();
    let mode = inference_wasm_codegen::CompilationMode::default();
    inference_wasm_codegen::codegen(
        &typed_context,
        target,
        mode,
        target.default_opt_level(),
    )
    .unwrap()
}

/// Generates codegen output from source code with an explicit compilation mode.
///
/// Uses `Wasm32` target with the specified mode.
pub(crate) fn codegen_output_with_mode(
    source_code: &str,
    mode: inference_wasm_codegen::CompilationMode,
) -> inference_wasm_codegen::CodegenOutput {
    let target = inference_wasm_codegen::Target::Wasm32;
    codegen_with_target_mode(source_code, target, mode).unwrap()
}

/// Generates codegen output from source code with explicit target and mode.
///
/// Returns `Result` to allow testing error cases (e.g., proof + Soroban rejection).
pub(crate) fn codegen_with_target_mode(
    source_code: &str,
    target: inference_wasm_codegen::Target,
    mode: inference_wasm_codegen::CompilationMode,
) -> anyhow::Result<inference_wasm_codegen::CodegenOutput> {
    let arena = build_ast(source_code.to_string());
    let typed_context = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
        .unwrap()
        .typed_context();
    let opt_level = target.default_opt_level();
    inference_wasm_codegen::codegen(&typed_context, target, mode, opt_level)
}

/// Generates codegen output from source code with explicit target, mode, and optimization level.
///
/// Returns `Result` to allow testing error cases and specific opt_level configurations.
pub(crate) fn codegen_with_full_config(
    source_code: &str,
    target: inference_wasm_codegen::Target,
    mode: inference_wasm_codegen::CompilationMode,
    opt_level: inference_wasm_codegen::OptLevel,
) -> anyhow::Result<inference_wasm_codegen::CodegenOutput> {
    let arena = build_ast(source_code.to_string());
    let typed_context = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
        .unwrap()
        .typed_context();
    inference_wasm_codegen::codegen(&typed_context, target, mode, opt_level)
}

/// Generates WebAssembly bytes from source code for a specific target.
///
/// Uses the specified target with default compilation mode (`Compile`).
pub(crate) fn wasm_codegen_with_target(
    source_code: &str,
    target: inference_wasm_codegen::Target,
) -> Vec<u8> {
    let arena = build_ast(source_code.to_string());
    let typed_context = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
        .unwrap()
        .typed_context();
    let mode = inference_wasm_codegen::CompilationMode::default();
    let opt_level = target.default_opt_level();
    let codegen_output =
        inference_wasm_codegen::codegen(&typed_context, target, mode, opt_level).unwrap();
    codegen_output.wasm().to_vec()
}

/// Generates WebAssembly bytes from source code using the default target (`Wasm32`) and mode (`Compile`).
///
/// Returns the WASM binary as bytes. The codegen produces WASM directly in-process
/// via `wasm-encoder`, no external binaries are needed.
pub(crate) fn wasm_codegen(source_code: &str) -> Vec<u8> {
    let output = codegen_output(source_code);
    output.wasm().to_vec()
}

/// Build the test directory path. When the last module path component equals test_name,
/// the file lives directly in that directory (no extra subdirectory).
///
/// # Examples
/// - module `codegen::wasm::base` + test_name `"trivial"` -> `.../base/trivial/`
/// - module `codegen::wasm::binops_bool` + test_name `"binops_bool"` -> `.../binops_bool/`
fn get_test_dir(module_path: &str, test_name: &str) -> std::path::PathBuf {
    let path_parts = get_test_path_parts(module_path);

    let mut path = get_test_data_path();
    for part in &path_parts {
        path = path.join(part);
    }

    if path_parts.last() != Some(&test_name) {
        path = path.join(test_name);
    }

    path
}

/// Automatically resolves a test data file path based on the test's module path and name.
///
/// When the last module path component equals the test name, no extra subdirectory is added.
///
/// # Examples
/// - `tests/src/codegen/wasm/base.rs::trivial_test`
///   -> `tests/test_data/codegen/wasm/base/trivial/trivial.inf`
/// - `tests/src/codegen/wasm/binops_bool.rs::binops_bool_test`
///   -> `tests/test_data/codegen/wasm/binops_bool/binops_bool.inf`
///
/// # Arguments
/// * `module_path` - The module path (use `module_path!()`)
/// * `test_name` - The test function name (without `_test` suffix)
pub(crate) fn get_test_file_path(module_path: &str, test_name: &str) -> std::path::PathBuf {
    get_test_dir(module_path, test_name).join(format!("{test_name}.inf"))
}

/// Automatically resolves a WASM test data file path based on the test's module path and name.
///
/// When the last module path component equals the test name, no extra subdirectory is added.
///
/// # Examples
/// - `tests/src/codegen/wasm/base.rs::trivial_test`
///   -> `tests/test_data/codegen/wasm/base/trivial/trivial.wasm`
/// - `tests/src/codegen/wasm/binops_bool.rs::binops_bool_test`
///   -> `tests/test_data/codegen/wasm/binops_bool/binops_bool.wasm`
///
/// # Arguments
/// * `module_path` - The module path (use `module_path!()`)
/// * `test_name` - The test function name (without `_test` suffix)
pub(crate) fn get_test_wasm_path(module_path: &str, test_name: &str) -> std::path::PathBuf {
    get_test_dir(module_path, test_name).join(format!("{test_name}.wasm"))
}

/// Automatically resolves a WAT test data file path based on the test's module path and name.
///
/// When the last module path component equals the test name, no extra subdirectory is added.
///
/// # Examples
/// - `tests/src/codegen/wasm/base.rs::trivial_test`
///   -> `tests/test_data/codegen/wasm/base/trivial/trivial.wat`
/// - `tests/src/codegen/wasm/binops_bool.rs::binops_bool_test`
///   -> `tests/test_data/codegen/wasm/binops_bool/binops_bool.wat`
///
/// # Arguments
/// * `module_path` - The module path (use `module_path!()`)
/// * `test_name` - The test function name (without `_test` suffix)
pub(crate) fn get_test_wat_path(module_path: &str, test_name: &str) -> std::path::PathBuf {
    get_test_dir(module_path, test_name).join(format!("{test_name}.wat"))
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

pub(crate) fn parse_simple_type(type_name: &str) -> Option<inference_ast::nodes::SimpleTypeKind> {
    use inference_ast::nodes::SimpleTypeKind;

    match type_name {
        "unit" => Some(SimpleTypeKind::Unit),
        "bool" => Some(SimpleTypeKind::Bool),
        "i8" => Some(SimpleTypeKind::I8),
        "i16" => Some(SimpleTypeKind::I16),
        "i32" => Some(SimpleTypeKind::I32),
        "i64" => Some(SimpleTypeKind::I64),
        "u8" => Some(SimpleTypeKind::U8),
        "u16" => Some(SimpleTypeKind::U16),
        "u32" => Some(SimpleTypeKind::U32),
        "u64" => Some(SimpleTypeKind::U64),
        _ => None,
    }
}

/// Asserts that a single binary expression with the expected operator exists in the AST.
///
/// Filters all binary expressions from the arena and verifies:
/// - Exactly one binary expression is found
/// - The operator matches the expected kind
///
/// # Panics
/// Panics if no binary expression is found or if the operator doesn't match.
pub(crate) fn assert_single_binary_op(arena: &Arena, expected: OperatorKind) {
    let binary_exprs =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Binary(_))));

    assert_eq!(
        binary_exprs.len(),
        1,
        "Expected 1 binary expression, found {}",
        binary_exprs.len()
    );

    if let AstNode::Expression(Expression::Binary(bin_expr)) = &binary_exprs[0] {
        assert_eq!(
            bin_expr.operator, expected,
            "Expected operator {:?}, found {:?}",
            expected, bin_expr.operator
        );
    } else {
        panic!("Expected binary expression");
    }
}

/// Asserts that a single prefix unary expression with the expected operator exists in the AST.
///
/// # Panics
/// Panics if no unary expression is found or if the operator doesn't match.
pub(crate) fn assert_single_unary_op(arena: &Arena, expected: UnaryOperatorKind) {
    let prefix_exprs =
        arena.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::PrefixUnary(_))));

    assert_eq!(
        prefix_exprs.len(),
        1,
        "Expected 1 prefix unary expression, found {}",
        prefix_exprs.len()
    );

    if let AstNode::Expression(Expression::PrefixUnary(unary_expr)) = &prefix_exprs[0] {
        assert_eq!(
            unary_expr.operator, expected,
            "Expected operator {:?}, found {:?}",
            expected, unary_expr.operator
        );
    } else {
        panic!("Expected prefix unary expression");
    }
}

/// Asserts function signature properties.
///
/// Verifies:
/// - Function name matches expected
/// - Parameter count matches (if `param_count` is provided)
/// - Return type presence matches `has_return`
///
/// # Panics
/// Panics if no function is found or if the signature doesn't match expectations.
pub(crate) fn assert_function_signature(
    arena: &Arena,
    name: &str,
    param_count: Option<usize>,
    has_return: bool,
) {
    let functions = arena.functions();
    assert!(!functions.is_empty(), "Expected at least 1 function");

    let func = functions.iter().find(|f| f.name.name == name);
    let func = func.unwrap_or_else(|| panic!("Expected function named '{name}'"));

    if let Some(expected_count) = param_count {
        let actual_count = func.arguments.as_ref().map_or(0, Vec::len);
        assert_eq!(
            actual_count, expected_count,
            "Function '{}' expected {} parameters, found {}",
            name, expected_count, actual_count
        );
    }

    assert_eq!(
        func.returns.is_some(),
        has_return,
        "Function '{}' return type: expected {}, found {}",
        name,
        if has_return { "present" } else { "absent" },
        if func.returns.is_some() {
            "present"
        } else {
            "absent"
        }
    );
}

/// Asserts that a single constant definition with expected name exists.
///
/// # Panics
/// Panics if no constant with the expected name is found.
pub(crate) fn assert_constant_def(arena: &Arena, name: &str) {
    let const_defs =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Constant(_))));

    assert!(
        !const_defs.is_empty(),
        "Expected at least 1 constant definition"
    );

    let found = const_defs.iter().any(|node| {
        if let AstNode::Definition(Definition::Constant(c)) = node {
            c.name.name == name
        } else {
            false
        }
    });

    assert!(found, "Expected constant named '{name}'");
}

/// Asserts that a single variable definition with expected name exists.
///
/// # Panics
/// Panics if no variable definition with the expected name is found.
pub(crate) fn assert_variable_def(arena: &Arena, name: &str) {
    let var_defs = arena
        .filter_nodes(|node| matches!(node, AstNode::Statement(Statement::VariableDefinition(_))));

    assert!(
        !var_defs.is_empty(),
        "Expected at least 1 variable definition"
    );

    let found = var_defs.iter().any(|node| {
        if let AstNode::Statement(Statement::VariableDefinition(v)) = node {
            v.name.name == name
        } else {
            false
        }
    });

    assert!(found, "Expected variable named '{name}'");
}

/// Asserts that a struct definition with expected name and field count exists.
///
/// # Panics
/// Panics if no struct with the expected name is found.
pub(crate) fn assert_struct_def(arena: &Arena, name: &str, field_count: Option<usize>) {
    let structs =
        arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Struct(_))));

    assert!(!structs.is_empty(), "Expected at least 1 struct definition");

    let struct_def = structs.iter().find_map(|node| {
        if let AstNode::Definition(Definition::Struct(s)) = node
            && s.name.name == name
        {
            return Some(s);
        }
        None
    });

    let struct_def = struct_def.unwrap_or_else(|| panic!("Expected struct named '{name}'"));

    if let Some(expected_count) = field_count {
        assert_eq!(
            struct_def.fields.len(),
            expected_count,
            "Struct '{}' expected {} fields, found {}",
            name,
            expected_count,
            struct_def.fields.len()
        );
    }
}

/// Attempt to generate WAT from WASM bytes and write to disk.
///
/// Silently skips if `wasmprinter` fails (e.g. custom non-det opcodes).
pub(crate) fn regenerate_wat(wasm_bytes: &[u8], dir: &std::path::Path, name: &str) {
    if let Ok(wat) = wasmprinter::print_bytes(wasm_bytes) {
        let wat_path = dir.join(format!("{name}.wat"));
        std::fs::write(&wat_path, &wat)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wat_path.display()));
        println!(
            "Regenerated WAT: {} ({} bytes)",
            wat_path.display(),
            wat.len()
        );
    }
}

/// Assert WAT equivalence if a golden `.wat` file exists.
///
/// Skips silently if no `.wat` file exists (non-det modules).
pub(crate) fn assert_wat_equivalence(wasm_bytes: &[u8], module_path: &str, test_name: &str) {
    let wat_path = get_test_wat_path(module_path, test_name);
    if wat_path.exists() {
        let expected = std::fs::read_to_string(&wat_path)
            .unwrap_or_else(|_| panic!("Failed to read WAT file: {wat_path:?}"));
        let actual = wasmprinter::print_bytes(wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to print WAT for {test_name}: {e}"));
        let expected = expected.replace("\r\n", "\n");
        let actual = actual.replace("\r\n", "\n");
        assert_eq!(expected, actual, "WAT mismatch for {test_name}");
    }
}

/// Asserts that an enum definition with expected name and variant count exists.
///
/// # Panics
/// Panics if no enum with the expected name is found.
pub(crate) fn assert_enum_def(arena: &Arena, name: &str, variant_count: Option<usize>) {
    let enums = arena.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Enum(_))));

    assert!(!enums.is_empty(), "Expected at least 1 enum definition");

    let enum_def = enums.iter().find_map(|node| {
        if let AstNode::Definition(Definition::Enum(e)) = node
            && e.name.name == name
        {
            return Some(e);
        }
        None
    });

    let enum_def = enum_def.unwrap_or_else(|| panic!("Expected enum named '{name}'"));

    if let Some(expected_count) = variant_count {
        assert_eq!(
            enum_def.variants.len(),
            expected_count,
            "Enum '{}' expected {} variants, found {}",
            name,
            expected_count,
            enum_def.variants.len()
        );
    }
}

/// Asserts that a function return type is a specific simple type.
///
/// # Panics
/// Panics if the function is not found or doesn't have the expected return type.
pub(crate) fn assert_function_returns_simple_type(
    arena: &Arena,
    func_name: &str,
    expected_type: inference_ast::nodes::SimpleTypeKind,
) {
    let functions = arena.functions();
    let func = functions
        .iter()
        .find(|f| f.name.name == func_name)
        .unwrap_or_else(|| panic!("Expected function named '{func_name}'"));

    if let Some(Type::Simple(kind)) = &func.returns {
        assert_eq!(
            *kind, expected_type,
            "Function '{}' expected return type {:?}, found {:?}",
            func_name, expected_type, kind
        );
    } else {
        panic!(
            "Function '{}' expected simple return type {:?}, but found {:?}",
            func_name, expected_type, func.returns
        );
    }
}
