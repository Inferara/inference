//! Unit tests for the choice lowering of specification functions: the appended
//! choice-parameter suffix, the `0xfc`-wrapper suppression, the
//! `@`-to-parameter seam with its retained domain normalization, and the
//! per-leaf expansion of an aggregate `@`.
//!
//! Each test drives the production traversal directly so the assembled bytes
//! can be inspected without the CLI in the loop; the obligation pass runs as
//! part of the traversal and every fixture here translates cleanly, so the
//! module carries its real `inference.hspecs` payload.
//!
//! Both validators run on every module: `inf_wasmparser` is the fork that
//! accepts the custom opcodes, so it structurally cannot observe stock
//! validity, and `wasmparser` is the stock decoder that can.

use inference_type_checker::TypeCheckerBuilder;
use inference_type_checker::typed_context::TypedContext;

use crate::compiler::Compiler;
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
fn compile_spec_module(source: &str) -> Vec<u8> {
    let ctx = type_check(source);
    let mut compiler = Compiler::new("choice_test");
    let hspecs = traverse_t_ast_with_compiler(&ctx, &mut compiler, CompilationMode::Proof)
        .expect("every fixture here translates cleanly");
    let (wasm, _spec_indices, _frame_sizes) = compiler.finish_and_take(&hspecs);
    inf_wasmparser::validate(&wasm)
        .unwrap_or_else(|e| panic!("the assembled module must validate: {e}"));
    wasmparser::Validator::new()
        .validate_all(&wasm)
        .unwrap_or_else(|e| panic!("the assembled module must pass stock validation: {e}"));
    wasm
}

/// Every `0xfc`-prefixed custom opcode the crate can emit: the two uzumaki
/// draws and the four non-deterministic block wrappers.
const CUSTOM_OPCODES: [u8; 6] = [0x31, 0x32, 0x3a, 0x3b, 0x3c, 0x3d];

/// Asserts the module carries no Inference custom opcode at all. Stronger than
/// stock validation on its own: it names which opcode leaked.
fn assert_vanilla(wasm: &[u8]) {
    for op in CUSTOM_OPCODES {
        assert!(
            !wasm_contains(wasm, &[0xfc, op]),
            "a choice-lowered module must carry no custom opcode, found 0xfc {op:#04x}"
        );
    }
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
    cov_mark::check!(wasm_codegen_choice_suffix);
    cov_mark::check!(wasm_codegen_choice_body_wrapper_suppressed);
    cov_mark::check!(wasm_codegen_choice_named_binding);
    cov_mark::check!(wasm_codegen_choice_param_load);
    let wasm = compile_spec_module(
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
    assert_vanilla(&wasm);
}

/// An `exists` body without any `@` still lowers vanilla — wrapper suppressed,
/// signature unchanged.
#[test]
fn a_choiceless_exists_body_keeps_its_declared_signature() {
    cov_mark::check!(wasm_codegen_choice_body_wrapper_suppressed);
    let wasm = compile_spec_module(
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
    assert_vanilla(&wasm);
}

/// A `unique` body lowers exactly like an `exists` one: suffix appended, no
/// `0xfc 0x3d` wrapper, no draw.
#[test]
fn a_unique_body_lowers_vanilla_with_its_choice_suffix() {
    let wasm = compile_spec_module(
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
    assert_vanilla(&wasm);
}

// ----- domain normalization ----------------------------------------------

/// A named `bool` choice is normalized in place: the parameter itself holds
/// the in-domain value after the `let`, and no fresh local is allocated.
#[test]
fn a_named_bool_choice_normalizes_into_its_own_parameter() {
    cov_mark::check!(wasm_codegen_choice_named_binding);
    let wasm = compile_spec_module(
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
    let wasm = compile_spec_module(
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
    let wasm = compile_spec_module(
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
    let wasm = compile_spec_module(
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
    cov_mark::check!(wasm_codegen_choice_param_load);
    let wasm = compile_spec_module(
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

// ----- nested blocks, siblings, and methods -------------------------------

/// Nested `exists`/`assume` blocks lower inline: their statements are emitted,
/// their wrappers are not, and their `@`s join the choice suffix.
#[test]
fn nested_exists_and_assume_blocks_lower_inline() {
    cov_mark::check!(wasm_codegen_choice_nested_block_inlined);
    let wasm = compile_spec_module(
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
    assert_vanilla(&wasm);
}

/// All four nested block kinds lose their wrapper, not a chosen pair: the
/// quantifier is carried by the obligation, which is built from the typed AST.
#[test]
fn every_nested_block_kind_loses_its_wrapper() {
    let wasm = compile_spec_module(
        "spec S {
          fn f(x: i32) forall {
            assume { assert(x >= x); }
            forall { assert(x >= x); }
            exists {
              let n: i32 = @;
              assert(n > x);
            }
          }
        }",
    );
    assert_vanilla(&wasm);
    let wat = flat(&wat_of(&wasm));
    assert!(
        wat.contains("(param $x i32) (param $n i32)"),
        "the nested exists block's `@` joins the choice suffix:\n{wat}"
    );
}

/// A `forall` sibling is choice-lowered exactly like its `exists` neighbour:
/// the suppression keys on the per-function choice plan, which every
/// specification function has, so neither body keeps a wrapper or a draw.
///
/// This inverts what the reachability-only lowering pinned here. Universality
/// is not carried by the bytes at all — it is carried by the obligation, which
/// the obligation pass builds from the typed AST, and by the obligation's kind,
/// which is what decides omission versus retention downstream.
#[test]
fn a_forall_sibling_is_choice_lowered_too() {
    let wasm = compile_spec_module(
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
    assert_vanilla(&wasm);
    assert_eq!(
        count_occurrences(&wasm, &[0xfc, 0x31]),
        0,
        "neither body draws: both read their choice parameter"
    );
    let wat = flat(&wat_of(&wasm));
    assert_eq!(
        wat.matches("(param $n i32)").count(),
        2,
        "each sibling carries its own one-parameter choice suffix:\n{wat}"
    );
}

/// A parameter written `_: T` costs a frame slot exactly as a named one does,
/// so the choice suffix begins after it.
///
/// An `exists`-bodied free function is the one shape whose obligation payload
/// denotes against the real activation frame, so the compiler asserts that the
/// observed suffix base equals the plan's recorded entry arity. A plan that
/// counted only the named parameters would put that arity at 0 while the
/// signature had already spent slot 0 on `_`, and the assertion would fire on
/// this program — a panic reachable from a source the front end accepts. Both
/// slots are `i32`, so nothing downstream would catch a suffix placed one slot
/// early: the module would validate and the obligation would read the argument
/// where it expects the drawn value.
#[test]
fn an_unnamed_parameter_costs_a_slot_before_the_choice_suffix() {
    let wasm = compile_spec_module(
        "spec S {
          fn f(_: i32) exists {
            let n: i32 = @;
            assert(n >= n);
          }
        }",
    );
    assert_vanilla(&wasm);
    let wat = flat(&wat_of(&wasm));
    assert!(
        wat.contains("(param i32) (param $n i32)"),
        "the unnamed parameter holds slot 0 and the choice takes slot 1:\n{wat}"
    );
}

/// A specification *method* is planned too. Its receiver — and, when it returns
/// a compound, its sret pointer — precede the declared parameters, so the
/// suffix base must be the local index observed at the suffix site rather than
/// the declared arity. Both would be `i32` here, so a wrong base still
/// validates: only reading back the right value distinguishes them.
#[test]
fn a_spec_method_with_an_sret_return_gets_the_right_suffix_base() {
    let wasm = compile_spec_module(
        "spec S {
          struct T {
            x: i32;
            fn dup(self) -> [i32; 2] {
              forall {
                let n: i32 = @;
                let pair: [i32; 2] = [n, self.x];
                return pair;
              }
            }
          }
        }",
    );
    assert_vanilla(&wasm);
    let wat = flat(&wat_of(&wasm));
    assert!(
        wat.contains("(param $sret i32) (param $self i32) (param $n i32)"),
        "sret pointer at local 0, receiver at local 1, choice at local 2:\n{wat}"
    );
    assert!(
        wat.contains("local.get $n local.set $n"),
        "the choice is read back from its own local; the declared arity is 1, so an \
         absolute `entry_arity + ordinal` would have read the receiver instead — and both \
         are i32, so the module would still validate:\n{wat}"
    );
}

// ----- aggregate choices --------------------------------------------------

/// A one-element array is the shape a leaf *count* cannot tell from a scalar.
/// It must take the aggregate path: a frame slot store per leaf, and a value
/// that is a pointer into the frame rather than the parameter itself.
#[test]
fn a_single_element_array_choice_stores_into_its_frame_slot() {
    cov_mark::check!(wasm_codegen_choice_leaf_cursor);
    let wasm = compile_spec_module(
        "spec S {
          fn f() forall {
            let a: [i32; 1] = @;
            assert(a[0] >= a[0]);
          }
        }",
    );
    assert_vanilla(&wasm);
    let wat = flat(&wat_of(&wasm));
    assert!(
        wat.contains("(param $__choice0 i32)"),
        "one leaf, one anonymous choice parameter:\n{wat}"
    );
    assert!(
        wat.contains("local.get $__frame_ptr i32.const 0 i32.add local.get $__choice0 i32.store"),
        "the leaf must be stored into the frame slot, not bound as the value:\n{wat}"
    );
    assert!(
        wat.contains("local.set $a"),
        "`a` binds a frame pointer through its own local:\n{wat}"
    );
}

/// A struct with exactly one scalar field is the other shape a leaf *count*
/// cannot tell apart from a scalar, and it reaches the aggregate emitters
/// through a different arm than the one-element array does. It has the same
/// requirement: the leaf is stored into a frame slot and the `let` binds a
/// pointer, not the choice parameter itself.
#[test]
fn a_single_field_struct_choice_stores_into_its_frame_slot() {
    let wasm = compile_spec_module(
        "struct Cell { v: i32; }
        spec S {
          fn f() forall {
            let c: Cell = @;
            assert(c.v >= c.v);
          }
        }",
    );
    assert_vanilla(&wasm);
    let wat = flat(&wat_of(&wasm));
    assert!(
        wat.contains("(param $__choice0 i32)"),
        "one field, one anonymous choice parameter:\n{wat}"
    );
    assert!(
        wat.contains("local.get $__frame_ptr i32.const 0 i32.add local.get $__choice0 i32.store"),
        "the field must be stored into the frame slot, not bound as the value:\n{wat}"
    );
    assert!(
        wat.contains("local.set $c"),
        "`c` binds a frame pointer through its own local:\n{wat}"
    );
}

/// Mixed aggregate and scalar choices in one body: three array leaves, a
/// struct's `i32`/`i64` fields, then a scalar `bool`, in exactly that order.
///
/// The obligation for this same body is pinned by
/// `mixed_aggregate_and_scalar_uzumaki_number_their_slots_by_leaf` in
/// `crate::hassert::tests`, which shows the payload numbering its six universal
/// slots over the same leaves in the same order. Neither numbering is derived
/// from the other — the suffix comes from the emitter, the payload from the
/// typed AST — so the pair is what says the lowering added parameters without
/// touching what is proved.
#[test]
fn aggregate_leaves_expand_in_layout_order() {
    let wasm = compile_spec_module(
        "struct Pt { x: i32; y: i64; }
        spec S {
          fn f() forall {
            let a: [i32; 3] = @;
            let p: Pt = @;
            let b: bool = @;
            assert(b || a[0] == 0 || p.x == 0);
          }
        }",
    );
    assert_vanilla(&wasm);
    let wat = flat(&wat_of(&wasm));
    assert!(
        wat.contains(
            "(param $__choice0 i32) (param $__choice1 i32) (param $__choice2 i32) \
             (param $__choice3 i32) (param $__choice4 i64) (param $b i32)"
        ),
        "six parameters: three i32 array leaves, the struct's i32 then i64 field, \
         then the named bool:\n{wat}"
    );
}

/// A struct field of array type expands to one parameter per element, and a
/// narrow leaf keeps the store-width round-trip that constrains its domain.
#[test]
fn a_narrow_struct_array_field_expands_per_element() {
    let wasm = compile_spec_module(
        "struct Row { tag: u8; cells: [i16; 2]; }
        spec S {
          fn f() forall {
            let r: Row = @;
            assert(r.tag >= r.tag);
          }
        }",
    );
    assert_vanilla(&wasm);
    let wat = flat(&wat_of(&wasm));
    assert!(
        wat.contains("(param $__choice0 i32) (param $__choice1 i32) (param $__choice2 i32)"),
        "one parameter for the u8 tag and one per i16 cell:\n{wat}"
    );
    assert!(
        wat.contains("i32.store8") && wat.contains("i32.store16"),
        "the store widths that carry each leaf's domain must be unchanged:\n{wat}"
    );
}

/// A multi-dimensional array expands to every leaf of every dimension.
#[test]
fn a_multidimensional_array_choice_expands_to_every_leaf() {
    let wasm = compile_spec_module(
        "spec S {
          fn f() forall {
            let a: [[i64; 2]; 3] = @;
            assert(a[0][0] >= a[0][0]);
          }
        }",
    );
    assert_vanilla(&wasm);
    let wat = flat(&wat_of(&wasm));
    assert!(
        wat.contains(
            "(param $__choice0 i64) (param $__choice1 i64) (param $__choice2 i64) \
             (param $__choice3 i64) (param $__choice4 i64) (param $__choice5 i64)"
        ),
        "2 x 3 leaves, all i64:\n{wat}"
    );
}

// ----- the parameter ceiling ----------------------------------------------

/// A choice suffix past WebAssembly's parameter limit is refused with a
/// diagnostic naming the specification function, rather than emitted as a
/// module this compiler's own verification step cannot parse.
#[test]
fn a_choice_suffix_past_the_parameter_ceiling_is_rejected() {
    cov_mark::check!(wasm_codegen_choice_suffix_too_large);
    let draws = (0..1001)
        .map(|k| format!("let v{k}: i32 = @;"))
        .collect::<Vec<_>>()
        .join(" ");
    let ctx = type_check(&format!(
        "spec S {{ fn f() forall {{ {draws} assert(v0 >= v0); }} }}"
    ));
    let err = crate::codegen(
        &ctx,
        "choice_test",
        crate::CodegenOptions {
            target: crate::Target::Wasm32,
            mode: CompilationMode::Proof,
            opt_level: crate::OptLevel::O0,
            features: crate::EmitFeatures::default(),
            layout: crate::MemoryLayout::default(),
        },
    )
    .expect_err("1001 choice parameters overflow WebAssembly's limit");
    let msg = err.to_string();
    assert!(
        msg.contains("'f'") && msg.contains("'S'") && msg.contains("1000"),
        "the ceiling diagnostic must name the function, the spec, and the limit: {msg}"
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
        "choice_test",
        crate::CodegenOptions {
            target: crate::Target::Wasm32,
            mode: CompilationMode::Proof,
            opt_level: crate::OptLevel::O0,
            features: crate::EmitFeatures::default(),
            layout: crate::MemoryLayout::default(),
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
        "choice_test",
        crate::CodegenOptions {
            target: crate::Target::Wasm32,
            mode: CompilationMode::Proof,
            opt_level: crate::OptLevel::O0,
            features: crate::EmitFeatures::default(),
            layout: crate::MemoryLayout::default(),
        },
    )
    .expect_err("a return statement in a unique body must fail codegen");
    let msg = err.to_string();
    assert!(
        msg.contains("contains a `return` statement") && msg.contains("'unique'-quantified"),
        "expected the return-statement clause of the no-return rule: {msg}"
    );
}
