//! The second flag the `stellar` CLI gives a contract method's parameter, and
//! the rule both gates build on it, held against the CLI's own algorithm and
//! against what the CLI measurably did.
//!
//! `stellar contract invoke` (stellar-cli 28.0.0, `build_custom_cmd` in
//! `cmd/soroban-cli/src/commands/contract/arg_parsing.rs`) gives every
//! parameter the flag `--<name>` and, as an alias, the name converted by
//! `heck` 0.5.0's `to_kebab_case`, and resolves a flag to whichever parameter
//! claims it first. Both gates refuse a method one of whose names is another
//! parameter's alias, or two of whose names are identical, and both compute the
//! alias with `inference_wasm_codegen::stellar_cli_flag_alias`, a transcription
//! of heck's algorithm. This module holds that transcription equal to `heck`
//! itself, and holds the gates' verdicts equal to the reliability
//! `MEASURED_ABI.md` records for each method the CLI was driven against.

use heck::ToKebabCase;
use inference_stellar_abi::StellarAbiError;
use inference_tests::corpus::{codegen_for_target, rewrite_default_build_for_stellar};
use inference_wasm_codegen::{Target, stellar_cli_flag_alias};

use crate::contracts::{fixture_names, fixture_source};

/// Identifiers chosen for the rules `heck` splits and lowercases by: runs of
/// underscores in every position, capitals alone, in runs and before a
/// lowercase letter, digits between cases, and plain names. The last few are
/// no Inference identifier, but a hand-built descriptor can carry any string,
/// and the transcription claims heck's answer for every one.
const IDENTIFIERS: [&str; 60] = [
    // underscores
    "_x", "__x", "x_", "x__", "_x_", "__", "___", "_", "_a_b_", "x__y", "a__b__c", "__init__",
    // case
    "X", "Amount", "toAddr", "ToAddr", "HTTPServer", "XMLHttpRequest", "IOError", "aB", "ABc",
    "ABC", "aBC", "AbC", "A", "camelCaseName", "PascalCase", "SHOUTY_SNAKE", "mixed_Case_Name",
    "getHTTPResponseCode", "x_Y", "_X_",
    // digits
    "x2", "a1B", "v2Beta", "_1x", "x1_2", "x1", "x_1", "a1b2C3", "V2", "HTTP2Server", "utf8Str",
    "u32Value", "ABC123def", "A1_",
    // plain
    "x", "y", "a", "to_addr", "_to_addr", "long_snake_name", "amount", "amountX",
    // strings no identifier spells
    "1x", "é", "Éa", "aÉ", "ΑΣ", "straße",
];

/// Every parameter name the exported functions of the fixtures under
/// `tests/test_data/stellar` declare, read off the descriptor code generation
/// records for each.
fn fixture_parameter_names() -> Vec<String> {
    let mut names = Vec::new();
    for fixture in fixture_names() {
        let output = codegen_for_target(&fixture_source(fixture), Target::Stellar)
            .unwrap_or_else(|e| panic!("{fixture} must compile for the Stellar target: {e}"));
        for signature in output.export_signatures() {
            names.extend(signature.params.iter().filter_map(|param| param.name.clone()));
        }
    }
    names
}

/// The transcription is `heck`'s `to_kebab_case`, name for name, over the
/// table above and every parameter name the fixtures declare. Every name is
/// compared before the test fails, so a failure lists each disagreement.
#[test]
fn the_flag_alias_is_hecks_kebab_case_of_every_name() {
    let fixture_names = fixture_parameter_names();
    assert!(
        fixture_names.len() >= 32,
        "the fixtures declare at least `max_arity`'s 32 parameters, found {}",
        fixture_names.len()
    );
    let disagreements: Vec<String> = IDENTIFIERS
        .iter()
        .copied()
        .chain(fixture_names.iter().map(String::as_str))
        .filter_map(|name| {
            let ours = stellar_cli_flag_alias(name);
            let hecks = name.to_kebab_case();
            (ours != hecks).then(|| format!("`{name}`: ours `{ours}`, heck's `{hecks}`"))
        })
        .collect();
    assert!(disagreements.is_empty(), "the alias disagrees with heck 0.5.0: {disagreements:#?}");
}

/// The fixture the CLI measurement of five methods was taken on, as
/// `MEASURED_ABI.md` quotes it: one method per line.
const ALIAS_FIXTURE: &str = "\
pub fn f1(_x: u32, __x: u32) -> u32 { return _x * 10 + __x; }
pub fn f2(x: u32, X: u32) -> u32 { return x * 10 + X; }
pub fn f3(to_addr: u32, _to_addr: u32) -> u32 { return to_addr * 10 + _to_addr; }
pub fn f4(to_addr: u32, toAddr: u32) -> u32 { return to_addr * 10 + toAddr; }
pub fn f5(x: u32, x_: u32) -> u32 { return x * 10 + x_; }
";

/// The fixture of the earlier measurement, of a method declaring `x` and `_x`,
/// as `MEASURED_ABI.md` quotes it.
const BOTH_FIXTURE: &str = "\
pub fn both(x: u32, _x: u32) -> u32 {
    return x * 10 + _x;
}
";

/// What the CLI did with each measured method, called through the flags its
/// `--help` lists for it: the calls that succeeded, of the calls made. `both`
/// was called twice with both flags, and refused both times; each of the
/// others eight times, on a deployed contract.
const MEASURED: [(&str, usize, usize); 6] = [
    ("both", 0, 2),
    ("f1", 8, 8),
    ("f2", 2, 8),
    ("f3", 8, 8),
    ("f4", 8, 8),
    ("f5", 3, 8),
];

/// The measured record, as committed beside this module.
const RECORD: &str = include_str!("MEASURED_ABI.md");

/// The two fixtures and every count in [`MEASURED`] are the ones
/// `MEASURED_ABI.md` quotes, so the verdicts below rest on the record rather
/// than on a second copy of it: each of `f1`–`f5` has one line summing its
/// calls through its listed flags, and `both` is called with both flags in
/// the transcript, each call answered with an error.
#[test]
fn the_measured_calls_are_the_ones_the_record_quotes() {
    assert!(RECORD.contains(ALIAS_FIXTURE), "the record quotes the alias fixture verbatim");
    assert!(RECORD.contains(BOTH_FIXTURE), "the record quotes the `both` fixture verbatim");
    let lines: Vec<&str> = RECORD.lines().collect();
    for (method, succeeded, calls) in MEASURED {
        if method == "both" {
            let answers: Vec<&str> = lines
                .windows(2)
                .filter(|pair| {
                    pair[0].starts_with("$ stellar contract invoke")
                        && pair[0].contains("-- both --")
                        && pair[0].contains("--x ")
                        && pair[0].contains("--_x ")
                })
                .map(|pair| pair[1])
                .collect();
            let refused = answers.iter().filter(|answer| answer.starts_with("error: ")).count();
            let recorded = (answers.len() - refused, answers.len());
            assert_eq!(
                (succeeded, calls),
                recorded,
                "`both` was called with both flags, and answered {answers:?}"
            );
            continue;
        }
        let prefix = format!("  {method} long flags");
        let summaries: Vec<&str> =
            lines.iter().copied().filter(|line| line.starts_with(&prefix)).collect();
        let summed = format!("-> succeeded {succeeded}/{calls}");
        assert!(
            matches!(summaries.as_slice(), [summary] if summary.contains(&summed)),
            "`{method}`: the record sums its calls as {summaries:?}, not `{summed}`"
        );
    }
}

/// One measured method's source: its line of [`ALIAS_FIXTURE`], or
/// [`BOTH_FIXTURE`].
fn measured_method(method: &str) -> String {
    if method == "both" {
        return BOTH_FIXTURE.to_string();
    }
    let prefix = format!("pub fn {method}(");
    let lines: Vec<&str> =
        ALIAS_FIXTURE.lines().filter(|line| line.starts_with(&prefix)).collect();
    let [line] = lines.as_slice() else {
        panic!("the alias fixture declares `{method}` {} times, not once", lines.len())
    };
    (*line).to_string()
}

/// A method is admitted by both gates exactly when every measured call to it
/// succeeded: the ones whose calls failed some of the time — `x` beside `_x`,
/// `X` or `x_` — are refused, for the collision rule, and the ones whose calls
/// all succeeded — `_x` beside `__x`, `to_addr` beside `_to_addr` or `toAddr`
/// — are admitted.
#[test]
fn each_measured_method_is_refused_exactly_when_the_cli_failed_to_call_it() {
    for (method, succeeded, calls) in MEASURED {
        let source = measured_method(method);
        let reliable = succeeded == calls;

        let source_gate = codegen_for_target(&source, Target::Stellar).map(|_| ());
        assert_eq!(
            source_gate.is_ok(),
            reliable,
            "`{method}` succeeded in {succeeded} of {calls} calls; the source gate said \
             {source_gate:?}"
        );
        if let Err(refusal) = &source_gate {
            assert!(
                refusal.to_string().contains("claim one `stellar` CLI flag"),
                "`{method}` must be refused for the collision rule: {refusal}"
            );
        }

        let rewriter = rewrite_default_build_for_stellar(&source)
            .unwrap_or_else(|e| panic!("the default build of `{method}` must link: {e}"));
        match rewriter {
            Ok(_) => assert!(reliable, "`{method}` failed some calls; the rewriter admitted it"),
            Err(StellarAbiError::ParameterNamesCollide { .. }) => {
                assert!(!reliable, "`{method}` succeeded in every call; the rewriter refused it");
            }
            Err(other) => panic!("`{method}` must be refused for the collision rule: {other}"),
        }
    }
}

/// The measured fixture as a whole is refused for its first method the CLI
/// failed to call, `f2`: the gates check one export at a time, in order.
#[test]
fn the_measured_fixture_is_refused_for_its_first_unreliable_method() {
    let refusal = codegen_for_target(ALIAS_FIXTURE, Target::Stellar)
        .err()
        .map(|e| e.to_string());
    assert!(
        refusal.as_deref().is_some_and(|message| message.contains(
            "exported function 'f2' cannot be a contract method because parameter 1 'x' and \
             parameter 2 'X' claim one `stellar` CLI flag"
        )),
        "the source gate said {refusal:?}"
    );
    let rewriter = rewrite_default_build_for_stellar(ALIAS_FIXTURE)
        .unwrap_or_else(|e| panic!("the default build must link: {e}"));
    assert_eq!(
        rewriter.err(),
        Some(StellarAbiError::ParameterNamesCollide {
            export: "f2".to_string(),
            first: 1,
            second: 2,
            first_name: "x".to_string(),
            second_name: "X".to_string(),
            flag: "x".to_string(),
        })
    );
}
