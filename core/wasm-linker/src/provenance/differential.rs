//! Differential tests: what the analysis *says* against what a module *does*.
//!
//! Every test in `provenance/tests.rs` asserts a verdict. None of them observes
//! a single byte of memory, so none of them can catch the failure that matters
//! most here — a closure the analysis admits which nevertheless reaches a fixed
//! absolute address. That is not a wrong verdict the unit tests can see; it is a
//! wrong verdict about *behaviour*, and only running the module reveals it.
//!
//! Each fixture below is executed under wasmtime twice, with two argument
//! vectors that share no pointer value, and the set of bytes it modified is
//! recorded each time. The two runs are then compared:
//!
//! - **If the analysis accepted**, the two written sets must be **disjoint**.
//!   Every admitted address is claimed to be a bijection in a caller argument,
//!   so moving the arguments must move every byte the module touches. A byte
//!   written at the same address on both runs is an address the module pinned,
//!   and pinning one is precisely what Tier B forbids.
//! - **If the fixture is declared a fabricator**, the two written sets must
//!   **intersect** — the fixture has to actually reach a fixed address, or its
//!   rejection test would be passing for no reason at all.
//!
//! The analysis is allowed to over-reject, so nothing here requires a rejected
//! fixture to be dangerous unless it says it is.

use wasmtime::{Engine, Instance, Module, Store, Val};

use super::tests::{NegateWith, correlated_recursive_root_wat, doubling_to_zero_wat};
use super::verify_param_addressing;
use crate::parse::ParsedModule;

/// One executable fixture: a module, the arguments to run it with, and whether
/// it provably reaches an address no caller chose.
struct Fixture {
    name: &'static str,
    wat: String,
    /// The closure's function indices and its root, as the linker would compute
    /// them for the exported `f`.
    func_indices: Vec<u32>,
    root: u32,
    /// Two argument vectors sharing no pointer value.
    args: [Vec<i32>; 2],
    /// The verdict this fixture must draw. Pinned so the table cannot drift into
    /// all-rejections, where the "admitted implies disjoint" check would hold
    /// vacuously for every row.
    expect_accept: bool,
    /// Whether the module reaches the same address on both runs by construction.
    /// A fixture claiming this must demonstrate it, so a rejection test cannot
    /// quietly become vacuous.
    fabricates: bool,
}

/// The set of memory offsets a run of `f` modified, as a sorted vector.
fn written_offsets(wasm: &[u8], args: &[i32]) -> Vec<usize> {
    let engine = Engine::default();
    let module = Module::new(&engine, wasm).expect("fixture module compiles");
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("fixture instantiates");

    let func = instance
        .get_func(&mut store, "f")
        .expect("fixture exports f");
    let result_count = func.ty(&store).results().len();
    let params: Vec<Val> = args.iter().map(|&a| Val::I32(a)).collect();
    let mut results = vec![Val::I32(0); result_count];
    func.call(&mut store, &params, &mut results)
        .expect("fixture runs without trapping");

    let memory = instance
        .get_memory(&mut store, "mem")
        .expect("fixture exports mem");
    memory
        .data(&store)
        .iter()
        .enumerate()
        .filter(|&(_, &byte)| byte != 0)
        .map(|(offset, _)| offset)
        .collect()
}

/// Whether the analysis admits the fixture's closure.
fn analysis_accepts(fixture: &Fixture, wasm: &[u8]) -> bool {
    let module = ParsedModule::parse(wasm).expect("fixture parses");
    verify_param_addressing(&module, &fixture.func_indices, fixture.root, "f").is_ok()
}

#[test]
fn analysis_verdicts_match_what_the_modules_actually_do() {
    for fixture in fixtures() {
        let wasm = wat::parse_str(&fixture.wat).expect("fixture is valid WAT");
        let accepted = analysis_accepts(&fixture, &wasm);

        let first = written_offsets(&wasm, &fixture.args[0]);
        let second = written_offsets(&wasm, &fixture.args[1]);
        let shared: Vec<usize> = first
            .iter()
            .copied()
            .filter(|offset| second.contains(offset))
            .collect();

        assert!(
            !first.is_empty() && !second.is_empty(),
            "{}: fixture wrote nothing, so it observes no address at all",
            fixture.name
        );
        assert_eq!(
            accepted, fixture.expect_accept,
            "{}: the analysis verdict moved",
            fixture.name
        );

        if accepted {
            assert!(
                shared.is_empty(),
                "{}: ADMITTED by the analysis, yet it wrote {:?} on both runs — \
                 an address the caller's arguments do not move",
                fixture.name,
                shared
            );
        }

        if fixture.fabricates {
            assert!(
                !shared.is_empty(),
                "{}: declared a fabricator, but no address survived changing every \
                 argument — the rejection test it backs would be vacuous",
                fixture.name
            );
            assert!(
                !accepted,
                "{}: reaches a fixed address and must not be admitted",
                fixture.name
            );
        }
    }
}

fn fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            name: "store_through_the_parameter",
            wat: r#"(module (memory (export "mem") 1)
                     (func (export "f") (param i32)
                       local.get 0 i32.const 170 i32.store8))"#
                .to_string(),
            func_indices: vec![0],
            root: 0,
            args: [vec![100], vec![5000]],
            expect_accept: true,
            fabricates: false,
        },
        Fixture {
            name: "base_plus_shifted_index",
            wat: r#"(module (memory (export "mem") 1)
                     (func (export "f") (param i32 i32)
                       local.get 0 local.get 1 i32.const 2 i32.shl i32.add
                       i32.const 170 i32.store8))"#
                .to_string(),
            func_indices: vec![0],
            root: 0,
            args: [vec![100, 3], vec![5000, 7]],
            expect_accept: true,
            fabricates: false,
        },
        Fixture {
            name: "scaled_index_inside_a_helper",
            wat: r#"(module (memory (export "mem") 1)
                     (type (;0;) (func (param i32 i32)))
                     (func (export "f") (type 0) (param i32 i32)
                       local.get 0 local.get 1 call 1)
                     (func (;1;) (type 0) (param i32 i32)
                       local.get 0 local.get 1 i32.const 2 i32.shl i32.add
                       i32.const 170 i32.store8))"#
                .to_string(),
            func_indices: vec![0, 1],
            root: 0,
            args: [vec![100, 3], vec![5000, 7]],
            expect_accept: true,
            fabricates: false,
        },
        Fixture {
            name: "two_parameter_extent_fill",
            wat: r#"(module (memory (export "mem") 1)
                     (type (;0;) (func (param i32 i32 i32)))
                     (func (export "f") (type 0) (param i32 i32 i32)
                       local.get 0 local.get 1 local.get 2 call 1)
                     (func (;1;) (type 0) (param i32 i32 i32)
                       local.get 0
                       i32.const 170
                       local.get 1 local.get 2 i32.add
                       memory.fill))"#
                .to_string(),
            func_indices: vec![0, 1],
            root: 0,
            args: [vec![100, 3, 4], vec![5000, 5, 6]],
            expect_accept: true,
            fabricates: false,
        },
        Fixture {
            name: "multiply_by_zero_reaches_4096",
            wat: r#"(module (memory (export "mem") 1)
                     (func (export "f") (param i32)
                       local.get 0 i32.const 0 i32.mul i32.const 4096 i32.add
                       i32.const 170 i32.store8))"#
                .to_string(),
            func_indices: vec![0],
            root: 0,
            args: [vec![100], vec![5000]],
            expect_accept: false,
            fabricates: true,
        },
        Fixture {
            name: "thirty_two_doublings_reach_4096",
            wat: doubling_to_zero_wat(),
            func_indices: vec![0],
            root: 0,
            args: [vec![100], vec![5000]],
            expect_accept: false,
            fabricates: true,
        },
        Fixture {
            name: "correlated_recursive_root_reaches_4096",
            wat: correlated_recursive_root_wat(NegateWith::Multiply),
            func_indices: vec![0],
            root: 0,
            args: [vec![100, 7, 1], vec![5000, 9, 1]],
            expect_accept: false,
            fabricates: true,
        },
        Fixture {
            name: "correlated_recursive_root_by_doubling_reaches_4096",
            wat: correlated_recursive_root_wat(NegateWith::Doubling),
            func_indices: vec![0],
            root: 0,
            args: [vec![100, 7, 1], vec![5000, 9, 1]],
            expect_accept: false,
            fabricates: true,
        },
    ]
}
