//! A compiled program running against the host functions it imports.
//!
//! The decode and differential sweeps register no host, so there an artifact
//! binding `use { f } from host::<module>;` is at most shown to be refused —
//! which says the imports are there and nothing about whether they are the
//! program's. This module registers hosts that record every call and answer
//! with values chosen per row, then runs the F´ fixture: `report(channel)`
//! sends `command(channel)`, hands the acknowledgement to
//! `telemetry(channel, ack)`, and asks `clock_ms()` only when telemetry was
//! accepted. The answers are distinct numbers, so a value can be followed
//! across the boundary: the acknowledgement `command` returned has to arrive as
//! `telemetry`'s second argument, and the clock reading has to come back as the
//! program's result. Each is wider than the half of its type a narrowing would
//! keep, so a value cut short at the boundary and widened back cannot pass for
//! itself.
//!
//! The recording hosts are closures over one shared `Rc<RefCell<..>>`, which
//! the interpreter accepts because it asks no host closure to be `Send`. A host
//! function is never `Send` itself — it boxes its closure as a plain `dyn Fn` —
//! so the compiler keeps the list on the row's own thread. The session each
//! row holds guards the interpreter's allocator, not this list.

use std::cell::RefCell;
use std::ops::ControlFlow;
use std::rc::Rc;
use std::sync::LazyLock;

use inference_tests::corpus::wasm_for_target;
use inference_wasm_codegen::Target;
use spacewasm::{
    HostFunction, HostFunctionBreak, HostFunctionResult, HostModule, HostName, HostValList,
    ParseError, TrapReason, ValidationError, Value,
};

use crate::support::{
    EMBEDDER_MAX_CONTROL_FRAMES, EMBEDDER_MAX_STACK_DEPTH, FUEL, LoadedModule, Outcome,
    SpaceWasmSession, decode_with, host_module, host_set,
};

/// The committed F´ fixture, read rather than copied so that this tier and the
/// codegen golden describe one program.
const FPRIME_SOURCE: &str = include_str!(
    "../../test_data/codegen/wasm/extern_import/host_import_fprime/host_import_fprime.inf"
);

/// The fixture compiled once at the SpaceWasm target, so every row below puts
/// the same bytes in front of the interpreter.
static FPRIME: LazyLock<Vec<u8>> =
    LazyLock::new(|| wasm_for_target(FPRIME_SOURCE, Target::SpaceWasm));

/// What `command` acknowledges with in a row that accepts.
///
/// Above 65,535, so an `i32` cut to sixteen bits on its way from the host to
/// `telemetry` — the width the interpreter writes its IR in — would arrive as
/// another number, where an acknowledgement as small as 7 survives the cut.
const ACKNOWLEDGEMENT: i32 = 70_007;

/// What `clock_ms` reads in a row that accepts: an epoch-milliseconds reading
/// of the kind a flight clock gives.
///
/// Above `u32::MAX`. The interpreter's stack holds an `i64` as two 32-bit
/// words, so a result of which only the low word reached the guest would come
/// back as another number, where a reading under 2^32 survives unchanged.
const CLOCK_READING: i64 = 1_700_000_000_123;

/// One host call as the host saw it: the module and field it was registered
/// under, and the arguments it was handed.
type Call = (&'static str, &'static str, Vec<Value>);

/// The list every recording host of one row appends to, in call order.
type Calls = Rc<RefCell<Vec<Call>>>;

/// A host function registered as `module.field`, with `params` and `result`
/// spelled in the interpreter's alphabet, that records each call in `calls`
/// and answers `reply`.
fn recording(
    calls: &Calls,
    (module, field): (&'static str, &'static str),
    (params, result): (&str, &str),
    reply: HostFunctionResult,
) -> HostFunction {
    let calls = Rc::clone(calls);
    HostFunction::try_new(
        HostName::try_from_str(field).expect("the row's field name is registrable"),
        HostValList::try_new(params).expect("the row's parameter types are registrable"),
        HostValList::try_new(result).expect("the row's result type is registrable"),
        move |_: &mut spacewasm::Engine, args: &[Value]| {
            calls.borrow_mut().push((module, field, args.to_vec()));
            reply
        },
    )
    .expect("the row's host function is registrable")
}

/// What each of the F´ program's three host functions answers in one row, and
/// the signature `clock_ms` is registered with.
struct Fprime {
    command: HostFunctionResult,
    telemetry: HostFunctionResult,
    clock_ms: HostFunctionResult,
    clock_ms_signature: (&'static str, &'static str),
}

impl Fprime {
    /// Every host function registered as the program declares it, answering
    /// so that `report` reaches all three: [`ACKNOWLEDGEMENT`], accepted
    /// telemetry, and [`CLOCK_READING`].
    fn accepting() -> Self {
        Self {
            command: ControlFlow::Continue(Some(Value::I32(ACKNOWLEDGEMENT))),
            telemetry: ControlFlow::Continue(Some(Value::I32(1))),
            clock_ms: ControlFlow::Continue(Some(Value::I64(CLOCK_READING))),
            clock_ms_signature: ("", "I"),
        }
    }

    /// The host set an F´ embedder registers: `command` and `telemetry` under
    /// `fprime_core`, `clock_ms` under `env`.
    fn hosts(&self, calls: &Calls) -> spacewasm::Vec<HostModule> {
        let fprime_core = vec![
            recording(calls, ("fprime_core", "command"), ("i", "i"), self.command),
            recording(calls, ("fprime_core", "telemetry"), ("ii", "i"), self.telemetry),
        ];
        let env =
            vec![recording(calls, ("env", "clock_ms"), self.clock_ms_signature, self.clock_ms)];
        host_set(vec![
            host_module("fprime_core", fprime_core, Vec::new())
                .expect("the module name is registrable"),
            host_module("env", env, Vec::new()).expect("the module name is registrable"),
        ])
    }
}

/// The fixture decoded under the reference embedder configuration against
/// `hosts`.
fn decoded(
    session: &mut SpaceWasmSession,
    hosts: spacewasm::Vec<HostModule>,
) -> Result<LoadedModule<'_>, ParseError> {
    decode_with::<EMBEDDER_MAX_CONTROL_FRAMES, EMBEDDER_MAX_STACK_DEPTH>(session, &FPRIME, hosts)
}

/// The fixture loaded against `hosts`, which the row has built to satisfy it.
fn loaded(session: &mut SpaceWasmSession, hosts: spacewasm::Vec<HostModule>) -> LoadedModule<'_> {
    decoded(session, hosts)
        .unwrap_or_else(|e| panic!("the fixture loads against the hosts it imports: {e:?}"))
}

/// The verdict the interpreter reaches on the fixture against `hosts`, which
/// the row has built to be refused.
fn refusal(session: &mut SpaceWasmSession, hosts: spacewasm::Vec<HostModule>) -> ValidationError {
    decoded(session, hosts)
        .err()
        .unwrap_or_else(|| panic!("the fixture loaded against a host set it must be refused by"))
        .err
        .err
}

/// The values the hosts answer travel through the program and back out.
///
/// The recording is compared whole, so it pins which host functions ran, in
/// which order, and with what: `telemetry`'s second argument is the
/// acknowledgement `command` answered, which only the program threading one
/// call's result into the next can put there, and the result is the clock
/// reading, which only the `clock_ms` host knows. Fails if the calls reach the
/// wrong host function, if an argument or a result is dropped, re-typed or
/// narrowed at the boundary, or if the program's result stops being the
/// host's answer.
#[test]
fn the_host_answers_travel_through_the_program() {
    let mut session = SpaceWasmSession::acquire();
    let calls = Calls::default();
    let hosts = Fprime::accepting().hosts(&calls);
    let mut module = loaded(&mut session, hosts);

    assert_eq!(
        module.invoke("report", &[Value::I32(1)], FUEL),
        Outcome::Value(Some(Value::I64(CLOCK_READING))),
        "the program returns the clock reading once telemetry is accepted"
    );
    assert_eq!(
        *calls.borrow(),
        vec![
            ("fprime_core", "command", vec![Value::I32(1)]),
            ("fprime_core", "telemetry", vec![Value::I32(1), Value::I32(ACKNOWLEDGEMENT)]),
            ("env", "clock_ms", vec![]),
        ],
        "each host function is called once, in program order, with the acknowledgement \
         `command` answered as telemetry's value"
    );
}

/// A host answer decides which branch the program takes.
///
/// The same bytes with telemetry refused: the program returns zero and never
/// asks the clock. Fails if a host function's answer stops reaching the
/// condition it feeds — the row above alone would pass for a program that
/// called all three hosts unconditionally.
#[test]
fn a_refused_telemetry_skips_the_clock() {
    let mut session = SpaceWasmSession::acquire();
    let calls = Calls::default();
    let hosts = Fprime {
        telemetry: ControlFlow::Continue(Some(Value::I32(0))),
        ..Fprime::accepting()
    }
    .hosts(&calls);
    let mut module = loaded(&mut session, hosts);

    assert_eq!(
        module.invoke("report", &[Value::I32(1)], FUEL),
        Outcome::Value(Some(Value::I64(0))),
        "refused telemetry returns zero"
    );
    assert_eq!(
        *calls.borrow(),
        vec![
            ("fprime_core", "command", vec![Value::I32(1)]),
            ("fprime_core", "telemetry", vec![Value::I32(1), Value::I32(ACKNOWLEDGEMENT)]),
        ],
        "`clock_ms` is not called once telemetry is refused"
    );
}

/// Without the hosts it imports, the program does not load, and the verdict is
/// the one naming an import nothing supplied.
///
/// Fails if the variant changes, which is the day the harness's decode-failure
/// report — worded against these two verdicts — needs reading again.
#[test]
fn without_its_hosts_the_program_is_refused_for_a_missing_import() {
    let mut session = SpaceWasmSession::acquire();
    assert_eq!(
        refusal(&mut session, spacewasm::Vec::zero()),
        ValidationError::FunctionImportNotFound
    );
}

/// A host registered under the right names with the wrong signature is refused
/// as a mismatch, whichever half of the signature is wrong.
///
/// Both halves, because the interpreter compares the parameters and the result
/// separately and a binder that compared only one would bind a host the
/// program cannot call. The two F´ `fprime_core` hosts are registered
/// correctly in both, so `clock_ms` is the only thing each decode can object
/// to. Fails if either mismatch starts binding, or is reported as the missing
/// import it is not.
#[test]
fn a_host_with_the_wrong_signature_is_refused_as_a_mismatch() {
    let mut session = SpaceWasmSession::acquire();
    for (label, signature) in [
        ("a parameter too many", ("i", "I")),
        ("the wrong result type", ("", "i")),
    ] {
        let calls = Calls::default();
        let hosts = Fprime {
            clock_ms_signature: signature,
            ..Fprime::accepting()
        }
        .hosts(&calls);
        assert_eq!(
            refusal(&mut session, hosts),
            ValidationError::FunctionImportTypeMismatch,
            "`clock_ms` registered with {label}"
        );
    }
}

/// The artifact is conformant: the target's checker accepts the very bytes the
/// interpreter needs hosts to load.
///
/// The checker asks whether an embedder *can* load the module, which includes
/// registering the hosts it imports; the decode sweep is what registers none.
/// Fails if the checker starts treating a host import as a conformance defect,
/// which would refuse every program this target's host-import support exists
/// for.
#[test]
fn the_program_is_conformant_as_built() {
    let verdict = inference_target_conformance::spacewasm::check(&FPRIME);
    assert!(verdict.is_ok(), "the SpaceWasm checker refused the F´ artifact: {verdict:?}");
}

/// A host that traps makes the call trap, reported as a trap.
///
/// `command` answers with a trap, so `report` stops at its first host call.
/// Fails if a host's trap is reported as a harness fault — a panic — or as a
/// value, or if the program runs on past it to call `telemetry`.
#[test]
fn a_trapping_host_traps_the_call() {
    let mut session = SpaceWasmSession::acquire();
    let calls = Calls::default();
    let hosts = Fprime {
        command: ControlFlow::Break(HostFunctionBreak::Trap),
        ..Fprime::accepting()
    }
    .hosts(&calls);
    let mut module = loaded(&mut session, hosts);

    assert_eq!(module.invoke("report", &[Value::I32(1)], FUEL), Outcome::Trap(TrapReason::Host));
    assert_eq!(
        *calls.borrow(),
        vec![("fprime_core", "command", vec![Value::I32(1)])],
        "the program stops at the host that trapped"
    );
}
