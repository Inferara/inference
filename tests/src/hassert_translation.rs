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
