//! Unit tests for the reachability lowering of `exists`/`unique`-bodied
//! specification free functions: the appended choice-parameter suffix, the
//! `0xfc`-wrapper suppression, and the `@`-to-parameter seam with its retained
//! domain normalization.
//!
//! Each test drives the production traversal directly so the assembled bytes
//! can be inspected without the CLI in the loop; the obligation pass runs as
//! part of the traversal and every fixture here translates cleanly, so the
//! module carries its real `inference.hspecs` payload.

use inference_type_checker::TypeCheckerBuilder;
use inference_type_checker::typed_context::TypedContext;

use crate::compiler::Compiler;
use crate::hassert::reach::plan_reachability_specs;
use crate::target::CompilationMode;
use crate::traverse_t_ast_with_compiler;

fn type_check(source: &str) -> TypedContext {
    let parsed = inference_parser::parse(source);
    assert!(
        parsed.errors.is_empty(),
        "parse errors: {:?}",
        parsed.errors
    );
    TypeCheckerBuilder::build_typed_context(parsed.arena)
        .expect("type checking should succeed")
        .typed_context()
}

/// Compiles `source` in proof mode through the production traversal and
/// returns the assembled module bytes, obligations included.
fn compile_reach_module(source: &str) -> Vec<u8> {
    let ctx = type_check(source);
    let plans = plan_reachability_specs(&ctx).expect("the pre-scan should accept this fixture");
    let mut compiler = Compiler::new("reach_test");
    let hspecs = traverse_t_ast_with_compiler(&ctx, &mut compiler, CompilationMode::Proof, &plans)
        .expect("every fixture here translates cleanly");
    let (wasm, _spec_indices, _frame_sizes) = compiler.finish_and_take(&hspecs);
    inf_wasmparser::validate(&wasm)
        .unwrap_or_else(|e| panic!("the assembled module must validate: {e}"));
    wasm
}

fn wat_of(wasm: &[u8]) -> String {
    wasmprinter::print_bytes(wasm).unwrap_or_else(|e| panic!("WAT print failed: {e}"))
}

/// Collapses all whitespace to single spaces so instruction sequences can be
/// asserted without depending on wasmprinter's indentation.
fn flat(wat: &str) -> String {
    wat.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn wasm_contains(wasm: &[u8], needle: &[u8]) -> bool {
    wasm.windows(needle.len()).any(|w| w == needle)
}

fn count_occurrences(wasm: &[u8], needle: &[u8]) -> usize {
    wasm.windows(needle.len()).filter(|w| *w == needle).count()
}

// ----- signature shape ----------------------------------------------------

/// The choice suffix lands after the declared parameters, i32/i64 by declared
/// scalar class; a named choice carries its `let` name, an anonymous one its
/// `__choice{k}` debug name; the body draws nothing through `0xfc`.
#[test]
fn the_choice_suffix_extends_the_signature_in_source_order() {
    cov_mark::check!(wasm_codegen_reach_choice_suffix);
    cov_mark::check!(wasm_codegen_reach_body_wrapper_suppressed);
    cov_mark::check!(wasm_codegen_reach_named_choice_binding);
    cov_mark::check!(wasm_codegen_reach_choice_param_load);
    let wasm = compile_reach_module(
        "fn g(v: i32) -> i32 { return v; }
        spec S {
          fn f(x: i32) exists {
            let c: i64 = @;
            assert(c > 0);
            assert(g(@) == x);
          }
        }",
    );
    let wat = flat(&wat_of(&wasm));
    assert!(
        wat.contains("(param $x i32) (param $c i64) (param $__choice1 i32)"),
        "declared param, then the named i64 choice, then the anonymous i32 choice:\n{wat}"
    );
    assert!(
        !wasm_contains(&wasm, &[0xfc, 0x31])
            && !wasm_contains(&wasm, &[0xfc, 0x32])
            && !wasm_contains(&wasm, &[0xfc, 0x3b]),
        "a reachability-lowered module must contain no uzumaki draw and no exists wrapper"
    );
}

/// An `exists` body without any `@` still lowers vanilla — wrapper suppressed,
/// signature unchanged.
#[test]
fn a_choiceless_exists_body_keeps_its_declared_signature() {
    cov_mark::check!(wasm_codegen_reach_body_wrapper_suppressed);
    let wasm = compile_reach_module(
        "spec S {
          fn f(x: i32) exists {
            assert(x > 0);
          }
        }",
    );
    let wat = flat(&wat_of(&wasm));
    assert!(
        wat.contains("(param $x i32)") && !wat.contains("__choice"),
        "no choice suffix without a planned `@`:\n{wat}"
    );
    assert!(
        !wasm_contains(&wasm, &[0xfc, 0x3b]),
        "the exists body wrapper must be suppressed"
    );
}

/// A `unique` body lowers exactly like an `exists` one: suffix appended, no
/// `0xfc 0x3d` wrapper, no draw.
#[test]
fn a_unique_body_lowers_vanilla_with_its_choice_suffix() {
    let wasm = compile_reach_module(
        "spec S {
          fn f() unique {
            let n: i32 = @;
            assert(n == 7);
          }
        }",
    );
    let wat = flat(&wat_of(&wasm));
    assert!(
        wat.contains("(param $n i32)"),
        "the named choice is the function's only parameter:\n{wat}"
    );
    assert!(
        !wasm_contains(&wasm, &[0xfc, 0x3d]) && !wasm_contains(&wasm, &[0xfc, 0x31]),
        "no unique wrapper and no draw in a reachability-lowered body"
    );
}

// ----- domain normalization ----------------------------------------------

/// A named `bool` choice is normalized in place: the parameter itself holds
/// the in-domain value after the `let`, and no fresh local is allocated.
#[test]
fn a_named_bool_choice_normalizes_into_its_own_parameter() {
    cov_mark::check!(wasm_codegen_reach_named_choice_binding);
    let wasm = compile_reach_module(
        "spec S {
          fn f() exists {
            let b: bool = @;
            assert(b);
          }
        }",
    );
    let wat = flat(&wat_of(&wasm));
    assert!(
        wat.contains("local.get $b i32.const 1 i32.and local.set $b"),
        "the bool domain mapping must store back into the choice parameter:\n{wat}"
    );
    assert!(
        !wat.contains("(local "),
        "a named choice aliases its parameter; no fresh local may be declared:\n{wat}"
    );
}

/// A named sub-i32 choice keeps the sign-extending wrap a draw performs.
#[test]
fn a_named_i16_choice_wraps_into_its_declared_domain() {
    let wasm = compile_reach_module(
        "spec S {
          fn f() exists {
            let n: i16 = @;
            assert(n > 0);
          }
        }",
    );
    let wat = flat(&wat_of(&wasm));
    assert!(
        wat.contains("local.get $n i32.const 16 i32.shl i32.const 16 i32.shr_s local.set $n"),
        "the i16 wrap must be the shl/shr_s pair stored back into the parameter:\n{wat}"
    );
}

/// A named enum choice maps onto the tag range via `rem_u N`.
#[test]
fn a_named_enum_choice_maps_onto_the_tag_range() {
    let wasm = compile_reach_module(
        "enum Color { Red, Green, Blue }
        spec S {
          fn f() exists {
            let c: Color = @;
            assert(c == Color::Red);
          }
        }",
    );
    let wat = flat(&wat_of(&wasm));
    assert!(
        wat.contains("local.get $c i32.const 3 i32.rem_u local.set $c"),
        "the enum domain mapping must be `rem_u 3` stored back into the parameter:\n{wat}"
    );
}

/// A named `i64` choice needs no normalization; the binding degenerates to a
/// get/set pair on the parameter itself.
#[test]
fn a_named_i64_choice_needs_no_normalization() {
    let wasm = compile_reach_module(
        "spec S {
          fn f() exists {
            let c: i64 = @;
            assert(c > 0);
          }
        }",
    );
    let wat = flat(&wat_of(&wasm));
    assert!(
        wat.contains("local.get $c local.set $c"),
        "every i64 bit pattern is in-domain; only the store-back remains:\n{wat}"
    );
}

/// An anonymous narrow choice is normalized at its use site — the raw
/// parameter is loaded and mapped onto the domain right where it is consumed.
#[test]
fn an_anonymous_bool_choice_normalizes_at_its_use_site() {
    cov_mark::check!(wasm_codegen_reach_choice_param_load);
    let wasm = compile_reach_module(
        "fn g(b: bool) -> i32 { if b { return 1; } return 0; }
        spec S {
          fn f() exists {
            assert(g(@) == 1);
          }
        }",
    );
    let wat = flat(&wat_of(&wasm));
    assert!(
        wat.contains("local.get $__choice0 i32.const 1 i32.and call $g"),
        "the anonymous bool choice must load and normalize right before the call:\n{wat}"
    );
}

// ----- nested blocks and forall siblings ----------------------------------

/// Nested `exists`/`assume` blocks lower inline under the reachability flag:
/// their statements are emitted, their wrappers are not, and their `@`s join
/// the choice suffix.
#[test]
fn nested_exists_and_assume_blocks_lower_inline() {
    cov_mark::check!(wasm_codegen_reach_nested_block_inlined);
    let wasm = compile_reach_module(
        "spec S {
          fn f(x: i32) exists {
            assume { assert(x > 0); }
            exists {
              let n: i32 = @;
              assert(n > x);
            }
            assert(x < 100);
          }
        }",
    );
    let wat = flat(&wat_of(&wasm));
    assert!(
        wat.contains("(param $x i32) (param $n i32)"),
        "the nested block's `@` joins the choice suffix:\n{wat}"
    );
    assert!(
        !wasm_contains(&wasm, &[0xfc, 0x3b]) && !wasm_contains(&wasm, &[0xfc, 0x3c]),
        "nested exists/assume wrappers must be suppressed"
    );
}

/// The suppression keys on the per-function reachability flag, not on
/// `current_spec`: a `forall` sibling in the same spec keeps its body wrapper
/// and its `0xfc` draw byte-for-byte.
#[test]
fn a_forall_sibling_keeps_its_wrappers_and_draws() {
    let wasm = compile_reach_module(
        "spec S {
          fn g() forall {
            let n: i32 = @;
            assert(n >= n);
          }
          fn f() exists {
            let n: i32 = @;
            assert(n >= n);
          }
        }",
    );
    assert!(
        wasm_contains(&wasm, &[0xfc, 0x3a]),
        "the forall body wrapper must survive"
    );
    assert_eq!(
        count_occurrences(&wasm, &[0xfc, 0x31]),
        1,
        "exactly one draw: the forall body's; the exists body reads its parameter"
    );
    assert!(
        !wasm_contains(&wasm, &[0xfc, 0x3b]),
        "the exists body wrapper must be suppressed"
    );
}

// ----- the defensive no-return rule through the public entry point --------

/// The declared-return-type clause fails `codegen()` itself, before any byte
/// is emitted — the pipeline that skips analysis still cannot produce a
/// misaligned artifact.
#[test]
fn codegen_rejects_a_declared_return_type_on_an_exists_body() {
    let ctx = type_check(
        "spec S {
          fn f() -> i32 exists {
            let n: i32 = @;
            assert(n > 0);
          }
        }",
    );
    let err = crate::codegen(
        &ctx,
        "reach_test",
        crate::CodegenOptions {
            target: crate::Target::Wasm32,
            mode: CompilationMode::Proof,
            opt_level: crate::OptLevel::O0,
            features: crate::EmitFeatures::default(),
        },
    )
    .expect_err("a declared return type on an exists body must fail codegen");
    let msg = err.to_string();
    assert!(
        msg.contains("declares a return type") && msg.contains("'exists'-quantified"),
        "expected the declared-type clause of the no-return rule: {msg}"
    );
}

/// The return-statement clause fails `codegen()` the same way.
#[test]
fn codegen_rejects_a_return_statement_in_a_unique_body() {
    let ctx = type_check(
        "spec S {
          fn f() unique {
            let n: i32 = @;
            assert(n == 1);
            return;
          }
        }",
    );
    let err = crate::codegen(
        &ctx,
        "reach_test",
        crate::CodegenOptions {
            target: crate::Target::Wasm32,
            mode: CompilationMode::Proof,
            opt_level: crate::OptLevel::O0,
            features: crate::EmitFeatures::default(),
        },
    )
    .expect_err("a return statement in a unique body must fail codegen");
    let msg = err.to_string();
    assert!(
        msg.contains("contains a `return` statement") && msg.contains("'unique'-quantified"),
        "expected the return-statement clause of the no-return rule: {msg}"
    );
}
