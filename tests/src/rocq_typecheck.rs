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
    /// structured control flow (`if`/`loop`). The `unique` block and the
    /// `exists`-kind spec function are deliberately absent — neither has a
    /// `hassert` encoding, so proof-mode codegen rejects them with fatal
    /// `P002`/`P001` diagnostics (pinned by the unit tests in
    /// `core/wasm-codegen/src/hassert/tests.rs` and end-to-end by
    /// `build_v_rejects_unique_block_with_p002` in `apps/infs`) rather than by a
    /// corpus entry that would compile against the stub.
    const CORPUS: &[(&str, &str)] = &[
        ("with_spec.inf", "with_spec"),
        ("spec_nondet_blocks.inf", "spec_nondet_blocks"),
        ("three_specs.inf", "three_specs"),
        ("spec_calls_top.inf", "spec_calls_top"),
        ("spec_method.inf", "spec_method"),
        ("mixed_compile_proof.inf", "mixed_compile_proof"),
        ("rocq_control_flow.inf", "rocq_control_flow"),
        ("rocq_spec_shapes.inf", "rocq_spec_shapes"),
        ("rocq_prime_example.inf", "rocq_prime_example"),
        ("spec_narrow_uzumaki.inf", "spec_narrow_uzumaki"),
        ("spec_short_circuit.inf", "spec_short_circuit"),
        ("spec_narrow_abi.inf", "spec_narrow_abi"),
        ("spec_literal_ctx.inf", "spec_literal_ctx"),
    ];

    /// Constructs the corpus must keep exercising, in the emitted `.v`. Two
    /// families: WASM instructions that survive in the module record's
    /// *executable* function bodies (`BI_*`), and the `hassert` obligation shapes
    /// (`ValidSpec`, `term_eq`, `Himpl`, `T_app`, `T_local`, `HA_ex`, …). The
    /// fork-only non-deterministic constructors (`BI_forall`/`BI_exists`/
    /// `BI_assume`/`BI_unique`/`BI_uzumaki_num`) are deliberately ABSENT — spec
    /// functions are omitted from the module record and non-det is rejected in
    /// surviving bodies, so a regression that reintroduced one would fail the
    /// stub compile, not this needle set. Asserting these keeps the `coqc` gate
    /// meaningful even if a future change stops emitting one of them.
    const REQUIRED_CONSTRUCTS: &[&str] = &[
        "BI_if (",
        "BI_loop (",
        "BI_block (",
        "BI_br ",
        "BI_br_if ",
        "BI_call ",
        "BI_relop ",
        "BI_binop ",
        "BI_testop ",
        "ValidModule ",
        "ValidSpec ",
        ": hassert",
        "list hassert",
        "term_eq",
        "Himpl",
        "T_app ",
        "T_local ",
        "HA_ex",
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
            inference_wasm_codegen::EmitFeatures::default(),
        )
        .unwrap_or_else(|e| panic!("codegen failed for {file}: {e}"));
        // Empty explicit maps: the per-spec indices and the hassert obligations
        // both ride along in the embedded `inference.spec_funcs` /
        // `inference.hspecs` custom sections (see ROCQ_CONTRACT.md).
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let empty_hspecs = inference::HSpecMap::default();
        inference::wasm_to_v(module_name, output.wasm(), &empty, &empty_hspecs)
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

    /// Runs `coqc -Q <work>/wasm Wasm -Q <work>/wasm_verifier WasmVerifier
    /// <file>` and returns combined stdout/stderr on failure. Both logical roots
    /// are mapped so the emitted `From Wasm …` / `From WasmVerifier …` imports
    /// resolve.
    fn coqc_compile(coqc: &str, work: &Path, file: &Path) -> Result<(), String> {
        let output = Command::new(coqc)
            .arg("-Q")
            .arg(work.join("wasm"))
            .arg("Wasm")
            .arg("-Q")
            .arg(work.join("wasm_verifier"))
            .arg("WasmVerifier")
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
        std::fs::create_dir_all(work.join("wasm")).expect("create work/wasm dir");
        std::fs::create_dir_all(work.join("wasm_verifier")).expect("create work/wasm_verifier dir");
        let src_stub = stub_dir();
        // The stub is a two-namespace tree: `wasm/` (`Wasm.*`, the WASM datatypes)
        // and `wasm_verifier/` (`WasmVerifier.*`, the assertion language and the
        // proof-obligation predicates). Copy both, then compile each `.v` in
        // dependency order (`Wasm` first, since `WasmVerifier` imports it).
        let stub_modules: &[(&str, &str)] = &[
            ("wasm", "bytes"),
            ("wasm", "numerics"),
            ("wasm", "datatypes"),
            ("wasm", "host"),
            ("wasm_verifier", "Assertions"),
            ("wasm_verifier", "Verifier"),
        ];
        for (dir, module) in stub_modules {
            let rel = format!("{dir}/{module}.v");
            std::fs::copy(
                src_stub.join(dir).join(format!("{module}.v")),
                work.join(&rel),
            )
            .unwrap_or_else(|e| panic!("copy stub {rel}: {e}"));
        }
        for (dir, module) in stub_modules {
            let file = work.join(dir).join(format!("{module}.v"));
            if let Err(log) = coqc_compile(&coqc, &work, &file) {
                panic!(
                    "vendored stub failed to compile ({dir}/{module}.v):\n{log}\n\
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
    /// producer of `BT_valtype (Some ...)` — via its *executable* functions
    /// `guard_div`/`either`, whose bodies survive in the module record; assert
    /// the exact valued shape and that the fixture emits no term-level
    /// `Binop_i BOI_and` — since `&&`/`||` no longer lower to `i32.and`/`i32.or`,
    /// that shape would only reappear from bitwise `&`/`|` or narrowing masks
    /// (which this fixture has none of), so a regression to strict `i32.and`
    /// lowering surfaces here rather than silently.
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

    /// Committed `.v` golden for the PrimeExample fixture — the repository's first
    /// checked-in Rocq artifact. Regenerate with the `#[ignore]`d
    /// [`regenerate::regenerate_prime_example_v`] after an intentional emitter
    /// change.
    fn prime_golden_path() -> PathBuf {
        get_test_data_path()
            .join("rocq")
            .join("rocq_prime_example.v")
    }

    /// The proof-mode `.v` for the PrimeExample fixture must match a committed
    /// golden byte-for-byte, and that golden must carry the wasm-verifier contract
    /// shape: the module record omits the spec function (only `is_prime` survives),
    /// the obligation is a first-class `hassert` whose cross-call resolves to the
    /// defined function at index 0, an existential arm introduces an `HA_ex`
    /// binder, and both the module- and spec-level theorems are present.
    #[test]
    fn prime_example_matches_committed_v_golden() {
        let generated = generate_v("rocq_prime_example.inf", "rocq_prime_example");
        let golden_path = prime_golden_path();
        let golden = std::fs::read_to_string(&golden_path).unwrap_or_else(|e| {
            panic!(
                "read {} ({e}); regenerate with \
                 `cargo test -p inference-tests regenerate_prime_example_v -- --ignored`",
                golden_path.display()
            )
        });
        assert_eq!(
            generated,
            golden,
            "proof-mode `.v` for rocq_prime_example.inf drifted from the committed \
             golden {}; if the emitter change was intentional, regenerate with \
             `cargo test -p inference-tests regenerate_prime_example_v -- --ignored`",
            golden_path.display()
        );

        // Belt-and-braces on the golden's contract shape, independent of the byte
        // compare so a future regeneration cannot silently launder a contract
        // regression into the committed file.
        assert_eq!(
            golden.matches(": module_func").count(),
            1,
            "the module record must contain exactly one function body (`is_prime`); \
             the spec function must be omitted:\n{golden}"
        );
        assert!(
            !golden.contains("Definition prime_spec"),
            "the spec function `prime_spec` must not appear as a module definition:\n{golden}"
        );
        assert!(
            golden.contains("rocq_prime_example__prime_properties_hspec1 : hassert"),
            "the obligation must be emitted as a first-class `hassert`:\n{golden}"
        );
        assert!(
            golden.contains("T_app 0 ((T_local 0%N) :: nil)"),
            "the `is_prime` cross-call must resolve to defined-fn index 0:\n{golden}"
        );
        assert!(
            golden.contains("HA_ex"),
            "the existential else arm must introduce an `HA_ex` binder:\n{golden}"
        );
        assert!(
            golden.contains("Theorem valid_rocq_prime_example : ValidModule rocq_prime_example."),
            "the 1-ary module-validity theorem must be present:\n{golden}"
        );
        assert!(
            golden.contains(
                "Theorem valid_rocq_prime_example__prime_properties : \
                 ValidSpec rocq_prime_example rocq_prime_example__prime_properties_specs."
            ),
            "the spec-validity theorem must be present:\n{golden}"
        );
    }

    /// Committed `.v` golden for the contextual-literal-typing fixture.
    /// Regenerate with the `#[ignore]`d [`regenerate::regenerate_literal_ctx_v`]
    /// after an intentional emitter change.
    fn literal_ctx_golden_path() -> PathBuf {
        get_test_data_path().join("rocq").join("spec_literal_ctx.v")
    }

    /// A specification body types its integer literals from the positions they
    /// appear in, and the emitted `.v` is where that becomes checkable end to
    /// end: an obligation is only about the program that runs if its constants
    /// are the constants the program computes.
    ///
    /// Every literal in the fixture is wider than `i32`, so each `Vi64` below is
    /// load-bearing — a literal left at the `i32` default could not carry the
    /// value at all. `4294967296` is peer-typed by the `i64` slot it is compared
    /// against, `u64::MAX` is typed by the `u64` parameter it is passed to and
    /// reaches Rocq as the bit pattern `(-1)`, and `main`'s argument literal is
    /// typed by `scaled`'s parameter.
    #[test]
    fn spec_literal_ctx_matches_committed_v_golden() {
        let generated = generate_v("spec_literal_ctx.inf", "spec_literal_ctx");
        let golden_path = literal_ctx_golden_path();
        let golden = std::fs::read_to_string(&golden_path).unwrap_or_else(|e| {
            panic!(
                "read {} ({e}); regenerate with \
                 `cargo test -p inference-tests regenerate_literal_ctx_v -- --ignored`",
                golden_path.display()
            )
        });
        assert_eq!(
            generated,
            golden,
            "proof-mode `.v` for spec_literal_ctx.inf drifted from the committed \
             golden {}; if the emitter change was intentional, regenerate with \
             `cargo test -p inference-tests regenerate_literal_ctx_v -- --ignored`",
            golden_path.display()
        );

        // Contract shape, asserted independently of the byte compare so a future
        // regeneration cannot launder a typing regression into the golden.
        assert!(
            golden.contains("Vi64 4294967296"),
            "the comparison operand must be peer-typed `i64`:\n{golden}"
        );
        assert!(
            golden.contains("Vi64 (-1)"),
            "`u64::MAX` at a `u64` parameter must reach Rocq as the `i64` bit \
             pattern (-1):\n{golden}"
        );
        assert!(
            !golden.contains("Vi32 4294967296"),
            "no literal in this fixture fits `i32`; a `Vi32` spelling of one \
             would mean the default won over the position:\n{golden}"
        );
        assert!(
            golden.contains("BI_const_num (Vi64 4294967296)"),
            "the executable `main` must pass its argument literal at `i64` \
             width:\n{golden}"
        );
    }

    /// Regeneration helpers for the committed `.v` goldens. `#[ignore]`d by
    /// design (per CONTRIBUTING.md): they are not behavioral tests but rewrite a
    /// golden from current emitter output. Run explicitly after an intentional
    /// change, e.g.
    /// `cargo test -p inference-tests regenerate_prime_example_v -- --ignored`.
    #[cfg(test)]
    mod regenerate {
        use super::{generate_v, literal_ctx_golden_path, prime_golden_path};
        use std::path::Path;

        fn write_golden(v: &str, path: &Path) {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .unwrap_or_else(|e| panic!("create {}: {e}", parent.display()));
            }
            std::fs::write(path, v).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
            println!("Regenerated: {} ({} bytes)", path.display(), v.len());
        }

        #[test]
        #[ignore]
        fn regenerate_prime_example_v() {
            let v = generate_v("rocq_prime_example.inf", "rocq_prime_example");
            write_golden(&v, &prime_golden_path());
        }

        #[test]
        #[ignore]
        fn regenerate_literal_ctx_v() {
            let v = generate_v("spec_literal_ctx.inf", "spec_literal_ctx");
            write_golden(&v, &literal_ctx_golden_path());
        }
    }
}
