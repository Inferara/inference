//! The deployment tier: real Inference source, compiled by the real compiler
//! path, uploaded to the real Soroban host and invoked.
//!
//! Everything above this file measures a piece. `wrappers` measures the
//! marshalling sequences against hand-assembled modules, `envelope` measures
//! what the host accepts of a module's shape, and the unit tests in
//! `core/stellar-abi` measure the rewrite against synthetic descriptors. None
//! of them compiles a program. This one starts at `.inf` source, runs code
//! generation at the Stellar target, the link and the Val-ABI rewrite — the
//! same three steps `infc --target stellar` runs — and then asks the host to
//! execute the result.
//!
//! # The transparency comparison
//!
//! Every fixture is answered twice: once by the default build under the
//! in-process `wasmtime` tier the rest of this repository uses, and once by the
//! Stellar build under the Soroban host. The two answers must be equal. That is
//! the assertion this tier exists for. Code generation reads no target, so the
//! two artifacts hold the same computation; if the wrapper did anything beyond
//! marshalling — read a different parameter, sign-extend where it should not,
//! keep a payload in the wrong half of the word — the two answers would part.
//! It is also what
//! makes "prove the default build, deploy the Stellar one" a claim about the
//! deployed artifact rather than about its ancestor.
//!
//! The default-build side reads its results as the type the fixture declares,
//! because `u32`, `i32` and `bool` are all one `i32` in an ordinary
//! WebAssembly signature and a raw result cannot say which it is. Only the
//! value is under test there. The *type* is under test on the Stellar side,
//! where the wrapper writes a tag and the host rejects a word whose tag and
//! body disagree.

use inference_tests::corpus::{
    codegen_for_target, compile_for_stellar, test_data_path, wasm_for_target,
};
use inference_wasm_codegen::Target;
use soroban_env_host::Val;
use wasmparser::ExternalKind;

use crate::support::{
    Decoded, bool_val, call, decode, describe, fn_call_events, host, i32_val, session, u32_val,
    upload, void_val,
};

/// One argument to a contract method, in the source's own terms.
///
/// The two sides encode it differently — a tagged 64-bit word for the host, a
/// plain `i32` for the default build — which is exactly the difference under
/// test, so the case table names the source value and each side encodes it.
#[derive(Clone, Copy, Debug)]
enum Arg {
    U32(u32),
    I32(i32),
    Bool(bool),
}

impl Arg {
    /// The tagged word a contract caller passes.
    fn as_val(self) -> Val {
        match self {
            Self::U32(v) => u32_val(v),
            Self::I32(v) => i32_val(v),
            Self::Bool(v) => bool_val(v),
        }
    }

    /// The plain value an ordinary WebAssembly caller passes.
    fn as_core(self) -> wasmtime::Val {
        match self {
            Self::U32(v) => wasmtime::Val::I32(v.cast_signed()),
            Self::I32(v) => wasmtime::Val::I32(v),
            Self::Bool(v) => wasmtime::Val::I32(i32::from(v)),
        }
    }
}

/// What a contract method answered, in the source's own terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Answer {
    U32(u32),
    I32(i32),
    Bool(bool),
    Void,
}

impl Answer {
    /// Reads a plain WebAssembly result list as the type this answer names.
    ///
    /// # Panics
    ///
    /// Panics if the result list does not have the shape the declared return
    /// type calls for — no results for a `()` (or omitted) return type, one
    /// `i32` otherwise.
    fn read_from(self, results: &[wasmtime::Val]) -> Self {
        if matches!(self, Self::Void) {
            assert!(results.is_empty(), "a unit method returned {results:?}");
            return Self::Void;
        }
        let [wasmtime::Val::I32(raw)] = results else {
            panic!("expected exactly one i32 result, got {results:?}")
        };
        match self {
            Self::U32(_) => Self::U32(raw.cast_unsigned()),
            Self::I32(_) => Self::I32(*raw),
            Self::Bool(_) => Self::Bool(*raw != 0),
            Self::Void => unreachable!("the unit case returned above"),
        }
    }
}

impl From<Decoded> for Answer {
    fn from(decoded: Decoded) -> Self {
        match decoded {
            Decoded::U32(v) => Self::U32(v),
            Decoded::I32(v) => Self::I32(v),
            Decoded::Bool(v) => Self::Bool(v),
            Decoded::Void => Self::Void,
            other => panic!("the contract returned a word outside the scalar set: {other:?}"),
        }
    }
}

/// One invocation of one contract method, with the answer the source says it
/// must give.
struct Case {
    /// The fixture file under `test_data/stellar`, without its extension.
    fixture: &'static str,
    method: &'static str,
    args: &'static [Arg],
    expect: Answer,
}

/// The scalar set in every position a contract method has.
///
/// Read down the `fixture` column: parameters with no answer, an answer with
/// no parameters, neither, both together, a boolean round trip, a boolean
/// computed from an unsigned argument, a negative integer round trip, the most
/// negative integer there is, two methods in one contract, the widest method
/// the host will dispatch to, a body that needs linear memory, a private
/// function sitting ahead of the exported ones, and `main` beside another
/// method.
const CASES: &[Case] = &[
    Case {
        fixture: "params_only",
        method: "record",
        args: &[Arg::U32(7), Arg::I32(-9), Arg::Bool(true)],
        expect: Answer::Void,
    },
    Case { fixture: "return_only", method: "answer", args: &[], expect: Answer::U32(42) },
    Case { fixture: "zero_parameter", method: "tick", args: &[], expect: Answer::Void },
    Case {
        fixture: "mixed",
        method: "choose",
        args: &[Arg::U32(1), Arg::I32(-5), Arg::Bool(true)],
        expect: Answer::I32(-5),
    },
    Case {
        fixture: "mixed",
        method: "choose",
        args: &[Arg::U32(1), Arg::I32(-5), Arg::Bool(false)],
        expect: Answer::I32(0),
    },
    Case {
        fixture: "bool_round_trip",
        method: "negate",
        args: &[Arg::Bool(true)],
        expect: Answer::Bool(false),
    },
    Case {
        fixture: "bool_round_trip",
        method: "negate",
        args: &[Arg::Bool(false)],
        expect: Answer::Bool(true),
    },
    Case {
        fixture: "bool_from_u32",
        method: "is_zero",
        args: &[Arg::U32(0)],
        expect: Answer::Bool(true),
    },
    Case {
        fixture: "bool_from_u32",
        method: "is_zero",
        args: &[Arg::U32(1)],
        expect: Answer::Bool(false),
    },
    Case {
        fixture: "negative_i32",
        method: "negate_i32",
        args: &[Arg::I32(7)],
        expect: Answer::I32(-7),
    },
    Case {
        fixture: "negative_i32",
        method: "negate_i32",
        args: &[Arg::I32(-7)],
        expect: Answer::I32(7),
    },
    Case {
        fixture: "i32_min",
        method: "passthrough",
        args: &[Arg::I32(i32::MIN)],
        expect: Answer::I32(i32::MIN),
    },
    Case {
        fixture: "i32_min",
        method: "passthrough",
        args: &[Arg::I32(i32::MAX)],
        expect: Answer::I32(i32::MAX),
    },
    Case {
        fixture: "u32_methods",
        method: "identity",
        args: &[Arg::U32(u32::MAX)],
        expect: Answer::U32(u32::MAX),
    },
    Case {
        fixture: "u32_methods",
        method: "identity",
        args: &[Arg::U32(0)],
        expect: Answer::U32(0),
    },
    Case {
        fixture: "u32_methods",
        method: "add",
        args: &[Arg::U32(2), Arg::U32(40)],
        expect: Answer::U32(42),
    },
    Case {
        fixture: "max_arity",
        method: "widest",
        args: WIDEST_ARGS,
        expect: Answer::U32(1217),
    },
    Case {
        fixture: "array_local",
        method: "pick",
        args: &[Arg::I32(0)],
        expect: Answer::U32(10),
    },
    Case {
        fixture: "array_local",
        method: "pick",
        args: &[Arg::I32(3)],
        expect: Answer::U32(40),
    },
    Case {
        fixture: "private_helper",
        method: "tripled",
        args: &[Arg::U32(4)],
        expect: Answer::U32(12),
    },
    Case {
        fixture: "private_helper",
        method: "plain",
        args: &[Arg::U32(9)],
        expect: Answer::U32(9),
    },
    Case { fixture: "entry_main", method: "main", args: &[], expect: Answer::I32(3) },
    Case {
        fixture: "entry_main",
        method: "beside",
        args: &[Arg::U32(9)],
        expect: Answer::U32(9),
    },
];

/// Thirty-two arguments, the most the host will dispatch, each distinct so a
/// wrapper that unwrapped the wrong one would answer with the wrong number
/// rather than with the right one by coincidence.
const WIDEST_ARGS: &[Arg] = &[
    Arg::U32(1000), Arg::U32(1007), Arg::U32(1014), Arg::U32(1021),
    Arg::U32(1028), Arg::U32(1035), Arg::U32(1042), Arg::U32(1049),
    Arg::U32(1056), Arg::U32(1063), Arg::U32(1070), Arg::U32(1077),
    Arg::U32(1084), Arg::U32(1091), Arg::U32(1098), Arg::U32(1105),
    Arg::U32(1112), Arg::U32(1119), Arg::U32(1126), Arg::U32(1133),
    Arg::U32(1140), Arg::U32(1147), Arg::U32(1154), Arg::U32(1161),
    Arg::U32(1168), Arg::U32(1175), Arg::U32(1182), Arg::U32(1189),
    Arg::U32(1196), Arg::U32(1203), Arg::U32(1210), Arg::U32(1217),
];

/// The parts of a compiled contract's shape this tier asserts on.
struct Shape {
    /// Every export, in section order.
    exports: Vec<(String, ExternalKind)>,
    /// Each declared linear memory, as its minimum and maximum page counts.
    memories: Vec<(u64, Option<u64>)>,
    /// How many data segments the module carries.
    data_segments: usize,
}

impl Shape {
    /// Reads the sections a contract's shape is asserted on out of its bytes.
    ///
    /// # Panics
    ///
    /// Panics if `wasm` is not a readable WebAssembly module.
    fn of(wasm: &[u8]) -> Self {
        let mut shape = Self { exports: Vec::new(), memories: Vec::new(), data_segments: 0 };
        for payload in wasmparser::Parser::new(0).parse_all(wasm) {
            match payload.expect("the compiled contract parses") {
                wasmparser::Payload::ExportSection(section) => {
                    for export in section {
                        let export = export.expect("a readable export entry");
                        shape.exports.push((export.name.to_owned(), export.kind));
                    }
                }
                wasmparser::Payload::MemorySection(section) => {
                    for memory in section {
                        let memory = memory.expect("a readable memory entry");
                        shape.memories.push((memory.initial, memory.maximum));
                    }
                }
                wasmparser::Payload::DataSection(section) => {
                    shape.data_segments +=
                        usize::try_from(section.count()).expect("a segment count fits in usize");
                }
                _ => {}
            }
        }
        shape
    }

    /// The names exported as functions, sorted.
    fn exported_functions(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .exports
            .iter()
            .filter(|(_, kind)| *kind == ExternalKind::Func)
            .map(|(name, _)| name.as_str())
            .collect();
        names.sort_unstable();
        names
    }

    /// Whether `name` is exported as `kind`.
    fn has_export(&self, name: &str, kind: ExternalKind) -> bool {
        self.exports.iter().any(|(n, k)| n == name && *k == kind)
    }
}

/// The fixture source behind a case.
#[must_use]
pub(crate) fn fixture_source(stem: &str) -> String {
    let path = test_data_path().join("stellar").join(format!("{stem}.inf"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read the fixture at {}: {e}", path.display()))
}

/// The cases of [`CASES`], grouped by fixture in table order, so each fixture
/// is compiled and uploaded once.
fn by_fixture() -> Vec<(&'static str, Vec<&'static Case>)> {
    let mut groups: Vec<(&'static str, Vec<&'static Case>)> = Vec::new();
    for case in CASES {
        if let Some((_, cases)) = groups.iter_mut().find(|(name, _)| *name == case.fixture) {
            cases.push(case);
        } else {
            groups.push((case.fixture, vec![case]));
        }
    }
    groups
}

/// Every fixture [`CASES`] invokes, once each, in table order: the fixture
/// list another module of this binary sweeps, read off the one table.
#[must_use]
pub(crate) fn fixture_names() -> Vec<&'static str> {
    by_fixture().into_iter().map(|(fixture, _)| fixture).collect()
}

/// Compiles `source` for Stellar, uploads it, and answers each case through
/// the host.
fn stellar_answers(source: &str, cases: &[&Case]) -> Vec<Answer> {
    let wasm = compile_for_stellar(source);
    let _guard = session();
    let host = host();
    let contract = upload(&host, &wasm).expect("the compiled contract uploads");
    cases
        .iter()
        .map(|case| {
            let args: Vec<Val> = case.args.iter().map(|arg| arg.as_val()).collect();
            let returned = call(&host, contract, case.method, &args).unwrap_or_else(|e| {
                panic!("{}::{} was refused: {}", case.fixture, case.method, describe(&e))
            });
            Answer::from(decode(returned))
        })
        .collect()
}

/// Compiles `source` for the default target and answers each case under
/// `wasmtime`, the in-process tier the rest of this repository executes with.
fn default_target_answers(source: &str, cases: &[&Case]) -> Vec<Answer> {
    let wasm = wasm_for_target(source, Target::Wasm32);
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, &wasm).expect("the default build is a valid module");
    let mut store = wasmtime::Store::new(&engine, ());
    let instance = wasmtime::Instance::new(&mut store, &module, &[])
        .expect("the default build instantiates");
    cases
        .iter()
        .map(|case| {
            let func = instance
                .get_func(&mut store, case.method)
                .unwrap_or_else(|| panic!("the default build exports no `{}`", case.method));
            let args: Vec<wasmtime::Val> = case.args.iter().map(|arg| arg.as_core()).collect();
            let mut results = vec![wasmtime::Val::I32(0); func.ty(&store).results().len()];
            func.call(&mut store, &args, &mut results).unwrap_or_else(|e| {
                panic!("{}::{} trapped under wasmtime: {e}", case.fixture, case.method)
            });
            case.expect.read_from(&results)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The round trip
// ---------------------------------------------------------------------------

#[test]
fn every_fixture_round_trips_through_the_soroban_host() {
    for (fixture, cases) in by_fixture() {
        let answers = stellar_answers(&fixture_source(fixture), &cases);
        for (case, answer) in cases.iter().zip(answers) {
            assert_eq!(
                answer, case.expect,
                "{}::{}{:?} answered {answer:?}",
                case.fixture, case.method, case.args
            );
        }
    }
}

#[test]
fn the_default_build_and_the_stellar_build_agree_on_every_fixture() {
    for (fixture, cases) in by_fixture() {
        let source = fixture_source(fixture);
        let under_host = stellar_answers(&source, &cases);
        let under_wasmtime = default_target_answers(&source, &cases);
        for ((case, host_answer), core_answer) in
            cases.iter().zip(under_host).zip(under_wasmtime)
        {
            assert_eq!(
                host_answer, core_answer,
                "{}::{}{:?}: the Soroban host answered {host_answer:?} and the default build \
                 answered {core_answer:?}; the wrapper is doing more than marshalling",
                case.fixture, case.method, case.args
            );
        }
    }
}

/// A fixture nobody invokes measures nothing. The directory and the case table
/// are the two halves of this tier, and a file added to one without the other
/// is the way a tier quietly stops covering what its directory listing claims.
#[test]
fn every_fixture_file_is_invoked_by_a_case() {
    let dir = test_data_path().join("stellar");
    let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "inf"))
        .map(|path| path.file_stem().expect("a named file").to_string_lossy().into_owned())
        .collect();
    on_disk.sort();

    let mut invoked: Vec<String> =
        by_fixture().into_iter().map(|(name, _)| name.to_owned()).collect();
    invoked.sort();

    assert_eq!(on_disk, invoked, "the fixture directory and the case table disagree");
}

/// The same guard one level down. A method added to a fixture that already has
/// a case is wrapped, exported and invoked by nothing, and the file-level guard
/// above sees a directory that still matches. This one reads the exported names
/// off the compiled artifact and off the descriptor the rewriter is driven by,
/// and requires both to be exactly the methods the table calls.
#[test]
fn every_exported_method_is_invoked_by_a_case() {
    for (fixture, cases) in by_fixture() {
        let source = fixture_source(fixture);

        let output = codegen_for_target(&source, Target::Stellar)
            .unwrap_or_else(|e| panic!("{fixture} must compile for the Stellar target: {e}"));
        let mut described: Vec<&str> =
            output.export_signatures().iter().map(|signature| signature.name.as_str()).collect();
        described.sort_unstable();

        let contract = compile_for_stellar(&source);
        let shape = Shape::of(&contract);
        let exported = shape.exported_functions();

        let mut invoked: Vec<&str> = cases.iter().map(|case| case.method).collect();
        invoked.sort_unstable();
        invoked.dedup();

        assert_eq!(
            exported, invoked,
            "{fixture} exports {exported:?} and the case table invokes {invoked:?}"
        );
        assert_eq!(
            described, invoked,
            "{fixture}'s export descriptor names {described:?} and the case table invokes \
             {invoked:?}"
        );
    }
}

/// `MEASURED_ABI.md` states how much this tier covers, and prose does not go
/// red on its own. These are the two numbers it states.
#[test]
fn the_measured_record_states_the_counts_this_table_holds() {
    assert_eq!(by_fixture().len(), 13, "MEASURED_ABI.md says thirteen fixtures");
    assert_eq!(CASES.len(), 23, "MEASURED_ABI.md says twenty-three invocations");
}

/// The one fixture whose body needs a frame, and a scalar-only fixture beside
/// it as the control.
///
/// A contract carrying linear memory is the shape the compiler produces for any
/// program holding a compound value, and it is the shape the export retarget
/// has to carry non-function entries through. Every other fixture here is
/// scalar all the way down and its export section holds nothing but functions.
#[test]
fn the_memory_fixture_deploys_with_linear_memory_and_its_two_exports() {
    let with_memory = Shape::of(&compile_for_stellar(&fixture_source("array_local")));

    assert_eq!(with_memory.memories.len(), 1, "a contract declares at most one memory");
    let (minimum, maximum) = with_memory.memories[0];
    assert!(minimum >= 1, "the declared memory holds no pages");
    assert_eq!(
        maximum,
        Some(minimum),
        "the emitter declares a fixed memory, minimum equal to maximum"
    );
    assert!(
        with_memory.has_export("memory", ExternalKind::Memory),
        "the contract does not export its memory: {:?}",
        with_memory.exports
    );
    assert!(
        with_memory.has_export("__stack_pointer", ExternalKind::Global),
        "the contract does not export its stack pointer: {:?}",
        with_memory.exports
    );
    assert!(
        with_memory.has_export("pick", ExternalKind::Func),
        "the wrapped method is gone from the export section: {:?}",
        with_memory.exports
    );
    assert_eq!(
        with_memory.data_segments, 0,
        "the emitter emits no data segment; MEASURED_ABI.md records that, and this is what \
         would notice it starting to"
    );

    let scalar_only = Shape::of(&compile_for_stellar(&fixture_source("return_only")));
    assert!(
        scalar_only.memories.is_empty(),
        "memory is conditional on the program using a compound value, and this one does not"
    );
    assert!(
        scalar_only.exports.iter().all(|(_, kind)| *kind == ExternalKind::Func),
        "a scalar-only contract exports functions and nothing else: {:?}",
        scalar_only.exports
    );
}

// ---------------------------------------------------------------------------
// What a caller cannot do
//
// Each negative is paired with the control that differs from it in exactly the
// property being measured, because the host is the code under test here and
// cannot be neutralized: a refusal with no control beside it could as easily be
// the fixture being wrong as the rule being real.
// ---------------------------------------------------------------------------

#[test]
fn an_argument_with_the_wrong_tag_traps() {
    let wasm = compile_for_stellar(&fixture_source("u32_methods"));
    let _guard = session();
    let host = host();
    let contract = upload(&host, &wasm).expect("the compiled contract uploads");

    let control = call(&host, contract, "identity", &[u32_val(7)])
        .expect("the same method accepts a correctly tagged argument");
    assert_eq!(Answer::from(decode(control)), Answer::U32(7));

    // A well-formed `Val` of the wrong type, and the one every uninitialized
    // slot decodes to: the two ways a caller gets the tag wrong.
    for arg in [i32_val(7), void_val()] {
        let err = call(&host, contract, "identity", &[arg])
            .expect_err("a wrongly tagged argument must not be accepted");
        let rendered = describe(&err);
        assert!(
            rendered.contains("Error(WasmVm, InvalidAction)")
                && rendered.contains("VM call trapped: UnreachableCodeReached"),
            "expected the unwrap guard's trap, got: {rendered}"
        );
    }
}

#[test]
fn a_call_with_the_wrong_number_of_arguments_is_refused() {
    let wasm = compile_for_stellar(&fixture_source("u32_methods"));
    let _guard = session();
    let host = host();
    let contract = upload(&host, &wasm).expect("the compiled contract uploads");

    let control = call(&host, contract, "add", &[u32_val(2), u32_val(40)])
        .expect("the same method accepts the arity it declares");
    assert_eq!(Answer::from(decode(control)), Answer::U32(42));

    for args in [vec![u32_val(2)], vec![u32_val(2), u32_val(40), u32_val(1)]] {
        let err = call(&host, contract, "add", &args)
            .expect_err("a call of the wrong arity must not be accepted");
        let rendered = describe(&err);
        assert!(
            rendered.contains("Error(WasmVm, UnexpectedSize)")
                && rendered.contains("VM call failed: Func(MismatchingParameterLen)"),
            "expected the arity refusal, got: {rendered}"
        );
    }
}

// ---------------------------------------------------------------------------
// The boolean unwrap's laxity, and why it is unreachable
//
// The unwrap masks the low byte and refuses only a value above one, so a word
// whose tag is `True` but whose body is not empty passes the guard and yields
// `true`. The wrap side is strict about exactly that shape — `wrappers`
// measures the host refusing a returned `0x101` — so the two halves of the ABI
// disagree about it, and the reference SDK traps where we accept.
//
// The sequence is what was measured against the host and it is not changed for
// this. What the two tests below settle is whether the laxity is reachable:
// the first shows the host refusing such a word before any contract runs, the
// second shows the wrapper accepting it when the host is taken out of the way.
// Together they make the asymmetry a recorded choice rather than an accident.
// ---------------------------------------------------------------------------

/// A `True` (tag 1) whose body is not empty: bit 8 is set.
const BOOLEAN_WITH_A_DIRTY_BODY: u64 = 0x101;

#[test]
fn a_boolean_argument_with_a_nonzero_body_is_refused_on_both_routes_into_a_contract() {
    use soroban_env_host::{Env, EnvBase};

    let wasm = compile_for_stellar(&fixture_source("bool_round_trip"));
    let _guard = session();
    let host = host();
    let contract = upload(&host, &wasm).expect("the compiled contract uploads");

    let before_control = fn_call_events(&host);
    let control = call(&host, contract, "negate", &[bool_val(true)])
        .expect("the same method accepts a clean boolean");
    assert_eq!(Answer::from(decode(control)), Answer::Bool(false));
    let after_control = fn_call_events(&host);
    assert_eq!(
        after_control,
        before_control + 1,
        "an invocation that reaches the contract records one `fn_call` event"
    );

    let dirty = Val::from_payload(BOOLEAN_WITH_A_DIRTY_BODY);
    let err = call(&host, contract, "negate", &[dirty])
        .expect_err("a boolean with a nonzero body must not be accepted");
    let rendered = describe(&err);
    assert!(
        rendered.contains("Error(Value, InvalidInput)"),
        "expected the host's own argument refusal, got: {rendered}"
    );
    assert_eq!(
        fn_call_events(&host),
        after_control,
        "the refused call recorded an invocation, so it did reach the contract"
    );

    // Attribution: the refusal is the argument *vector* constructor, not the
    // contract and not dispatch. The clean word passing the same constructor is
    // what makes that a statement about the body bits.
    host.vec_new_from_slice(&[bool_val(true)]).expect("a clean boolean is a valid argument");
    let err = host
        .vec_new_from_slice(&[dirty])
        .expect_err("a boolean with a nonzero body is not a valid argument");
    assert!(
        describe(&err).contains("Error(Value, InvalidInput)"),
        "expected the argument-vector refusal, got: {}",
        describe(&err)
    );

    // The other way to build an argument vector, and the reason one route is
    // not an argument about the surface: `vec_new_from_slice` runs the
    // integrity check itself, so a test using only it measures that check
    // rather than what a contract is reachable with. `vec_push_back` makes no
    // such call — and the word is refused anyway, one layer out, because the
    // blanket `impl Env` every host function is reached through checks each
    // `Val` argument before it reaches the implementation. So the vector cannot
    // be built, and there is no third call to make.
    let empty = host.vec_new().expect("an empty argument vector");
    let err = host
        .vec_push_back(empty, dirty)
        .expect_err("pushing the word onto an argument vector must not be accepted");
    assert!(
        describe(&err).contains("Error(Value, InvalidInput)"),
        "expected the host-function argument refusal, got: {}",
        describe(&err)
    );
    assert_eq!(
        fn_call_events(&host),
        after_control,
        "no invocation reached a contract after the control call"
    );
}

#[test]
fn the_boolean_unwrap_accepts_a_nonzero_body_when_the_host_is_not_in_the_way() {
    let wasm = compile_for_stellar(&fixture_source("bool_round_trip"));
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, &wasm).expect("the contract is a valid module");
    let mut store = wasmtime::Store::new(&engine, ());
    let instance =
        wasmtime::Instance::new(&mut store, &module, &[]).expect("the contract instantiates");
    let negate: wasmtime::TypedFunc<i64, i64> = instance
        .get_typed_func(&mut store, "negate")
        .expect("the contract exports the wrapped method");

    let returned = negate
        .call(&mut store, BOOLEAN_WITH_A_DIRTY_BODY.cast_signed())
        .expect("the guard passes a tag of one whatever the body holds");
    assert_eq!(
        decode(Val::from_payload(returned.cast_unsigned())),
        Decoded::Bool(false),
        "the wrapper read the dirty word as `true`, so `negate` answered `false`"
    );

    // The guard is live: it is the tag it checks, and only the tag.
    let err = negate
        .call(&mut store, 2)
        .expect_err("a tag above one must still trap");
    let trap = err
        .downcast_ref::<wasmtime::Trap>()
        .unwrap_or_else(|| panic!("expected a wasmtime Trap, got: {err:?}"));
    assert_eq!(*trap, wasmtime::Trap::UnreachableCodeReached);
}
