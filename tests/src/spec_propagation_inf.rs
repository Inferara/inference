//! `.inf`-driven integration tests for spec propagation.
//!
//! Complements `spec_propagation.rs` (inline-string scenarios) by exercising
//! the type-checker × codegen × wasm-to-v contract on file-loaded source
//! fixtures under `tests/test_data/inf/`. Each fixture targets a coverage
//! gap surfaced by the post-merge review:
//!
//! - `spec_method.inf` — spec containing a struct with two methods.
//! - `spec_calls_top.inf` — spec-inner function calling a top-level helper.
//!   Asserts the lowered `Call` operand targets the top-level WASM index.
//! - `three_specs.inf` — three specs of mixed shapes (free fn / struct
//!   + method / empty), exercising sorted emission and empty-spec preservation.
//! - `mixed_compile_proof.inf` — same source compiled in both modes,
//!   asserting the byte-identity invariant (compile-mode WASM strictly
//!   shorter, no spec section name).
//! - `with_spec.inf` (pre-existing) — smoke test wiring an unused fixture.

#[cfg(test)]
mod helpers {
    use crate::utils::get_test_data_path;
    use inference_type_checker::TypeCheckerBuilder;
    use inference_wasm_codegen::{CodegenOutput, CompilationMode, OptLevel, Target};

    /// Loads source from `tests/test_data/inf/<file>` and runs the full
    /// pipeline (parse → type-check → codegen) under the given mode and
    /// module name. Returns the `CodegenOutput`.
    pub(super) fn compile_inf(
        file: &str,
        mode: CompilationMode,
        module_name: &str,
    ) -> CodegenOutput {
        let path = get_test_data_path().join("inf").join(file);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let arena = crate::utils::build_ast(source);
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
            },
        )
        .unwrap_or_else(|e| panic!("codegen failed for {file}: {e}"))
    }

    /// Returns true if `wasm` contains the byte slice `needle`.
    pub(super) fn wasm_contains(wasm: &[u8], needle: &[u8]) -> bool {
        wasm.windows(needle.len()).any(|w| w == needle)
    }

    /// Walks the WASM `code` section and returns the operand of the first
    /// `Call` instruction inside the function body at `target_func_idx`.
    /// Counts defined (non-import) functions only — matches how
    /// `inference-wasm-codegen` numbers functions for a Reactor module.
    pub(super) fn read_first_call_operand_for_func(
        wasm: &[u8],
        target_func_idx: u32,
    ) -> Option<u32> {
        use inf_wasmparser::{Operator, Parser, Payload};
        let mut defined_idx: u32 = 0;
        for payload in Parser::new(0).parse_all(wasm) {
            let payload = payload.ok()?;
            if let Payload::CodeSectionEntry(body) = payload {
                if defined_idx == target_func_idx {
                    let mut reader = body.get_operators_reader().ok()?;
                    while !reader.eof() {
                        if let Ok(Operator::Call { function_index }) = reader.read() {
                            return Some(function_index);
                        }
                    }
                    return None;
                }
                defined_idx += 1;
            }
        }
        None
    }
}

// ============================================================================
// Fixture 1: spec_method.inf — spec containing struct + multiple methods
// ============================================================================
#[cfg(test)]
mod fixture_spec_method {
    use super::helpers::compile_inf;
    use inference_wasm_codegen::CompilationMode;
    use rustc_hash::FxHashMap;

    /// The spec block contains a struct with two methods. Both methods must
    /// register as spec-inner functions in the per-spec map, the WASM must
    /// validate, and neither spec-inner method may appear in the export list.
    #[test]
    fn spec_methods_register_and_are_not_exported() {
        let output = compile_inf("spec_method.inf", CompilationMode::Proof, "specmethod");
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm).expect("WASM must validate");

        let by_spec = output.spec_func_indices_by_spec();
        let geometry = by_spec
            .get("Geometry")
            .expect("spec Geometry should surface a per-spec entry");
        assert_eq!(
            geometry.len(),
            2,
            "spec Geometry should expose two struct-method indices; got {geometry:?}"
        );

        // Spec-inner methods must NOT appear in the WASM export list. Only
        // top-level `pub fn main` should be exported.
        use wasmtime::{Engine, Module};
        let engine = Engine::default();
        let module = Module::new(&engine, wasm).expect("module must instantiate");
        let exports: Vec<String> = module.exports().map(|e| e.name().to_string()).collect();
        assert!(
            exports.iter().any(|n| n == "main"),
            "top-level pub fn `main` must be exported; got {exports:?}"
        );
        assert!(
            !exports.iter().any(|n| n.contains("sum_coords") || n.contains("doubled_x")),
            "spec-inner struct methods must not be exported; got {exports:?}"
        );

        // Round-trip via the embedded section into Rocq; the per-spec
        // Definition + Theorem pair must materialize.
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let v = inference::wasm_to_v("Ignored", wasm, &empty, &inference::HSpecMap::default())
            .expect("translate ok");
        assert!(
            v.contains("Definition specmethod__Geometry_specs"),
            "per-spec definition for Geometry must be emitted:\n{v}"
        );
        assert!(
            v.contains("Theorem valid_specmethod__Geometry : ValidSpec specmethod specmethod__Geometry_specs."),
            "per-spec ValidSpec theorem missing:\n{v}"
        );
    }
}

// ============================================================================
// Fixture 2: spec_calls_top.inf — cross-scope call resolution
// ============================================================================
#[cfg(test)]
mod fixture_spec_calls_top {
    use super::helpers::{compile_inf, read_first_call_operand_for_func};
    use inference_wasm_codegen::CompilationMode;
    use rustc_hash::FxHashMap;

    /// A spec-inner function `caller()` returns `helper()` — a top-level
    /// function. The lowered `Call` instruction in `caller`'s body must
    /// target `helper`'s WASM function index (not, say, the spec-inner
    /// `caller` index, and not a placeholder). This pins down the
    /// "spec-inner can refer to top-level" half of the spec-aware
    /// `try_spec_first / fall_back_to_top_level` lookup policy.
    #[test]
    fn spec_inner_call_targets_top_level_helper() {
        let output = compile_inf("spec_calls_top.inf", CompilationMode::Proof, "calltop");
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm).expect("WASM must validate");

        // Codegen index order (see lib.rs::register_function_indices):
        //   regular fns (base 0) → regular methods → spec fns → spec methods.
        // The source has two top-level fns (`helper`, `main`) and one spec
        // fn (`caller`). So helper=0, main=1, caller=2.
        let by_spec = output.spec_func_indices_by_spec();
        let caller_list = by_spec
            .get("Caller")
            .expect("spec Caller should have a per-spec index list");
        assert_eq!(caller_list.len(), 1, "spec Caller has one inner fn; got {caller_list:?}");
        let caller_idx = caller_list[0];
        assert_eq!(caller_idx, 2, "caller should land at index 2");

        let call_target = read_first_call_operand_for_func(wasm, caller_idx)
            .expect("spec-inner `caller` body must contain a Call instruction");
        // helper is index 0 — the first top-level fn declared.
        assert_eq!(
            call_target, 0,
            "spec-inner `caller` must invoke top-level `helper` at index 0; \
             observed call operand: {call_target}"
        );

        // Sanity: top-level `main` also calls helper at index 0.
        let main_call_target = read_first_call_operand_for_func(wasm, 1)
            .expect("top-level `main` body must contain a Call instruction");
        assert_eq!(
            main_call_target, 0,
            "top-level main must also call helper at 0; observed: {main_call_target}"
        );

        // Round-trip: `caller`'s body is a `return helper()`, which contributes
        // no obligation term, so `caller` yields a trivial `HA_true` obligation
        // gathered into Caller's `_specs`. The spec function itself is omitted
        // from the module record (the WASM-level call index checked above is on
        // the executable bytes, which retain it).
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let v = inference::wasm_to_v("Ignored", wasm, &empty, &inference::HSpecMap::default())
            .expect("translate ok");
        assert!(
            v.contains("Definition calltop__Caller_specs : list hassert := (calltop__Caller_hspec1 :: nil)."),
            "Caller_specs should gather its single obligation:\n{v}"
        );
        assert!(
            v.contains("Theorem valid_calltop__Caller : ValidSpec calltop calltop__Caller_specs."),
            "Caller must get its ValidSpec theorem:\n{v}"
        );
    }
}

// ============================================================================
// Fixture 3: three_specs.inf — sorted emission + mixed shapes + empty spec
// ============================================================================
#[cfg(test)]
mod fixture_three_specs {
    use super::helpers::compile_inf;
    use inference_wasm_codegen::CompilationMode;
    use rustc_hash::FxHashMap;

    /// Three specs (`Alpha` free fn, `Beta` struct+method, `Gamma` empty)
    /// exercise: (a) sorted per-spec emission order; (b) mixed-shape
    /// registration (free fn AND spec-inner struct method); (c) empty-spec
    /// preservation through the `ensure_spec_registered` path.
    #[test]
    fn three_specs_sorted_and_empty_spec_preserved() {
        let output = compile_inf("three_specs.inf", CompilationMode::Proof, "threespecs");
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm).expect("WASM must validate");

        let by_spec = output.spec_func_indices_by_spec();
        let mut names: Vec<&str> = by_spec.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            vec!["Alpha", "Beta", "Gamma"],
            "all three specs must surface in the per-spec map"
        );

        // Per-spec inner counts: Alpha → 1 free fn; Beta → 1 struct method;
        // Gamma → 0 (preserved as empty).
        assert_eq!(by_spec.get("Alpha").map(Vec::len), Some(1));
        assert_eq!(by_spec.get("Beta").map(Vec::len), Some(1));
        assert_eq!(by_spec.get("Gamma").map(Vec::len), Some(0));

        // Translate via the embedded section. The output must contain all
        // three per-spec definitions, sorted alphabetically by spec name.
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let v = inference::wasm_to_v("Ignored", wasm, &empty, &inference::HSpecMap::default())
            .expect("translate ok");

        let pos_alpha = v.find("Definition threespecs__Alpha_specs");
        let pos_beta = v.find("Definition threespecs__Beta_specs");
        let pos_gamma = v.find("Definition threespecs__Gamma_specs");
        assert!(pos_alpha.is_some(), "Alpha definition missing:\n{v}");
        assert!(pos_beta.is_some(), "Beta definition missing:\n{v}");
        assert!(pos_gamma.is_some(), "Gamma definition missing:\n{v}");
        assert!(
            pos_alpha.unwrap() < pos_beta.unwrap() && pos_beta.unwrap() < pos_gamma.unwrap(),
            "definitions must be emitted alphabetically (Alpha < Beta < Gamma):\n{v}"
        );

        // Gamma (empty) and Beta (methods only, no free-fn obligation) both
        // render their obligation lists as `(@nil hassert)`; Alpha's free fn
        // yields a trivial obligation, so its list is non-empty.
        assert!(
            v.contains("Definition threespecs__Gamma_specs : list hassert := (@nil hassert)."),
            "empty spec must emit `(@nil hassert)`:\n{v}"
        );

        // Each spec also gets its `valid_<mod>__<Spec>` ValidSpec theorem.
        for spec in &["Alpha", "Beta", "Gamma"] {
            let needle = format!(
                "Theorem valid_threespecs__{spec} : ValidSpec threespecs threespecs__{spec}_specs."
            );
            assert!(
                v.contains(&needle),
                "missing per-spec theorem `{needle}` in:\n{v}"
            );
        }
    }
}

// ============================================================================
// Fixture 4: mixed_compile_proof.inf — byte-identity invariant
// ============================================================================
#[cfg(test)]
mod fixture_mixed_compile_proof {
    use super::helpers::{compile_inf, wasm_contains};
    use inference_wasm_codegen::CompilationMode;

    /// Compile the same `.inf` in both `Compile` and `Proof` modes. Verifies:
    ///   1. Compile-mode WASM byte length is strictly less than proof-mode
    ///      (compile strips spec functions; proof preserves them).
    ///   2. Compile-mode WASM does NOT contain the `inference.spec_funcs`
    ///      section name (byte-identity invariant for non-spec consumers).
    ///   3. Proof-mode WASM DOES contain the section name.
    ///   4. Compile-mode in-memory spec map is empty; proof-mode is not.
    #[test]
    fn compile_mode_strictly_smaller_and_lacks_spec_section() {
        let compile_out =
            compile_inf("mixed_compile_proof.inf", CompilationMode::Compile, "mixed");
        let proof_out =
            compile_inf("mixed_compile_proof.inf", CompilationMode::Proof, "mixed");

        let compile_wasm = compile_out.wasm();
        let proof_wasm = proof_out.wasm();

        // (1) Size invariant: proof mode preserves two extra spec functions
        // plus the custom section, so it is strictly larger.
        assert!(
            compile_wasm.len() < proof_wasm.len(),
            "compile-mode WASM ({} bytes) should be strictly smaller than \
             proof-mode WASM ({} bytes) for spec-bearing source",
            compile_wasm.len(),
            proof_wasm.len(),
        );

        // (2) and (3): section name presence.
        let needle = inference::SPEC_FUNCS_SECTION_NAME.as_bytes();
        assert!(
            !wasm_contains(compile_wasm, needle),
            "compile-mode WASM must not contain `{}`",
            inference::SPEC_FUNCS_SECTION_NAME
        );
        assert!(
            wasm_contains(proof_wasm, needle),
            "proof-mode WASM must contain `{}`",
            inference::SPEC_FUNCS_SECTION_NAME
        );

        // (4): in-memory map shape.
        assert!(
            compile_out.spec_func_indices_by_spec().is_empty(),
            "compile-mode spec map must be empty"
        );
        let proof_map = proof_out.spec_func_indices_by_spec();
        assert_eq!(
            proof_map.get("Witness").map(Vec::len),
            Some(2),
            "proof-mode Witness spec must list both inner fns; got {proof_map:?}"
        );
    }
}

// ============================================================================
// Fixture 5: with_spec.inf — smoke test wiring the previously-unused fixture
// ============================================================================
#[cfg(test)]
mod fixture_with_spec_smoke {
    use super::helpers::compile_inf;
    use inference_wasm_codegen::CompilationMode;
    use rustc_hash::FxHashMap;

    /// `with_spec.inf` contains a spec `prop()` that calls a top-level `foo`
    /// from inside a `forall` block. End-to-end compilation must succeed in
    /// proof mode and the spec must surface in the per-spec map.
    #[test]
    fn with_spec_inf_compiles_and_surfaces_spec() {
        let output = compile_inf("with_spec.inf", CompilationMode::Proof, "withspec");
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm).expect("WASM must validate");

        let by_spec = output.spec_func_indices_by_spec();
        assert!(
            by_spec.contains_key("MySpec"),
            "spec MySpec must appear in the per-spec map; got {by_spec:?}"
        );

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let v = inference::wasm_to_v("Ignored", wasm, &empty, &inference::HSpecMap::default())
            .expect("translate ok");
        assert!(
            v.contains("Definition withspec__MySpec_specs"),
            "MySpec per-spec definition must appear:\n{v}"
        );

        // `prop`'s `forall { let i = @; assert(foo(i) == i); }` becomes an
        // obligation, not module-record WASM: the spec function is OMITTED from
        // the emitted module, so no `BI_forall`/`BI_uzumaki` appears in the `.v`.
        // The obligation is `nz(relop Eq (foo(i)) i)` = `HA_not (term_eq …)`,
        // where `foo(i)` is a `T_app` into `mod_funcs` and `i` is the universal
        // `T_local 0`.
        assert!(
            v.contains("Definition withspec__MySpec_hspec1 : hassert :="),
            "MySpec's obligation must be emitted:\n{v}"
        );
        assert!(
            !v.contains("BI_forall") && !v.contains("BI_uzumaki"),
            "the omitted spec function must contribute no non-det WASM to the `.v`:\n{v}"
        );
        assert!(
            v.contains("term_eq") && v.contains("T_app ") && v.contains("T_local 0%N"),
            "the obligation must apply `foo` (T_app) to the universal slot (T_local 0):\n{v}"
        );
        assert!(
            v.contains(
                "Theorem valid_withspec__MySpec : ValidSpec withspec withspec__MySpec_specs."
            ),
            "MySpec must get its ValidSpec theorem:\n{v}"
        );
    }
}

// ============================================================================
// Fixture 6: spec_nondet_blocks.inf — inline non-det block lowering + omission
// ============================================================================
#[cfg(test)]
mod fixture_nondet_block_lowering {
    use super::helpers::{compile_inf, wasm_contains};
    use inference_wasm_codegen::CompilationMode;
    use rustc_hash::FxHashMap;

    /// The three inline non-deterministic blocks (`forall`/`exists`/`assume`
    /// inside plain spec functions) lower to the custom `0xfc`-prefixed WASM
    /// opcodes in the module's bytes — that lowering coverage lives at the byte
    /// level now. In the emitted `.v`, however, spec functions are OMITTED from
    /// the module record, so none of the `BI_forall`/`BI_exists`/`BI_assume`
    /// constructors appear; each spec function contributes its `hassert`
    /// obligation instead. This pins both halves: the WASM carries the opcodes,
    /// the `.v` carries the obligations and omits the spec bodies.
    #[test]
    fn inline_blocks_lower_to_wasm_opcodes_and_are_omitted_from_v() {
        let output = compile_inf("spec_nondet_blocks.inf", CompilationMode::Proof, "nondet");
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm).expect("WASM must validate");

        // WASM-byte lowering coverage: forall (0xfc 0x3a), exists (0xfc 0x3b),
        // assume (0xfc 0x3c), and the uzumaki rvalue (0xfc 0x31) each appear.
        assert!(
            wasm_contains(wasm, &[0xfc, 0x3a]),
            "forall opcode must lower"
        );
        assert!(
            wasm_contains(wasm, &[0xfc, 0x3b]),
            "exists opcode must lower"
        );
        assert!(
            wasm_contains(wasm, &[0xfc, 0x3c]),
            "assume opcode must lower"
        );
        assert!(
            wasm_contains(wasm, &[0xfc, 0x31]),
            "i32.uzumaki opcode must lower"
        );

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let v = inference::wasm_to_v("Ignored", wasm, &empty, &inference::HSpecMap::default())
            .expect("translate ok");

        // The spec bodies are omitted, so no non-det constructor reaches the `.v`.
        assert!(
            !v.contains("BI_forall") && !v.contains("BI_exists") && !v.contains("BI_assume"),
            "omitted spec bodies must not leave any non-det constructor in the `.v`:\n{v}"
        );
        // Each spec function contributes its obligation instead.
        assert!(
            v.contains("Definition nondet__NondetBlocks_specs : list hassert :=")
                && v.contains("Theorem valid_nondet__NondetBlocks : ValidSpec nondet nondet__NondetBlocks_specs."),
            "the spec's obligations and theorem must be emitted:\n{v}"
        );
    }
}

// ============================================================================
// Fixture 7: spec_nondet_body_modifiers.inf — exists/assume body modifiers reject
// ============================================================================
#[cfg(test)]
mod fixture_nondet_body_modifier_rejected {
    use crate::utils::build_ast;
    use inference_type_checker::TypeCheckerBuilder;
    use inference_wasm_codegen::{CompilationMode, OptLevel, Target};

    /// A spec function whose *body modifier* is `exists`/`assume`
    /// (`fn f() exists { … }`) carries a proof obligation with no milestone-1
    /// `hassert` encoding — only `forall`-quantified (or plain) spec functions
    /// translate. Proof-mode codegen must reject it with `P001`, naming both the
    /// `exists`- and `assume`-modified functions. (Previously this fixture rode
    /// the coqc corpus while the obligation pass was additive; the flip to fatal
    /// makes it a hard error, so it is a negative test now.)
    #[test]
    fn exists_and_assume_body_modifiers_are_rejected() {
        let path = crate::utils::get_test_data_path()
            .join("inf")
            .join("spec_nondet_body_modifiers.inf");
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let arena = build_ast(source);
        let typed_context = TypeCheckerBuilder::build_typed_context(arena)
            .expect("type check should succeed")
            .typed_context();
        let err = inference_wasm_codegen::codegen(
            &typed_context,
            "nbm",
            inference_wasm_codegen::CodegenOptions {
                target: Target::Wasm32,
                mode: CompilationMode::Proof,
                opt_level: OptLevel::O3,
                features: inference_wasm_codegen::EmitFeatures::default(),
            },
        )
        .expect_err("exists/assume body-modifier specs have no assertion encoding");
        let msg = err.to_string();
        assert!(
            msg.contains("P001") && msg.contains("exists_fn") && msg.contains("assume_fn"),
            "expected a P001 rejection naming both body-modifier functions; got:\n{msg}"
        );
    }
}

// ============================================================================
// A spec-function identifier long enough that its `inference.hspecs` symbol
// (`{spec}.{fn}`) overflows the codec's name cap is a clean codegen error, not
// a panic from the infallible encoder.
// ============================================================================
#[cfg(test)]
mod over_long_spec_function_name_rejected {
    use crate::utils::build_ast;
    use inference_type_checker::TypeCheckerBuilder;
    use inference_wasm_codegen::{CompilationMode, OptLevel, Target};

    /// An arbitrarily long spec-function name is legal source — the lexer and
    /// type checker impose no identifier-length limit (verified: this module
    /// type-checks the fixture before codegen) — so nothing upstream stops it.
    /// Its `inference.hspecs` function symbol is `{spec}.{fn}`; once the
    /// identifier pushes that symbol past the shared codec's byte cap, the
    /// fail-closed pre-encode validator turns it into a clean `HspecNameTooLong`
    /// codegen error rather than letting the infallible encoder panic on a
    /// decode-rejected artifact. The name is sized off `MAX_NAME_LEN` so the
    /// test tracks the cap.
    #[test]
    fn over_long_spec_function_name_is_a_clean_codegen_error() {
        let long_name = "f".repeat(inference_hassert::MAX_NAME_LEN + 64);
        let source = format!("spec S {{ fn {long_name}() -> i32 {{ return 1; }} }}");
        let arena = build_ast(source);
        let typed_context = TypeCheckerBuilder::build_typed_context(arena)
            .expect("a long identifier is legal source and must type-check")
            .typed_context();
        let err = inference_wasm_codegen::codegen(
            &typed_context,
            "olsfn",
            inference_wasm_codegen::CodegenOptions {
                target: Target::Wasm32,
                mode: CompilationMode::Proof,
                opt_level: OptLevel::O3,
                features: inference_wasm_codegen::EmitFeatures::default(),
            },
        )
        .expect_err("a 300-char spec function name overflows the inference.hspecs name cap");
        let msg = err.to_string();
        assert!(
            msg.contains("inference.hspecs") && msg.contains(&long_name),
            "expected an HspecNameTooLong diagnostic naming the identifier; got:\n{msg}"
        );
    }
}
