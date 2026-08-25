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
use inference_hassert::{
    HAssert, HBinop, HConst, HFnRef, HNumType, HRelop, HSpecMap, HTerm, ReachMeta, SpecKind,
};
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
/// diagnostics. The reachability plans are built by the same pre-scan production
/// code generation runs, so the pass sees exactly what it would see in a real
/// proof-mode build.
fn translate(ctx: &TypedContext) -> (HSpecMap, Vec<String>) {
    let buckets = buckets_of(ctx);
    let reach_plans = super::reach::plan_reachability_specs(ctx)
        .expect("the reachability pre-scan should accept every translation-test body");
    let (map, diagnostics) = super::translate_spec_fns(ctx, &buckets, &reach_plans);
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

/// Every function symbol an obligation applies, in first-encounter order with
/// duplicates removed — the names that must resolve against the emitted
/// module's name section.
fn applied_symbols(a: &HAssert) -> Vec<String> {
    fn walk_assert(a: &HAssert, acc: &mut Vec<String>) {
        match a {
            HAssert::True | HAssert::False => {}
            HAssert::Not(x) | HAssert::Ex(x) | HAssert::All(x) => walk_assert(x, acc),
            HAssert::And(l, r) | HAssert::Imp(l, r) | HAssert::Or(l, r) => {
                walk_assert(l, acc);
                walk_assert(r, acc);
            }
            HAssert::TermEq(l, r) => {
                walk_term(l, acc);
                walk_term(r, acc);
            }
            HAssert::HasType(t, _) | HAssert::Defined(t) => walk_term(t, acc),
            HAssert::AppOk(f, args) => {
                acc.push(f.0.clone());
                for arg in args {
                    walk_term(arg, acc);
                }
            }
        }
    }
    fn walk_term(t: &HTerm, acc: &mut Vec<String>) {
        match t {
            HTerm::Const(_) | HTerm::LVar(_) | HTerm::Local(_) => {}
            HTerm::App(f, args) => {
                acc.push(f.0.clone());
                for arg in args {
                    walk_term(arg, acc);
                }
            }
            HTerm::Binop(_, _, l, r) | HTerm::Relop(_, _, l, r) => {
                walk_term(l, acc);
                walk_term(r, acc);
            }
        }
    }
    let mut acc = Vec::new();
    walk_assert(a, &mut acc);
    acc.dedup();
    acc
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
fn all(a: HAssert) -> HAssert {
    HAssert::All(Box::new(a))
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

/// The bound a declaration at `decl_ty` puts on the term `x`, or `None` where
/// the declaration admits every value of the class its readouts ride in.
///
/// The per-type domain table this file checks emission against, and
/// deliberately spelled out from the primitive builders above rather than
/// routed through the pass's own resolver — see the harness note: an
/// expectation that asks production code what to expect cannot fail when
/// production code is wrong. Every test below that pins a domain restates
/// these rows literally; this helper is what the multi-slot expectations
/// assemble from, not a single source the tests defer to.
fn domain_of(decl_ty: &str, x: &HTerm) -> Option<HAssert> {
    let x = || x.clone();
    match decl_ty {
        "bool" => Some(nz(ltu(x(), i32c(2)))),
        "u8" => Some(nz(ltu(x(), i32c(256)))),
        "u16" => Some(nz(ltu(x(), i32c(65536)))),
        "i8" => Some(and(nz(les(i32c(-128), x())), nz(lts(x(), i32c(128))))),
        "i16" => Some(and(nz(les(i32c(-32768), x())), nz(lts(x(), i32c(32768))))),
        "i32" | "u32" | "i64" | "u64" => None,
        other => panic!("no declared domain is recorded here for `{other}`"),
    }
}

/// The one hypothesis an introduction of `x` declared at `decl_ty` contributes:
/// its typing guard conjoined with the values its declaration admits, or the
/// bare typing guard where the declaration admits the whole class.
fn hypothesis_of(decl_ty: &str, x: &HTerm) -> HAssert {
    let typing = hastype(x.clone(), guard_width(decl_ty));
    match domain_of(decl_ty, x) {
        Some(bound) => and(typing, bound),
        None => typing,
    }
}

/// The antecedent one slot declared at `decl_ty` sits under.
fn guard_of(decl_ty: &str, n: u32) -> HAssert {
    hypothesis_of(decl_ty, &local(n))
}

/// The antecedent the guard drain builds over `slots` in introduction order:
/// one right fold across the single hypothesis each slot contributes. That is
/// not the same tree as conjoining the slots pairwise, so the fold is spelled
/// out rather than assembled from per-slot antecedents.
fn guards_of(slots: &[(&str, u32)]) -> HAssert {
    slots
        .iter()
        .map(|(decl_ty, n)| guard_of(decl_ty, *n))
        .rev()
        .reduce(|acc, hypothesis| and(hypothesis, acc))
        .expect("every slot contributes at least its typing guard")
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
fn les(l: HTerm, r: HTerm) -> HTerm {
    rel(HNumType::I32, HRelop::LeS, l, r)
}
fn ges(l: HTerm, r: HTerm) -> HTerm {
    rel(HNumType::I32, HRelop::GeS, l, r)
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
    assert_eq!(
        *antecedent,
        guards_of(&[(decl_ty, 0), (decl_ty, 1)]),
        "both universal slots must state their declared width and domain"
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
            guard_of("bool", 0),
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
    assert_eq!(*antecedent, guard_of(decl_ty, 0));
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
            guard_of("bool", 0),
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
            and(guard(0), nz(ltu(local(0), i32c(3)))),
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

/// The declared type fixes both halves of a slot's hypothesis. The width: only
/// `i64`/`u64` guard at 64 bits, while bool, enums, and every sub-word integer
/// ride i32. And the value domain: every type but the four full widths admits
/// fewer values than its class holds, and says so.
#[test]
fn slot_guards_state_the_declared_width_and_domain() {
    for decl_ty in ["bool", "i8", "u8", "i16", "u16", "i32", "u32", "i64", "u64"] {
        let body = format!("forall {{ let x: {decl_ty} = @; assert(x == x); }}");
        let obligation = obligation_of("", &body);
        let HAssert::Imp(antecedent, _) = obligation else {
            panic!("expected a guarded implication for `{decl_ty}`, got {obligation:?}");
        };
        assert_eq!(
            *antecedent,
            guard_of(decl_ty, 0),
            "the hypothesis a `{decl_ty}` slot carries"
        );
    }
    // An enum slot is its i32 tag, below the variant count.
    let body = "forall { let c: Color = @; assert(c == Color::Red); }";
    assert_eq!(
        obligation_of("enum Color { Red, Green, Blue }", body),
        imp(
            and(guard(0), nz(ltu(local(0), i32c(3)))),
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
        HAssert::Not(inner) | HAssert::Ex(inner) | HAssert::All(inner) => {
            unguarded_reads(inner, guarded, out);
        }
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
fn p001_rejects_an_assume_bodied_spec_function() {
    let e = err("spec S { fn f() assume { let x: i32 = @; assert(x > 0); } } ");
    assert!(e.contains("error[P001]"), "{e}");
    assert!(
        e.contains("has an `assume` body") && e.contains("`assume` is not a quantifier"),
        "the message must explain why an assume body states no property: {e}"
    );
}

#[test]
fn p002_rejects_constructs_without_an_encoding() {
    let loop_src = "spec S { fn f() forall { let n: i32 = @; loop { assert(n > 0); } } }";
    assert!(err(loop_src).contains("error[P002]"));
    let unique_src = "spec S { fn f() forall { let n: i32 = @; unique { assert(n > 0); } } }";
    assert!(err(unique_src).contains("error[P002]"));
}

/// `loop` has its own message: the constructs sharing `error_no_encoding` have
/// no substitute to point at, while a loop's whole purpose — saying something
/// about every element — is exactly what quantifying says directly.
#[test]
fn p002_points_a_loop_at_the_quantifier_idiom() {
    let e = err("spec S { fn f() forall { let n: i32 = @; loop { assert(n > 0); } } }");
    assert!(
        e.contains(
            "`loop` has no encoding in the verification assertion language: a loop states a \
             property only through an invariant this translation cannot infer"
        ) && e.contains("constrain the index in an `assume` block"),
        "{e}"
    );
}

/// The shared `error_no_encoding` template still serves every other construct;
/// lifting `loop` out of it must not have reworded its neighbours.
#[test]
fn the_shared_no_encoding_template_is_unchanged() {
    let e = err("spec S { fn f() forall { let n: i32 = @; unique { assert(n > 0); } } }");
    assert!(
        e.contains(
            "`unique` block has no encoding in the verification assertion language; remove it \
             from the spec body or move the logic into an executable helper function"
        ),
        "{e}"
    );
}

#[test]
fn p003_rejects_reassignment() {
    let src = "spec S { fn f() forall { let a: i32 = @; let mut b: i32 = a; b = b + 1; assert(b > 0); } }";
    assert!(err(src).contains("error[P003]"));
}

/// The `P003` decision is permanent, so the message states a rule rather than a
/// gap: nothing in it should read as "not yet".
#[test]
fn p003_states_a_rule_rather_than_a_schedule() {
    let e = err(
        "spec S { fn f() forall { let a: i32 = @; let mut b: i32 = a; b = b + 1; assert(b > 0); } }",
    );
    assert!(
        e.contains(
            "reassignment has no place in a specification body: a specification names values, \
             not storage"
        ) && e.contains("bind a new `let` for the new value"),
        "{e}"
    );
    assert!(
        !e.contains("not supported") && !e.contains("not yet"),
        "a permanent rule must not be worded as a pending feature: {e}"
    );
}

/// A parameter of a supported aggregate shape now leaf-expands; what stays
/// `P004` is the out-of-surface shape — here an array of structs, the same
/// boundary A028 draws for the executable `@`.
#[test]
fn p004_rejects_an_out_of_surface_compound_parameter() {
    let src = "\
struct P { x: i32; }
spec S { fn f(ps: [P; 2]) forall { let n: i32 = @; assert(n > 0); } }
";
    let e = err(src);
    assert!(e.contains("error[P004]"), "{e}");
    assert!(
        e.contains(
            "type `[P; 2]` cannot appear in a specification term; a term is a bool, an \
             integer, or an enum value, and the only aggregates a specification names are \
             arrays of those at any rank and structs whose fields are those or one-dimensional \
             arrays of those"
        ),
        "P004 must name the whole representable surface, not the scalar part of it: {e}"
    );
}

/// The surface clause has to be exact about rank, because the two aggregate
/// kinds differ: an array of scalars nests to any depth, a struct field may be
/// a scalar or a one-dimensional array of scalars and no deeper. A struct with
/// a multidimensional array field is rejected, so the clause must not name that
/// field shape as legal.
#[test]
fn the_surface_clause_does_not_promise_a_multidimensional_struct_field() {
    let deep = err("\
struct Deep { grid: [[i32; 2]; 2]; tag: i32; }
spec S { fn f(d: Deep) forall { let n: i32 = @; assert(n > 0); } }
");
    assert!(deep.contains("error[P004]"), "{deep}");
    assert!(
        deep.contains("structs whose fields are those or one-dimensional arrays of those"),
        "{deep}"
    );

    // The rank claim the same clause makes for arrays is the permissive one,
    // and it is true: a rank-3 scalar array is in surface.
    let rank_three = "spec S { fn f() forall { let m: [[[i32; 2]; 2]; 2] = @; \
                      assert(m[0][0][0] >= m[0][0][0]); } }";
    let _ = ok(rank_three);
}

/// An aggregate read whole where a term is required is a different mistake from
/// an unrepresentable type, and says so. Passing an aggregate to a call is the
/// commonest way to arrive: the compiled callee takes a pointer, so the symbol
/// a `T_app` names could not be applied to leaves.
#[test]
fn an_aggregate_in_term_position_is_told_it_is_not_a_term() {
    let e = err("\
fn head(v: [i32; 2]) -> i32 { return v[0]; }
spec S { fn f() forall { let a: [i32; 2] = @; assert(head(a) == a[0]); } }
");
    assert!(e.contains("error[P004]"), "{e}");
    assert!(
        e.contains(
            "type `[i32; 2]` is an aggregate, and a term is one scalar value: a specification \
             names an aggregate by its scalar leaves rather than as a value of its own"
        ) && e.contains("name the component you mean"),
        "{e}"
    );
    assert!(
        !e.contains("the only aggregates a specification names"),
        "the surface enumeration would contradict this very rejection: {e}"
    );
}

#[test]
fn p005_rejects_unbound_extern_and_nondeterministic_callees() {
    let extern_src = "\
external fn ext(x: i32) -> i32;
spec S { fn f() forall { let a: i32 = @; assert(ext(a) == a); } }
";
    let e = err(extern_src);
    assert!(e.contains("error[P005]"), "{e}");
    assert!(
        e.contains("no `use … from` binding"),
        "the rejection must name what is missing — a binding, not the extern itself: {e}"
    );

    let nondet_src = "\
fn helper() -> i32 {
  let x: i32 = @;
  return x;
}
spec S { fn f() forall { let a: i32 = @; assert(helper() == a); } }
";
    assert!(err(nondet_src).contains("error[P005]"));
}

/// A *bound* `external fn` is a legitimate specification subject: the static
/// merge splices its body into the emitted module, so the obligation applies it
/// under the name the merge gives that body.
#[test]
fn a_bound_extern_applies_under_its_merged_symbol() {
    let map = ok("\
external fn double(x: i32) -> i32;
use { double } from mathlib;
spec S { fn f() forall { let a: i32 = @; assert(double(a) == a + a); } }
");
    let applied = applied_symbols(&sole_obligation(&map, "S"));
    assert_eq!(
        applied,
        vec!["mathlib.double".to_string()],
        "the obligation must apply the extern under the linker's merged-body name"
    );
}

/// An extern bound under a `::`-joined logical module keeps that module in its
/// symbol: the merged name is per source module, not per export field, because
/// two modules may export the same field.
#[test]
fn a_bound_extern_keeps_its_logical_module_in_the_symbol() {
    let map = ok("\
external fn hash(x: i32) -> i32;
use { hash } from crypto::digest;
spec S { fn f() forall { let a: i32 = @; assert(hash(a) == a); } }
");
    assert_eq!(
        applied_symbols(&sole_obligation(&map, "S")),
        vec!["crypto::digest.hash".to_string()]
    );
}

/// Resolution is by *declaration*, not by name. A spec-inner `external fn`
/// shadows a same-named bound one at file scope, and it is unbound — so the
/// call inside that spec is rejected, while the identical call in a sibling
/// spec resolves to the bound declaration.
///
/// A name-keyed lookup passes the first half of this and fails the second: it
/// would hand the inner declaration the outer one's origin and emit an
/// obligation naming a merged body the call does not reach. Reachable here
/// because this harness runs no analysis — through the full pipeline `A024`
/// rejects the call first.
#[test]
fn a_spec_inner_extern_shadows_the_bound_one_of_the_same_name() {
    let shadowed = "\
external fn probe(x: i32) -> i32;
use { probe } from sensors;
spec Inner {
  external fn probe(x: i32) -> i32;
  fn f() forall { let a: i32 = @; assert(probe(a) == a); }
}
";
    let e = err(shadowed);
    assert!(e.contains("error[P005]"), "{e}");
    assert!(
        e.contains("no `use … from` binding"),
        "the spec-inner declaration is the one in scope, and it is unbound: {e}"
    );

    let sibling = "\
external fn probe(x: i32) -> i32;
use { probe } from sensors;
spec Inner {
  external fn probe(x: i32) -> i32;
  fn g() forall { let a: i32 = @; assert(a == a + 0); }
}
spec Outer { fn f() forall { let a: i32 = @; assert(probe(a) == a); } }
";
    let map = ok(sibling);
    assert_eq!(
        applied_symbols(&sole_obligation(&map, "Outer")),
        vec!["sensors.probe".to_string()],
        "the sibling spec sees only the bound file-scope declaration"
    );
}

/// An `external fn` is visible only in the file that declares it, so a bare
/// name written in two files means two declarations and each file's obligation
/// names its own.
///
/// The scope key is `(defining file, enclosing spec)`, and this pins the file
/// half of it: without that component the index holds one `scale` for the whole
/// program, and one of the two specs states an obligation about the *other*
/// library — a body its call never reaches, which still resolves post-link and
/// elaborates under `coqc`, so nothing downstream would notice.
///
/// The two declarations are bound to different libraries because that is what
/// makes the mix-up visible at all: bound to the same one, the wrong answer and
/// the right answer are the same string. A local function of the same name
/// cannot serve as the witness — a top-level function sharing a bare name with a
/// top-level `external fn` anywhere in the program is rejected outright.
///
/// Code generation resolves this very program through this very index, so the
/// obligation and the emitted call name one function. That agreement is what
/// makes the assertions below worth having: were the two keyed differently, this
/// test would pin the correct reading of a call compiled as something else.
#[test]
fn an_extern_in_one_file_does_not_capture_a_sibling_files_call() {
    let ctx = type_check_multi(&[
        (
            vec![],
            "use side;\nexternal fn scale(x: i32) -> i32;\nuse { scale } from libA;\nspec \
             MainSpec { fn f() forall { let a: i32 = @; assert(scale(a) == a); } }\n",
        ),
        (
            vec!["side"],
            "external fn scale(x: i32) -> i32;\nuse { scale } from libB;\nspec SideSpec { fn f() \
             forall { let a: i32 = @; assert(scale(a) == a); } }\n",
        ),
    ]);
    let (map, diagnostics) = translate(&ctx);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        applied_symbols(&sole_obligation(&map, "MainSpec")),
        vec!["libA.scale".to_string()],
        "the entry file's spec names the declaration its own file bound"
    );
    assert_eq!(
        applied_symbols(&sole_obligation(&map, "side_SideSpec")),
        vec!["libB.scale".to_string()],
        "the sibling's spec names the declaration its own file bound"
    );
}

/// The invariant that lets callee resolution check externs before defined
/// functions: a spec-inner function may not shadow a top-level one of the same
/// name, external or not, so a name is one or the other and the order decides
/// nothing.
///
/// Pinned here rather than left to the type checker's own suite because it is
/// this pass that depends on it: were the rule relaxed, a file-scope extern
/// would hide the spec-sibling function that shadows it, and the obligation
/// would name a body the call does not reach.
#[test]
fn a_spec_inner_function_cannot_shadow_a_top_level_name() {
    for source in [
        "external fn probe(x: i32) -> i32;\nuse { probe } from sensors;\nspec S {\n  fn probe(x: \
         i32) -> i32 { return x; }\n  fn f() forall { let a: i32 = @; assert(probe(a) == a); }\n}",
        "fn probe(x: i32) -> i32 { return x; }\nspec S {\n  fn probe(x: i32) -> i32 { return x; }\n \
         fn f() forall { let a: i32 = @; assert(probe(a) == a); }\n}",
    ] {
        let parsed = inference_parser::parse(source);
        assert!(
            parsed.errors.is_empty(),
            "parse errors: {:?}",
            parsed.errors
        );
        let error = TypeCheckerBuilder::build_typed_context(parsed.arena)
            .err()
            .expect("a spec-inner function shadowing a top-level name must be rejected");
        assert!(
            error.to_string().contains("shadows a top-level function"),
            "the rejection must be the shadowing rule, got: {error}"
        );
    }
}

/// The second invariant the extern symbol rests on: an `external fn`
/// *declaration* is bound to at most one module.
///
/// An obligation names the module its declaration was bound to, and a call is
/// emitted against the import that same declaration registered. Both are
/// answers about a declaration, so both stay well defined exactly as long as a
/// *declaration* has one module — not as long as a *name* does. Two files may
/// each declare `scale` and bind it to a different library; the declarations
/// are distinct, so the program is legal and each obligation names its own
/// module. The rule is per file: one file naming one field from two modules is
/// still the conflict, and is covered by the binding pass's own tests.
#[test]
fn one_extern_declaration_is_bound_to_one_module() {
    let mut arena = AstArena::default();
    for (module_path, source) in [
        (
            "a",
            "external fn scale(x: i32) -> i32;\nuse { scale } from libA;\npub fn ua(x: i32) -> \
             i32 { return scale(x); }\n",
        ),
        (
            "b",
            "external fn scale(x: i32) -> i32;\nuse { scale } from libB;\npub fn ub(x: i32) -> \
             i32 { return scale(x); }\n",
        ),
    ] {
        let parsed = inference_parser::parse_into(arena, source, vec![module_path.to_string()]);
        assert!(
            parsed.errors.is_empty(),
            "parse errors: {:?}",
            parsed.errors
        );
        arena = parsed.arena;
    }
    let ctx = TypeCheckerBuilder::build_typed_context(arena)
        .expect("two files may each bind their own declaration to their own module")
        .typed_context();
    let index = ctx.extern_index();
    let in_a = index
        .lookup_top_level(&["a".to_string()], "scale")
        .expect("file `a` declares `scale`");
    let in_b = index
        .lookup_top_level(&["b".to_string()], "scale")
        .expect("file `b` declares `scale`");
    assert_ne!(
        in_a, in_b,
        "the fixture must declare two distinct `scale`s, or this test proves nothing"
    );
    assert_eq!(
        ctx.extern_origin_by_decl(in_a).map(|o| o.logical_module),
        Some("libA".to_string())
    );
    assert_eq!(
        ctx.extern_origin_by_decl(in_b).map(|o| o.logical_module),
        Some("libB".to_string())
    );
}

/// A bare statement call to a bound extern becomes `HA_app_ok` under the same
/// symbol — the void-result path through the same resolution.
///
/// Asserted structurally rather than by the applied-symbol list, because that
/// list cannot tell `HA_app_ok f τs` from a `T_app f τs` buried in some other
/// atom, and those are different claims: one says the call is defined, the
/// other names its result.
#[test]
fn a_bound_extern_statement_call_becomes_app_ok() {
    let map = ok("\
external fn emit(x: i32);
use { emit } from telemetry;
spec S { fn f() forall { let a: i32 = @; emit(a); } }
");
    assert_eq!(
        sole_obligation(&map, "S"),
        imp(
            guard(0),
            HAssert::AppOk(HFnRef("telemetry.emit".to_string()), vec![local(0)])
        )
    );
}

/// A `@` at a supported aggregate shape now leaf-expands; what stays `P008`
/// is the out-of-surface shape — an array of structs (`A028`'s boundary),
/// reachable here because this harness runs no analysis.
#[test]
fn p008_rejects_an_out_of_surface_compound_uzumaki() {
    let src = "\
struct P { x: i32; }
spec S { fn f() forall { let ps: [P; 2] = @; let n: i32 = @; assert(n > 0); } }
";
    let e = err(src);
    assert!(e.contains("error[P008]"), "{e}");
    // The universal wording names the *shape* restriction, not a missing
    // encoding: a compound `@` encodes now, so "has no assertion encoding"
    // would state a rule the language no longer has.
    assert!(
        e.contains(
            "uzumaki (@) over compound type `[P; 2]` quantifies a shape the assertion encoding \
             cannot take apart"
        ) && e.contains("structs whose fields are scalars or one-dimensional arrays of those"),
        "{e}"
    );
    assert!(!e.contains("has no assertion encoding"), "{e}");
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
    // An out-of-surface compound parameter (P004) and a loop (P002) in the
    // same body.
    let src = "\
struct P { x: i32; }
spec S { fn f(ps: [P; 2]) forall { let n: i32 = @; loop { } } }
";
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
                    layout: crate::MemoryLayout::default(),
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

    let compound = err("struct P { x: i32; }\nspec S { fn f(ps: [P; 2]) forall { } }");
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

/// Completes `spec S { struct T { x: i32; fn m(self) … } }` with `rest` — a
/// body, optionally preceded by a return type or a quantifier — giving the
/// method shape whose helper exemption the reports below delimit.
fn spec_method(rest: &str) -> String {
    format!("spec S {{ struct T {{ x: i32; fn m(self) {rest} }} }}")
}

/// The message a plain method that loses an assertion is reported with.
const METHOD_STATES_A_PROPERTY: &str = "spec method `T.m` states a property, but a spec method \
                                        carries no verification obligation — move the property \
                                        into a `forall` spec function";

/// Asserts that the method `T.m` written as `rest` is reported for losing an
/// assertion.
fn method_states_a_property(rest: &str) {
    let rendered = err(&spec_method(rest));
    assert!(
        rendered.contains("error[P009]"),
        "method `{rest}`: {rendered}"
    );
    assert!(
        rendered.contains(METHOD_STATES_A_PROPERTY),
        "method `{rest}`: {rendered}"
    );
}

/// Asserts that the method `T.m` written as `rest` is left alone.
fn method_is_a_silent_helper(rest: &str) {
    let map = ok(&spec_method(rest));
    assert!(
        map.is_empty(),
        "method `{rest}`: a spec method contributes no obligation, got {map:?}"
    );
}

/// A specification method carries no obligation, so an `assert` written in one
/// is dropped without a trace. It is reported instead, wherever the `assert`
/// sits: at the top of the body, or inside a non-deterministic block within it.
#[test]
fn a_plain_spec_method_that_states_a_property_is_reported() {
    method_states_a_property("{ assert(1 > 0); }");
    method_states_a_property("{ exists { let y: i32 = @; assert(y > 0); } }");
}

/// A plain method is reported only when an assertion is actually lost, so a
/// non-deterministic block that asserts nothing is left alone — however it is
/// written, wherever it sits, and whatever it binds. Writing `forall` is an
/// intent marker for wording a message, never on its own a stated property.
#[test]
fn a_plain_spec_method_whose_nondet_block_asserts_nothing_is_not_reported() {
    for rest in [
        "{ forall { } }",
        "{ exists { let y: i32 = @; } }",
        "{ assume { } }",
        "{ unique { } }",
        "{ forall { exists { } } }",
        "{ if self.x > 0 { forall { } } }",
    ] {
        method_is_a_silent_helper(rest);
    }
}

/// The exemption is the missing assertion, not the non-deterministic block: an
/// `assert` the block encloses is reported however deeply it nests, and so is
/// one that merely follows the block, or that sits in a block a branch guards —
/// the shapes the first marker alone points away from.
#[test]
fn an_assert_around_a_nondet_block_in_a_plain_spec_method_is_reported() {
    for rest in [
        "{ forall { assert(self.x > 0); } }",
        "{ forall { exists { assert(self.x > 0); } } }",
        "{ assume { assert(self.x > 0); } }",
        "{ forall { } assert(self.x > 0); }",
        "{ if self.x > 0 { forall { assert(self.x > 0); } } }",
        "{ if self.x > 0 { } else { forall { assert(self.x < 0); } } }",
    ] {
        method_states_a_property(rest);
    }
}

/// The helper role a free function lost is exactly the one a method keeps: a
/// method that only computes claims nothing, carries no obligation either way,
/// and is left alone.
#[test]
fn a_spec_method_that_only_computes_stays_a_silent_helper() {
    method_is_a_silent_helper("-> i32 { return 1; }");
}

/// A quantified method keeps the report it always had, naming the quantifier —
/// distinct wording from the one a plain method that claims a property gets, so
/// the two cases stay tellable apart. The quantifier is the obligation, so the
/// report does not wait for an `assert` the way a plain body's does.
#[test]
fn a_quantified_spec_method_keeps_its_own_report() {
    for rest in ["forall { let y: i32 = @; assert(y > 0); }", "forall { }"] {
        let rendered = err(&spec_method(rest));
        assert!(
            rendered.contains("error[P009]"),
            "method `{rest}`: {rendered}"
        );
        assert!(
            rendered.contains(
                "spec method `T.m` is `forall`-quantified; a quantified spec method carries a \
                 proof obligation that cannot yet be translated to a verification assertion — \
                 move the property into a `forall` spec function"
            ),
            "method `{rest}`: {rendered}"
        );
    }
}

// ----- 14. reachability obligations (exists/unique bodies) ----------------

/// The single entry of a spec that has exactly one, kind included.
fn sole_entry(map: &HSpecMap, spec: &str) -> inference_hassert::HSpecEntry {
    let entries = map.get(spec).unwrap_or_else(|| {
        panic!(
            "no spec `{spec}`; have {:?}",
            map.keys().collect::<Vec<_>>()
        )
    });
    assert_eq!(entries.len(), 1, "expected exactly one entry for `{spec}`");
    entries[0].clone()
}

/// Walks an assertion and fails on any `HA_has_type`: a reachability payload
/// denotes against the frame an actual execution reaches, where every slot
/// carries its runtime type, so a stated typing has no place in it.
fn assert_no_typing_guards(h: &HAssert) {
    match h {
        HAssert::True
        | HAssert::False
        | HAssert::TermEq(_, _)
        | HAssert::AppOk(_, _)
        | HAssert::Defined(_) => {}
        HAssert::HasType(t, ty) => {
            panic!(
                "a reachability payload must carry no typing guard, found HasType({t:?}, {ty:?})"
            )
        }
        HAssert::Not(inner) | HAssert::Ex(inner) | HAssert::All(inner) => {
            assert_no_typing_guards(inner);
        }
        HAssert::And(l, r) | HAssert::Imp(l, r) | HAssert::Or(l, r) => {
            assert_no_typing_guards(l);
            assert_no_typing_guards(r);
        }
    }
}

/// An `exists` body translates operationally: the entry parameter and the
/// named choice both read their own frame slots, with no binder and no typing
/// guard, and the entry carries the reachability kind with its metadata.
#[test]
fn an_exists_body_binds_its_choices_to_frame_slots() {
    let map = ok("spec S { fn f(x: i32) exists { let n: i32 = @; assert(n > x); } }");
    let entry = sole_entry(&map, "S");
    assert_eq!(entry.hassert, nz(gts(local(1), local(0))));
    assert_no_typing_guards(&entry.hassert);
    assert_eq!(
        entry.kind,
        SpecKind::Exists(ReachMeta {
            entry_arity: 1,
            visible_locs: vec![0, 1],
        })
    );
}

/// A `unique` body translates exactly like an `exists` one — only the kind
/// differs — and `==` takes the strict `term_eq` the existential path uses.
#[test]
fn a_unique_body_translates_like_exists_under_its_own_kind() {
    let map = ok("spec S { fn f() unique { let n: i32 = @; assert(n == 7); } }");
    let entry = sole_entry(&map, "S");
    assert_eq!(entry.hassert, teq(local(0), i32c(7)));
    assert_eq!(
        entry.kind,
        SpecKind::Unique(ReachMeta {
            entry_arity: 0,
            visible_locs: vec![0],
        })
    );
}

/// Nested `assume` and `exists` blocks are conjuncts of a reachability body:
/// their statements translate in the same mode, so the nested block's named
/// choice still reads its hoisted choice parameter (and joins `visible_locs`).
#[test]
fn nested_assume_and_exists_blocks_are_conjuncts_in_a_reach_body() {
    let map = ok("spec S {
        fn f(x: i32) exists {
          assume { assert(x > 0); }
          exists {
            let n: i32 = @;
            assert(n > x);
          }
          assert(x < 100);
        }
      }");
    let entry = sole_entry(&map, "S");
    assert_eq!(
        entry.hassert,
        and(
            nz(gts(local(0), i32c(0))),
            and(nz(gts(local(1), local(0))), nz(lts(local(0), i32c(100)))),
        )
    );
    assert_eq!(
        entry.kind,
        SpecKind::Exists(ReachMeta {
            entry_arity: 1,
            visible_locs: vec![0, 1],
        })
    );
}

/// An `if` in a reachability body is the strict disjunction of guarded
/// conjunctions the existential path builds, so a non-denoting condition
/// cannot fabricate a reached exit.
#[test]
fn a_reach_if_is_a_strict_disjunction_of_guarded_conjunctions() {
    let map = ok(
        "spec S { fn f(x: i32) exists { if x > 0 { assert(x == 1); } else { assert(x == 2); } } }",
    );
    let cond = || gts(local(0), i32c(0));
    assert_eq!(
        sole_entry(&map, "S").hassert,
        or(
            and(nz(cond()), teq(local(0), i32c(1))),
            and(eqz(cond()), teq(local(0), i32c(2))),
        )
    );
}

/// An anonymous call-argument `@` in an `exists` body reads its own choice
/// parameter — no `HA_ex` binder — and stays out of `visible_locs`: only what
/// the source names is part of the observable face.
#[test]
fn an_anonymous_choice_in_an_exists_body_reads_its_parameter() {
    let map = ok("fn g(v: i32) -> i32 { return v; }
        spec S { fn f(x: i32) exists { assert(g(@) == x); } }");
    let entry = sole_entry(&map, "S");
    assert_eq!(entry.hassert, teq(app("g", vec![local(1)]), local(0)));
    assert_eq!(
        entry.kind,
        SpecKind::Exists(ReachMeta {
            entry_arity: 1,
            visible_locs: vec![0],
        })
    );
}

/// A pure `let` is inlined as its term on the reachability path exactly as on
/// the universal one, occupies no payload slot, and stays out of
/// `visible_locs`.
#[test]
fn a_pure_let_is_inlined_and_stays_out_of_visible_locs() {
    let map = ok("spec S { fn f() exists { let n: i32 = @; let t: i32 = n + 1; assert(t > 0); } }");
    let entry = sole_entry(&map, "S");
    assert_eq!(
        entry.hassert,
        nz(gts(
            bin(HNumType::I32, HBinop::Add, local(0), i32c(1)),
            i32c(0)
        ))
    );
    assert_eq!(
        entry.kind,
        SpecKind::Exists(ReachMeta {
            entry_arity: 0,
            visible_locs: vec![0],
        })
    );
}

/// `HA_ex` survives in a reachability payload for exactly one purpose: the
/// pinned witness of a short-circuit `&&`/`||`, whose machinery is
/// mode-independent. The `@`s themselves never bind one.
#[test]
fn a_short_circuit_witness_keeps_its_binder_in_a_reach_body() {
    let map = ok("spec S { fn f(x: i32) exists { let ok: bool = x == 0 || x > 5; assert(ok); } }");
    let entry = sole_entry(&map, "S");
    let taken = || nz(eqs(local(0), i32c(0)));
    let skipped = || eqz(eqs(local(0), i32c(0)));
    assert_eq!(
        entry.hassert,
        ex(and(
            or(
                and(taken(), teq(lvar(0), i32c(1))),
                and(skipped(), teq(lvar(0), gts(local(0), i32c(5)))),
            ),
            nz(lvar(0)),
        ))
    );
    assert_no_typing_guards(&entry.hassert);
}

/// One spec holding all three kinds: the forall sibling keeps its universal
/// entry (typing guard included), and each reachability sibling carries its
/// own kind — the partition downstream selects theorems by.
#[test]
fn mixed_kind_siblings_carry_their_own_kinds() {
    let map = ok("spec S {
        fn a() forall { let n: i32 = @; assert(n >= n); }
        fn e() exists { let n: i32 = @; assert(n > 0); }
        fn u() unique { let n: i32 = @; assert(n == 1); }
      }");
    let entries = map.get("S").expect("spec S");
    assert_eq!(entries.len(), 3, "one entry per free function");
    let by_symbol = |name: &str| {
        entries
            .iter()
            .find(|e| e.fn_symbol == HFnRef(name.to_string()))
            .unwrap_or_else(|| panic!("no entry `{name}`"))
    };
    let meta = || ReachMeta {
        entry_arity: 0,
        visible_locs: vec![0],
    };
    assert_eq!(by_symbol("S.a").kind, SpecKind::Forall);
    assert_eq!(by_symbol("S.e").kind, SpecKind::Exists(meta()));
    assert_eq!(by_symbol("S.u").kind, SpecKind::Unique(meta()));
    assert!(
        matches!(by_symbol("S.a").hassert, HAssert::Imp(_, _)),
        "the universal sibling keeps its guarded shape"
    );
    assert_no_typing_guards(&by_symbol("S.e").hassert);
    assert_no_typing_guards(&by_symbol("S.u").hassert);
}

/// The vacuity verdict applies to reachability bodies unchanged: an `exists`
/// body that asserts nothing collapses to `⊤` and is reported as `P010` — not
/// `P001`, which no longer covers `exists`.
#[test]
fn a_vacuous_exists_body_is_p010_not_p001() {
    let e = err("spec S { fn f() exists { let n: i32 = @; } }");
    assert!(e.contains("error[P010]"), "{e}");
    assert!(
        e.contains("is `exists`-quantified but asserts nothing"),
        "{e}"
    );
    assert!(
        !e.contains("P001"),
        "P001 must not fire for an exists body: {e}"
    );
}

/// A spec body calling an `exists`/`unique` sibling is `P011` — and
/// specifically not `P005`, whose non-deterministic-body arm sits *after* the
/// reachability carve-out on the resolve path and would otherwise swallow it
/// with the wrong wording and remedy.
#[test]
fn p011_rejects_a_call_to_a_reachability_spec_function() {
    let from_forall = err("spec S {
        fn e() exists { let n: i32 = @; assert(n > 0); }
        fn f() forall { let a: i32 = @; e(); assert(a >= a); }
      }");
    assert!(from_forall.contains("error[P011]"), "{from_forall}");
    assert!(
        from_forall.contains("call to `e` is not allowed")
            && from_forall.contains("`exists`-quantified spec function"),
        "{from_forall}"
    );
    assert!(
        !from_forall.contains("P005"),
        "P011 must pre-empt the non-det-body P005: {from_forall}"
    );

    let from_exists = err("spec S {
        fn u() unique { let n: i32 = @; assert(n == 1); }
        fn f() exists { u(); assert(1 == 1); }
      }");
    assert!(from_exists.contains("error[P011]"), "{from_exists}");
    assert!(
        from_exists.contains("is a `unique`-quantified spec function"),
        "{from_exists}"
    );
    assert!(!from_exists.contains("P005"), "{from_exists}");
}

/// Every message that names a quantifier takes the article the word is spoken
/// with — `unique` reads as a consonant despite its leading vowel letter, so a
/// leading-vowel test would write "an `unique`" at each of these sites.
///
/// Both quantifiers are pinned at all four families. The article helper's
/// fallback is `"a"`, so only the `exists` arm can regress silently, and P011
/// is the family where such a regression would otherwise leave the suite green.
#[test]
fn a_named_quantifier_takes_its_spoken_article() {
    let call = err("spec S {
        fn u() unique { let n: i32 = @; assert(n == 1); }
        fn f() exists { u(); assert(1 == 1); }
      }");
    let uzumaki =
        err("spec S { fn f() unique { let a: [i32; 2] = @; let n: i32 = @; assert(n > 0); } }");
    let parameter = err("spec S { fn f(a: [i32; 2]) unique { let c: i32 = @; assert(c > 0); } }");
    let nested = err("spec S { fn f() unique { let n: i32 = @; forall { assert(n > 0); } } }");
    for message in [&call, &uzumaki, &parameter, &nested] {
        assert!(message.contains("a `unique`-quantified"), "{message}");
        assert!(!message.contains("an `unique`"), "{message}");
    }

    let exists_call = err("spec S {
        fn e() exists { let n: i32 = @; assert(n > 0); }
        fn f() forall { let a: i32 = @; e(); assert(a >= a); }
      }");
    let exists_uzumaki =
        err("spec S { fn f() exists { let a: [i32; 2] = @; let n: i32 = @; assert(n > 0); } }");
    let exists_parameter =
        err("spec S { fn f(a: [i32; 2]) exists { let c: i32 = @; assert(c > 0); } }");
    let exists_nested =
        err("spec S { fn f() exists { let n: i32 = @; forall { assert(n > 0); } } }");
    for message in [
        &exists_call,
        &exists_uzumaki,
        &exists_parameter,
        &exists_nested,
    ] {
        assert!(message.contains("an `exists`-quantified"), "{message}");
        assert!(!message.contains(" a `exists`"), "{message}");
    }
}

/// A compound `@` in *call-argument* position of a reachability body is the
/// second spelling of the same mistake, and reads the same: the pre-scan plans
/// only scalar choices, so an unplanned argument lands on its own emit path.
///
/// Analysis rule A014 rejects this spelling first whenever analysis runs, but
/// the corpus and unit pipelines go parse → typecheck → codegen without it, so
/// the translator's own rejection is the only guard there — the same reason
/// `P014` exists beside A037.
#[test]
fn a_compound_argument_uzumaki_in_a_reach_body_reads_like_the_let_form() {
    let src = "fn g(v: [i32; 2]) -> i32 { return v[0]; }
        spec S { fn f() exists { assert(g(@) == 0); } }";
    let e = err(src);
    assert!(e.contains("error[P008]"), "{e}");
    assert!(
        e.contains(
            "uzumaki (@) over compound type `[i32; 2]` cannot be a reachability choice: this \
             is an `exists`-quantified spec function"
        ) && e.contains("state the property in a `forall`-bodied spec function"),
        "{e}"
    );
}

/// The same rejection reaches the term translator through the same resolve
/// path. A reachability callee is void by construction (the no-return rule),
/// so no value-demanding position can hold one in a type-correct program; the
/// term path's reachable door is a parenthesized expression statement, which
/// is read as a term for its diagnostics — the callee resolves before its
/// result is classified, so the carve-out fires there exactly as it does for
/// a bare statement call.
#[test]
fn p011_fires_on_the_term_path_too() {
    let e = err("spec S {
        fn e() exists { let n: i32 = @; assert(n > 0); }
        fn f() forall { let a: i32 = @; (e()); assert(a >= a); }
      }");
    assert!(e.contains("error[P011]"), "{e}");
    assert!(!e.contains("P005"), "{e}");
}

/// An anonymous `@` argument is rejected in a `unique` body (`P012`): it is
/// excluded from the source-visible observation, so distinct choices nothing
/// names would collapse into one observation — while the same shape in an
/// `exists` body stays accepted, where the exclusion cannot change whether
/// the observation set is non-empty.
#[test]
fn p012_rejects_an_anonymous_choice_in_a_unique_body_only() {
    let unique = err("fn g(v: i32) -> i32 { return v; }
        spec S { fn f() unique { let n: i32 = @; assert(g(@) == n); } }");
    assert!(unique.contains("error[P012]"), "{unique}");
    assert!(
        unique.contains("anonymous `@` argument in a `unique` spec function")
            && unique.contains("bind it first"),
        "{unique}"
    );

    let exists = ok("fn g(v: i32) -> i32 { return v; }
        spec S { fn f() exists { let n: i32 = @; assert(g(@) == n); } }");
    assert_eq!(
        sole_entry(&exists, "S").hassert,
        teq(app("g", vec![local(1)]), local(0))
    );
}

/// A compound `@` in a reachability body keeps `P008` but explains the
/// reachability-specific impossibility: this quantifier's obligation is about
/// one actual run, whose choices arrive one scalar at a time. The wording must
/// name the quantifier — the identical declaration leaf-expands in a `forall`
/// body — and must offer the `forall`-bodied alternative.
#[test]
fn p008_speaks_reachability_for_a_compound_choice_in_a_reach_body() {
    let e =
        err("spec S { fn f() exists { let arr: [i32; 2] = @; let n: i32 = @; assert(n > 0); } }");
    assert!(e.contains("error[P008]"), "{e}");
    assert!(
        e.contains(
            "uzumaki (@) over compound type `[i32; 2]` cannot be a reachability choice: this \
             is an `exists`-quantified spec function"
        ) && e.contains("each choice arrives as one scalar parameter of that run")
            && e.contains("state the property in a `forall`-bodied spec function"),
        "{e}"
    );
}

/// The same rejection in a `unique` body names *its* quantifier, with the
/// article the word takes when spoken.
#[test]
fn p008_names_the_unique_quantifier_in_a_unique_body() {
    let e =
        err("spec S { fn f() unique { let arr: [i32; 2] = @; let n: i32 = @; assert(n > 0); } }");
    assert!(
        e.contains("this is a `unique`-quantified spec function"),
        "{e}"
    );
}

/// A nested `forall` block keeps `P007` and a nested `unique` block keeps
/// `P002` inside a reachability body, exactly as inside an `exists` block.
#[test]
fn nested_forall_and_unique_blocks_keep_their_rejections_in_a_reach_body() {
    let forall = err("spec S { fn f() exists { let n: i32 = @; forall { assert(n > 0); } } }");
    assert!(forall.contains("error[P007]"), "{forall}");

    let unique = err("spec S { fn f() exists { let n: i32 = @; unique { assert(n > 0); } } }");
    assert!(unique.contains("error[P002]"), "{unique}");
}

/// The highest-risk invariant, checked end to end: the payload's `T_local`
/// indices must equal the compiled function's actual parameter layout. The
/// same source runs through real proof-mode code generation; the emitted type
/// section (via its name entries) pins where each parameter sits, and the
/// obligation attached to the same output must read exactly those indices —
/// entry parameter at 0, named choice at 1, anonymous choice at 2.
#[test]
fn reach_payload_slots_match_the_compiled_parameter_layout() {
    let source = "\
fn g(a: i32, b: bool) -> i32 {
  if b {
    return a;
  }
  return 0;
}

spec S {
  fn f(x: i32) exists {
    let c: i64 = @;
    assume { assert(c > 0); }
    assert(g(x, @) == x);
  }
}
";
    let ctx = type_check(source);
    let output = crate::codegen(
        &ctx,
        "align",
        crate::CodegenOptions {
            target: crate::Target::Wasm32,
            mode: CompilationMode::Proof,
            opt_level: crate::Target::Wasm32.default_opt_level(),
            features: crate::EmitFeatures::default(),
            layout: crate::MemoryLayout::default(),
        },
    )
    .expect("proof-mode codegen should succeed");

    // The compiled layout: the declared parameter, then the choices in source
    // order, i64/i32 by declared class.
    let wat = wasmprinter::print_bytes(output.wasm()).expect("WAT print should succeed");
    let flat = wat.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains("(param $x i32) (param $c i64) (param $__choice1 i32)"),
        "the compiled signature must be entry + named choice + anonymous choice:\n{flat}"
    );

    // The payload attached to the same output reads exactly those indices.
    let entry = sole_entry(output.hspecs(), "S");
    assert_eq!(
        entry.hassert,
        and(
            nz(rel(HNumType::I64, HRelop::GtS, local(1), i64c(0))),
            teq(app("g", vec![local(0), local(2)]), local(0)),
        )
    );
    assert_eq!(
        entry.kind,
        SpecKind::Exists(ReachMeta {
            entry_arity: 1,
            visible_locs: vec![0, 1],
        })
    );
}

// ----- 15. aggregate values (leaf encoding) -------------------------------

/// The issue's acceptance shape: a compound `@` binds one guarded universal
/// slot per scalar leaf, and a constant-index read is that leaf's term.
#[test]
fn aggregate_uzumaki_binds_one_guarded_slot_per_leaf() {
    assert_eq!(
        obligation_of("", "forall { let a: [i32; 3] = @; assert(a[0] <= a[0]); }"),
        imp(
            and(guard(0), and(guard(1), guard(2))),
            nz(rel(HNumType::I32, HRelop::LeS, local(0), local(0)))
        )
    );
}

/// A multi-rank scalar array enumerates row-major — the same order the
/// runtime unrolling walks — so `m[i][j]` is leaf `i * cols + j`.
#[test]
fn multi_rank_array_leaves_enumerate_row_major() {
    assert_eq!(
        obligation_of(
            "",
            "forall { let m: [[i32; 2]; 2] = @; assert(m[1][0] > m[0][1]); }"
        ),
        imp(
            and(guard(0), and(guard(1), and(guard(2), guard(3)))),
            nz(gts(local(2), local(1)))
        )
    );
}

/// Struct leaves follow field-layout order (declaration order), each guarded
/// at its own width, and a 1-D scalar-array field contributes its elements in
/// place — the mixed chain `r.row[k]` lands on the right leaf.
#[test]
fn struct_uzumaki_leaves_follow_field_layout_order_and_widths() {
    let prelude = "struct Rec { lo: i32; wide: i64; row: [i32; 2]; }";
    assert_eq!(
        obligation_of(
            prelude,
            "forall { let r: Rec = @; assert(r.wide > 0); assert(r.row[1] >= r.row[0]); }"
        ),
        imp(
            and(
                guard(0),
                and(hastype(local(1), HNumType::I64), and(guard(2), guard(3)))
            ),
            and(
                nz(rel(HNumType::I64, HRelop::GtS, local(1), i64c(0))),
                nz(rel(HNumType::I32, HRelop::GeS, local(3), local(2)))
            )
        )
    );
}

/// A compound parameter takes one slot and one guard per leaf, ahead of the
/// parameters declared after it, so slot numbers stay source-aligned.
#[test]
fn compound_parameter_binds_leaf_slots_before_later_parameters() {
    let source = "spec S { fn all_ge(a: [i32; 3], b: i32) forall { assert(a[0] >= b); } }";
    assert_eq!(
        sole_obligation(&ok(source), "S"),
        imp(
            and(guard(0), and(guard(1), and(guard(2), guard(3)))),
            nz(rel(HNumType::I32, HRelop::GeS, local(0), local(3)))
        )
    );
}

/// An ignored compound parameter consumes and guards its leaf slots exactly
/// like a named one — uniformity keeps later slot numbers source-aligned.
#[test]
fn ignored_compound_parameter_still_consumes_and_guards_its_leaf_slots() {
    let source = "spec S { fn f(_: [i32; 2], b: i32) forall { assert(b > 0); } }";
    assert_eq!(
        sole_obligation(&ok(source), "S"),
        imp(
            and(guard(0), and(guard(1), guard(2))),
            nz(gts(local(2), i32c(0)))
        )
    );
}

/// A struct parameter leaf-expands like a struct `@`.
#[test]
fn struct_parameter_binds_leaf_slots() {
    let source = "\
struct Pt { x: i32; y: i32; }
spec S { fn f(p: Pt) forall { assert(p.x == p.y); } }
";
    assert_eq!(
        sole_obligation(&ok(source), "S"),
        imp(and(guard(0), guard(1)), nz(eqs(local(0), local(1))))
    );
}

/// A cross-module struct type behaves exactly like an unqualified one: the
/// `lib::Pair` spelling resolves through the qualified-path lookup and
/// leaf-expands, both as a parameter and as a `@`.
#[test]
fn cross_module_struct_types_leaf_expand_like_unqualified_ones() {
    let ctx = type_check_multi(&[
        (
            vec![],
            "use lib;\nspec S {\n  fn f(p: lib::Pair) forall {\n    assert(p.a == p.b);\n  }\n  fn g() forall {\n    let q: lib::Pair = @;\n    assert(q.b >= q.a);\n  }\n}\n",
        ),
        (vec!["lib"], "pub struct Pair { a: i32; b: i32; }\n"),
    ]);
    let (map, diagnostics) = translate(&ctx);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        obligation_named(&map, "S", "S.f"),
        imp(and(guard(0), guard(1)), nz(eqs(local(0), local(1))))
    );
    assert_eq!(
        obligation_named(&map, "S", "S.g"),
        imp(
            and(guard(0), guard(1)),
            nz(rel(HNumType::I32, HRelop::GeS, local(1), local(0)))
        )
    );
}

/// An aggregate `@` inside an `assume`/`exists` context binds one nested
/// `HA_ex` per leaf, levels in enumeration order.
#[test]
fn aggregate_uzumaki_in_an_exists_block_binds_nested_ex_binders() {
    let body = "forall { let n: i32 = @; exists { let a: [i32; 2] = @; assert(a[0] == n && a[1] == a[0]); } }";
    assert_eq!(
        obligation_of("", body),
        imp(
            guard(0),
            ex(ex(and(teq(lvar(1), local(0)), teq(lvar(0), lvar(1)))))
        )
    );
}

/// An array literal is a value tree: a constant-index read of it is the
/// element's own translated term.
#[test]
fn array_literal_elements_resolve_by_constant_index() {
    assert_eq!(
        obligation_of(
            "",
            "forall { let v: [i32; 3] = [1, 2, 3]; assert(v[1] == 2); }"
        ),
        nz(eqs(i32c(2), i32c(2)))
    );
}

/// A struct literal's fields reorder from source order to field-layout order;
/// access is by name, so the reordering is unobservable.
#[test]
fn struct_literal_fields_resolve_by_name_across_reordering() {
    let prelude = "struct Pt { x: i32; y: i32; }";
    assert_eq!(
        obligation_of(
            prelude,
            "forall { let p: Pt = Pt { y: 5, x: 4 }; assert(p.x < p.y); }"
        ),
        nz(lts(i32c(4), i32c(5)))
    );
}

/// Nested literals are part of the enclosing introduction and resolve through
/// constant-index chains.
#[test]
fn nested_array_literal_resolves_through_a_constant_chain() {
    assert_eq!(
        obligation_of(
            "",
            "forall { let m: [[i32; 2]; 2] = [[1, 2], [3, 4]]; assert(m[1][0] == 3); }"
        ),
        nz(eqs(i32c(3), i32c(3)))
    );
}

/// A block-local `const` at an aggregate type binds a value tree like a pure
/// `let`.
#[test]
fn aggregate_const_binds_a_value_tree() {
    assert_eq!(
        obligation_of(
            "",
            "forall { const C: [i32; 2] = [7, 8]; assert(C[0] == 7); }"
        ),
        nz(eqs(i32c(7), i32c(7)))
    );
}

/// An aggregate copy (`let b: T = a;`) clones the bound value tree —
/// value-copy semantics make the pure inlining exact.
#[test]
fn aggregate_copy_clones_the_bound_value() {
    assert_eq!(
        obligation_of(
            "",
            "forall { let a: [i32; 2] = @; let b: [i32; 2] = a; assert(b[1] == a[1]); }"
        ),
        imp(and(guard(0), guard(1)), nz(eqs(local(1), local(1))))
    );
}

/// Aggregate `==` in assertion position is the leafwise conjunction of
/// per-leaf `term_eq`, in leaf enumeration order.
#[test]
fn aggregate_equality_is_a_leafwise_conjunction() {
    assert_eq!(
        obligation_of(
            "",
            "forall { let a: [i32; 2] = @; let b: [i32; 2] = @; assert(a == b); }"
        ),
        imp(
            and(guard(0), and(guard(1), and(guard(2), guard(3)))),
            and(teq(local(0), local(2)), teq(local(1), local(3)))
        )
    );
}

/// Aggregate `!=` is the De Morgan dual — a disjunction of negated per-leaf
/// equalities — and a negated `==` flips to the same shape.
#[test]
fn aggregate_inequality_and_negated_equality_are_the_leafwise_dual() {
    let expected = imp(
        and(guard(0), and(guard(1), and(guard(2), guard(3)))),
        or(not(teq(local(0), local(2))), not(teq(local(1), local(3)))),
    );
    assert_eq!(
        obligation_of(
            "",
            "forall { let a: [i32; 2] = @; let b: [i32; 2] = @; assert(a != b); }"
        ),
        expected
    );
    assert_eq!(
        obligation_of(
            "",
            "forall { let a: [i32; 2] = @; let b: [i32; 2] = @; assert(!(a == b)); }"
        ),
        expected
    );
}

/// An aggregate compared against a literal reads the literal's leaf terms.
#[test]
fn aggregate_equality_against_a_literal_uses_its_leaf_terms() {
    let prelude = "struct Pt { x: i32; y: i32; }";
    assert_eq!(
        obligation_of(
            prelude,
            "forall { let p: Pt = @; assert(p == Pt { x: 1, y: 2 }); }"
        ),
        imp(
            and(guard(0), guard(1)),
            and(teq(local(0), i32c(1)), teq(local(1), i32c(2)))
        )
    );
}

/// Aggregate comparison in *term* position stays rejected: an aggregate is
/// not a term.
#[test]
fn aggregate_comparison_in_term_position_stays_rejected() {
    let src = "\
spec S { fn f() forall { let a: [i32; 2] = @; let b: [i32; 2] = @; let t: bool = a == b; assert(t); } }
";
    let e = err(src);
    assert!(e.contains("error[P004]"), "{e}");
}

/// A folded-constant in-range index resolves like a literal one.
#[test]
fn folded_constant_index_resolves_in_range() {
    assert_eq!(
        obligation_of(
            "",
            "forall { let a: [i32; 3] = @; const K: i32 = 2; assert(a[K] > 0); }"
        ),
        imp(
            and(guard(0), and(guard(1), guard(2))),
            nz(gts(local(2), i32c(0)))
        )
    );
}

/// A folded-constant out-of-bounds index is `P014` — the same fact A037
/// states for the direct-literal spelling, at the path A037's pattern cannot
/// see.
#[test]
fn p014_rejects_a_folded_constant_out_of_bounds_index() {
    let src =
        "spec S { fn f() forall { let a: [i32; 3] = @; const K: i32 = 5; assert(a[K] > 0); } }";
    let e = err(src);
    assert!(e.contains("error[P014]"), "{e}");
    assert!(
        e.contains("array index 5 is out of bounds for array of length 3; valid indices are 0..3"),
        "{e}"
    );
}

/// An in-range unsigned constant index resolves too: reading the fold at the
/// index's own signedness must not cost the positive path.
#[test]
fn folded_unsigned_constant_index_resolves_in_range() {
    assert_eq!(
        obligation_of(
            "",
            "forall { let a: [i32; 3] = @; const K: u32 = 2; assert(a[K] > 0); }"
        ),
        imp(
            and(guard(0), and(guard(1), guard(2))),
            nz(gts(local(2), i32c(0)))
        )
    );
}

/// `P014` names the index the source wrote. An unsigned constant rides in the
/// term language at its signed bit pattern, and reporting that pattern would
/// name a number found nowhere in the program.
#[test]
fn p014_names_an_unsigned_index_as_the_source_wrote_it() {
    let out_of_bounds = |declaration: &str| {
        err(&format!(
            "spec S {{ fn f() forall {{ let a: [i32; 3] = @; {declaration} \
             assert(a[K] > 0); }} }}"
        ))
    };
    // All ones at 32 bits, which reads back as `-1` when read signed.
    let max = out_of_bounds("const K: u32 = 4294967295;");
    assert!(max.contains("error[P014]"), "{max}");
    assert!(
        max.contains("array index 4294967295 is out of bounds for array of length 3"),
        "{max}"
    );
    // One past the signed maximum, where a signed reading shows as `i32::MIN`
    // instead — a different wrong number, so the two cases discriminate.
    let past_signed_max = out_of_bounds("const K: u32 = 2147483648;");
    assert!(
        past_signed_max.contains("array index 2147483648 is out of bounds for array of length 3"),
        "{past_signed_max}"
    );
    // The widest index a program can name still reports whole.
    let widest = out_of_bounds("const K: u64 = 18446744073709551615;");
    assert!(
        widest.contains("array index 18446744073709551615 is out of bounds for array of length 3"),
        "{widest}"
    );
    // An unsigned index small enough to read the same either way.
    let small = out_of_bounds("const K: u32 = 5;");
    assert!(
        small.contains("array index 5 is out of bounds for array of length 3"),
        "{small}"
    );
    // A signed index keeps reading signed.
    let signed = out_of_bounds("const K: i32 = 7;");
    assert!(
        signed.contains("array index 7 is out of bounds for array of length 3"),
        "{signed}"
    );
}

/// This harness runs no analysis, so a direct-literal OOB index (A037's case)
/// reaches the translator too; `P014` is the only guard on this path.
#[test]
fn p014_also_guards_the_direct_literal_spelling_on_no_analysis_paths() {
    let src = "spec S { fn f() forall { let a: [i32; 3] = @; assert(a[5] > 0); } }";
    let e = err(src);
    assert!(e.contains("error[P014]"), "{e}");
}

/// An index computed from constants is as statically certain as one written
/// out, so it is folded and reported the same way. Left symbolic it would be
/// defined by cases against a closed term nothing satisfies — an unprovable
/// goal in place of a diagnostic.
#[test]
fn p014_rejects_an_out_of_bounds_arithmetic_index() {
    let e = err("spec S { fn f() forall { let a: [i32; 2] = @; assert(a[1 + 1] == 0); } }");
    assert!(e.contains("error[P014]"), "{e}");
    assert!(
        e.contains("array index 2 is out of bounds for array of length 2; valid indices are 0..2"),
        "{e}"
    );
}

/// The same for arithmetic over a named constant, which A037 cannot see even
/// with analysis on.
#[test]
fn p014_rejects_an_out_of_bounds_index_computed_from_a_constant() {
    let e = err(
        "spec S { fn f() forall { let a: [i32; 2] = @; const K: i32 = 1; assert(a[K + 1] == 0); } }",
    );
    assert!(e.contains("error[P014]"), "{e}");
    assert!(
        e.contains("array index 2 is out of bounds for array of length 2; valid indices are 0..2"),
        "{e}"
    );
}

/// An in-range arithmetic index descends to its element like any other
/// constant — the fold must not cost the positive path a real term and hand
/// back a case split instead.
#[test]
fn an_in_range_arithmetic_index_descends_to_its_element() {
    assert_eq!(
        obligation_of("", "forall { let a: [i32; 3] = @; assert(a[0 + 1] > 0); }"),
        imp(
            and(guard(0), and(guard(1), guard(2))),
            nz(gts(local(1), i32c(0)))
        )
    );
}

/// Arithmetic at an unsigned index type folds at that type's own reading, so
/// an in-range result descends and an out-of-range one is named as the source
/// computes it.
#[test]
fn an_unsigned_arithmetic_index_folds_at_its_own_signedness() {
    assert_eq!(
        obligation_of(
            "",
            "forall { let a: [i32; 3] = @; const K: u32 = 1; assert(a[K + 1] > 0); }"
        ),
        imp(
            and(guard(0), and(guard(1), guard(2))),
            nz(gts(local(2), i32c(0)))
        )
    );
    let e = err(
        "spec S { fn f() forall { let a: [i32; 3] = @; const K: u32 = 2; \
         assert(a[K * 2] > 0); } }",
    );
    assert!(e.contains("error[P014]"), "{e}");
    assert!(
        e.contains("array index 4 is out of bounds for array of length 3"),
        "{e}"
    );
}

/// Arithmetic the source's own width would wrap is not folded: the symbolic
/// path carries that wrap faithfully, and guessing an unwrapped number would
/// name an index the program never computes.
#[test]
fn arithmetic_that_wraps_its_width_is_left_to_the_case_split() {
    let source = "forall { let a: [i32; 2] = @; const K: u32 = 4294967295; \
                  assert(a[K + 1] == a[0]); }";
    // `K + 1` wraps to `0` at `u32`, which is in range — so no `P014`, and the
    // index is defined by cases over a closed term the assertion language
    // evaluates at the same width.
    let obligation = obligation_of("", source);
    let index = bin(HNumType::I32, HBinop::Add, i32c(-1), i32c(1));
    let definition = element_def(&index, &lvar(0), &[local(0), local(1)]);
    assert_eq!(
        obligation,
        imp(
            and(guard(0), guard(1)),
            ex(and(definition, nz(eqs(lvar(0), local(0)))))
        )
    );
}

/// An operation with no reading as a number stays unfolded rather than being
/// guessed at: a bitwise operator names a bit pattern, not a number the fold
/// could resolve to an element. A division by zero never arrives here to be
/// folded — the type checker rejects it — so the fold's guard against it is
/// defensive only.
#[test]
fn an_undecidable_index_operation_is_left_to_the_case_split() {
    let shift = obligation_of(
        "",
        "forall { let a: [i32; 2] = @; assert(a[1 << 1] == a[0]); }",
    );
    let index = bin(HNumType::I32, HBinop::Shl, i32c(1), i32c(1));
    assert_eq!(
        shift,
        imp(
            and(guard(0), guard(1)),
            ex(and(
                element_def(&index, &lvar(0), &[local(0), local(1)]),
                nz(eqs(lvar(0), local(0)))
            ))
        )
    );
}

// ----- 15a. the cumulative leaf budget (P013) ----------------------------

/// A single introduction past the cap is `P013`, reported from the type
/// before anything is materialized.
#[test]
fn p013_rejects_a_single_oversized_uzumaki() {
    let src = "spec S { fn f() forall { let big: [i32; 65] = @; assert(big[0] > 0); } }";
    let e = err(src);
    assert!(e.contains("error[P013]"), "{e}");
    assert!(
        e.contains(
            "uzumaki (@) over compound type `[i32; 65]` quantifies 65 scalar leaves, and this \
             specification already quantifies 0 of the 64 one function may hold"
        ),
        "{e}"
    );
}

/// The budget is a per-function running total: introductions each under the
/// cap still cross it together, and the report lands on the crossing
/// introduction with the cumulative context — never on the encoder backstop.
#[test]
fn p013_reports_the_crossing_introduction_cumulatively() {
    let src = "spec S { fn f(a: [i32; 40], b: [i32; 40]) forall { assert(a[0] == b[0]); } }";
    let ctx = type_check(src);
    let (_, diagnostics) = translate(&ctx);
    assert_eq!(
        diagnostics.len(),
        1,
        "the sentinel keeps later reads silent: {diagnostics:?}"
    );
    assert!(diagnostics[0].contains("error[P013]"), "{diagnostics:?}");
    assert!(
        diagnostics[0].contains(
            "parameter `b` of type `[i32; 40]` contributes 40 scalar leaves, and this \
             specification already quantifies 40 of the 64 one function may hold"
        ),
        "{diagnostics:?}"
    );
}

/// A literal introduction counts against the same budget, with its own
/// remedy wording — and its own reason: a literal's leaves are constants, so
/// the message names the nesting they cost rather than the quantified
/// variables and typing guards they never create.
#[test]
fn p013_counts_literal_introductions_against_the_same_budget() {
    let src = "\
spec S { fn f() forall { let a: [i32; 60] = @; let v: [i32; 8] = [1, 2, 3, 4, 5, 6, 7, 8]; assert(v[0] == a[0]); } }
";
    let ctx = type_check(src);
    let (_, diagnostics) = translate(&ctx);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[0].contains("error[P013]"), "{diagnostics:?}");
    assert!(
        diagnostics[0].contains(
            "this `[i32; 8]` literal has 8 scalar leaves, and this specification already \
             quantifies 60 of the 64 one function may hold: each leaf becomes a term of its \
             own, a comparison against the value nests one conjunct per leaf, and the \
             assertion encoding caps how deeply one obligation may nest; build a smaller \
             value, or state the property over the elements it reads"
        ),
        "{diagnostics:?}"
    );
    assert!(
        !diagnostics[0].contains("quantified variable"),
        "a literal's leaves bind no variables: {diagnostics:?}"
    );
}

/// Through the whole code generator, a budget overrun is the named `P013`
/// rejection, never the encoder's `HspecTreeTooDeep` backstop — the failure
/// the cumulative cap exists to preempt.
#[test]
fn budget_overrun_never_reaches_the_encoder_backstop() {
    let source = "spec S { fn f(a: [i32; 64], b: [i32; 64], c: [i32; 64], d: [i32; 64]) \
                  forall { assert(a[0] == b[0]); } }"
        .to_string();
    let err = proof_codegen(source).expect_err("an over-budget spec must fail code generation");
    match err {
        CodegenError::UntranslatableSpec(details) => {
            assert!(details.contains("error[P013]"), "{details}");
        }
        other => panic!("expected UntranslatableSpec carrying P013, got: {other:?}"),
    }
}

/// Exactly the cap in one function still translates — the budget bounds the
/// guard chain, and 64 guards fit the encoder with room for the claim.
#[test]
fn a_full_budget_introduction_still_translates() {
    let source = "spec S { fn f(a: [i32; 64]) forall { assert(a[0] == a[63]); } }".to_string();
    let size = proof_codegen(source)
        .expect("a spec at exactly the leaf budget must survive the whole code generator");
    assert!(size > 0, "code generation produced an empty module");
}

// ----- 15b. sentinels, out-of-surface shapes, kept rejections -------------

/// One rejected aggregate yields exactly one diagnostic: every later read of
/// the sentinel resolves silently instead of cascading into further errors.
#[test]
fn a_rejected_aggregate_is_reported_exactly_once() {
    let src = "\
struct P { x: i32; }
spec S { fn f() forall { let ps: [P; 2] = @; assert(ps[0].x > 0 && ps[1].x > 0); } }
";
    let ctx = type_check(src);
    let (_, diagnostics) = translate(&ctx);
    assert_eq!(
        diagnostics.len(),
        1,
        "the sentinel must keep reads of a rejected aggregate silent: {diagnostics:?}"
    );
    assert!(diagnostics[0].contains("error[P008]"), "{diagnostics:?}");
}

/// The same one-mistake-one-message discipline for an out-of-surface
/// parameter.
#[test]
fn a_rejected_parameter_is_reported_exactly_once() {
    let src = "\
struct P { x: i32; }
spec S { fn f(ps: [P; 2]) forall { assert(ps[0].x == 1); } }
";
    let ctx = type_check(src);
    let (_, diagnostics) = translate(&ctx);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[0].contains("error[P004]"), "{diagnostics:?}");
}

/// A struct containing a struct field is out of the surface on every path —
/// `@` (P008) and parameter (P004) — the boundary A027 draws for the
/// executable unrolling.
#[test]
fn struct_in_struct_stays_rejected_on_every_path() {
    let prelude = "struct In { v: i32; }\nstruct Out { a: In; }\n";
    let via_uzumaki = err(&format!(
        "{prelude}spec S {{ fn f() forall {{ let o: Out = @; assert(o.a.v > 0); }} }}"
    ));
    assert!(via_uzumaki.contains("error[P008]"), "{via_uzumaki}");
    let via_parameter = err(&format!(
        "{prelude}spec S {{ fn f(o: Out) forall {{ assert(o.a.v > 0); }} }}"
    ));
    assert!(via_parameter.contains("error[P004]"), "{via_parameter}");
}

/// The `::`-qualified spelling of a struct is rejected exactly like the
/// unqualified spelling of the same struct: an out-of-surface `@` is `P008`
/// under both, because one spelling-total classifier answers for every way a
/// struct type can be named. A partial classifier here would hand the
/// cross-module spelling the `P004` the byte-identical single-file shape does
/// not get.
#[test]
fn a_qualified_struct_uzumaki_is_rejected_like_the_unqualified_one() {
    let ctx = type_check_multi(&[
        (
            vec![],
            "use geom;\nspec S {\n  fn f() forall {\n    let o: geom::Outer = @;\n    \
             assert(o.a.v > 0);\n  }\n}\n",
        ),
        (
            vec!["geom"],
            "pub struct In { v: i32; }\npub struct Outer { a: In; }\n",
        ),
    ]);
    let (_, diagnostics) = translate(&ctx);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[0].contains("error[P008]"), "{diagnostics:?}");
    assert!(
        diagnostics[0].contains(
            "uzumaki (@) over compound type `geom::Outer` quantifies a shape the assertion \
             encoding cannot take apart"
        ),
        "{diagnostics:?}"
    );
}

/// The call-argument spelling of a compound reachability choice classifies the
/// same way as the `let` spelling, for every way a struct type can be named.
/// The two sites read the type from different places — the `let` from its
/// declaration, the argument from the checker's record for the `@` — so a
/// classifier that agreed only on the unqualified spelling would hand the
/// cross-module one the term-surface `P004` instead of the reachability `P008`.
#[test]
fn a_qualified_struct_argument_uzumaki_in_a_reach_body_is_rejected_like_the_let_form() {
    let ctx = type_check_multi(&[
        (
            vec![],
            "use geom;\nfn take(o: geom::Outer) -> i32 { return o.a.v; }\nspec S {\n  \
             fn f() exists {\n    assert(take(@) == 0);\n  }\n}\n",
        ),
        (
            vec!["geom"],
            "pub struct In { v: i32; }\npub struct Outer { a: In; }\n",
        ),
    ]);
    let (_, diagnostics) = translate(&ctx);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[0].contains("error[P008]"), "{diagnostics:?}");
    // The rendered type is pinned, not just the code: the two sites read the
    // type from different places, so a classifier that agreed on the code
    // while the argument form rendered a bare `Outer` would still be two
    // messages for one mistake.
    assert!(
        diagnostics[0].contains(
            "uzumaki (@) over compound type `geom::Outer` cannot be a reachability choice"
        ),
        "{diagnostics:?}"
    );
}

/// An out-of-surface declared type keeps its right-hand side's own `P002`:
/// the aggregate binding translates the literal as a term, and a literal has
/// no term encoding. Both bindings that take that path — the `let` and the
/// block-local `const` — are pinned, because routing either shape-miss
/// through the non-scalar diagnostic would silently downgrade the message to
/// `P004` with the rest of the suite still green.
#[test]
fn an_out_of_surface_literal_keeps_the_pre_existing_p002() {
    let one_p002 = |binding: &str| {
        let src = format!(
            "struct P {{ x: i32; }}\nspec S {{ fn f() forall {{ {binding} \
             assert(ps[0].x == 1); }} }}\n"
        );
        let ctx = type_check(&src);
        let (_, diagnostics) = translate(&ctx);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].contains("error[P002]"), "{diagnostics:?}");
        assert!(
            diagnostics[0].contains(
                "an array literal has no encoding in the verification assertion language"
            ),
            "{diagnostics:?}"
        );
    };
    one_p002("let ps: [P; 2] = [P { x: 1 }, P { x: 2 }];");
    one_p002("const ps: [P; 2] = [P { x: 1 }, P { x: 2 }];");
}

/// The other way an out-of-surface literal is reached — as an operand of an
/// aggregate comparison, where no declared type screens it first — states the
/// shape restriction rather than borrowing the shared no-encoding template. A
/// literal of a supported shape encodes now, so "has no encoding" would name a
/// rule the language dropped, and the template's "move the logic into an
/// executable helper" remedy dead-ends on a compound result.
#[test]
fn an_out_of_surface_literal_operand_states_the_shape_restriction() {
    let src = "\
struct P { x: i32; }
spec S { fn f(a: [P; 2]) forall { assert(a == [P { x: 1 }, P { x: 2 }]); } }
";
    let ctx = type_check(src);
    let (_, diagnostics) = translate(&ctx);
    // Two independent mistakes, two messages: `P004` for the out-of-surface
    // parameter and `P002` for the literal. The count is the assertion that
    // matters — this is the one body exercising the sentinel discipline with
    // two mistakes at once, so a parameter that started cascading into the
    // comparison would show up here as a third message and nowhere else.
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    let joined = diagnostics.join("\n");
    assert!(joined.contains("error[P004]"), "{joined}");
    assert!(joined.contains("error[P002]"), "{joined}");
    assert!(
        joined.contains("an array literal of this shape has no assertion encoding")
            && joined.contains("build the components you need as separate values"),
        "{joined}"
    );
}

/// A struct with a multidimensional-array field is out of the surface too
/// (A027 permits only 1-D scalar-array fields).
#[test]
fn struct_with_multidim_array_field_stays_rejected() {
    let src = "\
struct M { g: [[i32; 2]; 2]; }
spec S { fn f(m: M) forall { assert(m.g[0][0] > 0); } }
";
    let e = err(src);
    assert!(e.contains("error[P004]"), "{e}");
}

/// A field-less struct type-checks (A045 rejects it only under analysis, and
/// this harness runs none), and an empty leaf list would silently ⊤-collapse
/// an introduction — so the zero-leaf shape is classified out of the surface
/// and keeps the pre-existing rejections instead.
#[test]
fn a_zero_leaf_struct_is_out_of_the_surface_on_every_path() {
    let prelude = "struct E { }\n";
    let via_parameter = err(&format!(
        "{prelude}spec S {{ fn f(e: E) forall {{ let n: i32 = @; assert(n > 0); }} }}"
    ));
    assert!(via_parameter.contains("error[P004]"), "{via_parameter}");
    let via_uzumaki = err(&format!(
        "{prelude}spec S {{ fn f() forall {{ let e: E = @; let n: i32 = @; assert(n > 0); }} }}"
    ));
    assert!(via_uzumaki.contains("error[P008]"), "{via_uzumaki}");
}

/// A zero-length array never reaches the translator at all: the type checker
/// rejects the type, which is what keeps the other zero-leaf spelling
/// unreachable.
#[test]
fn a_zero_length_array_is_rejected_by_the_type_checker() {
    let parsed = inference_parser::parse("spec S { fn f(a: [i32; 0]) forall { assert(1 > 0); } }");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert!(
        TypeCheckerBuilder::build_typed_context(parsed.arena).is_err(),
        "a zero-length array type must be rejected before translation"
    );
}

/// Reachability mode keeps both rejections: a compound parameter stays
/// `P004` and a compound `@` stays `P008` (reachability wording). The
/// parameter rejection is load-bearing alignment, not conservatism: the
/// downstream judgment reads the k-th choice at frame slot `entry_arity + k`,
/// so every declared parameter must keep costing exactly one payload slot —
/// leaf expansion would misalign every choice after it.
#[test]
fn reach_mode_keeps_compound_parameter_and_uzumaki_rejections() {
    let param =
        err("spec S { fn f(a: [i32; 2], x: i32) exists { let c: i32 = @; assert(x == c); } }");
    assert!(param.contains("error[P004]"), "{param}");
    assert!(
        param.contains(
            "parameter `a` of type `[i32; 2]` cannot appear in an `exists`-quantified spec \
             function"
        ) && param.contains("is one pointer local"),
        "the reach-mode P004 must name the quantifier, since the same declaration \
         leaf-expands in a `forall` body: {param}"
    );

    let uzumaki = err("spec S { fn f(x: i32) exists { let a: [i32; 2] = @; assert(x > 0); } }");
    assert!(uzumaki.contains("error[P008]"), "{uzumaki}");
    assert!(
        uzumaki.contains("cannot be a reachability choice"),
        "the reachability P008 wording must stay unchanged: {uzumaki}"
    );
}

/// A `@` bound after a compound parameter takes the next slot past the
/// parameter's leaves — source alignment holds across the expansion.
#[test]
fn uzumaki_after_a_compound_parameter_takes_the_next_leaf_slot() {
    let source = "spec S { fn f(a: [i32; 2]) forall { let n: i32 = @; assert(n > a[1]); } }";
    assert_eq!(
        sole_obligation(&ok(source), "S"),
        imp(
            and(guard(0), and(guard(1), guard(2))),
            nz(gts(local(2), local(1)))
        )
    );
}

/// A short-circuit witness inside a literal element scopes over the rest of
/// the block through the same `scoped_over_rest` path a pure `let`'s witness
/// takes: the binder wraps everything that can read the bound aggregate, and
/// the slot guards pending at the binding drain around it.
#[test]
fn witness_inside_a_literal_element_scopes_over_the_rest() {
    let body = "forall { let p: bool = @; let q: bool = @; \
                let flags: [bool; 2] = [p && q, p]; assert(flags[0] == flags[1]); }";
    let witness_def = or(
        and(nz(local(0)), teq(lvar(0), local(1))),
        and(eqz(local(0)), teq(lvar(0), i32c(0))),
    );
    assert_eq!(
        obligation_of("", body),
        imp(
            guards_of(&[("bool", 0), ("bool", 1)]),
            ex(and(witness_def, nz(eqs(lvar(0), local(0)))))
        )
    );
}

// ----- 15c. non-constant index access (bounded iteration) -----------------

fn ltu(l: HTerm, r: HTerm) -> HTerm {
    rel(HNumType::I32, HRelop::LtU, l, r)
}

/// The constraint a non-constant index pins its element with: the unsigned
/// range bound as the *first* conjunct, then one implication per element.
/// Built independently of the pass, so a change to either half shows up as a
/// tree mismatch rather than as a silently different obligation.
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
    and(nz(ltu(index.clone(), i32c(extent))), cases)
}

/// A non-constant index binds one witness for the element, defined by the
/// index's unsigned range and one case per element of the array.
#[test]
fn a_non_constant_index_pins_its_element_by_cases() {
    let body = "forall { let a: [i32; 2] = @; let i: i32 = @; assert(a[i] > 0); }";
    let definition = element_def(&local(2), &lvar(0), &[local(0), local(1)]);
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), and(guard(1), guard(2))),
            ex(and(definition, nz(gts(lvar(0), i32c(0)))))
        )
    );
}

/// The range bound leads the definition. A reader of a failing goal meets the
/// condition under which the element is defined before the case analysis that
/// says which element it is, so the ordering is pinned on its own rather than
/// only inside a whole-tree compare.
#[test]
fn the_range_bound_is_the_first_conjunct_of_the_definition() {
    let body = "forall { let a: [i32; 3] = @; let i: i32 = @; assert(a[i] > 0); }";
    let HAssert::Imp(_, consequent) = obligation_of("", body) else {
        panic!("a body with slot guards states them as an antecedent");
    };
    let HAssert::Ex(atom) = *consequent else {
        panic!("a non-constant index binds a witness");
    };
    let HAssert::And(definition, _) = *atom else {
        panic!("the witness definition is conjoined with the claim");
    };
    let HAssert::And(range, _) = *definition else {
        panic!("the definition is the range bound and the case split");
    };
    assert_eq!(*range, nz(ltu(local(3), i32c(3))));
}

/// The issue's bounded-iteration acceptance shape, end to end through the
/// translator: an array `@`, an index `@`, a range `assume`, and a claim about
/// the element at that index. Each of the two reads binds its own witness,
/// both pinned to the same element.
#[test]
fn the_bounded_iteration_idiom_translates() {
    let body = "forall { let a: [i32; 3] = @; let i: i32 = @; \
                assume { assert(0 <= i && i < 3); } assert(a[i] == a[i]); }";
    let leaves = [local(0), local(1), local(2)];
    // Both witnesses read as de Bruijn index 0 inside their own binder.
    let definition = || element_def(&local(3), &lvar(0), &leaves);
    let filter = and(
        nz(rel(HNumType::I32, HRelop::LeS, i32c(0), local(3))),
        nz(lts(local(3), i32c(3))),
    );
    assert_eq!(
        obligation_of("", body),
        imp(
            and(
                guard(0),
                and(guard(1), and(guard(2), and(guard(3), filter)))
            ),
            ex(and(
                definition(),
                ex(and(definition(), nz(eqs(lvar(1), lvar(0)))))
            ))
        )
    );
}

/// Constant steps of a chain descend eagerly, so `m[1][j]` splits over the
/// already-selected row alone — one case per element of that row, not per
/// element of the matrix.
#[test]
fn constant_steps_descend_before_the_non_constant_one() {
    let body = "forall { let m: [[i32; 2]; 2] = @; let j: i32 = @; assert(m[1][j] > 0); }";
    let definition = element_def(&local(4), &lvar(0), &[local(2), local(3)]);
    assert_eq!(
        obligation_of("", body),
        imp(
            and(
                guard(0),
                and(guard(1), and(guard(2), and(guard(3), guard(4))))
            ),
            ex(and(definition, nz(gts(lvar(0), i32c(0)))))
        )
    );
}

/// A constant step *after* the non-constant one applies inside the split:
/// `m[i][0]` is one case per row, each case naming that row's first element —
/// not a split whose result is indexed again.
#[test]
fn a_constant_step_after_the_non_constant_one_applies_inside_the_split() {
    let body = "forall { let m: [[i32; 2]; 2] = @; let i: i32 = @; assert(m[i][0] > 0); }";
    let definition = element_def(&local(4), &lvar(0), &[local(0), local(2)]);
    assert_eq!(
        obligation_of("", body),
        imp(
            and(
                guard(0),
                and(guard(1), and(guard(2), and(guard(3), guard(4))))
            ),
            ex(and(definition, nz(gts(lvar(0), i32c(0)))))
        )
    );
}

/// A struct field selects the array before the index splits it.
#[test]
fn a_field_step_precedes_the_non_constant_index() {
    let prelude = "struct Rec { lo: i32; row: [i32; 2]; }";
    let body = "forall { let r: Rec = @; let k: i32 = @; assert(r.row[k] > 0); }";
    let definition = element_def(&local(3), &lvar(0), &[local(1), local(2)]);
    assert_eq!(
        obligation_of(prelude, body),
        imp(
            and(guard(0), and(guard(1), and(guard(2), guard(3)))),
            ex(and(definition, nz(gts(lvar(0), i32c(0)))))
        )
    );
}

/// A literal's elements are constants, and a non-constant index over them
/// splits on those constants — the encoding does not care where a leaf came
/// from.
#[test]
fn a_non_constant_index_reads_a_literal_by_cases() {
    let body = "forall { let v: [i32; 2] = [7, 8]; let i: i32 = @; assert(v[i] > 0); }";
    let definition = element_def(&local(0), &lvar(0), &[i32c(7), i32c(8)]);
    assert_eq!(
        obligation_of("", body),
        imp(guard(0), ex(and(definition, nz(gts(lvar(0), i32c(0))))))
    );
}

/// Two non-constant indices in one chain are rejected: the split would be the
/// product of the two extents, and one obligation carries one split per chain.
#[test]
fn two_non_constant_indices_in_one_chain_are_rejected() {
    let src = "spec S { fn f() forall { let m: [[i32; 2]; 2] = @; let i: i32 = @; \
               let j: i32 = @; assert(m[i][j] > 0); } }";
    let e = err(src);
    assert!(e.contains("error[P002]"), "{e}");
    assert!(
        e.contains(
            "an access chain with more than one non-constant index has no assertion encoding: \
             each non-constant index defines the element by cases, and one obligation supports \
             one such case split per chain; make all but one index constant, or assert over the \
             constant-index elements directly"
        ),
        "{e}"
    );
}

/// A chain that ends on an aggregate is rejected with its own message: a case
/// split pins one value, and an aggregate element would need one binder per
/// leaf of the selected sub-tree. The remedy names the shape that works, since
/// indexing itself does encode and moving the access into a helper would only
/// meet `P004` on the aggregate argument.
#[test]
fn a_non_constant_index_onto_an_aggregate_is_rejected() {
    let src = "spec S { fn f() forall { let m: [[i32; 2]; 2] = @; let i: i32 = @; \
               let row: [i32; 2] = m[i]; assert(row[0] > 0); } }";
    let e = err(src);
    assert!(e.contains("error[P002]"), "{e}");
    assert!(
        e.contains(
            "an access chain whose non-constant index selects an aggregate has no assertion \
             encoding: only a scalar leaf can be named by cases, and every candidate element here \
             would itself be an aggregate — there is no single term to define; index through to a \
             scalar (`m[i][0]`), or make the index constant"
        ),
        "{e}"
    );
}

/// The constraint rides into the arm that evaluates the access, exactly like
/// any other witness constraint: in the right operand of `&&` it joins the
/// conjunct the source only evaluates when the left one holds.
#[test]
fn a_non_constant_index_in_a_conjunction_rides_into_its_arm() {
    let body = "forall { let a: [i32; 2] = @; let i: i32 = @; assert(i == 0 && a[i] > 0); }";
    let definition = element_def(&local(2), &lvar(0), &[local(0), local(1)]);
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), and(guard(1), guard(2))),
            ex(and(
                nz(eqs(local(2), i32c(0))),
                and(definition, nz(gts(lvar(0), i32c(0))))
            ))
        )
    );
}

/// The same in the right operand of `||`, where the arm is the one the source
/// reaches only when the left disjunct fails.
#[test]
fn a_non_constant_index_in_a_disjunction_rides_into_its_arm() {
    let body = "forall { let a: [i32; 2] = @; let i: i32 = @; assert(i > 1 || a[i] > 0); }";
    let definition = element_def(&local(2), &lvar(0), &[local(0), local(1)]);
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), and(guard(1), guard(2))),
            ex(or(
                nz(gts(local(2), i32c(1))),
                and(definition, nz(gts(lvar(0), i32c(0))))
            ))
        )
    );
}

/// In falsiness position the same movement happens through the De Morgan
/// dual: negating `x || a[i] > 0` puts the access in the second conjunct.
#[test]
fn a_non_constant_index_moves_with_its_arm_in_falsiness_position() {
    let body = "forall { let a: [i32; 2] = @; let i: i32 = @; assert(!(i > 1 || a[i] > 0)); }";
    let definition = element_def(&local(2), &lvar(0), &[local(0), local(1)]);
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), and(guard(1), guard(2))),
            ex(and(
                eqz(gts(local(2), i32c(1))),
                and(definition, eqz(gts(lvar(0), i32c(0))))
            ))
        )
    );
}

/// In *both* polarities the definition is **conjoined** with the claim, never
/// made its antecedent. That is what makes an out-of-range index refute the
/// atom rather than discharge it vacuously: the range bound is demanded
/// alongside the claim, so no index outside `0..N` satisfies the existential.
/// The element is defined only where it exists — a definedness rule, not a
/// mirror of any runtime check.
#[test]
fn an_out_of_range_index_refutes_the_atom_in_both_polarities() {
    let definition = element_def(&local(2), &lvar(0), &[local(0), local(1)]);
    let claim_shape = |source_assert: &str, claim: HAssert| {
        let body = format!("forall {{ let a: [i32; 2] = @; let i: i32 = @; {source_assert} }}");
        assert_eq!(
            obligation_of("", &body),
            imp(
                and(guard(0), and(guard(1), guard(2))),
                ex(and(definition.clone(), claim))
            )
        );
    };
    claim_shape("assert(a[i] > 0);", nz(gts(lvar(0), i32c(0))));
    claim_shape("assert(!(a[i] > 0));", eqz(gts(lvar(0), i32c(0))));
}

/// Inside an `assume` block the access translates the same way; the block's
/// existential statement semantics change what the atom becomes, not how the
/// element is defined.
#[test]
fn a_non_constant_index_under_assume_keeps_its_definition() {
    let body = "forall { let a: [i32; 2] = @; let i: i32 = @; \
                assume { assert(a[i] > 0); } assert(i >= 0); }";
    let definition = element_def(&local(2), &lvar(0), &[local(0), local(1)]);
    assert_eq!(
        obligation_of("", body),
        imp(
            and(
                guard(0),
                and(
                    guard(1),
                    and(guard(2), ex(and(definition, nz(gts(lvar(0), i32c(0))))))
                )
            ),
            nz(rel(HNumType::I32, HRelop::GeS, local(2), i32c(0)))
        )
    );
}

/// A pure `let` of an element scopes the witness over the rest of the block,
/// where the inlined term is read — the `scoped_over_rest` path a
/// short-circuit witness already takes.
#[test]
fn an_element_bound_by_a_pure_let_scopes_over_the_rest() {
    let body = "forall { let a: [i32; 2] = @; let i: i32 = @; let x: i32 = a[i]; \
                assert(x > 0); }";
    let definition = element_def(&local(2), &lvar(0), &[local(0), local(1)]);
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), and(guard(1), guard(2))),
            ex(and(definition, nz(gts(lvar(0), i32c(0)))))
        )
    );
}

/// In an existential context the leaves are logical variables rather than
/// slots, and the element's own binder takes the level past them.
#[test]
fn a_non_constant_index_in_an_exists_block_splits_over_lvar_leaves() {
    let body = "forall { let n: i32 = @; exists { let a: [i32; 2] = @; let i: i32 = @; \
                assert(a[i] == n); } }";
    // Inside the four binders the array's leaves read as indices 3 and 2, the
    // index variable as 1, and the element's own witness as 0.
    let definition = element_def(&lvar(1), &lvar(0), &[lvar(3), lvar(2)]);
    assert_eq!(
        obligation_of("", body),
        imp(
            guard(0),
            ex(ex(ex(ex(and(definition, teq(lvar(0), local(0)))))))
        )
    );
}

/// The index's own numeric class carries into the range bound and the case
/// equalities, so a wider index type stays well-typed. (Analysis rule A019
/// rejects a 64-bit index before this in a real build; this harness runs no
/// analysis, which is exactly why the translator must still be total here.)
#[test]
fn the_index_class_carries_into_the_range_bound() {
    let body = "forall { let a: [i32; 2] = @; let i: i64 = @; assert(a[i] > 0); }";
    let index = || local(2);
    let definition = and(
        nz(rel(HNumType::I64, HRelop::LtU, index(), i64c(2))),
        HAssert::and(
            imp(teq(index(), i64c(0)), teq(lvar(0), local(0))),
            imp(teq(index(), i64c(1)), teq(lvar(0), local(1))),
        ),
    );
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), and(guard(1), hastype(local(2), HNumType::I64))),
            ex(and(definition, nz(gts(lvar(0), i32c(0)))))
        )
    );
}

/// An index carries its own signedness into the comparison the source writes,
/// while the element's range bound is unsigned whatever the index type is.
///
/// Nothing downstream can catch a wrong choice here: the type guard records
/// only the width, so `u32` and `i32` indices are indistinguishable in the
/// payload apart from these operator spellings, and the range reasoning is
/// sound only when the source comparison and the range bound agree about which
/// values are in range.
#[test]
fn an_index_comparison_carries_the_index_types_signedness() {
    let unsigned = obligation_of(
        "",
        "forall { let a: [i32; 2] = @; let i: u32 = @; assert(i > 1 || a[i] > 0); }",
    );
    let signed = obligation_of(
        "",
        "forall { let a: [i32; 2] = @; let i: i32 = @; assert(i > 1 || a[i] > 0); }",
    );
    let element = |guard_of_index: HTerm| {
        ex(HAssert::or(
            nz(guard_of_index),
            and(
                element_def(&local(2), &lvar(0), &[local(0), local(1)]),
                nz(gts(lvar(0), i32c(0))),
            ),
        ))
    };
    assert_eq!(
        unsigned,
        imp(
            and(guard(0), and(guard(1), guard(2))),
            element(rel(HNumType::I32, HRelop::GtU, local(2), i32c(1)))
        )
    );
    assert_eq!(
        signed,
        imp(
            and(guard(0), and(guard(1), guard(2))),
            element(gts(local(2), i32c(1)))
        )
    );
}

/// A non-constant index over an aggregate reachability mode already rejected
/// adds no second message: the sentinel swallows the read, so one mistake
/// still yields one diagnostic.
#[test]
fn a_non_constant_index_on_a_rejected_aggregate_stays_silent() {
    let src = "spec S { fn f(x: i32) exists { let a: [i32; 2] = @; let i: i32 = @; \
               assert(a[i] == x); } }";
    let ctx = type_check(src);
    let (_, diagnostics) = translate(&ctx);
    assert_eq!(
        diagnostics.len(),
        1,
        "only the rejected compound `@` may be reported: {diagnostics:?}"
    );
    assert!(diagnostics[0].contains("error[P008]"), "{diagnostics:?}");
}

// ----- 15d. non-constant index in a reachability body (P016) --------------

/// The shape `P016` exists for: an index read from an *entry* parameter of an
/// `exists`-quantified function. Code generation guards the access with a
/// trap, and the reachability judgment fixes the entry vector before letting
/// the choices range — so at `i = 2` every choice traps, the observation set
/// is empty, and the obligation is **false** rather than restricted to the
/// entries whose index is in range. The Rocq gate admits open proofs, so it
/// would ship such an obligation green.
#[test]
fn p016_rejects_an_entry_derived_index_in_an_exists_body() {
    let e = err("spec S { fn f(i: i32) exists { let a: [i32; 2] = [1, 2]; assert(a[i] == 1); } }");
    assert!(e.contains("error[P016]"), "{e}");
    assert!(
        e.contains(
            "a non-constant array index has no place in an `exists`-quantified spec function"
        ),
        "{e}"
    );
    assert!(
        e.contains("makes the claim false rather than narrowing it to the entries it admits"),
        "{e}"
    );
    // The remedy, without which the diagnostic only names the problem — and the
    // route it must not offer. A retained body really does call executable
    // functions, and the reachability judgment reduces the whole activation,
    // callee frames included, so an entry-derived index one call deep reaches
    // the same trap and empties the same observation set. A remedy naming that
    // route would send the author into the failure this rule exists to prevent,
    // to a site the rule is lexical enough that it cannot see.
    assert!(
        e.contains(
            "make the index constant, or move the access into a `forall`-bodied spec function, \
             whose body no judgment reduces"
        ),
        "{e}"
    );
    assert!(
        e.contains(
            "moving it into an executable function this body calls does not escape the trap, \
             because the judgment reduces the whole activation, the callee included"
        ),
        "{e}"
    );
}

/// A `unique` body names its own quantifier. Its judgment demands a *singleton*
/// observation set per entry, which a trapping entry empties exactly as it
/// empties an `exists` one.
#[test]
fn p016_rejects_an_entry_derived_index_in_a_unique_body() {
    let e = err(
        "spec S { fn f(i: i32) unique { let a: [i32; 2] = [1, 2]; let n: i32 = @; \
         assert(a[i] == n); } }",
    );
    assert!(e.contains("error[P016]"), "{e}");
    assert!(
        e.contains("has no place in a `unique`-quantified spec function"),
        "{e}"
    );
}

/// The rule reads the index, not where its value came from. A choice-derived
/// index cannot by itself falsify the obligation — a trapping choice is simply
/// not the witness — but scoping the rejection to entry-derived indices would
/// make the reach of a diagnostic a dataflow question a reader cannot settle
/// from the access it fires on.
#[test]
fn p016_rejects_a_choice_derived_index_too() {
    let e = err(
        "spec S { fn f() exists { let a: [i32; 2] = [1, 2]; let n: i32 = @; \
         assert(a[n] == 1); } }",
    );
    assert!(e.contains("error[P016]"), "{e}");
}

/// Any step of the chain, not only the one directly under the read: `m[0][i]`
/// descends through a constant index first and is rejected at the second.
#[test]
fn p016_rejects_a_non_constant_index_deeper_in_a_chain() {
    let e = err(
        "spec S { fn f(i: i32) exists { let m: [[i32; 2]; 2] = [[1, 2], [3, 4]]; \
         assert(m[0][i] == 1); } }",
    );
    assert!(e.contains("error[P016]"), "{e}");
}

/// Two non-constant indices in one chain report `P016`, not `P002`'s
/// one-case-split-per-chain limit. Here neither index is encodable whatever
/// the budget, so naming the budget would send the author to make one of them
/// constant and meet the same rejection on the other.
#[test]
fn p016_leads_a_chain_carrying_two_non_constant_indices() {
    let e = err(
        "spec S { fn f(i: i32, j: i32) exists { let m: [[i32; 2]; 2] = [[1, 2], [3, 4]]; \
         assert(m[i][j] == 1); } }",
    );
    assert!(e.contains("error[P016]"), "{e}");
    assert!(!e.contains("error[P002]"), "{e}");
}

/// A `forall`-quantified body keeps its non-constant index. Its function is
/// omitted from the emitted module's functions and never reduced, so the guard
/// code generation writes has no bearing on the claim: the index carries the
/// symbolic range bound the element's own definition states, in the universal
/// polarity (an `Himpl` antecedent over the slot guards, with the definition
/// inside the witness binder).
#[test]
fn a_non_constant_index_in_a_forall_body_is_untouched() {
    let src = "spec S { fn f(i: i32) forall { let a: [i32; 2] = @; assert(a[i] > 0); } }";
    let definition = element_def(&local(0), &lvar(0), &[local(1), local(2)]);
    assert_eq!(
        sole_obligation(&ok(src), "S"),
        imp(
            and(guard(0), and(guard(1), guard(2))),
            ex(and(definition, nz(gts(lvar(0), i32c(0)))))
        )
    );
}

/// A nested `exists` block inside a `forall` function keeps it too. The
/// enclosing function is still the omitted, never-reduced kind — the block
/// changes the polarity the range bound rides in (a conjunct inside the
/// binder), not whether the body is executed by a judgment.
#[test]
fn a_non_constant_index_in_a_nested_exists_block_is_untouched() {
    let src = "spec S { fn f(i: i32) forall { exists { let a: [i32; 2] = @; \
               assert(a[i] > 0); } } }";
    let definition = element_def(&local(0), &lvar(0), &[lvar(2), lvar(1)]);
    assert_eq!(
        sole_obligation(&ok(src), "S"),
        imp(
            guard(0),
            ex(ex(ex(and(definition, nz(gts(lvar(0), i32c(0)))))))
        )
    );
}

/// A constant index in a reachability body descends to its element as it
/// always did, under either quantifier. Code generation folds it to a static
/// byte offset and emits no guard at all, so there is no trap for an
/// obligation to be wrong about.
#[test]
fn a_constant_index_in_a_reachability_body_is_untouched() {
    let exists_src = "spec S { fn f(i: i32) exists { let a: [i32; 2] = [1, 2]; let n: i32 = @; \
                      assert(a[0] == n); } }";
    assert_eq!(
        sole_obligation(&ok(exists_src), "S"),
        teq(i32c(1), local(1))
    );
    let unique_src = "spec S { fn f(i: i32) unique { let a: [i32; 2] = [1, 2]; let n: i32 = @; \
                      assert(a[1] == n); } }";
    assert_eq!(
        sole_obligation(&ok(unique_src), "S"),
        teq(i32c(2), local(1))
    );
}

/// An index named through a constant is constant here, and stays accepted even
/// though code generation — which folds only a *direct* literal — still guards
/// it. The two notions differ exactly on statically-known in-range indices,
/// where the guard it emits can never fire, the retained body cannot trap, and
/// the obligation stays true. Rejecting these would refuse sound programs to
/// no purpose; the out-of-range ones are already `P014`, decided by this very
/// fold one branch away.
#[test]
fn a_folded_constant_index_in_a_reachability_body_is_untouched() {
    let src = "spec S { fn f(i: i32) exists { let a: [i32; 2] = [1, 2]; const K: i32 = 1; \
               let n: i32 = @; assert(a[K] == n); } }";
    assert_eq!(sole_obligation(&ok(src), "S"), teq(i32c(2), local(1)));
}

// ----- 16. quantifier alternation (the nested universal binder) ------------

/// A `@` bound inside a `forall` block adds a `+` over the enclosing
/// existential witness — the canonical `∃k. ∀x. x + k = x`.
///
/// Three things are pinned at once. The `Hall` sits *inside* the `HA_ex`, which
/// is the whole point: a slot standing in for the inner `forall` would be
/// quantified by the downstream judgment, outside the existential, and the
/// alternation would silently swap. The universal variable states its own
/// typing as an antecedent *within* its binder, because a `T_lvar` names
/// nothing outside the quantifier that introduced it. And the two contexts
/// read `==` differently in the same obligation — a witness equation under the
/// `assume`, a refutable relop under the `assert`.
#[test]
fn a_forall_block_inside_an_exists_block_binds_a_universal_variable() {
    let body = "forall { exists { let k: i32 = @; assume { assert(k == 0); } \
                forall { let x: i32 = @; assert(x + k == x); } } }";
    assert_eq!(
        obligation_of("", body),
        ex(and(
            teq(lvar(0), i32c(0)),
            all(imp(
                hastype(lvar(0), HNumType::I32),
                nz(eqs(
                    bin(HNumType::I32, HBinop::Add, lvar(0), lvar(1)),
                    lvar(0)
                ))
            ))
        ))
    );
}

/// An `assume` block translates existentially even under universal
/// quantification, so a `forall` nested in one alternates the same way — and
/// the resulting `Hall` lands in the antecedent the `assume` already builds.
#[test]
fn a_forall_block_inside_an_assume_block_alternates_the_same_way() {
    let body = "forall { let n: i32 = @; assume { forall { let x: i32 = @; assert(x >= x); } } \
                assert(n >= n); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            and(
                guard(0),
                all(imp(
                    hastype(lvar(0), HNumType::I32),
                    nz(ges(lvar(0), lvar(0)))
                ))
            ),
            nz(ges(local(0), local(0)))
        )
    );
}

/// The `if`-branch twin of the block form: the branch block itself carries the
/// `forall` kind, so the alternation appears inside one arm of the existential
/// disjunction rather than beside it.
#[test]
fn a_forall_if_branch_alternates_under_an_exists_context() {
    let body = "forall { let n: i32 = @; exists { if n > 0 forall { let x: i32 = @; \
                assert(x >= x); } } }";
    let cond = || gts(local(0), i32c(0));
    assert_eq!(
        obligation_of("", body),
        imp(
            guard(0),
            or(
                and(
                    nz(cond()),
                    all(imp(
                        hastype(lvar(0), HNumType::I32),
                        nz(ges(lvar(0), lvar(0)))
                    ))
                ),
                eqz(cond())
            )
        )
    );
}

/// An aggregate `@` under the nested quantifier binds one universal variable
/// per scalar leaf — the Phase-1 leaf machinery on the level channel — so a
/// two-element array nests two `Hall`s over one shared guard antecedent.
#[test]
fn an_aggregate_uzumaki_under_the_nested_quantifier_binds_one_variable_per_leaf() {
    let body = "forall { exists { let k: i32 = @; assume { assert(k == 0); } \
                forall { let a: [i32; 2] = @; assert(a[0] == a[1]); } } }";
    assert_eq!(
        obligation_of("", body),
        ex(and(
            teq(lvar(0), i32c(0)),
            all(all(imp(
                and(
                    hastype(lvar(1), HNumType::I32),
                    hastype(lvar(0), HNumType::I32)
                ),
                nz(eqs(lvar(1), lvar(0)))
            )))
        ))
    );
}

/// The two binder channels share one level counter, so a block that introduces
/// both a universal variable and a short-circuit witness must nest them in
/// allocation order and index every read at its own depth. This is the only
/// place the universal and existential allocators meet, and the lowering pass's
/// scope assertion is the only thing that would notice a miscount — loudly, but
/// only if some test walks the shape.
#[test]
fn a_universal_variable_and_a_witness_interleave_in_one_block() {
    let body = "forall { exists { let k: i32 = @; assume { assert(k == 0); } \
                forall { let x: i32 = @; let safe: bool = x == 0 || x != 0; \
                assert(safe); } } }";
    // Inside the `Hall`: x is index 0, the witness index 0 under its own `HA_ex`
    // and x becomes index 1 there.
    let witness_def = or_witness(
        lvar(0),
        eqs(lvar(1), i32c(0)),
        rel(HNumType::I32, HRelop::Ne, lvar(1), i32c(0)),
    );
    assert_eq!(
        obligation_of("", body),
        ex(and(
            teq(lvar(0), i32c(0)),
            all(imp(
                hastype(lvar(0), HNumType::I32),
                ex(and(witness_def, nz(lvar(0))))
            ))
        ))
    );
}

/// A call-argument `@` has no name to bind, so its universal binder wraps the
/// enclosing statement's atom directly — and carries its typing guard with it,
/// as an antecedent inside the binder rather than through the guard channel the
/// named form uses. The channel would drain *around* the statement, where the
/// variable is no longer bound.
#[test]
fn a_call_argument_uzumaki_under_the_nested_quantifier_carries_its_own_guard() {
    let prelude = "fn sq(n: i32) -> i32 {\n  return n * n;\n}";
    let body = "forall { exists { let k: i32 = @; assume { assert(k == 0); } \
                forall { assert(sq(@) >= k); } } }";
    assert_eq!(
        obligation_of(prelude, body),
        ex(and(
            teq(lvar(0), i32c(0)),
            all(imp(
                hastype(lvar(0), HNumType::I32),
                nz(ges(app("sq", vec![lvar(0)]), lvar(1)))
            ))
        ))
    );
}

/// The nested universal's typing guard never escapes its binder. Here the
/// enclosing block continues after the alternation, so the outer drain happens
/// at a statement the `Hall` does not enclose: what it carries is the *slot*
/// guard alone, and the logical variable's guard stays inside.
///
/// The distinction is not cosmetic. A `T_local` guard is meaningful wherever it
/// is written; a `T_lvar` guard outside its quantifier names nothing, and the
/// downstream strictification would collapse it, silently hardening the
/// obligation.
#[test]
fn a_nested_universal_guard_stays_inside_its_binder() {
    let body = "forall { let n: i32 = @; exists { forall { let x: i32 = @; assert(x >= x); } } \
                assert(n >= n); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            guard(0),
            and(
                all(imp(
                    hastype(lvar(0), HNumType::I32),
                    nz(ges(lvar(0), lvar(0)))
                )),
                nz(ges(local(0), local(0)))
            )
        )
    );
}

/// Statement semantics inside the nested block are universal throughout: an
/// `assume` becomes the antecedent of what follows, not a conjunct, and it
/// fuses with the pending typing guard exactly as it does at the top level.
/// The `assume`'s own body still translates existentially, one level further
/// in — alternation recurses.
#[test]
fn a_nested_forall_reads_assume_as_an_antecedent() {
    let body = "forall { exists { let k: i32 = @; forall { let x: i32 = @; \
                assume { assert(x == 1); } assert(x + k == k + 1); } } }";
    assert_eq!(
        obligation_of("", body),
        ex(all(imp(
            and(hastype(lvar(0), HNumType::I32), teq(lvar(0), i32c(1))),
            nz(eqs(
                bin(HNumType::I32, HBinop::Add, lvar(0), lvar(1)),
                bin(HNumType::I32, HBinop::Add, lvar(1), i32c(1))
            ))
        )))
    );
}

/// An `if` inside the nested universal block reads with universal statement
/// semantics — both arms are implications the payload must satisfy — rather
/// than the existential reading, where the arms are a disjunction of
/// conjunctions. Nothing else pins which `t_if` arm the new mode takes: both
/// arms accept it, so routing it to the existential one would compile and leave
/// every suite green while silently changing the obligation.
#[test]
fn an_if_inside_the_nested_universal_block_reads_universally() {
    let body = "forall { exists { let k: i32 = @; forall { let x: i32 = @; \
                if x > 0 { assert(k == 1); } else { assert(k == 2); } } } }";
    assert_eq!(
        obligation_of("", body),
        ex(all(imp(
            hastype(lvar(0), HNumType::I32),
            and(
                imp(nz(gts(lvar(0), i32c(0))), nz(eqs(lvar(1), i32c(1)))),
                imp(
                    HAssert::eqz(gts(lvar(0), i32c(0))),
                    nz(eqs(lvar(1), i32c(2)))
                )
            )
        )))
    );
}

/// `P007` survives where the lift cannot reach: inside an `exists`/`unique`
/// body every `@` is a hidden trailing choice parameter the downstream judgment
/// quantifies operationally, so a universal binder over one would need a
/// choice-plan and lowering redesign rather than translator work. Both the
/// block form and the `if`-branch form keep the rejection.
#[test]
fn p007_is_kept_for_a_forall_if_branch_in_a_reachability_body() {
    let src = "spec S { fn f(x: i32) exists { let n: i32 = @; if n > 0 forall { \
               assert(n > x); } } }";
    let e = err(src);
    assert!(e.contains("error[P007]"), "{e}");
}

/// The surviving `P007` names the quantifier that makes the nesting impossible,
/// since the identical nesting translates in a `forall`/plain body. Both forms
/// — the block and the `if` branch — raise the one message, and a `unique` body
/// names its own word.
#[test]
fn p007_names_the_reachability_quantifier_at_both_forms() {
    let opening = "a `forall` block has no encoding inside an `exists`-quantified spec function";
    let remedy = "move the universal claim into its own `forall`-bodied spec function";

    let block = err("spec S { fn f() exists { let n: i32 = @; forall { assert(n > 0); } } }");
    assert!(block.contains(opening) && block.contains(remedy), "{block}");

    let branch_src = "spec S { fn f(x: i32) exists { let n: i32 = @; \
                      if n > 0 forall { assert(n > x); } } }";
    let branch = err(branch_src);
    assert!(
        branch.contains(opening) && branch.contains(remedy),
        "{branch}"
    );

    let unique_opening = "a `forall` block has no encoding inside a `unique`-quantified \
                          spec function";
    let unique = err("spec S { fn f() unique { let n: i32 = @; forall { assert(n > 0); } } }");
    assert!(unique.contains(unique_opening), "{unique}");
}

/// The nested universal binder is a real `Hall`, never an `HA_ex`. Reading the
/// binder off the tree rather than comparing the whole obligation keeps this
/// pinned even if the surrounding shape changes.
#[test]
fn the_nested_universal_binder_is_not_an_existential() {
    let body = "forall { exists { let k: i32 = @; assume { assert(k == 0); } \
                forall { let x: i32 = @; assert(x >= k); } } }";
    let HAssert::Ex(outer) = obligation_of("", body) else {
        panic!("the enclosing `exists` binds the witness");
    };
    let HAssert::And(_, inner) = *outer else {
        panic!("the existential conjoins its filter with the nested claim");
    };
    assert!(
        matches!(*inner, HAssert::All(_)),
        "the nested `forall` must bind a universal variable, got {inner:?}"
    );
}

// ----- 17. declared value domains of narrow scalars -----------------------

fn leu(l: HTerm, r: HTerm) -> HTerm {
    rel(HNumType::I32, HRelop::LeU, l, r)
}

/// The zero-extending sub-word widths are bounded *unsigned*, strictly, at the
/// exclusive top of the set their normalization mask produces. The whole
/// obligation is compared literally here rather than through `guard_of`,
/// because the signedness and the strictness are exactly what a downstream
/// narrow-idiom lemma matches on and a wrong choice reads as plausible.
#[test]
fn a_u8_draw_is_bounded_unsigned_below_its_exclusive_top() {
    let body = "forall { let a: u8 = @; assert(a <= 200); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), nz(ltu(local(0), i32c(256)))),
            nz(leu(local(0), i32c(200)))
        )
    );
}

#[test]
fn a_u16_draw_is_bounded_unsigned_below_its_exclusive_top() {
    let body = "forall { let a: u16 = @; assert(a <= 1000); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), nz(ltu(local(0), i32c(65536)))),
            nz(leu(local(0), i32c(1000)))
        )
    );
}

/// A sign-extending width states *both* bounds: the assertion language has no
/// range predicate, so a one-sided signed bound would characterize nothing —
/// every negative value below the lower end would still satisfy it.
#[test]
fn an_i8_draw_is_bounded_by_a_signed_pair() {
    let body = "forall { let a: i8 = @; assert(a <= 100); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            and(
                guard(0),
                and(nz(les(i32c(-128), local(0))), nz(lts(local(0), i32c(128))))
            ),
            nz(rel(HNumType::I32, HRelop::LeS, local(0), i32c(100)))
        )
    );
}

#[test]
fn an_i16_draw_is_bounded_by_a_signed_pair() {
    let body = "forall { let a: i16 = @; assert(a <= 100); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            and(
                guard(0),
                and(
                    nz(les(i32c(-32768), local(0))),
                    nz(lts(local(0), i32c(32768)))
                )
            ),
            nz(rel(HNumType::I32, HRelop::LeS, local(0), i32c(100)))
        )
    );
}

/// `bool` has two normalization forms in compiled code — a mask for a draw, a
/// non-zero test for an entry parameter — and both land on `{0, 1}`, so one
/// bound covers the declaration.
#[test]
fn a_bool_draw_is_bounded_below_two() {
    let body = "forall { let a: bool = @; assert(a); }";
    assert_eq!(
        obligation_of("", body),
        imp(and(guard(0), nz(ltu(local(0), i32c(2)))), nz(local(0)))
    );
}

/// An enum's domain is its zero-based tags, so the bound is the variant count
/// and moves with it. Two enums of different sizes pin that it is the count
/// rather than a constant.
#[test]
fn an_enum_draw_is_bounded_by_its_variant_count() {
    let body = "forall { let b: Bit = @; assert(b == Bit::Off); }";
    assert_eq!(
        obligation_of("enum Bit { Off, On }", body),
        imp(
            and(guard(0), nz(ltu(local(0), i32c(2)))),
            nz(eqs(local(0), i32c(0)))
        )
    );
    let body = "forall { let d: Dir = @; assert(d == Dir::W); }";
    assert_eq!(
        obligation_of("enum Dir { N, E, S, W }", body),
        imp(
            and(guard(0), nz(ltu(local(0), i32c(4)))),
            nz(eqs(local(0), i32c(3)))
        )
    );
}

/// A `::`-qualified enum resolves to the same variant count an unqualified one
/// would, through the by-path lookup: a bare name and a path must classify
/// identically or a cross-module declaration would silently lose its bound.
#[test]
fn a_cross_module_enum_draw_resolves_its_variant_count() {
    let ctx = type_check_multi(&[
        (
            vec![],
            "use lib;\nspec S {\n  fn f() forall {\n    let c: lib::Level = @;\n    assert(c == c);\n  }\n}\n",
        ),
        (vec!["lib"], "pub enum Level { Low, Mid, High }\n"),
    ]);
    let (map, diagnostics) = translate(&ctx);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        obligation_named(&map, "S", "S.f"),
        imp(
            and(guard(0), nz(ltu(local(0), i32c(3)))),
            nz(eqs(local(0), local(0)))
        )
    );
}

/// A cross-module enum whose name collides with a local one resolves by the
/// *key* the type checker recorded, not by the name.
///
/// The two lookups the resolver tries in order disagree here and only here: a
/// bare-name lookup relative to the referencing file finds the local `Level`,
/// while the key names the one the callee declared. The site is an anonymous
/// `@` in call-argument position, so the domain comes from a *recorded*
/// expression type — the resolved spelling, the only carrier that has a key at
/// all — and the callee's three variants are the set the choice ranges over.
///
/// The collision is what makes this observable. Resolving by name here would
/// bound the choice at the local declaration's variant count, and a *tighter*
/// universal antecedent is the unsound direction: it makes a claim provable
/// over fewer values than the program can produce. Code generation resolves the
/// same pair in the same order when it emits the draw's normalization, so the
/// two would also stop agreeing about the value set.
#[test]
fn a_colliding_cross_module_enum_argument_resolves_by_key_not_by_name() {
    let ctx = type_check_multi(&[
        (
            vec![],
            "use lib;\nenum Level { Only }\nspec S {\n  fn f() forall {\n    \
             assert(lib::g(@) == 1);\n  }\n}\n",
        ),
        (
            vec!["lib"],
            "pub enum Level { Low, Mid, High }\npub fn g(v: Level) -> i32 {\n  return 1;\n}\n",
        ),
    ]);
    let (map, diagnostics) = translate(&ctx);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        obligation_named(&map, "S", "S.f"),
        imp(
            and(guard(0), nz(ltu(local(0), i32c(3)))),
            nz(eqs(app("lib.g", vec![local(0)]), i32c(1)))
        )
    );
}

/// The four full widths are untouched: their antecedent is exactly one
/// `HA_has_type` and nothing beside it, so a bound stated where the declaration
/// already admits every value of its class fails here rather than quietly
/// moving every existing obligation.
#[test]
fn the_full_widths_state_their_typing_and_nothing_else() {
    for decl_ty in ["i32", "u32", "i64", "u64"] {
        let body = format!("forall {{ let x: {decl_ty} = @; assert(x == x); }}");
        let obligation = obligation_of("", &body);
        let HAssert::Imp(antecedent, _) = obligation else {
            panic!("expected a guarded implication for `{decl_ty}`, got {obligation:?}");
        };
        assert_eq!(
            *antecedent,
            hastype(local(0), guard_width(decl_ty)),
            "a `{decl_ty}` slot admits every value of its class, so it states only its typing"
        );
    }
}

/// A narrow *parameter* states the same set a narrow draw does. A `spec`
/// function is compiled in proof mode but never exported, so its parameters
/// receive neither ABI normalization nor an enum tag guard: the antecedent is
/// the only place the declaration's meaning is written down.
#[test]
fn a_narrow_spec_parameter_states_its_domain() {
    let source = "spec S { fn f(p: u8) forall { assert(p <= 255); } }";
    assert_eq!(
        sole_obligation(&ok(source), "S"),
        imp(
            and(guard(0), nz(ltu(local(0), i32c(256)))),
            nz(leu(local(0), i32c(255)))
        )
    );
}

/// An existential bound is an `HA_and` conjunct *inside* its binder, never an
/// antecedent. In antecedent position the prover would pick a witness that
/// refutes the bound and discharge the obligation without ever meeting the
/// payload, so the connective is asserted on its own as well as through the
/// whole tree.
#[test]
fn an_existential_narrow_draw_bounds_its_witness_as_a_conjunct() {
    let body = "forall { exists { let a: u8 = @; assert(a == 44); } }";
    assert_eq!(
        obligation_of("", body),
        ex(and(nz(ltu(lvar(0), i32c(256))), teq(lvar(0), i32c(44))))
    );
    let HAssert::Ex(inner) = obligation_of("", body) else {
        panic!("the existential `@` binds a witness");
    };
    assert!(
        matches!(*inner, HAssert::And(..)),
        "the bound must constrain the witness, not sit in an antecedent the witness can \
         refute: {inner:?}"
    );
}

/// An existential `@` nothing reads keeps its vacuity. The bound is dropped
/// with every other unread binder definition, so the obligation still folds to
/// `HA_true` and is still refused. Conjoined unconditionally it would be
/// trivially true, say nothing about the program, and no longer look vacuous.
#[test]
fn an_unread_existential_narrow_draw_stays_vacuous_and_is_refused() {
    let unread = err("spec S { fn f() forall { exists { let a: u8 = @; } } }");
    assert!(unread.contains("error[P010]"), "{unread}");
}

/// Under the nested universal quantifier the bound lands inside the `Hall`,
/// beside the typing guard and under the same implication. Both name a
/// `T_lvar`, which denotes nothing outside the binder that introduced it.
#[test]
fn a_nested_universal_narrow_draw_states_its_domain_inside_its_hall() {
    let body = "forall { exists { let k: i32 = @; assume { assert(k == 0); } \
                forall { let x: u8 = @; assert(x <= 200); } } }";
    assert_eq!(
        obligation_of("", body),
        ex(and(
            teq(lvar(0), i32c(0)),
            all(imp(
                and(hastype(lvar(0), HNumType::I32), nz(ltu(lvar(0), i32c(256)))),
                nz(leu(lvar(0), i32c(200)))
            ))
        ))
    );
}

/// An anonymous `@` has no annotation, so its domain comes from the type
/// recorded for the argument — the callee's declared parameter type — exactly
/// as its guard width does.
#[test]
fn an_anonymous_narrow_argument_takes_its_domain_from_the_callee() {
    let prelude = "fn g(x: u8) -> i32 {\n  return 1;\n}";
    let body = "forall { assert(g(@) == 1); }";
    assert_eq!(
        obligation_of(prelude, body),
        imp(
            and(guard(0), nz(ltu(local(0), i32c(256)))),
            nz(eqs(app("g", vec![local(0)]), i32c(1)))
        )
    );
}

/// The existential twin of the anonymous form: the binder is not *pinned* — a
/// `@` is the free choice itself — but it is *bounded*, as a conjunct on the
/// binder rather than through the guard channel, which drains outside the wrap
/// where the variable is no longer bound.
#[test]
fn an_anonymous_narrow_argument_bounds_its_existential_binder() {
    let prelude = "fn g(x: u8) -> i32 {\n  return 1;\n}";
    let body = "forall { exists { assert(g(@) == 1); } }";
    assert_eq!(
        obligation_of(prelude, body),
        ex(and(
            nz(ltu(lvar(0), i32c(256))),
            teq(app("g", vec![lvar(0)]), i32c(1))
        ))
    );
}

/// A reachability payload is unchanged by any of this, and stays that way on
/// purpose. It denotes against the frame an actual run reaches, where the slot
/// already holds a value the compiled body drew and masked; the judgment counts
/// only runs that reduce normally, and every payload-contributing statement
/// traps when its condition fails, so the raw reading is already restricted to
/// the vectors a proof would pick. A bound here would be dead weight at best.
#[test]
fn a_reach_mode_narrow_draw_states_no_domain() {
    let source = "spec S { fn f() exists { let a: u8 = @; assert(a == 44); } }";
    assert_eq!(sole_obligation(&ok(source), "S"), teq(local(0), i32c(44)));
}

/// The nested-universal *anonymous* `@` carries its hypothesis on the binder
/// itself rather than through the guard channel, so the two spellings of the
/// same hypothesis are pinned separately. The channel drains around the
/// statement — outside the wrap, where the variable no longer denotes — which
/// is why this position cannot reuse it, and why a domain lost here would be
/// invisible to every test of the named form.
#[test]
fn a_nested_universal_anonymous_argument_states_its_domain_on_its_binder() {
    let prelude = "fn g(x: u8) -> i32 {\n  return 1;\n}";
    let body = "forall { exists { let k: i32 = @; assume { assert(k == 0); } \
                forall { assert(g(@) == 1); } } }";
    assert_eq!(
        obligation_of(prelude, body),
        ex(and(
            teq(lvar(0), i32c(0)),
            all(imp(
                and(hastype(lvar(0), HNumType::I32), nz(ltu(lvar(0), i32c(256)))),
                nz(eqs(app("g", vec![lvar(0)]), i32c(1)))
            ))
        ))
    );
}

/// A nested-universal binder whose variable the claim never reads keeps the
/// binder and drops its hypothesis. The binder has to stay — removing it would
/// shift the level of every binder allocated inside it — while the hypothesis
/// is an assumption about a variable nothing mentions, and stating it would put
/// an antecedent in front of a claim that has nothing to do with it.
#[test]
fn an_unread_nested_universal_binder_keeps_no_hypothesis() {
    let prelude = "fn g(x: u8) -> i32 {\n  return 1;\n}";
    let body = "forall { exists { let k: i32 = @; assume { assert(k == 0); } \
                forall { let t: i32 = g(@) + 1; assert(k == 0); } } }";
    assert_eq!(
        obligation_of(prelude, body),
        ex(and(teq(lvar(0), i32c(0)), all(nz(eqs(lvar(1), i32c(0))))))
    );
}

/// The same drop taken to its vacuous end: a nested-universal `@` in a
/// statement that contributes nothing leaves `HA_true` behind, which is refused
/// rather than emitted.
#[test]
fn a_nested_universal_binder_over_a_vacuous_statement_is_refused() {
    let prelude = "fn g(x: u8) -> i32 {\n  return 1;\n}";
    let source = format!(
        "{prelude}\nspec S {{ fn f() forall {{ exists {{ forall {{ if g(@) == 1 {{ }} }} }} }} }}"
    );
    let refused = err(&source);
    assert!(refused.contains("error[P010]"), "{refused}");
}

// ----- 18. declared value domains of aggregate leaves ---------------------

fn gtu(l: HTerm, r: HTerm) -> HTerm {
    rel(HNumType::I32, HRelop::GtU, l, r)
}

/// An array's leaves state what their declared *element* type admits, so a
/// narrow element reached one aggregate level deep is exactly as constrained as
/// the same declaration written as a scalar. Without this a `u8` element would
/// range over every `i32` while a `u8` variable did not — a soundness asymmetry
/// with no justification in the language.
#[test]
fn a_narrow_array_bounds_every_leaf_at_its_element_type() {
    let body = "forall { let a: [u8; 2] = @; assert(a[0] <= a[1]); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            guards_of(&[("u8", 0), ("u8", 1)]),
            nz(leu(local(0), local(1)))
        )
    );
}

/// A `bool` array's leaves land on `{0, 1}` like a `bool` variable's.
#[test]
fn a_bool_array_bounds_every_leaf_below_two() {
    let body = "forall { let a: [bool; 2] = @; assert(a[0] == a[1]); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            guards_of(&[("bool", 0), ("bool", 1)]),
            nz(eqs(local(0), local(1)))
        )
    );
}

/// Rank is irrelevant: the element domain travels down every array layer, so
/// all four leaves of a `[[i8; 2]; 2]` carry the signed pair.
#[test]
fn a_multi_rank_narrow_array_bounds_every_leaf() {
    let body = "forall { let m: [[i8; 2]; 2] = @; assert(m[0][0] == m[1][1]); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            guards_of(&[("i8", 0), ("i8", 1), ("i8", 2), ("i8", 3)]),
            nz(eqs(local(0), local(3)))
        )
    );
}

/// An array of enum leaves is bounded by the variant count, exactly as an enum
/// variable is.
#[test]
fn an_enum_array_bounds_every_leaf_by_the_variant_count() {
    let body = "forall { let a: [Color; 2] = @; assert(a[0] == a[1]); }";
    assert_eq!(
        obligation_of("enum Color { Red, Green, Blue }", body),
        imp(
            and(
                and(guard(0), nz(ltu(local(0), i32c(3)))),
                and(guard(1), nz(ltu(local(1), i32c(3))))
            ),
            nz(eqs(local(0), local(1)))
        )
    );
}

/// Each struct field states its own field type's domain, so a narrow field and
/// a full-width field in the same struct get different hypotheses. A per-struct
/// or per-introduction decision would show up here as one shape for both.
#[test]
fn a_struct_bounds_each_field_at_its_own_declared_type() {
    let prelude = "struct Mixed { small: u8; wide: i32; }";
    let body = "forall { let m: Mixed = @; assert(m.small == 1 && m.wide == 2); }";
    assert_eq!(
        obligation_of(prelude, body),
        imp(
            guards_of(&[("u8", 0), ("i32", 1)]),
            and(nz(eqs(local(0), i32c(1))), nz(eqs(local(1), i32c(2))))
        )
    );
}

/// A struct's enum field resolves its variant count, and a 64-bit field beside
/// it keeps its own class — the field loop states each field's own two halves
/// rather than one shape for the struct.
#[test]
fn a_struct_bounds_an_enum_field_by_its_variant_count() {
    let prelude = "enum Color { Red, Green, Blue }\nstruct Tagged { tag: Color; payload: i64; }";
    let body = "forall { let t: Tagged = @; assert(t.tag == Color::Red); }";
    assert_eq!(
        obligation_of(prelude, body),
        imp(
            and(
                and(guard(0), nz(ltu(local(0), i32c(3)))),
                hastype(local(1), HNumType::I64)
            ),
            nz(eqs(local(0), i32c(0)))
        )
    );
}

/// A one-dimensional scalar-array *field* is the second shape a struct admits,
/// and its element domain travels the same way an array introduction's does.
#[test]
fn a_narrow_array_field_bounds_its_elements() {
    let prelude = "struct Row { xs: [u16; 2]; }";
    let body = "forall { let r: Row = @; assert(r.xs[0] == r.xs[1]); }";
    assert_eq!(
        obligation_of(prelude, body),
        imp(
            guards_of(&[("u16", 0), ("u16", 1)]),
            nz(eqs(local(0), local(1)))
        )
    );
}

/// A full-width aggregate is untouched: its leaves state their typing and
/// nothing else, so a bound leaking onto a class that admits every value fails
/// here rather than quietly moving every existing obligation.
#[test]
fn a_full_width_array_states_only_its_leaf_typings() {
    let body = "forall { let a: [i32; 3] = @; assert(a[0] <= a[2]); }";
    assert_eq!(
        obligation_of("", body),
        imp(
            and(guard(0), and(guard(1), guard(2))),
            nz(rel(HNumType::I32, HRelop::LeS, local(0), local(2)))
        )
    );
}

/// A narrow aggregate *parameter* states its leaf domains too — the only mode
/// an aggregate parameter binds in, since parameters bind at function entry.
#[test]
fn a_narrow_aggregate_parameter_bounds_its_leaves() {
    let source = "spec S { fn f(a: [u8; 2]) forall { assert(a[0] <= a[1]); } }";
    assert_eq!(
        sole_obligation(&ok(source), "S"),
        imp(
            guards_of(&[("u8", 0), ("u8", 1)]),
            nz(leu(local(0), local(1)))
        )
    );
}

/// Under the nested universal quantifier the leaf hypotheses ride the same
/// channel a nested-universal scalar's does, and stay inside the `Hall` wraps
/// their `T_lvar`s are bound by.
#[test]
fn a_nested_universal_narrow_array_bounds_its_leaves_inside_its_halls() {
    let body = "forall { exists { forall { let a: [u8; 2] = @; assert(a[0] <= a[1]); } } }";
    assert_eq!(
        obligation_of("", body),
        all(all(imp(
            and(
                and(hastype(lvar(1), HNumType::I32), nz(ltu(lvar(1), i32c(256)))),
                and(hastype(lvar(0), HNumType::I32), nz(ltu(lvar(0), i32c(256))))
            ),
            nz(leu(lvar(1), lvar(0)))
        )))
    );
}

/// An existential aggregate bounds each witness as an `HA_and` conjunct inside
/// that witness's own binder — never an antecedent, which the prover could
/// refute to discharge the obligation without meeting the payload.
#[test]
fn an_existential_narrow_array_bounds_each_witness_as_a_conjunct() {
    let body = "forall { exists { let a: [u8; 2] = @; assert(a[0] > 200); } }";
    assert_eq!(
        obligation_of("", body),
        ex(and(
            nz(ltu(lvar(0), i32c(256))),
            ex(nz(gtu(lvar(1), i32c(200))))
        ))
    );
}

/// The bound belongs to the leaf that was allocated it. The binders wrap
/// innermost-first, so the last leaf allocated is the innermost quantifier;
/// walking them the other way would attach each bound to a different leaf, and
/// with leaves of two declared types that shows up as a `u8` bound over the
/// `i8` field and back.
#[test]
fn an_existential_struct_bounds_each_witness_at_its_own_field_type() {
    let prelude = "struct Pair { small: u8; signed: i8; }";
    let body = "forall { exists { let p: Pair = @; assert(p.small == 1 && p.signed == 2); } }";
    assert_eq!(
        obligation_of(prelude, body),
        ex(and(
            nz(ltu(lvar(0), i32c(256))),
            ex(and(
                and(nz(les(i32c(-128), lvar(0))), nz(lts(lvar(0), i32c(128)))),
                and(teq(lvar(1), i32c(1)), teq(lvar(0), i32c(2)))
            ))
        ))
    );
}

/// A leaf the claim never reads keeps its binder and drops its bound, like
/// every other unread binder definition. Reading only the *second* leaf pins
/// which binder the surviving bound sits on: the outer binder is the unread
/// one, so a bound there would be over the wrong variable as well as
/// trivially satisfiable.
#[test]
fn an_unread_existential_array_leaf_carries_no_bound() {
    let body = "forall { exists { let a: [u8; 2] = @; assert(a[1] == 1); } }";
    assert_eq!(
        obligation_of("", body),
        ex(ex(and(nz(ltu(lvar(0), i32c(256))), teq(lvar(0), i32c(1)))))
    );
}

/// An existential aggregate nothing reads keeps its vacuity: every bound is
/// dropped, the binders collapse, and the obligation is still refused rather
/// than becoming a trivially true range claim.
#[test]
fn an_unread_existential_narrow_array_stays_vacuous_and_is_refused() {
    let unread = err("spec S { fn f() forall { exists { let a: [u8; 2] = @; } } }");
    assert!(unread.contains("error[P010]"), "{unread}");
}

/// The asymmetry this closes, stated as the two spellings of one claim: a
/// witness outside the declared domain is refutable at scalar position and at
/// aggregate position alike. Both emit the same bound over the witness they
/// read, so neither can be discharged by a value the program cannot draw.
#[test]
fn a_witness_outside_the_declared_domain_is_bounded_at_either_nesting() {
    let scalar = "forall { exists { let a: u8 = @; assert(a > 255); } }";
    assert_eq!(
        obligation_of("", scalar),
        ex(and(
            nz(ltu(lvar(0), i32c(256))),
            nz(gtu(lvar(0), i32c(255)))
        ))
    );
    let leaf = "forall { exists { let a: [u8; 2] = @; assert(a[0] > 255); } }";
    assert_eq!(
        obligation_of("", leaf),
        ex(and(
            nz(ltu(lvar(0), i32c(256))),
            ex(nz(gtu(lvar(1), i32c(255))))
        ))
    );
}

// ----- 19. uninhabited declared types -------------------------------------

/// The refusal every position shares: a variantless enum admits no value, so
/// there is nothing to quantify. `⊥` would discharge every claim over it for
/// the wrong reason and any inhabited bound would be a lie, so the introduction
/// is refused instead of encoded.
fn uninhabited_refusal(source: &str) -> String {
    let refused = err(source);
    assert!(
        refused.contains("error[P015]"),
        "expected the uninhabited-type refusal, got {refused}"
    );
    refused
}

/// A `let … = @;` at a variantless enum, in every mode the statement translator
/// has. The universal, nested-universal and existential modes all quantify the
/// declaration outright; the reachability mode quantifies it operationally,
/// through a choice parameter of a run — no reading of it has a value to offer,
/// so all four refuse.
#[test]
fn a_variantless_enum_draw_is_refused_in_every_mode() {
    let univ = uninhabited_refusal(
        "enum Void { }\nspec S { fn f() forall { let a: Void = @; assert(a == a); } }",
    );
    assert!(
        univ.contains("`@` over enum `Void`, which has no variants, quantifies nothing"),
        "{univ}"
    );
    assert!(univ.contains("give `Void` at least one variant"), "{univ}");

    uninhabited_refusal(
        "enum Void { }\nspec S { fn f() forall { exists { let a: Void = @; assert(a == a); } } }",
    );
    uninhabited_refusal(
        "enum Void { }\nspec S { fn f() forall { exists { forall { let a: Void = @; \
         assert(a == a); } } } }",
    );
    uninhabited_refusal(
        "enum Void { }\nspec S { fn f() exists { let a: Void = @; assert(a == a); } }",
    );
}

/// A *parameter* is refused in the reader's own words: it is a declaration, not
/// a `@`, and naming it as one would send the author looking for a draw that is
/// not there.
#[test]
fn a_variantless_enum_parameter_is_refused_by_name() {
    let refused =
        uninhabited_refusal("enum Void { }\nspec S { fn f(p: Void) forall { assert(p == p); } }");
    assert!(
        refused.contains("parameter `p` over enum `Void`, which has no variants"),
        "{refused}"
    );
}

/// An anonymous call-argument `@` takes its domain from the callee's declared
/// parameter type, so an uninhabited parameter there is refused at the call
/// site — where the `@` the author can remove actually is.
///
/// All three modes this position reaches, because the refusal is raised ahead of
/// the mode split rather than on the branch that happens to want the domain. The
/// third wrapper is an `exists`-quantified *function*, whose body translates in
/// the reachability mode: there a `@` is an operationally existential choice
/// parameter and the domain is never read at all, so a refusal placed after the
/// split would silently let an uninhabited choice through.
#[test]
fn a_variantless_enum_call_argument_is_refused() {
    let prelude = "enum Void { }\nfn g(v: Void) -> i32 {\n  return 1;\n}";
    uninhabited_refusal(&format!(
        "{prelude}\nspec S {{ fn f() forall {{ assert(g(@) == 1); }} }}"
    ));
    uninhabited_refusal(&format!(
        "{prelude}\nspec S {{ fn f() forall {{ exists {{ assert(g(@) == 1); }} }} }}"
    ));
    uninhabited_refusal(&format!(
        "{prelude}\nspec S {{ fn f() exists {{ assert(g(@) == 1); }} }}"
    ));
}

/// An aggregate leaf is refused too — the leaf is a quantified variable like
/// any other, and an array of an uninhabited type is itself uninhabited.
///
/// One line for the array whatever its length, but not because the enumeration
/// is de-duplicated: an array shape holds one element shape however many
/// elements it has, so a walk over the shape visits its scalar leaf exactly
/// once. What the count pins here is that the shape walk does not multiply the
/// element out — the de-duplication itself is what
/// [`two_leaves_of_one_uninhabited_enum_are_one_refusal`] covers, over the only
/// shape that can reach a second leaf of the same declaration.
#[test]
fn a_variantless_enum_aggregate_leaf_is_refused() {
    let refused = uninhabited_refusal(
        "enum Void { }\nspec S { fn f() forall { let a: [Void; 2] = @; assert(a[0] == a[1]); } }",
    );
    assert!(
        refused.contains("a leaf of the `@` over enum `Void`"),
        "{refused}"
    );
    assert_eq!(
        refused.lines().count(),
        1,
        "an array shape holds one element shape, so its leaf is visited once: {refused}"
    );

    let field = uninhabited_refusal(
        "enum Void { }\nstruct Holder { tag: Void; n: i32; }\n\
         spec S { fn f(h: Holder) forall { assert(h.n == 1); } }",
    );
    assert!(
        field.contains("a leaf of parameter `h` over enum `Void`"),
        "{field}"
    );
}

/// Two fields of the *same* variantless enum are one mistake, reported once.
///
/// A struct is the only shape whose walk reaches two distinct leaves of one
/// declaration — an array holds a single element shape — so this is the one
/// source that can produce a duplicate at all, and the only place the
/// enumeration's per-name collapse is observable. Reporting it twice would send
/// the author to one declaration twice.
#[test]
fn two_leaves_of_one_uninhabited_enum_are_one_refusal() {
    let refused = uninhabited_refusal(
        "enum Void { }\nstruct Both { a: Void; b: Void; }\n\
         spec S { fn f() forall { let x: Both = @; assert(x.a == x.b); } }",
    );
    assert_eq!(
        refused.lines().count(),
        1,
        "one declaration to change is one refusal: {refused}"
    );
}

/// Two variantless enums in one aggregate are two mistakes, and the author
/// should see both rather than recompiling to find the second.
#[test]
fn two_uninhabited_leaf_enums_are_refused_separately() {
    let refused = uninhabited_refusal(
        "enum Void { }\nenum Nil { }\nstruct Both { a: Void; b: Nil; }\n\
         spec S { fn f() forall { let x: Both = @; assert(x.a == x.a); } }",
    );
    assert!(refused.contains("enum `Void`"), "{refused}");
    assert!(refused.contains("enum `Nil`"), "{refused}");
}

/// Nothing outside a specification body is touched. Analysis rule `A009` warns
/// about the declaration itself and executable code generation compiles a
/// program that uses one, so a variantless enum reaching this pass only through
/// executable code raises nothing: the refusal is about *quantifying* an
/// uninhabited type, which is something only a specification does.
#[test]
fn a_variantless_enum_outside_a_specification_is_untouched() {
    let source = "enum Void { }\nfn use_it(v: Void) -> i32 {\n  return 0;\n}\n\
                  spec S { fn f(p: i32) forall { assert(p == p); } }";
    assert_eq!(
        sole_obligation(&ok(source), "S"),
        imp(guard(0), nz(eqs(local(0), local(0))))
    );
}
