//! Tests covering the spec-funcidx follow-ups (issues #16-#22) verification
//! scenarios from `.claude/plans/i-realized-that-i-indexed-pinwheel.md`.
//!
//! Scenario index (see plan §Verification):
//!   1.  Type-checker scoping
//!   2.  Export gating
//!   3.  Custom WASM section round-trip
//!   4.  Per-spec emission ordering and theorems
//!   5.  Empty list `(@nil hassert)`
//!   6.  Invalid module names
//!   7.  No regressions (verified out-of-band via `cargo test`)
//!   8.  Compile-mode emits no spec section
//!   9.  Explicit-overrides-binary precedence and mismatch
//!  10.  `wasm_to_v` on compile-mode binary
//!
//! Each scenario has a dedicated `#[cfg(test)]` module below.

#[cfg(test)]
mod helpers {
    use crate::utils::build_ast;
    use inference_type_checker::TypeCheckerBuilder;
    use inference_wasm_codegen::{CodegenOutput, CompilationMode, OptLevel, Target};

    /// Compiles `source` end-to-end with the given mode and returns the codegen output.
    /// Panics on type-check or codegen failure (intended for tests asserting success).
    pub(super) fn compile(source: &str, mode: CompilationMode) -> CodegenOutput {
        compile_with_module(source, mode, "output")
    }

    /// Variant of [`compile`] that takes an explicit module name, exposed so
    /// scenarios exercising the module-name-flow-through-API path can pin a
    /// non-default value.
    pub(super) fn compile_with_module(
        source: &str,
        mode: CompilationMode,
        module_name: &str,
    ) -> CodegenOutput {
        let arena = build_ast(source.to_string());
        let typed_context = TypeCheckerBuilder::build_typed_context(arena)
            .expect("type check should succeed")
            .typed_context();
        inference_wasm_codegen::codegen(
            &typed_context,
            Target::Wasm32,
            mode,
            OptLevel::O3,
            module_name,
        )
        .expect("codegen should succeed")
    }

    /// Like [`compile`] but returns the codegen `Result` instead of unwrapping,
    /// for tests asserting a proof-mode obligation-translation rejection (a
    /// `P0xx` diagnostic is now a hard codegen error). Type-checking must still
    /// succeed — the construct is legal WASM, it only lacks an assertion
    /// encoding.
    pub(super) fn try_compile(
        source: &str,
        mode: CompilationMode,
    ) -> anyhow::Result<CodegenOutput> {
        let arena = build_ast(source.to_string());
        let typed_context = TypeCheckerBuilder::build_typed_context(arena)
            .expect("type check should succeed")
            .typed_context();
        inference_wasm_codegen::codegen(
            &typed_context,
            Target::Wasm32,
            mode,
            OptLevel::O3,
            "output",
        )
    }

    /// Returns true if `wasm` contains the byte slice `needle`.
    pub(super) fn wasm_contains(wasm: &[u8], needle: &[u8]) -> bool {
        wasm.windows(needle.len()).any(|w| w == needle)
    }
}

// Scenario 1: Type-checker scoping
#[cfg(test)]
mod scenario_1_type_checker_scoping {
    use crate::utils::build_ast;
    use inference_type_checker::TypeCheckerBuilder;

    /// A spec-inner function whose bare name matches a top-level function
    /// is rejected: the codegen layer would prefer the spec-mangled lookup
    /// while the type-checker would type the call against the closest
    /// binding, so allowing the collision risks silent miscompilation.
    /// Both sides must rename to disambiguate.
    #[test]
    fn spec_inner_function_shadowing_top_level_is_rejected() {
        let source =
            r#"fn foo() -> i32 { return 1; } spec S { fn foo() -> i32 { return 2; } }"#;
        let arena = build_ast(source.to_string());
        let result = TypeCheckerBuilder::build_typed_context(arena);
        let err = result.err().expect("shadowing must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("shadows a top-level function"),
            "diagnostic should name the shadow rule: {msg}"
        );
    }

    /// Renaming one side of the collision removes the shadow and the source
    /// type-checks. Documents the suggested remediation path from the
    /// diagnostic produced by `spec_inner_function_shadowing_top_level_is_rejected`.
    #[test]
    fn renaming_resolves_the_shadow() {
        let source = r#"fn foo() -> i32 { return 1; } fn caller() -> i32 { return foo(); } spec S { fn spec_foo() -> i32 { return 2; } }"#;
        let arena = build_ast(source.to_string());
        let result = TypeCheckerBuilder::build_typed_context(arena);
        assert!(
            result.is_ok(),
            "renamed spec-inner fn must type-check: {:?}",
            result.err()
        );
    }

    /// A `spec` containing a single function must type-check.
    #[test]
    fn single_spec_inner_function_compiles() {
        let source = r#"spec S { fn helper() -> i32 { return 99; } }"#;
        let arena = build_ast(source.to_string());
        let result = TypeCheckerBuilder::build_typed_context(arena);
        assert!(
            result.is_ok(),
            "single spec-inner fn should compile: {:?}",
            result.err()
        );
    }

    /// Intra-spec function calls resolve: a spec inner function may call a
    /// sibling inner function in the same spec. `enter_spec` re-enters the
    /// cached scope across the three type-checker phases so symbols registered
    /// during `collect_function_and_constant_definitions` remain visible during
    /// `infer_def`.
    #[test]
    fn intra_spec_call_resolves_sibling() {
        let source = r#"spec S { fn helper() -> i32 { return 99; } fn caller() -> i32 { return helper(); } }"#;
        let arena = build_ast(source.to_string());
        let result = TypeCheckerBuilder::build_typed_context(arena);
        assert!(
            result.is_ok(),
            "intra-spec sibling call should type-check: {:?}",
            result.err()
        );
    }

    /// Two specs declaring a struct with the same bare name must be rejected
    /// at type-check time. Cross-spec type mangling is not implemented: every
    /// struct access (field projection, sret layout, method dispatch) would
    /// have to carry spec context, a much wider change than the function-name
    /// mangling. Rejecting at registration surfaces a clear diagnostic
    /// (`error registering struct \`Foo\``) instead of the previous silent
    /// behavior where the first-registered spec's struct layout was
    /// transparently substituted for the second.
    #[test]
    fn same_named_struct_across_two_specs_is_rejected() {
        let source = r#"spec A { struct Foo { x: i32; } } spec B { struct Foo { y: i32; } }"#;
        let arena = build_ast(source.to_string());
        let result = TypeCheckerBuilder::build_typed_context(arena);
        assert!(
            result.is_err(),
            "same-named struct across specs must be rejected"
        );
        let msg = result.err().unwrap().to_string();
        assert!(
            msg.contains("error registering struct `Foo`"),
            "error must identify the colliding struct: {msg}"
        );
        assert!(
            msg.contains("duplicate definition within a file's spec scopes is not supported"),
            "error must cite the same-file spec collision reason: {msg}"
        );
    }

    /// Same-named enums across two specs are rejected for the same reason as
    /// structs: the codegen layout (tag indices, variant ordering) would have
    /// to be disambiguated by spec context, which is not implemented.
    #[test]
    fn same_named_enum_across_two_specs_is_rejected() {
        let source = r#"spec A { enum E { V } } spec B { enum E { W } }"#;
        let arena = build_ast(source.to_string());
        let result = TypeCheckerBuilder::build_typed_context(arena);
        assert!(
            result.is_err(),
            "same-named enum across specs must be rejected"
        );
        let msg = result.err().unwrap().to_string();
        assert!(
            msg.contains("error registering enum `E`"),
            "error must identify the colliding enum: {msg}"
        );
    }

    /// A top-level struct and a spec-inner struct with the same name are
    /// rejected: the top-level struct sits in the root scope, which is the
    /// parent of every spec scope, so a bare-name reference inside the spec
    /// would resolve to the top-level layout. Allowing both registrations
    /// would let later phases (codegen, analysis) pick whichever happens to
    /// be looked up first via `lookup_struct_anywhere`.
    #[test]
    fn same_named_struct_top_level_and_spec_is_rejected() {
        let source = r#"struct Foo { x: i32; } spec A { struct Foo { y: i32; } }"#;
        let arena = build_ast(source.to_string());
        let result = TypeCheckerBuilder::build_typed_context(arena);
        assert!(
            result.is_err(),
            "same-named struct top-level + spec must be rejected"
        );
        let msg = result.err().unwrap().to_string();
        assert!(
            msg.contains("error registering struct `Foo`"),
            "error must identify the colliding struct: {msg}"
        );
    }
}

// Scenario 2: Export gating
#[cfg(test)]
mod scenario_2_export_gating {
    use super::helpers::compile;
    use inference_wasm_codegen::CompilationMode;
    use wasmtime::{Engine, Module};

    /// `pub fn p()` inside a `spec` block must NOT appear in WASM exports;
    /// `pub fn y()` at top level must appear.
    #[test]
    fn pub_inside_spec_not_exported_top_level_is_exported() {
        let source = r#"pub fn y() -> i32 { return 7; } spec A { pub fn x() -> i32 { return 1; } }"#;
        let output = compile(source, CompilationMode::Proof);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm).expect("WASM must validate");

        // Enumerate exports authoritatively via wasmtime. A previous version
        // of this test also did a `wasm_contains(wasm, b"y")` byte-search
        // sanity check, but that is redundant with (and weaker than) the
        // structured enumeration: a single-byte needle catches every name
        // section / debug-info occurrence too.
        let engine = Engine::default();
        let module =
            Module::new(&engine, wasm).expect("module must instantiate");
        let exports: Vec<String> = module.exports().map(|e| e.name().to_string()).collect();
        assert!(
            exports.iter().any(|n| n == "y"),
            "top-level pub fn `y` should be exported; got {exports:?}"
        );
        assert!(
            exports.iter().all(|n| n != "x"),
            "spec-inner pub fn `x` must NOT be exported; got {exports:?}"
        );
    }
}

// A spec whose body builds or reads a struct has no assertion encoding, so
// proof-mode codegen now rejects it (the obligation is a required deliverable).
#[cfg(test)]
mod spec_struct_value_is_rejected {
    use super::helpers::try_compile;
    use inference_wasm_codegen::CompilationMode;

    /// A struct literal (with field-position uzumaki) and a subsequent field
    /// access in a spec `forall` body have no `hassert` encoding, so proof-mode
    /// codegen fails with `P002`. (Previously this compiled to WASM because the
    /// obligation pass was additive; the flip to fatal makes an untranslatable
    /// spec a hard error.)
    #[test]
    fn field_position_uzumaki_struct_in_spec_forall_is_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            spec S {
                fn prop() forall {
                    let p: Point = Point { x: @, y: @ };
                    assert(p.x == p.x);
                }
            }
        "#;
        let err = try_compile(source, CompilationMode::Proof)
            .expect_err("a struct-valued spec body has no assertion encoding");
        let msg = err.to_string();
        assert!(
            msg.contains("P002") && msg.contains("struct"),
            "expected a P002 rejection naming the struct construct; got:\n{msg}"
        );
    }

    /// The typed-let form (`let a: i32 = @; Point { x: a }`) reads a struct field
    /// in the assertion, so it is rejected for the same reason.
    #[test]
    fn typed_let_struct_in_spec_forall_is_rejected() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            spec S {
                fn prop() forall {
                    let a: i32 = @;
                    let b: i32 = @;
                    let p: Point = Point { x: a, y: b };
                    assert(p.x == p.x);
                }
            }
        "#;
        let err = try_compile(source, CompilationMode::Proof)
            .expect_err("a struct-valued spec body has no assertion encoding");
        let msg = err.to_string();
        assert!(
            msg.contains("P002"),
            "expected a P002 rejection for the struct construct; got:\n{msg}"
        );
    }
}

// Scenario 3: Custom WASM section round-trip
#[cfg(test)]
mod scenario_3_custom_section_round_trip {
    use super::helpers::{compile, wasm_contains};
    use inference_wasm_codegen::CompilationMode;
    use rustc_hash::FxHashMap;

    /// Proof-mode WASM embeds the `inference.spec_funcs` custom section so
    /// the binary is the self-describing artifact for downstream `wasm_to_v`
    /// consumers. `finish()` runs before `take_spec_func_indices_by_spec`
    /// drains the in-memory map, so the section is appended whenever any spec
    /// indices were recorded.
    #[test]
    fn proof_mode_embeds_spec_section() {
        let source = r#"spec A { fn p() -> i32 { return 1; } } spec B { fn q() -> i32 { return 2; } }"#;
        let output = compile(source, CompilationMode::Proof);

        assert!(
            !output.spec_func_indices_by_spec().is_empty(),
            "in-memory spec map should be non-empty in proof mode"
        );

        let needle = inference::SPEC_FUNCS_SECTION_NAME.as_bytes();
        assert!(
            wasm_contains(output.wasm(), needle),
            "proof-mode WASM must embed the spec section name"
        );
    }

    /// With an empty explicit map, the translator sources per-spec indices from
    /// the embedded `inference.spec_funcs` section. The resulting `.v` contains
    /// the per-spec `Definition <mod>__<SpecName>_specs` lines (the module name
    /// is sourced from the WASM `name` section, i.e. `output`).
    #[test]
    fn round_trip_with_empty_explicit_map_recovers_per_spec_defs() {
        let source = r#"spec A { fn p() -> i32 { return 1; } } spec B { fn q() -> i32 { return 2; } }"#;
        let output = compile(source, CompilationMode::Proof);

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let v = inference::wasm_to_v(
            "Mod",
            output.wasm(),
            &empty,
            &inference::HSpecMap::default(),
        )
        .expect("wasm_to_v should succeed");

        assert!(
            v.contains("Definition output__A_specs"),
            "per-spec definition for A should be sourced from the embedded \
             section:\n{v}"
        );
        assert!(
            v.contains("Definition output__B_specs"),
            "per-spec definition for B should be sourced from the embedded \
             section:\n{v}"
        );
    }
}

// Scenario 3b: `inference.hspecs` obligation section round-trip
#[cfg(test)]
mod scenario_3b_hspecs_section {
    use super::helpers::{compile, wasm_contains};
    use inference_hassert::HSPECS_SECTION_NAME;
    use inference_wasm_codegen::CompilationMode;

    /// A single regular-kind spec free function, which contributes one
    /// trivially-true (`HA_true`) obligation — enough to make the obligation map
    /// non-empty so the section is emitted.
    const ONE_SPEC: &str = r#"spec S { fn obligation() -> i32 { return 1; } }"#;

    /// Extracts the raw payload of the custom section named `name`, if present.
    fn custom_section(wasm: &[u8], name: &str) -> Option<Vec<u8>> {
        for payload in inf_wasmparser::Parser::new(0).parse_all(wasm) {
            if let Ok(inf_wasmparser::Payload::CustomSection(reader)) = payload
                && reader.name() == name
            {
                return Some(reader.data().to_vec());
            }
        }
        None
    }

    /// Proof mode embeds `inference.hspecs` exactly when the obligation map is
    /// non-empty — here, a spec function makes it non-empty.
    #[test]
    fn proof_mode_embeds_hspecs_when_map_non_empty() {
        let output = compile(ONE_SPEC, CompilationMode::Proof);
        assert!(
            !output.hspecs().is_empty(),
            "a proof-mode spec function must populate the obligation map"
        );
        assert!(
            wasm_contains(output.wasm(), HSPECS_SECTION_NAME.as_bytes()),
            "proof-mode WASM must embed the inference.hspecs section when the map is non-empty"
        );
    }

    /// The other direction of the "iff": a proof-mode program with no
    /// specifications has an empty obligation map, so the section is absent.
    #[test]
    fn proof_mode_omits_hspecs_when_map_empty() {
        let output = compile(r#"pub fn main() { }"#, CompilationMode::Proof);
        assert!(
            output.hspecs().is_empty(),
            "a program with no specs must have an empty obligation map"
        );
        assert!(
            !wasm_contains(output.wasm(), HSPECS_SECTION_NAME.as_bytes()),
            "proof-mode WASM must not embed the section when the map is empty"
        );
    }

    /// Compile mode strips specs, so the obligation map is empty and the section
    /// is never emitted — even for a program that carries a spec.
    #[test]
    fn compile_mode_never_embeds_hspecs() {
        let output = compile(ONE_SPEC, CompilationMode::Compile);
        assert!(
            output.hspecs().is_empty(),
            "compile mode strips specs, so the obligation map must be empty"
        );
        assert!(
            !wasm_contains(output.wasm(), HSPECS_SECTION_NAME.as_bytes()),
            "compile-mode WASM must never embed the inference.hspecs section"
        );
    }

    /// The embedded section is the self-describing artifact: decoding it must
    /// reproduce the exact obligation map codegen recorded in memory, so no
    /// obligation is lost or reordered on the wire.
    #[test]
    fn embedded_section_round_trips_to_the_output_map() {
        let source = r#"
            spec Alpha { fn a() -> i32 { return 1; } }
            spec Beta { fn b() -> i32 { return 2; } fn c() -> i32 { return 3; } }
        "#;
        let output = compile(source, CompilationMode::Proof);
        let data = custom_section(output.wasm(), HSPECS_SECTION_NAME)
            .expect("proof-mode WASM must carry the inference.hspecs section");
        let decoded =
            inference_hassert::decode(&data).expect("the embedded hspecs section must decode");
        assert_eq!(
            &decoded,
            output.hspecs(),
            "decoding the embedded section must reproduce the in-memory obligation map"
        );
    }
}

// Scenario 4: Per-spec emission ordering and theorems
#[cfg(test)]
mod scenario_4_per_spec_emission {
    use super::helpers::compile;
    use inference_wasm_codegen::CompilationMode;
    use rustc_hash::FxHashMap;

    /// Two specs `A` and `B` produce per-spec definitions and theorems sorted
    /// alphabetically. Each spec yields a `ValidSpec <mod> <mod>__<Spec>_specs`
    /// theorem.
    ///
    /// Note: the codegen always embeds module name `"output"` into the WASM
    /// name section, which the translator's custom-name-section handling uses
    /// to override the caller-supplied `mod_name`. So even though we pass
    /// `"Mod"` to `wasm_to_v`, the generated `.v` references `output`.
    #[test]
    fn two_specs_yield_sorted_per_spec_definitions_and_theorems() {
        let source = r#"spec A { fn p() -> i32 { return 1; } } spec B { fn q() -> i32 { return 2; } }"#;
        let output = compile(source, CompilationMode::Proof);

        // Pass the in-memory map explicitly to bypass the binary embedding path.
        let map = output.spec_func_indices_by_spec().clone();
        let v = inference::wasm_to_v("Mod", output.wasm(), &map, &inference::HSpecMap::default())
            .expect("translate ok");

        let pos_a = v.find("Definition output__A_specs");
        let pos_b = v.find("Definition output__B_specs");
        assert!(pos_a.is_some(), "output__A_specs definition missing:\n{v}");
        assert!(pos_b.is_some(), "output__B_specs definition missing:\n{v}");
        assert!(
            pos_a.unwrap() < pos_b.unwrap(),
            "specs should be emitted alphabetically (A before B):\n{v}"
        );
        assert!(
            v.contains("Theorem valid_output__A : ValidSpec output output__A_specs."),
            "per-spec theorem for A missing:\n{v}"
        );
        assert!(
            v.contains("Theorem valid_output__B : ValidSpec output output__B_specs."),
            "per-spec theorem for B missing:\n{v}"
        );
    }

    /// Two specs may each declare an inner function with the same bare name.
    /// The codegen mangles the registration key as `"<SpecName>.<fn>"` so the
    /// two definitions occupy distinct slots in `func_name_to_idx`. This test
    /// is the positive counterpart to the assert inside
    /// `build_func_name_to_idx_with_spec_names`: both functions must register,
    /// both must lower, and the per-spec index map must contain one distinct
    /// index per spec.
    #[test]
    fn two_specs_with_same_named_inner_fn_both_register_and_lower() {
        let source = r#"spec A { fn helper() -> i32 { return 1; } } spec B { fn helper() -> i32 { return 2; } }"#;
        let output = compile(source, CompilationMode::Proof);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm).expect("WASM must validate");

        let by_spec = output.spec_func_indices_by_spec();
        let a_indices = by_spec
            .get("A")
            .expect("spec A should have a per-spec index list");
        let b_indices = by_spec
            .get("B")
            .expect("spec B should have a per-spec index list");
        assert_eq!(
            a_indices.len(),
            1,
            "spec A should expose exactly one inner-function index; got {a_indices:?}"
        );
        assert_eq!(
            b_indices.len(),
            1,
            "spec B should expose exactly one inner-function index; got {b_indices:?}"
        );
        assert_ne!(
            a_indices[0], b_indices[0],
            "the two same-named inner functions must occupy distinct WASM \
             function indices; both resolved to {}",
            a_indices[0]
        );
    }

    /// Intra-spec call to a sibling function must lower to a `call` instruction
    /// targeting the sibling's WASM function index, not the top-level shadow.
    /// This is the codegen counterpart to `intra_spec_call_resolves_sibling`:
    /// the type checker accepts the call, and codegen must now resolve the
    /// callee to the spec-mangled `"<spec>.helper"` entry in `func_name_to_idx`.
    ///
    /// Before the spec-aware lookup landed, this source crashed codegen with
    /// `UnknownFunction("helper")` because `lower_function_call` looked up the
    /// bare `helper` key which was never registered (the registration side
    /// uses `"<SpecName>.helper"`). The spec is named `Sp` rather than `S`
    /// because the proof-mode binary embeds the spec name into the
    /// `inference.spec_funcs` section; `S` would be rejected by
    /// `validate_rocq_identifier` (Peano successor constructor) if anyone
    /// later threads this binary through `wasm_to_v`.
    #[test]
    fn intra_spec_call_lowers_to_correct_function_index() {
        let source = r#"spec Sp { fn helper() -> i32 { return 99; } fn caller() -> i32 { return helper(); } }"#;
        let output = compile(source, CompilationMode::Proof);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm).expect("WASM must validate");

        let by_spec = output.spec_func_indices_by_spec();
        let s_indices = by_spec
            .get("Sp")
            .expect("spec Sp should have a per-spec index list");
        assert_eq!(
            s_indices.len(),
            2,
            "spec Sp should expose two inner-function indices (helper, caller); got {s_indices:?}"
        );
        assert_ne!(
            s_indices[0], s_indices[1],
            "spec-inner functions must occupy distinct WASM function indices"
        );
    }

    /// Intra-spec call resolution must also work when a top-level function of
    /// the same name exists. The call must resolve to the spec's own `helper`,
    /// not the top-level one. We can verify this by giving the two definitions
    /// Spec-inner calls resolve via the spec-mangled key, and after H3 the
    /// distinct names (`helper` / `inner`) eliminate the prior ambiguity. The
    /// test confirms that codegen emits the call operand at the spec-inner
    /// function's index, not the top-level function's index — a regression
    /// in spec-aware lookup that pointed the operand at index 0 would not
    /// surface in `top()` runtime checking.
    #[test]
    fn intra_spec_call_resolves_to_spec_inner_definition() {
        // Spec is named `Sp` (not `S`) because the per-spec index map is
        // embedded into the WASM `inference.spec_funcs` section keyed by spec
        // name; `S` would collide with the Peano successor constructor that
        // `validate_rocq_identifier` rejects.
        let source = r#"fn helper() -> i32 { return 1; } pub fn top() -> i32 { return helper(); } spec Sp { fn inner() -> i32 { return 99; } fn caller() -> i32 { return inner(); } }"#;
        let output = compile(source, CompilationMode::Proof);
        let wasm = output.wasm();
        inf_wasmparser::validate(wasm).expect("WASM must validate");

        let by_spec = output.spec_func_indices_by_spec();
        let s_indices = by_spec
            .get("Sp")
            .expect("spec Sp should have a per-spec index list");
        assert_eq!(
            s_indices.len(),
            2,
            "spec Sp should expose two inner-function indices; got {s_indices:?}"
        );

        // Confirm at runtime that the top-level export `top` returns the
        // top-level `helper`'s value.
        use wasmtime::{Engine, Module, Store, TypedFunc};
        let engine = Engine::default();
        let module = Module::new(&engine, wasm).expect("module must instantiate");
        let mut store = Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .expect("instance must instantiate");
        let top: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "top")
            .expect("top should be exported");
        let v = top.call(&mut store, ()).expect("top() call should succeed");
        assert_eq!(v, 1, "top() must call the top-level `helper` (returns 1)");

        // The spec-inner `caller`'s body must contain a `Call` instruction
        // whose operand is the spec-inner `inner`'s WASM index (the first of
        // the two indices recorded for spec `Sp`).
        let spec_inner_idx = s_indices[0];
        let spec_caller_idx = s_indices[1];
        let call_target = read_first_call_operand_for_func(wasm, spec_caller_idx)
            .expect("spec-inner `caller` should contain a Call instruction");
        assert_eq!(
            call_target, spec_inner_idx,
            "spec-inner `caller` must call the spec-inner `inner` at idx {spec_inner_idx}; \
             observed call operand: {call_target}"
        );
    }

    /// Walks the WASM `code` section and returns the operand of the first
    /// `Call` instruction found inside the function body at `target_func_idx`.
    /// Counts only defined (non-import) functions, matching how
    /// `inference-wasm-codegen` assigns indices in a Reactor-style module
    /// (no host imports; codegen-emitted functions are numbered from 0).
    fn read_first_call_operand_for_func(wasm: &[u8], target_func_idx: u32) -> Option<u32> {
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

    /// Explicit map with two specs goes through the per-spec emission code path.
    /// The single-spec case is already covered by
    /// `translate_bytes_emits_per_spec_definition_and_theorem` in
    /// `core/wasm-to-v/src/lib.rs`; this test adds a multi-spec assertion.
    #[test]
    fn explicit_map_two_specs_via_inference_api() {
        let source = r#"spec A { fn p() -> i32 { return 1; } } spec B { fn q() -> i32 { return 2; } }"#;
        let output = compile(source, CompilationMode::Proof);
        // The explicit spec map must match the binary's embedded section (the
        // real indices), since a spec map now drives function omission; feed the
        // codegen-recorded map so the explicit path is exercised without
        // disagreeing with the embedded one. Obligations ride along in the
        // embedded `inference.hspecs` section (empty explicit hspecs adopts it).
        let map = output.spec_func_indices_by_spec().clone();
        // mod_name argument is overridden by the embedded "output" module name.
        let v = inference::wasm_to_v("M", output.wasm(), &map, &inference::HSpecMap::default())
            .expect("translate ok");
        assert!(
            v.contains("Definition output__A_specs : list hassert :="),
            "A def:\n{v}"
        );
        assert!(
            v.contains("Definition output__B_specs : list hassert :="),
            "B def:\n{v}"
        );
        assert!(
            v.contains("Theorem valid_output__A : ValidSpec output output__A_specs."),
            "A theorem:\n{v}"
        );
        assert!(
            v.contains("Theorem valid_output__B : ValidSpec output output__B_specs."),
            "B theorem:\n{v}"
        );
    }
}

// Scenario 5: Empty list `(@nil N)`
#[cfg(test)]
mod scenario_5_empty_list {
    use super::helpers::compile;
    use inference_wasm_codegen::CompilationMode;
    use rustc_hash::FxHashMap;

    /// When the spec map is empty, the translator must emit no `_specs` line
    /// at all (no per-spec definition). The `(@nil hassert)` literal is only
    /// relevant once a spec exists but has no free-function obligations; the
    /// emission strategy is to skip `Definition` lines entirely when the map
    /// is empty.
    ///
    /// The codegen embeds module name `"output"` into the WASM name section,
    /// so even though we pass `"Empty"`, the module definition is named
    /// `output`. With no specs, the translator emits no per-spec `ValidSpec`
    /// theorem — only the always-present 1-ary `ValidModule` one.
    #[test]
    fn empty_map_yields_no_spec_definition() {
        let source = r#"pub fn main() -> i32 { return 0; }"#;
        let output = compile(source, CompilationMode::Proof);
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let v = inference::wasm_to_v(
            "Empty",
            output.wasm(),
            &empty,
            &inference::HSpecMap::default(),
        )
        .expect("translate ok");
        assert!(
            !v.contains("_specs : list hassert"),
            "no per-spec definitions expected when map is empty:\n{v}"
        );
        assert!(
            !v.contains("ValidSpec "),
            "no per-spec theorem expected when the spec map is empty:\n{v}"
        );
        // The 1-ary module theorem is always emitted, spec-bearing or not.
        assert!(
            v.contains("Theorem valid_output : ValidModule output."),
            "the module theorem must always be present:\n{v}"
        );
    }

    /// `(@nil hassert)` is emitted when an explicit spec is present but its
    /// obligation list is empty (a method-only or empty spec).
    #[test]
    fn explicit_spec_with_empty_indices_emits_at_nil_hassert() {
        let source = r#"pub fn main() -> i32 { return 0; }"#;
        let output = compile(source, CompilationMode::Proof);
        let mut map: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        // Avoid `S` here: it shadows the Peano successor constructor and is
        // now rejected by `validate_rocq_identifier`.
        map.insert("MySpec".to_string(), Vec::new());
        let v = inference::wasm_to_v("Mod", output.wasm(), &map, &inference::HSpecMap::default())
            .expect("translate ok");
        assert!(
            v.contains("Definition output__MySpec_specs : list hassert := (@nil hassert)."),
            "expected `(@nil hassert)` for an empty spec obligation list:\n{v}"
        );
    }
}

// Scenario 6: Invalid module name
#[cfg(test)]
mod scenario_6_invalid_module_name {
    use super::helpers::compile;
    use inference_wasm_codegen::CompilationMode;
    use rustc_hash::FxHashMap;

    fn valid_wasm() -> Vec<u8> {
        let source = r#"pub fn main() -> i32 { return 0; }"#;
        compile(source, CompilationMode::Compile).wasm().to_vec()
    }

    /// Module name with `-` is rejected with `InvalidRocqIdentifier { reason: ContainsInvalidChar('-') }`.
    #[test]
    fn module_name_with_dash_is_rejected() {
        let wasm = valid_wasm();
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let result =
            inference::wasm_to_v("list-utils", &wasm, &empty, &inference::HSpecMap::default());
        let err = result.expect_err("expected error for `list-utils`");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> =
            err.downcast_ref();
        assert!(
            matches!(
                typed,
                Some(inference_wasm_to_v_translator::errors::WasmToVError::InvalidRocqIdentifier {
                    name,
                    reason: inference_wasm_to_v_translator::errors::InvalidIdentifierReason::ContainsInvalidChar('-'),
                }) if name == "list-utils"
            ),
            "expected InvalidRocqIdentifier ContainsInvalidChar('-') for `list-utils`; got: {err:?}"
        );
    }

    /// Module name shadowing the Rocq stdlib `list` is rejected with `RocqStdlibShadow`.
    #[test]
    fn module_name_stdlib_shadow_is_rejected() {
        let wasm = valid_wasm();
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let result = inference::wasm_to_v("list", &wasm, &empty, &inference::HSpecMap::default());
        let err = result.expect_err("expected error for `list`");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> =
            err.downcast_ref();
        assert!(
            matches!(
                typed,
                Some(inference_wasm_to_v_translator::errors::WasmToVError::RocqStdlibShadow {
                    name,
                }) if name == "list"
            ),
            "expected RocqStdlibShadow for `list`; got: {err:?}"
        );
    }

    /// Module name colliding with a Rocq vernacular keyword is rejected.
    #[test]
    fn module_name_reserved_keyword_is_rejected() {
        let wasm = valid_wasm();
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let result =
            inference::wasm_to_v("Definition", &wasm, &empty, &inference::HSpecMap::default());
        let err = result.expect_err("expected error for `Definition`");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> =
            err.downcast_ref();
        assert!(
            matches!(
                typed,
                Some(inference_wasm_to_v_translator::errors::WasmToVError::InvalidRocqIdentifier {
                    name,
                    reason: inference_wasm_to_v_translator::errors::InvalidIdentifierReason::ReservedKeyword,
                }) if name == "Definition"
            ),
            "expected ReservedKeyword for `Definition`; got: {err:?}"
        );
    }

    /// Module name containing `__` is rejected with `ContainsDoubleUnderscore`.
    /// The `__` sequence is reserved as the module/spec name separator in
    /// emitted Rocq output (`<mod>__<SpecName>_specs`); allowing it in either
    /// half would make split parsing ambiguous.
    #[test]
    fn module_name_with_double_underscore_is_rejected() {
        let wasm = valid_wasm();
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let result =
            inference::wasm_to_v("foo__bar", &wasm, &empty, &inference::HSpecMap::default());
        let err = result.expect_err("expected error for `foo__bar`");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> =
            err.downcast_ref();
        assert!(
            matches!(
                typed,
                Some(inference_wasm_to_v_translator::errors::WasmToVError::InvalidRocqIdentifier {
                    name,
                    reason: inference_wasm_to_v_translator::errors::InvalidIdentifierReason::ContainsDoubleUnderscore,
                }) if name == "foo__bar"
            ),
            "expected ContainsDoubleUnderscore for `foo__bar`; got: {err:?}"
        );
    }

    /// Empty module name is rejected with `EmptyName`.
    #[test]
    fn empty_module_name_is_rejected() {
        let wasm = valid_wasm();
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let result = inference::wasm_to_v("", &wasm, &empty, &inference::HSpecMap::default());
        let err = result.expect_err("expected error for empty module name");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> =
            err.downcast_ref();
        assert!(
            matches!(
                typed,
                Some(inference_wasm_to_v_translator::errors::WasmToVError::InvalidRocqIdentifier {
                    name,
                    reason: inference_wasm_to_v_translator::errors::InvalidIdentifierReason::EmptyName,
                }) if name.is_empty()
            ),
            "expected EmptyName for empty module name; got: {err:?}"
        );
    }

    /// Module name starting with `_` is rejected with `LeadingNonAlpha('_')`.
    /// Rocq reserves `_` as the wildcard pattern, so identifiers cannot start
    /// with it.
    #[test]
    fn module_name_with_leading_underscore_is_rejected() {
        let wasm = valid_wasm();
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let result = inference::wasm_to_v("_foo", &wasm, &empty, &inference::HSpecMap::default());
        let err = result.expect_err("expected error for `_foo`");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> =
            err.downcast_ref();
        assert!(
            matches!(
                typed,
                Some(inference_wasm_to_v_translator::errors::WasmToVError::InvalidRocqIdentifier {
                    name,
                    reason: inference_wasm_to_v_translator::errors::InvalidIdentifierReason::LeadingNonAlpha('_'),
                }) if name == "_foo"
            ),
            "expected LeadingNonAlpha('_') for `_foo`; got: {err:?}"
        );
    }

    /// Module name longer than 255 chars is rejected with `TooLong`.
    #[test]
    fn overlong_module_name_is_rejected() {
        let wasm = valid_wasm();
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let long = "a".repeat(256);
        let result = inference::wasm_to_v(&long, &wasm, &empty, &inference::HSpecMap::default());
        let err = result.expect_err("expected error for 256-char module name");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> =
            err.downcast_ref();
        assert!(
            matches!(
                typed,
                Some(inference_wasm_to_v_translator::errors::WasmToVError::InvalidRocqIdentifier {
                    name,
                    reason: inference_wasm_to_v_translator::errors::InvalidIdentifierReason::TooLong,
                }) if name.len() == 256
            ),
            "expected TooLong for 256-char module name; got: {err:?}"
        );
    }
}

// Scenario 6b: Embedded WASM data validation (B2, B3)
#[cfg(test)]
mod scenario_6b_embedded_data_validation {
    use super::helpers::compile;
    use inference_wasm_codegen::CompilationMode;
    use rustc_hash::FxHashMap;

    /// Encodes a `u32` as LEB128 unsigned.
    fn encode_leb128_u32(mut n: u32, out: &mut Vec<u8>) {
        loop {
            let mut byte = (n & 0x7f) as u8;
            n >>= 7;
            if n != 0 {
                byte |= 0x80;
                out.push(byte);
            } else {
                out.push(byte);
                return;
            }
        }
    }

    /// Appends a custom section with `name` and `payload` to a WASM module.
    /// Custom sections may appear anywhere after the version header per the
    /// spec, and parsers iterate all sections, so appending after the existing
    /// content is sufficient to round-trip via `inf_wasmparser`.
    fn append_custom_section(wasm: &mut Vec<u8>, name: &str, payload: &[u8]) {
        let mut body = Vec::new();
        let name_bytes = name.as_bytes();
        #[allow(clippy::cast_possible_truncation)]
        encode_leb128_u32(name_bytes.len() as u32, &mut body);
        body.extend_from_slice(name_bytes);
        body.extend_from_slice(payload);

        wasm.push(0); // custom section id
        #[allow(clippy::cast_possible_truncation)]
        encode_leb128_u32(body.len() as u32, wasm);
        wasm.extend_from_slice(&body);
    }

    /// Builds a `inference.spec_funcs` payload with a single (name, []) entry.
    /// The payload begins with the wire-format version varuint32 (currently 1);
    /// without it the decoder rejects the section before any per-entry parsing.
    fn spec_funcs_payload_with_name(name: &str) -> Vec<u8> {
        let mut p = Vec::new();
        encode_leb128_u32(inference::SPEC_FUNCS_SECTION_VERSION, &mut p);
        encode_leb128_u32(1, &mut p); // count = 1
        let name_bytes = name.as_bytes();
        #[allow(clippy::cast_possible_truncation)]
        encode_leb128_u32(name_bytes.len() as u32, &mut p);
        p.extend_from_slice(name_bytes);
        encode_leb128_u32(0, &mut p); // idx_count = 0
        p
    }

    /// Builds a minimal name-section payload containing only a Module subsection
    /// with the given module name.
    fn name_section_payload_with_module(name: &str) -> Vec<u8> {
        // Name section: each subsection is `id (u8) + size (LEB128) + body`.
        // Module subsection (id=0): `name_len (LEB128) + name_bytes`.
        let name_bytes = name.as_bytes();
        let mut sub_body = Vec::new();
        #[allow(clippy::cast_possible_truncation)]
        encode_leb128_u32(name_bytes.len() as u32, &mut sub_body);
        sub_body.extend_from_slice(name_bytes);

        let mut payload = Vec::new();
        payload.push(0u8); // module subsection id
        #[allow(clippy::cast_possible_truncation)]
        encode_leb128_u32(sub_body.len() as u32, &mut payload);
        payload.extend_from_slice(&sub_body);
        payload
    }

    fn baseline_wasm() -> Vec<u8> {
        let source = r#"pub fn main() -> i32 { return 0; }"#;
        compile(source, CompilationMode::Compile).wasm().to_vec()
    }

    /// Returns the baseline WASM with any custom section named `target_name`
    /// removed. Used by tests that append a hand-crafted version of the same
    /// section; the parser rejects duplicate name / spec_funcs sections, so
    /// the existing one must be stripped before appending.
    fn baseline_wasm_without_custom_section(target_name: &str) -> Vec<u8> {
        let wasm = baseline_wasm();
        let mut out = Vec::with_capacity(wasm.len());
        // Copy the 8-byte preamble (magic + version) verbatim.
        out.extend_from_slice(&wasm[0..8]);
        let mut i = 8;
        while i < wasm.len() {
            let section_id = wasm[i];
            let (section_len, leb_size) = decode_leb128_u32(&wasm[i + 1..]);
            let section_start = i;
            let section_end = i + 1 + leb_size + section_len as usize;
            if section_id == 0 {
                // Custom section: name_len LEB128, name bytes, then payload.
                let body_start = i + 1 + leb_size;
                let (name_len, name_leb_size) = decode_leb128_u32(&wasm[body_start..]);
                let name_start = body_start + name_leb_size;
                let name_end = name_start + name_len as usize;
                let section_name = std::str::from_utf8(&wasm[name_start..name_end])
                    .expect("custom section name must be UTF-8");
                if section_name == target_name {
                    i = section_end;
                    continue;
                }
            }
            out.extend_from_slice(&wasm[section_start..section_end]);
            i = section_end;
        }
        out
    }

    fn decode_leb128_u32(bytes: &[u8]) -> (u32, usize) {
        let mut result: u32 = 0;
        let mut shift: u32 = 0;
        for (i, &byte) in bytes.iter().enumerate() {
            result |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return (result, i + 1);
            }
            shift += 7;
        }
        (result, bytes.len())
    }

    /// B2: a hand-crafted binary whose `inference.spec_funcs` section advertises
    /// a spec named `foo__bar` (containing the reserved `__` separator) must be
    /// rejected at the decode boundary, not deferred to `translate()`.
    #[test]
    fn embedded_spec_funcs_section_with_invalid_name_is_rejected() {
        let mut wasm = baseline_wasm();
        let payload = spec_funcs_payload_with_name("foo__bar");
        append_custom_section(&mut wasm, inference::SPEC_FUNCS_SECTION_NAME, &payload);

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let err = inference::wasm_to_v("Mod", &wasm, &empty, &inference::HSpecMap::default())
            .expect_err("expected decode-boundary rejection of `foo__bar`");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> =
            err.downcast_ref();
        assert!(
            matches!(
                typed,
                Some(inference_wasm_to_v_translator::errors::WasmToVError::InvalidRocqIdentifier {
                    name,
                    reason: inference_wasm_to_v_translator::errors::InvalidIdentifierReason::ContainsDoubleUnderscore,
                }) if name == "foo__bar"
            ),
            "expected InvalidRocqIdentifier ContainsDoubleUnderscore for embedded `foo__bar`; got: {err:?}"
        );
    }

    /// B3: a hand-crafted binary whose `name` section overrides `mod_name` with
    /// an invalid identifier (`bad-name`) must be re-validated immediately after
    /// the override; the validated CLI argument cannot rescue an invalid
    /// embedded name.
    #[test]
    fn embedded_name_section_with_invalid_module_name_is_rejected() {
        // Strip the codegen-emitted `name` section first; the parser rejects
        // duplicate `name` custom sections, so appending a second one would
        // be caught before the override-then-validate path runs.
        let mut wasm = baseline_wasm_without_custom_section("name");
        let payload = name_section_payload_with_module("bad-name");
        append_custom_section(&mut wasm, "name", &payload);

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let err = inference::wasm_to_v("ValidMod", &wasm, &empty, &inference::HSpecMap::default())
            .expect_err("expected re-validation rejection of embedded `bad-name`");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> =
            err.downcast_ref();
        assert!(
            matches!(
                typed,
                Some(inference_wasm_to_v_translator::errors::WasmToVError::InvalidRocqIdentifier {
                    name,
                    reason: inference_wasm_to_v_translator::errors::InvalidIdentifierReason::ContainsInvalidChar('-'),
                }) if name == "bad-name"
            ),
            "expected InvalidRocqIdentifier ContainsInvalidChar('-') for embedded `bad-name`; got: {err:?}"
        );
    }

    /// T3.a: a truncated LEB128 (single `0x80` byte for count following the
    /// version varuint32) — continuation bit set but no following byte — must
    /// surface as `WasmParse` with the "truncated LEB128" defense string. The
    /// truncation is in the `count` varuint32, not in `version`, so the
    /// version check passes first and the count read trips the EOF.
    #[test]
    fn embedded_spec_funcs_truncated_leb128_is_rejected() {
        let mut wasm = baseline_wasm();
        // Payload: version=1, then a single continuation byte for count.
        let mut payload = Vec::new();
        encode_leb128_u32(inference::SPEC_FUNCS_SECTION_VERSION, &mut payload);
        payload.push(0x80u8);
        append_custom_section(&mut wasm, inference::SPEC_FUNCS_SECTION_NAME, &payload);

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let err = inference::wasm_to_v("Mod", &wasm, &empty, &inference::HSpecMap::default())
            .expect_err("truncated LEB128 must be rejected");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> =
            err.downcast_ref();
        match typed {
            Some(inference_wasm_to_v_translator::errors::WasmToVError::WasmParse(s)) => {
                assert!(
                    s.contains("truncated"),
                    "WasmParse must mention the truncation defense; got: {s}"
                );
            }
            other => panic!("expected WasmParse, got {other:?}"),
        }
    }

    /// T3.b: a `count` value that exceeds the remaining payload (advertising
    /// 100 specs in a one-byte payload) must surface as `WasmParse` with the
    /// "exceeds remaining payload" defense string.
    #[test]
    fn embedded_spec_funcs_count_exceeds_payload_is_rejected() {
        let mut wasm = baseline_wasm();
        // Payload: version=1, count = 100 (single-byte LEB128), then nothing else.
        let mut payload = Vec::new();
        encode_leb128_u32(inference::SPEC_FUNCS_SECTION_VERSION, &mut payload);
        payload.push(100u8);
        append_custom_section(&mut wasm, inference::SPEC_FUNCS_SECTION_NAME, &payload);

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let err = inference::wasm_to_v("Mod", &wasm, &empty, &inference::HSpecMap::default())
            .expect_err("overflowing count must be rejected");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> =
            err.downcast_ref();
        match typed {
            Some(inference_wasm_to_v_translator::errors::WasmToVError::WasmParse(s)) => {
                assert!(
                    s.contains("exceeds remaining payload"),
                    "WasmParse must mention the bound defense; got: {s}"
                );
            }
            other => panic!("expected WasmParse, got {other:?}"),
        }
    }

    /// T3.c: a spec name that is not valid UTF-8 must surface as `WasmParse`
    /// with the "invalid UTF-8 in spec name" defense string.
    #[test]
    fn embedded_spec_funcs_invalid_utf8_in_name_is_rejected() {
        let mut wasm = baseline_wasm();
        // Payload: version=1, count=1, name_len=2, bytes=0xff 0xfe, idx_count=0.
        let mut payload = Vec::new();
        encode_leb128_u32(inference::SPEC_FUNCS_SECTION_VERSION, &mut payload);
        payload.extend_from_slice(&[0x01u8, 0x02u8, 0xffu8, 0xfeu8, 0x00u8]);
        append_custom_section(&mut wasm, inference::SPEC_FUNCS_SECTION_NAME, &payload);

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let err = inference::wasm_to_v("Mod", &wasm, &empty, &inference::HSpecMap::default())
            .expect_err("invalid UTF-8 spec name must be rejected");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> =
            err.downcast_ref();
        match typed {
            Some(inference_wasm_to_v_translator::errors::WasmToVError::WasmParse(s)) => {
                assert!(
                    s.contains("invalid UTF-8"),
                    "WasmParse must mention the UTF-8 defense; got: {s}"
                );
            }
            other => panic!("expected WasmParse, got {other:?}"),
        }
    }

    /// A `inference.spec_funcs` payload whose leading version varuint32 is
    /// not the constant `SPEC_FUNCS_SECTION_VERSION` (currently 1) must be
    /// rejected at the decode boundary, surfacing as `WasmToVError::WasmParse`
    /// with a "version" defense string in the message. This is the bump-loud
    /// guarantee: a future format revision will hit this exact branch on
    /// today's parsers, rather than treating the next varuint32 as a count
    /// and silently misparsing the rest of the payload.
    #[test]
    fn decode_spec_funcs_unsupported_version_is_rejected() {
        let mut wasm = baseline_wasm();
        // Payload: version=0x99 (unsupported), then a well-formed count=0.
        // The version branch must trip before count is even read.
        let mut payload = Vec::new();
        encode_leb128_u32(0x99, &mut payload);
        encode_leb128_u32(0, &mut payload);
        append_custom_section(&mut wasm, inference::SPEC_FUNCS_SECTION_NAME, &payload);

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let err = inference::wasm_to_v("Mod", &wasm, &empty, &inference::HSpecMap::default())
            .expect_err("unsupported spec_funcs version must be rejected");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> =
            err.downcast_ref();
        match typed {
            Some(inference_wasm_to_v_translator::errors::WasmToVError::WasmParse(s)) => {
                assert!(
                    s.contains("version"),
                    "WasmParse must mention the version mismatch; got: {s}"
                );
            }
            other => panic!("expected WasmParse, got {other:?}"),
        }
    }

    /// T3.d: two `inference.spec_funcs` sections in the same binary must be
    /// rejected. Whichever side surfaces the duplicate first matters less
    /// than that the parser refuses to silently accept a mismatched or
    /// ambiguous payload.
    #[test]
    fn embedded_spec_funcs_duplicate_section_is_rejected() {
        let mut wasm = baseline_wasm();
        // Two non-empty payloads with different spec names so they cannot
        // be "the same section appearing twice".
        let payload_a = spec_funcs_payload_with_name("MySpecA");
        let payload_b = spec_funcs_payload_with_name("MySpecB");
        append_custom_section(&mut wasm, inference::SPEC_FUNCS_SECTION_NAME, &payload_a);
        append_custom_section(&mut wasm, inference::SPEC_FUNCS_SECTION_NAME, &payload_b);

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let err = inference::wasm_to_v("Mod", &wasm, &empty, &inference::HSpecMap::default())
            .expect_err("duplicate `inference.spec_funcs` sections must be rejected");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> =
            err.downcast_ref();
        match typed {
            Some(inference_wasm_to_v_translator::errors::WasmToVError::WasmParse(s)) => {
                assert!(
                    s.contains("duplicate"),
                    "WasmParse must mention `duplicate`; got: {s}"
                );
            }
            other => panic!("expected WasmParse, got {other:?}"),
        }
    }

    /// Trailing bytes after the last well-formed entry must be rejected.
    /// Without this guard a corrupted or hand-crafted payload could carry
    /// arbitrary tail bytes past the canonical schema and be silently
    /// accepted (C1, decoder hardening).
    #[test]
    fn embedded_spec_funcs_with_trailing_bytes_is_rejected() {
        let mut wasm = baseline_wasm();
        // Well-formed payload (version=1, count=1, name_len=1, "a",
        // idx_count=0) followed by a stray 0xff byte.
        let mut payload = Vec::new();
        encode_leb128_u32(inference::SPEC_FUNCS_SECTION_VERSION, &mut payload);
        encode_leb128_u32(1, &mut payload); // count=1
        encode_leb128_u32(1, &mut payload); // name_len=1
        payload.push(b'a');
        encode_leb128_u32(0, &mut payload); // idx_count=0
        payload.push(0xff); // trailing junk
        append_custom_section(&mut wasm, inference::SPEC_FUNCS_SECTION_NAME, &payload);

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let err = inference::wasm_to_v("Mod", &wasm, &empty, &inference::HSpecMap::default())
            .expect_err("trailing bytes must be rejected");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> =
            err.downcast_ref();
        match typed {
            Some(inference_wasm_to_v_translator::errors::WasmToVError::WasmParse(s)) => {
                assert!(
                    s.contains("trailing"),
                    "WasmParse must mention `trailing`; got: {s}"
                );
            }
            other => panic!("expected WasmParse, got {other:?}"),
        }
    }

    /// A spec entry whose declared `idx_count` exceeds the remaining payload
    /// must be rejected up front, before the decoder allocates a Vec sized
    /// against the bogus count. Without this defense, a malicious binary
    /// could request a multi-GB allocation.
    #[test]
    fn embedded_spec_funcs_idx_count_exceeds_payload_is_rejected() {
        let mut wasm = baseline_wasm();
        // version=1, count=1, name_len=1, "a", idx_count=u32::MAX.
        // No actual index bytes follow.
        let mut payload = Vec::new();
        encode_leb128_u32(inference::SPEC_FUNCS_SECTION_VERSION, &mut payload);
        encode_leb128_u32(1, &mut payload);
        encode_leb128_u32(1, &mut payload);
        payload.push(b'a');
        encode_leb128_u32(u32::MAX, &mut payload);
        append_custom_section(&mut wasm, inference::SPEC_FUNCS_SECTION_NAME, &payload);

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let err = inference::wasm_to_v("Mod", &wasm, &empty, &inference::HSpecMap::default())
            .expect_err("over-large idx_count must be rejected");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> =
            err.downcast_ref();
        match typed {
            Some(inference_wasm_to_v_translator::errors::WasmToVError::WasmParse(s)) => {
                assert!(
                    s.contains("index count exceeds remaining payload"),
                    "WasmParse must mention the idx_count guard; got: {s}"
                );
            }
            other => panic!("expected WasmParse, got {other:?}"),
        }
    }

    /// A second standard WASM `name` section in the same binary must be
    /// rejected. The original code silently let the second one overwrite
    /// the first, clobbering func_names_map and locals naming without
    /// warning (C2, parser hardening).
    #[test]
    fn duplicate_wasm_name_section_is_rejected() {
        let mut wasm = baseline_wasm();
        // baseline_wasm already contains one `name` section emitted by
        // codegen; appending a second triggers the duplicate guard.
        let payload = name_section_payload_with_module("Override");
        append_custom_section(&mut wasm, "name", &payload);

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let err = inference::wasm_to_v("Mod", &wasm, &empty, &inference::HSpecMap::default())
            .expect_err("duplicate `name` sections must be rejected");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> =
            err.downcast_ref();
        match typed {
            Some(inference_wasm_to_v_translator::errors::WasmToVError::WasmParse(s)) => {
                assert!(
                    s.contains("duplicate WASM `name`"),
                    "WasmParse must mention duplicate name section; got: {s}"
                );
            }
            other => panic!("expected WasmParse, got {other:?}"),
        }
    }

    /// Positive test: a single embedded `name` section overrides the
    /// caller-supplied `mod_name` argument, and that override flows into
    /// the emitted Rocq output (it determines the `Definition <mod> : module`
    /// identifier prefix).
    #[test]
    fn embedded_name_section_overrides_caller_mod_name() {
        // Strip the codegen-emitted `name` section, then append one whose
        // module name differs from the caller-supplied `Caller` argument.
        let mut wasm = baseline_wasm_without_custom_section("name");
        let payload = name_section_payload_with_module("Embedded");
        append_custom_section(&mut wasm, "name", &payload);

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let v_output =
            inference::wasm_to_v("Caller", &wasm, &empty, &inference::HSpecMap::default())
                .expect("override-then-validate must succeed for a valid embedded name");
        assert!(
            v_output.contains("Definition Embedded : module"),
            "embedded module name should drive the module identifier; got:\n{v_output}"
        );
        assert!(
            !v_output.contains("Definition Caller : module"),
            "caller-supplied mod_name should not appear once an embedded name overrides it; got:\n{v_output}"
        );
    }
}

// Scenario 7: Empty spec block surfaces a per-spec entry (B1 regression)
#[cfg(test)]
mod scenario_7_empty_spec {
    use super::helpers::{compile, wasm_contains};
    use inference_wasm_codegen::CompilationMode;
    use rustc_hash::FxHashMap;

    /// A user-authored empty `spec MySpec { }` must surface a per-spec entry
    /// with an empty index list, so the Rocq translator still emits both a
    /// `Definition output__MySpec_specs : list hassert := (@nil hassert).` line
    /// and a `Theorem valid_output__MySpec : ValidSpec output output__MySpec_specs.`
    /// theorem. Without `ensure_spec_registered`, the spec vanished silently
    /// from the proof artifact because the bucket iteration only recorded
    /// entries for non-empty inner defs.
    #[test]
    fn empty_spec_block_yields_definition_and_theorem_in_v_output() {
        // The grammar requires at least one top-level def alongside the spec
        // so wasm-encoder has something to emit; we add a trivial `main`.
        let source = r#"pub fn main() -> i32 { return 0; } spec MySpec { }"#;
        let output = compile(source, CompilationMode::Proof);

        let by_spec = output.spec_func_indices_by_spec();
        let my_spec = by_spec
            .get("MySpec")
            .expect("empty spec must still produce a per-spec entry in the in-memory map");
        assert!(
            my_spec.is_empty(),
            "empty spec must produce an empty per-spec index list; got {my_spec:?}"
        );

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let v = inference::wasm_to_v(
            "Mod",
            output.wasm(),
            &empty,
            &inference::HSpecMap::default(),
        )
        .expect("translate ok");
        assert!(
            v.contains("Definition output__MySpec_specs : list hassert := (@nil hassert)."),
            "empty spec must emit the `(@nil hassert)` definition line:\n{v}"
        );
        assert!(
            v.contains("Theorem valid_output__MySpec : ValidSpec output output__MySpec_specs."),
            "empty spec must emit the per-spec theorem:\n{v}"
        );
    }

    /// T1: mixing an empty spec with a non-empty one must produce both kinds
    /// of `Definition` line in the generated Rocq output, in alphabetical
    /// order. The empty spec's list renders as `(@nil hassert)`; the non-empty
    /// spec's `fn f() -> i32 { return 1; }` translates to a trivial `HA_true`
    /// obligation gathered into `(output__B_hspec1 :: nil)`. This guards a
    /// regression where empty-spec handling could short-circuit the per-spec
    /// emission loop and drop the non-empty entry (or vice versa).
    #[test]
    fn mixed_empty_and_non_empty_specs_yield_both_kinds() {
        let source = r#"pub fn main() -> i32 { return 0; } spec A { } spec B { fn f() -> i32 { return 1; } }"#;
        let output = compile(source, CompilationMode::Proof);
        let by_spec = output.spec_func_indices_by_spec();
        assert!(
            by_spec.get("A").expect("A entry").is_empty(),
            "A should be empty"
        );
        assert_eq!(
            by_spec.get("B").expect("B entry").len(),
            1,
            "B should have one inner fn"
        );

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let v = inference::wasm_to_v(
            "Mod",
            output.wasm(),
            &empty,
            &inference::HSpecMap::default(),
        )
        .expect("translate ok");
        assert!(
            v.contains("Definition output__A_specs : list hassert := (@nil hassert)."),
            "empty spec A must render `(@nil hassert)`:\n{v}"
        );
        assert!(
            v.contains("Definition output__B_hspec1 : hassert :="),
            "non-empty spec B must emit an hspec obligation:\n{v}"
        );
        assert!(
            v.contains("Definition output__B_specs : list hassert := (output__B_hspec1 :: nil)."),
            "non-empty spec B must gather its obligation into `_specs`:\n{v}"
        );
        let pos_a = v.find("Definition output__A_specs").unwrap();
        let pos_b = v.find("Definition output__B_specs").unwrap();
        assert!(
            pos_a < pos_b,
            "A must come before B in alphabetical order:\n{v}"
        );
    }

    /// T2: empty-spec custom-section bytes are present in proof mode. The
    /// section name appears (always written when *any* spec exists, even
    /// empty), and the encoded count for the empty indices is zero.
    #[test]
    fn empty_spec_proof_mode_embeds_section_with_zero_indices() {
        let source = r#"pub fn main() -> i32 { return 0; } spec MySpec { }"#;
        let output = compile(source, CompilationMode::Proof);
        let needle = inference::SPEC_FUNCS_SECTION_NAME.as_bytes();
        assert!(
            wasm_contains(output.wasm(), needle),
            "proof-mode WASM with an empty spec must still embed the spec section"
        );
        // The byte sequence for the spec name `MySpec` followed by the
        // 1-byte zero-indices count must also appear in the binary.
        let mut name_then_zero = b"MySpec".to_vec();
        name_then_zero.push(0u8);
        assert!(
            wasm_contains(output.wasm(), &name_then_zero),
            "empty spec must encode `MySpec` followed by a zero indices count"
        );
    }
}

// Scenario 7b: spec-inner struct methods register as spec-owned functions
#[cfg(test)]
mod scenario_7b_spec_methods {
    use super::helpers::compile;
    use inference_wasm_codegen::CompilationMode;
    use rustc_hash::FxHashMap;

    /// T4: a struct inside a spec block, with an instance method, must
    /// register the method in the per-spec `spec_methods` bucket so the
    /// resulting WASM function appears in the spec's per-spec index list.
    /// Verifies the `spec_methods` walker arm that landed in
    /// `core/wasm-codegen/src/lib.rs` is exercised by the e2e pipeline.
    #[test]
    fn spec_inner_struct_method_registers_as_spec_function() {
        let source = r#"pub fn main() -> i32 { return 0; } spec MySpec { struct P { x: i32; fn get_x(self) -> i32 { return self.x; } } }"#;
        let output = compile(source, CompilationMode::Proof);

        let by_spec = output.spec_func_indices_by_spec();
        let my_spec = by_spec
            .get("MySpec")
            .expect("spec MySpec must surface a per-spec entry");
        assert!(
            !my_spec.is_empty(),
            "spec MySpec's struct method must contribute at least one index; got {my_spec:?}"
        );

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let v = inference::wasm_to_v(
            "Mod",
            output.wasm(),
            &empty,
            &inference::HSpecMap::default(),
        )
        .expect("translate ok");
        assert!(
            v.contains("Definition output__MySpec_specs"),
            "per-spec definition for MySpec must be emitted:\n{v}"
        );
    }
}

// Scenario 8: Compile-mode emits no spec section
#[cfg(test)]
mod scenario_8_compile_mode_no_section {
    use super::helpers::{compile, wasm_contains};
    use inference_wasm_codegen::CompilationMode;

    /// In `CompilationMode::Compile`, the resulting WASM must contain zero
    /// occurrences of the spec section name bytes.
    #[test]
    fn compile_mode_wasm_omits_spec_section_name() {
        let source = r#"spec A { fn p() -> i32 { return 1; } } spec B { fn q() -> i32 { return 2; } }"#;
        let output = compile(source, CompilationMode::Compile);
        let needle = inference::SPEC_FUNCS_SECTION_NAME.as_bytes();
        assert!(
            !wasm_contains(output.wasm(), needle),
            "compile-mode WASM must not contain the spec section name"
        );
    }

    /// In `CompilationMode::Compile`, the in-memory spec map should also be
    /// empty (specs are stripped in compile mode).
    #[test]
    fn compile_mode_spec_map_is_empty() {
        let source = r#"spec A { fn p() -> i32 { return 1; } }"#;
        let output = compile(source, CompilationMode::Compile);
        assert!(
            output.spec_func_indices_by_spec().is_empty(),
            "compile-mode codegen output should expose an empty spec map"
        );
    }

    /// In-file regression smoke for the full compile-mode → `wasm_to_v`
    /// round-trip: the WASM bytes carry no `inference.spec_funcs` section, and
    /// translating those bytes (with an empty explicit map) yields a `.v` file
    /// that contains zero per-spec `_specs` definitions.
    #[test]
    fn compile_mode_round_trip_emits_no_per_spec_v_definitions() {
        use rustc_hash::FxHashMap;

        let source = r#"spec A { fn p() -> i32 { return 1; } } spec B { fn q() -> i32 { return 2; } }"#;
        let output = compile(source, CompilationMode::Compile);
        let needle = inference::SPEC_FUNCS_SECTION_NAME.as_bytes();
        assert!(
            !wasm_contains(output.wasm(), needle),
            "compile-mode WASM must not embed the spec section"
        );

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let v = inference::wasm_to_v(
            "Mod",
            output.wasm(),
            &empty,
            &inference::HSpecMap::default(),
        )
        .expect("translate ok");
        assert_eq!(
            v.matches("_specs : list hassert").count(),
            0,
            "compile-mode .v output must contain zero per-spec definitions:\n{v}",
        );
    }
}

// Scenario 9: Explicit-overrides-binary precedence and mismatch
#[cfg(test)]
mod scenario_9_explicit_vs_embedded {
    use super::helpers::compile;
    use inference_wasm_codegen::CompilationMode;
    use rustc_hash::FxHashMap;

    /// Matching case: the explicit map equals the embedded section. The
    /// `parse` step compares the two and proceeds when they agree, emitting
    /// the expected per-spec definitions.
    ///
    /// The WASM-embedded module name `"output"` overrides `mod_name`.
    #[test]
    fn explicit_map_matching_codegen_output_translates_ok() {
        let source = r#"spec A { fn p() -> i32 { return 1; } } spec B { fn q() -> i32 { return 2; } }"#;
        let output = compile(source, CompilationMode::Proof);
        let explicit = output.spec_func_indices_by_spec().clone();
        let v = inference::wasm_to_v(
            "Mod",
            output.wasm(),
            &explicit,
            &inference::HSpecMap::default(),
        )
        .expect("translate ok");
        assert!(
            v.contains("Definition output__A_specs"),
            "A def missing:\n{v}"
        );
        assert!(
            v.contains("Definition output__B_specs"),
            "B def missing:\n{v}"
        );
    }

    /// Mismatch case: the caller passes a non-empty explicit map that
    /// disagrees with the embedded `inference.spec_funcs` section. `parse`
    /// refuses to silently override either side and returns
    /// `WasmToVError::EmbeddedSpecMismatch { explicit, embedded }`.
    #[test]
    fn mismatched_explicit_map_returns_embedded_spec_mismatch() {
        let source = r#"spec A { fn p() -> i32 { return 1; } } spec B { fn q() -> i32 { return 2; } }"#;
        let output = compile(source, CompilationMode::Proof);
        let embedded = output.spec_func_indices_by_spec().clone();

        // Deliberately wrong: swap the indices so they cannot match the
        // embedded section.
        let mut bogus: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        bogus.insert("A".to_string(), vec![99]);
        bogus.insert("B".to_string(), vec![100]);

        let result = inference::wasm_to_v(
            "Mod",
            output.wasm(),
            &bogus,
            &inference::HSpecMap::default(),
        );
        let err = result.expect_err("expected EmbeddedSpecMismatch for a mismatched explicit map");
        let typed: Option<&inference_wasm_to_v_translator::errors::WasmToVError> =
            err.downcast_ref();
        assert!(
            matches!(
                typed,
                Some(inference_wasm_to_v_translator::errors::WasmToVError::EmbeddedSpecMismatch {
                    explicit,
                    embedded: emb,
                }) if explicit == &bogus && emb == &embedded
            ),
            "expected EmbeddedSpecMismatch with explicit={bogus:?} embedded={embedded:?}; got: {err:?}"
        );
    }
}

// Scenario 10: `wasm_to_v` on compile-mode binary
#[cfg(test)]
mod scenario_10_wasm_to_v_compile_mode {
    use super::helpers::compile;
    use inference_wasm_codegen::CompilationMode;
    use rustc_hash::FxHashMap;

    /// Compile a spec-bearing source in compile mode and feed the bytes to
    /// `wasm_to_v` with an empty explicit map. Compile mode embeds neither
    /// custom section, so the result carries NO per-spec definitions and NO
    /// per-spec `ValidSpec` theorem. The 1-ary `ValidModule` theorem, however,
    /// is emitted for every module regardless of specs.
    ///
    /// The WASM-embedded module name `"output"` overrides `mod_name`.
    #[test]
    fn compile_mode_binary_translates_to_specless_v() {
        let source = r#"spec A { fn p() -> i32 { return 1; } } spec B { fn q() -> i32 { return 2; } }"#;
        let output = compile(source, CompilationMode::Compile);
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let v = inference::wasm_to_v(
            "Mod",
            output.wasm(),
            &empty,
            &inference::HSpecMap::default(),
        )
        .expect("translate ok");

        // No per-spec definition or per-spec theorem lines.
        assert_eq!(
            v.matches("_specs : list hassert").count(),
            0,
            "no per-spec definitions expected:\n{v}"
        );
        assert_eq!(
            v.matches("ValidSpec ").count(),
            0,
            "no per-spec theorems expected for a specless translation:\n{v}"
        );
        // The module theorem is always present.
        assert!(
            v.contains("Theorem valid_output : ValidModule output."),
            "the module theorem must always be present:\n{v}"
        );
    }
}

// Scenario 11: Over-long spec name rejected at codegen (D2)
#[cfg(test)]
mod scenario_11_overlong_spec_name {
    use crate::utils::build_ast;
    use inference_type_checker::TypeCheckerBuilder;
    use inference_wasm_codegen::{CompilationMode, OptLevel, Target};

    /// Both `inference.spec_funcs` decoders (the linker and the Rocq
    /// translator) reject a spec name longer than 255 bytes. Codegen must
    /// refuse to emit such a name up front rather than produce a `.wasm`
    /// artifact that fails its own downstream link/translate step.
    #[test]
    fn spec_name_over_255_bytes_is_rejected_at_codegen() {
        let long_name = "S".repeat(256);
        let source = format!(
            "fn foo(x: i32) -> i32 {{ return x; }}\n\
             spec {long_name} {{\n  \
                 fn prop() forall {{\n    \
                     let i: i32 = @;\n    \
                     assert(foo(i) == i);\n  \
                 }}\n\
             }}\n"
        );

        let arena = build_ast(source);
        let typed_context = TypeCheckerBuilder::build_typed_context(arena)
            .expect("type check should succeed")
            .typed_context();
        let err = inference_wasm_codegen::codegen(
            &typed_context,
            Target::Wasm32,
            CompilationMode::Proof,
            OptLevel::O3,
            "output",
        )
        .expect_err("codegen must reject a spec name exceeding 255 bytes");

        let msg = err.to_string();
        assert!(
            msg.contains("256") && msg.contains("255"),
            "expected a spec-name-length diagnostic citing 256 and the 255 cap; got: {msg}"
        );
    }

    /// A spec name at exactly the 255-byte cap is emitted normally: the limit
    /// is inclusive, mirroring both decoders' `len() > MAX` rejection.
    #[test]
    fn spec_name_at_255_bytes_is_accepted() {
        let name = "S".repeat(255);
        let source = format!(
            "fn foo(x: i32) -> i32 {{ return x; }}\n\
             spec {name} {{\n  \
                 fn prop() forall {{\n    \
                     let i: i32 = @;\n    \
                     assert(foo(i) == i);\n  \
                 }}\n\
             }}\n"
        );

        let arena = build_ast(source);
        let typed_context = TypeCheckerBuilder::build_typed_context(arena)
            .expect("type check should succeed")
            .typed_context();
        inference_wasm_codegen::codegen(
            &typed_context,
            Target::Wasm32,
            CompilationMode::Proof,
            OptLevel::O3,
            "output",
        )
        .expect("a 255-byte spec name is at the cap and must be accepted");
    }
}
