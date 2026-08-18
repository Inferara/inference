//! Multi-file `external fn` scoping, executed through Wasmtime.
//!
//! An `external fn` is declared in one file and can be named only there. The
//! code generation seam that has to respect that is the call-site probe: a bare
//! callee name is resolved to the declaration the *calling* file can see before
//! it becomes an import index. A whole-program name table gets that wrong in a
//! way nothing downstream reports — the module validates either way, and a call
//! to an import type-checks against the same signature, so the only observable
//! difference is the value that comes back.
//!
//! Two rules meet here, and both are pinned below. A bare name shared by an
//! `external fn` and a function is rejected outright, so the program that
//! motivated this file never reaches code generation. A bare name shared by two
//! `external fn`s in two files stays legal, and *that* is the program the probe
//! still has to resolve per file.
//!
//! The foreign modules are supplied as host functions rather than statically
//! linked, so each import is backed by a body whose result is unmistakable:
//! `libA` multiplies by 999 and `libB` by 7, values nothing in the program
//! produces.

use crate::utils::{
    proof_wasm_codegen_multi_file, try_type_check_multi_file, wasm_codegen_multi_file,
};

use wasmtime::{Engine, Instance, Linker, Module, Store, TypedFunc};

/// The entry file defines its own `scale` and calls it. The sibling declares an
/// `external fn scale` and binds it to `libA`. Both calls are written
/// `scale(..)`, and only the file each is written in says which declaration it
/// means — which is what makes the shared name unreadable, and why the program
/// is rejected even though each call resolves correctly.
const ENTRY: &str = "\
use side;

fn scale(x: i32) -> i32 {
    return x * 10;
}

pub fn run(x: i32) -> i32 {
    return scale(x);
}
";

const SIDE: &str = "\
external fn scale(a: i32) -> i32;
use { scale } from libA;

pub fn boost(a: i32) -> i32 {
    return scale(a);
}
";

/// Instantiates `wasm_bytes` with `libA.scale` and `libB.scale` supplied by the
/// host. A module that imports only one of them still instantiates: the linker
/// is asked for the names the module names.
fn instantiate_with_host_libraries(wasm_bytes: &[u8]) -> (Store<()>, Instance) {
    inf_wasmparser::validate(wasm_bytes)
        .unwrap_or_else(|e| panic!("generated multi-file Wasm module is invalid: {e}"));
    let engine = Engine::default();
    let module = Module::new(&engine, wasm_bytes)
        .unwrap_or_else(|e| panic!("failed to create Wasm module: {e}"));
    let mut linker = Linker::new(&engine);
    linker
        .func_wrap("libA", "scale", |a: i32| a * 999)
        .expect("libA.scale is supplied by the host");
    linker
        .func_wrap("libB", "scale", |a: i32| a * 7)
        .expect("libB.scale is supplied by the host");
    let mut store = Store::new(&engine, ());
    let instance = linker
        .instantiate(&mut store, &module)
        .unwrap_or_else(|e| panic!("failed to instantiate Wasm module: {e}"));
    (store, instance)
}

/// The entry file's `fn scale` and the sibling's `external fn scale` share one
/// bare name, and the program is rejected before code generation.
///
/// The rejection is about the name, not about resolution: once the call-site
/// probe was keyed on the declaring file this program compiled and `run(2)` came
/// back 20, the entry's own `scale`. It is rejected because a reader cannot see
/// from `scale(x)` whether the callee is compiled here or linked in — so the
/// message states that rule and names both declarations, rather than reporting
/// a resolution the compiler in fact performs.
#[test]
fn a_siblings_extern_and_the_entry_files_function_collide() {
    let Err(err) = try_type_check_multi_file(&[(vec![], ENTRY), (vec!["side"], SIDE)]) else {
        panic!("a function and an `external fn` of one name must be rejected");
    };
    let message = err.to_string();
    assert!(
        message.contains("side:1:1: `external fn scale` and the function `scale` share one name")
            && message.contains("note: the function `scale` is defined at 3:1 in the entry file"),
        "both declarations must be named with their locations: {message}"
    );
}

/// Two files each declare `external fn scale` and bind it to a **different**
/// library. Each file's call must reach the import its own declaration
/// registered.
///
/// This is the program a whole-program name table cannot express: it holds one
/// `scale` for the whole program, so one of the two files ends up calling the
/// other's library. Nothing before execution tells the two apart — the
/// declarations agree on name and signature, so the module type-checks and
/// validates whichever import each call is wired to.
#[test]
fn two_files_binding_one_name_call_their_own_library() {
    const ENTRY_TWO_LIBRARIES: &str = "\
use side;

external fn scale(a: i32) -> i32;
use { scale } from libA;

pub fn from_a(x: i32) -> i32 {
    return scale(x);
}

pub fn from_b(x: i32) -> i32 {
    return side::via_b(x);
}
";

    const SIDE_TWO_LIBRARIES: &str = "\
external fn scale(a: i32) -> i32;
use { scale } from libB;

pub fn via_b(x: i32) -> i32 {
    return scale(x);
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], ENTRY_TWO_LIBRARIES),
        (vec!["side"], SIDE_TWO_LIBRARIES),
    ]);
    let (mut store, instance) = instantiate_with_host_libraries(&wasm);
    let from_a: TypedFunc<i32, i32> = instance
        .get_typed_func(&mut store, "from_a")
        .expect("`from_a` is exported from the entry file");
    let from_b: TypedFunc<i32, i32> = instance
        .get_typed_func(&mut store, "from_b")
        .expect("`from_b` is exported from the entry file");
    assert_eq!(
        from_a.call(&mut store, 2).expect("from_a(2) executes"),
        1998,
        "the entry file's `scale` is bound to `libA`, so `from_a(2)` is 2 * 999"
    );
    assert_eq!(
        from_b.call(&mut store, 2).expect("from_b(2) executes"),
        14,
        "the sibling's `scale` is bound to `libB`, so `from_b(2)` is 2 * 7"
    );
}

/// The entry file declares, binds and calls its own `external fn scale`; the
/// sibling declares one too and never binds or calls it.
///
/// The sibling's declaration is inert — nothing in the program refers to it —
/// and the entry file's program is byte-identical to one where the sibling does
/// not exist. A whole-program name table holds one declaration per name, so the
/// inert one can displace the bound one and leave the entry's working call
/// unbound: rejected by A024 if analysis runs, and reaching nothing if it does
/// not.
#[test]
fn an_inert_sibling_declaration_does_not_unbind_the_entrys_call() {
    const ENTRY_BOUND: &str = "\
use side;

external fn scale(a: i32) -> i32;
use { scale } from libA;

pub fn run(x: i32) -> i32 {
    return scale(x) + side::helper();
}
";

    const SIDE_INERT: &str = "\
external fn scale(a: i32) -> i32;

pub fn helper() -> i32 {
    return 0;
}
";

    let wasm = wasm_codegen_multi_file(&[(vec![], ENTRY_BOUND), (vec!["side"], SIDE_INERT)]);
    let (mut store, instance) = instantiate_with_host_libraries(&wasm);
    let run: TypedFunc<i32, i32> = instance
        .get_typed_func(&mut store, "run")
        .expect("`run` is exported from the entry file");
    assert_eq!(
        run.call(&mut store, 2).expect("run(2) executes"),
        1998,
        "the entry file's own binding must survive an inert sibling declaration"
    );
}

/// A `spec`-inner function shares its bare name with an `external fn` another
/// file declares and binds. The call written beside it inside the `spec` must
/// reach that sibling function, not the import the name registered elsewhere.
///
/// This is the shape where the two answers still differ and neither is an error.
/// A *top-level* function sharing a name with an `external fn` is rejected
/// outright, so the collision can no longer hide a wrong call — but a
/// `spec`-inner declaration is namespaced under its `spec` and takes no part in
/// that rule, so this program is legal and the call has a local target to reach.
/// A probe that widened back out to a program-wide name table would find the
/// import, emit `call <import_idx>`, and produce a module that validates and
/// links: the obligation would then be about the foreign body rather than the
/// function the specification is written about. `wasm_codegen_extern_out_of_scope`
/// is what records that the probe saw the name as an import elsewhere and still
/// declined it here.
///
/// Proof mode, because compile mode emits no specification function at all.
#[test]
fn a_spec_inner_function_is_not_displaced_by_a_siblings_import() {
    cov_mark::check_count!(wasm_codegen_extern_out_of_scope, 1);

    const ENTRY_BINDS_SCALE: &str = "\
use side;

external fn scale(a: i32) -> i32;
use { scale } from libA;

pub fn from_a(x: i32) -> i32 {
    return scale(x);
}

pub fn touch(x: i32) -> i32 {
    return side::helper(x);
}
";

    const SIDE_SPEC_SCALE: &str = "\
spec S {
    fn scale(a: i32) -> i32 {
        assert(a == a);
        return a * 10;
    }

    fn probe() forall {
        let x: i32 = @;
        assert(scale(x) == x * 10);
    }
}

pub fn helper(x: i32) -> i32 {
    return x;
}
";

    let wasm = proof_wasm_codegen_multi_file(&[
        (vec![], ENTRY_BINDS_SCALE),
        (vec!["side"], SIDE_SPEC_SCALE),
    ]);
    inf_wasmparser::validate(&wasm)
        .unwrap_or_else(|e| panic!("proof-mode multi-file module is invalid: {e}"));
}
