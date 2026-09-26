//! The SpaceWasm benchmark, tested in pieces.
//!
//! The `spacewasm-bench` example is three lines over
//! [`bench::run`](../spacewasm/bench.rs), so this binary includes the same
//! module and drives what `run` is built from: the record format, the
//! history reader, the comparison and its report, and the measurement itself.
//! The comparison rows are hand-built history lines rather than the committed
//! file, so each states the one rule it is about; the committed file is only
//! read to hold it readable.
//!
//! Like the embed harness's CLI matrix, this binary declares no interpreter
//! allocator: `inference-spacewasm-runner` defines it once for every program
//! that links it.

#[path = "spacewasm/bench.rs"]
mod bench;
#[path = "spacewasm/fprime_programs.rs"]
mod fprime_programs;
#[path = "spacewasm/support.rs"]
mod support;

use std::num::NonZeroUsize;
use std::path::Path;

use bench::{
    Channel, Command, Comparison, DEFAULT_THRESHOLD_PERCENT, Ending, Finding, FprimeCall, Key,
    Measured, Measurement, Record, Stamp, Unmeasured, append, committed_history, compare,
    compare_with, exit, fprime_calls, measure, measure_fixture, measure_fprime, measure_programs,
    parse_history, record_with, report, utc_date,
};
use fprime_programs::SPIN;
use inference_spacewasm_runner::{
    self as runner, EngineConfig, Fuel, HostLog, LIMITS_FROM, Outcome, Value, fprime,
};
use inference_tests::corpus::{
    SpaceWasmBuild, relative_to_test_data, single_file_corpus_sources, spacewasm_build,
    test_data_path, wasm_for_target,
};
use inference_wasm_codegen::Target;
use rustc_hash::FxHashSet;
use support::SpaceWasmSession;

// ---------------------------------------------------------------------------
// Rigging
// ---------------------------------------------------------------------------

/// One history line, written out by hand: `program`'s `main`, called with no
/// arguments, returning after `instructions`, in a module of `ir_words`.
fn line(commit: &str, program: &str, instructions: usize, ir_words: usize) -> String {
    line_under(LIMITS_FROM, commit, program, "main", "returned", instructions, ir_words)
}

/// [`line`] with every part a row may vary.
fn line_under(
    interpreter: &str,
    commit: &str,
    program: &str,
    export: &str,
    outcome: &str,
    instructions: usize,
    ir_words: usize,
) -> String {
    format!(
        r#"{{"date":"2026-09-26","commit":"{commit}","interpreter":"{interpreter}","program":"{program}","export":"{export}","args":[],"outcome":"{outcome}","instructions":{instructions},"code_pages":1,"ir_words":{ir_words},"wasm_bytes":100,"ir_bytes_per_wasm_byte":0.5}}"#
    )
}

/// The history the lines make.
fn history(lines: &[String]) -> Vec<Record> {
    parse_history(&lines.join("\n")).unwrap_or_else(|e| panic!("the rows' history reads: {e}"))
}

/// A measurement made now of `program`'s `main`, called with no arguments.
fn measured(program: &str, instructions: usize, ir_words: usize) -> Measurement {
    measured_as(program, "main", Ending::Returned, instructions, ir_words)
}

/// [`measured`] with every part a row may vary.
fn measured_as(
    program: &str,
    export: &str,
    outcome: Ending,
    instructions: usize,
    ir_words: usize,
) -> Measurement {
    Measurement {
        program: program.to_string(),
        export: export.to_string(),
        args: Vec::new(),
        outcome,
        instructions,
        code_pages: 1,
        ir_words,
        wasm_bytes: 100,
        ir_bytes_per_wasm_byte: 0.5,
    }
}

/// A run that measured every program it was to: `measurements`, and nothing
/// left unmeasured.
fn complete(measurements: &[Measurement]) -> Measured {
    Measured { measurements: measurements.to_vec(), unmeasured: Vec::new() }
}

/// What `program`'s `main` is known by.
fn key(program: &str) -> Key {
    Key { program: program.to_string(), export: "main".to_string(), args: Vec::new() }
}

/// The stamp every [`line`] carries, under `commit`.
fn stamped(commit: &str) -> Stamp {
    Stamp {
        date: "2026-09-26".to_string(),
        commit: commit.to_string(),
        interpreter: LIMITS_FROM.to_string(),
    }
}

/// The warning that `program`'s `main` took `to` instructions where the
/// snapshot `aaaaaaa` recorded `from`.
fn instructions_grew(program: &str, from: usize, to: usize) -> Finding {
    Finding::InstructionsGrew { key: key(program), from, to, at: stamped("aaaaaaa") }
}

/// The warning that `program` compiled to `to` IR words where the snapshot
/// `aaaaaaa` recorded `from`.
fn ir_grew(program: &str, from: usize, to: usize) -> Finding {
    Finding::IrGrew { program: program.to_string(), from, to, at: stamped("aaaaaaa") }
}

/// `current` compared with `lines` at the default threshold.
fn findings(lines: &[String], current: &[Measurement]) -> Vec<Finding> {
    compare(&history(lines), &complete(current), LIMITS_FROM, DEFAULT_THRESHOLD_PERCENT).findings
}

// ---------------------------------------------------------------------------
// The record
// ---------------------------------------------------------------------------

/// A record is one line of JSON with its keys in the documented order, and
/// the reader gives back the record it was written from.
///
/// Fails if a key is renamed, dropped or moved, if an ending is spelled
/// otherwise, or if an argument stops being a string.
#[test]
fn a_record_is_one_line_of_json_in_the_documented_key_order() {
    let record = Record {
        stamp: Stamp {
            date: "2026-09-26".to_string(),
            commit: "3171415aa".to_string(),
            interpreter: "spacewasm 0.7.1".to_string(),
        },
        measurement: Measurement {
            program: "fprime/spin".to_string(),
            export: "spin".to_string(),
            args: vec!["1000".to_string(), "-1".to_string()],
            outcome: Ending::OutOfFuel,
            instructions: 22_027,
            code_pages: 2,
            ir_words: 300,
            wasm_bytes: 240,
            ir_bytes_per_wasm_byte: 2.5,
        },
    };
    let text = record.to_line();
    assert_eq!(
        text,
        r#"{"date":"2026-09-26","commit":"3171415aa","interpreter":"spacewasm 0.7.1","program":"fprime/spin","export":"spin","args":["1000","-1"],"outcome":"out_of_fuel","instructions":22027,"code_pages":2,"ir_words":300,"wasm_bytes":240,"ir_bytes_per_wasm_byte":2.5}"#
    );
    assert_eq!(parse_history(&text), Ok(vec![record]));
    for (ending, spelled) in [
        (Ending::Returned, r#""returned""#),
        (Ending::Trapped, r#""trapped""#),
        (Ending::OutOfFuel, r#""out_of_fuel""#),
    ] {
        assert_eq!(serde_json::to_string(&ending).expect("an ending serializes"), spelled);
    }
}

/// The reader skips blank lines, reads a history checked out with CRLF line
/// ends, and names the first line it cannot read.
///
/// Fails if a blank line or a carriage return refuses a history, or if a
/// refusal stops saying where the bad line is.
#[test]
fn the_history_reader_skips_blank_lines_and_names_a_bad_one() {
    let good = line("aaaaaaa", "a.inf", 10, 20);
    let text = format!("{good}\n\n  \n{good}\n");
    assert_eq!(parse_history(&text).map(|records| records.len()), Ok(2));
    let checked_out_with_crlf = format!("{good}\r\n\r\n{good}\r\n");
    assert_eq!(parse_history(&checked_out_with_crlf).map(|records| records.len()), Ok(2));

    let broken = format!("{good}\n\n{{\"date\":\"2026-09-26\"}}\n");
    let refusal = parse_history(&broken).expect_err("the third line is no record");
    assert!(refusal.starts_with("line 3 is not a record: "), "{refusal}");
    assert!(parse_history("not json").expect_err("no record").starts_with("line 1 "));
}

/// Lines are appended after what the history holds, on a line of their own
/// even when its last line has no line break, and to a new file when there is
/// none.
///
/// Fails if an appended record is glued onto the last one, or if a missing
/// history refuses the first snapshot.
#[test]
fn a_snapshot_is_appended_on_lines_of_its_own() {
    let dir = tempfile::TempDir::new().expect("a temporary directory");
    let path = dir.path().join("history.jsonl");
    let first = line("aaaaaaa", "a.inf", 10, 20);
    let second = line("bbbbbbb", "a.inf", 11, 20);

    append(&path, &format!("{first}\n")).expect("a new history is created");
    append(&path, &format!("{second}\n")).expect("the history takes a second snapshot");
    assert_eq!(std::fs::read_to_string(&path).expect("readable"), format!("{first}\n{second}\n"));

    std::fs::write(&path, &first).expect("writable");
    append(&path, &format!("{second}\n")).expect("the history takes a snapshot");
    assert_eq!(std::fs::read_to_string(&path).expect("readable"), format!("{first}\n{second}\n"));
}

/// The date is the UTC civil day of a second since the epoch.
///
/// Fails if a leap day, a century rule or the last second of a day is
/// misplaced.
#[test]
fn the_date_is_the_utc_civil_day() {
    for (seconds, day) in [
        (0, "1970-01-01"),
        (86_399, "1970-01-01"),
        (86_400, "1970-01-02"),
        (951_782_400, "2000-02-29"),
        (951_868_800, "2000-03-01"),
        (1_709_164_800, "2024-02-29"),
        (1_790_380_800, "2026-09-26"),
        (1_790_467_199, "2026-09-26"),
        (4_107_456_000, "2100-02-28"),
        (4_107_542_400, "2100-03-01"),
    ] {
        assert_eq!(utc_date(seconds), day, "{seconds}");
    }
}

// ---------------------------------------------------------------------------
// The comparison
// ---------------------------------------------------------------------------

/// A figure that grew past the threshold is warned about, one that grew up to
/// it is not, and a threshold other than the default is the one applied.
///
/// Growth is judged strictly past the share: 105 instructions against 100 is
/// five percent and quiet, 106 is past it. Fails if either figure stops being
/// judged, if the boundary moves by one, or if the threshold is ignored.
#[test]
fn growth_past_the_threshold_warns_and_growth_up_to_it_does_not() {
    let lines = [line("aaaaaaa", "a.inf", 100, 200)];

    assert_eq!(findings(&lines, &[measured("a.inf", 105, 210)]), []);
    assert_eq!(findings(&lines, &[measured("a.inf", 95, 150)]), [], "shrinking is quiet");
    assert_eq!(
        findings(&lines, &[measured("a.inf", 106, 211)]),
        [ir_grew("a.inf", 200, 211), instructions_grew("a.inf", 100, 106)]
    );

    let current = complete(&[measured("a.inf", 110, 220)]);
    let loose = compare(&history(&lines), &current, LIMITS_FROM, 10);
    assert_eq!(loose.findings, []);
    let current = complete(&[measured("a.inf", 101, 200)]);
    let tight = compare(&history(&lines), &current, LIMITS_FROM, 0);
    assert_eq!(tight.findings, [instructions_grew("a.inf", 100, 101)]);

    let from_nothing = [line("aaaaaaa", "a.inf", 0, 200)];
    assert_eq!(
        findings(&from_nothing, &[measured("a.inf", 1, 200)]),
        [instructions_grew("a.inf", 0, 1)]
    );
}

/// A module's IR is judged once, however many of its exports are measured,
/// at the first of them the history holds, while each export's instructions
/// are judged on their own.
///
/// Fails if a module whose IR grew is warned about once per export, or not at
/// all when a new export comes ahead of one with history.
#[test]
fn a_modules_ir_is_judged_once_however_many_exports_are_measured() {
    let lines = [
        line_under(LIMITS_FROM, "aaaaaaa", "a.inf", "f", "returned", 100, 200),
        line_under(LIMITS_FROM, "aaaaaaa", "a.inf", "g", "returned", 100, 200),
    ];
    let current = [
        measured_as("a.inf", "f", Ending::Returned, 200, 300),
        measured_as("a.inf", "g", Ending::Returned, 200, 300),
    ];
    let found = findings(&lines, &current);
    let ir: Vec<&Finding> =
        found.iter().filter(|finding| matches!(finding, Finding::IrGrew { .. })).collect();
    let instructions: Vec<&Finding> = found
        .iter()
        .filter(|finding| matches!(finding, Finding::InstructionsGrew { .. }))
        .collect();
    assert_eq!(ir.len(), 1, "{found:?}");
    assert_eq!(instructions.len(), 2, "{found:?}");

    let only_g = [line_under(LIMITS_FROM, "aaaaaaa", "a.inf", "g", "returned", 100, 200)];
    let new_f_first = [
        measured_as("a.inf", "f", Ending::Returned, 100, 300),
        measured_as("a.inf", "g", Ending::Returned, 100, 300),
    ];
    let f = Key { program: "a.inf".to_string(), export: "f".to_string(), args: Vec::new() };
    assert_eq!(
        findings(&only_g, &new_f_first),
        [Finding::New { key: f }, ir_grew("a.inf", 200, 300)],
        "a new export ahead of one with history does not use up the module's one judgement"
    );
}

/// Each measurement is compared with the last line of its own key, which is
/// not always a line of the latest snapshot.
///
/// The key measured 100 then 200 is compared with 200, and the key only an
/// earlier snapshot holds is compared with that snapshot. Fails if the first
/// line of a key is taken for its baseline, or if a key the latest snapshot
/// lacks goes uncompared.
#[test]
fn each_measurement_is_compared_with_the_last_line_of_its_key() {
    let lines = [
        line("aaaaaaa", "a.inf", 100, 200),
        line("aaaaaaa", "b.inf", 100, 200),
        line("bbbbbbb", "a.inf", 200, 200),
    ];
    assert_eq!(
        findings(&lines, &[measured("a.inf", 205, 200), measured("b.inf", 150, 200)]),
        [instructions_grew("b.inf", 100, 150)]
    );
}

/// A measurement the history has never seen is new, and one the latest
/// snapshot holds that was not measured is gone; one only an earlier snapshot
/// holds is neither, and an empty history makes every measurement new.
///
/// Fails if a new measurement is compared with nothing and passed over, if a
/// program that stopped being measured goes unmentioned, or if every program
/// ever measured is reported gone forever.
#[test]
fn new_and_gone_measurements_are_noted() {
    let lines = [
        line("aaaaaaa", "retired.inf", 10, 20),
        line("bbbbbbb", "kept.inf", 10, 20),
        line("bbbbbbb", "dropped.inf", 10, 20),
    ];
    assert_eq!(
        findings(&lines, &[measured("kept.inf", 10, 20), measured("added.inf", 10, 20)]),
        [
            Finding::New { key: key("added.inf") },
            Finding::Gone { key: key("dropped.inf"), at: stamped("bbbbbbb") },
        ]
    );

    let current = complete(&[measured("a.inf", 10, 20)]);
    let empty = compare(&[], &current, LIMITS_FROM, DEFAULT_THRESHOLD_PERCENT);
    assert_eq!(empty.findings, [Finding::New { key: key("a.inf") }]);
    assert_eq!(empty.latest, None);
}

/// A snapshot is told from another by its whole stamp, so a second snapshot
/// of one commit, recorded on another day, is the latest on its own.
///
/// Fails if what the latest snapshot lacks is taken from every snapshot of
/// its commit, which would report as gone a measurement the latest snapshot
/// dropped long ago, or keep reporting it.
#[test]
fn the_latest_snapshot_is_told_apart_by_its_whole_stamp() {
    let earlier = line("aaaaaaa", "dropped.inf", 10, 20);
    let later = line("aaaaaaa", "kept.inf", 10, 20).replace("2026-09-26", "2026-09-27");
    assert_eq!(findings(&[earlier, later], &[measured("kept.inf", 10, 20)]), []);
}

/// A call that ends another way is noted and its instructions are not
/// judged, since the two counts are of two different runs; its module's IR
/// still is.
///
/// Fails if a trap that became a return is warned about as a regression of
/// its count, or if the change goes unmentioned.
#[test]
fn a_call_that_ends_another_way_is_noted_and_not_judged() {
    let lines = [line_under(LIMITS_FROM, "aaaaaaa", "a.inf", "main", "trapped", 5, 20)];
    let current = [measured_as("a.inf", "main", Ending::Returned, 500, 40)];
    assert_eq!(
        findings(&lines, &current),
        [
            ir_grew("a.inf", 20, 40),
            Finding::OutcomeChanged {
                key: key("a.inf"),
                from: Ending::Trapped,
                to: Ending::Returned,
                at: stamped("aaaaaaa"),
            },
        ]
    );
}

/// A measurement with history only under another interpreter release is
/// counted as not comparable and never warned about, and a line under the
/// current release is still its baseline when a later line is not.
///
/// Fails if a count from one release is judged against another's, or if such
/// a measurement is reported as new.
#[test]
fn a_measurement_under_another_interpreter_release_is_not_compared() {
    let older = "spacewasm 0.7.0";
    let lines = [
        line_under(older, "aaaaaaa", "moved.inf", "main", "returned", 10, 20),
        line_under(LIMITS_FROM, "aaaaaaa", "kept.inf", "main", "returned", 100, 20),
        line_under(older, "bbbbbbb", "kept.inf", "main", "returned", 1, 20),
    ];
    let current = [measured("moved.inf", 1_000, 2_000), measured("kept.inf", 104, 20)];
    assert_eq!(
        findings(&lines, &current),
        [Finding::NotComparable { count: 1, interpreter: LIMITS_FROM.to_string() }]
    );
}

/// A comparison reads as GitHub annotations inside Actions — one counting the
/// warnings first, then a `::warning` per figure and a `::notice` per note,
/// with `%` encoded — and as plain lines elsewhere, each ending with a plain
/// summary.
///
/// Fails if a warning stops being an annotation in Actions, if the count
/// that leads them goes missing, if a `%` reaches GitHub raw, or if a finding
/// changes its wording.
#[test]
fn a_comparison_reads_as_annotations_in_actions_and_as_plain_lines_elsewhere() {
    let comparison = Comparison {
        findings: vec![
            Finding::New { key: key("added.inf") },
            Finding::InstructionsGrew {
                key: key("a.inf"),
                from: 1_200,
                to: 1_350,
                at: stamped("3171415aaaa"),
            },
        ],
        measured: 2,
        latest: Some(stamped("3171415aaaa")),
    };
    let history = committed_history();
    assert_eq!(
        report(&comparison, &history, 5, Channel::Actions),
        [
            "::warning title=SpaceWasm benchmark::1 of the figures measured grew past the 5%25 \
             threshold against tests/bench/spacewasm/history.jsonl, whose latest snapshot is \
             3171415 (2026-09-26); this job's log lists every one",
            "::warning title=SpaceWasm benchmark::a.inf: main() took 1350 interpreter \
             instructions, up 12.5%25 from 1200 since 3171415 (2026-09-26)",
            "::notice title=SpaceWasm benchmark::added.inf: main() is new: the history holds no \
             measurement of it",
            "spacewasm-bench: compared 2 measurements with tests/bench/spacewasm/history.jsonl, \
             whose latest snapshot is 3171415 (2026-09-26), at a 5% threshold: 1 grew past it, 0 \
             end another way, 1 new, 0 gone, 0 not comparable, 0 could not be measured",
        ]
    );
    assert_eq!(
        report(&comparison, &history, 5, Channel::Plain),
        [
            "warning: a.inf: main() took 1350 interpreter instructions, up 12.5% from 1200 since \
             3171415 (2026-09-26)",
            "note: added.inf: main() is new: the history holds no measurement of it",
            "spacewasm-bench: compared 2 measurements with tests/bench/spacewasm/history.jsonl, \
             whose latest snapshot is 3171415 (2026-09-26), at a 5% threshold: 1 grew past it, 0 \
             end another way, 1 new, 0 gone, 0 not comparable, 0 could not be measured",
        ]
    );
}

/// Every finding says what it is about, the figures and the snapshot it
/// compares with, and a quiet comparison prints its summary alone.
///
/// Fails if a finding's sentence loses a number or the snapshot, or if a
/// quiet comparison prints an annotation.
#[test]
fn every_finding_names_its_figures_and_its_snapshot() {
    let at = stamped("3171415aaaa");
    let spin = Key {
        program: "fprime/spin".to_string(),
        export: "spin".to_string(),
        args: vec!["1000".to_string()],
    };
    for (finding, text) in [
        (
            Finding::IrGrew { program: "a.inf".to_string(), from: 0, to: 40, at: at.clone() },
            "a.inf compiled to 40 IR words, up from 0 since 3171415 (2026-09-26)",
        ),
        (
            Finding::OutcomeChanged {
                key: spin.clone(),
                from: Ending::Returned,
                to: Ending::OutOfFuel,
                at: at.clone(),
            },
            "fprime/spin: spin(1000) now runs out of fuel; it returned at 3171415 (2026-09-26)",
        ),
        (
            Finding::Gone { key: spin, at },
            "fprime/spin: spin(1000) is gone: it was measured at 3171415 (2026-09-26) and is not \
             measured now",
        ),
        (
            Finding::NotComparable { count: 3, interpreter: "spacewasm 0.8.0".to_string() },
            "3 measurements have history only under another interpreter release than spacewasm \
             0.8.0, whose instructions and IR are not spacewasm 0.8.0's, so they are not \
             compared; `record` a snapshot to start its series",
        ),
    ] {
        assert_eq!(finding.to_string(), text);
    }

    let quiet = Comparison { findings: Vec::new(), measured: 7, latest: None };
    assert_eq!(
        report(&quiet, Path::new("h.jsonl"), 5, Channel::Actions),
        ["spacewasm-bench: compared 7 measurements with h.jsonl, which holds no snapshot yet, at \
          a 5% threshold: 0 grew past it, 0 end another way, 0 new, 0 gone, 0 not comparable, 0 \
          could not be measured"]
    );
}

/// Each kind of finding reaches its channel as a warning or as a note, and
/// the summary counts each kind apart.
///
/// A program that could not be measured and a figure that grew are warnings,
/// and the annotation leading them counts the two kinds apart; an ending that
/// changed, a new, a gone and a not comparable measurement are notes. Fails if
/// a kind moves between the two — an IR regression shown as a notice, say —
/// if the leading annotation counts one kind as the other, or if the summary
/// miscounts or swaps two kinds.
#[test]
fn every_kind_of_finding_reaches_its_channel_and_its_count() {
    let at = stamped("3171415aaaa");
    let unmeasured = |program: String| {
        Finding::Unmeasured(Unmeasured { program, why: "it does not load".to_string() })
    };
    let changed = |program: &str| Finding::OutcomeChanged {
        key: key(program),
        from: Ending::Returned,
        to: Ending::Trapped,
        at: at.clone(),
    };
    let gone = |program: &str| Finding::Gone { key: key(program), at: at.clone() };
    let mut findings: Vec<Finding> = (1..=5).map(|n| unmeasured(format!("u{n}.inf"))).collect();
    findings.extend([
        Finding::IrGrew { program: "ir.inf".to_string(), from: 10, to: 20, at: at.clone() },
        changed("c1.inf"),
        changed("c2.inf"),
        gone("g1.inf"),
        gone("g2.inf"),
        gone("g3.inf"),
        Finding::New { key: key("n.inf") },
        Finding::NotComparable { count: 4, interpreter: LIMITS_FROM.to_string() },
    ]);
    let warnings: Vec<bool> = findings.iter().map(Finding::is_warning).collect();
    assert_eq!(
        warnings,
        [true, true, true, true, true, true, false, false, false, false, false, false, false]
    );
    let comparison = Comparison { findings, measured: 9, latest: Some(at.clone()) };

    let actions = report(&comparison, Path::new("h.jsonl"), 5, Channel::Actions);
    let prefixes: Vec<&str> = actions.iter().map(|line| &line[..line.len().min(9)]).collect();
    assert_eq!(
        prefixes,
        [
            "::warning", "::warning", "::warning", "::warning", "::warning", "::warning",
            "::warning", "::notice ", "::notice ", "::notice ", "::notice ", "::notice ",
            "::notice ", "::notice ", "spacewasm",
        ]
    );
    assert_eq!(
        actions[0],
        "::warning title=SpaceWasm benchmark::5 of the programs could not be measured, and 1 of \
         the figures measured grew past the 5%25 threshold against h.jsonl, whose latest snapshot \
         is 3171415 (2026-09-26); this job's log lists every one"
    );
    assert!(actions[1].ends_with("::u1.inf is not measured: it does not load"), "{}", actions[1]);
    assert!(actions[6].contains("::ir.inf compiled to 20 IR words"), "{}", actions[6]);
    let summary = "1 grew past it, 2 end another way, 1 new, 3 gone, 4 not comparable, 5 could \
                   not be measured";
    assert!(actions[14].ends_with(summary), "{}", actions[14]);

    let plain = report(&comparison, Path::new("h.jsonl"), 5, Channel::Plain);
    let kinds: Vec<&str> = plain.iter().map(|line| line.split(':').next().unwrap_or("")).collect();
    assert_eq!(
        kinds,
        [
            "warning", "warning", "warning", "warning", "warning", "warning", "note", "note",
            "note", "note", "note", "note", "note", "spacewasm-bench",
        ]
    );
    assert!(plain[13].ends_with(summary), "{}", plain[13]);
}

/// GitHub Actions is recognised by `GITHUB_ACTIONS` being `true`, as the
/// runner sets it, and anything else is a plain log.
///
/// Fails if a local run prints workflow commands, or a run inside Actions
/// prints plain lines that annotate nothing.
#[test]
fn github_actions_is_recognised_by_its_own_variable() {
    assert_eq!(Channel::of_github_actions(Some("true")), Channel::Actions);
    for value in [None, Some("false"), Some(""), Some("1"), Some("TRUE")] {
        assert_eq!(Channel::of_github_actions(value), Channel::Plain, "{value:?}");
    }
}

// ---------------------------------------------------------------------------
// The command line
// ---------------------------------------------------------------------------

/// The two commands read their options as documented, with the committed
/// history and the default threshold when none is given — the 5% the book's
/// SpaceWasm chapter quotes.
///
/// Fails if a default moves, if an option reaches the wrong command, or if a
/// refusal stops naming what it refuses, an unknown command ahead of its
/// options.
#[test]
fn the_command_line_is_read_as_documented() {
    let parse = |argv: &[&str]| {
        Command::parse(&argv.iter().map(ToString::to_string).collect::<Vec<_>>())
    };
    assert_eq!(
        parse(&["record"]),
        Ok(Command::Record { history: committed_history(), commit: None })
    );
    assert_eq!(
        parse(&["record", "--commit", "abc", "--history", "h.jsonl"]),
        Ok(Command::Record { history: "h.jsonl".into(), commit: Some("abc".to_string()) })
    );
    assert_eq!(
        parse(&["compare"]),
        Ok(Command::Compare { history: committed_history(), threshold: DEFAULT_THRESHOLD_PERCENT })
    );
    assert_eq!(
        parse(&["compare", "--threshold", "12", "--history", "h.jsonl"]),
        Ok(Command::Compare { history: "h.jsonl".into(), threshold: 12 })
    );
    assert_eq!(DEFAULT_THRESHOLD_PERCENT, 5);
    let committed = Path::new("bench").join("spacewasm").join("history.jsonl");
    assert!(committed_history().ends_with(committed));

    for (argv, refusal) in [
        (&[][..], "no command was given; it is `record` or `compare`"),
        (&["measure"][..], "unknown command `measure`; it is `record` or `compare`"),
        (
            &["measure", "--commit", "abc"][..],
            "unknown command `measure`; it is `record` or `compare`",
        ),
        (&["compare", "--commit", "abc"][..], "`compare` takes no option `--commit`"),
        (&["record", "--threshold", "5"][..], "`record` takes no option `--threshold`"),
        (
            &["compare", "--threshold", "5.5"][..],
            "`--threshold` takes a whole percentage, not `5.5`",
        ),
        (&["compare", "--threshold"][..], "`--threshold` needs a value"),
    ] {
        assert_eq!(parse(argv), Err(refusal.to_string()), "{argv:?}");
    }
}

// ---------------------------------------------------------------------------
// The history and the measurement
// ---------------------------------------------------------------------------

/// The committed history reads, holds a snapshot, and measures each program,
/// export and arguments once per snapshot.
///
/// Fails if a hand edit or a bad merge leaves a line the benchmark cannot
/// read, which would stop every comparison, or if one snapshot's lines are
/// appended twice.
#[test]
fn the_committed_history_reads_and_measures_each_call_once_per_snapshot() {
    let records = committed_records();
    assert!(!records.is_empty(), "the history holds at least the first snapshot");
    let mut seen = FxHashSet::default();
    for record in &records {
        assert!(
            seen.insert((record.stamp.clone(), record.measurement.key())),
            "{} is measured twice in one snapshot",
            record.measurement.key()
        );
    }
}

/// The newest snapshot of the committed history was recorded under the
/// interpreter release the workspace links, so `compare` has a series to set
/// a pull request's measurements beside.
///
/// `compare` never sets a count made under one release beside one made under
/// another, so a history whose newest snapshot is under an older release
/// compares nothing and warns about nothing, and says so only in a notice.
/// Fails when the `spacewasm` pin, and `LIMITS_FROM` with it, moves without a
/// snapshot recorded under the new release in the same change.
#[test]
fn the_newest_snapshot_is_under_the_interpreter_the_workspace_links() {
    let records = committed_records();
    let newest = &records.last().expect("the history holds a snapshot").stamp;
    assert_eq!(
        newest.interpreter,
        LIMITS_FROM,
        "the newest snapshot in {} was recorded under {}, and the workspace now links {}; run \
         `cargo run -p inference-tests --example spacewasm-bench -- record` in the same change \
         as the `spacewasm` pin bump and commit the lines it appends, or every comparison \
         compares nothing",
        committed_history().display(),
        newest.interpreter,
        LIMITS_FROM
    );
}

/// The committed history, read.
fn committed_records() -> Vec<Record> {
    let text = std::fs::read_to_string(committed_history()).expect("the history is committed");
    parse_history(&text).unwrap_or_else(|e| panic!("the committed history: {e}"))
}

/// The interpreter release every measurement is stamped with, and compared
/// under, is the one the workspace pins.
///
/// Fails if the pin moves and the runner's `LIMITS_FROM` does not, which
/// would compare counts from one release with another's as if they were one
/// series.
#[test]
fn the_interpreter_release_is_the_one_the_workspace_pins() {
    let path = test_data_path().join("..").join("..").join("Cargo.toml");
    let manifest = std::fs::read_to_string(path).expect("the workspace Cargo.toml is readable");
    let pin = manifest
        .lines()
        .find_map(|line| line.strip_prefix("spacewasm = { version = \"="))
        .and_then(|rest| rest.split('"').next())
        .expect("the workspace pins `spacewasm` with an `=`");
    assert_eq!(LIMITS_FROM, format!("spacewasm {pin}"));
}

/// A measured count is the least budget that runs the call to the same end,
/// and a measurement's module figures are the runner's `ir_stats` of the
/// module it ran, for a corpus fixture measured at the tier's configuration —
/// one whose exports take arguments, some returning on zeros and some
/// trapping — and for an F´ program measured against the reference hosts.
///
/// Fails if either route of the measurement counts an instruction more or
/// fewer than the call takes, measures a call other than the one it names,
/// or records a module figure under another key — pages as words, say, which
/// `compare` would then judge.
#[test]
fn a_measured_count_is_the_least_budget_that_finishes_the_call() {
    let fixture = "codegen/wasm/base/assert/assert.inf";
    let source = std::fs::read_to_string(test_data_path().join(fixture)).expect("the fixture");
    let SpaceWasmBuild::Runnable(output) = spacewasm_build(&source) else {
        panic!("{fixture} runs at the SpaceWasm target");
    };
    let mut session = SpaceWasmSession::acquire();
    let measurements = measure_fixture(&mut session, fixture, output.wasm())
        .unwrap_or_else(|why| panic!("{fixture} is measured: {why}"));
    let endings: FxHashSet<(Ending, bool)> =
        measurements.iter().map(|m| (m.outcome, m.args.is_empty())).collect();
    assert!(
        endings.contains(&(Ending::Returned, true)) && endings.contains(&(Ending::Trapped, false)),
        "{fixture} has an export taking nothing that returns and one taking arguments that traps"
    );
    for measurement in &measurements {
        let export = &measurement.export;
        let count = measurement.instructions;
        let mut module = support::decode(&mut session, output.wasm()).expect("it loads");
        let stats = support::ir_stats(&module, output.wasm().len());
        assert_module_figures(measurement, stats.code_pages, stats.ir_words, output.wasm().len());
        let params = module
            .exported_functions()
            .into_iter()
            .find(|function| function.name == *export)
            .expect("the measured function is exported")
            .params;
        let zeros: Vec<Value> = params.iter().map(|ty| support::zero_of(*ty)).collect();
        assert_eq!(measurement.args, vec!["0"; zeros.len()], "{export}");
        let ended = match module.invoke(export, &zeros, count) {
            support::Outcome::Value(_) => Ending::Returned,
            support::Outcome::Trap(_) => Ending::Trapped,
            support::Outcome::OutOfFuel => Ending::OutOfFuel,
        };
        assert_eq!(ended, measurement.outcome, "{export}");
        let short = module.invoke(export, &zeros, count - 1);
        assert_eq!(short, support::Outcome::OutOfFuel, "{export}");
    }

    let calls = fprime_calls();
    let fixture_call = &calls[0];
    let measurement =
        measure_fprime(&mut session, fixture_call).expect("the F´ fixture is measured");
    assert_eq!(measurement.export, "report");
    assert_eq!(measurement.args, ["3"]);
    let count = measurement.instructions;
    let wasm = wasm_for_target(&fixture_call.source, Target::SpaceWasm);
    for (budget, returns) in [(count, true), (count - 1, false)] {
        let fuel = Fuel::Limited(NonZeroUsize::new(budget).expect("a budget"));
        let log = HostLog::recording();
        let module = fprime::load(&mut session, &wasm, log, EngineConfig::REFERENCE)
            .expect("it loads");
        let stats = runner::ir_stats(module.module(), wasm.len());
        assert_module_figures(&measurement, stats.code_pages, stats.ir_words, wasm.len());
        let mut instance = module.start(fuel).expect("it has no start function");
        let outcome =
            instance.invoke(fixture_call.export, &fixture_call.args).expect("the call is made");
        assert_eq!(matches!(outcome, Outcome::Returned(_)), returns, "{budget}: {outcome:?}");
    }
}

/// Every function every runnable fixture exports, as the key of a call with
/// zero of each parameter's type, read from the decoder independently of the
/// benchmark's own walk.
fn every_export_of_every_runnable_fixture() -> FxHashSet<Key> {
    let mut session = SpaceWasmSession::acquire();
    let mut keys = FxHashSet::default();
    for (label, source) in single_file_corpus_sources() {
        let SpaceWasmBuild::Runnable(output) = spacewasm_build(&source) else {
            continue;
        };
        let program = relative_to_test_data(Path::new(&label));
        let module = support::decode(&mut session, output.wasm()).expect("a runnable module loads");
        let functions = module.exported_functions();
        assert!(!functions.is_empty(), "{program} exports a function, so its IR is measured");
        for function in functions {
            keys.insert(Key {
                program: program.clone(),
                export: function.name,
                args: vec!["0".to_string(); function.params.len()],
            });
        }
    }
    keys
}

/// Asserts that `measurement` carries its module's figures: `code_pages`
/// pages and `ir_words` words compiled from `wasm_bytes` bytes, and the ratio
/// they make. The three differ in every module measured, so a figure stored
/// under another's key is caught.
fn assert_module_figures(
    measurement: &Measurement,
    code_pages: usize,
    ir_words: usize,
    wasm_bytes: usize,
) {
    let figures = (measurement.code_pages, measurement.ir_words, measurement.wasm_bytes);
    assert_eq!(figures, (code_pages, ir_words, wasm_bytes), "{}", measurement.key());
    assert_ne!(code_pages, ir_words, "the row cannot tell pages from words");
    #[allow(clippy::cast_precision_loss)]
    let ratio = (2 * ir_words) as f64 / wasm_bytes as f64;
    assert!((measurement.ir_bytes_per_wasm_byte - ratio).abs() < 1e-12, "{}", measurement.key());
}

/// A program the benchmark cannot measure is left out and returned with why,
/// in the order it was met, beside the measurements of the programs that
/// could be measured, and never measured as something else: a module the
/// interpreter refuses, an F´ program binding a host the reference set lacks,
/// and a call of a function the program does not export.
///
/// Fails if a refused module is measured, if a program that could not be
/// measured goes unreported or is reported as another, if one that could be
/// is dropped with it, or if an F´ program that cannot be run is reported as
/// another failure.
#[test]
fn a_program_that_cannot_be_measured_is_left_out_and_named() {
    let grows = wat::parse_str(
        r#"(module (memory 1) (func (export "f") (drop (memory.grow (i32.const 1)))))"#,
    )
    .expect("the module is valid WAT");
    let returns = wat::parse_str(r#"(module (func (export "f") (result i32) (i32.const 7)))"#)
        .expect("the module is valid WAT");
    let call = |program: &str, source: &str, export| FprimeCall {
        program: program.to_string(),
        source: source.to_string(),
        export,
        args: vec![Value::I32(1)],
    };
    let beeps = "external fn beep();\nuse { beep } from host::fprime_core;\n\
                 pub fn go(n: i32) -> i32 { beep(); return n; }";
    let fixtures = [("grows.wat".to_string(), grows), ("returns.wat".to_string(), returns)];
    let calls = [
        call("fprime/beeps", beeps, "go"),
        call("fprime/spin", SPIN, "spin"),
        call("fprime/spun", SPIN, "spun"),
    ];
    let mut session = SpaceWasmSession::acquire();
    let measured = measure_programs(&mut session, fixtures, &calls);

    let programs: Vec<&str> = measured.measurements.iter().map(|m| m.program.as_str()).collect();
    assert_eq!(programs, ["returns.wat", "fprime/spin"]);
    let [grew, beeped, spun] = &measured.unmeasured[..] else {
        panic!("three programs are not measured: {:?}", measured.unmeasured);
    };
    for (unmeasured, program, why) in [
        (grew, "grows.wat", "the interpreter refuses it: "),
        (beeped, "fprime/beeps", "it does not load: "),
        (spun, "fprime/spun", "`spun` cannot be called: "),
    ] {
        assert_eq!(unmeasured.program, program);
        assert!(unmeasured.why.starts_with(why), "{program}: {}", unmeasured.why);
    }
    assert_eq!(grew.to_string(), format!("grows.wat is not measured: {}", grew.why));
}

/// A history that cannot be read or parsed stops `compare` with a failure
/// naming it, before anything is measured, rather than being compared as
/// though it were empty.
///
/// Fails if a missing or broken history turns into a quiet comparison, which
/// would leave the CI job green with nothing compared, or if anything is
/// measured before the history is read.
#[test]
fn a_history_that_cannot_be_read_fails_the_comparison() {
    let dir = tempfile::TempDir::new().expect("a temporary directory");
    let broken = dir.path().join("broken.jsonl");
    std::fs::write(&broken, "{\"date\":\"2026-09-26\"}\n").expect("writable");
    let missing = dir.path().join("missing.jsonl");
    for (history, why) in [(missing, "cannot read it: "), (broken, "line 1 is not a record: ")] {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let unread = || panic!("{} is measured against before it is read", history.display());
        let code = compare_with(&history, 5, Channel::Plain, unread, &mut out, &mut err);
        assert_eq!(code, exit::FAILED, "{}", history.display());
        assert!(out.is_empty(), "nothing is compared: {}", String::from_utf8_lossy(&out));
        let err = String::from_utf8(err).expect("UTF-8");
        let expected = format!("error: {}: {why}", history.display());
        assert!(err.starts_with(&expected), "{err}");
    }
}

/// A run that could not measure every program records nothing: `record`
/// names each program left out and why, then how many there were and that
/// nothing was recorded, and fails, leaving a history byte for byte as it was
/// and a history that did not exist absent.
///
/// The run measured a program as well, so a snapshot of what it did measure
/// would change the file. Fails if a snapshot missing a program is appended,
/// which would end that program's series with no later comparison reporting
/// it gone, if the refusal answers anything but a failure, or if a program
/// left out goes unnamed.
#[test]
fn a_run_that_left_a_program_unmeasured_records_nothing() {
    let dir = tempfile::TempDir::new().expect("a temporary directory");
    let kept = dir.path().join("kept.jsonl");
    let before = line("aaaaaaa", "a.inf", 10, 20);
    std::fs::write(&kept, &before).expect("writable");
    let absent = dir.path().join("absent.jsonl");
    let unmeasured = |program: &str, why: &str| Unmeasured {
        program: program.to_string(),
        why: why.to_string(),
    };
    let run = Measured {
        measurements: vec![measured("a.inf", 11, 20)],
        unmeasured: vec![
            unmeasured("b.inf", "the interpreter refuses it: FunctionImportNotFound at byte 73"),
            unmeasured("fprime/c", "it does not start: no memory"),
        ],
    };
    for history in [&kept, &absent] {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = record_with(history, &stamped("bbbbbbb"), &run, &mut out, &mut err);
        assert_eq!(code, exit::FAILED, "{}", history.display());
        assert!(out.is_empty(), "{}", String::from_utf8_lossy(&out));
        assert_eq!(
            String::from_utf8(err).expect("UTF-8"),
            format!(
                "spacewasm-bench: b.inf is not measured: the interpreter refuses it: \
                 FunctionImportNotFound at byte 73\n\
                 spacewasm-bench: fprime/c is not measured: it does not start: no memory\n\
                 error: 2 of the programs could not be measured, so nothing was recorded in {}: \
                 a snapshot without them would end their series\n",
                history.display()
            )
        );
    }
    assert_eq!(std::fs::read(&kept).expect("readable"), before.as_bytes());
    assert!(!absent.exists(), "a refused snapshot creates no history");
}

/// A run that measured every program is recorded as one snapshot: a line per
/// measurement, in the order they were made and each under the same stamp,
/// after what the history holds, or as a new history when there is none; and
/// `record` says how many it recorded, under which stamp and where.
///
/// Fails if a complete run is refused, if a measurement is dropped, written
/// twice or stamped otherwise, or if the history's earlier lines move.
#[test]
fn a_complete_run_is_recorded_as_one_snapshot() {
    let dir = tempfile::TempDir::new().expect("a temporary directory");
    let existing = dir.path().join("existing.jsonl");
    let earlier = format!("{}\n", line("aaaaaaa", "a.inf", 10, 20));
    std::fs::write(&existing, &earlier).expect("writable");
    let absent = dir.path().join("absent.jsonl");
    let current = complete(&[measured("a.inf", 11, 20), measured("b.inf", 5, 8)]);
    let snapshot =
        format!("{}\n{}\n", line("bbbbbbb", "a.inf", 11, 20), line("bbbbbbb", "b.inf", 5, 8));
    for (history, before) in [(&existing, earlier.as_str()), (&absent, "")] {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = record_with(history, &stamped("bbbbbbb"), &current, &mut out, &mut err);
        assert_eq!(code, exit::OK, "{}", String::from_utf8_lossy(&err));
        assert!(err.is_empty(), "{}", String::from_utf8_lossy(&err));
        let after = std::fs::read_to_string(history).expect("readable");
        assert_eq!(after, format!("{before}{snapshot}"), "{}", history.display());
        assert_eq!(
            String::from_utf8(out).expect("UTF-8"),
            format!(
                "spacewasm-bench: recorded 2 measurements at bbbbbbb (2026-09-26) under \
                 {LIMITS_FROM} in {}\n",
                history.display()
            )
        );
    }
}

/// A program `compare` could not measure is a warning, ahead of the notes:
/// inside Actions a `::warning` annotation, `%` encoded, which the annotation
/// leading the warnings counts, and elsewhere a `warning:` line; the summary
/// counts it; and the comparison still answers 0, since nothing it finds
/// fails it. Its measurements in the latest snapshot are gone as well, and
/// noted as such.
///
/// Fails if a program left out goes unreported or reads as a note, if the
/// leading annotation or the summary drops its count, or if a program that
/// could not be measured fails the comparison.
#[test]
fn a_program_compare_could_not_measure_is_a_warning_that_never_fails_it() {
    let dir = tempfile::TempDir::new().expect("a temporary directory");
    let history = dir.path().join("history.jsonl");
    let lines = [line("aaaaaaa", "a.inf", 10, 20), line("aaaaaaa", "b.inf", 10, 20)];
    std::fs::write(&history, lines.join("\n")).expect("writable");
    let measure = || Measured {
        measurements: vec![measured("a.inf", 10, 20)],
        unmeasured: vec![Unmeasured {
            program: "b.inf".to_string(),
            why: "the interpreter refuses it: 100% of its pages are taken".to_string(),
        }],
    };
    let gone = "b.inf: main() is gone: it was measured at aaaaaaa (2026-09-26) and is not \
                measured now";
    let summary = format!(
        "spacewasm-bench: compared 1 measurements with {}, whose latest snapshot is aaaaaaa \
         (2026-09-26), at a 5% threshold: 0 grew past it, 0 end another way, 0 new, 1 gone, 0 \
         not comparable, 1 could not be measured",
        history.display()
    );
    for (channel, expected) in [
        (
            Channel::Actions,
            vec![
                "::warning title=SpaceWasm benchmark::1 of the programs could not be measured; \
                 this job's log lists every one"
                    .to_string(),
                "::warning title=SpaceWasm benchmark::b.inf is not measured: the interpreter \
                 refuses it: 100%25 of its pages are taken"
                    .to_string(),
                format!("::notice title=SpaceWasm benchmark::{gone}"),
                summary.clone(),
            ],
        ),
        (
            Channel::Plain,
            vec![
                "warning: b.inf is not measured: the interpreter refuses it: 100% of its pages \
                 are taken"
                    .to_string(),
                format!("note: {gone}"),
                summary.clone(),
            ],
        ),
    ] {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = compare_with(&history, 5, channel, measure, &mut out, &mut err);
        assert_eq!(code, exit::OK, "{channel:?}: {}", String::from_utf8_lossy(&err));
        assert!(err.is_empty(), "{channel:?}: {}", String::from_utf8_lossy(&err));
        let out = String::from_utf8(out).expect("UTF-8");
        assert_eq!(out.lines().collect::<Vec<_>>(), expected, "{channel:?}");
    }
}

/// A function taking parameters is called with zero of each one's type,
/// recorded as `0`, and one taking an address stores through address 0.
///
/// Fails if a function taking parameters goes unmeasured, is called with
/// anything but zeros, or has its arguments recorded otherwise.
#[test]
fn a_function_taking_parameters_is_called_with_zeros() {
    let wasm = wat::parse_str(
        r#"(module
             (memory 1)
             (func (export "sum") (param i32 i64 f32 f64) (result i64)
               (i64.add (i64.extend_i32_s (local.get 0)) (local.get 1)))
             (func (export "store") (param i32) (i32.store (local.get 0) (i32.const 7)))
             (func (export "divide") (param i32) (result i32)
               (i32.div_s (i32.const 1) (local.get 0))))"#,
    )
    .expect("the module is valid WAT");
    let mut session = SpaceWasmSession::acquire();
    let measurements =
        measure_fixture(&mut session, "params.wat", &wasm).expect("the module is measured");
    let calls: Vec<(&str, Vec<&str>, Ending)> = measurements
        .iter()
        .map(|m| (m.export.as_str(), m.args.iter().map(String::as_str).collect(), m.outcome))
        .collect();
    assert_eq!(
        calls,
        [
            ("sum", vec!["0", "0", "0", "0"], Ending::Returned),
            ("store", vec!["0"], Ending::Returned),
            ("divide", vec!["0"], Ending::Trapped),
        ]
    );
}

/// Each call is measured on a module of its own, so its count does not
/// depend on what an earlier call left in the module.
///
/// `arm` sets the global `spin` counts down from. On one module shared by
/// both calls, `spin` would count down from 1000 after `arm`; on a module of
/// its own it counts from 0, as it does when it is the only call made. Fails
/// if the measurement reuses a module across the calls it makes.
#[test]
fn each_call_is_measured_on_a_module_of_its_own() {
    let wasm = wat::parse_str(
        r#"(module
             (global $left (mut i32) (i32.const 0))
             (func (export "arm") (global.set $left (i32.const 1000)))
             (func (export "spin")
               (block $done
                 (loop $again
                   (br_if $done (i32.eqz (global.get $left)))
                   (global.set $left (i32.sub (global.get $left) (i32.const 1)))
                   (br $again)))))"#,
    )
    .expect("the module is valid WAT");
    let mut session = SpaceWasmSession::acquire();
    let measurements =
        measure_fixture(&mut session, "armed.wat", &wasm).expect("the module is measured");
    let exports: Vec<&str> = measurements.iter().map(|m| m.export.as_str()).collect();
    assert_eq!(exports, ["arm", "spin"], "measured in export order");
    let mut alone = support::decode(&mut session, &wasm).expect("the module loads");
    let (_, unarmed) = alone.invoke_counting("spin", &[]);
    assert_eq!(measurements[1].instructions, unarmed);
}

/// The benchmark measures every function every runnable fixture exports,
/// each called with zero of each parameter's type, and the four F´ programs,
/// each ending as the tier's rows pin; and measuring the same tree twice gives
/// the same figures, so a comparison of a tree with itself is quiet.
///
/// Fails if the measured set shrinks below the differential sweep's own
/// floor, if an F´ program stops being measured or ends another way, if a
/// program cannot be measured, or if a figure depends on anything but the
/// tree — which would make every comparison noisy.
#[test]
fn the_tiers_calls_are_measured_and_twice_compare_quietly() {
    let Measured { measurements: first, unmeasured } = measure();
    assert_eq!(unmeasured, []);
    let fprime_count = fprime_calls().len();
    let measured: FxHashSet<Key> =
        first[..first.len() - fprime_count].iter().map(Measurement::key).collect();
    assert_eq!(measured, every_export_of_every_runnable_fixture());
    assert!(measured.len() >= 700, "{} calls; a narrowed walk would pass above", measured.len());
    let endings: Vec<(&str, &str, Ending)> = first[first.len() - fprime_count..]
        .iter()
        .map(|m| (m.program.as_str(), m.export.as_str(), m.outcome))
        .collect();
    assert_eq!(
        endings,
        [
            (
                "codegen/wasm/extern_import/host_import_fprime/host_import_fprime.inf",
                "report",
                Ending::Returned
            ),
            ("fprime/every_call", "go", Ending::Trapped),
            ("fprime/downlink", "downlink", Ending::Returned),
            ("fprime/spin", "spin", Ending::Returned),
        ]
    );

    let second = measure();
    assert_eq!(complete(&first), second, "a measurement depends only on the tree");
    let stamp = stamped("aaaaaaa");
    let recorded: Vec<Record> = first
        .into_iter()
        .map(|measurement| Record { stamp: stamp.clone(), measurement })
        .collect();
    let comparison = compare(&recorded, &second, LIMITS_FROM, 0);
    assert_eq!(comparison.findings, [], "not even a zero threshold finds a difference");
    assert_eq!(comparison.measured, second.measurements.len());
}
