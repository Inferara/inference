//! End-to-end tests for proof-mode `hassert` obligation emission.
//!
//! These drive the *whole* compiler front end — parse, type-check, and generate
//! WASM in proof mode — and inspect the [`CodegenOutput::hspecs`] the code
//! generator now carries. They complement the in-crate unit tests of the
//! translation pass with the guarantees only the full pipeline can make: that
//! the obligation survives real code generation, that compile mode carries none,
//! and that the corpus of existing spec fixtures still translates cleanly.
//!
//! They also pin that a specification body types its integer literals from the
//! positions they appear in, exactly as executable code does. The two run in one
//! traversal over one type table, so this is a property of the design rather than
//! of a shared code path — but it is the property the whole obligation rests on:
//! an obligation whose constants are not the program's constants is about a
//! different program than the one that runs.

#![cfg(test)]

use inference_wasm_codegen::{
    CompilationMode, HAssert, HBinop, HConst, HFnRef, HNumType, HRelop, HSpecMap, HTerm, Target,
};

use crate::utils::{
    codegen_output, codegen_with_target_mode_no_analysis, get_test_data_path,
    try_proof_codegen_multi_file_no_analysis,
};

/// Compiles source in proof mode (analysis skipped, so spec-only shapes are
/// exercised directly) and returns its obligation map.
fn proof_hspecs(source: &str) -> HSpecMap {
    codegen_with_target_mode_no_analysis(source, Target::Wasm32, CompilationMode::Proof)
        .expect("proof-mode codegen should succeed")
        .hspecs()
        .clone()
}

/// Reads a fixture from `tests/test_data/inf/`.
fn read_inf(file: &str) -> String {
    let path = get_test_data_path().join("inf").join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn sole_obligation(map: &HSpecMap, spec: &str) -> HAssert {
    let entries = map.get(spec).unwrap_or_else(|| {
        panic!(
            "no spec `{spec}`; have {:?}",
            map.keys().collect::<Vec<_>>()
        )
    });
    assert_eq!(
        entries.len(),
        1,
        "expected exactly one obligation for `{spec}`"
    );
    entries[0].hassert.clone()
}

// primitive expected-tree builders
fn i32c(v: i32) -> HTerm {
    HTerm::Const(HConst::I32(v))
}
fn i64c(v: i64) -> HTerm {
    HTerm::Const(HConst::I64(v))
}
fn local(n: u32) -> HTerm {
    HTerm::Local(n)
}
fn lvar(n: u32) -> HTerm {
    HTerm::LVar(n)
}
fn app(name: &str, args: Vec<HTerm>) -> HTerm {
    HTerm::App(HFnRef(name.to_string()), args)
}
fn rel(op: HRelop, l: HTerm, r: HTerm) -> HTerm {
    HTerm::Relop(HNumType::I32, op, Box::new(l), Box::new(r))
}
fn rel64(op: HRelop, l: HTerm, r: HTerm) -> HTerm {
    HTerm::Relop(HNumType::I64, op, Box::new(l), Box::new(r))
}
fn add64(l: HTerm, r: HTerm) -> HTerm {
    HTerm::Binop(HNumType::I64, HBinop::Add, Box::new(l), Box::new(r))
}
fn rems(l: HTerm, r: HTerm) -> HTerm {
    HTerm::Binop(HNumType::I32, HBinop::RemS, Box::new(l), Box::new(r))
}
fn not(a: HAssert) -> HAssert {
    HAssert::Not(Box::new(a))
}
fn and(a: HAssert, b: HAssert) -> HAssert {
    HAssert::And(Box::new(a), Box::new(b))
}
fn imp(a: HAssert, b: HAssert) -> HAssert {
    HAssert::Imp(Box::new(a), Box::new(b))
}
fn ex(a: HAssert) -> HAssert {
    HAssert::Ex(Box::new(a))
}
fn all(a: HAssert) -> HAssert {
    HAssert::All(Box::new(a))
}
fn teq(a: HTerm, b: HTerm) -> HAssert {
    HAssert::TermEq(a, b)
}
fn nz(t: HTerm) -> HAssert {
    not(teq(t, i32c(0)))
}
fn hastype(t: HTerm, ty: HNumType) -> HAssert {
    HAssert::HasType(t, ty)
}

/// Building the PrimeExample source through the whole front end must yield an
/// obligation structurally equal to wasm-verifier's `prime_hspec1`
/// (its `theories/examples/PrimeExample.v`).
#[test]
fn prime_example_end_to_end_matches_prime_hspec1() {
    // The else-arm existential binder is named `k` rather than `m`: two `let m`
    // in one function share a WASM local slot (analysis rule A041 forbids it),
    // and the obligation's shape is independent of the binder's source name —
    // the then-arm `m` is a `T_local` slot and the else-arm binder is a `T_lvar`.
    let source = "\
fn is_prime(n: i32) -> bool {
  return n > 1;
}

spec prime_properties {
  fn prime_spec() forall {
    let n: i32 = @;
    assume { assert(n > 1); }
    if is_prime(n) {
      let m: i32 = @;
      assume { assert(m > 1 && m < n); }
      assert(n % m > 0);
    } else exists {
      let k: i32 = @;
      assume { assert(k > 1 && k < n); }
      assert(n % k == 0);
    }
  }
}
";
    let map = proof_hspecs(source);
    let entries = map.get("prime_properties").expect("prime_properties spec");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].fn_symbol,
        HFnRef("prime_properties.prime_spec".to_string())
    );

    let n = || local(0);
    let m_then = || local(1);
    let m_ex = || lvar(0);
    let one = || i32c(1);
    let gts = |l: HTerm, r: HTerm| rel(HRelop::GtS, l, r);
    let lts = |l: HTerm, r: HTerm| rel(HRelop::LtS, l, r);
    let is_prime = || app("is_prime", vec![n()]);

    let expected = imp(
        and(hastype(n(), HNumType::I32), nz(gts(n(), one()))),
        and(
            imp(
                nz(is_prime()),
                imp(
                    and(
                        hastype(m_then(), HNumType::I32),
                        and(nz(gts(m_then(), one())), nz(lts(m_then(), n()))),
                    ),
                    nz(gts(rems(n(), m_then()), i32c(0))),
                ),
            ),
            imp(
                teq(is_prime(), i32c(0)),
                ex(and(
                    and(nz(gts(m_ex(), one())), nz(lts(m_ex(), n()))),
                    teq(rems(n(), m_ex()), i32c(0)),
                )),
            ),
        ),
    );
    assert_eq!(entries[0].hassert, expected);
}

/// `with_spec.inf` (a `forall` fn asserting `foo(i) == i`) produces the
/// `nz(relop_eq(app, local))` claim under the typing guard of the universal slot
/// it reads — structurally wasm-verifier's `with_spec__MySpec_hspec1_guarded`
/// (its `theories/examples/with_spec.v`), the payload that file proves the
/// hardened `ValidSpec` accepts where the unguarded one is rejected.
#[test]
fn with_spec_fixture_produces_the_expected_app_equality() {
    let map = proof_hspecs(&read_inf("with_spec.inf"));
    assert_eq!(
        sole_obligation(&map, "MySpec"),
        imp(
            hastype(local(0), HNumType::I32),
            nz(rel(HRelop::Eq, app("foo", vec![local(0)]), local(0)))
        )
    );
}

/// A spec free function claiming a property *about a file-scope function* turns
/// that call into a `T_app` over the compiled function, which is the only way an
/// obligation can be about the program at all: the spec block holds the claim
/// and the executable file scope holds the computation it constrains.
#[test]
fn spec_calls_top_fixture_applies_the_file_scope_function() {
    let map = proof_hspecs(&read_inf("spec_calls_top.inf"));
    assert_eq!(
        sole_obligation(&map, "Caller"),
        nz(rel(HRelop::Eq, app("helper", vec![]), i32c(7)))
    );
}

/// Three specs of mixed shapes: only `Alpha` has a free function, so only
/// `Alpha` carries an obligation. `Beta` (a struct method that only computes)
/// and `Gamma` (empty) contribute none and so do not appear in the map.
#[test]
fn three_specs_fixture_only_maps_the_free_function_spec() {
    let map = proof_hspecs(&read_inf("three_specs.inf"));
    assert_eq!(
        sole_obligation(&map, "Alpha"),
        nz(rel(HRelop::Eq, app("one", vec![]), i32c(1)))
    );
    assert!(
        !map.contains_key("Beta"),
        "Beta has only a method helper, no obligation"
    );
    assert!(!map.contains_key("Gamma"), "Gamma is empty");
}

/// A spec whose only inner definition is a struct with `Regular` methods carries
/// no obligation (the methods are callable helpers), and translates cleanly.
#[test]
fn spec_method_fixture_translates_without_obligations() {
    let map = proof_hspecs(&read_inf("spec_method.inf"));
    assert!(
        map.is_empty(),
        "spec methods are helpers, not obligations: {map:?}"
    );
}

/// A specification body types its integer literals from the positions they
/// appear in, exactly as executable code does — the same traversal types both.
///
/// Every literal here is wider than `i32`, so an obligation carrying `Vi32`
/// constants would be about a different program than the one that runs: the
/// peer operand of a comparison and the operand of `i64` arithmetic must both
/// come out as `HConst::I64`.
#[test]
fn spec_literals_take_the_peer_and_operand_types_at_i64() {
    let source = "\
fn scaled(n: i64) -> i64 {
  return n * 2;
}

spec Widths {
  fn widths() forall {
    let n: i64 = @;
    assume { assert(n > 4294967296); }
    assert(scaled(n) > n + 1);
  }
}
";
    let n = || local(0);
    let expected = imp(
        and(
            hastype(n(), HNumType::I64),
            not(teq(rel64(HRelop::GtS, n(), i64c(4_294_967_296)), i32c(0))),
        ),
        not(teq(
            rel64(HRelop::GtS, app("scaled", vec![n()]), add64(n(), i64c(1))),
            i32c(0),
        )),
    );
    assert_eq!(sole_obligation(&proof_hspecs(source), "Widths"), expected);
}

/// A literal at a `u64` parameter is typed by that parameter, which is the only
/// way `u64::MAX` is expressible at all — it fits no other integer type. The
/// obligation carries it as the `i64` bit pattern `-1`, the same reinterpretation
/// code generation performs, and the surrounding comparison is unsigned.
#[test]
fn spec_argument_literal_takes_a_u64_parameter_type() {
    let source = "\
fn is_max(n: u64) -> bool {
  return n == 18446744073709551615;
}

spec MaxArg {
  fn max_arg() forall {
    let n: u64 = @;
    assume { assert(n > 0); }
    assert(is_max(18446744073709551615));
  }
}
";
    let expected = imp(
        and(
            hastype(local(0), HNumType::I64),
            not(teq(rel64(HRelop::GtU, local(0), i64c(0)), i32c(0))),
        ),
        not(teq(app("is_max", vec![i64c(-1)]), i32c(0))),
    );
    assert_eq!(sole_obligation(&proof_hspecs(source), "MaxArg"), expected);
}

/// The `spec_literal_ctx.inf` fixture places `i64`/`u64` literals in the return,
/// argument and operand positions of one specification, and both of its
/// obligations carry a value with no `i32` reading — left at the default, none of
/// them could be spelled at all.
///
/// `threshold_is_i64` is the return position: the file-scope `threshold`'s
/// declared `-> i64` is what types its `return 4294967296;`, and the obligation
/// compares the `T_app` against a peer literal that has to come out at the same
/// width for the claim to be about that function's result.
///
/// `scaled_grows`' antecedent carries both halves of its `assume`, and the upper
/// half is a literal no `i32` reading could express either: it is the operand
/// bound that keeps the doubling inside `i64`, so the widths this test is about
/// are what state the range as well as what the range is about.
#[test]
fn spec_literal_ctx_fixture_types_return_argument_and_operand_positions() {
    let map = proof_hspecs(&read_inf("spec_literal_ctx.inf"));
    let entries = map
        .get("LiteralPositions")
        .expect("spec LiteralPositions should carry obligations");
    let by_symbol = |name: &str| {
        entries
            .iter()
            .find(|e| e.fn_symbol == HFnRef(name.to_string()))
            .unwrap_or_else(|| {
                panic!(
                    "no obligation for `{name}`; have {:?}",
                    entries.iter().map(|e| &e.fn_symbol).collect::<Vec<_>>()
                )
            })
            .hassert
            .clone()
    };

    assert_eq!(
        by_symbol("LiteralPositions.threshold_is_i64"),
        nz(rel64(
            HRelop::Eq,
            app("threshold", vec![]),
            i64c(4_294_967_296)
        ))
    );

    let n = || local(0);
    let expected = imp(
        and(
            hastype(n(), HNumType::I64),
            and(
                not(teq(rel64(HRelop::GtS, n(), i64c(4_294_967_296)), i32c(0))),
                not(teq(
                    rel64(HRelop::LeS, n(), i64c(4_611_686_018_427_387_903)),
                    i32c(0),
                )),
            ),
        ),
        and(
            not(teq(
                rel64(HRelop::GtS, app("scaled", vec![n()]), add64(n(), i64c(1))),
                i32c(0),
            )),
            not(teq(app("nonzero", vec![i64c(-1)]), i32c(0))),
        ),
    );
    assert_eq!(by_symbol("LiteralPositions.scaled_grows"), expected);
}

/// An `exists`-bodied spec function survives the whole pipeline: proof-mode
/// codegen succeeds (the P001 rejection is lifted), the obligation binds the
/// entry parameter and both choices to their frame slots — the named `let`
/// choice at 1, the anonymous call-argument choice at 2 — with no `HA_ex`
/// binder and no typing guard, and the entry carries the exists kind whose
/// `visible_locs` include the named choice but not the anonymous one.
///
/// The sum is marked modular because this body is retained and reduced, where
/// an operator that traps is what `P017` refuses; the annotation is dropped
/// from the term, which is the shape asserted below.
#[test]
fn exists_spec_end_to_end_reads_frame_slots_under_its_kind() {
    let source = "\
fn g(v: i32) -> i32 {
  return v;
}

spec Reach {
  fn f(x: i32) exists {
    let n: i32 = @;
    assume { assert(n > 0); }
    assert(g(@) == wrapping(x + n));
  }
}
";
    let map = proof_hspecs(source);
    let entries = map.get("Reach").expect("spec Reach");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].fn_symbol, HFnRef("Reach.f".to_string()));
    let add =
        |l: HTerm, r: HTerm| HTerm::Binop(HNumType::I32, HBinop::Add, Box::new(l), Box::new(r));
    assert_eq!(
        entries[0].hassert,
        and(
            nz(rel(HRelop::GtS, local(1), i32c(0))),
            teq(app("g", vec![local(2)]), add(local(0), local(1))),
        )
    );
    assert_eq!(
        entries[0].kind,
        inference_hassert::SpecKind::Exists(inference_hassert::ReachMeta {
            entry_arity: 1,
            visible_locs: vec![0, 1],
        })
    );
}

/// A `unique`-bodied spec function takes the same pipeline under its own kind:
/// identical statement semantics (`==` is the strict `term_eq`), the named
/// choice in `visible_locs` — the projection the uniqueness judgment compares
/// exit states through.
#[test]
fn unique_spec_end_to_end_reads_frame_slots_under_its_kind() {
    let source = "\
spec Reach {
  fn f(x: i32) unique {
    let n: i32 = @;
    assert(n == x);
  }
}
";
    let map = proof_hspecs(source);
    let entries = map.get("Reach").expect("spec Reach");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].hassert, teq(local(1), local(0)));
    assert_eq!(
        entries[0].kind,
        inference_hassert::SpecKind::Unique(inference_hassert::ReachMeta {
            entry_arity: 1,
            visible_locs: vec![0, 1],
        })
    );
}

/// Compile mode strips specs, so no obligation is ever attached.
#[test]
fn compile_mode_carries_no_obligations() {
    let source = "\
fn is_prime(n: i32) -> bool {
  return n > 1;
}

spec prime_properties {
  fn prime_spec() forall {
    let n: i32 = @;
    assume { assert(n > 1); }
    assert(is_prime(n));
  }
}
";
    let output = codegen_output(source);
    assert!(output.hspecs().is_empty());
}

/// The issue's acceptance shape survives the whole pipeline: a compound `@`
/// binds one guarded universal slot per scalar leaf, and the constant-index
/// read is that leaf's term.
#[test]
fn aggregate_uzumaki_produces_leaf_slots_end_to_end() {
    let source = "\
spec Agg {
  fn leaf_bounds() forall {
    let a: [i32; 3] = @;
    assert(a[0] <= a[0]);
  }
}
";
    let expected = imp(
        and(
            hastype(local(0), HNumType::I32),
            and(
                hastype(local(1), HNumType::I32),
                hastype(local(2), HNumType::I32),
            ),
        ),
        nz(rel(HRelop::LeS, local(0), local(0))),
    );
    assert_eq!(sole_obligation(&proof_hspecs(source), "Agg"), expected);
}

/// A struct parameter leaf-expands through real code generation — the
/// compiled function keeps its single pointer parameter while the payload
/// quantifies one slot per scalar leaf, at each leaf's own width.
#[test]
fn struct_parameter_leaves_carry_their_widths_end_to_end() {
    let source = "\
struct Rec {
  lo: i32;
  wide: i64;
  row: [i32; 2];
}

spec Agg {
  fn record(r: Rec) forall {
    assert(r.wide > r.wide - 1);
  }
}
";
    let expected = imp(
        and(
            hastype(local(0), HNumType::I32),
            and(
                hastype(local(1), HNumType::I64),
                and(
                    hastype(local(2), HNumType::I32),
                    hastype(local(3), HNumType::I32),
                ),
            ),
        ),
        nz(rel64(
            HRelop::GtS,
            local(1),
            HTerm::Binop(
                HNumType::I64,
                HBinop::Sub,
                Box::new(local(1)),
                Box::new(i64c(1)),
            ),
        )),
    );
    assert_eq!(sole_obligation(&proof_hspecs(source), "Agg"), expected);
}

/// Aggregate equality is leafwise through the whole pipeline, and a bound
/// literal's leaves are its own translated constants. (The literal is bound
/// by a `let` rather than written as a comparison operand because the
/// executable lowering of the same body only places a literal where an
/// enclosing variable names its frame slot.)
#[test]
fn aggregate_equality_is_leafwise_end_to_end() {
    let source = "\
spec Agg {
  fn pinned() forall {
    let a: [i32; 2] = @;
    let b: [i32; 2] = [1, 2];
    assert(a == b);
  }
}
";
    let expected = imp(
        and(
            hastype(local(0), HNumType::I32),
            hastype(local(1), HNumType::I32),
        ),
        and(teq(local(0), i32c(1)), teq(local(1), i32c(2))),
    );
    assert_eq!(sole_obligation(&proof_hspecs(source), "Agg"), expected);
}

/// The constraint a non-constant index pins its element with, built
/// independently of the pass: the unsigned range bound first, then one
/// implication per element.
fn element_def(index: &HTerm, witness: &HTerm, leaves: &[HTerm]) -> HAssert {
    let extent = i32::try_from(leaves.len()).expect("test extents are small");
    let cases = leaves
        .iter()
        .enumerate()
        .rev()
        .fold(HAssert::True, |acc, (case, leaf)| {
            let case = i32::try_from(case).expect("test extents are small");
            HAssert::and(
                imp(
                    teq(index.clone(), i32c(case)),
                    teq(witness.clone(), leaf.clone()),
                ),
                acc,
            )
        });
    and(nz(rel(HRelop::LtU, index.clone(), i32c(extent))), cases)
}

/// The issue's bounded-iteration acceptance shape survives the whole pipeline:
/// an array `@`, an index `@`, a range `assume`, and a claim about the element
/// at that index. The element is a fresh binder defined by the index's
/// unsigned range and one case per element, and the definition is conjoined
/// with the claim — which is what makes an out-of-range index refute the
/// obligation rather than discharge it vacuously.
#[test]
fn bounded_iteration_pins_its_element_by_cases_end_to_end() {
    let source = "\
spec Iter {
  fn element_defined() forall {
    let a: [i32; 3] = @;
    let i: i32 = @;
    assume { assert(0 <= i && i < 3); }
    assert(a[i] == a[i]);
  }
}
";
    let leaves = [local(0), local(1), local(2)];
    // Both reads bind their own witness; each reads as de Bruijn index 0
    // inside its own binder.
    let definition = || element_def(&local(3), &lvar(0), &leaves);
    let filter = and(
        nz(rel(HRelop::LeS, i32c(0), local(3))),
        nz(rel(HRelop::LtS, local(3), i32c(3))),
    );
    let expected = imp(
        and(
            hastype(local(0), HNumType::I32),
            and(
                hastype(local(1), HNumType::I32),
                and(
                    hastype(local(2), HNumType::I32),
                    and(hastype(local(3), HNumType::I32), filter),
                ),
            ),
        ),
        ex(and(
            definition(),
            ex(and(definition(), nz(rel(HRelop::Eq, lvar(1), lvar(0))))),
        )),
    );
    assert_eq!(sole_obligation(&proof_hspecs(source), "Iter"), expected);
}

/// A constant step of an access chain descends before the non-constant one, so
/// `m[1][j]` splits over the two elements of row `[1]` rather than over the
/// four elements of the matrix.
#[test]
fn a_constant_step_descends_before_the_non_constant_one_end_to_end() {
    let source = "\
spec Iter {
  fn matrix_row(j: i32) forall {
    let m: [[i32; 2]; 2] = @;
    assert(m[1][j] == m[1][0]);
  }
}
";
    // Slot 0 is the declared parameter `j`; the matrix takes slots 1..4.
    let definition = element_def(&local(0), &lvar(0), &[local(3), local(4)]);
    let expected = imp(
        and(
            hastype(local(0), HNumType::I32),
            and(
                hastype(local(1), HNumType::I32),
                and(
                    hastype(local(2), HNumType::I32),
                    and(
                        hastype(local(3), HNumType::I32),
                        hastype(local(4), HNumType::I32),
                    ),
                ),
            ),
        ),
        ex(and(definition, nz(rel(HRelop::Eq, lvar(0), local(3))))),
    );
    assert_eq!(sole_obligation(&proof_hspecs(source), "Iter"), expected);
}

/// Quantifier alternation survives the whole pipeline: a `forall` block nested
/// inside an `exists` block binds a universal *logical variable* under the
/// existential witness, with its typing stated as an antecedent inside its own
/// binder.
///
/// The nesting order is the claim. A slot standing in for the inner `forall`
/// would be quantified by the downstream judgment — outside the `HA_ex` — and
/// `∃k. ∀x` would silently read as `∀x. ∃k`, which is a different and weaker
/// property.
#[test]
fn quantifier_alternation_nests_a_universal_under_an_existential_end_to_end() {
    let source = "\
spec Alt {
  fn additive_identity() forall {
    exists {
      let k: i32 = @;
      assume { assert(k == 0); }
      forall {
        let x: i32 = @;
        assert(x + k == x);
      }
    }
  }
}
";
    let expected = ex(and(
        teq(lvar(0), i32c(0)),
        all(imp(
            hastype(lvar(0), HNumType::I32),
            nz(rel(
                HRelop::Eq,
                HTerm::Binop(
                    HNumType::I32,
                    HBinop::Add,
                    Box::new(lvar(0)),
                    Box::new(lvar(1)),
                ),
                lvar(0),
            )),
        )),
    ));
    assert_eq!(sole_obligation(&proof_hspecs(source), "Alt"), expected);
}

/// The alternation fixture the `coqc` corpus compiles also translates through
/// real code generation, and every one of its obligations carries a universal
/// binder — it is the fixture that exercises the shape at every nesting the
/// language admits, so a body that stopped emitting one would leave that stub
/// declaration resting on whatever else happens to nest a `forall`.
#[test]
fn quantifier_alternation_fixture_emits_a_universal_binder_in_every_obligation() {
    let map = proof_hspecs(&read_inf("spec_quantifier_alternation.inf"));
    let entries = map
        .get("QuantifierAlternation")
        .expect("the alternation spec must produce obligations");
    assert_eq!(entries.len(), 6, "one obligation per spec free function");
    for entry in entries {
        assert!(
            binds_a_universal(&entry.hassert),
            "`{}` must bind a universal logical variable: {:?}",
            entry.fn_symbol.0,
            entry.hassert
        );
    }
}

/// Whether the tree binds a universal logical variable anywhere.
fn binds_a_universal(a: &HAssert) -> bool {
    match a {
        HAssert::All(_) => true,
        HAssert::Not(x) | HAssert::Ex(x) => binds_a_universal(x),
        HAssert::And(l, r) | HAssert::Imp(l, r) | HAssert::Or(l, r) => {
            binds_a_universal(l) || binds_a_universal(r)
        }
        HAssert::True
        | HAssert::False
        | HAssert::TermEq(_, _)
        | HAssert::HasType(_, _)
        | HAssert::Defined(_)
        | HAssert::AppOk(_, _) => false,
    }
}

// narrow-domain expected-tree builders

/// `x <u hi_excl`, non-zero: the bound a zero-extending width, a `bool` or an
/// enum tag is quantified under. The comparison is unsigned because the
/// normalization these widths receive zero-extends.
fn below_u(x: HTerm, hi_excl: i32) -> HAssert {
    nz(rel(HRelop::LtU, x, i32c(hi_excl)))
}

/// `lo_incl <=s x /\ x <s hi_excl`, both halves non-zero and in the order the
/// pass conjoins them: a sign-extending width has a lower bound as well as an
/// upper one, and one half alone characterizes nothing.
fn between_s(x: HTerm, lo_incl: i32, hi_excl: i32) -> HAssert {
    and(
        nz(rel(HRelop::LeS, i32c(lo_incl), x.clone())),
        nz(rel(HRelop::LtS, x, i32c(hi_excl))),
    )
}

/// A universal introduction's guard: the class its values ride in and the set
/// its declaration admits, grouped into one conjunct — the shape both the guard
/// channel and a universal binder build.
fn guarded(x: HTerm, ty: HNumType, domain: HAssert) -> HAssert {
    and(hastype(x, ty), domain)
}

/// A narrow `@` states the values its declaration admits beside the class they
/// ride in, through the whole front end. Both signednesses appear in one body,
/// in the order their introductions drained: an unsigned bound for the
/// zero-extending width, a two-sided signed pair for the sign-extending one.
#[test]
fn a_narrow_uzumaki_states_its_declared_domain_end_to_end() {
    let source = "\
spec Narrow {
  fn draws() forall {
    let a: u8 = @;
    let b: i8 = @;
    assert(a <= 255);
    assert(b >= -128);
  }
}
";
    let expected = imp(
        and(
            guarded(local(0), HNumType::I32, below_u(local(0), 256)),
            guarded(local(1), HNumType::I32, between_s(local(1), -128, 128)),
        ),
        and(
            nz(rel(HRelop::LeU, local(0), i32c(255))),
            nz(rel(HRelop::GeS, local(1), i32c(-128))),
        ),
    );
    assert_eq!(sole_obligation(&proof_hspecs(source), "Narrow"), expected);
}

/// A narrow *parameter* states the same set a narrow draw does. A `spec`
/// function is compiled in proof mode but never exported, so its parameters
/// receive neither the entry ABI's normalization nor an enum tag guard: the
/// antecedent is the only place the declaration's meaning is written down.
#[test]
fn a_narrow_spec_parameter_states_its_declared_domain_end_to_end() {
    let source = "\
spec Narrow {
  fn param(p: u16) forall {
    assert(p <= 65535);
  }
}
";
    let expected = imp(
        guarded(local(0), HNumType::I32, below_u(local(0), 65536)),
        nz(rel(HRelop::LeU, local(0), i32c(65535))),
    );
    assert_eq!(sole_obligation(&proof_hspecs(source), "Narrow"), expected);
}

/// An aggregate leaf is bounded at its declared *element* type, one bound per
/// leaf. The element's narrowing happens at the load that reads it back, so a
/// leaf admits exactly the set its scalar counterpart does.
#[test]
fn narrow_array_leaves_state_their_element_domain_end_to_end() {
    let source = "\
spec Narrow {
  fn leaves() forall {
    let a: [i16; 2] = @;
    assert(a[0] <= a[1]);
  }
}
";
    let expected = imp(
        and(
            guarded(local(0), HNumType::I32, between_s(local(0), -32768, 32768)),
            guarded(local(1), HNumType::I32, between_s(local(1), -32768, 32768)),
        ),
        nz(rel(HRelop::LeS, local(0), local(1))),
    );
    assert_eq!(sole_obligation(&proof_hspecs(source), "Narrow"), expected);
}

/// A struct field's leaf is bounded by the field's own declared type, and a
/// full-width field beside it keeps the bare typing guard it always had — the ⊤
/// a full-width domain builds is absorbed rather than emitted.
#[test]
fn a_narrow_struct_field_leaf_states_its_domain_end_to_end() {
    let source = "\
struct Pixel {
  level: u8;
  wide: i32;
}

spec Narrow {
  fn field(p: Pixel) forall {
    assert(p.level <= 255);
  }
}
";
    let expected = imp(
        and(
            guarded(local(0), HNumType::I32, below_u(local(0), 256)),
            hastype(local(1), HNumType::I32),
        ),
        nz(rel(HRelop::LeU, local(0), i32c(255))),
    );
    assert_eq!(sole_obligation(&proof_hspecs(source), "Narrow"), expected);
}

/// Existentially the bound is a *conjunct inside the binder*, never an
/// antecedent, and the binder carries no typing guard at all: the value is the
/// prover's to choose, so there is no unconstrained valuation to type — only a
/// set to keep the choice inside.
///
/// The polarity is the whole point. An implication under `HA_ex` would let a
/// proof pick an out-of-domain witness, refute the guard, and discharge the
/// obligation with nothing said about the claim.
#[test]
fn an_existential_narrow_uzumaki_bounds_its_witness_end_to_end() {
    let source = "\
spec Narrow {
  fn witness() forall {
    exists {
      let m: u8 = @;
      assert(m == 200);
    }
  }
}
";
    let expected = ex(and(below_u(lvar(0), 256), teq(lvar(0), i32c(200))));
    assert_eq!(sole_obligation(&proof_hspecs(source), "Narrow"), expected);
}

/// An enum is bounded by its variant count rather than by a width, and the
/// bound is unsigned like the zero-extending widths — the tag normalization it
/// mirrors is an `i32.rem_u`.
#[test]
fn an_enum_uzumaki_states_its_variant_count_end_to_end() {
    let source = "\
enum Color {
  Red,
  Green,
  Blue,
}

fn id_color(v: Color) -> Color {
  return v;
}

spec Narrow {
  fn tag() forall {
    let c: Color = @;
    assert(id_color(c) == c);
  }
}
";
    let expected = imp(
        guarded(local(0), HNumType::I32, below_u(local(0), 3)),
        nz(rel(HRelop::Eq, app("id_color", vec![local(0)]), local(0))),
    );
    assert_eq!(sole_obligation(&proof_hspecs(source), "Narrow"), expected);
}

/// `nz(l op r)` at `i32`, matching only the operands supplied: `None` accepts
/// whatever term an introduction assigned to the variable being bounded, which
/// is what lets a bound be recognized without knowing which slot or binder
/// level it landed on.
fn is_nz_relop(a: &HAssert, op: HRelop, lhs: Option<&HTerm>, rhs: Option<&HTerm>) -> bool {
    let HAssert::Not(equality) = a else {
        return false;
    };
    let HAssert::TermEq(HTerm::Relop(HNumType::I32, found, l, r), zero) = &**equality else {
        return false;
    };
    *found == op
        && *zero == i32c(0)
        && lhs.is_none_or(|want| **l == *want)
        && rhs.is_none_or(|want| **r == *want)
}

/// Whether `a` is an unsigned upper bound at `hi_excl`, over any term.
fn is_below_u(a: &HAssert, hi_excl: i32) -> bool {
    is_nz_relop(a, HRelop::LtU, None, Some(&i32c(hi_excl)))
}

/// Whether `a` is a signed two-sided bound at `lo_incl ..< hi_excl`, over any
/// term. Both halves are required: a signed width that stated only its upper
/// bound would still be quantified over every negative value.
fn is_between_s(a: &HAssert, lo_incl: i32, hi_excl: i32) -> bool {
    let HAssert::And(lo, hi) = a else {
        return false;
    };
    is_nz_relop(lo, HRelop::LeS, Some(&i32c(lo_incl)), None)
        && is_nz_relop(hi, HRelop::LtS, None, Some(&i32c(hi_excl)))
}

/// Whether any obligation of `spec` contains an assertion `pred` accepts.
fn some_obligation_states(map: &HSpecMap, spec: &str, pred: &dyn Fn(&HAssert) -> bool) -> bool {
    let entries = map.get(spec).unwrap_or_else(|| {
        panic!(
            "no spec `{spec}`; have {:?}",
            map.keys().collect::<Vec<_>>()
        )
    });
    entries
        .iter()
        .any(|entry| holds_anywhere(&entry.hassert, pred))
}

/// Whether `pred` accepts `a` or any assertion inside it.
fn holds_anywhere(a: &HAssert, pred: &dyn Fn(&HAssert) -> bool) -> bool {
    if pred(a) {
        return true;
    }
    match a {
        HAssert::Not(x) | HAssert::Ex(x) | HAssert::All(x) => holds_anywhere(x, pred),
        HAssert::And(l, r) | HAssert::Imp(l, r) | HAssert::Or(l, r) => {
            holds_anywhere(l, pred) || holds_anywhere(r, pred)
        }
        HAssert::True
        | HAssert::False
        | HAssert::TermEq(_, _)
        | HAssert::HasType(_, _)
        | HAssert::Defined(_)
        | HAssert::AppOk(_, _) => false,
    }
}

/// A narrow choice in a reachability body carries no bound, and that absence is
/// the correct emission rather than a gap.
///
/// An `exists`-quantified function states a reachability claim: its choices are
/// parameters of the runs the judgment quantifies, not variables an assertion
/// binds, so there is no valuation for a bound to narrow. What makes reading the
/// parameter raw sound is elsewhere — code generation writes a *named* choice's
/// narrowed value back into the parameter itself, so the payload and the
/// compiled body read one value.
#[test]
fn a_narrow_reachability_choice_carries_no_bound_end_to_end() {
    let source = "\
fn id_u8(v: u8) -> u8 {
  return v;
}

spec Reach {
  fn choose() exists {
    let c: u8 = @;
    assert(id_u8(c) == c);
  }
}
";
    // `==` in a reachability payload is the strict `term_eq` the judgment
    // compares exit states through, not a relop the body computes.
    let expected = teq(app("id_u8", vec![local(0)]), local(0));
    assert_eq!(sole_obligation(&proof_hspecs(source), "Reach"), expected);
}

/// The two narrow fixtures the `coqc` corpus compiles carry every row of the
/// domain table between them, through real code generation.
///
/// The gate over the corpus matches the *printed* `.v`; this matches the tree,
/// so a printer change cannot stand in for an emission one. The bounds are
/// recognized without naming the slot or binder level they landed on, so
/// reordering a fixture's own introductions cannot break the claim — what it
/// pins is that each row still has a producer.
#[test]
fn the_narrow_fixtures_carry_every_row_of_the_domain_table() {
    let uzumaki = proof_hspecs(&read_inf("spec_narrow_uzumaki.inf"));
    let abi = proof_hspecs(&read_inf("spec_narrow_abi.inf"));
    let unsigned: [(i32, &str); 4] = [
        (256, "`u8`"),
        (65536, "`u16`"),
        (2, "`bool`"),
        (3, "the three-variant enum `Color`"),
    ];
    for (hi_excl, what) in unsigned {
        assert!(
            some_obligation_states(&uzumaki, "NarrowUzu", &|a| is_below_u(a, hi_excl)),
            "spec_narrow_uzumaki.inf no longer bounds {what} above by {hi_excl}"
        );
    }
    for (lo_incl, hi_excl, what) in [(-128, 128, "`i8`"), (-32768, 32768, "`i16`")] {
        assert!(
            some_obligation_states(&uzumaki, "NarrowUzu", &|a| is_between_s(
                a, lo_incl, hi_excl
            )),
            "spec_narrow_uzumaki.inf no longer bounds {what} to {lo_incl}..{hi_excl}"
        );
    }
    assert!(
        some_obligation_states(&abi, "NarrowAbi", &|a| is_below_u(a, 256)),
        "spec_narrow_abi.inf no longer bounds a `u8` at the declaration boundary"
    );
    assert!(
        some_obligation_states(&abi, "NarrowAbi", &|a| is_between_s(a, -128, 128)),
        "spec_narrow_abi.inf no longer bounds an `i8` at the declaration boundary"
    );
}

/// A non-constant array index inside an `exists`/`unique` body fails the whole
/// proof-mode build (`P016`), and its constant-index counterpart does not.
///
/// The unit tests read the diagnostic list the translation pass returns; this
/// reads what a user gets. It pins the two halves the pass alone cannot show:
/// that the diagnostic is *fatal* rather than collected and dropped, and that
/// the accepted shape really does reach the translator through real code
/// generation — an acceptance the reachability pre-scan or body lowering
/// rejected first would prove nothing about `P016`'s scope.
#[test]
fn a_non_constant_index_in_a_reachability_body_fails_the_proof_build() {
    let program = |index: &str| {
        format!(
            "fn main() -> i32 {{ return 0; }}
             spec S {{
               fn f(i: i32) exists {{
                 let a: [i32; 2] = [1, 2];
                 let n: i32 = @;
                 assert(a[{index}] == n);
               }}
             }}"
        )
    };
    let error =
        codegen_with_target_mode_no_analysis(&program("i"), Target::Wasm32, CompilationMode::Proof)
            .expect_err("a non-constant index in an `exists` body must fail the proof-mode build");
    let rendered = format!("{error:#}");
    assert!(rendered.contains("error[P016]"), "{rendered}");
    assert!(
        rendered.contains("has no place in an `exists`-quantified spec function"),
        "{rendered}"
    );

    let accepted = proof_hspecs(&program("0"));
    assert_eq!(sole_obligation(&accepted, "S"), teq(i32c(1), local(1)));
}

/// An annotation is refused wherever in a payload body it is written, not only
/// where the term walk would meet it.
///
/// The positions below are the ones a term walk never reaches: an array index
/// and an aggregate-literal element are folded through on their way to a value,
/// a statement-position expression yields nothing at all, and a nested
/// quantifier block is translated by a different arm. All four accepted the
/// annotation silently before the body scan owned the rule, which is exactly the
/// failure mode a check on the term path cannot see.
#[test]
fn an_annotation_is_refused_in_every_position_of_a_payload_body() {
    let positions = [
        (
            "an array index",
            "let xs: [i32; 2] = [1, 2]; assert(xs[wrapping(0)] == 1);",
        ),
        (
            "an aggregate-literal element",
            "let xs: [i32; 2] = [checked(1 + 1), 2]; assert(xs[0] == 2);",
        ),
        ("statement position", "wrapping(a + b); assert(a == a);"),
        (
            "a nested `exists` block",
            "exists { assert(checked(a + b) == 0); }",
        ),
        (
            "a nested `forall` block",
            "forall { assert(wrapping(a + b) == 0); }",
        ),
    ];
    for (position, body) in positions {
        let source = format!(
            "fn main() -> i32 {{ return 0; }}
             spec S {{
               fn f(a: i32, b: i32) forall {{
                 {body}
               }}
             }}"
        );
        let error =
            codegen_with_target_mode_no_analysis(&source, Target::Wasm32, CompilationMode::Proof)
                .unwrap_err();
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("error[P017]"),
            "{position} must be refused: {rendered}"
        );
        assert!(
            rendered.contains("has no meaning inside a specification body"),
            "{position}: {rendered}"
        );
    }
}

/// A method declared on a struct inside a `spec` block is executable code, and
/// an arithmetic-mode annotation is legal in it.
///
/// The two shapes look alike in source and differ completely in what happens to
/// them. A helper `fn` in a `spec` block is inlined into the obligation term, so
/// its arithmetic is a term and an annotation over it means nothing; a method is
/// compiled to WebAssembly and called like any other, so its arithmetic is the
/// machine operator and the annotation decides which one. The rule keeps them
/// apart by translating only the specification *free* functions, which is a
/// property of one loop rather than a stated rule — this holds it to the
/// behaviour, so a future pass over specification methods cannot start refusing
/// an annotation that governs real emitted code.
#[test]
fn an_annotation_in_a_spec_method_is_accepted_and_guards_its_arithmetic() {
    let source = "pub fn main() -> i32 { return 0; }
         spec Geometry {
           struct Point {
             x: i32;
             y: i32;
             fn sum_coords(self) -> i32 { return checked(self.x + self.y); }
           }
           fn claim(n: i32) forall { assert(n == n); }
         }";
    let output =
        codegen_with_target_mode_no_analysis(source, Target::Wasm32, CompilationMode::Proof)
            .expect("an annotation in a specification method must compile");
    let guarded: Vec<String> = output
        .guarded_functions()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        guarded,
        vec!["Geometry.Point.sum_coords"],
        "the method's `+` must carry the guard the annotation asks for"
    );
}

/// A helper `fn` declared inside a `spec` block is a payload body too.
///
/// It is not executable code that happens to live in a specification: the pass
/// translates it into an obligation of its own, so its arithmetic is the same
/// wrapping term every other payload body's is and an annotation over it is as
/// meaningless there.
#[test]
fn an_annotation_in_a_spec_inner_helper_is_refused() {
    let source = "fn main() -> i32 { return 0; }
         spec S {
           fn helper(a: i32, b: i32) -> i32 { return checked(a + b); }
           fn claim(a: i32) forall { assert(a == a); }
         }";
    let error =
        codegen_with_target_mode_no_analysis(source, Target::Wasm32, CompilationMode::Proof)
            .unwrap_err();
    let rendered = format!("{error:#}");
    assert!(rendered.contains("error[P017]"), "{rendered}");
    assert!(
        rendered.contains("`checked(...)` has no meaning inside a specification body"),
        "{rendered}"
    );
}

/// The comparison-only shape: nothing inside the annotation is an operator it
/// could govern, and it is still refused.
///
/// This is the spelling that reaches the translator in term position with no
/// governed arithmetic at all, so nothing but the rule about the annotation
/// itself can reject it — the analysis rule that reports a decorative annotation
/// never runs on this path, because obligations are derived without it.
#[test]
fn an_annotation_over_a_bare_comparison_is_refused_in_a_payload_body() {
    let source = "fn main() -> i32 { return 0; }
         spec S {
           fn f(a: i32, b: i32) forall { assert(checked(a > b)); }
         }";
    let error =
        codegen_with_target_mode_no_analysis(source, Target::Wasm32, CompilationMode::Proof)
            .unwrap_err();
    let rendered = format!("{error:#}");
    assert!(rendered.contains("error[P017]"), "{rendered}");
}

/// An annotation in index position is refused rather than folded away.
///
/// The index fold looks through both grouping forms, so `a[wrapping(0)]` would
/// select the element `a[0]` selects and produce the identical obligation — the
/// annotation would vanish without a word. That silence is the reason the rule
/// is a body scan rather than a check on the term walk: this position never
/// reaches the term walk at all.
#[test]
fn a_constant_index_under_an_annotation_is_refused() {
    let program = |index: &str| {
        format!(
            "fn main() -> i32 {{ return 0; }}
             spec S {{
               fn f() forall {{
                 let a: [i32; 2] = [1, 2];
                 let n: i32 = @;
                 assert(a[{index}] == n);
               }}
             }}"
        )
    };
    // The unannotated program is accepted, so the rejection below is about the
    // annotation and not about the shape it sits in.
    let _ = sole_obligation(&proof_hspecs(&program("0")), "S");
    for annotated in ["wrapping(0)", "checked(0)", "(wrapping(0))"] {
        let error = codegen_with_target_mode_no_analysis(
            &program(annotated),
            Target::Wasm32,
            CompilationMode::Proof,
        )
        .unwrap_err();
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("error[P017]"),
            "`a[{annotated}]` must be refused: {rendered}"
        );
    }
}

/// An arithmetic-mode annotation inside an obligation term fails the proof
/// build, and the diagnostic quotes back the spelling the author wrote.
///
/// The reason is the same for both spellings: an obligation's arithmetic *is*
/// the machine operator, so an annotation over it would be dropped without a
/// word. The rejection is what turns a silently ignored annotation into
/// something the author is told about.
#[test]
fn an_arithmetic_mode_annotation_in_a_spec_body_fails_the_proof_build() {
    for quantifier in ["forall", ""] {
        for spelling in ["checked", "wrapping"] {
            let source = format!(
                "fn main() -> i32 {{ return 0; }}
                 spec S {{
                   fn f(a: i32, b: i32) {quantifier} {{
                     assert({spelling}(a + b) == 0);
                   }}
                 }}"
            );
            let error = codegen_with_target_mode_no_analysis(
                &source,
                Target::Wasm32,
                CompilationMode::Proof,
            )
            .expect_err("an arithmetic-mode annotation must fail the proof-mode build");
            let rendered = format!("{error:#}");
            assert!(rendered.contains("error[P017]"), "{rendered}");
            assert!(
                rendered.contains(&format!(
                    "`{spelling}(...)` has no meaning inside a specification body"
                )),
                "{rendered}"
            );
        }
    }
}

/// Borrows an owned multi-file program as the `(module path, source)` slices the
/// multi-file helpers take.
fn as_slices<'a>(files: &'a [(Vec<&'static str>, String)]) -> Vec<(Vec<&'static str>, &'a str)> {
    files
        .iter()
        .map(|(path, source)| (path.clone(), source.as_str()))
        .collect()
}

/// A reachability body around `body`, with a `lo` entry parameter and a `@`
/// choice, in a spec named so it does not collide with a Rocq stdlib name.
fn reach_program(quantifier: &str, body: &str) -> String {
    format!(
        "fn main() -> i32 {{ return 0; }}
         spec Claims {{
           fn f(lo: i32) {quantifier} {{
             let n: i32 = @;
             assume {{ assert(n >= lo); }}
             {body}
           }}
         }}"
    )
}

/// Effectively-checked arithmetic written in a reachability body is refused,
/// and `wrapping(...)` is what the diagnostic tells the author to write.
///
/// The operator is unmarked, which is the shape this rule exists for now that
/// the language's default traps; an explicit `checked(...)` is refused here too,
/// by the same scan over effective modes, and is named as the annotated operator
/// it is rather than as an unmarked one — an author shown "unmarked" about text
/// they marked is being told about a program they did not write.
///
/// The judgment reduces this body, so a trap in it empties the observation set
/// at every entry that reaches it and the theorem is false rather than narrowed
/// — the same failure `P016` describes for a dynamic index, one operator over.
#[test]
fn trapping_arithmetic_inline_in_a_reachability_body_is_refused() {
    for quantifier in ["exists", "unique"] {
        let article = if quantifier == "exists" { "n" } else { "" };
        let source = reach_program(quantifier, "assert(n + n >= lo);");
        let error =
            codegen_with_target_mode_no_analysis(&source, Target::Wasm32, CompilationMode::Proof)
                .unwrap_err();
        let rendered = format!("{error:#}");
        assert!(rendered.contains("error[P017]"), "{rendered}");
        assert!(
            rendered.contains(&format!(
                "a `+` at `i32` has no place unmarked in the body of a{article} \
                 `{quantifier}`-quantified spec function"
            )),
            "{rendered}"
        );
        assert!(
            rendered.contains("write `wrapping(n + n)` if this body's arithmetic is meant to be \
                               modular"),
            "the remedy must quote back the expression the author wrote: {rendered}"
        );

        let annotated = reach_program(quantifier, "assert(checked(n + n) >= lo);");
        let error = codegen_with_target_mode_no_analysis(
            &annotated,
            Target::Wasm32,
            CompilationMode::Proof,
        )
        .unwrap_err();
        let rendered = format!("{error:#}");
        assert!(rendered.contains("error[P017]"), "{rendered}");
        assert!(
            rendered.contains(&format!(
                "a `+` at `i32` inside a `checked(...)` has no place in the body of a{article} \
                 `{quantifier}`-quantified spec function"
            )),
            "{rendered}"
        );
    }
}

/// `wrapping(...)` is accepted throughout a reachability body — the final
/// `assert` included — and is dropped from the obligation.
///
/// Acceptance in the `assert` is what makes the annotation a remedy rather than
/// a second rejection: that arithmetic is both compiled into the reduced body
/// and translated into the payload term, so a rejection there would leave the
/// fix the diagnostic prescribes unspellable at the one place authors write it.
///
/// The term is spelled out rather than compared against the unmarked source,
/// which this rule now refuses: an obligation's `+` is the wrapping machine
/// operator whatever encloses it, and `T_binop ... BOI_add` over the two frame
/// slots is what that means. The cross-polarity form of the same equality — the
/// annotated body producing what the unmarked one produced before the default
/// moved — is pinned inside the code generation crate, which can compile one
/// source at either polarity.
#[test]
fn a_wrapping_annotation_is_accepted_and_inert_in_a_reachability_body() {
    let annotated = proof_hspecs(&reach_program("exists", "assert(wrapping(n + n) >= lo);"));
    let add = HTerm::Binop(
        HNumType::I32,
        HBinop::Add,
        Box::new(local(1)),
        Box::new(local(1)),
    );
    assert_eq!(
        sole_obligation(&annotated, "Claims"),
        and(
            nz(rel(HRelop::GeS, local(1), local(0))),
            nz(rel(HRelop::GeS, add, local(0))),
        ),
        "the annotation must be dropped and the operator translated as the machine's own"
    );
}

/// A nested `exists { }` block inside a `forall` function is a payload body, not
/// a reachability one.
///
/// Nothing reduces it, so its arithmetic is a term like the rest of the
/// function's and the annotation is meaningless rather than mandatory. The two
/// rules have to agree on this, and the way they agree is that both read the
/// *function's* quantifier: `P017`'s first wording fires here, and the second —
/// which would have demanded the very annotation the first refuses — does not.
#[test]
fn a_nested_exists_block_takes_the_payload_wording() {
    let source = "fn main() -> i32 { return 0; }
         spec Claims {
           fn f(a: i32, b: i32) forall {
             exists { assert(checked(a + b) == 0); }
           }
         }";
    let error =
        codegen_with_target_mode_no_analysis(source, Target::Wasm32, CompilationMode::Proof)
            .unwrap_err();
    let rendered = format!("{error:#}");
    assert!(rendered.contains("error[P017]"), "{rendered}");
    assert!(
        rendered.contains("has no meaning inside a specification body"),
        "the payload wording, not the reachability one: {rendered}"
    );
    assert!(
        !rendered.contains("has no place in the body of"),
        "the reachability wording must not fire on a nested block: {rendered}"
    );
}

/// A reachability body carrying no governed operator at all is accepted, and its
/// obligation is emitted.
///
/// The rule is stated against the effective mode rather than against the
/// spelling, so what it refuses is arithmetic that traps and not arithmetic. A
/// remainder cannot leave its type and no annotation reaches it, so this body
/// passes the scan without saying anything about `+`, `-` or `*` — which is
/// what makes it the case that separates "this rule looks at the governed
/// operators" from "this rule looks at bodies".
#[test]
fn ungoverned_arithmetic_in_a_reachability_body_is_accepted() {
    let map = proof_hspecs(&reach_program("exists", "assert(n % 7 >= lo);"));
    assert!(map.contains_key("Claims"), "the obligation must be emitted");
}

/// Neither annotation is touched in executable code, which is the whole point of
/// having them.
#[test]
fn both_annotations_are_accepted_in_every_executable_position() {
    let sources = [
        "pub fn f(a: i32, b: i32) -> i32 { return checked(a + b); }",
        "pub fn f(a: i32, b: i32) -> i32 { return wrapping(a * b); }",
        "struct P { x: i32; fn bump(self) -> i32 { return checked(self.x + 1); } }
         pub fn f(p: P) -> i32 { return p.bump(); }",
    ];
    for source in sources {
        for mode in [CompilationMode::Compile, CompilationMode::Proof] {
            codegen_with_target_mode_no_analysis(source, Target::Wasm32, mode)
                .unwrap_or_else(|e| panic!("{source:?} at {mode:?}: {e:#}"));
        }
    }
}

/// A reachability body that reaches an overflow guard through a call is refused,
/// at one hop and at two, from a statement call and from a term one.
///
/// The lexical rule sees the operators written in the body; this one sees the
/// ones the body reaches. The judgment reduces callee activations too, so the
/// two describe one hazard.
#[test]
fn a_reachability_body_reaching_a_guard_through_calls_is_refused() {
    let cases = [
        (
            "one hop, statement call",
            "pub fn step(a: i32) -> i32 { return checked(a + 1); }",
            "step(n);",
        ),
        (
            "one hop, term call",
            "pub fn step(a: i32) -> i32 { return checked(a + 1); }",
            "let y: i32 = step(n); assert(y >= lo);",
        ),
        (
            "two hops",
            "pub fn inner(a: i32) -> i32 { return checked(a + 1); }
             pub fn step(a: i32) -> i32 { return inner(a); }",
            "step(n);",
        ),
    ];
    for (case, callees, call) in cases {
        for quantifier in ["exists", "unique"] {
            let body = format!("{call} assert(n >= lo);");
            let source = format!("{callees} {}", reach_program(quantifier, &body));
            let error = codegen_with_target_mode_no_analysis(
                &source,
                Target::Wasm32,
                CompilationMode::Proof,
            )
            .unwrap_err();
            let rendered = format!("{error:#}");
            assert!(
                rendered.contains("error[P018]"),
                "{case} ({quantifier}): {rendered}"
            );
            assert!(
                rendered.contains("reaches arithmetic that traps on overflow, in `inner`")
                    || rendered.contains("reaches arithmetic that traps on overflow, in `step`"),
                "{case} ({quantifier}) must name the guarded function: {rendered}"
            );
        }
    }
}

/// An instance method is an ordinary hop, not a callee the rule declines to
/// follow.
///
/// The resolver the obligation *term* uses refuses a method outright, because a
/// method has no term encoding. Reusing that refusal as a visibility answer
/// would have rejected every reachability body that calls one; this rule uses
/// code generation's resolution instead, where a method is a normal call.
#[test]
fn a_reachability_body_reaching_a_guard_through_a_method_is_refused() {
    let source = format!(
        "struct Counter {{
           v: i32;
           fn step(self) -> i32 {{ return self.v + 1; }}
         }}
         {}",
        reach_program(
            "exists",
            "let c: Counter = Counter { v: n }; c.step(); assert(n >= lo);"
        )
    );
    let error =
        codegen_with_target_mode_no_analysis(&source, Target::Wasm32, CompilationMode::Proof)
            .unwrap_err();
    let rendered = format!("{error:#}");
    assert!(rendered.contains("error[P018]"), "{rendered}");
    assert!(
        rendered.contains("reaches arithmetic that traps on overflow, in `step`"),
        "{rendered}"
    );
}

/// The three acceptance shapes: a universal caller, a reachable set with no
/// guard, and a callee whose arithmetic is explicitly modular.
///
/// The universal case is deliberate rather than an oversight. A `forall` body is
/// not reduced by any judgment, so a guard in what it calls does not empty an
/// observation set — but its obligation can still be *false*, because
/// `assert(f(x) == e)` over a callee that traps at an admitted `x` claims
/// something the run does not deliver. Nothing here checks that; the standing
/// corpus invariant is what covers it, and only for this repository's fixtures.
#[test]
fn a_body_that_reaches_no_guard_is_accepted() {
    let guarded = "pub fn step(a: i32) -> i32 { return a + 1; }";
    let modular = "pub fn step(a: i32) -> i32 { return wrapping(a + 1); }";
    // Division is not an operator any annotation governs, so this callee has
    // arithmetic and still no guard — a stronger acceptance case than a body
    // with nothing in it, which would pass whatever the walk did.
    let ungoverned = "pub fn step(a: i32) -> i32 { return a / 2; }";

    // A universal caller reaching the guard.
    let universal = format!(
        "{guarded}
         fn main() -> i32 {{ return 0; }}
         spec Claims {{
           fn f(x: i32) forall {{ let y: i32 = step(x); assert(y >= x); }}
         }}"
    );
    codegen_with_target_mode_no_analysis(&universal, Target::Wasm32, CompilationMode::Proof)
        .expect("a `forall` body is not reduced, so a guard it reaches is not this rule's business");

    for callee in [modular, ungoverned] {
        let source = format!(
            "{callee} {}",
            reach_program("exists", "step(n); assert(n >= lo);")
        );
        codegen_with_target_mode_no_analysis(&source, Target::Wasm32, CompilationMode::Proof)
            .unwrap_or_else(|e| panic!("{callee:?}: {e:#}"));
    }
}

/// An `external fn` callee is skipped, not counted.
///
/// Code generation never receives a dependency's bytes — they arrive at link
/// time, after this pass has run — so nothing here can say whether a foreign
/// body traps, and answering either way would be a claim about bytes this
/// compiler has not seen.
#[test]
fn an_external_callee_is_skipped() {
    let source = format!(
        "external fn twice(a: i32) -> i32;
         use {{ twice }} from mathlib;
         {}",
        reach_program("exists", "twice(n); assert(n >= lo);")
    );
    codegen_with_target_mode_no_analysis(&source, Target::Wasm32, CompilationMode::Proof)
        .expect("an extern callee is skipped, not counted as trapping");
}

/// The walk resolves each hop in the *callee's* own scope, not in the scope the
/// specification started from.
///
/// Both files define `inner`, and only one of the two carries a guard. `outer`
/// lives in `lib` and calls `inner` by bare name, so the function that call
/// reaches is `lib`'s — which is the function code generation lowers it to. A
/// walk holding the specification's own file fixed across hops would read the
/// entry file's namesake instead and answer the opposite way in both polarities,
/// which is what the two directions below pin.
#[test]
fn each_hop_resolves_in_its_own_file() {
    let program = |lib_guarded: bool| {
        let (lib_inner, entry_inner) = if lib_guarded {
            ("a + 1", "wrapping(a + 1)")
        } else {
            ("wrapping(a + 1)", "a + 1")
        };
        vec![
            (
                vec!["lib"],
                format!(
                    "pub fn inner(a: i32) -> i32 {{ return {lib_inner}; }}
                     pub fn outer(a: i32) -> i32 {{ return inner(a); }}"
                ),
            ),
            (
                vec![],
                format!(
                    "use lib;
                     pub fn inner(a: i32) -> i32 {{ return {entry_inner}; }}
                     pub fn main() -> i32 {{ return 0; }}
                     spec Claims {{
                       fn f(lo: i32) exists {{
                         let n: i32 = @;
                         assume {{ assert(n >= lo); }}
                         lib::outer(n);
                         assert(n >= lo);
                       }}
                     }}"
                ),
            ),
        ]
    };

    let guarded = program(true);
    let error = try_proof_codegen_multi_file_no_analysis(&as_slices(&guarded)).unwrap_err();
    let rendered = format!("{error:#}");
    assert!(
        rendered.contains("error[P018]"),
        "hop two must resolve to `lib`'s guarded `inner`: {rendered}"
    );
    assert!(
        rendered.contains("reaches arithmetic that traps on overflow, in `inner`"),
        "{rendered}"
    );

    let unguarded = program(false);
    try_proof_codegen_multi_file_no_analysis(&as_slices(&unguarded)).expect(
        "hop two must resolve to `lib`'s unguarded `inner`, not the entry file's namesake",
    );
}

/// The same statement at hop zero: a bare call in the specification's own file
/// resolves there, and the other file's namesake does not decide the verdict.
#[test]
fn a_bare_call_resolves_in_the_specification_s_own_file() {
    let program = |entry_guarded: bool| {
        let (lib_step, entry_step) = if entry_guarded {
            ("wrapping(a + 1)", "a + 1")
        } else {
            ("a + 1", "wrapping(a + 1)")
        };
        vec![
            (
                vec!["lib"],
                format!("pub fn step(a: i32) -> i32 {{ return {lib_step}; }}"),
            ),
            (
                vec![],
                format!(
                    "pub fn step(a: i32) -> i32 {{ return {entry_step}; }}
                     pub fn main() -> i32 {{ return 0; }}
                     spec Claims {{
                       fn f(lo: i32) exists {{
                         let n: i32 = @;
                         assume {{ assert(n >= lo); }}
                         step(n);
                         assert(n >= lo);
                       }}
                     }}"
                ),
            ),
        ]
    };

    let guarded = program(true);
    let error = try_proof_codegen_multi_file_no_analysis(&as_slices(&guarded)).unwrap_err();
    assert!(
        format!("{error:#}").contains("error[P018]"),
        "{error:#}"
    );

    let unguarded = program(false);
    try_proof_codegen_multi_file_no_analysis(&as_slices(&unguarded))
        .expect("the other file's guarded namesake is not what this call reaches");
}

/// The same specification body builds in compile mode, where no obligation is
/// derived and the annotation is inert in the emitted instructions.
#[test]
fn an_arithmetic_mode_annotation_in_a_spec_body_still_compiles_in_compile_mode() {
    let source = "fn main() -> i32 { return 0; }
         spec S {
           fn f(a: i32, b: i32) forall {
             assert(wrapping(a + b) == 0);
           }
         }";
    let output =
        codegen_with_target_mode_no_analysis(source, Target::Wasm32, CompilationMode::Compile)
            .expect("compile mode derives no obligations, so `P017` cannot reject it");
    assert!(
        output.hspecs().is_empty(),
        "compile mode must derive no obligations at all"
    );
}

/// `P016` is a proof-mode diagnostic. Compile mode derives no obligations at
/// all, so the same program still builds — the rejection costs the deployed
/// artifact nothing.
#[test]
fn a_non_constant_index_in_a_reachability_body_still_compiles_in_compile_mode() {
    let source = "fn main() -> i32 { return 0; }
         spec S {
           fn f(i: i32) exists {
             let a: [i32; 2] = [1, 2];
             let n: i32 = @;
             assert(a[i] == n);
           }
         }";
    let output =
        codegen_with_target_mode_no_analysis(source, Target::Wasm32, CompilationMode::Compile)
            .expect("compile mode derives no obligations, so `P016` cannot reject it");
    assert!(
        output.hspecs().is_empty(),
        "compile mode must derive no obligations at all"
    );
}
