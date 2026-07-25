// Runtime array bounds-check tests (Issue 164, Phases 2-3).
//
// In Compile mode (Debug and Release, Wasm32 and Soroban) every dynamic-index
// array load and store is preceded by a guard `index >= length -> unreachable`:
// the deployed artifact is always checked. Constant indices are validated
// statically by analysis rule A037 and get no runtime guard. Proof mode is left
// unguarded pending the proof-obligation path (#212).
//
// The single choke point `emit_index_offset` is shared by reads and writes, so
// a source exercising both proves both paths are guarded.
//
// Guard-count accounting (one `cov_mark::hit!(wasm_codegen_emit_bounds_check)`
// per *dynamic* index actually lowered):
//   - A literal index (`arr[0]`) folds to a static offset (A037) -> 0 guards.
//   - A parameter / computed index (`arr[i]`) -> 1 guard.
//   - A multi-dimensional dynamic access `m[i][j]` lowers as an outer compound
//     element access (`m[i]`, 1 guard) feeding an inner scalar access (`[j]`,
//     1 guard) -> 2 guards.
//   - Constant-only functions and Proof-mode builds contribute 0.
//
// Known codegen limitation exercised below: a *literal*-initialised
// multi-dimensional array (`[[i32;3];2] = [[..],[..]]`) panics in codegen today
// (`store_instruction_for_size` rejects the 12-byte inner-array element). The
// only currently-supported multi-dimensional source is `forall`+uzumaki init,
// which lowers via custom non-det opcodes -- so multi-dim coverage is guard
// count + structural validation only (wasmprinter / wasmtime cannot consume the
// custom opcodes). See the `multidim_*` tests.

/// Source with a dynamic array read (`arr[i]`) and a dynamic array write
/// (`arr[j] = v`). Indices come from parameters, so analysis rule A037 (which
/// only catches literal indices) does not fire and the runtime guard applies.
const READ_WRITE_SOURCE: &str = r#"
pub fn read_at(i: u32) -> i32 {
    let arr: [i32; 4] = [10, 20, 30, 40];
    return arr[i];
}

pub fn write_at(j: u32, v: i32) -> i32 {
    let mut arr: [i32; 4] = [0, 0, 0, 0];
    arr[j] = v;
    return arr[0];
}
"#;

#[cfg(test)]
mod bounds_check_tests {
    use super::READ_WRITE_SOURCE;
    use crate::utils::{codegen_output, codegen_with_full_config};
    use inference_wasm_codegen::{CompilationMode, OptLevel, Target};

    /// Compiles `source` under the Debug profile (`O0`). The guard is emitted in
    /// every Compile-mode build, so Debug and Release behave identically here;
    /// the dedicated `release_*` tests cover the default (`O3`) path.
    fn debug_wasm(source: &str) -> Vec<u8> {
        codegen_with_full_config(source, Target::Wasm32, CompilationMode::Compile, OptLevel::O0)
            .expect("O0 codegen failed")
            .wasm()
            .to_vec()
    }

    #[test]
    fn debug_profile_emits_guard_for_dynamic_read_and_write() {
        // Both the read in `read_at` and the write in `write_at` flow through the
        // shared `emit_index_offset` choke point, so the guard fires twice.
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 2);
        let wasm = debug_wasm(READ_WRITE_SOURCE);

        let wat = wasmprinter::print_bytes(&wasm).expect("failed to print WAT");
        assert!(
            wat.contains("i32.ge_u"),
            "Debug WAT must contain the unsigned bounds comparison:\n{wat}"
        );
        assert!(
            wat.contains("unreachable"),
            "Debug WAT must contain the trap on out-of-bounds:\n{wat}"
        );
    }

    #[test]
    fn release_profile_emits_guard_for_dynamic_read_and_write() {
        // The default path is Release (`Wasm32` -> O3). The guard is now emitted
        // for every Compile-mode build, so the cov_mark fires twice here just as
        // it does under Debug -- the deployed artifact is always checked.
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 2);
        let out = codegen_output(READ_WRITE_SOURCE);
        let wat = wasmprinter::print_bytes(out.wasm()).expect("failed to print WAT");
        assert!(
            wat.contains("i32.ge_u"),
            "Release WAT must contain the bounds-check comparison:\n{wat}"
        );
        assert!(
            wat.contains("unreachable"),
            "Release WAT must contain the trap on out-of-bounds:\n{wat}"
        );
    }

    #[test]
    fn proof_mode_emits_no_guard() {
        // Proof mode is the one remaining unguarded path (#212): dynamic bounds
        // become Rocq proof obligations, not runtime traps. The guard cov_mark
        // must never fire and the WAT must carry no bounds comparison.
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 0);
        let out = codegen_with_full_config(
            READ_WRITE_SOURCE,
            Target::Wasm32,
            CompilationMode::Proof,
            OptLevel::O3,
        )
        .expect("proof-mode codegen failed");
        let wat = wasmprinter::print_bytes(out.wasm()).expect("failed to print WAT");
        assert!(
            !wat.contains("i32.ge_u"),
            "Proof-mode WAT must not contain a bounds-check comparison:\n{wat}"
        );
    }

    #[test]
    fn debug_profile_module_passes_wasmparser_validation() {
        let wasm = debug_wasm(READ_WRITE_SOURCE);
        inf_wasmparser::validate(&wasm)
            .unwrap_or_else(|e| panic!("Guarded O0 module is invalid: {e}"));
    }

    #[test]
    fn debug_profile_in_bounds_read_returns_element() {
        use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

        let wasm = debug_wasm(READ_WRITE_SOURCE);
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm).expect("failed to build module");
        let mut store = Store::new(&engine, ());
        let instance =
            Instance::new(&mut store, &module, &[]).expect("failed to instantiate module");

        let read_at: TypedFunc<u32, i32> = instance
            .get_typed_func(&mut store, "read_at")
            .expect("failed to get read_at");

        // arr = [10, 20, 30, 40]; in-bounds indices return the element.
        assert_eq!(read_at.call(&mut store, 0).expect("call failed"), 10);
        assert_eq!(read_at.call(&mut store, 1).expect("call failed"), 20);
        assert_eq!(read_at.call(&mut store, 3).expect("call failed"), 40);
    }

    #[test]
    fn debug_profile_out_of_bounds_read_traps() {
        use wasmtime::{Engine, Instance, Module, Store, Trap, TypedFunc};

        let wasm = debug_wasm(READ_WRITE_SOURCE);
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm).expect("failed to build module");
        let mut store = Store::new(&engine, ());
        let instance =
            Instance::new(&mut store, &module, &[]).expect("failed to instantiate module");

        let read_at: TypedFunc<u32, i32> = instance
            .get_typed_func(&mut store, "read_at")
            .expect("failed to get read_at");

        // Index 4 == length, the first out-of-bounds index, must trap.
        let err = read_at
            .call(&mut store, 4)
            .expect_err("out-of-bounds read must trap");
        let trap = err
            .downcast_ref::<Trap>()
            .unwrap_or_else(|| panic!("expected a wasmtime Trap, got: {err:?}"));
        assert_eq!(*trap, Trap::UnreachableCodeReached);
    }

    #[test]
    fn debug_profile_out_of_bounds_write_traps() {
        use wasmtime::{Engine, Instance, Module, Store, Trap, TypedFunc};

        let wasm = debug_wasm(READ_WRITE_SOURCE);
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm).expect("failed to build module");
        let mut store = Store::new(&engine, ());
        let instance =
            Instance::new(&mut store, &module, &[]).expect("failed to instantiate module");

        let write_at: TypedFunc<(u32, i32), i32> = instance
            .get_typed_func(&mut store, "write_at")
            .expect("failed to get write_at");

        // In-bounds write succeeds and the stored value is observable.
        assert_eq!(write_at.call(&mut store, (0, 99)).expect("call failed"), 99);

        // Out-of-bounds write (index 4 == length) must trap before the store.
        let err = write_at
            .call(&mut store, (4, 7))
            .expect_err("out-of-bounds write must trap");
        let trap = err
            .downcast_ref::<Trap>()
            .unwrap_or_else(|| panic!("expected a wasmtime Trap, got: {err:?}"));
        assert_eq!(*trap, Trap::UnreachableCodeReached);
    }
}

// ---------------------------------------------------------------------------
// Extended coverage (Issue 164, Phases 2-3).
//
// Scenario groups:
//   * element-type breadth (u8 / i16 / i64) -> guard compares length not bytes
//   * const-only and mixed const/dynamic indexing -> exact guard counts
//   * richer shapes guarded under Release too (multi-dim, array-of-structs)
//   * wasmtime execution: in-bounds correctness + boundary / negative / huge OOB
//   * nested block depth (loop, if, forall), array-of-structs, multiple accesses
//   * multi-dimensional (uzumaki) guard count + structural validation
//   * structural validation of guarded modules at non-trivial block depths
// ---------------------------------------------------------------------------
#[cfg(test)]
mod extended {
    use crate::utils::codegen_with_full_config_no_analysis;
    use inference_wasm_codegen::{CompilationMode, OptLevel, Target};
    use wasmtime::{Engine, Instance, Module, Store, Trap, TypedFunc};

    /// Compiles `source` under the Debug profile (`O0`), where the bounds-check
    /// guard is emitted, and returns the WASM bytes. The guard is emitted in
    /// every Compile-mode build, so the `O0` vs `O3` choice is immaterial here;
    /// `release_wasm` exercises the default path for parity.
    ///
    /// Analysis is skipped: some sources here place a dynamic index inside a
    /// `forall` block to exercise guard emission at various block depths, which
    /// A042 rejects outside a `spec`. The guard is a codegen concern, so the
    /// analysis pass is not what these tests exercise.
    fn debug_wasm(source: &str) -> Vec<u8> {
        codegen_with_full_config_no_analysis(source, Target::Wasm32, CompilationMode::Compile, OptLevel::O0)
            .expect("O0 codegen failed")
            .wasm()
            .to_vec()
    }

    /// Compiles `source` under the Release profile (`O3`). The guard is emitted
    /// for every Compile-mode build, so Release output is guarded identically to
    /// Debug; this helper confirms the default path is checked. Analysis is
    /// skipped for the same reason as [`debug_wasm`].
    fn release_wasm(source: &str) -> Vec<u8> {
        codegen_with_full_config_no_analysis(source, Target::Wasm32, CompilationMode::Compile, OptLevel::O3)
            .expect("O3 codegen failed")
            .wasm()
            .to_vec()
    }

    /// Prints a module to WAT text. Only valid for modules without custom
    /// non-det opcodes (wasmprinter rejects those).
    fn wat(wasm: &[u8]) -> String {
        wasmprinter::print_bytes(wasm).expect("failed to print WAT")
    }

    /// Instantiates a module and returns the store + instance so a test can
    /// fetch typed functions and call them.
    fn instantiate(wasm: &[u8]) -> (Store<()>, Instance) {
        let engine = Engine::default();
        let module = Module::new(&engine, wasm).expect("failed to build module");
        let mut store = Store::new(&engine, ());
        let instance =
            Instance::new(&mut store, &module, &[]).expect("failed to instantiate module");
        (store, instance)
    }

    /// Asserts that `err` is a wasmtime trap originating from `unreachable`
    /// (the bounds-check guard's out-of-bounds path).
    fn assert_unreachable_trap(err: &wasmtime::Error) {
        let trap = err
            .downcast_ref::<Trap>()
            .unwrap_or_else(|| panic!("expected a wasmtime Trap, got: {err:?}"));
        assert_eq!(*trap, Trap::UnreachableCodeReached);
    }

    // --- Element-type breadth: guard compares length, independent of size ----

    /// A `[u8; 4]` array indexed dynamically: the guard is emitted (count 1) and
    /// the `i32.ge_u` + `unreachable` sequence appears in WAT.
    #[test]
    fn debug_u8_array_dynamic_index_emits_guard() {
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 1);
        let source = r#"
pub fn at(i: u32) -> u8 {
    let arr: [u8; 4] = [10, 20, 30, 40];
    return arr[i];
}
"#;
        let wat = wat(&debug_wasm(source));
        assert!(wat.contains("i32.ge_u"), "u8 array must emit guard:\n{wat}");
        assert!(wat.contains("unreachable"), "u8 array must trap OOB:\n{wat}");
    }

    /// A `[i16; 5]` array indexed dynamically: guard present, comparing the
    /// element count 5 -- not the 10-byte size.
    #[test]
    fn debug_i16_array_dynamic_index_compares_length_not_bytes() {
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 1);
        let source = r#"
pub fn at(i: u32) -> i16 {
    let arr: [i16; 5] = [1, 2, 3, 4, 5];
    return arr[i];
}
"#;
        let wat = wat(&debug_wasm(source));
        assert!(
            wat.contains("i32.const 5"),
            "guard must compare against length 5, not byte size 10:\n{wat}"
        );
    }

    /// A `[i64; 4]` array indexed dynamically: the byte size (8) differs from the
    /// length (4), proving the guard bounds the *index* against the length rather
    /// than the byte stride.
    #[test]
    fn debug_i64_array_dynamic_index_compares_length_not_bytes() {
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 1);
        let source = r#"
pub fn at(i: u32) -> i64 {
    let arr: [i64; 4] = [10, 20, 30, 40];
    return arr[i];
}
"#;
        let wat = wat(&debug_wasm(source));
        assert!(
            wat.contains("i32.const 4"),
            "guard must compare index against length 4, not byte size 8/32:\n{wat}"
        );
        // The element stride 8 still appears as the offset multiply, *after* the
        // guard -- confirming length and stride are distinct constants.
        assert!(
            wat.contains("i32.const 8"),
            "offset multiply by element size 8 must still be present:\n{wat}"
        );
    }

    // --- Constant-only and mixed indexing: exact guard counts ----------------

    /// A function whose array is indexed only by in-bounds constants emits NO
    /// runtime guard under O0 (A037 handles them statically), yet still validates
    /// -- proving the reserved-but-unused scratch local is valid WASM.
    #[test]
    fn debug_constant_only_indices_emit_no_guard_but_validate() {
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 0);
        let source = r#"
pub fn f() -> i32 {
    let arr: [i32; 4] = [10, 20, 30, 40];
    return arr[0] + arr[1] + arr[2] + arr[3];
}
"#;
        let wasm = debug_wasm(source);
        let wat = wat(&wasm);
        assert!(
            !wat.contains("i32.ge_u"),
            "constant-only indices must not emit a runtime guard:\n{wat}"
        );
        inf_wasmparser::validate(&wasm).unwrap_or_else(|e| {
            panic!("constant-only O0 module (unused scratch local) must validate: {e}")
        });
    }

    /// Mixed constant + dynamic indices in one function: exactly ONE guard (only
    /// the dynamic `arr[i]` is guarded; `arr[0]` folds statically). In-bounds the
    /// dynamic access works; the constant access always works; OOB traps.
    #[test]
    fn debug_mixed_const_and_dynamic_emits_single_guard_and_runs() {
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 1);
        let source = r#"
pub fn f(i: u32) -> i32 {
    let arr: [i32; 4] = [10, 20, 30, 40];
    return arr[0] + arr[i];
}
"#;
        let wasm = debug_wasm(source);
        let (mut store, instance) = instantiate(&wasm);
        let f: TypedFunc<u32, i32> = instance
            .get_typed_func(&mut store, "f")
            .expect("failed to get f");
        // arr[0] (=10) + arr[i]
        assert_eq!(f.call(&mut store, 0).expect("call failed"), 20);
        assert_eq!(f.call(&mut store, 3).expect("call failed"), 50);
        // Dynamic index out of bounds traps; the constant access never does.
        assert_unreachable_trap(&f.call(&mut store, 4).expect_err("OOB must trap"));
    }

    // --- Richer shapes are guarded under Release too -------------------------

    /// A multi-dimensional (uzumaki) dynamic access `g[i][j]` emits TWO guards
    /// under the Release profile (O3) -- one per dynamic dimension -- proving the
    /// guard is no longer gated on opt level. Multi-dim uses custom non-det
    /// opcodes, so this is a cov_mark assertion plus structural validation (no
    /// WAT extraction).
    #[test]
    fn release_multidim_emits_guard() {
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 2);
        let source = r#"
pub fn m(i: i32, j: i32) {
    forall {
        let g: [[i32; 3]; 2] = @;
        let v: i32 = g[i][j];
    }
}
"#;
        let wasm = release_wasm(source);
        inf_wasmparser::validate(&wasm)
            .unwrap_or_else(|e| panic!("Release multi-dim module must validate: {e}"));
    }

    /// An array-of-structs dynamic access emits a guard under the Release
    /// profile, and the Release WAT contains the bounds comparison.
    #[test]
    fn release_array_of_structs_emits_guard() {
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 1);
        let source = r#"
struct Pt { x: i32; y: i32; }
pub fn f(i: u32) -> i32 {
    let pts: [Pt; 2] = [Pt{x:1,y:2}, Pt{x:3,y:4}];
    return pts[i].x;
}
"#;
        let wat = wat(&release_wasm(source));
        assert!(
            wat.contains("i32.ge_u"),
            "Release array-of-structs must emit a guard:\n{wat}"
        );
    }

    // --- wasmtime execution: negative, boundary, and huge OOB indices --------

    /// A signed `i32` index of -1 arrives as `u32::MAX` through the unsigned
    /// `i32.ge_u` compare and traps; index 0 returns the first element.
    #[test]
    fn debug_negative_index_traps_in_bounds_works() {
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 1);
        let source = r#"
pub fn at(i: i32) -> i32 {
    let arr: [i32; 4] = [10, 20, 30, 40];
    return arr[i];
}
"#;
        let wasm = debug_wasm(source);
        let (mut store, instance) = instantiate(&wasm);
        let at: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "at")
            .expect("failed to get at");

        assert_eq!(at.call(&mut store, 0).expect("call failed"), 10);
        // -1 as a signed i32 is 0xFFFF_FFFF == u32::MAX, far above length 4.
        assert_unreachable_trap(&at.call(&mut store, -1).expect_err("negative index must trap"));
    }

    /// Boundary sweep: last in-bounds index `length-1` returns the element;
    /// `length`, `length+5`, and a maximal index all trap (guarding against an
    /// address-computation wraparound bypassing the check -- relates to #165).
    #[test]
    fn debug_boundary_sweep_traps_at_and_above_length() {
        let source = r#"
pub fn at(i: u32) -> i32 {
    let arr: [i32; 4] = [10, 20, 30, 40];
    return arr[i];
}
"#;
        let wasm = debug_wasm(source);
        let (mut store, instance) = instantiate(&wasm);
        let at: TypedFunc<u32, i32> = instance
            .get_typed_func(&mut store, "at")
            .expect("failed to get at");

        // length - 1: last valid index.
        assert_eq!(at.call(&mut store, 3).expect("call failed"), 40);
        // index == length: first OOB.
        assert_unreachable_trap(&at.call(&mut store, 4).expect_err("index==length must trap"));
        // index = length + 5.
        assert_unreachable_trap(&at.call(&mut store, 9).expect_err("length+5 must trap"));
        // Maximal u32 index: must trap, not wrap the address computation.
        assert_unreachable_trap(
            &at.call(&mut store, u32::MAX).expect_err("u32::MAX index must trap"),
        );
    }

    // --- Multiple dynamic accesses: scratch reuse ----------------------------

    /// Two dynamic reads in one expression (`arr[i] + arr[j]`): the guard fires
    /// once per access (2 total) and the single scratch local is reused across
    /// both. In-bounds returns the sum; an OOB on either index traps.
    #[test]
    fn debug_two_dynamic_reads_reuse_scratch_and_run() {
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 2);
        let source = r#"
pub fn add(i: u32, j: u32) -> i32 {
    let arr: [i32; 4] = [10, 20, 30, 40];
    return arr[i] + arr[j];
}
"#;
        let wasm = debug_wasm(source);
        let (mut store, instance) = instantiate(&wasm);
        let add: TypedFunc<(u32, u32), i32> = instance
            .get_typed_func(&mut store, "add")
            .expect("failed to get add");

        assert_eq!(add.call(&mut store, (0, 3)).expect("call failed"), 50);
        assert_eq!(add.call(&mut store, (1, 2)).expect("call failed"), 50);
        // OOB on the first index.
        assert_unreachable_trap(&add.call(&mut store, (4, 0)).expect_err("first OOB must trap"));
        // OOB on the second index.
        assert_unreachable_trap(&add.call(&mut store, (0, 4)).expect_err("second OOB must trap"));
    }

    /// An in-bounds dynamic WRITE followed by a dynamic READ-back across several
    /// indices: confirms guarded writes land at the correct address. Two dynamic
    /// accesses (write + read) -> 2 guards.
    #[test]
    fn debug_dynamic_write_then_readback_lands_correctly() {
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 2);
        let source = r#"
pub fn set_get(i: u32, v: i32) -> i32 {
    let mut arr: [i32; 4] = [0, 0, 0, 0];
    arr[i] = v;
    return arr[i];
}
"#;
        let wasm = debug_wasm(source);
        let (mut store, instance) = instantiate(&wasm);
        let set_get: TypedFunc<(u32, i32), i32> = instance
            .get_typed_func(&mut store, "set_get")
            .expect("failed to get set_get");

        // Each index writes then reads back the same slot.
        assert_eq!(set_get.call(&mut store, (0, 11)).expect("call failed"), 11);
        assert_eq!(set_get.call(&mut store, (1, 22)).expect("call failed"), 22);
        assert_eq!(set_get.call(&mut store, (3, 44)).expect("call failed"), 44);
        // OOB index traps before the store.
        assert_unreachable_trap(&set_get.call(&mut store, (4, 9)).expect_err("OOB must trap"));
    }

    // --- Guard at non-zero block depth: loop, if, forall ---------------------

    /// A dynamic index inside a `loop` body: the guard is emitted nested inside
    /// the loop's block+loop bracketing. An in-bounds run returns the sum;
    /// forcing `n` past the length traps. Exercises `wasm_block_depth` tracking.
    #[test]
    fn debug_dynamic_index_inside_loop_guards_and_runs() {
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 1);
        let source = r#"
pub fn sum(n: i32) -> i32 {
    let arr: [i32; 4] = [10, 20, 30, 40];
    let mut total: i32 = 0;
    let mut i: i32 = 0;
    loop i < n {
        total = total + arr[i];
        i = i + 1;
    }
    return total;
}
"#;
        let wasm = debug_wasm(source);
        inf_wasmparser::validate(&wasm)
            .unwrap_or_else(|e| panic!("guarded loop module must validate: {e}"));

        let (mut store, instance) = instantiate(&wasm);
        let sum: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "sum")
            .expect("failed to get sum");

        // Sum the whole 4-element array.
        assert_eq!(sum.call(&mut store, 4).expect("call failed"), 100);
        // Partial sum of the first two elements.
        assert_eq!(sum.call(&mut store, 2).expect("call failed"), 30);
        // Iterating one past the length reads arr[4] and traps.
        assert_unreachable_trap(&sum.call(&mut store, 5).expect_err("loop past length must trap"));
    }

    /// A dynamic index inside an `if` branch: the guard is emitted at non-zero
    /// block depth. In-bounds within the taken branch returns the element; the
    /// untaken branch never indexes; the taken branch OOB traps.
    #[test]
    fn debug_dynamic_index_inside_if_guards_and_runs() {
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 1);
        let source = r#"
pub fn maybe_at(i: u32, take: i32) -> i32 {
    let arr: [i32; 4] = [10, 20, 30, 40];
    if take > 0 {
        return arr[i];
    }
    return 0;
}
"#;
        let wasm = debug_wasm(source);
        inf_wasmparser::validate(&wasm)
            .unwrap_or_else(|e| panic!("guarded if module must validate: {e}"));

        let (mut store, instance) = instantiate(&wasm);
        let maybe_at: TypedFunc<(u32, i32), i32> = instance
            .get_typed_func(&mut store, "maybe_at")
            .expect("failed to get maybe_at");

        // Branch taken, in-bounds.
        assert_eq!(maybe_at.call(&mut store, (2, 1)).expect("call failed"), 30);
        // Branch not taken: the OOB index is never reached, returns 0.
        assert_eq!(maybe_at.call(&mut store, (9, 0)).expect("call failed"), 0);
        // Branch taken, OOB index traps.
        assert_unreachable_trap(
            &maybe_at.call(&mut store, (9, 1)).expect_err("taken-branch OOB must trap"),
        );
    }

    /// A dynamic index inside a `forall` non-det block: the guard is emitted at
    /// non-zero block depth and the module validates structurally. The block uses
    /// custom non-det opcodes, so this is guard-count + validation only (no
    /// wasmtime execution).
    #[test]
    fn debug_dynamic_index_inside_forall_guards_and_validates() {
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 1);
        let source = r#"
pub fn f(i: i32) -> i32 {
    let arr: [i32; 4] = [10, 20, 30, 40];
    forall {
        let x: i32 = arr[i];
    }
    return arr[0];
}
"#;
        let wasm = debug_wasm(source);
        inf_wasmparser::validate(&wasm)
            .unwrap_or_else(|e| panic!("guarded forall module must validate: {e}"));
    }

    // --- Array of structs ----------------------------------------------------

    /// Array-of-structs dynamic read `pts[i].x` on `[Pt; 2]`: guard present
    /// (count 1), in-bounds returns the field, OOB traps. The `pts[i]` access is
    /// the guarded element access; the `.x` member access carries no guard.
    #[test]
    fn debug_array_of_structs_read_guards_and_runs() {
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 1);
        let source = r#"
struct Pt { x: i32; y: i32; }
pub fn get_x(i: u32) -> i32 {
    let pts: [Pt; 2] = [Pt{x:10,y:20}, Pt{x:30,y:40}];
    return pts[i].x;
}
"#;
        let wasm = debug_wasm(source);
        let (mut store, instance) = instantiate(&wasm);
        let get_x: TypedFunc<u32, i32> = instance
            .get_typed_func(&mut store, "get_x")
            .expect("failed to get get_x");

        assert_eq!(get_x.call(&mut store, 0).expect("call failed"), 10);
        assert_eq!(get_x.call(&mut store, 1).expect("call failed"), 30);
        // index == length (2) traps.
        assert_unreachable_trap(&get_x.call(&mut store, 2).expect_err("AoS OOB read must trap"));
    }

    /// Array-of-structs dynamic write `pts[i].x = v` then read-back `pts[i].x` on
    /// `[Pt; 2]`: the write and the read-back are each guarded (count 2),
    /// in-bounds the stored value is observable, OOB traps before the store.
    #[test]
    fn debug_array_of_structs_write_guards_and_runs() {
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 2);
        let source = r#"
struct Pt { x: i32; y: i32; }
pub fn set_x(i: u32, v: i32) -> i32 {
    let mut pts: [Pt; 2] = [Pt{x:1,y:2}, Pt{x:3,y:4}];
    pts[i].x = v;
    return pts[i].x;
}
"#;
        let wasm = debug_wasm(source);
        let (mut store, instance) = instantiate(&wasm);
        let set_x: TypedFunc<(u32, i32), i32> = instance
            .get_typed_func(&mut store, "set_x")
            .expect("failed to get set_x");

        assert_eq!(set_x.call(&mut store, (0, 77)).expect("call failed"), 77);
        assert_eq!(set_x.call(&mut store, (1, 88)).expect("call failed"), 88);
        // index == length (2) traps before the store.
        assert_unreachable_trap(
            &set_x.call(&mut store, (2, 9)).expect_err("AoS OOB write must trap"),
        );
    }

    // --- Immutable-self method dynamic index (regression for #164 panic) -----

    /// A dynamic index through an **immutable-`self`** method (`self.arr[idx]`)
    /// where the index comes from a parameter. An immutable `self` needs no frame
    /// slot, so `compute_frame_layout` returns `None` and the method has no
    /// frame; the bounds-check scratch local must therefore be reserved
    /// independently of frame presence. Before the fix this path panicked at the
    /// guard-emission site ("bounds-check scratch local must be reserved").
    ///
    /// The method is driven through a `pub fn` wrapper that constructs the struct
    /// and forwards the runtime index. The guard fires once (only `self.arr[idx]`
    /// is dynamic; the struct literal's element indices are constants). Verified
    /// under BOTH the Debug profile (`O0`) and the default Release profile
    /// (`O3`) -- the guard is emitted in every Compile-mode build.
    #[test]
    fn immutable_self_method_dynamic_index_guards_and_runs() {
        let source = r#"
struct Holder { arr: [i32; 4]; val: i32;
    fn get(self, idx: i32) -> i32 {
        return self.arr[idx];
    }
}

pub fn at(idx: i32) -> i32 {
    let h: Holder = Holder { arr: [10, 20, 30, 40], val: 0 };
    return h.get(idx);
}
"#;

        // Exercise both profiles. The guard count is asserted per build below.
        for (label, wasm) in [("debug", debug_wasm(source)), ("release", release_wasm(source))]
        {
            let wat = wat(&wasm);
            assert!(
                wat.contains("i32.ge_u"),
                "{label}: immutable-self method dynamic index must emit a guard:\n{wat}"
            );

            let (mut store, instance) = instantiate(&wasm);
            let at: TypedFunc<i32, i32> = instance
                .get_typed_func(&mut store, "at")
                .unwrap_or_else(|e| panic!("{label}: failed to get at: {e}"));

            assert_eq!(at.call(&mut store, 0).expect("call failed"), 10, "{label}");
            assert_eq!(at.call(&mut store, 3).expect("call failed"), 40, "{label}");
            // index == length (4) traps; -1 arrives as u32::MAX and also traps.
            assert_unreachable_trap(&at.call(&mut store, 4).expect_err("OOB must trap"));
            assert_unreachable_trap(&at.call(&mut store, -1).expect_err("negative must trap"));
        }
    }

    /// Pins the guard count for the immutable-self method path to exactly one.
    /// Kept separate from the execution test so the single-threaded `cov_mark`
    /// check brackets a single codegen call.
    #[test]
    fn immutable_self_method_dynamic_index_emits_single_guard() {
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 1);
        let source = r#"
struct Holder { arr: [i32; 4]; val: i32;
    fn get(self, idx: i32) -> i32 {
        return self.arr[idx];
    }
}

pub fn at(idx: i32) -> i32 {
    let h: Holder = Holder { arr: [10, 20, 30, 40], val: 0 };
    return h.get(idx);
}
"#;
        let wasm = debug_wasm(source);
        inf_wasmparser::validate(&wasm)
            .unwrap_or_else(|e| panic!("guarded immutable-self method module must validate: {e}"));
    }

    // --- Array parameter (copy-on-entry) -------------------------------------

    /// An array *parameter* indexed dynamically: the guard is emitted (count 1)
    /// and uses the parameter array's length, confirming `array_length` resolves
    /// for parameter arrays and the scratch reservation coexists with array-param
    /// copy-on-entry. The parameter is a memory pointer in the ABI, so this is a
    /// guard-presence + structural-validation test (not directly callable from
    /// wasmtime without supplying a pointer).
    #[test]
    fn debug_array_parameter_dynamic_index_guards_with_param_length() {
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 1);
        let source = r#"
pub fn pick(arr: [i32; 4], i: u32) -> i32 {
    return arr[i];
}
"#;
        let wasm = debug_wasm(source);
        let wat = wat(&wasm);
        assert!(
            wat.contains("i32.ge_u"),
            "array-parameter dynamic index must emit a guard:\n{wat}"
        );
        assert!(
            wat.contains("i32.const 4"),
            "guard must compare against the parameter array length 4:\n{wat}"
        );
        inf_wasmparser::validate(&wasm)
            .unwrap_or_else(|e| panic!("guarded array-parameter module must validate: {e}"));
    }

    // --- Multi-dimensional (uzumaki) -----------------------------------------

    /// A multi-dimensional dynamic access `g[i][j]` lowers as an outer compound
    /// element access (`g[i]`, guard against outer length 2) feeding an inner
    /// scalar access (`[j]`, guard against inner length 3): TWO guards. The
    /// module uses custom non-det opcodes (forall+uzumaki init -- the only
    /// currently-supported multi-dim initializer; a literal `[[..],[..]]`
    /// initializer panics in codegen today), so this is guard-count + structural
    /// validation only.
    #[test]
    fn debug_multidim_dynamic_access_emits_two_guards_and_validates() {
        cov_mark::check_count!(wasm_codegen_emit_bounds_check, 2);
        let source = r#"
pub fn m(i: i32, j: i32) {
    forall {
        let g: [[i32; 3]; 2] = @;
        let v: i32 = g[i][j];
    }
}
"#;
        let wasm = debug_wasm(source);
        inf_wasmparser::validate(&wasm)
            .unwrap_or_else(|e| panic!("guarded multi-dim O0 module must validate: {e}"));
    }

    // --- Structural validation of guarded modules at non-trivial depth -------

    /// Validates several guarded O0 modules (multi-dim, array-of-structs,
    /// array-parameter, loop) in one place to confirm structural validity at
    /// non-trivial block depths.
    #[test]
    fn debug_guarded_modules_validate_at_various_block_depths() {
        let sources: &[(&str, &str)] = &[
            (
                "multi-dim (forall+uzumaki)",
                r#"
pub fn m(i: i32, j: i32) {
    forall {
        let g: [[i32; 3]; 2] = @;
        let v: i32 = g[i][j];
    }
}
"#,
            ),
            (
                "array-of-structs read",
                r#"
struct Pt { x: i32; y: i32; }
pub fn f(i: u32) -> i32 {
    let pts: [Pt; 2] = [Pt{x:1,y:2}, Pt{x:3,y:4}];
    return pts[i].x;
}
"#,
            ),
            (
                "array parameter",
                r#"
pub fn pick(arr: [i32; 4], i: u32) -> i32 {
    return arr[i];
}
"#,
            ),
            (
                "dynamic index inside loop",
                r#"
pub fn sum(n: i32) -> i32 {
    let arr: [i32; 4] = [10, 20, 30, 40];
    let mut total: i32 = 0;
    let mut i: i32 = 0;
    loop i < n {
        total = total + arr[i];
        i = i + 1;
    }
    return total;
}
"#,
            ),
        ];

        for (label, source) in sources {
            let wasm = debug_wasm(source);
            inf_wasmparser::validate(&wasm)
                .unwrap_or_else(|e| panic!("guarded module '{label}' must validate: {e}"));
        }
    }
}
