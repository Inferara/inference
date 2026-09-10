// Arithmetic at the boundaries of each width, asserted in both spellings.
//
// WebAssembly's own integer operators wrap and never trap, so every one of these
// eight operations used to return the wrapped value. `+`, `-`, `*` and unary `-`
// now carry an overflow guard unless they are written `wrapping(...)`, and each
// of these operations is exactly the pair that guard exists to catch. The
// fixture therefore carries the same eight operations twice: once over `const`
// bindings under `wrapping(...)`, where the execution test asserts the value
// each produced before, and once over parameters unmarked, where it asserts that
// the call traps and that it traps through the guard's own `unreachable` rather
// than through anything the machine raises on its own.
//
// The eight pairs, wrapped value on the left and trapping call on the right:
//   i32_max_plus_one_wrap  = -2147483648  i32_add(2147483647, 1)
//   i32_min_minus_one_wrap = 2147483647   i32_sub(-2147483648, 1)
//   i64_max_plus_one_wrap  = i64::MIN     i64_add(i64::MAX, 1)
//   i64_min_minus_one_wrap = i64::MAX     i64_sub(i64::MIN, 1)
//   u32_max_plus_one_wrap  = 0            u32_add(4294967295, 1)
//   i32_mul_overflow_wrap  = -2           i32_mul(2147483647, 2)
//   i32_neg_min_wrap       = i32::MIN     i32_neg(i32::MIN)
//   i64_neg_min_wrap       = i64::MIN     i64_neg(i64::MIN)
//
// Total: 12 binary expressions, 4 prefix unary (neg), 11 constant definitions,
// 8 annotations and 8 guards — none of the guards in an annotated twin.

#[cfg(test)]
mod arith_overflow_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn arith_overflow_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 12);
        cov_mark::check_count!(wasm_codegen_emit_prefix_unary_expression, 4);
        cov_mark::check_count!(wasm_codegen_emit_unary_neg, 4);
        cov_mark::check_count!(wasm_codegen_emit_constant_definition, 11);
        cov_mark::check_count!(wasm_codegen_emit_arith_mode_expression, 8);
        // One guard per parameterized function and none anywhere else: the
        // annotated twins compute the same eight results with no guard at all,
        // which is what makes them twins rather than copies.
        cov_mark::check_count!(wasm_codegen_overflow_guard_add, 3);
        cov_mark::check_count!(wasm_codegen_overflow_guard_sub, 2);
        cov_mark::check_count!(wasm_codegen_overflow_guard_mul, 1);
        cov_mark::check_count!(wasm_codegen_overflow_guard_neg, 2);
        let test_name = "arith_overflow";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let expected = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
    }

    #[test]
    fn arith_overflow_execution_test() {
        use wasmtime::{Engine, Module, Store, TypedFunc};

        let test_name = "arith_overflow";
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let wasm_bytes = wasm_codegen(&source_code);

        let engine = Engine::default();
        let module = Module::new(&engine, &wasm_bytes)
            .unwrap_or_else(|e| panic!("Failed to create Wasm module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate Wasm module: {e}"));

        // Calls a nullary export and asserts the value it returns.
        macro_rules! wraps_to {
            ($name:expr, $ty:ty, $expected:expr) => {{
                let f: TypedFunc<(), $ty> = instance
                    .get_typed_func(&mut store, $name)
                    .unwrap_or_else(|e| panic!("Failed to get '{}': {e}", $name));
                let value = f
                    .call(&mut store, ())
                    .unwrap_or_else(|e| panic!("{} must not trap: {e}", $name));
                assert_eq!(
                    value, $expected,
                    "{} must still compute the value the machine operator gives",
                    $name
                );
            }};
        }

        // Calls an export on the vector its `wrapping(...)` twin wraps at and
        // asserts that the guard, not the machine, stopped the program.
        macro_rules! traps {
            ($name:expr, $args:ty, $ret:ty, $vector:expr) => {{
                let f: TypedFunc<$args, $ret> = instance
                    .get_typed_func(&mut store, $name)
                    .unwrap_or_else(|e| panic!("Failed to get '{}': {e}", $name));
                let error = f
                    .call(&mut store, $vector)
                    .expect_err(concat!($name, " must trap on overflow"));
                assert_eq!(
                    error.downcast_ref::<wasmtime::Trap>(),
                    Some(&wasmtime::Trap::UnreachableCodeReached),
                    "{} must trap through the guard rather than through the machine",
                    $name
                );
            }};
        }

        // --- i32 ---

        // i32::MAX + 1
        wraps_to!("i32_max_plus_one_wrap", i32, i32::MIN);
        traps!("i32_add", (i32, i32), i32, (2147483647, 1));
        // i32::MIN - 1
        wraps_to!("i32_min_minus_one_wrap", i32, i32::MAX);
        traps!("i32_sub", (i32, i32), i32, (-2147483648, 1));

        // --- i64 ---

        // i64::MAX + 1
        wraps_to!("i64_max_plus_one_wrap", i64, i64::MIN);
        traps!("i64_add", (i64, i64), i64, (i64::MAX, 1));
        // i64::MIN - 1
        wraps_to!("i64_min_minus_one_wrap", i64, i64::MAX);
        traps!("i64_sub", (i64, i64), i64, (i64::MIN, 1));

        // --- u32, passed and returned as i32 in WASM, so u32::MAX arrives as -1 ---

        // u32::MAX + 1
        wraps_to!("u32_max_plus_one_wrap", i32, 0);
        traps!("u32_add", (i32, i32), i32, (-1, 1));

        // --- Multiplication ---

        // 2147483647 * 2
        wraps_to!("i32_mul_overflow_wrap", i32, -2);
        traps!("i32_mul", (i32, i32), i32, (2147483647, 2));

        // --- Negation of MIN, the one value each signed width cannot negate ---

        wraps_to!("i32_neg_min_wrap", i32, i32::MIN);
        traps!("i32_neg", i32, i32, i32::MIN);
        wraps_to!("i64_neg_min_wrap", i64, i64::MIN);
        traps!("i64_neg", i64, i64, i64::MIN);
    }
}

/// Test data regeneration helper.
///
/// Regenerates the expected `.wasm` and `.wat` golden files from the current compiler output.
/// Run with `--ignored` flag:
///
/// ```bash
/// cargo test -p inference-tests codegen::wasm::arith_overflow::regenerate -- --ignored
/// ```
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("arith_overflow")
    }

    #[test]
    #[ignore]
    fn regenerate_arith_overflow_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("arith_overflow.inf"))
            .expect("Failed to read arith_overflow.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("arith_overflow.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "arith_overflow");
    }
}
