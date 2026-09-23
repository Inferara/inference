//! Whether the two gates that state the Stellar admissibility policy agree.
//!
//! The policy — how many parameters a contract method may take, how long its
//! name may be, which prefix is reserved, which types may cross the boundary,
//! whether each parameter is named and how long that name may be — is stated
//! twice. `inference-wasm-codegen` states it over the program the author wrote,
//! before any file is produced. `inference-stellar-abi` states it again over
//! the linked module and its export descriptor, where things the first gate
//! cannot see arrive. The duplication is structural and stays:
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
//! #324 for the work that would lift the restriction. The cost is the quality
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
//! That module then reaches the rewriter the way a shipped artifact does, which
//! is two paths rather than one. A program binding no host import goes through
//! `inference::link`, whose merge over no externals is the documented
//! byte-identical pass-through. A program binding one is returned by
//! `inference::link_resolved` unchanged — its imports are meant to survive for
//! an embedder, so there is nothing to merge — and a merge entry point handed
//! its bytes fails closed on the unsatisfied import instead. Both paths end at
//! code generation's own bytes, so the rewriter judges the same module either
//! way; the split is here because taking the wrong one turns a host row into a
//! link failure that never reaches the gate under test.
//!
//! # Three rules this file cannot reach
//!
//! Both gates refuse an empty export name, a name outside `[A-Za-z0-9_]`, and
//! an empty parameter name, which each treats as no name at all. None is
//! expressible in Inference source: an export name and a parameter name are
//! identifiers, and the lexer spells an identifier `[A-Za-z_][A-Za-z0-9_]*` —
//! never empty, and never carrying a character the two gates would refuse.
//! Faking one would mean hand-assembling a module, which measures the rewriter
//! against a hand-written descriptor rather than measuring the two gates
//! against one program. Those three rules stay unit-tested inside each
//! crate — the empty parameter name by the two
//! `an_empty_parameter_name_is_refused_as_unnamed` tests, one in
//! `inference-wasm-codegen`'s `stellar_gate_tests` and one in
//! `inference-stellar-abi`'s `rewrite` module.
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
    /// Which of the two shipping routes puts this row's module in front of the
    /// rewriter. See the module documentation for the two.
    binds_a_host: bool,
}

fn row(label: &'static str, source: &str, admissible: bool) -> Row {
    Row {
        label,
        source: source.to_string(),
        admissible,
        binds_a_host: false,
    }
}

/// A row whose program binds a host import.
///
/// Its own constructor rather than a fourth argument on every call: one row of
/// the matrix takes the other route to the rewriter, and a bare `false` on
/// every other row would say nothing to a reader about which route that is.
fn host_row(label: &'static str, source: &str, admissible: bool) -> Row {
    Row {
        label,
        source: source.to_string(),
        admissible,
        binds_a_host: true,
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

/// An exported function whose one parameter is named in `len` ASCII bytes, of
/// an admissible type, so the parameter-name rule is the only one the row can
/// trip.
fn parameter_named(len: usize) -> String {
    let name: String = std::iter::repeat_n('p', len).collect();
    format!("pub fn f({name}: u32) -> u32 {{ return {name}; }}")
}

/// The matrix: every rule both gates state, in every form Inference source can
/// express, on both sides of each boundary.
///
/// A return row declares an admissible parameter. Written with a parameter of
/// the type under test, the source gate refuses at the parameter and never
/// reaches its return check, so the row would agree about a rule it never ran.
/// A parameter-name row declares admissible types for the same reason: both
/// gates check every type before any name, so a row whose unnamed parameter
/// were a `u64` would agree about the type rule and never reach the name one.
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
        row("an unnamed parameter", "pub fn f(_: u32) -> u32 { return 1; }", false),
        row("a 30-byte parameter name", &parameter_named(30), true),
        row("a 31-byte parameter name", &parameter_named(31), false),
        row(
            "a leading-underscore parameter name",
            "pub fn f(_amount: u32) -> u32 { return _amount; }",
            true,
        ),
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
        // The one rule stated about what a program *imports* rather than about
        // what it exports. The source gate refuses the binding; the rewriter
        // refuses the import that binding emits, having no way to know which
        // declaration it came from. Its module reaches the rewriter unlinked,
        // because that is what the shipping path does with a host program.
        host_row(
            "a host import",
            "external fn put(k: i32, v: i32) -> i32; \
             use { put } from host::l; \
             pub fn store(k: i32, v: i32) -> i32 { return put(k, v); }",
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
            | StellarAbiError::UnnamedParameter { .. }
            | StellarAbiError::ParameterNameTooLong { .. }
            | StellarAbiError::UnsupportedReturn { .. }
            | StellarAbiError::CompoundReturn { .. }
            | StellarAbiError::ImportsUnsupported { .. }
    )
}

/// The verdict of the Val-ABI rewriter on the same source, reached without the
/// source gate having seen it.
fn val_abi_rewriter(row: &Row) -> Verdict {
    match rewriter_outcome(row) {
        Ok(()) => Verdict::Admitted,
        Err(refusal) => Verdict::Refused(refusal.to_string()),
    }
}

/// What the Val-ABI rewriter says about a row's program: nothing, or the
/// admissibility rule it refused it under.
///
/// Which of the two routes the module takes is the row's own: a host program's
/// shipped artifact is code generation's bytes, and handing those to the merge
/// would fail closed on the import no external satisfies, well before the gate
/// under test was reached.
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
fn rewriter_outcome(row: &Row) -> Result<(), StellarAbiError> {
    let Row {
        label,
        source,
        binds_a_host,
        ..
    } = row;
    let built = codegen_for_target(source, Target::Wasm32).unwrap_or_else(|e| {
        panic!(
            "row '{label}': the default build must succeed to put this row in front of the \
             rewriter, but it failed: {e}"
        )
    });
    let linked = if *binds_a_host {
        built.wasm().to_vec()
    } else {
        inference::link(built.wasm(), &[], None)
            .unwrap_or_else(|e| panic!("row '{label}': the link must succeed, but it failed: {e}"))
    };

    match rewrite(&linked, built.export_signatures(), STELLAR_ENV_PROTOCOL) {
        Ok(_) => Ok(()),
        Err(refusal) => {
            assert!(
                is_admissibility_refusal(&refusal),
                "row '{label}': the rewriter refused it over something other than method \
                 admissibility, so the row measures the module rather than the policy: {refusal}"
            );
            Err(refusal)
        }
    }
}

/// The parity claim: one program, one answer, whichever gate is asked.
#[test]
fn the_source_gate_and_the_rewriter_agree_on_every_row() {
    for row in rows() {
        let Row { label, source, .. } = &row;
        let gate = source_gate(source);
        let rewriter = val_abi_rewriter(&row);
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
        ..
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

/// Whether a refusal of the rewriter's is the rule a row expects.
type RuleMatcher = fn(&StellarAbiError) -> bool;

/// A program breaking two rules is refused for the same one of them by both
/// gates.
///
/// Each gate pins its refusal order in a unit test of its own, and a unit test
/// moves with the order it pins: reorder one gate's checks, update its test,
/// and both suites stay green while the two gates name different rules for one
/// program. Each row here breaks two rules, and both gates must report the
/// earlier one — the rewriter by its variant, the source gate in the words its
/// message spells that rule with. Every two rules adjacent in the order both
/// gates state — the method name, the parameter count, every parameter's type,
/// the return, every parameter's name, `_` before length — share a row, so
/// moving any one check of either gate makes some row disagree.
#[test]
fn a_program_breaking_two_rules_is_refused_for_the_same_one_by_both_gates() {
    let too_long = "p".repeat(31);
    let every_unnamed = vec!["_: u32"; 33].join(", ");
    let every_named: Vec<String> = (0..33).map(|index| format!("p{index}: u32")).collect();
    let mut last_is_u64 = every_named.clone();
    last_is_u64[32] = "p32: u64".to_string();
    let cases: Vec<(&'static str, String, RuleMatcher, &str)> = vec![
        (
            "a reserved name, and 33 parameters",
            format!("pub fn __wide({}) -> u32 {{ return p0; }}", every_named.join(", ")),
            |refusal| matches!(refusal, StellarAbiError::ReservedExportName { .. }),
            "a prefix the host reserves",
        ),
        (
            "a reserved name, and an unnamed parameter",
            "pub fn __f(_: u32) -> u32 { return 1; }".to_string(),
            |refusal| matches!(refusal, StellarAbiError::ReservedExportName { .. }),
            "a prefix the host reserves",
        ),
        (
            "33 parameters, the last a u64",
            format!("pub fn wide({}) -> u32 {{ return p0; }}", last_is_u64.join(", ")),
            |refusal| matches!(refusal, StellarAbiError::TooManyParameters { count: 33, .. }),
            "takes 33 parameters",
        ),
        (
            "33 parameters, every one unnamed",
            format!("pub fn wide({every_unnamed}) -> u32 {{ return 1; }}"),
            |refusal| matches!(refusal, StellarAbiError::TooManyParameters { count: 33, .. }),
            "takes 33 parameters",
        ),
        (
            "a u64 parameter, and a u64 return",
            "pub fn f(a: u64) -> u64 { return a; }".to_string(),
            |refusal| matches!(refusal, StellarAbiError::UnsupportedParameter { position: 1, .. }),
            "parameter 1 'a' is declared 'u64'",
        ),
        (
            "an unnamed parameter, then a u64 one",
            "pub fn f(_: u32, b: u64) -> u32 { return 1; }".to_string(),
            |refusal| matches!(refusal, StellarAbiError::UnsupportedParameter { position: 2, .. }),
            "parameter 2 'b' is declared 'u64'",
        ),
        (
            "an unnamed parameter, and a u64 return",
            "pub fn f(_: u32) -> u64 { return 1; }".to_string(),
            |refusal| matches!(refusal, StellarAbiError::UnsupportedReturn { .. }),
            "it returns 'u64'",
        ),
        (
            "an over-long parameter name, then an unnamed one",
            format!("pub fn f({too_long}: u32, _: u32) -> u32 {{ return {too_long}; }}"),
            |refusal| matches!(refusal, StellarAbiError::UnnamedParameter { position: 2, .. }),
            "parameter 2 is written '_'",
        ),
    ];
    for (label, source, is_the_earlier_rule, fragment) in cases {
        let gate = source_gate(&source);
        assert!(
            matches!(&gate, Verdict::Refused(message) if message.contains(fragment)),
            "'{label}': the source gate must refuse it for the earlier rule, saying \
             '{fragment}', but it {gate}"
        );
        let rewriter = rewriter_outcome(&row(label, &source, false));
        assert!(
            rewriter.as_ref().is_err_and(is_the_earlier_rule),
            "'{label}': the rewriter must refuse it for the rule the source gate named \
             ('{fragment}'), but it returned {rewriter:?}"
        );
    }
}
