//! `coqc` round-trip gate for proof-mode `wasm-to-v` output (issue #231).
//!
//! Every other `wasm-to-v` test string-matches the emitted `.v`; none ever
//! type-checks it. That is how the #230 `BI_forall`/`BI_exists` arity bug
//! shipped green — a 1-ary library constructor was applied to two arguments and
//! every substring assertion still passed. This suite closes that gap: it drives
//! the real pipeline (parse → type-check → proof-mode codegen → `wasm_to_v`) for
//! a corpus of fixtures spanning the proof surface, then compiles each generated
//! module with `coqc` against the vendored signature stub in
//! `core/wasm-to-v/rocq-stub/`. A mis-aritied or renamed constructor becomes a
//! `coqc` type error instead of a silent pass that only fails on the paid prover
//! worker.
//!
//! The stub encodes the contract *as the emitter writes it* (see the stub
//! README). It provides signatures only — no semantics, no proofs — so this gate
//! asserts **type-checking**, not that proofs close. The emitted per-spec
//! theorems carry an unfilled `(* TODO *)` proof terminated by `Qed.`, which
//! `coqc` rejects as incomplete; [`admit_open_proofs`] rewrites those `Qed.`
//! terminators to `Admitted.` so `coqc` still fully elaborates every `Definition`
//! (the module record and all instruction terms, where arity bugs live) and
//! every theorem *statement* (where a `ValidModule` drift would surface) without
//! demanding a closed proof.
//!
//! `coqc` gating: the compile step runs only when `coqc` is available (via the
//! `COQC` environment variable, else `coqc` on `PATH`). When it is absent the
//! suite prints a clear "skipped" line and returns `Ok` — CI installs `coqc`, so
//! the gate is real there. The corpus generation and proof-surface coverage
//! assertions always run, so even without `coqc` this suite still guards that
//! codegen keeps emitting the arity-critical constructors.

#[cfg(test)]
mod gate {
    use crate::utils::{build_ast, get_test_data_path};
    use inference_type_checker::TypeCheckerBuilder;
    use inference_wasm_codegen::{CompilationMode, OptLevel, Target};
    use rustc_hash::FxHashMap;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// Corpus fixtures under `tests/test_data/inf/`, paired with the module name
    /// each is translated under. Together they exercise the proof-mode surface
    /// the issue calls out: inline and function-body-modifier `forall`/`exists`/
    /// `assume`, cross-function calls (`BI_call`), comparisons, `assert`, and
    /// structured control flow (`if`/`loop`). The `unique` block is deliberately
    /// absent — it has no honest Rocq lowering, so its proof-mode rejection is
    /// pinned by [`unique_block_is_rejected_in_proof_mode`] below rather than by
    /// a corpus entry that would compile against the stub.
    const CORPUS: &[(&str, &str)] = &[
        ("with_spec.inf", "with_spec"),
        ("spec_nondet_blocks.inf", "spec_nondet_blocks"),
        ("spec_nondet_body_modifiers.inf", "spec_nondet_body_modifiers"),
        ("three_specs.inf", "three_specs"),
        ("spec_calls_top.inf", "spec_calls_top"),
        ("spec_method.inf", "spec_method"),
        ("mixed_compile_proof.inf", "mixed_compile_proof"),
        ("rocq_control_flow.inf", "rocq_control_flow"),
        ("spec_narrow_uzumaki.inf", "spec_narrow_uzumaki"),
        ("spec_short_circuit.inf", "spec_short_circuit"),
        ("spec_narrow_abi.inf", "spec_narrow_abi"),
    ];

    /// Constructors the corpus must keep exercising. These are the
    /// arity-sensitive shapes a codegen or translator regression would silently
    /// drop; asserting their presence keeps the `coqc` gate meaningful even if a
    /// future change stops emitting one of them.
    const REQUIRED_CONSTRUCTS: &[&str] = &[
        "BI_forall (",
        "BI_exists (",
        "BI_assume (",
        "BI_if (",
        "BI_loop (",
        "BI_block (",
        "BI_br ",
        "BI_br_if ",
        "BI_call ",
        "BI_relop ",
        "BI_binop ",
        "BI_testop ",
        "BI_uzumaki_num ",
        "ValidModule ",
        "BT_valtype (Some",
    ];

    /// Proof-mode `.v` for one fixture, driven entirely in-process.
    fn generate_v(file: &str, module_name: &str) -> String {
        let path = get_test_data_path().join("inf").join(file);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let arena = build_ast(source);
        let typed_context = TypeCheckerBuilder::build_typed_context(arena)
            .unwrap_or_else(|e| panic!("type check failed for {file}: {e}"))
            .typed_context();
        let output = inference_wasm_codegen::codegen(
            &typed_context,
            Target::Wasm32,
            CompilationMode::Proof,
            OptLevel::O3,
            module_name,
        )
        .unwrap_or_else(|e| panic!("codegen failed for {file}: {e}"));
        // Empty explicit map: the per-spec indices ride along in the embedded
        // `inference.spec_funcs` custom section (see ROCQ_CONTRACT.md).
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        inference::wasm_to_v(module_name, output.wasm(), &empty)
            .unwrap_or_else(|e| panic!("wasm_to_v failed for {file}: {e}"))
    }

    /// Rewrites each `Qed.`-only line to `Admitted.` so `coqc` type-checks the
    /// statements and definitions without requiring the emitted `(* TODO *)`
    /// proofs to close. The match is whitespace-tolerant: a line is rewritten
    /// when its content is exactly `Qed.` after trimming surrounding whitespace,
    /// preserving the line's original leading indentation and newline bytes. A
    /// stricter column-0 match would be silently skipped if a future emitter
    /// indented the terminator, and the skip would surface as a misleading
    /// `coqc` "incomplete proof" error rather than the type error this gate
    /// exists to catch. The emitter only ever writes `Qed.` for these unfilled
    /// per-spec theorem stubs, so this never downgrades a genuinely closed proof.
    fn admit_open_proofs(v: &str) -> String {
        let mut out = String::with_capacity(v.len());
        for line in v.split_inclusive('\n') {
            let content = line.trim_end_matches(['\r', '\n']);
            if content.trim() == "Qed." {
                let newline = &line[content.len()..];
                let indent = &content[..content.len() - content.trim_start().len()];
                out.push_str(indent);
                out.push_str("Admitted.");
                out.push_str(newline);
            } else {
                out.push_str(line);
            }
        }
        out
    }

    #[test]
    fn admit_open_proofs_rewrites_qed_variants() {
        let input = "Qed.\n  Qed.\r\n(* not a terminator: Qed. *)\nQed.";
        let expected = "Admitted.\n  Admitted.\r\n(* not a terminator: Qed. *)\nAdmitted.";
        assert_eq!(admit_open_proofs(input), expected);
    }

    /// Physical path of the vendored stub directory, relative to this crate.
    fn stub_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("tests crate has a parent directory")
            .join("core")
            .join("wasm-to-v")
            .join("rocq-stub")
    }

    /// Resolves the `coqc` binary: `COQC` override first, else `coqc` on `PATH`.
    /// Returns `None` when no working `coqc` is reachable.
    fn find_coqc() -> Option<String> {
        let candidate = std::env::var("COQC").unwrap_or_else(|_| "coqc".to_string());
        let ok = Command::new(&candidate)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        ok.then_some(candidate)
    }

    /// Runs `coqc -Q <stub_root> Wasm <file>` and returns combined stdout/stderr
    /// on failure.
    fn coqc_compile(coqc: &str, stub_root: &Path, file: &Path) -> Result<(), String> {
        let output = Command::new(coqc)
            .arg("-Q")
            .arg(stub_root)
            .arg("Wasm")
            .arg(file)
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn {coqc}: {e}"));
        if output.status.success() {
            return Ok(());
        }
        let mut log = String::from_utf8_lossy(&output.stdout).into_owned();
        log.push_str(&String::from_utf8_lossy(&output.stderr));
        Err(log)
    }

    #[test]
    fn corpus_type_checks_against_vendored_stub() {
        // 1. Generate every corpus module in-process.
        let generated: Vec<(&str, String)> = CORPUS
            .iter()
            .map(|&(file, name)| (file, admit_open_proofs(&generate_v(file, name))))
            .collect();

        // 2. Always-on guard: the corpus must keep exercising the proof surface,
        //    independent of whether `coqc` is present on this machine.
        let all: String = generated.iter().map(|(_, v)| v.as_str()).collect();
        for needle in REQUIRED_CONSTRUCTS {
            assert!(
                all.contains(needle),
                "corpus no longer emits `{needle}`; the coqc gate would stop \
                 covering it — add or fix a fixture in tests/test_data/inf/"
            );
        }

        // 3. The coqc compile is gated: real in CI, skipped locally when absent.
        let Some(coqc) = find_coqc() else {
            eprintln!(
                "skipped: coqc not found (set COQC or put coqc on PATH). \
                 Corpus generated and proof-surface coverage verified; \
                 type-checking against the vendored stub was not run."
            );
            return;
        };

        // 4. Compile the vendored stub once into a private temp dir (coqc writes
        //    `.vo` next to sources, so copy out of the read-only repo tree). On a
        //    coqc failure the dir is deliberately kept so the rejected `.v` and
        //    compiled stub are available for a manual `coqc` repro; a successful
        //    run removes it, and the pre-clean below handles a stale same-PID
        //    leftover.
        let work =
            std::env::temp_dir().join(format!("inference_rocq_typecheck_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work).expect("create work dir");
        let src_stub = stub_dir();
        for module in ["bytes", "numerics", "datatypes", "verifier"] {
            let name = format!("{module}.v");
            std::fs::copy(src_stub.join(&name), work.join(&name))
                .unwrap_or_else(|e| panic!("copy stub {name}: {e}"));
        }
        for module in ["bytes", "numerics", "datatypes", "verifier"] {
            let file = work.join(format!("{module}.v"));
            if let Err(log) = coqc_compile(&coqc, &work, &file) {
                panic!(
                    "vendored stub failed to compile ({module}.v):\n{log}\n\
                     work dir kept for inspection: {}",
                    work.display()
                );
            }
        }

        // 5. Type-check every generated module against the compiled stub.
        for (file, v) in &generated {
            let v_path = work.join(format!("{}.v", file.trim_end_matches(".inf")));
            std::fs::write(&v_path, v).unwrap_or_else(|e| panic!("write {file}: {e}"));
            if let Err(log) = coqc_compile(&coqc, &work, &v_path) {
                panic!(
                    "coqc rejected proof-mode output for `{file}` against the \
                     vendored Wasm stub:\n{log}\n\
                     work dir kept for inspection: {}",
                    work.display()
                );
            }
        }

        let _ = std::fs::remove_dir_all(&work);
    }

    /// `&&`/`||` lower to a valued `if (result i32)` block, which proof-mode
    /// translation renders as a valued `BI_if`. This fixture is the first corpus
    /// producer of `BT_valtype (Some ...)`; assert the exact valued shape and
    /// that the fixture emits no term-level `Binop_i BOI_and` — since `&&`/`||`
    /// no longer lower to `i32.and`/`i32.or`, that shape would only reappear from
    /// bitwise `&`/`|` or narrowing masks (which this fixture has none of), so a
    /// regression to strict `i32.and` lowering surfaces here rather than silently.
    #[test]
    fn short_circuit_emits_valued_bi_if() {
        let v = generate_v("spec_short_circuit.inf", "spec_short_circuit");
        assert!(
            v.contains("BI_if (BT_valtype (Some (T_num T_i32)))"),
            "expected a valued `BI_if` from short-circuit `&&`/`||` lowering; got:\n{v}"
        );
        assert!(
            !v.contains("Binop_i BOI_and"),
            "short-circuit lowering must not emit a term-level `Binop_i BOI_and`; got:\n{v}"
        );
    }

    /// `unique` has no honest Rocq lowering (the wasm-verifier library defines
    /// no `BI_unique` constructor), so proof-mode translation must reject it
    /// with a recoverable `UnsupportedFeature` naming the construct — codegen
    /// itself still succeeds, since the WASM-side `0xfc 0x3d` emission is
    /// legitimate proof scaffolding.
    #[test]
    fn unique_block_is_rejected_in_proof_mode() {
        let path = get_test_data_path().join("inf").join("rocq_unique.inf");
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let arena = build_ast(source);
        let typed_context = TypeCheckerBuilder::build_typed_context(arena)
            .unwrap_or_else(|e| panic!("type check failed: {e}"))
            .typed_context();
        let output = inference_wasm_codegen::codegen(
            &typed_context,
            Target::Wasm32,
            CompilationMode::Proof,
            OptLevel::O3,
            "rocq_unique",
        )
        .unwrap_or_else(|e| panic!("codegen must still succeed for `unique`: {e}"));
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let err = inference::wasm_to_v("rocq_unique", output.wasm(), &empty)
            .expect_err("proof-mode translation must reject `unique`");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> = err.downcast_ref();
        assert!(
            matches!(
                typed,
                Some(inference_wasm_to_v_translator::errors::WasmToVError::UnsupportedFeature { .. })
            ),
            "expected UnsupportedFeature; got {err:?}"
        );
        assert!(
            err.to_string().contains("`unique`"),
            "must name the construct; got {err:?}"
        );
    }
}
