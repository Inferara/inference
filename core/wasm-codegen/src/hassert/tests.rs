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
fn hastype(t: HTerm, ty: HNumType) -> HAssert {
    HAssert::HasType(t, ty)
}
fn nz(t: HTerm) -> HAssert {
    not(teq(t, i32c(0)))
}
fn eqz(t: HTerm) -> HAssert {
    teq(t, i32c(0))
}

/// The typing guard universal slot `n` carries at the common i32 width — the
/// antecedent every positive-position read of that slot sits under.
fn guard(n: u32) -> HAssert {
    hastype(local(n), HNumType::I32)
}

/// The class a slot declared at `decl_ty` is guarded at.
fn guard_width(decl_ty: &str) -> HNumType {
    if matches!(decl_ty, "i64" | "u64") {
        HNumType::I64
    } else {
        HNumType::I32
    }
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
/// `prime_hspec1` (wasm-verifier's `theories/examples/PrimeExample.v`)
/// node-for-node — including the two argument-typing guards fused into the
/// antecedents the source's `assume`s already build, which is what makes the
/// obligation dischargeable under the verifier's strictified `ValidSpec`.
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
        and(guard(0), nz(gts(n(), one()))),
        and(
            imp(
                nz(is_prime()),
                imp(
                    and(
                        guard(1),
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
    assert_eq!(obligation, expected);
}

// ----- 2. op / atom coverage --------------------------------------------

/// `assert(<lhs op rhs> == <cmp>)` in universal mode is `nz(relop Eq lhs_term
/// cmp_term)` under both slots' typing guards; this returns `lhs_term` for a
/// two-slot body so each operator's width/signedness/narrowing can be pinned in
/// isolation.
fn lhs_term_of_binary(decl_ty: &str, op: &str) -> HTerm {
    // Parenthesize the operation so it is not re-associated against `==`; `term`
    // unwraps the parentheses, so the extracted term is the bare operation.
    let body = format!(
        "forall {{ let a: {decl_ty} = @; let b: {decl_ty} = @; assert((a {op} b) == a); }}"
    );
    let obligation = obligation_of("", &body);
    let HAssert::Imp(antecedent, claim) = obligation else {
        panic!("expected a guarded implication, got {obligation:?}");
    };
    let width = guard_width(decl_ty);
    assert_eq!(
        *antecedent,
        and(hastype(local(0), width), hastype(local(1), width)),
        "both universal slots must be guarded at their declared width"
    );
    // `nz(relop Eq lhs rhs)`; the outer relop width is the operand width (I64 for
    // 64-bit operands), which is irrelevant here — only `lhs` is under test.
    match *claim {
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
        imp(
            and(guard(0), guard(1)),
            nz(rel(HNumType::I32, HRelop::LtU, local(0), local(1)))
        )
    );
    let body = "forall { let a: i64 = @; let b: i64 = @; assert(a >= b); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            and(
                hastype(local(0), HNumType::I64),
                hastype(local(1), HNumType::I64)
            ),
            nz(rel(HNumType::I64, HRelop::GeS, local(0), local(1)))
        )
    );
}

#[test]
fn unary_operators_mirror_codegen() {
    // Negation: 0 - x (i32, no narrowing at i32).
    let body = "forall { let a: i32 = @; assert(-a == a); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            guard(0),
            nz(rel(
                HNumType::I32,
                HRelop::Eq,
                bin(HNumType::I32, HBinop::Sub, i32c(0), local(0)),
                local(0),
            ))
        )
    );
    // Bitwise not: x ^ -1 (i32).
    let body = "forall { let a: i32 = @; assert(~a == a); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            guard(0),
            nz(rel(
                HNumType::I32,
                HRelop::Eq,
                bin(HNumType::I32, HBinop::Xor, local(0), i32c(-1)),
                local(0),
            ))
        )
    );
    // Term-position `!x` is the i32.eqz form (relop Eq x 0).
    let body = "forall { let a: bool = @; assert(!a == a); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            guard(0),
            nz(rel(
                HNumType::I32,
                HRelop::Eq,
                rel(HNumType::I32, HRelop::Eq, local(0), i32c(0)),
                local(0),
            ))
        )
    );
}

/// The constant a literal of type `decl_ty` denotes. A typed `let` fixes the
/// literal's width and the pure-let inlines its constant, so the right-hand side
/// of the comparison is the parsed constant itself.
fn literal_const_of(decl_ty: &str, literal: &str) -> HTerm {
    let body =
        format!("forall {{ let m: {decl_ty} = {literal}; let a: {decl_ty} = @; assert(a == m); }}");
    let obligation = obligation_of("", &body);
    let HAssert::Imp(antecedent, claim) = obligation else {
        panic!("expected a guarded implication, got {obligation:?}");
    };
    // Only `a` takes a slot: a pure `let` inlines its constant and is unguarded.
    assert_eq!(*antecedent, hastype(local(0), guard_width(decl_ty)));
    match *claim {
        HAssert::Not(inner) => match *inner {
            HAssert::TermEq(HTerm::Relop(_, HRelop::Eq, _, rhs), _) => *rhs,
            other => panic!("expected nz(relop Eq ..), got {other:?}"),
        },
        other => panic!("expected nz(..), got {other:?}"),
    }
}

#[test]
fn literals_parse_per_width_including_cast_signed() {
    // Every width below 64 bits rides in an i32 constant: signed widths at their
    // most negative value, unsigned widths at their maximum.
    assert_eq!(literal_const_of("i8", "-128"), i32c(-128));
    assert_eq!(literal_const_of("i16", "-32768"), i32c(-32768));
    assert_eq!(literal_const_of("i32", "-2147483648"), i32c(i32::MIN));
    assert_eq!(literal_const_of("u8", "255"), i32c(255));
    assert_eq!(literal_const_of("u16", "65535"), i32c(65535));
    // 64-bit widths take an i64 constant.
    assert_eq!(literal_const_of("i64", "5"), i64c(5));
    assert_eq!(
        literal_const_of("i64", "9223372036854775807"),
        i64c(i64::MAX)
    );
    assert_eq!(
        literal_const_of("i64", "-9223372036854775808"),
        i64c(i64::MIN)
    );
    // An unsigned value is the signed constant with the same bit pattern. The
    // maxima alone would not discriminate — every all-ones pattern is -1 at any
    // width — so each also takes the value one past its signed maximum, which is
    // where a wrong-width reinterpretation would show.
    assert_eq!(literal_const_of("u32", "4294967295"), i32c(-1));
    assert_eq!(literal_const_of("u32", "2147483648"), i32c(i32::MIN));
    assert_eq!(literal_const_of("u64", "18446744073709551615"), i64c(-1));
    assert_eq!(
        literal_const_of("u64", "9223372036854775808"),
        i64c(i64::MIN)
    );
    // bool literal.
    let body = "forall { let m: bool = true; let a: bool = @; assert(a == m); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            guard(0),
            nz(rel(HNumType::I32, HRelop::Eq, local(0), i32c(1)))
        )
    );
}

#[test]
fn enum_variant_lowers_to_its_tag_constant() {
    let body = "forall { let c: Color = @; assert(c == Color::Blue); }";
    let obligation = obligation_of("enum Color { Red, Green, Blue }", body);
    assert_eq!(
        obligation,
        imp(
            guard(0),
            nz(rel(HNumType::I32, HRelop::Eq, local(0), i32c(2)))
        )
    );
}

// ----- 3. boolean structure ---------------------------------------------

#[test]
fn conjunction_splits_and_disjunction_is_or() {
    let body = "forall { let a: i32 = @; let b: i32 = @; assert(a > 0 && b > 0); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), guard(1)),
            and(nz(gts(local(0), i32c(0))), nz(gts(local(1), i32c(0))))
        )
    );
    let body = "forall { let a: i32 = @; let b: i32 = @; assert(a > 0 || b > 0); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), guard(1)),
            or(nz(gts(local(0), i32c(0))), nz(gts(local(1), i32c(0))))
        )
    );
}

#[test]
fn negation_of_a_comparison_is_the_zero_equality() {
    let body = "forall { let a: i32 = @; assert(!(a > 0)); }";
    assert_eq!(
        obligation_of("", body),
        imp(guard(0), eqz(gts(local(0), i32c(0))))
    );
}

#[test]
fn equality_is_non_strict_universally_and_strict_existentially() {
    // Universal `==` is nz(relop Eq ..).
    let body = "forall { let a: i32 = @; let b: i32 = @; assert(a == b); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), guard(1)),
            nz(rel(HNumType::I32, HRelop::Eq, local(0), local(1)))
        )
    );
    // Existential `==` is strict term_eq. Only the universal `n` is guarded; the
    // witness the prover picks needs no typing hypothesis.
    let body = "forall { let n: i32 = @; exists { let m: i32 = @; assert(m == n); } }";
    assert_eq!(
        obligation_of("", body),
        imp(guard(0), ex(teq(lvar(0), local(0))))
    );
}

/// `!=` carries no `HA_defined` conjunct of its own. Both sides denoting is
/// exactly what the verifier's strictified negated equality already demands, so
/// an emitted conjunct would restate the relop's own definedness — including on
/// the `T_app`-bearing side, which is the case the conjunct used to single out.
#[test]
fn disequality_is_a_bare_negated_relop_like_every_comparison() {
    let body = "forall { let a: i32 = @; let b: i32 = @; assert(a != b); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), guard(1)),
            nz(rel(HNumType::I32, HRelop::Ne, local(0), local(1)))
        )
    );
    let prelude = "fn g(x: i32) -> i32 { return x; }";
    let body = "forall { let a: i32 = @; assert(g(a) != a); }";
    assert_eq!(
        obligation_of(prelude, body),
        imp(
            guard(0),
            nz(rel(
                HNumType::I32,
                HRelop::Ne,
                app("g", vec![local(0)]),
                local(0)
            ))
        )
    );
}

// ----- 4. bindings, slots, scoping --------------------------------------

#[test]
fn pure_let_is_inlined_as_a_term() {
    let body = "forall { let a: i32 = @; let s: i32 = a + 1; assert(s > 0); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            guard(0),
            nz(gts(
                bin(HNumType::I32, HBinop::Add, local(0), i32c(1)),
                i32c(0)
            ))
        )
    );
}

#[test]
fn let_of_call_inlines_the_application() {
    let prelude = "fn g(x: i32) -> i32 { return x; }";
    let body = "forall { let a: i32 = @; let c: i32 = g(a); assert(c > 0); }";
    assert_eq!(
        obligation_of(prelude, body),
        imp(guard(0), nz(gts(app("g", vec![local(0)]), i32c(0))))
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
    // p = slot 0, a = slot 1, t = inlined term, b = slot 2. All three guards are
    // still pending at the assert, and drain there in introduction order.
    assert_eq!(
        sole_obligation(&ok(source), "S"),
        imp(
            and(guard(0), and(guard(1), guard(2))),
            nz(gts(
                local(2),
                bin(HNumType::I32, HBinop::Add, local(0), local(1))
            ))
        )
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
    // trailing assert after the branch scope is restored. `b`'s guard is scoped
    // to the consequent it was introduced in, while `a`'s spans the whole `if`.
    let expected = imp(
        guard(0),
        and(
            imp(
                nz(gts(local(0), i32c(0))),
                imp(guard(1), nz(gts(local(1), i32c(5)))),
            ),
            nz(gts(local(0), i32c(3))),
        ),
    );
    assert_eq!(sole_obligation(&ok(source), "S"), expected);
}

#[test]
fn block_local_const_is_inlined() {
    let body = "forall { let a: i32 = @; const k: i32 = 3; assert(a > k); }";
    assert_eq!(
        obligation_of("", body),
        imp(guard(0), nz(gts(local(0), i32c(3))))
    );
}

// ----- 5. existentials / de Bruijn --------------------------------------

#[test]
fn single_existential_binds_lvar_zero() {
    let body = "forall { let n: i32 = @; exists { let m: i32 = @; assert(m > n); } }";
    assert_eq!(
        obligation_of("", body),
        imp(guard(0), ex(nz(gts(lvar(0), local(0)))))
    );
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
        imp(
            guard(0),
            imp(nz(gts(local(0), i32c(0))), nz(gts(local(0), i32c(1))))
        )
    );
}

#[test]
fn universal_if_else_is_a_guard_pair() {
    let body = "forall { let n: i32 = @; if n > 0 { assert(n > 1); } else { assert(n < 0); } }";
    let cond = || gts(local(0), i32c(0));
    let expected = imp(
        guard(0),
        and(
            imp(nz(cond()), nz(gts(local(0), i32c(1)))),
            imp(eqz(cond()), nz(lts(local(0), i32c(0)))),
        ),
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
            guard(0),
            imp(
                nz(app("is_even", vec![local(0)])),
                nz(gts(local(0), i32c(0)))
            )
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
        imp(
            guard(0),
            nz(rel(
                HNumType::I32,
                HRelop::Eq,
                app("S.helper", vec![]),
                local(0)
            ))
        )
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
        imp(
            guard(0),
            nz(rel(
                HNumType::I32,
                HRelop::Eq,
                app("lib.add", vec![local(0), local(0)]),
                local(0),
            ))
        )
    );
}

// ----- 8. simplification -------------------------------------------------

#[test]
fn assert_free_and_empty_bodies_are_trivially_true() {
    assert_eq!(
        sole_obligation(&ok("spec S { fn f() forall { } }"), "S"),
        HAssert::True
    );
    // A slot with nothing left to read it guards nothing: the pending guard is
    // dropped rather than left dangling over a `⊤` claim.
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
    // The slot's guard fuses into the `assume` antecedent; the fold leaves no
    // `∧ ⊤` behind it.
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), nz(gts(local(0), i32c(0)))),
            nz(gts(local(0), i32c(1)))
        )
    );
}

// ----- 9. universal-slot typing guards -----------------------------------

/// Both a named and an ignored parameter take a guarded slot. A guard on a slot
/// no payload reads is inert, and emitting one uniformly keeps slot numbering
/// out of a use analysis.
#[test]
fn parameter_slots_are_guarded_whether_named_or_ignored() {
    let source = "spec S { fn f(p: i32, _: i32) forall { assert(p > 0); } }";
    assert_eq!(
        sole_obligation(&ok(source), "S"),
        imp(and(guard(0), guard(1)), nz(gts(local(0), i32c(0))))
    );
}

/// A parameter's guard fuses into the antecedent a following `assume` builds,
/// exactly as a `let`-introduced slot's does.
#[test]
fn parameter_guard_fuses_with_a_following_assume() {
    let source = "spec S { fn f(p: i32) forall { assume { assert(p > 0); } assert(p > 1); } }";
    assert_eq!(
        sole_obligation(&ok(source), "S"),
        imp(
            and(guard(0), nz(gts(local(0), i32c(0)))),
            nz(gts(local(0), i32c(1)))
        )
    );
}

/// Several slots pending at one `assume` fuse in introduction order, the guards
/// first and the source filter innermost.
#[test]
fn several_slot_guards_fuse_with_one_assume() {
    let body =
        "forall { let a: i32 = @; let b: i32 = @; assume { assert(a > b); } assert(a > 0); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), and(guard(1), nz(gts(local(0), local(1))))),
            nz(gts(local(0), i32c(0)))
        )
    );
}

/// An anonymous slot taken by a `@` in call-argument position is guarded like a
/// named one, and joins the drain of the statement that introduced it.
#[test]
fn uzumaki_in_call_argument_position_takes_a_guarded_slot() {
    let prelude = "fn g(x: i32) -> i32 { return x; }";
    let body = "forall { assert(g(@) > 0); }";
    assert_eq!(
        obligation_of(prelude, body),
        imp(guard(0), nz(gts(app("g", vec![local(0)]), i32c(0))))
    );
    // An anonymous slot has no declared type, so the width comes from the type
    // recorded for the argument — a 64-bit parameter position guards at i64.
    let prelude = "fn h(x: i64) -> i64 { return x; }";
    let body = "forall { assert(h(@) > 0); }";
    assert_eq!(
        obligation_of(prelude, body),
        imp(
            hastype(local(0), HNumType::I64),
            nz(rel(
                HNumType::I64,
                HRelop::GtS,
                app("h", vec![local(0)]),
                i64c(0)
            ))
        )
    );
}

/// A `@` inside a *pure* `let`'s right-hand side takes its slot there, but the
/// guard waits for the next structural statement, so it scopes over the uses of
/// the inlined term rather than over the `let` alone.
#[test]
fn uzumaki_inside_a_pure_let_drains_at_the_next_statement() {
    let prelude = "fn g(x: i32) -> i32 { return x; }";
    let body = "forall { let t: i32 = g(@); assert(t > 0); }";
    assert_eq!(
        obligation_of(prelude, body),
        imp(guard(0), nz(gts(app("g", vec![local(0)]), i32c(0))))
    );
}

/// One guard covers every later reader of its slot, not merely the next
/// statement: the drain runs before the rest of the block is translated, so a
/// deeper structural statement cannot capture the guard into a narrower scope.
#[test]
fn one_guard_dominates_every_later_assert() {
    let body = "forall { let a: i32 = @; assert(a > 0); assert(a < 10); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            guard(0),
            and(nz(gts(local(0), i32c(0))), nz(lts(local(0), i32c(10))))
        )
    );
}

/// The declared type fixes the guard's width: only `i64`/`u64` guard at 64
/// bits, while bool, enums, and every sub-word integer ride i32.
#[test]
fn slot_guard_width_follows_the_declared_type() {
    for decl_ty in ["bool", "i8", "u8", "i16", "u16", "i32", "u32", "i64", "u64"] {
        let body = format!("forall {{ let x: {decl_ty} = @; assert(x == x); }}");
        let obligation = obligation_of("", &body);
        let HAssert::Imp(antecedent, _) = obligation else {
            panic!("expected a guarded implication for `{decl_ty}`, got {obligation:?}");
        };
        assert_eq!(
            *antecedent,
            hastype(local(0), guard_width(decl_ty)),
            "guard width for a `{decl_ty}` slot"
        );
    }
    // An enum slot is its i32 tag.
    let body = "forall { let c: Color = @; assert(c == Color::Red); }";
    assert_eq!(
        obligation_of("enum Color { Red, Green, Blue }", body),
        imp(
            guard(0),
            nz(rel(HNumType::I32, HRelop::Eq, local(0), i32c(0)))
        )
    );
}

/// The shape a knowingly false specification emits. Downstream, wasm-verifier's
/// strictified `ValidSpec` rejects the unguarded form of this payload outright
/// and discharges the guarded one only against a real interpretation of `f`;
/// its `theories/examples/with_spec.v` carries the two as a negative/positive
/// pair.
#[test]
fn a_false_spec_emits_the_guarded_shape() {
    let prelude = "fn foo(x: i32) -> i32 { return x; }";
    let body = "forall { let x: i32 = @; assert(foo(x) == 42); }";
    assert_eq!(
        obligation_of(prelude, body),
        imp(
            guard(0),
            nz(rel(
                HNumType::I32,
                HRelop::Eq,
                app("foo", vec![local(0)]),
                i32c(42)
            ))
        )
    );
}

/// Collects the slots an antecedent states a typing for, walking the `And` spine
/// the drain builds.
fn guarded_slots(antecedent: &HAssert, into: &mut Vec<u32>) {
    match antecedent {
        HAssert::HasType(HTerm::Local(slot), _) => into.push(*slot),
        HAssert::And(l, r) => {
            guarded_slots(l, into);
            guarded_slots(r, into);
        }
        _ => {}
    }
}

/// Records every universal slot `assertion` reads in positive position without a
/// dominating typing guard.
///
/// This is an over-approximation of the emitter's own discipline, not a semantic
/// check: an `Imp` contributes the `HasType` conjuncts of its antecedent to the
/// guarded set of its consequent and is not itself descended into, since an
/// antecedent is a hypothesis whose own reads are discharged by refuting it.
/// Every other connective passes the guarded set through unchanged.
fn unguarded_reads(assertion: &HAssert, guarded: &[u32], out: &mut Vec<u32>) {
    match assertion {
        // A constant reads nothing, and a typing or definedness atom states a
        // slot's premise rather than relying on one.
        HAssert::True | HAssert::False | HAssert::HasType(_, _) | HAssert::Defined(_) => {}
        HAssert::Not(inner) | HAssert::Ex(inner) => unguarded_reads(inner, guarded, out),
        HAssert::And(l, r) | HAssert::Or(l, r) => {
            unguarded_reads(l, guarded, out);
            unguarded_reads(r, guarded, out);
        }
        HAssert::Imp(antecedent, consequent) => {
            let mut extended = guarded.to_vec();
            guarded_slots(antecedent, &mut extended);
            unguarded_reads(consequent, &extended, out);
        }
        HAssert::TermEq(a, b) => {
            unguarded_term_reads(a, guarded, out);
            unguarded_term_reads(b, guarded, out);
        }
        HAssert::AppOk(_, args) => {
            for arg in args {
                unguarded_term_reads(arg, guarded, out);
            }
        }
    }
}

fn unguarded_term_reads(term: &HTerm, guarded: &[u32], out: &mut Vec<u32>) {
    match term {
        HTerm::Local(slot) if !guarded.contains(slot) => out.push(*slot),
        HTerm::Local(_) | HTerm::Const(_) | HTerm::LVar(_) => {}
        HTerm::App(_, args) => {
            for arg in args {
                unguarded_term_reads(arg, guarded, out);
            }
        }
        HTerm::Binop(_, _, l, r) | HTerm::Relop(_, _, l, r) => {
            unguarded_term_reads(l, guarded, out);
            unguarded_term_reads(r, guarded, out);
        }
    }
}

/// Whatever a spec body's shape, no positive-position read of a universal slot
/// escapes its typing guard. The matrix crosses every way a slot is introduced
/// (parameter named and ignored, `let`, bare call argument, call argument inside
/// a pure `let`) with every place the drain can land (assert, chained asserts,
/// `assume`, `if`/`else`, a branch-local slot, an exists arm).
#[test]
fn every_universal_slot_read_is_dominated_by_its_guard() {
    let sources = [
        "spec S { fn f(p: i32, q: i32) forall { assert(p > q); } }",
        "spec S { fn f(p: i32, _: i32) forall { assert(p > 0); } }",
        "spec S { fn f() forall { let a: i32 = @; let b: i32 = @; assert(a > b); } }",
        "spec S { fn f() forall { let a: i32 = @; assert(a > 0); assert(a < 10); } }",
        "spec S { fn f() forall { let a: i32 = @; assume { assert(a > 0); } assert(a > 1); } }",
        "spec S { fn f(p: i32) forall { let a: i32 = @; assume { assert(a > p); } assert(a > 0); } }",
        "spec S { fn f() forall { let a: i32 = @; if a > 0 { assert(a > 1); } else { assert(a < 0); } } }",
        "spec S { fn f() forall { let a: i32 = @; if a > 0 { let b: i32 = @; assert(b > a); } assert(a > 3); } }",
        "spec S { fn f() forall { let n: i32 = @; exists { let m: i32 = @; assert(m > n); } } }",
        "fn g(x: i32) -> i32 { return x; }\nspec S { fn f() forall { assert(g(@) > 0); } }",
        "fn g(x: i32) -> i32 { return x; }\nspec S { fn f() forall { let t: i32 = g(@); assert(t > 0); } }",
        "fn g(x: i32) -> i32 { return x; }\nspec S { fn f() forall { let a: i32 = @; assert(g(a) != a); } }",
    ];
    for source in sources {
        for entries in ok(source).values() {
            for entry in entries {
                let mut unguarded = Vec::new();
                unguarded_reads(&entry.hassert, &[], &mut unguarded);
                assert!(
                    unguarded.is_empty(),
                    "slots {unguarded:?} are read with no dominating guard in `{source}`; \
                     obligation: {:?}",
                    entry.hassert
                );
            }
        }
    }
}

/// The checker above must be able to fail, or the matrix proves nothing: the
/// pre-guard shape of a claim, and a guard over the wrong slot, are both caught.
#[test]
fn the_domination_checker_catches_an_unguarded_read() {
    let mut bare = Vec::new();
    unguarded_reads(&nz(gts(local(0), i32c(0))), &[], &mut bare);
    assert_eq!(bare, vec![0]);

    let mut mismatched = Vec::new();
    unguarded_reads(
        &imp(guard(1), nz(gts(local(0), i32c(0)))),
        &[],
        &mut mismatched,
    );
    assert_eq!(mismatched, vec![0]);
}

// ----- 10. diagnostics ---------------------------------------------------

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
