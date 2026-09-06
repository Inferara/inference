//! Parameters written `_: T`.
//!
//! An unnamed parameter still occupies an ABI slot: the call site pushes an
//! argument for it and the declared signature counts it, so it takes a
//! WebAssembly parameter and advances the slot counter. What it does not take is
//! a name — nothing in the body can read it, so no `locals_map` entry is invented
//! for it and no frame slot is allocated for a compound one, which is why the
//! `ignored_compound_parameter` module declares no linear memory at all.
//!
//! At an export boundary the treatment is uniform with a named parameter,
//! because the WebAssembly ABI contract is a property of the slot rather than of
//! the binding: a host passing 300 for a `u8` slot gets it canonicalized, and one
//! passing a tag no variant names traps, whether or not the body reads the value.

#[cfg(test)]
mod unnamed_params_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };
    use wasmtime::{Engine, Instance, Module, Store, Trap, TypedFunc};

    /// Reads a fixture, generates its module, and byte-compares it against the
    /// committed golden (and the `.wat` rendering when one exists).
    fn assert_golden(test_name: &str) -> Vec<u8> {
        let test_file_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&test_file_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {test_file_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {e}"));
        let expected_path = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected_path)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
        actual
    }

    fn instantiate(wasm: &[u8]) -> (Store<()>, Instance) {
        let engine = Engine::default();
        let module =
            Module::new(&engine, wasm).unwrap_or_else(|e| panic!("Failed to build module: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("Failed to instantiate: {e}"));
        (store, instance)
    }

    fn assert_main_returns(wasm_bytes: &[u8], expected: i32) {
        let (mut store, instance) = instantiate(wasm_bytes);
        let main: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "main")
            .unwrap_or_else(|e| panic!("Failed to get 'main': {e}"));
        let result = main
            .call(&mut store, ())
            .unwrap_or_else(|e| panic!("Call to 'main' failed: {e}"));
        assert_eq!(result, expected, "'main' returned an unexpected value");
    }

    #[test]
    fn ignored_parameter_test() {
        cov_mark::check_count!(wasm_codegen_emit_unnamed_param, 1);
        assert_golden("ignored_parameter");
    }

    /// The unnamed slot still receives its argument, so the named parameter ahead
    /// of it keeps slot 0 and the call reads the value it was given rather than
    /// the one that followed.
    #[test]
    fn ignored_parameter_execution_test() {
        let wasm_bytes = assert_golden("ignored_parameter");
        assert_main_returns(&wasm_bytes, 42);
    }

    #[test]
    fn ignored_compound_parameter_test() {
        cov_mark::check_count!(wasm_codegen_emit_unnamed_param, 1);
        let wasm_bytes = assert_golden("ignored_compound_parameter");
        // The decision this fixture exists for: an unnamed compound parameter
        // earns no frame slot and forces no memory. Nothing else in the fixture
        // needs linear memory, so a copy-on-entry for it would be visible as a
        // memory section.
        assert!(
            !wasmprinter::print_bytes(&wasm_bytes)
                .expect("the module must print")
                .contains("(memory"),
            "an unnamed compound parameter must not force a linear memory"
        );
    }

    /// The pointer handed over for the unnamed array is never dereferenced, so
    /// any value is a legal argument for it — passing one that is not a valid
    /// address is exactly the check that it is unread.
    #[test]
    fn ignored_compound_parameter_execution_test() {
        let wasm_bytes = assert_golden("ignored_compound_parameter");
        let (mut store, instance) = instantiate(&wasm_bytes);
        let taking: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "taking")
            .unwrap_or_else(|e| panic!("Failed to get 'taking': {e}"));
        let result = taking
            .call(&mut store, (0x7fff_ffff, 3))
            .unwrap_or_else(|e| panic!("Call to 'taking' failed: {e}"));
        assert_eq!(result, 3, "the named parameter's value must be returned");
        assert_main_returns(&wasm_bytes, 5);
    }

    #[test]
    fn ignored_parameter_after_receiver_test() {
        cov_mark::check_count!(wasm_codegen_emit_unnamed_param, 1);
        assert_golden("ignored_parameter_after_receiver");
    }

    /// The receiver keeps the first parameter slot. Both it and the unnamed
    /// parameter are `i32`, so a module that numbered them the other way round
    /// would still validate and would read the argument as the receiver pointer.
    #[test]
    fn ignored_parameter_after_receiver_execution_test() {
        let wasm_bytes = assert_golden("ignored_parameter_after_receiver");
        assert_main_returns(&wasm_bytes, 42);
    }

    /// One normalization, on the unnamed slot: the named `b: i32` needs none, and
    /// `main` declares no parameters. Were the export prologue to skip unnamed
    /// slots, this count would be zero.
    #[test]
    fn exported_ignored_narrow_parameter_test() {
        cov_mark::check_count!(wasm_codegen_emit_unnamed_param, 1);
        cov_mark::check_count!(wasm_codegen_entry_param_normalization, 1);
        assert_golden("exported_ignored_narrow_parameter");
    }

    #[test]
    fn exported_ignored_narrow_parameter_execution_test() {
        let wasm_bytes = assert_golden("exported_ignored_narrow_parameter");
        let (mut store, instance) = instantiate(&wasm_bytes);
        let f: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "f")
            .unwrap_or_else(|e| panic!("Failed to get 'f': {e}"));
        for host_u8 in [0_i32, 7, 300, -1] {
            let result = f
                .call(&mut store, (host_u8, 7))
                .unwrap_or_else(|e| panic!("Call to 'f' failed: {e}"));
            assert_eq!(
                result, 7,
                "canonicalizing the unnamed slot must not disturb the named one"
            );
        }
        assert_main_returns(&wasm_bytes, 7);
    }

    #[test]
    fn exported_ignored_enum_parameter_test() {
        cov_mark::check_count!(wasm_codegen_emit_unnamed_param, 1);
        cov_mark::check_count!(wasm_codegen_entry_enum_tag_guard, 1);
        assert_golden("exported_ignored_enum_parameter");
    }

    /// The behavioral half of the uniform-ABI decision: an out-of-range tag traps
    /// even though nothing reads the parameter, exactly as it would for a named
    /// one. `Color` has three variants, so 0..2 name a variant and 3 does not;
    /// a negative tag arrives as a huge unsigned value and the same `i32.ge_u`
    /// catches it.
    #[test]
    fn exported_ignored_enum_parameter_execution_test() {
        let wasm_bytes = assert_golden("exported_ignored_enum_parameter");
        let (mut store, instance) = instantiate(&wasm_bytes);
        let pick: TypedFunc<(i32, i32), i32> = instance
            .get_typed_func(&mut store, "pick")
            .unwrap_or_else(|e| panic!("Failed to get 'pick': {e}"));
        for tag in [0_i32, 1, 2] {
            let result = pick
                .call(&mut store, (tag, 11))
                .unwrap_or_else(|e| panic!("in-range tag {tag} must not trap: {e}"));
            assert_eq!(result, 11);
        }
        for tag in [3_i32, -1] {
            let err = pick
                .call(&mut store, (tag, 11))
                .expect_err("an out-of-range enum tag must trap");
            assert_eq!(
                *err.downcast_ref::<Trap>().expect("wasmtime Trap"),
                Trap::UnreachableCodeReached,
                "enum tag {tag} should trap as unreachable",
            );
        }
    }

    #[test]
    fn exported_ignored_enum_parameter_main_execution_test() {
        let wasm_bytes = assert_golden("exported_ignored_enum_parameter");
        assert_main_returns(&wasm_bytes, 11);
    }

    /// Proof mode is the only one that lowers a specification body's `@`s into
    /// appended choice parameters, and the suffix begins after every declared
    /// parameter. An unnamed one has to be counted there as it is in the
    /// signature: an `exists`-bodied free function's obligation payload denotes
    /// against the real activation frame, so a suffix placed one slot early would
    /// read the argument where the payload expects the drawn value. The two
    /// counts are asserted against each other during emission, so the
    /// disagreement surfaces as a refused compilation rather than a wrong proof.
    #[test]
    fn an_unnamed_parameter_compiles_in_proof_mode() {
        use crate::utils::{AnalysisMode, CodegenAttempt, codegen_attempt_with_mode};

        let source = "spec S { fn f(_: i32) exists { let n: i32 = @; assert(n >= n); } }";
        match codegen_attempt_with_mode(
            source,
            AnalysisMode::Run,
            inference_wasm_codegen::CompilationMode::Proof,
        ) {
            CodegenAttempt::Ok(_) => {}
            CodegenAttempt::Rejected(message) => {
                panic!("an unnamed parameter must compile in proof mode: {message}")
            }
            CodegenAttempt::Panicked(payload) => {
                panic!("proof-mode emission must not crash: {payload}")
            }
        }
    }
}

/// Regenerates the committed goldens from current compiler output.
///
/// ```bash
/// cargo test -p inference-tests codegen::wasm::unnamed_params::regenerate -- --ignored
/// ```
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn unnamed_params_test_dir() -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("unnamed_params")
    }

    fn regenerate_one(test_name: &str) {
        let dir = unnamed_params_test_dir().join(test_name);
        let source_path = dir.join(format!("{test_name}.inf"));
        let source_code = std::fs::read_to_string(&source_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", source_path.display()));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid: {e}"));
        let wasm_path = dir.join(format!("{test_name}.wasm"));
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, test_name);
    }

    #[test]
    #[ignore]
    fn regenerate_ignored_parameter_wasm() {
        regenerate_one("ignored_parameter");
    }

    #[test]
    #[ignore]
    fn regenerate_ignored_compound_parameter_wasm() {
        regenerate_one("ignored_compound_parameter");
    }

    #[test]
    #[ignore]
    fn regenerate_ignored_parameter_after_receiver_wasm() {
        regenerate_one("ignored_parameter_after_receiver");
    }

    #[test]
    #[ignore]
    fn regenerate_exported_ignored_narrow_parameter_wasm() {
        regenerate_one("exported_ignored_narrow_parameter");
    }

    #[test]
    #[ignore]
    fn regenerate_exported_ignored_enum_parameter_wasm() {
        regenerate_one("exported_ignored_enum_parameter");
    }
}
