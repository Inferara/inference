//! Semantic execution tests for the static-merge linker (issue #9, audit S5).
//!
//! Every other linker test asserts only *structure* plus `inf_wasmparser::validate`
//! passing. That leaves a soundness hole the audit named S5: a re-index bug that
//! produced a **validating** module which nonetheless wired an import onto the
//! *wrong* same-signature body would pass every structural assertion. Two external
//! functions with identical signatures (`sum`/`sub`, `store_at`/`load_at`) are the
//! canonical trap — swapping their merged bodies keeps the module valid but changes
//! what it computes.
//!
//! These tests close that hole by **executing** the merged module. Each fixture is
//! assembled from inline WAT (mirroring the linker's own integration tests), driven
//! through the real `inference::link`, instantiated in `wasmtime`, and asserted on
//! the *computed result* — a value chosen to distinguish correct wiring from the
//! plausible swap. The merged module exports its shared memory and its entry
//! function, so a Tier-B round-trip can be observed directly through that memory.

#[cfg(test)]
mod extern_link_exec_tests {
    use inference::link;
    use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

    /// Assembles a `.wasm` binary from WAT, panicking with the WAT on error.
    fn wasm(wat: &str) -> Vec<u8> {
        wat::parse_str(wat).unwrap_or_else(|e| panic!("invalid WAT fixture: {e}\n{wat}"))
    }

    /// Instantiates `wasm` with no imports (the merge removes them all) and hands
    /// the caller the live `Store`/`Instance` to read exports from.
    fn instantiate(wasm: &[u8]) -> (Store<()>, Instance) {
        let engine = Engine::default();
        let module =
            Module::new(&engine, wasm).unwrap_or_else(|e| panic!("merged module rejected: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("merged module failed to instantiate: {e}"));
        (store, instance)
    }

    #[test]
    fn tier_a_merge_wires_each_same_signature_extern_to_its_own_body() {
        // Two externals with the *same* `(i32,i32)->i32` signature. `run` calls
        // both, so a re-index that swapped the merged bodies would still validate
        // but compute a different number. `run(a,b) = sum(a,b) - sub(a,b)`:
        //   correct  = (a+b) - (a-b) = 2b
        //   swapped  = (a-b) - (a+b) = -2b
        // run(7,3) is 6 when wired correctly and -6 under a swap — distinct, and
        // distinct from the also-plausible "both wired to sum" (0) and "both wired
        // to sub" (0) miswirings.
        let main = wasm(
            r#"
            (module
              (type (;0;) (func (param i32 i32) (result i32)))
              (import "arith" "sum" (func (;0;) (type 0)))
              (import "arith" "sub" (func (;1;) (type 0)))
              (func (;2;) (type 0) (param i32 i32) (result i32)
                local.get 0
                local.get 1
                call 0
                local.get 0
                local.get 1
                call 1
                i32.sub)
              (export "run" (func 2)))
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
                i32.sub)
              (export "sum" (func 0))
              (export "sub" (func 1)))
            "#,
        );

        let linked = link(&main, &[("arith", &lib)]).expect("Tier-A merge succeeds");
        inf_wasmparser::validate(&linked).expect("merged module is valid wasm");

        let (mut store, instance) = instantiate(&linked);
        let run: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "run")
            .expect("merged module exports `run`");

        assert_eq!(
            run.call(&mut store, (7, 3)).expect("run(7,3) executes"),
            6,
            "run(a,b) must be sum-then-sub = 2b; a swapped wiring would yield -6"
        );
        assert_eq!(
            run.call(&mut store, (10, 4)).expect("run(10,4) executes"),
            8,
            "second point pins 2b again, ruling out a constant-offset coincidence"
        );
        assert_eq!(
            run.call(&mut store, (3, 0)).expect("run(3,0) executes"),
            0,
            "b=0 collapses both directions to 0, guarding the sign of the wiring"
        );
    }

    #[test]
    fn tier_b_store_load_round_trips_through_shared_memory() {
        // Two externals over the main module's shared memory: `store_at(ptr,val)`
        // writes, `load_at(ptr)` reads. Both touch memory only through their
        // caller-passed pointer — Tier B. `run(ptr,val)` stores then loads back the
        // same address, so a correct merge round-trips the value. Storing at one
        // address and loading another (`isolate`) confirms the two distinct-but-
        // same-family bodies were not collapsed onto one address.
        let main = wasm(
            r#"
            (module
              (type (;0;) (func (param i32 i32)))
              (type (;1;) (func (param i32) (result i32)))
              (type (;2;) (func (param i32 i32) (result i32)))
              (import "memlib" "store_at" (func (;0;) (type 0)))
              (import "memlib" "load_at" (func (;1;) (type 1)))
              (memory (;0;) 1 1)
              (func (;2;) (type 2) (param i32 i32) (result i32)
                local.get 0
                local.get 1
                call 0
                local.get 0
                call 1)
              (func (;3;) (type 2) (param i32 i32) (result i32)
                local.get 0
                local.get 1
                call 0
                local.get 0
                i32.const 4
                i32.add
                call 1)
              (export "memory" (memory 0))
              (export "run" (func 2))
              (export "isolate" (func 3)))
            "#,
        );
        let lib = wasm(
            r#"
            (module
              (type (;0;) (func (param i32 i32)))
              (type (;1;) (func (param i32) (result i32)))
              (memory (;0;) 1)
              (func (;0;) (type 0) (param i32 i32)
                local.get 0
                local.get 1
                i32.store)
              (func (;1;) (type 1) (param i32) (result i32)
                local.get 0
                i32.load)
              (export "store_at" (func 0))
              (export "load_at" (func 1)))
            "#,
        );

        let linked = link(&main, &[("memlib", &lib)]).expect("Tier-B merge succeeds");
        inf_wasmparser::validate(&linked).expect("merged module is valid wasm");

        let (mut store, instance) = instantiate(&linked);
        let run: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "run")
            .expect("merged module exports `run`");

        for &(ptr, val) in &[(0_i32, 42_i32), (16, -7), (256, 1_000_000)] {
            assert_eq!(
                run.call(&mut store, (ptr, val)).expect("store-then-load executes"),
                val,
                "store_at then load_at over shared memory must round-trip the value"
            );
        }

        // Storing at `ptr` and loading `ptr+4` reads a slot `run` never wrote: the
        // store and load are wired to genuinely different addresses, not collapsed.
        let isolate: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "isolate")
            .expect("merged module exports `isolate`");
        assert_eq!(
            isolate.call(&mut store, (64, 99)).expect("isolate executes"),
            0,
            "loading an untouched neighbouring slot must read the memory's zero init"
        );

        // The store is observable in the module's *own* exported memory — direct
        // confirmation the merge folded both bodies onto the one shared memory.
        run.call(&mut store, (128, 0x1234)).expect("seed memory");
        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("merged module exports its shared memory");
        let mut slot = [0u8; 4];
        memory
            .read(&store, 128, &mut slot)
            .expect("read the written slot");
        assert_eq!(
            i32::from_le_bytes(slot),
            0x1234,
            "the merged store must be visible in the shared exported memory"
        );
    }

    #[test]
    fn interprocedural_tier_b_sort_pair_moves_the_right_bytes() {
        // The multi-function param-addressed case: `sort_pair(ptr)` reads the two
        // i32s at `[ptr]` and `[ptr+4]` and, when out of order, calls `swap(ptr)`
        // to exchange them. Every address is derived from the caller's `ptr`, so
        // the interprocedural provenance fixpoint admits it to Tier B. Correctness
        // here is byte movement through shared memory: a re-index that mis-wired
        // `sort_pair`'s `call` to `swap` (or swapped the load/store offsets) would
        // leave the pair unsorted or corrupt it. `run` writes a pair, sorts it, and
        // returns both slots packed so a single result pins the whole outcome.
        let main = wasm(
            r#"
            (module
              (type (;0;) (func (param i32)))
              (type (;1;) (func (param i32 i32 i32) (result i32)))
              (import "sortlib" "sort_pair" (func (;0;) (type 0)))
              (memory (;0;) 1 1)
              (func (;1;) (type 1) (param i32 i32 i32) (result i32)
                ;; write lo-candidate and hi-candidate at base and base+4
                local.get 0
                local.get 1
                i32.store
                local.get 0
                i32.const 4
                i32.add
                local.get 2
                i32.store
                ;; sort the pair in place
                local.get 0
                call 0
                ;; pack: low slot in the high half, high slot in the low half so the
                ;; ordering is visible in one i32 result (lo*1000 + hi).
                local.get 0
                i32.load
                i32.const 1000
                i32.mul
                local.get 0
                i32.const 4
                i32.add
                i32.load
                i32.add)
              (export "memory" (memory 0))
              (export "run" (func 1)))
            "#,
        );
        let lib = wasm(
            r#"
            (module
              (type (;0;) (func (param i32)))
              (memory (;0;) 1)
              ;; swap(ptr): exchange [ptr] and [ptr+4]
              (func (;0;) (type 0) (param i32)
                (local i32 i32)
                local.get 0
                i32.load
                local.set 1
                local.get 0
                i32.const 4
                i32.add
                i32.load
                local.set 2
                local.get 0
                local.get 2
                i32.store
                local.get 0
                i32.const 4
                i32.add
                local.get 1
                i32.store)
              ;; sort_pair(ptr): if [ptr] > [ptr+4], swap
              (func (;1;) (type 0) (param i32)
                local.get 0
                i32.load
                local.get 0
                i32.const 4
                i32.add
                i32.load
                i32.gt_s
                if
                  local.get 0
                  call 0
                end)
              (export "sort_pair" (func 1)))
            "#,
        );

        let linked = link(&main, &[("sortlib", &lib)]).expect("interprocedural Tier-B merge succeeds");
        inf_wasmparser::validate(&linked).expect("merged module is valid wasm");

        let (mut store, instance) = instantiate(&linked);
        let run: TypedFunc<(i32, i32, i32), i32> = instance
            .get_typed_func(&mut store, "run")
            .expect("merged module exports `run`");

        // Already sorted: stays (3, 9) -> 3*1000 + 9.
        assert_eq!(
            run.call(&mut store, (0, 3, 9)).expect("run executes"),
            3009,
            "an already-ordered pair must be left untouched"
        );
        // Out of order: (9, 3) must become (3, 9) -> 3009. A mis-wired call or a
        // bad offset would leave 9003 or some corrupt mix.
        assert_eq!(
            run.call(&mut store, (16, 9, 3)).expect("run executes"),
            3009,
            "an out-of-order pair must be sorted by the merged swap"
        );
        // Negative values exercise the signed comparison through the merge.
        assert_eq!(
            run.call(&mut store, (32, 5, -5)).expect("run executes"),
            -4995,
            "sorting must place -5 low and 5 high: -5*1000 + 5"
        );
    }
}
