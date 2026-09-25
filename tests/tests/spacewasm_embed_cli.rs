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

use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;

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

/// Both streams of one harness run written into one buffer, so the order
/// between them survives.
///
/// [`Run`] keeps stdout and stderr apart, which is what nearly every row
/// compares. A row about the order *between* the two — a host call printed
/// before the result it led to — needs them interleaved as the harness wrote
/// them, and two handles of this sink share one buffer.
#[derive(Clone, Default)]
struct Interleaved(Rc<RefCell<Vec<u8>>>);

impl Write for Interleaved {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Interleaved {
    /// Runs the harness on `argv` and returns its exit code and everything it
    /// printed to either stream, in the order it printed it.
    fn of(argv: &[&str]) -> (u8, String) {
        let argv: Vec<String> = argv.iter().map(|arg| (*arg).to_string()).collect();
        let sink = Self::default();
        let code = support::run_with(&argv, &mut sink.clone(), &mut sink.clone());
        (code, String::from_utf8(sink.0.take()).expect("the harness writes UTF-8"))
    }
}

/// Asserts that a refused command line was answered with the usage line, and
/// that the line spells `--host`.
fn assert_usage_line(run: &Run) {
    assert!(
        run.err.contains(
            "usage: spacewasm-embed <module.wasm> [--invoke NAME [ARG…]] [--fuel N] \
             [--stats [--json]] [--host MODULE.FIELD=PARAMS[:RESULT]]…"
        ),
        "{}",
        run.transcript()
    );
}

/// The command line naming `--host` beside `specs`, one `--host` per spec,
/// after `head`.
fn with_hosts<'a>(head: &[&'a str], specs: &'a [String]) -> Vec<&'a str> {
    let mut argv = head.to_vec();
    for spec in specs {
        argv.extend(["--host", spec.as_str()]);
    }
    argv
}

/// The field `f` of the modules `m0` to `m{count - 1}`, in order.
fn one_field_from_each_module(count: usize) -> Vec<(String, String)> {
    (0..count).map(|index| (format!("m{index}"), "f".to_string())).collect()
}

/// A module importing one nullary `i32` function per `(module, field)`, in
/// order, and exporting `last`, which calls the last of them.
fn importing_each(imports: &[(String, String)]) -> Artifact {
    let mut text = String::from("(module\n");
    for (module, field) in imports {
        text.push_str(&format!("  (import \"{module}\" \"{field}\" (func (result i32)))\n"));
    }
    let last = imports.len() - 1;
    text.push_str(&format!("  (func (export \"last\") (result i32) (call {last})))"));
    Artifact::assembled(&text)
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

/// The committed F´ fixture: `report(channel)` announces itself through
/// `message`, sends `command(1, channel)`, downlinks a value through
/// `telemetry` under the id `command` answered with, and asks `clock_ms()` only
/// if telemetry answered zero. It binds all six hosts of the F´ reference set
/// and calls four of them. Read rather than copied, so this row and the codegen
/// golden are about one program.
const FPRIME: &str = include_str!(
    "../test_data/codegen/wasm/extern_import/host_import_fprime/host_import_fprime.inf"
);

/// Every host of the F´ reference set, each called whatever the call before it
/// answered: `go` calls the three hosts the committed program calls before the
/// clock (`message`, `command`, `telemetry`, passing `command`'s answer to
/// `telemetry` as its id), then the two that program never calls, `rsleep` and
/// `panic`, then asks the clock and returns what it said. It sleeps for more
/// than `u32::MAX` ticks, so an `i64` argument logged from its low word alone
/// would read as another number.
const FPRIME_EVERY_CALL: &str = r"external fn panic(text: [u8; 4], len: i32, line: i32);
external fn rsleep(ticks: i64);
external fn command(opcode: i32, arg: i32) -> i32;
external fn message(text: [u8; 2], len: i32);
external fn telemetry(id: i32, mut time: [u8; 11], time_len: i32, value: [u8; 4], value_len: i32) -> i32;
use { panic, rsleep, command, message, telemetry } from host::fprime_core;

external fn clock_ms() -> i64;
use { clock_ms } from host::env;

pub fn go() -> i64 {
    let hi: [u8; 2] = [104, 105];
    message(hi, 2);
    let id: i32 = command(42, 7);
    let mut time: [u8; 11] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let value: [u8; 4] = [21, 0, 0, 0];
    let status: i32 = telemetry(id, time, 11, value, 4);
    rsleep(5000000000);
    let boom: [u8; 4] = [98, 111, 111, 109];
    panic(boom, 4, 17);
    return clock_ms();
}";

/// A stub for each host of the F´ reference set, as the command line spells
/// them: every one at the signature the reference embedder registers it with.
const FPRIME_STUBS: [&str; 12] = [
    "--host",
    "fprime_core.panic=iii",
    "--host",
    "fprime_core.rsleep=I",
    "--host",
    "fprime_core.command=ii:i",
    "--host",
    "fprime_core.message=ii",
    "--host",
    "fprime_core.telemetry=iiiii:i",
    "--host",
    "env.clock_ms=:I",
];

/// A guest address in an expected `host call:` log, standing for whatever
/// decimal the compiler's frame layout put there. An array argument reaches a
/// host as the address of the caller's buffer, and a row pins that a call
/// carried an address in that position, not where the buffer was laid out.
const ADDRESS: &str = "<address>";

/// Whether `log` reads as `expected`, each [`ADDRESS`] in `expected` standing
/// for one unsigned decimal.
fn reads_as(log: &str, expected: &str) -> bool {
    let mut pieces = expected.split(ADDRESS);
    let head = pieces.next().unwrap_or_default();
    let Some(mut rest) = log.strip_prefix(head) else {
        return false;
    };
    for piece in pieces {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        match rest[digits..].strip_prefix(piece) {
            Some(tail) if digits > 0 => rest = tail,
            _ => return false,
        }
    }
    rest.is_empty()
}

/// One import and one export that calls it, the smallest module a host stub
/// makes loadable.
const CLOCK: &str = r#"(module
     (import "env" "clock_ms" (func $clock (result i64)))
     (func (export "now") (result i64) (call $clock)))"#;

/// The line closing a decode-failure report while some import has a spec and
/// no stub matching it.
const HOST_POINTER: &str = "  register a stub for each with `--host MODULE.FIELD=PARAMS[:RESULT]`, \
                            or run the module from the embedder that supplies them";

/// The line closing a decode-failure report while every import left
/// unsupplied is one no spec can supply, or while no `--host` set can supply
/// them all at once.
const EMBEDDER_POINTER: &str = "  run the module from the embedder that supplies them";

/// The line closing a decode-failure report while some imports left
/// unsupplied have a spec and the others have none.
const SOME_STUBS_POINTER: &str = "  `--host` can stub only some of these, so run the module from \
                                  the embedder that supplies them";

/// The line closing a decode-failure report on a module one of whose imports
/// the interpreter cannot load however it is supplied.
const NO_EMBEDDER: &str = "  no SpaceWasm embedder can load this module as built";

/// The line closing a decode-failure report on a module refused outside its
/// import section while some import is still unsupplied.
const REFUSED_BEFORE_IMPORTS: &str = "  the interpreter refused the module before it bound any \
                                      import, so no stub changes this verdict";

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

/// A module whose imports nothing supplies is a decode failure that names them,
/// each with the spec that would supply it.
///
/// The interpreter's own verdict is `FunctionImportNotFound` and carries no
/// names — a flight decoder keeps no string it does not have to — so naming the
/// import is the harness reading the module a second time. Fails if it stops,
/// which would leave a two-import module reporting one anonymous refusal; if
/// the spec it prints stops being the import's own signature; or if the line
/// pointing at `--host` stops closing the report.
///
/// The section the decoder stopped in is pinned here as well. It is the one
/// conditional line of the refusal, and this is the fixture that carries it:
/// the verdict on an unresolved import arrives from inside the import section,
/// where the malformed-tail row next door is refused before any section has
/// been entered and prints no such line.
#[test]
fn an_unresolvable_import_is_a_decode_failure_that_names_the_import() {
    let artifact = Artifact::assembled(CLOCK);
    let run = Run::of(&[&artifact.arg(), "--invoke", "now"]);
    assert_eq!(run.code, support::exit::DECODE, "{}", run.transcript());
    assert!(
        run.err.contains("does not load under the SpaceWasm interpreter: FunctionImportNotFound"),
        "{}",
        run.transcript()
    );
    assert!(run.err.contains("  reading the Import section"), "{}", run.transcript());
    assert!(
        run.err.contains(
            "  it imports these, and no `--host` stub was registered for any of them:\n    \
             env.clock_ms: no stub; it needs `--host env.clock_ms=:I`\n"
        ),
        "{}",
        run.transcript()
    );
    assert_eq!(run.err.lines().last(), Some(HOST_POINTER), "{}", run.transcript());
}

/// A stub for the import lets the module load and run, and the call it made is
/// printed before the result it led to.
///
/// The stub answers zero, so `now = 0` is the program handing back what its
/// host said, and the one `host call:` line is the only record that the host
/// was asked at all. The call log is held while the interpreter runs and
/// printed once it returns, so the two streams are also read as one, in the
/// order the harness wrote them. Fails if the log is printed after the result
/// line — a program's answer above the host call that produced it — or if the
/// call is printed twice, or not at all.
#[test]
fn a_stub_lets_the_module_run_and_its_call_is_printed_before_the_result() {
    let artifact = Artifact::assembled(CLOCK);
    let path = artifact.arg();
    let argv = [path.as_str(), "--host", "env.clock_ms=:I", "--invoke", "now"];

    let run = Run::of(&argv);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert_eq!(run.out, "now = 0\n", "{}", run.transcript());
    assert_eq!(run.err, "host call: env.clock_ms()\n", "{}", run.transcript());
    assert_eq!(
        Interleaved::of(&argv),
        (support::exit::OK, "host call: env.clock_ms()\nnow = 0\n".to_string()),
        "the host call is printed before the result it led to"
    );
}

/// A refused module that imports nothing gets no import list.
///
/// The control for the row above: without it, "names the import" would be
/// satisfied by a harness that prints the same three lines under every decode
/// failure, and a malformed module would be reported as though an embedder
/// could have rescued it. Fails if the emptiness check goes away, which would
/// leave a bare heading and a pointer to `--host` under a module that imports
/// nothing.
#[test]
fn a_refused_module_that_imports_nothing_names_no_import() {
    let artifact = Artifact::new(b"\0asm\x01\0\0\0extra");
    let run = Run::of(&[&artifact.arg()]);
    assert_eq!(run.code, support::exit::DECODE, "{}", run.transcript());
    assert!(!run.err.contains("it imports these"), "{}", run.transcript());
    assert!(!run.err.contains(HOST_POINTER), "{}", run.transcript());
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
// Host stubs
// ---------------------------------------------------------------------------

/// An [`ADDRESS`] in an expected log stands for one decimal and for nothing
/// else.
///
/// Fails if [`reads_as`] accepts a log whose address is missing or not a
/// number, or whose text around the address differs from the expected text —
/// any of which would let the F´ rows below pass over a log they do not
/// describe.
#[test]
fn an_address_in_an_expected_log_stands_for_one_decimal_only() {
    let expected = "host call: m.f(<address>, 6)\n";
    for log in ["host call: m.f(65504, 6)\n", "host call: m.f(0, 6)\n"] {
        assert!(reads_as(log, expected), "{log:?} must read as {expected:?}");
    }
    for log in [
        "host call: m.f(, 6)\n",
        "host call: m.f(x, 6)\n",
        "host call: m.f(65504x, 6)\n",
        "host call: m.f(65504, 7)\n",
        "host call: m.g(65504, 6)\n",
        "host call: m.f(65504, 6)\nhost call: m.f(65504, 6)\n",
        "host call: m.f(65504, 6)",
        "",
    ] {
        assert!(!reads_as(log, expected), "{log:?} must not read as {expected:?}");
    }
}

/// The F´ program, compiled from source, loads against a stub for each host of
/// the reference set it binds, and the log is the calls it made.
///
/// Every stub answers zero, so `telemetry` reads as accepted and `report` goes
/// on to ask the clock, returning the clock's zero. No answer here can be told
/// from another, which is why the host-imports tier, where the answers are
/// chosen per row, is where values are shown crossing the boundary. Each array
/// argument is logged as an address; the row pins that one is there, not its
/// value. Fails if a call is logged under the wrong name, with its arguments
/// reordered or rendered some other way, twice or not at all; if a stub's zero
/// stops reaching the program; or if a program built by this compiler stops
/// running under the harness.
#[test]
fn the_fprime_program_runs_against_stubs_that_answer_zero() {
    let artifact = Artifact::compiled(FPRIME);
    let path = artifact.arg();
    let mut argv = vec![path.as_str()];
    argv.extend(FPRIME_STUBS);
    argv.extend(["--invoke", "report", "3"]);

    let run = Run::of(&argv);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert_eq!(run.out, "report = 0\n", "{}", run.transcript());
    assert!(
        reads_as(
            &run.err,
            "host call: fprime_core.message(<address>, 6)\n\
             host call: fprime_core.command(1, 3)\n\
             host call: fprime_core.telemetry(0, <address>, 11, <address>, 4)\n\
             host call: env.clock_ms()\n"
        ),
        "{}",
        run.transcript()
    );
}

/// Every host of the F´ reference set is logged when a compiled program calls
/// all six, in the order it called them.
///
/// The committed program binds `rsleep` and `panic` and never calls them, so
/// the row above sees four of its six imports. This program makes every call
/// whatever the answers, so the other two — `rsleep`'s `i64` argument and
/// `panic`'s three — are observed from the command line too. A stub never
/// traps, so `panic` returns and the clock is asked after it. Fails if any of
/// the six is logged under the wrong name, out of order, with its arguments
/// rendered some other way, twice or not at all; if `command`'s zero stops
/// reaching `telemetry`; or if `go` stops returning the clock's zero.
#[test]
fn a_program_calling_every_fprime_import_logs_all_six_in_call_order() {
    let artifact = Artifact::compiled(FPRIME_EVERY_CALL);
    let path = artifact.arg();
    let mut argv = vec![path.as_str()];
    argv.extend(FPRIME_STUBS);
    argv.extend(["--invoke", "go"]);

    let run = Run::of(&argv);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert_eq!(run.out, "go = 0\n", "{}", run.transcript());
    assert!(
        reads_as(
            &run.err,
            "host call: fprime_core.message(<address>, 2)\n\
             host call: fprime_core.command(42, 7)\n\
             host call: fprime_core.telemetry(0, <address>, 11, <address>, 4)\n\
             host call: fprime_core.rsleep(5000000000)\n\
             host call: fprime_core.panic(<address>, 4, 17)\n\
             host call: env.clock_ms()\n"
        ),
        "{}",
        run.transcript()
    );
}

/// A spec that is not `MODULE.FIELD=PARAMS[:RESULT]` is a usage error naming
/// the form, and a signature the interpreter will not register is one naming
/// the interpreter's refusal and the cap behind it.
///
/// Each refusal is compared as a whole line, because the line is what tells a
/// reader which part of the spec to fix: a character that names no value type
/// is named, since a stray second `:` or one wrong letter is hard to see inside
/// the list it sits in. Every row is refused before the module file is read —
/// the stub is built under the session and before the load — so the fixture is
/// one a correct spec would load, and the last pairs a refused spec with a path
/// that does not exist. Fails if a malformed spec starts registering
/// something, if a refusal loses the spec, the form, the interpreter's own
/// error name, the character it refused or the number of results it was
/// given, if the usage line stops being printed, or if the stubs are built
/// after the file is read, which would answer the missing path with the
/// unreadable-module code.
///
/// The ten-letter result is refused as `ParameterListTooLong` because the
/// interpreter spells both halves of a signature with one list type; the row
/// holds the explanation to the cap on results, which is the one exceeded.
#[test]
fn a_malformed_host_spec_is_a_usage_error() {
    let artifact = Artifact::assembled(CLOCK);
    let form = "`--host` takes MODULE.FIELD=PARAMS[:RESULT], and";
    let letters = "each value type is one character, `i` for `i32`, `I` for `i64`, `f` for `f32` \
                   or `d` for `f64`";
    for (spec, expected) in [
        ("env.clock_ms", format!("{form} `env.clock_ms` has no `=`")),
        ("env=:I", format!("{form} `env=:I` has no `.` between a module and a field")),
        (".clock_ms=:I", format!("{form} `.clock_ms=:I` names no module")),
        ("env.=:I", format!("{form} `env.=:I` names no field")),
        (
            "env.clock_ms=I:",
            format!("{form} `env.clock_ms=I:` has a `:` and no result type after it"),
        ),
        (
            "env.clock_ms=x",
            format!(
                "the interpreter refuses the parameter types `x` of `--host env.clock_ms=x` \
                 (ValListInvalidItem): `x` names no value type; {letters}"
            ),
        ),
        (
            "env.clock_ms=:x",
            format!(
                "the interpreter refuses the result type `x` of `--host env.clock_ms=:x` \
                 (ValListInvalidItem): `x` names no value type; {letters}"
            ),
        ),
        (
            "env.clock_ms=iiL",
            format!(
                "the interpreter refuses the parameter types `iiL` of `--host env.clock_ms=iiL` \
                 (ValListInvalidItem): `L` names no value type; {letters}"
            ),
        ),
        (
            "env.clock_ms=i:i:I",
            format!(
                "the interpreter refuses the result type `i:I` of `--host env.clock_ms=i:i:I` \
                 (ValListInvalidItem): `:` names no value type, and a spec has at most one \
                 `:`; {letters}"
            ),
        ),
        (
            "env.clock_ms=:ii",
            "the interpreter refuses the result type `ii` of `--host env.clock_ms=:ii` \
             (MultiReturnNotAllowed): a host function returns at most one value, and this one \
             returns 2"
                .to_string(),
        ),
        (
            "env.clock_ms=:iiiiiiiiii",
            "the interpreter refuses the result type `iiiiiiiiii` of `--host \
             env.clock_ms=:iiiiiiiiii` (ParameterListTooLong): a host function returns at most \
             one value, and this one returns 10"
                .to_string(),
        ),
    ] {
        let run = Run::of(&[&artifact.arg(), "--host", spec, "--invoke", "now"]);
        assert_eq!(run.code, support::exit::USAGE, "`{spec}`: {}", run.transcript());
        assert_eq!(
            run.err.lines().next(),
            Some(format!("error: {expected}").as_str()),
            "`{spec}`: {}",
            run.transcript()
        );
        assert_usage_line(&run);
    }

    let bare = Run::of(&[&artifact.arg(), "--host", "--invoke", "now"]);
    assert_eq!(bare.code, support::exit::USAGE, "{}", bare.transcript());
    assert!(
        bare.err.contains("`--host` needs MODULE.FIELD=PARAMS[:RESULT]"),
        "{}",
        bare.transcript()
    );
    assert_usage_line(&bare);

    let dir = tempfile::TempDir::new().expect("a temporary directory");
    let missing = dir.path().join("nothing-here.wasm").display().to_string();
    let unread = Run::of(&[&missing, "--host", "env.clock_ms=x"]);
    assert_eq!(unread.code, support::exit::USAGE, "{}", unread.transcript());
    assert_eq!(
        unread.err.lines().next(),
        Some(
            format!(
                "error: the interpreter refuses the parameter types `x` of `--host \
                 env.clock_ms=x` (ValListInvalidItem): `x` names no value type; {letters}"
            )
            .as_str()
        ),
        "{}",
        unread.transcript()
    );
    assert_usage_line(&unread);
}

/// A name at the interpreter's 31-byte cap is registered, and a name one byte
/// over it is refused naming the cap, for the module and for the field alike.
///
/// Both sides, because a refusal alone is satisfied by a harness that refuses
/// every long name, and the accepting side is what shows the harness asks the
/// interpreter rather than a limit of its own. The 31 is spelled here rather
/// than read from the interpreter, so a cap that moved fails this row instead of
/// moving it. Fails if the two names stop being told apart in the refusal.
#[test]
fn host_names_are_held_to_the_interpreter_cap_from_both_sides() {
    let (module, field) = ("m".repeat(31), "f".repeat(31));
    let artifact =
        Artifact::assembled(&format!(r#"(module (import "{module}" "{field}" (func)))"#));
    let accepted = Run::of(&[&artifact.arg(), "--host", &format!("{module}.{field}=")]);
    assert_eq!(accepted.code, support::exit::OK, "{}", accepted.transcript());

    for (kind, spec, name) in [
        ("module", format!("{module}m.{field}="), format!("{module}m")),
        ("function", format!("{module}.{field}f="), format!("{field}f")),
    ] {
        let run = Run::of(&[&artifact.arg(), "--host", &spec]);
        assert_eq!(run.code, support::exit::USAGE, "{kind}: {}", run.transcript());
        assert_eq!(
            run.err.lines().next(),
            Some(
                format!(
                    "error: the interpreter refuses the {kind} name `{name}` of `--host {spec}` \
                     (HostNameError): a host {kind} name holds at most 31 bytes, and this one \
                     is 32"
                )
                .as_str()
            ),
            "{kind}: {}",
            run.transcript()
        );
        assert_usage_line(&run);
    }
}

/// Nine parameters are registered and ten are refused naming the cap and the
/// count given.
///
/// Fails if the harness starts counting parameters itself and disagrees with
/// the interpreter on either side, or if the refusal stops naming nine or the
/// ten it was given.
#[test]
fn host_arity_is_held_to_the_interpreter_cap_from_both_sides() {
    let artifact = Artifact::assembled(&format!(
        r#"(module (import "host" "call" (func (param {}))))"#,
        "i32 ".repeat(9)
    ));
    let accepted = Run::of(&[&artifact.arg(), "--host", "host.call=iiiiiiiii"]);
    assert_eq!(accepted.code, support::exit::OK, "{}", accepted.transcript());

    let refused = Run::of(&[&artifact.arg(), "--host", "host.call=iiiiiiiiii"]);
    assert_eq!(refused.code, support::exit::USAGE, "{}", refused.transcript());
    assert_eq!(
        refused.err.lines().next(),
        Some(
            "error: the interpreter refuses the parameter types `iiiiiiiiii` of `--host \
             host.call=iiiiiiiiii` (ParameterListTooLong): a host function takes at most 9 \
             parameters, and this one takes 10"
        ),
        "{}",
        refused.transcript()
    );
    assert_usage_line(&refused);
}

/// Two specs for one import are refused, whether they disagree or agree.
///
/// An import binds to exactly one host function, so a second spec could only be
/// ignored or shadow the first, and either would log calls under a stub the
/// reader did not mean. The disagreeing pairs are the hazard: the binder takes
/// the first host function of a matching name, so one spec would be silently
/// shadowed, and a check keyed on the whole spec rather than on its two names
/// would register both. They are run in both orders, so neither spec is the
/// one a check happens to see first. Fails if the check is dropped or keyed on
/// anything but the two names.
#[test]
fn one_import_named_twice_is_a_usage_error() {
    let artifact = Artifact::assembled(CLOCK);
    for (first, second) in [
        ("env.clock_ms=:I", "env.clock_ms=i:I"),
        ("env.clock_ms=i:I", "env.clock_ms=:I"),
        ("env.clock_ms=:I", "env.clock_ms=:I"),
    ] {
        let run = Run::of(&[&artifact.arg(), "--host", first, "--host", second]);
        assert_eq!(run.code, support::exit::USAGE, "{first} {second}: {}", run.transcript());
        assert_eq!(
            run.err.lines().next(),
            Some("error: `--host env.clock_ms` was given twice; an import is bound to one stub"),
            "{first} {second}: {}",
            run.transcript()
        );
        assert_usage_line(&run);
    }
}

/// Two specs naming one module load a module importing both, and each call is
/// logged under its own stub, in the order the program made them.
///
/// `both` passes `a`'s answer to `b`, so the second line carries the first
/// stub's zero. Fails if the second spec of a module is dropped or replaces the
/// first — the module would not load — or if the two stubs are confused. It
/// cannot see whether the two share one host module; the 257-stub row below is
/// the one that pins the grouping.
#[test]
fn two_specs_for_one_module_both_bind_and_log_under_their_own_names() {
    let artifact = Artifact::assembled(
        r#"(module
             (import "env" "a" (func $a (result i32)))
             (import "env" "b" (func $b (param i32) (result i32)))
             (func (export "both") (result i32) (call $b (call $a))))"#,
    );
    let run = Run::of(&[
        &artifact.arg(),
        "--host",
        "env.a=:i",
        "--host",
        "env.b=i:i",
        "--invoke",
        "both",
    ]);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert_eq!(run.out, "both = 0\n", "{}", run.transcript());
    assert_eq!(run.err, "host call: env.a()\nhost call: env.b(0)\n", "{}", run.transcript());
}

/// The stubs of one module name are one host module, which is what keeps the
/// 257th stub of a name bound to itself.
///
/// The interpreter's binder searches every host module carrying an import's
/// module name, so a harness registering one module per stub binds a two-stub
/// module exactly as this one does, and the row above cannot tell them apart.
/// The count is what differs: the store refers to a host module by a
/// `HostModuleRef`, which silently wraps an index it cannot hold, so the 257th
/// module would be addressed as the first and the call below would be logged
/// as `env.f0`. Fails if the stubs stop being grouped under their module name.
#[test]
fn stubs_of_one_module_stay_bound_past_the_host_module_count() {
    let imports: Vec<(String, String)> =
        (0..257).map(|index| ("env".to_string(), format!("f{index}"))).collect();
    let artifact = importing_each(&imports);
    let specs: Vec<String> = imports.iter().map(|(m, f)| format!("{m}.{f}=:i")).collect();
    let path = artifact.arg();
    let mut argv = with_hosts(&[path.as_str()], &specs);
    argv.extend(["--invoke", "last"]);

    let run = Run::of(&argv);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert_eq!(run.out, "last = 0\n", "{}", run.transcript());
    assert_eq!(run.err, "host call: env.f256()\n", "{}", run.transcript());
}

/// As many module names as a `HostModuleRef` addresses are registered, and one
/// more is refused rather than bound to another module's stubs.
///
/// Both sides of the boundary: 256 names load and the last one's stub is the
/// one called, and 257 are a usage error giving both the count asked for and
/// the count the interpreter addresses, the second read off its own
/// `HostModuleRef` constructor. Fails if the check moves by one either way, or
/// goes, which would log the 257th module's calls under the first module's
/// name, or if the refusal stops naming the limit.
#[test]
fn more_module_names_than_the_interpreter_addresses_are_refused() {
    let addressable = one_field_from_each_module(256);
    let artifact = importing_each(&addressable);
    let specs: Vec<String> = addressable.iter().map(|(m, f)| format!("{m}.{f}=:i")).collect();
    let path = artifact.arg();
    let mut argv = with_hosts(&[path.as_str()], &specs);
    argv.extend(["--invoke", "last"]);
    let run = Run::of(&argv);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert_eq!(run.err, "host call: m255.f()\n", "{}", run.transcript());

    let one_more = one_field_from_each_module(257);
    let artifact = importing_each(&one_more);
    let specs: Vec<String> = one_more.iter().map(|(m, f)| format!("{m}.{f}=:i")).collect();
    let path = artifact.arg();
    let run = Run::of(&with_hosts(&[path.as_str()], &specs));
    assert_eq!(run.code, support::exit::USAGE, "{}", run.transcript());
    assert_eq!(
        run.err.lines().next(),
        Some(
            "error: `--host` registers stubs under 257 module names, and the interpreter \
             addresses at most 256 host modules: the imports of the rest would be bound to \
             another module's stubs"
        ),
        "{}",
        run.transcript()
    );
    assert_usage_line(&run);
}

/// The specs of one module name are one host module wherever they stand on the
/// command line, and not only when they stand together.
///
/// Every other row gives a module's specs side by side, and for those,
/// grouping only neighbouring specs registers the same host set; below the
/// count the binder searches every host module carrying an import's module
/// name, so a module split in two binds either way. Here `m0.g` is given
/// last, with 255 other module names between it and `m0.f`: 256 names load
/// and `m0.g` is the stub called, where a second `m0` module would push the
/// set one past the count and be refused, and 257 names split the same way
/// are refused as 257 rather than 258. Fails if the grouping stops looking
/// further back than the spec before.
#[test]
fn the_specs_of_one_module_name_are_grouped_wherever_they_stand() {
    let split = |count: usize| -> Vec<(String, String)> {
        let mut imports = one_field_from_each_module(count);
        imports.push(("m0".to_string(), "g".to_string()));
        imports
    };

    let addressable = split(256);
    let artifact = importing_each(&addressable);
    let specs: Vec<String> = addressable.iter().map(|(m, f)| format!("{m}.{f}=:i")).collect();
    let path = artifact.arg();
    let mut argv = with_hosts(&[path.as_str()], &specs);
    argv.extend(["--invoke", "last"]);
    let run = Run::of(&argv);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert_eq!(run.out, "last = 0\n", "{}", run.transcript());
    assert_eq!(run.err, "host call: m0.g()\n", "{}", run.transcript());

    let one_more = split(257);
    let artifact = importing_each(&one_more);
    let specs: Vec<String> = one_more.iter().map(|(m, f)| format!("{m}.{f}=:i")).collect();
    let path = artifact.arg();
    let run = Run::of(&with_hosts(&[path.as_str()], &specs));
    assert_eq!(run.code, support::exit::USAGE, "{}", run.transcript());
    assert_eq!(
        run.err.lines().next(),
        Some(
            "error: `--host` registers stubs under 257 module names, and the interpreter \
             addresses at most 256 host modules: the imports of the rest would be bound to \
             another module's stubs"
        ),
        "{}",
        run.transcript()
    );
    assert_usage_line(&run);
}

/// As many stubs of one module name as the interpreter addresses in one host
/// module are registered, and one more is refused rather than bound to the
/// first.
///
/// Grouping the stubs by module name is what keeps the host-module count
/// small, and it moves the same wrap inside the module: the binder records a
/// host function's position as sixteen bits, so the 65,537th stub of one name
/// would be bound to the first and its calls logged as `env.f0`. The module
/// imports only the last stub, which is what a silent wrap would mis-bind.
/// Both sides, so the bound cannot move by one unnoticed. Fails if the check
/// goes, moves, or stops naming the two counts.
#[test]
fn more_stubs_of_one_module_than_the_interpreter_addresses_are_refused() {
    let only_the_last = |count: usize| -> (Artifact, Vec<String>) {
        let last = ("env".to_string(), format!("f{}", count - 1));
        let specs = (0..count).map(|index| format!("env.f{index}=:i")).collect();
        (importing_each(&[last]), specs)
    };

    let (artifact, specs) = only_the_last(65_536);
    let path = artifact.arg();
    let mut argv = with_hosts(&[path.as_str()], &specs);
    argv.extend(["--invoke", "last"]);
    let run = Run::of(&argv);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert_eq!(run.err, "host call: env.f65535()\n", "{}", run.transcript());

    let (artifact, specs) = only_the_last(65_537);
    let path = artifact.arg();
    let mut argv = with_hosts(&[path.as_str()], &specs);
    argv.extend(["--invoke", "last"]);
    let run = Run::of(&argv);
    assert_eq!(run.code, support::exit::USAGE, "{}", run.transcript());
    assert_eq!(
        run.err.lines().next(),
        Some(
            "error: `--host` registers 65537 stubs under `env`, and the interpreter addresses \
             at most 65536 functions of one host module: an import of any stub past that many \
             would be bound to one of the first"
        ),
        "{}",
        run.transcript()
    );
    assert_usage_line(&run);
}

/// Suggestions `--host` registers every one of alone, and no command line can
/// register together, end on the reason and point at an embedder.
///
/// A module importing a function from each of 257 module names is suggested a
/// spec for every import, and pasting all 257 would be refused for the
/// host-module count, so the report says so itself, in the words `--host`
/// would refuse them in, and does not point at `--host`. 256 names are the
/// control and close on the pointer at `--host`, since every suggestion there
/// can be registered at once. Fails if the suggestions stop being judged as a
/// set, or are judged against a bound off by one.
#[test]
fn suggestions_no_command_line_can_register_together_point_at_an_embedder() {
    let artifact = importing_each(&one_field_from_each_module(256));
    let run = Run::of(&[&artifact.arg()]);
    assert_eq!(run.code, support::exit::DECODE, "{}", run.transcript());
    assert!(!run.err.contains("no `--host` set can supply them all"), "{}", run.transcript());
    assert_eq!(run.err.lines().last(), Some(HOST_POINTER), "{}", run.transcript());

    let artifact = importing_each(&one_field_from_each_module(257));
    let run = Run::of(&[&artifact.arg()]);
    assert_eq!(run.code, support::exit::DECODE, "{}", run.transcript());
    assert!(
        run.err.contains(
            "    m256.f: no stub; it needs `--host m256.f=:i`\n  no `--host` set can supply them \
             all: `--host` registers stubs under 257 module names, and the interpreter addresses \
             at most 256 host modules: the imports of the rest would be bound to another \
             module's stubs\n"
        ),
        "{}",
        run.transcript()
    );
    assert_eq!(run.err.lines().last(), Some(EMBEDDER_POINTER), "{}", run.transcript());
}

/// `--host` is read wherever it stands: before `--invoke`, and after the
/// export's argument list.
///
/// A token starting with `--` ends `--invoke`'s arguments, which is the rule
/// that lets a spec follow them. Fails if `--host` after the list is swallowed
/// as an argument — the export would be given two and refuse the count — or if
/// the option is read only in one position.
#[test]
fn host_is_read_before_invoke_and_after_its_arguments() {
    let artifact = Artifact::assembled(
        r#"(module
             (import "env" "clock_ms" (func $clock (result i64)))
             (func (export "later") (param i64) (result i64)
               (i64.add (local.get 0) (call $clock))))"#,
    );
    let path = artifact.arg();
    for argv in [
        [path.as_str(), "--host", "env.clock_ms=:I", "--invoke", "later", "5"],
        [path.as_str(), "--invoke", "later", "5", "--host", "env.clock_ms=:I"],
    ] {
        let run = Run::of(&argv);
        assert_eq!(run.code, support::exit::OK, "{argv:?}: {}", run.transcript());
        assert_eq!(run.out, "later = 5\n", "{argv:?}: {}", run.transcript());
        assert_eq!(run.err, "host call: env.clock_ms()\n", "{argv:?}: {}", run.transcript());
    }
}

/// A host module named `main` binds like any other, in a program this compiler
/// built from `use { f } from host::main;`.
///
/// The interpreter refuses a module whose name a host module already carries,
/// as `DuplicateModuleName` and before it reads a section, and exempts only
/// the empty name — so the harness decodes the artifact under that name. The
/// report is read first, and suggests the spec as it does for any module
/// name; the spec is then given, and the program runs and its call is logged.
/// Fails if the artifact is given a name again: a guest called `main` would be
/// refused at byte 8 the moment `--host main.f=i:i` registered its module,
/// a verdict about a name the artifact never carried.
#[test]
fn a_host_module_named_main_binds_like_any_other() {
    let artifact = Artifact::compiled(
        "external fn f(v: i32) -> i32;\n\
         use { f } from host::main;\n\
         pub fn go() -> i32 { return f(7); }",
    );
    let refused = Run::of(&[&artifact.arg(), "--invoke", "go"]);
    assert_eq!(refused.code, support::exit::DECODE, "{}", refused.transcript());
    assert!(
        refused.err.contains("    main.f: no stub; it needs `--host main.f=i:i`\n"),
        "{}",
        refused.transcript()
    );
    assert_eq!(refused.err.lines().last(), Some(HOST_POINTER), "{}", refused.transcript());

    let run = Run::of(&[&artifact.arg(), "--host", "main.f=i:i", "--invoke", "go"]);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert_eq!(run.out, "go = 0\n", "{}", run.transcript());
    assert_eq!(run.err, "host call: main.f(7)\n", "{}", run.transcript());
}

/// A stub registered under the import's names with another signature is a
/// decode failure whose report names the stub that did not match, beside the
/// one that did.
///
/// The interpreter's verdict is `FunctionImportTypeMismatch`, which names no
/// import, so the report compares each stub with the import it was registered
/// for. Fails if the mismatch starts binding, if its verdict is reported as a
/// missing import, if the report stops saying which stub is wrong and what the
/// import needs instead, or if a matching stub is reported as wrong beside it.
#[test]
fn a_stub_with_the_wrong_signature_is_named_in_the_report() {
    let artifact = Artifact::assembled(
        r#"(module
             (import "env" "a" (func (result i32)))
             (import "env" "clock_ms" (func (result i64))))"#,
    );
    let run = Run::of(&[
        &artifact.arg(),
        "--host",
        "env.a=:i",
        "--host",
        "env.clock_ms=i:I",
    ]);
    assert_eq!(run.code, support::exit::DECODE, "{}", run.transcript());
    assert!(
        run.err.contains(
            "does not load under the SpaceWasm interpreter: FunctionImportTypeMismatch"
        ),
        "{}",
        run.transcript()
    );
    assert!(
        run.err.contains(
            "  it imports these; beside each, what `--host` registered under its two names:\n    \
             env.a: stub registered\n    env.clock_ms: the stub `--host env.clock_ms=i:I` takes \
             1 parameter where the import takes none; it needs `--host env.clock_ms=:I`\n"
        ),
        "{}",
        run.transcript()
    );
    assert!(run.err.contains(HOST_POINTER), "{}", run.transcript());
}

/// A mismatching stub is described by what it gets wrong, so the wrong letter
/// is read off the report rather than found by comparing two specs by eye.
///
/// One row per shape a signature can be wrong in: one parameter of the wrong
/// type, named by its position — the ninth, where comparing by eye is hardest
/// — too few parameters, the wrong result type, a result where the import
/// returns nothing, and both halves wrong at once. Several parameters of the
/// wrong type are every one named, two and then three of one pair of types,
/// and two pairs in one list, since a clause naming only the first would send
/// a reader who fixed it to a second report. Fails if a half that differs goes
/// unmentioned, if one that matches is blamed, if a position or either type is
/// misreported or left out, or if the spec the import needs stops closing the
/// line, which is what a reader copies.
#[test]
fn a_mismatch_names_the_half_and_the_position_that_differ() {
    for (import, stub, difference, needed) in [
        (
            "(param i32 i32 i32 i32 i32 i32 i32 i32 i32)",
            "iiiiiiiiI",
            "takes `i64` as its ninth parameter where the import takes `i32`",
            "iiiiiiiii",
        ),
        (
            "(param i32 i64) (result i32)",
            "iI:I",
            "returns `i64` where the import returns `i32`",
            "iI:i",
        ),
        (
            "(param i32 i32) (result i32)",
            "iI:i",
            "takes `i64` as its second parameter where the import takes `i32`",
            "ii:i",
        ),
        ("(param i32 i32)", "i", "takes 1 parameter where the import takes 2 parameters", "ii"),
        ("", ":f", "returns `f32` where the import returns nothing", ""),
        (
            "(param f64) (result i64)",
            ":i",
            "takes none where the import takes 1 parameter, and returns `i32` where the import \
             returns `i64`",
            "d:I",
        ),
        (
            "(param i32 i32 i32)",
            "IIi",
            "takes `i64` as its first and second parameters where the import takes `i32`",
            "iii",
        ),
        (
            "(param i32 i32 i32)",
            "III",
            "takes `i64` as its first, second and third parameters where the import takes `i32`",
            "iii",
        ),
        (
            "(param i32 i32 i32 i32)",
            "IfIi",
            "takes `i64` as its first and third parameters where the import takes `i32`, and \
             takes `f32` as its second parameter where the import takes `i32`",
            "iiii",
        ),
    ] {
        let artifact =
            Artifact::assembled(&format!(r#"(module (import "host" "f" (func {import})))"#));
        let run = Run::of(&[&artifact.arg(), "--host", &format!("host.f={stub}")]);
        assert_eq!(run.code, support::exit::DECODE, "{stub}: {}", run.transcript());
        assert!(
            run.err.contains(&format!(
                "    host.f: the stub `--host host.f={stub}` {difference}; it needs `--host \
                 host.f={needed}`\n"
            )),
            "{stub}: {}",
            run.transcript()
        );
    }
}

/// The spec a report says an import needs is one that binds it, for every
/// letter of the alphabet, and each argument type is logged as its value.
///
/// The report spells an import's type in `--host`'s alphabet itself, so a
/// letter it maps wrongly would send the reader to a spec that is refused as a
/// mismatch. The import takes one of each of three types and returns the
/// fourth; the spec is lifted from the refusal and fed back. Fails if a letter
/// is mapped to the wrong type, if the spec stops round-tripping, if a stub's
/// zero stops matching its declared result type, or if an argument is logged in
/// anything but the harness's own rendering.
#[test]
fn the_spec_a_report_names_is_one_that_binds() {
    let artifact = Artifact::assembled(
        r#"(module
             (import "env" "mix" (func $mix (param i32 i64 f32) (result f64)))
             (func (export "go") (result f64)
               (call $mix (i32.const -1) (i64.const 9000000000) (f32.const 0.5))))"#,
    );
    let refused = Run::of(&[&artifact.arg()]);
    assert_eq!(refused.code, support::exit::DECODE, "{}", refused.transcript());
    let spec = refused
        .err
        .split("it needs `--host ")
        .nth(1)
        .and_then(|rest| rest.split('`').next())
        .unwrap_or_else(|| panic!("the report names a spec: {}", refused.transcript()));
    assert_eq!(spec, "env.mix=iIf:d", "{}", refused.transcript());

    let run = Run::of(&[&artifact.arg(), "--host", spec, "--invoke", "go"]);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert_eq!(run.out, "go = 0\n", "{}", run.transcript());
    assert_eq!(run.err, "host call: env.mix(-1, 9000000000, 0.5)\n", "{}", run.transcript());
}

/// A module whose every import has a matching stub, refused for something
/// else, is not told to register stubs.
///
/// The body returns an `i64` where it declares an `i32`, so the verdict comes
/// from the code section with the import already bound. Fails if the pointer to
/// `--host` or to an embedder is printed whenever a module imports anything,
/// which would send a reader to fix the stubs of a module whose defect is in
/// its code.
#[test]
fn a_module_refused_despite_its_stubs_is_not_told_to_register_them() {
    let artifact = Artifact::assembled(
        r#"(module
             (import "env" "f" (func))
             (func (export "g") (result i32) (i64.const 0)))"#,
    );
    let run = Run::of(&[&artifact.arg(), "--host", "env.f="]);
    assert_eq!(run.code, support::exit::DECODE, "{}", run.transcript());
    assert_eq!(run.err.lines().last(), Some("    env.f: stub registered"), "{}", run.transcript());
}

/// A module refused before its import section is reached is not told a stub
/// would load it.
///
/// The type section declares a `v128` function type no import uses, which
/// the interpreter refuses as `MalformedValueType` inside that section, before
/// it binds anything; the import it goes on to declare is still listed with
/// the spec it needs, since that is a fact about the import. With that spec
/// given, the verdict is the same and the import reads as supplied. Fails if
/// the report ends on the pointer at `--host` for a module no stub changes,
/// which a reader who pasted the spec would find answered by the same verdict
/// and no closing line at all.
#[test]
fn a_module_refused_before_its_imports_is_not_told_a_stub_would_help() {
    let artifact = Artifact::assembled(
        r#"(module
             (type (func (param v128)))
             (import "env" "f" (func)))"#,
    );
    let bare = Run::of(&[&artifact.arg()]);
    assert_eq!(bare.code, support::exit::DECODE, "{}", bare.transcript());
    assert!(
        bare.err.contains(
            "does not load under the SpaceWasm interpreter: MalformedValueType(123) at byte "
        ),
        "{}",
        bare.transcript()
    );
    assert!(bare.err.contains("\n  reading the Type section\n"), "{}", bare.transcript());
    assert!(
        bare.err.contains("    env.f: no stub; it needs `--host env.f=`\n"),
        "{}",
        bare.transcript()
    );
    assert_eq!(bare.err.lines().last(), Some(REFUSED_BEFORE_IMPORTS), "{}", bare.transcript());

    let stubbed = Run::of(&[&artifact.arg(), "--host", "env.f="]);
    assert_eq!(stubbed.code, support::exit::DECODE, "{}", stubbed.transcript());
    assert!(stubbed.err.contains("MalformedValueType(123)"), "{}", stubbed.transcript());
    assert_eq!(
        stubbed.err.lines().last(),
        Some("    env.f: stub registered"),
        "{}",
        stubbed.transcript()
    );
}

/// An import no spec can supply is listed with the reason, and the report then
/// points at an embedder alone.
///
/// One row per kind of import that is not a function, since `--host` registers
/// functions only and an embedder's host module can carry all three, which
/// the report names apart. Fails if any of them is given a spec, if a reason
/// is dropped or misattributed, a kind named as another, or if the report
/// still tells the reader to register a stub the line above says cannot
/// exist.
#[test]
fn an_import_no_spec_can_supply_is_listed_with_the_reason() {
    for (text, expected) in [
        (
            r#"(module (import "env" "g" (global i32)))"#,
            "    env.g: no `--host` stub can bind it: it is a global import, and `--host` \
             registers functions only\n",
        ),
        (
            r#"(module (import "env" "m" (memory 1)))"#,
            "    env.m: no `--host` stub can bind it: it is a memory import, and `--host` \
             registers functions only\n",
        ),
        (
            r#"(module (import "env" "t" (table 1 funcref)))"#,
            "    env.t: no `--host` stub can bind it: it is a table import, and `--host` \
             registers functions only\n",
        ),
    ] {
        let artifact = Artifact::assembled(text);
        let run = Run::of(&[&artifact.arg()]);
        assert_eq!(run.code, support::exit::DECODE, "{text}: {}", run.transcript());
        assert!(run.err.contains(expected), "{text}: {}", run.transcript());
        assert!(!run.err.contains("it needs `--host"), "{text}: {}", run.transcript());
        assert!(!run.err.contains(HOST_POINTER), "{text}: {}", run.transcript());
        assert_eq!(run.err.lines().last(), Some(EMBEDDER_POINTER), "{text}: {}", run.transcript());
    }
}

/// An import the interpreter cannot load however it is supplied is listed with
/// the reason, and the report ends on that rather than pointing anywhere.
///
/// One row per reason: a value type outside the interpreter's four, as a
/// parameter and as a result; a tag import; and a function import whose type
/// cannot be read, here one of type 0 in a module with no type section at all,
/// which no text assembler writes. `--host`'s four letters are the
/// interpreter's whole value-type set and its import reader knows no tag, so
/// neither a stub nor an embedder is a remedy. Each row pins the interpreter's
/// own verdict as well, since that is the premise: a refusal of the bytes
/// themselves, reached before any host is consulted. The last case puts such
/// an import beside one a stub can supply and one only an embedder can, which
/// is what shows it outranks both pointers. Fails if any of them is blamed on
/// `--host` or given a spec, if the report points at a stub or an embedder for
/// a module no embedder can load, or if the interpreter starts accepting one
/// of these shapes, which is the day the kind needs reading again.
#[test]
fn an_import_the_interpreter_cannot_load_ends_the_report_on_that() {
    for (artifact, verdict, expected) in [
        (
            Artifact::assembled(r#"(module (import "env" "r" (func (param externref))))"#),
            "MalformedValueType(111)",
            "    env.r: no `--host` stub can bind it: its type holds `externref`, a value type \
             the SpaceWasm interpreter does not have\n",
        ),
        (
            Artifact::assembled(r#"(module (import "env" "v" (func (result v128))))"#),
            "MalformedValueType(123)",
            "    env.v: no `--host` stub can bind it: its type holds `v128`, a value type the \
             SpaceWasm interpreter does not have\n",
        ),
        (
            Artifact::assembled(r#"(module (import "env" "t" (tag)))"#),
            "MalformedImportExportDesc(4)",
            "    env.t: no `--host` stub can bind it: it is a tag import, which the SpaceWasm \
             interpreter does not support\n",
        ),
        (
            Artifact::new(b"\0asm\x01\0\0\0\x02\x09\x01\x03env\x01f\x00\x00"),
            "TypeIdxOutOfRange",
            "    env.f: no `--host` stub can bind it: its function type cannot be read\n",
        ),
    ] {
        let run = Run::of(&[&artifact.arg()]);
        assert_eq!(run.code, support::exit::DECODE, "{expected}: {}", run.transcript());
        assert!(
            run.err.contains(&format!(
                "does not load under the SpaceWasm interpreter: {verdict} at byte "
            )),
            "{expected}: {}",
            run.transcript()
        );
        assert!(run.err.contains(expected), "{expected}: {}", run.transcript());
        assert!(!run.err.contains("it needs `--host"), "{expected}: {}", run.transcript());
        assert_eq!(run.err.lines().last(), Some(NO_EMBEDDER), "{expected}: {}", run.transcript());
    }

    let beside_both = Artifact::assembled(
        r#"(module
             (import "env" "f" (func))
             (import "env" "g" (global i32))
             (import "env" "t" (tag)))"#,
    );
    let run = Run::of(&[&beside_both.arg()]);
    assert_eq!(run.code, support::exit::DECODE, "{}", run.transcript());
    assert!(
        run.err.contains(
            "    env.f: no stub; it needs `--host env.f=`\n    env.g: no `--host` stub can bind \
             it: it is a global import, and `--host` registers functions only\n    env.t: no \
             `--host` stub can bind it: it is a tag import, which the SpaceWasm interpreter \
             does not support\n"
        ),
        "{}",
        run.transcript()
    );
    assert_eq!(run.err.lines().last(), Some(NO_EMBEDDER), "{}", run.transcript());
}

/// An import whose host function the interpreter refuses to build is one no
/// embedder can supply, and the report ends on that.
///
/// The interpreter decodes imports it can never bind: a name one byte past a
/// host name's cap, for the module and for the field, and a tenth parameter.
/// The constructors `--host` calls refuse the function each would need, and
/// every embedder builds a host function through the same constructors, whose
/// types hold no more — so the refusal `--host` would print for the spec is
/// quoted, with the spec's own length or count beside the cap, and the report
/// ends on no embedder rather than pointing at one. Two more rows reach the
/// same line from a verdict the decoder gives itself: a 33-byte field, past
/// its own limit and refused as `VecTooLong` inside the import section, and a
/// second result, refused as `FunctionReturnsTooLarge` in the type section —
/// which also shows an import no embedder can supply outranking the line for
/// a module refused before its imports. Each row pins the verdict and the
/// section it came from. Fails if any of them is given a spec, which would be
/// refused the moment the reader pasted it, if the quoted refusal loses its
/// length or count, or if the report points at an embedder that could not
/// register the function either.
#[test]
fn an_import_the_host_function_api_refuses_is_one_no_embedder_can_supply() {
    let (module_32, field_32, field_33) = ("m".repeat(32), "f".repeat(32), "f".repeat(33));
    for (text, verdict, section, expected) in [
        (
            format!(r#"(module (import "host" "call" (func (param {}))))"#, "i32 ".repeat(10)),
            "FunctionImportNotFound",
            "Import",
            "    host.call: no `--host` stub can bind it: the interpreter refuses the parameter \
             types `iiiiiiiiii` of `--host host.call=iiiiiiiiii` (ParameterListTooLong): a host \
             function takes at most 9 parameters, and this one takes 10\n"
                .to_string(),
        ),
        (
            r#"(module (import "env" "pair" (func (result i32 i32))))"#.to_string(),
            "FunctionReturnsTooLarge",
            "Type",
            "    env.pair: no `--host` stub can bind it: the interpreter refuses the result type \
             `ii` of `--host env.pair=:ii` (MultiReturnNotAllowed): a host function returns at \
             most one value, and this one returns 2\n"
                .to_string(),
        ),
        (
            format!(r#"(module (import "env" "{field_32}" (func)))"#),
            "FunctionImportNotFound",
            "Import",
            format!(
                "    env.{field_32}: no `--host` stub can bind it: the interpreter refuses the \
                 function name `{field_32}` of `--host env.{field_32}=` (HostNameError): a host \
                 function name holds at most 31 bytes, and this one is 32\n"
            ),
        ),
        (
            format!(r#"(module (import "{module_32}" "f" (func)))"#),
            "FunctionImportNotFound",
            "Import",
            format!(
                "    {module_32}.f: no `--host` stub can bind it: the interpreter refuses the \
                 module name `{module_32}` of `--host {module_32}.f=` (HostNameError): a host \
                 module name holds at most 31 bytes, and this one is 32\n"
            ),
        ),
        (
            format!(r#"(module (import "env" "{field_33}" (func)))"#),
            "VecTooLong",
            "Import",
            format!(
                "    env.{field_33}: no `--host` stub can bind it: the interpreter refuses the \
                 function name `{field_33}` of `--host env.{field_33}=` (HostNameError): a host \
                 function name holds at most 31 bytes, and this one is 33\n"
            ),
        ),
    ] {
        let artifact = Artifact::assembled(&text);
        let run = Run::of(&[&artifact.arg()]);
        assert_eq!(run.code, support::exit::DECODE, "{text}: {}", run.transcript());
        assert!(
            run.err.contains(&format!(
                "does not load under the SpaceWasm interpreter: {verdict} at byte "
            )),
            "{text}: {}",
            run.transcript()
        );
        assert!(
            run.err.contains(&format!("\n  reading the {section} section\n")),
            "{text}: {}",
            run.transcript()
        );
        assert!(run.err.contains(&expected), "{text}: {}", run.transcript());
        assert!(!run.err.contains("it needs `--host"), "{text}: {}", run.transcript());
        assert_eq!(run.err.lines().last(), Some(NO_EMBEDDER), "{text}: {}", run.transcript());
    }
}

/// One name imported with two types is an import nothing can supply, and no
/// second spec is suggested for it.
///
/// WebAssembly allows the pair, and the interpreter binds a name to the first
/// function registered under it, so the second import is a mismatch whatever
/// supplies it — and a second spec for the same two names is one `--host`
/// refuses as given twice. With the first import's stub registered, the report
/// says that stub cannot bind the second. The same name imported twice with
/// one type is the control: one stub binds both, and it loads. Fails if the
/// report suggests `--host m.f=i`, which the reader would paste beside
/// `--host m.f=` and be refused, if the first import stops being suggested its
/// own spec, if the report points at a stub or an embedder for a module
/// nothing can load, or if a repeated name of one type is refused as well.
#[test]
fn one_name_imported_with_two_types_is_one_nothing_can_supply() {
    let artifact = Artifact::assembled(
        r#"(module
             (import "m" "f" (func))
             (import "m" "f" (func (param i32))))"#,
    );
    let rebound = "an earlier import has the same two names and another type, and the \
                   interpreter binds one name to one function";

    let bare = Run::of(&[&artifact.arg()]);
    assert_eq!(bare.code, support::exit::DECODE, "{}", bare.transcript());
    assert!(
        bare.err.contains(&format!(
            "    m.f: no stub; it needs `--host m.f=`\n    m.f: no `--host` stub can bind it: \
             {rebound}\n"
        )),
        "{}",
        bare.transcript()
    );
    assert!(!bare.err.contains("`--host m.f=i`"), "{}", bare.transcript());
    assert_eq!(bare.err.lines().last(), Some(NO_EMBEDDER), "{}", bare.transcript());

    let stubbed = Run::of(&[&artifact.arg(), "--host", "m.f="]);
    assert_eq!(stubbed.code, support::exit::DECODE, "{}", stubbed.transcript());
    assert!(
        stubbed.err.contains(
            "does not load under the SpaceWasm interpreter: FunctionImportTypeMismatch"
        ),
        "{}",
        stubbed.transcript()
    );
    assert!(
        stubbed.err.contains(&format!(
            "    m.f: stub registered\n    m.f: the stub `--host m.f=` cannot bind it: \
             {rebound}\n"
        )),
        "{}",
        stubbed.transcript()
    );
    assert_eq!(stubbed.err.lines().last(), Some(NO_EMBEDDER), "{}", stubbed.transcript());

    let one_type = Artifact::assembled(
        r#"(module
             (import "m" "f" (func))
             (import "m" "f" (func)))"#,
    );
    let refused = Run::of(&[&one_type.arg()]);
    assert_eq!(refused.code, support::exit::DECODE, "{}", refused.transcript());
    assert!(
        refused.err.contains(
            "    m.f: no stub; it needs `--host m.f=`\n    m.f: no stub; it needs `--host m.f=`\n"
        ),
        "{}",
        refused.transcript()
    );
    assert_eq!(refused.err.lines().last(), Some(HOST_POINTER), "{}", refused.transcript());
    let loaded = Run::of(&[&one_type.arg(), "--host", "m.f="]);
    assert_eq!(loaded.code, support::exit::OK, "{}", loaded.transcript());
}

/// Imports of which a stub can supply some and only an embedder the rest are
/// not told that stubs are enough.
///
/// Registering the function's stub would leave the global unsupplied, and a
/// reader told to register it would meet a second refusal before the pointer
/// they needed. With the stub registered, the global is all that is left and
/// the report points at an embedder alone. Fails if the report closes on the
/// pointer at `--host` whenever some import has a spec, or on the bare pointer
/// at an embedder, which would leave the line suggesting a stub unexplained.
#[test]
fn stubs_for_some_imports_are_not_offered_as_enough_for_all() {
    let artifact = Artifact::assembled(
        r#"(module
             (import "env" "f" (func))
             (import "env" "g" (global i32)))"#,
    );
    let global = "    env.g: no `--host` stub can bind it: it is a global import, and `--host` \
                  registers functions only\n";

    let run = Run::of(&[&artifact.arg()]);
    assert_eq!(run.code, support::exit::DECODE, "{}", run.transcript());
    assert!(
        run.err.contains(&format!("    env.f: no stub; it needs `--host env.f=`\n{global}")),
        "{}",
        run.transcript()
    );
    assert_eq!(run.err.lines().last(), Some(SOME_STUBS_POINTER), "{}", run.transcript());

    let stubbed = Run::of(&[&artifact.arg(), "--host", "env.f="]);
    assert_eq!(stubbed.code, support::exit::DECODE, "{}", stubbed.transcript());
    assert!(
        stubbed.err.contains(&format!("    env.f: stub registered\n{global}")),
        "{}",
        stubbed.transcript()
    );
    assert_eq!(stubbed.err.lines().last(), Some(EMBEDDER_POINTER), "{}", stubbed.transcript());
}

/// A spec's name splits at its last dot, so a module name may contain a dot and
/// a field name may not — and an import whose field does is one the report
/// names no spec for, with its names printed apart and the reason the obvious
/// spelling fails.
///
/// `a.b.f` registers `f` under the module `a.b`. The import `b.c` of module `a`
/// would be spelled `a.b.c` too, which reads back as `c` under `a.b`: a stub
/// for other names. Printed joined, the import itself would read as that other
/// import, and the reader's obvious retry, `--host a.b.c=`, would meet the
/// same line; so the line names the two names apart and says what that spec
/// registers instead. The retry is run too, and it is listed as naming no
/// import. Fails if the split moves to the first dot, which would stop a
/// dotted module name loading; if the report stops reading its suggestion back
/// into the two names it is for, which would print a spec that registers a
/// stub the import never binds; or if the names are joined again.
#[test]
fn a_module_name_may_contain_a_dot_and_a_field_name_may_not() {
    let dotted_module = Artifact::assembled(r#"(module (import "a.b" "f" (func)))"#);
    let run = Run::of(&[&dotted_module.arg(), "--host", "a.b.f="]);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());

    let dotted_field = Artifact::assembled(r#"(module (import "a" "b.c" (func)))"#);
    let unwritable = "    module `a`, field `b.c`: no `--host` spec names it: `--host a.b.c=` \
                      would register the field `c` of the module `a.b`\n";
    let run = Run::of(&[&dotted_field.arg()]);
    assert_eq!(run.code, support::exit::DECODE, "{}", run.transcript());
    assert!(run.err.contains(unwritable), "{}", run.transcript());
    assert!(!run.err.contains("it needs `--host"), "{}", run.transcript());
    assert_eq!(run.err.lines().last(), Some(EMBEDDER_POINTER), "{}", run.transcript());

    let retry = Run::of(&[&dotted_field.arg(), "--host", "a.b.c="]);
    assert_eq!(retry.code, support::exit::DECODE, "{}", retry.transcript());
    assert!(
        retry.err.contains(&format!(
            "{unwritable}    `--host a.b.c=` names no import of this module\n"
        )),
        "{}",
        retry.transcript()
    );
}

/// An import with an `=` in either name is one no spec names, and the report
/// says the `=` is why.
///
/// A spec splits at its first `=`, so an `=` ends whichever name holds it.
/// Spelled regardless, the module `a=b` would read back as a spec with no dot
/// in its name, and the field `f=g` as the field `f` — either way a reason
/// about names the import does not carry. Fails if either check goes, which
/// brings back that reason, if the names are printed joined, or if a spec is
/// suggested.
#[test]
fn an_equals_sign_in_either_name_is_the_reason_no_spec_names_it() {
    for (text, expected) in [
        (
            r#"(module (import "a=b" "f" (func)))"#,
            "    module `a=b`, field `f`: no `--host` spec names it: its module name `a=b` \
             contains `=`, which ends a name in `--host`'s spelling\n",
        ),
        (
            r#"(module (import "a" "f=g" (func)))"#,
            "    module `a`, field `f=g`: no `--host` spec names it: its field name `f=g` \
             contains `=`, which ends a name in `--host`'s spelling\n",
        ),
    ] {
        let artifact = Artifact::assembled(text);
        let run = Run::of(&[&artifact.arg()]);
        assert_eq!(run.code, support::exit::DECODE, "{text}: {}", run.transcript());
        assert!(run.err.contains(expected), "{text}: {}", run.transcript());
        assert!(!run.err.contains("it needs `--host"), "{text}: {}", run.transcript());
        assert_eq!(run.err.lines().last(), Some(EMBEDDER_POINTER), "{text}: {}", run.transcript());
    }
}

/// A spec that names no import of the module is listed as such, so a misspelt
/// name is read off the report rather than found by comparing it against the
/// command line.
///
/// The typo registers a stub under names nothing imports, so the import it was
/// meant for is still unsupplied and is listed with the spec it needs, beside
/// the five stubs that did bind; the misspelt spec follows. Fails if a spec
/// matching no import is left out, if a matching one is listed as an orphan,
/// or if the heading goes back to claiming a stub sits beside every import.
#[test]
fn a_spec_naming_no_import_is_listed_after_the_imports() {
    let artifact = Artifact::compiled(FPRIME);
    let run = Run::of(&[
        &artifact.arg(),
        "--host",
        "fprime_core.panic=iii",
        "--host",
        "fprime_core.rsleep=I",
        "--host",
        "fprime_core.command=ii:i",
        "--host",
        "fprime_core.message=ii",
        "--host",
        "fprime_core.telemetry=iiiii:i",
        "--host",
        "env.clok_ms=:I",
        "--invoke",
        "report",
        "3",
    ]);
    assert_eq!(run.code, support::exit::DECODE, "{}", run.transcript());
    assert!(
        run.err.contains(
            "  it imports these; beside each, what `--host` registered under its two names:\n    \
             fprime_core.panic: stub registered\n    fprime_core.rsleep: stub registered\n    \
             fprime_core.command: stub registered\n    fprime_core.message: stub registered\n    \
             fprime_core.telemetry: stub registered\n    \
             env.clock_ms: no stub; it needs `--host env.clock_ms=:I`\n    `--host \
             env.clok_ms=:I` names no import of this module\n"
        ),
        "{}",
        run.transcript()
    );
    assert_eq!(run.err.lines().last(), Some(HOST_POINTER), "{}", run.transcript());
}

/// An export that is the module's own import exported again is not callable,
/// and the harness says why and what to invoke instead rather than panicking.
///
/// The engine invokes only WebAssembly functions, and a stub has no body there
/// to run, so the export is listed apart from the functions that can be
/// invoked, named for the import behind it and given no signature; asked for
/// by name, it is a missing export with its own explanation. The import
/// re-exported is the third function of the second host module, so a refusal
/// reading either index as zero, or the two swapped, names another stub or
/// none. Fails if the harness panics on it, as the shared `invoke` does for an
/// in-process caller, if the refusal or the listing names any import but the
/// one behind the export, if the refusal loses its reason or its remedy, or if
/// the listing — a bare run's and the one after the refusal alike — drops the
/// export, which would read the export section as shorter than it is, or
/// lists it with a signature a reader would copy onto the command line.
#[test]
fn an_export_of_a_host_import_is_named_rather_than_called() {
    let artifact = Artifact::assembled(
        r#"(module
             (import "env" "a" (func $a (result i32)))
             (import "clock" "tick" (func $tick (result i32)))
             (import "clock" "tock" (func $tock (result i32)))
             (import "clock" "ms" (func $ms (result i64)))
             (export "ms" (func $ms))
             (func (export "now") (result i64) (call $ms)))"#,
    );
    let path = artifact.arg();
    let stubs = [
        path.as_str(),
        "--host",
        "env.a=:i",
        "--host",
        "clock.tick=:i",
        "--host",
        "clock.tock=:i",
        "--host",
        "clock.ms=:I",
    ];
    let listing = "  it exports:\n    now() -> i64\n    ms: the host import `clock.ms`, which the \
                   interpreter cannot invoke\n";

    let bare = Run::of(&stubs);
    assert_eq!(bare.code, support::exit::OK, "{}", bare.transcript());
    assert_eq!(bare.out, format!("loaded {path}\n{listing}"), "{}", bare.transcript());

    let mut argv = stubs.to_vec();
    argv.extend(["--invoke", "ms"]);
    let run = Run::of(&argv);
    assert_eq!(run.code, support::exit::NO_SUCH_EXPORT, "{}", run.transcript());
    assert_eq!(
        run.err.lines().next(),
        Some(
            format!(
                "error: {}: `ms` resolves to the host import `clock.ms`: the interpreter invokes \
                 only functions with a WebAssembly body, and a host import's body is the \
                 embedder's, so invoke an export that calls it instead",
                artifact.arg()
            )
            .as_str()
        ),
        "{}",
        run.transcript()
    );
    assert!(run.err.contains(listing), "{}", run.transcript());
    assert!(!run.err.contains("    ms("), "{}", run.transcript());
}

/// A call a start function makes is printed straight after the load, before
/// anything the command line asked for, and whichever way the run then ends.
///
/// The start function runs inside the load, before the command line is acted
/// on, so its calls would otherwise wait for a later point that some runs
/// never reach. A bare run is read with both streams interleaved, since the
/// call line on stderr has to come before the `loaded` line on stdout, and
/// that order is invisible to the two streams read apart; so is a run asking
/// for `--stats`, whose measurement is the first thing printed after the
/// load. Two runs that end before any invocation follow: a missing export and
/// a missing argument, each a refusal printed after the load. Fails if the log
/// is printed only on a path that reaches an invocation, on a bare run's way
/// out after the listing, or after the measurement, which would lose or
/// misplace every host call made while the module was being set up.
#[test]
fn host_calls_a_start_function_makes_are_printed_straight_after_the_load() {
    let artifact = Artifact::assembled(
        r#"(module
             (import "env" "ping" (func $ping))
             (func $setup (call $ping))
             (start $setup)
             (func (export "go") (param i32)))"#,
    );
    let path = artifact.arg();
    let run = Run::of(&[&path, "--host", "env.ping="]);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert!(run.out.starts_with("loaded "), "{}", run.transcript());
    assert_eq!(run.err, "host call: env.ping()\n", "{}", run.transcript());

    let (code, both) = Interleaved::of(&[&path, "--host", "env.ping="]);
    assert_eq!(code, support::exit::OK, "{both}");
    assert!(both.starts_with("host call: env.ping()\nloaded "), "{both}");

    let (code, both) = Interleaved::of(&[&path, "--host", "env.ping=", "--stats"]);
    assert_eq!(code, support::exit::OK, "{both}");
    assert!(both.starts_with("host call: env.ping()\ncode pages: "), "{both}");

    for (ending, code) in [
        ("missing", support::exit::NO_SUCH_EXPORT),
        ("go", support::exit::USAGE),
    ] {
        let run = Run::of(&[&path, "--host", "env.ping=", "--invoke", ending]);
        assert_eq!(run.code, code, "{ending}: {}", run.transcript());
        assert!(
            run.err.starts_with("host call: env.ping()\nerror: "),
            "{ending}: {}",
            run.transcript()
        );
        assert_eq!(run.err.matches("host call:").count(), 1, "{ending}: {}", run.transcript());
    }
}

/// A start function's calls are printed once, before the calls the invocation
/// makes, and the invocation's are not printed with them.
///
/// The log is printed twice in a run that invokes something — after the load,
/// for the start function, and after the invocation — and each printing
/// forgets what it printed. This is the one row with a call in both, so it is
/// the one that sees a printing that does not forget: `env.ping()` would be
/// printed again after the invocation. Fails if either call is printed twice,
/// out of order, or not at all.
#[test]
fn a_start_call_is_printed_once_before_the_invocations_calls() {
    let artifact = Artifact::assembled(
        r#"(module
             (import "env" "ping" (func $ping))
             (import "env" "pong" (func $pong))
             (func $setup (call $ping))
             (start $setup)
             (func (export "go") (call $pong)))"#,
    );
    let run = Run::of(&[
        &artifact.arg(),
        "--host",
        "env.ping=",
        "--host",
        "env.pong=",
        "--invoke",
        "go",
    ]);
    assert_eq!(run.code, support::exit::OK, "{}", run.transcript());
    assert_eq!(run.out, "go = (unit)\n", "{}", run.transcript());
    assert_eq!(run.err, "host call: env.ping()\nhost call: env.pong()\n", "{}", run.transcript());
}

/// A host call made before a trap is printed before the trap is.
///
/// Fails if the log is flushed only on a clean return, which would hide the
/// last thing a trapping program asked of its host, or if it is printed after
/// the trap line.
#[test]
fn a_host_call_before_a_trap_is_printed_before_the_trap() {
    let artifact = Artifact::assembled(
        r#"(module
             (import "env" "ping" (func $ping))
             (func (export "boom") (call $ping) (unreachable)))"#,
    );
    let run = Run::of(&[&artifact.arg(), "--host", "env.ping=", "--invoke", "boom"]);
    assert_eq!(run.code, support::exit::TRAP, "{}", run.transcript());
    assert_eq!(
        run.err,
        "host call: env.ping()\nerror: `boom` trapped: Unreachable\n",
        "{}",
        run.transcript()
    );
}

/// A host call made before the budget runs out is printed before the line
/// saying it ran out.
///
/// An exhausted budget is the third way an invocation ends, and the row above
/// cannot see it: a log printed only after a value or a trap would drop the
/// call here. Fails if the log is flushed only on the other two endings, or
/// after the fuel line.
#[test]
fn a_host_call_before_the_budget_runs_out_is_printed_before_the_fuel_line() {
    let artifact = Artifact::assembled(
        r#"(module
             (import "env" "ping" (func $ping))
             (func (export "spin") (call $ping) (loop (br 0))))"#,
    );
    let run = Run::of(&[
        &artifact.arg(),
        "--host",
        "env.ping=",
        "--invoke",
        "spin",
        "--fuel",
        "100",
    ]);
    assert_eq!(run.code, support::exit::OUT_OF_FUEL, "{}", run.transcript());
    assert_eq!(
        run.err,
        "host call: env.ping()\nerror: `spin` was still running after 100 instructions; raise \
         `--fuel` if the program is meant to run longer\n",
        "{}",
        run.transcript()
    );
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
