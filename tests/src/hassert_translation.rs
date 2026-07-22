//! End-to-end tests for proof-mode `hassert` obligation emission.
//!
//! These drive the *whole* compiler front end — parse, type-check, and generate
//! WASM in proof mode — and inspect the [`CodegenOutput::hspecs`] the code
//! generator now carries. They complement the in-crate unit tests of the
//! translation pass with the guarantees only the full pipeline can make: that
//! the obligation survives real code generation, that compile mode carries none,
//! and that the corpus of existing spec fixtures still translates cleanly.

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
fn teq(a: HTerm, b: HTerm) -> HAssert {
    HAssert::TermEq(a, b)
}
fn nz(t: HTerm) -> HAssert {
    not(teq(t, i32c(0)))
}

/// Building the PrimeExample source through the whole front end must yield an
/// obligation structurally equal to wasm-verifier's `prime_hspec1`
/// (theories/examples/PrimeExample.v:147-164).
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
        nz(gts(n(), one())),
        and(
            imp(
                nz(is_prime()),
                imp(
                    and(nz(gts(m_then(), one())), nz(lts(m_then(), n()))),
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
/// `Himpl`-free `nz(relop_eq(app, local))` shape.
#[test]
fn with_spec_fixture_produces_the_expected_app_equality() {
    let map = proof_hspecs(&read_inf("with_spec.inf"));
    assert_eq!(
        sole_obligation(&map, "MySpec"),
        nz(rel(HRelop::Eq, app("foo", vec![local(0)]), local(0)))
    );
}

/// A plain (`Regular`) spec free function is kept as a trivially-true obligation
/// so the fixture still translates and counts one obligation.
#[test]
fn spec_calls_top_fixture_emits_a_true_obligation() {
    let map = proof_hspecs(&read_inf("spec_calls_top.inf"));
    assert_eq!(sole_obligation(&map, "Caller"), HAssert::True);
}

/// Three specs of mixed shapes: only `Alpha` has a free function (a trivially
/// true obligation); `Beta` (a struct method) and `Gamma` (empty) carry no
/// obligation and so do not appear in the map.
#[test]
fn three_specs_fixture_only_maps_the_free_function_spec() {
    let map = proof_hspecs(&read_inf("three_specs.inf"));
    assert_eq!(sole_obligation(&map, "Alpha"), HAssert::True);
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
