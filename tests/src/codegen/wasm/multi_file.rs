//! Multi-file codegen smoke tests.
//!
//! These exercise multi-file flattening end to end through Wasmtime rather than
//! against golden `.wasm` files. Each test assembles a multi-file arena from
//! inline `(module_path, source)` pairs via [`wasm_codegen_multi_file`], so no
//! filesystem fixtures are needed.
//!
//! Coverage:
//! - a re-export chain (`main` → `math` re-exports `lib::arith`) compiles to a
//!   single valid module whose cross-file call executes correctly under Wasmtime;
//! - an item import (`use lib::arith::{add};`) resolves a bare call across files;
//! - two files each defining a same-named struct get distinct layouts, so a
//!   field read picks the right offset for the file it is written in;
//! - only the entry file's `pub fn`s are WASM exports (root-only export policy).

use crate::utils::wasm_codegen_multi_file;

use inference_wasm_codegen::{CodegenOutput, CompilationMode, OptLevel, Target};
use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

/// Runs the multi-file pipeline in proof mode (no analysis, so spec-only
/// patterns are exercised) and returns the full [`CodegenOutput`] so a test can
/// inspect the per-spec index map.
fn proof_codegen_multi_file(files: &[(Vec<&str>, &str)]) -> CodegenOutput {
    let mut arena = inference_ast::arena::AstArena::default();
    for (module_path, source) in files {
        let module_path: Vec<String> = module_path.iter().map(|s| (*s).to_string()).collect();
        let parsed = inference_parser::parse_into(arena, source, module_path);
        assert!(
            parsed.errors.is_empty(),
            "multi-file proof source has syntax errors: {:?}",
            parsed.errors
        );
        arena = parsed.arena;
    }
    let typed_context = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
        .expect("multi-file proof type check should succeed")
        .typed_context();
    inference_wasm_codegen::codegen(
        &typed_context,
        Target::Wasm32,
        CompilationMode::Proof,
        OptLevel::O3,
        "output",
    )
    .expect("multi-file proof codegen should succeed")
}

/// Instantiates `wasm_bytes` and returns the store + instance for calling
/// exported functions.
fn instantiate(wasm_bytes: &[u8]) -> (Store<()>, Instance) {
    inf_wasmparser::validate(wasm_bytes)
        .unwrap_or_else(|e| panic!("generated multi-file Wasm module is invalid: {e}"));
    let engine = Engine::default();
    let module = Module::new(&engine, wasm_bytes)
        .unwrap_or_else(|e| panic!("failed to create Wasm module: {e}"));
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[])
        .unwrap_or_else(|e| panic!("failed to instantiate Wasm module: {e}"));
    (store, instance)
}

/// Calls a zero-argument exported function returning `i32`.
fn call_i32(store: &mut Store<()>, instance: &Instance, name: &str) -> i32 {
    let f: TypedFunc<(), i32> = instance
        .get_typed_func(&mut *store, name)
        .unwrap_or_else(|e| panic!("failed to get '{name}': {e}"));
    f.call(&mut *store, ())
        .unwrap_or_else(|e| panic!("call to '{name}' failed: {e}"))
}

#[test]
fn re_export_chain_cross_file_call_executes() {
    // The corrected three-file example from the issue: `main` imports `math`,
    // which re-exports `lib::arith` with `pub use`, so `main` reaches
    // `math::arith::add` only through the re-export chain.
    let main = "\
use math;

pub fn run() -> i32 {
    return math::arith::add(1, 2);
}
";
    let math = "\
pub use lib::arith;

pub fn foo() {}
";
    let lib_arith = "\
pub fn add(a: i32, b: i32) -> i32 {
    return a + b;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["math"], math),
        (vec!["lib", "arith"], lib_arith),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 3);
}

#[test]
fn two_segment_namespace_call_executes() {
    // The basic file-import pattern: `use util;` binds namespace `util`, and the
    // two-segment `util::helper()` resolves to `util.inf`'s function. This is the
    // shortest cross-file call shape — no re-export chain, no braced item import.
    let main = "\
use util;

pub fn run() -> i32 {
    return util::helper();
}
";
    let util = "\
pub fn helper() -> i32 {
    return 7;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["util"], util),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 7);
}

#[test]
fn item_import_bare_cross_file_call_executes() {
    // A braced item import binds `add` directly; the bare call must resolve to
    // the foreign file's function index.
    let main = "\
use lib::arith::{add};

pub fn run() -> i32 {
    return add(40, 2);
}
";
    let lib_arith = "\
pub fn add(a: i32, b: i32) -> i32 {
    return a + b;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "arith"], lib_arith),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 42);
}

#[test]
fn same_named_structs_in_two_files_get_distinct_layouts() {
    // `Pair` in the entry file is `{ a: i32, b: i32 }` (b at offset 4); `Pair`
    // in `other.inf` is `{ a: i64, b: i32 }` (b at offset 8). Each file reads
    // `.b` on its own `Pair`, so a single shared layout would mis-read one of
    // them. Both reads returning the stored sentinel proves distinct layouts.
    let main = "\
use lib::shapes;

pub struct Pair {
    a: i32;
    b: i32;
}

pub fn here_b() -> i32 {
    let p: Pair = Pair { a: 10, b: 20 };
    return p.b;
}

pub fn there_b() -> i32 {
    return lib::shapes::there_b();
}
";
    let shapes = "\
pub struct Pair {
    a: i64;
    b: i32;
}

pub fn there_b() -> i32 {
    let p: Pair = Pair { a: 100, b: 200 };
    return p.b;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "shapes"], shapes),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "here_b"), 20);
    assert_eq!(call_i32(&mut store, &instance, "there_b"), 200);
}

#[test]
fn only_entry_file_pub_fns_are_exported() {
    // `lib::arith::add` is `pub` but lives in an imported file, so it is
    // intra-project visible, not a WASM export. Only the entry file's `pub fn`s
    // appear in the export section.
    let main = "\
use lib::arith;

pub fn run() -> i32 {
    return lib::arith::add(2, 3);
}
";
    let lib_arith = "\
pub fn add(a: i32, b: i32) -> i32 {
    return a + b;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "arith"], lib_arith),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    // The entry export is callable...
    assert_eq!(call_i32(&mut store, &instance, "run"), 5);
    // ...and the imported file's `pub fn` is not exported.
    assert!(
        instance.get_func(&mut store, "add").is_none(),
        "imported `pub fn add` must not be a WASM export"
    );
}

#[test]
fn proof_mode_specs_are_file_qualified_across_files() {
    // An entry-file spec keeps its bare name in the spec section; a spec in an
    // imported file is qualified by that file's module path, so the per-spec
    // entries from different files stay distinct.
    let main = "\
spec EntrySpec {
    fn obligation() -> i32 {
        return 1;
    }
}

pub fn main() {}
";
    let lib_checks = "\
spec LibSpec {
    fn obligation() -> i32 {
        return 2;
    }
}
";

    let output = proof_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "checks"], lib_checks),
    ]);
    let by_spec = output.spec_func_indices_by_spec();

    assert!(
        by_spec.contains_key("EntrySpec"),
        "entry-file spec must keep its bare name; keys: {:?}",
        by_spec.keys().collect::<Vec<_>>()
    );
    assert!(
        by_spec.contains_key("lib_checks_LibSpec"),
        "imported-file spec must be file-qualified with an underscore-joined, \
         Rocq-legal key; keys: {:?}",
        by_spec.keys().collect::<Vec<_>>()
    );
    assert!(
        !by_spec.contains_key("LibSpec"),
        "the imported spec must not also appear under its bare name; keys: {:?}",
        by_spec.keys().collect::<Vec<_>>()
    );
}
