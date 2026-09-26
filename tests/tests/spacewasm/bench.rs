//! The SpaceWasm benchmark: what this compiler's output costs the flight
//! interpreter, measured on a fixed set of programs and kept as a history.
//!
//! For a flight target the IR a module compiles to and the instructions a call
//! takes are budget numbers. The code builder fills a fixed number of IR pages,
//! and a call runs inside a cycle of fixed length, so a change to the compiler
//! that grows either is a change a mission pays for. This module measures both
//! and keeps the measurements in a file the repository commits,
//! `tests/bench/spacewasm/history.jsonl`, so how they move from one release to
//! the next is on record and a pull request that moves them says so.
//!
//! It is the body of the `spacewasm-bench` example and lives in the SpaceWasm
//! tier's directory for the reason the embed harness does: the example is
//! three lines over [`run`], and `tests/tests/spacewasm_bench.rs` tests the
//! pieces in process.
//!
//! # What is measured
//!
//! The programs are the ones the SpaceWasm tier already runs. Every function
//! exported by every single-file codegen fixture that compiles at the
//! `spacewasm` target, declares no import and carries no verification
//! operator — the fixtures the differential sweep runs — is called with zero
//! of each parameter's type, on a module loaded for that one call, at the
//! tier's configuration. Each F´ program of [`fprime_calls`] is loaded against the
//! runner's F´ reference hosts at the reference configuration `infs run`
//! loads at, and called with the arguments that table gives. A fixture the
//! target refuses, or the interpreter will not load, is not measured: the
//! tier's sweeps are where that is a failure, and here it shows as a
//! measurement gone from the history.
//!
//! Each call is one measurement: the IR its module compiled to, measured by
//! the runner's `ir_stats` exactly as the embed harness's `--stats` measures
//! it, and the exact number of interpreter instructions the call took, which
//! the runner's `invoke_counting` counts by running the call one instruction
//! at a time. Both figures are in the interpreter's own terms — its IR, and
//! its instructions over that IR — so they change with the interpreter release
//! as well as with the compiler, and every measurement names the release.
//!
//! # The history
//!
//! One JSON object per line and one line per measurement, in this key order:
//!
//! | key | meaning |
//! |---|---|
//! | `date` | the UTC day the measurement was recorded, `YYYY-MM-DD` |
//! | `commit` | the commit the measured tree was built from, `git rev-parse HEAD` |
//! | `interpreter` | the interpreter release, as the runner's `LIMITS_FROM` names it |
//! | `program` | a fixture's path under `tests/test_data`, or `fprime/NAME` for an F´ program |
//! | `export` | the function called |
//! | `args` | its arguments, as decimal strings |
//! | `outcome` | how the call ended: `returned`, `trapped` or `out_of_fuel` |
//! | `instructions` | the interpreter instructions the call took |
//! | `code_pages` | IR pages the module's code filled |
//! | `ir_words` | sixteen-bit IR words written across them |
//! | `wasm_bytes` | the module's size |
//! | `ir_bytes_per_wasm_byte` | bytes of IR per byte of WebAssembly, unrounded |
//!
//! The last four describe the module rather than the call, and carry the
//! meanings the embed harness's `--stats --json` gives the same keys. The
//! ratio is kept for a reader and never compared, since it follows from
//! `ir_words` and `wasm_bytes`.
//!
//! The lines one `record` run appends are a snapshot: they share their
//! `date`, `commit` and `interpreter`, and those three together are what
//! tells one snapshot from another. A measurement is known by its `program`,
//! `export` and `args` together.
//!
//! # The two commands
//!
//! ```text
//! spacewasm-bench record  [--history PATH] [--commit SHA]
//! spacewasm-bench compare [--history PATH] [--threshold PERCENT]
//! ```
//!
//! `record` measures every program and appends the snapshot to the history,
//! stamped with the commit `git rev-parse HEAD` names unless `--commit` names
//! another. It is run by hand before a release, from a clean checkout of the
//! commit being released, and the lines it appends are committed with it.
//!
//! `compare` measures every program and compares each measurement with the
//! last line the history holds for it under the same interpreter release. It
//! warns when a call's instructions, or its module's IR words, grew by more
//! than the threshold, [`DEFAULT_THRESHOLD_PERCENT`] unless `--threshold`
//! gives another whole percentage. A module's IR is judged once, however many
//! of its exports are called, and by its words rather than its pages, since a
//! page count is only the words rounded up. A call that now ends another way
//! is noted rather than judged, since its two counts measure two different
//! runs. It notes as well a measurement the history has never seen, one the
//! history's latest snapshot holds and this run did not make, and how many
//! have history only under another interpreter release. Those are not
//! compared at all, so the pull request that moves the interpreter's pin is
//! the one to record a snapshot under the new release: until one exists,
//! `compare` warns about nothing, and a test of the benchmark fails. Inside
//! GitHub Actions a warning is a `::warning` annotation and a note a
//! `::notice`, with one annotation counting the warnings ahead of them, since
//! a pull request shows only the first few of each kind; the job log holds
//! every line. Elsewhere they are plain lines. Whatever it finds, `compare`
//! exits 0: it reports, and a person decides. It exits 1 only when it cannot
//! run: when the history cannot be read.
//!
//! The history is appended to once a release, so a figure that grew in one
//! pull request is warned about on every pull request after it until the
//! next snapshot; every warning names the snapshot it compares with.

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use inference_spacewasm_runner::{
    self as runner, EngineConfig, Fuel, HostLog, IrStats, LIMITS_FROM, Value, fprime, render,
};
use inference_tests::corpus::{
    SpaceWasmBuild, codegen_for_target, relative_to_test_data, single_file_corpus_sources,
    spacewasm_build, test_data_path,
};
use inference_wasm_codegen::Target;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use crate::fprime_programs::{DOWNLINK, EVERY_CALL, SPIN};
use crate::support::{self, FUEL, SpaceWasmSession};

/// Growth, in whole percent, past which `compare` warns: a figure that grew
/// by more than this share of its recorded value. The book's SpaceWasm
/// chapter quotes it.
pub const DEFAULT_THRESHOLD_PERCENT: u32 = 5;

/// The title every annotation carries in GitHub Actions.
const ANNOTATION_TITLE: &str = "SpaceWasm benchmark";

/// How to call the benchmark, printed under every refused command line.
///
/// Spelled apart from [`exit::USAGE`], the code the same refusal exits with,
/// as the embed harness spells its own.
const USAGE_LINE: &str = "usage: spacewasm-bench record [--history PATH] [--commit SHA]\n       \
                     spacewasm-bench compare [--history PATH] [--threshold PERCENT]";

/// The committed F´ fixture, by its path under `tests/test_data`.
const FPRIME_FIXTURE: &str = "codegen/wasm/extern_import/host_import_fprime/host_import_fprime.inf";

/// What the benchmark answers the shell with.
pub mod exit {
    /// The command did what it was asked, whatever a comparison found.
    pub const OK: u8 = 0;
    /// The history could not be read or written, or the commit not named.
    pub const FAILED: u8 = 1;
    /// The command line could not be read.
    pub const USAGE: u8 = 2;
}

// ---------------------------------------------------------------------------
// The record
// ---------------------------------------------------------------------------

/// How a measured call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ending {
    /// It returned.
    Returned,
    /// It trapped.
    Trapped,
    /// It was still running when the instruction budget ran out.
    OutOfFuel,
}

impl Ending {
    /// How a sentence says a call ended this way: "returned".
    fn past(self) -> &'static str {
        match self {
            Self::Returned => "returned",
            Self::Trapped => "trapped",
            Self::OutOfFuel => "ran out of fuel",
        }
    }

    /// How a sentence says a call ends this way: "returns".
    fn present(self) -> &'static str {
        match self {
            Self::Returned => "returns",
            Self::Trapped => "traps",
            Self::OutOfFuel => "runs out of fuel",
        }
    }
}

/// When a snapshot was recorded, and against what.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Stamp {
    /// The UTC day, `YYYY-MM-DD`.
    pub date: String,
    /// The commit the measured tree was built from.
    pub commit: String,
    /// The interpreter release, as `LIMITS_FROM` names it.
    pub interpreter: String,
}

impl Stamp {
    /// The commit shortened to the seven characters a reader recognises, and
    /// the day: `3171415 (2026-09-26)`.
    fn reference(&self) -> String {
        let commit = self.commit.get(..7).unwrap_or(&self.commit);
        format!("{commit} ({})", self.date)
    }
}

/// One call, measured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measurement {
    /// A fixture's path under `tests/test_data`, or `fprime/NAME`.
    pub program: String,
    /// The function called.
    pub export: String,
    /// Its arguments, as decimal strings.
    pub args: Vec<String>,
    /// How the call ended.
    pub outcome: Ending,
    /// The interpreter instructions the call took.
    pub instructions: usize,
    /// IR pages the module's code filled.
    pub code_pages: usize,
    /// Sixteen-bit IR words written across them.
    pub ir_words: usize,
    /// The module's size.
    pub wasm_bytes: usize,
    /// Bytes of IR per byte of WebAssembly, unrounded.
    pub ir_bytes_per_wasm_byte: f64,
}

impl Measurement {
    /// A measurement of `export` called with `args` in `program`, which
    /// ended as `outcome` after `instructions`, in a module that compiled to
    /// `stats`.
    #[must_use]
    pub fn new(
        program: &str,
        export: &str,
        args: &[Value],
        outcome: Ending,
        instructions: usize,
        stats: IrStats,
    ) -> Self {
        Self {
            program: program.to_string(),
            export: export.to_string(),
            args: args.iter().map(|value| render(*value)).collect(),
            outcome,
            instructions,
            code_pages: stats.code_pages,
            ir_words: stats.ir_words,
            wasm_bytes: stats.wasm_bytes,
            ir_bytes_per_wasm_byte: stats.ir_bytes_per_wasm_byte(),
        }
    }

    /// What the measurement is known by.
    #[must_use]
    pub fn key(&self) -> Key {
        Key { program: self.program.clone(), export: self.export.clone(), args: self.args.clone() }
    }
}

/// A program, a function of it and the arguments it is called with: what a
/// measurement is known by across snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key {
    /// A fixture's path under `tests/test_data`, or `fprime/NAME`.
    pub program: String,
    /// The function called.
    pub export: String,
    /// Its arguments, as decimal strings.
    pub args: Vec<String>,
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}({})", self.program, self.export, self.args.join(", "))
    }
}

/// One line of the history: a measurement and the stamp of its snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    #[serde(flatten)]
    pub stamp: Stamp,
    #[serde(flatten)]
    pub measurement: Measurement,
}

impl Record {
    /// The record as its line of the history, without the line break.
    ///
    /// # Panics
    ///
    /// Never: a record's fields are strings, numbers and a list of strings.
    /// Its one float is finite as well, since a module that loaded is at least
    /// eight bytes long, so the line reads back as the record it was written
    /// from.
    #[must_use]
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).expect("a record serializes")
    }
}

/// Reads a history, one record per line, skipping blank lines.
///
/// # Errors
///
/// The first line that is not a record, by its number, counted from 1.
pub fn parse_history(text: &str) -> Result<Vec<Record>, String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            serde_json::from_str(line)
                .map_err(|e| format!("line {} is not a record: {e}", index + 1))
        })
        .collect()
}

/// The UTC day `seconds` after the Unix epoch, as `YYYY-MM-DD`.
///
/// The civil-from-days conversion of the proleptic Gregorian calendar, in the
/// form Howard Hinnant published it, since the standard library keeps no
/// calendar and this is the one date the benchmark writes.
#[must_use]
pub fn utc_date(seconds: u64) -> String {
    let days = seconds / 86_400;
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 { month_from_march + 3 } else { month_from_march - 9 };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

// ---------------------------------------------------------------------------
// Measuring
// ---------------------------------------------------------------------------

/// One call of an F´ program, made against the reference hosts.
pub struct FprimeCall {
    /// The name the history knows the program by.
    pub program: String,
    /// Its source.
    pub source: String,
    /// The function called.
    pub export: &'static str,
    /// The arguments it is called with.
    pub args: Vec<Value>,
}

/// The F´ programs the benchmark runs, with the one call it makes of each:
/// the committed F´ fixture, read from `tests/test_data`, and the programs the
/// F´ rows of the SpaceWasm tier run, which `fprime_programs` shares with
/// them.
///
/// # Panics
///
/// Panics if the committed fixture cannot be read.
#[must_use]
pub fn fprime_calls() -> Vec<FprimeCall> {
    let fixture = test_data_path().join(FPRIME_FIXTURE);
    let fixture = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", fixture.display()));
    let call = |program: &str, source: &str, export, args: &[Value]| FprimeCall {
        program: program.to_string(),
        source: source.to_string(),
        export,
        args: args.to_vec(),
    };
    vec![
        call(FPRIME_FIXTURE, &fixture, "report", &[Value::I32(3)]),
        call("fprime/every_call", EVERY_CALL, "go", &[]),
        call("fprime/downlink", DOWNLINK, "downlink", &[Value::I32(11)]),
        call("fprime/spin", SPIN, "spin", &[Value::I32(1_000)]),
    ]
}

/// Measures every program, the corpus first and the F´ programs after it,
/// writing to `err` each one that could not be measured.
///
/// Acquires the interpreter session itself, so a caller already holding one
/// would deadlock.
///
/// # Panics
///
/// When a fixture or an F´ program does not parse or type-check, as the
/// SpaceWasm tier's sweeps do.
#[must_use]
pub fn measure(err: &mut dyn Write) -> Vec<Measurement> {
    let mut measurements = Vec::new();
    let mut session = SpaceWasmSession::acquire();
    for (label, source) in single_file_corpus_sources() {
        // The differential sweep's selection: a fixture it does not run is
        // that sweep's to account for.
        if let SpaceWasmBuild::Runnable(output) = spacewasm_build(&source) {
            let program = relative_to_test_data(Path::new(&label));
            measure_fixture(&mut session, &program, output.wasm(), &mut measurements, err);
        }
    }
    for call in fprime_calls() {
        match measure_fprime(&mut session, &call) {
            Ok(measurement) => measurements.push(measurement),
            Err(why) => note_unmeasured(err, &call.program, &why),
        }
    }
    measurements
}

/// Calls every function the fixture `program`, compiled to `wasm`, exports,
/// with zero of each parameter's type, on a module loaded for that one call,
/// and adds a measurement of each to `measurements`.
///
/// Zero is the one argument every parameter can take — an `i32` or an `i64`,
/// and an array or a struct, which the lowering passes as an address, here
/// address 0 — so the call is the same on every run, and every module the
/// interpreter loads has its IR measured, whatever its exports take. The call
/// may trap or return early on such arguments; it ends the same way each
/// time, which is all a series needs.
///
/// Each call is given a module of its own, as the differential sweep gives
/// one, so a call's count cannot depend on what an earlier call left in the
/// module's memory or globals.
pub fn measure_fixture(
    session: &mut SpaceWasmSession,
    program: &str,
    wasm: &[u8],
    measurements: &mut Vec<Measurement>,
    err: &mut dyn Write,
) {
    let exports = match support::decode(session, wasm) {
        Ok(module) => module.exported_functions(),
        Err(verdict) => {
            let why =
                format!("the interpreter refuses it: {:?} at byte {}", verdict.err, verdict.offset);
            note_unmeasured(err, program, &why);
            return;
        }
    };
    for export in exports {
        let args: Vec<Value> = export.params.iter().map(|ty| support::zero_of(*ty)).collect();
        let mut module = support::decode(session, wasm).expect("the module loaded a moment ago");
        let stats = support::ir_stats(&module, wasm.len());
        let (outcome, instructions) = module.invoke_counting(&export.name, &args);
        let ending = match outcome {
            support::Outcome::Value(_) => Ending::Returned,
            support::Outcome::Trap(_) => Ending::Trapped,
            support::Outcome::OutOfFuel => Ending::OutOfFuel,
        };
        let measured = Measurement::new(program, &export.name, &args, ending, instructions, stats);
        measurements.push(measured);
    }
}

/// Measures `call` against the F´ reference hosts, at the reference
/// configuration, within [`FUEL`], with the hosts' lines recorded rather than
/// printed.
///
/// # Errors
///
/// Why the program could not be measured: code generation refuses it, the
/// runner does not load or start it, or the call cannot be made.
///
/// # Panics
///
/// When the source does not parse or type-check, which the corpus helpers
/// compiling it treat as a broken fixture rather than an answer.
pub fn measure_fprime(
    session: &mut SpaceWasmSession,
    call: &FprimeCall,
) -> Result<Measurement, String> {
    let output = codegen_for_target(&call.source, Target::SpaceWasm)
        .map_err(|e| format!("code generation refuses it: {e}"))?;
    let wasm = output.wasm();
    let module = fprime::load(session, wasm, HostLog::recording(), EngineConfig::REFERENCE)
        .map_err(|e| format!("it does not load: {e}"))?;
    let stats = runner::ir_stats(module.module(), wasm.len());
    let mut instance = module
        .start(Fuel::Limited(FUEL.try_into().expect("the budget is not zero")))
        .map_err(|e| format!("it does not start: {e}"))?;
    let counted = instance
        .invoke_counting(call.export, &call.args)
        .map_err(|e| format!("`{}` cannot be called: {e}", call.export))?;
    let ending = match counted.outcome {
        runner::Outcome::Returned(_) => Ending::Returned,
        runner::Outcome::Trapped(_) => Ending::Trapped,
        runner::Outcome::OutOfFuel { .. } => Ending::OutOfFuel,
    };
    let instructions = counted.instructions;
    Ok(Measurement::new(&call.program, call.export, &call.args, ending, instructions, stats))
}

/// Says that `program` was not measured, and why.
fn note_unmeasured(err: &mut dyn Write, program: &str, why: &str) {
    line(err, &format!("spacewasm-bench: {program} is not measured: {why}"));
}

// ---------------------------------------------------------------------------
// Comparing
// ---------------------------------------------------------------------------

/// Something a comparison found.
#[derive(Debug, Clone, PartialEq)]
pub enum Finding {
    /// A call took more instructions than the threshold allows.
    InstructionsGrew { key: Key, from: usize, to: usize, at: Stamp },
    /// A module compiled to more IR words than the threshold allows.
    IrGrew { program: String, from: usize, to: usize, at: Stamp },
    /// A call ends another way than it did.
    OutcomeChanged { key: Key, from: Ending, to: Ending, at: Stamp },
    /// A measurement the history has never seen.
    New { key: Key },
    /// A measurement the history's latest snapshot holds and this run did
    /// not make.
    Gone { key: Key, at: Stamp },
    /// Measurements with history only under another interpreter release.
    NotComparable { count: usize, interpreter: String },
}

impl Finding {
    /// Whether this is a warning rather than a note: a figure that grew.
    #[must_use]
    pub fn is_warning(&self) -> bool {
        matches!(self, Self::InstructionsGrew { .. } | Self::IrGrew { .. })
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InstructionsGrew { key, from, to, at } => write!(
                f,
                "{key} took {to} interpreter instructions, {} since {}",
                growth(*from, *to),
                at.reference()
            ),
            Self::IrGrew { program, from, to, at } => write!(
                f,
                "{program} compiled to {to} IR words, {} since {}",
                growth(*from, *to),
                at.reference()
            ),
            Self::OutcomeChanged { key, from, to, at } => write!(
                f,
                "{key} now {}; it {} at {}",
                to.present(),
                from.past(),
                at.reference()
            ),
            Self::New { key } => write!(f, "{key} is new: the history holds no measurement of it"),
            Self::Gone { key, at } => write!(
                f,
                "{key} is gone: it was measured at {} and is not measured now",
                at.reference()
            ),
            Self::NotComparable { count, interpreter } => write!(
                f,
                "{count} measurements have history only under another interpreter release than \
                 {interpreter}, whose instructions and IR are not {interpreter}'s, so they are \
                 not compared; `record` a snapshot to start its series"
            ),
        }
    }
}

/// How far `to` is from `from`: "up 12.5% from 1200", or "up from 0".
fn growth(from: usize, to: usize) -> String {
    if from == 0 {
        return "up from 0".to_string();
    }
    let percent = 100.0 * (to as f64 - from as f64) / from as f64;
    format!("up {percent:.1}% from {from}")
}

/// Whether `to` is more than `threshold` percent above `from`. Anything above
/// zero is, when `from` is zero.
fn grew_past(from: usize, to: usize, threshold: u32) -> bool {
    to as u128 * 100 > from as u128 * (100 + u128::from(threshold))
}

/// What comparing a run's measurements with the history found.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    /// Every finding, in the order the measurements were made, with the
    /// measurements gone and those not comparable after them.
    pub findings: Vec<Finding>,
    /// The measurements this run made.
    pub measured: usize,
    /// The stamp of the history's last line, when it has one.
    pub latest: Option<Stamp>,
}

/// Compares `current`, measured under `interpreter`, with `history`, warning
/// past `threshold` percent.
///
/// A measurement is compared with the last line of the history known by the
/// same key and recorded under the same interpreter release. A module's IR is
/// judged at the first of its measurements that has such a line, and only
/// there; a call's instructions are judged unless it now ends another way,
/// which is a finding of its own. A measurement is gone when the history's
/// latest snapshot — the lines stamped as its last line is — holds it and
/// `current` does not.
#[must_use]
pub fn compare(
    history: &[Record],
    current: &[Measurement],
    interpreter: &str,
    threshold: u32,
) -> Comparison {
    let mut baselines: FxHashMap<Key, &Record> = FxHashMap::default();
    let mut elsewhere: FxHashSet<Key> = FxHashSet::default();
    for record in history {
        let key = record.measurement.key();
        if record.stamp.interpreter == interpreter {
            baselines.insert(key, record);
        } else {
            elsewhere.insert(key);
        }
    }

    let mut findings = Vec::new();
    let mut not_comparable = 0;
    let mut ir_judged: FxHashSet<&str> = FxHashSet::default();
    for measurement in current {
        let key = measurement.key();
        let Some(baseline) = baselines.get(&key) else {
            if elsewhere.contains(&key) {
                not_comparable += 1;
            } else {
                findings.push(Finding::New { key });
            }
            continue;
        };
        let (before, at) = (&baseline.measurement, &baseline.stamp);
        if ir_judged.insert(&measurement.program)
            && grew_past(before.ir_words, measurement.ir_words, threshold)
        {
            findings.push(Finding::IrGrew {
                program: measurement.program.clone(),
                from: before.ir_words,
                to: measurement.ir_words,
                at: at.clone(),
            });
        }
        if before.outcome != measurement.outcome {
            findings.push(Finding::OutcomeChanged {
                key,
                from: before.outcome,
                to: measurement.outcome,
                at: at.clone(),
            });
        } else if grew_past(before.instructions, measurement.instructions, threshold) {
            findings.push(Finding::InstructionsGrew {
                key,
                from: before.instructions,
                to: measurement.instructions,
                at: at.clone(),
            });
        }
    }

    let latest = history.last().map(|record| record.stamp.clone());
    if let Some(latest) = &latest {
        let measured: FxHashSet<Key> = current.iter().map(Measurement::key).collect();
        let mut gone: FxHashSet<Key> = FxHashSet::default();
        for record in history.iter().filter(|record| record.stamp == *latest) {
            let key = record.measurement.key();
            if !measured.contains(&key) && gone.insert(key.clone()) {
                findings.push(Finding::Gone { key, at: record.stamp.clone() });
            }
        }
    }
    if not_comparable > 0 {
        findings.push(Finding::NotComparable {
            count: not_comparable,
            interpreter: interpreter.to_string(),
        });
    }
    Comparison { findings, measured: current.len(), latest }
}

/// Where a comparison's findings are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// A GitHub Actions log, which turns a workflow command into an
    /// annotation on the run and on the pull request.
    Actions,
    /// A terminal or a file.
    Plain,
}

impl Channel {
    /// The channel of the process's own environment.
    #[must_use]
    pub fn of_environment() -> Self {
        Self::of_github_actions(std::env::var("GITHUB_ACTIONS").ok().as_deref())
    }

    /// The channel `GITHUB_ACTIONS` names: Actions when it is `true`, as the
    /// runner sets it, and plain lines otherwise, unset included.
    #[must_use]
    pub fn of_github_actions(value: Option<&str>) -> Self {
        match value {
            Some("true") => Self::Actions,
            _ => Self::Plain,
        }
    }

    /// `finding` as one line of this channel.
    #[must_use]
    pub fn line_for(self, finding: &Finding) -> String {
        let text = finding.to_string();
        match (self, finding.is_warning()) {
            (Self::Actions, true) => {
                format!("::warning title={ANNOTATION_TITLE}::{}", escape_workflow_data(&text))
            }
            (Self::Actions, false) => {
                format!("::notice title={ANNOTATION_TITLE}::{}", escape_workflow_data(&text))
            }
            (Self::Plain, true) => format!("warning: {text}"),
            (Self::Plain, false) => format!("note: {text}"),
        }
    }
}

/// `path` as a reader of a log finds it: relative to the repository when it
/// is inside it, `/`-separated on every platform, and as given otherwise.
fn shown(path: &Path) -> String {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap_or(Path::new(""));
    match path.strip_prefix(repository) {
        Ok(inside) => inside
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => path.display().to_string(),
    }
}

/// `text` as the data of a workflow command, which GitHub reads with `%`,
/// carriage return and line feed percent-encoded.
fn escape_workflow_data(text: &str) -> String {
    text.replace('%', "%25").replace('\r', "%0D").replace('\n', "%0A")
}

/// The lines a comparison prints: its warnings, then its notes, then a
/// summary, which is a plain line on every channel.
///
/// On [`Channel::Actions`] one warning counting the others goes first when
/// there are any, because GitHub annotates only the first few warnings of a
/// step and a pull request would otherwise show a handful of a long list as
/// though it were all of it.
#[must_use]
pub fn report(
    comparison: &Comparison,
    history: &Path,
    threshold: u32,
    channel: Channel,
) -> Vec<String> {
    let (warnings, notes): (Vec<&Finding>, Vec<&Finding>) =
        comparison.findings.iter().partition(|finding| finding.is_warning());
    let history = match &comparison.latest {
        Some(stamp) => {
            format!("{}, whose latest snapshot is {}", shown(history), stamp.reference())
        }
        None => format!("{}, which holds no snapshot yet", shown(history)),
    };
    let mut lines = Vec::new();
    if channel == Channel::Actions && !warnings.is_empty() {
        let count = format!(
            "{} of the figures measured grew past the {threshold}% threshold against {history}; \
             this job's log lists every one",
            warnings.len()
        );
        lines.push(format!("::warning title={ANNOTATION_TITLE}::{}", escape_workflow_data(&count)));
    }
    lines.extend(warnings.iter().chain(&notes).map(|finding| channel.line_for(finding)));
    let count = |kind: fn(&Finding) -> bool| comparison.findings.iter().filter(|f| kind(f)).count();
    let not_comparable = comparison
        .findings
        .iter()
        .map(|finding| match finding {
            Finding::NotComparable { count, .. } => *count,
            _ => 0,
        })
        .sum::<usize>();
    lines.push(format!(
        "spacewasm-bench: compared {} measurements with {history}, at a {threshold}% \
         threshold: {} grew past it, {} end another way, {} new, {} gone, {not_comparable} not \
         comparable",
        comparison.measured,
        warnings.len(),
        count(|f| matches!(f, Finding::OutcomeChanged { .. })),
        count(|f| matches!(f, Finding::New { .. })),
        count(|f| matches!(f, Finding::Gone { .. })),
    ));
    lines
}

// ---------------------------------------------------------------------------
// The command line
// ---------------------------------------------------------------------------

/// One parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Measure, and append the snapshot to `history`.
    Record {
        /// The history to append to.
        history: PathBuf,
        /// The commit to stamp, when the command line names one rather than
        /// leaving it to `git rev-parse HEAD`.
        commit: Option<String>,
    },
    /// Measure, and compare with `history`.
    Compare {
        /// The history to compare with.
        history: PathBuf,
        /// Growth, in whole percent, past which a figure is warned about.
        threshold: u32,
    },
}

/// The history this repository commits.
#[must_use]
pub fn committed_history() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("bench").join("spacewasm").join("history.jsonl")
}

impl Command {
    /// Reads `argv`, which excludes the program name.
    ///
    /// # Errors
    ///
    /// Why the command line is not one this benchmark takes.
    pub fn parse(argv: &[String]) -> Result<Self, String> {
        let Some((command, options)) = argv.split_first() else {
            return Err("no command was given; it is `record` or `compare`".to_string());
        };
        if !matches!(command.as_str(), "record" | "compare") {
            return Err(format!("unknown command `{command}`; it is `record` or `compare`"));
        }
        let mut history = committed_history();
        let mut commit = None;
        let mut threshold = DEFAULT_THRESHOLD_PERCENT;
        let mut index = 0;
        while index < options.len() {
            let option = options[index].as_str();
            let value = options
                .get(index + 1)
                .map(String::as_str)
                .ok_or_else(|| format!("`{option}` needs a value"))?;
            match (command.as_str(), option) {
                (_, "--history") => history = PathBuf::from(value),
                ("record", "--commit") => commit = Some(value.to_string()),
                ("compare", "--threshold") => {
                    threshold = value.parse().map_err(|_| {
                        format!("`--threshold` takes a whole percentage, not `{value}`")
                    })?;
                }
                _ => return Err(format!("`{command}` takes no option `{option}`")),
            }
            index += 2;
        }
        if command == "record" {
            Ok(Self::Record { history, commit })
        } else {
            Ok(Self::Compare { history, threshold })
        }
    }
}

/// The `spacewasm-bench` example, whole.
///
/// `argv` excludes the program name. The channel is read from the process's
/// environment, and the day and the commit from the clock and from git.
///
/// The `allow` is the counterpart of the one on the embed harness's `run`:
/// two binaries compile this file and only the example calls it, while the
/// test binary drives the pieces below it one at a time.
#[allow(dead_code)]
#[must_use]
pub fn run(argv: &[String]) -> ExitCode {
    let (stdout, stderr) = (std::io::stdout(), std::io::stderr());
    let (mut out, mut err) = (stdout.lock(), stderr.lock());
    let command = match Command::parse(argv) {
        Ok(command) => command,
        Err(message) => {
            line(&mut err, &format!("error: {message}"));
            line(&mut err, USAGE_LINE);
            return ExitCode::from(exit::USAGE);
        }
    };
    let code = match command {
        Command::Record { history, commit } => match commit.map_or_else(head_commit, Ok) {
            Ok(commit) => record(&history, commit, &mut out, &mut err),
            Err(why) => {
                line(&mut err, &format!("error: {why}; name the commit with `--commit`"));
                exit::FAILED
            }
        },
        Command::Compare { history, threshold } => {
            let channel = Channel::of_environment();
            compare_with(&history, threshold, channel, &mut out, &mut err)
        }
    };
    ExitCode::from(code)
}

/// Measures, and appends the snapshot to `history`, stamped with `commit`.
fn record(history: &Path, commit: String, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
    let stamp = Stamp { date: utc_date(seconds), commit, interpreter: LIMITS_FROM.to_string() };
    let measurements = measure(err);
    let lines: String = measurements
        .iter()
        .map(|measurement| {
            let record = Record { stamp: stamp.clone(), measurement: measurement.clone() };
            record.to_line() + "\n"
        })
        .collect();
    if let Err(e) = append(history, &lines) {
        line(err, &format!("error: cannot append to {}: {e}", history.display()));
        return exit::FAILED;
    }
    line(
        out,
        &format!(
            "spacewasm-bench: recorded {} measurements at {} under {} in {}",
            measurements.len(),
            stamp.reference(),
            stamp.interpreter,
            shown(history)
        ),
    );
    exit::OK
}

/// Appends `lines` to the file at `path`, creating it if there is none, and
/// starting them on a line of their own when the file does not end with one.
///
/// # Errors
///
/// When the file cannot be read or written.
pub fn append(path: &Path, lines: &str) -> std::io::Result<()> {
    let existing = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e),
    };
    let separator = match existing.last() {
        Some(b'\n') | None => "",
        Some(_) => "\n",
    };
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(format!("{separator}{lines}").as_bytes())
}

/// The commit `HEAD` names in this repository.
fn head_commit() -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .map_err(|e| format!("cannot run `git rev-parse HEAD`: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "`git rev-parse HEAD` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Measures, and compares with `history`, writing the report to `out`.
///
/// The history is read first, so a history that cannot be read or parsed is
/// refused, with [`exit::FAILED`], before anything is measured. Every other
/// run answers [`exit::OK`], whatever the comparison found.
pub fn compare_with(
    history: &Path,
    threshold: u32,
    channel: Channel,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let recorded = match std::fs::read_to_string(history) {
        Ok(text) => parse_history(&text),
        Err(e) => Err(format!("cannot read it: {e}")),
    };
    let recorded = match recorded {
        Ok(recorded) => recorded,
        Err(why) => {
            line(err, &format!("error: {}: {why}", history.display()));
            return exit::FAILED;
        }
    };
    let measurements = measure(err);
    let comparison = compare(&recorded, &measurements, LIMITS_FROM, threshold);
    for text in report(&comparison, history, threshold, channel) {
        line(out, &text);
    }
    exit::OK
}

/// Writes one line, ignoring a failed write, as the embed harness does.
fn line(sink: &mut dyn Write, text: &str) {
    let _ = writeln!(sink, "{text}");
}
