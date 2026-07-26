// Short-circuit lowering of `&&` and `||` (see `short_circuit.inf`).
//
// Each `&&`/`||` lowers to a valued `if (result i32)` block whose right operand
// runs only when the left operand does not decide the result. The execution
// suite separates the two path classes by trap identity: a skipped right operand
// never traps, while an evaluated one traps `IntegerDivisionByZero` (division) or
// `UnreachableCodeReached` (out-of-bounds index under Compile-mode bounds checks).

#[cfg(test)]
mod short_circuit_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };

    #[test]
    fn short_circuit_test() {
        cov_mark::check_count!(wasm_codegen_emit_binary_expression, 38);
        cov_mark::check_count!(wasm_codegen_emit_short_circuit_and, 8);
        cov_mark::check_count!(wasm_codegen_emit_short_circuit_or, 3);
        let test_name = "short_circuit";
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
    fn short_circuit_execution_test() {
        use wasmtime::{Engine, Module, Store, Trap, TypedFunc};

        let test_name = "short_circuit";
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

        // A call that must return a value: the short-circuit either skipped the
        // trapping right operand or the right operand ran and produced a value.
        macro_rules! call {
            ($name:expr, $ty:ty, $args:expr, $expected:expr) => {{
                let f: TypedFunc<_, $ty> = instance
                    .get_typed_func(&mut store, $name)
                    .unwrap_or_else(|e| panic!("Failed to get '{}': {e}", $name));
                let result = f
                    .call(&mut store, $args)
                    .unwrap_or_else(|e| panic!("Call to '{}' failed: {e}", $name));
                assert_eq!(result, $expected, "{}({:?}) expected {:?}", $name, $args, $expected);
            }};
        }

        // A call that must trap: the right operand was evaluated and its trap
        // identity pins which operand ran (division vs out-of-bounds index).
        macro_rules! trap {
            ($name:expr, $ty:ty, $args:expr, $trap:expr) => {{
                let f: TypedFunc<_, $ty> = instance
                    .get_typed_func(&mut store, $name)
                    .unwrap_or_else(|e| panic!("Failed to get '{}': {e}", $name));
                let err = f.call(&mut store, $args).err().unwrap_or_else(|| {
                    panic!("{}({:?}) expected to trap but returned a value", $name, $args)
                });
                let trap = err.downcast_ref::<Trap>().unwrap_or_else(|| {
                    panic!("expected a wasmtime Trap from '{}', got: {err:?}", $name)
                });
                assert_eq!(*trap, $trap, "{}({:?}) trap kind", $name, $args);
            }};
        }

        // Headline: the guard idiom `x != 0 && 100 / x > 1` no longer divides by
        // zero at x == 0 (traps on the strict/pre-change lowering).
        call!("guard_div", i32, 0_i32, 0_i32);
        call!("guard_div", i32, 50_i32, 1_i32);
        call!("guard_div", i32, 200_i32, 0_i32);
        call!("guard_div", i32, 1_i32, 1_i32);

        // `x == 0 || 100 / x > 1`: a true left operand of `||` skips the divide.
        call!("guard_div_or", i32, 0_i32, 1_i32);
        call!("guard_div_or", i32, 50_i32, 1_i32);
        call!("guard_div_or", i32, 200_i32, 0_i32);

        // Anti-over-claim: the right operand still runs when the left does not
        // decide. A true `&&` left and a false `||` left both force the divide.
        trap!("and_rhs_runs", i32, (5_i32, 0_i32), Trap::IntegerDivisionByZero);
        call!("and_rhs_runs", i32, (0_i32, 0_i32), 0_i32);
        trap!("or_rhs_runs", i32, (5_i32, 0_i32), Trap::IntegerDivisionByZero);
        call!("or_rhs_runs", i32, (0_i32, 0_i32), 1_i32);

        // Chain laziness: a false first term skips both later divides; a later
        // term still traps once an earlier term forces its evaluation.
        call!("chain3_div", i32, (200_i32, 0_i32, 0_i32), 0_i32);
        trap!(
            "chain3_div",
            i32,
            (1_i32, 0_i32, 5_i32),
            Trap::IntegerDivisionByZero
        );
        call!("chain3_div", i32, (1_i32, 1_i32, 1_i32), 1_i32);

        // Trap identity distinguishes which operand ran: division on the left,
        // out-of-bounds index on the evaluated right (`arr` has length 2).
        trap!("trap_kind", i32, (0_i32, 0_i32), Trap::IntegerDivisionByZero);
        trap!("trap_kind", i32, (5_i32, 9_i32), Trap::UnreachableCodeReached);
        call!("trap_kind", i32, (5_i32, 1_i32), 1_i32);

        // Normalization: `a || b && c` (with `&&` binding tighter) yields exactly
        // 0 or 1, never any other nonzero.
        call!("prec_mix", i32, (1_i32, 0_i32, 0_i32), 1_i32);
        call!("prec_mix", i32, (0_i32, 1_i32, 0_i32), 0_i32);
        call!("prec_mix", i32, (0_i32, 1_i32, 1_i32), 1_i32);

        // `!a` as the `&&` condition: a false `!a` skips the trapping divide.
        call!("mixed_not", i32, (0_i32, 5_i32), 1_i32);
        call!("mixed_not", i32, (1_i32, 0_i32), 0_i32);

        // Loop guard `i < 4 && arr[i] > 0`: at i == 4 the false `i < 4` skips the
        // would-be out-of-bounds `arr[4]`, so the scan stops without trapping.
        call!("loop_guard", i32, (), 10_i32);
    }
}

/// Test data regeneration helper.
///
/// Regenerates the expected `.wasm`/`.wat` from the current compiler output.
/// Run with `--ignored`:
///
/// ```bash
/// cargo test -p inference-tests codegen::wasm::short_circuit::regenerate::regenerate_short_circuit_wasm -- --ignored
/// ```
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("short_circuit")
    }

    #[test]
    #[ignore]
    fn regenerate_short_circuit_wasm() {
        let dir = test_dir();
        let source_code = std::fs::read_to_string(dir.join("short_circuit.inf"))
            .expect("Failed to read short_circuit.inf");
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {}", e));
        let wasm_path = dir.join("short_circuit.wasm");
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, "short_circuit");
    }
}
