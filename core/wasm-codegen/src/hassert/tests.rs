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

use crate::errors::CodegenError;
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
fn eqs(l: HTerm, r: HTerm) -> HTerm {
    rel(HNumType::I32, HRelop::Eq, l, r)
}
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

// ----- 4. short-circuit `&&`/`||` as terms -------------------------------

/// The constraint a term-position `l || r` pins its fresh witness `v` with:
/// `Hor (l ≠ 0 ∧ v = 1) (l = 0 ∧ v = r)`, naming the same two cases the
/// compiled `if l != 0 then 1 else r` branches on.
fn or_witness(v: HTerm, l: HTerm, r: HTerm) -> HAssert {
    or(
        and(nz(l.clone()), teq(v.clone(), i32c(1))),
        and(eqz(l), teq(v, r)),
    )
}

/// The dual for `l && r`: `Hor (l ≠ 0 ∧ v = r) (l = 0 ∧ v = 0)`, mirroring
/// `if l != 0 then r else 0`.
fn and_witness(v: HTerm, l: HTerm, r: HTerm) -> HAssert {
    or(
        and(nz(l.clone()), teq(v.clone(), r)),
        and(eqz(l), teq(v, i32c(0))),
    )
}

/// [`or_witness`] where the right operand introduced a constraint of its own,
/// conjoined *inside* the arm that evaluates that operand. Kept as a separate
/// builder rather than an `⊤` argument to [`or_witness`], so the two shapes stay
/// distinguishable: the pass absorbs an `⊤` conjunct away, and a test that let
/// the two collapse into one spelling could no longer tell them apart.
fn or_witness_with(v: HTerm, l: HTerm, right: HAssert, r: HTerm) -> HAssert {
    or(
        and(nz(l.clone()), teq(v.clone(), i32c(1))),
        and(eqz(l), and(right, teq(v, r))),
    )
}

/// The term language is strict and has no conditional, so a term-position `||`
/// cannot be an eager `T_binop`: `x == 0 || 10 / x == 10 / x` is true for every
/// `i32` yet an eager encoding demands the quotient at `x = 0` and turns it into
/// a refutable claim. The operator is a fresh `HA_ex`-bound witness instead,
/// pinned by a two-armed constraint over the value the compiled code branches to.
#[test]
fn term_position_or_binds_a_pinned_witness() {
    let body = "forall { let a: i32 = @; let b: i32 = @; assert((a == 0 || b == 0) == true); }";
    let v = || lvar(0);
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), guard(1)),
            ex(and(
                or_witness(v(), eqs(local(0), i32c(0)), eqs(local(1), i32c(0))),
                nz(eqs(v(), i32c(1)))
            ))
        )
    );
}

/// The `&&` dual, whose skipped arm pins the witness to `0` rather than `1`.
#[test]
fn term_position_and_binds_a_pinned_witness() {
    let body = "forall { let a: i32 = @; let b: i32 = @; assert((a == 0 && b == 0) == true); }";
    let v = || lvar(0);
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), guard(1)),
            ex(and(
                and_witness(v(), eqs(local(0), i32c(0)), eqs(local(1), i32c(0))),
                nz(eqs(v(), i32c(1)))
            ))
        )
    );
}

/// The case that separates the correct encoding from the plausible wrong one.
/// When the right operand is itself short-circuit, its constraint must sit
/// *inside* the outer operator's `eqz` arm: that arm is the only one where the
/// source evaluates it, so a constraint hoisted above the disjunction would be
/// demanded on the arm the program skips — the original bug, one level up.
///
/// The two binders and their levels are pinned with it: the inner witness is
/// allocated first and so binds outermost, leaving the outer operator's witness
/// at index 0 and the inner one at index 1 where the constraint reads them.
#[test]
fn a_short_circuit_right_operands_constraint_stays_inside_the_arm_that_evaluates_it() {
    let body = "forall { let a: i32 = @; let b: i32 = @; let c: i32 = @; \
                assert((a == 0 || b == 0 && c == 0) == true); }";
    let outer = || lvar(0);
    let inner = || lvar(1);
    let inner_def = || and_witness(inner(), eqs(local(1), i32c(0)), eqs(local(2), i32c(0)));
    let guards = || and(guard(0), and(guard(1), guard(2)));
    let claim = || nz(eqs(outer(), i32c(1)));

    assert_eq!(
        obligation_of("", body),
        imp(
            guards(),
            ex(ex(and(
                or_witness_with(outer(), eqs(local(0), i32c(0)), inner_def(), inner()),
                claim()
            )))
        )
    );

    // Spelled out because it is the shape a hoist produces and the reason the
    // capture exists: the same tree with the inner constraint lifted above the
    // outer disjunction, where `c` is demanded even when `a` already decided the
    // result. Everything else about the two trees is identical.
    let hoisted = imp(
        guards(),
        ex(ex(and(
            inner_def(),
            and(
                or_witness(outer(), eqs(local(0), i32c(0)), inner()),
                claim(),
            ),
        ))),
    );
    assert_ne!(obligation_of("", body), hoisted);
}

/// A *left* operand is evaluated unconditionally, so its constraint keeps the
/// unconditional placement every other operand position gets — at its own
/// binder, not inside an arm. Left-associative chains (`a || b || c`) are all
/// left operands, so none of them takes the conditional treatment.
#[test]
fn a_left_operands_constraint_is_unconditional_at_its_own_binder() {
    let body = "forall { let a: i32 = @; let b: i32 = @; let c: i32 = @; \
                assert(((a == 0 || b == 0) || c == 0) == true); }";
    // The left `||`'s witness binds outermost: index 0 under its own binder,
    // index 1 once the outer operator's binder is entered.
    let left_at_1 = || lvar(0);
    let left_at_2 = || lvar(1);
    let outer = || lvar(0);
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), and(guard(1), guard(2))),
            ex(and(
                or_witness(left_at_1(), eqs(local(0), i32c(0)), eqs(local(1), i32c(0))),
                ex(and(
                    or_witness(outer(), left_at_2(), eqs(local(2), i32c(0))),
                    nz(eqs(outer(), i32c(1)))
                ))
            ))
        )
    );
}

/// Two independent witnesses in one `assert`. Levels are absolute while the
/// emitted `T_lvar`s are de Bruijn indices, so an off-by-one in the final
/// level-to-index pass shows here as the first witness escaping its binder: it
/// is read at index 1 from inside the second binder and at index 0 outside it.
#[test]
fn two_witnesses_in_one_assert_index_correctly() {
    let body = "forall { let a: i32 = @; let b: i32 = @; let c: i32 = @; let d: i32 = @; \
                assert((a == 0 || b == 0) == (c == 0 || d == 0)); }";
    let first_at_1 = || lvar(0);
    let first_at_2 = || lvar(1);
    let second = || lvar(0);
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), and(guard(1), and(guard(2), guard(3)))),
            ex(and(
                or_witness(first_at_1(), eqs(local(0), i32c(0)), eqs(local(1), i32c(0))),
                ex(and(
                    or_witness(second(), eqs(local(2), i32c(0)), eqs(local(3), i32c(0))),
                    nz(eqs(first_at_2(), second()))
                ))
            ))
        )
    );
}

/// Assertion position already splits `&&`/`||` into `HA_and`/`Hor`, and the same
/// placement rule applies to what the split's right side brings with it: a
/// witness constraint from the right operand rides into that operand's own arm.
/// The binder itself still hoists to the statement's atom, so on the arm the
/// source skips it is bound but unconstrained — which is sound, because nothing
/// there reads it.
#[test]
fn an_assertion_position_operator_carries_its_right_constraint_into_its_own_arm() {
    let v = || lvar(0);
    let def = || or_witness(v(), eqs(local(0), i32c(0)), eqs(local(1), i32c(0)));
    let claim = || nz(eqs(v(), i32c(1)));
    let guards = || and(guard(0), guard(1));
    let left = || nz(gts(local(0), i32c(0)));

    let body = "forall { let a: i32 = @; let b: i32 = @; \
                assert(a > 0 && (a == 0 || b == 0) == true); }";
    assert_eq!(
        obligation_of("", body),
        imp(guards(), ex(and(left(), and(def(), claim()))))
    );

    let body = "forall { let a: i32 = @; let b: i32 = @; \
                assert(a > 0 || (a == 0 || b == 0) == true); }";
    assert_eq!(
        obligation_of("", body),
        imp(guards(), ex(or(left(), and(def(), claim()))))
    );
}

/// `!` over a term-position short-circuit takes the falsiness of the *witness*,
/// leaving the constraint untouched — the operator's own encoding does not
/// change with the polarity it is read in.
#[test]
fn negating_a_term_position_short_circuit_negates_only_the_witness() {
    let v = || lvar(0);
    let guards = || and(guard(0), guard(1));
    let operands = || (eqs(local(0), i32c(0)), eqs(local(1), i32c(0)));

    let body = "forall { let a: i32 = @; let b: i32 = @; assert(!((a == 0 || b == 0) == true)); }";
    let (l, r) = operands();
    assert_eq!(
        obligation_of("", body),
        imp(
            guards(),
            ex(and(or_witness(v(), l, r), eqz(eqs(v(), i32c(1)))))
        )
    );

    let body = "forall { let a: i32 = @; let b: i32 = @; assert(!((a == 0 && b == 0) == true)); }";
    let (l, r) = operands();
    assert_eq!(
        obligation_of("", body),
        imp(
            guards(),
            ex(and(and_witness(v(), l, r), eqz(eqs(v(), i32c(1)))))
        )
    );
}

/// The falsiness dual moves a right operand's constraint exactly as the positive
/// translation does. De Morgan swaps which connective the arm sits under, so a
/// capture implemented on one side only would leak the constraint out of its arm
/// as soon as the assertion appeared under a `!`.
#[test]
fn the_falsiness_dual_moves_a_right_constraint_too() {
    let v = || lvar(0);
    let def = || or_witness(v(), eqs(local(0), i32c(0)), eqs(local(1), i32c(0)));
    let claim = || eqz(eqs(v(), i32c(1)));
    let guards = || and(guard(0), guard(1));
    let left = || eqz(gts(local(0), i32c(0)));

    // ¬(p ∧ q) becomes ¬p ∨ (C_q ∧ ¬q).
    let body = "forall { let a: i32 = @; let b: i32 = @; \
                assert(!(a > 0 && (a == 0 || b == 0) == true)); }";
    assert_eq!(
        obligation_of("", body),
        imp(guards(), ex(or(left(), and(def(), claim()))))
    );

    // ¬(p ∨ q) becomes ¬p ∧ (C_q ∧ ¬q).
    let body = "forall { let a: i32 = @; let b: i32 = @; \
                assert(!(a > 0 || (a == 0 || b == 0) == true)); }";
    assert_eq!(
        obligation_of("", body),
        imp(guards(), ex(and(left(), and(def(), claim()))))
    );
}

/// A pure `let` is where the inlined term is *read*, not where it is claimed, so
/// its witness binder scopes over the rest of the block. The pending slot guards
/// drain ahead of the binder rather than inside it: the constraint reads the very
/// slots they type, so `HA_ex (guard → …)` would pin a value at slots nothing has
/// typed yet — the escape the guards exist to close. Chained bindings nest in
/// source order.
#[test]
fn a_witness_bound_by_a_pure_let_scopes_over_the_rest_under_its_guards() {
    let body = "forall { let a: i32 = @; let b: i32 = @; \
                let ok: bool = a == 0 || b == 0; assert(ok); }";
    let v = || lvar(0);
    let obligation = obligation_of("", body);
    assert_eq!(
        obligation,
        imp(
            and(guard(0), guard(1)),
            ex(and(
                or_witness(v(), eqs(local(0), i32c(0)), eqs(local(1), i32c(0))),
                nz(v())
            ))
        )
    );
    // Stated on its own so the nesting order cannot be lost in a future
    // regeneration of the tree above: guard outside, binder inside.
    assert!(
        matches!(&obligation, HAssert::Imp(_, consequent) if matches!(**consequent, HAssert::Ex(_))),
        "the slot guards must dominate the binder, not sit inside it: {obligation:?}"
    );

    // Two bindings: the second binder nests inside the first, and the first is
    // still read at the deeper level.
    let body = "forall { let a: i32 = @; let b: i32 = @; let p: bool = a == 0 || b == 0; \
                let q: bool = a > 0 && b > 0; assert(p); assert(q); }";
    let p_at_1 = || lvar(0);
    let p_at_2 = || lvar(1);
    let q = || lvar(0);
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), guard(1)),
            ex(and(
                or_witness(p_at_1(), eqs(local(0), i32c(0)), eqs(local(1), i32c(0))),
                ex(and(
                    and_witness(q(), gts(local(0), i32c(0)), gts(local(1), i32c(0))),
                    and(nz(p_at_2()), nz(q()))
                ))
            ))
        )
    );
}

/// A block-local `const` binds a witness exactly like a pure `let` does — same
/// scoping over the rest of the block, same guard drain ahead of the binder.
#[test]
fn a_witness_bound_by_a_const_scopes_over_the_rest_like_a_pure_let() {
    let body = "forall { let a: i32 = @; const k: bool = 1 == 0 || 2 == 0; \
                assert(k); assert(a > 0); }";
    let v = || lvar(0);
    assert_eq!(
        obligation_of("", body),
        imp(
            guard(0),
            ex(and(
                or_witness(v(), eqs(i32c(1), i32c(0)), eqs(i32c(2), i32c(0))),
                and(nz(v()), nz(gts(local(0), i32c(0))))
            ))
        )
    );
}

/// An `if` condition is translated once and read on both arms, so one binder
/// wraps the whole contribution and the constraint appears once. An encoding
/// that rebuilt the condition per arm would duplicate the whole two-armed
/// constraint and, with it, every trap-guarding operand inside it.
#[test]
fn a_witness_in_an_if_condition_is_bound_once_over_both_arms() {
    let v = || lvar(0);
    let def = || or_witness(v(), eqs(local(0), i32c(0)), eqs(local(1), i32c(0)));
    let guards = || and(guard(0), guard(1));

    let body = "forall { let a: i32 = @; let b: i32 = @; \
                if a == 0 || b == 0 { assert(a > 0); } }";
    assert_eq!(
        obligation_of("", body),
        imp(
            guards(),
            ex(and(def(), imp(nz(v()), nz(gts(local(0), i32c(0))))))
        )
    );

    let body = "forall { let a: i32 = @; let b: i32 = @; \
                if a == 0 || b == 0 { assert(a > 0); } else { assert(b > 0); } }";
    assert_eq!(
        obligation_of("", body),
        imp(
            guards(),
            ex(and(
                def(),
                and(
                    imp(nz(v()), nz(gts(local(0), i32c(0)))),
                    imp(eqz(v()), nz(gts(local(1), i32c(0))))
                )
            ))
        )
    );
}

/// A pinned witness and a prover-chosen `@` are both `HA_ex` binders and share
/// one allocation order, so an existential body that introduces both must
/// interleave their levels rather than keep two counters. The `@` binder is
/// emitted without a definition — pinning it would be the opposite of what `@`
/// means — while the witness keeps its own.
#[test]
fn an_existential_witness_and_a_call_argument_uzumaki_interleave_by_level() {
    let prelude = "fn g(x: i32) -> i32 { return x; }";
    // Three binders, outermost first: `m`, the anonymous call-argument `@`, and
    // the witness. Read from the innermost point they are indices 2, 1 and 0.
    let m = || lvar(2);
    let anon = || lvar(1);
    let v = || lvar(0);

    // Term position: the constraint stays at the witness's own binder.
    let body = "forall { let n: i32 = @; exists { let m: i32 = @; \
                assert((g(@) > 0 || m == 0) == true); } }";
    assert_eq!(
        obligation_of(prelude, body),
        imp(
            guard(0),
            ex(ex(ex(and(
                or_witness(v(), gts(app("g", vec![anon()]), i32c(0)), eqs(m(), i32c(0))),
                teq(v(), i32c(1))
            ))))
        )
    );

    // Assertion position: the constraint moves into the `HA_and`'s right arm and
    // both binders are left carrying nothing, which the wrap emits bare.
    let body = "forall { let n: i32 = @; exists { let m: i32 = @; \
                assert(g(@) > 0 && (m == 0 || n == 0) == true); } }";
    assert_eq!(
        obligation_of(prelude, body),
        imp(
            guard(0),
            ex(ex(ex(and(
                nz(gts(app("g", vec![anon()]), i32c(0))),
                and(
                    or_witness(v(), eqs(m(), i32c(0)), eqs(local(0), i32c(0))),
                    teq(v(), i32c(1))
                )
            ))))
        )
    );
}

/// A definition pins a value, so keeping one for a variable the payload never
/// reads would turn a specification that claims nothing into a refutable claim.
/// The binder still survives — dropping it would shift the level of every binder
/// allocated inside it — but its definition does not.
#[test]
fn a_witness_nothing_reads_is_emitted_without_its_constraint() {
    // Nothing follows the binding, so the whole body claims nothing: the
    // `∃x. ⊤` the wrap leaves behind collapses, and a specification function
    // that collapses to `⊤` is rejected instead of emitted.
    let source = "spec S { fn f() forall { let a: i32 = @; let b: i32 = @; \
                  let unused: bool = a == 0 || b == 0; } }";
    let dropped = err(source);
    assert!(dropped.contains("error[P010]"), "{dropped}");

    // An unrelated later claim keeps the binder but not the definition, so the
    // divide the source guards against is never demanded.
    let body = "forall { let a: i32 = @; let b: i32 = @; \
                let unused: bool = a == 0 || b == 0; assert(a > 0); }";
    assert_eq!(
        obligation_of("", body),
        imp(and(guard(0), guard(1)), ex(nz(gts(local(0), i32c(0)))))
    );
}

/// A witness in call-argument position is the `T_app`'s argument, so the applied
/// term names the binder rather than restating the operator.
#[test]
fn a_witness_passed_as_a_call_argument_is_the_t_app_argument() {
    let prelude = "fn h(x: bool) -> i32 { return 1; }";
    let body = "forall { let a: i32 = @; let b: i32 = @; assert(h(a == 0 || b == 0) == 1); }";
    let v = || lvar(0);
    assert_eq!(
        obligation_of(prelude, body),
        imp(
            and(guard(0), guard(1)),
            ex(and(
                or_witness(v(), eqs(local(0), i32c(0)), eqs(local(1), i32c(0))),
                nz(eqs(app("h", vec![v()]), i32c(1)))
            ))
        )
    );
}

/// A bare call statement claims `HA_app_ok`, and a witness in its arguments
/// wraps that atom alone: the binder is committed and discharged inside the
/// statement that allocated it, never handed to the next one.
#[test]
fn a_call_statements_witness_wraps_its_own_app_ok_atom() {
    let prelude = "fn h(x: bool) -> i32 { return 1; }";
    let body = "forall { let a: i32 = @; let b: i32 = @; \
                h(a == 0 || b == 0); assert(a > 0); }";
    let v = || lvar(0);
    assert_eq!(
        obligation_of(prelude, body),
        imp(
            and(guard(0), guard(1)),
            and(
                ex(and(
                    or_witness(v(), eqs(local(0), i32c(0)), eqs(local(1), i32c(0))),
                    HAssert::AppOk(HFnRef("h".to_string()), vec![v()])
                )),
                nz(gts(local(0), i32c(0)))
            )
        )
    );
}

/// An expression translated only for its diagnostics contributes no claim, so
/// nothing wraps the binders it introduced and they are dropped with the term.
/// Left pending they would be wrapped around a *later* statement's atom, at a
/// depth where their levels name something else — which is why the assertion
/// here is on the following statement's obligation.
#[test]
fn a_discarded_expression_leaks_no_binder_into_the_next_statement() {
    let body = "forall { let a: i32 = @; let b: i32 = @; a == 0 || b == 0; assert(a > 0); }";
    assert_eq!(
        obligation_of("", body),
        imp(and(guard(0), guard(1)), nz(gts(local(0), i32c(0))))
    );

    // The same for a returned expression. `return` is reachable only in a plain
    // (`Regular`) body, where analysis permits a statement after it.
    let source = "spec S { fn f(p: i32) -> bool { return p == 0 || p == 1; assert(p > 0); } }";
    assert_eq!(
        sole_obligation(&ok(source), "S"),
        imp(guard(0), nz(gts(local(0), i32c(0))))
    );
}

// ----- 5. bindings, slots, scoping --------------------------------------

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

// ----- 6. existentials / de Bruijn --------------------------------------

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

// ----- 7. if forms -------------------------------------------------------

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

// ----- 8. calls ----------------------------------------------------------

/// A call to a specification sibling resolves by that sibling's *folded* key, so
/// `helper` applied from `prop` is `T_app "S.helper"`. Both functions state a
/// property of their own, because a specification function that only computes is
/// rejected outright — a `return`-only sibling could not be the callee here.
///
/// The translation pass is driven directly, without the compile around it: the
/// full pipeline cannot yet resolve a specification-local helper's `T_app`, but
/// that is a separate open defect in the driver, not in the resolution this test
/// pins.
#[test]
fn spec_sibling_helper_is_a_t_app_by_its_folded_key() {
    let source = "\
spec S {
  fn helper(n: i32) -> i32 {
    assert(n > 0);
    return n;
  }
  fn prop() forall {
    let a: i32 = @;
    assert(helper(a) == a);
  }
}
";
    let map = ok(source);
    // The helper carries its own obligation and keeps its place in source order.
    assert_eq!(
        obligation_named(&map, "S", "S.helper"),
        imp(guard(0), nz(gts(local(0), i32c(0))))
    );
    assert_eq!(
        obligation_named(&map, "S", "S.prop"),
        imp(
            guard(0),
            nz(rel(
                HNumType::I32,
                HRelop::Eq,
                app("S.helper", vec![local(0)]),
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

// ----- 9. simplification -------------------------------------------------

/// The ⊤-absorbing fold is what makes "empty", "binds but never claims" and
/// "only computes" one family rather than three shapes to enumerate: each folds
/// to exactly `HA_true`. That collapse is no longer observable as an emitted
/// obligation, so it is observed through the rejection it now produces.
#[test]
fn assert_free_and_empty_bodies_fold_to_the_rejected_vacuous_obligation() {
    let empty = err("spec S { fn f() forall { } }");
    assert!(empty.contains("error[P010]"), "{empty}");
    // A slot with nothing left to read it guards nothing: the pending guard is
    // dropped rather than left dangling over a `⊤` claim.
    let unread = err("spec S { fn f() forall { let a: i32 = @; } }");
    assert!(unread.contains("error[P010]"), "{unread}");
    // A plain (Regular) spec free function folds to `⊤` the same way.
    let computed = err("spec S { fn f() -> i32 { return 0; } }");
    assert!(computed.contains("error[P010]"), "{computed}");
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

// ----- 10. universal-slot typing guards ----------------------------------

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

/// A bare nested block is a structural statement like any other: the guards
/// pending at it drain over both the block's own contribution and the rest of
/// the enclosing body, so a slot introduced before the block dominates a reader
/// inside it *and* a later sibling. The nested block never captures the guard
/// into its own narrower scope.
#[test]
fn a_bare_nested_block_drains_pending_guards() {
    let body = "forall { let a: i32 = @; { assert(a > 0); } assert(a < 10); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            guard(0),
            and(nz(gts(local(0), i32c(0))), nz(lts(local(0), i32c(10))))
        )
    );
}

/// A bare call statement contributes an `HA_app_ok` obligation, and it is
/// structural, so the slots it passes are typed by the guards it drains before
/// the call is claimed to be defined.
#[test]
fn a_bare_call_statement_is_a_guarded_app_ok() {
    let prelude = "fn g(x: i32) -> i32 { return x; }";
    let body = "forall { let a: i32 = @; g(a); }";
    assert_eq!(
        obligation_of(prelude, body),
        imp(
            guard(0),
            HAssert::AppOk(HFnRef("g".to_string()), vec![local(0)])
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
///
/// The short-circuit rows are the ones a witness can break. A witness constraint
/// reads the slots it compares, and it is planted at a binder rather than at a
/// statement's claim, so a drain that ran on the wrong side of the binder would
/// leave those reads under no guard at all. [`unguarded_reads`] walks `HA_ex`
/// transparently, so the binder is not itself a hiding place: an escaped read
/// inside one is reported like any other.
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
        // A witness bound by a pure `let`, reading one slot and then two: a
        // pure `let` is not structural, so the guards it drains are drained
        // nowhere else.
        "spec S { fn f() forall { let a: i32 = @; let ok: bool = a == 0 || a > 5; assert(ok); } }",
        "spec S { fn f() forall { let a: i32 = @; let b: i32 = @; let ok: bool = a > 0 || b > 0; assert(ok); } }",
        // A witness in a `const` initializer takes the same drain.
        "spec S { fn f() forall { let a: i32 = @; const k: bool = 1 == 0 || 2 == 0; assert(k); assert(a > 0); } }",
        // A witness in an `if` condition, where the binder wraps both arms.
        "spec S { fn f() forall { let a: i32 = @; let b: i32 = @; if a == 0 || b == 0 { assert(a > 0); } else { assert(b > 0); } } }",
        // A witness inside a nested block, whose atom is wrapped one level in
        // while the guards drain at the enclosing statement.
        "spec S { fn f() forall { let a: i32 = @; let b: i32 = @; { assert((a == 0 || b == 0) == true); } assert(a < 10); } }",
        // A witness in a branch body, reading a branch-local slot and an outer one.
        "spec S { fn f() forall { let a: i32 = @; if a > 0 { let b: i32 = @; assert((b == 0 || a == 0) == true); } assert(a < 10); } }",
        // A witness reading an anonymous call-argument slot alongside a named one.
        "fn g(x: i32) -> i32 { return x; }\nspec S { fn f() forall { let a: i32 = @; assert((g(@) == 0 || a == 0) == true); } }",
        // A witness whose guards were already drained by a preceding `assume`.
        "spec S { fn f(p: i32) forall { assume { assert(p > 0); } let ok: bool = p == 0 || p > 1; assert(ok); } }",
        // A witness inside an `assume` body, where the binder lands in the
        // antecedent the guards lead.
        "spec S { fn f() forall { let a: i32 = @; let b: i32 = @; assume { assert((a == 0 || b == 0) == true); } assert(a > 0); } }",
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

// ----- 11. diagnostics --------------------------------------------------

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

// ----- 12. the assertion-spine depth budget -------------------------------

/// A boolean chain of `n` operators, right-nested with explicit parentheses:
/// `a == 0 || (a == 1 || (… || a == n))`. Right nesting is the expensive shape,
/// because each operator's constraint is planted inside the previous one's
/// skipped arm rather than beside it.
fn nested_or_chain(n: usize) -> String {
    let mut expr = format!("a == {n}");
    for k in (0..n).rev() {
        expr = format!("a == {k} || ({expr})");
    }
    format!("spec S {{ fn f() forall {{ let a: i32 = @; assert(({expr}) == true); }} }}")
}

/// The same chain written the way a program actually writes one — no
/// parentheses, so the grammar left-associates it and every operator is a *left*
/// operand of the next.
fn flat_or_chain(n: usize) -> String {
    let mut expr = "a == 0".to_string();
    for k in 1..=n {
        expr = format!("{expr} || a == {k}");
    }
    format!("spec S {{ fn f() forall {{ let a: i32 = @; assert(({expr}) == true); }} }}")
}

/// Runs the whole code generator in proof mode on a compiler-sized stack.
///
/// The pre-encode payload gate lives in code generation, not in the translation
/// pass, so these cases cannot be driven through [`obligation_of`]. Deeply
/// nested source also needs more stack than the test harness hands a thread —
/// the parser and the type checker descend once per level — so the pipeline runs
/// on [`inference_parser::MIN_COMPILE_STACK`], the same reservation
/// `inference::with_compiler_stack` makes for a real compile. Without it these
/// cases would abort the process on a stack overflow long before reaching the
/// behaviour under test.
fn proof_codegen(source: String) -> Result<usize, CodegenError> {
    std::thread::Builder::new()
        .stack_size(inference_parser::MIN_COMPILE_STACK)
        .spawn(move || {
            let ctx = type_check(&source);
            crate::codegen(
                &ctx,
                "depth",
                crate::CodegenOptions {
                    target: crate::Target::Wasm32,
                    mode: CompilationMode::Proof,
                    opt_level: crate::Target::Wasm32.default_opt_level(),
                    features: crate::EmitFeatures::default(),
                },
            )
            .map(|output| output.wasm().len())
            .map_err(|e| {
                e.downcast::<CodegenError>()
                    .expect("code generation reports its own error type")
            })
        })
        .expect("spawning the compiler-sized thread")
        .join()
        .expect("the compiler-sized thread must not panic")
}

/// A term-position `&&`/`||` now spends assertion-spine levels rather than term
/// levels, and the two are budgeted separately: a term tree starts a fresh
/// counter at every assertion atom, while the spine accumulates. Each operator
/// costs three spine levels for its constraint plus one for its binder, so a
/// chain long enough to matter is far shorter than it used to be — measured, a
/// right-nested chain of 63 operators is the longest that fits and 64 is the
/// first that does not.
///
/// Overrunning must be an error naming the obligation, not a truncated payload:
/// the encoder is infallible, so an over-deep tree would otherwise serialize
/// into a section the codec's own decoder rejects downstream.
#[test]
fn an_over_deep_boolean_chain_is_rejected_by_name() {
    let err = proof_codegen(nested_or_chain(80))
        .expect_err("a chain past the payload depth cap must not be emitted");
    assert!(
        matches!(
            err,
            CodegenError::HspecTreeTooDeep { ref spec, ref function, max }
                if spec == "S" && function == "S.f" && max == inference_hassert::MAX_TREE_DEPTH
        ),
        "expected the depth cap to be reported against the obligation, got: {err:?}"
    );
}

/// The budget still covers chains of the length a specification plausibly
/// carries: a 32-clause right-nested chain — the expensive nesting — and a
/// 64-clause flat one, the shape source is actually written in, both compile.
#[test]
fn a_realistic_boolean_chain_still_compiles() {
    let nested = proof_codegen(nested_or_chain(32))
        .expect("a 32-operator right-nested chain must stay inside the payload depth cap");
    assert!(nested > 0, "code generation produced an empty module");
    let flat = proof_codegen(flat_or_chain(64))
        .expect("a 64-operator left-associated chain must stay inside the payload depth cap");
    assert!(flat > 0, "code generation produced an empty module");
}

// ----- 13. helper and property roles ------------------------------------

/// The explanation every vacuity report carries between its diagnosis and its
/// remedy. Asserting it apart from the two keeps the per-construct checks on the
/// clauses that actually differ.
const VACUOUS: &str = "so its obligation is the vacuous `HA_true` that any proof discharges \
                       without reading the program";

/// Asserts that `rest` — the text completing `spec S { fn f() … }`, a body
/// optionally preceded by a return type — is rejected as a vacuous obligation,
/// and returns the rendered diagnostic for a wording check.
fn vacuous(rest: &str) -> String {
    let rendered = err(&format!("spec S {{ fn f() {rest} }}"));
    assert!(
        rendered.contains("error[P010]"),
        "expected a vacuity report for `{rest}`, got: {rendered}"
    );
    rendered
}

/// A specification function that only computes states no property at all,
/// however the computation is written: a returned value, a chain of pure `let`s,
/// a block-local `const`, or nothing whatsoever. Each is a helper, and a helper
/// belongs at file scope where a specification can still apply it as a `T_app`.
#[test]
fn a_spec_function_that_only_computes_is_rejected() {
    for rest in [
        "{ }",
        "-> i32 { return 0; }",
        "{ let a: i32 = 1; let b: i32 = a + 2; }",
        "{ const K: i32 = 7; }",
    ] {
        vacuous(rest);
    }
}

/// Quantifying a body is not itself a claim. A `forall` that binds values and
/// never constrains them — or one whose only statement is an expression
/// evaluated for nothing — claims exactly as much as an empty one.
#[test]
fn a_quantified_body_that_binds_but_never_claims_is_rejected() {
    for rest in [
        "forall { }",
        "forall { let a: i32 = @; }",
        "forall { let n: i32 = @; n + 1; }",
    ] {
        vacuous(rest);
    }
}

/// Nesting rescues nothing: a bare block, a further quantifier, an inline
/// non-deterministic block, and an `if` whose arms are both vacuous all fold
/// through the same ⊤-absorbing constructors as the flat shapes do.
#[test]
fn nesting_does_not_rescue_a_body_that_claims_nothing() {
    for rest in [
        "forall { { } }",
        "forall { forall { } }",
        "forall { exists { let m: i32 = @; } }",
        "forall { let n: i32 = @; if n > 0 { } else { } }",
    ] {
        vacuous(rest);
    }
}

/// An `assume` builds an antecedent over the statements that follow it, so one
/// with nothing after it folds to `Imp(p, ⊤) = ⊤`. This is the family the body
/// shape alone would misjudge — the filter is written, translated, and then
/// absorbed — which is why the check reads the translated obligation instead.
#[test]
fn a_trailing_assume_is_absorbed_into_a_vacuous_obligation() {
    for rest in [
        "{ assume { assert(1 > 0); } }",
        "forall { let n: i32 = @; assume { assert(n > 0); } }",
    ] {
        vacuous(rest);
    }
}

/// The remedy names the construct the body actually wrote, so a quantified body,
/// an inline non-deterministic block, and a plain computation each get the fix
/// that applies to them rather than one generic sentence.
#[test]
fn the_vacuity_report_names_the_construct_the_body_wrote() {
    let quantified = vacuous("forall { let a: i32 = @; }");
    assert!(
        quantified.contains("spec function `f` is `forall`-quantified but asserts nothing"),
        "{quantified}"
    );
    assert!(
        quantified.contains("add an `assert` over the values it binds"),
        "{quantified}"
    );
    assert!(quantified.contains(VACUOUS), "{quantified}");

    let assumed = vacuous("{ assume { assert(1 > 0); } }");
    assert!(
        assumed.contains("spec function `f` claims nothing after its `assume` block"),
        "{assumed}"
    );
    assert!(
        assumed.contains("add an `assert` after the `assume` block"),
        "{assumed}"
    );

    let existential = vacuous("{ exists { let m: i32 = @; } }");
    assert!(
        existential.contains("spec function `f` claims nothing after its `exists` block"),
        "{existential}"
    );
    assert!(
        existential.contains("add an `assert` after the `exists` block"),
        "{existential}"
    );

    let computed = vacuous("-> i32 { return 0; }");
    assert!(
        computed.contains("spec function `f` only computes a value and states no property"),
        "{computed}"
    );
    assert!(
        computed.contains(
            "assert a property about the computation, or move the function out of the `spec` block"
        ),
        "{computed}"
    );
    assert!(computed.contains(VACUOUS), "{computed}");
}

/// A body that states a property keeps its obligation, whatever else it does. A
/// bare call statement counts: `HA_app_ok` claims the application is realized,
/// which is a property of the program rather than a value the specification
/// computes for itself.
#[test]
fn a_body_that_states_a_property_keeps_its_obligation() {
    assert_eq!(
        obligation_of("", "forall { let n: i32 = @; assert(n > 0); }"),
        imp(guard(0), nz(gts(local(0), i32c(0))))
    );
    let source = "spec S { fn f() -> i32 { assert(1 > 0); return 0; } }";
    assert_eq!(sole_obligation(&ok(source), "S"), nz(gts(i32c(1), i32c(0))));
    assert_eq!(
        obligation_of("fn side() -> i32 { return 1; }", "forall { side(); }"),
        HAssert::AppOk(HFnRef("side".to_string()), vec![])
    );
}

/// Nothing in the fold recognizes a tautology, so an `assert` contributes a
/// non-⊤ conjunct even for a claim that holds of every program. A body carrying
/// one is therefore never vacuous — the wording reserved for a tautology-only
/// specification has no free-function witness, and the case it describes is the
/// one a *method* reaches.
#[test]
fn a_tautological_assert_is_still_a_real_obligation() {
    assert_eq!(
        sole_obligation(&ok("spec S { fn f() { assert(1 > 0); } }"), "S"),
        nz(gts(i32c(1), i32c(0)))
    );
}

/// The vacuity check runs only on a function that is otherwise clean, so a body
/// already rejected for a construct with no encoding reports that construct — not
/// the `⊤` its abandoned translation happened to leave behind.
#[test]
fn an_earlier_diagnostic_suppresses_the_vacuity_report() {
    let looped = err("spec S { fn f() forall { loop { } } }");
    assert!(looped.contains("error[P002]"), "{looped}");
    assert!(!looped.contains("P010"), "{looped}");

    let compound = err("spec S { fn f(arr: [i32; 3]) forall { } }");
    assert!(compound.contains("error[P004]"), "{compound}");
    assert!(!compound.contains("P010"), "{compound}");
}

/// The verdict is per function. A vacuous sibling is reported by name without
/// taking the obligation of a function that does state a property with it, so the
/// remaining specification still says what it always said.
#[test]
fn a_vacuous_sibling_does_not_take_a_real_obligation_with_it() {
    let source = "spec S { fn a() forall { } fn b() forall { let n: i32 = @; assert(n > 0); } }";
    let ctx = type_check(source);
    let (map, diagnostics) = translate(&ctx);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[0].contains("error[P010]"), "{diagnostics:?}");
    assert!(
        diagnostics[0].contains("spec function `a`"),
        "{diagnostics:?}"
    );
    assert_eq!(
        obligation_named(&map, "S", "S.b"),
        imp(guard(0), nz(gts(local(0), i32c(0))))
    );
}

/// A specification method carries no obligation, so one that states a property
/// is reported rather than silently dropped — whether it claims through an
/// `assert` or through an inline non-deterministic block.
#[test]
fn a_plain_spec_method_that_states_a_property_is_reported() {
    for body in [
        "{ assert(1 > 0); }",
        "{ exists { let y: i32 = @; assert(y > 0); } }",
    ] {
        let rendered = err(&format!(
            "spec S {{ struct T {{ x: i32; fn m(self) {body} }} }}"
        ));
        assert!(
            rendered.contains("error[P009]"),
            "body `{body}`: {rendered}"
        );
        assert!(
            rendered.contains(
                "spec method `T.m` states a property, but a spec method carries no verification \
                 obligation — move the property into a `forall` spec function"
            ),
            "body `{body}`: {rendered}"
        );
    }
}

/// The helper role a free function lost is exactly the one a method keeps: a
/// method that only computes claims nothing, carries no obligation either way,
/// and is left alone.
#[test]
fn a_spec_method_that_only_computes_stays_a_silent_helper() {
    let map = ok("spec S { struct T { x: i32; fn m(self) -> i32 { return 1; } } }");
    assert!(
        map.is_empty(),
        "a spec method contributes no obligation, got {map:?}"
    );
}

/// A quantified method keeps the report it always had, naming the quantifier —
/// distinct wording from the one a plain method that claims a property gets, so
/// the two cases stay tellable apart.
#[test]
fn a_quantified_spec_method_keeps_its_own_report() {
    let source =
        "spec S { struct T { x: i32; fn m(self) forall { let y: i32 = @; assert(y > 0); } } }";
    let rendered = err(source);
    assert!(rendered.contains("error[P009]"), "{rendered}");
    assert!(
        rendered.contains(
            "spec method `T.m` is `forall`-quantified; a quantified spec method carries a proof \
             obligation that cannot yet be translated to a verification assertion — move the \
             property into a `forall` spec function"
        ),
        "{rendered}"
    );
}
