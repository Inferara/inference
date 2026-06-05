//! Golden + structural tests for `external fn` import emission (issue #9, Phase 2).
//!
//! Each `external fn` bound to a source module via `use … from <module>` is
//! emitted as a WASM function import. Imports occupy the lowest function indices
//! (`0..N`), so every locally defined function is shifted by the import count and
//! every extern call lowers to its import index.
//!
//! These tests run codegen WITHOUT analysis: analysis rule A024 still rejects
//! calls to external functions (the link step that satisfies the import is a
//! later phase), so the no-analysis path is the only way to exercise the codegen
//! in isolation — mirroring how the non-det golden tests bypass analysis.

#[cfg(test)]
mod extern_import_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen_no_analysis,
    };
    use inf_wasmparser::{ExternalKind, Operator, Parser, Payload, TypeRef, ValType};

    /// A single `(module, field, type_idx)` triple read back from the import
    /// section, plus the `Call` operands found in each defined function body.
    struct ModuleShape {
        imports: Vec<(String, String, u32)>,
        /// Type index of every locally defined function, in definition order.
        defined_func_types: Vec<u32>,
        /// `(export_name, function_index)` for every exported function.
        func_exports: Vec<(String, u32)>,
        /// `Call` operands for each defined function body, in definition order.
        calls_per_defined_func: Vec<Vec<u32>>,
    }

    fn read_shape(wasm: &[u8]) -> ModuleShape {
        let mut imports = Vec::new();
        let mut defined_func_types = Vec::new();
        let mut func_exports = Vec::new();
        let mut calls_per_defined_func = Vec::new();

        for payload in Parser::new(0).parse_all(wasm) {
            match payload.expect("valid wasm payload") {
                Payload::ImportSection(reader) => {
                    for import in reader {
                        let import = import.expect("valid import");
                        if let TypeRef::Func(type_idx) = import.ty {
                            imports.push((
                                import.module.to_string(),
                                import.name.to_string(),
                                type_idx,
                            ));
                        } else {
                            panic!("unexpected non-function import: {import:?}");
                        }
                    }
                }
                Payload::FunctionSection(reader) => {
                    for type_idx in reader {
                        defined_func_types.push(type_idx.expect("valid function type idx"));
                    }
                }
                Payload::ExportSection(reader) => {
                    for export in reader {
                        let export = export.expect("valid export");
                        if export.kind == ExternalKind::Func {
                            func_exports.push((export.name.to_string(), export.index));
                        }
                    }
                }
                Payload::CodeSectionEntry(body) => {
                    let mut calls = Vec::new();
                    let reader = body.get_operators_reader().expect("operators reader");
                    for op in reader {
                        // Fail fast on a malformed operator stream: silently
                        // skipping decode errors could mask broken codegen output
                        // and let a structural assertion pass on garbage.
                        if let Operator::Call { function_index } = op.expect("operator decodes") {
                            calls.push(function_index);
                        }
                    }
                    calls_per_defined_func.push(calls);
                }
                _ => {}
            }
        }

        ModuleShape {
            imports,
            defined_func_types,
            func_exports,
            calls_per_defined_func,
        }
    }

    /// Reads every function type in the type section, in type-index order, as
    /// `(params, results)`. Lets a test resolve an import's `type_idx` to the
    /// concrete WASM value types of the emitted import signature.
    fn read_func_types(wasm: &[u8]) -> Vec<(Vec<ValType>, Vec<ValType>)> {
        let mut func_types = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            if let Payload::TypeSection(reader) = payload.expect("valid wasm payload") {
                for func_type in reader.into_iter_err_on_gc_types() {
                    let func_type = func_type.expect("function type");
                    func_types.push((
                        func_type.params().to_vec(),
                        func_type.results().to_vec(),
                    ));
                }
            }
        }
        func_types
    }

    fn compile(test_name: &str) -> Vec<u8> {
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm = wasm_codegen_no_analysis(&source_code);
        inf_wasmparser::validate(&wasm)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid for {test_name}: {e}"));
        wasm
    }

    fn assert_matches_golden(test_name: &str, actual: &[u8]) {
        let expected_path = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected_path)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm for: {test_name}"));
        assert_wasms_modules_equivalence(&expected, actual);
        assert_wat_equivalence(actual, module_path!(), test_name);
    }

    /// One extern, one local function. The import takes function index 0 and the
    /// local `add_three` is shifted to index 1; the call to `sum` lowers to the
    /// import index 0.
    #[test]
    fn single_import_test() {
        cov_mark::check!(wasm_codegen_emit_import_section);
        cov_mark::check!(wasm_codegen_emit_extern_call);
        let test_name = "single_import";
        let actual = compile(test_name);
        assert_matches_golden(test_name, &actual);

        let shape = read_shape(&actual);
        assert_eq!(
            shape.imports,
            vec![("arith".to_string(), "sum".to_string(), 0)],
            "expected one import (arith.sum) referencing type 0"
        );
        // One local function, shifted past the single import.
        assert_eq!(shape.defined_func_types.len(), 1, "one local function");
        assert_eq!(
            shape.func_exports,
            vec![("add_three".to_string(), 1)],
            "local add_three is shifted to index 1 (after the import)"
        );
        assert_eq!(
            shape.calls_per_defined_func,
            vec![vec![0]],
            "the call to sum lowers to import index 0"
        );
    }

    /// Two externs, called in a nested expression. Imports take indices 0 and 1,
    /// the local `compute` is shifted to index 2, and `sum(neg(x), 3)` lowers to
    /// `call 1` (neg) then `call 0` (sum).
    #[test]
    fn multi_import_test() {
        let test_name = "multi_import";
        let actual = compile(test_name);
        assert_matches_golden(test_name, &actual);

        let shape = read_shape(&actual);
        assert_eq!(
            shape.imports,
            vec![
                ("arith".to_string(), "sum".to_string(), 0),
                ("arith".to_string(), "neg".to_string(), 1),
            ],
            "two imports in declaration order at indices 0 and 1"
        );
        assert_eq!(
            shape.func_exports,
            vec![("compute".to_string(), 2)],
            "local compute is shifted to index 2 (after both imports)"
        );
        assert_eq!(
            shape.calls_per_defined_func,
            vec![vec![1, 0]],
            "nested call order: neg (import 1) evaluated before sum (import 0)"
        );
    }

    /// One extern plus two local functions. Both locals shift past the import:
    /// `helper` -> index 1, `entry` -> index 2. `entry` calls `helper` (local
    /// index 1) and `ext_double` (import index 0).
    #[test]
    fn import_with_locals_test() {
        let test_name = "import_with_locals";
        let actual = compile(test_name);
        assert_matches_golden(test_name, &actual);

        let shape = read_shape(&actual);
        assert_eq!(
            shape.imports,
            vec![("helpers".to_string(), "ext_double".to_string(), 0)],
            "single import at index 0"
        );
        assert_eq!(shape.defined_func_types.len(), 2, "two local functions");
        assert_eq!(
            shape.func_exports,
            vec![("helper".to_string(), 1), ("entry".to_string(), 2)],
            "both locals shift past the import (helper -> 1, entry -> 2)"
        );
        // entry is the second defined function body; it calls helper (local
        // index 1) then ext_double (import index 0).
        assert_eq!(
            shape.calls_per_defined_func[1],
            vec![1, 0],
            "entry calls local helper (idx 1) then extern ext_double (import idx 0)"
        );
    }

    /// Two externs with an identical signature share a single type entry: both
    /// `inc` and `dec` reference type 0, while the local `run` body interns its
    /// own type. Verifies import-against-import type deduplication.
    #[test]
    fn import_dedup_test() {
        let test_name = "import_dedup";
        let actual = compile(test_name);
        assert_matches_golden(test_name, &actual);

        let shape = read_shape(&actual);
        assert_eq!(
            shape.imports,
            vec![
                ("arith".to_string(), "inc".to_string(), 0),
                ("arith".to_string(), "dec".to_string(), 0),
            ],
            "both same-signature imports dedup onto type 0"
        );
        assert_eq!(
            shape.func_exports,
            vec![("run".to_string(), 2)],
            "local run is shifted to index 2"
        );
        assert_eq!(
            shape.calls_per_defined_func,
            vec![vec![1, 0]],
            "inc(dec(x)): dec (import 1) evaluated first, then inc (import 0)"
        );
    }

    /// A bound `external fn` that is never called still emits its import and
    /// still shifts local functions: import emission is driven by the
    /// declaration + binding, not by call sites.
    #[test]
    fn uncalled_bound_extern_still_emits_import() {
        let source = "\
external fn unused(a: i32) -> i32;
use { unused } from lib;

pub fn run(x: i32) -> i32 {
    return x;
}
";
        let wasm = wasm_codegen_no_analysis(source);
        inf_wasmparser::validate(&wasm).expect("invalid wasm");
        let shape = read_shape(&wasm);
        assert_eq!(
            shape.imports,
            vec![("lib".to_string(), "unused".to_string(), 0)],
            "uncalled but bound extern is still imported"
        );
        assert_eq!(
            shape.func_exports,
            vec![("run".to_string(), 1)],
            "local run is still shifted past the import"
        );
        assert_eq!(
            shape.calls_per_defined_func,
            vec![Vec::<u32>::new()],
            "run makes no calls"
        );
    }

    /// A bare `external fn` with no binding `use` carries no provenance, so it is
    /// skipped: no import is emitted and local functions keep index `0` — the
    /// output is identical to a program with no externs at all.
    #[test]
    fn unbound_extern_emits_no_import() {
        let source = "\
external fn bare(a: i32) -> i32;

pub fn run(x: i32) -> i32 {
    return x;
}
";
        let wasm = wasm_codegen_no_analysis(source);
        inf_wasmparser::validate(&wasm).expect("invalid wasm");
        let shape = read_shape(&wasm);
        assert!(
            shape.imports.is_empty(),
            "unbound extern must not be emitted as an import: {:?}",
            shape.imports
        );
        assert_eq!(
            shape.func_exports,
            vec![("run".to_string(), 0)],
            "with no imports the local function keeps index 0"
        );
    }

    /// An ignored extern parameter (`_: i32`) still occupies an ABI slot: the
    /// call site pushes the argument and the real `.wasm` export declares the
    /// parameter, so it must appear in the emitted import signature. This locks
    /// codegen's `import_param_types` in lock-step with the validator's
    /// `lower_extern_signature`, which already treats `Ignored` as a real param.
    #[test]
    fn ignored_extern_param_present_in_import_signature() {
        let source = "\
external fn f(_: i32, x: i64) -> i32;
use { f } from m;

pub fn main() -> i32 {
    let a: i32 = 7;
    let b: i64 = 9;
    return f(a, b);
}
";
        let wasm = wasm_codegen_no_analysis(source);
        inf_wasmparser::validate(&wasm).expect("invalid wasm");

        let shape = read_shape(&wasm);
        assert_eq!(
            shape.imports,
            vec![("m".to_string(), "f".to_string(), 0)],
            "the bound extern f is imported from module m at type 0"
        );

        let func_types = read_func_types(&wasm);
        let (params, results) = &func_types[shape.imports[0].2 as usize];
        assert_eq!(
            params.as_slice(),
            &[ValType::I32, ValType::I64],
            "the ignored first parameter is present: import params are [i32, i64]"
        );
        assert_eq!(
            results.as_slice(),
            &[ValType::I32],
            "import result is [i32]"
        );
    }

    /// Regenerates the golden `.wasm` and `.wat` for every extern-import test.
    /// Run with `--ignored` after intentional codegen changes.
    #[test]
    #[ignore]
    fn regenerate_extern_import_wasm() {
        use crate::utils::{get_test_data_path, regenerate_wat};

        for test_name in [
            "single_import",
            "multi_import",
            "import_with_locals",
            "import_dedup",
        ] {
            let dir = get_test_data_path()
                .join("codegen")
                .join("wasm")
                .join("extern_import")
                .join(test_name);
            let source_code = std::fs::read_to_string(dir.join(format!("{test_name}.inf")))
                .unwrap_or_else(|_| panic!("Failed to read {test_name}.inf"));
            let actual = wasm_codegen_no_analysis(&source_code);
            inf_wasmparser::validate(&actual)
                .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {e}"));
            let wasm_path = dir.join(format!("{test_name}.wasm"));
            std::fs::write(&wasm_path, &actual)
                .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
            regenerate_wat(&actual, &dir, test_name);
        }
    }
}
