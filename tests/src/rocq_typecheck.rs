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
    use rustc_hash::{FxHashMap, FxHashSet};
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// Corpus fixtures under `tests/test_data/inf/`, paired with the module name
    /// each is translated under. Together they exercise the proof-mode surface
    /// the issue calls out: inline and function-body-modifier `forall`/`exists`/
    /// `assume`, cross-function calls (`BI_call`), comparisons, `assert`,
    /// structured control flow (`if`/`loop`), and negative integer constants at
    /// every width. The reachability kinds are covered by the last three
    /// entries: `rocq_exists_spec.inf` and `rocq_unique_spec.inf` are the
    /// corpus producers of the `ValidExistsSpec` and `ValidUniqueSpec`
    /// grammars respectively (each kind needs its own producer — the two
    /// select different predicates), and `spec_mixed_kinds.inf` puts all
    /// three kinds plus a spec method behind one module so the partitioned
    /// emission — the explicitly typed empty `(@nil hassert)` universal list
    /// next to non-empty `_ex_specs`/`_uq_specs` partitions — elaborates
    /// under `coqc`. Still deliberately absent is the nested `unique` *block*:
    /// it has no `hassert` encoding, so proof-mode codegen rejects it with a
    /// fatal `P002` (pinned by the unit tests in
    /// `core/wasm-codegen/src/hassert/tests.rs` and end-to-end by
    /// `build_v_rejects_unique_block_with_p002` in `apps/infs`).
    ///
    /// The `Hall` universal-binder sugar is reached only by a `forall` block
    /// nested inside an existential context, so the compile below elaborates
    /// that stub declaration only because some fixture writes one.
    /// `spec_quantifier_alternation.inf` is where the shape itself is
    /// exercised, at every nesting the language admits; the two narrow
    /// fixtures reach it incidentally, for the guard a narrow logical variable
    /// carries.
    ///
    /// The operator-matrix entries exist for operator coverage rather than for
    /// a proof shape: between them they put every arithmetic, bitwise, shift
    /// and comparison operator the obligation printer can spell into a fixture
    /// `coqc` elaborates. They are split by theme rather than merged because a
    /// gate failure should name the operator family it is about (#401).
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
        ("spec_narrow_discharge.inf", "spec_narrow_discharge"),
        ("spec_literal_ctx.inf", "spec_literal_ctx"),
        ("spec_negative_consts.inf", "spec_negative_consts"),
        ("spec_bitwise_arith.inf", "spec_bitwise_arith"),
        ("spec_operator_matrix.inf", "spec_operator_matrix"),
        ("rocq_exists_spec.inf", "rocq_exists_spec"),
        ("rocq_unique_spec.inf", "rocq_unique_spec"),
        ("spec_mixed_kinds.inf", "spec_mixed_kinds"),
        ("spec_aggregate_values.inf", "spec_aggregate_values"),
        ("spec_bounded_iteration.inf", "spec_bounded_iteration"),
        (
            "spec_quantifier_alternation.inf",
            "spec_quantifier_alternation",
        ),
    ];

    /// Where a linked external's `.wasm` comes from.
    ///
    /// The two arms answer different questions and neither subsumes the other.
    /// A fixture-built external keeps the gate self-contained — regenerate it by
    /// editing Inference source — but every instruction in it came out of the
    /// same emitter as the main module, so it can only ever restate what a
    /// single-file fixture already covers. A committed artifact is the only way
    /// to put a *foreign* compiler's instruction selection in front of `coqc`,
    /// which is the thing the linker envelope exists for, and the price is that
    /// its bytes are regenerated by hand.
    enum ExternalBytes {
        /// Compiled here from an Inference fixture under `tests/test_data/inf/`.
        Fixture {
            /// The fixture whose own compilation produces the bytes.
            source: &'static str,
            /// The module name the external is compiled under — deliberately not
            /// the logical module, so a confusion between the two cannot pass:
            /// the merged body's symbol tracks the binding, not the external's
            /// own name for itself.
            module_name: &'static str,
        },
        /// Read from a committed `.wasm` under `tests/test_data/wasmlib/`, built
        /// by a toolchain that is not this compiler. That directory's `README.md`
        /// records which one and how to regenerate the bytes.
        Artifact {
            /// File name under `tests/test_data/wasmlib/`.
            file: &'static str,
        },
    }

    /// One external `.wasm` a linked fixture merges: the logical module its
    /// `use … from` clause names, and where its bytes come from.
    struct LinkedExternal {
        /// The name the main fixture's `use … from` clause binds against, and
        /// the key the linker merges under.
        logical_module: &'static str,
        bytes: ExternalBytes,
    }

    impl LinkedExternal {
        /// The external's `.wasm`, built or read according to its source.
        fn wasm(&self) -> Vec<u8> {
            match self.bytes {
                ExternalBytes::Fixture {
                    source,
                    module_name,
                } => compile_fixture(source, module_name, CompilationMode::Compile),
                ExternalBytes::Artifact { file } => {
                    let path = get_test_data_path().join("wasmlib").join(file);
                    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
                }
            }
        }
    }

    /// Corpus fixtures that only type-check once their externals are merged in,
    /// paired with the module name and the externals to link.
    ///
    /// They are kept apart from [`CORPUS`] because they are a different
    /// pipeline, not a different fixture: a linked entry runs codegen twice and
    /// the linker once, and the module the gate compiles is the merged one.
    /// Between them the two entries are the only producers of a `T_app` whose
    /// target is a body the compiler never emitted, and each covers a different
    /// half of what that means.
    ///
    /// `spec_linked_extern.inf` is the minimal shape: one linked `.wasm`, one
    /// `spec` naming it, one obligation applying it. Stated honestly so it is
    /// not read as broader coverage than it is, no new *constructor* reaches
    /// `coqc` through it — an Inference-compiled external goes through the same
    /// emitter as everything else. What is new is that the module record `coqc`
    /// elaborates contains a body the linker produced, under a `Definition`
    /// name that came out of `sanitize_rocq_identifier` over a symbol
    /// containing `.`, with an obligation applying it at that body's own index.
    ///
    /// `spec_linked_toolchain.inf` is the acceptance criterion the envelope was
    /// built for (#363): its external is a committed `wasm32-unknown-unknown`
    /// artifact, so the bodies `coqc` elaborates through it were selected by
    /// LLVM rather than by this compiler. That is the one entry whose
    /// instruction *shapes* nothing else in the corpus can reach — a branchless
    /// clamp and a `BI_loop` carrying a result type, neither of which the
    /// Inference emitter has a way to produce.
    ///
    /// [`linked_corpus_carries_a_merged_body`] is the floor that keeps this
    /// block from being deleted unnoticed — `REQUIRED_CONSTRUCTS` cannot serve
    /// as one, because every needle it lists is already produced by a
    /// single-file fixture.
    ///
    /// Two constraints an entry here inherits, both of which would otherwise
    /// surface far from their cause. The corpus-wide checks apply to linked
    /// modules too, and one of them rejects a data segment in any corpus module
    /// — so an external fixture that used a string or array literal would trip
    /// it, reported against the *main* fixture's name. And a main declares its
    /// linear memory with a fixed maximum while the merge keeps the larger
    /// minimum, so a main fixture that needs a frame cannot take an external
    /// declaring more pages than the corpus compiles with.
    const LINKED_CORPUS: &[(&str, &str, &[LinkedExternal])] = &[
        (
            "spec_linked_extern.inf",
            "spec_linked_extern",
            &[LinkedExternal {
                logical_module: "mathlib",
                bytes: ExternalBytes::Fixture {
                    source: "spec_linked_extern_mathlib.inf",
                    module_name: "mathlib_impl",
                },
            }],
        ),
        (
            "spec_linked_toolchain.inf",
            "spec_linked_toolchain",
            &[LinkedExternal {
                logical_module: "rustlib",
                bytes: ExternalBytes::Artifact {
                    file: "rustlib.wasm",
                },
            }],
        ),
    ];

    /// Constructs the corpus must keep exercising, in the emitted `.v`. Two
    /// families: WASM instructions that survive in the module record's
    /// *executable* function bodies (`BI_*`), and the `hassert` obligation shapes
    /// (`ValidSpec`, `term_eq`, `Himpl`, `T_app`, `T_local`, `HA_ex`, …). The
    /// fork-only non-deterministic constructors (`BI_forall`/`BI_exists`/
    /// `BI_assume`/`BI_unique`/`BI_uzumaki_num`) are deliberately ABSENT —
    /// forall/plain spec functions are omitted from the module record, retained
    /// exists/unique bodies are reachability-lowered to vanilla WASM, and
    /// non-det is rejected in any body the record keeps, so a regression that
    /// reintroduced one would fail the stub compile, not this needle set.
    /// Asserting these keeps the `coqc` gate meaningful even if a future change
    /// stops emitting one of them.
    ///
    /// [`every_stub_declaration_has_a_producer`] now audits constructor
    /// coverage mechanically, which makes this list narrower than it used to be
    /// but not redundant, because the two ask different questions. The audit
    /// asks whether *some* gated module names a constructor, counting the
    /// hand-assembled WAT modules; this list asks whether the *corpus* still
    /// emits one, and the corpus is the only place the real `.inf` → type-check
    /// → codegen → translate chain runs. A lowering change that stopped
    /// emitting `BI_loop` from a source `while` would leave the audit green on
    /// the strength of a hand-written module and fail here. The needles are
    /// also applied *forms* rather than bare names — `BI_if (`,
    /// `BT_valtype (Some`, `list hassert` — where the audit is name-level by
    /// construction. What the audit catches and this list cannot is everything
    /// nobody thought to write down: a hand-maintained needle list only ever
    /// guards what somebody remembered to add, which is the #401 hole itself.
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
        // `Hor` reached the corpus only with the short-circuit witness. Until
        // then no fixture emitted one, so the `coqc` compile below never
        // elaborated the constructor at all and a drift in its arity or
        // spelling would have passed unnoticed — the same class of hole the
        // `BI_forall` arity bug shipped through.
        "Hor ",
        "T_app ",
        "T_local ",
        "HA_ex",
        // The universal binder, reached only by a `forall` block nested inside
        // an existential context — `spec_quantifier_alternation.inf` and the
        // two narrow fixtures are its only producers, so without them `coqc`
        // would never elaborate the `Hall` Definition and a drift in it would
        // pass unnoticed — the hole this needle list exists to keep closed.
        "Hall ",
        "HA_has_type ",
        // The declared value domain a narrow slot is quantified over, which
        // rides in the same conjunct as the width guard above. `HA_has_type`
        // alone cannot stand in for it: a slot whose declaration admits every
        // value of its class states only its width, so deleting the domain
        // emission outright leaves every needle before this point satisfied
        // while the obligations it protects go back to ranging a `u8` over all
        // of `i32`.
        //
        // Each needle is a contiguous form that names no slot, so it holds
        // wherever in a specification the narrow introduction sits. The first
        // is the grouped guard of a narrow universal *binder*, entire — the
        // variable a binder introduces is relative index 0 inside its own
        // guard at every nesting depth, which is what makes the whole
        // width-and-domain pair spellable as one substring. The other two are
        // the halves of a signed pair: the sign-extending widths take a lower
        // bound as well as an upper one, and the lower one is the only
        // comparison the corpus emits with a constant *left* operand.
        //
        // What these pin is the shape, not its provenance — a source
        // comparison written to the same spelling would satisfy them. The
        // fixtures that produce them are `spec_narrow_uzumaki.inf` and
        // `spec_narrow_abi.inf`, whose own comments record which position each
        // obligation covers.
        //
        // `spec_narrow_discharge.inf` is the third, and is here for a reason
        // this gate cannot itself deliver: it type-checks, but it rewrites
        // `Qed.` to `Admitted.` first, so it can no more tell a correct bound
        // from a false one than it can tell a proof from a stub. That fixture's
        // two obligations are discharged for real against wasm-verifier
        // (`theories/examples/Issue357NarrowDomainExample.v`), where the proof
        // stops closing if either bound is dropped or loosened by one. Its
        // presence here keeps the emitted text and the discharged text from
        // drifting apart silently.
        "HA_and (HA_has_type (T_lvar 0) T_i32) (HA_not (term_eq (T_relop T_i32 \
         (Relop_i (ROI_lt SX_U)) (T_lvar 0)",
        "(Relop_i (ROI_le SX_S)) (T_const (Vi32 (-128)))",
        "(Relop_i (ROI_lt SX_S)) (T_lvar 0) (T_const (Vi32 128))",
        "BT_valtype (Some",
        // The reachability grammar, in its applied forms: the kind-selected
        // theorems, the partition list's type ascription, and a record field
        // (one stands in for all four — a record literal spells them
        // together). Absent needles here would let the kind branch in the
        // emitter rot while every forall-only fixture stayed green.
        "ValidExistsSpec ",
        "ValidUniqueSpec ",
        "list reachability_spec",
        "reach_payload",
    ];

    /// Every `Binop_i` spelling the obligation printer can write, in the order
    /// `core/wasm-to-v/src/hassert_print.rs` matches them.
    ///
    /// The obligation printer and the instruction translator are two separate
    /// emitters with two separate per-operator match arms, and this list is the
    /// first one's. It is deliberately not derived from [`INTEGER_BINOPS`]: the
    /// `hassert` term language has no rotate, because the source language has no
    /// rotate operator to build one from, so the two lists differ by exactly the
    /// two rotates and saying so explicitly is clearer than a filter.
    ///
    /// A spelling absent from every emitted `.v` is a spelling `coqc` never
    /// elaborates, which is the hole this list exists to keep closed (#401): the
    /// arm could be renamed or given the wrong arity and the gate would still
    /// pass. Adding an arm to the printer means adding a fixture that produces
    /// it, not just a row here.
    const OBLIGATION_BINOPS: &[&str] = &[
        "BOI_add",
        "BOI_sub",
        "BOI_mul",
        "(BOI_div SX_S)",
        "(BOI_div SX_U)",
        "(BOI_rem SX_S)",
        "(BOI_rem SX_U)",
        "BOI_and",
        "BOI_or",
        "BOI_xor",
        "BOI_shl",
        "(BOI_shr SX_S)",
        "(BOI_shr SX_U)",
    ];

    /// Every `Relop_i` spelling the obligation printer can write. Unlike the
    /// binops this is the same set the instruction translator carries — see
    /// [`OBLIGATION_BINOPS`] for why the two are still listed separately.
    const OBLIGATION_RELOPS: &[&str] = &[
        "ROI_eq",
        "ROI_ne",
        "(ROI_lt SX_S)",
        "(ROI_lt SX_U)",
        "(ROI_gt SX_S)",
        "(ROI_gt SX_U)",
        "(ROI_le SX_S)",
        "(ROI_le SX_U)",
        "(ROI_ge SX_S)",
        "(ROI_ge SX_U)",
    ];

    /// The two integer widths every operator arm is duplicated across, in both
    /// emitters. `T_i32` and `T_i64` select different arms, so a spelling is
    /// only covered once it has been elaborated at both.
    const NUMBER_TYPES: &[&str] = &["T_i32", "T_i64"];

    /// WASM integer binary operators, each paired with the `Binop_i` spelling
    /// the *instruction* translator must print for it.
    ///
    /// This drives both halves of
    /// [`instruction_surface_type_checks_against_vendored_stub`]: the mnemonic
    /// builds the WAT that produces the instruction, the spelling is the needle
    /// that must come back. Deriving the fixture and the expectation from one
    /// table is what makes the coverage claim total — a new arm in the
    /// translator that nobody adds here produces no instruction and matches no
    /// needle, so it cannot quietly appear covered.
    ///
    /// `rotl`/`rotr` are the reason this list is not reachable from `.inf` at
    /// all: Inference has no rotate operator, so the only producer for those two
    /// arms is a hand-assembled module.
    const INTEGER_BINOPS: &[(&str, &str)] = &[
        ("add", "BOI_add"),
        ("sub", "BOI_sub"),
        ("mul", "BOI_mul"),
        ("div_s", "(BOI_div SX_S)"),
        ("div_u", "(BOI_div SX_U)"),
        ("rem_s", "(BOI_rem SX_S)"),
        ("rem_u", "(BOI_rem SX_U)"),
        ("and", "BOI_and"),
        ("or", "BOI_or"),
        ("xor", "BOI_xor"),
        ("shl", "BOI_shl"),
        ("shr_s", "(BOI_shr SX_S)"),
        ("shr_u", "(BOI_shr SX_U)"),
        ("rotl", "BOI_rotl"),
        ("rotr", "BOI_rotr"),
    ];

    /// WASM integer comparisons, paired with the `Relop_i` spelling the
    /// instruction translator must print. Used exactly like [`INTEGER_BINOPS`].
    const INTEGER_RELOPS: &[(&str, &str)] = &[
        ("eq", "ROI_eq"),
        ("ne", "ROI_ne"),
        ("lt_s", "(ROI_lt SX_S)"),
        ("lt_u", "(ROI_lt SX_U)"),
        ("gt_s", "(ROI_gt SX_S)"),
        ("gt_u", "(ROI_gt SX_U)"),
        ("le_s", "(ROI_le SX_S)"),
        ("le_u", "(ROI_le SX_U)"),
        ("ge_s", "(ROI_ge SX_S)"),
        ("ge_u", "(ROI_ge SX_U)"),
    ];

    /// Compiles one fixture under `tests/test_data/inf/` and returns its WASM.
    fn compile_fixture(file: &str, module_name: &str, mode: CompilationMode) -> Vec<u8> {
        let path = get_test_data_path().join("inf").join(file);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let arena = build_ast(source);
        let typed_context = TypeCheckerBuilder::build_typed_context(arena)
            .unwrap_or_else(|e| panic!("type check failed for {file}: {e}"))
            .typed_context();
        inference_wasm_codegen::codegen(
            &typed_context,
            module_name,
            inference_wasm_codegen::CodegenOptions {
                target: Target::Wasm32,
                mode,
                opt_level: OptLevel::O3,
                features: inference_wasm_codegen::EmitFeatures::default(),
                layout: inference_wasm_codegen::MemoryLayout::default(),
            },
        )
        .unwrap_or_else(|e| panic!("codegen failed for {file}: {e}"))
        .wasm()
        .to_vec()
    }

    /// Proof-mode `.v` for one single-file fixture, driven entirely in-process.
    fn generate_v(file: &str, module_name: &str) -> String {
        let wasm = compile_fixture(file, module_name, CompilationMode::Proof);
        translate(file, module_name, &wasm)
    }

    /// Proof-mode `.v` for a fixture that links external `.wasm` modules,
    /// running the same three steps a real `infc -L` build does: obtain each
    /// external's bytes, compile the main fixture, statically merge, and
    /// translate the *merged* module.
    ///
    /// The merge is what makes an obligation about an `external fn` resolvable
    /// at all: before it the external is an import, and an import carries no
    /// body for the downstream realization obligation to reduce.
    fn generate_linked_v(file: &str, module_name: &str, externals: &[LinkedExternal]) -> String {
        let libs: Vec<(&str, Vec<u8>)> = externals
            .iter()
            .map(|external| (external.logical_module, external.wasm()))
            .collect();
        let lib_refs: Vec<(&str, &[u8])> = libs
            .iter()
            .map(|(name, bytes)| (*name, bytes.as_slice()))
            .collect();
        let main = compile_fixture(file, module_name, CompilationMode::Proof);
        let linked = inference::link(&main, &lib_refs)
            .unwrap_or_else(|e| panic!("link failed for {file}: {e}"));
        translate(file, module_name, &linked)
    }

    /// The translation step both generators share.
    fn translate(file: &str, module_name: &str, wasm: &[u8]) -> String {
        // Empty explicit maps: the per-spec indices and the hassert obligations
        // both ride along in the embedded `inference.spec_funcs` /
        // `inference.hspecs` custom sections (see ROCQ_CONTRACT.md). For a
        // linked module they are also the only correct source — the linker
        // rewrote the embedded indices into the post-merge space, leaving
        // codegen's own record stale.
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let empty_hspecs = inference::HSpecMap::default();
        inference::wasm_to_v(module_name, wasm, &empty, &empty_hspecs)
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

    /// One generated module this suite hands to `coqc`.
    struct GatedModule {
        /// The fixture file for a corpus entry, the module name for a
        /// hand-assembled one. Only a failure message reads it.
        source: &'static str,
        /// The name the translator was given; also the `.v` basename written
        /// into the work directory.
        module: &'static str,
        /// Exactly the text `coqc` is handed — `Qed.` terminators already
        /// rewritten by [`admit_open_proofs`].
        v: String,
    }

    /// The hand-assembled gated modules. Each is a WASM module written directly
    /// rather than compiled from `.inf`, reaching contract shapes Inference
    /// codegen cannot produce.
    ///
    /// A closed enum rather than a list of names: the match in
    /// [`HandbuiltModule::build`] is exhaustive, so a member cannot be added
    /// without a builder and a gate cannot ask for a module that does not
    /// exist. [`Self::ALL`] is the one part still written twice, and forgetting
    /// a member there is caught downstream rather than by the compiler — an
    /// unlisted member is a member [`gated_modules`] does not compile, so
    /// whichever declarations it was written to produce come back unproduced
    /// from [`every_stub_declaration_has_a_producer`].
    #[derive(Clone, Copy)]
    enum HandbuiltModule {
        ForeignSegments,
        ModuleSurface,
        InstructionSurface,
        Obligations,
        Reachability,
    }

    impl HandbuiltModule {
        /// Every member, in gate order.
        const ALL: &'static [Self] = &[
            Self::ForeignSegments,
            Self::ModuleSurface,
            Self::InstructionSurface,
            Self::Obligations,
            Self::Reachability,
        ];

        /// The name the translator is given; also the `.v` basename and the
        /// work-directory label.
        fn module_name(self) -> &'static str {
            match self {
                Self::ForeignSegments => "foreign_segments",
                Self::ModuleSurface => "module_surface",
                Self::InstructionSurface => "instruction_surface",
                Self::Obligations => "handbuilt_obligations",
                Self::Reachability => "handbuilt_reachability",
            }
        }

        /// Builds this member. The only place a hand-assembled [`GatedModule`]
        /// is constructed, so the per-shape gate below and [`gated_modules`]
        /// cannot be looking at different text for the same member.
        fn build(self) -> GatedModule {
            let module = self.module_name();
            let v = match self {
                Self::ForeignSegments => translate_wat(module, FOREIGN_SEGMENTS_WAT),
                Self::ModuleSurface => translate_wat(module, MODULE_SURFACE_WAT),
                Self::InstructionSurface => translate_wat(module, &instruction_surface_wat()),
                Self::Obligations => handbuilt_obligations_v(module),
                Self::Reachability => handbuilt_reachability_v(module),
            };
            GatedModule {
                source: module,
                module,
                v: admit_open_proofs(&v),
            }
        }
    }

    /// Proof-mode `.v` for every corpus fixture: [`CORPUS`] in order, then
    /// [`LINKED_CORPUS`].
    fn corpus_modules() -> Vec<GatedModule> {
        CORPUS
            .iter()
            .map(|&(source, module)| GatedModule {
                source,
                module,
                v: admit_open_proofs(&generate_v(source, module)),
            })
            .chain(
                LINKED_CORPUS
                    .iter()
                    .map(|&(source, module, externals)| GatedModule {
                        source,
                        module,
                        v: admit_open_proofs(&generate_linked_v(source, module, externals)),
                    }),
            )
            .collect()
    }

    /// Every module this suite compiles with `coqc`: the whole corpus plus every
    /// hand-assembled module.
    ///
    /// This is the producer set [`every_stub_declaration_has_a_producer`]
    /// measures the vendored stub against, and "produced" is only worth
    /// anything if the producing module is one `coqc` elaborates. That audit
    /// therefore compiles this whole list itself, rather than trusting the
    /// per-shape gates below to have compiled it — a gate that is deleted,
    /// `#[ignore]`d or short-circuited can no longer leave the audit certifying
    /// coverage nobody checked.
    ///
    /// What the list still relies on the gates for is *membership*: a future
    /// gate that builds its own module instead of taking one from here would be
    /// compiled by nobody but itself. That drift fails the audit rather than
    /// passing it — the module's declarations look unproduced — so the unsafe
    /// direction is closed and the remaining one is merely noisy.
    fn gated_modules() -> Vec<GatedModule> {
        let mut modules = corpus_modules();
        modules.extend(HandbuiltModule::ALL.iter().map(|&m| m.build()));
        modules
    }

    /// The vendored stub's `.v` files, as (namespace directory, module name),
    /// in the dependency order `_CoqProject` fixes: the stub is a two-namespace
    /// tree, `wasm/` (`Wasm.*`, the WASM datatypes) and `wasm_verifier/`
    /// (`WasmVerifier.*`, the assertion language and the proof obligations),
    /// and the second imports the first.
    ///
    /// [`compile_stub`] compiles exactly these files and
    /// [`stub_declarations`] parses exactly these files, so the coverage audit
    /// can never measure itself against a contract the gate does not compile.
    const STUB_MODULES: &[(&str, &str)] = &[
        ("wasm", "bytes"),
        ("wasm", "numerics"),
        ("wasm", "datatypes"),
        ("wasm", "host"),
        ("wasm_verifier", "Assertions"),
        ("wasm_verifier", "Verifier"),
        ("wasm_verifier", "Exists"),
    ];

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

    /// Copies the vendored stub into a private work directory and compiles both
    /// namespaces in `_CoqProject` dependency order, returning that directory.
    ///
    /// `coqc` writes `.vo` files next to their sources, so the read-only repo
    /// tree cannot be compiled in place; `label` additionally keeps two gates
    /// running concurrently in this binary out of each other's directory. On a
    /// failure the directory is deliberately kept so the rejected `.v` and the
    /// compiled stub are available for a manual `coqc` repro; a successful gate
    /// removes it, and the pre-clean below handles a stale leftover.
    fn compile_stub(coqc: &str, label: &str) -> PathBuf {
        let work = std::env::temp_dir().join(format!(
            "inference_rocq_typecheck_{}_{label}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(work.join("wasm")).expect("create work/wasm dir");
        std::fs::create_dir_all(work.join("wasm_verifier")).expect("create work/wasm_verifier dir");
        let src_stub = stub_dir();
        for (dir, module) in STUB_MODULES {
            let rel = format!("{dir}/{module}.v");
            std::fs::copy(
                src_stub.join(dir).join(format!("{module}.v")),
                work.join(&rel),
            )
            .unwrap_or_else(|e| panic!("copy stub {rel}: {e}"));
        }
        for (dir, module) in STUB_MODULES {
            let file = work.join(dir).join(format!("{module}.v"));
            if let Err(log) = coqc_compile(coqc, &work, &file) {
                panic!(
                    "vendored stub failed to compile ({dir}/{module}.v):\n{log}\n\
                     work dir kept for inspection: {}",
                    work.display()
                );
            }
        }
        work
    }

    /// The `module_func` definitions of an emitted `.v`, in the order the
    /// module record lists them — which is the order that fixes every `T_app`
    /// index, so a function's position here *is* the ordinal an obligation
    /// applying it must carry.
    fn module_func_names(v: &str) -> Vec<&str> {
        v.lines()
            .filter_map(|line| {
                line.strip_prefix("Definition ")?
                    .strip_suffix(" : module_func := {|")
            })
            .collect()
    }

    /// One `module_func` definition of an emitted `.v`, from its `Definition`
    /// line to the `|}.` closing it.
    ///
    /// A needle matched against a whole module says only that *something* in it
    /// has that shape, which is the wrong question whenever the claim is about
    /// one function — and a module large enough to be interesting usually has a
    /// second producer of any given constructor.
    fn module_func_body<'v>(v: &'v str, name: &str) -> &'v str {
        let opening = format!("Definition {name} : module_func := {{|");
        let start = v
            .find(&opening)
            .unwrap_or_else(|| panic!("no `module_func` named `{name}` in:\n{v}"));
        let body = &v[start..];
        let end = body
            .find("|}.")
            .unwrap_or_else(|| panic!("the definition of `{name}` is unterminated in:\n{v}"));
        &body[..end]
    }

    /// The floor under [`LINKED_CORPUS`]: every entry must actually reach the
    /// linker and come back with a merged body its obligation applies.
    ///
    /// Without this, deleting the whole block leaves every other gate green —
    /// `REQUIRED_CONSTRUCTS` is measured over the corpus concatenated, and each
    /// needle it lists is already produced by a single-file fixture, so the one
    /// thing these entries exist to cover is the one thing nothing measures.
    ///
    /// The application is checked by *index*, because neither half of the
    /// obvious pair of substrings ties one to the other. A `Definition` named
    /// after the logical module appears as soon as the merge runs at all, which
    /// the fixture's own executable `pub fn` forces regardless of what the
    /// obligation names; and a bare `T_app ` matches any application, which is
    /// exactly why `REQUIRED_CONSTRUCTS` cannot serve as this floor. A fixture
    /// whose spec applies a *local* helper satisfies both and carries zero
    /// linked coverage. The merged body's own ordinal is the one thing only a
    /// linked obligation produces.
    #[test]
    fn linked_corpus_carries_a_merged_body() {
        assert!(
            !LINKED_CORPUS.is_empty(),
            "the linked corpus is the only place an obligation names a body the \
             compiler did not emit; an empty list silently drops that coverage"
        );
        for &(source, module, externals) in LINKED_CORPUS {
            let v = generate_linked_v(source, module, externals);
            let defined = module_func_names(&v);
            for external in externals {
                // The linker names a merged body `<logical module>.<field>`,
                // which `wasm-to-v` sanitizes into the `Definition` name; every
                // body spliced in from this external shares that prefix.
                let prefix = format!("{}_", external.logical_module.replace(['.', ':'], "_"));
                let merged: Vec<usize> = defined
                    .iter()
                    .enumerate()
                    .filter(|(_, name)| name.starts_with(&prefix))
                    .map(|(idx, _)| idx)
                    .collect();
                assert!(
                    !merged.is_empty(),
                    "{source}: no definition came from merging `{}`; the fixture is \
                     not exercising the link. Defined functions: {defined:?}",
                    external.logical_module
                );
                assert!(
                    merged
                        .iter()
                        .any(|idx| v.contains(&format!("T_app {idx} "))),
                    "{source}: merging `{}` produced bodies at record indices {merged:?}, \
                     and no obligation applies any of them — the entry claims linked \
                     coverage it does not have. Defined functions: {defined:?}",
                    external.logical_module
                );
            }
        }
    }

    /// The toolchain entry's own floor: the forms it is credited with elaborating
    /// are the forms it is still emitting.
    ///
    /// [`linked_corpus_carries_a_merged_body`] holds every linked entry to
    /// carrying *a* merged body some obligation applies, which the
    /// Inference-built entry satisfies equally well. What only this entry can
    /// claim is that a foreign compiler chose the instructions, and nothing else
    /// measures that: `REQUIRED_CONSTRUCTS` is a needle list over the corpus
    /// concatenated, so it cannot distinguish a construct this fixture uniquely
    /// produces from the same construct emitted anywhere else.
    ///
    /// Without this, a `rustc` that lowered the clamp with a branch or unrolled
    /// the loop would leave the whole gate green while the entry's stated
    /// coverage quietly became false — and the artifact is regenerated by hand,
    /// so that day arrives with no code change to notice it by.
    ///
    /// The needles are applied *forms* rather than bare constructor names for the
    /// same reason. `BI_loop` alone is produced by any `while`; it is the result
    /// type on it that no Inference lowering emits.
    ///
    /// Three of the four forms are unproducible here and one is not, and the
    /// difference is worth keeping straight rather than letting the stronger
    /// claim cover them all. `BI_select` in any form, a `BI_loop` carrying a
    /// result type, and `BI_cvtop` in either direction have no emitter arm at all
    /// — this compiler never selects, its `while` lowering always writes
    /// `BT_valtype None`, and it emits no width conversion, narrowing sub-`i32`
    /// values with shifts and masks instead. `BI_load T_i32 None (Ma 0%N 2%N)` is
    /// merely *absent from the corpus*: an ordinary `values[i]` emits exactly that
    /// form, so what that needle pins is the merged body's lowering staying put,
    /// not a shape only a foreign compiler can reach.
    ///
    /// The `BI_cvtop` pair is the reason this entry answers the question the
    /// numeric envelope was widened for. The conversions were admitted across the
    /// stub, translator, allow-list and feature gate, and until `mulhi` existed
    /// every module elaborating one was hand-assembled — the corpus could show
    /// the constructors were accepted, never that a real artifact containing them
    /// links and translates.
    ///
    /// Each needle is matched against the body it is about rather than the whole
    /// module, because module-wide the select needle is vacuous: the artifact
    /// carries *two* selects, the clamp's and the `n > 0 ? n : 0` guard opening
    /// the loop. A `rustc` that lowered the clamp with a branch — the drift this
    /// test names first — would leave the guard's select behind and keep a
    /// module-wide needle green.
    ///
    /// The application check is separate from and stricter than the shared floor:
    /// it demands an obligation about **each** merged body rather than any one of
    /// them, because a body nothing applies is elaborated without anything being
    /// claimed about it. Indices are read off the record rather than written down,
    /// so a reordering fails as a missing definition instead of a missing `T_app`.
    #[test]
    fn the_toolchain_entry_keeps_emitting_the_forms_it_is_credited_with() {
        const FIXTURE: &str = "spec_linked_toolchain.inf";

        let &(source, module, externals) = LINKED_CORPUS
            .iter()
            .find(|&&(source, _, _)| source == FIXTURE)
            .unwrap_or_else(|| {
                panic!(
                    "{FIXTURE} must stay a LINKED_CORPUS entry: it is the only module this \
                     gate compiles whose bodies a compiler other than this one selected"
                )
            });
        let v = generate_linked_v(source, module, externals);

        for (function, needle, what) in [
            (
                "rustlib_clamp_add",
                "BI_select None",
                "the clamp's branches were folded into a select, which this compiler's \
                 lowering never emits",
            ),
            (
                "rustlib_sum_n",
                "BI_loop (BT_valtype (Some (T_num T_i32)))",
                "a loop carrying a result type, where a `while` lowered here always \
                 emits `BT_valtype None`",
            ),
            (
                "rustlib_mulhi",
                "BI_cvtop T_i64 CVO_extend T_i32 (Some SX_S)",
                "the widening half of the 64-bit intermediate. This compiler emits no \
                 conversion at all, so without a foreign body the constructor reaches \
                 `coqc` only from a hand-assembled module",
            ),
            (
                "rustlib_mulhi",
                "BI_cvtop T_i32 CVO_wrap T_i64 None",
                "the narrowing half; the two travel together, and a lowering that dropped \
                 either would be computing something else",
            ),
            (
                "rustlib_sum_n",
                "BI_load T_i32 None (Ma 0%N 2%N)",
                "the Tier-B load off the caller's pointer; this form is elaborated by a \
                 hand-built module too, and what it pins here is that the merged body \
                 still reaches memory the way the fixture describes",
            ),
        ] {
            let body = module_func_body(&v, function);
            assert!(
                body.contains(needle),
                "`{function}` in {FIXTURE} no longer emits `{needle}` — {what}. The \
                 artifact was most likely regenerated with a toolchain that lowers it \
                 differently: either update this needle or record that the entry's \
                 coverage changed. That body was:\n{body}"
            );
        }

        let defined = module_func_names(&v);
        for name in ["rustlib_clamp_add", "rustlib_mulhi", "rustlib_sum_n"] {
            let index = defined
                .iter()
                .position(|candidate| *candidate == name)
                .unwrap_or_else(|| panic!("{FIXTURE} must merge `{name}`; it defines {defined:?}"));
            assert!(
                v.contains(&format!("T_app {index} ")),
                "{FIXTURE} merges `{name}` at record index {index} and no obligation \
                 applies it, so its instructions are elaborated while nothing is claimed \
                 about them:\n{v}"
            );
        }
    }

    #[test]
    fn corpus_type_checks_against_vendored_stub() {
        // 1. Generate every corpus module in-process.
        let generated = corpus_modules();

        // 2. Always-on guard: the corpus must keep exercising the proof surface,
        //    independent of whether `coqc` is present on this machine.
        let all: String = generated.iter().map(|m| m.v.as_str()).collect();
        for needle in REQUIRED_CONSTRUCTS {
            assert!(
                all.contains(needle),
                "corpus no longer emits `{needle}`; the coqc gate would stop \
                 covering it — add or fix a fixture in tests/test_data/inf/"
            );
        }

        // 3. The obligation printer's operator arms, each at both widths. This
        //    is the corpus's half of the #401 coverage: `T_binop`/`T_relop` come
        //    from a spec body and nothing else, so a hand-assembled WASM module
        //    cannot stand in for a fixture here the way it can for instructions.
        for (term, family, spellings) in [
            ("T_binop", "Binop_i", OBLIGATION_BINOPS),
            ("T_relop", "Relop_i", OBLIGATION_RELOPS),
        ] {
            for spelling in spellings {
                for width in NUMBER_TYPES {
                    let needle = format!("{term} {width} ({family} {spelling})");
                    assert!(
                        all.contains(&needle),
                        "no corpus fixture puts `{needle}` in an obligation, so `coqc` \
                         never elaborates that arm of the obligation printer and a \
                         rename or arity change in it would ship green — add a `spec` \
                         that produces it to tests/test_data/inf/"
                    );
                }
            }
        }

        // 4. Both data-segment preamble lines are conditional on a data segment,
        //    and Inference codegen emits none, so no corpus module carries one.
        //    Pinning their absence keeps the preamble free of anything a module
        //    does not use, and keeps every committed `.v` byte-identical to the
        //    output it had before byte literals gained a scope requirement and a
        //    private delimiting key.
        for m in &generated {
            for line in [
                "Open Scope byte_scope.",
                "Local Delimit Scope Z_scope with Zst.",
            ] {
                assert!(
                    !m.v.contains(line),
                    "`{}` carries no data segment, so its preamble must not \
                     carry `{line}`; got:\n{}",
                    m.source,
                    m.v
                );
            }
        }

        // 5. The coqc compile is gated: real in CI, skipped locally when absent.
        let Some(coqc) = find_coqc() else {
            eprintln!(
                "skipped: coqc not found (set COQC or put coqc on PATH). \
                 Corpus generated and proof-surface coverage verified; \
                 type-checking against the vendored stub was not run."
            );
            return;
        };

        // 6. Compile the vendored stub once into a private temp dir.
        let work = compile_stub(&coqc, "corpus");

        // 7. Type-check every generated module against the compiled stub.
        for m in &generated {
            let v_path = work.join(format!("{}.v", m.module));
            std::fs::write(&v_path, &m.v).unwrap_or_else(|e| panic!("write {}: {e}", m.source));
            if let Err(log) = coqc_compile(&coqc, &work, &v_path) {
                panic!(
                    "coqc rejected proof-mode output for `{}` against the \
                     vendored Wasm stub:\n{log}\n\
                     work dir kept for inspection: {}",
                    m.source,
                    work.display()
                );
            }
        }

        let _ = std::fs::remove_dir_all(&work);
    }

    /// The WAT for [`foreign_segments_type_check_against_vendored_stub`]: two
    /// `br_table`s (explicit targets with a distinct default, and a default-only
    /// table), all three element modes across both item forms (bare function
    /// indexes and `ref.func` initializer expressions), and active and passive
    /// data segments.
    ///
    /// The passive segment spans both byte spellings and the whole byte range:
    /// the two extremes, so a spelling that only works for the printable middle
    /// fails here, and `0x12`/`0x1f` from the twelve-value gap the contract
    /// declares no hex notation for, so a uniform hex spelling fails here too.
    const FOREIGN_SEGMENTS_WAT: &str = r#"
        (module
          (type (;0;) (func (param i32) (result i32)))
          (table (;0;) 4 4 funcref)
          (memory (;0;) 1)
          (elem (;0;) (i32.const 0) func 0 1)
          (elem (;1;) declare func 1)
          (elem (;2;) funcref (item ref.func 0))
          (data (;0;) (i32.const 0) "hi")
          (data (;1;) "\00\12\1f\ff")
          (func (;0;) (type 0) (param i32) (result i32)
            block
              block
                local.get 0
                br_table 0 1 1
              end
              block
                local.get 0
                br_table 0
              end
            end
            local.get 0)
          (func (;1;) (type 0) (param i32) (result i32)
            local.get 0))
        "#;

    /// A handcrafted foreign module carrying the element, data, and `br_table`
    /// shapes Inference codegen never produces.
    ///
    /// Element segments (in all three modes and both item forms), data segments,
    /// and `br_table` reach the translator only from foreign or statically-linked
    /// `.wasm`, so no `.inf` fixture can drive them into the corpus gate above.
    /// That is how three terms the proof contract has no constructor for stayed
    /// emitted, and how the `byte_scope` notations a data byte is written with
    /// stayed unmodelled on the *stub* side (#346). Byte spelling is the one
    /// place where the stub can fail in both directions — declaring too few
    /// notations rejects a module the backend accepts, declaring too many
    /// accepts one it rejects — so this fixture's data bytes cover the gap in
    /// the contract's own notation block as well as the range extremes. This
    /// gate assembles the constructs directly as WASM and runs the same public
    /// `wasm_to_v` entry the corpus uses.
    ///
    /// `br_table`'s default label deserves its own arm: it is a separate
    /// immediate that the explicit-target list never contains, and a table whose
    /// list is empty (`br_table 0`) is valid WASM carrying nothing else.
    #[test]
    fn foreign_segments_type_check_against_vendored_stub() {
        let module = HandbuiltModule::ForeignSegments.build();
        let v = &module.v;

        // Teeth without `coqc`: the exact terms the contract accepts.
        for needle in [
            "BI_br_table (0%N :: 1%N :: nil) 1%N",
            "BI_br_table nil 0%N",
            "ME_active 0%N",
            "ME_declarative",
            "ME_passive",
            "(BI_ref_func 0%N :: nil)",
            "(BI_ref_func 1%N :: nil)",
            "moddata_init := #68 :: #69 :: nil",
            "moddata_init := #00 :: (encode 18%Zst) :: (encode 31%Zst) :: #FF :: nil",
            "MD_active 0%N",
            "MD_passive",
            // The byte notations parse only inside `byte_scope`, so a module
            // carrying data segments opens it; `coqc` below is what proves the
            // line is load-bearing rather than decorative.
            "Open Scope byte_scope.\n",
            // The `encode` applications above spell their argument with a
            // private key, because mathcomp's algebra library delimits its own
            // `int_scope` with `Z`. `coqc` below proves the claim and the
            // spelling elaborate under vanilla Rocq; surviving the mathcomp
            // rebinding is what the key exists for.
            "Local Delimit Scope Z_scope with Zst.\n",
        ] {
            assert!(
                v.contains(needle),
                "the foreign module must emit `{needle}`; got:\n{v}"
            );
        }

        // Spellings the contract cannot elaborate: an element mode written into
        // the field that holds initializer expressions, `BI_br_table` applied
        // to fewer arguments than it takes, and the two hex byte notations from
        // the gap in the contract's notation block. The gap notations are the
        // regression this fixture's `\12`/`\1f` bytes exist for: they look like
        // every other byte spelling and parse nowhere.
        for retired in [
            "ME_functions",
            "ME_declared",
            "BI_br_table ::",
            "#12",
            "#1F",
        ] {
            assert!(
                !v.contains(retired),
                "`{retired}` is not a term the proof contract accepts; got:\n{v}"
            );
        }

        type_check_with_coqc(
            &module,
            "Foreign module generated and its element, data and `br_table` \
             shapes verified",
        );
    }

    /// Assembles a hand-written WAT module and drives it through the same public
    /// `wasm_to_v` entry the corpus uses, with no explicit spec or obligation
    /// maps.
    ///
    /// Building the module from text rather than from codegen is what makes the
    /// hand-assembled gates possible at all: it reaches constructs Inference
    /// never emits, and it produces a `.wasm` with no embedded
    /// `inference.hspecs` section for an explicit map to contradict.
    fn translate_wat(module_name: &str, wat: &str) -> String {
        let bytes = wat::parse_str(wat)
            .unwrap_or_else(|e| panic!("`{module_name}` fixture assembles: {e}"));
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let empty_hspecs = inference::HSpecMap::default();
        inference::wasm_to_v(module_name, &bytes, &empty, &empty_hspecs)
            .unwrap_or_else(|e| panic!("wasm_to_v failed for `{module_name}`: {e}"))
    }

    /// Compiles one gated module against a freshly built copy of the vendored
    /// stub, or skips with a message when `coqc` is unavailable.
    ///
    /// Taking a whole [`GatedModule`] rather than a loose name and string keeps
    /// a gate pointed at the same text [`gated_modules`] collects: the only
    /// hand-assembled `GatedModule` builder is [`HandbuiltModule::build`], so a
    /// gate reaches its module through a variant rather than by assembling one
    /// of its own. The module name doubles as the private work directory's
    /// label, so concurrently running gates in this binary never share one, and
    /// `covered` completes the skip line so a local run says exactly which half
    /// of the gate did and did not happen.
    fn type_check_with_coqc(module: &GatedModule, covered: &str) {
        let Some(coqc) = find_coqc() else {
            eprintln!(
                "skipped: coqc not found (set COQC or put coqc on PATH). \
                 {covered}; type-checking against the vendored stub was not run."
            );
            return;
        };
        let work = compile_stub(&coqc, module.module);
        let v_path = work.join(format!("{}.v", module.module));
        std::fs::write(&v_path, &module.v)
            .unwrap_or_else(|e| panic!("write {}.v: {e}", module.module));
        if let Err(log) = coqc_compile(&coqc, &work, &v_path) {
            panic!(
                "coqc rejected `{}`'s `.v` against the vendored Wasm stub:\n{log}\n\
                 work dir kept for inspection: {}",
                module.module,
                work.display()
            );
        }
        let _ = std::fs::remove_dir_all(&work);
    }

    /// A handcrafted module carrying the import, export, global and `start`
    /// surface Inference codegen never produces.
    ///
    /// Inference emits no `start` section and no exported table, memory or
    /// global. It does emit an import section — one entry per bound `extern
    /// fn`, `wasm_codegen_emit_import_section` in
    /// `core/wasm-codegen/src/compiler.rs` — but none of those survive to the
    /// translator: the static-merge linker is fail-closed on imports
    /// (`LinkError::UnsatisfiedImport` in `core/wasm-linker/src/lib.rs`), and
    /// `infc` links before it translates and aborts on a link failure, so `-v`
    /// is only ever handed a module whose imports are already merged away. That
    /// left `MID_func`/`MID_table`/`MID_mem`/`MID_global`, `MED_table`/
    /// `MED_mem`/`MED_global`, `MUT_const` and `modstart_func` with no producer
    /// anywhere in the gate and `coqc` never elaborated one (#401).
    /// `MID_table` is what that costs: it takes a whole `table_type` while its
    /// neighbour `MID_mem` takes a bare `limits`, the emitter applied it to a
    /// bare `limits` too, and nothing type-checked the result.
    ///
    /// Both mutabilities appear at both an import and a definition, because
    /// `MUT_const`/`MUT_var` are chosen in two unrelated places — the global
    /// section and the import descriptor.
    ///
    /// The index arithmetic is the other half. An imported function occupies
    /// index 0, so the module's one defined function is index 1 everywhere it is
    /// named: the `start` section and the function export both carry the shifted
    /// value, while `T_app` obligations use the unshifted `mod_funcs` position.
    /// Confusing the two numbering schemes is a live hazard the corpus cannot
    /// expose, since every corpus module has zero imports and the two coincide.
    /// Table, memory and global export indices are not remapped at all — they
    /// arrive already counting the imports — which is why the exported *defined*
    /// const global is `MED_global 2%N` and not `0%N`.
    #[test]
    fn module_surface_type_checks_against_vendored_stub() {
        let module = HandbuiltModule::ModuleSurface.build();
        let v = &module.v;

        for needle in [
            // Every import descriptor. `MID_func` carries a *type* index, the
            // other three carry the described type itself.
            r#"Mi "env" "imported_fn" (MID_func 1%N)"#,
            "MID_table {|tt_limits := {|lim_min := 1%N; lim_max := None|}; \
             tt_elem_type := T_funcref|}",
            "MID_mem {|lim_min := 1%N; lim_max := Some(2%N)|}",
            "MID_global {|tg_mut := MUT_const; tg_t := T_num T_i32|}",
            "MID_global {|tg_mut := MUT_var; tg_t := T_num T_i64|}",
            // Every export descriptor, at the indices described above.
            r#"Me "exported_table" (MED_table 0%N)"#,
            r#"Me "exported_mem" (MED_mem 0%N)"#,
            r#"Me "exported_const_global" (MED_global 2%N)"#,
            r#"Me "exported_mut_global" (MED_global 3%N)"#,
            r#"Me "exported_fn" (MED_func 1%N)"#,
            "mod_start := Some {|modstart_func := 1%N|}",
            // Defined globals, both mutabilities. The initializer list is
            // formatted raggedly, so only the constructor prefix is pinned.
            "Mg MUT_const (T_num T_i32) (",
            "Mg MUT_var (T_num T_i64) (",
            // A call whose callee is an import, the operand form the corpus's
            // import-free modules cannot produce.
            "BI_call 0%N ::",
        ] {
            assert!(
                v.contains(needle),
                "the module-surface fixture must emit `{needle}`; got:\n{v}"
            );
        }

        for wrong in [
            // The two descriptors' payloads swapped. `MID_table` applied to a
            // bare `limits` is the defect this fixture was written for; the
            // mirror is what a mechanical "fix" in the other direction looks
            // like.
            "MID_table {|lim_min",
            "MID_mem {|tt_limits",
            // A defined global renumbered from zero, as if the export index
            // shared the function remap. Both exported globals are defined, so
            // neither may name an imported slot.
            "MED_global 0%N",
            "MED_global 1%N",
            // `start` and the function export renumbered into `mod_funcs`
            // space, which is the numbering `T_app` uses and these must not.
            "modstart_func := 0%N",
            "MED_func 0%N",
        ] {
            assert!(
                !v.contains(wrong),
                "`{wrong}` is not a term the proof contract accepts; got:\n{v}"
            );
        }

        type_check_with_coqc(
            &module,
            "Module-surface fixture generated and its import, export, global \
             and start shapes verified",
        );
    }

    /// The WAT for [`module_surface_type_checks_against_vendored_stub`]: every
    /// import and export descriptor, both global mutabilities at both an import
    /// and a definition, and a `start` section.
    const MODULE_SURFACE_WAT: &str = r#"
        (module
          (type $void (func))
          (type $i2i (func (param i32) (result i32)))
          (import "env" "imported_fn" (func $imported_fn (type $i2i)))
          (import "env" "imported_table" (table $imported_table 1 funcref))
          (import "env" "imported_mem" (memory $imported_mem 1 2))
          (import "env" "imported_const_global" (global $imported_const_global i32))
          (import "env" "imported_mut_global" (global $imported_mut_global (mut i64)))
          (global $const_global i32 (i32.const 7))
          (global $mut_global (mut i64) (i64.const 8))
          (export "exported_table" (table $imported_table))
          (export "exported_mem" (memory $imported_mem))
          (export "exported_const_global" (global $const_global))
          (export "exported_mut_global" (global $mut_global))
          (export "exported_fn" (func $entry))
          (start $entry)
          (func $entry (type $void)
            i32.const 1
            call $imported_fn
            drop
            nop))
        "#;

    /// A handcrafted module carrying the instruction surface Inference codegen
    /// never reaches, plus the complete integer operator matrix.
    ///
    /// Inference has no rotate operator, no reference types, no `select`, no
    /// table, no `unreachable`, and its memory access is confined to the shapes
    /// its own lowering emits — so roughly half of the translator's per-operator
    /// match arms had no producer and were never elaborated (#401). A WAT module
    /// reaches all of them, and reaches them through the same public entry the
    /// corpus uses.
    ///
    /// The operator matrix is generated from [`INTEGER_BINOPS`] and
    /// [`INTEGER_RELOPS`] rather than written out: the same table supplies the
    /// mnemonic that produces each instruction and the spelling that must come
    /// back, so no arm can be listed as covered without a producer behind it.
    /// Both widths are emitted because the translator matches `i32` and `i64`
    /// in separate arms with separately-written strings.
    ///
    /// The load and store forms are the other bulk of it. Twelve loads and seven
    /// stores each pair a storage width with — on the loads — a sign extension,
    /// and every one is a distinct arm printing a distinct `Tp_i8`/`Tp_i16`/
    /// `Tp_i32` and `SX_S`/`SX_U` combination that only a hand-written module
    /// can request.
    ///
    /// The eight width-changing operators are here for the same reason and are
    /// the module's only producer of `Unop_extend`, `cvtop` and `BI_cvtop`:
    /// Inference codegen narrows sub-i32 values with shifts and masks and emits
    /// no conversion at all, so nothing in the corpus reaches them. They also
    /// split across two constructors along a line the WASM mnemonics obscure —
    /// the five `extendN_s` are unops, the three `wrap`/`extend_i32` are cvtops —
    /// which is exactly the kind of shape only elaboration settles.
    #[test]
    fn instruction_surface_type_checks_against_vendored_stub() {
        let module = HandbuiltModule::InstructionSurface.build();
        let v = &module.v;

        // The complete operator matrix, both emitters' widths.
        for (instruction, family, spellings) in [
            ("BI_binop", "Binop_i", INTEGER_BINOPS),
            ("BI_relop", "Relop_i", INTEGER_RELOPS),
        ] {
            for (mnemonic, spelling) in spellings {
                for width in NUMBER_TYPES {
                    let needle = format!("{instruction} {width} ({family} {spelling})");
                    assert!(
                        v.contains(&needle),
                        "the generated `{mnemonic}` at {width} must translate to \
                         `{needle}`; got:\n{v}"
                    );
                }
            }
        }

        for needle in [
            // Instructions with no Inference source form at all.
            "BI_unreachable ::",
            "BI_select None ::",
            "BI_ref_is_null ::",
            "BI_ref_func 0%N ::",
            // A block whose type is a type-section index rather than a single
            // value type. Only a multi-value or parameterized signature forces
            // it; anything spellable as a valtype is collapsed to `BT_valtype`.
            "BI_block (BT_id 1%N)",
            "BI_nop ::",
            "BI_drop ::",
            "BI_return ::",
            // Locals carry a name-section annotation when the module names
            // them, so the comment is part of the emitted text.
            "BI_local_get 0%N (*narrow*) ::",
            "BI_local_set 2%N (*spill*) ::",
            "BI_local_tee 2%N (*spill*) ::",
            "BI_global_get 0%N ::",
            "BI_global_set 0%N ::",
            // All twelve integer loads: the two full-width forms, then every
            // narrow width at both sign extensions.
            "BI_load T_i32 None (Ma 0%N 2%N)",
            "BI_load T_i64 None (Ma 0%N 3%N)",
            "BI_load T_i32 (Some (Tp_i8, SX_S)) (Ma 0%N 0%N)",
            "BI_load T_i32 (Some (Tp_i8, SX_U)) (Ma 0%N 0%N)",
            "BI_load T_i32 (Some (Tp_i16, SX_S)) (Ma 0%N 1%N)",
            "BI_load T_i32 (Some (Tp_i16, SX_U)) (Ma 0%N 1%N)",
            "BI_load T_i64 (Some (Tp_i8, SX_S)) (Ma 0%N 0%N)",
            "BI_load T_i64 (Some (Tp_i8, SX_U)) (Ma 0%N 0%N)",
            "BI_load T_i64 (Some (Tp_i16, SX_S)) (Ma 0%N 1%N)",
            "BI_load T_i64 (Some (Tp_i16, SX_U)) (Ma 0%N 1%N)",
            "BI_load T_i64 (Some (Tp_i32, SX_S)) (Ma 0%N 2%N)",
            "BI_load T_i64 (Some (Tp_i32, SX_U)) (Ma 0%N 2%N)",
            // All seven stores. A store has no sign extension, so its narrow
            // forms carry a bare `Tp_*` where the load carries a pair — two
            // shapes one arm could easily be written into.
            "BI_store T_i32 None (Ma 0%N 2%N)",
            "BI_store T_i64 None (Ma 0%N 3%N)",
            "BI_store T_i32 (Some Tp_i8) (Ma 0%N 0%N)",
            "BI_store T_i32 (Some Tp_i16) (Ma 0%N 1%N)",
            "BI_store T_i64 (Some Tp_i8) (Ma 0%N 0%N)",
            "BI_store T_i64 (Some Tp_i16) (Ma 0%N 1%N)",
            "BI_store T_i64 (Some Tp_i32) (Ma 0%N 2%N)",
            // Unary and test operators at both widths.
            "BI_unop T_i32 (Unop_i UOI_clz)",
            "BI_unop T_i32 (Unop_i UOI_ctz)",
            "BI_unop T_i32 (Unop_i UOI_popcnt)",
            "BI_unop T_i64 (Unop_i UOI_clz)",
            "BI_unop T_i64 (Unop_i UOI_ctz)",
            "BI_unop T_i64 (Unop_i UOI_popcnt)",
            "BI_testop T_i32 TO_eqz",
            "BI_testop T_i64 TO_eqz",
            // Sign extension. The contract spells it as a unop carrying a bare
            // `N`, not as a conversion — the same `BI_unop` constructor as
            // `clz`/`ctz`/`popcnt` above, with a different operator family. The
            // argument is the source width in BITS; see
            // `sign_extension_widths_are_bit_counts_not_byte_counts`.
            "BI_unop T_i32 (Unop_extend 8%N)",
            "BI_unop T_i32 (Unop_extend 16%N)",
            "BI_unop T_i64 (Unop_extend 8%N)",
            "BI_unop T_i64 (Unop_extend 16%N)",
            "BI_unop T_i64 (Unop_extend 32%N)",
            // The three integer-to-integer conversions, and the only three
            // well-typed `BI_cvtop` instances the contract has: `cvtop_valid`
            // admits `CVO_wrap` at `(i32, i64, None)` and `CVO_extend` at
            // `(i64, i32, Some sx)` and nothing else.
            "BI_cvtop T_i32 CVO_wrap T_i64 None",
            "BI_cvtop T_i64 CVO_extend T_i32 (Some SX_S)",
            "BI_cvtop T_i64 CVO_extend T_i32 (Some SX_U)",
            // Memory operators, including the bulk forms and the passive
            // segment they read from.
            "BI_memory_size ::",
            "BI_memory_grow ::",
            "BI_memory_copy ::",
            "BI_memory_fill ::",
            "BI_memory_init 1%N ::",
            "BI_data_drop 1%N ::",
            "MD_active 0%N",
            "MD_passive",
            // Table operators against both element types.
            "BI_table_get 0%N ::",
            "BI_table_set 0%N ::",
            "BI_table_fill 0%N ::",
            "BI_table_grow 0%N ::",
            "BI_table_size 0%N ::",
            "BI_table_size 1%N ::",
            // `call_indirect` takes two immediates, the type and the table.
            "BI_call_indirect 1%N 0%N ::",
            // Reference types in a signature, a table type and a global type —
            // the three places `T_ref` is written, by three different arms.
            "Tf (T_ref T_funcref :: T_ref T_externref :: nil) (nil)",
            "Mt {|lim_min := 2%N; lim_max := Some(2%N)|} T_funcref",
            "Mt {|lim_min := 1%N; lim_max := None|} T_externref",
            "Mg MUT_const (T_ref T_funcref) (",
        ] {
            assert!(
                v.contains(needle),
                "the instruction-surface fixture must emit `{needle}`; got:\n{v}"
            );
        }

        for wrong in [
            // `BI_select` takes an `option (list value_type)`; dropping the
            // immediate is exactly the #230 arity class.
            "BI_select ::",
            // The test operator is `TO_eqz`, alone among the operator families
            // in not carrying an `I`. Its three siblings are `BOI_`/`ROI_`/
            // `UOI_`-prefixed, which is what makes the typo plausible.
            "TOI_eqz",
            // Width dropped from the operators that take one.
            "BI_testop TO_eqz",
            "BI_unop (Unop_i",
            // `call_indirect` reduced to a single immediate.
            "BI_call_indirect 1%N ::",
            // A load's storage width and sign extension written as separate
            // arguments instead of the pair the contract takes.
            "BI_load T_i32 (Tp_i8",
            "BI_load T_i64 (Tp_i32",
            // A store given the load's pair shape.
            "BI_store T_i32 (Some (Tp_i8",
            // Sign extension written as a conversion — the misclassification
            // that grouped these five with `BI_cvtop` in the first place. Both
            // spellings below are what that mistake produces: the whole
            // instruction as a cvtop, and the operator wrapped in `Unop_i`
            // (which takes a `unop_i`, not an `N`).
            "BI_cvtop T_i32 CVO_extend T_i32",
            "Unop_i (Unop_extend",
            // `BI_cvtop` with its two number types collapsed to one, the arity
            // slip a four-argument constructor invites. `cvtop_valid` rejects
            // both, and the source type is what distinguishes wrap from extend.
            "BI_cvtop T_i32 CVO_wrap None",
            "BI_cvtop T_i64 CVO_extend (Some",
            // `CVO_extend` without its sign, and `CVO_wrap` with one. The
            // contract requires `sx` on exactly one of the two.
            "CVO_extend T_i32 None",
            "CVO_wrap T_i64 (Some",
        ] {
            assert!(
                !v.contains(wrong),
                "`{wrong}` is not a term the proof contract accepts; got:\n{v}"
            );
        }

        type_check_with_coqc(
            &module,
            "Instruction-surface fixture generated and its operator, memory, \
             table and reference shapes verified",
        );
    }

    /// `Unop_extend`'s argument is the source width in **bits**, and this is the
    /// only test that can say so.
    ///
    /// **Do not delete this as redundant with the gate above.** That was checked,
    /// not assumed: emitting `Unop_extend 1%N` was measured against this repo's
    /// `coqc`, and the resulting `.v` **compiled clean**. Only the byte
    /// comparison below caught it. The gate is structurally incapable of
    /// catching an argument-*value* error in this constructor — it proves a term
    /// elaborates, never that the term means what the instruction means —
    /// so these two literal comparisons (here and
    /// `sign_extension_operators_translate_with_bit_widths` in
    /// `core/wasm-to-v/src/lib.rs`) are the entire guard.
    ///
    /// `Unop_extend 1` type-checks just as
    /// well as `Unop_extend 8` — the model's `unop_type_agree` ignores the
    /// argument entirely — while its `app_unop` divides the argument by eight
    /// before extending, so a byte count denotes a zero-bit extension: the
    /// constant zero, for every input, at every one of the five opcodes. Every
    /// obligation written over such a body is provable and false.
    ///
    /// So the convention is pinned by byte comparison, in both directions: the
    /// bit spellings must be present, and the byte spellings they would collapse
    /// to must be absent. Absence is the load-bearing half — a wrong constant is
    /// a well-formed term no amount of elaboration objects to.
    #[test]
    fn sign_extension_widths_are_bit_counts_not_byte_counts() {
        let v = HandbuiltModule::InstructionSurface.build().v;

        for bits in ["Unop_extend 8%N", "Unop_extend 16%N", "Unop_extend 32%N"] {
            assert!(
                v.contains(bits),
                "the instruction-surface fixture must emit `{bits}`; got:\n{v}"
            );
        }
        for bytes in ["Unop_extend 1%N", "Unop_extend 2%N", "Unop_extend 4%N"] {
            assert!(
                !v.contains(bytes),
                "`{bytes}` is a byte count where the proof contract takes a bit \
                 width. It elaborates, satisfies the model's typing side \
                 condition, and denotes a constant-zero extension — so no `coqc` \
                 gate can catch it and this assertion is the only guard:\n{v}"
            );
        }
    }

    /// The WAT for [`instruction_surface_type_checks_against_vendored_stub`].
    ///
    /// `$ops` collects the instructions that need surrounding module state — a
    /// memory, two tables of different element types, a mutable global, an
    /// active and a passive data segment. `$arith` is generated from the
    /// operator tables. `$identity` exists to be the target of `ref.func`,
    /// `call` and `call_indirect`, and the element segment declares it so
    /// `ref.func` validates.
    fn instruction_surface_wat() -> String {
        let mut arith = String::new();
        for (width, operand) in [("i32", "local.get $narrow"), ("i64", "local.get $wide")] {
            for (mnemonic, _) in INTEGER_BINOPS.iter().chain(INTEGER_RELOPS.iter()) {
                arith.push_str(&format!("{operand}\n{operand}\n{width}.{mnemonic}\ndrop\n"));
            }
        }
        format!(
            r#"
            (module
              (type $void (func))
              (type $i2i (func (param i32) (result i32)))
              (type $refs (func (param funcref externref)))
              (table $funcs 2 2 funcref)
              (table $externs 1 externref)
              (memory 1 4)
              (global $counter (mut i32) (i32.const 0))
              (global $fnref funcref (ref.func $identity))
              (data $active (i32.const 0) "hi")
              (data $passive "\00\ff")
              (elem (i32.const 0) func $identity)

              (func $identity (type $i2i)
                local.get 0)

              (func $ref_params (type $refs)
                local.get 0
                ref.is_null
                drop)

              (func $arith (param $narrow i32) (param $wide i64)
                {arith})

              (func $ops (param $narrow i32) (result i32) (local $wide i64) (local $spill i32)
                nop
                i32.const 1
                drop
                local.get $narrow
                i32.const 2
                local.get $narrow
                select
                local.set $spill
                local.get $narrow
                local.tee $spill
                drop

                local.get $narrow
                block (type $i2i)
                end
                drop

                global.get $counter
                global.set $counter

                i32.const 0
                i32.load
                drop
                i32.const 0
                i64.load
                drop
                i32.const 0
                i32.load8_s
                drop
                i32.const 0
                i32.load8_u
                drop
                i32.const 0
                i32.load16_s
                drop
                i32.const 0
                i32.load16_u
                drop
                i32.const 0
                i64.load8_s
                drop
                i32.const 0
                i64.load8_u
                drop
                i32.const 0
                i64.load16_s
                drop
                i32.const 0
                i64.load16_u
                drop
                i32.const 0
                i64.load32_s
                drop
                i32.const 0
                i64.load32_u
                drop

                i32.const 0
                i32.const 1
                i32.store
                i32.const 0
                i64.const 1
                i64.store
                i32.const 0
                i32.const 1
                i32.store8
                i32.const 0
                i32.const 1
                i32.store16
                i32.const 0
                i64.const 1
                i64.store8
                i32.const 0
                i64.const 1
                i64.store16
                i32.const 0
                i64.const 1
                i64.store32

                local.get $narrow
                i32.clz
                drop
                local.get $narrow
                i32.ctz
                drop
                local.get $narrow
                i32.popcnt
                drop
                local.get $wide
                i64.clz
                drop
                local.get $wide
                i64.ctz
                drop
                local.get $wide
                i64.popcnt
                drop
                local.get $narrow
                i32.eqz
                drop
                local.get $wide
                i64.eqz
                drop

                local.get $narrow
                i32.extend8_s
                drop
                local.get $narrow
                i32.extend16_s
                drop
                local.get $wide
                i64.extend8_s
                drop
                local.get $wide
                i64.extend16_s
                drop
                local.get $wide
                i64.extend32_s
                drop

                local.get $wide
                i32.wrap_i64
                drop
                local.get $narrow
                i64.extend_i32_s
                drop
                local.get $narrow
                i64.extend_i32_u
                drop

                memory.size
                drop
                i32.const 1
                memory.grow
                drop
                i32.const 0
                i32.const 0
                i32.const 1
                memory.copy
                i32.const 0
                i32.const 0
                i32.const 1
                memory.fill
                i32.const 0
                i32.const 0
                i32.const 1
                memory.init $passive
                data.drop $passive

                i32.const 0
                table.get $funcs
                drop
                i32.const 0
                ref.func $identity
                table.set $funcs
                i32.const 0
                ref.func $identity
                i32.const 0
                table.fill $funcs
                ref.func $identity
                i32.const 0
                table.grow $funcs
                drop
                table.size $funcs
                drop
                table.size $externs
                drop

                local.get $narrow
                i32.const 0
                call_indirect $funcs (type $i2i)
                drop
                local.get $narrow
                call $identity
                drop

                local.get $narrow
                if
                  local.get $narrow
                  return
                end
                block
                  unreachable
                end
                local.get $narrow))
            "#
        )
    }

    /// Translates a bare one-function module under an explicit obligation map
    /// carrying the four `hassert` arms the corpus cannot put in front of
    /// `coqc`.
    ///
    /// The applied symbol resolves through the WASM name section against the raw
    /// unsanitized function name, which is why the fixture names its function
    /// with a `$` and why the module itself must stay anonymous — a name-section
    /// module name would override the module name passed in.
    fn handbuilt_obligations_v(module_name: &str) -> String {
        use inference_hassert::{HAssert, HFnRef, HSpecEntry, HTerm, SpecKind};

        let bytes = wat::parse_str("(module (func $probe (param i32) (result i32) local.get 0))")
            .expect("obligation-probe fixture assembles");

        let probe = || HFnRef("probe".to_string());
        // The raw variants, not the `HAssert::and`/`or`/`ex` smart
        // constructors: those absorb `HA_true` and would collapse the tree
        // before it reached the printer. That absorption is exactly why the
        // `HA_true` conjunct has to be assembled here at all — a translated
        // obligation reaches ⊤ only by collapsing entirely, and one that does is
        // rejected as vacuous rather than emitted.
        //
        // `HA_not (HA_false)` rather than a bare `HA_false` keeps the assembled
        // obligation a tautology instead of a contradiction, and the `HA_ex`
        // binds the de Bruijn index the two applications read.
        let body = HAssert::Ex(Box::new(HAssert::And(
            Box::new(HAssert::And(
                Box::new(HAssert::True),
                Box::new(HAssert::Not(Box::new(HAssert::False))),
            )),
            Box::new(HAssert::And(
                Box::new(HAssert::Defined(HTerm::App(probe(), vec![HTerm::LVar(0)]))),
                Box::new(HAssert::AppOk(probe(), vec![HTerm::LVar(0)])),
            )),
        )));

        let mut spec_funcs: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        spec_funcs.insert("Probe".to_string(), Vec::new());
        let mut hspecs = inference::HSpecMap::default();
        hspecs.insert(
            "Probe".to_string(),
            vec![HSpecEntry::new(probe(), body, SpecKind::Forall)],
        );

        inference::wasm_to_v(module_name, &bytes, &spec_funcs, &hspecs)
            .unwrap_or_else(|e| panic!("wasm_to_v failed for the obligation probe: {e}"))
    }

    /// A hand-built obligation map carrying the four `hassert` arms the corpus
    /// cannot put in front of `coqc`.
    ///
    /// `HA_true`, `HA_false`, `HA_defined` and `HA_app_ok` are live arms of
    /// `core/wasm-to-v/src/hassert_print.rs` that no corpus fixture elaborates,
    /// for three different reasons. `HA_false` and `HA_defined` have no upstream
    /// producer at all: the codegen pass that lowers a `spec` body into the
    /// `hassert` IR has no path that builds either. `HA_app_ok` does have one —
    /// a bare statement call in a spec body lowers to it — but no fixture writes
    /// that shape. And `HA_true` is unreachable by construction now that a
    /// specification function whose obligation collapses to ⊤ is rejected
    /// instead of emitted: the ⊤-absorbing smart constructors keep ⊤ out of
    /// every proper subterm, and the vacuity check keeps it out of the root.
    /// All four arms were unelaborated (#401). Handing `wasm_to_v` an explicit
    /// [`inference::HSpecMap`] reaches the printer directly, without a source
    /// program.
    ///
    /// Two constraints make that possible. The module must come from WAT rather
    /// than codegen, because a `.wasm` carrying its own `inference.hspecs`
    /// section is compared against the explicit map and a disagreement is an
    /// error — with no section present the map wins outright. And every key of
    /// the obligation map must also be a key of the spec-index map, or the
    /// translator rejects a spec that carries obligations while being absent
    /// from `inference.spec_funcs`; an empty index list satisfies that.
    #[test]
    fn handbuilt_obligations_type_check_against_vendored_stub() {
        let module = HandbuiltModule::Obligations.build();
        let v = &module.v;

        for needle in [
            // The applied form, not a bare `HA_true`: it is what proves the raw
            // variant survived to the printer rather than being absorbed away
            // by a smart constructor slipped into the fixture.
            "HA_and (HA_true)",
            "HA_not (HA_false)",
            // Both applications resolve the symbol to the function's `mod_funcs`
            // index. `HA_defined` takes a term, `HA_app_ok` takes the index and
            // the argument list directly — two different shapes around the same
            // application, which is what makes writing one into the other's arm
            // easy and worth type-checking.
            "HA_defined (T_app 0 ((T_lvar 0) :: nil))",
            "HA_app_ok 0 ((T_lvar 0) :: nil)",
            "Definition handbuilt_obligations__Probe_specs : list hassert",
            "ValidSpec handbuilt_obligations handbuilt_obligations__Probe_specs",
        ] {
            assert!(
                v.contains(needle),
                "the obligation probe must emit `{needle}`; got:\n{v}"
            );
        }

        for wrong in [
            // The unresolved symbol leaking into the emitted term instead of the
            // index it resolves to.
            "HA_app_ok probe",
            "T_app probe",
            // The two arms' shapes swapped: `HA_defined` given an index, or
            // `HA_app_ok` given a term.
            "HA_defined 0",
            "HA_app_ok (T_app",
        ] {
            assert!(
                !v.contains(wrong),
                "`{wrong}` is not a term the proof contract accepts; got:\n{v}"
            );
        }

        type_check_with_coqc(
            &module,
            "Obligation probe generated and its `HA_true`, `HA_false`, \
             `HA_defined` and `HA_app_ok` shapes verified",
        );
    }

    /// A hand-built reachability obligation map carrying both kinds at once,
    /// with explicitly chosen indices, arities and visible-locs lists.
    ///
    /// The corpus's `exists`/`unique` fixtures reach the same grammar through
    /// the real pipeline; this map exists to pin the *translator's* half in
    /// isolation — the record literal's `%N`/`%nat` scoping, the partition
    /// lists, the kind-selected theorems, and the conditional `Exists`
    /// preamble import — against inputs no codegen accident can drift.
    ///
    /// The obligation symbols are spelled in the producer's spec-folded form
    /// (`Probe.ex_probe`): the name section carries the bare function name and
    /// spec membership travels in `inference.spec_funcs`, so this also pins
    /// the translator's qualifier-stripping resolution against exactly the
    /// symbols codegen writes.
    fn handbuilt_reachability_v(module_name: &str) -> String {
        use inference_hassert::{HAssert, HFnRef, HSpecEntry, HTerm, ReachMeta, SpecKind};

        let bytes = wat::parse_str(
            "(module (func $ex_probe (param i32 i32)) (func $uq_probe (param i32) (local i32)))",
        )
        .expect("reachability-probe fixture assembles");

        let mut spec_funcs: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        spec_funcs.insert("Probe".to_string(), vec![0, 1]);
        let mut hspecs = inference::HSpecMap::default();
        hspecs.insert(
            "Probe".to_string(),
            vec![
                HSpecEntry::new(
                    HFnRef("Probe.ex_probe".to_string()),
                    HAssert::TermEq(HTerm::Local(1), HTerm::Local(0)),
                    SpecKind::Exists(ReachMeta {
                        entry_arity: 1,
                        visible_locs: vec![0, 1],
                    }),
                ),
                HSpecEntry::new(
                    HFnRef("Probe.uq_probe".to_string()),
                    HAssert::Defined(HTerm::Local(0)),
                    SpecKind::Unique(ReachMeta {
                        entry_arity: 0,
                        visible_locs: vec![0],
                    }),
                ),
            ],
        );

        inference::wasm_to_v(module_name, &bytes, &spec_funcs, &hspecs)
            .unwrap_or_else(|e| panic!("wasm_to_v failed for the reachability probe: {e}"))
    }

    /// The hand-built reachability map pins the kind-selected emission
    /// grammar: the `reachability_spec` record literal (with its `%N`/`%nat`
    /// scopes), the per-partition gathering lists, the `ValidExistsSpec`/
    /// `ValidUniqueSpec` theorems, the conditional `Exists` preamble import,
    /// and the retained bodies in the module record.
    #[test]
    fn handbuilt_reachability_obligations_pin_the_kind_selected_grammar() {
        let module = HandbuiltModule::Reachability.build();
        let v = &module.v;

        for needle in [
            "From WasmVerifier Require Import Assertions Verifier Exists.\n",
            "Definition handbuilt_reachability__Probe_exspec1 : reachability_spec :=",
            "reach_func := 0%N; reach_entry_arity := 1%nat",
            "reach_visible_locs := (0%N :: 1%N :: nil); \
             reach_payload := term_eq (T_local 1%N) (T_local 0%N)",
            "Definition handbuilt_reachability__Probe_ex_specs : list reachability_spec := \
             (handbuilt_reachability__Probe_exspec1 :: nil).",
            "Theorem valid_exists_handbuilt_reachability__Probe : ValidExistsSpec \
             handbuilt_reachability handbuilt_reachability__Probe_ex_specs.",
            "Definition handbuilt_reachability__Probe_uqspec1 : reachability_spec :=",
            "reach_func := 1%N; reach_entry_arity := 0%nat",
            "reach_visible_locs := (0%N :: nil); reach_payload := HA_defined (T_local 0%N)",
            "Definition handbuilt_reachability__Probe_uq_specs : list reachability_spec := \
             (handbuilt_reachability__Probe_uqspec1 :: nil).",
            "Theorem valid_unique_handbuilt_reachability__Probe : ValidUniqueSpec \
             handbuilt_reachability handbuilt_reachability__Probe_uq_specs.",
            // The universal grammar stays, with the explicitly-typed empty
            // list: no entry here is universal.
            "Definition handbuilt_reachability__Probe_specs : list hassert := (@nil hassert).",
            "Theorem valid_handbuilt_reachability__Probe : ValidSpec handbuilt_reachability \
             handbuilt_reachability__Probe_specs.",
            // Both retained bodies are ordinary `module_func` definitions.
            "Definition ex_probe : module_func :=",
            "Definition uq_probe : module_func :=",
        ] {
            assert!(
                v.contains(needle),
                "the reachability probe must emit `{needle}`; got:\n{v}"
            );
        }

        type_check_with_coqc(
            &module,
            "Reachability probe generated and its record-literal, partition \
             and kind-selected theorem shapes verified",
        );
    }

    /// A stub declaration that no gated module can name, with the reason why.
    ///
    /// Each entry claims a producer is *impossible*, never merely absent: "no
    /// fixture happens to emit it" is the hole this audit exists to close, so
    /// the remedy for such a name is a fixture, not a row here. Every resident
    /// below is a name the stub's own files spell while no emitted module can,
    /// which means [`compile_stub`] elaborates it before every gate runs and a
    /// drift in it fails the stub compile rather than shipping green.
    const DECLARATIONS_WITHOUT_A_PRODUCER: &[(&str, &str)] = &[
        (
            "byte",
            "an opaque type Parameter. An emitted module names byte *values* — \
             the `#NN` notations and `encode` — never the type they inhabit, \
             and elaborating any one of them forces it. The stub's own \
             `moddata_init`/`imp_name` field types spell it.",
        ),
        (
            "i32",
            "an opaque machine-integer type Parameter. Emitted output reaches it \
             only through the witness `i32m` and the constructor `VAL_int32`, \
             both of which are produced; the type name itself has no emitted \
             spelling.",
        ),
        (
            "i64",
            "an opaque machine-integer type Parameter, unreachable by name for \
             the same reason as `i32`.",
        ),
        (
            "HA_pred",
            "the emitter prints `term_eq`, which is a Definition *of* \
             `HA_pred pred_eq`. Every emitted `term_eq` therefore elaborates it, \
             and an arity drift in it stops `Assertions.v` from compiling at \
             all.",
        ),
        (
            "pred_eq",
            "the distinguished predicate index `term_eq` is defined from; \
             reached exactly as `HA_pred` is, and never printed by name.",
        ),
        (
            "seq",
            "a `Notation` for `list`, kept so the stub's inductive fields read \
             like the real library's `seq term`/`seq hassert`. The emitter \
             imports no mathcomp and writes `list hassert`, so the notation is \
             elaborated only where the stub itself uses it.",
        ),
    ];

    /// Every name the vendored stub declares must be named by a module this
    /// test compiles with `coqc`, or carry a reason in
    /// [`DECLARATIONS_WITHOUT_A_PRODUCER`].
    ///
    /// A declaration with no producer is a declaration `coqc` never elaborates:
    /// its arity and spelling can drift freely and every gate above still
    /// passes. That is precisely how the #230 `BI_forall` arity bug shipped, and
    /// roughly sixty declarations sat in that state before #401. The needle
    /// lists in the gates above only guard what somebody remembered to write
    /// down, so this test derives both sides mechanically instead: the declared
    /// side is parsed out of the stub `.v` files [`compile_stub`] compiles, and
    /// the produced side is tokenised out of every module in [`gated_modules`].
    ///
    /// Tokenising counts every mention of a name, and a mention is a
    /// *reference* to the stub's declaration only while nothing in the emitted
    /// text *binds* that name. A fixture function, module or spec whose emitted
    /// name happened to equal a stub constructor or record field would
    /// otherwise mark that declaration covered by defining something unrelated
    /// under its spelling — the constructor-shape drift this audit exists to
    /// catch, wearing a producer's badge. [`emitted_bindings`] therefore
    /// collects what each module binds, and the audit asserts that none of it
    /// is a stub declaration; that assertion is what makes the tokenised set
    /// mean what it claims. It asserts rather than subtracts because
    /// subtracting is unsound in the other direction: a name that is both bound
    /// and genuinely referenced would come back unproduced and fail a producer
    /// that is really there. A collision is worth failing on for its own sake
    /// anyway, because the binding shadows the stub's declaration for the rest
    /// of the file: a module that also *references* that name gets the local
    /// definition instead, and `coqc` rejects the `.v` (#405). The two halves
    /// are complementary rather than redundant — a module carrying no such
    /// reference compiles clean, and is exactly the module whose collision
    /// coverage cannot see — so the audit fails on the binding itself and says
    /// which fixture to rename.
    ///
    /// The audit then compiles that same set itself. Measuring coverage against
    /// generated *text* while leaving the elaboration to other tests would make
    /// the claim only as strong as their health: `#[ignore]` the two gates
    /// covering `ME_active` and a live arity drift in it ships with the audit
    /// green, which is the #230 failure mode wearing the audit's badge. Handing
    /// every counted module to `coqc` here makes "produced" mean "elaborated"
    /// by construction, and no other test's state can launder it.
    ///
    /// Three further ways to fail, on the coverage side proper, so the audit
    /// cannot rot into a rubber stamp: a declaration that is neither produced
    /// nor exempt (the hole reopening), an exemption for something that *is*
    /// produced (a stale reason, to be deleted rather than left to accumulate),
    /// and an exemption naming nothing the stub declares (a typo, which would
    /// silently exempt nothing). Those three and the collision check all run
    /// whether or not `coqc` is installed, and the skip message says which half
    /// of the claim the run actually established.
    ///
    /// This does not make the per-shape gates above redundant, for two reasons
    /// worth keeping straight. Their negative needles pin terms that type-check
    /// perfectly well and are still wrong — `MED_global 0%N` for a defined
    /// global, `modstart_func := 0%N` in `mod_funcs` numbering — and no amount
    /// of elaboration distinguishes those from the right index. And when they
    /// do overlap with this audit they fail *better*: a gate names the shape it
    /// is about, where a failure here is a raw `coqc` log or a list of
    /// uncovered names. What this audit adds is the floor underneath them —
    /// coverage of everything nobody thought to write a needle for.
    ///
    /// Two deliberate limits. Inductive and record *type* names (`hassert`,
    /// `module_func`, `sx`, …) are left out of the declared set: the emitter
    /// spells almost none of them, while `coqc` elaborates a type whenever it
    /// elaborates one of its constructors, so demanding a producer for the type
    /// would only grow the exemption list without covering anything new. And
    /// coverage here is name-level — it proves each constructor is elaborated
    /// somewhere, not that every *argument shape* of it is, nor that any
    /// particular module is what elaborates it. `BI_select` is the standing
    /// example of the first: its `None` form is produced, while its `Some` form
    /// needs a typed `select` the translator rejects outright. Deleting
    /// `spec_bitwise_arith.inf` from [`CORPUS`] is the standing example of the
    /// second — `BOI_sub` stays produced by the hand-assembled instruction
    /// module, and it is the corpus operator matrix in
    /// [`corpus_type_checks_against_vendored_stub`], not this audit, that
    /// notices.
    #[test]
    fn every_stub_declaration_has_a_producer() {
        let modules = gated_modules();
        // Comments are stripped from the emitted text for the same reason they
        // are stripped from the stub: `coqc` elaborates neither, so a name that
        // appears only inside an emitted name-section annotation such as
        // `(*narrow*)` is neither a producer nor a binding.
        let stripped: Vec<String> = modules.iter().map(|m| strip_rocq_comments(&m.v)).collect();
        let declared = stub_declarations();

        // First, because every assertion below reads a producer set that means
        // "references a stub declaration" only while this holds.
        let collisions: Vec<String> = stripped
            .iter()
            .zip(&modules)
            .flat_map(|(text, m)| {
                emitted_bindings(text)
                    .into_iter()
                    .map(move |name| (m, name))
            })
            .filter_map(|(m, name)| {
                let (_, file) = declared.iter().find(|(declared, _)| declared == name)?;
                Some(format!(
                    "  {name}  bound by `{}`, declared in {file}",
                    m.source
                ))
            })
            .collect();
        assert!(
            collisions.is_empty(),
            "these gated modules bind a name the vendored stub declares. The \
             binding shadows that declaration for the rest of the file, which \
             leaves the coverage measurement below meaningless — the \
             declaration counts as produced because something unrelated was \
             defined under its spelling, not because anything referenced it — \
             and turns any reference to it from the same module into a `coqc` \
             type error (#405):\n{}\n\
             Rename the offending fixture function, module or spec; the stub's \
             names are the contract's and cannot move.",
            collisions.join("\n")
        );

        let produced: FxHashSet<&str> = stripped
            .iter()
            .flat_map(|text| tokenize(text))
            .filter_map(|token| match token {
                Tok::Ident(name) => Some(name),
                _ => None,
            })
            .collect();

        let unknown: Vec<&str> = DECLARATIONS_WITHOUT_A_PRODUCER
            .iter()
            .map(|&(exempt, _)| exempt)
            .filter(|exempt| !declared.iter().any(|(name, _)| name == exempt))
            .collect();
        assert!(
            unknown.is_empty(),
            "these entries of DECLARATIONS_WITHOUT_A_PRODUCER name nothing the \
             vendored stub declares, so they exempt nothing and hide a real \
             hole behind a typo: {}",
            unknown.join(", ")
        );

        let stale: Vec<String> = DECLARATIONS_WITHOUT_A_PRODUCER
            .iter()
            .filter(|&&(exempt, _)| produced.contains(exempt))
            .map(|(exempt, reason)| format!("  {exempt}  — claimed: {reason}"))
            .collect();
        assert!(
            stale.is_empty(),
            "these entries of DECLARATIONS_WITHOUT_A_PRODUCER now have a \
             producer among the gated modules, so their stated reasons are \
             false; delete the entries rather than leaving a stale exemption \
             that would cover a future regression:\n{}",
            stale.join("\n")
        );

        let missing: Vec<String> = declared
            .iter()
            .filter(|(name, _)| !produced.contains(name.as_str()))
            .filter(|(name, _)| {
                !DECLARATIONS_WITHOUT_A_PRODUCER
                    .iter()
                    .any(|&(exempt, _)| exempt == name)
            })
            .map(|(name, file)| format!("  {name}  (declared in {file})"))
            .collect();
        assert!(
            missing.is_empty(),
            "no module the coqc gate compiles names these stub declarations, so \
             `coqc` never elaborates them and a rename or arity change in one \
             would ship green (#401):\n{}\n\
             Add a fixture or hand-assembled module that produces each — a \
             CORPUS entry under tests/test_data/inf/ for anything an Inference \
             program can express, a HandbuiltModule variant otherwise. If a \
             producer is genuinely impossible, add the name to \
             DECLARATIONS_WITHOUT_A_PRODUCER with the reason why; \"no fixture \
             emits it\" is the bug, not a reason.",
            missing.join("\n")
        );

        // Everything above compares names in generated text. What turns
        // "produced" into "elaborated" is this: the audit hands `coqc` the very
        // modules it just counted, under its own work-directory label, so the
        // strong claim rests on nothing but this test.
        let Some(coqc) = find_coqc() else {
            eprintln!(
                "skipped: coqc not found (set COQC or put coqc on PATH). \
                 Stub declarations parsed and partitioned into produced, exempt \
                 and missing — but coverage was measured against generated text \
                 only: nothing was elaborated, so a constructor's arity or shape \
                 is unverified on this run."
            );
            return;
        };
        let work = compile_stub(&coqc, "audit");
        for m in &modules {
            let v_path = work.join(format!("{}.v", m.module));
            std::fs::write(&v_path, &m.v).unwrap_or_else(|e| panic!("write {}: {e}", m.source));
            if let Err(log) = coqc_compile(&coqc, &work, &v_path) {
                panic!(
                    "coqc rejected `{}`, which this audit counts as a producer; \
                     until it elaborates, every declaration it is the only \
                     producer of is uncovered:\n{log}\n\
                     work dir kept for inspection: {}",
                    m.source,
                    work.display()
                );
            }
        }
        let _ = std::fs::remove_dir_all(&work);
    }

    /// Every name the vendored stub declares, paired with the stub file that
    /// declares it. Parses exactly the files [`compile_stub`] compiles.
    fn stub_declarations() -> Vec<(String, String)> {
        let stub = stub_dir();
        STUB_MODULES
            .iter()
            .flat_map(|&(dir, module)| {
                let file = format!("{dir}/{module}.v");
                let path = stub.join(dir).join(format!("{module}.v"));
                let source = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("read stub {}: {e}", path.display()));
                declared_names(&source)
                    .into_iter()
                    .map(move |name| (name, file.clone()))
            })
            .collect()
    }

    /// The Rocq vernacular an emitted module binds a name with, and all of it.
    /// `Translator::translate` in `core/wasm-to-v/src/translator.rs` is the
    /// only writer of top-level sentences, and the only sentences it writes
    /// that introduce a name are its `Definition`s (the preamble helpers, one
    /// per surviving function, the module record, and the per-spec
    /// obligations), its `Theorem`s, and the `Context` the `Section Host` block
    /// opens with. The rest — `Require`, `From … Require`, `Open Scope`,
    /// `Section`/`End`, `Proof`/`Qed` — bind nothing an emitted term can name.
    const EMITTED_BINDING_KEYWORDS: &[&str] = &["Definition", "Theorem", "Context"];

    /// Every name an emitted module binds, in emission order.
    ///
    /// All three forms share one shape: the bound names are the identifiers
    /// between the keyword and the first `:`, whether that colon opens a type
    /// annotation (`Definition is_prime : module_func`, `Theorem valid_m :
    /// ValidModule m`, ``Context `{ho: host}``) or heads the `:=` of an
    /// annotation-free helper (`Definition Mg mut t init :=`). A helper's
    /// parameters are therefore bound names too, which is the point — a
    /// parameter shadows inside the body exactly as the definition's own name
    /// shadows outside it, and either way a token spelled like a stub
    /// declaration has stopped referring to one.
    ///
    /// Takes text whose comments are already stripped, so an identifier inside
    /// an emitted `(*name*)` name-section annotation cannot be read as a
    /// binding.
    fn emitted_bindings(stripped: &str) -> Vec<&str> {
        let tokens = tokenize(stripped);
        let mut names = Vec::new();
        let mut at = 0;
        while at < tokens.len() {
            let keyword = ident_at(&tokens, at);
            at += 1;
            if !keyword.is_some_and(|k| EMITTED_BINDING_KEYWORDS.contains(&k)) {
                continue;
            }
            while at < tokens.len() && !is_punct(&tokens, at, ':') {
                if let Some(name) = ident_at(&tokens, at) {
                    names.push(name);
                }
                at += 1;
            }
        }
        names
    }

    /// [`emitted_bindings`] is the other half of the audit's measuring
    /// instrument, and a binding form it misses is a name that can collide with
    /// a stub declaration unnoticed — which is the whole reason the audit may
    /// read a token as a reference. This pins every form the emitter writes,
    /// against a miniature module shaped like a real one.
    ///
    /// The two traps are the point of the fixture. A definition named after a
    /// constructor it also *references* in its body (`ROI_eq`) must be reported
    /// once, as a binding — the collision case a fixture rename causes. And a
    /// constructor a body only mentions (`BOI_add`) must not be reported at
    /// all, or the audit would start rejecting honest producers.
    #[test]
    fn emitted_bindings_reads_every_emitted_binding_form() {
        let source = r#"
Require Import List.
From Wasm Require Import datatypes.
Open Scope byte_scope.

Definition Mg mut t init := {|modglob_type := {|tg_mut := mut; tg_t := t|}|}.

Definition ROI_eq : module_func := {|
  modfunc_body :=
    BI_relop T_i32 (Relop_i ROI_eq) ::
    BI_binop T_i32 (Binop_i BOI_add) ::
    nil;
|}.

Definition m__S_specs : list hassert := (@nil hassert).

Section Host.
Context `{ho: host}.

Theorem valid_m : ValidModule m.
Proof.
Admitted.

End Host.
"#;
        assert_eq!(
            emitted_bindings(source),
            [
                "Mg",
                "mut",
                "t",
                "init",
                "ROI_eq",
                "m__S_specs",
                "ho",
                "valid_m",
            ]
        );
    }

    /// A Rocq token, at the resolution the declaration parser needs.
    ///
    /// Identifiers are what both halves of the audit are about; a string
    /// literal is distinguished only because it is all the *name* of a
    /// `Notation "#00" := …` is, and those 244 byte notations must not be read
    /// as identifier declarations. Everything else — numbers, `%`, `->` — comes
    /// through one character at a time as [`Tok::Punct`], which is enough for
    /// the `|`, `:=` and `{ … ; … }` landmarks the parser steers by.
    enum Tok<'a> {
        Ident(&'a str),
        Str,
        Punct(char),
    }

    fn tokenize(source: &str) -> Vec<Tok<'_>> {
        let mut tokens = Vec::new();
        let mut chars = source.char_indices().peekable();
        while let Some((start, c)) = chars.next() {
            if c.is_whitespace() {
                continue;
            }
            if c == '"' {
                // Rocq escapes a quote inside a string literal by doubling it,
                // and pairing quotes off in order handles that without a special
                // case: a literal `"c0""c1""c2"` is read as three adjacent
                // literals covering exactly the same span, because each escape
                // contributes two quotes and leaves no characters between the
                // pair it splits. Nothing inside a literal can therefore reach
                // the token stream as source, which is all this tokenizer needs.
                for (_, c) in chars.by_ref() {
                    if c == '"' {
                        break;
                    }
                }
                tokens.push(Tok::Str);
                continue;
            }
            if c.is_ascii_alphabetic() || c == '_' {
                let mut end = start + c.len_utf8();
                while let Some(&(at, c)) = chars.peek() {
                    if c.is_ascii_alphanumeric() || c == '_' || c == '\'' {
                        end = at + c.len_utf8();
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Tok::Ident(&source[start..end]));
                continue;
            }
            tokens.push(Tok::Punct(c));
        }
        tokens
    }

    fn ident_at<'a>(tokens: &[Tok<'a>], at: usize) -> Option<&'a str> {
        match tokens.get(at) {
            Some(Tok::Ident(name)) => Some(name),
            _ => None,
        }
    }

    fn is_punct(tokens: &[Tok<'_>], at: usize, c: char) -> bool {
        matches!(tokens.get(at), Some(Tok::Punct(got)) if *got == c)
    }

    /// Rocq comments nest — `(* a (* b *) c *)` is one comment, and a parser
    /// that stops at the first `*)` would read the tail as source. Newlines are
    /// preserved so a stripped file keeps its line structure, and a string
    /// literal is copied through untouched so a `(*` inside one cannot open a
    /// comment. Rocq's escaped quote `""` needs no special case here for the
    /// reason it needs none in [`tokenize`]: the pair puts no characters
    /// between the literal it closes and the one it reopens, so the stripper is
    /// never outside a literal at a position that holds anything.
    ///
    /// One divergence from Rocq's own lexer, deliberately not modelled: Rocq
    /// recognises a string literal *inside* a comment, so `(* "*)" *)` is one
    /// comment, while this ends it at the inner `*)` and reads the rest as an
    /// unterminated literal that swallows the file. Nothing writes such a
    /// comment — the stub's are prose, the emitter's are `(*name*)`
    /// annotations from the WASM name section — and a name that could produce
    /// one would already be emitting `.v` that `coqc` rejects.
    fn strip_rocq_comments(source: &str) -> String {
        let mut out = String::with_capacity(source.len());
        let mut chars = source.chars().peekable();
        let mut depth = 0usize;
        let mut in_string = false;
        while let Some(c) = chars.next() {
            if in_string {
                out.push(c);
                in_string = c != '"';
            } else if depth == 0 && c == '"' {
                out.push(c);
                in_string = true;
            } else if c == '(' && chars.peek() == Some(&'*') {
                chars.next();
                depth += 1;
            } else if depth > 0 && c == '*' && chars.peek() == Some(&')') {
                chars.next();
                depth -= 1;
            } else if depth == 0 || c == '\n' {
                // Inside a comment only the newlines survive, so a stripped
                // file keeps the line numbering of the original.
                out.push(c);
            }
        }
        out
    }

    /// Every name a stub `.v` file declares, in source order.
    ///
    /// The four shapes that matter are inductive constructors, record field
    /// names, and the top-level `Definition`/`Parameter`/`Axiom` and
    /// identifier-named `Notation` bindings. Inductive and record *type* names
    /// are deliberately not collected — see
    /// [`every_stub_declaration_has_a_producer`] — and neither are the scope
    /// declarations (`Declare Scope`, `Delimit Scope`) or the `host` `Class`,
    /// none of which name a term an emitted module could apply.
    fn declared_names(source: &str) -> Vec<String> {
        let source = strip_rocq_comments(source);
        let tokens = tokenize(&source);
        let mut names = Vec::new();
        let mut at = 0;
        while at < tokens.len() {
            let Some(keyword) = ident_at(&tokens, at) else {
                // A constructor, in the leading-`|` form every inductive in the
                // stub is written with.
                if is_punct(&tokens, at, '|')
                    && is_punct(&tokens, at + 2, ':')
                    && let Some(name) = ident_at(&tokens, at + 1)
                {
                    names.push(name.to_string());
                }
                at += 1;
                continue;
            };
            match keyword {
                "Inductive" | "Variant" => {
                    // Rocq lets the *first* constructor omit its leading `|`, so
                    // it is read here, off the `:=`; the `|` arm above picks up
                    // the rest either way.
                    while at + 1 < tokens.len()
                        && !(is_punct(&tokens, at, ':') && is_punct(&tokens, at + 1, '='))
                    {
                        at += 1;
                    }
                    at += 2;
                    if is_punct(&tokens, at + 1, ':')
                        && let Some(name) = ident_at(&tokens, at)
                    {
                        names.push(name.to_string());
                    }
                }
                "Record" => at = push_record_fields(&tokens, at, &mut names),
                "Definition" => {
                    if let Some(name) = ident_at(&tokens, at + 1) {
                        names.push(name.to_string());
                    }
                    at += 1;
                }
                // `Parameter a b : T.` binds every name before the colon.
                "Parameter" | "Axiom" => {
                    at += 1;
                    while let Some(name) = ident_at(&tokens, at) {
                        names.push(name.to_string());
                        at += 1;
                    }
                }
                // A string-named notation binds no identifier.
                "Notation" => {
                    if let Some(name) = ident_at(&tokens, at + 1) {
                        names.push(name.to_string());
                    }
                    at += 1;
                }
                _ => at += 1,
            }
        }
        names
    }

    /// Pushes the field names of the `Record` whose keyword sits at `at`,
    /// returning the index just past its closing brace. A field is the
    /// identifier that opens each `;`-separated chunk of the brace block, which
    /// makes the scan independent of how the record is laid out across lines.
    fn push_record_fields(tokens: &[Tok<'_>], at: usize, names: &mut Vec<String>) -> usize {
        let mut at = at + 1;
        while at < tokens.len() && !is_punct(tokens, at, '{') {
            at += 1;
        }
        at += 1;
        let mut depth = 1usize;
        let mut at_field = true;
        while at < tokens.len() && depth > 0 {
            if is_punct(tokens, at, '{') {
                depth += 1;
                at_field = false;
            } else if is_punct(tokens, at, '}') {
                depth -= 1;
                at_field = false;
            } else if depth == 1 && is_punct(tokens, at, ';') {
                at_field = true;
            } else {
                if at_field
                    && depth == 1
                    && is_punct(tokens, at + 1, ':')
                    && let Some(name) = ident_at(tokens, at)
                {
                    names.push(name.to_string());
                }
                at_field = false;
            }
            at += 1;
        }
        at
    }

    /// The declaration parser is the audit's measuring instrument: a shape it
    /// silently misses is a declaration exempted from needing a producer, which
    /// is the failure the audit exists to prevent. This pins every shape the
    /// stub uses, plus the three traps — a nested comment (a parser that closed
    /// on the first `*)` would declare `commented_out`), a string-named
    /// notation (which must not be read as an identifier binding), and a
    /// `Module` wrapper (whose contents are declarations all the same).
    ///
    /// The escaped-quote line is not a fourth trap, and is not claimed as one:
    /// pairing quotes off in order already reads it correctly, for the reason
    /// [`tokenize`] gives. It is pinned rather than argued so that a later
    /// hand-written `""` case cannot get it wrong — `phantom` is what a parser
    /// that ended the literal at the first half of the pair would declare.
    ///
    /// What is *not* pinned, because it is not handled: a string literal
    /// inside a comment, which Rocq recognises and [`strip_rocq_comments`]
    /// does not. That limit is stated there rather than covered here.
    #[test]
    fn declared_names_reads_every_stub_declaration_shape() {
        // A `##` delimiter: the byte-notation line below contains `"#`, which
        // would close a plain `r#"…"#` raw string.
        let source = r##"
(* A comment (* nested (* twice *) *) hiding Parameter commented_out : Type. *)
Require Import BinNat.
Declare Scope fake_scope.
Delimit Scope fake_scope with fake.
Class a_class : Type := { }.
Parameter opaque_type : Type.
Parameter first second : nat.
Axiom an_axiom : nat.
Notation "#00" := (encode 0%Z) : fake_scope.
Notation "escaped ""Parameter phantom"" tail" := (list) : fake_scope.
Notation an_alias := list.
Unset Elimination Schemes.
Inductive piped : Type :=
| Ctor_a : piped
| Ctor_b : nat -> piped.
Set Elimination Schemes.
Inductive unpiped : Type := Ctor_c : unpiped | Ctor_d : unpiped.
Record a_record : Type := {
  field_one : nat;
  field_two : option nat
}.
Module A_module.
  Parameter inner : nat.
End A_module.
Definition a_definition (x : nat) : nat := x.
"##;
        assert_eq!(
            declared_names(source),
            [
                "opaque_type",
                "first",
                "second",
                "an_axiom",
                "an_alias",
                "Ctor_a",
                "Ctor_b",
                "Ctor_c",
                "Ctor_d",
                "field_one",
                "field_two",
                "inner",
                "a_definition",
            ]
        );
    }

    /// Gallina's `-` is an infix operator, so a negative integer constant has to
    /// reach the `.v` parenthesized: `Vi32 -1` parses as the subtraction
    /// `Vi32 - 1`, and `coqc` rejects the whole module with "The term `Vi32` has
    /// type `Z -> value_num` while it is expected to have type `nat`" — one
    /// negative constant anywhere makes proof mode unusable for the program
    /// (#314).
    ///
    /// The corpus-wide scan is the load-bearing half: it holds every emitter
    /// that renders a Rocq term to the rule, not only the arm this fixture
    /// happens to reach. The per-spelling assertions then pin that the fixture
    /// keeps *producing* a negative everywhere one is reachable — `i8`/`i16`/
    /// `i32` share `i32.const` while `i64` has its own arm, the two `MIN` values
    /// are the widest negatives each width can carry, the two unsigned cases are
    /// negatives no minus sign appears in the source for, and the two `T_const`
    /// spellings come from the separate `hassert` obligation printer.
    ///
    /// The scan runs over [`corpus_modules`] rather than [`CORPUS`] so linked
    /// modules are in it. That was a distinction without a difference while every
    /// constant in the corpus was chosen by a fixture author; a merged foreign
    /// body's constants are chosen by its own compiler, and one of them is
    /// already `i32::MIN`.
    #[test]
    fn negative_constants_are_parenthesized() {
        const FIXTURE: &str = "spec_negative_consts.inf";

        let generated = corpus_modules();
        let unparenthesized: Vec<String> = generated
            .iter()
            .flat_map(|m| {
                m.v.lines()
                    .filter(|line| line.contains("Vi32 -") || line.contains("Vi64 -"))
                    .map(move |line| format!("{}: {}", m.source, line.trim()))
            })
            .collect();
        assert!(
            unparenthesized.is_empty(),
            "a negative constant reached the `.v` unparenthesized; Gallina reads \
             it as a subtraction and `coqc` rejects the module:\n{}",
            unparenthesized.join("\n")
        );

        let v = &generated
            .iter()
            .find(|m| m.source == FIXTURE)
            .unwrap_or_else(|| panic!("{FIXTURE} must be a CORPUS entry"))
            .v;
        for needle in [
            "BI_const_num (Vi32 (-8))",                   // i8
            "BI_const_num (Vi32 (-300))",                 // i16
            "BI_const_num (Vi32 (-70000))",               // i32
            "BI_const_num (Vi32 (-2147483648))",          // i32 minimum
            "BI_const_num (Vi32 (-1))",                   // u32 all-ones
            "BI_const_num (Vi64 (-4294967296))",          // i64
            "BI_const_num (Vi64 (-9223372036854775808))", // i64 minimum
            "BI_const_num (Vi64 (-1))",                   // u64 all-ones
            "T_const (Vi32 (-70000))",                    // obligation term, i32
            "T_const (Vi64 (-4294967296))",               // obligation term, i64
        ] {
            assert!(
                v.contains(needle),
                "{FIXTURE} no longer emits `{needle}`; the coqc gate would stop \
                 covering that constant:\n{v}"
            );
        }
    }

    /// `&&`/`||` lower to a valued `if (result i32)` block, which proof-mode
    /// translation renders as a valued `BI_if`. This fixture is the first corpus
    /// producer of `BT_valtype (Some ...)` — via its *executable* functions
    /// `guard_div`/`either`, whose bodies survive in the module record.
    ///
    /// The obligations are the other half. A term-position `&&`/`||` cannot be
    /// an eager `T_binop`: the term language is strict in every operand, so an
    /// eager encoding demands the operand the compiled code branches around and
    /// turns a claim the program satisfies into a refutable one. The fixture
    /// therefore carries the witness shape in every term position — a pure
    /// `let`, an `if` condition, a comparison operand, and nested on the right —
    /// and this gate pins both directions: the two eager spellings are absent,
    /// and the witness spellings that replaced them are present.
    ///
    /// The absence half is only as strong as the fixture's reach. `&&`/`||`
    /// aside, `Binop_i BOI_and`/`BOI_or` come from bitwise `&`/`|` and from the
    /// masks that narrow a sub-word unsigned result, and the fixture has neither,
    /// so a regression to eager lowering has nowhere to hide behind an unrelated
    /// producer.
    #[test]
    fn short_circuit_emits_valued_bi_if() {
        let v = generate_v("spec_short_circuit.inf", "spec_short_circuit");
        assert!(
            v.contains("BI_if (BT_valtype (Some (T_num T_i32)))"),
            "expected a valued `BI_if` from short-circuit `&&`/`||` lowering; got:\n{v}"
        );
        for eager in ["Binop_i BOI_and", "Binop_i BOI_or"] {
            assert!(
                !v.contains(eager),
                "short-circuit lowering must not emit a term-level `{eager}`; got:\n{v}"
            );
        }
        for (needle, what) in [
            (
                "HA_ex (HA_and (Hor ",
                "a term-position operator is an `HA_ex` binder whose body leads \
                 with the two-armed constraint pinning it",
            ),
            (
                "(HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i ROI_eq) \
                 (T_local 0%N) (T_const (Vi32 0))) (T_const (Vi32 0)))) \
                 (term_eq (T_lvar 0) (T_const (Vi32 1))))",
                "the taken arm of a `||` pins the witness to 1 without \
                 evaluating the right operand",
            ),
            (
                "(HA_and (term_eq (T_relop T_i32 (Relop_i (ROI_gt SX_S)) \
                 (T_local 0%N) (T_const (Vi32 0))) (T_const (Vi32 0))) \
                 (term_eq (T_lvar 0) (T_const (Vi32 0))))",
                "the skipped arm of an `&&` pins the witness to 0 without \
                 evaluating the right operand",
            ),
            (
                "HA_ex (HA_ex (HA_and (Hor ",
                "a right-nested pair of operators binds two witnesses",
            ),
            (
                "(term_eq (T_relop T_i32 (Relop_i ROI_eq) (T_local 0%N) \
                 (T_const (Vi32 0))) (T_const (Vi32 0))) (HA_and (Hor ",
                "the inner operator's constraint sits inside the outer's \
                 skipped arm, the only arm that evaluates it",
            ),
        ] {
            assert!(v.contains(needle), "{what}; expected `{needle}` in:\n{v}");
        }
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
    /// reaches Rocq as the bit pattern `(-1)`, `main`'s argument literal is typed
    /// by `scaled`'s parameter, and the return position is typed by `threshold`'s
    /// declared `-> i64` — which the first obligation then applies, so the width
    /// the return position chose is visible in the claim and not only in the
    /// module record.
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
        assert!(
            golden.contains("(T_app 2 nil) (T_const (Vi64 4294967296))"),
            "the return-position literal must reach an obligation, compared \
             against `threshold`'s result at the width its `-> i64` chose:\n{golden}"
        );
        assert!(
            !golden.contains("HA_true"),
            "no obligation here may be vacuous; an `HA_true` would mean a claim \
             collapsed away and the golden was regenerated from the \
             collapse:\n{golden}"
        );
    }

    /// Committed `.v` golden for the `exists`-kind reachability fixture.
    /// Regenerate with the `#[ignore]`d [`regenerate::regenerate_exists_spec_v`]
    /// after an intentional emitter change.
    fn exists_spec_golden_path() -> PathBuf {
        get_test_data_path().join("rocq").join("rocq_exists_spec.v")
    }

    /// The proof-mode `.v` for the `exists`-kind fixture must match a committed
    /// golden byte-for-byte, and the golden must carry the reachability
    /// contract shape: the spec function is RETAINED in the module record with
    /// a vanilla body and a choice-suffixed type, the obligation is a
    /// `reachability_spec` record whose payload reads the real frame slots,
    /// the visible-locs list carries the entry parameter and both NAMED
    /// choices while excluding the anonymous one, and the kind selects
    /// `ValidExistsSpec` — never `ValidSpec` — over the payload.
    #[test]
    fn exists_spec_matches_committed_v_golden() {
        let generated = generate_v("rocq_exists_spec.inf", "rocq_exists_spec");
        let golden_path = exists_spec_golden_path();
        let golden = std::fs::read_to_string(&golden_path).unwrap_or_else(|e| {
            panic!(
                "read {} ({e}); regenerate with \
                 `cargo test -p inference-tests regenerate_exists_spec_v -- --ignored`",
                golden_path.display()
            )
        });
        assert_eq!(
            generated,
            golden,
            "proof-mode `.v` for rocq_exists_spec.inf drifted from the committed \
             golden {}; if the emitter change was intentional, regenerate with \
             `cargo test -p inference-tests regenerate_exists_spec_v -- --ignored`",
            golden_path.display()
        );

        // Contract shape, asserted independently of the byte compare so a
        // future regeneration cannot launder a reachability regression into
        // the golden.
        assert!(
            golden.contains("From WasmVerifier Require Import Assertions Verifier Exists.\n"),
            "a reachability-bearing module must import the `Exists` contract:\n{golden}"
        );
        assert!(
            golden.contains("Definition ex_double : module_func :="),
            "the exists spec function must be RETAINED in the module record — \
             the obligation's `reach_func` looks it up there:\n{golden}"
        );
        assert!(
            golden.contains(
                "Tf (T_num T_i32 :: T_num T_i32 :: T_num T_i64 :: T_num T_i32 :: nil) (nil)"
            ),
            "the retained function's type must carry the hidden choice suffix \
             (entry i32, then the i32/i64/i32 choices), with no result:\n{golden}"
        );
        assert!(
            golden.contains("reach_func := 1%N; reach_entry_arity := 1%nat"),
            "the record must name the retained function's `mod_funcs` index and \
             the declared-parameter count ahead of the suffix:\n{golden}"
        );
        assert!(
            golden.contains("reach_visible_locs := (0%N :: 1%N :: 2%N :: nil)"),
            "visible locs must be the entry parameter plus the two NAMED \
             choices — the anonymous call-argument choice at slot 3 has no \
             source-visible face and must be excluded:\n{golden}"
        );
        assert!(
            golden.contains("(T_app 0 ((T_local 3%N) :: nil))"),
            "the payload must still READ the anonymous choice through its \
             frame slot — excluded from the observation, not from the \
             claim:\n{golden}"
        );
        assert!(
            golden.contains(
                "Definition rocq_exists_spec__ReachableDouble_specs : list hassert := \
                 (@nil hassert)."
            ),
            "the universal list must stay, explicitly typed and empty — the \
             exists payload must NOT be emitted under `ValidSpec`:\n{golden}"
        );
        assert!(
            golden.contains(
                "Theorem valid_exists_rocq_exists_spec__ReachableDouble : \
                 ValidExistsSpec rocq_exists_spec rocq_exists_spec__ReachableDouble_ex_specs."
            ),
            "the exists partition must be consumed by a `ValidExistsSpec` \
             theorem:\n{golden}"
        );
        assert!(
            !golden.contains("HA_has_type"),
            "a reachability payload denotes against the real reached frame, \
             where every slot already carries its runtime type — the universal \
             slot guards must not appear:\n{golden}"
        );
        assert!(
            !golden.contains("BI_forall") && !golden.contains("BI_exists"),
            "the retained body must be vanilla WASM — no non-det constructor \
             may survive:\n{golden}"
        );
    }

    /// Committed `.v` golden for the aggregate-values fixture. Regenerate with
    /// the `#[ignore]`d [`regenerate::regenerate_aggregate_values_v`] after an
    /// intentional emitter change.
    fn aggregate_values_golden_path() -> PathBuf {
        get_test_data_path()
            .join("rocq")
            .join("spec_aggregate_values.v")
    }

    /// The proof-mode `.v` for the aggregate-values fixture must match a
    /// committed golden byte-for-byte, and the golden must carry the leaf
    /// encoding's contract shape: a compound `@` and a compound parameter
    /// quantify one `T_local` slot per scalar leaf, each leading its
    /// antecedent with an `HA_has_type` guard at the leaf's own width; an
    /// aggregate `==` is a leafwise `term_eq` conjunction; an aggregate `@`
    /// under an `exists` block binds nested `HA_ex` binders. No memory atom
    /// exists to be emitted — the pure fragment is the whole encoding.
    #[test]
    fn spec_aggregate_values_matches_committed_v_golden() {
        let generated = generate_v("spec_aggregate_values.inf", "spec_aggregate_values");
        let golden_path = aggregate_values_golden_path();
        let golden = std::fs::read_to_string(&golden_path).unwrap_or_else(|e| {
            panic!(
                "read {} ({e}); regenerate with \
                 `cargo test -p inference-tests regenerate_aggregate_values_v -- --ignored`",
                golden_path.display()
            )
        });
        assert_eq!(
            generated,
            golden,
            "proof-mode `.v` for spec_aggregate_values.inf drifted from the committed \
             golden {}; if the emitter change was intentional, regenerate with \
             `cargo test -p inference-tests regenerate_aggregate_values_v -- --ignored`",
            golden_path.display()
        );

        // Contract shape, asserted independently of the byte compare so a
        // future regeneration cannot launder an encoding regression into the
        // committed file.
        assert!(
            golden.contains("HA_has_type (T_local 2%N) T_i32"),
            "a 3-leaf array `@` must guard its third leaf slot:\n{golden}"
        );
        assert!(
            golden.contains("HA_has_type (T_local 1%N) T_i64"),
            "a struct's `i64` field leaf must be guarded at its own width:\n{golden}"
        );
        assert!(
            golden.contains("term_eq (T_local 0%N) (T_local 2%N)"),
            "aggregate `==` must be the leafwise `term_eq` of matching leaves:\n{golden}"
        );
        assert!(
            golden.contains("HA_ex (HA_ex"),
            "an aggregate `@` in an `exists` block must bind one nested `HA_ex` \
             per leaf:\n{golden}"
        );
        assert!(
            !golden.contains("HA_pto") && !golden.contains("HA_iter"),
            "the leaf encoding is pure — no memory atom may appear:\n{golden}"
        );
    }

    /// Committed `.v` golden for the bounded-iteration fixture. Regenerate
    /// with the `#[ignore]`d [`regenerate::regenerate_bounded_iteration_v`]
    /// after an intentional emitter change.
    fn bounded_iteration_golden_path() -> PathBuf {
        get_test_data_path()
            .join("rocq")
            .join("spec_bounded_iteration.v")
    }

    /// The proof-mode `.v` for the bounded-iteration fixture must match a
    /// committed golden byte-for-byte, and the golden must carry the
    /// non-constant index's contract shape: the element is a fresh `HA_ex`
    /// binder whose definition leads with the *unsigned* range bound and then
    /// names one case per element. The range bound is conjoined with the claim
    /// rather than guarding it, which is what makes an out-of-range index
    /// refute the obligation instead of discharging it vacuously.
    #[test]
    fn spec_bounded_iteration_matches_committed_v_golden() {
        let generated = generate_v("spec_bounded_iteration.inf", "spec_bounded_iteration");
        let golden_path = bounded_iteration_golden_path();
        let golden = std::fs::read_to_string(&golden_path).unwrap_or_else(|e| {
            panic!(
                "read {} ({e}); regenerate with \
                 `cargo test -p inference-tests regenerate_bounded_iteration_v -- --ignored`",
                golden_path.display()
            )
        });
        assert_eq!(
            generated,
            golden,
            "proof-mode `.v` for spec_bounded_iteration.inf drifted from the committed \
             golden {}; if the emitter change was intentional, regenerate with \
             `cargo test -p inference-tests regenerate_bounded_iteration_v -- --ignored`",
            golden_path.display()
        );

        // Contract shape, asserted independently of the byte compare so a
        // future regeneration cannot launder an encoding regression into the
        // committed file.
        assert!(
            golden.contains("T_relop T_i32 (Relop_i (ROI_lt SX_U))"),
            "the element's range bound must be the term-level *unsigned* `<`; a signed \
             comparison would leave a negative index unconstrained:\n{golden}"
        );
        assert!(
            golden.contains(
                "HA_ex (HA_and (HA_and (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_lt SX_U))"
            ),
            "the element is a fresh binder whose definition is *conjoined* with the claim and \
             leads with the range bound — conjoined is what refutes an out-of-range index \
             instead of discharging it vacuously:\n{golden}"
        );
        assert!(
            golden.contains("Himpl (term_eq (T_local"),
            "the element must be defined by cases over its index:\n{golden}"
        );
        assert!(
            !golden.contains("HA_pto") && !golden.contains("HA_iter"),
            "the element encoding is pure — no memory atom may appear:\n{golden}"
        );
    }

    /// Committed `.v` golden for the quantifier-alternation fixture.
    /// Regenerate with the `#[ignore]`d
    /// [`regenerate::regenerate_quantifier_alternation_v`] after an intentional
    /// emitter change.
    fn quantifier_alternation_golden_path() -> PathBuf {
        get_test_data_path()
            .join("rocq")
            .join("spec_quantifier_alternation.v")
    }

    /// The proof-mode `.v` for the quantifier-alternation fixture must match a
    /// committed golden byte-for-byte, and the golden must carry the
    /// alternation's contract shape: a `Hall` *inside* the enclosing `HA_ex`
    /// (the other order is the swap the encoding exists to prevent), a typing
    /// guard for the universal variable stated as an antecedent within its own
    /// binder, and one `Hall` per scalar leaf of an aggregate `@`.
    #[test]
    fn spec_quantifier_alternation_matches_committed_v_golden() {
        let generated = generate_v(
            "spec_quantifier_alternation.inf",
            "spec_quantifier_alternation",
        );
        let golden_path = quantifier_alternation_golden_path();
        let golden = std::fs::read_to_string(&golden_path).unwrap_or_else(|e| {
            panic!(
                "read {} ({e}); regenerate with \
                 `cargo test -p inference-tests regenerate_quantifier_alternation_v -- --ignored`",
                golden_path.display()
            )
        });
        assert_eq!(
            generated,
            golden,
            "proof-mode `.v` for spec_quantifier_alternation.inf drifted from the committed \
             golden {}; if the emitter change was intentional, regenerate with \
             `cargo test -p inference-tests regenerate_quantifier_alternation_v -- --ignored`",
            golden_path.display()
        );

        // Contract shape, asserted independently of the byte compare so a
        // future regeneration cannot launder an encoding regression into the
        // committed file.
        assert!(
            golden.contains("HA_ex (HA_and (term_eq (T_lvar 0) (T_const (Vi32 0))) (Hall "),
            "the universal binder must sit INSIDE the existential one; the other order is \
             the alternation swap the explicit binder exists to prevent:\n{golden}"
        );
        assert!(
            golden.contains("Hall (Himpl (HA_has_type (T_lvar 0) T_i32)"),
            "a nested-universal `@` must state its typing as an antecedent within its own \
             binder — a `T_lvar` guard outside its quantifier names nothing:\n{golden}"
        );
        assert!(
            golden.contains("Hall (Hall (Himpl (HA_and (HA_has_type (T_lvar 1) T_i32)"),
            "an aggregate `@` under the nested quantifier must bind one `Hall` per scalar \
             leaf over one shared guard antecedent:\n{golden}"
        );
        assert!(
            golden.contains("Hall (Himpl (HA_has_type (T_lvar 0) T_i32) (HA_ex (HA_and (Hor "),
            "the two binder channels must interleave inside one block: a short-circuit \
             witness allocated after a universal variable is bound within it, off the same \
             level counter:\n{golden}"
        );
    }

    /// Committed `.v` golden for the `unique`-kind reachability fixture.
    /// Regenerate with the `#[ignore]`d [`regenerate::regenerate_unique_spec_v`]
    /// after an intentional emitter change.
    fn unique_spec_golden_path() -> PathBuf {
        get_test_data_path().join("rocq").join("rocq_unique_spec.v")
    }

    /// The proof-mode `.v` for the `unique`-kind fixture must match a committed
    /// golden byte-for-byte, and the golden must carry the unique half of the
    /// reachability contract: the same retention and record grammar as
    /// `exists`, gathered into `_uq_specs` under `ValidUniqueSpec`, with the
    /// named choice in the visible-locs list — the projection `unique`
    /// compares exit states through. Nothing else in the repository
    /// distinguishes the named-choice rule from a params-only projection at
    /// the emitted-text level, so this golden is that rule's regression pin.
    #[test]
    fn unique_spec_matches_committed_v_golden() {
        let generated = generate_v("rocq_unique_spec.inf", "rocq_unique_spec");
        let golden_path = unique_spec_golden_path();
        let golden = std::fs::read_to_string(&golden_path).unwrap_or_else(|e| {
            panic!(
                "read {} ({e}); regenerate with \
                 `cargo test -p inference-tests regenerate_unique_spec_v -- --ignored`",
                golden_path.display()
            )
        });
        assert_eq!(
            generated,
            golden,
            "proof-mode `.v` for rocq_unique_spec.inf drifted from the committed \
             golden {}; if the emitter change was intentional, regenerate with \
             `cargo test -p inference-tests regenerate_unique_spec_v -- --ignored`",
            golden_path.display()
        );

        assert!(
            golden.contains("Definition uq_parity : module_func :="),
            "the unique spec function must be RETAINED in the module record:\n{golden}"
        );
        assert!(
            golden.contains("reach_visible_locs := (0%N :: 1%N :: nil)"),
            "visible locs must be the entry parameter AND the named choice — a \
             params-only projection would silently weaken `unique` to \
             `exists`:\n{golden}"
        );
        assert!(
            golden.contains(
                "Theorem valid_unique_rocq_unique_spec__UniqueParity : \
                 ValidUniqueSpec rocq_unique_spec rocq_unique_spec__UniqueParity_uq_specs."
            ),
            "the unique partition must be consumed by a `ValidUniqueSpec` \
             theorem:\n{golden}"
        );
        assert!(
            golden.contains(
                "Definition rocq_unique_spec__UniqueParity_specs : list hassert := \
                 (@nil hassert)."
            ),
            "the unique payload must NOT be emitted under `ValidSpec`:\n{golden}"
        );
        assert!(
            !golden.contains("HA_ex"),
            "the choices are quantified operationally by the predicate — no \
             `HA_ex` binder may double-quantify them in the payload:\n{golden}"
        );
    }

    /// Regeneration helpers for the committed `.v` goldens. `#[ignore]`d by
    /// design (per CONTRIBUTING.md): they are not behavioral tests but rewrite a
    /// golden from current emitter output. Run explicitly after an intentional
    /// change, e.g.
    /// `cargo test -p inference-tests regenerate_prime_example_v -- --ignored`.
    #[cfg(test)]
    mod regenerate {
        use super::{
            aggregate_values_golden_path, bounded_iteration_golden_path, exists_spec_golden_path,
            generate_v, literal_ctx_golden_path, prime_golden_path,
            quantifier_alternation_golden_path, unique_spec_golden_path,
        };
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

        #[test]
        #[ignore]
        fn regenerate_exists_spec_v() {
            let v = generate_v("rocq_exists_spec.inf", "rocq_exists_spec");
            write_golden(&v, &exists_spec_golden_path());
        }

        #[test]
        #[ignore]
        fn regenerate_unique_spec_v() {
            let v = generate_v("rocq_unique_spec.inf", "rocq_unique_spec");
            write_golden(&v, &unique_spec_golden_path());
        }

        #[test]
        #[ignore]
        fn regenerate_aggregate_values_v() {
            let v = generate_v("spec_aggregate_values.inf", "spec_aggregate_values");
            write_golden(&v, &aggregate_values_golden_path());
        }

        #[test]
        #[ignore]
        fn regenerate_bounded_iteration_v() {
            let v = generate_v("spec_bounded_iteration.inf", "spec_bounded_iteration");
            write_golden(&v, &bounded_iteration_golden_path());
        }

        #[test]
        #[ignore]
        fn regenerate_quantifier_alternation_v() {
            let v = generate_v(
                "spec_quantifier_alternation.inf",
                "spec_quantifier_alternation",
            );
            write_golden(&v, &quantifier_alternation_golden_path());
        }
    }
}
