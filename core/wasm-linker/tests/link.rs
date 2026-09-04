//! Integration tests for the static-merge linker.
//!
//! Each test builds its `.wasm` fixtures from inline WAT (via the `wat` crate),
//! links them, and asserts on the unified module: structural validity (through
//! `inf-wasmparser`'s validator), absence of cross-module imports, the merged
//! function bodies, and the precise rejection for Tier-C inputs.

use inf_wasmparser::{ExternalKind, Operator, Parser, Payload, TypeRef};
use inference_wasm_linker::{
    link as raw_link, link_with_warnings as raw_link_with_warnings, LinkError, LinkOutput,
    LinkWarning,
};

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
///
/// Every call in this file takes the **unchecked** write-set mode, and it is the
/// only coherent one: these fixtures are hand-written WAT with no Inference
/// source behind them, so there is no `external fn` declaration whose `mut`
/// annotations a contract could be derived from. The checked mode has its own
/// tests below, which supply contracts by hand, and is exercised end to end
/// through the `infc` pipeline in `inference-tests`.
fn link(main: &[u8], libs: &[&[u8]]) -> Result<Vec<u8>, LinkError> {
    let module = sole_import_module(main);
    let pairs: Vec<(&str, &[u8])> = libs.iter().map(|b| (module.as_str(), *b)).collect();
    raw_link(main, &pairs, None)
}

/// [`link`] through the warning-carrying entry point, for tests whose subject is
/// what the merge reports rather than what it emits.
fn link_with_warnings(main: &[u8], libs: &[&[u8]]) -> Result<LinkOutput, LinkError> {
    let module = sole_import_module(main);
    let pairs: Vec<(&str, &[u8])> = libs.iter().map(|b| (module.as_str(), *b)).collect();
    raw_link_with_warnings(main, &pairs, None)
}

/// The single logical module `main` imports from, or the empty string when it
/// imports nothing — a no-import main links any externals away to nothing, so
/// the label is irrelevant there.
fn sole_import_module(main: &[u8]) -> String {
    let modules: std::collections::BTreeSet<String> = function_imports(main)
        .into_iter()
        .map(|(module, _)| module)
        .collect();
    modules.into_iter().next().unwrap_or_default()
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
        names.contains(&(1, "mathlib::sum".to_string())),
        "merged sum must be named after its module-prefixed import field, got {names:?}"
    );
    assert!(
        names.contains(&(2, "mathlib::sub".to_string())),
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
        names.contains(&(1, "mathlib::sum".to_string())),
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

// -- Declared but unused: lld boilerplate does not force Tier C --------------

/// The first operator in body `func_idx` that names the global or table index
/// space, rendered for the failure message, or `None` if the body names
/// neither. `func_idx` of `None` scans every body.
///
/// Used on merges where no module contributes a global and the merge preserves
/// no table section, to confirm that admitting an external which *declares*
/// either leaves no operator behind that could resolve against the wrong module.
/// Where an external legitimately contributes globals, [`body_global_indices`]
/// is the sharper instrument: what matters there is not that the operator is
/// absent but that its operand was remapped.
fn body_naming_a_global_or_table(bytes: &[u8], func_idx: Option<usize>) -> Option<String> {
    let mut idx = 0usize;
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if func_idx.is_none_or(|wanted| wanted == idx) {
                for op in body.get_operators_reader().unwrap() {
                    let op = op.unwrap();
                    if matches!(
                        op,
                        Operator::GlobalGet { .. }
                            | Operator::GlobalSet { .. }
                            | Operator::CallIndirect { .. }
                            | Operator::TableGet { .. }
                            | Operator::TableSet { .. }
                            | Operator::TableGrow { .. }
                            | Operator::TableSize { .. }
                            | Operator::TableFill { .. }
                            | Operator::RefFunc { .. }
                    ) {
                        return Some(format!("{op:?}"));
                    }
                }
            }
            idx += 1;
        }
    }
    None
}

/// The `("get" | "set", global index)` of every global accessor in body
/// `func_idx`, in operator order.
///
/// The operand is the whole point. A merged external's `global.get 0` copied
/// verbatim still names *a* global, and where main declares one of the same type
/// the merged module validates and runs — so a test that only asserted the
/// operator survived, or only counted the module's globals, would pass on the
/// exact miscompile the remap exists to prevent. Only the index distinguishes
/// them.
fn body_global_indices(bytes: &[u8], func_idx: usize) -> Vec<(&'static str, u32)> {
    let mut idx = 0usize;
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if idx == func_idx {
                return body
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .filter_map(|op| match op.unwrap() {
                        Operator::GlobalGet { global_index } => Some(("get", global_index)),
                        Operator::GlobalSet { global_index } => Some(("set", global_index)),
                        _ => None,
                    })
                    .collect();
            }
            idx += 1;
        }
    }
    panic!("no body at index {func_idx}");
}

#[test]
fn external_with_an_unused_stack_pointer_global_links() {
    // Every `cargo build --target wasm32-unknown-unknown` artifact carries an
    // lld-synthesized `__stack_pointer` mutable global. A leaf integer function
    // never reads it, so the declaration alone must not reject the link: Tier C
    // is about what the closure *uses*, not what its module declares.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (global $__stack_pointer (mut i32) (i32.const 1048576))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("an unread __stack_pointer must not block the link");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    assert_eq!(code_body_count(&linked), 2);
    assert_eq!(body_call_targets(&linked, 0), vec![1]);
}

#[test]
fn external_with_an_unused_funcref_table_links() {
    // An lld `std` build also emits `(table 1 1 funcref)` with no element
    // segment. Nothing initializes or reads it, so it too is inert.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (table (;0;) 1 1 funcref)
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("an empty unused table must not block the link");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    assert_eq!(code_body_count(&linked), 2);
}

#[test]
fn external_with_unused_boilerplate_still_merges_over_caller_memory() {
    // The realistic shape: lld's global and table alongside a function that
    // stores through a caller-supplied pointer. The unread boilerplate must not
    // demote the closure out of Tier B, so the merged body keeps its store and
    // the output keeps the shared memory.
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
          (global $__stack_pointer (mut i32) (i32.const 1048576))
          (table (;0;) 1 1 funcref)
          (func (;0;) (type 0) (param i32 i32)
            local.get 0
            local.get 1
            i32.store)
          (export "store_at" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("lld boilerplate must not demote a Tier-B closure");
    assert_valid(&linked);
    assert_eq!(code_body_count(&linked), 2);
    assert!(
        body_has_i32_store(&linked, 1),
        "the merged Tier-B body must retain its memory store"
    );
    assert!(
        memory_limits(&linked).is_some(),
        "the merged module must keep the shared memory"
    );
}

#[test]
fn an_externals_declared_globals_and_tables_are_absent_from_the_merged_output() {
    // The soundness argument for the relaxed gate, asserted on the output rather
    // than left implicit.
    //
    // `merge` emits the main module's globals only and writes no table section,
    // and `rewrite`'s index map has no global or table remap arm. So a merged
    // body carrying `global.get N` would silently rebind to *main's* N-th global
    // and still pass post-merge validation (both are i32) — a wrong value with
    // no diagnostic. What rules that out is that closure effects are computed
    // over the closure's own bodies: a closure admitted with no global/table
    // effect has no such operator to leave behind.
    //
    // Main declares two globals of its own; the external declares two more plus
    // a table, and touches none of them.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (global (;0;) (mut i32) (i32.const 11))
          (global (;1;) i64 (i64.const 64))
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0
            global.get 0
            i32.add)
          (export "compute" (func 1))
          (export "state" (global 0)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (global $__stack_pointer (mut i32) (i32.const 1048576))
          (global (;1;) i64 (i64.const 999))
          (table (;0;) 1 1 funcref)
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("declared-but-unused external state must merge");
    assert_valid(&linked);

    // Only main's globals survive, at their original indices — the external's
    // two are dropped, not appended.
    let globals = module_globals(&linked);
    assert_eq!(
        globals,
        vec![
            (true, "I32Const { value: 11 }".to_string()),
            (false, "I64Const { value: 64 }".to_string()),
        ],
        "the external's globals must not reach the output, got {globals:?}"
    );

    // Main's own `global.get 0` still names main's first global.
    assert_eq!(
        body_call_targets(&linked, 0),
        vec![1],
        "main's call must retarget to the merged body"
    );

    // No table section is emitted, and no surviving body names a global or table
    // the merge did not re-index.
    let has_table_section = Parser::new(0)
        .parse_all(&linked)
        .any(|p| matches!(p.unwrap(), Payload::TableSection(_)));
    assert!(!has_table_section, "no table section may reach the output");

    // Main's body legitimately reads main's own global; the *merged* body (index
    // 1) must name nothing.
    assert_eq!(
        body_naming_a_global_or_table(&linked, Some(1)),
        None,
        "a merged external body must name no global or table"
    );
}

#[test]
fn a_global_read_outside_the_closure_does_not_block_the_link() {
    // Directly exercises what makes the relaxed gate safe: effects are computed
    // over the *closure*, not the module. The library's exported `sum` is pure,
    // while a sibling `read_state` — which nothing in `sum`'s closure calls —
    // reads the global. `sum` merges, `read_state` does not, and no `global.get`
    // reaches the output. A module-scoped effect would have rejected this.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (type (;1;) (func (result i32)))
          (global (;0;) (mut i32) (i32.const 7))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (func (;1;) (type 1) (result i32)
            global.get 0)
          (export "sum" (func 0))
          (export "read_state" (func 1)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("a global read outside the closure is irrelevant");
    assert_valid(&linked);
    assert_eq!(
        code_body_count(&linked),
        2,
        "only main's `compute` and the merged `sum` may reach the output"
    );
    assert_eq!(
        body_naming_a_global_or_table(&linked, None),
        None,
        "the global-reading sibling must not be merged"
    );
    assert!(module_globals(&linked).is_empty());
}

#[test]
fn a_global_read_through_a_transitive_call_is_remapped() {
    // The converse of the test above, pinning that the effect scan follows
    // calls: `sum` itself names no global, but the helper it calls does. The
    // helper *is* in the closure and is merged, so the external's globals must be
    // contributed on its account — the closure's `uses_globals` is what decides,
    // and it is set transitively or not at all.
    //
    // Main declares one global of its own, so the external's single global lands
    // at output index 1 and the helper's operand must move from 0 to 1. Were the
    // effect scan not transitive, the external's globals would be dropped, the
    // remap left empty, and the helper's `global.get 0` would fail the lookup —
    // so this fixture pins the transitivity from the merge side too.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (global (;0;) (mut i32) (i32.const 11))
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
          (type (;1;) (func (result i32)))
          (global (;0;) (mut i32) (i32.const 7))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add
            call 1
            i32.add)
          (func (;1;) (type 1) (result i32)
            global.get 0)
          (export "sum" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("a transitive global read must merge");
    assert_valid(&linked);

    // Bodies: main's `compute` (0), the merged `sum` (1), the merged helper (2).
    assert_eq!(code_body_count(&linked), 3);
    assert_eq!(
        body_global_indices(&linked, 2),
        vec![("get", 1)],
        "the transitively merged helper must read the external's global at its \
         remapped index, not main's global 0"
    );
    assert_eq!(
        module_globals(&linked),
        vec![
            (true, "I32Const { value: 11 }".to_string()),
            (true, "I32Const { value: 7 }".to_string()),
        ],
        "main's global keeps index 0 and the external's is appended after it"
    );
}

/// An external shaped like real lld output: a multi-page linear memory, a
/// `__stack_pointer` global pointing one page into it, an empty funcref table,
/// and a leaf `i32.add` export that touches none of them.
///
/// The minimal fixtures above deliberately omit the memory so they isolate the
/// global/table gate; this one is internally consistent (a stack pointer at
/// 1048576 presupposes at least 17 pages) and is therefore the fixture that says
/// what a stock artifact actually does at the link.
fn lld_shaped_lib() -> Vec<u8> {
    wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (memory (;0;) 17)
          (global $__stack_pointer (mut i32) (i32.const 1048576))
          (table (;0;) 1 1 funcref)
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    )
}

/// The same lld shape, but with a body that stores through a caller-supplied
/// pointer: a memory-*using* closure over a 17-page module. Its declared memory
/// is a fact about the merged output, so reconciliation must judge it.
fn lld_shaped_memory_using_lib() -> Vec<u8> {
    wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32)))
          (memory (;0;) 17)
          (global $__stack_pointer (mut i32) (i32.const 1048576))
          (table (;0;) 1 1 funcref)
          (func (;0;) (type 0) (param i32 i32)
            local.get 0
            local.get 1
            i32.store)
          (export "store_at" (func 0)))
        "#,
    )
}

/// A main module that imports `sum` and calls it from an exported `compute`,
/// under the memory shape the Inference compiler emits: `(memory 1 1)`, one page
/// with the maximum pinned equal to the minimum.
fn infc_shaped_main_importing_sum() -> Vec<u8> {
    wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (memory (;0;) 1 1)
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0)
          (export "memory" (memory 0))
          (export "compute" (func 1)))
        "#,
    )
}

#[test]
fn an_lld_shaped_external_links_onto_an_infc_shaped_main() {
    // What the relaxed global/table gate buys, pinned against the main-module
    // shape the Inference compiler actually emits: `(memory 1 1)`.
    //
    // The tier gate does not object: the closure reads no global and names no
    // table. Neither does memory reconciliation. The external declares 17 pages,
    // and the reconciler never relaxes the anchor module's pinned bound — but the
    // closure is a leaf `i32.add` that never addresses memory, so the external's
    // declaration is not folded in at all and main's single page is kept as-is.
    //
    // This test previously pinned the opposite outcome: an `IncompatibleMemory`
    // rejection of 17 pages against main's pinned 1. That was recorded as
    // behavior pinned rather than endorsed — the tier gate had been relaxed, and
    // the reconciler was the next thing to reject a stock
    // `wasm32-unknown-unknown` artifact, over pages nothing would have touched.
    // Adoption is now guarded on the closure's `uses_memory` effect. A
    // memory-*using* external still meets that rejection unchanged; see
    // `a_memory_using_lld_shaped_external_still_fails_reconciliation`.
    let main = infc_shaped_main_importing_sum();

    let linked = link(&main, &[&lld_shaped_lib()])
        .expect("a pure closure must not drag its module's 17 pages into the link");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    assert_eq!(code_body_count(&linked), 2);

    assert_eq!(
        memory_limits(&linked),
        Some((1, Some(1))),
        "main's own memory must survive untouched: neither widened to the external's \
         minimum nor relaxed to its unbounded maximum"
    );

    // The global/table gate still holds on the merged output.
    assert!(
        module_globals(&linked).is_empty(),
        "the external's __stack_pointer must not reach the output"
    );
    assert_eq!(
        body_naming_a_global_or_table(&linked, None),
        None,
        "no body may name a global or table the output does not declare"
    );
}

#[test]
fn an_lld_shaped_external_links_onto_a_memoryless_main_without_adopting_its_memory() {
    // The same external against a main that declares no memory of its own. The
    // link succeeds and the merged output declares no memory at all: the merged
    // closure is a pure `i32.add`, so the external's 17 pages are not adopted.
    //
    // This test previously pinned the opposite — an output memory of
    // `(17, None)` — explicitly as imprecision pinned rather than endorsed. It
    // reached further than the `.wasm`: `wasm-to-v` emits the reconciled limits
    // into the paired `.v` as the module's `mod_mems`, so a pure function's
    // incidental page count became an observable in the verification deliverable.
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

    let linked = link(&main, &[&lld_shaped_lib()])
        .expect("a memoryless main links a pure closure from a memory-declaring module");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    assert_eq!(code_body_count(&linked), 2);

    assert_eq!(
        memory_limits(&linked),
        None,
        "a closure that never addresses memory must not synthesize one in the output"
    );

    // The gate this change is about still holds on the merged output: neither the
    // external's global nor its table survives, and no body names either.
    assert!(
        module_globals(&linked).is_empty(),
        "the external's __stack_pointer must not reach the output"
    );
    assert_eq!(
        body_naming_a_global_or_table(&linked, None),
        None,
        "no body may name a global or table the output does not declare"
    );
}

#[test]
fn a_memory_using_lld_shaped_external_still_fails_reconciliation() {
    // The counterpart that keeps the guard honest. The same 17-page lld shape,
    // but the closure stores through a caller-supplied pointer, so its declared
    // memory *is* folded in — and the reconciler still refuses to widen main's
    // pinned single page to hold it.
    //
    // Guarding adoption narrows *which* externals contribute limits; it does not
    // loosen the reconciliation applied to the ones that do. The page-count
    // blocker for a stock memory-using artifact is untouched, and configurable
    // linear memory remains a separate change.
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

    let err = link(&main, &[&lld_shaped_memory_using_lib()])
        .expect_err("17 pages cannot be reconciled onto a pinned single page");
    match err {
        LinkError::IncompatibleMemory { field, reason } => {
            assert_eq!(field, "store_at");
            assert!(
                reason.contains("17 pages") && reason.contains("1 pages"),
                "the diagnostic must name both bounds so the real blocker is legible: {reason}"
            );
        }
        LinkError::RequiresRelocatableBuild { reasons, .. } => {
            panic!("the tier gate must not be what rejects this artifact, got {reasons:?}")
        }
        other => panic!("expected IncompatibleMemory, got {other:?}"),
    }
}

#[test]
fn a_memory_using_external_still_widens_the_reconciled_minimum() {
    // The guard must not amount to switching adoption off. A memory-using closure
    // over a 3-page external, folded onto a main that reserves one page under a
    // five-page cap, must still widen the output minimum to 3 — a result
    // distinguishable from dropping the external's declaration, which would have
    // left `(1, Some(5))`.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32)))
          (import "memlib" "store_at" (func (;0;) (type 0)))
          (memory (;0;) 1 5)
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
          (memory (;0;) 3)
          (func (;0;) (type 0) (param i32 i32)
            local.get 0
            local.get 1
            i32.store)
          (export "store_at" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("a memory-using Tier-B closure must still merge");
    assert_valid(&linked);
    assert_eq!(
        memory_limits(&linked),
        Some((3, Some(5))),
        "the memory-using external's minimum must still widen the output, under main's kept cap"
    );
    assert!(
        body_has_i32_store(&linked, 1),
        "the merged Tier-B body must retain its memory store"
    );
}

#[test]
fn a_memory_using_external_under_the_mains_bound_links_unchanged() {
    // The everyday Tier-B shape, asserted on the limits rather than only on the
    // memory export's survival: a one-page external whose closure stores through a
    // caller pointer, merged into the `(memory 1 1)` main the compiler emits. It
    // linked before the adoption guard and must link after it.
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

    let linked = link(&main, &[&lib]).expect("a one-page Tier-B external must merge");
    assert_valid(&linked);
    assert_eq!(
        memory_limits(&linked),
        Some((1, Some(1))),
        "main's pinned page must be kept, and the external's equal minimum changes nothing"
    );
    assert!(
        body_has_i32_store(&linked, 1),
        "the merged Tier-B body must retain its memory store"
    );
}

#[test]
fn memory_size_alone_counts_as_memory_use_and_adopts_the_memory() {
    // `memory.size` reads no byte of linear memory — it returns a page count —
    // so it is the operator most likely to be mistaken for memory-free. It is
    // exactly what the adoption guard must treat as use: the value it returns
    // *is* the reconciled minimum, so dropping the declaration that produced it
    // would change the merged program's observable answer.
    //
    // Against a memoryless main the proof is unambiguous: the output memory can
    // only have come from the external, so its presence shows the closure was
    // classified as memory-using.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (result i32)))
          (import "memlib" "pages" (func (;0;) (type 0)))
          (func (;1;) (type 0) (result i32)
            call 0)
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (result i32)))
          (memory (;0;) 4)
          (func (;0;) (type 0) (result i32)
            memory.size)
          (export "pages" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("a memory.size closure must merge");
    assert_valid(&linked);
    assert_eq!(
        memory_limits(&linked),
        Some((4, None)),
        "memory.size must count as memory use, so the external's declaration is adopted"
    );
}

#[test]
fn only_the_memory_using_external_contributes_its_declaration() {
    // The per-external form of the guard, against a memoryless main so every page
    // in the output is traceable to the external that contributed it. One
    // external is a pure `i32.add` over a 17-page lld-shaped module; the other
    // stores through a caller pointer over a 2-page module. The output must
    // reserve 2 pages — the memory-using external's — and not 17.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (type (;1;) (func (param i32 i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (import "mathlib" "store_at" (func (;1;) (type 1)))
          (func (;2;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            local.get 0
            local.get 1
            call 1
            call 0)
          (export "run" (func 2)))
        "#,
    );
    let store_lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32)))
          (memory (;0;) 2)
          (func (;0;) (type 0) (param i32 i32)
            local.get 0
            local.get 1
            i32.store)
          (export "store_at" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lld_shaped_lib(), &store_lib])
        .expect("both externals must merge");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    assert_eq!(
        memory_limits(&linked),
        Some((2, None)),
        "only the memory-using external may contribute limits; the pure external's 17 \
         pages must not appear"
    );
    assert!(
        module_globals(&linked).is_empty(),
        "neither external's global may reach the output"
    );
}

// -- The Tier-B reach warning ------------------------------------------------

/// A main module importing `touch` from `memlib`, calling it, and owning a
/// linear memory of `pages` pages pinned to that size — the shape the compiler
/// emits once a project configures its memory.
fn main_owning_pages(pages: u32) -> Vec<u8> {
    wasm(&format!(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "memlib" "touch" (func (;0;) (type 0)))
          (memory (;0;) {pages} {pages})
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call 0)
          (export "memory" (memory 0))
          (export "run" (func 1)))
        "#
    ))
}

/// An external exporting `touch : (i32, i32) -> i32` over a one-page memory that
/// its body never addresses: it adds its two parameters. Tier A.
///
/// It nonetheless *declares* the memory, so the only difference between this
/// fixture and a one-page [`tier_b_touch_lib`] is the operator in the body —
/// which is what makes the pair a test of the tier rather than of two modules
/// that happen to differ.
fn tier_a_touch_lib() -> Vec<u8> {
    wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (memory (;0;) 1)
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "touch" (func 0)))
        "#,
    )
}

/// The same external over `pages` pages, with a body that loads through its
/// first parameter: a caller-supplied address, so Tier B.
fn tier_b_touch_lib(pages: u32) -> Vec<u8> {
    wasm(&format!(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (memory (;0;) {pages})
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            i32.load
            local.get 1
            i32.add)
          (export "touch" (func 0)))
        "#
    ))
}

/// The single warning `out` carries, panicking when it carries any other count.
fn sole_warning(out: &LinkOutput) -> &LinkWarning {
    match out.warnings.as_slice() {
        [only] => only,
        other => panic!("expected exactly one warning, got {other:?}"),
    }
}

#[test]
fn a_tier_b_external_in_a_multi_page_memory_warns() {
    // What Tier B proves is that every address the external computes derives from
    // a parameter of the call — not that it stays inside the buffer that
    // parameter points into. A single page kept that gap mostly harmless by
    // accident: the reach usually left the memory and trapped. Two pages do not,
    // so the user is told.
    let out = link_with_warnings(&main_owning_pages(2), &[&tier_b_touch_lib(1)])
        .expect("a Tier-B external must still merge; this is a warning, not an error");
    assert_valid(&out.wasm);
    assert_eq!(
        memory_limits(&out.wasm),
        Some((2, Some(2))),
        "the fixture must actually produce the multi-page memory the warning is about"
    );

    assert_eq!(
        sole_warning(&out),
        &LinkWarning::TierBInMultiPageMemory {
            fields: vec!["touch".to_string()],
            pages: 2,
        }
    );

    // The rendered form is what a user reads, so the claim has to survive into it.
    let rendered = sole_warning(&out).to_string();
    assert!(
        rendered.contains("`touch`"),
        "the warning must name the external it is about: {rendered}"
    );
    assert!(
        rendered.contains("derives") && rendered.contains("2 pages"),
        "the warning must state the derivation claim and the page count: {rendered}"
    );
    assert!(
        rendered.contains("#420"),
        "the warning must point at the issue tracking containment analysis: {rendered}"
    );
}

#[test]
fn a_tier_a_external_in_a_multi_page_memory_does_not_warn() {
    // The exposure belongs to memory-addressing closures. A pure function has no
    // address to reach with, so the same two-page memory is not its problem —
    // warning about it would train the user to ignore the warning.
    //
    // The fixture differs from the Tier-B one by its body's operator alone: same
    // signature, same declared memory, same main module.
    let out = link_with_warnings(&main_owning_pages(2), &[&tier_a_touch_lib()])
        .expect("a Tier-A external must merge");
    assert_valid(&out.wasm);
    assert_eq!(
        memory_limits(&out.wasm),
        Some((2, Some(2))),
        "the memory the warning would be keyed on must be present, so the silence is \
         about the tier and not about the pages"
    );
    assert_eq!(
        out.warnings,
        Vec::new(),
        "a closure that never addresses memory has no unbounded reach to warn about"
    );
}

#[test]
fn a_tier_b_external_in_a_single_page_memory_does_not_warn() {
    // The other half of the condition. One page is the shape the accidental
    // backstop still covers: an address past the caller's buffer is usually past
    // the memory too, and traps.
    let out = link_with_warnings(&main_owning_pages(1), &[&tier_b_touch_lib(1)])
        .expect("a Tier-B external must merge");
    assert_valid(&out.wasm);
    assert_eq!(
        memory_limits(&out.wasm),
        Some((1, Some(1))),
        "the external's own single page must not widen the output past the condition"
    );
    assert_eq!(
        out.warnings,
        Vec::new(),
        "one page is the memory the warning exists to distinguish from"
    );
}

#[test]
fn a_warning_does_not_change_what_link_returns() {
    // A warning is never an error, and never a different artifact. `link` is the
    // warning-discarding form of the same merge, so the bytes must be identical
    // to the ones reported alongside the warning — the whole test suite links
    // through it, and would not notice a divergence.
    let main = main_owning_pages(2);
    let lib = tier_b_touch_lib(1);

    let out = link_with_warnings(&main, &[&lib]).expect("the merge succeeds");
    assert!(
        !out.warnings.is_empty(),
        "this fixture must warn, or the test compares two silent links"
    );

    let bytes = link(&main, &[&lib]).expect("a warning must not fail the link");
    assert_eq!(
        bytes, out.wasm,
        "the warning-discarding form must return the same module"
    );
}

#[test]
fn the_unreconcilable_minimum_names_the_knob_that_would_fix_it() {
    // The rule is deliberate — an external never relaxes the main module's own
    // memory bound — but the remedy lies on the other side of the link from the
    // error, and an author reading two page counts has no reason to guess that
    // the main module's page count is theirs to set.
    let err = link(&main_owning_pages(1), &[&tier_b_touch_lib(17)])
        .expect_err("17 pages cannot be reconciled onto a pinned single page");
    let LinkError::IncompatibleMemory { reason, .. } = err else {
        panic!("expected IncompatibleMemory, got {err:?}")
    };
    assert!(
        reason.contains("`pages`")
            && reason.contains("`[memory]`")
            && reason.contains("Inference.toml"),
        "the diagnostic must name the manifest key that raises the page count: {reason}"
    );
    assert!(
        reason.contains("--memory-pages"),
        "and the equivalent compiler flag, for a build with no manifest: {reason}"
    );
    assert!(
        reason.contains("is not relaxed"),
        "while still stating the rule it is applying: {reason}"
    );
}

#[test]
fn two_externals_each_carrying_unused_boilerplate_both_merge() {
    // The relaxed gate must hold per external, not just for a single one. Two
    // libraries, each with its own `__stack_pointer` and its own empty table —
    // the realistic multi-dependency shape — and neither contributes a global or
    // table to the shared output.
    let main = main_with_sum_and_sub();
    let add_lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (global $__stack_pointer (mut i32) (i32.const 1048576))
          (table (;0;) 1 1 funcref)
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
          (global $__stack_pointer (mut i32) (i32.const 2097152))
          (table (;0;) 2 2 funcref)
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.sub)
          (export "sub" (func 0)))
        "#,
    );

    let linked = link(&main, &[&add_lib, &sub_lib]).expect("both externals must merge");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    assert!(
        module_globals(&linked).is_empty(),
        "neither external's global may reach the output"
    );
    assert_eq!(
        body_naming_a_global_or_table(&linked, None),
        None,
        "no merged body may name a global or table"
    );
}

#[test]
fn an_unused_global_does_not_mask_an_absolute_address_rejection() {
    // Relaxing the global gate must not weaken the address-provenance proof that
    // guards Tier B. This external carries the same inert `__stack_pointer` as
    // the linking fixtures, but its body fabricates an absolute store address
    // from a parameter-cancelling computation — which must still be Tier C.
    //
    // The risk being pinned is order-dependent: `tier_c_reasons` runs before the
    // provenance analysis, so a module that used to be rejected on its global
    // declaration now proceeds far enough to be judged on its addressing. The
    // rejection must come from provenance, not vanish with the declaration.
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
          (global $__stack_pointer (mut i32) (i32.const 1048576))
          (func (;0;) (type 0) (param i32 i32)
            local.get 0
            local.get 0 i32.const 1 i32.mul i32.const 4096 i32.sub
            i32.sub
            local.get 1
            i32.store)
          (export "store_at" (func 0)))
        "#,
    );

    let err = link(&main, &[&lib]).expect_err("a fabricated absolute address must still reject");
    match err {
        LinkError::RequiresRelocatableBuild { field, reasons } => {
            assert_eq!(field, "store_at");
            assert!(
                !reasons.iter().any(|r| r.contains("global")),
                "the rejection must come from address provenance, not the inert global: {reasons:?}"
            );
        }
        other => panic!("expected RequiresRelocatableBuild from provenance, got {other:?}"),
    }
}

#[test]
fn reference_typed_table_operators_are_refused_by_the_feature_gate() {
    // Which layer actually rejects the table accessors, pinned so a future
    // reference-types enablement cannot silently widen the tier gate.
    //
    // `uses_tables` is set by `table.get`/`table.set`/`table.grow`/`table.size`/
    // `table.fill` and `ref.func`, but every one of them is a reference-types
    // instruction and `SUPPORTED_WASM_FEATURES` excludes that proposal — so an
    // external carrying one is refused by the *feature gate*, before its closure
    // is ever classified. The tier gate never sees them.
    //
    // That distinction matters now that the tier gate turns on use: if reference
    // types were enabled without revisiting tier classification, these operators
    // would start reaching `tier_c_reasons` and the flags in `safety.rs` would
    // become load-bearing rather than defense in depth. This test fails at that
    // moment — the error kind changes — rather than letting the widening pass
    // unnoticed.
    for (instruction, body) in [
        ("table.get", "i32.const 0 table.get 0 drop"),
        ("table.set", "i32.const 0 ref.null func table.set 0"),
        ("table.size", "table.size 0 drop"),
        ("table.grow", "ref.null func i32.const 1 table.grow 0 drop"),
        (
            "table.fill",
            "i32.const 0 ref.null func i32.const 0 table.fill 0",
        ),
        ("ref.func", "ref.func 0 drop"),
    ] {
        let main = main_importing_sum();
        let lib = wasm(&format!(
            r#"
            (module
              (type (;0;) (func (param i32 i32) (result i32)))
              (table (;0;) 1 1 funcref)
              (func (;0;) (type 0) (param i32 i32) (result i32)
                {body}
                local.get 0
                local.get 1
                i32.add)
              (export "sum" (func 0)))
            "#
        ));

        let Err(err) = link(&main, &[&lib]) else {
            panic!("`{instruction}` must be rejected, but the link succeeded");
        };
        match &err {
            LinkError::UnsupportedWasmFeature { details, .. } => assert!(
                details.contains("reference types"),
                "`{instruction}` must be named as a reference-types refusal: {details}"
            ),
            LinkError::RequiresRelocatableBuild { reasons, .. } => panic!(
                "`{instruction}` reached the tier gate, which means reference types \
                 were enabled without revisiting tier classification: {reasons:?}"
            ),
            other => panic!("`{instruction}`: expected UnsupportedWasmFeature, got {other:?}"),
        }
    }
}

#[test]
fn a_bare_table_is_inert_on_an_external_and_fatal_on_main() {
    // The asymmetry this change creates, asserted in one place so it is legible
    // rather than inferred from two distant tests.
    //
    // The *same* declaration — `(table 1 1 funcref)` with no element segment and
    // no instruction naming it — is now inert on an external (the closure never
    // touches it, so the merge drops it) and still fatal on the main module. The
    // main-side rejection is not about use at all: `emit` rebuilds main
    // section-by-section and writes no `TableSection`, so a main table would be
    // silently dropped along with anything that later referenced it. Rejecting
    // the section is the only way to keep that from becoming a valid-but-wrong
    // output, and it holds whether or not a body names the table.
    //
    // The existing `main_with_table_section_is_rejected` covers a main table that
    // `call_indirect` uses; this one covers the bare declaration, which is
    // exactly the shape that now links on the other side of the boundary.
    let bare_table_main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (table (;0;) 1 1 funcref)
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "compute" (func 0)))
        "#,
    );
    let err = link(&bare_table_main, &[]).expect_err("a bare main-side table must be rejected");
    assert!(
        matches!(&err, LinkError::UnsupportedConstruct(msg) if msg.contains("table")),
        "a main table must be rejected on its declaration, with no body naming it: {err:?}"
    );

    // The identical declaration on an external links.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (table (;0;) 1 1 funcref)
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );
    let linked = link(&main, &[&lib]).expect("the same bare table is inert on an external");
    assert_valid(&linked);
    assert!(
        !Parser::new(0)
            .parse_all(&linked)
            .any(|p| matches!(p.unwrap(), Payload::TableSection(_))),
        "the external's table must not reach the output"
    );
}

#[test]
fn a_pure_external_declaring_only_an_unused_global_leaves_the_output_global_free() {
    // Tier A with lld boilerplate: neither module owns a global that must
    // survive, so the merged output has no global section at all.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (global $__stack_pointer (mut i32) (i32.const 1048576))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("Tier A with lld boilerplate must link");
    assert_valid(&linked);
    assert!(
        module_globals(&linked).is_empty(),
        "no module contributes a surviving global"
    );
    assert_eq!(
        body_naming_a_global_or_table(&linked, None),
        None,
        "no body may name a global or table the output does not declare"
    );
    assert!(
        memory_limits(&linked).is_none(),
        "a pure merge declares no memory"
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
fn a_merged_global_read_names_the_externals_own_global() {
    // The central claim of the globals merge, and the one place a mistake is
    // invisible without checking the operand.
    //
    // Main declares two globals of its own, so the external's single global lands
    // at output index 2 and the merged `counter` must read `global.get 2`. If the
    // re-encoder had no arm for `global.get`, the operator would be copied
    // verbatim as `global.get 0` — main's first global, an `i32` like the
    // external's, so the merged module validates, links, and runs, returning
    // main's 11 where the library meant its own 7. Nothing but this index
    // separates the two outcomes: the body still contains a `global.get`, the
    // module still has a global section, and the counts are unchanged.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (result i32)))
          (import "statelib" "counter" (func (;0;) (type 0)))
          (global (;0;) (mut i32) (i32.const 11))
          (global (;1;) (mut i32) (i32.const 22))
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

    let linked = link(&main, &[&lib]).expect("a global-reading external must merge");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());

    assert_eq!(
        body_global_indices(&linked, 1),
        vec![("get", 2)],
        "the merged body must read the external's global at its remapped index"
    );
    assert_eq!(
        module_globals(&linked),
        vec![
            (true, "I32Const { value: 11 }".to_string()),
            (true, "I32Const { value: 22 }".to_string()),
            (true, "I32Const { value: 7 }".to_string()),
        ],
        "main's globals keep indices 0 and 1; the external's is appended at 2"
    );
    assert_eq!(
        body_global_indices(&linked, 0),
        vec![],
        "main's own body is untouched here, and its indices never shift regardless"
    );
}

#[test]
fn a_merged_global_write_names_the_externals_own_global() {
    // The write side, which is the more dangerous half: a `global.set` copied
    // verbatim would not merely read the wrong value but *corrupt* main's state,
    // and it would do so on a module that validates. Main's global 0 is a
    // mutable `i32` exactly like the external's, so type checking cannot tell the
    // two apart.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32)))
          (import "statelib" "set_counter" (func (;0;) (type 0)))
          (global (;0;) (mut i32) (i32.const 11))
          (func (;1;) (type 0) (param i32)
            local.get 0
            call 0)
          (export "run" (func 1))
          (export "state" (global 0)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32)))
          (global (;0;) (mut i32) (i32.const 0))
          (func (;0;) (type 0) (param i32)
            local.get 0
            global.set 0)
          (export "set_counter" (func 0)))
        "#,
    );

    let linked = link(&main, &[&lib]).expect("a global-writing external must merge");
    assert_valid(&linked);
    assert_eq!(
        body_global_indices(&linked, 1),
        vec![("set", 1)],
        "the merged body must write the external's own global, not main's state"
    );

    // Main's `state` export still names main's global 0. Appending the external's
    // globals above main's is what keeps every main-side global reference — bodies
    // and exports alike — correct without rewriting any of them.
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
        vec![("state".to_string(), 0)],
        "main's global export must still name main's own global"
    );
}

#[test]
fn two_externals_identical_globals_stay_distinct() {
    // Signatures are deduplicated across externals; globals must not be. Two
    // modules that each declare `(global (mut i32) (i32.const 0))` mean two
    // counters, not one shared cell — a global is state, not a description — so
    // the merged module must carry both, and each body must name its own.
    //
    // Collapsing them would produce a module that validates and runs, where one
    // library's writes silently appear as the other's reads. The two libraries
    // are deliberately byte-identical apart from their export names, so nothing
    // but the merge's refusal to dedup keeps them apart.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32)))
          (import "liba" "bump_a" (func (;0;) (type 0)))
          (import "libb" "bump_b" (func (;1;) (type 0)))
          (func (;2;) (type 0) (param i32)
            local.get 0
            call 0
            local.get 0
            call 1)
          (export "run" (func 2)))
        "#,
    );
    let counter_lib = |export: &str| {
        wasm(&format!(
            r#"
            (module
              (type (;0;) (func (param i32)))
              (global (;0;) (mut i32) (i32.const 0))
              (func (;0;) (type 0) (param i32)
                local.get 0
                global.set 0)
              (export "{export}" (func 0)))
            "#
        ))
    };
    let lib_a = counter_lib("bump_a");
    let lib_b = counter_lib("bump_b");

    let linked = raw_link(&main, &[("liba", &lib_a), ("libb", &lib_b)], None)
        .expect("two global-bearing externals must merge");
    assert_valid(&linked);

    assert_eq!(
        module_globals(&linked),
        vec![
            (true, "I32Const { value: 0 }".to_string()),
            (true, "I32Const { value: 0 }".to_string()),
        ],
        "structurally identical globals from two externals are distinct state and \
         must both survive"
    );

    // Bodies: main's `run` (0), then the two merged bodies (1, 2) in the order
    // their imports were satisfied. Each must name its own cell.
    assert_eq!(body_global_indices(&linked, 1), vec![("set", 0)]);
    assert_eq!(body_global_indices(&linked, 2), vec![("set", 1)]);
}

#[test]
fn a_global_bearing_external_links_onto_a_globalless_main() {
    // The main module the Inference compiler emits declares no globals at all,
    // and until now the merge skipped the global section entirely whenever main
    // had none. An external that brings one must still get a section, or its
    // merged body would name a global the output does not declare — caught by
    // post-merge validation, but only as a failure of the linker's own output.
    //
    // Main is memoryless too, so this fixture also pins that a globals-only
    // merge needs no memory: the external's global is state, not an address, and
    // nothing here is asked to reconcile a memory that no closure touches.
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

    let linked = link(&main, &[&lib]).expect("a globalless main must accept an external's global");
    assert_valid(&linked);
    assert_eq!(
        module_globals(&linked),
        vec![(true, "I32Const { value: 7 }".to_string())],
        "the external's global must be the sole entry of a section main did not open"
    );
    assert_eq!(
        body_global_indices(&linked, 1),
        vec![("get", 0)],
        "with no main globals below it the external's global keeps index 0"
    );
    assert!(
        memory_limits(&linked).is_none(),
        "a globals-only merge must not invent a linear memory"
    );
}

#[test]
fn a_global_used_to_address_memory_is_still_rejected() {
    // The soundness boundary, through the public API. A merged global's *value*
    // is carried over with no relocation, and in real toolchain output that value
    // is an address into the layout the external was compiled for —
    // `__stack_pointer` here. Merged onto a memory laid out for the host program
    // the index is right and the address means something else.
    //
    // What keeps that from linking is address provenance, which treats a value
    // read from a global as not parameter-derived. It is a *conditional*
    // protection: it runs only for a closure that touches memory. This test pins
    // the case where it does.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32)))
          (import "memlib" "push" (func (;0;) (type 0)))
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
          (memory (;0;) 17)
          (global $__stack_pointer (mut i32) (i32.const 1048576))
          (func (;0;) (type 0) (param i32)
            global.get 0
            local.get 0
            i32.store)
          (export "push" (func 0)))
        "#,
    );

    let err = link(&main, &[&lib]).expect_err("a global-derived store address must be rejected");
    match err {
        LinkError::RequiresRelocatableBuild { field, reasons } => {
            assert_eq!(field, "push");
            // The rejection must come from provenance. A surviving globals reason
            // would mean the gate was never relaxed, and every admission test
            // above would be measuring something else.
            assert!(
                !reasons.iter().any(|r| r.contains("global")),
                "the rejection must come from address provenance: {reasons:?}"
            );
        }
        other => panic!("expected RequiresRelocatableBuild from provenance, got {other:?}"),
    }
}

#[test]
fn tier_c_element_segment_requires_relocatable_build() {
    // A *bare* table is inert and links; an element segment does not, even
    // though nothing in the closure reads the table. That rejection is
    // conservatism, not a correctness requirement: the merged output declares no
    // table, so a dropped element segment initializes nothing anyone could
    // observe. It stays rejected because an element segment marks a module built
    // around indirect dispatch, and admitting one would silently discard a
    // construct the author wrote.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (table (;0;) 1 1 funcref)
          (elem (;0;) (i32.const 0) func 0)
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );

    let err = link(&main, &[&lib]).expect_err("an element segment must be rejected");
    match err {
        LinkError::RequiresRelocatableBuild { field, reasons } => {
            assert_eq!(field, "sum");
            assert_eq!(
                reasons,
                vec!["declares an element segment".to_string()],
                "the reason must name the element segment, and must not claim \
                 the closure touched the table space — this body never does"
            );
        }
        other => panic!("expected RequiresRelocatableBuild, got {other:?}"),
    }
}

#[test]
fn tier_c_unused_data_segment_requires_relocatable_build() {
    // The deliberate asymmetry with globals and tables. This external's closure
    // names no data segment at all — it is a pure integer add — yet the module's
    // *declared* data segment still rejects the link, because an active data
    // segment writes memory at instantiation whether or not any instruction
    // refers to it. Dropping it would silently change what the merged program
    // observes, so data stays declaration-gated where globals and tables no
    // longer are.
    let main = main_importing_sum();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (memory (;0;) 1)
          (data (;0;) (i32.const 0) "\2a\00\00\00")
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    );

    let err = link(&main, &[&lib]).expect_err("a declared data segment must be rejected");
    match err {
        LinkError::RequiresRelocatableBuild { field, reasons } => {
            assert_eq!(field, "sum");
            assert!(
                reasons.iter().any(|r| r.contains("data")),
                "reason should mention static data: {reasons:?}"
            );
        }
        other => panic!("expected RequiresRelocatableBuild, got {other:?}"),
    }
}

#[test]
fn tier_c_data_drop_requires_relocatable_build() {
    // The other segment-naming operator alongside `memory.init`: `data.drop`
    // names a data segment the merge does not carry across.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func))
          (import "droplib" "release" (func (;0;) (type 0)))
          (memory (;0;) 1 1)
          (func (;1;) (type 0)
            call 0)
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func))
          (memory (;0;) 1)
          (data (;0;) "\2a")
          (func (;0;) (type 0)
            data.drop 0)
          (export "release" (func 0)))
        "#,
    );

    let err = link(&main, &[&lib]).expect_err("data.drop must be rejected");
    match err {
        LinkError::RequiresRelocatableBuild { field, reasons } => {
            assert_eq!(field, "release");
            assert!(
                reasons.iter().any(|r| r.contains("data")),
                "reason should mention static data: {reasons:?}"
            );
        }
        other => panic!("expected RequiresRelocatableBuild, got {other:?}"),
    }
}

#[test]
fn tier_c_indirect_call_requires_relocatable_build() {
    // An external function that performs an indirect call needs the table /
    // element space, which the static merge does not relocate. Here it is the
    // `call_indirect` that rejects; the table declaration alone would not.
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
    // The main module owns its own globals (an i32 and an i64) — state an
    // external's closure may not touch, but perfectly fine on the main module,
    // which keeps its memory and globals. The merge must re-emit the global
    // section, both constant initializers, and a `Global`-kind export unchanged.
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
    let err = raw_link(b"not a wasm module", &[], None).expect_err("garbage must not parse");
    assert!(matches!(err, LinkError::Parse(_)), "expected Parse, got {err:?}");
}

#[test]
fn invalid_external_bytes_are_a_parse_error() {
    let main = main_with_sum_and_sub();
    let err = raw_link(&main, &[("mathlib", b"\0asm broken")], None)
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

    let err = raw_link(&main, &[], None).expect_err("a two-memory main must be rejected");
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
    let linked = raw_link(&main, &[("aaa", &sub_lib), ("bbb", &add_lib)], None)
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
    let reversed = raw_link(&main, &[("bbb", &add_lib), ("aaa", &sub_lib)], None)
        .expect("order must not matter");
    assert!(
        body_has_i32_add(&reversed, 1),
        "filename/slice order must not decide the merged body"
    );
}

/// Two externals bound under *different* logical modules both export `sum`, and
/// the main module imports `sum` from each. The module-prefixed naming makes the
/// two merged roots' name-section entries distinct by construction (`alib::sum`,
/// `blib::sum`), so neither collides nor forces wasm-to-v's index-suffix
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

    let linked = raw_link(&main, &[("alib", &alib), ("blib", &blib)], None)
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
        merged.contains("alib::sum"),
        "the alib-bound root must be named `alib::sum`, got {names:?}"
    );
    assert!(
        merged.contains("blib::sum"),
        "the blib-bound root must be named `blib::sum`, got {names:?}"
    );
    assert_eq!(
        merged.len(),
        2,
        "the two same-field roots must have distinct names by construction, got {names:?}"
    );
}

/// A logical module name is itself a `::`-joined path (`crypto::sha256`, from
/// `use { hash } from crypto::sha256;`), and it must flow into the merged name
/// verbatim and deterministically (`crypto::sha256::hash`), with no panic. The
/// prefix boundary is not found by looking for the first `::`, and does not need
/// to be: an export field is an Inference identifier, so the name decomposes
/// only one way. The downstream Rocq translator maps every non-identifier byte
/// to `_`, so the separators are its concern, not the linker's — the linker
/// keeps the logical name verbatim so the prefix stays traceable to its source
/// module.
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

    let linked = raw_link(&main, &[("crypto::sha256", &lib)], None)
        .expect("a `::`-separated logical module must link without panicking");
    assert_valid(&linked);

    let names = function_names(&linked);
    assert!(
        names.contains(&(1, "crypto::sha256::hash".to_string())),
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

// -- inference.hspecs section ------------------------------------------------

/// One obligation, keyed under spec `S` and owned by the function symbol
/// `compute`, whose assertion also *references* `compute` via a `T_app`. The
/// symbol therefore appears both as the entry owner and inside the tree, so the
/// merge must carry it through untouched for the reference to stay resolvable.
fn one_hspec_referencing(symbol: &str) -> inference_hassert::HSpecMap {
    use inference_hassert::{HAssert, HFnRef, HSpecEntry, HSpecMap, HTerm, SpecKind};
    let mut map = HSpecMap::default();
    map.insert(
        "S".to_string(),
        vec![HSpecEntry::new(
            HFnRef(symbol.to_string()),
            HAssert::nz(HTerm::App(
                HFnRef(symbol.to_string()),
                vec![HTerm::Local(0), HTerm::Local(1)],
            )),
            SpecKind::Forall,
        )],
    );
    map
}

/// A main module importing `sum`, with a local `compute` (named in the `name`
/// section) and an `inference.hspecs` section carrying `payload`. Built through
/// `wasm_encoder` — rather than the WAT `main_importing_sum` — so it carries the
/// function name the symbolic obligation must survive against.
fn main_named_with_hspecs(payload: &[u8]) -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, CustomSection, ExportKind, ExportSection, Function, FunctionSection,
        ImportSection, Instruction, Module, NameMap, NameSection, TypeSection, ValType,
    };

    let mut module = Module::new();

    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    module.section(&types);

    let mut imports = ImportSection::new();
    imports.import("mathlib", "sum", wasm_encoder::EntityType::Function(0));
    module.section(&imports);

    let mut funcs = FunctionSection::new();
    funcs.function(0); // local func -> global index 1 (after the one import)
    module.section(&funcs);

    let mut exports = ExportSection::new();
    exports.export("compute", ExportKind::Func, 1);
    module.section(&exports);

    let mut code = CodeSection::new();
    let mut compute = Function::new([]);
    compute.instruction(&Instruction::LocalGet(0));
    compute.instruction(&Instruction::LocalGet(1));
    compute.instruction(&Instruction::Call(0));
    compute.instruction(&Instruction::End);
    code.function(&compute);
    module.section(&code);

    let mut names = NameSection::new();
    names.module("main");
    let mut func_names = NameMap::new();
    func_names.append(1, "compute");
    names.functions(&func_names);
    module.section(&names);

    module.section(&CustomSection {
        name: inference_hassert::HSPECS_SECTION_NAME.into(),
        data: payload.into(),
    });

    module.finish()
}

/// The main module's `inference.hspecs` payload references functions by symbol,
/// not index, so the merge must carry it through byte-for-byte (no remap) while
/// preserving the main function names the symbols resolve against.
#[test]
fn main_hspecs_section_survives_the_merge_byte_for_byte() {
    let map = one_hspec_referencing("compute");
    let payload = inference_hassert::encode(&map);
    let main = main_named_with_hspecs(&payload);

    let lib = mathlib_pure();
    let linked = link(&main, &[&lib]).expect("link must preserve the hspecs section");
    assert_valid(&linked);

    let carried = custom_section_data(&linked, inference_hassert::HSPECS_SECTION_NAME)
        .expect("the linked module must still carry the inference.hspecs section");
    assert_eq!(
        carried, payload,
        "the symbolic hspecs payload must survive the merge verbatim (no remap)"
    );
    assert_eq!(
        inference_hassert::decode(&carried).expect("carried hspecs must decode"),
        map,
        "the round-tripped obligation map must equal the original"
    );

    // The referenced symbol `compute` must still name a defined function post-link
    // (shifted from pre-link index 1 to 0 once the `sum` import is removed), so the
    // Rocq translator can later resolve the symbolic `T_app` to its `mod_funcs`
    // index.
    assert!(
        function_names(&linked).contains(&(0, "compute".to_string())),
        "the main function the obligation names must survive the merge verbatim, got {:?}",
        function_names(&linked)
    );
}

/// An external's `inference.hspecs` section is verification-only scaffolding for
/// a module never emitted whole; building an executable must strip it, never
/// merge it into the output.
#[test]
fn external_hspecs_section_is_stripped_when_building_an_executable() {
    use wasm_encoder::{
        CodeSection, CustomSection, ExportKind, ExportSection, Function, FunctionSection,
        Instruction, Module, TypeSection, ValType,
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
        let mut sum = Function::new([]);
        sum.instruction(&Instruction::LocalGet(0));
        sum.instruction(&Instruction::LocalGet(1));
        sum.instruction(&Instruction::I32Add);
        sum.instruction(&Instruction::End);
        code.function(&sum);
        module.section(&code);
        let payload = inference_hassert::encode(&one_hspec_referencing("sum"));
        module.section(&CustomSection {
            name: inference_hassert::HSPECS_SECTION_NAME.into(),
            data: (&payload[..]).into(),
        });
        module.finish()
    };

    let main = main_importing_sum();
    let linked =
        link(&main, &[&lib]).expect("an external carrying an hspecs section must still link");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    assert!(
        custom_section_data(&linked, inference_hassert::HSPECS_SECTION_NAME).is_none(),
        "an external's hspecs section must not be merged into the executable output"
    );
}

#[test]
fn corrupt_main_hspecs_section_is_a_clean_link_error() {
    // A main `inference.hspecs` section is a verification deliverable: a corrupt
    // payload must fail the link with a clean error, never a corrupt artifact the
    // Rocq translator later chokes on. Version byte 3 is past the codec's
    // supported version 2, so the shared decoder rejects it — the sentinel must
    // stay one past the current version, or a format bump would turn these bytes
    // into a valid empty payload and this test into a false green.
    let corrupt = [3u8, 0, 0];
    let mut main = main_importing_sum();
    use wasm_encoder::Section as _;
    wasm_encoder::CustomSection {
        name: inference_hassert::HSPECS_SECTION_NAME.into(),
        data: (&corrupt[..]).into(),
    }
    .append_to(&mut main);

    let lib = mathlib_pure();
    let err = link(&main, &[&lib])
        .expect_err("a corrupt main hspecs section must be a clean rejection");
    assert!(
        matches!(&err, LinkError::Parse(msg) if msg.contains("hspecs")),
        "expected a Parse error naming the hspecs section, got {err:?}"
    );
}

#[test]
fn two_hspecs_sections_in_main_are_a_clean_error_not_a_silent_overwrite() {
    // Like the duplicate `spec_funcs` case: two `inference.hspecs` sections would
    // let a last-wins assignment silently drop the first section's obligations.
    // The parser must reject the duplicate rather than vanish obligations.
    let first = inference_hassert::encode(&one_hspec_referencing("compute"));
    let second = inference_hassert::encode(&one_hspec_referencing("other"));
    let mut main = main_importing_sum();
    use wasm_encoder::Section as _;
    wasm_encoder::CustomSection {
        name: inference_hassert::HSPECS_SECTION_NAME.into(),
        data: (&first[..]).into(),
    }
    .append_to(&mut main);
    wasm_encoder::CustomSection {
        name: inference_hassert::HSPECS_SECTION_NAME.into(),
        data: (&second[..]).into(),
    }
    .append_to(&mut main);

    let lib = mathlib_pure();
    let err = link(&main, &[&lib])
        .expect_err("a duplicate hspecs section must be rejected, never silently overwritten");
    assert!(
        matches!(&err, LinkError::Parse(msg) if msg.contains("hspecs")),
        "expected a clean error naming the duplicate hspecs section, got {err:?}"
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
    let err = raw_link(&main, &[], None)
        .expect_err("a main with an out-of-range type index must be a clean rejection");
    assert!(
        matches!(&err, LinkError::Parse(_)),
        "expected a clean Parse rejection, got {err:?}"
    );
}

// -- Obligation symbols in the merged name section ---------------------------

/// A one-entry obligation map under spec `S` whose assertion applies `symbol`.
///
/// `HA_app_ok` is the smallest position in which an obligation names a function
/// whose body the module must contain, which is exactly what the merge has to
/// answer for.
fn applying(symbol: &str) -> inference_hassert::HSpecMap {
    use inference_hassert::{HAssert, HFnRef, HSpecEntry, HSpecMap, HTerm, SpecKind};
    let mut map = HSpecMap::default();
    map.insert(
        "S".to_string(),
        vec![HSpecEntry::new(
            HFnRef("S.claim".to_string()),
            HAssert::AppOk(HFnRef(symbol.to_string()), vec![HTerm::Local(0)]),
            SpecKind::Forall,
        )],
    );
    map
}

/// `bytes` with an `inference.hspecs` section carrying `map` appended.
fn with_hspecs(bytes: &[u8], map: &inference_hassert::HSpecMap) -> Vec<u8> {
    use wasm_encoder::Section as _;
    let mut out = bytes.to_vec();
    wasm_encoder::CustomSection {
        name: inference_hassert::HSPECS_SECTION_NAME.into(),
        data: (&inference_hassert::encode(map)[..]).into(),
    }
    .append_to(&mut out);
    out
}

/// The function symbol the sole obligation of `bytes` applies.
fn sole_applied_symbol(bytes: &[u8]) -> String {
    let data = custom_section_data(bytes, inference_hassert::HSPECS_SECTION_NAME)
        .expect("the module must carry an inference.hspecs section");
    let map = inference_hassert::decode(&data).expect("carried hspecs must decode");
    let entries = map.get("S").expect("spec `S`");
    match &entries[..] {
        [entry] => match &entry.hassert {
            inference_hassert::HAssert::AppOk(symbol, _) => symbol.0.clone(),
            other => panic!("unexpected obligation shape: {other:?}"),
        },
        other => panic!("expected exactly one obligation, got {}", other.len()),
    }
}

/// A main module importing the named `mathlib` fields and defining one local
/// function per entry of `local_names`, each recorded in the `name` section
/// under the name given.
///
/// Built through `wasm_encoder` rather than WAT so a test can put an arbitrary
/// string at a chosen index — a merged body's `::`-joined name against one of
/// the program's own functions, which is the collision the two name spaces exist
/// to prevent. Every function shares `mathlib`'s `(i32, i32) -> i32` shape, so
/// the imports are satisfiable by [`mathlib_pure`].
fn main_with_names(imports: &[&str], local_names: &[&str]) -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, EntityType, Function, FunctionSection, ImportSection, Instruction, Module,
        NameMap, NameSection, TypeSection, ValType,
    };

    let mut module = Module::new();
    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    module.section(&types);

    if !imports.is_empty() {
        let mut section = ImportSection::new();
        for field in imports {
            section.import("mathlib", field, EntityType::Function(0));
        }
        module.section(&section);
    }

    let mut funcs = FunctionSection::new();
    let mut code = CodeSection::new();
    for _ in local_names {
        funcs.function(0);
        let mut body = Function::new([]);
        body.instruction(&Instruction::LocalGet(0));
        body.instruction(&Instruction::End);
        code.function(&body);
    }
    module.section(&funcs);
    module.section(&code);

    let mut names = NameSection::new();
    let mut func_names = NameMap::new();
    for (i, name) in local_names.iter().enumerate() {
        func_names.append(imports.len() as u32 + i as u32, name);
    }
    names.functions(&func_names);
    module.section(&names);

    module.finish()
}

/// An external exporting one body under two fields — the shape two `external fn`
/// declarations of different names, bound from the same module, resolve onto.
fn aliased_export_lib() -> Vec<u8> {
    wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0
            local.get 0
            i32.add)
          (export "double" (func 0))
          (export "twice" (func 0)))
        "#,
    )
}

/// A main module importing `mathlib`'s two aliases of one body, in the order
/// given, and calling each once.
fn main_importing_aliases(first: &str, second: &str) -> Vec<u8> {
    wasm(&format!(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (import "mathlib" "{first}" (func (;0;) (type 0)))
          (import "mathlib" "{second}" (func (;1;) (type 0)))
          (func $compute (;2;) (type 0) (param i32) (result i32)
            local.get 0
            call 0
            call 1)
          (export "compute" (func 2)))
        "#
    ))
}

/// One foreign body satisfying two imports must be reached by both of them, and
/// must be named the same way whichever order the main module lists them in.
///
/// The name section holds one name per function index, so only one of the two
/// root names can be recorded. Recording the *last* one written would make the
/// output depend on import order, and would leave the earlier alias naming
/// nothing at all.
#[test]
fn one_body_bound_under_two_fields_is_reached_by_both_and_named_stably() {
    let lib = aliased_export_lib();

    for (first, second) in [("double", "twice"), ("twice", "double")] {
        let main = main_importing_aliases(first, second);
        let linked = raw_link(&main, &[("mathlib", &lib)], None)
            .expect("one export bound under two fields must link");
        assert_valid(&linked);
        assert!(function_imports(&linked).is_empty());

        // The closure is deduped on the source function, so there is one merged
        // body and both calls redirect onto it.
        assert_eq!(
            code_body_count(&linked),
            2,
            "main `compute` plus the single merged body"
        );
        assert_eq!(
            body_call_targets(&linked, 0),
            vec![1, 1],
            "both imports must retarget onto the one merged body"
        );

        let merged: Vec<String> = function_names(&linked)
            .into_iter()
            .filter(|(idx, _)| *idx == 1)
            .map(|(_, name)| name)
            .collect();
        assert_eq!(
            merged,
            vec!["mathlib::double".to_string()],
            "the merged body must carry exactly one root name, the same one in either \
             import order, got {merged:?} for ({first}, {second})"
        );
    }
}

/// An obligation over the alias the name section could not record still applies
/// the body it was always about.
///
/// This is what keeps the one-name-per-index limit from silently costing an
/// obligation: the merge knows both root names denote one body, so it points the
/// obligation at the name the artifact carries rather than leaving it to resolve
/// against nothing.
#[test]
fn an_obligation_over_an_unrecorded_alias_is_pointed_at_the_recorded_name() {
    let main = with_hspecs(
        &main_importing_aliases("double", "twice"),
        &applying("mathlib::twice"),
    );
    let linked = raw_link(&main, &[("mathlib", &aliased_export_lib())], None)
        .expect("an obligation over an aliased root must link");
    assert_valid(&linked);

    assert_eq!(
        sole_applied_symbol(&linked),
        "mathlib::double",
        "the obligation must apply the name the merged body actually carries"
    );
    assert!(
        function_names(&linked).contains(&(1, "mathlib::double".to_string())),
        "and that name must be the merged body's, got {:?}",
        function_names(&linked)
    );
}

/// A merged root and a private callee of the same module whose debug name equals
/// the export field are two functions, and must stay two names.
///
/// The root's name comes from the import field, which is an Inference
/// identifier; the callee keeps the name its own module gave it, which is
/// unconstrained and may be exactly that field. The internal mark is what holds
/// them apart — without it both render `mathlib::double` and an obligation over
/// the root resolves ambiguously.
#[test]
fn a_private_callee_named_like_the_export_field_stays_distinct_from_the_root() {
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (func $entry (;0;) (type 0) (param i32) (result i32)
            local.get 0
            call 1)
          (func $double (;1;) (type 0) (param i32) (result i32)
            local.get 0
            local.get 0
            i32.add)
          (export "double" (func 0)))
        "#,
    );
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (import "mathlib" "double" (func (;0;) (type 0)))
          (func $compute (;1;) (type 0) (param i32) (result i32)
            local.get 0
            call 0)
          (export "compute" (func 1)))
        "#,
    );

    let linked = raw_link(&main, &[("mathlib", &lib)], None)
        .expect("a module whose private callee shares the export field must link");
    assert_valid(&linked);

    let names = function_names(&linked);
    assert!(
        names.contains(&(1, "mathlib::double".to_string())),
        "the root takes the import field, got {names:?}"
    );
    assert!(
        names.contains(&(2, "mathlib::#double".to_string())),
        "the private callee keeps its own name behind the internal mark, got {names:?}"
    );
}

/// An obligation naming a merged body the merge did not produce is rejected
/// here, where the imports it was meant to name are still known.
#[test]
fn an_obligation_naming_no_merged_body_is_rejected_naming_what_was_satisfied() {
    let main = with_hspecs(
        &main_with_names(&["sum"], &["compute"]),
        &applying("mathlib::product"),
    );
    let err = raw_link(&main, &[("mathlib", &mathlib_pure())], None)
        .expect_err("an obligation over an unsatisfied field must not link silently");

    assert!(
        matches!(
            &err,
            LinkError::UnresolvedObligationSymbol { symbol, merged_roots }
                if symbol == "mathlib::product" && merged_roots == &["mathlib::sum".to_string()]
        ),
        "expected the unresolved-symbol rejection, got {err:?}"
    );
    let rendered = err.to_string();
    assert!(
        rendered.contains("`mathlib::product`") && rendered.contains("`mathlib::sum`"),
        "the message must name both the symbol and what the merge did satisfy; got: {rendered}"
    );
}

/// The same rejection for a symbol in the *program* half of the name section
/// says something else, because the repair is not the same one: no import is
/// involved, so pointing at the satisfied import list would send the author
/// looking in the wrong place.
#[test]
fn an_obligation_naming_no_program_function_is_rejected_as_a_program_symbol() {
    let main = with_hspecs(
        &main_with_names(&["sum"], &["compute"]),
        &applying("helper"),
    );
    let err = raw_link(&main, &[("mathlib", &mathlib_pure())], None)
        .expect_err("an obligation over a function the program does not define must be rejected");

    assert!(
        matches!(&err, LinkError::UnresolvedObligationSymbol { symbol, .. } if symbol == "helper"),
        "expected the unresolved-symbol rejection, got {err:?}"
    );
    let rendered = err.to_string();
    assert!(
        rendered.contains("the program's own functions"),
        "the message must place the symbol in the program half; got: {rendered}"
    );
}

/// A program function and a merged body sharing one name is rejected, not
/// resolved.
///
/// Inference codegen cannot produce this — a compiled function's symbol is built
/// from identifiers joined by dots and can carry no `:` — but the linker is a
/// public API that accepts arbitrary main bytes, so the argument for the two
/// halves being disjoint is enforced here rather than assumed.
#[test]
fn a_program_function_sharing_a_merged_body_name_is_rejected_as_ambiguous() {
    let main = with_hspecs(
        &main_with_names(&["sum"], &["mathlib::sum"]),
        &applying("mathlib::sum"),
    );
    let err = raw_link(&main, &[("mathlib", &mathlib_pure())], None)
        .expect_err("two functions of one name must not resolve an obligation");

    assert!(
        matches!(
            &err,
            LinkError::AmbiguousObligationSymbol { symbol, carriers }
                if symbol == "mathlib::sum" && carriers.len() == 2
        ),
        "expected the ambiguous-symbol rejection, got {err:?}"
    );
    let rendered = err.to_string();
    assert!(
        rendered.contains("the program's own function at index 0")
            && rendered.contains("the body merged to satisfy `mathlib::sum`"),
        "the message must say where each carrier came from; got: {rendered}"
    );
}

/// Two of the program's own functions sharing one name is rejected too, on a
/// module that links no external at all.
///
/// The program half of the name section is not injective on its own: a method
/// key and a free-function key of a longer module path render alike, which is a
/// pre-existing property of the rendering rather than anything the merge does.
/// It stays fail-closed, and it fails here rather than in the translator, which
/// would have only the symbol to report.
#[test]
fn two_program_functions_sharing_one_name_are_rejected_as_ambiguous() {
    let main = with_hspecs(
        &main_with_names(&[], &["lib.geo.make", "lib.geo.make"]),
        &applying("lib.geo.make"),
    );
    let err = raw_link(&main, &[], None)
        .expect_err("two program functions of one name must not resolve an obligation");

    assert!(
        matches!(
            &err,
            LinkError::AmbiguousObligationSymbol { symbol, carriers }
                if symbol == "lib.geo.make" && carriers.len() == 2
        ),
        "expected the ambiguous-symbol rejection, got {err:?}"
    );
    assert!(
        err.to_string()
            .contains("the program's own function at index 1"),
        "the message must name both carriers; got: {err}"
    );
}

/// `bytes` with an `inference.spec_funcs` section recording each spec's function
/// indices, in the pre-link index space.
fn with_spec_funcs(bytes: &[u8], specs: &[(&str, &[u32])]) -> Vec<u8> {
    use wasm_encoder::Section as _;
    let mut payload = vec![1u8, u8::try_from(specs.len()).expect("fixture is small")];
    for (name, indices) in specs {
        payload.push(u8::try_from(name.len()).expect("fixture names are short"));
        payload.extend_from_slice(name.as_bytes());
        payload.push(u8::try_from(indices.len()).expect("fixture is small"));
        for &idx in *indices {
            payload.push(u8::try_from(idx).expect("fixture indices are small"));
        }
    }
    let mut out = bytes.to_vec();
    wasm_encoder::CustomSection {
        name: "inference.spec_funcs".into(),
        data: (&payload[..]).into(),
    }
    .append_to(&mut out);
    out
}

/// A specification function carrying an applied symbol's name does not make it
/// ambiguous — the link succeeds, as the translation of the same module does.
///
/// A spec-inner function's symbol is deliberately left unqualified by its
/// defining file, so it can share a string with the program's own function of
/// that name. No obligation may apply a specification function, so the shared
/// string leaves exactly one candidate; counting the spec function would fail a
/// link the translator would then have resolved correctly.
#[test]
fn a_spec_function_sharing_an_applied_symbol_does_not_make_it_ambiguous() {
    let main = with_spec_funcs(
        &with_hspecs(
            &main_with_names(&[], &["helper", "helper"]),
            &applying("helper"),
        ),
        &[("S", &[1])],
    );

    let linked = raw_link(&main, &[], None)
        .expect("a spec function of the same name must not make the symbol ambiguous");
    assert_valid(&linked);
}

/// When *every* carrier is a specification function the full set stands, so the
/// symbol still fails closed here rather than being quietly waved through.
#[test]
fn an_applied_symbol_carried_only_by_spec_functions_is_still_rejected() {
    let main = with_spec_funcs(
        &with_hspecs(
            &main_with_names(&[], &["helper", "helper"]),
            &applying("helper"),
        ),
        &[("S", &[0, 1])],
    );

    let err = raw_link(&main, &[], None)
        .expect_err("a symbol only specification functions carry must not link");
    assert!(
        matches!(
            &err,
            LinkError::AmbiguousObligationSymbol { symbol, carriers }
                if symbol == "helper" && carriers.len() == 2
        ),
        "expected the ambiguous-symbol rejection, got {err:?}"
    );
}

/// One spec carrier is deliberately let through the link, and rejected by the
/// translator that can say why.
///
/// The narrowing that drops specification carriers leaves nothing behind when
/// every carrier is one, so the count falls back to the full set — which for a
/// single spec function is one, and passes. That is not an oversight: the symbol
/// is unresolvable either way, and only the translator can name the target as an
/// omitted or a retained specification function. The link succeeding is the
/// contract, so it is pinned here rather than left to be "obviously" either way.
#[test]
fn an_applied_symbol_carried_only_by_one_spec_function_passes_the_link() {
    let main = with_spec_funcs(
        &with_hspecs(
            &main_with_names(&[], &["compute", "helper"]),
            &applying("helper"),
        ),
        &[("S", &[1])],
    );

    let linked = raw_link(&main, &[], None)
        .expect("a symbol one specification function carries is left to the translator");
    assert_valid(&linked);
}

/// The one-body-two-exports external at `main_with_names`' two-argument shape,
/// so a main module built there can import both of its aliases.
fn aliased_export_lib_i32x2() -> Vec<u8> {
    wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "double" (func 0))
          (export "twice" (func 0)))
        "#,
    )
}

/// A program function already named like an unrecorded root alias keeps the
/// obligation that applies it.
///
/// The alias rewrite exists to repair a name the merge could not record, and a
/// name some function *is* recorded under is not that. Rewriting it anyway would
/// move the obligation onto the merged body and then pass the ambiguity check
/// with a single carrier — a true claim about a function nobody wrote it about,
/// at exit 0. `link` takes arbitrary main bytes, so a `::`-joined program name is
/// reachable even though code generation cannot produce one.
#[test]
fn an_alias_a_program_function_already_carries_is_not_rewritten() {
    // `double` and `twice` are two exports of one body, so `mathlib::double`
    // is recorded and `mathlib::twice` is the alias the section drops — and a
    // program function is named exactly that.
    let main = with_hspecs(
        &main_with_names(&["double", "twice"], &["mathlib::twice"]),
        &applying("mathlib::twice"),
    );
    let linked = raw_link(&main, &[("mathlib", &aliased_export_lib_i32x2())], None)
        .expect("an obligation over the program's own function must link");
    assert_valid(&linked);

    assert_eq!(
        sole_applied_symbol(&linked),
        "mathlib::twice",
        "the obligation must keep naming the function the section records it against"
    );
    let names = function_names(&linked);
    assert!(
        names.contains(&(0, "mathlib::twice".to_string())),
        "the program's own function keeps the name, got {names:?}"
    );
    assert!(
        names.contains(&(1, "mathlib::double".to_string())),
        "and the merged body keeps the root name it could record, got {names:?}"
    );
}

/// A merge that changes no obligation symbol leaves the payload's bytes alone.
///
/// The alias rewrite runs on every link, so this pins that it is a no-op in the
/// common case: the emitted section is the canonical encoding of the map that
/// arrived, byte for byte.
#[test]
fn an_obligation_over_no_alias_survives_the_merge_byte_for_byte() {
    let map = applying("mathlib::sum");
    let payload = inference_hassert::encode(&map);
    let main = with_hspecs(&main_with_names(&["sum"], &["compute"]), &map);

    let linked = raw_link(&main, &[("mathlib", &mathlib_pure())], None)
        .expect("an obligation over a satisfied import must link");
    assert_eq!(
        custom_section_data(&linked, inference_hassert::HSPECS_SECTION_NAME)
            .expect("the linked module must carry the hspecs section"),
        payload,
        "a payload with no alias to rewrite must survive the merge verbatim"
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
            raw_link(&probe.main, &pairs, None)
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
/// (`lib::compute`); the inner callee must be filled with the marked
/// `<module>::#func_<out_idx>` (`lib::#func_2`) rather than left nameless (which
/// previously forced wasm-to-v down a per-process random-UUID path). The `lib::`
/// prefix sanitizes to `lib_` in the downstream Rocq names.
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
        names.iter().any(|(_, n)| n == "lib::compute"),
        "the closure root should be named `lib::compute`: {names:?}"
    );
    // Every merged function carries a name (a complete name section), and the
    // nameless inner callee is named deterministically from its output index,
    // under the same `<module>.` namespace as the root — never left out of the
    // section.
    assert!(
        names.iter().any(|(_, n)| n == "lib::#func_2"),
        "the nameless inner callee should get a deterministic `lib::#func_<idx>` name: {names:?}"
    );
    // No UUID-style name leaks in: a deterministic fallback name is a plain
    // `<module>::#func_<idx>` whose suffix after the mark parses as an integer.
    for (_, n) in &names {
        if let Some(suffix) = n.rsplit('#').next().and_then(|s| s.strip_prefix("func_")) {
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
// `SUPPORTED_WASM_FEATURES` is the integer WASM 1.0 core plus the two scalar
// post-MVP additions the merge models: bulk memory and sign-extension. An
// external using only these must pass the link gate and merge normally — the
// gate rejects *every* other post-1.0 proposal, including saturating
// float-to-int (the Rocq translator has no lowering for it), and all floating
// point (the Inference language has no `f32`/`f64` types).

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
fn sign_extension_external_passes_the_gate_and_merges() {
    // The sign-extension proposal is in the supported subset: the Rocq
    // translator lowers all five opcodes to `BI_unop t (Unop_extend n)`.
    // Inference codegen still emits none of them — it narrows sub-i32 values
    // with shifts and masks — but a real toolchain emits them constantly, and
    // this gate is what decides whether such an external gets as far as the
    // allow-list at all. Without `WasmFeatures::SIGN_EXTENSION` the validator
    // refuses the module before any body is scanned, so the allow-list entry
    // alone would be unreachable.
    let main = main_importing_f();
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32) (result i32)
            local.get 0
            i32.extend8_s
            i32.extend16_s)
          (export "f" (func 0)))
        "#,
    );
    let linked = link(&main, &[&lib]).expect("a sign-extension external must link");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    assert_eq!(code_body_count(&linked), 2);
}

#[test]
fn integer_width_conversion_external_passes_the_gate_and_merges() {
    // The three integer-to-integer width conversions are MVP instructions, so no
    // feature flag ever gated them; the allow-list was the only thing that
    // refused them, and it no longer does. Paired with the sign-extension case
    // above because the two halves of the numeric envelope were refused in
    // different places — one at the validator, one at the allow-list — and a
    // change that lifted only one would leave this pair split.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i64) (result i32)))
          (import "lib" "f" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i64) (result i32)
            local.get 0
            call 0)
          (export "run" (func 1)))
        "#,
    );
    let lib = wasm(
        r#"
        (module
          (type (;0;) (func (param i64) (result i32)))
          (func (;0;) (type 0) (param i64) (result i32)
            local.get 0
            i32.wrap_i64
            i64.extend_i32_s
            i32.wrap_i64
            i64.extend_i32_u
            i32.wrap_i64)
          (export "f" (func 0)))
        "#,
    );
    let linked = link(&main, &[&lib]).expect("an integer-width-conversion external must link");
    assert_valid(&linked);
    assert!(function_imports(&linked).is_empty());
    assert_eq!(code_body_count(&linked), 2);
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

/// The checked write-set mode, driven directly with hand-built contracts.
///
/// Every other test in this file takes the unchecked mode, because a WAT fixture
/// has no `external fn` declaration behind it. These supply the declaration the
/// front end would have produced, which lets the two modes be compared on
/// *identical bytes* — the only way to show that a rejection came from the
/// contract and not from the closure.
mod declared_write_sets {
    use super::*;
    use inference_wasm_linker::ImportWriteSet;

    /// A main importing `memlib::store_at(ptr, val)` and calling it.
    fn main_calling_store_at() -> Vec<u8> {
        wasm(
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
        )
    }

    /// `store_at(ptr, val)`: writes `val` at the caller's `ptr`. Tier B, and its
    /// write set is `{0}`.
    fn storing_lib() -> Vec<u8> {
        wasm(
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
        )
    }

    /// The same signature over the same memory, reading instead of writing, so
    /// the pair differs only in the operator under test.
    fn reading_lib() -> Vec<u8> {
        wasm(
            r#"
            (module
              (type (;0;) (func (param i32 i32)))
              (memory (;0;) 1)
              (func (;0;) (type 0) (param i32 i32)
                local.get 0
                i32.load
                drop)
              (export "store_at" (func 0)))
            "#,
        )
    }

    fn contract(mut_params: &[u32], param_names: &[Option<&str>]) -> Vec<ImportWriteSet> {
        vec![ImportWriteSet {
            module: "memlib".to_string(),
            field: "store_at".to_string(),
            mut_params: mut_params.to_vec(),
            param_names: param_names
                .iter()
                .map(|n| n.map(str::to_string))
                .collect(),
        }]
    }

    #[test]
    fn an_empty_declaration_rejects_a_closure_that_stores() {
        let contracts = contract(&[], &[Some("ptr"), Some("val")]);
        let err = raw_link(
            &main_calling_store_at(),
            &[("memlib", &storing_lib())],
            Some(&contracts),
        )
        .expect_err("a declaration claiming no write must not admit a storing body");
        assert!(
            matches!(
                &err,
                LinkError::UndeclaredExternWrite { module, field, param_index, param_name }
                    if module == "memlib"
                        && field == "store_at"
                        && *param_index == 0
                        && param_name.as_deref() == Some("ptr")
            ),
            "the rejection must name the import and the offending parameter, got {err:?}"
        );
    }


    /// The same signature writing through the **second** parameter, so a
    /// rejection that names parameter 0 has read nothing.
    fn stores_through_the_second_parameter() -> Vec<u8> {
        wasm(
            r#"
            (module
              (type (;0;) (func (param i32 i32)))
              (memory (;0;) 1)
              (func (;0;) (type 0) (param i32 i32)
                local.get 1
                local.get 0
                i32.store)
              (export "store_at" (func 0)))
            "#,
        )
    }

    #[test]
    fn an_empty_declaration_names_the_parameter_the_body_actually_writes() {
        // `an_empty_declaration_rejects_a_closure_that_stores` cannot tell the
        // attributed index from a default: its body writes through parameter 0,
        // which is also what any degenerate answer would say. This body writes
        // through parameter 1, so the reported index and name can only come from
        // the attribution.
        let contracts = contract(&[], &[Some("ptr"), Some("val")]);
        let err = raw_link(
            &main_calling_store_at(),
            &[("memlib", &stores_through_the_second_parameter())],
            Some(&contracts),
        )
        .expect_err("a declaration claiming no write must not admit a storing body");
        assert!(
            matches!(
                &err,
                LinkError::UndeclaredExternWrite { param_index, param_name, .. }
                    if *param_index == 1 && param_name.as_deref() == Some("val")
            ),
            "the rejection must name parameter 1 (`val`), the one the body stores \
             through, got {err:?}"
        );
    }

    #[test]
    fn a_non_empty_declaration_that_covers_the_wrong_parameter_is_rejected() {
        // The `D != 0` arm, which the structural no-store check never reaches:
        // the declaration does claim a write, just not the one the body performs.
        // Only the attributed set can separate this from an accepted link.
        let contracts = contract(&[1], &[Some("ptr"), Some("val")]);
        let err = raw_link(
            &main_calling_store_at(),
            &[("memlib", &storing_lib())],
            Some(&contracts),
        )
        .expect_err("declaring `mut val` does not cover a store through `ptr`");
        assert!(
            matches!(
                &err,
                LinkError::UndeclaredExternWrite { param_index, param_name, .. }
                    if *param_index == 0 && param_name.as_deref() == Some("ptr")
            ),
            "the rejection must name the uncovered parameter, got {err:?}"
        );
    }

    #[test]
    fn a_declaration_covering_the_write_admits_the_same_bytes() {
        let contracts = contract(&[0], &[Some("ptr"), Some("val")]);
        raw_link(
            &main_calling_store_at(),
            &[("memlib", &storing_lib())],
            Some(&contracts),
        )
        .expect("declaring `mut ptr` covers the only store the body performs");
    }

    #[test]
    fn the_unchecked_mode_admits_what_the_checked_mode_rejects() {
        // The same bytes the first test refuses, with no contract supplied. This
        // is what makes the two modes distinguishable rather than a nullable
        // spelling of one: the verdict differs on identical input, so `None`
        // cannot be reconstructed from an empty list.
        raw_link(&main_calling_store_at(), &[("memlib", &storing_lib())], None)
            .expect("the unchecked mode performs merge mechanics only");
    }

    #[test]
    fn an_import_no_contract_mentions_is_held_to_writing_nothing() {
        // Fail-closed: a missing entry is an empty declaration, never an
        // exemption. Supplying a contract for a *different* field leaves
        // `store_at` unmentioned, and the storing body must still be refused.
        let contracts = vec![ImportWriteSet {
            module: "memlib".to_string(),
            field: "load_at".to_string(),
            mut_params: vec![0],
            param_names: vec![Some("ptr".to_string())],
        }];
        let err = raw_link(
            &main_calling_store_at(),
            &[("memlib", &storing_lib())],
            Some(&contracts),
        )
        .expect_err("an unmentioned import must be held to an empty write set");
        assert!(
            matches!(
                &err,
                LinkError::UndescribedExternWrite { module, field, param_index }
                    if module == "memlib" && field == "store_at" && *param_index == 0
            ),
            "the unmentioned import must be the one rejected, got {err:?}"
        );
    }

    #[test]
    fn an_unmentioned_import_is_not_told_it_used_an_unnamed_parameter() {
        // The two ways a rejection can arrive with no parameter name in hand are
        // different situations, and the message must not conflate them. An
        // unnamed parameter is a declaration the linker read, whose repair is to
        // give the parameter a name; an unmentioned import has no declaration
        // behind it at all, so that advice would describe a declaration that
        // played no part in the verdict — and, on the live pipeline, one whose
        // parameters are in fact all named.
        let contracts = vec![ImportWriteSet {
            module: "memlib".to_string(),
            field: "load_at".to_string(),
            mut_params: vec![0],
            param_names: vec![Some("ptr".to_string())],
        }];
        let err = raw_link(
            &main_calling_store_at(),
            &[("memlib", &storing_lib())],
            Some(&contracts),
        )
        .expect_err("an unmentioned import must be held to an empty write set");
        let text = err.to_string();
        assert!(
            text.contains("describes no such import"),
            "the message must say the contract did not describe the import, got: {text}"
        );
        assert!(
            !text.contains("unnamed form"),
            "an unmentioned import must not be told its parameter was unnamed, got: {text}"
        );
    }

    #[test]
    fn an_unnamed_parameter_still_gets_the_naming_advice() {
        // The control for the test above: a contract that *does* describe the
        // import but writes the offending parameter without a name keeps the
        // advice that is specific to it. Without this the previous assertion
        // would also pass if the naming branch had simply been deleted.
        let contracts = contract(&[], &[None, None]);
        let err = raw_link(
            &main_calling_store_at(),
            &[("memlib", &storing_lib())],
            Some(&contracts),
        )
        .expect_err("a declaration claiming no write must not admit a storing body");
        let text = err.to_string();
        assert!(
            matches!(
                &err,
                LinkError::UndeclaredExternWrite { param_name, .. } if param_name.is_none()
            ),
            "an unnamed parameter must report no name, got {err:?}"
        );
        assert!(
            text.contains("unnamed form"),
            "an unnamed parameter must keep the advice to name it first, got: {text}"
        );
    }

    #[test]
    fn a_read_only_closure_satisfies_an_unmentioned_import() {
        // The other half: an unmentioned import is held to writing nothing, and a
        // body that only loads meets that claim. Without this, the rejection
        // above could be a blanket refusal of every unmentioned import rather
        // than the write-set check it is meant to be.
        let contracts = vec![ImportWriteSet {
            module: "memlib".to_string(),
            field: "load_at".to_string(),
            mut_params: vec![0],
            param_names: vec![Some("ptr".to_string())],
        }];
        raw_link(
            &main_calling_store_at(),
            &[("memlib", &reading_lib())],
            Some(&contracts),
        )
        .expect("a closure that never stores writes nothing, which is what nothing declared says");
    }

    /// A contract list is a map written as a slice, so two entries for one
    /// `(module, field)` have to be refused rather than resolved by position.
    ///
    /// The pair below is chosen so that order alone flips the verdict: the
    /// permissive entry covers the body's store and the restrictive one does
    /// not. If the duplicate were resolved by first match, one ordering would
    /// link the very bytes the other refuses — which is what a caller who wrote
    /// the two entries in either order would never be told.
    #[test]
    fn two_entries_for_one_import_are_refused_in_either_order() {
        let permissive = ImportWriteSet {
            module: "memlib".to_string(),
            field: "store_at".to_string(),
            mut_params: vec![0],
            param_names: vec![Some("ptr".to_string()), Some("val".to_string())],
        };
        let restrictive = ImportWriteSet {
            mut_params: Vec::new(),
            ..permissive.clone()
        };

        for (label, contracts) in [
            (
                "permissive first",
                vec![permissive.clone(), restrictive.clone()],
            ),
            ("restrictive first", vec![restrictive, permissive]),
        ] {
            let err = raw_link(
                &main_calling_store_at(),
                &[("memlib", &storing_lib())],
                Some(&contracts),
            )
            .err()
            .unwrap_or_else(|| panic!("a duplicated contract key must be refused ({label})"));
            assert!(
                matches!(
                    &err,
                    LinkError::DuplicateWriteContract { module, field }
                        if module == "memlib" && field == "store_at"
                ),
                "the rejection must name the duplicated import ({label}), got {err:?}"
            );
        }
    }

    #[test]
    fn two_entries_for_different_imports_are_not_a_duplicate() {
        // The control: the check keys on the whole `(module, field)` pair, not on
        // either half. A list naming two distinct imports must link normally.
        let contracts = vec![
            ImportWriteSet {
                module: "memlib".to_string(),
                field: "store_at".to_string(),
                mut_params: vec![0],
                param_names: vec![Some("ptr".to_string()), Some("val".to_string())],
            },
            ImportWriteSet {
                module: "memlib".to_string(),
                field: "load_at".to_string(),
                mut_params: vec![0],
                param_names: vec![Some("ptr".to_string())],
            },
        ];
        raw_link(
            &main_calling_store_at(),
            &[("memlib", &storing_lib())],
            Some(&contracts),
        )
        .expect("distinct imports in one list are not a duplicated key");
    }

    #[test]
    fn a_read_only_closure_satisfies_an_empty_declaration() {
        // The other half of the first test: the licence an empty declaration
        // grants is real, and a body that only loads earns it.
        let contracts = contract(&[], &[Some("ptr"), Some("val")]);
        raw_link(
            &main_calling_store_at(),
            &[("memlib", &reading_lib())],
            Some(&contracts),
        )
        .expect("a closure that never stores is covered by a declaration claiming no write");
    }

    #[test]
    fn a_declared_parameter_the_body_never_writes_is_not_an_error() {
        // `mut` is a permission, not an obligation. A declaration is free to
        // over-approximate — a library whose write depends on its arguments could
        // not be declared at all otherwise — so an unexercised `mut` must link.
        let contracts = contract(&[0, 1], &[Some("ptr"), Some("val")]);
        raw_link(
            &main_calling_store_at(),
            &[("memlib", &reading_lib())],
            Some(&contracts),
        )
        .expect("a `mut` the body never exercises must not itself be a rejection");
    }

    #[test]
    fn a_tier_a_closure_satisfies_every_contract() {
        // Tier A means no memory access at all, so the closure's store set is
        // empty by construction: every memory arm of the safety allow-list sets
        // `uses_memory`, which is exactly what routes a closure to Tier B. The
        // invariant is pinned here rather than re-derived by a second scan that
        // could disagree with the flag the tier itself turns on.
        let main = wasm(
            r#"
            (module
              (type (;0;) (func (param i32 i32) (result i32)))
              (import "memlib" "store_at" (func (;0;) (type 0)))
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
              (export "store_at" (func 0)))
            "#,
        );
        let contracts = contract(&[], &[Some("ptr"), Some("val")]);
        raw_link(&main, &[("memlib", &lib)], Some(&contracts))
            .expect("a memoryless closure writes nothing, so the strictest contract holds");
    }

    #[test]
    fn an_unnamed_parameter_is_reported_without_a_name() {
        // The front end supplies `None` for a parameter written in an unnamed
        // form, and the rejection has to carry that through: the fix it teaches
        // ("name it first") is the only one available, and a fabricated name
        // would send the author looking for a spelling that does not exist.
        let contracts = contract(&[], &[None, None]);
        let err = raw_link(
            &main_calling_store_at(),
            &[("memlib", &storing_lib())],
            Some(&contracts),
        )
        .expect_err("an unnamed parameter declares no write set");
        assert!(
            matches!(&err, LinkError::UndeclaredExternWrite { param_name, .. } if param_name.is_none()),
            "an unnamed parameter must be reported as unnamed, got {err:?}"
        );
        let rendered = err.to_string();
        assert!(
            rendered.contains("unnamed form") && rendered.contains("give it a name first"),
            "the message must teach the name-it-first fix; got: {rendered}"
        );
    }
}
