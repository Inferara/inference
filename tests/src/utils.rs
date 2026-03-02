use inference_ast::{
    arena::AstArena,
    builder::Builder,
    ids::{BlockId, DefId, ExprId, StmtId},
    nodes::{
        ArgKind, Def, Expr, OperatorKind, SimpleTypeKind, Stmt, TypeNode, UnaryOperatorKind,
    },
};

pub(crate) fn get_test_data_path() -> std::path::PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap());
    manifest_dir.join("test_data")
}

pub(crate) fn build_ast(source_code: String) -> AstArena {
    try_build_ast(source_code)
        .expect("Failed to build AST - check for syntax errors in the test source")
}

pub(crate) fn try_build_ast(source_code: String) -> anyhow::Result<AstArena> {
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

pub(crate) fn parse_simple_type(type_name: &str) -> Option<SimpleTypeKind> {
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

/// Helper to collect all function DefIds from an arena.
pub(crate) fn collect_function_def_ids(arena: &AstArena) -> Vec<DefId> {
    arena.function_def_ids()
}

/// Helper to find a function DefId by name.
pub(crate) fn find_function_by_name<'a>(arena: &'a AstArena, name: &str) -> Option<DefId> {
    for sf in arena.source_files() {
        for &def_id in &sf.defs {
            if let Def::Function { name: name_id, .. } = &arena[def_id].kind {
                if arena[*name_id].name == name {
                    return Some(def_id);
                }
            }
        }
    }
    None
}

/// Collects all expression IDs of a given kind from a block recursively.
pub(crate) fn collect_exprs_matching(
    arena: &AstArena,
    block_id: inference_ast::ids::BlockId,
    predicate: &dyn Fn(&Expr) -> bool,
) -> Vec<ExprId> {
    let mut results = Vec::new();
    let block = &arena[block_id];
    for &stmt_id in &block.stmts {
        collect_exprs_from_stmt(arena, stmt_id, predicate, &mut results);
    }
    results
}

fn collect_exprs_from_stmt(
    arena: &AstArena,
    stmt_id: StmtId,
    predicate: &dyn Fn(&Expr) -> bool,
    results: &mut Vec<ExprId>,
) {
    match &arena[stmt_id].kind {
        Stmt::Return { expr } => collect_exprs_from_expr(arena, *expr, predicate, results),
        Stmt::Expr(expr_id) => collect_exprs_from_expr(arena, *expr_id, predicate, results),
        Stmt::Assign { left, right } => {
            collect_exprs_from_expr(arena, *left, predicate, results);
            collect_exprs_from_expr(arena, *right, predicate, results);
        }
        Stmt::VarDef { value, .. } => {
            if let Some(val) = value {
                collect_exprs_from_expr(arena, *val, predicate, results);
            }
        }
        Stmt::If {
            condition,
            then_block,
            else_block,
        } => {
            collect_exprs_from_expr(arena, *condition, predicate, results);
            let then_exprs = collect_exprs_matching(arena, *then_block, predicate);
            results.extend(then_exprs);
            if let Some(else_id) = else_block {
                let else_exprs = collect_exprs_matching(arena, *else_id, predicate);
                results.extend(else_exprs);
            }
        }
        Stmt::Loop { condition, body } => {
            if let Some(cond) = condition {
                collect_exprs_from_expr(arena, *cond, predicate, results);
            }
            let body_exprs = collect_exprs_matching(arena, *body, predicate);
            results.extend(body_exprs);
        }
        Stmt::Block(block_id) => {
            let inner_exprs = collect_exprs_matching(arena, *block_id, predicate);
            results.extend(inner_exprs);
        }
        Stmt::Assert { expr } => collect_exprs_from_expr(arena, *expr, predicate, results),
        Stmt::Break | Stmt::TypeDef { .. } | Stmt::ConstDef(_) => {}
    }
}

fn collect_exprs_from_expr(
    arena: &AstArena,
    expr_id: ExprId,
    predicate: &dyn Fn(&Expr) -> bool,
    results: &mut Vec<ExprId>,
) {
    let expr = &arena[expr_id].kind;
    if predicate(expr) {
        results.push(expr_id);
    }
    match expr {
        Expr::Binary { left, right, .. } => {
            collect_exprs_from_expr(arena, *left, predicate, results);
            collect_exprs_from_expr(arena, *right, predicate, results);
        }
        Expr::PrefixUnary { expr, .. } => {
            collect_exprs_from_expr(arena, *expr, predicate, results);
        }
        Expr::Parenthesized { expr } => {
            collect_exprs_from_expr(arena, *expr, predicate, results);
        }
        Expr::FunctionCall { function, args, .. } => {
            collect_exprs_from_expr(arena, *function, predicate, results);
            for (_, arg_expr) in args {
                collect_exprs_from_expr(arena, *arg_expr, predicate, results);
            }
        }
        Expr::ArrayIndexAccess { array, index } => {
            collect_exprs_from_expr(arena, *array, predicate, results);
            collect_exprs_from_expr(arena, *index, predicate, results);
        }
        Expr::MemberAccess { expr, .. } | Expr::TypeMemberAccess { expr, .. } => {
            collect_exprs_from_expr(arena, *expr, predicate, results);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, field_expr) in fields {
                collect_exprs_from_expr(arena, *field_expr, predicate, results);
            }
        }
        Expr::ArrayLiteral { elements } => {
            for &elem in elements {
                collect_exprs_from_expr(arena, elem, predicate, results);
            }
        }
        Expr::Identifier(_)
        | Expr::NumberLiteral { .. }
        | Expr::BoolLiteral { .. }
        | Expr::StringLiteral { .. }
        | Expr::UnitLiteral
        | Expr::Uzumaki
        | Expr::Type(_) => {}
    }
}

/// Asserts that a single binary expression with the expected operator exists in the function body.
pub(crate) fn assert_single_binary_op(arena: &AstArena, expected: OperatorKind) {
    let func_ids = arena.function_def_ids();
    let mut binary_count = 0;
    let mut found_op = None;

    for def_id in &func_ids {
        if let Def::Function { body, .. } = &arena[*def_id].kind {
            let exprs = collect_exprs_matching(arena, *body, &|e| matches!(e, Expr::Binary { .. }));
            for expr_id in &exprs {
                if let Expr::Binary { op, .. } = &arena[*expr_id].kind {
                    binary_count += 1;
                    found_op = Some(op.clone());
                }
            }
        }
    }

    assert_eq!(
        binary_count, 1,
        "Expected 1 binary expression, found {binary_count}"
    );

    assert_eq!(
        found_op.as_ref(),
        Some(&expected),
        "Expected operator {expected:?}, found {found_op:?}"
    );
}

/// Asserts that a single prefix unary expression with the expected operator exists in the AST.
pub(crate) fn assert_single_unary_op(arena: &AstArena, expected: UnaryOperatorKind) {
    let func_ids = arena.function_def_ids();
    let mut unary_count = 0;
    let mut found_op = None;

    for def_id in &func_ids {
        if let Def::Function { body, .. } = &arena[*def_id].kind {
            let exprs = collect_exprs_matching(arena, *body, &|e| {
                matches!(e, Expr::PrefixUnary { .. })
            });
            for expr_id in &exprs {
                if let Expr::PrefixUnary { op, .. } = &arena[*expr_id].kind {
                    unary_count += 1;
                    found_op = Some(op.clone());
                }
            }
        }
    }

    assert_eq!(
        unary_count, 1,
        "Expected 1 prefix unary expression, found {unary_count}"
    );

    assert_eq!(
        found_op.as_ref(),
        Some(&expected),
        "Expected operator {expected:?}, found {found_op:?}"
    );
}

/// Asserts function signature properties.
pub(crate) fn assert_function_signature(
    arena: &AstArena,
    name: &str,
    param_count: Option<usize>,
    has_return: bool,
) {
    let def_id = find_function_by_name(arena, name)
        .unwrap_or_else(|| panic!("Expected function named '{name}'"));

    if let Def::Function {
        args, returns, ..
    } = &arena[def_id].kind
    {
        if let Some(expected_count) = param_count {
            assert_eq!(
                args.len(),
                expected_count,
                "Function '{name}' expected {expected_count} parameters, found {}",
                args.len()
            );
        }

        let has_ret = returns.map_or(false, |ty_id| !arena[ty_id].kind.is_unit_type());
        assert_eq!(
            has_ret || returns.is_some() && !has_return,
            has_return || returns.is_some() && !has_return,
            "Function '{name}' return type mismatch"
        );
        // Simpler check: returns.is_some() is what the old code tested
        assert_eq!(
            returns.is_some(),
            has_return,
            "Function '{}' return type: expected {}, found {}",
            name,
            if has_return { "present" } else { "absent" },
            if returns.is_some() { "present" } else { "absent" }
        );
    } else {
        panic!("DefId does not point to a function");
    }
}

/// Asserts that a single constant definition with expected name exists.
pub(crate) fn assert_constant_def(arena: &AstArena, name: &str) {
    let found = arena.source_files().iter().any(|sf| {
        sf.defs.iter().any(|&def_id| {
            if let Def::Constant {
                name: name_id, ..
            } = &arena[def_id].kind
            {
                arena[*name_id].name == name
            } else {
                false
            }
        })
    });
    assert!(found, "Expected constant named '{name}'");
}

/// Asserts that a single variable definition with expected name exists.
pub(crate) fn assert_variable_def(arena: &AstArena, name: &str) {
    let func_ids = arena.function_def_ids();
    let found = func_ids.iter().any(|&def_id| {
        if let Def::Function { body, .. } = &arena[def_id].kind {
            block_contains_var_def(arena, *body, name)
        } else {
            false
        }
    });
    assert!(found, "Expected variable named '{name}'");
}

fn block_contains_var_def(
    arena: &AstArena,
    block_id: inference_ast::ids::BlockId,
    name: &str,
) -> bool {
    let block = &arena[block_id];
    for &stmt_id in &block.stmts {
        match &arena[stmt_id].kind {
            Stmt::VarDef {
                name: name_id, ..
            } => {
                if arena[*name_id].name == name {
                    return true;
                }
            }
            Stmt::Block(inner) => {
                if block_contains_var_def(arena, *inner, name) {
                    return true;
                }
            }
            Stmt::If {
                then_block,
                else_block,
                ..
            } => {
                if block_contains_var_def(arena, *then_block, name) {
                    return true;
                }
                if let Some(else_id) = else_block {
                    if block_contains_var_def(arena, *else_id, name) {
                        return true;
                    }
                }
            }
            Stmt::Loop { body, .. } => {
                if block_contains_var_def(arena, *body, name) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Asserts that a struct definition with expected name and field count exists.
pub(crate) fn assert_struct_def(arena: &AstArena, name: &str, field_count: Option<usize>) {
    let mut found = false;
    for sf in arena.source_files() {
        for &def_id in &sf.defs {
            if let Def::Struct {
                name: name_id,
                fields,
                ..
            } = &arena[def_id].kind
            {
                if arena[*name_id].name == name {
                    found = true;
                    if let Some(expected_count) = field_count {
                        assert_eq!(
                            fields.len(),
                            expected_count,
                            "Struct '{name}' expected {expected_count} fields, found {}",
                            fields.len()
                        );
                    }
                }
            }
        }
    }
    assert!(found, "Expected struct named '{name}'");
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
pub(crate) fn assert_enum_def(arena: &AstArena, name: &str, variant_count: Option<usize>) {
    let mut found = false;
    for sf in arena.source_files() {
        for &def_id in &sf.defs {
            if let Def::Enum {
                name: name_id,
                variants,
                ..
            } = &arena[def_id].kind
            {
                if arena[*name_id].name == name {
                    found = true;
                    if let Some(expected_count) = variant_count {
                        assert_eq!(
                            variants.len(),
                            expected_count,
                            "Enum '{name}' expected {expected_count} variants, found {}",
                            variants.len()
                        );
                    }
                }
            }
        }
    }
    assert!(found, "Expected enum named '{name}'");
}

/// Collects all expression IDs matching a predicate across ALL function bodies in the arena.
pub(crate) fn collect_all_exprs(
    arena: &AstArena,
    predicate: &dyn Fn(&Expr) -> bool,
) -> Vec<ExprId> {
    let mut results = Vec::new();
    for sf in arena.source_files() {
        for &def_id in &sf.defs {
            collect_exprs_from_def(arena, def_id, predicate, &mut results);
        }
    }
    results
}

fn collect_exprs_from_def(
    arena: &AstArena,
    def_id: DefId,
    predicate: &dyn Fn(&Expr) -> bool,
    results: &mut Vec<ExprId>,
) {
    match &arena[def_id].kind {
        Def::Function { body, .. } => {
            let exprs = collect_exprs_matching(arena, *body, predicate);
            results.extend(exprs);
        }
        Def::Struct { methods, .. } => {
            for &method_id in methods {
                collect_exprs_from_def(arena, method_id, predicate, results);
            }
        }
        Def::Spec { defs, .. } => {
            for &inner_def in defs {
                collect_exprs_from_def(arena, inner_def, predicate, results);
            }
        }
        _ => {}
    }
}

/// Collects all statement IDs matching a predicate across ALL function bodies in the arena.
pub(crate) fn collect_all_stmts(
    arena: &AstArena,
    predicate: &dyn Fn(&Stmt) -> bool,
) -> Vec<StmtId> {
    let mut results = Vec::new();
    for sf in arena.source_files() {
        for &def_id in &sf.defs {
            collect_stmts_from_def(arena, def_id, predicate, &mut results);
        }
    }
    results
}

fn collect_stmts_from_def(
    arena: &AstArena,
    def_id: DefId,
    predicate: &dyn Fn(&Stmt) -> bool,
    results: &mut Vec<StmtId>,
) {
    match &arena[def_id].kind {
        Def::Function { body, .. } => {
            collect_stmts_from_block(arena, *body, predicate, results);
        }
        Def::Struct { methods, .. } => {
            for &method_id in methods {
                collect_stmts_from_def(arena, method_id, predicate, results);
            }
        }
        Def::Spec { defs, .. } => {
            for &inner_def in defs {
                collect_stmts_from_def(arena, inner_def, predicate, results);
            }
        }
        _ => {}
    }
}

fn collect_stmts_from_block(
    arena: &AstArena,
    block_id: BlockId,
    predicate: &dyn Fn(&Stmt) -> bool,
    results: &mut Vec<StmtId>,
) {
    let block = &arena[block_id];
    for &stmt_id in &block.stmts {
        let stmt = &arena[stmt_id].kind;
        if predicate(stmt) {
            results.push(stmt_id);
        }
        match stmt {
            Stmt::Block(inner) => collect_stmts_from_block(arena, *inner, predicate, results),
            Stmt::If {
                then_block,
                else_block,
                ..
            } => {
                collect_stmts_from_block(arena, *then_block, predicate, results);
                if let Some(else_id) = else_block {
                    collect_stmts_from_block(arena, *else_id, predicate, results);
                }
            }
            Stmt::Loop { body, .. } => {
                collect_stmts_from_block(arena, *body, predicate, results);
            }
            _ => {}
        }
    }
}

/// Asserts that a function return type is a specific simple type.
pub(crate) fn assert_function_returns_simple_type(
    arena: &AstArena,
    func_name: &str,
    expected_type: SimpleTypeKind,
) {
    let def_id = find_function_by_name(arena, func_name)
        .unwrap_or_else(|| panic!("Expected function named '{func_name}'"));

    if let Def::Function { returns, .. } = &arena[def_id].kind {
        let returns = returns.unwrap_or_else(|| {
            panic!("Function '{func_name}' should have a return type");
        });
        if let TypeNode::Simple(kind) = &arena[returns].kind {
            assert_eq!(
                *kind, expected_type,
                "Function '{func_name}' expected return type {expected_type:?}, found {kind:?}"
            );
        } else {
            panic!(
                "Function '{func_name}' expected simple return type {expected_type:?}, but found {:?}",
                arena[returns].kind
            );
        }
    } else {
        panic!("DefId does not point to a function");
    }
}
