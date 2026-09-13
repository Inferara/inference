//! Whether the two gates that state the Stellar admissibility policy agree.
//!
//! The policy — how many parameters a contract method may take, how long its
//! name may be, which prefix is reserved, which types may cross the boundary —
//! is stated twice. `inference-wasm-codegen` states it over the program the
//! author wrote, before any file is produced. `inference-stellar-abi` states it
//! again over the linked module and its export descriptor, where things the
//! first gate cannot see arrive. The duplication is structural and stays:
//! `inference-wasm-codegen` cannot depend on `inference-stellar-abi`, because
//! the dependency runs the other way.
//!
//! Nothing compared the two. Each crate's own boundary tests are written in
//! terms of its own constant — the source gate's against
//! `STELLAR_MAX_EXPORT_NAME_BYTES`, the rewriter's against
//! `MAX_VAL_PARAMETERS` — so editing a constant moves its own boundary test
//! with it and both suites stay green through the drift. They do not merely
//! fail to compare the two limits; they individually cannot notice that one
//! moved.
//!
//! # What drift would cost
//!
//! Not correctness. The rewriter fails closed, so no disagreement reachable
//! here uploads a broken contract. If the source gate alone were loosened, a
//! build would pass code generation and then die in the rewriter with a
//! byte-level message about exports, in place of the source-located one that
//! names the function, the parameter, the declared type, and points at issue
//! #464 for the work that would lift the restriction. The cost is the quality
//! of the message a user gets.
//!
//! # How a refused program is put in front of the rewriter
//!
//! The source gate runs only at the Stellar target, so a program it refuses
//! never produces a module for the rewriter to judge, and every refused row
//! would be untestable. Building the same source at the default target skips
//! the gate and yields both a module and the same export descriptor — the
//! descriptor is populated at every target, off the source declarations — so
//! what the rewriter is handed is what it would have been handed had the gate
//! let the program through. That the two builds hold the same computation is
//! established next door, by
//! `the_default_build_and_the_stellar_build_agree_on_every_fixture`.
//!
//! # Two rules this file cannot reach
//!
//! Both gates refuse an empty export name and a name outside `[A-Za-z0-9_]`.
//! Neither is expressible in Inference source: an export name is an identifier,
//! and the lexer spells an identifier `[A-Za-z_][A-Za-z0-9_]*` — never empty,
//! and never carrying a character the two gates would refuse. Faking one would
//! mean hand-assembling a module, which measures the rewriter against a
//! hand-written descriptor rather than measuring the two gates against one
//! program. Those two rules stay unit-tested inside each crate.
//!
//! # Why its own integration target
//!
//! Not build isolation — `soroban-env-host` is a dev-dependency of the whole
//! package, so a separate binary buys none. The `stellar` binary beside this
//! one declares itself the Soroban host tier, uploading and invoking modules in
//! process against a real host. This test uploads nothing and needs no host,
//! and does not share that stated purpose.

use inference_stellar_abi::{STELLAR_ENV_PROTOCOL, StellarAbiError, rewrite};
use inference_tests::corpus::codegen_for_target;
use inference_wasm_codegen::Target;

/// What one gate said about one program.
enum Verdict {
    Admitted,
    /// The refusing gate's own message, carried so a red assertion shows both
    /// sides rather than two booleans.
    Refused(String),
}

impl Verdict {
    fn admits(&self) -> bool {
        matches!(self, Self::Admitted)
    }
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Admitted => f.write_str("admitted it"),
            Self::Refused(message) => write!(f, "refused it with: {message}"),
        }
    }
}

/// One program, and where the policy puts it.
struct Row {
    /// The rule the row exercises, so a red assertion says which policy drifted
    /// rather than which string failed.
    label: &'static str,
    source: String,
    /// What the policy admits today.
    ///
    /// Pinned rather than left implicit because the parity assertion by itself
    /// is satisfied by two gates that check nothing: remove both and every row
    /// is admitted, every row agrees, and the matrix reads as full coverage
    /// while measuring nothing at all.
    admissible: bool,
}

fn row(label: &'static str, source: &str, admissible: bool) -> Row {
    Row {
        label,
        source: source.to_string(),
        admissible,
    }
}

/// An exported function taking `count` admissible parameters, for the arity
/// boundary. `count` is at least one, since the body names the first parameter.
fn arity(count: usize) -> String {
    let params: Vec<String> = (0..count).map(|index| format!("p{index}: u32")).collect();
    format!("pub fn wide({}) -> u32 {{ return p0; }}", params.join(", "))
}

/// An exported function whose name is `len` ASCII bytes, all of them inside the
/// symbol charset and none of them leading underscores, so the name-length rule
/// is the only one the row can trip.
fn named(len: usize) -> String {
    let name: String = std::iter::repeat_n('m', len).collect();
    format!("pub fn {name}(v: u32) -> u32 {{ return v; }}")
}

/// The matrix: every rule both gates state, in every form Inference source can
/// express, on both sides of each boundary.
///
/// A return row declares an admissible parameter. Written with a parameter of
/// the type under test, the source gate refuses at the parameter and never
/// reaches its return check, so the row would agree about a rule it never ran.
fn rows() -> Vec<Row> {
    let rec = "struct Rec { a: u32; b: u32; }";
    let gate = "enum Gate { Off, On }";
    vec![
        row("bool as a parameter", "pub fn f(v: bool) -> u32 { return 1; }", true),
        row("i32 as a parameter", "pub fn f(v: i32) -> u32 { return 1; }", true),
        row("u32 as a parameter", "pub fn f(v: u32) -> u32 { return v; }", true),
        row("bool as a return", "pub fn f(v: u32) -> bool { return true; }", true),
        row("i32 as a return", "pub fn f(v: u32) -> i32 { return 1; }", true),
        row("u32 as a return", "pub fn f(v: u32) -> u32 { return v; }", true),
        row("unit as a return", "pub fn f(v: u32) { let held: u32 = v; }", true),
        row("u8 as a parameter", "pub fn f(v: u8) -> u32 { return 1; }", false),
        row("i8 as a parameter", "pub fn f(v: i8) -> u32 { return 1; }", false),
        row("u16 as a parameter", "pub fn f(v: u16) -> u32 { return 1; }", false),
        row("i16 as a parameter", "pub fn f(v: i16) -> u32 { return 1; }", false),
        row("u8 as a return", "pub fn f(v: u32) -> u8 { return 1; }", false),
        row("i8 as a return", "pub fn f(v: u32) -> i8 { return 1; }", false),
        row("u16 as a return", "pub fn f(v: u32) -> u16 { return 1; }", false),
        row("i16 as a return", "pub fn f(v: u32) -> i16 { return 1; }", false),
        row("i64 as a parameter", "pub fn f(v: i64) -> u32 { return 1; }", false),
        row("u64 as a parameter", "pub fn f(v: u64) -> u32 { return 1; }", false),
        row("i64 as a return", "pub fn f(v: u32) -> i64 { return 1; }", false),
        row("u64 as a return", "pub fn f(v: u32) -> u64 { return 1; }", false),
        row(
            "a struct as a parameter",
            &format!("{rec}\npub fn f(r: Rec) -> u32 {{ return r.a; }}"),
            false,
        ),
        row(
            "a struct as a return",
            &format!(
                "{rec}\npub fn f(v: u32) -> Rec {{ let r: Rec = Rec {{ a: v, b: v }}; return r; }}"
            ),
            false,
        ),
        row(
            "an array as a parameter",
            "pub fn f(xs: [u32; 2]) -> u32 { return xs[0]; }",
            false,
        ),
        row(
            "an array as a return",
            "pub fn f(v: u32) -> [u32; 2] { let xs: [u32; 2] = [v, v]; return xs; }",
            false,
        ),
        row(
            "an enum as a parameter",
            &format!("{gate}\npub fn f(g: Gate) -> bool {{ return g == Gate::On; }}"),
            false,
        ),
        row(
            "an enum as a return",
            &format!("{gate}\npub fn f(v: u32) -> Gate {{ return Gate::On; }}"),
            false,
        ),
        row("arity 32", &arity(32), true),
        row("arity 33", &arity(33), false),
        row("a name of 32 bytes", &named(32), true),
        row("a name of 33 bytes", &named(33), false),
        // Two name rules both gates state have no row here and never will: an
        // empty name and a name outside `[A-Za-z0-9_]`. An export name is an
        // Inference identifier, and the identifier grammar spells neither. See
        // the module documentation for what covers them instead.
        row(
            "a name under the reserved prefix",
            "pub fn __hidden(v: u32) -> u32 { return v; }",
            false,
        ),
        row(
            "no exported function",
            "fn helper(v: u32) -> u32 { return v; }\nfn other(v: u32) -> u32 { return helper(v); }",
            false,
        ),
    ]
}

/// The verdict of the source-level gate, which runs inside code generation at
/// the Stellar target and nowhere else.
fn source_gate(source: &str) -> Verdict {
    match codegen_for_target(source, Target::Stellar) {
        Ok(_) => Verdict::Admitted,
        Err(refusal) => Verdict::Refused(refusal.to_string()),
    }
}

/// Whether a refusal is one of the admissibility rules this file compares, as
/// opposed to something about the module itself.
///
/// An allow-list rather than a list of the refusals to reject: a variant added
/// later is not an admissibility rule until someone says so here, which turns
/// an unclassified refusal into a red row instead of a silent one.
fn is_admissibility_refusal(refusal: &StellarAbiError) -> bool {
    matches!(
        refusal,
        StellarAbiError::NoExportedFunctions
            | StellarAbiError::EmptyExportName
            | StellarAbiError::ExportNameTooLong { .. }
            | StellarAbiError::ReservedExportName { .. }
            | StellarAbiError::ExportNameNotASymbol { .. }
            | StellarAbiError::TooManyParameters { .. }
            | StellarAbiError::UnsupportedParameter { .. }
            | StellarAbiError::UnsupportedReturn { .. }
            | StellarAbiError::CompoundReturn { .. }
    )
}

/// The verdict of the Val-ABI rewriter on the same source, reached without the
/// source gate having seen it.
///
/// # Panics
///
/// Panics when the default build or the link fails, and when the rewriter
/// refuses for anything other than an admissibility rule. The rewriter checks
/// the declared protocol and validates its input at the WebAssembly 1.0 feature
/// set before it reads a single export, and it refuses a descriptor that does
/// not match the module it came with — so a row can be refused by both sides
/// over something the policy never mentions. A row that agrees for the wrong
/// reason measures nothing while reading as coverage, which is worse than a row
/// that fails.
fn val_abi_rewriter(label: &str, source: &str) -> Verdict {
    let built = codegen_for_target(source, Target::Wasm32).unwrap_or_else(|e| {
        panic!(
            "row '{label}': the default build must succeed to put this row in front of the \
             rewriter, but it failed: {e}"
        )
    });
    let linked = inference::link(built.wasm(), &[], None)
        .unwrap_or_else(|e| panic!("row '{label}': the link must succeed, but it failed: {e}"));

    match rewrite(&linked, built.export_signatures(), STELLAR_ENV_PROTOCOL) {
        Ok(_) => Verdict::Admitted,
        Err(refusal) => {
            assert!(
                is_admissibility_refusal(&refusal),
                "row '{label}': the rewriter refused it over something other than method \
                 admissibility, so the row measures the module rather than the policy: {refusal}"
            );
            Verdict::Refused(refusal.to_string())
        }
    }
}

/// The parity claim: one program, one answer, whichever gate is asked.
#[test]
fn the_source_gate_and_the_rewriter_agree_on_every_row() {
    for Row { label, source, .. } in rows() {
        let gate = source_gate(&source);
        let rewriter = val_abi_rewriter(label, &source);
        assert_eq!(
            gate.admits(),
            rewriter.admits(),
            "row '{label}': the two gates state the same policy and have drifted apart. \
             The source gate {gate}, and the rewriter {rewriter}"
        );
    }
}

/// What the policy is, so that two gates agreeing on nothing cannot pass as two
/// gates agreeing.
///
/// The source gate alone: the assertion above carries the claim across to the
/// rewriter, so pinning the second one would restate it.
#[test]
fn every_row_lands_where_the_stated_policy_puts_it() {
    for Row {
        label,
        source,
        admissible,
    } in rows()
    {
        let gate = source_gate(&source);
        assert_eq!(
            gate.admits(),
            admissible,
            "row '{label}': the policy admits this program: {admissible}. The source gate \
             {gate}"
        );
    }
}
