//! A compiled program running against the host functions it imports.
//!
//! The decode and differential sweeps register no host, so there an artifact
//! binding `use { f } from host::<module>;` is at most shown to be refused —
//! which says the imports are there and nothing about whether they are the
//! program's. This module registers the F´ (F Prime) reference host set, at
//! the signatures the reference embedder registers it with, as hosts that
//! record every call and answer with values chosen per row, then runs the F´
//! fixture: `report(channel)` announces itself through `message`, sends
//! `command(1, channel)`, downlinks a four-byte value through `telemetry` under
//! the id `command` answered with, and asks `clock_ms()` only when telemetry
//! answered 0. The answers are distinct numbers, so a value can be followed
//! across the boundary: the acknowledgement `command` returned has to arrive as
//! `telemetry`'s `id`, and the clock reading has to come back as the program's
//! result. Each is wider than the half of its type a narrowing would keep, so a
//! value cut short at the boundary and widened back cannot pass for itself.
//!
//! An array argument reaches a host as the address of the caller's buffer, and
//! that address is wherever the compiler laid the buffer out in the caller's
//! frame — a layout decision, not something the program says. So a recording
//! host reads each buffer through its address and the length passed after it,
//! and records the bytes: no recorded argument is an address, and a row pins
//! what the program handed over rather than where it happened to sit.
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

/// One host of the F´ reference set, as the reference embedder registers it.
struct ReferenceHost {
    module: &'static str,
    field: &'static str,
    /// The parameter and result types, in the interpreter's alphabet.
    signature: (&'static str, &'static str),
    /// The parameters that carry a guest address. The parameter after each is
    /// the length of the buffer at that address.
    buffers: &'static [usize],
}

/// The F´ reference host set: five functions under `fprime_core` and one under
/// `env`, at the signatures `spacewasm_std`, the reference embedder in the
/// SpaceWasm repository, registers them with.
const REFERENCE_HOSTS: [ReferenceHost; 6] = [
    ReferenceHost {
        module: "fprime_core",
        field: "panic",
        signature: ("iii", ""),
        buffers: &[0],
    },
    ReferenceHost {
        module: "fprime_core",
        field: "rsleep",
        signature: ("I", ""),
        buffers: &[],
    },
    ReferenceHost {
        module: "fprime_core",
        field: "command",
        signature: ("ii", "i"),
        buffers: &[],
    },
    ReferenceHost {
        module: "fprime_core",
        field: "message",
        signature: ("ii", ""),
        buffers: &[0],
    },
    ReferenceHost {
        module: "fprime_core",
        field: "telemetry",
        signature: ("iiiii", "i"),
        buffers: &[1, 3],
    },
    ReferenceHost {
        module: "env",
        field: "clock_ms",
        signature: ("", "I"),
        buffers: &[],
    },
];

/// What `report` is invoked with in every row, and what it sends as
/// `command`'s argument.
///
/// Not 1, which is the opcode it sends beside it, so the two cannot trade
/// places unseen.
const CHANNEL: i32 = 3;

/// What `command` acknowledges with in a row that accepts, and so the id
/// `report` downlinks its telemetry under.
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

/// One argument as a recording host read it.
#[derive(Debug, Clone, PartialEq)]
enum Arg {
    /// A value, as the host was handed it.
    Value(Value),
    /// The bytes of a guest buffer, read through the address the host was
    /// handed and the length after it.
    Bytes(Vec<u8>),
}

/// One host call as the host saw it: the module and field it was registered
/// under, and the arguments it was handed.
type Call = (&'static str, &'static str, Vec<Arg>);

/// The list every recording host of one row appends to, in call order.
type Calls = Rc<RefCell<Vec<Call>>>;

/// An `i32` argument as a recording host records it.
fn int(value: i32) -> Arg {
    Arg::Value(Value::I32(value))
}

/// The `len` bytes at `address` in the guest's memory.
///
/// Both arrive as `i32`s and are read unsigned, as WebAssembly addresses its
/// memory.
fn guest_bytes(engine: &spacewasm::Engine, address: i32, len: i32) -> Vec<u8> {
    engine
        .memory
        .load(address.cast_unsigned() as usize, len.cast_unsigned() as usize)
        .expect("the program hands a host a buffer inside its memory")
        .to_vec()
}

/// `host` registered with `params` and `result` spelled in the interpreter's
/// alphabet, recording each call in `calls` and answering `reply`.
fn recording(
    calls: &Calls,
    host: &ReferenceHost,
    (params, result): (&str, &str),
    reply: HostFunctionResult,
) -> HostFunction {
    let calls = Rc::clone(calls);
    let (module, field, buffers) = (host.module, host.field, host.buffers);
    HostFunction::try_new(
        HostName::try_from_str(field).expect("the row's field name is registrable"),
        HostValList::try_new(params).expect("the row's parameter types are registrable"),
        HostValList::try_new(result).expect("the row's result type is registrable"),
        move |engine: &mut spacewasm::Engine, args: &[Value]| {
            let read = args
                .iter()
                .enumerate()
                .map(|(position, arg)| {
                    if buffers.contains(&position) {
                        let (Value::I32(address), Some(Value::I32(len))) =
                            (arg, args.get(position + 1))
                        else {
                            panic!(
                                "`{module}.{field}` takes a buffer as an i32 address and the \
                                 i32 length after it, got {args:?}"
                            );
                        };
                        Arg::Bytes(guest_bytes(engine, *address, *len))
                    } else {
                        Arg::Value(*arg)
                    }
                })
                .collect();
            calls.borrow_mut().push((module, field, read));
            reply
        },
    )
    .expect("the row's host function is registrable")
}

/// What the F´ program's answering hosts answer in one row, and the one host,
/// if any, registered at a signature other than its reference one. `panic`,
/// `rsleep` and `message` return nothing, and answer by returning.
struct Fprime {
    command: HostFunctionResult,
    telemetry: HostFunctionResult,
    clock_ms: HostFunctionResult,
    mismatched: Option<(&'static str, (String, String))>,
}

impl Fprime {
    /// Every host registered at its reference signature, answering so that
    /// `report` reaches all four it calls: [`ACKNOWLEDGEMENT`], telemetry
    /// accepted with 0, and [`CLOCK_READING`].
    fn accepting() -> Self {
        Self {
            command: ControlFlow::Continue(Some(Value::I32(ACKNOWLEDGEMENT))),
            telemetry: ControlFlow::Continue(Some(Value::I32(0))),
            clock_ms: ControlFlow::Continue(Some(Value::I64(CLOCK_READING))),
            mismatched: None,
        }
    }

    /// What the host registered as `field` answers.
    fn reply(&self, field: &str) -> HostFunctionResult {
        match field {
            "command" => self.command,
            "telemetry" => self.telemetry,
            "clock_ms" => self.clock_ms,
            _ => ControlFlow::Continue(None),
        }
    }

    /// The host set an F´ embedder registers: [`REFERENCE_HOSTS`], one host
    /// module per module name.
    fn hosts(&self, session: &SpaceWasmSession, calls: &Calls) -> spacewasm::Vec<HostModule> {
        let modules = ["fprime_core", "env"].map(|module| {
            let functions = REFERENCE_HOSTS
                .iter()
                .filter(|host| host.module == module)
                .map(|host| {
                    let signature = match &self.mismatched {
                        Some((field, (params, result))) if *field == host.field => {
                            (params.as_str(), result.as_str())
                        }
                        _ => host.signature,
                    };
                    recording(calls, host, signature, self.reply(host.field))
                })
                .collect();
            host_module(session, module, functions, Vec::new())
                .expect("the module name is registrable")
        });
        host_set(session, modules.into()).expect("the host set allocates")
    }
}

/// `message`'s call as `report` makes it: the six bytes of `report`, and their
/// length.
fn announcement() -> Call {
    ("fprime_core", "message", vec![Arg::Bytes(b"report".to_vec()), int(6)])
}

/// `command`'s call as `report` makes it: opcode 1, with [`CHANNEL`].
fn command_call() -> Call {
    ("fprime_core", "command", vec![int(1), int(CHANNEL)])
}

/// `telemetry`'s call as `report` makes it once `command` answered
/// [`ACKNOWLEDGEMENT`]: that answer as the id, then the eleven-byte time — the
/// bytes 1 to 11 the program put there, since the recording host writes
/// nothing into it — and the four-byte value 21, each followed by its length.
///
/// The time is not zero because most of the guest's memory is: a host handed
/// an address into any other untouched stretch would read eleven zeros too, so
/// only bytes the program chose show that the host was handed its buffer.
fn downlink() -> Call {
    (
        "fprime_core",
        "telemetry",
        vec![
            int(ACKNOWLEDGEMENT),
            Arg::Bytes((1..=11).collect()),
            int(11),
            Arg::Bytes(vec![21, 0, 0, 0]),
            int(4),
        ],
    )
}

/// The fixture decoded at the reference embedder's verifier bounds and IR page
/// budget, with the tier's 65,536-word stack, against `hosts`.
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
/// which order, and with what: `telemetry`'s id is the acknowledgement
/// `command` answered, which only the program threading one call's result into
/// the next can put there, and the result is the clock reading, which only the
/// `clock_ms` host knows. The buffers are compared by their bytes, which shows
/// an array argument arriving as the address of the caller's own buffer: the
/// host finds the program's text, time and value there, the time in the
/// `let mut` slot a host may write. Fails if the calls reach the
/// wrong host function, if an argument or a result is dropped, re-typed or
/// narrowed at the boundary, if a buffer's address stops leading to its bytes,
/// or if the program's result stops being the host's answer.
#[test]
fn the_host_answers_travel_through_the_program() {
    let mut session = SpaceWasmSession::acquire();
    let calls = Calls::default();
    let hosts = Fprime::accepting().hosts(&session, &calls);
    let mut module = loaded(&mut session, hosts);

    assert_eq!(
        module.invoke("report", &[Value::I32(CHANNEL)], FUEL),
        Outcome::Value(Some(Value::I64(CLOCK_READING))),
        "the program returns the clock reading once telemetry is accepted"
    );
    assert_eq!(
        *calls.borrow(),
        vec![announcement(), command_call(), downlink(), ("env", "clock_ms", vec![])],
        "each host function is called once, in program order, with the acknowledgement \
         `command` answered as telemetry's id"
    );
}

/// A host answer decides which branch the program takes.
///
/// The same bytes with telemetry refused: the program returns zero and never
/// asks the clock. Fails if a host function's answer stops reaching the
/// condition it feeds — the row above alone would pass for a program that
/// called its hosts unconditionally.
#[test]
fn a_refused_telemetry_skips_the_clock() {
    let mut session = SpaceWasmSession::acquire();
    let calls = Calls::default();
    let hosts = Fprime {
        telemetry: ControlFlow::Continue(Some(Value::I32(1))),
        ..Fprime::accepting()
    }
    .hosts(&session, &calls);
    let mut module = loaded(&mut session, hosts);

    assert_eq!(
        module.invoke("report", &[Value::I32(CHANNEL)], FUEL),
        Outcome::Value(Some(Value::I64(0))),
        "refused telemetry returns zero"
    );
    assert_eq!(
        *calls.borrow(),
        vec![announcement(), command_call(), downlink()],
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

/// `params` with its first parameter the other integer type — `i32` for an
/// `i64`, `i64` for an `i32` — and as many parameters as before, or `None` for
/// a host that takes none.
fn first_param_flipped(params: &str) -> Option<String> {
    let mut rest = params.chars();
    let flipped = match rest.next()? {
        'i' => 'I',
        'I' => 'i',
        other => panic!("the reference set passes only integers, got `{other}` in `{params}`"),
    };
    Some(std::iter::once(flipped).chain(rest).collect())
}

/// Every host of the reference set registered with a wrong signature is refused
/// as a mismatch, whichever half of the signature is wrong and whether it is
/// wrong in how many types it lists or in which.
///
/// Every host, because the program binds all six — `panic` and `rsleep`
/// included, though it never calls them — and the interpreter compares each
/// import's signature as it binds it; the row that loads shows each import is
/// exactly its host's reference signature, and this one shows the comparison
/// is made for each. Both halves, because the interpreter compares the
/// parameters and the result separately and a binder that compared only one
/// would bind a host the program cannot call. The other five hosts are
/// registered correctly in each decode, so the mismatched one is the only thing
/// it can object to.
///
/// Three rows per host: a parameter too many, another result, and the first
/// parameter at the other integer type with the count unchanged, which a binder
/// comparing only how many parameters there are would bind, handing the host a
/// value of the wrong width. `clock_ms` takes no parameters, so it has no third
/// row: its another-result row, an `i32` where it returns an `i64`, already
/// flips a type at an unchanged count. That is seventeen refusals, and the
/// count is asserted, so a row cannot drop out unseen. Fails if any mismatch
/// starts binding, or is reported as the missing import it is not.
#[test]
fn a_host_with_the_wrong_signature_is_refused_as_a_mismatch() {
    let mut session = SpaceWasmSession::acquire();
    let mut refused = 0;
    for host in &REFERENCE_HOSTS {
        let (params, result) = host.signature;
        let another_result = if result == "i" { "I" } else { "i" };
        let mut rows = vec![
            ("a parameter too many", (format!("{params}i"), result.to_string())),
            ("another result", (params.to_string(), another_result.to_string())),
        ];
        if let Some(flipped) = first_param_flipped(params) {
            rows.push((
                "its first parameter the other integer type",
                (flipped, result.to_string()),
            ));
        }
        for (label, signature) in rows {
            let calls = Calls::default();
            let hosts = Fprime {
                mismatched: Some((host.field, signature)),
                ..Fprime::accepting()
            }
            .hosts(&session, &calls);
            assert_eq!(
                refusal(&mut session, hosts),
                ValidationError::FunctionImportTypeMismatch,
                "`{}.{}` registered with {label}",
                host.module,
                host.field
            );
            refused += 1;
        }
    }
    assert_eq!(
        refused, 17,
        "three rows for each host that takes parameters, two for `clock_ms`"
    );
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
/// `command` answers with a trap, so `report` stops at its second host call.
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
    .hosts(&session, &calls);
    let mut module = loaded(&mut session, hosts);

    assert_eq!(
        module.invoke("report", &[Value::I32(CHANNEL)], FUEL),
        Outcome::Trap(TrapReason::Host)
    );
    assert_eq!(
        *calls.borrow(),
        vec![announcement(), command_call()],
        "the program stops at the host that trapped"
    );
}
