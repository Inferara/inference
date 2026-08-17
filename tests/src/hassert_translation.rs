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

use crate::utils::{codegen_output, codegen_with_target_mode_no_analysis, get_test_data_path};

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
            not(teq(rel64(HRelop::GtS, n(), i64c(4_294_967_296)), i32c(0))),
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
    assert(g(@) == x + n);
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
/// binder — the fixture is the gate's only producer of one, so a body that
/// stopped emitting it would leave that stub declaration unelaborated.
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
