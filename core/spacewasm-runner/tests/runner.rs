//! The runner against the real interpreter: every way a load, a start and a
//! call can end, each as the value the runner promises rather than a panic.
//!
//! The modules are hand-written, because most of what is pinned here — a
//! start function, a re-exported import, `memory.grow` — is a shape the
//! compiler never emits. What the compiler's own output does under the runner
//! is the SpaceWasm tier's to pin.

use std::cell::Cell;
use std::num::NonZeroUsize;
use std::ops::ControlFlow;
use std::rc::Rc;

use inference_spacewasm_runner::{
    ArgumentError, EngineConfig, ExportKind, ExportedFunction, Fuel, HostSet, HostSetError,
    Instance, InvokeError, LoadError, LoadedModule, Outcome, REFERENCE_MAX_CODE_PAGES,
    REFERENCE_MAX_CONTROL_FRAMES, REFERENCE_MAX_STACK_DEPTH, ReexportedImport, Session,
    StartError, TrapReason, ValType, Value, coerce_arguments, host_module, host_set, ir_stats,
    load, load_with,
};
use spacewasm::{
    HostFunction, HostFunctionBreak, HostFunctionResult, HostName, HostNameError, HostValList,
    MemoryError, ValidationError,
};

/// A roomy engine, so a row trips only the limit it is about.
const ENGINE: EngineConfig =
    EngineConfig { stack_words: 64 * 1024, max_code_pages: REFERENCE_MAX_CODE_PAGES };

/// A budget no row that is meant to finish comes near.
const FUEL: usize = 1_000_000;

/// [`FUEL`] as the budget a module is started under.
const BUDGET: Fuel = Fuel::Limited(NonZeroUsize::new(FUEL).expect("the budget is not zero"));

/// The reference control-frame bound, as the const generic `load_with` takes.
const FRAMES: usize = REFERENCE_MAX_CONTROL_FRAMES as usize;

/// The reference operand-stack bound, as the const generic `load_with` takes.
const DEPTH: usize = REFERENCE_MAX_STACK_DEPTH as usize;

/// Sixteen-bit words one IR page holds, which upstream's IR format fixes.
const WORDS_PER_PAGE: usize = 256;

fn wat(text: &str) -> Vec<u8> {
    wat::parse_str(text).unwrap_or_else(|e| panic!("the fixture is not valid WAT: {e}\n{text}"))
}

/// `wasm` loaded with no hosts, in the roomy engine, or a panic naming why not.
fn loaded<'s>(session: &'s mut Session, wasm: &[u8]) -> LoadedModule<'s> {
    load(session, wasm, ENGINE).unwrap_or_else(|e| panic!("the fixture loads: {e}"))
}

/// `wasm` loaded with no hosts, in the roomy engine, and started under
/// [`BUDGET`], or a panic naming why not.
fn started<'s>(session: &'s mut Session, wasm: &[u8]) -> Instance<'s> {
    loaded(session, wasm).start(BUDGET).unwrap_or_else(|e| panic!("the fixture starts: {e}"))
}

/// Why `wasm` did not load with no hosts, in `config`.
fn load_error(wasm: &[u8], config: EngineConfig) -> LoadError {
    let mut session = Session::acquire();
    match load(&mut session, wasm, config) {
        Ok(_) => panic!("the fixture must not load"),
        Err(error) => error,
    }
}

/// Why `wasm`, which loads with no hosts in `config`, did not start under
/// `fuel`.
fn start_error(wasm: &[u8], config: EngineConfig, fuel: Fuel) -> StartError {
    let mut session = Session::acquire();
    let module = load(&mut session, wasm, config)
        .unwrap_or_else(|e| panic!("the fixture loads, since a load runs nothing: {e}"));
    match module.start(fuel) {
        Ok(_) => panic!("the fixture must not start"),
        Err(error) => error,
    }
}

/// The decoder's own verdict inside a [`LoadError::Decode`].
fn decode_verdict(error: &LoadError) -> ValidationError {
    match error {
        LoadError::Decode(verdict) => verdict.err.err.clone(),
        other => panic!("expected a decode refusal, got {other}"),
    }
}

/// A host module `env` holding one function `f` of no parameters and no
/// result that answers `answer`.
fn env_f(session: &Session, answer: HostFunctionBreak) -> HostSet {
    let function = HostFunction::try_new(
        HostName::try_from_str("f").expect("a one-byte name"),
        HostValList::new(""),
        HostValList::new(""),
        move |_: &mut spacewasm::Engine, _: &[Value]| ControlFlow::Break(answer),
    )
    .expect("the host function is registrable");
    let env = host_module(session, "env", vec![function], Vec::new()).expect("`env` registers");
    host_set(session, vec![env]).expect("the host set allocates")
}

const ARITHMETIC: &str = r#"(module
  (func (export "add") (param i32 i32) (result i32) local.get 0 local.get 1 i32.add)
  (func (export "wide") (param i64) (result i64) local.get 0 i64.const 1 i64.add)
  (func (export "nothing"))
  (func (export "boom") unreachable)
  (func (export "divide") (param i32 i32) (result i32) local.get 0 local.get 1 i32.div_s)
  (func (export "spin") (loop br 0))
  (memory (export "mem") 1))"#;

/// A call returns its value, at its own width, and a function with no result
/// returns nothing.
///
/// Fails if a result is read at the wrong width, or if a unit function starts
/// returning the engine's placeholder word.
#[test]
fn a_call_returns_its_result() {
    let wasm = wat(ARITHMETIC);
    let mut session = Session::acquire();
    let mut instance = started(&mut session, &wasm);
    assert_eq!(
        instance.invoke("add", &[Value::I32(2), Value::I32(40)], FUEL),
        Ok(Outcome::Returned(Some(Value::I32(42))))
    );
    assert_eq!(
        instance.invoke("wide", &[Value::I64(i64::from(u32::MAX))], FUEL),
        Ok(Outcome::Returned(Some(Value::I64(i64::from(u32::MAX) + 1))))
    );
    assert_eq!(instance.invoke("nothing", &[], FUEL), Ok(Outcome::Returned(None)));
}

/// The exports are listed with their WebAssembly signatures, in export-section
/// order, and a memory export is left out.
#[test]
fn the_exported_functions_are_listed_with_their_signatures() {
    let wasm = wat(ARITHMETIC);
    let mut session = Session::acquire();
    let module = loaded(&mut session, &wasm);
    let function = |name: &str, params: &[ValType], result: Option<ValType>| ExportedFunction {
        name: name.to_string(),
        params: params.to_vec(),
        result,
    };
    assert_eq!(
        module.exported_functions(),
        vec![
            function("add", &[ValType::I32, ValType::I32], Some(ValType::I32)),
            function("wide", &[ValType::I64], Some(ValType::I64)),
            function("nothing", &[], None),
            function("boom", &[], None),
            function("divide", &[ValType::I32, ValType::I32], Some(ValType::I32)),
            function("spin", &[], None),
        ]
    );
    assert_eq!(module.exported_host_imports(), Vec::new());
}

/// A trap is an outcome carrying the interpreter's reason, and the module can
/// be called again after it.
///
/// Fails if a trap is reported as an error, or if its reason is dropped.
#[test]
fn a_trap_is_an_outcome_with_its_reason() {
    let wasm = wat(ARITHMETIC);
    let mut session = Session::acquire();
    let mut instance = started(&mut session, &wasm);
    assert_eq!(instance.invoke("boom", &[], FUEL), Ok(Outcome::Trapped(TrapReason::Unreachable)));
    assert_eq!(
        instance.invoke("divide", &[Value::I32(1), Value::I32(0)], FUEL),
        Ok(Outcome::Trapped(TrapReason::DivideByZero))
    );
    assert_eq!(
        instance.invoke("add", &[Value::I32(1), Value::I32(1)], FUEL),
        Ok(Outcome::Returned(Some(Value::I32(2))))
    );
}

/// A call still running when its budget runs out ends as `OutOfFuel`, and the
/// engine is idle again afterwards.
///
/// Fails if the abandoned call is left running, which would refuse the next
/// call as `Busy`.
#[test]
fn an_exhausted_budget_is_an_outcome_and_leaves_the_engine_idle() {
    let wasm = wat(ARITHMETIC);
    let mut session = Session::acquire();
    let mut instance = started(&mut session, &wasm);
    assert_eq!(instance.invoke("spin", &[], 1_000), Ok(Outcome::OutOfFuel { budget: 1_000 }));
    assert_eq!(
        instance.invoke("add", &[Value::I32(20), Value::I32(22)], FUEL),
        Ok(Outcome::Returned(Some(Value::I32(42))))
    );
}

/// A call whose frame does not fit the value stack is a `StackOverflow` trap,
/// as wasmtime reports its own, and not an error.
///
/// Fails if the engine's refusal to begin the call starts surfacing as an
/// [`InvokeError`], or if `stack_words` stops reaching the engine.
#[test]
fn a_frame_that_does_not_fit_the_stack_is_a_stack_overflow_trap() {
    let wasm = wat(ARITHMETIC);
    let mut session = Session::acquire();
    let mut instance = load(&mut session, &wasm, EngineConfig { stack_words: 1, ..ENGINE })
        .expect("the module loads; only calling it needs stack")
        .start(BUDGET)
        .expect("the module has no start function to need stack");
    assert_eq!(
        instance.invoke("add", &[Value::I32(1), Value::I32(2)], FUEL),
        Ok(Outcome::Trapped(TrapReason::StackOverflow))
    );
}

/// Every way a call cannot be made is its own error, and none is a panic.
///
/// Fails if a name, a kind, a count or a type stops being told apart, or if a
/// message drops the name it is about.
#[test]
fn a_call_that_cannot_be_made_is_an_error_naming_why() {
    let wasm = wat(ARITHMETIC);
    let mut session = Session::acquire();
    let mut instance = started(&mut session, &wasm);

    let absent = instance.invoke("absent", &[], FUEL).expect_err("nothing is exported as `absent`");
    assert_eq!(
        absent,
        InvokeError::NoSuchExport {
            export: "absent".to_string(),
            exports: ["add", "wide", "nothing", "boom", "divide", "spin"]
                .map(str::to_string)
                .to_vec(),
        }
    );
    assert_eq!(
        absent.to_string(),
        "the module exports no function named `absent`; it exports `add`, `wide`, `nothing`, \
         `boom`, `divide`, `spin`"
    );

    let memory = instance.invoke("mem", &[], FUEL).expect_err("`mem` is a memory");
    assert_eq!(
        memory,
        InvokeError::NotAFunction { export: "mem".to_string(), kind: ExportKind::Memory }
    );

    let arity = instance.invoke("add", &[Value::I32(1)], FUEL).expect_err("one argument short");
    assert_eq!(arity, InvokeError::Arity { export: "add".to_string(), expected: 2, given: 1 });
    assert_eq!(arity.to_string(), "`add` takes 2 arguments; 1 argument given");

    let types = instance
        .invoke("add", &[Value::I32(1), Value::I64(2)], FUEL)
        .expect_err("the second argument is 64 bits wide");
    assert_eq!(
        types,
        InvokeError::Argument {
            export: "add".to_string(),
            expected: vec![ValType::I32, ValType::I32],
            given: vec![ValType::I32, ValType::I64],
        }
    );
    assert_eq!(types.to_string(), "`add` takes (i32, i32), and was called with (i32, i64)");

    assert_eq!(
        instance.invoke("add", &[Value::I32(1), Value::I32(2)], FUEL),
        Ok(Outcome::Returned(Some(Value::I32(3)))),
        "a refused call leaves the module callable"
    );
}

/// A module with no function exports says so where a missing name is refused.
#[test]
fn a_module_exporting_no_function_says_so() {
    let wasm = wat("(module (memory (export \"mem\") 1))");
    let mut session = Session::acquire();
    let mut instance = started(&mut session, &wasm);
    assert_eq!(
        instance.invoke("main", &[], FUEL).map_err(|e| e.to_string()),
        Err("the module exports no function named `main`; it exports no functions".to_string())
    );
}

/// A name exported as something other than a function is refused naming what
/// it is: the module's linear memory, its table or one of its globals.
///
/// Fails if a kind is dropped from the refusal or reported as another.
#[test]
fn a_name_exported_as_no_function_is_refused_naming_what_it_is() {
    let wasm = wat(
        r#"(module
             (memory (export "mem") 1)
             (table (export "tbl") 1 funcref)
             (global (export "g") i32 (i32.const 7)))"#,
    );
    let mut session = Session::acquire();
    let mut instance = started(&mut session, &wasm);
    for (export, kind, text) in [
        (
            "mem",
            ExportKind::Memory,
            "`mem` is exported, but it is the module's linear memory, not a function",
        ),
        (
            "tbl",
            ExportKind::Table,
            "`tbl` is exported, but it is the module's table, not a function",
        ),
        (
            "g",
            ExportKind::Global,
            "`g` is exported, but it is one of the module's globals, not a function",
        ),
    ] {
        let error = instance.invoke(export, &[], FUEL).expect_err("not a function");
        assert_eq!(error, InvokeError::NotAFunction { export: export.to_string(), kind });
        assert_eq!(error.to_string(), text);
    }
}

/// A host import the module exports again is listed apart and refused by
/// name, with the import behind it.
///
/// Fails if the re-export is invoked, listed as a function, or refused as a
/// missing export.
#[test]
fn a_reexported_host_import_is_refused_naming_the_import() {
    let wasm = wat(
        r#"(module
             (import "env" "f" (func $f))
             (export "again" (func $f))
             (func (export "call") call $f))"#,
    );
    let mut session = Session::acquire();
    let hosts = env_f(&session, HostFunctionBreak::Trap);
    let mut instance = load_with::<FRAMES, DEPTH>(&mut session, &wasm, hosts, ENGINE)
        .expect("the import is supplied")
        .start(BUDGET)
        .expect("the module has no start function");
    assert_eq!(
        instance.module().exported_host_imports(),
        vec![ReexportedImport { name: "again".to_string(), import: "env.f".to_string() }]
    );
    assert_eq!(
        instance.module().exported_functions().len(),
        1,
        "only `call` has a WebAssembly body"
    );
    let refusal = instance.invoke("again", &[], FUEL).expect_err("a host import has no body here");
    assert_eq!(
        refusal,
        InvokeError::HostReexport { export: "again".to_string(), import: "env.f".to_string() }
    );
    assert!(
        refusal.to_string().starts_with(
            "`again` resolves to the host import `env.f`: the interpreter invokes only functions \
             with a WebAssembly body"
        ),
        "{refusal}"
    );
    assert_eq!(
        instance.invoke("call", &[], FUEL),
        Ok(Outcome::Trapped(TrapReason::Host)),
        "a host that breaks with a trap traps the call"
    );
}

/// A name resolves to the function a call of it would run, with its
/// WebAssembly signature, and a name no call could run to the refusal the call
/// would give: nothing by that name, a memory, a table or a global by kind, and
/// a host import exported again by the import behind it. The module is only
/// read, so nothing runs.
///
/// Fails if a lookup reports another signature than the call would check,
/// refuses a name another way than the call does, or finds a function the call
/// would refuse.
#[test]
fn a_name_resolves_to_its_function_as_a_call_would_without_running_it() {
    let mut session = Session::acquire();
    {
        let wasm = wat(ARITHMETIC);
        let module = loaded(&mut session, &wasm);
        assert_eq!(
            module.function("add"),
            Ok(ExportedFunction {
                name: "add".to_string(),
                params: vec![ValType::I32, ValType::I32],
                result: Some(ValType::I32),
            })
        );
        assert_eq!(
            module.function("wide").map(|function| (function.params, function.result)),
            Ok((vec![ValType::I64], Some(ValType::I64)))
        );
        assert_eq!(
            module.function("nothing").map(|function| (function.params, function.result)),
            Ok((Vec::new(), None))
        );
        assert_eq!(
            module.function("absent"),
            Err(InvokeError::NoSuchExport {
                export: "absent".to_string(),
                exports: ["add", "wide", "nothing", "boom", "divide", "spin"]
                    .map(str::to_string)
                    .to_vec(),
            })
        );
    }
    {
        let wasm = wat(
            r#"(module
                 (memory (export "mem") 1)
                 (table (export "tbl") 1 funcref)
                 (global (export "g") i32 (i32.const 7)))"#,
        );
        let module = loaded(&mut session, &wasm);
        for (export, kind) in
            [("mem", ExportKind::Memory), ("tbl", ExportKind::Table), ("g", ExportKind::Global)]
        {
            assert_eq!(
                module.function(export),
                Err(InvokeError::NotAFunction { export: export.to_string(), kind })
            );
        }
    }
    let wasm = wat(
        r#"(module
             (import "env" "f" (func $f))
             (export "again" (func $f))
             (func (export "call") call $f))"#,
    );
    let hosts = env_f(&session, HostFunctionBreak::Trap);
    let module = load_with::<FRAMES, DEPTH>(&mut session, &wasm, hosts, ENGINE)
        .expect("the import is supplied");
    assert_eq!(
        module.function("again"),
        Err(InvokeError::HostReexport { export: "again".to_string(), import: "env.f".to_string() })
    );
    assert_eq!(
        module.function("call").map(|function| (function.params, function.result)),
        Ok((Vec::new(), None))
    );
}

/// A host that pauses a call is an error, since the runner never resumes one,
/// and the engine is idle again afterwards.
///
/// Fails if a pause is reported as an outcome, or if the paused call is left
/// holding the engine.
#[test]
fn a_paused_call_is_an_error_and_leaves_the_engine_idle() {
    let wasm = wat(
        r#"(module
             (import "env" "f" (func $f))
             (func (export "call") call $f)
             (func (export "seven") (result i32) i32.const 7))"#,
    );
    let mut session = Session::acquire();
    let hosts = env_f(&session, HostFunctionBreak::Pause);
    let mut instance = load_with::<FRAMES, DEPTH>(&mut session, &wasm, hosts, ENGINE)
        .expect("the import is supplied")
        .start(BUDGET)
        .expect("the module has no start function");
    assert_eq!(
        instance.invoke("call", &[], FUEL),
        Err(InvokeError::Paused { export: "call".to_string() })
    );
    assert_eq!(instance.invoke("seven", &[], FUEL), Ok(Outcome::Returned(Some(Value::I32(7)))));
}

/// Bytes the interpreter refuses are a decode error carrying its verdict.
#[test]
fn refused_bytes_are_a_decode_error_with_the_verdict() {
    let error = load_error(b"not wasm", ENGINE);
    assert_eq!(decode_verdict(&error), ValidationError::MalformedMagic);
    assert!(
        error.to_string().starts_with(
            "the module does not load under the SpaceWasm interpreter: MalformedMagic at byte "
        ),
        "{error}"
    );
}

/// An import nothing supplies is refused while the module decodes.
#[test]
fn an_unsupplied_import_is_a_decode_error() {
    let wasm = wat(r#"(module (import "env" "f" (func)))"#);
    assert_eq!(
        decode_verdict(&load_error(&wasm, ENGINE)),
        ValidationError::FunctionImportNotFound
    );
}

/// The code builder refuses `memory.grow`, as `spacewasm_std`'s does.
///
/// Fails if a load starts compiling with `allow_memory_grow` on.
#[test]
fn memory_grow_is_refused() {
    let wasm = wat(
        r#"(module (memory 1)
             (func (export "grow") (result i32) i32.const 1 memory.grow))"#,
    );
    assert_eq!(decode_verdict(&load_error(&wasm, ENGINE)), ValidationError::IllegalMemoryGrow);
}

/// A body nested `depth` blocks deep inside its own frame, so it needs
/// `depth + 1` control frames.
fn nested(depth: usize) -> Vec<u8> {
    let body = format!("{}{}", "(block ".repeat(depth), ")".repeat(depth));
    wat(&format!("(module (func (export \"f\") {body}))"))
}

/// [`load`] holds a module to the reference embedder's control-frame bound,
/// and [`load_with`] to the one it is given.
///
/// Fails if `load` stops passing the reference bound through, which would let
/// the deeper module load, or if `load_with`'s const generic stops reaching
/// the decoder.
#[test]
fn load_uses_the_reference_bounds_and_load_with_its_own() {
    let at_the_bound = nested(FRAMES - 1);
    let past_the_bound = nested(FRAMES);
    let mut session = Session::acquire();
    load(&mut session, &at_the_bound, ENGINE)
        .expect("the reference bound's own number of frames fits it");
    drop(session);
    assert_eq!(
        decode_verdict(&load_error(&past_the_bound, ENGINE)),
        ValidationError::AllocError(spacewasm::AllocError::OutOfMemory)
    );
    let mut session = Session::acquire();
    load_with::<{ FRAMES + 1 }, DEPTH>(
        &mut session,
        &past_the_bound,
        spacewasm::Vec::zero(),
        ENGINE,
    )
    .expect("one frame past the reference bound fits a bound one frame larger");
}

/// A body whose operand stack peaks at `peak` values: `peak` constants pushed,
/// then as many dropped.
fn operand_peak(peak: usize) -> Vec<u8> {
    let body = format!("{}{}", "i32.const 0 ".repeat(peak), "drop ".repeat(peak));
    wat(&format!("(module (func (export \"f\") {body}))"))
}

/// [`load`] holds a module to the reference embedder's operand-stack bound,
/// and [`load_with`] to the one it is given.
///
/// Fails if `load` stops passing the reference depth through, which would let
/// the taller module load, or if `load_with`'s second const generic stops
/// reaching the decoder.
#[test]
fn load_uses_the_reference_operand_stack_bound_and_load_with_its_own() {
    let at_the_bound = operand_peak(DEPTH);
    let past_the_bound = operand_peak(DEPTH + 1);
    let mut session = Session::acquire();
    let mut instance = load(&mut session, &at_the_bound, ENGINE)
        .expect("a peak of the reference bound's own height fits it")
        .start(BUDGET)
        .expect("the module has no start function");
    assert_eq!(instance.invoke("f", &[], FUEL), Ok(Outcome::Returned(None)));
    drop(instance);
    drop(session);
    assert_eq!(
        decode_verdict(&load_error(&past_the_bound, ENGINE)),
        ValidationError::AllocError(spacewasm::AllocError::OutOfMemory)
    );
    let mut session = Session::acquire();
    load_with::<FRAMES, { DEPTH + 1 }>(
        &mut session,
        &past_the_bound,
        spacewasm::Vec::zero(),
        ENGINE,
    )
    .expect("one value past the reference bound fits a bound one value taller");
}

/// A code-page budget too small for the module is the decoder's refusal, and
/// one past what the code builder addresses is a resource error.
///
/// Fails if `max_code_pages` stops reaching the code builder.
#[test]
fn the_code_page_budget_reaches_the_code_builder() {
    let wasm = wat(ARITHMETIC);
    assert_eq!(
        decode_verdict(&load_error(&wasm, EngineConfig { max_code_pages: 0, ..ENGINE })),
        ValidationError::AllocError(spacewasm::AllocError::OutOfMemory)
    );
    let error = load_error(&wasm, EngineConfig { max_code_pages: 1 << 24, ..ENGINE });
    assert!(
        matches!(
            error,
            LoadError::Resource { part: "its IR code pages", error: MemoryError::AllocationFailed }
        ),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "the SpaceWasm interpreter could not allocate its IR code pages: AllocationFailed"
    );
}

/// A start function runs once, when the module is started, before any call.
///
/// The start function adds to what it finds, so a second run would read 14.
/// Fails if the start function is skipped, run by the load as well as by the
/// start, or run again by the first call.
#[test]
fn a_start_function_runs_once_when_the_module_is_started() {
    let wasm = wat(
        r#"(module
             (global $g (mut i32) (i32.const 0))
             (func $start global.get $g i32.const 7 i32.add global.set $g)
             (start $start)
             (func (export "get") (result i32) global.get $g))"#,
    );
    let mut session = Session::acquire();
    let mut instance = started(&mut session, &wasm);
    assert_eq!(instance.invoke("get", &[], FUEL), Ok(Outcome::Returned(Some(Value::I32(7)))));
    assert_eq!(instance.invoke("get", &[], FUEL), Ok(Outcome::Returned(Some(Value::I32(7)))));
}

/// A host set whose one function, `env.f`, counts its calls and answers
/// `answer`.
fn counting_env_f(session: &Session, answer: HostFunctionResult) -> (HostSet, Rc<Cell<usize>>) {
    let calls = Rc::new(Cell::new(0));
    let counted = Rc::clone(&calls);
    let function = HostFunction::try_new(
        HostName::try_from_str("f").expect("a one-byte name"),
        HostValList::new(""),
        HostValList::new(""),
        move |_: &mut spacewasm::Engine, _: &[Value]| {
            counted.set(counted.get() + 1);
            answer
        },
    )
    .expect("the host function is registrable");
    let env = host_module(session, "env", vec![function], Vec::new()).expect("`env` registers");
    (host_set(session, vec![env]).expect("the host set allocates"), calls)
}

/// A module whose start function calls the host `env.f`, as does its export
/// `go`, beside an `add` of two parameters.
const CALLS_THE_HOST_ON_START: &str = r#"(module
  (import "env" "f" (func $f))
  (func $s call $f)
  (start $s)
  (func (export "go") call $f)
  (func (export "add") (param i32 i32) (result i32) local.get 0 local.get 1 i32.add))"#;

/// A load runs nothing: a caller can list the exports, resolve one that does
/// not exist, read arguments its function refuses and measure the IR, and no
/// host has been called — the start function's call comes with the start, and
/// the export's after it, in that order.
///
/// Fails if a load runs the start function, which would call the host before
/// any of the refusals a caller makes on the loaded module, or if the start or
/// the call stops reaching it. Fails too if the instance reads the module
/// differently than the loaded module did.
#[test]
fn nothing_runs_until_the_module_is_started() {
    let wasm = wat(CALLS_THE_HOST_ON_START);
    let mut session = Session::acquire();
    let (hosts, calls) = counting_env_f(&session, ControlFlow::Continue(None));
    let module = load_with::<FRAMES, DEPTH>(&mut session, &wasm, hosts, ENGINE)
        .expect("the import is supplied");
    assert_eq!(calls.get(), 0, "the load called the host");

    assert_eq!(
        module.function("missing"),
        Err(InvokeError::NoSuchExport {
            export: "missing".to_string(),
            exports: vec!["go".to_string(), "add".to_string()],
        })
    );
    let add = module.function("add").expect("`add` is exported");
    assert!(
        matches!(
            coerce_arguments(&add, &["1".to_string()]),
            Err(ArgumentError::Count { given, .. }) if given == ["1"]
        ),
        "one argument for two parameters is refused"
    );
    assert!(
        matches!(
            coerce_arguments(&add, &["x".to_string(), "1".to_string()]),
            Err(ArgumentError::NotAnInteger { position: 1, .. })
        ),
        "an argument that is no number is refused"
    );
    let exported = module.exported_functions();
    let stats = ir_stats(&module, wasm.len());
    assert_eq!(calls.get(), 0, "reading the loaded module called the host");

    let mut instance = module.start(BUDGET).expect("the start function returns");
    assert_eq!(calls.get(), 1, "the start function calls the host once");
    assert_eq!(instance.invoke("go", &[], FUEL), Ok(Outcome::Returned(None)));
    assert_eq!(calls.get(), 2, "the call calls the host after the start function did");

    assert_eq!(instance.module().exported_functions(), exported);
    assert_eq!(instance.module().function("add"), Ok(add));
    assert_eq!(ir_stats(instance.module(), wasm.len()), stats);
}

/// A module whose start function would trap still loads, with or without a
/// host set, and is read like any other; the trap comes only when it is
/// started, and the host that stops the start function is called then, once.
///
/// Fails if a load runs the start function: it would refuse both modules as
/// they load, or, if the load ran it and let the trap go, call the host before
/// the module is started, and a second time when it is.
#[test]
fn a_module_whose_start_function_traps_loads_and_fails_to_start() {
    let wasm = wat(CALLS_THE_HOST_ON_START);
    let mut session = Session::acquire();
    let (hosts, calls) = counting_env_f(&session, ControlFlow::Break(HostFunctionBreak::Trap));
    let module = load_with::<FRAMES, DEPTH>(&mut session, &wasm, hosts, ENGINE)
        .expect("the load runs nothing, so the trap is not met");
    assert_eq!(calls.get(), 0, "the load called the host");
    assert_eq!(
        module.function("go").map(|function| (function.params, function.result)),
        Ok((Vec::new(), None))
    );
    assert_eq!(module.start(BUDGET).err(), Some(StartError::Trapped(TrapReason::Host)));
    assert_eq!(calls.get(), 1, "the start function calls the host once, and it traps");

    let unreachable = wat(
        r#"(module
             (func $s unreachable)
             (start $s)
             (func (export "get") (result i32) i32.const 1))"#,
    );
    let module = load(&mut session, &unreachable, ENGINE).expect("the load runs nothing");
    assert_eq!(module.exported_functions().len(), 1);
    assert_eq!(module.start(BUDGET).err(), Some(StartError::Trapped(TrapReason::Unreachable)));
}

/// Every way a start function can fail to return is a start error of its own,
/// met when the module is started and never when it is loaded.
///
/// Fails if a start function's trap, exhausted budget or pause is reported as
/// another, as a load error, or as a panic.
#[test]
fn a_start_function_that_does_not_return_is_a_start_error() {
    let trapping = wat("(module (func $s unreachable) (start $s))");
    let error = start_error(&trapping, ENGINE, BUDGET);
    assert_eq!(error, StartError::Trapped(TrapReason::Unreachable));
    assert_eq!(error.to_string(), "the module's start function trapped: Unreachable");

    let spinning = wat("(module (func $s (loop br 0)) (start $s))");
    let error = start_error(&spinning, ENGINE, limited(1_000));
    assert_eq!(error, StartError::OutOfFuel { budget: 1_000 });
    assert_eq!(
        error.to_string(),
        "the module's start function was still running after 1000 instructions"
    );

    let mut session = Session::acquire();
    let hosts = env_f(&session, HostFunctionBreak::Pause);
    let pausing = wat(r#"(module (import "env" "f" (func $f)) (func $s call $f) (start $s))"#);
    let error = load_with::<FRAMES, DEPTH>(&mut session, &pausing, hosts, ENGINE)
        .expect("the load runs nothing, so the pause is not met")
        .start(BUDGET)
        .err()
        .expect("the host pauses it");
    assert_eq!(error, StartError::Paused);
    assert_eq!(
        error.to_string(),
        "the module's start function paused: a host function it called returned \
         `HostFunctionBreak::Pause`, and the runner never resumes a paused call"
    );
    drop(session);

    let crowded = start_error(
        &wat("(module (func $s (local i64 i64)) (start $s))"),
        EngineConfig { stack_words: 1, ..ENGINE },
        BUDGET,
    );
    assert_eq!(crowded, StartError::Trapped(TrapReason::StackOverflow));
}

/// Starting a module that declares no start function spends nothing, so the
/// call made within the budget is given all of it.
///
/// `f` takes five instructions. Fails if starting such a module spends any of
/// the budget, or refuses a budget too small for anything, or if the call is
/// given anything but the whole budget.
#[test]
fn a_module_without_a_start_function_leaves_the_call_the_whole_budget() {
    let wasm = wat(r#"(module (func (export "f") i32.const 1 i32.const 2 i32.add drop))"#);
    let mut session = Session::acquire();
    for (budget, ended) in [
        (5, Outcome::Returned(None)),
        (6, Outcome::Returned(None)),
        (4, Outcome::OutOfFuel { budget: 4 }),
        (1, Outcome::OutOfFuel { budget: 1 }),
    ] {
        let mut instance = loaded(&mut session, &wasm)
            .start(limited(budget))
            .unwrap_or_else(|e| panic!("nothing runs on start, so {budget} is enough: {e}"));
        assert_eq!(instance.invoke_within_budget("f", &[]), Ok(ended), "a budget of {budget}");
    }
}

/// A host module name longer than the interpreter holds is refused naming it.
#[test]
fn an_overlong_host_module_name_is_refused() {
    let session = Session::acquire();
    let name = "m".repeat(32);
    assert_eq!(
        host_module(&session, &name, Vec::new(), Vec::new()).map(|_| ()),
        Err(HostSetError::Name { name: name.clone(), error: HostNameError })
    );
    host_module(&session, &name[..31], Vec::new(), Vec::new()).expect("31 bytes fit");
}

/// `count` functions, each adding two constants, which the interpreter
/// compiles to four IR words apiece: 64 of them fill one page exactly, and a
/// 65th spills onto a second.
fn many_functions(count: usize) -> Vec<u8> {
    let function = "(func (result i32) i32.const 1 i32.const 2 i32.add)";
    wat(&format!("(module {})", function.repeat(count)))
}

/// The IR measurement is the pages held and the words written — every page
/// but the last counted full, plus the writer's offset into the last —
/// against the artifact's size.
///
/// The word counts are upstream's own IR, pinned exactly for the interpreter
/// release this workspace pins. Fails if the words written are read as the
/// pages' whole capacity, or if a page count, a byte count or the ratio stops
/// following from them.
#[test]
fn the_ir_measurement_is_consistent() {
    let mut session = Session::acquire();
    for (wasm, pages, words) in [
        (wat(ARITHMETIC), 1, 24),
        (many_functions(64), 1, 256),
        (many_functions(65), 2, 260),
        (many_functions(100), 2, 400),
    ] {
        let module = loaded(&mut session, &wasm);
        let stats = ir_stats(&module, wasm.len());
        assert_eq!((stats.code_pages, stats.ir_words), (pages, words), "{stats:?}");
        let into_the_last_page = stats.ir_words - WORDS_PER_PAGE * (stats.code_pages - 1);
        assert!((1..=WORDS_PER_PAGE).contains(&into_the_last_page), "{stats:?}");
        assert_eq!(stats.ir_words_capacity(), WORDS_PER_PAGE * pages);
        assert_eq!(stats.ir_bytes(), stats.ir_words * 2);
        let ratio = stats.ir_bytes_per_wasm_byte();
        assert!((ratio * wasm.len() as f64 - stats.ir_bytes() as f64).abs() < 1e-9, "{ratio}");
    }
}

/// The two refusals only an interpreter defect reaches still say what
/// happened.
///
/// No module provokes either, so each is built here rather than reached. Fails
/// if a text stops naming the function called or the interpreter's refusal.
#[test]
fn the_refusals_only_an_interpreter_defect_reaches_are_worded() {
    assert_eq!(
        StartError::Refused { refusal: "Busy".to_string() }.to_string(),
        "the SpaceWasm interpreter refused to invoke the module's start function: Busy"
    );
    assert_eq!(
        InvokeError::NoResult { export: "get".to_string() }.to_string(),
        "`get` declares a result, and the SpaceWasm interpreter finished the call without one"
    );
}

/// Each body, and the instructions the interpreter counts for a call to a
/// function made of it: its closing return included, a `nop` compiled to
/// nothing. Measured against the interpreter release this workspace pins.
const BODIES: [(&str, usize); 5] = [
    ("", 1),
    ("nop", 1),
    ("i32.const 1 drop", 3),
    ("i32.const 1 i32.const 2 i32.add drop", 5),
    ("(block (br 0))", 2),
];

/// `count` as a limited budget.
fn limited(count: usize) -> Fuel {
    Fuel::Limited(NonZeroUsize::new(count).expect("a budget of at least one instruction"))
}

/// A budget of exactly the instructions a call takes finishes it, and one
/// fewer runs out — for a call and for a start function alike.
///
/// The interpreter reports a call's return from inside the instruction that
/// executes it, so the budget needs no instruction to spare. The start
/// function is run one instruction at a time and the call in one run, and the
/// two must agree on every count. Fails if either route needs an instruction
/// more or fewer than the call takes, or if an exhausted budget reports
/// another number.
#[test]
fn a_budget_of_exactly_a_calls_instructions_finishes_it() {
    for (body, count) in BODIES {
        let callable = wat(&format!(r#"(module (func (export "f") {body}))"#));
        let mut session = Session::acquire();
        let mut instance = loaded(&mut session, &callable)
            .start(Fuel::Unbounded)
            .expect("a module without a start function starts");
        assert_eq!(instance.invoke("f", &[], count), Ok(Outcome::Returned(None)), "{body:?}");
        assert_eq!(
            instance.invoke("f", &[], count - 1),
            Ok(Outcome::OutOfFuel { budget: count - 1 }),
            "{body:?}"
        );
        drop(instance);

        let starting = wat(&format!("(module (func $s {body}) (start $s))"));
        loaded(&mut session, &starting)
            .start(limited(count))
            .unwrap_or_else(|e| panic!("{body:?} runs in {count}: {e}"));
        if count > 1 {
            let error = loaded(&mut session, &starting)
                .start(limited(count - 1))
                .err()
                .expect("one instruction short");
            assert_eq!(error, StartError::OutOfFuel { budget: count - 1 }, "{body:?}");
        }
    }
}

/// The start function and the call made within the start's budget share it:
/// the call is given exactly what the start function left, and running out
/// reports the whole budget.
///
/// The start function takes three instructions and `f` five. Fails if the
/// call is given the whole budget again, or less than the start function
/// left, or if an exhausted budget reports only the call's share.
#[test]
fn the_start_function_and_the_call_share_one_budget() {
    let wasm = wat(
        r#"(module
             (func $s i32.const 1 drop)
             (start $s)
             (func (export "f") i32.const 1 i32.const 2 i32.add drop))"#,
    );
    let mut session = Session::acquire();
    for (budget, ended) in [
        (8, Outcome::Returned(None)),
        (9, Outcome::Returned(None)),
        (7, Outcome::OutOfFuel { budget: 7 }),
        (3, Outcome::OutOfFuel { budget: 3 }),
    ] {
        let mut instance = loaded(&mut session, &wasm)
            .start(limited(budget))
            .unwrap_or_else(|e| panic!("the start function fits {budget}: {e}"));
        assert_eq!(instance.invoke_within_budget("f", &[]), Ok(ended), "a budget of {budget}");
    }
    let error = loaded(&mut session, &wasm).start(limited(2)).err().expect("the start needs 3");
    assert_eq!(error, StartError::OutOfFuel { budget: 2 });

    let mut instance = loaded(&mut session, &wasm).start(limited(3)).expect("the start fits");
    assert_eq!(
        instance.invoke("f", &[], 5),
        Ok(Outcome::Returned(None)),
        "a call given its own budget spends nothing of the start's"
    );
    let mut instance = loaded(&mut session, &wasm).start(Fuel::Unbounded).expect("no budget");
    assert_eq!(instance.invoke_within_budget("f", &[]), Ok(Outcome::Returned(None)));
}

/// A loop counting to a million, which takes several million instructions.
const COUNT_TO_A_MILLION: &str = r#"(module
  (global $counted (mut i32) (i32.const 0))
  (func $count (result i32) (local $i i32)
    (loop $again
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br_if $again (i32.lt_u (local.get $i) (i32.const 1000000))))
    (local.get $i))
  (func $start (global.set $counted (call $count)))
  (func (export "count") (result i32) (call $count))
  (func (export "counted") (result i32) (global.get $counted))
  (start $start))"#;

/// Without a budget, a start function and a call both run as long as they
/// take; with one, the same call runs out.
///
/// On a 64-bit host one run of the interpreter already allows more
/// instructions than any call takes, so what this pins is that no budget is
/// imposed where none was asked for. Fails if an unbounded run is cut short
/// anywhere, or if the limited control stops running out.
#[test]
fn an_unbounded_run_goes_on_until_the_call_returns() {
    let wasm = wat(COUNT_TO_A_MILLION);
    let mut session = Session::acquire();
    let mut instance = loaded(&mut session, &wasm)
        .start(Fuel::Unbounded)
        .expect("the start function counts to a million");
    assert_eq!(
        instance.invoke_within_budget("counted", &[]),
        Ok(Outcome::Returned(Some(Value::I32(1_000_000))))
    );
    assert_eq!(
        instance.invoke_within_budget("count", &[]),
        Ok(Outcome::Returned(Some(Value::I32(1_000_000))))
    );
    drop(instance);

    let error = loaded(&mut session, &wasm)
        .start(limited(1_000_000))
        .err()
        .expect("a million instructions do not count to a million");
    assert_eq!(error, StartError::OutOfFuel { budget: 1_000_000 });
}
