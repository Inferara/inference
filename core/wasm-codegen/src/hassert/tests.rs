//! Unit tests for the specification-to-`hassert` translation.
//!
//! Each test parses inline source, type-checks it, collects the emittable
//! buckets in proof mode, and runs [`translate_spec_fns`](super::translate_spec_fns)
//! directly — deliberately *without* compiling the bodies to WASM. Driving the
//! pass in isolation is what lets a case that would otherwise abort code
//! generation first (a `**` that is `todo!()` in the lowerer, a construct with no
//! encoding) still be exercised for its diagnostic, and keeps every structural
//! assertion about the produced obligation independent of byte emission.
#![allow(clippy::similar_names)] // the expected-tree builders use short, related names

use inference_ast::arena::AstArena;
use inference_hassert::{HAssert, HBinop, HConst, HFnRef, HNumType, HRelop, HSpecMap, HTerm};
use inference_type_checker::TypeCheckerBuilder;
use inference_type_checker::typed_context::TypedContext;

use crate::target::CompilationMode;
use crate::{EmittableFunctions, collect_emittable_functions};

// ----- harness ----------------------------------------------------------

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

fn type_check_multi(files: &[(Vec<&str>, &str)]) -> TypedContext {
    let mut arena = AstArena::default();
    for (module_path, source) in files {
        let module_path: Vec<String> = module_path.iter().map(|s| (*s).to_string()).collect();
        let parsed = inference_parser::parse_into(arena, source, module_path);
        assert!(
            parsed.errors.is_empty(),
            "parse errors: {:?}",
            parsed.errors
        );
        arena = parsed.arena;
    }
    TypeCheckerBuilder::build_typed_context(arena)
        .expect("multi-file type checking should succeed")
        .typed_context()
}

fn buckets_of(ctx: &TypedContext) -> EmittableFunctions {
    let mut buckets = EmittableFunctions::default();
    for source_file in ctx.source_files() {
        collect_emittable_functions(
            ctx.arena(),
            &source_file.defs,
            &source_file.module_path,
            CompilationMode::Proof,
            &mut buckets,
        )
        .expect("collecting emittable functions should succeed");
    }
    buckets
}

/// Translates a type-checked program, returning its obligations and the rendered
/// diagnostics.
fn translate(ctx: &TypedContext) -> (HSpecMap, Vec<String>) {
    let buckets = buckets_of(ctx);
    let (map, diagnostics) = super::translate_spec_fns(ctx, &buckets);
    (map, diagnostics.iter().map(ToString::to_string).collect())
}

/// Translates single-file source expected to raise no diagnostics.
fn ok(source: &str) -> HSpecMap {
    let ctx = type_check(source);
    let (map, diagnostics) = translate(&ctx);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
    map
}

/// Translates single-file source expected to raise diagnostics, returning them
/// joined into one string.
fn err(source: &str) -> String {
    let ctx = type_check(source);
    let (_, diagnostics) = translate(&ctx);
    assert!(!diagnostics.is_empty(), "expected diagnostics but got none");
    diagnostics.join("\n")
}

/// The single obligation of a spec that has exactly one.
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

/// The obligation of the spec free function whose symbol is `symbol`.
fn obligation_named(map: &HSpecMap, spec: &str, symbol: &str) -> HAssert {
    let entries = map.get(spec).unwrap_or_else(|| panic!("no spec `{spec}`"));
    entries
        .iter()
        .find(|e| e.fn_symbol == HFnRef(symbol.to_string()))
        .unwrap_or_else(|| panic!("no obligation `{symbol}` in spec `{spec}`"))
        .hassert
        .clone()
}

/// Translates one spec free function whose body is `body`, wrapped in the
/// boilerplate `spec S { fn f() <body> }`, and returns its obligation. `prelude`
/// is emitted ahead of the spec (helper functions, enums, …).
fn obligation_of(prelude: &str, body: &str) -> HAssert {
    let source = format!("{prelude}\nspec S {{\n  fn f() {body}\n}}\n");
    sole_obligation(&ok(&source), "S")
}

// ----- expected-tree builders (primitive, independent of the pass) ------

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
fn bin(ty: HNumType, op: HBinop, l: HTerm, r: HTerm) -> HTerm {
    HTerm::Binop(ty, op, Box::new(l), Box::new(r))
}
fn rel(ty: HNumType, op: HRelop, l: HTerm, r: HTerm) -> HTerm {
    HTerm::Relop(ty, op, Box::new(l), Box::new(r))
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
fn or(a: HAssert, b: HAssert) -> HAssert {
    HAssert::Or(Box::new(a), Box::new(b))
}
fn ex(a: HAssert) -> HAssert {
    HAssert::Ex(Box::new(a))
}
fn teq(a: HTerm, b: HTerm) -> HAssert {
    HAssert::TermEq(a, b)
}
fn defined(t: HTerm) -> HAssert {
    HAssert::Defined(t)
}
fn nz(t: HTerm) -> HAssert {
    not(teq(t, i32c(0)))
}
fn eqz(t: HTerm) -> HAssert {
    teq(t, i32c(0))
}

// convenience relop/binop shorthands at i32/signed (the common width)
fn gts(l: HTerm, r: HTerm) -> HTerm {
    rel(HNumType::I32, HRelop::GtS, l, r)
}
fn lts(l: HTerm, r: HTerm) -> HTerm {
    rel(HNumType::I32, HRelop::LtS, l, r)
}
fn rems(l: HTerm, r: HTerm) -> HTerm {
    bin(HNumType::I32, HBinop::RemS, l, r)
}

// ----- 1. the canonical prime obligation --------------------------------

/// The single most important test: the `PrimeExample` source spec must produce
/// `prime_hspec1` (wasm-verifier's `theories/examples/PrimeExample.v`) node-for-node.
#[test]
fn canonical_prime_spec_matches_prime_hspec1() {
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
      let m: i32 = @;
      assume { assert(m > 1 && m < n); }
      assert(n % m == 0);
    }
  }
}
";
    let map = ok(source);
    let obligation = obligation_named(&map, "prime_properties", "prime_properties.prime_spec");

    let n = || local(0);
    let m_then = || local(1);
    let m_ex = || lvar(0);
    let one = || i32c(1);
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
    assert_eq!(obligation, expected);
}

// ----- 2. op / atom coverage --------------------------------------------

/// `assert(<lhs op rhs> == <cmp>)` in universal mode is `nz(relop Eq lhs_term
/// cmp_term)`; this returns `lhs_term` for a two-slot body so each operator's
/// width/signedness/narrowing can be pinned in isolation.
fn lhs_term_of_binary(decl_ty: &str, op: &str) -> HTerm {
    // Parenthesize the operation so it is not re-associated against `==`; `term`
    // unwraps the parentheses, so the extracted term is the bare operation.
    let body = format!(
        "forall {{ let a: {decl_ty} = @; let b: {decl_ty} = @; assert((a {op} b) == a); }}"
    );
    let obligation = obligation_of("", &body);
    // `nz(relop Eq lhs rhs)`; the outer relop width is the operand width (I64 for
    // 64-bit operands), which is irrelevant here — only `lhs` is under test.
    match obligation {
        HAssert::Not(inner) => match *inner {
            HAssert::TermEq(HTerm::Relop(_, HRelop::Eq, lhs, _), _) => *lhs,
            other => panic!("expected nz(relop Eq ..), got {other:?}"),
        },
        other => panic!("expected nz(..), got {other:?}"),
    }
}

#[test]
fn binops_pick_width_and_signedness_from_left_operand() {
    assert_eq!(
        lhs_term_of_binary("i32", "/"),
        bin(HNumType::I32, HBinop::DivS, local(0), local(1))
    );
    assert_eq!(
        lhs_term_of_binary("u32", "/"),
        bin(HNumType::I32, HBinop::DivU, local(0), local(1))
    );
    assert_eq!(
        lhs_term_of_binary("i64", "/"),
        bin(HNumType::I64, HBinop::DivS, local(0), local(1))
    );
    assert_eq!(
        lhs_term_of_binary("u64", "/"),
        bin(HNumType::I64, HBinop::DivU, local(0), local(1))
    );
    // Remainder and shift take signedness but are not narrowed at any width.
    assert_eq!(
        lhs_term_of_binary("u32", "%"),
        bin(HNumType::I32, HBinop::RemU, local(0), local(1))
    );
    assert_eq!(
        lhs_term_of_binary("u64", ">>"),
        bin(HNumType::I64, HBinop::ShrU, local(0), local(1))
    );
    assert_eq!(
        lhs_term_of_binary("i64", ">>"),
        bin(HNumType::I64, HBinop::ShrS, local(0), local(1))
    );
    // Bitwise operators.
    assert_eq!(
        lhs_term_of_binary("i32", "^"),
        bin(HNumType::I32, HBinop::Xor, local(0), local(1))
    );
    assert_eq!(
        lhs_term_of_binary("i32", "<<"),
        bin(HNumType::I32, HBinop::Shl, local(0), local(1))
    );
}

#[test]
fn sub_word_arithmetic_is_narrowed_but_mod_is_not() {
    // i8 add: sign-extend via (x << 24) >>s 24.
    let expected_i8 = bin(
        HNumType::I32,
        HBinop::ShrS,
        bin(
            HNumType::I32,
            HBinop::Shl,
            bin(HNumType::I32, HBinop::Add, local(0), local(1)),
            i32c(24),
        ),
        i32c(24),
    );
    assert_eq!(lhs_term_of_binary("i8", "+"), expected_i8);

    // u8 add: zero-extend via x & 0xFF.
    let expected_u8 = bin(
        HNumType::I32,
        HBinop::And,
        bin(HNumType::I32, HBinop::Add, local(0), local(1)),
        i32c(0xFF),
    );
    assert_eq!(lhs_term_of_binary("u8", "+"), expected_u8);

    // u16 add: x & 0xFFFF.
    let expected_u16 = bin(
        HNumType::I32,
        HBinop::And,
        bin(HNumType::I32, HBinop::Add, local(0), local(1)),
        i32c(0xFFFF),
    );
    assert_eq!(lhs_term_of_binary("u16", "+"), expected_u16);

    // %, at a sub-word width, is not narrowed.
    assert_eq!(
        lhs_term_of_binary("u8", "%"),
        bin(HNumType::I32, HBinop::RemU, local(0), local(1))
    );
}

#[test]
fn relational_operators_carry_width_and_signedness() {
    // `a < b` for u32 is a GtU-family LtU comparison at i32 width.
    let body = "forall { let a: u32 = @; let b: u32 = @; assert(a < b); }";
    assert_eq!(
        obligation_of("", body),
        nz(rel(HNumType::I32, HRelop::LtU, local(0), local(1)))
    );
    let body = "forall { let a: i64 = @; let b: i64 = @; assert(a >= b); }";
    assert_eq!(
        obligation_of("", body),
        nz(rel(HNumType::I64, HRelop::GeS, local(0), local(1)))
    );
}

#[test]
fn unary_operators_mirror_codegen() {
    // Negation: 0 - x (i32, no narrowing at i32).
    let body = "forall { let a: i32 = @; assert(-a == a); }";
    assert_eq!(
        obligation_of("", body),
        nz(rel(
            HNumType::I32,
            HRelop::Eq,
            bin(HNumType::I32, HBinop::Sub, i32c(0), local(0)),
            local(0),
        ))
    );
    // Bitwise not: x ^ -1 (i32).
    let body = "forall { let a: i32 = @; assert(~a == a); }";
    assert_eq!(
        obligation_of("", body),
        nz(rel(
            HNumType::I32,
            HRelop::Eq,
            bin(HNumType::I32, HBinop::Xor, local(0), i32c(-1)),
            local(0),
        ))
    );
    // Term-position `!x` is the i32.eqz form (relop Eq x 0).
    let body = "forall { let a: bool = @; assert(!a == a); }";
    assert_eq!(
        obligation_of("", body),
        nz(rel(
            HNumType::I32,
            HRelop::Eq,
            rel(HNumType::I32, HRelop::Eq, local(0), i32c(0)),
            local(0),
        ))
    );
}

#[test]
fn literals_parse_per_width_including_cast_signed() {
    // A typed `let` fixes the literal's width; the pure-let inlines its constant,
    // so the comparison term is the parsed constant. u32 max casts to signed -1.
    let body = "forall { let m: u32 = 4294967295; let a: u32 = @; assert(a == m); }";
    assert_eq!(
        obligation_of("", body),
        nz(rel(HNumType::I32, HRelop::Eq, local(0), i32c(-1)))
    );
    // i64 literal.
    let body = "forall { let m: i64 = 5; let a: i64 = @; assert(a == m); }";
    assert_eq!(
        obligation_of("", body),
        nz(rel(HNumType::I64, HRelop::Eq, local(0), i64c(5)))
    );
    // u64 max casts to signed -1.
    let body = "forall { let m: u64 = 18446744073709551615; let a: u64 = @; assert(a == m); }";
    assert_eq!(
        obligation_of("", body),
        nz(rel(HNumType::I64, HRelop::Eq, local(0), i64c(-1)))
    );
    // bool literal.
    let body = "forall { let m: bool = true; let a: bool = @; assert(a == m); }";
    assert_eq!(
        obligation_of("", body),
        nz(rel(HNumType::I32, HRelop::Eq, local(0), i32c(1)))
    );
}

#[test]
fn enum_variant_lowers_to_its_tag_constant() {
    let body = "forall { let c: Color = @; assert(c == Color::Blue); }";
    let obligation = obligation_of("enum Color { Red, Green, Blue }", body);
    assert_eq!(
        obligation,
        nz(rel(HNumType::I32, HRelop::Eq, local(0), i32c(2)))
    );
}

// ----- 3. boolean structure ---------------------------------------------

#[test]
fn conjunction_splits_and_disjunction_is_or() {
    let body = "forall { let a: i32 = @; let b: i32 = @; assert(a > 0 && b > 0); }";
    assert_eq!(
        obligation_of("", body),
        and(nz(gts(local(0), i32c(0))), nz(gts(local(1), i32c(0))))
    );
    let body = "forall { let a: i32 = @; let b: i32 = @; assert(a > 0 || b > 0); }";
    assert_eq!(
        obligation_of("", body),
        or(nz(gts(local(0), i32c(0))), nz(gts(local(1), i32c(0))))
    );
}

#[test]
fn negation_of_a_comparison_is_the_zero_equality() {
    let body = "forall { let a: i32 = @; assert(!(a > 0)); }";
    assert_eq!(obligation_of("", body), eqz(gts(local(0), i32c(0))));
}

#[test]
fn equality_is_non_strict_universally_and_strict_existentially() {
    // Universal `==` is nz(relop Eq ..).
    let body = "forall { let a: i32 = @; let b: i32 = @; assert(a == b); }";
    assert_eq!(
        obligation_of("", body),
        nz(rel(HNumType::I32, HRelop::Eq, local(0), local(1)))
    );
    // Existential `==` is strict term_eq.
    let body = "forall { let n: i32 = @; exists { let m: i32 = @; assert(m == n); } }";
    assert_eq!(obligation_of("", body), ex(teq(lvar(0), local(0))));
}

#[test]
fn disequality_conjoins_defined_only_for_app_bearing_sides() {
    // No app on either side: no HA_defined.
    let body = "forall { let a: i32 = @; let b: i32 = @; assert(a != b); }";
    assert_eq!(
        obligation_of("", body),
        nz(rel(HNumType::I32, HRelop::Ne, local(0), local(1)))
    );
    // Left side bears a T_app: conjoin HA_defined for it.
    let prelude = "fn g(x: i32) -> i32 { return x; }";
    let body = "forall { let a: i32 = @; assert(g(a) != a); }";
    let call = || app("g", vec![local(0)]);
    assert_eq!(
        obligation_of(prelude, body),
        and(
            nz(rel(HNumType::I32, HRelop::Ne, call(), local(0))),
            defined(call()),
        )
    );
}

// ----- 4. bindings, slots, scoping --------------------------------------

#[test]
fn pure_let_is_inlined_as_a_term() {
    let body = "forall { let a: i32 = @; let s: i32 = a + 1; assert(s > 0); }";
    assert_eq!(
        obligation_of("", body),
        nz(gts(
            bin(HNumType::I32, HBinop::Add, local(0), i32c(1)),
            i32c(0)
        ))
    );
}

#[test]
fn let_of_call_inlines_the_application() {
    let prelude = "fn g(x: i32) -> i32 { return x; }";
    let body = "forall { let a: i32 = @; let c: i32 = g(a); assert(c > 0); }";
    assert_eq!(
        obligation_of(prelude, body),
        nz(gts(app("g", vec![local(0)]), i32c(0)))
    );
}

#[test]
fn params_take_slots_and_interleaved_pure_lets_do_not() {
    let source = "\
spec S {
  fn f(p: i32) forall {
    let a: i32 = @;
    let t: i32 = p + a;
    let b: i32 = @;
    assert(b > t);
  }
}
";
    // p = slot 0, a = slot 1, t = inlined term, b = slot 2.
    assert_eq!(
        sole_obligation(&ok(source), "S"),
        nz(gts(
            local(2),
            bin(HNumType::I32, HBinop::Add, local(0), local(1))
        ))
    );
}

#[test]
fn branch_local_binding_does_not_leak_to_the_enclosing_block() {
    let source = "\
spec S {
  fn f() forall {
    let a: i32 = @;
    if a > 0 {
      let b: i32 = @;
      assert(b > 5);
    }
    assert(a > 3);
  }
}
";
    // `b` is a branch-local slot 1; the outer `a` (slot 0) is still bound for the
    // trailing assert after the branch scope is restored.
    let expected = and(
        imp(nz(gts(local(0), i32c(0))), nz(gts(local(1), i32c(5)))),
        nz(gts(local(0), i32c(3))),
    );
    assert_eq!(sole_obligation(&ok(source), "S"), expected);
}

#[test]
fn block_local_const_is_inlined() {
    let body = "forall { let a: i32 = @; const k: i32 = 3; assert(a > k); }";
    assert_eq!(obligation_of("", body), nz(gts(local(0), i32c(3))));
}

// ----- 5. existentials / de Bruijn --------------------------------------

#[test]
fn single_existential_binds_lvar_zero() {
    let body = "forall { let n: i32 = @; exists { let m: i32 = @; assert(m > n); } }";
    assert_eq!(obligation_of("", body), ex(nz(gts(lvar(0), local(0)))));
}

#[test]
fn two_existentials_with_an_assert_between_index_correctly() {
    let body =
        "forall { exists { let a: i32 = @; assert(a > 0); let b: i32 = @; assert(b > a); } }";
    // Outer binder = a (index 1 at the inner use), inner = b (index 0).
    let expected = ex(and(
        nz(gts(lvar(0), i32c(0))),
        ex(nz(gts(lvar(0), lvar(1)))),
    ));
    assert_eq!(obligation_of("", body), expected);
}

#[test]
fn pure_let_capturing_an_existential_reindexes_at_a_deeper_use() {
    let body =
        "forall { exists { let a: i32 = @; let t: i32 = a + 1; let b: i32 = @; assert(b > t); } }";
    // b is the inner binder (index 0); a, captured by `t`, is index 1 at the
    // deeper use — the level-based reindex the finalize pass performs.
    let expected = ex(ex(nz(gts(
        lvar(0),
        bin(HNumType::I32, HBinop::Add, lvar(1), i32c(1)),
    ))));
    assert_eq!(obligation_of("", body), expected);
}

#[test]
fn uzumaki_as_a_call_argument_in_exists_binds_a_fresh_lvar() {
    let prelude = "fn g(x: i32) -> i32 { return x; }";
    let body = "forall { exists { assert(g(@) > 0); } }";
    assert_eq!(
        obligation_of(prelude, body),
        ex(nz(gts(app("g", vec![lvar(0)]), i32c(0))))
    );
}

#[test]
fn assume_inside_exists_is_a_conjunct() {
    let body = "forall { exists { let m: i32 = @; assume { assert(m > 0); } assert(m < 10); } }";
    assert_eq!(
        obligation_of("", body),
        ex(and(nz(gts(lvar(0), i32c(0))), nz(lts(lvar(0), i32c(10)))))
    );
}

// ----- 6. if forms -------------------------------------------------------

#[test]
fn universal_if_without_else_is_a_single_guarded_implication() {
    let body = "forall { let n: i32 = @; if n > 0 { assert(n > 1); } }";
    assert_eq!(
        obligation_of("", body),
        imp(nz(gts(local(0), i32c(0))), nz(gts(local(0), i32c(1))))
    );
}

#[test]
fn universal_if_else_is_a_guard_pair() {
    let body = "forall { let n: i32 = @; if n > 0 { assert(n > 1); } else { assert(n < 0); } }";
    let cond = || gts(local(0), i32c(0));
    let expected = and(
        imp(nz(cond()), nz(gts(local(0), i32c(1)))),
        imp(eqz(cond()), nz(lts(local(0), i32c(0)))),
    );
    assert_eq!(obligation_of("", body), expected);
}

#[test]
fn existential_if_is_a_strict_disjunction() {
    let body = "forall { exists { let n: i32 = @; if n > 0 { assert(n > 1); } } }";
    let cond = || gts(lvar(0), i32c(0));
    let expected = ex(or(and(nz(cond()), nz(gts(lvar(0), i32c(1)))), eqz(cond())));
    assert_eq!(obligation_of("", body), expected);
}

#[test]
fn if_condition_may_be_a_call_result() {
    let prelude = "fn is_even(x: i32) -> bool { return x > 0; }";
    let body = "forall { let n: i32 = @; if is_even(n) { assert(n > 0); } }";
    assert_eq!(
        obligation_of(prelude, body),
        imp(
            nz(app("is_even", vec![local(0)])),
            nz(gts(local(0), i32c(0)))
        )
    );
}

// ----- 7. calls ----------------------------------------------------------

#[test]
fn spec_sibling_helper_is_a_t_app_by_its_folded_key() {
    let source = "\
spec S {
  fn helper() -> i32 {
    return 3;
  }
  fn prop() forall {
    let a: i32 = @;
    assert(helper() == a);
  }
}
";
    let map = ok(source);
    // The helper itself is a plain (Regular) spec fn, so it contributes a
    // trivially-true obligation and keeps its place in source order.
    assert_eq!(obligation_named(&map, "S", "S.helper"), HAssert::True);
    assert_eq!(
        obligation_named(&map, "S", "S.prop"),
        nz(rel(
            HNumType::I32,
            HRelop::Eq,
            app("S.helper", vec![]),
            local(0)
        ))
    );
}

#[test]
fn cross_file_qualified_callee_carries_its_defining_path() {
    let ctx = type_check_multi(&[
        (
            vec![],
            "use lib::{add};\nspec S {\n  fn f() forall {\n    let a: i32 = @;\n    assert(add(a, a) == a);\n  }\n}\n",
        ),
        (
            vec!["lib"],
            "pub fn add(x: i32, y: i32) -> i32 {\n  return x + y;\n}\n",
        ),
    ]);
    let (map, diagnostics) = translate(&ctx);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        sole_obligation(&map, "S"),
        nz(rel(
            HNumType::I32,
            HRelop::Eq,
            app("lib.add", vec![local(0), local(0)]),
            local(0),
        ))
    );
}

// ----- 8. simplification -------------------------------------------------

#[test]
fn assert_free_and_empty_bodies_are_trivially_true() {
    assert_eq!(
        sole_obligation(&ok("spec S { fn f() forall { } }"), "S"),
        HAssert::True
    );
    assert_eq!(
        sole_obligation(&ok("spec S { fn f() forall { let a: i32 = @; } }"), "S"),
        HAssert::True
    );
    // A plain (Regular) spec free function is also a trivially-true obligation.
    assert_eq!(
        sole_obligation(&ok("spec S { fn f() -> i32 { return 0; } }"), "S"),
        HAssert::True
    );
}

#[test]
fn assume_then_assert_has_no_trailing_conjunction_with_true() {
    let body = "forall { let n: i32 = @; assume { assert(n > 0); } assert(n > 1); }";
    // imp(nz(n>0), nz(n>1)) — the fold leaves no `∧ ⊤`.
    assert_eq!(
        obligation_of("", body),
        imp(nz(gts(local(0), i32c(0))), nz(gts(local(0), i32c(1))))
    );
}

// ----- 9. diagnostics ----------------------------------------------------

#[test]
fn p001_rejects_a_quantified_spec_function() {
    let e = err("spec S { fn f() exists { let x: i32 = @; assert(x > 0); } } ");
    assert!(e.contains("error[P001]"), "{e}");
}

#[test]
fn p002_rejects_constructs_without_an_encoding() {
    let loop_src = "spec S { fn f() forall { let n: i32 = @; loop { assert(n > 0); } } }";
    assert!(err(loop_src).contains("error[P002]"));
    let unique_src = "spec S { fn f() forall { let n: i32 = @; unique { assert(n > 0); } } }";
    assert!(err(unique_src).contains("error[P002]"));
}

#[test]
fn p003_rejects_reassignment() {
    let src = "spec S { fn f() forall { let a: i32 = @; let mut b: i32 = a; b = b + 1; assert(b > 0); } }";
    assert!(err(src).contains("error[P003]"));
}

#[test]
fn p004_rejects_a_compound_parameter() {
    let src = "spec S { fn f(arr: [i32; 3]) forall { let n: i32 = @; assert(n > 0); } }";
    assert!(err(src).contains("error[P004]"));
}

#[test]
fn p005_rejects_extern_and_nondeterministic_callees() {
    let extern_src = "\
external fn ext(x: i32) -> i32;
spec S { fn f() forall { let a: i32 = @; assert(ext(a) == a); } }
";
    assert!(err(extern_src).contains("error[P005]"));

    let nondet_src = "\
fn helper() -> i32 {
  let x: i32 = @;
  return x;
}
spec S { fn f() forall { let a: i32 = @; assert(helper() == a); } }
";
    assert!(err(nondet_src).contains("error[P005]"));
}

#[test]
fn p007_rejects_a_forall_block_inside_an_exists_block() {
    let src = "spec S { fn f() forall { let n: i32 = @; exists { forall { assert(n > 0); } } } }";
    assert!(err(src).contains("error[P007]"));
}

#[test]
fn p008_rejects_a_compound_uzumaki() {
    let src = "spec S { fn f() forall { let arr: [i32; 3] = @; let n: i32 = @; assert(n > 0); } }";
    assert!(err(src).contains("error[P008]"));
}

#[test]
fn p009_rejects_a_quantified_spec_method() {
    let src = "\
spec S {
  struct T {
    x: i32;
    fn m(self) forall {
      let y: i32 = @;
      assert(y > 0);
    }
  }
}
";
    assert!(err(src).contains("error[P009]"));
}

#[test]
fn every_diagnostic_is_collected_before_failing() {
    // A compound parameter (P004) and a loop (P002) in the same body.
    let src = "spec S { fn f(arr: [i32; 2]) forall { let n: i32 = @; loop { } } }";
    let e = err(src);
    assert!(e.contains("error[P004]"), "{e}");
    assert!(e.contains("error[P002]"), "{e}");
}
