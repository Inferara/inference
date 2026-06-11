//! Unit tests for the sound address-provenance analysis.
//!
//! The matrix below is the design's full adversarial test set. Each case is a
//! single-function WAT body (or, for the interprocedural cases, a whole module)
//! checked against the analysis. The naming convention follows the design:
//!
//! - **ACCEPT** — every address operand is provably param-derived on every path.
//! - **REJECT** — at least one address operand may be a fabricated, caller-
//!   independent value; the closure is Tier C.
//!
//! The legitimate cases (`8a`) must stay accepted; every laundering case
//! (`8b`–`8h`) must be rejected. Over-rejections that the design documents as
//! sound (a unary-converted pointer, alignment masking) are asserted REJECT.

use super::*;
use inf_wasmparser::{Parser, Payload};

/// Assembles `wat` and returns the raw body bytes of its first function.
fn first_body(wat: &str) -> Vec<u8> {
    let bytes = wat::parse_str(wat).expect("valid WAT");
    for payload in Parser::new(0).parse_all(&bytes) {
        if let Payload::CodeSectionEntry(body) = payload.expect("payload") {
            return body.as_bytes().to_vec();
        }
    }
    panic!("no code section");
}

/// Parses `wat` into the linker's owned module representation.
fn module(wat: &str) -> ParsedModule {
    let bytes = wat::parse_str(wat).expect("valid WAT");
    ParsedModule::parse(&bytes).expect("parse")
}

/// Runs the single-function analysis over the first function of `wat`. An empty
/// module is used for call resolution; the call cases that need a resolvable
/// callee use [`accepts_in`] with an explicit module.
fn accepts(wat: &str, params: usize) -> bool {
    let m = ParsedModule::default();
    let body = first_body(wat);
    function_is_param_addressing(&m, &body, params).expect("analysis runs")
}

/// Parses `module_wat` and runs the analysis over function `func_index`'s body
/// with `params` leading parameters, so its calls resolve against the module.
fn accepts_in(module_wat: &str, func_index: u32, params: usize) -> bool {
    let m = module(module_wat);
    let local = &m.local_funcs[(func_index - m.local_func_base()) as usize];
    function_is_param_addressing(&m, &local.body, params).expect("analysis runs")
}

// ===========================================================================
// 8a — MUST-ACCEPT: legitimate, sound param-addressing
// ===========================================================================

#[test]
fn a1_direct_param_load() {
    assert!(accepts(
        r#"(module (memory 1) (func (param i32) (result i32)
             local.get 0 i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn a2_param_plus_const_struct_field() {
    assert!(accepts(
        r#"(module (memory 1) (func (param i32) (result i32)
             local.get 0 i32.const 8 i32.add i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn a3_param_base_with_nonzero_memarg_offset() {
    assert!(accepts(
        r#"(module (memory 1) (func (param i32) (result i32)
             local.get 0 i32.load offset=12) (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn a4_store_through_param() {
    assert!(accepts(
        r#"(module (memory 1) (func (param i32 i32)
             local.get 0 local.get 1 i32.store) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn a5_param_plus_const_store_with_memarg() {
    assert!(accepts(
        r#"(module (memory 1) (func (param i32 i32)
             local.get 0 i32.const 16 i32.add local.get 1 i32.store offset=4)
           (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn a6_ptr_plus_param_len_add_propagates() {
    // The headline ptr+len case: `add` with either operand Param is Param.
    assert!(accepts(
        r#"(module (memory 1) (func (param i32 i32) (result i32)
             local.get 0 local.get 1 i32.add i32.load) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn a7_param_minus_const() {
    // `param - const`: minuend Param, subtrahend NotParam => Param.
    assert!(accepts(
        r#"(module (memory 1) (func (param i32) (result i32)
             local.get 0 i32.const 4 i32.sub i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn a8_param_copied_through_scratch_local() {
    assert!(accepts(
        r#"(module (memory 1) (func (param i32) (result i32) (local i32)
             local.get 0 local.set 1 local.get 1 i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn a9_param_through_local_tee() {
    // `local.tee` re-pushes the Param value, which then addresses the load.
    assert!(accepts(
        r#"(module (memory 1) (func (param i32) (result i32) (local i32)
             local.get 0 local.tee 1 i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn a11_select_of_two_params() {
    // select(param0, param1) => join(Param, Param) = Param.
    assert!(accepts(
        r#"(module (memory 1) (func (param i32 i32) (result i32)
             local.get 0 local.get 1 i32.const 1 select i32.load)
           (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn a12_memory_fill_at_param() {
    assert!(accepts(
        r#"(module (memory 1) (func (param i32 i32 i32)
             local.get 0 local.get 1 local.get 2 memory.fill)
           (export "f" (func 0)))"#,
        3,
    ));
}

#[test]
fn a13_memory_copy_both_params() {
    assert!(accepts(
        r#"(module (memory 1) (func (param i32 i32 i32)
             local.get 0 local.get 1 local.get 2 memory.copy)
           (export "f" (func 0)))"#,
        3,
    ));
}

#[test]
fn a14_param_as_block_result() {
    // A param produced inside a block and left as the block's single result must
    // survive as Param into the enclosing load.
    assert!(accepts(
        r#"(module (memory 1) (func (param i32) (result i32)
             (block (result i32) local.get 0) i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn a15_if_both_arms_param() {
    // Both arms yield the param => join keeps Param.
    assert!(accepts(
        r#"(module (memory 1) (func (param i32 i32) (result i32)
             local.get 1
             (if (result i32) (then local.get 0) (else local.get 0))
             i32.load) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn a16_loop_result_stays_param() {
    // A degenerate loop whose body yields the param; the fixpoint keeps Param.
    assert!(accepts(
        r#"(module (memory 1) (func (param i32) (result i32)
             (loop (result i32) local.get 0) i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn a17_pure_function_no_memory() {
    // No memory access at all: trivially safe (Tier A in practice).
    assert!(accepts(
        r#"(module (func (param i32 i32) (result i32)
             local.get 0 local.get 1 i32.add) (export "f" (func 0)))"#,
        2,
    ));
}

// ===========================================================================
// 8b — MUST-REJECT: C-2 param-nulling arithmetic
// ===========================================================================

#[test]
fn n1_param_minus_param_is_zero() {
    // (param - param) + const = const: sub with a Param subtrahend is NotParam.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32)
             local.get 0 local.get 0 i32.sub i32.const 65536 i32.add
             local.get 1 i32.store) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn n2_param_times_zero() {
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32)
             local.get 0 i32.const 0 i32.mul i32.const 4096 i32.add
             local.get 1 i32.store) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn n3_param_and_zero() {
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32 i32)
             local.get 0 i32.const 0 i32.and i32.const 32768 i32.add
             local.get 1 local.get 2 memory.fill) (export "f" (func 0)))"#,
        3,
    ));
}

#[test]
fn n4_param_xor_param_via_tee() {
    // param ^ param = 0, laundered through local.tee, then + const.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32) (result i32) (local i32)
             local.get 0 local.tee 1 local.get 1 i32.xor
             i32.const 49152 i32.add i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn n5_param_shl_then_and() {
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32) (result i32)
             local.get 0 i32.const 5 i32.shl i32.const 1024 i32.and i32.load)
           (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn n6_param_div_param_is_one() {
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32) (result i32)
             local.get 0 local.get 0 i32.div_u i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn n7_param_eqz() {
    // eqz yields 0/1, a caller-independent address.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32) (result i32)
             local.get 0 i32.eqz i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn n8_param_wrap_i64_unary_over_rejection() {
    // Documented sound over-rejection: a width conversion erases Param.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i64) (result i32)
             local.get 0 i32.wrap_i64 i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

// ===========================================================================
// 8b' — MUST-REJECT: add-side algebraic cancellation `(C - p) + p == C`
//
// The round-2 `sub` rule correctly demotes `const - param` to NotParam, but the
// value it produces is `C - p` (a *negated* parameter), not a constant. Adding
// `p` back recovers the caller-independent constant `C`. The `add` rule must
// therefore never re-promote a `Param + NotParam` to `Param`; only a proven
// `Const` addend keeps the base `Param`. Every case below stores/loads at a
// fixed absolute address regardless of the caller's pointer and MUST reject.
// (The mirror `(C + p) - p` was already correctly rejected by the `sub` rule;
// `cancel7` re-asserts that to pin the symmetry.)
// ===========================================================================

#[test]
fn cancel1_const_minus_param_plus_param_store() {
    // (C - p) + p == C. `i32.const 65536; local.get 0; i32.sub` = C - p
    // (NotParam), then `local.get 0; i32.add` re-adds p. Must NOT re-promote.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32)
             i32.const 65536 local.get 0 i32.sub local.get 0 i32.add
             local.get 1 i32.store) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn cancel2_param_plus_const_minus_param_store() {
    // p + (C - p) == C. The commuted operand order: the param is the first
    // `add` operand and the `(C - p)` NotParam is on top.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32)
             local.get 0 i32.const 65536 local.get 0 i32.sub i32.add
             local.get 1 i32.store) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn cancel3_bulk_memory_fill_variant() {
    // (C - p) + p == C addressing a memory.fill destination.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32 i32)
             i32.const 65536 local.get 0 i32.sub local.get 0 i32.add
             local.get 1 local.get 2 memory.fill) (export "f" (func 0)))"#,
        3,
    ));
}

#[test]
fn cancel4_const_minus_param_laundered_through_local() {
    // (C - p) parked in local 2, then `local.get 2; local.get 0; i32.add`
    // reconstitutes the constant. The local must carry NotParam, not Param.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32) (result i32) (local i32)
             i32.const 65536 local.get 0 i32.sub local.set 2
             local.get 2 local.get 0 i32.add i32.load) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn cancel5_two_param_slots_store_at_other() {
    // (C - p0) + p0 == C, stored at p1: the cancelled address is independent of
    // BOTH params; only the value path uses a genuine param.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32)
             i32.const 65536 local.get 0 i32.sub local.get 0 i32.add
             local.get 1 i32.store) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn cancel6_const_minus_param_load_directly() {
    // The `(C - p)` value itself is NotParam, so loading through it is rejected
    // even without the re-adding `add` — pins the `sub`-side classification.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32) (result i32)
             i32.const 65536 local.get 0 i32.sub i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn cancel7_mirror_const_plus_param_minus_param_rejected() {
    // The already-sound mirror `(C + p) - p == C`: `i32.const C; local.get 0;
    // i32.add` = Param, then `local.get 0; i32.sub` = Param - Param = NotParam.
    // Asserted to lock the symmetry the `add` fix restores.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32)
             i32.const 65536 local.get 0 i32.add local.get 0 i32.sub
             local.get 1 i32.store) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn cancel8_param_plus_const_offset_still_accepted() {
    // Soundness must not over-reject the legitimate `param + const` it protects:
    // a genuine struct-field offset stays Param. (Mirrors a2, re-asserted in the
    // cancellation family so a future regression here is caught alongside it.)
    assert!(accepts(
        r#"(module (memory 1) (func (param i32) (result i32)
             local.get 0 i32.const 12 i32.add i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

// ===========================================================================
// 8b'' — MUST-REJECT: sub-side algebraic cancellation `p - (p - C) == C`
//
// The mirror of the add-side cancellation family above. `Param - NotParam` must
// NOT preserve `Param`: `NotParam` means *not provably constant*, so the
// subtrahend may itself be a negated/offset parameter such as `p - C`, and
// `p - (p - C) == C` is a fixed, caller-independent absolute address. Only a
// proven `Const` subtrahend keeps the minuend's param-derivation. Each case
// below addresses a constant regardless of the caller's pointer and MUST reject.
// ===========================================================================

#[test]
fn cancel9_param_minus_param_times_one_is_zero() {
    // `p - (p * 1) == 0`. `p * 1` is a multiply, classified NotParam, but its
    // runtime value is exactly the caller pointer, so the subtraction cancels to
    // the absolute address 0. `Param - NotParam` must NOT re-promote to Param.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32) (result i32)
             local.get 0 local.get 0 i32.const 1 i32.mul i32.sub i32.load)
           (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn cancel10_param_minus_notparam_offset_is_const() {
    // `p - ((p * 1) - C) == C`, the laundering wholly within one function. The
    // subtrahend `(p * 1) - C` is genuinely NotParam (a multiply makes `p * 1`
    // NotParam, and `NotParam - Const` stays NotParam), yet its runtime value is
    // `p - C`. The outer `p - (p - C)` recovers the constant `C` as a store
    // address. `Param - NotParam` must NOT re-promote to Param.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32)
             local.get 0
             local.get 0 i32.const 1 i32.mul i32.const 4096 i32.sub
             i32.sub
             local.get 1 i32.store) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn cancel11_param_minus_helper_result_is_const_store() {
    // The interprocedural form with a STORE: `$s(p) = p - 4096`. A call result is
    // modeled NotParam, so `p - $s(p) == 4096` is a fixed absolute store address
    // that the closure root's caller never supplies. The whole closure must
    // reject rather than admit a fabricated host-memory write as Tier B.
    let m = module(
        r#"
        (module
          (memory (;0;) 1)
          (type (;0;) (func (param i32)))
          (type (;1;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32)
            local.get 0
            local.get 0
            call 1
            i32.sub
            i32.const 1234
            i32.store)
          (func (;1;) (type 1) (param i32) (result i32)
            local.get 0 i32.const 4096 i32.sub)
          (export "writer" (func 0)))
        "#,
    );
    let err = verify_param_addressing(&m, &[0, 1], 0, "writer")
        .expect_err("p - (p - C) laundered through a call must be rejected");
    assert!(
        matches!(err, LinkError::RequiresRelocatableBuild { .. }),
        "{err:?}"
    );
}

#[test]
fn cancel12_param_minus_const_offset_still_accepted() {
    // The positive control: the fix must not over-reject the legitimate
    // `param - const` it protects. A negative offset into the caller's buffer
    // (`p - 8`, a struct field below the pointer) stays Param and Tier B. Only a
    // *provable* Const subtrahend keeps param-derivation, which this exercises.
    assert!(accepts(
        r#"(module (memory 1) (func (param i32) (result i32)
             local.get 0 i32.const 8 i32.sub i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

// ===========================================================================
// 8c — MUST-REJECT: C-1 control-flow-laundered absolute address
// ===========================================================================

#[test]
fn f1_if_then_partial_write_skip_keeps_const() {
    // The headline C-1: local 2 = join(const 1024 on skip, param0 on taken) =
    // NotParam. The skip path leaves the const, which addresses the load.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32) (result i32) (local i32)
             i32.const 1024 local.set 2
             (block local.get 1 (if (then local.get 0 local.set 2)))
             local.get 2 i32.load) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn f2_if_else_one_arm_const() {
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32) (result i32) (local i32)
             local.get 1
             (if (then local.get 0 local.set 2) (else i32.const 2048 local.set 2))
             local.get 2 i32.load) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn f3_single_arm_fallthrough_default() {
    // The skip path leaves local 2 at its default (NotParam); join => NotParam.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32) (result i32) (local i32)
             local.get 1 (if (then local.get 0 local.set 2))
             local.get 2 i32.load) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn f4_loop_back_edge_clobber() {
    // A later iteration overwrites local 2 with a const; the fixpoint joins the
    // back-edge and demotes local 2 to NotParam.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32) (result i32) (local i32)
             local.get 0 local.set 2
             (loop
               local.get 1
               (if (then i32.const 4096 local.set 2 br 1))
               local.get 1 br_if 0)
             local.get 2 i32.load) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn f5_br_if_guarded_param_write() {
    // br_if skips the param write on one path; the merge demotes local 2.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32) (result i32) (local i32)
             i32.const 8192 local.set 2
             (block local.get 1 br_if 0 local.get 0 local.set 2)
             local.get 2 i32.load) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn f6_br_table_skips_param_write() {
    // One table edge skips the param write into local 2.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32) (result i32) (local i32)
             i32.const 16 local.set 2
             (block (block local.get 1 br_table 0 1) local.get 0 local.set 2)
             local.get 2 i32.load) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn f7_param_on_stack_does_not_cross_into_block() {
    // A param left on the operand stack before a block is not threaded in as the
    // block's param unless the block type declares it; conservative reject.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32) (result i32)
             local.get 0 (block (result i32) i32.load)) (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn f8_control_laundered_store() {
    // F1's join, but the demoted local 2 addresses a store instead of a load.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32) (local i32)
             i32.const 1024 local.set 2
             (block local.get 1 (if (then local.get 0 local.set 2)))
             local.get 2 local.get 0 i32.store) (export "f" (func 0)))"#,
        2,
    ));
}

// ===========================================================================
// 8d — MUST-REJECT: straight-line constant / global regression guards
// ===========================================================================

#[test]
fn s1_const_load_no_params() {
    assert!(!accepts(
        r#"(module (memory 1) (func (result i32)
             i32.const 1024 i32.load) (export "f" (func 0)))"#,
        0,
    ));
}

#[test]
fn s2_store_at_const() {
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32)
             i32.const 4096 local.get 0 i32.store) (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn s3_global_address_load() {
    assert!(!accepts(
        r#"(module (memory 1) (global i32 (i32.const 0)) (func (result i32)
             global.get 0 i32.load) (export "f" (func 0)))"#,
        0,
    ));
}

#[test]
fn s4_const_in_scratch_local() {
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32) (result i32) (local i32)
             i32.const 2048 local.set 1 local.get 1 i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn s5_memory_fill_at_const() {
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32)
             i32.const 0 local.get 0 local.get 1 memory.fill) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn s6_memory_grow_result_is_not_an_address() {
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32) (result i32)
             local.get 0 memory.grow i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn s7_memory_size_result_is_not_an_address() {
    assert!(!accepts(
        r#"(module (memory 1) (func (result i32)
             memory.size i32.load) (export "f" (func 0)))"#,
        0,
    ));
}

// ===========================================================================
// 8e — C-3 call boundaries: the SOUND interprocedural analysis. A constant
// laundered through a `call` rejects (the callee's param is untrusted at that
// site); a param-derived argument threaded through a `call` is accepted (the
// callee's param is trusted at every site).
// ===========================================================================

#[test]
fn c3a_const_arg_through_helper_call_is_rejected() {
    // $sum: const 1024; call $g    $g: param0 load. The only call site passes a
    // constant for $g's param 0, so param 0 is NOT trusted in $g, and $g's load
    // through it is rejected interprocedurally.
    let m = module(
        r#"
        (module
          (memory (;0;) 1)
          (type (;0;) (func (param i32 i32) (result i32)))
          (type (;1;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            i32.const 1024 call 1)
          (func (;1;) (type 1) (param i32) (result i32)
            local.get 0 i32.load)
          (export "sum" (func 0)))
        "#,
    );
    let err = verify_param_addressing(&m, &[0, 1], 0, "sum")
        .expect_err("a const arg laundered through a call must be rejected");
    assert!(
        matches!(err, LinkError::RequiresRelocatableBuild { .. }),
        "{err:?}"
    );
}

#[test]
fn c3b_param_arg_through_helper_is_accepted() {
    // The legitimate factored helper: $sum passes its own param 0 to $g, which
    // loads through its (now trusted) param 0. The sound interprocedural fixpoint
    // accepts this — the call-laundering stopgap no longer over-rejects it.
    let m = module(
        r#"
        (module
          (memory (;0;) 1)
          (type (;0;) (func (param i32 i32) (result i32)))
          (type (;1;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32 i32) (result i32)
            local.get 0 call 1)
          (func (;1;) (type 1) (param i32) (result i32)
            local.get 0 i32.load)
          (export "sum" (func 0)))
        "#,
    );
    assert!(
        verify_param_addressing(&m, &[0, 1], 0, "sum").is_ok(),
        "a param-derived arg threaded through a call must be accepted"
    );
}

#[test]
fn c3c_call_result_used_as_address_is_rejected() {
    // The call result is NotParam, so using it as an address is rejected even at
    // the single-function level.
    assert!(!accepts_in(
        r#"
        (module
          (memory (;0;) 1)
          (type (;0;) (func (result i32)))
          (func (;0;) (type 0) (result i32)
            call 1 i32.load)
          (func (;1;) (type 0) (result i32)
            i32.const 1024)
          (export "f" (func 0)))
        "#,
        0,
        0,
    ));
}

#[test]
fn single_function_memory_closure_is_still_analyzed() {
    // A single-function closure IS its own root, so its parameters seed the
    // trusted set and the analysis proves it precisely (the n=1 case unchanged).
    let m = module(
        r#"(module (memory 1) (func (param i32) (result i32)
             local.get 0 i32.load) (export "f" (func 0)))"#,
    );
    assert!(verify_param_addressing(&m, &[0], 0, "f").is_ok());
}

// ===========================================================================
// 8i — SOUND interprocedural address-provenance. The closure root's parameters
// are the only caller-supplied pointers; an inner function's parameter is
// trusted only when *every* reachable call site passes it a param-derived
// argument. Each case is a whole module with two or more functions sharing the
// one memory.
// ===========================================================================

/// Runs the interprocedural verifier over a whole `module_wat`, treating
/// `func_index` as the closure root and every function as in the closure.
fn verify(module_wat: &str, func_indices: &[u32], root: u32) -> Result<(), LinkError> {
    let m = module(module_wat);
    verify_param_addressing(&m, func_indices, root, "export")
}

#[test]
fn ip1_sort_calls_swap_with_param_derived_pointer_accepts() {
    // The headline case (a): `sort(ptr,len)` calls `swap(p,a,b)` with a
    // param-derived `ptr` argument; `swap` dereferences its pointer param.
    // `swap`'s param 0 is trusted at the only call site (it is `sort`'s ptr), so
    // the whole closure is accepted.
    assert!(verify(
        r#"
        (module
          (memory (;0;) 1)
          (type (;0;) (func (param i32 i32)))
          (type (;1;) (func (param i32 i32 i32)))
          (func (;0;) (type 0) (param i32 i32)
            local.get 0 local.get 0 local.get 1 call 1)
          (func (;1;) (type 1) (param i32 i32 i32)
            local.get 0 local.get 1 i32.store
            local.get 0 local.get 2 i32.store)
          (export "sort" (func 0)))
        "#,
        &[0, 1],
        0,
    )
    .is_ok());
}

#[test]
fn ip2_helper_called_with_constant_address_rejects() {
    // Case (b): a helper `g(addr)` that loads through its param, called with a
    // *constant* argument. `g`'s param 0 is untrusted (the const arg), so its
    // load is rejected.
    assert!(verify(
        r#"
        (module
          (memory (;0;) 1)
          (type (;0;) (func (result i32)))
          (type (;1;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (result i32)
            i32.const 1024 call 1)
          (func (;1;) (type 1) (param i32) (result i32)
            local.get 0 i32.load)
          (export "root" (func 0)))
        "#,
        &[0, 1],
        0,
    )
    .is_err());
}

#[test]
fn ip3_helper_called_from_two_sites_one_const_rejects() {
    // Case (c): a helper called from two sites — one param-derived, one constant.
    // The must-join over call sites demotes the helper's param to untrusted, so
    // its dereference is rejected.
    assert!(verify(
        r#"
        (module
          (memory (;0;) 1)
          (type (;0;) (func (param i32)))
          (type (;1;) (func (param i32)))
          (func (;0;) (type 0) (param i32)
            local.get 0 call 2
            i32.const 4096 call 2)
          (func (;1;) (type 0) (param i32)
            local.get 0 call 2)
          (func (;2;) (type 1) (param i32)
            local.get 0 i32.const 0 i32.store)
          (export "root" (func 0)))
        "#,
        &[0, 1, 2],
        0,
    )
    .is_err());
}

#[test]
fn ip3b_helper_called_from_two_param_derived_sites_accepts() {
    // Control for (c): the same two-call-site shape, but *both* sites pass a
    // param-derived argument. The helper's param stays trusted; accepted.
    assert!(verify(
        r#"
        (module
          (memory (;0;) 1)
          (type (;0;) (func (param i32 i32)))
          (type (;1;) (func (param i32)))
          (func (;0;) (type 0) (param i32 i32)
            local.get 0 call 1
            local.get 1 call 1)
          (func (;1;) (type 1) (param i32)
            local.get 0 i32.const 0 i32.store)
          (export "root" (func 0)))
        "#,
        &[0, 1],
        0,
    )
    .is_ok());
}

#[test]
fn ip4a_self_recursion_passing_param_accepts() {
    // Case (d): self-recursion passing a param-derived argument (`f(p)` calls
    // `f(p+1)`), dereferencing its param. The greatest fixpoint keeps the param
    // trusted across the back-edge; accepted.
    assert!(verify(
        r#"
        (module
          (memory (;0;) 1)
          (type (;0;) (func (param i32)))
          (func (;0;) (type 0) (param i32)
            local.get 0 i32.const 0 i32.store
            local.get 0 i32.const 1 i32.add call 0)
          (export "f" (func 0)))
        "#,
        &[0],
        0,
    )
    .is_ok());
}

#[test]
fn ip4b_self_recursion_passing_const_rejects() {
    // Case (d): self-recursion passing a *constant* argument that the function
    // dereferences. The fixpoint removes the param from the trusted set (a const
    // reaches it on the recursive path), so its dereference is rejected.
    assert!(verify(
        r#"
        (module
          (memory (;0;) 1)
          (type (;0;) (func (param i32)))
          (func (;0;) (type 0) (param i32)
            local.get 0 i32.load drop
            i32.const 2048 call 0)
          (export "f" (func 0)))
        "#,
        &[0],
        0,
    )
    .is_err());
}

#[test]
fn ip5_mutual_recursion_param_derived_accepts() {
    // Case (e): mutual recursion `a(p) -> b(p) -> a(p)`, each dereferencing its
    // param, every call threading a param-derived argument. The fixpoint keeps
    // both params trusted; accepted.
    assert!(verify(
        r#"
        (module
          (memory (;0;) 1)
          (type (;0;) (func (param i32)))
          (func (;0;) (type 0) (param i32)
            local.get 0 i32.const 0 i32.store
            local.get 0 call 1)
          (func (;1;) (type 0) (param i32)
            local.get 0 i32.const 0 i32.store
            local.get 0 call 0)
          (export "a" (func 0)))
        "#,
        &[0, 1],
        0,
    )
    .is_ok());
}

#[test]
fn ip5b_mutual_recursion_one_const_arg_rejects() {
    // Case (e): mutual recursion where one leg passes a constant to the other,
    // which dereferences it. The const poisons the callee's param; rejected.
    assert!(verify(
        r#"
        (module
          (memory (;0;) 1)
          (type (;0;) (func (param i32)))
          (func (;0;) (type 0) (param i32)
            i32.const 512 call 1)
          (func (;1;) (type 0) (param i32)
            local.get 0 i32.const 0 i32.store
            local.get 0 call 0)
          (export "a" (func 0)))
        "#,
        &[0, 1],
        0,
    )
    .is_err());
}

#[test]
fn ip6_call_indirect_result_as_address_rejects() {
    // Case (f): a `call_indirect` whose result feeds an address. The result is
    // NotParam (no callee param is trusted through an indirect dispatch), so the
    // dereference is rejected. (A table use also marks the closure Tier C
    // upstream; this pins the provenance-level conservatism directly.)
    assert!(!accepts_in(
        r#"
        (module
          (memory (;0;) 1)
          (table (;0;) 1 funcref)
          (type (;0;) (func (result i32)))
          (func (;0;) (type 0) (result i32)
            i32.const 0 call_indirect (type 0) i32.load)
          (export "f" (func 0)))
        "#,
        0,
        0,
    ));
}

#[test]
fn ip7_root_param_is_trusted_even_when_a_callsite_passes_const() {
    // The root's parameters are seeded trusted unconditionally (the caller owns
    // the shared memory). A const passed to a *helper* poisons only the helper's
    // param, never the root's own dereference. Here the root dereferences its own
    // param 0 directly and also calls a const-fed helper that does NOT touch
    // memory — the root access stays accepted.
    assert!(verify(
        r#"
        (module
          (memory (;0;) 1)
          (type (;0;) (func (param i32) (result i32)))
          (type (;1;) (func (param i32) (result i32)))
          (func (;0;) (type 0) (param i32) (result i32)
            i32.const 9 call 1 drop
            local.get 0 i32.load)
          (func (;1;) (type 1) (param i32) (result i32)
            local.get 0 i32.const 1 i32.add)
          (export "root" (func 0)))
        "#,
        &[0, 1],
        0,
    )
    .is_ok());
}

#[test]
fn ip8_callee_reached_only_via_table_param_is_untrusted() {
    // Fail-closed (f): a function dereferences its param but is reachable only
    // through the table (no direct `call` site records an argument). With no
    // call site to justify trusting its param, the param defaults untrusted and
    // its dereference is rejected. Modeled here as an inner function present in
    // the closure with no direct caller.
    assert!(verify(
        r#"
        (module
          (memory (;0;) 1)
          (type (;0;) (func (param i32)))
          (func (;0;) (type 0) (param i32)
            local.get 0 i32.const 0 i32.store)
          (func (;1;) (type 0) (param i32)
            local.get 0 i32.load drop)
          (export "root" (func 0)))
        "#,
        &[0, 1],
        0,
    )
    .is_err());
}

#[test]
fn ip9_diamond_all_param_derived_accepts() {
    // A diamond: root calls two mids, both of which call one shared leaf with a
    // param-derived pointer; the leaf dereferences its param. Every call site is
    // param-derived, so the leaf's param is trusted; accepted.
    assert!(verify(
        r#"
        (module
          (memory (;0;) 1)
          (type (;0;) (func (param i32)))
          (func (;0;) (type 0) (param i32)
            local.get 0 call 1
            local.get 0 call 2)
          (func (;1;) (type 0) (param i32)
            local.get 0 call 3)
          (func (;2;) (type 0) (param i32)
            local.get 0 call 3)
          (func (;3;) (type 0) (param i32)
            local.get 0 i32.const 0 i32.store)
          (export "root" (func 0)))
        "#,
        &[0, 1, 2, 3],
        0,
    )
    .is_ok());
}

#[test]
fn ip10_diamond_one_leg_const_rejects() {
    // The same diamond, but one mid passes a constant to the shared leaf. The
    // must-join over the leaf's two call sites demotes its param; rejected.
    assert!(verify(
        r#"
        (module
          (memory (;0;) 1)
          (type (;0;) (func (param i32)))
          (func (;0;) (type 0) (param i32)
            local.get 0 call 1
            local.get 0 call 2)
          (func (;1;) (type 0) (param i32)
            local.get 0 call 3)
          (func (;2;) (type 0) (param i32)
            i32.const 64 call 3)
          (func (;3;) (type 0) (param i32)
            local.get 0 i32.const 0 i32.store)
          (export "root" (func 0)))
        "#,
        &[0, 1, 2, 3],
        0,
    )
    .is_err());
}

#[test]
fn ip11_non_root_export_position_seeds_only_the_root() {
    // The root is whichever function satisfies the export, not function 0. Here
    // function 1 is the root; it calls function 0 (the helper) with a constant.
    // The helper's param is untrusted; its dereference is rejected — proving the
    // seed follows the `root` argument, not the lowest index.
    assert!(verify(
        r#"
        (module
          (memory (;0;) 1)
          (type (;0;) (func (param i32)))
          (func (;0;) (type 0) (param i32)
            local.get 0 i32.load drop)
          (func (;1;) (type 0) (param i32)
            i32.const 7 call 0)
          (export "root" (func 1)))
        "#,
        &[0, 1],
        1,
    )
    .is_err());
}

// ===========================================================================
// 8f — MUST-REJECT: memory.copy / multi-operand partial-param
// ===========================================================================

#[test]
fn mc1_copy_src_is_const() {
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32)
             local.get 0 i32.const 0 local.get 1 memory.copy) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn mc2_copy_dest_is_const() {
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32)
             i32.const 0 local.get 0 local.get 1 memory.copy) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn mc3_copy_both_params() {
    assert!(accepts(
        r#"(module (memory 1) (func (param i32 i32 i32)
             local.get 0 local.get 1 local.get 2 memory.copy) (export "f" (func 0)))"#,
        3,
    ));
}

#[test]
fn mc4_copy_src_is_param_plus_zero() {
    // src = param1 + 0 => add => Param; dest = param0 => Param. ACCEPT.
    assert!(accepts(
        r#"(module (memory 1) (func (param i32 i32 i32)
             local.get 0 local.get 1 i32.const 0 i32.add local.get 2 memory.copy)
           (export "f" (func 0)))"#,
        3,
    ));
}

// ===========================================================================
// 8f' — S1: the bulk-memory SIZE / extent operand must be caller-derived too.
//
// A bulk-memory op touches `[address, address + size)`. Modeling only the start
// address let an external clobber/read an unbounded region above a caller
// pointer with a constant extent (`memory.fill(param, v, 0x8000)`). The extent
// now carries the same caller-derivation requirement as an address: a constant
// or global size (empty mask) REJECTS, a caller-passed size (Param) ADMITS.
// ===========================================================================

#[test]
fn ext1_fill_param_dest_const_size_rejected() {
    // `memory.fill(param0, v, 0x8000)`: dest is caller-derived, but the constant
    // extent could scorch host memory above the pointer. REJECT.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32)
             local.get 0 local.get 1 i32.const 32768 memory.fill)
           (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn ext2_fill_param_dest_param_size_accepted() {
    // `memory.fill(param0, v, param2)`: both the destination and the extent are
    // caller-supplied, so the clobber is bounded by a value the caller owns.
    // ACCEPT.
    assert!(accepts(
        r#"(module (memory 1) (func (param i32 i32 i32)
             local.get 0 local.get 1 local.get 2 memory.fill)
           (export "f" (func 0)))"#,
        3,
    ));
}

#[test]
fn ext3_copy_params_const_size_rejected() {
    // `memory.copy(param0, param1, 0x8000)`: both ends are caller-derived, but
    // the constant extent is unbounded relative to the caller's pointers. REJECT.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32)
             local.get 0 local.get 1 i32.const 32768 memory.copy)
           (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn ext4_copy_all_params_accepted() {
    // `memory.copy(dst_param, src_param, len_param)`: dest, src, and extent are
    // all caller-supplied. ACCEPT.
    assert!(accepts(
        r#"(module (memory 1) (func (param i32 i32 i32)
             local.get 0 local.get 1 local.get 2 memory.copy)
           (export "f" (func 0)))"#,
        3,
    ));
}

#[test]
fn ext5_fill_const_size_via_local_rejected() {
    // The extent laundered through a scratch local is still a constant: the
    // local carries `Const`, whose empty mask rejects. REJECT.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32) (result i32) (local i32)
             i32.const 32768 local.set 2
             local.get 0 local.get 1 local.get 2 memory.fill
             local.get 0) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn ext6_copy_param_extent_plus_const_accepted() {
    // `len = param2 + const` stays Param, so a caller-bounded extent adjusted by
    // a fixed offset is still admitted (the realistic `len - 1` / `len + 1`
    // pattern). ACCEPT.
    assert!(accepts(
        r#"(module (memory 1) (func (param i32 i32 i32)
             local.get 0 local.get 1 local.get 2 i32.const 1 i32.add memory.copy)
           (export "f" (func 0)))"#,
        3,
    ));
}

// ===========================================================================
// 8g — select-laundered & nested-block edge cases
// ===========================================================================

#[test]
fn sl1_select_param_and_const() {
    // select(param, const) => join(Param, NotParam) = NotParam.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32) (result i32)
             local.get 0 i32.const 1024 i32.const 1 select i32.load)
           (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn sl2_select_param_and_param() {
    // select(param0, param1) => Param (= a11).
    assert!(accepts(
        r#"(module (memory 1) (func (param i32 i32) (result i32)
             local.get 0 local.get 1 i32.const 1 select i32.load)
           (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn nb1_param_threaded_through_nested_blocks() {
    assert!(accepts(
        r#"(module (memory 1) (func (param i32) (result i32)
             (block (result i32)
               (block (result i32)
                 (block (result i32) local.get 0)))
             i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

#[test]
fn nb2_inner_if_writes_const_demotes_outward() {
    // An inner if conditionally writes a const into the address local; the join
    // demotes it, and the demotion propagates out of the nested blocks.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32 i32) (result i32) (local i32)
             local.get 0 local.set 2
             (block
               (block
                 local.get 1
                 (if (then i32.const 256 local.set 2))))
             local.get 2 i32.load) (export "f" (func 0)))"#,
        2,
    ));
}

#[test]
fn tee1_local_tee_const_under_control_flow() {
    // local.tee writes a const on the taken arm; merge with the skip-path entry
    // (local 1 at default NotParam) => NotParam.
    assert!(!accepts(
        r#"(module (memory 1) (func (param i32) (result i32) (local i32)
             local.get 0
             (if (then i32.const 100 local.tee 1 drop))
             local.get 1 i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

// ===========================================================================
// 8h — M-1 resource guard: over-declared locals must not OOM
// ===========================================================================

#[test]
fn r1_over_declared_locals_rejected_without_huge_alloc() {
    // A tiny body whose single locals group claims u32::MAX locals must be
    // rejected as a clean LinkError::Parse before any per-local allocation,
    // never driving a multi-gigabyte `vec!`.
    let body = over_declared_locals_body(u32::MAX);
    let m = ParsedModule::default();
    let err = function_is_param_addressing(&m, &body, 0)
        .expect_err("over-declared locals must be rejected");
    assert!(
        matches!(err, LinkError::Parse(msg) if msg.contains("too many locals")),
        "expected a clean Parse rejection for the over-declared locals count"
    );
}

#[test]
fn r2_locals_exceeding_body_length_rejected() {
    // A locals group declaring more locals than the body has bytes is malformed
    // (each local costs >= 1 byte); reject before allocation.
    let body = over_declared_locals_body(1_000_000);
    let m = ParsedModule::default();
    let err = function_is_param_addressing(&m, &body, 0)
        .expect_err("locals exceeding body length must be rejected");
    assert!(matches!(err, LinkError::Parse(_)), "{err:?}");
}

#[test]
fn r3_locals_under_the_cap_are_analyzed() {
    // A modest, legitimate locals count runs the analysis to completion.
    assert!(accepts(
        r#"(module (memory 1) (func (param i32) (result i32)
             (local i32 i32 i32)
             local.get 0 i32.load) (export "f" (func 0)))"#,
        1,
    ));
}

// ===========================================================================
// Deep nesting: the analysis must fail closed, never overflow its own stack
// ===========================================================================

#[test]
fn deeply_nested_blocks_fail_closed_without_aborting() {
    // A body nested far past the analysis depth cap must be rejected as a normal
    // `Ok(false)` (Tier C), never recurse until the analysis stack overflows.
    let depth = super::MAX_ANALYSIS_DEPTH + 50;
    let mut wat = String::from(
        "(module (memory 1) (func (param i32) (result i32) local.get 0 ",
    );
    for _ in 0..depth {
        wat.push_str("(block (result i32) ");
    }
    wat.push_str("i32.load");
    for _ in 0..depth {
        wat.push(')');
    }
    wat.push_str(") (export \"f\" (func 0)))");

    // Past the depth cap the analysis fails closed (rejects), and it must return
    // a verdict rather than abort.
    assert!(!accepts(&wat, 1));
}

/// Builds a raw function body whose single locals group declares `count` locals
/// of type `i32`, followed by `i32.const 0; i32.load; drop; end` (a memory-using
/// body). The body is hand-encoded because `wat` would reject a u32::MAX locals
/// count; this exercises the analysis's own pre-allocation cap.
fn over_declared_locals_body(count: u32) -> Vec<u8> {
    let mut body = Vec::new();
    // locals: one group of (count, i32). count is a LEB128 u32; i32 == 0x7F.
    body.push(0x01); // one locals group
    write_leb_u32(&mut body, count);
    body.push(0x7F); // i32
                     // i32.const 0
    body.push(0x41);
    body.push(0x00);
    // i32.load (align=2, offset=0)
    body.push(0x28);
    body.push(0x02);
    body.push(0x00);
    // drop
    body.push(0x1A);
    // end
    body.push(0x0B);
    body
}

/// Writes `value` as unsigned LEB128.
fn write_leb_u32(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}
