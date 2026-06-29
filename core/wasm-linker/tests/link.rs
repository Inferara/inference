//! Integration tests for the static-merge linker.
//!
//! Each test builds its `.wasm` fixtures from inline WAT (via the `wat` crate),
//! links them, and asserts on the unified module: structural validity (through
//! `inf-wasmparser`'s validator), absence of cross-module imports, the merged
//! function bodies, and the precise rejection for Tier-C inputs.

use inf_wasmparser::{ExternalKind, Operator, Parser, Payload, TypeRef};
use inference_wasm_linker::{link as raw_link, LinkError};

/// Assembles a `.wasm` binary from WAT source, panicking with the WAT on error.
fn wasm(wat: &str) -> Vec<u8> {
    wat::parse_str(wat).unwrap_or_else(|e| panic!("invalid WAT fixture: {e}\n{wat}"))
}

/// Links `main` against the given externals, tagging each external with the
/// single logical module `main` imports from.
///
/// The public `link` API takes `(logical_module, bytes)` pairs so it can match
/// an import's recorded `(module, field)` against the right external. Every
/// single-module fixture in this file imports all of its externs from one
/// logical module, so this helper derives that module from `main`'s import
/// section and pairs all externals with it — keeping each test's call site as
/// `link(&main, &[&lib])`. Tests that need *distinct* logical modules per
/// external (multi-module satisfaction, same-field disambiguation) call
/// [`raw_link`] directly with explicit pairs.
fn link(main: &[u8], libs: &[&[u8]]) -> Result<Vec<u8>, LinkError> {
    let modules: std::collections::BTreeSet<String> = function_imports(main)
        .into_iter()
        .map(|(module, _)| module)
        .collect();
    // A no-import main links any externals away to nothing; the module label is
    // irrelevant there. With imports, every fixture here uses a single module.
    let module = modules.into_iter().next().unwrap_or_default();
    let pairs: Vec<(&str, &[u8])> = libs.iter().map(|b| (module.as_str(), *b)).collect();
    raw_link(main, &pairs)
}

/// Validates `bytes` as a complete WASM module.
fn assert_valid(bytes: &[u8]) {
    inf_wasmparser::validate(bytes)
        .unwrap_or_else(|e| panic!("linked module failed validation: {e}"));
}

/// The `(module, field)` pairs of every function import in `bytes`.
fn function_imports(bytes: &[u8]) -> Vec<(String, String)> {
    let mut imports = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::ImportSection(reader) = payload.unwrap() {
            for import in reader {
                let import = import.unwrap();
                if matches!(import.ty, TypeRef::Func(_)) {
                    imports.push((import.module.to_string(), import.name.to_string()));
                }
            }
        }
    }
    imports
}

/// Number of function bodies in the code section.
fn code_body_count(bytes: &[u8]) -> usize {
    let mut count = 0;
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(_) = payload.unwrap() {
            count += 1;
        }
    }
    count
}

/// The exported-function names of `bytes`.
fn exported_functions(bytes: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::ExportSection(reader) = payload.unwrap() {
            for export in reader {
                let export = export.unwrap();
                if export.kind == ExternalKind::Func {
                    names.push(export.name.to_string());
                }
            }
        }
    }
    names
}

/// The `call` target indices in the body of the function at `func_idx`.
fn body_call_targets(bytes: &[u8], func_idx: usize) -> Vec<u32> {
    let mut calls_per_body: Vec<Vec<u32>> = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            let mut calls = Vec::new();
            for op in body.get_operators_reader().unwrap() {
                if let Operator::Call { function_index } = op.unwrap() {
                    calls.push(function_index);
                }
            }
            calls_per_body.push(calls);
        }
    }
    calls_per_body[func_idx].clone()
}

/// The `(function index, name)` pairs recorded in the module's `name` custom
/// section, or an empty vector if no name section is present.
fn function_names(bytes: &[u8]) -> Vec<(u32, String)> {
    let mut names = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CustomSection(custom) = payload.unwrap()
            && let inf_wasmparser::KnownCustom::Name(reader) = custom.as_known()
        {
            for sub in reader {
                if let inf_wasmparser::Name::Function(map) = sub.unwrap() {
                    for naming in map {
                        let naming = naming.unwrap();
                        names.push((naming.index, naming.name.to_string()));
                    }
                }
            }
        }
    }
    names
}

/// The raw payload of the custom section named `name`, if present.
fn custom_section_data(bytes: &[u8], name: &str) -> Option<Vec<u8>> {
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CustomSection(custom) = payload.unwrap()
            && custom.name() == name
        {
            return Some(custom.data().to_vec());
        }
    }
    None
}

/// Decodes the `inference.spec_funcs` payload into `(spec_name, [idx])` pairs,
/// mirroring the encoder's format, for asserting post-link index rewriting.
fn decode_spec_funcs(data: &[u8]) -> Vec<(String, Vec<u32>)> {
    let mut reader = inf_wasmparser::BinaryReader::new(data, 0);
    let version = reader.read_var_u32().unwrap();
    assert_eq!(version, 1, "spec_funcs version");
    let count = reader.read_var_u32().unwrap();
    let mut out = Vec::new();
    for _ in 0..count {
        let name = reader.read_string().unwrap().to_string();
        let idx_count = reader.read_var_u32().unwrap();
        let mut indices = Vec::new();
        for _ in 0..idx_count {
            indices.push(reader.read_var_u32().unwrap());
        }
        out.push((name, indices));
    }
    out
}

/// Decodes the v2 (`with kinds`) `inference.spec_funcs` payload into
/// `(spec_name, [(idx, kind_byte)])` pairs, for asserting that the obligation
/// kind survives the decode/remap/re-encode link path alongside the remapped
/// index.
fn decode_spec_funcs_v2(data: &[u8]) -> Vec<(String, Vec<(u32, u8)>)> {
    let mut reader = inf_wasmparser::BinaryReader::new(data, 0);
    let version = reader.read_var_u32().unwrap();
    assert_eq!(version, 2, "spec_funcs version (with kinds)");
    let count = reader.read_var_u32().unwrap();
    let mut out = Vec::new();
    for _ in 0..count {
        let name = reader.read_string().unwrap().to_string();
        let idx_count = reader.read_var_u32().unwrap();
        let mut indices: Vec<(u32, u8)> = Vec::new();
        for _ in 0..idx_count {
            indices.push((reader.read_var_u32().unwrap(), 0));
        }
        for slot in &mut indices {
            slot.1 = reader.read_u8().unwrap();
        }
        out.push((name, indices));
    }
    out
}

/// Whether the body of the function at `func_idx` contains an `i32.add`.
fn body_has_i32_add(bytes: &[u8], func_idx: usize) -> bool {
    let mut idx = 0;
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if idx == func_idx {
                return body
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .any(|op| matches!(op.unwrap(), Operator::I32Add));
            }
            idx += 1;
        }
    }
    false
}

/// Whether the body of the function at `func_idx` contains an `i32.store`.
fn body_has_i32_store(bytes: &[u8], func_idx: usize) -> bool {
    let mut idx = 0;
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if idx == func_idx {
                return body
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .any(|op| matches!(op.unwrap(), Operator::I32Store { .. }));
            }
            idx += 1;
        }
    }
    false
}

/// The `(initial, maximum)` page limits of the module's single linear memory, or
/// `None` if it declares no memory.
fn memory_limits(bytes: &[u8]) -> Option<(u64, Option<u64>)> {
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::MemorySection(reader) = payload.unwrap() {
            let mem = reader.into_iter().next()?.unwrap();
            return Some((mem.initial, mem.maximum));
        }
    }
    None
}

// -- Tier A: pure functions --------------------------------------------------

/// A main module that imports two pure functions, `sum` and `sub`, and calls
/// each from a local `compute` function it exports.
fn main_with_sum_and_sub() -> Vec<u8> {
    wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (import "mathlib" "sub" (func (;1;) (type 0)))
          (func (;2;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0
            local.get 0
            local.get 1
            call 1
            i32.sub)
          (export "compute" (func 2)))
        "#,
    )
}

/// An external module exporting pure `sum` and `sub`.
fn mathlib_pure() -> Vec<u8> {
    wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.sub)
          (export "sum" (func 0))
          (export "sub" (func 1)))
        "#,
    )
}

#[test]
fn tier_a_merges_pure_functions() {
    let main = main_with_sum_and_sub();
    let lib = mathlib_pure();

    let linked = link(&main, &[&lib]).expect("link should succeed");
    assert_valid(&linked);

    // No cross-module imports remain.
    assert!(
        function_imports(&linked).is_empty(),
        "expected no function imports, found {:?}",
        function_imports(&linked)
    );

    // Three bodies: the main `compute`, plus the two merged `sum`/`sub`.
    assert_eq!(code_body_count(&linked), 3);

    // The main module's export survives.
    assert_eq!(exported_functions(&linked), vec!["compute".to_string()]);
}

#[test]
fn tier_a_main_calls_point_at_merged_bodies() {
    let main = main_with_sum_and_sub();
    let lib = mathlib_pure();
    let linked = link(&main, &[&lib]).expect("link should succeed");

    // After the merge, `compute` is local function 0; the merged `sum` and
    // `sub` are functions 1 and 2. The two `call` operators in `compute` must
    // now target 1 and 2 (originally imports 0 and 1).
    assert_eq!(body_call_targets(&linked, 0), vec![1, 2]);
}

// -- Name section ------------------------------------------------------------

#[test]
fn merged_functions_are_named_after_satisfied_import_fields() {
    // Neither fixture carries a `name` section. The two merged closure roots
    // must still be named — after the import fields they satisfy, prefixed with
    // their logical module (`mathlib`) — so the Rocq translator emits
    // `Definition mathlib_sum` / `Definition mathlib_sub` rather than opaque
    // `func_<uuid>` placeholders.
    let main = main_with_sum_and_sub();
    let lib = mathlib_pure();
    let linked = link(&main, &[&lib]).expect("link should succeed");
    assert_valid(&linked);

    // Output indices: compute=0, merged sum=1, merged sub=2.
    let names = function_names(&linked);
    assert!(
        names.contains(&(1, "mathlib.sum".to_string())),
        "merged sum must be named after its module-prefixed import field, got {names:?}"
    );
    assert!(
        names.contains(&(2, "mathlib.sub".to_string())),
        "merged sub must be named after its module-prefixed import field, got {names:?}"
    );
}

#[test]
fn main_function_names_survive_the_merge() {
    // The main module names its local `compute` in a `name` section; that name
    // must follow the function onto its import-free output index (0).
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (func $compute (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0)
          (export "compute" (func 1)))
        "#,
    );
    let lib = mathlib_pure();
    let linked = link(&main, &[&lib]).expect("link should succeed");
    assert_valid(&linked);

    let names = function_names(&linked);
    assert!(
        names.contains(&(0, "compute".to_string())),
        "main `compute` name must survive at output index 0, got {names:?}"
    );
    assert!(
        names.contains(&(1, "mathlib.sum".to_string())),
        "merged `sum` must be named with its module prefix, got {names:?}"
    );
}

// -- Type dedup --------------------------------------------------------------

#[test]
fn shared_signatures_dedup_into_one_type() {
    // `sum` and `sub` share `(i32,i32)->i32`, which also matches `compute`'s
    // type and the import type. The output type section must collapse them.
    let main = main_with_sum_and_sub();
    let lib = mathlib_pure();
    let linked = link(&main, &[&lib]).expect("link should succeed");
    assert_valid(&linked);

    let mut type_count = 0;
    for payload in Parser::new(0).parse_all(&linked) {
        if let Payload::TypeSection(reader) = payload.unwrap() {
            type_count = reader.count();
        }
    }
    assert_eq!(type_count, 1, "all functions share one (i32,i32)->i32 type");
}

// -- Transitive closure ------------------------------------------------------

#[test]
fn transitive_closure_pulls_in_called_internals() {
    // `sum` is exported but delegates to a non-exported internal `add_impl`.
    // The closure must drag `add_impl` into the merge and re-index `sum`'s call
    // to it.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0)
          (export "compute" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 1)
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("link should succeed");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());

    // compute(0) + merged sum(1) + merged add_impl(2) == 3 bodies.
    assert_eq!(code_body_count(&linked), 3);

    // compute's call (originally import 0) now targets merged `sum` at 1.
    assert_eq!(body_call_targets(&linked, 0), vec![1]);

    // merged `sum` (body 1) must now call merged `add_impl` at 2, not its
    // original index 1.
    assert_eq!(body_call_targets(&linked, 1), vec![2]);
}

#[test]
fn closure_does_not_pull_unreferenced_functions() {
    // The library exports `sum` and also defines an unrelated `unused` function
    // that nothing in `sum`'s closure calls. `unused` must not be merged.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0)
          (export "compute" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.mul)
          (export "sum" (func 0))
          (export "unused" (func 1)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("link should succeed");
    assert_valid(&linked);
    // compute + merged sum only; `unused` (i32.mul) is dropped.
    assert_eq!(code_body_count(&linked), 2);
}

// -- Tier B: caller-passed pointers ------------------------------------------

#[test]
fn tier_b_merges_function_over_caller_memory() {
    // `store_at` writes a value to a caller-supplied address. It touches memory
    // but defines no data of its own — Tier B. The main module owns the shared
    // memory.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32)))
          (import "memlib" "store_at" (func (;0;) (type 0)))
          (memory (;0;) 1 1)
          (func (;1;) (type 0) (param i32 i32)
            local.get 0
            local.get 1
            call 0)
          (export "memory" (memory 0))
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32)))
          (memory (;0;) 1)
          (func (;0;) (type 0) (param i32 i32)
            local.get 0
            local.get 1
            i32.store)
          (export "store_at" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("Tier B should merge");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    assert_eq!(code_body_count(&linked), 2);

    // The shared memory export survives.
    let mut has_memory_export = false;
    for payload in Parser::new(0).parse_all(&linked) {
        if let Payload::ExportSection(reader) = payload.unwrap() {
            for export in reader {
                if export.unwrap().kind == ExternalKind::Memory {
                    has_memory_export = true;
                }
            }
        }
    }
    assert!(has_memory_export, "shared memory export must survive");

    // The merged `store_at` body keeps its `i32.store`.
    assert!(
        body_has_i32_store(&linked, 1),
        "merged Tier-B body must retain its memory store"
    );
}

// -- Tier C: rejected --------------------------------------------------------

#[test]
fn tier_c_data_segment_requires_relocatable_build() {
    // `lookup` reads from a baked-in data segment via `memory.init`. That is
    // own static data — Tier C — and must be rejected.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (result i32)))
          (import "tablelib" "lookup" (func (;0;) (type 0)))
          (memory (;0;) 1 1)
          (func (;1;) (type 0) (result i32)
            call 0)
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (result i32)))
          (memory (;0;) 1)
          (data (;0;) (i32.const 0) "\2a\00\00\00")
          (func (;0;) (type 0) (result i32)
            i32.const 0
            i32.const 0
            i32.const 4
            memory.init 0
            i32.const 0
            i32.load)
          (export "lookup" (func 0)))
        "#,
    );

    let err = link(&main, &[&lib]).expect_err("Tier C must be rejected");
    match err {
        LinkError::RequiresRelocatableBuild { field, reasons } => {
            assert_eq!(field, "lookup");
            assert!(
                reasons.iter().any(|r| r.contains("data")),
                "reason should mention static data: {reasons:?}"
            );
        }
        other => panic!("expected RequiresRelocatableBuild, got {other:?}"),
    }
}

#[test]
fn tier_c_global_requires_relocatable_build() {
    // `counter` reads a module-defined global — per-module mutable state that
    // cannot be merged into a shared module without relocation.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (result i32)))
          (import "statelib" "counter" (func (;0;) (type 0)))
          (func (;1;) (type 0) (result i32)
            call 0)
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (result i32)))
          (global (;0;) (mut i32) (i32.const 7))
          (func (;0;) (type 0) (result i32)
            global.get 0)
          (export "counter" (func 0)))
        "#,
    );

    let err = link(&main, &[&lib]).expect_err("Tier C global must be rejected");
    match err {
        LinkError::RequiresRelocatableBuild { field, reasons } => {
            assert_eq!(field, "counter");
            assert!(
                reasons.iter().any(|r| r.contains("global")),
                "reason should mention globals: {reasons:?}"
            );
        }
        other => panic!("expected RequiresRelocatableBuild, got {other:?}"),
    }
}

#[test]
fn tier_c_indirect_call_requires_relocatable_build() {
    // An external function that performs an indirect call needs the table /
    // element space, which the static merge does not relocate.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (result i32)))
          (import "dispatch" "run" (func (;0;) (type 0)))
          (func (;1;) (type 0) (result i32)
            call 0)
          (export "go" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (result i32)))
          (table (;0;) 1 funcref)
          (func (;0;) (type 0) (result i32)
            i32.const 0
            call_indirect (type 0))
          (export "run" (func 0)))
        "#,
    );

    let err = link(&main, &[&lib]).expect_err("Tier C indirect call must be rejected");
    assert!(
        matches!(err, LinkError::RequiresRelocatableBuild { .. }),
        "expected RequiresRelocatableBuild, got {err:?}"
    );
}

#[test]
fn tier_c_subtraction_fabricated_absolute_address_requires_relocatable_build() {
    // An external that computes `p - (p - C)` fabricates the fixed absolute
    // address `C` from its caller pointer `p`: `(p * 1)` is the caller pointer
    // by value but classified not-provably-param, so the subtraction cancels to
    // a caller-independent constant. Storing through it would write host memory
    // the caller never authorised. The provenance analysis must classify the
    // closure Tier C — `Param - NotParam` may not preserve param-derivation —
    // and the whole link must reject rather than admit the write as Tier B.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32)))
          (import "memlib" "store_at" (func (;0;) (type 0)))
          (memory (;0;) 1 1)
          (func (;1;) (type 0) (param i32 i32)
            local.get 0
            local.get 1
            call 0)
          (export "memory" (memory 0))
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32)))
          (memory (;0;) 1)
          (func (;0;) (type 0) (param i32 i32)
            local.get 0
            local.get 0 i32.const 1 i32.mul i32.const 4096 i32.sub
            i32.sub
            local.get 1
            i32.store)
          (export "store_at" (func 0)))
        "#,
    );

    let err = link(&main, &[&lib])
        .expect_err("a fabricated absolute store address must be rejected");
    assert!(
        matches!(err, LinkError::RequiresRelocatableBuild { .. }),
        "expected RequiresRelocatableBuild, got {err:?}"
    );
}

// -- Multiple externals / unsatisfied ---------------------------------------

#[test]
fn imports_satisfied_across_multiple_externals() {
    // `sum` comes from one library, `sub` from another. Both imports must be
    // satisfied and removed.
    let main = main_with_sum_and_sub();
    let sum_lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );
    let sub_lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.sub)
          (export "sub" (func 0)))
        "#,
    );

    let linked = link(&main, &[&sum_lib, &sub_lib]).expect("both imports satisfiable");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    assert_eq!(code_body_count(&linked), 3);
}

#[test]
fn unsatisfied_import_is_an_error() {
    let main = main_with_sum_and_sub();
    // Only `sum` is provided; `sub` has no body to merge.
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );

    let err = link(&main, &[&lib]).expect_err("missing `sub` must fail");
    match err {
        LinkError::UnsatisfiedImport { field } => assert_eq!(field, "sub"),
        other => panic!("expected UnsatisfiedImport, got {other:?}"),
    }
}

#[test]
fn no_imports_passes_through_unchanged() {
    // A self-contained main module with no imports must link to a still-valid
    // module with the same single body.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "add" (func 0)))
        "#,
    );

    let linked = link(&main, &[]).expect("no-import link should succeed");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    assert_eq!(code_body_count(&linked), 1);
    assert_eq!(exported_functions(&linked), vec!["add".to_string()]);
}

#[test]
fn transitive_host_import_is_rejected() {
    // The library's `sum` calls one of its own host imports. There is no body
    // to merge for that import, so the link must fail clearly.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0)
          (export "compute" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "host" "log" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0)
          (export "sum" (func 1)))
        "#,
    );

    let err = link(&main, &[&lib]).expect_err("transitive host import must fail");
    assert!(
        matches!(err, LinkError::TransitiveHostImport { .. }),
        "expected TransitiveHostImport, got {err:?}"
    );
}

// -- Body re-encoding: locals, mixed value types, value-typed blocks ---------

/// The declared `(count, ValType)` locals of the body at `func_idx`, rendered as
/// a printable string so a mismatch shows the actual locals.
fn body_locals(bytes: &[u8], func_idx: usize) -> Vec<(u32, String)> {
    let mut idx = 0;
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if idx == func_idx {
                let mut out = Vec::new();
                for entry in body.get_locals_reader().unwrap() {
                    let (count, ty) = entry.unwrap();
                    out.push((count, format!("{ty:?}")));
                }
                return out;
            }
            idx += 1;
        }
    }
    Vec::new()
}

/// The `(params, results)` value-type lists of every type-section entry,
/// rendered as printable strings.
fn type_signatures(bytes: &[u8]) -> Vec<(Vec<String>, Vec<String>)> {
    let mut sigs = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::TypeSection(reader) = payload.unwrap() {
            for group in reader {
                let group = group.unwrap();
                for sub in group.types() {
                    if let inf_wasmparser::CompositeInnerType::Func(ft) =
                        &sub.composite_type.inner
                    {
                        let params = ft.params().iter().map(|t| format!("{t:?}")).collect();
                        let results = ft.results().iter().map(|t| format!("{t:?}")).collect();
                        sigs.push((params, results));
                    }
                }
            }
        }
    }
    sigs
}

/// Builds an external module exporting `sum:(i32,i32)->i32` whose body opens a
/// function-typed `block` referencing type index 9, which the module's single
/// type entry does not define. `wat` cannot assemble an out-of-range numeric
/// type index, so this is emitted directly with `wasm-encoder`. The merge's
/// total-remap scan must reject it as a clean error.
fn lib_with_out_of_range_block_type() -> Vec<u8> {
    use wasm_encoder::{
        BlockType, CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction,
        Module, TypeSection, ValType,
    };

    let mut module = Module::new();

    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    module.section(&types);

    let mut funcs = FunctionSection::new();
    funcs.function(0);
    module.section(&funcs);

    let mut exports = ExportSection::new();
    exports.export("sum", ExportKind::Func, 0);
    module.section(&exports);

    let mut code = CodeSection::new();
    let mut f = Function::new([]);
    // A function-typed block over a type index the module never defines.
    f.instruction(&Instruction::Block(BlockType::FunctionType(9)));
    f.instruction(&Instruction::End);
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::End);
    code.function(&f);
    module.section(&code);

    module.finish()
}

/// Builds an external module exporting `entry:(funcref)->()` whose body is a
/// trivial `local.get 0; drop` — no reference-producing operator. The ref type
/// lives only in the signature, so this exercises the merge's signature-intern
/// rejection of reference types in isolation (the operator allow-list never
/// sees a reference op here). `wat` cannot easily express a body that drops a
/// funcref parameter without a reference operator the gate would catch.
fn lib_exporting_funcref_param_entry() -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction, Module,
        RefType, TypeSection, ValType,
    };

    let mut module = Module::new();

    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::Ref(RefType::FUNCREF)], []);
    module.section(&types);

    let mut funcs = FunctionSection::new();
    funcs.function(0);
    module.section(&funcs);

    let mut exports = ExportSection::new();
    exports.export("entry", ExportKind::Func, 0);
    module.section(&exports);

    let mut code = CodeSection::new();
    let mut f = Function::new([]);
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::Drop);
    f.instruction(&Instruction::End);
    code.function(&f);
    module.section(&code);

    module.finish()
}

#[test]
fn merged_body_with_locals_and_value_block_survives_reencode() {
    // The merged external `classify` declares locals (i64 and i32) and uses an
    // `if (result i32)` value-typed block. Re-encoding the body must preserve the
    // locals vector and re-emit the value block type — exercising the locals and
    // block-type paths the pure-arithmetic fixtures never touch. The locals are
    // integer types only: the Inference language has no `f32`/`f64` types, so a
    // float local is rejected by the value-type chokepoint rather than merged.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (import "logic" "classify" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32) (result i32)
            local.get 0
            call 0)
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32) (result i32)
            (local i64 i32)
            local.get 0
            i32.const 0
            i32.gt_s
            (if (result i32)
              (then i32.const 1)
              (else i32.const 0)))
          (export "classify" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("merge with locals + value block");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());

    // The merged body is output function 1; its locals must survive.
    let locals = body_locals(&linked, 1);
    assert_eq!(
        locals,
        vec![(1, "I64".to_string()), (1, "I32".to_string())],
        "merged body locals must be preserved through re-encoding, got {locals:?}"
    );

    // The value-typed `if` block re-encodes to an i32-result block; the body
    // must still validate and produce its i32 result (asserted by assert_valid).
    let calls = body_call_targets(&linked, 0);
    assert_eq!(calls, vec![1], "run's call now targets the merged body at 1");
}

#[test]
fn merged_closure_with_mixed_value_types_dedups_and_reencodes() {
    // The library's `mix` takes (i64, i32) and returns i64, and delegates to an
    // internal `helper` of the same signature. Merging exercises the non-i32
    // arms of every value-type mapping (type-section emission, sig dedup key,
    // and external-body type remap) plus a transitive closure re-index. The
    // signature mixes i64 and i32 — distinct integer value types — since the
    // Inference language has no `f32`/`f64` types and a float signature would be
    // rejected by the value-type chokepoint rather than deduped and merged.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i64 i32) (result i64)))
          (import "ints" "mix" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i64 i32) (result i64)
            local.get 0
            local.get 1
            call 0)
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i64 i32) (result i64)))
          (func (;0;) (type 0) (param i64 i32) (result i64)
            local.get 0
            local.get 1
            call 1)
          (func (;1;) (type 0) (param i64 i32) (result i64)
            local.get 0
            i64.const 1
            i64.add)
          (export "mix" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("merge mixed value types");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    // run + merged mix + merged helper.
    assert_eq!(code_body_count(&linked), 3);

    // All four functions share one (i64,i32)->i64 type; it must dedup to one.
    let sigs = type_signatures(&linked);
    assert_eq!(
        sigs,
        vec![(
            vec!["I64".to_string(), "I32".to_string()],
            vec!["I64".to_string()]
        )],
        "the single (i64,i32)->i64 signature must dedup to one type, got {sigs:?}"
    );

    // mix (output 1) re-indexes its internal call to helper (output 2).
    assert_eq!(body_call_targets(&linked, 1), vec![2]);
}

#[test]
fn tail_call_external_is_rejected_at_the_feature_gate() {
    // An external whose body uses `return_call` (the tail-call proposal). The
    // tail-call proposal is outside the supported WASM 1.0 subset
    // (`SUPPORTED_WASM_FEATURES`), and Inference codegen never emits `return_call`
    // (the sret-forwarding path lowers to a plain `call`), so the only such body
    // is a third-party external. The link gate's feature pass must reject it up
    // front with a `UnsupportedWasmFeature` whose message names tail calls —
    // rather than admitting it through the closure scanner's tail-call-as-call
    // handling and re-indexing the target.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (import "tail" "entry" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32) (result i32)
            local.get 0
            call 0)
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0
            return_call 1)
          (func (;1;) (type 0) (param i32) (result i32)
            local.get 0
            i32.const 1
            i32.add)
          (export "entry" (func 0)))
        "#,
    );

    let err = assert_clean_rejection(&main, &lib, "tail call");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("tail call")),
        "expected an UnsupportedWasmFeature naming tail calls, got {err:?}"
    );
}

// -- Diamond closure: one inner callee shared by two roots -------------------

#[test]
fn diamond_closure_merges_shared_internal_once() {
    // Two exported roots `a` and `b` both call the same internal `shared`. The
    // merge must copy `shared` exactly once (the `merged_index` dedup), giving
    // four bodies total: main `run`, merged `a`, merged `b`, merged `shared`.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (import "lib" "a" (func (;0;) (type 0)))
          (import "lib" "b" (func (;1;) (type 0)))
          (func (;2;) (type 0) (param i32) (result i32)
            local.get 0
            call 0
            call 1)
          (export "run" (func 2)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0
            call 2)
          (func (;1;) (type 0) (param i32) (result i32)
            local.get 0
            call 2)
          (func (;2;) (type 0) (param i32) (result i32)
            local.get 0
            i32.const 2
            i32.mul)
          (export "a" (func 0))
          (export "b" (func 1)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("diamond closure merges");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    // run + a + b + shared (exactly one shared copy).
    assert_eq!(
        code_body_count(&linked),
        4,
        "the shared internal must be merged exactly once"
    );

    // Merge order: import `a` closes over `{a, shared}` first (a -> output 1,
    // shared -> output 2), then import `b` adds only itself (b -> output 3)
    // because `shared` is already merged. Both merged roots therefore call the
    // single merged `shared` at output 2.
    assert_eq!(
        body_call_targets(&linked, 1),
        vec![2],
        "merged `a` must call the single shared body at output 2"
    );
    assert_eq!(
        body_call_targets(&linked, 3),
        vec![2],
        "merged `b` must call the same shared body at output 2, proving one copy"
    );
}

// -- Main module globals survive the merge -----------------------------------

/// The `(mutable, init)` of every global, where `init` is the rendered first
/// operator of its constant initializer.
fn module_globals(bytes: &[u8]) -> Vec<(bool, String)> {
    let mut globals = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::GlobalSection(reader) = payload.unwrap() {
            for g in reader {
                let g = g.unwrap();
                let first = g
                    .init_expr
                    .get_operators_reader()
                    .into_iter()
                    .next()
                    .map(|op| format!("{:?}", op.unwrap()))
                    .unwrap_or_default();
                globals.push((g.ty.mutable, first));
            }
        }
    }
    globals
}

#[test]
fn main_globals_and_global_export_survive_the_merge() {
    // The main module owns its own globals (an i32 and an i64) — Tier-C state on
    // an *external* module, but perfectly fine on the main module, which keeps
    // its memory and globals. The merge must re-emit the global section, both
    // constant initializers, and a `Global`-kind export unchanged.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (import "lib" "inc" (func (;0;) (type 0)))
          (global (;0;) (mut i32) (i32.const 11))
          (global (;1;) i64 (i64.const 64))
          (func (;1;) (type 0) (param i32) (result i32)
            local.get 0
            call 0)
          (export "run" (func 1))
          (export "state" (global 0))
          (export "limit" (global 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0
            i32.const 1
            i32.add)
          (export "inc" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("main globals must survive");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());

    // Both globals re-emitted with their mutability and constant initializers.
    let globals = module_globals(&linked);
    assert_eq!(
        globals,
        vec![
            (true, "I32Const { value: 11 }".to_string()),
            (false, "I64Const { value: 64 }".to_string()),
        ],
        "both main globals (i32 + i64) must survive with their initializers, got {globals:?}"
    );

    // The `state`/`limit` global exports survive at their original indices.
    let mut global_exports = Vec::new();
    for payload in Parser::new(0).parse_all(&linked) {
        if let Payload::ExportSection(reader) = payload.unwrap() {
            for export in reader {
                let export = export.unwrap();
                if export.kind == ExternalKind::Global {
                    global_exports.push((export.name.to_string(), export.index));
                }
            }
        }
    }
    assert_eq!(
        global_exports,
        vec![("state".to_string(), 0), ("limit".to_string(), 1)],
        "global exports must survive the merge, got {global_exports:?}"
    );
}

// -- External module that imports its environment ----------------------------

#[test]
fn external_importing_its_environment_is_unsupported() {
    // The library that would satisfy `sum` itself imports a *memory* from its
    // host environment. A static merge cannot reconstruct that environment, so
    // the link must reject it as an unsupported construct (distinct from a
    // transitive host *function* import).
    let main = main_with_sum_and_sub();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "env" "memory" (memory (;0;) 1))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.sub)
          (export "sum" (func 0))
          (export "sub" (func 1)))
        "#,
    );

    let err = link(&main, &[&lib]).expect_err("environment import must be rejected");
    match err {
        LinkError::UnsupportedConstruct(msg) => assert!(
            msg.contains("imports its environment"),
            "message should explain the environment import: {msg}"
        ),
        other => panic!("expected UnsupportedConstruct, got {other:?}"),
    }
}

// -- Invalid input -----------------------------------------------------------

#[test]
fn invalid_main_bytes_are_a_parse_error() {
    // `raw_link` directly: the `link` test helper parses `main` to derive the
    // import module, so garbage main bytes must go straight to the linker.
    let err = raw_link(b"not a wasm module", &[]).expect_err("garbage must not parse");
    assert!(matches!(err, LinkError::Parse(_)), "expected Parse, got {err:?}");
}

#[test]
fn invalid_external_bytes_are_a_parse_error() {
    let main = main_with_sum_and_sub();
    let err = raw_link(&main, &[("mathlib", b"\0asm broken")])
        .expect_err("garbage external must not parse");
    assert!(matches!(err, LinkError::Parse(_)), "expected Parse, got {err:?}");
}

// -- Adversarial / malformed external bodies (robustness audit issues) -------
//
// These externals signature-match the import (so the signature-only
// `validate_extern` upstream would accept them) but carry bodies the merge
// cannot soundly emit. Each must yield a clean `LinkError`, never a panic.

/// A main module importing a pure `sum:(i32,i32)->i32` and calling it.
fn main_importing_sum() -> Vec<u8> {
    wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0)
          (export "compute" (func 1)))
        "#,
    )
}

#[test]
fn out_of_range_call_index_is_a_clean_error() {
    // H1: an external whose `sum` body calls function index 99, far past the one
    // function the module declares. The closure walk must surface a clean
    // `LinkError`, not index `local_funcs` out of bounds and panic.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add
            call 99
            drop
            local.get 0)
          (export "sum" (func 0)))
        "#,
    );

    let err = link(&main, &[&lib]).expect_err("out-of-range call must fail, not panic");
    assert!(
        matches!(err, LinkError::Parse(_)),
        "expected a Parse error for the out-of-range call, got {err:?}"
    );
}

#[test]
fn function_typed_block_external_is_rejected_at_the_feature_gate() {
    // H2 / feature gate: a Tier-A pure external whose body contains a
    // function-typed block `(block (type 1) (param i32) (result i32))` referencing
    // a *defined* signature. A block that references a type index (so it can take
    // params or yield multiple results) is a multi-value construct, outside the
    // supported WASM 1.0 subset, so the gate's feature pass rejects the module up
    // front, naming multi-value.
    //
    // The merge's total-type-remap mechanism (interning a block's referenced
    // signature via `scan_body_type_indices`, the H2 fix that avoided an unmapped-
    // index panic) remains in `merge.rs` as defense-in-depth behind this gate: it
    // is no longer reachable through the public `link` API because the only Tier-A/B
    // bodies that reference a foreign type index are these multi-value blocks, which
    // the gate now fronts.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (type (;1;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            (block (type 1) (param i32) (result i32))
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );

    let err = assert_clean_rejection(&main, &lib, "function-typed block");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("multi-value")),
        "expected an UnsupportedWasmFeature naming multi-value, got {err:?}"
    );
}

#[test]
fn function_typed_block_over_an_out_of_range_type_is_a_clean_error() {
    // H2 (out-of-range variant): a function-typed block whose type index names
    // no type in the source module. The total-remap scan must surface this as a
    // clean parse error, not a silent map or a panic. `wat` cannot assemble an
    // out-of-range numeric type index, so the body is hand-encoded: a `block`
    // (0x02) with a function block type index 9 (LEB 0x09) that the 1-entry type
    // section does not define, then `end`/`end`.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );
    let _ = lib; // the WAT form cannot express the out-of-range index; build it directly.
    let lib = lib_with_out_of_range_block_type();

    let err =
        link(&main, &[&lib]).expect_err("out-of-range block type index must be a clean error");
    assert!(
        matches!(err, LinkError::Parse(_) | LinkError::UnsupportedConstruct(_)),
        "expected a clean Parse/UnsupportedConstruct for the out-of-range type, got {err:?}"
    );
}

#[test]
fn reference_typed_local_in_merged_body_is_rejected_at_the_feature_gate() {
    // H3 / feature gate: an external whose exported `sum` body declares a
    // `funcref` local. A `funcref` local is a reference-types construct, outside
    // the supported WASM 1.0 subset, so the gate's feature pass rejects the module
    // up front, naming reference types.
    //
    // The emit-time backstop — `read_locals`/`map_val_type` in `rewrite.rs`
    // rejecting a ref-typed local rather than escalating to a panic — remains the
    // defense-in-depth layer behind this gate and is covered by a direct unit test
    // there (`reference_typed_local_is_unsupported`).
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            (local funcref)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );

    let err = link(&main, &[&lib]).expect_err("ref-typed local must fail, not panic");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("reference types")),
        "expected an UnsupportedWasmFeature naming reference types, got {err:?}"
    );
}

#[test]
fn wide_external_links_quickly_and_correctly() {
    // H21: a wide external module of many trivial functions, only function 0
    // exported, must parse in linear time. The old `iter_mut().find(is_empty)`
    // body assignment was O(N^2) and stalled the build for tens of seconds on a
    // few-MiB module. This builds a moderately wide module and asserts the link
    // both succeeds and produces a valid module well within a generous bound.
    const N: usize = 20_000;
    let main = main_importing_sum();

    let mut wat = String::from(
        "(module (type (;0;) (func (param i32 i32) (result i32)))\n\
         (type (;1;) (func))\n",
    );
    // Function 0 is the exported `sum`; the rest are trivial padding bodies that
    // widen the module without entering the closure.
    wat.push_str(
        "(func (;0;) (type 0) (param i32 i32) (result i32) local.get 0 local.get 1 i32.add)\n",
    );
    for i in 1..N {
        wat.push_str(&format!("(func (;{i};) (type 1))\n"));
    }
    wat.push_str("(export \"sum\" (func 0)))");
    let lib = wasm(&wat);

    let start = std::time::Instant::now();
    let linked = link(&main, &[&lib]).expect("wide external must link");
    let elapsed = start.elapsed();

    assert_valid(&linked);
    // Only `sum`'s closure (itself) is merged; the padding functions are dropped.
    assert_eq!(code_body_count(&linked), 2, "compute + merged sum only");
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "linking a {N}-function external took {elapsed:?}; O(N^2) parse regressed"
    );
}

#[test]
fn external_with_start_section_is_rejected() {
    // H22: an external declaring a start function. Its initialization closure is
    // never folded into the merge, so silently dropping it would lose the
    // side-effects (and bypass the transitive-host-import gate). The link must
    // reject it cleanly.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (type (;1;) (func))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (func (;1;) (type 1))
          (start 1)
          (export "sum" (func 0)))
        "#,
    );

    let err = link(&main, &[&lib]).expect_err("start section must be rejected");
    assert!(
        matches!(err, LinkError::UnsupportedConstruct(msg) if msg.contains("start function")),
        "expected an UnsupportedConstruct mentioning the start function"
    );
}

#[test]
fn main_with_start_section_is_rejected() {
    // A main module declaring its own start function. `emit` rebuilds the main
    // module section-by-section and writes no `StartSection`, so the start
    // function (and its initializer side-effects) would silently vanish from the
    // output — a valid-but-wrong `.wasm`/`.v`. The merge must reject it up front,
    // mirroring the main-side data/element-segment guards.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func))
          (type (;1;) (func (result i32)))
          (global $g (mut i32) (i32.const 0))
          (func $init (;0;) (type 0)
            i32.const 42
            global.set 0)
          (func $main (;1;) (type 1) (result i32)
            global.get 0)
          (start 0)
          (export "main" (func 1)))
        "#,
    );

    let err = link(&main, &[]).expect_err("main start section must be rejected");
    assert!(
        matches!(&err, LinkError::UnsupportedConstruct(msg) if msg.contains("start")),
        "expected an UnsupportedConstruct mentioning the start section, got {err:?}"
    );
}

#[test]
fn main_importing_a_non_function_is_rejected() {
    // A main module importing a global from its environment. `emit` writes no
    // import section, so the imported global silently vanishes and a body's
    // `global.get 0` rebinds to the first *defined* global — a wrong value in a
    // valid-but-wrong output, with no diagnostic. The merge models function
    // imports only; reject the non-function import up front.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (result i32)))
          (import "env" "g" (global (;0;) i32))
          (global (;1;) i32 (i32.const 42))
          (func (;0;) (type 0) (result i32)
            global.get 0)
          (export "main" (func 0)))
        "#,
    );

    let err = link(&main, &[]).expect_err("main non-function import must be rejected");
    assert!(
        matches!(&err, LinkError::UnsupportedConstruct(msg) if msg.contains("non-function")),
        "expected an UnsupportedConstruct mentioning a non-function import, got {err:?}"
    );
}

#[test]
fn main_importing_a_float_global_is_rejected_not_swallowed() {
    // The float variant of the non-function-import case: an `f32` global import.
    // Dropping the import section would silently swallow it, defeating the
    // no-floats contract. The non-function-import guard rejects it before the
    // float ever has a chance to reach (or bypass) any value-type check.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (result i32)))
          (import "env" "g" (global (;0;) f32))
          (func (;0;) (type 0) (result i32)
            i32.const 0)
          (export "main" (func 0)))
        "#,
    );

    let err = link(&main, &[]).expect_err("main float-global import must be rejected");
    assert!(
        matches!(&err, LinkError::UnsupportedConstruct(msg) if msg.contains("non-function")),
        "expected an UnsupportedConstruct mentioning a non-function import, got {err:?}"
    );
}

#[test]
fn main_with_table_section_is_rejected() {
    // A main module declaring a table and using it via `call_indirect`. `emit`
    // writes no `TableSection`, so the table is silently dropped; the surviving
    // `call_indirect` then fails *after* the merge as
    // `InvalidMergedModule("unknown table 0")`, blaming the linker's own output
    // rather than naming the unsupported main-side construct. Reject the table
    // section up front with a clear diagnostic.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (result i32)))
          (table (;0;) 1 funcref)
          (func (;0;) (type 0) (result i32)
            i32.const 0
            call_indirect (type 0))
          (export "main" (func 0)))
        "#,
    );

    let err = link(&main, &[]).expect_err("main table section must be rejected");
    assert!(
        matches!(&err, LinkError::UnsupportedConstruct(msg) if msg.contains("table")),
        "expected an UnsupportedConstruct mentioning the table section, got {err:?}"
    );
}

#[test]
fn main_with_two_memories_is_rejected() {
    // The static merge models a single shared linear memory. An external is
    // already rejected for declaring more than one memory; a main module with two
    // memories was asymmetrically tolerated — the parser kept only memory 0 and
    // silently discarded the rest, so a body's memarg over memory 1 would rebind
    // to memory 0 in a valid-but-wrong output. Reject the second memory up front,
    // mirroring the external guard and the main data/element/start/table guards.
    let mut module = wasm_encoder::Module::new();
    let mut mems = wasm_encoder::MemorySection::new();
    mems.memory(wasm_encoder::MemoryType {
        minimum: 1,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    mems.memory(wasm_encoder::MemoryType {
        minimum: 1,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&mems);
    let main = module.finish();

    let err = raw_link(&main, &[]).expect_err("a two-memory main must be rejected");
    assert!(
        matches!(&err, LinkError::UnsupportedConstruct(msg) if msg.contains("memor")),
        "expected an UnsupportedConstruct mentioning the multiple memories, got {err:?}"
    );
}

#[test]
fn main_with_v128_local_is_rejected() {
    // A main module whose body declares a `v128` local. The Inference language has
    // no SIMD types, and every SIMD operator is rejected, so the value-type axis
    // must be consistent: a `v128` local would otherwise pass through the
    // main-module re-encode path (which bypasses the feature gate) into the
    // output. Reject it on the value-type axis.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (result i32)))
          (func (;0;) (type 0) (result i32)
            (local v128)
            i32.const 0)
          (export "main" (func 0)))
        "#,
    );

    let err = link(&main, &[]).expect_err("main v128 local must be rejected");
    assert!(
        matches!(&err, LinkError::UnsupportedConstruct(msg) if msg.contains("v128")),
        "expected an UnsupportedConstruct mentioning v128, got {err:?}"
    );
}

#[test]
fn main_with_unused_v128_type_entry_is_rejected() {
    // A `v128` reaching the output through a type-section entry rather than a
    // local: the merged type table copies the main module's function signatures,
    // so a signature naming `v128` would carry the SIMD type through even with no
    // SIMD operator present. Reject it on the type/signature axis.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (result i32)))
          (type (;1;) (func (param v128)))
          (func (;0;) (type 0) (result i32)
            i32.const 0)
          (export "main" (func 0)))
        "#,
    );

    let err = link(&main, &[]).expect_err("main v128 type entry must be rejected");
    assert!(
        matches!(&err, LinkError::UnsupportedConstruct(msg) if msg.contains("v128")),
        "expected an UnsupportedConstruct mentioning v128, got {err:?}"
    );
}

#[test]
fn atomic_op_into_memoryless_main_is_rejected_not_silently() {
    // H26 / feature gate: a shared-memory atomic external linked into a
    // memoryless main. The atomic op and its shared memory belong to the threads
    // proposal, outside the supported WASM 1.0 subset, so the link gate's feature
    // pass rejects the external up front with a clean `UnsupportedWasmFeature`.
    //
    // The deeper backstops this case once exercised — the closure scanner's
    // allow-list rejecting the atomic op, and the post-merge `InvalidMergedModule`
    // gate catching a body copied into a memoryless module — remain present and
    // are covered directly (the allow-list in `safety.rs`, the post-merge gate by
    // the corpus sweep). Any of those clean rejections is acceptable here; a
    // silent `Ok` (invalid artifact) or a panic is not.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (memory (;0;) 1 1 shared)
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.atomic.rmw.add
            local.get 0
            i32.add)
          (export "sum" (func 0)))
        "#,
    );

    let result = link(&main, &[&lib]);
    match result {
        Err(LinkError::UnsupportedWasmFeature { .. })
        | Err(LinkError::InvalidMergedModule(_))
        | Err(LinkError::RequiresRelocatableBuild { .. })
        | Err(LinkError::UnsupportedConstruct(_)) => {}
        Err(other) => panic!("expected a clean rejection, got {other:?}"),
        Ok(bytes) => panic!(
            "merge silently produced a {}-byte module; it must be rejected",
            bytes.len()
        ),
    }
}

#[test]
fn ambiguous_import_across_two_externals_is_rejected() {
    // Defensive: two externals both export a signature-matching `sum`. The
    // field-keyed binding cannot soundly choose between them, so the merge must
    // reject rather than silently pick the first (sort-order-dependent) match.
    let main = main_importing_sum();
    let lib_a = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );
    let lib_b = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.sub)
          (export "sum" (func 0)))
        "#,
    );

    let err = link(&main, &[&lib_a, &lib_b]).expect_err("ambiguous import must be rejected");
    match err {
        LinkError::AmbiguousImport { module, field } => {
            assert_eq!(field, "sum");
            assert_eq!(module, "mathlib");
        }
        other => panic!("expected AmbiguousImport, got {other:?}"),
    }
}

/// C4: two externals export the same field `sum`, but the main module binds it
/// from `bbb`. The merge must fold *bbb's* body (`i32.add`) regardless of the
/// decoy's logical name or slice position — the field-keyed merge previously let
/// the earlier-sorting `aaa` (`i32.sub`) win.
#[test]
fn same_field_two_modules_binds_the_named_module_not_the_first() {
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "bbb" "sum" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0)
          (export "compute" (func 1)))
        "#,
    );
    let sub_lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.sub)
          (export "sum" (func 0)))
        "#,
    );
    let add_lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );

    // The decoy `aaa` sorts before `bbb`; the field-keyed merge would have
    // picked it. Pass it first to make the slice order also favor the decoy.
    let linked = raw_link(&main, &[("aaa", &sub_lib), ("bbb", &add_lib)])
        .expect("the bbb-bound `sum` must satisfy the import");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    // Output func 1 is the merged `sum`; it must carry bbb's `i32.add` body.
    assert!(
        body_has_i32_add(&linked, 1),
        "the merged `sum` must be bbb's `i32.add`, not aaa's `i32.sub`"
    );

    // Reversing the slice order must not change which body is merged: the
    // binding is on the logical module, not the position.
    let reversed = raw_link(&main, &[("bbb", &add_lib), ("aaa", &sub_lib)])
        .expect("order must not matter");
    assert!(
        body_has_i32_add(&reversed, 1),
        "filename/slice order must not decide the merged body"
    );
}

/// Two externals bound under *different* logical modules both export `sum`, and
/// the main module imports `sum` from each. The module-prefixed naming makes the
/// two merged roots' name-section entries distinct by construction (`alib.sum`,
/// `blib.sum`), so neither collides nor forces wasm-to-v's index-suffix
/// disambiguation — even though their import field is identical.
#[test]
fn same_field_two_modules_get_distinct_prefixed_names() {
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "alib" "sum" (func (;0;) (type 0)))
          (import "blib" "sum" (func (;1;) (type 0)))
          (func (;2;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0
            local.get 0
            local.get 1
            call 1
            i32.sub)
          (export "compute" (func 2)))
        "#,
    );
    let alib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );
    let blib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.mul)
          (export "sum" (func 0)))
        "#,
    );

    let linked = raw_link(&main, &[("alib", &alib), ("blib", &blib)])
        .expect("both same-field modules must satisfy their respective imports");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());

    // Output indices: compute=0, then the two merged `sum` roots. Both carry the
    // import field `sum`, so only the module prefix keeps their name-section
    // entries distinct.
    let names = function_names(&linked);
    let merged: std::collections::BTreeSet<&str> = names
        .iter()
        .filter(|(idx, _)| *idx != 0)
        .map(|(_, n)| n.as_str())
        .collect();
    assert!(
        merged.contains("alib.sum"),
        "the alib-bound root must be named `alib.sum`, got {names:?}"
    );
    assert!(
        merged.contains("blib.sum"),
        "the blib-bound root must be named `blib.sum`, got {names:?}"
    );
    assert_eq!(
        merged.len(),
        2,
        "the two same-field roots must have distinct names by construction, got {names:?}"
    );
}

/// A logical module name carrying Inference's `::` path separator
/// (`crypto::sha256`) must flow through the prefix unchanged and deterministically
/// (`crypto::sha256.hash`), with no panic. The downstream Rocq translator
/// sanitizes every non-alphanumeric to `_`, so the residual `::` is the
/// translator's concern, not the linker's — the linker keeps the logical name
/// verbatim so the prefix stays traceable to its source module.
#[test]
fn a_path_separated_logical_module_name_prefixes_deterministically() {
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "crypto::sha256" "hash" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0)
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "hash" (func 0)))
        "#,
    );

    let linked = raw_link(&main, &[("crypto::sha256", &lib)])
        .expect("a `::`-separated logical module must link without panicking");
    assert_valid(&linked);

    let names = function_names(&linked);
    assert!(
        names.contains(&(1, "crypto::sha256.hash".to_string())),
        "the merged root must keep its logical module verbatim in the prefix, got {names:?}"
    );
}

/// H25 + C1: a main module carrying an `inference.spec_funcs` section that binds
/// an extern must (a) keep the section after linking (H25) and (b) rewrite each
/// recorded index from the pre-link space into the post-link space (C1). Here
/// the main imports `sum` and records spec index 1 (its own local function, in
/// the pre-link space that counts the import as index 0); after the import is
/// removed that function shifts down to index 0.
#[test]
fn spec_funcs_section_survives_and_is_reindexed() {
    // version=1, count=1, name_len=1 'S', idx_count=1, index=1
    let spec_payload = [1u8, 1, 1, b'S', 1, 1];
    let main_wat = r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0)
          (export "compute" (func 1)))
        "#;
    let mut main = wasm(main_wat);
    // Append the spec_funcs custom section (the `wat` crate does not emit it).
    use wasm_encoder::Section as _;
    wasm_encoder::CustomSection {
        name: "inference.spec_funcs".into(),
        data: (&spec_payload[..]).into(),
    }
    .append_to(&mut main);

    let lib = mathlib_pure();
    let linked = link(&main, &[&lib]).expect("link must preserve the spec section");
    assert_valid(&linked);

    let data = custom_section_data(&linked, "inference.spec_funcs")
        .expect("the linked module must still carry the spec_funcs section (H25)");
    let decoded = decode_spec_funcs(&data);
    assert_eq!(
        decoded,
        vec![("S".to_string(), vec![0])],
        "pre-link index 1 (import + 1 local) must rewrite to post-link index 0 (C1)"
    );
}

/// Same H25 + C1 reindexing as above, but for a **version-2** spec section that
/// carries an obligation-kind byte per index (here `Exists` = 1). The link must
/// remap the index from the pre-link space (1) to the post-link space (0) while
/// preserving the kind byte verbatim, and re-emit a v2 section (the kind is
/// non-zero, so it cannot normalize back to v1). This pins the kind-preserving
/// remap path that drives downstream `ValidExistsSpec`/`ValidUniqueSpec`
/// selection.
#[test]
fn spec_funcs_v2_section_survives_reindexed_and_keeps_kind() {
    // version=2, count=1, name_len=1 'S', idx_count=1, index=1, kind=1 (Exists)
    let spec_payload = [2u8, 1, 1, b'S', 1, 1, 1];
    let main_wat = r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0)
          (export "compute" (func 1)))
        "#;
    let mut main = wasm(main_wat);
    use wasm_encoder::Section as _;
    wasm_encoder::CustomSection {
        name: "inference.spec_funcs".into(),
        data: (&spec_payload[..]).into(),
    }
    .append_to(&mut main);

    let lib = mathlib_pure();
    let linked = link(&main, &[&lib]).expect("link must preserve the v2 spec section");
    assert_valid(&linked);

    let data = custom_section_data(&linked, "inference.spec_funcs")
        .expect("the linked module must still carry the spec_funcs section (H25)");
    let decoded = decode_spec_funcs_v2(&data);
    assert_eq!(
        decoded,
        vec![("S".to_string(), vec![(0u32, 1u8)])],
        "pre-link index 1 must rewrite to post-link index 0 (C1) while the \
         Exists kind byte (1) is preserved verbatim"
    );
}

#[test]
fn out_of_range_spec_funcs_index_is_a_clean_parse_error() {
    // S2: a spec index past the main module's function count must reject as a
    // clean `LinkError::Parse`, not silently remap onto a wrong/nonexistent
    // function. The post-merge validator treats the `inference.spec_funcs` custom
    // section as opaque, so without the explicit bound in `map_main_func` this
    // would emit a garbage Rocq proof obligation that still passes validation.
    //
    // The main here has 1 import (index 0) + 1 local (index 1), so a pre-link
    // index of 5 is out of range.
    // version=1, count=1, name_len=1 'S', idx_count=1, index=5
    let spec_payload = [1u8, 1, 1, b'S', 1, 5];
    let main_wat = r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0)
          (export "compute" (func 1)))
        "#;
    let mut main = wasm(main_wat);
    use wasm_encoder::Section as _;
    wasm_encoder::CustomSection {
        name: "inference.spec_funcs".into(),
        data: (&spec_payload[..]).into(),
    }
    .append_to(&mut main);

    let lib = mathlib_pure();
    let err = link(&main, &[&lib])
        .expect_err("an out-of-range spec index must be a clean rejection, never a wrong remap");
    assert!(
        matches!(&err, LinkError::Parse(msg) if msg.contains("out of range")),
        "expected a Parse error naming the out-of-range index, got {err:?}"
    );
}

#[test]
fn two_spec_funcs_sections_in_main_are_a_clean_error_not_a_silent_overwrite() {
    // The `inference.spec_funcs` section is a verification deliverable: its proof
    // obligations must never be silently dropped. A main carrying two such
    // sections previously kept only the last (last-wins overwrite), discarding the
    // first section's obligations. The parser must instead reject the duplicate
    // with a clean error so the lost obligations are surfaced, never vanished.
    // version=1, count=1, name_len=1, idx_count=1, index=0 in each section, but
    // recording different spec names so a silent overwrite would be observable.
    let first = [1u8, 1, 1, b'A', 1, 0];
    let second = [1u8, 1, 1, b'B', 1, 0];
    let main_wat = r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0)
          (export "compute" (func 1)))
        "#;
    let mut main = wasm(main_wat);
    use wasm_encoder::Section as _;
    wasm_encoder::CustomSection {
        name: "inference.spec_funcs".into(),
        data: (&first[..]).into(),
    }
    .append_to(&mut main);
    wasm_encoder::CustomSection {
        name: "inference.spec_funcs".into(),
        data: (&second[..]).into(),
    }
    .append_to(&mut main);

    let lib = mathlib_pure();
    let err = link(&main, &[&lib])
        .expect_err("a duplicate spec_funcs section must be rejected, never silently overwritten");
    assert!(
        matches!(&err, LinkError::Parse(msg) | LinkError::UnsupportedConstruct(msg) if msg.contains("spec_funcs")),
        "expected a clean error naming the duplicate spec_funcs section, got {err:?}"
    );
}

#[test]
fn malformed_main_with_out_of_range_type_index_is_a_clean_error_not_a_panic() {
    // S3: the public `link` API accepts arbitrary main bytes. A main whose
    // FunctionSection names a function type index past the type section must be
    // rejected with a clean `LinkError` (the entry-side structural validation),
    // never panic on a raw slice index in `emit` before the post-merge gate runs.
    // `wat` validates and so cannot build this; assemble it with `wasm-encoder`.
    use wasm_encoder::{
        CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction, Module,
        TypeSection,
    };

    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []); // only type index 0 exists
    module.section(&types);

    let mut funcs = FunctionSection::new();
    funcs.function(5); // out-of-range: no type index 5
    module.section(&funcs);

    let mut exports = ExportSection::new();
    exports.export("run", ExportKind::Func, 0);
    module.section(&exports);

    let mut code = CodeSection::new();
    let mut f = Function::new([]);
    f.instruction(&Instruction::End);
    code.function(&f);
    module.section(&code);

    let main = module.finish();

    // No externals: the merge still parses, plans, and emits the main module, so
    // the out-of-range type index is reached on the emit path.
    let err = raw_link(&main, &[])
        .expect_err("a main with an out-of-range type index must be a clean rejection");
    assert!(
        matches!(&err, LinkError::Parse(_)),
        "expected a clean Parse rejection, got {err:?}"
    );
}

// -- WU2: fail-closed rejection of unmergeable operator families -------------
//
// Each test below feeds the merge an external `.wasm` containing a construct the
// static merge cannot model — an atomic, a SIMD op, exception handling, a typed
// reference, a multi-memory access, or a reference-typed signature. The merge
// must reject every one with a CLEAN `LinkError` (never panic, and never a
// silent `Ok` of a structurally-invalid module). Most fixtures assemble from
// inline WAT (the `wat` crate does not validate, so it happily emits these into
// an otherwise-MVP module); the few WAT cannot express are built with
// `wasm-encoder`.

/// Asserts that linking `main` against `lib` is a clean rejection: a returned
/// `LinkError` of one of the fail-closed kinds, never a panic, and never an
/// `Ok` (which for these fixtures would be a silently-invalid artifact).
fn assert_clean_rejection(main: &[u8], lib: &[u8], what: &str) -> LinkError {
    match link(main, &[lib]) {
        Ok(bytes) => panic!(
            "{what}: merge silently produced a {}-byte module; it must be rejected",
            bytes.len()
        ),
        Err(
            e @ (LinkError::UnsupportedConstruct(_)
            | LinkError::RequiresRelocatableBuild { .. }
            | LinkError::InvalidMergedModule(_)
            | LinkError::IncompatibleMemory { .. }
            | LinkError::UnsupportedWasmFeature { .. }
            | LinkError::Parse(_)),
        ) => e,
        Err(other) => panic!("{what}: expected a fail-closed rejection, got {other:?}"),
    }
}

#[test]
fn atomic_memory_op_is_rejected_at_the_feature_gate() {
    // H17 / feature gate: an external with a shared memory and an
    // `i32.atomic.rmw.add` body. Atomics belong to the threads proposal, outside
    // the supported WASM 1.0 subset, so the link gate's feature pass rejects the
    // module up front — before the closure scanner's allow-list (the
    // defense-in-depth backstop, still tested directly in `safety.rs`) would
    // reach the operator. The shared memory the atomic op requires is itself a
    // threads-proposal construct, so the validator names `threads`.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (memory (;0;) 1 1 shared)
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.atomic.rmw.add
            local.get 0
            i32.add)
          (export "sum" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "atomic rmw");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("threads")),
        "expected an UnsupportedWasmFeature naming the threads proposal, got {err:?}"
    );
}

#[test]
fn simd_v128_memory_load_is_rejected_at_the_feature_gate() {
    // H18 / feature gate: an external with a `v128.load` body. SIMD is outside
    // the supported WASM 1.0 subset, so the link gate's feature pass rejects the
    // module up front, naming SIMD — before the closure scanner's allow-list (the
    // backstop, still tested directly in `safety.rs`) would reach the opcode.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (memory (;0;) 1)
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            v128.load
            drop
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "v128.load");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("SIMD")),
        "expected an UnsupportedWasmFeature naming SIMD, got {err:?}"
    );
}

#[test]
fn exception_handling_throw_is_rejected_at_the_feature_gate() {
    // H11 / feature gate: an external with a tag section and a `throw 0` body.
    // Exception handling is outside the supported WASM 1.0 subset, so the link
    // gate's feature pass rejects the module up front, naming the exceptions
    // proposal — before the allow-list (the backstop, tested directly in
    // `safety.rs`) would reach the EH operator.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (type (;1;) (func))
          (tag (;0;) (type 1))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            throw 0)
          (export "sum" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "throw");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("exceptions")),
        "expected an UnsupportedWasmFeature naming exceptions, got {err:?}"
    );
}

#[test]
fn exception_handling_try_table_is_rejected_at_the_feature_gate() {
    // H11 (try_table variant) / feature gate: the structured `try_table` block is
    // likewise part of the exceptions proposal, outside the supported subset, so
    // the gate's feature pass rejects it naming exceptions.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            block
              try_table
              end
            end
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "try_table");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("exceptions")),
        "expected an UnsupportedWasmFeature naming exceptions, got {err:?}"
    );
}

#[test]
fn call_ref_is_rejected_at_the_feature_gate() {
    // H12 / feature gate: an external whose body uses `call_ref`. Typed function
    // references are outside the supported WASM 1.0 subset, so the gate's feature
    // pass rejects the module up front, naming reference types — before the
    // allow-list (the backstop, tested directly in `safety.rs`) would reach the
    // operator.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            ref.null 0
            call_ref 0)
          (export "sum" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "call_ref");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("reference types")),
        "expected an UnsupportedWasmFeature naming reference types, got {err:?}"
    );
}

#[test]
fn ref_null_is_rejected_at_the_feature_gate() {
    // H12/H13 / feature gate: an external whose body uses `ref.null func`. The
    // reference-types proposal is outside the supported subset, so the gate's
    // feature pass rejects the module naming reference types — before the
    // allow-list would reach the operator.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            ref.null func
            drop
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "ref.null");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("reference types")),
        "expected an UnsupportedWasmFeature naming reference types, got {err:?}"
    );
}

#[test]
fn multi_memory_external_is_rejected_at_the_feature_gate() {
    // H14 / feature gate: an external declaring two memories whose body loads from
    // memory 1. Multiple memories belong to the multi-memory proposal, outside the
    // supported WASM 1.0 subset, so the gate's feature pass rejects the module up
    // front — before the per-external `memory_count > 1` guard and the non-zero
    // memarg allow-list (both still tested directly) would reach it. The validator
    // names multiple memories.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (memory (;0;) 1)
          (memory (;1;) 1)
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            i32.load 1
            drop
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "multi-memory");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("memor")),
        "expected an UnsupportedWasmFeature mentioning memories, got {err:?}"
    );
}

#[test]
fn nonzero_memarg_memory_index_is_rejected_cleanly() {
    // H14 (single-memory, non-zero memarg): even a one-memory module whose body
    // names memory 1 in a memarg must be rejected — the load-bearing fix drives
    // off memarg presence so the index can never silently dangle.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (memory (;0;) 1)
          (memory (;1;) 1)
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.store 1
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );
    assert_clean_rejection(&main, &lib, "non-zero memarg");
}

#[test]
fn reference_typed_parameter_signature_is_rejected_at_the_feature_gate() {
    // H23 / feature gate: a crafted external whose exported `entry` has a
    // `funcref` parameter, with a body that uses no reference-producing operator
    // (just `local.get`/`drop`). A `funcref` in a function signature is itself a
    // reference-types construct, so the link gate's feature pass rejects the
    // module up front, naming reference types — before the merge's signature
    // interning is reached.
    //
    // The intern-time backstop (`val_type_tag`/`sig_key` rejecting a ref type so
    // it can never be collapsed to `i32` and silently emitted) remains the
    // defense-in-depth layer behind this gate and is covered by a direct unit
    // test in `merge.rs` (`ref_typed_signature_is_rejected_at_intern_time`).
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (import "reflib" "entry" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32) (result i32)
            local.get 0
            call 0)
          (export "run" (func 1)))
        "#,
    );
    let lib = lib_exporting_funcref_param_entry();
    let err = assert_clean_rejection(&main, &lib, "ref-typed parameter");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("reference types")),
        "expected an UnsupportedWasmFeature naming reference types, got {err:?}"
    );
}

#[test]
fn memory64_external_against_memory32_main_is_rejected_at_the_feature_gate() {
    // H16 (partial) / feature gate: a memory64 external folded onto a memory32
    // main. The memory64 proposal is outside the supported WASM 1.0 subset, so
    // the gate's feature pass rejects the external up front, naming memory64 —
    // before the memory reconciler's shape guard (the backstop, still tested
    // directly in `merge.rs`) would reach it.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (memory (;0;) 1)
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0)
          (export "compute" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (memory (;0;) i64 1)
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "memory64 vs memory32");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("memory64")),
        "expected an UnsupportedWasmFeature naming memory64, got {err:?}"
    );
}

#[test]
fn memory64_external_onto_a_memoryless_main_is_rejected_at_the_feature_gate() {
    // C-4 / feature gate: a `memory64` external forwarded by a *memoryless* main.
    // The memory64 proposal is outside the supported subset, so the gate's feature
    // pass rejects the external up front, naming memory64 — before the
    // reconciler's `None => ext` adopt-path shape guard (the backstop, still
    // tested directly in `merge.rs`) would reach it.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i64) (result i64)))
          (import "memlib" "load_at" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i64) (result i64)
            local.get 0
            call 0)
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i64) (result i64)))
          (memory (;0;) i64 1)
          (func (;0;) (type 0) (param i64) (result i64)
            local.get 0
            i64.load)
          (export "load_at" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "memory64 onto a memoryless main");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("memory64")),
        "expected an UnsupportedWasmFeature naming memory64, got {err:?}"
    );
}

#[test]
fn bare_shared_external_onto_a_memoryless_main_is_rejected_at_the_feature_gate() {
    // L-1 / feature gate: a bare `shared` external memory whose body uses no
    // atomic op (so the operator allow-list does not catch it) folded onto a
    // memoryless main. A `shared` memory is a threads-proposal construct, outside
    // the supported WASM 1.0 subset, so the gate's feature pass rejects the
    // external up front, naming the threads requirement — before the reconciler's
    // adopt-path shape guard (the backstop, still tested directly in `merge.rs`)
    // would reach it.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (import "memlib" "load_at" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32) (result i32)
            local.get 0
            call 0)
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (memory (;0;) 1 1 shared)
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0
            i32.load)
          (export "load_at" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "bare shared onto a memoryless main");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. }
            if details.contains("threads") || details.contains("shared")),
        "expected an UnsupportedWasmFeature naming the threads/shared requirement, got {err:?}"
    );
}

// -- Address-provenance (C2) -------------------------------------------------

#[test]
fn tier_b_param_addressed_load_merges_into_a_valid_module() {
    // C2 (safe case): a Tier-B external that loads through its *parameter* keeps
    // the Tier-B contract — every address is caller-supplied. It must merge into
    // a valid module with the load body intact.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (import "memlib" "load_at" (func (;0;) (type 0)))
          (memory (;0;) 1 1)
          (func (;1;) (type 0) (param i32) (result i32)
            local.get 0
            call 0)
          (export "memory" (memory 0))
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (memory (;0;) 1)
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0
            i32.const 4
            i32.add
            i32.load)
          (export "load_at" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("param-addressed Tier B should merge");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    assert_eq!(code_body_count(&linked), 2);
}

#[test]
fn tier_b_multi_function_param_addressed_helper_merges() {
    // The headline interprocedural case: a `sort(ptr, len)` export that calls an
    // internal `swap(p, a, b)` helper, passing `swap` a param-derived pointer; the
    // helper dereferences its pointer parameter. Because every call site supplies
    // `swap`'s pointer from the root's own parameters, the sound interprocedural
    // analysis proves the whole *two-function* closure is caller-addressed and
    // merges it — the conservative `>1 function => reject` stopgap is gone.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32)))
          (import "sortlib" "sort" (func (;0;) (type 0)))
          (memory (;0;) 1 1)
          (func (;1;) (type 0) (param i32 i32)
            local.get 0
            local.get 1
            call 0)
          (export "memory" (memory 0))
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32)))
          (type (;1;) (func (param i32 i32 i32)))
          (memory (;0;) 1)
          (func (;0;) (type 0) (param i32 i32)
            local.get 0
            local.get 1
            i32.add
            local.get 0
            local.get 1
            call 1)
          (func (;1;) (type 1) (param i32 i32 i32)
            local.get 0
            local.get 1
            i32.load
            i32.store
            local.get 1
            local.get 2
            i32.load
            i32.store)
          (export "sort" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("param-addressed helper closure should merge");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    // run + merged sort + merged swap.
    assert_eq!(code_body_count(&linked), 3);
}

#[test]
fn tier_b_helper_called_with_constant_address_is_rejected() {
    // The interprocedural reject case: the root discards a constant into a helper
    // that dereferences its parameter. The constant argument makes the helper's
    // parameter untrusted at its only call site, so the helper's load aliases a
    // fixed host address — rejected as Tier C, not silently merged.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (result i32)))
          (import "memlib" "peek" (func (;0;) (type 0)))
          (memory (;0;) 1 1)
          (func (;1;) (type 0) (result i32)
            call 0)
          (export "memory" (memory 0))
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (result i32)))
          (type (;1;) (func (param i32) (result i32)))
          (memory (;0;) 1)
          (func (;0;) (type 0) (result i32)
            i32.const 1024
            call 1)
          (func (;1;) (type 1) (param i32) (result i32)
            local.get 0
            i32.load)
          (export "peek" (func 0)))
        "#,
    );

    let err = link(&main, &[&lib]).expect_err("a const-fed helper deref must be rejected");
    match err {
        LinkError::RequiresRelocatableBuild { field, reasons } => {
            assert_eq!(field, "peek");
            assert!(
                reasons.iter().any(|r| r.contains("parameter")),
                "reason should mention parameter provenance: {reasons:?}"
            );
        }
        other => panic!("expected RequiresRelocatableBuild, got {other:?}"),
    }
}

#[test]
fn tier_b_self_recursive_param_addressed_helper_merges() {
    // A self-recursive export that dereferences its parameter and recurses with a
    // *param-derived* argument (`ptr + 4`). The greatest fixpoint keeps the
    // parameter trusted across the back-edge, so the recursive closure merges.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32)))
          (import "memlib" "walk" (func (;0;) (type 0)))
          (memory (;0;) 1 1)
          (func (;1;) (type 0) (param i32)
            local.get 0
            call 0)
          (export "memory" (memory 0))
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32)))
          (memory (;0;) 1)
          (func (;0;) (type 0) (param i32)
            local.get 0
            i32.load
            if
              local.get 0
              i32.const 4
              i32.add
              call 0
            end)
          (export "walk" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("self-recursive param-addressed closure should merge");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    assert_eq!(code_body_count(&linked), 2);
}

#[test]
fn tier_b_absolute_address_load_is_rejected() {
    // C2 (the defect): an external that loads from a *fixed absolute address* not
    // derived from any parameter would silently alias the host program's own
    // memory. The address-provenance analysis must reject it as Tier C rather
    // than merge it.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (result i32)))
          (import "memlib" "peek" (func (;0;) (type 0)))
          (memory (;0;) 1 1)
          (func (;1;) (type 0) (result i32)
            call 0)
          (export "memory" (memory 0))
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (result i32)))
          (memory (;0;) 1)
          (func (;0;) (type 0) (result i32)
            i32.const 1024
            i32.load)
          (export "peek" (func 0)))
        "#,
    );

    let err = link(&main, &[&lib]).expect_err("absolute-address load must be rejected");
    match err {
        LinkError::RequiresRelocatableBuild { field, reasons } => {
            assert_eq!(field, "peek");
            assert!(
                reasons.iter().any(|r| r.contains("parameter")),
                "reason should mention parameter provenance: {reasons:?}"
            );
        }
        other => panic!("expected RequiresRelocatableBuild, got {other:?}"),
    }
}

#[test]
fn tier_b_store_at_absolute_address_is_rejected() {
    // C2 (store variant): a store to a fixed absolute address corrupts the host
    // program's memory at a baked-in offset. It must be rejected.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32)))
          (import "memlib" "poke" (func (;0;) (type 0)))
          (memory (;0;) 1 1)
          (func (;1;) (type 0) (param i32)
            local.get 0
            call 0)
          (export "memory" (memory 0))
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32)))
          (memory (;0;) 1)
          (func (;0;) (type 0) (param i32)
            i32.const 2048
            local.get 0
            i32.store)
          (export "poke" (func 0)))
        "#,
    );

    let err = link(&main, &[&lib]).expect_err("absolute-address store must be rejected");
    assert!(
        matches!(err, LinkError::RequiresRelocatableBuild { .. }),
        "expected RequiresRelocatableBuild, got {err:?}"
    );
}

// -- Memory reconciliation (H15, H16, H24) -----------------------------------

#[test]
fn memoryless_main_with_param_addressed_external_synthesizes_memory() {
    // H24: the main module declares no memory of its own, but the external it
    // links uses memory (through a parameter) and declares its own. The merge
    // must synthesize an output memory from the external's declaration so the
    // merged body's `memory 0` reference is satisfied — a valid module.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32)))
          (import "memlib" "store_at" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32 i32)
            local.get 0
            local.get 1
            call 0)
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32)))
          (memory (;0;) 3)
          (func (;0;) (type 0) (param i32 i32)
            local.get 0
            local.get 1
            i32.store)
          (export "store_at" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("memoryless main + memory external must reconcile");
    assert_valid(&linked);
    let (initial, _max) = memory_limits(&linked).expect("output must declare the synthesized memory");
    assert_eq!(initial, 3, "the external's minimum must be carried into the output");
}

#[test]
fn external_minimum_is_reconciled_so_no_out_of_bounds() {
    // H15: the external declares `(memory 10)`, a far larger minimum than the
    // main module's 1-page memory. The reconciled output minimum must be the max
    // of the two (10), so an access in the external's static range is in-bounds
    // rather than trapping.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (import "memlib" "load_at" (func (;0;) (type 0)))
          (memory (;0;) 1 20)
          (func (;1;) (type 0) (param i32) (result i32)
            local.get 0
            call 0)
          (export "memory" (memory 0))
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (memory (;0;) 10 20)
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0
            i32.load)
          (export "load_at" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("memory minimums must reconcile");
    assert_valid(&linked);
    let (initial, maximum) = memory_limits(&linked).expect("output declares a memory");
    assert_eq!(initial, 10, "reconciled minimum is the max of both module minimums");
    assert_eq!(maximum, Some(20), "reconciled maximum widens to admit both ranges");
}

#[test]
fn memory_grow_against_a_growable_memory_is_reconciled() {
    // H15 (grow, accepted): the external grows memory, and the reconciled
    // memory's maximum exceeds its minimum, so growth can succeed. The merge
    // must accept it and produce a valid module.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (import "memlib" "grow_by" (func (;0;) (type 0)))
          (memory (;0;) 1 10)
          (func (;1;) (type 0) (param i32) (result i32)
            local.get 0
            call 0)
          (export "memory" (memory 0))
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (memory (;0;) 1 10)
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0
            memory.grow)
          (export "grow_by" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("growable memory must reconcile");
    assert_valid(&linked);
    let (_initial, maximum) = memory_limits(&linked).expect("output declares a memory");
    assert!(
        maximum.is_none_or(|m| m > 1),
        "a growable memory must keep room above the minimum, got {maximum:?}"
    );
}

#[test]
fn memory_grow_against_a_fixed_memory_is_rejected() {
    // H15 (grow, rejected): the external grows memory, but every module's memory
    // is pinned (min == max), so growth always fails at runtime. The merge must
    // reject it with a clean diagnostic rather than emit a module whose
    // `memory.grow` silently returns -1.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (import "memlib" "grow_by" (func (;0;) (type 0)))
          (memory (;0;) 1 1)
          (func (;1;) (type 0) (param i32) (result i32)
            local.get 0
            call 0)
          (export "memory" (memory 0))
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (memory (;0;) 1 1)
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0
            memory.grow)
          (export "grow_by" (func 0)))
        "#,
    );

    let err = link(&main, &[&lib]).expect_err("growth against a fixed memory must reject");
    assert!(
        matches!(err, LinkError::IncompatibleMemory { .. }),
        "expected IncompatibleMemory, got {err:?}"
    );
}

#[test]
fn custom_page_size_mismatch_is_rejected_cleanly() {
    // H16 (page-size flag): an external whose memory uses a custom page size
    // changes the address-to-page mapping and cannot be folded onto the main
    // module's default-page memory. The merge must reject it cleanly.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (import "memlib" "load_at" (func (;0;) (type 0)))
          (memory (;0;) 1 1)
          (func (;1;) (type 0) (param i32) (result i32)
            local.get 0
            call 0)
          (export "memory" (memory 0))
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (memory (;0;) 1 1 (pagesize 1))
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0
            i32.load)
          (export "load_at" (func 0)))
        "#,
    );

    // The custom-page-sizes proposal is disabled in the linker's structural
    // validator, so the universal external pre-validation gate (mirroring the
    // driver) now rejects this external as invalid WASM before the memory-shape
    // reconciler is reached. Either rejection is fail-closed and clean: a `Parse`
    // naming the custom page size from the entry gate, or the reconciler's
    // `IncompatibleMemory` if validation is ever relaxed for this proposal. Both
    // are accepted here; what matters is that the merge never folds a
    // custom-page-size memory into the default-page output.
    let err = assert_clean_rejection(&main, &lib, "custom page size");
    let mentions_page_size = match &err {
        LinkError::Parse(message) => message.contains("page size"),
        LinkError::IncompatibleMemory { reason, .. } => reason.contains("page size"),
        _ => false,
    };
    assert!(
        mentions_page_size,
        "expected a clean rejection mentioning the custom page size, got {err:?}"
    );
}

// -- Deterministic adversarial property sweep --------------------------------
//
// This is the stable-`cargo test` analogue of the `cargo-fuzz` target in
// `core/wasm-linker/fuzz/`. `cargo-fuzz` needs a nightly toolchain that is not
// part of the default build, so the generative guard cannot run everywhere; this
// property test runs the same *invariant* over a fixed, hand-seeded corpus on
// every `cargo test`:
//
//   for every (main, externals) in the corpus, `link` must either
//     (a) return `Err`, or
//     (b) return `Ok` with bytes that pass `inf_wasmparser::validate`.
//
// It must NEVER panic, hang, OOM, or return a silently-invalid module. The
// corpus is the union of the audit reproductions (one adversarial external per
// confirmed body-level issue) and a deterministic byte-mutation sweep of a valid
// external, which broadly exercises the parse/closure/merge/emit paths without
// any nondeterminism. Each `link` call is wrapped in `catch_unwind` so a
// regression that reintroduces a panic fails this test with the offending
// fixture named, rather than aborting the run with an opaque backtrace.

/// The outcome a corpus probe demands of `link`.
///
/// Every probe forbids a panic / hang / OOM and a silently-invalid `Ok`. The
/// variants additionally pin *which* clean outcome is correct, so a regression
/// that turns a soundness rejection into a (validating but wrong) merge is
/// caught — not just one that reintroduces a crash.
#[derive(Clone, Copy, PartialEq)]
enum Expect {
    /// Any clean outcome is acceptable: a returned `Err`, or an `Ok` whose bytes
    /// validate. Used by the malformed/mutation inputs whose resolution is shape-
    /// dependent (some parse-reject, some are caught later).
    CleanOutcome,
    /// The probe MUST merge into a valid module. A returned `Err` is a
    /// regression. Used by the legitimate positive controls.
    Merges,
    /// The probe MUST be rejected with a clean `Err`. An `Ok` — even one whose
    /// bytes validate — is a silent miscompile. Used by every soundness
    /// reproduction (the laundering / shape / resource cases) whose merge would
    /// model the wrong machine.
    Rejected,
}

/// One labelled corpus entry: a main module, its externals, the demanded
/// outcome, and a human-readable description used in failure messages.
struct Probe {
    label: &'static str,
    main: Vec<u8>,
    externals: Vec<Vec<u8>>,
    expect: Expect,
}

/// Assembles a `.wasm` from WAT, returning `None` for inputs `wat` itself
/// rejects. The mutation sweep deliberately produces some non-assemblable
/// fixtures via byte flips on already-assembled bytes, never via WAT, so this
/// only guards the hand-written seeds.
fn try_wasm(src: &str) -> Option<Vec<u8>> {
    wat::parse_str(src).ok()
}

/// The hand-seeded adversarial corpus: each external signature-matches the
/// import `main_importing_sum` declares (so a signature-only upstream check
/// would admit it) but carries a body or section the merge must handle without
/// panicking — either by merging into a valid module or by a clean `LinkError`.
/// These mirror the seeds the `cargo-fuzz` target is meant to start from.
fn seed_probes() -> Vec<Probe> {
    let main = main_importing_sum();
    let sum_lib = |body: &str| -> Option<Vec<u8>> {
        try_wasm(&format!(
            "(module (type (;0;) (func (param i32 i32) (result i32))) \
             (func (;0;) (type 0) (param i32 i32) (result i32) {body}) \
             (export \"sum\" (func 0)))"
        ))
    };

    let mut probes = Vec::new();
    let mut push = |label: &'static str, lib: Option<Vec<u8>>| {
        if let Some(lib) = lib {
            probes.push(Probe {
                label,
                main: main.clone(),
                externals: vec![lib],
                // The first-audit body seeds resolve to a clean outcome whose
                // exact shape (parse-reject vs. tier-reject vs. validate-reject)
                // is an internal detail; the invariant is only "no panic, no
                // silently-invalid Ok".
                expect: Expect::CleanOutcome,
            });
        }
    };

    // H1: out-of-range call index.
    push(
        "H1 out-of-range call",
        sum_lib("local.get 0 local.get 1 i32.add call 99 drop local.get 0"),
    );
    // H2: function-typed block over a defined, non-own type index.
    push(
        "H2 function-typed block",
        sum_lib("local.get 0 (block (param) (result)) local.get 1 i32.add"),
    );
    // H3: reference-typed local in the body.
    push(
        "H3 funcref local",
        sum_lib("(local funcref) local.get 0 local.get 1 i32.add"),
    );
    // H11: exception-handling body with a tag section.
    push(
        "H11 throw",
        try_wasm(
            "(module (type (;0;) (func (param i32 i32) (result i32))) \
             (tag (;0;)) \
             (func (;0;) (type 0) (param i32 i32) (result i32) \
               local.get 0 local.get 1 i32.add throw 0) \
             (export \"sum\" (func 0)))",
        ),
    );
    // H12/H13: typed-reference operators.
    push("H12 ref.null", sum_lib("ref.null func drop local.get 0 local.get 1 i32.add"));
    // H17: shared-memory atomic op into a memoryless main.
    push(
        "H17 atomic rmw",
        try_wasm(
            "(module (type (;0;) (func (param i32 i32) (result i32))) \
             (memory (;0;) 1 1 shared) \
             (func (;0;) (type 0) (param i32 i32) (result i32) \
               i32.const 0 local.get 0 i32.atomic.rmw.add drop local.get 1) \
             (export \"sum\" (func 0)))",
        ),
    );
    // H18: SIMD V128 memory load into a memoryless main.
    push(
        "H18 v128.load",
        try_wasm(
            "(module (type (;0;) (func (param i32 i32) (result i32))) \
             (memory (;0;) 1) \
             (func (;0;) (type 0) (param i32 i32) (result i32) \
               i32.const 0 v128.load drop local.get 0 local.get 1 i32.add) \
             (export \"sum\" (func 0)))",
        ),
    );
    // H24: Tier-B memory op merged into a memoryless main.
    push(
        "H24 memory.size",
        try_wasm(
            "(module (type (;0;) (func (param i32 i32) (result i32))) \
             (memory (;0;) 1) \
             (func (;0;) (type 0) (param i32 i32) (result i32) \
               memory.size drop local.get 0 local.get 1 i32.add) \
             (export \"sum\" (func 0)))",
        ),
    );
    // H14: multi-memory external with a non-zero memarg memory index.
    push(
        "H14 multi-memory",
        try_wasm(
            "(module (type (;0;) (func (param i32 i32) (result i32))) \
             (memory (;0;) 1) (memory (;1;) 1) \
             (func (;0;) (type 0) (param i32 i32) (result i32) \
               local.get 0 i32.load 1 drop local.get 0 local.get 1 i32.add) \
             (export \"sum\" (func 0)))",
        ),
    );
    // C2: a load from a fixed absolute address, no parameter provenance.
    push(
        "C2 absolute-address load",
        try_wasm(
            "(module (type (;0;) (func (param i32 i32) (result i32))) \
             (memory (;0;) 1) \
             (func (;0;) (type 0) (param i32 i32) (result i32) \
               i32.const 1024 i32.load drop local.get 0 local.get 1 i32.add) \
             (export \"sum\" (func 0)))",
        ),
    );
    // Hand-encoded: an out-of-range function-typed block index `wat` cannot express.
    probes.push(Probe {
        label: "H2 out-of-range block type",
        main: main.clone(),
        externals: vec![lib_with_out_of_range_block_type()],
        expect: Expect::Rejected,
    });
    // A genuinely-pure external that must merge into a valid module (the positive
    // control: the sweep must not become vacuously all-`Err`).
    probes.push(Probe {
        label: "pure control (must merge)",
        main: main.clone(),
        externals: vec![mathlib_pure()],
        expect: Expect::Merges,
    });
    // Empty / truncated externals — the parser must reject, not index past the end.
    probes.push(Probe {
        label: "empty external",
        main: main.clone(),
        externals: vec![Vec::new()],
        expect: Expect::Rejected,
    });
    probes.push(Probe {
        label: "magic-only external",
        main: main.clone(),
        externals: vec![b"\0asm\x01\0\0\0".to_vec()],
        expect: Expect::Rejected,
    });

    probes.extend(round2_probes());
    probes
}

/// The round-2 audit reproductions, folded into the same panic-free /
/// `Ok ⇒ valid` invariant sweep as the first-audit seeds. Each is the exact
/// laundering / shape / resource shape the dedicated regression tests assert a
/// clean outcome for; including them here additionally guarantees that *however*
/// each is resolved (clean `Err`, or a valid merge for the legitimate cases), it
/// is never a panic, hang, OOM, or silently-invalid module.
///
/// The provenance-laundering probes (C-1/C-2/C-3) address the host's *own*
/// linear memory, so they pair against a memory-owning main that exports a
/// shared memory — the practically-reachable Tier-B shape — rather than the
/// memoryless `main_importing_sum`.
fn round2_probes() -> Vec<Probe> {
    let mut probes = Vec::new();

    // A memory-owning main exporting a shared memory and importing `peek`/`poke`
    // from `mathlib` (the module label the property sweep tags every external
    // with), mirroring the `tier_b_*` reproductions.
    let mem_main = |import_ty: &str, import_field: &str, body: &str| -> Option<Vec<u8>> {
        try_wasm(&format!(
            "(module {import_ty} \
             (import \"mathlib\" \"{import_field}\" (func (;0;) (type 0))) \
             (memory (;0;) 1 1) \
             {body} \
             (export \"memory\" (memory 0)) (export \"run\" (func 1)))"
        ))
    };
    let mem_lib = |ty: &str, field: &str, body: &str| -> Option<Vec<u8>> {
        try_wasm(&format!(
            "(module {ty} (memory (;0;) 1) \
             (func (;0;) (type 0) {body}) (export \"{field}\" (func 0)))"
        ))
    };

    // Every round-2 reproduction is a soundness case: the merge would model the
    // wrong machine, so the only correct outcome is a clean rejection.
    let mut push = |label: &'static str, main: Option<Vec<u8>>, lib: Option<Vec<u8>>| {
        if let (Some(main), Some(lib)) = (main, lib) {
            probes.push(Probe { label, main, externals: vec![lib], expect: Expect::Rejected });
        }
    };

    // C-1: a constant address laundered through a control-flow join into an
    // address-feeding local. The skip path leaves the const, so the address is
    // not parameter-derived on every path — a host-memory alias the provenance
    // analysis must reject.
    push(
        "C-1 control-flow-join laundered load",
        mem_main(
            "(type (;0;) (func (param i32 i32) (result i32)))",
            "peek",
            "(func (;1;) (type 0) (param i32 i32) (result i32) \
               local.get 0 local.get 1 call 0)",
        ),
        mem_lib(
            "(type (;0;) (func (param i32 i32) (result i32)))",
            "peek",
            "(param i32 i32) (result i32) (local i32) \
               i32.const 1024 local.set 2 \
               (block local.get 1 (if (then local.get 0 local.set 2))) \
               local.get 2 i32.load",
        ),
    );

    // C-2: param-nulling arithmetic. `(addr - addr) == 0`, so `+ 65536` is a
    // fixed host address regardless of the caller's pointer — the two-point
    // lattice cannot prove the operands unequal, so `sub` must not propagate.
    push(
        "C-2 param-nulling arithmetic store",
        mem_main(
            "(type (;0;) (func (param i32 i32)))",
            "poke",
            "(func (;1;) (type 0) (param i32 i32) \
               local.get 0 local.get 1 call 0)",
        ),
        mem_lib(
            "(type (;0;) (func (param i32 i32)))",
            "poke",
            "(param i32 i32) \
               local.get 0 local.get 0 i32.sub i32.const 65536 i32.add \
               local.get 1 i32.store",
        ),
    );

    // C-2b: add-side algebraic cancellation `(C - p) + p == C`. The round-2
    // `sub` rule demotes `const - param` to NotParam, but that value is a
    // *negated* parameter, not a constant; the `add` rule must not re-promote a
    // `Param + NotParam` to Param, or `(C - p) + p` recovers the fixed host
    // address C and aliases the host's own memory at offset 65536 for every
    // caller pointer. End-to-end mirror of the headline reproduction.
    push(
        "C-2b add-side cancellation store",
        mem_main(
            "(type (;0;) (func (param i32 i32)))",
            "poke",
            "(func (;1;) (type 0) (param i32 i32) \
               local.get 0 local.get 1 call 0)",
        ),
        mem_lib(
            "(type (;0;) (func (param i32 i32)))",
            "poke",
            "(param i32 i32) \
               i32.const 65536 local.get 0 i32.sub local.get 0 i32.add \
               local.get 1 i32.store",
        ),
    );

    // C-2b (commuted): p + (C - p) == C, the operand-order mirror, where the
    // param is the first `add` operand and the negated-param NotParam is on top.
    push(
        "C-2b add-side cancellation store (commuted)",
        mem_main(
            "(type (;0;) (func (param i32 i32)))",
            "poke",
            "(func (;1;) (type 0) (param i32 i32) \
               local.get 0 local.get 1 call 0)",
        ),
        mem_lib(
            "(type (;0;) (func (param i32 i32)))",
            "poke",
            "(param i32 i32) \
               local.get 0 i32.const 65536 local.get 0 i32.sub i32.add \
               local.get 1 i32.store",
        ),
    );

    // C-3: a constant address laundered across a `call` boundary. `$peek`
    // discards a const and calls a helper that loads through its own (untrusted)
    // param; the multi-function memory closure must be rejected.
    push(
        "C-3 call-laundered load",
        mem_main(
            "(type (;0;) (func (param i32 i32) (result i32)))",
            "peek",
            "(func (;1;) (type 0) (param i32 i32) (result i32) \
               local.get 0 local.get 1 call 0)",
        ),
        try_wasm(
            "(module (type (;0;) (func (param i32 i32) (result i32))) \
             (type (;1;) (func (param i32) (result i32))) \
             (memory (;0;) 1) \
             (func (;0;) (type 0) (param i32 i32) (result i32) \
               i32.const 1024 call 1) \
             (func (;1;) (type 1) (param i32) (result i32) \
               local.get 0 i32.load) \
             (export \"peek\" (func 0)))",
        ),
    );

    // C-4: a `memory64` external folded onto a memoryless main, addressing its
    // i64 memory directly through its i64 parameter so provenance accepts it and
    // the shape guard — not provenance — is what must fire. The `.wasm` would be
    // a 64-bit machine but the `.v` a 32-bit one, so the merge must reject the
    // shape outright rather than adopt it. The main imports `load_at` from
    // `mathlib` (the module label the sweep tags externals with).
    push(
        "C-4 memory64 external onto memoryless main",
        try_wasm(
            "(module (type (;0;) (func (param i64) (result i64))) \
             (import \"mathlib\" \"load_at\" (func (;0;) (type 0))) \
             (func (;1;) (type 0) (param i64) (result i64) local.get 0 call 0) \
             (export \"run\" (func 1)))",
        ),
        try_wasm(
            "(module (type (;0;) (func (param i64) (result i64))) \
             (memory (;0;) i64 1) \
             (func (;0;) (type 0) (param i64) (result i64) local.get 0 i64.load) \
             (export \"load_at\" (func 0)))",
        ),
    );

    // H-3: a deeply-nested external body the merge must reject before it can
    // later abort the wasm-to-v translator's unbounded recursion.
    if let Some(lib) = {
        let mut body = String::new();
        for _ in 0..5_000 {
            body.push_str("block ");
        }
        for _ in 0..5_000 {
            body.push_str("end ");
        }
        try_wasm(&format!(
            "(module (type (;0;) (func (param i32 i32) (result i32))) \
             (func (;0;) (type 0) (param i32 i32) (result i32) {body} \
               local.get 0 local.get 1 i32.add) \
             (export \"sum\" (func 0)))"
        ))
    } {
        probes.push(Probe {
            label: "H-3 deeply-nested external body",
            main: main_importing_sum(),
            externals: vec![lib],
            expect: Expect::Rejected,
        });
    }

    // M-1: an over-declared locals count (the value a 6-byte locals group can
    // set). The universal pre-validation gate must reject it before provenance
    // sizes a per-local `vec!` — no multi-GB allocation.
    probes.push(Probe {
        label: "M-1 over-declared locals",
        main: main_importing_sum(),
        externals: vec![over_declared_locals_external(u32::MAX)],
        expect: Expect::Rejected,
    });

    // M-2: a main module carrying an active data segment. `emit` rebuilds the
    // main without a data section, so a surviving merge would silently drop the
    // initializer; the guard must reject it.
    probes.push(Probe {
        label: "M-2 main-side data segment",
        main: wasm(
            r#"
            (module
              (type (;0;) (func (param i32 i32) (result i32)))
              (import "mathlib" "sum" (func (;0;) (type 0)))
              (memory (;0;) 1 1)
              (data (;0;) (i32.const 0) "\2a\00\00\00")
              (func (;1;) (type 0) (param i32 i32) (result i32)
                local.get 0 local.get 1 call 0)
              (export "compute" (func 1)))
            "#,
        ),
        externals: vec![mathlib_pure()],
        expect: Expect::Rejected,
    });

    probes
}

/// A deterministic single-byte and truncation sweep over an assembled valid
/// external. Flipping bytes in a real module produces a large family of
/// structurally-broken inputs (bad section lengths, dangling indices, illegal
/// opcodes) that the merge must reject cleanly. The stride keeps the test fast
/// while still covering every section boundary region.
fn mutation_probes() -> Vec<Probe> {
    let main = main_importing_sum();
    let base = mathlib_pure();
    let mut probes = Vec::new();

    // A single-byte flip at every offset of the valid module. The base module is
    // small (tens of bytes), so the full sweep stays fast while covering every
    // section header, length prefix, type, index, and opcode byte.
    for offset in 0..base.len() {
        let mut bytes = base.clone();
        bytes[offset] ^= 0xFF;
        probes.push(Probe {
            label: "byte-flip mutation",
            main: main.clone(),
            externals: vec![bytes],
            // A flipped byte may break a length prefix, an index, or an opcode —
            // or, rarely, leave a still-valid (differently-shaped) module. Either
            // a clean `Err` or a validating `Ok` is acceptable.
            expect: Expect::CleanOutcome,
        });
    }

    // Progressive truncations: a representative set of prefixes of the valid
    // module, so a length prefix can name more bytes than remain.
    for cut in (1..base.len()).step_by(3) {
        probes.push(Probe {
            label: "truncation mutation",
            main: main.clone(),
            externals: vec![base[..cut].to_vec()],
            expect: Expect::CleanOutcome,
        });
    }

    probes
}

#[test]
fn adversarial_corpus_never_panics_and_only_emits_valid_modules() {
    let probes = seed_probes()
        .into_iter()
        .chain(mutation_probes())
        .collect::<Vec<_>>();

    // The sweep must be substantial — guard against a refactor silently emptying
    // the corpus (e.g. a builder helper that starts returning nothing).
    assert!(
        probes.len() > 30,
        "the adversarial corpus is unexpectedly small ({} probes)",
        probes.len()
    );

    let mut merged_ok = 0usize;
    for probe in &probes {
        let pairs: Vec<(&str, &[u8])> = probe
            .externals
            .iter()
            .map(|bytes| ("mathlib", bytes.as_slice()))
            .collect();

        // `link` is panic-free by contract; wrap it so a reintroduced panic fails
        // here with the offending fixture named rather than aborting the run.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            raw_link(&probe.main, &pairs)
        }));

        let result = outcome.unwrap_or_else(|_| {
            panic!("`{}`: link panicked on adversarial input — it must return an Err", probe.label)
        });

        match (&result, probe.expect) {
            // A merge must always be structurally valid, whatever the expectation.
            (Ok(merged), _) => {
                inf_wasmparser::validate(merged).unwrap_or_else(|e| {
                    panic!(
                        "`{}`: link returned Ok but the merged module fails validation: {e}",
                        probe.label
                    )
                });
                // A soundness reproduction that *merges* is a silent miscompile —
                // the worst outcome — even though the bytes validate.
                assert!(
                    probe.expect != Expect::Rejected,
                    "`{}`: link merged a soundness reproduction into a valid-but-wrong \
                     module; it must reject it cleanly",
                    probe.label
                );
                merged_ok += 1;
            }
            // A returned error is correct for `Rejected` and acceptable for
            // `CleanOutcome`, but a regression for a positive control.
            (Err(e), Expect::Merges) => panic!(
                "`{}`: a legitimate probe must merge, got a rejection: {e}",
                probe.label
            ),
            (Err(_), _) => {}
        }
    }

    // At least the pure control must have merged successfully, proving the sweep
    // is not vacuously rejecting everything (which would make the `Ok ⇒ valid`
    // arm untested).
    assert!(
        merged_ok >= 1,
        "no probe merged successfully; the `Ok ⇒ valid` invariant went untested"
    );
}

// -- H-3: deeply-nested external rejected at the merge ----------------------

/// Builds a main importing `deep` and an external `deep` whose body nests
/// `depth` empty `block`s. Mirrors the adversarial external that, merged
/// unchecked, would later overflow the wasm-to-v translator's recursion on the
/// `-v` proof path.
fn deep_nesting_main_and_lib(depth: usize) -> (Vec<u8>, Vec<u8>) {
    let main = wasm(
        r#"
        (module
          (type (;0;) (func))
          (import "deeplib" "deep" (func (;0;) (type 0)))
          (func (;1;) (type 0) call 0)
          (export "run" (func 1)))
        "#,
    );

    let mut body = String::new();
    for _ in 0..depth {
        body.push_str("block ");
    }
    for _ in 0..depth {
        body.push_str("end ");
    }
    let lib = wasm(&format!(
        r#"(module (func (;0;) (export "deep") {body}))"#
    ));
    (main, lib)
}

/// H-3: an external whose body nests structured control flow far past the
/// merge's cap must be rejected with a clean [`LinkError`] on the link/`-o`
/// path — never merged into the output where it would later abort the
/// wasm-to-v translator (an unrecoverable SIGABRT) on the `-v` path.
#[test]
fn deeply_nested_external_body_is_rejected_at_link() {
    let (main, lib) = deep_nesting_main_and_lib(5_000);
    let err = link(&main, &[&lib]).expect_err("a deeply-nested external must be rejected");
    match err {
        LinkError::UnsupportedConstruct(msg) => {
            assert!(
                msg.contains("nests structured control flow"),
                "the diagnostic should name the nesting-depth limit: {msg}"
            );
        }
        other => panic!("expected UnsupportedConstruct for deep nesting, got {other:?}"),
    }
}

/// H-3: an external nested within the cap still merges cleanly, so the guard
/// rejects only pathological depth, never a legitimately nested function.
#[test]
fn external_nested_within_the_cap_merges() {
    let (main, lib) = deep_nesting_main_and_lib(16);
    let linked = link(&main, &[&lib]).expect("a modestly-nested external should merge");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
}

// -- H-4: deterministic name for a nameless merged inner callee -------------

/// H-4: a merged external's nameless inner callee must receive a deterministic
/// name derived from its output function index, so the merged module always
/// carries a complete name section and the downstream `.v` is reproducible.
///
/// The external (built from plain WAT) has no name section, so its inner callee
/// `func 1` starts nameless. The closure root that satisfies the import is
/// renamed to the import field, prefixed with its logical module
/// (`lib.compute`); the inner callee must be filled with `<module>.func_<out_idx>`
/// (`lib.func_2`) rather than left nameless (which previously forced wasm-to-v
/// down a per-process random-UUID path). The `lib.` prefix sanitizes to `lib_`
/// in the downstream Rocq names.
#[test]
fn nameless_merged_inner_callee_gets_deterministic_name() {
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (import "lib" "compute" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32) (result i32)
            local.get 0
            call 0)
          (export "run" (func 1)))
        "#,
    );
    // No name section: the root `compute` (func 0) calls a nameless inner
    // helper (func 1).
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0
            call 1)
          (func (;1;) (type 0) (param i32) (result i32)
            local.get 0
            i32.const 1
            i32.add)
          (export "compute" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("closure with a nameless inner callee should merge");
    assert_valid(&linked);

    let names = function_names(&linked);
    // The closure root is named after the import field it satisfies, prefixed
    // with its logical module.
    assert!(
        names.iter().any(|(_, n)| n == "lib.compute"),
        "the closure root should be named `lib.compute`: {names:?}"
    );
    // Every merged function carries a name (a complete name section), and the
    // nameless inner callee is named deterministically from its output index,
    // under the same `<module>.` namespace as the root — never left out of the
    // section.
    assert!(
        names.iter().any(|(_, n)| n == "lib.func_2"),
        "the nameless inner callee should get a deterministic `lib.func_<idx>` name: {names:?}"
    );
    // No UUID-style name leaks in: a deterministic fallback name is a plain
    // `<module>.func_<idx>` whose suffix after the last `.` parses as an integer.
    for (_, n) in &names {
        if let Some(suffix) = n.rsplit('.').next().and_then(|s| s.strip_prefix("func_")) {
            assert!(
                suffix.parse::<u32>().is_ok(),
                "a `func_`-prefixed fallback name must be index-derived, not a UUID: {n}"
            );
        }
    }
}

// -- H-2 (corrected): verification-only constructs in externals -------------
//
// Inference's non-deterministic blocks (`forall`/`exists`/`assume`/`unique`) and
// uzumaki rvalues (`i32`/`i64.uzumaki`) are verification-only: they have meaning
// only in the Rocq lowering and no executable runtime semantics. When building
// an executable binary, an *external* whose merged-closure body carries one of
// these opcodes would yield a non-executable output (a miscompile), so the
// linker must reject it with a clean `LinkError`. (The main module in proof mode
// legitimately carries these opcodes as proof scaffolding and must pass through
// unaffected — covered by the wasm-to-v proof-path tests.) Separately, spec
// functions and the `inference.spec_funcs` custom section in an external are
// stripped: they are never in the executable closure and are not merged.

/// The single-byte `0xfc` sub-opcode for each Inference non-det block, matching
/// the codegen and `inf-wasmparser` decoder.
const NONDET_SUBOPCODES: &[(u8, &str)] = &[
    (0x3a, "forall"),
    (0x3b, "exists"),
    (0x3c, "assume"),
    (0x3d, "unique"),
];

/// The `0xfc` sub-opcode for each uzumaki rvalue.
const UZUMAKI_SUBOPCODES: &[(u8, &str)] = &[(0x31, "i32.uzumaki"), (0x32, "i64.uzumaki")];

/// Builds an external exporting `sum:(i32,i32)->i32` whose body opens an
/// Inference non-det block (`sub_opcode`) with an empty block type, then
/// computes the sum. `wat` cannot assemble the custom `0xfc`-prefixed opcode, so
/// the body is hand-encoded:
/// `<nondet> (empty) end; local.get 0; local.get 1; i32.add; end`.
///
/// The non-det block is verification-only, so an executable merge of this
/// external must reject it rather than copy it into a non-executable output.
fn lib_with_nondet_block(sub_opcode: u8) -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction, Module,
        TypeSection, ValType,
    };

    let mut module = Module::new();

    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    module.section(&types);

    let mut funcs = FunctionSection::new();
    funcs.function(0);
    module.section(&funcs);

    let mut exports = ExportSection::new();
    exports.export("sum", ExportKind::Func, 0);
    module.section(&exports);

    let mut code = CodeSection::new();
    let mut f = Function::new([]);
    // `<nondet> (empty)` = 0xfc <sub_opcode> 0x40. The empty block has no stack
    // effect, so the surrounding stack stays valid.
    f.raw([0xfc, sub_opcode, 0x40]);
    f.instruction(&Instruction::End); // close the non-det block
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::End);
    code.function(&f);
    module.section(&code);

    module.finish()
}

/// Builds an external exporting `sum:(i32,i32)->i32` whose body contains an
/// uzumaki rvalue (`sub_opcode`), immediately dropped to keep the stack
/// balanced. The uzumaki rvalue is verification-only, so an executable merge of
/// this external must reject it.
fn lib_with_uzumaki(sub_opcode: u8) -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction, Module,
        TypeSection, ValType,
    };

    let mut module = Module::new();

    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    module.section(&types);

    let mut funcs = FunctionSection::new();
    funcs.function(0);
    module.section(&funcs);

    let mut exports = ExportSection::new();
    exports.export("sum", ExportKind::Func, 0);
    module.section(&exports);

    let mut code = CodeSection::new();
    let mut f = Function::new([]);
    // `<uzumaki>` = 0xfc <sub_opcode>; it pushes a value, dropped immediately.
    f.raw([0xfc, sub_opcode]);
    f.instruction(&Instruction::Drop);
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::End);
    code.function(&f);
    module.section(&code);

    module.finish()
}

#[test]
fn external_nondet_block_is_rejected_as_non_executable() {
    // H-2 (corrected): each non-det block in an external's merged-closure body is
    // verification-only, so an executable merge must reject it with a clean
    // `LinkError` rather than copy it into a non-executable output. Covers
    // forall/exists/assume/unique.
    let main = main_importing_sum();
    for &(sub_opcode, name) in NONDET_SUBOPCODES {
        let lib = lib_with_nondet_block(sub_opcode);
        let err = assert_clean_rejection(&main, &lib, name);
        if let LinkError::UnsupportedConstruct(msg) = &err {
            assert!(
                msg.contains("verification-only"),
                "{name}: expected a verification-only rejection, got {msg}"
            );
        }
    }
}

#[test]
fn external_uzumaki_is_rejected_as_non_executable() {
    // H-2 (corrected): each uzumaki rvalue in an external's merged-closure body
    // is verification-only and has no executable semantics, so an executable
    // merge must reject it. Covers i32.uzumaki and i64.uzumaki.
    let main = main_importing_sum();
    for &(sub_opcode, name) in UZUMAKI_SUBOPCODES {
        let lib = lib_with_uzumaki(sub_opcode);
        let err = assert_clean_rejection(&main, &lib, name);
        if let LinkError::UnsupportedConstruct(msg) = &err {
            assert!(
                msg.contains("verification-only"),
                "{name}: expected a verification-only rejection, got {msg}"
            );
        }
    }
}

#[test]
fn external_nondet_functype_block_is_rejected_as_non_executable() {
    // H-2 (corrected): the function-typed non-det form is rejected identically to
    // the empty form — the construct is verification-only regardless of its block
    // type, so the merge never even reaches the block-type remap. The fixture's
    // `forall (type 1)` would, under the old (now-wrong) semantics, have been
    // remapped and copied; it must now reject cleanly.
    use wasm_encoder::{
        CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction, Module,
        TypeSection, ValType,
    };

    let lib = {
        let mut module = Module::new();
        let mut types = TypeSection::new();
        types
            .ty()
            .function([ValType::I32, ValType::I32], [ValType::I32]);
        types.ty().function([], []);
        module.section(&types);
        let mut funcs = FunctionSection::new();
        funcs.function(0);
        module.section(&funcs);
        let mut exports = ExportSection::new();
        exports.export("sum", ExportKind::Func, 0);
        module.section(&exports);
        let mut code = CodeSection::new();
        let mut f = Function::new([]);
        // `forall (type 1)` = 0xfc 0x3a <s33 = 1>.
        f.raw([0xfc, 0x3a, 0x01]);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::LocalGet(1));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::End);
        code.function(&f);
        module.section(&code);
        module.finish()
    };

    let main = main_importing_sum();
    assert_clean_rejection(&main, &lib, "forall function-typed block");
}

/// Builds an external exporting an executable `sum:(i32,i32)->i32` that ALSO
/// carries (1) a separate spec function with a non-det body, and (2) an
/// `inference.spec_funcs` custom section naming it. `sum` does not call the spec
/// function, so the spec function is outside the executable closure: merging
/// `sum` must strip both the spec function and the spec section, with no error.
fn lib_with_spec_function_and_section() -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, CustomSection, ExportKind, ExportSection, Function, FunctionSection, Instruction,
        Module, TypeSection, ValType,
    };

    let mut module = Module::new();

    let mut types = TypeSection::new();
    // type 0: the `sum` signature; type 1: the spec function signature `()->()`.
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    types.ty().function([], []);
    module.section(&types);

    let mut funcs = FunctionSection::new();
    funcs.function(0); // func 0: sum
    funcs.function(1); // func 1: spec (verification-only body)
    module.section(&funcs);

    // Only `sum` is exported, so only it (and its closure) can be merged.
    let mut exports = ExportSection::new();
    exports.export("sum", ExportKind::Func, 0);
    module.section(&exports);

    let mut code = CodeSection::new();
    // func 0: executable sum, no verification-only opcodes.
    let mut sum = Function::new([]);
    sum.instruction(&Instruction::LocalGet(0));
    sum.instruction(&Instruction::LocalGet(1));
    sum.instruction(&Instruction::I32Add);
    sum.instruction(&Instruction::End);
    code.function(&sum);
    // func 1: spec body carrying a `forall` block — legal in a spec function,
    // never executed, and never pulled into `sum`'s closure.
    let mut spec = Function::new([]);
    spec.raw([0xfc, 0x3a, 0x40]); // forall (empty)
    spec.instruction(&Instruction::End); // close forall
    spec.instruction(&Instruction::End); // close function
    code.function(&spec);
    module.section(&code);

    // An `inference.spec_funcs` section naming the spec function (index 1).
    // version=1, count=1, name_len=1 'S', idx_count=1, idx=1.
    let spec_section_payload = [1u8, 1, 1, b'S', 1, 1];
    module.section(&CustomSection {
        name: "inference.spec_funcs".into(),
        data: (&spec_section_payload[..]).into(),
    });

    module.finish()
}

#[test]
fn external_spec_function_and_section_are_stripped_when_building_an_executable() {
    // (1) An external that ALSO contains a spec function (verification-only body)
    // and an `inference.spec_funcs` section must link successfully when building
    // an executable: the spec function is outside the executable closure of the
    // satisfied export, so it is not merged, and the spec section is stripped —
    // no error on its presence.
    let main = main_importing_sum();
    let lib = lib_with_spec_function_and_section();

    let linked = link(&main, &[&lib]).expect("an external with specs must link, with specs stripped");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());

    // The merged output must carry NO `inference.spec_funcs` section from the
    // external (the main module here has none either, so the section is absent).
    assert!(
        custom_section_data(&linked, "inference.spec_funcs").is_none(),
        "an external's spec section must not be merged into the executable output"
    );

    // No merged function body may carry a verification-only opcode: the spec
    // function (with its `forall`) must have been stripped, not merged.
    for payload in Parser::new(0).parse_all(&linked) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            for op in body.get_operators_reader().unwrap() {
                assert!(
                    !matches!(
                        op.unwrap(),
                        Operator::Forall { .. }
                            | Operator::Exists { .. }
                            | Operator::Assume { .. }
                            | Operator::Unique { .. }
                            | Operator::I32Uzumaki { .. }
                            | Operator::I64Uzumaki { .. }
                    ),
                    "no merged executable body may carry a verification-only opcode"
                );
            }
        }
    }
}

#[test]
fn external_malformed_spec_section_does_not_fail_the_link() {
    // An external's spec section is stripped, so even a *malformed* one must not
    // fail the link: it is irrelevant to the executable merge. Build a valid
    // executable `sum` external, then append a garbage `inference.spec_funcs`
    // section (a bogus version byte the main-module decoder would reject).
    use wasm_encoder::{
        CodeSection, CustomSection, ExportKind, ExportSection, Function, FunctionSection, Instruction,
        Module, TypeSection, ValType,
    };

    let lib = {
        let mut module = Module::new();
        let mut types = TypeSection::new();
        types
            .ty()
            .function([ValType::I32, ValType::I32], [ValType::I32]);
        module.section(&types);
        let mut funcs = FunctionSection::new();
        funcs.function(0);
        module.section(&funcs);
        let mut exports = ExportSection::new();
        exports.export("sum", ExportKind::Func, 0);
        module.section(&exports);
        let mut code = CodeSection::new();
        let mut f = Function::new([]);
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::LocalGet(1));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::End);
        code.function(&f);
        module.section(&code);
        // A garbage spec section: version byte 0xff, which the main decoder would
        // reject as an unsupported version. For an external it is simply skipped.
        module.section(&CustomSection {
            name: "inference.spec_funcs".into(),
            data: (&[0xffu8, 0xff, 0xff][..]).into(),
        });
        module.finish()
    };

    let main = main_importing_sum();
    let linked =
        link(&main, &[&lib]).expect("a malformed external spec section must not fail the link");
    assert_valid(&linked);
}

#[test]
fn malformed_main_spec_section_fails_the_link() {
    // COV-4: the *main* module's `inference.spec_funcs` section IS decoded (it
    // drives proof-mode translation and is re-emitted re-indexed), so a malformed
    // one — here a bogus version byte 0xff — reaching `link()` must be a hard
    // `LinkError::Parse`, not silently dropped. This mirrors
    // `external_malformed_spec_section_does_not_fail_the_link` for the side that
    // is actually decoded.
    let main_wat = r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0)
          (export "compute" (func 1)))
        "#;
    let mut main = wasm(main_wat);
    use wasm_encoder::Section as _;
    wasm_encoder::CustomSection {
        name: "inference.spec_funcs".into(),
        data: (&[0xffu8, 0xff, 0xff][..]).into(),
    }
    .append_to(&mut main);

    let lib = mathlib_pure();
    let err = link(&main, &[&lib])
        .expect_err("a malformed main spec section must be a hard link error");
    assert!(
        matches!(&err, LinkError::Parse(_)),
        "expected a Parse error for the malformed main spec section, got {err:?}"
    );
}

/// Builds a proof-mode MAIN module that imports `sum` and whose own exported
/// body carries verification-only opcodes (a `forall` block and an
/// `i32.uzumaki`) as Rocq proof scaffolding, alongside an executable `call` to
/// the import. `wat` cannot assemble the custom opcodes, so the whole module is
/// hand-encoded.
fn proof_mode_main_with_nondet_and_uzumaki() -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, ImportSection,
        Instruction, Module, TypeSection, ValType,
    };

    let mut module = Module::new();

    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    module.section(&types);

    let mut imports = ImportSection::new();
    imports.import("mathlib", "sum", EntityType::Function(0));
    module.section(&imports);

    let mut funcs = FunctionSection::new();
    funcs.function(0); // the main local function (output index 0 after the import is removed)
    module.section(&funcs);

    let mut exports = ExportSection::new();
    exports.export("compute", ExportKind::Func, 1); // import is 0, local is 1
    module.section(&exports);

    let mut code = CodeSection::new();
    let mut f = Function::new([]);
    // Proof scaffolding: a `forall (empty)` block and an `i32.uzumaki` (dropped),
    // both verification-only and legal in the main module.
    f.raw([0xfc, 0x3a, 0x40]); // forall (empty)
    f.instruction(&Instruction::End); // close forall
    f.raw([0xfc, 0x31]); // i32.uzumaki
    f.instruction(&Instruction::Drop);
    // Executable tail: sum(arg0, arg1) via the (to-be-merged) import.
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::Call(0)); // call the imported `sum`
    f.instruction(&Instruction::End);
    code.function(&f);
    module.section(&code);

    module.finish()
}

#[test]
fn proof_mode_main_nondet_and_uzumaki_survive_the_merge() {
    // (c): a proof-mode MAIN module carrying non-det/uzumaki opcodes that links a
    // plain executable external must still compile, and its verification-only
    // opcodes must survive into the linked output unaltered (they are Rocq proof
    // scaffolding the merge must not strip, reject, or alter — only the MAIN
    // module is exempt; externals are rejected).
    let main = proof_mode_main_with_nondet_and_uzumaki();
    let lib = mathlib_pure();

    let linked = link(&main, &[&lib]).expect("proof-mode main with non-det/uzumaki must link");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());

    let mut saw_forall = false;
    let mut saw_uzumaki = false;
    for payload in Parser::new(0).parse_all(&linked) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            for op in body.get_operators_reader().unwrap() {
                match op.unwrap() {
                    Operator::Forall { .. } => saw_forall = true,
                    Operator::I32Uzumaki { .. } => saw_uzumaki = true,
                    _ => {}
                }
            }
        }
    }
    assert!(
        saw_forall,
        "the main module's `forall` proof scaffolding must survive the merge"
    );
    assert!(
        saw_uzumaki,
        "the main module's `i32.uzumaki` proof scaffolding must survive the merge"
    );
}

// -- M-1 / M-2: the public `link` API is self-defending -----------------------
//
// `link` is an entry point in its own right; its contract previously only
// *assumed* pre-validated externals (the CLI driver validates, the library API
// did not). These tests pin the two universal backstops: a structural
// pre-validation gate over every external (M-1) and a main-side data/element
// guard (M-2).

/// Appends `value` as a little-endian base-128 (unsigned LEB128) varint, the
/// encoding WASM uses for counts and indices.
fn push_uleb(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

/// Wraps `section_bytes` in a section with the given `id`, prefixing the byte
/// length WASM section framing requires.
fn framed_section(id: u8, section_bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![id];
    push_uleb(&mut out, section_bytes.len() as u32);
    out.extend_from_slice(section_bytes);
    out
}

/// Hand-assembles a memory-using external exporting `sum : (i32, i32) -> i32`
/// whose single function over-declares its locals count as `locals_count`. A
/// real assembler cannot emit this — `wat`/`wasm_encoder` compute the locals
/// header from the declared types — so the code section is written byte-by-byte.
///
/// With `locals_count = u32::MAX` this is the M-1 reproduction: the value a
/// 6-byte locals group can set, which the provenance interpreter would once have
/// turned into a ~4.3 GB `vec!`. The module is deliberately *invalid* WASM (the
/// declared locals do not exist), so the universal pre-validation gate must
/// reject it before any byte reaches provenance.
fn over_declared_locals_external(locals_count: u32) -> Vec<u8> {
    // Type section: one `(func (param i32 i32) (result i32))`.
    let type_section = framed_section(
        0x01,
        &[
            0x01, // one type
            0x60, // func
            0x02, 0x7f, 0x7f, // two i32 params
            0x01, 0x7f, // one i32 result
        ],
    );

    // Function section: one function of type 0.
    let function_section = framed_section(0x03, &[0x01, 0x00]);

    // Memory section: one memory, min 1 page (so the body's `i32.load` is
    // structurally placed against a real memory).
    let memory_section = framed_section(0x05, &[0x01, 0x00, 0x01]);

    // Export section: `sum` -> func 0.
    let mut export_payload = vec![0x01]; // one export
    push_uleb(&mut export_payload, 3); // name length ("sum")
    export_payload.extend_from_slice(b"sum");
    export_payload.push(0x00); // kind: func
    export_payload.push(0x00); // func index 0
    let export_section = framed_section(0x07, &export_payload);

    // Code section: one body whose single locals group claims `locals_count`
    // i32 locals, then loads from address 0 and returns a constant. The
    // over-declaration is the payload.
    let mut body = Vec::new();
    body.push(0x01); // one locals group
    push_uleb(&mut body, locals_count); // (count, i32) — the over-declaration
    body.push(0x7f); // i32
    body.extend_from_slice(&[0x41, 0x00]); // i32.const 0
    body.extend_from_slice(&[0x28, 0x02, 0x00]); // i32.load (align 2, offset 0)
    body.push(0x1a); // drop
    body.extend_from_slice(&[0x41, 0x00]); // i32.const 0 (the i32 result)
    body.push(0x0b); // end

    let mut code_payload = vec![0x01]; // one code entry
    push_uleb(&mut code_payload, body.len() as u32); // body size
    code_payload.extend_from_slice(&body);
    let code_section = framed_section(0x0a, &code_payload);

    let mut module = Vec::new();
    module.extend_from_slice(b"\0asm"); // magic
    module.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]); // version 1
    module.extend_from_slice(&type_section);
    module.extend_from_slice(&function_section);
    module.extend_from_slice(&memory_section);
    module.extend_from_slice(&export_section);
    module.extend_from_slice(&code_section);
    module
}

#[test]
fn over_declared_locals_external_via_link_is_rejected_without_huge_alloc() {
    // M-1 on the public library path: a tiny external whose single locals group
    // claims `u32::MAX` locals, linked through the public `link` API. The
    // universal pre-validation gate must reject it as a clean `LinkError` before
    // the provenance interpreter would size a per-local `vec!` — no multi-GB
    // allocation, no panic, no hang.
    let main = main_importing_sum();
    let lib = over_declared_locals_external(u32::MAX);

    let err = link(&main, &[&lib]).expect_err("an over-declared-locals external must be rejected");
    assert!(
        matches!(err, LinkError::Parse(_)),
        "expected a clean Parse rejection from the pre-validation gate, got {err:?}"
    );
}

#[test]
fn main_module_with_a_data_segment_is_rejected_cleanly() {
    // M-2: a main module carrying an active data segment must be rejected, not
    // silently merged. `emit` rebuilds the main module without a `DataSection`,
    // so a surviving merge would drop the initializer — a valid-but-wrong
    // `.wasm`/`.v`. The guard rejects up front with a clean diagnostic.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (memory (;0;) 1 1)
          (data (;0;) (i32.const 0) "\2a\00\00\00")
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0)
          (export "compute" (func 1)))
        "#,
    );
    let lib = mathlib_pure();

    let err = link(&main, &[&lib]).expect_err("a main-side data segment must be rejected");
    assert!(
        matches!(&err, LinkError::UnsupportedConstruct(msg) if msg.contains("data segment")),
        "expected an UnsupportedConstruct naming the main-side data segment, got {err:?}"
    );
}

#[test]
fn main_module_with_an_element_segment_is_rejected_cleanly() {
    // M-2 (element half): a main module carrying an element segment must be
    // rejected too. `emit` omits both the main `TableSection` and any
    // `ElementSection`, so a surviving merge would orphan the element's table
    // reference. The guard rejects it as a clean diagnostic rather than relying
    // on the post-merge validate gate to catch the orphan.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (table (;0;) 1 1 funcref)
          (elem (;0;) (i32.const 0) func 1)
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0)
          (export "compute" (func 1)))
        "#,
    );
    let lib = mathlib_pure();

    let err = link(&main, &[&lib]).expect_err("a main-side element segment must be rejected");
    assert!(
        matches!(&err, LinkError::UnsupportedConstruct(msg) if msg.contains("element segment")),
        "expected an UnsupportedConstruct naming the main-side element segment, got {err:?}"
    );
}

// -- WASM 1.0 feature gate: supported post-MVP additions link ------------------
//
// `SUPPORTED_WASM_FEATURES` is the integer WASM 1.0 core plus exactly one scalar
// post-MVP addition the merge models: bulk memory. An external using only this
// must pass the link gate and merge normally — the gate rejects *every* other
// post-1.0 proposal, including sign-extension and saturating float-to-int (the
// Rocq translator models neither), and all floating point (the Inference language
// has no `f32`/`f64` types).

/// A main module importing a pure `f:(i32)->i32` from `lib` and calling it. The
/// shared shape for the feature-gate fixtures, each of which supplies a `lib`
/// exporting `f` whose body exercises one post-MVP op.
fn main_importing_f() -> Vec<u8> {
    wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (import "lib" "f" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32) (result i32)
            local.get 0
            call 0)
          (export "run" (func 1)))
        "#,
    )
}

#[test]
fn sign_extension_external_is_rejected_at_the_feature_gate() {
    // The sign-extension proposal (`i32.extend8_s`) is outside the supported
    // subset: the Rocq translator has no lowering for it, and Inference codegen
    // narrows sub-i32 values with shifts/masks instead of emitting it. The gate's
    // feature pass rejects such an external up front with the validator's
    // sign-extension diagnostic — before the closure scanner's allow-list (the
    // defense-in-depth backstop, tested directly in `safety.rs`) is reached.
    let main = main_importing_f();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0
            i32.extend8_s)
          (export "f" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "sign extension");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("sign extension")),
        "expected an UnsupportedWasmFeature naming sign extension, got {err:?}"
    );
}

#[test]
fn saturating_float_to_int_external_is_rejected_at_the_feature_gate() {
    // The saturating float-to-int proposal (`i32.trunc_sat_f32_s`) is outside the
    // supported subset: the Rocq translator has no lowering for it, and its
    // operand is a float — and the Inference language has no `f32`/`f64` types.
    // The body takes an f32 and returns an i32, so the gate rejects it on the
    // float type first; the validator names floating point.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param f32) (result i32)))
          (import "lib" "f" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param f32) (result i32)
            local.get 0
            call 0)
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param f32) (result i32)))
          (func (;0;) (type 0) (param f32) (result i32)
            local.get 0
            i32.trunc_sat_f32_s)
          (export "f" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "saturating float-to-int");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("floating-point")),
        "expected an UnsupportedWasmFeature naming floating point, got {err:?}"
    );
}

#[test]
fn bulk_memory_copy_external_passes_the_gate_and_merges() {
    // The bulk-memory proposal (`memory.copy` over the single shared memory) is in
    // the supported subset. The external declares the memory and copies a region;
    // the param-addressed copy is Tier B and folds onto the shared memory.
    //
    // The copy's dest, src, AND length are all caller-passed (`copy(dst, src,
    // len)`): under the S1 extent rule a constant copy length would reject at
    // Tier B (an unbounded clobber above the caller's pointer), so the realistic
    // caller-owns-`(ptr, len)` shape is what keeps the bulk-memory opcode
    // mergeable. The assertion remains that the bulk-memory opcode passes the
    // WASM-1.0 feature gate and the body links.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32 i32)))
          (import "lib" "f" (func (;0;) (type 0)))
          (memory (;0;) 1)
          (func (;1;) (type 0) (param i32 i32 i32)
            local.get 0
            local.get 1
            local.get 2
            call 0)
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32 i32)))
          (memory (;0;) 1)
          (func (;0;) (type 0) (param i32 i32 i32)
            local.get 0
            local.get 1
            local.get 2
            memory.copy)
          (export "f" (func 0)))
        "#,
    );
    let linked = link(&main, &[&lib]).expect("a bulk-memory external must link");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    assert_eq!(code_body_count(&linked), 2);
}

#[test]
fn plain_mvp_external_passes_the_gate_and_merges() {
    // A baseline: a pure MVP external (no post-MVP op at all) passes the gate.
    let main = main_importing_f();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0
            i32.const 1
            i32.add)
          (export "f" (func 0)))
        "#,
    );
    let linked = link(&main, &[&lib]).expect("a plain MVP external must link");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    assert_eq!(code_body_count(&linked), 2);
}

// -- WASM 1.0 feature gate: post-1.0 proposals are rejected naming the feature --
//
// Each negative case below proves a distinct post-1.0 proposal is rejected *at
// the gate* (before the merge's per-operator/per-section backstops) with a
// feature-named `UnsupportedWasmFeature`. The atomics/SIMD/reference-types/
// exceptions/memory64/multi-memory/tail-call cases live with the rejection-helper
// tests above (each updated to the gate outcome); this section adds the
// multi-value proposal, which has no per-operator backstop of its own (a
// multi-result block is structurally valid MVP-shaped bytes the allow-list copies
// verbatim) and so relies on the gate as its sole filter.

#[test]
fn multi_value_block_external_is_rejected_at_the_feature_gate() {
    // The multi-value proposal lets a block reference a *type index* (so it can
    // take params, not just produce a single inline result). The function-typed
    // `block (type 1) (param i32) (result i32)` below is well-formed under the
    // parser's default features but outside the supported WASM 1.0 subset, so the
    // gate's feature pass rejects it naming multi-value. Unlike SIMD/atomics/etc.,
    // multi-value carries no distinguishing opcode the allow-list could catch — the
    // gate is its only filter, which is precisely why the explicit feature contract
    // matters here.
    let main = main_importing_f();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (type (;1;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0
            (block (type 1) (param i32) (result i32)))
          (export "f" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "multi-value block");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("multi-value")),
        "expected an UnsupportedWasmFeature naming multi-value, got {err:?}"
    );
}

#[test]
fn multi_result_function_external_is_rejected_at_the_feature_gate() {
    // The multi-value proposal also lets a *function* return more than one value.
    // A `(result i32 i32)` function signature is well-formed under default
    // features but outside the supported subset, so the gate rejects it naming
    // multi-value at the type-section level — before any body is scanned.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32 i32)))
          (import "lib" "f" (func (;0;) (type 0)))
          (type (;1;) (func (param i32) (result i32)))
          (func (;1;) (type 1) (param i32) (result i32)
            local.get 0
            call 0
            drop)
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32 i32)))
          (func (;0;) (type 0) (param i32) (result i32 i32)
            local.get 0
            local.get 0)
          (export "f" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "multi-result function");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("multi-value")),
        "expected an UnsupportedWasmFeature naming multi-value, got {err:?}"
    );
}

// -- Floating point: no f32/f64 anywhere, rejected at the gate ----------------
//
// The Inference language has no `f32`/`f64` types: codegen never emits a float
// operator, value type, or constant, and the Rocq translator models none. The
// feature gate (`SUPPORTED_WASM_FEATURES`) drops the fork's baseline `FLOATS`
// flag, so the validator rejects, at the feature pass, any float instruction
// ("floating-point instruction disallowed") and any float value type in a
// signature, local, or global ("floating-point support is disabled"). Each case
// below proves a distinct float surface — operator, signature, local, global,
// constant — is rejected *at the gate* with a feature-named `UnsupportedWasmFeature`
// naming floating point, before the per-opcode / value-type backstops in the
// merge are reached.

#[test]
fn float_op_external_is_rejected_at_the_feature_gate() {
    // A float *operator* in the external body. The signature stays integer so the
    // rejection is attributable to the operator, not the type.
    let main = main_importing_f();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32) (result i32)
            f32.const 1
            f32.const 1
            f32.add
            drop
            local.get 0)
          (export "f" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "float operator");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("floating-point")),
        "expected an UnsupportedWasmFeature naming floating point, got {err:?}"
    );
}

#[test]
fn float_in_signature_only_external_is_rejected_at_the_feature_gate() {
    // A float appears *only* in a reachable function's signature — no float
    // operator anywhere. The reachable `(param f64) (result i32)` helper sits
    // behind an i32 root, so a signature-blind upstream check could admit it; the
    // gate must still reject on the float value type in the signature.
    let main = main_importing_f();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (type (;1;) (func (param f64) (result i32)))
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0)
          (func (;1;) (type 1) (param f64) (result i32)
            i32.const 0)
          (export "f" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "float in signature");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("floating-point")),
        "expected an UnsupportedWasmFeature naming floating point, got {err:?}"
    );
}

#[test]
fn float_local_only_external_is_rejected_at_the_feature_gate() {
    // A float appears only as a *local* — no float operator, no float in any
    // signature. The gate rejects on the float value type of the local.
    let main = main_importing_f();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32) (result i32)
            (local f32)
            local.get 0)
          (export "f" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "float local");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("floating-point")),
        "expected an UnsupportedWasmFeature naming floating point, got {err:?}"
    );
}

#[test]
fn float_global_external_is_rejected_at_the_feature_gate() {
    // An `f32` global declared in the external. The gate rejects on the float
    // value type of the global, before the global-collection chokepoint in
    // `parse::collect_global` is reached.
    let main = main_importing_f();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (global (;0;) f32 (f32.const 1))
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0)
          (export "f" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "float global");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("floating-point")),
        "expected an UnsupportedWasmFeature naming floating point, got {err:?}"
    );
}

#[test]
fn float_const_only_external_is_rejected_at_the_feature_gate() {
    // A lone `f64.const` (immediately dropped) with an otherwise-integer
    // signature. The float constant is itself a float instruction, so the gate's
    // feature pass rejects it.
    let main = main_importing_f();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32) (result i32)
            f64.const 1
            drop
            local.get 0)
          (export "f" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "float const");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("floating-point")),
        "expected an UnsupportedWasmFeature naming floating point, got {err:?}"
    );
}

#[test]
fn main_module_carrying_a_float_is_rejected_cleanly() {
    // The MAIN module is not passed through the feature gate (it is the linker's
    // own codegen output on the live pipeline), but the public `link()` API
    // accepts arbitrary main bytes. A main carrying a float operator must still be
    // rejected with a clean `LinkError` — never a panic, never a silent merge.
    // The allow-list backstop on the main re-encode path catches the float op.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (import "lib" "f" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32) (result i32)
            f32.const 1
            f32.const 1
            f32.add
            drop
            local.get 0
            call 0)
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0)
          (export "f" (func 0)))
        "#,
    );
    match link(&main, &[&lib]) {
        Ok(bytes) => panic!(
            "a float-carrying main silently produced a {}-byte module; it must be rejected",
            bytes.len()
        ),
        Err(LinkError::UnsupportedConstruct(msg)) => {
            assert!(
                msg.contains("floating-point"),
                "expected a floating-point UnsupportedConstruct, got {msg:?}"
            );
        }
        Err(other) => panic!("expected a floating-point UnsupportedConstruct, got {other:?}"),
    }
}

// -- COV-2 (D3): GC and stack-switching externals are rejected ---------------
//
// `SUPPORTED_WASM_FEATURES` names the fork's baseline `GC_TYPES` value-type flag
// directly, but admits NO GC *proposal* construct: a GC type still needs the `GC`
// feature, which is off. These tests pin the ACTUAL
// rejection layer determined empirically — the GC type is caught by the gate's
// feature pass naming `gc`, the stack-switching construct by the gate's
// structural pass (continuation types are off even under default features).

#[test]
fn gc_typed_external_is_rejected_naming_the_gc_feature() {
    // A GC `struct` type is well-formed under the parser's default features but
    // needs the `GC` proposal to validate, which is outside the supported subset.
    // The gate's feature pass rejects it with an `UnsupportedWasmFeature` naming
    // `gc`, before any body or opcode is examined.
    let main = main_importing_f();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (struct (field i32)))
          (type (;1;) (func (param i32) (result i32)))
          (func (;0;) (type 1) (param i32) (result i32)
            local.get 0)
          (export "f" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "gc struct type");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("gc")),
        "expected an UnsupportedWasmFeature naming the gc feature, got {err:?}"
    );
}

#[test]
fn externref_typed_external_is_rejected_naming_reference_types() {
    // A GC reference value (`externref`, produced by `ref.null extern`) needs the
    // reference-types proposal, which is off. The gate's feature pass rejects it
    // naming reference types — a GC/reference *value type*, not an instruction the
    // allow-list would see.
    let main = main_importing_f();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32) (result i32)
            ref.null extern
            drop
            local.get 0)
          (export "f" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "externref value");
    assert!(
        matches!(&err, LinkError::UnsupportedWasmFeature { details, .. } if details.contains("reference types")),
        "expected an UnsupportedWasmFeature naming reference types, got {err:?}"
    );
}

#[test]
fn stack_switching_external_is_rejected_naming_stack_switching() {
    // The stack-switching proposal's continuation types are off even under the
    // parser's *default* features in this fork, so a module declaring `(cont 0)`
    // fails the gate's STRUCTURAL pass — surfaced as `Parse` naming stack
    // switching, not as a feature-pass `UnsupportedWasmFeature`. The external
    // exports a `(func)`-typed `f`, matching a dedicated no-arg main.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func))
          (import "lib" "f" (func (;0;) (type 0)))
          (func (;1;) (type 0)
            call 0)
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func))
          (type (;1;) (cont 0))
          (func (;0;) (type 0))
          (export "f" (func 0)))
        "#,
    );
    let err = assert_clean_rejection(&main, &lib, "stack-switching continuation type");
    assert!(
        matches!(&err, LinkError::Parse(msg) if msg.contains("stack switching")),
        "expected a Parse error naming stack switching, got {err:?}"
    );
}

// -- COV-7 (D4): structural validation runs before the feature pass ----------

#[test]
fn malformed_post_1_0_external_is_parse_not_unsupported_feature() {
    // COV-7: lock the two-pass ordering (structural-before-feature). The external
    // uses a post-1.0 feature (SIMD `v128`) AND is structurally broken (its body
    // returns an i32 where a `v128` result is declared). The structural pass runs
    // first under the parser's default features (which include SIMD), so it
    // catches the type mismatch and reports `Parse` — NOT `UnsupportedWasmFeature`.
    // Were the order reversed, the restricted feature pass (no SIMD) would reject
    // naming SIMD first, masking the real structural defect.
    use inference_wasm_linker::validate_external;

    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (result v128)))
          (func (;0;) (type 0) (result v128)
            i32.const 0)
          (export "f" (func 0)))
        "#,
    );

    // At the validate_external entry: a Parse, naming the structural mismatch.
    let direct = validate_external("lib", &lib)
        .expect_err("a structurally-broken external must be rejected");
    assert!(
        matches!(&direct, LinkError::Parse(_)),
        "structural validation must run first, reporting Parse, got {direct:?}"
    );

    // And through the full `link` path: the same structural-first ordering holds.
    let main = main_importing_f();
    let via_link = link(&main, &[&lib])
        .expect_err("a structurally-broken external must fail the link");
    assert!(
        matches!(&via_link, LinkError::Parse(_)),
        "the link entry must report the structural defect as Parse, not a feature name, got {via_link:?}"
    );
}
