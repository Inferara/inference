//! The F´ reference hosts, the import check and the F´ loader, against the
//! real interpreter.
//!
//! The modules are hand-written so that a row can put a buffer exactly where
//! it wants one — on the last byte of memory, past it, over bytes that are not
//! UTF-8 — and read memory back after a call. What a program this compiler
//! built does against the same hosts is the SpaceWasm tier's to pin.

use std::num::NonZeroUsize;
use std::time::Duration;

use inference_spacewasm_runner::fprime::{
    self, HostedInstance, LOWERING_NOTE, REFERENCE_HOSTS, ReferenceHost, check_imports,
    reference_table_lines,
};
use inference_spacewasm_runner::{
    ArgumentError, EngineConfig, Fuel, HostLog, HostTrap, HostedStartError, ImportProblem,
    InvokeError, Limit, LoadError, Outcome, Session, StartError, TrapReason, TrapReport,
    UnsupportedImports, Value, coerce_arguments, ir_stats,
};
use inference_target_conformance::spacewasm::check;
use spacewasm::{AllocError, MemoryError, ValidationError};

/// A budget no row that is meant to finish comes near.
const BUDGET: Fuel = Fuel::Limited(NonZeroUsize::new(1_000_000).expect("the budget is not zero"));

/// The bytes one page of linear memory holds.
const PAGE: usize = 65_536;

fn wat(text: &str) -> Vec<u8> {
    wat::parse_str(text).unwrap_or_else(|e| panic!("the fixture is not valid WAT: {e}\n{text}"))
}

/// The reference host registered as `name`, written `module.field`.
fn host(name: &str) -> &'static ReferenceHost {
    REFERENCE_HOSTS
        .iter()
        .find(|host| host.name() == name)
        .unwrap_or_else(|| panic!("`{name}` is a reference host"))
}

/// The six imports at their reference signatures, under the names the rows
/// call them by.
const IMPORTS: &str = r#"
  (import "fprime_core" "panic" (func $panic (param i32 i32 i32)))
  (import "fprime_core" "rsleep" (func $rsleep (param i64)))
  (import "fprime_core" "command" (func $command (param i32 i32) (result i32)))
  (import "fprime_core" "message" (func $message (param i32 i32)))
  (import "fprime_core" "telemetry" (func $telemetry (param i32 i32 i32 i32 i32) (result i32)))
  (import "env" "clock_ms" (func $clock_ms (result i64)))"#;

/// A module importing all six hosts, with one page of memory, a `peek`
/// export reading eight bytes of it, and `rest`.
fn fprime_module(rest: &str) -> Vec<u8> {
    wat(&format!(
        r#"(module {IMPORTS}
             (memory 1)
             (func (export "peek") (param i32) (result i64) (i64.load (local.get 0)))
             {rest})"#
    ))
}

/// `wasm` loaded against the reference hosts, logging to `log`, and started
/// under [`BUDGET`].
fn hosted<'s>(session: &'s mut Session, wasm: &[u8], log: &HostLog) -> HostedInstance<'s> {
    fprime::load(session, wasm, log.clone(), EngineConfig::REFERENCE)
        .unwrap_or_else(|e| panic!("the fixture loads: {e}"))
        .start(BUDGET)
        .unwrap_or_else(|e| panic!("the fixture starts: {e}"))
}

/// How one call to `export` ended: its outcome, the detail a host recorded,
/// and the lines the hosts logged.
struct Call {
    outcome: Outcome,
    detail: Option<HostTrap>,
    log: Vec<String>,
}

/// Loads `wasm` against the reference hosts and calls `export` once.
fn call(wasm: &[u8], export: &str) -> Call {
    let mut session = Session::acquire();
    let log = HostLog::recording();
    let mut module = hosted(&mut session, wasm, &log);
    let outcome = module.invoke(export, &[]).expect("the call can be made");
    Call { outcome, detail: module.host_trap(), log: log.recorded() }
}

/// The sixteen bytes at `address`, as two `peek`s of `module` read them.
fn sixteen_bytes(instance: &mut HostedInstance<'_>, address: i32) -> Vec<u8> {
    [address, address + 8]
        .into_iter()
        .flat_map(|at| match instance.invoke("peek", &[Value::I32(at)]) {
            Ok(Outcome::Returned(Some(Value::I64(word)))) => word.to_le_bytes(),
            other => panic!("`peek` reads memory: {other:?}"),
        })
        .collect()
}

/// Every host logs its call in the reference embedder's style, in call order,
/// and `panic` stops the program after logging.
///
/// Fails if a host logs under another verb, with its values in another form or
/// out of order, or if `panic` stops returning a trap.
#[test]
fn every_host_logs_its_call_in_the_reference_style() {
    let wasm = fprime_module(
        r#"(data (i32.const 16) "hello") (data (i32.const 32) "boom")
           (func (export "go")
             (call $message (i32.const 16) (i32.const 5))
             (drop (call $command (i32.const 42) (i32.const 7)))
             (call $rsleep (i64.const 5000000000))
             (drop (call $telemetry (i32.const 3) (i32.const 100) (i32.const 11) (i32.const 0)
                                    (i32.const 4)))
             (drop (call $clock_ms))
             (call $panic (i32.const 32) (i32.const 4) (i32.const 17)))"#,
    );
    let run = call(&wasm, "go");
    assert_eq!(
        run.log,
        ["MESSAGE hello", "COMMAND 42 7", "RSLEEP 5000000000", "TELEMETRY 3", "PANIC boom:17"]
    );
    assert_eq!(run.outcome, Outcome::Trapped(TrapReason::Host));
    assert_eq!(run.detail, Some(HostTrap::Panic { host: host("fprime_core.panic") }));
}

/// Every logged value is a plain signed decimal, a negative one included.
///
/// Fails if a value is logged in Rust's debug form, as upstream logs
/// `rsleep` and `command`, or read unsigned.
#[test]
fn logged_values_are_plain_signed_decimals() {
    let wasm = fprime_module(
        r#"(data (i32.const 0) "x")
           (func (export "go")
             (drop (call $command (i32.const -1) (i32.const 2147483647)))
             (call $rsleep (i64.const -1))
             (drop (call $telemetry (i32.const -5) (i32.const 100) (i32.const 11) (i32.const 0)
                                    (i32.const 0)))
             (call $panic (i32.const 0) (i32.const 1) (i32.const -3)))"#,
    );
    assert_eq!(
        call(&wasm, "go").log,
        ["COMMAND -1 2147483647", "RSLEEP -1", "TELEMETRY -5", "PANIC x:-3"]
    );
}

/// `command` and `telemetry` both answer 0, as `i32`s, and the program goes on
/// past them.
///
/// Fails if either answers another number or another type — the interpreter
/// panics on a host answering a type its signature does not declare.
#[test]
fn command_and_telemetry_answer_zero() {
    let wasm = fprime_module(
        r#"(func (export "go") (result i32)
             (i32.add
               (i32.add (call $command (i32.const 1) (i32.const 2)) (i32.const 40))
               (call $telemetry (i32.const 1) (i32.const 100) (i32.const 11) (i32.const 0)
                                (i32.const 0))))"#,
    );
    let run = call(&wasm, "go");
    assert_eq!(run.outcome, Outcome::Returned(Some(Value::I32(40))));
    assert_eq!(run.log, ["COMMAND 1 2", "TELEMETRY 1"]);
}

/// `clock_ms` answers the milliseconds since its own load, never less than
/// the time that has passed, and logs nothing.
///
/// The first load is left for a second before it is started and read at once,
/// and a second load is started and read at once. Fails if the clock counts
/// from the start or from the first call rather than the load — the first read
/// would be under 1000 — or from an epoch every load shares, whether the
/// process's start or the first load's — the second load's first read would be
/// 1000 or more. The pause is a whole second so that a busy machine cannot
/// stretch a read taken at once past it. Fails too if it answers a clock of its
/// own epoch — the wall clock would read above 1.7 trillion — if it runs
/// backwards or stands still across a sleep, or if it starts logging.
#[test]
fn clock_ms_counts_milliseconds_since_the_load_and_logs_nothing() {
    const IDLE_MS: i64 = 1_000;
    let idle = Duration::from_millis(IDLE_MS.unsigned_abs());
    let wasm = fprime_module(r#"(func (export "now") (result i64) (call $clock_ms))"#);
    let read = |instance: &mut HostedInstance<'_>| match instance.invoke("now", &[]) {
        Ok(Outcome::Returned(Some(Value::I64(millis)))) => millis,
        other => panic!("`clock_ms` answers an i64: {other:?}"),
    };
    let mut session = Session::acquire();
    let log = HostLog::recording();
    let loaded = fprime::load(&mut session, &wasm, log.clone(), EngineConfig::REFERENCE)
        .expect("the fixture loads");
    std::thread::sleep(idle);
    let mut instance = loaded.start(BUDGET).expect("the fixture starts");
    let first = read(&mut instance);
    std::thread::sleep(Duration::from_millis(25));
    let second = read(&mut instance);
    assert!((IDLE_MS..60_000).contains(&first), "{first} ms since a load {IDLE_MS} ms old");
    assert!(second - first >= 25, "{first} ms, then {second} ms after sleeping 25");
    assert_eq!(log.line_count(), 0, "`clock_ms` logs nothing: {:?}", log.recorded());
    drop(instance);

    let mut reloaded = hosted(&mut session, &wasm, &log);
    let fresh = read(&mut reloaded);
    assert!((0..IDLE_MS).contains(&fresh), "{fresh} ms since a load read at once");
}

/// A buffer the memory does not hold stops the program with the host's
/// detail, reading the address and the length unsigned, and logs nothing —
/// while a buffer ending on the last byte is read.
///
/// Fails if an out-of-bounds read fails the host instead of trapping, if the
/// detail's numbers are read signed or dropped, if the bounds are off by one
/// either way, or if `panic` logs a line it could not read.
#[test]
fn a_buffer_outside_memory_stops_the_program_with_its_detail() {
    let message = host("fprime_core.message");
    let panic = host("fprime_core.panic");
    for (calling, address, len) in [
        ("$message", 65_532_i32, 5_i32),
        ("$message", -1, 1),
        ("$message", 0, -1),
        ("$message", 70_000, 0),
    ] {
        let wasm = fprime_module(&format!(
            r#"(func (export "go") (call {calling} (i32.const {address}) (i32.const {len})))"#
        ));
        let run = call(&wasm, "go");
        assert_eq!(run.outcome, Outcome::Trapped(TrapReason::Host), "{address} {len}");
        assert_eq!(
            run.detail,
            Some(HostTrap::OutOfBounds {
                host: message,
                address: address.cast_unsigned(),
                len: len.cast_unsigned(),
                memory_bytes: PAGE,
            })
        );
        assert!(run.log.is_empty(), "{:?}", run.log);
    }

    let panicking = fprime_module(
        r#"(func (export "go") (call $panic (i32.const 65535) (i32.const 2) (i32.const 9)))"#,
    );
    let run = call(&panicking, "go");
    assert_eq!(
        run.detail,
        Some(HostTrap::OutOfBounds { host: panic, address: 65_535, len: 2, memory_bytes: PAGE })
    );
    assert!(run.log.is_empty(), "no PANIC line for a message never read: {:?}", run.log);

    let last_bytes = fprime_module(
        r#"(data (i32.const 65531) "tail!")
           (func (export "go") (call $message (i32.const 65531) (i32.const 5)))"#,
    );
    let run = call(&last_bytes, "go");
    assert_eq!(run.outcome, Outcome::Returned(None));
    assert_eq!(run.log, ["MESSAGE tail!"]);
}

/// A module with no memory has no byte a host can read, and says so as a
/// memory of none.
#[test]
fn a_module_without_memory_holds_no_buffer() {
    let wasm = wat(
        r#"(module
             (import "fprime_core" "message" (func $message (param i32 i32)))
             (func (export "go") (call $message (i32.const 0) (i32.const 1)))
             (func (export "empty") (call $message (i32.const 0) (i32.const 0))))"#,
    );
    let run = call(&wasm, "go");
    assert_eq!(
        run.detail,
        Some(HostTrap::OutOfBounds {
            host: host("fprime_core.message"),
            address: 0,
            len: 1,
            memory_bytes: 0,
        })
    );
    assert_eq!(call(&wasm, "empty").log, ["MESSAGE "], "no bytes at address 0 is an empty text");
}

/// Bytes that are not UTF-8 stop the program, naming where the first invalid
/// byte is, and log nothing.
///
/// Fails if invalid text is logged — lossily or raw — or if the offset is not
/// the first invalid byte's, a truncated sequence at the end included.
#[test]
fn bytes_that_are_not_utf8_stop_the_program_at_the_first_invalid_one() {
    for (calling, bytes, first_invalid, len) in [
        ("$message", r"he\ffllo", 2, 5),
        ("$message", r"h\c3", 1, 2),
        ("$message", r"\80", 0, 1),
        ("$panic", r"ok\ed\a0\80", 2, 5),
    ] {
        let wasm = fprime_module(&format!(
            r#"(data (i32.const 0) "{bytes}")
               (func (export "go") (call {calling} (i32.const 0) (i32.const {len}) {line}))"#,
            line = if calling == "$panic" { "(i32.const 1)" } else { "" }
        ));
        let run = call(&wasm, "go");
        assert_eq!(run.outcome, Outcome::Trapped(TrapReason::Host), "{bytes}");
        let name = if calling == "$panic" { "fprime_core.panic" } else { "fprime_core.message" };
        assert_eq!(
            run.detail,
            Some(HostTrap::NotUtf8 { host: host(name), first_invalid, len }),
            "{bytes}"
        );
        assert!(run.log.is_empty(), "{bytes}: {:?}", run.log);
    }
}

/// A payload's control characters are escaped, so each payload is one line,
/// and printable text reaches the log as it is.
///
/// Fails if a newline or a carriage return reaches the log raw — which lets a
/// `MESSAGE` forge a `PANIC` line of its own — if zero padding vanishes
/// instead of showing, or if printable text beyond ASCII is escaped.
#[test]
fn a_payloads_control_characters_are_escaped_onto_one_line() {
    for (calling, bytes, line) in [
        ("$message", r"a\0aPANIC x:1", r"MESSAGE a\nPANIC x:1"),
        ("$message", r"hello\00\00\00", r"MESSAGE hello\0\0\0"),
        ("$message", r"\0d\09", r"MESSAGE \r\t"),
        ("$message", r"\1b[1m\7f", r"MESSAGE \u{1b}[1m\u{7f}"),
        ("$message", r"\c2\85", r"MESSAGE \u{85}"),
        ("$message", r"h\c3\a9llo \e2\9c\93", "MESSAGE héllo ✓"),
        ("$panic", r"x\0aPANIC y", r"PANIC x\nPANIC y:9"),
    ] {
        let wasm = fprime_module(&format!(
            r#"(data (i32.const 0) "{bytes}")
               (func (export "go") (call {calling} (i32.const 0) (i32.const {len}) {line_no}))"#,
            len = payload_len(bytes),
            line_no = if calling == "$panic" { "(i32.const 9)" } else { "" }
        ));
        let run = call(&wasm, "go");
        assert_eq!(run.log, [line], "{bytes}");
        assert!(run.log.iter().all(|logged| !logged.contains(['\n', '\r'])), "{:?}", run.log);
    }
}

/// The byte length of a WAT string body written with `\hh` escapes.
fn payload_len(escaped: &str) -> usize {
    let mut len = 0;
    let mut rest = escaped.as_bytes();
    while let Some((&first, tail)) = rest.split_first() {
        rest = if first == b'\\' { &tail[2..] } else { tail };
        len += 1;
    }
    len
}

/// `telemetry` writes an eleven-byte time of zero at `time_ptr` — whatever
/// `time_len` says past eleven — and not a byte more.
///
/// Fails if the time stops being written, is written short or long, or lands
/// anywhere but `time_ptr`.
#[test]
fn telemetry_writes_eleven_zero_bytes_at_the_time_pointer() {
    for time_len in [11, 12, 64] {
        let wasm = fprime_module(&format!(
            r#"(data (i32.const 96) "\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff")
               (func (export "go") (result i32)
                 (call $telemetry (i32.const 3) (i32.const 100) (i32.const {time_len})
                                  (i32.const 0) (i32.const 4)))"#
        ));
        let mut session = Session::acquire();
        let log = HostLog::recording();
        let mut module = hosted(&mut session, &wasm, &log);
        assert_eq!(module.invoke("go", &[]), Ok(Outcome::Returned(Some(Value::I32(0)))));
        let mut expected = vec![0xff; 16];
        expected[4..15].fill(0);
        assert_eq!(sixteen_bytes(&mut module, 96), expected, "time_len {time_len}");
        assert_eq!(log.recorded(), ["TELEMETRY 3"]);
    }
}

/// A `time_len` below eleven stops the program before anything is written.
///
/// The reference embedder writes eleven bytes whatever `time_len` says, which
/// runs past a shorter buffer. Fails if the guard goes, or moves to ten or
/// twelve, or if it writes first and traps after — the seeded bytes would
/// read zero — or if the call logs a line.
#[test]
fn a_short_time_len_stops_the_program_before_writing() {
    for time_len in [10, 1, 0, -1, i32::MIN] {
        let wasm = fprime_module(&format!(
            r#"(data (i32.const 96) "\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff")
               (func (export "go") (result i32)
                 (call $telemetry (i32.const 3) (i32.const 100) (i32.const {time_len})
                                  (i32.const 0) (i32.const 4)))"#
        ));
        let mut session = Session::acquire();
        let log = HostLog::recording();
        let mut module = hosted(&mut session, &wasm, &log);
        assert_eq!(module.invoke("go", &[]), Ok(Outcome::Trapped(TrapReason::Host)));
        assert_eq!(
            module.host_trap(),
            Some(HostTrap::ShortTime { host: host("fprime_core.telemetry"), time_len })
        );
        assert_eq!(sixteen_bytes(&mut module, 96), vec![0xff; 16], "time_len {time_len}");
        assert_eq!(log.line_count(), 0);
    }
}

/// A time running past the end of memory is written field by field up to the
/// first field that does not fit, as the reference embedder's eager stores
/// write it, and then stops the program.
///
/// The expected bytes are worked out here from the four fields — a `u16` at
/// +0, a `u8` at +2, and `u32`s at +3 and +7 — each written whole or not at
/// all. Fails if a time that fits exactly traps, if one that does not stops
/// writing any sooner or later than that, or if the trap loses its address.
#[test]
fn a_time_past_the_end_of_memory_is_written_up_to_the_first_field_that_does_not_fit() {
    const SEEDED_FROM: usize = PAGE - 16;
    for time_ptr in [PAGE - 11, PAGE - 10, PAGE - 6, PAGE - 1, PAGE] {
        let wasm = fprime_module(&format!(
            r#"(data (i32.const {SEEDED_FROM}) "\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff")
               (func (export "go") (result i32)
                 (call $telemetry (i32.const 3) (i32.const {time_ptr}) (i32.const 11)
                                  (i32.const 0) (i32.const 4)))"#
        ));
        let mut expected = vec![0xff_u8; 16];
        let mut fits = true;
        for (offset, width) in [(0, 2), (2, 1), (3, 4), (7, 4)] {
            let start = time_ptr + offset;
            if start + width <= PAGE {
                expected[start - SEEDED_FROM..start - SEEDED_FROM + width].fill(0);
            } else {
                fits = false;
            }
        }
        let mut session = Session::acquire();
        let log = HostLog::recording();
        let mut module = hosted(&mut session, &wasm, &log);
        let outcome = module.invoke("go", &[]).expect("the call can be made");
        let detail = module.host_trap();
        let seeded = i32::try_from(SEEDED_FROM).expect("an address");
        assert_eq!(sixteen_bytes(&mut module, seeded), expected, "time_ptr {time_ptr}");
        if fits {
            assert_eq!(outcome, Outcome::Returned(Some(Value::I32(0))), "time_ptr {time_ptr}");
            assert_eq!(log.recorded(), ["TELEMETRY 3"]);
        } else {
            assert_eq!(outcome, Outcome::Trapped(TrapReason::Host), "time_ptr {time_ptr}");
            assert_eq!(
                detail,
                Some(HostTrap::TimeOutOfBounds {
                    host: host("fprime_core.telemetry"),
                    address: u32::try_from(time_ptr).expect("an address"),
                    memory_bytes: PAGE,
                })
            );
            assert_eq!(log.line_count(), 0);
        }
    }
}

/// `telemetry` reads nothing at `value_ptr`, as the reference embedder reads
/// nothing there, so a value outside memory is no refusal.
#[test]
fn telemetry_reads_nothing_at_the_value_pointer() {
    let wasm = fprime_module(
        r#"(func (export "go") (result i32)
             (call $telemetry (i32.const 3) (i32.const 100) (i32.const 11) (i32.const 70000)
                              (i32.const 400000)))"#,
    );
    assert_eq!(call(&wasm, "go").outcome, Outcome::Returned(Some(Value::I32(0))));
}

/// A call forgets the detail an earlier call's host recorded, so a later trap
/// of another kind is not reported as the host's.
///
/// Fails if the detail outlives its call.
#[test]
fn a_call_forgets_the_detail_of_an_earlier_one() {
    let wasm = fprime_module(
        r#"(func (export "bad") (call $message (i32.const 70000) (i32.const 1)))
           (func (export "boom") unreachable)"#,
    );
    let mut session = Session::acquire();
    let log = HostLog::recording();
    let mut module = hosted(&mut session, &wasm, &log);
    assert_eq!(module.invoke("bad", &[]), Ok(Outcome::Trapped(TrapReason::Host)));
    assert!(module.host_trap().is_some());
    assert_eq!(module.invoke("boom", &[]), Ok(Outcome::Trapped(TrapReason::Unreachable)));
    assert_eq!(module.host_trap(), None);
    assert_eq!(
        module.trap_report("boom", TrapReason::Unreachable).headline(),
        "`boom` trapped: a runtime check failed (Unreachable)."
    );
}

/// A report of a host's trap names what the host recorded, and a report of
/// an exhausted stack names the words the module was loaded with.
#[test]
fn a_trap_report_carries_the_hosts_detail_and_the_loads_stack() {
    let wasm = fprime_module(
        r#"(func (export "bad") (call $message (i32.const 70000) (i32.const 5)))
           (func $deep (export "deep") (call $deep))"#,
    );
    let mut session = Session::acquire();
    let log = HostLog::recording();
    let mut module = hosted(&mut session, &wasm, &log);
    assert_eq!(module.invoke("bad", &[]), Ok(Outcome::Trapped(TrapReason::Host)));
    let report = module.trap_report("main", TrapReason::Host);
    assert_eq!(
        report.headline(),
        "`main` trapped: `fprime_core.message` was given 5 bytes at address 70000, outside the \
         module's 65536-byte linear memory (Host)."
    );
    assert_eq!(
        report.explanation("`embedder`").as_deref(),
        Some(
            "The host reads `len` bytes from the address it is given; check the length you pass \
             with the array."
        )
    );
    assert_eq!(module.invoke("deep", &[]), Ok(Outcome::Trapped(TrapReason::StackOverflow)));
    assert_eq!(
        module.trap_report("deep", TrapReason::StackOverflow).explanation("`embedder`").as_deref(),
        Some(
            "`embedder` gives the interpreter 1024 words of call stack, as spacewasm_std does, and \
             this call chain's frames need more. Recursion is refused at compile time (A035), so a \
             deep chain of large frames is the cause."
        )
    );
}

/// The stack a load is given is the one its engine holds and the one the
/// report of an exhausted stack names, whichever it is.
///
/// `wide` keeps 2,000 words of locals in its one frame: more than the
/// reference embedder's 1,024, less than 4,096. `deep` recurses until any
/// stack is full. Fails if `fprime::load` builds the engine, or the report,
/// from anything but the configuration it was handed — the reference one or a
/// number of its own.
#[test]
fn a_loads_stack_is_the_one_its_engine_holds_and_its_report_names() {
    let wasm = fprime_module(&format!(
        r#"(func (export "wide") (result i32) (local {})
             (local.set 1999 (i32.const 7))
             (local.get 1999))
           (func $deep (export "deep") (call $deep))"#,
        "i32 ".repeat(2000)
    ));
    let overflows = Outcome::Trapped(TrapReason::StackOverflow);
    let fits = Outcome::Returned(Some(Value::I32(7)));
    for (stack_words, wide, given) in [
        (512, overflows.clone(), "512 words of call stack,"),
        (1024, overflows.clone(), "1024 words of call stack, as spacewasm_std does,"),
        (4096, fits.clone(), "4096 words of call stack,"),
        (65_536, fits, "65536 words of call stack,"),
    ] {
        let config = EngineConfig { stack_words, ..EngineConfig::REFERENCE };
        let mut session = Session::acquire();
        let mut instance = fprime::load(&mut session, &wasm, HostLog::recording(), config)
            .unwrap_or_else(|e| panic!("the fixture loads with {stack_words} words: {e}"))
            .start(BUDGET)
            .unwrap_or_else(|e| panic!("the fixture starts with {stack_words} words: {e}"));
        assert_eq!(instance.invoke("wide", &[]), Ok(wide), "{stack_words} words");
        assert_eq!(instance.invoke("deep", &[]), Ok(overflows.clone()), "{stack_words} words");
        assert_eq!(
            instance.trap_report("deep", TrapReason::StackOverflow).explanation("`embedder`"),
            Some(format!(
                "`embedder` gives the interpreter {given} and this call chain's frames need \
                 more. Recursion is refused at compile time (A035), so a deep chain of large \
                 frames is the cause."
            ))
        );
    }
}

// ---------------------------------------------------------------------------
// The import check
// ---------------------------------------------------------------------------

/// What `check_imports` refuses in a module holding `imports`.
fn refused(imports: &str) -> UnsupportedImports {
    check_imports(&wat(&format!("(module {imports})")))
        .expect_err("the module imports something the reference hosts do not provide")
}

/// Every import at its reference signature is accepted, and so is a module
/// with none.
#[test]
fn the_reference_signatures_and_no_imports_are_accepted() {
    assert_eq!(check_imports(&wat(&format!("(module {IMPORTS})"))), Ok(()));
    assert_eq!(check_imports(&wat("(module)")), Ok(()));
    assert_eq!(
        check_imports(&wat(r#"(module (import "env" "clock_ms" (func (result i64))))"#)),
        Ok(())
    );
}

/// A function no reference host has both names of is unknown, and one whose
/// field a host carries under the other module says which.
///
/// Fails if a name is matched by its field alone — `fprime_core.clock_ms`
/// would pass — or if the hint names the wrong host or goes missing.
#[test]
fn an_unknown_function_is_refused_with_a_hint_at_the_other_module() {
    for (module, field, elsewhere) in [
        ("fprime_core", "beep", None),
        ("fprime_core", "clock_ms", Some("env.clock_ms")),
        ("env", "message", Some("fprime_core.message")),
        ("env", "telemetry", Some("fprime_core.telemetry")),
        ("fprime", "panic", Some("fprime_core.panic")),
        ("", "", None),
    ] {
        let unsupported = refused(&format!(r#"(import "{module}" "{field}" (func))"#));
        let [import] = unsupported.imports() else {
            panic!("one import is refused: {unsupported:?}");
        };
        assert_eq!((import.module.as_str(), import.field.as_str()), (module, field));
        assert_eq!(import.problem, ImportProblem::Unknown { elsewhere: elsewhere.map(host) });
    }
}

/// A reference host's two names at another signature are refused with both
/// signatures, whichever half differs and however.
///
/// Fails if a signature is compared by its count alone, by its parameters
/// alone or by its results alone, or if the declared side is misread.
#[test]
fn a_signature_mismatch_is_refused_with_both_signatures() {
    for (module, field, signature, params, results) in [
        ("fprime_core", "message", "(param i32)", &["i32"][..], &[][..]),
        ("fprime_core", "message", "(param f32 i32)", &["f32", "i32"][..], &[][..]),
        (
            "fprime_core",
            "message",
            "(param i32 i32) (result i32)",
            &["i32", "i32"][..],
            &["i32"][..],
        ),
        ("fprime_core", "command", "(param i32 i32)", &["i32", "i32"][..], &[][..]),
        ("fprime_core", "rsleep", "(param i32)", &["i32"][..], &[][..]),
        ("fprime_core", "panic", "(param i32 i32 i64)", &["i32", "i32", "i64"][..], &[][..]),
        (
            "fprime_core",
            "telemetry",
            "(param i32 i32 i32 i32) (result i32)",
            &["i32", "i32", "i32", "i32"][..],
            &["i32"][..],
        ),
        ("env", "clock_ms", "(result i32)", &[][..], &["i32"][..]),
        ("env", "clock_ms", "(result i64 i64)", &[][..], &["i64", "i64"][..]),
    ] {
        let unsupported = refused(&format!(r#"(import "{module}" "{field}" (func {signature}))"#));
        let [import] = unsupported.imports() else {
            panic!("one import is refused: {unsupported:?}");
        };
        assert_eq!(
            import.problem,
            ImportProblem::Mismatch {
                params: params.iter().map(ToString::to_string).collect(),
                results: results.iter().map(ToString::to_string).collect(),
                host: host(&format!("{module}.{field}")),
            },
            "{module}.{field} {signature}"
        );
    }
}

/// An import that is not a function is refused by its kind, whatever its
/// names.
#[test]
fn an_import_that_is_not_a_function_is_refused_by_its_kind() {
    for (import, kind) in [
        (r#"(import "env" "memory" (memory 1))"#, "memory"),
        (r#"(import "env" "table" (table 1 funcref))"#, "table"),
        (r#"(import "env" "clock_ms" (global i64))"#, "global"),
        (r#"(import "fprime_core" "panic" (tag))"#, "tag"),
    ] {
        let unsupported = refused(import);
        let [refusal] = unsupported.imports() else {
            panic!("one import is refused: {unsupported:?}");
        };
        assert_eq!(refusal.problem, ImportProblem::NotAFunction { kind }, "{import}");
    }
}

/// Every offender is listed, sorted by module and then field — not by the two
/// joined with a dot — and an accepted import among them is not.
///
/// Fails if the list stops at the first offender, keeps the module's order,
/// or sorts `a.b` `c` before `a` `z` as the joined strings `a.b.c` and `a.z`
/// would.
#[test]
fn every_offender_is_listed_sorted_by_module_then_field() {
    let unsupported = refused(
        r#"(import "fprime_core" "zeta" (func))
           (import "env" "clock_ms" (func (result i64)))
           (import "a.b" "c" (func))
           (import "fprime_core" "message" (func (param i32)))
           (import "a" "z" (func))
           (import "env" "beep" (func))
           (import "env" "beep" (func))"#,
    );
    let names: Vec<(&str, &str)> = unsupported
        .imports()
        .iter()
        .map(|import| (import.module.as_str(), import.field.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("a", "z"),
            ("a.b", "c"),
            ("env", "beep"),
            ("env", "beep"),
            ("fprime_core", "message"),
            ("fprime_core", "zeta"),
        ]
    );
}

/// The refusal reads as the reference table's own text: each import's lines,
/// the lowering note and the table.
///
/// Fails if a line's words, indentation or column alignment drift, if the
/// buffer-length note goes missing from a declaration taking a buffer or
/// appears on one that takes none, or if the two flags stop saying which
/// companion text the lines are owed: the lowering note for a signature
/// mismatch and the table for an unknown name, each on its own, both, or
/// neither for an import that is not a function.
#[test]
fn the_refusal_reads_as_the_reference_text() {
    let unsupported = refused(
        r#"(import "fprime_core" "message" (func (param i32)))
           (import "fprime_core" "beep" (func))"#,
    );
    assert_eq!(
        unsupported.import_lines("`embedder`"),
        [
            "  fprime_core.beep",
            "    not a host function `embedder` provides",
            "  fprime_core.message",
            "    this program declares  (i32)",
            "    the host provides      (ptr: i32, len: i32)",
            "    a declaration that matches: external fn message(text: [u8; N], len: i32);   (N: \
             your buffer's length)",
        ]
    );
    assert!(unsupported.shows_signatures());
    assert!(unsupported.names_an_unknown_function());
    assert_eq!(
        LOWERING_NOTE,
        "Signatures are WebAssembly's: an array or struct argument is passed as its address, and \
         every integer narrower than 64 bits and every `bool` travels as one i32, so `text: [u8; \
         5]` is `i32` here."
    );
    assert_eq!(reference_table_lines().len(), REFERENCE_HOSTS.len());

    let other = refused(
        r#"(import "fprime_core" "clock_ms" (func (result i64)))
           (import "fprime_core" "command" (func (param i32) (result i32)))
           (import "env" "memory" (memory 1))"#,
    );
    assert_eq!(
        other.import_lines("`embedder`"),
        [
            "  env.memory",
            "    a memory import; the reference hosts provide functions only",
            "  fprime_core.clock_ms",
            "    not a host function `embedder` provides; `clock_ms` is provided under `env`: use \
             { clock_ms } from host::env;",
            "  fprime_core.command",
            "    this program declares  (i32) -> i32",
            "    the host provides      (opcode: i32, arg: i32) -> i32",
            "    a declaration that matches: external fn command(opcode: i32, arg: i32) -> i32;",
        ]
    );
    assert!(other.shows_signatures());
    assert!(other.names_an_unknown_function());

    for (imports, shows_signatures, names_an_unknown_function) in [
        (r#"(import "fprime_core" "beep" (func))"#, false, true),
        (r#"(import "fprime_core" "clock_ms" (func (result i64)))"#, false, true),
        (r#"(import "env" "beep" (func)) (import "env" "memory" (memory 1))"#, false, true),
        (r#"(import "fprime_core" "message" (func (param i32)))"#, true, false),
        (r#"(import "env" "clock_ms" (func (result i32)))"#, true, false),
        (
            r#"(import "fprime_core" "rsleep" (func (param i32)))
               (import "env" "table" (table 1 funcref))"#,
            true,
            false,
        ),
        (r#"(import "env" "memory" (memory 1))"#, false, false),
        (r#"(import "env" "g" (global i32)) (import "env" "memory" (memory 1))"#, false, false),
    ] {
        let unsupported = refused(imports);
        assert_eq!(
            (unsupported.shows_signatures(), unsupported.names_an_unknown_function()),
            (shows_signatures, names_an_unknown_function),
            "{imports}"
        );
    }
}

/// The refusal as an error says how many imports it is about and lists them.
#[test]
fn the_refusal_as_an_error_counts_and_lists_the_imports() {
    let one = refused(r#"(import "env" "beep" (func))"#);
    assert_eq!(
        one.to_string(),
        "the module has 1 import that the F Prime reference hosts do not provide as declared:\n  \
         env.beep\n    not a host function the F Prime reference set provides"
    );
    let two = refused(r#"(import "env" "beep" (func)) (import "env" "boop" (func))"#);
    assert!(two.to_string().starts_with("the module has 2 imports that"), "{two}");
}

/// Bytes the check cannot read are left to the interpreter, which refuses
/// them whole.
#[test]
fn bytes_the_check_cannot_read_are_left_to_the_interpreter() {
    assert_eq!(check_imports(b"not wasm"), Ok(()));
    assert_eq!(check_imports(b""), Ok(()));
}

// ---------------------------------------------------------------------------
// The loader
// ---------------------------------------------------------------------------

/// A module carrying an import the hosts do not provide is refused before a
/// byte of it is decoded, and nothing of it runs.
///
/// The module also uses `memory.grow`, which the decoder refuses, and has a
/// start function that calls `message`: without the unsupported import it
/// would be the decoder's refusal, and without either it loads, running
/// nothing, and logs once it is started. Fails if the check stops running
/// first — the refusal would be the decoder's — or if anything of the module
/// runs before it is started.
#[test]
fn an_unsupported_import_is_refused_before_the_module_is_decoded() {
    let module = |beep: &str, grow: &str| {
        wat(&format!(
            r#"(module
                 (import "fprime_core" "message" (func $message (param i32 i32)))
                 {beep}
                 (memory 1)
                 (data (i32.const 0) "hi")
                 (func $start (call $message (i32.const 0) (i32.const 2)))
                 (start $start)
                 (func (export "grow") (result i32) {grow}))"#
        ))
    };
    let beep = r#"(import "env" "beep" (func))"#;
    let grow = "(memory.grow (i32.const 1))";

    let log = HostLog::recording();
    let mut session = Session::acquire();
    let mut refuse = |wasm: &[u8]| {
        fprime::load(&mut session, wasm, log.clone(), EngineConfig::REFERENCE)
            .map(drop)
            .expect_err("the module is refused")
    };
    let refusal = refuse(&module(beep, grow));
    assert!(matches!(refusal, LoadError::UnsupportedImports(_)), "{refusal:?}");
    assert_eq!(log.line_count(), 0, "nothing ran: {:?}", log.recorded());

    let refusal = refuse(&module("", grow));
    let LoadError::Decode(verdict) = refusal else {
        panic!("the decoder's refusal: {refusal:?}");
    };
    assert_eq!(verdict.err.err, ValidationError::IllegalMemoryGrow);

    let loaded = fprime::load(
        &mut session,
        &module("", "(i32.const 0)"),
        log.clone(),
        EngineConfig::REFERENCE,
    )
    .expect("the module loads");
    assert_eq!(log.line_count(), 0, "a load runs nothing: {:?}", log.recorded());
    loaded.start(BUDGET).expect("the start function returns");
    assert_eq!(log.recorded(), ["MESSAGE hi"], "the start function ran and logged");
}

/// A body nested `depth` blocks deep inside its own frame, so it needs
/// `depth + 1` control frames, in a function named `deep`.
fn nested(depth: usize) -> String {
    format!(r#"(func $deep (export "deep") {}{})"#, "(block ".repeat(depth), ")".repeat(depth))
}

/// A body whose operand stack peaks at `peak` values, in a function named
/// `tall`.
fn tall(peak: usize) -> String {
    format!(
        r#"(func $tall (export "tall") {}{})"#,
        "i32.const 0 ".repeat(peak),
        "drop ".repeat(peak)
    )
}

/// Why `wasm` did not load against the reference hosts in `config`.
fn load_error(wasm: &[u8], config: EngineConfig) -> LoadError {
    let mut session = Session::acquire();
    match fprime::load(&mut session, wasm, HostLog::recording(), config) {
        Ok(_) => panic!("the fixture must not load"),
        Err(error) => error,
    }
}

/// A module over one of the verifier's two bounds is reported by the bound it
/// exceeds, the function that needs the most and both numbers — the control
/// nesting first when both are over.
///
/// The decoder's verdict is one `AllocError(OutOfMemory)` for all of it. Fails
/// if the report names the wrong limit, function or number, or loses the
/// decoder's verdict.
#[test]
fn a_module_over_a_verifier_bound_is_reported_by_the_bound_it_exceeds() {
    let frames = load_error(&wat(&format!("(module {})", nested(64))), EngineConfig::REFERENCE);
    let LoadError::OverLimit(over) = &frames else {
        panic!("over the frame bound: {frames:?}");
    };
    assert_eq!(
        (over.limit, over.function.as_str(), over.needed, over.allowed),
        (Limit::ControlFrames, "deep", 65, 64)
    );
    assert_eq!(over.verdict.err.err, ValidationError::AllocError(AllocError::OutOfMemory));
    assert_eq!(
        frames.to_string(),
        "the module does not load under the SpaceWasm interpreter: its control nesting of 65 \
         frames (in `deep`) exceeds the 64 the interpreter's verifier was built with"
    );

    let stack = load_error(&wat(&format!("(module {})", tall(300))), EngineConfig::REFERENCE);
    let LoadError::OverLimit(over) = &stack else {
        panic!("over the stack bound: {stack:?}");
    };
    assert_eq!(
        (over.limit, over.function.as_str(), over.needed, over.allowed),
        (Limit::OperandStack, "tall", 300, 256)
    );
    assert_eq!(over.fact(), "tallest operand stack of 300 values (in `tall`) exceeds the 256");

    let over_both = wat(&format!("(module {} {})", tall(300), nested(70)));
    let both = load_error(&over_both, EngineConfig::REFERENCE);
    let LoadError::OverLimit(over) = &both else {
        panic!("over both bounds: {both:?}");
    };
    assert_eq!((over.limit, over.needed), (Limit::ControlFrames, 71));
}

/// A module exactly at each verifier bound loads through `fprime::load` and
/// runs: a control nesting of 64 frames, and an operand stack peaking at 256
/// values.
///
/// The rows above show the loader refusing one past each bound. Fails if the
/// loader is built with a tighter bound than the reference embedder's, which
/// would refuse a module the reference embedder loads, and misreport why.
#[test]
fn a_module_at_both_verifier_bounds_loads_and_runs() {
    for (body, export) in [(nested(63), "deep"), (tall(256), "tall")] {
        let wasm = wat(&format!("(module {body})"));
        let mut session = Session::acquire();
        let mut instance = fprime::load(
            &mut session,
            &wasm,
            HostLog::recording(),
            EngineConfig::REFERENCE,
        )
        .unwrap_or_else(|e| panic!("`{export}` is at the bound and loads: {e}"))
        .start(BUDGET)
        .unwrap_or_else(|e| panic!("`{export}` has no start function to fail: {e}"));
        assert_eq!(instance.invoke(export, &[]), Ok(Outcome::Returned(None)), "{export}");
    }
}

/// A module inside both verifier bounds that the decoder refuses for want of
/// memory is reported as too much IR for the code pages it was given.
///
/// One page holds 64 functions of four IR words each; a 65th spills. Fails if
/// the elimination names a stack instead, or the page count is not the one
/// the load was given.
#[test]
fn a_module_within_both_bounds_that_runs_out_of_memory_is_too_much_ir() {
    let function = "(func (result i32) i32.const 1 i32.const 2 i32.add)";
    let one_page = EngineConfig { max_code_pages: 1, ..EngineConfig::REFERENCE };
    let error = load_error(&wat(&format!("(module {})", function.repeat(65))), one_page);
    let LoadError::CodePagesExhausted { pages, verdict } = &error else {
        panic!("out of code pages: {error:?}");
    };
    assert_eq!(*pages, 1);
    assert_eq!(verdict.err.err, ValidationError::AllocError(AllocError::OutOfMemory));
    assert!(
        error.to_string().starts_with(
            "the module does not load under the SpaceWasm interpreter: the interpreter's compiled \
             form of it does not fit the 1 IR code page the runner provides (the interpreter \
             reported AllocError(OutOfMemory) at byte "
        ),
        "{error}"
    );
    let mut session = Session::acquire();
    fprime::load(
        &mut session,
        &wat(&format!("(module {})", function.repeat(64))),
        HostLog::recording(),
        one_page,
    )
    .expect("64 functions fill the one page exactly");
}

/// A module the conformance check accepts and the decoder refuses for
/// something in its bytes the check should have caught is reported as a gap
/// in the check, with the decoder's verdict: a data segment that does not fit
/// linear memory, which the decoder writes while it decodes, and one at a
/// negative offset.
///
/// The segments on either side of the end of memory load, so the rows over it
/// are refused for where they are. Fails if one of these refusals is reported
/// as the decoder's plain verdict, which reads as though the check had refused
/// the module too, or if the segment bound moves.
#[test]
fn a_module_the_check_accepts_and_the_decoder_refuses_is_a_gap_in_the_check() {
    let refused_for = |body: &str, expected: ValidationError| {
        let wasm = wat(&format!("(module {body})"));
        assert!(check(&wasm).is_ok(), "the check accepts `{body}`");
        let error = load_error(&wasm, EngineConfig::REFERENCE);
        let LoadError::ConformanceGap(verdict) = &error else {
            panic!("a gap in the check for `{body}`: {error:?}");
        };
        assert_eq!(verdict.err.err, expected, "`{body}`");
        assert_eq!(
            error.to_string(),
            format!(
                "the module does not load under the SpaceWasm interpreter: {expected:?} at byte \
                 {}. infc's conformance check accepts this module, so this is a gap in that \
                 check; please report it at https://github.com/Inferara/inference/issues with \
                 the artifact",
                verdict.offset
            ),
            "`{body}`"
        );
    };
    let out_of_bounds = || ValidationError::MemoryError(MemoryError::OutOfBounds);
    for body in [
        r#"(memory 1) (data (i32.const 65536) "x")"#,
        r#"(memory 1) (data (i32.const 65535) "xy")"#,
        r#"(memory 1) (data (i32.const 70000) "x")"#,
        r#"(memory 1) (data (i32.const 65537) "")"#,
        r#"(memory 0) (data (i32.const 0) "x")"#,
    ] {
        refused_for(body, out_of_bounds());
    }
    refused_for(
        r#"(memory 1) (data (i32.const -1) "x")"#,
        ValidationError::InvalidNegativeMemOffset,
    );

    let mut session = Session::acquire();
    for body in
        [r#"(memory 1) (data (i32.const 65535) "x")"#, r#"(memory 1) (data (i32.const 65536) "")"#]
    {
        let wasm = wat(&format!("(module {body})"));
        fprime::load(&mut session, &wasm, HostLog::recording(), EngineConfig::REFERENCE)
            .unwrap_or_else(|e| panic!("`{body}` fits memory and loads: {e}"));
    }
}

/// A `memory.grow` the conformance check accepts is refused by the load's own
/// configuration, not missed by the check: the load tells the code builder to
/// refuse it, as `spacewasm_std` tells its own, and the check leaves that
/// choice to the embedder. So the refusal keeps the decoder's plain verdict,
/// wherever the instruction sits and whether or not the memory could grow.
///
/// Fails if a row is reported as a gap in the check, which would send its
/// reader to report the loader's intended behaviour as a bug, or if the
/// check starts refusing `memory.grow` itself.
#[test]
fn a_memory_grow_is_the_loads_own_refusal_not_a_gap_in_the_check() {
    for body in [
        r#"(memory 1) (func (export "grow") (result i32) (memory.grow (i32.const 0)))"#,
        r#"(memory 1 1) (func (export "grow") (result i32) (memory.grow (i32.const 1)))"#,
        r#"(memory 1 4) (func (export "grow") (result i32) (memory.grow (i32.const 1)))"#,
        r#"(memory 1 4) (func $start (drop (memory.grow (i32.const 1)))) (start $start)"#,
        r#"(memory 1 4)
           (func (export "f") (result i32) i32.const 7)
           (func (export "g") (result i32) (block (result i32) (memory.grow (i32.const 2))))"#,
    ] {
        let wasm = wat(&format!("(module {body})"));
        assert!(check(&wasm).is_ok(), "the check leaves `memory.grow` to the embedder: `{body}`");
        let error = load_error(&wasm, EngineConfig::REFERENCE);
        let LoadError::Decode(verdict) = &error else {
            panic!("the decoder's own refusal for `{body}`: {error:?}");
        };
        assert_eq!(verdict.err.err, ValidationError::IllegalMemoryGrow, "`{body}`");
        assert_eq!(
            error.to_string(),
            format!(
                "the module does not load under the SpaceWasm interpreter: IllegalMemoryGrow at \
                 byte {}",
                verdict.offset
            ),
            "`{body}`"
        );
    }
}

// ---------------------------------------------------------------------------
// The budget
// ---------------------------------------------------------------------------

/// `count` as a limited budget.
fn limited(count: usize) -> Fuel {
    Fuel::Limited(NonZeroUsize::new(count).expect("a budget of at least one instruction"))
}

/// A start function calling `rsleep`, three instructions with its return, and
/// an entry `f` calling `command`, five; the call is the start function's
/// second instruction and the entry's third. Measured against the interpreter
/// release this workspace pins.
const SHARED: &str = r#"(func $s (call $rsleep (i64.const 1)))
   (start $s)
   (func (export "f") (drop (call $command (i32.const 42) (i32.const 7))))"#;

/// The start function [`fprime::HostedModule::start`] runs and the call
/// [`HostedInstance::invoke`] makes share the start's one budget, and a line a
/// host logged stays logged when the run then runs out.
///
/// Eight instructions run both to the end and seven stop the entry just before
/// its return, after it called `command`; five stop it before that call, three
/// leave it nothing; two stop the start function after it called `rsleep`,
/// and one before. Fails if the start drops the caller's budget — a limit
/// ignored runs every row to the end — if the call is given a budget of its
/// own instead of what the start function left, if running out reports only
/// the call's share, or if the log loses or invents a line around it.
#[test]
fn the_start_function_and_the_entry_share_the_starts_budget() {
    let wasm = fprime_module(SHARED);
    let both = ["RSLEEP 1", "COMMAND 42 7"];
    for (fuel, ended, lines) in [
        (limited(8), Outcome::Returned(None), &both[..]),
        (limited(9), Outcome::Returned(None), &both[..]),
        (Fuel::Unbounded, Outcome::Returned(None), &both[..]),
        (limited(7), Outcome::OutOfFuel { budget: 7 }, &both[..]),
        (limited(5), Outcome::OutOfFuel { budget: 5 }, &both[..1]),
        (limited(3), Outcome::OutOfFuel { budget: 3 }, &both[..1]),
    ] {
        let mut session = Session::acquire();
        let log = HostLog::recording();
        let mut instance = fprime::load(&mut session, &wasm, log.clone(), EngineConfig::REFERENCE)
            .expect("the module loads")
            .start(fuel)
            .unwrap_or_else(|e| panic!("the start function runs within {fuel:?}: {e}"));
        assert_eq!(instance.invoke("f", &[]), Ok(ended), "{fuel:?}");
        assert_eq!(log.recorded(), lines, "{fuel:?}");
        assert_eq!(log.line_count(), lines.len(), "{fuel:?}");
        assert_eq!(instance.host_trap(), None, "no host stopped the run: {fuel:?}");
    }

    for (budget, lines) in [(2, &both[..1]), (1, &[][..])] {
        let mut session = Session::acquire();
        let log = HostLog::recording();
        let failure = fprime::load(&mut session, &wasm, log.clone(), EngineConfig::REFERENCE)
            .expect("the module loads")
            .start(limited(budget))
            .map(drop)
            .expect_err("the start function needs three instructions");
        assert_eq!(failure.error(), &StartError::OutOfFuel { budget }, "a budget of {budget}");
        assert_eq!(failure.host_trap(), None, "no host stopped the start function");
        assert_eq!(log.recorded(), lines, "a budget of {budget}");
    }
}

/// An entry that makes a host call and then runs past its budget ends out of
/// fuel, carrying the whole budget, with the call it made logged and no host
/// trap recorded — and given no budget, the same entry runs to its end.
///
/// The entry counts to a million, which takes several million instructions, so
/// a loader that stops passing the budget on returns instead of running out.
/// Fails if the budget is dropped on the way to the call, if running out
/// reports another number, if the host call made before it is lost from the
/// log or its count, or if running out is recorded as a host's trap.
#[test]
fn an_entry_that_runs_out_of_fuel_keeps_the_host_calls_it_made() {
    let wasm = fprime_module(
        r#"(func (export "go") (result i32) (local $i i32)
             (drop (call $command (i32.const 42) (i32.const 7)))
             (loop $again
               (local.set $i (i32.add (local.get $i) (i32.const 1)))
               (br_if $again (i32.lt_u (local.get $i) (i32.const 1000000))))
             (local.get $i))"#,
    );
    let mut session = Session::acquire();
    let log = HostLog::recording();
    let mut instance = fprime::load(&mut session, &wasm, log.clone(), EngineConfig::REFERENCE)
        .expect("the module loads")
        .start(limited(1_000))
        .expect("the module has no start function");
    assert_eq!(instance.invoke("go", &[]), Ok(Outcome::OutOfFuel { budget: 1_000 }));
    assert_eq!(log.recorded(), ["COMMAND 42 7"]);
    assert_eq!(log.line_count(), 1);
    assert_eq!(instance.host_trap(), None);
    assert_eq!(
        instance.invoke("go", &[]),
        Ok(Outcome::OutOfFuel { budget: 1_000 }),
        "the engine is idle again, and a second call is given the same budget"
    );
    assert_eq!(log.line_count(), 2);
    drop(instance);

    let log = HostLog::recording();
    let mut instance = fprime::load(&mut session, &wasm, log.clone(), EngineConfig::REFERENCE)
        .expect("the module loads")
        .start(Fuel::Unbounded)
        .expect("the module has no start function");
    assert_eq!(instance.invoke("go", &[]), Ok(Outcome::Returned(Some(Value::I32(1_000_000)))));
    assert_eq!(log.recorded(), ["COMMAND 42 7"]);
}

// ---------------------------------------------------------------------------
// The start function
// ---------------------------------------------------------------------------

/// `raw`, as the text arguments a command line carries.
fn texts(raw: &[&str]) -> Vec<String> {
    raw.iter().map(ToString::to_string).collect()
}

/// A load runs nothing: a caller can refuse an export the module does not
/// have and arguments its function cannot take, list the exports and measure
/// the IR, and no host has logged a line. The start function's line comes
/// with the start, and the call's after it.
///
/// This is the order `infs run` takes, and the reason it can refuse a call
/// with nothing executed whatever the module declares. Fails if a load runs
/// the start function — its line would be logged before any refusal — if the
/// start or the call stops logging, or if the two log out of order.
#[test]
fn a_hosted_module_runs_nothing_before_it_is_started() {
    let wasm = fprime_module(
        r#"(data (i32.const 16) "start") (data (i32.const 32) "go")
           (func $s (call $message (i32.const 16) (i32.const 5)))
           (start $s)
           (func (export "go") (call $message (i32.const 32) (i32.const 2)))
           (func (export "add") (param i32 i32) (result i32)
             (i32.add (local.get 0) (local.get 1)))"#,
    );
    let mut session = Session::acquire();
    let log = HostLog::recording();
    let module = fprime::load(&mut session, &wasm, log.clone(), EngineConfig::REFERENCE)
        .expect("the module loads");
    assert_eq!(log.line_count(), 0, "the load ran the start function: {:?}", log.recorded());

    let missing = module.module().function("missing");
    assert!(matches!(missing, Err(InvokeError::NoSuchExport { .. })), "{missing:?}");
    let add = module.module().function("add").expect("`add` exists");
    let short = coerce_arguments(&add, &texts(&["1"]));
    assert!(matches!(short, Err(ArgumentError::Count { .. })), "{short:?}");
    let word = coerce_arguments(&add, &texts(&["x", "1"]));
    assert!(matches!(word, Err(ArgumentError::NotAnInteger { position: 1, .. })), "{word:?}");
    assert_eq!(module.module().exported_functions().len(), 3, "`peek`, `go` and `add`");
    let stats = ir_stats(module.module(), wasm.len());
    assert_eq!(log.line_count(), 0, "reading the loaded module ran it: {:?}", log.recorded());

    let mut instance = module.start(BUDGET).expect("the start function returns");
    assert_eq!(log.recorded(), ["MESSAGE start"]);
    assert_eq!(instance.invoke("go", &[]), Ok(Outcome::Returned(None)));
    assert_eq!(log.recorded(), ["MESSAGE start", "MESSAGE go"]);
    assert_eq!(instance.log().line_count(), 2, "the instance logs to the load's log");
    assert_eq!(ir_stats(instance.module(), wasm.len()), stats);
}

/// How the start of `wasm`, which loads against the reference hosts, failed,
/// with the lines the hosts logged.
fn start_failure(wasm: &[u8], config: EngineConfig) -> (HostedStartError, Vec<String>) {
    let mut session = Session::acquire();
    let log = HostLog::recording();
    let failure = fprime::load(&mut session, wasm, log.clone(), config)
        .expect("the module loads, since a load runs nothing")
        .start(BUDGET)
        .map(drop)
        .expect_err("the start function does not return");
    (failure, log.recorded())
}

/// The report of a start failure's trap, which a trap has.
fn trapped(failure: &HostedStartError) -> TrapReport {
    failure.trap_report().unwrap_or_else(|| panic!("a trap has a report: {failure:?}"))
}

/// The explanation of every start function's trap but an exhausted stack's
/// and a defect's.
const NOT_FROM_THIS_COMPILER: &str =
    "Inference never emits a start function, so the code that trapped did not come from this \
     compiler.";

/// A start function a host stopped is reported by what the host recorded, as
/// a call's trap is: the start failure keeps the detail, and its report names
/// the start function and the host's fact — but not the host's remedy, which
/// is about the Inference declaration of a host, while this compiler never
/// emits a start function.
///
/// Fails if the detail is lost when the module goes with its failed start —
/// the report would fall back to "a host function stopped it" — if the report
/// names an export instead of the start function, if a detail reaches the
/// report of a trap that was not the host's, or if a start function's trap is
/// explained as a call's.
#[test]
fn a_start_function_a_host_stopped_is_reported_with_the_hosts_detail() {
    let (panicked, log) = start_failure(
        &fprime_module(
            r#"(data (i32.const 0) "boom")
               (func $s (call $panic (i32.const 0) (i32.const 4) (i32.const 3)))
               (start $s)"#,
        ),
        EngineConfig::REFERENCE,
    );
    assert_eq!(panicked.error(), &StartError::Trapped(TrapReason::Host));
    assert_eq!(panicked.host_trap(), Some(HostTrap::Panic { host: host("fprime_core.panic") }));
    assert_eq!(log, ["PANIC boom:3"]);
    let report = trapped(&panicked);
    assert_eq!(
        report.headline(),
        "the module's start function trapped: it called `fprime_core.panic`, which always stops \
         the program; its message is the PANIC line above (Host)."
    );
    assert_eq!(report.explanation("`embedder`").as_deref(), Some(NOT_FROM_THIS_COMPILER));
    assert_eq!(panicked.to_string(), "the module's start function trapped: Host");

    let (short, log) = start_failure(
        &fprime_module(
            r#"(func $s (drop (call $telemetry (i32.const 3) (i32.const 100) (i32.const 8)
                                              (i32.const 0) (i32.const 4))))
               (start $s)"#,
        ),
        EngineConfig::REFERENCE,
    );
    let detail = HostTrap::ShortTime { host: host("fprime_core.telemetry"), time_len: 8 };
    assert_eq!(short.host_trap(), Some(detail));
    assert!(log.is_empty(), "{log:?}");
    let report = trapped(&short);
    assert_eq!(
        report.headline(),
        "the module's start function trapped: `fprime_core.telemetry` writes an 11-byte F Prime \
         time, and `time_len` is 8 (Host)."
    );
    assert_eq!(report.explanation("`embedder`").as_deref(), Some(NOT_FROM_THIS_COMPILER));

    let (outside, _) = start_failure(
        &fprime_module(r#"(func $s (call $message (i32.const 70000) (i32.const 5))) (start $s)"#),
        EngineConfig::REFERENCE,
    );
    assert_eq!(
        trapped(&outside).headline(),
        "the module's start function trapped: `fprime_core.message` was given 5 bytes at address \
         70000, outside the module's 65536-byte linear memory (Host)."
    );

    let (unreachable, log) = start_failure(
        &fprime_module(r#"(func $s unreachable) (start $s)"#),
        EngineConfig::REFERENCE,
    );
    assert_eq!(unreachable.error(), &StartError::Trapped(TrapReason::Unreachable));
    assert_eq!(unreachable.host_trap(), None);
    assert!(log.is_empty(), "{log:?}");
    assert_eq!(
        trapped(&unreachable).headline(),
        "the module's start function trapped: a runtime check failed (Unreachable)."
    );
}

/// A start failure that is no trap has no trap report: a start function that
/// ran out of fuel, having made a host call first.
///
/// Fails if a report is made up for a failure that is not a trap, which would
/// word an exhausted budget as a trap of some reason.
#[test]
fn a_start_failure_that_is_no_trap_has_no_trap_report() {
    let (spun, log) = start_failure(
        &fprime_module(
            r#"(func $s (local $i i32)
                 (call $rsleep (i64.const 1))
                 (loop $again
                   (local.set $i (i32.add (local.get $i) (i32.const 1)))
                   (br_if $again (i32.lt_u (local.get $i) (i32.const 1000000)))))
               (start $s)"#,
        ),
        EngineConfig::REFERENCE,
    );
    assert_eq!(spun.error(), &StartError::OutOfFuel { budget: 1_000_000 });
    assert_eq!(spun.host_trap(), None);
    assert_eq!(spun.trap_report(), None);
    assert_eq!(log, ["RSLEEP 1"]);
}

/// The report of a start function whose frames do not fit the stack names
/// the words the module was loaded with, as a call's report does, and says
/// the start function's call chain needs more — without a call's word on
/// recursion in Inference, which never emits a start function.
///
/// `$s` keeps 2,000 words of locals: more than the reference embedder's
/// 1,024, less than 4,096. Fails if the start failure reports another stack
/// than the one its load was given, or if the start function is given
/// another.
#[test]
fn a_start_function_that_overflows_the_stack_is_reported_with_the_loads_stack() {
    let wasm = fprime_module(&format!(
        r#"(func $s (local {}) (local.set 1999 (i32.const 7))) (start $s)"#,
        "i32 ".repeat(2000)
    ));
    for (stack_words, given) in [
        (512, "512 words of call stack,"),
        (1024, "1024 words of call stack, as spacewasm_std does,"),
    ] {
        let config = EngineConfig { stack_words, ..EngineConfig::REFERENCE };
        let (failure, _) = start_failure(&wasm, config);
        assert_eq!(failure.error(), &StartError::Trapped(TrapReason::StackOverflow));
        let report = trapped(&failure);
        assert_eq!(
            report.headline(),
            "the module's start function trapped: the interpreter's call stack is full \
             (StackOverflow)."
        );
        assert_eq!(
            report.explanation("`embedder`"),
            Some(format!(
                "`embedder` gives the interpreter {given} and the start function's call chain \
                 needs more."
            ))
        );
    }
    let mut session = Session::acquire();
    let roomy = EngineConfig { stack_words: 4096, ..EngineConfig::REFERENCE };
    fprime::load(&mut session, &wasm, HostLog::recording(), roomy)
        .expect("the module loads")
        .start(BUDGET)
        .expect("2,000 words fit 4,096");
}
