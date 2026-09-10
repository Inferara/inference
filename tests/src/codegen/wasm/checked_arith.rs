// The overflow-guard catalogue, one exported function per row.
//
// Four tiers over one fixture: the module's bytes, its WAT, the fork's own
// validator at its default features (every guard opcode is core WebAssembly, so
// no partition of the golden walkers is needed and none is added), and an
// execution matrix under wasmtime. The last tier is also the stock evidence:
// wasmtime instantiates and runs the module, which the fork's validator cannot
// establish about itself.
//
// The matrix asserts more than "it trapped". Every guard must trap through
// `unreachable`, because the Rocq contract enumerates the roles that
// instruction plays and a guard that reached WebAssembly's own trap would be
// outside that table — and because the multiply row's own round-trip, `r / b
// == a`, does exactly that at (MIN, -1), where the wrapped product is MIN and
// the divisor is -1, unless the `-1` arm keeps that pair away from the
// division. So each overflowing vector asserts
// `Trap::UnreachableCodeReached`, which `Trap::IntegerOverflow` and
// `Trap::IntegerDivisionByZero` both fail.
//
// The boundary vectors are as load-bearing as the trapping ones: a guard that
// fired one value early would pass every trap assertion here and be wrong. The
// pairs that must NOT trap include (2^62, -2), whose product lands exactly on
// i64::MIN, and 3037000499 squared, the largest square an i64 holds.

#[cfg(test)]
mod checked_arith_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, codegen_with_full_config,
        get_test_file_path, get_test_wasm_path, wasm_codegen,
    };
    use inference_wasm_codegen::{CompilationMode, OptLevel, Target};
    use wasmtime::{Engine, Instance, Module, Store, Trap, TypedFunc};

    const TEST_NAME: &str = "checked_arith";

    /// The fixture source.
    fn source() -> String {
        let path = get_test_file_path(module_path!(), TEST_NAME);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
    }

    /// The fixture compiled, instantiated and ready to call.
    fn instantiate() -> (Store<()>, Instance) {
        let wasm = wasm_codegen(&source());
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm).unwrap_or_else(|e| panic!("module build: {e}"));
        let mut store = Store::new(&engine, ());
        let instance =
            Instance::new(&mut store, &module, &[]).unwrap_or_else(|e| panic!("instantiate: {e}"));
        (store, instance)
    }

    /// Asserts that a call's failure was a guard firing rather than a trap the
    /// machine raised on its own.
    fn assert_guard_trap(error: &wasmtime::Error, what: &str) {
        let trap = error
            .downcast_ref::<Trap>()
            .unwrap_or_else(|| panic!("{what} failed without a wasm trap: {error}"));
        assert_eq!(
            *trap,
            Trap::UnreachableCodeReached,
            "{what} must trap through the guard's `unreachable`, not through the machine"
        );
    }

    #[test]
    fn checked_arith_test() {
        let actual = wasm_codegen(&source());
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("guarded module is invalid: {e}"));
        let expected_path = get_test_wasm_path(module_path!(), TEST_NAME);
        let expected = std::fs::read(&expected_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", expected_path.display()));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), TEST_NAME);
    }

    #[test]
    fn proof_and_compile_builds_of_a_guarded_source_are_byte_identical() {
        // `verified = deployed`, stated as bytes. The fixture is spec-free, so
        // the two builds have no proof-mode-only construct to account for a
        // difference — and a guard is not one: it is emitted from the source's
        // own arithmetic, identically under both modes, because a proof about
        // the emitted `.v` has to be a proof about the module that ships.
        let text = source();
        let compile =
            codegen_with_full_config(&text, Target::Wasm32, CompilationMode::Compile, OptLevel::O3)
                .expect("compile-mode codegen failed");
        let proof =
            codegen_with_full_config(&text, Target::Wasm32, CompilationMode::Proof, OptLevel::O3)
                .expect("proof-mode codegen failed");
        assert_wasms_modules_equivalence(compile.wasm(), proof.wasm());
    }

    /// The issue's own reproducer, written the way a program that means to wrap
    /// writes it: it returns the number the language returned for it before any
    /// of this existed.
    ///
    /// The trapping half is the fixture's `run`, which is unannotated like every
    /// other row of the catalogue. This half is inline rather than a second pair
    /// of functions in that fixture so the catalogue's golden stays exactly the
    /// module it was: those bytes were produced when each row was written
    /// `checked(...)` under a modular default, and they are still what the
    /// fixture compiles to now that each row is written plainly under a checked
    /// one — which is the whole of what removing the annotations was allowed to
    /// do.
    #[test]
    fn the_modular_spelling_of_the_reproducer_returns_the_wrapped_product() {
        let wasm = wasm_codegen(
            "pub fn fixmul(a: i64, b: i64) -> i64 { \
               const ONE: i64 = 1048576; \
               return wrapping(a * b) / ONE; \
             } \
             pub fn run() -> i64 { \
               const H: i64 = 4194304000; \
               return fixmul(H, H); \
             }",
        );
        let engine = Engine::default();
        let module = Module::new(&engine, &wasm).unwrap_or_else(|e| panic!("module build: {e}"));
        let mut store = Store::new(&engine, ());
        let instance =
            Instance::new(&mut store, &module, &[]).unwrap_or_else(|e| panic!("instantiate: {e}"));
        let run: TypedFunc<(), i64> = instance
            .get_typed_func(&mut store, "run")
            .expect("the module exports `run`");
        // The true product `H * H` is 17592186044416000000, which sixty-four
        // bits do not hold; wrapped and scaled back by `ONE` it is this. The
        // value the fixed-point multiply is reaching for, 16777216000000, sits
        // well inside `i64` — the overflow is in the intermediate.
        assert_eq!(run.call(&mut store, ()).expect("call"), -814_970_044_416);
    }

    #[test]
    fn a_same_kind_nesting_emits_one_guard_per_operator() {
        // Two additions under two annotations produce two guards in one
        // function, sharing one pool of scratch locals. That is only sound
        // because the inner guard's live range closes before the outer one's
        // opens: the outer guard writes its first slot after both of its
        // operands are on the stack, and computing the right-hand operand is
        // what ran the inner guard to completion.
        cov_mark::check_count!(wasm_codegen_overflow_guard_add, 2);
        let wasm = wasm_codegen(
            "pub fn nested(a: i64, b: i64, c: i64) -> i64 { return checked(a + checked(b + c)); }",
        );
        inf_wasmparser::validate(&wasm).unwrap_or_else(|e| panic!("nested module is invalid: {e}"));
    }

    #[test]
    fn a_same_kind_multiply_nesting_emits_one_guard_per_operator() {
        cov_mark::check_count!(wasm_codegen_overflow_guard_mul, 2);
        let wasm = wasm_codegen(
            "pub fn nested(a: i64, b: i64, c: i64) -> i64 { return checked(a * checked(b * c)); }",
        );
        inf_wasmparser::validate(&wasm).unwrap_or_else(|e| panic!("nested module is invalid: {e}"));
    }

    #[test]
    fn a_same_kind_subtract_nesting_emits_one_guard_per_operator() {
        // The subtract row's own witness that two guards of one kind share one
        // pool. Its sequence differs from the add row's in the sense of the
        // second comparison alone, so a nesting that worked for `+` says
        // nothing about `-`.
        cov_mark::check_count!(wasm_codegen_overflow_guard_sub, 2);
        let wasm = wasm_codegen(
            "pub fn nested(a: i64, b: i64, c: i64) -> i64 { return checked(a - checked(b - c)); }",
        );
        inf_wasmparser::validate(&wasm).unwrap_or_else(|e| panic!("nested module is invalid: {e}"));
    }

    #[test]
    fn a_checked_negation_reaches_the_negation_row() {
        // The one governed unary operator, pinned to its own row: negation is
        // classified separately from the binary operators, so nothing else in
        // this file would notice it falling through to no guard at all.
        cov_mark::check!(wasm_codegen_overflow_guard_neg);
        let wasm = wasm_codegen("pub fn neg(a: i64) -> i64 { return checked(-a); }");
        inf_wasmparser::validate(&wasm)
            .unwrap_or_else(|e| panic!("negation module is invalid: {e}"));
    }

    #[test]
    fn a_checked_narrow_operator_reaches_the_narrow_row() {
        // The narrow widths take one row for every operator, chosen before the
        // per-operator table is consulted, so a sum at `i8` is the shortest
        // source that shows the classifier still routes them there.
        cov_mark::check!(wasm_codegen_overflow_guard_narrow);
        let wasm = wasm_codegen("pub fn add(a: i8, b: i8) -> i8 { return checked(a + b); }");
        inf_wasmparser::validate(&wasm).unwrap_or_else(|e| panic!("narrow module is invalid: {e}"));
    }

    #[test]
    fn checked_arith_execution_test() {
        let (mut store, instance) = instantiate();

        macro_rules! func {
            ($name:literal, $args:ty, $ret:ty) => {{
                let f: TypedFunc<$args, $ret> = instance
                    .get_typed_func(&mut store, $name)
                    .unwrap_or_else(|e| panic!("get {}: {e}", $name));
                f
            }};
        }

        macro_rules! value {
            ($f:expr, $name:literal, $args:expr, $expected:expr) => {{
                let args = $args;
                let got = $f
                    .call(&mut store, args)
                    .unwrap_or_else(|e| panic!("{}{:?} must not trap: {e}", $name, args));
                assert_eq!(got, $expected, "{}{:?}", $name, args);
            }};
        }

        macro_rules! traps {
            ($f:expr, $name:literal, $args:expr) => {{
                let args = $args;
                let error = $f
                    .call(&mut store, args)
                    .expect_err(concat!($name, " must trap"));
                assert_guard_trap(&error, &format!("{}{:?}", $name, args));
            }};
        }

        // --- signed add and subtract, i32 ---
        let add_i32 = func!("add_i32", (i32, i32), i32);
        value!(add_i32, "add_i32", (i32::MAX, 0), i32::MAX);
        traps!(add_i32, "add_i32", (i32::MAX, 1));
        traps!(add_i32, "add_i32", (i32::MIN, -1));
        let sub_i32 = func!("sub_i32", (i32, i32), i32);
        value!(sub_i32, "sub_i32", (5, 3), 2);
        traps!(sub_i32, "sub_i32", (i32::MIN, 1));
        traps!(sub_i32, "sub_i32", (i32::MAX, -1));
        // `b` at exactly zero, where the second comparison flips: the row reads
        // `b >s 0`, and reading `b >=s 0` instead makes this pair trap.
        value!(sub_i32, "sub_i32", (i32::MIN, 0), i32::MIN);

        // --- signed add and subtract, i64 ---
        let add_i64 = func!("add_i64", (i64, i64), i64);
        value!(add_i64, "add_i64", (i64::MAX, 0), i64::MAX);
        traps!(add_i64, "add_i64", (i64::MAX, 1));
        traps!(add_i64, "add_i64", (i64::MIN, -1));
        let sub_i64 = func!("sub_i64", (i64, i64), i64);
        value!(sub_i64, "sub_i64", (5, 3), 2);
        traps!(sub_i64, "sub_i64", (i64::MIN, 1));
        traps!(sub_i64, "sub_i64", (i64::MAX, -1));
        value!(sub_i64, "sub_i64", (i64::MIN, 0), i64::MIN);

        // --- unsigned add and subtract (u32 rides in an i32, u64 in an i64) ---
        let add_u32 = func!("add_u32", (i32, i32), i32);
        value!(add_u32, "add_u32", (3, 5), 8);
        traps!(add_u32, "add_u32", (-1, 1)); // u32::MAX + 1
        // The sum equal to the operand it is compared against: the row reads
        // `r <u b`, and reading `r <=u b` instead makes every such pair trap.
        value!(add_u32, "add_u32", (0, 0), 0);
        let sub_u32 = func!("sub_u32", (i32, i32), i32);
        value!(sub_u32, "sub_u32", (5, 3), 2);
        traps!(sub_u32, "sub_u32", (3, 5));
        // Equal operands, the difference exactly zero: the row reads `a <u b`,
        // and reading `a <=u b` instead makes this pair trap.
        value!(sub_u32, "sub_u32", (5, 5), 0);
        let add_u64 = func!("add_u64", (i64, i64), i64);
        value!(add_u64, "add_u64", (3, 5), 8);
        traps!(add_u64, "add_u64", (-1, 1)); // u64::MAX + 1
        value!(add_u64, "add_u64", (0, 0), 0);
        let sub_u64 = func!("sub_u64", (i64, i64), i64);
        value!(sub_u64, "sub_u64", (5, 3), 2);
        traps!(sub_u64, "sub_u64", (3, 5));
        value!(sub_u64, "sub_u64", (5, 5), 0);

        // --- negation: the width's minimum is the one input without a result ---
        let neg_i32 = func!("neg_i32", i32, i32);
        value!(neg_i32, "neg_i32", i32::MAX, i32::MIN + 1);
        traps!(neg_i32, "neg_i32", i32::MIN);
        let neg_i64 = func!("neg_i64", i64, i64);
        value!(neg_i64, "neg_i64", i64::MAX, i64::MIN + 1);
        traps!(neg_i64, "neg_i64", i64::MIN);

        // --- signed multiply at i64, the row the division round-trip is for ---
        let mul_i64 = func!("mul_i64", (i64, i64), i64);
        value!(mul_i64, "mul_i64", (i64::MIN, 1), i64::MIN);
        value!(mul_i64, "mul_i64", (0, i64::MIN), 0);
        // The product lands exactly on i64::MIN and is therefore representable;
        // a guard that tested the sign of the result instead would trap here.
        value!(mul_i64, "mul_i64", (1 << 62, -2), i64::MIN);
        // 3037000499 is the largest N whose square an i64 holds.
        value!(
            mul_i64,
            "mul_i64",
            (3_037_000_499, 3_037_000_499),
            9_223_372_030_926_249_001
        );
        traps!(mul_i64, "mul_i64", (3_037_000_500, 3_037_000_500));
        // Both orders of the (MIN, -1) pair: the first is caught by the `-1`
        // arm before any division runs, the second by the round-trip. Without
        // the `-1` arm the first would reach the machine's own overflow trap.
        traps!(mul_i64, "mul_i64", (i64::MIN, -1));
        traps!(mul_i64, "mul_i64", (-1, i64::MIN));
        // A zero right operand, the one divisor the round-trip cannot use. The
        // row excludes it before dividing; without that arm this pair reaches
        // the machine's own division-by-zero trap instead of returning.
        value!(mul_i64, "mul_i64", (i64::MIN, 0), 0);

        // --- signed multiply at i32 ---
        let mul_i32 = func!("mul_i32", (i32, i32), i32);
        value!(mul_i32, "mul_i32", (46_340, 46_340), 2_147_395_600);
        traps!(mul_i32, "mul_i32", (46_341, 46_341));
        traps!(mul_i32, "mul_i32", (i32::MIN, -1));
        value!(mul_i32, "mul_i32", (i32::MIN, 0), 0);
        // The minimum multiplied by one: representable, and the pair that
        // reaches the round-trip with the widest operand the division can
        // carry back.
        value!(mul_i32, "mul_i32", (i32::MIN, 1), i32::MIN);

        // --- unsigned multiply ---
        let mul_u32 = func!("mul_u32", (i32, i32), i32);
        value!(mul_u32, "mul_u32", (65_535, 65_535), -131_071); // 4294836225 as u32
        traps!(mul_u32, "mul_u32", (65_536, 65_536)); // 2^32 exactly
        value!(mul_u32, "mul_u32", (-1, 0), 0); // u32::MAX * 0
        let mul_u64 = func!("mul_u64", (i64, i64), i64);
        value!(mul_u64, "mul_u64", (1 << 32, 1 << 31), i64::MIN); // 2^63 as u64
        traps!(mul_u64, "mul_u64", (1 << 32, 1 << 32)); // 2^64
        value!(mul_u64, "mul_u64", (-1, 0), 0); // u64::MAX * 0

        // --- the narrow widths, where overflow is "re-narrowing changes it" ---
        let add_i8 = func!("add_i8", (i32, i32), i32);
        value!(add_i8, "add_i8", (100, 27), 127);
        traps!(add_i8, "add_i8", (127, 1));
        let sub_i8 = func!("sub_i8", (i32, i32), i32);
        value!(sub_i8, "sub_i8", (-100, 28), -128);
        traps!(sub_i8, "sub_i8", (-128, 1));
        let mul_i8 = func!("mul_i8", (i32, i32), i32);
        value!(mul_i8, "mul_i8", (63, 2), 126);
        traps!(mul_i8, "mul_i8", (100, 2));
        value!(mul_i8, "mul_i8", (100, 0), 0);
        let neg_i8 = func!("neg_i8", i32, i32);
        value!(neg_i8, "neg_i8", 127, -127);
        traps!(neg_i8, "neg_i8", -128);

        let add_i16 = func!("add_i16", (i32, i32), i32);
        value!(add_i16, "add_i16", (32_000, 767), 32_767);
        traps!(add_i16, "add_i16", (32_767, 1));
        let sub_i16 = func!("sub_i16", (i32, i32), i32);
        value!(sub_i16, "sub_i16", (-32_768, 0), -32_768);
        traps!(sub_i16, "sub_i16", (32_767, -1));
        let mul_i16 = func!("mul_i16", (i32, i32), i32);
        value!(mul_i16, "mul_i16", (181, 181), 32_761);
        traps!(mul_i16, "mul_i16", (182, 182));
        let neg_i16 = func!("neg_i16", i32, i32);
        value!(neg_i16, "neg_i16", 32_767, -32_767);
        traps!(neg_i16, "neg_i16", -32_768);

        let add_u8 = func!("add_u8", (i32, i32), i32);
        value!(add_u8, "add_u8", (200, 55), 255);
        traps!(add_u8, "add_u8", (255, 1));
        let sub_u8 = func!("sub_u8", (i32, i32), i32);
        value!(sub_u8, "sub_u8", (5, 3), 2);
        traps!(sub_u8, "sub_u8", (3, 5));
        let mul_u8 = func!("mul_u8", (i32, i32), i32);
        value!(mul_u8, "mul_u8", (15, 17), 255);
        traps!(mul_u8, "mul_u8", (16, 16));

        let add_u16 = func!("add_u16", (i32, i32), i32);
        value!(add_u16, "add_u16", (65_000, 535), 65_535);
        traps!(add_u16, "add_u16", (65_535, 1));
        let sub_u16 = func!("sub_u16", (i32, i32), i32);
        value!(sub_u16, "sub_u16", (65_535, 1), 65_534);
        traps!(sub_u16, "sub_u16", (3, 5));
        let mul_u16 = func!("mul_u16", (i32, i32), i32);
        value!(mul_u16, "mul_u16", (255, 257), 65_535);
        // The corpus pins this product wrapping to 34464 where it is unguarded;
        // under a guard the same source traps instead.
        traps!(mul_u16, "mul_u16", (1_000, 100));

        // --- the nestings ---
        // The inner sum wraps and the guarded product then fits, so the outer
        // guard must not fire on a wrap it was told to allow.
        let mixed = func!("mixed_nesting", (i64, i64, i64), i64);
        value!(mixed, "mixed_nesting", (1, i64::MAX, 1), i64::MIN);
        let nested_add = func!("nested_add_i64", (i64, i64, i64), i64);
        value!(nested_add, "nested_add_i64", (1, 2, 3), 6);
        traps!(nested_add, "nested_add_i64", (0, i64::MAX, 1));
        traps!(nested_add, "nested_add_i64", (1, i64::MAX, 0));
        let nested_mul = func!("nested_mul_i64", (i64, i64, i64), i64);
        value!(nested_mul, "nested_mul_i64", (2, 3, 5), 30);
        traps!(nested_mul, "nested_mul_i64", (1, 3_037_000_500, 3_037_000_500));
        traps!(nested_mul, "nested_mul_i64", (3_037_000_500, 3_037_000_500, 1));

        // --- both width classes in one function, each reading its own pool ---
        let both = func!("both_widths", (i32, i32, i64, i64), i32);
        value!(both, "both_widths", (6, 7, 1, 1), 42);
        traps!(both, "both_widths", (6, 7, i64::MAX, 1));
        traps!(both, "both_widths", (46_341, 46_341, 1, 1));

        // --- a guard inside a loop, with a `break` after it ---
        // The three vectors take the three ways out of that loop, so the block
        // levels the `break` counts are observed with the guard's own `if`s
        // opened and closed before it.
        let guarded_loop = func!("guarded_loop", (i32, i32), i32);
        value!(guarded_loop, "guarded_loop", (2, 3), 9); // the loop condition
        value!(guarded_loop, "guarded_loop", (10, 2), 16); // the `break`
        traps!(guarded_loop, "guarded_loop", (10, 46_341)); // the guard

        // --- the reproducer ---
        let fixmul = func!("fixmul", (i64, i64), i64);
        value!(fixmul, "fixmul", (2_097_152, 2_097_152), 4_194_304);
        let run = func!("run", (), i64);
        traps!(run, "run", ());
    }
}

/// Regenerates the checked_arith golden artifacts.
///
/// ```bash
/// cargo test -p inference-tests codegen::wasm::checked_arith::regenerate -- --ignored
/// ```
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("checked_arith")
    }

    #[test]
    #[ignore]
    fn regenerate_checked_arith_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("checked_arith.inf"))
            .expect("failed to read checked_arith.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("generated wasm module is invalid: {e}"));
        let wasm_path = dir.join("checked_arith.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "checked_arith");
    }
}
