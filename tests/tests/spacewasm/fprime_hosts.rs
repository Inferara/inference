//! Programs this compiler built, run against the F´ (F Prime) reference hosts
//! the runner registers.
//!
//! `host_imports` next door registers hosts of its own that record every call
//! and answer what a row chooses. This module registers none of its own: it
//! loads each program through `fprime::load`, the one way the runner offers to
//! run a module against the reference hosts, so what is pinned is those hosts
//! — their log lines, what they read out of and write into a compiled
//! program's memory, and how they stop it — meeting the code generator's own
//! layout of an array argument, which is passed as the address of the caller's
//! buffer.

use std::num::NonZeroUsize;
use std::sync::LazyLock;
use std::time::Duration;

use inference_spacewasm_runner::fprime::{self, HostedModule, REFERENCE_HOSTS, ReferenceHost};
use inference_spacewasm_runner::{
    EngineConfig, Fuel, HostLog, HostTrap, ImportProblem, LoadError, Outcome, TrapReason, Value,
    out_of_fuel,
};
use inference_tests::corpus::wasm_for_target;
use inference_wasm_codegen::Target;

use crate::support::SpaceWasmSession;

/// The committed F´ fixture, read rather than copied so that this module, the
/// recording rows next door and the codegen golden describe one program.
const FPRIME_SOURCE: &str = include_str!(
    "../../test_data/codegen/wasm/extern_import/host_import_fprime/host_import_fprime.inf"
);

/// A program calling all six reference hosts and ending in `panic`:
/// `command`'s answer is `telemetry`'s id, and `rsleep` sleeps for the clock
/// reading pushed past `u32::MAX`.
const EVERY_CALL: &str = r"external fn panic(text: [u8; 4], len: i32, line: i32);
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
    let now: i64 = clock_ms();
    rsleep(now + 5000000000);
    let boom: [u8; 4] = [98, 111, 111, 109];
    panic(boom, 4, 17);
    return now;
}";

/// Payloads handed to `message` and `panic` from an eleven-byte array, with
/// the length each function passes beside it.
const PAYLOADS: &str = r"external fn message(text: [u8; 11], len: i32);
external fn panic(text: [u8; 11], len: i32, line: i32);
use { message, panic } from host::fprime_core;

pub fn forged() {
    let text: [u8; 11] = [97, 10, 80, 65, 78, 73, 67, 32, 120, 58, 49];
    message(text, 11);
}

pub fn padded() {
    let text: [u8; 11] = [104, 101, 108, 108, 111, 0, 0, 0, 0, 0, 0];
    message(text, 8);
}

pub fn unicode() {
    let text: [u8; 11] = [104, 195, 169, 108, 108, 111, 32, 226, 156, 147, 0];
    message(text, 10);
}

pub fn past_memory() {
    let text: [u8; 11] = [104, 101, 108, 108, 111, 0, 0, 0, 0, 0, 0];
    message(text, 70000);
}

pub fn not_text() {
    let text: [u8; 11] = [104, 101, 255, 108, 111, 0, 0, 0, 0, 0, 0];
    message(text, 5);
}

pub fn forged_panic() {
    let text: [u8; 11] = [97, 10, 80, 65, 78, 73, 67, 32, 120, 58, 49];
    panic(text, 11, 5);
}

pub fn panic_not_text() {
    let text: [u8; 11] = [111, 107, 128, 0, 0, 0, 0, 0, 0, 0, 0];
    panic(text, 3, 5);
}";

/// A downlink whose time the caller seeded with the bytes 1 to 11, answering
/// the sum of those bytes after the call: 66 if the host wrote nothing, 0 if
/// it wrote the whole time, and 255 if it refused the downlink.
const DOWNLINK: &str = r"external fn telemetry(id: i32, mut time: [u8; 11], time_len: i32, value: [u8; 4], value_len: i32) -> i32;
use { telemetry } from host::fprime_core;

pub fn downlink(time_len: i32) -> u8 {
    let mut time: [u8; 11] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
    let value: [u8; 4] = [21, 0, 0, 0];
    let status: i32 = telemetry(3, time, time_len, value, 4);
    if status != 0 {
        return 255;
    }
    return time[0] + time[1] + time[2] + time[3] + time[4] + time[5] + time[6] + time[7]
        + time[8] + time[9] + time[10];
}";

/// Each program compiled once at the SpaceWasm target.
static FPRIME: LazyLock<Vec<u8>> = LazyLock::new(|| compiled(FPRIME_SOURCE));
static EVERY_CALL_WASM: LazyLock<Vec<u8>> = LazyLock::new(|| compiled(EVERY_CALL));
static PAYLOADS_WASM: LazyLock<Vec<u8>> = LazyLock::new(|| compiled(PAYLOADS));
static DOWNLINK_WASM: LazyLock<Vec<u8>> = LazyLock::new(|| compiled(DOWNLINK));

fn compiled(source: &str) -> Vec<u8> {
    wasm_for_target(source, Target::SpaceWasm)
}

/// A budget none of these programs comes near.
const BUDGET: Fuel = Fuel::Limited(NonZeroUsize::new(1_000_000).expect("the budget is not zero"));

/// The reference host named `field`.
fn host(field: &str) -> &'static ReferenceHost {
    REFERENCE_HOSTS.iter().find(|host| host.field == field).expect("a reference host")
}

/// `wasm` loaded against the reference hosts at the reference configuration,
/// logging to `log`.
fn hosted<'s>(
    session: &'s mut SpaceWasmSession,
    wasm: &[u8],
    log: &HostLog,
) -> HostedModule<'s> {
    fprime::load(session, wasm, log.clone(), BUDGET, EngineConfig::REFERENCE)
        .unwrap_or_else(|e| panic!("the program loads against the reference hosts: {e}"))
}

/// How one call ended: its outcome, the detail a host recorded, and the lines
/// the hosts logged.
struct Call {
    outcome: Outcome,
    detail: Option<HostTrap>,
    log: Vec<String>,
}

/// Calls `export` of `wasm` once with `args`, against the reference hosts.
fn call(wasm: &[u8], export: &str, args: &[Value]) -> Call {
    let mut session = SpaceWasmSession::acquire();
    let log = HostLog::recording();
    let mut module = hosted(&mut session, wasm, &log);
    let outcome = module.invoke(export, args).expect("the call can be made");
    Call { outcome, detail: module.host_trap(), log: log.recorded() }
}

/// The committed F´ fixture runs end to end: it announces itself, sends its
/// command, downlinks under the id the command answered — 0, from the
/// reference host — and, telemetry answering 0 too, returns the clock.
///
/// The call is made 25 ms after the load, and a refused downlink returns 0
/// without reading the clock, so a reading of 25 or more is `clock_ms`'s.
/// Fails if a host's line, its order or an answer changes, if the program
/// stops reaching the clock, or if the clock stops counting from the load.
#[test]
fn the_committed_fixture_runs_against_the_reference_hosts() {
    let mut session = SpaceWasmSession::acquire();
    let log = HostLog::recording();
    let mut module = hosted(&mut session, &FPRIME, &log);
    std::thread::sleep(Duration::from_millis(25));
    let outcome = module.invoke("report", &[Value::I32(3)]);
    assert_eq!(log.recorded(), ["MESSAGE report", "COMMAND 1 3", "TELEMETRY 0"]);
    let Ok(Outcome::Returned(Some(Value::I64(millis)))) = outcome else {
        panic!("`report` returns the clock: {outcome:?}");
    };
    assert!((25..60_000).contains(&millis), "{millis} ms since a load 25 ms old");
    assert_eq!(module.host_trap(), None);
}

/// A program calling every host logs each call in order and is stopped by
/// `panic`, which the report names.
///
/// The call is made 25 ms after the load, and `rsleep` is handed the clock
/// reading plus 5,000,000,000. Fails if a host drops a line, if the program
/// stops reaching `clock_ms` — the ticks would be 5,000,000,000 flat — if an
/// argument is read at the wrong width — the ticks exceed `u32::MAX` — or if
/// `panic` stops ending the program.
#[test]
fn a_program_calling_every_host_is_stopped_by_panic() {
    let mut session = SpaceWasmSession::acquire();
    let log = HostLog::recording();
    let mut module = hosted(&mut session, &EVERY_CALL_WASM, &log);
    std::thread::sleep(Duration::from_millis(25));
    assert_eq!(module.invoke("go", &[]), Ok(Outcome::Trapped(TrapReason::Host)));
    let recorded = log.recorded();
    let ticks: i64 = recorded
        .get(3)
        .and_then(|line| line.strip_prefix("RSLEEP "))
        .and_then(|ticks| ticks.parse().ok())
        .unwrap_or_else(|| panic!("the fourth line is `rsleep`'s: {recorded:?}"));
    assert!(
        (5_000_000_025..5_000_060_000).contains(&ticks),
        "`rsleep` was handed {ticks}, the clock of a load 25 ms old plus 5000000000"
    );
    assert_eq!(
        recorded,
        [
            "MESSAGE hi".to_string(),
            "COMMAND 42 7".to_string(),
            "TELEMETRY 0".to_string(),
            format!("RSLEEP {ticks}"),
            "PANIC boom:17".to_string(),
        ]
    );
    assert_eq!(module.host_trap(), Some(HostTrap::Panic { host: host("panic") }));
    let report = module.trap_report("go", TrapReason::Host);
    assert_eq!(
        report.headline(),
        "`go` trapped: it called `fprime_core.panic`, which always stops the program; its \
         message is the PANIC line above (Host)."
    );
    assert_eq!(report.explanation("`embedder`"), None);
}

/// A payload read out of a compiled array is logged on one line, its control
/// characters escaped and its printable text as it is.
///
/// Fails if a newline in a program's text reaches the log raw — which lets a
/// `MESSAGE` forge a `PANIC` line — if zero padding vanishes, or if text
/// beyond ASCII is escaped.
#[test]
fn a_compiled_payload_is_logged_on_one_line() {
    for (export, line) in [
        ("forged", r"MESSAGE a\nPANIC x:1"),
        ("padded", r"MESSAGE hello\0\0\0"),
        ("unicode", "MESSAGE héllo ✓"),
    ] {
        let run = call(&PAYLOADS_WASM, export, &[]);
        assert_eq!(run.outcome, Outcome::Returned(None), "{export}");
        assert_eq!(run.log, [line], "{export}");
    }
    let run = call(&PAYLOADS_WASM, "forged_panic", &[]);
    assert_eq!(run.outcome, Outcome::Trapped(TrapReason::Host));
    assert_eq!(run.log, [r"PANIC a\nPANIC x:1:5"]);
}

/// A length past the end of memory, trusted as the host trusts it, stops the
/// program with the host's detail and logs nothing.
///
/// The array sits in the caller's frame near the top of the one page, so its
/// address is the code generator's to choose; the row pins that it is inside
/// memory and that 70000 bytes from it are not. Fails if the host fails
/// instead of trapping, or if the report loses a number.
#[test]
fn a_compiled_length_past_the_end_of_memory_stops_the_program() {
    let mut session = SpaceWasmSession::acquire();
    let log = HostLog::recording();
    let mut module = hosted(&mut session, &PAYLOADS_WASM, &log);
    assert_eq!(module.invoke("past_memory", &[]), Ok(Outcome::Trapped(TrapReason::Host)));
    let Some(HostTrap::OutOfBounds { host: trapped, address, len, memory_bytes }) =
        module.host_trap()
    else {
        panic!("an out-of-bounds read: {:?}", module.host_trap());
    };
    assert_eq!((trapped, len, memory_bytes), (host("message"), 70_000, 65_536));
    assert!(address < 65_536, "the array is in memory, at {address}");
    assert_eq!(
        module.trap_report("past_memory", TrapReason::Host).headline(),
        format!(
            "`past_memory` trapped: `fprime_core.message` was given 70000 bytes at address \
             {address}, outside the module's 65536-byte linear memory (Host)."
        )
    );
    assert_eq!(log.line_count(), 0);
    assert_eq!(module.invoke("padded", &[]), Ok(Outcome::Returned(None)), "still callable");
}

/// Bytes a compiled array holds that are not UTF-8 stop the program at the
/// first invalid one, whichever host prints them.
#[test]
fn compiled_bytes_that_are_not_utf8_stop_the_program() {
    for (export, field, first_invalid, len) in
        [("not_text", "message", 2, 5), ("panic_not_text", "panic", 2, 3)]
    {
        let run = call(&PAYLOADS_WASM, export, &[]);
        assert_eq!(run.outcome, Outcome::Trapped(TrapReason::Host), "{export}");
        assert_eq!(
            run.detail,
            Some(HostTrap::NotUtf8 { host: host(field), first_invalid, len }),
            "{export}"
        );
        assert!(run.log.is_empty(), "{export}: {:?}", run.log);
    }
}

/// `telemetry` writes its eleven-byte time into the caller's own `let mut`
/// array, which the program reads back after the call; told the array is
/// shorter, it stops the program instead.
///
/// The array is seeded with the bytes 1 to 11, so the sum the program returns
/// is 66 untouched and 0 written. Fails if the time stops reaching the
/// caller's buffer — the host would be handed a copy — if it is written short,
/// or if a short `time_len` stops being refused.
#[test]
fn telemetry_writes_the_callers_time_and_refuses_a_short_one() {
    for time_len in [11, 12] {
        let run = call(&DOWNLINK_WASM, "downlink", &[Value::I32(time_len)]);
        assert_eq!(run.outcome, Outcome::Returned(Some(Value::I32(0))), "time_len {time_len}");
        assert_eq!(run.log, ["TELEMETRY 3"]);
    }

    let mut session = SpaceWasmSession::acquire();
    let log = HostLog::recording();
    let mut module = hosted(&mut session, &DOWNLINK_WASM, &log);
    assert_eq!(
        module.invoke("downlink", &[Value::I32(8)]),
        Ok(Outcome::Trapped(TrapReason::Host))
    );
    assert_eq!(
        module.host_trap(),
        Some(HostTrap::ShortTime { host: host("telemetry"), time_len: 8 })
    );
    let report = module.trap_report("downlink", TrapReason::Host);
    assert_eq!(
        report.headline(),
        "`downlink` trapped: `fprime_core.telemetry` writes an 11-byte F Prime time, and \
         `time_len` is 8 (Host)."
    );
    assert_eq!(
        report.explanation("`embedder`").as_deref(),
        Some("Declare the parameter `mut time: [u8; 11]` and pass 11.")
    );
    assert_eq!(log.line_count(), 0);
}

/// A compiled program that sends a command and then counts past its budget
/// runs out of fuel with the command logged, and given no budget counts to
/// the end.
///
/// Counting to a million takes several million instructions, and counting to
/// zero fits the same budget of a thousand, so it is the loop that runs out.
/// Fails if the budget stops reaching a compiled program's call, if running
/// out reports another number or loses the line a host logged before it, or
/// if running out is recorded as a host's trap.
#[test]
fn a_compiled_program_that_runs_out_of_fuel_keeps_the_host_calls_it_made() {
    let wasm = compiled(
        r"external fn command(opcode: i32, arg: i32) -> i32;
use { command } from host::fprime_core;

pub fn spin(to: i32) -> i32 {
    let id: i32 = command(42, 7);
    let mut i: i32 = 0;
    loop i < to {
        i = i + 1;
    }
    return i + id;
}",
    );
    let thousand = Fuel::Limited(NonZeroUsize::new(1_000).expect("the budget is not zero"));
    let mut session = SpaceWasmSession::acquire();
    for (to, ended) in [
        (0, Outcome::Returned(Some(Value::I32(0)))),
        (1_000_000, Outcome::OutOfFuel { budget: 1_000 }),
    ] {
        let log = HostLog::recording();
        let mut module =
            fprime::load(&mut session, &wasm, log.clone(), thousand, EngineConfig::REFERENCE)
                .unwrap_or_else(|e| panic!("the program loads: {e}"));
        assert_eq!(module.invoke("spin", &[Value::I32(to)]), Ok(ended), "counting to {to}");
        assert_eq!(log.recorded(), ["COMMAND 42 7"], "counting to {to}");
        assert_eq!(log.line_count(), 1);
        assert_eq!(module.host_trap(), None, "counting to {to}");
    }
    assert_eq!(
        out_of_fuel("spin", 1_000),
        "`spin` ran out of fuel: the SpaceWasm interpreter stopped it after 1000 interpreter \
         instructions"
    );

    let log = HostLog::recording();
    let mut module =
        fprime::load(&mut session, &wasm, log.clone(), Fuel::Unbounded, EngineConfig::REFERENCE)
            .unwrap_or_else(|e| panic!("the program loads: {e}"));
    assert_eq!(
        module.invoke("spin", &[Value::I32(1_000_000)]),
        Ok(Outcome::Returned(Some(Value::I32(1_000_000))))
    );
    assert_eq!(log.recorded(), ["COMMAND 42 7"]);
}

/// A compiled program declaring a host the reference set does not have, or
/// one at another signature, is refused before it is decoded, with the
/// signature its declaration lowered to.
///
/// Fails if a compiled import is misjudged — an array parameter is one `i32`
/// — or if a program the reference hosts would refuse is loaded at all.
#[test]
fn a_compiled_program_declaring_other_hosts_is_refused_before_loading() {
    let wasm = compiled(
        r"external fn beep();
external fn message(text: [u8; 5]);
external fn telemetry(a: i32, b: i32) -> i32;
use { beep, message, telemetry } from host::fprime_core;

pub fn go() -> i32 {
    beep();
    let text: [u8; 5] = [104, 101, 108, 108, 111];
    message(text);
    return telemetry(1, 2);
}",
    );
    let mut session = SpaceWasmSession::acquire();
    let log = HostLog::recording();
    let refusal = fprime::load(&mut session, &wasm, log, BUDGET, EngineConfig::REFERENCE)
        .map(drop)
        .expect_err("the imports are not the reference hosts");
    let LoadError::UnsupportedImports(unsupported) = refusal else {
        panic!("an import refusal: {refusal:?}");
    };
    let problems: Vec<(&str, &ImportProblem)> = unsupported
        .imports()
        .iter()
        .map(|import| (import.field.as_str(), &import.problem))
        .collect();
    assert_eq!(
        problems,
        [
            ("beep", &ImportProblem::Unknown { elsewhere: None }),
            (
                "message",
                &ImportProblem::Mismatch {
                    params: vec!["i32".to_string()],
                    results: Vec::new(),
                    host: host("message"),
                }
            ),
            (
                "telemetry",
                &ImportProblem::Mismatch {
                    params: vec!["i32".to_string(), "i32".to_string()],
                    results: vec!["i32".to_string()],
                    host: host("telemetry"),
                }
            ),
        ]
    );
    assert_eq!(fprime::check_imports(&FPRIME), Ok(()), "the committed fixture is accepted");
}
