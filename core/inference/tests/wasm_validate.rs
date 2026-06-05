//! Integration tests for `external fn` validation against a real `.wasm` module
//! (`inference::wasm_link::validate`).
//!
//! Fixtures are built with `wasm-encoder` so the bytes are genuine WASM, then
//! fed through `validate_extern`. The two failure modes — missing export and
//! signature mismatch — are asserted to surface as **distinct** error variants.

use inference::wasm_link::validate::{
    lower_extern_signature, validate_extern, DeclaredSignature, ValidateError, WasmValType,
};
use inference_ast::nodes::Def;

use wasm_encoder::{
    CodeSection, ExportKind, ExportSection, EntityType, Function, FunctionSection, ImportSection,
    Instruction, Module, TypeSection, ValType,
};

/// Builds a module exporting one function `name` with the given signature.
/// The body returns zero/zeros to keep it trivially valid.
fn module_exporting(name: &str, params: &[ValType], results: &[ValType]) -> Vec<u8> {
    let mut module = Module::new();

    let mut types = TypeSection::new();
    types.ty().function(params.iter().copied(), results.iter().copied());
    module.section(&types);

    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);

    let mut exports = ExportSection::new();
    exports.export(name, ExportKind::Func, 0);
    module.section(&exports);

    let mut code = CodeSection::new();
    let mut func = Function::new([]);
    for result in results {
        match result {
            ValType::I64 => func.instruction(&Instruction::I64Const(0)),
            _ => func.instruction(&Instruction::I32Const(0)),
        };
    }
    func.instruction(&Instruction::End);
    code.function(&func);
    module.section(&code);

    module.finish()
}

/// Builds a module with one *imported* function and one local exported function,
/// so the exported function lives at function index 1 (imports occupy index 0).
/// Validation must follow the index space and read the *local* function's type.
fn module_with_import_then_export(
    export_name: &str,
    params: &[ValType],
    results: &[ValType],
) -> Vec<u8> {
    let mut module = Module::new();

    let mut types = TypeSection::new();
    // type 0: the imported function (i32) -> ()
    types.ty().function([ValType::I32], []);
    // type 1: the exported function
    types.ty().function(params.iter().copied(), results.iter().copied());
    module.section(&types);

    let mut imports = ImportSection::new();
    imports.import("host", "log", EntityType::Function(0));
    module.section(&imports);

    let mut functions = FunctionSection::new();
    functions.function(1);
    module.section(&functions);

    let mut exports = ExportSection::new();
    // imported func is index 0; the local func is index 1.
    exports.export(export_name, ExportKind::Func, 1);
    module.section(&exports);

    let mut code = CodeSection::new();
    let mut func = Function::new([]);
    for result in results {
        match result {
            ValType::I64 => func.instruction(&Instruction::I64Const(0)),
            _ => func.instruction(&Instruction::I32Const(0)),
        };
    }
    func.instruction(&Instruction::End);
    code.function(&func);
    module.section(&code);

    module.finish()
}

/// Builds a module that exports a *memory* (not a function) named `name`.
fn module_exporting_memory(name: &str) -> Vec<u8> {
    let mut module = Module::new();

    let mut memories = wasm_encoder::MemorySection::new();
    memories.memory(wasm_encoder::MemoryType {
        minimum: 1,
        maximum: Some(1),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memories);

    let mut exports = ExportSection::new();
    exports.export(name, ExportKind::Memory, 0);
    module.section(&exports);

    module.finish()
}

fn sig(params: &[WasmValType], results: &[WasmValType]) -> DeclaredSignature {
    DeclaredSignature {
        params: params.to_vec(),
        results: results.to_vec(),
    }
}

#[test]
fn accepts_matching_signature() {
    let bytes = module_exporting("sum", &[ValType::I32, ValType::I32], &[ValType::I32]);
    let declared = sig(&[WasmValType::I32, WasmValType::I32], &[WasmValType::I32]);
    validate_extern(&bytes, "sum", &declared).expect("matching signature should validate");
}

#[test]
fn accepts_i64_and_void_signatures() {
    let bytes = module_exporting("store", &[ValType::I64, ValType::I32], &[]);
    let declared = sig(&[WasmValType::I64, WasmValType::I32], &[]);
    validate_extern(&bytes, "store", &declared).expect("i64/void signature should validate");
}

#[test]
fn missing_export_is_distinct_error() {
    let bytes = module_exporting("sum", &[ValType::I32], &[ValType::I32]);
    let declared = sig(&[WasmValType::I32], &[WasmValType::I32]);
    let err = validate_extern(&bytes, "product", &declared).unwrap_err();
    match err {
        ValidateError::ExportNotFound {
            export_field,
            available_functions,
        } => {
            assert_eq!(export_field, "product");
            assert_eq!(available_functions, vec!["sum".to_string()]);
        }
        other => panic!("expected ExportNotFound, got {other:?}"),
    }
}

#[test]
fn non_function_export_of_same_name_is_export_not_found() {
    // A memory named `sum` is not a function export; validation must report it as
    // a missing *function* export, not a signature mismatch.
    let bytes = module_exporting_memory("sum");
    let declared = sig(&[WasmValType::I32], &[WasmValType::I32]);
    let err = validate_extern(&bytes, "sum", &declared).unwrap_err();
    assert!(
        matches!(err, ValidateError::ExportNotFound { .. }),
        "expected ExportNotFound, got {err:?}"
    );
}

#[test]
fn mismatched_param_count_is_signature_mismatch() {
    let bytes = module_exporting("sum", &[ValType::I32, ValType::I32], &[ValType::I32]);
    // declares one parameter; module has two.
    let declared = sig(&[WasmValType::I32], &[WasmValType::I32]);
    let err = validate_extern(&bytes, "sum", &declared).unwrap_err();
    match err {
        ValidateError::SignatureMismatch { export_field, mismatch } => {
            assert_eq!(export_field, "sum");
            assert_eq!(mismatch.found_params, vec![WasmValType::I32, WasmValType::I32]);
        }
        other => panic!("expected SignatureMismatch, got {other:?}"),
    }
}

#[test]
fn mismatched_param_type_is_signature_mismatch() {
    let bytes = module_exporting("sum", &[ValType::I64], &[ValType::I32]);
    // declares i32 param; module has i64.
    let declared = sig(&[WasmValType::I32], &[WasmValType::I32]);
    let err = validate_extern(&bytes, "sum", &declared).unwrap_err();
    assert!(
        matches!(err, ValidateError::SignatureMismatch { .. }),
        "expected SignatureMismatch, got {err:?}"
    );
}

#[test]
fn mismatched_return_type_is_signature_mismatch() {
    let bytes = module_exporting("sum", &[ValType::I32], &[ValType::I64]);
    // declares i32 return; module returns i64.
    let declared = sig(&[WasmValType::I32], &[WasmValType::I32]);
    let err = validate_extern(&bytes, "sum", &declared).unwrap_err();
    assert!(
        matches!(err, ValidateError::SignatureMismatch { .. }),
        "expected SignatureMismatch, got {err:?}"
    );
}

#[test]
fn validates_export_behind_imported_function_index() {
    // The exported function is at index 1 (an import occupies index 0). The
    // validator must read the *local* function's type, not the import's.
    let bytes = module_with_import_then_export("sum", &[ValType::I32, ValType::I32], &[ValType::I32]);
    let declared = sig(&[WasmValType::I32, WasmValType::I32], &[WasmValType::I32]);
    validate_extern(&bytes, "sum", &declared)
        .expect("export behind import index should validate against the local type");
}

#[test]
fn rejects_invalid_wasm_bytes() {
    let err = validate_extern(b"not wasm at all", "sum", &sig(&[], &[])).unwrap_err();
    assert!(matches!(err, ValidateError::Parse(_)), "got {err:?}");
}

#[test]
fn lowers_extern_declaration_to_wasm_signature() {
    // The signature comparison is only meaningful if the declared side is lowered
    // exactly like codegen. Lower a real `external fn` and check the value types.
    let arena = inference::parse(
        "spec s { external fn mix(a: i32, b: i64, c: bool) -> u64; }",
    )
    .expect("parse");

    let extern_def = arena
        .source_files()
        .flat_map(|file| file.defs.iter().copied())
        .flat_map(|def_id| collect_externs(&arena, def_id))
        .next()
        .expect("an external fn");

    let Def::ExternFunction { args, returns, .. } = &arena[extern_def].kind else {
        unreachable!("collect_externs only yields externs");
    };

    let declared = lower_extern_signature(&arena, args, *returns).expect("lower");
    assert_eq!(
        declared,
        sig(
            &[WasmValType::I32, WasmValType::I64, WasmValType::I32],
            &[WasmValType::I64],
        )
    );
}

/// Yields every `external fn` reachable from `def_id`, descending into specs.
fn collect_externs(
    arena: &inference_ast::arena::AstArena,
    def_id: inference_ast::ids::DefId,
) -> Vec<inference_ast::ids::DefId> {
    match &arena[def_id].kind {
        Def::ExternFunction { .. } => vec![def_id],
        Def::Spec { defs, .. } => defs
            .iter()
            .flat_map(|&inner| collect_externs(arena, inner))
            .collect(),
        _ => Vec::new(),
    }
}
