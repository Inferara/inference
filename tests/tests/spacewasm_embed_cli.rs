//! The embedder harness, driven end to end.
//!
//! The `spacewasm-embed` example is three lines over
//! [`support::run`](../spacewasm/support.rs), and this binary is why: it calls
//! the same entry point in process, with both output streams captured, so every
//! way a run can end is a row here rather than a thing a developer once saw on
//! a terminal.
//!
//! In process and not through the built binary. `CARGO_BIN_EXE_*` — the way the
//! `rocq-discharge` CLI test finds its subject — is set only for a package's
//! `[[bin]]` targets, and the harness is deliberately an example so that it can
//! see `spacewasm`, which is a dev-dependency. What is left is shelling out to
//! `cargo run --example` from inside `cargo test`, which contends for the build
//! lock the test run is already holding, on every CI leg at once.
//!
//! This is a binary of its own, so it declares the interpreter's allocator
//! singleton at its own root, exactly as the corpus-sweep binary next door does
//! and for the same reason: the macro expands to `static mut` globals, and one
//! binary is one interpreter.

#[path = "spacewasm/support.rs"]
mod support;

use std::path::PathBuf;

use inference_tests::corpus::wasm_for_target;
use inference_wasm_codegen::Target;

spacewasm::global_allocator!(support::StdAllocator, support::StdAllocator);

// ---------------------------------------------------------------------------
// Rigging
// ---------------------------------------------------------------------------

/// A module on disk for the length of one test.
///
/// The harness takes a path because an embedder does; nothing here is testable
/// through a byte slice.
struct Artifact {
    /// Dropped with the test, taking the file with it.
    _dir: tempfile::TempDir,
    path: PathBuf,
}

impl Artifact {
    /// Writes `wasm` where the harness can be pointed at it.
    fn new(wasm: &[u8]) -> Self {
        let dir = tempfile::TempDir::new().expect("a temporary directory");
        let path = dir.path().join("module.wasm");
        std::fs::write(&path, wasm).expect("the module file is writable");
        Self { _dir: dir, path }
    }

    /// Compiles `source` at the SpaceWasm target and writes what it produced.
    fn compiled(source: &str) -> Self {
        Self::new(&wasm_for_target(source, Target::SpaceWasm))
    }

    /// Assembles `text` and writes what it produced, for the shapes this
    /// compiler cannot emit.
    fn assembled(text: &str) -> Self {
        Self::new(&wat::parse_str(text).unwrap_or_else(|e| panic!("the fixture is not WAT: {e}")))
    }

    /// The path as the command line spells it.
    fn arg(&self) -> String {
        self.path.display().to_string()
    }
}

/// What one harness run did.
struct Run {
    code: u8,
    out: String,
    err: String,
}

impl Run {
    /// Runs the harness on `argv`, which excludes the program name.
    fn of(argv: &[&str]) -> Self {
        let argv: Vec<String> = argv.iter().map(|arg| (*arg).to_string()).collect();
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = support::run_with(&argv, &mut out, &mut err);
        Self {
            code,
            out: String::from_utf8(out).expect("the harness writes UTF-8 to stdout"),
            err: String::from_utf8(err).expect("the harness writes UTF-8 to stderr"),
        }
    }

    /// Everything the run printed, for a failure message that shows the run.
    fn transcript(&self) -> String {
        format!("exit {}\n--- stdout ---\n{}--- stderr ---\n{}", self.code, self.out, self.err)
    }
}

/// The JSON keys the harness documents, read out of the table that defines
/// them.
///
/// The table lives in the module that emits the object, and a test comparing
/// the emitted keys against a copy of it here would go on passing while the two
/// drifted apart — which is the drift that matters, since the table is what a
/// benchmark tracker is pointed at. Sorted, for comparison against a sorted set
/// of emitted keys.
fn documented_keys() -> Vec<String> {
    const SUPPORT: &str = include_str!("spacewasm/support.rs");
    const HEADING: &str = "//! | key | meaning |";

    let mut keys: Vec<String> = SUPPORT
        .lines()
        .skip_while(|line| *line != HEADING)
        .skip(2)
        .take_while(|line| line.starts_with("//! |"))
        .map(|row| {
            row.split('`')
                .nth(1)
                .unwrap_or_else(|| panic!("a key row names its key in backticks: {row}"))
                .to_string()
        })
        .collect();
    assert!(!keys.is_empty(), "the key table is where the JSON contract is written down");
    keys.sort_unstable();
    keys
}

/// One export returning a constant.
const ANSWER: &str = "pub fn answer() -> i32 { return 42; }";

/// One export returning nothing.
const UNIT: &str = "pub fn nothing() { }";

/// One export the caller has to widen an argument for.
const ECHO_I64: &str = "pub fn echo64(v: i64) -> i64 { return v; }";

/// One export that gives back whatever narrow integer it was handed.
const ECHO_I32: &str = "pub fn echo32(v: i32) -> i32 { return v; }";

/// One export that traps on the vector the guard was added for.
const CHECKED_ADD: &str = "pub fn i32_add(a: i32, b: i32) -> i32 { return a + b; }";

/// One export that runs for far longer than a hundred instructions.
const SPIN: &str = r"pub fn spin() -> i32 {
    let mut i: i32 = 0;
    loop i < 1000000 {
        i = i + 1;
    }
    return i;
}";

// ---------------------------------------------------------------------------
// Invocation
// ---------------------------------------------------------------------------

/// A returned value is printed beside the name that produced it.
///
/// Fails if the result line loses the export name or the value, or if a
/// successful run stops exiting zero.
#[test]
fn an_i32_result_prints_as_name_equals_value() {
    let artifact = Artifact::compiled(ANSWER);
    let run = Run::of(&[&artifact.arg(), "--invoke", "answer"]);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert_eq!(run.out, "answer = 42\n", "{}", run.transcript());
    assert!(run.err.is_empty(), "{}", run.transcript());
}

/// A function that returns nothing says so, rather than printing an empty value
/// or nothing at all.
///
/// Fails if the unit case starts rendering as `answer = ` or is silently
/// skipped — either of which would make "it ran" and "it ran and returned
/// nothing" indistinguishable at a terminal.
#[test]
fn a_unit_result_prints_as_name_equals_unit() {
    let artifact = Artifact::compiled(UNIT);
    let run = Run::of(&[&artifact.arg(), "--invoke", "nothing"]);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert_eq!(run.out, "nothing = (unit)\n", "{}", run.transcript());
}

/// A 64-bit argument survives the command line.
///
/// The value is `i64::MAX`, which is the whole point: an argument parsed into
/// an `i32` and widened, or parsed as `f64`, would come back wrong rather than
/// come back at all. Fails if the coercion stops reading the declared parameter
/// type and picks a width of its own.
#[test]
fn an_i64_argument_round_trips_through_the_command_line() {
    let artifact = Artifact::compiled(ECHO_I64);
    let run = Run::of(&[&artifact.arg(), "--invoke", "echo64", "9223372036854775807"]);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert_eq!(run.out, "echo64 = 9223372036854775807\n", "{}", run.transcript());
}

/// A negative argument is an argument, and the option after it is an option.
///
/// `--invoke`'s argument list ends at the first token starting with `--`, which
/// is the rule that lets one hyphen mean a sign and two mean an option. The
/// rows below pin the refusing half of that predicate; this one pins the half
/// the harness advertises. Fails if the terminator widens to a single hyphen —
/// `-1` would then end the list and be read as an unknown option — or narrows
/// so that `--fuel` is swallowed as a third argument to `echo32`.
#[test]
fn a_negative_argument_is_read_as_an_argument_and_not_as_an_option() {
    let artifact = Artifact::compiled(ECHO_I32);
    let run = Run::of(&[&artifact.arg(), "--invoke", "echo32", "-1", "--fuel", "100000"]);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert_eq!(run.out, "echo32 = -1\n", "{}", run.transcript());
}

/// A trap is its own exit code, and the reason reaches the terminal.
///
/// The vector is the one the checked-arithmetic guard was added for, and the
/// guard lowers an overflowing addition to `unreachable`, so the reason pinned
/// here is the interpreter's own spelling of that instruction: an overflow that
/// stopped trapping, or that started arriving as some other trap, fails here
/// rather than passing on any non-blank reason. Fails too if a trap starts
/// sharing an exit code with a decode failure or a missing export, or if the
/// reason stops being printed — a flight integrator reading a CI log has only
/// these two things.
#[test]
fn a_trapping_call_exits_with_the_trap_code_and_names_the_reason() {
    let artifact = Artifact::compiled(CHECKED_ADD);
    let run = Run::of(&[&artifact.arg(), "--invoke", "i32_add", "2147483647", "1"]);
    assert_eq!(run.code, support::exit::TRAP, "{}", run.transcript());
    let transcript = run.transcript();
    let reason = run
        .err
        .split("trapped: ")
        .nth(1)
        .unwrap_or_else(|| panic!("the reason follows the colon: {transcript}"));
    assert_eq!(
        reason.trim(),
        "Unreachable",
        "the guard lowers an overflow to `unreachable`: {transcript}"
    );
    assert!(run.out.is_empty(), "a trap produces no result line: {transcript}");
}

/// The same program under a budget it cannot finish in ends out of fuel, not in
/// a trap and not in a hang.
///
/// Fails if the fuel budget stops being read off `--fuel`, or if exhausting it
/// is reported as anything else: "the program is wrong" and "the budget was too
/// small" are opposite conclusions for whoever reads the log.
#[test]
fn an_unfinished_program_under_a_tiny_budget_exits_out_of_fuel() {
    let artifact = Artifact::compiled(SPIN);
    let run = Run::of(&[&artifact.arg(), "--invoke", "spin", "--fuel", "100"]);
    assert_eq!(run.code, support::exit::OUT_OF_FUEL, "{}", run.transcript());
    assert!(run.err.contains("still running after 100 instructions"), "{}", run.transcript());
}

/// The same program with the default budget finishes.
///
/// The control for the row above: without it, an out-of-fuel verdict would be
/// satisfied by a harness that never runs anything. Fails if the default budget
/// stops being enough for a program of this size, which is the day the sweeps'
/// shared `FUEL` needs revisiting too.
#[test]
fn the_same_program_finishes_on_the_default_budget() {
    let artifact = Artifact::compiled(SPIN);
    let run = Run::of(&[&artifact.arg(), "--invoke", "spin"]);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert_eq!(run.out, "spin = 1000000\n", "{}", run.transcript());
}

/// Asking for an export that is not there says what is there.
///
/// Fails if the harness starts panicking on a missing export — the shared
/// `LoadedModule::invoke` does, by design, since inside a sweep that is a
/// harness fault — or if the list of what the module does export is dropped.
#[test]
fn a_missing_export_exits_with_its_own_code_and_lists_what_there_is() {
    let artifact = Artifact::compiled(ANSWER);
    let run = Run::of(&[&artifact.arg(), "--invoke", "absent"]);
    assert_eq!(run.code, support::exit::NO_SUCH_EXPORT, "{}", run.transcript());
    assert!(run.err.contains("exports no function `absent`"), "{}", run.transcript());
    assert!(run.err.contains("answer() -> i32"), "{}", run.transcript());
}

/// An export that returns nothing is described without a result type.
///
/// The listing is the only place a signature is rendered, and it is what a
/// reader copies back onto the command line. Fails if the arrow starts printing
/// unconditionally, which would advertise a result the function does not have.
#[test]
fn an_export_returning_nothing_is_described_without_an_arrow() {
    let artifact = Artifact::compiled(UNIT);
    let run = Run::of(&[&artifact.arg()]);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert!(run.out.contains("nothing()"), "{}", run.transcript());
    assert!(
        !run.out.contains("nothing() ->"),
        "a function returning nothing has no result type: {}",
        run.transcript()
    );
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// A module whose imports nothing supplies is a decode failure that names them.
///
/// The interpreter's own verdict is `FunctionImportNotFound` and carries no
/// names — a flight decoder keeps no string it does not have to — so naming the
/// import is the harness reading the module a second time. Fails if it stops,
/// which would leave a two-import module reporting one anonymous refusal.
///
/// The section the decoder stopped in is pinned here as well. It is the one
/// conditional line of the refusal, and this is the fixture that carries it:
/// the verdict on an unresolved import arrives from inside the import section,
/// where the malformed-tail row next door is refused before any section has
/// been entered and prints no such line.
#[test]
fn an_unresolvable_import_is_a_decode_failure_that_names_the_import() {
    let artifact = Artifact::assembled(
        r#"(module
             (import "env" "clock_ms" (func $clock (result i64)))
             (func (export "now") (result i64) (call $clock)))"#,
    );
    let run = Run::of(&[&artifact.arg(), "--invoke", "now"]);
    assert_eq!(run.code, support::exit::DECODE, "{}", run.transcript());
    assert!(
        run.err.contains("does not load under the SpaceWasm interpreter"),
        "{}",
        run.transcript()
    );
    assert!(run.err.contains("  reading the Import section"), "{}", run.transcript());
    assert!(run.err.contains("env.clock_ms"), "{}", run.transcript());
    assert!(run.err.contains("issue #464"), "{}", run.transcript());
}

/// A refused module that imports nothing gets no import list.
///
/// The control for the row above: without it, "names the import" would be
/// satisfied by a harness that prints the same three lines under every decode
/// failure, and a malformed module would be reported as though an embedder
/// could have rescued it. Fails if the emptiness check goes away, which would
/// leave a bare heading and a `#464` pointer under a module that imports
/// nothing.
#[test]
fn a_refused_module_that_imports_nothing_names_no_import() {
    let artifact = Artifact::new(b"\0asm\x01\0\0\0extra");
    let run = Run::of(&[&artifact.arg()]);
    assert_eq!(run.code, support::exit::DECODE, "{}", run.transcript());
    assert!(!run.err.contains("issue #464"), "{}", run.transcript());
    assert!(!run.err.contains("registers no host module"), "{}", run.transcript());
}

/// A path that is not a file is its own failure, distinct from bytes the
/// interpreter refused.
///
/// Fails if a mistyped path starts reading as a malformed module, which sends
/// the reader to debug an artifact that was never opened.
#[test]
fn an_unreadable_module_exits_with_its_own_code() {
    let dir = tempfile::TempDir::new().expect("a temporary directory");
    let missing = dir.path().join("nothing-here.wasm");
    let run = Run::of(&[&missing.display().to_string()]);
    assert_eq!(run.code, support::exit::UNREADABLE, "{}", run.transcript());
    assert!(run.err.contains("cannot read"), "{}", run.transcript());
}

/// A run that asks for nothing still reports what it loaded.
///
/// Fails if a bare load goes silent, which would make "this artifact decodes on
/// the flight interpreter" — the cheapest question the harness answers —
/// indistinguishable from a harness that did nothing at all.
#[test]
fn a_bare_run_reports_what_it_loaded() {
    let artifact = Artifact::compiled(ECHO_I64);
    let run = Run::of(&[&artifact.arg()]);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert!(run.out.contains("loaded "), "{}", run.transcript());
    assert!(run.out.contains("echo64(i64) -> i64"), "{}", run.transcript());
}

// ---------------------------------------------------------------------------
// Measurement
// ---------------------------------------------------------------------------

/// `--stats --json` is one object on the first line, with exactly the keys the
/// module documentation defines.
///
/// The first-line rule is the contract that lets `--json` coexist with
/// `--invoke`: a reader takes line one and gets the whole measurement. The keys
/// are checked as a set against the module's own table rather than against a
/// copy of it, so a key added without a documentation line fails here and so
/// does a documented key the harness stopped emitting; and the two derived
/// figures are recomputed from the raw ones, so a ratio wired to the wrong
/// numerator fails too. The ratio is compared exactly rather than to a
/// tolerance, which is what holds it at full precision: a key rounded for a
/// reader is a key a tracker cannot see a small regression through, and two
/// decimals is around eight percent of the figure this fixture produces.
#[test]
fn stats_json_is_one_documented_object_on_the_first_line() {
    let artifact = Artifact::compiled(ANSWER);
    let run = Run::of(&[&artifact.arg(), "--stats", "--json", "--invoke", "answer"]);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());

    let mut lines = run.out.lines();
    let object = lines.next().expect("the object is the first line");
    assert_eq!(lines.next(), Some("answer = 42"), "{}", run.transcript());
    assert_eq!(lines.next(), None, "{}", run.transcript());

    let parsed: serde_json::Value =
        serde_json::from_str(object).unwrap_or_else(|e| panic!("`{object}` is not JSON: {e}"));
    let object = parsed.as_object().expect("the line is a JSON object");
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        documented_keys(),
        "the documented key set is the contract a benchmark tracker would read"
    );

    let number = |key: &str| object[key].as_f64().unwrap_or_else(|| panic!("`{key}` is a number"));
    assert!(number("code_pages") >= 1.0, "{}", run.transcript());
    assert!(number("ir_words") >= 1.0, "{}", run.transcript());
    assert!(
        (number("ir_bytes") - 2.0 * number("ir_words")).abs() < f64::EPSILON,
        "IR bytes are IR words at two bytes each: {}",
        run.transcript()
    );
    // The one figure with an authority outside the object. Every other
    // assertion here recomputes one field from another, and a measurement
    // consistent with itself is what a harness reading nothing also produces.
    let on_disk = std::fs::metadata(&artifact.path).expect("the artifact is on disk").len();
    assert!(
        (number("wasm_bytes") - on_disk as f64).abs() < f64::EPSILON,
        "the denominator is the artifact's own size, {on_disk} bytes: {}",
        run.transcript()
    );
    let expected = number("ir_bytes") / number("wasm_bytes");
    assert!(
        (number("ir_bytes_per_wasm_byte") - expected).abs() < f64::EPSILON,
        "the ratio divides IR bytes by wasm bytes, unrounded: {}",
        run.transcript()
    );
}

/// A measurement is taken and printed even when the call it precedes traps.
///
/// The measurement describes the module, and a module that trapped is exactly
/// the one somebody wants the measurement of. Fails if the block is moved below
/// the invocation or made conditional on the outcome, either of which would
/// lose the figures for every run that did not end cleanly — and the JSON line
/// would stop being the first line of stdout, which is the contract a reader
/// taking `head -1` relies on.
#[test]
fn a_trapping_run_still_reports_the_measurement_first() {
    let artifact = Artifact::compiled(CHECKED_ADD);
    let run = Run::of(&[
        &artifact.arg(),
        "--stats",
        "--json",
        "--invoke",
        "i32_add",
        "2147483647",
        "1",
    ]);
    assert_eq!(run.code, support::exit::TRAP, "{}", run.transcript());
    let object = run.out.lines().next().unwrap_or_else(|| panic!("{}", run.transcript()));
    let parsed: serde_json::Value =
        serde_json::from_str(object).unwrap_or_else(|e| panic!("`{object}` is not JSON: {e}"));
    assert!(
        parsed["ir_words"].as_f64().is_some_and(|words| words >= 1.0),
        "{}",
        run.transcript()
    );
    assert!(run.err.contains("trapped: "), "{}", run.transcript());
}

/// Without `--json` the same measurement is human lines carrying the same
/// numbers, plus the capacity only that rendering reports.
///
/// The two renderings are compared against each other rather than grepped for
/// labels, because two descriptions of one artifact that disagree are worse
/// than one. Fails if the human rendering drops a figure, prints one under
/// another's label, or loses the page capacity or the share of it in use — and
/// the line count fails if a `--stats` run starts printing anything besides the
/// measurement it asked for.
///
/// The 256 words to a page is spelled here and not read from the harness: an
/// assertion written in terms of the constant it guards moves with it.
#[test]
fn stats_without_json_prints_the_same_figures_the_json_carries() {
    let artifact = Artifact::compiled(ANSWER);

    let json = Run::of(&[&artifact.arg(), "--stats", "--json"]);
    assert_eq!(json.code, support::exit::OK, "{}", json.transcript());
    let object = json.out.lines().next().unwrap_or_else(|| panic!("{}", json.transcript()));
    let parsed: serde_json::Value =
        serde_json::from_str(object).unwrap_or_else(|e| panic!("`{object}` is not JSON: {e}"));
    let whole = |key: &str| {
        parsed[key].as_u64().unwrap_or_else(|| panic!("`{key}` is a whole number: {object}"))
    };
    let (pages, words, bytes, wasm) =
        (whole("code_pages"), whole("ir_words"), whole("ir_bytes"), whole("wasm_bytes"));

    let run = Run::of(&[&artifact.arg(), "--stats"]);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    let capacity = pages * 256;
    for expected in [
        format!("code pages: {pages}"),
        format!(
            "IR words (16-bit): {words} / {capacity} ({:.2}%)",
            100.0 * words as f64 / capacity as f64
        ),
        format!("IR bytes: {bytes}"),
        format!("wasm bytes: {wasm}"),
        format!("IR bytes per wasm byte: {:.2}", bytes as f64 / wasm as f64),
    ] {
        assert!(run.out.contains(&expected), "missing `{expected}`: {}", run.transcript());
    }
    assert_eq!(
        run.out.lines().count(),
        5,
        "the measurement is the whole report of a --stats run: {}",
        run.transcript()
    );
}

/// A module with no code at all measures as zero words of zero, not as `NaN%`.
///
/// The percentage is a share of the page capacity, and a module with no
/// function bodies fills no page, so the capacity it would be a share of is
/// zero. This is the only fixture that reaches that arm — every other module in
/// the matrix compiles to at least one page — and it prints `NaN%` the moment
/// the guard is dropped, which is what an integrator would be shown for an
/// artifact carrying no code.
#[test]
fn a_module_with_no_code_measures_as_zero_words_of_zero() {
    let artifact = Artifact::assembled("(module)");
    let run = Run::of(&[&artifact.arg(), "--stats"]);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert!(run.out.contains("code pages: 0"), "{}", run.transcript());
    assert!(run.out.contains("IR words (16-bit): 0 / 0 (0.00%)"), "{}", run.transcript());
}

// ---------------------------------------------------------------------------
// The command line itself
// ---------------------------------------------------------------------------

/// An unreadable command line is its own exit code, and says how to write one.
///
/// Fails if a typo starts exiting like a trap, or if the usage line stops being
/// printed beside the complaint.
#[test]
fn an_unknown_option_is_a_usage_error() {
    let artifact = Artifact::compiled(ANSWER);
    let run = Run::of(&[&artifact.arg(), "--verbose"]);
    assert_eq!(run.code, support::exit::USAGE, "{}", run.transcript());
    assert!(run.err.contains("unknown option `--verbose`"), "{}", run.transcript());
    assert!(run.err.contains("usage: spacewasm-embed"), "{}", run.transcript());
}

/// No arguments at all is a usage error rather than a panic.
///
/// Fails if the module path stops being required, which would leave the first
/// missing-index read to decide what happens.
#[test]
fn an_empty_command_line_is_a_usage_error() {
    let run = Run::of(&[]);
    assert_eq!(run.code, support::exit::USAGE, "{}", run.transcript());
    assert!(run.err.contains("no module was given"), "{}", run.transcript());
}

/// `--json` on its own is refused rather than ignored.
///
/// It selects the shape of a measurement that was not asked for, so honouring
/// it silently would print nothing and exit zero. Fails if the pairing check is
/// dropped.
#[test]
fn json_without_stats_is_a_usage_error() {
    let artifact = Artifact::compiled(ANSWER);
    let run = Run::of(&[&artifact.arg(), "--json"]);
    assert_eq!(run.code, support::exit::USAGE, "{}", run.transcript());
    assert!(run.err.contains("needs `--stats` beside it"), "{}", run.transcript());
}

/// `--fuel` without a budget, and with something that is not one, are both
/// refused.
///
/// Fails if either stops being checked: an option that swallows the next flag
/// would turn `--fuel --stats` into a run with no statistics and a parse error
/// nobody sees.
#[test]
fn fuel_needs_a_decimal_budget() {
    let artifact = Artifact::compiled(ANSWER);

    let missing = Run::of(&[&artifact.arg(), "--fuel", "--stats"]);
    assert_eq!(missing.code, support::exit::USAGE, "{}", missing.transcript());
    assert!(
        missing.err.contains("`--fuel` needs an instruction budget"),
        "{}",
        missing.transcript()
    );

    let malformed = Run::of(&[&artifact.arg(), "--fuel", "plenty"]);
    assert_eq!(malformed.code, support::exit::USAGE, "{}", malformed.transcript());
    assert!(malformed.err.contains("not `plenty`"), "{}", malformed.transcript());
}

/// The wrong number of arguments names both counts.
///
/// Fails if the arity check goes away and the shared `invoke` panics instead,
/// which turns a mistyped command line into a backtrace.
#[test]
fn the_wrong_argument_count_is_a_usage_error_naming_both() {
    let artifact = Artifact::compiled(CHECKED_ADD);
    let run = Run::of(&[&artifact.arg(), "--invoke", "i32_add", "1"]);
    assert_eq!(run.code, support::exit::USAGE, "{}", run.transcript());
    assert!(
        run.err.contains("takes 2 arguments; 1 argument given"),
        "{}",
        run.transcript()
    );
}

/// An argument that is not a decimal integer of the declared type is refused
/// with the position and the type.
///
/// Fails if a malformed argument starts defaulting to zero, which would run the
/// program on operands nobody asked for and report the answer as though it were
/// the one requested.
#[test]
fn a_malformed_integer_argument_names_its_position_and_type() {
    let artifact = Artifact::compiled(CHECKED_ADD);
    let run = Run::of(&[&artifact.arg(), "--invoke", "i32_add", "1", "two"]);
    assert_eq!(run.code, support::exit::USAGE, "{}", run.transcript());
    assert!(run.err.contains("argument 2 of `i32_add` is `i32`"), "{}", run.transcript());
}

/// A floating-point result is printed rather than refused, at both widths.
///
/// A parameter this harness cannot spell on a command line is refused, and the
/// row below says so; a *result* costs the caller nothing to receive, so the
/// two are separate decisions and the rendering arms for a float are reachable.
/// Both widths, because the type name and the value rendering each branch on
/// the width and a copy-paste that left one arm reading the other would print
/// a plausible wrong answer. This compiler emits no float, so the module is
/// assembled. Fails if the refusal widens from parameters to signatures, or if
/// either float arm is folded into another.
#[test]
fn a_floating_point_result_is_printed_at_both_widths() {
    let artifact = Artifact::assembled(
        r#"(module
             (func (export "half") (result f32) (f32.const 0.5))
             (func (export "quarter") (result f64) (f64.const 0.25)))"#,
    );

    let listing = Run::of(&[&artifact.arg()]);
    assert_eq!(listing.code, support::exit::OK, "{}", listing.transcript());
    assert!(listing.out.contains("half() -> f32"), "{}", listing.transcript());
    assert!(listing.out.contains("quarter() -> f64"), "{}", listing.transcript());

    let narrow = Run::of(&[&artifact.arg(), "--invoke", "half"]);
    assert_eq!(narrow.code, support::exit::OK, "{}", narrow.transcript());
    assert_eq!(narrow.out, "half = 0.5\n", "{}", narrow.transcript());

    let wide = Run::of(&[&artifact.arg(), "--invoke", "quarter"]);
    assert_eq!(wide.code, support::exit::OK, "{}", wide.transcript());
    assert_eq!(wide.out, "quarter = 0.25\n", "{}", wide.transcript());
}

/// A floating-point parameter is refused rather than guessed at.
///
/// This compiler emits no float, so the module is assembled; the harness loads
/// whatever it is pointed at, and the alternative to a refusal is a panic in
/// the shared `invoke` when the value handed over does not match the signature.
/// Fails if the arm is dropped or starts parsing floats without a decision
/// about how one is spelled.
#[test]
fn a_floating_point_parameter_is_refused_rather_than_guessed_at() {
    let artifact = Artifact::assembled(
        r#"(module (func (export "half") (param f32) (result f32) (local.get 0)))"#,
    );
    let run = Run::of(&[&artifact.arg(), "--invoke", "half", "0.5"]);
    assert_eq!(run.code, support::exit::USAGE, "{}", run.transcript());
    assert!(run.err.contains("is `f32`"), "{}", run.transcript());
    assert!(run.err.contains("decimal integers only"), "{}", run.transcript());
}

/// The module path comes first, and an option in its place is refused rather
/// than opened.
///
/// Fails if the first token stops being checked, which would leave the harness
/// reporting `--stats` as a file it could not read.
#[test]
fn an_option_in_the_module_slot_is_a_usage_error() {
    let run = Run::of(&["--stats", "module.wasm"]);
    assert_eq!(run.code, support::exit::USAGE, "{}", run.transcript());
    assert!(
        run.err.contains("expected a module path, found the option `--stats`"),
        "{}",
        run.transcript()
    );
}

/// `--invoke` needs a name, and only one of them.
///
/// Both rows guard the same thing from opposite sides: a run must call exactly
/// one export, so an `--invoke` with nothing after it and a second `--invoke`
/// are equally unanswerable. Fails if either check goes, which would silently
/// call the last-named export, or none, and report the exit code of whatever
/// it did instead.
#[test]
fn invoke_needs_exactly_one_export_name() {
    let artifact = Artifact::compiled(ANSWER);

    let nameless = Run::of(&[&artifact.arg(), "--invoke", "--stats"]);
    assert_eq!(nameless.code, support::exit::USAGE, "{}", nameless.transcript());
    assert!(
        nameless.err.contains("`--invoke` needs the name of an exported function"),
        "{}",
        nameless.transcript()
    );

    let twice = Run::of(&[&artifact.arg(), "--invoke", "answer", "--invoke", "answer"]);
    assert_eq!(twice.code, support::exit::USAGE, "{}", twice.transcript());
    assert!(
        twice.err.contains("`--invoke` was given twice"),
        "{}",
        twice.transcript()
    );
}

/// A 64-bit parameter refuses a malformed argument in its own words.
///
/// The `i32` arm has a row of its own; this one exists because the two are
/// separate branches over the declared type, and a copy-paste that left the
/// wider one reading `i32` would still refuse `two` — and would also refuse
/// every value above `i32::MAX`, silently, as though it were malformed. Fails
/// if the arm loses its own message.
#[test]
fn a_malformed_wide_argument_names_the_wide_type() {
    let artifact = Artifact::compiled(ECHO_I64);
    let run = Run::of(&[&artifact.arg(), "--invoke", "echo64", "two"]);
    assert_eq!(run.code, support::exit::USAGE, "{}", run.transcript());
    assert!(run.err.contains("argument 1 of `echo64` is `i64`"), "{}", run.transcript());
}

/// A module that exports no function says so rather than printing an empty
/// list.
///
/// Fails if the empty case is folded into the general one, which would leave a
/// heading with nothing under it and a reader wondering whether the list was
/// truncated.
#[test]
fn a_module_with_no_exports_says_so() {
    let artifact = Artifact::assembled("(module)");
    let run = Run::of(&[&artifact.arg(), "--invoke", "anything"]);
    assert_eq!(run.code, support::exit::NO_SUCH_EXPORT, "{}", run.transcript());
    assert!(run.err.contains("it exports no functions"), "{}", run.transcript());
}

/// Every way a run can end has an exit code of its own, and success is zero.
///
/// The rows above each pin one code, and every one of them would still pass if
/// the constants all held the same number — an equality between two names of
/// one value is not a distinction. Fails the moment two of them collide.
///
/// `OK` is the one value here with an authority outside this harness: a shell
/// reads zero as success whatever a constant is called, so it is asserted
/// outright rather than left to travel with the rows that cite it, which would
/// move with it. The failure codes are then required to be non-zero, which is
/// the property those rows are written to guard and the reason the two
/// assertions beside it are here.
///
/// The list is `support::exit::ALL` rather than a copy of it here, so a code
/// added to the harness and left out of the check is an omission a reader sees
/// on the same screen as the constants instead of an unguarded collision.
#[test]
fn every_failure_mode_has_an_exit_code_of_its_own() {
    let codes = support::exit::ALL;
    let mut sorted = codes.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), codes.len(), "two outcomes share an exit code: {codes:?}");
    assert_eq!(support::exit::OK, 0, "a run that worked exits zero");
    for code in codes {
        if code == support::exit::OK {
            continue;
        }
        assert_ne!(code, 0, "a run that ended badly exits non-zero: {codes:?}");
    }
}
