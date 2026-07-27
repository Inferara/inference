// Narrow (i8/i16) signed division overflow guard tests.
//
// A narrow signed division computes in the promoted i32 width, where the one
// overflowing pair (MIN, -1) yields a quotient (+128 / +32768) that is
// representable in i32 — so wasm's own div_s trap never fires — and the
// mandatory re-narrowing would silently sign-wrap it back to MIN. The compiler
// guards the promoted quotient (`local.tee; i32.const 128|32768; i32.eq; if;
// unreachable; end`) after div_s and before narrowing, so division overflow
// traps at every signed width.
//
// Trap-code asymmetry, asserted deliberately below:
//   - narrow overflow guard   -> Trap::UnreachableCodeReached
//   - full-width (i32/i64)     -> Trap::IntegerOverflow (wasm-native)
//   - divide by zero (any)     -> Trap::IntegerDivisionByZero (wasm-native)
// The trap-or-not contract is width-uniform; the trap *code* is not.

#[cfg(test)]
mod div_overflow_tests {
    use crate::utils::wasm_codegen;
    use wasmtime::{Engine, Instance, Module, Store, Trap, TypedFunc};

    fn instantiate(source: &str) -> (Store<()>, Instance) {
        let wasm = wasm_codegen(source);
        inf_wasmparser::validate(&wasm)
            .unwrap_or_else(|e| panic!("generated module is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm).unwrap_or_else(|e| panic!("module build: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("instantiate: {e}"));
        (store, instance)
    }

    /// Full-width (i32/i64) signed division: overflow traps as wasm's *native*
    /// integer-overflow trap, a different trap code than the narrow guard's
    /// `unreachable` — the trap-or-not contract, not the trap code, is uniform.
    /// This is the first test to assert the native full-width overflow trap.
    #[test]
    fn full_width_division_overflow_traps_natively() {
        const SOURCE: &str = r#"
pub fn div32(a: i32, b: i32) -> i32 { return a / b; }
pub fn div64(a: i64, b: i64) -> i64 { return a / b; }
"#;
        let (mut store, instance) = instantiate(SOURCE);

        let div32: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "div32")
            .expect("get div32");
        let err = div32
            .call(&mut store, (i32::MIN, -1))
            .expect_err("div32(MIN, -1) must trap");
        assert_eq!(
            *err.downcast_ref::<Trap>().expect("wasmtime Trap"),
            Trap::IntegerOverflow,
            "full-width i32 MIN/-1 is wasm's native integer-overflow trap"
        );
        // A non-overflowing full-width division returns normally.
        assert_eq!(div32.call(&mut store, (i32::MIN, 1)).expect("div32 ok"), i32::MIN);

        let div64: TypedFunc<(i64, i64), i64> = instance
            .get_typed_func(&mut store, "div64")
            .expect("get div64");
        let err = div64
            .call(&mut store, (i64::MIN, -1))
            .expect_err("div64(MIN, -1) must trap");
        assert_eq!(
            *err.downcast_ref::<Trap>().expect("wasmtime Trap"),
            Trap::IntegerOverflow,
            "full-width i64 MIN/-1 is wasm's native integer-overflow trap"
        );
        assert_eq!(div64.call(&mut store, (i64::MIN, 1)).expect("div64 ok"), i64::MIN);
    }

    /// A narrow signed division in a `const` initializer lowers through the same
    /// `lower_named_binding_init` path as a `let`, so the guard must fire there.
    /// Without the predicate's ConstDef descent, the reservation is missed and
    /// codegen panics on the guard's scratch `.expect` — this test would then
    /// fail to compile the module at all.
    #[test]
    fn const_initializer_division_is_guarded() {
        const SOURCE: &str = r#"
pub fn constdiv(a: i8, b: i8) -> i8 {
    const Q: i8 = a / b;
    return Q;
}
"#;
        let (mut store, instance) = instantiate(SOURCE);
        let constdiv: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "constdiv")
            .expect("get constdiv");

        let err = constdiv
            .call(&mut store, (-128, -1))
            .expect_err("constdiv(-128, -1) must trap");
        assert_eq!(
            *err.downcast_ref::<Trap>().expect("wasmtime Trap"),
            Trap::UnreachableCodeReached,
        );
        assert_eq!(constdiv.call(&mut store, (-128, 1)).expect("constdiv ok"), -128);
    }

    /// The guard is emitted correctly when it sits inside a surrounding loop —
    /// the `wasm_block_depth += 1 / -= 1` bookkeeping around the guard's own
    /// `if`/`end` must compose with the enclosing loop's depth.
    #[test]
    fn division_guard_inside_loop() {
        const SOURCE: &str = r#"
pub fn loopdiv(a: i8, b: i8, n: i32) -> i8 {
    let mut acc: i8 = 0;
    let mut i: i32 = 0;
    loop i < n {
        acc = a / b;
        i = i + 1;
    }
    return acc;
}
"#;
        let (mut store, instance) = instantiate(SOURCE);
        let loopdiv: TypedFunc<(i32, i32, i32), i32> = instance
            .get_typed_func(&mut store, "loopdiv")
            .expect("get loopdiv");

        // Benign operands: one iteration computes 10 / 2 = 5.
        assert_eq!(loopdiv.call(&mut store, (10, 2, 1)).expect("loopdiv ok"), 5);
        // Zero iterations: the loop body (and its guard) never runs.
        assert_eq!(loopdiv.call(&mut store, (-128, -1, 0)).expect("loopdiv ok"), 0);
        // Overflowing operands reach the guard on the first iteration.
        let err = loopdiv
            .call(&mut store, (-128, -1, 1))
            .expect_err("loopdiv(-128, -1, 1) must trap");
        assert_eq!(
            *err.downcast_ref::<Trap>().expect("wasmtime Trap"),
            Trap::UnreachableCodeReached,
        );
    }

    /// A single function containing BOTH a dynamic array index (bounds guard)
    /// and a narrow signed division (overflow guard). Each guard owns a distinct
    /// scratch local; both fire independently. If they shared a scratch, one
    /// would clobber the other and these executions would misbehave.
    #[test]
    fn bounds_and_division_guards_coexist() {
        const SOURCE: &str = r#"
pub fn coexist(i: u32, a: i8, b: i8) -> i8 {
    let arr: [i8; 4] = [10, 20, 30, 40];
    let x: i8 = arr[i];
    let y: i8 = a / b;
    return x + y;
}
"#;
        let (mut store, instance) = instantiate(SOURCE);
        let coexist: TypedFunc<(u32, i32, i32), i32> = instance
            .get_typed_func(&mut store, "coexist")
            .expect("get coexist");

        // In-bounds index + benign division: arr[0]=10, 10/2=5 -> 15.
        assert_eq!(coexist.call(&mut store, (0, 10, 2)).expect("coexist ok"), 15);
        // In-bounds index, overflowing division -> division guard traps.
        let err = coexist
            .call(&mut store, (0, -128, -1))
            .expect_err("coexist division overflow must trap");
        assert_eq!(
            *err.downcast_ref::<Trap>().expect("wasmtime Trap"),
            Trap::UnreachableCodeReached,
        );
        // Out-of-bounds index -> bounds guard traps (before the division runs).
        let err = coexist
            .call(&mut store, (4, 10, 2))
            .expect_err("coexist out-of-bounds must trap");
        assert_eq!(
            *err.downcast_ref::<Trap>().expect("wasmtime Trap"),
            Trap::UnreachableCodeReached,
        );
    }

    /// The guard is per division *site*, not export-gated: a private helper's
    /// narrow division is guarded even though the helper is never exported, so a
    /// call through an exported wrapper still traps on overflow.
    #[test]
    fn non_exported_helper_division_is_guarded() {
        const SOURCE: &str = r#"
fn helper(a: i8, b: i8) -> i8 { return a / b; }
pub fn wrapper(a: i8, b: i8) -> i8 { return helper(a, b); }
"#;
        let (mut store, instance) = instantiate(SOURCE);
        let wrapper: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "wrapper")
            .expect("get wrapper");

        let err = wrapper
            .call(&mut store, (-128, -1))
            .expect_err("wrapper(-128, -1) must trap in the private helper");
        assert_eq!(
            *err.downcast_ref::<Trap>().expect("wasmtime Trap"),
            Trap::UnreachableCodeReached,
        );
        assert_eq!(wrapper.call(&mut store, (-100, 4)).expect("wrapper ok"), -25);
    }

    /// Remainder is intentionally NOT guarded: `MIN % -1 == 0` is the correct
    /// remainder at every width and never out of range, so it does not trap.
    /// A zero divisor still trips wasm's native remainder-by-zero trap.
    #[test]
    fn remainder_min_neg_one_is_zero_not_a_trap() {
        const SOURCE: &str = r#"
pub fn rem8(a: i8, b: i8) -> i8 { return a % b; }
pub fn rem16(a: i16, b: i16) -> i16 { return a % b; }
"#;
        let (mut store, instance) = instantiate(SOURCE);

        let rem8: TypedFunc<(i32, i32), i32> =
            instance.get_typed_func(&mut store, "rem8").expect("get rem8");
        assert_eq!(rem8.call(&mut store, (-128, -1)).expect("rem8 ok"), 0);

        let rem16: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "rem16")
            .expect("get rem16");
        assert_eq!(rem16.call(&mut store, (-32768, -1)).expect("rem16 ok"), 0);

        // Zero divisor still traps natively (remainder by zero).
        let err = rem8
            .call(&mut store, (1, 0))
            .expect_err("rem8(1, 0) must trap");
        assert_eq!(
            *err.downcast_ref::<Trap>().expect("wasmtime Trap"),
            Trap::IntegerDivisionByZero,
        );
    }
}
