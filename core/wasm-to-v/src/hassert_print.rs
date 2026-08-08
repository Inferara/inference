//! Pretty-printer for `hassert` verification obligations into wasm-verifier
//! Gallina.
//!
//! A pure function of the [`inference_hassert`] IR plus a resolution map that
//! carries each applied function symbol to its `mod_funcs` index (computed by
//! the translator against the emitted module's function layout). The printer
//! mirrors the wasm-verifier `Assertions.v` constructors one-to-one, with the
//! only sugar being the three definitionally-transparent forms the IR keeps
//! explicit: [`HAssert::TermEq`] → `term_eq`, [`HAssert::Imp`] → `Himpl`,
//! [`HAssert::Or`] → `Hor`. Every compound argument is parenthesized, so the
//! output is unambiguous regardless of Gallina's application/precedence rules.

use crate::gallina::z_literal;
use inference_hassert::{HAssert, HBinop, HConst, HFnRef, HNumType, HRelop, HTerm};
use rustc_hash::FxHashMap;

/// Renders one obligation as the right-hand side of a
/// `Definition … : hassert :=` binding (no surrounding parentheses).
///
/// `resolved` must carry every function symbol the tree applies; the translator
/// builds it by resolving each symbol against the module's function layout and
/// fails closed before printing, so a missing entry is an internal invariant
/// violation rather than a reachable error.
pub(crate) fn print_assert(a: &HAssert, resolved: &FxHashMap<String, u32>) -> String {
    assert_str(a, resolved)
}

/// Pushes every function symbol applied by a `T_app`/`HA_app_ok` anywhere in
/// the tree onto `acc` (with duplicates; the caller sorts and de-duplicates).
pub(crate) fn collect_symbols<'a>(a: &'a HAssert, acc: &mut Vec<&'a str>) {
    match a {
        HAssert::True | HAssert::False => {}
        HAssert::Not(x) | HAssert::Ex(x) => collect_symbols(x, acc),
        HAssert::And(l, r) | HAssert::Imp(l, r) | HAssert::Or(l, r) => {
            collect_symbols(l, acc);
            collect_symbols(r, acc);
        }
        HAssert::TermEq(l, r) => {
            collect_term_symbols(l, acc);
            collect_term_symbols(r, acc);
        }
        HAssert::HasType(t, _) | HAssert::Defined(t) => collect_term_symbols(t, acc),
        HAssert::AppOk(f, args) => {
            acc.push(f.0.as_str());
            for arg in args {
                collect_term_symbols(arg, acc);
            }
        }
    }
}

fn collect_term_symbols<'a>(t: &'a HTerm, acc: &mut Vec<&'a str>) {
    match t {
        HTerm::Const(_) | HTerm::LVar(_) | HTerm::Local(_) => {}
        HTerm::App(f, args) => {
            acc.push(f.0.as_str());
            for arg in args {
                collect_term_symbols(arg, acc);
            }
        }
        HTerm::Binop(_, _, l, r) | HTerm::Relop(_, _, l, r) => {
            collect_term_symbols(l, acc);
            collect_term_symbols(r, acc);
        }
    }
}

fn assert_str(a: &HAssert, r: &FxHashMap<String, u32>) -> String {
    match a {
        HAssert::True => "HA_true".to_string(),
        HAssert::False => "HA_false".to_string(),
        HAssert::Not(x) => format!("HA_not {}", paren_assert(x, r)),
        HAssert::And(l, rr) => format!("HA_and {} {}", paren_assert(l, r), paren_assert(rr, r)),
        HAssert::Imp(l, rr) => format!("Himpl {} {}", paren_assert(l, r), paren_assert(rr, r)),
        HAssert::Or(l, rr) => format!("Hor {} {}", paren_assert(l, r), paren_assert(rr, r)),
        HAssert::Ex(body) => format!("HA_ex {}", paren_assert(body, r)),
        HAssert::TermEq(l, rr) => format!("term_eq {} {}", paren_term(l, r), paren_term(rr, r)),
        HAssert::HasType(t, ty) => format!("HA_has_type {} {}", paren_term(t, r), numtype(*ty)),
        HAssert::Defined(t) => format!("HA_defined {}", paren_term(t, r)),
        HAssert::AppOk(f, args) => {
            format!("HA_app_ok {} {}", app_index(f, r), term_seq(args, r))
        }
    }
}

fn paren_assert(a: &HAssert, r: &FxHashMap<String, u32>) -> String {
    format!("({})", assert_str(a, r))
}

fn term_str(t: &HTerm, r: &FxHashMap<String, u32>) -> String {
    match t {
        HTerm::Const(c) => format!("T_const ({})", const_str(*c)),
        HTerm::LVar(i) => format!("T_lvar {i}"),
        HTerm::Local(i) => format!("T_local {i}%N"),
        HTerm::App(f, args) => format!("T_app {} {}", app_index(f, r), term_seq(args, r)),
        HTerm::Binop(ty, op, l, rr) => format!(
            "T_binop {} ({}) {} {}",
            numtype(*ty),
            binop(*op),
            paren_term(l, r),
            paren_term(rr, r)
        ),
        HTerm::Relop(ty, op, l, rr) => format!(
            "T_relop {} ({}) {} {}",
            numtype(*ty),
            relop(*op),
            paren_term(l, r),
            paren_term(rr, r)
        ),
    }
}

fn paren_term(t: &HTerm, r: &FxHashMap<String, u32>) -> String {
    format!("({})", term_str(t, r))
}

/// A `seq term` argument list: `nil` when empty, `(t1 :: t2 :: nil)` otherwise,
/// each element parenthesized so cons never binds inside an application.
fn term_seq(args: &[HTerm], r: &FxHashMap<String, u32>) -> String {
    if args.is_empty() {
        return "nil".to_string();
    }
    let mut list = String::from("(");
    for arg in args {
        list.push_str(&paren_term(arg, r));
        list.push_str(" :: ");
    }
    list.push_str("nil)");
    list
}

/// The resolved `mod_funcs` index of an applied function symbol.
fn app_index(f: &HFnRef, r: &FxHashMap<String, u32>) -> u32 {
    r.get(&f.0)
        .copied()
        .expect("every applied symbol is resolved before printing")
}

fn numtype(ty: HNumType) -> &'static str {
    match ty {
        HNumType::I32 => "T_i32",
        HNumType::I64 => "T_i64",
    }
}

fn binop(op: HBinop) -> &'static str {
    match op {
        HBinop::Add => "Binop_i BOI_add",
        HBinop::Sub => "Binop_i BOI_sub",
        HBinop::Mul => "Binop_i BOI_mul",
        HBinop::DivS => "Binop_i (BOI_div SX_S)",
        HBinop::DivU => "Binop_i (BOI_div SX_U)",
        HBinop::RemS => "Binop_i (BOI_rem SX_S)",
        HBinop::RemU => "Binop_i (BOI_rem SX_U)",
        HBinop::And => "Binop_i BOI_and",
        HBinop::Or => "Binop_i BOI_or",
        HBinop::Xor => "Binop_i BOI_xor",
        HBinop::Shl => "Binop_i BOI_shl",
        HBinop::ShrS => "Binop_i (BOI_shr SX_S)",
        HBinop::ShrU => "Binop_i (BOI_shr SX_U)",
    }
}

fn relop(op: HRelop) -> &'static str {
    match op {
        HRelop::Eq => "Relop_i ROI_eq",
        HRelop::Ne => "Relop_i ROI_ne",
        HRelop::LtS => "Relop_i (ROI_lt SX_S)",
        HRelop::LtU => "Relop_i (ROI_lt SX_U)",
        HRelop::GtS => "Relop_i (ROI_gt SX_S)",
        HRelop::GtU => "Relop_i (ROI_gt SX_U)",
        HRelop::LeS => "Relop_i (ROI_le SX_S)",
        HRelop::LeU => "Relop_i (ROI_le SX_U)",
        HRelop::GeS => "Relop_i (ROI_ge SX_S)",
        HRelop::GeU => "Relop_i (ROI_ge SX_U)",
    }
}

/// Renders a numeric constant as the emitted `Vi32`/`Vi64` helper application.
fn const_str(c: HConst) -> String {
    match c {
        HConst::I32(v) => format!("Vi32 {}", z_literal(i64::from(v))),
        HConst::I64(v) => format!("Vi64 {}", z_literal(v)),
    }
}
