//! Invariant: anything `inference_wasm_linker::link()` accepts is either lowered
//! to Rocq by `wasm-to-v` or rejected with a recoverable error — never a panic.
//!
//! The linker copies the main module's body verbatim and folds external function
//! bodies in after gating every operator through the fail-closed allow-list
//! (`crate::safety::check_operator`). The paired downstream phase, the `wasm-to-v`
//! translator, lowers that linked output to a Rocq `.v` proof artifact. The two
//! must agree on the instruction set: every operator the linker is willing to
//! emit into its output must have a translator lowering. An operator the linker
//! admits but the translator hits `todo!()` on is a latent SIGABRT on the `-v`
//! proof path — a clean link followed by an unrecoverable crash in the next phase.
//!
//! This test pins that agreement. For each allow-listed opcode family it links a
//! fixture that drives an operator of that family into the linked output, then
//! translates the output under `std::panic::catch_unwind`. A `todo!()` in the
//! translator surfaces here as a labeled test failure naming the family, rather
//! than as an opaque abort deep in a later compilation.
//!
//! ## Audit that motivated this test
//!
//! The allow-list was audited against the translator's operator match. Several
//! families were allow-listed (or admitted at the feature gate) yet reached a
//! `todo!()` in the translator; they have since been removed from the
//! allow-list / feature gate so the two phases agree:
//!
//! - **saturating float-to-int truncations** (8 opcodes:
//!   `i32`/`i64`.`trunc_sat`_`f32`/`f64`_`s`/`u`),
//! - **tail calls** (`return_call`, `return_call_indirect`),
//! - **segment-indexed table initialization** (`table.init`, `elem.drop`,
//!   `table.copy`),
//! - **all floating-point** operators and value types (`f32`/`f64`).
//!
//! Each of those is rejected before reaching the merge, so it can never enter a
//! linked output. The corpus below covers only what the linker admits.
//!
//! ## The numeric envelope, restored in lockstep
//!
//! Two integer families were retracted alongside the list above and have since
//! been restored, both phases together:
//!
//! - **sign-extension** (5 opcodes: `i32.extend8_s`, `i32.extend16_s`,
//!   `i64.extend8_s`, `i64.extend16_s`, `i64.extend32_s`), which the proof model
//!   spells as an ordinary unop, `BI_unop t (Unop_extend n)` — not as a
//!   conversion. That misclassification is why they were grouped with the
//!   conversion block and retracted with it.
//! - **integer width conversions** (`i32.wrap_i64`, `i64.extend_i32_s`,
//!   `i64.extend_i32_u`), which the proof model spells as `BI_cvtop` with the
//!   `CVO_wrap`/`CVO_extend` constructors.
//!
//! The earlier retraction was correct at the time: the translator emitted
//! `BI_cvtop` against a contract that declared no such constructor, so the
//! allow-list premise ("an allow-listed family has a translator lowering") held
//! at the Rust level and failed at `coqc`. What changed is the contract, not the
//! premise — the `coqc` gate in `tests/src/rocq_typecheck.rs` now elaborates both
//! families. [`integer_width_conversions_translate`] and
//! [`sign_extension_operators_translate`] pin the restored lowerings here.
//! The float-naming conversions stay rejected: the contract declares no float
//! number type for them to mention.
//!
//! ## How the corpus drives operators into the output
//!
//! Most families are exercised in the **main module**, which the linker re-encodes
//! verbatim into the output — the surest way to guarantee a specific operator
//! reaches the translator, since an external body must additionally survive tier
//! classification (a memory access through a non-parameter address is rejected as
//! Tier C). Direct `call` is inherent in every fixture (the main module calls the
//! satisfied import, whose body is merged in).
//!
//! ## Non-deterministic instructions are translatable only by omission
//!
//! The verification-only opcodes (`forall`/`exists`/`assume`/`unique` and
//! `i32`/`i64.uzumaki`) have no counterpart in the vanilla WasmCert proof model
//! `wasm-to-v` targets. They reach the `.v` path only inside a `spec` function's
//! body, which the translator OMITS from the module record entirely — so they are
//! "translatable" purely by not being emitted. A non-det instruction in a
//! surviving (executable, non-spec) body has no lowering and is a fail-closed
//! `wasm-to-v` rejection. The linker still admits these opcodes (its allow-list is
//! unchanged, so a main module carrying them still links); the phase agreement
//! they must uphold is therefore that such an output *is rejected*, not that it
//! translates — [`proof_path_nondet_and_uzumaki_is_rejected`] pins that, and
//! [`proof_path_unique_is_rejected_at_translation`] pins the `unique` arm in
//! isolation (the combined fixture rejects on its first opcode, so `unique`
//! would otherwise ride along untested).

use inference_wasm_linker::link as raw_link;
use inference_wasm_to_v_translator::wasm_parser::translate_bytes;
use rustc_hash::FxHashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Assembles a `.wasm` binary from WAT source, panicking with the WAT on error.
fn wasm(wat: &str) -> Vec<u8> {
    wat::parse_str(wat).unwrap_or_else(|e| panic!("invalid WAT fixture: {e}\n{wat}"))
}

/// The pure `mathlib` external every fixture links against: it exports
/// `sum:(i32,i32)->i32`, the import each main module satisfies. Reused so the
/// link always has a body to merge (exercising the direct-call path) without each
/// fixture restating the library.
fn mathlib_sum() -> Vec<u8> {
    wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
          (export "sum" (func 0)))
        "#,
    )
}

/// Links `main` against `mathlib_sum`, satisfying its `mathlib::sum` import, and
/// asserts the link succeeds. The logical module label must match the import's
/// recorded module (`mathlib`) so the merge resolves the import against the
/// external.
fn link_against_mathlib(main: &[u8]) -> Vec<u8> {
    let lib = mathlib_sum();
    raw_link(main, &[("mathlib", &lib)], None)
        .unwrap_or_else(|e| panic!("link must accept the fixture, got {e:?}"))
}

/// The invariant check for one corpus entry: the linked output of `main` must
/// translate to Rocq without panicking. `translate_bytes` is run under
/// `catch_unwind`, so a `todo!()` for an unlowered operator surfaces as a labeled
/// failure naming the opcode family rather than an opaque process abort.
///
/// `Ok(Ok(_))` is the only acceptance: the closure must not panic (a `todo!()`
/// would make `catch_unwind` return `Err`) *and* the translation must succeed (a
/// recoverable `Err` would mean the operator is rejected rather than lowered,
/// which for an allow-listed family is itself a phase-disagreement worth flagging).
fn assert_output_translates(label: &str, main: &[u8]) {
    let linked = link_against_mathlib(main);

    let result = catch_unwind(AssertUnwindSafe(|| {
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let empty_hspecs = inference_hassert::HSpecMap::default();
        translate_bytes("Prog", &linked, &empty, &empty_hspecs)
    }));

    match result {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => panic!(
            "{label}: the linker accepted this output but wasm-to-v rejected it with a \
             recoverable error ({e:?}); an allow-listed family must have a translator \
             lowering, so the allow-list and the translator have diverged"
        ),
        Err(_) => panic!(
            "{label}: the linker accepted this output but wasm-to-v PANICKED translating it \
             (an unlowered operator hit `todo!()`); this family is allow-listed in \
             core/wasm-linker/src/safety.rs without a translator lowering — either add the \
             lowering in core/wasm-to-v/src/translator.rs or remove the family from the \
             allow-list"
        ),
    }
}

/// The dual of [`assert_output_translates`] for the non-deterministic families:
/// the linker accepts `main`, but because the opcodes sit in a surviving
/// (non-spec) body, `wasm-to-v` must reject the linked output with a recoverable
/// [`inference_wasm_to_v_translator::errors::WasmToVError::UnsupportedFeature`]
/// naming the vanilla-WasmCert limitation — never a panic, never a silent
/// success.
fn assert_output_rejected_as_nondet(label: &str, main: &[u8]) {
    let linked = link_against_mathlib(main);

    let result = catch_unwind(AssertUnwindSafe(|| {
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let empty_hspecs = inference_hassert::HSpecMap::default();
        translate_bytes("Prog", &linked, &empty, &empty_hspecs)
    }));

    match result {
        Ok(Err(e)) => {
            let downcast = e.downcast_ref::<inference_wasm_to_v_translator::errors::WasmToVError>();
            assert!(
                matches!(
                    downcast,
                    Some(
                        inference_wasm_to_v_translator::errors::WasmToVError::UnsupportedFeature { .. }
                    )
                ),
                "{label}: a non-deterministic instruction in a surviving body must be a \
                 recoverable UnsupportedFeature rejection; got {e:?}"
            );
        }
        Ok(Ok(_)) => panic!(
            "{label}: a non-deterministic instruction in a surviving (non-spec) body must be \
             rejected by wasm-to-v (the vanilla WasmCert proof model has no such construct), \
             but translation succeeded"
        ),
        Err(_) => panic!(
            "{label}: wasm-to-v PANICKED on a non-deterministic instruction instead of \
             returning a recoverable UnsupportedFeature error"
        ),
    }
}

/// The dual of [`assert_output_translates`] for a family the allow-list has
/// retracted: the linker must refuse `main` outright, with a
/// [`inference_wasm_linker::LinkError::UnsupportedConstruct`] whose message names
/// the family. Retracting a family from `is_numeric` leaves it to the fail-closed
/// `other =>` arm of `safety::check_operator`, which reaches every re-encoded body
/// including the main module's — so the rejection lands here, one phase earlier
/// than the translator's, and no linked output is ever produced.
///
/// `family` is the label the linker's `operator_family` carries; asserting on it
/// rather than on the whole sentence keeps the test tied to the family
/// classification instead of the diagnostic's phrasing.
fn assert_link_rejected_as_unmodeled(label: &str, main: &[u8], family: &str) {
    let lib = mathlib_sum();
    let err = raw_link(main, &[("mathlib", &lib)], None)
        .err()
        .unwrap_or_else(|| panic!("{label}: the linker must refuse a retracted family"));

    let msg = format!("{err}");
    assert!(
        msg.contains(family),
        "{label}: the rejection must name the `{family}` family; got {err:?}"
    );
}

/// A main module that imports `mathlib::sum`, runs `body` (a WAT instruction
/// sequence the fixture under test exercises), then calls the import so the link
/// always has a body to merge. `body` runs on a `(param i32 i32) (result i32)`
/// function with no memory; the call's result is the function's result.
///
/// The body's stack effect must be net-zero (every value it pushes it must also
/// consume), so the trailing `call 0` leaves exactly the one `i32` result on the
/// stack.
fn main_with_body(body: &str) -> Vec<u8> {
    wasm(&format!(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32 i32) (result i32)
            {body}
            local.get 0
            local.get 1
            call 0)
          (export "compute" (func 1)))
        "#,
    ))
}

/// Like [`main_with_body`] but the function additionally declares a `(memory 1)`
/// and `extra_locals` (e.g. `(local i64)`), so memory-touching and 64-bit
/// fixtures have an address space and scratch slots. The reconciled output keeps
/// this memory (the pure `mathlib` external declares none).
fn main_with_memory_body(extra_locals: &str, body: &str) -> Vec<u8> {
    wasm(&format!(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (memory (;0;) 1)
          (func (;1;) (type 0) (param i32 i32) (result i32)
            {extra_locals}
            {body}
            local.get 0
            local.get 1
            call 0)
          (export "compute" (func 1)))
        "#,
    ))
}

#[test]
fn structured_control_flow_translates() {
    // block / loop / if / else / br / br_if / br_table / return / unreachable /
    // nop. Each opens or targets a structured region the translator reconstructs
    // into a nested Rocq expression.
    let main = main_with_body(
        r#"
        nop
        block
          i32.const 1
          br_if 0
          br 0
        end
        block
          loop
            br 1
          end
        end
        block
          block
            block
              local.get 0
              br_table 0 1 2
            end
          end
        end
        local.get 0
        i32.eqz
        if (result i32)
          local.get 0
          local.get 1
          call 0
          return
        else
          i32.const 0
        end
        drop
        local.get 0
        i32.eqz
        if
          unreachable
        end
        "#,
    );
    assert_output_translates("structured control flow", &main);
}

#[test]
fn parametric_ops_translate() {
    // drop / select.
    let main = main_with_body(
        r#"
        i32.const 7
        drop
        i32.const 1
        i32.const 2
        i32.const 0
        select
        drop
        "#,
    );
    assert_output_translates("parametric (drop/select)", &main);
}

#[test]
fn locals_translate() {
    // local.get / local.set / local.tee. The two params are the only locals on
    // the base signature; `tee` leaves its value for the trailing `drop`.
    let main = main_with_body(
        r#"
        local.get 0
        local.set 1
        local.get 1
        local.tee 0
        drop
        "#,
    );
    assert_output_translates("locals (get/set/tee)", &main);
}

#[test]
fn direct_call_translates() {
    // `call` is inherent in every fixture (the main body calls the satisfied
    // import, whose body is merged in), so a bare fixture exercises it. The
    // assertion is that the merged call site and the merged `sum` body both lower.
    let main = main_with_body("nop");
    assert_output_translates("direct call", &main);
}

#[test]
fn integer_loads_and_stores_translate() {
    // Every integer load and store width: i32/i64 full-width, and the sub-width
    // signed/unsigned loads and narrowing stores. Each reads or writes the single
    // shared memory the merge folds onto.
    let main = main_with_memory_body(
        "(local i64)",
        r#"
        local.get 0 i32.load drop
        local.get 0 i64.load drop
        local.get 0 i32.load8_s drop
        local.get 0 i32.load8_u drop
        local.get 0 i32.load16_s drop
        local.get 0 i32.load16_u drop
        local.get 0 i64.load8_s drop
        local.get 0 i64.load8_u drop
        local.get 0 i64.load16_s drop
        local.get 0 i64.load16_u drop
        local.get 0 i64.load32_s drop
        local.get 0 i64.load32_u drop
        local.get 0 local.get 1 i32.store
        local.get 0 local.get 1 i32.store8
        local.get 0 local.get 1 i32.store16
        local.get 0 local.get 2 i64.store
        local.get 0 local.get 2 i64.store8
        local.get 0 local.get 2 i64.store16
        local.get 0 local.get 2 i64.store32
        "#,
    );
    assert_output_translates("integer loads/stores", &main);
}

#[test]
fn memory_ops_translate() {
    // memory.size / memory.grow / memory.copy / memory.fill over the single
    // shared memory.
    let main = main_with_memory_body(
        "",
        r#"
        memory.size drop
        local.get 0 memory.grow drop
        local.get 0 local.get 1 i32.const 4 memory.copy
        local.get 0 i32.const 0 i32.const 4 memory.fill
        "#,
    );
    assert_output_translates("memory size/grow/copy/fill", &main);
}

#[test]
fn integer_constants_translate() {
    // i32.const / i64.const.
    let main = main_with_body(
        r#"
        i32.const -1
        drop
        i64.const 9223372036854775807
        drop
        "#,
    );
    assert_output_translates("integer constants", &main);
}

#[test]
fn i32_comparisons_translate() {
    // i32: eqz / eq / ne / lt_s / lt_u / gt_s / gt_u / le_s / le_u / ge_s / ge_u.
    let main = main_with_body(
        r#"
        local.get 0 i32.eqz drop
        local.get 0 local.get 1 i32.eq drop
        local.get 0 local.get 1 i32.ne drop
        local.get 0 local.get 1 i32.lt_s drop
        local.get 0 local.get 1 i32.lt_u drop
        local.get 0 local.get 1 i32.gt_s drop
        local.get 0 local.get 1 i32.gt_u drop
        local.get 0 local.get 1 i32.le_s drop
        local.get 0 local.get 1 i32.le_u drop
        local.get 0 local.get 1 i32.ge_s drop
        local.get 0 local.get 1 i32.ge_u drop
        "#,
    );
    assert_output_translates("i32 comparisons", &main);
}

#[test]
fn i64_comparisons_translate() {
    // i64: eqz / eq / ne / lt_s / lt_u / gt_s / gt_u / le_s / le_u / ge_s / ge_u.
    let main = main_with_memory_body(
        "(local i64) (local i64)",
        r#"
        local.get 2 i64.eqz drop
        local.get 2 local.get 3 i64.eq drop
        local.get 2 local.get 3 i64.ne drop
        local.get 2 local.get 3 i64.lt_s drop
        local.get 2 local.get 3 i64.lt_u drop
        local.get 2 local.get 3 i64.gt_s drop
        local.get 2 local.get 3 i64.gt_u drop
        local.get 2 local.get 3 i64.le_s drop
        local.get 2 local.get 3 i64.le_u drop
        local.get 2 local.get 3 i64.ge_s drop
        local.get 2 local.get 3 i64.ge_u drop
        "#,
    );
    assert_output_translates("i64 comparisons", &main);
}

#[test]
fn i32_arithmetic_and_bitwise_translate() {
    // i32: clz / ctz / popcnt / add / sub / mul / div_s / div_u / rem_s / rem_u /
    // and / or / xor / shl / shr_s / shr_u / rotl / rotr.
    let main = main_with_body(
        r#"
        local.get 0 i32.clz drop
        local.get 0 i32.ctz drop
        local.get 0 i32.popcnt drop
        local.get 0 local.get 1 i32.add drop
        local.get 0 local.get 1 i32.sub drop
        local.get 0 local.get 1 i32.mul drop
        local.get 0 local.get 1 i32.div_s drop
        local.get 0 local.get 1 i32.div_u drop
        local.get 0 local.get 1 i32.rem_s drop
        local.get 0 local.get 1 i32.rem_u drop
        local.get 0 local.get 1 i32.and drop
        local.get 0 local.get 1 i32.or drop
        local.get 0 local.get 1 i32.xor drop
        local.get 0 local.get 1 i32.shl drop
        local.get 0 local.get 1 i32.shr_s drop
        local.get 0 local.get 1 i32.shr_u drop
        local.get 0 local.get 1 i32.rotl drop
        local.get 0 local.get 1 i32.rotr drop
        "#,
    );
    assert_output_translates("i32 arithmetic/bitwise", &main);
}

#[test]
fn i64_arithmetic_and_bitwise_translate() {
    // i64: clz / ctz / popcnt / add / sub / mul / div_s / div_u / rem_s / rem_u /
    // and / or / xor / shl / shr_s / shr_u / rotl / rotr.
    let main = main_with_memory_body(
        "(local i64) (local i64)",
        r#"
        local.get 2 i64.clz drop
        local.get 2 i64.ctz drop
        local.get 2 i64.popcnt drop
        local.get 2 local.get 3 i64.add drop
        local.get 2 local.get 3 i64.sub drop
        local.get 2 local.get 3 i64.mul drop
        local.get 2 local.get 3 i64.div_s drop
        local.get 2 local.get 3 i64.div_u drop
        local.get 2 local.get 3 i64.rem_s drop
        local.get 2 local.get 3 i64.rem_u drop
        local.get 2 local.get 3 i64.and drop
        local.get 2 local.get 3 i64.or drop
        local.get 2 local.get 3 i64.xor drop
        local.get 2 local.get 3 i64.shl drop
        local.get 2 local.get 3 i64.shr_s drop
        local.get 2 local.get 3 i64.shr_u drop
        local.get 2 local.get 3 i64.rotl drop
        local.get 2 local.get 3 i64.rotr drop
        "#,
    );
    assert_output_translates("i64 arithmetic/bitwise", &main);
}

#[test]
fn integer_width_conversions_translate() {
    // i32.wrap_i64 / i64.extend_i32_s / i64.extend_i32_u. Each is exercised in
    // isolation rather than in one body, so a lowering that exists for only one
    // of the three cannot be carried by its neighbours: the linked output for
    // each fixture contains exactly one conversion.
    for (op, operand) in [
        ("i32.wrap_i64", "local.get 2"),
        ("i64.extend_i32_s", "local.get 0"),
        ("i64.extend_i32_u", "local.get 0"),
    ] {
        let main = main_with_memory_body("(local i64)", &format!("{operand} {op} drop"));
        assert_output_translates(op, &main);
    }
}

#[test]
fn sign_extension_operators_translate() {
    // The five sign-extension opcodes, each in its own fixture for the same
    // reason as the conversions above. They are unops in the proof model, so
    // their lowering lives beside `clz`/`ctz`/`popcnt` rather than beside the
    // conversions — a distinction the earlier grouping got wrong, and the reason
    // this test is separate from [`integer_width_conversions_translate`] rather
    // than folded into it.
    for (op, operand) in [
        ("i32.extend8_s", "local.get 0"),
        ("i32.extend16_s", "local.get 0"),
        ("i64.extend8_s", "local.get 2"),
        ("i64.extend16_s", "local.get 2"),
        ("i64.extend32_s", "local.get 2"),
    ] {
        let main = main_with_memory_body("(local i64)", &format!("{operand} {op} drop"));
        assert_output_translates(op, &main);
    }
}

#[test]
fn tail_calls_are_rejected() {
    // The retraction direction of the lockstep contract, kept live now that the
    // numeric families have moved to the translating side. `return_call` has no
    // translator lowering, so the linker must refuse it outright rather than
    // produce an output the `-v` path cannot render.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            local.get 1
            return_call 0)
          (export "compute" (func 1)))
        "#,
    );
    assert_link_rejected_as_unmodeled("return_call", &main, "tail calls (return_call)");
}

#[test]
fn main_globals_translate() {
    // global.get / global.set on a main-side mutable global. Globals live on the
    // main module (a Tier-C external carrying its own globals is rejected), so the
    // fixture declares the global itself.
    let main = wasm(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (global (;0;) (mut i32) (i32.const 0))
          (func (;1;) (type 0) (param i32 i32) (result i32)
            local.get 0
            global.set 0
            global.get 0
            drop
            local.get 0
            local.get 1
            call 0)
          (export "compute" (func 1)))
        "#,
    );
    assert_output_translates("main globals (get/set)", &main);
}

#[test]
fn ref_func_translates() {
    // `ref.func <idx>` pushes a function reference, which the translator lowers to
    // `BI_ref_func`. It needs no table: the reference names an exported function
    // (the export declares it referenceable), so the operator survives the link
    // into the output even though the merge preserves no table section. The pushed
    // reference is immediately dropped to keep the body's stack net-zero. Pinned
    // so a future translator regression on this opcode surfaces here.
    //
    // Function 1 is this fixture's own exported local (`compute`); function 0 is
    // the satisfied import, merged in. Referencing 1 keeps the reference valid in
    // the linked output's index space.
    let main = main_with_body(
        r#"
        ref.func 1
        drop
        "#,
    );
    assert_output_translates("ref.func", &main);
}

// `call_indirect` is allow-listed (and translatable: `wasm-to-v` lowers it), but
// it cannot appear in a linkable *output* today: the merge preserves no
// `TableSection`, and a main-side table is now rejected outright (alongside the
// already-rejected main-side element segment — see `merge::Plan::build`). So any
// output `call_indirect` would reference a non-existent table; there is no linked
// output in which to exercise the opcode, and a corpus entry would only assert
// the merge's table rejection, not v-alignment. When the merge gains table
// preservation, add a `call_indirect` entry here.

#[test]
fn proof_path_nondet_and_uzumaki_is_rejected() {
    // The verification-only proof-path opcodes (forall/exists/assume and
    // i32.uzumaki/i64.uzumaki) have no counterpart in the vanilla WasmCert proof
    // model. Here they sit in the main module's *surviving* (non-spec) body — the
    // module carries no `inference.spec_funcs` section, so nothing is omitted —
    // and `wasm-to-v` must reject the linked output rather than translate it. The
    // linker still admits the opcodes (its allow-list is unchanged), so the phase
    // agreement is a clean rejection, not a lowering. Rejection fires on the
    // first opcode reached, so `unique` (`0xfc 0x3d`) is exercised in isolation
    // by [`proof_path_unique_is_rejected_at_translation`]. `wat` cannot assemble
    // these custom `0xfc`-prefixed opcodes, so the body is hand-encoded.
    let main = proof_mode_main_with_nondet_and_uzumaki();
    assert_output_rejected_as_nondet("non-det blocks + uzumaki (proof path)", &main);
}

/// Builds a proof-mode MAIN module that imports `mathlib::sum` and whose own
/// exported body carries the translatable verification-only opcodes the proof
/// path uses — the three non-det blocks (`forall`/`exists`/`assume`) and both
/// uzumaki rvalues (`i32.uzumaki`/`i64.uzumaki`) — alongside an executable `call`
/// to the import. `unique` (`0xfc 0x3d`) is deliberately excluded: rejection
/// fires on the first opcode reached, so it is exercised in isolation by its
/// own fixture ([`main_with_unique_block`]) rather than riding along untested
/// here. `wat` cannot assemble the custom opcodes, so the module is
/// hand-encoded byte-by-byte, mirroring the encoding in `link.rs`.
fn proof_mode_main_with_nondet_and_uzumaki() -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, ImportSection,
        Instruction, Module, TypeSection, ValType,
    };

    let mut module = Module::new();

    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    module.section(&types);

    let mut imports = ImportSection::new();
    imports.import("mathlib", "sum", EntityType::Function(0));
    module.section(&imports);

    let mut funcs = FunctionSection::new();
    funcs.function(0);
    module.section(&funcs);

    let mut exports = ExportSection::new();
    // The import is output index 0; the local function is index 1.
    exports.export("compute", ExportKind::Func, 1);
    module.section(&exports);

    let mut code = CodeSection::new();
    let mut f = Function::new([]);
    // Each non-det block (`0xfc <sub_opcode> 0x40` = empty block type) opens a
    // region closed by `End`; the empty block has no stack effect. `unique`
    // (`0x3d`) is omitted here — its rejection is pinned separately.
    for sub_opcode in [0x3a_u8, 0x3b, 0x3c] {
        f.raw([0xfc, sub_opcode, 0x40]);
        f.instruction(&Instruction::End);
    }
    // Each uzumaki rvalue (`0xfc <sub_opcode>`) pushes a value, dropped to keep
    // the stack balanced.
    f.raw([0xfc, 0x31]); // i32.uzumaki
    f.instruction(&Instruction::Drop);
    f.raw([0xfc, 0x32]); // i64.uzumaki
    f.instruction(&Instruction::Drop);
    // Executable tail: sum(arg0, arg1) via the (to-be-merged) import.
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::Call(0));
    f.instruction(&Instruction::End);
    code.function(&f);
    module.section(&code);

    module.finish()
}

#[test]
fn proof_path_unique_is_rejected_at_translation() {
    // `unique` (0xfc 0x3d) links fine in a main module's proof scaffolding but
    // has no honest Rocq lowering, so the linked output must be rejected by
    // wasm-to-v with a recoverable error. `wat` cannot assemble the custom
    // opcode, so the body is hand-encoded.
    let main = main_with_unique_block();
    assert_output_rejected_as_nondet("unique block (proof path)", &main);
}

/// Builds a proof-mode MAIN module that imports `mathlib::sum` and whose own
/// exported body carries a single `unique` block (`0xfc 0x3d 0x40`) closed by
/// `End`, alongside an executable `call` to the import. The linker admits this
/// (a main module's proof scaffolding is re-encoded verbatim), but wasm-to-v
/// must reject it — the vanilla WasmCert proof model has no constructor to
/// lower it to. `wat` cannot assemble the custom opcode, so the module is
/// hand-encoded, mirroring [`proof_mode_main_with_nondet_and_uzumaki`].
fn main_with_unique_block() -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, ImportSection,
        Instruction, Module, TypeSection, ValType,
    };

    let mut module = Module::new();

    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    module.section(&types);

    let mut imports = ImportSection::new();
    imports.import("mathlib", "sum", EntityType::Function(0));
    module.section(&imports);

    let mut funcs = FunctionSection::new();
    funcs.function(0);
    module.section(&funcs);

    let mut exports = ExportSection::new();
    // The import is output index 0; the local function is index 1.
    exports.export("compute", ExportKind::Func, 1);
    module.section(&exports);

    let mut code = CodeSection::new();
    let mut f = Function::new([]);
    // The one `unique` block (`0xfc 0x3d 0x40` = empty block type), closed by
    // `End`; the empty block has no stack effect.
    f.raw([0xfc, 0x3d, 0x40]);
    f.instruction(&Instruction::End);
    // Executable tail: sum(arg0, arg1) via the (to-be-merged) import.
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::LocalGet(1));
    f.instruction(&Instruction::Call(0));
    f.instruction(&Instruction::End);
    code.function(&f);
    module.section(&code);

    module.finish()
}

/// A main module whose exported body nests `depth` empty `block` regions, then
/// calls `mathlib::sum` so the link has an executable tail. Used to pin the
/// structured-control-flow depth cap on the main re-encode path: the linker must
/// reject a body the downstream wasm-to-v translator (which recurses one frame
/// per level) cannot render, so the v-alignment invariant — anything linkable is
/// translatable — holds at the cap boundary as well as below it.
fn main_with_nested_blocks(depth: usize) -> Vec<u8> {
    let mut body = String::new();
    for _ in 0..depth {
        body.push_str("block ");
    }
    for _ in 0..depth {
        body.push_str("end ");
    }
    wasm(&format!(
        r#"
        (module
          (type (;0;) (func (param i32 i32) (result i32)))
          (import "mathlib" "sum" (func (;0;) (type 0)))
          (func (;1;) (type 0) (param i32 i32) (result i32)
            {body}
            local.get 0
            local.get 1
            call 0)
          (export "compute" (func 1)))
        "#,
    ))
}

#[test]
fn main_body_at_the_control_depth_cap_links_and_translates() {
    // A main body nested one level below the cap must both link and translate.
    // The closure scan and the wasm-to-v translator both admit nesting strictly
    // below 256 levels; the main re-encode path must agree, so a legitimately
    // deep (but in-bounds) body is never spuriously rejected.
    let main = main_with_nested_blocks(255);
    assert_output_translates("main body at the control-depth cap", &main);
}

#[test]
fn main_body_past_the_control_depth_cap_is_rejected_before_translation() {
    // A main body nested at the cap must be rejected by the linker, not linked
    // and then rejected by wasm-to-v. The main re-encode path previously left
    // the depth cap unenforced, so such a body linked cleanly and only failed
    // downstream — violating the invariant that anything linkable is
    // translatable. The link must now reject it up front.
    let main = main_with_nested_blocks(256);
    let lib = mathlib_sum();
    let err = raw_link(&main, &[("mathlib", &lib)], None)
        .expect_err("a main body past the control-depth cap must be rejected by the linker");
    match err {
        inference_wasm_linker::LinkError::UnsupportedConstruct(msg) => assert!(
            msg.contains("256") && msg.contains("control"),
            "expected an UnsupportedConstruct naming the control-depth limit, got {msg:?}"
        ),
        other => panic!("expected UnsupportedConstruct, got {other:?}"),
    }
}
